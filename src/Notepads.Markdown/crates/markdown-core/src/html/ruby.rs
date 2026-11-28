// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded grammar shared by raw HTML and inline Markdown ruby projection.

use std::ops::Range;

pub(crate) const MAX_SOURCE_BYTES: usize = 4096;
const MAX_TEXT_BYTES: usize = 384;
const MAX_UNITS: usize = 32;

pub(super) struct Unit {
    pub base: Range<usize>,
    pub annotation: Range<usize>,
}

pub(super) struct Ruby {
    pub closing: Range<usize>,
    pub units: Vec<Unit>,
}

fn tag(bytes: &[u8], start: usize, prefix: &[u8]) -> Option<usize> {
    let after = start.checked_add(prefix.len())?;
    if !bytes.get(start..after)?.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let mut end = after;
    while matches!(bytes.get(end), Some(b' ' | b'\t')) && end - after < 16 {
        end += 1;
    }
    (bytes.get(end) == Some(&b'>')).then_some(end + 1)
}

fn text(source: &str, bytes: &[u8], start: usize) -> Option<Range<usize>> {
    let input = bytes.get(start..)?;
    let relative = input
        .iter()
        .take(MAX_TEXT_BYTES + 1)
        .position(|&ch| ch == b'<')?;
    let end = start + relative;
    let raw = source.get(start..end)?;
    let text = htmlize::unescape(raw);
    if raw.contains('\\')
        || text.is_empty()
        || text.trim() != text
        || text
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}' | '\u{fffc}'))
    {
        return None;
    }
    Some(start..end)
}

fn fallback(source: &str, bytes: &[u8], start: usize) -> Option<usize> {
    let content = tag(bytes, start, b"<rp")?;
    let body = text(source, bytes, content)?;
    tag(bytes, body.end, b"</rp")
}

pub(super) fn parse_at(source: &str, start: usize) -> Option<Ruby> {
    let end = start.checked_add(MAX_SOURCE_BYTES)?.min(source.len());
    let bytes = source.as_bytes().get(..end)?;
    let mut next = tag(bytes, start, b"<ruby")?;
    let mut units = Vec::new();
    loop {
        let base = text(source, bytes, next)?;
        let mut at = base.end;
        let leading_fallback = tag(bytes, at, b"<rp").is_some();
        if leading_fallback {
            at = fallback(source, bytes, at)?;
        }
        let annotation = text(source, bytes, tag(bytes, at, b"<rt")?)?;
        next = tag(bytes, annotation.end, b"</rt")?;
        if leading_fallback {
            next = fallback(source, bytes, next)?;
        }
        if units.len() >= MAX_UNITS {
            return None;
        }
        units.push(Unit { base, annotation });
        if let Some(end) = tag(bytes, next, b"</ruby") {
            return Some(Ruby {
                closing: next..end,
                units,
            });
        }
    }
}

pub(crate) fn marker(window: &[u8]) -> bool {
    window
        .windows(5)
        .any(|name| name.eq_ignore_ascii_case(b"<ruby"))
}

pub(crate) fn is_opening(raw: &[u8]) -> bool {
    tag(raw, 0, b"<ruby") == Some(raw.len())
}

pub(crate) fn closing_at(source: &str, start: usize) -> Option<Range<usize>> {
    parse_at(source, start).map(|ruby| ruby.closing)
}

// Offsets refer to the rendered fragment until the model binds them to Main.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayUnit {
    pub start: u64,
    pub base_end: u64,
    pub end: u64,
}

impl DisplayUnit {
    pub(super) fn offset(self, amount: u64) -> Self {
        Self {
            start: self.start + amount,
            base_end: self.base_end + amount,
            end: self.end + amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_ruby_ranges_support_repeated_units_and_balanced_fallback() {
        let input = "先😀 <RUBY>漢<rp>(</rp><rt>かん</rt><rp>)</rp>字<RT >じ</RT></RUBY> 後";
        let start = input.find('<').unwrap();
        let ruby = parse_at(input, start).unwrap();
        assert_eq!(&input[ruby.closing], "</RUBY>");
        assert_eq!(ruby.units.len(), 2);
        assert_eq!(&input[ruby.units[0].base.clone()], "漢");
        assert_eq!(&input[ruby.units[0].annotation.clone()], "かん");
        assert_eq!(&input[ruby.units[1].base.clone()], "字");
        assert_eq!(&input[ruby.units[1].annotation.clone()], "じ");
    }

    #[test]
    fn incomplete_or_unbounded_ruby_has_no_partial_units() {
        for source in [
            "<ruby><rt>reading</rt></ruby>",
            "<ruby>base<rt> </rt></ruby>",
            "<ruby>base<rt>annotation</rt><rp>)</rp></ruby>",
            "<ruby>base<rp>(</rp><rt>annotation</rt></ruby>",
            "<ruby>base<rt><b>annotation</b></rt></ruby>",
            "<ruby class=other>base<rt>annotation</rt></ruby>",
            "<ruby>base<rt>annotation</rt>tail</ruby>",
            "<ruby>base\r\n<rt>annotation</rt></ruby>",
            "<ruby>base<rt>a&#8232;b</rt></ruby>",
            "<ruby>b&#xfffc;a<rt>annotation</rt></ruby>",
            "<ruby>base<rt>annotation</rt>",
        ] {
            assert!(parse_at(source, 0).is_none(), "{source:?}");
        }
        let body = "字<rt>じ</rt>".repeat(MAX_UNITS);
        assert!(parse_at(&format!("<ruby>{body}</ruby>"), 0).is_some());
        assert!(parse_at(&format!("<ruby>{body}字<rt>じ</rt></ruby>"), 0).is_none());
        let long = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(parse_at(&format!("<ruby>{long}<rt>long</rt></ruby>"), 0).is_none());
    }
}
