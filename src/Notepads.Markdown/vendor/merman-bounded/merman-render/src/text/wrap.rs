//! Text wrapping helpers used across diagrams.
//!
//! This module intentionally mirrors Mermaid behavior (including quirks) for parity.

use super::{
    DeterministicTextMeasurer, TextMeasurer, TextStyle, trim_end_html_collapsible_ascii_whitespace,
    trim_html_collapsible_ascii_whitespace,
};
use unicode_segmentation::UnicodeSegmentation;

pub fn ceil_to_1_64_px(v: f64) -> f64 {
    if !(v.is_finite() && v >= 0.0) {
        return 0.0;
    }
    // Avoid "ceil to next 1/64" due to tiny FP drift (e.g. `...0000000002` over the exact
    // lattice). Upstream Mermaid fixtures frequently land exactly on the 1/64px grid.
    let x = v * 64.0;
    let r = x.round();
    if (x - r).abs() < 1e-4 {
        return r / 64.0;
    }
    ((x) - 1e-5).ceil() / 64.0
}

pub fn round_to_1_64_px(v: f64) -> f64 {
    if !(v.is_finite() && v >= 0.0) {
        return 0.0;
    }
    let x = v * 64.0;
    let r = (x + 0.5).floor();
    r / 64.0
}

pub fn round_to_1_64_px_ties_to_even(v: f64) -> f64 {
    if !(v.is_finite() && v >= 0.0) {
        return 0.0;
    }
    let x = v * 64.0;
    let f = x.floor();
    let frac = x - f;
    let i = if frac < 0.5 {
        f
    } else if frac > 0.5 {
        f + 1.0
    } else {
        let fi = f as i64;
        if fi % 2 == 0 { f } else { f + 1.0 }
    };
    let out = i / 64.0;
    if out == -0.0 { 0.0 } else { out }
}

/// Splits at a grapheme boundary using complete candidates and their newly appended graphemes.
pub(super) fn split_graphemes_to_width<E>(
    token: &str,
    max_width_px: f64,
    mut measure_candidate: impl FnMut(&str, &str) -> Result<f64, E>,
) -> Result<(&str, &str), E> {
    let mut split_at = 0;
    let mut width_px = 0.0;
    let mut has_positive_advance = false;

    for (index, grapheme) in token.grapheme_indices(true) {
        let candidate_end = index + grapheme.len();
        let candidate_width_px = measure_candidate(&token[..candidate_end], grapheme)?;
        if has_positive_advance
            && candidate_width_px > width_px
            && candidate_width_px > max_width_px
        {
            break;
        }

        split_at = candidate_end;
        width_px = candidate_width_px;
        has_positive_advance |= candidate_width_px > 0.0;
    }

    // A visible grapheme wider than the whole line must still make progress. Leading
    // zero-width graphemes stay attached to it instead of becoming an orphan line.
    if split_at == 0 {
        split_at = token.graphemes(true).next().map_or(0, str::len);
    }
    Ok(token.split_at(split_at))
}

/// Wraps SVG-like text into lines using the provided [`TextMeasurer`] for width decisions.
///
/// This mirrors Mermaid's `wrapLabel(...)` behavior at a high level (greedy word wrapping), but
/// delegates width measurements to the active measurer so the operation-selected font backend and
/// SVG measurement phase determine wrapping breakpoints.
pub fn wrap_text_lines_measurer(
    text: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_width_px: Option<f64>,
) -> Vec<String> {
    fn wrap_line(
        line: &str,
        measurer: &dyn TextMeasurer,
        style: &TextStyle,
        max_width_px: f64,
    ) -> Vec<String> {
        use std::collections::VecDeque;

        if !max_width_px.is_finite() || max_width_px <= 0.0 {
            return vec![line.to_string()];
        }

        let mut tokens = VecDeque::from(DeterministicTextMeasurer::split_line_to_words(line));
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::new();

        while let Some(tok) = tokens.pop_front() {
            if cur.is_empty() && tok == " " {
                continue;
            }

            let candidate = format!("{cur}{tok}");
            if measurer
                .measure(
                    trim_end_html_collapsible_ascii_whitespace(&candidate),
                    style,
                )
                .width
                <= max_width_px
            {
                cur = candidate;
                continue;
            }

            if !trim_html_collapsible_ascii_whitespace(&cur).is_empty() {
                out.push(trim_end_html_collapsible_ascii_whitespace(&cur).to_string());
                cur.clear();
                tokens.push_front(tok);
                continue;
            }

            if tok == " " {
                continue;
            }

            let (head, tail) = match split_graphemes_to_width(&tok, max_width_px, |candidate, _| {
                Ok::<_, std::convert::Infallible>(measurer.measure(candidate, style).width)
            }) {
                Ok(parts) => parts,
                Err(error) => match error {},
            };
            out.push(head.to_string());
            if !tail.is_empty() {
                tokens.push_front(tail.to_string());
            }
        }

        if !trim_html_collapsible_ascii_whitespace(&cur).is_empty() {
            out.push(trim_end_html_collapsible_ascii_whitespace(&cur).to_string());
        }

        if out.is_empty() {
            vec!["".to_string()]
        } else {
            out
        }
    }

    let mut out: Vec<String> = Vec::new();
    for line in split_html_br_lines(text) {
        if let Some(w) = max_width_px {
            out.extend(wrap_line(line, measurer, style, w));
        } else {
            out.push(line.to_string());
        }
    }
    if out.is_empty() {
        vec!["".to_string()]
    } else {
        out
    }
}

