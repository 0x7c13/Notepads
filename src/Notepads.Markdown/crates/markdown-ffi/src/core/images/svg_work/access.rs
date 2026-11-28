// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Owned, serialized access to the original account. Engine callbacks call the
//! shared Rust transactions directly; they never reenter exported FFI functions.
//! This module does not expose a job ABI or enable SVG image publication.

use super::*;
use markdown_core::policy::Operation;
use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourceKind, Ticket};
use std::sync::{Arc, OnceLock};

struct AccountPointer(NonNull<Owned<SvgWork>>);
// SAFETY: construction transfers the sole canonical account from its caller.
// Every access is guarded by AccountAccess::state. No borrowed arena row or
// retained owner pointer is exposed, dereferenced, or destroyed by this module.
// Reclamation requires all Host references to have physically retired first.
unsafe impl Send for AccountPointer {}

struct State {
    account: AccountPointer,
    operation: Operation,
    first_error: Option<CoreError>,
}

pub(super) struct AccountAccess {
    state: Mutex<State>,
    coverage: OnceLock<markdown_svg::coverage::NativeCoverageProvider>,
}

/// Canonical task-side owner. Accidental drop quarantines the account and its
/// funded Arc rather than inventing a physical-free acknowledgement.
pub(super) struct AccountOwner {
    access: Option<Arc<AccountAccess>>,
    bootstrap: SvgWorkToken,
    quarantined: Option<AccountPointer>,
}

fn arc_layout<T>() -> Layout {
    Layout::new::<(usize, usize)>()
        .extend(Layout::new::<T>())
        .unwrap()
        .0
        .pad_to_align()
}

impl AccountOwner {
    /// # Safety
    /// Caller owns this live, exclusive canonical account, with zero tickets and
    /// no outstanding calls. On success `account` becomes null. The original
    /// request remains pinned by the task through completion. No borrowed raw
    /// pointer may be used after this ownership transfer.
    pub(super) unsafe fn take(account: &mut *mut Owned<SvgWork>) -> Result<Self, CoreError> {
        let pointer = NonNull::new(*account).ok_or(CoreError::InvalidState)?;
        // SAFETY: exclusive original account, not published until success.
        let value = unsafe { &mut (*pointer.as_ptr()).value };
        if value.poisoned || value.tickets != 0 || value.pending != 0 {
            return Err(CoreError::InvalidState);
        }
        let bytes = arc_layout::<AccountAccess>().size() as u64;
        let bootstrap = value.reserve(0, bytes)?;
        let operation = value.funding.begin_engine_operation();
        let access = Arc::new(AccountAccess {
            state: Mutex::new(State {
                account: AccountPointer(pointer),
                operation,
                first_error: None,
            }),
            coverage: OnceLock::new(),
        });
        if let Err(error) = value.admit(bootstrap, bytes) {
            // No Host reference escaped. Free the real Arc before its pending
            // funding is returned. The caller still owns the original handle.
            drop(access);
            value.release_after_free(bootstrap)?;
            return Err(error);
        }
        *account = ptr::null_mut();
        Ok(Self {
            access: Some(access),
            bootstrap,
            quarantined: None,
        })
    }

    #[cfg(test)]
    pub(super) fn actual_arc_allocation(&self) -> (usize, usize) {
        let (layout, offset) = Layout::new::<(usize, usize)>()
            .extend(Layout::new::<AccountAccess>())
            .unwrap();
        let data = Arc::as_ptr(self.access.as_ref().unwrap()) as usize;
        (data - offset, layout.pad_to_align().size())
    }

    pub(super) fn host(&self) -> Arc<AccountAccess> {
        self.access.as_ref().expect("live canonical access").clone()
    }

    /// Requires the engine shell, its cancellation owner and font callbacks to
    /// have retired. This does not assert that a Native thread has signaled.
    pub(super) fn reclaim(&mut self) -> Result<*mut Owned<SvgWork>, CoreError> {
        let access = self.access.as_ref().ok_or(CoreError::InvalidState)?;
        if Arc::strong_count(access) != 1 || Arc::weak_count(access) != 0 {
            return Err(CoreError::InvalidState);
        }
        let access = self.access.take().unwrap();
        let access = match Arc::try_unwrap(access) {
            Ok(access) => access,
            Err(access) => {
                self.access = Some(access);
                return Err(CoreError::InvalidState);
            }
        };
        // try_unwrap has physically freed the Arc allocation (no Weak escape).
        // The returned State keeps the canonical account alive independently.
        let (state, poisoned) = match access.state.into_inner() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        let pointer = state.account;
        if poisoned {
            // SAFETY: all Host references retired before Arc::try_unwrap.
            // Returning the canonical handle must never erase mutex poison.
            unsafe {
                (*pointer.0.as_ptr()).value.poisoned = true;
            }
        }
        drop(state.operation);
        // SAFETY: unique canonical handle recovered after every Host retired.
        let result = unsafe {
            (*pointer.0.as_ptr())
                .value
                .release_after_free(self.bootstrap)
        };
        if let Err(error) = result {
            self.quarantined = Some(pointer);
            return Err(error);
        }
        Ok(pointer.0.as_ptr())
    }
}

impl Drop for AccountOwner {
    fn drop(&mut self) {
        if let Some(access) = self.access.take() {
            std::mem::forget(access);
        }
        // A failed reclamation's raw canonical allocation is deliberately kept.
    }
}

impl AccountAccess {
    pub(super) fn bind_coverage(
        &self,
        provider: markdown_svg::coverage::NativeCoverageProvider,
    ) -> Result<(), CoreError> {
        self.coverage
            .set(provider)
            .map_err(|_| CoreError::InvalidState)
    }
    pub(super) fn with<T>(
        &self,
        action: impl FnOnce(&mut SvgWork, &mut Operation) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.with_cleanup(false, action)
    }

