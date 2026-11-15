use super::defs::prepare_flowchart_defs;
use super::document::{FlowchartSvgDocumentRequest, prepare_flowchart_svg_document};
use super::render_config::{FlowchartRenderConfig, prepare_flowchart_render_config};
use super::render_input::{FlowchartRenderInputs, prepare_flowchart_render_inputs};
use super::viewbox::{
    FlowchartRenderedBoundsRequest, FlowchartViewboxBounds, FlowchartViewboxBoundsRequest,
    prepare_flowchart_rendered_bounds, prepare_flowchart_viewbox_bounds,
};
use super::*;

pub(in crate::svg::parity) fn render_flowchart_svg_artifact(
    artifact: &crate::family::FlowchartFamilyArtifact<FlowchartLayout>,
    metadata: &merman_core::ParseMetadata,
    options: &SvgExecution<'_>,
    edge_paint_geometry: Option<&mut Vec<crate::model::EdgePaintGeometry>>,
) -> Result<root_svg::RootedSvg> {
    render_flowchart_svg_model(
        FlowchartSvgModelRequest {
            layout: artifact.pair().layout(),
            swimlane_layout: None,
            model: artifact.pair().semantic(),
            render_context: artifact.render_context(),
            effective_config: &metadata.effective_config,
            diagram_type: metadata.diagram_type.as_str(),
            diagram_title: metadata.title.as_deref(),
            presentation_policy: artifact.policy(),
            svg_label_sidecar: artifact.svg_label_sidecar(),
        },
        options,
        edge_paint_geometry,
    )
}

pub(super) struct FlowchartSvgModelRequest<'a> {
    pub(super) layout: &'a FlowchartLayout,
    pub(super) swimlane_layout: Option<&'a crate::model::SwimlaneLayout>,
    pub(super) model: &'a crate::flowchart::FlowchartModel,
    pub(super) render_context: &'a crate::flowchart::FlowchartRenderContext,
    pub(super) effective_config: &'a merman_core::MermaidConfig,
    pub(super) diagram_type: &'a str,
    pub(super) diagram_title: Option<&'a str>,
    pub(super) presentation_policy: Option<crate::presentation::FlowchartPresentationPolicy>,
    pub(super) svg_label_sidecar: &'a crate::flowchart::FlowchartSvgLabelSidecar,
}

