//! Deterministic text measurement and wrapping fallback.

use super::heuristic::append_text_width_em;
use super::html_wrap::{HtmlWhiteSpace, NormalizedHtmlText, plan_html_lines};
use super::wrap::split_graphemes_to_width;
use super::{
    TextMeasurer, TextMetrics, TextStyle, WrapMode, trim_end_html_collapsible_ascii_whitespace,
    trim_html_collapsible_ascii_whitespace,
};
use std::convert::Infallible;
use std::sync::Arc;

type TextWidthPxFn = Arc<dyn Fn(&str, &TextStyle) -> f64 + Send + Sync>;

#[derive(Clone)]
struct WidthCallbackTextMeasurer {
    width_px: TextWidthPxFn,
    line_height_factor: f64,
}

#[derive(Clone, Copy, Default)]
struct LineWidthAccumulator {
    heuristic_em: f64,
    char_count: usize,
}

#[derive(Clone, Copy)]
enum LineWidthSource<'a> {
    Heuristic,
    UniformAdvanceEm(f64),
    Callback(&'a TextWidthPxFn),
}

#[derive(Clone, Copy)]
struct LineWidthModel<'a> {
    font_size: f64,
    source: LineWidthSource<'a>,
    style: &'a TextStyle,
    html_text: bool,
}

impl LineWidthModel<'_> {
    fn width_px(self, text: &str) -> f64 {
        if let LineWidthSource::Callback(width_px) = self.source {
            return width_px(text, self.style);
        }

        let mut width = LineWidthAccumulator::default();
        self.append_builtin(&mut width, text);
        self.finish_builtin(width)
    }

    fn candidate_width_px(
        self,
        width: LineWidthAccumulator,
        candidate: &str,
        appended: &str,
    ) -> (LineWidthAccumulator, f64) {
        if let LineWidthSource::Callback(width_px) = self.source {
            return (width, width_px(candidate, self.style));
        }

        let mut width = width;
        self.append_builtin(&mut width, appended);
        (width, self.finish_builtin(width))
    }

    fn append_builtin(self, width: &mut LineWidthAccumulator, text: &str) {
        match self.source {
            LineWidthSource::Heuristic => append_text_width_em(&mut width.heuristic_em, text),
            LineWidthSource::UniformAdvanceEm(_) => {
                width.char_count = width.char_count.saturating_add(
                    text.chars()
                        .filter(|ch| !self.html_text || *ch != '\u{00ad}')
                        .count(),
                );
            }
            LineWidthSource::Callback(_) => unreachable!("callback widths use complete strings"),
        }
    }

    fn finish_builtin(self, width: LineWidthAccumulator) -> f64 {
        match self.source {
            LineWidthSource::Heuristic => width.heuristic_em * self.font_size,
            LineWidthSource::UniformAdvanceEm(advance_em) => {
                width.char_count as f64 * self.font_size * advance_em
            }
            LineWidthSource::Callback(_) => unreachable!("callback widths use complete strings"),
        }
    }
}

trait LineWidthProvider {
    type Error;

    fn width_px(&mut self, text: &str) -> Result<f64, Self::Error>;

    fn begin_html_line_width(&self) -> Option<crate::environment::BuiltinHtmlLineWidth> {
        None
    }

    fn final_width_px(&mut self, text: &str) -> Result<f64, Self::Error> {
        self.width_px(text)
    }

    fn candidate_width_px(
        &mut self,
        width: LineWidthAccumulator,
        candidate: &str,
        appended: &str,
    ) -> Result<(LineWidthAccumulator, f64), Self::Error>;
}

impl LineWidthProvider for LineWidthModel<'_> {
    type Error = Infallible;

    fn begin_html_line_width(&self) -> Option<crate::environment::BuiltinHtmlLineWidth> {
        let factor = match self.source {
            LineWidthSource::Heuristic => 0.0,
            LineWidthSource::UniformAdvanceEm(factor) => factor,
            LineWidthSource::Callback(_) => return None,
        };
        Some(crate::environment::BuiltinHtmlLineWidth::deterministic(
            self.style, factor,
        ))
    }

    fn width_px(&mut self, text: &str) -> Result<f64, Infallible> {
        Ok((*self).width_px(text))
    }

    fn candidate_width_px(
        &mut self,
        width: LineWidthAccumulator,
        candidate: &str,
        appended: &str,
    ) -> Result<(LineWidthAccumulator, f64), Infallible> {
        Ok((*self).candidate_width_px(width, candidate, appended))
    }
}

struct FallibleLineWidth<'a, F> {
    width_px: F,
    style: &'a TextStyle,
}

struct MeasuredLineWidth<'a, F, M> {
    measure: F,
    style: &'a TextStyle,
    final_metrics: Vec<M>,
}

impl<E, F, M> LineWidthProvider for MeasuredLineWidth<'_, F, M>
where
    F: FnMut(&str, &TextStyle) -> Result<(f64, M), E>,
{
    type Error = E;

    fn width_px(&mut self, text: &str) -> Result<f64, E> {
        (self.measure)(text, self.style).map(|(width, _)| width)
    }

    fn final_width_px(&mut self, text: &str) -> Result<f64, E> {
        let (width, metrics) = (self.measure)(text, self.style)?;
        self.final_metrics.push(metrics);
        Ok(width)
    }

    fn candidate_width_px(
        &mut self,
        width: LineWidthAccumulator,
        candidate: &str,
        _appended: &str,
    ) -> Result<(LineWidthAccumulator, f64), E> {
        self.width_px(candidate).map(|width_px| (width, width_px))
    }
}

