// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. See project LICENSE.
use crate::text::{TextMetrics, TextStyle};
use merman_core::native_collections::OrderedMap;

fn parse_style_decl(s: &str) -> Option<(&str, &str)> {
    crate::mermaid_style::parse_safe_style_decl(s)
}

fn normalize_css_font_family(font_family: &str) -> String {
    let font_family = font_family.trim().trim_end_matches(';').trim();
    if crate::mermaid_style::is_safe_css_font_family_value(font_family) {
        font_family.to_string()
    } else {
        String::new()
    }
}

pub(crate) fn flowchart_split_mermaid_style_decls(s: &str) -> impl Iterator<Item = &str> {
    fn looks_like_key_start(s: &str) -> bool {
        let s = s.trim_start();
        let Some((k, _)) = s.split_once(':') else {
            return false;
        };
        let k = k.trim();
        !k.is_empty()
            && k.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    }

    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        if ch != ',' {
            continue;
        }
        if looks_like_key_start(&s[i + 1..]) {
            let p = s[start..i].trim();
            if !p.is_empty() {
                parts.push(p);
            }
            start = i + 1;
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts.into_iter()
}

fn apply_text_style_decl(style: &mut std::borrow::Cow<'_, TextStyle>, key: &str, value: &str) {
    match key.trim().to_ascii_lowercase().as_str() {
        "font-size" => {
            let inherited_px = style.as_ref().font_size;
            if let Some(px) = crate::mermaid_style::parse_css_font_size_px(value, inherited_px) {
                style.to_mut().font_size = px;
            }
        }
        "font-family" => {
            style.to_mut().font_family = Some(normalize_css_font_family(value));
        }
        "font-weight" => {
            style.to_mut().font_weight = Some(value.trim().to_string());
        }
        "font-style" => {
            style.to_mut().font_style = Some(value.trim().to_string());
        }
        _ => {}
    }
}

fn flowchart_effective_text_style_for_class_names<'a>(
    base: &'a TextStyle,
    class_defs: &OrderedMap<String, Vec<String>>,
    class_names: impl IntoIterator<Item = &'a str>,
    inline_styles: &[String],
) -> std::borrow::Cow<'a, TextStyle> {
    let mut style = std::borrow::Cow::Borrowed(base);

    for class in class_names {
        let Some(decls) = class_defs.get(class) else {
            continue;
        };
        for d in decls {
            for d in flowchart_split_mermaid_style_decls(d) {
                let Some((k, v)) = parse_style_decl(d) else {
                    continue;
                };
                apply_text_style_decl(&mut style, k, v);
            }
        }
    }

    for d in inline_styles {
        for d in flowchart_split_mermaid_style_decls(d) {
            let Some((k, v)) = parse_style_decl(d) else {
                continue;
            };
            apply_text_style_decl(&mut style, k, v);
        }
    }

    style
}

pub(crate) fn flowchart_effective_node_class_names<'a>(
    class_defs: &'a OrderedMap<String, Vec<String>>,
    classes: &'a [String],
) -> Vec<&'a str> {
    let mut effective: Vec<&'a str> = Vec::with_capacity(classes.len() + 2);
    if class_defs.contains_key("default") {
        effective.push("default");
    }
    if class_defs.contains_key("node") {
        effective.push("node");
    }
    effective.extend(classes.iter().map(|class| class.as_str()));
    effective
}

pub(crate) fn flowchart_effective_text_style_for_node_classes<'a>(
    base: &'a TextStyle,
    class_defs: &'a OrderedMap<String, Vec<String>>,
    classes: &'a [String],
    inline_styles: &[String],
) -> std::borrow::Cow<'a, TextStyle> {
    let effective_classes = flowchart_effective_node_class_names(class_defs, classes);
    if effective_classes.is_empty() && inline_styles.is_empty() {
        return std::borrow::Cow::Borrowed(base);
    }
    flowchart_effective_text_style_for_class_names(
        base,
        class_defs,
        effective_classes,
        inline_styles,
    )
}

