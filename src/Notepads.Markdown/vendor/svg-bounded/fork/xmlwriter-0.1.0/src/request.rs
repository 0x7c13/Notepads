// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! A request-only facade: neither its writer nor mutable bytes can escape.
use crate::{
    engine::WriterCore,
    storage::{Bytes, Paid, PaidBytes},
    Options,
};
use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};
use std::fmt::{self, Display};

/// XML writer whose actual byte and depth allocations belong to one request.
/// Formatting implementations must do bounded work between writes and record
/// their own resource error before destroying any temporary paid owners.
pub struct RequestWriter {
    core: WriterCore<Paid>,
    resources: ResourcesRef,
    finished: bool,
}

impl RequestWriter {
    /// Create paid-empty buffers, without making a second request account.
    pub fn try_new(options: Options, resources: &ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        Ok(Self {
            core: WriterCore::new(
                Paid(resources.clone()),
                PaidBytes(BudgetVec::new(resources.clone(), ResourceKind::Scratch)),
                options,
            ),
            resources: resources.clone(),
            finished: false,
        })
    }
    fn check(&self) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        if self.finished {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        Ok(())
    }
    /// Write the original XML declaration.
    pub fn try_write_declaration(&mut self) -> Result<(), Error> {
        self.check()?;
        self.core.write_declaration()
    }
    /// Start an element using its original name.
    pub fn try_start_element(&mut self, name: &str) -> Result<(), Error> {
        self.check()?;
        self.core.start_element(name)
    }
    /// Write an attribute without creating an intermediate String.
    pub fn try_write_attribute<V: Display + ?Sized>(
        &mut self,
        name: &str,
        value: &V,
    ) -> Result<(), Error> {
        self.try_write_attribute_fmt(name, format_args!("{}", value))
    }
    /// Format directly into the paid buffer, then apply the original escaping.
    pub fn try_write_attribute_fmt(
        &mut self,
        name: &str,
        args: fmt::Arguments<'_>,
    ) -> Result<(), Error> {
        self.check()?;
        self.core.write_attribute_fmt(name, args)
    }
    /// Write a comment using the shared formatting kernel.
    pub fn try_write_comment(&mut self, text: &str) -> Result<(), Error> {
        self.try_write_comment_fmt(format_args!("{}", text))
    }
    /// Write a formatted comment without an intermediate String.
    pub fn try_write_comment_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<(), Error> {
        self.check()?;
        self.core.write_comment_fmt(args)
    }
    /// Select the original whitespace behavior.
    pub fn try_set_preserve_whitespaces(&mut self, preserve: bool) -> Result<(), Error> {
        self.check()?;
        self.core.set_preserve_whitespaces(preserve);
        self.resources.checkpoint(0)
    }
    /// Write escaped text using the shared kernel.
    pub fn try_write_text(&mut self, text: &str) -> Result<(), Error> {
        self.try_write_text_fmt(format_args!("{}", text))
    }
    /// Format escaped text directly into paid storage.
    pub fn try_write_text_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<(), Error> {
        self.check()?;
        self.core.write_text_fmt(args)
    }
    /// Close an element, retaining capacity and its real allocation fee.
    pub fn try_end_element(&mut self) -> Result<(), Error> {
        self.check()?;
        self.core.end_element()
    }
    /// Close all elements and borrow the final paid bytes. Repeated finish is
    /// read-only; later mutation is rejected. Consumers must retire this whole
    /// writer and check the original slot before publishing their next result.
    pub fn try_finish(&mut self) -> Result<&[u8], Error> {
        self.resources.checkpoint(0)?;
        if !self.finished {
            self.core.finish()?;
            self.finished = true;
        }
        self.resources.checkpoint(0)?;
        Ok(self.core.buf.as_slice())
    }
}
