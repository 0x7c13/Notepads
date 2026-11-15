use super::super::roughjs_common::ops_to_svg_path_d;
use super::super::theme::VennTheme;
use super::super::*;
use merman_core::diagrams::venn::VennDiagramRenderModel;
use merman_core::theme_color::transparentize;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr as _;

fn stable_sets_key(sets: &[String]) -> String {
    sets.join("|")
}

fn escape_css_attr(value: &str) -> String {
    escape_attr(value)
}

fn data_sets_attr(sets: &[String]) -> String {
    sets.join("_")
}

fn build_style_by_key(model: &VennDiagramRenderModel) -> HashMap<String, BTreeMap<String, String>> {
    let mut out = HashMap::new();
    for entry in &model.style_entries {
        let key = stable_sets_key(&entry.targets);
        out.entry(key)
            .or_insert_with(BTreeMap::new)
            .extend(entry.styles.clone());
    }
    out
}

fn style_value<'a>(styles: Option<&'a BTreeMap<String, String>>, key: &str) -> Option<&'a str> {
    styles
        .and_then(|styles| styles.get(key))
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn render_label(area: &crate::model::VennAreaLayout) -> &str {
    if let Some(label) = area.label.as_deref().filter(|label| !label.is_empty()) {
        label
    } else if area.sets.len() == 1 {
        area.sets[0].as_str()
    } else {
        ""
    }
}

fn invalid_rough_options(context: &str, error: impl std::fmt::Display) -> Error {
    Error::InvalidModel {
        message: format!("invalid Venn {context} RoughJS options: {error}"),
    }
}

fn rough_color(value: &str) -> Result<roughr::Srgba> {
    let color = roughr::Color::from_str(value.trim()).map_err(|error| Error::InvalidModel {
        message: format!("invalid Venn RoughJS color `{value}`: {error}"),
    })?;
    Ok(roughr::Srgba::new(
        color.red as f32 / 255.0,
        color.green as f32 / 255.0,
        color.blue as f32 / 255.0,
        color.alpha as f32 / 255.0,
    ))
}

fn parse_stroke_width(value: &str) -> Option<f32> {
    let value = value.trim_start();
    value
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(value.len()))
        .filter(|end| *end > 0)
        .rev()
        .find_map(|end| value[..end].parse::<f32>().ok())
        .filter(|value| value.is_finite())
}

fn rough_circle_paths(
    circle: &crate::model::VennCircleLayout,
    base_color: &str,
    stroke_color: &str,
    stroke_width: f32,
    hachure_angle: f32,
    randomness: &roughr::core::RoughRandomness,
) -> Result<(String, String)> {
    let options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.7)
        .bowing(1.0)
        .fill(rough_color(base_color)?)
        .fill_style(roughr::core::FillStyle::Hachure)
        .fill_weight(2.0)
        .hachure_gap(8.0)
        .hachure_angle(hachure_angle)
        .stroke(rough_color(stroke_color)?)
        .stroke_width(stroke_width)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .map_err(|error| invalid_rough_options("circle", error))?;
    let drawable = roughr::generator::Generator::default().circle::<f64>(
        circle.x,
        circle.y,
        circle.radius * 2.0,
        &Some(options),
    );
    let mut fill_path = None;
    let mut stroke_path = None;
    for set in drawable.sets {
        match set.op_set_type {
            roughr::core::OpSetType::FillPath | roughr::core::OpSetType::FillSketch => {
                fill_path = Some(ops_to_svg_path_d(&set));
            }
            roughr::core::OpSetType::Path => {
                stroke_path = Some(ops_to_svg_path_d(&set));
            }
        }
    }
    match (fill_path, stroke_path) {
        (Some(fill_path), Some(stroke_path)) => Ok((fill_path, stroke_path)),
        _ => Err(Error::InvalidModel {
            message: "Venn RoughJS circle did not produce fill and stroke paths".to_string(),
        }),
    }
}

