// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded native math rendering for Notepads Markdown.
#![forbid(unsafe_code)]

#[doc(hidden)]
pub mod chinese_text;
pub mod fonts;
#[cfg(test)]
mod funding;
pub mod limits;
pub mod outline;
/// Private native integration surface; not a WinRT or application API.
#[doc(hidden)]
pub mod pending;
mod render;
pub mod types;
pub use limits::{MathLimits, WorkerStackContract};
pub use ratex_types::execution::{ExecutionControl, Limits as ExecutionLimits, Phase, Resource};
pub use ratex_types::storage::{StorageFamily, StorageLedger};
pub use render::{JobFunding, OwnedMathArtifact, render};
pub use types::*;

#[cfg(test)]
mod font_tests;
