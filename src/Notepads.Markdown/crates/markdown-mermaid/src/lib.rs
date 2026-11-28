// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Platform-independent flowchart admission, funded jobs and canonical SVG.
//! Windows text services are injected by the Windows platform crate; the core
//! does not own COM objects, graphics devices, XAML or application policies.

pub mod accessibility;
pub mod canonical;
pub mod funding;
pub mod labels;
pub mod layout;
mod native_common;
pub mod native_pie;
pub mod native_relationships;
pub mod native_sequence;
pub mod native_state;
pub mod rich_text;
pub mod scene;

pub use markdown_memory::{Family, MemoryError, MemoryScope};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramError {
    Memory(MemoryError),
    Funding(markdown_core::resources::derived::DerivedError),
    InvalidFunding,
    InvalidInput,
    Unsupported,
    Cancelled,
    Deadline,
    WorkLimited,
    InvalidGeometry,
    FontUnavailable,
    Backend,
    StackUnavailable,
}

impl From<MemoryError> for DiagramError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}