impl<E, F> LineWidthProvider for FallibleLineWidth<'_, F>
where
    F: FnMut(&str, &TextStyle) -> Result<f64, E>,
{
    type Error = E;

    fn width_px(&mut self, text: &str) -> Result<f64, E> {
        (self.width_px)(text, self.style)
    }

    fn candidate_width_px(
        &mut self,
        width: LineWidthAccumulator,
        candidate: &str,
        _appended: &str,
    ) -> Result<(LineWidthAccumulator, f64), E> {
        self.width_px(candidate).map(|width_px| (width, width_px))
    }
}

/// Shared wrapping output with the exact lines used to calculate its metrics.
#[derive(Debug, Clone)]
pub struct WrappedTextLayout {
    pub metrics: TextMetrics,
    pub lines: Vec<String>,
    /// Natural pre-wrap width for HTML-like labels; absent for SVG-like labels.
    pub raw_width: Option<f64>,
}

/// Measures and wraps text using Merman's existing whitespace and line-break rules.
///
/// The callback receives complete candidate strings, preserving shaping and kerning. It must
/// return finite, non-negative CSS pixel widths. `line_height_px` is supplied by the active font
/// backend rather than estimated from font size and must be finite and non-negative.
/// Callback errors abort immediately, including
/// errors raised while measuring the natural width or final emitted lines.
pub fn try_measure_wrapped_text<E>(
    text: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
    line_height_px: f64,
    width_px: impl FnMut(&str, &TextStyle) -> Result<f64, E>,
) -> Result<WrappedTextLayout, E> {
    let mut width_model = FallibleLineWidth { width_px, style };
    let mut layout = wrapped_text_layout(text, max_width, wrap_mode, &mut width_model)?;
    layout.metrics.height = layout.metrics.line_count as f64 * line_height_px;
    Ok(layout)
}