pub(crate) fn flowchart_effective_text_style_for_classes<'a>(
    base: &'a TextStyle,
    class_defs: &OrderedMap<String, Vec<String>>,
    classes: &'a [String],
    inline_styles: &[String],
) -> std::borrow::Cow<'a, TextStyle> {
    if classes.is_empty() && inline_styles.is_empty() {
        return std::borrow::Cow::Borrowed(base);
    }

    flowchart_effective_text_style_for_class_names(
        base,
        class_defs,
        classes.iter().map(|class| class.as_str()),
        inline_styles,
    )
}

/// Mermaid first compiles edge classes and then applies the concatenated `linkStyle default`
/// and per-edge declarations. The resulting style is applied after SVG line wrapping, but it owns
/// the final text bbox used by the layout graph.
pub(crate) fn flowchart_effective_edge_label_text_style<'a>(
    base: &'a TextStyle,
    class_defs: &OrderedMap<String, Vec<String>>,
    classes: &'a [String],
    default_edge_styles: &[String],
    edge_styles: &[String],
) -> std::borrow::Cow<'a, TextStyle> {
    if classes.is_empty() && default_edge_styles.is_empty() && edge_styles.is_empty() {
        return std::borrow::Cow::Borrowed(base);
    }

    let mut style = flowchart_effective_text_style_for_class_names(
        base,
        class_defs,
        classes.iter().map(|class| class.as_str()),
        &[],
    );
    for declaration in default_edge_styles.iter().chain(edge_styles) {
        for declaration in flowchart_split_mermaid_style_decls(declaration) {
            let Some((key, value)) = parse_style_decl(declaration) else {
                continue;
            };
            apply_text_style_decl(&mut style, key, value);
        }
    }
    style
}

/// Mermaid's Swimlane adapter moves an edge label onto a fresh `labelRect` node and copies only
/// the first entry from the already-concatenated default/edge `labelStyle` array. Classes and later
/// style entries remain on the original edge and must not affect the label node's measurement.
pub(crate) fn flowchart_swimlane_label_rect_text_style<'a>(
    base: &'a TextStyle,
    default_edge_styles: &[String],
    edge_styles: &[String],
) -> std::borrow::Cow<'a, TextStyle> {
    let Some(first_style) = default_edge_styles.first().or_else(|| edge_styles.first()) else {
        return std::borrow::Cow::Borrowed(base);
    };

    let mut style = std::borrow::Cow::Borrowed(base);
    for declaration in flowchart_split_mermaid_style_decls(first_style) {
        let Some((key, value)) = parse_style_decl(declaration) else {
            continue;
        };
        apply_text_style_decl(&mut style, key, value);
    }
    style
}

#[derive(Debug, Clone, Copy, Default)]
struct CssBoxEdges {
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
}

impl CssBoxEdges {
    fn set_shorthand(&mut self, values: &[f64]) {
        match values {
            [all] => {
                self.top = *all;
                self.right = *all;
                self.bottom = *all;
                self.left = *all;
            }
            [vertical, horizontal] => {
                self.top = *vertical;
                self.right = *horizontal;
                self.bottom = *vertical;
                self.left = *horizontal;
            }
            [top, horizontal, bottom] => {
                self.top = *top;
                self.right = *horizontal;
                self.bottom = *bottom;
                self.left = *horizontal;
            }
            [top, right, bottom, left] => {
                self.top = *top;
                self.right = *right;
                self.bottom = *bottom;
                self.left = *left;
            }
            _ => {}
        }
    }

    fn horizontal(self) -> f64 {
        self.left + self.right
    }

