//! Bitmap strikes and glyphs.
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use alloc::vec::Vec;

use super::{instance::Size, metrics::GlyphMetricsData};
use crate::control::{self, FontControl, Ordinary};
use crate::prelude::LocationRef;
use raw::{
    tables::{bitmap, cbdt, cblc, ebdt, eblc, sbix},
    types::{GlyphId, Tag},
    FontData, FontRef,
};

/// Set of strikes, each containing embedded bitmaps of a single size.
#[derive(Clone)]
pub struct BitmapStrikes<'a>(StrikesData<'a>);

impl<'a> BitmapStrikes<'a> {
    /// Creates a new `BitmapStrikes` for the given font.
    ///
    /// This will prefer `sbix`, `CBDT`, and `CBLC` formats in that order.
    ///
    /// To select a specific format, use [`with_format`](Self::with_format).
    pub fn new(font: &FontRef<'a>) -> Self {
        Self(control::ordinary(StrikesData::try_new(
            font,
            &mut Ordinary::new(),
        )))
    }

    /// Creates a new `BitmapStrikes` for the given font and format.
    ///
    /// Returns `None` if the requested format is not available.
    pub fn with_format(font: &FontRef<'a>, format: BitmapFormat) -> Option<Self> {
        control::ordinary(StrikesData::try_with_format(
            font,
            format,
            &mut Ordinary::new(),
        ))
        .map(Self)
    }

    /// Returns the format representing the underlying table for this set of strikes.
    pub fn format(&self) -> Option<BitmapFormat> {
        self.0.format()
    }

    /// Returns the number of available strikes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if there are no available strikes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the strike at the given index.
    pub fn get(&self, index: usize) -> Option<BitmapStrike<'a>> {
        control::ordinary(self.0.try_get(index, &mut Ordinary::new())).map(BitmapStrike)
    }

    /// Returns the best matching glyph for the given size and glyph identifier.
    ///
    /// In this case, "best" means a glyph of the exact size, nearest larger
    /// size, or nearest smaller size, in that order.
    pub fn glyph_for_size(&self, size: Size, glyph_id: GlyphId) -> Option<BitmapGlyph<'a>> {
        control::ordinary(
            self.0
                .try_glyph_for_size(size, glyph_id, &mut Ordinary::new()),
        )
    }

    /// Returns an iterator over all available strikes.
    pub fn iter(&self) -> impl Iterator<Item = BitmapStrike<'a>> + 'a + Clone {
        let this = self.clone();
        (0..this.len()).filter_map(move |ix| this.get(ix))
    }
}

