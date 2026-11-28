// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private shared storage policy for bounded codec consumers.

#[cfg(feature = "notepads-markdown-resources")]
pub use markdown_svg_budget::{
    codec::{Buffer, Control},
    Error,
};

#[cfg(not(feature = "notepads-markdown-resources"))]
pub use plain::{Buffer, Control, Error};

pub fn is_paid(control: &Control) -> bool {
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

/// Consume the existing request-wide prepaid work credit; no second counter.
pub fn consume_work(control: &Control, work: usize) -> Result<(), Error> {
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = control.resources() {
        return resources.consume_scan_work(work);
    }
    let _ = (control, work);
    Ok(())
}

pub fn capacity_error(control: &Control) -> Error {
    #[cfg(feature = "notepads-markdown-resources")]
    {
        control.terminal(Error::new(
            markdown_svg_budget::FailureKind::Quota,
            markdown_svg_budget::LimitKind::Capacity,
        ))
    }
    #[cfg(not(feature = "notepads-markdown-resources"))]
    {
        let _ = control;
        Error::Capacity
    }
}

pub fn terminal_decode(control: &Control) {
    #[cfg(feature = "notepads-markdown-resources")]
    {
        control.terminal(Error::new(
            markdown_svg_budget::FailureKind::ImageDecode,
            markdown_svg_budget::LimitKind::None,
        ));
    }
    #[cfg(not(feature = "notepads-markdown-resources"))]
    {
        let _ = control;
    }
}

#[cfg(not(feature = "notepads-markdown-resources"))]
mod plain {
    use alloc::vec::Vec;
    use core::{
        fmt,
        ops::{Deref, DerefMut},
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Error {
        Allocation,
        Capacity,
    }

    #[derive(Clone, Default)]
    pub struct Control;
    impl Control {
        pub const fn plain() -> Self {
            Self
        }
        pub fn failure(&self) -> Option<Error> {
            None
        }
        pub fn step(&self, _work: u64) -> Result<(), Error> {
            Ok(())
        }
        pub fn terminal(&self, error: Error) -> Error {
            error
        }
    }

    pub struct Buffer<T> {
        values: Vec<T>,
        control: Control,
    }
    impl<T> Buffer<T> {
        pub fn new(control: &Control) -> Self {
            Self {
                values: Vec::new(),
                control: control.clone(),
            }
        }
        pub fn len(&self) -> usize {
            self.values.len()
        }
        pub fn is_empty(&self) -> bool {
            self.values.is_empty()
        }
        pub fn capacity(&self) -> usize {
            self.values.capacity()
        }
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
            self.values
                .len()
                .checked_add(additional)
                .ok_or(Error::Capacity)?;
            self.values
                .try_reserve(additional)
                .map_err(|_| Error::Allocation)
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
            self.try_reserve(1)?;
            self.values.push(value);
            Ok(())
        }
        pub fn pop(&mut self) -> Option<T> {
            self.values.pop()
        }
        pub fn truncate(&mut self, len: usize) {
            self.values.truncate(len);
        }
        pub fn clear(&mut self) {
            self.values.clear();
        }
        pub fn release_storage(&mut self) {
            self.values = Vec::new();
        }
        pub fn try_resize_with(
            &mut self,
            len: usize,
            mut make: impl FnMut(&Control) -> Result<T, Error>,
        ) -> Result<(), Error> {
            let old_len = self.len();
            if len <= old_len {
                self.truncate(len);
                return Ok(());
            }
            self.try_reserve_exact(len - old_len)?;
            while self.len() < len {
                match make(&self.control) {
                    Ok(value) => self.values.push(value),
                    Err(error) => {
                        self.truncate(old_len);
                        return Err(error);
                    }
                }
            }
            Ok(())
        }
        pub fn try_clone_with(
            &self,
            mut make: impl FnMut(&T, &Control) -> Result<T, Error>,
        ) -> Result<Self, Error> {
            let mut result = Self::new(&self.control);
            result.try_reserve_exact(self.len())?;
            for value in &self.values {
                result.values.push(make(value, &self.control)?);
            }
            Ok(result)
        }
        pub fn as_plain_vec(&self) -> Option<&Vec<T>> {
            Some(&self.values)
        }
        pub fn try_into_plain(self) -> Result<Vec<T>, Self> {
            Ok(self.values)
        }
    }
    impl<T: Copy> Buffer<T> {
        pub fn try_resize(&mut self, len: usize, value: T) -> Result<(), Error> {
            self.try_resize_with(len, |_| Ok(value))
        }
        pub fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), Error> {
            self.try_reserve_exact(values.len())?;
            for value in values {
                self.values.push(*value);
            }
            Ok(())
        }
        pub fn try_clone(&self) -> Result<Self, Error> {
            self.try_clone_with(|value, _| Ok(*value))
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
}
