use crate::Result;
use crate::config::{config_bool, value_at};
use crate::model::{
    Bounds, RailroadDiagramLayout, RailroadElementLayout, RailroadPathLayout, RailroadRuleLayout,
};
use crate::text::{
    TextMeasurer, TextStyle, is_ecmascript_whitespace, trim_ecmascript_whitespace,
    trim_start_ecmascript_whitespace,
};
use merman_core::diagrams::railroad::{
    RailroadAstNode, RailroadDiagramRenderModel, RailroadRepeatBound, RailroadRuleModel,
};

#[derive(Debug, Clone)]
pub(crate) struct RailroadStyle {
    pub padding: f64,
    pub vertical_separation: f64,
    pub horizontal_separation: f64,
    pub arc_radius: f64,
    pub font_size: f64,
    pub font_family: String,
    pub terminal_fill: String,
    pub terminal_stroke: String,
    pub terminal_text_color: String,
    pub non_terminal_fill: String,
    pub non_terminal_stroke: String,
    pub non_terminal_text_color: String,
    pub line_color: String,
    pub stroke_width: f64,
    pub marker_fill: String,
    pub comment_fill: String,
    pub comment_stroke: String,
    pub comment_text_color: String,
    pub special_fill: String,
    pub special_stroke: String,
    pub rule_name_color: String,
    pub marker_radius: f64,
    pub use_max_width: bool,
}

#[derive(Debug, Clone)]
struct ExprLayout {
    width: f64,
    height: f64,
    up: f64,
    down: f64,
    elements: Vec<RailroadElementLayout>,
    paths: Vec<RailroadPathLayout>,
    render_node: RailroadRenderNode,
}

#[derive(Debug, Clone)]
pub(crate) enum RailroadRenderNode {
    Group {
        class: &'static str,
        transform: Option<(f64, f64)>,
        children: Vec<RailroadRenderNode>,
    },
    Element {
        layout: RailroadElementLayout,
        transform: Option<(f64, f64)>,
    },
    Path(RailroadPathLayout),
}

impl RailroadRenderNode {
    fn set_transform(&mut self, x: f64, y: f64) {
        match self {
            Self::Group { transform, .. } | Self::Element { transform, .. } => {
                *transform = Some((x, y));
            }
            Self::Path(path) => {
                path.x = x;
                path.y = y;
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn layout_railroad_diagram_typed(
    model: &RailroadDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<RailroadDiagramLayout> {
    layout_railroad_diagram_typed_for_type(model, "railroad", effective_config, measurer)
}

pub(crate) fn layout_railroad_diagram_typed_for_type(
    model: &RailroadDiagramRenderModel,
    diagram_type: &str,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<RailroadDiagramLayout> {
    let style = railroad_style(effective_config);
    let mut y = style.padding;
    let mut max_width: f64 = 0.0;
    let mut rules = Vec::new();

    for rule in &model.rules {
        let mut rule_layout = layout_rule(rule, y, &style, measurer);
        y += rule_layout.height + style.vertical_separation;
        max_width = max_width.max(rule_layout.width);
        rule_layout.x = 0.0;
        rules.push(rule_layout);
    }

    let width = if rules.is_empty() {
        200.0
    } else {
        max_width + style.padding * 2.0
    };
    let height = if rules.is_empty() {
        100.0
    } else {
        y + style.padding
    };

    Ok(RailroadDiagramLayout {
        bounds: Some(Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: width,
            max_y: height,
        }),
        diagram_type: normalized_railroad_diagram_type(diagram_type).to_string(),
        width,
        height,
        use_max_width: style.use_max_width,
        rules,
    })
}

fn normalized_railroad_diagram_type(diagram_type: &str) -> &'static str {
    match diagram_type {
        "railroadEbnf" => "railroadEbnf",
        "railroadAbnf" => "railroadAbnf",
        "railroadPeg" => "railroadPeg",
        _ => "railroad",
    }
}

fn railroad_config_value<'a>(
    effective_config: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    value_at(effective_config, &["railroad", key])
}

fn theme_config_value<'a>(
    effective_config: &'a serde_json::Value,
    primary_key: &str,
    secondary_key: Option<&str>,
) -> Option<&'a serde_json::Value> {
    let primary = value_at(effective_config, &["themeVariables", primary_key]);
    match primary {
        Some(value) if !value.is_null() => Some(value),
        _ => secondary_key.and_then(|key| value_at(effective_config, &["themeVariables", key])),
    }
}

fn sanitize_color_value(value: Option<&serde_json::Value>, fallback: &str) -> String {
    value
        .and_then(serde_json::Value::as_str)
        .map(trim_ecmascript_whitespace)
        .filter(|value| is_valid_color_value(value))
        .unwrap_or(fallback)
        .to_string()
}

fn is_valid_color_value(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8)
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    }

    let Some(open) = value.find('(') else {
        return !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_alphabetic());
    };
    if !value.ends_with(')') {
        return false;
    }

    let function = &value[..open];
    let parameters = &value[open + 1..value.len() - 1];
    let valid_function = [
        "rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch",
    ]
    .iter()
    .any(|candidate| function.eq_ignore_ascii_case(candidate));

    valid_function
        && !parameters.is_empty()
        && parameters.chars().all(|character| {
            character.is_ascii_digit()
                || is_ecmascript_whitespace(character)
                || matches!(character, '%' | '+' | ',' | '.' | '/' | '-')
        })
}

