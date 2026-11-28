// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::canonical::RunMetrics;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn layout(
    label: &mut CompiledLabel,
    provider: &dyn RichTextProvider,
) -> Result<(), CanonicalError> {
    let mut paragraph_start = 0;
    loop {
        let end = label.text.as_str()[paragraph_start..]
            .find('\n')
            .map_or(label.text.len(), |n| paragraph_start + n);
        paragraph(label, provider, paragraph_start, end)?;
        if end == label.text.len() {
            break;
        }
        paragraph_start = end + 1;
    }
    let mut top = 0.0;
    let mut seen = false;
    for line in &mut label.lines {
        line.baseline = top + line.ascent;
        let x = (label.width - line.width) / 2.0 - line.left;
        if line.ink.right > line.ink.left && line.ink.bottom > line.ink.top {
            let r = Rect {
                left: x + line.ink.left,
                right: x + line.ink.right,
                top: line.baseline + line.ink.top,
                bottom: line.baseline + line.ink.bottom,
            };
            label.ink = if !seen {
                r
            } else {
                Rect {
                    left: label.ink.left.min(r.left),
                    right: label.ink.right.max(r.right),
                    top: label.ink.top.min(r.top),
                    bottom: label.ink.bottom.max(r.bottom),
                }
            };
            seen = true;
        }
        top += line.ascent + line.descent + label.policy.line_gap;
    }
    label.height = (top - label.policy.line_gap).max(0.0);
    if !valid(label.width) || !valid(label.height) {
        return Err(CanonicalError::InvalidGeometry);
    }
    Ok(())
}

fn paragraph(
    label: &mut CompiledLabel,
    provider: &dyn RichTextProvider,
    start: usize,
    end: usize,
) -> Result<(), CanonicalError> {
    // Resolve the complete explicit paragraph before choosing soft line breaks.
    // The funded temporary spans drop here, rather than remaining live while
    // probing and storing each line. Windows receives the same explicit base
    // direction for measurement and outlines, including Latin/neutral tails.
    let direction = {
        let runs = label.runs(start..end)?;
        resolve_direction(&runs, label.policy.direction)
    };
    if start == end {
        return emit(label, provider, start..end, direction, None);
    }
    let mut current = start;
    while current < end {
        check(&label.scope, &label.control, 1)?;
        let mut measured = None;
        let mut last_space = None;
        let mut chosen = end;
        for (offset, grapheme) in label.text.as_str()[current..end].grapheme_indices(true) {
            let candidate_end = current + offset + grapheme.len();
            let runs = label.runs(current..candidate_end)?;
            let metrics = provider.measure_spans(&runs, direction)?;
            let width = metrics.advance.max(metrics.ink.right) - metrics.ink.left.min(0.0);
            if !valid(width) {
                return Err(CanonicalError::InvalidGeometry);
            }
            // A separating space is itself a usable break even when adding
            // that space is the first thing that exceeds the line width.
            if grapheme == " " {
                last_space = Some(current + offset);
            }
            if width > label.policy.wrap_width {
                let (measured_end, _) = *measured.get_or_insert((candidate_end, metrics));
                chosen = last_space
                    .filter(|&at| at > current)
                    .unwrap_or(measured_end);
                break;
            }
            measured = Some((candidate_end, metrics));
        }
        let trimmed = label.text.as_str()[current..chosen]
            .trim_end_matches(' ')
            .len()
            + current;
        // Trimming or breaking at an earlier space may select a different shaped run.
        let metrics = measured
            .filter(|(at, _)| *at == trimmed)
            .map(|(_, metrics)| metrics);
        emit(label, provider, current..trimmed, direction, metrics)?;
        current = chosen;
        while current < end && label.text.as_str().as_bytes()[current] == b' ' {
            current += 1;
        }
    }
    Ok(())
}

fn emit(
    label: &mut CompiledLabel,
    provider: &dyn RichTextProvider,
    range: Range<usize>,
    direction: TextDirection,
    measured: Option<RunMetrics>,
) -> Result<(), CanonicalError> {
    if label.lines.len() >= 1024 {
        return Err(CanonicalError::Limited);
    }
    let metrics = match measured {
        Some(metrics) => {
            check(&label.scope, &label.control, 1)?;
            metrics
        }
        None => {
            let runs = label.runs(range.clone())?;
            provider.measure_spans(&runs, direction)?
        }
    };
    if ![
        metrics.advance,
        metrics.ascent,
        metrics.descent,
        metrics.ink.left,
        metrics.ink.right,
        metrics.ink.top,
        metrics.ink.bottom,
    ]
    .into_iter()
    .all(valid)
        || metrics.advance < 0.0
        || metrics.ascent <= 0.0
        || metrics.descent < 0.0
    {
        return Err(CanonicalError::InvalidGeometry);
    }
    let left = metrics.ink.left.min(0.0);
    let width = metrics.advance.max(metrics.ink.right) - left;
    label.width = label.width.max(width);
    label.lines.try_push(Line {
        range,
        direction,
        left,
        width,
        ascent: metrics.ascent.max(-metrics.ink.top),
        descent: metrics.descent.max(metrics.ink.bottom),
        ink: metrics.ink,
        baseline: 0.0,
    })?;
    Ok(())
}
