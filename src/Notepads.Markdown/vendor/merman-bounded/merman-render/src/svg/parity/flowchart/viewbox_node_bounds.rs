//! Flowchart node rendered-bounds preparation for final viewBox calculation.

use super::render::node::geom::generate_full_sine_wave_points;
use super::*;

fn union_svg_path_bounds(paths: &[&str]) -> Option<crate::svg::parity::path_bounds::SvgPathBounds> {
    let mut bounds: Option<crate::svg::parity::path_bounds::SvgPathBounds> = None;
    for d in paths {
        let Some(pb) = crate::svg::parity::path_bounds::svg_path_bounds_from_d(d) else {
            continue;
        };
        bounds = Some(match bounds {
            Some(mut acc) => {
                acc.min_x = acc.min_x.min(pb.min_x);
                acc.min_y = acc.min_y.min(pb.min_y);
                acc.max_x = acc.max_x.max(pb.max_x);
                acc.max_y = acc.max_y.max(pb.max_y);
                acc
            }
            None => pb,
        });
    }
    bounds
}

fn rough_svg_path_bounds(
    randomness: &roughr::core::RoughRandomness,
    path_data: &str,
) -> Option<crate::svg::parity::path_bounds::SvgPathBounds> {
    let (fill_d, stroke_d) =
        crate::svg::parity::flowchart::render::node::roughjs::roughjs_paths_for_svg_path(
            path_data, 1.3, "0 0", randomness,
        )?;
    union_svg_path_bounds(&[fill_d.as_str(), stroke_d.as_str()])
}

fn rough_stroke_svg_path_bounds(
    randomness: &roughr::core::RoughRandomness,
    path_data: &str,
) -> Option<crate::svg::parity::path_bounds::SvgPathBounds> {
    let stroke_d =
        crate::svg::parity::flowchart::render::node::roughjs::roughjs_stroke_path_for_svg_path(
            path_data, 1.3, "0 0", randomness,
        )?;
    crate::svg::parity::path_bounds::svg_path_bounds_from_d(&stroke_d)
}

fn measure_flowchart_layout_node_label(
    ctx: &FlowchartRenderCtx<'_>,
    n: &LayoutNode,
) -> Option<crate::text::TextMetrics> {
    let flow_node = ctx.nodes_by_id.get(n.id.as_str())?;
    let label = ctx.model.node_label_for_render(flow_node).unwrap_or("");
    let label_type = flow_node
        .label_type
        .as_deref()
        .unwrap_or(if ctx.node_html_labels { "html" } else { "text" });
    let label_base_style = if ctx.node_wrap_mode == crate::text::WrapMode::HtmlLike {
        &ctx.html_label_text_style
    } else {
        &ctx.text_style
    };
    let node_text_style = crate::flowchart::flowchart_effective_text_style_for_node_classes(
        label_base_style,
        ctx.class_defs,
        &flow_node.classes,
        &flow_node.styles,
    );
    let metrics = crate::flowchart::flowchart_label_metrics_for_layout(
        crate::flowchart::FlowchartLabelMetricsRequest {
            measurer: ctx.measurer,
            raw_label: label,
            label_type,
            style: &node_text_style,
            max_width_px: Some(ctx.wrapping_width),
            wrap_mode: ctx.node_wrap_mode,
            config: ctx.config,
            math_renderer: ctx.math_renderer,
        },
    );
    let min_width = if ctx.subgraphs_by_id.contains_key(n.id.as_str()) {
        0.0
    } else {
        crate::flowchart::flowchart_node_label_min_width(
            label,
            flow_node.layout_shape.as_deref(),
            ctx.config,
        )
    };
    Some(metrics.with_label_min_width(label, min_width, None))
}

fn layout_node_metrics_or_zero(
    ctx: &FlowchartRenderCtx<'_>,
    n: &LayoutNode,
) -> crate::text::TextMetrics {
    if let (Some(width), Some(height)) = (n.label_width, n.label_height) {
        return crate::text::TextMetrics {
            width,
            height,
            line_count: 0,
        };
    }
    measure_flowchart_layout_node_label(ctx, n).unwrap_or(crate::text::TextMetrics {
        width: 0.0,
        height: 0.0,
        line_count: 0,
    })
}

fn layout_node_label_size_or_zero(ctx: &FlowchartRenderCtx<'_>, n: &LayoutNode) -> (f64, f64) {
    let metrics = layout_node_metrics_or_zero(ctx, n);
    (metrics.width, metrics.height)
}

