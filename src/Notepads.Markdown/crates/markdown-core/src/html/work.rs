// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Projection borrows the original work authority, never its Operation.

use crate::error::CoreError;
use crate::policy::{Lease, Operation, OwnedWork, SourceBudget};

pub(crate) trait ProjectionWork {
    fn checkpoint(&mut self) -> Result<(), CoreError>;
    fn checkpoint_now(&mut self) -> Result<(), CoreError>;
    fn ensure_budget(&mut self, budget: &SourceBudget) -> Result<(), CoreError>;
    fn events(&mut self, count: u64) -> Result<(), CoreError>;
    fn scan(&mut self, count: u64) -> Result<(), CoreError>;
    fn copy(&mut self, count: u64) -> Result<(), CoreError>;
    fn fail(&mut self, error: CoreError) -> CoreError;
    /// Optional lowering scratch keeps the existing soft quota fallback. Work
    /// errors still latch on the original authority through the methods above.
    fn reserve_optional(&mut self, bytes: u64) -> Result<Lease, CoreError>;
}

impl ProjectionWork for Operation {
    fn checkpoint(&mut self) -> Result<(), CoreError> {
        self.checkpoint()
    }
    fn checkpoint_now(&mut self) -> Result<(), CoreError> {
        self.checkpoint_now()
    }
    fn ensure_budget(&mut self, budget: &SourceBudget) -> Result<(), CoreError> {
        self.ensure_budget(budget)
    }
    fn events(&mut self, count: u64) -> Result<(), CoreError> {
        self.events(count)
    }
    fn scan(&mut self, count: u64) -> Result<(), CoreError> {
        self.scan(count)
    }
    fn copy(&mut self, count: u64) -> Result<(), CoreError> {
        self.copy(count)
    }
    fn fail(&mut self, error: CoreError) -> CoreError {
        self.fail(error)
    }
    fn reserve_optional(&mut self, bytes: u64) -> Result<Lease, CoreError> {
        self.budget().reserve_auxiliary(bytes)
    }
}

pub(crate) struct OwnedProjectionWork<'work, 'budget, 'id, 'op> {
    work: &'work mut OwnedWork<'id, 'op>,
    budget: &'budget SourceBudget,
}

impl<'work, 'budget, 'id, 'op> OwnedProjectionWork<'work, 'budget, 'id, 'op> {
    pub(crate) fn new(
        work: &'work mut OwnedWork<'id, 'op>,
        budget: &'budget SourceBudget,
    ) -> Result<Self, CoreError> {
        // The immutable source supplies only the optional reservation root.
        // This same-operation check proves it cannot borrow a different root;
        // the wrapper never creates an Operation or construction identity.
        work.ensure_budget(budget)?;
        Ok(Self { work, budget })
    }
}

impl ProjectionWork for OwnedProjectionWork<'_, '_, '_, '_> {
    fn checkpoint(&mut self) -> Result<(), CoreError> {
        self.work.check()
    }
    fn checkpoint_now(&mut self) -> Result<(), CoreError> {
        self.work.checkpoint_now()
    }
    fn ensure_budget(&mut self, budget: &SourceBudget) -> Result<(), CoreError> {
        self.work.ensure_budget(budget)
    }
    fn events(&mut self, count: u64) -> Result<(), CoreError> {
        self.work.events(count)
    }
    fn scan(&mut self, count: u64) -> Result<(), CoreError> {
        self.work.scan(count)
    }
    fn copy(&mut self, count: u64) -> Result<(), CoreError> {
        self.work.copy(count)
    }
    fn fail(&mut self, error: CoreError) -> CoreError {
        self.work.fail(error)
    }
    fn reserve_optional(&mut self, bytes: u64) -> Result<Lease, CoreError> {
        self.budget.reserve_auxiliary(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{CancellationToken, ModelLimits, SourceLimits, construction_scope};

    #[test]
    fn owned_projection_never_borrows_another_budget_or_resets_a_fault() {
        let budget = SourceBudget::preview();
        let other = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            assert!(matches!(
                OwnedProjectionWork::new(&mut work, &other),
                Err(CoreError::WrongBudget)
            ));
            assert_eq!(work.first_fault(), Some(CoreError::WrongBudget));
            assert_eq!(work.finish(), Err(CoreError::WrongBudget));
        });
    }

    #[test]
    fn optional_scratch_quota_fallback_keeps_original_work_usable() {
        let budget = SourceBudget::new_with_model_limits(
            SourceLimits {
                auxiliary_capacity: 1,
                ..SourceLimits::default()
            },
            ModelLimits::default(),
        )
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut projection = OwnedProjectionWork::new(&mut work, &budget).unwrap();
            assert!(matches!(
                projection.reserve_optional(2),
                Err(CoreError::ResourceLimit { .. })
            ));
            projection.scan(1).unwrap();
            assert_eq!(work.first_fault(), None);
            work.finish().unwrap();
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
