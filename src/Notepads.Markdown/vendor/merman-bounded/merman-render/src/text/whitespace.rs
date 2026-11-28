pub(crate) fn is_html_collapsible_ascii_whitespace(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\u{000C}' | '\r' | ' ')
}

pub(crate) fn is_ecmascript_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

pub(crate) fn trim_html_collapsible_ascii_whitespace(text: &str) -> &str {
    text.trim_matches(is_html_collapsible_ascii_whitespace)
}

pub(crate) fn trim_end_html_collapsible_ascii_whitespace(text: &str) -> &str {
    text.trim_end_matches(is_html_collapsible_ascii_whitespace)
}

pub(crate) fn trim_ecmascript_whitespace(text: &str) -> &str {
    text.trim_matches(is_ecmascript_whitespace)
}

pub(crate) fn trim_start_ecmascript_whitespace(text: &str) -> &str {
    text.trim_start_matches(is_ecmascript_whitespace)
}
