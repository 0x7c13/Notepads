//! Support for rendering variable composite glyphs from the VARC table.
// Modifications copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::control::storage::{Sequence, SmallBuffer};
use crate::control::{self, FontControl, KernelError, KernelResult, Ordinary};
use markdown_svg_budget::WorkControl;

use read_fonts::{
    tables::{
        layout::Condition,
        varc::{
            DecomposedTransform, MultiItemVariationStore, SparseVariationRegionList, Varc,
            VarcComponent, VarcFlags,
        },
        variations::NO_VARIATION_INDEX,
    },
    types::{F2Dot14, GlyphId, Matrix},
    FontRef, ReadError,
};

use crate::{
    instance::Size,
    outline::{cff, glyf, metrics::GlyphHMetrics, pen::PathStyle, DrawError, OutlinePen},
    GLYF_COMPOSITE_RECURSION_LIMIT,
};

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::CoreFloat;

use super::OutlineKind;

#[cfg(test)]
mod control_tests;

type GlyphStack<C = Ordinary> = <C as FontControl>::Small<GlyphId, 8>;
type CoordVec<C = Ordinary> = <C as FontControl>::Small<F2Dot14, 64>;
type AxisIndexVec<C = Ordinary> = <C as FontControl>::Small<u16, 64>;
type AxisValueVec<C = Ordinary> = <C as FontControl>::Small<f32, 64>;
type DeltaVec<C = Ordinary> = <C as FontControl>::Small<f32, 64>;
type ScalarCacheVec<C = Ordinary> = <C as FontControl>::Small<f32, 128>;
type Affine = Matrix<f32>;

struct Scratchpad<C: FontControl = Ordinary> {
    deltas: DeltaVec<C>,
    axis_indices: AxisIndexVec<C>,
    axis_values: AxisValueVec<C>,
}

#[cfg(test)]
impl Scratchpad {
    fn new() -> Self {
        control::ordinary(Self::try_new(&mut Ordinary::new()))
    }
}
impl<C: FontControl> Scratchpad<C> {
    fn try_new(cx: &mut C) -> Result<Self, C::Error> {
        Ok(Self {
            deltas: cx.small()?,
            axis_indices: cx.small()?,
            axis_values: cx.small()?,
        })
    }
}

struct VarcSharedContext<'a, 'b> {
    font_coords: &'b [F2Dot14],
    size: Size,
    path_style: PathStyle,
    coverage: &'b read_fonts::tables::layout::CoverageTable<'a>,
    var_store: Option<&'b MultiItemVariationStore<'a>>,
    store_regions: Option<(
        &'b MultiItemVariationStore<'a>,
        &'b SparseVariationRegionList<'a>,
    )>,
}

#[derive(Clone)]
enum BaseOutlines<'a> {
    Glyf(glyf::Outlines<'a>),
    Cff(cff::Outlines<'a>),
}

impl<'a> BaseOutlines<'a> {
    fn glyph_count(&self) -> u32 {
        match self {
            Self::Glyf(glyf) => glyf.glyph_count() as u32,
            Self::Cff(cff) => cff.glyph_count() as u32,
        }
    }

    fn font(&self) -> &FontRef<'a> {
        match self {
            Self::Glyf(glyf) => &glyf.font,
            Self::Cff(cff) => &cff.font,
        }
    }

    fn try_base_outline_kind<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<Option<OutlineKind<'a>>, C::Error> {
        cx.checkpoint(0)?;
        let kind = match self {
            Self::Glyf(glyf) => glyf
                .try_outline(glyph_id, cx)?
                .ok()
                .map(|outline| OutlineKind::Glyf(glyf.clone(), outline)),
            Self::Cff(cff) => Some(OutlineKind::Cff(
                cff.clone(),
                glyph_id,
                cff.try_subfont_index(glyph_id, cx)?,
            )),
        };
        cx.checkpoint(0)?;
        Ok(kind)
    }

    fn try_base_outline_memory<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<usize, C::Error> {
        cx.checkpoint(0)?;
        Ok(match self {
            Self::Glyf(glyf) => glyf
                .try_outline(glyph_id, cx)?
                .ok()
                .map(|outline| cx.glyf_buffer_size(&outline, super::Hinting::None))
                .transpose()?
                .unwrap_or(0),
            Self::Cff(..) => 0,
        })
    }
}

#[derive(Clone)]
pub(crate) struct Outlines<'a> {
    varc: Varc<'a>,
    coverage: read_fonts::tables::layout::CoverageTable<'a>,
    var_store: Option<MultiItemVariationStore<'a>>,
    regions: Option<SparseVariationRegionList<'a>>,
    base: BaseOutlines<'a>,
    glyph_metrics: GlyphHMetrics<'a>,
    units_per_em: u16,
    axis_count: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Outline {
    pub(crate) glyph_id: GlyphId,
    pub(crate) coverage_index: u16,
    max_component_memory: usize,
}

impl Outline {
    pub fn required_buffer_size(&self) -> usize {
        self.max_component_memory
    }
}