    pub(super) fn with_cleanup<T>(
        &self,
        cleanup: bool,
        action: impl FnOnce(&mut SvgWork, &mut Operation) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        #[cfg(test)]
        BEFORE_LOCK
            .with_borrow_mut(|hook| hook.take())
            .map(|hook| hook());
        let (mut state, poisoned) = match self.state.lock() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        let State {
            account,
            operation,
            first_error,
        } = &mut *state;
        // SAFETY: this module owns the canonical allocation; the mutex excludes
        // every other access and no account borrow survives this invocation.
        let value = unsafe { &mut (*account.0.as_ptr()).value };
        if poisoned {
            value.poisoned = true;
            first_error.get_or_insert(CoreError::InternalInvariant);
        }
        let result = if value.poisoned && !cleanup {
            Err(CoreError::InvalidState)
        } else {
            action(value, operation)
        };
        if let Err(error) = result {
            first_error.get_or_insert(error);
        }
        result
    }

    #[cfg(test)]
    pub(super) fn panic_with_lock(&self) {
        let _guard = self.state.lock().unwrap();
        panic!("injected engine account panic");
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> (u32, u32, u64, Option<CoreError>) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: exclusive mutex borrow of the owned account.
        let value = unsafe { &(*state.account.0.as_ptr()).value };
        (
            value.tickets,
            value.pending,
            value.funding.stats().job_bytes,
            state.first_error,
        )
    }
}

fn token(token: &Ticket) -> SvgWorkToken {
    SvgWorkToken {
        account_nonce: token.account_nonce(),
        slot: token.slot(),
        generation: token.generation(),
    }
}
pub(super) fn resource_error(error: CoreError) -> Error {
    match error {
        CoreError::Cancelled | CoreError::StaleResult => {
            Error::new(FailureKind::Cancelled, LimitKind::None)
        }
        CoreError::ResourceLimit { kind, .. } => Error::new(
            FailureKind::Quota,
            if kind == markdown_core::error::LimitKind::Work {
                LimitKind::Work
            } else {
                LimitKind::Bytes
            },
        ),
        CoreError::OutOfMemory => Error::new(FailureKind::Allocation, LimitKind::None),
        CoreError::DeadlineExceeded { .. } => Error::native(0x800705b4u32 as i32),
        CoreError::ArithmeticOverflow => Error::new(FailureKind::Quota, LimitKind::Capacity),
        _ => Error::new(FailureKind::Invariant, LimitKind::None),
    }
}

impl markdown_svg::Host for AccountAccess {
    fn coverage_provider(&self) -> Option<&dyn markdown_svg::coverage::CoverageProvider> {
        self.coverage
            .get()
            .map(|provider| provider as &dyn markdown_svg::coverage::CoverageProvider)
    }
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        #[cfg(test)]
        match ENGINE_RESERVE.with(|slot| slot.replace(0)) {
            1 => return Err(Error::native(-779)),
            2 => panic!("injected engine constructor reserve panic"),
            _ => {}
        }
        self.with(|account, _| account.reserve_resource(kind as u32, bytes as u64))
            .map_err(resource_error)
            .and_then(|t| Ticket::new(t.account_nonce, t.slot, t.generation))
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        self.with(|account, _| account.admit(token(ticket), bytes as u64))
            .map_err(resource_error)
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        self.with_cleanup(true, |account, _| {
            account.release_after_free(token(&ticket))
        })
        .map_err(resource_error)
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.checkpoint_core(work).map_err(resource_error)
    }
}

#[cfg(test)]
thread_local! {
    static BEFORE_LOCK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    static ENGINE_RESERVE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
impl AccountAccess {
    pub(super) fn inject_engine_reserve(point: u8) {
        ENGINE_RESERVE.with(|slot| slot.set(point));
    }

    pub(super) fn before_next_lock(hook: impl FnOnce() + 'static) {
        BEFORE_LOCK.with_borrow_mut(|slot| *slot = Some(Box::new(hook)));
    }
    pub(super) fn while_locked(&self, action: impl FnOnce()) {
        let _guard = self.state.lock().unwrap();
        action();
    }
}

impl AccountOwner {
    pub(super) fn canonical_pin(&self) -> *mut Owned<SvgWork> {
        if let Some(access) = &self.access {
            access
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .account
                .0
                .as_ptr()
        } else {
            self.quarantined
                .as_ref()
                .map_or(ptr::null_mut(), |p| p.0.as_ptr())
        }
    }
    pub(super) fn bootstrap_token(&self) -> SvgWorkToken {
        self.bootstrap
    }
    pub(super) fn has_exact_references(&self, expected: usize) -> bool {
        self.access
            .as_ref()
            .is_some_and(|a| Arc::strong_count(a) == expected && Arc::weak_count(a) == 0)
    }
}

impl AccountAccess {
    pub(super) fn checkpoint_core(&self, work: u64) -> Result<(), CoreError> {
        self.with(|account, operation| {
            if account.poisoned || account.funding.stats().closed {
                return Err(CoreError::InvalidState);
            }
            if !account.funding.is_current() {
                return Err(CoreError::StaleResult);
            }
            // Engine batches (for example 4096 work credit) are larger than
            // Core's bounded event blocks. Preserve the entire same operation
            // and charge every unit in its original <=256-event windows.
            let mut remaining = work;
            while remaining != 0 {
                let batch = remaining.min(markdown_core::policy::CHECKPOINT_EVENTS);
                operation.external_events(batch)?;
                remaining -= batch;
            }
            operation.boundary_work().checkpoint_now()
        })
    }
}
