// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use super::storage::Control;
use crate::{
    hb::{
        face::{BasicFontMetrics, Scale},
        tables::TableRanges,
    },
    GlyphExtents, GlyphInfo, GlyphPosition, Tag,
};
use markdown_svg_budget::{Error, RequestControl};
use read_fonts::{
    tables::{
        glyf::Glyf,
        gvar::Gvar,
        hmtx::{Hmtx, LongMetric},
        hvar::Hvar,
        loca::Loca,
        mvar::Mvar,
        vmtx::Vmtx,
        vorg::Vorg,
        vvar::Vvar,
    },
    types::{BoundingBox, F2Dot14, Fixed, GlyphId, Point},
    FontRef, TableProvider,
};

#[derive(Clone)]
pub struct GlyphMetrics<'a> {
    control: Control,
    _hmtx: Option<Hmtx<'a>>,
    h_metrics: &'a [LongMetric],
    hvar: Option<Hvar<'a>>,
    vmtx: Option<Vmtx<'a>>,
    vvar: Option<Vvar<'a>>,
    vorg: Option<Vorg<'a>>,
    glyf: Option<GlyfTables<'a>>,
    mvar: Option<Mvar<'a>>,
    num_glyphs: u32,
    upem: u16,
    ascent: i16,
    descent: i16,
}

#[derive(Clone)]
struct GlyfTables<'a> {
    loca: Loca<'a>,
    glyf: Glyf<'a>,
    gvar: Option<Gvar<'a>>,
}

