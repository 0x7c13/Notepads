//! Flowchart-aware text metrics and Markdown measurement helpers.

use super::html_wrap::html_line_text_with_break;
use super::line_break::{
    HtmlBreakKind, html_break_spaces_segments,
    html_break_spaces_segments_with_metadata_and_checkpoints,
};
use super::{
    MermaidMarkdownWordType, TextMeasurer, TextMetrics, TextStyle, WrapMode, ceil_to_1_64_px,
    is_html_collapsible_ascii_whitespace, mermaid_markdown_to_lines,
    mermaid_xhtml_label_plain_text, round_to_1_64_px, trim_html_collapsible_ascii_whitespace, wrap,
};
use crate::environment::{
    BuiltinHtmlLineWidth, BuiltinTextMeasurementOperationCarrier, InlineHtmlMeasurementCarrier,
    TextMeasurementOperation,
};

// This input has already passed through the lightweight HTML parser. Do not use the generic text
// normalizer here: its Unicode trim would erase a final NBSP-only line before inline measurement.
fn normalized_preparsed_html_text_lines(text: &str) -> Vec<String> {
    let mut out = text.split('\n').map(str::to_string).collect::<Vec<_>>();

    while out.len() > 1
        && out
            .last()
            .is_some_and(|line| trim_html_collapsible_ascii_whitespace(line).is_empty())
    {
        out.pop();
    }

    if out.is_empty() {
        vec![String::new()]
    } else {
        out
    }
}

pub(crate) fn measure_xhtml_label_fragment(
    measurer: &dyn TextMeasurer,
    fragment: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
) -> TextMetrics {
    if let Some(plain_text) = mermaid_xhtml_label_plain_text(fragment)
        && !plain_text.contains('<')
    {
        measurer.measure_wrapped(&plain_text, style, max_width, wrap_mode)
    } else {
        measure_html_with_inline_styles(measurer, fragment, style, max_width, wrap_mode)
    }
}

pub(crate) fn style_requests_bold_font_weight(style: &TextStyle) -> bool {
    let Some(w) = style.font_weight.as_deref() else {
        return false;
    };
    let w = w.trim();
    if w.is_empty() {
        return false;
    }
    let lower = w.to_ascii_lowercase();
    if lower == "bold" || lower == "bolder" {
        return true;
    }
    lower
        .parse::<f64>()
        .ok()
        .is_some_and(|n| n.is_finite() && n >= 600.0)
}

pub(crate) fn style_requests_italic_font_style(style: &TextStyle) -> bool {
    let Some(value) = style.font_style.as_deref() else {
        return false;
    };
    let value = value.trim().to_ascii_lowercase();
    value == "italic" || value.starts_with("italic ") || value.starts_with("oblique")
}

#[derive(Debug, Clone, Default)]
struct InlineTextRun {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
}

fn push_inline_text_char(
    runs: &mut Vec<InlineTextRun>,
    ch: char,
    bold: bool,
    italic: bool,
    code: bool,
) {
    if let Some(run) = runs
        .last_mut()
        .filter(|run| run.bold == bold && run.italic == italic && run.code == code)
    {
        run.text.push(ch);
    } else {
        runs.push(InlineTextRun {
            text: ch.to_string(),
            bold,
            italic,
            code,
        });
    }
}

fn inline_text_style(base: &TextStyle, bold: bool, italic: bool, code: bool) -> TextStyle {
    let mut style = base.clone();
    if bold && !style_requests_bold_font_weight(&style) {
        style.font_weight = Some("700".to_string());
    }
    if italic && !style_requests_italic_font_style(&style) {
        style.font_style = Some("italic".to_string());
    }
    if code {
        style.font_family = Some("monospace".to_string());
    }
    style
}

fn measure_inline_run_width_px<M: TextMeasurer + ?Sized>(
    measurer: &M,
    text: &str,
    style: &TextStyle,
    wrap_mode: WrapMode,
    svg_advance: bool,
) -> f64 {
    if wrap_mode == WrapMode::HtmlLike {
        measurer.measure_html_line_width_px(text, style)
    } else if svg_advance {
        measurer.measure_svg_text_computed_length_px(text, style)
    } else {
        measurer.measure_wrapped(text, style, None, wrap_mode).width
    }
}

fn measure_inline_runs_width_px<M: TextMeasurer + ?Sized>(
    measurer: &M,
    runs: &[InlineTextRun],
    style: &TextStyle,
    wrap_mode: WrapMode,
    svg_advance: bool,
) -> f64 {
    runs.iter()
        .filter(|run| !run.text.is_empty())
        .map(|run| {
            let run_style = inline_text_style(style, run.bold, run.italic, run.code);
            measure_inline_run_width_px(measurer, &run.text, &run_style, wrap_mode, svg_advance)
        })
        .sum()
}

#[derive(Debug, Clone, Copy)]
struct InlineRunFragment {
    run_index: usize,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy)]
struct InlineBreakSegment {
    fragment_start: usize,
    fragment_end: usize,
    end: HtmlBreakKind,
}

#[derive(Debug, Default)]
struct InlineHtmlPlanningStats {
    #[cfg(test)]
    source_bytes: usize,
    #[cfg(test)]
    run_visits: usize,
    #[cfg(test)]
    break_segments: usize,
    #[cfg(test)]
    fragment_refs: usize,
    #[cfg(test)]
    fragment_capacity_refs: usize,
    #[cfg(test)]
    line_plan_updates: usize,
    #[cfg(test)]
    line_plan_bytes: usize,
    #[cfg(test)]
    merged_scratch_bytes: usize,
    #[cfg(test)]
    fragment_measure_visits: usize,
    #[cfg(test)]
    measurement_payload_copy_bytes: usize,
    #[cfg(test)]
    source_range: Option<(usize, usize)>,
    #[cfg(test)]
    opaque_style_group_index_bytes: usize,
    #[cfg(test)]
    backend_requests: usize,
    #[cfg(test)]
    backend_request_bytes: usize,
    #[cfg(test)]
    builtin_stream_bytes: usize,
    #[cfg(test)]
    builtin_stream_scalars: usize,
    #[cfg(test)]
    builtin_profile_initializations: usize,
    #[cfg(test)]
    builtin_profile_cache_bytes: usize,
    #[cfg(test)]
    builtin_group_state_copies: usize,
    #[cfg(test)]
    builtin_checkpoint_copies: usize,
}

impl InlineHtmlPlanningStats {
    fn record_source(&mut self, bytes: usize, runs: usize) {
        #[cfg(test)]
        {
            self.source_bytes += bytes;
            // One pass computes the exact concatenation capacity; the second fills it.
            self.run_visits += runs.saturating_mul(2);
        }
        #[cfg(not(test))]
        let _ = (bytes, runs);
    }

    fn record_source_range(&mut self, source: &str) {
        #[cfg(test)]
        {
            let start = source.as_ptr() as usize;
            self.source_range = Some((start, start.saturating_add(source.len())));
        }
        #[cfg(not(test))]
        let _ = source;
    }

    fn record_source_slice(&mut self, text: &str) {
        #[cfg(test)]
        {
            if text.is_empty() {
                return;
            }
            let start = text.as_ptr() as usize;
            let end = start.saturating_add(text.len());
            let is_borrowed = self.source_range.is_some_and(|(source_start, source_end)| {
                start >= source_start && end <= source_end
            });
            if !is_borrowed {
                self.measurement_payload_copy_bytes += text.len();
            }
        }
        #[cfg(not(test))]
        let _ = text;
    }

    fn record_indexed_run(&mut self) {
        #[cfg(test)]
        {
            self.run_visits += 1;
        }
    }

    fn record_segment(&mut self) {
        #[cfg(test)]
        {
            self.break_segments += 1;
        }
    }

    fn record_fragment(&mut self) {
        #[cfg(test)]
        {
            self.fragment_refs += 1;
        }
    }

    fn record_fragment_capacity(&mut self, capacity: usize) {
        #[cfg(test)]
        {
            self.fragment_capacity_refs += capacity;
        }
        #[cfg(not(test))]
        let _ = capacity;
    }

    fn record_line_plan(&mut self, bytes: usize) {
        #[cfg(test)]
        {
            self.line_plan_bytes += bytes;
        }
        #[cfg(not(test))]
        let _ = bytes;
    }

    fn record_line_plan_update(&mut self) {
        #[cfg(test)]
        {
            self.line_plan_updates += 1;
        }
    }

    fn record_fragment_measure_visits(&mut self, visits: usize) {
        #[cfg(test)]
        {
            self.fragment_measure_visits += visits;
        }
        #[cfg(not(test))]
        let _ = visits;
    }

    fn record_opaque_style_group_index(&mut self, capacity: usize) {
        #[cfg(test)]
        {
            self.opaque_style_group_index_bytes += capacity * std::mem::size_of::<usize>();
        }
        #[cfg(not(test))]
        let _ = capacity;
    }

    fn record_backend_request(&mut self, bytes: usize) {
        #[cfg(test)]
        {
            self.backend_requests += 1;
            self.backend_request_bytes += bytes;
        }
        #[cfg(not(test))]
        let _ = bytes;
    }

    fn record_builtin_stream(&mut self, text: &str) {
        #[cfg(test)]
        {
            self.builtin_stream_bytes += text.len();
            self.builtin_stream_scalars += text.chars().count();
        }
        #[cfg(not(test))]
        let _ = text;
    }

    fn record_builtin_profile_initialization(&mut self) {
        #[cfg(test)]
        {
            self.builtin_profile_initializations += 1;
        }
    }

    fn record_builtin_profile_cache(&mut self, bytes: usize) {
        #[cfg(test)]
        {
            self.builtin_profile_cache_bytes += bytes;
        }
        #[cfg(not(test))]
        let _ = bytes;
    }

    fn record_builtin_group_state_copy(&mut self) {
        #[cfg(test)]
        {
            self.builtin_group_state_copies += 1;
        }
    }

    fn record_builtin_checkpoint_copy(&mut self) {
        #[cfg(test)]
        {
            self.builtin_checkpoint_copies += 1;
        }
    }

    #[cfg(test)]
    fn logical_temporary_bytes(&self) -> usize {
        self.source_bytes
            + self.break_segments * std::mem::size_of::<&str>()
            + self.fragment_capacity_refs * std::mem::size_of::<InlineRunFragment>()
            + self.break_segments * std::mem::size_of::<InlineBreakSegment>()
            + self.line_plan_bytes
            + self.merged_scratch_bytes
            + self.opaque_style_group_index_bytes
            + self.builtin_profile_cache_bytes
    }
}

#[derive(Debug)]
struct InlineBreakPlan {
    source: String,
    fragments: Vec<InlineRunFragment>,
    segments: Vec<InlineBreakSegment>,
    opaque_style_group_ends: Vec<usize>,
}

fn index_inline_run_breaks(
    runs: &[InlineTextRun],
    stats: &mut InlineHtmlPlanningStats,
) -> InlineBreakPlan {
    let source_bytes = runs.iter().map(|run| run.text.len()).sum();
    stats.record_source(source_bytes, runs.len());
    if source_bytes == 0 {
        stats.record_segment();
        let source = String::new();
        stats.record_source_range(&source);
        return InlineBreakPlan {
            source,
            fragments: Vec::new(),
            segments: vec![InlineBreakSegment {
                fragment_start: 0,
                fragment_end: 0,
                end: HtmlBreakKind::Mandatory,
            }],
            opaque_style_group_ends: Vec::new(),
        };
    }

    // Unicode line breaking needs the logical text as one string. Build it once, then advance a
    // single run cursor across the resulting byte offsets. A run is never restarted for each
    // segment, and segment text is never copied into owned run objects.
    let mut text = String::with_capacity(source_bytes);
    for run in runs {
        text.push_str(&run.text);
    }
    stats.record_source_range(&text);
    let break_segments =
        match html_break_spaces_segments_with_metadata_and_checkpoints(&text, &mut || {
            Ok::<(), std::convert::Infallible>(())
        }) {
            Ok(segments) => segments,
            Err(error) => match error {},
        };

    let fragment_capacity = runs
        .len()
        .saturating_add(break_segments.len().saturating_sub(1));
    stats.record_fragment_capacity(fragment_capacity);
    let mut fragments = Vec::with_capacity(fragment_capacity);
    let mut segments = Vec::with_capacity(break_segments.len());
    let mut segment_start = 0usize;
    let mut run_index = 0usize;
    let mut run_start = 0usize;

    for break_segment in break_segments {
        let segment_end = segment_start + break_segment.text.len();
        let fragment_start = fragments.len();
        while run_index < runs.len() && run_start < segment_end {
            let run = &runs[run_index];
            let run_end = run_start + run.text.len();
            stats.record_indexed_run();
            if run_end <= segment_start {
                run_start = run_end;
                run_index += 1;
                continue;
            }

            let overlap_start = segment_start.max(run_start);
            let overlap_end = segment_end.min(run_end);
            if overlap_start < overlap_end {
                fragments.push(InlineRunFragment {
                    run_index,
                    start: overlap_start,
                    end: overlap_end,
                });
                stats.record_fragment();
            }

            if run_end <= segment_end {
                run_start = run_end;
                run_index += 1;
            } else {
                break;
            }
        }
        segments.push(InlineBreakSegment {
            fragment_start,
            fragment_end: fragments.len(),
            end: break_segment.end,
        });
        stats.record_segment();
        segment_start = segment_end;
    }

    InlineBreakPlan {
        source: text,
        fragments,
        segments,
        opaque_style_group_ends: Vec::new(),
    }
}

