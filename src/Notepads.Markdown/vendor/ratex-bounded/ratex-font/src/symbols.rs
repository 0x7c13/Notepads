// Notepads Markdown read-only symbol-index modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::data::symbols_data;
pub(crate) const fn bounded_index_bytes() -> usize {
    std::mem::size_of_val(&NAME_INDEX) + std::mem::size_of_val(&CODEPOINT_INDEX)
}

/// A symbol's mode in TeX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Math,
    Text,
}

/// The font family for a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolFont {
    Main,
    Ams,
}

/// The group/atom type of a symbol, determining spacing behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Bin,
    Close,
    Inner,
    Open,
    Punct,
    Rel,
    AccentToken,
    MathOrd,
    OpToken,
    Spacing,
    TextOrd,
}

impl Group {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bin" => Some(Self::Bin),
            "close" => Some(Self::Close),
            "inner" => Some(Self::Inner),
            "open" => Some(Self::Open),
            "punct" => Some(Self::Punct),
            "rel" => Some(Self::Rel),
            "accent-token" => Some(Self::AccentToken),
            "mathord" => Some(Self::MathOrd),
            "op-token" => Some(Self::OpToken),
            "spacing" => Some(Self::Spacing),
            "textord" => Some(Self::TextOrd),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Close => "close",
            Self::Inner => "inner",
            Self::Open => "open",
            Self::Punct => "punct",
            Self::Rel => "rel",
            Self::AccentToken => "accent-token",
            Self::MathOrd => "mathord",
            Self::OpToken => "op-token",
            Self::Spacing => "spacing",
            Self::TextOrd => "textord",
        }
    }

    pub fn is_atom(self) -> bool {
        matches!(
            self,
            Self::Bin | Self::Close | Self::Inner | Self::Open | Self::Punct | Self::Rel
        )
    }
}

/// Information about a resolved symbol.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: &'static str,
    pub mode: Mode,
    pub font: SymbolFont,
    pub group: Group,
    pub codepoint: Option<char>,
}

/// Presentation metadata for a symbol whose bundled replacement glyph must be
/// fitted to another symbol's metric box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolRenderSpec {
    pub target_font: SymbolFont,
    pub target_codepoint: char,
}

fn name_index(name: &str, mode: u8) -> Option<usize> {
    NAME_INDEX
        .binary_search_by(|&index| {
            let (entry_name, entry_mode, _, _, _) = symbols_data::SYMBOLS[usize::from(index)];
            entry_mode.cmp(&mode).then_with(|| entry_name.cmp(name))
        })
        .ok()
        .map(|index| usize::from(NAME_INDEX[index]))
}

fn codepoint_index(codepoint: char, mode: u8) -> Option<usize> {
    CODEPOINT_INDEX
        .binary_search_by(|&index| {
            let (_, entry_mode, _, _, entry_codepoint) = symbols_data::SYMBOLS[usize::from(index)];
            entry_mode
                .cmp(&mode)
                .then_with(|| entry_codepoint.cmp(&Some(codepoint)))
        })
        .ok()
        .map(|index| usize::from(CODEPOINT_INDEX[index]))
}

fn entry_to_info(idx: usize, mode: Mode) -> SymbolInfo {
    let (n, _, f, g, cp) = symbols_data::SYMBOLS[idx];
    SymbolInfo {
        name: n,
        mode,
        font: if f == 0 {
            SymbolFont::Main
        } else {
            SymbolFont::Ams
        },
        group: Group::parse(g).unwrap_or(Group::MathOrd),
        codepoint: cp,
    }
}

/// Look up a symbol by name in a given mode using a fixed read-only sorted index.
///
/// When `name` is a single Unicode character, this also tries codepoint lookup,
/// matching KaTeX's `acceptUnicodeChar` behavior where both `name` and `replace`
/// are valid keys (e.g. both `\alpha` and `α` resolve to the same symbol).
pub fn get_symbol(name: &str, mode: Mode) -> Option<SymbolInfo> {
    let mode_val: u8 = match mode {
        Mode::Math => 0,
        Mode::Text => 1,
    };
    // Direct name lookup (command name like "\\alpha")
    if let Some(idx) = name_index(name, mode_val) {
        return Some(entry_to_info(idx, mode));
    }
    // KaTeX acceptUnicodeChar: replace string is also a key. Single-char name → try by codepoint.
    let mut chars = name.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        if let Some(info) = get_symbol_by_codepoint(ch, mode) {
            return Some(info);
        }
    }
    None
}

/// Look up a math-mode symbol by name (convenience function).
pub fn get_math_symbol(name: &str) -> Option<SymbolInfo> {
    get_symbol(name, Mode::Math)
}

