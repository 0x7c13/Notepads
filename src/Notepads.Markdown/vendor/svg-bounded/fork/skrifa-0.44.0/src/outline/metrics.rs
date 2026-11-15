//! Helper for loading (possibly variable) horizontal glyph metrics.
// Modifications copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::control::{self, Ordinary};
use markdown_svg_budget::WorkControl;
use read_fonts::control::TryFontReadWithArgs;

use raw::{
    tables::{hmtx::Hmtx, hvar::Hvar},
    types::{F2Dot14, GlyphId},
    FontRef,
};

/// Access to horizontal glyph metrics.
#[derive(Clone)]
pub(crate) struct GlyphHMetrics<'a> {
    pub hmtx: Hmtx<'a>,
    pub hvar: Option<Hvar<'a>>,
}

impl<'a> GlyphHMetrics<'a> {
    pub(crate) fn try_new<C: WorkControl>(
        font: &FontRef<'a>,
        cx: &mut C,
    ) -> Result<Option<Self>, C::Error> {
        // Note: hmtx is required and HVAR is optional
        cx.checkpoint(0)?;
        let hhea = match control::table::<raw::tables::hhea::Hhea, _>(font, cx)? {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let data = match control::data(font, raw::types::Tag::new(b"hmtx"), cx)? {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let hmtx = match Hmtx::try_read_with_args(data, &hhea.number_of_h_metrics(), cx)? {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let hvar = control::table::<Hvar, _>(font, cx)?.ok();
        cx.checkpoint(0)?;
        Ok(Some(Self { hmtx, hvar }))
    }

    /// Returns the advance width (in font units) for the given glyph and
    /// the location in variation space represented by the set of normalized
    /// coordinates in 2.14 fixed point.
    pub fn advance_width(&self, gid: GlyphId, coords: &'a [F2Dot14]) -> i32 {
        control::ordinary(self.try_advance_width(gid, coords, &mut Ordinary::new()))
    }

    pub(crate) fn try_advance_width<C: WorkControl>(
        &self,
        gid: GlyphId,
        coords: &[F2Dot14],
        cx: &mut C,
    ) -> Result<i32, C::Error> {
        cx.checkpoint(0)?;
        let mut advance = self.hmtx.try_advance(gid, cx)?.unwrap_or_default() as i32;
        if let (false, Some(hvar)) = (coords.is_empty(), &self.hvar) {
            advance += hvar
                .try_advance_width_delta(gid, coords, cx)?
                .map(|delta| delta.to_i32())
                .unwrap_or(0);
        }
        cx.checkpoint(0)?;
        Ok(advance)
    }

    /// Returns the left side bearing (in font units) for the given glyph and
    /// the location in variation space represented by the set of normalized
    /// coordinates in 2.14 fixed point.    

    pub(crate) fn try_lsb<C: WorkControl>(
        &self,
        gid: GlyphId,
        coords: &[F2Dot14],
        cx: &mut C,
    ) -> Result<i32, C::Error> {
        cx.checkpoint(0)?;
        let mut lsb = self.hmtx.try_side_bearing(gid, cx)?.unwrap_or_default() as i32;
        if let (false, Some(hvar)) = (coords.is_empty(), &self.hvar) {
            lsb += hvar
                .try_lsb_delta(gid, coords, cx)?
                .map(|delta| delta.to_i32())
                .unwrap_or(0);
        }
        cx.checkpoint(0)?;
        Ok(lsb)
    }
}
