use super::IconRenderRequest;
use super::ingest::ResolvedIcon;
use super::xml::ValidatedIconBody;
use crate::svg::pipeline::validate_well_formed_svg_with_controls;
use merman_core::OperationPhase;
use merman_core::sanitize::{SanitizeFailure, SanitizeOutputSink};

const WORK_BYTES_PER_UNIT: usize = 256;
// lol_html serializes a literal double quote in an attribute value as `&quot;` (six bytes).
// The element allowance covers handler-added attributes and normalized closing tags without
// relying on an empirical whole-document multiplier.
const SANITIZER_MAX_BYTE_EXPANSION: usize = "&quot;".len();
const SANITIZER_MAX_ADDED_BYTES_PER_ELEMENT: usize = 64;
const SANITIZER_FIXED_OVERHEAD_BYTES: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundedSanitizeError {
    OutputLimitExceeded { max_bytes: usize },
    AllocationFailed,
}

#[derive(Debug, Clone, Copy)]
struct BoundedSanitizeSink {
    max_output_bytes: usize,
}

impl BoundedSanitizeSink {
    const fn new(max_output_bytes: usize) -> Self {
        Self { max_output_bytes }
    }

    const fn output_limit_error(self) -> BoundedSanitizeError {
        BoundedSanitizeError::OutputLimitExceeded {
            max_bytes: self.max_output_bytes,
        }
    }
}

impl SanitizeOutputSink for BoundedSanitizeSink {
    type Error = BoundedSanitizeError;

    fn checked_output_len(&self, current: usize, additional: usize) -> Result<usize, Self::Error> {
        let next = current
            .checked_add(additional)
            .ok_or_else(|| self.output_limit_error())?;
        if next > self.max_output_bytes {
            return Err(self.output_limit_error());
        }
        Ok(next)
    }

    fn string_with_capacity(&self, capacity: usize) -> Result<String, Self::Error> {
        self.checked_output_len(0, capacity)?;
        let mut output = String::new();
        output
            .try_reserve_exact(capacity)
            .map_err(|_| BoundedSanitizeError::AllocationFailed)?;
        Ok(output)
    }

    fn output_buffer(&self, input_len: usize) -> Result<Vec<u8>, Self::Error> {
        let mut output = Vec::new();
        output
            .try_reserve_exact(input_len.min(self.max_output_bytes))
            .map_err(|_| BoundedSanitizeError::AllocationFailed)?;
        Ok(output)
    }

    fn push_output_chunk(&self, output: &mut Vec<u8>, chunk: &[u8]) -> Result<(), Self::Error> {
        self.checked_output_len(output.len(), chunk.len())?;
        output
            .try_reserve(chunk.len())
            .map_err(|_| BoundedSanitizeError::AllocationFailed)?;
        output.extend_from_slice(chunk);
        Ok(())
    }
}

