use super::config::{DEFAULT_LANE_ID, DEFAULT_LANE_PADDING, GROUP_PADDING};
use super::working::{WorkingEdge, WorkingLayout, WorkingNode, WorkingNodeKind};
use crate::flowchart::{
    FlowchartConfigView, FlowchartLabelMetricsRequest, FlowchartRenderModelRef,
    FlowchartSvgLabelOwner, FlowchartSvgLabelSidecarBuilder, FlowchartSvgWidthMode,
    NodeLayoutDimensionsRequest, flowchart_effective_text_style_for_classes,
    flowchart_effective_text_style_for_node_classes, flowchart_label_metrics_for_layout,
    flowchart_node_svg_width_mode, flowchart_swimlane_label_rect_text_style,
    measure_flowchart_svg_label_for_layout, node_layout_dimensions,
};
use crate::math::MathRenderer;
use crate::model::SwimlaneDirection;
use crate::text::{TextMeasurer, WrapMode};
use indexmap::IndexMap;
use merman_core::MermaidConfig;
use merman_core::diagrams::flowchart::{
    FlowEdge, FlowNode, FlowSubgraph, FlowchartModel, FlowchartRenderContext,
};
use std::collections::{HashMap, HashSet};

fn normalize_direction(direction: Option<&str>) -> SwimlaneDirection {
    match direction
        .unwrap_or("TB")
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "LR" => SwimlaneDirection::Lr,
        "BT" => SwimlaneDirection::Bt,
        "RL" => SwimlaneDirection::Rl,
        "TB" | "TD" => SwimlaneDirection::Tb,
        _ => SwimlaneDirection::Tb,
    }
}

struct MeasureContext<'a> {
    model: &'a FlowchartRenderModelRef<'a>,
    config: &'a MermaidConfig,
    measurer: &'a dyn TextMeasurer,
    math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    direction: SwimlaneDirection,
    settings: &'a crate::flowchart::FlowchartLayoutSettings,
    title_html_labels: bool,
    svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
}

fn measure_content_node(
    node: &FlowNode,
    render_label: &str,
    owner: Option<FlowchartSvgLabelOwner>,
    ctx: &MeasureContext<'_>,
) -> WorkingNode {
    let wrap_mode = ctx.settings.node_wrap_mode;
    let base_style = if wrap_mode == WrapMode::HtmlLike {
        &ctx.settings.html_label_text_style
    } else {
        &ctx.settings.text_style
    };
    let style = flowchart_effective_text_style_for_node_classes(
        base_style,
        &ctx.model.class_defs,
        &node.classes,
        &node.styles,
    );
    let label = render_label;
    let semantic_label = node.label.as_deref().unwrap_or(&node.id);
    let label_type = node.label_type.as_deref().unwrap_or("text");
    let svg_width_mode = flowchart_node_svg_width_mode(
        label,
        label_type,
        wrap_mode,
        node.layout_shape.as_deref().unwrap_or("squareRect"),
    );
    let metrics = measure_flowchart_svg_label_for_layout(
        ctx.svg_label_sidecar,
        owner,
        owner.map(|_| node.id.as_str()),
        FlowchartLabelMetricsRequest {
            measurer: ctx.measurer,
            raw_label: label,
            label_type,
            style: style.as_ref(),
            max_width_px: Some(ctx.settings.wrapping_width),
            wrap_mode,
            config: ctx.config,
            math_renderer: ctx.math_renderer,
        },
        svg_width_mode,
    );

    let (width, height) = node_layout_dimensions(NodeLayoutDimensionsRequest {
        layout_shape: node.layout_shape.as_deref(),
        layout_direction: ctx.direction.as_str(),
        metrics,
        has_label: !label.is_empty(),
        wrapping_width: ctx.settings.wrapping_width,
        node_constraint: node.constraint.as_deref(),
        padding: ctx.settings.node_padding,
        look_is_neo: crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        state_padding: ctx.settings.state_padding,
        node_icon: node.icon.as_deref(),
        node_img: node.img.as_deref(),
        node_pos: node.pos.as_deref(),
        node_asset_width: node.asset_width,
        node_asset_height: node.asset_height,
    });

    WorkingNode {
        id: node.id.clone(),
        // Geometry follows the parser-owned render spelling, while the public layout projection
        // remains a projection of the semantic model.
        label: semantic_label.to_string(),
        label_type: label_type.to_string(),
        shape: node
            .layout_shape
            .clone()
            .unwrap_or_else(|| "squareRect".to_string()),
        kind: WorkingNodeKind::Content,
        parent_id: None,
        top_lane_id: None,
        requested_dir: None,
        padding: ctx.settings.node_padding,
        x: 0.0,
        y: 0.0,
        width,
        height,
        label_width: metrics.width.max(0.0),
        label_height: metrics.height.max(0.0),
        layer: 0,
        order: 0,
        content_top: None,
        title_rect: None,
    }
}

