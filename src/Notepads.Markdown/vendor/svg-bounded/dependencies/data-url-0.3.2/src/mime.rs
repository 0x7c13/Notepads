// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::storage::{self, Builder, Fault, List, Plain, Storage, Work};
#[path = "parameter_index.rs"]
mod parameter_index;
use alloc::{string::String, vec::Vec};
use core::fmt::{self, Write};
use core::str::FromStr;
use parameter_index::ParameterIndex;

/// <https://mimesniff.spec.whatwg.org/#mime-type-representation>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mime {
    pub type_: String,
    pub subtype: String,
    /// (name, value)
    pub parameters: Vec<(String, String)>,
}

impl Mime {
    /// Construct a new [`Mime`] with the given `type_` and `subtype` and an
    /// empty parameter list.
    pub fn new(type_: &str, subtype: &str) -> Self {
        Self {
            type_: type_.into(),
            subtype: subtype.into(),
            parameters: vec![],
        }
    }

    /// Return true if this [`Mime`] matches a given type and subtype, regardless
    /// of what parameters it has.
    pub fn matches(&self, type_: &str, subtype: &str) -> bool {
        self.type_ == type_ && self.subtype == subtype
    }

    pub fn get_parameter<P>(&self, name: &P) -> Option<&str>
    where
        P: ?Sized + PartialEq<str>,
    {
        self.parameters
            .iter()
            .find(|&(n, _)| name == &**n)
            .map(|(_, v)| &**v)
    }
}

#[derive(Debug)]
pub struct MimeParsingError(());

impl fmt::Display for MimeParsingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid mime type")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MimeParsingError {}

/// <https://mimesniff.spec.whatwg.org/#parsing-a-mime-type>
impl FromStr for Mime {
    type Err = MimeParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse(s).ok_or(MimeParsingError(()))
    }
}

fn parse(s: &str) -> Option<Mime> {
    let work = Work::plain();
    let record = parse_with::<Plain>(s, &work)
        .unwrap_or_else(|_| unreachable!("plain MIME work cannot fail"))?;
    Some(Mime {
        type_: record.type_,
        subtype: record.subtype,
        parameters: record.parameters,
    })
}

pub(crate) struct ParsedMime<S: Storage> {
    pub(crate) type_: S::Text,
    pub(crate) subtype: S::Text,
    pub(crate) parameters: S::List<(S::Text, S::Text)>,
}

pub(crate) fn parse_with<S: Storage>(s: &str, work: &Work) -> Result<Option<ParsedMime<S>>, Fault> {
    let trimmed = storage::trim_start(s, work, http_whitespace_byte)?;
    let trimmed = storage::trim_end(trimmed, work, http_whitespace_byte)?;
    let (type_, rest) = storage::split2(trimmed, b'/', work)?;
    if type_.is_empty() || !only_http_token_with(type_, work)? {
        return Ok(None);
    }
    let Some(rest) = rest else {
        return Ok(None);
    };
    let (subtype, rest) = storage::split2(rest, b';', work)?;
    let subtype = storage::trim_end(subtype, work, http_whitespace_byte)?;
    if subtype.is_empty() || !only_http_token_with(subtype, work)? {
        return Ok(None);
    }
    let mut parameters = S::list(work);
    if let Some(rest) = rest {
        parse_parameters::<S>(rest, &mut parameters, work)?;
    }
    let record = ParsedMime {
        type_: lowercase::<S>(type_, work)?,
        subtype: lowercase::<S>(subtype, work)?,
        parameters,
    };
    work.checked()?;
    Ok(Some(record))
}

pub(crate) fn fallback<S: Storage>(work: &Work) -> Result<ParsedMime<S>, Fault> {
    let mut parameters = S::list(work);
    parameters.push((
        storage::literal::<S>("charset", work)?,
        storage::literal::<S>("US-ASCII", work)?,
    ))?;
    Ok(ParsedMime {
        type_: storage::literal::<S>("text", work)?,
        subtype: storage::literal::<S>("plain", work)?,
        parameters,
    })
}

fn lowercase<S: Storage>(s: &str, work: &Work) -> Result<S::Text, Fault> {
    let mut result = Builder::<S>::new(work);
    // MIME token names are ASCII; lowercasing only ASCII preserves the original
    // to_ascii_lowercase behavior even when called for a rejected parameter.
    for ch in s.chars() {
        work.charge(ch.len_utf8())?;
        result.push(ch.to_ascii_lowercase())?;
    }
    result.finish()
}

struct Pieces<'a> {
    rest: Option<&'a str>,
}
impl<'a> Pieces<'a> {
    fn next(&mut self, work: &Work) -> Result<Option<&'a str>, Fault> {
        let Some(rest) = self.rest.take() else {
            return Ok(None);
        };
        let (piece, next) = storage::split2(rest, b';', work)?;
        self.rest = next;
        Ok(Some(piece))
    }
}

