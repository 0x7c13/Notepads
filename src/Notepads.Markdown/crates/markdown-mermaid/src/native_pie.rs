// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded native pie charts. Authored strings only enter the funded label
//! compiler; the producer emits typed paths and never accepts authored SVG.

use crate::{
    DiagramError,
    canonical::{PathCommand, PathSink, Rect},
    labels::CompiledLabel,
    layout::{LayoutPolicy, ThemePolicy},
    native_common::{canonical, check, label, scope, shape, text},
    rich_text::RichTextProvider,
    scene::{Color, OwnedScene, Paint, Point, SceneBuilder, shapes::Shape},
};
use merman_core::OperationControl;
use markdown_memory::{Family, FundedString, FundedVec, MemoryError, MemoryScope};
use std::{
    f64::consts::{FRAC_PI_2, PI, TAU},
    fmt::Write,
};

const SOURCE_LIMIT: usize = 16 * 1024;
const ROW_LIMIT: usize = 128;
const RADIUS: f64 = 150.0;
const LEGEND_GAP: f64 = 28.0;
const SWATCH: f64 = 16.0;

// Twelve colors retain separate boundaries even when the palette repeats.
// Each slice chooses black/white text using its actual relative luminance;
// title, legend, boundaries and the no-data state use the injected theme.
const PALETTE: [u32; 12] = [
    0x4477aa, 0xee6677, 0x228833, 0xccbb44, 0x66ccee, 0xaa3377, 0xbbbbbb, 0x332288, 0x44aa99,
    0x999933, 0x882255, 0xddcc77,
];

struct Entry<'a> {
    name: FundedString,
    raw_value: &'a str,
    value: f64,
}

struct Pie<'a> {
    entries: FundedVec<Entry<'a>>,
    title: Option<FundedString>,
    show_data: bool,
}

struct Row {
    legend: CompiledLabel,
    percent: CompiledLabel,
    fraction: f64,
    color: Color,
}

fn limited<T>(scope: &MemoryScope) -> Result<T, DiagramError> {
    scope.record_failure(MemoryError::WorkLimited);
    Err(DiagramError::WorkLimited)
}

