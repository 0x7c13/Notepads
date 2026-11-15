use super::*;

pub(in crate::svg::parity) fn render_state_diagram_svg_model(
    layout: &StateDiagramLayout,
    model: &StateSvgModel,
    effective_config: &serde_json::Value,
    diagram_title: Option<&str>,
    measurer: &dyn TextMeasurer,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let timing = options.timing();
    let mut timings = super::timing::RenderTimings::default();
    let total_timer = timing.start();

    let diagram_id = options.diagram_id_or("merman");

    let _g_build_ctx = timing.section(&mut timings.build_ctx);

    let mut hidden_prefixes: Vec<String> = Vec::new();
    for (id, st) in &model.states {
        let Some(note) = st.note.as_ref() else {
            continue;
        };
        if note.text.trim().is_empty() {
            continue;
        }
        if note.position.is_none() {
            hidden_prefixes.push(id.clone());
        }
    }

    // Mermaid computes the final root viewport from DOM `svg.getBBox()` plus a fixed padding
    // (`setupViewPortForSVG(svg, padding=8)`). It does *not* pre-normalize the coordinate space by
    // shifting the entire rendered graph to start at (0,0).
    //
    // Keep the top-level origin at (0,0) and derive `viewBox` / `max-width` later from the emitted
    // SVG bounds approximation (see below).
    let viewport_padding = 8.0;
    let origin_x = 0.0;
    let origin_y = 0.0;

    let diagram_title = diagram_title
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let state_render_settings =
        crate::state::StateConfigView::new(effective_config).render_settings();
    let title_top_margin = state_render_settings.title_top_margin;
    let hand_drawn_seed = options.rough_randomness(
        state_render_settings.hand_drawn_seed,
        "render.state.roughjs",
    );
    let rough_cache = StateRoughCache::default();

    let has_acc_title = model
        .acc_title
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let has_acc_descr = model
        .acc_descr
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());

    let text_style = state_render_settings.text_style.clone();

    let mut nodes_by_id: FxHashMap<&str, &StateSvgNode> =
        FxHashMap::with_capacity_and_hasher(model.nodes.len(), Default::default());
    for n in &model.nodes {
        nodes_by_id.insert(n.id.as_str(), n);
    }

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

    let mut parent: FxHashMap<&str, &str> =
        FxHashMap::with_capacity_and_hasher(model.nodes.len(), Default::default());
    for n in &model.nodes {
        if let Some(p) = n.parent_id.as_deref() {
            parent.insert(n.id.as_str(), p);
        }
    }

    // Mermaid's state diagram DOM insertion order follows the order of `StateDB.getData().nodes`
    // (see `dataFetcher.ts` + dagre renderer `graph.nodes()` iteration). Our semantic model's
    // `nodes` already preserves that first-seen insertion order, so use it directly.
    let node_order: Vec<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();

    let mut ctx = StateRenderCtx {
        diagram_id,
        uses_elk_adapter_dom: layout.uses_elk_adapter_dom,
        elk_edge_paths: &layout.elk_edge_paths,
        elk_line_hop_paths: FxHashMap::default(),
        diagram_look: state_render_settings.diagram_look,
        palette_size: state_palette_size(effective_config),
        hand_drawn_seed,
        html_labels: state_render_settings.html_labels,
        html_label_wrapping_width: state_render_settings.html_label_wrapping_width,
        label_min_width: state_render_settings.label_min_width,
        state_padding: state_render_settings.state_padding,
        node_order,
        nodes_by_id,
        layout_nodes_by_id,
        layout_edges_by_id,
        layout_clusters_by_id,
        parent,
        nested_roots: std::collections::BTreeSet::new(),
        hidden_prefixes,
        security_level_loose: state_render_settings.security_level_loose,
        links: &model.links,
        states: &model.states,
        edges: &model.edges,
        include_edges: options.debug.include_edges,
        include_nodes: options.debug.include_nodes,
        measurer,
        text_style,
        theme_defaults: StateThemeDefaults::from_config(effective_config),
        rough_cache,
    };

    fn compute_state_nested_roots(ctx: &StateRenderCtx<'_>) -> std::collections::BTreeSet<String> {
        let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        let mut composite_self_loops: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for e in ctx.edges {
            if state_is_hidden(ctx, e.start.as_str())
                || state_is_hidden(ctx, e.end.as_str())
                || state_is_hidden(ctx, e.id.as_str())
            {
                continue;
            }
            if e.start != e.end {
                continue;
            }
            let id = e.start.as_str();
            let Some(n) = ctx.nodes_by_id.get(id).copied() else {
                continue;
            };
            if n.is_group && n.shape != "noteGroup" {
                composite_self_loops.insert(id);
            }
        }

        let mut composite_externals: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for e in ctx.edges {
            if state_is_hidden(ctx, e.start.as_str())
                || state_is_hidden(ctx, e.end.as_str())
                || state_is_hidden(ctx, e.id.as_str())
            {
                continue;
            }
            let a = state_endpoint_context_raw(ctx, e.start.as_str());
            let b = state_endpoint_context_raw(ctx, e.end.as_str());
            let ca = state_context_chain_raw(ctx, a);
            let cb = state_context_chain_raw(ctx, b);

            for anc in &ca {
                let Some(id) = *anc else {
                    continue;
                };
                if cb.contains(anc) {
                    continue;
                }
                let Some(n) = ctx.nodes_by_id.get(id).copied() else {
                    continue;
                };
                if n.is_group && n.shape != "noteGroup" {
                    composite_externals.insert(id);
                }
            }
            for anc in &cb {
                let Some(id) = *anc else {
                    continue;
                };
                if ca.contains(anc) {
                    continue;
                }
                let Some(n) = ctx.nodes_by_id.get(id).copied() else {
                    continue;
                };
                if n.is_group && n.shape != "noteGroup" {
                    composite_externals.insert(id);
                }
            }
        }

        for e in ctx.edges {
            if state_is_hidden(ctx, e.start.as_str())
                || state_is_hidden(ctx, e.end.as_str())
                || state_is_hidden(ctx, e.id.as_str())
            {
                continue;
            }
            // Mermaid avoids creating a nested root for composites that have a self-loop edge on
            // the composite itself (e.g. `Active --> Active`).
            if composite_self_loops.contains(e.start.as_str()) && e.start == e.end {
                continue;
            }
            let Some(c) = state_edge_context_raw(ctx, e) else {
                continue;
            };
            if composite_externals.contains(c) {
                continue;
            }
            out.insert(c.to_string());
        }

        // Mermaid usually renders composite states in a nested root even when they don't contain
        // internal transitions, but it avoids doing so when the composite has a self-loop edge.
        for (child_id, parent_id) in &ctx.parent {
            if state_is_hidden(ctx, child_id) || state_is_hidden(ctx, parent_id) {
                continue;
            }
            if composite_self_loops.contains(parent_id) {
                continue;
            }
            if composite_externals.contains(parent_id) {
                continue;
            }
            let Some(pn) = ctx.nodes_by_id.get(parent_id).copied() else {
                continue;
            };
            if pn.is_group && pn.shape != "noteGroup" {
                out.insert((*parent_id).to_string());
            }
        }

        // If a nested graph is needed for a descendant composite state, Mermaid also nests
        // its composite state ancestors.
        let seeds: Vec<String> = out.iter().cloned().collect();
        for cid in seeds {
            let mut cur: Option<&str> = Some(cid.as_str());
            while let Some(id) = cur {
                let Some(pid) = ctx.parent.get(id).copied() else {
                    break;
                };
                let Some(pn) = ctx.nodes_by_id.get(pid).copied() else {
                    cur = Some(pid);
                    continue;
                };
                if pn.is_group && pn.shape != "noteGroup" {
                    if composite_self_loops.contains(pid) || composite_externals.contains(pid) {
                        cur = Some(pid);
                        continue;
                    }
                    out.insert(pid.to_string());
                }
                cur = Some(pid);
            }
        }

        out
    }

    if layout.uses_elk_adapter_dom {
        ctx.elk_line_hop_paths = prepare_state_line_hop_paths(&ctx, effective_config, options)?;
    } else {
        ctx.nested_roots = compute_state_nested_roots(&ctx);
    }

    drop(_g_build_ctx);

    let _g_render_svg = timing.section(&mut timings.render_svg);

    // Mermaid derives the final root viewport via `svg.getBBox()` (after rendering). We don't
    // have a browser DOM, so approximate that by parsing the SVG we just emitted and unioning
    // bboxes for the SVG elements we generate (`rect`/`path`/`circle`/`foreignObject`, etc).
    const TITLE_PLACEHOLDER_COMMENT: &str = "<!--__MERMAID_TITLE__-->";

    // Mermaid emits a single `<style>` element with diagram-scoped CSS.
    let css = state_css(diagram_id, model, effective_config);
    options.checkpoint_emit()?;

    let estimated_svg_bytes = 2048usize
        + css.len()
        + layout.nodes.len().saturating_mul(512)
        + layout.edges.len().saturating_mul(384)
        + layout.clusters.len().saturating_mul(256);
    let mut out = String::with_capacity(estimated_svg_bytes);
    let aria_labelledby = has_acc_title.then(|| format!("chart-title-{diagram_id}"));
    let aria_describedby = has_acc_descr.then(|| format!("chart-desc-{diagram_id}"));
    let root_context =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::State, diagram_id);
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "stateDiagram");
    root_chrome.class = Some("statediagram");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom = root_svg::RootDomProfile {
        aria_attr_order: root_svg::SvgRootAriaAttrOrder::LabelledbyThenDescribedby,
        trailing_newline: false,
        ..root_svg::RootDomProfile::default()
    };
    let root_document = root_context.begin_document(
        &mut out,
        root_svg::DeferredRootSpec::responsive(),
        root_chrome,
    )?;
    options.checkpoint_emit()?;

    if has_acc_title {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{}">{}"#,
            diagram_id,
            escape_xml_display(model.acc_title.as_deref().unwrap_or_default())
        );
        out.push_str("</title>");
    }
    if has_acc_descr {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{}">{}"#,
            diagram_id,
            escape_xml_display(model.acc_descr.as_deref().unwrap_or_default())
        );
        out.push_str("</desc>");
    }

    let _ = write!(&mut out, "<style>{}</style>", css);

    // Mermaid wraps diagram content (defs + root) in a single `<g>` element.
    out.push_str("<g>");
    state_markers(&mut out, diagram_id, effective_config);
    options.checkpoint_emit()?;

    // `svg.getBBox()` does not include `<style>` and typically excludes non-rendered `<defs>`
    // content from the rendered bbox. Scan only the rendered graph payload to reduce overhead
    // in our SVG bounds approximation.
    let bounds_scan_start = out.len();
    let mut detail = StateRenderDetails::default();
    render_state_root(
        &mut out,
        &ctx,
        None,
        (origin_x, origin_y),
        options,
        timing,
        &mut detail,
    )?;
    let bounds_scan_end = out.len();

    out.push_str("</g>");
    state_root_defs(&mut out, diagram_id, effective_config);
    options.checkpoint_emit()?;
    out.push_str(TITLE_PLACEHOLDER_COMMENT);
    out.push_str("</svg>\n");

    drop(_g_render_svg);

    let mut viewbox_svg_scan = std::time::Duration::ZERO;
    let _g_viewbox = timing.section(&mut timings.viewbox);
    let _g_scan = timing.section(&mut viewbox_svg_scan);
    let mut content_bounds = svg_emitted_bounds_from_svg(&out[bounds_scan_start..bounds_scan_end])
        .or_else(|| state_viewport_bounds_from_layout(layout))
        .unwrap_or(Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 100.0,
            max_y: 100.0,
        });
    drop(_g_scan);

    let mut title_svg = String::new();
    if let Some(title) = diagram_title.as_deref() {
        // Mermaid centers the title using the pre-title content bbox:
        // `x = bbox.x + bbox.width/2`, `y = -titleTopMargin`.
        let title_x = (content_bounds.min_x + content_bounds.max_x) / 2.0;
        let title_y = -title_top_margin;

        let mut title_style = crate::state::state_text_style(effective_config);
        title_style.font_size = 18.0;
        let (title_left, title_right) = measurer.measure_svg_title_bbox_x(title, &title_style);

        let (ascent, descent) = crate::text::svg_text_vertical_extents_px(
            measurer,
            title,
            &title_style,
            crate::text::svg_title_bbox_vertical_extents_px(&title_style),
        );

        content_bounds.min_x = content_bounds.min_x.min(title_x - title_left);
        content_bounds.max_x = content_bounds.max_x.max(title_x + title_right);
        content_bounds.min_y = content_bounds.min_y.min(title_y - ascent);
        content_bounds.max_y = content_bounds.max_y.max(title_y + descent);

        title_svg = String::with_capacity(title.len() + 128);
        let _ = write!(
            &mut title_svg,
            r#"<text text-anchor="middle" x="{}" y="{}" class="statediagramTitleText">{}</text>"#,
            fmt(title_x),
            fmt(title_y),
            escape_xml_display(title)
        );
    }

    let root_bounds = root_svg::DiagramBounds::from_extents(
        content_bounds.min_x,
        content_bounds.min_y,
        content_bounds.max_x,
        content_bounds.max_y,
        viewport_padding,
    );

    let root_document = root_context.finish_document(
        &mut out,
        root_document,
        root_svg::RootViewportSpec::responsive(root_bounds)
            .with_max_width(root_svg::RootMaxWidth::CssSixSignificant(root_bounds.width)),
    )?;

    drop(_g_viewbox);
    let _g_finalize = timing.section(&mut timings.finalize_svg);

    out = out.replacen(TITLE_PLACEHOLDER_COMMENT, title_svg.as_str(), 1);

    drop(_g_finalize);
    timings.total = total_timer
        .map(merman_core::runtime::OperationTimer::elapsed)
        .unwrap_or_default();
    if timing.is_enabled() {
        eprintln!(
            "[render-timing] diagram=stateDiagram total={:?} deserialize={:?} build_ctx={:?} render_svg={:?} viewbox={:?} viewbox_svg_scan={:?} finalize={:?} root_calls={} clusters={:?} edge_paths={:?} edge_labels={:?} leaf_nodes={:?} leaf_style_parse={:?} leaf_roughjs={:?} leaf_roughjs_calls={} leaf_roughjs_unique={} leaf_measure={:?} leaf_label_html={:?} leaf_emit={:?} nested_roots={:?} self_loop_placeholders={:?}",
            timings.total,
            timings.deserialize_model,
            timings.build_ctx,
            timings.render_svg,
            timings.viewbox,
            viewbox_svg_scan,
            timings.finalize_svg,
            detail.root_calls,
            detail.clusters,
            detail.edge_paths,
            detail.edge_labels,
            detail.leaf_nodes,
            detail.leaf_nodes_style_parse,
            detail.leaf_nodes_roughjs,
            detail.leaf_roughjs_calls,
            detail.leaf_roughjs_unique.len(),
            detail.leaf_nodes_measure,
            detail.leaf_nodes_label_html,
            detail.leaf_nodes_emit,
            detail.nested_roots,
            detail.self_loop_placeholders,
        );
    }
    root_document.complete(out)
}

