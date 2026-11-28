use crate::config::{config_f64_css_px, config_string};
use crate::graph_label::{FlowchartLabelMetricsRequest, flowchart_label_metrics_for_layout};
use crate::layout_work::OperationLayoutWorkControl;
use crate::math::MathRenderer;
use crate::model::{Bounds, LayoutEdge, LayoutNode, LayoutPoint, MindmapDiagramLayout};
use crate::text::WrapMode;
use crate::text::{TextMeasurer, TextStyle};
use crate::{Error, Result};
use merman_core::MermaidConfig;
use serde_json::Value;
use std::sync::Arc;

mod registered;
mod tidy_tree;

pub(crate) fn mindmap_max_node_width_px(effective_config: &Value) -> f64 {
    config_f64_css_px(effective_config, &["mindmap", "maxNodeWidth"])
        .unwrap_or(200.0)
        .max(1.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MindmapLayoutBackend {
    Cose,
    TidyTree,
    Swimlane,
    Dagre,
    #[cfg(feature = "layout-elk")]
    Elk,
}

pub(crate) fn layout_backend(config: &Value) -> MindmapLayoutBackend {
    let requested = config.get("layout").and_then(Value::as_str).unwrap_or("");
    match requested {
        "tidy-tree" => MindmapLayoutBackend::TidyTree,
        "dagre" => MindmapLayoutBackend::Dagre,
        "swimlane" => MindmapLayoutBackend::Swimlane,
        #[cfg(feature = "layout-elk")]
        _ if crate::layout_backend::resolve_graph_layout(config).backend
            == crate::layout_backend::GraphLayoutBackend::Elk =>
        {
            MindmapLayoutBackend::Elk
        }
        _ if cfg!(feature = "layout-cytoscape") => MindmapLayoutBackend::Cose,
        _ => MindmapLayoutBackend::Dagre,
    }
}

pub(crate) fn required_layout_capability(
    config: &MermaidConfig,
) -> Option<crate::RenderCapability> {
    match layout_backend(config.as_value()) {
        MindmapLayoutBackend::Cose => Some(crate::RenderCapability::LayoutCytoscape),
        #[cfg(feature = "layout-elk")]
        MindmapLayoutBackend::Elk => Some(crate::RenderCapability::LayoutElk),
        _ => None,
    }
}

/// The Dagre renderer clips circular nodes along the endpoint-to-center ray.
pub(crate) fn dagre_shape_points(
    mut points: Vec<LayoutPoint>,
    start: &LayoutNode,
    end: &LayoutNode,
    start_circle: bool,
    end_circle: bool,
) -> Vec<LayoutPoint> {
    if points.len() < 2 {
        return points;
    }
    let intersect = |node: &LayoutNode, point: &LayoutPoint| {
        let dx = point.x - node.x;
        let dy = point.y - node.y;
        let length = dx.hypot(dy);
        if length == 0.0 {
            return point.clone();
        }
        let radius = node.width / 2.0;
        LayoutPoint {
            x: node.x + radius * dx / length,
            y: node.y + radius * dy / length,
        }
    };
    if start_circle {
        points[0] = intersect(start, &points[1]);
    }
    if end_circle {
        let last = points.len() - 1;
        points[last] = intersect(end, &points[last - 1]);
    }
    points
}

type MindmapModel = merman_core::diagrams::mindmap::MindmapDiagramRenderModel;
type MindmapNodeModel = merman_core::diagrams::mindmap::MindmapDiagramRenderNode;
type MindmapEdgeModel = merman_core::diagrams::mindmap::MindmapDiagramRenderEdge;

fn mindmap_text_style(effective_config: &Value) -> TextStyle {
    // Mermaid mindmap labels are rendered via HTML `<foreignObject>` and inherit the global font.
    let font_family = config_string(effective_config, &["fontFamily"])
        .or_else(|| config_string(effective_config, &["themeVariables", "fontFamily"]))
        .or_else(|| Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()));
    // Mermaid mindmap uses HTML `<foreignObject>` labels. Mermaid CLI baselines show that the
    // HTML label contents do not reliably inherit SVG-root `font-size` rules; measurement matches
    // a 16px default even when users override `themeVariables.fontSize`.
    let font_size = 16.0;
    TextStyle {
        font_family,
        font_size,
        font_weight: None,
        font_style: None,
    }
}

pub(crate) fn mindmap_label_text_for_layout(text: &str) -> &str {
    if !text.contains('\n') && !text.contains('\r') {
        return text;
    }

    let mut normalized = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if normalized.is_some() {
            return text;
        }
        normalized = Some(line);
    }

    normalized.unwrap_or(text)
}