/// Wraps with the shared policy and returns each emitted row's measured payload in row order.
/// Candidate payloads are discarded; final payloads can supply height without another probe.
/// No text or measurement cache is introduced, and callback errors abort immediately.
/// Widths must be finite and non-negative. Height remains zero for the caller to derive from
/// the payloads. Temporary payload storage is linear in the number of emitted rows.
pub fn try_measure_wrapped_text_with_line_metrics<E, M>(
    text: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
    measure: impl FnMut(&str, &TextStyle) -> Result<(f64, M), E>,
) -> Result<(WrappedTextLayout, Vec<M>), E> {
    let mut width_model = MeasuredLineWidth {
        measure,
        style,
        final_metrics: Vec::new(),
    };
    let layout = wrapped_text_layout(text, max_width, wrap_mode, &mut width_model)?;
    Ok((layout, width_model.final_metrics))
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicTextMeasurer {
    pub char_width_factor: f64,
    pub line_height_factor: f64,
}

impl DeterministicTextMeasurer {
    /// Uses caller-supplied widths with Merman's wrapping behavior.
    ///
    /// Uses `width_px` to measure complete candidate strings while Merman retains its wrapping and
    /// line-height behavior. Vertical metrics and baseline offsets remain deterministic
    /// approximations; the callback does not provide glyph bounds or font ascent/descent.
    /// HTML candidates retain unselected U+00AD controls for shaping. The callback must measure
    /// their zero glyph advance without deleting the input controls before shaping.
    ///
    /// The callback must return a finite, non-negative width in CSS pixels. Use
    /// [`crate::environment::HostTextMeasurer`] when measurement can fail or vary by operation.
    pub fn with_width_callback(
        self,
        width_px: impl Fn(&str, &TextStyle) -> f64 + Send + Sync + 'static,
    ) -> Arc<dyn TextMeasurer + Send + Sync> {
        Arc::new(WidthCallbackTextMeasurer {
            width_px: Arc::new(width_px),
            line_height_factor: self.line_height_factor,
        })
    }

    fn collapse_svg_text_whitespace(text: &str) -> String {
        let mut collapsed = String::with_capacity(text.len());
        let mut pending_space = false;

        for ch in text.chars() {
            if matches!(ch, ' ' | '\t' | '\r' | '\n') {
                pending_space = !collapsed.is_empty();
            } else {
                if pending_space {
                    collapsed.push(' ');
                    pending_space = false;
                }
                collapsed.push(ch);
            }
        }

        collapsed
    }

    fn replace_br_variants(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut i = 0usize;
        while i < text.len() {
            let Some(rest) = text.get(i..) else {
                break;
            };

            // Mirror Mermaid's `lineBreakRegex = /<br\\s*\\/?>/gi` behavior:
            // - allow ASCII whitespace between `br` and the optional `/` or `>`
            // - do NOT accept extra characters (e.g. `<br \\t/>` should *not* count as a break)
            if rest.starts_with('<') {
                let bytes = text.as_bytes();
                if i + 3 < bytes.len()
                    && matches!(bytes[i + 1], b'b' | b'B')
                    && matches!(bytes[i + 2], b'r' | b'R')
                {
                    let mut j = i + 3;
                    while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\r' | b'\n') {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j] == b'/' {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j] == b'>' {
                        out.push('\n');
                        i = j + 1;
                        continue;
                    }
                }
            }

            let Some(ch) = rest.chars().next() else {
                break;
            };
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    fn normalized_text_lines_with(
        text: &str,
        trailing_line_is_empty: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        let t = Self::replace_br_variants(text);
        let mut out = t.split('\n').map(|s| s.to_string()).collect::<Vec<_>>();

        // Mermaid often produces labels with a trailing newline (e.g. YAML `|` block scalars from
        // FlowDB). The rendered label does not keep an extra blank line at the end, so we trim
        // trailing empty lines to keep height parity.
        while out.len() > 1 && out.last().is_some_and(|s| trailing_line_is_empty(s)) {
            out.pop();
        }

        if out.is_empty() {
            vec!["".to_string()]
        } else {
            out
        }
    }

    pub fn normalized_text_lines(text: &str) -> Vec<String> {
        Self::normalized_text_lines_with(text, |line| {
            trim_html_collapsible_ascii_whitespace(line).is_empty()
        })
    }

    pub(crate) fn split_line_to_words(text: &str) -> Vec<String> {
        // Mirrors Mermaid's `splitLineToWords` fallback behavior when `Intl.Segmenter` is absent:
        // split by spaces, then re-add the spaces as separate tokens (preserving multiple spaces).
        let parts = text.split(' ').collect::<Vec<_>>();
        let mut out: Vec<String> = Vec::new();
        for part in parts {
            if !part.is_empty() {
                out.push(part.to_string());
            }
            out.push(" ".to_string());
        }
        while out.last().is_some_and(|s| s == " ") {
            out.pop();
        }
        out
    }

    fn wrapped_line_has_visible_content(text: &str) -> bool {
        !trim_html_collapsible_ascii_whitespace(text).is_empty()
    }

    fn split_token_to_width<'t, W: LineWidthProvider>(
        token: &'t str,
        max_width_px: f64,
        width_model: &mut W,
    ) -> Result<(&'t str, &'t str), W::Error> {
        let mut width = LineWidthAccumulator::default();
        split_graphemes_to_width(token, max_width_px, |candidate, grapheme| {
            let (candidate_width, candidate_width_px) =
                width_model.candidate_width_px(width, candidate, grapheme)?;
            width = candidate_width;
            Ok(candidate_width_px)
        })
    }

    fn wrap_line<W: LineWidthProvider>(
        line: &str,
        max_width_px: f64,
        break_long_words: bool,
        width_model: &mut W,
    ) -> Result<Vec<String>, W::Error> {
        if !max_width_px.is_finite() || max_width_px <= 0.0 {
            return Ok(vec![line.to_string()]);
        }

        let tokens = Self::split_line_to_words(line);
        let mut tokens = std::collections::VecDeque::from(tokens);
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut cur_width = LineWidthAccumulator::default();

        while let Some(tok) = tokens.pop_front() {
            if cur.is_empty() && tok == " " {
                continue;
            }

            let previous_len = cur.len();
            cur.push_str(&tok);
            let (candidate_width, candidate_width_px) =
                width_model.candidate_width_px(cur_width, &cur, &tok)?;
            if candidate_width_px <= max_width_px {
                cur_width = candidate_width;
                continue;
            }
            cur.truncate(previous_len);

            if Self::wrapped_line_has_visible_content(&cur) {
                let line = trim_end_html_collapsible_ascii_whitespace(&cur).to_string();
                cur.clear();
                out.push(line);
                cur_width = LineWidthAccumulator::default();
                tokens.push_front(tok);
                continue;
            }

            // `tok` itself does not fit on an empty line.
            if tok == " " {
                continue;
            }
            if !break_long_words {
                out.push(tok);
            } else {
                // Split at the largest grapheme boundary that fits. Zero-width graphemes do not
                // consume line capacity, and an over-wide grapheme still advances the queue.
                let (head, tail) = Self::split_token_to_width(&tok, max_width_px, width_model)?;
                out.push(head.to_string());
                if !tail.is_empty() {
                    tokens.push_front(tail.to_string());
                }
            }
        }

        if Self::wrapped_line_has_visible_content(&cur) {
            out.push(trim_end_html_collapsible_ascii_whitespace(&cur).to_string());
        }

        if out.is_empty() {
            Ok(vec!["".to_string()])
        } else {
            Ok(out)
        }
    }
}

impl TextMeasurer for DeterministicTextMeasurer {
    fn measure_html_line_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        let source = if self.char_width_factor == 0.0 {
            LineWidthSource::Heuristic
        } else {
            LineWidthSource::UniformAdvanceEm(self.char_width_factor)
        };
        let model = LineWidthModel {
            font_size: style.font_size.max(1.0),
            source,
            style,
            html_text: true,
        };
        model.width_px(text).max(0.0)
    }

    #[allow(private_interfaces)]
    fn begin_html_line_width(
        &self,
        style: &TextStyle,
    ) -> Option<crate::environment::BuiltinHtmlLineWidth> {
        Some(crate::environment::BuiltinHtmlLineWidth::deterministic(
            style,
            self.char_width_factor,
        ))
    }

    #[allow(private_interfaces)]
    fn begin_svg_text_computed_length(
        &self,
        style: &TextStyle,
    ) -> Option<crate::environment::BuiltinSvgComputedLength> {
        (self.char_width_factor == 0.0)
            .then(|| crate::environment::BuiltinSvgComputedLength::deterministic(style))
    }

    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
        self.measure_wrapped(text, style, None, WrapMode::SvgLike)
    }

    fn measure_wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> TextMetrics {
        self.measure_wrapped_impl(text, style, max_width, wrap_mode)
            .0
    }

    fn measure_wrapped_with_raw_width(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> (TextMetrics, Option<f64>) {
        self.measure_wrapped_impl(text, style, max_width, wrap_mode)
    }

    fn measure_mermaid_calculate_text_dimensions(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> TextMetrics {
        let collapsed = Self::collapse_svg_text_whitespace(text);
        TextMetrics {
            width: self.measure_svg_simple_text_bbox_width_for_wrap_px(&collapsed, style),
            height: self.measure_svg_simple_text_bbox_height_px(&collapsed, style),
            line_count: 1,
        }
    }

    fn measure_svg_simple_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        let t = trim_end_html_collapsible_ascii_whitespace(text);
        if t.is_empty() {
            return 0.0;
        }
        (style.font_size.max(1.0) * 1.1).max(0.0)
    }

    fn measure_svg_tspan_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        if trim_end_html_collapsible_ascii_whitespace(text).is_empty() {
            0.0
        } else {
            super::svg_wrapped_first_line_bbox_height_px(style)
        }
    }
}

