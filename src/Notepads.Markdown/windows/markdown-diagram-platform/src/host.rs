// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::{FontCapability, PlatformError, outline};
use markdown_memory::{Family, MemoryError, MemoryScope, OwnedBlock};
#[cfg(test)]
use markdown_memory::{FundedString, FundedVec};
use markdown_mermaid::canonical::{
    CanonicalError, FontFamily, FontPlan, OutlineProvider, PathSink, RunMetrics, TextRun,
};
use markdown_mermaid::rich_text::{RichTextProvider, TextDirection, resolve_direction};
use merman_core::OperationControl;
#[cfg(test)]
use merman_render::{
    environment::{
        HostMeasurementResult, HostTextMeasurement, HostTextMeasurementRequest, HostTextMeasurer,
        TextMeasurementOperation as Op,
    },
    text::{TextMetrics, WrapMode},
};
use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};
#[cfg(test)]
use unicode_segmentation::UnicodeSegmentation;
use windows::{
    Win32::{
        Graphics::DirectWrite::*,
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
    },
    core::{BOOL, IUnknown, Interface, PCWSTR, w},
};

pub struct WindowsText {
    factory: IDWriteFactory,
    collection: IDWriteFontCollection,
    fallback: IDWriteFontFallback,
    locale: [u16; 85],
    epoch: u64,
    scope: MemoryScope,
    control: OperationControl,
    gate: Mutex<()>,
    failure: AtomicU32,
}

