//! Shared terminal-safe normalization primitives.
//!
//! This module is public only so workspace crates can share one normalization owner. Applications
//! should use the facade or renderer entry points rather than depending on these internals.

use std::borrow::Cow;
use std::convert::Infallible;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const MAX_DIAGNOSTIC_GRAPHEMES: usize = 256;
const MAX_DIAGNOSTIC_INPUT_BYTES: usize = 16 * 1024;
const MAX_DIAGNOSTIC_NORMALIZED_BYTES: usize = MAX_DIAGNOSTIC_INPUT_BYTES * 10;
const MAX_DIAGNOSTIC_BYTES: usize = 4 * 1024;
const DIAGNOSTIC_ELLIPSIS: &str = "...";
const NORMALIZATION_FAILURE_PLACEHOLDER: &str = "[terminal text unavailable]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalTextSegmentKind<'a> {
    Grapheme(&'a str),
    VisibleEscape(char),
    LineBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalTextSegment<'a> {
    kind: TerminalTextSegmentKind<'a>,
    source_grapheme_bytes: usize,
}

impl<'a> TerminalTextSegment<'a> {
    pub const fn kind(self) -> TerminalTextSegmentKind<'a> {
        self.kind
    }

    pub const fn source_grapheme_bytes(self) -> usize {
        self.source_grapheme_bytes
    }
}

/// Normalizes untrusted authored text for terminal display without changing printable text.
#[must_use]
pub fn normalize_terminal_text(value: &str) -> Cow<'_, str> {
    normalize_terminal_text_with_capacity(value, usize::MAX, false)
}

/// Produces a bounded, terminal-safe human-readable diagnostic.
#[must_use]
pub fn normalize_terminal_diagnostic(value: &str) -> String {
    normalize_terminal_diagnostic_with_capacity(value, MAX_DIAGNOSTIC_BYTES)
}

pub fn try_normalize_terminal_line_text(
    value: &str,
) -> Result<Cow<'_, str>, TerminalTextNormalizationError> {
    try_normalize_terminal_text_with_options(value, usize::MAX, true)
}

fn normalize_terminal_text_with_capacity(
    value: &str,
    maximum_capacity: usize,
    escape_line_feed: bool,
) -> Cow<'_, str> {
    try_normalize_terminal_text_with_options(value, maximum_capacity, escape_line_feed)
        .unwrap_or(Cow::Borrowed(NORMALIZATION_FAILURE_PLACEHOLDER))
}

