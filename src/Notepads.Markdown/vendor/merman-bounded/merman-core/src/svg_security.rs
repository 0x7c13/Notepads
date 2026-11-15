use crate::entities::decode_mermaid_entity_placeholders;

/// Identifies the representation layer of an SVG URI attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MermaidSvgUriRepresentation {
    /// The value has already been parsed into a DOM attribute value.
    DomValue,
    /// The value still belongs to serialized SVG markup.
    SerializedSvg,
}

/// Selects Mermaid's final navigation cleanup behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MermaidNavigationSecurity {
    /// Match Mermaid's trusted `securityLevel: loose` output.
    Loose,
    /// Match Mermaid's DOMPurify-backed strict-like security levels.
    Sanitized,
}

impl MermaidNavigationSecurity {
    pub const fn from_security_level_loose(loose: bool) -> Self {
        if loose { Self::Loose } else { Self::Sanitized }
    }
}

/// A navigation URL serialized exactly once for insertion between SVG attribute quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedMermaidNavigationHref(String);

impl SerializedMermaidNavigationHref {
    pub fn as_serialized_str(&self) -> &str {
        &self.0
    }
}

/// Applies Mermaid's final renderer-created navigation transition.
///
/// Mermaid serializes its renderer DOM, restores `encodeEntities` placeholders, and then applies
/// DOMPurify for strict-like security levels. The returned value remains serialized markup so
/// renderers cannot accidentally validate or escape the wrong representation layer.
pub fn prepare_mermaid_navigation_href(
    value: &str,
    security: MermaidNavigationSecurity,
) -> Option<SerializedMermaidNavigationHref> {
    let serialized_before_cleanup = serialize_svg_attribute_value(value);
    let cleaned_serialized = decode_mermaid_entity_placeholders(&serialized_before_cleanup);

    if security == MermaidNavigationSecurity::Loose {
        return Some(SerializedMermaidNavigationHref(
            cleaned_serialized.into_owned(),
        ));
    }

    let normalized_dom = admit_mermaid_svg_uri_attribute(
        cleaned_serialized.as_ref(),
        MermaidSvgUriRepresentation::SerializedSvg,
    )?;
    Some(SerializedMermaidNavigationHref(
        serialize_svg_attribute_value(&normalized_dom),
    ))
}

/// Normalizes a URI attribute at a declared SVG representation boundary.
pub fn admit_mermaid_svg_uri_attribute(
    value: &str,
    representation: MermaidSvgUriRepresentation,
) -> Option<String> {
    match representation {
        MermaidSvgUriRepresentation::DomValue => {
            crate::sanitize::dompurify_normalize_dom_uri_attribute(value)
        }
        MermaidSvgUriRepresentation::SerializedSvg => {
            crate::sanitize::dompurify_normalize_serialized_uri_attribute(value)
        }
    }
}

/// Applies Mermaid's strict-like normalization to a renderer-created tooltip attribute.
pub fn normalize_mermaid_tooltip_attribute(value: &str) -> &str {
    crate::sanitize::dompurify_normalize_dom_attribute_value("title", value)
}

fn serialize_svg_attribute_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if !is_xml_1_0_char(ch) {
            continue;
        }
        match ch {
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn is_xml_1_0_char(ch: char) -> bool {
    matches!(ch, '\u{9}' | '\u{A}' | '\u{D}')
        || matches!(
            ch,
            '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
        )
}

#[cfg(test)]
mod tests {
    use super::{
        MermaidNavigationSecurity, MermaidSvgUriRepresentation, admit_mermaid_svg_uri_attribute,
        prepare_mermaid_navigation_href,
    };

    fn strict(value: &str) -> Option<String> {
        prepare_mermaid_navigation_href(value, MermaidNavigationSecurity::Sanitized)
            .map(|href| href.as_serialized_str().to_string())
    }

    fn loose(value: &str) -> String {
        prepare_mermaid_navigation_href(value, MermaidNavigationSecurity::Loose)
            .expect("loose security preserves renderer-created navigation")
            .as_serialized_str()
            .to_string()
    }

    #[test]
    fn strict_security_runs_cleanup_before_uri_admission() {
        assert_eq!(strict("javascriptﬂ°colon¶ßalert(1)"), None);
        assert_eq!(strict("\u{FEFF}javascript:alert(1)"), None);
        assert_eq!(
            strict(" https://example.test/ticket "),
            Some("https://example.test/ticket".into())
        );
        assert_eq!(strict("   "), Some(String::new()));
    }

    #[test]
    fn strict_security_consumes_exactly_one_serialization_layer() {
        assert_eq!(
            strict("jav&#x61;script:ticket"),
            Some("jav&amp;#x61;script:ticket".into())
        );
        assert_eq!(
            strict("javascript&colon;ticket"),
            Some("javascript&amp;colon;ticket".into())
        );
        assert_eq!(strict("javascript:ticket"), None);
    }

    #[test]
    fn loose_security_stops_after_mermaid_cleanup() {
        assert_eq!(
            loose("javascriptﬂ°colon¶ßalert(1)"),
            "javascript&colon;alert(1)"
        );
        assert_eq!(loose("A&B"), "A&amp;B");
    }

    #[test]
    fn representation_kind_controls_entity_decoding() {
        assert_eq!(
            admit_mermaid_svg_uri_attribute(
                "javascript&amp;colon;ticket",
                MermaidSvgUriRepresentation::SerializedSvg,
            ),
            Some("javascript&colon;ticket".into())
        );
        assert_eq!(
            admit_mermaid_svg_uri_attribute(
                "javascript&amp;colon;ticket",
                MermaidSvgUriRepresentation::DomValue,
            ),
            Some("javascript&amp;colon;ticket".into())
        );
    }
}
