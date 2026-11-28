// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private isolated font replay hook. SVG/CSS, positioning, scene conversion and
//! rasterization remain in this usvg/resvg fork. No Windows objects enter Rust.
use crate::{FillRule, FontVariation, Text};
use std::sync::Arc;

/// Actual instance global metrics, in original font units (y up descent signed).
#[derive(Clone, Copy, Debug)]
pub struct ReplayMetrics {
    /// Units per em.
    pub units_per_em: u32,
    /// Ascender.
    pub ascent: i32,
    /// Descender, negative below the baseline.
    pub descent: i32,
    /// X height.
    pub x_height: i32,
    /// Underline position.
    pub underline_position: i32,
    /// Underline thickness.
    pub underline_thickness: i32,
    /// Strike position.
    pub strike_position: i32,
}
/// A registered actual instance; normal instances keep raw OpenType metrics.
#[derive(Clone, Copy, Debug)]
pub struct ReplayFace {
    /// Exact manually registered fontdb identity, not a family query result.
    pub id: fontdb::ID,
    /// Native metrics only when actual simulations require the bridge.
    pub synthetic_metrics: Option<ReplayMetrics>,
}
/// Origin of the actual local path, independent of font family or cluster order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayGeometryOrigin {
    /// Ordinary font outline or normalized Native non-sideways outline, y up.
    FontDesignOriginYUp,
    /// Native sideways run pen, after the unchanged (-y,-x) normalization.
    NativeSidewaysRunPenNormalized,
}
/// Placement actually applied to the glyph's local path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayPlacementPolicy {
    /// The original horizontal flow or explicit SVG rotation.
    UnchangedHorizontal,
    /// Original upright placement using the engine metrics midpoint and pivot.
    EngineMetricsMidpointUpright,
    /// Original rotated placement using the engine metrics midpoint.
    EngineMetricsMidpointRotated,
    /// Upright Native run-pen geometry needs inverse-90 linear placement only.
    NativeSidewaysInverse90,
}
/// A real author length declaration. Its normalized subtree is reconstructed
/// from retained source membership, never inherited float equality.
#[derive(Clone, Copy, Debug)]
pub struct ReplayLengthDirective {
    /// Original declaring node identity.
    pub owner: usize,
    /// Original parent declaring node, if any.
    pub parent: Option<usize>,
    /// Root-first nesting depth.
    pub depth: usize,
    /// Resolved requested length in user units.
    pub target: f32,
    /// Actual source lengthAdjust value.
    pub adjust: crate::LengthAdjust,
}
/// A checked source parent, in whole semantic Text UTF8 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplaySourceCell {
    /// Inclusive source byte offset.
    pub start: usize,
    /// Exclusive source byte offset.
    pub end: usize,
}
/// Immutable proof owner; implemented by the private admitted Rust plan. It
/// contains no Adapter, font database or scene reference.
pub trait ReplayLogicalSource: std::fmt::Debug + Send + Sync {
    /// Exact source/HB closure cells, including cells with no output glyph.
    fn cells(&self) -> &[ReplaySourceCell];
    /// Charge real private Rust work against the same job.
    fn checkpoint(&self, work: u64) -> Result<(), ()>;
    /// Actual source bidi level, including source with no output glyph.
    fn source_bidi_level(&self, _offset: usize) -> Option<u8> {
        None
    }
}
/// Checked glyph membership. The raw HB start/end/advance/offset remain separate.
#[derive(Clone, Copy, Debug)]
pub struct ReplayGlyphIdentity {
    /// Source parent index in ReplayLogicalSource.
    pub parent: usize,
    /// Original font/shape part ordinal.
    pub part: usize,
    /// Consecutive original Native coverage parts retained by this HB run.
    pub part_count: usize,
    /// Original positioning chunk/domain ordinal.
    pub position_domain: usize,
    /// Semantic style ordinal within that original chunk.
    pub visual_span: usize,
    /// This glyph's actual source em before fallback scaling.
    pub semantic_em: f32,
    /// Same checked metadata used by Native and HarfRust.
    pub native_orientation: u32,
    /// Same source-derived script, ISO15924 big-endian tag.
    pub script_be: u32,
}
/// HarfRust result in font units; SVG transforms are deliberately absent.
#[derive(Clone, Debug)]
#[allow(
    missing_copy_implementations,
    reason = "Replay records retain explicit duplication at producer call sites."
)]
pub struct ReplayGlyph {
    /// None is used only by old isolated mechanism fixtures, never actual shaping.
    pub identity: Option<ReplayGlyphIdentity>,
    /// Logical UTF8 cluster start within the chunk.
    pub start: usize,
    /// Logical UTF8 cluster end within the chunk.
    pub end: usize,
    /// Raw actual glyph id.
    pub glyph: u32,
    /// Complete actual instance identity.
    pub face: ReplayFace,
    /// The local path origin for this glyph, not a cluster-wide assumption.
    pub geometry_origin: ReplayGeometryOrigin,
    /// Final HB GSUB/GPOS offset.
    pub x_offset: i32,
    /// Final HB GSUB/GPOS offset.
    pub y_offset: i32,
    /// Final HB advance; Native nominal callbacks do not replace this value.
    pub advance: i32,
    /// Unmodified HB y_advance; zero-ink suppression requires both axes zero.
    pub y_advance: i32,
    /// Actual fallback em scale, finite and positive.
    pub fallback_scale: f32,
}
/// Private replay storage. Controlled producers retain the exact paid owner;
/// compatibility fixtures preserve the upstream Vec representation explicitly.
#[derive(Debug)]
pub enum ReplayList<T> {
    /// Legacy source-only fixtures; never an admitted controlled allocation.
    Compat(Vec<T>),
    /// Same-request storage, admitted after its actual allocation succeeds.
    Paid(markdown_svg_budget::BudgetVec<T>),
}
impl<T> From<Vec<T>> for ReplayList<T> {
    fn from(value: Vec<T>) -> Self {
        Self::Compat(value)
    }
}
impl<T> From<markdown_svg_budget::BudgetVec<T>> for ReplayList<T> {
    fn from(value: markdown_svg_budget::BudgetVec<T>) -> Self {
        Self::Paid(value)
    }
}
impl<T> std::ops::Deref for ReplayList<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Compat(value) => value,
            Self::Paid(value) => value.as_slice(),
        }
    }
}
impl<'a, T> IntoIterator for &'a ReplayList<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
/// Prepared once for the entire semantic Text, before any layout or outline.
#[derive(Debug)]
pub struct ReplayText {
    /// The admitted checked source proof survives through actual layout/flatten.
    pub logical: Option<Arc<dyn ReplayLogicalSource>>,
    /// Shape results, one vector per unchanged usvg chunk.
    pub chunks: ReplayList<ReplayList<ReplayGlyph>>,
    /// Actual primary instance for each semantic style span, by chunk then span.
    pub styles: ReplayList<ReplayList<ReplayFace>>,
}
/// Ordinary raw outline/color path, or an explicitly owned synthetic outline.
#[derive(Clone, Debug)]
pub enum ReplayOutline {
    /// Existing controlled Rust ordinary/color renderer owns this glyph.
    Ordinary,
    /// A Native simulated outline; None is a real empty path, not permission to fallback.
    Synthetic {
        /// Owned typed local y-up path.
        data: Option<Arc<tiny_skia_path::Path>>,
        /// Actual Native fill rule.
        rule: FillRule,
    },
}
/// Borrowed fvar metadata. Checked observations cannot extract an ordinary
/// collection or iterator and therefore retain their request's typed failures.
pub enum ReplayAxes<'a> {
    /// Explicit compatibility input, without resource control.
    Ordinary(skrifa::AxisCollection<'a>),
    /// The same request/axes used to build the actual location.
    Request(&'a skrifa::request::Axes<'a, 'a>),
}
impl ReplayAxes<'_> {
    /// Number of original fvar records.
    pub fn len(&self) -> usize {
        match self {
            Self::Ordinary(axes) => axes.len(),
            Self::Request(axes) => axes.len(),
        }
    }
    /// Whether the original collection has no axes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Read one original scalar record without discarding a request error.
    pub fn try_get(
        &self,
        index: usize,
    ) -> Result<Option<skrifa::Axis>, markdown_svg_budget::Error> {
        match self {
            Self::Ordinary(axes) => Ok(axes.get(index)),
            Self::Request(axes) => axes.try_get(index),
        }
    }
}
impl std::fmt::Debug for ReplayAxes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayAxes")
            .field("len", &self.len())
            .finish()
    }
}