pub(super) fn render_flowchart_svg_model(
    request: FlowchartSvgModelRequest<'_>,
    options: &SvgExecution<'_>,
    edge_paint_geometry: Option<&mut Vec<crate::model::EdgePaintGeometry>>,
) -> Result<root_svg::RootedSvg> {
    let FlowchartSvgModelRequest {
        layout,
        swimlane_layout,
        model,
        render_context,
        effective_config,
        diagram_type,
        diagram_title,
        presentation_policy,
        svg_label_sidecar,
    } = request;
    let render_model = crate::flowchart::FlowchartRenderModelRef::new(model, render_context);
    let model = &render_model;
    if model
        .nodes
        .iter()
        .any(|node| node.layout_shape.as_deref() == Some("ellipse"))
    {
        return Err(crate::Error::InvalidModel {
            message: "No such shape: ellipse. Please check your syntax.".to_string(),
        });
    }

    let render_timing = options.timing();
    let measurer = options.text_measurer();
    let mut timings = timing::RenderTimings::default();
    let total_timer = render_timing.start();

    let effective_config_value = effective_config.as_value();
    let hand_drawn_seed = options.rough_randomness(
        effective_config_value
            .get("handDrawnSeed")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(options.seed() as f64),
        "render.flowchart.roughjs",
    );

    let diagram_id = options.diagram_id_or("merman");
    let checkpoint_emit = || options.checkpoint_emit();
    let emit = FlowchartEmitCheckpoint::new(&checkpoint_emit);
    let _g_build_ctx = render_timing.section(&mut timings.build_ctx);

    let FlowchartRenderInputs {
        mut render_edges,
        extra_nodes,
    } = prepare_flowchart_render_inputs(model, render_context, layout.uses_elk_adapter_dom);
    if let Some(swimlane_layout) = swimlane_layout {
        super::swimlane::apply_swimlane_edge_curves(&mut render_edges, swimlane_layout);
    }

    let FlowchartRenderConfig {
        font_family,
        font_size,
        wrapping_width,
        node_html_labels,
        edge_html_labels,
        swimlane_title_html_labels,
        node_wrap_mode,
        edge_wrap_mode,
        diagram_padding,
        use_max_width,
        title_top_margin,
        node_padding,
        text_style,
        html_label_text_style,
        default_edge_interpolate,
        default_edge_style,
        node_border_color,
        node_fill_color,
        edge_corner_radius,
        edge_label_padding,
        compact_edge_corners,
    } = prepare_flowchart_render_config(
        model,
        effective_config_value,
        layout.uses_elk_adapter_dom,
        presentation_policy,
    );

    let mut nodes_by_id: FxHashMap<&str, &crate::flowchart::FlowNode> =
        FxHashMap::with_capacity_and_hasher(
            model.nodes.len() + extra_nodes.len(),
            Default::default(),
        );
    for n in &model.nodes {
        nodes_by_id.insert(n.id.as_str(), n);
    }
    for n in &extra_nodes {
        let _ = nodes_by_id.entry(n.id.as_str()).or_insert(n);
    }

    // Source-ported ELK should preserve Mermaid's edge emission order, not the layout engine's
    // internal reordering. `render_edges` already reflects the source-backed ordering rules.
    let edge_order: Vec<&str> = render_edges
        .iter()
        .map(|e| e.as_ref().id.as_str())
        .collect();
    let mut edges_by_id: FxHashMap<&str, &crate::flowchart::FlowEdge> =
        FxHashMap::with_capacity_and_hasher(render_edges.len(), Default::default());
    for e in &render_edges {
        let edge = e.as_ref();
        edges_by_id.insert(edge.id.as_str(), edge);
    }

    let swimlane_direction = swimlane_layout.map(|layout| layout.direction);
    let swimlane_lanes_by_id: FxHashMap<&str, &crate::model::SwimlaneLaneLayout> = swimlane_layout
        .into_iter()
        .flat_map(|layout| layout.lanes.iter())
        .map(|lane| (lane.id.as_str(), lane))
        .collect();
    let swimlane_edge_label_edges_by_node_id: FxHashMap<&str, &crate::flowchart::FlowEdge> =
        swimlane_layout
            .into_iter()
            .flat_map(|layout| layout.edges.iter())
            .filter_map(|layout_edge| {
                let label_node_id = layout_edge.label_node_id.as_deref()?;
                let edge = edges_by_id.get(layout_edge.id.as_str()).copied()?;
                Some((label_node_id, edge))
            })
            .collect();
    let mut subgraph_order: Vec<&str> = Vec::with_capacity(model.subgraphs.len());
    let mut subgraphs_by_id: FxHashMap<&str, &crate::flowchart::FlowSubgraph> =
        FxHashMap::with_capacity_and_hasher(model.subgraphs.len(), Default::default());
    let mut subgraph_indices_by_id: FxHashMap<&str, usize> =
        FxHashMap::with_capacity_and_hasher(model.subgraphs.len(), Default::default());
    let mut subgraph_ids_with_children: FxHashSet<&str> = FxHashSet::default();
    for (subgraph_index, sg) in model.subgraphs.iter().enumerate() {
        let id = sg.id.as_str();
        if let std::collections::hash_map::Entry::Vacant(entry) = subgraphs_by_id.entry(id) {
            entry.insert(sg);
            subgraph_indices_by_id.insert(id, subgraph_index);
            subgraph_order.push(id);
        }
        if !sg.nodes.is_empty()
            && !render_context.is_subgraph_collapsed(id)
            && render_context.collapsed_replacement(id).is_none()
        {
            subgraph_ids_with_children.insert(id);
        }
    }

    let mut parent: FxHashMap<&str, &str> = FxHashMap::default();
    for sg in model.subgraphs.iter().rev() {
        let sg_id = sg.id.as_str();
        for child in &sg.nodes {
            parent.insert(child.as_str(), sg_id);
        }
    }
    for n in &extra_nodes {
        let id = n.id.as_str();
        let Some((base, _)) = id.split_once("---") else {
            continue;
        };
        if let Some(&p) = parent.get(base) {
            parent.insert(id, p);
        }
    }

    // Layout extraction is the source of truth for recursive cluster roots. Recomputing this from
    // semantic edges loses Mermaid 11.16's explicit-direction extraction branch and can make every
    // node inside an extracted cluster disappear from the SVG DOM.
    let recursive_clusters: FxHashSet<&str> = layout
        .dom_node_order_by_root
        .keys()
        .filter(|id| !id.is_empty())
        .map(String::as_str)
        .collect();

    let mut layout_nodes_by_id: FxHashMap<&str, &LayoutNode> =
        FxHashMap::with_capacity_and_hasher(layout.nodes.len(), Default::default());
    for n in &layout.nodes {
        layout_nodes_by_id.insert(n.id.as_str(), n);
    }

    let mut layout_edges_by_id: FxHashMap<&str, &crate::model::LayoutEdge> =
        FxHashMap::with_capacity_and_hasher(layout.edges.len(), Default::default());
    for e in &layout.edges {
        layout_edges_by_id.insert(e.id.as_str(), e);
    }

    let mut layout_clusters_by_id: FxHashMap<&str, &LayoutCluster> =
        FxHashMap::with_capacity_and_hasher(layout.clusters.len(), Default::default());
    for c in &layout.clusters {
        layout_clusters_by_id.insert(c.id.as_str(), c);
    }

    // Mermaid flowchart-v2 does not translate the root `.root` group; node/edge coordinates are
    // already in the Dagre coordinate space (including Dagre's fixed `marginx/marginy=8`).
    // `diagramPadding` is applied only when computing the final SVG viewBox.
    let tx = 0.0;
    let ty = 0.0;

    let mut node_dom_index = flowchart_node_dom_indices(model);
    for node in &model.nodes {
        if let Some(index) = render_context.node_dom_index(&node.id) {
            node_dom_index.insert(node.id.as_str(), index);
        }
    }

    let flowchart_edge_trace = options.debug.flowchart_edge_trace();
    let icon_registry = options.icon_registry();
    let icon_scope_prefix = icon_registry
        .map(|_| {
            crate::svg::icon_registry::IconIdScopePrefix::from_parts(
                &[diagram_id.semantic_str(), "-flowchart-icon-"],
                options.work_meter(),
            )
        })
        .transpose()?;
    let ctx = FlowchartRenderCtx {
        model,
        diagram_id,
        diagram_type,
        tx,
        ty,
        measurer,
        config: effective_config,
        hand_drawn_seed,
        work_meter: options.work_meter(),
        emit,
        math_renderer: options.math_renderer(),
        svg_label_sidecar: Some(svg_label_sidecar),
        icon_registry,
        icon_scope_prefix,
        security_level_loose: effective_config.get_str("securityLevel") == Some("loose"),
        node_html_labels,
        edge_html_labels,
        swimlane_title_html_labels,
        uses_elk_adapter_dom: layout.uses_elk_adapter_dom,
        class_defs: &model.class_defs,
        node_border_color,
        node_fill_color,
        edge_corner_radius,
        edge_label_padding,
        compact_edge_corners,
        default_edge_interpolate,
        default_edge_style,
        trace_edge_id: flowchart_edge_trace.map(|(edge_id, _)| edge_id),
        trace_collector: flowchart_edge_trace.map(|(_, collector)| collector),
        subgraph_order,
        edge_order,
        nodes_by_id,
        edges_by_id,
        subgraphs_by_id,
        subgraph_indices_by_id,
        subgraph_ids_with_children,
        tooltips: &model.tooltips,
        recursive_clusters,
        parent,
        layout_nodes_by_id,
        layout_edges_by_id,
        layout_clusters_by_id,
        swimlane_direction,
        swimlane_lanes_by_id,
        swimlane_edge_label_edges_by_node_id,
        dom_node_order_by_root: &layout.dom_node_order_by_root,
        node_dom_index,
        node_padding,
        wrapping_width,
        node_wrap_mode,
        edge_wrap_mode,
        text_style,
        html_label_text_style,
    };

    let mut edge_path_cache: FxHashMap<&str, FlowchartEdgePathCacheEntry> =
        FxHashMap::with_capacity_and_hasher(render_edges.len(), Default::default());

    let subgraph_title_y_shift = crate::flowchart::FlowchartConfigView::new(effective_config_value)
        .render_subgraph_title_y_shift();

    fn self_loop_label_base_node_id(id: &str) -> Option<&str> {
        let mut parts = id.split("---");
        let a = parts.next()?;
        let b = parts.next()?;
        let n = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if a != b {
            return None;
        }
        if n != "1" && n != "2" {
            return None;
        }
        Some(a)
    }

    drop(_g_build_ctx);

    let mut detail = FlowchartRenderDetails::default();
    let mut viewbox_edge_curve_bounds = std::time::Duration::ZERO;
    let _g_viewbox = render_timing.section(&mut timings.viewbox);

    let effective_parent_for_id = |id: &str| -> Option<&str> {
        let mut cur = ctx.parent.get(id).copied();
        if cur.is_none()
            && let Some(base) = self_loop_label_base_node_id(id)
        {
            cur = ctx.parent.get(base).copied();
        }
        while let Some(p) = cur {
            if ctx.subgraphs_by_id.contains_key(p)
                && !ctx.recursive_clusters.contains(p)
                && !ctx.is_subgraph_collapsed(p)
            {
                cur = ctx.parent.get(p).copied();
                continue;
            }
            return Some(p);
        }
        None
    };

    let bounds = prepare_flowchart_rendered_bounds(
        FlowchartRenderedBoundsRequest {
            ctx: &ctx,
            layout,
            subgraph_title_y_shift,
        },
        &effective_parent_for_id,
    );
    let FlowchartViewboxBounds {
        diagram_title,
        title_anchor_x,
        bbox_min_x,
        bbox_min_y,
        bbox_max_x,
        bbox_max_y,
    } = prepare_flowchart_viewbox_bounds(
        FlowchartViewboxBoundsRequest {
            ctx: &ctx,
            render_edges: &render_edges,
            base_bounds: bounds,
            diagram_title,
            font_family: &font_family,
            title_top_margin,
            timing: render_timing,
            viewbox_edge_curve_bounds: &mut viewbox_edge_curve_bounds,
            detail: &mut detail,
            edge_path_cache: &mut edge_path_cache,
        },
        &effective_parent_for_id,
    )?;

    // `data_points` is written only by `finish_edge_route` inside the viewbox pass and is
    // read-only afterwards, so this is the exact point list the emitted `data-points`
    // attribute carries — before any emit-phase mutation of the cache.
    let clipped_points: Option<FxHashMap<&str, Vec<crate::model::LayoutPoint>>> =
        edge_paint_geometry.as_ref().map(|_| {
            let mut map = FxHashMap::default();
            for edge in &render_edges {
                let id = edge.as_ref().id.as_str();
                let Some(entry) = edge_path_cache.get(id) else {
                    continue;
                };
                map.insert(
                    id,
                    entry
                        .geom
                        .data_points
                        .iter()
                        .map(|point| crate::model::LayoutPoint {
                            x: point.x + entry.origin_x - ctx.tx,
                            y: point.y + entry.origin_y - ctx.ty,
                        })
                        .collect(),
                );
            }
            map
        });
    let document = prepare_flowchart_svg_document(FlowchartSvgDocumentRequest {
        family_kind: if swimlane_layout.is_some() {
            crate::family::RenderFamilyKind::Swimlane
        } else if diagram_type == "agentflow" {
            crate::family::RenderFamilyKind::Agentflow
        } else {
            crate::family::RenderFamilyKind::Flowchart
        },
        diagram_id,
        diagram_type,
        model,
        use_max_width,
        diagram_padding,
        bbox_min_x,
        bbox_min_y,
        bbox_max_x,
        bbox_max_y,
    });

    drop(_g_viewbox);
    let _g_render_svg = render_timing.section(&mut timings.render_svg);

    let css_presentation_policy = presentation_policy
        .filter(|_| crate::config::config_diagram_look(effective_config_value).is_neo());
    let mut css = flowchart_css(
        diagram_id,
        diagram_type,
        effective_config_value,
        &font_family,
        font_size,
        css_presentation_policy,
        emit,
    )?;
    let class_defs_css = options.materialize_counted_svg_component(
        "Flowchart class-definition stylesheet",
        |out| {
            write_flowchart_class_defs_css(
                out,
                diagram_id,
                effective_config_value,
                &model.class_defs,
            )
        },
        |out| {
            write_flowchart_class_defs_css(
                out,
                diagram_id,
                effective_config_value,
                &model.class_defs,
            )
        },
    )?;
    css.push_str(&class_defs_css);
    if diagram_type == "agentflow" {
        css.push_str(&super::agentflow::css(
            diagram_id,
            effective_config_value,
            emit,
        )?);
    }
    emit.checkpoint()?;
    if swimlane_layout.is_some() {
        css.push_str(&super::swimlane::swimlane_css(diagram_id, effective_config));
        emit.checkpoint()?;
    }

    let estimated_svg_bytes = 2048usize
        + css.len()
        + layout.nodes.len().saturating_mul(256)
        + render_edges.len().saturating_mul(256)
        + layout.clusters.len().saturating_mul(128);
    emit.checkpoint()?;
    let mut out = String::with_capacity(estimated_svg_bytes);

    let root_document = document.push_root_open(&mut out)?;
    document.push_accessibility_metadata(&mut out);
    out.push_str("<style>");
    out.push_str(&css);
    out.push_str("</style>");
    emit.checkpoint()?;

    let defs = prepare_flowchart_defs(diagram_id, diagram_type, &ctx);

    let mut root_session = FlowchartRootRenderSession {
        timing: render_timing,
        details: &mut detail,
        edge_cache: &mut edge_path_cache,
        edge_label_positions: edge_paint_geometry.as_ref().map(|_| FxHashMap::default()),
    };
    if layout.uses_elk_adapter_dom {
        out.push_str("<g>");
        defs.push_base_markers(&mut out);
        emit.checkpoint()?;
        defs.push_extra_markers(&mut out);
        emit.checkpoint()?;
        render_flowchart_elk_root_groups(&mut out, &ctx, &mut root_session)?;
        out.push_str("</g>");
        // The shadow filters are siblings of Mermaid's marker/root wrapper,
        // rather than children of the wrapper that owns the painted graph.
        super::super::look_defs::push_look_shadow_defs(
            &mut out,
            diagram_id,
            effective_config_value,
        );
        emit.checkpoint()?;
    } else {
        super::super::look_defs::push_look_shadow_defs(
            &mut out,
            diagram_id,
            effective_config_value,
        );
        emit.checkpoint()?;
        out.push_str("<g>");
        defs.push_base_markers(&mut out);
        emit.checkpoint()?;
        render_flowchart_root(&mut out, &ctx, None, 0.0, 0.0, &mut root_session)?;

        defs.push_extra_markers(&mut out);
        emit.checkpoint()?;
        out.push_str("</g>");
    }
    super::super::look_defs::push_look_gradient(&mut out, diagram_id, effective_config_value);
    emit.checkpoint()?;
    if let Some(title) = diagram_title.as_deref() {
        let title_x = title_anchor_x;
        let title_y = -title_top_margin;
        let _ = write!(
            &mut out,
            r#"<text text-anchor="middle" x="{}" y="{}" class="{}">{}</text>"#,
            fmt(title_x),
            fmt(title_y),
            title_css_class(diagram_type),
            escape_xml(title)
        );
    }
    out.push_str("</svg>\n");

    let edge_label_positions = root_session.edge_label_positions.take();
    drop(root_session);
    drop(_g_render_svg);
    timings.total = total_timer
        .map(merman_core::runtime::OperationTimer::elapsed)
        .unwrap_or_default();
    if render_timing.is_enabled() {
        eprintln!(
            "[render-timing] diagram=flowchart-v2 total={:?} deserialize={:?} build_ctx={:?} viewbox={:?} viewbox_edge_curve_bounds={:?} viewbox_edge_curve_lca={:?} viewbox_edge_curve_offsets={:?} viewbox_edge_curve_geom={:?} viewbox_edge_curve_bbox_union={:?} viewbox_edge_curve_geom_calls={} viewbox_edge_curve_geom_skipped_bounds={} render_svg={:?} finalize={:?} root_calls={} clusters={:?} edges_select={:?} edge_paths={:?} edge_labels={:?} dom_order={:?} nodes={:?} node_style_compile={:?} node_roughjs={:?} node_roughjs_calls={} node_label_html={:?} node_label_html_calls={} nested_roots={:?}",
            timings.total,
            timings.deserialize_model,
            timings.build_ctx,
            timings.viewbox,
            viewbox_edge_curve_bounds,
            detail.viewbox_edge_curve_lca,
            detail.viewbox_edge_curve_offsets,
            detail.viewbox_edge_curve_geom,
            detail.viewbox_edge_curve_bbox_union,
            detail.viewbox_edge_curve_geom_calls,
            detail.viewbox_edge_curve_geom_skipped_bounds,
            timings.render_svg,
            timings.finalize_svg,
            detail.root_calls,
            detail.clusters,
            detail.edges_select,
            detail.edge_paths,
            detail.edge_labels,
            detail.dom_order,
            detail.nodes,
            detail.node_style_compile,
            detail.node_roughjs,
            detail.node_roughjs_calls,
            detail.node_label_html,
            detail.node_label_html_calls,
            detail.nested_roots,
        );
    }
    if let Some(out) = edge_paint_geometry {
        out.extend(render_edges.iter().map(|edge| {
            let id = edge.as_ref().id.as_str();
            crate::model::EdgePaintGeometry {
                id: id.to_owned(),
                points: clipped_points.as_ref().and_then(|map| map.get(id).cloned()),
                label_position: edge_label_positions
                    .as_ref()
                    .and_then(|map| map.get(id).cloned()),
            }
        }));
    }
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::RenderEnvironment;
    use crate::model::{FlowchartLayout, LayoutNode};
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};
    use crate::svg::{SvgDebugOptions, SvgRenderOptions};
    use merman_core::{Engine, ParseOptions, RenderSemanticModel};

    #[test]
    fn diagram_id_terminal_precedes_later_flowchart_node_emission_error() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TD\nA@{ img: \"https://example.invalid/a.svg\", label: \"A\" }\n",
                ParseOptions::strict(),
            )
            .expect("parse succeeds")
            .expect("detects Flowchart");
        let render_context = parsed
            .flowchart_render_context()
            .expect("Flowchart render context")
            .clone();
        let (metadata, semantic) = parsed.into_parts();
        let RenderSemanticModel::Flowchart(mut model) = semantic else {
            panic!("expected Flowchart model");
        };
        let node = model.nodes.first_mut().expect("fixture node");
        assert_eq!(node.layout_shape.as_deref(), Some("imageSquare"));
        node.img = None;

        let layout = FlowchartLayout {
            nodes: vec![LayoutNode {
                id: node.id.clone(),
                x: 40.0,
                y: 30.0,
                width: 80.0,
                height: 60.0,
                is_cluster: false,
                label_width: Some(10.0),
                label_height: Some(10.0),
            }],
            edges: Vec::new(),
            clusters: Vec::new(),
            bounds: None,
            dom_node_order_by_root: std::collections::HashMap::from([(
                String::new(),
                vec![node.id.clone()],
            )]),
            uses_elk_adapter_dom: false,
        };
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, 1)
            .expect("valid SVG byte limit");
        let session = RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session()
            .expect("begin render session");
        let request = SvgRenderOptions {
            diagram_id: Some("terminal".to_string()),
            ..SvgRenderOptions::default()
        };
        let debug = SvgDebugOptions::default();
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let sidecar = crate::flowchart::FlowchartSvgLabelSidecar::default();

        let error = render_flowchart_svg_model(
            FlowchartSvgModelRequest {
                layout: &layout,
                swimlane_layout: None,
                model: &model,
                render_context: &render_context,
                effective_config: &metadata.effective_config,
                diagram_type: metadata.diagram_type.as_str(),
                diagram_title: metadata.title.as_deref(),
                presentation_policy: None,
                svg_label_sidecar: &sidecar,
            },
            &execution,
            None,
        )
        .expect_err("diagram-ID rejection must stop before the invalid image node is emitted");

        let crate::Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte rejection, got {error}");
        };
        assert_eq!(details.limit, ResourceLimitId::MaxSvgBytes.as_str());
    }

    #[test]
    fn elk_terminal_straightening_reaches_svg_and_preserves_ports() {
        use crate::model::{LayoutEdge, LayoutPoint};
        use base64::Engine as _;

        // Pinned Mermaid geometry.spec.ts terminal-jog case, with measured rectangles whose
        // boundaries coincide with its ports. The layout is fixed to isolate SVG postprocessing.
        let render = |enabled: bool, trace: bool| {
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(
                    &format!("---\nconfig:\n  elk:\n    straightenEdges: {enabled}\n---\nflowchart LR\nA --> B"),
                    ParseOptions::strict(),
                ).unwrap().unwrap();
            let render_context = parsed.flowchart_render_context().unwrap().clone();
            let (metadata, semantic) = parsed.into_parts();
            let RenderSemanticModel::Flowchart(model) = semantic else {
                panic!("Flowchart");
            };
            let edge_id = model.edges[0].id.clone();
            let layout = FlowchartLayout {
                nodes: [("A", 153.0, 116.25), ("B", 400.0, 320.0)]
                    .into_iter()
                    .map(|(id, x, y)| LayoutNode {
                        id: id.into(),
                        x,
                        y,
                        width: 80.0,
                        height: 40.0,
                        is_cluster: false,
                        label_width: Some(10.0),
                        label_height: Some(10.0),
                    })
                    .collect(),
                edges: vec![LayoutEdge {
                    id: edge_id.clone(),
                    from: "A".into(),
                    to: "B".into(),
                    from_cluster: None,
                    to_cluster: None,
                    points: [
                        (193.0, 116.25),
                        (218.0, 116.25),
                        (218.0, 119.5),
                        (400.0, 119.5),
                        (400.0, 300.0),
                    ]
                    .into_iter()
                    .map(|(x, y)| LayoutPoint { x, y })
                    .collect(),
                    label: None,
                    start_label_left: None,
                    start_label_right: None,
                    end_label_left: None,
                    end_label_right: None,
                    start_marker: None,
                    end_marker: None,
                    stroke_dasharray: None,
                }],
                clusters: Vec::new(),
                bounds: None,
                dom_node_order_by_root: std::collections::HashMap::from([(
                    String::new(),
                    vec!["A".into(), "B".into()],
                )]),
                uses_elk_adapter_dom: true,
            };
            let session = RenderEnvironment::deterministic().begin_session().unwrap();
            let request = SvgRenderOptions {
                diagram_id: Some("terminal-jog".into()),
                ..SvgRenderOptions::default()
            };
            let debug = if trace {
                SvgDebugOptions::default().with_flowchart_edge_trace(
                    edge_id,
                    crate::svg::FlowchartEdgeTraceCollector::default(),
                )
            } else {
                SvgDebugOptions::default()
            };
            let execution = SvgExecution::new(&request, &debug, &session).unwrap();
            let sidecar = crate::flowchart::FlowchartSvgLabelSidecar::default();
            render_flowchart_svg_model(
                FlowchartSvgModelRequest {
                    layout: &layout,
                    swimlane_layout: None,
                    model: &model,
                    render_context: &render_context,
                    effective_config: &metadata.effective_config,
                    diagram_type: metadata.diagram_type.as_str(),
                    diagram_title: None,
                    presentation_policy: None,
                    svg_label_sidecar: &sidecar,
                },
                &execution,
                None,
            )
            .unwrap()
            .to_string()
        };
        let route = |svg: &str| {
            let doc = roxmltree::Document::parse(svg).unwrap();
            let path = doc
                .descendants()
                .find(|n| n.attribute("data-points").is_some())
                .unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(path.attribute("data-points").unwrap())
                .unwrap();
            let points: Vec<LayoutPoint> = serde_json::from_slice(&bytes).unwrap();
            (
                path.attribute("d").unwrap().to_owned(),
                points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(),
            )
        };
        let off = render(false, false);
        let on = render(true, false);
        let (old_path, old_points) = route(&off);
        let (new_path, new_points) = route(&on);
        assert_eq!(old_points.len(), 5);
        assert_eq!(new_points.len(), 3);
        assert_eq!(old_points.first(), new_points.first());
        assert_eq!(old_points.last(), new_points.last());
        assert_eq!(new_points[0].1, new_points[1].1);
        assert_ne!(old_path, new_path);
        assert_eq!(
            on,
            render(true, true),
            "diagnostics must preserve processed geometry"
        );
    }
}