impl<'a> Outlines<'a> {
    pub fn new(font: &FontRef<'a>) -> Option<Self> {
        control::ordinary(Self::try_new(font, &mut Ordinary::new()))
    }
    pub(crate) fn try_new<C: WorkControl>(
        font: &FontRef<'a>,
        cx: &mut C,
    ) -> Result<Option<Self>, C::Error> {
        cx.checkpoint(0)?;
        let varc = match control::table::<Varc, _>(font, cx)? {
            Ok(varc) => varc,
            Err(_) => return Ok(None),
        };
        if let Some(glyf) = glyf::Outlines::try_new(font, cx)? {
            return Self::try_from_base(font, varc, BaseOutlines::Glyf(glyf), cx);
        }
        if let Some(cff) = cff::Outlines::try_new(font, cx)? {
            return Self::try_from_base(font, varc, BaseOutlines::Cff(cff), cx);
        }
        Ok(None)
    }
    fn try_from_base<C: WorkControl>(
        font: &FontRef<'a>,
        varc: Varc<'a>,
        base: BaseOutlines<'a>,
        cx: &mut C,
    ) -> Result<Option<Self>, C::Error> {
        let Some(glyph_metrics) = GlyphHMetrics::try_new(font, cx)? else {
            return Ok(None);
        };
        let units_per_em = match control::table::<read_fonts::tables::head::Head, _>(font, cx)? {
            Ok(head) => head.units_per_em(),
            Err(_) => return Ok(None),
        };
        // AxisCollection::new only constructs the fvar/avar views before len().
        let fvar = control::table::<read_fonts::tables::fvar::Fvar, _>(font, cx)?.ok();
        let _avar = control::table::<read_fonts::tables::avar::Avar, _>(font, cx)?.ok();
        let axis_count = fvar.map(|fvar| fvar.axis_count() as usize).unwrap_or(0);
        let coverage = match varc.try_coverage(cx)? {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let var_store = match varc.try_multi_var_store(cx)?.transpose() {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let regions = match var_store
            .as_ref()
            .map(|s| s.try_region_list(cx))
            .transpose()?
            .transpose()
        {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        cx.checkpoint(0)?;
        Ok(Some(Self {
            varc,
            coverage,
            var_store,
            regions,
            base,
            glyph_metrics,
            units_per_em,
            axis_count,
        }))
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn glyph_count(&self) -> u32 {
        self.base.glyph_count()
    }

    pub fn prefer_interpreter(&self) -> bool {
        false
    }

    pub fn fractional_size_hinting(&self) -> bool {
        false
    }

    pub fn font(&self) -> &FontRef<'a> {
        self.base.font()
    }

    pub(crate) fn try_fallback_outline_kind<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<Option<OutlineKind<'a>>, C::Error> {
        self.base.try_base_outline_kind(glyph_id, cx)
    }

    #[cfg(test)]
    pub fn outline(&self, glyph_id: GlyphId) -> Result<Option<Outline>, DrawError> {
        control::ordinary(self.try_outline(glyph_id, &mut Ordinary::new()))
    }
    pub(crate) fn try_outline<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> control::DrawResult<Option<Outline>, C> {
        let result = (|| {
            control::checkpoint(cx, 0)?;
            let Some(coverage_index) = self
                .coverage
                .try_get(glyph_id, cx)
                .map_err(KernelError::Control)?
            else {
                return Ok(None);
            };
            let mut stack: GlyphStack<C> = cx.small().map_err(KernelError::Control)?;
            let memory =
                self.max_component_memory_for_glyph(glyph_id, coverage_index, &mut stack, cx)?;
            Ok(Some(Outline {
                glyph_id,
                coverage_index,
                max_component_memory: memory,
            }))
        })();
        // All stack temporaries retire before the caller can consume this outline.
        cx.checkpoint(0)?;
        control::split(result)
    }
    fn coverage_index<C: WorkControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<Option<u16>, C::Error> {
        self.coverage.try_get(glyph_id, cx)
    }
    fn max_component_memory_for_glyph<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        coverage_index: u16,
        stack: &mut GlyphStack<C>,
        cx: &mut C,
    ) -> KernelResult<usize, C::Error> {
        control::checkpoint(cx, stack.len() as u64)?;
        if stack.contains(&glyph_id) {
            return Ok(0);
        }
        if stack.len() >= GLYF_COMPOSITE_RECURSION_LIMIT {
            return Ok(0);
        }
        stack.try_push(glyph_id).map_err(KernelError::Control)?;
        let mut max_memory = 0;
        let glyph = control::read(self.varc.try_glyph(coverage_index as usize, cx))?;
        let mut components = glyph.try_components();
        while let Some(component) = components.try_next(cx).map_err(KernelError::Control)? {
            let component = component?;
            let gid = component.gid();
            let memory = if gid == glyph_id {
                self.base
                    .try_base_outline_memory(gid, cx)
                    .map_err(KernelError::Control)?
            } else if let Some(index) =
                self.coverage_index(gid, cx).map_err(KernelError::Control)?
            {
                self.max_component_memory_for_glyph(gid, index, stack, cx)?
            } else {
                self.base
                    .try_base_outline_memory(gid, cx)
                    .map_err(KernelError::Control)?
            };
            max_memory = max_memory.max(memory);
        }
        stack.pop();
        control::checkpoint(cx, 0)?;
        Ok(max_memory)
    }

    pub fn draw(
        &self,
        outline: &Outline,
        buf: &mut [u8],
        size: Size,
        coords: &[F2Dot14],
        path_style: PathStyle,
        pen: &mut impl OutlinePen,
    ) -> Result<(), DrawError> {
        control::ordinary(self.try_draw(
            outline,
            buf,
            size,
            coords,
            path_style,
            pen,
            &mut Ordinary::new(),
        ))
    }
    pub(crate) fn try_draw<C: FontControl>(
        &self,
        outline: &Outline,
        buf: &mut [u8],
        size: Size,
        coords: &[F2Dot14],
        path_style: PathStyle,
        pen: &mut impl OutlinePen,
        cx: &mut C,
    ) -> control::DrawResult<(), C> {
        let result = (|| {
            control::checkpoint(cx, 0)?;

            let mut font_coords: CoordVec<C> = cx.small().map_err(KernelError::Control)?;
            try_expand_coords::<C>(&mut font_coords, self.axis_count, coords, cx)
                .map_err(KernelError::Control)?;
            let mut stack: GlyphStack<C> = cx.small().map_err(KernelError::Control)?;
            let pen: &mut dyn OutlinePen = pen;
            let mut scalar_cache = self.try_scalar_cache_from_store(self.var_store.as_ref(), cx)?;
            let mut scratch = Scratchpad::<C>::try_new(cx).map_err(KernelError::Control)?;
            let ctx = VarcSharedContext {
                font_coords: &font_coords,
                size,
                path_style,
                coverage: &self.coverage,
                var_store: self.var_store.as_ref(),
                store_regions: self.var_store.as_ref().zip(self.regions.as_ref()),
            };
            self.draw_glyph(
                outline.glyph_id,
                outline.coverage_index,
                &font_coords,
                Affine::IDENTITY,
                &ctx,
                buf,
                pen,
                &mut stack,
                &mut scalar_cache,
                &mut scratch,
                cx,
            )
        })();
        // Includes natural retirement of coordinates, scalar cache and scratch.
        cx.checkpoint(0)?;
        control::split(result)
    }

    pub fn draw_unscaled(
        &self,
        outline: &Outline,
        buf: &mut [u8],
        coords: &[F2Dot14],
        pen: &mut impl OutlinePen,
    ) -> Result<i32, DrawError> {
        let size = Size::unscaled();
        self.draw(outline, buf, size, coords, PathStyle::default(), pen)?;
        Ok(self.glyph_metrics.advance_width(outline.glyph_id, coords))
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        coverage_index: u16,
        current_coords: &[F2Dot14],
        parent_matrix: Affine,
        ctx: &VarcSharedContext<'a, '_>,
        buf: &mut [u8],
        pen: &mut dyn OutlinePen,
        stack: &mut GlyphStack<C>,
        scalar_cache: &mut ScalarCache<C>,
        scratch: &mut Scratchpad<C>,
        cx: &mut C,
    ) -> KernelResult<(), C::Error> {
        control::checkpoint(cx, 1)?;
        if stack.len() >= GLYF_COMPOSITE_RECURSION_LIMIT {
            return Err(DrawError::RecursionLimitExceeded(glyph_id).into());
        }
        let glyph = control::read(self.varc.try_glyph(coverage_index as usize, cx))?;
        stack.try_push(glyph_id).map_err(KernelError::Control)?;
        let coverage = ctx.coverage;
        let store_regions = ctx.store_regions;
        let mut component_coords_buffer: CoordVec<C> = cx.small().map_err(KernelError::Control)?;
        let mut child_scalar_cache: Option<ScalarCache<C>> = None;
        let mut components = glyph.try_components();
        while let Some(component) = components.try_next(cx).map_err(KernelError::Control)? {
            let component = component?;
            if !self.try_component_condition_met(
                &component,
                current_coords,
                scalar_cache,
                scratch,
                store_regions,
                cx,
            )? {
                continue;
            }
            let component_gid = component.gid();
            let flags = component.flags();

            let coords_the_same = !flags.contains(VarcFlags::HAVE_AXES)
                && !flags.contains(VarcFlags::RESET_UNSPECIFIED_AXES);

            let component_coords = if coords_the_same {
                current_coords
            } else {
                self.try_component_coords(
                    &component,
                    current_coords,
                    &mut component_coords_buffer,
                    scalar_cache,
                    scratch,
                    ctx.font_coords,
                    store_regions,
                    cx,
                )?;
                component_coords_buffer.as_slice()
            };

            let mut transform = *component.transform();
            self.try_apply_transform_variations(
                &component,
                current_coords,
                &mut transform,
                scalar_cache,
                scratch,
                store_regions,
                cx,
            )?;
            let scale = ctx.size.linear_scale(self.units_per_em);
            let matrix = parent_matrix * scale_matrix(transform.matrix(), scale);
            if component_gid != glyph_id {
                if let Some(coverage_index) = coverage
                    .try_get(component_gid, cx)
                    .map_err(KernelError::Control)?
                {
                    control::checkpoint(cx, stack.len() as u64)?;
                    if !stack.contains(&component_gid) {
                        // Optimization: if coordinates haven't changed, we can reuse the scalar cache.
                        if coords_the_same {
                            self.draw_glyph(
                                component_gid,
                                coverage_index,
                                current_coords,
                                matrix,
                                ctx,
                                buf,
                                pen,
                                stack,
                                scalar_cache,
                                scratch,
                                cx,
                            )?;
                        } else {
                            if let Some(ref mut cache) = child_scalar_cache {
                                control::fill(&mut cache.values, ScalarCache::<C>::INVALID, cx)
                                    .map_err(KernelError::Control)?;
                            } else {
                                child_scalar_cache =
                                    Some(self.try_scalar_cache_from_store(ctx.var_store, cx)?);
                            }
                            self.draw_glyph(
                                component_gid,
                                coverage_index,
                                component_coords,
                                matrix,
                                ctx,
                                buf,
                                pen,
                                stack,
                                child_scalar_cache.as_mut().unwrap(),
                                scratch,
                                cx,
                            )?;
                        }
                        continue;
                    }
                }
            }
            let mut transform_pen = TransformPen::new(pen, matrix);
            self.try_draw_base_glyph(
                component_gid,
                component_coords,
                ctx.size,
                ctx.path_style,
                buf,
                &mut transform_pen,
                cx,
            )?;
        }
        stack.pop();
        control::checkpoint(cx, 0)?;
        Ok(())
    }

    fn try_draw_base_glyph<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        size: Size,
        path_style: PathStyle,
        buf: &mut [u8],
        pen: &mut impl OutlinePen,
        cx: &mut C,
    ) -> KernelResult<(), C::Error> {
        let Some(kind) = self
            .base
            .try_base_outline_kind(glyph_id, cx)
            .map_err(KernelError::Control)?
        else {
            return Err(DrawError::GlyphNotFound(glyph_id).into());
        };
        crate::outline::OutlineGlyph { kind }.draw_unhinted_core(
            size,
            crate::instance::LocationRef::new(coords),
            Some(buf),
            path_style,
            pen,
            cx,
        )?;
        control::checkpoint(cx, 0)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]

