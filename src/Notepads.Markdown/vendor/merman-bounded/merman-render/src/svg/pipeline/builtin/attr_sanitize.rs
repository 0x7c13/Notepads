use crate::Result;
use cssparser::{Delimiter, Parser};
use merman_core::svg_security::{MermaidSvgUriRepresentation, admit_mermaid_svg_uri_attribute};
use std::borrow::Cow;
use std::convert::Infallible;

use super::css_sanitize::sanitize_css_value_with_checkpoints;
use super::presentation_fallback::is_mermaid_missing_amount_hsl;
use super::util::{
    SvgTagScanner, checkpoint_loop, escape_xml_attr, find_with_checkpoints, next_svg_quoted_attr,
    next_svg_quoted_attr_with_checkpoints, start_tag_name,
};
use crate::svg::pipeline::{SvgPostprocessContext, SvgPostprocessor};

#[derive(Debug, Clone, Copy, Default)]
pub struct SanitizeSvgAttributesPostprocessor;

impl SvgPostprocessor for SanitizeSvgAttributesPostprocessor {
    fn name(&self) -> &'static str {
        "sanitize-svg-attributes"
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        ctx: &SvgPostprocessContext<'_>,
    ) -> Result<Cow<'a, str>> {
        sanitize_element_attributes_cow_with_checkpoints(svg, &mut || ctx.checkpoint())
    }
}

#[cfg(test)]
fn sanitize_element_attributes(svg: &str) -> String {
    infallible(sanitize_element_attributes_cow_with_checkpoints(
        Cow::Borrowed(svg),
        &mut || Ok::<(), Infallible>(()),
    ))
    .into_owned()
}

