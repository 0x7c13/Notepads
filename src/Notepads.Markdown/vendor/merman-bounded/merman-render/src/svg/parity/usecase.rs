//! Mermaid 12 Usecase paint over its prepared graph and measured labels.

use super::*;
use crate::usecase::{UsecaseLabelPlan, UsecaseNodePlan, UsecasePreparedArtifact};
use merman_core::diagrams::usecase::{
    UsecaseActorType, UsecaseAnimation, UsecaseDiagramRenderModel, UsecaseLabelType,
    UsecaseNodeKind, UsecaseRelationshipType,
};
use std::collections::HashMap;

mod shapes;
mod theme;

fn dom_part(value: &str) -> String {
    let mut result = String::new();
    let mut replacement = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            result.push(ch);
            replacement = false;
        } else if !replacement {
            result.push('_');
            replacement = true;
        }
    }
    let result = result.trim_matches('_');
    if result.is_empty() {
        "element".into()
    } else {
        result.into()
    }
}

fn accessible_label(label: &UsecaseLabelPlan) -> String {
    accessible_text(&label.text, label.label_type)
}

fn accessible_text(text: &str, kind: UsecaseLabelType) -> String {
    match kind {
        UsecaseLabelType::Text => text.to_owned(),
        UsecaseLabelType::Markdown => crate::text::mermaid_markdown_to_lines(text, true)
            .iter()
            .map(|line| {
                line.iter()
                    .map(|word| word.0.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn styles(model: &UsecaseDiagramRenderModel, classes: &[String], inline: &[String]) -> String {
    crate::usecase::compiled_styles(model, classes, inline)
        .into_iter()
        .filter(|(key, _)| !crate::mermaid_style::is_label_style_key(key))
        .map(|(key, value)| {
            format!(
                "{key}:{} !important;",
                value.trim_end_matches("!important").trim()
            )
        })
        .collect()
}

struct UsecaseLabelRenderContext<'a> {
    config: &'a merman_core::MermaidConfig,
    measurer: &'a dyn TextMeasurer,
}

enum UsecaseLabelKind<'a> {
    Node(&'a str),
    Boundary,
    Edge(&'a str),
}

fn write_label(
    out: &mut String,
    plan: &UsecaseLabelPlan,
    kind: UsecaseLabelKind<'_>,
    center_x: f64,
    center_y: f64,
    context: UsecaseLabelRenderContext<'_>,
) {
    let UsecaseLabelRenderContext { config, measurer } = context;
    let html = config_bool(config.as_value(), &["htmlLabels"]).unwrap_or(true);
    let font_family = plan.style.font_family.as_deref().unwrap_or("sans-serif");
    let font_weight = plan.style.font_weight.as_deref().unwrap_or("normal");
    let css: String = plan
        .styles
        .iter()
        .map(|(key, value)| {
            let key = if !html && key == "color" {
                "fill"
            } else {
                key.as_str()
            };
            format!(
                "{key}:{} !important;",
                value.trim_end_matches("!important").trim()
            )
        })
        .collect();
    let is_edge_label = matches!(kind, UsecaseLabelKind::Edge(_));
    let is_boundary = matches!(kind, UsecaseLabelKind::Boundary);
    let class_attr = match kind {
        UsecaseLabelKind::Node(class) => class,
        UsecaseLabelKind::Boundary => "cluster-label system-boundary-title",
        UsecaseLabelKind::Edge(_) => "label",
    };
    if let UsecaseLabelKind::Edge(data_id) = kind {
        let _ = write!(
            out,
            r#"<g class="edgeLabel" transform="translate({},{})"><g class="label" data-id="{}" transform="translate({}, {})">"#,
            fmt(center_x),
            fmt(center_y),
            escape_attr(data_id),
            fmt(-plan.metrics.width / 2.0),
            fmt(-plan.metrics.height / 2.0),
        );
    } else {
        let _ = write!(
            out,
            r#"<g class="{}" transform="translate({},{})" style="font-family:{};font-size:{}px;font-weight:{}">"#,
            escape_attr(class_attr),
            fmt(center_x - plan.metrics.width / 2.0),
            fmt(center_y - plan.metrics.height / 2.0),
            escape_attr(font_family),
            fmt(plan.style.font_size),
            escape_attr(font_weight)
        );
    }
    if html {
        let content = plan
            .math_html
            .clone()
            .unwrap_or_else(|| match plan.label_type {
                // Mermaid's plain-text HTML label path emits one paragraph around the escaped text.
                UsecaseLabelType::Text => {
                    let text = crate::usecase::normalize_plain_label_line_breaks(&plan.text);
                    format!("<p>{}</p>", escape_xml(&text).replace('\n', "<br/>"))
                }
                // The Markdown helper already returns the complete XHTML fragment, including its
                // paragraph wrapper when the source is an inline paragraph. Wrapping it again breaks
                // folded stereotype annotation and changes the DOM nesting.
                UsecaseLabelType::Markdown => {
                    crate::text::mermaid_markdown_to_xhtml_label_fragment(
                        &plan.text,
                        config_bool(config.as_value(), &["markdownAutoWrap"]).unwrap_or(true),
                    )
                }
            });
        if !is_edge_label && !is_boundary {
            out.push_str("<rect/>");
        }
        let _ = write!(
            out,
            r#"<foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml"{} style="{}display:inline-block;white-space:normal;line-height:1.5;text-align:center;width:{}px"><span class="{}" style="{}">{}</span></div></foreignObject>"#,
            fmt(plan.metrics.width),
            fmt(plan.metrics.height),
            if is_edge_label {
                r#" class="labelBkg""#
            } else {
                ""
            },
            escape_attr(&css),
            fmt(plan.metrics.width),
            if is_edge_label {
                "edgeLabel"
            } else {
                "nodeLabel"
            },
            escape_attr(&css),
            content
        );
    } else {
        // Usecase plain labels are literal text, including strings that resemble HTML.
        match plan.label_type {
            UsecaseLabelType::Text => {
                // prepareUsecaseLayoutData escapes plain labels before createText splits
                // lines and words. Decode those entities only when writing text content,
                // so a literal <br/> remains visible instead of becoming a line break.
                let source = crate::usecase::normalize_plain_label_line_breaks(&plan.text)
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                label::write_svg_text_centered_from_create_text_source_with_style(
                    out, &source, &css,
                )
            }
            UsecaseLabelType::Markdown => {
                label::write_svg_text_markdown_wrapped_centered_with_style(
                    out,
                    &plan.text,
                    &css,
                    measurer,
                    &plan.style,
                    plan.max_width,
                )
            }
        }
    }
    out.push_str("</g>");
    if is_edge_label {
        out.push_str("</g>");
    }
}

pub(crate) fn render_usecase_diagram_svg_model(
    prepared: &UsecasePreparedArtifact,
    model: &UsecaseDiagramRenderModel,
    config: &merman_core::MermaidConfig,
    diagram_title: Option<&str>,
    measurer: &dyn TextMeasurer,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("usecase");
    let cfg = config.as_value();
    let layout = prepared.layout();
    let geometry: HashMap<_, _> = layout
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let plans: HashMap<_, _> = prepared
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let source_nodes: HashMap<_, _> = model
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let boundaries: HashMap<_, _> = model
        .boundaries
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let notes: HashMap<_, _> = model
        .notes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let json_nodes: HashMap<_, _> = model
        .json_nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let relationships: HashMap<_, _> = model
        .relationships
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    let edge_geometry: HashMap<_, _> = layout
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    let source_labels: HashMap<_, _> = plans
        .iter()
        .map(|(&id, plan)| {
            let kind = source_nodes
                .get(id)
                .map_or(plan.label.label_type, |node| node.label_type);
            (id, accessible_text(&plan.source_label, kind))
        })
        .collect();
    let padding = config_f64(cfg, &["usecase", "diagramPadding"]).unwrap_or(20.0);
    let bounds = layout.bounds.clone().unwrap_or(Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 0.0,
        max_y: 0.0,
    });
    let title = model
        .title
        .as_deref()
        .or(diagram_title)
        .filter(|text| !text.is_empty());
    let title_x = (bounds.min_x + bounds.max_x) / 2.0;
    let mut viewport_bounds = bounds.clone();
    if let Some(title) = title {
        let style = theme::font_style(cfg);
        let (left, right) = measurer.measure_svg_title_bbox_x(title, &style);
        let (ascent, descent) = crate::text::svg_text_vertical_extents_px(
            measurer,
            title,
            &style,
            crate::text::svg_title_bbox_vertical_extents_px(&style),
        );
        viewport_bounds.min_x = viewport_bounds.min_x.min(title_x - left);
        viewport_bounds.max_x = viewport_bounds.max_x.max(title_x + right);
        viewport_bounds.min_y = viewport_bounds.min_y.min(-ascent);
        viewport_bounds.max_y = viewport_bounds.max_y.max(descent);
    }
    let spec = root_svg::RootViewportSpec::mermaid(
        root_svg::DiagramBounds::from_extents(
            viewport_bounds.min_x,
            viewport_bounds.min_y,
            viewport_bounds.max_x,
            viewport_bounds.max_y,
            padding,
        ),
        config_bool(cfg, &["usecase", "useMaxWidth"]).unwrap_or(true),
    );
    let acc_title_id = model
        .acc_title
        .as_ref()
        .map(|_| format!("chart-title-{diagram_id}"));
    let acc_descr_id = model
        .acc_description
        .as_ref()
        .map(|_| format!("chart-desc-{diagram_id}"));
    let mut chrome = root_svg::RootChrome::new(diagram_id, "usecase");
    chrome.class = Some("usecaseDiagram");
    chrome.aria_labelledby = acc_title_id.as_deref();
    chrome.aria_describedby = acc_descr_id.as_deref();
    let actor_font = crate::usecase::text_style(cfg, Some(true));
    let usecase_font = crate::usecase::text_style(cfg, Some(false));
    let actor_font_size = format!("{}px", fmt(actor_font.font_size));
    let usecase_font_size = format!("{}px", fmt(usecase_font.font_size));
    let font_properties = [
        (
            "--mermaid-usecase-actor-font-size",
            actor_font_size.as_str(),
        ),
        (
            "--mermaid-usecase-actor-font-family",
            actor_font.font_family.as_deref().unwrap_or("sans-serif"),
        ),
        (
            "--mermaid-usecase-actor-font-weight",
            actor_font.font_weight.as_deref().unwrap_or("normal"),
        ),
        ("--mermaid-usecase-font-size", usecase_font_size.as_str()),
        (
            "--mermaid-usecase-font-family",
            usecase_font.font_family.as_deref().unwrap_or("sans-serif"),
        ),
        (
            "--mermaid-usecase-font-weight",
            usecase_font.font_weight.as_deref().unwrap_or("normal"),
        ),
    ];
    chrome.custom_properties = &font_properties;
    let mut out = String::new();
    let document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Usecase, diagram_id)
            .write_open(&mut out, spec, chrome)?;
    if let Some(title) = &model.acc_title {
        let _ = write!(
            out,
            r#"<title id="chart-title-{diagram_id}">{}</title>"#,
            escape_xml(title)
        );
    }
    if let Some(description) = &model.acc_description {
        let _ = write!(
            out,
            r#"<desc id="chart-desc-{diagram_id}">{}</desc>"#,
            escape_xml(description)
        );
    }
    theme::write_css(&mut out, diagram_id, cfg);
    // Mermaid emits marker wrappers that are observable in strict SVG DOM parity.
    out.push_str("<g>");
    markers::push_base_edge_markers(&mut out, diagram_id, "usecase");
    out.push_str("<defs>");
    let _ = write!(
        out,
        r#"<marker id="{diagram_id}_usecase-extensionStart" class="marker extension usecase" refX="18" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse"><path d="M 1,7 L18,13 V 1 Z"/></marker>"#
    );
    out.push_str("</defs><defs>");
    let _ = write!(
        out,
        r#"<marker id="{diagram_id}_usecase-extensionEnd" class="marker extension usecase" refX="1" refY="7" markerWidth="20" markerHeight="28" markerUnits="userSpaceOnUse" orient="auto"><path d="M 1,1 V 13 L18,7 Z"/></marker>"#
    );
    out.push_str("</defs>");
    let _ = write!(
        out,
        r#"<marker id="{diagram_id}_usecase-extensionStart-margin" class="marker extension usecase" refX="18" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse" viewBox="0 0 20 14"><polygon points="10,7 18,13 18,1" style="stroke-width: 2; stroke-dasharray: 0;"/></marker>"#
    );
    out.push_str("<defs>");
    let _ = write!(
        out,
        r#"<marker id="{diagram_id}_usecase-extensionEnd-margin" class="marker extension usecase" refX="9" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse" viewBox="0 0 20 14"><polygon points="10,1 10,13 18,7" style="stroke-width: 2; stroke-dasharray: 0;"/></marker>"#
    );
    out.push_str(r#"</defs><g class="root"><g class="clusters">"#);
    for (color_index, plan) in prepared
        .nodes
        .iter()
        .filter(|plan| plan.is_boundary)
        .enumerate()
    {
        options.checkpoint_emit()?;
        let node = geometry[plan.id.as_str()];
        let boundary = boundaries[plan.id.as_str()];
        let kind = if plan.package { "package" } else { "rect" };
        let accessible = format!("{kind} system boundary {}", accessible_label(&plan.label));
        let appearance = theme::appearance_attributes(cfg, Some(color_index));
        let _ = write!(
            out,
            r#"<g id="{}-usecase-{}" class="cluster usecase-system-boundary usecase-system-boundary-{kind} default system-boundary system-boundary-{kind} {}" data-boundary-type="{kind}"{appearance} data-usecase-id="{}" data-usecase-kind="boundary" role="img" aria-label="{}">"#,
            dom_part(diagram_id.semantic_str()),
            dom_part(&plan.id),
            escape_attr(&boundary.classes.join(" ")),
            escape_attr(&plan.id),
            escape_attr(&accessible)
        );
        let style = styles(model, &boundary.classes, &boundary.styles);
        let tab_height = if plan.package {
            plan.label.metrics.height + 10.0
        } else {
            0.0
        };
        let tab_width = node
            .width
            .min(80.0_f64.max(plan.label.metrics.width + 20.0));
        let left = node.x - node.width / 2.0;
        let top = node.y - node.height / 2.0;
        if plan.package {
            shapes::rect(
                &mut out,
                "boundary-tab system-boundary-package-tab label-container",
                left,
                top,
                tab_width,
                tab_height,
                &style,
            );
        }
        shapes::rect(
            &mut out,
            "boundary-body label-container",
            left,
            top + tab_height,
            node.width,
            node.height - tab_height,
            &style,
        );
        write_label(
            &mut out,
            &plan.label,
            UsecaseLabelKind::Boundary,
            if plan.package {
                left + tab_width / 2.0
            } else {
                node.x
            },
            top + if plan.package {
                tab_height / 2.0
            } else {
                plan.label.metrics.height / 2.0
                    + config_f64(cfg, &["flowchart", "subGraphTitleMargin", "top"]).unwrap_or(0.0)
            },
            UsecaseLabelRenderContext { config, measurer },
        );
        out.push_str("</g>");
    }
    let elk = crate::layout_backend::resolve_graph_layout(cfg).backend
        == crate::layout_backend::GraphLayoutBackend::Elk;
    let hop_points = if elk && config_bool(cfg, &["elk", "lineHops"]) != Some(false) {
        let clone_work = prepared.edges.iter().fold(0usize, |total, plan| {
            total.saturating_add(
                edge_geometry[plan.id.as_str()]
                    .points
                    .len()
                    .saturating_add(3),
            )
        });
        options.work_meter().charge(clone_work)?;
        prepared
            .edges
            .iter()
            .map(|plan| prepared.edge_points(edge_geometry[plan.id.as_str()]))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let hop_edges = prepared
        .edges
        .iter()
        .zip(&hop_points)
        .map(|(plan, points)| line_hops::LineHopEdge {
            id: plan.id.as_str(),
            points,
            curve: Some(if edge_geometry[plan.id.as_str()].points.is_empty() {
                "linear"
            } else {
                "rounded"
            }),
            arrow_type_start: plan.start_marker.as_deref(),
            arrow_type_end: plan.end_marker.as_deref(),
        })
        .collect::<Vec<_>>();
    let hopped_paths = line_hops::elk_line_hop_paths(cfg, &hop_edges, options.work_meter())?;
    out.push_str("</g><g class=\"edgePaths edges\">");
    for plan in &prepared.edges {
        options.checkpoint_emit()?;
        let edge = edge_geometry[plan.id.as_str()];
        let points = prepared.edge_points(edge);
        let data_points = base64::engine::general_purpose::STANDARD
            .encode(crate::svg::parity::util::json_stringify_points(&points));
        let data_look = config_string(cfg, &["look"]).unwrap_or_else(|| "neo".to_owned());
        let mut points = points;
        let curve = if elk {
            if edge.points.is_empty() {
                "linear"
            } else {
                "rounded"
            }
        } else {
            cfg.pointer("/flowchart/curve")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("basis")
        };
        let path = edge_path::render_path(
            &mut points,
            curve,
            plan.start_marker.as_deref(),
            plan.end_marker.as_deref(),
        );
        let path = hopped_paths
            .get(plan.id.as_str())
            .map_or(path.as_str(), String::as_str);
        let relation = relationships
            .get(plan.original_id.as_deref().unwrap_or(&plan.id))
            .copied();
        let label = relation.map(|edge| match edge.relationship_type {
            UsecaseRelationshipType::Association => plan
                .label
                .as_ref()
                .map(|label| format!("association {}", accessible_label(label)))
                .unwrap_or_else(|| "association".into()),
            UsecaseRelationshipType::Include => "include".into(),
            UsecaseRelationshipType::Extend => "extend".into(),
            UsecaseRelationshipType::Generalization => "generalization".into(),
        });
        let relation_kind = relation.map_or("note", |edge| match edge.relationship_type {
            UsecaseRelationshipType::Association => "association",
            UsecaseRelationshipType::Include => "include",
            UsecaseRelationshipType::Extend => "extend",
            UsecaseRelationshipType::Generalization => "generalization",
        });
        let mut classes = format!(
            "edge-thickness-normal edge-pattern-{} default relationship relationship-{relation_kind}",
            if plan.dotted { "dotted" } else { "solid" }
        );
        if let Some(edge) = relation {
            for class in &edge.classes {
                classes.push(' ');
                classes.push_str(class);
            }
            if edge.animate || edge.animation.is_some() {
                classes.push_str(match edge.animation {
                    Some(UsecaseAnimation::Slow) => " edge-animation-slow",
                    _ => " edge-animation-fast",
                });
            }
        }
        let _ = write!(
            out,
            r#"<path id="{}{}-{}" data-id="{}" data-et="edge" data-edge="true" data-look="{}" data-points="{}" class="{}" d="{}""#,
            if plan.dagre_recursive { "" } else { "usecase-" },
            dom_part(diagram_id.semantic_str()),
            dom_part(&plan.id),
            escape_attr(&plan.id),
            escape_attr(&data_look),
            escape_attr(&data_points),
            escape_attr(&classes),
            path
        );
        if !plan.dagre_recursive {
            let _ = write!(
                out,
                r#" data-usecase-id="{}" data-usecase-kind="{}""#,
                escape_attr(&plan.id),
                if plan.internal {
                    "note-connector"
                } else {
                    "relationship"
                }
            );
        }
        if plan.dagre_recursive {
            // Extracted edges are absent from the top-level LayoutData annotation pass.
        } else if plan.internal {
            out.push_str(" aria-hidden=\"true\"");
        } else {
            let name = if plan.original_id.is_some() {
                plan.id.clone()
            } else {
                format!(
                    "{} from {} to {}",
                    label.unwrap_or_default(),
                    source_labels[plan.source.as_str()],
                    source_labels[plan.target.as_str()]
                )
            };
            let _ = write!(out, r#" role="img" aria-label="{}""#, escape_attr(&name));
        }
        let mut edge_style = relation
            .map(|edge| styles(model, &edge.classes, &edge.styles))
            .unwrap_or_default();
        if plan.dotted {
            edge_style.push_str("stroke-dasharray:3;");
        }
        let edge_style = if let Some(hopped_d) = hopped_paths.get(plan.id.as_str()) {
            line_hops::rewrite_style_after_line_hop(&edge_style, hopped_d, options.work_meter())?
        } else {
            std::borrow::Cow::Borrowed(edge_style.as_str())
        };
        let _ = write!(out, r#" style="{}""#, escape_attr(&edge_style));
        for (end, marker) in [("start", &plan.start_marker), ("end", &plan.end_marker)] {
            if let Some(marker) = marker {
                let name = marker.strip_prefix("arrow_").unwrap_or(marker);
                let suffix = if end == "start" { "Start" } else { "End" };
                let _ = write!(
                    out,
                    r#" marker-{end}="url(#{diagram_id}_usecase-{name}{suffix}-margin)""#
                );
            }
        }
        out.push_str("/>");
    }
    out.push_str("</g><g class=\"edgeLabels\">");
    for plan in &prepared.edges {
        if let Some(label) = &plan.label {
            let edge = edge_geometry[plan.id.as_str()];
            if let Some(position) = &edge.label {
                write_label(
                    &mut out,
                    label,
                    UsecaseLabelKind::Edge(&plan.id),
                    position.x,
                    position.y,
                    UsecaseLabelRenderContext { config, measurer },
                );
            }
        } else if plan.original_id.is_some() {
            shapes::write_empty_edge_label(&mut out, &plan.id, config);
        }
    }
    out.push_str("</g><g class=\"nodes\">");
    for (color_index, plan) in prepared
        .nodes
        .iter()
        .filter(|plan| !plan.is_boundary)
        .enumerate()
    {
        options.checkpoint_emit()?;
        let node = geometry[plan.id.as_str()];
        if plan.dagre_helper {
            shapes::write_dagre_helper(&mut out, node, config);
            continue;
        }
        let source = source_nodes.get(plan.id.as_str()).copied();
        let note = notes.get(plan.id.as_str()).copied();
        let json = json_nodes.get(plan.id.as_str()).copied();
        let (kind, name, classes, inline) = if let Some(source) = source {
            let label = &source_labels[plan.id.as_str()];
            let stereotype = source
                .stereotype
                .as_ref()
                .map(|value| {
                    format!(
                        ", stereotype {}",
                        merman_core::sanitize::sanitize_text(value, config)
                    )
                })
                .unwrap_or_default();
            let business = if source.business { "business " } else { "" };
            let actor = source.kind == UsecaseNodeKind::Actor;
            let variant = match source.actor_type {
                UsecaseActorType::Normal => "",
                UsecaseActorType::Hollow => "hollow ",
                UsecaseActorType::Awesome => "awesome ",
                UsecaseActorType::Icon => "icon ",
            };
            (
                if actor { "actor" } else { "usecase" },
                if actor {
                    format!("{business}{variant}actor {label}{stereotype}")
                } else {
                    format!("{business}use case {label}{stereotype}")
                },
                source.classes.as_slice(),
                source.styles.as_slice(),
            )
        } else if let Some(note) = note {
            (
                "note",
                format!(
                    "Note for {}: {}",
                    source_labels[note.target.as_str()],
                    accessible_label(&plan.label)
                ),
                [].as_slice(),
                [].as_slice(),
            )
        } else if let Some(json) = json {
            let rows = plan
                .table
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|row| format!("{}: {}", row.accessible_key, row.value.text))
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            (
                "json",
                if rows.is_empty() {
                    json.id.clone()
                } else {
                    format!("{}: {rows}", json.id)
                },
                json.classes.as_slice(),
                json.styles.as_slice(),
            )
        } else {
            return Err(Error::InvalidModel {
                message: format!("missing Usecase node {}", plan.id),
            });
        };
        let appearance = theme::appearance_attributes(cfg, source.map(|_| color_index));
        let role_classes = match source {
            Some(node) if node.kind == UsecaseNodeKind::Actor => {
                let variant = match node.actor_type {
                    UsecaseActorType::Normal => "normal",
                    UsecaseActorType::Hollow => "hollow",
                    UsecaseActorType::Awesome => "awesome",
                    UsecaseActorType::Icon => "icon",
                };
                format!(
                    "usecase-actor usecase-actor-{variant}{} usecase-actor-variant usecase-actor-{variant}",
                    if node.business {
                        " usecase-business"
                    } else {
                        ""
                    }
                )
            }
            Some(node) => format!(
                "usecase-element usecase-{}{}",
                if plan.ellipse { "ellipse" } else { "rect" },
                if node.business && plan.ellipse {
                    " usecase-business usecase-business-shape"
                } else if node.business {
                    " usecase-business"
                } else {
                    ""
                }
            ),
            None if note.is_some() => "usecase-note".into(),
            None => "usecase-json-table usecase-json-table".into(),
        };
        let _ = write!(
            out,
            r#"<g id="{}-usecase-{}"{appearance} class="node default {role_classes} {}" data-usecase-id="{}" data-usecase-kind="{kind}" role="img" aria-label="{}" transform="translate({},{})">"#,
            dom_part(diagram_id.semantic_str()),
            dom_part(&plan.id),
            escape_attr(&classes.join(" ")),
            escape_attr(&plan.id),
            escape_attr(&name),
            fmt(node.x),
            fmt(node.y)
        );
        let style = styles(model, classes, inline);
        shapes::write_node(
            &mut out,
            node,
            plan,
            &shapes::UsecaseNodeRenderContext {
                source,
                note: note.is_some(),
                style: &style,
                config,
                measurer,
                diagram_id,
                options,
            },
        )?;
        out.push_str("</g>");
    }
    out.push_str("</g></g></g>");
    super::look_defs::push_look_shadow_defs(&mut out, diagram_id, cfg);
    super::look_defs::push_look_gradient(&mut out, diagram_id, cfg);
    if let Some(title) = title {
        let _ = write!(
            out,
            r#"<text class="usecaseDiagramTitleText" x="{}" y="{}" text-anchor="middle">{}</text>"#,
            fmt(title_x),
            fmt(0.0),
            escape_xml(title)
        );
    }
    out.push_str("</svg>");
    options.checkpoint_emit()?;
    document.complete(out)
}
