use super::super::*;

// ER diagram SVG renderer implementation (split from parity.rs).

// ELK overwrites the model curve after routing; Dagre retains ER's basis curve.
fn er_edge_path_d(
    points: &[crate::model::LayoutPoint],
    is_elk_layout: bool,
    missing_section: bool,
) -> String {
    if missing_section {
        return super::super::curve::curve_linear_path_d(points);
    }
    if is_elk_layout {
        return super::super::curve::curve_rounded_path_d_and_bounds(points, 5.0, false, None).0;
    }
    if let [a, b] = points {
        return curve_basis_path_d(&[
            a.clone(),
            crate::model::LayoutPoint {
                x: (a.x + b.x) / 2.0,
                y: (a.y + b.y) / 2.0,
            },
            b.clone(),
        ]);
    }
    curve_basis_path_d(points)
}

fn is_er_redux_color_theme(effective_config: &serde_json::Value) -> bool {
    matches!(
        SvgTheme::new(effective_config).theme_name().as_str(),
        "redux-color" | "redux-dark-color"
    )
}

fn er_redux_color_id(border_colors: &[String], color_index: usize) -> Option<String> {
    (!border_colors.is_empty()).then(|| format!("color-{}", color_index % border_colors.len()))
}

fn er_color_indices(
    model: &merman_core::diagrams::er::ErDiagramRenderModel,
) -> std::collections::HashMap<String, usize> {
    fn insertion_index(id: &str) -> Option<usize> {
        id.rsplit_once('-')?.1.parse().ok()
    }

    let mut entities: Vec<_> = model.entities.values().collect();
    entities.sort_by(|left, right| {
        (insertion_index(&left.id), left.id.as_str())
            .cmp(&(insertion_index(&right.id), right.id.as_str()))
    });
    entities
        .into_iter()
        .enumerate()
        .map(|(index, entity)| (entity.id.clone(), index))
        .collect()
}

fn er_redux_color_css<I>(
    diagram_id: I,
    data_look: &str,
    border_colors: &[String],
    background_colors: &[String],
) -> String
where
    I: Copy + std::fmt::Display,
{
    let mut out = String::new();
    for (index, border_color) in border_colors.iter().enumerate() {
        let border_color = border_color.trim();
        let fill = if background_colors.is_empty() {
            String::new()
        } else {
            format!(
                "fill:{};",
                background_colors[index % background_colors.len()].trim()
            )
        };
        let _ = write!(
            &mut out,
            r#"#{} [data-look="{}"][data-color-id="color-{}"].node path{{stroke:{};{}}}#{} [data-look="{}"][data-color-id="color-{}"].node rect{{stroke:{};{}}}"#,
            diagram_id,
            escape_xml(data_look),
            index,
            border_color,
            fill,
            diagram_id,
            escape_xml(data_look),
            index,
            border_color,
            fill,
        );
    }
    out
}

fn insert_er_redux_color_css<I>(css: &mut String, diagram_id: I, color_css: &str)
where
    I: SvgDiagramIdValue,
{
    if color_css.is_empty() {
        return;
    }

    let escaped_id = diagram_id.semantic_value();
    let family_rule = format!("#{escaped_id} .entityBox");
    let root_rule = format!("#{escaped_id} :root");
    let insertion_point = css
        .find(&family_rule)
        .or_else(|| css.find(&root_rule))
        .unwrap_or(css.len());
    css.insert_str(insertion_point, color_css);
}

fn compile_er_entity_styles(
    entity: &crate::er::ErEntity,
    classes: &indexmap::IndexMap<String, crate::er::ErClassDef>,
) -> (Vec<String>, Vec<String>) {
    let mut compiled_box: Vec<String> = Vec::new();
    let mut compiled_text: Vec<String> = Vec::new();
    let mut seen_classes: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for class_name in entity.css_classes.split_whitespace() {
        if !seen_classes.insert(class_name) {
            continue;
        }
        let Some(def) = classes.get(class_name) else {
            continue;
        };
        for s in &def.styles {
            let t = s.trim();
            if t.is_empty() {
                continue;
            }
            compiled_box.push(t.to_string());
        }
        for s in &def.text_styles {
            let t = s.trim();
            if t.is_empty() {
                continue;
            }
            compiled_text.push(t.to_string());
        }
    }

    let mut rect_map: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut text_map: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    // Box styles: classDef styles + `style` statements.
    for s in compiled_box.iter().chain(entity.css_styles.iter()) {
        let Some((k, v)) = parse_style_decl(s) else {
            continue;
        };
        if is_rect_style_key(k) {
            rect_map.insert(k.to_string(), v.to_string());
        }
        // Mermaid treats `color:` as the HTML label text color (even if it comes from the style list).
        if k == "color" {
            text_map.insert("color".to_string(), v.to_string());
        }
    }

    // Text styles: classDef textStyles + `style` statements (only text-related keys).
    for s in compiled_text.iter().chain(entity.css_styles.iter()) {
        let Some((k, v)) = parse_style_decl(s) else {
            continue;
        };
        if !is_text_style_key(k) {
            continue;
        }
        if k == "color" {
            text_map.insert("color".to_string(), v.to_string());
        } else {
            text_map.insert(k.to_string(), v.to_string());
        }
    }

    let mut rect_decls: Vec<String> = Vec::new();
    for k in [
        "fill",
        "stroke",
        "stroke-width",
        "stroke-dasharray",
        "opacity",
        "fill-opacity",
        "stroke-opacity",
    ] {
        if let Some(v) = rect_map.get(k) {
            rect_decls.push(format!("{k}:{v}"));
        }
    }

    let mut text_decls: Vec<String> = Vec::new();
    for k in [
        "color",
        "font-family",
        "font-size",
        "font-weight",
        "opacity",
    ] {
        if let Some(v) = text_map.get(k) {
            text_decls.push(format!("{k}:{v}"));
        }
    }

    (rect_decls, text_decls)
}

fn compile_er_subgraph_styles(
    subgraph: &crate::er::ErSubgraph,
    classes: &indexmap::IndexMap<String, crate::er::ErClassDef>,
) -> (Vec<String>, Vec<String>) {
    let mut compiled_box = Vec::new();
    let mut compiled_text = Vec::new();
    let mut seen_classes = std::collections::HashSet::new();
    for class_name in &subgraph.classes {
        if !seen_classes.insert(class_name.as_str()) {
            continue;
        }
        let Some(def) = classes.get(class_name) else {
            continue;
        };
        compiled_box.extend(def.styles.iter().map(|style| style.trim().to_string()));
        compiled_text.extend(def.text_styles.iter().map(|style| style.trim().to_string()));
    }

    let mut rect_map = std::collections::BTreeMap::new();
    let mut text_map = std::collections::BTreeMap::new();
    for style in compiled_box.iter().chain(subgraph.css_styles.iter()) {
        let Some((key, value)) = parse_style_decl(style) else {
            continue;
        };
        if is_rect_style_key(key) {
            rect_map.insert(key.to_string(), value.to_string());
        }
        if key == "color" {
            text_map.insert("color".to_string(), value.to_string());
        }
    }
    for style in compiled_text.iter().chain(subgraph.css_styles.iter()) {
        let Some((key, value)) = parse_style_decl(style) else {
            continue;
        };
        if is_text_style_key(key) {
            text_map.insert(key.to_string(), value.to_string());
        }
    }

    let rect_decls = [
        "fill",
        "stroke",
        "stroke-width",
        "stroke-dasharray",
        "opacity",
        "fill-opacity",
        "stroke-opacity",
    ]
    .into_iter()
    .filter_map(|key| rect_map.get(key).map(|value| format!("{key}:{value}")))
    .collect();
    let text_decls = [
        "color",
        "font-family",
        "font-size",
        "font-weight",
        "opacity",
    ]
    .into_iter()
    .filter_map(|key| text_map.get(key).map(|value| format!("{key}:{value}")))
    .collect();
    (rect_decls, text_decls)
}

fn er_subgraph_label_fragment(subgraph: &crate::er::ErSubgraph) -> String {
    if subgraph.label_type == "string" || subgraph.label_type == "text" {
        escape_xml(&subgraph.title)
    } else {
        crate::text::mermaid_markdown_to_xhtml_label_fragment(&subgraph.title, true)
    }
}

#[derive(Clone, Copy)]
struct ErSubgraphRenderContext<'a> {
    diagram_id: SvgDiagramId<'a>,
    data_look: &'a str,
    classes: &'a indexmap::IndexMap<String, crate::er::ErClassDef>,
    use_html_labels: bool,
    measurer: &'a dyn crate::text::TextMeasurer,
    label_style: &'a crate::text::TextStyle,
    translate_x: f64,
    translate_y: f64,
}

