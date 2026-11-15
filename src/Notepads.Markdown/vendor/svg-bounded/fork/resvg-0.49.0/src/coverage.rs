// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Synchronous, request-borrowed whole-fill coverage. No Native owner is retained.

use std::cell::Cell;

use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};

use crate::render::Context;

const MAX_PATHS: usize = 32;
const SOFT_A8_BYTES: usize = 8 * 1024 * 1024;

/// A valid geometry that the coverage representation cannot encode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CoverageIneligibleReason {
    /// The checked coordinate representation domain was exceeded.
    CoordinateDomain = 1,
    /// The declared final sample-space precision cannot be met.
    PrecisionDomain = 2,
    /// A valid path exceeds a checked font format representation limit.
    FontFormatDomain = 3,
}

/// An explicit route back to the original target-density rasterizer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverageRoute {
    /// A validated text source explicitly disabled shape antialiasing.
    NonAntialiased,
    /// The provider returned a checked representation-domain reason.
    RepresentationIneligible(CoverageIneligibleReason),
}

/// One borrowed whole fill, in its real current-surface coordinate system.
#[derive(Clone, Copy, Debug)]
pub struct CoverageRequest<'a> {
    /// Unmodified complete local path, including its original command order.
    pub path: &'a tiny_skia::Path,
    /// Actual fill rule at the renderer consumption point.
    pub fill_rule: tiny_skia::FillRule,
    /// Original M bits in sx, ky, kx, sy, tx, ty order; never pre-multiplied by q.
    pub matrix_bits: [u32; 6],
    /// Frozen upward-rounded coverage density.
    pub s_bits: u32,
    /// Frozen actual target density.
    pub t_bits: u32,
    /// Current surface origin, fixed to zero in this implementation.
    pub target_x: i32,
    /// Current surface origin, fixed to zero in this implementation.
    pub target_y: i32,
    /// Complete current-surface width.
    pub width: u32,
    /// Complete current-surface height.
    pub height: u32,
    /// Disjoint output offset in the borrowed batch arena.
    pub a8_offset: usize,
    /// Tight A8 row stride.
    pub stride: usize,
    /// Checked complete logical A8 length.
    pub length: usize,
}

/// Copy-only callback result. No mask is consumed unless the complete batch validates.
#[derive(Clone, Copy, Debug)]
pub struct CoverageBatchResult {
    /// Paths whose entire logical output was written.
    pub ready_mask: u32,
    /// Paths explicitly rejected for a permitted representation-domain reason.
    pub ineligible_mask: u32,
    /// Exactly one permitted nonzero reason per ineligible path; all other entries are zero.
    pub reasons: [u32; MAX_PATHS],
    /// True only after all callback work and cleanup completed successfully.
    pub completed: bool,
}

/// Same-request synchronous coverage service. It must not retain any borrowed argument.
pub trait CoverageProvider: Send + Sync {
    /// Original nonzero font-session epoch.
    fn epoch(&self) -> u64;

    /// Fill at most 32 disjoint tight A8 outputs using the original request account.
    /// Errors invalidate every output, including any pixels already written.
    fn cover_batch(
        &self,
        requests: &[CoverageRequest<'_>],
        arena: &mut [u8],
        resources: &ResourcesRef,
    ) -> Result<CoverageBatchResult, Error>;

    /// Optional Rust-only diagnostic observation; this is not a per-path Native call.
    fn observe_route(&self, _path: &usvg::Path, _route: CoverageRoute) {}
}

/// Root display metadata and a borrowed provider, shared by every recursive surface.
/// Density is frozen only when a validated, antialiased text fill actually occurs.
pub struct CoverageConfig<'a> {
    provider: &'a dyn CoverageProvider,
    display_width_dip: f64,
    viewport_width: f32,
    target_scale: f32,
    frozen_scale: Cell<Option<f32>>,
}

impl<'a> CoverageConfig<'a> {
    /// Store the caller's already validated metadata without preflighting unused text work.
    pub fn new(
        provider: &'a dyn CoverageProvider,
        display_width_dip: f64,
        viewport_width: f32,
        target_scale: f32,
    ) -> Self {
        Self {
            provider,
            display_width_dip,
            viewport_width,
            target_scale,
            frozen_scale: Cell::new(None),
        }
    }