/// Supports the fixed vendor pie grammar: optional showData and title, quoted
/// sections, decimal values, comments, title and inline accTitle/accDescr.
/// Acc fields are validated nonvisual metadata; this scene has no separate
/// accessibility API, and the enclosing source descriptor retains their text.
/// Configuration directives and block accDescr are explicitly unsupported.
pub fn render_pie_native_controlled(
    source: &str,
    policy: LayoutPolicy,
    theme: ThemePolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<OwnedScene, DiagramError> {
    let funded = scope(control)?;
    let pie = Pie::parse(source, &funded, control)?;
    check(control, 1)?;
    if !policy.margin.is_finite() || !(0.0..=256.0).contains(&policy.margin) {
        return Err(DiagramError::InvalidGeometry);
    }
    let title = pie
        .title
        .as_ref()
        .map(|value| label(value, policy, provider, control))
        .transpose()?;
    let fractions = pie.fractions(&funded, control)?;
    let mut rows = FundedVec::new_in(funded.clone(), Family::Text);
    let mut legend_height = 0.0f64;
    let mut legend_width = 0.0f64;
    for (index, entry) in pie.entries.iter().enumerate() {
        check(
            control,
            entry
                .name
                .len()
                .saturating_add(entry.raw_value.len())
                .saturating_add(1),
        )?;
        let mut percentage = FundedString::new_in(funded.clone(), Family::Text);
        write!(percentage, "{:.1}%", fractions[index] * 100.0)
            .map_err(|_| format_error(&funded))?;
        let percent = label(&percentage, policy, provider, control)?;
        let mut legend = FundedString::new_in(funded.clone(), Family::Text);
        legend.try_push_str(&entry.name)?;
        legend.try_push_str(" — ")?;
        legend.try_push_str(&percentage)?;
        if pie.show_data {
            legend.try_push_str(" [")?;
            legend.try_push_str(entry.raw_value)?;
            legend.try_push_str("]")?;
        }
        let legend = label(&legend, policy, provider, control)?;
        legend_height += legend.height.max(SWATCH) + 10.0;
        legend_width = legend_width.max(legend.width);
        rows.try_push(Row {
            legend,
            percent,
            fraction: fractions[index],
            color: Color::rgb(PALETTE[index % PALETTE.len()]),
        })?;
    }
    if !rows.is_empty() {
        legend_height -= 10.0;
    }
    let content_width = if rows.is_empty() {
        RADIUS * 2.0
    } else {
        RADIUS * 2.0 + LEGEND_GAP + SWATCH + 10.0 + legend_width
    };
    let content_height = (RADIUS * 2.0).max(legend_height);
    let top = title.as_ref().map_or(0.0, |value| value.height + 20.0);
    let center = Point {
        x: RADIUS,
        y: top + content_height / 2.0,
    };
    let mut scene = SceneBuilder::new(&funded, 4 * 1024 * 1024, 65_536);
    if let Some(title) = &title {
        text(
            &mut scene,
            provider,
            title,
            theme.text,
            (content_width - title.width) / 2.0,
            0.0,
        )
        .map_err(|error| stopped(error, control))?;
    }
    let positive = rows.iter().filter(|row| row.fraction > 0.0).count();
    if positive == 0 {
        let mut paint = theme.node;
        paint.stroke = Some(theme.text);
        shape(
            &mut scene,
            Shape::Circle,
            circle_rect(center, RADIUS),
            paint,
        )
        .map_err(|error| stopped(error, control))?;
        let no_data = label("No data", policy, provider, control)?;
        text(
            &mut scene,
            provider,
            &no_data,
            theme.text,
            center.x - no_data.width / 2.0,
            center.y - no_data.height / 2.0,
        )
        .map_err(|error| stopped(error, control))?;
    } else {
        let mut start = -FRAC_PI_2;
        let last_positive = rows.iter().rposition(|row| row.fraction > 0.0).unwrap_or(0);
        for (index, row) in rows.iter().enumerate() {
            check(control, 1)?;
            if row.fraction == 0.0 {
                continue;
            }
            let end = if index == last_positive {
                TAU - FRAC_PI_2
            } else {
                (start + row.fraction * TAU).min(TAU - FRAC_PI_2)
            };
            let paint = Paint {
                fill: Some(row.color),
                stroke: Some(theme.text),
                stroke_width: 1.5,
                ..Paint::default()
            };
            if positive == 1 {
                shape(
                    &mut scene,
                    Shape::Circle,
                    circle_rect(center, RADIUS),
                    paint,
                )
                .map_err(|error| stopped(error, control))?;
            } else if end > start {
                wedge(&mut scene, center, start, end, paint, control)?;
            }
            // A bounding circle around the label must fit inside its sector.
            // These disjoint containment tests prevent adjacent text overlap.
            let half_diagonal = row.percent.width.hypot(row.percent.height) / 2.0;
            let label_radius = if positive == 1 { 0.0 } else { RADIUS * 0.65 };
            let span = end - start;
            let clearance = if span >= PI {
                label_radius
            } else {
                label_radius * (span / 2.0).sin()
            };
            if (positive == 1 && half_diagonal + 6.0 <= RADIUS)
                || (positive > 1
                    && half_diagonal + 6.0 <= clearance
                    && label_radius + half_diagonal + 6.0 <= RADIUS)
            {
                let angle = (start + end) / 2.0;
                let (sin, cos) = angle.sin_cos();
                text(
                    &mut scene,
                    provider,
                    &row.percent,
                    slice_text(row.color),
                    center.x + label_radius * cos - row.percent.width / 2.0,
                    center.y + label_radius * sin - row.percent.height / 2.0,
                )
                .map_err(|error| stopped(error, control))?;
            }
            start = end;
        }
    }
    let legend_x = RADIUS * 2.0 + LEGEND_GAP;
    let mut legend_y = top + (content_height - legend_height) / 2.0;
    for row in &rows {
        check(control, 1)?;
        let height = row.legend.height.max(SWATCH);
        shape(
            &mut scene,
            Shape::Rectangle,
            Rect {
                left: legend_x,
                top: legend_y + (height - SWATCH) / 2.0,
                right: legend_x + SWATCH,
                bottom: legend_y + (height + SWATCH) / 2.0,
            },
            Paint {
                fill: Some(row.color),
                stroke: Some(theme.text),
                stroke_width: 1.0,
                ..Paint::default()
            },
        )
        .map_err(|error| stopped(error, control))?;
        text(
            &mut scene,
            provider,
            &row.legend,
            theme.text,
            legend_x + SWATCH + 10.0,
            legend_y + (height - row.legend.height) / 2.0,
        )
        .map_err(|error| stopped(error, control))?;
        legend_y += height + 10.0;
    }
    check(control, 1)?;
    let result = scene
        .finish(policy.margin)
        .map_err(|error| canonical(error, control))?;
    check(control, 1)?;
    Ok(result)
}

fn format_error(scope: &MemoryScope) -> DiagramError {
    DiagramError::Memory(scope.failure().unwrap_or(MemoryError::InvalidState))
}

fn stopped(error: DiagramError, control: &OperationControl) -> DiagramError {
    if let Some(failure) = control.memory_scope().and_then(MemoryScope::failure) {
        return DiagramError::Memory(failure);
    }
    if error == DiagramError::Cancelled {
        return check(control, 0).err().unwrap_or(error);
    }
    error
}

fn circle_rect(center: Point, radius: f64) -> Rect {
    Rect {
        left: center.x - radius,
        top: center.y - radius,
        right: center.x + radius,
        bottom: center.y + radius,
    }
}

// Each arc is at most one quarter circle, so cubic tangents remain bounded.
// The endpoints carry the requested angle exactly; no coarse polygon changes
// the ratio, and a single positive entry uses the four-cubic full circle.
fn wedge(
    scene: &mut SceneBuilder,
    center: Point,
    start: f64,
    end: f64,
    paint: Paint,
    control: &OperationControl,
) -> Result<(), DiagramError> {
    let span = end - start;
    if !span.is_finite() || span <= 0.0 || span > TAU + 1.0e-12 {
        return Err(DiagramError::InvalidGeometry);
    }
    let count = ((span / FRAC_PI_2).ceil() as usize).clamp(1, 4);
    scene
        .begin_path(paint, Point::default())
        .map_err(|error| canonical(error, control))?;
    scene
        .push(PathCommand::move_to(center.x, center.y))
        .map_err(|error| canonical(error, control))?;
    let (sin, cos) = start.sin_cos();
    scene
        .push(PathCommand::line_to(
            center.x + RADIUS * cos,
            center.y + RADIUS * sin,
        ))
        .map_err(|error| canonical(error, control))?;
    for part in 0..count {
        check(control, 1)?;
        let a = start + span * part as f64 / count as f64;
        let b = start + span * (part + 1) as f64 / count as f64;
        let k = 4.0 / 3.0 * ((b - a) / 4.0).tan();
        let (sa, ca) = a.sin_cos();
        let (sb, cb) = b.sin_cos();
        scene
            .push(PathCommand::cubic(
                center.x + RADIUS * (ca - k * sa),
                center.y + RADIUS * (sa + k * ca),
                center.x + RADIUS * (cb + k * sb),
                center.y + RADIUS * (sb - k * cb),
                center.x + RADIUS * cb,
                center.y + RADIUS * sb,
            ))
            .map_err(|error| canonical(error, control))?;
    }
    scene
        .push(PathCommand::close())
        .map_err(|error| canonical(error, control))?;
    scene.end_path().map_err(|error| canonical(error, control))
}

fn slice_text(color: Color) -> Color {
    fn channel(value: u32) -> f64 {
        let value = value as f64 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    let luminance = channel((color.rgb >> 16) & 255) * 0.2126
        + channel((color.rgb >> 8) & 255) * 0.7152
        + channel(color.rgb & 255) * 0.0722;
    if (luminance + 0.05) / 0.05 >= 1.05 / (luminance + 0.05) {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

impl<'a> Pie<'a> {
    fn parse(
        source: &'a str,
        funded: &MemoryScope,
        control: &OperationControl,
    ) -> Result<Self, DiagramError> {
        if source.len() > SOURCE_LIMIT {
            return limited(funded);
        }
        check(control, source.len().saturating_add(1))?;
        validate_text(source)?;
        let mut pie = Self {
            entries: FundedVec::new_in(funded.clone(), Family::Parse),
            title: None,
            show_data: false,
        };
        let mut header = false;
        let mut row_count = 0usize;
        for raw in source.trim_start_matches('\u{feff}').lines() {
            check(control, raw.len().saturating_add(1))?;
            let raw = raw.trim();
            if raw.starts_with("%%{") {
                return Err(DiagramError::Unsupported);
            }
            let mut line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if !header {
                line = keyword(line, "pie")
                    .ok_or(DiagramError::InvalidInput)?
                    .trim_start();
                header = true;
                if let Some(rest) = keyword(line, "showData") {
                    pie.show_data = true;
                    line = rest.trim_start();
                }
                if line.is_empty() {
                    continue;
                }
            }
            if let Some(rest) = keyword(line, "title") {
                pie.title = Some(normalize_inline(rest, funded)?);
                continue;
            }
            if let Some(rest) = line.strip_prefix("accTitle") {
                let rest = rest
                    .trim_start()
                    .strip_prefix(':')
                    .ok_or(DiagramError::InvalidInput)?;
                validate_text(rest)?;
                continue;
            }
            if let Some(rest) = line.strip_prefix("accDescr") {
                let rest = rest.trim_start();
                if rest.starts_with('{') || rest.is_empty() {
                    return Err(DiagramError::Unsupported);
                }
                validate_text(rest.strip_prefix(':').ok_or(DiagramError::InvalidInput)?)?;
                continue;
            }
            if !line.starts_with(['"', '\'']) {
                let head = line.split_whitespace().next().unwrap_or("");
                return Err(
                    if matches!(
                        head,
                        "style"
                            | "classDef"
                            | "class"
                            | "click"
                            | "direction"
                            | "config"
                            | "init"
                            | "theme"
                            | "linkStyle"
                    ) {
                        DiagramError::Unsupported
                    } else {
                        DiagramError::InvalidInput
                    },
                );
            }
            row_count += 1;
            if row_count > ROW_LIMIT {
                return limited(funded);
            }
            let (name, rest) = quoted(line, funded, control)?;
            let raw_value = rest
                .trim_start()
                .strip_prefix(':')
                .ok_or(DiagramError::InvalidInput)?
                .trim();
            let value = number(raw_value)?;
            // Vendor semantics: first label wins, but every duplicate still
            // has to parse and satisfy the nonnegative finite value rule.
            check(
                control,
                pie.entries
                    .len()
                    .saturating_mul(name.len().saturating_add(1)),
            )?;
            if pie.entries.iter().any(|entry| entry.name == name) {
                continue;
            }
            pie.entries.try_push(Entry {
                name,
                value,
                raw_value,
            })?;
        }
        if !header {
            return Err(DiagramError::InvalidInput);
        }
        check(control, 1)?;
        Ok(pie)
    }

    fn fractions(
        &self,
        funded: &MemoryScope,
        control: &OperationControl,
    ) -> Result<FundedVec<f64>, DiagramError> {
        check(
            control,
            self.entries.len().saturating_mul(3).saturating_add(1),
        )?;
        let maximum = self
            .entries
            .iter()
            .map(|entry| entry.value)
            .fold(0.0f64, f64::max);
        let mut fractions = FundedVec::new_in(funded.clone(), Family::Work);
        let mut sum = 0.0;
        let mut correction = 0.0;
        for entry in &self.entries {
            check(control, 1)?;
            let value = if maximum == 0.0 {
                0.0
            } else {
                entry.value / maximum
            };
            fractions.try_push(value)?;
            let next = value - correction;
            let total = sum + next;
            correction = (total - sum) - next;
            sum = total;
        }
        if sum > 0.0 {
            for value in &mut fractions {
                check(control, 1)?;
                *value /= sum;
            }
        }
        Ok(fractions)
    }
}

fn validate_text(value: &str) -> Result<(), DiagramError> {
    if value
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\r' | '\n'))
    {
        Err(DiagramError::InvalidInput)
    } else {
        Ok(())
    }
}

fn keyword<'a>(value: &'a str, word: &str) -> Option<&'a str> {
    let rest = value.strip_prefix(word)?;
    (rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace)).then_some(rest)
}

fn strip_comment(value: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    let mut chars = value.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escaped = true;
            continue;
        }
        if matches!(ch, '"' | '\'') {
            match quote {
                Some(open) if open == ch => quote = None,
                None => quote = Some(ch),
                _ => {}
            }
        }
        if quote.is_none() && ch == '%' && chars.peek().is_some_and(|(_, next)| *next == '%') {
            return &value[..index];
        }
    }
    value
}