fn render_er_subgraph_cluster(
    out: &mut String,
    cluster: &crate::model::LayoutCluster,
    subgraph: &crate::er::ErSubgraph,
    context: ErSubgraphRenderContext<'_>,
) {
    let (rect_styles, text_styles) = compile_er_subgraph_styles(subgraph, context.classes);
    let rect_style_attr = if rect_styles.is_empty() {
        "style=\"\"".to_string()
    } else {
        format!(
            r#"style="{}""#,
            escape_xml(&style_decls_with_important(&rect_styles))
        )
    };
    let text_style_attr = if text_styles.is_empty() {
        "style=\"\"".to_string()
    } else {
        format!(
            r#"style="{}""#,
            escape_xml(&style_decls_with_important(&text_styles))
        )
    };
    let class_attr = if subgraph.classes.is_empty() {
        "cluster".to_string()
    } else {
        format!("cluster {}", subgraph.classes.join(" "))
    };
    let width = cluster.width.max(1.0);
    let height = cluster.height.max(1.0);
    let left = cluster.x - width / 2.0 + context.translate_x;
    let top = cluster.y - height / 2.0 + context.translate_y;
    let title_width = cluster.title_label.width.max(0.0);
    let title_height = cluster.title_label.height.max(0.0);
    let title_x = cluster.title_label.x + context.translate_x;
    let title_y = cluster.title_label.y + context.translate_y;
    let _ = write!(
        out,
        r#"<g id="{}-{}" class="{}" data-look="{}"><rect class="basic label-container" {} x="{}" y="{}" width="{}" height="{}"/>"#,
        escape_xml(&format!("{}", context.diagram_id)),
        escape_xml(&subgraph.id),
        escape_xml(&class_attr),
        escape_xml(context.data_look),
        rect_style_attr,
        fmt(left),
        fmt(top),
        fmt(width),
        fmt(height),
    );
    if context.use_html_labels {
        let _ = write!(
            out,
            r#"<g class="cluster-label" transform="translate({}, {})"><foreignObject x="{}" y="{}" width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; text-align: center;"><span class="nodeLabel" {}>{}</span></div></foreignObject></g>"#,
            fmt(title_x),
            fmt(title_y),
            fmt(-title_width / 2.0),
            fmt(-title_height / 2.0),
            fmt(title_width),
            fmt(title_height),
            text_style_attr,
            er_subgraph_label_fragment(subgraph),
        );
    } else {
        let _ = write!(
            out,
            r#"<g class="cluster-label" transform="translate({}, {})" {}>"#,
            fmt(title_x),
            fmt(title_y - title_height / 2.0),
            text_style_attr.replace("color:", "fill:"),
        );
        if subgraph.label_type == "string" || subgraph.label_type == "text" {
            super::super::label::write_svg_text_centered_from_create_text_source_with_style(
                out,
                &subgraph.title,
                &style_decls_with_important(&text_styles).replace("color:", "fill:"),
            );
        } else {
            super::super::label::write_svg_text_markdown_wrapped_centered_with_style(
                out,
                &subgraph.title,
                &style_decls_with_important(&text_styles).replace("color:", "fill:"),
                context.measurer,
                context.label_style,
                None,
            );
        }
        out.push_str("</g>");
    }
    out.push_str("</g>");
    out.push('\n');
}

fn style_decls_with_important_join(decls: &[String], join: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for d in decls {
        let Some((k, v)) = parse_style_decl(d) else {
            continue;
        };
        out.push(format!("{k}:{v} !important"));
    }
    out.join(join)
}

fn render_er_subgraph_clusters(
    out: &mut String,
    clusters: &[crate::model::LayoutCluster],
    model: &merman_core::diagrams::er::ErDiagramRenderModel,
    context: ErSubgraphRenderContext<'_>,
) {
    for cluster in clusters {
        let Some(subgraph) = model.subgraphs.iter().find(|item| item.id == cluster.id) else {
            continue;
        };
        render_er_subgraph_cluster(out, cluster, subgraph, context);
    }
}

fn style_decls_with_important(decls: &[String]) -> String {
    style_decls_with_important_join(decls, "; ")
}

fn last_style_value(decls: &[String], key: &str) -> Option<String> {
    for d in decls.iter().rev() {
        let Some((k, v)) = parse_style_decl(d) else {
            continue;
        };
        if k == key {
            return Some(v.to_string());
        }
    }
    None
}

fn concat_style_keys(decls: &[String], keys: &[&str]) -> String {
    let mut out = String::new();
    for k in keys {
        if let Some(v) = last_style_value(decls, k) {
            out.push_str(k);
            out.push(':');
            out.push_str(&v);
        }
    }
    out
}

fn parse_px_f64(v: &str) -> Option<f64> {
    let raw = v.trim().trim_end_matches(';').trim();
    let raw = raw.trim_end_matches("px").trim();
    if raw.is_empty() {
        return None;
    }
    raw.parse::<f64>().ok()
}