struct Apartment;
pub(crate) fn directwrite_error(error: windows::core::Error) -> CanonicalError {
    if error.code().0 as u32 == 0x8007000e {
        CanonicalError::Memory(MemoryError::OutOfMemory)
    } else {
        CanonicalError::FontUnavailable
    }
}
fn platform_error(error: windows::core::Error) -> PlatformError {
    if error.code().0 as u32 == 0x8007000e {
        PlatformError::Memory(MemoryError::OutOfMemory)
    } else {
        PlatformError::DirectWrite(error.code().0)
    }
}
impl Apartment {
    fn enter() -> Result<Self, CanonicalError> {
        // CoUninitialize is paired on this exact call stack, never in a Send owner Drop.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(directwrite_error)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

impl WindowsText {
    pub fn font_epoch(&self) -> u64 {
        self.epoch
    }
    pub(crate) fn same_scope(&self, scope: &MemoryScope) -> bool {
        self.scope.same_root(scope)
    }
    /// # Safety
    /// The native owner supplies live COM IUnknown pointers to the specified
    /// free-threaded DirectWrite objects and retains them until QI completes.
    /// The capability is private and never sourced from Markdown or clients.
    pub unsafe fn from_capability(
        capability: FontCapability,
        scope: MemoryScope,
        control: OperationControl,
    ) -> Result<Self, PlatformError> {
        if control
            .memory_scope()
            .is_none_or(|bound| !bound.same_root(&scope))
        {
            return Err(PlatformError::Memory(MemoryError::Unfunded));
        }
        if let Some(error) = scope.failure() {
            return Err(PlatformError::Memory(error));
        }
        control.checkpoint().map_err(|_| PlatformError::Cancelled)?;
        if capability.version != 1
            || capability.size as usize != std::mem::size_of::<FontCapability>()
            || capability.font_epoch == 0
            || capability.factory.is_null()
            || capability.collection.is_null()
            || capability.fallback.is_null()
            || capability.locale_name[0] == 0
            || !capability.locale_name.contains(&0)
        {
            return Err(PlatformError::InvalidCapability);
        }
        // SAFETY: borrowed IUnknown wrappers do not release the caller's refs;
        // QueryInterface validates each IID and returns our own strong refs.
        let factory = unsafe { IUnknown::from_raw_borrowed(&capability.factory) }
            .ok_or(PlatformError::InvalidCapability)?
            .cast::<IDWriteFactory>()
            .map_err(platform_error)?;
        let collection = unsafe { IUnknown::from_raw_borrowed(&capability.collection) }
            .ok_or(PlatformError::InvalidCapability)?
            .cast::<IDWriteFontCollection>()
            .map_err(platform_error)?;
        let fallback = unsafe { IUnknown::from_raw_borrowed(&capability.fallback) }
            .ok_or(PlatformError::InvalidCapability)?
            .cast::<IDWriteFontFallback>()
            .map_err(platform_error)?;
        let result = Self {
            factory,
            collection,
            fallback,
            locale: capability.locale_name,
            epoch: capability.font_epoch,
            scope,
            control,
            gate: Mutex::new(()),
            failure: AtomicU32::new(0),
        };
        for family in [
            FontFamily::SegoeUi,
            FontFamily::MicrosoftYaHeiUi,
            FontFamily::SegoeUiSymbol,
            FontFamily::SegoeUiEmoji,
        ] {
            result.family_index(family).map_err(|error| match error {
                CanonicalError::Memory(error) => PlatformError::Memory(error),
                _ => PlatformError::FontUnavailable,
            })?;
        }
        Ok(result)
    }

    pub fn failure(&self) -> Option<CanonicalError> {
        match self.failure.load(Ordering::Acquire) {
            0 => None,
            1 => Some(CanonicalError::Cancelled),
            2 => Some(CanonicalError::Limited),
            3 => Some(CanonicalError::Memory(MemoryError::BudgetLimited)),
            4 => Some(CanonicalError::Memory(MemoryError::OutOfMemory)),
            5 => Some(CanonicalError::Memory(MemoryError::ArithmeticOverflow)),
            6 => Some(CanonicalError::InvalidGeometry),
            8 => Some(CanonicalError::Memory(MemoryError::WorkLimited)),
            9 => Some(CanonicalError::Memory(MemoryError::Cancelled)),
            10 => Some(CanonicalError::Memory(MemoryError::Unfunded)),
            11 => Some(CanonicalError::Memory(MemoryError::InvalidState)),
            _ => Some(CanonicalError::FontUnavailable),
        }
    }

    pub(crate) fn record(&self, error: CanonicalError) {
        let code = match error {
            CanonicalError::Cancelled => 1,
            CanonicalError::Limited => 2,
            CanonicalError::Memory(MemoryError::BudgetLimited) => 3,
            CanonicalError::Memory(MemoryError::OutOfMemory) => 4,
            CanonicalError::Memory(MemoryError::ArithmeticOverflow) => 5,
            CanonicalError::InvalidGeometry => 6,
            CanonicalError::Memory(MemoryError::WorkLimited) => 8,
            CanonicalError::Memory(MemoryError::Cancelled) => 9,
            CanonicalError::Memory(MemoryError::Unfunded) => 10,
            CanonicalError::Memory(MemoryError::InvalidState) => 11,
            _ => 7,
        };
        let _ = self
            .failure
            .compare_exchange(0, code, Ordering::AcqRel, Ordering::Acquire);
        if let CanonicalError::Memory(error) = error {
            self.scope.record_failure(error);
        }
        self.control.cancel();
    }

    pub(crate) fn checkpoint(&self) -> Result<(), CanonicalError> {
        if let Some(error) = self.scope.failure() {
            return Err(error.into());
        }
        if let Some(error) = self.failure() {
            return Err(error);
        }
        self.control
            .checkpoint()
            .map_err(|_| CanonicalError::Cancelled)
    }

    fn family_index(&self, family: FontFamily) -> Result<u32, CanonicalError> {
        let mut index = 0;
        let mut exists = BOOL(0);
        unsafe {
            self.collection
                .FindFamilyName(family_name(family), &mut index, &mut exists)
        }
        .map_err(directwrite_error)?;
        if !exists.as_bool() {
            return Err(CanonicalError::FontUnavailable);
        }
        Ok(index)
    }

    fn font_facts(&self, font: FontPlan) -> Result<(f64, f64, f64), CanonicalError> {
        let family = unsafe {
            self.collection
                .GetFontFamily(self.family_index(font.family)?)
        }
        .map_err(directwrite_error)?;
        let face = unsafe {
            family.GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT(font.weight as i32),
                DWRITE_FONT_STRETCH_NORMAL,
                if font.italic {
                    DWRITE_FONT_STYLE_ITALIC
                } else {
                    DWRITE_FONT_STYLE_NORMAL
                },
            )
        }
        .map_err(directwrite_error)?;
        let mut facts = DWRITE_FONT_METRICS::default();
        unsafe {
            face.GetMetrics(&mut facts);
        }
        if facts.designUnitsPerEm == 0 {
            return Err(CanonicalError::FontUnavailable);
        }
        let scale = font.em_size / facts.designUnitsPerEm as f64;
        Ok((
            facts.ascent as f64 * scale,
            facts.descent as f64 * scale,
            facts.xHeight as f64 * scale,
        ))
    }

    fn validate_font(&self, font: FontPlan) -> Result<(), CanonicalError> {
        if !font.em_size.is_finite()
            || !(1.0..=256.0).contains(&font.em_size)
            || !(1..=999).contains(&font.weight)
            || font.decoration > 2
            || (font.epoch != 0 && font.epoch != self.epoch)
        {
            return Err(CanonicalError::FontUnavailable);
        }
        Ok(())
    }

    fn layout_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
    ) -> Result<IDWriteTextLayout, CanonicalError> {
        self.checkpoint()?;
        if spans.is_empty() || spans.len() > 4096 {
            return Err(CanonicalError::Limited);
        }
        let mut count = 0usize;
        for span in spans {
            self.validate_font(span.font)?;
            if span.text.contains(['\r', '\n']) {
                return Err(CanonicalError::InvalidGeometry);
            }
            count = count
                .checked_add(span.text.encode_utf16().count())
                .ok_or(CanonicalError::Limited)?;
        }
        if count > 16384 {
            return Err(CanonicalError::Limited);
        }
        self.scope.charge_work(count.saturating_add(spans.len()))?;
        let mut utf16 = OwnedBlock::try_array::<u16>(&self.scope, count, Family::Text)?;
        // SAFETY: exact aligned initialized u16 storage; lifetime covers CreateTextLayout's copy.
        let data =
            unsafe { std::slice::from_raw_parts_mut(utf16.as_mut_ptr().cast::<u16>(), count) };
        for (output, input) in data
            .iter_mut()
            .zip(spans.iter().flat_map(|r| r.text.encode_utf16()))
        {
            *output = input;
        }
        let font = spans[0].font;
        let format = unsafe {
            self.factory.CreateTextFormat(
                family_name(font.family),
                &self.collection,
                DWRITE_FONT_WEIGHT(font.weight as i32),
                if font.italic {
                    DWRITE_FONT_STYLE_ITALIC
                } else {
                    DWRITE_FONT_STYLE_NORMAL
                },
                DWRITE_FONT_STRETCH_NORMAL,
                font.em_size as f32,
                PCWSTR(self.locale.as_ptr()),
            )
        }
        .map_err(directwrite_error)?;
        unsafe { format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP) }
            .map_err(directwrite_error)?;
        let rtl = resolve_direction(spans, direction) == TextDirection::Rtl;
        unsafe {
            (|| -> windows::core::Result<()> {
                format.SetReadingDirection(if rtl {
                    DWRITE_READING_DIRECTION_RIGHT_TO_LEFT
                } else {
                    DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
                })?;
                // Both paragraph directions use physical left origin. Pure layout
                // centers the complete measured line, rather than a million-DIP box.
                format.SetTextAlignment(if rtl {
                    DWRITE_TEXT_ALIGNMENT_TRAILING
                } else {
                    DWRITE_TEXT_ALIGNMENT_LEADING
                })
            })()
        }
        .map_err(directwrite_error)?;
        let layout = unsafe {
            self.factory
                .CreateTextLayout(data, &format, 1_000_000.0, 1_000_000.0)
        }
        .map_err(directwrite_error)?;
        let layout2 = layout
            .cast::<IDWriteTextLayout2>()
            .map_err(directwrite_error)?;
        unsafe { layout2.SetFontFallback(&self.fallback) }.map_err(directwrite_error)?;
        let mut position = 0u32;
        for span in spans {
            self.checkpoint()?;
            let count = span.text.encode_utf16().count() as u32;
            let range = DWRITE_TEXT_RANGE {
                startPosition: position,
                length: count,
            };
            position += count;
            if count == 0 {
                continue;
            }
            unsafe {
                (|| -> windows::core::Result<()> {
                    layout.SetFontFamilyName(family_name(span.font.family), range)?;
                    layout.SetFontSize(span.font.em_size as f32, range)?;
                    layout.SetFontWeight(DWRITE_FONT_WEIGHT(span.font.weight as i32), range)?;
                    layout.SetFontStyle(
                        if span.font.italic {
                            DWRITE_FONT_STYLE_ITALIC
                        } else {
                            DWRITE_FONT_STYLE_NORMAL
                        },
                        range,
                    )?;
                    layout.SetUnderline(span.font.decoration == 1, range)?;
                    layout.SetStrikethrough(span.font.decoration == 2, range)
                })()
            }
            .map_err(directwrite_error)?;
        }
        self.checkpoint()?;
        Ok(layout)
    }

    #[cfg(test)]
    fn advance(&self, run: TextRun<'_>) -> Result<f64, CanonicalError> {
        self.checkpoint()?;
        let _apartment = Apartment::enter()?;
        let _gate = self
            .gate
            .lock()
            .map_err(|_| CanonicalError::InvalidGeometry)?;
        let layout = self.layout_spans(&[run], TextDirection::Auto)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut metrics) }.map_err(directwrite_error)?;
        self.checkpoint()?;
        Ok(metrics.widthIncludingTrailingWhitespace as f64)
    }

    fn actual(
        &self,
        run: TextRun<'_>,
        sink: Option<&mut dyn PathSink>,
    ) -> Result<RunMetrics, CanonicalError> {
        self.actual_spans(&[run], TextDirection::Auto, sink)
    }

    fn actual_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
        sink: Option<&mut dyn PathSink>,
    ) -> Result<RunMetrics, CanonicalError> {
        self.checkpoint()?;
        let _apartment = Apartment::enter()?;
        let _gate = self
            .gate
            .lock()
            .map_err(|_| CanonicalError::InvalidGeometry)?;
        let layout = self.layout_spans(spans, direction)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut metrics) }.map_err(directwrite_error)?;
        if metrics.lineCount > 16384 {
            return Err(CanonicalError::Limited);
        }
        let mut lines = OwnedBlock::try_array::<DWRITE_LINE_METRICS>(
            &self.scope,
            (metrics.lineCount as usize).max(1),
            Family::Text,
        )?;
        let data = unsafe {
            std::slice::from_raw_parts_mut(
                lines.as_mut_ptr().cast::<DWRITE_LINE_METRICS>(),
                (metrics.lineCount as usize).max(1),
            )
        };
        let mut actual = 0;
        unsafe { layout.GetLineMetrics(Some(data), &mut actual) }.map_err(directwrite_error)?;
        let (font_ascent, font_descent, x_height) = self.font_facts(spans[0].font)?;
        let (baseline, ascent, descent) = line_facts(data, actual, font_ascent, font_descent)?;
        let ink = outline::draw(
            &self.scope,
            &self.control,
            &self.collection,
            &layout,
            baseline,
            sink,
        )?;
        self.checkpoint()?;
        Ok(RunMetrics {
            advance: metrics.widthIncludingTrailingWhitespace as f64,
            ascent,
            descent,
            x_height,
            ink,
        })
    }

    #[cfg(test)]
    fn font(&self, request: HostTextMeasurementRequest<'_>) -> Result<FontPlan, CanonicalError> {
        let family = request
            .style
            .font_family
            .as_deref()
            .unwrap_or("Segoe UI")
            .split(',')
            .next()
            .ok_or(CanonicalError::FontUnavailable)?;
        Ok(FontPlan {
            epoch: self.epoch,
            family: FontFamily::parse(family).ok_or(CanonicalError::FontUnavailable)?,
            em_size: request.style.font_size,
            weight: match request.style.font_weight.as_deref().unwrap_or("normal") {
                "normal" => 400,
                "bold" => 700,
                value => value
                    .parse::<u16>()
                    .ok()
                    .filter(|n| (1..=999).contains(n))
                    .ok_or(CanonicalError::FontUnavailable)?,
            },
            decoration: 0,
            italic: match request.style.font_style.as_deref().unwrap_or("normal") {
                "normal" => false,
                "italic" | "oblique" => true,
                _ => return Err(CanonicalError::FontUnavailable),
            },
        })
    }

    fn hanging_offset(&self, font: FontPlan) -> Result<f64, CanonicalError> {
        self.checkpoint()?;
        let _apartment = Apartment::enter()?;
        let _gate = self
            .gate
            .lock()
            .map_err(|_| CanonicalError::InvalidGeometry)?;
        let family = unsafe {
            self.collection
                .GetFontFamily(self.family_index(font.family)?)
        }
        .map_err(directwrite_error)?;
        let matching = unsafe {
            family.GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT(font.weight as i32),
                DWRITE_FONT_STRETCH_NORMAL,
                if font.italic {
                    DWRITE_FONT_STYLE_ITALIC
                } else {
                    DWRITE_FONT_STYLE_NORMAL
                },
            )
        }
        .map_err(directwrite_error)?;
        let face = unsafe { matching.CreateFontFace() }.map_err(directwrite_error)?;
        let mut metrics = DWRITE_FONT_METRICS::default();
        unsafe {
            matching.GetMetrics(&mut metrics);
        }
        if metrics.designUnitsPerEm == 0 {
            return Err(CanonicalError::FontUnavailable);
        }
        let analyzer = unsafe { self.factory.CreateTextAnalyzer() }
            .map_err(directwrite_error)?
            .cast::<IDWriteTextAnalyzer1>()
            .map_err(directwrite_error)?;
        let mut coordinate = 0;
        let mut exists = BOOL(0);
        // Native baseline policy explicitly uses DirectWrite's simulated baseline
        // when the font lacks BASE. It is a Windows profile fact, not an outline
        // or Chrome pixel-parity claim; the same value drives measurement/paint.
        unsafe {
            analyzer.GetBaseline(
                &face,
                DWRITE_BASELINE_HANGING,
                false,
                true,
                DWRITE_SCRIPT_ANALYSIS::default(),
                PCWSTR(self.locale.as_ptr()),
                &mut coordinate,
                &mut exists,
            )
        }
        .map_err(directwrite_error)?;
        Ok(coordinate as f64 * font.em_size / metrics.designUnitsPerEm as f64)
    }

    #[cfg(test)]
    pub(crate) fn measurement(
        &self,
        request: HostTextMeasurementRequest<'_>,
    ) -> Result<HostTextMeasurement, CanonicalError> {
        self.checkpoint()?;
        let font = self.font(request)?;
        if matches!(
            request.operation,
            Op::Measure | Op::Wrapped | Op::WrappedWithRawWidth
        ) {
            let max = if request.operation == Op::Measure {
                None
            } else {
                request.max_width
            };
            let (metrics, raw) = self.wrapped(request.text, font, max, request.wrap_mode)?;
            return Ok(if request.operation == Op::WrappedWithRawWidth {
                HostTextMeasurement::WrappedWithRawWidth {
                    metrics,
                    raw_width: raw,
                }
            } else {
                HostTextMeasurement::Metrics(metrics)
            });
        }
        let normalized =
            if request.operation == Op::TspanBBoxWidth && request.wrap_mode == WrapMode::HtmlLike {
                FundedString::try_from_str(&self.scope, Family::Text, request.text)?
            } else {
                collapse(&self.scope, request.text)?
            };
        if matches!(
            request.operation,
            Op::ComputedLength | Op::CanvasMeasureTextWidth
        ) {
            return self
                .advance(TextRun {
                    text: normalized.as_str(),
                    font,
                })
                .map(HostTextMeasurement::Length);
        }
        let metrics = self.actual(
            TextRun {
                text: normalized.as_str(),
                font,
            },
            None,
        )?;
        let width = (metrics.ink.right - metrics.ink.left).max(0.0);
        let height = (metrics.ink.bottom - metrics.ink.top).max(0.0);
        let value = match request.operation {
            Op::ComputedLength | Op::CanvasMeasureTextWidth => {
                HostTextMeasurement::Length(metrics.advance)
            }
            Op::BBoxX | Op::BBoxXWithAsciiOverhang | Op::TitleBBoxX => {
                let (left, right) = if width == 0.0 {
                    (0.0, 0.0)
                } else {
                    (
                        (metrics.advance / 2.0 - metrics.ink.left).max(0.0),
                        (metrics.ink.right - metrics.advance / 2.0).max(0.0),
                    )
                };
                HostTextMeasurement::HorizontalExtents { left, right }
            }
            Op::SimpleBBoxWidth
            | Op::RawBBoxWidth
            | Op::TspanBBoxWidth
            | Op::WrapProbeBBoxWidth
            | Op::BoundingClientRectWidth => HostTextMeasurement::Length(width),
            Op::TspanBBoxHeight | Op::SimpleBBoxHeight | Op::RawBBoxHeight => {
                HostTextMeasurement::Length(height)
            }
            Op::RawBBoxYOffset => HostTextMeasurement::Length(metrics.ink.top),
            // Merman's createFormattedText probe overrides the outer text's
            // y=-10.1 with tspan y=-0.1em, dy=1.1em: baseline is exactly +1em.
            Op::CreateTextBBoxYOffset => {
                HostTextMeasurement::Length(font.em_size + metrics.ink.top)
            }
            Op::CreateTextMiddleBBoxYOffset => {
                HostTextMeasurement::Length(font.em_size + metrics.ink.top + metrics.x_height / 2.0)
            }
            Op::RawMiddleBBoxYOffset => {
                HostTextMeasurement::Length(metrics.ink.top + metrics.x_height / 2.0)
            }
            Op::RawHangingBBoxYOffset => {
                HostTextMeasurement::Length(metrics.ink.top + self.hanging_offset(font)?)
            }
            Op::MermaidCalculateTextDimensions => HostTextMeasurement::Metrics(TextMetrics {
                width,
                height,
                line_count: 1,
            }),
            Op::Measure | Op::Wrapped | Op::WrappedWithRawWidth => {
                return Err(CanonicalError::InvalidGeometry);
            }
        };
        Ok(value)
    }

    // The token/greedy/grapheme policy follows Merman0.9.2 text/deterministic.rs;
    // storage is replaced by the common explicit-scope collections. Original
    // Merman license/notices remain in vendor/merman-bounded (MIT OR Apache-2.0).
    #[cfg(test)]
    fn wrapped(
        &self,
        text: &str,
        font: FontPlan,
        max_width: Option<f64>,
        mode: WrapMode,
    ) -> Result<(TextMetrics, Option<f64>), CanonicalError> {
        let normalized = line_breaks(&self.scope, text)?;
        let mut lines = FundedVec::new_in(self.scope.clone(), Family::Text);
        for line in normalized.as_str().split('\n') {
            lines.try_push(line)?;
        }
        while lines.len() > 1 && lines.last().is_some_and(|s| trim(s).is_empty()) {
            lines.pop();
        }
        if max_width.is_some_and(|value| !value.is_finite() || value <= 0.0) {
            return Err(CanonicalError::InvalidGeometry);
        }
        let mut raw = 0.0f64;
        let mut width = 0.0f64;
        let mut count = 0usize;
        let facts = self.actual(TextRun { text: "", font }, None)?;
        let line_height = facts.ascent + facts.descent;
        let mut candidate = FundedString::new_in(self.scope.clone(), Family::Text);
        let mut current = FundedString::new_in(self.scope.clone(), Family::Text);
        for line in &lines {
            self.checkpoint()?;
            let raw_line = self.advance(TextRun { text: line, font })?;
            raw = raw.max(raw_line);
            let Some(limit) = max_width else {
                width = width.max(raw_line);
                count += 1;
                continue;
            };
            let mut tokens = FundedVec::new_in(self.scope.clone(), Family::Text);
            for word in line.split(' ') {
                if !word.is_empty() {
                    tokens.try_push(word)?;
                }
                tokens.try_push(" ")?;
            }
            while tokens.last() == Some(&" ") {
                tokens.pop();
            }
            current.clear();
            let mut index = 0;
            let mut emitted = 0;
            while index < tokens.len() {
                self.checkpoint()?;
                let token = tokens[index];
                if current.is_empty() && token == " " {
                    index += 1;
                    continue;
                }
                candidate.clear();
                candidate.try_push_str(current.as_str())?;
                candidate.try_push_str(token)?;
                let candidate_width = self.advance(TextRun {
                    text: trim_end(candidate.as_str()),
                    font,
                })?;
                if candidate_width <= limit {
                    current.try_push_str(token)?;
                    index += 1;
                    continue;
                }
                if !trim(current.as_str()).is_empty() {
                    let value = self.advance(TextRun {
                        text: trim_end(current.as_str()),
                        font,
                    })?;
                    width = width.max(value);
                    count += 1;
                    emitted += 1;
                    current.clear();
                    continue;
                }
                if token == " " {
                    index += 1;
                    continue;
                }
                if mode == WrapMode::HtmlLike {
                    width = width.max(self.advance(TextRun { text: token, font })?);
                    count += 1;
                    emitted += 1;
                    index += 1;
                    continue;
                }
                let mut boundary = 0;
                let mut last_width = 0.0;
                let mut visible = false;
                for (start, g) in token.grapheme_indices(true) {
                    let end = start + g.len();
                    let value = self.advance(TextRun {
                        text: &token[..end],
                        font,
                    })?;
                    if visible && value > last_width && value > limit {
                        break;
                    }
                    boundary = end;
                    last_width = value;
                    visible |= value > 0.0;
                }
                if boundary == 0 {
                    boundary = token.graphemes(true).next().map_or(token.len(), str::len);
                }
                width = width.max(self.advance(TextRun {
                    text: &token[..boundary],
                    font,
                })?);
                count += 1;
                emitted += 1;
                if boundary < token.len() {
                    tokens[index] = &token[boundary..];
                } else {
                    index += 1;
                }
            }
            if !trim(current.as_str()).is_empty() {
                width = width.max(self.advance(TextRun {
                    text: trim_end(current.as_str()),
                    font,
                })?);
                count += 1;
                emitted += 1;
            }
            if emitted == 0 {
                count += 1;
            }
        }
        if mode == WrapMode::HtmlLike && max_width.is_some_and(|max| raw > max) {
            width = width.max(max_width.unwrap_or(0.0));
        }
        Ok((
            TextMetrics {
                width,
                height: count as f64 * line_height,
                line_count: count,
            },
            if mode == WrapMode::HtmlLike {
                Some(raw)
            } else {
                None
            },
        ))
    }
}

