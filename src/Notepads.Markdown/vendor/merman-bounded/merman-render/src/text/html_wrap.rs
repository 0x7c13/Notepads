//! Shared CSS whitespace and line-box planning for HTML labels.

use super::line_break::{HtmlBreakKind, html_break_spaces_segments_with_metadata_and_checkpoints};
use crate::environment::BuiltinHtmlLineWidth;
use std::ops::Range;

pub(super) fn html_line_text_with_break(
    text: &str,
    selected_break: bool,
) -> std::borrow::Cow<'_, str> {
    if !selected_break {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut selected = text.strip_suffix('\u{00ad}').unwrap_or(text).to_owned();
    selected.push('-');
    std::borrow::Cow::Owned(selected)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum HtmlWhiteSpace {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

impl HtmlWhiteSpace {
    pub(crate) fn from_css_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "nowrap" => Some(Self::Nowrap),
            "pre" => Some(Self::Pre),
            "pre-wrap" => Some(Self::PreWrap),
            "pre-line" => Some(Self::PreLine),
            "break-spaces" => Some(Self::BreakSpaces),
            _ => None,
        }
    }

    pub(crate) fn preserves_spaces(self) -> bool {
        matches!(self, Self::Pre | Self::PreWrap | Self::BreakSpaces)
    }

    fn allows_wrap(self) -> bool {
        !matches!(self, Self::Pre | Self::Nowrap)
    }
}

#[derive(Debug)]
struct WhiteSpaceRun {
    range: Range<usize>,
    mode: HtmlWhiteSpace,
}

/// Decoded text with computed whitespace modes, retained across inline element boundaries.
#[derive(Debug, Default)]
pub(crate) struct NormalizedHtmlText {
    text: String,
    runs: Vec<WhiteSpaceRun>,
    collapsed_space: bool,
    collapsed_wraps: Vec<usize>,
    boundaries: Vec<(usize, usize, HtmlWhiteSpace)>,
}

