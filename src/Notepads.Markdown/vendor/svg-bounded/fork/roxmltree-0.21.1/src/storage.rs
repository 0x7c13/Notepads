// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Main DOM storage. The upstream path stays available without a resource capability.

use crate::{Error, StringStorage};
use alloc::string::String;
use alloc::vec::{Drain, Vec};
use core::ops::{Deref, DerefMut, RangeFull};

#[cfg(feature = "notepads-markdown-resources")]
pub(crate) type Resources = Option<markdown_svg_budget::ResourcesRef>;
#[cfg(not(feature = "notepads-markdown-resources"))]
pub(crate) type Resources = ();

pub(crate) enum XmlVec<T> {
    Plain(Vec<T>),
    #[cfg(feature = "notepads-markdown-resources")]
    Paid(markdown_svg_budget::BudgetVec<T>),
}
impl<T> Default for XmlVec<T> {
    fn default() -> Self {
        Self::Plain(Vec::new())
    }
}
impl<T> XmlVec<T> {
    pub(crate) fn new(#[allow(unused)] resources: Resources) -> Self {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = resources {
            return Self::Paid(markdown_svg_budget::BudgetVec::new(
                resources,
                markdown_svg_budget::ResourceKind::Scratch,
            ));
        }
        Self::default()
    }
    pub(crate) fn push(&mut self, value: T) -> Result<(), Error> {
        match self {
            Self::Plain(values) => {
                values.push(value);
                Ok(())
            }
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => values.try_push(value).map_err(Error::Resource),
        }
    }
    pub(crate) fn insert(&mut self, at: usize, value: T) -> Result<(), Error>
    where
        T: Copy,
    {
        match self {
            Self::Plain(values) => {
                values.insert(at, value);
                Ok(())
            }
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => {
                let resources = values.resources().clone();
                let length = values.len();
                if at > length {
                    return Err(Error::Resource(resources.record(
                        markdown_svg_budget::Error::new(
                            markdown_svg_budget::FailureKind::Invariant,
                            markdown_svg_budget::LimitKind::Capacity,
                        ),
                    )));
                }
                values.try_reserve(1).map_err(Error::Resource)?;
                values.push_reserved(value);
                markdown_svg_budget::slice::copy_within_with_control(
                    values.as_mut_slice(),
                    at..length,
                    at + 1,
                    &mut markdown_svg_budget::RequestControl(&resources),
                )
                .map_err(Error::Resource)?;
                resources.checkpoint(1).map_err(Error::Resource)?;
                values[at] = value;
                resources.checkpoint(0).map_err(Error::Resource)
            }
        }
    }
    pub(crate) fn resources(&self) -> Resources {
        #[cfg(feature = "notepads-markdown-resources")]
        {
            match self {
                Self::Plain(_) => None,
                Self::Paid(values) => Some(values.resources().clone()),
            }
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        {
            ()
        }
    }
    pub(crate) fn pop(&mut self) -> Option<T> {
        match self {
            Self::Plain(values) => values.pop(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => values.pop(),
        }
    }
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Plain(values) => values.clear(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => values.clear(),
        }
    }
    pub(crate) fn drain(&mut self, range: RangeFull) -> Drain<'_, T> {
        match self {
            Self::Plain(values) => values.drain(range),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => values.drain(range),
        }
    }
    pub(crate) fn shrink_to_fit(&mut self) {
        // The controlled path keeps its admitted capacity until physical Drop.
        // Shrinking via an opaque realloc would invalidate the lease ordering.
        match self {
            Self::Plain(values) => values.shrink_to_fit(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(_) => {}
        }
    }
    pub(crate) fn release_storage(&mut self) {
        match self {
            Self::Plain(values) => *values = Vec::new(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => values.release_storage(),
        }
    }
}
impl<T> Deref for XmlVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Plain(values) => values,
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => values,
        }
    }
}
impl<T> DerefMut for XmlVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Plain(values) => values,
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(values) => values,
        }
    }
}
impl<'a, T> IntoIterator for &'a XmlVec<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// One owned UTF-8 builder; controlled storage never passes through String or
/// a second unpaired byte allocation when it becomes shared DOM storage.
pub(crate) enum XmlText {
    Plain(String),
    #[cfg(feature = "notepads-markdown-resources")]
    Paid(markdown_svg_budget::PaidUtf8),
}
impl XmlText {
    pub(crate) fn new(#[allow(unused)] resources: Resources) -> Self {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = resources {
            return Self::Paid(markdown_svg_budget::PaidUtf8::new(
                resources,
                markdown_svg_budget::ResourceKind::Scratch,
            ));
        }
        Self::Plain(String::new())
    }
    pub(crate) fn push_str(&mut self, text: &str) -> Result<(), Error> {
        match self {
            Self::Plain(value) => {
                value.push_str(text);
                Ok(())
            }
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(value) => value.try_push_str(text).map_err(Error::Resource),
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Plain(value) => value.is_empty(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(value) => value.is_empty(),
        }
    }
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Plain(value) => value.clear(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(value) => value.clear(),
        }
    }
    pub(crate) fn into_storage<'input>(self) -> Result<StringStorage<'input>, Error> {
        match self {
            Self::Plain(value) => Ok(StringStorage::Owned(value.into())),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(value) => value
                .into_shared()
                .map(StringStorage::Paid)
                .map_err(Error::Resource),
        }
    }
}

