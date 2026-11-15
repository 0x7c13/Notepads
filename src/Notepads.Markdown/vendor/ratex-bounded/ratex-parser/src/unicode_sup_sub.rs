/// Unicode superscript/subscript character mappings.
/// Mirrors KaTeX's unicodeSupOrSub module.
/// Returns `Some((mapped_char, is_subscript))` if the character is a Unicode
/// superscript or subscript; `None` otherwise.
pub fn unicode_sub_sup(c: char) -> Option<(&'static str, bool)> {
    let is_sub = matches!(c, '\u{2080}'..='\u{209C}' | '\u{1D62}'..='\u{1D6A}' | '\u{2C7C}');
    let mapped = match c {
        // ── Subscript digits ──
        '\u{2080}' => "0",
        '\u{2081}' => "1",
        '\u{2082}' => "2",
        '\u{2083}' => "3",
        '\u{2084}' => "4",
        '\u{2085}' => "5",
        '\u{2086}' => "6",
        '\u{2087}' => "7",
        '\u{2088}' => "8",
        '\u{2089}' => "9",
        // ── Subscript operators ──
        '\u{208A}' => "+",
        '\u{208B}' => "\u{2212}", // minus sign
        '\u{208C}' => "=",
        '\u{208D}' => "(",
        '\u{208E}' => ")",
        // ── Subscript letters ──
        '\u{2090}' => "a",
        '\u{2091}' => "e",
        '\u{2092}' => "o",
        '\u{2093}' => "x",
        '\u{2095}' => "h",
        '\u{2096}' => "k",
        '\u{2097}' => "l",
        '\u{2098}' => "m",
        '\u{2099}' => "n",
        '\u{209A}' => "p",
        '\u{209B}' => "s",
        '\u{209C}' => "t",
        '\u{1D62}' => "i",
        '\u{1D63}' => "r",
        '\u{1D64}' => "u",
        '\u{1D65}' => "v",
        '\u{2C7C}' => "j",
        // ── Subscript Greek ──
        '\u{1D66}' => "\u{03B2}", // β
        '\u{1D67}' => "\u{03B3}", // γ
        '\u{1D68}' => "\u{03C1}", // ρ
        '\u{1D69}' => "\u{03C6}", // φ
        '\u{1D6A}' => "\u{03C7}", // χ

        // ── Superscript digits ──
        '\u{2070}' => "0",
        '\u{00B9}' => "1",
        '\u{00B2}' => "2",
        '\u{00B3}' => "3",
        '\u{2074}' => "4",
        '\u{2075}' => "5",
        '\u{2076}' => "6",
        '\u{2077}' => "7",
        '\u{2078}' => "8",
        '\u{2079}' => "9",
        // ── Superscript operators ──
        '\u{207A}' => "+",
        '\u{207B}' => "\u{2212}", // minus sign
        '\u{207C}' => "=",
        '\u{207D}' => "(",
        '\u{207E}' => ")",
        // ── Superscript letters (lowercase) ──
        '\u{2071}' => "i",
        '\u{207F}' => "n",
        '\u{1D43}' => "a",
        '\u{1D47}' => "b",
        '\u{1D48}' => "d",
        '\u{1D49}' => "e",
        '\u{1D4D}' => "g",
        '\u{02B0}' => "h",
        '\u{02B2}' => "j",
        '\u{1D4F}' => "k",
        '\u{02E1}' => "l",
        '\u{1D50}' => "m",
        '\u{1D52}' => "o",
        '\u{1D56}' => "p",
        '\u{02B3}' => "r",
        '\u{02E2}' => "s",
        '\u{1D57}' => "t",
        '\u{1D58}' => "u",
        '\u{1D5B}' => "v",
        '\u{02B7}' => "w",
        '\u{02E3}' => "x",
        '\u{02B8}' => "y",
        // ── Superscript letters (uppercase) ──
        '\u{1D2C}' => "A",
        '\u{1D2E}' => "B",
        '\u{1D30}' => "D",
        '\u{1D31}' => "E",
        '\u{1D33}' => "G",
        '\u{1D34}' => "H",
        '\u{1D35}' => "I",
        '\u{1D36}' => "J",
        '\u{1D37}' => "K",
        '\u{1D38}' => "L",
        '\u{1D39}' => "M",
        '\u{1D3A}' => "N",
        '\u{1D3C}' => "O",
        '\u{1D3E}' => "P",
        '\u{1D3F}' => "R",
        '\u{1D40}' => "T",
        '\u{1D41}' => "U",
        '\u{1D42}' => "W",
        // ── Superscript Greek ──
        '\u{1D5D}' => "\u{03B2}", // β
        '\u{1D5E}' => "\u{03B3}", // γ
        '\u{1D5F}' => "\u{03B4}", // δ
        '\u{1D60}' => "\u{03C6}", // φ
        '\u{1D61}' => "\u{03C7}", // χ
        '\u{1DBF}' => "\u{03B8}", // θ

        _ => return None,
    };
    Some((mapped, is_sub))
}