fn index_opaque_inline_style_groups(
    runs: &[InlineTextRun],
    breaks: &mut InlineBreakPlan,
    stats: &mut InlineHtmlPlanningStats,
) {
    breaks.opaque_style_group_ends = vec![0; breaks.fragments.len()];
    stats.record_opaque_style_group_index(breaks.opaque_style_group_ends.capacity());
    let mut group_end = breaks.fragments.len();
    for index in (0..breaks.fragments.len()).rev() {
        if index + 1 == breaks.fragments.len()
            || !same_inline_style(
                &runs[breaks.fragments[index].run_index],
                &runs[breaks.fragments[index + 1].run_index],
            )
        {
            group_end = index + 1;
        }
        breaks.opaque_style_group_ends[index] = group_end;
        stats.record_fragment_measure_visits(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InlineStyleKey {
    bold: bool,
    italic: bool,
    code: bool,
}

impl InlineStyleKey {
    const fn from_run(run: &InlineTextRun) -> Self {
        Self {
            bold: run.bold,
            italic: run.italic,
            code: run.code,
        }
    }

    fn resolve(self, base: &TextStyle) -> TextStyle {
        inline_text_style(base, self.bold, self.italic, self.code)
    }

    const fn index(self) -> usize {
        (self.bold as usize) | ((self.italic as usize) << 1) | ((self.code as usize) << 2)
    }
}

fn same_inline_style(left: &InlineTextRun, right: &InlineTextRun) -> bool {
    InlineStyleKey::from_run(left) == InlineStyleKey::from_run(right)
}

fn measure_inline_fragments_width_px<M: TextMeasurer + ?Sized>(
    measurer: &M,
    runs: &[InlineTextRun],
    breaks: &InlineBreakPlan,
    fragment_start: usize,
    fragment_end: usize,
    style: &TextStyle,
    stats: &mut InlineHtmlPlanningStats,
) -> f64 {
    measure_inline_fragments_with_break_width_px(
        measurer,
        runs,
        breaks,
        fragment_start,
        fragment_end,
        HtmlBreakKind::Ordinary,
        style,
        stats,
    )
}

#[allow(clippy::too_many_arguments)]
fn measure_inline_fragments_with_break_width_px<M: TextMeasurer + ?Sized>(
    measurer: &M,
    runs: &[InlineTextRun],
    breaks: &InlineBreakPlan,
    fragment_start: usize,
    fragment_end: usize,
    selected_break: HtmlBreakKind,
    style: &TextStyle,
    stats: &mut InlineHtmlPlanningStats,
) -> f64 {
    // Keep every request observable. `TextMeasurer` may route to a stateful or fallible host, so
    // this shared planner cannot reuse widths unless the complete operation owns a private
    // built-in measurement carrier.
    let mut width = 0.0;
    let mut index = fragment_start;
    while index < fragment_end {
        stats.record_fragment_measure_visits(1);
        let first = breaks.fragments[index];
        let first_run = &runs[first.run_index];
        let end = breaks.opaque_style_group_ends[index].min(fragment_end);
        let last = breaks.fragments[end - 1];
        let text = &breaks.source[first.start..last.end];
        // The inserted glyph belongs to the run containing U+00AD. Measuring it together with
        // that complete style group preserves shaping; a separately measured hyphen is not an
        // additive substitute for the host's width of the resulting string.
        let text = html_line_text_with_break(
            text,
            end == fragment_end && selected_break == HtmlBreakKind::SoftHyphen,
        );
        stats.record_source_slice(&text);
        let run_style = inline_text_style(style, first_run.bold, first_run.italic, first_run.code);
        stats.record_backend_request(text.len());
        width += measurer.measure_html_line_width_px(&text, &run_style);
        index = end;
    }
    width
}

#[derive(Debug, Clone)]
struct BuiltinInlineGroupWidth {
    style: InlineStyleKey,
    width: BuiltinHtmlLineWidth,
}

struct BuiltinInlineWidthProfiles {
    carrier: InlineHtmlMeasurementCarrier,
    initial: [Option<BuiltinHtmlLineWidth>; 8],
}

impl BuiltinInlineWidthProfiles {
    fn new(carrier: InlineHtmlMeasurementCarrier) -> Self {
        debug_assert!(carrier.is_builtin());
        Self {
            carrier,
            initial: std::array::from_fn(|_| None),
        }
    }

    fn begin(
        &mut self,
        style: InlineStyleKey,
        base_style: &TextStyle,
        stats: &mut InlineHtmlPlanningStats,
    ) -> BuiltinHtmlLineWidth {
        let initial = &mut self.initial[style.index()];
        if initial.is_none() {
            let run_style = style.resolve(base_style);
            *initial = Some(
                self.carrier
                    .begin_inline_html_width(&run_style)
                    .expect("built-in inline route was validated before planning"),
            );
            stats.record_builtin_profile_initialization();
        }
        stats.record_builtin_group_state_copy();
        initial
            .as_ref()
            .expect("inline profile was initialized")
            .clone()
    }
}

impl BuiltinInlineGroupWidth {
    fn new(
        profiles: &mut BuiltinInlineWidthProfiles,
        style: InlineStyleKey,
        base_style: &TextStyle,
        stats: &mut InlineHtmlPlanningStats,
    ) -> Self {
        Self {
            style,
            width: profiles.begin(style, base_style, stats),
        }
    }

    fn push_text(&mut self, text: &str, stats: &mut InlineHtmlPlanningStats) {
        stats.record_builtin_stream(text);
        self.width.push_text(text);
    }

    fn width_px(&self) -> f64 {
        self.width.width_px()
    }
}

#[derive(Debug, Clone, Default)]
struct BuiltinInlineLineWidth {
    width_before_last: f64,
    last_group: Option<BuiltinInlineGroupWidth>,
}

impl BuiltinInlineLineWidth {
    fn is_empty(&self) -> bool {
        self.last_group.is_none()
    }

    fn width_px(&self) -> f64 {
        self.last_group
            .as_ref()
            .map_or(0.0, |last| self.width_before_last + last.width_px())
    }

    fn push_fragments(
        &mut self,
        profiles: &mut BuiltinInlineWidthProfiles,
        runs: &[InlineTextRun],
        source: &str,
        fragments: &[InlineRunFragment],
        style: &TextStyle,
        stats: &mut InlineHtmlPlanningStats,
    ) {
        for fragment in fragments {
            stats.record_fragment_measure_visits(1);
            let run = &runs[fragment.run_index];
            let run_style = InlineStyleKey::from_run(run);
            if !self
                .last_group
                .as_ref()
                .is_some_and(|group| group.style == run_style)
            {
                if let Some(last) = self.last_group.take() {
                    self.width_before_last += last.width_px();
                }
                self.last_group = Some(BuiltinInlineGroupWidth::new(
                    profiles, run_style, style, stats,
                ));
            }
            self.last_group
                .as_mut()
                .expect("inline group was initialized")
                .push_text(&source[fragment.start..fragment.end], stats);
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct InlineLinePlan {
    fragment_start: usize,
    fragment_end: usize,
}

impl InlineLinePlan {
    fn from_segment(segment: InlineBreakSegment) -> Self {
        Self {
            fragment_start: segment.fragment_start,
            fragment_end: segment.fragment_end,
        }
    }

    fn is_empty(self) -> bool {
        self.fragment_start == self.fragment_end
    }

    fn append(&mut self, segment: InlineBreakSegment, stats: &mut InlineHtmlPlanningStats) -> Self {
        let checkpoint = *self;
        if self.is_empty() {
            *self = Self::from_segment(segment);
        } else {
            debug_assert_eq!(self.fragment_end, segment.fragment_start);
            self.fragment_end = segment.fragment_end;
        }
        stats.record_line_plan_update();
        checkpoint
    }
}

#[derive(Debug, Clone, Copy)]
struct InlineHtmlLineLayout {
    natural_width: f64,
    wrapped_width: f64,
    min_content_width: f64,
    line_count: usize,
}

#[cfg(test)]
fn measure_inline_html_line_layout<M: TextMeasurer + ?Sized>(
    measurer: &M,
    runs: &[InlineTextRun],
    style: &TextStyle,
    max_width: Option<f64>,
) -> InlineHtmlLineLayout {
    let mut stats = InlineHtmlPlanningStats::default();
    measure_inline_html_line_layout_with_carrier_and_stats(
        measurer,
        InlineHtmlMeasurementCarrier::opaque(),
        runs,
        style,
        max_width,
        &mut stats,
    )
}

#[cfg(test)]
fn measure_inline_html_line_layout_with_stats<M: TextMeasurer + ?Sized>(
    measurer: &M,
    runs: &[InlineTextRun],
    style: &TextStyle,
    max_width: Option<f64>,
    stats: &mut InlineHtmlPlanningStats,
) -> InlineHtmlLineLayout {
    measure_inline_html_line_layout_with_carrier_and_stats(
        measurer,
        InlineHtmlMeasurementCarrier::opaque(),
        runs,
        style,
        max_width,
        stats,
    )
}

fn measure_inline_html_line_layout_with_carrier<M: TextMeasurer + ?Sized>(
    measurer: &M,
    carrier: InlineHtmlMeasurementCarrier,
    runs: &[InlineTextRun],
    style: &TextStyle,
    max_width: Option<f64>,
) -> InlineHtmlLineLayout {
    let mut stats = InlineHtmlPlanningStats::default();
    measure_inline_html_line_layout_with_carrier_and_stats(
        measurer, carrier, runs, style, max_width, &mut stats,
    )
}

fn measure_inline_html_line_layout_with_carrier_and_stats<M: TextMeasurer + ?Sized>(
    measurer: &M,
    carrier: InlineHtmlMeasurementCarrier,
    runs: &[InlineTextRun],
    style: &TextStyle,
    max_width: Option<f64>,
    stats: &mut InlineHtmlPlanningStats,
) -> InlineHtmlLineLayout {
    let natural_width = runs
        .iter()
        .filter(|run| !run.text.is_empty())
        .map(|run| {
            let run_style = inline_text_style(style, run.bold, run.italic, run.code);
            stats.record_backend_request(run.text.len());
            measure_inline_run_width_px(measurer, &run.text, &run_style, WrapMode::HtmlLike, false)
        })
        .sum();
    if carrier.is_builtin() {
        let active_max_width = max_width.filter(|width| width.is_finite() && *width > 0.0);
        if active_max_width.is_none_or(|max_width| natural_width <= max_width) {
            // Min-content cannot affect the public result while the natural line already fits (or
            // wrapping is disabled). Returning the natural width as a conservative private value
            // avoids indexing every break segment on the dominant built-in fast path; it is at
            // most the active maximum and therefore cannot widen another overflowing line.
            return InlineHtmlLineLayout {
                natural_width,
                wrapped_width: natural_width,
                min_content_width: natural_width,
                line_count: 1,
            };
        }

        let breaks = index_inline_run_breaks(runs, stats);
        return measure_builtin_inline_html_line_layout(
            carrier,
            runs,
            style,
            max_width,
            natural_width,
            &breaks,
            stats,
        );
    }

    let mut breaks = index_inline_run_breaks(runs, stats);
    index_opaque_inline_style_groups(runs, &mut breaks, stats);
    measure_opaque_inline_html_line_layout(
        measurer,
        runs,
        style,
        max_width,
        natural_width,
        &breaks,
        stats,
    )
}

fn measure_opaque_inline_html_line_layout<M: TextMeasurer + ?Sized>(
    measurer: &M,
    runs: &[InlineTextRun],
    style: &TextStyle,
    max_width: Option<f64>,
    natural_width: f64,
    breaks: &InlineBreakPlan,
    stats: &mut InlineHtmlPlanningStats,
) -> InlineHtmlLineLayout {
    if breaks
        .segments
        .iter()
        .any(|segment| segment.end == HtmlBreakKind::SoftHyphen)
    {
        return measure_inline_soft_hyphen_line_layout(
            breaks,
            max_width,
            natural_width,
            stats,
            |line, selected_break, stats| {
                measure_inline_fragments_with_break_width_px(
                    measurer,
                    runs,
                    breaks,
                    line.fragment_start,
                    line.fragment_end,
                    selected_break,
                    style,
                    stats,
                )
            },
        );
    }
    let min_content_width = breaks
        .segments
        .iter()
        .map(|segment| {
            measure_inline_fragments_width_px(
                measurer,
                runs,
                breaks,
                segment.fragment_start,
                segment.fragment_end,
                style,
                stats,
            )
        })
        .fold(0.0_f64, f64::max);

    let Some(max_width) = max_width.filter(|width| width.is_finite() && *width > 0.0) else {
        return InlineHtmlLineLayout {
            natural_width,
            wrapped_width: natural_width,
            min_content_width,
            line_count: 1,
        };
    };
    if natural_width <= max_width {
        return InlineHtmlLineLayout {
            natural_width,
            wrapped_width: natural_width,
            min_content_width,
            line_count: 1,
        };
    }

    // A candidate line is always one contiguous range in the indexed fragment table. Appending
    // and rolling back therefore update two offsets instead of copying a growing fragment vector.
    let mut current = InlineLinePlan::default();
    stats.record_line_plan(std::mem::size_of::<InlineLinePlan>());
    let mut wrapped_width = 0.0_f64;
    let mut line_count = 0usize;
    for segment in breaks.segments.iter().copied() {
        let checkpoint = current.append(segment, stats);
        let candidate_width = measure_inline_fragments_width_px(
            measurer,
            runs,
            breaks,
            current.fragment_start,
            current.fragment_end,
            style,
            stats,
        );
        if checkpoint.is_empty() || candidate_width <= max_width {
            continue;
        }

        current = checkpoint;
        wrapped_width = wrapped_width.max(measure_inline_fragments_width_px(
            measurer,
            runs,
            breaks,
            current.fragment_start,
            current.fragment_end,
            style,
            stats,
        ));
        line_count += 1;
        current = InlineLinePlan::from_segment(segment);
        stats.record_line_plan_update();
    }

    if !current.is_empty() {
        wrapped_width = wrapped_width.max(measure_inline_fragments_width_px(
            measurer,
            runs,
            breaks,
            current.fragment_start,
            current.fragment_end,
            style,
            stats,
        ));
        line_count += 1;
    }

    InlineHtmlLineLayout {
        natural_width,
        wrapped_width,
        min_content_width,
        line_count: line_count.max(1),
    }
}

fn measure_builtin_inline_html_line_layout(
    carrier: InlineHtmlMeasurementCarrier,
    runs: &[InlineTextRun],
    style: &TextStyle,
    max_width: Option<f64>,
    natural_width: f64,
    breaks: &InlineBreakPlan,
    stats: &mut InlineHtmlPlanningStats,
) -> InlineHtmlLineLayout {
    let mut profiles = BuiltinInlineWidthProfiles::new(carrier);
    stats.record_builtin_profile_cache(std::mem::size_of::<BuiltinInlineWidthProfiles>());
    if breaks
        .segments
        .iter()
        .any(|segment| segment.end == HtmlBreakKind::SoftHyphen)
    {
        let mut measured_line = InlineLinePlan::default();
        let mut current_width = BuiltinInlineLineWidth::default();
        return measure_inline_soft_hyphen_line_layout(
            breaks,
            max_width,
            natural_width,
            stats,
            |line, selected_break, stats| {
                if measured_line.fragment_start != line.fragment_start
                    || measured_line.fragment_end > line.fragment_end
                {
                    current_width = BuiltinInlineLineWidth::default();
                    measured_line = InlineLinePlan {
                        fragment_start: line.fragment_start,
                        fragment_end: line.fragment_start,
                    };
                }
                current_width.push_fragments(
                    &mut profiles,
                    runs,
                    &breaks.source,
                    &breaks.fragments[measured_line.fragment_end..line.fragment_end],
                    style,
                    stats,
                );
                measured_line = line;
                if selected_break == HtmlBreakKind::SoftHyphen {
                    let mut with_hyphen = current_width.clone();
                    stats.record_builtin_checkpoint_copy();
                    if let Some(group) = with_hyphen.last_group.as_mut() {
                        group.push_text("-", stats);
                    }
                    with_hyphen.width_px()
                } else {
                    current_width.width_px()
                }
            },
        );
    }
    let mut min_content_width = 0.0_f64;
    for segment in &breaks.segments {
        let mut segment_width = BuiltinInlineLineWidth::default();
        segment_width.push_fragments(
            &mut profiles,
            runs,
            &breaks.source,
            &breaks.fragments[segment.fragment_start..segment.fragment_end],
            style,
            stats,
        );
        min_content_width = min_content_width.max(segment_width.width_px());
    }

    let Some(max_width) = max_width.filter(|width| width.is_finite() && *width > 0.0) else {
        return InlineHtmlLineLayout {
            natural_width,
            wrapped_width: natural_width,
            min_content_width,
            line_count: 1,
        };
    };
    if natural_width <= max_width {
        return InlineHtmlLineLayout {
            natural_width,
            wrapped_width: natural_width,
            min_content_width,
            line_count: 1,
        };
    }

    let mut current = BuiltinInlineLineWidth::default();
    stats.record_line_plan(std::mem::size_of::<BuiltinInlineLineWidth>());
    let mut wrapped_width = 0.0_f64;
    let mut line_count = 0usize;
    for segment in &breaks.segments {
        let checkpoint = current.clone();
        stats.record_builtin_checkpoint_copy();
        current.push_fragments(
            &mut profiles,
            runs,
            &breaks.source,
            &breaks.fragments[segment.fragment_start..segment.fragment_end],
            style,
            stats,
        );
        stats.record_line_plan_update();
        if checkpoint.is_empty() || current.width_px() <= max_width {
            continue;
        }

        current = checkpoint;
        wrapped_width = wrapped_width.max(current.width_px());
        line_count += 1;
        current = BuiltinInlineLineWidth::default();
        current.push_fragments(
            &mut profiles,
            runs,
            &breaks.source,
            &breaks.fragments[segment.fragment_start..segment.fragment_end],
            style,
            stats,
        );
        stats.record_line_plan_update();
    }

    if !current.is_empty() {
        wrapped_width = wrapped_width.max(current.width_px());
        line_count += 1;
    }

    InlineHtmlLineLayout {
        natural_width,
        wrapped_width,
        min_content_width,
        line_count: line_count.max(1),
    }
}

fn measure_inline_soft_hyphen_line_layout(
    breaks: &InlineBreakPlan,
    max_width: Option<f64>,
    natural_width: f64,
    stats: &mut InlineHtmlPlanningStats,
    mut measure: impl FnMut(InlineLinePlan, HtmlBreakKind, &mut InlineHtmlPlanningStats) -> f64,
) -> InlineHtmlLineLayout {
    // Min-content chooses every available discretionary break. Each selected U+00AD therefore
    // contributes its conditional glyph even though the natural unbroken line hides it.
    let min_content_width = breaks
        .segments
        .iter()
        .map(|segment| measure(InlineLinePlan::from_segment(*segment), segment.end, stats))
        .fold(0.0_f64, f64::max);
    let Some(max_width) = max_width.filter(|width| width.is_finite() && *width > 0.0) else {
        return InlineHtmlLineLayout {
            natural_width,
            wrapped_width: natural_width,
            min_content_width,
            line_count: 1,
        };
    };
    // Table-cell sizing cannot shrink below min-content. Applying the same lower bound when
    // selecting lines avoids an impossible break that would not fit its own conditional glyph.
    let max_width = max_width.max(min_content_width);
    if natural_width <= max_width {
        return InlineHtmlLineLayout {
            natural_width,
            wrapped_width: natural_width,
            min_content_width,
            line_count: 1,
        };
    }

    let mut current = InlineLinePlan::default();
    let mut last_fitting_break: Option<(usize, InlineLinePlan, HtmlBreakKind)> = None;
    let mut wrapped_width = 0.0_f64;
    let mut line_count = 0usize;
    let mut index = 0usize;
    stats.record_line_plan(std::mem::size_of::<InlineLinePlan>());
    while index < breaks.segments.len() {
        let segment = breaks.segments[index];
        current.append(segment, stats);
        let candidate_width = measure(current, HtmlBreakKind::Ordinary, stats);
        if candidate_width > max_width
            && let Some((break_index, line, end)) = last_fitting_break.take()
        {
            wrapped_width = wrapped_width.max(measure(line, end, stats));
            line_count += 1;
            current = InlineLinePlan::default();
            index = break_index + 1;
            continue;
        }
        if index + 1 < breaks.segments.len() && measure(current, segment.end, stats) <= max_width {
            last_fitting_break = Some((index, current, segment.end));
        }
        index += 1;
    }
    if !current.is_empty() {
        wrapped_width = wrapped_width.max(measure(current, HtmlBreakKind::Ordinary, stats));
        line_count += 1;
    }
    InlineHtmlLineLayout {
        natural_width,
        wrapped_width,
        min_content_width,
        line_count: line_count.max(1),
    }
}

fn measure_inline_html_layout<M: TextMeasurer + ?Sized>(
    measurer: &M,
    carrier: InlineHtmlMeasurementCarrier,
    runs_by_line: &[Vec<InlineTextRun>],
    style: &TextStyle,
    max_width: Option<f64>,
) -> InlineHtmlLineLayout {
    let mut layout = InlineHtmlLineLayout {
        natural_width: 0.0,
        wrapped_width: 0.0,
        min_content_width: 0.0,
        line_count: 0,
    };
    for runs in runs_by_line {
        let line =
            measure_inline_html_line_layout_with_carrier(measurer, carrier, runs, style, max_width);
        layout.natural_width = layout.natural_width.max(line.natural_width);
        layout.wrapped_width = layout.wrapped_width.max(line.wrapped_width);
        layout.min_content_width = layout.min_content_width.max(line.min_content_width);
        layout.line_count += line.line_count;
    }
    layout.line_count = layout.line_count.max(1);
    layout
}

fn explicit_inline_html_line_boxes(
    runs_by_line: &[Vec<InlineTextRun>],
    image_on_line: &[bool],
) -> usize {
    // Consecutive `<br>` elements create empty inline line boxes. Keep those boxes up to the last
    // visible text/image line; a final image-only line is left to the browser-dependent replaced
    // element bounds instead of assigning it a guessed intrinsic height.
    runs_by_line
        .iter()
        .zip(image_on_line)
        .rposition(|(runs, has_image)| !runs.is_empty() || *has_image)
        .map(|last_content_line| {
            runs_by_line[..=last_content_line]
                .iter()
                .zip(&image_on_line[..=last_content_line])
                .filter(|(runs, has_image)| !runs.is_empty() || !**has_image)
                .count()
        })
        // With no visible runs, N explicit `<br>` elements produce N line boxes. The parser keeps
        // one initial row plus one row per break, while block-end rows are suppressed below.
        .unwrap_or_else(|| runs_by_line.len().saturating_sub(1))
}

fn finish_inline_html_layout<M: TextMeasurer + ?Sized>(
    measurer: &M,
    carrier: InlineHtmlMeasurementCarrier,
    runs_by_line: &[Vec<InlineTextRun>],
    break_spaces_runs_by_line: Option<&[Vec<InlineTextRun>]>,
    style: &TextStyle,
    max_width: Option<f64>,
    explicit_line_boxes: usize,
) -> TextMetrics {
    let collapsed = measure_inline_html_layout(measurer, carrier, runs_by_line, style, max_width);
    let active_max_width = max_width.filter(|width| width.is_finite() && *width > 0.0);
    let break_spaces_active = active_max_width.is_some_and(|max_width| {
        collapsed.natural_width >= max_width && collapsed.min_content_width <= max_width
    });
    let layout = if break_spaces_active {
        break_spaces_runs_by_line
            .map(|runs| measure_inline_html_layout(measurer, carrier, runs, style, max_width))
            .unwrap_or(collapsed)
    } else {
        collapsed
    };
    let width = if break_spaces_active {
        active_max_width
            .unwrap_or(layout.natural_width)
            .max(layout.min_content_width)
    } else if let Some(max_width) = active_max_width {
        if layout.natural_width > max_width {
            layout
                .wrapped_width
                .max(layout.min_content_width)
                .max(max_width)
        } else {
            layout.natural_width.min(max_width)
        }
    } else {
        layout.natural_width
    };
    TextMetrics {
        width: round_to_1_64_px(width),
        height: layout.line_count.max(explicit_line_boxes) as f64 * style.font_size.max(1.0) * 1.5,
        line_count: layout.line_count.max(explicit_line_boxes),
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod inline_planning_tests {
    use super::*;
    use crate::environment::{
        HostMeasurementResult, HostTextMeasurement, HostTextMeasurementRequest, HostTextMeasurer,
        MeasurementProfileId, RenderEnvironment, TextMeasurementOperation, TextMeasurementPhase,
        TextMeasurementPolicy, TextMeasurementProfileIdentity,
    };
    use crate::text::DeterministicTextMeasurer;
    use std::cell::{Cell, RefCell};
    use std::sync::{Arc, Mutex};

    fn inline_html_carrier<M: TextMeasurer + ?Sized>(measurer: &M) -> InlineHtmlMeasurementCarrier {
        measurer
            .builtin_operation_carrier(TextMeasurementOperation::WrappedWithRawWidth)
            .and_then(BuiltinTextMeasurementOperationCarrier::into_inline_html)
            .unwrap_or_else(InlineHtmlMeasurementCarrier::opaque)
    }

    type RecordedMeasurementCall = (String, Option<String>, Option<String>, Option<String>);

    #[derive(Default)]
    struct RecordingMeasurer {
        calls: RefCell<Vec<RecordedMeasurementCall>>,
    }

    impl TextMeasurer for RecordingMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            self.measure_wrapped(text, style, None, WrapMode::HtmlLike)
        }

        fn measure_wrapped(
            &self,
            text: &str,
            style: &TextStyle,
            _max_width: Option<f64>,
            _wrap_mode: WrapMode,
        ) -> TextMetrics {
            self.calls.borrow_mut().push((
                text.to_string(),
                style.font_weight.clone(),
                style.font_style.clone(),
                style.font_family.clone(),
            ));
            TextMetrics {
                width: text.len() as f64,
                height: style.font_size,
                line_count: 1,
            }
        }
    }

    struct ScriptedStatefulMeasurer {
        widths: Vec<f64>,
        next: Cell<usize>,
        calls: RefCell<Vec<String>>,
    }

    struct RecordingHostMeasurer {
        calls: Arc<Mutex<Vec<(TextMeasurementOperation, String)>>>,
    }

    impl HostTextMeasurer for RecordingHostMeasurer {
        fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            self.calls
                .lock()
                .expect("host call log lock")
                .push((request.operation, request.text.to_string()));
            if request.operation == TextMeasurementOperation::CanvasMeasureTextWidth {
                Ok(Some(HostTextMeasurement::Length(request.text.len() as f64)))
            } else {
                Ok(Some(HostTextMeasurement::Metrics(TextMetrics {
                    width: request.text.len() as f64,
                    height: request.style.font_size,
                    line_count: 1,
                })))
            }
        }
    }

    impl ScriptedStatefulMeasurer {
        fn new(widths: Vec<f64>) -> Self {
            Self {
                widths,
                next: Cell::new(0),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl TextMeasurer for ScriptedStatefulMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            self.measure_wrapped(text, style, None, WrapMode::HtmlLike)
        }

        fn measure_wrapped(
            &self,
            text: &str,
            style: &TextStyle,
            _max_width: Option<f64>,
            _wrap_mode: WrapMode,
        ) -> TextMetrics {
            let index = self.next.get();
            self.next.set(index + 1);
            self.calls.borrow_mut().push(text.to_string());
            TextMetrics {
                width: self.widths[index],
                height: style.font_size,
                line_count: 1,
            }
        }
    }

    fn run(text: impl Into<String>, variant: usize) -> InlineTextRun {
        InlineTextRun {
            text: text.into(),
            bold: variant == 1,
            italic: variant == 2,
            code: variant == 3,
        }
    }

    fn fixed_byte_runs(
        source_bytes: usize,
        run_count: usize,
        break_count: usize,
    ) -> Vec<InlineTextRun> {
        assert!(source_bytes > 0);
        assert!((1..=source_bytes).contains(&run_count));
        assert!(break_count < source_bytes);

        let mut text = vec![b'a'; source_bytes];
        for index in 0..break_count {
            let offset = (index + 1) * source_bytes / (break_count + 1);
            text[offset] = b' ';
        }

        let mut runs = Vec::with_capacity(run_count);
        let mut start = 0usize;
        for index in 0..run_count {
            let end = (index + 1) * source_bytes / run_count;
            runs.push(run(
                String::from_utf8(text[start..end].to_vec()).expect("ASCII workload"),
                index % 4,
            ));
            start = end;
        }
        runs
    }

    fn fragment_text(source: &str, fragments: &[InlineRunFragment]) -> String {
        let mut text = String::new();
        for fragment in fragments {
            text.push_str(&source[fragment.start..fragment.end]);
        }
        text
    }

    #[test]
    fn indexed_break_walk_preserves_unicode_and_cross_run_fragments() {
        let runs = vec![
            run("A\u{301} ", 0),
            run("👩‍💻 ", 1),
            run("مرحبا ", 2),
            run("世界", 3),
        ];
        let expected_text = runs.iter().map(|run| run.text.as_str()).collect::<String>();
        let expected_segments = html_break_spaces_segments(&expected_text);
        let mut stats = InlineHtmlPlanningStats::default();
        let plan = index_inline_run_breaks(&runs, &mut stats);
        let actual_segments = plan
            .segments
            .iter()
            .map(|segment| {
                fragment_text(
                    &plan.source,
                    &plan.fragments[segment.fragment_start..segment.fragment_end],
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            actual_segments,
            expected_segments
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(stats.source_bytes, expected_text.len());
        assert_eq!(stats.break_segments, actual_segments.len());
        assert!(stats.fragment_refs < runs.len() + actual_segments.len());
        assert_eq!(stats.run_visits, runs.len() * 2 + stats.fragment_refs);
    }

    #[derive(Default)]
    struct SoftHyphenMeasurer {
        calls: RefCell<Vec<RecordedMeasurementCall>>,
        kern_hyphen: bool,
    }

    impl TextMeasurer for SoftHyphenMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            self.measure_wrapped(text, style, None, WrapMode::HtmlLike)
        }

        fn measure_wrapped(
            &self,
            text: &str,
            style: &TextStyle,
            _max_width: Option<f64>,
            _wrap_mode: WrapMode,
        ) -> TextMetrics {
            self.calls.borrow_mut().push((
                text.to_string(),
                style.font_weight.clone(),
                style.font_style.clone(),
                style.font_family.clone(),
            ));
            let advance = if style_requests_bold_font_weight(style) {
                20.0
            } else {
                10.0
            };
            let kerning = if self.kern_hyphen && text.len() > 1 && text.ends_with('-') {
                5.0
            } else {
                0.0
            };
            TextMetrics {
                width: text.chars().filter(|ch| *ch != '\u{00ad}').count() as f64 * advance
                    - kerning,
                height: style.font_size,
                line_count: 1,
            }
        }
    }

    #[test]
    fn soft_hyphen_inline_min_content_includes_the_selected_glyph() {
        let measurer = SoftHyphenMeasurer::default();
        let style = TextStyle::default();
        let runs = vec![run("alpha\u{00ad}beta", 0)];
        let layout = measure_inline_html_line_layout(&measurer, &runs, &style, Some(50.0));
        assert_eq!(layout.natural_width, 90.0);
        assert_eq!(layout.min_content_width, 60.0);
        assert_eq!(layout.wrapped_width, 60.0);
        assert_eq!(layout.line_count, 2);

        let unbroken = measure_inline_html_line_layout(&measurer, &runs, &style, None);
        assert_eq!(unbroken.wrapped_width, 90.0);
        assert_eq!(unbroken.line_count, 1);
        let terminal = measure_inline_html_line_layout(
            &measurer,
            &[run("alpha\u{00ad}", 0)],
            &style,
            Some(50.0),
        );
        assert_eq!(terminal.natural_width, 50.0);
        assert_eq!(terminal.min_content_width, 50.0);
        assert_eq!(terminal.line_count, 1);
    }

    #[test]
    fn soft_hyphen_inline_glyph_inherits_its_run_and_parent_style() {
        let measurer = SoftHyphenMeasurer::default();
        let mut style = TextStyle {
            font_family: Some("inherited-family".to_string()),
            ..TextStyle::default()
        };
        let runs = vec![run("al", 0), run("pha\u{00ad}", 1), run("beta", 2)];
        let layout = measure_inline_html_line_layout(&measurer, &runs, &style, Some(100.0));
        assert_eq!(layout.natural_width, 120.0);
        assert_eq!(layout.min_content_width, 100.0);
        assert_eq!(layout.wrapped_width, 100.0);
        assert_eq!(layout.line_count, 2);
        assert!(measurer.calls.borrow().contains(&(
            "pha-".to_string(),
            Some("700".to_string()),
            None,
            Some("inherited-family".to_string()),
        )));

        style.font_weight = Some("700".to_string());
        let layout = measure_inline_html_line_layout(
            &measurer,
            &[run("alpha\u{00ad}beta", 0)],
            &style,
            Some(120.0),
        );
        assert_eq!(layout.min_content_width, 120.0);
        assert_eq!(layout.wrapped_width, 120.0);
        assert_eq!(layout.line_count, 2);
    }

    #[test]
    fn soft_hyphen_inline_rewinds_to_an_earlier_fitting_break() {
        let measurer = SoftHyphenMeasurer::default();
        let layout = measure_inline_html_line_layout(
            &measurer,
            &[run("x alpha\u{00ad}beta", 0)],
            &TextStyle::default(),
            Some(70.0),
        );
        assert_eq!(layout.natural_width, 110.0);
        assert_eq!(layout.min_content_width, 60.0);
        assert_eq!(layout.wrapped_width, 60.0);
        assert_eq!(layout.line_count, 3);
    }

    #[test]
    fn soft_hyphen_inline_callback_shapes_the_complete_visible_style_group() {
        let measurer = SoftHyphenMeasurer {
            kern_hyphen: true,
            ..Default::default()
        };
        let layout = measure_inline_html_line_layout(
            &measurer,
            &[run("al", 0), run("pha\u{00ad}beta", 0)],
            &TextStyle::default(),
            Some(55.0),
        );
        assert_eq!(layout.min_content_width, 55.0);
        assert_eq!(layout.wrapped_width, 55.0);
        assert_eq!(layout.line_count, 2);
        let calls = measurer.calls.borrow();
        assert!(calls.iter().any(|call| call.0 == "alpha-"));
        assert!(calls.iter().all(|call| call.0 != "-"));
    }

    #[test]
    fn soft_hyphen_inline_complete_word_fit_does_not_select_a_wider_hyphen() {
        struct WideHyphenMeasurer;
        impl TextMeasurer for WideHyphenMeasurer {
            fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
                self.measure_wrapped(text, style, None, WrapMode::HtmlLike)
            }

            fn measure_wrapped(
                &self,
                text: &str,
                style: &TextStyle,
                _max_width: Option<f64>,
                _wrap_mode: WrapMode,
            ) -> TextMetrics {
                TextMetrics {
                    width: text
                        .chars()
                        .map(|ch| match ch {
                            '\u{00ad}' => 0.0,
                            '-' => 30.0,
                            _ => 10.0,
                        })
                        .sum(),
                    height: style.font_size,
                    line_count: 1,
                }
            }
        }
        let layout = measure_inline_html_line_layout(
            &WideHyphenMeasurer,
            &[run("alpha\u{00ad}b", 0)],
            &TextStyle::default(),
            Some(60.0),
        );
        assert_eq!(layout.natural_width, 60.0);
        assert_eq!(layout.wrapped_width, 60.0);
        assert_eq!(layout.line_count, 1);
    }

    #[test]
    fn soft_hyphen_inline_html_entity_preserves_conditional_styled_measurement() {
        let measurer = SoftHyphenMeasurer::default();
        let layout = measure_html_with_inline_styles(
            &measurer,
            "<strong>alpha&shy;</strong>beta",
            &TextStyle::default(),
            Some(120.0),
            WrapMode::HtmlLike,
        );
        assert_eq!(layout.width, 120.0);
        assert_eq!(layout.line_count, 2);
        assert!(
            measurer
                .calls
                .borrow()
                .iter()
                .any(|call| { call.0 == "alpha-" && call.1.as_deref() == Some("700") })
        );
    }

    #[test]
    fn soft_hyphen_inline_stream_preserves_combining_boundaries_in_one_chunk() {
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        let measurer = session.text_measurer(TextMeasurementPhase::Wrap);
        let carrier = inline_html_carrier(&measurer);
        let style = TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        };
        let mut profiles = BuiltinInlineWidthProfiles::new(carrier);
        let mut stats = InlineHtmlPlanningStats::default();
        let mut group = BuiltinInlineGroupWidth::new(
            &mut profiles,
            InlineStyleKey::from_run(&run("", 1)),
            &style,
            &mut stats,
        );
        let text = format!("a{}", "\u{00ad}\u{0301}".repeat(8192));
        group.push_text(&text, &mut stats);
        assert_eq!(group.width.pushed_chunks, 1);
        assert_eq!(group.width_px(), 4.3);

        let mut stats = InlineHtmlPlanningStats::default();
        let runs = vec![run(format!("W {text}"), 1)];
        let layout = measure_inline_html_line_layout_with_carrier_and_stats(
            &measurer,
            carrier,
            &runs,
            &style,
            Some(15.0),
            &mut stats,
        );
        assert_eq!(layout.line_count, 2);
        assert!((layout.natural_width - 16.1).abs() < 1.0e-12);
        assert_eq!(stats.source_bytes, runs[0].text.len());
        assert!(
            stats.builtin_stream_bytes <= stats.source_bytes * 3,
            "stats={stats:?}"
        );
    }

    #[test]
    fn soft_hyphen_inline_builtin_and_opaque_routes_choose_the_same_lines() {
        let session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("deterministic render session");
        let builtin = session.text_measurer(TextMeasurementPhase::Wrap);
        let deterministic = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        for runs in [
            vec![run("alpha\u{00ad}beta", 0)],
            vec![run("x ", 2), run("alpha\u{00ad}", 1), run("beta", 0)],
            vec![run("ab\u{00ad}cd\u{00ad}ef\u{00ad}gh", 0)],
        ] {
            for max_width in [20.0, 32.0, 48.0] {
                let expected =
                    measure_inline_html_line_layout(&deterministic, &runs, &style, Some(max_width));
                let actual = measure_inline_html_line_layout_with_carrier(
                    &builtin,
                    inline_html_carrier(&builtin),
                    &runs,
                    &style,
                    Some(max_width),
                );
                if expected.natural_width > max_width {
                    assert_layout_eq(actual, expected);
                } else {
                    assert_eq!(actual.line_count, expected.line_count);
                    assert_eq!(actual.wrapped_width, expected.wrapped_width);
                }
            }
        }
    }

    #[test]
    fn opaque_measurer_observes_the_legacy_request_order() {
        let measurer = RecordingMeasurer::default();
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let runs = vec![run("aa ", 0), run("BB", 1), run(" cc", 0)];
        let mut stats = InlineHtmlPlanningStats::default();

        let layout = measure_inline_html_line_layout_with_stats(
            &measurer,
            &runs,
            &style,
            Some(4.0),
            &mut stats,
        );

        let calls = measurer.calls.into_inner();
        let regular = |text: &str| (text.to_string(), None, None, Some("sans-serif".to_string()));
        let bold = |text: &str| {
            (
                text.to_string(),
                Some("700".to_string()),
                None,
                Some("sans-serif".to_string()),
            )
        };
        assert_eq!(
            calls,
            vec![
                regular("aa "),
                bold("BB"),
                regular(" cc"),
                regular("aa "),
                bold("BB"),
                regular(" "),
                regular("cc"),
                regular("aa "),
                regular("aa "),
                bold("BB"),
                regular(" "),
                regular("aa "),
                bold("BB"),
                regular(" cc"),
                bold("BB"),
                regular(" "),
                regular("cc"),
            ]
        );
        assert_eq!(layout.line_count, 3);
        assert_eq!(layout.natural_width, 8.0);
        assert_eq!(layout.min_content_width, 3.0);
        assert_eq!(layout.wrapped_width, 3.0);
        assert_eq!(stats.backend_requests, calls.len());
        assert_eq!(
            stats.backend_request_bytes,
            calls.iter().map(|(text, ..)| text.len()).sum::<usize>()
        );
    }

    #[test]
    fn opaque_same_style_fragments_preserve_payload_order_without_recopying() {
        let measurer = RecordingMeasurer::default();
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let runs = vec![run("aa ", 0), run("bb ", 0), run("cc", 0)];
        let mut stats = InlineHtmlPlanningStats::default();

        let layout = measure_inline_html_line_layout_with_stats(
            &measurer,
            &runs,
            &style,
            Some(4.0),
            &mut stats,
        );

        assert_eq!(
            measurer
                .calls
                .into_inner()
                .into_iter()
                .map(|(text, ..)| text)
                .collect::<Vec<_>>(),
            vec![
                "aa ", "bb ", "cc", "aa ", "bb ", "cc", "aa ", "aa bb ", "aa ", "bb cc", "bb ",
                "cc",
            ]
        );
        assert_eq!(layout.line_count, 3);
        assert_eq!(stats.measurement_payload_copy_bytes, 0);
    }

    #[test]
    fn opaque_public_html_operation_preserves_the_full_request_order() {
        let measurer = RecordingMeasurer::default();
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };

        let metrics = measure_html_with_inline_styles(
            &measurer,
            "aa <strong>BB</strong> cc",
            &style,
            Some(4.0),
            WrapMode::HtmlLike,
        );

        let calls = measurer.calls.into_inner();
        let regular = |text: &str| (text.to_string(), None, None, Some("sans-serif".to_string()));
        let bold = |text: &str| {
            (
                text.to_string(),
                Some("700".to_string()),
                None,
                Some("sans-serif".to_string()),
            )
        };
        let layout_pass = vec![
            regular("aa "),
            bold("BB"),
            regular(" cc"),
            regular("aa "),
            bold("BB"),
            regular(" "),
            regular("cc"),
            regular("aa "),
            regular("aa "),
            bold("BB"),
            regular(" "),
            regular("aa "),
            bold("BB"),
            regular(" cc"),
            bold("BB"),
            regular(" "),
            regular("cc"),
        ];
        let mut expected = vec![
            regular("aa BB cc"),
            regular("aa "),
            bold("BB"),
            regular(" cc"),
        ];
        // Mermaid remeasures after switching from nowrap to break-spaces at the width boundary.
        expected.extend(layout_pass.iter().cloned());
        expected.extend(layout_pass);
        assert_eq!(calls, expected);
        assert_eq!(metrics.width, 4.0);
        assert_eq!(metrics.line_count, 3);
    }

    #[test]
    fn opaque_public_html_operation_preserves_failure_position() {
        struct PanickingMeasurer {
            panic_at: usize,
            calls: RefCell<Vec<String>>,
        }

        impl TextMeasurer for PanickingMeasurer {
            fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
                self.measure_wrapped(text, style, None, WrapMode::HtmlLike)
            }

            fn measure_wrapped(
                &self,
                text: &str,
                style: &TextStyle,
                _max_width: Option<f64>,
                _wrap_mode: WrapMode,
            ) -> TextMetrics {
                let call_index = self.calls.borrow().len() + 1;
                self.calls.borrow_mut().push(text.to_string());
                assert_ne!(call_index, self.panic_at, "scripted measurement failure");
                TextMetrics {
                    width: text.len() as f64,
                    height: style.font_size,
                    line_count: 1,
                }
            }
        }

        let measurer = PanickingMeasurer {
            panic_at: 5,
            calls: RefCell::new(Vec::new()),
        };
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            measure_html_with_inline_styles(
                &measurer,
                "aa <strong>BB</strong> cc",
                &style,
                Some(4.0),
                WrapMode::HtmlLike,
            )
        }));

        assert!(result.is_err());
        assert_eq!(
            measurer.calls.into_inner(),
            vec!["aa BB cc", "aa ", "BB", " cc", "aa "]
        );
    }

    #[test]
    fn opaque_stateful_measurer_keeps_fit_and_overflow_call_semantics() {
        let measurer = ScriptedStatefulMeasurer::new(vec![
            10.0, 10.0, 10.0, // natural width
            3.0, 2.0, 1.0, 2.0, // min-content segments
            3.0, // first candidate
            3.0, 2.0, 1.0, // second candidate fits
            20.0, 2.0, 4.0, // third candidate overflows
            3.0, 2.0, 1.0, // committed checkpoint
            2.0, // final segment
        ]);
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let runs = vec![run("aa ", 0), run("BB", 1), run(" cc", 0)];
        let mut stats = InlineHtmlPlanningStats::default();

        let layout = measure_inline_html_line_layout_with_stats(
            &measurer,
            &runs,
            &style,
            Some(25.0),
            &mut stats,
        );

        assert_eq!(
            measurer.calls.into_inner(),
            vec![
                "aa ", "BB", " cc", "aa ", "BB", " ", "cc", "aa ", "aa ", "BB", " ", "aa ", "BB",
                " cc", "aa ", "BB", " ", "cc",
            ]
        );
        assert_eq!(layout.natural_width, 30.0);
        assert_eq!(layout.min_content_width, 3.0);
        assert_eq!(layout.wrapped_width, 6.0);
        assert_eq!(layout.line_count, 2);
        assert_eq!(stats.backend_requests, 18);
    }

    #[test]
    fn host_route_keeps_the_full_opaque_html_operation_trace() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let identity = TextMeasurementProfileIdentity::new(
            MeasurementProfileId::new("test.inline-recording-host")
                .expect("valid measurement profile id"),
            "v1",
        )
        .expect("valid measurement profile identity");
        let policy = TextMeasurementPolicy::host_display(
            identity,
            Arc::new(RecordingHostMeasurer {
                calls: Arc::clone(&calls),
            }),
            [TextMeasurementPhase::Wrap],
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session()
            .expect("begin host render session");
        let measurer = session.text_measurer(TextMeasurementPhase::Wrap);
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let metrics = measure_html_with_inline_styles(
            &measurer,
            "aa <strong>BB</strong> cc",
            &style,
            Some(4.0),
            WrapMode::HtmlLike,
        );

        let calls = calls.lock().expect("host call log lock").clone();
        assert_eq!(calls[0].0, TextMeasurementOperation::Wrapped);
        assert!(calls[1..].iter().all(|(operation, _)| {
            *operation == TextMeasurementOperation::CanvasMeasureTextWidth
        }));
        assert_eq!(
            calls
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>(),
            {
                let layout_pass = [
                    "aa ", "BB", " cc", "aa ", "BB", " ", "cc", "aa ", "aa ", "BB", " ", "aa ",
                    "BB", " cc", "BB", " ", "cc",
                ];
                let mut expected = vec!["aa BB cc", "aa ", "BB", " cc"];
                expected.extend(layout_pass);
                expected.extend(layout_pass);
                expected
            }
        );
        assert_eq!(metrics.width, 4.0);
        assert_eq!(metrics.line_count, 3);
    }

    fn assert_layout_eq(left: InlineHtmlLineLayout, right: InlineHtmlLineLayout) {
        assert!((left.natural_width - right.natural_width).abs() <= 1.0e-9);
        assert!((left.wrapped_width - right.wrapped_width).abs() <= 1.0e-9);
        assert!((left.min_content_width - right.min_content_width).abs() <= 1.0e-9);
        assert_eq!(left.line_count, right.line_count);
    }

    fn assert_layout_bits_eq(
        left: InlineHtmlLineLayout,
        right: InlineHtmlLineLayout,
        max_width: f64,
    ) {
        assert_eq!(
            left.natural_width.to_bits(),
            right.natural_width.to_bits(),
            "natural width mismatch at max_width={max_width}: left={left:?}, right={right:?}"
        );
        assert_eq!(
            left.wrapped_width.to_bits(),
            right.wrapped_width.to_bits(),
            "wrapped width mismatch at max_width={max_width}: left={left:?}, right={right:?}"
        );
        if right.natural_width > max_width {
            assert_eq!(
                left.min_content_width.to_bits(),
                right.min_content_width.to_bits()
            );
        }
        assert_eq!(
            left.line_count, right.line_count,
            "line-count mismatch at max_width={max_width}: left={left:?}, right={right:?}"
        );
    }

    fn next_down(value: f64) -> f64 {
        debug_assert!(value.is_finite() && value > 0.0);
        f64::from_bits(value.to_bits() - 1)
    }

    fn next_up(value: f64) -> f64 {
        debug_assert!(value.is_finite() && value >= 0.0);
        f64::from_bits(value.to_bits() + 1)
    }

    #[test]
    fn qualified_builtin_routes_preserve_ulp_wrap_boundaries_and_backend_sum_order() {
        fn assert_backend(
            backend: &dyn TextMeasurer,
            routed: &dyn TextMeasurer,
            carrier: InlineHtmlMeasurementCarrier,
            style: &TextStyle,
        ) {
            let runs = vec![run("AV ", 0), run("To ", 1), run("fi", 0)];
            let bold_style = inline_text_style(style, true, false, false);
            let boundary = backend
                .measure_wrapped("AV ", style, None, WrapMode::HtmlLike)
                .width
                + backend
                    .measure_wrapped("To ", &bold_style, None, WrapMode::HtmlLike)
                    .width;

            for max_width in [next_down(boundary), boundary, next_up(boundary)] {
                let expected =
                    measure_inline_html_line_layout(backend, &runs, style, Some(max_width));
                let actual = measure_inline_html_line_layout_with_carrier(
                    routed,
                    carrier,
                    &runs,
                    style,
                    Some(max_width),
                );
                assert_layout_bits_eq(actual, expected, max_width);
            }

            let same_style_runs = vec![run("A", 0), run("V ", 0), run("office ", 0), run("fi", 0)];
            for max_width in [32.0, 64.0, 96.0] {
                let expected = measure_inline_html_line_layout(
                    backend,
                    &same_style_runs,
                    style,
                    Some(max_width),
                );
                let actual = measure_inline_html_line_layout_with_carrier(
                    routed,
                    carrier,
                    &same_style_runs,
                    style,
                    Some(max_width),
                );
                assert_layout_bits_eq(actual, expected, max_width);
            }

            let escaped_break_runs = vec![run("wide<br   />tail end", 0)];
            for max_width in [24.0, 64.0, 128.0] {
                let expected = measure_inline_html_line_layout(
                    backend,
                    &escaped_break_runs,
                    style,
                    Some(max_width),
                );
                let actual = measure_inline_html_line_layout_with_carrier(
                    routed,
                    carrier,
                    &escaped_break_runs,
                    style,
                    Some(max_width),
                );
                assert_layout_bits_eq(actual, expected, max_width);

                let html = "wide&lt;br   /&gt;tail end";
                let expected = measure_html_with_inline_styles(
                    backend,
                    html,
                    style,
                    Some(max_width),
                    WrapMode::HtmlLike,
                );
                let actual = measure_html_with_inline_styles(
                    routed,
                    html,
                    style,
                    Some(max_width),
                    WrapMode::HtmlLike,
                );
                assert_eq!(actual.width.to_bits(), expected.width.to_bits());
                assert_eq!(actual.height.to_bits(), expected.height.to_bits());
                assert_eq!(actual.line_count, expected.line_count);
            }
        }

        let style = TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let builtin_session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("deterministic render session");
        let builtin = builtin_session.text_measurer(TextMeasurementPhase::Wrap);
        let builtin_carrier = inline_html_carrier(&builtin);
        assert_backend(
            &DeterministicTextMeasurer::default(),
            &builtin,
            builtin_carrier,
            &style,
        );
        let mut unknown_font_style = style.clone();
        unknown_font_style.font_family = Some("fixture-private-font".to_string());
        assert_backend(
            &DeterministicTextMeasurer::default(),
            &builtin,
            builtin_carrier,
            &unknown_font_style,
        );

        let deterministic_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::deterministic())
            .begin_session()
            .expect("deterministic text session");
        let deterministic = deterministic_session.text_measurer(TextMeasurementPhase::Wrap);
        assert_backend(
            &DeterministicTextMeasurer::default(),
            &deterministic,
            inline_html_carrier(&deterministic),
            &style,
        );
    }

    #[test]
    fn qualified_builtin_stream_reports_only_actual_backend_requests() {
        let session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("deterministic render session");
        let measurer = session.text_measurer(TextMeasurementPhase::Wrap);
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let html = "alpha <strong>BETA </strong>omega";

        let _ = measure_html_with_inline_styles(
            &measurer,
            html,
            &style,
            Some(48.0),
            WrapMode::HtmlLike,
        );

        let report = session.text_measurement_report();
        assert_eq!(report.entries().len(), 1);
        let summary = &report.entries()[0];
        assert_eq!(
            summary.provenance().operation,
            TextMeasurementOperation::CanvasMeasureTextWidth
        );
        assert!(
            (3..=6).contains(&summary.count()),
            "each of the three visible runs should require at most one fit and one overflow probe"
        );
    }

    #[test]
    fn qualified_builtin_fit_path_skips_break_indexing_and_stream_replay() {
        let session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("deterministic render session");
        let measurer = session.text_measurer(TextMeasurementPhase::Wrap);
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let runs = vec![run("alpha beta ", 0), run("gamma delta", 1)];
        let natural_width =
            measure_inline_runs_width_px(&measurer, &runs, &style, WrapMode::HtmlLike, false);

        for max_width in [None, Some(natural_width), Some(natural_width + 1.0)] {
            let mut stats = InlineHtmlPlanningStats::default();
            let layout = measure_inline_html_line_layout_with_carrier_and_stats(
                &measurer,
                inline_html_carrier(&measurer),
                &runs,
                &style,
                max_width,
                &mut stats,
            );

            assert_eq!(layout.natural_width.to_bits(), natural_width.to_bits());
            assert_eq!(layout.wrapped_width.to_bits(), natural_width.to_bits());
            assert_eq!(layout.line_count, 1);
            assert_eq!(stats.backend_requests, runs.len());
            assert_eq!(stats.break_segments, 0);
            assert_eq!(stats.fragment_refs, 0);
            assert_eq!(stats.builtin_stream_bytes, 0);
            assert_eq!(stats.builtin_checkpoint_copies, 0);
        }
    }

    #[test]
    fn qualified_builtin_routes_match_opaque_layout_for_unicode_and_rollback() {
        let style = TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let runs = vec![
            run("A\u{301} alpha ", 0),
            run("👩‍💻 beta ", 1),
            run("مرحبا gamma ", 2),
            run("世界 delta", 3),
        ];
        let html =
            "A\u{301} alpha <strong>👩‍💻 beta </strong><em>مرحبا gamma </em><code>世界 delta</code>";

        let builtin_session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("deterministic render session");
        let builtin = builtin_session.text_measurer(TextMeasurementPhase::Wrap);
        let builtin_expected = measure_inline_html_line_layout(
            &DeterministicTextMeasurer::default(),
            &runs,
            &style,
            Some(96.0),
        );
        let builtin_actual = measure_inline_html_line_layout_with_carrier(
            &builtin,
            inline_html_carrier(&builtin),
            &runs,
            &style,
            Some(96.0),
        );
        assert_layout_eq(builtin_actual, builtin_expected);
        let builtin_actual =
            measure_html_with_inline_styles(&builtin, html, &style, Some(96.0), WrapMode::HtmlLike);
        let builtin_expected = measure_html_with_inline_styles(
            &DeterministicTextMeasurer::default(),
            html,
            &style,
            Some(96.0),
            WrapMode::HtmlLike,
        );
        assert_eq!(builtin_actual.width, builtin_expected.width);
        assert_eq!(builtin_actual.height, builtin_expected.height);
        assert_eq!(builtin_actual.line_count, builtin_expected.line_count);

        let mut unknown_font_style = style.clone();
        unknown_font_style.font_family = Some("fixture-private-font".to_string());
        let fallback_expected = measure_inline_html_line_layout(
            &DeterministicTextMeasurer::default(),
            &runs,
            &unknown_font_style,
            Some(96.0),
        );
        let fallback_actual = measure_inline_html_line_layout_with_carrier(
            &builtin,
            inline_html_carrier(&builtin),
            &runs,
            &unknown_font_style,
            Some(96.0),
        );
        assert_layout_eq(fallback_actual, fallback_expected);
        let fallback_actual = measure_html_with_inline_styles(
            &builtin,
            html,
            &unknown_font_style,
            Some(96.0),
            WrapMode::HtmlLike,
        );
        let fallback_expected = measure_html_with_inline_styles(
            &DeterministicTextMeasurer::default(),
            html,
            &unknown_font_style,
            Some(96.0),
            WrapMode::HtmlLike,
        );
        assert_eq!(fallback_actual.width, fallback_expected.width);
        assert_eq!(fallback_actual.height, fallback_expected.height);
        assert_eq!(fallback_actual.line_count, fallback_expected.line_count);

        let deterministic_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::deterministic())
            .begin_session()
            .expect("deterministic text session");
        let deterministic = deterministic_session.text_measurer(TextMeasurementPhase::Wrap);
        let deterministic_expected = measure_inline_html_line_layout(
            &DeterministicTextMeasurer::default(),
            &runs,
            &style,
            Some(96.0),
        );
        let deterministic_actual = measure_inline_html_line_layout_with_carrier(
            &deterministic,
            inline_html_carrier(&deterministic),
            &runs,
            &style,
            Some(96.0),
        );
        assert_layout_eq(deterministic_actual, deterministic_expected);
        let deterministic_actual = measure_html_with_inline_styles(
            &deterministic,
            html,
            &style,
            Some(96.0),
            WrapMode::HtmlLike,
        );
        let deterministic_expected = measure_html_with_inline_styles(
            &DeterministicTextMeasurer::default(),
            html,
            &style,
            Some(96.0),
            WrapMode::HtmlLike,
        );
        assert_eq!(deterministic_actual.width, deterministic_expected.width);
        assert_eq!(deterministic_actual.height, deterministic_expected.height);
        assert_eq!(
            deterministic_actual.line_count,
            deterministic_expected.line_count
        );
    }

    #[test]
    fn rich_html_path_handles_many_styles_breaks_entities_and_unicode() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let mut html = String::new();
        for index in 0..1024 {
            let (open, close) = match index % 3 {
                0 => ("<strong>", "</strong>"),
                1 => ("<em>", "</em>"),
                _ => ("<code>", "</code>"),
            };
            html.push_str(open);
            html.push_str("a ");
            html.push_str(close);
        }
        html.push_str("&amp;\r\n<br>👩‍💻 مرحبا 世界");

        let metrics = measure_html_with_inline_styles(
            &measurer,
            &html,
            &style,
            Some(96.0),
            WrapMode::HtmlLike,
        );

        assert!(metrics.width.is_finite() && metrics.width > 0.0);
        assert!(metrics.height.is_finite() && metrics.height > 0.0);
        assert!(metrics.line_count > 2);
    }

    #[test]
    fn inline_planner_work_is_additive_for_fixed_byte_r_by_k_matrix() {
        const SOURCE_BYTES: usize = 4096;
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };

        for run_count in [1, 32, 256] {
            for break_count in [0, 31, 255] {
                let runs = fixed_byte_runs(SOURCE_BYTES, run_count, break_count);
                let measurer = RecordingMeasurer::default();
                let mut stats = InlineHtmlPlanningStats::default();
                let _ = measure_inline_html_line_layout_with_stats(
                    &measurer,
                    &runs,
                    &style,
                    Some(32.0),
                    &mut stats,
                );

                let segment_count = break_count + 1;
                let fragment_bound = run_count + break_count;
                assert_eq!(stats.source_bytes, SOURCE_BYTES);
                assert_eq!(stats.break_segments, segment_count);
                assert!(stats.fragment_refs <= fragment_bound);
                assert_eq!(stats.fragment_capacity_refs, fragment_bound);
                assert_eq!(stats.run_visits, run_count * 2 + stats.fragment_refs);
                assert!(stats.line_plan_updates <= segment_count * 2);
                assert_eq!(stats.line_plan_bytes, std::mem::size_of::<InlineLinePlan>());

                let temporary_byte_bound = SOURCE_BYTES
                    + segment_count * std::mem::size_of::<&str>()
                    + fragment_bound * std::mem::size_of::<InlineRunFragment>()
                    + segment_count * std::mem::size_of::<InlineBreakSegment>()
                    + fragment_bound * std::mem::size_of::<usize>()
                    + std::mem::size_of::<InlineLinePlan>();
                assert!(stats.logical_temporary_bytes() <= temporary_byte_bound);
                assert_eq!(stats.backend_requests, measurer.calls.borrow().len());
                assert_eq!(
                    stats.backend_request_bytes,
                    measurer
                        .calls
                        .borrow()
                        .iter()
                        .map(|(text, ..)| text.len())
                        .sum::<usize>()
                );
            }
        }
    }

    #[test]
    fn opaque_same_style_active_line_work_is_linear_in_fragments_and_requests() {
        const SOURCE_BYTES: usize = 4096;
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };

        for run_count in [32, 128, 256] {
            for segment_count in [32, 64, 128] {
                let mut runs = fixed_byte_runs(SOURCE_BYTES, run_count, segment_count - 1);
                for run in &mut runs {
                    run.bold = false;
                    run.italic = false;
                    run.code = false;
                }
                let measurer = RecordingMeasurer::default();
                let mut stats = InlineHtmlPlanningStats::default();
                let _ = measure_inline_html_line_layout_with_stats(
                    &measurer,
                    &runs,
                    &style,
                    Some(32.0),
                    &mut stats,
                );

                let fragment_requests = stats.backend_requests - run_count;
                assert_eq!(
                    stats.fragment_measure_visits,
                    stats.fragment_refs + fragment_requests,
                    "r={run_count}, k={segment_count}, stats={stats:?}"
                );
                assert_eq!(stats.measurement_payload_copy_bytes, 0);
            }
        }
    }

    #[test]
    fn opaque_same_style_utf8_fragments_borrow_source_across_rollback() {
        let measurer = RecordingMeasurer::default();
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let runs = vec![run("A\u{301} ", 0), run("👩‍💻 ", 0), run("世界", 0)];
        let mut stats = InlineHtmlPlanningStats::default();

        let layout = measure_inline_html_line_layout_with_stats(
            &measurer,
            &runs,
            &style,
            Some(8.0),
            &mut stats,
        );

        assert!(layout.line_count > 1);
        assert_eq!(stats.measurement_payload_copy_bytes, 0);
    }

    #[test]
    fn qualified_builtin_active_line_work_is_linear_for_r_by_k_matrix() {
        const SOURCE_BYTES: usize = 4096;
        let style = TextStyle {
            font_family: Some("sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("deterministic render session");
        let measurer = session.text_measurer(TextMeasurementPhase::Wrap);

        for run_count in [1, 32, 128] {
            for segment_count in [16, 32, 64, 128] {
                let runs = fixed_byte_runs(SOURCE_BYTES, run_count, segment_count - 1);
                let natural_width = measure_inline_runs_width_px(
                    &measurer,
                    &runs,
                    &style,
                    WrapMode::HtmlLike,
                    false,
                );
                let mut stats = InlineHtmlPlanningStats::default();
                let layout = measure_inline_html_line_layout_with_carrier_and_stats(
                    &measurer,
                    inline_html_carrier(&measurer),
                    &runs,
                    &style,
                    Some(natural_width - 1.0e-6),
                    &mut stats,
                );

                let fragment_bound = run_count + segment_count - 1;
                assert_eq!(stats.source_bytes, SOURCE_BYTES);
                assert_eq!(stats.break_segments, segment_count);
                assert!(stats.fragment_refs <= fragment_bound);
                assert!(stats.fragment_measure_visits >= stats.fragment_refs * 2);
                assert!(stats.fragment_measure_visits <= stats.fragment_refs * 3);
                assert!(stats.line_plan_updates <= segment_count * 2);
                assert_eq!(
                    stats.line_plan_bytes,
                    std::mem::size_of::<BuiltinInlineLineWidth>()
                );
                assert_eq!(stats.measurement_payload_copy_bytes, 0);
                assert_eq!(stats.backend_requests, run_count);
                assert_eq!(stats.backend_request_bytes, SOURCE_BYTES);
                assert!(stats.builtin_stream_bytes >= SOURCE_BYTES * 2);
                assert!(
                    stats.builtin_stream_bytes <= SOURCE_BYTES * 3,
                    "r={run_count}, k={segment_count}, stats={stats:?}"
                );
                assert!(stats.builtin_stream_scalars <= stats.builtin_stream_bytes);
                assert_eq!(stats.builtin_profile_initializations, run_count.min(4));
                assert_eq!(
                    stats.builtin_profile_cache_bytes,
                    std::mem::size_of::<BuiltinInlineWidthProfiles>()
                );
                assert!(stats.builtin_group_state_copies <= stats.fragment_measure_visits);
                assert_eq!(stats.builtin_checkpoint_copies, segment_count);
                let temporary_byte_bound = SOURCE_BYTES
                    + segment_count * std::mem::size_of::<&str>()
                    + fragment_bound * std::mem::size_of::<InlineRunFragment>()
                    + segment_count * std::mem::size_of::<InlineBreakSegment>()
                    + std::mem::size_of::<BuiltinInlineLineWidth>()
                    + std::mem::size_of::<BuiltinInlineWidthProfiles>();
                assert!(stats.logical_temporary_bytes() <= temporary_byte_bound);
                assert_eq!(layout.line_count, 2);
            }
        }
    }
}

/// Finds retained inline HTML typography that the caller's measurement policy cannot represent.
/// Admission uses the same Markdown projection and HTML normalization as SVG emission.
#[doc(hidden)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub fn unsupported_html_label_style(
    label: &str,
    label_type: &str,
    config: &merman_core::MermaidConfig,
    unsupported_properties: &[&str],
    control: &merman_core::OperationControl,
) -> Result<Option<String>, merman_core::OperationCancelled> {
    use cssparser::{Delimiter, Parser};
    use quick_xml::{Reader, events::Event};

    control.checkpoint()?;
    let html = if label_type == "markdown" {
        let decoded = crate::entities::decode_mermaid_entities_for_render_text(label);
        let decoded = if decoded.contains("\\\\") {
            std::borrow::Cow::Owned(decoded.replace("\\\\", "\\"))
        } else {
            decoded
        };
        let markdown_auto_wrap = config
            .as_value()
            .get("markdownAutoWrap")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        super::mermaid_markdown_to_html_label_fragment(
            decoded.trim_end_matches('\n'),
            markdown_auto_wrap,
        )
    } else {
        crate::flowchart::flowchart_non_markdown_label_for_html(label, label_type)
    };
    // Sanitizer-removed attributes and encoded angle brackets cannot affect rendered typography.
    let html = crate::xml::normalize_html_fragment_for_xhtml(
        &merman_core::sanitize::sanitize_text(&html, config),
    );
    let mut tags = Reader::from_str(&html);
    tags.config_mut().check_end_names = false;
    tags.config_mut().allow_unmatched_ends = true;
    loop {
        control.checkpoint()?;
        let opening = match tags.read_event() {
            Ok(Event::Start(opening) | Event::Empty(opening)) => opening,
            Ok(Event::Eof) | Err(_) => break,
            _ => continue,
        };
        let Some(style) = opening
            .html_attributes()
            .with_checks(false)
            .filter_map(Result::ok)
            .find(|attr| attr.key.as_ref().eq_ignore_ascii_case("style"))
        else {
            continue;
        };
        let style = merman_core::entities::decode_html_entities_to_unicode(style.value.as_ref());
        let element = opening.name().as_ref().to_ascii_lowercase();
        let mut declarations = Parser::new(&style);
        while !declarations.is_exhausted() {
            control.checkpoint()?;
            let declaration = declarations.parse_until_after(Delimiter::Semicolon, |declaration| {
                let property = declaration.expect_ident_cloned()?.to_ascii_lowercase();
                declaration.expect_colon()?;
                let first = declaration.next().ok().cloned();
                let inherits = matches!(
                    &first,
                    Some(cssparser::Token::Ident(value)) if value.eq_ignore_ascii_case("inherit")
                ) && (declaration.is_exhausted()
                    || (declaration.try_parse(cssparser::parse_important).is_ok()
                        && declaration.is_exhausted()));
                while declaration.next().is_ok() {}
                // Inheritance can override the intrinsic emphasis or code face modeled below.
                let overrides_element = match element.as_str() {
                    "strong" | "b" => matches!(property.as_str(), "font-weight" | "font"),
                    "em" | "i" => matches!(property.as_str(), "font-style" | "font"),
                    "code" => matches!(property.as_str(), "font-family" | "font"),
                    _ => false,
                };
                Ok::<_, cssparser::ParseError<()>>((
                    property,
                    first.is_some(),
                    inherits && !overrides_element,
                ))
            });
            control.checkpoint()?;
            if let Ok((property, true, false)) = declaration
                && (matches!(
                    property.as_str(),
                    "font-size" | "font-family" | "font-weight" | "font-style"
                ) || unsupported_properties
                    .iter()
                    .any(|name| property.eq_ignore_ascii_case(name)))
            {
                return Ok(Some(property));
            }
        }
    }
    Ok(None)
}

