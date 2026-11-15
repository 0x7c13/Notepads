// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private storage modes for the single XML writing kernel.
use std::{convert::Infallible, fmt, io::Write, ops::Range};

use markdown_svg_budget::{
    slice::copy_within_with_control, BudgetVec, Error, FailureKind, LimitKind, RequestControl,
    ResourceKind, ResourcesRef,
};

use crate::DepthData;

pub(crate) fn plain<T>(value: Result<T, Infallible>) -> T {
    match value {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

pub(crate) trait Bytes {
    type Error;
    fn as_slice(&self) -> &[u8];
    fn push(&mut self, value: u8) -> Result<(), Self::Error>;
    fn extend(&mut self, values: &[u8]) -> Result<(), Self::Error>;
    fn replace(&mut self, range: Range<usize>, values: &[u8]) -> Result<(), Self::Error>;
    fn append_existing(&mut self, range: Range<usize>) -> Result<(), Self::Error>;
    fn format(&mut self, args: fmt::Arguments<'_>) -> Result<(), Self::Error>;
}

pub(crate) trait Depths {
    type Error;
    fn len(&self) -> usize;
    fn push(&mut self, value: DepthData) -> Result<(), Self::Error>;
    fn pop(&mut self) -> Option<DepthData>;
    fn last_mut(&mut self) -> Option<&mut DepthData>;
}

pub(crate) trait Policy {
    type Error;
    type Bytes: Bytes<Error = Self::Error>;
    type Depths: Depths<Error = Self::Error>;
    fn depths(&self) -> Self::Depths;
    fn check(&self, work: usize) -> Result<(), Self::Error>;
    fn invalid_state(&self, message: &str) -> Self::Error;
}

pub(crate) struct Plain;
pub(crate) struct PlainBytes(pub(crate) Vec<u8>);
pub(crate) struct PlainDepths(Vec<DepthData>);

impl Policy for Plain {
    type Error = Infallible;
    type Bytes = PlainBytes;
    type Depths = PlainDepths;
    fn depths(&self) -> PlainDepths {
        PlainDepths(Vec::new())
    }
    fn check(&self, _: usize) -> Result<(), Infallible> {
        Ok(())
    }
    fn invalid_state(&self, message: &str) -> Infallible {
        panic!("{}", message)
    }
}

impl Bytes for PlainBytes {
    type Error = Infallible;
    fn as_slice(&self) -> &[u8] {
        &self.0
    }
    fn push(&mut self, value: u8) -> Result<(), Infallible> {
        self.0.push(value);
        Ok(())
    }
    fn extend(&mut self, values: &[u8]) -> Result<(), Infallible> {
        self.0.extend_from_slice(values);
        Ok(())
    }
    fn replace(&mut self, range: Range<usize>, values: &[u8]) -> Result<(), Infallible> {
        self.0.splice(range, values.iter().cloned());
        Ok(())
    }
    fn append_existing(&mut self, range: Range<usize>) -> Result<(), Infallible> {
        // Keep the original ordinary allocation/growth behavior.
        for i in range {
            self.0.push(self.0[i]);
        }
        Ok(())
    }
    fn format(&mut self, args: fmt::Arguments<'_>) -> Result<(), Infallible> {
        self.0.write_fmt(args).unwrap();
        Ok(())
    }
}

impl Depths for PlainDepths {
    type Error = Infallible;
    fn len(&self) -> usize {
        self.0.len()
    }
    fn push(&mut self, value: DepthData) -> Result<(), Infallible> {
        self.0.push(value);
        Ok(())
    }
    fn pop(&mut self) -> Option<DepthData> {
        self.0.pop()
    }
    fn last_mut(&mut self) -> Option<&mut DepthData> {
        self.0.last_mut()
    }
}

pub(crate) struct Paid(pub(crate) ResourcesRef);
pub(crate) struct PaidBytes(pub(crate) BudgetVec<u8>);
pub(crate) struct PaidDepths(BudgetVec<DepthData>);

impl Policy for Paid {
    type Error = Error;
    type Bytes = PaidBytes;
    type Depths = PaidDepths;
    fn depths(&self) -> PaidDepths {
        PaidDepths(BudgetVec::new(self.0.clone(), ResourceKind::Scratch))
    }
    fn check(&self, work: usize) -> Result<(), Error> {
        self.0.checkpoint(work as u64)
    }
    fn invalid_state(&self, _: &str) -> Error {
        self.0
            .record(Error::new(FailureKind::Invariant, LimitKind::None))
    }
}

impl PaidBytes {
    fn invalid(&self) -> Error {
        self.0
            .resources()
            .record(Error::new(FailureKind::Invariant, LimitKind::None))
    }
    fn length(&self, additional: usize) -> Result<usize, Error> {
        self.0.len().checked_add(additional).ok_or_else(|| {
            self.0
                .resources()
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
        })
    }
}

impl Bytes for PaidBytes {
    type Error = Error;
    fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
    fn push(&mut self, value: u8) -> Result<(), Error> {
        self.0.resources().checkpoint(1)?;
        self.0.try_push(value)
    }
    fn extend(&mut self, values: &[u8]) -> Result<(), Error> {
        self.0.resources().checkpoint(0)?;
        self.0.try_reserve(values.len())?;
        for window in values.chunks(256) {
            // BudgetVec's ordinary append owns allocation accounting, not the
            // caller's copy work. Prepay this actual output window even when
            // the complete capacity was retained from an earlier write.
            self.0.resources().checkpoint(window.len() as u64)?;
            self.0.try_extend_from_slice(window)?;
        }
        self.0.resources().checkpoint(0)
    }
    fn replace(&mut self, range: Range<usize>, values: &[u8]) -> Result<(), Error> {
        let old_length = self.0.len();
        // The XML kernel only inserts longer constant escapes for one byte.
        if range.start > range.end || range.end > old_length || values.len() < range.len() {
            return Err(self.invalid());
        }
        let length = self.length(values.len() - range.len())?;
        self.0.try_resize_copy(length, 0)?;
        let resources = self.0.resources().clone();
        let destination = range
            .start
            .checked_add(values.len())
            .ok_or_else(|| self.invalid())?;
        copy_within_with_control(
            self.0.as_mut_slice(),
            range.end..old_length,
            destination,
            &mut RequestControl(&resources),
        )?;
        // Escapes are fixed 4/6-byte ASCII sequences from the kernel.
        resources.checkpoint(values.len() as u64)?;
        self.0.as_mut_slice()[range.start..destination].copy_from_slice(values);
        resources.checkpoint(0)
    }
    fn append_existing(&mut self, range: Range<usize>) -> Result<(), Error> {
        let old_length = self.0.len();
        if range.start > range.end || range.end > old_length {
            return Err(self.invalid());
        }
        let length = self.length(range.len())?;
        self.0.try_resize_copy(length, 0)?;
        let resources = self.0.resources().clone();
        copy_within_with_control(
            self.0.as_mut_slice(),
            range,
            old_length,
            &mut RequestControl(&resources),
        )
    }
    fn format(&mut self, args: fmt::Arguments<'_>) -> Result<(), Error> {
        struct Sink<'a>(&'a mut PaidBytes);
        impl fmt::Write for Sink<'_> {
            fn write_str(&mut self, text: &str) -> fmt::Result {
                self.0.extend(text.as_bytes()).map_err(|_| fmt::Error)
            }
        }
        self.0.resources().checkpoint(0)?;
        let formatted = fmt::write(&mut Sink(self), args);
        if formatted.is_err() {
            return Err(self.invalid());
        }
        self.0.resources().checkpoint(0)
    }
}

impl Depths for PaidDepths {
    type Error = Error;
    fn len(&self) -> usize {
        self.0.len()
    }
    fn push(&mut self, value: DepthData) -> Result<(), Error> {
        self.0
            .resources()
            .checkpoint(std::mem::size_of::<DepthData>() as u64)?;
        self.0.try_push(value)
    }
    fn pop(&mut self) -> Option<DepthData> {
        self.0.pop()
    }
    fn last_mut(&mut self) -> Option<&mut DepthData> {
        self.0.as_mut_slice().last_mut()
    }
}