fn sanitize_font_family_value(value: Option<&serde_json::Value>, fallback: &str) -> String {
    value
        .and_then(serde_json::Value::as_str)
        .map(trim_ecmascript_whitespace)
        .filter(|value| {
            !value.is_empty()
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || character == '_'
                        || matches!(character, ' ' | '"' | '\'' | ',' | '.' | '-')
                })
        })
        .unwrap_or(fallback)
        .to_string()
}

fn sanitize_number_value(value: Option<&serde_json::Value>, fallback: f64) -> f64 {
    parse_number_value(value)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

fn parse_theme_font_size(value: Option<&serde_json::Value>) -> Option<f64> {
    parse_number_value(value).filter(|value| value.is_finite() && *value > 0.0)
}

fn parse_number_value(value: Option<&serde_json::Value>) -> Option<f64> {
    match value? {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => parse_js_float_prefix(text),
        _ => None,
    }
}

fn parse_js_float_prefix(text: &str) -> Option<f64> {
    let text = trim_start_ecmascript_whitespace(text);
    let bytes = text.as_bytes();
    let mut index = 0;

    if matches!(bytes.first(), Some(b'+' | b'-')) {
        index += 1;
    }

    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let mut has_digits = index > integer_start;

    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        has_digits |= index > fraction_start;
    }

    if !has_digits {
        return None;
    }

    let mut end = index;
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut exponent_index = index + 1;
        if matches!(bytes.get(exponent_index), Some(b'+' | b'-')) {
            exponent_index += 1;
        }
        let exponent_start = exponent_index;
        while bytes.get(exponent_index).is_some_and(u8::is_ascii_digit) {
            exponent_index += 1;
        }
        if exponent_index > exponent_start {
            end = exponent_index;
        }
    }

    text[..end].parse::<f64>().ok()
}

pub(crate) fn railroad_style(effective_config: &serde_json::Value) -> RailroadStyle {
    let theme_font_family = sanitize_font_family_value(
        theme_config_value(effective_config, "fontFamily", None),
        "monospace",
    );
    let theme_font_size =
        parse_theme_font_size(theme_config_value(effective_config, "fontSize", None))
            .unwrap_or(14.0);
    let theme_terminal_fill = sanitize_color_value(
        theme_config_value(effective_config, "secondBkg", Some("secondaryColor")),
        "#FFFFC0",
    );
    let theme_terminal_stroke = sanitize_color_value(
        theme_config_value(effective_config, "secondaryBorderColor", Some("lineColor")),
        "#000000",
    );
    let theme_terminal_text_color = sanitize_color_value(
        theme_config_value(effective_config, "secondaryTextColor", Some("textColor")),
        "#000000",
    );
    let theme_non_terminal_fill = sanitize_color_value(
        theme_config_value(effective_config, "mainBkg", Some("background")),
        "#FFFFFF",
    );
    let theme_non_terminal_stroke = sanitize_color_value(
        theme_config_value(effective_config, "primaryBorderColor", Some("lineColor")),
        "#000000",
    );
    let theme_non_terminal_text_color = sanitize_color_value(
        theme_config_value(effective_config, "primaryTextColor", Some("textColor")),
        "#000000",
    );
    let theme_line_color = sanitize_color_value(
        theme_config_value(effective_config, "lineColor", None),
        "#000000",
    );
    let theme_comment_fill = sanitize_color_value(
        theme_config_value(effective_config, "labelBackground", Some("tertiaryColor")),
        "#E8E8E8",
    );
    let theme_comment_stroke = sanitize_color_value(
        theme_config_value(effective_config, "tertiaryBorderColor", Some("lineColor")),
        "#888888",
    );
    let theme_comment_text_color = sanitize_color_value(
        theme_config_value(effective_config, "tertiaryTextColor", Some("textColor")),
        "#666666",
    );
    let theme_special_fill = sanitize_color_value(
        theme_config_value(effective_config, "tertiaryColor", Some("secondaryColor")),
        "#F0E0FF",
    );
    let theme_special_stroke = sanitize_color_value(
        theme_config_value(
            effective_config,
            "tertiaryBorderColor",
            Some("secondaryBorderColor"),
        ),
        "#8800CC",
    );
    let theme_rule_name_color = sanitize_color_value(
        theme_config_value(effective_config, "titleColor", Some("textColor")),
        "#000066",
    );

    RailroadStyle {
        padding: sanitize_number_value(railroad_config_value(effective_config, "padding"), 10.0),
        vertical_separation: sanitize_number_value(
            railroad_config_value(effective_config, "verticalSeparation"),
            8.0,
        ),
        horizontal_separation: sanitize_number_value(
            railroad_config_value(effective_config, "horizontalSeparation"),
            10.0,
        ),
        arc_radius: sanitize_number_value(
            railroad_config_value(effective_config, "arcRadius"),
            10.0,
        ),
        font_size: sanitize_number_value(
            railroad_config_value(effective_config, "fontSize"),
            theme_font_size,
        ),
        font_family: sanitize_font_family_value(
            railroad_config_value(effective_config, "fontFamily"),
            &theme_font_family,
        ),
        terminal_fill: sanitize_color_value(
            railroad_config_value(effective_config, "terminalFill"),
            &theme_terminal_fill,
        ),
        terminal_stroke: sanitize_color_value(
            railroad_config_value(effective_config, "terminalStroke"),
            &theme_terminal_stroke,
        ),
        terminal_text_color: sanitize_color_value(
            railroad_config_value(effective_config, "terminalTextColor"),
            &theme_terminal_text_color,
        ),
        non_terminal_fill: sanitize_color_value(
            railroad_config_value(effective_config, "nonTerminalFill"),
            &theme_non_terminal_fill,
        ),
        non_terminal_stroke: sanitize_color_value(
            railroad_config_value(effective_config, "nonTerminalStroke"),
            &theme_non_terminal_stroke,
        ),
        non_terminal_text_color: sanitize_color_value(
            railroad_config_value(effective_config, "nonTerminalTextColor"),
            &theme_non_terminal_text_color,
        ),
        line_color: sanitize_color_value(
            railroad_config_value(effective_config, "lineColor"),
            &theme_line_color,
        ),
        stroke_width: sanitize_number_value(
            railroad_config_value(effective_config, "strokeWidth"),
            2.0,
        ),
        marker_fill: sanitize_color_value(
            railroad_config_value(effective_config, "markerFill"),
            &theme_line_color,
        ),
        comment_fill: sanitize_color_value(
            railroad_config_value(effective_config, "commentFill"),
            &theme_comment_fill,
        ),
        comment_stroke: sanitize_color_value(
            railroad_config_value(effective_config, "commentStroke"),
            &theme_comment_stroke,
        ),
        comment_text_color: sanitize_color_value(
            railroad_config_value(effective_config, "commentTextColor"),
            &theme_comment_text_color,
        ),
        special_fill: sanitize_color_value(
            railroad_config_value(effective_config, "specialFill"),
            &theme_special_fill,
        ),
        special_stroke: sanitize_color_value(
            railroad_config_value(effective_config, "specialStroke"),
            &theme_special_stroke,
        ),
        rule_name_color: sanitize_color_value(
            railroad_config_value(effective_config, "ruleNameColor"),
            &theme_rule_name_color,
        ),
        marker_radius: sanitize_number_value(
            railroad_config_value(effective_config, "markerRadius"),
            5.0,
        ),
        use_max_width: config_bool(effective_config, &["railroad", "useMaxWidth"]).unwrap_or(true),
    }
}

