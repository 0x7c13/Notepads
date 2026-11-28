//! Mermaid CSS/style helpers shared by layout and SVG parity code.

pub(crate) fn parse_safe_style_decl(s: &str) -> Option<(&str, &str)> {
    let s = s.trim().trim_end_matches(';').trim();
    if s.is_empty() {
        return None;
    }
    let (k, v) = s.split_once(':')?;
    let k = k.trim();
    let v = v.trim();
    if !is_safe_css_property_name(k) || !is_safe_css_declaration_value(v) {
        return None;
    }
    Some((k, v))
}

pub(crate) fn is_safe_css_font_family_value(value: &str) -> bool {
    is_safe_css_declaration_value(value) && !value.contains(':')
}

fn is_safe_css_property_name(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn is_safe_css_declaration_value(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }

    let lower = value.to_ascii_lowercase();
    if lower.contains("url(") || lower.contains("expression(") {
        return false;
    }

    value
        .chars()
        .all(|ch| !ch.is_control() && !matches!(ch, '<' | '>' | '{' | '}' | ';' | '@'))
}

pub(crate) fn is_label_style_key(key: &str) -> bool {
    matches!(
        key.trim(),
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

pub(crate) fn parse_css_font_size_px(raw: &str, inherited_px: f64) -> Option<f64> {
    let raw = raw.trim().trim_end_matches(';').trim();
    if raw.is_empty() {
        return None;
    }
    let lower = raw.to_ascii_lowercase();
    let inherited_px = inherited_px.max(1.0);

    if let Some(v) = lower.strip_suffix("px") {
        return parse_positive_f64(v);
    }
    if let Some(v) = lower.strip_suffix('%') {
        return parse_positive_f64(v).map(|pct| inherited_px * pct / 100.0);
    }
    if let Some(v) = lower.strip_suffix("rem") {
        return parse_positive_f64(v).map(|scale| inherited_px * scale);
    }
    if let Some(v) = lower.strip_suffix("em") {
        return parse_positive_f64(v).map(|scale| inherited_px * scale);
    }

    match lower.as_str() {
        "xx-small" => Some(inherited_px * 0.6),
        "x-small" => Some(inherited_px * 0.75),
        "small" => Some(inherited_px * 0.89),
        "medium" => Some(inherited_px),
        "large" => Some(inherited_px * 1.2),
        "x-large" => Some(inherited_px * 1.5),
        "xx-large" => Some(inherited_px * 2.0),
        "smaller" => Some(inherited_px * 0.8),
        "larger" => Some(inherited_px * 1.2),
        _ => parse_positive_f64(raw),
    }
}

fn parse_positive_f64(raw: &str) -> Option<f64> {
    let v = raw.trim().parse::<f64>().ok()?;
    (v.is_finite() && v > 0.0).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_safe_style_decl_accepts_mermaid_style_values() {
        assert_eq!(
            parse_safe_style_decl("fill: rgba(232,232,232, 0.8)"),
            Some(("fill", "rgba(232,232,232, 0.8)"))
        );
        assert_eq!(
            parse_safe_style_decl("font-family: \"IBM Plex Sans\", Arial, sans-serif"),
            Some(("font-family", "\"IBM Plex Sans\", Arial, sans-serif"))
        );
        assert_eq!(
            parse_safe_style_decl("stroke-dasharray: 5,5"),
            Some(("stroke-dasharray", "5,5"))
        );
    }

    #[test]
    fn parse_safe_style_decl_rejects_structural_injection_values() {
        for raw in [
            "fill: red;</style><svg>",
            "fill: red; stroke: blue",
            "fill: red} :not(&){background: green",
            "background: url(javascript:alert(1))",
            "width: expression(alert(1))",
            "bad>key: red",
        ] {
            assert_eq!(parse_safe_style_decl(raw), None, "expected reject: {raw}");
        }
    }
}
