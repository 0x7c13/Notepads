// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned storage around the original URL, MIME and body algorithms.
use crate::{
    forgiving_base64, mime,
    storage::{Paid, Work},
    DataUrlError, ParsedUrl, ProcessError,
};
use core::fmt;
use markdown_svg_budget::{
    BudgetVec, Error as ResourceError, FailureKind, LimitKind, PaidUtf8, ResourceKind, ResourcesRef,
};

/// Syntax is not itself a failed request: callers retain their original fallback
/// policy. A body error is returned while this URL and its paid header live, so
/// the caller can record semantic failure before dropping owned temporaries.
#[derive(Debug)]
pub enum Error {
    /// Original URL prefix/header-delimiter classification.
    Syntax(DataUrlError),
    /// Original forgiving-base64 body classification.
    InvalidBase64(forgiving_base64::InvalidBase64),
    /// Original request quota/cancellation/platform failure.
    Resource(ResourceError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(e) => e.fmt(f),
            Self::InvalidBase64(e) => e.fmt(f),
            Self::Resource(e) => e.fmt(f),
        }
    }
}
#[cfg(feature = "std")]
impl std::error::Error for Error {}
impl From<ResourceError> for Error {
    fn from(error: ResourceError) -> Self {
        Self::Resource(error)
    }
}

/// Borrowed complete parsed MIME fields, with no ordinary String/Vec escape.
#[derive(Clone, Copy)]
pub struct MimeView<'a> {
    mime: &'a mime::ParsedMime<Paid>,
}
impl<'a> MimeView<'a> {
    /// Parsed lowercase MIME type.
    pub fn type_(&self) -> &'a str {
        self.mime.type_.as_str()
    }
    /// Parsed lowercase MIME subtype.
    pub fn subtype(&self) -> &'a str {
        self.mime.subtype.as_str()
    }
    /// Original parameter order and first/duplicate handling.
    pub fn parameters(&self) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.mime
            .parameters
            .as_slice()
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
    /// Match the parsed essence. Engine callers compare fixed supported names.
    pub fn matches(&self, type_: &str, subtype: &str) -> bool {
        self.type_() == type_ && self.subtype() == subtype
    }
}

/// A standalone MIME parse using the same core and paid storage as URL headers.
/// Invalid MIME is a benign None, allowing the original header fallback policy.
pub struct Mime {
    parsed: mime::ParsedMime<Paid>,
    _resources: ResourcesRef,
}
impl fmt::Debug for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("controlled::Mime(..)")
    }
}
impl Mime {
    /// Parse the original MIME grammar without URL header normalization.
    pub fn parse(input: &str, resources: ResourcesRef) -> Result<Option<Self>, Error> {
        let work = Work::paid(&resources);
        work.checked()?;
        let parsed = mime::parse_with::<Paid>(input, &work)?;
        work.checked()?;
        Ok(parsed.map(|parsed| Self {
            parsed,
            _resources: resources,
        }))
    }
    /// Borrow complete parsed fields without converting their owners.
    pub fn view(&self) -> MimeView<'_> {
        MimeView { mime: &self.parsed }
    }
}

/// A fragment borrows the original immutable input; formatting is caller policy.
#[derive(Clone, Copy, Debug)]
pub struct Fragment<'a>(&'a str);
impl<'a> Fragment<'a> {
    /// Raw fragment with the original URL-parser normalization rules deferred.
    pub fn as_str(&self) -> &'a str {
        self.0
    }
}