fn normalize_inline(value: &str, funded: &MemoryScope) -> Result<FundedString, DiagramError> {
    // Unlike quoted section labels, common inline terminals end at %% even
    // inside quotes and collapse runs of horizontal whitespace like vendor.
    let value = value
        .split_once("%%")
        .map_or(value, |(visible, _)| visible)
        .trim();
    validate_text(value)?;
    let mut result = FundedString::new_in(funded.clone(), Family::Text);
    let mut horizontal_count = 0usize;
    let mut horizontal_char = ' ';
    for ch in value.chars() {
        if matches!(ch, ' ' | '\t') {
            horizontal_count += 1;
            horizontal_char = ch;
            continue;
        }
        if horizontal_count > 0 {
            result.try_push(if horizontal_count == 1 {
                horizontal_char
            } else {
                ' '
            })?;
            horizontal_count = 0;
        }
        result.try_push(ch)?;
    }
    Ok(result)
}

fn quoted<'a>(
    value: &'a str,
    funded: &MemoryScope,
    control: &OperationControl,
) -> Result<(FundedString, &'a str), DiagramError> {
    let mut chars = value.char_indices();
    let (_, quote) = chars.next().ok_or(DiagramError::InvalidInput)?;
    if !matches!(quote, '"' | '\'') {
        return Err(DiagramError::InvalidInput);
    }
    let mut result = FundedString::new_in(funded.clone(), Family::Text);
    let mut escaped = false;
    for (index, ch) in chars {
        check(control, 1)?;
        if escaped {
            if matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}') {
                return Err(DiagramError::InvalidInput);
            }
            result.try_push(ch)?;
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Ok((result, &value[index + ch.len_utf8()..]));
        } else {
            result.try_push(ch)?;
        }
    }
    Err(DiagramError::InvalidInput)
}

fn number(value: &str) -> Result<f64, DiagramError> {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (integer, fraction) = unsigned
        .split_once('.')
        .map_or((unsigned, None), |(a, b)| (a, Some(b)));
    if integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || fraction
            .is_some_and(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        || (fraction.is_none() && integer.len() > 1 && integer.starts_with('0'))
    {
        return Err(DiagramError::InvalidInput);
    }
    let value = value
        .parse::<f64>()
        .map_err(|_| DiagramError::InvalidInput)?;
    if !value.is_finite() || value < 0.0 {
        return Err(DiagramError::InvalidInput);
    }
    Ok(value)
}

#[cfg(test)]
#[path = "pie/tests.rs"]
mod tests;
