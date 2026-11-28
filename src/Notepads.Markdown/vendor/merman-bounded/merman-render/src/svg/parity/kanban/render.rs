use super::super::*;
use crate::kanban::{
    KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX, KANBAN_SECTION_PADDING_PX, KanbanPreparedArtifact,
    KanbanPreparedLabelGeometry,
};

fn kanban_css(
    diagram_id: impl Copy + std::fmt::Display,
    effective_config: &serde_json::Value,
) -> Result<String> {
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let theme = PresentationTheme::new(effective_config).kanban()?;
    let mut out = parts.css_prefix;
    let root_rule = parts.root_rule;

    let _ = write!(&mut out, r#"#{} .edge{{stroke-width:3;}}"#, diagram_id);
    for (i, section_theme) in theme.sections.iter().enumerate() {
        let section = i as i64 - 1;
        let sw = 17_i64 - 3_i64 * (i as i64);
        let _ = write!(
            &mut out,
            r#"#{} .section-{} rect,#{} .section-{} path,#{} .section-{} circle,#{} .section-{} polygon,#{} .section-{} path{{fill:{};stroke:{};}}#{} .section-{} text{{fill:{};}}#{} .node-icon-{}{{font-size:40px;color:{};}}#{} .section-edge-{}{{stroke:{};}}#{} .edge-depth-{}{{stroke-width:{};}}#{} .section-{} line{{stroke:{};stroke-width:3;}}#{} .disabled,#{} .disabled circle,#{} .disabled text{{fill:lightgray;}}#{} .disabled text{{fill:#efefef;}}#{} .node rect,#{} .node circle,#{} .node ellipse,#{} .node polygon,#{} .node path{{fill:{};stroke:{};stroke-width:1px;}}#{} .kanban-ticket-link{{fill:{};stroke:{};text-decoration:underline;}}"#,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            section_theme.section_fill,
            section_theme.section_fill,
            diagram_id,
            section,
            section_theme.c_scale_label,
            diagram_id,
            section,
            section_theme.c_scale_label,
            diagram_id,
            section,
            section_theme.c_scale,
            diagram_id,
            section,
            sw,
            diagram_id,
            section,
            section_theme.c_scale_inv,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            theme.background,
            theme.node_border,
            diagram_id,
            theme.background,
            theme.node_border
        );
    }

    let _ = write!(
        &mut out,
        r#"#{} .section-root rect,#{} .section-root path,#{} .section-root circle,#{} .section-root polygon{{fill:{};}}#{} .section-root text{{fill:{};}}#{} .icon-container{{height:100%;display:flex;justify-content:center;align-items:center;}}#{} .edge{{fill:none;}}#{} .cluster-label,#{} .label{{color:{};fill:{};}}#{} .kanban-label{{dy:1em;alignment-baseline:middle;text-anchor:middle;dominant-baseline:middle;text-align:center;}}#{} .label-icon{{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}}#{} .node .label-icon path{{fill:currentColor;stroke:revert;stroke-width:revert;}}"#,
        diagram_id,
        diagram_id,
        diagram_id,
        diagram_id,
        theme.root_fill,
        diagram_id,
        theme.root_label,
        diagram_id,
        diagram_id,
        diagram_id,
        diagram_id,
        theme.text_color,
        theme.text_color,
        diagram_id,
        diagram_id,
        diagram_id
    );
    out.push_str(&root_rule);
    Ok(out)
}

fn kanban_dom_id(diagram_id: SvgDiagramId<'_>, raw_id: &str) -> String {
    format!("{diagram_id}-{raw_id}")
}

fn measure_kanban_plain_label(
    text_measurer: &dyn crate::text::TextMeasurer,
    text: Option<&str>,
    style: &crate::text::TextStyle,
) -> KanbanPreparedLabelGeometry {
    let Some(text) = text.filter(|text| !text.is_empty()) else {
        return KanbanPreparedLabelGeometry::empty();
    };
    let metrics = text_measurer.measure_wrapped(text, style, None, crate::text::WrapMode::HtmlLike);
    KanbanPreparedLabelGeometry {
        content_height: metrics.height,
        foreign_object_width: metrics.width,
        wrapped: false,
    }
}

struct KanbanLabelRenderContext {
    max_width: f64,
    min_height: f64,
}

struct KanbanLabelGroup<'a> {
    position: (f64, f64),
    text: Option<&'a str>,
    html: Option<&'a str>,
    geometry: KanbanPreparedLabelGeometry,
    div_class: Option<&'a str>,
    wrap_title: bool,
}