fn measure_edge_label(
    edge: &FlowEdge,
    render_label: &str,
    label_node_id: String,
    parent_id: Option<String>,
    owner: FlowchartSvgLabelOwner,
    ctx: &MeasureContext<'_>,
) -> WorkingNode {
    // Mermaid turns Swimlane edge labels into fresh `labelRect` nodes. The conversion copies the
    // label and label style, but deliberately does not copy `edge.labelType`; `labelHelper`
    // therefore treats the new node as ordinary non-Markdown text. Its initial width is zero, so
    // createText also uses the configured Flowchart wrapping width rather than the ordinary
    // Flowchart edge-label constant.
    let wrap_mode = ctx.settings.node_wrap_mode;
    let base_style = if wrap_mode == WrapMode::HtmlLike {
        &ctx.settings.html_label_text_style
    } else {
        &ctx.settings.text_style
    };
    let default_edge_styles = ctx
        .model
        .edge_defaults
        .as_ref()
        .map_or(&[][..], |defaults| defaults.style.as_slice());
    let style =
        flowchart_swimlane_label_rect_text_style(base_style, default_edge_styles, &edge.style);
    let label = render_label;
    let semantic_label = edge.label.as_deref().unwrap_or_default();
    let label_type = "text";
    let metrics = measure_flowchart_svg_label_for_layout(
        ctx.svg_label_sidecar,
        Some(owner),
        Some(edge.id.as_str()),
        FlowchartLabelMetricsRequest {
            measurer: ctx.measurer,
            raw_label: label,
            label_type,
            style: style.as_ref(),
            max_width_px: Some(ctx.settings.wrapping_width),
            wrap_mode,
            config: ctx.config,
            math_renderer: ctx.math_renderer,
        },
        FlowchartSvgWidthMode::Bbox,
    );

    WorkingNode {
        id: label_node_id,
        label: semantic_label.to_string(),
        label_type: label_type.to_string(),
        shape: "labelRect".to_string(),
        kind: WorkingNodeKind::EdgeLabel,
        parent_id,
        top_lane_id: None,
        requested_dir: None,
        padding: 0.0,
        x: 0.0,
        y: 0.0,
        // createGraphWithElements overwrites labelRect's hidden 0.1 x 0.1 SVG rect with the
        // complete label group's measured bbox before the Swimlane layout core runs.
        width: metrics.width.max(0.0),
        height: metrics.height.max(0.0),
        label_width: metrics.width.max(0.0),
        label_height: metrics.height.max(0.0),
        layer: 0,
        order: 0,
        content_top: None,
        title_rect: None,
    }
}

fn working_edge(edge: &FlowEdge) -> WorkingEdge {
    WorkingEdge {
        id: edge.id.clone(),
        from: edge.from.clone(),
        to: edge.to.clone(),
        reference_id: edge.id.clone(),
        label_node_id: None,
        reversed_for_layout: false,
        points: Vec::new(),
    }
}

fn measure_group_title(
    declaration_ordinal: usize,
    subgraph: &FlowSubgraph,
    render_title: &str,
    ctx: &MeasureContext<'_>,
) -> (f64, f64) {
    // The dedicated Mermaid Swimlane cluster renderer reads `flowchart.htmlLabels` directly.
    // This deliberately differs from ordinary Flowchart clusters, which use the effective
    // root-first compatibility setting.
    let wrap_mode = if ctx.title_html_labels {
        WrapMode::HtmlLike
    } else {
        WrapMode::SvgLike
    };
    let base_style = if wrap_mode == WrapMode::HtmlLike {
        &ctx.settings.html_label_text_style
    } else {
        &ctx.settings.text_style
    };
    let (classes, styles) = ctx
        .model
        .effective_subgraph_css(declaration_ordinal, subgraph);
    let style = flowchart_effective_text_style_for_classes(
        base_style,
        &ctx.model.class_defs,
        classes,
        styles,
    );
    // Mermaid 11.16's dedicated Swimlane renderer omits createText's `markdown` option, so the
    // default `true` applies independently of FlowDB's public subgraph labelType.
    let render_label_type = "markdown";
    let metrics = flowchart_label_metrics_for_layout(FlowchartLabelMetricsRequest {
        measurer: ctx.measurer,
        raw_label: render_title,
        label_type: render_label_type,
        style: style.as_ref(),
        max_width_px: Some(ctx.settings.cluster_title_wrapping_width),
        wrap_mode,
        config: ctx.config,
        math_renderer: ctx.math_renderer,
    });
    (metrics.width.max(0.0), metrics.height.max(0.0))
}