pub fn measure_html_with_inline_styles(
    measurer: &dyn TextMeasurer,
    html: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
) -> TextMetrics {
    // Keep the public `TextMeasurer` surface request-oriented. A whole-operation override would
    // let an opaque custom measurer bypass the established callback order; only the unnameable
    // routed carrier may select the built-in streaming implementation.
    let carrier = measurer
        .builtin_operation_carrier(TextMeasurementOperation::WrappedWithRawWidth)
        .and_then(BuiltinTextMeasurementOperationCarrier::into_inline_html)
        .unwrap_or_else(InlineHtmlMeasurementCarrier::opaque);
    carrier.measure_html_with_inline_styles(measurer, html, style, max_width, wrap_mode)
}

impl InlineHtmlMeasurementCarrier {
    pub(crate) fn measure_html_with_inline_styles<M: TextMeasurer + ?Sized>(
        self,
        measurer: &M,
        html: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> TextMetrics {
        measure_html_with_inline_styles_with_carrier(
            measurer, self, html, style, max_width, wrap_mode,
        )
    }
}

fn measure_html_with_inline_styles_with_carrier<M: TextMeasurer + ?Sized>(
    measurer: &M,
    carrier: InlineHtmlMeasurementCarrier,
    html: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
) -> TextMetrics {
    fn html_tag_class_attr(tag: &str) -> Option<String> {
        let lower = tag.to_ascii_lowercase();
        let idx = lower.find("class=")?;
        let rest = tag[idx + 6..].trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }

        let mut it = rest.chars();
        let _ = it.next();
        let mut value = String::new();
        for ch in it {
            if ch == quote {
                break;
            }
            value.push(ch);
        }

        Some(value)
    }