#[cfg(test)]
impl HostTextMeasurer for WindowsText {
    fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
        self.measure_with_control(request, &self.control)
    }
    fn measure_with_control(
        &self,
        request: HostTextMeasurementRequest<'_>,
        control: &OperationControl,
    ) -> HostMeasurementResult {
        if control
            .memory_scope()
            .is_none_or(|scope| !scope.same_root(&self.scope))
        {
            self.record(CanonicalError::Memory(MemoryError::Unfunded));
            control.cancel();
            return Ok(None);
        }
        match self.measurement(request) {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                self.record(error);
                control.cancel();
                // The controlled route observes cancellation before decode/fallback.
                // Do not construct HostTextMeasurementError's String/Arc on this path.
                Ok(None)
            }
        }
    }
}

impl OutlineProvider for WindowsText {
    fn baseline_offset(&self, font: FontPlan, baseline: &str) -> Result<f64, CanonicalError> {
        self.checkpoint()?;
        self.validate_font(font)?;
        if matches!(baseline, "alphabetic" | "auto" | "baseline") {
            return Ok(0.0);
        }
        if baseline == "hanging" {
            return self.hanging_offset(font);
        }
        let m = self.actual(TextRun { text: "", font }, None)?;
        match baseline {
            "auto" | "alphabetic" | "baseline" => Ok(0.0),
            "middle" => Ok(m.x_height / 2.0),
            "central" => Ok((m.ascent - m.descent) / 2.0),
            "text-before-edge" => Ok(m.ascent),
            "text-after-edge" => Ok(-m.descent),
            _ => Err(CanonicalError::FontUnavailable),
        }
    }
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        let result = self.actual(run, None);
        if let Err(error) = result {
            self.record(error);
        }
        result
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        let result = self.actual(run, Some(sink)).map(|_| ());
        if let Err(error) = result {
            self.record(error);
        }
        result
    }
}

