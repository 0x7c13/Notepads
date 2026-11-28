// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! The [COLR](https://docs.microsoft.com/en-us/typography/opentype/spec/colr) table

#[cfg(feature = "std")]
mod closure;
use super::variations::{DeltaSetIndexMap, ItemVariationStore};
use crate::control::{self, ReadCtl};
use markdown_svg_budget::WorkControl;
include!("../../generated/generated_colr.rs");

/// Unique paint identifier used for detecting cycles in the paint graph.
pub type PaintId = usize;

// The fixed std binary search has ceil(log2(n))+1 scalar comparisons.
fn search_work(n: usize) -> u64 {
    if n == 0 {
        0
    } else {
        (usize::BITS - (n - 1).leading_zeros()) as u64 + 1
    }
}
impl<'a> Colr<'a> {
    /// Returns the COLRv0 base glyph's original layer range, if present.
    pub fn v0_base_glyph(&self, glyph_id: GlyphId) -> Result<Option<Range<usize>>, ReadError> {
        control::ordinary(self.try_v0_base_glyph(glyph_id, &mut control::OrdinaryControl::new()))
    }
    pub fn try_v0_base_glyph<C: WorkControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> ReadCtl<Option<Range<usize>>, C> {
        cx.checkpoint(0)?;
        let Some(records) = self.try_base_glyph_records(cx)? else {
            return Ok(Err(ReadError::NullOffset));
        };
        let records = control::font_try!(records);
        let Ok(glyph_id) = glyph_id.try_into() else {
            return Ok(Ok(None));
        };
        cx.checkpoint(search_work(records.len()))?;
        let index = records.binary_search_by(|rec| rec.glyph_id().cmp(&glyph_id));
        cx.checkpoint(0)?;
        let Ok(index) = index else {
            return Ok(Ok(None));
        };
        let record = &records[index];
        let start = record.first_layer_index() as usize;
        Ok(Ok(Some(start..start + record.num_layers() as usize)))
    }
    /// Returns the glyph identifier and palette index of a COLRv0 layer.
    pub fn v0_layer(&self, index: usize) -> Result<(GlyphId16, u16), ReadError> {
        control::ordinary(self.try_v0_layer(index, &mut control::OrdinaryControl::new()))
    }
    pub fn try_v0_layer<C: WorkControl>(
        &self,
        index: usize,
        cx: &mut C,
    ) -> ReadCtl<(GlyphId16, u16), C> {
        cx.checkpoint(1)?;
        let Some(layers) = self.try_layer_records(cx)? else {
            return Ok(Err(ReadError::NullOffset));
        };
        let layers = control::font_try!(layers);
        let layer = control::font_try!(layers.get(index).ok_or(ReadError::OutOfBounds));
        Ok(Ok((layer.glyph_id(), layer.palette_index())))
    }
    /// Returns the COLRv1 root paint and its original font-address identity.
    pub fn v1_base_glyph(
        &self,
        glyph_id: GlyphId,
    ) -> Result<Option<(Paint<'a>, PaintId)>, ReadError> {
        control::ordinary(self.try_v1_base_glyph(glyph_id, &mut control::OrdinaryControl::new()))
    }
    pub fn try_v1_base_glyph<C: WorkControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> ReadCtl<Option<(Paint<'a>, PaintId)>, C> {
        cx.checkpoint(0)?;
        let Ok(glyph_id) = glyph_id.try_into() else {
            return Ok(Ok(None));
        };
        let Some(list) = self.try_base_glyph_list(cx)? else {
            return Ok(Err(ReadError::NullOffset));
        };
        let list = control::font_try!(list);
        let records = list.base_glyph_paint_records();
        cx.checkpoint(search_work(records.len()))?;
        let index = records.binary_search_by(|rec| rec.glyph_id().cmp(&glyph_id));
        cx.checkpoint(0)?;
        let Ok(index) = index else {
            return Ok(Ok(None));
        };
        let record = &records[index];
        let data = list.offset_data();
        let id = control::font_try!((data.as_ref().as_ptr() as usize)
            .checked_add(record.paint_offset().to_u32() as usize)
            .ok_or(ReadError::OutOfBounds));
        let paint = control::font_try!(record.try_paint(data, cx)?);
        Ok(Ok(Some((paint, id))))
    }
    /// Returns a COLRv1 layer paint and its font-address identity.
    pub fn v1_layer(&self, index: usize) -> Result<(Paint<'a>, PaintId), ReadError> {
        control::ordinary(self.try_v1_layer(index, &mut control::OrdinaryControl::new()))
    }
    pub fn try_v1_layer<C: WorkControl>(
        &self,
        index: usize,
        cx: &mut C,
    ) -> ReadCtl<(Paint<'a>, PaintId), C> {
        cx.checkpoint(1)?;
        let Some(list) = self.try_layer_list(cx)? else {
            return Ok(Err(ReadError::NullOffset));
        };
        let list = control::font_try!(list);
        let offset = control::font_try!(list
            .paint_offsets()
            .get(index)
            .ok_or(ReadError::OutOfBounds))
        .get();
        let data = list.offset_data();
        let id = control::font_try!((data.as_ref().as_ptr() as usize)
            .checked_add(offset.to_u32() as usize)
            .ok_or(ReadError::OutOfBounds));
        let paint = control::font_try!(offset.try_resolve(data, cx)?);
        Ok(Ok((paint, id)))
    }
    /// Returns a COLRv1 clip box for the glyph, if present.
    pub fn v1_clip_box(&self, glyph_id: GlyphId) -> Result<Option<ClipBox<'a>>, ReadError> {
        control::ordinary(self.try_v1_clip_box(glyph_id, &mut control::OrdinaryControl::new()))
    }
    pub fn try_v1_clip_box<C: WorkControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> ReadCtl<Option<ClipBox<'a>>, C> {
        use core::cmp::Ordering;
        cx.checkpoint(0)?;
        let Ok(glyph_id): Result<GlyphId16, _> = glyph_id.try_into() else {
            return Ok(Ok(None));
        };
        let Some(list) = self.try_clip_list(cx)? else {
            return Ok(Err(ReadError::NullOffset));
        };
        let list = control::font_try!(list);
        let clips = list.clips();
        cx.checkpoint(search_work(clips.len()))?;
        let index = clips.binary_search_by(|clip| {
            if glyph_id < clip.start_glyph_id() {
                Ordering::Greater
            } else if glyph_id > clip.end_glyph_id() {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        });
        cx.checkpoint(0)?;
        let Ok(index) = index else {
            return Ok(Ok(None));
        };
        Ok(clips[index].try_clip_box(list.offset_data(), cx)?.map(Some))
    }
}

#[cfg(test)]
#[path = "representation_tests.rs"]
mod representation_tests;

#[cfg(test)]
#[path = "representation_range_tests.rs"]
mod representation_range_tests;