    fn try_component_coords<C: FontControl>(
        &self,
        component: &VarcComponent<'_>,
        current_coords: &[F2Dot14],
        coords: &mut CoordVec<C>,
        scalar_cache: &mut ScalarCache<C>,
        scratch: &mut Scratchpad<C>,
        font_coords: &[F2Dot14],
        store_regions: Option<(&MultiItemVariationStore<'a>, &SparseVariationRegionList<'a>)>,
        cx: &mut C,
    ) -> KernelResult<(), C::Error> {
        control::checkpoint(cx, 0)?;
        let flags = component.flags();
        if flags.contains(VarcFlags::RESET_UNSPECIFIED_AXES) {
            try_expand_coords::<C>(coords, font_coords.len(), font_coords, cx)
                .map_err(KernelError::Control)?;
        } else {
            try_expand_coords::<C>(coords, current_coords.len(), current_coords, cx)
                .map_err(KernelError::Control)?;
        }

        if !flags.contains(VarcFlags::HAVE_AXES) {
            return Ok(());
        }

        let axis_indices_index = component
            .axis_indices_index()
            .ok_or(ReadError::MalformedData("Missing axisIndicesIndex"))?;
        let num_axes =
            self.try_axis_indices(axis_indices_index as usize, &mut scratch.axis_indices, cx)?;

        self.try_axis_values(component, num_axes, &mut scratch.axis_values, cx)?;
        if let Some(var_idx) = component.axis_values_var_index() {
            let (store, regions) = store_regions.ok_or(ReadError::NullOffset)?;
            control::read(try_compute_tuple_deltas::<C>(
                store,
                regions,
                var_idx,
                current_coords,
                scratch.axis_indices.len(),
                scalar_cache,
                &mut scratch.deltas,
                cx,
            ))?;
            for (value, delta) in scratch.axis_values.iter_mut().zip(scratch.deltas.iter()) {
                control::checkpoint(cx, 1)?;
                *value += *delta;
            }
        }

        for (axis_index, value) in scratch
            .axis_indices
            .iter()
            .zip(scratch.axis_values.iter().copied())
        {
            control::checkpoint(cx, 1)?;
            let Some(slot) = coords.get_mut(*axis_index as usize) else {
                return Err(DrawError::Read(ReadError::OutOfBounds).into());
            };
            let raw = value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            *slot = F2Dot14::from_bits(raw);
        }
        Ok(())
    }

