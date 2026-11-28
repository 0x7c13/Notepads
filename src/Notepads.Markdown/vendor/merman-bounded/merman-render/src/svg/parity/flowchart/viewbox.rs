//! Flowchart final viewBox/content-bounds preparation.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use super::super::timing::RenderTiming;
use super::viewbox_node_bounds::include_flowchart_node_rendered_bounds;
use super::*;

const TITLE_FONT_SIZE_PX: f64 = 18.0;

pub(in crate::svg::parity::flowchart) struct FlowchartRenderedBoundsRequest<'view, 'data> {
    pub ctx: &'view FlowchartRenderCtx<'data>,
    pub layout: &'view FlowchartLayout,
    pub subgraph_title_y_shift: f64,
}

pub(in crate::svg::parity::flowchart) struct FlowchartViewboxBoundsRequest<
    'borrow,
    'view,
    'data,
    'title,
> {
    pub ctx: &'view FlowchartRenderCtx<'data>,
    pub render_edges: &'view [Cow<'data, crate::flowchart::FlowEdge>],
    pub base_bounds: Bounds,
    pub diagram_title: Option<&'title str>,
    pub font_family: &'borrow str,
    pub title_top_margin: f64,
    pub timing: RenderTiming,
    pub viewbox_edge_curve_bounds: &'borrow mut std::time::Duration,
    pub detail: &'borrow mut FlowchartRenderDetails,
    pub edge_path_cache: &'borrow mut FxHashMap<&'view str, FlowchartEdgePathCacheEntry>,
}

pub(in crate::svg::parity::flowchart) struct FlowchartViewboxBounds {
    pub diagram_title: Option<String>,
    pub title_anchor_x: f64,
    pub bbox_min_x: f64,
    pub bbox_min_y: f64,
    pub bbox_max_x: f64,
    pub bbox_max_y: f64,
}

