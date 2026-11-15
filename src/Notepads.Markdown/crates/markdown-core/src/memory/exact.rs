// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Lazy final allocations with a checked, immutable planned row count.

pub(super) mod build;

#[cfg(test)]
use super::{ChargedString, ChargedVec};
#[cfg(test)]
use crate::error::CoreError;
#[cfg(test)]
use crate::policy::{CHECKPOINT_BYTES, Operation, SourceBudget};

#[cfg(test)]
mod task;

#[cfg(test)]
pub(crate) struct ExactRows<T: Copy> {
    data: Option<ChargedVec<T>>,
    budget: SourceBudget,
    count: usize,
}

#[cfg(test)]
impl<T: Copy> ExactRows<T> {
    pub(crate) fn new(count: usize, op: &mut Operation) -> Result<Self, CoreError> {
        op.events(1)?;
        if size_of::<T>() == 0 || size_of::<T>() as u64 > CHECKPOINT_BYTES {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        count
            .checked_mul(size_of::<T>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        Ok(Self {
            data: None,
            budget: op.budget().clone(),
            count,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.data.as_ref().map_or(0, |data| data.as_slice().len())
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        self.data.as_ref().map_or(&[], ChargedVec::as_slice)
    }

    pub(crate) fn has_allocation(&self) -> bool {
        self.data.is_some()
    }

    pub(crate) fn push(&mut self, row: T, op: &mut Operation) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        if self.len() >= self.count {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        if self.data.is_none() {
            self.data = Some(ChargedVec::new(self.count, op)?);
        }
        op.copy(size_of::<T>() as u64)?;
        self.data
            .as_mut()
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?
            .push(row, op)
    }

    /// Only the owning unpublished transaction may change an already written row.
    pub(crate) fn replace(
        &mut self,
        index: usize,
        row: T,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        let slot = self
            .data
            .as_mut()
            .and_then(|data| data.values.get_mut(index))
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        op.copy(size_of::<T>() as u64)?;
        *slot = row;
        Ok(())
    }

    pub(crate) fn complete(&self, op: &mut Operation) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        if self.len() != self.count {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) struct ExactText {
    data: Option<ChargedString>,
    budget: SourceBudget,
    count: usize,
}

#[cfg(test)]
impl ExactText {
    pub(crate) fn new(count: usize, op: &mut Operation) -> Result<Self, CoreError> {
        op.events(1)?;
        Ok(Self {
            data: None,
            budget: op.budget().clone(),
            count,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.as_str().len()
    }

    pub(crate) fn as_str(&self) -> &str {
        self.data.as_ref().map_or("", ChargedString::as_str)
    }

    pub(crate) fn append(&mut self, text: &str, op: &mut Operation) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        let end = self
            .len()
            .checked_add(text.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        if end > self.count {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        if self.data.is_none() && !text.is_empty() {
            self.data = Some(ChargedString::auxiliary(self.count, op)?);
        }
        let mut start = 0;
        while start < text.len() {
            op.events(1)?;
            let mut end = text
                .len()
                .min(start.saturating_add(CHECKPOINT_BYTES as usize));
            while !text.is_char_boundary(end) {
                op.scan(1)?;
                end -= 1;
            }
            self.data
                .as_mut()
                .ok_or_else(|| op.fail(CoreError::InternalInvariant))?
                .append(&text[start..end], op)?;
            start = end;
        }
        Ok(())
    }

    pub(crate) fn complete(&self, op: &mut Operation) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        if self.len() != self.count {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{CancellationToken, SourceLimits};

    #[test]
    fn exact_lazy_allocations_reject_after_reserve_without_retaining_buffers() {
        let budget = SourceBudget::task_fixture();
        let mut preparation = budget.begin(CancellationToken::new());
        preparation.set_clock_samples_for_test(&[100_000_000]);
        let mut rows = ExactRows::<u64>::new(1, &mut preparation).unwrap();
        let mut text = ExactText::new(3, &mut preparation).unwrap();
        let deadline = CoreError::DeadlineExceeded {
            elapsed_ns: 500_000_000,
            limit_ns: 500_000_000,
        };

        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
        assert_eq!(rows.push(7, &mut op), Err(deadline));
        assert!(rows.as_slice().is_empty());
        assert_eq!(budget.usage().auxiliary_capacity, 0);

        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
        assert_eq!(text.append("abc", &mut op), Err(deadline));
        assert!(text.as_str().is_empty());
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn exact_capacity_is_lazy_and_never_grows_or_completes_missing_rows() {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = ExactRows::<u64>::new(3, &mut op).unwrap();
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        rows.push(1, &mut op).unwrap();
        assert_eq!(budget.usage().auxiliary_capacity, 24);
        rows.push(2, &mut op).unwrap();
        rows.push(3, &mut op).unwrap();
        rows.complete(&mut op).unwrap();
        assert_eq!(op.stats().expect("statistics enabled").copied_bytes(), 24);
        assert_eq!(rows.push(4, &mut op), Err(CoreError::InvalidProvenance));
        let mut later = budget.begin(CancellationToken::new());
        rows.replace(1, 9, &mut later).unwrap();
        assert_eq!(rows.as_slice(), &[1, 9, 3]);
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        let mut op = budget.begin(CancellationToken::new());
        let rows = ExactRows::<u64>::new(1, &mut op).unwrap();
        assert_eq!(rows.complete(&mut op), Err(CoreError::InvalidProvenance));
    }

    #[test]
    fn exact_allocation_failure_rolls_back_and_text_keeps_scalar_boundaries() {
        let budget = SourceBudget::new(SourceLimits {
            auxiliary_capacity: 16,
            ..SourceLimits::default()
        })
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = ExactRows::<u64>::new(3, &mut op).unwrap();
        assert!(matches!(
            rows.push(1, &mut op),
            Err(CoreError::ResourceLimit { .. })
        ));
        assert!(rows.as_slice().is_empty());
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let text = "中文😀".repeat(9000);
        let mut pool = ExactText::new(text.len(), &mut op).unwrap();
        pool.append(&text, &mut op).unwrap();
        pool.complete(&mut op).unwrap();
        assert_eq!(pool.as_str(), text);
        assert_eq!(
            op.stats().expect("statistics enabled").copied_bytes(),
            text.len() as u64
        );
        drop(pool);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
