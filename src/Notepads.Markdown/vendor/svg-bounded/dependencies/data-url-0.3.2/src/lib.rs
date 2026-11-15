// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Processing of `data:` URLs according to the Fetch Standard:
//! <https://fetch.spec.whatwg.org/#data-urls>
//! but starting from a string rather than a parsed URL to avoid extra copies.
//!
//! ```rust
//! use data_url::{DataUrl, mime};
//!
//! let url = DataUrl::process("data:,Hello%20World!").unwrap();
//! let (body, fragment) = url.decode_to_vec().unwrap();
//!
//! assert!(url.mime_type().matches("text", "plain"));
//! assert_eq!(url.mime_type().get_parameter("charset"), Some("US-ASCII"));
//! assert_eq!(body, b"Hello World!");
//! assert!(fragment.is_none());
//! ```
#![no_std]

// For forwards compatibility
#[cfg(feature = "std")]
extern crate std;

#[macro_use]
extern crate alloc;

#[cfg(not(feature = "alloc"))]
compile_error!("the `alloc` feature must be enabled");

use alloc::{string::String, vec::Vec};
use core::fmt;

pub mod forgiving_base64;
pub mod mime;
#[cfg(all(test, feature = "std", feature = "notepads-markdown-resources"))]
mod resource_tests;
mod storage;
use storage::{Builder, Fault, Plain, Storage, Text, Work};

#[cfg(feature = "notepads-markdown-resources")]
pub mod controlled;

pub struct DataUrl<'a> {
    mime_type: mime::Mime,
    base64: bool,
    encoded_body_plus_fragment: &'a str,
}

#[derive(Debug)]
pub enum DataUrlError {
    NotADataUrl,
    NoComma,
}

impl fmt::Display for DataUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADataUrl => write!(f, "not a valid data url"),
            Self::NoComma => write!(
                f,
                "data url is missing comma delimiting attributes and body"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DataUrlError {}

impl<'a> DataUrl<'a> {
    /// <https://fetch.spec.whatwg.org/#data-url-processor>
    /// but starting from a string rather than a parsed `Url`, to avoid extra string copies.
    pub fn process(input: &'a str) -> Result<Self, DataUrlError> {
        let parsed = process_with::<Plain>(input, &Work::plain()).map_err(|error| match error {
            ProcessError::Syntax(error) => error,
            ProcessError::Resource(_) => unreachable!("plain URL work cannot fail"),
        })?;
        Ok(DataUrl {
            mime_type: mime::Mime {
                type_: parsed.mime.type_,
                subtype: parsed.mime.subtype,
                parameters: parsed.mime.parameters,
            },
            base64: parsed.base64,
            encoded_body_plus_fragment: parsed.body,
        })
    }

    pub fn mime_type(&self) -> &mime::Mime {
        &self.mime_type
    }

    /// Streaming-decode the data URL’s body to `write_body_bytes`,
    /// and return the URL’s fragment identifier if it has one.
    pub fn decode<F, E>(
        &self,
        write_body_bytes: F,
    ) -> Result<Option<FragmentIdentifier<'a>>, forgiving_base64::DecodeError<E>>
    where
        F: FnMut(&[u8]) -> Result<(), E>,
    {
        if self.base64 {
            decode_with_base64(self.encoded_body_plus_fragment, write_body_bytes)
        } else {
            decode_without_base64(self.encoded_body_plus_fragment, write_body_bytes)
                .map_err(forgiving_base64::DecodeError::WriteError)
        }
    }

    /// Return the decoded body, and the URL’s fragment identifier if it has one.
    pub fn decode_to_vec(
        &self,
    ) -> Result<(Vec<u8>, Option<FragmentIdentifier<'a>>), forgiving_base64::InvalidBase64> {
        let mut body = Vec::new();
        let fragment = self.decode(|bytes| {
            body.extend_from_slice(bytes);
            Ok(())
        })?;
        Ok((body, fragment))
    }
}

/// The URL’s fragment identifier (after `#`)
pub struct FragmentIdentifier<'a>(&'a str);