pub(in crate::svg::parity::flowchart) fn prepare_flowchart_rendered_bounds<'data, F>(
    request: FlowchartRenderedBoundsRequest<'_, 'data>,
    effective_parent_for_id: &F,
) -> Bounds
where
    F: Fn(&str) -> Option<&'data str>,
{
    let FlowchartRenderedBoundsRequest {
        ctx,
        layout,
        subgraph_title_y_shift,
    } = request;
    let mut lca_scratch: Vec<&str> = Vec::new();

    let y_offset_for_root = |root: Option<&str>| -> f64 {
        if root.is_some() && subgraph_title_y_shift.abs() >= 1e-9 {
            -subgraph_title_y_shift
        } else {
            0.0
        }
    };

    // Mermaid's flowchart-v2 renderer draws the self-loop helper nodes (`labelRect`) as
    // `<g class="label edgeLabel" transform="translate(x, y)">` with a `0.1 x 0.1` rect anchored
    // at the translated origin (top-left). Dagre's `x/y` still represent a node center, but the
    // rendered DOM bbox that drives `setupViewPortForSVG(svg, diagramPadding)` is top-left based.
    // Account for that when approximating the final `svg.getBBox()`.
    let mut b: Option<Bounds> = None;
    let mut include_rect = |min_x: f64, min_y: f64, max_x: f64, max_y: f64| {
        if let Some(ref mut cur) = b {
            cur.min_x = cur.min_x.min(min_x);
            cur.min_y = cur.min_y.min(min_y);
            cur.max_x = cur.max_x.max(max_x);
            cur.max_y = cur.max_y.max(max_y);
        } else {
            b = Some(Bounds {
                min_x,
                min_y,
                max_x,
                max_y,
            });
        }
    };

    for c in &layout.clusters {
        let root = if ctx.recursive_clusters.contains(c.id.as_str()) {
            Some(c.id.as_str())
        } else {
            effective_parent_for_id(&c.id)
        };
        let y_off = y_offset_for_root(root);
        let hw = c.width / 2.0;
        let hh = c.height / 2.0;
        include_rect(c.x - hw, c.y + y_off - hh, c.x + hw, c.y + y_off + hh);

        let lhw = c.title_label.width / 2.0;
        let lhh = c.title_label.height / 2.0;
        include_rect(
            c.title_label.x - lhw,
            c.title_label.y + y_off - lhh,
            c.title_label.x + lhw,
            c.title_label.y + y_off + lhh,
        );
    }

    include_flowchart_node_rendered_bounds(
        ctx,
        &layout.nodes,
        subgraph_title_y_shift,
        effective_parent_for_id,
        &mut include_rect,
    );

    for e in &layout.edges {
        let root = lca_for_ids(
            e.from.as_str(),
            e.to.as_str(),
            effective_parent_for_id,
            &mut lca_scratch,
        );
        let y_off = y_offset_for_root(root);
        let missing_section_label =
            edge_geom::missing_section_label_position(ctx, e, ctx.tx, ctx.ty);
        for (label_index, lbl) in [
            e.label.as_ref(),
            e.start_label_left.as_ref(),
            e.start_label_right.as_ref(),
            e.end_label_left.as_ref(),
            e.end_label_right.as_ref(),
        ]
        .into_iter()
        .enumerate()
        .filter_map(|(index, label)| label.map(|label| (index, label)))
        {
            // ELK main labels may move after clipping. Include their final painted position
            // below, after terminal straightening and paired-label separation.
            if ctx.uses_elk_adapter_dom && label_index == 0 {
                continue;
            }
            let edge_label_padding = if ctx.edge_html_labels && lbl.width > 0.0 && lbl.height > 0.0
            {
                ctx.edge_label_padding
            } else {
                0.0
            };
            let label_width = if ctx.edge_html_labels {
                lbl.width + 2.0 * edge_label_padding
            } else {
                lbl.width
            };
            let hw = label_width / 2.0;
            let label_height = if ctx.edge_html_labels {
                lbl.height + 2.0 * edge_label_padding
            } else {
                lbl.height
            };
            let hh = label_height / 2.0;
            let (x, y) = if label_index == 0
                && let Some(point) = &missing_section_label
            {
                (point.x, point.y)
            } else {
                (lbl.x, lbl.y)
            };
            include_rect(x - hw, y + y_off - hh, x + hw, y + y_off + hh);
        }
    }

    b.unwrap_or({
        if layout.nodes.is_empty() && layout.edges.is_empty() && layout.clusters.is_empty() {
            Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
            }
        } else {
            Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 100.0,
                max_y: 100.0,
            }
        }
    })
}

