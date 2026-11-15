// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Closed leaf work: no allocation, reentrant Operation or application callback inside.

use super::{CHECKPOINT_EVENTS, CoreError, Operation};
use std::ops::ControlFlow;
use std::str::Chars;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarChargeOrder {
    ScanThenEvent,
    EventThenScan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarBatchEnd {
    Exhausted,
    Full,
    Break,
}

impl Operation {
    /// Caller has already charged its event and validated length/page, in that order.
    /// The action only checks the slot and copies one fixed row, with no other work.
    pub(crate) fn read_metadata<T: Copy>(
        &mut self,
        bytes: u64,
        read: impl FnOnce() -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.view_scan(bytes)?;
        self.view_copy(bytes)?;
        read().map_err(|error| self.fail(error))
    }

    /// At most 256 next/visitor steps. Visitors only fold local scalar state;
    /// they must break before an operation, allocation or any additional scan.
    pub(crate) fn visit_scalar_batch(
        &mut self,
        input: &mut Chars<'_>,
        order: ScalarChargeOrder,
        visit: impl FnMut(char) -> Result<ControlFlow<()>, CoreError>,
    ) -> Result<ScalarBatchEnd, CoreError> {
        let result = scalar_loop(
            input,
            |scalar| match order {
                ScalarChargeOrder::ScanThenEvent => {
                    self.scan(scalar.len_utf8() as u64)?;
                    self.events(1)
                }
                ScalarChargeOrder::EventThenScan => {
                    self.events(1)?;
                    self.scan(scalar.len_utf8() as u64)
                }
            },
            visit,
        );
        result.map_err(|error| self.fail(error))
    }
}

fn scalar_loop(
    input: &mut Chars<'_>,
    mut charge: impl FnMut(char) -> Result<(), CoreError>,
    mut visit: impl FnMut(char) -> Result<ControlFlow<()>, CoreError>,
) -> Result<ScalarBatchEnd, CoreError> {
    for _ in 0..CHECKPOINT_EVENTS {
        let Some(scalar) = input.next() else {
            return Ok(ScalarBatchEnd::Exhausted);
        };
        charge(scalar)?;
        if visit(scalar)?.is_break() {
            return Ok(ScalarBatchEnd::Break);
        }
    }
    Ok(ScalarBatchEnd::Full)
}
