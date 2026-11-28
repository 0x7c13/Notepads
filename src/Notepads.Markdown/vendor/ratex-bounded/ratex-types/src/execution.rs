// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Experimental explicit, borrowed per-operation control. No thread-local job or allocator.
//! Counters are logical consumed work, not an actual allocation/RSS guarantee.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Phase {
    Admission,
    Lexer,
    Parser,
    Macro,
    Layout,
    Validate,
    Display,
    Outline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Resource {
    InputBytes,
    Tokens,
    ParserSteps,
    MacroExpansions,
    LayoutNodes,
    DisplayItems,
    OutlineSegments,
    GeneratedBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionError {
    InternalInvariant {
        phase: Phase,
    },
    Unsupported {
        phase: Phase,
    },
    StorageFailure {
        phase: Phase,
    },
    Cancelled {
        phase: Phase,
    },
    Deadline {
        phase: Phase,
    },
    ResourceLimit {
        phase: Phase,
        resource: Resource,
        limit: u64,
        requested: u64,
    },
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ExecutionError {}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub values: [u64; 8],
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            values: [
                4096,
                100_000,
                100_000,
                1000,
                100_000,
                65_536,
                131_072,
                8 * 1024 * 1024,
            ],
        }
    }
}

/// An explicit host observer is optional. The isolated driver uses it to request
/// cancellation at a real loop checkpoint, after that phase has consumed work.
/// It is neither a process-global current operation nor a successful-result hook.
pub trait Observer {
    fn checkpoint(&self, phase: Phase, ordinal: u64);
}

pub struct ExecutionControl<'a> {
    pub(crate) storage_enabled: Cell<bool>,
    pub(crate) storage_bytes: [Cell<u64>; crate::storage::FAMILY_COUNT],
    pub(crate) storage_slots: [Cell<u64>; crate::storage::FAMILY_COUNT],
    pub(crate) storage_failure: Cell<Option<crate::storage::StorageError>>,
    cancelled: &'a AtomicBool,
    deadline: Option<Instant>,
    limits: Limits,
    failure: Cell<Option<ExecutionError>>,
    checkpoints: [Cell<u64>; 8],
    consumed: [Cell<u64>; 8],
    observer: Option<&'a dyn Observer>,
}

impl<'a> ExecutionControl<'a> {
    /// A production bridge rejects observers before any checkpoint can call them.
    pub fn has_observer(&self) -> bool {
        self.observer.is_some()
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub fn new(cancelled: &'a AtomicBool, deadline: Option<Instant>, limits: Limits) -> Self {
        Self {
            storage_enabled: Cell::new(false),
            storage_bytes: std::array::from_fn(|_| Cell::new(0)),
            storage_slots: std::array::from_fn(|_| Cell::new(0)),
            storage_failure: Cell::new(None),
            cancelled,
            deadline,
            limits,
            failure: Cell::new(None),
            checkpoints: std::array::from_fn(|_| Cell::new(0)),
            consumed: std::array::from_fn(|_| Cell::new(0)),
            observer: None,
        }
    }

    pub fn with_observer(mut self, observer: &'a dyn Observer) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn checkpoint(&self, phase: Phase) -> Result<(), ExecutionError> {
        if self.storage_failure.get().is_some() {
            return Err(ExecutionError::StorageFailure { phase });
        }
        if let Some(error) = self.failure.get() {
            return Err(error);
        }
        let ordinal = self.checkpoints[phase as usize].get().saturating_add(1);
        self.checkpoints[phase as usize].set(ordinal);
        if let Some(observer) = self.observer {
            observer.checkpoint(phase, ordinal);
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(self.fail(ExecutionError::Cancelled { phase }));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(self.fail(ExecutionError::Deadline { phase }));
        }
        Ok(())
    }

    /// Reject before the corresponding work or allocation, including amount zero.
    pub fn consume(
        &self,
        phase: Phase,
        resource: Resource,
        amount: u64,
    ) -> Result<(), ExecutionError> {
        self.checkpoint(phase)?;
        let index = resource as usize;
        let next = self.consumed[index].get().checked_add(amount);
        let limit = self.limits.values[index];
        match next {
            Some(value) if value <= limit => {
                self.consumed[index].set(value);
                Ok(())
            }
            _ => Err(self.fail(ExecutionError::ResourceLimit {
                phase,
                resource,
                limit,
                requested: next.unwrap_or(u64::MAX),
            })),
        }
    }

    pub fn fail(&self, error: ExecutionError) -> ExecutionError {
        if self.failure.get().is_none() {
            self.failure.set(Some(error));
        }
        self.failure.get().unwrap()
    }
    pub fn failure(&self) -> Option<ExecutionError> {
        self.failure.get()
    }
    pub fn checkpoint_counts(&self) -> [u64; 8] {
        std::array::from_fn(|i| self.checkpoints[i].get())
    }
    pub fn consumed_counts(&self) -> [u64; 8] {
        std::array::from_fn(|i| self.consumed[i].get())
    }
    pub fn storage(&self) -> Option<crate::storage::StorageLedger<'_>> {
        self.storage_enabled
            .get()
            .then(|| crate::storage::StorageLedger::view(self))
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
}
