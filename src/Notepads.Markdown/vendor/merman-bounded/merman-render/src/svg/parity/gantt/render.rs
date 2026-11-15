use super::super::*;
use merman_core::diagrams::gantt::{GanttDiagramRenderModel, GanttRenderTask};

// Gantt diagram SVG renderer implementation (split from parity.rs).

fn gantt_scale_time_round(ms: i64, min_ms: i64, max_ms: i64, range: f64) -> f64 {
    if max_ms <= min_ms {
        // D3 scaleTime returns the midpoint of the range for degenerate domains.
        return (range / 2.0).round();
    }
    let t = (ms - min_ms) as f64 / (max_ms - min_ms) as f64;
    (t * range).round()
}

fn fmt_allow_nan(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    fmt_string(v)
}

fn gantt_dom_id(diagram_id: SvgDiagramId<'_>, raw_id: &str) -> String {
    format!("{diagram_id}-{raw_id}")
}

fn gantt_insert_before_width(base: &str, insert: &str) -> String {
    let insert = insert.trim();
    if insert.is_empty() {
        return base.to_string();
    }
    let mut parts: Vec<&str> = base.split_whitespace().collect();
    let insert_parts: Vec<&str> = insert.split_whitespace().collect();
    let idx = parts.iter().position(|p| p.starts_with("width-"));
    match idx {
        Some(i) => {
            for (off, p) in insert_parts.iter().enumerate() {
                parts.insert(i + off, p);
            }
        }
        None => parts.extend(insert_parts),
    }
    parts.join(" ")
}

fn render_gantt_axis_group(
    out: &mut String,
    layout: &crate::model::GanttDiagramLayout,
    ticks: &[crate::model::GanttAxisTickLayout],
    y: f64,
    with_dy: bool,
) {
    let range = (layout.width - layout.left_padding - layout.right_padding).max(1.0);
    // Mermaid renders two possible axis grids:
    // - bottom axis (ticks extend upward, label baseline uses `dy="1em"` at `y="3"`)
    // - optional top axis (ticks extend downward, labels use `dy="0em"` at `y="-3"`)
    let tick_size = if with_dy {
        -layout.height + layout.top_padding + layout.grid_line_start_padding
    } else {
        layout.height - layout.top_padding - layout.grid_line_start_padding
    };

    let _ = write!(
        out,
        r#"<g class="grid" transform="translate({}, {})" fill="none" font-size="10" font-family="sans-serif" text-anchor="middle">"#,
        fmt(layout.left_padding),
        fmt(y)
    );

    let d = format!(
        "M0.5,{}V0.5H{}V{}",
        fmt(tick_size),
        fmt(range + 0.5),
        fmt(tick_size)
    );
    let _ = write!(
        out,
        r#"<path class="domain" stroke="currentColor" d="{}"/>"#,
        escape_attr(&d)
    );

    for t in ticks {
        let tx = (t.x - layout.left_padding) + 0.5;
        let _ = write!(
            out,
            r#"<g class="tick" opacity="1" transform="translate({},0)">"#,
            fmt(tx)
        );
        let _ = write!(
            out,
            r#"<line stroke="currentColor" y2="{}"/>"#,
            fmt(tick_size)
        );
        if with_dy {
            let _ = write!(
                out,
                r##"<text fill="#000" y="3" dy="1em" stroke="none" font-size="10" style="text-anchor: middle;">{}</text>"##,
                escape_xml(&t.label)
            );
        } else {
            let _ = write!(
                out,
                r##"<text fill="#000" y="-3" dy="0em" stroke="none" font-size="10" style="text-anchor: middle;">{}</text>"##,
                escape_xml(&t.label)
            );
        }
        out.push_str("</g>");
    }

    out.push_str("</g>");
}