impl TextMeasurer for WidthCallbackTextMeasurer {
    fn measure_html_line_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        (self.width_px)(text, style)
    }

    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
        self.measure_wrapped(text, style, None, WrapMode::SvgLike)
    }

    fn measure_wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> TextMetrics {
        DeterministicTextMeasurer::measure_wrapped_impl_with_width(
            text,
            style,
            max_width,
            wrap_mode,
            LineWidthSource::Callback(&self.width_px),
            self.line_height_factor,
        )
        .0
    }

    fn measure_wrapped_with_raw_width(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> (TextMetrics, Option<f64>) {
        DeterministicTextMeasurer::measure_wrapped_impl_with_width(
            text,
            style,
            max_width,
            wrap_mode,
            LineWidthSource::Callback(&self.width_px),
            self.line_height_factor,
        )
    }

    fn measure_mermaid_calculate_text_dimensions(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> TextMetrics {
        let collapsed = DeterministicTextMeasurer::collapse_svg_text_whitespace(text);
        TextMetrics {
            width: self.measure_svg_simple_text_bbox_width_for_wrap_px(&collapsed, style),
            height: self.measure_svg_simple_text_bbox_height_px(&collapsed, style),
            line_count: 1,
        }
    }

    fn measure_svg_simple_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        let text = trim_end_html_collapsible_ascii_whitespace(text);
        if text.is_empty() {
            return 0.0;
        }
        (style.font_size.max(1.0) * 1.1).max(0.0)
    }

    fn measure_svg_tspan_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        if trim_end_html_collapsible_ascii_whitespace(text).is_empty() {
            0.0
        } else {
            super::svg_wrapped_first_line_bbox_height_px(style)
        }
    }
}