/// Splits a Mermaid label into lines using Mermaid's `<br>`-style line breaks.
///
/// Mirrors Mermaid's `lineBreakRegex = /<br\\s*\\/?>/gi` behavior:
/// - allows ASCII whitespace between `br` and the optional `/` or `>`
/// - does not accept extra characters (e.g. `<br \\t/>` with a literal backslash)
pub fn split_html_br_lines(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i + 3 < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let b1 = b[i + 1];
        let b2 = b[i + 2];
        if !matches!(b1, b'b' | b'B') || !matches!(b2, b'r' | b'R') {
            i += 1;
            continue;
        }
        let mut j = i + 3;
        while j < b.len() && matches!(b[j], b' ' | b'\t' | b'\r' | b'\n') {
            j += 1;
        }
        if j < b.len() && b[j] == b'/' {
            j += 1;
        }
        if j < b.len() && b[j] == b'>' {
            parts.push(&text[start..i]);
            start = j + 1;
            i = start;
            continue;
        }
        i += 1;
    }
    parts.push(&text[start..]);
    parts
}

fn try_wrap_label_like_mermaid_lines<E>(
    label: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_width_px: f64,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<Vec<String>, E> {
    checkpoint()?;
    if label.is_empty() {
        return Ok(Vec::new());
    }
    if !max_width_px.is_finite() || max_width_px <= 0.0 {
        return Ok(vec![label.to_string()]);
    }

    // Mermaid short-circuits wrapping if the label already contains `<br>` breaks.
    let explicit_lines = split_html_br_lines(label);
    if explicit_lines.len() > 1 {
        let mut lines = Vec::with_capacity(explicit_lines.len());
        for line in explicit_lines {
            checkpoint()?;
            lines.push(line.to_string());
        }
        checkpoint()?;
        return Ok(lines);
    }
    fn w_px<E>(
        measurer: &dyn TextMeasurer,
        style: &TextStyle,
        s: &str,
        checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
    ) -> std::result::Result<f64, E> {
        // Upstream uses `calculateTextWidth(...)` which rounds the SVG bbox width.
        checkpoint()?;
        let width = measurer
            .measure_svg_simple_text_bbox_width_for_wrap_px(s, style)
            .round();
        checkpoint()?;
        Ok(width)
    }

    fn break_string_like_mermaid<E>(
        word: &str,
        max_width_px: f64,
        measurer: &dyn TextMeasurer,
        style: &TextStyle,
        checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
    ) -> std::result::Result<(Vec<String>, String), E> {
        let chars: Vec<char> = word.chars().collect();
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for (idx, ch) in chars.iter().enumerate() {
            checkpoint()?;
            let next_line = format!("{current}{ch}");
            let line_w = w_px(measurer, style, &next_line, checkpoint)?;
            if line_w >= max_width_px {
                let is_last = idx + 1 == chars.len();
                if is_last {
                    lines.push(next_line);
                } else {
                    lines.push(format!("{next_line}-"));
                }
                current.clear();
            } else {
                current = next_line;
            }
        }
        Ok((lines, current))
    }

    // Mermaid splits on ASCII spaces and drops empty chunks (collapsing multiple spaces).
    let words: Vec<&str> = label.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return Ok(vec![label.to_string()]);
    }

    let mut completed: Vec<String> = Vec::new();
    let mut next_line = String::new();
    for (idx, word) in words.iter().enumerate() {
        checkpoint()?;
        let word_len = w_px(measurer, style, &format!("{word} "), checkpoint)?;
        let next_len = w_px(measurer, style, &next_line, checkpoint)?;
        if word_len > max_width_px {
            let (hyphenated, remaining) =
                break_string_like_mermaid(word, max_width_px, measurer, style, checkpoint)?;
            completed.push(next_line.clone());
            completed.extend(hyphenated);
            next_line = remaining;
        } else if next_len + word_len >= max_width_px {
            completed.push(next_line.clone());
            next_line = (*word).to_string();
        } else if next_line.is_empty() {
            next_line = (*word).to_string();
        } else {
            next_line.push(' ');
            next_line.push_str(word);
        }

        let is_last = idx + 1 == words.len();
        if is_last {
            completed.push(next_line.clone());
        }
    }

    checkpoint()?;
    Ok(completed.into_iter().filter(|l| !l.is_empty()).collect())
}

