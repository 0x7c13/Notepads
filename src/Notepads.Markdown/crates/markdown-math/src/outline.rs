// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One borrowed DisplayList becomes one owned numerical batch. No SVG or font
//! bytes escape this module. Filled paths use NonZero; strokes use Butt caps
//! and Bevel joins. The native consumer must preserve those fixed semantics.
//! Ink bounds conservatively contain the curve control-point hull and stroke.

use ratex_types::{
    Color, DisplayItem, DisplayList, PathCommand,
    execution::{ExecutionError, Phase, Resource},
    storage::{StorageFamily, StorageLedger},
};

use crate::{
    fonts::{FontAssets, FontLookupError, UNKNOWN_FONT},
    types::{InkBounds, MathArtifactData, MathError, MathMetrics, MathPathCommand, Primitive},
};

const MAX_ITEMS: usize = 2_048;
const MAX_COMMANDS: usize = 65_536;
const PATH_STROKE_WIDTH: f64 = 0.04;

pub(crate) fn outline_drawing(
    display: &DisplayList,
    fonts: &FontAssets,
    storage: &StorageLedger<'_>,
) -> Result<MathArtifactData, MathError> {
    outline_drawing_with_chinese_text(display, fonts, storage, &[])
}

pub(crate) fn outline_drawing_with_chinese_text(
    display: &DisplayList,
    fonts: &FontAssets,
    storage: &StorageLedger<'_>,
    chinese_text: &[Vec<PathCommand>],
) -> Result<MathArtifactData, MathError> {
    storage.control().checkpoint(Phase::Outline)?;
    if ![display.width, display.height, display.depth]
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
        || !(display.height + display.depth).is_finite()
    {
        return Err(MathError::InvalidGeometry);
    }
    if display.items.len() > MAX_ITEMS {
        return Err(resource_limit(
            storage,
            Resource::DisplayItems,
            MAX_ITEMS,
            display.items.len(),
        ));
    }
    let mut output = OutputBuilder {
        storage,
        primitives: Vec::new(),
        commands: Vec::new(),
        ink: None,
    };
    for item in &display.items {
        storage.control().checkpoint(Phase::Outline)?;
        match item {
            DisplayItem::GlyphPath {
                x,
                y,
                scale,
                font,
                char_code,
                color,
            } => {
                validate_color(*color)?;
                if ![x, y, scale].iter().all(|value| value.is_finite()) || *scale <= 0.0 {
                    return Err(MathError::InvalidGeometry);
                }
                if let Some(index) =
                    ratex_layout::layout_options::chinese_text_run_index(font, *char_code)
                {
                    let commands = chinese_text.get(index).ok_or(MathError::InvalidGeometry)?;
                    output.path(commands, *x, *y, *scale, *color, true)?;
                    continue;
                }
                // Face parsing and a complete outline_glyph call are opaque,
                // allocation-free borrowed scans. Checkpoints bracket them;
                // a callback error stops appends, not the underlying scan.
                storage.control().checkpoint(Phase::Outline)?;
                let face = fonts.face(font).map_err(font_error)?;
                storage.control().checkpoint(Phase::Outline)?;
                let glyph = face.glyph(*char_code).map_err(font_error)?;
                let factor = scale / f64::from(face.face().units_per_em());
                if !factor.is_finite() || factor <= 0.0 {
                    return Err(MathError::InvalidGeometry);
                }
                let start = output.commands.len();
                let mut state = PathState::default();
                let mut callback = GlyphOutline {
                    output: &mut output,
                    state: &mut state,
                    origin_x: *x,
                    origin_y: *y,
                    factor,
                    failure: None,
                };
                let bounds = face.face().outline_glyph(glyph, &mut callback);
                if let Some(error) = callback.failure {
                    return Err(error);
                }
                storage.control().checkpoint(Phase::Outline)?;
                if bounds.is_none() {
                    let is_space = char::from_u32(*char_code).is_some_and(char::is_whitespace);
                    if !is_space || output.commands.len() != start {
                        return Err(MathError::MissingGlyph {
                            font: face.index(),
                            codepoint: *char_code,
                        });
                    }
                } else if output.commands.len() == start {
                    return Err(MathError::InvalidFont { font: face.index() });
                }
                output.finish(start, *color, true, 0.0, state.ink)?;
            }
            DisplayItem::Line {
                x,
                y,
                width,
                thickness,
                color,
                dashed,
            } => {
                validate_color(*color)?;
                if ![x, y, width, thickness]
                    .iter()
                    .all(|value| value.is_finite())
                    || *width < 0.0
                    || *thickness < 0.0
                {
                    return Err(MathError::InvalidGeometry);
                }
                if *width == 0.0 || *thickness == 0.0 {
                    continue;
                }
                let start = output.commands.len();
                let mut state = PathState::default();
                if *dashed {
                    let dash = thickness * 4.0;
                    let stride = dash * 2.0;
                    let count = (width / stride).ceil();
                    if !dash.is_finite() || !stride.is_finite() || stride <= 0.0 {
                        return Err(MathError::InvalidGeometry);
                    }
                    if !count.is_finite() || count > (MAX_COMMANDS / 2) as f64 {
                        return Err(resource_limit(
                            storage,
                            Resource::OutlineSegments,
                            MAX_COMMANDS,
                            usize::MAX,
                        ));
                    }
                    let count = count as usize;
                    let required = count
                        .checked_mul(2)
                        .and_then(|value| value.checked_add(start))
                        .ok_or(MathError::InvalidGeometry)?;
                    if required > MAX_COMMANDS {
                        return Err(resource_limit(
                            storage,
                            Resource::OutlineSegments,
                            MAX_COMMANDS,
                            required,
                        ));
                    }
                    for index in 0..count {
                        let offset = index as f64 * stride;
                        let end = (offset + dash).min(*width);
                        output.append(
                            MathPathCommand::MoveTo {
                                x: x + offset,
                                y: *y,
                            },
                            &mut state,
                        )?;
                        output.append(MathPathCommand::LineTo { x: x + end, y: *y }, &mut state)?;
                    }
                } else {
                    output.append(MathPathCommand::MoveTo { x: *x, y: *y }, &mut state)?;
                    output.append(
                        MathPathCommand::LineTo {
                            x: x + width,
                            y: *y,
                        },
                        &mut state,
                    )?;
                }
                output.finish(start, *color, false, *thickness, state.ink)?;
            }
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                color,
            } => {
                validate_color(*color)?;
                if ![x, y, width, height].iter().all(|value| value.is_finite())
                    || *width < 0.0
                    || *height < 0.0
                {
                    return Err(MathError::InvalidGeometry);
                }
                if *width == 0.0 || *height == 0.0 {
                    continue;
                }
                let start = output.commands.len();
                let mut state = PathState::default();
                output.append(MathPathCommand::MoveTo { x: *x, y: *y }, &mut state)?;
                output.append(
                    MathPathCommand::LineTo {
                        x: x + width,
                        y: *y,
                    },
                    &mut state,
                )?;
                output.append(
                    MathPathCommand::LineTo {
                        x: x + width,
                        y: y + height,
                    },
                    &mut state,
                )?;
                output.append(
                    MathPathCommand::LineTo {
                        x: *x,
                        y: y + height,
                    },
                    &mut state,
                )?;
                output.append(MathPathCommand::Close, &mut state)?;
                output.finish(start, *color, true, 0.0, state.ink)?;
            }
            DisplayItem::Path {
                x,
                y,
                commands,
                fill,
                color,
            } => {
                validate_color(*color)?;
                if !x.is_finite() || !y.is_finite() {
                    return Err(MathError::InvalidGeometry);
                }
                output.path(commands, *x, *y, 1.0, *color, *fill)?;
            }
        }
    }
    storage.control().checkpoint(Phase::Outline)?;
    Ok(MathArtifactData {
        metrics: MathMetrics {
            advance: display.width,
            ascent: display.height,
            descent: display.depth,
            ink: output.ink.unwrap_or_default(),
        },
        primitives: output.primitives,
        commands: output.commands,
    })
}