impl DeterministicTextMeasurer {
    fn measure_wrapped_impl(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> (TextMetrics, Option<f64>) {
        let width_source = if self.char_width_factor == 0.0 {
            LineWidthSource::Heuristic
        } else {
            LineWidthSource::UniformAdvanceEm(self.char_width_factor)
        };
        Self::measure_wrapped_impl_with_width(
            text,
            style,
            max_width,
            wrap_mode,
            width_source,
            self.line_height_factor,
        )
    }

    fn measure_wrapped_impl_with_width(
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
        width_source: LineWidthSource<'_>,
        configured_line_height_factor: f64,
    ) -> (TextMetrics, Option<f64>) {
        let default_line_height_factor = match wrap_mode {
            WrapMode::SvgLike | WrapMode::SvgLikeSingleRun => 1.1,
            WrapMode::HtmlLike => 1.5,
        };
        let line_height_factor = if configured_line_height_factor == 0.0 {
            default_line_height_factor
        } else {
            configured_line_height_factor
        };

        let font_size = style.font_size.max(1.0);
        let mut width_model = LineWidthModel {
            font_size,
            source: width_source,
            style,
            html_text: wrap_mode == WrapMode::HtmlLike,
        };
        let mut layout = match wrapped_text_layout(text, max_width, wrap_mode, &mut width_model) {
            Ok(layout) => layout,
            Err(error) => match error {},
        };
        layout.metrics.height = layout.metrics.line_count as f64 * font_size * line_height_factor;
        (layout.metrics, layout.raw_width)
    }
}

fn wrapped_text_layout<W: LineWidthProvider>(
    text: &str,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
    width_model: &mut W,
) -> Result<WrappedTextLayout, W::Error> {
    let max_width = max_width.filter(|width| width.is_finite() && *width > 0.0);
    let break_long_words = matches!(wrap_mode, WrapMode::SvgLike | WrapMode::SvgLikeSingleRun);
    let raw_lines = DeterministicTextMeasurer::normalized_text_lines(text);
    let raw_width = if wrap_mode == WrapMode::HtmlLike || max_width.is_none() {
        let mut width = 0.0_f64;
        for line in &raw_lines {
            width = width.max(if max_width.is_none() {
                width_model.final_width_px(line)?
            } else {
                width_model.width_px(line)?
            });
        }
        Some(width)
    } else {
        None
    };
    let html_break_spaces_active = wrap_mode == WrapMode::HtmlLike
        && max_width.is_some_and(|width| raw_width.is_some_and(|raw| raw > width));
    let mut lines = Vec::new();
    for line in raw_lines {
        if wrap_mode == WrapMode::HtmlLike
            && let Some(width) = max_width
        {
            let line = if html_break_spaces_active {
                line.as_str()
            } else {
                trim_html_collapsible_ascii_whitespace(&line)
            };
            let normalized = NormalizedHtmlText::single_mode(line, HtmlWhiteSpace::BreakSpaces);
            let builtin = width_model.begin_html_line_width();
            let planned = plan_html_lines(
                &normalized,
                Some(width),
                builtin,
                &mut |text| width_model.width_px(text),
                &mut || Ok(()),
            )?;
            lines.extend(planned.into_iter().map(|line| line.text));
        } else if let Some(width) = max_width {
            lines.extend(DeterministicTextMeasurer::wrap_line(
                &line,
                width,
                break_long_words,
                width_model,
            )?);
        } else {
            lines.push(line);
        }
    }
    let mut width = if max_width.is_none() {
        raw_width.unwrap_or_default()
    } else {
        let mut width = 0.0_f64;
        for line in &lines {
            width = width.max(width_model.final_width_px(line)?);
        }
        width
    };
    if html_break_spaces_active && let Some(max_width) = max_width {
        // Overflowing HTML labels occupy the table width unless min-content is wider.
        width = width.max(max_width);
    }
    Ok(WrappedTextLayout {
        metrics: TextMetrics {
            width,
            height: 0.0,
            line_count: lines.len(),
        },
        lines,
        raw_width: if wrap_mode == WrapMode::HtmlLike {
            raw_width
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn final_row_payloads_match_emitted_rows_without_additional_probes() {
        let style = super::TextStyle::default();
        for mode in [
            super::WrapMode::SvgLike,
            super::WrapMode::SvgLikeSingleRun,
            super::WrapMode::HtmlLike,
        ] {
            for limit in [None, Some(45.0), Some(0.0), Some(f64::NAN)] {
                for label in [
                    "alpha beta<br>last",
                    "",
                    " ",
                    "A<br><br>B",
                    "alpha beta ",
                    "\u{a0}A\u{a0}",
                    "👩‍🔬👨‍🔬",
                ] {
                    let mut expected_calls = Vec::new();
                    let expected = super::try_measure_wrapped_text(
                        label,
                        &style,
                        limit,
                        mode,
                        0.0,
                        |text, _| {
                            expected_calls.push(text.to_owned());
                            Ok::<_, ()>(text.len() as f64 * 10.0)
                        },
                    )
                    .unwrap();
                    let mut actual_calls = Vec::new();
                    let (actual, payloads) = super::try_measure_wrapped_text_with_line_metrics(
                        label,
                        &style,
                        limit,
                        mode,
                        |text, _| {
                            actual_calls.push(text.to_owned());
                            Ok::<_, ()>((text.len() as f64 * 10.0, text.to_owned()))
                        },
                    )
                    .unwrap();
                    assert_eq!(actual_calls, expected_calls);
                    assert_eq!(payloads, actual.lines);
                    assert_eq!(actual.lines, expected.lines);
                    assert_eq!(actual.metrics.width, expected.metrics.width);
                    assert_eq!(actual.raw_width, expected.raw_width);
                }
            }
        }
    }

    #[test]
    fn fallible_html_wrapping_preserves_unicode_and_discretionary_breaks() {
        let measurer = super::DeterministicTextMeasurer {
            char_width_factor: 1.0,
            ..super::DeterministicTextMeasurer::default()
        };
        let style = super::TextStyle {
            font_size: 10.0,
            ..super::TextStyle::default()
        };
        for label in [
            "负责人审批",
            "alpha beta",
            "half-rounded",
            "inter\u{00ad}face",
            " A  B\u{00a0} ",
            "a  b",
        ] {
            for limit in [Some(25.0), Some(50.0), Some(100.0), None] {
                let expected = super::TextMeasurer::measure_wrapped_with_raw_width(
                    &measurer,
                    label,
                    &style,
                    limit,
                    super::WrapMode::HtmlLike,
                );
                let actual = super::try_measure_wrapped_text(
                    label,
                    &style,
                    limit,
                    super::WrapMode::HtmlLike,
                    15.0,
                    |text, _| {
                        Ok::<_, ()>(
                            text.chars().filter(|ch| *ch != '\u{00ad}').count() as f64 * 10.0,
                        )
                    },
                )
                .unwrap();
                assert_eq!(
                    actual.metrics.width, expected.0.width,
                    "{label:?}, {limit:?}"
                );
                assert_eq!(
                    actual.metrics.height, expected.0.height,
                    "{label:?}, {limit:?}"
                );
                assert_eq!(
                    actual.metrics.line_count, expected.0.line_count,
                    "{label:?}, {limit:?}"
                );
                assert_eq!(actual.raw_width, expected.1, "{label:?}, {limit:?}");
            }
        }
    }

    #[test]
    fn final_row_payload_errors_stop_measurement_immediately() {
        let mut calls = 0;
        let result = super::try_measure_wrapped_text_with_line_metrics(
            "first<br>second",
            &super::TextStyle::default(),
            None,
            super::WrapMode::SvgLike,
            |text, _| {
                calls += 1;
                if text == "second" {
                    Err("canceled")
                } else {
                    Ok((1.0, ()))
                }
            },
        );
        assert_eq!(result.unwrap_err(), "canceled");
        assert_eq!(calls, 2);
    }

    use super::{
        DeterministicTextMeasurer, LineWidthModel, LineWidthSource, try_measure_wrapped_text,
    };
    use crate::text::{TextMeasurer, TextStyle, WrapMode};
    use std::sync::{Arc, Mutex};
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn wrapping_uses_estimated_width_instead_of_character_count() {
        let measurer = DeterministicTextMeasurer::default();
        let metrics = measurer.measure_wrapped(
            "iiii WWWW",
            &TextStyle {
                font_size: 10.0,
                ..TextStyle::default()
            },
            Some(20.0),
            WrapMode::SvgLike,
        );

        assert_eq!(metrics.line_count, 3);
        assert!((metrics.width - 17.0).abs() < f64::EPSILON, "{metrics:?}");
    }

    #[test]
    fn long_word_splitting_keeps_zero_width_scalars_with_visible_text() {
        let (head, tail) = DeterministicTextMeasurer::split_token_to_width(
            "W\u{0301}W",
            8.0,
            &mut LineWidthModel {
                font_size: 10.0,
                source: LineWidthSource::Heuristic,
                style: &TextStyle::default(),
                html_text: false,
            },
        )
        .unwrap();
        assert_eq!(head, "W\u{0301}");
        assert_eq!(tail, "W");

        let metrics = DeterministicTextMeasurer::default().measure_wrapped(
            "\u{0301}WW",
            &TextStyle {
                font_size: 10.0,
                ..TextStyle::default()
            },
            Some(8.5),
            WrapMode::SvgLike,
        );

        assert_eq!(metrics.line_count, 2);
        assert!((metrics.width - 8.5).abs() < f64::EPSILON, "{metrics:?}");
    }

    #[test]
    fn default_width_uses_sequence_aware_unicode_display_width() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };

        for sequence in ["👩‍🔬", "👨‍👩‍👧‍👦", "👍🏽", "🇨🇳", "1️⃣"]
        {
            let metrics = measurer.measure(sequence, &style);
            assert_eq!(metrics.width, 16.0, "sequence={sequence:?}");
        }
    }

    #[test]
    fn wrapping_splits_long_emoji_runs_only_at_grapheme_boundaries() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        let mut width_model = LineWidthModel {
            font_size: 16.0,
            source: LineWidthSource::Heuristic,
            style: &style,
            html_text: false,
        };

        for (text, first, second) in [
            ("👩‍🔬👨‍🔬", "👩‍🔬", "👨‍🔬"),
            ("👍🏽👍🏻", "👍🏽", "👍🏻"),
            ("🇨🇳🇺🇸", "🇨🇳", "🇺🇸"),
            ("1️⃣2️⃣", "1️⃣", "2️⃣"),
        ] {
            let (head, tail) =
                DeterministicTextMeasurer::split_token_to_width(text, 16.0, &mut width_model)
                    .unwrap();
            assert_eq!((head, tail), (first, second), "text={text:?}");

            let metrics = measurer.measure_wrapped(text, &style, Some(16.0), WrapMode::SvgLike);
            assert_eq!(metrics.line_count, 2, "text={text:?}: {metrics:?}");
            assert_eq!(metrics.width, 16.0, "text={text:?}: {metrics:?}");
        }
    }

    #[test]
    fn html_long_words_preserve_their_min_content_width() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        };

