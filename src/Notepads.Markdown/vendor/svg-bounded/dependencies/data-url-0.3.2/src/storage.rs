// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Allocation policy used by the original URL and MIME algorithms.
use alloc::{string::String, vec::Vec};
use core::cell::Cell;

#[cfg(feature = "notepads-markdown-resources")]
use markdown_svg_budget::{BudgetVec, Error, PaidUtf8, ResourceKind, ResourcesRef};

#[cfg(feature = "notepads-markdown-resources")]
pub(crate) type Fault = Error;
#[cfg(not(feature = "notepads-markdown-resources"))]
pub(crate) type Fault = core::convert::Infallible;

pub(crate) struct Work {
    #[cfg(feature = "notepads-markdown-resources")]
    resources: Option<ResourcesRef>,
    credit: Cell<usize>,
}
impl Work {
    pub(crate) const fn plain() -> Self {
        Self {
            #[cfg(feature = "notepads-markdown-resources")]
            resources: None,
            credit: Cell::new(0),
        }
    }
    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) fn paid(resources: &ResourcesRef) -> Self {
        Self {
            resources: Some(resources.clone()),
            credit: Cell::new(0),
        }
    }
    pub(crate) fn charge(&self, mut amount: usize) -> Result<(), Fault> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            if let Some(error) = resources.failure() {
                return Err(error);
            }
            while amount != 0 {
                if self.credit.get() == 0 {
                    resources.consume_scan_work(256)?;
                    self.credit.set(256);
                }
                let count = amount.min(self.credit.get());
                self.credit.set(self.credit.get() - count);
                amount -= count;
            }
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        let _ = (&self.credit, &mut amount);
        Ok(())
    }
    pub(crate) fn checked(&self) -> Result<(), Fault> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            resources.checkpoint(0)?;
        }
        Ok(())
    }
    pub(crate) fn invariant(&self) -> Fault {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            return resources.record(Error::new(
                markdown_svg_budget::FailureKind::Invariant,
                markdown_svg_budget::LimitKind::Capacity,
            ));
        }
        panic!("internal MIME index invariant")
    }
    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) fn resources(&self) -> &ResourcesRef {
        self.resources
            .as_ref()
            .expect("paid storage has request resources")
    }
}

pub(crate) trait Text {
    fn as_str(&self) -> &str;
    fn append(&mut self, text: &str) -> Result<(), Fault>;
}
impl Text for String {
    fn as_str(&self) -> &str {
        self.as_str()
    }
    fn append(&mut self, text: &str) -> Result<(), Fault> {
        self.push_str(text);
        Ok(())
    }
}
#[cfg(feature = "notepads-markdown-resources")]
impl Text for PaidUtf8 {
    fn as_str(&self) -> &str {
        self.as_str()
    }
    fn append(&mut self, text: &str) -> Result<(), Fault> {
        self.try_push_str(text)
    }
}

