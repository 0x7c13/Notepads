// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use ratex_types::{Color, execution::ExecutionError, storage::StorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathMode {
    Inline,
    Display,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InkBounds {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathMetrics {
    pub advance: f64,
    pub ascent: f64,
    pub descent: f64,
    pub ink: InkBounds,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MathPathCommand {
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
    QuadTo {
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    CubicTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Primitive {
    pub command_start: u32,
    pub command_len: u32,
    pub color: Color,
    pub fill: bool,
    pub stroke_width: f64,
}

pub struct MathArtifactData {
    pub metrics: MathMetrics,
    pub primitives: Vec<Primitive>,
    pub commands: Vec<MathPathCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    Execution(ExecutionError),
    Storage(StorageError),
    Funding(markdown_core::resources::derived::DerivedError),
    ObserverNotAllowed,
    MissingFont { font: u16 },
    MissingGlyph { font: u16, codepoint: u32 },
    InvalidFont { font: u16 },
    InvalidGeometry,
    Unsupported,
    StackUnavailable { required: u64, available: u64 },
    InvalidFunding,
    InvalidLimits,
    ParseFailure,
    InternalInvariant,
}
impl From<ExecutionError> for MathError {
    fn from(value: ExecutionError) -> Self {
        Self::Execution(value)
    }
}
impl From<StorageError> for MathError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}
impl From<markdown_core::resources::derived::DerivedError> for MathError {
    fn from(value: markdown_core::resources::derived::DerivedError) -> Self {
        Self::Funding(value)
    }
}
impl std::fmt::Display for MathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for MathError {}

const _: () = assert!(size_of::<MathPathCommand>() <= 64);
const _: () = assert!(size_of::<Primitive>() <= 256);