fn try_normalize_terminal_text_with_options(
    value: &str,
    maximum_capacity: usize,
    escape_line_feed: bool,
) -> Result<Cow<'_, str>, TerminalTextNormalizationError> {
    let plan = terminal_normalization_plan(value, escape_line_feed)
        .ok_or(TerminalTextNormalizationError)?;
    if !plan.changed {
        return Ok(Cow::Borrowed(value));
    }
    if plan.output_bytes > maximum_capacity {
        return Err(TerminalTextNormalizationError);
    }

    let mut output = String::new();
    output
        .try_reserve_exact(plan.output_bytes)
        .map_err(|_| TerminalTextNormalizationError)?;
    let normalized = visit_terminal_text_segments(value, |segment| {
        match segment.kind() {
            TerminalTextSegmentKind::Grapheme(grapheme) => output.push_str(grapheme),
            TerminalTextSegmentKind::VisibleEscape(ch) => push_visible_escape(&mut output, ch),
            TerminalTextSegmentKind::LineBreak if escape_line_feed => {
                push_visible_escape(&mut output, '\n')
            }
            TerminalTextSegmentKind::LineBreak => output.push('\n'),
        }
        Ok::<(), Infallible>(())
    });
    match normalized {
        Ok(()) => {
            debug_assert_eq!(output.len(), plan.output_bytes);
            Ok(Cow::Owned(output))
        }
        Err(never) => match never {},
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalTextNormalizationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalNormalizationPlan {
    output_bytes: usize,
    changed: bool,
}

fn terminal_normalization_plan(
    value: &str,
    escape_line_feed: bool,
) -> Option<TerminalNormalizationPlan> {
    let mut output_bytes = 0usize;
    let mut changed = false;
    let planned = visit_terminal_text_segments(value, |segment| {
        let segment_bytes = match segment.kind() {
            TerminalTextSegmentKind::Grapheme(grapheme) => grapheme.len(),
            TerminalTextSegmentKind::VisibleEscape(ch) => {
                changed = true;
                visible_escape_len(ch)
            }
            TerminalTextSegmentKind::LineBreak => {
                if escape_line_feed {
                    changed = true;
                    visible_escape_len('\n')
                } else {
                    changed |= segment.source_grapheme_bytes() != 1;
                    1
                }
            }
        };
        output_bytes = output_bytes.checked_add(segment_bytes).ok_or(())?;
        Ok::<(), ()>(())
    });
    planned.ok().map(|()| TerminalNormalizationPlan {
        output_bytes,
        changed,
    })
}

pub fn visit_terminal_text_segments<'a, E>(
    value: &'a str,
    mut visit: impl FnMut(TerminalTextSegment<'a>) -> Result<(), E>,
) -> Result<(), E> {
    for grapheme in value.graphemes(true) {
        let source_grapheme_bytes = grapheme.len();
        if grapheme == "\r\n" || grapheme == "\n" {
            visit(TerminalTextSegment {
                kind: TerminalTextSegmentKind::LineBreak,
                source_grapheme_bytes,
            })?;
            continue;
        }

        if grapheme.chars().any(scalar_requires_visible_escape) {
            for (offset, ch) in grapheme.char_indices() {
                let kind = if ch == '\n' {
                    TerminalTextSegmentKind::LineBreak
                } else if scalar_requires_visible_escape(ch)
                    || UnicodeWidthChar::width(ch).unwrap_or(0) == 0
                {
                    TerminalTextSegmentKind::VisibleEscape(ch)
                } else {
                    TerminalTextSegmentKind::Grapheme(&grapheme[offset..offset + ch.len_utf8()])
                };
                visit(TerminalTextSegment {
                    kind,
                    source_grapheme_bytes,
                })?;
            }
            continue;
        }

        if UnicodeWidthStr::width(grapheme) == 0 {
            for ch in grapheme.chars() {
                visit(TerminalTextSegment {
                    kind: TerminalTextSegmentKind::VisibleEscape(ch),
                    source_grapheme_bytes,
                })?;
            }
        } else {
            visit(TerminalTextSegment {
                kind: TerminalTextSegmentKind::Grapheme(grapheme),
                source_grapheme_bytes,
            })?;
        }
    }
    Ok(())
}

pub fn terminal_grapheme_requires_normalization(grapheme: &str) -> bool {
    grapheme == "\r\n"
        || (grapheme != "\n"
            && (grapheme.chars().any(scalar_requires_visible_escape)
                || UnicodeWidthStr::width(grapheme) == 0))
}

pub fn scalar_requires_visible_escape(ch: char) -> bool {
    ch == '\r'
        || (ch <= '\u{1f}' && ch != '\n')
        || ('\u{7f}'..='\u{9f}').contains(&ch)
        || is_bidi_control(ch)
}

fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'
            | '\u{202b}'
            | '\u{202c}'
            | '\u{202d}'
            | '\u{202e}'
            | '\u{2066}'
            | '\u{2067}'
            | '\u{2068}'
            | '\u{2069}'
    )
}

pub fn visible_escape_len(ch: char) -> usize {
    let mut digits = 1;
    let mut value = u32::from(ch);
    while value >= 16 {
        digits += 1;
        value /= 16;
    }
    4 + digits
}

pub fn visible_escape(ch: char, buffer: &mut [u8; 10]) -> &str {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let digits = visible_escape_len(ch) - 4;
    buffer[..3].copy_from_slice(b"\\u{");
    let mut value = u32::from(ch);
    for index in (0..digits).rev() {
        buffer[3 + index] = HEX[(value & 0x0f) as usize];
        value >>= 4;
    }
    buffer[3 + digits] = b'}';
    std::str::from_utf8(&buffer[..4 + digits]).expect("visible escapes contain only ASCII")
}

