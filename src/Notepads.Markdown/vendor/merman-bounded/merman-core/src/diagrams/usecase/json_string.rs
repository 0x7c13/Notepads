//! JSON string decoding through serde_json's lossless WTF-8 byte visitor.

use serde::Deserializer as _;
use serde::de::{self, Visitor};
use std::fmt::{self, Write as _};

fn with_bytes<T>(literal: &str, decode: impl FnOnce(&[u8]) -> T) -> Result<T, serde_json::Error> {
    struct BytesVisitor<F>(F);
    impl<'de, T, F: FnOnce(&[u8]) -> T> Visitor<'de> for BytesVisitor<F> {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON string")
        }
        fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<T, E> {
            Ok((self.0)(value))
        }
    }
    let mut deserializer = serde_json::Deserializer::from_str(literal);
    let value = deserializer.deserialize_bytes(BytesVisitor(decode))?;
    deserializer.end()?;
    Ok(value)
}

pub(super) fn code_units(literal: &str) -> Result<Vec<u16>, serde_json::Error> {
    with_bytes(literal, |bytes| {
        let mut units = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            let first = bytes[index];
            let (mut point, length) = match first {
                0..=0x7f => (u32::from(first), 1),
                0xc0..=0xdf => (u32::from(first & 0x1f), 2),
                0xe0..=0xef => (u32::from(first & 0x0f), 3),
                _ => (u32::from(first & 0x07), 4),
            };
            // serde_json validates escapes and supplies valid UTF-8 or WTF-8 here.
            for byte in &bytes[index + 1..index + length] {
                point = (point << 6) | u32::from(byte & 0x3f);
            }
            if point <= 0xffff {
                units.push(point as u16);
            } else {
                point -= 0x10000;
                units.push(0xd800 | (point >> 10) as u16);
                units.push(0xdc00 | (point & 0x3ff) as u16);
            }
            index += length;
        }
        units
    })
}

pub(super) fn canonical(literal: &str) -> Result<String, serde_json::Error> {
    let units = code_units(literal)?;
    let mut output = String::with_capacity(units.len().saturating_mul(6).saturating_add(2));
    output.push('"');
    for unit in units {
        let _ = write!(output, "\\u{unit:04x}");
    }
    output.push('"');
    Ok(output)
}

pub(super) fn token_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut end = start + 1;
    while end < bytes.len() {
        match bytes[end] {
            b'"' => return end + 1,
            b'\\' => end += 2,
            _ => end += 1,
        }
    }
    source.len()
}

/// Called only after serde_json has validated the complete JSON syntax.
pub(super) fn contains_unpaired(source: &str) -> Result<bool, serde_json::Error> {
    let mut offset = 0;
    while offset < source.len() {
        if source.as_bytes()[offset] == b'"' {
            let end = token_end(source, offset);
            if with_bytes(&source[offset..end], |bytes| {
                std::str::from_utf8(bytes).is_err()
            })? {
                return Ok(true);
            }
            offset = end;
        } else {
            offset += 1;
        }
    }
    Ok(false)
}