fn layout_rule(
    rule: &RailroadRuleModel,
    y: f64,
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> RailroadRuleLayout {
    let rule_name = format!("{} =", rule.name);
    let name_width = measure_text(rule_name.as_str(), style, measurer).0 + 20.0;
    let definition_x = name_width + 20.0;
    let definition = layout_expr(&rule.definition, style, measurer);
    let baseline_y = 20.0_f64.max(definition.up);
    let definition_y = baseline_y - definition.up;
    let mut elements = definition.elements;
    let mut paths = definition.paths;
    translate_elements(&mut elements, definition_x, definition_y);
    translate_paths(&mut paths, definition_x, definition_y);

    paths.push(railroad_path(
        PathBuilder::new()
            .move_to(name_width + style.marker_radius, baseline_y)
            .line_to(definition_x, baseline_y)
            .build(),
    ));
    paths.push(railroad_path(
        PathBuilder::new()
            .move_to(definition_x + definition.width, baseline_y)
            .line_to(
                definition_x + definition.width + 10.0 - style.marker_radius,
                baseline_y,
            )
            .build(),
    ));

    RailroadRuleLayout {
        name: rule.name.clone(),
        x: 0.0,
        y,
        width: definition_x + definition.width + 10.0 + style.marker_radius,
        height: 40.0_f64.max(definition_y + definition.height + style.padding * 2.0),
        baseline_y,
        name_width,
        definition_x,
        start_marker_x: name_width,
        end_marker_x: definition_x + definition.width + 10.0,
        marker_radius: style.marker_radius,
        elements,
        paths,
    }
}

fn layout_expr(
    node: &RailroadAstNode,
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> ExprLayout {
    match node {
        RailroadAstNode::Terminal { value, .. } => layout_box("terminal", value, style, measurer),
        RailroadAstNode::NonTerminal { name, .. } => {
            layout_box("nonterminal", name, style, measurer)
        }
        RailroadAstNode::Special { text, .. } => {
            layout_box("special", &format!("? {text} ?"), style, measurer)
        }
        RailroadAstNode::Sequence { elements, .. } => layout_sequence(elements, style, measurer),
        RailroadAstNode::Choice { alternatives, .. } => {
            layout_choice(alternatives, style, measurer)
        }
        RailroadAstNode::Optional { element, .. } => layout_optional(element, style, measurer),
        RailroadAstNode::Repetition { element, min, .. } => {
            layout_repetition(element, *min, style, measurer)
        }
    }
}

pub(crate) fn railroad_render_node(
    node: &RailroadAstNode,
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> (RailroadRenderNode, f64) {
    let layout = layout_expr(node, style, measurer);
    (layout.render_node, layout.up)
}

fn layout_box(
    kind: &str,
    label: &str,
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> ExprLayout {
    let (text_width, text_height) = measure_text(label, style, measurer);
    let width = text_width + style.padding * 2.0;
    let height = text_height + style.padding * 2.0;
    let element = RailroadElementLayout {
        kind: kind.to_string(),
        label: label.to_string(),
        x: 0.0,
        y: 0.0,
        width,
        height,
        text_x: width / 2.0,
        text_y: height / 2.0,
    };
    ExprLayout {
        width,
        height,
        up: height / 2.0,
        down: height / 2.0,
        elements: vec![element.clone()],
        paths: Vec::new(),
        render_node: RailroadRenderNode::Element {
            layout: element,
            transform: None,
        },
    }
}

fn layout_sequence(
    elements: &[RailroadAstNode],
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> ExprLayout {
    let rendered: Vec<_> = elements
        .iter()
        .map(|element| layout_expr(element, style, measurer))
        .collect();
    if rendered.is_empty() {
        return empty_expr();
    }

    let mut width = rendered.iter().map(|item| item.width).sum::<f64>();
    width += (rendered.len().saturating_sub(1)) as f64 * style.horizontal_separation;
    let up = rendered.iter().map(|item| item.up).fold(0.0, f64::max);
    let down = rendered.iter().map(|item| item.down).fold(0.0, f64::max);
    let mut out = ExprLayout {
        width,
        height: up + down,
        up,
        down,
        elements: Vec::new(),
        paths: Vec::new(),
        render_node: RailroadRenderNode::Group {
            class: "railroad-sequence",
            transform: None,
            children: Vec::new(),
        },
    };
    let mut render_children = Vec::new();
    let mut x = 0.0;
    for (idx, mut child) in rendered.into_iter().enumerate() {
        let y = up - child.up;
        let child_width = child.width;
        translate_elements(&mut child.elements, x, y);
        translate_paths(&mut child.paths, x, y);
        out.elements.extend(child.elements);
        out.paths.extend(child.paths);
        child.render_node.set_transform(x, y);
        render_children.push(child.render_node);
        if idx + 1 < elements.len() {
            let line_x1 = x + child_width;
            let line_x2 = line_x1 + style.horizontal_separation;
            let path = railroad_path(
                PathBuilder::new()
                    .move_to(line_x1, up)
                    .line_to(line_x2, up)
                    .build(),
            );
            out.paths.push(path.clone());
            render_children.push(RailroadRenderNode::Path(path));
        }
        x += child_width + style.horizontal_separation;
    }
    if let RailroadRenderNode::Group { children, .. } = &mut out.render_node {
        *children = render_children;
    }
    out
}

fn layout_choice(
    alternatives: &[RailroadAstNode],
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> ExprLayout {
    let rendered: Vec<_> = alternatives
        .iter()
        .map(|alternative| layout_expr(alternative, style, measurer))
        .collect();
    if rendered.is_empty() {
        return empty_expr();
    }

    let max_width = rendered.iter().map(|item| item.width).fold(0.0, f64::max);
    let mut total_height = rendered.iter().map(|item| item.height).sum::<f64>();
    total_height += (rendered.len().saturating_sub(1)) as f64 * style.vertical_separation;
    let arc_radius = style.arc_radius;
    let total_width = max_width + arc_radius * 4.0;
    let center_y = total_height / 2.0;
    let mut out = ExprLayout {
        width: total_width,
        height: total_height,
        up: center_y,
        down: total_height - center_y,
        elements: Vec::new(),
        paths: Vec::new(),
        render_node: RailroadRenderNode::Group {
            class: "railroad-choice",
            transform: None,
            children: Vec::new(),
        },
    };
    let mut render_children = Vec::new();

    let mut y = 0.0;
    for mut child in rendered {
        let elem_y = y;
        let elem_center_y = elem_y + child.up;
        let elem_x = arc_radius * 2.0 + (max_width - child.width) / 2.0;
        let is_center = same_layout_coordinate(elem_center_y, center_y);
        let below_center = !is_center && elem_center_y > center_y;
        let left_path = if is_center {
            PathBuilder::new()
                .move_to(0.0, center_y)
                .line_to(elem_x, elem_center_y)
                .build()
        } else {
            PathBuilder::new()
                .move_to(0.0, center_y)
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    below_center,
                    arc_radius,
                    center_y
                        + if below_center {
                            arc_radius
                        } else {
                            -arc_radius
                        },
                )
                .line_to(
                    arc_radius,
                    elem_center_y
                        - if below_center {
                            arc_radius
                        } else {
                            -arc_radius
                        },
                )
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    !below_center,
                    arc_radius * 2.0,
                    elem_center_y,
                )
                .line_to(elem_x, elem_center_y)
                .build()
        };
        let left_path = railroad_path(left_path);
        out.paths.push(left_path.clone());

        let right_start = elem_x + child.width;
        let right_lane_x = total_width - arc_radius * 2.0;
        let right_path = if is_center {
            PathBuilder::new()
                .move_to(right_start, elem_center_y)
                .line_to(total_width, center_y)
                .build()
        } else {
            PathBuilder::new()
                .move_to(right_start, elem_center_y)
                .line_to(right_lane_x, elem_center_y)
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    !below_center,
                    total_width - arc_radius,
                    elem_center_y
                        + if below_center {
                            -arc_radius
                        } else {
                            arc_radius
                        },
                )
                .line_to(
                    total_width - arc_radius,
                    center_y
                        + if below_center {
                            arc_radius
                        } else {
                            -arc_radius
                        },
                )
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    below_center,
                    total_width,
                    center_y,
                )
                .build()
        };
        let right_path = railroad_path(right_path);
        out.paths.push(right_path.clone());
        child.render_node.set_transform(elem_x, elem_y);
        render_children.push(child.render_node);
        render_children.push(RailroadRenderNode::Path(left_path));
        render_children.push(RailroadRenderNode::Path(right_path));
        translate_elements(&mut child.elements, elem_x, elem_y);
        translate_paths(&mut child.paths, elem_x, elem_y);
        out.elements.extend(child.elements);
        out.paths.extend(child.paths);
        y += child.height + style.vertical_separation;
    }

    if let RailroadRenderNode::Group { children, .. } = &mut out.render_node {
        *children = render_children;
    }

    out
}

fn same_layout_coordinate(left: f64, right: f64) -> bool {
    // Distinct addition orders can drift while still producing the same emitted coordinate.
    left.is_finite() && right.is_finite() && fmt_number(left) == fmt_number(right)
}

fn layout_optional(
    element: &RailroadAstNode,
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> ExprLayout {
    let mut inner = layout_expr(element, style, measurer);
    let arc_radius = style.arc_radius;
    let arc_height = arc_radius * 2.0;
    let total_width = inner.width + arc_radius * 4.0;
    let total_height = inner.height + arc_height;
    let elem_x = arc_radius * 2.0;
    let elem_y = arc_height;
    let center_y = elem_y + inner.up;
    translate_elements(&mut inner.elements, elem_x, elem_y);
    translate_paths(&mut inner.paths, elem_x, elem_y);
    inner.render_node.set_transform(elem_x, elem_y);

    let mut paths = inner.paths;
    let lower_path = railroad_path(
        PathBuilder::new()
            .move_to(0.0, center_y)
            .line_to(arc_radius * 2.0, center_y)
            .build(),
    );
    paths.push(lower_path.clone());
    let lower_path_2 = railroad_path(
        PathBuilder::new()
            .move_to(elem_x + inner.width, center_y)
            .line_to(total_width, center_y)
            .build(),
    );
    paths.push(lower_path_2.clone());
    let bypass_path = railroad_path(
        PathBuilder::new()
            .move_to(0.0, center_y)
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                false,
                arc_radius,
                center_y - arc_radius,
            )
            .line_to(arc_radius, arc_radius)
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                arc_radius * 2.0,
                0.0,
            )
            .line_to(total_width - arc_radius * 2.0, 0.0)
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                total_width - arc_radius,
                arc_radius,
            )
            .line_to(total_width - arc_radius, center_y - arc_radius)
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                false,
                total_width,
                center_y,
            )
            .build(),
    );
    paths.push(bypass_path.clone());

    ExprLayout {
        width: total_width,
        height: total_height,
        up: center_y,
        down: total_height - center_y,
        elements: inner.elements,
        paths,
        render_node: RailroadRenderNode::Group {
            class: "railroad-optional",
            transform: None,
            children: vec![
                inner.render_node,
                RailroadRenderNode::Path(lower_path),
                RailroadRenderNode::Path(lower_path_2),
                RailroadRenderNode::Path(bypass_path),
            ],
        },
    }
}

