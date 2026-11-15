use crate::{Error, Result};
use std::borrow::Cow;

use super::css_override::{CssOverridePolicy, strip_css_important};
use super::util::{
    checkpoint_loop, find_tag_end_with_checkpoints, find_with_checkpoints, rfind_with_checkpoints,
    trim_with_checkpoints,
};
use crate::svg::pipeline::{SvgPostprocessContext, SvgPostprocessExecution, SvgPostprocessor};

mod rewrite;

const STYLE_OPEN: &str = r#"<style data-merman-postprocess="scoped-css">"#;
const STYLE_CLOSE: &str = "</style>";

#[derive(Debug, Clone)]
pub struct ScopedCssPostprocessor {
    css: String,
    override_policy: CssOverridePolicy,
    merge_into_existing_style: bool,
}

impl ScopedCssPostprocessor {
    pub fn new(css: impl Into<String>) -> Self {
        Self {
            css: css.into(),
            override_policy: CssOverridePolicy::Preserve,
            merge_into_existing_style: false,
        }
    }

    pub fn with_override_policy(mut self, policy: CssOverridePolicy) -> Self {
        self.override_policy = policy;
        self
    }

    pub fn css(&self) -> &str {
        &self.css
    }

    pub fn override_policy(&self) -> CssOverridePolicy {
        self.override_policy
    }

    pub fn with_existing_style_merge(mut self) -> Self {
        self.merge_into_existing_style = true;
        self
    }
}

impl SvgPostprocessor for ScopedCssPostprocessor {
    fn name(&self) -> &'static str {
        "scoped-css"
    }

    fn scoped_css(&self) -> Option<&str> {
        Some(&self.css)
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        ctx: &SvgPostprocessContext<'_>,
    ) -> Result<Cow<'a, str>> {
        let execution = ctx.execution();
        execution.checkpoint()?;
        let mut checkpoint = || execution.checkpoint();
        if trim_with_checkpoints(&self.css, &mut checkpoint)?.is_empty() {
            return Ok(svg);
        }

        let base = match self.override_policy {
            CssOverridePolicy::Preserve => svg,
            CssOverridePolicy::StripExistingImportant => {
                let stripped = strip_css_important(svg.as_ref());
                execution.checkpoint()?;
                Cow::Owned(stripped)
            }
        };
        let css = decode_mermaid_css_hash_placeholders(&self.css, execution)?;
        let scope = css_scope(ctx.svg_id(), execution)?;
        let plan = InjectionPlan::locate(base.as_ref(), self.merge_into_existing_style, execution)?;

        let projected_css_bytes =
            rewrite::projected_css_bytes(css.as_ref(), scope.as_deref(), execution)?;
        let projected_svg_bytes = plan
            .projected_svg_bytes(base.len(), projected_css_bytes)
            .ok_or_else(|| execution.svg_byte_count_overflow())?;
        execution.preflight_svg_byte_count(projected_svg_bytes)?;
        execution.checkpoint()?;

        let mut output = String::new();
        output
            .try_reserve_exact(projected_svg_bytes)
            .map_err(|error| {
                Error::svg_postprocess(
                    "scoped-css",
                    format!("failed to allocate scoped SVG: {error}"),
                )
            })?;
        output.push_str(&base[..plan.insertion]);
        if plan.wrapped {
            output.push_str(STYLE_OPEN);
        }
        rewrite::materialize_css(css.as_ref(), scope.as_deref(), &mut output, execution)?;
        if plan.wrapped {
            output.push_str(STYLE_CLOSE);
        }
        output.push_str(&base[plan.insertion..]);
        execution.checkpoint()?;
        if output.len() != projected_svg_bytes {
            return Err(Error::svg_postprocess(
                "scoped-css",
                "scoped SVG byte projection changed during materialization",
            ));
        }
        Ok(Cow::Owned(output))
    }
}

