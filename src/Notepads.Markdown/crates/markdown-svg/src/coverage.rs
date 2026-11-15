// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Same-worker, coarse Native geometry-coverage bridge. No borrowed pointer is
//! retained after the synchronous callback; Native state owns no Rust owner.

use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};
pub use resvg::CoverageProvider;
use resvg::{CoverageBatchResult, CoverageRequest};
use tiny_skia_path::PathSegment;

pub const VERSION: u32 = 1;
pub const MAX_PATHS: usize = 32;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Command {
    pub verb: u32,
    pub reserved: u32,
    pub xy_bits: [u32; 6],
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Path {
    pub command_start: u32,
    pub command_count: u32,
    pub fill_rule: u32,
    pub reserved: u32,
    pub matrix_bits: [u32; 6],
    pub s_bits: u32,
    pub t_bits: u32,
    pub target_x: i32,
    pub target_y: i32,
    pub width: u32,
    pub height: u32,
    pub a8_offset: u64,
    pub stride: u64,
    pub length: u64,
}
#[repr(C, align(8))]
pub struct Batch {
    pub size: u32,
    pub version: u32,
    pub path_count: u32,
    pub command_count: u32,
    pub paths: *const Path,
    #[cfg(target_pointer_width = "32")]
    pub paths_padding: u32,
    pub commands: *const Command,
    #[cfg(target_pointer_width = "32")]
    pub commands_padding: u32,
    pub a8: *mut u8,
    #[cfg(target_pointer_width = "32")]
    pub a8_padding: u32,
    pub a8_capacity: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Output {
    pub size: u32,
    pub version: u32,
    pub ready_mask: u32,
    pub ineligible_mask: u32,
    pub reasons: [u32; MAX_PATHS],
    pub completed: u32,
    pub reserved: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct Callbacks {
    pub size: u32,
    pub version: u32,
    pub session: u64,
    pub cover_batch: Option<unsafe extern "C" fn(u64, *const Batch, *mut Output) -> i32>,
    #[cfg(target_pointer_width = "32")]
    pub callback_padding: u32,
}

const fn invalid() -> Error {
    Error::new(FailureKind::Invariant, LimitKind::None)
}

impl Callbacks {
    pub fn validate(self, font_session: u64) -> Result<Self, Error> {
        if self.size != size_of::<Self>() as u32
            || self.version != VERSION
            || self.session == 0
            || self.session != font_session
            || self.cover_batch.is_none()
        {
            return Err(invalid());
        }
        #[cfg(target_pointer_width = "32")]
        if self.callback_padding != 0 {
            return Err(invalid());
        }
        Ok(self)
    }
}

/// Immutable table stored in the original account Host. It contains no Arc,
/// resource owner, callback context pointer, or foreign generation registry.
pub struct NativeCoverageProvider {
    callbacks: Callbacks,
}
impl NativeCoverageProvider {
    pub fn new(callbacks: Callbacks, font_session: u64) -> Result<Self, Error> {
        Ok(Self {
            callbacks: callbacks.validate(font_session)?,
        })
    }
}

fn output_mask(count: usize) -> u32 {
    if count == MAX_PATHS {
        u32::MAX
    } else {
        (1u32 << count) - 1
    }
}

impl CoverageProvider for NativeCoverageProvider {
    fn epoch(&self) -> u64 {
        self.callbacks.session
    }

    fn cover_batch(
        &self,
        requests: &[CoverageRequest<'_>],
        arena: &mut [u8],
        resources: &ResourcesRef,
    ) -> Result<CoverageBatchResult, Error> {
        resources.checkpoint(0)?;
        if requests.len() > MAX_PATHS {
            return Err(resources.record(invalid()));
        }
        // Keep both paid containers outside the fallible scope. Every original
        // error is latched before their Drop can produce a refund failure.
        let mut paths = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        let mut commands = BudgetVec::new(resources.clone(), ResourceKind::Geometry);
        let outcome = (|| {
            resources.checkpoint(requests.len() as u64)?;
            let command_count = requests.iter().try_fold(0usize, |sum, request| {
                sum.checked_add(request.path.len()).ok_or_else(invalid)
            })?;
            let wire_count = u32::try_from(command_count).map_err(|_| invalid())?;
            paths.try_reserve_exact(requests.len())?;
            commands.try_reserve_exact(command_count)?;
            let mut paid_commands = 0usize;
            for request in requests {
                validate_request(request, requests.first(), arena.len())?;
                let start = commands.len();
                let count = request.path.len();
                let mut segments = request.path.segments();
                for index in 0..count {
                    if paid_commands == 0 {
                        paid_commands = (command_count - commands.len()).min(256);
                        resources.checkpoint(paid_commands as u64)?;
                    }
                    // Payment precedes next(), which reads original points.
                    let segment = segments.next().ok_or_else(invalid)?;
                    paid_commands -= 1;
                    commands.try_push(encode(segment))?;
                    if index + 1 == count && segments.next().is_some() {
                        return Err(invalid());
                    }
                }
                paths.try_push(Path {
                    command_start: u32::try_from(start).map_err(|_| invalid())?,
                    command_count: u32::try_from(count).map_err(|_| invalid())?,
                    fill_rule: u32::from(request.fill_rule == tiny_skia::FillRule::EvenOdd),
                    reserved: 0,
                    matrix_bits: request.matrix_bits,
                    s_bits: request.s_bits,
                    t_bits: request.t_bits,
                    target_x: request.target_x,
                    target_y: request.target_y,
                    width: request.width,
                    height: request.height,
                    a8_offset: request.a8_offset as u64,
                    stride: request.stride as u64,
                    length: request.length as u64,
                })?;
            }
            validate_output_spans(requests)?;
            let input = Batch {
                size: size_of::<Batch>() as u32,
                version: VERSION,
                path_count: paths.len() as u32,
                command_count: wire_count,
                paths: if paths.is_empty() {
                    core::ptr::null()
                } else {
                    paths.as_ptr()
                },
                #[cfg(target_pointer_width = "32")]
                paths_padding: 0,
                commands: if commands.is_empty() {
                    core::ptr::null()
                } else {
                    commands.as_ptr()
                },
                #[cfg(target_pointer_width = "32")]
                commands_padding: 0,
                a8: if arena.is_empty() {
                    core::ptr::null_mut()
                } else {
                    arena.as_mut_ptr()
                },
                #[cfg(target_pointer_width = "32")]
                a8_padding: 0,
                a8_capacity: arena.len() as u64,
            };
            let mut output = Output {
                size: size_of::<Output>() as u32,
                version: VERSION,
                ..Output::default()
            };
            resources.checkpoint(0)?;
            // SAFETY: the validated same-worker table is pinned by the original
            // Host; all owned wire buffers, arena and output outlive this call.
            // The private callback contract forbids retaining those borrows.
            let hr = unsafe {
                (self.callbacks.cover_batch.unwrap())(self.callbacks.session, &input, &mut output)
            };
            if hr != 0 {
                // Native and this ResourcesRef share the exact first-fault slot.
                // Preserve an existing typed fault; otherwise latch this work
                // error before a fresh cancellation or buffer refund can win.
                return Err(Error::native(if hr < 0 {
                    hr
                } else {
                    0x8000ffffu32 as i32
                }));
            }
            resources.checkpoint(0)?;
            let mask = output_mask(requests.len());
            if output.size != size_of::<Output>() as u32
                || output.version != VERSION
                || output.reserved != 0
                || output.completed != 1
                || output.ready_mask & output.ineligible_mask != 0
                || output.ready_mask | output.ineligible_mask != mask
            {
                return Err(invalid());
            }
            for (index, reason) in output.reasons.iter().enumerate() {
                let ineligible =
                    index < requests.len() && output.ineligible_mask & (1u32 << index) != 0;
                if (ineligible && !(1..=3).contains(reason)) || (!ineligible && *reason != 0) {
                    return Err(invalid());
                }
            }
            Ok(CoverageBatchResult {
                ready_mask: output.ready_mask,
                ineligible_mask: output.ineligible_mask,
                reasons: output.reasons,
                completed: true,
            })
        })();
        outcome.map_err(|error| resources.record(error))
    }
}

fn validate_request(
    request: &CoverageRequest<'_>,
    first: Option<&CoverageRequest<'_>>,
    capacity: usize,
) -> Result<(), Error> {
    let s = f32::from_bits(request.s_bits);
    let t = f32::from_bits(request.t_bits);
    if !s.is_finite()
        || !t.is_finite()
        || t <= 0.0
        || s < t
        || request.target_x != 0
        || request.target_y != 0
        || request
            .matrix_bits
            .iter()
            .any(|bits| !f32::from_bits(*bits).is_finite())
        || first
            .is_some_and(|first| first.s_bits != request.s_bits || first.t_bits != request.t_bits)
    {
        return Err(invalid());
    }
    let needed = if request.width != 0 && request.height != 0 {
        if request.stride < request.width as usize {
            return Err(invalid());
        }
        (request.height as usize - 1)
            .checked_mul(request.stride)
            .and_then(|value| value.checked_add(request.width as usize))
            .ok_or_else(invalid)?
    } else {
        0
    };
    if needed > request.length
        || request
            .a8_offset
            .checked_add(request.length)
            .is_none_or(|end| end > capacity)
    {
        return Err(invalid());
    }
    Ok(())
}

fn validate_output_spans(requests: &[CoverageRequest<'_>]) -> Result<(), Error> {
    for (index, request) in requests.iter().enumerate() {
        let end = request
            .a8_offset
            .checked_add(request.length)
            .ok_or_else(invalid)?;
        for previous in &requests[..index] {
            let previous_end = previous
                .a8_offset
                .checked_add(previous.length)
                .ok_or_else(invalid)?;
            if request.length != 0
                && previous.length != 0
                && request.a8_offset < previous_end
                && previous.a8_offset < end
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

fn encode(segment: PathSegment) -> Command {
    let mut result = Command::default();
    let point = |x: f32, y: f32| [x.to_bits(), y.to_bits()];
    match segment {
        PathSegment::MoveTo(p) => {
            result.verb = 0;
            result.xy_bits[..2].copy_from_slice(&point(p.x, p.y));
        }
        PathSegment::LineTo(p) => {
            result.verb = 1;
            result.xy_bits[..2].copy_from_slice(&point(p.x, p.y));
        }
        PathSegment::QuadTo(a, b) => {
            result.verb = 2;
            result.xy_bits[..2].copy_from_slice(&point(a.x, a.y));
            result.xy_bits[2..4].copy_from_slice(&point(b.x, b.y));
        }
        PathSegment::CubicTo(a, b, c) => {
            result.verb = 3;
            result.xy_bits[..2].copy_from_slice(&point(a.x, a.y));
            result.xy_bits[2..4].copy_from_slice(&point(b.x, b.y));
            result.xy_bits[4..].copy_from_slice(&point(c.x, c.y));
        }
        PathSegment::Close => result.verb = 4,
    }
    result
}

const _: () = {
    assert!(size_of::<Command>() == 32 && align_of::<Command>() == 8);
    assert!(size_of::<Path>() == 88 && align_of::<Path>() == 8);
    assert!(size_of::<Batch>() == 48 && align_of::<Batch>() == 8);
    assert!(size_of::<Output>() == 152 && align_of::<Output>() == 8);
    assert!(size_of::<Callbacks>() == 24 && align_of::<Callbacks>() == 8);
    assert!(core::mem::offset_of!(Path, matrix_bits) == 16);
    assert!(core::mem::offset_of!(Path, s_bits) == 40);
    assert!(core::mem::offset_of!(Path, a8_offset) == 64);
    assert!(core::mem::offset_of!(Batch, paths) == 16);
    assert!(core::mem::offset_of!(Batch, commands) == 24);
    assert!(core::mem::offset_of!(Batch, a8) == 32);
    assert!(core::mem::offset_of!(Batch, a8_capacity) == 40);
    assert!(core::mem::offset_of!(Output, completed) == 144);
    assert!(core::mem::offset_of!(Callbacks, cover_batch) == 16);
};