    fn try_axis_indices<C: FontControl>(
        &self,
        nth: usize,
        out: &mut AxisIndexVec<C>,
        cx: &mut C,
    ) -> KernelResult<usize, C::Error> {
        control::checkpoint(cx, 0)?;
        let packed = control::read(self.varc.try_axis_indices(nth, cx))?;
        out.clear();
        let mut values = packed.try_iter(cx).map_err(KernelError::Control)?;
        while let Some(value) = values.try_next(cx).map_err(KernelError::Control)? {
            out.try_push(value as u16).map_err(KernelError::Control)?;
        }
        Ok(out.len())
    }

    fn try_axis_values<C: FontControl>(
        &self,
        component: &VarcComponent<'_>,
        count: usize,
        out: &mut AxisValueVec<C>,
        cx: &mut C,
    ) -> KernelResult<(), C::Error> {
        control::checkpoint(cx, 0)?;
        let Some(packed) = component.axis_values() else {
            out.clear();
            return Ok(());
        };
        out.try_resize_and_fill(count, 0.0)
            .map_err(KernelError::Control)?;
        let mut values = packed.try_iter(cx).map_err(KernelError::Control)?;
        for slot in out.iter_mut() {
            control::checkpoint(cx, 1)?;
            let Some(value) = values.try_next(cx).map_err(KernelError::Control)? else {
                break;
            };
            *slot = value as f32;
        }
        Ok(())
    }

    #[cfg(test)]
    fn apply_transform_variations(
        &self,
        component: &VarcComponent<'_>,
        coords: &[F2Dot14],
        transform: &mut DecomposedTransform,
        scalar_cache: &mut ScalarCache,
        scratch: &mut Scratchpad,
        store_regions: Option<(&MultiItemVariationStore<'a>, &SparseVariationRegionList<'a>)>,
    ) -> Result<(), DrawError> {
        control::ordinary(control::split(self.try_apply_transform_variations(
            component,
            coords,
            transform,
            scalar_cache,
            scratch,
            store_regions,
            &mut Ordinary::new(),
        )))
    }
    fn try_apply_transform_variations<C: FontControl>(
        &self,
        component: &VarcComponent<'_>,
        coords: &[F2Dot14],
        transform: &mut DecomposedTransform,
        scalar_cache: &mut ScalarCache<C>,
        scratch: &mut Scratchpad<C>,
        store_regions: Option<(&MultiItemVariationStore<'a>, &SparseVariationRegionList<'a>)>,
        cx: &mut C,
    ) -> KernelResult<(), C::Error> {
        control::checkpoint(cx, 0)?;
        let Some(var_idx) = component.transform_var_index() else {
            return Ok(());
        };

        let flags = component.flags();

        // Count transform fields using a mask + count_ones
        const TRANSFORM_MASK: VarcFlags = VarcFlags::from_bits_truncate(
            VarcFlags::HAVE_TRANSLATE_X.bits()
                | VarcFlags::HAVE_TRANSLATE_Y.bits()
                | VarcFlags::HAVE_ROTATION.bits()
                | VarcFlags::HAVE_SCALE_X.bits()
                | VarcFlags::HAVE_SCALE_Y.bits()
                | VarcFlags::HAVE_SKEW_X.bits()
                | VarcFlags::HAVE_SKEW_Y.bits()
                | VarcFlags::HAVE_TCENTER_X.bits()
                | VarcFlags::HAVE_TCENTER_Y.bits(),
        );
        let field_count = (flags.bits() & TRANSFORM_MASK.bits()).count_ones() as usize;
        if field_count == 0 {
            return Ok(());
        }

        let (store, regions) = store_regions.ok_or(ReadError::NullOffset)?;
        control::read(try_compute_tuple_deltas::<C>(
            store,
            regions,
            var_idx,
            coords,
            field_count,
            scalar_cache,
            &mut scratch.deltas,
            cx,
        ))?;

        // Apply deltas in flag order, consuming from iterator
        let mut delta_iter = scratch.deltas.iter().copied();

        if flags.contains(VarcFlags::HAVE_TRANSLATE_X) {
            let delta = delta_iter.next().unwrap_or(0.0);
            transform.set_translate_x(transform.translate_x() + delta);
        }
        if flags.contains(VarcFlags::HAVE_TRANSLATE_Y) {
            let delta = delta_iter.next().unwrap_or(0.0);
            transform.set_translate_y(transform.translate_y() + delta);
        }
        if flags.contains(VarcFlags::HAVE_ROTATION) {
            let delta = delta_iter.next().unwrap_or(0.0);
            transform.set_rotation(transform.rotation() + delta / 4096.0);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_X) {
            let delta = delta_iter.next().unwrap_or(0.0);
            transform.set_scale_x(transform.scale_x() + delta / 1024.0);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_Y) {
            let delta = delta_iter.next().unwrap_or(0.0);
            transform.set_scale_y(transform.scale_y() + delta / 1024.0);
        }
        const SKEW_OR_CENTER: VarcFlags = VarcFlags::from_bits_truncate(
            VarcFlags::HAVE_SKEW_X.bits()
                | VarcFlags::HAVE_SKEW_Y.bits()
                | VarcFlags::HAVE_TCENTER_X.bits()
                | VarcFlags::HAVE_TCENTER_Y.bits(),
        );
        if flags.intersects(SKEW_OR_CENTER) {
            if flags.contains(VarcFlags::HAVE_SKEW_X) {
                let delta = delta_iter.next().unwrap_or(0.0);
                transform.set_skew_x(transform.skew_x() + delta / 4096.0);
            }
            if flags.contains(VarcFlags::HAVE_SKEW_Y) {
                let delta = delta_iter.next().unwrap_or(0.0);
                transform.set_skew_y(transform.skew_y() + delta / 4096.0);
            }
            if flags.contains(VarcFlags::HAVE_TCENTER_X) {
                let delta = delta_iter.next().unwrap_or(0.0);
                transform.set_center_x(transform.center_x() + delta);
            }
            if flags.contains(VarcFlags::HAVE_TCENTER_Y) {
                let delta = delta_iter.next().unwrap_or(0.0);
                transform.set_center_y(transform.center_y() + delta);
            }
        }

        if !flags.contains(VarcFlags::HAVE_SCALE_Y) {
            transform.set_scale_y(transform.scale_x());
        }
        Ok(())
    }

