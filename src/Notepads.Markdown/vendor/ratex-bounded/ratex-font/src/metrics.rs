use crate::data::metrics_data::{self, MetricsEntry};
use crate::font_id::FontId;

/// Per-character font metrics, all values in em units.
#[derive(Debug, Clone, Copy)]
pub struct CharMetrics {
    pub depth: f64,
    pub height: f64,
    pub italic: f64,
    pub skew: f64,
    pub width: f64,
}

/// TeX math constants used by the layout engine, all in em units.
///
/// Values from Computer Modern / Latin Modern via KaTeX's `fontMetrics.ts`.
/// KaTeX provides three sets of values indexed by size:
///   [0] = textstyle (size >= 5, i.e. >= 9pt)
///   [1] = scriptstyle (size 3-4, i.e. 7-8pt)
///   [2] = scriptscriptstyle (size 1-2, i.e. 5-6pt)
#[derive(Debug, Clone, Copy)]
pub struct MathConstants {
    pub slant: f64,
    pub space: f64,
    pub stretch: f64,
    pub shrink: f64,
    pub x_height: f64,
    pub quad: f64,
    pub extra_space: f64,
    pub num1: f64,
    pub num2: f64,
    pub num3: f64,
    pub denom1: f64,
    pub denom2: f64,
    pub sup1: f64,
    pub sup2: f64,
    pub sup3: f64,
    pub sub1: f64,
    pub sub2: f64,
    pub sup_drop: f64,
    pub sub_drop: f64,
    pub delim1: f64,
    pub delim2: f64,
    pub axis_height: f64,
    pub default_rule_thickness: f64,
    pub big_op_spacing1: f64,
    pub big_op_spacing2: f64,
    pub big_op_spacing3: f64,
    pub big_op_spacing4: f64,
    pub big_op_spacing5: f64,
    pub sqrt_rule_thickness: f64,
    pub pt_per_em: f64,
    pub double_rule_sep: f64,
    pub array_rule_width: f64,
    pub fboxsep: f64,
    pub fboxrule: f64,
}

impl MathConstants {
    /// mu is 1/18 of a quad (em).
    pub fn css_em_per_mu(&self) -> f64 {
        self.quad / 18.0
    }
}

/// Three sets of TeX math constants, indexed by size (0=text, 1=script, 2=scriptscript).
/// Exact values from KaTeX's `fontMetrics.ts` sigmasAndXis table.
pub static MATH_CONSTANTS_BY_SIZE: [MathConstants; 3] = [
    // [0] textstyle (size index >= 5, >= 9pt) — from cmsy10
    MathConstants {
        slant: 0.250,
        space: 0.0,
        stretch: 0.0,
        shrink: 0.0,
        x_height: 0.431,
        quad: 1.0,
        extra_space: 0.0,
        num1: 0.677,
        num2: 0.394,
        num3: 0.444,
        denom1: 0.686,
        denom2: 0.345,
        sup1: 0.413,
        sup2: 0.363,
        sup3: 0.289,
        sub1: 0.150,
        sub2: 0.247,
        sup_drop: 0.386,
        sub_drop: 0.050,
        delim1: 2.390,
        delim2: 1.010,
        axis_height: 0.250,
        default_rule_thickness: 0.04,
        big_op_spacing1: 0.111,
        big_op_spacing2: 0.166,
        big_op_spacing3: 0.2,
        big_op_spacing4: 0.6,
        big_op_spacing5: 0.1,
        sqrt_rule_thickness: 0.04,
        pt_per_em: 10.0,
        double_rule_sep: 0.2,
        array_rule_width: 0.04,
        fboxsep: 0.3,
        fboxrule: 0.04,
    },
    // [1] scriptstyle (size index 3-4, 7-8pt) — from cmsy7
    MathConstants {
        slant: 0.250,
        space: 0.0,
        stretch: 0.0,
        shrink: 0.0,
        x_height: 0.431,
        quad: 1.171,
        extra_space: 0.0,
        num1: 0.732,
        num2: 0.384,
        num3: 0.471,
        denom1: 0.752,
        denom2: 0.344,
        sup1: 0.503,
        sup2: 0.431,
        sup3: 0.286,
        sub1: 0.143,
        sub2: 0.286,
        sup_drop: 0.353,
        sub_drop: 0.071,
        delim1: 1.700,
        delim2: 1.157,
        axis_height: 0.250,
        default_rule_thickness: 0.049,
        big_op_spacing1: 0.111,
        big_op_spacing2: 0.166,
        big_op_spacing3: 0.2,
        big_op_spacing4: 0.611,
        big_op_spacing5: 0.143,
        sqrt_rule_thickness: 0.04,
        pt_per_em: 10.0,
        double_rule_sep: 0.2,
        array_rule_width: 0.04,
        fboxsep: 0.3,
        fboxrule: 0.04,
    },
    // [2] scriptscriptstyle (size index 1-2, 5-6pt) — from cmsy5
    MathConstants {
        slant: 0.250,
        space: 0.0,
        stretch: 0.0,
        shrink: 0.0,
        x_height: 0.431,
        quad: 1.472,
        extra_space: 0.0,
        num1: 0.925,
        num2: 0.387,
        num3: 0.504,
        denom1: 1.025,
        denom2: 0.532,
        sup1: 0.504,
        sup2: 0.404,
        sup3: 0.294,
        sub1: 0.200,
        sub2: 0.400,
        sup_drop: 0.494,
        sub_drop: 0.100,
        delim1: 1.980,
        delim2: 1.420,
        axis_height: 0.250,
        default_rule_thickness: 0.049,
        big_op_spacing1: 0.111,
        big_op_spacing2: 0.166,
        big_op_spacing3: 0.2,
        big_op_spacing4: 0.611,
        big_op_spacing5: 0.143,
        sqrt_rule_thickness: 0.04,
        pt_per_em: 10.0,
        double_rule_sep: 0.2,
        array_rule_width: 0.04,
        fboxsep: 0.3,
        fboxrule: 0.04,
    },
];