impl NormalizedHtmlText {
    pub(crate) fn single_mode(text: &str, mode: HtmlWhiteSpace) -> Self {
        Self {
            text: text.to_owned(),
            runs: vec![WhiteSpaceRun {
                range: 0..text.len(),
                mode,
            }],
            ..Self::default()
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(crate) fn inline_boundary(&mut self, depth: usize, mode: HtmlWhiteSpace) {
        if let Some((offset, previous_depth, previous_mode)) = self.boundaries.last_mut()
            && *offset == self.text.len()
        {
            // Fully collapsed leaves contribute no character, but their boundaries still expose
            // the common ancestor of the retained text on either side in document order.
            if depth < *previous_depth {
                *previous_depth = depth;
                *previous_mode = mode;
            }
        } else {
            self.boundaries.push((self.text.len(), depth, mode));
        }
    }

    fn break_mode_at(&self, offset: usize) -> HtmlWhiteSpace {
        let index = self
            .boundaries
            .partition_point(|(boundary, _, _)| *boundary < offset);
        self.boundaries
            .get(index)
            .filter(|(boundary, _, _)| *boundary == offset)
            .map_or_else(
                || self.mode_at(offset.saturating_sub(1)),
                |(_, _, mode)| *mode,
            )
    }

    fn push(&mut self, ch: char, mode: HtmlWhiteSpace) {
        let start = self.text.len();
        self.text.push(ch);
        if let Some(last) = self.runs.last_mut()
            && last.mode == mode
        {
            last.range.end = self.text.len();
        } else {
            self.runs.push(WhiteSpaceRun {
                range: start..self.text.len(),
                mode,
            });
        }
    }

    pub(crate) fn hard_break(&mut self) {
        self.push('\n', HtmlWhiteSpace::Pre);
        self.collapsed_space = true;
    }

    pub(crate) fn append<E>(
        &mut self,
        text: &str,
        mode: HtmlWhiteSpace,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), E> {
        for (index, ch) in text.chars().enumerate() {
            if index.is_multiple_of(64) {
                checkpoint()?;
            }
            if ch == '\n' && (mode.preserves_spaces() || mode == HtmlWhiteSpace::PreLine) {
                self.push('\n', mode);
                self.collapsed_space = true;
            } else if !mode.preserves_spaces() && super::is_html_collapsible_ascii_whitespace(ch) {
                if !self.collapsed_space {
                    self.push(' ', mode);
                    self.collapsed_space = true;
                } else if mode.allows_wrap()
                    && self.text.ends_with(' ')
                    && self.collapsed_wraps.last().copied() != Some(self.text.len())
                {
                    // CSS retains a discarded collapsible space's own soft-wrap opportunity.
                    self.collapsed_wraps.push(self.text.len());
                }
            } else {
                self.push(ch, mode);
                self.collapsed_space = false;
            }
        }
        checkpoint()
    }

    fn mode_at(&self, offset: usize) -> HtmlWhiteSpace {
        let index = self.runs.partition_point(|run| run.range.end <= offset);
        self.runs
            .get(index)
            .map_or(HtmlWhiteSpace::Normal, |run| run.mode)
    }
}

#[derive(Debug)]
pub(crate) struct PlannedHtmlLine {
    pub(crate) text: String,
    pub(crate) width: f64,
    pub(crate) alignment_width: f64,
}

fn separator(ch: char) -> bool {
    matches!(
        ch,
        '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

/// Growing line state. The built-in route retains Unicode shaping state; opaque routes always
/// receive the complete visible candidate, including a conditional hyphen when selected.
struct Candidate {
    text: String,
    width: Option<BuiltinHtmlLineWidth>,
    trimmed: usize,
    non_hanging: usize,
    trimmed_width: Option<BuiltinHtmlLineWidth>,
    non_hanging_width: Option<BuiltinHtmlLineWidth>,
    retained_mode: HtmlWhiteSpace,
}

struct CandidateSnapshot {
    len: usize,
    trimmed: usize,
    non_hanging: usize,
    width: Option<BuiltinHtmlLineWidth>,
    trimmed_width: Option<BuiltinHtmlLineWidth>,
    non_hanging_width: Option<BuiltinHtmlLineWidth>,
    retained_mode: HtmlWhiteSpace,
}

impl Candidate {
    fn snapshot(&self) -> CandidateSnapshot {
        CandidateSnapshot {
            len: self.text.len(),
            trimmed: self.trimmed,
            non_hanging: self.non_hanging,
            width: self.width.clone(),
            trimmed_width: self.trimmed_width.clone(),
            non_hanging_width: self.non_hanging_width.clone(),
            retained_mode: self.retained_mode,
        }
    }

    fn restore(&mut self, snapshot: CandidateSnapshot) {
        self.text.truncate(snapshot.len);
        self.trimmed = snapshot.trimmed;
        self.non_hanging = snapshot.non_hanging;
        self.width = snapshot.width;
        self.trimmed_width = snapshot.trimmed_width;
        self.non_hanging_width = snapshot.non_hanging_width;
        self.retained_mode = snapshot.retained_mode;
    }
    fn new(mut width: Option<BuiltinHtmlLineWidth>) -> Self {
        if let Some(width) = &mut width {
            width.reset();
        }
        Self {
            text: String::new(),
            trimmed: 0,
            non_hanging: 0,
            trimmed_width: width.clone(),
            non_hanging_width: width.clone(),
            width,
            retained_mode: HtmlWhiteSpace::Normal,
        }
    }

    fn append<E>(
        &mut self,
        text: &str,
        source: &NormalizedHtmlText,
        offset: usize,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), E> {
        let mut regular_start = 0;
        for (index, (relative, ch)) in text.char_indices().enumerate() {
            if index.is_multiple_of(64) {
                checkpoint()?;
            }
            if ch != ' ' && ch != '\t' && !separator(ch) {
                continue;
            }
            self.append_regular(
                &text[regular_start..relative],
                source.mode_at(offset + relative.saturating_sub(1)),
            );
            regular_start = relative + ch.len_utf8();
            let mode = source.mode_at(offset + relative);
            if self.text.is_empty() && ch == ' ' && !mode.preserves_spaces() {
                continue;
            }
            self.text.push(ch);
            if let Some(width) = &mut self.width {
                width.push_text(ch.encode_utf8(&mut [0; 4]));
            }
            if ch != ' ' || mode.preserves_spaces() {
                self.trimmed = self.text.len();
                self.trimmed_width = self.width.clone();
                self.retained_mode = mode;
            }
            let hangs = match mode {
                HtmlWhiteSpace::Normal | HtmlWhiteSpace::Nowrap | HtmlWhiteSpace::PreLine => {
                    ch == ' ' || separator(ch)
                }
                HtmlWhiteSpace::PreWrap => ch == ' ' || ch == '\t' || separator(ch),
                HtmlWhiteSpace::Pre | HtmlWhiteSpace::BreakSpaces => false,
            };
            if !hangs {
                self.non_hanging = self.text.len();
                self.non_hanging_width = self.width.clone();
            }
        }
        self.append_regular(
            &text[regular_start..],
            source.mode_at(offset + text.len().saturating_sub(1)),
        );
        checkpoint()
    }

    fn append_regular(&mut self, text: &str, mode: HtmlWhiteSpace) {
        if text.is_empty() {
            return;
        }
        // SHY has no unselected glyph, but its control boundary still participates in shaping.
        // Removing it here would join otherwise independent combining or ZWJ sequences.
        self.text.push_str(text);
        if let Some(width) = &mut self.width {
            width.push_text(text);
        }
        self.trimmed = self.text.len();
        self.non_hanging = self.text.len();
        self.trimmed_width = self.width.clone();
        self.retained_mode = mode;
        self.non_hanging_width = self.width.clone();
    }

    fn measured<E>(
        &self,
        end: usize,
        state: &Option<BuiltinHtmlLineWidth>,
        hyphen: bool,
        measure: &mut impl FnMut(&str) -> Result<f64, E>,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<f64, E> {
        checkpoint()?;
        let result = if let Some(width) = state {
            let mut width = width.clone();
            if hyphen {
                width.push_text("-");
            }
            width.width_px()
        } else if hyphen {
            measure(&html_line_text_with_break(&self.text[..end], true))?
        } else {
            measure(&self.text[..end])?
        };
        checkpoint()?;
        Ok(result)
    }

    fn fit_width<E>(
        &self,
        hyphen: bool,
        measure: &mut impl FnMut(&str) -> Result<f64, E>,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<f64, E> {
        self.measured(
            self.non_hanging,
            &self.non_hanging_width,
            hyphen,
            measure,
            checkpoint,
        )
    }

    fn finish<E>(
        mut self,
        soft: bool,
        hyphen: bool,
        limit: Option<f64>,
        measure: &mut impl FnMut(&str) -> Result<f64, E>,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<PlannedHtmlLine, E> {
        self.text.truncate(self.trimmed);
        if hyphen {
            if self.text.ends_with('\u{00ad}') {
                self.text.truncate(self.text.len() - '\u{00ad}'.len_utf8());
            }
            self.text.push('-');
        }
        // Final measurement uses the same backend contract as natural-width measurement.
        checkpoint()?;
        let width = measure(&self.text)?;
        checkpoint()?;
        let conditional = self.retained_mode == HtmlWhiteSpace::PreWrap && !soft;
        let hangs = !hyphen
            && self.non_hanging < self.trimmed
            && (!conditional || limit.is_some_and(|limit| width > limit));
        let alignment_width = if hangs {
            self.measured(
                self.non_hanging,
                &self.non_hanging_width,
                false,
                measure,
                checkpoint,
            )?
        } else {
            width
        };
        Ok(PlannedHtmlLine {
            text: self.text,
            width,
            alignment_width,
        })
    }
}

/// Plans HTML line boxes after entity decoding and CSS whitespace normalization.
pub(crate) fn plan_html_lines<E>(
    source: &NormalizedHtmlText,
    max_width: Option<f64>,
    builtin: Option<BuiltinHtmlLineWidth>,
    measure: &mut impl FnMut(&str) -> Result<f64, E>,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Vec<PlannedHtmlLine>, E> {
    let limit = max_width.filter(|width| width.is_finite() && *width > 0.0);
    let mut lines = Vec::new();
    let mut base = 0;
    // A terminating preserved newline does not create another empty line box.
    for raw in source.text.split_terminator('\n') {
        checkpoint()?;
        let mut whole = Candidate::new(builtin.clone());
        whole.append(raw, source, base, checkpoint)?;
        let natural = whole.measured(
            whole.trimmed,
            &whole.trimmed_width,
            false,
            measure,
            checkpoint,
        )?;
        if limit.is_none_or(|limit| natural <= limit) {
            lines.push(whole.finish(false, false, limit, measure, checkpoint)?);
            base += raw.len() + 1;
            continue;
        }
        let segments = html_break_spaces_segments_with_metadata_and_checkpoints(raw, checkpoint)?;
        let mut atoms = Vec::new();
        let mut start = 0;
        let mut end = 0;
        for segment in segments {
            end += segment.text.len();
            let owner = source.mode_at(base + end.saturating_sub(1));
            let boundary = source.break_mode_at(base + end);
            let (allowed, kind) = if segment.end == HtmlBreakKind::SoftHyphen {
                if owner.allows_wrap() {
                    (true, HtmlBreakKind::SoftHyphen)
                } else {
                    (boundary.allows_wrap(), HtmlBreakKind::Ordinary)
                }
            } else if segment.text.ends_with(' ') || segment.text.ends_with('\t') {
                (
                    owner.allows_wrap()
                        || source.collapsed_wraps.binary_search(&(base + end)).is_ok(),
                    segment.end,
                )
            } else {
                (boundary.allows_wrap(), segment.end)
            };
            if end == raw.len() || allowed {
                atoms.push((start..end, kind));
                start = end;
            }
        }
        if atoms.len() <= 1 {
            lines.push(whole.finish(false, false, limit, measure, checkpoint)?);
            base += raw.len() + 1;
            continue;
        }
        let mut active_limit = limit.unwrap();
        if atoms
            .iter()
            .any(|(_, kind)| *kind == HtmlBreakKind::SoftHyphen)
        {
            // CSS table sizing cannot place a selected discretionary glyph below min-content.
            let mut minimum = 0.0_f64;
            for (range, kind) in &atoms {
                let mut atom = Candidate::new(builtin.clone());
                atom.append(&raw[range.clone()], source, base + range.start, checkpoint)?;
                minimum = minimum.max(atom.fit_width(
                    *kind == HtmlBreakKind::SoftHyphen,
                    measure,
                    checkpoint,
                )?);
            }
            active_limit = active_limit.max(minimum);
        }
        if natural <= active_limit {
            lines.push(whole.finish(false, false, limit, measure, checkpoint)?);
            base += raw.len() + 1;
            continue;
        }
        let mut candidate = Candidate::new(builtin.clone());
        let mut last_break: Option<(usize, CandidateSnapshot, HtmlBreakKind)> = None;
        let mut index = 0;
        while index < atoms.len() {
            checkpoint()?;
            let (range, kind) = &atoms[index];
            let token = &raw[range.clone()];
            let token_base = base + range.start;
            candidate.append(token, source, token_base, checkpoint)?;
            let pre_wrap_spaces = source.mode_at(token_base) == HtmlWhiteSpace::PreWrap
                && token.chars().all(|ch| ch == ' ' || ch == '\t');
            let natural_candidate = candidate.fit_width(false, measure, checkpoint)?;
            if !pre_wrap_spaces
                && natural_candidate > active_limit
                && let Some((next_index, snapshot, end_kind)) = last_break.take()
            {
                candidate.restore(snapshot);
                lines.push(candidate.finish(
                    true,
                    end_kind == HtmlBreakKind::SoftHyphen,
                    limit,
                    measure,
                    checkpoint,
                )?);
                candidate = Candidate::new(builtin.clone());
                index = next_index;
                continue;
            }
            let selected_width = if *kind == HtmlBreakKind::SoftHyphen {
                candidate.fit_width(true, measure, checkpoint)?
            } else {
                natural_candidate
            };
            if index + 1 < atoms.len()
                && (selected_width <= active_limit || pre_wrap_spaces || last_break.is_none())
            {
                last_break = Some((index + 1, candidate.snapshot(), *kind));
            }
            index += 1;
        }
        lines.push(candidate.finish(false, false, limit, measure, checkpoint)?);
        base += raw.len() + 1;
    }
    if lines.len() > 1
        && lines.last().is_some_and(|line| line.text.is_empty())
        && !source.text.ends_with('\n')
    {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(Candidate::new(builtin).finish(false, false, limit, measure, checkpoint)?);
    }
    checkpoint()?;
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    fn plan(parts: &[(&str, HtmlWhiteSpace)], limit: f64) -> Vec<PlannedHtmlLine> {
        let mut source = NormalizedHtmlText::default();
        let mut checkpoint = || Ok::<(), Infallible>(());
        for (text, mode) in parts {
            source.append(text, *mode, &mut checkpoint).unwrap();
        }
        plan_html_lines(
            &source,
            Some(limit),
            None,
            &mut |text| Ok(text.chars().count() as f64 * 10.0),
            &mut checkpoint,
        )
        .unwrap()
    }

    #[test]
    fn whitespace_modes_keep_source_breaks_only_when_preserved() {
        for mode in [HtmlWhiteSpace::Normal, HtmlWhiteSpace::Nowrap] {
            let lines = plan(&[("  a \n  b  ", mode)], 100.0);
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0].text, "a b");
        }
        let lines = plan(&[("  a \n\n  b  ", HtmlWhiteSpace::PreLine)], 100.0);
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["a", "", "b"]
        );
        for mode in [
            HtmlWhiteSpace::Pre,
            HtmlWhiteSpace::PreWrap,
            HtmlWhiteSpace::BreakSpaces,
        ] {
            let lines = plan(&[("  a \n\n  b  ", mode)], 200.0);
            assert_eq!(
                lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                ["  a ", "", "  b  "]
            );
        }
    }

    #[test]
    fn nowrap_is_local_to_its_text_run() {
        let lines = plan(
            &[
                ("a ", HtmlWhiteSpace::Normal),
                ("b c ", HtmlWhiteSpace::Nowrap),
                ("d e", HtmlWhiteSpace::Normal),
            ],
            30.0,
        );
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["a", "b c d", "e"]
        );
    }

    #[test]
    fn pre_wrap_end_spaces_hang_only_when_they_overflow() {
        for (limit, expected_alignment) in [(50.0, 30.0), (20.0, 10.0)] {
            let lines = plan(&[("a  ", HtmlWhiteSpace::PreWrap)], limit);
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0].text, "a  ");
            assert_eq!(lines[0].width, 30.0);
            assert_eq!(lines[0].alignment_width, expected_alignment);
        }
        let lines = plan(&[("a  \nb", HtmlWhiteSpace::PreWrap)], 50.0);
        assert_eq!(lines[0].alignment_width, 30.0);
        let lines = plan(&[("a  b", HtmlWhiteSpace::PreWrap)], 20.0);
        assert_eq!(lines[0].alignment_width, 10.0);
        let lines = plan(&[("a  ", HtmlWhiteSpace::BreakSpaces)], 50.0);
        assert_eq!(lines[0].alignment_width, 30.0);
    }

    #[test]
    fn wider_discretionary_glyph_does_not_preempt_a_later_fitting_word() {
        let source = NormalizedHtmlText::single_mode("p W\u{00ad}i x", HtmlWhiteSpace::Normal);
        let lines = plan_html_lines(
            &source,
            Some(40.0),
            None,
            &mut |text| {
                Ok::<f64, Infallible>(
                    text.chars()
                        .map(|ch| match ch {
                            'W' | '-' => 20.0,
                            'p' => 10.0,
                            '\u{00ad}' => 0.0,
                            _ => 5.0,
                        })
                        .sum(),
                )
            },
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["p W\u{00ad}i", "x"]
        );
    }

    #[test]
    fn raw_soft_hyphens_keep_builtin_and_opaque_zwj_wraps_equivalent() {
        let chain = format!("{}👩", "👩\u{200d}\u{00ad}".repeat(128));
        let style = super::super::TextStyle {
            font_size: 10.0,
            ..super::super::TextStyle::default()
        };
        let measurer = super::super::DeterministicTextMeasurer::default();
        for text in [
            format!("W {chain}"),
            format!("W {chain} x"),
            format!("x {chain}!!"),
            format!("{chain} x"),
        ] {
            let source = NormalizedHtmlText::single_mode(&text, HtmlWhiteSpace::BreakSpaces);
            let mut checkpoint = || Ok::<(), Infallible>(());
            let mut measure = |text: &str| {
                Ok::<f64, Infallible>(super::super::TextMeasurer::measure_html_line_width_px(
                    &measurer, text, &style,
                ))
            };
            let optimized = plan_html_lines(
                &source,
                Some(15.0),
                Some(BuiltinHtmlLineWidth::deterministic(&style, 0.0)),
                &mut measure,
                &mut checkpoint,
            )
            .unwrap();
            let opaque =
                plan_html_lines(&source, Some(15.0), None, &mut measure, &mut checkpoint).unwrap();
            assert_eq!(optimized.len(), opaque.len(), "text={text:?}");
            for (optimized, opaque) in optimized.iter().zip(&opaque) {
                assert_eq!(optimized.text, opaque.text, "text={text:?}");
                assert_eq!(
                    optimized.width.to_bits(),
                    opaque.width.to_bits(),
                    "text={text:?}"
                );
                assert_eq!(
                    optimized.alignment_width.to_bits(),
                    opaque.alignment_width.to_bits(),
                    "text={text:?}"
                );
            }
        }
    }

    #[test]
    fn raw_soft_hyphen_separates_presentation_context_and_retains_disabled_breaks() {
        let style = super::super::TextStyle {
            font_size: 10.0,
            ..super::super::TextStyle::default()
        };
        let measurer = super::super::DeterministicTextMeasurer::default();
        let raw = "♈\u{00ad}\u{fe0e}";
        let without_control = "♈\u{fe0e}";
        let width = super::super::TextMeasurer::measure_html_line_width_px(&measurer, raw, &style);
        let joined_width = super::super::TextMeasurer::measure_html_line_width_px(
            &measurer,
            without_control,
            &style,
        );
        assert_eq!(width, 10.0);
        assert_eq!(joined_width, 6.0);
        for mode in [
            HtmlWhiteSpace::Normal,
            HtmlWhiteSpace::Nowrap,
            HtmlWhiteSpace::Pre,
        ] {
            let source = NormalizedHtmlText::single_mode(raw, mode);
            let mut measure = |text: &str| {
                Ok::<f64, Infallible>(super::super::TextMeasurer::measure_html_line_width_px(
                    &measurer, text, &style,
                ))
            };
            let lines = plan_html_lines(
                &source,
                Some(100.0),
                Some(BuiltinHtmlLineWidth::deterministic(&style, 0.0)),
                &mut measure,
                &mut || Ok(()),
            )
            .unwrap();
            assert_eq!(lines[0].text, raw);
            assert_eq!(lines[0].width, width);
        }
        assert!(matches!(
            html_line_text_with_break(raw, false),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(
            html_line_text_with_break("a\u{00ad}b\u{00ad}", true),
            "a\u{00ad}b-"
        );
    }
    #[test]
    fn terminal_newline_does_not_create_a_synthetic_box() {
        for mode in [
            HtmlWhiteSpace::Pre,
            HtmlWhiteSpace::PreWrap,
            HtmlWhiteSpace::PreLine,
        ] {
            assert_eq!(plan(&[("a\n", mode)], 100.0).len(), 1);
            assert_eq!(plan(&[("a\n\n", mode)], 100.0).len(), 2);
        }
        assert_eq!(plan(&[("a\n ", HtmlWhiteSpace::PreLine)], 100.0).len(), 1);
        assert_eq!(plan(&[("a\n ", HtmlWhiteSpace::PreWrap)], 100.0).len(), 2);
    }

    #[test]
    fn nbsp_never_hangs_or_collapses() {
        for mode in [
            HtmlWhiteSpace::Normal,
            HtmlWhiteSpace::Nowrap,
            HtmlWhiteSpace::PreLine,
            HtmlWhiteSpace::PreWrap,
        ] {
            let lines = plan(&[("a\u{00a0}", mode)], 10.0);
            assert_eq!(lines[0].text, "a\u{00a0}");
            assert_eq!(lines[0].width, 20.0);
            assert_eq!(lines[0].alignment_width, 20.0);
        }
    }

    #[test]
    fn hanging_mode_comes_from_the_last_retained_character() {
        let lines = plan(
            &[
                ("a ", HtmlWhiteSpace::PreWrap),
                (" ", HtmlWhiteSpace::Normal),
            ],
            40.0,
        );
        assert_eq!(lines[0].text, "a ");
        assert_eq!(lines[0].alignment_width, 20.0);
    }

    #[test]
    fn inline_boundary_wrap_uses_the_common_ancestor_mode() {
        let mut checkpoint = || Ok::<(), Infallible>(());
        for (parent, expected) in [
            (HtmlWhiteSpace::Normal, vec!["中文", "中"]),
            (HtmlWhiteSpace::Nowrap, vec!["中文中"]),
        ] {
            let mut source = NormalizedHtmlText::default();
            source
                .append("中文", HtmlWhiteSpace::Nowrap, &mut checkpoint)
                .unwrap();
            source.inline_boundary(1, parent);
            source
                .append("中", HtmlWhiteSpace::Normal, &mut checkpoint)
                .unwrap();
            let lines = plan_html_lines(
                &source,
                Some(10.0),
                None,
                &mut |text| Ok(text.chars().count() as f64 * 10.0),
                &mut checkpoint,
            )
            .unwrap();
            assert_eq!(
                lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    #[test]
    fn soft_hyphen_controls_remain_in_streamed_combining_text() {
        let text = format!("a{}", "\u{00ad}\u{0301}".repeat(8192));
        let source = NormalizedHtmlText::single_mode(&text, HtmlWhiteSpace::BreakSpaces);
        let style = super::super::TextStyle::default();
        let state = BuiltinHtmlLineWidth::deterministic(&style, 0.0);
        let mut candidate = Candidate::new(Some(state));
        candidate
            .append(&text, &source, 0, &mut || Ok::<(), Infallible>(()))
            .unwrap();
        assert_eq!(candidate.text, text);
        let width = candidate.width.unwrap();
        assert_eq!(width.width_px(), 0.43 * style.font_size);
        assert_eq!(
            width.pushed_chunks, 1,
            "the original text should be streamed without rebuilding hidden-control prefixes"
        );
    }
}
