use crate::model::{C4TextRenderPlan, C4TextRowLayout};
use crate::text::{TextMeasurer, TextStyle, WrapMode, measure_mermaid_text_dimensions};
use merman_core::diagrams::c4::{C4DiagramRenderModel, C4ShapeRenderModel};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;

mod config;

pub(crate) use config::{
    C4_DEFAULT_FONT_FAMILY, C4_ELEMENT_TYPES, C4ConfigView, C4LayoutSettings, default_use_max_width,
};

type C4Model = C4DiagramRenderModel;
type C4Conf = C4LayoutSettings;

/// The unified Mermaid shape used to draw a C4 element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum C4NodeShape {
    Rounded,
    Framed,
    Person,
    Cylinder,
    HorizontalCylinder,
}

fn value_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn keyword_shape(value: Option<&str>) -> Option<C4NodeShape> {
    match value?.to_ascii_lowercase().as_str() {
        "person" => Some(C4NodeShape::Person),
        "box" | "rounded" => Some(C4NodeShape::Rounded),
        "component" => Some(C4NodeShape::Framed),
        "cylinder" | "database" | "db" => Some(C4NodeShape::Cylinder),
        "queue" | "pipe" => Some(C4NodeShape::HorizontalCylinder),
        _ => None,
    }
}

/// Resolves `$shape`, `$sprite`, `$tags`, and the legacy C4 element type in the
/// same order as Mermaid's `c4ShapeAdapter`.
pub(crate) fn c4_node_shape(shape: &C4ShapeRenderModel) -> C4NodeShape {
    keyword_shape(value_string(shape.shape.as_ref()))
        .or_else(|| keyword_shape(value_string(shape.sprite.as_ref())))
        .or_else(|| {
            value_string(shape.tags.as_ref()).and_then(|tags| {
                tags.split(',')
                    .find_map(|tag| keyword_shape(Some(tag.trim())))
            })
        })
        .unwrap_or_else(|| match shape.type_c4_shape.as_str() {
            "person" | "external_person" => C4NodeShape::Person,
            value if value.ends_with("_db") => C4NodeShape::Cylinder,
            value if value.ends_with("_queue") => C4NodeShape::HorizontalCylinder,
            _ => C4NodeShape::Rounded,
        })
}

fn c4_stereotype_name(type_c4_shape: &str) -> String {
    let base = type_c4_shape
        .strip_prefix("external_")
        .unwrap_or(type_c4_shape);
    let base = base
        .strip_suffix("_db")
        .or_else(|| base.strip_suffix("_queue"))
        .unwrap_or(base);
    match base {
        "person" => "Person".to_string(),
        "system" => "Software System".to_string(),
        "container" => "Container".to_string(),
        "component" => "Component".to_string(),
        other => other.replace('_', " "),
    }
}

