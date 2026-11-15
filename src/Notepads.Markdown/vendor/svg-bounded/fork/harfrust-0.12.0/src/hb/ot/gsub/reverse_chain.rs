// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::hb::buffer::GlyphInfo;
use crate::hb::ot::coverage_get;
use crate::hb::ot_layout::MAX_NESTING_LEVEL;
use crate::hb::ot_layout_gsubgpos::OT::hb_ot_apply_context_t;
use crate::hb::ot_layout_gsubgpos::{
    match_backtrack, match_lookahead, Apply, WouldApply, WouldApplyContext,
};
use read_fonts::tables::gsub::ReverseChainSingleSubstFormat1;

impl WouldApply for ReverseChainSingleSubstFormat1<'_> {
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
            .ok()
            .and_then(|coverage| coverage_get(&coverage, ctx.glyphs[0], &control))
            .is_some()
    }
}

impl Apply for ReverseChainSingleSubstFormat1<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        // No chaining to this type.
        if ctx.nesting_level_left != MAX_NESTING_LEVEL {
            return None;
        }

        let glyph = ctx.buffer.cur(0).as_glyph();
        let coverage = font_read!(
            control,
            control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                self.coverage_offset(),
                self.offset_data()
            )
        )
        .ok()?;
        let index = coverage_get(&coverage, glyph, &control)? as usize;
        let substitutes = self.substitute_glyph_ids();
        if index >= substitutes.len() {
            return None;
        }

        let subst = substitutes.get(index)?.get();

        let backtrack_coverages = self.backtrack_coverages();
        let lookahead_coverages = self.lookahead_coverages();

        let f1 = |info: &mut GlyphInfo, index| {
            font_read!(
                control,
                control.offset_get(&backtrack_coverages, index as usize)
            )
            .ok()
            .and_then(|coverage| coverage_get(&coverage, info.glyph_id, &control))
            .is_some()
        };

        let f2 = |info: &mut GlyphInfo, index| {
            font_read!(
                control,
                control.offset_get(&lookahead_coverages, index as usize)
            )
            .ok()
            .and_then(|coverage| coverage_get(&coverage, info.glyph_id, &control))
            .is_some()
        };

        let mut start_index = 0;
        let mut end_index = 0;

        if match_backtrack(ctx, backtrack_coverages.len() as u16, f1, &mut start_index) {
            if match_lookahead(
                ctx,
                lookahead_coverages.len() as u16,
                f2,
                ctx.buffer.idx + 1,
                &mut end_index,
            ) {
                ctx.buffer
                    .unsafe_to_break_from_outbuffer(Some(start_index), Some(end_index));
                ctx.replace_glyph_inplace(subst.into());

                // Note: We DON'T decrease buffer.idx.  The main loop does it
                // for us.  This is useful for preventing surprises if someone
                // calls us through a Context lookup.
                return Some(());
            }
        }

        ctx.buffer
            .unsafe_to_concat_from_outbuffer(Some(start_index), Some(end_index));
        None
    }
}