fn layout_repetition(
    element: &RailroadAstNode,
    min: RailroadRepeatBound,
    style: &RailroadStyle,
    measurer: &dyn TextMeasurer,
) -> ExprLayout {
    let mut inner = layout_expr(element, style, measurer);
    let arc_radius = style.arc_radius;
    let arc_height = arc_radius * 2.0;
    let total_width = inner.width + arc_radius * 4.0;
    let has_bypass = min.is_zero();
    let total_height = inner.height + arc_height + if has_bypass { arc_height } else { 0.0 };
    let elem_x = arc_radius * 2.0;
    let elem_y = if has_bypass { arc_height } else { 0.0 };
    let center_y = elem_y + inner.up;
    translate_elements(&mut inner.elements, elem_x, elem_y);
    translate_paths(&mut inner.paths, elem_x, elem_y);
    inner.render_node.set_transform(elem_x, elem_y);
    let mut paths = inner.paths;
    let forward_path = railroad_path(
        PathBuilder::new()
            .move_to(0.0, center_y)
            .line_to(arc_radius * 2.0, center_y)
            .build(),
    );
    paths.push(forward_path.clone());
    let forward_path_2 = railroad_path(
        PathBuilder::new()
            .move_to(elem_x + inner.width, center_y)
            .line_to(total_width, center_y)
            .build(),
    );
    paths.push(forward_path_2.clone());

    let loop_y = elem_y + inner.height + arc_radius;
    let loop_path = railroad_path(
        PathBuilder::new()
            .move_to(elem_x + inner.width, center_y)
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                elem_x + inner.width + arc_radius,
                center_y + arc_radius,
            )
            .line_to(elem_x + inner.width + arc_radius, loop_y)
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                elem_x + inner.width,
                loop_y + arc_radius,
            )
            .line_to(arc_radius * 2.0, loop_y + arc_radius)
            .arc_to(arc_radius, arc_radius, 0.0, false, true, arc_radius, loop_y)
            .line_to(arc_radius, center_y + arc_radius)
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                arc_radius * 2.0,
                center_y,
            )
            .build(),
    );
    paths.push(loop_path.clone());

    let mut bypass_render_path = None;
    if has_bypass {
        let bypass_path = railroad_path(
            PathBuilder::new()
                .move_to(0.0, center_y)
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    false,
                    arc_radius,
                    center_y - arc_radius,
                )
                .line_to(arc_radius, arc_radius)
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    true,
                    arc_radius * 2.0,
                    0.0,
                )
                .line_to(total_width - arc_radius * 2.0, 0.0)
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    true,
                    total_width - arc_radius,
                    arc_radius,
                )
                .line_to(total_width - arc_radius, center_y - arc_radius)
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    false,
                    total_width,
                    center_y,
                )
                .build(),
        );
        paths.push(bypass_path.clone());
        bypass_render_path = Some(bypass_path);
    }

    let mut render_children = vec![
        inner.render_node,
        RailroadRenderNode::Path(forward_path),
        RailroadRenderNode::Path(forward_path_2),
        RailroadRenderNode::Path(loop_path),
    ];
    if let Some(path) = bypass_render_path {
        render_children.push(RailroadRenderNode::Path(path));
    }

    ExprLayout {
        width: total_width,
        height: total_height,
        up: center_y,
        down: total_height - center_y,
        elements: inner.elements,
        paths,
        render_node: RailroadRenderNode::Group {
            class: "railroad-repetition",
            transform: None,
            children: render_children,
        },
    }
}