/// Returns the structured stereotype line used by the unified C4 label helper.
pub(crate) fn c4_stereotype_text(shape: &C4ShapeRenderModel) -> String {
    let stereotype = c4_stereotype_name(shape.type_c4_shape.as_str());
    match shape
        .techn
        .as_ref()
        .map(|text| text.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(technology) => format!("[{stereotype}: {technology}]"),
        None => format!("[{stereotype}]"),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TextMeasure {
    width: f64,
    height: f64,
    line_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct UnifiedTextMeasure {
    pub(crate) metrics: TextMeasure,
    pub(crate) render_plan: C4TextRenderPlan,
}

fn measure_c4_text(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    wrap: bool,
    text_limit_width: f64,
) -> TextMeasure {
    let dimensions = measure_mermaid_text_dimensions(measurer, text, style);
    if wrap {
        let m = measurer.measure_wrapped(text, style, Some(text_limit_width), WrapMode::SvgLike);
        let line_count = m.line_count.max(1);
        return TextMeasure {
            width: text_limit_width,
            height: dimensions.line_height.max(0) as f64 * line_count as f64,
            line_count,
        };
    }

    TextMeasure {
        width: dimensions.width.max(0) as f64,
        height: dimensions.height.max(0) as f64,
        line_count: crate::text::split_html_br_lines(text).len().max(1),
    }
}

/// Measures one section of the unified C4 label.
///
/// Unlike the legacy C4 path, the unified label helper sizes from the rendered
/// section's actual wrapped bounding box; the configured wrap width is only a
/// line-breaking constraint. Keep the legacy helper above unchanged for
/// boundaries and relationships, which still use Mermaid's fixed-width path.
fn measure_c4_unified_text(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    wrap: bool,
    text_limit_width: f64,
) -> UnifiedTextMeasure {
    let source_lines = c4_source_word_lines(text);
    // Mermaid's `createText` installs `font-weight="normal"` and
    // `font-style="normal"` on the temporary tspans used by its line-fitting
    // probe. `c4LabelHelper` removes those attributes only after wrapping, so
    // the final section can inherit C4 typography such as the bold name style.
    // Keep those two measurement phases distinct: wrapping follows the
    // temporary normal text, while the final bounds use the rendered style.
    let mut wrapping_style = style.clone();
    wrapping_style.font_weight = Some("normal".to_string());
    wrapping_style.font_style = Some("normal".to_string());
    let rows = c4_wrap_source_word_lines(
        measurer,
        &source_lines,
        &wrapping_style,
        wrap.then_some(text_limit_width),
    );
    let visible = c4_visible_text(&rows);

    if visible.is_empty() {
        return UnifiedTextMeasure {
            metrics: TextMeasure {
                width: 0.0,
                height: 0.0,
                line_count: 0,
            },
            render_plan: C4TextRenderPlan {
                rows,
                bbox_x: 0.0,
                bbox_y: 0.0,
            },
        };
    }

    // The rows above are the source of truth for both layout and SVG. Measuring the joined
    // visible rows avoids asking the host to wrap the original source a second time.
    let measured = measurer.measure_wrapped(&visible, style, None, WrapMode::SvgLike);
    let mut left: f64 = 0.0;
    let mut right: f64 = 0.0;
    for row in &rows {
        let row_text = c4_visible_row(&row.words);
        if row_text.is_empty() {
            continue;
        }
        let (row_left, row_right) = measurer.measure_svg_text_bbox_x(&row_text, style);
        left = left.max(row_left.max(0.0));
        right = right.max(row_right.max(0.0));
    }
    let first_row = rows
        .first()
        .map(|row| c4_visible_row(&row.words))
        .unwrap_or_default();
    let bbox_y = measurer.measure_svg_create_text_bbox_y_offset_px(&first_row, style);
    let metrics = TextMeasure {
        width: measured.width.max(left + right).max(0.0),
        height: measured.height.max(0.0),
        line_count: rows.len().max(1),
    };
    UnifiedTextMeasure {
        metrics,
        render_plan: C4TextRenderPlan {
            rows,
            bbox_x: -left,
            bbox_y,
        },
    }
}

fn c4_source_word_lines(text: &str) -> Vec<Vec<String>> {
    // Mermaid's nonMarkdownToLines treats literal `\\n`, actual newlines and `<br>` variants as
    // explicit line boundaries before tokenizing each trimmed line.
    let normalized = text.replace("\\n", "\n");
    crate::text::split_html_br_lines(&normalized)
        .into_iter()
        .map(|line| {
            crate::text::non_markdown_svg_words(line.trim())
                .map(str::to_string)
                .collect()
        })
        .collect()
}

fn c4_visible_row(row: &[String]) -> String {
    let mut visible = String::new();
    for (index, word) in row.iter().enumerate() {
        if index > 0 {
            visible.push(' ');
        }
        let decoded = crate::entities::decode_svg_text_content_entities(word);
        visible.push_str(decoded.as_ref());
    }
    visible
}

fn c4_visible_text(rows: &[C4TextRowLayout]) -> String {
    rows.iter()
        .map(|row| c4_visible_row(&row.words))
        .collect::<Vec<_>>()
        .join("\n")
}

fn c4_row_width(measurer: &dyn TextMeasurer, row: &[String], style: &TextStyle) -> f64 {
    measurer.measure_svg_text_computed_length_px(&c4_visible_row(row), style)
}

fn c4_split_long_word(
    measurer: &dyn TextMeasurer,
    word: &str,
    style: &TextStyle,
    max_width: f64,
) -> Vec<String> {
    let boundaries = word
        .grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain(std::iter::once(word.len()))
        .collect::<Vec<_>>();
    if boundaries.len() <= 1 {
        return vec![word.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start + 1 < boundaries.len() {
        let mut end = start + 1;
        let mut best = end;
        while end < boundaries.len() {
            let candidate = &word[boundaries[start]..boundaries[end]];
            if measurer.measure_svg_text_computed_length_px(candidate, style) <= max_width {
                best = end;
                end += 1;
            } else {
                break;
            }
        }
        chunks.push(word[boundaries[start]..boundaries[best]].to_string());
        start = best;
    }
    chunks
}

fn c4_wrap_source_word_lines(
    measurer: &dyn TextMeasurer,
    source_lines: &[Vec<String>],
    style: &TextStyle,
    max_width: Option<f64>,
) -> Vec<C4TextRowLayout> {
    let Some(max_width) = max_width.filter(|width| width.is_finite() && *width > 0.0) else {
        return source_lines
            .iter()
            .cloned()
            .map(|words| C4TextRowLayout { words })
            .collect();
    };

    let mut wrapped = Vec::new();
    for source_line in source_lines {
        if source_line.is_empty() {
            wrapped.push(C4TextRowLayout { words: Vec::new() });
            continue;
        }
        if c4_row_width(measurer, source_line, style) <= max_width {
            wrapped.push(C4TextRowLayout {
                words: source_line.clone(),
            });
            continue;
        }

        let mut current = Vec::new();
        for word in source_line {
            let mut candidate = current.clone();
            candidate.push(word.clone());
            if c4_row_width(measurer, &candidate, style) <= max_width {
                current.push(word.clone());
                continue;
            }
            if !current.is_empty() {
                wrapped.push(C4TextRowLayout {
                    words: std::mem::take(&mut current),
                });
            }
            if c4_row_width(measurer, std::slice::from_ref(word), style) <= max_width {
                current.push(word.clone());
            } else {
                for chunk in c4_split_long_word(measurer, word, style, max_width) {
                    wrapped.push(C4TextRowLayout { words: vec![chunk] });
                }
            }
        }
        if !current.is_empty() {
            wrapped.push(C4TextRowLayout { words: current });
        }
    }

    if wrapped.is_empty() {
        vec![C4TextRowLayout { words: Vec::new() }]
    } else {
        wrapped
    }
}

mod layout;
pub(crate) use layout::layout_c4_diagram_typed;

#[cfg(test)]
mod tests {
    use crate::text::{TextMeasurer, TextMetrics};

    use super::{TextStyle, measure_c4_text, measure_c4_unified_text};

    struct C4ProbeMeasurer;

    impl TextMeasurer for C4ProbeMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: 0.0,
                height: 0.0,
                line_count: 3,
            }
        }

        fn measure_svg_simple_text_bbox_width_px(&self, _text: &str, style: &TextStyle) -> f64 {
            if style.font_family.as_deref() == Some("sans-serif") {
                80.0
            } else {
                120.0
            }
        }

        fn measure_svg_simple_text_bbox_height_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            19.0
        }
    }

    #[test]
    fn c4_unwrapped_text_consumes_shared_mermaid_dimensions() {
        let measured = measure_c4_text(
            &C4ProbeMeasurer,
            "configured text",
            &TextStyle::default(),
            false,
            500.0,
        );

        assert_eq!(measured.width, 120.0);
        assert_eq!(measured.height, 19.0);
        assert_eq!(measured.line_count, 1);
    }

    #[test]
    fn c4_wrapped_text_uses_shared_bbox_line_height() {
        let measured = measure_c4_text(
            &C4ProbeMeasurer,
            "Allows customers to view information about their bank accounts, and make payments.",
            &TextStyle::default(),
            true,
            200.0,
        );

        assert_eq!(measured.width, 200.0);
        assert_eq!(measured.height, 57.0);
        assert_eq!(measured.line_count, 3);
    }

    struct C4UnifiedWrapProbeMeasurer;

    impl TextMeasurer for C4UnifiedWrapProbeMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            let width_per_character = if style.font_weight.as_deref() == Some("bold") {
                8.0
            } else {
                7.0
            };
            let lines = text.lines().collect::<Vec<_>>();
            TextMetrics {
                width: lines
                    .iter()
                    .map(|line| line.chars().count() as f64 * width_per_character)
                    .fold(0.0, f64::max),
                height: lines.len().max(1) as f64 * 20.0,
                line_count: lines.len().max(1),
            }
        }
    }

    #[test]
    fn c4_unified_text_wraps_before_inheriting_final_section_weight() {
        let style = TextStyle {
            font_weight: Some("bold".to_string()),
            ..TextStyle::default()
        };
        let measured = measure_c4_unified_text(
            &C4UnifiedWrapProbeMeasurer,
            "Mainframe Banking System",
            &style,
            true,
            176.0,
        );

        assert_eq!(measured.metrics.line_count, 1);
        assert_eq!(measured.render_plan.rows.len(), 1);
        assert!(measured.metrics.width > 176.0);
    }
}
