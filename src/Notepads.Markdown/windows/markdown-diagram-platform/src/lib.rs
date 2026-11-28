// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Windows-only DirectWrite platform for the private, whole-diagram job ABI.
//! The caller captures free-threaded factory/font collection/fallback objects;
//! no XAML, host document, or application object enters this crate.

#![cfg(windows)]

mod host;
mod native;
#[cfg(test)]
mod native_measure;
mod outline;
#[cfg(test)]
mod tests;

pub use host::WindowsText;
pub use native::render_native;

const _: () = assert!(std::mem::size_of::<WindowsText>() <= 1024);

/// Captured semantic colors for one whole diagram; authored static style rules
/// remain authoritative through Merman's normal flowchart paint cascade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTheme {
    pub dark: bool,
    pub foreground: [u8; 4],
    pub background: [u8; 4],
    pub border: [u8; 4],
    pub panel: [u8; 4],
    pub link: [u8; 4],
}

/// Private ABI capability, never constructible from Markdown. The enclosing
/// native owner retains its three COM objects through acquisition and until the
/// controlled thread is signaled. `WindowsText` takes its own COM strong refs.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FontCapability {
    pub version: u32,
    pub size: u32,
    pub factory: *mut core::ffi::c_void,
    pub collection: *mut core::ffi::c_void,
    pub fallback: *mut core::ffi::c_void,
    pub font_epoch: u64,
    pub locale_name: [u16; 85],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformError {
    InvalidCapability,
    Apartment,
    FontUnavailable,
    DirectWrite(i32),
    Memory(markdown_memory::MemoryError),
    Cancelled,
}
