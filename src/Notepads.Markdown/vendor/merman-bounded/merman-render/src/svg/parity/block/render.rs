use super::super::*;
use crate::block::{
    BlockRectangleKind, BlockShapeBoundary, BlockShapeGeometry, block_label_is_effectively_empty,
};
use crate::model::LayoutPoint;
use crate::svg::parity::roughjs_common::{closed_path_d_from_points, ops_to_svg_path_d};

// Block diagram SVG renderer implementation (split from parity.rs).

pub(crate) fn render_block_diagram_svg_model(
    layout: &BlockDiagramLayout,
    model: &merman_core::diagrams::block::BlockDiagramRenderModel,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    fn decode_block_label_html(raw: &str) -> String {
        // Mermaid's block diagram labels are rendered via an HTML foreignObject label helper,
        // which decodes HTML entities (notably `&nbsp;`).
        raw.replace("&nbsp;", "\u{00A0}")
    }

    fn roughjs_block_paths(
        path_data: &str,
        stroke_width: f32,
        randomness: &roughr::core::RoughRandomness,
    ) -> Option<(String, String)> {
        let mut stroke_options = roughr::core::OptionsBuilder::default()
            .randomness(randomness.clone())
            .roughness(0.0)
            .bowing(1.0)
            .fill_style(roughr::core::FillStyle::Solid)
            .stroke_width(stroke_width)
            .stroke_line_dash(vec![0.0, 0.0])
            .stroke_line_dash_offset(0.0)
            .fill_line_dash(vec![0.0, 0.0])
            .fill_line_dash_offset(0.0)
            .disable_multi_stroke(false)
            .disable_multi_stroke_fill(false)
            .build()
            .ok()?;

        // Mermaid's RoughJS path emitter draws the outline first, then reuses the advanced
        // randomizer for the solid fill pass.  Keep that order even at roughness zero because the
        // seeded path data is part of the stable SVG contract.
        let stroke_opset =
            roughr::renderer::svg_path::<f64>(path_data.to_string(), &mut stroke_options);
        let distance = 0.5;
        let sets = roughr::points_on_path::points_on_path::<f64>(
            path_data.to_string(),
            Some(1.0),
            Some(distance),
        );
        let mut fill_options = stroke_options.clone();
        let fill_opset = if sets.len() == 1 {
            fill_options.disable_multi_stroke = Some(true);
            fill_options.roughness = Some(0.0);
            let mut opset =
                roughr::renderer::svg_path::<f64>(path_data.to_string(), &mut fill_options);
            opset.ops = opset
                .ops
                .iter()
                .cloned()
                .enumerate()
                .filter_map(|(index, op)| {
                    if index != 0 && op.op == roughr::core::OpType::Move {
                        None
                    } else {
                        Some(op)
                    }
                })
                .collect();
            opset
        } else {
            roughr::renderer::solid_fill_polygon(&sets, &mut fill_options)
        };

        Some((
            ops_to_svg_path_d(&fill_opset),
            ops_to_svg_path_d(&stroke_opset),
        ))
    }

    fn block_stadium_points(width: f64, height: f64) -> Vec<LayoutPoint> {
        fn circle_points(
            center_x: f64,
            center_y: f64,
            radius: f64,
            count: usize,
            start_deg: f64,
            end_deg: f64,
        ) -> Vec<LayoutPoint> {
            let start = start_deg.to_radians();
            let step = (end_deg.to_radians() - start) / (count.saturating_sub(1).max(1) as f64);
            (0..count)
                .map(|index| {
                    let angle = start + index as f64 * step;
                    // Mermaid's generateCirclePoints() negates generated coordinates.
                    LayoutPoint {
                        x: -(center_x + radius * angle.cos()),
                        y: -(center_y + radius * angle.sin()),
                    }
                })
                .collect()
        }

        let radius = height / 2.0;
        let mut points = vec![
            LayoutPoint {
                x: -width / 2.0 + radius,
                y: -height / 2.0,
            },
            LayoutPoint {
                x: width / 2.0 - radius,
                y: -height / 2.0,
            },
        ];
        points.extend(circle_points(
            -width / 2.0 + radius,
            0.0,
            radius,
            50,
            90.0,
            270.0,
        ));
        points.push(LayoutPoint {
            x: width / 2.0 - radius,
            y: height / 2.0,
        });
        points.extend(circle_points(
            width / 2.0 - radius,
            0.0,
            radius,
            50,
            270.0,
            450.0,
        ));
        points
    }

    struct RoughPathRenderOptions<'a> {
        style: &'a str,
        fill: &'a str,
        stroke: &'a str,
        stroke_width: f32,
        randomness: &'a roughr::core::RoughRandomness,
        transform: Option<(f64, f64)>,
    }

    fn emit_rough_paths(
        out: &mut String,
        path_data: &str,
        options: RoughPathRenderOptions<'_>,
    ) -> bool {
        if let Some((fill_d, stroke_d)) =
            roughjs_block_paths(path_data, options.stroke_width, options.randomness)
        {
            if let Some((tx, ty)) = options.transform {
                let _ = write!(
                    out,
                    r#"<g class="basic label-container outer-path" transform="translate({},{})">"#,
                    fmt_display(tx),
                    fmt_display(ty)
                );
            } else {
                let _ = write!(out, r#"<g class="basic label-container outer-path">"#);
            }
            let _ = write!(
                out,
                r#"<path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/>"#,
                escape_attr(&fill_d),
                escape_attr(options.fill),
                escape_attr(options.style)
            );
            let _ = write!(
                out,
                r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0" style="{}"/>"#,
                escape_attr(&stroke_d),
                escape_attr(options.stroke),
                fmt_display(options.stroke_width as f64),
                escape_attr(options.style)
            );
            out.push_str("</g>");
            true
        } else {
            false
        }
    }

    #[derive(Clone)]
    struct RenderNode {
        color_index: Option<usize>,
        label: String,
        block_type: String,
        classes: Vec<String>,
        styles: Vec<String>,
    }

    let nodes_by_id = model
        .blocks_flat
        .iter()
        .map(|node| {
            (
                node.id.clone(),
                RenderNode {
                    color_index: node.color_index,
                    label: node.label.clone(),
                    block_type: node.block_type.clone(),
                    classes: node.classes.clone(),
                    styles: node.styles.clone(),
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let shape_geometries_by_id: std::collections::HashMap<_, _> = layout
        .shape_geometries
        .iter()
        .map(|geometry| (geometry.id.as_str(), geometry))
        .collect();

    fn marker_id(diagram_id: SvgDiagramId<'_>, marker: &str) -> String {
        format!("{diagram_id}_block-{marker}")
    }

    fn marker_url(diagram_id: SvgDiagramId<'_>, marker: &str) -> String {
        format!("url(#{})", marker_id(diagram_id, marker))
    }

    fn dom_id(diagram_id: SvgDiagramId<'_>, raw_id: &str) -> String {
        if diagram_id.semantic_str().is_empty() {
            raw_id.to_string()
        } else {
            format!("{diagram_id}-{raw_id}")
        }
    }

    fn edge_marker_end(arrow: Option<&str>) -> Option<&'static str> {
        match arrow.unwrap_or("").trim() {
            "arrow_point" => Some("pointEnd"),
            "arrow_circle" => Some("circleEnd"),
            "arrow_cross" => Some("crossEnd"),
            "arrow_open" | "" => None,
            _ => Some("pointEnd"),
        }
    }

    fn edge_marker_start(arrow: Option<&str>) -> Option<&'static str> {
        match arrow.unwrap_or("").trim() {
            "arrow_point" => Some("pointStart"),
            "arrow_circle" => Some("circleStart"),
            "arrow_cross" => Some("crossStart"),
            "arrow_open" | "" => None,
            _ => None,
        }
    }

    fn push_ordered_decl(out: &mut Vec<(String, String)>, key: &str, raw: &str) {
        if let Some((_, value)) = out.iter_mut().find(|(existing, _)| existing == key) {
            *value = raw.to_string();
            return;
        }
        out.push((key.to_string(), raw.to_string()));
    }

    fn compile_block_inline_styles(styles: &[String]) -> (String, String, String) {
        let mut box_decls: Vec<(String, String)> = Vec::new();
        let mut text_decls: Vec<(String, String)> = Vec::new();

        for raw in styles {
            let trimmed = raw.trim().trim_end_matches(';').trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some((key, value)) = parse_style_decl(trimmed) else {
                continue;
            };
            if is_rect_style_key(key) {
                push_ordered_decl(&mut box_decls, key, trimmed);
            }
            if is_text_style_key(key) {
                let _ = value;
                push_ordered_decl(&mut text_decls, key, trimmed);
            }
        }

        let style_attr = |decls: &[(String, String)]| -> String {
            let mut out = String::new();
            for (_, raw) in decls {
                out.push_str(raw);
                out.push(';');
            }
            out
        };

        let mut div_prefix = String::new();
        for (key, raw) in &text_decls {
            if key == "color" {
                let value = raw.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
                if !value.is_empty() {
                    let _ = write!(
                        &mut div_prefix,
                        "color: {}; ",
                        super::super::util::cssom_color_value(value)
                    );
                }
            } else {
                div_prefix.push_str(raw);
                div_prefix.push_str("; ");
            }
        }

        (style_attr(&box_decls), style_attr(&text_decls), div_prefix)
    }

    fn block_edge_start_marker_inset(arrow: Option<&str>) -> f64 {
        match arrow.unwrap_or("").trim() {
            "arrow_point" => 4.5,
            _ => 0.0,
        }
    }

    fn block_edge_end_marker_inset(arrow: Option<&str>) -> f64 {
        match arrow.unwrap_or("").trim() {
            "arrow_point" => 4.0,
            _ => 0.0,
        }
    }

    fn move_point_towards(point: &LayoutPoint, target: &LayoutPoint, distance: f64) -> LayoutPoint {
        if distance.abs() <= 1e-12 {
            return point.clone();
        }
        let dx = target.x - point.x;
        let dy = target.y - point.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 1e-12 {
            return point.clone();
        }
        LayoutPoint {
            x: point.x + dx / len * distance,
            y: point.y + dy / len * distance,
        }
    }

    fn block_class_css(
        diagram_id: SvgDiagramId<'_>,
        class_defs: &indexmap::IndexMap<
            String,
            merman_core::diagrams::block::BlockClassDefRenderModel,
        >,
        options: &SvgExecution<'_>,
    ) -> Result<String> {
        fn important_declarations(styles: &[String]) -> String {
            let mut out = String::new();
            for style in styles {
                let Some((key, value)) = parse_style_decl(style) else {
                    continue;
                };
                let _ = write!(&mut out, "{key}:{}!important;", escape_xml(value));
            }
            out
        }

        let mut out = String::new();
        for class_def in class_defs.values() {
            options.checkpoint_emit()?;
            let class = escape_xml(&class_def.id);
            let shape_style = important_declarations(&class_def.styles);
            if !shape_style.is_empty() {
                let _ = write!(
                    &mut out,
                    r#"#{} .{}&gt;*{{{}}}#{} .{} span{{{}}}"#,
                    diagram_id,
                    class.as_str(),
                    shape_style,
                    diagram_id,
                    class.as_str(),
                    shape_style
                );
            }

            let text_style = important_declarations(&class_def.text_styles);
            if !text_style.is_empty() {
                let _ = write!(
                    &mut out,
                    r#"#{} .{} tspan{{{}}}"#,
                    diagram_id,
                    class.as_str(),
                    text_style
                );
            }
            options.checkpoint_emit()?;
        }
        Ok(out)
    }

    fn block_css(
        diagram_id: SvgDiagramId<'_>,
        effective_config: &serde_json::Value,
        class_defs: &indexmap::IndexMap<
            String,
            merman_core::diagrams::block::BlockClassDefRenderModel,
        >,
        options: &SvgExecution<'_>,
    ) -> Result<String> {
        let theme = PresentationTheme::new(effective_config).node_diagram();
        let font_family = theme.common.font_family_css.as_str();
        let font_size = theme.common.font_size_px;
        let text_color = theme.common.text_color.as_str();
        let node_text_color = theme.node_text_color.as_str();
        let title_color = theme.title_color.as_str();
        let main_bkg = theme.main_bkg.as_str();
        let node_border = theme.node_border.as_str();
        let line_color = theme.common.line_color.as_str();
        let arrowhead_color = theme.arrowhead_color.as_str();
        let stroke_width = theme.stroke_width.as_str();
        let edge_label_background = theme.edge_label_background.as_str();
        let cluster_bkg = theme.cluster_bkg.as_str();
        let cluster_border = theme.cluster_border.as_str();
        let cluster_bkg = css_rgba_fade(cluster_bkg, 0.5)?;
        let cluster_border = css_rgba_fade(cluster_border, 0.2)?;

        let mut out = String::new();
        let _ = write!(
            &mut out,
            r#"#{}{{font-family:{};font-size:{}px;fill:{};}}"#,
            diagram_id,
            font_family,
            fmt(font_size),
            node_text_color
        );
        let _ = write!(
            &mut out,
            r#"#{} .edge-thickness-normal{{stroke-width:{}px;}}#{} .edge-thickness-thick{{stroke-width:3.5px;}}#{} .edge-pattern-solid{{stroke-dasharray:0;}}#{} .edge-thickness-invisible{{stroke-width:0;fill:none;}}#{} .edge-pattern-dashed{{stroke-dasharray:3;}}#{} .edge-pattern-dotted{{stroke-dasharray:2;}}"#,
            diagram_id, stroke_width, diagram_id, diagram_id, diagram_id, diagram_id, diagram_id
        );
        super::palette::write_palette_css(&mut out, diagram_id, effective_config, options)?;
        let _ = write!(
            &mut out,
            r#"#{} .label{{font-family:{};color:{};}}#{} p{{margin:0;}}#{} .label text,#{} span,#{} p{{fill:{};color:{};}}"#,
            diagram_id,
            font_family,
            node_text_color,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            node_text_color,
            node_text_color
        );
        let _ = write!(
            &mut out,
            r#"#{} .cluster-label text{{fill:{};}}#{} .cluster-label span,#{} .cluster-label p{{color:{};}}"#,
            diagram_id, title_color, diagram_id, diagram_id, title_color
        );
        let _ = write!(
            &mut out,
            r#"#{} .node rect,#{} .node circle,#{} .node ellipse,#{} .node polygon,#{} .node path{{fill:{};stroke:{};stroke-width:1px;}}#{} .flowchart-label text{{text-anchor:middle;}}#{} .node .label{{text-align:center;}}#{} .node.clickable{{cursor:pointer;}}"#,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            diagram_id,
            main_bkg,
            node_border,
            diagram_id,
            diagram_id,
            diagram_id
        );
        let _ = write!(
            &mut out,
            r#"#{} .arrowheadPath,#{} .arrowMarkerPath{{fill:{};stroke:{};}}#{} .edgePaths .path{{stroke:{};stroke-width:2.0px;}}#{} .flowchart-link{{stroke:{};fill:none;}}"#,
            diagram_id,
            diagram_id,
            arrowhead_color,
            line_color,
            diagram_id,
            line_color,
            diagram_id,
            line_color
        );
        let _ = write!(
            &mut out,
            r#"#{} .edgeLabel{{background-color:{};text-align:center;}}#{} .edgeLabel p{{margin:0;padding:0;display:inline;}}#{} .edgeLabel rect{{opacity:0.5;background-color:{};fill:{};}}#{} .labelBkg{{background-color:{}}}"#,
            diagram_id,
            edge_label_background,
            diagram_id,
            diagram_id,
            edge_label_background,
            edge_label_background,
            diagram_id,
            edge_label_background
        );
        let _ = write!(
            &mut out,
            r#"#{} .node .cluster{{fill:{};stroke:{};stroke-width:1px;}}#{} .cluster text{{fill:{};}}#{} .cluster span,#{} .cluster p{{color:{};}}#{} .flowchartTitleText{{text-anchor:middle;font-size:18px;fill:{};}}"#,
            diagram_id,
            cluster_bkg,
            cluster_border,
            diagram_id,
            title_color,
            diagram_id,
            diagram_id,
            title_color,
            diagram_id,
            text_color,
        );
        let _ = crate::svg::parity::css::write_mermaid_base_css_root_rule_to(
            &mut out,
            diagram_id,
            &crate::config::config_root_font_family_css(effective_config),
        );
        out.push_str(&block_class_css(diagram_id, class_defs, options)?);
        Ok(out)
    }

    let diagram_id = options.diagram_id_or("merman");
    let palette_size = super::palette::palette_size(effective_config);
    let look = crate::config::config_diagram_look(effective_config);
    let hand_drawn_seed = options.rough_randomness(
        effective_config
            .get("handDrawnSeed")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(options.seed() as f64),
        "render.block.roughjs",
    );
    let node_theme = PresentationTheme::new(effective_config).node_diagram();
    let node_fill_color = node_theme.main_bkg.as_str();
    let node_stroke_color = node_theme.node_border.as_str();
    // RoughJS uses 1.3px as its default node stroke width in Mermaid's handDrawnShapeStyles.
    // Keep this independent from the CSS `stroke-width: 1px` rule used by ordinary shapes.
    let node_stroke_width = 1.3_f32;

    let layout_bounds = layout.bounds.clone().unwrap_or(Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 100.0,
    });
    let diagram_padding = config_f64(effective_config, &["block", "diagramPadding"])
        .unwrap_or(5.0)
        .max(0.0);

    // Mermaid derives Block's root viewport from the rendered node group (`getBBox()`), not the
    // grid slots used during layout.  Use the shared shape geometry so circles, arrows, and
    // slanted shapes that extend beyond their slots remain inside the root viewBox.
    let rendered_bounds = layout
        .shape_geometries
        .iter()
        .map(BlockShapeGeometry::rendered_extents)
        .fold(None, |bounds: Option<(f64, f64, f64, f64)>, extents| {
            Some(match bounds {
                None => extents,
                Some((min_x, min_y, max_x, max_y)) => (
                    min_x.min(extents.0),
                    min_y.min(extents.1),
                    max_x.max(extents.2),
                    max_y.max(extents.3),
                ),
            })
        })
        .unwrap_or((
            layout_bounds.min_x,
            layout_bounds.min_y,
            layout_bounds.max_x,
            layout_bounds.max_y,
        ));

    let mut out = String::new();
    let root_bounds = root_svg::DiagramBounds::from_extents(
        rendered_bounds.0,
        rendered_bounds.1,
        rendered_bounds.2,
        rendered_bounds.3,
        diagram_padding,
    );
    let root_spec = root_svg::RootViewportSpec::responsive(root_bounds)
        .with_max_width(root_svg::RootMaxWidth::CssSixSignificant(root_bounds.width));
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "block");
    root_chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Block, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    options.checkpoint_emit()?;
    out.push_str("<style>");
    out.push_str(&block_css(
        diagram_id,
        effective_config,
        &model.class_defs,
        options,
    )?);
    out.push_str("</style><g/>");
    options.checkpoint_emit()?;

    // Block owns its render loop and invokes insertLookDefs before inserting markers.
    super::super::look_defs::push_look_shadow_defs(&mut out, diagram_id, effective_config);
    super::super::look_defs::push_look_gradient(&mut out, diagram_id, effective_config);
    options.checkpoint_emit()?;

    super::super::markers::push_base_edge_markers(&mut out, diagram_id, "block");
    options.checkpoint_emit()?;

    out.push_str(r#"<g class="block">"#);

    for n in &layout.nodes {
        let Some(node) = nodes_by_id.get(&n.id) else {
            continue;
        };

        let class_str = if node.classes.is_empty() {
            "default flowchart-label".to_string()
        } else {
            format!("{} flowchart-label", node.classes.join(" "))
        };
        let (node_box_style, node_text_style, node_div_style_prefix) =
            compile_block_inline_styles(&node.styles);

        let geometry =
            shape_geometries_by_id
                .get(n.id.as_str())
                .ok_or_else(|| Error::InvalidModel {
                    message: format!("missing Block shape geometry for node `{}`", n.id),
                })?;
        let id_attr = format!(r#" id="{}""#, escape_attr(&dom_id(diagram_id, &n.id)));
        options.checkpoint_emit()?;
        let _ = write!(
            &mut out,
            r#"<g class="node {}"{} transform="translate({}, {})" data-look="{}""#,
            escape_attr(&class_str),
            id_attr,
            fmt(geometry.allocated.x),
            fmt(geometry.allocated.y),
            escape_attr(look.as_str())
        );
        if palette_size > 0
            && let Some(index) = node.color_index
        {
            let _ = write!(out, r#" data-color-id="color-{}""#, index % palette_size);
        }
        out.push('>');

        match &geometry.boundary {
            BlockShapeBoundary::Rectangle {
                width,
                height,
                radius,
                kind,
            } => {
                let class = match kind {
                    BlockRectangleKind::Basic => "basic label-container",
                    BlockRectangleKind::Composite => "basic cluster composite label-container",
                };
                let _ = write!(
                    &mut out,
                    r#"<rect class="{}" rx="{}" ry="{}" style="{}" x="{}" y="{}" width="{}" height="{}"/>"#,
                    class,
                    fmt(*radius),
                    fmt(*radius),
                    escape_attr(&node_box_style),
                    fmt(-width / 2.0),
                    fmt(-height / 2.0),
                    fmt(*width),
                    fmt(*height)
                );
            }
            BlockShapeBoundary::Circle { radius, .. } => {
                let _ = write!(
                    &mut out,
                    r#"<circle class="basic label-container" style="{}" r="{}" cx="0" cy="0"/>"#,
                    escape_attr(&node_box_style),
                    fmt(*radius),
                );
            }
            BlockShapeBoundary::DoubleCircle {
                outer_radius,
                inner_radius,
                ..
            } => {
                let _ = write!(
                    &mut out,
                    r#"<g class="basic label-container" style="{}"><circle class="outer-circle" style="{}" r="{}" cx="0" cy="0"/><circle class="inner-circle" style="{}" r="{}" cx="0" cy="0"/></g>"#,
                    escape_attr(&node_box_style),
                    escape_attr(&node_box_style),
                    fmt(*outer_radius),
                    escape_attr(&node_box_style),
                    fmt(*inner_radius),
                );
            }
            BlockShapeBoundary::Stadium { width, height } => {
                let points = block_stadium_points(*width, *height);
                let path_data = closed_path_d_from_points(
                    &points
                        .iter()
                        .map(|point| (point.x, point.y))
                        .collect::<Vec<_>>(),
                );
                if !emit_rough_paths(
                    &mut out,
                    &path_data,
                    RoughPathRenderOptions {
                        style: &node_box_style,
                        fill: node_fill_color,
                        stroke: node_stroke_color,
                        stroke_width: node_stroke_width,
                        randomness: &hand_drawn_seed,
                        transform: None,
                    },
                ) {
                    let radius = height / 2.0;
                    let _ = write!(
                        &mut out,
                        r#"<rect class="basic label-container" style="{}" x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}"/>"#,
                        escape_attr(&node_box_style),
                        fmt(-width / 2.0),
                        fmt(-height / 2.0),
                        fmt(*width),
                        fmt(*height),
                        fmt(radius),
                        fmt(radius)
                    );
                }
            }
            BlockShapeBoundary::Cylinder {
                width,
                body_height,
                radius_x,
                radius_y,
            } => {
                let _ = write!(
                    &mut out,
                    r#"<path d="M{},{} a{},{} 0,0,0 {} 0 a{},{} 0,0,0 {} 0 l0,{} a{},{} 0,0,0 {} 0 l0,{}" class="basic label-container outer-path" style="{}" transform="translate({}, {})"/>"#,
                    fmt_display(0.0),
                    fmt_display(*radius_y),
                    fmt_display(*radius_x),
                    fmt_display(*radius_y),
                    fmt_display(*width),
                    fmt_display(*radius_x),
                    fmt_display(*radius_y),
                    fmt_display(-width),
                    fmt_display(*body_height),
                    fmt_display(*radius_x),
                    fmt_display(*radius_y),
                    fmt_display(*width),
                    fmt_display(-body_height),
                    escape_attr(&node_box_style),
                    fmt_display(-width / 2.0),
                    fmt_display(-(body_height / 2.0 + radius_y))
                );
            }
            BlockShapeBoundary::Polygon {
                points,
                translation,
            } => {
                let odd_shape = matches!(node.block_type.as_str(), "odd" | "rect_left_inv_arrow");
                let points_as_tuples: Vec<(f64, f64)> =
                    points.iter().map(|point| (point.x, point.y)).collect();
                let path_data = closed_path_d_from_points(&points_as_tuples);
                if odd_shape
                    && emit_rough_paths(
                        &mut out,
                        &path_data,
                        RoughPathRenderOptions {
                            style: &node_box_style,
                            fill: node_fill_color,
                            stroke: node_stroke_color,
                            stroke_width: node_stroke_width,
                            randomness: &hand_drawn_seed,
                            transform: Some((translation.x, translation.y)),
                        },
                    )
                {
                    // The odd shape is always emitted through RoughJS in Mermaid 11.17.2,
                    // including the default look (roughness is simply zero there).
                } else {
                    out.push_str(r#"<polygon points=""#);
                    for (index, point) in points.iter().enumerate() {
                        if index > 0 {
                            out.push(' ');
                        }
                        let _ = write!(
                            &mut out,
                            "{},{}",
                            fmt_display(point.x),
                            fmt_display(point.y)
                        );
                    }
                    let _ = write!(
                        &mut out,
                        r#"" class="label-container" style="{}" transform="translate({},{})"/>"#,
                        escape_attr(&node_box_style),
                        fmt_display(translation.x),
                        fmt_display(translation.y)
                    );
                }
            }
        }

        let label = decode_block_label_html(&node.label);
        let label_effectively_empty =
            node.label.is_empty() || block_label_is_effectively_empty(&label);
        let (label_tx, label_ty, label_w, label_h) = if label_effectively_empty {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            let label_w = n.label_width.unwrap_or(0.0).max(0.0);
            let label_h = n.label_height.unwrap_or(0.0).max(0.0);
            let label_dx = if matches!(node.block_type.as_str(), "odd" | "rect_left_inv_arrow") {
                match &geometry.boundary {
                    BlockShapeBoundary::Polygon { translation, .. } => translation.x,
                    _ => 0.0,
                }
            } else {
                0.0
            };
            (label_dx - label_w / 2.0, -label_h / 2.0, label_w, label_h)
        };
        let span_style_attr = if node_text_style.is_empty() {
            String::new()
        } else {
            format!(r#" style="{}""#, escape_attr(&node_text_style))
        };
        let label_markup = if node.label.is_empty() {
            String::new()
        } else {
            format!("<p>{}</p>", escape_xml(&label))
        };
        let _ = write!(
            &mut out,
            r#"<g class="label" style="{}" transform="translate({}, {})"><rect/><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}display: table-cell; white-space: nowrap; line-height: 1.5;"><span class="nodeLabel"{}>{}</span></div></foreignObject></g>"#,
            escape_attr(&node_text_style),
            fmt(label_tx),
            fmt(label_ty),
            fmt(label_w),
            fmt(label_h),
            escape_attr(&node_div_style_prefix),
            span_style_attr,
            label_markup
        );

        out.push_str("</g>");
    }

    for e in &model.edges {
        let Some(le) = layout.edges.iter().find(|x| x.id == e.id) else {
            continue;
        };
        let mut edge_points = match (
            shape_geometries_by_id.get(e.start.as_str()),
            shape_geometries_by_id.get(e.end.as_str()),
        ) {
            (Some(from), Some(to)) => {
                let mid = le.points.get(1).cloned().unwrap_or(LayoutPoint {
                    x: from.allocated.x + (to.allocated.x - from.allocated.x) / 2.0,
                    y: from.allocated.y + (to.allocated.y - from.allocated.y) / 2.0,
                });
                vec![from.intersect(&mid), mid.clone(), to.intersect(&mid)]
            }
            _ => le.points.clone(),
        };
        let data_points =
            base64::engine::general_purpose::STANDARD.encode(json_stringify_points(&edge_points));
        if edge_points.len() >= 2 {
            let start_inset = block_edge_start_marker_inset(e.arrow_type_start.as_deref());
            if start_inset > 0.0 {
                edge_points[0] = move_point_towards(&edge_points[0], &edge_points[1], start_inset);
            }
            let end_inset = block_edge_end_marker_inset(e.arrow_type_end.as_deref());
            if end_inset > 0.0 {
                let last = edge_points.len() - 1;
                edge_points[last] =
                    move_point_towards(&edge_points[last], &edge_points[last - 1], end_inset);
            }
        }
        let d = curve_basis_path_d(&edge_points);
        let class_attr = "edge-thickness-normal edge-pattern-solid edge-thickness-normal edge-pattern-solid flowchart-link LS-a1 LE-b1";
        let prefixed_edge_id = dom_id(diagram_id, &e.id);
        let path_id = dom_id(diagram_id, &prefixed_edge_id);
        let _ = write!(
            &mut out,
            r#"<path d="{}" id="{}" class="{}" style="undefined;;;undefined" data-edge="true" data-et="edge" data-id="{}" data-points="{}""#,
            escape_attr(&d),
            escape_attr(&path_id),
            escape_attr(class_attr),
            escape_attr(&prefixed_edge_id),
            escape_attr(&data_points)
        );

        if let Some(m) = edge_marker_start(e.arrow_type_start.as_deref()) {
            let _ = write!(
                &mut out,
                r#" marker-start="{}""#,
                escape_attr(&marker_url(diagram_id, m))
            );
        }
        if let Some(m) = edge_marker_end(e.arrow_type_end.as_deref()) {
            let _ = write!(
                &mut out,
                r#" marker-end="{}""#,
                escape_attr(&marker_url(diagram_id, m))
            );
        }
        options.checkpoint_emit()?;
        out.push_str("/>");
    }

    for e in &model.edges {
        let Some(le) = layout.edges.iter().find(|x| x.id == e.id) else {
            continue;
        };
        let Some(lbl) = le.label.as_ref().filter(|_| !e.label.trim().is_empty()) else {
            continue;
        };

        let _ = write!(
            &mut out,
            r#"<g class="edgeLabel" transform="translate({}, {})"><g class="label" data-id="{}" transform="translate({}, {})"><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;"><span class="edgeLabel"><p>{}</p></span></div></foreignObject></g></g>"#,
            fmt(lbl.x),
            fmt(lbl.y),
            escape_attr(&e.id),
            fmt(-lbl.width / 2.0),
            fmt(-lbl.height / 2.0),
            fmt(lbl.width),
            fmt(lbl.height),
            escape_xml(&decode_block_label_html(&e.label))
        );
    }

    out.push_str("</g></svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::RenderEnvironment;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};

    #[test]
    fn diagram_id_terminal_precedes_later_block_model_validation() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, 1)
            .expect("valid SVG byte limit");
        let session = RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session()
            .expect("begin render session");
        let request = SvgRenderOptions {
            diagram_id: Some("terminal".to_string()),
            ..SvgRenderOptions::default()
        };
        let debug = SvgDebugOptions::default();
        let options = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let layout = BlockDiagramLayout {
            nodes: vec![LayoutNode {
                id: "node".to_string(),
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                is_cluster: false,
                label_width: None,
                label_height: None,
            }],
            edges: Vec::new(),
            shape_geometries: Vec::new(),
            bounds: None,
        };
        let parsed = merman_core::Engine::new()
            .parse_diagram_for_render_model_sync(
                "block\nnode\n",
                merman_core::ParseOptions::strict(),
            )
            .expect("parse Block render model")
            .expect("Block input is detected");
        let merman_core::RenderSemanticModel::Block(model) = parsed.model() else {
            panic!("expected Block render model");
        };

        let error =
            render_block_diagram_svg_model(&layout, model, &serde_json::json!({}), &options)
                .expect_err("diagram-id projection must stop before missing geometry validation");
        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection");
        };
        assert_eq!(details.limit, ResourceLimitId::MaxSvgBytes.as_str());
    }
}
