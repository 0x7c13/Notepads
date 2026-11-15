//! Mermaid `createText` tokenization shared by measurement and SVG emission.

use super::is_ecmascript_whitespace;

/// Borrowed tokens equivalent to Mermaid's non-Markdown
/// `line.trim().match(/<[^>]+>|[^\s<>]+/g)` operation.
pub(crate) struct NonMarkdownSvgWords<'a> {
    line: &'a str,
    offset: usize,
    closing_angle_exhausted: bool,
}

impl<'a> NonMarkdownSvgWords<'a> {
    fn new(line: &'a str) -> Self {
        Self {
            line,
            offset: 0,
            closing_angle_exhausted: false,
        }
    }
}

impl<'a> Iterator for NonMarkdownSvgWords<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        while self.offset < self.line.len() {
            let rest = &self.line[self.offset..];
            let character = rest
                .chars()
                .next()
                .expect("an in-bounds UTF-8 offset starts with a character");

            if character == '<' {
                if !self.closing_angle_exhausted {
                    if let Some(relative_end) = rest[1..].find('>') {
                        let end = relative_end + 1;
                        if end > 1 {
                            let token = &rest[..=end];
                            self.offset += end + 1;
                            return Some(token);
                        }
                    } else {
                        // Once the remaining suffix has no `>`, no later `<` can start a tag.
                        // Remember that fact so malformed input stays linear instead of searching
                        // the same shrinking suffix for every delimiter.
                        self.closing_angle_exhausted = true;
                    }
                }
                self.offset += character.len_utf8();
                continue;
            }

            if character == '>' || is_ecmascript_whitespace(character) {
                self.offset += character.len_utf8();
                continue;
            }

            let start = self.offset;
            self.offset += character.len_utf8();
            while self.offset < self.line.len() {
                let next = self.line[self.offset..]
                    .chars()
                    .next()
                    .expect("an in-bounds UTF-8 offset starts with a character");
                if next == '<' || next == '>' || is_ecmascript_whitespace(next) {
                    break;
                }
                self.offset += next.len_utf8();
            }
            return Some(&self.line[start..self.offset]);
        }

        None
    }
}

pub(crate) fn non_markdown_svg_words(line: &str) -> NonMarkdownSvgWords<'_> {
    NonMarkdownSvgWords::new(line)
}

#[cfg(test)]
mod tests {
    use super::non_markdown_svg_words;

    #[test]
    fn tokenizer_matches_mermaid_non_markdown_word_boundaries() {
        let words =
            non_markdown_svg_words(" <strong>A\tB\u{00A0}C\u{2000}D</strong> x <em>E F</em> y ")
                .collect::<Vec<_>>();

        assert_eq!(
            words,
            [
                "<strong>",
                "A",
                "B",
                "C",
                "D",
                "</strong>",
                "x",
                "<em>",
                "E",
                "F",
                "</em>",
                "y",
            ]
        );
    }

    #[test]
    fn tokenizer_keeps_next_line_control_inside_a_word() {
        assert_eq!(
            non_markdown_svg_words("A\u{0085}B").collect::<Vec<_>>(),
            ["A\u{0085}B"]
        );
    }

    #[test]
    fn tokenizer_skips_unmatched_angle_delimiters_like_the_upstream_regex() {
        assert_eq!(
            non_markdown_svg_words("<> <open tail end").collect::<Vec<_>>(),
            ["open", "tail", "end"]
        );
    }

    #[test]
    fn tokenizer_keeps_many_unmatched_angle_delimiters_linear() {
        let mut input = "< ".repeat(8_192);
        input.push_str("tail");

        assert_eq!(non_markdown_svg_words(&input).collect::<Vec<_>>(), ["tail"]);
    }
}