/// MIME storage is owned and paid; body and fragment borrow immutable input.
pub struct DataUrl<'a> {
    parsed: ParsedUrl<'a, Paid>,
    resources: ResourcesRef,
}
impl fmt::Debug for DataUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("controlled::DataUrl(..)")
    }
}
impl<'a> DataUrl<'a> {
    /// Process using the original parser with paid storage and inner work gates.
    pub fn process(input: &'a str, resources: ResourcesRef) -> Result<Self, Error> {
        let work = Work::paid(&resources);
        work.checked()?;
        let parsed = crate::process_with::<Paid>(input, &work).map_err(|error| match error {
            ProcessError::Syntax(error) => Error::Syntax(error),
            ProcessError::Resource(error) => Error::Resource(error),
        })?;
        Ok(Self { parsed, resources })
    }
    /// Complete MIME fields, borrowing their paid owners.
    pub fn mime_type(&self) -> MimeView<'_> {
        MimeView {
            mime: &self.parsed.mime,
        }
    }

    /// Construct the same type/subtype essence accepted by legacy resolvers.
    pub fn mime_essence(&self) -> Result<PaidUtf8, Error> {
        let mut result = PaidUtf8::new(self.resources.clone(), ResourceKind::Scratch);
        result.try_push_str(self.parsed.mime.type_.as_str())?;
        result.try_push_str("/")?;
        result.try_push_str(self.parsed.mime.subtype.as_str())?;
        self.resources.checkpoint(0)?;
        Ok(result)
    }

    fn capacity_error(&self) -> Error {
        self.resources
            .record(ResourceError::new(FailureKind::Quota, LimitKind::Capacity))
            .into()
    }
    fn invariant_error(&self) -> Error {
        self.resources
            .record(ResourceError::new(
                FailureKind::Invariant,
                LimitKind::Capacity,
            ))
            .into()
    }

    fn decode<F>(&self, write: F) -> Result<Option<Fragment<'a>>, Error>
    where
        F: FnMut(&[u8]) -> Result<(), Error>,
    {
        let work = Work::paid(&self.resources);
        let fragment = if self.parsed.base64 {
            crate::decode_with_base64_work(
                self.parsed.body,
                write,
                &work,
                Work::paid(&self.resources),
                Error::Resource,
            )
            .map_err(|error| match error {
                forgiving_base64::DecodeError::InvalidBase64(e) => Error::InvalidBase64(e),
                forgiving_base64::DecodeError::WriteError(e) => e,
            })?
        } else {
            crate::decode_without_base64_with(self.parsed.body, write, &work, Error::Resource)?
        };
        Ok(fragment.map(|fragment| Fragment(fragment.0)))
    }

    /// First pass: validate the original body and count exact output bytes. No
    /// body owner is allocated and no child admission is consumed by this call.
    pub fn decoded_length(&self) -> Result<usize, Error> {
        let mut length = 0usize;
        self.decode(|bytes| {
            length = length
                .checked_add(bytes.len())
                .ok_or_else(|| self.capacity_error())?;
            Ok(())
        })?;
        self.resources.checkpoint(0)?;
        Ok(length)
    }

    /// Second pass into the caller's empty, already-funded exact-sized buffer.
    /// The caller admits the child between passes. No counters are reset here.
    pub fn decode_into(
        &self,
        expected_length: usize,
        output: &mut BudgetVec<u8>,
    ) -> Result<Option<Fragment<'a>>, Error> {
        if output.len() != 0
            || output.capacity() < expected_length
            || !output.belongs_to(&self.resources)
        {
            return Err(self.invariant_error());
        }
        let mut staging = [0u8; 4096];
        let mut used = 0usize;
        let mut written = 0usize;
        let copy_work = Work::paid(&self.resources);
        let fragment = self.decode(|mut bytes| {
            written = written
                .checked_add(bytes.len())
                .ok_or_else(|| self.capacity_error())?;
            if written > expected_length {
                return Err(self.invariant_error());
            }
            while !bytes.is_empty() {
                let count = bytes.len().min(staging.len() - used);
                copy_work.charge(count)?;
                staging[used..used + count].copy_from_slice(&bytes[..count]);
                used += count;
                bytes = &bytes[count..];
                if used == staging.len() {
                    output.try_extend_from_slice(&staging)?;
                    used = 0;
                }
            }
            Ok(())
        })?;
        if used != 0 {
            output.try_extend_from_slice(&staging[..used])?;
        }
        if written != expected_length || output.len() != expected_length {
            return Err(self.invariant_error());
        }
        self.resources.checkpoint(0)?;
        Ok(fragment)
    }
}