fn layout_node_label_width_if_known(ctx: &FlowchartRenderCtx<'_>, n: &LayoutNode) -> Option<f64> {
    n.label_width
        .or_else(|| measure_flowchart_layout_node_label(ctx, n).map(|metrics| metrics.width))
}

pub(in crate::svg::parity::flowchart) fn include_flowchart_node_rendered_bounds<'data, F>(
    ctx: &FlowchartRenderCtx<'data>,
    nodes: &[LayoutNode],
    subgraph_title_y_shift: f64,
    effective_parent_for_id: &F,
    include_rect: &mut impl FnMut(f64, f64, f64, f64),
) where
    F: Fn(&str) -> Option<&'data str>,
{
    let node_padding = ctx.node_padding;
    // ViewBox estimation rebuilds RoughJS geometry but does not emit it. Keep that work on an
    // isolated stream so it cannot advance the operation-owned stream used by actual SVG nodes.
    let bounds_randomness = ctx.hand_drawn_seed.isolated_copy();

    let y_offset_for_root = |root: Option<&str>| -> f64 {
        if root.is_some() && subgraph_title_y_shift.abs() >= 1e-9 {
            -subgraph_title_y_shift
        } else {
            0.0
        }
    };

    for n in nodes {
        let is_empty_subgraph_node = ctx.subgraphs_by_id.contains_key(n.id.as_str())
            && !ctx.subgraph_has_children(n.id.as_str());
        let root = if n.is_cluster && ctx.recursive_clusters.contains(n.id.as_str()) {
            Some(n.id.as_str())
        } else {
            effective_parent_for_id(&n.id)
        };
        let y_off = y_offset_for_root(root);
        if n.is_cluster
            || ctx.node_dom_index.contains_key(n.id.as_str())
            || is_empty_subgraph_node
            || ctx.is_subgraph_collapsed(n.id.as_str())
        {
            let mut left_hw = n.width / 2.0;
            let mut right_hw = left_hw;
            let mut top_hh = n.height / 2.0;
            let mut bottom_hh = top_hh;
            if !n.is_cluster
                && let Some(shape) = ctx
                    .nodes_by_id
                    .get(n.id.as_str())
                    .and_then(|node| node.layout_shape.as_deref())
            {
                if matches!(shape, "bang" | "cloud") {
                    let (label_width, label_height) = layout_node_label_size_or_zero(ctx, n);
                    let geometry = if shape == "bang" {
                        crate::flowchart::bang_geometry(label_width, label_height, node_padding)
                    } else {
                        crate::flowchart::cloud_geometry(label_width, label_height, node_padding)
                    };
                    left_hw = (-geometry.rendered_min_x()).max(0.0);
                    right_hw = geometry.rendered_max_x().max(0.0);
                    top_hh = (-geometry.rendered_min_y()).max(0.0);
                    bottom_hh = geometry.rendered_max_y().max(0.0);
                }

                // Mermaid's flowchart-v2 rhombus node renderer offsets the polygon by
                // `(-width/2 + 0.5, height/2)` so the diamond outline stays on the same
                // pixel lattice as other nodes. This makes the DOM bbox slightly asymmetric
                // around the node center and affects the root `getBBox()` width.
                if shape == "diamond" || shape == "diam" || shape == "rhombus" {
                    left_hw = (left_hw - 0.5).max(0.0);
                    right_hw += 0.5;
                }

                // Mermaid `curvedTrapezoid.ts` draws its rough path from the
                // "theoretical" text+padding width, but Dagre uses the
                // `updateNodeBounds(...)` bbox which can be slightly narrower.
                if matches!(shape, "curv-trap" | "display" | "curved-trapezoid")
                    && let Some(label_w) = layout_node_label_width_if_known(ctx, n)
                {
                    let geometry = crate::flowchart::DisplayGeometry::from_label(
                        label_w,
                        layout_node_label_size_or_zero(ctx, n).1,
                        node_padding,
                        crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
                    );
                    left_hw = geometry.width / 2.0;
                    right_hw = (n.width - left_hw).max(0.0);
                }

                // Mermaid `waveEdgedRectangle.ts` (document) stores Dagre dimensions from
                // `updateNodeBounds(...)`, but the final root viewport comes from the rendered
                // RoughJS path bbox. Rebuild that bbox directly.
                if matches!(shape, "doc" | "document") {
                    let (label_w, label_h) = layout_node_label_size_or_zero(ctx, n);
                    let look_is_neo =
                        crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
                    let padding_x = if look_is_neo { 16.0 } else { node_padding };
                    let padding_y = if look_is_neo { 12.0 } else { node_padding };
                    let w = (label_w + 2.0 * padding_x).max(0.0);
                    let h = (label_h + 2.0 * padding_y).max(0.0);
                    let wave_amplitude = if look_is_neo { h / 4.0 } else { h / 8.0 };
                    let final_h = h + wave_amplitude;
                    let extra_w = ((14.0 - w).max(0.0)) / 2.0;
                    let mut points: Vec<(f64, f64)> = Vec::new();
                    points.push((-w / 2.0 - extra_w, final_h / 2.0));
                    points.extend(generate_full_sine_wave_points(
                        -w / 2.0 - extra_w,
                        final_h / 2.0,
                        w / 2.0 + extra_w,
                        final_h / 2.0,
                        wave_amplitude,
                        0.8,
                    ));
                    points.push((w / 2.0 + extra_w, -final_h / 2.0));
                    points.push((-w / 2.0 - extra_w, -final_h / 2.0));

                    let path_data =
                        crate::svg::parity::roughjs_common::closed_path_d_from_points(&points);
                    if let Some(pb) = rough_svg_path_bounds(&bounds_randomness, &path_data) {
                        let y_shift = -wave_amplitude / 2.0;
                        left_hw = (-pb.min_x).max(0.0);
                        right_hw = pb.max_x.max(0.0);
                        top_hh = (-(pb.min_y + y_shift)).max(0.0);
                        bottom_hh = (pb.max_y + y_shift).max(0.0);
                    }
                }

                // Mermaid `linedWaveEdgedRect.ts` follows the same split as the other wave
                // document shapes: Dagre uses the post-`updateNodeBounds(...)` dimensions,
                // while the rendered root bbox comes from the original label-box path.
                if matches!(shape, "lin-doc" | "lined-document") {
                    let (label_w, label_h) = layout_node_label_size_or_zero(ctx, n);
                    let look_is_neo =
                        crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
                    let padding_x = if look_is_neo { 16.0 } else { node_padding };
                    let padding_y = if look_is_neo { 12.0 } else { node_padding };
                    let w = (label_w + 2.0 * padding_x).max(0.0);
                    let h = (label_h + 2.0 * padding_y).max(0.0);
                    let wave_amplitude = if look_is_neo { h / 4.0 } else { h / 8.0 };
                    let final_h = h + wave_amplitude;
                    let extra = (w / 2.0) * 0.1;
                    let mut points: Vec<(f64, f64)> = Vec::new();
                    points.push((-w / 2.0 - extra, -final_h / 2.0));
                    points.push((-w / 2.0 - extra, final_h / 2.0));
                    points.extend(generate_full_sine_wave_points(
                        -w / 2.0 - extra,
                        final_h / 2.0,
                        w / 2.0 + extra,
                        final_h / 2.0,
                        wave_amplitude,
                        0.8,
                    ));
                    points.push((w / 2.0 + extra, -final_h / 2.0));
                    points.push((-w / 2.0 - extra, -final_h / 2.0));
                    points.push((-w / 2.0, -final_h / 2.0));
                    points.push((-w / 2.0, (final_h / 2.0) * 1.1));
                    points.push((-w / 2.0, -final_h / 2.0));

                    let path_data =
                        crate::svg::parity::roughjs_common::closed_path_d_from_points(&points);
                    if let Some(pb) = rough_svg_path_bounds(&bounds_randomness, &path_data) {
                        let y_shift = -wave_amplitude / 2.0;
                        left_hw = (-pb.min_x).max(0.0);
                        right_hw = pb.max_x.max(0.0);
                        top_hh = (-(pb.min_y + y_shift)).max(0.0);
                        bottom_hh = (pb.max_y + y_shift).max(0.0);
                    }
                }

                // Mermaid `taggedWaveEdgedRectangle.ts` (tagged-document) renders from the
                // base label box, then `updateNodeBounds(...)` stores a slightly shorter outer
                // bbox. The rendered wave is also vertically asymmetric.
                if matches!(shape, "tag-doc" | "tagged-document") {
                    let (label_w, label_h) = layout_node_label_size_or_zero(ctx, n);
                    let w = (label_w + 2.0 * node_padding).max(0.0);
                    let h = (label_h + 2.0 * node_padding).max(0.0);
                    let wave_amplitude = h / 8.0;
                    let final_h = h + wave_amplitude;
                    let extra = (w / 2.0) * 0.1;
                    let tag_width = 0.2 * w;
                    let tag_height = 0.2 * h;

                    let mut wave_points: Vec<(f64, f64)> = Vec::new();
                    wave_points.push((-w / 2.0 - extra, final_h / 2.0));
                    wave_points.extend(generate_full_sine_wave_points(
                        -w / 2.0 - extra,
                        final_h / 2.0,
                        w / 2.0 + extra,
                        final_h / 2.0,
                        wave_amplitude,
                        0.8,
                    ));
                    wave_points.push((w / 2.0 + extra, -final_h / 2.0));
                    wave_points.push((-w / 2.0 - extra, -final_h / 2.0));

                    let x = -w / 2.0 + extra;
                    let y = -final_h / 2.0 - tag_height * 0.4;
                    let mut tag_points: Vec<(f64, f64)> = Vec::new();
                    tag_points.push((x + w - tag_width, (y + h) * 1.3));
                    tag_points.push((x + w, y + h - tag_height));
                    tag_points.push((x + w, (y + h) * 0.9));
                    tag_points.extend(generate_full_sine_wave_points(
                        x + w,
                        (y + h) * 1.25,
                        x + w - tag_width,
                        (y + h) * 1.3,
                        -h * 0.02,
                        0.5,
                    ));

                    let wave_path_data =
                        crate::svg::parity::roughjs_common::closed_path_d_from_points(&wave_points);
                    let tag_path_data =
                        crate::svg::parity::roughjs_common::closed_path_d_from_points(&tag_points);
                    let mut bounds: Option<crate::svg::parity::path_bounds::SvgPathBounds> = None;
                    for pb in [
                        rough_svg_path_bounds(&bounds_randomness, &wave_path_data),
                        rough_svg_path_bounds(&bounds_randomness, &tag_path_data),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        bounds = Some(match bounds {
                            Some(mut acc) => {
                                acc.min_x = acc.min_x.min(pb.min_x);
                                acc.min_y = acc.min_y.min(pb.min_y);
                                acc.max_x = acc.max_x.max(pb.max_x);
                                acc.max_y = acc.max_y.max(pb.max_y);
                                acc
                            }
                            None => pb,
                        });
                    }
                    if let Some(pb) = bounds {
                        let y_shift = -wave_amplitude / 2.0;
                        left_hw = (-pb.min_x).max(0.0);
                        right_hw = pb.max_x.max(0.0);
                        top_hh = (-(pb.min_y + y_shift)).max(0.0);
                        bottom_hh = (pb.max_y + y_shift).max(0.0);
                    }
                }

                // Mermaid computes the root viewport from the rendered DOM bbox. Curly
                // brace/comment shapes emit narrow RoughJS stroke paths plus an invisible
                // path; using the inflated Dagre `node.width / 2` keeps a phantom edge.
                if matches!(
                    shape,
                    "comment" | "brace" | "brace-l" | "brace-r" | "braces"
                ) {
                    let metrics = layout_node_metrics_or_zero(ctx, n);
                    let geometry = crate::svg::parity::flowchart::render::node::shapes::curly_brace_comment_geometry(
                            shape,
                            metrics.width,
                            metrics.height,
                            node_padding,
                            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
                        );
                    // Root getBBox includes the separately translated label,
                    // which can extend beyond Neo's fixed shape padding.
                    include_rect(
                        n.x + geometry.label_dx - metrics.width / 2.0,
                        n.y + y_off + geometry.label_dy - metrics.height / 2.0,
                        n.x + geometry.label_dx + metrics.width / 2.0,
                        n.y + y_off + geometry.label_dy + metrics.height / 2.0,
                    );
                    let mut bounds: Option<crate::svg::parity::path_bounds::SvgPathBounds> = None;
                    for path in geometry.paths {
                        if let Some(mut pb) =
                            rough_stroke_svg_path_bounds(&bounds_randomness, &path.d)
                        {
                            pb.min_x += geometry.group_tx;
                            pb.max_x += geometry.group_tx;
                            bounds = Some(match bounds {
                                Some(mut acc) => {
                                    acc.min_x = acc.min_x.min(pb.min_x);
                                    acc.min_y = acc.min_y.min(pb.min_y);
                                    acc.max_x = acc.max_x.max(pb.max_x);
                                    acc.max_y = acc.max_y.max(pb.max_y);
                                    acc
                                }
                                None => pb,
                            });
                        }
                    }
                    if let Some(pb) = bounds {
                        left_hw = (-pb.min_x).max(0.0);
                        right_hw = pb.max_x.max(0.0);
                        top_hh = (-pb.min_y).max(0.0);
                        bottom_hh = pb.max_y.max(0.0);
                    }
                }

                // Mermaid `forkJoin.ts` inflates Dagre dimensions but the rendered bar
                // remains `70x10` (or `10x70` for LR).
                if matches!(shape, "fork" | "join") {
                    if n.width >= n.height {
                        left_hw = 35.0;
                        right_hw = 35.0;
                        top_hh = 5.0;
                        bottom_hh = 5.0;
                    } else {
                        left_hw = 5.0;
                        right_hw = 5.0;
                        top_hh = 35.0;
                        bottom_hh = 35.0;
                    }
                }

                // Root getBBox includes both painted paths and the displaced label;
                // the unshifted outer polygon remains the layout/intersection boundary.
                if matches!(shape, "docs" | "documents" | "st-doc" | "stacked-document") {
                    let (label_w, label_h) = layout_node_label_size_or_zero(ctx, n);
                    let geometry = crate::flowchart::flowchart_stacked_document_geometry(
                        label_w,
                        label_h,
                        node_padding,
                        crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
                    );
                    let outer = crate::svg::parity::roughjs_common::closed_path_d_from_points(
                        &geometry.outer_points,
                    );
                    let inner = crate::svg::parity::roughjs_common::closed_path_d_from_points(
                        &geometry.inner_points,
                    );
                    let mut bounds: Option<crate::svg::parity::path_bounds::SvgPathBounds> = None;
                    for path in [&outer, &inner] {
                        if let Some(pb) = rough_svg_path_bounds(&bounds_randomness, path) {
                            bounds = Some(match bounds {
                                Some(mut acc) => {
                                    acc.min_x = acc.min_x.min(pb.min_x);
                                    acc.min_y = acc.min_y.min(pb.min_y);
                                    acc.max_x = acc.max_x.max(pb.max_x);
                                    acc.max_y = acc.max_y.max(pb.max_y);
                                    acc
                                }
                                None => pb,
                            });
                        }
                    }
                    if let Some(pb) = bounds {
                        left_hw = (-pb.min_x).max(0.0);
                        right_hw = pb.max_x.max(0.0);
                        top_hh = (-(pb.min_y + geometry.group_dy)).max(0.0);
                        bottom_hh = (pb.max_y + geometry.group_dy).max(0.0);
                    }
                    include_rect(
                        n.x + geometry.label_dx - label_w / 2.0,
                        n.y + y_off + geometry.label_dy - label_h / 2.0,
                        n.x + geometry.label_dx + label_w / 2.0,
                        n.y + y_off + geometry.label_dy + label_h / 2.0,
                    );
                }

                if matches!(shape, "delay" | "half-rounded-rectangle") {
                    let label_w = n.label_width.unwrap_or(0.0);
                    let label_h = n.label_height.unwrap_or(0.0);
                    let geometry = crate::flowchart::DelayGeometry::from_label(
                        label_w,
                        label_h,
                        node_padding,
                        crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
                    );
                    let points = geometry.points;

                    let path_data =
                        crate::svg::parity::roughjs_common::closed_path_d_from_points(&points);
                    if let Some(pb) = rough_svg_path_bounds(&bounds_randomness, &path_data) {
                        left_hw = (-pb.min_x).max(0.0);
                        right_hw = pb.max_x.max(0.0);
                        top_hh = (-pb.min_y).max(0.0);
                        bottom_hh = pb.max_y.max(0.0);
                    }
                }

                if matches!(shape, "notch-pent" | "loop-limit" | "notched-pentagon") {
                    let label_w = n.label_width.unwrap_or(0.0);
                    let label_h = n.label_height.unwrap_or(0.0);
                    let look_is_neo =
                        crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
                    let padding_x = if look_is_neo { 16.0 } else { node_padding };
                    let padding_y = if look_is_neo { 12.0 } else { node_padding };
                    let w = label_w + 2.0 * padding_x;
                    let h = label_h + 2.0 * padding_y;
                    let points = vec![
                        ((-w / 2.0) * 0.8, -h / 2.0),
                        ((w / 2.0) * 0.8, -h / 2.0),
                        (w / 2.0, (-h / 2.0) * 0.6),
                        (w / 2.0, h / 2.0),
                        (-w / 2.0, h / 2.0),
                        (-w / 2.0, (-h / 2.0) * 0.6),
                    ];
                    let path_data =
                        crate::svg::parity::roughjs_common::closed_path_d_from_points(&points);
                    if let Some(pb) = rough_svg_path_bounds(&bounds_randomness, &path_data) {
                        left_hw = (-pb.min_x).max(0.0);
                        right_hw = pb.max_x.max(0.0);
                        top_hh = (-pb.min_y).max(0.0);
                        bottom_hh = pb.max_y.max(0.0);
                    }
                }
            }
            include_rect(
                n.x - left_hw,
                n.y + y_off - top_hh,
                n.x + right_hw,
                n.y + y_off + bottom_hh,
            );
        } else {
            include_rect(n.x, n.y + y_off, n.x + n.width, n.y + y_off + n.height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::RenderEnvironment;
    use crate::model::FlowchartLayout;
    use crate::svg::parity::flowchart::svg_emit::{
        FlowchartSvgModelRequest, render_flowchart_svg_model,
    };
    use crate::svg::{SvgDebugOptions, SvgRenderOptions};
    use merman_core::{Engine, ParseOptions, RenderSemanticModel};

    fn render_measured_shape(
        shape: &str,
        neo: bool,
        padding: f64,
        metrics: crate::text::TextMetrics,
    ) -> String {
        let look = if neo { "neo" } else { "classic" };
        let source = format!(
            "---\nconfig:\n  look: {look}\n  flowchart:\n    htmlLabels: true\n    minNodeWidth: 0\n    padding: {padding}\n---\nflowchart TD\nA@{{ shape: {shape}, label: Label }}\n"
        );
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        let render_context = parsed.flowchart_render_context().unwrap().clone();
        let (metadata, semantic) = parsed.into_parts();
        let RenderSemanticModel::Flowchart(model) = semantic else {
            panic!("expected Flowchart");
        };
        let (width, height) =
            crate::flowchart::flowchart_node_render_dimensions(Some(shape), metrics, padding, neo);
        let layout = FlowchartLayout {
            nodes: vec![LayoutNode {
                id: "A".into(),
                x: 100.0,
                y: 100.0,
                width,
                height,
                is_cluster: false,
                label_width: Some(metrics.width),
                label_height: Some(metrics.height),
            }],
            edges: Vec::new(),
            clusters: Vec::new(),
            bounds: None,
            dom_node_order_by_root: std::collections::HashMap::from([(
                String::new(),
                vec!["A".into()],
            )]),
            uses_elk_adapter_dom: false,
        };
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        let request = SvgRenderOptions {
            diagram_id: Some("brace-label-bounds".into()),
            ..SvgRenderOptions::default()
        };
        let debug = SvgDebugOptions::default();
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
    }

    fn assert_neo_shape_viewport_contains_shifted_label(shape: &str) {
        for padding in [15.0, 100.0] {
            let metrics = crate::text::TextMetrics {
                width: 100.0,
                height: 20.0,
                line_count: 1,
            };
            let (width, height) = crate::flowchart::flowchart_node_render_dimensions(
                Some(shape),
                metrics,
                padding,
                true,
            );
            let svg = render_measured_shape(shape, true, padding, metrics);
            let doc = roxmltree::Document::parse(&svg).unwrap();
            let viewbox: Vec<f64> = doc
                .root_element()
                .attribute("viewBox")
                .unwrap()
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            let label = doc
                .descendants()
                .find(|node| node.has_tag_name("foreignObject"))
                .unwrap();
            let mut label_x = 0.0;
            let mut label_y = 0.0;
            for node in label.ancestors() {
                if let Some(transform) = node.attribute("transform") {
                    let translation = transform
                        .strip_prefix("translate(")
                        .and_then(|value| value.strip_suffix(')'))
                        .expect("translation only");
                    let values: Vec<f64> = translation
                        .split([',', ' '])
                        .filter(|value| !value.is_empty())
                        .map(|value| value.parse().unwrap())
                        .collect();
                    label_x += values[0];
                    label_y += values.get(1).copied().unwrap_or(0.0);
                }
            }
            let label_width: f64 = label.attribute("width").unwrap().parse().unwrap();
            let label_height: f64 = label.attribute("height").unwrap().parse().unwrap();
            let diagram_padding = 8.0;
            assert!(
                viewbox[0] <= label_x - diagram_padding + 1e-6,
                "{shape}, padding {padding}"
            );
            assert!(
                viewbox[1] <= label_y - diagram_padding + 1e-6,
                "{shape}, padding {padding}"
            );
            assert!(
                viewbox[0] + viewbox[2] >= label_x + label_width + diagram_padding - 1e-6,
                "{shape}, padding {padding}: label exceeds right viewport; viewbox={viewbox:?}, label=({label_x},{label_y},{label_width},{label_height})"
            );
            assert!(
                viewbox[1] + viewbox[3] >= label_y + label_height + diagram_padding - 1e-6,
                "{shape}, padding {padding}: label exceeds bottom viewport; viewbox={viewbox:?}, label=({label_x},{label_y},{label_width},{label_height})"
            );
            if shape == "documents" {
                // Pinned h=56, amplitude14: label offset(-10,-4), group y=-7.
                assert_eq!((label_x, label_y), (40.0, 86.0));
                let body = doc
                    .descendants()
                    .find(|node| {
                        node.attribute("class") == Some("basic label-container outer-path")
                    })
                    .unwrap();
                assert_eq!(body.attribute("transform"), Some("translate(0,-7)"));
            } else if padding == 15.0 {
                // A contained label must not inflate the ordinary shape viewport.
                assert!((viewbox[2] - width - 2.0 * diagram_padding).abs() < 1e-6);
                assert!((viewbox[3] - height - 2.0 * diagram_padding).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn neo_brace_viewport_contains_shifted_labels_at_large_padding() {
        for shape in ["brace", "brace-r", "braces"] {
            assert_neo_shape_viewport_contains_shifted_label(shape);
        }
    }

    #[test]
    fn neo_stacked_document_viewport_contains_source_positioned_label() {
        assert_neo_shape_viewport_contains_shifted_label("documents");
    }
    #[test]
    fn wave_document_labels_use_source_padding_and_wave_offsets() {
        for (shape, neo) in [
            ("doc", false),
            ("doc", true),
            ("lin-doc", false),
            ("lin-doc", true),
        ] {
            let padding = 15.0;
            let svg = render_measured_shape(
                shape,
                neo,
                padding,
                crate::text::TextMetrics {
                    width: 120.0,
                    height: if shape == "doc" { 42.0 } else { 63.0 },
                    line_count: 1,
                },
            );
            let doc = roxmltree::Document::parse(&svg).unwrap();
            let label = doc
                .descendants()
                .find(|node| node.has_tag_name("foreignObject"))
                .unwrap();
            let label_width: f64 = label.attribute("width").unwrap().parse().unwrap();
            let label_height: f64 = label.attribute("height").unwrap().parse().unwrap();
            let padding_x = if neo { 16.0 } else { padding };
            let padding_y = if neo { 12.0 } else { padding };
            let w = label_width + 2.0 * padding_x;
            let h = label_height + 2.0 * padding_y;
            let wave_amplitude = if neo { h / 4.0 } else { h / 8.0 };
            let extension = if shape == "lin-doc" {
                (w / 2.0) * 0.1 / 2.0
            } else {
                0.0
            };
            let expected_x = -w / 2.0 + padding + extension;
            let expected_y = -h / 2.0 + padding - wave_amplitude;
            let transform = label
                .parent_element()
                .and_then(|node| node.attribute("transform"))
                .unwrap();
            let values: Vec<f64> = transform
                .strip_prefix("translate(")
                .and_then(|value| value.strip_suffix(')'))
                .unwrap()
                .split([',', ' '])
                .filter(|value| !value.is_empty())
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(values.len(), 2, "{shape}, neo={neo}");
            assert!(
                (values[0] - expected_x).abs() < 1e-6,
                "{shape}, neo={neo}: values={values:?}, expected={expected_x}"
            );
            assert!(
                (values[1] - expected_y).abs() < 1e-6,
                "{shape}, neo={neo}: values={values:?}, expected={expected_y}"
            );
        }
    }

    #[test]
    fn sloped_rectangle_label_uses_authored_padding_against_neo_geometry() {
        let padding = 15.0;
        let metrics = crate::text::TextMetrics {
            width: 120.0,
            height: 42.0,
            line_count: 1,
        };
        let svg = render_measured_shape("manual-input", true, padding, metrics);
        let doc = roxmltree::Document::parse(&svg).unwrap();
        let label = doc
            .descendants()
            .find(|node| node.has_tag_name("foreignObject"))
            .unwrap();
        let label_width: f64 = label.attribute("width").unwrap().parse().unwrap();
        let label_height: f64 = label.attribute("height").unwrap().parse().unwrap();
        let w = label_width + 32.0;
        let h = label_height + 24.0;
        let transform = label
            .parent_element()
            .and_then(|node| node.attribute("transform"))
            .unwrap();
        let values: Vec<f64> = transform
            .strip_prefix("translate(")
            .and_then(|value| value.strip_suffix(')'))
            .unwrap()
            .split([',', ' '])
            .filter(|value| !value.is_empty())
            .map(|value| value.parse().unwrap())
            .collect();
        assert!((values[0] - (-w / 2.0 + padding)).abs() < 1e-6);
        assert!((values[1] - (-h / 4.0 + padding)).abs() < 1e-6);
    }

    #[test]
    fn neo_shape_svg_uses_source_vertices_rings_and_label_shift() {
        let metrics = crate::text::TextMetrics {
            width: 100.0,
            height: 20.0,
            line_count: 1,
        };
        for (shape, expected) in [
            (
                "lean-r",
                vec![(-17.5, 0.0), (130.0, 0.0), (147.5, -35.0), (0.0, -35.0)],
            ),
            (
                "lean-l",
                vec![(0.0, 0.0), (147.5, 0.0), (130.0, -35.0), (-17.5, -35.0)],
            ),
            (
                "trap-b",
                vec![(-17.5, 0.0), (147.5, 0.0), (130.0, -35.0), (0.0, -35.0)],
            ),
            (
                "trap-t",
                vec![(0.0, 0.0), (160.0, 0.0), (185.0, -50.0), (-25.0, -50.0)],
            ),
            (
                "hex",
                vec![
                    (25.714285714285715, 0.0),
                    (157.71428571428572, 0.0),
                    (183.42857142857144, -45.0),
                    (157.71428571428572, -90.0),
                    (25.714285714285715, -90.0),
                    (0.0, -45.0),
                ],
            ),
        ] {
            let svg = render_measured_shape(shape, true, 15.0, metrics);
            let doc = roxmltree::Document::parse(&svg).unwrap();
            let polygon = doc
                .descendants()
                .find(|node| {
                    node.has_tag_name("polygon")
                        && node.attribute("class") == Some("label-container")
                })
                .unwrap();
            let actual: Vec<(f64, f64)> = polygon
                .attribute("points")
                .unwrap()
                .split_whitespace()
                .map(|point| {
                    let (x, y) = point.split_once(',').unwrap();
                    (x.parse().unwrap(), y.parse().unwrap())
                })
                .collect();
            assert_eq!(actual.len(), expected.len(), "{shape}");
            for (actual, expected) in actual.iter().zip(&expected) {
                // SVG formatting rounds coordinates; the pure source test retains full precision.
                assert!(
                    (actual.0 - expected.0).abs() < 0.001 && (actual.1 - expected.1).abs() < 0.001,
                    "{shape}: {actual:?} != {expected:?}"
                );
            }
        }
        for padding in [0.0, 15.0, 31.0] {
            let svg = render_measured_shape("odd", true, padding, metrics);
            let doc = roxmltree::Document::parse(&svg).unwrap();
            let body = doc
                .descendants()
                .find(|node| node.attribute("class") == Some("basic label-container outer-path"))
                .unwrap();
            assert_eq!(body.attribute("transform"), Some("translate(5.5,0)"));
            // Exact RoughJS path bounds are covered by pure geometry tests; this DOM check
            // only guards the source label shift and final containment.
            let label = doc
                .descendants()
                .find(|node| node.has_tag_name("foreignObject"))
                .unwrap();
            assert_eq!(
                label.parent_element().unwrap().attribute("transform"),
                Some("translate(-44.5,-10)")
            );
            let viewbox: Vec<f64> = doc
                .root_element()
                .attribute("viewBox")
                .unwrap()
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            let label_width: f64 = label.attribute("width").unwrap().parse().unwrap();
            let label_height: f64 = label.attribute("height").unwrap().parse().unwrap();
            assert!(viewbox[2] >= label_width + 16.0 && viewbox[3] >= label_height + 16.0);
        }
        let metrics = crate::text::TextMetrics {
            width: 40.0,
            height: 30.0,
            line_count: 1,
        };
        for (neo, padding, inner, outer) in [
            (false, 0.0, 25.0, 30.0),
            (false, 15.0, 40.0, 45.0),
            (false, 31.0, 56.0, 61.0),
            (true, 0.0, 41.0, 53.0),
            (true, 15.0, 41.0, 53.0),
            (true, 31.0, 41.0, 53.0),
        ] {
            let svg = render_measured_shape("dbl-circ", neo, padding, metrics);
            let doc = roxmltree::Document::parse(&svg).unwrap();
            for (class, expected) in [("inner-circle", inner), ("outer-circle", outer)] {
                let circle = doc
                    .descendants()
                    .find(|node| node.attribute("class") == Some(class))
                    .unwrap();
                let radius: f64 = circle.attribute("r").unwrap().parse().unwrap();
                assert_eq!(radius, expected, "neo={neo}, padding={padding}, {class}");
            }
            let viewbox: Vec<f64> = doc
                .root_element()
                .attribute("viewBox")
                .unwrap()
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            assert_eq!(
                viewbox,
                vec![
                    100.0 - outer - 8.0,
                    100.0 - outer - 8.0,
                    2.0 * outer + 16.0,
                    2.0 * outer + 16.0
                ]
            );
        }
    }
}