/// Shared borrowed representation. The request facade never stores an ordinary
/// metrics wrapper or offers a conversion to the ordinary bitmap collection.
#[derive(Clone)]
pub(crate) enum StrikesData<'a> {
    None,
    Sbix(sbix::Sbix<'a>, GlyphMetricsData<'a>),
    Cbdt(CbdtTables<'a>),
    Ebdt(EbdtTables<'a>),
}

impl<'a> StrikesData<'a> {
    pub(crate) fn try_new<C: FontControl>(
        font: &FontRef<'a>,
        cx: &mut C,
    ) -> Result<Self, C::Error> {
        cx.checkpoint(0)?;
        for format in [BitmapFormat::Sbix, BitmapFormat::Cbdt, BitmapFormat::Ebdt] {
            if let Some(strikes) = Self::try_with_format(font, format, cx)? {
                return Ok(strikes);
            }
        }
        cx.checkpoint(0)?;
        Ok(Self::None)
    }

    fn try_with_format<C: FontControl>(
        font: &FontRef<'a>,
        format: BitmapFormat,
        cx: &mut C,
    ) -> Result<Option<Self>, C::Error> {
        cx.checkpoint(1)?;
        let value = match format {
            BitmapFormat::Sbix => {
                let Ok(sbix) = control::sbix(font, cx)? else {
                    return Ok(None);
                };
                let (metrics, _) =
                    GlyphMetricsData::try_new(font, Size::unscaled(), LocationRef::default(), cx)?;
                Self::Sbix(sbix, metrics)
            }
            BitmapFormat::Cbdt => {
                let Ok(location) = control::table::<cblc::Cblc, _>(font, cx)? else {
                    return Ok(None);
                };
                let Ok(data) = control::table::<cbdt::Cbdt, _>(font, cx)? else {
                    return Ok(None);
                };
                Self::Cbdt(CbdtTables::new(location, data))
            }
            BitmapFormat::Ebdt => {
                let Ok(location) = control::table::<eblc::Eblc, _>(font, cx)? else {
                    return Ok(None);
                };
                let Ok(data) = control::table::<ebdt::Ebdt, _>(font, cx)? else {
                    return Ok(None);
                };
                Self::Ebdt(EbdtTables::new(location, data))
            }
        };
        cx.checkpoint(0)?;
        Ok(Some(value))
    }

    pub(crate) fn format(&self) -> Option<BitmapFormat> {
        match self {
            Self::None => None,
            Self::Sbix(..) => Some(BitmapFormat::Sbix),
            Self::Cbdt(..) => Some(BitmapFormat::Cbdt),
            Self::Ebdt(..) => Some(BitmapFormat::Ebdt),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Sbix(sbix, _) => sbix.strikes().len(),
            Self::Cbdt(cbdt) => cbdt.location.bitmap_sizes().len(),
            Self::Ebdt(ebdt) => ebdt.location.bitmap_sizes().len(),
        }
    }

    fn try_get<C: FontControl>(
        &self,
        index: usize,
        cx: &mut C,
    ) -> Result<Option<StrikeData<'a>>, C::Error> {
        // Charge every attempted index, including malformed strikes that the
        // original filter_map skips before yielding its next successful item.
        cx.checkpoint(1)?;
        let value = match self {
            Self::None => return Ok(None),
            Self::Sbix(sbix, metrics) => {
                let Ok(strike) = sbix.strikes().try_get(index, cx)? else {
                    return Ok(None);
                };
                StrikeData::Sbix(strike, metrics.clone())
            }
            Self::Cbdt(tables) => {
                let Some(size) = tables.location.bitmap_sizes().get(index).copied() else {
                    return Ok(None);
                };
                StrikeData::Cbdt(size, tables.clone())
            }
            Self::Ebdt(tables) => {
                let Some(size) = tables.location.bitmap_sizes().get(index).copied() else {
                    return Ok(None);
                };
                StrikeData::Ebdt(size, tables.clone())
            }
        };
        cx.checkpoint(0)?;
        Ok(Some(value))
    }

    pub(crate) fn try_glyph_for_size<C: FontControl>(
        &self,
        size: Size,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<Option<BitmapGlyph<'a>>, C::Error> {
        cx.checkpoint(0)?;
        // The original fold's exact/nearest larger/nearest smaller comparisons
        // and order are preserved, including ties and unavailable glyphs.
        let size = size.ppem().unwrap_or(f32::MAX);
        let mut best: Option<BitmapGlyph<'a>> = None;
        for index in 0..self.len() {
            let Some(entry) = self.try_get(index, cx)? else {
                continue;
            };
            let entry_size = entry.ppem();
            best = if let Some(best) = best {
                let best_size = best.ppem_y;
                if (entry_size >= size && entry_size < best_size)
                    || (best_size < size && entry_size > best_size)
                {
                    entry.try_get(glyph_id, cx)?.or(Some(best))
                } else {
                    Some(best)
                }
            } else {
                entry.try_get(glyph_id, cx)?
            };
        }
        cx.checkpoint(0)?;
        Ok(best)
    }
}

/// Set of embedded bitmap glyphs of a specific size.
#[derive(Clone)]
pub struct BitmapStrike<'a>(StrikeData<'a>);

impl<'a> BitmapStrike<'a> {
    /// Returns the pixels-per-em (size) of this strike.
    pub fn ppem(&self) -> f32 {
        self.0.ppem()
    }

    /// Returns a bitmap glyph for the given identifier, if available.
    pub fn get(&self, glyph_id: GlyphId) -> Option<BitmapGlyph<'a>> {
        control::ordinary(self.0.try_get(glyph_id, &mut Ordinary::new()))
    }
}