fn empty_expr() -> ExprLayout {
    ExprLayout {
        width: 0.0,
        height: 0.0,
        up: 0.0,
        down: 0.0,
        elements: Vec::new(),
        paths: Vec::new(),
        render_node: RailroadRenderNode::Group {
            class: "railroad-group",
            transform: None,
            children: Vec::new(),
        },
    }
}

fn measure_text(text: &str, style: &RailroadStyle, measurer: &dyn TextMeasurer) -> (f64, f64) {
    let text_style = TextStyle {
        font_family: Some(style.font_family.clone()),
        font_size: style.font_size,
        font_weight: None,
        font_style: None,
    };
    let width = measurer.measure_svg_raw_text_bbox_width_px(text, &text_style);
    let height = measurer
        .measure_svg_simple_text_bbox_height_px(text, &text_style)
        .max(style.font_size);
    (width, height)
}

fn translate_elements(elements: &mut [RailroadElementLayout], dx: f64, dy: f64) {
    for element in elements {
        element.x += dx;
        element.y += dy;
    }
}

fn translate_paths(paths: &mut [RailroadPathLayout], dx: f64, dy: f64) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    for path in paths {
        path.x += dx;
        path.y += dy;
    }
}

fn railroad_path(d: String) -> RailroadPathLayout {
    RailroadPathLayout { x: 0.0, y: 0.0, d }
}