fn font_error(error: FontLookupError) -> MathError {
    match error {
        FontLookupError::UnknownFont => MathError::MissingFont { font: UNKNOWN_FONT },
        FontLookupError::InvalidFont { font } => MathError::InvalidFont { font },
        FontLookupError::InvalidCodepoint { font, codepoint }
        | FontLookupError::MissingGlyph { font, codepoint } => {
            MathError::MissingGlyph { font, codepoint }
        }
    }
}

fn validate_color(color: Color) -> Result<(), MathError> {
    if [color.r, color.g, color.b, color.a]
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        Ok(())
    } else {
        Err(MathError::InvalidGeometry)
    }
}

fn resource_limit(
    storage: &StorageLedger<'_>,
    resource: Resource,
    limit: usize,
    requested: usize,
) -> MathError {
    MathError::Execution(storage.control().fail(ExecutionError::ResourceLimit {
        phase: Phase::Outline,
        resource,
        limit: limit as u64,
        requested: requested as u64,
    }))
}

struct OutputBuilder<'s, 'c> {
    storage: &'s StorageLedger<'c>,
    primitives: Vec<Primitive>,
    commands: Vec<MathPathCommand>,
    ink: Option<InkBounds>,
}

impl OutputBuilder<'_, '_> {
    fn append(&mut self, command: MathPathCommand, state: &mut PathState) -> Result<(), MathError> {
        if self.commands.len() >= MAX_COMMANDS {
            return Err(resource_limit(
                self.storage,
                Resource::OutlineSegments,
                MAX_COMMANDS,
                self.commands.len() + 1,
            ));
        }
        let mut next = *state;
        next.include(command)?;
        self.storage
            .control()
            .consume(Phase::Outline, Resource::OutlineSegments, 1)?;
        self.storage.try_push(
            Phase::Outline,
            StorageFamily::Path,
            &mut self.commands,
            command,
        )?;
        *state = next;
        Ok(())
    }

