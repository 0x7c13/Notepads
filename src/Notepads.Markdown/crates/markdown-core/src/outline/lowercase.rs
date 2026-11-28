// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::storage::{copy, events, scan};
use crate::error::CoreError;
use crate::policy::{BoundaryReservation, Operation};

/// The only precharged std string-growth boundary in the outline builder.
/// The String payload retires before its reservation.
pub(super) struct Lowercase {
    pub text: String,
    _reservation: BoundaryReservation,
}

#[cfg(test)]
thread_local! {
    static CALLS: std::cell::Cell<(usize, bool)> = const { std::cell::Cell::new((0, false)) };
    static AFTER_STD: std::cell::Cell<Option<fn(&mut Operation)>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(super) fn calls() -> (usize, bool) {
    CALLS.get()
}

#[cfg(test)]
pub(super) fn after_std(hook: Option<fn(&mut Operation)>) {
    AFTER_STD.set(hook);
}

impl Lowercase {
    pub fn new(input: &str, has_sigma: bool, op: &mut Operation) -> Result<Self, CoreError> {
        // Audited against Rust1.99.1 alloc/str.rs, alloc/raw_vec/mod.rs and
        // core/unicode/unicode_data.rs; exact SHA inputs are retained in the
        // R10 design-review/BINDING-V3.json. Upgrade requires revalidation.
        // Output <=2n for that table, geometric requested new+old capacities
        // <=8n+16. This is not a Windows heap-internal/RSS bound.
        let n = u64::try_from(input.len()).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let requested = n
            .checked_mul(12)
            .and_then(|v| v.checked_add(64))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let (factor, fixed) = if has_sigma { (4096, 256) } else { (64, 128) };
        let work = n
            .checked_mul(factor)
            .and_then(|v| v.checked_add(fixed))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let writes = n
            .checked_mul(16)
            .and_then(|v| v.checked_add(128))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        // Charge the same Operation before any call. Case_Ignorable's maximum
        // 251-iteration skip run is not a constant-small lookup: each scalar is
        // queried by at most both adjacent Sigma contexts. 512 scan bytes and
        // 1536 events/query, plus <=n cased queries (128 each) and main64n,
        // fit4096n; no-Sigma executes none of those context property queries.
        scan(work, op)?;
        copy(writes, op)?;
        events(work, op)?;
        let mut reservation = op.reserve_boundary(requested)?;
        op.checkpoint_now()?;
        #[cfg(test)]
        CALLS.set((CALLS.get().0 + 1, has_sigma));
        let text = input.to_lowercase();
        #[cfg(test)]
        if let Some(hook) = AFTER_STD.take() {
            hook(op);
        }
        // The standard-library call cannot be interrupted. No result is usable
        // if cancellation/time elapsed inside it; the local text is freed first.
        op.checkpoint_now()?;
        let actual = text.capacity() as u64;
        if actual > requested {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        reservation.shrink_to(actual).map_err(|e| op.fail(e))?;
        Ok(Self {
            text,
            _reservation: reservation,
        })
    }
}