#[derive(Clone)]
enum StrikeData<'a> {
    Sbix(sbix::Strike<'a>, GlyphMetricsData<'a>),
    Cbdt(bitmap::BitmapSize, CbdtTables<'a>),
    Ebdt(bitmap::BitmapSize, EbdtTables<'a>),
}

impl<'a> StrikeData<'a> {
    fn ppem(&self) -> f32 {
        match self {
            Self::Sbix(sbix, _) => sbix.ppem() as f32,
            // Original Skia implementation also considers ppem_y here.
            Self::Cbdt(size, _) | Self::Ebdt(size, _) => size.ppem_y() as f32,
        }
    }

    fn try_get<C: FontControl>(
        &self,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> Result<Option<BitmapGlyph<'a>>, C::Error> {
        cx.checkpoint(1)?;
        let value = match self {
            Self::Sbix(sbix, metrics) => {
                let Some(glyph) = sbix.try_glyph_data(glyph_id, cx)?.ok().flatten() else {
                    return Ok(None);
                };
                if glyph.graphic_type() != Tag::new(b"png ") {
                    return Ok(None);
                }

                // Preserve the Skia/CoreText placement calculation rather than
                // replacing it with the different specification description:
                // https://github.com/google/skia/blob/02cd0561f4f756bf4f7b16641d8fc4c61577c765/src/ports/fontations/src/bitmap.rs#L161-L178
                let glyf_bb = metrics.try_bounds(&[], glyph_id, cx)?.unwrap_or_default();
                let lsb = metrics
                    .try_left_side_bearing(&[], glyph_id, cx)?
                    .unwrap_or_default();
                let ppem = sbix.ppem() as f32;
                let reader = FontData::new(glyph.data());
                let Ok(width) = reader.read_at::<u32>(16) else {
                    return Ok(None);
                };
                let Ok(height) = reader.read_at::<u32>(20) else {
                    return Ok(None);
                };
                BitmapGlyph {
                    data: BitmapData::Png(glyph.data()),
                    bearing_x: lsb,
                    bearing_y: glyf_bb.y_min,
                    inner_bearing_x: glyph.origin_offset_x() as f32,
                    inner_bearing_y: glyph.origin_offset_y() as f32,
                    ppem_x: ppem,
                    ppem_y: ppem,
                    width,
                    height,
                    advance: None,
                    placement_origin: Origin::BottomLeft,
                }
            }
            Self::Cbdt(size, tables) => {
                let Ok(location) =
                    size.try_location(tables.location.offset_data(), glyph_id, cx)?
                else {
                    return Ok(None);
                };
                let Ok(data) = tables.data.try_data(&location, cx)? else {
                    return Ok(None);
                };
                let Some(value) = BitmapGlyph::from_bdt(size, &data) else {
                    return Ok(None);
                };
                value
            }
            Self::Ebdt(size, tables) => {
                let Ok(location) =
                    size.try_location(tables.location.offset_data(), glyph_id, cx)?
                else {
                    return Ok(None);
                };
                let Ok(data) = tables.data.try_data(&location, cx)? else {
                    return Ok(None);
                };
                let Some(value) = BitmapGlyph::from_bdt(size, &data) else {
                    return Ok(None);
                };
                value
            }
        };
        cx.checkpoint(0)?;
        Ok(Some(value))
    }
}

#[derive(Clone)]
pub(crate) struct BdtTables<L, D> {
    location: L,
    data: D,
}

impl<L, D> BdtTables<L, D> {
    fn new(location: L, data: D) -> Self {
        Self { location, data }
    }
}

type CbdtTables<'a> = BdtTables<cblc::Cblc<'a>, cbdt::Cbdt<'a>>;
type EbdtTables<'a> = BdtTables<eblc::Eblc<'a>, ebdt::Ebdt<'a>>;

/// An embedded bitmap glyph.
#[derive(Clone)]
pub struct BitmapGlyph<'a> {
    /// The underlying data of the bitmap glyph.
    pub data: BitmapData<'a>,
    /// Outer glyph bearings in the x direction, given in font units.
    pub bearing_x: f32,
    /// Outer glyph bearings in the y direction, given in font units.
    pub bearing_y: f32,
    /// Inner glyph bearings in the x direction, given in pixels. This value should be scaled
    /// by `ppem_*` and be applied as an offset when placing the image within the bounds rectangle.
    pub inner_bearing_x: f32,
    /// Inner glyph bearings in the y direction, given in pixels. This value should be scaled
    /// by `ppem_*` and be applied as an offset when placing the image within the bounds rectangle.
    pub inner_bearing_y: f32,
    /// The assumed pixels-per-em in the x direction.
    pub ppem_x: f32,
    /// The assumed pixels-per-em in the y direction.
    pub ppem_y: f32,
    /// The horizontal advance width of the bitmap glyph in pixels, if given.
    pub advance: Option<f32>,
    /// The number of columns in the bitmap.
    pub width: u32,
    /// The number of rows in the bitmap.
    pub height: u32,
    /// The placement origin of the bitmap.
    pub placement_origin: Origin,
}