fn parse_parameters<S: Storage>(
    s: &str,
    parameters: &mut S::List<(S::Text, S::Text)>,
    work: &Work,
) -> Result<(), Fault> {
    let mut index = ParameterIndex::<S>::new(work);
    let mut semicolon_separated = Pieces { rest: Some(s) };

    while let Some(piece) = semicolon_separated.next(work)? {
        let piece = storage::trim_start(piece, work, http_whitespace_byte)?;
        let (name, value) = storage::split2(piece, b'=', work)?;
        // We can not early return on an invalid name here, because the value
        // parsing later may consume more semicolon seperated pieces.
        let name_valid = !name.is_empty()
            && only_http_token_with(name, work)?
            && !index.contains(name, parameters.as_slice(), work)?;
        if let Some(value) = value {
            let value = if let Some(stripped) = value.strip_prefix('"') {
                let mut unescaped_value = Builder::<S>::new(work);
                let mut chars = stripped.chars();
                'until_closing_quote: loop {
                    while let Some(c) = chars.next() {
                        work.charge(c.len_utf8())?;
                        match c {
                            '"' => break 'until_closing_quote,
                            '\\' => {
                                let escaped = if let Some(ch) = chars.next() {
                                    work.charge(ch.len_utf8())?;
                                    ch
                                } else if let Some(piece) = semicolon_separated.next(work)? {
                                    chars = piece.chars();
                                    ';'
                                } else {
                                    '\\'
                                };
                                unescaped_value.push(escaped)?;
                            }
                            _ => unescaped_value.push(c)?,
                        }
                    }
                    if let Some(piece) = semicolon_separated.next(work)? {
                        // A semicolon inside a quoted value is not a separator
                        // for the next parameter, but part of the value.
                        unescaped_value.push(';')?;
                        chars = piece.chars()
                    } else {
                        break;
                    }
                }
                if !name_valid || !valid_value_with(value, work)? {
                    continue;
                }
                unescaped_value.finish()?
            } else {
                let value = storage::trim_end(value, work, http_whitespace_byte)?;
                if value.is_empty() {
                    continue;
                }
                if !name_valid || !valid_value_with(value, work)? {
                    continue;
                }
                let mut builder = Builder::<S>::new(work);
                for part in value.as_bytes().chunks(64 * 1024) {
                    work.charge(part.len())?;
                }
                builder.push_str(value)?;
                builder.finish()?
            };
            parameters.push((lowercase::<S>(name, work)?, value))?;
            index.insert(parameters.as_slice().len() - 1, parameters.as_slice(), work)?;
        }
    }
    drop(index);
    work.checked()?;
    Ok(())
}

fn valid_value_with(s: &str, work: &Work) -> Result<bool, Fault> {
    for c in s.chars() {
        work.charge(c.len_utf8())?;
        // <https://mimesniff.spec.whatwg.org/#http-quoted-string-token-code-point>
        if !matches!(c, '\t' | ' '..='~' | '\u{80}'..='\u{FF}') {
            return Ok(false);
        }
    }
    Ok(true)
}

/// <https://mimesniff.spec.whatwg.org/#serializing-a-mime-type>
impl fmt::Display for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.type_)?;
        f.write_str("/")?;
        f.write_str(&self.subtype)?;
        for (name, value) in &self.parameters {
            f.write_str(";")?;
            f.write_str(name)?;
            f.write_str("=")?;
            if only_http_token_code_points(value) && !value.is_empty() {
                f.write_str(value)?
            } else {
                f.write_str("\"")?;
                for c in value.chars() {
                    if c == '"' || c == '\\' {
                        f.write_str("\\")?
                    }
                    f.write_char(c)?
                }
                f.write_str("\"")?
            }
        }
        Ok(())
    }
}

fn http_whitespace_byte(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}
fn only_http_token_with(s: &str, work: &Work) -> Result<bool, Fault> {
    for byte in s.bytes() {
        work.charge(1)?;
        if !IS_HTTP_TOKEN[byte as usize] {
            return Ok(false);
        }
    }
    Ok(true)
}

fn only_http_token_code_points(s: &str) -> bool {
    s.bytes().all(|byte| IS_HTTP_TOKEN[byte as usize])
}

macro_rules! byte_map {
    ($($flag:expr,)*) => ([
        $($flag != 0,)*
    ])
}

// Copied from https://github.com/hyperium/mime/blob/v0.3.5/src/parse.rs#L293
#[rustfmt::skip]
static IS_HTTP_TOKEN: [bool; 256] = byte_map![
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[test]
fn test_basic_mime() {
    let mime = Mime::new("text", "plain");
    assert!(mime.matches("text", "plain"));

    let cloned = mime.clone();
    assert!(cloned.matches("text", "plain"));

    let mime = Mime {
        type_: "text".into(),
        subtype: "html".into(),
        parameters: vec![("one".into(), "two".into())],
    };
    assert!(mime.matches("text", "html"));
}
