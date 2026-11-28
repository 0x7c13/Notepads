// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! The [sbix (Standard Bitmap Graphics)](https://docs.microsoft.com/en-us/typography/opentype/spec/sbix) table

include!("../../generated/generated_sbix.rs");

impl<'a> Strike<'a> {
    pub fn glyph_data(&self, glyph_id: GlyphId) -> Result<Option<GlyphData<'a>>, ReadError> {
        crate::control::ordinary(
            self.try_glyph_data(glyph_id, &mut crate::control::OrdinaryControl::new()),
        )
    }
    pub fn try_glyph_data<C: markdown_svg_budget::WorkControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> crate::control::ReadCtl<Option<GlyphData<'a>>, C> {
        cx.checkpoint(2)?;
        let offsets = self.glyph_data_offsets();
        let start_ix = glyph_id.to_u32() as usize;
        let start = crate::control::font_try!(offsets.get(start_ix).ok_or(ReadError::OutOfBounds))
            .get() as usize;
        let end_ix =
            crate::control::font_try!(start_ix.checked_add(1).ok_or(ReadError::OutOfBounds));
        let end = crate::control::font_try!(offsets.get(end_ix).ok_or(ReadError::OutOfBounds)).get()
            as usize;
        if start == end {
            return Ok(Ok(None));
        }
        let data = crate::control::font_try!(self
            .offset_data()
            .slice(start..end)
            .ok_or(ReadError::OutOfBounds));
        Ok(<GlyphData<'a> as crate::control::TryFontRead<'a>>::try_read(data, cx)?.map(Some))
    }
}

#[cfg(test)]
mod tests {
    use font_test_data::bebuffer::BeBuffer;

    use crate::tables::sbix::Sbix;

    #[test]
    fn sbix_strikes_count_overflow_table() {
        // Contains an invalid `num_strikes` values which would move the cursor outside the table.
        // See https://issues.chromium.org/issues/347835680 for the ClusterFuzz report.
        // Failure only reproduces on 32-bit, for example, run with:
        // cargo test --target=i686-unknown-linux-gnu "sbix_strikes_count_overflow_table"
        let sbix = BeBuffer::new()
            .push(1u16) // version
            .push(0u16) // flags
            .push(u32::MAX); // num_strikes

        let table = Sbix::read(sbix.data().into(), 5).unwrap();
        assert_eq!(table.num_strikes(), u32::MAX);
        // Must not panic with "attempt to multiply with overflow".
        assert_eq!(table.strike_offsets().len(), 0);
    }
}