    fn try_component_condition_met<C: FontControl>(
        &self,
        component: &VarcComponent<'_>,
        coords: &[F2Dot14],
        scalar_cache: &mut ScalarCache<C>,
        scratch: &mut Scratchpad<C>,
        store_regions: Option<(&MultiItemVariationStore<'a>, &SparseVariationRegionList<'a>)>,
        cx: &mut C,
    ) -> KernelResult<bool, C::Error> {
        control::checkpoint(cx, 0)?;
        let Some(condition_index) = component.condition_index() else {
            return Ok(true);
        };
        let Some(condition_list) = self
            .varc
            .try_condition_list(cx)
            .map_err(KernelError::Control)?
        else {
            return Err(DrawError::Read(ReadError::NullOffset).into());
        };
        let condition_list = condition_list?;
        let condition = control::read(
            condition_list
                .conditions()
                .try_get(condition_index as usize, cx),
        )?;
        let (store, regions) = store_regions.ok_or(ReadError::NullOffset)?;
        Self::try_eval_condition(
            &condition,
            coords,
            store,
            regions,
            scalar_cache,
            scratch,
            cx,
        )
    }

    fn try_eval_condition<C: FontControl>(
        condition: &Condition<'a>,
        coords: &[F2Dot14],
        var_store: &MultiItemVariationStore<'a>,
        regions: &SparseVariationRegionList<'a>,
        scalar_cache: &mut ScalarCache<C>,
        scratch: &mut Scratchpad<C>,
        cx: &mut C,
    ) -> KernelResult<bool, C::Error> {
        let result = (|| {
            let mut frames: C::Owned<ConditionFrame<'a>> =
                cx.owned().map_err(KernelError::Control)?;
            control::checkpoint(cx, 1)?;
            frames
                .try_push(ConditionFrame {
                    node: condition.clone(),
                    next_child: 0,
                })
                .map_err(KernelError::Control)?;
            let mut returned = None;
            loop {
                control::checkpoint(cx, 1)?;
                if frames.as_slice().is_empty() {
                    return Ok(returned.unwrap_or(false));
                }
                if let Some(value) = returned.take() {
                    let frame = frames.as_slice().last().ok_or(ReadError::OutOfBounds)?;
                    let short = match &frame.node {
                        Condition::Format3And(_) => {
                            if !value {
                                Some(false)
                            } else {
                                None
                            }
                        }
                        Condition::Format4Or(_) => {
                            if value {
                                Some(true)
                            } else {
                                None
                            }
                        }
                        Condition::Format5Negate(_) => Some(!value),
                        _ => {
                            return Err(ReadError::MalformedData(
                                "condition continuation has no child",
                            )
                            .into())
                        }
                    };
                    if let Some(value) = short {
                        frames.pop();
                        returned = Some(value);
                        continue;
                    }
                }
                let step = {
                    let frame = frames
                        .as_mut_slice()
                        .last_mut()
                        .ok_or(ReadError::OutOfBounds)?;
                    match &frame.node {
                        Condition::Format1AxisRange(node) => {
                            let coord = coords
                                .get(node.axis_index() as usize)
                                .copied()
                                .unwrap_or(F2Dot14::ZERO);
                            ConditionStep::Return(
                                coord >= node.filter_range_min_value()
                                    && coord <= node.filter_range_max_value(),
                            )
                        }
                        Condition::Format2VariableValue(node) => {
                            control::read(try_compute_tuple_deltas::<C>(
                                var_store,
                                regions,
                                node.var_index(),
                                coords,
                                1,
                                scalar_cache,
                                &mut scratch.deltas,
                                cx,
                            ))?;
                            let delta = scratch.deltas.first().copied().unwrap_or(0.0);
                            ConditionStep::Return(node.default_value() as f32 + delta > 0.0)
                        }
                        Condition::Format3And(node) => {
                            let offsets = node.conditions();
                            if frame.next_child == offsets.len() {
                                ConditionStep::Return(true)
                            } else {
                                let child = control::read(offsets.try_get(frame.next_child, cx))?;
                                frame.next_child += 1;
                                ConditionStep::Descend(child)
                            }
                        }
                        Condition::Format4Or(node) => {
                            let offsets = node.conditions();
                            if frame.next_child == offsets.len() {
                                ConditionStep::Return(false)
                            } else {
                                let child = control::read(offsets.try_get(frame.next_child, cx))?;
                                frame.next_child += 1;
                                ConditionStep::Descend(child)
                            }
                        }
                        Condition::Format5Negate(node) => {
                            ConditionStep::Descend(control::read(node.try_condition(cx))?)
                        }
                    }
                };
                match step {
                    ConditionStep::Return(value) => {
                        frames.pop();
                        returned = Some(value);
                    }
                    ConditionStep::Descend(node) => {
                        control::checkpoint(cx, 1)?;
                        frames
                            .try_push(ConditionFrame {
                                node,
                                next_child: 0,
                            })
                            .map_err(KernelError::Control)?;
                    }
                }
            }
        })();
        // Even the last paid frame allocation must retire before this consumer.
        control::checkpoint(cx, 0)?;
        result
    }

    fn try_scalar_cache_from_store<C: FontControl>(
        &self,
        store: Option<&MultiItemVariationStore<'a>>,
        cx: &mut C,
    ) -> KernelResult<ScalarCache<C>, C::Error> {
        control::checkpoint(cx, 0)?;
        let region_count = match store {
            Some(store) => control::read(store.try_region_list(cx))?.region_count() as usize,
            // The table's variation store is genuinely nullable. Static
            // components need no scalars; a component that actually needs a
            // missing store still returns the original NullOffset font error.
            None => 0,
        };
        ScalarCache::<C>::try_new(region_count, cx).map_err(KernelError::Control)
    }
}