/// Borrowed actual Skrifa draw inputs/results, observed at the real draw site.
pub struct ReplayOutlineObservation<'a> {
    /// Exact registered font.
    pub font: fontdb::ID,
    /// Actual used glyph.
    pub glyph: u32,
    /// Actual parsed TTC index.
    pub face_index: u32,
    /// Actual head units per em at the consuming outline reader.
    pub units_per_em: u16,
    /// The argument used to normalize this draw.
    pub variations: &'a [FontVariation],
    /// Real location passed to DrawSettings, not a later recomputation.
    pub location: skrifa::instance::LocationRef<'a>,
    /// Actual parsed fvar order/min/default/max for the location above.
    pub axes: ReplayAxes<'a>,
    /// Actual y-up local path returned by this draw, before SVG transforms.
    pub path: Option<&'a tiny_skia_path::Path>,
}
/// The real unscaled Skrifa metrics call; separate from synthetic final overrides.
pub struct ReplayMetricsObservation<'a> {
    /// Actual font identity.
    pub font: fontdb::ID,
    /// Actual TTC index passed to the reader.
    pub face_index: u32,
    /// Actual variations used to create the location.
    pub variations: &'a [FontVariation],
    /// Location passed to the real metrics call.
    pub location: skrifa::instance::LocationRef<'a>,
    /// Same parsed font's fvar metadata in actual location order.
    pub axes: ReplayAxes<'a>,
    /// Real metrics returned by that call, before any Native synthetic override.
    pub metrics: skrifa::metrics::Metrics,
}
/// Final resolved font metrics genuinely returned to text layout.
#[derive(Clone, Copy, Debug)]
pub struct ReplayLayoutMetricsObservation {
    /// Actual font identity.
    pub font: fontdb::ID,
    /// Actual denominator.
    pub units_per_em: u16,
    /// Native synthetic metrics override was really consumed.
    pub synthetic: bool,
    /// Actual final ascent/descent/xheight/underline position/thickness/strike.
    pub values: [i32; 6],
}
/// Real final layout values and path consumed by flatten, all borrowed in Rust.
#[derive(Clone, Copy, Debug)]
pub struct ReplayGeometryObservation<'a> {
    /// Stable actual fontdb identity.
    pub font: fontdb::ID,
    /// Actual used glyph id.
    pub glyph: u32,
    /// Real source Text id, optional empty.
    pub element_id: &'a str,
    /// Source cluster text carried by PositionedGlyph.
    pub cluster_text: &'a str,
    /// 0 ordinary outline, 1 Native synthetic, 2 COLR, 3 SVG, 4 bitmap.
    pub representation: u32,
    /// Actual layout font-unit denominator.
    pub units_per_em: u16,
    /// Actual final font size, including the fallback scale.
    pub font_size: f32,
    /// Origin carried from the real shaping/geometry instance to consumption.
    pub geometry_origin: ReplayGeometryOrigin,
    /// Actual per-glyph placement policy used to build the consumed matrix.
    pub placement_policy: ReplayPlacementPolicy,
    /// Engine heuristic actually consumed, in DIP; never a font BASE witness.
    pub engine_metrics_midpoint: Option<f32>,
    /// Real layout matrices, not formulas reconstructed by the adapter.
    pub glyph_transform: tiny_skia_path::Transform,
    /// Real cluster matrix.
    pub cluster_transform: tiny_skia_path::Transform,
    /// Real span matrix.
    pub span_transform: tiny_skia_path::Transform,
    /// Transform actually about to consume the representation.
    pub representation_transform: tiny_skia_path::Transform,
    /// Real Text ancestor scene transform.
    pub scene_transform: tiny_skia_path::Transform,
    /// Local path consumed by outline flatten. Color representations use None;
    /// this alone is explicitly not a color-leaf geometry witness.
    pub path: Option<&'a tiny_skia_path::Path>,
    /// Actual selected fill rule.
    pub fill_rule: FillRule,
    /// Actual span fill opacity, before ancestor compositing.
    pub fill_opacity: Option<f32>,
    /// Actual span visibility.
    pub visible: bool,
}
/// Same-request paid storage guard. It owns only ledger/context state and must
/// never retain an Adapter, Tree, representation or borrowed buffer.
pub trait ReplayRetention: std::fmt::Debug + Send + Sync {}