fn mindmap_label_bbox_px(
    text: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_node_width_px: f64,
) -> (f64, f64) {
    let decoded = merman_core::entities::decode_mermaid_entities_to_unicode(text);
    let text = mindmap_label_text_for_layout(decoded.as_ref());

    // Mermaid mindmap labels are rendered via HTML `<foreignObject>` and respect
    // `mindmap.maxNodeWidth` (default 200px). When the raw label is wider than that, Mermaid
    // switches the label container to a fixed 200px width and allows HTML-like wrapping (e.g.
    // `white-space: break-spaces` in upstream SVG baselines).
    //
    // Mirror that by measuring with an explicit max width in HTML-like mode.
    let max_node_width_px = max_node_width_px.max(1.0);

    // Upstream `mindmapDb.flattenNodes()` assigns `labelType: 'markdown'` to every node. Measure
    // the same post-Markdown HTML fragment that `createText()` inserts into the foreignObject,
    // including raw inline HTML. SVG emission applies the configured sanitizer after rendering.
    let html = crate::text::mermaid_markdown_to_html_label_fragment(text, true);
    let wrapped = crate::text::measure_html_with_inline_styles(
        measurer,
        &html,
        style,
        Some(max_node_width_px),
        WrapMode::HtmlLike,
    );

    // The HTML-like measurement path already includes min-content width for unbreakable tokens.
    // Do not re-expand normal wrapping prose back to its unwrapped paragraph width, or Mindmap
    // layout/root bounds drift far wider than Mermaid's fixed-width wrapping container.
    (wrapped.width.max(0.0), wrapped.height.max(0.0))
}

