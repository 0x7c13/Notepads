// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::hb::buffer::GlyphPropsFlags;
use crate::hb::ot::coverage_get;
use crate::hb::ot_layout_gsubgpos::OT::hb_ot_apply_context_t;
use crate::hb::ot_layout_gsubgpos::{Apply, WouldApply, WouldApplyContext};
use read_fonts::tables::gsub::MultipleSubstFormat1;

impl WouldApply for MultipleSubstFormat1<'_> {
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

impl Apply for MultipleSubstFormat1<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let hb_loop_control = ctx.buffer.control();
        if let Err(error) = hb_loop_control.step(0) {
            ctx.buffer.stop_storage(error);
            return Default::default();
        }
        let gid = ctx.buffer.cur(0).as_glyph();
        let index = coverage_get(
            &font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            )
            .ok()?,
            gid,
            &control,
        )? as usize;
        let substs = font_read!(control, control.offset_get(&self.sequences(), index))
            .ok()?
            .substitute_glyph_ids();
        match substs.len() {
            // Spec disallows this, but Uniscribe allows it.
            // https://github.com/harfbuzz/harfbuzz/issues/253
            0 => ctx.buffer.delete_glyph(),

            // Special-case to make it in-place and not consider this
            // as a "multiplied" substitution.
            1 => ctx.replace_glyph(substs.first()?.get().into()),

            _ => {
                let class = if ctx.buffer.cur(0).is_ligature() {
                    GlyphPropsFlags::BASE_GLYPH
                } else {
                    GlyphPropsFlags::empty()
                };
                let lig_id = ctx.buffer.cur(0).lig_id();

                for (i, subst) in substs.iter().enumerate() {
                    if let Err(error) = hb_loop_control.step(1) {
                        ctx.buffer.stop_storage(error);
                        return Default::default();
                    }
                    let subst = subst.get().into();
                    // If is attached to a ligature, don't disturb that.
                    // https://github.com/harfbuzz/harfbuzz/issues/3069
                    if lig_id == 0 {
                        // Index is truncated to 4 bits anway, so we can safely cast to u8.
                        ctx.buffer.cur_mut(0).set_lig_props_for_component(i as u8);
                    }
                    ctx.output_glyph_for_component(subst, class);
                }

                ctx.buffer.skip_glyph();
            }
        }
        Some(())
    }
}
