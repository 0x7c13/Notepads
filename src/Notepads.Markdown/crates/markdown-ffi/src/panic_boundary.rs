// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Shared unwind containment for every private native boundary.

use crate::STATUS_PANIC;
use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub(crate) fn drop_panic_payload(payload: Box<dyn Any + Send>) {
    // A panic payload can panic again during Drop. Contain that unwind too;
    // only the exceptional second payload is intentionally leaked.
    if let Err(second_payload) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
        std::mem::forget(second_payload);
    }
}

pub(crate) fn contain_panic(action: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(action)) {
        Ok(code) => code,
        Err(payload) => {
            drop_panic_payload(payload);
            STATUS_PANIC
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_payload_drop_is_contained() {
        struct PanickingPayload;
        impl Drop for PanickingPayload {
            fn drop(&mut self) {
                panic!("panic payload drop probe");
            }
        }
        assert_eq!(
            contain_panic(|| std::panic::panic_any(PanickingPayload)),
            STATUS_PANIC
        );
    }
}
