//! Native SVG compatibility for Mermaid marker selectors.
//!
//! Mermaid 11.14 introduced ID suffix selectors. usvg 0.47.0 uses
//! simplecss 0.2.2, which rejects these and loses class fallbacks in the
//! same selector list. The upstream change is Mermaid commit
//! 107edad02d0efbb4aa504a9afe3911399f173bb0 (class/styles.js).
//! Transparent paint is supported; the lost rule is
//! the cause of filled aggregation diamonds. See Merman issue #181:
//! https://github.com/Latias94/merman/issues/181
//! Related parser limitation, with a different trigger: Typst issue #6595.
//! https://github.com/typst/typst/issues/6595
//!
//! Exact attribute matches retain specificity. Only resvg-safe output
//! applies this conversion, after IDs and user styles have been finalized.

use crate::svg::pipeline::SvgPostprocessExecution;
use crate::{Error, Result};
use cssparser::{Delimiter, ParseErrorKind, Parser, ToCss, Token};
use quick_xml::{Reader, events::Event};
use std::borrow::Cow;

fn error(message: impl std::fmt::Debug) -> Error {
    Error::svg_postprocess("lower-id-suffix-selectors", format!("{message:?}"))
}

fn css_error(parse_error: cssparser::ParseError<Error>) -> Error {
    match parse_error.kind {
        ParseErrorKind::Custom(error) => error,
        _ => error(parse_error),
    }
}

fn append(output: &mut String, value: &str, execution: SvgPostprocessExecution<'_>) -> Result<()> {
    let bytes = output
        .len()
        .checked_add(value.len())
        .ok_or_else(|| execution.svg_byte_count_overflow())?;
    execution.preflight_svg_byte_count(bytes)?;
    output.push_str(value);
    Ok(())
}

