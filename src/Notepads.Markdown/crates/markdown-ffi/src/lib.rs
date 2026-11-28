// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private C ABI. Pointer provenance and call ordering are caller obligations.

mod core;
#[cfg(feature = "diagnostics")]
mod diagnostics;
mod measurement_cache;
mod panic_boundary;

pub use core::*;
#[cfg(feature = "diagnostics")]
pub use diagnostics::*;
pub use measurement_cache::*;
use panic_boundary::contain_panic;

pub const STATUS_OK: i32 = 0;
pub const STATUS_INVALID_ARGUMENT: i32 = 1;
pub const STATUS_ABI_MISMATCH: i32 = 2;
pub const STATUS_INVALID_UTF8: i32 = 3;
pub const STATUS_CANCELED: i32 = 4;
pub const STATUS_INVALID_STATE: i32 = 5;
pub const STATUS_PANIC: i32 = 6;
pub const STATUS_OUT_OF_MEMORY: i32 = 7;