fn rough_intersection_fill_path(
    path: &str,
    fill: &str,
    randomness: &roughr::core::RoughRandomness,
) -> Result<String> {
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.7)
        .bowing(1.0)
        .fill(rough_color(fill)?)
        .fill_style(roughr::core::FillStyle::CrossHatch)
        .fill_weight(2.0)
        .hachure_gap(6.0)
        .hachure_angle(60.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .map_err(|error| invalid_rough_options("intersection", error))?;
    options.stroke = None;

    let distance = (1.0 + options.roughness.unwrap_or(1.0) as f64) / 2.0;
    let polygons =
        roughr::points_on_path::points_on_path::<f64>(path.to_string(), Some(1.0), Some(distance));
    // RoughJS computes the outline even when `stroke: none`, so its PRNG state advances before
    // the cross-hatch fill is generated. The discarded operation is part of seeded parity.
    let _discarded_outline = roughr::renderer::svg_path::<f64>(path.to_string(), &mut options);
    let fill_path = roughr::renderer::pattern_fill_polygons(polygons, &mut options);
    if fill_path.op_set_type != roughr::core::OpSetType::FillSketch {
        return Err(Error::InvalidModel {
            message: "Venn RoughJS intersection did not produce a sketch fill path".to_string(),
        });
    }
    Ok(ops_to_svg_path_d(&fill_path))
}

fn write_area_label(
    out: &mut String,
    area: &crate::model::VennAreaLayout,
    font_size: f64,
    text_color: &str,
) {
    let _ = write!(
        out,
        r#"<text class="label" text-anchor="middle" dy=".35em" x="{x}" y="{y}" style="font-size: {font_size}px; fill: {text_fill};"><tspan x="{x}" y="{y}" dy="0.35em">{label}</tspan></text>"#,
        x = fmt(area.text_x),
        y = fmt(area.text_y),
        font_size = fmt(font_size),
        text_fill = escape_css_attr(text_color),
        label = escape_xml(render_label(area)),
    );
}

fn root_open(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    layout: &VennDiagramLayout,
    aria_labelledby: Option<&str>,
    aria_describedby: Option<&str>,
) -> Result<root_svg::RootDocument> {
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "venn");
    root_chrome.aria_labelledby = aria_labelledby;
    root_chrome.aria_describedby = aria_describedby;
    root_chrome.dom = root_svg::RootDomProfile {
        fixed_height_placement: root_svg::SvgRootFixedHeightPlacement::AfterXmlns,
        fixed_style_placement: root_svg::RootStylePlacement::Tail,
        trailing_newline: false,
        ..root_svg::RootDomProfile::default()
    };
    root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Venn, diagram_id)
        .write_open(
            out,
            root_svg::RootViewportSpec::mermaid(
                root_svg::DiagramBounds::from_view_box(0.0, 0.0, layout.width, layout.height),
                layout.use_max_width,
            ),
            root_chrome,
        )
}

fn venn_css<I>(diagram_id: I, theme: &VennTheme) -> String
where
    I: Copy + std::fmt::Display,
{
    format!(
        "#{diagram_id} .venn-title{{font-size:32px;fill:{title_color};font-family:{font_family};}}\
#{diagram_id} .venn-circle text{{font-size:48px;font-family:{font_family};}}\
#{diagram_id} .venn-intersection text{{font-size:48px;fill:{set_text_color};font-family:{font_family};}}\
#{diagram_id} .venn-text-node{{font-family:{font_family};color:{set_text_color};}}",
        title_color = theme.title_color,
        font_family = theme.font_family_css,
        set_text_color = theme.set_text_color,
    )
}