fn decode_mermaid_css_hash_placeholders<'a>(
    css: &'a str,
    execution: SvgPostprocessExecution<'_>,
) -> Result<Cow<'a, str>> {
    let mut checkpoint = || execution.checkpoint();
    let mut has_placeholder = false;
    for (iteration, character) in css.chars().enumerate() {
        checkpoint_loop(iteration, &mut checkpoint)?;
        if matches!(character, 'ﬂ' | '¶') {
            has_placeholder = true;
            break;
        }
    }
    checkpoint()?;
    if !has_placeholder {
        return Ok(Cow::Borrowed(css));
    }

    let mut decoded = String::new();
    decoded.try_reserve_exact(css.len()).map_err(|error| {
        Error::svg_postprocess(
            "scoped-css",
            format!("failed to allocate decoded scoped CSS: {error}"),
        )
    })?;
    let mut cursor = 0usize;
    let mut iteration = 0usize;
    while cursor < css.len() {
        checkpoint_loop(iteration, &mut checkpoint)?;
        iteration = iteration.wrapping_add(1);
        let tail = &css[cursor..];
        if tail.starts_with("ﬂ°°") {
            decoded.push('#');
            cursor += "ﬂ°°".len();
        } else if tail.starts_with("ﬂ°") {
            decoded.push('#');
            cursor += "ﬂ°".len();
        } else if tail.starts_with("¶ß") {
            decoded.push(';');
            cursor += "¶ß".len();
        } else {
            let character = tail
                .chars()
                .next()
                .expect("cursor remains on a UTF-8 character boundary");
            decoded.push(character);
            cursor += character.len_utf8();
        }
    }
    checkpoint()?;
    Ok(Cow::Owned(decoded))
}

fn css_scope(
    svg_id: Option<&str>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<Option<String>> {
    let Some(svg_id) = svg_id else {
        return Ok(None);
    };
    let mut checkpoint = || execution.checkpoint();
    let svg_id = trim_with_checkpoints(svg_id, &mut checkpoint)?;
    if svg_id.is_empty() {
        return Ok(None);
    }

    let mut checkpoint = || execution.checkpoint();
    let mut escaped_bytes = 1usize;
    for (iteration, character) in svg_id.chars().enumerate() {
        checkpoint_loop(iteration, &mut checkpoint)?;
        escaped_bytes = escaped_bytes
            .checked_add(if is_css_identifier_character(character) {
                character.len_utf8()
            } else {
                1 + character.len_utf8()
            })
            .ok_or_else(|| execution.svg_byte_count_overflow())?;
    }
    checkpoint()?;
    let mut scope = String::new();
    scope.try_reserve_exact(escaped_bytes).map_err(|error| {
        Error::svg_postprocess(
            "scoped-css",
            format!("failed to allocate scoped CSS root selector: {error}"),
        )
    })?;
    scope.push('#');
    for (iteration, character) in svg_id.chars().enumerate() {
        checkpoint_loop(iteration, &mut checkpoint)?;
        if !is_css_identifier_character(character) {
            scope.push('\\');
        }
        scope.push(character);
    }
    checkpoint()?;
    Ok(Some(scope))
}

fn is_css_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-' || character == '_'
}

#[derive(Debug, Clone, Copy)]
struct InjectionPlan {
    insertion: usize,
    wrapped: bool,
}

impl InjectionPlan {
    fn locate(
        svg: &str,
        merge_into_existing_style: bool,
        execution: SvgPostprocessExecution<'_>,
    ) -> Result<Self> {
        let mut checkpoint = || execution.checkpoint();
        if merge_into_existing_style
            && let Some(insertion) = find_with_checkpoints(svg, "</style", &mut checkpoint)?
        {
            return Ok(Self {
                insertion,
                wrapped: false,
            });
        }

        if let Some(root_start) = find_with_checkpoints(svg, "<svg", &mut checkpoint)?
            && let Some(root_end) = find_tag_end_with_checkpoints(svg, root_start, &mut checkpoint)?
        {
            if let Some(style_start) = rfind_with_checkpoints(svg, "</style", &mut checkpoint)?
                && let Some(style_end) =
                    find_tag_end_with_checkpoints(svg, style_start, &mut checkpoint)?
            {
                return Ok(Self {
                    insertion: style_end + 1,
                    wrapped: true,
                });
            }
            return Ok(Self {
                insertion: root_end + 1,
                wrapped: true,
            });
        }

        Ok(Self {
            insertion: svg.len(),
            wrapped: true,
        })
    }