/// Request-owned adapter. All methods run on the same synchronous worker. A Tree
/// retains this owner until its paths/fontdb have physically dropped. Its Native
/// session ticket resolves only through that worker's registry.
pub trait FontReplay: std::fmt::Debug + Send + Sync {
    /// Original Native font session ticket for actual outline-member capture.
    /// Zero keeps explicit compatibility providers outside this capability.
    /// A nonzero value never substitutes for same-request resource identity.
    fn coverage_epoch(&self) -> u64 {
        0
    }
    /// Synchronously borrow one actual flatten Path and its captured members.
    /// Do not retain either input. Later paint-server processing can alter its
    /// fill/stroke; this is not a final renderer-state observation or Native call.
    fn observe_glyph_fill_source(
        &self,
        _source: &crate::GlyphFillSource,
        _path: &crate::Path,
    ) -> Result<(), ()> {
        Ok(())
    }
    /// The existing request resources, without a second account or reset limits.
    /// None is reserved for explicit legacy compatibility providers.
    fn request_resources(&self) -> Option<markdown_svg_budget::ResourcesRef> {
        None
    }
    /// Latch a declared capability limit through the real request first-error
    /// path; the reason itself provides no source or layout proof.
    fn unsupported(&self, _reason: &'static str) -> Result<(), ()> {
        Err(())
    }
    /// Reserve same-request storage. Known capacities reserve before allocation;
    /// post-codec retention accounting alone does not qualify earlier allocations
    /// or total peak memory. The request also retains the guard through scene drop.
    fn reserve_replay_bytes(&self, _bytes: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
        Err(())
    }
    /// One same-cascade whole Text logical plan, real HB shape, used geometry batches.
    fn prepare(&self, text: &Text, fontdb: &mut Arc<fontdb::Database>) -> Result<ReplayText, ()>;
    /// Latest additive request-owned database, shared by nested glyph SVG.
    fn fontdb(&self) -> Result<Arc<fontdb::Database>, ()>;
    /// Exact actual face axes; memory-only lookup, no cross-language glyph call.
    fn variations(&self, font: fontdb::ID) -> Result<Arc<[FontVariation]>, ()>;
    /// Memory-only typed outline lookup after prepare's used-gid batch has completed.
    fn outline(&self, font: fontdb::ID, glyph: u32) -> Result<ReplayOutline, ()>;
    /// Memory-only observer; no cross-language per-glyph calls or retained borrow.
    fn observe_outline(&self, _value: ReplayOutlineObservation<'_>) -> Result<(), ()> {
        Ok(())
    }
    /// Real metrics call witness. No Native glyph callback is made here.
    fn observe_metrics(&self, _value: ReplayMetricsObservation<'_>) -> Result<(), ()> {
        Ok(())
    }
    /// Final ordinary/synthetic metrics handed to layout.
    fn observe_layout_metrics(&self, _value: ReplayLayoutMetricsObservation) -> Result<(), ()> {
        Ok(())
    }
    /// Memory-only observer of values genuinely consumed by flatten.
    fn observe_geometry(&self, _value: ReplayGeometryObservation<'_>) -> Result<(), ()> {
        Ok(())
    }
}
/// Match the existing SVG vertical-orientation policy without confusing an SVG
/// rotate transform with Native sideways metrics/geometry.
pub fn replay_orientation(mode: crate::WritingMode, ch: char) -> u32 {
    u32::from(
        mode == crate::WritingMode::TopToBottom
            && unicode_vo::char_orientation(ch) == unicode_vo::Orientation::Upright,
    )
}

