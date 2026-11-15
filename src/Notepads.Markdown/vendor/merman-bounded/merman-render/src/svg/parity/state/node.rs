use super::*;

fn state_leaf_label_width(
    node: &StateSvgNode,
    ctx: &StateRenderCtx<'_>,
    label: &str,
    measured_width: f64,
) -> f64 {
    let width = measured_width.max(0.0);
    if matches!(node.shape.as_str(), "rect" | "note") && !label.is_empty() {
        width.max(node.min_width.unwrap_or(ctx.label_min_width).max(0.0))
    } else {
        width
    }
}
use merman_core::svg_security::{
    MermaidNavigationSecurity, normalize_mermaid_tooltip_attribute, prepare_mermaid_navigation_href,
};

pub(super) fn render_state_node_svg(
    out: &mut String,
    ctx: &StateRenderCtx<'_>,
    node_id: &str,
    origin_x: f64,
    origin_y: f64,
    timing: super::timing::RenderTiming,
    details: &mut StateRenderDetails,
) {
    let Some(node) = ctx.nodes_by_id.get(node_id).copied() else {
        return;
    };
    let Some(ln) = ctx.layout_nodes_by_id.get(node_id).copied() else {
        return;
    };
    if ln.is_cluster {
        return;
    }
    let cx = ln.x - origin_x;
    let cy = ln.y - origin_y;
    let w = ln.width.max(1.0);
    let h = ln.height.max(1.0);

    let node_class = if node.css_classes.trim().is_empty() {
        "node".to_string()
    } else {
        format!("node {}", node.css_classes)
    };
    let node_dom_id = state_scoped_dom_id(ctx, &node.dom_id);
    let data_look = state_data_look(ctx);
    let style_parse_start = timing.start();
    let mut shape_decls: Vec<StateInlineDecl<'_>> = Vec::new();
    let mut text_decls: Vec<StateInlineDecl<'_>> = Vec::new();
    let mut fill_override: Option<&str> = None;
    let mut stroke_override: Option<&str> = None;
    let mut stroke_width_override: Option<f64> = None;
    for raw in node
        .css_compiled_styles
        .iter()
        .chain(node.css_styles.iter())
    {
        let Some(d) = state_parse_inline_decl(raw) else {
            continue;
        };
        if d.key.trim().eq_ignore_ascii_case("fill") {
            fill_override = Some(d.val.trim());
        }
        if d.key.trim().eq_ignore_ascii_case("stroke") {
            stroke_override = Some(d.val.trim());
        }
        if d.key.trim().eq_ignore_ascii_case("stroke-width") {
            let val = d.val.trim().trim_end_matches("px").trim();
            if let Ok(v) = val.parse::<f64>() {
                stroke_width_override = Some(v);
            }
        }
        if state_is_text_style_key(d.key) {
            text_decls.push(d);
        } else {
            shape_decls.push(d);
        }
    }
    let shape_style_attr = state_compact_style_attr(&shape_decls);
    let text_style_attr = state_compact_style_attr(&text_decls);
    let div_style_prefix = state_div_style_prefix(&text_decls);
    if let Some(s) = style_parse_start {
        details.leaf_nodes_style_parse += s.elapsed();
    }

    let small_shadow_attr = if matches!(node.shape.as_str(), "stateStart" | "stateEnd")
        && w < 25.0
        && ctx.theme_defaults.node_shadow
        && data_look != "handDrawn"
    {
        format!(
            r#" style="filter:url(#{}-drop-shadow-small)""#,
            ctx.diagram_id
        )
    } else {
        String::new()
    };

    match node.shape.as_str() {
        "stateStart" => {
            let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
            let _ = write!(
                out,
                r#"<g class="node default" id="{}" data-look="{}" transform="translate({}, {})"><circle class="state-start" r="7" width="14" height="14"{}/></g>"#,
                node_dom_id,
                escape_xml_display(data_look),
                fmt_display(cx),
                fmt_display(cy),
                small_shadow_attr
            );
            drop(_g_emit);
        }
        "stateEnd" => {
            let rough_start = timing.start();
            if timing.is_enabled() {
                details.leaf_roughjs_calls += 2;
                details.leaf_roughjs_unique.insert(StateRoughCacheKey {
                    tag: 1,
                    a: 14.0f64.to_bits(),
                    b: 0,
                    seed: ctx.hand_drawn_seed.seed(),
                });
                details.leaf_roughjs_unique.insert(StateRoughCacheKey {
                    tag: 2,
                    a: 5.0f64.to_bits(),
                    b: 0,
                    seed: ctx.hand_drawn_seed.seed(),
                });
            }
            let outer_key = StateRoughCacheKey {
                tag: 1,
                a: 14.0f64.to_bits(),
                b: 0,
                seed: ctx.hand_drawn_seed.seed(),
            };
            let inner_key = StateRoughCacheKey {
                tag: 2,
                a: 5.0f64.to_bits(),
                b: 0,
                seed: ctx.hand_drawn_seed.seed(),
            };

            let outer_d = ctx.rough_cache.get_or_build_circle(outer_key, || {
                roughjs_circle_path_d(14.0, &ctx.hand_drawn_seed)
                    .unwrap_or_else(|| "M0,0".to_string())
            });
            let inner_d = ctx.rough_cache.get_or_build_circle(inner_key, || {
                roughjs_circle_path_d(5.0, &ctx.hand_drawn_seed)
                    .unwrap_or_else(|| "M0,0".to_string())
            });
            if let Some(s) = rough_start {
                details.leaf_nodes_roughjs += s.elapsed();
            }
            let shape_style_escaped = escape_attr(&shape_style_attr);
            // stateEnd overrides RoughJS options with lineColor for the outer outline and
            // stateBorder (falling back to nodeBorder) for both paints of the inner circle.
            let outer_fill = fill_override.unwrap_or(ctx.theme_defaults.main_bkg.as_str());
            let outer_stroke = ctx.theme_defaults.line_color.as_str();
            let inner_fill = ctx.theme_defaults.state_border.as_str();
            let inner_stroke = inner_fill;
            let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
            let _ = write!(
                out,
                r##"<g class="node default" id="{}" data-look="{}" transform="translate({}, {})"><g class="outer-path"{}><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="2" fill="none" stroke-dasharray="0 0" style="{}"/><g><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="2" fill="none" stroke-dasharray="0 0" style="{}"/></g></g></g>"##,
                node_dom_id.attr(),
                escape_attr(data_look),
                fmt(cx),
                fmt(cy),
                small_shadow_attr,
                outer_d.as_str(),
                escape_attr(outer_fill),
                shape_style_escaped,
                outer_d.as_str(),
                escape_attr(outer_stroke),
                shape_style_escaped,
                inner_d.as_str(),
                escape_attr(inner_fill),
                shape_style_escaped,
                inner_d.as_str(),
                escape_attr(inner_stroke),
                shape_style_escaped,
            );
            drop(_g_emit);
        }
        "fork" | "join" => {
            let rough_start = timing.start();
            let key = StateRoughCacheKey {
                tag: 3,
                a: w.to_bits(),
                b: h.to_bits(),
                seed: ctx.hand_drawn_seed.seed(),
            };
            if timing.is_enabled() {
                details.leaf_roughjs_calls += 1;
                details.leaf_roughjs_unique.insert(key);
            }
            let (fill_d, stroke_d) = ctx.rough_cache.get_or_build_paths(key, || {
                roughjs_paths_for_rect(StateRoughRectSpec {
                    x: -w / 2.0,
                    y: -h / 2.0,
                    w,
                    h,
                    stroke_width: 1.3,
                    randomness: &ctx.hand_drawn_seed,
                })
                .unwrap_or_else(|| ("M0,0".to_string(), "M0,0".to_string()))
            });
            if let Some(s) = rough_start {
                details.leaf_nodes_roughjs += s.elapsed();
            }
            let fill_attr =
                fill_override.unwrap_or(ctx.theme_defaults.special_state_color.as_str());
            let stroke_attr =
                stroke_override.unwrap_or(ctx.theme_defaults.special_state_color.as_str());
            let stroke_width_attr = stroke_width_override.unwrap_or(1.3).max(0.0);
            let shape_style_escaped = escape_attr(&shape_style_attr);
            let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
            let _ = write!(
                out,
                r##"<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"><g><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0" style="{}"/></g></g>"##,
                escape_xml_display(&node_class),
                node_dom_id,
                escape_xml_display(data_look),
                fmt_display(cx),
                fmt_display(cy),
                fill_d.as_str(),
                escape_xml_display(fill_attr),
                shape_style_escaped,
                stroke_d.as_str(),
                escape_xml_display(stroke_attr),
                fmt_display(stroke_width_attr),
                shape_style_escaped
            );
            drop(_g_emit);
        }
        "choice" => {
            let rough_start = timing.start();
            let key = StateRoughCacheKey {
                tag: 4,
                a: w.to_bits(),
                b: h.to_bits(),
                seed: ctx.hand_drawn_seed.seed(),
            };
            if timing.is_enabled() {
                details.leaf_roughjs_calls += 1;
                details.leaf_roughjs_unique.insert(key);
            }
            let (fill_d, stroke_d) = ctx.rough_cache.get_or_build_paths(key, || {
                roughjs_paths_for_svg_path(
                    &mermaid_choice_diamond_path_data(w, h),
                    1.3,
                    "0 0",
                    &ctx.hand_drawn_seed,
                )
                .unwrap_or_else(|| ("M0,0".to_string(), "M0,0".to_string()))
            });
            if let Some(s) = rough_start {
                details.leaf_nodes_roughjs += s.elapsed();
            }

            let fill_attr = fill_override.unwrap_or(ctx.theme_defaults.main_bkg.as_str());
            let stroke_attr = stroke_override.unwrap_or(ctx.theme_defaults.state_border.as_str());
            let stroke_width_attr = stroke_width_override.unwrap_or(1.3).max(0.0);
            let shape_style_escaped = escape_attr(&shape_style_attr);
            let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
            let _ = write!(
                out,
                r##"<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"><g><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0" style="{}"/></g></g>"##,
                escape_xml_display(&node_class),
                node_dom_id,
                escape_xml_display(data_look),
                fmt_display(cx),
                fmt_display(cy),
                fill_d.as_str(),
                escape_xml_display(fill_attr),
                shape_style_escaped,
                stroke_d.as_str(),
                escape_xml_display(stroke_attr),
                fmt_display(stroke_width_attr),
                shape_style_escaped
            );
            drop(_g_emit);
        }
        "note" => {
            let label = state_node_label_text(node);
            let measure_start = timing.start();
            let wrap_mode = if ctx.html_labels {
                WrapMode::HtmlLike
            } else {
                WrapMode::SvgLike
            };
            let measurement = crate::state::measure_state_markdown_label(
                &label,
                ctx.measurer,
                &ctx.text_style,
                Some(ctx.html_label_wrapping_width),
                wrap_mode,
            );
            let metrics = &measurement.metrics;
            if let Some(s) = measure_start {
                details.leaf_nodes_measure += s.elapsed();
            }
            let lw = state_leaf_label_width(node, ctx, &label, metrics.width);
            let lh = metrics.height.max(0.0);
            let rough_start = timing.start();
            let key = StateRoughCacheKey {
                tag: 5,
                a: w.to_bits(),
                b: h.to_bits(),
                seed: ctx.hand_drawn_seed.seed(),
            };
            if timing.is_enabled() {
                details.leaf_roughjs_calls += 1;
                details.leaf_roughjs_unique.insert(key);
            }
            let (fill_d, stroke_d) = ctx.rough_cache.get_or_build_paths(key, || {
                roughjs_paths_for_rect(StateRoughRectSpec {
                    x: -w / 2.0,
                    y: -h / 2.0,
                    w,
                    h,
                    stroke_width: 1.3,
                    randomness: &ctx.hand_drawn_seed,
                })
                .unwrap_or_else(|| ("M0,0".to_string(), "M0,0".to_string()))
            });
            if let Some(s) = rough_start {
                details.leaf_nodes_roughjs += s.elapsed();
            }
            let label_html_start = timing.start();
            let label_dom = if ctx.html_labels {
                state_node_label_html(&label, ctx.text_style.font_size)
            } else {
                state_svg_text_label(&label, false, None)
            };
            if let Some(s) = label_html_start {
                details.leaf_nodes_label_html += s.elapsed();
            }
            let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
            if ctx.html_labels {
                let div_style = if measurement.uses_html_wrapping_table {
                    format!(
                        "display: table; white-space: break-spaces; line-height: 1.5; max-width: {}px; text-align: center; width: {}px;",
                        fmt(ctx.html_label_wrapping_width),
                        fmt(ctx.html_label_wrapping_width),
                    )
                } else if lw > metrics.width {
                    format!(
                        "display: table; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center; width: {}px;",
                        fmt(ctx.html_label_wrapping_width),
                        fmt(lw),
                    )
                } else {
                    format!(
                        "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center;",
                        fmt(ctx.html_label_wrapping_width),
                    )
                };
                let _ = write!(
                    out,
                    r##"<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"><g class="basic label-container outer-path"><path d="{}" stroke="none" stroke-width="0" fill="{}"/><path d="{}" stroke="{}" stroke-width="1.3" fill="none" stroke-dasharray="0 0"/></g><g class="label noteLabel" style="" transform="translate({}, {})"><rect/><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}">{}</div></foreignObject></g></g>"##,
                    escape_xml_display(&node_class),
                    node_dom_id,
                    escape_xml_display(data_look),
                    fmt_display(cx),
                    fmt_display(cy),
                    fill_d.as_str(),
                    escape_xml_display(ctx.theme_defaults.note_bkg.as_str()),
                    stroke_d.as_str(),
                    escape_xml_display(ctx.theme_defaults.note_border.as_str()),
                    fmt_display(-lw / 2.0),
                    fmt_display(-lh / 2.0),
                    fmt_display(lw),
                    fmt_display(lh),
                    div_style,
                    label_dom
                );
            } else {
                let _ = write!(
                    out,
                    r##"<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"><g class="basic label-container outer-path"><path d="{}" stroke="none" stroke-width="0" fill="{}"/><path d="{}" stroke="{}" stroke-width="1.3" fill="none" stroke-dasharray="0 0"/></g><g class="label noteLabel" style="" transform="translate({}, {})"><rect/>{}</g></g>"##,
                    escape_xml_display(&node_class),
                    node_dom_id,
                    escape_xml_display(data_look),
                    fmt_display(cx),
                    fmt_display(cy),
                    fill_d.as_str(),
                    escape_xml_display(ctx.theme_defaults.note_bkg.as_str()),
                    stroke_d.as_str(),
                    escape_xml_display(ctx.theme_defaults.note_border.as_str()),
                    fmt_display(-lw / 2.0),
                    fmt_display(-lh / 2.0),
                    label_dom
                );
            }
            drop(_g_emit);
        }
        "rectWithTitle" => {
            let title = node
                .label
                .as_ref()
                .map(state_value_to_label_text)
                .unwrap_or_else(|| node.id.clone());
            let desc = node
                .description
                .as_ref()
                .map(|v| v.join("\n"))
                .unwrap_or_default();
            let measure_start = timing.start();
            let title_metrics =
                ctx.measurer
                    .measure_wrapped(&title, &ctx.text_style, None, WrapMode::HtmlLike);
            let desc_metrics =
                ctx.measurer
                    .measure_wrapped(&desc, &ctx.text_style, None, WrapMode::HtmlLike);
            if let Some(s) = measure_start {
                details.leaf_nodes_measure += s.elapsed();
            }

            let title_w = title_metrics.width.max(0.0);
            let title_h = title_metrics.height.max(0.0);
            let desc_w = desc_metrics.width.max(0.0);
            let desc_h = desc_metrics.height.max(0.0);
            let padding = node.padding.unwrap_or(ctx.state_padding).max(0.0);
            let geometry = crate::state::RectWithTitleGeometry::from_metrics(
                title_w, title_h, desc_w, desc_h, padding,
            );
            let label_html_start = timing.start();
            let (title_dom, desc_dom) = if ctx.html_labels {
                (
                    state_node_label_plain_html(&title),
                    state_node_label_plain_html(&desc),
                )
            } else {
                (
                    state_svg_text_label(&title, false, None),
                    state_svg_text_label(&desc, false, None),
                )
            };
            if let Some(s) = label_html_start {
                details.leaf_nodes_label_html += s.elapsed();
            }
            let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
            if ctx.html_labels {
                let _ = write!(
                    out,
                    r#"<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"><g><rect class="outer title-state" style="" x="{}" y="{}" width="{}" height="{}"/><line class="divider" x1="{}" x2="{}" y1="{}" y2="{}"/></g><g class="label" style="" transform="translate({}, {})"><foreignObject width="{}" height="{}" transform="translate( {}, 0)"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5;">{}</div></foreignObject><foreignObject width="{}" height="{}" transform="translate( {}, {})"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5;">{}</div></foreignObject></g></g>"#,
                    escape_xml_display(&node_class),
                    node_dom_id,
                    escape_xml_display(data_look),
                    fmt_display(cx),
                    fmt_display(cy),
                    fmt_display(-w / 2.0),
                    fmt_display(-h / 2.0),
                    fmt_display(w),
                    fmt_display(h),
                    fmt_display(-w / 2.0),
                    fmt_display(w / 2.0),
                    fmt_display(geometry.divider_y),
                    fmt_display(geometry.divider_y),
                    fmt_display(geometry.label_x),
                    fmt_display(geometry.label_y),
                    fmt_display(title_w),
                    fmt_display(title_h),
                    fmt_display(geometry.title_x),
                    title_dom,
                    fmt_display(desc_w),
                    fmt_display(desc_h),
                    fmt_display(geometry.description_x),
                    fmt_display(geometry.description_y),
                    desc_dom
                );
            } else {
                let _ = write!(
                    out,
                    r#"<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"><g><rect class="outer title-state" style="" x="{}" y="{}" width="{}" height="{}"/><line class="divider" x1="{}" x2="{}" y1="{}" y2="{}"/></g><g class="label" style="" transform="translate({}, {})"><g transform="translate({}, 0)">{}</g><g transform="translate({}, {})">{}</g></g></g>"#,
                    escape_xml_display(&node_class),
                    node_dom_id,
                    escape_xml_display(data_look),
                    fmt_display(cx),
                    fmt_display(cy),
                    fmt_display(-w / 2.0),
                    fmt_display(-h / 2.0),
                    fmt_display(w),
                    fmt_display(h),
                    fmt_display(-w / 2.0),
                    fmt_display(w / 2.0),
                    fmt_display(geometry.divider_y),
                    fmt_display(geometry.divider_y),
                    fmt_display(geometry.label_x),
                    fmt_display(geometry.label_y),
                    fmt_display(geometry.title_x),
                    title_dom,
                    fmt_display(geometry.description_x),
                    fmt_display(geometry.description_y),
                    desc_dom
                );
            }
            drop(_g_emit);
        }
        _ => {
            let label = state_node_label_text(node);

            fn parse_css_px_f64(v: &str) -> Option<f64> {
                let t = v.trim();
                let t = t.trim_end_matches(';').trim();
                let t = t.trim_end_matches("!important").trim();
                let t = t.trim_end_matches("px").trim();
                t.parse::<f64>().ok()
            }

            let mut measure_style = ctx.text_style.clone();

            for d in &text_decls {
                let k = d.key.trim().to_ascii_lowercase();
                let v = d.val.trim().trim_end_matches(';').trim();
                let v_no_imp = v.trim_end_matches("!important").trim();
                match k.as_str() {
                    "font-weight" if !v_no_imp.is_empty() => {
                        measure_style.font_weight = Some(v_no_imp.to_string());
                    }
                    "font-style" if !v_no_imp.is_empty() => {
                        measure_style.font_style = Some(v_no_imp.to_string());
                    }
                    "font-size" => {
                        if let Some(px) = parse_css_px_f64(v_no_imp)
                            && px.is_finite()
                            && px > 0.0
                        {
                            measure_style.font_size = px;
                        }
                    }
                    "font-family" if !v_no_imp.is_empty() => {
                        measure_style.font_family = Some(v_no_imp.to_string());
                    }
                    _ => {}
                }
            }

            let measure_start = timing.start();
            let wrap_mode = if ctx.html_labels {
                WrapMode::HtmlLike
            } else {
                WrapMode::SvgLike
            };
            let measurement = crate::state::measure_state_markdown_label(
                &label,
                ctx.measurer,
                &measure_style,
                Some(ctx.html_label_wrapping_width),
                wrap_mode,
            );
            let metrics = &measurement.metrics;
            if let Some(s) = measure_start {
                details.leaf_nodes_measure += s.elapsed();
            }

            let lw = state_leaf_label_width(node, ctx, &label, metrics.width);
            let lh = metrics.height.max(0.0);

            let mut link_open = String::new();
            let mut link_close = String::new();
            let mut node_title_attr = String::new();
            if let Some(links) = ctx.links.get(node_id) {
                let mut push_link = |link: &StateSvgLink| {
                    let title_attr = if link.tooltip.is_empty() {
                        String::new()
                    } else {
                        let tooltip = if ctx.security_level_loose {
                            link.tooltip.as_str()
                        } else {
                            normalize_mermaid_tooltip_attribute(&link.tooltip)
                        };
                        format!(r#" title="{}""#, escape_attr(tooltip))
                    };
                    if !title_attr.is_empty() {
                        node_title_attr = title_attr.clone();
                    }

                    if let Some(href) = prepare_mermaid_navigation_href(
                        &link.url,
                        MermaidNavigationSecurity::from_security_level_loose(
                            ctx.security_level_loose,
                        ),
                    ) {
                        let target_attr = if ctx.security_level_loose {
                            r#" target="_blank""#
                        } else {
                            ""
                        };
                        link_open.push_str(&format!(
                            r#"<a xlink:href="{}"{}{}>"#,
                            href.as_serialized_str(),
                            target_attr,
                            title_attr
                        ));
                    } else {
                        link_open.push_str(&format!(r#"<a{}>"#, title_attr));
                    }
                    link_close.push_str("</a>");
                };

                match links {
                    StateSvgLinks::One(link) => push_link(link),
                    StateSvgLinks::Many(links) => {
                        for link in links {
                            push_link(link);
                        }
                    }
                }
            }

            let fill_attr = fill_override.unwrap_or(ctx.theme_defaults.state_bkg.as_str());
            let stroke_attr = stroke_override.unwrap_or(ctx.theme_defaults.state_border.as_str());
            let stroke_width_attr = stroke_width_override
                .unwrap_or(ctx.theme_defaults.rough_stroke_width_value)
                .max(0.0);

            let label_span_style = if text_style_attr.is_empty() {
                None
            } else {
                Some(text_style_attr.as_str())
            };
            let label_html_start = timing.start();
            let label_dom = if ctx.html_labels {
                state_node_label_html_with_style(&label, label_span_style, ctx.text_style.font_size)
            } else {
                state_svg_text_label(&label, false, label_span_style)
            };
            if let Some(s) = label_html_start {
                details.leaf_nodes_label_html += s.elapsed();
            }

            let div_style = if measurement.uses_html_wrapping_table {
                format!(
                    r#"{}display: table; white-space: break-spaces; line-height: 1.5; max-width: {}px; text-align: center; width: {}px;"#,
                    div_style_prefix,
                    fmt(ctx.html_label_wrapping_width),
                    fmt(ctx.html_label_wrapping_width),
                )
            } else if lw > metrics.width {
                format!(
                    r#"{}display: table; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center; width: {}px;"#,
                    div_style_prefix,
                    fmt(ctx.html_label_wrapping_width),
                    fmt(lw),
                )
            } else {
                format!(
                    r#"{}display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center;"#,
                    div_style_prefix,
                    fmt(ctx.html_label_wrapping_width)
                )
            };

            if data_look != "handDrawn" {
                let rect_radius = ctx.theme_defaults.rect_radius;
                let rect_style = escape_xml_display(&shape_style_attr);
                let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
                if ctx.html_labels {
                    let _ = write!(
                        out,
                        r##"{}<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"{}><rect class="basic label-container" style="{}" rx="{}" ry="{}" x="{}" y="{}" width="{}" height="{}"/><g class="label" style="{}" transform="translate({}, {})"><rect/><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}">{}</div></foreignObject></g></g>{}"##,
                        link_open,
                        escape_xml_display(&node_class),
                        node_dom_id,
                        escape_xml_display(data_look),
                        fmt_display(cx),
                        fmt_display(cy),
                        node_title_attr,
                        rect_style,
                        fmt_display(rect_radius),
                        fmt_display(rect_radius),
                        fmt_display(-w / 2.0),
                        fmt_display(-h / 2.0),
                        fmt_display(w),
                        fmt_display(h),
                        escape_xml_display(&text_style_attr),
                        fmt_display(-lw / 2.0),
                        fmt_display(-lh / 2.0),
                        fmt_display(lw),
                        fmt_display(lh),
                        div_style,
                        label_dom,
                        link_close
                    );
                } else {
                    let _ = write!(
                        out,
                        r##"{}<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"{}><rect class="basic label-container" style="{}" rx="{}" ry="{}" x="{}" y="{}" width="{}" height="{}"/><g class="label" style="{}" transform="translate({}, {})"><rect/>{}</g></g>{}"##,
                        link_open,
                        escape_xml_display(&node_class),
                        node_dom_id,
                        escape_xml_display(data_look),
                        fmt_display(cx),
                        fmt_display(cy),
                        node_title_attr,
                        rect_style,
                        fmt_display(rect_radius),
                        fmt_display(rect_radius),
                        fmt_display(-w / 2.0),
                        fmt_display(-h / 2.0),
                        fmt_display(w),
                        fmt_display(h),
                        escape_xml_display(&text_style_attr),
                        fmt_display(-lw / 2.0),
                        fmt_display(-lh / 2.0),
                        label_dom,
                        link_close
                    );
                }
                drop(_g_emit);
                return;
            }

            let rough_start = timing.start();
            let key = StateRoughCacheKey {
                tag: 6,
                a: w.to_bits(),
                b: h.to_bits(),
                seed: ctx.hand_drawn_seed.seed(),
            };
            if timing.is_enabled() {
                details.leaf_roughjs_calls += 1;
                details.leaf_roughjs_unique.insert(key);
            }
            let (fill_d, stroke_d) = ctx.rough_cache.get_or_build_paths(key, || {
                roughjs_paths_for_svg_path(
                    &mermaid_rounded_rect_path_data(w, h),
                    1.3,
                    "0 0",
                    &ctx.hand_drawn_seed,
                )
                .unwrap_or_else(|| ("M0,0".to_string(), "M0,0".to_string()))
            });
            if let Some(s) = rough_start {
                details.leaf_nodes_roughjs += s.elapsed();
            }

            let _g_emit = detail_guard(timing, &mut details.leaf_nodes_emit);
            if ctx.html_labels {
                let _ = write!(
                    out,
                    r##"{}<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"{}><g class="basic label-container outer-path"><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0" style="{}"/></g><g class="label" style="{}" transform="translate({}, {})"><rect/><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}">{}</div></foreignObject></g></g>{}"##,
                    link_open,
                    escape_xml_display(&node_class),
                    node_dom_id,
                    escape_xml_display(data_look),
                    fmt_display(cx),
                    fmt_display(cy),
                    node_title_attr,
                    fill_d.as_str(),
                    escape_xml_display(fill_attr),
                    escape_xml_display(&shape_style_attr),
                    stroke_d.as_str(),
                    escape_xml_display(stroke_attr),
                    fmt_display(stroke_width_attr),
                    escape_xml_display(&shape_style_attr),
                    escape_xml_display(&text_style_attr),
                    fmt_display(-lw / 2.0),
                    fmt_display(-lh / 2.0),
                    fmt_display(lw),
                    fmt_display(lh),
                    div_style,
                    label_dom,
                    link_close
                );
            } else {
                let _ = write!(
                    out,
                    r##"{}<g class="{}" id="{}" data-look="{}" transform="translate({}, {})"{}><g class="basic label-container outer-path"><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0" style="{}"/></g><g class="label" style="{}" transform="translate({}, {})"><rect/>{}</g></g>{}"##,
                    link_open,
                    escape_xml_display(&node_class),
                    node_dom_id,
                    escape_xml_display(data_look),
                    fmt_display(cx),
                    fmt_display(cy),
                    node_title_attr,
                    fill_d.as_str(),
                    escape_xml_display(fill_attr),
                    escape_xml_display(&shape_style_attr),
                    stroke_d.as_str(),
                    escape_xml_display(stroke_attr),
                    fmt_display(stroke_width_attr),
                    escape_xml_display(&shape_style_attr),
                    escape_xml_display(&text_style_attr),
                    fmt_display(-lw / 2.0),
                    fmt_display(-lh / 2.0),
                    label_dom,
                    link_close
                );
            }
            drop(_g_emit);
        }
    }
}