    fn vertical(self) -> f64 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum HtmlSpanDisplay {
    #[default]
    Inline,
    InlineBlock,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct HtmlSpanBoxStyle {
    display: HtmlSpanDisplay,
    margin: CssBoxEdges,
    padding: CssBoxEdges,
    border_width: CssBoxEdges,
    border_visible: [bool; 4],
    line_height: f64,
}

impl HtmlSpanBoxStyle {
    fn new(font_size: f64) -> Self {
        Self {
            display: HtmlSpanDisplay::Inline,
            margin: CssBoxEdges::default(),
            padding: CssBoxEdges::default(),
            // CSS initializes border width to `medium`, but a `none` border style makes its
            // used width zero until a declaration enables it.
            border_width: CssBoxEdges {
                top: 3.0,
                right: 3.0,
                bottom: 3.0,
                left: 3.0,
            },
            border_visible: [false; 4],
            line_height: font_size.max(1.0) * 1.5,
        }
    }

    fn visible_border_width(self) -> CssBoxEdges {
        CssBoxEdges {
            top: if self.border_visible[0] {
                self.border_width.top
            } else {
                0.0
            },
            right: if self.border_visible[1] {
                self.border_width.right
            } else {
                0.0
            },
            bottom: if self.border_visible[2] {
                self.border_width.bottom
            } else {
                0.0
            },
            left: if self.border_visible[3] {
                self.border_width.left
            } else {
                0.0
            },
        }
    }
}

fn css_box_length_px(raw: &str, font_size: f64, allow_negative: bool) -> Option<f64> {
    let raw = raw.trim().trim_end_matches("!important").trim();
    if raw.eq_ignore_ascii_case("auto") {
        return Some(0.0);
    }
    let lower = raw.to_ascii_lowercase();
    let value = if let Some(value) = lower.strip_suffix("px") {
        value.trim().parse::<f64>().ok()?
    } else if let Some(value) = lower.strip_suffix("rem") {
        value.trim().parse::<f64>().ok()? * font_size
    } else if let Some(value) = lower.strip_suffix("em") {
        value.trim().parse::<f64>().ok()? * font_size
    } else if lower == "0" || lower == "+0" || lower == "-0" {
        0.0
    } else {
        return None;
    };
    if !value.is_finite() || (!allow_negative && value < 0.0) {
        return None;
    }
    Some(value)
}

fn css_box_shorthand_px(raw: &str, font_size: f64, allow_negative: bool) -> Option<Vec<f64>> {
    let values = raw
        .split_ascii_whitespace()
        .map(|value| css_box_length_px(value, font_size, allow_negative))
        .collect::<Option<Vec<_>>>()?;
    (1..=4).contains(&values.len()).then_some(values)
}

fn css_border_width_px(raw: &str, font_size: f64) -> Option<f64> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => css_box_length_px(raw, font_size, false),
    }
}

fn css_border_style_visible(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" | "hidden" => Some(false),
        "dotted" | "dashed" | "solid" | "double" | "groove" | "ridge" | "inset" | "outset" => {
            Some(true)
        }
        _ => None,
    }
}

fn css_border_shorthand(raw: &str, font_size: f64) -> (f64, bool) {
    let mut width = 3.0;
    let mut visible = false;
    for token in raw.split_ascii_whitespace() {
        if let Some(value) = css_border_width_px(token, font_size) {
            width = value;
        }
        if let Some(value) = css_border_style_visible(token) {
            visible = value;
        }
    }
    (width, visible)
}

fn set_border_width_side(edges: &mut CssBoxEdges, side: usize, width: f64) {
    match side {
        0 => edges.top = width,
        1 => edges.right = width,
        2 => edges.bottom = width,
        3 => edges.left = width,
        _ => unreachable!("CSS box side index"),
    }
}