impl<'a> BitmapGlyph<'a> {
    fn from_bdt(
        bitmap_size: &bitmap::BitmapSize,
        bitmap_data: &bitmap::BitmapData<'a>,
    ) -> Option<Self> {
        let metrics = BdtMetrics::new(bitmap_data);
        let (ppem_x, ppem_y) = (bitmap_size.ppem_x() as f32, bitmap_size.ppem_y() as f32);
        let bpp = bitmap_size.bit_depth();
        let data = match bpp {
            32 => {
                match &bitmap_data.content {
                    bitmap::BitmapContent::Data(bitmap::BitmapDataFormat::Png, bytes) => {
                        BitmapData::Png(bytes)
                    }
                    // 32-bit formats are always byte aligned
                    bitmap::BitmapContent::Data(bitmap::BitmapDataFormat::ByteAligned, bytes) => {
                        BitmapData::Bgra(bytes)
                    }
                    _ => return None,
                }
            }
            1 | 2 | 4 | 8 => {
                let (data, is_packed) = match &bitmap_data.content {
                    bitmap::BitmapContent::Data(bitmap::BitmapDataFormat::ByteAligned, bytes) => {
                        (bytes, false)
                    }
                    bitmap::BitmapContent::Data(bitmap::BitmapDataFormat::BitAligned, bytes) => {
                        (bytes, true)
                    }
                    _ => return None,
                };
                BitmapData::Mask(MaskData {
                    bpp,
                    is_packed,
                    data,
                })
            }
            // All other bit depth values are invalid
            _ => return None,
        };
        Some(Self {
            data,
            bearing_x: 0.0,
            bearing_y: 0.0,
            inner_bearing_x: metrics.inner_bearing_x,
            inner_bearing_y: metrics.inner_bearing_y,
            ppem_x,
            ppem_y,
            width: metrics.width,
            height: metrics.height,
            advance: Some(metrics.advance),
            placement_origin: Origin::TopLeft,
        })
    }
}

struct BdtMetrics {
    inner_bearing_x: f32,
    inner_bearing_y: f32,
    advance: f32,
    width: u32,
    height: u32,
}

impl BdtMetrics {
    fn new(data: &bitmap::BitmapData) -> Self {
        match data.metrics {
            bitmap::BitmapMetrics::Small(metrics) => Self {
                inner_bearing_x: metrics.bearing_x() as f32,
                inner_bearing_y: metrics.bearing_y() as f32,
                advance: metrics.advance() as f32,
                width: metrics.width() as u32,
                height: metrics.height() as u32,
            },
            bitmap::BitmapMetrics::Big(metrics) => Self {
                inner_bearing_x: metrics.hori_bearing_x() as f32,
                inner_bearing_y: metrics.hori_bearing_y() as f32,
                advance: metrics.hori_advance() as f32,
                width: metrics.width() as u32,
                height: metrics.height() as u32,
            },
        }
    }
}

///The origin point for drawing a bitmap glyph.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Origin {
    /// The origin is in the top-left.
    TopLeft,
    /// The origin is in the bottom-left.
    BottomLeft,
}

