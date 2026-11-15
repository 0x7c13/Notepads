use super::super::theme::JourneyTheme;
use super::super::*;
use crate::journey::{
    JOURNEY_FACE_RADIUS_PX, JOURNEY_TITLE_EXTRA_HEIGHT_PX, JOURNEY_VIEWBOX_TOP_PAD_PX,
    JourneyConfigView,
};
use merman_core::diagrams::journey::JourneyDiagramRenderModel;

fn fmt_task_face_y(v: Option<f64>) -> String {
    v.map(|x| fmt(x).to_string())
        .unwrap_or_else(|| "NaN".to_string())
}

fn journey_css(
    diagram_id: impl Copy + std::fmt::Display,
    effective_config: &serde_json::Value,
    theme: &JourneyTheme,
) -> String {
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let mut out = parts.css_prefix;
    let font = theme.font_family_css.as_str();
    let text_color = theme.text_color.as_str();
    let line_color = theme.line_color.as_str();

    // Mermaid's journey diagram reuses the historical "user-journey" stylesheet, post-processed by
    // Mermaid's CSS pipeline (nesting expansion + id scoping + minification).
    let _ = write!(
        &mut out,
        r#"#{} .label{{font-family:{};color:{};}}"#,
        diagram_id, font, text_color
    );
    let _ = write!(&mut out, r#"#{} .mouth{{stroke:#666;}}"#, diagram_id);
    let _ = write!(
        &mut out,
        r#"#{} line{{stroke:{};}}"#,
        diagram_id, text_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .legend{{fill:{};font-family:{};}}"#,
        diagram_id, text_color, font
    );
    let _ = write!(
        &mut out,
        r#"#{} .label text{{fill:{};}}"#,
        diagram_id, text_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .label{{color:{};}}"#,
        diagram_id, text_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .face{{fill:{};stroke:#999;}}"#,
        diagram_id, theme.face_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .node rect,#{} .node circle,#{} .node ellipse,#{} .node polygon,#{} .node path{{fill:{};stroke:{};stroke-width:1px;}}"#,
        diagram_id,
        diagram_id,
        diagram_id,
        diagram_id,
        diagram_id,
        theme.main_bkg,
        theme.node_border
    );
    let _ = write!(
        &mut out,
        r#"#{} .node .label{{text-align:center;}}"#,
        diagram_id
    );
    let _ = write!(
        &mut out,
        r#"#{} .node.clickable{{cursor:pointer;}}"#,
        diagram_id
    );
    let _ = write!(
        &mut out,
        r#"#{} .arrowheadPath{{fill:{};}}"#,
        diagram_id, theme.arrowhead_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .edgePaths .path{{stroke:{};stroke-width:1.5px;}}"#,
        diagram_id, line_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .flowchart-link{{stroke:{};fill:none;}}"#,
        diagram_id, line_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .edgeLabel{{background-color:{};text-align:center;}}"#,
        diagram_id, theme.edge_label_background
    );
    let _ = write!(
        &mut out,
        r#"#{} .edgeLabel rect{{opacity:0.5;}}"#,
        diagram_id
    );
    let _ = write!(
        &mut out,
        r#"#{} .cluster text{{fill:{};}}"#,
        diagram_id, theme.title_color
    );
    let _ = write!(
        &mut out,
        r#"#{} div.mermaidTooltip{{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:{};font-size:12px;background:{};border:1px solid {};border-radius:2px;pointer-events:none;z-index:100;}}"#,
        diagram_id, font, theme.tertiary_color, theme.border2
    );
    for (i, fill) in theme.fill_types.iter().enumerate() {
        let _ = write!(
            &mut out,
            r#"#{} .task-type-{},#{} .section-type-{}{{fill:{};}}"#,
            diagram_id, i, diagram_id, i, fill
        );
    }
    for (i, fill) in theme.actor_colors.iter().enumerate() {
        if let Some(fill) = fill {
            let _ = write!(
                &mut out,
                r#"#{} .actor-{}{{fill:{};}}"#,
                diagram_id, i, fill
            );
        }
    }
    let _ = write!(
        &mut out,
        r#"#{} .label-icon{{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}}"#,
        diagram_id
    );
    let _ = write!(
        &mut out,
        r#"#{} .node .label-icon path{{fill:currentColor;stroke:revert;stroke-width:revert;}}"#,
        diagram_id
    );

    out.push_str(&parts.root_rule);
    out
}

pub(crate) fn render_journey_diagram_svg_model(
    layout: &crate::model::JourneyDiagramLayout,
    model: &JourneyDiagramRenderModel,
    effective_config: &serde_json::Value,
    diagram_title: Option<&str>,
    _measurer: &dyn TextMeasurer,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("merman");

    let diagram_title = layout
        .title
        .as_deref()
        .or(diagram_title)
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let title_from_meta = layout.title.is_none() && diagram_title.is_some();

    let bounds = layout.bounds.clone().unwrap_or(Bounds {
        min_x: 0.0,
        min_y: -JOURNEY_VIEWBOX_TOP_PAD_PX,
        max_x: 100.0,
        max_y: 100.0,
    });
    let vb_min_x = bounds.min_x;
    let vb_min_y = bounds.min_y;
    let vb_w = (bounds.max_x - bounds.min_x).max(1.0);
    let mut vb_h = (bounds.max_y - bounds.min_y).max(1.0);
    // Mermaid journey titles can also come from YAML frontmatter (`---\ntitle: ...\n---`).
    // When the title is supplied via frontmatter, our semantic/layout layer currently leaves
    // `layout.title` empty. Upstream still accounts for the title when sizing the root viewBox,
    // so mirror that here to keep `parity-root` stable.
    if title_from_meta {
        vb_h += JOURNEY_TITLE_EXTRA_HEIGHT_PX;
    }

    let render_settings = JourneyConfigView::new(effective_config).render_settings();
    let task_font_size = render_settings.task_text_style.font_size;
    let task_font_family = render_settings
        .task_text_style
        .font_family
        .as_deref()
        .unwrap_or("\"Open Sans\", sans-serif");
    let title_font_size = render_settings.title_font_size.as_str();
    let title_font_family = render_settings.title_font_family.as_str();
    let title_color = render_settings.title_color.as_str();

    fn split_html_br_lines(text: &str) -> Vec<String> {
        let b = text.as_bytes();
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut i = 0usize;
        while i < b.len() {
            if b[i] != b'<' {
                let Some(ch) = text.get(i..).and_then(|rest| rest.chars().next()) else {
                    break;
                };
                cur.push(ch);
                i += ch.len_utf8();
                continue;
            }
            if i + 3 >= b.len() {
                cur.push('<');
                i += 1;
                continue;
            }
            if b[i + 1] == b'/' {
                cur.push('<');
                i += 1;
                continue;
            }
            let b1 = b[i + 1];
            let b2 = b[i + 2];
            if !matches!(b1, b'b' | b'B') || !matches!(b2, b'r' | b'R') {
                cur.push('<');
                i += 1;
                continue;
            }
            let mut j = i + 3;
            while j < b.len() && matches!(b[j], b' ' | b'\t' | b'\r' | b'\n') {
                j += 1;
            }
            if j < b.len() && b[j] == b'/' {
                j += 1;
            }
            if j < b.len() && b[j] == b'>' {
                out.push(std::mem::take(&mut cur));
                i = j + 1;
                continue;
            }
            cur.push('<');
            i += 1;
        }
        out.push(cur);
        if out.is_empty() {
            vec!["".to_string()]
        } else {
            out
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct JourneyTextBox {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    #[derive(Debug, Clone, Copy)]
    struct JourneyTextStyle<'a> {
        task_font_size: f64,
        task_font_family: &'a str,
    }

    fn write_text_candidate(
        out: &mut String,
        content: &str,
        class: &str,
        text_box: JourneyTextBox,
        style: JourneyTextStyle<'_>,
        text_fill: &str,
    ) {
        let JourneyTextBox {
            x,
            y,
            width,
            height,
        } = text_box;
        let JourneyTextStyle {
            task_font_size,
            task_font_family,
        } = style;
        let content_esc = escape_xml(content);
        let class_esc = escape_attr(class);
        let font_family_esc = escape_attr(task_font_family);
        let fill_esc = escape_attr(text_fill);
        let cx = x + width / 2.0;
        let cy = y + height / 2.0;

        out.push_str("<switch>");
        let _ = write!(
            out,
            r#"<foreignObject x="{x}" y="{y}" width="{w}" height="{h}">"#,
            x = fmt(x),
            y = fmt(y),
            w = fmt(width),
            h = fmt(height),
        );
        let _ = write!(
            out,
            r#"<div class="{class}" xmlns="http://www.w3.org/1999/xhtml" style="display: table; height: 100%; width: 100%;"><div class="label" style="display: table-cell; text-align: center; vertical-align: middle;">{text}</div></div>"#,
            class = class_esc,
            text = content_esc
        );
        out.push_str("</foreignObject>");

        let lines = split_html_br_lines(content);
        let n = lines.len().max(1) as f64;
        for (i, line) in lines.into_iter().enumerate() {
            let dy = (i as f64) * task_font_size - (task_font_size * (n - 1.0)) / 2.0;
            let _ = write!(
                out,
                r#"<text x="{x}" y="{y}" dominant-baseline="central" alignment-baseline="central" class="{class}" style="text-anchor: middle; font-size: {fs}px; font-family: {ff}; fill: {fill};"><tspan x="{x}" dy="{dy}">{text}</tspan></text>"#,
                x = fmt(cx),
                y = fmt(cy),
                fill = fill_esc,
                class = class_esc,
                fs = fmt(task_font_size),
                ff = font_family_esc,
                dy = fmt(dy),
                text = escape_xml(&line)
            );
        }

        out.push_str("</switch>");
    }

    let mut out = String::new();
    let aria_labelledby = model
        .acc_title
        .as_deref()
        .map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = model
        .acc_descr
        .as_deref()
        .map(|_| format!("chart-desc-{diagram_id}"));

    let svg_height = if vb_min_y < 0.0 {
        vb_h - vb_min_y
    } else {
        vb_h
    };
    let preserve_aspect_ratio: [(&str, &str); 1] = [("preserveAspectRatio", "xMinYMin meet")];
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "journey");
    root_chrome.extra_attrs = &preserve_aspect_ratio;
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom = root_svg::RootDomProfile {
        fixed_height_placement: root_svg::SvgRootFixedHeightPlacement::AfterXmlns,
        fixed_style_placement: root_svg::RootStylePlacement::Tail,
        responsive_height_placement: root_svg::RootResponsiveHeightPlacement::AfterExtraAttrs,
        trailing_newline: false,
        ..root_svg::RootDomProfile::default()
    };
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Journey, diagram_id)
            .write_open(
                &mut out,
                root_svg::RootViewportSpec::mermaid(
                    root_svg::DiagramBounds::from_view_box(vb_min_x, vb_min_y, vb_w, vb_h),
                    layout.use_max_width,
                )
                .with_mermaid_responsive_height(layout.use_max_width, svg_height)
                .with_fixed_size(layout.width, svg_height)
                .with_max_width(root_svg::RootMaxWidth::CssSixSignificant(layout.width)),
                root_chrome,
            )?;
    options.checkpoint_emit()?;

    if let Some(title) = model.acc_title.as_deref() {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{id}">{text}</title>"#,
            id = diagram_id,
            text = escape_xml(title)
        );
    }
    if let Some(desc) = model.acc_descr.as_deref() {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{id}">{text}</desc>"#,
            id = diagram_id,
            text = escape_xml(desc)
        );
    }

    let theme = PresentationTheme::new(effective_config).journey();
    let css = journey_css(diagram_id, effective_config, &theme);
    let _ = write!(&mut out, r#"<style>{}</style>"#, css);
    out.push_str(r#"<g/>"#);
    let arrowhead_id = scoped_svg_id(diagram_id, "arrowhead");
    let arrowhead_url = scoped_svg_url(diagram_id, "arrowhead");
    let _ = write!(
        &mut out,
        r#"<defs><marker id="{}" refX="5" refY="2" markerWidth="6" markerHeight="4" orient="auto"><path d="M 0,0 V 4 L6,2 Z"/></marker></defs>"#,
        escape_attr_display(arrowhead_id)
    );
    options.checkpoint_emit()?;

    for item in &layout.actor_legend {
        let _ = write!(
            &mut out,
            r##"<circle cx="{cx}" cy="{cy}" class="actor-{pos}" fill="{fill}" stroke="#000" r="{r}"/>"##,
            cx = fmt(item.circle_cx),
            cy = fmt(item.circle_cy),
            pos = item.pos,
            fill = escape_attr(&item.color),
            r = fmt(item.circle_r),
        );
        for line in &item.label_lines {
            let _ = write!(
                &mut out,
                r#"<text x="{x}" y="{y}" class="legend"><tspan x="{tx}">{text}</tspan></text>"#,
                x = fmt(line.x),
                y = fmt(line.y),
                tx = fmt(line.tspan_x),
                text = escape_xml(&line.text),
            );
        }
    }

    let mut section_iter = layout.sections.iter();
    let mut last_section: Option<&str> = None;
    for task in &layout.tasks {
        if last_section != Some(task.section.as_str()) {
            let Some(section) = section_iter.next() else {
                break;
            };
            let section_class = format!("journey-section section-type-{}", section.num);
            let _ = write!(
                &mut out,
                r##"<g><rect x="{x}" y="{y}" fill="{fill}" stroke="#666" width="{w}" height="{h}" rx="3" ry="3" class="{class}"/>"##,
                x = fmt(section.x),
                y = fmt(section.y),
                fill = escape_attr(&section.fill),
                w = fmt(section.width),
                h = fmt(section.height),
                class = escape_attr(&section_class),
            );
            write_text_candidate(
                &mut out,
                &section.section,
                &section_class,
                JourneyTextBox {
                    x: section.x,
                    y: section.y,
                    width: section.width,
                    height: section.height,
                },
                JourneyTextStyle {
                    task_font_size,
                    task_font_family,
                },
                &theme.text_color,
            );
            out.push_str("</g>");
        }

        last_section = Some(task.section.as_str());

        let _ = write!(
            &mut out,
            r##"<g><line id="{id}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" class="task-line" stroke-width="1px" stroke-dasharray="4 2" stroke="#666"/>"##,
            id = escape_attr_display(scoped_svg_id(diagram_id, &task.line_id)),
            x1 = fmt(task.line_x1),
            y1 = fmt(task.line_y1),
            x2 = fmt(task.line_x2),
            y2 = fmt(task.line_y2),
        );
        options.checkpoint_emit()?;

        let _ = write!(
            &mut out,
            r#"<circle cx="{cx}" cy="{cy}" class="face" r="{r}" stroke-width="2" overflow="visible"/>"#,
            cx = fmt(task.face_cx),
            cy = fmt_task_face_y(task.face_cy),
            r = fmt(JOURNEY_FACE_RADIUS_PX),
        );
        out.push_str("<g>");
        let eye_dx = JOURNEY_FACE_RADIUS_PX / 3.0;
        let eye_r = 1.5;
        let _ = write!(
            &mut out,
            r##"<circle cx="{cx}" cy="{cy}" r="{r}" stroke-width="2" fill="#666" stroke="#666"/>"##,
            cx = fmt(task.face_cx - eye_dx),
            cy = fmt_task_face_y(task.face_cy.map(|v| v - eye_dx)),
            r = fmt(eye_r),
        );
        let _ = write!(
            &mut out,
            r##"<circle cx="{cx}" cy="{cy}" r="{r}" stroke-width="2" fill="#666" stroke="#666"/>"##,
            cx = fmt(task.face_cx + eye_dx),
            cy = fmt_task_face_y(task.face_cy.map(|v| v - eye_dx)),
            r = fmt(eye_r),
        );

        match task.mouth {
            crate::model::JourneyMouthKind::Smile => {
                let _ = write!(
                    &mut out,
                    r#"<path class="mouth" d="M7.5,0A7.5,7.5,0,1,1,-7.5,0L-6.818,0A6.818,6.818,0,1,0,6.818,0Z" transform="translate({x},{y})"/>"#,
                    x = fmt(task.face_cx),
                    y = fmt_task_face_y(task.face_cy.map(|v| v + 2.0)),
                );
            }
            crate::model::JourneyMouthKind::Sad => {
                let _ = write!(
                    &mut out,
                    r#"<path class="mouth" d="M-7.5,0A7.5,7.5,0,1,1,7.5,0L6.818,0A6.818,6.818,0,1,0,-6.818,0Z" transform="translate({x},{y})"/>"#,
                    x = fmt(task.face_cx),
                    y = fmt_task_face_y(task.face_cy.map(|v| v + 7.0)),
                );
            }
            crate::model::JourneyMouthKind::Ambivalent => {
                let _ = write!(
                    &mut out,
                    r##"<line class="mouth" stroke="#666" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke-width="1px"/>"##,
                    x1 = fmt(task.face_cx - 5.0),
                    y1 = fmt_task_face_y(task.face_cy.map(|v| v + 7.0)),
                    x2 = fmt(task.face_cx + 5.0),
                    y2 = fmt_task_face_y(task.face_cy.map(|v| v + 7.0)),
                );
            }
        }

        out.push_str("</g>");

        let _ = write!(
            &mut out,
            r##"<rect x="{x}" y="{y}" fill="{fill}" stroke="#666" width="{w}" height="{h}" rx="3" ry="3" class="task task-type-{num}"/>"##,
            x = fmt(task.x),
            y = fmt(task.y),
            fill = escape_attr(&task.fill),
            w = fmt(task.width),
            h = fmt(task.height),
            num = task.num,
        );

        for c in &task.actor_circles {
            let _ = write!(
                &mut out,
                r##"<circle cx="{cx}" cy="{cy}" class="actor-{pos}" fill="{fill}" stroke="#000" r="{r}"><title>{title}</title></circle>"##,
                cx = fmt(c.cx),
                cy = fmt(c.cy),
                pos = c.pos,
                fill = escape_attr(&c.color),
                r = fmt(c.r),
                title = escape_xml(&c.actor),
            );
        }

        write_text_candidate(
            &mut out,
            &task.task,
            "task",
            JourneyTextBox {
                x: task.x,
                y: task.y,
                width: task.width,
                height: task.height,
            },
            JourneyTextStyle {
                task_font_size,
                task_font_family,
            },
            &theme.text_color,
        );

        out.push_str("</g>");
    }

    if let Some(title) = diagram_title {
        let _ = write!(
            &mut out,
            r#"<text x="{x}" font-size="{fs}" font-weight="bold" y="{y}" fill="{fill}" font-family="{ff}">{text}</text>"#,
            x = fmt(layout.title_x),
            fs = escape_attr(title_font_size),
            y = fmt(layout.title_y),
            fill = escape_attr(title_color),
            ff = escape_attr(title_font_family),
            text = escape_xml(title),
        );
    }

    let _ = write!(
        &mut out,
        r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke-width="4" stroke="black" marker-end="{marker_end}"/>"#,
        x1 = fmt(layout.activity_line.x1),
        y1 = fmt(layout.activity_line.y1),
        x2 = fmt(layout.activity_line.x2),
        y2 = fmt(layout.activity_line.y2),
        marker_end = escape_attr_display(arrowhead_url),
    );

    out.push_str("</svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Bounds, JourneyLineLayout, JourneyMouthKind, JourneySectionLayout, JourneyTaskLayout,
    };
    use crate::text::DeterministicTextMeasurer;
    use merman_core::diagrams::journey::JourneyDiagramRenderModel;

    #[test]
    fn journey_css_honors_mermaid_11_15_theme_options() {
        let cfg = serde_json::json!({
            "themeVariables": {
                "fontFamily": "\"ibm plex sans\", arial, sans-serif",
                "textColor": "#101010",
                "lineColor": "#202020",
                "faceColor": "#303030",
                "mainBkg": "#404040",
                "nodeBorder": "#505050",
                "arrowheadColor": "#606060",
                "edgeLabelBackground": "#707070",
                "titleColor": "#808080",
                "tertiaryColor": "#909090",
                "border2": "#a0a0a0",
                "fillType0": "#b0b0b0",
                "fillType1": "#c0c0c0",
                "actor0": "#d0d0d0",
                "actor1": "#e0e0e0"
            }
        });

        let theme = PresentationTheme::new(&cfg).journey();
        let css = journey_css("journey", &cfg, &theme);

        assert!(css.contains(r#"#journey line{stroke:#101010;}"#));
        assert!(css.contains(r#"#journey .face{fill:#303030;stroke:#999;}"#));
        assert!(css.contains(r#"#journey .node rect,#journey .node circle,#journey .node ellipse,#journey .node polygon,#journey .node path{fill:#404040;stroke:#505050;stroke-width:1px;}"#));
        assert!(css.contains(r#"#journey .arrowheadPath{fill:#606060;}"#));
        assert!(css.contains(r#"#journey .edgePaths .path{stroke:#202020;stroke-width:1.5px;}"#));
        assert!(
            css.contains(r#"#journey .edgeLabel{background-color:#707070;text-align:center;}"#)
        );
        assert!(css.contains(r#"#journey .cluster text{fill:#808080;}"#));
        assert!(css.contains(r#"background:#909090;border:1px solid #a0a0a0;"#));
        assert!(css.contains(r#"#journey .task-type-0,#journey .section-type-0{fill:#b0b0b0;}"#));
        assert!(css.contains(r#"#journey .task-type-1,#journey .section-type-1{fill:#c0c0c0;}"#));
        assert!(css.contains(r#"#journey .actor-0{fill:#d0d0d0;}"#));
        assert!(css.contains(r#"#journey .actor-1{fill:#e0e0e0;}"#));
        assert!(css.contains(r#"#journey .flowchart-link{stroke:#202020;fill:none;}"#));
    }

    #[test]
    fn journey_rect_fills_use_layout_section_fills_not_theme_fill_types() {
        let cfg = serde_json::json!({
            "themeVariables": {
                "fillType0": "#b0b0b0"
            }
        });
        let layout = crate::model::JourneyDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -25.0,
                max_x: 500.0,
                max_y: 515.0,
            }),
            left_margin: 150.0,
            max_actor_label_width: 0.0,
            width: 500.0,
            height: 470.0,
            svg_height: 565.0,
            use_max_width: true,
            title: Some("Font size precedence should be deterministic".to_string()),
            title_x: 150.0,
            title_y: 25.0,
            actor_legend: Vec::new(),
            sections: vec![JourneySectionLayout {
                section: "A".to_string(),
                num: 0,
                x: 150.0,
                y: 50.0,
                width: 150.0,
                height: 50.0,
                fill: "#191970".to_string(),
                task_count: 1,
            }],
            tasks: vec![JourneyTaskLayout {
                index: 0,
                section: "A".to_string(),
                task: "Hello".to_string(),
                score: 5,
                x: 150.0,
                y: 110.0,
                width: 150.0,
                height: 50.0,
                fill: "#191970".to_string(),
                num: 0,
                people: Vec::new(),
                actor_circles: Vec::new(),
                line_id: "task0".to_string(),
                line_x1: 225.0,
                line_y1: 110.0,
                line_x2: 225.0,
                line_y2: 450.0,
                face_cx: 225.0,
                face_cy: Some(300.0),
                mouth: JourneyMouthKind::Smile,
            }],
            activity_line: JourneyLineLayout {
                x1: 150.0,
                y1: 200.0,
                x2: 346.0,
                y2: 200.0,
            },
        };
        let options = SvgRenderOptions {
            diagram_id: Some("journey".to_string()),
            ..Default::default()
        };

        let svg = with_test_svg_execution(&options, |options| {
            render_journey_diagram_svg_model(
                &layout,
                &JourneyDiagramRenderModel::default(),
                &cfg,
                None,
                &DeterministicTextMeasurer::default(),
                options,
            )
        })
        .unwrap();

        assert!(svg.contains(r#"#journey .task-type-0,#journey .section-type-0{fill:#b0b0b0;}"#));
        assert!(svg.contains(r##"<rect x="150" y="50" fill="#191970" stroke="#666" width="150" height="50" rx="3" ry="3" class="journey-section section-type-0"/>"##));
        assert!(svg.contains(r##"<rect x="150" y="110" fill="#191970" stroke="#666" width="150" height="50" rx="3" ry="3" class="task task-type-0"/>"##));
        assert!(!svg.contains(r##"<rect x="150" y="50" fill="#b0b0b0""##));
        assert!(!svg.contains(r##"<rect x="150" y="110" fill="#b0b0b0""##));
    }

    #[test]
    fn journey_root_honors_disabled_max_width() {
        let layout = crate::model::JourneyDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -25.0,
                max_x: 320.0,
                max_y: 175.0,
            }),
            left_margin: 150.0,
            max_actor_label_width: 0.0,
            width: 320.0,
            height: 200.0,
            svg_height: 225.0,
            use_max_width: false,
            title: None,
            title_x: 150.0,
            title_y: 25.0,
            actor_legend: Vec::new(),
            sections: Vec::new(),
            tasks: Vec::new(),
            activity_line: JourneyLineLayout {
                x1: 150.0,
                y1: 200.0,
                x2: 166.0,
                y2: 200.0,
            },
        };
        let options = SvgRenderOptions {
            diagram_id: Some("journeyFixed".to_string()),
            ..Default::default()
        };

        let svg = with_test_svg_execution(&options, |options| {
            render_journey_diagram_svg_model(
                &layout,
                &JourneyDiagramRenderModel::default(),
                &serde_json::json!({}),
                None,
                &DeterministicTextMeasurer::default(),
                options,
            )
        })
        .unwrap();
        let root_open = svg.split_once('>').expect("root svg open tag").0;

        assert!(root_open.contains(r#"width="320""#), "{root_open}");
        assert!(root_open.contains(r#"height="225""#), "{root_open}");
        assert!(
            root_open.contains(r#"viewBox="0 -25 320 200""#),
            "{root_open}"
        );
        assert!(
            root_open.contains(r#"preserveAspectRatio="xMinYMin meet""#),
            "{root_open}"
        );
        assert!(!root_open.contains("style="), "{root_open}");
        assert!(!root_open.contains(r#"width="100%""#), "{root_open}");
        assert!(!root_open.contains("max-width"), "{root_open}");
    }
}
