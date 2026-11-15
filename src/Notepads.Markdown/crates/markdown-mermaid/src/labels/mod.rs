// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

mod scan;
#[cfg(test)]
mod tests;
mod wrap;

use crate::{
    canonical::{CanonicalError, FontPlan, PathCommand, PathSink, PathVerb, Rect, TextRun},
    rich_text::{RichTextProvider, TextDirection, resolve_direction},
};
use markdown_memory::{Family, FundedString, FundedVec, MemoryError, MemoryScope};
use merman_core::OperationControl;
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
pub struct LabelPolicy {
    pub font: FontPlan,
    pub wrap_width: f64,
    pub direction: TextDirection,
    pub line_gap: f64,
}
impl Default for LabelPolicy {
    fn default() -> Self {
        Self {
            font: FontPlan::default(),
            wrap_width: 240.0,
            direction: TextDirection::Auto,
            line_gap: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Span {
    range: Range<usize>,
    font: FontPlan,
}
struct Line {
    range: Range<usize>,
    direction: TextDirection,
    left: f64,
    width: f64,
    ascent: f64,
    descent: f64,
    ink: Rect,
    baseline: f64,
}

pub struct CompiledLabel {
    pub width: f64,
    pub height: f64,
    pub ink: Rect,
    text: FundedString,
    spans: FundedVec<Span>,
    lines: FundedVec<Line>,
    policy: LabelPolicy,
    scope: MemoryScope,
    control: OperationControl,
}

pub fn compile_label(
    raw: &str,
    markdown: bool,
    policy: LabelPolicy,
    scope: &MemoryScope,
    control: &OperationControl,
    provider: &dyn RichTextProvider,
) -> Result<CompiledLabel, CanonicalError> {
    if raw.len() > 16 * 1024
        || !valid(policy.wrap_width)
        || policy.wrap_width <= 0.0
        || !valid(policy.line_gap)
        || policy.line_gap < 0.0
        || policy.line_gap > 256.0
        || !policy.font.em_size.is_finite()
        || !(1.0..=256.0).contains(&policy.font.em_size)
    {
        return Err(CanonicalError::Limited);
    }
    if control
        .memory_scope()
        .is_none_or(|bound| !bound.same_root(scope))
    {
        return Err(CanonicalError::Memory(MemoryError::Unfunded));
    }
    check(scope, control, raw.len().saturating_add(1))?;
    let (text, spans) = scan::parse(raw, markdown, policy.font, scope, control)?;
    let mut label = CompiledLabel {
        width: 0.0,
        height: 0.0,
        ink: Rect::default(),
        text,
        spans,
        lines: FundedVec::new_in(scope.clone(), Family::Text),
        policy,
        scope: scope.clone(),
        control: control.clone(),
    };
    wrap::layout(&mut label, provider)?;
    Ok(label)
}

impl CompiledLabel {
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
    pub fn text(&self) -> &str {
        self.text.as_str()
    }

    pub(crate) fn paragraph_end(&self, index: usize) -> Result<Option<f64>, CanonicalError> {
        check(
            &self.scope,
            &self.control,
            self.text.len() + self.lines.len(),
        )?;
        let Some((end, _)) = self.text.as_str().match_indices('\n').nth(index) else {
            return Ok(None);
        };
        Ok(self
            .lines
            .iter()
            .take_while(|line| line.range.end <= end)
            .last()
            .map(|line| line.baseline + line.descent + self.policy.line_gap / 2.0))
    }

    pub fn outline_at(
        &self,
        provider: &dyn RichTextProvider,
        origin_x: f64,
        origin_y: f64,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        if !valid(origin_x) || !valid(origin_y) {
            return Err(CanonicalError::InvalidGeometry);
        }
        for line in &self.lines {
            check(&self.scope, &self.control, 1)?;
            let runs = self.runs(line.range.clone())?;
            let x = origin_x + (self.width - line.width) / 2.0 - line.left;
            let y = origin_y + line.baseline;
            let mut translated = Translated { sink, x, y };
            provider.outline_spans(&runs, line.direction, &mut translated)?;
        }
        check(&self.scope, &self.control, 1)
    }

    fn runs(&self, range: Range<usize>) -> Result<FundedVec<TextRun<'_>>, CanonicalError> {
        check(
            &self.scope,
            &self.control,
            self.spans.len().saturating_add(range.len()),
        )?;
        let mut runs = FundedVec::new_in(self.scope.clone(), Family::Text);
        for span in &self.spans {
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            if start < end {
                runs.try_push(TextRun {
                    text: &self.text.as_str()[start..end],
                    font: span.font,
                })?;
            }
        }
        if runs.is_empty() {
            runs.try_push(TextRun {
                text: "",
                font: self.policy.font,
            })?;
        }
        Ok(runs)
    }
}

struct Translated<'a> {
    sink: &'a mut dyn PathSink,
    x: f64,
    y: f64,
}
impl PathSink for Translated<'_> {
    fn push(&mut self, mut c: PathCommand) -> Result<(), CanonicalError> {
        if c.verb != PathVerb::Close {
            c.x1 += self.x;
            c.y1 += self.y;
        }
        if matches!(c.verb, PathVerb::Quad | PathVerb::Cubic) {
            c.x2 += self.x;
            c.y2 += self.y;
        }
        if c.verb == PathVerb::Cubic {
            c.x3 += self.x;
            c.y3 += self.y;
        }
        if ![c.x1, c.y1, c.x2, c.y2, c.x3, c.y3].into_iter().all(valid) {
            return Err(CanonicalError::InvalidGeometry);
        }
        self.sink.push(c)
    }
}

fn valid(v: f64) -> bool {
    v.is_finite() && v.abs() <= 1.0e9
}
fn check(
    scope: &MemoryScope,
    control: &OperationControl,
    work: usize,
) -> Result<(), CanonicalError> {
    if let Some(error) = scope.failure() {
        return Err(error.into());
    }
    scope.charge_work(work)?;
    control.checkpoint().map_err(|_| CanonicalError::Cancelled)
}

fn same_font(a: FontPlan, b: FontPlan) -> bool {
    a.epoch == b.epoch
        && a.family == b.family
        && a.em_size.to_bits() == b.em_size.to_bits()
        && a.weight == b.weight
        && a.italic == b.italic
        && a.decoration == b.decoration
}