pub(crate) fn render_gantt_diagram_svg_model(
    layout: &crate::model::GanttDiagramLayout,
    model: &GanttDiagramRenderModel,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("merman");

    let w = layout.width.max(1.0);
    let h = layout.height.max(1.0);

    let acc_title = model
        .acc_title
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let acc_descr = model
        .acc_descr
        .as_deref()
        .map(|s| s.trim_end_matches('\n'))
        .filter(|s| !s.trim().is_empty());

    let mut out = String::new();
    let aria_labelledby = acc_title
        .as_ref()
        .map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = acc_descr
        .as_ref()
        .map(|_| format!("chart-desc-{diagram_id}"));
    let root_bounds = root_svg::DiagramBounds::from_view_box(0.0, 0.0, w, h);
    let root_spec = root_svg::RootViewportSpec::responsive(root_bounds)
        .with_max_width(root_svg::RootMaxWidth::SvgNumber(w));
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "gantt");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom.style_viewbox_order = root_svg::SvgRootStyleViewBoxOrder::ViewBoxThenStyle;
    root_chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Gantt, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    options.checkpoint_emit()?;

    if let Some(title) = acc_title {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{id}">{text}</title>"#,
            id = diagram_id,
            text = escape_xml(title)
        );
    }
    if let Some(descr) = acc_descr {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{id}">{text}</desc>"#,
            id = diagram_id,
            text = escape_xml(descr)
        );
    }

    let css = gantt_css(diagram_id, effective_config);
    let _ = write!(&mut out, r#"<style>{}</style>"#, css);
    out.push_str(r#"<g/>"#);
    options.checkpoint_emit()?;

    let (min_ms, max_ms) = match (
        layout.tasks.iter().map(|t| t.start_ms).min(),
        layout.tasks.iter().map(|t| t.end_ms).max(),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => (0, 0),
    };
    let range = (w - layout.left_padding - layout.right_padding).max(1.0);
    let gap = layout.bar_height + layout.bar_gap;
    // Mermaid's Gantt renderer assigns this custom attribute before the final SVG cleanup.
    // Loose security skips DOMPurify, while every sanitized mode removes the attribute.
    let preserve_task_text_height = effective_config
        .get("securityLevel")
        .and_then(serde_json::Value::as_str)
        == Some("loose");
    let local_time_zone = options.local_time_zone();
    let min_day_start_ms = crate::gantt::start_of_day_ms(min_ms, local_time_zone).unwrap_or(min_ms);
    let min_in_day_offset_ms = (min_ms - min_day_start_ms).max(0);

    // Exclude layer (drawn before the grid in Mermaid).
    if layout.has_excludes_layer {
        if layout.excludes.is_empty() {
            out.push_str("<g/>");
        } else {
            out.push_str("<g>");
            for (i, r) in layout.excludes.iter().enumerate() {
                // Mermaid's gantt exclude rectangles use a slightly unintuitive origin:
                //
                // - `x`/`width` are based on `startOf('day')` / `endOf('day')`
                // - `transform-origin` uses the raw `dayjs(minTime)` time-of-day offset:
                //   `timeScale(d.start) + sidePad + 0.5*(timeScale(d.end)-timeScale(d.start))`
                //
                // This matters when date-only inputs are parsed with a timezone-derived offset
                // (e.g. `YYYY-MM-DD` treated as UTC midnight and shifted when rendered locally).
                let start_day_start_ms = crate::gantt::start_of_day_ms(r.start_ms, local_time_zone)
                    .unwrap_or(r.start_ms);
                let end_day_start_ms =
                    crate::gantt::start_of_day_ms(r.end_ms, local_time_zone).unwrap_or(r.end_ms);
                let start_raw_ms = start_day_start_ms.saturating_add(min_in_day_offset_ms);
                let end_raw_ms = end_day_start_ms.saturating_add(min_in_day_offset_ms);

                let start_x = gantt_scale_time_round(start_raw_ms, min_ms, max_ms, range);
                let end_x = gantt_scale_time_round(end_raw_ms, min_ms, max_ms, range);
                let cx = start_x + layout.left_padding + 0.5 * (end_x - start_x);
                let cy = (i as f64) * gap + 0.5 * h;

                let _ = write!(
                    &mut out,
                    r#"<rect id="{id}" x="{x}" y="{y}" width="{w}" height="{h}" transform-origin="{cx}px {cy}px" class="exclude-range"/>"#,
                    id = escape_attr(&gantt_dom_id(diagram_id, &r.id)),
                    x = fmt(r.x),
                    y = fmt(r.y),
                    w = fmt(r.width),
                    h = fmt(r.height),
                    cx = fmt_allow_nan(cx),
                    cy = fmt_allow_nan(cy),
                );
                options.checkpoint_emit()?;
            }
            out.push_str("</g>");
        }
    }

    let bottom_axis_y = h - layout.top_padding;
    render_gantt_axis_group(&mut out, layout, &layout.bottom_ticks, bottom_axis_y, true);

    if layout.top_axis {
        render_gantt_axis_group(
            &mut out,
            layout,
            &layout.top_ticks,
            layout.top_padding,
            false,
        );
    }

    if layout.rows.is_empty() {
        out.push_str("<g/>");
    } else {
        out.push_str("<g>");
        for r in &layout.rows {
            let _ = write!(
                &mut out,
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" class="{cls}"/>"#,
                x = fmt(r.x),
                y = fmt(r.y),
                w = fmt(r.width),
                h = fmt(r.height),
                cls = escape_attr(&r.class),
            );
        }
        out.push_str("</g>");
    }

    let mut tasks_in_draw_order: Vec<(usize, &crate::model::GanttTaskLayout)> =
        layout.tasks.iter().enumerate().collect();
    tasks_in_draw_order.sort_by(|(ai, a), (bi, b)| a.vert.cmp(&b.vert).then(ai.cmp(bi)));

    let mut semantic_task_by_id: std::collections::HashMap<&str, &GanttRenderTask> =
        std::collections::HashMap::new();
    for t in &model.tasks {
        semantic_task_by_id.insert(t.id.as_str(), t);
    }

    if layout.tasks.is_empty() {
        out.push_str("<g/>");
    } else {
        out.push_str("<g>");

        for (_idx, t) in &tasks_in_draw_order {
            let start_x = gantt_scale_time_round(t.start_ms, min_ms, max_ms, range);
            let end_x = gantt_scale_time_round(t.end_ms, min_ms, max_ms, range);
            let center_x = start_x + layout.left_padding + 0.5 * (end_x - start_x);
            let center_y = (t.order as f64) * gap + layout.top_padding + 0.5 * layout.bar_height;
            let origin = format!(
                "{}px {}px",
                fmt_allow_nan(center_x),
                fmt_allow_nan(center_y)
            );

            let _ = write!(&mut out, r#"<rect"#);
            let _ = write!(
                &mut out,
                r#" id="{}""#,
                escape_attr(&gantt_dom_id(diagram_id, &t.bar.id))
            );
            options.checkpoint_emit()?;
            let _ = write!(
                &mut out,
                r#" rx="{rx}" ry="{ry}" x="{x}" y="{y}" width="{w}" height="{h}" transform-origin="{origin}" class="{cls}"/>"#,
                rx = fmt(t.bar.rx),
                ry = fmt(t.bar.ry),
                x = fmt(t.bar.x),
                y = fmt(t.bar.y),
                w = fmt(t.bar.width),
                h = fmt(t.bar.height),
                origin = escape_attr(&origin),
                cls = escape_attr(&t.bar.class),
            );
        }

        for (_idx, t) in &tasks_in_draw_order {
            let base_class = &t.label.class;
            let mut task_type_class = String::new();
            if let Some(st) = semantic_task_by_id.get(t.id.as_str()) {
                let sec_num = crate::gantt::gantt_section_class_suffix(
                    &st.task_type,
                    &layout.categories,
                    layout.number_section_styles,
                );
                if st.active {
                    if st.crit {
                        task_type_class = format!("activeCritText{sec_num}");
                    } else {
                        task_type_class = format!("activeText{sec_num}");
                    }
                }
                if st.done {
                    if st.crit {
                        if !task_type_class.is_empty() {
                            task_type_class.push(' ');
                        }
                        task_type_class.push_str(&format!("doneCritText{sec_num}"));
                    } else {
                        if !task_type_class.is_empty() {
                            task_type_class.push(' ');
                        }
                        task_type_class.push_str(&format!("doneText{sec_num}"));
                    }
                } else if st.crit {
                    if !task_type_class.is_empty() {
                        task_type_class.push(' ');
                    }
                    task_type_class.push_str(&format!("critText{sec_num}"));
                }

                if st.milestone {
                    if !task_type_class.is_empty() {
                        task_type_class.push(' ');
                    }
                    task_type_class.push_str("milestoneText");
                }

                if st.vert {
                    if !task_type_class.is_empty() {
                        task_type_class.push(' ');
                    }
                    task_type_class.push_str("vertText");
                }
            }

            let class = gantt_insert_before_width(base_class, &task_type_class);
            let _ = write!(
                &mut out,
                r#"<text id="{id}" font-size="{fs}" x="{x}" y="{y}""#,
                id = escape_attr(&gantt_dom_id(diagram_id, &t.label.id)),
                fs = fmt(t.label.font_size),
                x = fmt(t.label.x),
                y = fmt(t.label.y),
            );
            options.checkpoint_emit()?;
            if preserve_task_text_height {
                let _ = write!(&mut out, r#" text-height="{}""#, fmt(layout.bar_height));
            }
            let _ = write!(
                &mut out,
                r#" class="{cls}">{txt}</text>"#,
                cls = escape_attr(&class),
                txt = escape_xml(&t.label.text),
            );
        }

        out.push_str("</g>");
    }

    if layout.section_titles.is_empty() {
        out.push_str("<g/>");
    } else {
        out.push_str("<g>");
        for st in &layout.section_titles {
            let _ = write!(
                &mut out,
                r#"<text dy="{dy}em" x="{x}" y="{y}" font-size="{fs}" class="{cls}">"#,
                dy = fmt(st.dy_em),
                x = fmt(st.x),
                y = fmt(st.y),
                fs = fmt(layout.section_font_size),
                cls = escape_attr(&st.class),
            );
            for (j, line) in st.lines.iter().enumerate() {
                if j == 0 {
                    let _ = write!(
                        &mut out,
                        r#"<tspan alignment-baseline="central" x="{x}">{txt}</tspan>"#,
                        x = fmt(st.x),
                        txt = escape_xml(line)
                    );
                } else {
                    let _ = write!(
                        &mut out,
                        r#"<tspan alignment-baseline="central" x="{x}" dy="1em">{txt}</tspan>"#,
                        x = fmt(st.x),
                        txt = escape_xml(line)
                    );
                }
            }
            out.push_str("</text>");
        }
        out.push_str("</g>");
    }

    if model.today_marker.trim() != "off" {
        let today_x = if layout.tasks.is_empty() {
            f64::NAN
        } else {
            let now_ms = options.unix_ms();
            gantt_scale_time_round(now_ms, min_ms, max_ms, range) + layout.left_padding
        };
        let y1 = layout.title_top_margin;
        let y2 = h - layout.title_top_margin;
        out.push_str(r#"<g class="today">"#);
        let _ = write!(
            &mut out,
            r#"<line x1="{x}" x2="{x}" y1="{y1}" y2="{y2}" class="today""#,
            x = fmt_allow_nan(today_x),
            y1 = fmt(y1),
            y2 = fmt(y2),
        );
        let style_raw = model.today_marker.trim();
        if !style_raw.is_empty() && style_raw != "off" {
            let mut style = style_raw.to_string();
            // Mermaid upstream mmdc output for `todayMarker stroke:#00f;opacity:0.5` ends up as
            // `style="stroke:&00f;opacity:0.5"` (note the `#` → `&`), while comma-separated style
            // strings preserve `#`. Mirror this quirk based on whether the raw marker contains `;`.
            if style.contains(';') {
                style = style.replace('#', "&");
            }
            style = style.replace(',', ";");
            let _ = write!(&mut out, r#" style="{}""#, escape_attr(&style));
        }
        out.push_str("/></g>");
    }

    let title = layout.title.as_deref().unwrap_or_default();
    let _ = write!(
        &mut out,
        r#"<text x="{x}" y="{y}" class="titleText">{txt}</text>"#,
        x = fmt(layout.title_x),
        y = fmt(layout.title_y),
        txt = escape_xml(title),
    );

    out.push_str("</svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}