/// Look up a text-mode symbol by name (convenience function).
pub fn get_text_symbol(name: &str) -> Option<SymbolInfo> {
    get_symbol(name, Mode::Text)
}

/// Look up a symbol by Unicode codepoint using a fixed read-only sorted index.
pub fn get_symbol_by_codepoint(ch: char, mode: Mode) -> Option<SymbolInfo> {
    let mode_val: u8 = match mode {
        Mode::Math => 0,
        Mode::Text => 1,
    };
    codepoint_index(ch, mode_val).map(|idx| entry_to_info(idx, mode))
}

/// Look up optional data-driven glyph fitting metadata for a symbol.
pub fn get_symbol_render_spec(name: &str, mode: Mode) -> Option<SymbolRenderSpec> {
    let mode_val: u8 = match mode {
        Mode::Math => 0,
        Mode::Text => 1,
    };
    symbols_data::SYMBOL_RENDER_SPECS
        .iter()
        .find(|&&(entry_name, entry_mode, _, _)| entry_name == name && entry_mode == mode_val)
        .map(|&(_, _, target_font, target_codepoint)| SymbolRenderSpec {
            target_font: if target_font == 0 {
                SymbolFont::Main
            } else {
                SymbolFont::Ams
            },
            target_codepoint,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_equiv() {
        let sym = get_math_symbol("\\equiv").unwrap();
        assert_eq!(sym.group, Group::Rel);
        assert_eq!(sym.font, SymbolFont::Main);
        assert_eq!(sym.codepoint, Some('\u{2261}'));
    }

    #[test]
    fn test_get_alpha() {
        let sym = get_math_symbol("\\alpha").unwrap();
        assert_eq!(sym.group, Group::MathOrd);
        assert_eq!(sym.codepoint, Some('\u{03B1}'));
    }

    #[test]
    fn test_get_plus() {
        let sym = get_math_symbol("+").unwrap();
        assert_eq!(sym.group, Group::Bin);
    }

    #[test]
    fn test_get_lparen() {
        let sym = get_math_symbol("\\lparen").unwrap();
        assert_eq!(sym.group, Group::Open);
        assert_eq!(sym.codepoint, Some('('));
    }

    #[test]
    fn test_get_rparen() {
        let sym = get_math_symbol("\\rparen").unwrap();
        assert_eq!(sym.group, Group::Close);
        assert_eq!(sym.codepoint, Some(')'));
    }

    #[test]
    fn test_get_sum() {
        let sym = get_math_symbol("\\sum").unwrap();
        assert_eq!(sym.group, Group::OpToken);
    }

    #[test]
    fn test_get_int() {
        let sym = get_math_symbol("\\int").unwrap();
        assert_eq!(sym.group, Group::OpToken);
    }

    #[test]
    fn test_get_frac_not_a_symbol() {
        // \frac is a command, not a symbol
        assert!(get_math_symbol("\\frac").is_none());
    }

    #[test]
    fn test_text_mode_hash() {
        let sym = get_text_symbol("\\#").unwrap();
        assert_eq!(sym.group, Group::TextOrd);
    }

    #[test]
    fn test_math_forall() {
        let sym = get_math_symbol("\\forall").unwrap();
        assert_eq!(sym.codepoint, Some('\u{2200}'));
    }

    #[test]
    fn test_ams_symbol() {
        let sym = get_math_symbol("\\beth").unwrap();
        assert_eq!(sym.font, SymbolFont::Ams);
        assert_eq!(sym.codepoint, Some('\u{2136}'));
    }

    #[test]
    fn test_equals_is_rel() {
        let sym = get_math_symbol("=").unwrap();
        assert_eq!(sym.group, Group::Rel);
    }

    #[test]
    fn test_by_codepoint() {
        let sym = get_symbol_by_codepoint('\u{2261}', Mode::Math).unwrap();
        assert_eq!(sym.name, "\\equiv");
    }

    #[test]
    fn test_nonexistent() {
        assert!(get_math_symbol("\\nonexistent_command_xyz").is_none());
    }

    #[test]
    fn test_digit_is_textord() {
        // In KaTeX, digits in math mode are classified as "textord"
        for d in '0'..='9' {
            let name = d.to_string();
            let sym = get_math_symbol(&name).unwrap();
            assert_eq!(sym.group, Group::TextOrd, "digit {} should be textord", d);
        }
    }

    #[test]
    fn test_letters_are_mathord() {
        for ch in 'a'..='z' {
            let name = ch.to_string();
            let sym = get_math_symbol(&name).unwrap();
            assert_eq!(sym.group, Group::MathOrd, "letter {} should be mathord", ch);
        }
    }

    #[test]
    fn test_accept_unicode_char_by_name() {
        // KaTeX acceptUnicodeChar: symbol can be looked up by replace (Unicode char) as name
        let sym = get_symbol("α", Mode::Math).unwrap();
        assert_eq!(sym.name, "\\alpha");
        assert_eq!(sym.group, Group::MathOrd);
        assert_eq!(sym.codepoint, Some('α'));
    }

    #[test]
    fn white_circle_uses_large_circle_glyph_as_textord() {
        let sym = get_symbol("○", Mode::Math).unwrap();
        assert_eq!(sym.name, "○");
        assert_eq!(sym.group, Group::TextOrd);
        assert_eq!(sym.font, SymbolFont::Main);
        assert_eq!(sym.codepoint, Some('◯'));
        assert_eq!(
            get_symbol_render_spec("○", Mode::Math),
            Some(SymbolRenderSpec {
                target_font: SymbolFont::Ams,
                target_codepoint: '□',
            })
        );

        let bigcirc = get_math_symbol("\\bigcirc").unwrap();
        assert_eq!(bigcirc.group, Group::Bin);
        assert_eq!(bigcirc.codepoint, Some('◯'));

        // U+25EF LARGE CIRCLE remains the raw-Unicode spelling of \bigcirc.
        // The U+25CB alias is appended after KaTeX symbols specifically so it
        // cannot take precedence in the replacement-codepoint index.
        let raw_bigcirc = get_symbol("◯", Mode::Math).unwrap();
        assert_eq!(raw_bigcirc.name, "\\bigcirc");
        assert_eq!(raw_bigcirc.group, Group::Bin);
        assert_eq!(raw_bigcirc.codepoint, Some('◯'));
    }

    #[test]
    fn white_circle_extension_is_math_mode_only() {
        assert!(get_symbol("○", Mode::Text).is_none());
        assert!(get_symbol_render_spec("○", Mode::Text).is_none());
    }

    fn assert_original_result(actual: Option<SymbolInfo>, expected: Option<usize>, mode: Mode) {
        let fields = |value: SymbolInfo| {
            (
                value.name,
                value.mode,
                value.font,
                value.group,
                value.codepoint,
            )
        };
        assert_eq!(
            actual.map(fields),
            expected.map(|index| fields(entry_to_info(index, mode)))
        );
    }

    #[test]
    fn fixed_indices_preserve_every_original_first_alias_and_name_priority() {
        // Recreate the original initialization only in this counter-test. The
        // production lookups never construct a HashMap or run a lazy initializer.
        let mut names = std::collections::HashMap::new();
        let mut codepoints = std::collections::HashMap::new();
        for (index, &(name, mode, _, _, codepoint)) in symbols_data::SYMBOLS.iter().enumerate() {
            names.entry((mode, name)).or_insert(index);
            if let Some(codepoint) = codepoint {
                codepoints.entry((mode, codepoint)).or_insert(index);
            }
        }
        assert_eq!(NAME_INDEX.len(), names.len());
        assert_eq!(CODEPOINT_INDEX.len(), codepoints.len());
        for (mode_value, mode) in [(0, Mode::Math), (1, Mode::Text)] {
            for &(name, _, _, _, codepoint) in symbols_data::SYMBOLS {
                let mut chars = name.chars();
                let fallback = match (chars.next(), chars.next()) {
                    (Some(codepoint), None) => codepoints.get(&(mode_value, codepoint)),
                    _ => None,
                };
                let expected = names.get(&(mode_value, name)).or(fallback).copied();
                assert_original_result(get_symbol(name, mode), expected, mode);
                if let Some(codepoint) = codepoint {
                    let expected = codepoints.get(&(mode_value, codepoint)).copied();
                    assert_original_result(
                        get_symbol_by_codepoint(codepoint, mode),
                        expected,
                        mode,
                    );
                    let mut utf8 = [0; 4];
                    let raw_name = codepoint.encode_utf8(&mut utf8);
                    let expected = names.get(&(mode_value, &*raw_name)).copied().or(expected);
                    assert_original_result(get_symbol(raw_name, mode), expected, mode);
                }
            }
            assert!(get_symbol("\0", mode).is_none());
            assert!(get_symbol_by_codepoint('\0', mode).is_none());
            assert!(get_symbol_by_codepoint('\u{10ffff}', mode).is_none());
        }
    }

    #[test]
    fn fixed_indices_are_strictly_sorted_and_refer_only_to_original_entries() {
        for pair in NAME_INDEX.windows(2) {
            let left = symbols_data::SYMBOLS[usize::from(pair[0])];
            let right = symbols_data::SYMBOLS[usize::from(pair[1])];
            assert!((left.1, left.0) < (right.1, right.0));
        }
        for &index in &CODEPOINT_INDEX {
            assert!(symbols_data::SYMBOLS[usize::from(index)].4.is_some());
        }
        for pair in CODEPOINT_INDEX.windows(2) {
            let left = symbols_data::SYMBOLS[usize::from(pair[0])];
            let right = symbols_data::SYMBOLS[usize::from(pair[1])];
            assert!((left.1, left.4) < (right.1, right.4));
        }
    }
}

// Read-only indices generated from symbols_data.rs, preserving first occurrence.
// Source SHA256: c6d18d0d21ed52d9743c2a80d5df1d565f402c7dee75b1450f1bd3a4419ca450
// Rule: decode each original Rust string/char literal as UTF-8 without normalization,
// enumerate the 1,882 original rows, retain the first index for each (mode, name)
// and (mode, Some(codepoint)) key, sort those unique keys, then emit their u16 indices.
// Names sort by (mode, UTF-8 name); codepoints sort by (mode, Unicode scalar).
static NAME_INDEX: [u16; 1882] = [
    398, 602, 378, 379, 435, 380, 601, 599, 589, 590, 591, 592, 593, 594, 595, 596, 597, 598, 403,
    436, 402, 397, 600, 628, 630, 632, 634, 636, 638, 640, 642, 644, 646, 648, 650, 652, 654, 656,
    658, 660, 662, 664, 666, 668, 670, 672, 674, 676, 678, 426, 28, 306, 309, 30, 513, 1861, 1875,
    1876, 1879, 1880, 1862, 1863, 1864, 1865, 1866, 1860, 1867, 1868, 1869, 1870, 1877, 1878, 1871,
    1872, 1873, 1874, 73, 166, 207, 252, 253, 319, 167, 249, 486, 151, 152, 318, 46, 248, 321, 75,
    82, 275, 76, 83, 79, 282, 328, 50, 326, 323, 327, 44, 78, 303, 297, 48, 324, 194, 224, 320,
    484, 488, 325, 232, 478, 203, 322, 311, 517, 32, 434, 348, 72, 314, 404, 182, 66, 11, 240, 153,
    191, 192, 473, 521, 437, 242, 349, 172, 235, 493, 68, 494, 503, 502, 501, 507, 158, 447, 446,
    492, 490, 491, 157, 156, 154, 155, 238, 241, 345, 13, 445, 255, 256, 270, 522, 69, 206, 386,
    381, 25, 266, 523, 170, 369, 382, 219, 281, 296, 146, 145, 265, 444, 264, 40, 160, 405, 489,
    387, 197, 227, 263, 262, 280, 295, 52, 448, 173, 272, 271, 22, 55, 70, 519, 514, 564, 351, 164,
    163, 449, 43, 175, 383, 257, 526, 17, 188, 250, 254, 268, 269, 485, 298, 285, 300, 38, 346,
    352, 218, 247, 210, 179, 0, 354, 161, 35, 190, 37, 33, 18, 350, 406, 407, 208, 209, 408, 10,
    214, 244, 174, 118, 115, 116, 117, 518, 409, 212, 213, 216, 217, 215, 211, 524, 34, 45, 87, 88,
    142, 498, 497, 287, 410, 315, 495, 267, 496, 356, 357, 396, 358, 389, 394, 459, 461, 471, 24,
    512, 419, 302, 418, 277, 93, 90, 273, 80, 274, 279, 289, 260, 420, 177, 178, 181, 183, 186,
    187, 185, 180, 469, 61, 245, 9, 184, 243, 59, 101, 98, 99, 343, 100, 74, 81, 85, 77, 278, 294,
    390, 144, 465, 421, 258, 395, 571, 84, 510, 527, 568, 148, 150, 8, 417, 62, 359, 288, 137, 140,
    138, 125, 130, 36, 39, 123, 86, 342, 149, 424, 114, 19, 135, 139, 425, 97, 107, 433, 428, 124,
    102, 103, 136, 106, 415, 119, 120, 416, 110, 111, 126, 127, 360, 109, 108, 95, 439, 506, 505,
    504, 371, 362, 63, 440, 286, 443, 441, 23, 12, 442, 4, 368, 363, 236, 384, 567, 1, 199, 196, 5,
    105, 131, 104, 198, 316, 499, 20, 370, 401, 399, 460, 463, 472, 304, 470, 60, 246, 364, 422,
    293, 94, 91, 291, 96, 290, 301, 261, 189, 58, 466, 259, 400, 89, 388, 42, 233, 234, 365, 3, 7,
    205, 508, 251, 204, 14, 427, 47, 159, 65, 67, 195, 15, 225, 16, 165, 450, 411, 413, 193, 112,
    113, 2, 229, 226, 6, 122, 132, 121, 228, 500, 412, 414, 223, 128, 129, 393, 92, 366, 239, 355,
    222, 221, 520, 385, 423, 344, 317, 143, 451, 201, 220, 452, 231, 276, 292, 133, 134, 483, 487,
    284, 299, 64, 367, 283, 202, 372, 176, 347, 377, 374, 237, 375, 376, 373, 141, 200, 230, 515,
    21, 525, 392, 438, 475, 391, 41, 71, 361, 168, 353, 453, 477, 456, 305, 680, 682, 684, 686,
    688, 690, 692, 694, 696, 698, 700, 702, 704, 706, 708, 710, 712, 714, 716, 718, 720, 722, 724,
    726, 728, 730, 474, 26, 1854, 1856, 1858, 528, 529, 329, 330, 331, 332, 333, 334, 335, 336,
    337, 338, 339, 340, 341, 573, 575, 587, 577, 579, 581, 583, 585, 1881, 732, 734, 736, 738, 740,
    742, 744, 746, 748, 750, 752, 754, 756, 758, 760, 762, 764, 766, 768, 770, 772, 774, 776, 778,
    780, 782, 784, 786, 788, 790, 792, 794, 796, 798, 800, 802, 804, 806, 808, 810, 812, 814, 816,
    818, 820, 822, 824, 826, 828, 830, 832, 834, 836, 838, 840, 842, 844, 846, 848, 850, 852, 854,
    856, 858, 860, 862, 864, 866, 868, 870, 872, 874, 876, 878, 880, 882, 884, 886, 888, 890, 892,
    894, 896, 898, 900, 902, 904, 906, 908, 910, 912, 914, 916, 918, 920, 922, 924, 926, 928, 930,
    932, 934, 936, 938, 940, 942, 944, 946, 948, 950, 952, 954, 956, 958, 960, 962, 964, 966, 968,
    970, 972, 974, 976, 978, 980, 982, 984, 986, 988, 990, 992, 994, 996, 998, 1000, 1002, 1004,
    1006, 1008, 1010, 1012, 1014, 1016, 1018, 1020, 1022, 1024, 1026, 1028, 1030, 1032, 1034, 1036,
    1038, 1040, 1042, 1670, 1674, 1678, 1682, 1686, 1690, 1694, 1698, 1702, 1706, 1710, 1714, 1718,
    1722, 1726, 1730, 1734, 1738, 1742, 1746, 1750, 1754, 1758, 1762, 1766, 1770, 1044, 1046, 1048,
    1050, 1052, 1054, 1056, 1058, 1060, 1062, 1064, 1066, 1068, 1070, 1072, 1074, 1076, 1078, 1080,
    1082, 1084, 1086, 1088, 1090, 1092, 1094, 1096, 1098, 1100, 1102, 1104, 1106, 1108, 1110, 1112,
    1114, 1116, 1118, 1120, 1122, 1124, 1126, 1128, 1130, 1132, 1134, 1136, 1138, 1140, 1142, 1144,
    1146, 1668, 1672, 1676, 1680, 1684, 1688, 1692, 1696, 1700, 1704, 1708, 1712, 1716, 1720, 1724,
    1728, 1732, 1736, 1740, 1744, 1748, 1752, 1756, 1760, 1764, 1768, 1772, 1148, 1150, 1152, 1154,
    1156, 1158, 1160, 1162, 1164, 1166, 1168, 1170, 1172, 1174, 1176, 1178, 1180, 1182, 1184, 1186,
    1188, 1190, 1192, 1194, 1196, 1198, 1200, 1202, 1204, 1206, 1208, 1210, 1212, 1214, 1216, 1218,
    1220, 1222, 1224, 1226, 1228, 1230, 1232, 1234, 1236, 1238, 1240, 1242, 1244, 1246, 1248, 1250,
    1252, 1254, 1256, 1258, 1260, 1262, 1264, 1266, 1268, 1270, 1272, 1274, 1276, 1278, 1280, 1282,
    1284, 1286, 1288, 1290, 1292, 1294, 1296, 1298, 1300, 1302, 1304, 1306, 1308, 1310, 1312, 1314,
    1316, 1318, 1320, 1322, 1324, 1326, 1328, 1330, 1332, 1334, 1336, 1338, 1340, 1342, 1344, 1346,
    1348, 1350, 1352, 1354, 1356, 1358, 1360, 1362, 1364, 1366, 1368, 1370, 1372, 1374, 1376, 1378,
    1380, 1382, 1384, 1386, 1388, 1390, 1392, 1394, 1396, 1398, 1400, 1402, 1404, 1406, 1408, 1410,
    1412, 1414, 1416, 1418, 1420, 1422, 1424, 1426, 1428, 1430, 1432, 1434, 1436, 1438, 1440, 1442,
    1444, 1446, 1448, 1450, 1452, 1454, 1456, 1458, 1460, 1462, 1464, 1466, 1468, 1470, 1472, 1474,
    1476, 1478, 1480, 1482, 1484, 1486, 1488, 1490, 1492, 1494, 1496, 1498, 1500, 1502, 1504, 1506,
    1508, 1510, 1512, 1514, 1516, 1518, 1520, 1522, 1524, 1526, 1528, 1530, 1532, 1534, 1536, 1538,
    1540, 1542, 1544, 1546, 1548, 1550, 1552, 1554, 1556, 1558, 1560, 1562, 1564, 1566, 1568, 1570,
    1572, 1574, 1576, 1578, 1580, 1582, 1584, 1586, 1588, 1590, 1592, 1594, 1596, 1598, 1600, 1602,
    1604, 1606, 1608, 1610, 1612, 1614, 1616, 1618, 1620, 1622, 1624, 1626, 1628, 1630, 1632, 1634,
    1636, 1638, 1640, 1642, 1644, 1646, 1648, 1650, 1652, 1654, 1656, 1658, 1660, 1662, 1664, 1666,
    1774, 1776, 1778, 1780, 1782, 1784, 1786, 1788, 1790, 1792, 1794, 1796, 1798, 1800, 1802, 1804,
    1806, 1808, 1810, 1812, 1814, 1816, 1818, 1820, 1822, 1824, 1826, 1828, 1830, 1832, 1834, 1836,
    1838, 1840, 1842, 1844, 1846, 1848, 1850, 1852, 430, 613, 621, 558, 562, 616, 617, 615, 620,
    627, 618, 552, 554, 626, 625, 603, 604, 605, 606, 607, 608, 609, 610, 611, 612, 623, 622, 619,
    624, 614, 629, 631, 633, 635, 637, 639, 641, 643, 645, 647, 649, 651, 653, 655, 657, 659, 661,
    663, 665, 667, 669, 671, 673, 675, 677, 679, 429, 549, 29, 307, 310, 31, 539, 545, 543, 536,
    550, 538, 537, 51, 49, 541, 312, 540, 533, 546, 171, 147, 53, 56, 565, 530, 531, 462, 511, 572,
    432, 535, 534, 569, 547, 464, 431, 532, 482, 480, 481, 476, 479, 455, 458, 551, 54, 57, 566,
    308, 509, 555, 553, 468, 467, 561, 563, 557, 559, 570, 313, 544, 548, 516, 169, 454, 457, 542,
    556, 560, 681, 683, 685, 687, 689, 691, 693, 695, 697, 699, 701, 703, 705, 707, 709, 711, 713,
    715, 717, 719, 721, 723, 725, 727, 729, 731, 27, 1855, 1857, 162, 1859, 574, 576, 588, 578,
    580, 582, 584, 586, 733, 735, 737, 739, 741, 743, 745, 747, 749, 751, 753, 755, 757, 759, 761,
    763, 765, 767, 769, 771, 773, 775, 777, 779, 781, 783, 785, 787, 789, 791, 793, 795, 797, 799,
    801, 803, 805, 807, 809, 811, 813, 815, 817, 819, 821, 823, 825, 827, 829, 831, 833, 835, 837,
    839, 841, 843, 845, 847, 849, 851, 853, 855, 857, 859, 861, 863, 865, 867, 869, 871, 873, 875,
    877, 879, 881, 883, 885, 887, 889, 891, 893, 895, 897, 899, 901, 903, 905, 907, 909, 911, 913,
    915, 917, 919, 921, 923, 925, 927, 929, 931, 933, 935, 937, 939, 941, 943, 945, 947, 949, 951,
    953, 955, 957, 959, 961, 963, 965, 967, 969, 971, 973, 975, 977, 979, 981, 983, 985, 987, 989,
    991, 993, 995, 997, 999, 1001, 1003, 1005, 1007, 1009, 1011, 1013, 1015, 1017, 1019, 1021,
    1023, 1025, 1027, 1029, 1031, 1033, 1035, 1037, 1039, 1041, 1043, 1671, 1675, 1679, 1683, 1687,
    1691, 1695, 1699, 1703, 1707, 1711, 1715, 1719, 1723, 1727, 1731, 1735, 1739, 1743, 1747, 1751,
    1755, 1759, 1763, 1767, 1771, 1045, 1047, 1049, 1051, 1053, 1055, 1057, 1059, 1061, 1063, 1065,
    1067, 1069, 1071, 1073, 1075, 1077, 1079, 1081, 1083, 1085, 1087, 1089, 1091, 1093, 1095, 1097,
    1099, 1101, 1103, 1105, 1107, 1109, 1111, 1113, 1115, 1117, 1119, 1121, 1123, 1125, 1127, 1129,
    1131, 1133, 1135, 1137, 1139, 1141, 1143, 1145, 1147, 1669, 1673, 1677, 1681, 1685, 1689, 1693,
    1697, 1701, 1705, 1709, 1713, 1717, 1721, 1725, 1729, 1733, 1737, 1741, 1745, 1749, 1753, 1757,
    1761, 1765, 1769, 1773, 1149, 1151, 1153, 1155, 1157, 1159, 1161, 1163, 1165, 1167, 1169, 1171,
    1173, 1175, 1177, 1179, 1181, 1183, 1185, 1187, 1189, 1191, 1193, 1195, 1197, 1199, 1201, 1203,
    1205, 1207, 1209, 1211, 1213, 1215, 1217, 1219, 1221, 1223, 1225, 1227, 1229, 1231, 1233, 1235,
    1237, 1239, 1241, 1243, 1245, 1247, 1249, 1251, 1253, 1255, 1257, 1259, 1261, 1263, 1265, 1267,
    1269, 1271, 1273, 1275, 1277, 1279, 1281, 1283, 1285, 1287, 1289, 1291, 1293, 1295, 1297, 1299,
    1301, 1303, 1305, 1307, 1309, 1311, 1313, 1315, 1317, 1319, 1321, 1323, 1325, 1327, 1329, 1331,
    1333, 1335, 1337, 1339, 1341, 1343, 1345, 1347, 1349, 1351, 1353, 1355, 1357, 1359, 1361, 1363,
    1365, 1367, 1369, 1371, 1373, 1375, 1377, 1379, 1381, 1383, 1385, 1387, 1389, 1391, 1393, 1395,
    1397, 1399, 1401, 1403, 1405, 1407, 1409, 1411, 1413, 1415, 1417, 1419, 1421, 1423, 1425, 1427,
    1429, 1431, 1433, 1435, 1437, 1439, 1441, 1443, 1445, 1447, 1449, 1451, 1453, 1455, 1457, 1459,
    1461, 1463, 1465, 1467, 1469, 1471, 1473, 1475, 1477, 1479, 1481, 1483, 1485, 1487, 1489, 1491,
    1493, 1495, 1497, 1499, 1501, 1503, 1505, 1507, 1509, 1511, 1513, 1515, 1517, 1519, 1521, 1523,
    1525, 1527, 1529, 1531, 1533, 1535, 1537, 1539, 1541, 1543, 1545, 1547, 1549, 1551, 1553, 1555,
    1557, 1559, 1561, 1563, 1565, 1567, 1569, 1571, 1573, 1575, 1577, 1579, 1581, 1583, 1585, 1587,
    1589, 1591, 1593, 1595, 1597, 1599, 1601, 1603, 1605, 1607, 1609, 1611, 1613, 1615, 1617, 1619,
    1621, 1623, 1625, 1627, 1629, 1631, 1633, 1635, 1637, 1639, 1641, 1643, 1645, 1647, 1649, 1651,
    1653, 1655, 1657, 1659, 1661, 1663, 1665, 1667, 1775, 1777, 1779, 1781, 1783, 1785, 1787, 1789,
    1791, 1793, 1795, 1797, 1799, 1801, 1803, 1805, 1807, 1809, 1811, 1813, 1815, 1817, 1819, 1821,
    1823, 1825, 1827, 1829, 1831, 1833, 1835, 1837, 1839, 1841, 1843, 1845, 1847, 1849, 1851, 1853,
];
static CODEPOINT_INDEX: [u16; 649] = [
    398, 602, 28, 306, 309, 30, 465, 466, 379, 435, 24, 599, 589, 590, 591, 592, 593, 594, 595,
    596, 597, 598, 403, 436, 421, 402, 409, 397, 600, 329, 330, 573, 634, 331, 638, 640, 333, 334,
    646, 335, 650, 336, 337, 338, 339, 581, 583, 664, 340, 668, 670, 672, 341, 676, 332, 461, 473,
    463, 524, 311, 680, 682, 684, 686, 688, 690, 692, 587, 696, 698, 700, 702, 704, 706, 708, 710,
    712, 714, 716, 718, 720, 722, 724, 726, 728, 730, 453, 456, 520, 426, 567, 168, 48, 519, 342,
    146, 564, 384, 50, 1854, 385, 1856, 161, 383, 1858, 528, 529, 523, 521, 517, 518, 522, 526,
    527, 318, 319, 320, 321, 322, 323, 324, 325, 326, 327, 328, 348, 349, 350, 351, 372, 353, 354,
    355, 356, 357, 358, 359, 360, 361, 362, 363, 364, 376, 365, 366, 367, 377, 369, 370, 371, 373,
    368, 374, 175, 176, 375, 352, 305, 52, 55, 510, 316, 153, 525, 34, 46, 38, 41, 44, 150, 151,
    32, 172, 174, 173, 152, 408, 483, 422, 485, 80, 487, 95, 86, 89, 92, 135, 136, 276, 292, 277,
    293, 84, 87, 88, 278, 294, 289, 139, 282, 297, 280, 295, 281, 296, 90, 93, 299, 284, 91, 94,
    300, 285, 291, 274, 273, 283, 290, 298, 279, 96, 137, 140, 138, 75, 484, 78, 486, 82, 488, 275,
    303, 301, 272, 271, 33, 160, 442, 35, 149, 346, 36, 410, 19, 240, 499, 489, 500, 380, 62, 250,
    251, 66, 382, 69, 393, 20, 315, 314, 148, 159, 8, 107, 12, 124, 389, 390, 386, 387, 495, 497,
    498, 504, 505, 506, 239, 242, 3, 191, 71, 106, 247, 7, 405, 123, 222, 182, 11, 207, 206, 17,
    188, 190, 189, 218, 219, 220, 0, 419, 406, 177, 208, 99, 116, 9, 10, 235, 97, 114, 425, 424,
    180, 211, 185, 215, 1, 2, 196, 226, 198, 228, 102, 119, 411, 412, 413, 414, 415, 416, 112, 128,
    64, 195, 225, 15, 16, 65, 67, 440, 63, 441, 443, 439, 444, 265, 264, 256, 255, 270, 445, 21,
    22, 344, 4, 202, 232, 203, 108, 109, 130, 125, 200, 230, 133, 134, 286, 287, 288, 267, 438,
    437, 491, 490, 493, 494, 449, 25, 450, 257, 13, 258, 259, 260, 261, 192, 263, 262, 194, 224,
    252, 253, 236, 183, 213, 184, 214, 186, 216, 197, 227, 103, 120, 100, 117, 104, 121, 110, 126,
    111, 127, 515, 513, 514, 471, 472, 469, 470, 18, 14, 59, 58, 145, 1877, 1878, 1879, 1880, 163,
    164, 156, 165, 154, 141, 241, 452, 155, 143, 238, 451, 144, 68, 158, 47, 45, 43, 40, 37, 39,
    42, 170, 571, 394, 399, 61, 60, 74, 77, 81, 76, 79, 83, 85, 157, 503, 502, 501, 492, 507, 72,
    254, 178, 209, 181, 212, 98, 115, 101, 118, 187, 217, 179, 210, 5, 6, 131, 132, 199, 229, 105,
    122, 193, 223, 113, 129, 1867, 1868, 1862, 1861, 1863, 1864, 1866, 1865, 1869, 1872, 1870,
    1874, 1871, 1873, 1860, 1875, 1876, 613, 621, 29, 307, 310, 31, 616, 617, 615, 620, 627, 618,
    626, 625, 603, 604, 605, 606, 607, 608, 609, 610, 611, 612, 623, 622, 467, 619, 468, 624, 614,
    629, 631, 574, 635, 637, 639, 641, 576, 645, 647, 649, 651, 653, 578, 657, 580, 582, 584, 665,
    667, 669, 671, 673, 675, 677, 586, 462, 481, 464, 482, 312, 681, 683, 685, 687, 689, 691, 693,
    588, 697, 699, 701, 703, 705, 707, 709, 711, 713, 715, 717, 719, 721, 723, 725, 727, 729, 731,
    454, 476, 457, 480, 429, 569, 169, 49, 549, 147, 565, 51, 546, 536, 1855, 538, 1857, 532, 533,
    162, 535, 1859, 530, 537, 534, 531, 541, 548, 543, 539, 540, 544, 545, 547, 542, 550, 552, 554,
    556, 558, 560, 562, 53, 56, 509, 479, 27, 516, 551, 171, 572,
];

const _: () = {
    assert!(symbols_data::SYMBOLS.len() <= 4_096);
    assert!(symbols_data::SYMBOLS.len() <= u16::MAX as usize);
    assert!(std::mem::size_of_val(&NAME_INDEX) + std::mem::size_of_val(&CODEPOINT_INDEX) <= 16_384);
};
