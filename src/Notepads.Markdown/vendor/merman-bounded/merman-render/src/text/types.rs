//! Shared text measurement types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WrapMode {
    #[default]
    SvgLike,
    /// SVG `<text>` behaves as a single shaping run (no whitespace-to-`<tspan>` tokenization).
    ///
    /// Mermaid uses this behavior in some diagrams (e.g. sequence message labels), where the
    /// resulting `getBBox()` width differs measurably from per-word `<tspan>` tokenization.
    SvgLikeSingleRun,
    HtmlLike,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_family: Option<String>,
    pub font_size: f64,
    pub font_weight: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_style: Option<String>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TextMetrics {
    pub width: f64,
    pub height: f64,
    pub line_count: usize,
}

impl TextMetrics {
    /// Apply Mermaid's `labelHelper` minimum without changing the text's height or wrapping.
    /// The caller selects shapes that use `labelHelper`; explicit shape widths remain authoritative.
    pub(crate) fn with_label_min_width(
        mut self,
        label: &str,
        min_width: f64,
        explicit_width: Option<f64>,
    ) -> Self {
        if !label.is_empty() && !explicit_width.is_some_and(|width| width != 0.0) {
            self.width = self.width.max(min_width);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::TextMetrics;

    #[test]
    fn label_minimum_preserves_empty_and_explicit_width_contracts() {
        let metrics = TextMetrics {
            width: 20.0,
            height: 16.0,
            line_count: 2,
        };
        for (label, explicit_width, expected) in [
            ("X", None, 120.0),
            ("X", Some(90.0), 20.0),
            ("X", Some(0.0), 120.0),
            ("", None, 20.0),
            (" ", None, 120.0),
        ] {
            let actual = metrics.with_label_min_width(label, 120.0, explicit_width);
            assert_eq!(actual.width, expected);
            assert_eq!(actual.height, metrics.height);
            assert_eq!(actual.line_count, metrics.line_count);
        }
        assert_eq!(metrics.with_label_min_width("X", 10.0, None).width, 20.0);
    }
}