/// Data content of a bitmap.
#[derive(Clone)]
pub enum BitmapData<'a> {
    /// Uncompressed 32-bit color bitmap data, pre-multiplied in BGRA order
    /// and encoded in the sRGB color space.
    Bgra(&'a [u8]),
    /// Compressed PNG bitmap data.
    Png(&'a [u8]),
    /// Data representing a single channel alpha mask.
    Mask(MaskData<'a>),
}

/// A single channel alpha mask.
#[derive(Clone)]
pub struct MaskData<'a> {
    /// Number of bits-per-pixel. Always 1, 2, 4 or 8.
    pub bpp: u8,
    /// True if each row of the data is bit-aligned. Otherwise, each row
    /// is padded to the next byte.
    pub is_packed: bool,
    /// Raw bitmap data.
    pub data: &'a [u8],
}

/// Error type returned by [`MaskData::decode`] and [`MaskData::decode_to_slice`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MaskDataDecodeError {
    /// The width and height product overflows `usize`.
    SizeOverflow,
    /// The data buffer is too small for the given dimensions and bit depth.
    InvalidDimensions,
}

impl MaskData<'_> {
    /// Decodes the raw packed bitmap data into 8-bit-per-pixel values,
    /// writing the result into the provided buffer.
    ///
    /// The buffer must be at least `width * height` bytes long. Each pixel
    /// value is scaled to the 0–255 range.
    pub fn decode_to_slice(
        &self,
        width: u32,
        height: u32,
        dst: &mut [u8],
    ) -> Result<(), MaskDataDecodeError> {
        let w = width as usize;
        let h = height as usize;
        let total_pixels = w.checked_mul(h).ok_or(MaskDataDecodeError::SizeOverflow)?;
        if total_pixels == 0 {
            return Ok(());
        }
        let bits = self.bpp as usize;
        if dst.len() < total_pixels {
            return Err(MaskDataDecodeError::InvalidDimensions);
        }
        let dst = &mut dst[..total_pixels];
        if !self.is_packed {
            // Byte-aligned: each row is padded to a byte boundary.
            let row_bytes = (w * bits).div_ceil(8);
            let expected_data_len = row_bytes
                .checked_mul(h)
                .ok_or(MaskDataDecodeError::SizeOverflow)?;
            if self.data.len() < expected_data_len {
                return Err(MaskDataDecodeError::InvalidDimensions);
            }
            let mut dst_idx = 0;
            match self.bpp {
                1 => {
                    for row in self.data.chunks(row_bytes) {
                        for x in 0..w {
                            dst[dst_idx] = ((row[x >> 3] >> (!x & 7)) & 1) * 255;
                            dst_idx += 1;
                        }
                    }
                }
                2 => {
                    for row in self.data.chunks(row_bytes) {
                        for x in 0..w {
                            dst[dst_idx] = ((row[x >> 2] >> (!(x * 2) & 6)) & 3) * 85;
                            dst_idx += 1;
                        }
                    }
                }
                4 => {
                    for row in self.data.chunks(row_bytes) {
                        for x in 0..w {
                            dst[dst_idx] = ((row[x >> 1] >> (!(x * 4) & 4)) & 15) * 17;
                            dst_idx += 1;
                        }
                    }
                }
                8 => {
                    for row in self.data.chunks(row_bytes) {
                        dst[dst_idx..dst_idx + w].copy_from_slice(&row[..w]);
                        dst_idx += w;
                    }
                }
                _ => return Err(MaskDataDecodeError::InvalidDimensions),
            }
        } else {
            // Bit-aligned: pixels are tightly packed with no row padding.
            let total_bits = total_pixels
                .checked_mul(bits)
                .ok_or(MaskDataDecodeError::SizeOverflow)?;
            let expected_data_len = total_bits.div_ceil(8);
            if self.data.len() < expected_data_len {
                return Err(MaskDataDecodeError::InvalidDimensions);
            }
            match self.bpp {
                1 => {
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        *pixel = ((self.data[x >> 3] >> (!x & 7)) & 1) * 255;
                    }
                }
                2 => {
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        *pixel = ((self.data[x >> 2] >> (!(x * 2) & 6)) & 3) * 85;
                    }
                }
                4 => {
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        *pixel = ((self.data[x >> 1] >> (!(x * 4) & 4)) & 15) * 17;
                    }
                }
                8 => {
                    dst.copy_from_slice(&self.data[..total_pixels]);
                }
                _ => return Err(MaskDataDecodeError::InvalidDimensions),
            }
        }
        Ok(())
    }

    /// Decodes the raw packed bitmap data into 8-bit-per-pixel values.
    ///
    /// Returns a `Vec<u8>` of `width * height` bytes, with each pixel
    /// value scaled to the 0–255 range.
    pub fn decode(&self, width: u32, height: u32) -> Result<Vec<u8>, MaskDataDecodeError> {
        let w = width as usize;
        let h = height as usize;
        let total_pixels = w.checked_mul(h).ok_or(MaskDataDecodeError::SizeOverflow)?;
        let mut dst = vec![0u8; total_pixels];
        self.decode_to_slice(width, height, &mut dst)?;
        Ok(dst)
    }
}

