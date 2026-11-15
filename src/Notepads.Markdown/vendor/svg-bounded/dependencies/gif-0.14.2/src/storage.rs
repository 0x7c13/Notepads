// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private GIF policy reuses the LZW storage representation and same request.
pub(crate) use weezl::storage::{is_paid, Buffer, Control, Error};

pub(crate) fn resource(error: Error) -> crate::DecodingError {
    crate::DecodingError::Resources(error)
}
pub(crate) fn allocation(control: &Control, error: Error) -> crate::DecodingError {
    if is_paid(control) {
        resource(error)
    } else {
        crate::DecodingError::OutOfMemory
    }
}
pub(crate) fn check(control: &Control) -> Result<(), crate::DecodingError> {
    control
        .failure()
        .map_or(Ok(()), |error| Err(resource(error)))
}
pub(crate) fn step(control: &Control, work: usize) -> Result<(), crate::DecodingError> {
    control.step(0).map_err(resource)?;
    weezl::storage::consume_work(control, work).map_err(resource)
}
pub(crate) fn terminal(control: &Control, error: crate::DecodingError) -> crate::DecodingError {
    if error.resource_error().is_none() {
        weezl::storage::terminal_decode(control);
    }
    error
}
pub(crate) fn format(control: &Control, message: &'static str) -> crate::DecodingError {
    crate::DecodingError::format_with_control(control, message)
}