pub(crate) fn apply_lower_id_suffix_selectors<'a>(
    svg: Cow<'a, str>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<Cow<'a, str>> {
    execution.checkpoint()?;
    if super::util::find_with_checkpoints(&svg, "$=", &mut || execution.checkpoint())?.is_none() {
        return Ok(svg);
    }
    let mut ids = Vec::new();
    let mut styles = Vec::new();
    let mut style = None::<(usize, String)>;
    let mut reader = Reader::from_str(&svg);
    loop {
        execution.checkpoint()?;
        let event_start = reader.buffer_position() as usize;
        match reader.read_event().map_err(error)? {
            Event::Start(tag) => {
                collect_ids(&tag, &mut ids, execution)?;
                if tag.local_name().as_ref() == "style" {
                    style = Some((reader.buffer_position() as usize, String::new()));
                }
            }
            Event::Empty(tag) => collect_ids(&tag, &mut ids, execution)?,
            Event::End(tag) if tag.local_name().as_ref() == "style" => {
                if let Some((start, css)) = style.take() {
                    styles.push((start, event_start, css));
                }
            }
            Event::Text(text) if style.is_some() => {
                style
                    .as_mut()
                    .expect("guarded above")
                    .1
                    .push_str(&text.xml10_content());
            }
            Event::CData(text) if style.is_some() => {
                style
                    .as_mut()
                    .expect("guarded above")
                    .1
                    .push_str(&text.xml10_content());
            }
            Event::GeneralRef(reference) if style.is_some() => {
                let reference = format!("&{};", reference.as_ref());
                style
                    .as_mut()
                    .expect("guarded above")
                    .1
                    .push_str(&quick_xml::escape::unescape(&reference).map_err(error)?);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    ids.sort_unstable();
    ids.dedup();
    let mut output = String::new();
    let mut cursor = 0;
    for (start, end, css) in styles {
        execution.checkpoint()?;
        let lowered = lower_stylesheet(&css, &ids, execution)?;
        if lowered == css {
            continue;
        }
        append(&mut output, &svg[cursor..start], execution)?;
        append(
            &mut output,
            &quick_xml::escape::partial_escape(&lowered),
            execution,
        )?;
        cursor = end;
    }
    if cursor == 0 {
        return Ok(svg);
    }
    append(&mut output, &svg[cursor..], execution)?;
    Ok(Cow::Owned(output))
}

fn collect_ids(
    tag: &quick_xml::events::BytesStart<'_>,
    ids: &mut Vec<String>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    for attribute in tag.attributes() {
        execution.checkpoint()?;
        let attribute = attribute.map_err(error)?;
        if attribute.key.as_ref() == "id" {
            ids.push(
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(error)?
                    .into_owned(),
            );
        }
    }
    Ok(())
}

fn consume_values(
    input: &mut Parser<'_>,
    execution: SvgPostprocessExecution<'_>,
    depth: u8,
) -> std::result::Result<(), cssparser::ParseError<Error>> {
    loop {
        execution
            .checkpoint()
            .map_err(cssparser::ParseError::custom)?;
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(parse_error)
                if matches!(parse_error.kind, cssparser::BasicParseErrorKind::EndOfInput) =>
            {
                return Ok(());
            }
            Err(parse_error) => return Err(parse_error.into()),
        };
        if matches!(
            token,
            Token::Function(_)
                | Token::SquareBracketBlock
                | Token::ParenthesisBlock
                | Token::CurlyBracketBlock
        ) {
            if depth == 64 {
                return Err(cssparser::ParseError::custom(error("CSS nesting limit")));
            }
            input.parse_nested_block(|nested| consume_values(nested, execution, depth + 1))?;
        }
    }
}

fn lower_stylesheet(
    css: &str,
    ids: &[String],
    execution: SvgPostprocessExecution<'_>,
) -> Result<String> {
    let mut input = Parser::new(css);
    let mut rule_start = input.position();
    let mut output = String::new();
    while !input.is_exhausted() {
        execution.checkpoint()?;
        let start = input.position();
        let token = input
            .next_including_whitespace_and_comments()
            .map_err(error)?
            .clone();
        if token == Token::Semicolon {
            append(
                &mut output,
                input.slice(rule_start..input.position()),
                execution,
            )?;
            rule_start = input.position();
            continue;
        }
        if matches!(
            token,
            Token::Function(_) | Token::SquareBracketBlock | Token::ParenthesisBlock
        ) {
            input
                .parse_nested_block(|nested| consume_values(nested, execution, 0))
                .map_err(css_error)?;
            continue;
        }
        if token != Token::CurlyBracketBlock {
            continue;
        }
        let prelude = input.slice(rule_start..start).to_string();
        input
            .parse_nested_block(|body| consume_values(body, execution, 0))
            .map_err(css_error)?;
        let end = input.position();
        let is_at_rule = matches!(Parser::new(&prelude).next(), Ok(Token::AtKeyword(_)));
        // usvg does not evaluate conditional at-rules. Preserve them verbatim;
        // do not reinterpret nested CSS or declaration values as selectors.
        let lowered = if is_at_rule {
            prelude
        } else {
            lower_selector_list(&prelude, ids, execution)?
        };
        if !lowered.trim().is_empty() {
            append(&mut output, &lowered, execution)?;
            append(&mut output, input.slice(start..end), execution)?;
        }
        rule_start = end;
    }
    append(&mut output, input.slice_from(rule_start), execution)?;
    Ok(output)
}

fn lower_selector_list(
    css: &str,
    ids: &[String],
    execution: SvgPostprocessExecution<'_>,
) -> Result<String> {
    let mut input = Parser::new(css);
    let mut output = String::new();
    while !input.is_exhausted() {
        execution.checkpoint()?;
        let selector = input
            .parse_until_before(Delimiter::Comma, |branch| {
                let start = branch.position();
                consume_values(branch, execution, 0)?;
                Ok(branch.slice_from(start).to_string())
            })
            .map_err(css_error)?;
        for expanded in lower_selector(&selector, ids, execution)? {
            if !output.is_empty() {
                append(&mut output, ",", execution)?;
            }
            append(&mut output, &expanded, execution)?;
        }
        if !input.is_exhausted() {
            input.expect_comma().map_err(error)?;
        }
    }
    Ok(output)
}

fn lower_selector(
    css: &str,
    ids: &[String],
    execution: SvgPostprocessExecution<'_>,
) -> Result<Vec<String>> {
    let mut input = Parser::new(css);
    let mut suffix_match = None;
    while !input.is_exhausted() {
        execution.checkpoint()?;
        let start = input.position().byte_index();
        match input
            .next_including_whitespace_and_comments()
            .map_err(error)?
            .clone()
        {
            Token::SquareBracketBlock => {
                let suffix = input
                    .parse_nested_block(|attribute| {
                        if attribute.expect_ident()?.as_ref() != "id" {
                            return Err(cssparser::ParseError::<()>::custom(()));
                        }
                        if *attribute.next()? != Token::SuffixMatch {
                            return Err(cssparser::ParseError::<()>::custom(()));
                        }
                        let suffix = attribute.expect_ident_or_string()?.to_string();
                        attribute.expect_exhausted()?;
                        Ok(suffix)
                    })
                    .ok();
                if let Some(suffix) = suffix {
                    if suffix_match.is_some() {
                        // Mermaid emits one suffix per branch. Avoid a Cartesian
                        // expansion for arbitrary compound user selectors.
                        return Ok(vec![css.to_string()]);
                    }
                    suffix_match = Some((start, input.position().byte_index(), suffix));
                }
            }
            Token::Function(_) | Token::ParenthesisBlock => return Ok(vec![css.to_string()]),
            _ => {}
        }
    }
    let Some((start, end, suffix)) = suffix_match else {
        return Ok(vec![css.to_string()]);
    };
    let mut output = Vec::new();
    let mut projected = 0usize;
    for id in ids {
        execution.checkpoint()?;
        // CSS suffix matching with an empty operand never matches.
        if suffix.is_empty() || !id.ends_with(&suffix) {
            continue;
        }
        let value = Token::QuotedString(id.as_str().into()).to_css_string();
        let bytes = start
            .checked_add(5)
            .and_then(|size| size.checked_add(value.len()))
            .and_then(|size| size.checked_add(css.len() - end))
            .ok_or_else(|| execution.svg_byte_count_overflow())?;
        projected = projected
            .checked_add(bytes)
            .and_then(|size| size.checked_add(1))
            .ok_or_else(|| execution.svg_byte_count_overflow())?;
        execution.preflight_svg_byte_count(projected)?;
        output.push(format!("{}[id={value}]{}", &css[..start], &css[end..]));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::RenderEnvironment;
    use crate::svg::pipeline::{ScopedCssPostprocessor, SvgPipeline};

    fn lower(svg: &str) -> String {
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        apply_lower_id_suffix_selectors(Cow::Borrowed(svg), SvgPostprocessExecution::new(&session))
            .unwrap()
            .into_owned()
    }

    #[test]
    fn expands_actual_ids_and_keeps_selector_context_and_declarations() {
        let svg = r#"<svg><style>defs [id$='-arrowhead'] path,[id$='-missing'],.fallback{fill:transparent!important;stroke:red}</style><g id='b-arrowhead'/><g id='a-arrowhead'/></svg>"#;
        let out = lower(svg);
        let doc = roxmltree::Document::parse(&out).unwrap();
        let css = doc
            .descendants()
            .find(|n| n.has_tag_name("style"))
            .unwrap()
            .text()
            .unwrap();
        assert_eq!(
            css,
            r#"defs [id="a-arrowhead"] path,defs [id="b-arrowhead"] path,.fallback{fill:transparent!important;stroke:red}"#
        );
        assert_eq!(lower(&out), out);
    }

    #[test]
    fn drops_only_unmatched_branches_including_empty_suffix() {
        let out = lower(
            r#"<svg><style>[id$='-missing']{fill:red}[id$=''],.aggregation{fill:transparent}</style></svg>"#,
        );
        assert_eq!(
            out,
            "<svg><style>.aggregation{fill:transparent}</style></svg>"
        );
    }

    #[test]
    fn preserves_other_selectors_and_does_not_rewrite_declaration_strings() {
        let svg = r#"<svg><style>[ID$='-end'],[data-id$='-end'],:is([id$='-end']),[id$='-end' i],[id$='-end'] [id$='-start']{content:'[id$="-end"]'}</style></svg>"#;
        assert_eq!(lower(svg), svg);
    }

    #[test]
    fn handles_cdata_entities_and_escaped_id_values() {
        let svg = r#"<svg><style><![CDATA[[id$='-end']{fill:red}]]></style><path id='a&quot;&amp;-end'/></svg>"#;
        let out = lower(svg);
        let doc = roxmltree::Document::parse(&out).unwrap();
        let css = doc
            .descendants()
            .find(|n| n.has_tag_name("style"))
            .unwrap()
            .text()
            .unwrap();
        assert_eq!(css, r#"[id="a\"&-end"]{fill:red}"#);
    }

    #[test]
    fn only_resvg_safe_lowers_user_styles_after_scoping() {
        let svg = "<svg id='diagram'><path id='diagram-aggregationEnd'/></svg>";
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        let css = r#"[id$="-aggregationEnd"]{fill:transparent!important}"#;
        let parity = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(css))
            .process_to_string(svg, &session)
            .unwrap();
        assert!(parity.contains("$="));
        let readable = SvgPipeline::readable()
            .with_postprocessor(ScopedCssPostprocessor::new(css))
            .process_to_string(svg, &session)
            .unwrap();
        assert!(readable.contains("$="));
        let safe = SvgPipeline::resvg_safe()
            .with_postprocessor(ScopedCssPostprocessor::new(css))
            .process_to_string(svg, &session)
            .unwrap();
        let doc = roxmltree::Document::parse(&safe).unwrap();
        let css = doc
            .descendants()
            .find(|n| n.has_tag_name("style"))
            .unwrap()
            .text()
            .unwrap();
        assert!(
            css.contains(r#"#diagram [id="diagram-aggregationEnd"]"#),
            "{css}"
        );
        assert!(!css.contains("$="));
    }
    #[test]
    fn expansion_obeys_svg_byte_limit() {
        use crate::resources::{RenderResourcePolicy, ResourceLimitId};
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, 64)
            .unwrap();
        let session = RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session()
            .unwrap();
        let ids = vec![
            "very-long-diagram-prefix-aggregationEnd".to_string(),
            "another-long-diagram-prefix-aggregationEnd".to_string(),
        ];
        let result = lower_stylesheet(
            "[id$='-aggregationEnd']{fill:transparent}",
            &ids,
            SvgPostprocessExecution::new(&session),
        );
        let error = result.unwrap_err();
        assert!(error.to_string().contains("max_svg_bytes"), "{error}");
    }

    #[test]
    fn css_walk_preserves_cancellation_provenance() {
        use merman_core::{OperationControl, OperationPhase};
        let control = OperationControl::new();
        let session = RenderEnvironment::deterministic()
            .begin_session_with_control(control.clone())
            .unwrap();
        control.cancel_after_checkpoints(3);
        let error = lower_stylesheet(
            "[id$='-end']{fill:rgb(1,2,3)}",
            &[],
            SvgPostprocessExecution::new(&session),
        )
        .unwrap_err();
        let Error::Cancelled(cancelled) = error else {
            panic!("expected cancellation, got {error}");
        };
        assert_eq!(cancelled.phase, OperationPhase::Postprocess);
    }

    #[test]
    fn resvg_safe_resolves_rebased_marker_ids() {
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        let svg = r#"<svg id="diagram"><style>#diagram [id$="-aggregationEnd"]{fill:transparent}</style><path id="diagram-aggregationEnd"/></svg>"#;
        let out = SvgPipeline::resvg_safe()
            .with_rebased_ids("embed")
            .process_to_string(svg, &session)
            .unwrap();
        let doc = roxmltree::Document::parse(&out).unwrap();
        let id = doc
            .descendants()
            .find(|n| n.has_tag_name("path"))
            .unwrap()
            .attribute("id")
            .unwrap();
        let css = doc
            .descendants()
            .find(|n| n.has_tag_name("style"))
            .unwrap()
            .text()
            .unwrap();
        assert!(css.contains(&format!("[id=\"{id}\"]")), "{css}");
        assert!(!css.contains("$="));
    }
}