/// The format (or table) containing the data backing a set of bitmap strikes.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum BitmapFormat {
    Sbix,
    Cbdt,
    Ebdt,
}

#[cfg(test)]
mod tests {
    use crate::bitmap::{BitmapData, MaskData, MaskDataDecodeError, StrikesData};
    use crate::prelude::Size;
    use crate::{GlyphId, MetadataProvider};
    use raw::FontRef;

    #[test]
    fn cbdt_metadata() {
        let font = FontRef::new(font_test_data::CBDT).unwrap();
        let strikes = font.bitmap_strikes();

        assert!(matches!(strikes.0, StrikesData::Cbdt(_)));
        assert!(matches!(strikes.len(), 3));

        // Note that this is only `ppem_y`.
        assert!(matches!(strikes.get(0).unwrap().ppem(), 16.0));
        assert!(matches!(strikes.get(1).unwrap().ppem(), 64.0));
        assert!(matches!(strikes.get(2).unwrap().ppem(), 128.0));
    }

    #[test]
    fn cbdt_glyph_metrics() {
        let font = FontRef::new(font_test_data::CBDT).unwrap();
        let strike_0 = font.bitmap_strikes().get(0).unwrap();

        let zero = strike_0.get(GlyphId::new(0)).unwrap();
        assert_eq!(zero.width, 11);
        assert_eq!(zero.height, 13);
        assert_eq!(zero.bearing_x, 0.0);
        assert_eq!(zero.bearing_y, 0.0);
        assert_eq!(zero.inner_bearing_x, 1.0);
        assert_eq!(zero.inner_bearing_y, 13.0);
        assert_eq!(zero.advance, Some(12.0));

        let strike_1 = font.bitmap_strikes().get(1).unwrap();

        let zero = strike_1.get(GlyphId::new(2)).unwrap();
        assert_eq!(zero.width, 39);
        assert_eq!(zero.height, 52);
        assert_eq!(zero.bearing_x, 0.0);
        assert_eq!(zero.bearing_y, 0.0);
        assert_eq!(zero.inner_bearing_x, 6.0);
        assert_eq!(zero.inner_bearing_y, 52.0);
        assert_eq!(zero.advance, Some(51.0));
    }

    #[test]
    fn cbdt_glyph_selection() {
        let font = FontRef::new(font_test_data::CBDT).unwrap();
        let strikes = font.bitmap_strikes();

        let g1 = strikes
            .glyph_for_size(Size::new(12.0), GlyphId::new(2))
            .unwrap();
        assert_eq!(g1.ppem_x, 16.0);

        let g2 = strikes
            .glyph_for_size(Size::new(17.0), GlyphId::new(2))
            .unwrap();
        assert_eq!(g2.ppem_x, 64.0);

        let g3 = strikes
            .glyph_for_size(Size::new(60.0), GlyphId::new(2))
            .unwrap();
        assert_eq!(g3.ppem_x, 64.0);

        let g4 = strikes
            .glyph_for_size(Size::unscaled(), GlyphId::new(2))
            .unwrap();
        assert_eq!(g4.ppem_x, 128.0);
    }

    #[test]
    fn sbix_metadata() {
        let font = FontRef::new(font_test_data::NOTO_HANDWRITING_SBIX).unwrap();
        let strikes = font.bitmap_strikes();

        assert!(matches!(strikes.0, StrikesData::Sbix(_, _)));
        assert!(matches!(strikes.len(), 1));

        assert!(matches!(strikes.get(0).unwrap().ppem(), 109.0));
    }