pub(in crate::svg::parity::flowchart) fn prepare_flowchart_viewbox_bounds<'data, F>(
    request: FlowchartViewboxBoundsRequest<'_, '_, 'data, '_>,
    effective_parent_for_id: &F,
) -> Result<FlowchartViewboxBounds>
where
    F: Fn(&str) -> Option<&'data str>,
{
    let FlowchartViewboxBoundsRequest {
        ctx,
        render_edges,
        base_bounds,
        diagram_title,
        font_family,
        title_top_margin,
        timing,
        viewbox_edge_curve_bounds,
        detail,
        edge_path_cache,
    } = request;

    // Mermaid computes the final viewport using `svg.getBBox()` after inserting the title, then
    // applies `setupViewPortForSVG(svg, diagramPadding)`.
    let diagram_title = diagram_title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);

    let mut bbox_min_x = base_bounds.min_x + ctx.tx;
    let mut bbox_min_y = base_bounds.min_y + ctx.ty;
    let mut bbox_max_x = base_bounds.max_x + ctx.tx;
    let mut bbox_max_y = base_bounds.max_y + ctx.ty;

    bbox_max_y += extra_recursive_root_y(ctx);

    // Mermaid derives the final viewport using `svg.getBBox()` (after rendering). For flowcharts
    // this includes the actual curve geometry generated by D3 (which can extend beyond the routed
    // polyline points). Headlessly, approximate that by unioning a tight bbox over each rendered
    // edge path `d` into our base bbox.
    {
        let edge_bounds_base = (bbox_min_x, bbox_min_y, bbox_max_x, bbox_max_y);
        let _g = timing.section(viewbox_edge_curve_bounds);
        let mut lca_scratch: Vec<&str> = Vec::new();
        let mut scratch = FlowchartEdgeDataPointsScratch::default();
        let mut root_offsets: FxHashMap<&str, FlowchartRootOffsets> =
            FxHashMap::with_capacity_and_hasher(8, Default::default());
        root_offsets.insert(
            "",
            FlowchartRootOffsets {
                origin_x: 0.0,
                origin_y: 0.0,
                abs_top_transform: 0.0,
            },
        );
        let mut prepared_routes = Vec::new();
        let mut prepared_edges = Vec::new();
        let original_label_path_work = render_edges
            .iter()
            .filter_map(|edge| ctx.layout_edges_by_id.get(edge.as_ref().id.as_str()))
            .fold(0usize, |sum, edge| {
                sum.saturating_add(edge.points.len().max(2))
            });
        ctx.work_meter.charge(original_label_path_work)?;
        if ctx.uses_elk_adapter_dom {
            let route_work = render_edges
                .iter()
                .filter_map(|edge| ctx.layout_edges_by_id.get(edge.as_ref().id.as_str()))
                .fold(0usize, |sum, edge| {
                    sum.saturating_add(edge.points.len().max(2).saturating_mul(2).saturating_add(1))
                });
            ctx.work_meter.charge(route_work)?;
        }
        for e in render_edges {
            let e = e.as_ref();
            // ELK edges are emitted in the root edge group, including compound edges.
            let root_id = if ctx.uses_elk_adapter_dom {
                ""
            } else {
                let _g = detail_guard(timing, &mut detail.viewbox_edge_curve_lca);
                lca_for_ids(
                    e.from.as_str(),
                    e.to.as_str(),
                    effective_parent_for_id,
                    &mut lca_scratch,
                )
                .unwrap_or("")
            };
            let off = {
                let _g = detail_guard(timing, &mut detail.viewbox_edge_curve_offsets);
                *root_offsets.entry(root_id).or_insert_with(|| {
                    flowchart_cluster_root_offsets(ctx, root_id).unwrap_or(FlowchartRootOffsets {
                        origin_x: 0.0,
                        origin_y: 0.0,
                        abs_top_transform: 0.0,
                    })
                })
            };

            let request = FlowchartEdgePathGeomRequest {
                ctx,
                edge: e,
                origin_x: off.origin_x,
                origin_y: off.origin_y,
                trace_enabled: false,
            };
            if ctx.uses_elk_adapter_dom {
                if let Some(route) = edge_geom::prepare_edge_route(request, &mut scratch) {
                    prepared_routes.push(route);
                    prepared_edges.push((e, off));
                }
                continue;
            }
            detail.viewbox_edge_curve_geom_calls += 1;
            let _g = detail_guard(timing, &mut detail.viewbox_edge_curve_geom);
            let Some(geom) = flowchart_compute_edge_path_geom(request, &mut scratch) else {
                continue;
            };
            if geom.bounds_skipped_for_viewbox {
                detail.viewbox_edge_curve_geom_skipped_bounds += 1;
            }

            {
                let _g = detail_guard(timing, &mut detail.viewbox_edge_curve_bbox_union);
                if let Some(pb) = geom.pb {
                    bbox_min_x = bbox_min_x.min(pb.min_x + off.origin_x);
                    bbox_min_y = bbox_min_y.min(pb.min_y + off.abs_top_transform);
                    bbox_max_x = bbox_max_x.max(pb.max_x + off.origin_x);
                    bbox_max_y = bbox_max_y.max(pb.max_y + off.abs_top_transform);
                }

                edge_path_cache.insert(
                    e.id.as_str(),
                    FlowchartEdgePathCacheEntry {
                        origin_x: off.origin_x,
                        origin_y: off.origin_y,
                        abs_top_transform: off.abs_top_transform,
                        geom,
                    },
                );
            }
        }

        if ctx.uses_elk_adapter_dom {
            if ctx
                .config
                .as_value()
                .get("elk")
                .and_then(|elk| elk.get("straightenEdges"))
                .and_then(serde_json::Value::as_bool)
                != Some(false)
            {
                edge_geom::straighten_edge_terminals(&mut prepared_routes, ctx.work_meter)?;
            }
            edge_geom::separate_edge_labels(
                &mut prepared_routes,
                prepared_edges.iter().map(|(edge, _)| {
                    (
                        edge.from.as_str(),
                        edge.to.as_str(),
                        ctx.model
                            .edge_label_for_render(edge)
                            .is_some_and(|label| !label.is_empty()),
                    )
                }),
                ctx.work_meter,
            )?;
            for ((edge, off), route) in prepared_edges.into_iter().zip(prepared_routes) {
                detail.viewbox_edge_curve_geom_calls += 1;
                let _g = detail_guard(timing, &mut detail.viewbox_edge_curve_geom);
                let Some(geom) = edge_geom::finish_edge_route(
                    FlowchartEdgePathGeomRequest {
                        ctx,
                        edge,
                        origin_x: off.origin_x,
                        origin_y: off.origin_y,
                        trace_enabled: false,
                    },
                    route,
                    &mut scratch,
                ) else {
                    continue;
                };
                if let Some(pb) = geom.pb {
                    bbox_min_x = bbox_min_x.min(pb.min_x + off.origin_x);
                    bbox_min_y = bbox_min_y.min(pb.min_y + off.abs_top_transform);
                    bbox_max_x = bbox_max_x.max(pb.max_x + off.origin_x);
                    bbox_max_y = bbox_max_y.max(pb.max_y + off.abs_top_transform);
                }
                edge_path_cache.insert(
                    edge.id.as_str(),
                    FlowchartEdgePathCacheEntry {
                        origin_x: off.origin_x,
                        origin_y: off.origin_y,
                        abs_top_transform: off.abs_top_transform,
                        geom,
                    },
                );
            }
        }

        if ctx.swimlane_direction.is_some() || ctx.uses_elk_adapter_dom {
            super::swimlane::apply_line_hops_to_edge_geometries(
                edge_path_cache,
                render_edges,
                ctx.config,
                ctx.work_meter,
                ctx.uses_elk_adapter_dom,
            )?;

            // Line hops are a render-time replacement of the original path. Rebuild edge bounds
            // from the same post-processed geometry that SVG emission consumes, while retaining
            // node, cluster, and label bounds as the base.
            (bbox_min_x, bbox_min_y, bbox_max_x, bbox_max_y) = edge_bounds_base;
            for cache_entry in edge_path_cache.values() {
                let _g = detail_guard(timing, &mut detail.viewbox_edge_curve_bbox_union);
                if let Some(pb) = cache_entry.geom.pb {
                    bbox_min_x = bbox_min_x.min(pb.min_x + cache_entry.origin_x);
                    bbox_min_y = bbox_min_y.min(pb.min_y + cache_entry.abs_top_transform);
                    bbox_max_x = bbox_max_x.max(pb.max_x + cache_entry.origin_x);
                    bbox_max_y = bbox_max_y.max(pb.max_y + cache_entry.abs_top_transform);
                }
            }
        }
    }

    if ctx.uses_elk_adapter_dom {
        ctx.work_meter.charge(render_edges.len())?;
        for edge in render_edges {
            let Some(layout_edge) = ctx.layout_edges_by_id.get(edge.id.as_str()) else {
                continue;
            };
            let Some(label) = layout_edge.label.as_ref() else {
                continue;
            };
            let anchor = render::resolve_flowchart_edge_label_position(
                ctx,
                layout_edge,
                label,
                0.0,
                0.0,
                edge_path_cache,
                false,
            );
            let padding = if ctx.edge_html_labels && label.width > 0.0 && label.height > 0.0 {
                ctx.edge_label_padding
            } else {
                0.0
            };
            let half_width = label.width / 2.0 + padding;
            let half_height = label.height / 2.0 + padding;
            bbox_min_x = bbox_min_x.min(anchor.x - half_width);
            bbox_min_y = bbox_min_y.min(anchor.y - half_height);
            bbox_max_x = bbox_max_x.max(anchor.x + half_width);
            bbox_max_y = bbox_max_y.max(anchor.y + half_height);
        }
    }

    // Mermaid centers the title using the pre-title `getBBox()` of the rendered root group.
    let title_anchor_x = (bbox_min_x + bbox_max_x) / 2.0;

    if let Some(title) = diagram_title.as_deref() {
        let title_style = TextStyle {
            font_family: Some(font_family.to_string()),
            font_size: TITLE_FONT_SIZE_PX,
            font_weight: None,
            font_style: None,
        };
        let (title_left, title_right) = ctx.measurer.measure_svg_title_bbox_x(title, &title_style);
        let baseline_y = -title_top_margin;
        let (ascent, descent) = crate::text::svg_text_vertical_extents_px(
            ctx.measurer,
            title,
            &title_style,
            crate::text::svg_title_bbox_vertical_extents_px(&title_style),
        );

        bbox_min_x = bbox_min_x.min(title_anchor_x - title_left);
        bbox_max_x = bbox_max_x.max(title_anchor_x + title_right);
        bbox_min_y = bbox_min_y.min(baseline_y - ascent);
        bbox_max_y = bbox_max_y.max(baseline_y + descent);
    }

    Ok(FlowchartViewboxBounds {
        diagram_title,
        title_anchor_x,
        bbox_min_x,
        bbox_min_y,
        bbox_max_x,
        bbox_max_y,
    })
}