/// Wraps a label using Mermaid's `wrapLabel(...)` logic, producing wrapped *lines*.
///
/// This is used by Sequence diagrams (Mermaid@11.x) when `wrap: true` is enabled and when actor
/// descriptions are marked `wrap: true` by the DB layer.
pub fn wrap_label_like_mermaid_lines(
    label: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_width_px: f64,
) -> Vec<String> {
    let mut checkpoint = || Ok::<(), std::convert::Infallible>(());
    match try_wrap_label_like_mermaid_lines(label, measurer, style, max_width_px, &mut checkpoint) {
        Ok(lines) => lines,
        Err(error) => match error {},
    }
}

pub(crate) fn wrap_label_like_mermaid_lines_with_checkpoint(
    label: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_width_px: f64,
    mut checkpoint: impl FnMut() -> crate::Result<()>,
) -> crate::Result<Vec<String>> {
    try_wrap_label_like_mermaid_lines(label, measurer, style, max_width_px, &mut checkpoint)
}

#[cfg(test)]
mod tests {
    use super::{split_graphemes_to_width, wrap_text_lines_measurer};
    use crate::text::{
        DeterministicTextMeasurer, TextMeasurer, TextStyle, WrapMode, try_measure_wrapped_text,
    };

    #[test]
    fn grapheme_splitting_keeps_non_increasing_overwide_candidates() {
        let mut widths = [0.0, 8.0, 8.0, 7.0, 9.0].into_iter();
        let parts = split_graphemes_to_width("\u{0301}Wabc", 1.0, |_, _| {
            Ok::<_, ()>(widths.next().unwrap())
        })
        .unwrap();
        assert_eq!(parts, ("\u{0301}Wab", "c"));
    }

    #[test]
    fn grapheme_splitting_stops_on_measurement_errors() {
        let mut probes = Vec::new();
        let result =
            split_graphemes_to_width("\u{0301}👩‍🔬WZ", 1.0, |candidate, grapheme| {
                probes.push((candidate.to_string(), grapheme.to_string()));
                match grapheme {
                    "\u{0301}" => Ok(0.0),
                    "👩‍🔬" => Ok(8.0),
                    _ => Err("canceled"),
                }
            });
        assert_eq!(result, Err("canceled"));
        assert_eq!(
            probes,
            [
                ("\u{0301}".to_string(), "\u{0301}".to_string()),
                ("\u{0301}👩‍🔬".to_string(), "👩‍🔬".to_string()),
                ("\u{0301}👩‍🔬W".to_string(), "W".to_string()),
            ],
        );
    }

    #[test]
    fn emitted_long_tokens_match_shared_grapheme_wrapping() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        for text in [
            "W\u{0301}W",
            "\u{0301}WW",
            "👩‍🔬👨‍🔬",
            "👍🏽👍🏻",
            "🇨🇳🇺🇸",
            "1️⃣2️⃣",
            "a\u{200b}b",
        ] {
            for limit in [1.0, 8.0, 16.0, 25.0] {
                let expected = try_measure_wrapped_text(
                    text,
                    &style,
                    Some(limit),
                    WrapMode::SvgLike,
                    19.0,
                    |candidate, style| Ok::<_, ()>(measurer.measure(candidate, style).width),
                )
                .unwrap();
                let emitted = wrap_text_lines_measurer(text, &measurer, &style, Some(limit));
                assert_eq!(emitted, expected.lines, "text={text:?}, limit={limit}");
                assert_eq!(emitted.concat(), text);
            }
        }
    }

    #[test]
    fn emitted_wrapping_keeps_leading_zero_width_text_with_overwide_grapheme() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        assert_eq!(
            wrap_text_lines_measurer("\u{0301}WW", &measurer, &style, Some(1.0)),
            ["\u{0301}W", "W"],
        );
        assert_eq!(
            wrap_text_lines_measurer("👩‍🔬👨‍🔬", &measurer, &style, Some(1.0)),
            ["👩‍🔬", "👨‍🔬"],
        );
    }
}