/// Read the existing fixed VO table. The second value identifies Tu/Tr fallback;
/// it does not claim that vertical GSUB substitutions have been applied.
pub fn replay_source_vertical_orientation(ch: char) -> (bool, u32) {
    match unicode_vo::char_orientation(ch) {
        unicode_vo::Orientation::Upright => (true, 0),
        unicode_vo::Orientation::Rotated => (false, 0),
        unicode_vo::Orientation::TransformedOrUpright => (true, 1),
        unicode_vo::Orientation::TransformedOrRotated => (false, 2),
    }
}

/// Full effective visual equivalence at a raw indivisible HB component. Resource
/// paints compare their existing Arc identity; equal author node IDs or colors
/// alone are not sufficient. Directive declaration ownership is kept separately.
pub fn replay_visual_equivalent(a: &crate::TextSpan, b: &crate::TextSpan) -> bool {
    let mut control = markdown_svg_budget::NoControl::<core::convert::Infallible>::new();
    match replay_visual_equivalent_with(a, b, &mut control) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

/// Compare effective source styles while controlling author-length work.
pub fn try_replay_visual_equivalent(
    a: &crate::TextSpan,
    b: &crate::TextSpan,
    resources: &markdown_svg_budget::ResourcesRef,
) -> Result<bool, markdown_svg_budget::Error> {
    replay_visual_equivalent_with(a, b, &mut crate::geometry::RequestControl(resources))
}

fn replay_visual_equivalent_with<C: markdown_svg_budget::WorkControl>(
    a: &crate::TextSpan,
    b: &crate::TextSpan,
    control: &mut C,
) -> Result<bool, C::Error> {
    control.checkpoint(0)?;
    let value = replay_visual_equivalent_core(a, b, control)?;
    control.checkpoint(0)?;
    Ok(value)
}

fn replay_visual_equivalent_core<C: markdown_svg_budget::WorkControl>(
    a: &crate::TextSpan,
    b: &crate::TextSpan,
    control: &mut C,
) -> Result<bool, C::Error> {
    control.checkpoint(1)?;
    let fill = match (&a.fill, &b.fill) {
        (None, None) => true,
        (Some(a), Some(b)) => a.paint == b.paint && a.opacity == b.opacity && a.rule == b.rule,
        _ => false,
    };
    let stroke = match (&a.stroke, &b.stroke) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.paint == b.paint
                && a.opacity == b.opacity
                && a.width == b.width
                && match (a.dasharray.as_deref(), b.dasharray.as_deref()) {
                    (None, None) => true,
                    (Some(a), Some(b)) => {
                        visual_sequence_equal(a, b, control, |a, b, _| Ok(a == b))?
                    }
                    _ => false,
                }
                && a.dashoffset == b.dashoffset
                && a.miterlimit.get() == b.miterlimit.get()
                && a.linecap == b.linecap
                && a.linejoin == b.linejoin
        }
        _ => false,
    };
    Ok(fill
        && stroke
        && a.computed_color == b.computed_color
        && a.paint_order == b.paint_order
        && a.visible == b.visible
        && a.font_size == b.font_size
        && visual_font_equal(&a.font, &b.font, control)?
        && a.dominant_baseline == b.dominant_baseline
        && a.alignment_baseline == b.alignment_baseline
        && visual_sequence_equal(&a.baseline_shift, &b.baseline_shift, control, |a, b, _| {
            Ok(a == b)
        })?)
}