pub(super) fn prepare(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    svg_label_sidecar: Option<&FlowchartSvgLabelSidecarBuilder>,
) -> WorkingLayout {
    let render_model = FlowchartRenderModelRef::new(model, render_label_sources);
    let model = &render_model;
    let direction = normalize_direction(model.direction.as_deref());
    let config_view = FlowchartConfigView::new(effective_config.as_value());
    let swimlane_title_html_labels = config_view.swimlane_title_html_labels();
    let settings = config_view.layout_settings();
    let measure_ctx = MeasureContext {
        model,
        config: effective_config,
        measurer,
        math_renderer,
        direction,
        settings: &settings,
        title_html_labels: swimlane_title_html_labels,
        svg_label_sidecar,
    };

    // FlowDB builds parentDB by walking subgraphs from last to first.
    let mut parent_by_id: HashMap<String, String> = HashMap::new();
    for subgraph in model.subgraphs.iter().rev() {
        for child in &subgraph.nodes {
            parent_by_id.insert(child.clone(), subgraph.id.clone());
        }
    }

    let mut nodes = IndexMap::new();
    for subgraph_index in (0..model.subgraphs.len()).rev() {
        let subgraph = &model.subgraphs[subgraph_index];
        let render_title = model.subgraph_title_for_render(subgraph_index, subgraph);
        let (label_width, label_height) =
            measure_group_title(subgraph_index, subgraph, render_title, &measure_ctx);
        nodes.insert(
            subgraph.id.clone(),
            WorkingNode {
                id: subgraph.id.clone(),
                label: subgraph.title.clone(),
                label_type: subgraph
                    .label_type
                    .clone()
                    .unwrap_or_else(|| "text".to_string()),
                shape: "rect".to_string(),
                kind: WorkingNodeKind::Group,
                parent_id: parent_by_id.get(&subgraph.id).cloned(),
                top_lane_id: None,
                requested_dir: subgraph.dir.as_ref().map(|dir| {
                    if dir.eq_ignore_ascii_case("TD") {
                        "TB".to_string()
                    } else {
                        dir.trim().to_ascii_uppercase()
                    }
                }),
                padding: GROUP_PADDING,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                label_width,
                label_height,
                layer: 0,
                order: 0,
                content_top: None,
                title_rect: None,
            },
        );
    }

    let group_ids: HashSet<String> = model
        .subgraphs
        .iter()
        .map(|subgraph| subgraph.id.clone())
        .collect();
    for (node_index, node) in model.nodes.iter().enumerate() {
        if group_ids.contains(&node.id) {
            continue;
        }
        let render_label = model.node_label_for_render(node).unwrap_or(&node.id);
        let mut measured = measure_content_node(
            node,
            render_label,
            Some(FlowchartSvgLabelOwner::SwimlaneNode(node_index)),
            &measure_ctx,
        );
        measured.parent_id = parent_by_id.get(&node.id).cloned();
        nodes.insert(node.id.clone(), measured);
    }
    for id in &model.vertex_calls {
        if nodes.contains_key(id) {
            continue;
        }
        let synthetic = FlowNode {
            id: id.clone(),
            provenance: Default::default(),
            label: Some(id.clone()),
            label_type: Some("text".to_string()),
            layout_shape: Some("squareRect".to_string()),
            shape: None,
            icon: None,
            form: None,
            pos: None,
            img: None,
            constraint: None,
            asset_width: None,
            asset_height: None,
            classes: Vec::new(),
            styles: Vec::new(),
            link: None,
            link_target: None,
            have_callback: false,
        };
        let mut measured = measure_content_node(&synthetic, id, None, &measure_ctx);
        measured.parent_id = parent_by_id.get(id).cloned();
        nodes.insert(id.clone(), measured);
    }

    let loose_ids: Vec<String> = nodes
        .values()
        .filter(|node| !node.is_group() && node.parent_id.is_none())
        .map(|node| node.id.clone())
        .collect();
    if !loose_ids.is_empty() {
        if let Some(default_lane) = nodes.get_mut(DEFAULT_LANE_ID) {
            // Mermaid reuses an explicit group with the reserved id. Preserve
            // its title, classes, styles, and measured geometry; only apply
            // the swimlane shape/direction required by the layout adapter.
            if default_lane.is_group() {
                default_lane.shape = "swimlane".to_string();
                default_lane.requested_dir = Some(direction.as_str().to_string());
            }
        } else {
            nodes.insert(
                DEFAULT_LANE_ID.to_string(),
                WorkingNode {
                    id: DEFAULT_LANE_ID.to_string(),
                    label: String::new(),
                    label_type: "text".to_string(),
                    shape: "swimlane".to_string(),
                    kind: WorkingNodeKind::Group,
                    parent_id: None,
                    top_lane_id: None,
                    requested_dir: Some(direction.as_str().to_string()),
                    padding: DEFAULT_LANE_PADDING,
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                    label_width: 0.0,
                    label_height: 0.0,
                    layer: 0,
                    order: 0,
                    content_top: None,
                    title_rect: None,
                },
            );
        }
        for id in loose_ids {
            if let Some(node) = nodes.get_mut(&id) {
                node.parent_id = Some(DEFAULT_LANE_ID.to_string());
            }
        }
    }

    // Only top-level groups are swimlanes. Nested groups keep the regular rect shape.
    for node in nodes.values_mut() {
        if node.is_group() && node.parent_id.is_none() {
            node.shape = "swimlane".to_string();
            node.requested_dir = Some(direction.as_str().to_string());
        }
    }

    let mut original_edges = Vec::with_capacity(model.edges.len());
    let mut graph_edges = Vec::with_capacity(model.edges.len() * 2);
    for (edge_index, edge) in model.edges.iter().enumerate() {
        let mut original = working_edge(edge);
        let render_label = model.edge_label_for_render(edge);
        let has_label = render_label.is_some_and(|label| !label.is_empty());
        if has_label && nodes.contains_key(&edge.from) && nodes.contains_key(&edge.to) {
            let label_node_id = format!("edge-label-{}-{}-{}", edge.from, edge.to, edge.id);
            let source_parent = nodes
                .get(&edge.from)
                .and_then(|node| node.parent_id.clone());
            let target_parent = nodes.get(&edge.to).and_then(|node| node.parent_id.clone());
            let label_parent = if source_parent != target_parent {
                target_parent
            } else {
                source_parent
            };
            let label_node = measure_edge_label(
                edge,
                render_label.unwrap_or_default(),
                label_node_id.clone(),
                label_parent,
                FlowchartSvgLabelOwner::SwimlaneEdgeLabel(edge_index),
                &measure_ctx,
            );
            nodes.insert(label_node_id.clone(), label_node);
            original.label_node_id = Some(label_node_id.clone());
            graph_edges.push(WorkingEdge {
                id: format!("{}-to-label", edge.id),
                from: edge.from.clone(),
                to: label_node_id.clone(),
                reference_id: edge.id.clone(),
                label_node_id: None,
                reversed_for_layout: false,
                points: Vec::new(),
            });
            graph_edges.push(WorkingEdge {
                id: format!("{}-from-label", edge.id),
                from: label_node_id,
                to: edge.to.clone(),
                reference_id: edge.id.clone(),
                label_node_id: None,
                reversed_for_layout: false,
                points: Vec::new(),
            });
        } else {
            graph_edges.push(original.clone());
        }
        original_edges.push(original);
    }

    let group_order: Vec<String> = nodes
        .values()
        .filter(|node| node.is_group())
        .map(|node| node.id.clone())
        .collect();
    let top_lane_order = group_order
        .iter()
        .rev()
        .filter(|id| nodes.get(*id).is_some_and(|node| node.parent_id.is_none()))
        .cloned()
        .collect();

    let mut layout = WorkingLayout {
        direction,
        nodes,
        graph_edges,
        original_edges,
        top_lane_order,
    };
    layout.refresh_top_lane_ids();
    layout
}
