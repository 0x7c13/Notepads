use ratex_font::{MathConstants, get_global_metrics};
use ratex_types::color::Color;
use ratex_types::math_style::MathStyle;

/// Metrics of a glyph the KaTeX fonts lack, in em, as the host's fallback font draws it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallbackGlyphMetrics {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
}

#[doc(hidden)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChineseTextFamily {
    #[default]
    Serif,
    SansSerif,
    Monospace,
}

#[doc(hidden)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChineseTextStyle {
    pub family: ChineseTextFamily,
    pub bold: bool,
    pub italic: bool,
}

impl ChineseTextStyle {
    pub(crate) fn with_command(mut self, font: Option<&str>) -> Self {
        match font.map(|font| font.trim_start_matches('\\')) {
            Some("textnormal" | "textrm") => self.family = ChineseTextFamily::Serif,
            Some("textsf") => self.family = ChineseTextFamily::SansSerif,
            Some("texttt") => self.family = ChineseTextFamily::Monospace,
            Some("textbf") => self.bold = true,
            Some("textmd") => self.bold = false,
            Some("textit" | "textsl") => self.italic = true,
            Some("textup") => self.italic = false,
            Some("emph") => self.italic = !self.italic,
            _ => {}
        }
        self
    }

    pub(crate) fn with_font(self, font: &str) -> Self {
        let mut style = Self::default();
        match font.trim_start_matches('\\') {
            "mathrm" => {}
            "mathbf" => style.bold = true,
            "mathit" => style.italic = true,
            "mathsf" => style.family = ChineseTextFamily::SansSerif,
            "mathtt" => style.family = ChineseTextFamily::Monospace,
            _ => return self,
        }
        style
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChineseTextRequest<'a> {
    pub text: &'a str,
    pub style: ChineseTextStyle,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChineseTextRun<'a> {
    pub request: ChineseTextRequest<'a>,
    pub metrics: Option<FallbackGlyphMetrics>,
}

// A positioned Chinese run uses the existing CJK glyph item and a job-local
// ID outside the Unicode scalar range. Only the opt-in host path creates IDs.
const CHINESE_TEXT_ID_START: u32 = 0x11_0000;

pub(crate) fn chinese_text_codepoint(index: usize) -> Option<u32> {
    CHINESE_TEXT_ID_START.checked_add(u32::try_from(index).ok()?)
}

#[doc(hidden)]
pub fn chinese_text_run_index(font: &str, codepoint: u32) -> Option<usize> {
    if font != ratex_font::FontId::CjkRegular.as_str() {
        return None;
    }
    codepoint
        .checked_sub(CHINESE_TEXT_ID_START)
        .map(|id| id as usize)
}

/// Layout options passed through the layout tree.
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub style: MathStyle,
    pub color: Color,
    /// When set (e.g. in align/aligned), cap relation spacing to this many mu for consistency.
    pub align_relation_spacing: Option<f64>,
    /// When inside \\left...\\right, the stretch height for \\middle delimiters (second pass only).
    pub leftright_delim_height: Option<f64>,
    /// Extra horizontal kern between glyphs (em), e.g. for custom text tracking.
    pub inter_glyph_kern_em: f64,
    /// Explicit TeX sizing multiplier currently in force (`1.0` for `\normalsize`).
    ///
    /// This is layout state propagated through `\tiny` ... `\Huge`; callers
    /// should normally leave it at its default.
    pub explicit_size_multiplier: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            style: MathStyle::Display,
            color: Color::BLACK,
            align_relation_spacing: None,
            leftright_delim_height: None,
            inter_glyph_kern_em: 0.0,
            explicit_size_multiplier: 1.0,
        }
    }
}

impl LayoutOptions {
    pub fn metrics(&self) -> &'static MathConstants {
        get_global_metrics(self.style.size_index())
    }

    pub fn size_multiplier(&self) -> f64 {
        self.style.size_multiplier()
    }

    pub fn with_style(&self, style: MathStyle) -> Self {
        Self {
            style,
            color: self.color,
            align_relation_spacing: self.align_relation_spacing,
            leftright_delim_height: self.leftright_delim_height,
            inter_glyph_kern_em: self.inter_glyph_kern_em,
            explicit_size_multiplier: self.explicit_size_multiplier,
        }
    }

    pub fn with_color(&self, color: Color) -> Self {
        Self {
            style: self.style,
            color,
            align_relation_spacing: self.align_relation_spacing,
            leftright_delim_height: self.leftright_delim_height,
            inter_glyph_kern_em: self.inter_glyph_kern_em,
            explicit_size_multiplier: self.explicit_size_multiplier,
        }
    }

    pub fn with_inter_glyph_kern(&self, em: f64) -> Self {
        Self {
            inter_glyph_kern_em: em,
            ..self.clone()
        }
    }

    pub(crate) fn with_explicit_size_multiplier(&self, multiplier: f64) -> Self {
        Self {
            explicit_size_multiplier: multiplier,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_inter_glyph_kern_option_survives_option_derivation() {
        let options = LayoutOptions::default()
            .with_inter_glyph_kern(0.02)
            .with_style(MathStyle::Script)
            .with_color(Color::new(0.1, 0.2, 0.3, 1.0));

        assert_eq!(options.inter_glyph_kern_em, 0.02);
        assert_eq!(options.explicit_size_multiplier, 1.0);
    }
}