    fn density(&self) -> Result<f32, Error> {
        if let Some(scale) = self.frozen_scale.get() {
            return Ok(scale);
        }
        if !self.display_width_dip.is_finite()
            || self.display_width_dip <= 0.0
            || !self.viewport_width.is_finite()
            || self.viewport_width <= 0.0
            || !self.target_scale.is_finite()
            || self.target_scale <= 0.0
        {
            return Err(invariant());
        }
        let required = (4.0 * self.display_width_dip / f64::from(self.viewport_width))
            .max(f64::from(self.target_scale));
        let mut scale = required as f32;
        if f64::from(scale) < required {
            scale = f32::from_bits(scale.to_bits().checked_add(1).ok_or_else(capacity)?);
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(capacity());
        }
        self.frozen_scale.set(Some(scale));
        Ok(scale)
    }
}

pub(crate) fn invariant() -> Error {
    Error::new(FailureKind::Invariant, LimitKind::None)
}

fn capacity() -> Error {
    Error::new(FailureKind::Quota, LimitKind::Capacity)
}

/// Unprepared is reserved for a single tail (including a recursive pattern fill).
/// Original records a completed route decision and must never ask the provider again.
#[derive(Clone, Copy)]
pub(crate) enum FillCoverage<'a> {
    Unprepared,
    Original,
    Ready(tiny_skia::MaskRef<'a>),
}

pub(crate) fn has_pattern(path: &usvg::Path) -> bool {
    path.fill()
        .is_some_and(|fill| matches!(fill.paint(), usvg::Paint::Pattern(_)))
        || path
            .stroke()
            .is_some_and(|stroke| matches!(stroke.paint(), usvg::Paint::Pattern(_)))
}

/// A direct nonrecursive sibling span. Even a single output above the soft
/// threshold is admitted normally against the original real resource limits.
pub(crate) fn direct_span(nodes: &[usvg::Node], width: u32, height: u32) -> usize {
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .unwrap_or(usize::MAX);
    let limit = if pixels == 0 {
        MAX_PATHS
    } else {
        (SOFT_A8_BYTES / pixels).clamp(1, MAX_PATHS)
    };
    nodes
        .iter()
        .take(limit)
        .take_while(|node| matches!(node, usvg::Node::Path(path) if !has_pattern(path)))
        .count()
}

pub(crate) struct PreparedBatch<'a> {
    // Distinct scene Paths can share the same Arc<Path> geometry. Route and
    // result identity belongs to the actual fill, never its shared data pointer.
    paths: [Option<&'a usvg::Path>; MAX_PATHS],
    requests: BudgetVec<CoverageRequest<'a>>,
    arena: BudgetVec<u8>,
    result: CoverageBatchResult,
}

impl<'a> PreparedBatch<'a> {
    pub(crate) fn prepare(
        paths: impl IntoIterator<Item = &'a usvg::Path>,
        ctx: &Context,
        transform: tiny_skia::Transform,
        width: u32,
        height: u32,
    ) -> Result<Option<Self>, Error> {
        let Some(config) = ctx.coverage else {
            return Ok(None);
        };
        // Do not request an account or freeze sampling for an ordinary shape.
        let mut batch: Option<Self> = None;
        // Keep every paid batch allocation outside the fallible scope. Latch
        // an unrecorded provider/contract error before RAII cleanup can report
        // a separate failed refund into the same authoritative first slot.
        let result = (|| -> Result<(), Error> {
            for path in paths {
                if !eligible(path, ctx, config)? {
                    continue;
                }
                let resources = ctx.processing.resources().ok_or_else(invariant)?;
                resources.checkpoint(1)?;
                let s = config.density()?;
                let matrix = [
                    transform.sx,
                    transform.ky,
                    transform.kx,
                    transform.sy,
                    transform.tx,
                    transform.ty,
                ];
                if matrix.iter().any(|value| !value.is_finite()) {
                    return Err(Error::new(FailureKind::Decode, LimitKind::None));
                }
                let length = (width as usize)
                    .checked_mul(height as usize)
                    .ok_or_else(capacity)?;
                let entry = batch.get_or_insert_with(|| Self {
                    paths: [None; MAX_PATHS],
                    requests: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
                    arena: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
                    result: CoverageBatchResult {
                        ready_mask: 0,
                        ineligible_mask: 0,
                        reasons: [0; MAX_PATHS],
                        completed: false,
                    },
                });
                if entry.requests.len() == MAX_PATHS {
                    return Err(invariant());
                }
                entry.paths[entry.requests.len()] = Some(path);
                let offset = entry
                    .requests
                    .len()
                    .checked_mul(length)
                    .ok_or_else(capacity)?;
                entry.requests.try_push(CoverageRequest {
                    path: path.data(),
                    fill_rule: match path.fill().ok_or_else(invariant)?.rule() {
                        usvg::FillRule::NonZero => tiny_skia::FillRule::Winding,
                        usvg::FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
                    },
                    matrix_bits: matrix.map(f32::to_bits),
                    s_bits: s.to_bits(),
                    t_bits: config.target_scale.to_bits(),
                    target_x: 0,
                    target_y: 0,
                    width,
                    height,
                    a8_offset: offset,
                    stride: width as usize,
                    length,
                })?;
            }
            let Some(batch) = batch.as_mut() else {
                return Ok(());
            };
            let resources = ctx.processing.resources().ok_or_else(invariant)?;
            let last = batch.requests.last().ok_or_else(invariant)?;
            let length = last
                .a8_offset
                .checked_add(last.length)
                .ok_or_else(capacity)?;
            batch.arena.try_reserve_exact(length)?;
            const ZERO: [u8; 4096] = [0; 4096];
            while batch.arena.len() < length {
                let count = (length - batch.arena.len()).min(ZERO.len());
                resources.checkpoint(count as u64)?;
                batch.arena.try_extend_from_slice(&ZERO[..count])?;
            }
            resources.checkpoint(0)?;
            batch.result =
                config
                    .provider
                    .cover_batch(&batch.requests, &mut batch.arena, resources)?;
            resources.checkpoint(0)?;
            batch.validate()?;
            // Route observations happen only after the entire callback result validates.
            // The concrete Path is supplied at consumption, preserving original paint order.
            Ok(())
        })();
        result.map_err(|error| ctx.processing.record_resource(error))?;
        Ok(batch)
    }

