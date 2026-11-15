// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! The [VVAR (Vertical Metrics Variation)](https://docs.microsoft.com/en-us/typography/opentype/spec/vvar) table

use super::variations::{self, DeltaSetIndexMap, ItemVariationStore};

include!("../../generated/generated_vvar.rs");

impl Vvar<'_> {
    /// Returns the advance height delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn advance_height_delta(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
    ) -> Result<Fixed, ReadError> {
        variations::advance_delta(
            self.advance_height_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }
    pub fn try_advance_height_delta<C: markdown_svg_budget::WorkControl>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        cx: &mut C,
    ) -> crate::control::ReadCtl<Fixed, C> {
        let mapping = self.try_advance_height_mapping(cx)?;
        let store = self.try_item_variation_store(cx)?;
        variations::try_advance_delta(mapping, store, glyph_id, coords, cx)
    }

    /// Returns the top side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn tsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        variations::item_delta(
            self.tsb_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }
    pub fn try_tsb_delta<C: markdown_svg_budget::WorkControl>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        cx: &mut C,
    ) -> crate::control::ReadCtl<Fixed, C> {
        let mapping = self.try_tsb_mapping(cx)?;
        let store = self.try_item_variation_store(cx)?;
        variations::try_item_delta(mapping, store, glyph_id, coords, cx)
    }

    /// Returns the bottom side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn bsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        variations::item_delta(
            self.bsb_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }
    pub fn try_bsb_delta<C: markdown_svg_budget::WorkControl>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        cx: &mut C,
    ) -> crate::control::ReadCtl<Fixed, C> {
        let mapping = self.try_bsb_mapping(cx)?;
        let store = self.try_item_variation_store(cx)?;
        variations::try_item_delta(mapping, store, glyph_id, coords, cx)
    }

    /// Returns the vertical origin delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn v_org_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        variations::item_delta(
            self.v_org_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }
    pub fn try_v_org_delta<C: markdown_svg_budget::WorkControl>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        cx: &mut C,
    ) -> crate::control::ReadCtl<Fixed, C> {
        let mapping = self.try_v_org_mapping(cx)?;
        let store = self.try_item_variation_store(cx)?;
        variations::try_item_delta(mapping, store, glyph_id, coords, cx)
    }
}
