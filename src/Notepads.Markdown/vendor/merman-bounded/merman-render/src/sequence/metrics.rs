use super::SequenceTextCheckpoints;
use super::constants::{sequence_text_dimensions_height_px, sequence_text_line_step_px};
use crate::Result;
use crate::math::{DelimitedMathLine, MathRenderer, parse_delimited_math_line};
use crate::text::{
    TextMeasurer, TextMetrics, TextStyle, WrapMode,
    measure_mermaid_text_dimensions_with_checkpoint, split_html_br_lines,
    wrap_label_like_mermaid_lines_with_checkpoint,
};
use merman_core::MermaidConfig;

struct SequenceWrapTextMeasurer<'a> {
    measurer: &'a dyn TextMeasurer,
    checkpoints: SequenceTextCheckpoints<'a>,
}

impl TextMeasurer for SequenceWrapTextMeasurer<'_> {
    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
        // The fallible wrapping core checks the same operation immediately after this infallible
        // trait call, so a swallowed cancellation is propagated before another probe begins.
        if self.checkpoints.checkpoint().is_err() {
            return TextMetrics {
                width: 0.0,
                height: 0.0,
                line_count: 0,
            };
        }
        let metrics = self.measurer.measure(text, style);
        let _ = self.checkpoints.checkpoint();
        metrics
    }

    fn measure_svg_simple_text_bbox_width_for_wrap_px(&self, text: &str, style: &TextStyle) -> f64 {
        measure_svg_like_with_html_br(self.measurer, text, style, self.checkpoints)
            .map_or(0.0, |metrics| metrics.0)
    }
}