/// Get math constants for a given size index (0=text, 1=script, 2=scriptscript).
pub fn get_global_metrics(size_index: usize) -> &'static MathConstants {
    &MATH_CONSTANTS_BY_SIZE[size_index.min(2)]
}

fn get_metrics_table(font_id: FontId) -> &'static [MetricsEntry] {
    match font_id {
        FontId::AmsRegular => metrics_data::AMS_REGULAR,
        FontId::CaligraphicRegular => metrics_data::CALIGRAPHIC_REGULAR,
        FontId::FrakturRegular => metrics_data::FRAKTUR_REGULAR,
        FontId::FrakturBold => metrics_data::FRAKTUR_BOLD,
        FontId::MainBold => metrics_data::MAIN_BOLD,
        FontId::MainBoldItalic => metrics_data::MAIN_BOLDITALIC,
        FontId::MainItalic => metrics_data::MAIN_ITALIC,
        FontId::MainRegular => metrics_data::MAIN_REGULAR,
        FontId::MathBoldItalic => metrics_data::MATH_BOLDITALIC,
        FontId::MathItalic => metrics_data::MATH_ITALIC,
        FontId::SansSerifBold => metrics_data::SANSSERIF_BOLD,
        FontId::SansSerifItalic => metrics_data::SANSSERIF_ITALIC,
        FontId::SansSerifRegular => metrics_data::SANSSERIF_REGULAR,
        FontId::ScriptRegular => metrics_data::SCRIPT_REGULAR,
        FontId::Size1Regular => metrics_data::SIZE1_REGULAR,
        FontId::Size2Regular => metrics_data::SIZE2_REGULAR,
        FontId::Size3Regular => metrics_data::SIZE3_REGULAR,
        FontId::Size4Regular => metrics_data::SIZE4_REGULAR,
        FontId::TypewriterRegular => metrics_data::TYPEWRITER_REGULAR,
        FontId::CjkRegular => &[],
        FontId::CjkFallback => &[],
        FontId::EmojiFallback => &[],
    }
}

/// Look up character metrics for a given font and character code.
///
/// Uses binary search on the sorted metrics table for O(log n) lookup.
pub fn get_char_metrics(font_id: FontId, char_code: u32) -> Option<CharMetrics> {
    let table = get_metrics_table(font_id);
    table
        .binary_search_by_key(&char_code, |entry| entry.0)
        .ok()
        .map(|idx| {
            let (_, depth, height, italic, skew, width) = table[idx];
            CharMetrics {
                depth,
                height,
                italic,
                skew,
                width,
            }
        })
}