impl FragmentIdentifier<'_> {
    /// Like in a parsed URL
    pub fn to_percent_encoded(&self) -> String {
        let mut string = Builder::<Plain>::new(&Work::plain());
        for byte in self.0.bytes() {
            match byte {
                // Ignore ASCII tabs or newlines like the URL parser would
                b'\t' | b'\n' | b'\r' => continue,
                // https://url.spec.whatwg.org/#fragment-percent-encode-set
                b'\0'..=b' ' | b'"' | b'<' | b'>' | b'`' | b'\x7F'..=b'\xFF' => {
                    percent_encode(byte, &mut string).unwrap_or_else(|_| unreachable!())
                }
                // Printable ASCII
                _ => string.push(byte as char).unwrap_or_else(|_| unreachable!()),
            }
        }
        string.finish().unwrap_or_else(|_| unreachable!())
    }
}

/// Similar to <https://url.spec.whatwg.org/#concept-basic-url-parser>
/// followed by <https://url.spec.whatwg.org/#concept-url-serializer>
///
/// * `None`: not a data URL.
///
/// * `Some(s)`: sort of the result of serialization, except:
///
///   - `data:` prefix removed
///   - The fragment is included
///   - Other components are **not** UTF-8 percent-encoded
///   - ASCII tabs and newlines in the middle are **not** removed
pub(crate) struct ParsedUrl<'a, S: Storage> {
    mime: mime::ParsedMime<S>,
    base64: bool,
    body: &'a str,
}
pub(crate) enum ProcessError {
    Syntax(DataUrlError),
    Resource(Fault),
}
impl From<Fault> for ProcessError {
    fn from(error: Fault) -> Self {
        Self::Resource(error)
    }
}
pub(crate) fn process_with<'a, S: Storage>(
    input: &'a str,
    work: &Work,
) -> Result<ParsedUrl<'a, S>, ProcessError> {
    let after_colon = pretend_parse_data_url(input, work)?
        .ok_or(ProcessError::Syntax(DataUrlError::NotADataUrl))?;
    let (header, body) = find_comma_before_fragment(after_colon, work)?
        .ok_or(ProcessError::Syntax(DataUrlError::NoComma))?;
    let (mime, base64) = parse_header::<S>(header, work)?;
    work.checked()?;
    Ok(ParsedUrl { mime, base64, body })
}

fn pretend_parse_data_url<'a>(input: &'a str, work: &Work) -> Result<Option<&'a str>, Fault> {
    // Trim C0 control or space
    let left_trimmed = storage::trim_start(input, work, |ch| ch <= b' ')?;

    let mut bytes = left_trimmed.bytes();
    {
        // Ignore ASCII tabs or newlines like the URL parser would
        for expected in b"data:" {
            let actual = loop {
                let Some(byte) = bytes.next() else {
                    return Ok(None);
                };
                work.charge(1)?;
                if !matches!(byte, b'\t' | b'\n' | b'\r') {
                    break byte;
                }
            };
            if !actual.eq_ignore_ascii_case(expected) {
                return Ok(None);
            }
        }
    }
    let bytes_consumed = left_trimmed.len() - bytes.len();
    let after_colon = &left_trimmed[bytes_consumed..];

    // Trim C0 control or space
    Ok(Some(storage::trim_end(after_colon, work, |ch| ch <= b' ')?))
}

fn find_comma_before_fragment<'a>(
    after_colon: &'a str,
    work: &Work,
) -> Result<Option<(&'a str, &'a str)>, Fault> {
    for (i, byte) in after_colon.bytes().enumerate() {
        work.charge(1)?;
        if byte == b',' {
            return Ok(Some((&after_colon[..i], &after_colon[i + 1..])));
        }
        if byte == b'#' {
            break;
        }
    }
    Ok(None)
}