    fn validate(&self) -> Result<(), Error> {
        let count = self.requests.len();
        let mask = if count == MAX_PATHS {
            u32::MAX
        } else {
            (1u32 << count) - 1
        };
        if !self.result.completed
            || self.result.ready_mask & self.result.ineligible_mask != 0
            || self.result.ready_mask | self.result.ineligible_mask != mask
        {
            return Err(invariant());
        }
        for (index, &reason) in self.result.reasons.iter().enumerate() {
            let ineligible = index < count && self.result.ineligible_mask & (1u32 << index) != 0;
            if if ineligible {
                decode_reason(reason).is_none()
            } else {
                reason != 0
            } {
                return Err(invariant());
            }
        }
        Ok(())
    }

    pub(crate) fn for_path(&self, path: &usvg::Path, ctx: &Context) -> FillCoverage<'_> {
        let Some(index) = self.paths[..self.requests.len()]
            .iter()
            .position(|candidate| candidate.is_some_and(|candidate| std::ptr::eq(candidate, path)))
        else {
            return FillCoverage::Original;
        };
        let request = &self.requests[index];
        if self.result.ineligible_mask & (1u32 << index) != 0 {
            if let Some(config) = ctx.coverage {
                config.provider.observe_route(
                    path,
                    CoverageRoute::RepresentationIneligible(
                        decode_reason(self.result.reasons[index]).expect("validated batch reason"),
                    ),
                );
            }
            return FillCoverage::Original;
        }
        let data = &self.arena[request.a8_offset..request.a8_offset + request.length];
        FillCoverage::Ready(
            tiny_skia::MaskRef::from_bytes(data, request.width, request.height)
                .expect("checked complete tight surface"),
        )
    }
}

fn decode_reason(value: u32) -> Option<CoverageIneligibleReason> {
    match value {
        1 => Some(CoverageIneligibleReason::CoordinateDomain),
        2 => Some(CoverageIneligibleReason::PrecisionDomain),
        3 => Some(CoverageIneligibleReason::FontFormatDomain),
        _ => None,
    }
}

#[cfg(feature = "text")]
fn eligible(path: &usvg::Path, ctx: &Context, config: &CoverageConfig<'_>) -> Result<bool, Error> {
    if !path.is_visible() || path.fill().is_none() {
        return Ok(false);
    }
    let Some(source) = path.glyph_fill_source() else {
        return Ok(false);
    };
    let resources = ctx.processing.resources().ok_or_else(invariant)?;
    // Validate before AA routing, bounds shortcuts, or recursive paint preparation.
    if !source.belongs_to(resources)
        || source.epoch() == 0
        || source.epoch() != config.provider.epoch()
    {
        return Err(invariant());
    }
    if !path.rendering_mode().use_shape_antialiasing() {
        config
            .provider
            .observe_route(path, CoverageRoute::NonAntialiased);
        return Ok(false);
    }
    Ok(path.data().bounds().width() != 0.0 && path.data().bounds().height() != 0.0)
}