    fn path(
        &mut self,
        commands: &[PathCommand],
        x: f64,
        y: f64,
        scale: f64,
        color: Color,
        fill: bool,
    ) -> Result<(), MathError> {
        let start = self.commands.len();
        let mut state = PathState::default();
        let point = |px: f64, py: f64| (x + px * scale, y + py * scale);
        for command in commands {
            let command = match *command {
                PathCommand::MoveTo { x, y } => {
                    let (x, y) = point(x, y);
                    MathPathCommand::MoveTo { x, y }
                }
                PathCommand::LineTo { x, y } => {
                    let (x, y) = point(x, y);
                    MathPathCommand::LineTo { x, y }
                }
                PathCommand::QuadTo { x1, y1, x, y } => {
                    let (x1, y1) = point(x1, y1);
                    let (x, y) = point(x, y);
                    MathPathCommand::QuadTo { x1, y1, x, y }
                }
                PathCommand::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    let (x1, y1) = point(x1, y1);
                    let (x2, y2) = point(x2, y2);
                    let (x, y) = point(x, y);
                    MathPathCommand::CubicTo {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    }
                }
                PathCommand::Close => MathPathCommand::Close,
            };
            self.append(command, &mut state)?;
        }
        self.finish(
            start,
            color,
            fill,
            if fill { 0.0 } else { PATH_STROKE_WIDTH },
            state.ink,
        )
    }

    fn finish(
        &mut self,
        start: usize,
        color: Color,
        fill: bool,
        stroke_width: f64,
        bounds: Option<InkBounds>,
    ) -> Result<(), MathError> {
        let length = self
            .commands
            .len()
            .checked_sub(start)
            .ok_or(MathError::InternalInvariant)?;
        if length == 0 {
            return Ok(());
        }
        let primitive = Primitive {
            command_start: u32::try_from(start).map_err(|_| MathError::InternalInvariant)?,
            command_len: u32::try_from(length).map_err(|_| MathError::InternalInvariant)?,
            color,
            fill,
            stroke_width,
        };
        self.storage.try_push(
            Phase::Outline,
            StorageFamily::Mirror,
            &mut self.primitives,
            primitive,
        )?;
        if let Some(mut bounds) = bounds {
            let margin = if fill { 0.0 } else { stroke_width / 2.0 };
            bounds.left -= margin;
            bounds.top -= margin;
            bounds.right += margin;
            bounds.bottom += margin;
            if ![bounds.left, bounds.top, bounds.right, bounds.bottom]
                .iter()
                .all(|value| value.is_finite())
            {
                return Err(MathError::InvalidGeometry);
            }
            union(&mut self.ink, bounds);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct PathState {
    current: Option<(f64, f64)>,
    start: Option<(f64, f64)>,
    ink: Option<InkBounds>,
}

impl PathState {
    fn point(&mut self, x: f64, y: f64) -> Result<(), MathError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(MathError::InvalidGeometry);
        }
        union(
            &mut self.ink,
            InkBounds {
                left: x,
                top: y,
                right: x,
                bottom: y,
            },
        );
        Ok(())
    }

    fn include(&mut self, command: MathPathCommand) -> Result<(), MathError> {
        match command {
            MathPathCommand::MoveTo { x, y } => {
                if !x.is_finite() || !y.is_finite() {
                    return Err(MathError::InvalidGeometry);
                }
                self.current = Some((x, y));
                self.start = self.current;
            }
            MathPathCommand::LineTo { x, y } => {
                let (px, py) = self.current.ok_or(MathError::InvalidGeometry)?;
                self.point(px, py)?;
                self.point(x, y)?;
                self.current = Some((x, y));
            }
            MathPathCommand::QuadTo { x1, y1, x, y } => {
                let (px, py) = self.current.ok_or(MathError::InvalidGeometry)?;
                self.point(px, py)?;
                self.point(x1, y1)?;
                self.point(x, y)?;
                self.current = Some((x, y));
            }
            MathPathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (px, py) = self.current.ok_or(MathError::InvalidGeometry)?;
                self.point(px, py)?;
                self.point(x1, y1)?;
                self.point(x2, y2)?;
                self.point(x, y)?;
                self.current = Some((x, y));
            }
            MathPathCommand::Close => {
                let (x, y) = self.start.ok_or(MathError::InvalidGeometry)?;
                if let Some((px, py)) = self.current {
                    self.point(px, py)?;
                    self.point(x, y)?;
                }
                self.current = Some((x, y));
            }
        }
        Ok(())
    }
}