fn infallible<T>(result: std::result::Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(crate) fn sanitize_element_attributes_cow_with_checkpoints<'a, E>(
    svg: Cow<'a, str>,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<Cow<'a, str>, E> {
    let source = svg.as_ref();
    let mut out = None::<String>;
    let mut scanner = SvgTagScanner::new(source);
    let mut copied_until = 0;
    let mut tag_index = 0usize;

    while let Some(tag) = scanner.next_with_checkpoints(checkpoint)? {
        checkpoint_loop(tag_index, checkpoint)?;
        tag_index = tag_index.saturating_add(1);
        let raw_tag = tag.raw();
        if let Some(active_name) = active_svg_element_name(raw_tag) {
            let output = out.get_or_insert_with(|| String::with_capacity(source.len()));
            output.push_str(&source[copied_until..tag.start()]);
            copied_until = if tag.is_self_closing() {
                scanner.cursor()
            } else {
                find_close_tag_end(source, scanner.cursor(), active_name, checkpoint)?
                    .unwrap_or(scanner.cursor())
            };
            scanner.skip_to(copied_until);
            continue;
        }

        if is_bad_rect_tag(raw_tag) {
            let output = out.get_or_insert_with(|| String::with_capacity(source.len()));
            output.push_str(&source[copied_until..tag.start()]);
            copied_until = if tag.is_self_closing() {
                scanner.cursor()
            } else {
                find_with_checkpoints(&source[scanner.cursor()..], "</rect>", checkpoint)?
                    .map(|rel_close| scanner.cursor() + rel_close + "</rect>".len())
                    .unwrap_or(scanner.cursor())
            };
            scanner.skip_to(copied_until);
            continue;
        }

        match sanitize_tag_attributes(raw_tag, checkpoint)? {
            Cow::Borrowed(_) => {
                if let Some(output) = out.as_mut() {
                    output.push_str(&source[copied_until..scanner.cursor()]);
                    copied_until = scanner.cursor();
                }
            }
            Cow::Owned(sanitized) => {
                let output = out.get_or_insert_with(|| String::with_capacity(source.len()));
                output.push_str(&source[copied_until..tag.start()]);
                output.push_str(&sanitized);
                copied_until = scanner.cursor();
            }
        }
    }

    let Some(mut out) = out else {
        checkpoint()?;
        return Ok(svg);
    };
    out.push_str(&source[copied_until..]);
    checkpoint()?;
    Ok(Cow::Owned(out))
}

fn sanitize_tag_attributes<'a, E>(
    tag: &'a str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<Cow<'a, str>, E> {
    if tag.starts_with("</")
        || tag.starts_with("<!--")
        || tag.starts_with("<!")
        || tag.starts_with("<?")
    {
        return Ok(Cow::Borrowed(tag));
    }

    let element_name = start_tag_name(tag).map(local_name).unwrap_or_default();
    let mut changed = false;
    let mut out = String::new();
    let mut copied_until = 0usize;
    let mut cursor = 0usize;
    let mut attr_index = 0usize;

    while let Some(attr) = next_svg_quoted_attr_with_checkpoints(tag, cursor, checkpoint)? {
        checkpoint_loop(attr_index, checkpoint)?;
        attr_index = attr_index.saturating_add(1);
        let name = &tag[attr.name_start..attr.name_end];
        let value = &tag[attr.value_start..attr.value_end];

        let replacement = sanitized_attr_replacement(element_name, name, value, checkpoint)?;
        if let AttrReplacement::Unchanged = replacement {
            cursor = attr.full_end;
            continue;
        }

        if !changed {
            out = String::with_capacity(tag.len());
            changed = true;
        }
        out.push_str(&tag[copied_until..attr.full_start]);
        match replacement {
            AttrReplacement::Unchanged => {}
            AttrReplacement::Drop => {}
            AttrReplacement::Replace(replacement) => out.push_str(&replacement),
        }
        copied_until = attr.full_end;
        cursor = attr.full_end;
    }

    if changed {
        out.push_str(&tag[copied_until..]);
        Ok(Cow::Owned(out))
    } else {
        Ok(Cow::Borrowed(tag))
    }
}

enum AttrReplacement {
    Unchanged,
    Drop,
    Replace(String),
}

fn sanitized_attr_replacement<E>(
    element_name: &str,
    name: &str,
    value: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<AttrReplacement, E> {
    checkpoint()?;
    if is_namespace_declaration(name) {
        return Ok(AttrReplacement::Unchanged);
    }

    if is_anchor_navigation_attribute(element_name, name) {
        let Some(normalized_value) =
            admit_mermaid_svg_uri_attribute(value, MermaidSvgUriRepresentation::SerializedSvg)
        else {
            return Ok(AttrReplacement::Drop);
        };
        let normalized_value = escape_xml_attr(&normalized_value);
        if normalized_value != value {
            return Ok(AttrReplacement::Replace(format!(
                r#" {name}="{normalized_value}""#
            )));
        }
        return Ok(AttrReplacement::Unchanged);
    }

    if should_drop_attribute_with_checkpoints(element_name, name, value, checkpoint)? {
        return Ok(AttrReplacement::Drop);
    }

    if let Some(value) = normalize_px_attribute(name, value) {
        return Ok(AttrReplacement::Replace(format!(r#" {name}="{value}""#)));
    }

    if local_name(name).eq_ignore_ascii_case("style") {
        let sanitized = sanitize_style_attribute(value, checkpoint)?;
        if sanitized.trim().is_empty() {
            return Ok(AttrReplacement::Drop);
        }
        if sanitized != value {
            return Ok(AttrReplacement::Replace(format!(
                r#" {name}="{sanitized}""#
            )));
        }
    }

    checkpoint()?;
    Ok(AttrReplacement::Unchanged)
}

fn should_drop_attribute(element_name: &str, name: &str, value: &str) -> bool {
    infallible(should_drop_attribute_with_checkpoints(
        element_name,
        name,
        value,
        &mut || Ok::<(), Infallible>(()),
    ))
}

fn should_drop_attribute_with_checkpoints<E>(
    element_name: &str,
    name: &str,
    value: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<bool, E> {
    if is_namespace_declaration(name) {
        return Ok(false);
    }

    let semantic_name = local_name(name.trim());
    if semantic_name.eq_ignore_ascii_case("style") {
        return Ok(false);
    }

    if is_event_handler_attribute(name)
        || is_unsafe_url_attribute(element_name, name, value)
        || is_base_url_attribute(name)
    {
        return Ok(true);
    }

    let normalized = semantic_name.to_ascii_lowercase();
    if is_url_function_attribute(&normalized) && css_value_violates_url_safety(value, checkpoint)? {
        return Ok(true);
    }

    if value.trim().is_empty() {
        return Ok(matches!(
            normalized.as_str(),
            "fill"
                | "stroke"
                | "width"
                | "height"
                | "x"
                | "y"
                | "x1"
                | "x2"
                | "y1"
                | "y2"
                | "r"
                | "cx"
                | "cy"
                | "rx"
                | "ry"
                | "stroke-width"
                | "transform"
                | "d"
                | "points"
        ));
    }

    let drop = match normalized.as_str() {
        "fill" | "stroke" => is_mermaid_missing_amount_hsl(value),
        "width" | "height" | "x" | "y" | "x1" | "x2" | "y1" | "y2" | "r" | "cx" | "cy" | "rx"
        | "ry" | "stroke-width" => is_provably_invalid_scalar(value),
        "transform" => is_invalid_svg_transform(value),
        "d" | "points" => contains_non_finite_numeric_token(value),
        _ => false,
    };
    checkpoint()?;
    Ok(drop)
}

pub(in crate::svg::pipeline) fn parsed_attribute_violates_resvg_contract(
    element_name: &str,
    name: &str,
    value: &str,
) -> bool {
    if is_anchor_navigation_attribute(element_name, name) {
        return admit_mermaid_svg_uri_attribute(value, MermaidSvgUriRepresentation::DomValue)
            .is_none_or(|normalized| normalized != value);
    }

    // The scanner contract consumes serialized source. Re-serialize this parsed XML value before
    // reusing it so character references are interpreted exactly once, not once per validation
    // stage.
    should_drop_attribute(element_name, name, &escape_xml_attr(value))
}

fn is_event_handler_attribute(name: &str) -> bool {
    let name = local_name(name.trim());
    name.len() > 2
        && name
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
        && name.as_bytes()[2].is_ascii_alphabetic()
}

fn is_base_url_attribute(name: &str) -> bool {
    let Some((prefix, local)) = name.trim().rsplit_once(':') else {
        return false;
    };
    !prefix.eq_ignore_ascii_case("xmlns") && local.eq_ignore_ascii_case("base")
}

fn is_unsafe_url_attribute(element_name: &str, name: &str, value: &str) -> bool {
    let attribute_name = local_name(name.trim()).to_ascii_lowercase();
    match attribute_name.as_str() {
        "href" => match local_name(element_name).to_ascii_lowercase().as_str() {
            "a" => is_unsafe_navigation_url_value(value),
            "image" => !is_safe_data_image_url(value),
            "feimage" => is_unsafe_render_resource_url_value(value),
            _ => !is_same_document_fragment(value),
        },
        "src" => !is_safe_data_image_url(value),
        "srcset" => true,
        _ => false,
    }
}

fn is_anchor_navigation_attribute(element_name: &str, name: &str) -> bool {
    local_name(element_name).eq_ignore_ascii_case("a")
        && local_name(name.trim()).eq_ignore_ascii_case("href")
}

fn is_url_function_attribute(name: &str) -> bool {
    matches!(
        name,
        "fill"
            | "stroke"
            | "filter"
            | "clip-path"
            | "mask"
            | "marker-start"
            | "marker-mid"
            | "marker-end"
            | "cursor"
            | "background"
            | "background-image"
    )
}

fn is_unsafe_navigation_url_value(value: &str) -> bool {
    admit_mermaid_svg_uri_attribute(value, MermaidSvgUriRepresentation::SerializedSvg).is_none()
}

pub(super) fn is_unsafe_render_resource_url_value(value: &str) -> bool {
    if is_same_document_fragment(value) {
        return false;
    }

    !is_safe_data_image_url(value)
}

fn is_same_document_fragment(value: &str) -> bool {
    let value = normalize_url_attr_for_scheme_check(value);
    is_same_document_fragment_normalized(&value)
}

fn is_same_document_fragment_normalized(value: &str) -> bool {
    value
        .strip_prefix('#')
        .is_some_and(|fragment| !fragment.is_empty())
}

fn css_value_violates_url_safety<E>(
    value: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<bool, E> {
    checkpoint()?;
    let decoded = merman_core::entities::decode_html_entities_to_unicode(value);
    checkpoint()?;
    Ok(sanitize_css_value_with_checkpoints(decoded.as_ref(), checkpoint)?.is_none())
}

fn normalize_url_attr_for_scheme_check(value: &str) -> String {
    let decoded = merman_core::entities::decode_html_entities_to_unicode(value);
    let mut out = String::with_capacity(decoded.len());
    for ch in decoded.trim().chars() {
        if !ch.is_whitespace() && !ch.is_control() {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

pub(in crate::svg::pipeline) fn is_safe_data_image_url(value: &str) -> bool {
    let decoded = merman_core::entities::decode_html_entities_to_unicode(value);
    let Ok(url) = data_url::DataUrl::process(decoded.as_ref()) else {
        return false;
    };
    let mime = url.mime_type();
    let approved_mime = mime.matches("image", "png")
        || mime.matches("image", "jpeg")
        || mime.matches("image", "jpg")
        || mime.matches("image", "gif")
        || mime.matches("image", "webp");
    approved_mime
        && has_valid_percent_encoding(decoded.as_ref())
        && url
            .decode(|_| Ok::<(), std::convert::Infallible>(()))
            .is_ok()
}

fn has_valid_percent_encoding(data_url: &str) -> bool {
    let Some((_, encoded_body)) = data_url.split_once(',') else {
        return false;
    };
    let encoded_body = encoded_body
        .split_once('#')
        .map_or(encoded_body, |(body, _)| body);
    let mut bytes = encoded_body.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%'
            && (!bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit())
                || !bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit()))
        {
            return false;
        }
    }
    true
}

fn normalize_px_attribute(name: &str, value: &str) -> Option<String> {
    let normalized = local_name(name.trim()).to_ascii_lowercase();
    let guarded = matches!(
        normalized.as_str(),
        "width"
            | "height"
            | "x"
            | "y"
            | "x1"
            | "x2"
            | "y1"
            | "y2"
            | "r"
            | "cx"
            | "cy"
            | "rx"
            | "ry"
            | "stroke-width"
    );
    if !guarded {
        return None;
    }

    let trimmed = value.trim();
    let number = trimmed.strip_suffix("px")?.trim();
    if number.parse::<f64>().is_ok_and(f64::is_finite) {
        Some(number.to_string())
    } else {
        None
    }
}

fn is_start_or_empty_tag(tag: &str, expected: &str) -> bool {
    let tag = tag.trim_start();
    if !tag.starts_with('<') || tag.starts_with("</") || tag.starts_with("<!--") {
        return false;
    }

    let name = tag[1..]
        .chars()
        .take_while(|ch| !ch.is_whitespace() && *ch != '/' && *ch != '>')
        .collect::<String>();
    name.eq_ignore_ascii_case(expected)
}

fn active_svg_element_name(tag: &str) -> Option<&str> {
    let name = start_tag_name(tag)?;
    matches_active_svg_element(name).then_some(name)
}

pub(in crate::svg::pipeline) fn matches_active_svg_element(name: &str) -> bool {
    matches!(
        local_name(name).to_ascii_lowercase().as_str(),
        "animate"
            | "animatecolor"
            | "animatemotion"
            | "animatetransform"
            | "applet"
            | "audio"
            | "canvas"
            | "discard"
            | "embed"
            | "foreignobject"
            | "form"
            | "iframe"
            | "link"
            | "mpath"
            | "object"
            | "script"
            | "set"
            | "video"
    )
}

fn local_name(name: &str) -> &str {
    name.rsplit_once(':')
        .map_or(name, |(_, local_name)| local_name)
}

fn is_namespace_declaration(name: &str) -> bool {
    let name = name.trim();
    name == "xmlns" || name.starts_with("xmlns:")
}

fn find_close_tag_end<E>(
    svg: &str,
    from: usize,
    name: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<Option<usize>, E> {
    let mut scanner = SvgTagScanner::new(svg);
    scanner.skip_to(from);
    while let Some(tag) = scanner.next_with_checkpoints(checkpoint)? {
        if close_tag_matches(tag.raw(), name) {
            return Ok(Some(scanner.cursor()));
        }
    }
    checkpoint()?;
    Ok(None)
}

fn close_tag_matches(tag: &str, expected: &str) -> bool {
    let tag = tag.trim_start();
    if !tag.starts_with("</") {
        return false;
    }
    let name_start = 2;
    let name_end = name_start
        + tag[name_start..]
            .find(|ch: char| ch.is_whitespace() || ch == '>')
            .unwrap_or(tag.len() - name_start);
    if name_start >= name_end {
        return false;
    }
    let after_name = tag[name_end..].trim();
    after_name == ">" && tag[name_start..name_end].eq_ignore_ascii_case(expected)
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let mut cursor = 0usize;
    while let Some(attr) = next_svg_quoted_attr(tag, cursor) {
        if tag[attr.name_start..attr.name_end].eq_ignore_ascii_case(name) {
            return Some(tag[attr.value_start..attr.value_end].to_string());
        }
        cursor = attr.full_end;
    }
    None
}

fn is_missing_or_invalid_rect_dimension(value: Option<&str>) -> bool {
    let Some(value) = value.map(str::trim) else {
        return true;
    };
    if value.is_empty() {
        return true;
    }
    if let Ok(n) = value.parse::<f64>() {
        return !n.is_finite() || n <= 0.0;
    }
    false
}

fn is_bad_rect_tag(tag: &str) -> bool {
    if !is_start_or_empty_tag(tag, "rect") {
        return false;
    }

    let width = attr_value(tag, "width");
    let height = attr_value(tag, "height");
    is_missing_or_invalid_rect_dimension(width.as_deref())
        || is_missing_or_invalid_rect_dimension(height.as_deref())
}

fn sanitize_style_attribute<E>(
    value: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<String, E> {
    let mut out = Vec::new();

    for (index, (property, value)) in parse_css_declarations(value, checkpoint)?
        .into_iter()
        .enumerate()
    {
        checkpoint_loop(index, checkpoint)?;
        let Some(normalized_value) = sanitize_css_value_with_checkpoints(&value, checkpoint)?
        else {
            continue;
        };
        if normalized_value.is_empty()
            || is_invalid_style_property_value(&property, &normalized_value)
        {
            continue;
        }
        if property.to_ascii_lowercase().starts_with("animation") {
            continue;
        }

        out.push(format!("{property}:{normalized_value}"));
    }

    checkpoint()?;
    Ok(escape_xml_attr(&out.join(";")))
}

fn parse_css_declarations<E>(
    value: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<Vec<(String, String)>, E> {
    checkpoint()?;
    let decoded = merman_core::entities::decode_html_entities_to_unicode(value);
    checkpoint()?;
    let mut parser = Parser::new(&decoded);
    let mut declarations = Vec::new();

    while !parser.is_exhausted() {
        checkpoint()?;
        let declaration = parser.parse_until_after(Delimiter::Semicolon, |declaration| {
            let property = declaration.expect_ident_cloned()?.to_string();
            declaration.expect_colon()?;
            let value_start = declaration.position();
            // `expect_no_error_token` recursively descends into every block. The bounded
            // sanitizer below validates the raw value after this iterative boundary scan.
            while declaration.next_including_whitespace().is_ok() {}
            let value = declaration.slice_from(value_start).trim().to_string();
            Ok::<_, cssparser::ParseError<()>>((property, value))
        });
        checkpoint()?;
        if let Ok(declaration) = declaration {
            declarations.push(declaration);
        }
    }
    Ok(declarations)
}

fn is_invalid_style_property_value(property: &str, value: &str) -> bool {
    match property.trim().to_ascii_lowercase().as_str() {
        "fill" | "stroke" | "color" | "flood-color" | "lighting-color" | "stop-color" => {
            is_mermaid_missing_amount_hsl(value)
        }
        "stroke-width" | "stroke-dashoffset" | "fill-opacity" | "stroke-opacity" | "opacity"
        | "x" | "y" | "width" | "height" | "rx" | "ry" => is_provably_invalid_scalar(value),
        "transform" => is_invalid_svg_transform(value),
        "d" => contains_non_finite_numeric_token(value),
        _ => false,
    }
}

fn is_provably_invalid_scalar(value: &str) -> bool {
    let value = value.trim();
    if value.eq_ignore_ascii_case("undefined") {
        return true;
    }

    let number = ["rem", "px", "%", "em", "ex", "in", "cm", "mm", "pt", "pc"]
        .into_iter()
        .find_map(|unit| value.strip_suffix(unit))
        .unwrap_or(value)
        .trim();
    number
        .parse::<f64>()
        .is_ok_and(|number| !number.is_finite())
}

fn is_invalid_svg_transform(value: &str) -> bool {
    let mut remaining = value.trim();
    if remaining.eq_ignore_ascii_case("none") {
        return false;
    }

    while !remaining.is_empty() {
        remaining = remaining.trim_start_matches(|ch: char| ch.is_whitespace() || ch == ',');
        if remaining.is_empty() {
            break;
        }
        let name_end = remaining
            .find(|ch: char| !ch.is_ascii_alphabetic())
            .unwrap_or(remaining.len());
        let name = &remaining[..name_end];
        let Some(expected) = transform_argument_counts(name) else {
            return false;
        };
        remaining = &remaining[name_end..];
        let Some(after_open) = remaining.strip_prefix('(') else {
            return true;
        };
        let Some(close) = after_open.find(')') else {
            return true;
        };
        let arguments_text = &after_open[..close];
        let arguments = arguments_text
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .filter(|argument| !argument.is_empty())
            .collect::<Vec<_>>();
        if !expected.contains(&arguments.len())
            || arguments.iter().any(|argument| {
                argument
                    .parse::<f64>()
                    .map_or(true, |number| !number.is_finite())
            })
        {
            return true;
        }
        remaining = &after_open[close + 1..];
    }
    false
}

fn transform_argument_counts(name: &str) -> Option<&'static [usize]> {
    match name {
        "matrix" => Some(&[6]),
        "translate" | "scale" => Some(&[1, 2]),
        "rotate" => Some(&[1, 3]),
        "skewX" | "skewY" => Some(&[1]),
        _ => None,
    }
}

fn contains_non_finite_numeric_token(value: &str) -> bool {
    ["NaN", "Infinity", "-Infinity", "+Infinity"]
        .into_iter()
        .any(|token| contains_delimited_numeric_token(value, token))
}

fn contains_delimited_numeric_token(value: &str, token: &str) -> bool {
    value.match_indices(token).any(|(start, _)| {
        let before = value[..start].chars().next_back();
        let after = value[start + token.len()..].chars().next();
        is_numeric_token_boundary(before) && is_numeric_token_boundary(after)
    })
}

fn is_numeric_token_boundary(ch: Option<char>) -> bool {
    ch.is_none_or(|ch| {
        ch.is_whitespace()
            || matches!(ch, ',' | '(' | ')')
            || matches!(
                ch,
                'M' | 'm'
                    | 'Z'
                    | 'z'
                    | 'L'
                    | 'l'
                    | 'H'
                    | 'h'
                    | 'V'
                    | 'v'
                    | 'C'
                    | 'c'
                    | 'S'
                    | 's'
                    | 'Q'
                    | 'q'
                    | 'T'
                    | 't'
                    | 'A'
                    | 'a'
            )
    })
}

#[cfg(test)]
mod tests {
    use super::{parsed_attribute_violates_resvg_contract, sanitize_element_attributes};

    #[test]
    fn sanitize_style_attribute_drops_invalid_bare_declarations() {
        let svg = r#"<svg><path style="undefined; stroke: #333; undefined"/></svg>"#;
        let out = sanitize_element_attributes(svg);

        assert!(!out.contains("undefined"), "got: {out}");
        assert!(out.contains(r#"style="stroke:#333""#), "got: {out}");
    }

    #[test]
    fn sanitize_element_attributes_drops_rects_without_positive_dimensions() {
        let svg = r#"<svg><rect/><rect width="0" height="10"/><rect width="12" height="8"/><g><rect width="NaN" height="10"><title>bad</title></rect></g></svg>"#;
        let out = sanitize_element_attributes(svg);

        assert!(!out.contains("<rect/>"), "got: {out}");
        assert!(!out.contains(r#"width="0""#), "got: {out}");
        assert!(!out.contains("NaN"), "got: {out}");
        assert!(!out.contains("<title>bad</title>"), "got: {out}");
        assert!(
            out.contains(r#"<rect width="12" height="8"/>"#),
            "got: {out}"
        );
    }

    #[test]
    fn sanitize_element_attributes_scans_double_quoted_attrs_without_regex() {
        let svg = r#"<svg><path data-keep = "ok" x = "10px" stroke="" style="transform: rotate(45deg); animation: dash 1s; stroke: #333;"/></svg>"#;
        let out = sanitize_element_attributes(svg);

        assert!(out.contains(r#"data-keep = "ok""#), "got: {out}");
        assert!(out.contains(r#" x="10""#), "got: {out}");
        assert!(!out.contains(r#"stroke="""#), "got: {out}");
        assert!(
            out.contains(r#"style="transform:rotate(45);stroke:#333""#),
            "got: {out}"
        );
        assert!(!out.contains("animation"), "got: {out}");
    }

    #[test]
    fn sanitize_element_attributes_scans_single_quoted_attrs() {
        let svg = r#"<svg><path x = '10px' style='animation: dash 1s; stroke: #333;'/></svg>"#;
        let out = sanitize_element_attributes(svg);

        assert!(out.contains(r#" x="10""#), "got: {out}");
        assert!(out.contains(r#"style="stroke:#333""#), "got: {out}");
        assert!(!out.contains("animation"), "got: {out}");
    }

    #[test]
    fn sanitize_element_attributes_uses_scanned_attrs_for_bad_rect_detection() {
        let svg = r#"<svg><rect WIDTH = "12" HEIGHT = "8"/><rect width = "NaN" height = "8"><title>bad</title></rect></svg>"#;
        let out = sanitize_element_attributes(svg);

        assert!(
            out.contains(r#"<rect WIDTH = "12" HEIGHT = "8"/>"#),
            "got: {out}"
        );
        assert!(!out.contains("NaN"), "got: {out}");
        assert!(!out.contains("<title>bad</title>"), "got: {out}");
    }

    #[test]
    fn sanitize_element_attributes_strips_active_svg_elements() {
        let svg = r##"<svg><script>alert(1)</script><svg:script>alert(2)</svg:script><SCRIPT/><iframe src="https://example.com"></iframe><object data="x"></object><animate attributeName="x"/><animateMotion><mpath href="#route"/></animateMotion><animateTransform/><animateColor/><set attributeName="fill" to="red"/><discard/><rect width="12" height="8"/></svg>"##;
        let out = sanitize_element_attributes(svg);
        let lower = out.to_ascii_lowercase();

        for element in [
            "script",
            "svg:script",
            "iframe",
            "object",
            "animate",
            "animatemotion",
            "animatetransform",
            "animatecolor",
            "mpath",
            "set",
            "discard",
        ] {
            assert!(!lower.contains(&format!("<{element}")), "got: {out}");
        }
        assert!(
            out.contains(r#"<rect width="12" height="8"/>"#),
            "got: {out}"
        );
    }

    #[test]
    fn sanitize_element_attributes_strips_event_and_unsafe_url_attrs() {
        let svg = r##"<svg><a href="#local"><use href="#shape" xlink:href="#shape"/></a><a href="javascript&colon;alert(1)" onclick="alert(1)" svg:onload="alert(1)"><text>bad</text></a><image href="data:image/png;base64,AAAA"/><image href="data:text/html;base64,AAAA"/><image href="file:///etc/passwd"/><a href="java&#x3a;script:alert(1)"/></svg>"##;
        let out = sanitize_element_attributes(svg);
        let lower = out.to_ascii_lowercase();

        assert!(out.contains(r##"href="#local""##), "got: {out}");
        assert!(out.contains(r##"href="#shape""##), "got: {out}");
        assert!(out.contains(r##"xlink:href="#shape""##), "got: {out}");
        assert!(
            out.contains(r#"href="data:image/png;base64,AAAA""#),
            "got: {out}"
        );
        assert!(!lower.contains("onclick"), "got: {out}");
        assert!(!lower.contains("svg:onload"), "got: {out}");
        assert!(!lower.contains("javascript"), "got: {out}");
        assert!(!lower.contains("data:text/html"), "got: {out}");
        assert!(!lower.contains("file:///"), "got: {out}");
    }

    #[test]
    fn sanitize_element_attributes_decodes_exactly_one_svg_serialization_layer() {
        let svg = r##"<svg>
<a href="jav&#x61;script:alert(1)"><text>decoded by the SVG parser</text></a>
<a href="jav&amp;#x61;script:alert(2)"><text>literal DOM entity spelling</text></a>
<a href=""><text>empty href</text></a>
</svg>"##;

        let out = sanitize_element_attributes(svg);

        assert!(!out.contains(r#"href="jav&#x61;script:alert(1)""#), "{out}");
        assert!(
            out.contains(r#"href="jav&amp;#x61;script:alert(2)""#),
            "{out}"
        );
        assert!(out.contains(r#"href="""#), "{out}");
    }

    #[test]
    fn sanitize_element_attributes_normalizes_anchor_dom_values_like_dompurify() {
        let svg = format!(
            r##"<svg>
<a href="  https://example.test/ticket  "><text>trimmed</text></a>
<a href="   "><text>empty</text></a>
<a href="{}javascript:alert(1)"><text>bom</text></a>
</svg>"##,
            '\u{FEFF}'
        );

        let out = sanitize_element_attributes(&svg);

        assert!(
            out.contains(r#"href="https://example.test/ticket""#),
            "{out}"
        );
        assert!(out.contains(r#"href="""#), "{out}");
        assert!(!out.contains("javascript:alert(1)"), "{out}");
    }

    #[test]
    fn parsed_resvg_contract_does_not_decode_navigation_entities_twice() {
        assert!(!parsed_attribute_violates_resvg_contract(
            "a",
            "href",
            "javascript&colon;ticket"
        ));
        assert!(parsed_attribute_violates_resvg_contract(
            "a",
            "href",
            "javascript:ticket"
        ));
        assert!(parsed_attribute_violates_resvg_contract(
            "a",
            "href",
            " https://example.test "
        ));
    }

    #[test]
    fn sanitize_element_attributes_applies_contract_to_usvg_namespace_aliases() {
        let svg = r##"<svg xmlns:s="http://www.w3.org/2000/svg" xmlns:q="http://www.w3.org/1999/xlink" xmlns:style="urn:example:ignored">
<path
    s:style="animation:spin 1s;stroke:#333"
    q:fill="url(file:///tmp/paint.svg#paint)"
    xml:width="NaN"
    s:transform="rotate(NaN)"
    q:d="M 0 NaN"
    xml:x="10px"
/>
</svg>"##;

        let out = sanitize_element_attributes(svg);

        assert!(
            out.contains(r#"xmlns:s="http://www.w3.org/2000/svg""#),
            "{out}"
        );
        assert!(
            out.contains(r#"xmlns:q="http://www.w3.org/1999/xlink""#),
            "{out}"
        );
        assert!(
            out.contains(r#"xmlns:style="urn:example:ignored""#),
            "{out}"
        );
        assert!(out.contains(r#"s:style="stroke:#333""#), "{out}");
        assert!(out.contains(r#"xml:x="10""#), "{out}");
        for unsafe_value in [
            "animation",
            "file:///",
            "q:fill",
            "xml:width",
            "s:transform",
            "q:d",
            "NaN",
        ] {
            assert!(
                !out.contains(unsafe_value),
                "{unsafe_value:?} survived: {out}"
            );
        }
    }

    #[test]
    fn sanitize_element_attributes_requires_decodable_data_image_payloads() {
        let svg = r#"<svg>
<image href="data:image/png;base64,AAAA"/>
<image href="data:image/png,%89PNG%0D%0A"/>
<image href="data:image/png;base64,AA*A"/>
<image href="data:image/png;base64,A==="/>
<image href="data:image/png,%"/>
<image href="data:image/png,%GG"/>
<image href="data:image/png;base64,AA%GG"/>
</svg>"#;

        let out = sanitize_element_attributes(svg);

        assert!(
            out.contains(r#"href="data:image/png;base64,AAAA""#),
            "{out}"
        );
        assert!(
            out.contains(r#"href="data:image/png,%89PNG%0D%0A""#),
            "{out}"
        );
        for malformed in [
            "base64,AA*A",
            "base64,A===",
            r#"href="data:image/png,%""#,
            "data:image/png,%GG",
            "base64,AA%GG",
        ] {
            assert!(!out.contains(malformed), "{malformed:?} survived: {out}");
        }
    }

    #[test]
    fn sanitize_element_attributes_closes_non_navigation_render_resources() {
        let svg = r##"<svg xmlns:q="http://www.w3.org/1999/xlink" xmlns:b="http://www.w3.org/XML/1998/namespace" xml:base="/tmp/">
<a href="https://example.com/docs" q:href="../guide" data-bad="data:image/png;base64,CCCC"><text>links</text></a>
<a href="data:image/png;base64,BBBB"><text>data link</text></a>
<g b:base="../nested/"><image href="#nested-image"/></g>
<image href="/tmp/absolute.png"/>
<image href="../relative.png"/>
<image href="./sibling.png"/>
<image href="bare.png"/>
<image href="//example.com/protocol-relative.png"/>
<image href="\\server\share\image.png"/>
<image href="C:\private\image.png"/>
<image href="https://example.com/remote.png"/>
<image href="data:image/png;base64,AAAA"/>
<image href="data:image/png"/>
<image href="d a t a:image/png;base64,EEEE"/>
<image href="data:image /png;base64,FFFF"/>
<image href="data:image/svg+xml;base64,PHN2Zy8+"/>
<image href="#embedded-image"/>
<feImage href="#embedded-filter-source"/>
<feImage href="data:image/png;base64,GGGG"/>
<feImage q:href="../filter.png"/>
<use href="#shape" q:href="#shape"/>
<use href="data:image/png;base64,DDDD"/>
<use href="https://example.com/sprite.svg#shape"/>
<textPath href="#path">safe</textPath>
<textPath href="../text.svg#path">unsafe</textPath>
</svg>"##;

        let out = sanitize_element_attributes(svg);

        assert!(out.contains(r#"href="https://example.com/docs""#), "{out}");
        assert!(out.contains(r#"q:href="../guide""#), "{out}");
        assert!(
            out.contains(r#"href="data:image/png;base64,AAAA""#),
            "{out}"
        );
        assert!(out.contains(r##"href="#embedded-filter-source""##), "{out}");
        assert!(
            out.contains(r#"href="data:image/png;base64,GGGG""#),
            "{out}"
        );
        assert!(out.contains(r##"href="#shape""##), "{out}");
        assert!(out.contains(r##"q:href="#shape""##), "{out}");
        assert!(out.contains(r##"href="#path""##), "{out}");

        for external in [
            "/tmp/",
            "absolute.png",
            "relative.png",
            "sibling.png",
            "bare.png",
            "protocol-relative.png",
            r"\\server\share",
            r"C:\private",
            "remote.png",
            r#"href="data:image/png""#,
            "d a t a:",
            "image /png",
            "image/svg+xml",
            "embedded-image",
            "base64,DDDD",
            "filter.png",
            "sprite.svg",
            "text.svg",
            "xml:base",
            "b:base",
        ] {
            assert!(!out.contains(external), "{external:?} survived: {out}");
        }
        assert!(
            out.contains(r#"data-bad="data:image/png;base64,CCCC""#),
            "{out}"
        );
        assert!(!out.contains("data:image/png;base64,BBBB"), "{out}");
        assert_eq!(sanitize_element_attributes(&out), out);
    }

    #[test]
    fn sanitize_element_attributes_preserves_dompurify_safe_anchor_schemes() {
        let svg = r##"<svg>
<a href="ftp://example.com/file"><text>ftp</text></a>
<a href="ftps://example.com/file"><text>ftps</text></a>
<a href="tel:+123"><text>tel</text></a>
<a href="callto:+123"><text>call</text></a>
<a href="sms:+123"><text>sms</text></a>
<a href="cid:part@example.com"><text>cid</text></a>
<a href="xmpp:user@example.com"><text>xmpp</text></a>
<a href="matrix:r/room:example.com"><text>matrix</text></a>
<a href="javascript:alert(1)"><text>script</text></a>
</svg>"##;

        let out = sanitize_element_attributes(svg);

        for href in [
            "ftp://example.com/file",
            "ftps://example.com/file",
            "tel:+123",
            "callto:+123",
            "sms:+123",
            "cid:part@example.com",
            "xmpp:user@example.com",
            "matrix:r/room:example.com",
        ] {
            assert!(out.contains(&format!(r#"href="{href}""#)), "{href}: {out}");
        }
        assert!(!out.contains("javascript:alert(1)"), "{out}");
    }

    #[test]
    fn sanitize_element_attributes_drops_external_css_resource_urls() {
        let svg = r##"<svg><path
            fill="url(#paint)"
            cursor="url(../cursor.svg)"
            background="url(https://example.com/background.png)"
            style="clip-path:url(#clip);background-image:url(/tmp/background.png);--inline:url(data:image/webp;base64,AAAA);--missing-comma:url(data:image/png);--spaced:url(&quot;d a t a:image/png;base64,BBBB&quot;);stroke:#333"
        /></svg>"##;

        let out = sanitize_element_attributes(svg);

        assert!(out.contains(r##"fill="url(#paint)""##), "{out}");
        assert!(out.contains("clip-path:url(#clip)"), "{out}");
        assert!(
            out.contains("--inline:url(data:image/webp;base64,AAAA)"),
            "{out}"
        );
        assert!(out.contains("stroke:#333"), "{out}");
        assert!(!out.contains("cursor="), "{out}");
        assert!(!out.contains("background="), "{out}");
        assert!(!out.contains("background-image"), "{out}");
        assert!(!out.contains("missing-comma"), "{out}");
        assert!(!out.contains("--spaced"), "{out}");
        assert!(!out.contains("/tmp/"), "{out}");
        assert!(!out.contains("https://"), "{out}");
    }

    #[test]
    fn sanitize_element_attributes_preserves_safe_fragment_ids_with_invalid_token_prefixes() {
        let svg = r##"<svg><defs><linearGradient id="undefined"/><linearGradient id="nan"/><linearGradient id="undefined-gradient"/><linearGradient id="nan-stroke"/></defs><circle fill="url(#undefined)" stroke="url(#nan)"/><circle fill="url(#undefined-gradient)" stroke="url(#nan-stroke)"/></svg>"##;

        let out = sanitize_element_attributes(svg);

        assert!(out.contains(r##"fill="url(#undefined)""##), "{out}");
        assert!(out.contains(r##"stroke="url(#nan)""##), "{out}");
        assert!(
            out.contains(r##"fill="url(#undefined-gradient)""##),
            "{out}"
        );
        assert!(out.contains(r##"stroke="url(#nan-stroke)""##), "{out}");
    }

    #[test]
    fn sanitize_style_preserves_custom_property_identifier_values() {
        let svg =
            r#"<svg><path style="--state: undefined; --number-kind: NaN; stroke: #333"/></svg>"#;

        let out = sanitize_element_attributes(svg);

        assert!(out.contains("--state:undefined"), "{out}");
        assert!(out.contains("--number-kind:NaN"), "{out}");
        assert!(out.contains("stroke:#333"), "{out}");
    }

    #[test]
    fn sanitize_style_keeps_xml_entities_and_nested_semicolons_inside_values() {
        let svg = r#"<svg><text style="font-family:&quot;a;b&quot;,sans-serif;--data:url(data:image/png;base64,AAAA);stroke:#333"/></svg>"#;

        let out = sanitize_element_attributes(svg);
        roxmltree::Document::parse(&out).expect("sanitized style must remain valid XML");

        assert!(
            out.contains("font-family:&quot;a;b&quot;,sans-serif"),
            "{out}"
        );
        assert!(
            out.contains("--data:url(data:image/png;base64,AAAA)"),
            "{out}"
        );
        assert!(out.contains("stroke:#333"), "{out}");
    }

    #[test]
    fn css_url_tokens_handle_quoted_parentheses_escapes_and_nested_functions() {
        let svg = r##"<svg><defs><linearGradient id="paint)close"/></defs><circle fill="url(&quot;#paint)close&quot;)" stroke="u\72l(&quot;j\61vascript:alert(1)&quot;)" filter="drop-shadow(0 0 2px u\72l(&quot;j\61vascript:alert(1)&quot;))"/></svg>"##;

        let out = sanitize_element_attributes(svg);

        assert!(
            out.contains(r##"fill="url(&quot;#paint)close&quot;)""##),
            "{out}"
        );
        assert!(!out.contains(" stroke="), "{out}");
        assert!(!out.contains(" filter="), "{out}");
        assert!(!out.contains(r"u\72l"), "{out}");
    }

    #[test]
    fn sanitize_element_attributes_strips_unsafe_css_url_functions() {
        let svg = r##"<svg><path fill="url(#paint)" stroke="url(javascript:alert(1))" style="clip-path: url(#clip); filter: url('java&#x73;cript:alert(1)'); stroke: #333"/></svg>"##;
        let out = sanitize_element_attributes(svg);
        let lower = out.to_ascii_lowercase();

        assert!(out.contains(r##"fill="url(#paint)""##), "got: {out}");
        assert!(out.contains("clip-path:url(#clip)"), "got: {out}");
        assert!(out.contains("stroke:#333"), "got: {out}");
        assert!(!lower.contains("javascript"), "got: {out}");
        assert!(!lower.contains(r#"stroke="url("#), "got: {out}");
        assert!(!lower.contains("filter:"), "got: {out}");
    }

    #[test]
    fn sanitize_style_rewrites_angle_tokens_without_corrupting_text() {
        let svg = r##"<svg><path style="transform:rotate(45deg);content:&quot;45deg&quot;;background:url(#a45deg);--label:foo45deg"/></svg>"##;
        let out = sanitize_element_attributes(svg);

        assert!(out.contains("transform:rotate(45)"), "{out}");
        assert!(out.contains("content:&quot;45deg&quot;"), "{out}");
        assert!(out.contains("background:url(#a45deg)"), "{out}");
        assert!(out.contains("--label:foo45deg"), "{out}");
    }
}