    #[test]
    fn sbix_glyph_metrics() {
        let font = FontRef::new(font_test_data::NOTO_HANDWRITING_SBIX).unwrap();
        let strike_0 = font.bitmap_strikes().get(0).unwrap();

        let g0 = strike_0.get(GlyphId::new(7)).unwrap();
        // `bearing_x` is always the lsb, which is 0 for this glyph.
        assert_eq!(g0.bearing_x, 0.0);
        // The glyph doesn't have an associated outline, so `bbox.min_y` is 0, and thus bearing_y
        // should also be 0.

        assert_eq!(g0.bearing_y, 0.0);
        // Origin offsets are 4.0 and -27.0 respectively.
        assert_eq!(g0.inner_bearing_x, 4.0);
        assert_eq!(g0.inner_bearing_y, -27.0);
        assert!(matches!(g0.data, BitmapData::Png(_)))
    }

    #[test]
    fn decode_1bpp_non_packed() {
        // 4×2 image, 1 bpp, byte-aligned (each row padded to 1 byte).
        // Row 0: pixels [1,0,1,0] → 0b1010_0000 = 0xA0
        // Row 1: pixels [0,1,0,1] → 0b0101_0000 = 0x50
        let mask = MaskData {
            bpp: 1,
            is_packed: false,
            data: &[0xA0, 0x50],
        };
        let decoded = mask.decode(4, 2).unwrap();
        assert_eq!(decoded, [255, 0, 255, 0, 0, 255, 0, 255],);
    }

    #[test]
    fn decode_2bpp_non_packed() {
        // 4×1 image, 2 bpp, byte-aligned.
        // Pixels [3, 2, 1, 0] → 0b11_10_01_00 = 0xE4
        // Scaled by 85: [255, 170, 85, 0]
        let mask = MaskData {
            bpp: 2,
            is_packed: false,
            data: &[0xE4],
        };
        let decoded = mask.decode(4, 1).unwrap();
        assert_eq!(decoded, [255, 170, 85, 0]);
    }

    #[test]
    fn decode_4bpp_non_packed() {
        // 3×2 image, 4 bpp, byte-aligned.
        // row_bytes = ceil(3*4 / 8) = 2
        // Row 0: pixels [15, 8, 4]
        //   byte 0: (15 << 4) | 8 = 0xF8
        //   byte 1: (4  << 4) | 0 = 0x40  (low nibble is padding)
        // Row 1: pixels [0, 5, 10]
        //   byte 0: (0  << 4) | 5 = 0x05
        //   byte 1: (10 << 4) | 0 = 0xA0
        // Scaled by 17: [255, 136, 68, 0, 85, 170]
        let mask = MaskData {
            bpp: 4,
            is_packed: false,
            data: &[0xF8, 0x40, 0x05, 0xA0],
        };
        let decoded = mask.decode(3, 2).unwrap();
        assert_eq!(decoded, [255, 136, 68, 0, 85, 170]);
    }