pub(crate) fn wrap_sequence_label_like_mermaid_lines(
    label: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_width_px: f64,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<Vec<String>> {
    let sequence_measurer = SequenceWrapTextMeasurer {
        measurer,
        checkpoints,
    };
    wrap_label_like_mermaid_lines_with_checkpoint(
        label,
        &sequence_measurer,
        style,
        max_width_px,
        || checkpoints.checkpoint(),
    )
}

/// Accept the static family-list grammar using CSS tokens, including quoted semicolons and
/// escaped identifiers. A declaration separator is never a family value passed to CSSOM.
pub(crate) fn sequence_inline_font_family(style: &TextStyle) -> Option<String> {
    use cssparser::ToCss;
    let family = style.font_family.as_deref()?;
    let mut parser = cssparser::Parser::new(family);
    let mut has_name = false;
    let mut quoted = false;
    let mut css = String::new();
    while !parser.is_exhausted() {
        match parser.next() {
            Ok(token @ cssparser::Token::Ident(_)) if !quoted => {
                if has_name {
                    css.push(' ');
                }
                css.push_str(&token.to_css_string());
                has_name = true;
            }
            Ok(token @ cssparser::Token::QuotedString(_)) if !has_name => {
                css.push_str(&token.to_css_string());
                has_name = true;
                quoted = true;
            }
            Ok(cssparser::Token::Comma) if has_name => {
                css.push_str(", ");
                has_name = false;
                quoted = false;
            }
            _ => return None,
        }
    }
    has_name.then_some(css)
}

pub(crate) fn sequence_drawn_text_style(style: &TextStyle, config: &MermaidConfig) -> TextStyle {
    let mut effective = style.clone();
    // CSSOM rejects invalid assignments, while accepted tokens must be serialized before
    // embedding them in a declaration list (an EOF-terminated string needs its closing quote).
    effective.font_family = Some(
        sequence_inline_font_family(style)
            .unwrap_or_else(|| crate::config::config_font_family_css(config.as_value())),
    );
    effective
}

pub(super) fn measure_svg_like_with_html_br(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<(f64, f64)> {
    let dimensions =
        measure_mermaid_text_dimensions_with_checkpoint(measurer, text, style, || {
            checkpoints.checkpoint()
        })?;
    Ok((dimensions.width as f64, dimensions.height as f64))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SequenceDrawnTextNode {
    Direct,
    Tspan,
}

pub(super) fn measure_drawn_svg_like_with_html_br(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    node: SequenceDrawnTextNode,
    config: &MermaidConfig,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<(f64, f64)> {
    let effective_style = sequence_drawn_text_style(style, config);
    let lines = split_html_br_lines(text);
    let mut width = 0.0_f64;
    let mut height = 0.0_f64;
    for line in lines {
        let measured_line = if line.is_empty() { "\u{200b}" } else { line };
        checkpoints.checkpoint()?;
        let line_width = match node {
            SequenceDrawnTextNode::Direct => {
                measurer.measure_svg_raw_text_bbox_width_px(measured_line, &effective_style)
            }
            SequenceDrawnTextNode::Tspan => {
                measurer.measure_svg_tspan_text_bbox_width_px(measured_line, &effective_style)
            }
        }
        .max(0.0);
        checkpoints.checkpoint()?;
        let line_height = measure_sequence_drawn_line_height(
            measurer,
            measured_line,
            &effective_style,
            node,
            checkpoints,
        )?;
        width = width.max(line_width);
        height += line_height;
    }

    Ok((width, height))
}

/// Measures the final text DOM shape, after resolving its inherited font.
pub(crate) fn measure_sequence_drawn_line_height(
    measurer: &dyn TextMeasurer,
    text: &str,
    drawn_style: &TextStyle,
    node: SequenceDrawnTextNode,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<f64> {
    let text = if text.is_empty() { "\u{200b}" } else { text };
    checkpoints.checkpoint()?;
    let height = match node {
        SequenceDrawnTextNode::Direct => {
            measurer.measure_svg_raw_text_bbox_height_px(text, drawn_style)
        }
        SequenceDrawnTextNode::Tspan => {
            measurer.measure_svg_tspan_text_bbox_height_px(text, drawn_style)
        }
    };
    checkpoints.checkpoint()?;
    Ok(height.max(0.0))
}

/// The middle/center branch of svgDraw.drawText, before its per-row rounding.
pub(crate) fn sequence_drawn_text_first_y(base_y: f64, margin: f64) -> f64 {
    base_y + if margin > 0.0 { margin / 2.0 } else { 0.0 }
}

pub(crate) fn sequence_drawn_text_y(first_y: f64, margin: f64, preceding_height: f64) -> f64 {
    if margin > 0.0 {
        // Math.round chooses the upper integer at negative half ties, unlike f64::round.
        let y = first_y + preceding_height;
        let floor = y.floor();
        floor + if y - floor >= 0.5 { 1.0 } else { 0.0 }
    } else {
        first_y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SequenceMathHeightMode {
    Actor,
    Bound,
    Draw,
}

fn sequence_math_chunks<'text>(
    text: &'text str,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<Vec<&'text str>> {
    let mut chunks = Vec::new();
    let mut search_from = 0usize;
    while let Some(start_rel) = text[search_from..].find("$$") {
        checkpoints.checkpoint()?;
        let start = search_from + start_rel;
        let content_start = start + 2;
        let Some(end_rel) = text[content_start..].find("$$") else {
            break;
        };
        let end = content_start + end_rel + 2;
        chunks.push(&text[start..end]);
        search_from = end;
    }
    checkpoints.checkpoint()?;
    Ok(chunks)
}

fn measure_plain_sequence_fragment(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<TextMetrics> {
    checkpoints.checkpoint()?;
    let metrics = measurer.measure_wrapped(text, style, None, WrapMode::SvgLikeSingleRun);
    checkpoints.checkpoint()?;
    Ok(metrics)
}

fn measure_sequence_mixed_math_line(
    measurer: &dyn TextMeasurer,
    parsed: DelimitedMathLine<'_>,
    style: &TextStyle,
    config: &MermaidConfig,
    math_renderer: &(dyn MathRenderer + Send + Sync),
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<Option<(f64, f64)>> {
    let mut width = 0.0_f64;
    let mut height = 0.0_f64;

    for fragment in parsed.fragments {
        checkpoints.checkpoint()?;
        if !fragment.leading_text.is_empty() {
            let metrics = measure_plain_sequence_fragment(
                measurer,
                fragment.leading_text,
                style,
                checkpoints,
            )?;
            width += metrics.width.max(0.0);
            height = height.max(metrics.height.max(0.0));
        }

        checkpoints.checkpoint()?;
        let mut math_metrics =
            math_renderer.measure_sequence_html_label(fragment.delimited, config);
        checkpoints.checkpoint()?;
        if math_metrics.is_none() {
            checkpoints.checkpoint()?;
            math_metrics = math_renderer.measure_html_label(
                fragment.delimited,
                config,
                style,
                Some(10_000.0),
                WrapMode::HtmlLike,
            );
            checkpoints.checkpoint()?;
        }
        let Some(math_metrics) = math_metrics else {
            return Ok(None);
        };
        width += math_metrics.width.max(0.0);
        height = height.max(math_metrics.height.max(0.0));
    }

    if !parsed.trailing_text.is_empty() {
        let metrics =
            measure_plain_sequence_fragment(measurer, parsed.trailing_text, style, checkpoints)?;
        width += metrics.width.max(0.0);
        height = height.max(metrics.height.max(0.0));
    }

    checkpoints.checkpoint()?;
    Ok(Some((width, height.max(1.0))))
}

fn measure_sequence_mixed_math_label(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    config: &MermaidConfig,
    math_renderer: &(dyn MathRenderer + Send + Sync),
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<Option<TextMetrics>> {
    let mut saw_math = false;
    let mut width = 0.0_f64;
    let mut height = 0.0_f64;
    let mut line_count = 0usize;

    for line in split_html_br_lines(text) {
        checkpoints.checkpoint()?;
        line_count += 1;
        let (line_width, line_height) = if let Some(parsed) = parse_delimited_math_line(line) {
            saw_math = true;
            let Some(metrics) = measure_sequence_mixed_math_line(
                measurer,
                parsed,
                style,
                config,
                math_renderer,
                checkpoints,
            )?
            else {
                return Ok(None);
            };
            metrics
        } else {
            let (w, h) = measure_svg_like_with_html_br(measurer, line, style, checkpoints)?;
            (w.max(0.0), h.max(0.0))
        };
        width = width.max(line_width);
        height += line_height;
    }

    checkpoints.checkpoint()?;
    Ok(saw_math.then_some(TextMetrics {
        width,
        height: height.max(1.0),
        line_count: line_count.max(1),
    }))
}

fn sequence_math_height_px(
    text: &str,
    style: &TextStyle,
    config: &MermaidConfig,
    math_renderer: &(dyn MathRenderer + Send + Sync),
    mode: SequenceMathHeightMode,
    full_metrics: &TextMetrics,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<f64> {
    let height = match mode {
        SequenceMathHeightMode::Actor => full_metrics.height.round().max(1.0),
        SequenceMathHeightMode::Bound | SequenceMathHeightMode::Draw => {
            let line_step = sequence_text_line_step_px(style.font_size).round().max(1.0);
            let base = if mode == SequenceMathHeightMode::Draw {
                line_step
            } else {
                (line_step - 1.0)
                    .max(sequence_text_dimensions_height_px(style.font_size))
                    .max(1.0)
            };
            let mut math_h = base;
            for chunk in sequence_math_chunks(text, checkpoints)? {
                checkpoints.checkpoint()?;
                let metrics = math_renderer.measure_sequence_html_label(chunk, config);
                checkpoints.checkpoint()?;
                if let Some(metrics) = metrics {
                    math_h = math_h.max(metrics.height.round() + 2.0);
                }
            }
            math_h.round().max(1.0)
        }
    };
    Ok(height)
}

pub(crate) fn measure_sequence_math_label(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    config: &MermaidConfig,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    mode: SequenceMathHeightMode,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<Option<(f64, f64)>> {
    checkpoints.checkpoint()?;
    if !text.contains("$$") {
        return Ok(None);
    }
    let Some(renderer) = math_renderer else {
        return Ok(None);
    };
    checkpoints.checkpoint()?;
    let mut full_metrics = renderer.measure_sequence_html_label(text, config);
    checkpoints.checkpoint()?;
    if full_metrics.is_none() {
        full_metrics = measure_sequence_mixed_math_label(
            measurer,
            text,
            style,
            config,
            renderer,
            checkpoints,
        )?;
    }
    if full_metrics.is_none() {
        checkpoints.checkpoint()?;
        full_metrics =
            renderer.measure_html_label(text, config, style, Some(10_000.0), WrapMode::HtmlLike);
        checkpoints.checkpoint()?;
    }
    let Some(full_metrics) = full_metrics else {
        return Ok(None);
    };
    let height = sequence_math_height_px(
        text,
        style,
        config,
        renderer,
        mode,
        &full_metrics,
        checkpoints,
    )?;
    Ok(Some((full_metrics.width.round().max(1.0), height)))
}

pub(super) fn measure_sequence_label_for_layout(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    config: &MermaidConfig,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    mode: SequenceMathHeightMode,
    checkpoints: SequenceTextCheckpoints<'_>,
) -> Result<(f64, f64)> {
    if let Some(metrics) = measure_sequence_math_label(
        measurer,
        text,
        style,
        config,
        math_renderer,
        mode,
        checkpoints,
    )? {
        Ok(metrics)
    } else {
        measure_svg_like_with_html_br(measurer, text, style, checkpoints)
    }
}

#[cfg(test)]
mod tests {
    use crate::math::MathRenderer;
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use crate::text::{TextMeasurer, TextMetrics, TextStyle};
    use merman_core::{OperationControl, OperationPhase};
    use std::cell::{Cell, RefCell};

    #[derive(Debug)]
    struct PreciseMathRenderer;

    impl MathRenderer for PreciseMathRenderer {
        fn render_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
        ) -> Option<String> {
            text.contains("$$").then(|| text.to_string())
        }

        fn measure_sequence_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
        ) -> Option<TextMetrics> {
            (text.starts_with("$$") && text.ends_with("$$")).then_some(TextMetrics {
                width: 10.008,
                height: 20.008,
                line_count: 1,
            })
        }
    }

    struct PreciseTextMeasurer;

    impl TextMeasurer for PreciseTextMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: 1.001,
                height: 2.002,
                line_count: 1,
            }
        }
    }

    #[derive(Default)]
    struct OperationProbe {
        calls: RefCell<Vec<(String, String, String)>>,
    }

    impl OperationProbe {
        fn record(&self, operation: &str, text: &str, style: &TextStyle) {
            self.calls.borrow_mut().push((
                operation.to_string(),
                text.to_string(),
                style.font_family.clone().unwrap_or_default(),
            ));
        }
    }

    impl TextMeasurer for OperationProbe {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            panic!("Sequence text must use the DOM-shape operation, not generic measure")
        }

        fn measure_svg_simple_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
            self.record("simple-width", text, style);
            match style.font_family.as_deref() {
                Some("sans-serif") => 60.0,
                Some("serif") => 70.0,
                _ => 50.0,
            }
        }

        fn measure_mermaid_calculate_text_dimensions(
            &self,
            text: &str,
            style: &TextStyle,
        ) -> TextMetrics {
            self.record("mermaid-dimensions", text, style);
            let width = match style.font_family.as_deref() {
                Some("sans-serif") => 60.0,
                Some(family) if family.trim_end().ends_with(';') => 70.0,
                _ => 50.0,
            };
            let height = match style.font_family.as_deref() {
                Some("sans-serif") => 16.0,
                Some(family) if family.trim_end().ends_with(';') => 17.0,
                _ => 19.0,
            };
            TextMetrics {
                width,
                height,
                line_count: 1,
            }
        }

        fn measure_svg_raw_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
            self.record("raw-width", text, style);
            101.0
        }

        fn measure_svg_tspan_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
            self.record("tspan-width", text, style);
            202.0
        }

        fn measure_svg_tspan_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
            self.record("tspan-height", text, style);
            23.0
        }

        fn measure_svg_raw_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
            self.record("raw-height", text, style);
            19.0
        }

        fn measure_svg_simple_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
            self.record("simple-height", text, style);
            match style.font_family.as_deref() {
                Some("sans-serif") => 16.0,
                Some("serif") => 17.0,
                _ => 19.0,
            }
        }
    }

    fn default_sequence_style() -> TextStyle {
        TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif;".to_string()),
            font_size: 16.0,
            font_weight: Some("400".to_string()),
            font_style: None,
        }
    }

    fn drawn_text_config() -> merman_core::MermaidConfig {
        merman_core::MermaidConfig::from_value(serde_json::json!({
            "fontFamily": "\"trebuchet ms\", verdana, arial, sans-serif;",
            "themeVariables": { "fontFamily": "Theme Family, sans-serif" }
        }))
    }

    #[test]
    fn drawn_text_inherits_theme_only_when_inline_family_is_missing_or_rejected() {
        let config = drawn_text_config();
        for family in [
            None,
            Some(""),
            Some("  "),
            Some("Inline Family; "),
            Some("Arial; font-style: italic"),
        ] {
            let mut style = default_sequence_style();
            style.font_family = family.map(str::to_string);
            let drawn = super::sequence_drawn_text_style(&style, &config);
            assert_eq!(
                drawn.font_family.as_deref(),
                Some("Theme Family,sans-serif")
            );
            assert_eq!(style.font_family.as_deref(), family);
            assert_eq!(drawn.font_size, style.font_size);
            assert_eq!(drawn.font_weight, style.font_weight);
        }

        let mut style = default_sequence_style();
        style.font_family = Some("Inline Family, monospace".to_string());
        let drawn = super::sequence_drawn_text_style(&style, &config);
        assert_eq!(drawn.font_family, style.font_family);
    }

    fn checkpoints(
        meter: &OperationWorkMeter,
        phase: OperationPhase,
    ) -> crate::sequence::SequenceTextCheckpoints<'_> {
        crate::sequence::SequenceTextCheckpoints::for_phase(meter, phase)
    }

    struct CancellingDimensionsMeasurer {
        control: OperationControl,
        calls: Cell<usize>,
    }

    impl TextMeasurer for CancellingDimensionsMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            unreachable!("Sequence dimensions use the explicit Mermaid DOM operation")
        }

        fn measure_mermaid_calculate_text_dimensions(
            &self,
            _text: &str,
            _style: &TextStyle,
        ) -> TextMetrics {
            let calls = self.calls.get() + 1;
            self.calls.set(calls);
            if calls == 1 {
                self.control.cancel();
            }
            TextMetrics {
                width: 10.0,
                height: 20.0,
                line_count: 1,
            }
        }
    }

    #[test]
    fn multiline_sequence_measurement_stops_after_callback_cancels_layout() {
        let control = OperationControl::new();
        let meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control.clone(),
        );
        let measurer = CancellingDimensionsMeasurer {
            control,
            calls: Cell::new(0),
        };
        let label = (0..130)
            .map(|line| format!("line-{line}"))
            .collect::<Vec<_>>()
            .join("<br>");

        let error = super::measure_svg_like_with_html_br(
            &measurer,
            &label,
            &default_sequence_style(),
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap_err();
        let crate::Error::Cancelled(error) = error else {
            panic!("expected Sequence layout cancellation");
        };

        assert_eq!(error.phase, OperationPhase::Layout);
        assert_eq!(measurer.calls.get(), 1);
    }

    #[test]
    fn sequence_mixed_math_metrics_preserve_fragment_precision() {
        let config = merman_core::MermaidConfig::default();
        let style = TextStyle::default();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let metrics = super::measure_sequence_mixed_math_label(
            &PreciseTextMeasurer,
            "a$$x$$b",
            &style,
            &config,
            &PreciseMathRenderer,
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap()
        .unwrap();

        assert!((metrics.width - 12.01).abs() < 1e-12, "{metrics:?}");
        assert!((metrics.height - 20.008).abs() < 1e-12, "{metrics:?}");
    }

    #[cfg(feature = "math")]
    #[test]
    fn sequence_math_measurement_handles_multiple_formulas_on_one_line() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let renderer = crate::math::RatexMathRenderer;
        let config = merman_core::MermaidConfig::default();
        let style = crate::text::TextStyle::default();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let (width, height) = super::measure_sequence_math_label(
            &measurer,
            "a $$x$$ b $$y$$ c",
            &style,
            &config,
            Some(&renderer),
            super::SequenceMathHeightMode::Actor,
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap()
        .expect("each same-line math fragment should contribute to Sequence measurement");

        assert!(width > 0.0, "expected positive measured width");
        assert!(height > 0.0, "expected positive measured height");
    }

    #[cfg(feature = "math")]
    #[test]
    fn sequence_math_measurement_ignores_unclosed_delimiters_on_plain_lines() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let renderer = crate::math::RatexMathRenderer;
        let config = merman_core::MermaidConfig::default();
        let style = crate::text::TextStyle::default();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        assert!(
            super::measure_sequence_math_label(
                &measurer,
                "literal $$",
                &style,
                &config,
                Some(&renderer),
                super::SequenceMathHeightMode::Actor,
                checkpoints(&meter, OperationPhase::Layout),
            )
            .unwrap()
            .is_none(),
            "an unmatched delimiter alone must use plain Sequence measurement"
        );

        let metrics = super::measure_sequence_math_label(
            &measurer,
            "valid $$x$$<br>literal $$",
            &style,
            &config,
            Some(&renderer),
            super::SequenceMathHeightMode::Actor,
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap();

        assert!(
            metrics.is_some(),
            "an unmatched delimiter on a plain line must not discard complete formulas"
        );
    }

    #[test]
    fn sequence_calculated_dimensions_preserve_cssom_input_for_the_exact_operation() {
        let measurer = OperationProbe::default();
        let style = default_sequence_style();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let dimensions = super::measure_svg_like_with_html_br(
            &measurer,
            "alpha<br><br>beta",
            &style,
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap();

        assert_eq!(dimensions, (70.0, 51.0));
        let calls = measurer.calls.borrow();
        assert_eq!(calls.len(), 6);
        assert!(
            calls
                .iter()
                .all(|(operation, _, _)| operation == "mermaid-dimensions")
        );
        assert!(calls.iter().any(|(_, _, family)| family == "sans-serif"));
        assert!(calls.iter().any(|(_, _, family)| family.ends_with(';')));
        assert!(calls.iter().any(|(_, text, _)| text == "\u{200b}"));
    }

    #[test]
    fn sequence_drawn_dimensions_route_direct_and_tspan_dom_shapes_separately() {
        let style = default_sequence_style();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let direct = OperationProbe::default();
        let direct_dimensions = super::measure_drawn_svg_like_with_html_br(
            &direct,
            "alpha<br><br>beta",
            &style,
            super::SequenceDrawnTextNode::Direct,
            &drawn_text_config(),
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap();
        assert_eq!(direct_dimensions, (101.0, 57.0));
        let direct_calls = direct.calls.borrow();
        assert_eq!(
            direct_calls
                .iter()
                .filter(|(operation, _, _)| operation == "raw-width")
                .count(),
            3
        );
        assert!(
            direct_calls
                .iter()
                .all(|(_, _, family)| family == "Theme Family,sans-serif")
        );
        assert!(direct_calls.iter().any(|(_, text, _)| text == "\u{200b}"));
        assert!(
            direct_calls
                .iter()
                .any(|(operation, _, _)| operation == "raw-height")
        );
        assert!(
            direct_calls
                .iter()
                .all(|(operation, _, _)| operation != "simple-height")
        );

        let tspan = OperationProbe::default();
        let tspan_dimensions = super::measure_drawn_svg_like_with_html_br(
            &tspan,
            "alpha",
            &style,
            super::SequenceDrawnTextNode::Tspan,
            &drawn_text_config(),
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap();
        assert_eq!(tspan_dimensions, (202.0, 23.0));
        let tspan_calls = tspan.calls.borrow();
        assert!(
            tspan_calls
                .iter()
                .all(|(_, _, family)| family == "Theme Family,sans-serif")
        );
        assert!(
            tspan_calls
                .iter()
                .any(|(operation, _, _)| operation == "tspan-width")
        );
        assert!(
            tspan_calls
                .iter()
                .all(|(operation, _, _)| operation != "raw-width")
        );
        assert!(
            tspan_calls
                .iter()
                .any(|(operation, _, _)| operation == "tspan-height")
        );
        assert!(
            tspan_calls
                .iter()
                .all(|(operation, _, _)| operation != "simple-height")
        );
    }

    #[test]
    fn sequence_multiline_tspan_height_rounds_only_after_raw_line_accumulation() {
        struct SmallFontProbe;

        impl TextMeasurer for SmallFontProbe {
            fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
                unreachable!("the Sequence DOM operation is explicit")
            }

            fn measure_svg_tspan_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
                1.0
            }

            fn measure_svg_tspan_text_bbox_height_px(
                &self,
                _text: &str,
                _style: &TextStyle,
            ) -> f64 {
                11.05078125
            }
        }

        let text = std::iter::repeat_n("g", 10)
            .collect::<Vec<_>>()
            .join("<br>");
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let (_, raw_height) = super::measure_drawn_svg_like_with_html_br(
            &SmallFontProbe,
            &text,
            &default_sequence_style(),
            super::SequenceDrawnTextNode::Tspan,
            &drawn_text_config(),
            checkpoints(&meter, OperationPhase::Layout),
        )
        .unwrap();

        assert_eq!(raw_height, 110.5078125);
        assert_eq!(raw_height.round(), 111.0);
        assert_ne!(raw_height.round(), 10.0 * 11.05078125_f64.round());
    }
    #[test]
    fn drawn_text_y_rounds_accumulated_heights_with_javascript_half_ties() {
        let first = super::sequence_drawn_text_first_y(10.25, 5.0);
        assert_eq!(super::sequence_drawn_text_y(first, 5.0, 0.0), 13.0);
        assert_eq!(super::sequence_drawn_text_y(first, 5.0, 10.4), 23.0);
        assert_eq!(super::sequence_drawn_text_y(first, 5.0, 30.8), 44.0);
        assert_eq!(super::sequence_drawn_text_y(-1.5, 5.0, 0.0), -1.0);
        for margin in [0.0, -5.0] {
            let first = super::sequence_drawn_text_first_y(10.25, margin);
            assert_eq!(first, 10.25);
            assert_eq!(super::sequence_drawn_text_y(first, margin, 30.8), 10.25);
        }
    }
    #[test]
    fn inline_font_family_uses_css_tokens_without_turning_values_into_declarations() {
        let config = drawn_text_config();
        for family in [
            "Arial; font-style: italic",
            "Arial;",
            "Arial !important",
            "Arial,",
            "Arial,,serif",
            "'Quoted' Extra",
        ] {
            let mut style = default_sequence_style();
            style.font_family = Some(family.to_string());
            assert_eq!(super::sequence_inline_font_family(&style), None, "{family}");
            assert_eq!(
                super::sequence_drawn_text_style(&style, &config)
                    .font_family
                    .as_deref(),
                Some("Theme Family,sans-serif")
            );
        }
        for family in [
            "Arial, sans-serif",
            r#""Semi;Colon", serif"#,
            r"Escaped\;Name, serif",
            "Arial /*comment*/, serif",
        ] {
            let mut style = default_sequence_style();
            style.font_family = Some(family.to_string());
            let inline = super::sequence_inline_font_family(&style);
            assert!(inline.is_some(), "{family}");
            assert_eq!(
                super::sequence_drawn_text_style(&style, &config).font_family,
                inline
            );
        }
    }
}