impl RichTextProvider for WindowsText {
    fn measure_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        let result = self.actual_spans(spans, direction, None);
        if let Err(error) = result {
            self.record(error);
        }
        result
    }

    fn outline_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        let result = self.actual_spans(spans, direction, Some(sink)).map(|_| ());
        if let Err(error) = result {
            self.record(error);
        }
        result
    }
}

fn family_name(family: FontFamily) -> PCWSTR {
    match family {
        FontFamily::SegoeUi => w!("Segoe UI"),
        FontFamily::MicrosoftYaHeiUi => w!("Microsoft YaHei UI"),
        FontFamily::SegoeUiSymbol => w!("Segoe UI Symbol"),
        FontFamily::SegoeUiEmoji => w!("Segoe UI Emoji"),
    }
}

fn line_facts(
    lines: &[DWRITE_LINE_METRICS],
    actual: u32,
    font_ascent: f64,
    font_descent: f64,
) -> Result<(f64, f64, f64), CanonicalError> {
    let lines = lines
        .get(..actual as usize)
        .ok_or(CanonicalError::InvalidGeometry)?;
    Ok(lines
        .first()
        .map_or((0.0, font_ascent, font_descent), |line| {
            let baseline = line.baseline as f64;
            (
                baseline,
                baseline,
                (line.height - line.baseline).max(0.0) as f64,
            )
        }))
}