    #[test]
    fn decode_8bpp_non_packed() {
        // 3×2 image, 8 bpp, byte-aligned (trivial copy).
        let mask = MaskData {
            bpp: 8,
            is_packed: false,
            data: &[10, 20, 30, 40, 50, 60],
        };
        let decoded = mask.decode(3, 2).unwrap();
        assert_eq!(decoded, [10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn decode_1bpp_packed() {
        // 3×3 image, 1 bpp, bit-aligned (packed, no row padding).
        // 9 pixels packed into 2 bytes, MSB first.
        // Pixels: [1,0,1, 0,1,0, 1,1,0]
        // Bits:    1 0 1 0 1 0 1 1 | 0 x x x x x x x
        // Byte 0: 0b10101011 = 0xAB
        // Byte 1: 0b00000000 = 0x00 (only MSB used)
        let mask = MaskData {
            bpp: 1,
            is_packed: true,
            data: &[0xAB, 0x00],
        };
        let decoded = mask.decode(3, 3).unwrap();
        assert_eq!(decoded, [255, 0, 255, 0, 255, 0, 255, 255, 0],);
    }

    #[test]
    fn decode_2bpp_packed() {
        // 5×2 image, 2 bpp, bit-aligned (packed, no row padding).
        // 10 pixels × 2 bits = 20 bits = 3 bytes (last 4 bits unused).
        // Pixels: [3, 2, 1, 0, 3,  0, 1, 2, 3, 0]
        // Byte 0: 0b11_10_01_00 = 0xE4  (pixels 0–3)
        // Byte 1: 0b11_00_01_10 = 0xC6  (pixels 4–7)
        // Byte 2: 0b11_00_0000 = 0xC0   (pixels 8–9, rest padding)
        // Scaled by 85: [255, 170, 85, 0, 255, 0, 85, 170, 255, 0]
        let mask = MaskData {
            bpp: 2,
            is_packed: true,
            data: &[0xE4, 0xC6, 0xC0],
        };
        let decoded = mask.decode(5, 2).unwrap();
        assert_eq!(decoded, [255, 170, 85, 0, 255, 0, 85, 170, 255, 0]);
    }

    #[test]
    fn decode_4bpp_packed() {
        // 3×2 image, 4 bpp, bit-aligned (packed, no row padding).
        // 6 pixels × 4 bits = 24 bits = 3 bytes exactly.
        // Pixels: [15, 0, 8, 4, 10, 5]
        // Byte 0: (15 << 4) | 0  = 0xF0
        // Byte 1: (8  << 4) | 4  = 0x84
        // Byte 2: (10 << 4) | 5  = 0xA5
        // Scaled by 17: [255, 0, 136, 68, 170, 85]
        let mask = MaskData {
            bpp: 4,
            is_packed: true,
            data: &[0xF0, 0x84, 0xA5],
        };
        let decoded = mask.decode(3, 2).unwrap();
        assert_eq!(decoded, [255, 0, 136, 68, 170, 85]);
    }

    #[test]
    fn decode_8bpp_packed() {
        // 3×2 image, 8 bpp, bit-aligned (packed, trivial copy).
        // Each pixel is one byte, so packed and non-packed are equivalent.
        let mask = MaskData {
            bpp: 8,
            is_packed: true,
            data: &[100, 200, 50, 0, 128, 255],
        };
        let decoded = mask.decode(3, 2).unwrap();
        assert_eq!(decoded, [100, 200, 50, 0, 128, 255]);
    }

    #[test]
    fn decode_error_cases() {
        // Zero dimensions return Ok with empty output.
        let mask = MaskData {
            bpp: 8,
            is_packed: false,
            data: &[],
        };
        assert!(mask.decode(0, 0).unwrap().is_empty());
        assert!(mask.decode(0, 5).unwrap().is_empty());
        assert!(mask.decode(5, 0).unwrap().is_empty());

        // Data too short for non-packed.
        let mask = MaskData {
            bpp: 8,
            is_packed: false,
            data: &[1, 2, 3],
        };
        assert_eq!(
            mask.decode(4, 2),
            Err(MaskDataDecodeError::InvalidDimensions)
        );

        // Data too short for packed (9 pixels at 1 bpp needs 2 bytes).
        let mask = MaskData {
            bpp: 1,
            is_packed: true,
            data: &[0xFF],
        };
        assert_eq!(
            mask.decode(3, 3),
            Err(MaskDataDecodeError::InvalidDimensions)
        );
    }

    #[test]
    fn decode_to_slice_basic_and_errors() {
        let mask = MaskData {
            bpp: 8,
            is_packed: false,
            data: &[10, 20, 30, 40, 50, 60],
        };

        // Successful decode into a provided buffer.
        let mut buf = [0u8; 6];
        mask.decode_to_slice(3, 2, &mut buf).unwrap();
        assert_eq!(buf, [10, 20, 30, 40, 50, 60]);

        // Output buffer too small.
        let mut small_buf = [0u8; 4];
        assert_eq!(
            mask.decode_to_slice(3, 2, &mut small_buf),
            Err(MaskDataDecodeError::InvalidDimensions)
        );

        // Zero dimensions succeed with empty buffer.
        let empty = MaskData {
            bpp: 8,
            is_packed: false,
            data: &[],
        };
        let mut empty_buf = [0u8; 0];
        empty.decode_to_slice(0, 0, &mut empty_buf).unwrap();
    }
}