    fn fontawesome_icon_width_px(tag: &str, font_size: f64) -> Option<f64> {
        let class_attr = html_tag_class_attr(tag)?;
        let mut prefix: Option<&str> = None;
        let mut icon: Option<&str> = None;

        for token in class_attr.split_ascii_whitespace() {
            if matches!(token, "fa" | "fab" | "fak" | "fal" | "far" | "fas") {
                prefix = Some(token);
                continue;
            }
            if let Some(name) = token.strip_prefix("fa-") {
                icon = Some(name);
            }
        }

        let prefix = prefix?;
        let icon = icon?;
        let advance_em = match (prefix, icon) {
            ("fa" | "fab" | "fak" | "fal" | "far" | "fas", _) => 1.25,
            _ => return None,
        };

        Some(round_to_1_64_px(font_size.max(1.0) * advance_em))
    }

    // Mermaid supports inline FontAwesome icons via `<i class="fa fa-..."></i>` inside HTML
    // labels. Upstream layout is computed with FontAwesome CSS available, while exported SVGs
    // keep only the empty `<i>` element. Model the layout-time glyph advance explicitly.
    let mut plain = String::new();
    let mut icon_width_px_by_line: Vec<f64> = vec![0.0];
    let mut icon_on_line: Vec<bool> = vec![false];
    let mut image_on_line: Vec<bool> = vec![false];
    let mut inline_runs_by_line: Vec<Vec<InlineTextRun>> = vec![Vec::new()];
    let mut break_spaces_runs_by_line: Option<Vec<Vec<InlineTextRun>>> = None;
    let mut strong_depth: usize = 0;
    let mut em_depth: usize = 0;
    let mut code_depth: usize = 0;
    let mut fa_icon_depth: usize = 0;
    let mut inline_replaced_boundary = false;

