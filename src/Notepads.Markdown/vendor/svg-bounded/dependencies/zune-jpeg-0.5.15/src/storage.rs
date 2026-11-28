// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use crate::errors::{DecodeErrors, FormatCode};
use alloc::string::String;
use zune_core::bytestream::storage;
pub(crate) use zune_core::bytestream::storage::{Buffer, Control};

pub(crate) fn resource(error: storage::Error) -> DecodeErrors {
    DecodeErrors::Resources(error)
}
pub(crate) fn step(control: &Control, work: usize) -> Result<(), DecodeErrors> {
    control.step(work as u64).map_err(resource)
}
/// Fixed small kernels at the audited call sites are at most 1024 units.
/// Poll separately at loop/tail boundaries so a full loop batch cannot hide
/// additional scalar/SIMD edge work before the next cancellation observation.
pub(crate) fn fixed(control: &Control, work: usize) -> Result<(), DecodeErrors> {
    step(control, 0)?;
    storage::consume_work(control, work).map_err(resource)
}
pub(crate) fn check(control: &Control) -> Result<(), DecodeErrors> {
    control
        .failure()
        .map_or(Ok(()), |error| Err(resource(error)))
}
/// Reject hard failures before applying an original ordinary-error salvage.
pub(crate) fn candidate(control: &Control, error: &DecodeErrors) -> Result<(), DecodeErrors> {
    if let Some(error) = error.resource_error() {
        return Err(resource(control.terminal(error)));
    }
    check(control)
}
pub(crate) fn mul(control: &Control, factors: &[usize]) -> Result<usize, DecodeErrors> {
    factors
        .iter()
        .try_fold(1usize, |value, factor| value.checked_mul(*factor))
        .ok_or_else(|| resource(storage::capacity_error(control)))
}
pub(crate) fn filled<T: Copy>(
    control: &Control,
    len: usize,
    value: T,
) -> Result<Buffer<T>, DecodeErrors> {
    let mut output = Buffer::new(control);
    output.try_resize(len, value).map_err(resource)?;
    Ok(output)
}
pub(crate) fn bytes(control: &Control, input: &[u8]) -> Result<Buffer<u8>, DecodeErrors> {
    let mut output = Buffer::new(control);
    output.try_extend_from_slice(input).map_err(resource)?;
    Ok(output)
}
pub(crate) fn copy<T: Copy>(
    control: &Control,
    output: &mut [T],
    input: &[T],
) -> Result<(), DecodeErrors> {
    if output.len() != input.len() {
        return Err(terminal(
            control,
            DecodeErrors::FormatStatic("Mismatched codec copy lengths"),
        ));
    }
    for (out, source) in output.chunks_mut(1024).zip(input.chunks(1024)) {
        step(control, 0)?;
        let work = mul(control, &[source.len(), core::mem::size_of::<T>().max(1)])?;
        storage::consume_work(control, work).map_err(resource)?;
        out.copy_from_slice(source);
    }
    Ok(())
}
pub(crate) fn fill<T: Copy>(
    control: &Control,
    output: &mut [T],
    value: T,
) -> Result<(), DecodeErrors> {
    for chunk in output.chunks_mut(1024) {
        step(control, 0)?;
        let work = mul(control, &[chunk.len(), core::mem::size_of::<T>().max(1)])?;
        storage::consume_work(control, work).map_err(resource)?;
        for out in chunk {
            *out = value;
        }
    }
    Ok(())
}
/// Only use at a definite terminal decision, before local owners are dropped.
pub(crate) fn terminal(control: &Control, error: DecodeErrors) -> DecodeErrors {
    if error.resource_error().is_none() {
        storage::terminal_decode(control);
    }
    error
}
/// Preserve ordinary candidate errors without allocating controlled diagnostics.
pub(crate) fn diagnostic(
    control: &Control,
    code: FormatCode,
    make: impl FnOnce() -> String,
) -> DecodeErrors {
    if storage::is_paid(control) {
        return DecodeErrors::Code(code);
    }
    match code {
        FormatCode::Format => DecodeErrors::Format(make()),
        FormatCode::Huffman => DecodeErrors::HuffmanDecode(make()),
        FormatCode::Dqt => DecodeErrors::DqtError(make()),
        FormatCode::Sos => DecodeErrors::SosError(make()),
        FormatCode::Sof => DecodeErrors::SofError(make()),
        FormatCode::Mcu => DecodeErrors::MCUError(make()),
    }
}

/// Poll bounded batches and consume the same request-wide prepaid work credit
/// for the items actually visited. This owns no allocation or resource account.
/// Only O(1) standard range/chunk/zip iterators are used; their next operation
/// must not hide a long scan, allocation, deep construction or callback.
pub(crate) struct Work<'a, I> {
    iter: I,
    control: &'a Control,
    unit: u64,
    credit: usize,
    failed: bool,
}
pub(crate) fn polled<I: IntoIterator>(
    iter: I,
    control: &Control,
    unit: u64,
) -> Work<'_, I::IntoIter> {
    Work {
        iter: iter.into_iter(),
        control,
        unit: unit.max(1),
        credit: 0,
        failed: false,
    }
}
impl<I: Iterator> Iterator for Work<'_, I> {
    type Item = Result<I::Item, DecodeErrors>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        if let Some(error) = self.control.failure() {
            self.failed = true;
            return Some(Err(resource(error)));
        }
        let item = self.iter.next()?;
        let unit = match <usize as core::convert::TryFrom<u64>>::try_from(self.unit) {
            Ok(value) if value <= 1024 => value,
            _ => {
                self.failed = true;
                return Some(Err(resource(storage::capacity_error(self.control))));
            }
        };
        if self.credit < unit {
            if let Err(error) = self.control.step(0) {
                self.failed = true;
                return Some(Err(resource(error)));
            }
            self.credit = 1024;
        }
        if let Err(error) = storage::consume_work(self.control, unit) {
            self.failed = true;
            return Some(Err(resource(error)));
        }
        self.credit -= unit;
        Some(Ok(item))
    }
}
