use crate::Result;
use cssparser::{Delimiter, Parser};
use std::borrow::Cow;

use super::util::{escape_xml_attr, find_quoted_attr_value_span, find_tag_end};
use crate::svg::pipeline::{SvgPostprocessContext, SvgPostprocessor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootBackgroundPostprocessor {
    background_color: String,
}

impl RootBackgroundPostprocessor {
    pub fn new(background_color: impl Into<String>) -> Self {
        Self {
            background_color: background_color.into(),
        }
    }

    pub fn background_color(&self) -> &str {
        &self.background_color
    }
}

impl SvgPostprocessor for RootBackgroundPostprocessor {
    fn name(&self) -> &'static str {
        "root-background"
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        _ctx: &SvgPostprocessContext<'_>,
    ) -> Result<Cow<'a, str>> {
        let background_color = self.background_color.trim();
        if background_color.is_empty() || !svg.contains("<svg") {
            return Ok(svg);
        }

        Ok(Cow::Owned(set_root_background_color(
            svg.as_ref(),
            background_color,
        )))
    }
}

pub(crate) fn set_root_background_color(svg: &str, background_color: &str) -> String {
    let Some(svg_start) = svg.find("<svg") else {
        return svg.to_string();
    };
    let Some(svg_end) = find_tag_end(svg, svg_start) else {
        return svg.to_string();
    };

    let tag = &svg[svg_start..=svg_end];
    let escaped_color = escape_xml_attr(background_color.trim());

    if let Some((style_value_start, style_value_end)) = find_quoted_attr_value_span(tag, "style") {
        let style = &tag[style_value_start..style_value_end];
        let rewritten = set_background_in_style_attr(style, &escaped_color);
        let absolute_value_start = svg_start + style_value_start;
        let absolute_value_end = svg_start + style_value_end;

        let mut out =
            String::with_capacity(svg.len() + rewritten.len().saturating_sub(style.len()));
        out.push_str(&svg[..absolute_value_start]);
        out.push_str(&rewritten);
        out.push_str(&svg[absolute_value_end..]);
        return out;
    }

    let insert_at = if svg.as_bytes().get(svg_end.saturating_sub(1)) == Some(&b'/') {
        svg_end - 1
    } else {
        svg_end
    };

    let mut out = String::with_capacity(svg.len() + escaped_color.len() + 34);
    out.push_str(&svg[..insert_at]);
    out.push_str(r#" style="background-color: "#);
    out.push_str(&escaped_color);
    out.push_str(r#";""#);
    out.push_str(&svg[insert_at..]);
    out
}

fn set_background_in_style_attr(style: &str, background_color: &str) -> String {
    if !style.split(';').any(|declaration| {
        declaration
            .split_once(':')
            .is_some_and(|(property, _)| property.trim().eq_ignore_ascii_case("background-color"))
    }) {
        let separator = if style.trim().is_empty() || style.trim_end().ends_with(';') {
            ""
        } else {
            ";"
        };
        return format!("{style}{separator} background-color: {background_color};");
    }
    let decoded = merman_core::entities::decode_html_entities_to_unicode(style);
    let background_color = merman_core::entities::decode_html_entities_to_unicode(background_color);
    let mut parser = Parser::new(&decoded);
    let mut declarations = Vec::new();
    let mut replaced = false;

    while !parser.is_exhausted() {
        let declaration = parser.parse_until_after(Delimiter::Semicolon, |declaration| {
            let start = declaration.position();
            while declaration.next_including_whitespace().is_ok() {}
            Ok::<_, cssparser::ParseError<()>>(declaration.slice_from(start).trim().to_string())
        });
        let Ok(trimmed) = declaration else {
            continue;
        };
        if trimmed.is_empty() {
            continue;
        }

        let Some((property, _value)) = trimmed.split_once(':') else {
            declarations.push(trimmed);
            continue;
        };

        if property.trim().eq_ignore_ascii_case("background-color") {
            if !replaced {
                declarations.push(format!("background-color: {background_color}"));
                replaced = true;
            }
        } else {
            declarations.push(trimmed);
        }
    }

    if !replaced {
        declarations.push(format!("background-color: {background_color}"));
    }

    escape_xml_attr(&format!("{};", declarations.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::pipeline::SvgPipeline;

    fn render_session() -> crate::environment::RenderSession {
        crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap()
    }

    #[test]
    fn root_background_rewrites_existing_background_color() {
        let svg =
            r#"<svg id="diagram" style="max-width: 400px; background-color: white;"><g/></svg>"#;
        let session = render_session();

        let out = SvgPipeline::parity()
            .with_postprocessor(RootBackgroundPostprocessor::new("#111827"))
            .process_to_string(svg, &session)
            .unwrap();

        assert_eq!(
            out,
            r#"<svg id="diagram" style="max-width: 400px; background-color: #111827;"><g/></svg>"#
        );
    }

    #[test]
    fn root_background_adds_missing_style_property() {
        let svg = r#"<svg id="diagram" width="100%"><g/></svg>"#;
        let session = render_session();

        let out = SvgPipeline::parity()
            .with_postprocessor(RootBackgroundPostprocessor::new("transparent"))
            .process_to_string(svg, &session)
            .unwrap();

        assert_eq!(
            out,
            r#"<svg id="diagram" width="100%" style="background-color: transparent;"><g/></svg>"#
        );
    }

    #[test]
    fn root_background_escapes_xml_attribute_value() {
        let svg = r#"<svg id="diagram" style="max-width: 400px;"><g/></svg>"#;

        let out = set_root_background_color(svg, "rgb(1, 2, 3)&");

        assert!(out.contains("background-color: rgb(1, 2, 3)&amp;;"));
    }

    #[test]
    fn root_background_preserves_custom_font_entities() {
        let svg = r#"<svg style="--font-family: &quot;Open Sans&quot;, sans-serif;"><g/></svg>"#;
        let out = set_root_background_color(svg, "white");
        assert_eq!(
            out,
            r#"<svg style="--font-family: &quot;Open Sans&quot;, sans-serif; background-color: white;"><g/></svg>"#
        );
        let replaced = set_root_background_color(&out, "transparent");
        assert_eq!(
            replaced,
            r#"<svg style="--font-family: &quot;Open Sans&quot;, sans-serif; background-color: transparent;"><g/></svg>"#
        );
    }

    #[test]
    fn root_background_preserves_semicolons_in_css_values() {
        let svg = r#"<svg style="--font: &quot;A;B&quot;; background-color: white;"><g/></svg>"#;
        assert_eq!(
            set_root_background_color(svg, "#112233"),
            r##"<svg style="--font: &quot;A;B&quot;; background-color: #112233;"><g/></svg>"##
        );
    }

    #[test]
    fn root_background_rewrites_single_quoted_style_attr() {
        let svg =
            r#"<svg id="diagram" style='max-width: 400px; background-color: white;'><g/></svg>"#;

        let out = set_root_background_color(svg, "#111827");

        assert_eq!(
            out,
            r##"<svg id="diagram" style='max-width: 400px; background-color: #111827;'><g/></svg>"##
        );
    }
}
