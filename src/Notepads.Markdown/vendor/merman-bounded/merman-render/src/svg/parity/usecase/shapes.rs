use super::*;
use merman_core::diagrams::usecase::UsecaseNode;

pub(super) fn write_dagre_helper(
    out: &mut String,
    node: &LayoutNode,
    config: &merman_core::MermaidConfig,
) {
    let _ = write!(
        out,
        r#"<g class="label edgeLabel" id="{}" transform="translate({},{})"><rect width="0.1" height="0.1"/><g class="label" style="" transform="translate(0,0)"><rect/>"#,
        escape_attr(&node.id),
        fmt(node.x),
        fmt(node.y)
    );
    if config_bool(config.as_value(), &["htmlLabels"]).unwrap_or(true) {
        out.push_str(r#"<foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" style="display:table-cell;white-space:nowrap;line-height:1.5;max-width:10px;text-align:center"><span class="nodeLabel"/></div></foreignObject>"#);
    } else {
        out.push_str(r#"<g><rect class="background" style="stroke: none"/><text y="-10.1" style=""><tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em"/></text></g>"#);
    }
    out.push_str("</g></g>");
}

pub(super) fn write_empty_edge_label(
    out: &mut String,
    id: &str,
    config: &merman_core::MermaidConfig,
) {
    let html = config_bool(config.as_value(), &["htmlLabels"]).unwrap_or(true);
    let _ = write!(
        out,
        r#"<g class="edgeLabel"><g class="label" data-id="{}" transform="translate({},0)">"#,
        escape_attr(id),
        if html { "-2" } else { "0" }
    );
    if html {
        out.push_str(r#"<foreignObject width="4" height="0"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="display:table-cell;white-space:nowrap;line-height:1.5;max-width:200px;text-align:center"><span class="edgeLabel"/></div></foreignObject>"#);
    } else {
        out.push_str(r#"<text y="-10.1" text-anchor="middle"><tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em" text-anchor="middle"/></text>"#);
    }
    out.push_str("</g></g>");
}

pub(super) fn rect(
    out: &mut String,
    class: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &str,
) {
    let _ = write!(
        out,
        r#"<rect class="{}" x="{}" y="{}" width="{}" height="{}" style="{}"/>"#,
        escape_attr(class),
        fmt(x),
        fmt(y),
        fmt(width),
        fmt(height),
        escape_attr(style)
    );
}

pub(super) struct UsecaseNodeRenderContext<'a> {
    pub(super) source: Option<&'a UsecaseNode>,
    pub(super) note: bool,
    pub(super) style: &'a str,
    pub(super) config: &'a merman_core::MermaidConfig,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) diagram_id: SvgDiagramId<'a>,
    pub(super) options: &'a SvgExecution<'a>,
}

pub(super) fn write_node(
    out: &mut String,
    node: &LayoutNode,
    plan: &UsecaseNodePlan,
    context: &UsecaseNodeRenderContext<'_>,
) -> Result<()> {
    if let Some(actor) = context
        .source
        .filter(|node| node.kind == UsecaseNodeKind::Actor)
    {
        let _ = write!(
            out,
            r#"<rect class="usecase-actor-outline" x="{}" y="{}" width="{}" height="{}" opacity="0" aria-hidden="true"/>"#,
            fmt(-node.width / 2.0),
            fmt(-node.height / 2.0),
            fmt(node.width),
            fmt(node.height)
        );
        let content_height = node.height - 20.0;
        let glyph_y = -content_height / 2.0 + 36.0;
        let mut y = -content_height / 2.0 + 80.0;
        if let Some(stereotype) = &plan.stereotype {
            write_label(
                out,
                stereotype,
                UsecaseLabelKind::Node("label usecase-stereotype"),
                0.0,
                y + stereotype.metrics.height / 2.0,
                UsecaseLabelRenderContext {
                    config: context.config,
                    measurer: context.measurer,
                },
            );
            y += stereotype.metrics.height + 2.0;
        }
        write_label(
            out,
            &plan.label,
            UsecaseLabelKind::Node("label actor-label usecase-actor-label"),
            0.0,
            y + plan.label.metrics.height / 2.0,
            UsecaseLabelRenderContext {
                config: context.config,
                measurer: context.measurer,
            },
        );
        let variant = match actor.actor_type {
            UsecaseActorType::Normal => "shape usecase-actor-normal",
            UsecaseActorType::Hollow => "hollow",
            UsecaseActorType::Awesome => "awesome",
            UsecaseActorType::Icon => "icon",
        };
        let _ = write!(
            out,
            r#"<g class="usecase-actor-glyph usecase-actor-{variant}" transform="translate(0,{})" style="{}">"#,
            fmt(glyph_y),
            escape_attr(context.style)
        );
        match actor.actor_type {
            UsecaseActorType::Normal => out.push_str(r#"<path class="usecase-actor-stick" d="M 0 -12 C 6.627 -12 12 -17.373 12 -24 C 12 -30.627 6.627 -36 0 -36 C -6.627 -36 -12 -30.627 -12 -24 C -12 -17.373 -6.627 -12 0 -12 Z M 0 -12 V 8 M -17 -5 H 17 M 0 8 L -15 28 M 0 8 L 15 28"/>"#),
            UsecaseActorType::Hollow => out.push_str(r#"<circle class="usecase-actor-hollow-head" cx="0" cy="-23" r="9" fill="none"/><path class="usecase-actor-hollow-body" d="M -22 -10 H 22 V 0 H 6 L 22 17 L 13 28 L 0 13 L -13 28 L -22 17 L -6 0 H -22 Z" fill="none"/>"#),
            UsecaseActorType::Awesome => out.push_str(r#"<path class="usecase-actor-awesome-silhouette" d="M 0 -34 C 7.18 -34 13 -28.18 13 -21 C 13 -13.82 7.18 -8 0 -8 C -7.18 -8 -13 -13.82 -13 -21 C -13 -28.18 -7.18 -34 0 -34 Z M -24 25 C -24 7 -14 -3 0 -3 C 14 -3 24 7 24 25 C 24 28 21 30 18 30 H -18 C -21 30 -24 28 -24 25 Z"/>"#),
            UsecaseActorType::Icon => {
                out.push_str(r#"<rect class="usecase-actor-icon-frame" x="-26" y="-28" width="52" height="52" rx="4" ry="4"/>"#);
                let icon = if let Some(registry) = context.options.icon_registry() {
                    let work = context.options.work_meter();
                    let prefix = crate::svg::icon_registry::IconIdScopePrefix::from_parts(&["usecase-", context.diagram_id.semantic_str(), "-"], work)?;
                    registry.render_icon(crate::svg::icon_registry::IconRenderRequest {
                        icon_name: actor.icon.as_deref().unwrap_or(""), width_px: 42.0, height_px: 42.0,
                        fallback_prefix: Some("fa"), extra_class: None, id_scope: prefix.scope_parts(&[&actor.id], work)?, effective_config: context.config, work_meter: work,
                    })?
                } else { None };
                let fallback = icon.is_none();
                let icon = icon.unwrap_or_else(|| crate::svg::icon_registry::mermaid_unknown_icon_svg(42, 42));
                let _ = write!(out, r#"<g class="usecase-actor-icon-symbol{}" aria-hidden="true" transform="translate(-21,-23)"><g>{icon}</g></g>"#, if fallback { " usecase-actor-icon-fallback" } else { "" });
            }
        }
        if actor.business {
            let (center_y, radius): (f64, f64) = match actor.actor_type {
                UsecaseActorType::Hollow => (-23.0, 9.0),
                UsecaseActorType::Awesome => (-21.0, 13.0),
                _ => (-24.0, 12.0),
            };
            let angle = std::f64::consts::PI / 3.0;
            let (dx, dy) = (angle.cos(), -angle.sin());
            let offset = radius * 0.6;
            let chord = radius * 0.8;
            let (x, y) = (offset * -dy, center_y + offset * dx);
            let path = if actor.actor_type == UsecaseActorType::Icon {
                "M 12 -8 L 26 -26".into()
            } else {
                format!(
                    "M {} {} L {} {}",
                    fmt(x - chord * dx),
                    fmt(y - chord * dy),
                    fmt(x + chord * dx),
                    fmt(y + chord * dy)
                )
            };
            let _ = write!(
                out,
                r#"<path class="usecase-business-marker usecase-actor-business-marker" d="{path}" fill="none" style="stroke:inherit!important;stroke-width:inherit!important"/>"#
            );
        }
        out.push_str("</g>");
    } else if let Some(table) = &plan.table {
        rect(
            out,
            "label-container usecase-json-border",
            -node.width / 2.0,
            -node.height / 2.0,
            node.width,
            node.height,
            context.style,
        );
        let inner_width = node.width - 2.0 * table.border_width;
        let left = -inner_width / 2.0;
        let top = -node.height / 2.0 + table.border_width;
        out.push_str(r#"<g class="usecase-json-table-grid">"#);
        rect(
            out,
            "usecase-json-cell usecase-json-title-cell",
            left,
            top,
            inner_width,
            table.title_height,
            context.style,
        );
        write_label(
            out,
            &plan.label,
            UsecaseLabelKind::Node("label usecase-json-title"),
            0.0,
            top + table.title_height / 2.0,
            UsecaseLabelRenderContext {
                config: context.config,
                measurer: context.measurer,
            },
        );
        let mut row_top = top + table.title_height;
        for (index, row) in table.rows.iter().enumerate() {
            context.options.checkpoint_emit()?;
            let _ = write!(
                out,
                r#"<g class="usecase-json-row" data-row-index="{index}" transform="translate(0,{})">"#,
                fmt(row_top)
            );
            rect(
                out,
                "usecase-json-cell usecase-json-key-cell",
                left,
                0.0,
                table.key_width,
                row.height,
                context.style,
            );
            rect(
                out,
                "usecase-json-cell usecase-json-value-cell",
                left + table.key_width,
                0.0,
                table.value_width,
                row.height,
                context.style,
            );
            write_label(
                out,
                &row.key,
                UsecaseLabelKind::Node("label usecase-json-key"),
                left + table.key_width / 2.0,
                row.height / 2.0,
                UsecaseLabelRenderContext {
                    config: context.config,
                    measurer: context.measurer,
                },
            );
            write_label(
                out,
                &row.value,
                UsecaseLabelKind::Node("label usecase-json-value"),
                left + table.key_width + table.value_width / 2.0,
                row.height / 2.0,
                UsecaseLabelRenderContext {
                    config: context.config,
                    measurer: context.measurer,
                },
            );
            out.push_str("</g>");
            row_top += row.height;
        }
        out.push_str("</g>");
    } else {
        if plan.ellipse {
            let business = context.source.is_some_and(|node| node.business);
            let _ = write!(
                out,
                r#"<ellipse class="basic label-container{}" cx="0" cy="0" rx="{}" ry="{}" style="{}"/>"#,
                if business {
                    " usecase-business-ellipse"
                } else {
                    ""
                },
                fmt(node.width / 2.0),
                fmt(node.height / 2.0),
                escape_attr(context.style)
            );
            if business {
                let (rx, ry) = (node.width / 2.0, node.height / 2.0);
                let label_width = plan.label.metrics.width.max(
                    plan.stereotype
                        .as_ref()
                        .map_or(0.0, |label| label.metrics.width),
                );
                let (x1, x2) = (label_width / 2.0 + 2.0, rx - 2.0);
                let y1 = ry * (1.0 - (x1 / rx).powi(2)).max(0.0).sqrt();
                let y2 = -ry * (1.0 - (x2 / rx).powi(2)).max(0.0).sqrt();
                let _ = write!(
                    out,
                    r#"<path class="usecase-business-marker" d="M {} {} L {} {}" fill="none" style="{}"/>"#,
                    fmt(x1),
                    fmt(y1),
                    fmt(x2),
                    fmt(y2),
                    escape_attr(context.style)
                );
            }
        } else if context.note {
            // note.ts uses a RoughJS rectangle even for classic/neo. At zero
            // roughness its fill and outline still remain separate SVG paths.
            let randomness = context.options.rough_randomness(
                config_f64(context.config.as_value(), &["handDrawnSeed"]).unwrap_or(0.0),
                "usecase-note",
            );
            let note_fill = config_string(
                context.config.as_value(),
                &["themeVariables", "noteBkgColor"],
            )
            .unwrap_or_else(|| "#fff5ad".to_owned());
            let note_stroke = config_string(
                context.config.as_value(),
                &["themeVariables", "noteBorderColor"],
            )
            .unwrap_or_else(|| "#aaaa33".to_owned());
            let (fill, stroke) =
                roughjs_common::roughjs_paths_for_rect(roughjs_common::RoughRectSpec {
                    x: -node.width / 2.0,
                    y: -node.height / 2.0,
                    w: node.width,
                    h: node.height,
                    stroke_width: 1.3,
                    randomness: &randomness,
                })
                .ok_or_else(|| Error::InvalidModel {
                    message: "failed to construct Usecase note geometry".into(),
                })?;
            let _ = write!(
                out,
                r#"<g class="basic label-container outer-path"><path d="{fill}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{stroke}" stroke="{}" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style="{}"/></g>"#,
                escape_attr(&note_fill),
                escape_attr(context.style),
                escape_attr(&note_stroke),
                escape_attr(context.style)
            );
        } else {
            rect(
                out,
                "basic label-container",
                -node.width / 2.0,
                -node.height / 2.0,
                node.width,
                node.height,
                context.style,
            );
        }
        let stereo_height = plan
            .stereotype
            .as_ref()
            .map_or(0.0, |label| label.metrics.height);
        let gap = if plan.stereotype.is_some() && context.source.is_some_and(|node| node.business) {
            2.0
        } else {
            0.0
        };
        let height = plan.label.metrics.height + stereo_height + gap;
        if let Some(stereotype) = &plan.stereotype {
            write_label(
                out,
                stereotype,
                UsecaseLabelKind::Node("label usecase-stereotype"),
                0.0,
                -height / 2.0 + stereo_height / 2.0,
                UsecaseLabelRenderContext {
                    config: context.config,
                    measurer: context.measurer,
                },
            );
        }
        let label_start = out.len();
        write_label(
            out,
            &plan.label,
            if context.note {
                UsecaseLabelKind::Node("label noteLabel")
            } else if context
                .source
                .is_some_and(|node| node.business && plan.ellipse)
            {
                UsecaseLabelKind::Node("label usecase-label")
            } else {
                UsecaseLabelKind::Node("label")
            },
            0.0,
            height / 2.0 - plan.label.metrics.height / 2.0,
            UsecaseLabelRenderContext {
                config: context.config,
                measurer: context.measurer,
            },
        );
        if plan.folded_stereotype {
            annotate_folded_stereotype(out, label_start)?;
        }
    }
    Ok(())
}

fn annotate_folded_stereotype(out: &mut String, start: usize) -> Result<()> {
    // Match the first emitted label text node, as annotateUsecaseElements does.
    // The fragment is our own complete label group, so byte ranges can be applied
    // without reparsing the entire diagram or depending on literal label content.
    let fragment = &out[start..];
    let document = roxmltree::Document::parse(fragment).map_err(|error| Error::InvalidModel {
        message: format!("invalid prepared Usecase stereotype label: {error}"),
    })?;
    if let Some(span) = document
        .descendants()
        .find(|node| node.has_tag_name("span") && node.attribute("class") == Some("nodeLabel"))
    {
        let container = span
            .children()
            .find(|node| node.has_tag_name("p"))
            .unwrap_or(span);
        if let Some(text) = container.first_child().filter(|node| node.is_text()) {
            let range = text.range();
            out.insert_str(start + range.end, "</span>");
            out.insert_str(start + range.start, "<span class=\"usecase-stereotype\">");
        }
    } else if let Some(span) = document
        .descendants()
        .find(|node| node.has_tag_name("tspan") && node.children().any(|child| child.is_text()))
        && let Some(attribute) = span
            .attributes()
            .find(|attribute| attribute.name() == "class")
    {
        let insertion = attribute.range_value().end;
        out.insert_str(start + insertion, " usecase-stereotype");
    }
    Ok(())
}
