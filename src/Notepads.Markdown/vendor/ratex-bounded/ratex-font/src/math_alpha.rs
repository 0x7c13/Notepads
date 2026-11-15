//! Mathematical Alphanumeric Symbols (U+1D400–U+1D7FF) — KaTeX `symbols.ts` wide tables.

use crate::FontId;

/// Maps a Unicode mathematical alphanumeric codepoint to the [`FontId`] and ASCII metric codepoint
/// used by bundled KaTeX font metrics (`fontMetricsData`) and **`.ttf` cmaps** (glyphs live at ASCII
/// letter/digit slots, not at the Unicode scalar).
pub fn font_and_metric_for_mathematical_alphanumeric(cp: u32) -> Option<(FontId, u32)> {
    // All math alphanumeric symbols are U+1D400–U+1D7FF (way above ASCII).
    // Early exit for ASCII saves 9+ range checks per glyph in the hot path.
    if cp <= 0x7F {
        return None;
    }
    const LETTERS52: u32 = 52;
    const BASES_LETTERS: &[(u32, FontId)] = &[
        (0x1D400, FontId::MainBold),       // bold
        (0x1D434, FontId::MathItalic),     // italic
        (0x1D468, FontId::MathBoldItalic), // bold italic
        (0x1D504, FontId::FrakturRegular), // Fraktur
        (0x1D56C, FontId::FrakturBold),    // bold Fraktur
        (0x1D5A0, FontId::SansSerifRegular),
        (0x1D5D4, FontId::SansSerifBold),
        (0x1D608, FontId::SansSerifItalic),
        (0x1D670, FontId::TypewriterRegular),
    ];
    for &(base, fid) in BASES_LETTERS {
        if cp >= base && cp < base + LETTERS52 {
            let i = cp - base;
            let metric = if i < 26 { 0x41 + i } else { 0x61 + (i - 26) };
            return Some((fid, metric));
        }
    }
    if (0x1D538..0x1D538 + 26).contains(&cp) {
        return Some((FontId::AmsRegular, 0x41 + (cp - 0x1D538)));
    }
    if (0x1D49C..0x1D49C + 26).contains(&cp) {
        return Some((FontId::ScriptRegular, 0x41 + (cp - 0x1D49C)));
    }
    if cp == 0x1D55C {
        return Some((FontId::AmsRegular, u32::from(b'k')));
    }
    for &(base, fid) in &[
        (0x1D7CE, FontId::MainBold),
        (0x1D7E2, FontId::SansSerifRegular),
        (0x1D7EC, FontId::SansSerifBold),
        (0x1D7F6, FontId::TypewriterRegular),
    ] {
        if cp >= base && cp < base + 10 {
            return Some((fid, 0x30 + (cp - base)));
        }
    }
    None
}

/// Character for `ab_glyph` / HarfBuzz cmap lookup in KaTeX `.ttf` files.
///
/// The display list keeps the real Unicode scalar in `char_code` (for web canvas / SVG `<text>`).
/// Outlines in shipped KaTeX fonts are keyed by ASCII letters and digits for these ranges.
pub fn katex_ttf_glyph_char(font_id: FontId, display_char_code: u32) -> char {
    if let Some((mapped_font, metric)) =
        font_and_metric_for_mathematical_alphanumeric(display_char_code)
    {
        if mapped_font == font_id {
            return char::from_u32(metric).unwrap_or('\u{fffd}');
        }
    }
    char::from_u32(display_char_code).unwrap_or('\u{fffd}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttf_cmap_maps_math_bold_a_to_ascii_a_on_main_bold() {
        // MATHEMATICAL BOLD CAPITAL A
        let cp = 0x1D400u32;
        assert_eq!(
            katex_ttf_glyph_char(FontId::MainBold, cp),
            'A',
            "KaTeX Main-Bold.ttf cmap uses 'A', not U+1D400"
        );
        assert_eq!(katex_ttf_glyph_char(FontId::MainRegular, cp), '\u{1D400}');
    }

    #[test]
    fn ttf_cmap_hyphen_unchanged() {
        assert_eq!(katex_ttf_glyph_char(FontId::MainBold, u32::from(b'-')), '-');
    }
}
