#[cfg(test)]
use crate::RenderResourcePolicy;
use crate::Result;
use crate::resources::OperationWorkMeter;
use std::sync::Arc;

// Fine-grained routing and post-processing counters are internal ticks. Keep their scale separate
// from the stable, family-level policy unit so adding an explicitly metered pass does not silently
// multiply every public resource profile.
const SWIMLANE_INTERNAL_TICKS_PER_WORK_UNIT: usize = 80;

pub(super) struct LayoutWorkBudget {
    meter: Arc<OperationWorkMeter>,
    pending_ticks: usize,
    ticks_per_work_unit: usize,
}

impl LayoutWorkBudget {
    pub(super) fn for_operation(meter: Arc<OperationWorkMeter>) -> Self {
        Self {
            meter,
            pending_ticks: 0,
            ticks_per_work_unit: SWIMLANE_INTERNAL_TICKS_PER_WORK_UNIT,
        }
    }

    #[cfg(test)]
    pub(super) fn new(policy: RenderResourcePolicy, initial: usize) -> Result<Self> {
        let meter = Arc::new(OperationWorkMeter::new(policy));
        meter.charge(initial)?;
        Ok(Self {
            meter,
            pending_ticks: 0,
            ticks_per_work_unit: 1,
        })
    }

    /// Checks a conservative internal estimate without reserving or consuming it.
    pub(super) fn preflight(&self, additional_ticks: usize) -> Result<()> {
        let ticks = self.pending_ticks.saturating_add(additional_ticks);
        let whole_units = ticks / self.ticks_per_work_unit;
        let work_units = whole_units
            .saturating_add(usize::from(!ticks.is_multiple_of(self.ticks_per_work_unit)));
        self.meter.preflight(work_units)?;
        Ok(())
    }

    pub(super) fn charge(&mut self, additional_ticks: usize) -> Result<()> {
        let ticks = self.pending_ticks.saturating_add(additional_ticks);
        let work_units = ticks / self.ticks_per_work_unit;
        if work_units != 0 {
            self.meter.charge(work_units)?;
        }
        self.pending_ticks = ticks % self.ticks_per_work_unit;
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        if self.pending_ticks != 0 {
            self.meter.charge(1)?;
            self.pending_ticks = 0;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn used(&self) -> usize {
        self.meter.used()
    }

    #[cfg(test)]
    pub(super) fn pending_ticks(&self) -> usize {
        self.pending_ticks
    }

    #[cfg(test)]
    pub(super) fn unbounded_for_tests() -> Self {
        Self::new(RenderResourcePolicy::unbounded_for_trusted_input(), 0)
            .expect("unbounded test policy accepts zero initial work")
    }
}

pub(super) fn unordered_pair_count(items: usize) -> usize {
    let (first, second) = if items.is_multiple_of(2) {
        (items / 2, items.saturating_sub(1))
    } else {
        (items, items.saturating_sub(1) / 2)
    };
    first.saturating_mul(second)
}

pub(super) fn sorting_work_units(items: usize) -> usize {
    if items <= 1 {
        return 0;
    }
    let passes = (usize::BITS - items.saturating_sub(1).leading_zeros()) as usize;
    items.saturating_mul(passes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};

    fn policy(max: usize) -> RenderResourcePolicy {
        RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, max)
            .unwrap()
    }

    #[test]
    fn exact_test_budget_accumulates_to_the_boundary() {
        let mut budget = LayoutWorkBudget::new(policy(10), 3).unwrap();
        budget.charge(4).unwrap();
        budget.charge(3).unwrap();
        assert_eq!(budget.used(), 10);
    }

    #[test]
    fn sorting_work_units_use_a_stable_ceiling_log_bound() {
        assert_eq!(sorting_work_units(0), 0);
        assert_eq!(sorting_work_units(1), 0);
        assert_eq!(sorting_work_units(2), 2);
        assert_eq!(sorting_work_units(3), 6);
        assert_eq!(sorting_work_units(4), 8);
        assert_eq!(sorting_work_units(5), 15);
    }

    #[test]
    fn rejected_charge_saturates_without_wrapping_or_reducing_usage() {
        let mut budget = LayoutWorkBudget::new(policy(10), 8).unwrap();
        let error = budget.charge(usize::MAX).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, usize::MAX);
        assert_eq!(error.max, 10);
        assert_eq!(budget.used(), 8);
    }

    #[test]
    fn internal_ticks_accumulate_before_rounding() {
        let meter = Arc::new(OperationWorkMeter::new(policy(2)));
        let mut budget = LayoutWorkBudget::for_operation(Arc::clone(&meter));

        budget.charge(79).unwrap();
        assert_eq!(meter.used(), 0);
        budget.charge(1).unwrap();
        assert_eq!(meter.used(), 1);
        budget.charge(79).unwrap();
        assert_eq!(meter.used(), 1);
        budget.finish().unwrap();
        assert_eq!(meter.used(), 2);
    }

    #[test]
    fn rejected_preflight_is_non_consuming_and_sticky() {
        let meter = Arc::new(OperationWorkMeter::new(policy(1)));
        let mut budget = LayoutWorkBudget::for_operation(Arc::clone(&meter));

        budget.charge(79).unwrap();
        budget.preflight(1).unwrap();
        let error = budget.preflight(2).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, 2);
        assert_eq!(error.max, 1);
        assert_eq!(meter.used(), 0);
        assert_eq!(budget.pending_ticks(), 79);

        let replayed = budget.charge(1).unwrap_err();
        let crate::Error::ResourceLimitExceeded(replayed) = replayed else {
            panic!("expected sticky max_layout_work_units resource limit error");
        };
        assert_eq!(replayed, error);
        assert_eq!(meter.used(), 0);
        assert_eq!(budget.pending_ticks(), 79);
    }

    #[test]
    fn rejected_tick_flush_preserves_meter_and_pending_remainder() {
        let meter = Arc::new(OperationWorkMeter::new(policy(1)));
        let mut budget = LayoutWorkBudget::for_operation(Arc::clone(&meter));

        budget.charge(80).unwrap();
        budget.charge(79).unwrap();
        let finish_error = budget.finish().unwrap_err();
        let crate::Error::ResourceLimitExceeded(finish_error) = finish_error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(finish_error.actual, 2);
        assert_eq!(finish_error.max, 1);
        assert_eq!(meter.used(), 1);
        assert_eq!(budget.pending_ticks(), 79);

        let charge_error = budget.charge(1).unwrap_err();
        let crate::Error::ResourceLimitExceeded(charge_error) = charge_error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(charge_error.actual, 2);
        assert_eq!(charge_error.max, 1);
        assert_eq!(meter.used(), 1);
        assert_eq!(budget.pending_ticks(), 79);
    }

    #[test]
    fn unordered_pair_count_saturates_without_dividing_the_saturated_value() {
        assert_eq!(unordered_pair_count(4), 6);
        assert_eq!(unordered_pair_count(usize::MAX), usize::MAX);
    }
}
