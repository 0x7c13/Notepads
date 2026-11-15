// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::{ExecutionLimits, MathError};

#[derive(Clone, Copy, Debug)]
pub struct WorkerStackContract {
    pub usable_headroom_bytes: u64,
    pub platform_margin_bytes: u64,
    pub audited_required_bytes: Option<u64>,
}
impl WorkerStackContract {
    pub const fn unknown() -> Self {
        Self {
            usable_headroom_bytes: 0,
            platform_margin_bytes: 0,
            audited_required_bytes: None,
        }
    }
    pub fn admit(self) -> Result<(), MathError> {
        let margin = self.platform_margin_bytes.max(64 * 1024);
        let required = self
            .audited_required_bytes
            .filter(|bytes| *bytes > 0)
            .and_then(|bytes| bytes.checked_add(margin));
        match required {
            Some(required) if required <= self.usable_headroom_bytes => Ok(()),
            _ => Err(MathError::StackUnavailable {
                required: required.unwrap_or(u64::MAX),
                available: self.usable_headroom_bytes,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MathLimits;
impl MathLimits {
    pub const INPUT_BYTES: usize = 1024;
    pub const DEPTH: usize = 16;
    pub const MACROS: u64 = 128;
    pub const TOKENS: u64 = 4096;
    pub const GENERATED_BYTES: u64 = 65536;
    pub const WEIGHTED_LAYOUT: u64 = 8192;
    pub const OUTPUT_ITEMS: u64 = 2048;
    pub const OUTLINE_COMMANDS: u64 = 65536;
    pub const fn execution(self) -> ExecutionLimits {
        ExecutionLimits {
            values: [1024, 4096, 16384, 128, 8192, 2048, 65536, 65536],
        }
    }
    pub(crate) fn validate_control(self, actual: ExecutionLimits) -> Result<(), MathError> {
        if actual
            .values
            .iter()
            .zip(self.execution().values)
            .any(|(actual, maximum)| *actual > maximum)
        {
            Err(MathError::InvalidLimits)
        } else {
            Ok(())
        }
    }
}