fn prepare_state_line_hop_paths(
    ctx: &StateRenderCtx<'_>,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<FxHashMap<String, String>> {
    use crate::svg::parity::line_hops::{LineHopEdge, elk_line_hop_paths};

    if effective_config
        .pointer("/elk/lineHops")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        return Ok(FxHashMap::default());
    }
    options.work_meter().charge(ctx.edges.len())?;
    let point_count = ctx
        .edges
        .iter()
        .filter_map(|edge| ctx.layout_edges_by_id.get(edge.id.as_str()))
        .map(|edge| {
            ctx.elk_edge_paths
                .get(&edge.id)
                .map_or(edge.points.len(), Vec::len)
        })
        .fold(0usize, usize::saturating_add);
    options.work_meter().charge(point_count.saturating_mul(2))?;

    struct OwnedEdge<'a> {
        id: &'a str,
        points: Vec<crate::model::LayoutPoint>,
        curve: Option<&'static str>,
        arrow_type_end: Option<&'a str>,
    }

    let mut owned_edges = Vec::new();
    for edge in ctx.edges {
        if state_is_hidden(ctx, edge.start.as_str())
            || state_is_hidden(ctx, edge.end.as_str())
            || state_is_hidden(ctx, edge.id.as_str())
        {
            continue;
        }
        let Some(layout_edge) = ctx.layout_edges_by_id.get(edge.id.as_str()).copied() else {
            continue;
        };
        let geometry = state_edge_prepare_geometry(
            ctx,
            layout_edge,
            Some(edge.arrow_type_end.as_str()),
            0.0,
            0.0,
        );
        let curve = if layout_edge.points.is_empty() {
            "linear"
        } else {
            "rounded"
        };
        owned_edges.push(OwnedEdge {
            id: edge.id.as_str(),
            points: geometry.data_points,
            curve: Some(curve),
            arrow_type_end: Some(edge.arrow_type_end.as_str()),
        });
    }

    let edges: Vec<_> = owned_edges
        .iter()
        .map(|edge| LineHopEdge {
            id: edge.id,
            points: &edge.points,
            curve: edge.curve,
            arrow_type_start: None,
            arrow_type_end: edge.arrow_type_end,
        })
        .collect();
    Ok(
        elk_line_hop_paths(effective_config, &edges, options.work_meter())?
            .into_iter()
            .map(|(id, path)| (id.to_owned(), path))
            .collect(),
    )
}