/// Map characters without direct metrics to similar Latin characters.
/// Covers Latin-1 extended and Cyrillic, matching KaTeX's `extraCharacterMap`.
fn extra_character_fallback(ch: u32) -> Option<u32> {
    let c = char::from_u32(ch)?;
    let mapped = match c {
        'Å' => 'A',
        'Ð' => 'D',
        'Þ' => 'o',
        'å' => 'a',
        'ð' => 'd',
        'þ' => 'o',
        'А' => 'A',
        'Б' => 'B',
        'В' => 'B',
        'Г' => 'F',
        'Д' => 'A',
        'Е' => 'E',
        'Ж' => 'K',
        'З' => '3',
        'И' => 'N',
        'Й' => 'N',
        'К' => 'K',
        'Л' => 'N',
        'М' => 'M',
        'Н' => 'H',
        'О' => 'O',
        'П' => 'N',
        'Р' => 'P',
        'С' => 'C',
        'Т' => 'T',
        'У' => 'y',
        'Ф' => 'O',
        'Х' => 'X',
        'Ц' => 'U',
        'Ч' => 'h',
        'Ш' => 'W',
        'Щ' => 'W',
        'Ъ' => 'B',
        'Ы' => 'X',
        'Ь' => 'B',
        'Э' => '3',
        'Ю' => 'X',
        'Я' => 'R',
        'а' => 'a',
        'б' => 'b',
        'в' => 'a',
        'г' => 'r',
        'д' => 'y',
        'е' => 'e',
        'ж' => 'm',
        'з' => 'e',
        'и' => 'n',
        'й' => 'n',
        'к' => 'n',
        'л' => 'n',
        'м' => 'm',
        'н' => 'n',
        'о' => 'o',
        'п' => 'n',
        'р' => 'p',
        'с' => 'c',
        'т' => 'o',
        'у' => 'y',
        'ф' => 'b',
        'х' => 'x',
        'ц' => 'n',
        'ч' => 'n',
        'ш' => 'w',
        'щ' => 'w',
        'ъ' => 'a',
        'ы' => 'm',
        'ь' => 'a',
        'э' => 'e',
        'ю' => 'm',
        'я' => 'r',
        _ => return None,
    };
    Some(mapped as u32)
}

/// Look up character metrics with fallback for Latin-1/Cyrillic characters.
///
/// First tries direct lookup, then falls back to `extraCharacterMap` approximation.
pub fn get_char_metrics_with_fallback(font_id: FontId, char_code: u32) -> Option<CharMetrics> {
    get_char_metrics(font_id, char_code).or_else(|| {
        extra_character_fallback(char_code).and_then(|fallback| get_char_metrics(font_id, fallback))
    })
}