fn union(destination: &mut Option<InkBounds>, bounds: InkBounds) {
    match destination {
        Some(value) => {
            value.left = value.left.min(bounds.left);
            value.top = value.top.min(bounds.top);
            value.right = value.right.max(bounds.right);
            value.bottom = value.bottom.max(bounds.bottom);
        }
        None => *destination = Some(bounds),
    }
}

struct GlyphOutline<'a, 's, 'c> {
    output: &'a mut OutputBuilder<'s, 'c>,
    state: &'a mut PathState,
    origin_x: f64,
    origin_y: f64,
    factor: f64,
    failure: Option<MathError>,
}

impl GlyphOutline<'_, '_, '_> {
    fn x(&self, value: f32) -> f64 {
        self.origin_x + f64::from(value) * self.factor
    }
    fn y(&self, value: f32) -> f64 {
        self.origin_y - f64::from(value) * self.factor
    }
    fn append(&mut self, command: MathPathCommand) {
        if self.failure.is_none()
            && let Err(error) = self.output.append(command, self.state)
        {
            self.failure = Some(error);
        }
    }
}

impl ttf_parser::OutlineBuilder for GlyphOutline<'_, '_, '_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.append(MathPathCommand::MoveTo {
            x: self.x(x),
            y: self.y(y),
        });
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.append(MathPathCommand::LineTo {
            x: self.x(x),
            y: self.y(y),
        });
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.append(MathPathCommand::QuadTo {
            x1: self.x(x1),
            y1: self.y(y1),
            x: self.x(x),
            y: self.y(y),
        });
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.append(MathPathCommand::CubicTo {
            x1: self.x(x1),
            y1: self.y(y1),
            x2: self.x(x2),
            y2: self.y(y2),
            x: self.x(x),
            y: self.y(y),
        });
    }
    fn close(&mut self) {
        self.append(MathPathCommand::Close);
    }
}
