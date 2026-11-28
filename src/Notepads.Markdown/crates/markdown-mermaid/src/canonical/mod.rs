// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private, funded compiler for Merman's static flowchart SVG output.
//! Text outlines are supplied by the platform through a Rust-only trait. No
//! Windows pointer, third-party tree, or container crosses the artifact ABI.

mod artifact;
mod css;
mod emit;
mod geometry;
#[cfg(test)]
mod tests;
mod xml;

use merman_core::OperationControl;
use markdown_memory::{Family, FundedString, MemoryError, MemoryScope};

pub use artifact::{ArtifactFacts, accept_artifact};
pub use geometry::{Matrix, PathCommand, PathVerb, Rect};

/// Shared pure CSS color primitive; it does not create a stylesheet or DOM.
pub fn parse_color_value(value: &str, scope: &MemoryScope) -> Result<(u32, u8), CanonicalError> {
    scope.charge_work(
        value
            .len()
            .checked_add(2048)
            .ok_or(CanonicalError::Limited)?,
    )?;
    let color = css::color(value.trim())?;
    let r = (color.r * 255.0).round() as u32;
    let g = (color.g * 255.0).round() as u32;
    let b = (color.b * 255.0).round() as u32;
    Ok(((r << 16) | (g << 8) | b, (color.a * 255.0).round() as u8))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalError {
    Memory(MemoryError),
    Cancelled,
    Limited,
    MalformedXml,
    UnsupportedElement,
    UnsupportedProperty,
    UnsupportedCss,
    InvalidGeometry,
    InvalidReference,
    FontUnavailable,
}

impl From<MemoryError> for CanonicalError {
    fn from(value: MemoryError) -> Self {
        Self::Memory(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFamily {
    SegoeUi,
    MicrosoftYaHeiUi,
    SegoeUiSymbol,
    SegoeUiEmoji,
}

impl FontFamily {
    pub fn name(self) -> &'static str {
        match self {
            Self::SegoeUi => "Segoe UI",
            Self::MicrosoftYaHeiUi => "Microsoft YaHei UI",
            Self::SegoeUiSymbol => "Segoe UI Symbol",
            Self::SegoeUiEmoji => "Segoe UI Emoji",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().trim_matches(['\'', '"']);
        [
            Self::SegoeUi,
            Self::MicrosoftYaHeiUi,
            Self::SegoeUiSymbol,
            Self::SegoeUiEmoji,
        ]
        .into_iter()
        .find(|family| family.name().eq_ignore_ascii_case(value))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FontPlan {
    pub epoch: u64,
    pub family: FontFamily,
    pub em_size: f64,
    pub weight: u16,
    pub italic: bool,
    /// 0 none, 1 underline, 2 strikethrough; actual platform decoration metrics.
    pub decoration: u8,
}

impl Default for FontPlan {
    fn default() -> Self {
        Self {
            epoch: 0,
            family: FontFamily::SegoeUi,
            em_size: 16.0,
            weight: 400,
            italic: false,
            decoration: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextRun<'a> {
    pub text: &'a str,
    pub font: FontPlan,
}

/// Every y coordinate and ink bound is relative to the alphabetic baseline.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunMetrics {
    pub advance: f64,
    pub ascent: f64,
    pub descent: f64,
    pub x_height: f64,
    pub ink: Rect,
}

pub trait PathSink {
    fn push(&mut self, command: PathCommand) -> Result<(), CanonicalError>;
}

pub trait OutlineProvider {
    fn baseline_offset(&self, font: FontPlan, baseline: &str) -> Result<f64, CanonicalError> {
        if matches!(baseline, "alphabetic" | "auto" | "baseline") {
            return Ok(0.0);
        }
        let metrics = self.measure(TextRun { text: "", font })?;
        match baseline {
            "middle" => Ok(metrics.x_height / 2.0),
            "central" => Ok((metrics.ascent - metrics.descent) / 2.0),
            "text-before-edge" => Ok(metrics.ascent),
            "text-after-edge" => Ok(-metrics.descent),
            _ => Err(CanonicalError::FontUnavailable),
        }
    }
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError>;
    /// Writes one actual shaped run at alphabetic baseline (0,0), y down.
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError>;
}

#[derive(Clone, Copy, Debug)]
pub struct CanonicalLimits {
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub elements: usize,
    pub attributes: usize,
    pub rules: usize,
    pub depth: usize,
    pub commands: usize,
    pub expansions: usize,
}

impl Default for CanonicalLimits {
    fn default() -> Self {
        Self {
            input_bytes: 4 * 1024 * 1024,
            output_bytes: 4 * 1024 * 1024,
            elements: 16384,
            attributes: 131072,
            rules: 4096,
            depth: 64,
            commands: 65536,
            expansions: 65536,
        }
    }
}

pub struct CanonicalDocument {
    bytes: FundedString,
    pub width: f64,
    pub height: f64,
    pub path_commands: usize,
    pub text_runs: usize,
}

impl CanonicalDocument {
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_str().as_bytes()
    }
}

pub fn compile(
    svg: &str,
    scope: &MemoryScope,
    control: &OperationControl,
    outlines: &dyn OutlineProvider,
    limits: CanonicalLimits,
) -> Result<CanonicalDocument, CanonicalError> {
    let hard = CanonicalLimits::default();
    if svg.len() > limits.input_bytes
        || limits.depth == 0
        || limits.depth > hard.depth
        || limits.input_bytes > hard.input_bytes
        || limits.output_bytes > hard.output_bytes
        || limits.elements > hard.elements
        || limits.attributes > hard.attributes
        || limits.rules > hard.rules
        || limits.commands > hard.commands
        || limits.expansions > hard.expansions
    {
        return Err(CanonicalError::Limited);
    }
    if control
        .memory_scope()
        .is_none_or(|bound| !bound.same_root(scope))
    {
        return Err(CanonicalError::Memory(MemoryError::Unfunded));
    }
    checkpoint(scope, control)?;
    let tree = xml::Tree::parse(svg, scope, control, limits)?;
    let sheet = css::Sheet::parse(&tree, scope, control, limits)?;
    emit::Compiler::new(&tree, &sheet, scope, control, outlines, limits)?.run()
}

pub(crate) fn checkpoint(
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<(), CanonicalError> {
    if let Some(error) = scope.failure() {
        return Err(error.into());
    }
    scope.charge_work(1)?;
    control.checkpoint().map_err(|_| CanonicalError::Cancelled)
}

pub(crate) fn string(scope: &MemoryScope, value: &str) -> Result<FundedString, CanonicalError> {
    FundedString::try_from_str(scope, Family::Canonical, value).map_err(Into::into)
}