pub(crate) fn render_venn_diagram_svg_model(
    layout: &VennDiagramLayout,
    model: &VennDiagramRenderModel,
    effective_config: &serde_json::Value,
    diagram_title: Option<&str>,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("venn");
    let title = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| {
            diagram_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
        });
    let has_acc_title = model
        .acc_title
        .as_deref()
        .is_some_and(|title| !title.trim().is_empty());
    let has_acc_descr = model
        .acc_descr
        .as_deref()
        .is_some_and(|descr| !descr.trim().is_empty());
    let aria_labelledby = has_acc_title.then(|| format!("chart-title-{diagram_id}"));
    let aria_describedby = has_acc_descr.then(|| format!("chart-desc-{diagram_id}"));

    let mut out = String::new();
    let root_document = root_open(
        &mut out,
        diagram_id,
        layout,
        aria_labelledby.as_deref(),
        aria_describedby.as_deref(),
    )?;
    options.checkpoint_emit()?;

    if has_acc_title {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{id}">{text}</title>"#,
            id = diagram_id,
            text = escape_xml(model.acc_title.as_deref().unwrap_or_default())
        );
    }
    if has_acc_descr {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{id}">{text}</desc>"#,
            id = diagram_id,
            text = escape_xml(model.acc_descr.as_deref().unwrap_or_default())
        );
    }

    let theme = PresentationTheme::new(effective_config).venn()?;
    let css = venn_css(diagram_id, &theme);
    let _ = write!(&mut out, r#"<style>{css}</style>"#);
    out.push_str("<g/>");
    options.checkpoint_emit()?;

    if let Some(title) = title {
        let _ = write!(
            &mut out,
            r#"<text class="venn-title" font-size="{font_size}px" text-anchor="middle" dominant-baseline="middle" x="50%" y="{y}" style="fill: {fill};">{text}</text>"#,
            font_size = fmt(32.0 * layout.scale),
            y = fmt(32.0 * layout.scale),
            fill = escape_xml(&theme.title_color),
            text = escape_xml(title)
        );
    }

    let _ = write!(
        &mut out,
        r#"<g transform="translate(0, {title_height})">"#,
        title_height = fmt(layout.title_height)
    );

    let style_by_key = build_style_by_key(model);
    let is_hand_drawn = config_diagram_look(effective_config).as_str() == "handDrawn";
    let hand_drawn_seed = options.rough_randomness(
        effective_config
            .get("handDrawnSeed")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(options.seed() as f64),
        "render.venn.roughjs",
    );
    let mut circle_index = 0usize;

    for area in &layout.areas {
        let sets_key = stable_sets_key(&area.sets);
        let styles = style_by_key.get(&sets_key);
        if area.sets.len() == 1 {
            let base_color = style_value(styles, "fill")
                .map(str::to_string)
                .unwrap_or_else(|| {
                    theme
                        .circle_colors
                        .get(circle_index % theme.circle_colors.len().max(1))
                        .cloned()
                        .unwrap_or_else(|| theme.primary_color.clone())
                });
            let fill_opacity = style_value(styles, "fill-opacity").unwrap_or("0.1");
            let stroke_color = style_value(styles, "stroke").unwrap_or(base_color.as_str());
            let stroke_width = style_value(styles, "stroke-width")
                .map(str::to_string)
                .unwrap_or_else(|| fmt_string(5.0 * layout.scale));
            let text_color = match style_value(styles, "color") {
                Some(color) => color.to_string(),
                None => theme.circle_text_color(&base_color)?,
            };
            let _ = write!(
                &mut out,
                r#"<g class="venn-area venn-circle venn-set-{set_class}" data-venn-sets="{sets}">"#,
                set_class = circle_index % 8,
                sets = escape_attr(&data_sets_attr(&area.sets)),
            );
            if is_hand_drawn {
                let circle = area.circles.first().ok_or_else(|| Error::InvalidModel {
                    message: format!(
                        "Venn set `{sets_key}` has no circle geometry for hand-drawn rendering"
                    ),
                })?;
                let stroke_width_value =
                    parse_stroke_width(&stroke_width).ok_or_else(|| Error::InvalidModel {
                        message: format!(
                            "Venn set `{sets_key}` has invalid stroke width `{stroke_width}`"
                        ),
                    })?;
                let (fill_path, stroke_path) = rough_circle_paths(
                    circle,
                    &base_color,
                    stroke_color,
                    stroke_width_value,
                    -41.0 + circle_index as f32 * 60.0,
                    &hand_drawn_seed,
                )?;
                let fill_stroke = transparentize(&base_color, 0.7)?;
                let _ = write!(
                    &mut out,
                    r#"<g><path d="{fill_path}" stroke="{fill_stroke}" stroke-width="2" fill="none"/><path d="{stroke_path}" stroke="{stroke}" stroke-width="{stroke_width}" fill="none"/></g>"#,
                    fill_path = escape_attr(&fill_path),
                    fill_stroke = escape_attr(&fill_stroke),
                    stroke_path = escape_attr(&stroke_path),
                    stroke = escape_attr(stroke_color),
                    stroke_width = fmt(stroke_width_value as f64),
                );
            } else {
                let _ = write!(
                    &mut out,
                    r#"<path d="{path}" style="fill: {fill}; fill-opacity: {fill_opacity}; stroke: {stroke}; stroke-width: {stroke_width}; stroke-opacity: 0.95;"/>"#,
                    path = escape_attr(&area.path),
                    fill = escape_css_attr(&base_color),
                    fill_opacity = escape_css_attr(fill_opacity),
                    stroke = escape_css_attr(stroke_color),
                    stroke_width = escape_css_attr(&stroke_width),
                );
            }
            write_area_label(&mut out, area, 48.0 * layout.scale, text_color.as_str());
            out.push_str("</g>");
            circle_index += 1;
        } else {
            let custom_fill = style_value(styles, "fill");
            let text_color = style_value(styles, "color").unwrap_or(theme.set_text_color.as_str());
            let _ = write!(
                &mut out,
                r#"<g class="venn-area venn-intersection" data-venn-sets="{sets}">"#,
                sets = escape_attr(&data_sets_attr(&area.sets)),
            );
            if is_hand_drawn {
                if let Some(fill) = custom_fill {
                    let fill_path =
                        rough_intersection_fill_path(&area.path, fill, &hand_drawn_seed)?;
                    let fill_stroke = transparentize(fill, 0.3)?;
                    let _ = write!(
                        &mut out,
                        r#"<g><path d="{fill_path}" stroke="{fill_stroke}" stroke-width="2" fill="none"/></g>"#,
                        fill_path = escape_attr(&fill_path),
                        fill_stroke = escape_attr(&fill_stroke),
                    );
                } else {
                    let _ = write!(
                        &mut out,
                        r#"<path d="{path}" style="fill-opacity: 0;"/>"#,
                        path = escape_attr(&area.path),
                    );
                }
            } else {
                let fill = custom_fill.unwrap_or("transparent");
                let fill_opacity = if custom_fill.is_some() { "1" } else { "0" };
                let _ = write!(
                    &mut out,
                    r#"<path d="{path}" style="fill-opacity: {fill_opacity}; fill: {fill};"/>"#,
                    path = escape_attr(&area.path),
                    fill_opacity = fill_opacity,
                    fill = escape_css_attr(fill),
                );
            }
            write_area_label(&mut out, area, 48.0 * layout.scale, text_color);
            out.push_str("</g>");
        }
    }

    if !layout.text_areas.is_empty() {
        let mut nodes_by_key: HashMap<String, Vec<&crate::model::VennTextNodeLayout>> =
            HashMap::new();
        for node in &layout.text_nodes {
            nodes_by_key
                .entry(stable_sets_key(&node.sets))
                .or_default()
                .push(node);
        }

        out.push_str(r#"<g class="venn-text-nodes">"#);
        for text_area in &layout.text_areas {
            let key = stable_sets_key(&text_area.sets);
            let nodes = nodes_by_key.get(&key).map(Vec::as_slice).unwrap_or(&[]);
            let _ = write!(
                &mut out,
                r#"<g class="venn-text-area" font-size="{font_size}px">"#,
                font_size = fmt(text_area.font_size)
            );
            if layout.use_debug_layout {
                let _ = write!(
                    &mut out,
                    r#"<circle class="venn-text-debug-circle" cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="purple" stroke-width="{stroke_width}" stroke-dasharray="{dash} {gap}"/>"#,
                    cx = fmt(text_area.center_x),
                    cy = fmt(text_area.center_y),
                    r = fmt(text_area.inner_radius),
                    stroke_width = fmt(1.5 * layout.scale),
                    dash = fmt(6.0 * layout.scale),
                    gap = fmt(4.0 * layout.scale)
                );
                for cell in &text_area.debug_cells {
                    let _ = write!(
                        &mut out,
                        r#"<rect class="venn-text-debug-cell" x="{x}" y="{y}" width="{width}" height="{height}" fill="none" stroke="teal" stroke-width="{stroke_width}" stroke-dasharray="{dash} {gap}"/>"#,
                        x = fmt(cell.x),
                        y = fmt(cell.y),
                        width = fmt(cell.width),
                        height = fmt(cell.height),
                        stroke_width = fmt(layout.scale),
                        dash = fmt(4.0 * layout.scale),
                        gap = fmt(3.0 * layout.scale)
                    );
                }
            }

            for node in nodes {
                let text_color = style_by_key
                    .get(&node.id)
                    .and_then(|styles| style_value(Some(styles), "color"));
                let mut span_style = "display: flex; width: 100%; height: 100%; white-space: normal; align-items: center; justify-content: center; text-align: center; overflow-wrap: normal; word-break: normal;".to_string();
                if let Some(text_color) = text_color {
                    span_style.push_str(" color: ");
                    span_style.push_str(text_color);
                    span_style.push(';');
                }
                let label = node.label.as_deref().unwrap_or(node.id.as_str());
                let _ = write!(
                    &mut out,
                    r#"<foreignObject class="venn-text-node-fo" width="{width}" height="{height}" x="{x}" y="{y}" overflow="visible"><span xmlns="http://www.w3.org/1999/xhtml" class="venn-text-node" style="{style}">{label}</span></foreignObject>"#,
                    width = fmt(node.width),
                    height = fmt(node.height),
                    x = fmt(node.x),
                    y = fmt(node.y),
                    style = escape_attr(&span_style),
                    label = escape_xml(label)
                );
            }
            out.push_str("</g>");
        }
        out.push_str("</g>");
    }

    out.push_str("</g></svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}