    let html = html.replace("\r\n", "\n");
    let mut it = html.chars().peekable();
    let mut entity_reference = String::new();
    while let Some(ch) = it.next() {
        if ch == '<' {
            let mut tag = String::new();
            for c in it.by_ref() {
                if c == '>' {
                    break;
                }
                tag.push(c);
            }
            let tag = trim_html_collapsible_ascii_whitespace(&tag);
            let tag_lower = tag.to_ascii_lowercase();
            let tag_trim = trim_html_collapsible_ascii_whitespace(&tag_lower);
            if tag_trim.starts_with('!') || tag_trim.starts_with('?') {
                continue;
            }
            let is_closing = tag_trim.starts_with('/');
            let name = tag_trim
                .trim_start_matches('/')
                .trim_end_matches('/')
                .split(is_html_collapsible_ascii_whitespace)
                .find(|part| !part.is_empty())
                .unwrap_or("");

            let fontawesome_icon_width = if name == "i" && !is_closing {
                fontawesome_icon_width_px(tag, style.font_size)
            } else {
                None
            };

            match name {
                "strong" | "b" => {
                    if is_closing {
                        strong_depth = strong_depth.saturating_sub(1);
                    } else {
                        strong_depth += 1;
                    }
                }
                "em" | "i" => {
                    if is_closing {
                        if name == "i" && fa_icon_depth > 0 {
                            fa_icon_depth = fa_icon_depth.saturating_sub(1);
                        } else {
                            em_depth = em_depth.saturating_sub(1);
                        }
                    } else if let Some(icon_w) = fontawesome_icon_width {
                        let line_idx = icon_width_px_by_line.len().saturating_sub(1);
                        icon_width_px_by_line[line_idx] += icon_w;
                        if let Some(slot) = icon_on_line.get_mut(line_idx) {
                            *slot = true;
                        }
                        if let Some(runs) = inline_runs_by_line.get_mut(line_idx) {
                            runs.push(InlineTextRun::default());
                        }
                        if let Some(runs) = break_spaces_runs_by_line
                            .as_mut()
                            .and_then(|lines| lines.last_mut())
                        {
                            runs.push(InlineTextRun::default());
                        }
                        inline_replaced_boundary = true;
                        fa_icon_depth += 1;
                    } else {
                        em_depth += 1;
                    }
                }
                "code" => {
                    if is_closing {
                        code_depth = code_depth.saturating_sub(1);
                    } else {
                        code_depth += 1;
                    }
                }
                "img" if !is_closing => {
                    if let Some(slot) = image_on_line.last_mut() {
                        *slot = true;
                    }
                    inline_replaced_boundary = true;
                }
                "br" => {
                    plain.push('\n');
                    icon_width_px_by_line.push(0.0);
                    icon_on_line.push(false);
                    image_on_line.push(false);
                    inline_runs_by_line.push(Vec::new());
                    if let Some(lines) = break_spaces_runs_by_line.as_mut() {
                        lines.push(Vec::new());
                    }
                    inline_replaced_boundary = false;
                }
                "p" | "div" | "li" | "tr" | "ul" | "ol" if is_closing => {
                    // A block end starts the next line only when the current line has content.
                    // Reusing the `<br>` branch here would append a phantom row after
                    // `<p><br></p>` and make blank-only labels impossible to count exactly.
                    if !plain.ends_with('\n') && !plain.is_empty() {
                        plain.push('\n');
                        icon_width_px_by_line.push(0.0);
                        icon_on_line.push(false);
                        image_on_line.push(false);
                        inline_runs_by_line.push(Vec::new());
                        if let Some(lines) = break_spaces_runs_by_line.as_mut() {
                            lines.push(Vec::new());
                        }
                    }
                    inline_replaced_boundary = false;
                }
                _ => {}
            }
            continue;
        }

        let push_char = |decoded: char,
                         plain: &mut String,
                         icon_on_line: &mut Vec<bool>,
                         inline_runs_by_line: &mut Vec<Vec<InlineTextRun>>,
                         break_spaces_runs_by_line: &mut Option<Vec<Vec<InlineTextRun>>>,
                         inline_replaced_boundary: &mut bool| {
            if wrap_mode == WrapMode::HtmlLike
                && is_html_collapsible_ascii_whitespace(decoded)
                && break_spaces_runs_by_line.is_none()
            {
                // The browser decodes entities before it decides whether fixed-width
                // `break-spaces` layout is needed. Lazily fork the exact-text projection at the
                // first decoded whitespace so literal and entity-produced whitespace share the
                // same semantics without duplicating every ordinary label up front.
                *break_spaces_runs_by_line = Some(inline_runs_by_line.clone());
            }
            if let Some(lines) = break_spaces_runs_by_line.as_mut() {
                if decoded == '\n' || decoded == '\r' {
                    lines.push(Vec::new());
                } else if let Some(runs) = lines.last_mut() {
                    push_inline_text_char(
                        runs,
                        decoded,
                        strong_depth > 0,
                        em_depth > 0,
                        code_depth > 0,
                    );
                }
            }
            let decoded = if is_html_collapsible_ascii_whitespace(decoded) {
                ' '
            } else {
                decoded
            };
            if decoded == ' ' {
                let line_has_visual = icon_on_line.last().copied().unwrap_or(false)
                    || image_on_line.last().copied().unwrap_or(false);
                if (plain.ends_with(' ') && !*inline_replaced_boundary)
                    || (plain.ends_with('\n') && !line_has_visual)
                    || (plain.is_empty() && !line_has_visual)
                {
                    return;
                }
            }
            plain.push(decoded);
            if let Some(runs) = inline_runs_by_line.last_mut() {
                push_inline_text_char(
                    runs,
                    decoded,
                    strong_depth > 0,
                    em_depth > 0,
                    code_depth > 0,
                );
            }
            *inline_replaced_boundary = false;
        };

        if ch == '&' {
            entity_reference.clear();
            entity_reference.push('&');
            let mut saw_semicolon = false;
            while let Some(&c) = it.peek() {
                if c == ';' {
                    it.next();
                    saw_semicolon = true;
                    break;
                }
                if c == '<'
                    || c == '&'
                    || is_html_collapsible_ascii_whitespace(c)
                    || entity_reference.len() > 65
                {
                    break;
                }
                entity_reference.push(c);
                it.next();
            }
            if saw_semicolon {
                entity_reference.push(';');
            }
            let decoded = merman_core::entities::decode_html_entities_to_unicode(&entity_reference);
            for decoded in decoded.chars() {
                push_char(
                    decoded,
                    &mut plain,
                    &mut icon_on_line,
                    &mut inline_runs_by_line,
                    &mut break_spaces_runs_by_line,
                    &mut inline_replaced_boundary,
                );
            }
            continue;
        }

        push_char(
            ch,
            &mut plain,
            &mut icon_on_line,
            &mut inline_runs_by_line,
            &mut break_spaces_runs_by_line,
            &mut inline_replaced_boundary,
        );
    }

