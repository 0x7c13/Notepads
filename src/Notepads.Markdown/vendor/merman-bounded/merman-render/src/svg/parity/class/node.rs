use crate::entities::{decode_entities_minimal, decode_entities_minimal_cow};
use crate::model::{Bounds, ClassNodeLabelPlan, ClassPreparedHtmlLabel, LayoutNode};
use crate::text::{TextMeasurer, TextStyle, WrapMode};
use merman_core::models::class_diagram::ClassMember;
use merman_core::svg_security::{MermaidNavigationSecurity, prepare_mermaid_navigation_href};
use std::fmt::Write as _;
use std::time::Duration;

use super::super::SvgDiagramId;
use super::super::timing::RenderTiming;
use super::super::{escape_attr_display, escape_xml_into, fmt, fmt_into};
use super::bounds::{include_path_d, include_xywh};
use super::context::ClassEmitCheckpoint;
use super::label::{
    ClassHtmlLabelSpec, class_html_div_style, class_html_label_metrics, class_html_title_metrics,
    class_svg_label_rect, render_class_html_label, wrap_class_svg_text_like_mermaid,
    write_class_svg_text_markdown,
};
use super::rough::{
    class_rough_hachure_rect_paths, class_rough_hand_drawn_line_path,
    class_rough_line_double_path_and_bounds, class_rough_rect_stroke_path_and_bounds,
    class_rough_seed,
};
use super::{ClassSvgNode, Rect};

#[derive(Debug, Clone, Copy)]
pub(super) struct ClassNodeRenderPosition {
    pub node_tx: f64,
    pub node_ty: f64,
    pub node_bounds_tx: f64,
    pub node_bounds_ty: f64,
}

#[derive(Debug, Clone)]
pub(super) struct ClassNodeBoxGeometry {
    pub w: f64,
    pub h: f64,
    pub left: f64,
    pub rough_seed: roughr::core::RoughRandomness,
}

pub(super) struct ClassNodeRenderState<'a> {
    pub out: &'a mut String,
    pub content_bounds: &'a mut Option<Bounds>,
}

pub(super) struct ClassNodeShellContext<'a> {
    pub diagram_id: SvgDiagramId<'a>,
    pub emit: ClassEmitCheckpoint<'a>,
    pub look: &'a str,
    pub security_level_loose: bool,
    pub color_index: Option<usize>,
    pub palette_size: usize,
}

pub(super) struct ClassNodeBasicContainerContext<'a> {
    pub diagram_id: SvgDiagramId<'a>,
    pub node_style_attr: &'a str,
    pub node_fill: &'a str,
    pub node_stroke: &'a str,
    pub node_stroke_width: &'a str,
    pub node_stroke_dasharray: &'a str,
    pub look: &'a str,
    pub hand_drawn_seed: roughr::core::RoughRandomness,
    pub timing: RenderTiming,
}

pub(super) struct ClassNodeDividerContext<'a> {
    pub node_style_attr: &'a str,
    pub node_stroke: &'a str,
    pub node_stroke_width: &'a str,
    pub node_stroke_dasharray: &'a str,
    pub look: &'a str,
    pub use_gradient: bool,
    pub timing: RenderTiming,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ClassNodeRenderStats {
    pub path_bounds: Duration,
    pub path_bounds_calls: usize,
}

pub(super) struct ClassNodeBasicContainerResult {
    pub geometry: ClassNodeBoxGeometry,
    pub stats: ClassNodeRenderStats,
}

pub(super) struct ClassHtmlNodeRow<'a> {
    pub text: String,
    pub row_style: String,
    pub metrics: crate::text::TextMetrics,
    pub max_width_px: i64,
    pub prepared_xhtml: Option<&'a str>,
    pub y: f64,
}

pub(super) struct ClassHtmlNodeRows<'a> {
    pub rows: Vec<ClassHtmlNodeRow<'a>>,
    pub raw_height: f64,
}

pub(super) struct ClassHtmlNodeRowsContext<'a> {
    pub measurer: &'a dyn TextMeasurer,
    pub text_style: &'a TextStyle,
    pub html_calc_text_style: &'a TextStyle,
    pub line_height: f64,
}

pub(super) struct ClassSvgNodeLabelRun {
    pub text: String,
    pub style: String,
    pub metrics: crate::text::TextMetrics,
    pub y_offset: f64,
}

pub(super) struct ClassHtmlNodeLabelGroupSpec<'a> {
    pub label_style: &'a str,
    pub translate_y: f64,
    pub width: f64,
    pub height: f64,
    pub div_style: &'a str,
    pub text: &'a str,
    pub include_p: bool,
    pub extra_span_class: Option<&'a str>,
    pub span_style: Option<&'a str>,
    pub prepared_xhtml: Option<&'a str>,
    pub mermaid_config: Option<&'a merman_core::MermaidConfig>,
    pub math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
}

pub(super) struct ClassHtmlNodeBodyContext<'a> {
    pub measurer: &'a dyn TextMeasurer,
    pub text_style: &'a TextStyle,
    pub html_calc_text_style: &'a TextStyle,
    pub line_height: f64,
    pub class_padding: f64,
    pub hide_empty_members_box: bool,
    pub node_style_attr: &'a str,
    pub node_stroke: &'a str,
    pub node_stroke_width: &'a str,
    pub node_stroke_dasharray: &'a str,
    pub look: &'a str,
    pub use_gradient: bool,
    pub mermaid_config: Option<&'a merman_core::MermaidConfig>,
    pub math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub timing: RenderTiming,
}