#[cfg(test)]
mod line_tests {
    use super::*;

    #[test]
    fn zero_actual_lines_use_font_facts_instead_of_spare_capacity() {
        let lines = [DWRITE_LINE_METRICS::default()];
        assert_eq!(line_facts(&lines, 0, 12.0, 4.0), Ok((0.0, 12.0, 4.0)));
        assert_eq!(
            line_facts(&lines, 2, 12.0, 4.0),
            Err(CanonicalError::InvalidGeometry)
        );
    }
}
#[cfg(test)]
fn trim(value: &str) -> &str {
    value.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{000c}'))
}
#[cfg(test)]
fn trim_end(value: &str) -> &str {
    value.trim_end_matches([' ', '\t', '\n', '\r', '\u{000c}'])
}
#[cfg(test)]
fn collapse(scope: &MemoryScope, value: &str) -> Result<FundedString, CanonicalError> {
    let mut out = FundedString::new_in(scope.clone(), Family::Text);
    let mut pending = false;
    for ch in value.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\r') {
            pending = !out.is_empty();
        } else {
            if pending {
                out.try_push(' ')?;
                pending = false;
            }
            out.try_push(ch)?;
        }
    }
    Ok(out)
}
#[cfg(test)]
fn line_breaks(scope: &MemoryScope, text: &str) -> Result<FundedString, CanonicalError> {
    let mut out = FundedString::new_in(scope.clone(), Family::Text);
    let mut cursor = 0;
    while cursor < text.len() {
        let rest = &text[cursor..];
        if rest.len() >= 4
            && rest
                .as_bytes()
                .get(..3)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"<br"))
        {
            let mut end = 3;
            while rest
                .as_bytes()
                .get(end)
                .is_some_and(u8::is_ascii_whitespace)
            {
                end += 1;
            }
            if rest.as_bytes().get(end) == Some(&b'/') {
                end += 1;
            }
            if rest.as_bytes().get(end) == Some(&b'>') {
                out.try_push('\n')?;
                cursor += end + 1;
                continue;
            }
        }
        let ch = rest.chars().next().ok_or(CanonicalError::MalformedXml)?;
        out.try_push(ch)?;
        cursor += ch.len_utf8();
    }
    Ok(out)
}