fn apply_html_span_box_decl(
    style: &mut HtmlSpanBoxStyle,
    property: &str,
    value: &str,
    font_size: f64,
) {
    let property = property.trim().to_ascii_lowercase();
    match property.as_str() {
        "display" => {
            style.display = match value.trim().to_ascii_lowercase().as_str() {
                "inline" => HtmlSpanDisplay::Inline,
                "inline-block" => HtmlSpanDisplay::InlineBlock,
                _ => HtmlSpanDisplay::Other,
            };
        }
        "margin" => {
            if let Some(values) = css_box_shorthand_px(value, font_size, true) {
                style.margin.set_shorthand(&values);
            }
        }
        "padding" => {
            if let Some(values) = css_box_shorthand_px(value, font_size, false) {
                style.padding.set_shorthand(&values);
            }
        }
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" | "padding-top"
        | "padding-right" | "padding-bottom" | "padding-left" => {
            let is_margin = property.starts_with("margin-");
            let allow_negative = is_margin;
            let Some(length) = css_box_length_px(value, font_size, allow_negative) else {
                return;
            };
            let edges = if is_margin {
                &mut style.margin
            } else {
                &mut style.padding
            };
            match property.rsplit('-').next() {
                Some("top") => edges.top = length,
                Some("right") => edges.right = length,
                Some("bottom") => edges.bottom = length,
                Some("left") => edges.left = length,
                _ => unreachable!("matched CSS box side"),
            }
        }
        "border" => {
            let (width, visible) = css_border_shorthand(value, font_size);
            style.border_width.set_shorthand(&[width]);
            style.border_visible.fill(visible);
        }
        "border-width" => {
            let values = value
                .split_ascii_whitespace()
                .map(|token| css_border_width_px(token, font_size))
                .collect::<Option<Vec<_>>>();
            if let Some(values) = values.filter(|values| (1..=4).contains(&values.len())) {
                style.border_width.set_shorthand(&values);
            }
        }
        "border-style" => {
            let values = value
                .split_ascii_whitespace()
                .map(css_border_style_visible)
                .collect::<Option<Vec<_>>>();
            if let Some(values) = values.filter(|values| (1..=4).contains(&values.len())) {
                let mut expanded = CssBoxEdges::default();
                expanded.set_shorthand(
                    &values
                        .iter()
                        .map(|visible| f64::from(*visible))
                        .collect::<Vec<_>>(),
                );
                style.border_visible = [
                    expanded.top != 0.0,
                    expanded.right != 0.0,
                    expanded.bottom != 0.0,
                    expanded.left != 0.0,
                ];
            }
        }
        "border-top" | "border-right" | "border-bottom" | "border-left" => {
            let side = match property.as_str() {
                "border-top" => 0,
                "border-right" => 1,
                "border-bottom" => 2,
                "border-left" => 3,
                _ => unreachable!("matched CSS border side"),
            };
            let (width, visible) = css_border_shorthand(value, font_size);
            set_border_width_side(&mut style.border_width, side, width);
            style.border_visible[side] = visible;
        }
        "border-top-width" | "border-right-width" | "border-bottom-width" | "border-left-width" => {
            let Some(width) = css_border_width_px(value, font_size) else {
                return;
            };
            let side = match property.as_str() {
                "border-top-width" => 0,
                "border-right-width" => 1,
                "border-bottom-width" => 2,
                "border-left-width" => 3,
                _ => unreachable!("matched CSS border width side"),
            };
            set_border_width_side(&mut style.border_width, side, width);
        }
        "border-top-style" | "border-right-style" | "border-bottom-style" | "border-left-style" => {
            let Some(visible) = css_border_style_visible(value) else {
                return;
            };
            let side = match property.as_str() {
                "border-top-style" => 0,
                "border-right-style" => 1,
                "border-bottom-style" => 2,
                "border-left-style" => 3,
                _ => unreachable!("matched CSS border style side"),
            };
            style.border_visible[side] = visible;
        }
        "line-height" => {
            let value = value.trim().trim_end_matches("!important").trim();
            let parsed = if let Some(percent) = value.strip_suffix('%') {
                percent
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|percent| font_size * percent / 100.0)
            } else if let Ok(factor) = value.parse::<f64>() {
                Some(font_size * factor)
            } else {
                css_box_length_px(value, font_size, false)
            };
            if let Some(line_height) = parsed.filter(|line_height| *line_height > 0.0) {
                style.line_height = line_height;
            }
        }
        _ => {}
    }
}

fn flowchart_html_span_box_style(
    class_defs: &OrderedMap<String, Vec<String>>,
    classes: &[String],
    font_size: f64,
) -> HtmlSpanBoxStyle {
    let effective = flowchart_effective_node_class_names(class_defs, classes);
    let mut style = HtmlSpanBoxStyle::new(font_size);

    // `createCssStyles()` emits one selector per class definition in definition order. All
    // selectors have equal specificity, so stylesheet order, rather than the order in a node's
    // `class` attribute, owns the cascade for the generated `<span>`.
    for (class, declarations) in class_defs {
        if !effective.contains(&class.as_str()) {
            continue;
        }
        for declaration in declarations {
            for declaration in flowchart_split_mermaid_style_decls(declaration) {
                let Some((property, value)) = parse_style_decl(declaration) else {
                    continue;
                };
                apply_html_span_box_decl(&mut style, property, value, font_size);
            }
        }
    }
    style
}

