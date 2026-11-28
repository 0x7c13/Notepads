/// Unicode script detection for characters supported in `\text{}` blocks.
///
/// Port of KaTeX's `unicodeScripts.ts`. Characters from these scripts can
/// appear inside `\text{}` even when no font metrics exist for them.
///
/// Each script has a name and one or more [start, end] (inclusive) blocks.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeScript {
    Latin,
    Cyrillic,
    Armenian,
    Brahmic,
    Georgian,
    Cjk,
    Hangul,
}

impl UnicodeScript {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Latin => "latin",
            Self::Cyrillic => "cyrillic",
            Self::Armenian => "armenian",
            Self::Brahmic => "brahmic",
            Self::Georgian => "georgian",
            Self::Cjk => "cjk",
            Self::Hangul => "hangul",
        }
    }
}

struct ScriptDef {
    script: UnicodeScript,
    blocks: &'static [(u32, u32)],
}

static SCRIPT_DATA: &[ScriptDef] = &[
    ScriptDef {
        script: UnicodeScript::Latin,
        blocks: &[
            (0x0100, 0x024F), // Latin Extended-A and Latin Extended-B
            (0x0300, 0x036F), // Combining Diacritical Marks
        ],
    },
    ScriptDef {
        script: UnicodeScript::Cyrillic,
        blocks: &[(0x0400, 0x04FF)],
    },
    ScriptDef {
        script: UnicodeScript::Armenian,
        blocks: &[(0x0530, 0x058F)],
    },
    ScriptDef {
        script: UnicodeScript::Brahmic,
        blocks: &[(0x0900, 0x109F)],
    },
    ScriptDef {
        script: UnicodeScript::Georgian,
        blocks: &[(0x10A0, 0x10FF)],
    },
    ScriptDef {
        script: UnicodeScript::Cjk,
        blocks: &[
            (0x3000, 0x30FF), // CJK symbols, Hiragana, Katakana
            (0x4E00, 0x9FAF), // CJK Unified Ideographs
            (0xFF00, 0xFF60), // Fullwidth forms
        ],
    },
    ScriptDef {
        script: UnicodeScript::Hangul,
        blocks: &[(0xAC00, 0xD7AF)],
    },
];

/// Identify the script/script family of a Unicode codepoint, if known.
pub fn script_from_codepoint(codepoint: u32) -> Option<UnicodeScript> {
    for def in SCRIPT_DATA {
        for &(lo, hi) in def.blocks {
            if codepoint >= lo && codepoint <= hi {
                return Some(def.script);
            }
        }
    }
    None
}

/// Return `true` if the codepoint falls within one of the supported scripts.
///
/// Used in text mode to decide whether a character without font metrics
/// should be rendered with fallback metrics (Latin capital M).
pub fn supported_codepoint(codepoint: u32) -> bool {
    for def in SCRIPT_DATA {
        for &(lo, hi) in def.blocks {
            if codepoint >= lo && codepoint <= hi {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_latin_not_matched() {
        assert!(!supported_codepoint('A' as u32));
        assert!(!supported_codepoint('z' as u32));
    }

    #[test]
    fn test_latin_extended() {
        assert!(supported_codepoint(0x0100)); // Ā
        assert!(supported_codepoint(0x024F)); // end of Latin Extended-B
        assert_eq!(script_from_codepoint(0x0100), Some(UnicodeScript::Latin));
    }

    #[test]
    fn test_combining_diacritical() {
        assert!(supported_codepoint(0x0300)); // combining grave accent
        assert!(supported_codepoint(0x0301)); // combining acute accent
        assert!(supported_codepoint(0x036F)); // end of combining marks
        assert_eq!(script_from_codepoint(0x0301), Some(UnicodeScript::Latin));
    }

    #[test]
    fn test_cyrillic() {
        assert!(supported_codepoint('А' as u32)); // Cyrillic А = 0x0410
        assert!(supported_codepoint('я' as u32)); // Cyrillic я = 0x044F
        assert_eq!(
            script_from_codepoint('А' as u32),
            Some(UnicodeScript::Cyrillic)
        );
    }

    #[test]
    fn test_cjk() {
        assert!(supported_codepoint('中' as u32)); // U+4E2D
        assert!(supported_codepoint('あ' as u32)); // Hiragana, U+3042
        assert_eq!(script_from_codepoint('中' as u32), Some(UnicodeScript::Cjk));
    }

    #[test]
    fn test_hangul() {
        assert!(supported_codepoint(0xAC00)); // first Hangul syllable
        assert_eq!(script_from_codepoint(0xAC00), Some(UnicodeScript::Hangul));
    }

    #[test]
    fn test_brahmic() {
        assert!(supported_codepoint(0x0900)); // Devanagari start
        assert!(supported_codepoint(0x0E01)); // Thai
        assert_eq!(script_from_codepoint(0x0900), Some(UnicodeScript::Brahmic));
    }

    #[test]
    fn test_armenian() {
        assert!(supported_codepoint(0x0530));
        assert_eq!(script_from_codepoint(0x0531), Some(UnicodeScript::Armenian));
    }

    #[test]
    fn test_georgian() {
        assert!(supported_codepoint(0x10A0));
        assert_eq!(script_from_codepoint(0x10A0), Some(UnicodeScript::Georgian));
    }

    #[test]
    fn test_unsupported_codepoint() {
        assert!(!supported_codepoint(0xFFFF));
        assert_eq!(script_from_codepoint(0xFFFF), None);
    }
}