fn parse_header<S: Storage>(
    from_colon_to_comma: &str,
    work: &Work,
) -> Result<(mime::ParsedMime<S>, bool), Fault> {
    // "Strip leading and trailing ASCII whitespace"
    //     \t, \n, and \r would have been filtered by the URL parser
    //     \f percent-encoded by the URL parser
    //     space is the only remaining ASCII whitespace
    let trimmed = storage::trim_start(from_colon_to_comma, work, |c| {
        matches!(c, b' ' | b'\t' | b'\n' | b'\r')
    })?;
    let trimmed = storage::trim_end(trimmed, work, |c| matches!(c, b' ' | b'\t' | b'\n' | b'\r'))?;

    let without_base64_suffix = remove_base64_suffix(trimmed, work)?;
    let base64 = without_base64_suffix.is_some();
    let mime_type = without_base64_suffix.unwrap_or(trimmed);

    let mut string = Builder::<S>::new(work);
    if mime_type.starts_with(';') {
        string.push_str("text/plain")?;
    }
    let mut in_query = false;
    for byte in mime_type.bytes() {
        work.charge(1)?;
        match byte {
            // Ignore ASCII tabs or newlines like the URL parser would
            b'\t' | b'\n' | b'\r' => continue,

            // https://url.spec.whatwg.org/#c0-control-percent-encode-set
            b'\0'..=b'\x1F' | b'\x7F'..=b'\xFF' => percent_encode(byte, &mut string)?,

            // Bytes other than the C0 percent-encode set that are percent-encoded
            // by the URL parser in the query state.
            // '#' is also in that list but cannot occur here
            // since it indicates the start of the URL’s fragment.
            b' ' | b'"' | b'<' | b'>' if in_query => percent_encode(byte, &mut string)?,

            b'?' => {
                in_query = true;
                string.push('?')?;
            }

            // Printable ASCII
            _ => string.push(byte as char)?,
        }
    }

    // FIXME: does Mime::from_str match the MIME Sniffing Standard’s parsing algorithm?
    // <https://mimesniff.spec.whatwg.org/#parse-a-mime-type>
    let string = string.finish()?;
    let mime_type = match mime::parse_with::<S>(string.as_str(), work)? {
        Some(value) => value,
        None => mime::fallback::<S>(work)?,
    };
    drop(string);
    work.checked()?;
    Ok((mime_type, base64))
}

/// None: no base64 suffix
#[allow(clippy::skip_while_next)]
fn remove_base64_suffix<'a>(s: &'a str, work: &Work) -> Result<Option<&'a str>, Fault> {
    let mut bytes = s.bytes();
    {
        // Ignore ASCII tabs or newlines like the URL parser would
        let mut next = || -> Result<Option<u8>, Fault> {
            while let Some(byte) = bytes.next_back() {
                work.charge(1)?;
                if !matches!(byte, b'\t' | b'\n' | b'\r') {
                    return Ok(Some(byte));
                }
            }
            Ok(None)
        };
        for expected in b"46esab" {
            let Some(byte) = next()? else {
                return Ok(None);
            };
            if !byte.eq_ignore_ascii_case(expected) {
                return Ok(None);
            }
        }
        loop {
            match next()? {
                Some(b' ') => continue,
                Some(b';') => break,
                _ => return Ok(None),
            }
        }
    }
    Ok(Some(&s[..bytes.len()]))
}

fn percent_encode<S: Storage>(byte: u8, string: &mut Builder<S>) -> Result<(), Fault> {
    const HEX_UPPER: [u8; 16] = *b"0123456789ABCDEF";
    string.push('%')?;
    string.push(HEX_UPPER[(byte >> 4) as usize] as char)?;
    string.push(HEX_UPPER[(byte & 0x0f) as usize] as char)
}

