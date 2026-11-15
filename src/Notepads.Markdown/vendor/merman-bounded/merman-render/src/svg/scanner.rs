/// Returns the byte index of the first unquoted `>` at or after `start`.
pub(crate) fn find_tag_end(input: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in input.get(start..)?.char_indices() {
        match character {
            '\'' | '"' if quote == Some(character) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(character),
            '>' if quote.is_none() => return Some(start + offset),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::find_tag_end;

    #[test]
    fn tag_end_ignores_quoted_delimiters_and_reports_utf8_byte_offset() {
        let input = "<tag title='x > y'>\u{03bb}</tag>";
        assert_eq!(find_tag_end(input, 0), input.find(">\u{03bb}"));
    }
}