fn lca_for_ids<'a, F>(
    a: &str,
    b: &str,
    effective_parent_for_id: &F,
    scratch: &mut Vec<&'a str>,
) -> Option<&'a str>
where
    F: Fn(&str) -> Option<&'a str>,
{
    scratch.clear();
    let mut cur = effective_parent_for_id(a);
    while let Some(p) = cur {
        scratch.push(p);
        cur = effective_parent_for_id(p);
    }

    let mut cur = effective_parent_for_id(b);
    while let Some(p) = cur {
        if scratch.contains(&p) {
            return Some(p);
        }
        cur = effective_parent_for_id(p);
    }
    None
}

fn extra_recursive_root_y(ctx: &FlowchartRenderCtx<'_>) -> f64 {
    fn effective_parent<'a>(
        parent: &'a FxHashMap<&'a str, &'a str>,
        subgraphs_by_id: &'a FxHashMap<&'a str, &'a crate::flowchart::FlowSubgraph>,
        recursive_clusters: &FxHashSet<&'a str>,
        id: &str,
    ) -> Option<&'a str> {
        let mut cur = parent.get(id).copied();
        while let Some(p) = cur {
            if subgraphs_by_id.contains_key(p) && !recursive_clusters.contains(p) {
                cur = parent.get(p).copied();
                continue;
            }
            return Some(p);
        }
        None
    }

    let mut max_y: f64 = 0.0;
    for &cid in &ctx.recursive_clusters {
        let Some(cluster) = ctx.layout_clusters_by_id.get(cid) else {
            continue;
        };
        let my_parent = effective_parent(
            &ctx.parent,
            &ctx.subgraphs_by_id,
            &ctx.recursive_clusters,
            cid,
        );
        let has_empty_sibling = ctx.subgraphs_by_id.keys().any(|&id| {
            id != cid
                && !ctx.subgraph_has_children(id)
                && ctx.layout_clusters_by_id.contains_key(id)
                && effective_parent(
                    &ctx.parent,
                    &ctx.subgraphs_by_id,
                    &ctx.recursive_clusters,
                    id,
                ) == my_parent
        });
        if has_empty_sibling {
            max_y = max_y.max(cluster.offset_y.max(0.0) * 2.0);
        }
    }
    max_y
}