fn write_kanban_label_group(
    out: &mut String,
    context: &KanbanLabelRenderContext,
    group: KanbanLabelGroup<'_>,
) {
    let KanbanLabelGroup {
        position: (x, y),
        text,
        html,
        geometry,
        div_class,
        wrap_title,
    } = group;
    let max_width = context.max_width;
    let div_style_overrides = match text {
        Some(text) if !text.is_empty() && wrap_title && geometry.wrapped => Some(format!(
            "display: table; white-space: break-spaces; line-height: 1.5; max-width: {width}px; width: {width}px;",
            width = fmt(max_width),
        )),
        Some(text) if !text.is_empty() => Some(format!(
            "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {width}px;",
            width = fmt(max_width),
        )),
        _ => None,
    };
    let class_attr = div_class
        .map(|class| format!(r#" class="{}""#, escape_attr(class)))
        .unwrap_or_default();
    let div_style = if let Some(overrides) = div_style_overrides {
        format!("text-align: center; {overrides}")
    } else {
        format!(
            "text-align: center; display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {width}px;",
            width = fmt(max_width),
        )
    };
    let span_class = if wrap_title {
        "nodeLabel markdown-node-label"
    } else {
        "nodeLabel"
    };
    let foreign_object_height = if text.is_some_and(|text| !text.is_empty()) {
        geometry.content_height.max(context.min_height)
    } else {
        0.0
    };
    let _ = write!(
        out,
        r##"<g class="label" style="text-align:left !important" transform="translate({x}, {y})"><rect/><foreignObject width="{width}" height="{height}"><div style="{div_style}" xmlns="http://www.w3.org/1999/xhtml"{class_attr}><span style="text-align:left !important" class="{span_class}">"##,
        x = fmt(x),
        y = fmt(y),
        width = fmt(geometry.foreign_object_width),
        height = fmt(foreign_object_height),
        div_style = escape_attr(&div_style),
        class_attr = class_attr,
        span_class = span_class,
    );
    if let Some(html) = html.filter(|_| text.is_some_and(|text| !text.is_empty())) {
        out.push_str(html);
    } else if let Some(text) = text.filter(|text| !text.is_empty()) {
        let _ = write!(out, r#"<p>{}</p>"#, escape_xml(text));
    }
    out.push_str("</span></div></foreignObject></g>");
}

pub(crate) fn render_kanban_diagram_svg(
    prepared: &KanbanPreparedArtifact,
    effective_config: &merman_core::MermaidConfig,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let (layout, prepared_sections, prepared_items) = prepared.render_parts();
    debug_assert_eq!(layout.sections.len(), prepared_sections.len());
    debug_assert_eq!(layout.items.len(), prepared_items.len());
    let security_level_loose = effective_config.get_str("securityLevel") == Some("loose");
    let effective_config = effective_config.as_value();
    let diagram_id = options.diagram_id_or("merman");

    let bounds = layout.bounds.clone().unwrap_or(Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 100.0,
    });
    let vb_min_x = bounds.min_x;
    let vb_min_y = bounds.min_y;
    let vb_w = (bounds.max_x - bounds.min_x).max(1.0);
    let vb_h = (bounds.max_y - bounds.min_y).max(1.0);

    let mut out = String::new();
    let root_bounds = root_svg::DiagramBounds::from_view_box(vb_min_x, vb_min_y, vb_w, vb_h);
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "kanban");
    root_chrome.dom = root_svg::RootDomProfile {
        fixed_height_placement: root_svg::SvgRootFixedHeightPlacement::AfterXmlns,
        fixed_style_placement: root_svg::RootStylePlacement::Tail,
        trailing_newline: false,
        ..root_svg::RootDomProfile::default()
    };
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Kanban, diagram_id)
            .write_open(
                &mut out,
                root_svg::RootViewportSpec::mermaid(root_bounds, layout.use_max_width)
                    .with_max_width(root_svg::RootMaxWidth::CssSixSignificant(vb_w)),
                root_chrome,
            )?;
    options.checkpoint_emit()?;

    let css = kanban_css(diagram_id, effective_config)?;
    let _ = write!(&mut out, r#"<style>{}</style>"#, css);
    options.checkpoint_emit()?;
    let config_view = crate::kanban::KanbanConfigView::new(effective_config);
    let label_style = config_view.layout_settings().text_style;
    let label_min_height =
        KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX * (label_style.font_size / 16.0).max(f64::EPSILON);
    let data_look = config_view.look();
    let data_look_attr = escape_attr(data_look.as_str());

    // Mermaid emits a single empty <g/> before the diagram content for kanban.
    out.push_str(r#"<g/>"#);

    out.push_str(r#"<g class="sections">"#);
    for (s, prepared_label) in layout.sections.iter().zip(prepared_sections) {
        let section_dom_id = kanban_dom_id(diagram_id, &s.id);
        options.checkpoint_emit()?;
        let left = s.center_x - s.width / 2.0;
        let geometry = prepared_label.geometry;
        let label_width = geometry.foreign_object_width;
        let label_x = left + (s.width - label_width) / 2.0;
        let section_div_style = if geometry.wrapped {
            format!(
                "display: table; white-space: break-spaces; line-height: 1.5; max-width: {width}px; text-align: center; width: {width}px;",
                width = fmt(s.width.max(0.0)),
            )
        } else {
            format!(
                "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center;",
                fmt(s.width.max(1.0)),
            )
        };

        let _ = write!(
            &mut out,
            r##"<g class="cluster undefined section-{idx}" id="{id}" data-look="{look}"><rect style="" rx="{rx}" ry="{ry}" x="{x}" y="{y}" width="{w}" height="{h}"/><g class="cluster-label" transform="translate({lx}, {ly})"><foreignObject width="{lw}" height="{fo_h}"><div xmlns="http://www.w3.org/1999/xhtml" style="{div_style}"><span class="nodeLabel">{label}</span></div></foreignObject></g></g>"##,
            idx = s.index,
            id = escape_attr(&section_dom_id),
            look = data_look_attr,
            rx = fmt(s.rx),
            ry = fmt(s.ry),
            x = fmt(left),
            y = fmt(s.rect_y),
            w = fmt(s.width),
            h = fmt(s.rect_height),
            lx = fmt(label_x),
            ly = fmt(s.rect_y),
            lw = fmt(label_width),
            fo_h = fmt(s.label_height),
            div_style = escape_attr(&section_div_style),
            label = prepared_label.html.as_str(),
        );
    }
    out.push_str("</g>");

    out.push_str(r#"<g class="items">"#);
    let item_label_inset_x = KANBAN_SECTION_PADDING_PX;
    let text_measurer = options.text_measurer_for(TextMeasurementPhase::Wrap);

    fn kanban_priority_stroke(priority: &str) -> Option<&'static str> {
        match priority.trim() {
            "Very High" => Some("red"),
            "High" => Some("orange"),
            "Medium" => None,
            "Low" => Some("blue"),
            "Very Low" => Some("lightblue"),
            _ => None,
        }
    }

    for (n, prepared_item) in layout.items.iter().zip(prepared_items) {
        let item_dom_id = kanban_dom_id(diagram_id, &n.id);
        options.checkpoint_emit()?;
        let max_w = (n.width - item_label_inset_x).max(0.0);
        let rect_x = -n.width / 2.0;
        let rect_y = -n.height / 2.0;
        let title_geometry = prepared_item.title.geometry;
        let ticket_geometry =
            measure_kanban_plain_label(&text_measurer, n.ticket.as_deref(), &label_style);
        let assigned_geometry =
            measure_kanban_plain_label(&text_measurer, n.assigned.as_deref(), &label_style);
        let height_adj = ticket_geometry
            .content_height
            .max(assigned_geometry.content_height)
            / 2.0;

        let left_x = rect_x + item_label_inset_x;
        let right_x = if assigned_geometry.foreign_object_width > 0.0 {
            n.width / 2.0 - item_label_inset_x - assigned_geometry.foreign_object_width
        } else {
            n.width / 2.0 - item_label_inset_x
        };

        let title_y = -height_adj - title_geometry.content_height / 2.0;
        let details_y = -height_adj + title_geometry.content_height / 2.0;

        let _ = write!(
            &mut out,
            r##"<g class="node undefined" id="{id}" transform="translate({x}, {y})">"##,
            id = escape_attr(&item_dom_id),
            x = fmt(n.center_x),
            y = fmt(n.center_y),
        );
        let _ = write!(
            &mut out,
            r##"<rect class="basic label-container __APA__" style="" rx="{rx}" ry="{ry}" x="{x}" y="{y}" width="{w}" height="{h}"/>"##,
            rx = fmt(n.rx),
            ry = fmt(n.ry),
            x = fmt(rect_x),
            y = fmt(rect_y),
            w = fmt(n.width),
            h = fmt(n.height),
        );

        let label_context = KanbanLabelRenderContext {
            max_width: max_w,
            min_height: label_min_height,
        };

        // Title label (may wrap).
        write_kanban_label_group(
            &mut out,
            &label_context,
            KanbanLabelGroup {
                position: (left_x, title_y),
                text: Some(n.label.as_str()),
                html: Some(prepared_item.title.html.as_str()),
                geometry: title_geometry,
                div_class: n.icon.as_deref().map(|_| "labelBkg"),
                wrap_title: true,
            },
        );

        // Ticket label: wrap in <a> when ticketBaseUrl is configured (upstream behavior).
        let ticket_text = n.ticket.as_deref();
        if let Some(t) = ticket_text.filter(|t| !t.is_empty()) {
            if let Some(ticket_link) = prepared_item.ticket_link.as_ref() {
                out.push_str(r#"<a class="kanban-ticket-link""#);
                if let Some(href) = ticket_link.href.as_ref() {
                    let _ = write!(&mut out, r#" xlink:href="{}""#, href.as_serialized_str());
                }
                if security_level_loose {
                    out.push_str(r#" target="_blank""#);
                }
                out.push('>');
                write_kanban_label_group(
                    &mut out,
                    &label_context,
                    KanbanLabelGroup {
                        position: (left_x, details_y),
                        text: Some(t),
                        html: None,
                        geometry: ticket_geometry,
                        div_class: None,
                        wrap_title: false,
                    },
                );
                out.push_str("</a>");
            } else {
                write_kanban_label_group(
                    &mut out,
                    &label_context,
                    KanbanLabelGroup {
                        position: (left_x, details_y),
                        text: Some(t),
                        html: None,
                        geometry: ticket_geometry,
                        div_class: None,
                        wrap_title: false,
                    },
                );
            }
        } else {
            write_kanban_label_group(
                &mut out,
                &label_context,
                KanbanLabelGroup {
                    position: (left_x, details_y),
                    text: None,
                    html: None,
                    geometry: ticket_geometry,
                    div_class: None,
                    wrap_title: false,
                },
            );
        }

        // Assigned label.
        write_kanban_label_group(
            &mut out,
            &label_context,
            KanbanLabelGroup {
                position: (right_x, details_y),
                text: n.assigned.as_deref(),
                html: None,
                geometry: assigned_geometry,
                div_class: None,
                wrap_title: false,
            },
        );

        if let Some(p) = n.priority.as_deref() {
            let y1 = rect_y + (n.rx / 2.0).floor();
            let y2 = rect_y + n.height - (n.rx / 2.0).floor();
            let stroke_attr = kanban_priority_stroke(p)
                .map(|s| format!(r#" stroke="{}""#, escape_attr(s)))
                .unwrap_or_default();
            let _ = write!(
                &mut out,
                r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke-width="4"{stroke_attr}/>"#,
                x1 = fmt(rect_x + 2.0),
                y1 = fmt(y1),
                x2 = fmt(rect_x + 2.0),
                y2 = fmt(y2),
                stroke_attr = stroke_attr,
            );
        }

        out.push_str("</g>");
    }

    out.push_str("</g>");
    out.push_str("</svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{
        HostMeasurementResult, HostTextMeasurement, HostTextMeasurementRequest, HostTextMeasurer,
        MeasurementProfileId, RenderEnvironment, TextMeasurementOperation, TextMeasurementPhase,
        TextMeasurementPolicy, TextMeasurementProfileIdentity, TextMeasurementSource,
    };
    use crate::kanban::{
        KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX, prepare_kanban_artifact_from_layout_for_test,
    };
    use crate::model::{Bounds, KanbanDiagramLayout, KanbanItemLayout, KanbanSectionLayout};
    use crate::text::TextMetrics;
    use std::sync::Arc;

    struct WideHost;

    impl HostTextMeasurer for WideHost {
        fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            let width = match request.operation {
                TextMeasurementOperation::Wrapped => request.max_width.unwrap_or(400.0),
                TextMeasurementOperation::Measure => 400.0,
                _ => return Ok(None),
            };
            Ok(Some(HostTextMeasurement::Metrics(TextMetrics {
                width,
                height: 40.0,
                line_count: 2,
            })))
        }
    }

    fn render_test_kanban(
        layout: &KanbanDiagramLayout,
        effective_config: &serde_json::Value,
        options: &SvgRenderOptions,
    ) -> Result<String> {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let effective_config = merman_core::MermaidConfig::from_value(effective_config.clone());
        let prepared =
            prepare_kanban_artifact_from_layout_for_test(layout, &effective_config, &measurer);
        with_test_svg_execution(options, |options| {
            render_kanban_diagram_svg(&prepared, &effective_config, options)
        })
        .and_then(|svg| svg.into_string_for(crate::family::RenderFamilyKind::Kanban))
    }

    fn attr_f64(tag: &str, name: &str) -> f64 {
        let prefix = format!(r#"{name}=""#);
        let start = tag.find(&prefix).expect("attribute") + prefix.len();
        let end = start + tag[start..].find('"').expect("attribute end");
        tag[start..end].parse().expect("numeric attribute")
    }

    fn foreign_object_before_text<'a>(svg: &'a str, text: &str) -> &'a str {
        let text_end = svg.find(text).expect("label text");
        let start = svg[..text_end]
            .rfind("<foreignObject ")
            .expect("foreignObject before label");
        let end = start + svg[start..].find('>').expect("foreignObject tag end") + 1;
        &svg[start..end]
    }

    fn ticket_layout(ticket: &str) -> KanbanDiagramLayout {
        KanbanDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -300.0,
                max_x: 220.0,
                max_y: 80.0,
            }),
            section_width: 200.0,
            padding: KANBAN_SECTION_PADDING_PX,
            max_label_height: KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX,
            viewbox_padding: 8.0,
            use_max_width: true,
            sections: Vec::new(),
            items: vec![KanbanItemLayout {
                id: "task".to_string(),
                label: "Create parsing tests".to_string(),
                parent_id: "todo".to_string(),
                center_x: 100.0,
                center_y: -240.0,
                width: 185.0,
                height: 56.0,
                rx: 5.0,
                ry: 5.0,
                ticket: Some(ticket.to_string()),
                assigned: None,
                priority: None,
                icon: None,
            }],
        }
    }

    #[test]
    fn kanban_css_includes_upstream_theme_rules() {
        let css = kanban_css("k", &serde_json::json!({})).unwrap();

        assert!(
            css.contains(
                "#k .section--1 rect,#k .section--1 path,#k .section--1 circle,#k .section--1 polygon,#k .section--1 path{fill:hsl(240, 100%, 86.2745098039%);stroke:hsl(240, 100%, 86.2745098039%);}"
            ),
            "expected generated section fill/stroke rules: {css}"
        );
        assert!(
            css.contains(
                "#k .node rect,#k .node circle,#k .node ellipse,#k .node polygon,#k .node path{fill:white;stroke:#9370DB;stroke-width:1px;}"
            ),
            "expected kanban item node background rules: {css}"
        );
        assert!(
            css.contains(
                "#k .kanban-ticket-link{fill:white;stroke:#9370DB;text-decoration:underline;}"
            ),
            "expected kanban ticket link styling: {css}"
        );
        assert!(
            css.contains(
                "#k .kanban-label{dy:1em;alignment-baseline:middle;text-anchor:middle;dominant-baseline:middle;text-align:center;}"
            ),
            "expected kanban label styling: {css}"
        );
    }

    #[test]
    fn kanban_ticket_links_preserve_the_final_upstream_anchor_shape() {
        let layout = ticket_layout("MC-2038");
        let config = serde_json::json!({
            "securityLevel": "strict",
            "kanban": {
                "ticketBaseUrl": "https://mermaidchart.atlassian.net/browse/#TICKET#"
            }
        });

        let svg = render_test_kanban(&layout, &config, &SvgRenderOptions::default()).unwrap();

        assert!(svg.contains(
            r#"<a class="kanban-ticket-link" xlink:href="https://mermaidchart.atlassian.net/browse/MC-2038">"#
        ));
        assert!(!svg.contains(r#"target="_blank""#), "{svg}");
    }

    #[test]
    fn kanban_loose_parity_preserves_upstream_ticket_target() {
        let layout = ticket_layout("MC-2038");
        let config = serde_json::json!({
            "securityLevel": "loose",
            "kanban": {
                "ticketBaseUrl": "https://mermaidchart.atlassian.net/browse/#TICKET#"
            }
        });

        let svg = render_test_kanban(&layout, &config, &SvgRenderOptions::default()).unwrap();

        assert!(svg.contains(
            r#"<a class="kanban-ticket-link" xlink:href="https://mermaidchart.atlassian.net/browse/MC-2038" target="_blank">"#
        ));
    }

    #[test]
    fn kanban_strict_security_removes_unsafe_href_but_preserves_ticket_anchor() {
        let layout = ticket_layout("MC-2038");
        let config = serde_json::json!({
            "securityLevel": "strict",
            "kanban": {
                "ticketBaseUrl": "javascript:alert('#TICKET#')"
            }
        });

        let svg = render_test_kanban(&layout, &config, &SvgRenderOptions::default()).unwrap();

        assert!(svg.contains(r#"<a class="kanban-ticket-link">"#), "{svg}");
        assert!(!svg.contains("xlink:href"), "{svg}");
        assert!(!svg.contains(r#"target="_blank""#), "{svg}");
        assert!(!svg.contains("about:blank"), "{svg}");
        assert!(svg.contains("<p>MC-2038</p>"), "{svg}");
    }

    #[test]
    fn kanban_strict_security_preserves_dom_literal_entity_spelling() {
        let layout = ticket_layout("MC-2038");
        let config = serde_json::json!({
            "securityLevel": "strict",
            "kanban": {
                "ticketBaseUrl": "jav&#x61;script:#TICKET#"
            }
        });

        let svg = render_test_kanban(&layout, &config, &SvgRenderOptions::default()).unwrap();

        assert!(
            svg.contains(
                r#"<a class="kanban-ticket-link" xlink:href="jav&amp;#x61;script:MC-2038">"#
            ),
            "{svg}"
        );
    }

    #[test]
    fn kanban_dom_ids_are_scoped_by_diagram_id() {
        let layout = KanbanDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 240.0,
                max_y: 180.0,
            }),
            section_width: 200.0,
            padding: KANBAN_SECTION_PADDING_PX,
            max_label_height: KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX,
            viewbox_padding: 8.0,
            use_max_width: true,
            sections: vec![KanbanSectionLayout {
                id: "constructor".to_string(),
                label: "Todo".to_string(),
                index: 1,
                center_x: 100.0,
                center_y: 0.0,
                width: 200.0,
                rect_y: -300.0,
                rect_height: 100.0,
                rx: 5.0,
                ry: 5.0,
                label_width: 40.0,
                label_height: KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX,
            }],
            items: vec![
                KanbanItemLayout {
                    id: "task1".to_string(),
                    label: "Task".to_string(),
                    parent_id: "constructor".to_string(),
                    center_x: 100.0,
                    center_y: -240.0,
                    width: 185.0,
                    height: 44.0,
                    rx: 5.0,
                    ry: 5.0,
                    ticket: None,
                    assigned: None,
                    priority: None,
                    icon: None,
                },
                KanbanItemLayout {
                    id: "__proto__".to_string(),
                    label: String::new(),
                    parent_id: "constructor".to_string(),
                    center_x: 100.0,
                    center_y: -190.0,
                    width: 185.0,
                    height: 44.0,
                    rx: 5.0,
                    ry: 5.0,
                    ticket: None,
                    assigned: None,
                    priority: None,
                    icon: None,
                },
            ],
        };
        let options = SvgRenderOptions {
            diagram_id: Some("kanban_fixture".to_string()),
            ..Default::default()
        };

        let svg = render_test_kanban(&layout, &serde_json::json!({}), &options).unwrap();

        assert!(svg.contains(r#"id="kanban_fixture-constructor""#));
        assert!(svg.contains(r#"id="kanban_fixture-task1""#));
        assert!(svg.contains(r#"id="kanban_fixture-__proto__""#));
        assert!(svg.contains(
            r#"<span style="text-align:left !important" class="nodeLabel markdown-node-label"><p>Task</p></span>"#
        ));
        assert!(!svg.contains(r#"id="constructor""#));
        assert!(!svg.contains(r#"id="task1""#));
        assert!(!svg.contains(r#"id="__proto__""#));
        let empty_item = svg
            .split_once(r#"id="kanban_fixture-__proto__""#)
            .expect("empty-label item")
            .1;
        let empty_title = empty_item
            .split_once("<foreignObject ")
            .expect("empty title foreignObject")
            .1;
        assert!(
            empty_title.starts_with(r#"width="0" height="0""#),
            "empty item titles retain Mermaid's zero-sized label geometry: {empty_title}"
        );
    }

    #[test]
    fn kanban_section_and_item_labels_use_mermaid_markdown() {
        let layout = KanbanDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -300.0,
                max_x: 220.0,
                max_y: 80.0,
            }),
            section_width: 200.0,
            padding: KANBAN_SECTION_PADDING_PX,
            max_label_height: KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX,
            viewbox_padding: 8.0,
            use_max_width: true,
            sections: vec![KanbanSectionLayout {
                id: "ready".to_string(),
                label: "*Ready*".to_string(),
                index: 1,
                center_x: 100.0,
                center_y: 0.0,
                width: 200.0,
                rect_y: -300.0,
                rect_height: 120.0,
                rx: 5.0,
                ry: 5.0,
                label_width: 40.0,
                label_height: KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX,
            }],
            items: vec![KanbanItemLayout {
                id: "ship".to_string(),
                label: "**Ship it**".to_string(),
                parent_id: "ready".to_string(),
                center_x: 100.0,
                center_y: -240.0,
                width: 185.0,
                height: 44.0,
                rx: 5.0,
                ry: 5.0,
                ticket: None,
                assigned: None,
                priority: None,
                icon: None,
            }],
        };
        let options = SvgRenderOptions {
            diagram_id: Some("kb".to_string()),
            ..Default::default()
        };

        let config = serde_json::json!({"securityLevel": "strict"});
        let svg = render_test_kanban(&layout, &config, &options).unwrap();

        assert!(svg.contains(r#"<span class="nodeLabel"><p><em>Ready</em></p></span>"#));
        assert!(svg.contains(
            r#"<span style="text-align:left !important" class="nodeLabel markdown-node-label"><p><strong>Ship it</strong></p></span>"#
        ));
        assert!(!svg.contains("*Ready*"), "{svg}");
        assert!(!svg.contains("**Ship it**"), "{svg}");

        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = crate::kanban::KanbanConfigView::new(&config)
            .layout_settings()
            .text_style;
        let sanitize_config = merman_core::MermaidConfig::from_value(config.clone());
        let markdown = crate::kanban::KanbanMarkdown::new(&sanitize_config);
        let section_html = markdown.render("*Ready*");
        let section_metrics = markdown.measure_html(&measurer, &section_html, &style, None);
        let source_metrics = markdown.measure_html(&measurer, "*Ready*", &style, None);
        let section_fo = foreign_object_before_text(&svg, "<p><em>Ready</em></p>");
        assert_eq!(attr_f64(section_fo, "width"), section_metrics.width);
        assert_ne!(
            attr_f64(section_fo, "width"),
            source_metrics.width,
            "renderer must not reuse dimensions measured from Markdown source markers"
        );
    }

    #[test]
    fn kanban_markdown_labels_are_sanitized_before_dom_insertion() {
        let config = serde_json::json!({
            "securityLevel": "strict"
        });
        let sanitize_config = merman_core::MermaidConfig::from_value(config);
        let html = crate::kanban::KanbanMarkdown::new(&sanitize_config)
            .render("*Ready* <b onclick=\"alert(1)\">safe</b><script>alert(2)</script>");

        assert!(html.contains("<em>Ready</em>"), "{html}");
        assert!(html.contains("<b>safe</b>"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert!(!html.contains("script"), "{html}");
        assert!(!html.contains("alert(2)"), "{html}");
    }

    #[test]
    fn kanban_sections_use_configured_look_in_dom_attributes() {
        let layout = KanbanDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -300.0,
                max_x: 220.0,
                max_y: 80.0,
            }),
            section_width: 200.0,
            padding: KANBAN_SECTION_PADDING_PX,
            max_label_height: KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX,
            viewbox_padding: 8.0,
            use_max_width: true,
            sections: vec![KanbanSectionLayout {
                id: "todo".to_string(),
                label: "Todo".to_string(),
                index: 1,
                center_x: 100.0,
                center_y: 0.0,
                width: 200.0,
                rect_y: -300.0,
                rect_height: 120.0,
                rx: 5.0,
                ry: 5.0,
                label_width: 40.0,
                label_height: KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX,
            }],
            items: Vec::new(),
        };
        let options = SvgRenderOptions {
            diagram_id: Some("kb".to_string()),
            ..Default::default()
        };

        let svg =
            render_test_kanban(&layout, &serde_json::json!({"look": "neo"}), &options).unwrap();

        assert!(
            svg.contains(r#"id="kb-todo" data-look="neo""#),
            "expected kanban section to propagate configured look: {svg}"
        );
        assert!(
            !svg.contains(r#"data-look="classic""#),
            "configured kanban look must not leave classic section attributes: {svg}"
        );
    }

    #[test]
    fn kanban_root_honors_disabled_max_width() {
        let layout = KanbanDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -300.0,
                max_x: 220.0,
                max_y: 80.0,
            }),
            section_width: 200.0,
            padding: KANBAN_SECTION_PADDING_PX,
            max_label_height: KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX,
            viewbox_padding: 8.0,
            use_max_width: false,
            sections: Vec::new(),
            items: Vec::new(),
        };
        let options = SvgRenderOptions {
            diagram_id: Some("kanbanFixed".to_string()),
            ..Default::default()
        };

        let svg = render_test_kanban(&layout, &serde_json::json!({}), &options).unwrap();
        let root_open = svg.split_once('>').expect("root svg open tag").0;
        let view_box = root_open
            .split_once(r#"viewBox=""#)
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(value, _)| {
                value
                    .split_whitespace()
                    .map(|part| part.parse::<f64>().expect("viewBox number"))
                    .collect::<Vec<_>>()
            })
            .expect("root viewBox");
        let bounds = layout.bounds.as_ref().expect("layout bounds");
        let expected_width = bounds.max_x - bounds.min_x;
        let expected_height = bounds.max_y - bounds.min_y;

        assert_eq!(attr_f64(root_open, "width"), expected_width);
        assert_eq!(attr_f64(root_open, "height"), expected_height);
        assert_eq!(
            view_box,
            vec![bounds.min_x, bounds.min_y, expected_width, expected_height]
        );
        assert!(!root_open.contains("style="), "{root_open}");
        assert!(!root_open.contains("max-width"), "{root_open}");
    }

    #[test]
    fn kanban_item_title_foreign_object_uses_rendered_content_width() {
        let layout = KanbanDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -300.0,
                max_x: 220.0,
                max_y: 80.0,
            }),
            section_width: 200.0,
            padding: KANBAN_SECTION_PADDING_PX,
            max_label_height: KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX,
            viewbox_padding: 8.0,
            use_max_width: true,
            sections: vec![KanbanSectionLayout {
                id: "todo".to_string(),
                label: "Todo".to_string(),
                index: 1,
                center_x: 100.0,
                center_y: 0.0,
                width: 200.0,
                rect_y: -300.0,
                rect_height: 120.0,
                rx: 5.0,
                ry: 5.0,
                label_width: 40.0,
                label_height: KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX,
            }],
            items: vec![KanbanItemLayout {
                id: "renderer".to_string(),
                label: "Implement renderer".to_string(),
                parent_id: "todo".to_string(),
                center_x: 100.0,
                center_y: -240.0,
                width: 185.0,
                height: 44.0,
                rx: 5.0,
                ry: 5.0,
                ticket: None,
                assigned: None,
                priority: None,
                icon: None,
            }],
        };
        let options = SvgRenderOptions {
            diagram_id: Some("kb".to_string()),
            ..Default::default()
        };

        let config = serde_json::json!({});
        let svg = render_test_kanban(&layout, &config, &options).unwrap();

        let title_fo = foreign_object_before_text(&svg, "<p>Implement renderer</p>");
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let style = crate::kanban::KanbanConfigView::new(&config)
            .layout_settings()
            .text_style;
        let sanitize_config = merman_core::MermaidConfig::from_value(config.clone());
        let markdown = crate::kanban::KanbanMarkdown::new(&sanitize_config);
        let title_html = markdown.render("Implement renderer");
        let title_metrics = markdown.measure_html(&measurer, &title_html, &style, None);
        let item = &layout.items[0];
        assert_eq!(attr_f64(title_fo, "width"), title_metrics.width);
        assert!(
            attr_f64(title_fo, "width") < item.width - KANBAN_SECTION_PADDING_PX,
            "an unwrapped upstream HTML label uses its content bbox: {title_fo}"
        );
        assert!(
            attr_f64(title_fo, "height") >= KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX,
            "title foreignObject must accommodate at least one label row: {title_fo}"
        );
    }

    #[test]
    fn kanban_host_wrap_policy_changes_geometry_and_records_provenance() {
        let layout = KanbanDiagramLayout {
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: -300.0,
                max_x: 220.0,
                max_y: 80.0,
            }),
            section_width: 200.0,
            padding: KANBAN_SECTION_PADDING_PX,
            max_label_height: KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX,
            viewbox_padding: 8.0,
            use_max_width: true,
            sections: Vec::new(),
            items: vec![KanbanItemLayout {
                id: "renderer".to_string(),
                label: "Implement renderer".to_string(),
                parent_id: "todo".to_string(),
                center_x: 100.0,
                center_y: -240.0,
                width: 185.0,
                height: 84.0,
                rx: 5.0,
                ry: 5.0,
                ticket: Some("KAN-1".to_string()),
                assigned: Some("Ada".to_string()),
                priority: None,
                icon: None,
            }],
        };
        let identity = TextMeasurementProfileIdentity::new(
            MeasurementProfileId::new("test.kanban-host").unwrap(),
            "v1",
        )
        .unwrap();
        let environment = RenderEnvironment::deterministic().with_text_measurement_policy(
            TextMeasurementPolicy::host_display(
                identity,
                Arc::new(WideHost),
                [TextMeasurementPhase::Wrap],
            ),
        );
        let session = environment.begin_session().unwrap();
        let measurer = session.text_measurer(TextMeasurementPhase::Wrap);
        let request = SvgRenderOptions::default();
        let debug = SvgDebugOptions::default();
        let options = SvgExecution::new(&request, &debug, &session).expect("SVG execution");

        let config = merman_core::MermaidConfig::default();
        let prepared = prepare_kanban_artifact_from_layout_for_test(&layout, &config, &measurer);
        let measurement_count_before_render: u64 = session
            .text_measurement_report()
            .entries()
            .iter()
            .map(|entry| entry.count())
            .sum();
        let svg = render_kanban_diagram_svg(&prepared, &config, &options).unwrap();
        let report = session.text_measurement_report();
        let measurement_count_after_render: u64 =
            report.entries().iter().map(|entry| entry.count()).sum();

        let title_fo = foreign_object_before_text(&svg, "<p>Implement renderer</p>");
        assert_eq!(
            attr_f64(title_fo, "width"),
            layout.items[0].width - KANBAN_SECTION_PADDING_PX
        );
        assert_eq!(attr_f64(title_fo, "height"), 40.0);
        assert_eq!(
            measurement_count_after_render,
            measurement_count_before_render + 2,
            "SVG emission must measure each non-empty detail label exactly once"
        );
        assert!(report.entries().iter().any(|entry| {
            entry.provenance().phase == TextMeasurementPhase::Wrap
                && entry.provenance().source == TextMeasurementSource::Host
        }));
    }
}