impl<'a> GlyphMetrics<'a> {
    fn read_args<T: read_fonts::control::TryFontReadWithArgs<'a>>(
        range: super::tables::TableRange,
        font: &FontRef<'a>,
        args: &T::Args,
        control: &Control,
    ) -> Result<Option<T>, Error> {
        control.step(0)?;
        let Some(data) = range.resolve_data(font) else {
            return Ok(None);
        };
        match &control.0 {
            Some(r) => T::try_read_with_args(data, args, &mut RequestControl(r)).map(|v| v.ok()),
            None => Ok(T::read_with_args(data, args).ok()),
        }
    }
    fn hvar_width(&self, t: &Hvar, g: GlyphId, c: &[F2Dot14]) -> Result<Fixed, Error> {
        Ok(match &self.control.0 {
            Some(r) => t.try_advance_width_delta(g, c, &mut RequestControl(r))?,
            None => t.advance_width_delta(g, c),
        }
        .unwrap_or_default())
    }
    fn vvar_height(&self, t: &Vvar, g: GlyphId, c: &[F2Dot14]) -> Result<Fixed, Error> {
        Ok(match &self.control.0 {
            Some(r) => t.try_advance_height_delta(g, c, &mut RequestControl(r))?,
            None => t.advance_height_delta(g, c),
        }
        .unwrap_or_default())
    }
    fn vvar_tsb(&self, t: &Vvar, g: GlyphId, c: &[F2Dot14]) -> Result<Fixed, Error> {
        Ok(match &self.control.0 {
            Some(r) => t.try_tsb_delta(g, c, &mut RequestControl(r))?,
            None => t.tsb_delta(g, c),
        }
        .unwrap_or_default())
    }
    fn vvar_origin(&self, t: &Vvar, g: GlyphId, c: &[F2Dot14]) -> Result<Fixed, Error> {
        Ok(match &self.control.0 {
            Some(r) => t.try_v_org_delta(g, c, &mut RequestControl(r))?,
            None => t.v_org_delta(g, c),
        }
        .unwrap_or_default())
    }
    fn mvar_delta(&self, t: &Mvar, tag: Tag, c: &[F2Dot14]) -> Result<Fixed, Error> {
        Ok(match &self.control.0 {
            Some(r) => t.try_metric_delta(tag, c, &mut RequestControl(r))?,
            None => t.metric_delta(tag, c),
        }
        .unwrap_or_default())
    }
    fn vorg_origin(&self, t: &Vorg, g: GlyphId) -> Result<i16, Error> {
        match &self.control.0 {
            Some(r) => t.try_vertical_origin_y(g, &mut RequestControl(r)),
            None => Ok(t.vertical_origin_y(g)),
        }
    }

    pub(crate) fn try_new(
        font: &FontRef<'a>,
        table_ranges: &TableRanges,
        control: &Control,
    ) -> Result<Self, Error> {
        let num_glyphs = table_ranges.num_glyphs;
        let upem = table_ranges.units_per_em;
        let hmtx = Self::read_args::<Hmtx>(
            table_ranges.hmtx,
            font,
            &table_ranges.num_h_metrics,
            control,
        )?;
        let h_metrics = hmtx
            .as_ref()
            .map(|hmtx| hmtx.h_metrics())
            .unwrap_or_default();
        let hvar = table_ranges.hvar.try_resolve_table(font, control)?;
        let vmtx = Self::read_args::<Vmtx>(
            table_ranges.vmtx,
            font,
            &table_ranges.num_v_metrics,
            control,
        )?;
        let vvar = table_ranges.vvar.try_resolve_table(font, control)?;
        let vorg = table_ranges.vorg.try_resolve_table(font, control)?;
        let loca =
            Self::read_args::<Loca>(table_ranges.loca, font, &table_ranges.loca_long, control)?;
        let glyf = table_ranges.glyf.try_resolve_table(font, control)?;
        let glyf = if let Some((loca, glyf)) = loca.zip(glyf) {
            let gvar = table_ranges.gvar.try_resolve_table(font, control)?;
            Some(GlyfTables { loca, glyf, gvar })
        } else {
            None
        };
        let mvar = table_ranges.mvar.try_resolve_table(font, control)?;
        let ascent = table_ranges.ascent;
        let descent = table_ranges.descent;
        control.step(0)?;
        Ok(Self {
            control: control.clone(),
            _hmtx: hmtx,
            h_metrics,
            hvar,
            vmtx,
            vvar,
            vorg,
            glyf,
            mvar,
            num_glyphs,
            upem,
            ascent,
            descent,
        })
    }

    pub(crate) fn from_tables(font: &impl TableProvider<'a>, metrics: &BasicFontMetrics) -> Self {
        let hmtx = font.hmtx().ok();
        let h_metrics = hmtx
            .as_ref()
            .map(|hmtx| hmtx.h_metrics())
            .unwrap_or_default();
        let hvar = font.hvar().ok();
        let vmtx = font.vmtx().ok();
        let vvar = font.vvar().ok();
        let vorg = font.vorg().ok();
        let loca = font.loca(None).ok();
        let glyf = font.glyf().ok();
        let glyf = if let Some((loca, glyf)) = loca.zip(glyf) {
            let gvar = font.gvar().ok();
            Some(GlyfTables { loca, glyf, gvar })
        } else {
            None
        };
        let mvar = font.mvar().ok();
        Self {
            control: Control::default(),
            _hmtx: hmtx,
            h_metrics,
            hvar,
            vmtx,
            vvar,
            vorg,
            glyf,
            mvar,
            num_glyphs: metrics.num_glyphs,
            upem: metrics.units_per_em,
            ascent: metrics.ascent,
            descent: metrics.descent,
        }
    }

    pub(crate) fn advance_width(&self, gid: impl Into<GlyphId>, coords: &[F2Dot14]) -> Option<i32> {
        match self.try_advance_width(gid, coords) {
            Ok(value) => value,
            Err(error) => {
                self.control.record(error);
                None
            }
        }
    }

    pub(crate) fn try_advance_width(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Result<Option<i32>, Error> {
        self.control.step(1)?;
        let gid = gid.into();
        let Some(mut advance) = self
            .h_metrics
            .get(gid.to_u32() as usize)
            .or_else(|| self.h_metrics.last())
            .map(|metric| metric.advance() as i32)
        else {
            return Ok((gid.to_u32() < self.num_glyphs).then_some(self.upem as i32 / 2));
        };
        if !coords.is_empty() {
            if let Some(hvar) = self.hvar.as_ref() {
                advance += self.hvar_width(hvar, gid, coords)?.to_i32();
            } else if let Some(deltas) = self.try_phantom_deltas(gid, coords)? {
                advance += deltas[1].x.to_i32() - deltas[0].x.to_i32();
            }
        }
        Ok(Some(advance))
    }

    pub(crate) fn populate_advance_widths(
        &self,
        infos: &[GlyphInfo],
        pos: &mut [GlyphPosition],
        coords: &[F2Dot14],
        scale: Scale,
    ) {
        if let Err(e) = self.try_populate_advance_widths(infos, pos, coords, scale) {
            self.control.record(e);
        }
    }
    pub(crate) fn try_populate_advance_widths(
        &self,
        infos: &[GlyphInfo],
        pos: &mut [GlyphPosition],
        coords: &[F2Dot14],
        scale: Scale,
    ) -> Result<(), Error> {
        self.control.step(0)?;
        for (info, pos) in infos.iter().zip(pos.iter_mut()) {
            self.control.step(1)?;
            pos.x_advance = self
                .h_metrics
                .get(info.glyph_id as usize)
                .or_else(|| self.h_metrics.last())
                .map(|metric| metric.advance() as i32)
                .or_else(|| (info.glyph_id < self.num_glyphs).then_some(self.upem as i32 / 2))
                .unwrap_or_default();
        }
        if !coords.is_empty() {
            if let Some(hvar) = self.hvar.as_ref() {
                for (info, pos) in infos.iter().zip(pos.iter_mut()) {
                    self.control.step(1)?;
                    pos.x_advance += self.hvar_width(hvar, info.as_glyph(), coords)?.to_i32();
                }
            } else {
                for (info, pos) in infos.iter().zip(pos.iter_mut()) {
                    self.control.step(1)?;
                    if let Some(deltas) = self.try_phantom_deltas(info.as_glyph(), coords)? {
                        pos.x_advance += deltas[1].x.to_i32() - deltas[0].x.to_i32();
                    }
                }
            }
        }
        for pos in pos.iter_mut() {
            self.control.step(1)?;
            pos.x_advance = scale.scale_x(pos.x_advance);
        }
        self.control.step(0)
    }

    pub(crate) fn advance_height(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Option<i32> {
        match self.try_advance_height(gid, coords) {
            Ok(value) => value,
            Err(error) => {
                self.control.record(error);
                None
            }
        }
    }

    pub(crate) fn try_advance_height(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Result<Option<i32>, Error> {
        self.control.step(1)?;
        let gid = gid.into();
        let Some(mut advance) = self
            .vmtx
            .as_ref()
            .and_then(|vmtx| vmtx.advance(gid))
            .map(|advance| advance as i32)
        else {
            return Ok(Some(self.ascent as i32 - self.descent as i32));
        };
        if !coords.is_empty() {
            if let Some(vvar) = self.vvar.as_ref() {
                advance += self.vvar_height(vvar, gid, coords)?.to_i32();
            } else if let Some(deltas) = self.try_phantom_deltas(gid, coords)? {
                advance += deltas[3].y.to_i32() - deltas[2].y.to_i32();
            }
        }
        Ok(Some(advance))
    }

    pub(crate) fn try_top_side_bearing(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Result<Option<i32>, Error> {
        self.control.step(1)?;
        let gid = gid.into();
        let mut bearing = if let Some(vmtx) = self.vmtx.as_ref() {
            vmtx.side_bearing(gid).unwrap_or_default() as i32
        } else {
            return Ok(None);
        };
        if !coords.is_empty() {
            if let Some(vvar) = self.vvar.as_ref() {
                bearing += self.vvar_tsb(vvar, gid, coords)?.to_i32();
            } else if let Some(deltas) = self.try_phantom_deltas(gid, coords)? {
                bearing += deltas[3].y.to_i32();
            }
        }
        Ok(Some(bearing))
    }

    pub(crate) fn v_origin(&self, gid: impl Into<GlyphId>, coords: &[F2Dot14]) -> Option<i32> {
        match self.try_v_origin(gid, coords) {
            Ok(value) => value,
            Err(error) => {
                self.control.record(error);
                None
            }
        }
    }

    pub(crate) fn try_v_origin(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Result<Option<i32>, Error> {
        self.control.step(1)?;
        let gid = gid.into();
        let origin = if let Some(vorg) = self.vorg.as_ref() {
            let mut origin = self.vorg_origin(vorg, gid)? as i32;
            if !coords.is_empty() {
                if let Some(vvar) = self.vvar.as_ref() {
                    origin += self.vvar_origin(vvar, gid, coords)?.to_i32();
                }
            }
            origin
        } else if let Some(extents) = self.try_bounds(gid, coords)? {
            let origin = if self.vmtx.is_some() {
                let mut origin = Some(extents.y_max);
                let tsb = self.try_top_side_bearing(gid, coords)?;
                if let Some(tsb) = tsb {
                    origin = Some(origin.unwrap() + tsb);
                } else {
                    origin = None;
                }
                if origin.is_some() && !coords.is_empty() {
                    if let Some(vvar) = self.vvar.as_ref() {
                        origin =
                            Some(origin.unwrap() + self.vvar_origin(vvar, gid, coords)?.to_i32());
                    }
                }
                origin
            } else {
                None
            };

            if let Some(origin) = origin {
                origin
            } else {
                let mut advance = self.ascent as i32 - self.descent as i32;
                if let Some(mvar) = self.mvar.as_ref() {
                    advance += self.mvar_delta(mvar, Tag::new(b"hasc"), coords)?.to_i32()
                        - self.mvar_delta(mvar, Tag::new(b"hdsc"), coords)?.to_i32();
                }
                let diff = advance - (extents.y_max - extents.y_min);
                extents.y_max + (diff >> 1)
            }
        } else {
            let mut ascent = self.ascent as i32;
            if let Some(mvar) = self.mvar.as_ref() {
                ascent += self.mvar_delta(mvar, Tag::new(b"hasc"), coords)?.to_i32();
            }
            ascent
        };
        Ok(Some(origin))
    }

    fn try_bounds(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Result<Option<BoundingBox<i32>>, Error> {
        self.control.step(1)?;
        let gid = gid.into();
        let Some(glyf) = self.glyf.as_ref() else {
            return Ok(None);
        };
        let glyph = match &self.control.0 {
            Some(r) => glyf
                .loca
                .try_get_glyf(gid, &glyf.glyf, &mut RequestControl(r))?,
            None => glyf.loca.get_glyf(gid, &glyf.glyf),
        };
        let Ok(glyph) = glyph else {
            return Ok(None);
        };
        let Some(glyph) = glyph else {
            // Return empty extents for empty glyph
            return Ok(Some(BoundingBox::default()));
        };
        if !coords.is_empty() {
            return Ok(None); // TODO https://github.com/harfbuzz/harfrust/pull/52#issuecomment-2878117808
        }
        Ok(Some(BoundingBox {
            x_min: glyph.x_min() as i32,
            y_min: glyph.y_min() as i32,
            x_max: glyph.x_max() as i32,
            y_max: glyph.y_max() as i32,
        }))
    }

    pub(crate) fn extents(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Option<GlyphExtents> {
        match self.try_extents(gid, coords) {
            Ok(value) => value,
            Err(error) => {
                self.control.record(error);
                None
            }
        }
    }

    pub(crate) fn try_extents(
        &self,
        gid: impl Into<GlyphId>,
        coords: &[F2Dot14],
    ) -> Result<Option<GlyphExtents>, Error> {
        self.control.step(1)?;
        let gid = gid.into();
        let Some(glyf) = self.glyf.as_ref() else {
            return Ok(None);
        };
        let glyph = match &self.control.0 {
            Some(r) => glyf
                .loca
                .try_get_glyf(gid, &glyf.glyf, &mut RequestControl(r))?,
            None => glyf.loca.get_glyf(gid, &glyf.glyf),
        };
        let Ok(glyph) = glyph else {
            return Ok(None);
        };
        let Some(glyph) = glyph else {
            // Return empty extents for empty glyph
            return Ok(Some(GlyphExtents::default()));
        };
        if !coords.is_empty() {
            return Ok(None); // TODO https://github.com/harfbuzz/harfrust/pull/52#issuecomment-2878117808
        }
        Ok(Some(GlyphExtents {
            x_bearing: glyph.x_min() as i32,
            y_bearing: glyph.y_max() as i32,
            width: glyph.x_max() as i32 - glyph.x_min() as i32,
            height: glyph.y_min() as i32 - glyph.y_max() as i32,
        }))
    }

    fn try_phantom_deltas(
        &self,
        gid: GlyphId,
        coords: &[F2Dot14],
    ) -> Result<Option<[Point<Fixed>; 4]>, Error> {
        self.control.step(0)?;
        let Some(glyf) = self.glyf.as_ref() else {
            return Ok(None);
        };
        let Some(gvar) = glyf.gvar.as_ref() else {
            return Ok(None);
        };
        let value = match &self.control.0 {
            Some(r) => gvar.try_phantom_point_deltas(
                &glyf.glyf,
                &glyf.loca,
                coords,
                gid,
                &mut RequestControl(r),
            )?,
            None => gvar.phantom_point_deltas(&glyf.glyf, &glyf.loca, coords, gid),
        };
        Ok(value.ok().flatten())
    }
}
