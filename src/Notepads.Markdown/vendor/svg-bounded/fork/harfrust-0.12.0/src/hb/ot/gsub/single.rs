// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::hb::ot::coverage_get;
use crate::hb::ot_layout_gsubgpos::OT::hb_ot_apply_context_t;
use crate::hb::ot_layout_gsubgpos::{Apply, WouldApply, WouldApplyContext};
use read_fonts::tables::gsub::{SingleSubstFormat1, SingleSubstFormat2};

impl WouldApply for SingleSubstFormat1<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let gid = ctx.glyphs[0];
        ctx.glyphs.len() == 1
            && font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            )
            .is_ok_and(|cov| coverage_get(&cov, gid, &control).is_some())
    }
}

impl Apply for SingleSubstFormat1<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        coverage_get(
            &font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            )
            .ok()?,
            glyph,
            &control,
        )?;
        let subst = (glyph.to_u32() as i32 + self.delta_glyph_id() as i32) as u16;
        ctx.replace_glyph(subst.into());
        Some(())
    }
}

impl WouldApply for SingleSubstFormat2<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        ctx.glyphs.len() == 1
            && font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            )
            .is_ok_and(|cov| coverage_get(&cov, ctx.glyphs[0], &control).is_some())
    }
}

impl Apply for SingleSubstFormat2<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let index = coverage_get(
            &font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            )
            .ok()?,
            glyph,
            &control,
        )? as usize;
        let subst = self.substitute_glyph_ids().get(index)?.get().to_u16();
        ctx.replace_glyph(subst.into());
        Some(())
    }
}