struct PathBuilder {
    d: String,
}

impl PathBuilder {
    fn new() -> Self {
        Self { d: String::new() }
    }

    fn move_to(mut self, x: f64, y: f64) -> Self {
        self.push_cmd("M", &[x, y]);
        self
    }

    fn line_to(mut self, x: f64, y: f64) -> Self {
        self.push_cmd("L", &[x, y]);
        self
    }

    #[allow(clippy::too_many_arguments)]
    fn arc_to(
        mut self,
        rx: f64,
        ry: f64,
        rotation: f64,
        large_arc: bool,
        sweep: bool,
        x: f64,
        y: f64,
    ) -> Self {
        self.push_raw("A");
        self.push_number(rx);
        self.push_number(ry);
        self.push_number(rotation);
        self.push_number(if large_arc { 1.0 } else { 0.0 });
        self.push_number(if sweep { 1.0 } else { 0.0 });
        self.push_number(x);
        self.push_number(y);
        self
    }

    fn build(self) -> String {
        self.d.trim().to_string()
    }

    fn push_cmd(&mut self, cmd: &str, values: &[f64]) {
        self.push_raw(cmd);
        for value in values {
            self.push_number(*value);
        }
    }

    fn push_raw(&mut self, value: &str) {
        if !self.d.is_empty() {
            self.d.push(' ');
        }
        self.d.push_str(value);
    }

    fn push_number(&mut self, value: f64) {
        if !self.d.is_empty() {
            self.d.push(' ');
        }
        self.d.push_str(&fmt_number(value));
    }
}

