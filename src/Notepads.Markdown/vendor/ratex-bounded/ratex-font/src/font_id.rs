/// Font families used in KaTeX math rendering.
///
/// Each variant corresponds to a specific OpenType font with pre-extracted metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontId {
    AmsRegular,
    CaligraphicRegular,
    FrakturRegular,
    /// Bold Fraktur — glyphs from `KaTeX_Fraktur-Bold.ttf`; advances from bold `hmtx` (`FRAKTUR_BOLD`).
    FrakturBold,
    MainBold,
    MainBoldItalic,
    MainItalic,
    MainRegular,
    MathBoldItalic,
    MathItalic,
    SansSerifBold,
    SansSerifItalic,
    SansSerifRegular,
    ScriptRegular,
    Size1Regular,
    Size2Regular,
    Size3Regular,
    Size4Regular,
    TypewriterRegular,
    CjkRegular,
    /// System font fallback for characters not present in the primary CJK font
    /// (e.g. emoji when RATEX_UNICODE_FONT points to a CJK-only font).
    CjkFallback,
    /// Color / outline emoji face (e.g. Apple Color Emoji) when `CjkFallback` still has no glyph.
    EmojiFallback,
}

impl FontId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AmsRegular => "AMS-Regular",
            Self::CaligraphicRegular => "Caligraphic-Regular",
            Self::FrakturRegular => "Fraktur-Regular",
            Self::FrakturBold => "Fraktur-Bold",
            Self::MainBold => "Main-Bold",
            Self::MainBoldItalic => "Main-BoldItalic",
            Self::MainItalic => "Main-Italic",
            Self::MainRegular => "Main-Regular",
            Self::MathBoldItalic => "Math-BoldItalic",
            Self::MathItalic => "Math-Italic",
            Self::SansSerifBold => "SansSerif-Bold",
            Self::SansSerifItalic => "SansSerif-Italic",
            Self::SansSerifRegular => "SansSerif-Regular",
            Self::ScriptRegular => "Script-Regular",
            Self::Size1Regular => "Size1-Regular",
            Self::Size2Regular => "Size2-Regular",
            Self::Size3Regular => "Size3-Regular",
            Self::Size4Regular => "Size4-Regular",
            Self::TypewriterRegular => "Typewriter-Regular",
            Self::CjkRegular => "CJK-Regular",
            Self::CjkFallback => "CJK-Fallback",
            Self::EmojiFallback => "Emoji-Fallback",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "AMS-Regular" => Some(Self::AmsRegular),
            "Caligraphic-Regular" => Some(Self::CaligraphicRegular),
            "Fraktur-Regular" => Some(Self::FrakturRegular),
            "Fraktur-Bold" => Some(Self::FrakturBold),
            "Main-Bold" => Some(Self::MainBold),
            "Main-BoldItalic" => Some(Self::MainBoldItalic),
            "Main-Italic" => Some(Self::MainItalic),
            "Main-Regular" => Some(Self::MainRegular),
            "Math-BoldItalic" => Some(Self::MathBoldItalic),
            "Math-Italic" => Some(Self::MathItalic),
            "SansSerif-Bold" => Some(Self::SansSerifBold),
            "SansSerif-Italic" => Some(Self::SansSerifItalic),
            "SansSerif-Regular" => Some(Self::SansSerifRegular),
            "Script-Regular" => Some(Self::ScriptRegular),
            "Size1-Regular" => Some(Self::Size1Regular),
            "Size2-Regular" => Some(Self::Size2Regular),
            "Size3-Regular" => Some(Self::Size3Regular),
            "Size4-Regular" => Some(Self::Size4Regular),
            "Typewriter-Regular" => Some(Self::TypewriterRegular),
            "CJK-Regular" => Some(Self::CjkRegular),
            "CJK-Fallback" => Some(Self::CjkFallback),
            "Emoji-Fallback" => Some(Self::EmojiFallback),
            _ => None,
        }
    }
}

impl std::fmt::Display for FontId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::FontId;

    #[test]
    fn cjk_regular_parse() {
        assert_eq!(FontId::parse("CJK-Regular"), Some(FontId::CjkRegular));
    }

    #[test]
    fn cjk_regular_as_str() {
        assert_eq!(FontId::CjkRegular.as_str(), "CJK-Regular");
    }

    #[test]
    fn cjk_regular_display() {
        assert_eq!(format!("{}", FontId::CjkRegular), "CJK-Regular");
    }

    #[test]
    fn cjk_regular_roundtrip() {
        assert_eq!(
            FontId::parse(FontId::CjkRegular.as_str()),
            Some(FontId::CjkRegular)
        );
    }

    #[test]
    fn cjk_fallback_roundtrip() {
        assert_eq!(
            FontId::parse(FontId::CjkFallback.as_str()),
            Some(FontId::CjkFallback)
        );
    }

    #[test]
    fn emoji_fallback_roundtrip() {
        assert_eq!(
            FontId::parse(FontId::EmojiFallback.as_str()),
            Some(FontId::EmojiFallback)
        );
    }

    #[test]
    fn parse_unknown_font() {
        assert_eq!(FontId::parse("NotARealFont"), None);
    }

    #[test]
    fn all_variants_roundtrip() {
        let variants = [
            FontId::AmsRegular,
            FontId::CaligraphicRegular,
            FontId::FrakturRegular,
            FontId::FrakturBold,
            FontId::MainBold,
            FontId::MainBoldItalic,
            FontId::MainItalic,
            FontId::MainRegular,
            FontId::MathBoldItalic,
            FontId::MathItalic,
            FontId::SansSerifBold,
            FontId::SansSerifItalic,
            FontId::SansSerifRegular,
            FontId::ScriptRegular,
            FontId::Size1Regular,
            FontId::Size2Regular,
            FontId::Size3Regular,
            FontId::Size4Regular,
            FontId::TypewriterRegular,
            FontId::CjkRegular,
            FontId::CjkFallback,
            FontId::EmojiFallback,
        ];
        for v in variants {
            assert_eq!(
                FontId::parse(v.as_str()),
                Some(v),
                "roundtrip failed for {:?}",
                v
            );
        }
    }
}