    if let Some(lines) = break_spaces_runs_by_line.as_mut() {
        while lines.len() > 1
            && lines
                .last()
                .is_some_and(|runs| runs.iter().all(|run| run.text.is_empty()))
        {
            lines.pop();
        }
    }

    // Keep whitespace adjacent to inline icons: in HTML it becomes significant when it separates
    // text from an inline-block `<i>` (for both `<i> text` and `text <i>`). Otherwise discard only
    // collapsible ASCII boundary whitespace. U+00A0 remains visible at either boundary.
    let plain = if icon_on_line.iter().any(|v| *v) {
        plain.trim_end_matches('\n').to_string()
    } else {
        plain
            .trim_end_matches(is_html_collapsible_ascii_whitespace)
            .to_string()
    };
    let has_inline_icons = icon_on_line.iter().any(|has_icon| *has_icon);

    if wrap_mode == WrapMode::HtmlLike && !has_inline_icons && carrier.is_builtin() {
        let explicit_line_boxes =
            explicit_inline_html_line_boxes(&inline_runs_by_line, &image_on_line);
        let mut lines = normalized_preparsed_html_text_lines(&plain);
        if lines.is_empty() {
            lines.push(String::new());
        }
        inline_runs_by_line.resize_with(lines.len(), Vec::new);

        // The pinned Mermaid `createText.ts:addHtmlSpan` first measures decoded HTML with
        // `display: table-cell`, `white-space: nowrap`, and `max-width`. When the browser reports
        // exactly that maximum width, it switches to table layout with `white-space: break-spaces`
        // and a fixed width before measuring again. The built-in planner preserves that
        // natural-width activation, min-content expansion, and greedy line count directly;
        // exact browser shaping and `getBoundingClientRect()` floats remain a bounded residual.
        // Consequently the discarded plain-label and per-run premeasurement passes are not part
        // of this operation and need not call the built-in backend.
        return finish_inline_html_layout(
            measurer,
            carrier,
            &inline_runs_by_line,
            break_spaces_runs_by_line.as_deref(),
            style,
            max_width,
            explicit_line_boxes,
        );
    }