/// This is <https://url.spec.whatwg.org/#string-percent-decode> while also:
///
/// * Ignoring ASCII tab or newlines
/// * Stopping at the first '#' (which indicates the start of the fragment)
///
/// Anything that would have been UTF-8 percent-encoded by the URL parser
/// would be percent-decoded here.
/// We skip that round-trip and pass it through unchanged.
fn decode_without_base64<F, E>(
    encoded_body_plus_fragment: &str,
    write_bytes: F,
) -> Result<Option<FragmentIdentifier<'_>>, E>
where
    F: FnMut(&[u8]) -> Result<(), E>,
{
    decode_without_base64_with(
        encoded_body_plus_fragment,
        write_bytes,
        &Work::plain(),
        storage::plain_fault::<E>,
    )
}

fn decode_without_base64_with<'a, F, E, M>(
    encoded_body_plus_fragment: &'a str,
    mut write_bytes: F,
    work: &Work,
    map_fault: M,
) -> Result<Option<FragmentIdentifier<'a>>, E>
where
    F: FnMut(&[u8]) -> Result<(), E>,
    M: Fn(Fault) -> E,
{
    let bytes = encoded_body_plus_fragment.as_bytes();
    let mut slice_start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        work.charge(1).map_err(&map_fault)?;
        // We only need to look for 5 different "special" byte values.
        // For everything else we make slices as large as possible, borrowing the input,
        // in order to make fewer write_all() calls.
        if matches!(byte, b'%' | b'#' | b'\t' | b'\n' | b'\r') {
            // Write everything (if anything) "non-special" we’ve accumulated
            // before this special byte
            if i > slice_start {
                write_bytes(&bytes[slice_start..i])?;
                slice_start = i;
            }
            // Then deal with the special byte.
            match byte {
                b'%' => {
                    let l = bytes.get(i + 2).and_then(|&b| (b as char).to_digit(16));
                    let h = bytes.get(i + 1).and_then(|&b| (b as char).to_digit(16));
                    if let (Some(h), Some(l)) = (h, l) {
                        // '%' followed by two ASCII hex digits
                        let one_byte = h as u8 * 0x10 + l as u8;
                        write_bytes(&[one_byte])?;
                        slice_start = i + 3;
                    } else {
                        // Do nothing. Leave slice_start unchanged.
                        // The % sign will be part of the next slice.
                    }
                }

                b'#' => {
                    let fragment_start = i + 1;
                    let fragment = &encoded_body_plus_fragment[fragment_start..];
                    work.checked().map_err(&map_fault)?;
                    return Ok(Some(FragmentIdentifier(fragment)));
                }

                // Ignore over '\t' | '\n' | '\r'
                _ => slice_start = i + 1,
            }
        }
    }
    write_bytes(&bytes[slice_start..])?;
    work.checked().map_err(map_fault)?;
    Ok(None)
}

/// `decode_without_base64()` composed with
/// <https://infra.spec.whatwg.org/#isomorphic-decode> composed with
/// <https://infra.spec.whatwg.org/#forgiving-base64-decode>.
fn decode_with_base64<F, E>(
    encoded_body_plus_fragment: &str,
    write_bytes: F,
) -> Result<Option<FragmentIdentifier<'_>>, forgiving_base64::DecodeError<E>>
where
    F: FnMut(&[u8]) -> Result<(), E>,
{
    decode_with_base64_work(
        encoded_body_plus_fragment,
        write_bytes,
        &Work::plain(),
        Work::plain(),
        storage::plain_fault::<E>,
    )
}

fn decode_with_base64_work<'a, F, E>(
    encoded_body_plus_fragment: &'a str,
    write_bytes: F,
    work: &Work,
    base64_work: Work,
    map_fault: fn(Fault) -> E,
) -> Result<Option<FragmentIdentifier<'a>>, forgiving_base64::DecodeError<E>>
where
    F: FnMut(&[u8]) -> Result<(), E>,
{
    let mut decoder = forgiving_base64::Decoder::with_work(write_bytes, base64_work, map_fault);
    let fragment = decode_without_base64_with(
        encoded_body_plus_fragment,
        |bytes| decoder.feed(bytes),
        work,
        |error| forgiving_base64::DecodeError::WriteError(map_fault(error)),
    )?;
    decoder.finish()?;
    Ok(fragment)
}
