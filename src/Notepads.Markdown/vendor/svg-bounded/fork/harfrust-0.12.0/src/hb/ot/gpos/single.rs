// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::hb::ot::coverage_get;
use crate::hb::ot_layout_gsubgpos::Apply;
use crate::hb::ot_layout_gsubgpos::OT::hb_ot_apply_context_t;
use read_fonts::tables::gpos::{SinglePosFormat1, SinglePosFormat2};

impl Apply for SinglePosFormat1<'_> {
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
        let format = self.value_format();
        let offset = self.value_record_byte_range().start;
        super::apply_value(ctx, ctx.buffer.idx, &self.offset_data(), offset, format);
        ctx.buffer.idx += 1;
        Some(())
    }
}

impl Apply for SinglePosFormat2<'_> {
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
        let format = self.value_format();
        let offset = self.value_records_byte_range().start + (format.record_byte_len() * index);
        super::apply_value(ctx, ctx.buffer.idx, &self.offset_data(), offset, format);
        ctx.buffer.idx += 1;
        Some(())
    }
}
