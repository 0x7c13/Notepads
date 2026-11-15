//! Heuristic text width estimation helpers.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn estimate_char_width_em(ch: char) -> f64 {
    if ch == '\u{00ad}' {
        // Keep the source control as a shaping boundary without an unselected glyph advance.
        return 0.0;
    }
    if matches!(ch, ' ' | '\u{00A0}') {
        return 0.33;
    }
    if ch == '\t' {
        return 0.66;
    }
    if ch == '_' || ch == '-' {
        return 0.33;
    }
    if matches!(ch, '.' | ',' | ':' | ';') {
        return 0.28;
    }
    if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | '/') {
        return 0.33;
    }
    if matches!(ch, '+' | '*' | '=' | '\\' | '^' | '|' | '~') {
        return 0.45;
    }
    if ch.is_ascii_digit() {
        return 0.56;
    }
    if ch.is_ascii_uppercase() {
        return match ch {
            'I' => 0.30,
            'W' => 0.85,
            _ => 0.60,
        };
    }
    if ch.is_ascii_lowercase() {
        return match ch {
            'i' | 'l' => 0.28,
            'm' | 'w' => 0.78,
            'k' | 'y' => 0.55,
            _ => 0.43,
        };
    }
    if !ch.is_ascii() {
        return match UnicodeWidthChar::width(ch) {
            Some(0) => 0.0,
            Some(2..) => 1.0,
            _ => 0.60,
        };
    }

    // Remaining ASCII punctuation and controls use a conservative single-column estimate.
    0.60
}

fn estimate_grapheme_width_em(grapheme: &str) -> f64 {
    let mut chars = grapheme.chars();
    let Some(first) = chars.next() else {
        return 0.0;
    };
    if chars.next().is_none() {
        return estimate_char_width_em(first);
    }

    // Preserve the proportional ASCII model for the only multi-scalar ASCII grapheme (CRLF).
    // Non-ASCII clusters use the sequence-aware width API so grapheme-local ZWJ emoji,
    // modifiers, flags, keycaps, and presentation selectors are not charged once per scalar.
    if grapheme.is_ascii() {
        return grapheme.chars().map(estimate_char_width_em).sum();
    }

    let sequence_columns = UnicodeWidthStr::width(grapheme);
    match sequence_columns {
        0 => 0.0,
        columns => {
            let mut visible = grapheme
                .chars()
                .filter(|ch| UnicodeWidthChar::width(*ch).is_some_and(|width| width > 0));
            let Some(base) = visible.next() else {
                return 0.0;
            };
            if visible.next().is_none() && UnicodeWidthChar::width(base) == Some(columns) {
                // Keep combining-mark sequences aligned with their single visible base. This
                // preserves the existing proportional ASCII estimates for decomposed text.
                estimate_char_width_em(base)
            } else if columns == 1 {
                0.60
            } else {
                columns as f64 * 0.5
            }
        }
    }
}

pub(crate) fn append_text_width_em(width_em: &mut f64, text: &str) {
    for grapheme in text.graphemes(true) {
        *width_em += estimate_grapheme_width_em(grapheme);
    }
}

pub(crate) fn estimate_text_width_em(text: &str) -> f64 {
    let mut width_em = 0.0;
    append_text_width_em(&mut width_em, text);
    width_em
}

#[cfg(test)]
mod tests {
    use super::estimate_text_width_em;

    fn estimate_line_width_px(line: &str, font_size: f64) -> f64 {
        estimate_text_width_em(line) * font_size
    }

    #[test]
    fn non_breaking_space_uses_the_regular_space_advance() {
        assert_eq!(
            estimate_line_width_px("A\u{00A0}B", 16.0),
            estimate_line_width_px("A B", 16.0),
        );
    }

    #[test]
    fn unicode_display_width_distinguishes_wide_and_zero_width_scalars() {
        assert_eq!(estimate_line_width_px("中文测试", 16.0), 64.0);
        assert_eq!(
            estimate_line_width_px("e\u{0301}", 16.0),
            estimate_line_width_px("e", 16.0),
        );
        assert_eq!(estimate_line_width_px("🙂", 16.0), 16.0);
        assert_eq!(estimate_line_width_px("é", 16.0), 9.6);
    }

    #[test]
    fn unicode_display_width_collapses_emoji_sequences_to_one_cluster() {
        for sequence in ["👩‍🔬", "👨‍👩‍👧‍👦", "👍🏽", "🇨🇳", "1️⃣"]
        {
            assert_eq!(
                estimate_line_width_px(sequence, 16.0),
                16.0,
                "sequence={sequence:?}"
            );
        }
    }

    #[test]
    fn unicode_presentation_selectors_adjust_the_sequence_display_width() {
        // U+26A0 defaults to text presentation, while VS16 selects its standardized emoji form.
        assert_eq!(estimate_line_width_px("⚠", 16.0), 9.6);
        assert_eq!(estimate_line_width_px("⚠️", 16.0), 16.0);

        // U+2648 defaults to emoji presentation, while VS15 selects its standardized text form.
        assert_eq!(estimate_line_width_px("♈", 16.0), 16.0);
        assert_eq!(estimate_line_width_px("♈\u{fe0e}", 16.0), 9.6);
    }
}