pub(super) fn render_resolved_icon(
    icon: &ResolvedIcon,
    request: &IconRenderRequest<'_>,
) -> crate::Result<String> {
    let width = render_dimension(request.width_px, "width")?;
    let height = render_dimension(request.height_px, "height")?;
    let transformed = transformed_icon(icon)?;
    let class_attr = request
        .extra_class
        .map(|class| format!(r#" class="{}""#, escape_xml_attr(class)))
        .unwrap_or_default();
    let xlink_attr = if icon.body.uses_xlink() {
        r#" xmlns:xlink="http://www.w3.org/1999/xlink""#
    } else {
        ""
    };
    let open = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"{xlink_attr}{class_attr} width="{width}" height="{height}" viewBox="{} {} {} {}">"#,
        js_number(transformed.left),
        js_number(transformed.top),
        js_number(transformed.width),
        js_number(transformed.height),
    );
    let projected_body_bytes = transformed.projected_body_bytes(&icon.body)?;
    let projected_svg_bytes = open
        .len()
        .checked_add(projected_body_bytes)
        .and_then(|bytes| bytes.checked_add("</svg>".len()))
        .ok_or_else(|| crate::Error::icon_processing("projected icon SVG size overflowed"))?;
    let sanitizer_ceiling =
        sanitizer_output_ceiling(projected_svg_bytes, icon.body.element_count())?;
    let work_units = icon_render_work_units(icon, projected_svg_bytes, sanitizer_ceiling)?;

    // Both ledgers are charged before cloning the retained body or allocating the assembled SVG.
    request.work_meter.charge(work_units)?;
    request.work_meter.charge_svg_bytes(projected_svg_bytes)?;

    let body = transformed.apply(&icon.body, request.id_scope)?;
    let mut raw_svg = String::with_capacity(projected_svg_bytes);
    raw_svg.push_str(&open);
    raw_svg.push_str(&body);
    raw_svg.push_str("</svg>");
    if raw_svg.len() > projected_svg_bytes {
        return Err(crate::Error::icon_processing(
            "assembled icon SVG exceeded its precharged size",
        ));
    }

    // Mermaid performs sanitization after transforms, deterministic ID replacement, and outer SVG
    // assembly. The effective request config must therefore be applied on every render.
    let requested_growth = sanitizer_ceiling - projected_svg_bytes;
    let growth_reservation = request
        .work_meter
        .reserve_svg_bytes_up_to(requested_growth)?;
    let reserved_svg_bytes = projected_svg_bytes
        .checked_add(growth_reservation.additional_bytes)
        .ok_or_else(|| crate::Error::icon_processing("sanitizer reservation overflowed"))?;
    let sanitize_sink = BoundedSanitizeSink::new(reserved_svg_bytes);
    let sanitized = match merman_core::sanitize::sanitize_text_with_sink(
        &raw_svg,
        request.effective_config,
        &sanitize_sink,
    ) {
        Ok(sanitized) => sanitized,
        Err(SanitizeFailure::Output(BoundedSanitizeError::OutputLimitExceeded { .. })) => {
            request
                .work_meter
                .reconcile_svg_bytes(reserved_svg_bytes, 0)?;
            if let Some(error) = growth_reservation.limit_error {
                return Err(request
                    .work_meter
                    .terminate_absolute_resource_error(error, OperationPhase::Emit)
                    .into());
            }
            return Err(crate::Error::icon_processing(
                "icon SVG sanitizer exceeded its fixed expansion ceiling",
            ));
        }
        Err(SanitizeFailure::RejectedInput) => {
            request
                .work_meter
                .reconcile_svg_bytes(reserved_svg_bytes, 0)?;
            return Err(crate::Error::invalid_icon_output(
                "icon SVG sanitizer rejected the output",
            ));
        }
        Err(
            SanitizeFailure::Output(BoundedSanitizeError::AllocationFailed)
            | SanitizeFailure::InvalidUtf8Output,
        ) => {
            request
                .work_meter
                .reconcile_svg_bytes(reserved_svg_bytes, 0)?;
            return Err(crate::Error::icon_processing(
                "icon SVG sanitizer failed internally",
            ));
        }
        Err(_) => {
            request
                .work_meter
                .reconcile_svg_bytes(reserved_svg_bytes, 0)?;
            return Err(crate::Error::icon_processing(
                "icon SVG sanitizer returned an unknown failure",
            ));
        }
    };
    if sanitized.is_empty() {
        request
            .work_meter
            .reconcile_svg_bytes(reserved_svg_bytes, 0)?;
        return Err(crate::Error::invalid_icon_output(
            "sanitization removed the complete icon SVG",
        ));
    }
    let validation = validate_well_formed_svg_with_controls(
        &sanitized,
        &mut || {
            request
                .work_meter
                .checkpoint(OperationPhase::Postprocess)
                .map_err(crate::Error::from)
        },
        &mut |elements, tree_depth| {
            request
                .work_meter
                .preflight_svg_structure(elements, tree_depth, OperationPhase::Postprocess)
                .map_err(crate::Error::from)
        },
    );
    if let Err(error) = validation {
        if matches!(
            &error,
            crate::Error::Cancelled(_)
                | crate::Error::ResourceLimitExceeded(_)
                | crate::Error::OperationResourceTerminal(_)
        ) {
            return Err(error);
        }
        request.work_meter.checkpoint(OperationPhase::Postprocess)?;
        request
            .work_meter
            .reconcile_svg_bytes(reserved_svg_bytes, 0)?;
        return Err(crate::Error::icon_processing(
            "sanitizer produced invalid SVG XML",
        ));
    }
    request
        .work_meter
        .reconcile_svg_bytes(reserved_svg_bytes, sanitized.len())?;
    Ok(sanitized)
}

fn sanitizer_output_ceiling(
    projected_svg_bytes: usize,
    element_count: usize,
) -> crate::Result<usize> {
    projected_svg_bytes
        .checked_mul(SANITIZER_MAX_BYTE_EXPANSION)
        .and_then(|bytes| {
            element_count
                .checked_mul(SANITIZER_MAX_ADDED_BYTES_PER_ELEMENT)
                .and_then(|overhead| bytes.checked_add(overhead))
        })
        .and_then(|bytes| bytes.checked_add(SANITIZER_FIXED_OVERHEAD_BYTES))
        .ok_or_else(|| crate::Error::icon_processing("sanitizer output limit overflowed"))
}

fn icon_render_work_units(
    icon: &ResolvedIcon,
    projected_svg_bytes: usize,
    sanitizer_ceiling: usize,
) -> crate::Result<usize> {
    let edit_work = icon
        .body
        .edit_count()
        .checked_mul(4)
        .ok_or_else(|| crate::Error::icon_processing("icon edit work estimate overflowed"))?;
    let projected_work = ceil_div(projected_svg_bytes, WORK_BYTES_PER_UNIT);
    let sanitizer_work = ceil_div(sanitizer_ceiling, WORK_BYTES_PER_UNIT);
    let element_work = icon.body.element_count().checked_mul(3).ok_or_else(|| {
        crate::Error::icon_processing("icon sanitizer element work estimate overflowed")
    })?;
    ceil_div(icon.body.source_len(), WORK_BYTES_PER_UNIT)
        // Assembly plus the first sanitizer input traversal.
        .checked_add(projected_work.checked_mul(2).ok_or_else(|| {
            crate::Error::icon_processing("icon projected work estimate overflowed")
        })?)
        // The second sanitizer pass and final XML validation are bounded by the admitted output.
        .and_then(|work| work.checked_add(sanitizer_work.checked_mul(2)?))
        .and_then(|work| work.checked_add(element_work))
        .and_then(|work| work.checked_add(edit_work))
        .and_then(|work| work.checked_add(1))
        .ok_or_else(|| crate::Error::icon_processing("icon render work estimate overflowed"))
}

const fn ceil_div(value: usize, divisor: usize) -> usize {
    value / divisor + if value.is_multiple_of(divisor) { 0 } else { 1 }
}

fn render_dimension(value: f64, name: &'static str) -> crate::Result<String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(crate::Error::invalid_icon_output(format!(
            "requested icon {name} is not finite and positive"
        )));
    }
    Ok(js_number(value))
}

