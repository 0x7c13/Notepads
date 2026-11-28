//! Flowchart node label renderer.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::label::{flowchart_label_html, flowchart_label_plain_text};
use crate::svg::parity::flowchart::style::FlowchartCompiledStyles;
use crate::svg::parity::flowchart::types::{FlowchartRenderCtx, FlowchartRenderDetails};
use crate::svg::parity::flowchart::util::{
    HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR, OptionalStyleXmlAttr,
};
use crate::svg::parity::flowchart::{
    write_flowchart_svg_source_word_lines, write_flowchart_svg_text_markdown_wrapped,
};
use crate::svg::parity::{escape_xml_display, fmt_display};

pub(in crate::svg::parity::flowchart::render::node) fn render_flowchart_node_label(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    common: &super::FlowchartNodeRenderCommon<'_>,
    label: &super::FlowchartNodeLabelState<'_>,
    compiled_styles: &FlowchartCompiledStyles,
    details: &mut FlowchartRenderDetails,
) {
    render_flowchart_node_label_with_wrapper(
        out,
        ctx,
        common,
        label,
        compiled_styles,
        details,
        true,
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_flowchart_node_label_before_tail(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    common: &super::FlowchartNodeRenderCommon<'_>,
    label: &super::FlowchartNodeLabelState<'_>,
    compiled_styles: &FlowchartCompiledStyles,
    details: &mut FlowchartRenderDetails,
) {
    render_flowchart_node_label_with_wrapper(
        out,
        ctx,
        common,
        label,
        compiled_styles,
        details,
        false,
    );
}

fn render_flowchart_node_label_with_wrapper(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    common: &super::FlowchartNodeRenderCommon<'_>,
    label: &super::FlowchartNodeLabelState<'_>,
    compiled_styles: &FlowchartCompiledStyles,
    details: &mut FlowchartRenderDetails,
    close_node_wrapper: bool,
) {
    let label_base_style = if ctx.node_wrap_mode == crate::text::WrapMode::HtmlLike {
        &ctx.html_label_text_style
    } else {
        &ctx.text_style
    };
    let node_text_style = crate::flowchart::flowchart_effective_text_style_for_node_classes(
        label_base_style,
        ctx.class_defs,
        common.node_classes,
        common.node_styles,
    );
    let prepared_svg_label = (!ctx.node_html_labels && label.label_type != "markdown").then(|| {
        let owner = ctx.svg_label_sidecar.and_then(|sidecar| {
            sidecar.node_owner(common.node_id, ctx.swimlane_direction.is_some())
        });
        crate::flowchart::FlowchartSvgLabelRenderPlan::new(
            ctx.svg_label_sidecar,
            owner,
            label.text,
            ctx.measurer,
            node_text_style.as_ref(),
            Some(ctx.wrapping_width),
            true,
            crate::flowchart::flowchart_node_svg_width_mode(
                label.text,
                label.label_type,
                ctx.node_wrap_mode,
                common.shape,
            ),
        )
    });
    let label_text_plain = prepared_svg_label.as_ref().map_or_else(
        || flowchart_label_plain_text(label.text, label.label_type, ctx.node_html_labels),
        |prepared| prepared.plain_text().to_string(),
    );
    let mut label_dy = label.dy;
    if !ctx.node_html_labels
        && flowchart_node_label_uses_markdown_bbox(label.label_type, label.text)
        && matches!(
            common.shape,
            "doc"
                | "document"
                | "lin-doc"
                | "lined-document"
                | "lin-cyl"
                | "disk"
                | "lined-cylinder"
                | "tag-doc"
                | "tagged-document"
                | "docs"
                | "documents"
                | "st-doc"
                | "stacked-document"
                | "div-rect"
                | "div-proc"
                | "divided-rectangle"
                | "divided-process"
                | "win-pane"
                | "internal-storage"
                | "window-pane"
                | "st-rect"
                | "procs"
                | "processes"
                | "stacked-rectangle"
                | "lin-rect"
                | "lined-rectangle"
                | "lined-process"
                | "lin-proc"
                | "shaded-process"
                | "brace"
                | "brace-l"
                | "comment"
                | "brace-r"
                | "braces"
        )
    {
        // Mermaid shape renderers override `labelHelper(...)`'s default centering using
        // `-bbox.y`. Chromium reports these wrapped SVG markdown labels with a small positive
        // `getBBox().y`, so model that render-time offset here instead of baking literal `-1`s
        // into individual shapes.
        label_dy -= ctx
            .measurer
            .measure_svg_create_text_bbox_y_offset_px(label.text, &node_text_style);
    }
    let metrics = super::helpers::compute_node_label_metrics(
        ctx,
        Some(common.layout_node),
        label.text,
        label.label_type,
        common.node_classes,
        common.node_styles,
    );
    // Only authored FlowDB nodes carry minWidth; subgraph titles retain their own sizing.
    let min_width = ctx
        .nodes_by_id
        .get(common.node_id)
        .filter(|_| !ctx.subgraphs_by_id.contains_key(common.node_id))
        .map_or(0.0, |node| {
            crate::flowchart::flowchart_node_label_min_width(
                label.text,
                node.layout_shape.as_deref(),
                ctx.config,
            )
        });
    let label_group_class = if common.shape == "note" {
        "label noteLabel"
    } else {
        "label"
    };
    if !ctx.node_html_labels {
        let _ = write!(
            out,
            r#"<g class="{}" style="{}" transform="translate({},{})"><rect/><g><rect class="background" style="stroke: none"/>"#,
            label_group_class,
            escape_xml_display(&compiled_styles.label_style),
            fmt_display(label.dx),
            fmt_display(-metrics.height / 2.0 + label_dy)
        );
        if label.label_type == "markdown" {
            write_flowchart_svg_text_markdown_wrapped(
                out,
                label.text,
                true,
                ctx.measurer,
                &node_text_style,
                Some(ctx.wrapping_width),
            );
        } else {
            let wrapped = prepared_svg_label
                .as_ref()
                .expect("non-Markdown SVG labels are prepared before emission")
                .wrapped_lines();
            write_flowchart_svg_source_word_lines(out, &wrapped, true);
        }
        out.push_str("</g></g>");
    } else {
        let label_html = super::helpers::timed_node_label_html(common.timing, details, || {
            flowchart_label_html(label.text, label.label_type, ctx.config, ctx.math_renderer)
        });
        let span_style_attr = OptionalStyleXmlAttr(compiled_styles.label_style.as_str());
        let is_math_html_label = ctx.node_wrap_mode == crate::text::WrapMode::HtmlLike
            && label.text.contains("$$")
            && ctx.math_renderer.is_some();

        let mut label_below_minimum = false;
        let needs_wrap = if ctx.node_wrap_mode == crate::text::WrapMode::HtmlLike {
            if is_math_html_label {
                let natural = if min_width > 0.0 {
                    crate::flowchart::flowchart_label_metrics_for_layout(
                        crate::flowchart::FlowchartLabelMetricsRequest {
                            measurer: ctx.measurer,
                            raw_label: label.text,
                            label_type: label.label_type,
                            style: &node_text_style,
                            max_width_px: Some(ctx.wrapping_width),
                            wrap_mode: ctx.node_wrap_mode,
                            config: ctx.config,
                            math_renderer: ctx.math_renderer,
                        },
                    )
                } else {
                    metrics
                };
                label_below_minimum = natural.width < min_width;
                natural.width >= ctx.wrapping_width - 0.01
            } else {
                let has_inline_style_tags =
                    ctx.node_html_labels && label.label_type != "markdown" && {
                        let lower = label_html.to_ascii_lowercase();
                        crate::text::flowchart_html_has_inline_style_tags(&lower)
                    };

                let raw = if label.label_type == "markdown" {
                    crate::text::measure_markdown_with_inline_styles(
                        ctx.measurer,
                        label.text,
                        &node_text_style,
                        None,
                        ctx.node_wrap_mode,
                    )
                    .width
                } else if has_inline_style_tags {
                    crate::text::measure_html_with_inline_styles(
                        ctx.measurer,
                        &label_html,
                        &node_text_style,
                        None,
                        ctx.node_wrap_mode,
                    )
                    .width
                } else {
                    ctx.measurer
                        .measure_wrapped(
                            &label_text_plain,
                            &node_text_style,
                            None,
                            ctx.node_wrap_mode,
                        )
                        .width
                };
                label_below_minimum = raw < min_width;
                raw > ctx.wrapping_width
            }
        } else {
            false
        };

        let mut div_style = crate::svg::parity::flowchart::style::flowchart_label_div_style_prefix(
            compiled_styles,
            true,
        );
        if needs_wrap {
            let _ = write!(
                &mut div_style,
                "display: table; white-space: break-spaces; line-height: 1.5; max-width: {mw}px; text-align: center; width: {mw}px;",
                mw = fmt_display(ctx.wrapping_width)
            );
        } else if label_below_minimum {
            let _ = write!(
                &mut div_style,
                "display: table; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center; width: {min}px;",
                mw = fmt_display(ctx.wrapping_width),
                min = fmt_display(min_width),
            );
        } else {
            let _ = write!(
                &mut div_style,
                "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;",
                mw = fmt_display(ctx.wrapping_width)
            );
        }
        let _ = write!(
            out,
            r#"<g class="{}" style="{}" transform="translate({},{})"><rect/><foreignObject width="{}" height="{}"{}><div xmlns="http://www.w3.org/1999/xhtml" style="{}"><span class="{}"{}>{}</span></div></foreignObject></g>"#,
            label_group_class,
            escape_xml_display(&compiled_styles.label_style),
            fmt_display(-metrics.width / 2.0 + label.dx),
            fmt_display(-metrics.height / 2.0 + label_dy),
            fmt_display(metrics.width),
            fmt_display(metrics.height),
            HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR,
            escape_xml_display(&div_style),
            super::helpers::flowchart_node_label_span_class(label.label_type),
            span_style_attr,
            label_html
        );
    }
    if close_node_wrapper {
        out.push_str("</g>");
        if common.wrapped_in_a {
            out.push_str("</a>");
        }
    }
}

fn flowchart_node_label_uses_markdown_bbox(label_type: &str, text: &str) -> bool {
    // Mermaid 11.16's labelHelper passes `markdown: true` only for the parser-owned label type.
    label_type == "markdown" && crate::text::mermaid_markdown_to_lines(text, true).len() > 1
}

#[cfg(test)]
mod tests {
    use super::flowchart_node_label_uses_markdown_bbox;

    #[test]
    fn markdown_bbox_selection_uses_the_parser_label_type() {
        assert!(!flowchart_node_label_uses_markdown_bbox(
            "text",
            "ordinary_name\nsecond_line"
        ));
        assert!(!flowchart_node_label_uses_markdown_bbox(
            "text",
            "*unfinished\nsecond"
        ));
        assert!(flowchart_node_label_uses_markdown_bbox(
            "markdown",
            "**first**\n_second_"
        ));
        assert!(!flowchart_node_label_uses_markdown_bbox(
            "markdown", "one line"
        ));
    }
}
