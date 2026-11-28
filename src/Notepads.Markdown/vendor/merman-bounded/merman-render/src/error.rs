use crate::Result;
use crate::model::ErrorDiagramLayout;
use crate::text::TextMeasurer;

pub const UPSTREAM_MERMAID_VERSION: &str = merman_core::baseline::PINNED_MERMAID_BASELINE_VERSION;

pub(crate) fn layout_error_diagram_typed(
    semantic: &merman_core::diagrams::error_diagram::ErrorDiagramRenderModel,
    _effective_config: &serde_json::Value,
    _measurer: &dyn TextMeasurer,
) -> Result<ErrorDiagramLayout> {
    let lines = wrap_error_message(semantic.error_message.as_deref().unwrap_or_default());
    let height = if lines.is_empty() {
        512.0
    } else {
        500.0 + lines.len() as f64 * 56.0
    };
    Ok(ErrorDiagramLayout {
        viewbox_width: 2412.0,
        viewbox_height: height,
        max_width_px: height,
    })
}

/// Wraps the upstream error text at 75 Unicode code points and at most four lines.
pub(crate) fn wrap_error_message(message: &str) -> Vec<String> {
    const MAX_LINE_LENGTH: usize = 75;
    const MAX_LINES: usize = 4;
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_length = 0;
    for token in
        message.split(|ch: char| (ch.is_whitespace() && ch != '\u{0085}') || ch == '\u{feff}')
    {
        let mut chars = token.chars().peekable();
        while chars.peek().is_some() {
            let word: String = chars.by_ref().take(MAX_LINE_LENGTH).collect();
            let word_length = word.chars().count();
            let separator_length = usize::from(!current.is_empty());
            if current_length + separator_length + word_length > MAX_LINE_LENGTH {
                lines.push(std::mem::take(&mut current));
                if lines.len() == MAX_LINES {
                    let last = &mut lines[MAX_LINES - 1];
                    *last = last.chars().take(MAX_LINE_LENGTH - 3).collect();
                    last.push_str("...");
                    return lines;
                }
                current_length = 0;
            }
            if !current.is_empty() {
                current.push(' ');
                current_length += 1;
            }
            current.push_str(&word);
            current_length += word_length;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::wrap_error_message;

    #[test]
    fn wraps_error_text_using_upstream_word_and_code_point_boundaries() {
        assert!(wrap_error_message("\t \n\u{feff}").is_empty());
        assert_eq!(wrap_error_message("A short error"), ["A short error"]);
        assert_eq!(
            wrap_error_message(&format!(
                "a{}{} c{}",
                " ".repeat(40),
                "b".repeat(40),
                "d".repeat(39)
            )),
            [
                format!("a {}", "b".repeat(40)),
                format!("c{}", "d".repeat(39))
            ],
        );
        assert_eq!(
            wrap_error_message(&"x".repeat(160)),
            ["x".repeat(75), "x".repeat(75), "x".repeat(10)]
        );
        assert_eq!(
            wrap_error_message(&"\u{1f600}".repeat(80)),
            ["\u{1f600}".repeat(75), "\u{1f600}".repeat(5)]
        );
        assert_eq!(wrap_error_message("a\u{0085}b"), ["a\u{0085}b"]);
    }

    #[test]
    fn error_text_is_capped_only_when_more_than_four_lines_are_needed() {
        assert_eq!(
            wrap_error_message(&"x".repeat(300)),
            vec!["x".repeat(75); 4]
        );
        let lines = wrap_error_message(&"x".repeat(301));
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[3], format!("{}...", "x".repeat(72)));
        let lines = wrap_error_message(&vec!["\u{1f600}".repeat(70); 6].join(" "));
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[3], format!("{}...", "\u{1f600}".repeat(70)));
    }
}
