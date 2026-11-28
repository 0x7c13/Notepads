#![forbid(unsafe_code)]

// NOTE: This fallback module intentionally keeps parsing "cheap" and non-validating.
// It is a best-effort readability fallback for SVG consumers that do not fully
// support HTML inside `<foreignObject>` (e.g. many rasterizers).

mod attr;
mod cascade;
mod context;
mod css;
mod html;
mod transform;

use crate::svg::pipeline::{
    SvgPostprocessExecution, SvgStructureMetrics, SvgTagScanner, checkpoint_loop, end_tag_name,
    escape_xml_attr_with_checkpoints, escape_xml_text_with_checkpoints,
    find_tag_end_with_checkpoints, find_with_checkpoints, rfind_with_checkpoints, start_tag_name,
};
use crate::text::TextMeasurer;
use std::collections::HashSet;
use std::convert::Infallible;
use std::fmt::{self, Write};

use attr::is_self_closing;
use cascade::{CascadeIndex, Namespace, SourceElement};
use context::{SourceFrame, attribute, source_class_attr_tokens};
use html::{foreign_object_html_wrapping, normalized_foreign_object_text, wrap_html_text};

/// Adds a best-effort `<text>/<tspan>` overlay extracted from Mermaid label `<foreignObject>`
/// content.
///
/// Many headless SVG renderers and rasterizers do not fully support HTML inside `<foreignObject>`.
/// The returned SVG aims to be *more readable* for raster outputs and UI previews.
///
/// Important:
/// - This does not aim for Mermaid DOM parity.
/// - For parity-focused SVG output, keep the original SVG unchanged.
pub fn foreign_object_label_fallback_svg_text(
    svg: &str,
    text_measurer: &dyn TextMeasurer,
) -> String {
    let mut checkpoint = || Ok::<(), Infallible>(());
    match foreign_object_label_fallback_svg_text_with_checkpoints(
        svg,
        text_measurer,
        &mut checkpoint,
        &mut |_, _| Ok::<(), Infallible>(()),
        &mut |_, _, _| Ok::<(), Infallible>(()),
        &mut |_| Ok::<(), Infallible>(()),
    ) {
        Ok(output) => output,
        Err(error) => match error {},
    }
}

/// Controlled variant used by the SVG postprocess pipeline.
///
/// The public helper above remains infallible for compatibility. Pipeline callers provide the
/// operation-owned Postprocess checkpoint so a host measurement that declines, fails, or returns
/// an invalid value cannot enter its fallback backend after cancellation.
pub(crate) fn foreign_object_label_fallback_svg_text_controlled(
    svg: &str,
    text_measurer: &dyn TextMeasurer,
    execution: SvgPostprocessExecution<'_>,
    structure: SvgStructureMetrics,
) -> crate::Result<String> {
    foreign_object_label_fallback_svg_text_with_checkpoints(
        svg,
        text_measurer,
        &mut || execution.checkpoint(),
        &mut |actual, maximum| Err(execution.selector_index_limit(actual, maximum)),
        &mut |projected_bytes, generated_elements, generated_depth| {
            execution.preflight_svg_byte_count(projected_bytes)?;
            execution.preflight_svg_structure(
                structure.elements.saturating_add(generated_elements),
                structure.max_tree_depth.max(generated_depth),
            )
        },
        &mut |message| {
            Err(crate::Error::SvgPostprocess {
                pass: "foreign-object-fallback".to_string(),
                message: message.to_string(),
            })
        },
    )
}

