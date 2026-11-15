// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! The [vmtx (Vertical Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/vmtx) table

use super::hmtx;
pub use super::hmtx::LongMetric;

include!("../../generated/generated_vmtx.rs");

impl Vmtx<'_> {
    /// Returns the advance height for the given glyph identifier.
    pub fn advance(&self, glyph_id: GlyphId) -> Option<u16> {
        crate::control::ordinary(
            self.try_advance(glyph_id, &mut crate::control::OrdinaryControl::new()),
        )
    }
    pub fn try_advance<C: markdown_svg_budget::WorkControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<Option<u16>, C::Error> {
        cx.checkpoint(1)?;
        let result = self.advance_scalar(glyph_id);
        cx.checkpoint(0)?;
        Ok(result)
    }
    fn advance_scalar(&self, glyph_id: GlyphId) -> Option<u16> {
        hmtx::advance(self.v_metrics(), glyph_id)
    }

    /// Returns the top side bearing for the given glyph identifier.
    pub fn side_bearing(&self, glyph_id: GlyphId) -> Option<i16> {
        crate::control::ordinary(
            self.try_side_bearing(glyph_id, &mut crate::control::OrdinaryControl::new()),
        )
    }
    pub fn try_side_bearing<C: markdown_svg_budget::WorkControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<Option<i16>, C::Error> {
        cx.checkpoint(1)?;
        let result = self.side_bearing_scalar(glyph_id);
        cx.checkpoint(0)?;
        Ok(result)
    }
    fn side_bearing_scalar(&self, glyph_id: GlyphId) -> Option<i16> {
        hmtx::side_bearing(self.v_metrics(), self.top_side_bearings(), glyph_id)
    }
}