    fn projected_svg_bytes(self, svg_bytes: usize, css_bytes: usize) -> Option<usize> {
        let wrapper_bytes = if self.wrapped {
            STYLE_OPEN.len() + STYLE_CLOSE.len()
        } else {
            0
        };
        svg_bytes.checked_add(css_bytes)?.checked_add(wrapper_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::RenderEnvironment;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId, ResourceLimitPhase};
    use crate::svg::pipeline::{SvgPipeline, SvgPipelinePreset, SvgPostprocessMetadata};
    use merman_core::{CancelReason, OperationControl, OperationPhase};

    fn render_session() -> crate::environment::RenderSession {
        crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap()
    }

    #[test]
    fn scoped_css_injects_after_root_svg_tag_when_no_style_exists() {
        let svg = r#"<svg id="diagram"><rect class="node"/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(
                ".node rect, text.label { fill: red; }",
            ))
            .process_to_string(svg, &session)
            .unwrap();

        assert!(out.starts_with(r#"<svg id="diagram"><style"#));
        assert!(out.contains("#diagram .node rect, #diagram text.label { fill: red; }"));
    }

    #[test]
    fn scoped_css_injects_after_existing_style_for_cascade_order() {
        let svg =
            r#"<svg id="diagram"><style>#diagram .node rect { fill: red; }</style><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(".node rect { fill: green; }"))
            .process_to_string(svg, &session)
            .unwrap();

        let existing = out.find("fill: red").unwrap();
        let injected = out.find("fill: green").unwrap();
        assert!(
            existing < injected,
            "injected CSS should follow Mermaid CSS for cascade order: {out}"
        );
    }

    #[test]
    fn scoped_css_can_merge_into_mermaid_generated_stylesheet() {
        let svg = r#"<svg id="diagram"><style>#diagram{fill:red;}</style><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(
                ScopedCssPostprocessor::new(".node { fill: green; }").with_existing_style_merge(),
            )
            .process_to_string(svg, &session)
            .unwrap();

        assert_eq!(out.matches("<style").count(), 1);
        assert!(out.contains("#diagram{fill:red;}#diagram .node { fill: green; }</style>"));
        assert!(!out.contains("data-merman-postprocess"));
    }

    #[test]
    fn scoped_css_merge_targets_mermaids_first_global_stylesheet() {
        let svg = r#"<svg id="diagram"><style>.global{fill:red;}</style><g><style>.nested{fill:blue;}</style></g></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(
                ScopedCssPostprocessor::new(".node { fill: green; }").with_existing_style_merge(),
            )
            .process_to_string(svg, &session)
            .unwrap();

