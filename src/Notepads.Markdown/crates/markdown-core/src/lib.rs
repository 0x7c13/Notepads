// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Platform-independent source, native parsing, semantic IR and bidirectional mapping.

#![forbid(unsafe_code)]

mod code_fence;
pub mod config;
pub mod diagram_accessibility;
pub mod error;
pub mod highlight;
mod html;
pub mod identity;
pub mod ir;
pub mod layout;
pub mod mapping;
pub mod measurement_cache;
mod memory;
pub mod model;
pub mod outline;
pub(crate) mod parse;
pub mod policy;
pub mod presentation;
pub mod reader;
pub mod resources;
pub mod session;
pub mod source;
