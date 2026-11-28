// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Borrowed request bitmap views. Mask bytes cannot escape as an ordinary
//! decoder with its separate allocation and pixel traversal behavior.

use crate::{
    bitmap::{self, BitmapFormat, Origin, StrikesData},
    instance::Size,
    FontRef, GlyphId,
};
use markdown_svg_budget::{Error, RequestControl, ResourcesRef};

/// Bitmap strikes using the caller's original request for every deep read.
pub struct BitmapStrikes<'f, 'r> {
    data: StrikesData<'f>,
    resources: &'r ResourcesRef,
}

impl<'f, 'r> BitmapStrikes<'f, 'r> {
    /// Missing and malformed font tables retain the original empty collection.
    pub fn try_new(font: &FontRef<'f>, resources: &'r ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let data = StrikesData::try_new(font, &mut RequestControl(resources))?;
        resources.checkpoint(0)?;
        Ok(Self { data, resources })
    }

    pub fn format(&self) -> Option<BitmapFormat> {
        self.data.format()
    }

    /// Select using the ordinary exact/nearest larger/nearest smaller rule.
    pub fn try_glyph_for_size(
        &self,
        size: Size,
        glyph_id: GlyphId,
    ) -> Result<Option<BitmapGlyph<'f>>, Error> {
        self.resources.checkpoint(0)?;
        let glyph =
            self.data
                .try_glyph_for_size(size, glyph_id, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(glyph.map(BitmapGlyph::from_borrowed))
    }
}

/// Original numeric placement fields, copied without scaling or normalization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BitmapMetadata {
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub inner_bearing_x: f32,
    pub inner_bearing_y: f32,
    pub ppem_x: f32,
    pub ppem_y: f32,
    pub advance: Option<f32>,
    pub width: u32,
    pub height: u32,
    pub placement_origin: Origin,
}

/// Borrowed bytes and format information only. This view has no decode API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapDataRef<'f> {
    Png(&'f [u8]),
    Bgra(&'f [u8]),
    Mask {
        bpp: u8,
        is_packed: bool,
        data: &'f [u8],
    },
}

/// An immutable bitmap result with no ordinary decoder or collection escape.
pub struct BitmapGlyph<'f> {
    data: BitmapDataRef<'f>,
    metadata: BitmapMetadata,
}

impl<'f> BitmapGlyph<'f> {
    pub fn data(&self) -> BitmapDataRef<'f> {
        self.data
    }

    pub fn metadata(&self) -> BitmapMetadata {
        self.metadata
    }

    fn from_borrowed(glyph: bitmap::BitmapGlyph<'f>) -> Self {
        Self {
            data: match glyph.data {
                bitmap::BitmapData::Png(bytes) => BitmapDataRef::Png(bytes),
                bitmap::BitmapData::Bgra(bytes) => BitmapDataRef::Bgra(bytes),
                bitmap::BitmapData::Mask(mask) => BitmapDataRef::Mask {
                    bpp: mask.bpp,
                    is_packed: mask.is_packed,
                    data: mask.data,
                },
            },
            metadata: BitmapMetadata {
                bearing_x: glyph.bearing_x,
                bearing_y: glyph.bearing_y,
                inner_bearing_x: glyph.inner_bearing_x,
                inner_bearing_y: glyph.inner_bearing_y,
                ppem_x: glyph.ppem_x,
                ppem_y: glyph.ppem_y,
                advance: glyph.advance,
                width: glyph.width,
                height: glyph.height,
                placement_origin: glyph.placement_origin,
            },
        }
    }
}

#[cfg(test)]
#[path = "request_bitmap_tests.rs"]
mod tests;