struct TransformedIcon {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    wrapper: Option<(String, &'static str)>,
}

impl TransformedIcon {
    fn projected_body_bytes(&self, body: &ValidatedIconBody) -> crate::Result<usize> {
        let Some((start, end)) = &self.wrapper else {
            return Ok(body.scoped_len());
        };
        body.transformed_scoped_len(start.len(), end.len())
            .map_err(|_| crate::Error::icon_processing("transformed icon size overflowed"))
    }

    fn apply(&self, body: &ValidatedIconBody, scope: super::IconIdScope) -> crate::Result<String> {
        let Some((start, end)) = &self.wrapper else {
            return body
                .scope(scope)
                .map_err(|_| crate::Error::icon_processing("validated icon ID scoping failed"));
        };
        body.scope_transformed(scope, start, end)
            .map_err(|_| crate::Error::icon_processing("validated icon transform assembly failed"))
    }
}

fn transformed_icon(icon: &ResolvedIcon) -> crate::Result<TransformedIcon> {
    let mut left = icon.left;
    let mut top = icon.top;
    let mut width = icon.width;
    let mut height = icon.height;
    let mut transformations = Vec::with_capacity(3);
    let mut rotation = i32::from(icon.rotate);

    if icon.h_flip {
        if icon.v_flip {
            rotation += 2;
        } else {
            transformations.push(format!(
                "translate({} {})",
                js_number(width + left),
                js_number(-top)
            ));
            transformations.push("scale(-1 1)".to_string());
            left = 0.0;
            top = 0.0;
        }
    } else if icon.v_flip {
        transformations.push(format!(
            "translate({} {})",
            js_number(-left),
            js_number(height + top)
        ));
        transformations.push("scale(1 -1)".to_string());
        left = 0.0;
        top = 0.0;
    }

    rotation = rotation.rem_euclid(4);
    match rotation {
        1 => {
            let center = height / 2.0 + top;
            transformations.insert(
                0,
                format!("rotate(90 {} {})", js_number(center), js_number(center)),
            );
        }
        2 => transformations.insert(
            0,
            format!(
                "rotate(180 {} {})",
                js_number(width / 2.0 + left),
                js_number(height / 2.0 + top)
            ),
        ),
        3 => {
            let center = width / 2.0 + left;
            transformations.insert(
                0,
                format!("rotate(-90 {} {})", js_number(center), js_number(center)),
            );
        }
        _ => {}
    }
    if rotation % 2 == 1 {
        std::mem::swap(&mut left, &mut top);
        std::mem::swap(&mut width, &mut height);
    }

    for value in [left, top, width, height] {
        if !value.is_finite() {
            return Err(crate::Error::invalid_icon_output(
                "icon transformation produced non-finite geometry",
            ));
        }
    }
    let wrapper = (!transformations.is_empty()).then(|| {
        (
            format!(r#"<g transform="{}">"#, transformations.join(" ")),
            "</g>",
        )
    });
    Ok(TransformedIcon {
        left,
        top,
        width,
        height,
        wrapper,
    })
}

fn escape_xml_attr(value: &str) -> String {
    crate::xml::strip_forbidden_xml_1_0_chars(value)
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn js_number(mut value: f64) -> String {
    if value == -0.0 {
        value = 0.0;
    }
    let mut buffer = ryu_js::Buffer::new();
    buffer.format_finite(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy, ResourceLimitId};
    use crate::svg::icon_registry::{icon_id_scope_for_test, limits::IconRegistryBuildLimits};
    use serde_json::json;
    use std::sync::Arc;

    #[derive(Debug, Clone, Copy)]
    struct InvalidUtf8Sink(BoundedSanitizeSink);

    impl SanitizeOutputSink for InvalidUtf8Sink {
        type Error = BoundedSanitizeError;

        fn checked_output_len(
            &self,
            current: usize,
            additional: usize,
        ) -> Result<usize, Self::Error> {
            self.0.checked_output_len(current, additional)
        }

        fn string_with_capacity(&self, capacity: usize) -> Result<String, Self::Error> {
            self.0.string_with_capacity(capacity)
        }

        fn output_buffer(&self, input_len: usize) -> Result<Vec<u8>, Self::Error> {
            self.0.output_buffer(input_len)
        }

        fn push_output_chunk(&self, output: &mut Vec<u8>, chunk: &[u8]) -> Result<(), Self::Error> {
            self.0.push_output_chunk(output, chunk)?;
            if let Some(first) = output.first_mut() {
                *first = u8::MAX;
            }
            Ok(())
        }
    }

    fn resolved_icon(body: &str) -> ResolvedIcon {
        ResolvedIcon {
            body: Arc::new(
                ValidatedIconBody::parse(body.to_owned(), 0, &IconRegistryBuildLimits::fixed())
                    .expect("test icon body must be admitted"),
            ),
            left: 0.0,
            top: 0.0,
            width: 16.0,
            height: 16.0,
            h_flip: false,
            v_flip: false,
            rotate: 0,
        }
    }

    fn render(
        body: &str,
        security_level: &str,
        policy: RenderResourcePolicy,
    ) -> crate::Result<String> {
        render_with_work_usage(body, security_level, policy).0
    }

    fn render_with_work_usage(
        body: &str,
        security_level: &str,
        policy: RenderResourcePolicy,
    ) -> (crate::Result<String>, usize) {
        let icon = resolved_icon(body);
        let config = merman_core::MermaidConfig::from_value(json!({
            "securityLevel": security_level,
            "htmlLabels": true
        }));
        let work_meter = OperationWorkMeter::new(policy);
        let result = render_resolved_icon(
            &icon,
            &IconRenderRequest {
                icon_name: "test:icon",
                width_px: 16.0,
                height_px: 16.0,
                fallback_prefix: None,
                extra_class: None,
                id_scope: icon_id_scope_for_test("icon-scope"),
                effective_config: &config,
                work_meter: &work_meter,
            },
        );
        (result, work_meter.used())
    }

    fn render_repeated_with_usage(
        body: &str,
        security_level: &str,
        repetitions: usize,
        policy: RenderResourcePolicy,
    ) -> (crate::Result<Vec<String>>, usize, usize) {
        let icon = resolved_icon(body);
        let config = merman_core::MermaidConfig::from_value(json!({
            "securityLevel": security_level,
            "htmlLabels": true
        }));
        let work_meter = OperationWorkMeter::new(policy);
        let mut rendered = Vec::with_capacity(repetitions);
        let result = (|| {
            for index in 0..repetitions {
                let scope = format!("icon-scope-{index}");
                rendered.push(render_resolved_icon(
                    &icon,
                    &IconRenderRequest {
                        icon_name: "test:icon",
                        width_px: 16.0,
                        height_px: 16.0,
                        fallback_prefix: None,
                        extra_class: None,
                        id_scope: icon_id_scope_for_test(&scope),
                        effective_config: &config,
                        work_meter: &work_meter,
                    },
                )?);
            }
            Ok(rendered)
        })();
        (result, work_meter.used(), work_meter.projected_svg_bytes())
    }

    #[test]
    fn ambiguous_icon_markup_is_rejected_in_strict_and_loose_modes() {
        let body = "<select><xmp><script>alert(1)</script></xmp></select>";
        for security_level in ["strict", "loose"] {
            let error = render(
                body,
                security_level,
                RenderResourcePolicy::unbounded_for_trusted_input(),
            )
            .expect_err("ambiguous sanitizer input must fail closed");
            assert!(matches!(error, crate::Error::InvalidIconOutput { .. }));
        }
    }

    #[test]
    fn bounded_sanitizer_accepts_exact_output_limit_and_rejects_one_less() {
        let input = r#"<a href="https://example.com" target="_blank">example</a>"#;
        let expected =
            r#"<a href="https://example.com" rel="noopener" target="_blank">example</a>"#;
        let config = merman_core::MermaidConfig::from_value(json!({
            "securityLevel": "strict",
            "htmlLabels": true
        }));

        assert_eq!(
            merman_core::sanitize::sanitize_text_with_sink(
                input,
                &config,
                &BoundedSanitizeSink::new(expected.len()),
            )
            .unwrap(),
            expected
        );
        assert_eq!(
            merman_core::sanitize::sanitize_text_with_sink(
                input,
                &config,
                &BoundedSanitizeSink::new(expected.len() - 1),
            ),
            Err(SanitizeFailure::Output(
                BoundedSanitizeError::OutputLimitExceeded {
                    max_bytes: expected.len() - 1,
                },
            ))
        );
    }

    #[test]
    fn bounded_sanitizer_classifies_allocation_and_invalid_utf8_failures() {
        assert_eq!(
            BoundedSanitizeSink::new(usize::MAX).string_with_capacity(usize::MAX),
            Err(BoundedSanitizeError::AllocationFailed)
        );

        let config = merman_core::MermaidConfig::from_value(json!({
            "securityLevel": "strict",
            "htmlLabels": true
        }));
        assert_eq!(
            merman_core::sanitize::sanitize_text_with_sink(
                "<b>ok</b>",
                &config,
                &InvalidUtf8Sink(BoundedSanitizeSink::new(4096)),
            ),
            Err(SanitizeFailure::InvalidUtf8Output)
        );
    }

    #[test]
    fn active_external_icon_content_is_sanitized_in_strict_and_loose_modes() {
        let cases = [
            ("script", "<script>alert(1)</script><path d=\"M0 0h1v1z\"/>"),
            ("event", "<path onload=\"alert(1)\" d=\"M0 0h1v1z\"/>"),
            (
                "style",
                "<style>.owned{fill:red}</style><path class=\"owned\" d=\"M0 0h1v1z\"/>",
            ),
            (
                "foreign-object",
                "<foreignObject><div onclick=\"alert(1)\">owned</div></foreignObject><path d=\"M0 0h1v1z\"/>",
            ),
            (
                "href",
                "<a href=\"javascript:alert(1)\"><path d=\"M0 0h1v1z\"/></a>",
            ),
            (
                "xlink-href",
                "<a xmlns:xlink=\"http://www.w3.org/1999/xlink\" xlink:href=\"javascript:alert(1)\"><path d=\"M0 0h1v1z\"/></a>",
            ),
        ];

        for security_level in ["strict", "loose"] {
            for (name, body) in cases {
                let rendered = render(
                    body,
                    security_level,
                    RenderResourcePolicy::unbounded_for_trusted_input(),
                )
                .unwrap_or_else(|error| {
                    panic!("{name} must sanitize safely in {security_level} mode: {error}")
                });
                let lower = rendered.to_ascii_lowercase();
                for forbidden in [
                    "<script",
                    "onload=",
                    "onclick=",
                    "<style",
                    "<foreignobject",
                    "javascript:",
                    "xlink:href=",
                ] {
                    assert!(
                        !lower.contains(forbidden),
                        "{name} retained {forbidden:?} in {security_level} mode: {rendered}"
                    );
                }
                assert!(lower.starts_with("<svg"), "{rendered}");
                assert!(lower.ends_with("</svg>"), "{rendered}");
            }
        }
    }

    #[test]
    fn safe_sanitizer_growth_is_accounted_at_the_svg_limit() {
        let body = r#"<a href="https://example.com" target="_blank"><path d="M0 0h16v16H0z"/></a>"#;
        let rendered = render(
            body,
            "strict",
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )
        .expect("safe sanitizer growth is accepted");
        assert!(rendered.contains(r#"rel="noopener""#));

        let exact = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, rendered.len())
            .unwrap();
        assert_eq!(render(body, "strict", exact).unwrap(), rendered);

        let one_less = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, rendered.len() - 1)
            .unwrap();
        let error = render(body, "strict", one_less)
            .expect_err("sanitized output one byte above the SVG budget must be rejected");
        assert!(matches!(error, crate::Error::ResourceLimitExceeded(_)));
    }

    #[test]
    fn icon_xml_validation_preserves_resource_error_classification() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgElements, 1)
            .unwrap();

        let error = render("<path d=\"M0 0h16v16H0z\"/>", "strict", policy)
            .expect_err("the generated svg and path exceed the one-element validation limit");

        assert!(matches!(error, crate::Error::ResourceLimitExceeded(_)));
    }

    #[test]
    fn sanitizer_ceiling_covers_worst_case_attribute_quote_serialization() {
        let quote_count = 70 * 1024;
        let quotes = "\"".repeat(quote_count);
        let body = format!(r#"<a target='{quotes}'><path d="M0 0h16v16H0z"/></a>"#);

        let rendered = render(
            &body,
            "strict",
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )
        .expect("a valid admitted attribute must fit the source-backed sanitizer ceiling");

        assert_eq!(rendered.matches("&quot;").count(), quote_count);
    }

    #[test]
    fn icon_render_work_precharge_is_exact_and_includes_sanitizer_passes() {
        let body = r#"<a target="_blank"><path d="M0 0h16v16H0z"/></a>"#;
        let (_, used) = render_with_work_usage(
            body,
            "strict",
            RenderResourcePolicy::unbounded_for_trusted_input(),
        );
        assert!(used > 0);

        let exact = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, used)
            .unwrap();
        assert!(render(body, "strict", exact).is_ok());

        let one_less = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, used - 1)
            .unwrap();
        let error = render(body, "strict", one_less)
            .expect_err("one work unit below the precharge must reject before rendering");
        assert!(matches!(error, crate::Error::ResourceLimitExceeded(_)));
    }