#[cfg(not(feature = "text"))]
fn eligible(
    _path: &usvg::Path,
    _ctx: &Context,
    _config: &CoverageConfig<'_>,
) -> Result<bool, Error> {
    Ok(false)
}

/// Validate source identity before a recursive shader is built. A later preparation
/// still makes the actual AA route decision; no density or mask is allocated here.
pub(crate) fn validate_source(path: &usvg::Path, ctx: &Context) -> Result<(), Error> {
    #[cfg(feature = "text")]
    if let (Some(config), Some(source)) = (ctx.coverage, path.glyph_fill_source()) {
        let resources = ctx.processing.resources().ok_or_else(invariant)?;
        if !source.belongs_to(resources)
            || source.epoch() == 0
            || source.epoch() != config.provider.epoch()
        {
            return Err(invariant());
        }
    }
    #[cfg(not(feature = "text"))]
    let _ = (path, ctx);
    Ok(())
}

pub(crate) fn has_source(path: &usvg::Path, ctx: &Context) -> bool {
    #[cfg(feature = "text")]
    {
        ctx.coverage.is_some() && path.glyph_fill_source().is_some()
    }
    #[cfg(not(feature = "text"))]
    {
        let _ = (path, ctx);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_svg_budget::{Backend, FirstFailureRef, Limits, Ticket};
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Account(AtomicU32);
    impl Backend for Account {
        fn reserve(&self, _: ResourceKind, _: usize) -> Result<Ticket, Error> {
            Ticket::new(1, self.0.fetch_add(1, Ordering::Relaxed) + 1, 1)
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
            Ok(())
        }
        fn release_after_free(&self, _: Ticket) -> Result<(), Error> {
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), Error> {
            Ok(())
        }
    }

    #[test]
    fn distinct_scene_paths_sharing_path_data_keep_their_own_result_masks() {
        let tree = usvg::Tree::from_str(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0H1V1H0Z"/></svg>"#,
            &usvg::Options::default(),
        )
        .unwrap();
        let usvg::Node::Path(path) = &tree.root().children()[0] else {
            panic!("actual parsed path")
        };
        let first = path
            .as_ref()
            .try_clone_in(tree.processing_context())
            .unwrap();
        let second = first.try_clone_in(tree.processing_context()).unwrap();
        assert!(std::ptr::eq(first.data(), second.data()));
        assert!(!std::ptr::eq(&first, &second));
        let r = ResourcesRef::new_prepaid(
            Account(AtomicU32::new(0)),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        let ctx = Context {
            max_bbox: tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap(),
            processing: std::sync::Arc::new(usvg::ProcessingContext::with_resources(r.clone())),
            coverage: None,
        };
        let mut batch = PreparedBatch {
            paths: [None; MAX_PATHS],
            requests: BudgetVec::new(r.clone(), ResourceKind::Scratch),
            arena: BudgetVec::new(r, ResourceKind::Scratch),
            result: CoverageBatchResult {
                ready_mask: 3,
                ineligible_mask: 0,
                reasons: [0; MAX_PATHS],
                completed: true,
            },
        };
        for (index, path) in [&first, &second].iter().enumerate() {
            batch.paths[index] = Some(path);
            batch
                .requests
                .try_push(CoverageRequest {
                    path: path.data(),
                    fill_rule: tiny_skia::FillRule::Winding,
                    matrix_bits: [1f32.to_bits(), 0, 0, 1f32.to_bits(), 0, 0],
                    s_bits: 4f32.to_bits(),
                    t_bits: 1f32.to_bits(),
                    target_x: 0,
                    target_y: 0,
                    width: 1,
                    height: 1,
                    a8_offset: index,
                    stride: 1,
                    length: 1,
                })
                .unwrap();
            batch
                .arena
                .try_push(if index == 0 { 0 } else { 255 })
                .unwrap();
        }
        batch.validate().unwrap();
        let FillCoverage::Ready(first_mask) = batch.for_path(&first, &ctx) else {
            panic!("ready first")
        };
        let FillCoverage::Ready(second_mask) = batch.for_path(&second, &ctx) else {
            panic!("ready second")
        };
        assert_eq!(first_mask.data(), [0]);
        assert_eq!(second_mask.data(), [255]);
    }
}