fn normalize_terminal_diagnostic_with_capacity(value: &str, maximum_capacity: usize) -> String {
    let (raw_prefix, input_truncated) = bounded_grapheme_prefix(value);
    let normalized = match try_normalize_terminal_text_with_options(
        raw_prefix,
        MAX_DIAGNOSTIC_NORMALIZED_BYTES,
        false,
    ) {
        Ok(normalized) => normalized,
        Err(_) => return owned_normalization_failure_placeholder(String::new()),
    };
    let content_byte_limit = MAX_DIAGNOSTIC_BYTES - DIAGNOSTIC_ELLIPSIS.len();
    let mut output = String::new();
    let mut truncated = input_truncated;
    let mut graphemes = 0usize;
    for grapheme in normalized.graphemes(true) {
        match append_diagnostic_grapheme(
            &mut output,
            &mut graphemes,
            grapheme,
            content_byte_limit,
            maximum_capacity,
        ) {
            Ok(()) => {}
            Err(DiagnosticAppendError::Truncated) => {
                truncated = true;
                break;
            }
            Err(DiagnosticAppendError::AllocationFailed) => {
                return owned_normalization_failure_placeholder(output);
            }
        }
    }
    if truncated {
        let Some(final_bytes) = output.len().checked_add(DIAGNOSTIC_ELLIPSIS.len()) else {
            return owned_normalization_failure_placeholder(output);
        };
        if final_bytes > maximum_capacity || output.try_reserve(DIAGNOSTIC_ELLIPSIS.len()).is_err()
        {
            return owned_normalization_failure_placeholder(output);
        }
        output.push_str(DIAGNOSTIC_ELLIPSIS);
    }
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagnosticAppendError {
    Truncated,
    AllocationFailed,
}

fn append_diagnostic_grapheme(
    output: &mut String,
    graphemes: &mut usize,
    grapheme: &str,
    content_byte_limit: usize,
    maximum_capacity: usize,
) -> Result<(), DiagnosticAppendError> {
    let next_bytes = output
        .len()
        .checked_add(grapheme.len())
        .ok_or(DiagnosticAppendError::AllocationFailed)?;
    if *graphemes == MAX_DIAGNOSTIC_GRAPHEMES || next_bytes > content_byte_limit {
        return Err(DiagnosticAppendError::Truncated);
    }
    if next_bytes > maximum_capacity || output.try_reserve(grapheme.len()).is_err() {
        return Err(DiagnosticAppendError::AllocationFailed);
    }
    output.push_str(grapheme);
    *graphemes = graphemes
        .checked_add(1)
        .ok_or(DiagnosticAppendError::AllocationFailed)?;
    Ok(())
}

fn bounded_grapheme_prefix(value: &str) -> (&str, bool) {
    if value.len() <= MAX_DIAGNOSTIC_INPUT_BYTES {
        return (value, false);
    }

    let mut byte_limit = MAX_DIAGNOSTIC_INPUT_BYTES;
    while !value.is_char_boundary(byte_limit) {
        byte_limit -= 1;
    }
    let probe_end = value[byte_limit..]
        .chars()
        .next()
        .map_or(byte_limit, |ch| byte_limit + ch.len_utf8());
    let probe = &value[..probe_end];
    let mut byte_end = 0usize;
    for (start, grapheme) in probe.grapheme_indices(true) {
        let grapheme_end = start + grapheme.len();
        if grapheme_end > byte_limit {
            break;
        }
        byte_end = grapheme_end;
    }
    (&value[..byte_end], true)
}

fn owned_normalization_failure_placeholder(mut output: String) -> String {
    output.clear();
    if output.capacity() >= NORMALIZATION_FAILURE_PLACEHOLDER.len()
        || output
            .try_reserve_exact(NORMALIZATION_FAILURE_PLACEHOLDER.len())
            .is_ok()
    {
        output.push_str(NORMALIZATION_FAILURE_PLACEHOLDER);
    }
    output
}

fn push_visible_escape(output: &mut String, ch: char) {
    let mut buffer = [0u8; 10];
    output.push_str(visible_escape(ch, &mut buffer));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_text_normalization_is_safe_borrowed_and_idempotent() {
        let printable = "Cafe\u{301} 👩‍💻 🇺🇸 中文";
        assert!(matches!(
            normalize_terminal_text(printable),
            Cow::Borrowed(_)
        ));

        let once = normalize_terminal_text("a\u{1b}\u{202e}\u{301}\r\nb").into_owned();
        assert_eq!(once, "a\\u{1B}\\u{202E}\\u{301}\nb");
        assert_eq!(normalize_terminal_text(&once), once);
    }

    #[test]
    fn single_line_normalization_exposes_line_feeds() {
        assert_eq!(
            try_normalize_terminal_line_text("one\ntwo")
                .expect("single-line normalization should succeed"),
            "one\\u{A}two"
        );
    }

    #[test]
    fn normalization_capacity_failure_returns_a_safe_static_placeholder() {
        let input = "\u{1b}".repeat(16);
        let normalized = normalize_terminal_text_with_capacity(
            &input,
            NORMALIZATION_FAILURE_PLACEHOLDER.len(),
            false,
        );
        assert!(matches!(normalized, Cow::Borrowed(_)));
        assert_eq!(normalized, NORMALIZATION_FAILURE_PLACEHOLDER);
        assert!(!normalized.contains('\u{1b}'));
        assert_eq!(normalize_terminal_text(&normalized), normalized);
    }

    #[test]
    fn c0_c1_escape_del_and_bidi_controls_are_exhaustively_visible() {
        let bidi = [
            '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
            '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
        ];
        let mut input = String::new();
        input.extend(['\0', '\u{1b}', '\u{7f}', '\u{80}', '\u{9f}']);
        input.extend(bidi);

        let normalized = normalize_terminal_text(&input);
        for control in input.chars() {
            assert!(
                !normalized.contains(control),
                "raw control {control:?} leaked"
            );
            assert!(
                normalized.contains(&format!("\\u{{{:X}}}", u32::from(control))),
                "missing visible escape for {control:?}: {normalized}"
            );
        }
    }

    #[test]
    fn zero_width_graphemes_are_visible_but_joined_graphemes_survive() {
        assert_eq!(
            normalize_terminal_text("\u{301}\u{200d}\u{fe0f}"),
            "\\u{301}\\u{200D}\\u{FE0F}"
        );
        for input in ["👩‍💻", "✈️", "a\u{200c}b", "👍🏽"] {
            assert_eq!(normalize_terminal_text(input), input);
        }
    }

    #[test]
    fn diagnostic_is_bounded_without_splitting_graphemes() {
        let input = "👩‍💻".repeat(MAX_DIAGNOSTIC_GRAPHEMES + 1);
        let normalized = normalize_terminal_diagnostic(&input);
        assert_eq!(
            normalized.graphemes(true).count(),
            MAX_DIAGNOSTIC_GRAPHEMES + DIAGNOSTIC_ELLIPSIS.graphemes(true).count()
        );
        assert!(normalized.ends_with(DIAGNOSTIC_ELLIPSIS));
    }

    #[test]
    fn diagnostic_bounds_one_oversized_grapheme_before_segmentation() {
        let input = format!("a{}b", "\u{301}".repeat(MAX_DIAGNOSTIC_INPUT_BYTES));
        assert_eq!(normalize_terminal_diagnostic(&input), DIAGNOSTIC_ELLIPSIS);
    }

    #[test]
    fn diagnostic_prefix_retains_a_grapheme_ending_at_the_input_limit() {
        let input = format!("{}b", "a".repeat(MAX_DIAGNOSTIC_INPUT_BYTES));
        let (prefix, truncated) = bounded_grapheme_prefix(&input);

        assert_eq!(prefix.len(), MAX_DIAGNOSTIC_INPUT_BYTES);
        assert!(truncated);
    }

    #[test]
    fn diagnostic_counts_graphemes_after_visible_escape_projection() {
        let input = format!("{}\u{1b}\u{0903}", "a".repeat(250));
        let normalized = normalize_terminal_diagnostic(&input);

        assert!(!normalized.ends_with(DIAGNOSTIC_ELLIPSIS));
        assert_eq!(normalized.graphemes(true).count(), MAX_DIAGNOSTIC_GRAPHEMES);
    }

    #[test]
    fn diagnostic_bounds_control_escape_expansion_by_bytes() {
        let input = "\u{1b}".repeat(MAX_DIAGNOSTIC_INPUT_BYTES * 2);
        let normalized = normalize_terminal_diagnostic(&input);

        assert!(normalized.len() <= MAX_DIAGNOSTIC_BYTES);
        assert!(normalized.ends_with(DIAGNOSTIC_ELLIPSIS));
        assert!(!normalized.contains('\u{1b}'));
    }
}