/// XML name comparison events; bytes use the same request's bounded scan credit.
pub(crate) struct NamespaceWork {
    resources: Resources,
    events: u16,
}
impl NamespaceWork {
    pub(crate) fn new(resources: Resources) -> Self {
        Self {
            resources,
            events: 0,
        }
    }
    pub(crate) fn event(&mut self) -> Result<(), Error> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            if let Some(error) = resources.failure() {
                return Err(Error::Resource(error));
            }
            if self.events == 0 {
                resources.checkpoint(0).map_err(Error::Resource)?;
                self.events = 256;
            }
            resources.consume_scan_work(1).map_err(Error::Resource)?;
            self.events -= 1;
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<(), Error> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            resources.checkpoint(0).map_err(Error::Resource)?;
        }
        Ok(())
    }
    pub(crate) fn name(
        &mut self,
        a: Option<&str>,
        b: Option<&str>,
    ) -> Result<core::cmp::Ordering, Error> {
        use core::cmp::Ordering::*;
        match (a, b) {
            (None, None) => Ok(Equal),
            (None, Some(_)) => Ok(Less),
            (Some(_), None) => Ok(Greater),
            (Some(a), Some(b)) => self.text(a, b),
        }
    }
    pub(crate) fn equal(&mut self, a: &str, b: &str) -> Result<bool, Error> {
        // Equality does not need to scan different lengths or the same borrowed bytes.
        if a.len() != b.len() {
            return Ok(false);
        }
        if core::ptr::eq(a.as_ptr(), b.as_ptr()) {
            return Ok(true);
        }
        self.text(a, b)
            .map(|order| order == core::cmp::Ordering::Equal)
    }
    pub(crate) fn text(&mut self, a: &str, b: &str) -> Result<core::cmp::Ordering, Error> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
            while !a.is_empty() && !b.is_empty() {
                let count = a.len().min(b.len()).min(4096);
                resources
                    .consume_scan_work(count)
                    .map_err(Error::Resource)?;
                let order = a[..count].cmp(&b[..count]);
                if order != core::cmp::Ordering::Equal {
                    return Ok(order);
                }
                a = &a[count..];
                b = &b[count..];
            }
            return Ok(a.len().cmp(&b.len()));
        }
        Ok(a.cmp(b))
    }
    pub(crate) fn namespace_limit(&self) -> Error {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            return Error::Resource(resources.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Quota,
                markdown_svg_budget::LimitKind::Namespaces,
            )));
        }
        Error::NamespacesLimitReached
    }
}