fn visual_sequence_equal<T, C: markdown_svg_budget::WorkControl>(
    a: &[T],
    b: &[T],
    control: &mut C,
    mut equal: impl FnMut(&T, &T, &mut C) -> Result<bool, C::Error>,
) -> Result<bool, C::Error> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(b) {
        control.checkpoint(1)?;
        if !equal(a, b, control)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn visual_name_equal<C: markdown_svg_budget::WorkControl>(
    a: &str,
    b: &str,
    control: &mut C,
) -> Result<bool, C::Error> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.as_bytes().chunks(256).zip(b.as_bytes().chunks(256)) {
        control.checkpoint(a.len() as u64)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}

fn visual_font_equal<C: markdown_svg_budget::WorkControl>(
    a: &crate::Font,
    b: &crate::Font,
    control: &mut C,
) -> Result<bool, C::Error> {
    Ok(visual_sequence_equal(
        a.families(),
        b.families(),
        control,
        |a, b, control| match (a, b) {
            (crate::FontFamily::Named(a), crate::FontFamily::Named(b)) => {
                visual_name_equal(a, b, control)
            }
            _ => Ok(a == b),
        },
    )? && a.style() == b.style()
        && a.stretch() == b.stretch()
        && a.weight() == b.weight()
        && visual_sequence_equal(
            a.variations(),
            b.variations(),
            control,
            |a, b, _| Ok(a == b),
        )?)
}

impl std::fmt::Debug for ReplayOutlineObservation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayOutlineObservation")
            .field("font", &self.font)
            .field("glyph", &self.glyph)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for ReplayMetricsObservation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayMetricsObservation")
            .field("font", &self.font)
            .field("face_index", &self.face_index)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "visual_control_tests.rs"]
mod visual_control_tests;