        let metrics = measurer.measure_wrapped(
            "Supercalifragilisticexpialidocious",
            &style,
            Some(20.0),
            WrapMode::HtmlLike,
        );

        assert_eq!(metrics.line_count, 1);
        assert!(metrics.width > 20.0, "{metrics:?}");
    }

    #[test]
    fn html_wrapping_uses_unicode_soft_breaks_and_fixed_container_width() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        };

        let cjk = measurer.measure_wrapped("负责人审批", &style, Some(25.0), WrapMode::HtmlLike);
        assert_eq!(cjk.line_count, 3);
        assert_eq!(cjk.width.to_bits(), 25.0_f64.to_bits());

        let breakable =
            measurer.measure_wrapped("alpha beta", &style, Some(40.0), WrapMode::HtmlLike);
        assert_eq!(breakable.line_count, 2);
        assert_eq!(breakable.width.to_bits(), 40.0_f64.to_bits());

        let hyphenated =
            measurer.measure_wrapped("half-rounded", &style, Some(21.5), WrapMode::HtmlLike);
        assert_eq!(hyphenated.line_count, 2);
        assert!(
            hyphenated.width > 21.5,
            "unbreakable segment keeps min-content width: {hyphenated:?}"
        );

        let preserved_spaces =
            measurer.measure_wrapped("a  b", &style, Some(7.0), WrapMode::HtmlLike);
        assert_eq!(preserved_spaces.line_count, 3);
        assert!(preserved_spaces.width > 7.0, "{preserved_spaces:?}");
    }

    #[test]
    fn html_soft_hyphen_is_hidden_until_selected_and_contributes_its_visible_width() {
        let measurer = DeterministicTextMeasurer {
            char_width_factor: 1.0,
            ..DeterministicTextMeasurer::default()
        };
        let style = TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        };
        let text = "alpha\u{00ad}beta";

        let natural = measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike);
        assert_eq!(natural.width, 90.0);
        assert_eq!(natural.line_count, 1);
        let fitted = measurer.measure_wrapped(text, &style, Some(90.0), WrapMode::HtmlLike);
        assert_eq!(fitted.width, 90.0);
        assert_eq!(fitted.line_count, 1);

        let wrapped = measurer.measure_wrapped(text, &style, Some(60.0), WrapMode::HtmlLike);
        assert_eq!(wrapped.line_count, 2);
        assert_eq!(wrapped.width, 60.0);
        let (wrapped, raw_width) =
            measurer.measure_wrapped_with_raw_width(text, &style, Some(50.0), WrapMode::HtmlLike);
        assert_eq!(wrapped.line_count, 2);
        assert_eq!(
            wrapped.width, 60.0,
            "the selected hyphen contributes min-content width"
        );
        assert_eq!(raw_width, Some(90.0));
    }

    #[test]
    fn html_inactive_break_spaces_trims_only_collapsible_edges_and_keeps_raw_width() {
        let measurer = DeterministicTextMeasurer {
            char_width_factor: 1.0,
            ..DeterministicTextMeasurer::default()
        };
        let style = TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        };
        let text = " A  B\u{00a0} ";
        let natural = measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike);
        assert_eq!(natural.width, 70.0);
        let (fitted, raw) =
            measurer.measure_wrapped_with_raw_width(text, &style, Some(70.0), WrapMode::HtmlLike);
        assert_eq!(fitted.width, 50.0);
        assert_eq!(fitted.line_count, 1);
        assert_eq!(raw, Some(70.0));

        let (active, raw) =
            measurer.measure_wrapped_with_raw_width(text, &style, Some(60.0), WrapMode::HtmlLike);
        assert_eq!(active.line_count, 2);
        assert_eq!(active.width, 60.0);
        assert_eq!(raw, Some(70.0));
    }

    #[test]
    fn html_soft_hyphen_callback_preserves_shaping_controls_in_complete_candidates() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let measurer = DeterministicTextMeasurer::default().with_width_callback(move |text, _| {
            recorded.lock().unwrap().push(text.to_string());
            match text {
                "f\u{00ad}fi" => 18.0,
                "f-" => 16.0,
                "fi" => 11.0,
                _ => text.chars().filter(|ch| *ch != '\u{00ad}').count() as f64 * 10.0,
            }
        });
        let style = TextStyle::default();
        let text = "f\u{00ad}fi";

        let natural = measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike);
        assert_eq!(natural.width, 18.0);
        assert_eq!(&*calls.lock().unwrap(), &["f\u{00ad}fi"]);
        calls.lock().unwrap().clear();

        let wrapped = measurer.measure_wrapped(text, &style, Some(16.0), WrapMode::HtmlLike);
        assert_eq!(wrapped.line_count, 2);
        assert_eq!(wrapped.width, 16.0);
        let calls = calls.lock().unwrap();
        assert!(calls.iter().any(|candidate| candidate == "f\u{00ad}fi"));
        assert!(calls.iter().any(|candidate| candidate == "f-"));
        assert!(calls.iter().any(|candidate| candidate == "fi"));
    }

    #[test]
    fn decoded_html_line_width_keeps_literal_markup_and_matches_streamed_candidates() {
        let style = TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        };
        for char_width_factor in [0.0, 1.0] {
            let measurer = DeterministicTextMeasurer {
                char_width_factor,
                ..DeterministicTextMeasurer::default()
            };
            let text = "left<br>👩‍🔬A\u{301}right";
            let expected = measurer.measure_html_line_width_px(text, &style);
            let mut streamed = measurer
                .begin_html_line_width(&style)
                .expect("built-in line width");
            for part in ["left<", "br>", "👩", "\u{200d}", "🔬A", "\u{301}right"] {
                streamed.push_text(part);
            }
            assert_eq!(streamed.width_px().to_bits(), expected.to_bits());
            assert!(
                expected
                    > measurer
                        .measure_wrapped(text, &style, None, WrapMode::HtmlLike)
                        .width
            );
            assert_eq!(
                measurer
                    .measure_html_line_width_px("a\u{00ad}b", &style)
                    .to_bits(),
                measurer.measure_html_line_width_px("ab", &style).to_bits()
            );
        }

        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let callback = DeterministicTextMeasurer::default().with_width_callback(move |text, _| {
            recorded.lock().unwrap().push(text.to_string());
            23.5
        });
        assert_eq!(callback.measure_html_line_width_px("a<br>b", &style), 23.5);
        assert_eq!(&*calls.lock().unwrap(), &["a<br>b"]);
    }

    #[test]
    fn repeated_measurement_is_bitwise_deterministic() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        let first = measurer.measure_wrapped(
            "Latin / 中文 / 🙂 / e\u{0301}",
            &style,
            Some(80.0),
            WrapMode::SvgLike,
        );
        let second = measurer.measure_wrapped(
            "Latin / 中文 / 🙂 / e\u{0301}",
            &style,
            Some(80.0),
            WrapMode::SvgLike,
        );

        assert_eq!(first.width.to_bits(), second.width.to_bits());
        assert_eq!(first.height.to_bits(), second.height.to_bits());
        assert_eq!(first.line_count, second.line_count);
    }

    #[test]
    fn exact_natural_width_does_not_trigger_a_spurious_wrap() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        let text = "FtQHasK pGRJ";

        for mode in [WrapMode::SvgLike, WrapMode::HtmlLike] {
            let natural = measurer.measure_wrapped(text, &style, None, mode);
            let fitted = measurer.measure_wrapped(text, &style, Some(natural.width), mode);
            assert_eq!(fitted.line_count, 1, "{mode:?}: {natural:?} -> {fitted:?}");
            assert_eq!(fitted.width.to_bits(), natural.width.to_bits());
        }
    }

    #[test]
    fn non_additive_callback_fits_at_exact_natural_width() {
        let measurer = DeterministicTextMeasurer::default().with_width_callback(|text, _| {
            let count = text.chars().count();
            count as f64 * 10.0 - count.saturating_sub(1) as f64
        });
        let style = TextStyle::default();
        let text = "A V";

        for mode in [WrapMode::SvgLike, WrapMode::HtmlLike] {
            let natural = measurer.measure_wrapped(text, &style, None, mode);
            assert_eq!(natural.width, 28.0);
            let fitted = measurer.measure_wrapped(text, &style, Some(natural.width), mode);
            assert_eq!(fitted.line_count, 1, "{mode:?}: {natural:?} -> {fitted:?}");
            assert_eq!(fitted.width.to_bits(), natural.width.to_bits());
        }
    }

    #[test]
    fn callback_preserves_html_fixed_width_and_long_word_overflow() {
        let measurer = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.graphemes(true).count() as f64 * 10.0);
        let style = TextStyle::default();

        let breakable =
            measurer.measure_wrapped("alpha beta", &style, Some(60.0), WrapMode::HtmlLike);
        assert_eq!(breakable.line_count, 2);
        assert_eq!(breakable.width, 60.0);

        let long_word =
            measurer.measure_wrapped("abcdefgh", &style, Some(30.0), WrapMode::HtmlLike);
        assert_eq!(long_word.line_count, 1);
        assert_eq!(long_word.width, 80.0);
    }

    #[test]
    fn callback_svg_wrapping_splits_only_at_grapheme_boundaries() {
        let measurer = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.graphemes(true).count() as f64 * 10.0);

        let metrics =
            measurer.measure_wrapped("👩‍🔬👨‍🔬", &TextStyle::default(), Some(10.0), WrapMode::SvgLike);

        assert_eq!(metrics.line_count, 2);
        assert_eq!(metrics.width, 10.0);
    }

    #[test]
    fn callback_receives_complete_text_and_returns_pixel_width() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let measurer =
            DeterministicTextMeasurer::default().with_width_callback(move |text, style| {
                recorded
                    .lock()
                    .unwrap()
                    .push((text.to_string(), style.font_size));
                37.0
            });
        let style = TextStyle {
            font_size: 99.0,
            ..TextStyle::default()
        };

        assert_eq!(measurer.measure("ffi", &style).width, 37.0);
        assert_eq!(&*calls.lock().unwrap(), &[("ffi".to_string(), 99.0)]);
    }

    #[test]
    fn shared_wrapping_matches_existing_modes_and_preserves_native_line_height() {
        let width = |text: &str| {
            let count = text.graphemes(true).count();
            count as f64 * 10.0 - count.saturating_sub(1) as f64
        };
        let measurer =
            DeterministicTextMeasurer::default().with_width_callback(move |text, _| width(text));
        let style = TextStyle::default();

        for mode in [
            WrapMode::SvgLike,
            WrapMode::SvgLikeSingleRun,
            WrapMode::HtmlLike,
        ] {
            for text in [
                "",
                "a  b",
                "alpha beta",
                "abcdefgh",
                "甲乙丙",
                "👩‍🔬👨‍🔬",
                "a<BR/>b\n",
            ] {
                for limit in [None, Some(20.0), Some(60.0), Some(f64::NAN)] {
                    let expected =
                        measurer.measure_wrapped_with_raw_width(text, &style, limit, mode);
                    let actual =
                        try_measure_wrapped_text(text, &style, limit, mode, 23.75, |text, _| {
                            Ok::<_, ()>(width(text))
                        })
                        .unwrap();
                    assert_eq!(actual.metrics.width.to_bits(), expected.0.width.to_bits());
                    assert_eq!(actual.metrics.line_count, expected.0.line_count);
                    assert_eq!(actual.raw_width, expected.1);
                    assert_eq!(actual.lines.len(), actual.metrics.line_count);
                    assert_eq!(actual.metrics.height, actual.lines.len() as f64 * 23.75);
                }
            }
        }
    }

    #[test]
    fn shared_wrapping_returns_emitted_grapheme_lines_and_html_raw_width() {
        let style = TextStyle::default();
        let svg = try_measure_wrapped_text(
            "👩‍🔬👨‍🔬",
            &style,
            Some(10.0),
            WrapMode::SvgLike,
            17.0,
            |text, _| Ok::<_, ()>(text.graphemes(true).count() as f64 * 10.0),
        )
        .unwrap();
        assert_eq!(svg.lines, ["👩‍🔬", "👨‍🔬"]);
        assert_eq!(svg.metrics.height, 34.0);
        assert_eq!(svg.raw_width, None);

        let html = try_measure_wrapped_text(
            "alpha beta",
            &style,
            Some(60.0),
            WrapMode::HtmlLike,
            19.0,
            |text, _| Ok::<_, ()>(text.len() as f64 * 10.0),
        )
        .unwrap();
        assert_eq!(html.lines, ["alpha ", "beta"]);
        assert_eq!(html.metrics.width, 60.0);
        assert_eq!(html.metrics.height, 38.0);
        assert_eq!(html.raw_width, Some(100.0));
    }

    #[test]
    fn shared_wrapping_propagates_errors_without_further_probes() {
        let style = TextStyle::default();
        for mode in [WrapMode::SvgLike, WrapMode::HtmlLike] {
            let mut probes = Vec::new();
            let result = try_measure_wrapped_text(
                "alpha beta",
                &style,
                Some(40.0),
                mode,
                19.0,
                |text, _| {
                    probes.push(text.to_string());
                    Err::<f64, _>("canceled")
                },
            );
            assert_eq!(result.unwrap_err(), "canceled");
            assert_eq!(probes.len(), 1);
        }
    }
}