fn fmt_number(value: f64) -> String {
    if !value.is_finite() || value.abs() < 0.0005 {
        return "0".to_string();
    }
    let mut rounded = if value.abs() <= f64::MAX / 1000.0 {
        (value * 1000.0).round() / 1000.0
    } else {
        value
    };
    if rounded.abs() < 0.0005 {
        rounded = 0.0;
    }
    let mut s = format!("{rounded:.3}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{DeterministicTextMeasurer, TextMetrics};
    use roughr::{PathParser, PathSegment};

    struct RailroadBBoxMeasurer;

    impl TextMeasurer for RailroadBBoxMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: self.measure_svg_raw_text_bbox_width_px(text, style),
                height: style.font_size * 1.1,
                line_count: 1,
            }
        }

        fn measure_svg_raw_text_bbox_width_px(&self, text: &str, _style: &TextStyle) -> f64 {
            match text {
                "other" => 31.25,
                "? anything ?" => 62.75,
                _ => 40.0,
            }
        }

        fn measure_svg_simple_text_bbox_height_px(&self, _text: &str, style: &TextStyle) -> f64 {
            style.font_size * 1.1
        }
    }

    fn choice_branch_connector_paths(
        source: &str,
        effective_config: &serde_json::Value,
        branch_label: &str,
    ) -> (String, String) {
        let parsed = merman_core::Engine::new()
            .parse_diagram_for_render_model_sync(source, merman_core::ParseOptions::strict())
            .unwrap()
            .expect("railroad render model parses");
        let merman_core::RenderSemanticModel::Railroad(model) = parsed.model() else {
            panic!("expected railroad render model");
        };

        let style = railroad_style(effective_config);
        let (render_node, _) = railroad_render_node(
            &model.rules[0].definition,
            &style,
            &DeterministicTextMeasurer::default(),
        );
        let RailroadRenderNode::Group {
            class, children, ..
        } = render_node
        else {
            panic!("expected railroad choice render group");
        };
        assert_eq!(class, "railroad-choice");
        let mut branches = children.chunks_exact(3);
        assert!(
            branches.remainder().is_empty(),
            "choice render children must be branch/left-path/right-path triples"
        );
        let mut matching_branches = branches.by_ref().filter_map(|branch| {
            let [
                branch,
                RailroadRenderNode::Path(left_path),
                RailroadRenderNode::Path(right_path),
            ] = branch
            else {
                panic!("choice render children must be branch/left-path/right-path triples");
            };
            matches!(
                branch,
                RailroadRenderNode::Element { layout, .. } if layout.label == branch_label
            )
            .then_some((left_path, right_path))
        });
        let (left_path, right_path) = matching_branches
            .next()
            .unwrap_or_else(|| panic!("choice branch render node: {branch_label}"));
        assert!(
            matching_branches.next().is_none(),
            "choice branch label must be unique: {branch_label}"
        );

        (left_path.d.clone(), right_path.d.clone())
    }

    fn connector_segments(path: &str) -> Vec<PathSegment> {
        PathParser::from(path)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap_or_else(|error| panic!("valid connector path {path:?}: {error}"))
    }

    fn assert_straight_connector(path: &str) {
        assert!(
            matches!(
                connector_segments(path).as_slice(),
                [PathSegment::MoveTo { .. }, PathSegment::LineTo { .. }]
            ),
            "expected a straight connector: {path}"
        );
    }

    fn assert_curved_connector(path: &str) {
        assert!(
            connector_segments(path)
                .iter()
                .any(|segment| matches!(segment, PathSegment::EllipticalArc { .. })),
            "expected an arc connector: {path}"
        );
    }

    fn repetition_render_path_count(min: RailroadRepeatBound) -> usize {
        let node = RailroadAstNode::Repetition {
            element: Box::new(RailroadAstNode::Terminal {
                value: "item".to_string(),
                span: merman_core::SourceSpan::new(0, 0),
                selection: merman_core::SourceSpan::new(0, 0),
            }),
            min,
            max: RailroadRepeatBound::INFINITY,
            separator: None,
            span: merman_core::SourceSpan::new(0, 0),
        };
        let (render_node, _) = railroad_render_node(
            &node,
            &railroad_style(&serde_json::json!({})),
            &DeterministicTextMeasurer::default(),
        );
        let RailroadRenderNode::Group {
            class, children, ..
        } = render_node
        else {
            panic!("expected railroad repetition render group");
        };
        assert_eq!(class, "railroad-repetition");
        children
            .iter()
            .filter(|child| matches!(child, RailroadRenderNode::Path(_)))
            .count()
    }

    #[test]
    fn railroad_repetition_bypass_depends_only_on_zero_minimum() {
        assert_eq!(repetition_render_path_count(RailroadRepeatBound::ZERO), 4);
        assert_eq!(repetition_render_path_count(RailroadRepeatBound::ONE), 3);
        assert_eq!(
            repetition_render_path_count(RailroadRepeatBound::INFINITY),
            3
        );
    }

    #[test]
    fn railroad_layout_handles_sequence_choice_and_repetition() {
        let parsed = merman_core::Engine::new()
            .parse_diagram_for_render_model_sync(
                "railroad-beta\nexpr = sequence(nonterminal(\"term\"), zeroOrMore(terminal(\"+\"))) ;\n",
                merman_core::ParseOptions::strict(),
            )
            .unwrap()
            .expect("railroad render model parses");
        let merman_core::RenderSemanticModel::Railroad(model) = parsed.model() else {
            panic!("expected railroad render model");
        };

        let layout = layout_railroad_diagram_typed(
            model,
            &serde_json::json!({}),
            &DeterministicTextMeasurer::default(),
        )
        .unwrap();

        assert_eq!(layout.rules.len(), 1);
        assert!(layout.rules[0].width > 0.0);
        assert!(
            layout.rules[0]
                .paths
                .iter()
                .any(|path| path.d.contains('A'))
        );
        assert_eq!(layout.rules[0].elements.len(), 2);
        assert_eq!(layout.diagram_type, "railroad");

        let ebnf_layout = layout_railroad_diagram_typed_for_type(
            model,
            "railroadEbnf",
            &serde_json::json!({}),
            &DeterministicTextMeasurer::default(),
        )
        .unwrap();
        assert_eq!(ebnf_layout.diagram_type, "railroadEbnf");
        assert_eq!(
            serde_json::to_value(&ebnf_layout).unwrap()["diagram_type"],
            "railroadEbnf"
        );
    }

    #[test]
    fn railroad_choice_keeps_center_branch_connectors_straight() {
        let effective_config = serde_json::json!({
            "themeVariables": { "fontSize": 16 }
        });
        let source = "railroad-ebnf-beta\nx = a | b | c | number | e | f | g ;\n";
        let (left_path, right_path) =
            choice_branch_connector_paths(source, &effective_config, "number");

        assert_straight_connector(&left_path);
        assert_straight_connector(&right_path);

        let (upper_left_path, upper_right_path) =
            choice_branch_connector_paths(source, &effective_config, "a");
        assert_curved_connector(&upper_left_path);
        assert_curved_connector(&upper_right_path);
    }

    #[test]
    fn railroad_lane_coordinate_equality_matches_path_serialization() {
        assert!(same_layout_coordinate(56.5004, 56.50049));
        assert!(!same_layout_coordinate(56.5004, 56.5006));
        assert!(!same_layout_coordinate(f64::INFINITY, f64::INFINITY));
        assert!(!same_layout_coordinate(f64::NAN, f64::NAN));
        assert_ne!(fmt_number(f64::MAX), "inf");
    }

    #[test]
    fn railroad_choice_keeps_center_branch_connectors_straight_with_fractional_spacing() {
        let effective_config = serde_json::json!({
            "themeVariables": { "fontSize": 16 },
            "railroad": { "verticalSeparation": 0.1 }
        });
        let (left_path, right_path) = choice_branch_connector_paths(
            "railroad-ebnf-beta\nx = a | b | c ;\n",
            &effective_config,
            "b",
        );

        assert_straight_connector(&left_path);
        assert_straight_connector(&right_path);
    }

    #[test]
    fn railroad_variants_use_raw_svg_bbox_width_without_character_floor() {
        let parsed = merman_core::Engine::new()
            .parse_diagram_for_render_model_sync(
                "railroad-beta\nexpr = choice(nonterminal(\"other\"), special(\"anything\")) ;\n",
                merman_core::ParseOptions::strict(),
            )
            .unwrap()
            .expect("railroad render model parses");
        let merman_core::RenderSemanticModel::Railroad(model) = parsed.model() else {
            panic!("expected railroad render model");
        };

        for diagram_type in ["railroad", "railroadEbnf", "railroadAbnf", "railroadPeg"] {
            let layout = layout_railroad_diagram_typed_for_type(
                model,
                diagram_type,
                &serde_json::json!({}),
                &RailroadBBoxMeasurer,
            )
            .unwrap();
            let elements = &layout.rules[0].elements;
            let other = elements
                .iter()
                .find(|element| element.label == "other")
                .expect("nonterminal element");
            let anything = elements
                .iter()
                .find(|element| element.label == "? anything ?")
                .expect("special element");

            assert_eq!(layout.diagram_type, diagram_type);
            assert_eq!(other.width, 31.25 + 20.0, "{diagram_type}");
            assert_eq!(anything.width, 62.75 + 20.0, "{diagram_type}");
        }
    }

    #[test]
    fn railroad_style_matches_upstream_css_value_whitelists() {
        for color in [
            "#abc",
            "#abcd",
            "#abcdef",
            "#abcdef12",
            "rgb(10 20 30 / 50%)",
            "hsl(120, 40%, 50%)",
            "oklch(70% 0.1 200)",
            "currentColor",
        ] {
            assert!(is_valid_color_value(color), "expected valid color: {color}");
        }
        for color in [
            "#abcde",
            "var(--railroad-color)",
            "url(javascript:alert(1))",
            "#fff;stroke:red",
            "red}",
        ] {
            assert!(
                !is_valid_color_value(color),
                "expected invalid color: {color}"
            );
        }

        let valid_font = serde_json::json!("\"Fira Code\", monospace");
        let invalid_font = serde_json::json!("safe\"} body { display: none; } /*");
        assert_eq!(
            sanitize_font_family_value(Some(&valid_font), "fallback"),
            "\"Fira Code\", monospace"
        );
        assert_eq!(
            sanitize_font_family_value(Some(&invalid_font), "fallback"),
            "fallback"
        );
    }

    #[test]
    fn railroad_style_numbers_follow_number_parse_float_semantics() {
        for (input, expected) in [
            ("12px", 12.0),
            ("1e2junk", 100.0),
            ("0x10", 0.0),
            (".5rem", 0.5),
        ] {
            assert_eq!(parse_js_float_prefix(input), Some(expected), "{input}");
        }
        for input in ["", "NaN", "Infinity", "-Infinity"] {
            assert_eq!(
                sanitize_number_value(Some(&serde_json::json!(input)), 7.0),
                7.0,
                "{input}"
            );
        }
        assert_eq!(
            sanitize_number_value(Some(&serde_json::json!("-1px")), 7.0),
            7.0
        );
        assert_eq!(sanitize_number_value(Some(&serde_json::json!(0)), 7.0), 0.0);
        assert_eq!(parse_theme_font_size(Some(&serde_json::json!(0))), None);
    }

    #[test]
    fn railroad_style_uses_ecmascript_whitespace_semantics() {
        let bom_color = serde_json::json!("\u{FEFF}rgb(1\u{FEFF}2 3)\u{FEFF}");
        let nel_color = serde_json::json!("rgb(1\u{0085}2 3)");
        assert_eq!(
            sanitize_color_value(Some(&bom_color), "fallback"),
            "rgb(1\u{FEFF}2 3)"
        );
        assert_eq!(
            sanitize_color_value(Some(&nel_color), "fallback"),
            "fallback"
        );
        assert_eq!(parse_js_float_prefix("\u{FEFF}18px"), Some(18.0));
        assert_eq!(parse_js_float_prefix("\u{0085}18px"), None);
    }

    #[test]
    fn invalid_primary_theme_color_uses_hard_fallback() {
        let style = railroad_style(&serde_json::json!({
            "themeVariables": {
                "secondBkg": "#fff; stroke: red",
                "secondaryColor": "#123456"
            }
        }));

        assert_eq!(style.terminal_fill, "#FFFFC0");
    }
}
