use super::super::*;
use crate::model::TreeViewNodeLayout;
use crate::svg::icon_registry::mermaid_unknown_icon_svg;
use crate::tree_view::{
    TREE_VIEW_DESCRIPTION_FONT_STYLE, TREE_VIEW_DIRECTORY_FONT_WEIGHT,
    TREE_VIEW_HIGHLIGHT_RECT_EXTENSION, TREE_VIEW_HIGHLIGHT_WIDTH_GROWTH, TREE_VIEW_ICON_SIZE,
    is_tree_view_highlight_class,
};
use merman_core::diagrams::tree_view::TreeViewDiagramRenderModel;

const TREE_VIEW_ICON_PREFIX: &str = "mermaid-treeview";
const TREE_VIEW_DIRECTORY_NODE_TYPE: &str = "directory";

struct TreeViewNodeRenderContext<'a, 'id> {
    layout: &'a TreeViewDiagramLayout,
    diagram_id: SvgDiagramId<'id>,
    icon_registry: Option<&'a crate::svg::IconRegistry>,
    effective_config: &'a merman_core::MermaidConfig,
    work_meter: &'a crate::resources::OperationWorkMeter,
}

pub(crate) fn render_tree_view_diagram_svg_model(
    layout: &TreeViewDiagramLayout,
    model: &TreeViewDiagramRenderModel,
    effective_config: &merman_core::MermaidConfig,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let effective_config_value = effective_config.as_value();
    let diagram_id = options.diagram_id_or("treeView");
    let acc_title = model
        .acc_title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let acc_descr = model
        .acc_descr
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    let aria_labelledby = acc_title.map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = acc_descr.map(|_| format!("chart-desc-{diagram_id}"));
    let root_bounds = root_svg::DiagramBounds::from_view_box(
        -layout.line_thickness / 2.0,
        0.0,
        layout.total_width,
        layout.total_height,
    );
    let root_spec = root_svg::RootViewportSpec::mermaid(root_bounds, layout.use_max_width);

    let mut out = String::new();
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "treeView");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::TreeView, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    options.checkpoint_emit()?;

    let css = tree_view_css(diagram_id, effective_config_value);
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
    let _ = write!(&mut out, "<style>{css}</style>");
    options.checkpoint_emit()?;
    out.push_str("<g/>");
    out.push_str(r#"<g class="tree-view">"#);
    let mut next_node = 0usize;
    let highlighted_node_count = layout
        .nodes
        .iter()
        .filter(|node| is_tree_view_highlight_class(node.css_class.as_deref()))
        .count();
    let mut width_before_highlight =
        layout.total_width - highlighted_node_count as f64 * TREE_VIEW_HIGHLIGHT_WIDTH_GROWTH;
    let node_context = TreeViewNodeRenderContext {
        layout,
        diagram_id,
        icon_registry: options.icon_registry(),
        effective_config,
        work_meter: options.work_meter(),
    };
    for line in &layout.lines {
        if line.kind == "horizontal"
            && let Some(node) = layout.nodes.get(next_node)
        {
            push_tree_view_node(&mut out, node, &node_context, &mut width_before_highlight)?;
            next_node += 1;
        }
        let _ = write!(
            &mut out,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke-width="{}" class="treeView-node-line"></line>"#,
            fmt(line.x1),
            fmt(line.y1),
            fmt(line.x2),
            fmt(line.y2),
            fmt(line.stroke_width)
        );
    }
    for node in layout.nodes.iter().skip(next_node) {
        push_tree_view_node(&mut out, node, &node_context, &mut width_before_highlight)?;
    }
    out.push_str("</g></svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

fn push_tree_view_node(
    out: &mut String,
    node: &TreeViewNodeLayout,
    context: &TreeViewNodeRenderContext<'_, '_>,
    width_before_highlight: &mut f64,
) -> Result<()> {
    out.push_str("<g>");
    let label_classes = tree_view_label_classes(node);
    if is_tree_view_highlight_class(node.css_class.as_deref()) {
        let rect_width =
            (*width_before_highlight - node.x + TREE_VIEW_HIGHLIGHT_RECT_EXTENSION).max(0.0);
        let _ = write!(
            out,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="3" class="treeView-highlight-bg"></rect>"#,
            fmt(node.x),
            fmt(node.y + 1.0),
            fmt(rect_width),
            fmt((node.height - 2.0).max(0.0))
        );
        *width_before_highlight += TREE_VIEW_HIGHLIGHT_WIDTH_GROWTH;
    }
    if let Some(icon) = node.resolved_icon.as_deref() {
        let icon_svg = tree_view_icon_svg(
            icon,
            context.diagram_id,
            node.id,
            context.icon_registry,
            context.effective_config,
            context.work_meter,
        )?;
        let _ = write!(
            out,
            r#"<g class="treeView-node-icon" transform="translate({}, {})">{}</g>"#,
            fmt(node.x + context.layout.padding_x),
            fmt(node.y + context.layout.padding_y),
            icon_svg
        );
    }
    let _ = write!(
        out,
        r#"<text dominant-baseline="middle" class="{}" x="{}" y="{}">{}</text>"#,
        escape_xml(&label_classes),
        fmt(node.label_x),
        fmt(node.label_y),
        escape_xml(&node.name)
    );
    if let (Some(description), Some(description_x)) =
        (node.description.as_deref(), node.description_x)
    {
        let _ = write!(
            out,
            r#"<text dominant-baseline="middle" class="treeView-node-description" x="{}" y="{}">{}</text>"#,
            fmt(description_x),
            fmt(node.label_y),
            escape_xml(description)
        );
    }
    out.push_str("</g>");
    Ok(())
}

fn tree_view_css(diagram_id: SvgDiagramId<'_>, effective_config: &serde_json::Value) -> String {
    let theme = PresentationTheme::new(effective_config).tree_view();

    format!(
        "#{diagram_id} .treeView-node-label {{ font-size: {}; fill: {}; white-space: pre; }} #{diagram_id} .treeView-node-dir {{ font-weight: {}; }} #{diagram_id} .treeView-node-line {{ stroke: {}; }} #{diagram_id} .treeView-node-icon {{ color: {}; }} #{diagram_id} .treeView-node-description {{ font-size: {}; fill: {}; font-style: {}; white-space: pre; }} #{diagram_id} .treeView-highlight-bg {{ fill: {}; stroke: {}; stroke-width: 1; }}",
        theme.label_font_size_css,
        theme.label_color,
        TREE_VIEW_DIRECTORY_FONT_WEIGHT,
        theme.line_color,
        theme.icon_color,
        theme.label_font_size_css,
        theme.description_color,
        TREE_VIEW_DESCRIPTION_FONT_STYLE,
        theme.highlight_bg,
        theme.highlight_stroke
    )
}

fn tree_view_label_classes(node: &TreeViewNodeLayout) -> String {
    let mut classes = vec!["treeView-node-label".to_string()];
    if node.node_type == TREE_VIEW_DIRECTORY_NODE_TYPE {
        classes.push("treeView-node-dir".to_string());
    }
    if let Some(css_class) = node.css_class.as_deref() {
        classes.extend(
            css_class
                .split_whitespace()
                .filter(|class| !class.is_empty())
                .map(str::to_string),
        );
    }
    classes.join(" ")
}

fn tree_view_icon_svg(
    icon: &str,
    diagram_id: SvgDiagramId<'_>,
    node_id: i64,
    icon_registry: Option<&crate::svg::IconRegistry>,
    effective_config: &merman_core::MermaidConfig,
    work_meter: &crate::resources::OperationWorkMeter,
) -> Result<String> {
    if let Some(body) = tree_view_icon_body(icon) {
        return Ok(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 24 24">{body}</svg>"#,
            fmt(TREE_VIEW_ICON_SIZE),
            fmt(TREE_VIEW_ICON_SIZE)
        ));
    }

    let node_id = node_id.to_string();
    let icon_svg = match icon_registry {
        Some(registry) => {
            let prefix = crate::svg::icon_registry::IconIdScopePrefix::from_parts(
                &["tree-view-", diagram_id.semantic_str(), "-"],
                work_meter,
            )?;
            let id_scope = prefix.scope_parts(&[&node_id], work_meter)?;
            registry.render_icon(crate::svg::icon_registry::IconRenderRequest {
                icon_name: icon,
                width_px: TREE_VIEW_ICON_SIZE,
                height_px: TREE_VIEW_ICON_SIZE,
                fallback_prefix: None,
                extra_class: None,
                id_scope,
                effective_config,
                work_meter,
            })?
        }
        None => None,
    }
    .unwrap_or_else(|| {
        mermaid_unknown_icon_svg(fmt(TREE_VIEW_ICON_SIZE), fmt(TREE_VIEW_ICON_SIZE))
    });
    Ok(icon_svg)
}

fn tree_view_icon_body(icon: &str) -> Option<&'static str> {
    match icon
        .strip_prefix(TREE_VIEW_ICON_PREFIX)?
        .strip_prefix(':')?
    {
        "folder" => Some(
            r#"<path fill="currentColor" d="M10.59 4.59A2 2 0 0 0 9.17 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.17z"/>"#,
        ),
        "file" => Some(
            r#"<path fill="currentColor" fill-rule="evenodd" d="M6 2a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8.83a2 2 0 0 0-.59-1.42l-4.82-4.82A2 2 0 0 0 13.17 2H6Zm7.5 1.9l4.6 4.6h-3.6a1 1 0 0 1-1-1V3.9Z" clip-rule="evenodd"/>"#,
        ),
        _ => None,
    }
}