struct ConditionFrame<'f> {
    node: Condition<'f>,
    next_child: usize,
}
enum ConditionStep<'f> {
    Descend(Condition<'f>),
    Return(bool),
}

struct ScalarCache<C: FontControl = Ordinary> {
    values: ScalarCacheVec<C>,
}

#[cfg(test)]
impl ScalarCache {
    fn new(count: usize) -> Self {
        control::ordinary(Self::try_new(count, &mut Ordinary::new()))
    }
}
impl<C: FontControl> ScalarCache<C> {
    const INVALID: f32 = 2.0;
    fn try_new(count: usize, cx: &mut C) -> Result<Self, C::Error> {
        let mut values: ScalarCacheVec<C> = cx.small()?;
        values.try_resize_and_fill(count, Self::INVALID)?;
        Ok(Self { values })
    }
    fn get(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(Self::INVALID)
    }
    fn set(&mut self, index: usize, value: f32) {
        if let Some(slot) = self.values.get_mut(index) {
            *slot = value;
        }
    }
}

#[cfg(test)]
fn expand_coords(out: &mut CoordVec, axis_count: usize, coords: &[F2Dot14]) {
    control::ordinary(try_expand_coords::<Ordinary>(
        out,
        axis_count,
        coords,
        &mut Ordinary::new(),
    ))
}
fn try_expand_coords<C: FontControl>(
    out: &mut CoordVec<C>,
    axis_count: usize,
    coords: &[F2Dot14],
    cx: &mut C,
) -> Result<(), C::Error> {
    cx.checkpoint(0)?;
    out.try_resize_and_fill(axis_count, F2Dot14::ZERO)?;
    let count = out.len().min(coords.len());
    control::copy(&mut out[..count], &coords[..count], cx)
}

#[cfg(test)]
fn compute_tuple_deltas(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[F2Dot14],
    tuple_len: usize,
    cache: &mut ScalarCache,
    out: &mut DeltaVec,
) -> Result<(), ReadError> {
    control::ordinary(try_compute_tuple_deltas::<Ordinary>(
        store,
        regions,
        var_idx,
        coords,
        tuple_len,
        cache,
        out,
        &mut Ordinary::new(),
    ))
}
fn try_compute_tuple_deltas<C: FontControl>(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[F2Dot14],
    tuple_len: usize,
    cache: &mut ScalarCache<C>,
    out: &mut DeltaVec<C>,
    cx: &mut C,
) -> read_fonts::control::ReadCtl<(), C> {
    macro_rules! font {
        ($expression:expr) => {
            match $expression {
                Ok(value) => value,
                Err(error) => return Ok(Err(error)),
            }
        };
    }
    cx.checkpoint(0)?;
    out.try_resize_and_fill(tuple_len, 0.0)?;
    if tuple_len == 0 || var_idx == NO_VARIATION_INDEX {
        return Ok(Ok(()));
    }
    let outer = (var_idx >> 16) as usize;
    let inner = (var_idx & 0xFFFF) as usize;
    // Both accessors construct offset-array views only; children are resolved
    // through try_get below, never the ordinary FontRead bridge.
    let data = font!(store
        .variation_data()
        .try_get(outer, cx)?
        .map_err(|_| ReadError::InvalidCollectionIndex(outer as _)));
    let delta_set = font!(data.try_delta_set(inner, cx)?);
    let mut deltas = delta_set.fetcher();
    let regions = regions.regions();
    let out_slice = out.as_mut_slice();
    let mut skip = 0;
    for region_index in data.region_indices().iter() {
        cx.checkpoint(1)?;
        let region_idx = region_index.get() as usize;
        let mut scalar = cache.get(region_idx);
        if scalar >= 2.0 {
            scalar = font!(regions.try_get(region_idx, cx)?).try_compute_scalar_f32(coords, cx)?;
            cache.set(region_idx, scalar);
        }
        // Keep the original lazy skipping order, including its unused tail.
        if scalar == 0.0 {
            skip += out_slice.len();
        } else {
            if skip != 0 {
                font!(deltas.try_skip(skip, cx)?);
                skip = 0;
            }
            font!(deltas.try_add_to_f32_scaled(out_slice, scalar, cx)?);
        }
    }
    cx.checkpoint(0)?;
    Ok(Ok(()))
}

#[inline(always)]
fn scale_matrix(mut m: Affine, s: f32) -> Affine {
    m.dx *= s;
    m.dy *= s;
    m
}

struct TransformPen<'a, P: OutlinePen + ?Sized> {
    pen: &'a mut P,
    matrix: Affine,
}

impl<'a, P: OutlinePen + ?Sized> TransformPen<'a, P> {
    fn new(pen: &'a mut P, matrix: Affine) -> Self {
        Self { pen, matrix }
    }

    #[inline(always)]
    fn transform(&self, x: f32, y: f32) -> (f32, f32) {
        self.matrix.transform(x, y)
    }
}

