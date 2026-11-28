use super::*;
use crate::model::RailroadElementLayout;
use merman_core::diagrams::railroad::RailroadDiagramRenderModel;

pub(crate) fn render_railroad_diagram_svg_model(
    layout: &RailroadDiagramLayout,
    model: &RailroadDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("railroad");
    let acc_title = model
        .acc_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let acc_descr = model
        .acc_descr
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let aria_labelledby = acc_title.map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = acc_descr.map(|_| format!("chart-desc-{diagram_id}"));
    let root_bounds = root_svg::DiagramBounds::from_view_box(0.0, 0.0, layout.width, layout.height);
    let root_spec = root_svg::RootViewportSpec::mermaid(root_bounds, layout.use_max_width);
    let style = crate::railroad::railroad_style(effective_config);

    let mut out = String::new();
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, &layout.diagram_type);
    root_chrome.class = Some("railroad-diagram");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Railroad, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    options.checkpoint_emit()?;

    if let Some(title) = acc_title {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{diagram_id}">{}</title>"#,
            escape_xml_display(title)
        );
    }
    if let Some(descr) = acc_descr {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{diagram_id}">{}</desc>"#,
            escape_xml_display(descr)
        );
    }
    let _ = write!(
        &mut out,
        "<style>{}</style>",
        railroad_css(&style, diagram_id)
    );
    out.push_str("<g/>");
    options.checkpoint_emit()?;

    for (rule_index, rule) in layout.rules.iter().enumerate() {
        let model_rule = model
            .rules
            .get(rule_index)
            .ok_or_else(|| Error::InvalidModel {
                message: format!(
                    "railroad layout contains rule {} without a matching semantic rule",
                    rule.name
                ),
            })?;
        let _ = write!(
            &mut out,
            r#"<g class="railroad-rule" transform="translate({}, {})">"#,
            fmt(rule.x),
            fmt(rule.y)
        );
        let (render_node, definition_up) =
            crate::railroad::railroad_render_node(&model_rule.definition, &style, measurer);
        let _ = write!(
            &mut out,
            r#"<g transform="translate({}, {})">"#,
            fmt(rule.definition_x),
            fmt(rule.baseline_y - definition_up)
        );
        push_render_node(&mut out, &render_node);
        out.push_str("</g>");
        let _ = write!(
            &mut out,
            r#"<g class="railroad-rule-name-group"><text class="railroad-rule-name" x="0" y="{}">{} =</text></g>"#,
            fmt(rule.baseline_y),
            escape_xml_display(&rule.name)
        );
        let _ = write!(
            &mut out,
            r#"<g class="railroad-start"><circle cx="{}" cy="{}" r="{}"></circle></g><g class="railroad-end"><circle cx="{}" cy="{}" r="{}"></circle></g>"#,
            fmt(rule.start_marker_x),
            fmt(rule.baseline_y),
            fmt(rule.marker_radius),
            fmt(rule.end_marker_x),
            fmt(rule.baseline_y),
            fmt(rule.marker_radius)
        );
        for path in rule.paths.iter().rev().take(2).rev() {
            push_path(&mut out, path);
        }
        out.push_str("</g>");
    }

    out.push_str("</svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

fn push_render_node(out: &mut String, node: &crate::railroad::RailroadRenderNode) {
    match node {
        crate::railroad::RailroadRenderNode::Group {
            class,
            transform,
            children,
        } => {
            let _ = write!(out, r#"<g class="{}""#, escape_attr_display(class));
            push_optional_transform(out, *transform);
            out.push('>');
            for child in children {
                push_render_node(out, child);
            }
            out.push_str("</g>");
        }
        crate::railroad::RailroadRenderNode::Element { layout, transform } => {
            push_element(out, layout, *transform);
        }
        crate::railroad::RailroadRenderNode::Path(path) => push_path(out, path),
    }
}

fn push_optional_transform(out: &mut String, transform: Option<(f64, f64)>) {
    if let Some((x, y)) = transform {
        let _ = write!(out, r#" transform="translate({}, {})""#, fmt(x), fmt(y));
    }
}

fn push_path(out: &mut String, path: &crate::model::RailroadPathLayout) {
    out.push_str(r#"<path class="railroad-line""#);
    if path.x != 0.0 || path.y != 0.0 {
        let _ = write!(
            out,
            r#" transform="translate({}, {})""#,
            fmt(path.x),
            fmt(path.y)
        );
    }
    let _ = write!(out, r#" d="{}"></path>"#, escape_attr_display(&path.d));
}

fn push_element(out: &mut String, element: &RailroadElementLayout, transform: Option<(f64, f64)>) {
    let class = match element.kind.as_str() {
        "terminal" => "railroad-terminal",
        "nonterminal" => "railroad-nonterminal",
        "special" => "railroad-special",
        _ => "railroad-group",
    };
    let _ = write!(out, r#"<g class="{}""#, class);
    push_optional_transform(out, transform);
    out.push('>');
    match element.kind.as_str() {
        "terminal" => {
            let _ = write!(
                out,
                r#"<rect x="0" y="0" width="{}" height="{}" rx="10" ry="10"></rect>"#,
                fmt(element.width),
                fmt(element.height)
            );
        }
        _ => {
            let _ = write!(
                out,
                r#"<rect x="0" y="0" width="{}" height="{}"></rect>"#,
                fmt(element.width),
                fmt(element.height)
            );
        }
    }
    let _ = write!(
        out,
        r#"<text x="{}" y="{}">{}</text></g>"#,
        fmt(element.text_x),
        fmt(element.text_y),
        escape_xml_display(&element.label)
    );
}

#[derive(Clone, Copy)]
struct RailroadCssScope<I>(I);

impl<I> std::fmt::Display for RailroadCssScope<I>
where
    I: Copy + std::fmt::Display,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("#")?;
        for ch in self.0.to_string().chars() {
            if matches!(ch, '.' | ':') {
                formatter.write_str("\\")?;
            }
            let mut encoded = [0_u8; 4];
            formatter.write_str(ch.encode_utf8(&mut encoded))?;
        }
        Ok(())
    }
}

fn railroad_css<I>(style: &crate::railroad::RailroadStyle, diagram_id: I) -> String
where
    I: Copy + std::fmt::Display,
{
    let scope = RailroadCssScope(diagram_id);
    format!(
        "{scope} .railroad-diagram{{font-family:{};font-size:{}px;}}\
{scope} .railroad-terminal rect{{fill:{};stroke:{};stroke-width:{}px;}}\
{scope} .railroad-terminal text{{fill:{};font-family:{};font-size:{}px;text-anchor:middle;dominant-baseline:middle;}}\
{scope} .railroad-nonterminal rect{{fill:{};stroke:{};stroke-width:{}px;}}\
{scope} .railroad-nonterminal text{{fill:{};font-family:{};font-size:{}px;text-anchor:middle;dominant-baseline:middle;}}\
{scope} .railroad-line{{stroke:{};stroke-width:{}px;fill:none;}}\
{scope} .railroad-start circle,{scope} .railroad-end circle{{fill:{};}}\
{scope} .railroad-comment ellipse{{fill:{};stroke:{};stroke-width:{}px;}}\
{scope} .railroad-comment text{{fill:{};font-style:italic;font-family:{};font-size:{}px;text-anchor:middle;dominant-baseline:middle;}}\
{scope} .railroad-special rect{{fill:{};stroke:{};stroke-width:{}px;stroke-dasharray:5,3;}}\
{scope} .railroad-special text{{fill:{};font-family:{};font-size:{}px;text-anchor:middle;dominant-baseline:middle;}}\
{scope} .railroad-rule-name{{font-weight:bold;fill:{};font-family:{};font-size:{}px;}}\
{scope} .railroad-group{{}}",
        style.font_family,
        fmt(style.font_size),
        style.terminal_fill,
        style.terminal_stroke,
        fmt(style.stroke_width),
        style.terminal_text_color,
        style.font_family,
        fmt(style.font_size),
        style.non_terminal_fill,
        style.non_terminal_stroke,
        fmt(style.stroke_width),
        style.non_terminal_text_color,
        style.font_family,
        fmt(style.font_size),
        style.line_color,
        fmt(style.stroke_width),
        style.marker_fill,
        style.comment_fill,
        style.comment_stroke,
        fmt(style.stroke_width),
        style.comment_text_color,
        style.font_family,
        fmt(style.font_size),
        style.special_fill,
        style.special_stroke,
        fmt(style.stroke_width),
        style.non_terminal_text_color,
        style.font_family,
        fmt(style.font_size),
        style.rule_name_color,
        style.font_family,
        fmt(style.font_size)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_font_rule_matches_mermaid_namespacing() {
        let style = crate::railroad::railroad_style(&serde_json::json!({}));
        let css = railroad_css(&style, "railroad.fixture");

        assert!(css.starts_with("#railroad\\.fixture .railroad-diagram{"));
        assert!(!css.starts_with("#railroad\\.fixture.railroad-diagram{"));
    }
}