pub(crate) fn flowchart_apply_html_node_class_box_metrics(
    metrics: &mut TextMetrics,
    raw_label: &str,
    label_type: &str,
    text_style: &TextStyle,
    class_defs: &OrderedMap<String, Vec<String>>,
    classes: &[String],
) {
    if raw_label.is_empty() || class_defs.is_empty() {
        return;
    }

    let has_block_child = if label_type == "markdown" {
        crate::text::mermaid_markdown_wants_paragraph_wrap(raw_label)
    } else {
        // Mermaid's `nonMarkdownToHTML()` wraps every non-empty label in `<p>...</p>`.
        true
    };
    let box_style = flowchart_html_span_box_style(class_defs, classes, text_style.font_size);
    let border = box_style.visible_border_width();
    let horizontal =
        box_style.margin.horizontal() + box_style.padding.horizontal() + border.horizontal();
    let vertical = box_style.margin.vertical() + box_style.padding.vertical() + border.vertical();

    match box_style.display {
        HtmlSpanDisplay::Inline if has_block_child && horizontal.abs() > f64::EPSILON => {
            // An inline span containing a block `<p>` is split into an inline fragment before the
            // block, the block itself, and a fragment after it. Horizontal box edges give both
            // otherwise-empty fragments inline advance, so each occupies one inherited line box.
            metrics.width = metrics.width.max(horizontal.abs());
            metrics.height += 2.0 * box_style.line_height;
            metrics.line_count += 2;
        }
        HtmlSpanDisplay::Inline => {
            metrics.width = (metrics.width + horizontal).max(0.0);
        }
        HtmlSpanDisplay::InlineBlock => {
            metrics.width = (metrics.width + horizontal).max(0.0);
            metrics.height = (metrics.height + vertical).max(0.0);
        }
        HtmlSpanDisplay::Other => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swimlane_label_rect_uses_only_the_first_concatenated_edge_style() {
        let base = TextStyle::default();
        let default_styles = vec![
            "font-size:24px".to_string(),
            "font-weight:bold".to_string(),
            "font-size:20px".to_string(),
        ];
        let edge_styles = vec![
            "font-size:12px".to_string(),
            "font-style:italic".to_string(),
        ];

        let style = flowchart_swimlane_label_rect_text_style(&base, &default_styles, &edge_styles);

        assert_eq!(style.font_size, 24.0);
        assert_eq!(style.font_weight, None);
        assert_eq!(style.font_style, None);
    }

    #[test]
    fn swimlane_label_rect_falls_back_to_the_first_edge_style() {
        let base = TextStyle::default();
        let edge_styles = vec![
            "font-size:18px,font-style:italic".to_string(),
            "font-size:30px".to_string(),
        ];

        let style = flowchart_swimlane_label_rect_text_style(&base, &[], &edge_styles);

        assert_eq!(style.font_size, 18.0);
        assert_eq!(style.font_style.as_deref(), Some("italic"));
    }

    #[test]
    fn edge_label_text_style_applies_class_then_default_then_edge_declarations() {
        let base = TextStyle::default();
        let class_defs: OrderedMap<_, _> = [(
            "accent".to_string(),
            vec!["font-size:18px,font-style:italic".to_string()],
        )].into_iter().collect();
        let classes = vec!["accent".to_string()];
        let default_styles = vec!["font-size:24px,font-weight:bold".to_string()];
        let edge_styles = vec!["font-size:12px,font-style:normal".to_string()];

        let style = flowchart_effective_edge_label_text_style(
            &base,
            &class_defs,
            &classes,
            &default_styles,
            &edge_styles,
        );

        assert_eq!(style.font_size, 12.0);
        assert_eq!(style.font_weight.as_deref(), Some("bold"));
        assert_eq!(style.font_style.as_deref(), Some("normal"));
    }
}