        assert!(out.contains(".global{fill:red;}#diagram .node { fill: green; }</style>"));
        assert!(out.contains("<style>.nested{fill:blue;}</style>"));
    }

    #[test]
    fn scoped_css_can_strip_existing_important_before_injection() {
        let svg = r#"<svg id="diagram"><style>.node{fill:red !important;}</style></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(
                ScopedCssPostprocessor::new(".node { fill: green; }")
                    .with_override_policy(CssOverridePolicy::StripExistingImportant),
            )
            .process_to_string(svg, &session)
            .unwrap();

        assert!(!out.contains("!important"));
        assert!(out.contains("#diagram .node { fill: green; }"));
    }

    #[test]
    fn scoped_css_matches_mermaid_ampersand_selector_namespace() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(
                ":not(&){background:green !important}",
            ))
            .process_to_string(svg, &session)
            .unwrap();

        assert!(out.contains("#diagram :not(#diagram) {background:green !important}"));
    }

    #[test]
    fn scoped_css_matches_mermaid_namespace_boundary_rules() {
        fn normalize_css_whitespace(value: &str) -> String {
            let mut normalized = String::with_capacity(value.len());
            let mut pending_space = false;
            for character in value.chars() {
                if matches!(character, ' ' | '\n' | '\r' | '\t' | '\u{000C}') {
                    pending_space = true;
                    continue;
                }
                if pending_space && !normalized.is_empty() {
                    normalized.push(' ');
                }
                pending_space = false;
                normalized.push(character);
            }
            normalized
        }

        let cases = [
            ("& ~ *", "color: red;", "#diagram #diagram ~ *"),
            (
                "& \n\t \r \u{000C} \r\n + *",
                "color: red;",
                "#diagram #diagram \n\t \r \u{000C} \r\n + *",
            ),
            ("& || *", "color: red;", "#diagram #diagram || *"),
            ("&", "color: red;", "#diagram #diagram"),
            (
                "&",
                "font-family: serif; font-size: 12px; fill: red;",
                "#diagram",
            ),
            ("#diagram", "color: red;", "#diagram #diagram"),
            (
                "#diagram",
                "font-family: serif; font-size: 12px; fill: red;",
                "#diagram",
            ),
            ("& > *", "color: red;", "#diagram > *"),
            ("& *", "color: red;", "#diagram *"),
        ];

        for (selector, body, expected) in cases {
            let css = format!("{selector}{{{body}}}");
            let svg = r#"<svg id="diagram"><g/></svg>"#;
            let session = render_session();
            let out = SvgPipeline::parity()
                .with_postprocessor(ScopedCssPostprocessor::new(css))
                .process_to_string(svg, &session)
                .unwrap();
            let normalized_output = normalize_css_whitespace(&out);
            let normalized_expected = normalize_css_whitespace(&format!("{expected} {{{body}}}"));
            assert!(
                normalized_output.contains(&normalized_expected),
                "selector: {selector:?}; output: {out}"
            );
        }
    }

    #[test]
    fn scoped_css_scopes_nested_grouping_at_rules_and_drops_unsupported_rules() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(
                "@layer theme; @import url('https://example.test/styles.css'); @media (max-width: 600px) { * { fill: red; } } @supports selector(h2 > p) { h2 > p { color: red; } }",
            ))
            .process_to_string(svg, &session)
            .unwrap();

        assert!(out.contains("@layer theme;"));
        assert!(!out.contains("@import"));
        assert!(out.contains("@media (max-width: 600px) {"));
        assert!(out.contains("#diagram * { fill: red; }"));
        assert!(out.contains("@supports selector(h2 > p) {"));
        assert!(out.contains("#diagram h2 > p { color: red; }"));
    }

    #[test]
    fn scoped_css_keeps_keyframes_unscoped_like_mermaid() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(
                "@keyframes dash { to { stroke-dashoffset: 1000; } } .edge { animation: dash 1s; }",
            ))
            .process_to_string(svg, &session)
            .unwrap();

        assert!(out.contains("@keyframes dash { to { stroke-dashoffset: 1000; } }"));
        assert!(out.contains("#diagram .edge { animation: dash 1s; }"));
    }

    #[test]
    fn scoped_css_decodes_mermaid_hash_placeholders_as_css_hashes() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(".node { fill: ﬂ°°123456¶ß }"))
            .process_to_string(svg, &session)
            .unwrap();

        assert!(out.contains("#diagram .node { fill: #123456; }"));
    }

    #[test]
    fn scoped_css_uses_css_tokens_for_braces_and_selector_commas() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(
                r#"/* { } */ svg:not([data-x="{"]) { content: "}"; opacity: 0 } .node:is(.a, .b) { fill: red }"#,
            ))
            .process_to_string(svg, &session)
            .unwrap();

        assert!(
            out.contains(r#"#diagram svg:not([data-x="{"]) { content: "}"; opacity: 0 }"#),
            "{out}"
        );
        assert!(
            out.contains("#diagram .node:is(.a, .b) { fill: red }"),
            "{out}"
        );
    }

    #[test]
    fn scoped_css_escapes_style_terminators_after_tokenized_rewrite() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let out = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(
                r#".node { content: "</style"; }"#,
            ))
            .process_to_string(svg, &session)
            .unwrap();

        assert!(out.contains(r#"content: "\3c /style";"#), "{out}");
        assert_eq!(out.matches("</style>").count(), 1, "{out}");
    }

    #[test]
    fn scoped_css_rejects_unclosed_rule_before_injection() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let error = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(
                r#".node { content: "{"; fill: red;"#,
            ))
            .process_to_string(svg, &session)
            .unwrap_err();

        assert!(error.to_string().contains("unclosed"), "{error}");
    }

    #[test]
    fn scoped_css_rejects_excessive_rule_nesting() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let session = render_session();
        let css = format!(
            "{} .node {{ fill: red; }} {}",
            "@media all {".repeat(64),
            "}".repeat(64)
        );
        let error = SvgPipeline::parity()
            .with_postprocessor(ScopedCssPostprocessor::new(css))
            .process_to_string(svg, &session)
            .unwrap_err();

        assert!(error.to_string().contains("nesting"), "{error}");
    }

    #[test]
    fn scoped_css_preflights_exact_projected_bytes_before_materialization() {
        let svg = r#"<svg id="diagram"><g/></svg>"#;
        let processor =
            ScopedCssPostprocessor::new(".node, .edge, .label { fill: red; stroke: blue; }");
        let metadata = SvgPostprocessMetadata::from_svg(svg);
        let unbounded_session = RenderEnvironment::deterministic()
            .with_resource_policy(RenderResourcePolicy::unbounded_for_trusted_input())
            .begin_session()
            .unwrap();
        let unbounded_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "scoped-css",
            &metadata,
            &unbounded_session,
        );
        let projected_bytes = processor
            .process(Cow::Borrowed(svg), &unbounded_context)
            .expect("unbounded scoped CSS should materialize")
            .len();

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, projected_bytes)
            .unwrap();
        let exact_session = RenderEnvironment::deterministic()
            .with_resource_policy(exact_policy)
            .begin_session()
            .unwrap();
        let exact_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "scoped-css",
            &metadata,
            &exact_session,
        );
        assert_eq!(
            processor
                .process(Cow::Borrowed(svg), &exact_context)
                .expect("the exact SVG byte limit should admit scoped CSS")
                .len(),
            projected_bytes
        );

        let limited_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, projected_bytes - 1)
            .unwrap();
        let limited_session = RenderEnvironment::deterministic()
            .with_resource_policy(limited_policy)
            .begin_session()
            .unwrap();
        let limited_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "scoped-css",
            &metadata,
            &limited_session,
        );
        let error = processor
            .process(Cow::Borrowed(svg), &limited_context)
            .unwrap_err();
        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection, got {error}");
        };
        assert_eq!(details.phase, ResourceLimitPhase::SvgPostprocess);
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.actual, projected_bytes);
        assert_eq!(details.max, projected_bytes - 1);
    }

    #[test]
    fn scoped_css_token_walk_observes_mid_stream_cancellation() {
        let control = OperationControl::new();
        let session = RenderEnvironment::deterministic()
            .with_resource_policy(RenderResourcePolicy::unbounded_for_trusted_input())
            .begin_session_with_control(control.clone())
            .unwrap();
        let css = ".node { fill: red; }".repeat(512);
        control.cancel_after_checkpoints(2);

        let error = rewrite::projected_css_bytes(
            &css,
            Some("#diagram"),
            SvgPostprocessExecution::new(&session),
        )
        .expect_err("the CSS token walk must observe cancellation before completion");
        let Error::Cancelled(cancelled) = error else {
            panic!("expected structured cancellation, got {error}");
        };
        assert_eq!(cancelled.phase, OperationPhase::Postprocess);
        assert_eq!(cancelled.reason, CancelReason::Requested);
    }
}