/// Look up character metrics with full KaTeX-compatible fallback chain.
///
/// 1. Direct metric lookup
/// 2. `extraCharacterMap` approximation (Latin-1 / Cyrillic)
/// 3. In text mode only: if the codepoint belongs to a supported Unicode script
///    (CJK, Brahmic, etc.), fall back to the metrics of Latin capital 'M'.
///    This matches KaTeX's behavior for Asian/complex scripts in `\text{}`.
pub fn get_char_metrics_for_mode(
    font_id: FontId,
    char_code: u32,
    is_text_mode: bool,
) -> Option<CharMetrics> {
    get_char_metrics_with_fallback(font_id, char_code).or_else(|| {
        if is_text_mode && ratex_types::supported_codepoint(char_code) {
            get_char_metrics(font_id, 77) // 'M'
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_regular_lowercase_a() {
        let m = get_char_metrics(FontId::MainRegular, 97).unwrap();
        assert!(
            (m.height - 0.43056).abs() < 0.001,
            "height of 'a': {}",
            m.height
        );
        assert!(m.depth.abs() < 0.001, "depth of 'a': {}", m.depth);
        assert!(m.width > 0.0, "width of 'a' should be positive");
    }

    #[test]
    fn test_main_regular_uppercase_a() {
        let m = get_char_metrics(FontId::MainRegular, 65).unwrap();
        assert!(
            (m.height - 0.68333).abs() < 0.001,
            "height of 'A': {}",
            m.height
        );
        assert!(m.depth.abs() < 0.001);
    }

    #[test]
    fn test_main_regular_digit_0() {
        let m = get_char_metrics(FontId::MainRegular, 48).unwrap();
        assert!(m.height > 0.0);
        assert!(m.width > 0.0);
    }

    #[test]
    fn test_math_italic_lowercase_a() {
        let m = get_char_metrics(FontId::MathItalic, 97).unwrap();
        assert!(m.height > 0.0);
        assert!(m.width > 0.0);
    }

    #[test]
    fn test_nonexistent_char() {
        assert!(get_char_metrics(FontId::MainRegular, 99999).is_none());
    }

    #[test]
    fn test_space_char() {
        let m = get_char_metrics(FontId::MainRegular, 32).unwrap();
        assert!((m.width - 0.25).abs() < 0.001, "space width: {}", m.width);
    }

    #[test]
    fn test_ams_regular() {
        let m = get_char_metrics(FontId::AmsRegular, 65).unwrap();
        assert!((m.height - 0.68889).abs() < 0.001);
    }

    #[test]
    fn test_extra_char_map_cyrillic() {
        // Cyrillic А maps to Latin A
        let m = get_char_metrics(FontId::MainRegular, 'А' as u32);
        // Direct lookup returns None (no Cyrillic in metric table)
        assert!(m.is_none());
        // Fallback lookup should work
        let m = get_char_metrics_with_fallback(FontId::MainRegular, 'А' as u32).unwrap();
        assert!(m.height > 0.0);
    }

    #[test]
    fn test_math_constants_textstyle() {
        let mc = get_global_metrics(0);
        assert!((mc.axis_height - 0.25).abs() < 0.001);
        assert!((mc.default_rule_thickness - 0.04).abs() < 0.001);
        assert!(mc.num1 > mc.num2);
        assert!(mc.sup1 > mc.sup2);
        assert!((mc.quad - 1.0).abs() < 0.001);
        assert!((mc.pt_per_em - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_math_constants_scriptstyle() {
        let mc = get_global_metrics(1);
        assert!((mc.axis_height - 0.25).abs() < 0.001);
        assert!((mc.quad - 1.171).abs() < 0.001);
        assert!((mc.default_rule_thickness - 0.049).abs() < 0.001);
    }

    #[test]
    fn test_math_constants_scriptscriptstyle() {
        let mc = get_global_metrics(2);
        assert!((mc.quad - 1.472).abs() < 0.001);
        assert!((mc.num1 - 0.925).abs() < 0.001);
    }

    #[test]
    fn test_css_em_per_mu() {
        let mc = get_global_metrics(0);
        assert!((mc.css_em_per_mu() - 1.0 / 18.0).abs() < 0.0001);
    }

    #[test]
    fn test_text_mode_fallback_cjk() {
        // CJK character '中' (U+4E2D) has no metrics, but in text mode
        // it should fall back to 'M' metrics.
        let ch = '中' as u32;
        assert!(get_char_metrics(FontId::MainRegular, ch).is_none());
        assert!(get_char_metrics_with_fallback(FontId::MainRegular, ch).is_none());

        let m = get_char_metrics_for_mode(FontId::MainRegular, ch, true).unwrap();
        let m_ref = get_char_metrics(FontId::MainRegular, 77).unwrap(); // 'M'
        assert!((m.height - m_ref.height).abs() < f64::EPSILON);
        assert!((m.width - m_ref.width).abs() < f64::EPSILON);
    }

    #[test]
    fn test_text_mode_fallback_not_in_math() {
        let ch = '中' as u32;
        assert!(get_char_metrics_for_mode(FontId::MainRegular, ch, false).is_none());
    }

    #[test]
    fn test_text_mode_fallback_devanagari() {
        // Devanagari (Brahmic script) U+0900-U+097F
        let ch = 0x0915u32; // क
        let m = get_char_metrics_for_mode(FontId::MainRegular, ch, true).unwrap();
        assert!(m.height > 0.0);
    }
}