    #[test]
    fn repeated_maximum_body_precharges_aggregate_svg_and_work_exactly() {
        let maximum =
            usize::try_from(crate::svg::IconRegistryResourceLimitId::MaxBodyBytes.fixed_value())
                .expect("maximum icon body bytes fit usize");
        let prefix = r#"<path data-padding=""#;
        let suffix = r#"" d="M0 0H16V16H0z"/>"#;
        let body = format!(
            "{prefix}{}{suffix}",
            "x".repeat(maximum - prefix.len() - suffix.len())
        );
        assert_eq!(body.len(), maximum);

        let repetitions = 4;
        let (baseline, work, svg_bytes) = render_repeated_with_usage(
            &body,
            "strict",
            repetitions,
            RenderResourcePolicy::unbounded_for_trusted_input(),
        );
        assert_eq!(
            baseline.expect("maximum icon body renders").len(),
            repetitions
        );
        assert!(work > 0);
        assert!(svg_bytes >= maximum.checked_mul(repetitions).unwrap());

        let exact = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, work)
            .unwrap()
            .with_limit(ResourceLimitId::MaxSvgBytes, svg_bytes)
            .unwrap();
        assert!(
            render_repeated_with_usage(&body, "strict", repetitions, exact)
                .0
                .is_ok()
        );

        let svg_one_less = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, svg_bytes - 1)
            .unwrap();
        let error = render_repeated_with_usage(&body, "strict", repetitions, svg_one_less)
            .0
            .expect_err("aggregate SVG bytes plus one must fail before the final expansion");
        assert!(matches!(error, crate::Error::ResourceLimitExceeded(_)));

        let work_one_less = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, work - 1)
            .unwrap();
        let error = render_repeated_with_usage(&body, "strict", repetitions, work_one_less)
            .0
            .expect_err("aggregate icon work plus one must fail before the final expansion");
        assert!(matches!(error, crate::Error::ResourceLimitExceeded(_)));
    }
}