pub(crate) trait List<T> {
    fn as_slice(&self) -> &[T];
    fn as_mut_slice(&mut self) -> &mut [T];
    fn push(&mut self, value: T) -> Result<(), Fault>;
}
impl<T> List<T> for Vec<T> {
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
    fn as_mut_slice(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
    fn push(&mut self, value: T) -> Result<(), Fault> {
        self.push(value);
        Ok(())
    }
}
#[cfg(feature = "notepads-markdown-resources")]
impl<T> List<T> for BudgetVec<T> {
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
    fn as_mut_slice(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
    fn push(&mut self, value: T) -> Result<(), Fault> {
        self.try_push(value)
    }
}

pub(crate) trait Storage {
    type Text: Text;
    type List<T>: List<T>;
    fn text(work: &Work) -> Self::Text;
    fn list<T>(work: &Work) -> Self::List<T>;
}
pub(crate) enum Plain {}
impl Storage for Plain {
    type Text = String;
    type List<T> = Vec<T>;
    fn text(_: &Work) -> Self::Text {
        String::new()
    }
    fn list<T>(_: &Work) -> Self::List<T> {
        Vec::new()
    }
}
#[cfg(feature = "notepads-markdown-resources")]
pub(crate) enum Paid {}
#[cfg(feature = "notepads-markdown-resources")]
impl Storage for Paid {
    type Text = PaidUtf8;
    type List<T> = BudgetVec<T>;
    fn text(work: &Work) -> Self::Text {
        PaidUtf8::new(work.resources().clone(), ResourceKind::Scratch)
    }
    fn list<T>(work: &Work) -> Self::List<T> {
        BudgetVec::new(work.resources().clone(), ResourceKind::Scratch)
    }
}

/// Batches UTF8 construction so a character does not become a Native callback.
pub(crate) struct Builder<S: Storage> {
    text: S::Text,
    staging: [u8; 4096],
    length: usize,
}
impl<S: Storage> Builder<S> {
    pub(crate) fn new(work: &Work) -> Self {
        Self {
            text: S::text(work),
            staging: [0; 4096],
            length: 0,
        }
    }
    pub(crate) fn push(&mut self, ch: char) -> Result<(), Fault> {
        let mut encoded = [0; 4];
        self.push_str(ch.encode_utf8(&mut encoded))
    }
    pub(crate) fn push_str(&mut self, mut text: &str) -> Result<(), Fault> {
        while !text.is_empty() {
            let mut count = text.len().min(self.staging.len() - self.length);
            while !text.is_char_boundary(count) {
                count -= 1;
            }
            if count == 0 {
                self.flush()?;
                continue;
            }
            self.staging[self.length..self.length + count]
                .copy_from_slice(&text.as_bytes()[..count]);
            self.length += count;
            text = &text[count..];
            if self.length == self.staging.len() {
                self.flush()?;
            }
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), Fault> {
        if self.length != 0 {
            // Only str and encoded char prefixes enter staging; each flush is
            // at a UTF8 boundary. This checked borrow does not allocate.
            let text =
                core::str::from_utf8(&self.staging[..self.length]).expect("valid UTF8 staging");
            self.text.append(text)?;
            self.length = 0;
        }
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<S::Text, Fault> {
        self.flush()?;
        Ok(self.text)
    }
}

pub(crate) fn literal<S: Storage>(text: &str, work: &Work) -> Result<S::Text, Fault> {
    let mut builder = Builder::<S>::new(work);
    for chunk in text.as_bytes().chunks(64 * 1024) {
        work.charge(chunk.len())?;
    }
    builder.push_str(text)?;
    builder.finish()
}

pub(crate) fn trim_start<'a>(
    mut text: &'a str,
    work: &Work,
    matches: impl Fn(u8) -> bool,
) -> Result<&'a str, Fault> {
    while let Some(byte) = text.as_bytes().first() {
        work.charge(1)?;
        if !matches(*byte) {
            break;
        }
        text = &text[1..];
    }
    Ok(text)
}
pub(crate) fn trim_end<'a>(
    mut text: &'a str,
    work: &Work,
    matches: impl Fn(u8) -> bool,
) -> Result<&'a str, Fault> {
    while let Some(byte) = text.as_bytes().last() {
        work.charge(1)?;
        if !matches(*byte) {
            break;
        }
        text = &text[..text.len() - 1];
    }
    Ok(text)
}

pub(crate) fn split2<'a>(
    text: &'a str,
    separator: u8,
    work: &Work,
) -> Result<(&'a str, Option<&'a str>), Fault> {
    for (i, byte) in text.bytes().enumerate() {
        work.charge(1)?;
        if byte == separator {
            return Ok((&text[..i], Some(&text[i + 1..])));
        }
    }
    Ok((text, None))
}

pub(crate) fn plain_fault<E>(_: Fault) -> E {
    unreachable!("plain decoding has no resource backend")
}