pub(super) struct ClassSvgNodeBodyContext<'a> {
    pub measurer: &'a dyn TextMeasurer,
    pub text_style: &'a TextStyle,
    pub wrap_probe_font_size: f64,
    pub class_padding: f64,
    pub hide_empty_members_box: bool,
    pub node_style_attr: &'a str,
    pub node_stroke: &'a str,
    pub node_stroke_width: &'a str,
    pub node_stroke_dasharray: &'a str,
    pub look: &'a str,
    pub use_gradient: bool,
    pub timing: RenderTiming,
}

fn write_class_color_slot(out: &mut String, color_index: Option<usize>, palette_size: usize) {
    if palette_size > 0
        && let Some(index) = color_index
    {
        let _ = write!(out, r#" data-color-id="color-{}""#, index % palette_size);
    }
}

pub(super) fn render_class_node_shell_open(
    out: &mut String,
    node: &ClassSvgNode,
    position: ClassNodeRenderPosition,
    ctx: &ClassNodeShellContext<'_>,
) -> crate::Result<bool> {
    let tooltip = node.tooltip.as_deref().unwrap_or("").trim();
    let has_tooltip = !tooltip.is_empty();

    let link = node.link.as_deref().filter(|s| !s.is_empty());
    let href = link.and_then(|href| {
        prepare_mermaid_navigation_href(
            href,
            MermaidNavigationSecurity::from_security_level_loose(ctx.security_level_loose),
        )
    });
    let have_callback = node.have_callback;

    if link.is_some() {
        out.push_str(r#"<a data-look=""#);
        super::super::util::escape_attr_into(out, ctx.look);
        out.push('"');
        if let Some(href) = href.as_ref() {
            out.push_str(r#" xlink:href=""#);
            out.push_str(href.as_serialized_str());
            out.push('"');
        }
        if ctx.security_level_loose
            && let Some(target) = node
                .link_target
                .as_deref()
                .map(str::trim)
                .filter(|target| !target.is_empty())
        {
            out.push_str(r#" target=""#);
            super::super::util::escape_attr_into(out, target);
            out.push('"');
        }
        if have_callback {
            out.push_str(r#" class="null clickable""#);
        }
        out.push_str(r#" transform="translate("#);
        fmt_into(out, position.node_tx);
        out.push_str(", ");
        fmt_into(out, position.node_ty);
        out.push_str(r#")">"#);
    }

    out.push_str(r#"<g class=""#);
    if ctx.look == "handDrawn" {
        out.push_str("rough-node ");
    } else {
        out.push_str("node ");
    }
    super::super::util::escape_attr_into(out, node.css_classes.trim());
    out.push_str(r#"" id=""#);
    let _ = write!(out, "{}", ctx.diagram_id);
    ctx.emit.checkpoint()?;
    out.push('-');
    super::super::util::escape_attr_into(out, &node.dom_id);
    out.push('"');
    if link.is_none() {
        out.push_str(r#" data-look=""#);
        super::super::util::escape_attr_into(out, ctx.look);
        out.push('"');
    }
    write_class_color_slot(out, ctx.color_index, ctx.palette_size);
    if has_tooltip {
        out.push_str(r#" title=""#);
        super::super::util::escape_attr_into(out, tooltip);
        out.push('"');
    }
    if link.is_none() {
        out.push_str(r#" transform="translate("#);
        fmt_into(out, position.node_tx);
        out.push_str(", ");
        fmt_into(out, position.node_ty);
        out.push_str(r#")""#);
    }
    out.push('>');

    Ok(link.is_some())
}

pub(super) fn render_class_node_basic_container(
    state: ClassNodeRenderState<'_>,
    node: &ClassSvgNode,
    layout_node: &LayoutNode,
    position: ClassNodeRenderPosition,
    ctx: &ClassNodeBasicContainerContext<'_>,
) -> ClassNodeBasicContainerResult {
    let out = &mut *state.out;
    let content_bounds = &mut *state.content_bounds;
    let mut stats = ClassNodeRenderStats::default();

    let w = layout_node.width.max(1.0);
    let h = layout_node.height.max(1.0);
    let left = -w / 2.0;
    let top = -h / 2.0;
    let rough_seed = class_rough_seed(
        &ctx.hand_drawn_seed,
        ctx.diagram_id.semantic_str(),
        &node.dom_id,
    );
    let hand_drawn = ctx.look == "handDrawn";
    out.push_str(r#"<g class="basic label-container outer-path">"#);
    if !hand_drawn {
        let _ = write!(
            out,
            r#"<path d="M{} {} L{} {} L{} {} L{} {}" stroke="none" stroke-width="0" fill="{}" style="{}"/>"#,
            fmt(left),
            fmt(top),
            fmt(left + w),
            fmt(top),
            fmt(left + w),
            fmt(top + h),
            fmt(left),
            fmt(top + h),
            escape_attr_display(ctx.node_fill),
            escape_attr_display(ctx.node_style_attr)
        );
    }

    let (fill_d, stroke_d) = if hand_drawn {
        class_rough_hachure_rect_paths(
            left,
            top,
            w,
            h,
            ctx.node_fill,
            ctx.node_stroke,
            ctx.node_stroke_width.parse::<f32>().unwrap_or(1.3),
            ctx.node_stroke_dasharray,
            &rough_seed,
        )
        .unwrap_or_else(|| {
            let (stroke_d, _) =
                class_rough_rect_stroke_path_and_bounds(left, top, w, h, &rough_seed);
            (String::new(), stroke_d)
        })
    } else {
        let (stroke_d, _) = class_rough_rect_stroke_path_and_bounds(left, top, w, h, &rough_seed);
        (String::new(), stroke_d)
    };
    include_xywh(
        content_bounds,
        position.node_bounds_tx + left,
        position.node_bounds_ty + top,
        w,
        h,
    );
    let path_bounds_start = ctx.timing.start();
    include_path_d(
        content_bounds,
        &stroke_d,
        position.node_bounds_tx,
        position.node_bounds_ty,
    );
    if let Some(s) = path_bounds_start {
        stats.path_bounds += s.elapsed();
        stats.path_bounds_calls += 1;
    }
    if hand_drawn {
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="1.5" fill="none" stroke-dasharray="0 0"/>"#,
            escape_attr_display(&fill_d),
            escape_attr_display(ctx.node_fill),
        );
    }
    let _ = write!(
        out,
        r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/>"#,
        escape_attr_display(&stroke_d),
        escape_attr_display(ctx.node_stroke),
        escape_attr_display(ctx.node_stroke_width),
        escape_attr_display(ctx.node_stroke_dasharray),
        escape_attr_display(ctx.node_style_attr),
    );
    out.push_str("</g>");

    ClassNodeBasicContainerResult {
        geometry: ClassNodeBoxGeometry {
            w,
            h,
            left,
            rough_seed,
        },
        stats,
    }
}

pub(super) fn render_class_node_dividers(
    state: ClassNodeRenderState<'_>,
    position: ClassNodeRenderPosition,
    left: f64,
    right: f64,
    divider_ys: [f64; 2],
    rough_seed: &roughr::core::RoughRandomness,
    ctx: &ClassNodeDividerContext<'_>,
) -> ClassNodeRenderStats {
    let out = &mut *state.out;
    let content_bounds = &mut *state.content_bounds;
    let mut stats = ClassNodeRenderStats::default();

    for y in divider_ys {
        let _ = write!(
            out,
            r#"<g class="divider{}" style="{}">"#,
            if ctx.look == "neo" && !ctx.use_gradient {
                " neo-line"
            } else {
                ""
            },
            escape_attr_display(ctx.node_style_attr)
        );
        let d = if ctx.look == "handDrawn" {
            class_rough_hand_drawn_line_path(
                left,
                y,
                right,
                y + 0.001,
                ctx.node_stroke,
                ctx.node_stroke_width.parse::<f32>().unwrap_or(1.3),
                ctx.node_stroke_dasharray,
                rough_seed,
            )
            .unwrap_or_else(|| {
                class_rough_line_double_path_and_bounds(left, y, right, y, rough_seed).0
            })
        } else {
            class_rough_line_double_path_and_bounds(left, y, right, y, rough_seed).0
        };
        let path_bounds_start = ctx.timing.start();
        include_path_d(
            content_bounds,
            &d,
            position.node_bounds_tx,
            position.node_bounds_ty,
        );
        if let Some(s) = path_bounds_start {
            stats.path_bounds += s.elapsed();
            stats.path_bounds_calls += 1;
        }
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/>"#,
            escape_attr_display(&d),
            escape_attr_display(ctx.node_stroke),
            escape_attr_display(ctx.node_stroke_width),
            escape_attr_display(ctx.node_stroke_dasharray),
            escape_attr_display(ctx.node_style_attr),
        );
        out.push_str("</g>");
    }

    stats
}

pub(super) fn render_class_html_node_body(
    state: ClassNodeRenderState<'_>,
    position: ClassNodeRenderPosition,
    node: &ClassSvgNode,
    geometry: ClassNodeBoxGeometry,
    label_plan: Option<&ClassNodeLabelPlan>,
    ctx: &ClassHtmlNodeBodyContext<'_>,
) -> ClassNodeRenderStats {
    let out = &mut *state.out;
    let content_bounds = &mut *state.content_bounds;
    let padding = ctx.class_padding.max(0.0);
    let gap = padding;
    let members_rows = node.members.len();
    let methods_rows = node.methods.len();
    let render_extra_box = members_rows == 0 && methods_rows == 0 && !ctx.hide_empty_members_box;
    let content_bbox_height = if render_extra_box {
        (geometry.h - 4.0 * padding).max(0.0)
    } else if members_rows == 0 && methods_rows == 0 {
        (geometry.h - padding).max(0.0)
    } else {
        (geometry.h - 2.0 * padding).max(0.0)
    };
    let content_top = -content_bbox_height / 2.0;
    let text_translate_y = if render_extra_box {
        content_top
    } else if members_rows == 0 && methods_rows == 0 {
        content_top + padding * 1.5
    } else {
        content_top + padding
    };

    let (class_row_metrics, prepared_html) = match label_plan {
        Some(ClassNodeLabelPlan::RowMetrics(metrics)) => (Some(metrics), None),
        Some(ClassNodeLabelPlan::PreparedHtml(prepared)) => (None, Some(prepared)),
        None => (None, None),
    };

    let title_text = node.text.trim();
    let title_max_width_px = prepared_html.map_or_else(
        || {
            crate::class::class_html_create_text_width_px(
                title_text,
                ctx.measurer,
                ctx.html_calc_text_style,
            )
        },
        |prepared| prepared.title.max_width_px,
    );
    let title_metrics = prepared_html.map_or_else(
        || {
            ctx.mermaid_config
                .and_then(|config| {
                    crate::class::class_math_label_metrics(
                        title_text,
                        ctx.measurer,
                        ctx.text_style,
                        Some(title_max_width_px.max(1) as f64),
                        config,
                        ctx.math_renderer,
                    )
                })
                .unwrap_or_else(|| {
                    class_html_title_metrics(
                        ctx.measurer,
                        ctx.text_style,
                        title_text,
                        title_max_width_px,
                    )
                })
        },
        |prepared| prepared.title.metrics,
    );
    let title_width = title_metrics.width.max(1.0);
    let title_height = title_metrics.height.max(ctx.line_height).max(1.0);
    let title_x = -title_width / 2.0;

    let annotation_text = node.annotations.first().map(|annotation| {
        let decoded = decode_entities_minimal_cow(annotation.trim());
        let mut label = String::new();
        label.push('\u{00AB}');
        label.push_str(decoded.as_ref());
        label.push('\u{00BB}');
        label
    });
    let annotation_prepared = prepared_html.and_then(|prepared| prepared.annotation.as_ref());
    let annotation_metrics = annotation_text.as_deref().map(|text| {
        annotation_prepared.map_or_else(
            || {
                let max_width_px = crate::class::class_html_create_text_width_px(
                    text,
                    ctx.measurer,
                    ctx.html_calc_text_style,
                );
                ctx.mermaid_config
                    .and_then(|config| {
                        crate::class::class_math_label_metrics(
                            text,
                            ctx.measurer,
                            ctx.text_style,
                            Some(max_width_px.max(1) as f64),
                            config,
                            ctx.math_renderer,
                        )
                    })
                    .unwrap_or_else(|| {
                        class_html_label_metrics(
                            ctx.measurer,
                            ctx.text_style,
                            text,
                            max_width_px,
                            "",
                        )
                    })
            },
            |prepared| prepared.metrics,
        )
    });
    let annotation_width = annotation_metrics
        .as_ref()
        .map(|metrics| metrics.width.max(1.0))
        .unwrap_or(0.0);
    let annotation_height = annotation_metrics
        .as_ref()
        .map(|metrics| metrics.height.max(ctx.line_height).max(1.0))
        .unwrap_or(0.0);
    let annotation_group_x = if annotation_width > 0.0 {
        -annotation_width / 2.0
    } else {
        0.0
    };
    let annotation_group_y = text_translate_y;
    let title_y = annotation_height + text_translate_y;

    let html_rows_ctx = ClassHtmlNodeRowsContext {
        measurer: ctx.measurer,
        text_style: ctx.text_style,
        html_calc_text_style: ctx.html_calc_text_style,
        line_height: ctx.line_height,
    };
    let members_rows_rendered = measure_class_html_node_rows(
        &node.members,
        class_row_metrics.map(|rows| rows.members.as_slice()),
        prepared_html.map(|prepared| prepared.members.as_slice()),
        &html_rows_ctx,
    );
    let members_group_raw_height = members_rows_rendered.raw_height;
    let members_group_y = annotation_height + title_height + gap * 2.0 + text_translate_y;

    let methods_offset_base = if members_group_raw_height > 0.0 {
        members_group_raw_height + gap * 4.0
    } else {
        gap / 2.0 + gap * 4.0
    };
    let methods_rows_rendered = measure_class_html_node_rows(
        &node.methods,
        class_row_metrics.map(|rows| rows.methods.as_slice()),
        prepared_html.map(|prepared| prepared.methods.as_slice()),
        &html_rows_ctx,
    );
    let methods_group_y = annotation_height + title_height + methods_offset_base + text_translate_y;

    let members_group_width = members_rows_rendered
        .rows
        .iter()
        .fold(0.0_f64, |acc, row| acc.max(row.metrics.width.max(1.0)));
    let methods_group_width = methods_rows_rendered
        .rows
        .iter()
        .fold(0.0_f64, |acc, row| acc.max(row.metrics.width.max(1.0)));
    let mut content_bbox_min_x = 0.0_f64;
    let mut content_bbox_max_x = 0.0_f64;
    for centered_width in [annotation_width, title_width] {
        if centered_width > 0.0 {
            content_bbox_min_x = content_bbox_min_x.min(-centered_width / 2.0);
            content_bbox_max_x = content_bbox_max_x.max(centered_width / 2.0);
        }
    }
    for left_aligned_width in [members_group_width, methods_group_width] {
        if left_aligned_width > 0.0 {
            content_bbox_max_x = content_bbox_max_x.max(left_aligned_width);
        }
    }
    let content_bbox_width = (content_bbox_max_x - content_bbox_min_x).max(0.0);
    let members_x = -content_bbox_width / 2.0;

    let divider_adjust = if render_extra_box { padding / 2.0 } else { 0.0 };
    let divider1_y = (annotation_height - divider_adjust)
        + (title_height - divider_adjust)
        + content_top
        + padding;
    let divider2_y = (annotation_height - divider_adjust)
        + (title_height - divider_adjust)
        + (members_group_raw_height - divider_adjust)
        + content_top
        + padding
        + gap * 2.0;

    if let Some(annotation_text) = annotation_text.as_deref() {
        let annotation_max_width_px = annotation_prepared.map_or_else(
            || {
                crate::class::class_html_create_text_width_px(
                    annotation_text,
                    ctx.measurer,
                    ctx.html_calc_text_style,
                )
            },
            |prepared| prepared.max_width_px,
        );
        let annotation_div_style =
            class_html_div_style(annotation_width.max(1.0), annotation_max_width_px);
        let _ = write!(
            out,
            r#"<g class="annotation-group text" transform="translate({}, {})">"#,
            fmt(annotation_group_x),
            fmt(annotation_group_y)
        );
        render_class_html_node_label_group(
            out,
            &ClassHtmlNodeLabelGroupSpec {
                label_style: "",
                translate_y: -annotation_height / 2.0,
                width: annotation_width.max(1.0),
                height: annotation_height.max(1.0),
                div_style: annotation_div_style.as_str(),
                text: annotation_text,
                include_p: true,
                extra_span_class: Some("markdown-node-label"),
                span_style: Some(ctx.node_style_attr),
                prepared_xhtml: annotation_prepared.map(|prepared| prepared.xhtml.as_str()),
                mermaid_config: ctx.mermaid_config,
                math_renderer: ctx.math_renderer,
            },
        );
        out.push_str("</g>");
    } else {
        let _ = write!(
            out,
            r#"<g class="annotation-group text" transform="translate(0, {})"/>"#,
            fmt(annotation_group_y)
        );
    }

    let title_div_style = class_html_div_style(title_width, title_max_width_px);
    let _ = write!(
        out,
        r#"<g class="label-group text" transform="translate({}, {})">"#,
        fmt(title_x),
        fmt(title_y)
    );
    render_class_html_node_label_group(
        out,
        &ClassHtmlNodeLabelGroupSpec {
            label_style: "font-weight: bolder",
            translate_y: -12.0,
            width: title_width,
            height: title_height,
            div_style: title_div_style.as_str(),
            text: title_text,
            include_p: true,
            extra_span_class: Some("markdown-node-label"),
            span_style: Some(ctx.node_style_attr),
            prepared_xhtml: prepared_html.map(|prepared| prepared.title.xhtml.as_str()),
            mermaid_config: ctx.mermaid_config,
            math_renderer: ctx.math_renderer,
        },
    );
    out.push_str("</g>");

    render_class_html_node_rows_group(
        out,
        "members-group text",
        members_x,
        members_group_y,
        &members_rows_rendered,
        ctx,
    );

    render_class_html_node_rows_group(
        out,
        "methods-group text",
        members_x,
        methods_group_y,
        &methods_rows_rendered,
        ctx,
    );

    if ctx.hide_empty_members_box && members_rows == 0 && methods_rows == 0 {
        ClassNodeRenderStats::default()
    } else {
        render_class_node_dividers(
            ClassNodeRenderState {
                out,
                content_bounds,
            },
            position,
            geometry.left,
            geometry.left + geometry.w,
            [divider1_y, divider2_y],
            &geometry.rough_seed,
            &ClassNodeDividerContext {
                node_style_attr: ctx.node_style_attr,
                node_stroke: ctx.node_stroke,
                node_stroke_width: ctx.node_stroke_width,
                node_stroke_dasharray: ctx.node_stroke_dasharray,
                look: ctx.look,
                use_gradient: ctx.use_gradient,
                timing: ctx.timing,
            },
        )
    }
}

pub(super) fn render_class_svg_node_body(
    state: ClassNodeRenderState<'_>,
    position: ClassNodeRenderPosition,
    node: &ClassSvgNode,
    geometry: ClassNodeBoxGeometry,
    ctx: &ClassSvgNodeBodyContext<'_>,
) -> ClassNodeRenderStats {
    let out = &mut *state.out;
    let content_bounds = &mut *state.content_bounds;
    let padding = ctx.class_padding.max(0.0);
    let gap = padding;
    let text_padding = 3.0;

    let mut title_text = decode_entities_minimal_cow(node.text.trim()).into_owned();
    if title_text.starts_with('\\') {
        title_text = title_text.trim_start_matches('\\').to_string();
    }
    let title_markdown_analysis = crate::class::analyze_class_svg_markdown(&title_text);
    let wrapped_title_text = if title_markdown_analysis.all_runs_normal() {
        wrap_class_svg_text_like_mermaid(
            &title_text,
            ctx.measurer,
            ctx.text_style,
            ctx.wrap_probe_font_size,
            false,
        )
    } else {
        title_text.clone()
    };
    let title_lines =
        crate::text::DeterministicTextMeasurer::normalized_text_lines(&wrapped_title_text);
    let title_has_styled_runs = title_markdown_analysis.has_styled_runs;
    let mut title_metrics = if title_has_styled_runs {
        let title_md = title_lines
            .iter()
            .map(|l| format!("**{l}**"))
            .collect::<Vec<_>>()
            .join("\n");
        crate::text::measure_markdown_with_inline_styles(
            ctx.measurer,
            &title_md,
            ctx.text_style,
            None,
            WrapMode::SvgLike,
        )
    } else {
        let bold_title_style = TextStyle {
            font_family: ctx.text_style.font_family.clone(),
            font_size: ctx.text_style.font_size,
            font_weight: Some("bolder".to_string()),
            font_style: ctx.text_style.font_style.clone(),
        };
        ctx.measurer.measure_wrapped(
            &wrapped_title_text,
            &bold_title_style,
            None,
            WrapMode::SvgLike,
        )
    };
    if !title_has_styled_runs {
        let bold_title_style = TextStyle {
            font_family: ctx.text_style.font_family.clone(),
            font_size: ctx.text_style.font_size,
            font_weight: Some("bolder".to_string()),
            font_style: None,
        };
        let width = title_lines.iter().fold(0.0_f64, |width, line| {
            width.max(
                ctx.measurer
                    .measure_svg_tspan_text_bbox_width_px(line, &bold_title_style),
            )
        });
        if width.is_finite() && width > 0.0 {
            title_metrics.width = width;
        }
    }
    // Annotation group: Mermaid only renders the first annotation.
    let mut annotation_runs: Vec<ClassSvgNodeLabelRun> = Vec::new();
    let mut annotation_rect: Option<Rect> = None;
    let mut annotation_group_height: f64 = 0.0;
    let mut annotation_group_width: f64 = 0.0;
    if let Some(a) = node.annotations.first() {
        let decoded = decode_entities_minimal(a.trim());
        let mut text = format!("\u{00AB}{decoded}\u{00BB}");
        if crate::class::analyze_class_svg_markdown(&text).all_runs_normal() {
            text = wrap_class_svg_text_like_mermaid(
                &text,
                ctx.measurer,
                ctx.text_style,
                ctx.wrap_probe_font_size,
                false,
            );
        }
        let metrics = crate::text::measure_markdown_with_inline_styles(
            ctx.measurer,
            &text,
            ctx.text_style,
            None,
            WrapMode::SvgLike,
        );
        annotation_group_width = metrics.width.max(0.0);
        if let Some(r) = class_svg_label_rect(&metrics, 0.0) {
            annotation_group_height = r.height().max(0.0);
            annotation_rect = Some(r);
        }
        annotation_runs.push(ClassSvgNodeLabelRun {
            text,
            style: String::new(),
            metrics,
            y_offset: 0.0,
        });
    }

    let title_rect = class_svg_label_rect(&title_metrics, 0.0);
    let label_group_height = title_rect.as_ref().map(|r| r.height()).unwrap_or(0.0);
    let label_group_width = title_metrics.width.max(0.0);

    let mut members_runs: Vec<ClassSvgNodeLabelRun> = Vec::new();
    let mut members_rect: Option<Rect> = None;
    {
        let mut y_offset = 0.0;
        for m in &node.members {
            let mut text =
                decode_entities_minimal(crate::class::class_member_display_text(m).as_str());
            if text.starts_with('\\') {
                text = text.trim_start_matches('\\').to_string();
            }
            if crate::class::analyze_class_svg_markdown(&text).all_runs_normal() {
                text = wrap_class_svg_text_like_mermaid(
                    &text,
                    ctx.measurer,
                    ctx.text_style,
                    ctx.wrap_probe_font_size,
                    false,
                );
            }
            let metrics = crate::text::measure_markdown_with_inline_styles(
                ctx.measurer,
                &text,
                ctx.text_style,
                None,
                WrapMode::SvgLike,
            );
            if let Some(r) = class_svg_label_rect(&metrics, y_offset) {
                if let Some(cur) = members_rect.as_mut() {
                    cur.union(r);
                } else {
                    members_rect = Some(r);
                }
            }
            members_runs.push(ClassSvgNodeLabelRun {
                text,
                style: m.css_style.trim().to_string(),
                metrics,
                y_offset,
            });
            y_offset += metrics.height.max(0.0) + text_padding;
        }
    }
    let mut members_group_height = members_rect.as_ref().map(|r| r.height()).unwrap_or(0.0);
    if members_group_height <= 0.0 {
        // Mermaid reserves half a gap when the members group is empty.
        members_group_height = (gap / 2.0).max(0.0);
    }

    let mut methods_runs: Vec<ClassSvgNodeLabelRun> = Vec::new();
    let mut methods_rect: Option<Rect> = None;
    {
        let mut y_offset = 0.0;
        for m in &node.methods {
            let raw = decode_entities_minimal(m.display_text.trim());
            let mut text = raw;
            if text.starts_with('\\') {
                text = text.trim_start_matches('\\').to_string();
            }
            if crate::class::analyze_class_svg_markdown(&text).all_runs_normal() {
                text = wrap_class_svg_text_like_mermaid(
                    &text,
                    ctx.measurer,
                    ctx.text_style,
                    ctx.wrap_probe_font_size,
                    false,
                );
            }
            let metrics = crate::text::measure_markdown_with_inline_styles(
                ctx.measurer,
                &text,
                ctx.text_style,
                None,
                WrapMode::SvgLike,
            );
            if let Some(r) = class_svg_label_rect(&metrics, y_offset) {
                if let Some(cur) = methods_rect.as_mut() {
                    cur.union(r);
                } else {
                    methods_rect = Some(r);
                }
            }
            methods_runs.push(ClassSvgNodeLabelRun {
                text,
                style: m.css_style.trim().to_string(),
                metrics,
                y_offset,
            });
            y_offset += metrics.height.max(0.0) + text_padding;
        }
    }

    // textHelper(...) pre-adjust group transforms.
    let ann_tx = -annotation_group_width / 2.0;
    let ann_ty = 0.0;
    let label_tx = -label_group_width / 2.0;
    let label_ty = annotation_group_height;
    let members_tx = 0.0;
    let members_ty = annotation_group_height + label_group_height + gap * 2.0;
    let methods_tx = 0.0;
    let methods_ty =
        annotation_group_height + label_group_height + (members_group_height + gap * 4.0);

    // Compute bbox returned by textHelper(...) after group transforms.
    let mut bbox_opt: Option<Rect> = None;
    if let Some(mut r) = annotation_rect {
        r.translate(ann_tx, ann_ty);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }
    if let Some(mut r) = title_rect {
        r.translate(label_tx, label_ty);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }
    if let Some(mut r) = members_rect {
        r.translate(members_tx, members_ty);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }
    if let Some(mut r) = methods_rect {
        r.translate(methods_tx, methods_ty);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }
    let bbox = bbox_opt.unwrap_or_else(|| Rect::from_min_max(0.0, 0.0, 0.0, 0.0));
    let bbox_w = bbox.width().max(0.0);
    let mut bbox_h = bbox.height().max(0.0);
    let members_rows = node.members.len();
    let methods_rows = node.methods.len();
    if members_rows == 0 && methods_rows == 0 {
        bbox_h += gap;
    } else if members_rows > 0 && methods_rows == 0 {
        bbox_h += gap * 2.0;
    }
    let x = -bbox_w / 2.0;
    let y = -bbox_h / 2.0;

    let render_extra_box = members_rows == 0 && methods_rows == 0 && !ctx.hide_empty_members_box;
    let adjust_term = if render_extra_box {
        padding
    } else if members_rows == 0 && methods_rows == 0 {
        -padding / 2.0
    } else {
        0.0
    };

    // classBox.ts label adjustment stage.
    let adjust_y = |ty: f64| ty + y + padding - adjust_term - 4.0;
    let adjusted_label_group_x = -label_group_width / 2.0;
    let adjusted_annotation_group_x = -annotation_group_width / 2.0;
    let mut adjusted_text_group_x = x;
    let expected_text_group_x = -geometry.w / 2.0 + padding;
    if expected_text_group_x.is_finite()
        && adjusted_text_group_x.is_finite()
        && (expected_text_group_x - adjusted_text_group_x).abs() > 1e-6
    {
        // Keep the members/methods groups consistent with the already-laid-out node rectangle
        // width (`bbox.width + 2*PADDING` in Mermaid's `classBox.ts`).
        adjusted_text_group_x = expected_text_group_x;
    }

    let ann_new_x = if annotation_runs.is_empty() {
        0.0
    } else {
        adjusted_annotation_group_x
    };
    let ann_new_y = adjust_y(ann_ty);
    render_class_svg_node_runs_group(
        out,
        "annotation-group text",
        ann_new_x,
        ann_new_y,
        &annotation_runs,
    );

    let label_new_y = adjust_y(label_ty);
    render_class_svg_title_group(
        out,
        adjusted_label_group_x,
        label_new_y,
        &title_lines,
        &title_metrics,
    );

    let members_new_y = adjust_y(members_ty);
    render_class_svg_node_runs_group(
        out,
        "members-group text",
        adjusted_text_group_x,
        members_new_y,
        &members_runs,
    );

    let methods_new_y = adjust_y(methods_ty);
    render_class_svg_node_runs_group(
        out,
        "methods-group text",
        adjusted_text_group_x,
        methods_new_y,
        &methods_runs,
    );

    // Dividers (classBox.ts uses group bbox heights).
    if ctx.hide_empty_members_box && members_rows == 0 && methods_rows == 0 {
        ClassNodeRenderStats::default()
    } else {
        let mut ann_h = annotation_group_height;
        let mut label_h = label_group_height;
        let mut members_h = members_rect.as_ref().map(|r| r.height()).unwrap_or(0.0);
        if render_extra_box {
            let shrink = (padding / 2.0).max(0.0);
            ann_h -= shrink;
            label_h -= shrink;
            members_h -= shrink;
        }
        let divider1_y = ann_h + label_h + y + padding;
        let divider2_y = ann_h + label_h + members_h + y + gap * 2.0 + padding;
        render_class_node_dividers(
            ClassNodeRenderState {
                out,
                content_bounds,
            },
            position,
            geometry.left,
            geometry.left + geometry.w,
            [divider1_y, divider2_y],
            &geometry.rough_seed,
            &ClassNodeDividerContext {
                node_style_attr: ctx.node_style_attr,
                node_stroke: ctx.node_stroke,
                node_stroke_width: ctx.node_stroke_width,
                node_stroke_dasharray: ctx.node_stroke_dasharray,
                look: ctx.look,
                use_gradient: ctx.use_gradient,
                timing: ctx.timing,
            },
        )
    }
}

pub(super) fn measure_class_html_node_rows<'a>(
    members: &[ClassMember],
    row_metrics: Option<&[crate::text::TextMetrics]>,
    prepared_rows: Option<&'a [ClassPreparedHtmlLabel]>,
    ctx: &ClassHtmlNodeRowsContext<'_>,
) -> ClassHtmlNodeRows<'a> {
    let mut raw_height = 0.0;
    let mut rows = Vec::with_capacity(members.len());
    for (idx, member) in members.iter().enumerate() {
        let text = crate::class::class_member_create_text_input(member);
        let prepared = prepared_rows.and_then(|rows| rows.get(idx));
        let max_width_px = prepared.map_or_else(
            || {
                crate::class::class_html_create_text_width_px(
                    text.as_str(),
                    ctx.measurer,
                    ctx.html_calc_text_style,
                )
            },
            |prepared| prepared.max_width_px,
        );
        let metrics = prepared
            .map(|prepared| prepared.metrics)
            .unwrap_or_else(|| {
                row_metrics
                    .and_then(|rows| rows.get(idx).copied())
                    .unwrap_or_else(|| {
                        class_html_label_metrics(
                            ctx.measurer,
                            ctx.text_style,
                            text.as_str(),
                            max_width_px,
                            member.css_style.as_str(),
                        )
                    })
            });
        let row_height = metrics.height.max(ctx.line_height).max(1.0);
        let y = raw_height - row_height / 2.0;
        raw_height += row_height;
        rows.push(ClassHtmlNodeRow {
            text,
            row_style: member.css_style.trim().to_string(),
            metrics,
            max_width_px,
            prepared_xhtml: prepared.map(|prepared| prepared.xhtml.as_str()),
            y,
        });
    }

    ClassHtmlNodeRows { rows, raw_height }
}

pub(super) fn render_class_html_node_label_group(
    out: &mut String,
    spec: &ClassHtmlNodeLabelGroupSpec<'_>,
) {
    let _ = write!(
        out,
        r#"<g class="label" style="{}" transform="translate(0,{})"><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}">"#,
        escape_attr_display(spec.label_style),
        fmt(spec.translate_y),
        fmt(spec.width),
        fmt(spec.height),
        escape_attr_display(spec.div_style)
    );
    render_class_html_label(
        out,
        &ClassHtmlLabelSpec {
            span_class: "nodeLabel",
            text: spec.text,
            include_p: spec.include_p,
            extra_span_class: spec.extra_span_class,
            span_style: spec.span_style,
            prepared_xhtml: spec.prepared_xhtml,
            mermaid_config: spec.mermaid_config,
            math_renderer: spec.math_renderer,
        },
    );
    out.push_str("</div></foreignObject></g>");
}

fn render_class_html_node_rows_group(
    out: &mut String,
    group_class: &str,
    group_x: f64,
    group_y: f64,
    rows_rendered: &ClassHtmlNodeRows<'_>,
    ctx: &ClassHtmlNodeBodyContext<'_>,
) {
    if rows_rendered.rows.is_empty() {
        let _ = write!(
            out,
            r#"<g class="{}" transform="translate({}, {})"/>"#,
            group_class,
            fmt(group_x),
            fmt(group_y)
        );
        return;
    }

    let _ = write!(
        out,
        r#"<g class="{}" transform="translate({}, {})">"#,
        group_class,
        fmt(group_x),
        fmt(group_y)
    );
    for row in &rows_rendered.rows {
        let div_style = class_html_div_style(row.metrics.width.max(1.0), row.max_width_px);
        render_class_html_node_label_group(
            out,
            &ClassHtmlNodeLabelGroupSpec {
                label_style: row.row_style.as_str(),
                translate_y: row.y,
                width: row.metrics.width.max(1.0),
                height: row.metrics.height.max(ctx.line_height).max(1.0),
                div_style: div_style.as_str(),
                text: row.text.as_str(),
                include_p: true,
                extra_span_class: Some("markdown-node-label"),
                span_style: Some(ctx.node_style_attr),
                prepared_xhtml: row.prepared_xhtml,
                mermaid_config: ctx.mermaid_config,
                math_renderer: ctx.math_renderer,
            },
        );
    }
    out.push_str("</g>");
}

pub(super) fn render_class_svg_node_runs_group(
    out: &mut String,
    group_class: &str,
    group_x: f64,
    group_y: f64,
    runs: &[ClassSvgNodeLabelRun],
) {
    if runs.is_empty() {
        let _ = write!(
            out,
            r#"<g class="{}" transform="translate({}, {})"/>"#,
            group_class,
            fmt(group_x),
            fmt(group_y)
        );
        return;
    }

    let _ = write!(
        out,
        r#"<g class="{}" transform="translate({}, {})">"#,
        group_class,
        fmt(group_x),
        fmt(group_y)
    );
    for run in runs {
        let t_y = -run.metrics.height.max(0.0) / (2.0 * run.metrics.line_count.max(1) as f64)
            + run.y_offset;
        let _ = write!(
            out,
            r#"<g class="label" style="{}" transform="translate(0,{})"><g><rect class="background" style="stroke: none"/>"#,
            escape_attr_display(run.style.as_str()),
            fmt(t_y)
        );
        write_class_svg_text_markdown(out, run.text.as_str(), true);
        out.push_str("</g></g>");
    }
    out.push_str("</g>");
}

pub(super) fn render_class_svg_title_group(
    out: &mut String,
    group_x: f64,
    group_y: f64,
    title_lines: &[String],
    title_metrics: &crate::text::TextMetrics,
) {
    let _ = write!(
        out,
        r#"<g class="label-group text" transform="translate({}, {})">"#,
        fmt(group_x),
        fmt(group_y)
    );
    let t_y = -title_metrics.height.max(0.0) / (2.0 * title_metrics.line_count.max(1) as f64);
    let _ = write!(
        out,
        r#"<g class="label" style="font-weight: bolder" transform="translate(0,{})"><g><rect class="background" style="stroke: none"/><text y="-10.1" style="">"#,
        fmt(t_y)
    );
    for (idx, line) in title_lines.iter().enumerate() {
        if idx == 0 {
            out.push_str(
                r#"<tspan class="row text-outer-tspan" x="0" y="-0.1em" dy="1.1em" font-weight="">"#,
            );
        } else {
            let y_em = if idx == 1 {
                "1em".to_string()
            } else {
                format!("{:.1}em", 1.0 + (idx as f64 - 1.0) * 1.1)
            };
            let _ = write!(
                out,
                r#"<tspan class="row text-outer-tspan" x="0" y="{}" dy="1.1em" font-weight="">"#,
                y_em
            );
        }
        out.push_str(r#"<tspan font-style="normal" class="text-inner-tspan" font-weight="">"#);
        escape_xml_into(out, line);
        out.push_str("</tspan></tspan>");
    }
    out.push_str("</text></g></g></g>");
}

#[cfg(test)]
mod palette_tests {
    #[test]
    fn class_palette_slots_wrap_and_leave_unassigned_containers_unstamped() {
        let mut attributes = String::new();
        super::write_class_color_slot(&mut attributes, Some(4), 3);
        assert_eq!(attributes, r#" data-color-id="color-1""#);
        attributes.clear();
        super::write_class_color_slot(&mut attributes, None, 3);
        super::write_class_color_slot(&mut attributes, Some(0), 0);
        assert!(attributes.is_empty());
    }
}