pub(crate) fn render_er_diagram_svg_model(
    layout: &ErDiagramLayout,
    model: &merman_core::diagrams::er::ErDiagramRenderModel,
    effective_config: &serde_json::Value,
    diagram_title: Option<&str>,
    measurer: &dyn TextMeasurer,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("merman");
    // Mermaid's internal diagram type for ER is `er` (not `erDiagram`), and marker ids are derived
    // from this type (e.g. `<diagramId>_er-zeroOrMoreEnd`).
    let diagram_type = "er";
    let er_render_settings = crate::er::ErConfigView::new(effective_config).render_settings();
    let is_elk_layout = er_render_settings.is_elk_layout;
    let data_look = er_render_settings.diagram_look.as_str();
    let redux_color_theme = is_er_redux_color_theme(effective_config);
    let svg_theme = SvgTheme::new(effective_config);
    let redux_border_colors = if redux_color_theme {
        svg_theme.string_array("borderColorArray")
    } else {
        Vec::new()
    };
    let redux_background_colors = if redux_color_theme {
        svg_theme.string_array("bkgColorArray")
    } else {
        Vec::new()
    };
    let color_indices = er_color_indices(model);

    // Mermaid's computed theme variables are not currently present in `effective_config`.
    // Use Mermaid default theme fallbacks so Stage-B SVGs match upstream defaults more closely.
    let _stroke = theme_token(effective_config, "lineColor", "#333333");
    let node_border = theme_token(effective_config, "nodeBorder", "#9370DB");
    let main_bkg = theme_token(effective_config, "mainBkg", "#ECECFF");
    let _tertiary = theme_token(
        effective_config,
        "tertiaryColor",
        "hsl(80, 100%, 96.2745098039%)",
    );
    let text_color = theme_token(effective_config, "textColor", "#333333");
    let _node_text_color = theme_token(effective_config, "nodeTextColor", &text_color);
    let font_family = er_render_settings.font_family.clone();
    let font_size = er_render_settings.font_size;
    let title_top_margin = er_render_settings.title_top_margin;
    let use_max_width = er_render_settings.use_max_width;
    let label_style = er_render_settings.label_style.clone();
    let edge_html_labels = er_render_settings.relationship_html_labels;
    let entity_wrap_mode = er_render_settings.entity_html_label_wrap_mode;
    let entity_measurement = er_render_settings.entity_measurement;
    let hand_drawn_seed =
        options.rough_randomness(er_render_settings.hand_drawn_seed, "render.er.roughjs");
    let insert_title_top_margin = er_render_settings.insert_title_top_margin;
    fn parse_trailing_index(id: &str) -> Option<i64> {
        let (_, tail) = id.rsplit_once('-')?;
        tail.parse::<i64>().ok()
    }
    fn er_node_sort_key(id: &str) -> (i64, i64) {
        if id.contains("---") {
            return (1, parse_trailing_index(id).unwrap_or(i64::MAX));
        }
        (0, parse_trailing_index(id).unwrap_or(i64::MAX))
    }

    let mut nodes = layout.nodes.clone();
    nodes.sort_by_key(|n| er_node_sort_key(&n.id));

    // Layout keeps Dagre's helper segments for reproducible layout artifacts. The family
    // preparation path also carries Mermaid's SVG-facing projection, where self-loops are
    // already merged into one visible edge. Fall back to the canonical edges for hand-built
    // layouts used by unit tests and older callers.
    let mut edges = if layout.render_edges.is_empty() {
        layout.edges.clone()
    } else {
        layout.render_edges.clone()
    };
    fn er_edge_sort_key(edge: &crate::model::LayoutEdge) -> (i64, i64, i64) {
        // Mermaid's getEdgesToRender() appends merged self-loops after ordinary graph edges.
        let self_loop = i64::from(edge.from == edge.to);
        let id = edge.id.as_str();
        let Some(rest) = id.strip_prefix("er-rel-") else {
            return (self_loop, i64::MAX, i64::MAX);
        };
        let mut digits_len = 0usize;
        for ch in rest.chars() {
            if !ch.is_ascii_digit() {
                break;
            }
            digits_len += ch.len_utf8();
        }
        if digits_len == 0 {
            return (self_loop, i64::MAX, i64::MAX);
        }
        let Ok(idx) = rest[..digits_len].parse::<i64>() else {
            return (self_loop, i64::MAX, i64::MAX);
        };
        (self_loop, idx, 0)
    }
    edges.sort_by_key(|edge| {
        let (self_loop, index, secondary) = er_edge_sort_key(edge);
        (if is_elk_layout { 0 } else { self_loop }, index, secondary)
    });

    // Box and Rectpacking intentionally leave sections empty. Resolve their paint geometry
    // before measuring bounds, while retaining provenance to select a linear curve below.
    let mut missing_sections = rustc_hash::FxHashSet::default();
    if is_elk_layout {
        let nodes_by_id: rustc_hash::FxHashMap<_, _> =
            nodes.iter().map(|node| (node.id.as_str(), node)).collect();
        for edge in &mut edges {
            if edge.points.is_empty()
                && let (Some(start), Some(end)) = (
                    nodes_by_id.get(edge.from.as_str()),
                    nodes_by_id.get(edge.to.as_str()),
                )
            {
                edge.points = crate::elk_geometry::missing_rect_section_points(start, end);
                if let Some(label) = &mut edge.label {
                    let first = &edge.points[0];
                    let last = &edge.points[edge.points.len() - 1];
                    label.x = (first.x + last.x) / 2.0;
                    label.y = (first.y + last.y) / 2.0;
                }
                missing_sections.insert(edge.id.clone());
            }
        }
    }

    if is_elk_layout {
        for edge in &edges {
            options
                .work_meter()
                .charge(edge.points.len().saturating_add(2))?;
        }
        if effective_config
            .pointer("/elk/straightenEdges")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        {
            crate::elk_terminal_jogs::straighten_edge_terminals(&mut edges, |units| {
                options.work_meter().charge(units).map_err(Into::into)
            })?;
        }
        crate::elk_terminal_jogs::separate_opposite_edge_labels(
            edges.iter_mut().map(|edge| {
                let has_label = er_rel_idx_from_edge_id(&edge.id)
                    .and_then(|index| model.relationships.get(index))
                    .is_some_and(|relation| !relation.role_a.is_empty());
                (
                    edge.from.as_str(),
                    edge.to.as_str(),
                    if has_label { edge.label.as_mut() } else { None },
                )
            }),
            |units| options.work_meter().charge(units).map_err(Into::into),
        )?;
    }

    let include_md_parent = edges.iter().any(|e| {
        matches!(
            e.start_marker.as_deref(),
            Some("MD_PARENT_START") | Some("MD_PARENT_END")
        ) || matches!(
            e.end_marker.as_deref(),
            Some("MD_PARENT_START") | Some("MD_PARENT_END")
        )
    });

    let diagram_title = diagram_title.map(str::trim).filter(|t| !t.is_empty());
    let is_empty_diagram = nodes.is_empty() && edges.is_empty() && diagram_title.is_none();

    let bounds = compute_layout_bounds(&layout.clusters, &nodes, &edges).unwrap_or({
        if is_empty_diagram {
            Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
            }
        } else {
            Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 100.0,
                max_y: 100.0,
            }
        }
    });

    let mut content_bounds = bounds.clone();
    let diagram_title_x = diagram_title.map(|title| {
        let title_style = crate::text::TextStyle {
            font_family: Some(font_family.clone()),
            font_size,
            font_weight: None,
            font_style: None,
        };
        let (title_left, title_right) = measurer.measure_svg_title_bbox_x(title, &title_style);
        let (title_ascent, title_descent) = crate::text::svg_text_vertical_extents_px(
            measurer,
            title,
            &title_style,
            crate::text::svg_title_bbox_vertical_extents_px(&title_style),
        );
        let w = (content_bounds.max_x - content_bounds.min_x).max(1.0);
        let title_x = content_bounds.min_x + w / 2.0;
        let title_y = -title_top_margin;
        let title_min_x = title_x - title_left;
        let title_max_x = title_x + title_right;
        let title_min_y = title_y - title_ascent;
        let title_max_y = title_y + title_descent;
        content_bounds.min_x = content_bounds.min_x.min(title_min_x);
        content_bounds.max_x = content_bounds.max_x.max(title_max_x);
        content_bounds.min_y = content_bounds.min_y.min(title_min_y);
        content_bounds.max_y = content_bounds.max_y.max(title_max_y);
        title_x
    });

    let pad = options.viewbox_padding.max(0.0);
    let mut out = String::new();
    let (translate_x, translate_y, root_bounds, root_width_for_title) = if is_empty_diagram {
        let empty_span = (pad * 2.0).max(1.0);
        (
            0.0,
            0.0,
            root_svg::DiagramBounds::from_view_box(-pad, -pad, empty_span, empty_span),
            empty_span,
        )
    } else {
        let content_w = (content_bounds.max_x - content_bounds.min_x).max(1.0);
        let content_h = (content_bounds.max_y - content_bounds.min_y).max(1.0);
        let vb_w = content_w + pad * 2.0;
        let vb_h = content_h + pad * 2.0;
        (
            0.0,
            0.0,
            root_svg::DiagramBounds::from_view_box(
                content_bounds.min_x - pad,
                content_bounds.min_y - pad,
                vb_w,
                vb_h,
            ),
            vb_w,
        )
    };
    let root_spec = root_svg::RootViewportSpec::mermaid(root_bounds, use_max_width).with_max_width(
        root_svg::RootMaxWidth::CssSixSignificant(root_width_for_title),
    );
    let root_viewport =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Er, diagram_id);
    let root_plan = root_viewport.plan(root_spec)?;

    let has_acc_title = model.acc_title.as_ref().is_some_and(|s| !s.is_empty());
    let has_acc_descr = model.acc_descr.as_ref().is_some_and(|s| !s.is_empty());
    let aria_labelledby = has_acc_title.then(|| format!("chart-title-{diagram_id}"));
    let aria_describedby = has_acc_descr.then(|| format!("chart-desc-{diagram_id}"));
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, diagram_type);
    root_chrome.class = Some("erDiagram");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    let root_document = root_viewport.write_plan(&mut out, &root_plan, root_chrome)?;
    options.checkpoint_emit()?;

    if has_acc_title {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{}">{}"#,
            diagram_id,
            escape_xml(model.acc_title.as_deref().unwrap_or_default())
        );
        out.push_str("</title>");
    }
    if has_acc_descr {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{}">{}"#,
            diagram_id,
            escape_xml(model.acc_descr.as_deref().unwrap_or_default())
        );
        out.push_str("</desc>");
    }

    let mut css = er_css(diagram_id, effective_config)?;
    let redux_color_css = er_redux_color_css(
        diagram_id,
        data_look,
        &redux_border_colors,
        &redux_background_colors,
    );
    insert_er_redux_color_css(&mut css, diagram_id, &redux_color_css);
    let _ = write!(&mut out, r#"<style>{css}</style>"#);

    // Mermaid wraps diagram content (defs + root) in a single `<g>` element.
    out.push_str("<g>");

    // Mermaid 12 erRenderer-unified selects the themed marker variants only for Neo.
    // Note: ids follow Mermaid marker rules: `${diagramId}_${diagramType}-${markerType}{Start|End}`.
    // Mermaid's ER unified renderer enables four marker types by default; include MD_PARENT only if used.
    let diagram_type_esc = escape_xml(diagram_type);

    // Mermaid emits one `<defs>` wrapper per marker.
    if include_md_parent {
        let _ = writeln!(
            &mut out,
            r#"<defs><marker id="{diagram_id}_{diagram_type_esc}-mdParentStart" class="marker mdParent er" refX="0" refY="7" markerWidth="190" markerHeight="240" orient="auto"><path d="M 18,7 L9,13 L1,7 L9,1 Z"/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-mdParentEnd" class="marker mdParent er" refX="19" refY="7" markerWidth="20" markerHeight="28" orient="auto"><path d="M 18,7 L9,13 L1,7 L9,1 Z"/></marker></defs>"#
        );
    }

    let neo_markers = data_look == "neo";
    let marker_units = if neo_markers {
        r#" markerUnits="userSpaceOnUse""#
    } else {
        ""
    };
    let marker_stroke = if neo_markers {
        let stroke_width = config_string(effective_config, &["themeVariables", "strokeWidth"])
            .or_else(|| {
                effective_config
                    .pointer("/themeVariables/strokeWidth")
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| "undefined".to_owned());
        format!(r#" stroke-width="{}""#, escape_xml(&stroke_width))
    } else {
        String::new()
    };
    let marker_background = if neo_markers {
        effective_config
            .pointer("/themeVariables/mainBkg")
            .filter(|value| !value.is_null())
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string())
            })
            .unwrap_or_else(|| "white".to_owned())
    } else {
        "white".to_owned()
    };
    let marker_background = escape_xml(&marker_background);
    let zero_more_start_x = if neo_markers { "45.5" } else { "48" };
    let zero_more_end_x = if neo_markers { "11" } else { "9" };

    let _ = writeln!(
        &mut out,
        r#"<defs><marker id="{diagram_id}_{diagram_type_esc}-onlyOneStart" class="marker onlyOne er" refX="0" refY="9" markerWidth="18" markerHeight="18" orient="auto"{marker_units}><path d="M9,0 L9,18 M15,0 L15,18"{marker_stroke}/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-onlyOneEnd" class="marker onlyOne er" refX="18" refY="9" markerWidth="18" markerHeight="18" orient="auto"{marker_units}><path d="M3,0 L3,18 M9,0 L9,18"{marker_stroke}/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-zeroOrOneStart" class="marker zeroOrOne er" refX="0" refY="9" markerWidth="30" markerHeight="18" orient="auto"{marker_units}><circle fill="{marker_background}" cx="21" cy="9" r="6"{marker_stroke}/><path d="M9,0 L9,18"{marker_stroke}/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-zeroOrOneEnd" class="marker zeroOrOne er" refX="30" refY="9" markerWidth="30" markerHeight="18" orient="auto"{marker_units}><circle fill="{marker_background}" cx="9" cy="9" r="6"{marker_stroke}/><path d="M21,0 L21,18"{marker_stroke}/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-oneOrMoreStart" class="marker oneOrMore er" refX="18" refY="18" markerWidth="45" markerHeight="36" orient="auto"{marker_units}><path d="M0,18 Q 18,0 36,18 Q 18,36 0,18 M42,9 L42,27"{marker_stroke}/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-oneOrMoreEnd" class="marker oneOrMore er" refX="27" refY="18" markerWidth="45" markerHeight="36" orient="auto"{marker_units}><path d="M3,9 L3,27 M9,18 Q27,0 45,18 Q27,36 9,18"{marker_stroke}/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-zeroOrMoreStart" class="marker zeroOrMore er" refX="18" refY="18" markerWidth="57" markerHeight="36" orient="auto"{marker_units}><circle fill="{marker_background}" cx="{zero_more_start_x}" cy="18" r="6"{marker_stroke}/><path d="M0,18 Q18,0 36,18 Q18,36 0,18"{marker_stroke}/></marker></defs>
<defs><marker id="{diagram_id}_{diagram_type_esc}-zeroOrMoreEnd" class="marker zeroOrMore er" refX="39" refY="18" markerWidth="57" markerHeight="36" orient="auto"{marker_units}><circle fill="{marker_background}" cx="{zero_more_end_x}" cy="18" r="6"{marker_stroke}/><path d="M21,18 Q39,0 57,18 Q39,36 21,18"{marker_stroke}/></marker></defs>"#
    );
    options.checkpoint_emit()?;

    let mut entity_by_id: std::collections::HashMap<&str, &crate::er::ErEntity> =
        std::collections::HashMap::new();
    for e in model.entities.values() {
        entity_by_id.insert(e.id.as_str(), e);
    }

    fn er_rel_idx_from_edge_id(edge_id: &str) -> Option<usize> {
        let rest = edge_id.strip_prefix("er-rel-")?;
        let mut digits_len = 0usize;
        for ch in rest.chars() {
            if !ch.is_ascii_digit() {
                break;
            }
            digits_len += ch.len_utf8();
        }
        if digits_len == 0 {
            return None;
        }
        rest[..digits_len].parse::<usize>().ok()
    }

    fn er_edge_dom_id(edge_id: &str, relationships: &[crate::er::ErRelationship]) -> String {
        let Some(idx) = er_rel_idx_from_edge_id(edge_id) else {
            return edge_id.to_string();
        };
        let Some(rel) = relationships.get(idx) else {
            return edge_id.to_string();
        };
        format!("id_{}_{}_{}", rel.entity_a, rel.entity_b, idx)
    }

    let subgraph_context = ErSubgraphRenderContext {
        diagram_id,
        data_look,
        classes: &model.classes,
        use_html_labels: edge_html_labels,
        measurer,
        label_style: &label_style,
        translate_x,
        translate_y,
    };

    // Mermaid 12 keeps edges above clusters and below nodes for every layout.
    let _ = writeln!(&mut out, r#"<g class="root">"#);
    if layout.clusters.is_empty() {
        out.push_str(r#"<g class="clusters"/>"#);
    } else {
        out.push_str(r#"<g class="clusters">"#);
        render_er_subgraph_clusters(&mut out, &layout.clusters, model, subgraph_context);
        out.push_str("</g>");
    }

    if is_elk_layout {
        out.push_str(r#"<g class="edges edgePaths">"#);
    } else {
        out.push_str(r#"<g class="edgePaths">"#);
    }
    let line_hops_enabled = is_elk_layout
        && options.debug.include_edges
        && effective_config
            .pointer("/elk/lineHops")
            .and_then(serde_json::Value::as_bool)
            != Some(false);
    let shifted_hop_points: Vec<Vec<crate::model::LayoutPoint>> = if line_hops_enabled {
        options
            .work_meter()
            .charge(edges.iter().fold(0usize, |work, edge| {
                work.saturating_add(edge.points.len()).saturating_add(1)
            }))?;
        edges
            .iter()
            .map(|edge| {
                edge.points
                    .iter()
                    .map(|point| crate::model::LayoutPoint {
                        x: point.x + translate_x,
                        y: point.y + translate_y,
                    })
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    let hop_paths = if line_hops_enabled {
        options.work_meter().charge(edges.len())?;
        let hop_edges: Vec<_> = edges
            .iter()
            .zip(&shifted_hop_points)
            .map(|(edge, points)| {
                super::super::line_hops::LineHopEdge {
                    id: edge.id.as_str(),
                    points,
                    curve: Some(if missing_sections.contains(&edge.id) {
                        "linear"
                    } else {
                        "rounded"
                    }),
                    // ER cardinality markers do not shorten the routed path terminals.
                    arrow_type_start: None,
                    arrow_type_end: None,
                }
            })
            .collect();
        super::super::line_hops::elk_line_hop_paths(
            effective_config,
            &hop_edges,
            options.work_meter(),
        )?
    } else {
        std::collections::HashMap::new()
    };
    if options.debug.include_edges {
        for e in &edges {
            let missing_section = missing_sections.contains(&e.id);
            if e.points.is_empty() || (!missing_section && e.points.len() < 2) {
                continue;
            }
            let edge_dom_id = er_edge_dom_id(&e.id, &model.relationships);
            let edge_svg_id = format!("{diagram_id}-{edge_dom_id}");
            options.checkpoint_emit()?;
            let is_dashed = e.stroke_dasharray.as_deref() == Some("8,8");
            let pattern_class = if is_dashed {
                "edge-pattern-dashed"
            } else {
                "edge-pattern-solid"
            };
            let line_classes = format!("edge-thickness-normal {pattern_class} relationshipLine");
            let shifted: Vec<crate::model::LayoutPoint> = e
                .points
                .iter()
                .map(|p| crate::model::LayoutPoint {
                    x: p.x + translate_x,
                    y: p.y + translate_y,
                })
                .collect();
            let data_points = base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec(&shifted).unwrap_or_default());
            let original_d = er_edge_path_d(&shifted, is_elk_layout, missing_section);
            let hopped_d = hop_paths.get(e.id.as_str()).map(String::as_str);
            let d = hopped_d.unwrap_or(&original_d);

            let mut edge_style = String::new();
            if data_look == "neo"
                && let Some(length) = super::super::svg_path_length_from_d(&original_d)
            {
                super::super::edge_path::write_neo_edge_mask(
                    &mut edge_style,
                    length,
                    None,
                    None,
                    is_dashed,
                    false,
                );
            }
            // ELK's buildEdgeData supplies fill:none when the ER model has no edge style.
            edge_style.push_str(if is_elk_layout {
                "fill:none;;;fill:none"
            } else {
                "undefined;;;undefined"
            });
            let edge_style = if let Some(hopped_d) = hopped_d {
                super::super::line_hops::rewrite_style_after_line_hop(
                    &edge_style,
                    hopped_d,
                    options.work_meter(),
                )?
            } else {
                std::borrow::Cow::Borrowed(edge_style.as_str())
            };

            let _ = write!(
                &mut out,
                r#"<path d="{}" id="{}" class="{}" style="{}"#,
                escape_xml(d),
                escape_xml(&edge_svg_id),
                escape_xml(&line_classes),
                escape_xml(&edge_style),
            );
            let _ = write!(
                &mut out,
                r#"" data-edge="true" data-et="edge" data-id="{}" data-points="{}" data-look="{}""#,
                escape_xml(&edge_dom_id),
                escape_xml(&data_points),
                escape_xml(data_look)
            );
            if let Some(m) = &e.start_marker {
                let marker = er_unified_marker_id(diagram_id, diagram_type, m);
                let _ = write!(&mut out, r#" marker-start="url(#{})""#, escape_xml(&marker));
            }
            if let Some(m) = &e.end_marker {
                let marker = er_unified_marker_id(diagram_id, diagram_type, m);
                let _ = write!(&mut out, r#" marker-end="url(#{})""#, escape_xml(&marker));
            }
            options.checkpoint_emit()?;
            out.push_str(" />");
        }
    }
    out.push_str("</g>");

    out.push_str(r#"<g class="edgeLabels">"#);
    if options.debug.include_edges {
        for e in &edges {
            let rel_idx = er_rel_idx_from_edge_id(&e.id)
                .and_then(|idx| model.relationships.get(idx).map(|r| (idx, r)));

            let rel_text_raw = rel_idx.map(|(_, r)| r.role_a.as_str()).unwrap_or("");
            let rel_text = rel_text_raw.trim();
            let edge_dom_id = er_edge_dom_id(&e.id, &model.relationships);

            // Mermaid's shared renderer checks `Boolean(edge.label)`: an empty role has no
            // label wrapper, while whitespace remains a real (zero-size) label.
            if rel_text_raw.is_empty() {
                continue;
            }
            let has_label_text = !rel_text.is_empty();
            let (w, h, mut cx, mut cy) = if has_label_text {
                if let Some(lbl) = &e.label {
                    (
                        lbl.width.max(0.0),
                        lbl.height.max(0.0),
                        lbl.x + translate_x,
                        lbl.y + translate_y,
                    )
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                }
            } else {
                let (x, y) = e
                    .label
                    .as_ref()
                    .map(|label| (label.x + translate_x, label.y + translate_y))
                    .or_else(|| {
                        super::super::edge_label_geometry::calc_label_position(&e.points)
                            .map(|point| (point.x + translate_x, point.y + translate_y))
                    })
                    .unwrap_or((0.0, 0.0));
                (0.0, 0.0, x, y)
            };

            if has_label_text && w > 0.0 && h > 0.0 && !missing_sections.contains(&e.id) {
                // Mermaid 12.1 preserves the layout anchor and adds only the midpoint delta
                // from paint-time clipping. ER has no additional clipping here: curve and marker
                // projection change `d`, while both label polylines remain the layout route.
                let shifted: Vec<crate::model::LayoutPoint> = e
                    .points
                    .iter()
                    .map(|p| crate::model::LayoutPoint {
                        x: p.x + translate_x,
                        y: p.y + translate_y,
                    })
                    .collect();
                let rendered_d = er_edge_path_d(&shifted, is_elk_layout, false);
                let position = super::super::edge_label_geometry::position_edge_label(
                    crate::model::LayoutPoint { x: cx, y: cy },
                    Some(&shifted),
                    &shifted,
                    &rendered_d,
                    false,
                );
                cx = position.x;
                cy = position.y;
            }

            if has_label_text && w > 0.0 && h > 0.0 {
                // Mermaid ER relationship labels follow Mermaid's effective HTML-label
                // resolution: root `htmlLabels`, then `flowchart.htmlLabels`, then default
                // `true`. When both are unset, upstream still emits HTML `<foreignObject>`
                // labels through `createText(...)`.
                let _ = write!(
                    &mut out,
                    r#"<g class="edgeLabel" transform="translate({}, {})">"#,
                    fmt(cx),
                    fmt(cy)
                );
                let _ = write!(
                    &mut out,
                    r#"<g class="label" data-id="{}" transform="translate({}, {})">"#,
                    escape_xml_display(&edge_dom_id),
                    fmt(-w / 2.0),
                    fmt(-h / 2.0)
                );
                if edge_html_labels {
                    let _ = write!(
                        &mut out,
                        r#"<foreignObject width="{}" height="{}">"#,
                        fmt(w),
                        fmt(h)
                    );
                    out.push_str(r#"<div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;"><span class="edgeLabel">"#);
                    // Mermaid ER relationship labels use the generic HTML edge-label path, so they
                    // inherit `markdownToHTML()` semantics (Markdown emphasis + inline `<br/>`
                    // handling) rather than rendering literal `**...**` marker text.
                    let html =
                        crate::text::mermaid_markdown_to_xhtml_label_fragment(rel_text, true);
                    out.push_str(&html);
                    out.push_str(r#"</span></div></foreignObject></g></g>"#);
                } else {
                    out.push_str("<g>");
                    let _ = write!(
                        &mut out,
                        r#"<rect class="background" style="" x="{}" y="-1" width="{}" height="{}"/>"#,
                        fmt(-w / 2.0),
                        fmt(w),
                        fmt(h)
                    );
                    crate::svg::parity::label::write_svg_text_centered_from_create_text_source(
                        &mut out, rel_text, true,
                    );
                    out.push_str("</g></g></g>");
                }
            } else {
                if edge_html_labels {
                    // Whitespace is truthy to Mermaid's `hasEdgeLabel`, so it receives a
                    // zero-size wrapper at the measured edge position. Empty strings returned
                    // above before reaching this branch.
                    let _ = write!(
                        &mut out,
                        r#"<g class="edgeLabel" transform="translate({}, {})"><g class="label""#,
                        fmt(cx),
                        fmt(cy)
                    );
                    let _ = write!(
                        &mut out,
                        r#" data-id="{}""#,
                        escape_xml_display(&edge_dom_id)
                    );
                    out.push_str(r#" transform="translate(0, 0)"><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;"><span class="edgeLabel"></span></div></foreignObject></g></g>"#);
                } else {
                    let _ = write!(
                        &mut out,
                        r#"<g class="edgeLabel" transform="translate({}, {})"><g class="label""#,
                        fmt(cx),
                        fmt(cy)
                    );
                    let _ = write!(
                        &mut out,
                        r#" data-id="{}""#,
                        escape_xml_display(&edge_dom_id)
                    );
                    out.push_str(r#" transform="translate(0, 0)"><g><rect class="background" style="" x="0" y="-1" width="0" height="0"/>"#);
                    crate::svg::parity::label::write_svg_text_centered_from_create_text_source(
                        &mut out, "", true,
                    );
                    out.push_str("</g></g></g>");
                }
            }
        }
    }
    out.push_str("</g>\n");

    // Entities drawn after relationships so they cover markers when overlapping.
    out.push_str(r#"<g class="nodes">"#);
    for n in &nodes {
        if n.is_cluster {
            continue;
        }
        let Some(entity) = entity_by_id.get(n.id.as_str()).copied() else {
            if n.id.contains("---") {
                let cx = n.x + translate_x;
                let cy = n.y + translate_y;
                let _ = write!(
                    &mut out,
                    r#"<g class="label edgeLabel" id="{}" transform="translate({}, {})">"#,
                    escape_xml(&n.id),
                    fmt(cx),
                    fmt(cy)
                );
                out.push_str(r#"<rect width="0.1" height="0.1"/>"#);
                out.push_str(r#"<g class="label" style="" transform="translate(0, 0)"><rect/>"#);
                if entity_wrap_mode == crate::text::WrapMode::HtmlLike {
                    out.push_str(r#"<foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 10px; text-align: center;"><span class="nodeLabel"></span></div></foreignObject>"#);
                } else {
                    super::super::label::write_svg_text_markdown_from_create_text_source(
                        &mut out, "", true,
                    );
                }
                out.push_str("</g></g>");
            }
            continue;
        };

        let (rect_style_decls, text_style_decls) = compile_er_entity_styles(entity, &model.classes);
        let rect_style_attr = if rect_style_decls.is_empty() {
            r#"style="""#.to_string()
        } else {
            format!(
                r#"style="{}""#,
                escape_xml(&style_decls_with_important(&rect_style_decls))
            )
        };
        let label_style_attr = if text_style_decls.is_empty() {
            r#"style="""#.to_string()
        } else {
            format!(
                r#"style="{}""#,
                escape_xml(&if entity_wrap_mode == crate::text::WrapMode::HtmlLike {
                    style_decls_with_important(&text_style_decls)
                } else {
                    style_decls_with_important(&text_style_decls).replace("color:", "fill:")
                })
            )
        };

        let measure = layout
            .prepared_entities
            .get(&n.id)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing prepared ER entity geometry for {}", n.id),
            })?;
        let w = n.width.max(1.0);
        let h = n.height.max(1.0);
        if (measure.width - w).abs() > 1e-3 || (measure.height - h).abs() > 1e-3 {
            return Err(Error::InvalidModel {
                message: format!(
                    "ER entity measured size mismatch for {}: layout=({},{}), measure=({}, {})",
                    n.id, w, h, measure.width, measure.height
                ),
            });
        }

        let cx = n.x + translate_x;
        let cy = n.y + translate_y;
        let ox = -w / 2.0;
        let oy = -h / 2.0;

        let group_class = if entity.css_classes.trim().is_empty() {
            "node".to_string()
        } else {
            format!("node {}", entity.css_classes.trim())
        };
        let color_id_attr = color_indices
            .get(entity.id.as_str())
            .and_then(|index| er_redux_color_id(&redux_border_colors, *index))
            .map(|color_id| format!(r#" data-color-id="{}""#, escape_xml(&color_id)))
            .unwrap_or_default();
        let _ = write!(
            &mut out,
            r#"<g id="{}-{}" class="{}" data-look="{}"{} transform="translate({}, {})">"#,
            diagram_id,
            escape_xml(&entity.id),
            escape_xml(&group_class),
            escape_xml(data_look),
            color_id_attr,
            fmt(cx),
            fmt(cy)
        );
        options.checkpoint_emit()?;

        if entity.attributes.is_empty() {
            let _ = write!(
                &mut out,
                r#"<rect class="basic label-container" {} x="{}" y="{}" width="{}" height="{}"/>"#,
                rect_style_attr,
                fmt(ox),
                fmt(oy),
                fmt(w),
                fmt(h)
            );
            let lw = measure.label_html_width.max(0.0);
            let lh = measure.label_height.max(0.0);
            let _ = write!(
                &mut out,
                r#"<g class="label" transform="translate({}, {})" {}><rect/>"#,
                fmt(-lw / 2.0),
                fmt(-lh / 2.0),
                label_style_attr,
            );
            if entity_wrap_mode == crate::text::WrapMode::HtmlLike {
                let _ = write!(
                    &mut out,
                    r#"<foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center;">{}</div></foreignObject>"#,
                    fmt(lw),
                    fmt(lh),
                    measure.label_max_width_px.max(0),
                    html_label_content(&measure.label, "", true),
                );
            } else {
                super::super::label::write_svg_text_markdown_from_create_text_source(
                    &mut out,
                    measure.label.markdown_input(),
                    true,
                );
            }
            out.push_str("</g>");
            out.push_str("</g>");
            continue;
        }

        fn html_label_content(
            label: &crate::er::ErBoxLabel,
            span_style_attr: &str,
            markdown_node_label: bool,
        ) -> String {
            let span_class = if markdown_node_label {
                "nodeLabel markdown-node-label"
            } else {
                "nodeLabel"
            };
            let text = label.rendered_text();
            if text.is_empty() {
                return format!(r#"<span class="{}"{}></span>"#, span_class, span_style_attr);
            }

            if label.uses_generic_workaround() {
                return escape_xml(text);
            }

            format!(
                r#"<span class="{}"{}>{}</span>"#,
                span_class,
                span_style_attr,
                label.xhtml_fragment()
            )
        }

        let label_div_color_prefix = last_style_value(&text_style_decls, "color")
            .map(|value| {
                format!(
                    "color: {} !important; ",
                    super::super::util::cssom_color_value(&value)
                )
            })
            .unwrap_or_default();
        let span_style_attr = if text_style_decls.is_empty() {
            String::new()
        } else {
            format!(
                r#" style="{}""#,
                escape_xml(&style_decls_with_important(&text_style_decls))
            )
        };

        // Mermaid erBox.ts uses the configured label mode and paths for the table rows.
        let name_row_h = (measure.label_height + measure.text_padding).max(1.0);
        let box_x0 = ox;
        let box_y0 = oy;
        let box_x1 = ox + w;
        let box_y1 = oy + h;
        let sep_y = oy + name_row_h;

        let box_fill =
            last_style_value(&rect_style_decls, "fill").unwrap_or_else(|| main_bkg.clone());
        let box_stroke =
            last_style_value(&rect_style_decls, "stroke").unwrap_or_else(|| node_border.clone());
        let box_stroke_width = last_style_value(&rect_style_decls, "stroke-width")
            .and_then(|v| parse_px_f64(&v))
            .unwrap_or(1.3)
            .max(0.0);

        let stroke_width_attr = fmt(box_stroke_width);

        let group_style = concat_style_keys(&rect_style_decls, &["fill", "stroke", "stroke-width"]);
        let group_style_attr = if group_style.is_empty() {
            r#"style="""#.to_string()
        } else {
            format!(r#"style="{}""#, escape_xml(&group_style))
        };

        // Mermaid 12 lets explicit styles override every Redux table path, including
        // palette colors. Other themes retain themed rows except for even-row overrides.
        let redux_styles = matches!(
            svg_theme.theme_name().as_str(),
            "redux" | "redux-dark" | "redux-color" | "redux-dark-color"
        );
        let override_decls: Vec<String> = rect_style_decls
            .iter()
            .filter(|declaration| {
                redux_styles
                    || parse_style_decl(declaration).is_some_and(|(key, _)| key.contains("stroke"))
            })
            .cloned()
            .collect();
        let override_style_attr = if override_decls.is_empty() || data_look == "handDrawn" {
            String::new()
        } else {
            format!(
                r#" style="{}""#,
                escape_xml(&style_decls_with_important(&override_decls))
            )
        };
        let node_style_attr = if rect_style_decls.is_empty() || data_look == "handDrawn" {
            String::new()
        } else {
            format!(
                r#" style="{}""#,
                escape_xml(&style_decls_with_important_join(&rect_style_decls, ";"))
            )
        };

        // Mermaid erBox.ts uses Rough.js with `roughness=0` for default (non-handDrawn) nodes.
        //
        // Even with roughness=0, Rough.js still depends on seeded randomness via `divergePoint`.
        // For strict SVG parity we use the same Rough.js algorithm (v4.6.6) here instead of a
        // generic sketchy-stroke renderer.
        fn roughjs46_diverge_point(options: &mut roughr::core::Options) -> f64 {
            0.2 + options.random() * 0.2
        }

        fn roughjs46_double_line_path_d(
            options: &mut roughr::core::Options,
            x0: f64,
            y0: f64,
            x1: f64,
            y1: f64,
        ) -> String {
            let mut out = String::new();
            let dx = x1 - x0;
            let dy = y1 - y0;

            for _ in 0..2 {
                let d = roughjs46_diverge_point(options);
                // Rough.js `_line()` continues to call into `_offsetOpt()` even when `roughness=0`
                // (the random terms get multiplied by zero, but the PRNG state still advances).
                //
                // In Rough.js v4.6.6 `_line()` uses:
                // - 2 random() calls for `midDispX/midDispY` offsetOpt
                // - 2 random() calls for moveTo (x1/y1)
                // - 6 random() calls for bcurveTo (cp1/cp2/x2/y2)
                // Total: 10 random() calls after divergePoint.
                for _ in 0..10 {
                    let _ = options.random();
                }
                let cx1 = x0 + dx * d;
                let cy1 = y0 + dy * d;
                let cx2 = x0 + dx * 2.0 * d;
                let cy2 = y0 + dy * 2.0 * d;
                let _ = write!(
                    &mut out,
                    "M{} {} C{} {}, {} {}, {} {} ",
                    x0, y0, cx1, cy1, cx2, cy2, x1, y1
                );
            }

            out.trim_end().to_string()
        }

        fn rough_rect_border_path_d(
            randomness: &roughr::core::RoughRandomness,
            x0: f64,
            y0: f64,
            x1: f64,
            y1: f64,
        ) -> String {
            let w = (x1 - x0).max(0.0);
            let h = (y1 - y0).max(0.0);
            let mut options = roughr::core::OptionsBuilder::default()
                .randomness(randomness.clone())
                .build()
                .expect("ER rough rectangle options must be valid");

            // Rough.js v4.6.6 renderer.rectangle -> polygon -> linearPath:
            //   segments: (x,y)->(x+w,y)->(x+w,y+h)->(x,y+h)->(x,y)
            let mut out = String::new();
            let x2 = x0 + w;
            let y2 = y0 + h;

            let segs = [
                (x0, y0, x2, y0),
                (x2, y0, x2, y2),
                (x2, y2, x0, y2),
                (x0, y2, x0, y0),
            ];
            for (ax, ay, bx, by) in segs {
                let d = roughjs46_double_line_path_d(&mut options, ax, ay, bx, by);
                out.push_str(&d);
                out.push(' ');
            }

            out.trim_end().to_string()
        }

        fn roughjs46_rect_fill_path_d(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
            format!(
                "M{} {} L{} {} L{} {} L{} {}",
                x0, y0, x1, y0, x1, y1, x0, y1
            )
        }

        fn thin_divider_rect_bounds(x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64, f64, f64) {
            let half = 0.00005;
            if (y1 - y0).abs() <= (x1 - x0).abs() {
                (x0, y0 - half, x1, y0 + half)
            } else {
                (x0 - half, y0, x0 + half, y1)
            }
        }

        // Base box (fill + border)
        let _ = write!(&mut out, r#"<g {} class="outer-path">"#, group_style_attr);
        let _ = write!(
            &mut out,
            r#"<path d="{}" stroke="none" stroke-width="0" fill="{}"{} />"#,
            roughjs46_rect_fill_path_d(box_x0, box_y0, box_x1, box_y1),
            escape_xml(&box_fill),
            override_style_attr
        );
        let _ = write!(
            &mut out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"{} />"#,
            rough_rect_border_path_d(&hand_drawn_seed, box_x0, box_y0, box_x1, box_y1),
            escape_xml(&box_stroke),
            stroke_width_attr,
            override_style_attr
        );
        out.push_str("</g>");

        // Row rectangles
        let odd_fill = svg_theme.optional_color("rowOdd");
        let even_fill = svg_theme.optional_color("rowEven");
        let mut y = sep_y;
        for (idx, row) in measure.rows.iter().enumerate() {
            let row_h = row.height.max(1.0);
            let y0 = y;
            let y1 = y + row_h;
            y = y1;
            let is_odd = idx % 2 == 0;
            let row_class = if is_odd {
                "row-rect-odd"
            } else {
                "row-rect-even"
            };
            let row_fill = if is_odd {
                odd_fill.as_deref()
            } else {
                even_fill.as_deref()
            };
            let _ = write!(
                &mut out,
                r#"<g {} class="{}">"#,
                group_style_attr, row_class
            );
            let row_override_style_attr = if is_odd {
                &override_style_attr
            } else {
                &node_style_attr
            };
            // RoughJS omits the fill path when the theme does not supply a row color.
            if let Some(row_fill) = row_fill.filter(|fill| !fill.is_empty() && *fill != "none") {
                let _ = write!(
                    &mut out,
                    r#"<path d="{}" stroke="none" stroke-width="0" fill="{}"{} />"#,
                    roughjs46_rect_fill_path_d(box_x0, y0, box_x1, y1),
                    escape_xml(row_fill),
                    row_override_style_attr
                );
            }
            let _ = write!(
                &mut out,
                r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"{} />"#,
                rough_rect_border_path_d(&hand_drawn_seed, box_x0, y0, box_x1, y1),
                escape_xml(&node_border),
                stroke_width_attr,
                row_override_style_attr
            );
            out.push_str("</g>");
        }

        // ER table labels use createText in both HTML and SVG modes.
        let line_h = if entity_wrap_mode == crate::text::WrapMode::HtmlLike {
            (font_size * 1.5).max(1.0)
        } else {
            measure.label_height.max(1.0)
        };
        let mut pad = entity_measurement.diagram_padding;
        // Keep parity with Mermaid's erBox.ts `if (!config.htmlLabels) { PADDING *= 1.25; }`:
        // when `htmlLabels` is unset (undefined), upstream still applies the 1.25 multiplier.
        if !entity_measurement.html_labels_raw {
            pad *= 1.25;
        }

        let name_w = measure.label_html_width.max(0.0);
        let name_x = -name_w / 2.0;
        let name_y = oy + name_row_h / 2.0 - line_h / 2.0;
        let name_mw_px = measure.label_text_width_px + 100;
        let _ = write!(
            &mut out,
            r#"<g class="label name" transform="translate({}, {})" {}>"#,
            fmt(name_x),
            fmt(name_y),
            label_style_attr,
        );
        if entity_wrap_mode == crate::text::WrapMode::HtmlLike {
            let _ = write!(
                &mut out,
                r#"<foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: start;">{}</div></foreignObject>"#,
                fmt(name_w),
                fmt(line_h),
                escape_xml(&label_div_color_prefix),
                name_mw_px.max(0),
                html_label_content(&measure.label, &span_style_attr, false),
            );
        } else {
            super::super::label::write_svg_text_markdown_from_create_text_source(
                &mut out,
                measure.label.markdown_input(),
                true,
            );
        }
        out.push_str("</g>");

        let type_col_w = measure.type_col_w.max(0.0);
        let name_col_w = measure.name_col_w.max(0.0);
        let key_col_w = measure.key_col_w.max(0.0);
        let _comment_col_w = measure.comment_col_w.max(0.0);

        let left_text_x = ox + pad / 2.0;
        let type_left = left_text_x;
        let name_left = left_text_x + type_col_w;
        let key_left = left_text_x + type_col_w + name_col_w;
        let comment_left = left_text_x + type_col_w + name_col_w + key_col_w;

        let mut row_top = sep_y;
        for row in &measure.rows {
            let row_h = row.height.max(1.0);
            let cell_y = if entity_wrap_mode == crate::text::WrapMode::HtmlLike {
                row_top + row_h / 2.0 - line_h / 2.0
            } else {
                row_top + measure.text_padding / 2.0
            };

            for (index, (class, label, left)) in [
                ("attribute-type", &row.type_label, type_left),
                ("attribute-name", &row.name_label, name_left),
                ("attribute-keys", &row.key_label, key_left),
                ("attribute-comment", &row.comment_label, comment_left),
            ]
            .into_iter()
            .enumerate()
            {
                let _ = write!(
                    &mut out,
                    r#"<g class="label {}" transform="translate({}, {})" {}>"#,
                    class,
                    fmt(left),
                    fmt(cell_y),
                    label_style_attr,
                );
                if entity_wrap_mode == crate::text::WrapMode::HtmlLike {
                    let max_width = row.cell_max_widths_px[index];
                    let _ = write!(
                        &mut out,
                        r#"<foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: start;">{}</div></foreignObject>"#,
                        fmt(row.cell_widths[index]),
                        fmt(
                            if matches!(class, "attribute-keys" | "attribute-comment")
                                && label.rendered_text().is_empty()
                            {
                                0.0
                            } else {
                                line_h
                            }
                        ),
                        escape_xml(&label_div_color_prefix),
                        max_width.max(0),
                        html_label_content(label, &span_style_attr, false),
                    );
                } else {
                    super::super::label::write_svg_text_markdown_from_create_text_source(
                        &mut out,
                        label.markdown_input(),
                        true,
                    );
                }
                out.push_str("</g>");
            }

            row_top += row_h;
        }

        // Dividers (header separator + column boundaries)
        let divider_style = override_style_attr.clone();
        let divider_path_attrs = format!(
            r#" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"{}"#,
            escape_xml(&box_stroke),
            stroke_width_attr,
            divider_style
        );
        // Mermaid `erBox.ts` draws the header separator twice:
        // - once as the explicit "Name line"
        // - once via the later `yOffsets` pass (which always contains `0`)
        #[allow(clippy::too_many_arguments)]
        fn write_divider_group(
            out: &mut String,
            hand_drawn_seed: &roughr::core::RoughRandomness,
            x0: f64,
            y0: f64,
            x1: f64,
            y1: f64,
            fill: &str,
            fill_style_attr: &str,
            divider_path_attrs: &str,
        ) {
            let (rx0, ry0, rx1, ry1) = thin_divider_rect_bounds(x0, y0, x1, y1);
            let _ = write!(
                out,
                r#"<g class="divider"><path d="{}" stroke="none" stroke-width="0" fill="{}" fill-rule="evenodd"{}/><path d="{}"{} /></g>"#,
                roughjs46_rect_fill_path_d(rx0, ry0, rx1, ry1),
                escape_xml(fill),
                fill_style_attr,
                rough_rect_border_path_d(hand_drawn_seed, rx0, ry0, rx1, ry1),
                divider_path_attrs
            );
        }

        write_divider_group(
            &mut out,
            &hand_drawn_seed,
            box_x0,
            sep_y,
            box_x1,
            sep_y,
            &box_fill,
            &override_style_attr,
            &divider_path_attrs,
        );

        let mut divider_xs: Vec<f64> = Vec::new();
        divider_xs.push(ox + type_col_w);
        if measure.has_key {
            divider_xs.push(ox + type_col_w + name_col_w);
        }
        if measure.has_comment {
            divider_xs.push(ox + type_col_w + name_col_w + key_col_w);
        }
        for x in divider_xs {
            write_divider_group(
                &mut out,
                &hand_drawn_seed,
                x,
                sep_y,
                x,
                box_y1,
                &box_fill,
                &override_style_attr,
                &divider_path_attrs,
            );
        }

        write_divider_group(
            &mut out,
            &hand_drawn_seed,
            box_x0,
            sep_y,
            box_x1,
            sep_y,
            &box_fill,
            &override_style_attr,
            &divider_path_attrs,
        );

        out.push_str("</g>");
    }
    out.push_str("</g>\n");
    out.push_str("</g>\n</g>\n");

    push_er_gradient(&mut out, diagram_id, effective_config);

    if let Some(title) = diagram_title {
        // Mermaid `utils.insertTitle(...)` appends the title after rendering the graph content.
        // - `text-anchor="middle"`
        // - `x = bounds.x + bounds.width / 2`
        // - `y = -titleTopMargin` (default: 25)
        let fallback_title_x = root_plan
            .view_box()
            .map(|view_box| view_box.min_x + view_box.width / 2.0)
            .unwrap_or(root_width_for_title / 2.0);
        let _ = write!(
            &mut out,
            r#"<text text-anchor="middle" x="{}" y="{}" class="erDiagramTitleText">{}"#,
            fmt(diagram_title_x.unwrap_or(fallback_title_x)),
            fmt(-insert_title_top_margin),
            escape_xml(title)
        );
        out.push_str("</text>\n");
    }

    push_er_shadow_defs(&mut out, diagram_id, effective_config);

    out.push_str("</svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

fn push_er_shadow_defs(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config_value: &serde_json::Value,
) {
    let flood_color = effective_config_value
        .get("theme")
        .and_then(|v| v.as_str())
        .filter(|theme| theme.contains("dark"))
        .map(|_| "#FFFFFF")
        .unwrap_or("#000000");
    let _ = write!(
        out,
        r#"<defs><filter id="{}-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs><defs><filter id="{}-drop-shadow-small" height="150%" width="150%"><feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs>"#,
        diagram_id, flood_color, diagram_id, flood_color
    );
}

fn push_er_gradient(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config_value: &serde_json::Value,
) {
    if !config_bool(effective_config_value, &["themeVariables", "useGradient"]).unwrap_or(false) {
        return;
    }

    let gradient_start =
        config_string(effective_config_value, &["themeVariables", "gradientStart"])
            .or_else(|| {
                config_string(
                    effective_config_value,
                    &["themeVariables", "primaryBorderColor"],
                )
            })
            .unwrap_or_else(|| "#9370DB".to_string());
    let gradient_stop = config_string(effective_config_value, &["themeVariables", "gradientStop"])
        .or_else(|| {
            config_string(
                effective_config_value,
                &["themeVariables", "secondaryBorderColor"],
            )
        })
        .unwrap_or_else(|| gradient_start.clone());

    let gradient_start = escape_xml(&gradient_start);
    let gradient_stop = escape_xml(&gradient_stop);
    let _ = write!(
        out,
        r#"<linearGradient id="{}-gradient" gradientUnits="objectBoundingBox" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" stop-color="{}" stop-opacity="1"/><stop offset="100%" stop-color="{}" stop-opacity="1"/></linearGradient>"#,
        diagram_id,
        gradient_start.as_str(),
        gradient_stop.as_str()
    );
}

fn er_unified_marker_id(
    diagram_id: SvgDiagramId<'_>,
    diagram_type: &str,
    upstream_marker: &str,
) -> String {
    let upstream_marker = upstream_marker.trim();
    let (base, suffix) = if let Some(v) = upstream_marker.strip_suffix("_START") {
        (v, "Start")
    } else if let Some(v) = upstream_marker.strip_suffix("_END") {
        (v, "End")
    } else {
        return upstream_marker.to_string();
    };

    let marker_type = match base {
        "ONLY_ONE" => "onlyOne",
        "ZERO_OR_ONE" => "zeroOrOne",
        "ONE_OR_MORE" => "oneOrMore",
        "ZERO_OR_MORE" => "zeroOrMore",
        "MD_PARENT" => "mdParent",
        _ => return upstream_marker.to_string(),
    };

    format!("{diagram_id}_{diagram_type}-{marker_type}{suffix}")
}

#[cfg(test)]
mod tests {
    use crate::model::{Bounds, ErDiagramLayout, LayoutNode};
    use crate::svg::{SvgRenderOptions, with_test_svg_execution};
    use indexmap::IndexMap;
    use merman_core::diagrams::er::{ErDiagramRenderModel, ErEntityRenderModel};
    use serde_json::json;

    #[test]
    fn er_redux_color_theme_emits_per_entity_color_rules() {
        let config = json!({
            "theme": "redux-color",
            "themeVariables": {
                "borderColorArray": ["#e879f9", "#2dd4bf"],
                "bkgColorArray": ["#fdf4ff", "#f0fdfa"],
                "THEME_COLOR_LIMIT": 2
            }
        });
        let theme = super::SvgTheme::new(&config);
        let borders = theme.string_array("borderColorArray");
        let backgrounds = theme.string_array("bkgColorArray");

        assert!(super::is_er_redux_color_theme(&config));
        assert_eq!(
            super::er_redux_color_id(&borders, 0).as_deref(),
            Some("color-0")
        );
        assert_eq!(
            super::er_redux_color_id(&borders, 3).as_deref(),
            Some("color-1")
        );
        assert_eq!(
            super::er_redux_color_id(&[], 0),
            None,
            "an empty color palette must not produce a modulo-by-zero color id"
        );

        let css = super::er_redux_color_css("er", "classic", &borders, &backgrounds);
        assert!(css.contains(
            r##"#er [data-look="classic"][data-color-id="color-0"].node path{stroke:#e879f9;fill:#fdf4ff;}"##
        ));
        assert!(css.contains(
            r##"#er [data-look="classic"][data-color-id="color-1"].node rect{stroke:#2dd4bf;fill:#f0fdfa;}"##
        ));

        let dark_css = super::er_redux_color_css("er", "classic", &borders, &[]);
        assert!(dark_css.contains(
            r##"#er [data-look="classic"][data-color-id="color-0"].node path{stroke:#e879f9;}"##
        ));
        assert!(!dark_css.contains("fill:"), "{dark_css}");

        let mut merged = super::er_css("er", &config).unwrap();
        super::insert_er_redux_color_css(&mut merged, "er", &css);
        let colors = merged.find("[data-color-id=").unwrap();
        let family = merged.find("#er .entityBox").unwrap();
        let root = merged.find("#er :root").unwrap();
        assert!(colors < family && family < root, "{merged}");
    }

    #[test]
    fn er_redux_colors_follow_entity_insertion_ids() {
        let config = json!({
            "theme": "redux-color",
            "themeVariables": {
                "borderColorArray": ["#e879f9", "#2dd4bf"],
                "bkgColorArray": ["#fdf4ff", "#f0fdfa"],
                "THEME_COLOR_LIMIT": 2
            }
        });
        let zeta = ErEntityRenderModel {
            id: "entity-ZETA-0".to_string(),
            label: "ZETA".to_string(),
            shape: "erBox".to_string(),
            css_classes: "default".to_string(),
            ..ErEntityRenderModel::default()
        };
        let alpha = ErEntityRenderModel {
            id: "entity-ALPHA-1".to_string(),
            label: "ALPHA".to_string(),
            shape: "erBox".to_string(),
            css_classes: "default".to_string(),
            ..ErEntityRenderModel::default()
        };
        let model = ErDiagramRenderModel {
            direction: "TB".to_string(),
            entities: IndexMap::from([
                ("ALPHA".to_string(), alpha.clone()),
                ("ZETA".to_string(), zeta.clone()),
            ]),
            ..ErDiagramRenderModel::default()
        };
        let color_indices = super::er_color_indices(&model);
        assert_eq!(color_indices.get(&zeta.id), Some(&0));
        assert_eq!(color_indices.get(&alpha.id), Some(&1));
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let settings = crate::er::ErConfigView::new(&config).render_settings();
        let measure = |entity: &ErEntityRenderModel| {
            crate::er::measure_entity_box(
                entity,
                &measurer,
                &settings.label_style,
                &settings.label_style,
                settings.entity_measurement,
            )
        };
        let zeta_measure = measure(&zeta);
        let alpha_measure = measure(&alpha);
        let layout = ErDiagramLayout {
            // Deliberately reverse the layout vector. Mermaid colorIndex belongs to the entity,
            // not to whichever order the layout backend returns nodes in.
            nodes: vec![
                LayoutNode {
                    id: alpha.id.clone(),
                    x: 220.0,
                    y: 80.0,
                    width: alpha_measure.width,
                    height: alpha_measure.height,
                    is_cluster: false,
                    label_width: None,
                    label_height: None,
                },
                LayoutNode {
                    id: zeta.id.clone(),
                    x: 80.0,
                    y: 80.0,
                    width: zeta_measure.width,
                    height: zeta_measure.height,
                    is_cluster: false,
                    label_width: None,
                    label_height: None,
                },
            ],
            edges: Vec::new(),
            prepared_entities: std::collections::HashMap::from([
                (alpha.id.clone(), alpha_measure),
                (zeta.id.clone(), zeta_measure),
            ]),
            render_edges: Vec::new(),
            clusters: Vec::new(),
            bounds: Some(Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 300.0,
                max_y: 160.0,
            }),
        };
        let options = SvgRenderOptions {
            diagram_id: Some("er-colors".to_string()),
            ..SvgRenderOptions::default()
        };
        let svg = with_test_svg_execution(&options, |options| {
            super::render_er_diagram_svg_model(&layout, &model, &config, None, &measurer, options)
        })
        .and_then(|svg| svg.into_string_for(crate::family::RenderFamilyKind::Er))
        .unwrap();

        let zeta_dom = r#"<g id="er-colors-entity-ZETA-0" class="node default" data-look="classic" data-color-id="color-0""#;
        let alpha_dom = r#"<g id="er-colors-entity-ALPHA-1" class="node default" data-look="classic" data-color-id="color-1""#;
        assert!(svg.contains(zeta_dom), "{svg}");
        assert!(svg.contains(alpha_dom), "{svg}");
        assert!(svg.find(zeta_dom).unwrap() < svg.find(alpha_dom).unwrap());
    }

    #[test]
    fn er_font_family_css_uses_mermaid_default_fallback_for_raw_config() {
        assert_eq!(
            crate::er::ErConfigView::new(&json!({}))
                .text_style()
                .font_family
                .as_deref(),
            Some(crate::config::MERMAID_DEFAULT_FONT_FAMILY_CSS)
        );
    }

    #[test]
    fn er_font_family_css_prefers_theme_variables() {
        assert_eq!(
            crate::er::ErConfigView::new(&json!({
                "fontFamily": "Courier, monospace",
                "themeVariables": {
                    "fontFamily": "\"IBM Plex Sans\", Arial, sans-serif"
                }
            }))
            .text_style()
            .font_family
            .as_deref(),
            Some(r#""IBM Plex Sans",Arial,sans-serif"#)
        );
    }
}