    // Keep this request before all styled probes for arbitrary and host measurers. Their callbacks
    // are observable, including stateful return values and the exact position of a failure.
    let base_text = trim_html_collapsible_ascii_whitespace(&plain);
    let base = measurer.measure_wrapped(base_text, style, max_width, wrap_mode);
    let explicit_line_boxes = explicit_inline_html_line_boxes(&inline_runs_by_line, &image_on_line);

    let mut lines = normalized_preparsed_html_text_lines(&plain);
    if lines.is_empty() {
        lines.push(String::new());
    }
    icon_width_px_by_line.resize(lines.len(), 0.0);
    icon_on_line.resize(lines.len(), false);
    inline_runs_by_line.resize_with(lines.len(), Vec::new);
    let styled_text_width_px_by_line = inline_runs_by_line
        .iter()
        .map(|runs| measure_inline_runs_width_px(measurer, runs, style, wrap_mode, false))
        .collect::<Vec<_>>();
    let inline_width_px_by_line = styled_text_width_px_by_line
        .iter()
        .zip(&icon_width_px_by_line)
        .map(|(text_width, icon_width)| text_width + icon_width)
        .collect::<Vec<_>>();

    if wrap_mode == WrapMode::HtmlLike && !has_inline_icons {
        return finish_inline_html_layout(
            measurer,
            carrier,
            &inline_runs_by_line,
            break_spaces_runs_by_line.as_deref(),
            style,
            max_width,
            explicit_line_boxes,
        );
    }

    let icon_start_wrap = if wrap_mode == WrapMode::HtmlLike {
        max_width
            .filter(|w| w.is_finite() && *w > 0.0)
            .and_then(|w| {
                let mut extra_lines = 0usize;
                let mut wrapped_width: f64 = 0.0;
                let mut has_width_override = false;

                for (idx, line) in lines.iter().enumerate() {
                    if !icon_on_line[idx] || !line.starts_with(is_html_collapsible_ascii_whitespace)
                    {
                        continue;
                    }
                    let text = trim_html_collapsible_ascii_whitespace(line);
                    if text.is_empty() {
                        continue;
                    }

                    let segments = html_break_spaces_segments(text);
                    let text_width = styled_text_width_px_by_line[idx];
                    let first_segment = segments.first().copied().unwrap_or(text);
                    let first_segment_width = measurer
                        .measure_wrapped(first_segment, style, None, wrap_mode)
                        .width;
                    if first_segment_width + icon_width_px_by_line[idx] > w {
                        extra_lines += 1;
                        has_width_override = true;
                        for segment in segments {
                            if segment.is_empty() {
                                continue;
                            }
                            wrapped_width = wrapped_width.max(
                                measurer
                                    .measure_wrapped(segment, style, None, wrap_mode)
                                    .width,
                            );
                        }
                    } else if text_width <= w && inline_width_px_by_line[idx] > w {
                        extra_lines += 1;
                        has_width_override = true;
                        wrapped_width = wrapped_width.max(w);
                    } else if inline_width_px_by_line[idx] > w {
                        has_width_override = true;
                        let mut segment_width: f64 = 0.0;
                        for segment in segments {
                            if segment.is_empty() {
                                continue;
                            }
                            segment_width = segment_width.max(
                                measurer
                                    .measure_wrapped(segment, style, None, wrap_mode)
                                    .width,
                            );
                        }
                        wrapped_width = wrapped_width.max(segment_width.max(w));
                    }
                }

                (has_width_override || extra_lines > 0).then_some((wrapped_width, extra_lines))
            })
    } else {
        None
    };

