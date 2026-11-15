// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private storage policy; paid storage directly uses the shared request owner.
use crate::DecodingError;
#[cfg(feature = "notepads-markdown-resources")]
pub(crate) use markdown_svg_budget::{
    codec::{Buffer, Control, One},
    Error,
};
#[cfg(not(feature = "notepads-markdown-resources"))]
pub(crate) use plain::{Buffer, Control, Error, One};
use std::io::Read;

pub(crate) fn resource(error: Error) -> DecodingError {
    DecodingError::Resources(error)
}
pub(crate) fn step(control: &Control, work: usize) -> Result<(), DecodingError> {
    control.step(0).map_err(resource)?;
    consume(control, work)
}
fn consume(control: &Control, work: usize) -> Result<(), DecodingError> {
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = control.resources() {
        return resources.consume_scan_work(work).map_err(resource);
    }
    let _ = (control, work);
    Ok(())
}
/// A local polling cadence, never a second work ledger: all work is consumed
/// from the same Resources request-wide scan credit before the primitive runs.
pub(crate) struct Work {
    control: Control,
    since_poll: usize,
}
impl Work {
    pub(crate) fn new(control: &Control) -> Self {
        Self {
            control: control.clone(),
            since_poll: 1024,
        }
    }
    pub(crate) fn step(&mut self, work: usize) -> Result<(), DecodingError> {
        if work > 1024 {
            self.since_poll = 1024;
            return step(&self.control, work);
        }
        if self.since_poll > 1024 - work {
            self.control.step(0).map_err(resource)?;
            self.since_poll = 0;
        }
        consume(&self.control, work)?;
        self.since_poll += work;
        Ok(())
    }
}
pub(crate) fn paid(control: &Control) -> bool {
    #[cfg(feature = "notepads-markdown-resources")]
    {
        control.resources().is_some()
    }
    #[cfg(not(feature = "notepads-markdown-resources"))]
    {
        let _ = control;
        false
    }
}
pub(crate) fn terminal(control: &Control, error: DecodingError) -> DecodingError {
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = control.resources() {
        // Header EOF salvage and owner cleanup can expose an earlier hard
        // resource failure. Ordinary diagnostics must never hide that failure.
        if let Some(first) = resources.failure() {
            if !matches!(
                first.kind,
                markdown_svg_budget::FailureKind::ImageDecode
                    | markdown_svg_budget::FailureKind::Unsupported
            ) {
                return resource(first);
            }
        }
        let e = match &error {
            DecodingError::Resources(e) => *e,
            DecodingError::StaticUnsupported(_) => markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Unsupported,
                markdown_svg_budget::LimitKind::None,
            ),
            _ => markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::ImageDecode,
                markdown_svg_budget::LimitKind::None,
            ),
        };
        resources.record(e);
    }
    let _ = control;
    error
}
pub(crate) fn capacity(control: &Control) -> DecodingError {
    #[cfg(feature = "notepads-markdown-resources")]
    {
        resource(control.terminal(Error::new(
            markdown_svg_budget::FailureKind::Quota,
            markdown_svg_budget::LimitKind::Capacity,
        )))
    }
    #[cfg(not(feature = "notepads-markdown-resources"))]
    {
        let _ = control;
        resource(Error::Capacity)
    }
}
pub(crate) fn mul(control: &Control, values: &[usize]) -> Result<usize, DecodingError> {
    values
        .iter()
        .try_fold(1usize, |a, b| a.checked_mul(*b))
        .ok_or_else(|| capacity(control))
}
pub(crate) fn filled<T: Copy>(
    control: &Control,
    len: usize,
    value: T,
) -> Result<Buffer<T>, DecodingError> {
    let mut b = Buffer::new(control);
    b.try_resize(len, value).map_err(resource)?;
    Ok(b)
}
pub(crate) fn unsupported(control: &Control, text: &'static str) -> DecodingError {
    let error = if paid(control) {
        DecodingError::StaticUnsupported(text)
    } else {
        DecodingError::UnsupportedFeature(text.to_owned())
    };
    terminal(control, error)
}
pub(crate) fn copy<T: Copy>(
    control: &Control,
    out: &mut [T],
    input: &[T],
) -> Result<(), DecodingError> {
    for (dst, src) in out
        .chunks_mut((65536 / std::mem::size_of::<T>().max(1)).clamp(1, 1024))
        .zip(input.chunks((65536 / std::mem::size_of::<T>().max(1)).clamp(1, 1024)))
    {
        step(
            control,
            mul(control, &[src.len(), std::mem::size_of::<T>().max(1)])?,
        )?;
        dst.copy_from_slice(src);
    }
    Ok(())
}
pub(crate) fn read_exact(
    control: &Control,
    r: &mut impl Read,
    out: &mut [u8],
) -> Result<(), DecodingError> {
    for batch in out.chunks_mut(4096) {
        step(control, batch.len())?;
        r.read_exact(batch)
            .map_err(|e| terminal(control, e.into()))?;
    }
    Ok(())
}
pub(crate) fn read_to_end(
    control: &Control,
    r: &mut impl Read,
) -> Result<Buffer<u8>, DecodingError> {
    let mut out = Buffer::new(control);
    let mut batch = [0u8; 4096];
    loop {
        step(control, 0)?;
        // Obtain credit BEFORE reading, then consume only bytes actually read.
        // This reuses the request-wide credit, with no per-read rounding ledger.
        #[cfg(feature = "notepads-markdown-resources")]
        let available = match control.resources() {
            Some(resources) => resources
                .scan_allowance()
                .map_err(resource)?
                .min(batch.len()),
            None => batch.len(),
        };
        #[cfg(not(feature = "notepads-markdown-resources"))]
        let available = batch.len();
        let len = r
            .read(&mut batch[..available])
            .map_err(|e| terminal(control, e.into()))?;
        if len == 0 {
            break;
        }
        consume(control, len)?;
        out.try_extend_from_slice(&batch[..len]).map_err(resource)?;
    }
    Ok(out)
}
#[cfg(not(feature = "notepads-markdown-resources"))]
mod plain {
    use std::{
        fmt,
        ops::{Deref, DerefMut},
    };
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Error {
        Allocation,
        Capacity,
    }
    #[derive(Clone)]
    pub struct Control;
    impl Control {
        pub const fn plain() -> Self {
            Self
        }
        pub fn step(&self, _: u64) -> Result<(), Error> {
            Ok(())
        }
    }
    pub struct Buffer<T> {
        values: Vec<T>,
    }
    impl<T> Buffer<T> {
        pub fn new(_: &Control) -> Self {
            Self { values: Vec::new() }
        }
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
            self.values
                .len()
                .checked_add(additional)
                .ok_or(Error::Capacity)?;
            self.values
                .try_reserve_exact(additional)
                .map_err(|_| Error::Allocation)
        }
        pub fn try_push(&mut self, value: T) -> Result<(), Error> {
            self.values.try_reserve(1).map_err(|_| Error::Allocation)?;
            self.values.push(value);
            Ok(())
        }
        pub fn pop(&mut self) -> Option<T> {
            self.values.pop()
        }
        pub fn try_into_plain(self) -> Result<Vec<T>, Self> {
            Ok(self.values)
        }
    }
    impl<T: Copy> Buffer<T> {
        pub fn try_resize(&mut self, len: usize, value: T) -> Result<(), Error> {
            if len <= self.values.len() {
                self.values.truncate(len);
                return Ok(());
            }
            self.try_reserve_exact(len - self.values.len())?;
            while self.values.len() < len {
                self.values.push(value);
            }
            Ok(())
        }
        pub fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), Error> {
            self.try_reserve_exact(values.len())?;
            for value in values {
                self.values.push(*value);
            }
            Ok(())
        }
    }
    impl<T> Deref for Buffer<T> {
        type Target = [T];
        fn deref(&self) -> &[T] {
            &self.values
        }
    }
    impl<T> DerefMut for Buffer<T> {
        fn deref_mut(&mut self) -> &mut [T] {
            &mut self.values
        }
    }
    impl<T: fmt::Debug> fmt::Debug for Buffer<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.values.fmt(f)
        }
    }
    pub struct One<T> {
        values: Buffer<T>,
    }
    impl<T> One<T> {
        pub fn try_new(
            control: &Control,
            make: impl FnOnce(&Control) -> Result<T, Error>,
        ) -> Result<Self, Error> {
            let mut values = Buffer::new(control);
            values.try_reserve_exact(1)?;
            values.try_push(make(control)?)?;
            Ok(Self { values })
        }
    }
    impl<T> Deref for One<T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.values[0]
        }
    }
    impl<T> DerefMut for One<T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.values[0]
        }
    }
}
