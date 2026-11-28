use syntect::easy::HighlightLines;
use syntect::highlighting::Theme;
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// Common helper for benchmarking highlighting.
pub fn do_highlight(
    s: &str,
    syntax_set: &SyntaxSet,
    syntax: &SyntaxReference,
    theme: &Theme,
) -> usize {
    let mut h = HighlightLines::new(syntax, theme);
    let mut count = 0;
    for line in s.lines() {
        let regions = h.highlight_line(line, syntax_set).unwrap();
        count += regions.len();
    }
    count
}