    let inline_style_extra_wrap_lines = if wrap_mode == WrapMode::HtmlLike {
        max_width
            .filter(|w| w.is_finite() && *w > 0.0)
            .map(|w| {
                lines
                    .iter()
                    .enumerate()
                    .filter(|(idx, line)| {
                        let text = trim_html_collapsible_ascii_whitespace(line);
                        if text.is_empty()
                            || icon_on_line.get(*idx).copied().unwrap_or(false)
                            || !text.chars().any(is_html_collapsible_ascii_whitespace)
                        {
                            return false;
                        }
                        let raw_width =
                            measurer.measure_wrapped(text, style, None, wrap_mode).width;
                        raw_width <= w
                            && styled_text_width_px_by_line
                                .get(*idx)
                                .copied()
                                .unwrap_or(raw_width)
                                > w
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let max_line_width = inline_width_px_by_line
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);

    // Mermaid's upstream baselines land on a 1/64px lattice. For SVG-label measurement, the
    // underlying `getBBox()` numbers can hit exact `.5/64` ties; use ties-to-even rounding to
    // match the lattice choices observed in upstream class SVG fixtures.
    let mut width = match wrap_mode {
        WrapMode::SvgLike | WrapMode::SvgLikeSingleRun => {
            wrap::round_to_1_64_px_ties_to_even(max_line_width)
        }
        WrapMode::HtmlLike => round_to_1_64_px(max_line_width),
    };
    if wrap_mode == WrapMode::HtmlLike
        && let Some(w) = max_width.filter(|w| w.is_finite() && *w > 0.0)
    {
        let raw_w = max_line_width;
        let needs_wrap = raw_w > w;
        if needs_wrap {
            // When wrapping is active, the DOM-driven width behavior is governed by the
            // wrapped layout, not the unwrapped per-line extents. Reuse the wrapped baseline
            // width (without bold deltas) so we don't over-inflate `foreignObject width="..."`
            // from unwrapped lines.
            //
            // The underlying measurer is still responsible for modeling any min-content
            // expansion beyond `max-width`.
            width = icon_start_wrap
                .map(|(icon_width, _)| icon_width)
                .unwrap_or(base.width)
                .max(w);
        } else {
            width = width.min(w);
        }
    }

    let icon_only_extra_lines = if trim_html_collapsible_ascii_whitespace(&plain).is_empty() {
        0
    } else {
        lines
            .iter()
            .enumerate()
            .filter(|(idx, line)| {
                trim_html_collapsible_ascii_whitespace(line).is_empty()
                    && icon_on_line.get(*idx).copied().unwrap_or(false)
                    && icon_width_px_by_line.get(*idx).copied().unwrap_or(0.0) > 0.0
            })
            .count()
    };

    if icon_only_extra_lines > 0 {
        // DOM measurement keeps an inline icon-only line as a normal 1.5em line box and rounds the
        // resulting max line width upward on the 1/64px lattice.
        width = ceil_to_1_64_px(width);
    }

    let (mut height, mut line_count) = if let Some((_, extra_lines)) = icon_start_wrap {
        (
            base.height + extra_lines as f64 * style.font_size.max(1.0) * 1.5,
            base.line_count + extra_lines,
        )
    } else {
        (base.height, base.line_count)
    };
    if icon_only_extra_lines > 0 {
        height += icon_only_extra_lines as f64 * style.font_size.max(1.0) * 1.5;
        line_count += icon_only_extra_lines;
    }
    if inline_style_extra_wrap_lines > 0 {
        height += inline_style_extra_wrap_lines as f64 * style.font_size.max(1.0) * 1.5;
        line_count += inline_style_extra_wrap_lines;
    }

    TextMetrics {
        width,
        height,
        line_count,
    }
}

fn markdown_word_line_plain_text_and_width_px(
    measurer: &dyn TextMeasurer,
    words: &[(String, MermaidMarkdownWordType)],
    style: &TextStyle,
    wrap_mode: WrapMode,
) -> (String, f64) {
    let mut plain = String::new();
    let mut runs = Vec::new();

    for (word_idx, (word, ty)) in words.iter().enumerate() {
        let visible_word = match wrap_mode {
            WrapMode::HtmlLike => merman_core::entities::decode_html_entities_to_unicode(word),
            WrapMode::SvgLike | WrapMode::SvgLikeSingleRun => {
                crate::entities::decode_svg_text_content_entities(word)
            }
        };
        let bold = *ty == MermaidMarkdownWordType::Strong;
        let italic = *ty == MermaidMarkdownWordType::Em;

        if word_idx > 0 {
            plain.push(' ');
            push_inline_text_char(&mut runs, ' ', false, false, false);
        }
        for ch in visible_word.chars() {
            plain.push(ch);
            push_inline_text_char(&mut runs, ch, bold, italic, false);
        }
    }

    let width = measure_inline_runs_width_px(measurer, &runs, style, wrap_mode, true);
    (plain, width)
}

fn measure_markdown_word_line_width_px(
    measurer: &dyn TextMeasurer,
    words: &[(String, MermaidMarkdownWordType)],
    style: &TextStyle,
    wrap_mode: WrapMode,
) -> f64 {
    markdown_word_line_plain_text_and_width_px(measurer, words, style, wrap_mode).1
}

fn split_markdown_word_to_width_px(
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    word: &str,
    ty: MermaidMarkdownWordType,
    max_width_px: f64,
    wrap_mode: WrapMode,
) -> (String, String) {
    if max_width_px <= 0.0 {
        return (word.to_string(), String::new());
    }
    use unicode_segmentation::UnicodeSegmentation;

    let Some(first) = word.graphemes(true).next() else {
        return (String::new(), String::new());
    };

    // Mermaid's splitTextToChars uses Intl.Segmenter. An over-wide first grapheme must
    // still make progress without separating its combining marks or emoji joiners.
    let mut split_at = first.len();
    for (index, grapheme) in word.grapheme_indices(true) {
        let end = index + grapheme.len();
        let head = word[..end].to_string();
        let width = measure_markdown_word_line_width_px(measurer, &[(head, ty)], style, wrap_mode);
        if width.is_finite() && width <= max_width_px {
            split_at = end;
        } else {
            break;
        }
    }

    let (head, tail) = word.split_at(split_at);
    (head.to_string(), tail.to_string())
}

fn wrap_markdown_word_lines(
    measurer: &dyn TextMeasurer,
    parsed: &[Vec<(String, MermaidMarkdownWordType)>],
    style: &TextStyle,
    max_width_px: Option<f64>,
    wrap_mode: WrapMode,
    break_long_words: bool,
) -> Vec<Vec<(String, MermaidMarkdownWordType)>> {
    let Some(max_width_px) = max_width_px.filter(|w| w.is_finite() && *w > 0.0) else {
        return parsed.to_vec();
    };

    let mut out: Vec<Vec<(String, MermaidMarkdownWordType)>> = Vec::new();
    for line in parsed {
        if line.is_empty() {
            out.push(Vec::new());
            continue;
        }

        let mut tokens = std::collections::VecDeque::from(line.clone());
        let mut cur: Vec<(String, MermaidMarkdownWordType)> = Vec::new();

        while let Some((word, ty)) = tokens.pop_front() {
            let mut candidate = cur.clone();
            candidate.push((word.clone(), ty));
            if measure_markdown_word_line_width_px(measurer, &candidate, style, wrap_mode)
                <= max_width_px
            {
                cur = candidate;
                continue;
            }

            if !cur.is_empty() {
                out.push(cur);
                cur = Vec::new();
                tokens.push_front((word, ty));
                continue;
            }

            let single_word_width = measure_markdown_word_line_width_px(
                measurer,
                &[(word.clone(), ty)],
                style,
                wrap_mode,
            );
            if single_word_width <= max_width_px || !break_long_words {
                out.push(vec![(word, ty)]);
                continue;
            }

            let (head, tail) = split_markdown_word_to_width_px(
                measurer,
                style,
                &word,
                ty,
                max_width_px,
                wrap_mode,
            );
            out.push(vec![(head, ty)]);
            if !tail.is_empty() {
                tokens.push_front((tail, ty));
            }
        }

        if !cur.is_empty() {
            out.push(cur);
        }
    }

    if out.is_empty() {
        vec![Vec::new()]
    } else {
        out
    }
}

pub(crate) fn mermaid_markdown_to_wrapped_word_lines(
    measurer: &dyn TextMeasurer,
    markdown: &str,
    style: &TextStyle,
    max_width_px: Option<f64>,
    wrap_mode: WrapMode,
) -> Vec<Vec<(String, MermaidMarkdownWordType)>> {
    let parsed = mermaid_markdown_to_lines(markdown, true);
    wrap_markdown_word_lines(measurer, &parsed, style, max_width_px, wrap_mode, true)
}

fn html_markdown_paragraph_gap_lines(markdown: &str) -> usize {
    if !markdown.contains("\n\n") && !markdown.contains("\r\n\r\n") {
        return 0;
    }

    let markdown = markdown
        .strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(markdown)
        .replace("\r\n", "\n");
    let parser = pulldown_cmark::Parser::new_ext(
        &markdown,
        pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_TASKLISTS,
    );
    let paragraph_count = parser
        .filter(|ev| {
            matches!(
                ev,
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph)
            )
        })
        .count();

    paragraph_count.saturating_sub(1)
}

fn measure_markdown_with_inline_styles_impl(
    measurer: &dyn TextMeasurer,
    markdown: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
    manually_wrap_words: bool,
) -> TextMetrics {
    // Mermaid's flowchart HTML labels support inline Markdown images. These affect layout even
    // when the label has no textual content (e.g. `![](...)`).
    //
    // We keep the existing text-focused Markdown measurement for the common case, and only
    // special-case when we observe at least one image token.
    if markdown.contains("![") {
        #[derive(Debug, Default, Clone)]
        struct Paragraph {
            text: String,
            image_urls: Vec<String>,
        }

        fn measure_markdown_images(
            measurer: &dyn TextMeasurer,
            markdown: &str,
            style: &TextStyle,
            max_width: Option<f64>,
            wrap_mode: WrapMode,
        ) -> Option<TextMetrics> {
            let parser = pulldown_cmark::Parser::new_ext(
                markdown,
                pulldown_cmark::Options::ENABLE_TABLES
                    | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
                    | pulldown_cmark::Options::ENABLE_TASKLISTS,
            );

            let mut paragraphs: Vec<Paragraph> = Vec::new();
            let mut current = Paragraph::default();
            let mut in_paragraph = false;

            for ev in parser {
                match ev {
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph) => {
                        if in_paragraph {
                            paragraphs.push(std::mem::take(&mut current));
                        }
                        in_paragraph = true;
                    }
                    pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Paragraph) => {
                        if in_paragraph {
                            paragraphs.push(std::mem::take(&mut current));
                        }
                        in_paragraph = false;
                    }
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image {
                        dest_url, ..
                    }) => {
                        current.image_urls.push(dest_url.to_string());
                    }
                    pulldown_cmark::Event::Text(t) | pulldown_cmark::Event::Code(t) => {
                        current.text.push_str(&t);
                    }
                    pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                        current.text.push('\n');
                    }
                    _ => {}
                }
            }
            if in_paragraph {
                paragraphs.push(current);
            }

            let total_images: usize = paragraphs.iter().map(|p| p.image_urls.len()).sum();
            if total_images == 0 {
                return None;
            }

            let total_text = paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let has_any_text = !trim_html_collapsible_ascii_whitespace(&total_text).is_empty();

            // Mermaid renders a single standalone Markdown image without a `<p>` wrapper and
            // applies fixed `80px` sizing. In the upstream fixtures, missing/empty `src` yields
            // `height="0"` while keeping the width.
            if total_images == 1 && !has_any_text {
                let url = paragraphs
                    .iter()
                    .flat_map(|p| p.image_urls.iter())
                    .next()
                    .cloned()
                    .unwrap_or_default();
                let img_w = 80.0;
                let has_src = !url.trim().is_empty();
                let img_h = if has_src { img_w } else { 0.0 };
                return Some(TextMetrics {
                    width: ceil_to_1_64_px(img_w),
                    height: ceil_to_1_64_px(img_h),
                    line_count: if img_h > 0.0 { 1 } else { 0 },
                });
            }

            let max_w = max_width.unwrap_or(200.0).max(1.0);
            let line_height = style.font_size.max(1.0) * 1.5;

            let mut width: f64 = 0.0;
            let mut height: f64 = 0.0;
            let mut line_count: usize = 0;

            for p in paragraphs {
                let p_text = trim_html_collapsible_ascii_whitespace(&p.text).to_string();
                let text_metrics = if p_text.is_empty() {
                    TextMetrics {
                        width: 0.0,
                        height: 0.0,
                        line_count: 0,
                    }
                } else {
                    measurer.measure_wrapped(&p_text, style, Some(max_w), wrap_mode)
                };

                if !p.image_urls.is_empty() {
                    // Markdown images inside paragraphs use `width: 100%` in Mermaid's HTML label
                    // output, so they expand to the available width.
                    width = width.max(max_w);
                    if text_metrics.line_count == 0 {
                        // Image-only paragraphs include an extra line box from the `<p>` element.
                        height += line_height;
                        line_count += 1;
                    }
                    for url in p.image_urls {
                        let has_src = !url.trim().is_empty();
                        let img_h = if has_src { max_w } else { 0.0 };
                        height += img_h;
                        if img_h > 0.0 {
                            line_count += 1;
                        }
                    }
                }

                width = width.max(text_metrics.width);
                height += text_metrics.height;
                line_count += text_metrics.line_count;
            }

            Some(TextMetrics {
                width: ceil_to_1_64_px(width),
                height: ceil_to_1_64_px(height),
                line_count,
            })
        }

        if let Some(m) = measure_markdown_images(measurer, markdown, style, max_width, wrap_mode) {
            return m;
        }
    }

    let raw_parsed = mermaid_markdown_to_lines(markdown, true);
    let html_paragraph_gap_lines = if wrap_mode == WrapMode::HtmlLike {
        html_markdown_paragraph_gap_lines(markdown)
    } else {
        0
    };
    let parsed = if manually_wrap_words {
        wrap_markdown_word_lines(measurer, &raw_parsed, style, max_width, wrap_mode, true)
    } else {
        raw_parsed.clone()
    };

    let mut plain_lines: Vec<String> = Vec::with_capacity(parsed.len().max(1));
    let mut styled_width_px_by_line: Vec<f64> = Vec::with_capacity(parsed.len().max(1));
    for words in &parsed {
        let (plain, width) =
            markdown_word_line_plain_text_and_width_px(measurer, words, style, wrap_mode);
        plain_lines.push(plain);
        styled_width_px_by_line.push(width);
    }

    let plain = plain_lines.join("\n");
    let plain = match wrap_mode {
        WrapMode::HtmlLike => trim_html_collapsible_ascii_whitespace(&plain).to_string(),
        WrapMode::SvgLike | WrapMode::SvgLikeSingleRun => plain,
    };
    let base = if manually_wrap_words {
        measurer.measure_wrapped(&plain, style, None, wrap_mode)
    } else {
        measurer.measure_wrapped(&plain, style, max_width, wrap_mode)
    };

    let max_line_width = styled_width_px_by_line
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);

    // Mermaid's upstream baselines land on a power-of-two lattice:
    // - DOM-measured HTML labels tend to snap to 1/64px.
    // - SVG-label markdown `getBBox()` tends to snap to 1/64px in our upstream baselines.
    //
    // Quantize accordingly so strict-XML layout remains stable.
    let mut width = match wrap_mode {
        WrapMode::HtmlLike => round_to_1_64_px(max_line_width),
        WrapMode::SvgLike | WrapMode::SvgLikeSingleRun => round_to_1_64_px(max_line_width),
    };
    if wrap_mode == WrapMode::HtmlLike
        && let Some(w) = max_width.filter(|w| w.is_finite() && *w > 0.0)
    {
        let raw_w = raw_parsed
            .iter()
            .map(|words| {
                markdown_word_line_plain_text_and_width_px(measurer, words, style, wrap_mode).1
            })
            .fold(0.0_f64, f64::max);
        let needs_wrap = raw_w > w;
        if needs_wrap {
            if manually_wrap_words {
                width = width.max(w);
            } else {
                width = base.width.max(w);
            }
        } else {
            width = width.min(w);
        }
    }

    TextMetrics {
        width,
        height: base.height + html_paragraph_gap_lines as f64 * style.font_size.max(1.0) * 1.5,
        line_count: base.line_count + html_paragraph_gap_lines,
    }
}

pub fn measure_markdown_with_inline_styles(
    measurer: &dyn TextMeasurer,
    markdown: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
) -> TextMetrics {
    measure_markdown_with_inline_styles_impl(measurer, markdown, style, max_width, wrap_mode, false)
}

pub(crate) fn measure_wrapped_markdown_with_inline_styles(
    measurer: &dyn TextMeasurer,
    markdown: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
) -> TextMetrics {
    measure_markdown_with_inline_styles_impl(measurer, markdown, style, max_width, wrap_mode, true)
}