fn mindmap_label_bbox_px_with_math(
    text: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_node_width_px: f64,
    config: &MermaidConfig,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Result<(f64, f64)> {
    if !crate::math::contains_delimited_math(text) {
        return Ok(mindmap_label_bbox_px(
            text,
            measurer,
            style,
            max_node_width_px,
        ));
    }

    let math_renderer = math_renderer.ok_or_else(|| Error::MissingCapability {
        capability: crate::RenderCapability::Math,
        diagram_type: "mindmap".to_string(),
    })?;
    let metrics = flowchart_label_metrics_for_layout(FlowchartLabelMetricsRequest {
        measurer,
        raw_label: mindmap_label_text_for_layout(text),
        label_type: "markdown",
        style,
        max_width_px: Some(max_node_width_px.max(1.0)),
        wrap_mode: WrapMode::HtmlLike,
        config,
        math_renderer: Some(math_renderer),
    });
    Ok((metrics.width.max(0.0), metrics.height.max(0.0)))
}

fn mindmap_node_dimensions_from_label_bbox(
    node: &MindmapNodeModel,
    bbox_w: f64,
    bbox_h: f64,
) -> (f64, f64, f64, f64) {
    // Mermaid mindmap applies some shape-specific padding overrides during rendering (after
    // `mindmapDb.getData()`), notably for rounded nodes.
    //
    // Our semantic snapshots keep the DB padding (e.g. doubled padding for `(text)`), but layout
    // should follow the render-time effective padding so layout golden snapshots remain stable.
    let padding = match node.shape.as_str() {
        "rounded" => 15.0,
        _ => node.padding.max(0.0),
    };
    let half_padding = padding / 2.0;

    // Align with Mermaid shape sizing rules for mindmap nodes (via `labelHelper(...)` + shape
    // handlers in `rendering-elements/shapes/*`).
    let (w, h) = match node.shape.as_str() {
        // `defaultMindmapNode.ts`: w = bbox.width + 8 * halfPadding; h = bbox.height + 2 * halfPadding
        "" | "defaultMindmapNode" => (bbox_w + 8.0 * half_padding, bbox_h + 2.0 * half_padding),
        // Mindmap node shapes use the standard `labelHelper(...)` label bbox, but mindmap DB
        // adjusts `node.padding` depending on the delimiter type (e.g. `[` / `(` / `{{`).
        //
        // Upstream Mermaid@11.12.2 mindmap SVG baselines show:
        // - rect (`[text]`): w = bbox.width + 2*padding, h = bbox.height + padding
        // - rounded (`(text)`): w = bbox.width + 2*padding, h = bbox.height + 2*padding
        "rect" => (bbox_w + 2.0 * padding, bbox_h + padding),
        "rounded" => (bbox_w + 2.0 * padding, bbox_h + 2.0 * padding),
        // `mindmapCircle.ts` -> `circle.ts`: radius = bbox.width/2 + padding (mindmap passes full padding)
        "mindmapCircle" => {
            let d = bbox_w + 2.0 * padding;
            (d, d)
        }
        // `cloud.ts` first draws a path from w = bbox.width + 2*halfPadding and
        // h = bbox.height + 2*halfPadding, then upstream cose-bilkent lays out the node
        // using the inserted SVG node's rendered path bbox.
        "cloud" => {
            let shape_w = bbox_w + 2.0 * half_padding;
            let shape_h = bbox_h + 2.0 * half_padding;
            crate::svg::mindmap_cloud_rendered_bbox_size_px(shape_w, shape_h)
                .unwrap_or((shape_w, shape_h))
        }
        // `bang.ts`:
        // - w = bbox.width + 10*halfPadding; h = bbox.height + 8*halfPadding
        // - minWidth = bbox.width + 20; minHeight = bbox.height + 20
        // - effectiveWidth/Height = max(w/h, minWidth/Height)
        "bang" => {
            let w = bbox_w + 10.0 * half_padding;
            let h = bbox_h + 8.0 * half_padding;
            let min_w = bbox_w + 20.0;
            let min_h = bbox_h + 20.0;
            (w.max(min_w), h.max(min_h))
        }
        // `hexagon.ts` (classic): h = bbox.height + padding; m = h/4;
        // w = bbox.width + 2*m + padding.
        "hexagon" => {
            let h = bbox_h + padding;
            let m = h / 4.0;
            (bbox_w + 2.0 * m + padding, h)
        }
        _ => (bbox_w + 8.0 * half_padding, bbox_h + 2.0 * half_padding),
    };

    (w, h, bbox_w, bbox_h)
}

#[cfg(all(test, feature = "diagram-mindmap"))]
fn mindmap_node_dimensions_px(
    node: &MindmapNodeModel,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_node_width_px: f64,
) -> (f64, f64, f64, f64) {
    let (bbox_w, bbox_h) = mindmap_label_bbox_px(&node.label, measurer, style, max_node_width_px);
    mindmap_node_dimensions_from_label_bbox(node, bbox_w, bbox_h)
}

fn compute_bounds(nodes: &[LayoutNode], edges: &[LayoutEdge]) -> Option<Bounds> {
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for n in nodes {
        let x0 = n.x - n.width / 2.0;
        let y0 = n.y - n.height / 2.0;
        let x1 = n.x + n.width / 2.0;
        let y1 = n.y + n.height / 2.0;
        pts.push((x0, y0));
        pts.push((x1, y1));
    }
    for e in edges {
        for p in &e.points {
            pts.push((p.x, p.y));
        }
    }
    Bounds::from_points(pts)
}

fn shift_nodes_to_positive_bounds(nodes: &mut [LayoutNode], content_min: f64) {
    if nodes.is_empty() {
        return;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for n in nodes.iter() {
        min_x = min_x.min(n.x - n.width / 2.0);
        min_y = min_y.min(n.y - n.height / 2.0);
    }
    if !(min_x.is_finite() && min_y.is_finite()) {
        return;
    }
    let dx = content_min - min_x;
    let dy = content_min - min_y;
    for n in nodes.iter_mut() {
        n.x += dx;
        n.y += dy;
    }
}

fn mindmap_layout_adapter_work(
    model: &MindmapModel,
    use_tidy_tree: bool,
    work_control: &OperationLayoutWorkControl,
) -> Result<usize> {
    let node_work = work_control.checked_mul(model.nodes.len(), 12)?;
    let edge_work = work_control.checked_mul(model.edges.len(), 8)?;
    let backend_work = if use_tidy_tree {
        work_control.checked_add(
            work_control.checked_mul(model.nodes.len(), 16)?,
            work_control.checked_mul(model.edges.len(), 12)?,
        )?
    } else {
        work_control.checked_add(
            work_control.checked_mul(model.nodes.len(), 6)?,
            work_control.checked_mul(model.edges.len(), 4)?,
        )?
    };
    work_control.checked_add(
        node_work,
        work_control.checked_add(edge_work, backend_work)?,
    )
}

pub(crate) fn layout_mindmap_diagram_typed_with_work_meter(
    model: &MindmapModel,
    config: &MermaidConfig,
    text_measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    work_meter: Arc<crate::resources::OperationWorkMeter>,
    #[cfg(feature = "layout-elk")] operation_seed: merman_layout_elk::ElkOperationSeed,
) -> Result<MindmapDiagramLayout> {
    let mut work_control = OperationLayoutWorkControl::new(work_meter.clone());
    let use_tidy_tree = layout_backend(config.as_value()) == MindmapLayoutBackend::TidyTree;
    let adapter_work = mindmap_layout_adapter_work(model, use_tidy_tree, &work_control)?;
    work_control.charge_adapter(adapter_work)?;
    layout_mindmap_diagram_model(
        model,
        config,
        text_measurer,
        math_renderer,
        &mut work_control,
        work_meter,
        #[cfg(feature = "layout-elk")]
        operation_seed,
    )
}

fn layout_mindmap_diagram_model(
    model: &MindmapModel,
    config: &MermaidConfig,
    text_measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    _work_control: &mut OperationLayoutWorkControl,
    work_meter: Arc<crate::resources::OperationWorkMeter>,
    #[cfg(feature = "layout-elk")] operation_seed: merman_layout_elk::ElkOperationSeed,
) -> Result<MindmapDiagramLayout> {
    let effective_config = config.as_value();
    let text_style = mindmap_text_style(effective_config);
    let max_node_width_px = mindmap_max_node_width_px(effective_config);

    let mut nodes: Vec<LayoutNode> = Vec::with_capacity(model.nodes.len());
    // Mermaid's Mindmap DB emits nodes in parser preorder, and Cytoscape consumes that order
    // directly. The typed model already preserves it; re-sorting numeric-looking ids here added
    // O(N log N) work and changed the geometry of manually constructed source-order models.
    for n in &model.nodes {
        let (label_width, label_height) = mindmap_label_bbox_px_with_math(
            &n.label,
            text_measurer,
            &text_style,
            max_node_width_px,
            config,
            math_renderer,
        )?;
        let (width, height, label_width, label_height) =
            mindmap_node_dimensions_from_label_bbox(n, label_width, label_height);

        nodes.push(LayoutNode {
            id: n.id.clone(),
            // Mermaid mindmap uses Cytoscape COSE-Bilkent and initializes node positions at (0,0).
            // We keep that behavior so `manatee` can reproduce upstream placements deterministically.
            x: 0.0,
            y: 0.0,
            width: width.max(1.0),
            height: height.max(1.0),
            is_cluster: false,
            label_width: Some(label_width.max(0.0)),
            label_height: Some(label_height.max(0.0)),
        });
    }
    let mut id_to_idx: rustc_hash::FxHashMap<&str, usize> =
        rustc_hash::FxHashMap::with_capacity_and_hasher(nodes.len(), Default::default());
    for (idx, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.as_str(), idx);
    }

    let mut edge_indices: Vec<(usize, usize)> = Vec::with_capacity(model.edges.len());
    for e in &model.edges {
        let Some(&a) = id_to_idx.get(e.start.as_str()) else {
            return Err(Error::InvalidModel {
                message: format!("edge start node not found: {}", e.start),
            });
        };
        let Some(&b) = id_to_idx.get(e.end.as_str()) else {
            return Err(Error::InvalidModel {
                message: format!("edge end node not found: {}", e.end),
            });
        };
        edge_indices.push((a, b));
    }

    let backend = layout_backend(effective_config);
    match backend {
        MindmapLayoutBackend::Swimlane => {
            return registered::swimlane(model, nodes, config, work_meter);
        }
        MindmapLayoutBackend::Dagre => {
            return registered::dagre(model, nodes, effective_config, _work_control);
        }
        #[cfg(feature = "layout-elk")]
        MindmapLayoutBackend::Elk => {
            return registered::elk(
                model,
                nodes,
                effective_config,
                _work_control,
                operation_seed,
            );
        }
        _ => {}
    }
    let use_tidy_tree = backend == MindmapLayoutBackend::TidyTree;
    let tidy_tree_edges = if use_tidy_tree {
        Some(tidy_tree::layout(
            &mut nodes,
            &model.nodes,
            &model.edges,
            &edge_indices,
        )?)
    } else {
        #[cfg(feature = "layout-cytoscape")]
        {
            let indexed_nodes: Vec<manatee::algo::cose_bilkent::IndexedNode> = nodes
                .iter()
                .map(|n| manatee::algo::cose_bilkent::IndexedNode {
                    width: n.width,
                    height: n.height,
                    x: n.x,
                    y: n.y,
                })
                .collect();
            let mut indexed_edges: Vec<manatee::algo::cose_bilkent::IndexedEdge> =
                Vec::with_capacity(model.edges.len());
            for (edge_idx, (a, b)) in edge_indices.iter().copied().enumerate() {
                if a == b {
                    continue;
                }
                indexed_edges.push(manatee::algo::cose_bilkent::IndexedEdge { a, b });

                // Keep `edge_idx` referenced so unused warnings don't obscure failures if we ever
                // enhance indexed validation error messages.
                let _ = edge_idx;
            }

            let positions = manatee::algo::cose_bilkent::layout_indexed_with_work_control(
                &indexed_nodes,
                &indexed_edges,
                _work_control,
            )
            .map_err(|error| _work_control.map_manatee_error(error))?;

            for (n, p) in nodes.iter_mut().zip(positions) {
                n.x = p.x;
                n.y = p.y;
            }
            None
        }
        #[cfg(not(feature = "layout-cytoscape"))]
        {
            return Err(Error::MissingCapability {
                capability: crate::RenderCapability::LayoutCytoscape,
                diagram_type: "mindmap".to_string(),
            });
        }
    };

    // Mermaid's COSE-Bilkent post-layout normalizes to a positive coordinate space via
    // `transform(0,0)` (layout-base), yielding a content bbox that starts around (15,15) before
    // the 10px viewport padding is applied (viewBox starts at 5,5).
    //
    // Tidy-tree is centered around x=0 and intentionally grows in both horizontal directions.
    // COSE-Bilkent instead normalizes into Mermaid's positive Cytoscape coordinate space.
    if !use_tidy_tree {
        shift_nodes_to_positive_bounds(&mut nodes, 15.0);
    }

    let edges = if let Some(edges) = tidy_tree_edges {
        edges
    } else {
        model
            .edges
            .iter()
            .zip(edge_indices.iter().copied())
            .map(|(edge, (source_index, target_index))| {
                let source = &nodes[source_index];
                let target = &nodes[target_index];
                LayoutEdge {
                    id: edge.id.clone(),
                    from: edge.start.clone(),
                    to: edge.end.clone(),
                    from_cluster: None,
                    to_cluster: None,
                    points: vec![
                        LayoutPoint {
                            x: source.x,
                            y: source.y,
                        },
                        LayoutPoint {
                            x: target.x,
                            y: target.y,
                        },
                    ],
                    label: None,
                    start_label_left: None,
                    start_label_right: None,
                    end_label_left: None,
                    end_label_right: None,
                    start_marker: None,
                    end_marker: None,
                    stroke_dasharray: None,
                }
            })
            .collect()
    };
    let bounds = compute_bounds(&nodes, &edges);
    Ok(MindmapDiagramLayout {
        swimlane_lanes: Vec::new(),
        nodes,
        edges,
        bounds,
    })
}

#[cfg(all(test, feature = "diagram-mindmap"))]
mod tests {
    struct FixedMeasurer;

    impl crate::text::TextMeasurer for FixedMeasurer {
        fn measure(
            &self,
            _text: &str,
            _style: &crate::text::TextStyle,
        ) -> crate::text::TextMetrics {
            crate::text::TextMetrics {
                width: 73.0,
                height: 24.0,
                line_count: 1,
            }
        }
    }

    #[test]
    fn mindmap_max_node_width_accepts_number_and_px_string() {
        let numeric = serde_json::json!({
            "mindmap": {
                "maxNodeWidth": 320
            }
        });
        assert_eq!(super::mindmap_max_node_width_px(&numeric), 320.0);

        let px_string = serde_json::json!({
            "mindmap": {
                "maxNodeWidth": "280px"
            }
        });
        assert_eq!(super::mindmap_max_node_width_px(&px_string), 280.0);

        let plain_string = serde_json::json!({
            "mindmap": {
                "maxNodeWidth": "240"
            }
        });
        assert_eq!(super::mindmap_max_node_width_px(&plain_string), 240.0);

        let fallback = serde_json::json!({});
        assert_eq!(super::mindmap_max_node_width_px(&fallback), 200.0);
    }

    #[test]
    fn mindmap_label_text_for_layout_trims_single_line_delimiter_text() {
        assert_eq!(
            super::mindmap_label_text_for_layout("\n      The root\n    "),
            "The root"
        );
        assert_eq!(
            super::mindmap_label_text_for_layout("\r\nThe root"),
            "The root"
        );
        assert_eq!(super::mindmap_label_text_for_layout("The root"), "The root");
        assert_eq!(
            super::mindmap_label_text_for_layout("\n      first\n      second\n    "),
            "\n      first\n      second\n    "
        );
    }

    #[test]
    fn mindmap_plain_wrapping_label_uses_wrapped_container_width() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let (width, height) = super::mindmap_label_bbox_px(
            "A root with a long text that wraps to keep the node size in check",
            &measurer,
            &style,
            200.0,
        );

        assert!(width > 0.0 && width <= 200.0);
        assert!(
            height > style.font_size,
            "long prose should wrap to multiple rows"
        );
    }

    #[test]
    fn mindmap_markdown_wrapping_respects_max_node_width() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let (width, height) = super::mindmap_label_bbox_px(
            "The dog in **the** hog... a *very long text* that wraps to a new line",
            &measurer,
            &style,
            200.0,
        );

        assert_eq!(width, 200.0);
        assert_eq!(height, 72.0);
    }

    #[test]
    fn mindmap_break_spaces_preserves_trailing_indentation_line_box() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let source = "\n    Multi-line root\n    with three lines\n  ";
        assert_eq!(
            crate::text::mermaid_markdown_to_html_label_fragment(source, true),
            "    Multi-line root\n    with three lines\n  "
        );
        let (width, height) = super::mindmap_label_bbox_px(source, &measurer, &style, 200.0);

        assert_eq!(width, 200.0);
        assert_eq!(height, 72.0);
    }

    #[test]
    fn mindmap_html_labels_measure_visible_content_instead_of_markup() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let expected = crate::text::TextMeasurer::measure_wrapped(
            &measurer,
            "docs",
            &style,
            Some(200.0),
            crate::text::WrapMode::HtmlLike,
        );

        let actual = super::mindmap_label_bbox_px(
            r#"<a href='https://mermaid.js.org/' rel="noopener" target="_blank">docs</a>"#,
            &measurer,
            &style,
            200.0,
        );

        assert_eq!(
            actual,
            (
                crate::text::round_to_1_64_px(expected.width),
                expected.height
            )
        );
    }

    #[test]
    fn mindmap_layout_decodes_mermaid_entity_placeholders_before_measurement() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let expected = super::mindmap_label_bbox_px("Circle: &♥ ∞", &measurer, &style, 200.0);

        let actual =
            super::mindmap_label_bbox_px("Circle: &ﬂ°°9829¶ß &infin;", &measurer, &style, 200.0);

        assert_eq!(actual, expected);
    }

    #[test]
    fn mindmap_code_only_html_measures_visible_code_content() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let expected = crate::text::TextMeasurer::measure_wrapped(
            &measurer,
            "note about mermaid",
            &style,
            Some(200.0),
            crate::text::WrapMode::HtmlLike,
        );

        let actual = super::mindmap_label_bbox_px(
            r#"<a href="https://mermaid.js.org/"><code>note about mermaid</code></a>"#,
            &measurer,
            &style,
            200.0,
        );

        assert_eq!(
            actual,
            (
                crate::text::round_to_1_64_px(expected.width),
                expected.height
            )
        );
    }

    #[test]
    fn mindmap_plain_labels_use_the_selected_measurer_without_content_adjustment() {
        let measurer = FixedMeasurer;
        let style = super::mindmap_text_style(&serde_json::json!({}));

        for text in [
            "String containing []",
            "String containing ()",
            "Waterfall",
            "the root",
            "Root",
            "unseen label",
        ] {
            let (width, height) = super::mindmap_label_bbox_px(text, &measurer, &style, 200.0);
            assert_eq!(width, 73.0);
            assert_eq!(height, 24.0);
        }
    }

    #[test]
    fn mindmap_cloud_layout_uses_rendered_path_bbox_dimensions() {
        let measurer = FixedMeasurer;
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let node = super::MindmapNodeModel {
            id: "0".to_string(),
            dom_id: "node_0".to_string(),
            label: "arbitrary cloud label".to_string(),
            label_type: String::new(),
            is_group: false,
            shape: "cloud".to_string(),
            width: 0.0,
            height: 0.0,
            padding: 10.0,
            css_classes: "mindmap-node section-root section--1".to_string(),
            css_styles: Vec::new(),
            look: String::new(),
            icon: None,
            x: None,
            y: None,
            level: 0,
            node_id: "0".to_string(),
            node_type: 0,
            section: None,
        };

        let (width, height, label_width, label_height) =
            super::mindmap_node_dimensions_px(&node, &measurer, &style, 200.0);

        let shape_width = label_width + node.padding;
        let shape_height = label_height + node.padding;
        let expected = crate::svg::mindmap_cloud_rendered_bbox_size_px(shape_width, shape_height)
            .expect("cloud path bounds");

        assert_eq!((label_width, label_height), (73.0, 24.0));
        assert_eq!((width, height), expected);
        assert!(width > shape_width && height > shape_height);
    }

    #[test]
    fn mindmap_hexagon_layout_uses_mermaid_11_16_shape_geometry() {
        let measurer = FixedMeasurer;
        let style = super::mindmap_text_style(&serde_json::json!({}));
        let node = super::MindmapNodeModel {
            id: "0".to_string(),
            dom_id: "node_0".to_string(),
            label: "arbitrary hexagon label".to_string(),
            label_type: String::new(),
            is_group: false,
            shape: "hexagon".to_string(),
            width: 0.0,
            height: 0.0,
            padding: 20.0,
            css_classes: "mindmap-node section-root section--1".to_string(),
            css_styles: Vec::new(),
            look: String::new(),
            icon: None,
            x: None,
            y: None,
            level: 0,
            node_id: "0".to_string(),
            node_type: 0,
            section: None,
        };

        let dimensions = super::mindmap_node_dimensions_px(&node, &measurer, &style, 200.0);

        assert_eq!(dimensions, (115.0, 44.0, 73.0, 24.0));
    }
}