impl<P: OutlinePen + ?Sized> OutlinePen for TransformPen<'_, P> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform(x, y);
        self.pen.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform(x, y);
        self.pen.line_to(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (cx0, cy0) = self.transform(cx0, cy0);
        let (x, y) = self.transform(x, y);
        self.pen.quad_to(cx0, cy0, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (cx0, cy0) = self.transform(cx0, cy0);
        let (cx1, cy1) = self.transform(cx1, cy1);
        let (x, y) = self.transform(x, y);
        self.pen.curve_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.pen.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::pen::PathElement;
    use read_fonts::{FontRef, TableProvider};

    fn coord(value: f32) -> F2Dot14 {
        F2Dot14::from_f32(value)
    }

    fn assert_close(actual: f32, expected: f32) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= 1e-6,
            "expected {expected}, got {actual}, diff {diff}"
        );
    }

    fn path_head_signature(path: &[PathElement], n: usize) -> Vec<String> {
        path.iter()
            .take(n)
            .map(|el| match *el {
                PathElement::MoveTo { x, y } => format!("M{:.2},{:.2}", x, y),
                PathElement::LineTo { x, y } => format!("L{:.2},{:.2}", x, y),
                PathElement::QuadTo { cx0, cy0, x, y } => {
                    format!("Q{:.2},{:.2} {:.2},{:.2}", cx0, cy0, x, y)
                }
                PathElement::CurveTo {
                    cx0,
                    cy0,
                    cx1,
                    cy1,
                    x,
                    y,
                } => format!(
                    "C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
                    cx0, cy0, cx1, cy1, x, y
                ),
                PathElement::Close => "Z".to_string(),
            })
            .collect()
    }

    #[test]
    fn expand_coords_pads_and_truncates() {
        let mut out = CoordVec::<Ordinary>::new();
        expand_coords(&mut out, 4, &[coord(0.25), coord(-0.5)]);
        assert_eq!(
            out.as_slice(),
            &[coord(0.25), coord(-0.5), F2Dot14::ZERO, F2Dot14::ZERO]
        );

        expand_coords(&mut out, 1, &[coord(0.25), coord(-0.5)]);
        assert_eq!(out.as_slice(), &[coord(0.25)]);
    }

    #[test]
    fn scale_matrix_only_scales_translation() {
        let matrix = Matrix::from_elements([1.0, 2.0, 3.0, 4.0, 5.0, -6.0]);
        assert_eq!(
            scale_matrix(matrix, 10.0).elements(),
            [1.0, 2.0, 3.0, 4.0, 50.0, -60.0]
        );
    }

    #[test]
    fn compute_tuple_deltas_no_variation_index_is_noop_after_resize() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let varc = font.varc().unwrap();
        let store = varc.multi_var_store().unwrap().unwrap();
        let regions = store.region_list().unwrap();
        let mut cache = ScalarCache::new(regions.region_count() as usize);
        let mut out = DeltaVec::<Ordinary>::new();
        out.push(42.0);

        compute_tuple_deltas(
            &store,
            &regions,
            NO_VARIATION_INDEX,
            &[coord(0.0)],
            3,
            &mut cache,
            &mut out,
        )
        .unwrap();
        assert_eq!(out.as_slice(), &[0.0, 0.0, 0.0]);

        compute_tuple_deltas(
            &store,
            &regions,
            NO_VARIATION_INDEX,
            &[coord(0.0)],
            0,
            &mut cache,
            &mut out,
        )
        .unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn compute_tuple_deltas_invalid_outer_index_errors() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let varc = font.varc().unwrap();
        let store = varc.multi_var_store().unwrap().unwrap();
        let regions = store.region_list().unwrap();
        let mut cache = ScalarCache::new(regions.region_count() as usize);
        let mut out = DeltaVec::<Ordinary>::new();

        let err = compute_tuple_deltas(&store, &regions, 0xFFFF_0000, &[], 1, &mut cache, &mut out)
            .unwrap_err();
        assert!(matches!(err, ReadError::InvalidCollectionIndex(_)));
    }

    #[test]
    fn compute_tuple_deltas_matches_manual_decode() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let varc = font.varc().unwrap();
        let store = varc.multi_var_store().unwrap().unwrap();
        let regions = store.region_list().unwrap();
        let region_list = regions.regions();
        let coords = [coord(0.5); 8];

        let mut tried = 0usize;
        for (outer, data) in store.variation_data().iter().enumerate() {
            let data = data.unwrap();
            let region_count = data.region_indices().len();
            if region_count == 0 {
                continue;
            }
            let delta_set_count = data.delta_sets().unwrap().count() as usize;
            for inner in 0..delta_set_count.min(3) {
                let decoded = data.delta_set(inner).unwrap().iter().collect::<Vec<_>>();
                if decoded.is_empty() || decoded.len() % region_count != 0 {
                    continue;
                }
                let tuple_len = decoded.len() / region_count;
                let var_idx = ((outer as u32) << 16) | inner as u32;

                let mut cache = ScalarCache::new(regions.region_count() as usize);
                let mut actual = DeltaVec::<Ordinary>::new();
                compute_tuple_deltas(
                    &store,
                    &regions,
                    var_idx,
                    &coords,
                    tuple_len,
                    &mut cache,
                    &mut actual,
                )
                .unwrap();
                let first = actual.as_slice().to_vec();

                // Same cache after population should not alter results.
                compute_tuple_deltas(
                    &store,
                    &regions,
                    var_idx,
                    &coords,
                    tuple_len,
                    &mut cache,
                    &mut actual,
                )
                .unwrap();
                assert_eq!(actual.as_slice(), first.as_slice());

                let mut expected = vec![0.0f32; tuple_len];
                for (region_order, region_idx) in data.region_indices().iter().enumerate() {
                    let scalar = region_list
                        .get(region_idx.get() as usize)
                        .unwrap()
                        .compute_scalar_f32(&coords);
                    if scalar == 0.0 {
                        continue;
                    }
                    let base = region_order * tuple_len;
                    for (i, slot) in expected.iter_mut().enumerate() {
                        *slot += decoded[base + i] as f32 * scalar;
                    }
                }
                assert_eq!(actual.len(), expected.len());
                for (a, e) in actual.iter().zip(expected.iter()) {
                    assert_close(*a, *e);
                }
                tried += 1;
            }
        }
        assert!(tried > 0, "expected at least one tuple to be exercised");
    }

    fn apply_transform_variations_reference(
        component: &VarcComponent<'_>,
        coords: &[F2Dot14],
        transform: &mut DecomposedTransform,
        var_store: Option<&MultiItemVariationStore<'_>>,
        regions: Option<&SparseVariationRegionList<'_>>,
        scalar_cache: &mut ScalarCache,
        deltas: &mut DeltaVec,
    ) -> Result<(), DrawError> {
        let Some(var_idx) = component.transform_var_index() else {
            return Ok(());
        };
        let flags = component.flags();
        const TRANSFORM_MASK: VarcFlags = VarcFlags::from_bits_truncate(
            VarcFlags::HAVE_TRANSLATE_X.bits()
                | VarcFlags::HAVE_TRANSLATE_Y.bits()
                | VarcFlags::HAVE_ROTATION.bits()
                | VarcFlags::HAVE_SCALE_X.bits()
                | VarcFlags::HAVE_SCALE_Y.bits()
                | VarcFlags::HAVE_SKEW_X.bits()
                | VarcFlags::HAVE_SKEW_Y.bits()
                | VarcFlags::HAVE_TCENTER_X.bits()
                | VarcFlags::HAVE_TCENTER_Y.bits(),
        );
        let field_count = (flags.bits() & TRANSFORM_MASK.bits()).count_ones() as usize;
        if field_count == 0 {
            return Ok(());
        }

        let store = var_store.ok_or(ReadError::NullOffset)?;
        let regions = regions.ok_or(ReadError::NullOffset)?;
        compute_tuple_deltas(
            store,
            regions,
            var_idx,
            coords,
            field_count,
            scalar_cache,
            deltas,
        )?;

        let mut delta_iter = deltas.iter().copied();
        if flags.contains(VarcFlags::HAVE_TRANSLATE_X) {
            transform.set_translate_x(transform.translate_x() + delta_iter.next().unwrap_or(0.0));
        }
        if flags.contains(VarcFlags::HAVE_TRANSLATE_Y) {
            transform.set_translate_y(transform.translate_y() + delta_iter.next().unwrap_or(0.0));
        }
        if flags.contains(VarcFlags::HAVE_ROTATION) {
            transform
                .set_rotation(transform.rotation() + delta_iter.next().unwrap_or(0.0) / 4096.0);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_X) {
            transform.set_scale_x(transform.scale_x() + delta_iter.next().unwrap_or(0.0) / 1024.0);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_Y) {
            transform.set_scale_y(transform.scale_y() + delta_iter.next().unwrap_or(0.0) / 1024.0);
        }
        const SKEW_OR_CENTER: VarcFlags = VarcFlags::from_bits_truncate(
            VarcFlags::HAVE_SKEW_X.bits()
                | VarcFlags::HAVE_SKEW_Y.bits()
                | VarcFlags::HAVE_TCENTER_X.bits()
                | VarcFlags::HAVE_TCENTER_Y.bits(),
        );
        if flags.intersects(SKEW_OR_CENTER) {
            if flags.contains(VarcFlags::HAVE_SKEW_X) {
                transform
                    .set_skew_x(transform.skew_x() + delta_iter.next().unwrap_or(0.0) / 4096.0);
            }
            if flags.contains(VarcFlags::HAVE_SKEW_Y) {
                transform
                    .set_skew_y(transform.skew_y() + delta_iter.next().unwrap_or(0.0) / 4096.0);
            }
            if flags.contains(VarcFlags::HAVE_TCENTER_X) {
                transform.set_center_x(transform.center_x() + delta_iter.next().unwrap_or(0.0));
            }
            if flags.contains(VarcFlags::HAVE_TCENTER_Y) {
                transform.set_center_y(transform.center_y() + delta_iter.next().unwrap_or(0.0));
            }
        }
        if !flags.contains(VarcFlags::HAVE_SCALE_Y) {
            transform.set_scale_y(transform.scale_x());
        }
        Ok(())
    }

    #[test]
    fn apply_transform_variations_matches_reference_path() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let outlines = Outlines::new(&font).unwrap();
        let coverage = outlines.varc.coverage().unwrap();
        let var_store = outlines.varc.multi_var_store().transpose().unwrap();
        let regions = var_store
            .as_ref()
            .map(|s| s.region_list())
            .transpose()
            .unwrap();
        let region_count = regions
            .as_ref()
            .map(|r| r.region_count() as usize)
            .unwrap_or(0);

        let mut coords = CoordVec::<Ordinary>::new();
        coords.resize_and_fill(outlines.axis_count, F2Dot14::ZERO);
        for (i, c) in coords.iter_mut().enumerate() {
            *c = match i % 4 {
                0 => coord(0.5),
                1 => coord(-0.5),
                2 => coord(0.25),
                _ => coord(-0.25),
            };
        }

        let mut tested = 0usize;
        for gid16 in coverage.iter() {
            let gid: GlyphId = gid16.into();
            let coverage_index = coverage.get(gid).unwrap() as usize;
            let glyph = outlines.varc.glyph(coverage_index).unwrap();
            for component in glyph.components() {
                let component = component.unwrap();
                if component.transform_var_index().is_none() {
                    continue;
                }

                let mut transform_new = *component.transform();
                let mut transform_ref = *component.transform();
                let mut cache_new = ScalarCache::new(region_count);
                let mut cache_ref = ScalarCache::new(region_count);
                let mut deltas_ref = DeltaVec::<Ordinary>::new();
                let mut scratch = Scratchpad::new();
                let store_regions = var_store.as_ref().zip(regions.as_ref());

                outlines
                    .apply_transform_variations(
                        &component,
                        &coords,
                        &mut transform_new,
                        &mut cache_new,
                        &mut scratch,
                        store_regions,
                    )
                    .unwrap();
                apply_transform_variations_reference(
                    &component,
                    &coords,
                    &mut transform_ref,
                    var_store.as_ref(),
                    regions.as_ref(),
                    &mut cache_ref,
                    &mut deltas_ref,
                )
                .unwrap();

                assert_close(transform_new.translate_x(), transform_ref.translate_x());
                assert_close(transform_new.translate_y(), transform_ref.translate_y());
                assert_close(transform_new.rotation(), transform_ref.rotation());
                assert_close(transform_new.scale_x(), transform_ref.scale_x());
                assert_close(transform_new.scale_y(), transform_ref.scale_y());
                assert_close(transform_new.skew_x(), transform_ref.skew_x());
                assert_close(transform_new.skew_y(), transform_ref.skew_y());
                assert_close(transform_new.center_x(), transform_ref.center_x());
                assert_close(transform_new.center_y(), transform_ref.center_y());
                tested += 1;
                if tested >= 32 {
                    break;
                }
            }
            if tested >= 32 {
                break;
            }
        }
        assert!(tested > 0, "expected at least one transformed component");
    }

    #[test]
    fn draw_varc_6868_freetype_path_head_snapshot() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let outlines = Outlines::new(&font).unwrap();
        let gid = font.cmap().unwrap().map_codepoint(0x6868_u32).unwrap();
        let outline = outlines.outline(gid).unwrap().unwrap();
        let mut memory = vec![0u8; outline.required_buffer_size()];
        let mut pen = Vec::<PathElement>::new();
        outlines
            .draw(
                &outline,
                &mut memory,
                Size::unscaled(),
                &[],
                PathStyle::FreeType,
                &mut pen,
            )
            .unwrap();

        let head = path_head_signature(&pen, 8);
        assert_eq!(
            head,
            vec![
                "M454.56,574.77".to_string(),
                "Q477.58,585.56 499.80,598.25".to_string(),
                "Q522.02,610.95 543.36,625.16".to_string(),
                "Q564.71,639.37 584.54,655.19".to_string(),
                "Q604.38,671.02 623.03,688.41".to_string(),
                "Q641.67,705.80 658.41,724.46".to_string(),
                "Q675.14,743.12 689.79,763.21".to_string(),
                "Q704.43,783.31 717.03,804.56".to_string(),
            ]
        );
    }
}