fn parse_attr_f64<E>(
    tag: &str,
    name: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<f64>, E> {
    let Some(value) = attribute(tag, name, checkpoint)? else {
        return Ok(None);
    };
    checkpoint()?;
    let value = value.parse::<f64>().ok();
    checkpoint()?;
    Ok(value)
}

fn foreign_object_label_fallback_svg_text_with_checkpoints<E>(
    svg: &str,
    text_measurer: &dyn TextMeasurer,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
    selector_limit: &mut impl FnMut(usize, usize) -> Result<(), E>,
    preflight_generated: &mut impl FnMut(usize, usize, usize) -> Result<(), E>,
    invalid_geometry: &mut impl FnMut(&str) -> Result<(), E>,
) -> Result<String, E> {
    checkpoint()?;
    if find_with_checkpoints(svg, "<foreignObject", checkpoint)?.is_none() {
        return Ok(svg.to_string());
    }

    let close_tag = "</foreignObject>";
    preflight_generated(svg.len(), 0, 0)?;
    let mut out = String::with_capacity(svg.len());
    let mut overlays = String::new();
    let (mut identifiers, namespace) = collect_svg_ids(svg, checkpoint)?;
    let mut fallback_index = 0usize;
    let mut instance_bytes = 0usize;
    let mut generated_elements = 0usize;
    let mut geometry_stack: Vec<SourceFrame> = Vec::new();
    let mut source_stack: Vec<SourceElement> = Vec::new();
    let mut source_stack_overflow_depth = 0usize;
    let mut cascade_index = None;
    let mut i = 0usize;
    let mut iteration = 0usize;
    while let Some(lt_rel) = find_with_checkpoints(&svg[i..], "<", checkpoint)? {
        checkpoint_loop(iteration, checkpoint)?;
        iteration = iteration.saturating_add(1);
        let lt = i + lt_rel;
        out.push_str(&svg[i..lt]);

        let Some(gt) = find_tag_end_with_checkpoints(svg, lt, checkpoint)? else {
            out.push_str(&svg[lt..]);
            i = svg.len();
            break;
        };
        let gt = gt + 1;
        let tag = &svg[lt..gt];

        // Comments / declarations: passthrough.
        if tag.starts_with("<!--") || tag.starts_with("<!") || tag.starts_with("<?") {
            out.push_str(tag);
            i = gt;
            continue;
        }

        if let Some(name) = end_tag_name(tag) {
            geometry_stack.pop();
            if source_stack_overflow_depth > 0 {
                source_stack_overflow_depth -= 1;
            } else if source_stack
                .last()
                .is_some_and(|element| element.local_name.eq_ignore_ascii_case(name))
            {
                source_stack.pop();
            }
            out.push_str(tag);
            i = gt;
            continue;
        }

        let start_name = start_tag_name(tag);
        if start_name.is_some_and(|name| name.eq_ignore_ascii_case("foreignObject")) {
            let start_end = gt;
            let Some(close_rel) = find_with_checkpoints(&svg[start_end..], close_tag, checkpoint)?
            else {
                out.push_str(&svg[lt..]);
                i = svg.len();
                break;
            };
            let inner_start = start_end;
            let inner_end = inner_start + close_rel;
            let inner = &svg[inner_start..inner_end];
            let i_next = inner_end + close_tag.len();

            out.push_str(&svg[lt..i_next]);

            let from_switch =
                is_foreign_object_switch_native_fallback(svg, lt, i_next, checkpoint)?;

            let width = parse_attr_f64(tag, "width", checkpoint)?.unwrap_or(0.0);
            let height = parse_attr_f64(tag, "height", checkpoint)?.unwrap_or(0.0);
            if !width.is_finite() || !height.is_finite() {
                invalid_geometry("foreignObject dimensions must be finite")?;
                i = i_next;
                continue;
            }
            if width > 0.0 && height > 0.0 {
                let x = parse_attr_f64(tag, "x", checkpoint)?.unwrap_or(0.0);
                let y = parse_attr_f64(tag, "y", checkpoint)?.unwrap_or(0.0);
                if !x.is_finite() || !y.is_finite() {
                    invalid_geometry("foreignObject coordinates must be finite")?;
                    i = i_next;
                    continue;
                }
                let (anchor, text_x) = match attribute(tag, "text-anchor", checkpoint)?.as_deref() {
                    Some("start") => ("start", x),
                    Some("end") => ("end", x + width),
                    _ => ("middle", x + width / 2.0),
                };
                let text_y = y + height / 2.0;

                checkpoint()?;
                let cascade = match cascade_index.as_mut() {
                    Some(cascade) => cascade,
                    None => {
                        let cascade = CascadeIndex::new(svg, checkpoint, selector_limit)?;
                        cascade_index = Some(cascade);
                        cascade_index
                            .as_mut()
                            .expect("cascade index was initialized")
                    }
                };
                let resolved = cascade.resolve_foreign_object(
                    &source_stack,
                    tag,
                    inner,
                    checkpoint,
                    selector_limit,
                )?;
                let css_transform = resolved
                    .paint
                    .iter()
                    .find(|(property, _)| property == "transform")
                    .map(|(_, value)| value);
                let effective_tag = if let Some(css_transform) =
                    css_transform.filter(|value| !value.contains("px") && !value.contains('%'))
                {
                    let escaped = escape_xml_attr_with_checkpoints(css_transform, checkpoint)?;
                    format!(r#"<g transform="{escaped}"/>"#)
                } else {
                    tag.to_string()
                };
                let matrix = match geometry_stack.last() {
                    Some(frame) => frame.foreign_object_transform(&effective_tag, checkpoint)?,
                    None => transform::Affine::from_tag(&effective_tag, checkpoint)?,
                };
                let Some(matrix) = matrix else {
                    invalid_geometry(
                        "foreignObject transform or viewport cannot be resolved to a finite SVG matrix",
                    )?;
                    i = i_next;
                    continue;
                };
                if !matrix.maps_finite_rect(x, y, width, height) {
                    invalid_geometry("foreignObject coordinates must be finite")?;
                    i = i_next;
                    continue;
                }
                let normalized = normalized_foreign_object_text(&resolved.fragments, checkpoint)?;
                if !normalized.is_empty() {
                    let typography = resolved.typography;
                    let source_attr = if from_switch {
                        r#" data-merman-foreignobject-source="switch-native-fallback""#
                    } else {
                        ""
                    };
                    let source_classes =
                        source_class_attr_tokens(&geometry_stack, tag, inner, checkpoint)?;
                    let source_classes_attr = source_classes
                        .as_deref()
                        .map(|classes| format!(r#" data-merman-source-classes="{classes}""#))
                        .unwrap_or_default();
                    let measure_style = typography.text_style();
                    let wrapping = foreign_object_html_wrapping(tag, inner, checkpoint)?;
                    let lines = wrap_html_text(
                        &normalized,
                        wrapping,
                        text_measurer,
                        &measure_style,
                        checkpoint,
                    )?;
                    let line_height = typography.line_height;
                    let y0 = text_y - line_height * (lines.len() as f64 - 1.0) / 2.0;
                    let mut positions = Vec::with_capacity(lines.len());
                    let mut valid = true;
                    for (idx, line) in lines.iter().enumerate() {
                        checkpoint_loop(idx, checkpoint)?;
                        let anchor_factor = match anchor {
                            "middle" => 0.5,
                            "end" => 1.0,
                            _ => 0.0,
                        };
                        let line_x = text_x + (line.width - line.alignment_width) * anchor_factor;
                        let y_line = y0 + idx as f64 * line_height;
                        checkpoint()?;
                        // Emitted rows are decoded SVG text, not a new HTML label.
                        let bbox_width = text_measurer
                            .measure_svg_raw_text_bbox_width_px(&line.text, &measure_style);
                        checkpoint()?;
                        let bbox_height = text_measurer
                            .measure_svg_raw_text_bbox_height_px(&line.text, &measure_style);
                        checkpoint()?;
                        let measured_width = bbox_width.max(line.width);
                        if !line_x.is_finite()
                            || !y_line.is_finite()
                            || !bbox_width.is_finite()
                            || !bbox_height.is_finite()
                            || bbox_width < 0.0
                            || bbox_height < 0.0
                            || !matrix.maps_finite_rect(
                                line_x - measured_width * anchor_factor,
                                y_line - bbox_height / 2.0,
                                measured_width,
                                bbox_height,
                            )
                        {
                            invalid_geometry(
                                "foreignObject fallback text geometry must be finite",
                            )?;
                            valid = false;
                            break;
                        }
                        positions.push((line_x, y_line));
                    }
                    if !valid {
                        i = i_next;
                        continue;
                    }
                    // Keep the actual source ancestors for opacity, masks, clipping and viewports.
                    // Explicit styles isolate generated shapes from node/cluster CSS selectors.
                    let local = transform::Affine::from_tag(tag, checkpoint)?.unwrap_or_default();
                    let transform_attr = if local == transform::Affine::default() {
                        String::new()
                    } else {
                        format!(
                            r#" transform="matrix({} {} {} {} {} {})""#,
                            local.a, local.b, local.c, local.d, local.e, local.f
                        )
                    };
                    let mut paint_style = "opacity: 1; display: inline; visibility: inherit; clip-path: none; mask: none; filter: none;".to_string();
                    for (property, value) in &resolved.paint {
                        if property != "overflow" {
                            write!(paint_style, " {property}: {value};")
                                .expect("writing into a string");
                        }
                    }
                    if !resolved
                        .paint
                        .iter()
                        .any(|(property, _)| property == "transform")
                    {
                        write!(
                            paint_style,
                            " transform: matrix({} {} {} {} {} {});",
                            local.a, local.b, local.c, local.d, local.e, local.f
                        )
                        .expect("writing into a string");
                    }
                    let escaped_paint = escape_xml_attr_with_checkpoints(&paint_style, checkpoint)?;
                    let mut metadata = String::new();
                    for name in [
                        "role",
                        "aria-label",
                        "aria-labelledby",
                        "aria-describedby",
                        "aria-hidden",
                    ] {
                        if let Some(value) = attribute(tag, name, checkpoint)? {
                            let value =
                                escape_xml_attr_with_checkpoints(value.as_ref(), checkpoint)?;
                            write!(metadata, r#" {name}="{value}""#)
                                .expect("writing into a string");
                        }
                    }
                    let overlay_start = overlays.len();
                    let generated_depth = geometry_stack.len().saturating_add(2).max(5);
                    let identifier = loop {
                        let candidate = format!(
                            "{}-foreignobject-label-{fallback_index}",
                            namespace.as_deref().unwrap_or("merman")
                        );
                        fallback_index = fallback_index.saturating_add(1);
                        if identifiers.insert(candidate.clone()) {
                            break candidate;
                        }
                        checkpoint()?;
                    };
                    push_generated_fmt(
                        &mut overlays,
                        format_args!(
                            r#"<g data-merman-foreignobject="fallback"{source_attr} class="merman-foreignobject-fallback"{source_classes_attr}{transform_attr}{metadata} style="{escaped_paint}">"#
                        ),
                        1,
                        svg.len().saturating_add(instance_bytes),
                        generated_depth,
                        &mut generated_elements,
                        preflight_generated,
                    )?;
                    let inner_start = overlays.len();
                    let overflow = resolved
                        .paint
                        .iter()
                        .find(|(property, _)| property == "overflow")
                        .map_or("hidden", |(_, value)| value.as_str());
                    let escaped_overflow = escape_xml_attr_with_checkpoints(overflow, checkpoint)?;
                    push_generated_fmt(
                        &mut overlays,
                        format_args!(
                            r#"<svg xmlns="http://www.w3.org/2000/svg" x="0" y="0" width="{width}" height="{height}" viewBox="0 0 {width} {height}" preserveAspectRatio="none" style="overflow: {escaped_overflow}; opacity: 1; display: inline; visibility: inherit; clip-path: none; mask: none; filter: none; transform: none;">"#
                        ),
                        1,
                        svg.len().saturating_add(instance_bytes),
                        generated_depth,
                        &mut generated_elements,
                        preflight_generated,
                    )?;
                    // Preserve the source viewport's minimum geometric extent. Native glyph
                    // bboxes can still enlarge objectBoundingBox paints (a browser text residual).
                    push_generated_fmt(
                        &mut overlays,
                        format_args!(
                            r#"<rect x="0" y="0" width="{width}" height="{height}" fill="none" style="fill: none; stroke: none; opacity: 0; display: inline; transform: none;"/>"#
                        ),
                        1,
                        svg.len().saturating_add(instance_bytes),
                        generated_depth,
                        &mut generated_elements,
                        preflight_generated,
                    )?;
                    if let Some(label_bkg) = &typography.label_background {
                        let escaped_label_bkg =
                            escape_xml_attr_with_checkpoints(label_bkg, checkpoint)?;
                        push_generated_fmt(
                            &mut overlays,
                            format_args!(
                                r#"<rect x="0" y="0" width="{width}" height="{height}" fill="{escaped_label_bkg}" style="fill: {escaped_label_bkg}; fill-opacity: 1; stroke: none; opacity: 1; display: inline; visibility: inherit; clip-path: none; mask: none; filter: none; transform: none;"/>"#
                            ),
                            1,
                            svg.len().saturating_add(instance_bytes),
                            generated_depth,
                            &mut generated_elements,
                            preflight_generated,
                        )?;
                    }
                    let mut text_style = format!(
                        "text-anchor: {anchor}; font-size: {}px; font-family: {}; line-height: {}px; fill: {}; fill-opacity: 1; stroke: none; opacity: 1; display: inline; visibility: inherit; clip-path: none; mask: none; filter: none; transform: none; white-space: pre; dominant-baseline: central; alignment-baseline: central;",
                        typography.font_size,
                        typography.font_family,
                        typography.line_height,
                        typography.fill,
                    );
                    write!(
                        text_style,
                        " font-weight: {}; font-style: {};",
                        typography.font_weight.as_deref().unwrap_or("normal"),
                        typography.font_style.as_deref().unwrap_or("normal"),
                    )
                    .expect("writing into a string");
                    let escaped_fill =
                        escape_xml_attr_with_checkpoints(&typography.fill, checkpoint)?;
                    let escaped_style = escape_xml_attr_with_checkpoints(&text_style, checkpoint)?;
                    let text_attributes = format!(
                        r#" class="merman-foreignobject-fallback-text"{source_classes_attr}"#
                    );
                    for (idx, (line, (line_x, y_line))) in lines.iter().zip(positions).enumerate() {
                        checkpoint_loop(idx, checkpoint)?;
                        let text = escape_xml_text_with_checkpoints(&line.text, checkpoint)?;
                        let line_x = line_x - x;
                        let y_line = y_line - y;
                        push_generated_fmt(
                            &mut overlays,
                            format_args!(
                                r##"<text x="{line_x}" y="{y_line}" text-anchor="{anchor}" dominant-baseline="central" alignment-baseline="central" fill="{escaped_fill}"{text_attributes} xml:space="preserve" style="{escaped_style}">{text}</text>"##
                            ),
                            1,
                            svg.len().saturating_add(instance_bytes),
                            generated_depth,
                            &mut generated_elements,
                            preflight_generated,
                        )?;
                    }
                    push_generated_fmt(
                        &mut overlays,
                        format_args!("</svg>"),
                        0,
                        svg.len().saturating_add(instance_bytes),
                        generated_depth,
                        &mut generated_elements,
                        preflight_generated,
                    )?;
                    let inner_end = overlays.len();
                    let wrapper = overlays[overlay_start..inner_start].to_string();
                    let inner = overlays[inner_start..inner_end].to_string();
                    overlays.truncate(overlay_start);
                    let isolated =
                        format!(r#"<defs><g id="{identifier}"{source_attr}>{inner}</g></defs>"#);
                    push_generated_fmt(
                        &mut overlays,
                        format_args!("{isolated}"),
                        2,
                        svg.len().saturating_add(instance_bytes),
                        generated_depth,
                        &mut generated_elements,
                        preflight_generated,
                    )?;
                    let instance = format!(
                        r##"{wrapper}<use x="{x}" y="{y}" href="#{identifier}" style="opacity: 1; display: inline; visibility: inherit; clip-path: none; mask: none; filter: none; transform: none;"/></g>"##
                    );
                    let projected_instance_bytes = instance_bytes.saturating_add(instance.len());
                    preflight_generated(
                        svg.len()
                            .saturating_add(overlays.len())
                            .saturating_add(projected_instance_bytes),
                        generated_elements.saturating_add(1),
                        generated_depth,
                    )?;
                    generated_elements = generated_elements.saturating_add(1);
                    instance_bytes = projected_instance_bytes;
                    out.push_str(&instance);
                }
            }
            i = i_next;
            continue;
        }

        // Source styles are consumed by `CascadeIndex` in its first pass. Do
        // not treat CSS text (or CDATA containing a literal `</style>`) as SVG
        // structure while building the source ancestry for conversion.
        if start_name.is_some_and(|name| name.eq_ignore_ascii_case("style"))
            && !is_self_closing(tag)
        {
            let mut scanner = SvgTagScanner::new(svg);
            scanner.skip_to(gt);
            let mut close_end = None;
            while let Some(candidate) = scanner.next_with_checkpoints(checkpoint)? {
                if end_tag_name(candidate.raw())
                    .is_some_and(|name| name.eq_ignore_ascii_case("style"))
                {
                    close_end = Some(scanner.cursor());
                    break;
                }
            }
            if let Some(close_end) = close_end {
                out.push_str(&svg[lt..close_end]);
                i = close_end;
                continue;
            }
        }

        if let Some(name) = start_name
            && !is_self_closing(tag)
        {
            if source_stack_overflow_depth > 0 {
                source_stack_overflow_depth = source_stack_overflow_depth.saturating_add(1);
            } else if source_stack.len() >= cascade::MAX_SELECTOR_ANCESTRY_DEPTH {
                selector_limit(
                    source_stack.len().saturating_add(1),
                    cascade::MAX_SELECTOR_ANCESTRY_DEPTH,
                )?;
                source_stack_overflow_depth = 1;
            } else {
                source_stack.push(CascadeIndex::source_element(
                    tag,
                    Namespace::Svg,
                    checkpoint,
                )?);
            }
            geometry_stack.push(SourceFrame::from_tag(
                tag,
                name,
                geometry_stack.last(),
                source_stack_overflow_depth == 0,
                checkpoint,
            )?);
        }

        out.push_str(tag);
        i = gt;
    }

    if i < svg.len() {
        out.push_str(&svg[i..]);
    }

    if !overlays.is_empty()
        && let Some(close) = rfind_with_checkpoints(&out, "</svg>", checkpoint)?
    {
        preflight_generated(
            out.len().saturating_add(overlays.len()),
            generated_elements,
            5,
        )?;
        out.insert_str(close, &overlays);
    }
    checkpoint()?;
    Ok(out)
}

fn collect_svg_ids<E>(
    svg: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<(HashSet<String>, Option<String>), E> {
    let mut result = HashSet::new();
    let mut namespace = None;
    let mut saw_root = false;
    let mut scanner = SvgTagScanner::new(svg);
    while let Some(tag) = scanner.next_with_checkpoints(checkpoint)? {
        if let Some(name) = start_tag_name(tag.raw()) {
            let id = attribute(tag.raw(), "id", checkpoint)?;
            if !saw_root && name.eq_ignore_ascii_case("svg") {
                saw_root = true;
                // Renderer roots carry document-unique, fragment-safe diagram IDs.
                namespace = id
                    .as_deref()
                    .filter(|id| {
                        !id.is_empty()
                            && id
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
                    })
                    .map(str::to_owned);
            }
            if let Some(id) = id {
                result.insert(id.into_owned());
            }
        }
    }
    Ok((result, namespace))
}

#[derive(Default)]
struct FmtByteCounter {
    bytes: usize,
}

impl Write for FmtByteCounter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.bytes = self.bytes.saturating_add(value.len());
        Ok(())
    }
}

fn push_generated_fmt<E>(
    output: &mut String,
    arguments: fmt::Arguments<'_>,
    added_elements: usize,
    source_bytes: usize,
    generated_depth: usize,
    generated_elements: &mut usize,
    preflight_generated: &mut impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<(), E> {
    let mut counter = FmtByteCounter::default();
    counter
        .write_fmt(arguments)
        .expect("counting formatted SVG bytes cannot fail");
    let projected_bytes = source_bytes
        .saturating_add(output.len())
        .saturating_add(counter.bytes);
    let projected_elements = generated_elements.saturating_add(added_elements);
    preflight_generated(projected_bytes, projected_elements, generated_depth)?;
    output.reserve(counter.bytes);
    output
        .write_fmt(arguments)
        .expect("writing formatted SVG into a String cannot fail");
    *generated_elements = projected_elements;
    Ok(())
}

fn is_foreign_object_switch_native_fallback<E>(
    svg: &str,
    foreign_object_start: usize,
    foreign_object_end: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<bool, E> {
    let Some(switch_start) = find_wrapping_switch_start(svg, foreign_object_start, checkpoint)?
    else {
        return Ok(false);
    };
    let Some(switch_close_rel) =
        find_with_checkpoints(&svg[foreign_object_end..], "</switch>", checkpoint)?
    else {
        return Ok(false);
    };

    let has_intervening_close = find_with_checkpoints(
        &svg[switch_start..foreign_object_start],
        "</switch>",
        checkpoint,
    )?
    .is_some();
    let has_native_text = find_with_checkpoints(
        &svg[foreign_object_end..foreign_object_end + switch_close_rel],
        "<text",
        checkpoint,
    )?
    .is_some();
    Ok(!has_intervening_close && has_native_text)
}

fn find_wrapping_switch_start<E>(
    svg: &str,
    before: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    let mut search_end = before;
    while search_end > 0 {
        let Some(start) = rfind_with_checkpoints(&svg[..search_end], "<switch", checkpoint)? else {
            return Ok(None);
        };
        let Some(open_end) = find_tag_end_with_checkpoints(svg, start, checkpoint)? else {
            return Ok(None);
        };
        if open_end >= before {
            search_end = start;
            continue;
        }

        let tag = &svg[start..=open_end];
        if is_start_switch_tag(tag) {
            return Ok(Some(start));
        }

        search_end = start;
    }
    Ok(None)
}

fn is_start_switch_tag(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    tag.starts_with("<switch")
        && bytes
            .get("<switch".len())
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/')
        && !tag.trim_end().ends_with("/>")
}

#[cfg(test)]
mod tests {
    use super::foreign_object_label_fallback_svg_text as render_fallback;
    use crate::text::{DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle};
    use std::cell::RefCell;

    fn has_background_rect(svg: &str, color: &str, width: &str, height: &str) -> bool {
        let document = roxmltree::Document::parse(svg).unwrap();
        document.descendants().any(|node| {
            node.has_tag_name("rect")
                && node.attribute("fill") == Some(color)
                && node.attribute("x") == Some("0")
                && node.attribute("y") == Some("0")
                && node.attribute("width") == Some(width)
                && node.attribute("height") == Some(height)
        })
    }

    #[test]
    fn break_spaces_overlay_preserves_measured_trailing_spaces_in_svg_text() {
        let measurer =
            DeterministicTextMeasurer::default().with_width_callback(|text, _| text.len() as f64);
        let svg = r#"<svg><foreignObject width="9.5" height="100"><div style="white-space: break-spaces; width: 9.5px">iiii iiii iiii iiii iiii iiii</div></foreignObject></svg>"#;
        let converted = render_fallback(svg, measurer.as_ref());
        assert_eq!(converted.matches(r#"xml:space="preserve""#).count(), 5);
        assert_eq!(converted.matches(">iiii </text>").count(), 4);
        assert_eq!(converted.matches(">iiii iiii</text>").count(), 1);

        let normal = svg.replace("break-spaces", "normal");
        let converted = render_fallback(&normal, measurer.as_ref());
        // Normal-mode runs are already collapsed before SVG emission; preserve that result.
        assert_eq!(converted.matches(r#"xml:space="preserve""#).count(), 3);
        assert_eq!(converted.matches(">iiii iiii</text>").count(), 3);
    }

    #[test]
    fn overlay_keeps_nbsp_boundaries_and_explicit_empty_break_rows() {
        let measurer = DeterministicTextMeasurer::default();
        let nbsp = r#"<svg><foreignObject width="200" height="100"><div>&nbsp;A&nbsp;</div></foreignObject></svg>"#;
        let converted = render_fallback(nbsp, &measurer);
        assert!(converted.contains(">\u{a0}A\u{a0}</text>"));

        let breaks = r#"<svg><foreignObject width="200" height="100"><div>A<br/><br/>B</div></foreignObject></svg>"#;
        let converted = render_fallback(breaks, &measurer);
        assert_eq!(
            converted
                .matches("class=\"merman-foreignobject-fallback-text\"")
                .count(),
            3
        );
        assert!(converted.contains("></text>"));
        assert!(converted.contains(">A</text>"));
        assert!(converted.contains(">B</text>"));
    }

    #[test]
    fn break_spaces_overlay_keeps_boundary_spaces_and_normal_collapses_indentation() {
        let measurer = DeterministicTextMeasurer::default();
        let svg = r#"<svg><foreignObject width="200" height="100"><div style="white-space: break-spaces">  A  </div></foreignObject></svg>"#;
        let converted = render_fallback(svg, &measurer);
        assert!(converted.contains(">  A  </text>"));
        assert!(converted.contains(r#"xml:space="preserve""#));

        let normal = "<svg><foreignObject width=\"200\" height=\"100\">\n  <div>\n    A   B\n  </div>\n</foreignObject></svg>";
        let converted = render_fallback(normal, &measurer);
        assert!(converted.contains(">A B</text>"));
        assert_eq!(
            converted
                .matches("class=\"merman-foreignobject-fallback-text\"")
                .count(),
            1
        );
    }

    #[test]
    fn overlay_discards_only_trailing_empty_break_rows() {
        let measurer = DeterministicTextMeasurer::default();
        for (white_space, text, expected_rows) in [
            ("normal", "A<br/>", 1),
            ("break-spaces", "A<br/>", 1),
            ("normal", "A<br/><br/>B", 3),
            ("normal", "A<br/>&nbsp;", 2),
            ("break-spaces", "A<br/>  ", 2),
        ] {
            let svg = format!(
                r#"<svg><foreignObject width="200" height="100"><div style="white-space: {white_space}">{text}</div></foreignObject></svg>"#,
            );
            let converted = render_fallback(&svg, &measurer);
            assert_eq!(
                converted
                    .matches("class=\"merman-foreignobject-fallback-text\"")
                    .count(),
                expected_rows,
                "{white_space}: {text}",
            );
        }
    }

    #[derive(Default)]
    struct RecordingMeasurer {
        styles: RefCell<Vec<TextStyle>>,
    }

    impl TextMeasurer for RecordingMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            self.styles.borrow_mut().push(style.clone());
            TextMetrics {
                width: text.chars().count() as f64 * style.font_size,
                height: style.font_size,
                line_count: 1,
            }
        }
    }

    fn foreign_object_label_fallback_svg_text(svg: &str) -> String {
        render_fallback(svg, &DeterministicTextMeasurer::default())
    }

    #[test]
    fn foreign_object_inside_switch_with_native_text_generates_tagged_fallback() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><switch><foreignObject x="150" y="50" width="550" height="50"><div class="journey-section" xmlns="http://www.w3.org/1999/xhtml" style="display: table; height: 100%; width: 100%;"><div class="label" style="display: table-cell; text-align: center; vertical-align: middle;">Go to work</div></div></foreignObject><text x="425" y="75" fill="#333"><tspan x="425" dy="0">Go to work</tspan></text></switch></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);
        assert!(
            out.contains(r#"data-merman-foreignobject="fallback""#),
            "should generate fallback overlay: {out}"
        );
        assert!(
            out.contains(r#"data-merman-foreignobject-source="switch-native-fallback""#),
            "fallback should be tagged with switch source: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_accounts_for_parent_translate() {
        let svg = r#"<svg viewBox="90 -310 425 99" xmlns="http://www.w3.org/2000/svg"><g transform="translate(183.3046875, -300)"><foreignObject width="33.390625" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><p>Todo</p></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);
        assert!(
            out.contains(r#"transform="translate(183.3046875, -300)""#)
                && out.contains(r#"x="16.6953125""#),
            "the local center plus the parent matrix must place x at 200"
        );
        assert!(
            out.contains(r#"y="12""#),
            "the local center plus the parent matrix must place y at -288"
        );
        assert!(
            out.contains(">Todo<"),
            "expected text content to be present"
        );
    }

    #[test]
    fn foreign_object_overlay_renders_label_bkg_rect_when_present() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>#d .labelBkg{background-color:rgba(232,232,232,0.5);}</style><g id="d"><foreignObject x="10" y="20" width="30" height="24"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><p>Hello</p></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);
        assert!(
            out.contains(r#"fill="rgba(232,232,232,0.5)""#),
            "expected labelBkg fill"
        );
        assert!(
            out.contains(r#"<rect x="0" y="0" width="30" height="24""#)
                && out.contains(r#"<use x="10" y="20""#),
            "expected rect with foreignObject bounds"
        );
    }

    #[test]
    fn foreign_object_overlay_does_not_inherit_background_color_by_default() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g style="background-color:#ff0000"><foreignObject x="10" y="20" width="30" height="24"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><p>Hello</p></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);
        assert!(
            !out.contains(r##"fill="#ff0000""##),
            "background-color is not inherited by a child without a specified value: {out}"
        );

        let explicit_initial = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.labelBkg{background-color:initial}</style><foreignObject x="10" y="20" width="30" height="24"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><p>Hello</p></div></foreignObject></svg>"#;
        let out = foreign_object_label_fallback_svg_text(explicit_initial);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(
            !fallback.contains("rgba(232, 232, 232, 0.5)"),
            "explicit background-color: initial must not become the compatibility gray: {out}"
        );

        for clear in ["transparent", "initial"] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.labelBkg{{background-color:#ff0000}}.labelBkg .clear{{background-color:{clear}}}</style><foreignObject x="10" y="20" width="30" height="24"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="clear">Hello</span></div></foreignObject></svg>"#,
            );
            let out = foreign_object_label_fallback_svg_text(&svg);
            let fallback = out
                .split(r#"data-merman-foreignobject="fallback""#)
                .nth(1)
                .unwrap_or_else(|| panic!("expected fallback output: {out}"));
            assert!(
                !fallback.contains(r##"fill="#ff0000""##)
                    && !fallback.contains("rgba(232, 232, 232, 0.5)"),
                "a specified {clear} descendant must clear the parent fallback background: {out}"
            );
        }

        let inherited_child = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.labelBkg{background-color:#ff0000}.labelBkg .keep{background-color:inherit}</style><foreignObject x="10" y="20" width="30" height="24"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="keep">Hello</span></div></foreignObject></svg>"#;
        let out = foreign_object_label_fallback_svg_text(inherited_child);
        assert!(
            out.contains(r##"fill="#ff0000""##),
            "an explicit inherit descendant must retain the nearest resolved background: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_splits_literal_backslash_n() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(10, 20)"><foreignObject width="80" height="48"><div xmlns="http://www.w3.org/1999/xhtml"><p>Layer 7\nHTTP</p></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);
        assert!(out.contains(">Layer 7<"), "got: {out}");
        assert!(out.contains(">HTTP<"), "got: {out}");
        assert!(
            !out.contains(">Layer 7\\nHTTP</text>"),
            "literal backslash-n should not remain in fallback text overlay: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_propagates_style_context() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><g class="node selected" fill="#112233" style="font-size: 14px; font-family: Inter; font-weight: 600;"><foreignObject width="80" height="24"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg host-label" style="color: #abcdef; font-style: italic;"><p>Hello</p></div></foreignObject></g></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains(
                r#"class="merman-foreignobject-fallback" data-merman-source-classes="node selected labelBkg host-label""#
            ),
            "expected fallback group to retain source classes as inert metadata: {out}"
        );
        assert!(
            out.contains(
                r#"class="merman-foreignobject-fallback-text" data-merman-source-classes="node selected labelBkg host-label""#
            ),
            "expected fallback text to retain source classes as inert metadata: {out}"
        );
        assert!(
            out.contains(r##"fill="#abcdef""##),
            "expected inline HTML color to drive fallback fill: {out}"
        );
        assert!(
            out.contains("font-size: 14px")
                && out.contains("font-family: Inter")
                && out.contains("font-weight: 600")
                && out.contains("font-style: italic"),
            "expected font context to propagate: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_scoped_label_css_for_fallback_fill() {
        let svg = r##"<svg id="host-theme-block" xmlns="http://www.w3.org/2000/svg"><style>#host-theme-block{fill:#eeeeee;}#host-theme-block .node rect{fill:#111827;}#host-theme-block .label text,#host-theme-block span,#host-theme-block p{fill:#e5e7eb;color:#e5e7eb;}</style><g class="block"><g class="node flowchart-label"><g class="label"><foreignObject width="80" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><p>Alpha</p></div></foreignObject></g></g></g></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains(r##"fill="#e5e7eb""##),
            "expected scoped label CSS, not shape CSS/default fill, to drive fallback text: {out}"
        );
        assert!(
            !out.contains(r##"fill="#111827""##),
            "fallback text should not inherit node rectangle fill: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_root_fill_when_no_label_context_matches() {
        let svg = r##"<svg id="host-theme-root" xmlns="http://www.w3.org/2000/svg"><style>#host-theme-root{font-family:Inter;fill:#ddeeff;}#host-theme-root .node rect{fill:#111827;}</style><g><foreignObject width="80" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><p>Alpha</p></div></foreignObject></g></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains(r##"fill="#ddeeff""##),
            "expected root fill to be the final readable fallback: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_ignores_non_stylesheet_markup_before_and_inside_style() {
        let svg = r##"<svg id="brace-theme" xmlns="http://www.w3.org/2000/svg"><!-- > <style>#brace-theme{fill:red;}</style> --><text>{</text><style><![CDATA[/* </style> */ #brace-theme{fill:#ddeeff;font-family:Inter;}#brace-theme .labelBkg{background-color:#c0ffee;}]]></style><g><foreignObject x="10" y="20" width="30" height="24"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><p>Alpha</p></div></foreignObject></g></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains(r##"fill="#ddeeff""##),
            "expected the real root rule after authored brace text to drive fallback text: {out}"
        );
        assert!(
            out.contains(r##"fill="#c0ffee""##)
                && out.contains(r#"<rect x="0" y="0" width="30" height="24""#)
                && out.contains(r#"<use x="10" y="20""#),
            "expected the real labelBkg rule after authored brace text to drive fallback background: {out}"
        );
        assert!(
            out.contains("font-family: Inter"),
            "expected root font declarations to remain indexed: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_root_font_context() {
        let svg = r##"<svg id="host-theme-root-font" xmlns="http://www.w3.org/2000/svg"><style>#host-theme-root-font{font-family:Inter,system-ui;font-size:14px;fill:#ddeeff;}</style><g><foreignObject width="80" height="21"><div xmlns="http://www.w3.org/1999/xhtml"><p>Alpha</p></div></foreignObject></g></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(out.contains(r##"fill="#ddeeff""##), "got: {out}");
        assert!(out.contains("font-size: 14px"), "got: {out}");
        assert!(out.contains("font-family: Inter,system-ui"), "got: {out}");
    }

    #[test]
    fn foreign_object_overlay_does_not_flatten_descendant_font_selector() {
        let svg = r#"<svg id="class-context" xmlns="http://www.w3.org/2000/svg"><style>#class-context{font-size:16px}.classLabel .label{font-size:10px}</style><g class="node"><g class="label"><foreignObject width="80" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains("font-size: 16px"),
            "a missing .classLabel ancestor must not collapse the selector to .label: {out}"
        );
        assert!(
            !out.contains("font-size: 10px"),
            "the contextual 10px rule must not leak into the class-node label: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_matches_real_descendant_font_selector() {
        let svg = r#"<svg id="class-context-positive" xmlns="http://www.w3.org/2000/svg"><style>#class-context-positive{font-size:16px}.classLabel .label{font-size:10px}</style><g class="classLabel"><g class="label"><foreignObject width="80" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains("font-size: 10px"),
            "a fully matching contextual selector must remain effective: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_does_not_match_svg_text_selector_to_xhtml_text() {
        let svg = r#"<svg id="element-context" xmlns="http://www.w3.org/2000/svg"><style>#element-context{font-size:16px}g.classGroup text{font-size:10px}</style><g class="classGroup"><foreignObject width="80" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains("font-size: 16px"),
            "an SVG text selector must not match an XHTML span target: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_inherits_font_size_presentation_attribute() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g font-size="20px"><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span>Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains("font-size: 20px"),
            "font-size presentation attributes must participate in inheritance: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_nested_xhtml_font_style() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml" style="line-height:1.5"><span style="font-size:18px;font-style:italic">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(out.contains("font-size: 18px"), "got: {out}");
        assert!(out.contains("font-style: italic"), "got: {out}");
    }

    #[test]
    fn foreign_object_overlay_matches_admitted_attribute_selectors() {
        let svg = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>[data-fallback-role=label][data-tags~=choice]{font-size:13px}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span data-fallback-role='label' data-tags="primary choice">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(out.contains("font-size: 13px"), "got: {out}");
    }

    #[test]
    fn foreign_object_overlay_keeps_admitted_selector_siblings_and_rejects_invalid_lists() {
        let valid_but_unadmitted = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>span:hover,.nodeLabel{font-size:13px}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(valid_but_unadmitted);
        assert!(
            out.contains("font-size: 13px"),
            "a valid-but-unadmitted sibling must not discard an admitted branch: {out}"
        );

        for selector in [".a + .b", ".a ~ .b", "svg|a", "*|span", "a||b"] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>{selector},.nodeLabel{{font-size:13px}}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#,
            );
            let out = foreign_object_label_fallback_svg_text(&svg);
            assert!(
                out.contains("font-size: 13px"),
                "valid unsupported selector must preserve its admitted sibling ({selector}): {out}"
            );
        }

        let invalid = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel,{font-size:13px}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(invalid);
        assert!(
            out.contains("font-size: 16px"),
            "an invalid ordinary selector list must discard its complete rule: {out}"
        );

        let malformed_combinator = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>span >> .nodeLabel{font-size:10px}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(malformed_combinator);
        assert!(
            out.contains("font-size: 16px") && !out.contains("font-size: 10px"),
            "malformed repeated child combinators must fail closed: {out}"
        );

        let malformed_compound = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel,$bad{font-size:10px}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(malformed_compound);
        assert!(
            out.contains("font-size: 16px") && !out.contains("font-size: 10px"),
            "an invalid compound must invalidate its complete selector list: {out}"
        );

        for operator in ["|=", "^=", "$=", "*="] {
            let selector = format!("[data-tags{operator}choice],.nodeLabel");
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>{selector}{{font-size:13px}}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></svg>"#,
            );
            let out = foreign_object_label_fallback_svg_text(&svg);
            assert!(
                out.contains("font-size: 13px"),
                "a valid-but-unadmitted attribute operator must preserve an admitted sibling ({operator}): {out}"
            );
        }

        let malformed_attribute = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>[data-tags^=],.nodeLabel{font-size:13px}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></svg>"#;
        let out = foreign_object_label_fallback_svg_text(malformed_attribute);
        assert!(
            out.contains("font-size: 16px") && !out.contains("font-size: 13px"),
            "a malformed attribute selector must invalidate its complete selector list: {out}"
        );

        for malformed_selector in [
            "span:hover@bad",
            "span:hover$bad",
            "span:hover #",
            "span:hover .",
            "span:hover [data-tags=\"foo\" junk]",
            "a | b",
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>{malformed_selector},.nodeLabel{{font-size:13px}}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></svg>"#,
            );
            let out = foreign_object_label_fallback_svg_text(&svg);
            assert!(
                out.contains("font-size: 16px") && !out.contains("font-size: 13px"),
                "malformed unsupported selector must invalidate its sibling list ({malformed_selector}): {out}"
            );
        }
    }

    #[test]
    fn foreign_object_overlay_skips_nested_at_rules_without_leaking_inner_selectors() {
        let svg = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>@media screen {.nodeLabel{font-size:10px}} .nodeLabel{font-size:14px}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);
        assert!(
            out.contains("font-size: 14px"),
            "nested @media rules are outside the admitted fallback cascade and must not leak: {out}"
        );
        assert!(
            !out.contains("font-size: 10px"),
            "an inner at-rule selector must not be parsed as a top-level rule: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_resolves_important_inline_and_invalid_value_cascade() {
        let stylesheet_important = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{font-size:12px !important}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel" style="font-size:14px">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(stylesheet_important);
        assert!(out.contains("font-size: 12px"), "got: {out}");

        let inline_important = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{font-size:12px !important}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel" style="font-size:14px !important">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(inline_important);
        assert!(out.contains("font-size: 14px"), "got: {out}");

        let unsupported_winner = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{font-size:14px;font-size:calc(2px) !important}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(unsupported_winner);
        assert!(
            out.contains("font-size: 14px"),
            "an unsupported high-priority value must not erase a lower valid declaration: {out}"
        );

        let invalid_non_metric_winner = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{font-weight:600;font-weight:9999 !important;font-style:italic;font-style:bogus !important;fill:#123456;fill:definitely-not-a-paint !important;color:#abcdef;color:none !important;color:VAR(--missing) !important}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(invalid_non_metric_winner);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(fallback.contains("font-weight: 600"), "got: {out}");
        assert!(fallback.contains("font-style: italic"), "got: {out}");
        assert!(
            fallback.contains(r##"fill="#abcdef""##),
            "XHTML fallback text should prefer CSS color over SVG fill: {out}"
        );
        assert!(
            !fallback.contains("9999"),
            "invalid font weight leaked: {out}"
        );
        assert!(
            !fallback.contains("bogus"),
            "invalid font style leaked: {out}"
        );
        assert!(
            !fallback.contains("definitely-not-a-paint"),
            "invalid fill leaked: {out}"
        );
        assert!(
            !fallback.contains("VAR("),
            "case-insensitive var() leaked: {out}"
        );

        let invalid_relative_values = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{font-size:14px;line-height:1.5}.nodeLabel{font-size:20 px !important;line-height:1e308 !important}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(invalid_relative_values);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(fallback.contains("font-size: 14px"), "got: {out}");
        assert!(fallback.contains("line-height: 21px"), "got: {out}");
        assert!(
            !fallback.contains("20 px") && !fallback.contains("inf"),
            "non-finite metric leaked: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_rejects_malformed_functional_paints() {
        for invalid in [
            "rgb(foo)",
            "rgba(1)",
            "hsl(1 2 3)",
            "hsla(10,20%,30%,oops)",
            "rgb(1,2,3)garbage",
            "rgb(1, 2 3)",
            "hsl(1, 2% 3%)",
        ] {
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{{color:#123456;color:{invalid} !important}}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></svg>"##,
            );
            let out = foreign_object_label_fallback_svg_text(&svg);
            let fallback = out
                .split(r#"data-merman-foreignobject="fallback""#)
                .nth(1)
                .unwrap_or_else(|| panic!("expected fallback output: {out}"));
            assert!(
                fallback.contains(r##"fill="#123456""##) && !fallback.contains(invalid),
                "an invalid functional paint must not hide a lower valid declaration ({invalid}): {out}"
            );
        }

        for valid in [
            "rgb(18 52 86 / .5)",
            "rgb(18, 52, 86)",
            "hsl(210, 65%, 20%)",
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{{color:{valid}}}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></svg>"#,
            );
            let out = foreign_object_label_fallback_svg_text(&svg);
            assert!(
                out.contains(&format!(r#"fill="{valid}""#)),
                "a valid Mermaid functional paint must remain admitted ({valid}): {out}"
            );
        }
    }

    #[test]
    fn foreign_object_overlay_keeps_late_universal_winners() {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>"#);
        for _ in 0..super::cascade::MAX_UNIVERSAL_POSTINGS {
            svg.push_str("*{font-size:12px}");
        }
        svg.push_str(
            r#"*{font-size:19px}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span>Alpha</span></div></foreignObject></svg>"#,
        );

        let out = foreign_object_label_fallback_svg_text(&svg);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(
            fallback.contains("font-size: 12px") && !fallback.contains("font-size: 19px"),
            "the infallible helper must ignore universal rules beyond its bounded bucket: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_rejects_universal_selector_overflow_in_controlled_path() {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>"#);
        for _ in 0..=super::cascade::MAX_UNIVERSAL_POSTINGS {
            svg.push_str("*{font-size:12px}");
        }
        svg.push_str(
            r#"</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span>Alpha</span></div></foreignObject></svg>"#,
        );

        let measurer = DeterministicTextMeasurer::default();
        let mut checkpoint = || Ok::<(), &'static str>(());
        let mut selector_limit = |actual, maximum| {
            assert_eq!(actual, super::cascade::MAX_UNIVERSAL_POSTINGS + 1);
            assert_eq!(maximum, super::cascade::MAX_UNIVERSAL_POSTINGS);
            Err("selector limit")
        };
        let mut preflight = |_, _, _| Ok::<(), &'static str>(());
        let result = super::foreign_object_label_fallback_svg_text_with_checkpoints(
            &svg,
            &measurer,
            &mut checkpoint,
            &mut selector_limit,
            &mut preflight,
            &mut |_| Ok::<(), &'static str>(()),
        );

        assert_eq!(result, Err("selector limit"));
    }

    #[test]
    fn foreign_object_overlay_bounds_ordinary_selector_postings() {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>"#);
        for index in 0..super::cascade::MAX_SELECTOR_POSTINGS {
            if index > 0 {
                svg.push(',');
            }
            svg.push_str(".posting");
            svg.push_str(&index.to_string());
        }
        svg.push_str(r#"{font-size:12px}.nodeLabel{font-size:19px}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel posting"#);
        svg.push_str(&(super::cascade::MAX_SELECTOR_POSTINGS - 1).to_string());
        svg.push_str(r#"">Alpha</span></div></foreignObject></svg>"#);

        let out = foreign_object_label_fallback_svg_text(&svg);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(
            fallback.contains("font-size: 12px") && !fallback.contains("font-size: 19px"),
            "the infallible helper must ignore ordinary postings beyond the private cap: {out}"
        );

        let measurer = DeterministicTextMeasurer::default();
        let mut checkpoint = || Ok::<(), &'static str>(());
        let mut selector_limit = |actual, maximum| {
            assert_eq!(actual, super::cascade::MAX_SELECTOR_POSTINGS + 1);
            assert_eq!(maximum, super::cascade::MAX_SELECTOR_POSTINGS);
            Err("selector limit")
        };
        let mut preflight = |_, _, _| Ok::<(), &'static str>(());
        let result = super::foreign_object_label_fallback_svg_text_with_checkpoints(
            &svg,
            &measurer,
            &mut checkpoint,
            &mut selector_limit,
            &mut preflight,
            &mut |_| Ok::<(), &'static str>(()),
        );

        assert_eq!(result, Err("selector limit"));
    }

    #[test]
    fn foreign_object_overlay_rejects_oversized_selector_stylesheet() {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>"#);
        svg.extend(std::iter::repeat_n(
            ' ',
            super::cascade::MAX_SELECTOR_STYLESHEET_BYTES + 1,
        ));
        svg.push_str(
            r#"</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span>Alpha</span></div></foreignObject></svg>"#,
        );

        let measurer = DeterministicTextMeasurer::default();
        let mut checkpoint = || Ok::<(), &'static str>(());
        let mut selector_limit = |actual, maximum| {
            assert_eq!(actual, super::cascade::MAX_SELECTOR_STYLESHEET_BYTES + 1);
            assert_eq!(maximum, super::cascade::MAX_SELECTOR_STYLESHEET_BYTES);
            Err("selector limit")
        };
        let mut preflight = |_, _, _| Ok::<(), &'static str>(());
        let result = super::foreign_object_label_fallback_svg_text_with_checkpoints(
            &svg,
            &measurer,
            &mut checkpoint,
            &mut selector_limit,
            &mut preflight,
            &mut |_| Ok::<(), &'static str>(()),
        );

        assert_eq!(result, Err("selector limit"));

        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
            )
            .begin_session()
            .unwrap();
        let error = crate::svg::pipeline::SvgPipeline::resvg_safe()
            .process_to_string(&svg, &session)
            .expect_err("the public pipeline must preserve the selector resource error");
        let crate::Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected a resource limit error");
        };
        assert_eq!(
            limit.phase,
            crate::resources::ResourceLimitPhase::SvgPostprocess
        );
        assert_eq!(limit.limit, "svg_fallback_selector_index");
        assert_eq!(limit.cause, crate::resources::ResourceLimitCause::Ceiling);
        assert_eq!(
            (limit.actual, limit.max),
            (
                super::cascade::MAX_SELECTOR_STYLESHEET_BYTES + 1,
                super::cascade::MAX_SELECTOR_STYLESHEET_BYTES,
            )
        );
    }

    #[test]
    fn foreign_object_overlay_discards_a_rule_over_the_declaration_cap() {
        let mut svg = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{font-size:12px;"#,
        );
        for _ in 1..super::cascade::MAX_SELECTOR_DECLARATIONS_PER_RULE {
            svg.push_str("font-weight:400;");
        }
        svg.push_str(
            r#"font-size:19px}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></svg>"#,
        );

        let out = foreign_object_label_fallback_svg_text(&svg);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(
            fallback.contains("font-size: 16px")
                && !fallback.contains("font-size: 12px")
                && !fallback.contains("font-size: 19px"),
            "an over-limit declaration block must be discarded atomically: {out}"
        );

        let measurer = DeterministicTextMeasurer::default();
        let mut checkpoint = || Ok::<(), &'static str>(());
        let mut selector_limit = |actual, maximum| {
            assert_eq!(
                actual,
                super::cascade::MAX_SELECTOR_DECLARATIONS_PER_RULE + 1
            );
            assert_eq!(maximum, super::cascade::MAX_SELECTOR_DECLARATIONS_PER_RULE);
            Err("selector limit")
        };
        let mut preflight = |_, _, _| Ok::<(), &'static str>(());
        let result = super::foreign_object_label_fallback_svg_text_with_checkpoints(
            &svg,
            &measurer,
            &mut checkpoint,
            &mut selector_limit,
            &mut preflight,
            &mut |_| Ok::<(), &'static str>(()),
        );
        assert_eq!(result, Err("selector limit"));
    }

    #[test]
    fn foreign_object_overlay_discards_a_selector_list_over_the_component_cap() {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel"#);
        for _ in 0..super::cascade::MAX_SELECTOR_COMPONENTS_PER_BRANCH {
            svg.push_str(".extra");
        }
        svg.push_str(
            r#",.nodeLabel{font-size:19px}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></svg>"#,
        );

        let out = foreign_object_label_fallback_svg_text(&svg);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(
            fallback.contains("font-size: 16px") && !fallback.contains("font-size: 19px"),
            "a selector-list resource overflow must not retain an admitted sibling: {out}"
        );

        let measurer = DeterministicTextMeasurer::default();
        let mut checkpoint = || Ok::<(), &'static str>(());
        let mut selector_limit = |actual, maximum| {
            assert_eq!(
                actual,
                super::cascade::MAX_SELECTOR_COMPONENTS_PER_BRANCH + 1
            );
            assert_eq!(maximum, super::cascade::MAX_SELECTOR_COMPONENTS_PER_BRANCH);
            Err("selector limit")
        };
        let mut preflight = |_, _, _| Ok::<(), &'static str>(());
        let result = super::foreign_object_label_fallback_svg_text_with_checkpoints(
            &svg,
            &measurer,
            &mut checkpoint,
            &mut selector_limit,
            &mut preflight,
            &mut |_| Ok::<(), &'static str>(()),
        );
        assert_eq!(result, Err("selector limit"));
    }

    #[test]
    fn foreign_object_overlay_accounts_for_source_selector_width_in_match_work() {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>"#);
        for index in 0..512 {
            if index > 0 {
                svg.push(',');
            }
            svg.push_str(".hit.missing");
            svg.push_str(&index.to_string());
        }
        svg.push_str(
            r#"{font-size:12px}.hit{font-size:19px}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="hit"#,
        );
        for index in 0..3000 {
            svg.push_str(" filler");
            svg.push_str(&index.to_string());
        }
        svg.push_str(r#"">Alpha</span></div></foreignObject></svg>"#);

        let out = foreign_object_label_fallback_svg_text(&svg);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output"));
        assert!(
            fallback.contains("font-size: 16px")
                && !fallback.contains("font-size: 12px")
                && !fallback.contains("font-size: 19px"),
            "an exhausted match budget must skip all stylesheet candidates for the element"
        );

        let measurer = DeterministicTextMeasurer::default();
        let mut checkpoint = || Ok::<(), &'static str>(());
        let mut selector_limit = |actual, maximum| {
            assert!(actual > maximum);
            assert_eq!(maximum, super::cascade::MAX_SELECTOR_MATCH_WORK);
            Err("selector limit")
        };
        let mut preflight = |_, _, _| Ok::<(), &'static str>(());
        let result = super::foreign_object_label_fallback_svg_text_with_checkpoints(
            &svg,
            &measurer,
            &mut checkpoint,
            &mut selector_limit,
            &mut preflight,
            &mut |_| Ok::<(), &'static str>(()),
        );
        assert_eq!(result, Err("selector limit"));
    }

    #[test]
    fn foreign_object_overlay_bounds_source_ancestry_before_building_paths() {
        let mut svg = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><style>a{font-size:12px}</style>"#,
        );
        for _ in 0..=super::cascade::MAX_SELECTOR_ANCESTRY_DEPTH {
            svg.push_str("<a>");
        }
        svg.push_str(
            r#"<foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml">Alpha</div></foreignObject>"#,
        );
        for _ in 0..=super::cascade::MAX_SELECTOR_ANCESTRY_DEPTH {
            svg.push_str("</a>");
        }
        svg.push_str("</svg>");

        let out = foreign_object_label_fallback_svg_text(&svg);
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap_or_else(|| panic!("expected fallback output: {out}"));
        assert!(
            fallback.contains("font-size: 16px") && !fallback.contains("font-size: 12px"),
            "an over-deep source path must use the bounded fallback defaults: {out}"
        );

        let measurer = DeterministicTextMeasurer::default();
        let mut checkpoint = || Ok::<(), &'static str>(());
        let mut selector_limit = |actual, maximum| {
            assert_eq!(actual, super::cascade::MAX_SELECTOR_ANCESTRY_DEPTH + 1);
            assert_eq!(maximum, super::cascade::MAX_SELECTOR_ANCESTRY_DEPTH);
            Err("selector limit")
        };
        let mut preflight = |_, _, _| Ok::<(), &'static str>(());
        let result = super::foreign_object_label_fallback_svg_text_with_checkpoints(
            &svg,
            &measurer,
            &mut checkpoint,
            &mut selector_limit,
            &mut preflight,
            &mut |_| Ok::<(), &'static str>(()),
        );
        assert_eq!(result, Err("selector limit"));
    }

    #[test]
    fn foreign_object_overlay_resolves_nested_label_background_descendants() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.labelBkg .nested{background-color:#c0ffee}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="nested">Alpha</span></div></foreignObject></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains(r##"fill="#c0ffee""##),
            "a nested XHTML background declaration must drive the fallback rect: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_resolves_classless_common_background_owner() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.edgeLabel{background-color:#c0ffee}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="edgeLabel">Alpha</span></div></foreignObject></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            has_background_rect(&out, "#c0ffee", "80", "30"),
            "a classless common XHTML background owner must drive the fallback rect: {out}"
        );

        let shorthand = r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.edgeLabel{background:#ececff}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="edgeLabel">Alpha</span></div></foreignObject></svg>"##;
        let out = foreign_object_label_fallback_svg_text(shorthand);
        assert!(
            has_background_rect(&out, "#ececff", "80", "30"),
            "a color-only background shorthand must resolve on the real XHTML owner: {out}"
        );

        let current_color = r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.edgeLabel{color:#123456;background-color:currentColor}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="edgeLabel">Alpha</span></div></foreignObject></svg>"##;
        let out = foreign_object_label_fallback_svg_text(current_color);
        assert!(
            has_background_rect(&out, "#123456", "80", "30")
                && !out.contains(r#"fill="currentcolor""#),
            "background currentColor must be materialized before leaving XHTML context: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_does_not_expand_partial_run_background() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.highlight{background-color:#c0ffee}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="highlight">Alpha</span><span>Beta</span></div></foreignObject></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            !has_background_rect(&out, "#c0ffee", "80", "30"),
            "a background that covers only one rich-text run must not expand to the complete fallback label: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_classifies_common_xhtml_type_selectors() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>a{font-size:13px}code{font-weight:600}label{font-style:italic}</style><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><label><code><a>Alpha</a></code></label></div></foreignObject></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(out.contains("font-size: 13px"), "got: {out}");
        assert!(out.contains("font-weight: 600"), "got: {out}");
        assert!(out.contains("font-style: italic"), "got: {out}");
    }

    #[test]
    fn foreign_object_overlay_keeps_unprefixed_type_selectors_namespace_neutral() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>a{font-size:13px}</style><a><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span>Alpha</span></div></foreignObject></a></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains("font-size: 13px"),
            "an unprefixed type selector must match the real SVG ancestor namespace: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_resolves_specified_values_before_inheritance_and_source_order() {
        let inherited_important = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>#root{font-size:20px !important}.nodeLabel{font-size:14px}</style><g id="root"><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(inherited_important);
        assert!(
            out.contains("font-size: 14px"),
            "a child's specified normal value must win before inheriting a parent's important value: {out}"
        );

        let presentation_vs_stylesheet = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.host{font-size:12px}</style><g class="host" font-size="20px"><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span>Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(presentation_vs_stylesheet);
        assert!(
            out.contains("font-size: 12px"),
            "stylesheet declarations must follow presentation attributes in author source order: {out}"
        );

        let source_order = r#"
<svg xmlns="http://www.w3.org/2000/svg"><style>.nodeLabel{font-size:12px}.nodeLabel{font-size:14px}</style><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(source_order);
        assert!(
            out.contains("font-size: 14px"),
            "later equal-specificity declarations must win by stylesheet source order: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_root_rem_and_measurement_matches_emission() {
        let svg = r#"
<svg style="font-size:20px" xmlns="http://www.w3.org/2000/svg"><g style="font-size:10px"><foreignObject width="80" height="60"><div xmlns="http://www.w3.org/1999/xhtml" style="white-space:normal;width:80px;line-height:1.5"><span style="font-size:2rem;font-family:Inter;font-weight:600;font-style:italic">Alpha</span></div></foreignObject></g></svg>"#;
        let measurer = RecordingMeasurer::default();
        let out = render_fallback(svg, &measurer);
        let styles = measurer.styles.borrow();

        assert!(
            styles.iter().any(|style| {
                style.font_size == 40.0
                    && style.font_family.as_deref() == Some("Inter")
                    && style.font_weight.as_deref() == Some("600")
                    && style.font_style.as_deref() == Some("italic")
            }),
            "measurement must receive the resolved typography: {styles:?}"
        );
        assert!(out.contains("font-size: 40px"), "got: {out}");
        assert!(out.contains("font-family: Inter"), "got: {out}");
        assert!(out.contains("font-weight: 600"), "got: {out}");
        assert!(out.contains("font-style: italic"), "got: {out}");
        assert!(out.contains("line-height: 60px"), "got: {out}");
    }

    #[test]
    fn foreign_object_overlay_preserves_inherited_line_height_forms() {
        let multiplier = r#"
<svg xmlns="http://www.w3.org/2000/svg"><g><foreignObject width="80" height="48"><div xmlns="http://www.w3.org/1999/xhtml" style="line-height:1.5"><span style="font-size:20px">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(multiplier);
        assert!(
            out.contains("font-size: 20px") && out.contains("line-height: 30px"),
            "a unitless inherited line-height must scale at the child font size: {out}"
        );

        let absolute = r#"
<svg xmlns="http://www.w3.org/2000/svg"><g><foreignObject width="80" height="48"><div xmlns="http://www.w3.org/1999/xhtml" style="line-height:24px"><span style="font-size:20px">Alpha</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(absolute);
        assert!(
            out.contains("font-size: 20px") && out.contains("line-height: 24px"),
            "an absolute inherited line-height must remain absolute: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_common_ancestor_for_mixed_text_leaves() {
        let svg = r#"
<svg xmlns="http://www.w3.org/2000/svg"><g><foreignObject width="80" height="30"><div xmlns="http://www.w3.org/1999/xhtml" style="font-size:14px"><span>Alpha</span><strong style="font-size:20px">Beta</strong></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains("font-size: 14px"),
            "mixed text leaves should use their deepest common ancestor's fallback typography: {out}"
        );
        assert!(
            !out.contains("font-size: 20px"),
            "the fallback must not arbitrarily select one rich-text leaf: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_reads_unicode_root_style_identity() {
        let svg = r##"<svg id="图表-α" xmlns="http://www.w3.org/2000/svg"><style>#图表-α{font-family:Inter;font-size:14px;fill:#ddeeff;}</style><g><foreignObject width="80" height="21"><div xmlns="http://www.w3.org/1999/xhtml"><p>Alpha</p></div></foreignObject></g></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(out.contains(r##"fill="#ddeeff""##), "got: {out}");
        assert!(out.contains("font-family: Inter"), "got: {out}");
    }

    #[test]
    fn foreign_object_overlay_does_not_put_structural_label_class_on_text() {
        let svg = r##"<svg id="host-theme-edge-label" xmlns="http://www.w3.org/2000/svg"><style>#host-theme-edge-label .edgeLabel .label{fill:#665c54;font-size:14px;}#host-theme-edge-label .edgeLabel .label text{fill:#ebdbb2;}</style><g class="edgeLabel"><g class="label"><foreignObject width="80" height="21"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="edgeLabel">places</span></div></foreignObject></g></g></svg>"##;
        let out = foreign_object_label_fallback_svg_text(svg);
        let text_tag_start = out
            .find(r#"<text "#)
            .unwrap_or_else(|| panic!("expected fallback text: {out}"));
        let text_tag_end = out[text_tag_start..]
            .find('>')
            .map(|offset| text_tag_start + offset)
            .unwrap_or_else(|| panic!("expected fallback text tag end: {out}"));
        let text_tag = &out[text_tag_start..=text_tag_end];

        assert!(text_tag.contains(r##"fill="#665c54""##), "got: {out}");
        assert!(
            !text_tag.contains(r##"fill="#ebdbb2""##),
            "the SVG-only `text` branch must not match the XHTML source leaf: {out}"
        );
        assert!(
            text_tag.contains(r#"class="merman-foreignobject-fallback-text""#)
                && text_tag.contains(r#"data-merman-source-classes="edgeLabel label labelBkg""#),
            "fallback text should retain source classes without making them live selectors: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_decodes_double_escaped_html_entities_for_fallback_text() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g><foreignObject width="1000" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><p>List&amp;lt;Animal&amp;gt; &amp;amp; friends &amp;apos;x&amp;apos; &amp;quot;y&amp;quot; &amp;#39;z&amp;#39; &amp;#x2F;</p></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains(">List&lt;Animal&gt; &amp; friends 'x' \"y\" 'z' /<"),
            "expected fallback text to avoid double-escaped entities: {out}"
        );
        let fallback = &out[out
            .find(r#"data-merman-foreignobject="fallback""#)
            .expect("fallback group")..];
        assert!(!fallback.contains("&amp;lt;"), "got: {fallback}");
        assert!(!fallback.contains("&amp;gt;"), "got: {fallback}");
        assert!(!fallback.contains("&amp;amp;"), "got: {fallback}");
        assert!(!fallback.contains("&amp;apos;"), "got: {fallback}");
        assert!(!fallback.contains("&amp;quot;"), "got: {fallback}");
        assert!(!fallback.contains("&amp;#"), "got: {fallback}");
    }

    #[test]
    fn foreign_object_overlay_wraps_break_spaces_labels_to_foreign_object_width() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(20, 30)"><foreignObject width="200" height="48"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table; white-space: break-spaces; line-height: 1.5; max-width: 200px; text-align: center; width: 200px;"><span class="nodeLabel">Import / WebSurface / Data Egress Gates</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            !out.contains(">Import / WebSurface / Data Egress Gates</text>"),
            "fallback text should inherit Mermaid HTML soft wrapping instead of flattening into one SVG text line: {out}"
        );
        let fallback = out
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .expect("fallback group");
        assert!(
            fallback.contains("Import") && fallback.contains("Egress Gates"),
            "fallback text should preserve all visible content: {out}"
        );
        assert_eq!(
            fallback
                .matches("merman-foreignobject-fallback-text")
                .count(),
            2,
            "the deterministic fallback should fit this label in two readable rows: {out}"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_layout_breaks_for_unicode_and_long_paths() {
        use crate::text::{TextStyle, WrapMode};

        let measurer = DeterministicTextMeasurer::default().with_width_callback(|text, _| {
            text.chars().filter(|ch| *ch != '\u{200b}').count() as f64 * 10.0
        });
        for (label, limit, expected_rows) in [
            ("alpha-beta", 60.0, vec!["alpha-", "beta"]),
            ("alpha\u{200b}beta", 50.0, vec!["alpha\u{200b}", "beta"]),
            ("负责人审批", 20.0, vec!["负责", "人审", "批"]),
            ("a  b", 20.0, vec!["a ", " b"]),
            ("unbreakable", 20.0, vec!["unbreakable"]),
            ("alpha\u{a0}beta", 50.0, vec!["alpha\u{a0}beta"]),
            ("alpha\u{2011}beta", 50.0, vec!["alpha\u{2011}beta"]),
            ("https://x.test/a/b", 50.0, vec!["https://x.test/a/b"]),
            (
                "docs/plans/2026-10-11-release-checklist.final.md",
                200.0,
                vec!["docs/plans/2026-10-11-", "release-", "checklist.final.md"],
            ),
            (
                "src/components/navigation/sidebar-tree.tsx",
                200.0,
                vec!["src/components/navigation/sidebar-", "tree.tsx"],
            ),
        ] {
            let metrics = measurer.measure_wrapped(
                label,
                &TextStyle::default(),
                Some(limit),
                WrapMode::HtmlLike,
            );
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" style="font-size:16px"><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="white-space:break-spaces;max-width:{limit}px;width:{limit}px"><span>{label}</span></div></foreignObject></svg>"#,
                metrics.width, metrics.height,
            );
            let output = render_fallback(&svg, measurer.as_ref());
            let document = roxmltree::Document::parse(&output).expect("valid fallback SVG");
            let rows: Vec<_> = document
                .descendants()
                .filter(|node| {
                    node.has_tag_name("text")
                        && node.attribute("class") == Some("merman-foreignobject-fallback-text")
                })
                .map(|node| node.text().unwrap_or(""))
                .collect();
            assert_eq!(rows, expected_rows, "label={label:?}");
            assert_eq!(rows.concat(), label, "visible content must be preserved");
            assert_eq!(rows.len(), metrics.line_count, "label={label:?}");
            assert!(
                document
                    .descendants()
                    .filter(|node| node.has_tag_name("text"))
                    .all(
                        |node| node.attribute(("http://www.w3.org/XML/1998/namespace", "space"))
                            == Some("preserve")
                    )
            );
            assert!(rows.iter().all(|row| {
                measurer
                    .measure_wrapped(row, &TextStyle::default(), None, WrapMode::HtmlLike)
                    .width
                    <= metrics.width
            }));
        }
    }

    #[test]
    fn foreign_object_overlay_collapses_spaces_for_normal_and_pre_line_wrapping() {
        let measurer = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.chars().count() as f64 * 10.0);
        for white_space in ["normal", "pre-line"] {
            for (label, limit, expected_rows) in [
                ("a    b", 20.0, vec!["a", "b"]),
                ("a\t\tb", 20.0, vec!["a", "b"]),
                ("a    b", 30.0, vec!["a b"]),
                ("a\u{a0}b", 20.0, vec!["a\u{a0}b"]),
                ("alpha-beta", 60.0, vec!["alpha-", "beta"]),
            ] {
                let svg = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" style="font-size:16px"><foreignObject width="{limit}" height="48"><div xmlns="http://www.w3.org/1999/xhtml" style="white-space:{white_space};width:{limit}px">{label}</div></foreignObject></svg>"#,
                );
                let output = render_fallback(&svg, measurer.as_ref());
                let document = roxmltree::Document::parse(&output).expect("valid fallback SVG");
                let rows: Vec<_> = document
                    .descendants()
                    .filter(|node| node.has_tag_name("text"))
                    .map(|node| node.text().unwrap_or(""))
                    .collect();
                assert_eq!(
                    rows, expected_rows,
                    "white-space={white_space}, label={label:?}"
                );
            }
        }
    }

    #[test]
    fn foreign_object_overlay_hangs_preserved_trailing_spaces_for_pre_wrap() {
        let measurer = DeterministicTextMeasurer::default().with_width_callback(|text, _| {
            // Model a kerning pair so hanging width cannot be obtained by adding space widths.
            text.chars().count() as f64 * 10.0 - if text.contains("a ") { 8.0 } else { 0.0 }
        });
        for label in ["a    b", "aaaa    b"] {
            for (anchor, expected_x) in [
                ("start", [0.0, 0.0]),
                ("middle", [26.0, 10.0]),
                ("end", [52.0, 20.0]),
            ] {
                let svg = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" style="font-size:16px"><foreignObject width="20" height="48" text-anchor="{anchor}"><div xmlns="http://www.w3.org/1999/xhtml" style="white-space:pre-wrap;width:20px">{label}</div></foreignObject></svg>"#
                );
                let output = render_fallback(&svg, measurer.as_ref());
                let document = roxmltree::Document::parse(&output).expect("valid fallback SVG");
                let rows: Vec<_> = document
                    .descendants()
                    .filter(|node| node.has_tag_name("text"))
                    .collect();
                assert_eq!(
                    rows.iter()
                        .map(|node| node.text().unwrap_or(""))
                        .collect::<Vec<_>>(),
                    [label.trim_end_matches('b'), "b"]
                );
                for (row, x) in rows.iter().zip(expected_x) {
                    assert_eq!(row.attribute("x").unwrap().parse::<f64>().unwrap(), x);
                }
            }
        }
    }

    #[test]
    fn foreign_object_overlay_uses_each_text_run_whitespace_mode() {
        let measurer = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.chars().count() as f64 * 10.0);
        for (label, expected) in [
            (r#"<span style="white-space:normal">a    b</span>"#, "a b"),
            (
                r#"<span style="white-space:normal">a </span><span style="white-space:normal">   b</span>"#,
                "a b",
            ),
            (
                r#"<span style="white-space:normal">a    </span><span style="white-space:break-spaces">  b </span>"#,
                "a   b ",
            ),
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="100" height="24"><div style="white-space:break-spaces;width:100px">{label}</div></foreignObject></svg>"#
            );
            let output = render_fallback(&svg, measurer.as_ref());
            let document = roxmltree::Document::parse(&output).unwrap();
            let row = document
                .descendants()
                .find(|node| node.has_tag_name("text"))
                .unwrap();
            assert_eq!(row.text().unwrap_or(""), expected, "label={label}");
        }
    }

    #[test]
    fn foreign_object_overlay_preserves_boundary_spaces_and_empty_rows() {
        for mode in ["pre", "pre-wrap", "break-spaces"] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="200" height="72"><div style="white-space:{mode};width:200px">  a  <br/><br/> b </div></foreignObject></svg>"#
            );
            let output = foreign_object_label_fallback_svg_text(&svg);
            let document = roxmltree::Document::parse(&output).unwrap();
            let rows: Vec<_> = document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .map(|node| node.text().unwrap_or(""))
                .collect();
            assert_eq!(rows, ["  a  ", "", " b "], "mode={mode}");
        }
    }

    #[test]
    fn foreign_object_overlay_hangs_unicode_normal_separators() {
        let measurer = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.chars().count() as f64 * 10.0);
        for (anchor, x) in [("start", 0.0), ("middle", 15.0), ("end", 30.0)] {
            let label = "a\u{2002} b";
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="20" height="48" text-anchor="{anchor}"><div style="white-space:normal;width:20px">{label}</div></foreignObject></svg>"#
            );
            let output = render_fallback(&svg, measurer.as_ref());
            let document = roxmltree::Document::parse(&output).unwrap();
            let rows: Vec<_> = document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .collect();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].text(), Some("a\u{2002}"));
            assert_eq!(rows[0].attribute("x").unwrap().parse::<f64>().unwrap(), x);
        }
    }

    #[test]
    fn foreign_object_overlay_materializes_selected_soft_hyphens() {
        use crate::text::{TextStyle, WrapMode};
        let measurer = DeterministicTextMeasurer::default().with_width_callback(|text, _| {
            text.chars().filter(|ch| *ch != '\u{00ad}').count() as f64 * 10.0
        });
        for label in ["alpha\u{00ad}beta", "alpha&amp;shy;beta"] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="60" height="48"><div style="white-space:break-spaces;width:60px">{label}</div></foreignObject></svg>"#
            );
            let output = render_fallback(&svg, measurer.as_ref());
            let document = roxmltree::Document::parse(&output).unwrap();
            let rows: Vec<_> = document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .map(|node| node.text().unwrap_or(""))
                .collect();
            assert_eq!(rows, ["alpha-", "beta"]);
        }
        let metrics = measurer.measure_wrapped(
            "alpha\u{00ad}beta",
            &TextStyle::default(),
            Some(50.0),
            WrapMode::HtmlLike,
        );
        assert_eq!(metrics.line_count, 2);
        assert_eq!(
            metrics.width, 60.0,
            "min-content must include the visible hyphen"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_incremental_builtin_html_width() {
        use crate::environment::BuiltinHtmlLineWidth;
        use crate::text::{TextMeasurer, TextMetrics, TextStyle, WrapMode};
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct CountedBuiltin(AtomicUsize);
        impl TextMeasurer for CountedBuiltin {
            fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
                self.measure_wrapped(text, style, None, WrapMode::HtmlLike)
            }
            fn measure_wrapped(
                &self,
                text: &str,
                style: &TextStyle,
                width: Option<f64>,
                mode: WrapMode,
            ) -> TextMetrics {
                self.0.fetch_add(1, Ordering::Relaxed);
                DeterministicTextMeasurer::default().measure_wrapped(text, style, width, mode)
            }
            fn begin_html_line_width(&self, style: &TextStyle) -> Option<BuiltinHtmlLineWidth> {
                Some(BuiltinHtmlLineWidth::deterministic(style, 0.0))
            }
        }
        let measurer = CountedBuiltin(AtomicUsize::new(0));
        let label = "中".repeat(1000);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="8000" height="48"><div style="white-space:break-spaces;width:8000px">{label}</div></foreignObject></svg>"#
        );
        let output = render_fallback(&svg, &measurer);
        let document = roxmltree::Document::parse(&output).unwrap();
        assert_eq!(
            document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .count(),
            2
        );
        assert!(
            measurer.0.load(Ordering::Relaxed) < 16,
            "complete measurements must scale with rows, not Unicode segments"
        );
    }

    #[test]
    fn foreign_object_overlay_uses_common_ancestor_whitespace_at_inline_boundaries() {
        let measurer = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.chars().count() as f64 * 10.0);
        for (parent, label, expected) in [
            (
                "normal",
                r#"<span style="white-space:nowrap">中文</span><span style="white-space:normal">中</span>"#,
                vec!["中文", "中"],
            ),
            (
                "nowrap",
                r#"<span style="white-space:normal">中</span><span style="white-space:normal">文</span>"#,
                vec!["中文"],
            ),
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="10" height="48"><div style="white-space:{parent};width:10px">{label}</div></foreignObject></svg>"#
            );
            let output = render_fallback(&svg, measurer.as_ref());
            let document = roxmltree::Document::parse(&output).unwrap();
            let rows: Vec<_> = document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .map(|node| node.text().unwrap_or(""))
                .collect();
            assert_eq!(rows, expected);
        }
    }

    #[test]
    fn foreign_object_overlay_normalizes_source_cr_before_decoding_entities() {
        for (label, expected_rows) in [
            ("a\rb", 2),
            ("a\r\nb", 2),
            ("a&#13;b", 1),
            ("a&amp;#13;b", 1),
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="200" height="48"><div style="white-space:pre">{label}</div></foreignObject></svg>"#
            );
            let output = foreign_object_label_fallback_svg_text(&svg);
            let overlay = output
                .split(r#"data-merman-foreignobject="fallback""#)
                .nth(1)
                .unwrap();
            assert_eq!(
                overlay
                    .matches("merman-foreignobject-fallback-text")
                    .count(),
                expected_rows,
                "label={label:?}"
            );
        }
    }

    #[test]
    fn foreign_object_overlay_measures_decoded_markup_as_literal_text() {
        let measurer = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.chars().count() as f64 * 10.0);
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="100" height="48"><div style="white-space:pre-wrap;width:100px">a&amp;lt;br&amp;gt;  b</div></foreignObject></svg>"#;
        let output = render_fallback(svg, measurer.as_ref());
        let document = roxmltree::Document::parse(&output).unwrap();
        let rows: Vec<_> = document
            .descendants()
            .filter(|node| node.has_tag_name("text"))
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text(), Some("a<br>  b"));
        assert_eq!(rows[0].attribute("x"), Some("50"));
    }

    #[test]
    fn foreign_object_overlay_distinguishes_space_hyphen_and_character_boundaries() {
        let measurer = DeterministicTextMeasurer::default().with_width_callback(|text, _| {
            text.chars().filter(|ch| *ch != '\u{00ad}').count() as f64 * 10.0
        });
        for (parent, label, width, expected) in [
            (
                "normal",
                r#"<span style="white-space:nowrap">a </span><span>b</span>"#,
                10.0,
                vec!["a b"],
            ),
            (
                "nowrap",
                r#"<span style="white-space:normal">a </span><span>b</span>"#,
                10.0,
                vec!["a", "b"],
            ),
            (
                "normal",
                "<span style=\"white-space:nowrap\">alpha\u{00ad}</span><span>beta</span>",
                60.0,
                vec!["alpha\u{00ad}", "beta"],
            ),
            (
                "nowrap",
                "<span style=\"white-space:normal\">alpha\u{00ad}</span><span>beta</span>",
                60.0,
                vec!["alpha-", "beta"],
            ),
            (
                "normal",
                r#"<span style="white-space:nowrap">a <i> </i><i> </i></span><span>b</span>"#,
                10.0,
                vec!["a b"],
            ),
            (
                "normal",
                r#"<span style="white-space:nowrap">a </span><span style="white-space:normal"> </span><span>b</span>"#,
                10.0,
                vec!["a", "b"],
            ),
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="{width}" height="48"><div style="white-space:{parent};width:{width}px">{label}</div></foreignObject></svg>"#
            );
            let output = render_fallback(&svg, measurer.as_ref());
            let document = roxmltree::Document::parse(&output).unwrap();
            let rows: Vec<_> = document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .map(|node| node.text().unwrap_or(""))
                .collect();
            assert_eq!(rows, expected, "parent={parent}, label={label}");
        }
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="100" height="48"><div>a</br>b</div></foreignObject></svg>"#;
        let output = render_fallback(svg, measurer.as_ref());
        let overlay = output
            .split(r#"data-merman-foreignobject="fallback""#)
            .nth(1)
            .unwrap();
        assert_eq!(
            overlay
                .matches("merman-foreignobject-fallback-text")
                .count(),
            2
        );
    }

    #[test]
    fn foreign_object_overlay_keeps_nowrap_labels_as_single_fallback_line() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g><foreignObject width="200" height="24"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;"><span class="nodeLabel">Import / WebSurface / Data Egress Gates</span></div></foreignObject></g></svg>"#;
        let out = foreign_object_label_fallback_svg_text(svg);

        assert!(
            out.contains(">Import / WebSurface / Data Egress Gates</text>"),
            "explicit nowrap labels should keep the existing single-line fallback behavior: {out}"
        );
    }
}
