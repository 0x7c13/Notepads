// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use std::ops::RangeInclusive;

const MAX_METADATA_BYTES: usize = 1024;
const MAX_HIGHLIGHT_RANGES: usize = 64;
const MAX_LINE_NUMBER: u32 = 1_000_000_000;

pub(crate) struct FenceInfo<'a> {
    pub language: &'a str,
    pub first_line_number: Option<u32>,
    highlighted: Option<&'a str>,
}

impl<'a> FenceInfo<'a> {
    pub fn parse(raw: &'a str) -> Self {
        let raw = raw.trim();
        let token_end = raw
            .find(|ch: char| ch.is_whitespace() || ch == '{')
            .unwrap_or(raw.len());
        let token = &raw[..token_end];
        let mut info = Self {
            language: token,
            first_line_number: None,
            highlighted: None,
        };
        if let Some((language, option)) = token.split_once(':') {
            match option.split_once('=') {
                Some(("line-numbers", start)) => {
                    info.language = language;
                    info.first_line_number = positive_number(start);
                }
                None if option == "line-numbers" => {
                    info.language = language;
                    info.first_line_number = Some(1);
                }
                None if option == "no-line-numbers" => info.language = language,
                _ => {}
            }
        }
        let remainder = raw[token_end..].trim();
        if remainder.len() > MAX_METADATA_BYTES {
            return info;
        }
        let Some(attributes) = remainder
            .strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
        else {
            return info;
        };
        if valid_highlights(attributes) {
            info.highlighted = Some(attributes);
            return info;
        }
        let mut attributes = attributes.trim_ascii();
        let mut numbered = false;
        let mut start_from = None;
        while !attributes.is_empty() {
            let end = attributes
                .find(|ch: char| ch.is_ascii_whitespace() || ch == '=')
                .unwrap_or(attributes.len());
            let key = &attributes[..end];
            attributes = attributes[end..].trim_ascii_start();
            if let Some(remainder) = attributes.strip_prefix('=') {
                let Some((value, tail)) = attribute_value(remainder.trim_ascii_start()) else {
                    break;
                };
                attributes = tail;
                match key {
                    "startFrom" => start_from = positive_number(value),
                    "linenums" => info.first_line_number = positive_number(value),
                    "hl_lines" if valid_highlights(value) => info.highlighted = Some(value),
                    _ => {}
                }
            } else if let Some(class) = key.strip_prefix('.') {
                match class {
                    "numberLines" | "number-lines" => numbered = true,
                    "lineAnchors" | "line-anchors" => {}
                    _ if info.language.is_empty() => info.language = class,
                    _ => {}
                }
            }
        }
        if numbered {
            info.first_line_number = Some(start_from.unwrap_or(1));
        }
        info
    }

    pub fn highlights(&self) -> impl Iterator<Item = RangeInclusive<u32>> + 'a {
        self.highlighted
            .into_iter()
            .flat_map(range_items)
            .filter_map(parse_range)
    }
}

fn attribute_value(input: &str) -> Option<(&str, &str)> {
    let first = input.chars().next()?;
    let (value, tail) = if first == '"' || first == '\'' {
        input[first.len_utf8()..].split_once(first)?
    } else {
        input.split_at(
            input
                .find(|ch: char| ch.is_ascii_whitespace())
                .unwrap_or(input.len()),
        )
    };
    Some((value, tail.trim_ascii_start()))
}

fn positive_number(raw: &str) -> Option<u32> {
    if raw.is_empty() || !raw.as_bytes().iter().all(u8::is_ascii_digit) {
        return None;
    }
    raw.parse()
        .ok()
        .filter(|n| (1..=MAX_LINE_NUMBER).contains(n))
}

fn parse_range(raw: &str) -> Option<RangeInclusive<u32>> {
    if let Some((start, end)) = raw.split_once('-') {
        let start = positive_number(start)?;
        let end = positive_number(end)?;
        return (start <= end).then_some(start..=end);
    }
    let line = positive_number(raw)?;
    Some(line..=line)
}

fn range_items(text: &str) -> impl Iterator<Item = &str> {
    text.split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|token| !token.is_empty())
}

fn valid_highlights(text: &str) -> bool {
    let mut count = 0;
    for token in range_items(text) {
        count += 1;
        if count > MAX_HIGHLIGHT_RANGES || parse_range(token).is_none() {
            return false;
        }
    }
    count != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_and_numbering_metadata_use_physical_highlight_ranges() {
        for (text, lang, first) in [
            (
                "{#code .rust .numberLines startFrom=\"12\" hl_lines=\"1 3-4\"}",
                "rust",
                12,
            ),
            ("python{hl_lines='1 3-4' linenums=\"12\"}", "python", 12),
            ("js:line-numbers=12 {1,3-4}", "js", 12),
        ] {
            let info = FenceInfo::parse(text);
            assert_eq!(info.language, lang);
            assert_eq!(info.first_line_number, Some(first));
            assert_eq!(info.highlights().collect::<Vec<_>>(), [1..=1, 3..=4]);
        }
    }

    #[test]
    fn ordinary_language_and_invalid_line_metadata_fall_back() {
        let standard = FenceInfo::parse("objc++ other info");
        assert_eq!(standard.language, "objc++");
        assert_eq!(standard.first_line_number, None);
        let incomplete = FenceInfo::parse("rust {hl_lines=\"1 2}");
        assert_eq!(incomplete.language, "rust");
        assert_eq!(incomplete.highlights().count(), 0);
        for invalid in ["0", "9-2", "4294967296", "1,z"] {
            assert!(!valid_highlights(invalid));
        }
        let line_numbers = FenceInfo::parse("rust:line-numbers");
        assert_eq!(line_numbers.first_line_number, Some(1));
    }
}
