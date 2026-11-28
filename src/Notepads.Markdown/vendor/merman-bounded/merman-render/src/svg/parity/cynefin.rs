use super::*;
use merman_core::diagrams::cynefin::CynefinDiagramRenderModel;

pub(crate) fn render_cynefin_diagram_svg_model(
    layout: &CynefinDiagramLayout,
    model: &CynefinDiagramRenderModel,
    effective_config: &serde_json::Value,
    diagram_title: Option<&str>,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("cynefin");
    let acc_title = model.acc_title.as_deref().filter(|value| !value.is_empty());
    let acc_descr = model.acc_descr.as_deref().filter(|value| !value.is_empty());
    let aria_labelledby = acc_title.map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = acc_descr.map(|_| format!("chart-desc-{diagram_id}"));
    let root_bounds =
        root_svg::DiagramBounds::from_view_box(0.0, 0.0, layout.total_width, layout.total_height);
    let root_spec = root_svg::RootViewportSpec::mermaid(root_bounds, layout.use_max_width);
    let theme = crate::cynefin::cynefin_theme(effective_config);
    let title = model
        .title
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| diagram_title.filter(|value| !value.is_empty()));
    let seed = crate::cynefin::resolve_seed(layout.seed, diagram_id.semantic_str());

    let mut out = String::new();
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "cynefin");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Cynefin, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    options.checkpoint_emit()?;

    if let Some(title) = acc_title {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{diagram_id}">{}</title>"#,
            escape_xml_display(title)
        );
    }
    if let Some(descr) = acc_descr {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{diagram_id}">{}</desc>"#,
            escape_xml_display(descr)
        );
    }

    let _ = write!(
        &mut out,
        "<style>{}</style>",
        cynefin_css(diagram_id, effective_config, &theme)
    );
    out.push_str("<g/>");
    options.checkpoint_emit()?;
    if let Some(title) = acc_title {
        let _ = write!(&mut out, "<title>{}</title>", escape_xml_display(title));
    }
    if let Some(descr) = acc_descr {
        let _ = write!(&mut out, "<desc>{}</desc>", escape_xml_display(descr));
    }

    let _ = write!(
        &mut out,
        r#"<g transform="translate({}, {})">"#,
        fmt(layout.padding),
        fmt(layout.padding)
    );
    push_backgrounds(&mut out, layout, &theme);
    push_boundaries(&mut out, layout, seed, &theme);
    push_labels(&mut out, layout);
    if layout.show_domain_descriptions {
        push_subtitles(&mut out, layout);
    }
    push_items(&mut out, layout, &theme);
    push_transitions(&mut out, layout, diagram_id, options)?;
    if let Some(title) = title {
        let _ = write!(
            &mut out,
            r#"<text class="cynefinTitle" x="{}" y="{}" text-anchor="middle" dominant-baseline="middle">{}</text>"#,
            fmt(layout.width / 2.0),
            fmt(-layout.padding / 2.0),
            escape_xml_display(title)
        );
    }
    out.push_str("</g>");

    if !layout.transitions.is_empty() {
        let _ = write!(
            &mut out,
            r#"<defs><marker id="cynefin-arrow-{diagram_id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" class="cynefinArrowHead"></path></marker></defs>"#,
        );
    }
    out.push_str("</svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

fn push_backgrounds(
    out: &mut String,
    layout: &CynefinDiagramLayout,
    theme: &crate::cynefin::CynefinTheme,
) {
    out.push_str(r#"<g class="cynefin-backgrounds">"#);
    for domain_name in crate::cynefin::quadrant_domains() {
        let Some(domain) = layout
            .domain_layouts
            .iter()
            .find(|item| item.name == *domain_name)
        else {
            continue;
        };
        let _ = write!(
            out,
            r#"<rect class="cynefinDomain" x="{}" y="{}" width="{}" height="{}" fill="{}" fill-opacity="0.4" stroke="none"></rect>"#,
            fmt(domain.x),
            fmt(domain.y),
            fmt(domain.width),
            fmt(domain.height),
            escape_attr_display(crate::cynefin::domain_fill(theme, domain_name))
        );
    }
    out.push_str("</g>");
}

fn push_boundaries(
    out: &mut String,
    layout: &CynefinDiagramLayout,
    seed: f64,
    theme: &crate::cynefin::CynefinTheme,
) {
    out.push_str(r#"<g class="cynefin-boundaries">"#);
    let fold_path = crate::cynefin::generate_fold_path(
        layout.width,
        layout.height,
        seed,
        Some(layout.boundary_amplitude),
    );
    let horizontal_path = crate::cynefin::generate_horizontal_boundary(
        layout.width,
        layout.height,
        seed + 100.0,
        Some(layout.boundary_amplitude),
    );
    let cliff_path = crate::cynefin::generate_cliff_path(layout.width, layout.height);
    let _ = write!(
        out,
        r#"<path class="cynefinBoundary" d="{}" fill="none"></path><path class="cynefinBoundary" d="{}" fill="none"></path><path class="cynefinCliff" d="{}" fill="none"></path>"#,
        escape_attr_display(&fold_path),
        escape_attr_display(&horizontal_path),
        escape_attr_display(&cliff_path)
    );
    out.push_str("</g>");

    let confusion_path = crate::cynefin::generate_confusion_path(
        layout.width / 2.0,
        layout.height / 2.0,
        layout.width * 0.15,
        layout.height * 0.15,
    );
    let _ = write!(
        out,
        r#"<path class="cynefinConfusion" d="{}" fill="{}" fill-opacity="0.5"></path>"#,
        escape_attr_display(&confusion_path),
        escape_attr_display(&theme.confusion_bg)
    );
}

fn push_labels(out: &mut String, layout: &CynefinDiagramLayout) {
    out.push_str(r#"<g class="cynefin-labels">"#);
    for domain_name in crate::cynefin::quadrant_domains() {
        let Some(domain) = layout
            .domain_layouts
            .iter()
            .find(|item| item.name == *domain_name)
        else {
            continue;
        };
        let y = if layout.show_domain_descriptions {
            domain.cy - 30.0
        } else {
            domain.cy
        };
        let _ = write!(
            out,
            r#"<text class="cynefinDomainLabel" x="{}" y="{}" text-anchor="middle" dominant-baseline="middle">{}</text>"#,
            fmt(domain.cx),
            fmt(y),
            escape_xml_display(crate::cynefin::domain_title(domain_name))
        );
    }
    let y = if layout.show_domain_descriptions {
        layout.height / 2.0 - 10.0
    } else {
        layout.height / 2.0
    };
    let _ = write!(
        out,
        r#"<text class="cynefinDomainLabel" x="{}" y="{}" text-anchor="middle" dominant-baseline="middle">Confusion</text>"#,
        fmt(layout.width / 2.0),
        fmt(y)
    );
    out.push_str("</g>");
}

fn push_subtitles(out: &mut String, layout: &CynefinDiagramLayout) {
    out.push_str(r#"<g class="cynefin-subtitles">"#);
    for domain_name in crate::cynefin::quadrant_domains() {
        let Some(domain) = layout
            .domain_layouts
            .iter()
            .find(|item| item.name == *domain_name)
        else {
            continue;
        };
        let (model, practice) = crate::cynefin::domain_model_and_practice(domain_name);
        let _ = write!(
            out,
            r#"<text class="cynefinSubtitle" x="{}" y="{}" text-anchor="middle" dominant-baseline="middle">{}</text><text class="cynefinSubtitle" x="{}" y="{}" text-anchor="middle" dominant-baseline="middle">{}</text>"#,
            fmt(domain.cx),
            fmt(domain.cy - 10.0),
            escape_xml_display(model),
            fmt(domain.cx),
            fmt(domain.cy + 5.0),
            escape_xml_display(practice)
        );
    }
    let _ = write!(
        out,
        r#"<text class="cynefinSubtitle" x="{}" y="{}" text-anchor="middle" dominant-baseline="middle">Disorder</text>"#,
        fmt(layout.width / 2.0),
        fmt(layout.height / 2.0 + 8.0)
    );
    out.push_str("</g>");
}

fn push_items(
    out: &mut String,
    layout: &CynefinDiagramLayout,
    theme: &crate::cynefin::CynefinTheme,
) {
    out.push_str(r#"<g class="cynefin-items">"#);
    for item in &layout.items {
        let fill = crate::cynefin::domain_fill(theme, &item.domain);
        let rect_class = if item.overflow {
            "cynefinItemOverflow"
        } else {
            "cynefinItem"
        };
        let _ = write!(
            out,
            r#"<g transform="translate({}, {})"><rect class="{}" x="0" y="0" width="{}" height="{}" rx="4" ry="4" fill="{}" fill-opacity="{}"></rect><text class="cynefinItemText" x="{}" y="{}" text-anchor="middle" dominant-baseline="central">{}</text></g>"#,
            fmt(item.x),
            fmt(item.y),
            rect_class,
            fmt(item.width),
            fmt(item.height),
            escape_attr_display(fill),
            if item.overflow { "0.6" } else { "0.95" },
            fmt(item.text_x),
            fmt(item.text_y),
            escape_xml_display(&item.label)
        );
    }
    out.push_str("</g>");
}

fn push_transitions(
    out: &mut String,
    layout: &CynefinDiagramLayout,
    diagram_id: SvgDiagramId<'_>,
    options: &SvgExecution<'_>,
) -> Result<()> {
    if layout.transitions.is_empty() {
        return Ok(());
    }
    out.push_str(r#"<g class="cynefin-arrows">"#);
    for transition in &layout.transitions {
        options.checkpoint_emit()?;
        let d = format!(
            "M{},{} Q{},{} {},{}",
            fmt(transition.x1),
            fmt(transition.y1),
            fmt(transition.cpx),
            fmt(transition.cpy),
            fmt(transition.x2),
            fmt(transition.y2)
        );
        let _ = write!(
            out,
            r#"<path class="cynefinArrowLine" d="{}" fill="none" marker-end="url(#cynefin-arrow-{diagram_id})"></path>"#,
            escape_attr_display(&d),
        );
        if let Some(label) = transition
            .label
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            let _ = write!(
                out,
                r#"<text class="cynefinArrowLabel" x="{}" y="{}" text-anchor="middle" dominant-baseline="auto">{}</text>"#,
                fmt(transition.cpx),
                fmt(transition.cpy - 6.0),
                escape_xml_display(label)
            );
        }
        options.checkpoint_emit()?;
    }
    out.push_str("</g>");
    Ok(())
}

fn cynefin_css<I>(
    diagram_id: I,
    effective_config: &serde_json::Value,
    theme: &crate::cynefin::CynefinTheme,
) -> String
where
    I: Copy + std::fmt::Display,
{
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let mut out = parts.css_prefix;
    let _ = write!(
        &mut out,
        "#{diagram_id} .cynefinDomain{{stroke:none;}}\
#{diagram_id} .cynefinDomainLabel{{font-size:{}px;font-weight:bold;fill:{};}}\
#{diagram_id} .cynefinSubtitle{{font-size:{}px;fill:{};font-style:italic;}}\
#{diagram_id} .cynefinItem{{fill-opacity:0.95;stroke:{};stroke-width:1;}}\
#{diagram_id} .cynefinItemText{{font-size:{}px;fill:{};}}\
#{diagram_id} .cynefinItemOverflow{{fill-opacity:0.6;stroke:{};stroke-width:1;stroke-dasharray:3 2;}}\
#{diagram_id} .cynefinBoundary{{stroke:{};stroke-width:{};stroke-dasharray:6 3;}}\
#{diagram_id} .cynefinCliff{{stroke:{};stroke-width:{};}}\
#{diagram_id} .cynefinConfusion{{stroke:{};stroke-width:1.5;stroke-dasharray:4 2;}}\
#{diagram_id} .cynefinArrowLine{{stroke:{};stroke-width:{};fill:none;}}\
#{diagram_id} .cynefinArrowHead{{fill:{};stroke:none;}}\
#{diagram_id} .cynefinArrowLabel{{font-size:{}px;fill:{};}}\
#{diagram_id} .cynefinTitle{{font-size:{}px;font-weight:bold;fill:{};}}",
        fmt(theme.domain_font_size),
        theme.label_color,
        fmt((theme.item_font_size - 1.0).max(1.0)),
        theme.text_color,
        theme.boundary_color,
        fmt(theme.item_font_size),
        theme.text_color,
        theme.boundary_color,
        theme.boundary_color,
        fmt(theme.boundary_width),
        theme.cliff_color,
        fmt(theme.cliff_width),
        theme.boundary_color,
        theme.arrow_color,
        fmt(theme.arrow_width),
        theme.arrow_color,
        fmt((theme.item_font_size - 1.0).max(1.0)),
        theme.text_color,
        fmt(theme.domain_font_size + 2.0),
        theme.label_color
    );
    out.push_str(&parts.root_rule);
    out
}