fn render_state_root(
    out: &mut String,
    ctx: &StateRenderCtx<'_>,
    root: Option<&str>,
    parent_origin: (f64, f64),
    options: &SvgExecution<'_>,
    timing: super::timing::RenderTiming,
    details: &mut StateRenderDetails,
) -> Result<()> {
    details.root_calls += 1;
    options.checkpoint_emit()?;
    let (parent_origin_x, parent_origin_y) = parent_origin;

    // Mermaid's dagre-wrapper uses a fixed graph margin (`marginx/marginy=8`). For nested state
    // roots (extracted cluster graphs), Mermaid keeps the root cluster frame at x/y=8 in the
    // nested coordinate space and compensates via the root group's `translate(...)`.
    //
    // If we anchor the nested origin at the cluster's top-left, the emitted cluster rect starts at
    // (0,0) and the root group's transform drifts from upstream DOM. Shift the origin by the fixed
    // margin so nested roots start at (8,8), matching Mermaid's SVG structure more closely.
    const GRAPH_MARGIN_PX: f64 = 8.0;

    let (origin_x, origin_y, transform_attr) = if let Some(root_id) = root {
        if let Some(c) = ctx.layout_clusters_by_id.get(root_id).copied() {
            let left = c.x - c.width / 2.0;
            let top = c.y - c.height / 2.0;
            let origin_x = left - GRAPH_MARGIN_PX;
            let origin_y = top - GRAPH_MARGIN_PX;
            let tx = origin_x - parent_origin_x;
            let ty = origin_y - parent_origin_y;
            (
                origin_x,
                origin_y,
                format!(r#" transform="translate({}, {})""#, fmt(tx), fmt(ty)),
            )
        } else {
            (
                parent_origin_x,
                parent_origin_y,
                r#" transform="translate(0, 0)""#.to_string(),
            )
        }
    } else {
        (parent_origin_x, parent_origin_y, String::new())
    };

    let _ = write!(out, r#"<g class="root"{}>"#, transform_attr);

    // clusters
    let _g_clusters = detail_guard(timing, &mut details.clusters);
    out.push_str(r#"<g class="clusters">"#);
    if let Some(root_id) = root {
        render_state_cluster(out, ctx, root_id, origin_x, origin_y);
        options.checkpoint_emit()?;
    }

    for &cluster_id in &ctx.node_order {
        if root == Some(cluster_id) {
            continue;
        }
        if !ctx.layout_clusters_by_id.contains_key(cluster_id) {
            continue;
        }
        if state_is_hidden(ctx, cluster_id) {
            continue;
        }
        if ctx.nested_roots.contains(cluster_id) {
            continue;
        }
        let Some(node) = ctx.nodes_by_id.get(cluster_id).copied() else {
            continue;
        };
        if !node.is_group || node.shape == "noteGroup" {
            continue;
        }
        if state_insertion_context(ctx, cluster_id) != root {
            continue;
        }
        render_state_cluster(out, ctx, cluster_id, origin_x, origin_y);
        options.checkpoint_emit()?;
    }

    for &cluster_id in &ctx.node_order {
        if !ctx.layout_clusters_by_id.contains_key(cluster_id) {
            continue;
        }
        let Some(cluster) = ctx.layout_clusters_by_id.get(cluster_id).copied() else {
            continue;
        };
        if state_is_hidden(ctx, cluster_id) {
            continue;
        }
        let Some(node) = ctx.nodes_by_id.get(cluster_id).copied() else {
            continue;
        };
        if node.shape != "noteGroup" {
            continue;
        }
        let note_owner = state_note_owner_id(cluster_id);
        if ctx.hidden_prefixes.iter().any(|p| p == note_owner) {
            continue;
        }
        let has_position = ctx
            .states
            .get(note_owner)
            .and_then(|s| s.note.as_ref())
            .and_then(|n| n.position.as_ref())
            .is_some();
        if !has_position {
            continue;
        }

        let target_root = state_insertion_context(ctx, note_owner);
        if target_root != root {
            continue;
        }

        let left = cluster.x - cluster.width / 2.0;
        let top = cluster.y - cluster.height / 2.0;
        let x = left - origin_x;
        let y = top - origin_y;
        let dom_id = state_node_scoped_dom_id(ctx, cluster_id);
        let _ = write!(
            out,
            r#"<g id="{}" class="note-cluster"><rect x="{}" y="{}" width="{}" height="{}" fill="none"/></g>"#,
            dom_id,
            fmt_display(x),
            fmt_display(y),
            fmt_display(cluster.width.max(1.0)),
            fmt_display(cluster.height.max(1.0))
        );
        options.checkpoint_emit()?;
    }
    out.push_str("</g>");
    drop(_g_clusters);

    // edge paths
    let _g_edge_paths = detail_guard(timing, &mut details.edge_paths);
    out.push_str(if ctx.uses_elk_adapter_dom {
        r#"<g class="edges edgePaths">"#
    } else {
        r#"<g class="edgePaths">"#
    });
    if ctx.include_edges {
        for (edge_index, edge) in ctx.edges.iter().enumerate() {
            if state_is_hidden(ctx, edge.start.as_str())
                || state_is_hidden(ctx, edge.end.as_str())
                || state_is_hidden(ctx, edge.id.as_str())
            {
                continue;
            }
            if state_edge_context(ctx, edge) != root {
                continue;
            }
            if state_is_shadowed_self_loop_edge(ctx, edge_index, edge, root) {
                continue;
            }
            if let Some(layout_edge) = ctx.layout_edges_by_id.get(edge.id.as_str()) {
                let original_path_work = ctx
                    .elk_edge_paths
                    .get(&layout_edge.id)
                    .map_or(layout_edge.points.len(), Vec::len);
                options.work_meter().charge(original_path_work)?;
            }
            render_state_edge_path(out, ctx, edge, origin_x, origin_y, options.work_meter())?;
            options.checkpoint_emit()?;
        }
    }
    out.push_str("</g>");
    drop(_g_edge_paths);

    // edge labels
    let _g_edge_labels = detail_guard(timing, &mut details.edge_labels);
    out.push_str(r#"<g class="edgeLabels">"#);
    if ctx.include_edges {
        for (edge_index, edge) in ctx.edges.iter().enumerate() {
            if state_is_hidden(ctx, edge.start.as_str())
                || state_is_hidden(ctx, edge.end.as_str())
                || state_is_hidden(ctx, edge.id.as_str())
            {
                continue;
            }
            if state_edge_context(ctx, edge) != root {
                continue;
            }
            if state_is_shadowed_self_loop_edge(ctx, edge_index, edge, root) {
                continue;
            }
            if let Some(layout_edge) = ctx.layout_edges_by_id.get(edge.id.as_str()) {
                let original_path_work = ctx
                    .elk_edge_paths
                    .get(&layout_edge.id)
                    .map_or(layout_edge.points.len(), Vec::len);
                options.work_meter().charge(original_path_work)?;
            }
            render_state_edge_label(out, ctx, edge, origin_x, origin_y);
            options.checkpoint_emit()?;
        }
    }
    out.push_str("</g>");
    drop(_g_edge_labels);

    // nodes (leaf nodes + nested roots)
    out.push_str(r#"<g class="nodes">"#);
    let mut nested: Vec<&str> = Vec::new();
    for &id in &ctx.node_order {
        let Some(n) = ctx.nodes_by_id.get(id).copied() else {
            continue;
        };
        if state_is_hidden(ctx, id) {
            continue;
        }
        if n.is_group
            && n.shape != "noteGroup"
            && ctx.nested_roots.contains(id)
            && state_insertion_context(ctx, id) == root
        {
            nested.push(id);
        }
    }

    if ctx.include_nodes {
        let leaf_start = timing.start();
        for &id in &ctx.node_order {
            let Some(n) = ctx.layout_nodes_by_id.get(id).copied() else {
                continue;
            };
            if state_is_hidden(ctx, id) {
                continue;
            }
            if n.is_cluster {
                continue;
            }
            if state_leaf_context(ctx, id) != root {
                continue;
            }
            render_state_node_svg(out, ctx, id, origin_x, origin_y, timing, details);
            options.checkpoint_emit()?;
        }
        if let Some(s) = leaf_start {
            details.leaf_nodes += s.elapsed();
        }
    }

    for child_root in nested {
        let nested_start = timing.start();
        render_state_root(
            out,
            ctx,
            Some(child_root),
            (origin_x, origin_y),
            options,
            timing,
            details,
        )?;
        options.checkpoint_emit()?;
        if let Some(s) = nested_start {
            details.nested_roots += s.elapsed();
        }
    }

    // Dagre adds dummy edgeLabel nodes for self loops; ELK retains the original edges.
    if ctx.include_edges && !ctx.uses_elk_adapter_dom {
        let _g_placeholders = detail_guard(timing, &mut details.self_loop_placeholders);
        for (edge_index, edge) in ctx.edges.iter().enumerate() {
            if state_is_hidden(ctx, edge.start.as_str())
                || state_is_hidden(ctx, edge.end.as_str())
                || state_is_hidden(ctx, edge.id.as_str())
            {
                continue;
            }
            if edge.start != edge.end {
                continue;
            }
            if state_edge_context(ctx, edge) != root {
                continue;
            }
            if state_is_shadowed_self_loop_edge(ctx, edge_index, edge, root) {
                continue;
            }

            let start = edge.start.as_str();
            let id1 = format!("{start}---{start}---1");
            let id2 = format!("{start}---{start}---2");

            for id in [id1, id2] {
                let (cx, cy) = ctx
                    .layout_nodes_by_id
                    .get(id.as_str())
                    .map(|n| {
                        let x = (n.x - n.width / 2.0) - origin_x;
                        let y = (n.y - n.height / 2.0) - origin_y;
                        (x, y)
                    })
                    .unwrap_or((0.0, 0.0));
                if ctx.html_labels {
                    let _ = write!(
                        out,
                        r#"<g class="label edgeLabel" id="{}" transform="translate({}, {})"><rect width="0.1" height="0.1"/><g class="label" style="" transform="translate(0, 0)"><rect/><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 10px; text-align: center;"><span class="nodeLabel"></span></div></foreignObject></g></g>"#,
                        escape_xml_display(&id),
                        fmt_display(cx),
                        fmt_display(cy),
                    );
                } else {
                    let _ = write!(
                        out,
                        r#"<g class="label edgeLabel" id="{}" transform="translate({}, {})"><rect width="0.1" height="0.1"/><g class="label" style="" transform="translate(0, 0)"><rect/></g></g>"#,
                        escape_xml_display(&id),
                        fmt_display(cx),
                        fmt_display(cy),
                    );
                }
                options.checkpoint_emit()?;
            }
        }
        drop(_g_placeholders);
    }

    out.push_str("</g>");
    out.push_str("</g>");
    Ok(())
}

fn render_state_cluster(
    out: &mut String,
    ctx: &StateRenderCtx<'_>,
    cluster_id: &str,
    origin_x: f64,
    origin_y: f64,
) {
    let Some(cluster) = ctx.layout_clusters_by_id.get(cluster_id).copied() else {
        return;
    };

    let data_look = state_data_look(ctx);

    let shape = ctx
        .nodes_by_id
        .get(cluster_id)
        .copied()
        .map(|n| n.shape.as_str())
        .unwrap_or("");

    let class = ctx
        .nodes_by_id
        .get(cluster_id)
        .copied()
        .map(|n| n.css_classes.trim())
        .filter(|c| !c.is_empty())
        .unwrap_or("statediagram-state statediagram-cluster");

    let left = cluster.x - cluster.width / 2.0;
    let top = cluster.y - cluster.height / 2.0;
    let x = left - origin_x;
    let y = top - origin_y;
    let dom_id = state_node_scoped_dom_id(ctx, cluster_id);

    let _ = write!(
        out,
        r#"<g class="{}" id="{}""#,
        escape_attr(class),
        dom_id.attr()
    );
    if shape != "divider" {
        let _ = write!(out, r#" data-id="{}""#, escape_attr(cluster_id));
    }
    let _ = write!(out, r#" data-look="{}""#, escape_attr(data_look));
    if ctx.palette_size > 0
        && let Some(color_index) = ctx
            .nodes_by_id
            .get(cluster_id)
            .and_then(|node| node.color_index)
    {
        let _ = write!(
            out,
            r#" data-color-id="color-{}""#,
            color_index % ctx.palette_size
        );
    }
    out.push('>');

    if shape == "divider" {
        let _ = write!(
            out,
            r#"<g><rect class="divider" x="{}" y="{}" width="{}" height="{}" data-look="{}"/></g></g>"#,
            fmt(x),
            fmt(y),
            fmt(cluster.width.max(1.0)),
            fmt(cluster.height.max(1.0)),
            escape_attr(data_look),
        );
        return;
    }

    let title = ctx
        .nodes_by_id
        .get(cluster_id)
        .copied()
        .map(state_node_label_text)
        .unwrap_or_else(|| cluster_id.to_string());

    // roundedWithTitle in Mermaid's clusters.js uses the painted label bbox in both
    // rendering modes. ELK's reduced title-strip reservation is not the paint height.
    let title_height = cluster.title_label.height;
    let inner_y = y + title_height + 2.0;
    let inner_height = cluster.height - title_height - 6.0;

    if ctx.html_labels {
        let _ = write!(
            out,
            r#"<g><rect class="outer" x="{}" y="{}" width="{}" height="{}" data-look="{}"/></g><g class="cluster-label" transform="translate({}, {})"><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5;">{}</div></foreignObject></g><rect class="inner" x="{}" y="{}" width="{}" height="{}"/></g>"#,
            fmt(x),
            fmt(y),
            fmt(cluster.width.max(1.0)),
            fmt(cluster.height.max(1.0)),
            escape_attr(data_look),
            fmt(x + (cluster.width.max(1.0) - cluster.title_label.width.max(0.0)) / 2.0),
            fmt(y + 1.0),
            fmt(cluster.title_label.width.max(0.0)),
            fmt(title_height),
            state_node_label_plain_html(&title),
            fmt(x),
            fmt(inner_y),
            fmt(cluster.width.max(1.0)),
            fmt(inner_height)
        );
    } else {
        let title_dom = state_svg_text_label(&title, false, None);
        let _ = write!(
            out,
            r#"<g><rect class="outer" x="{}" y="{}" width="{}" height="{}" data-look="{}"/></g><g class="cluster-label" transform="translate({}, {})">{}</g><rect class="inner" x="{}" y="{}" width="{}" height="{}"/></g>"#,
            fmt(x),
            fmt(y),
            fmt(cluster.width.max(1.0)),
            fmt(cluster.height.max(1.0)),
            escape_attr(data_look),
            fmt(x + (cluster.width.max(1.0) - cluster.title_label.width.max(0.0)) / 2.0),
            fmt(y - 2.0),
            title_dom,
            fmt(x),
            fmt(inner_y),
            fmt(cluster.width.max(1.0)),
            fmt(inner_height)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_elk_neo_note_crossing_rewrites_the_complete_original_mask() {
        let parsed = merman_core::Engine::new()
            .parse_diagram_for_render_model_sync(
                "stateDiagram-v2\nA --> B: move\nC --> D\nnote right of A: annotation\n",
                merman_core::ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let merman_core::RenderSemanticModel::State(model) = parsed.model() else {
            panic!("expected State render model");
        };
        let note = model
            .edges
            .iter()
            .find(|edge| edge.classes.contains("note-edge"))
            .unwrap();
        // Supply controlled provider routes to the real State paint path. The note crosses an
        // unrelated transition, while the labelled transition is deliberately away from both.
        let edges: Vec<crate::model::LayoutEdge> = model
            .edges
            .iter()
            .map(|edge| {
                let points = if edge.id == note.id {
                    vec![
                        crate::model::LayoutPoint { x: 0.0, y: 50.0 },
                        crate::model::LayoutPoint { x: 100.0, y: 50.0 },
                    ]
                } else if edge.start == "C" {
                    vec![
                        crate::model::LayoutPoint { x: 50.0, y: 0.0 },
                        crate::model::LayoutPoint { x: 50.0, y: 100.0 },
                    ]
                } else {
                    vec![
                        crate::model::LayoutPoint { x: 0.0, y: 200.0 },
                        crate::model::LayoutPoint { x: 100.0, y: 200.0 },
                    ]
                };
                crate::model::LayoutEdge {
                    id: edge.id.clone(),
                    from: edge.start.clone(),
                    to: edge.end.clone(),
                    from_cluster: None,
                    to_cluster: None,
                    points,
                    label: (!edge.label.is_empty()).then_some(crate::model::LayoutLabel {
                        x: 50.0,
                        y: 200.0,
                        width: 32.0,
                        height: 24.0,
                    }),
                    start_label_left: None,
                    start_label_right: None,
                    end_label_left: None,
                    end_label_right: None,
                    start_marker: None,
                    end_marker: None,
                    stroke_dasharray: None,
                }
            })
            .collect();
        let layout = StateDiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            edges,
            bounds: None,
            uses_elk_adapter_dom: true,
            elk_edge_paths: Default::default(),
        };
        let render = |hops: serde_json::Value| {
            let config =
                serde_json::json!({"look":"neo", "htmlLabels":false, "elk":{"lineHops":hops}});
            crate::svg::with_test_svg_execution(&SvgRenderOptions::default(), |execution| {
                render_state_diagram_svg_model(
                    &layout,
                    model,
                    &config,
                    None,
                    &crate::text::DeterministicTextMeasurer::default(),
                    execution,
                )
                .unwrap()
                .into_string_for(crate::family::RenderFamilyKind::State)
                .unwrap()
            })
        };
        let baseline = render(serde_json::json!(false));
        let baseline_doc = roxmltree::Document::parse(&baseline).unwrap();
        for hops in [serde_json::json!(true), serde_json::json!("gap")] {
            let svg = render(hops);
            let doc = roxmltree::Document::parse(&svg).unwrap();
            for expected in baseline_doc
                .descendants()
                .filter(|node| node.attribute("data-edge") == Some("true"))
            {
                let actual = doc
                    .descendants()
                    .find(|node| {
                        node.attribute("data-id") == expected.attribute("data-id")
                            && node.has_tag_name("path")
                    })
                    .unwrap();
                assert_eq!(
                    actual.attribute("data-points"),
                    expected.attribute("data-points")
                );
                assert_eq!(
                    actual.attribute("marker-end"),
                    expected.attribute("marker-end")
                );
                if expected.attribute("data-id") != Some(note.id.as_str()) {
                    assert_eq!(actual.attribute("d"), expected.attribute("d"));
                    assert_eq!(actual.attribute("style"), expected.attribute("style"));
                    continue;
                }
                let d = actual.attribute("d").unwrap();
                assert_ne!(Some(d), expected.attribute("d"));
                let style = actual.attribute("style").unwrap();
                let mask = style
                    .split(';')
                    .find_map(|declaration| declaration.trim().strip_prefix("stroke-dasharray:"))
                    .unwrap();
                let mask: Vec<f64> = mask
                    .split_whitespace()
                    .map(|number| number.parse().unwrap())
                    .collect();
                assert_eq!(mask.len(), 4, "{style}");
                assert_eq!((mask[0], mask[1], mask[3]), (0.0, 0.0, 2.0));
                let length = crate::svg::parity::svg_path_length_from_d(d).unwrap();
                assert!(
                    (mask[2] + 2.0 - length).abs() < 1e-6,
                    "{style}: length={length}"
                );
            }
            let label = |document: &roxmltree::Document<'_>| {
                let group = document
                    .descendants()
                    .find(|node| node.attribute("class") == Some("edgeLabels"))
                    .unwrap();
                group
                    .descendants()
                    .filter_map(|node| node.attribute("transform"))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            };
            assert_eq!(label(&doc), label(&baseline_doc));
        }
    }
}
