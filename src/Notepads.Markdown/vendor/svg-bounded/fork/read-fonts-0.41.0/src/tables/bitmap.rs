// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Common bitmap (EBLC/EBDT/CBLC/CBDT) types.

include!("../../generated/generated_bitmap.rs");

impl BitmapSize {
    /// Returns the bitmap location information for the given glyph.
    ///
    /// The `offset_data` parameter is provided by the `offset_data()` method
    /// of the parent `Eblc` or `Cblc` table.
    ///
    /// The resulting [`BitmapLocation`] value is used by the `data()` method
    /// in the associated `Ebdt` or `Cbdt` table to extract the bitmap data.
    pub fn location(
        &self,
        offset_data: FontData,
        glyph_id: GlyphId,
    ) -> Result<BitmapLocation, ReadError> {
        crate::control::ordinary(self.try_location(
            offset_data,
            glyph_id,
            &mut crate::control::OrdinaryControl::new(),
        ))
    }
    pub fn try_location<C: markdown_svg_budget::WorkControl>(
        &self,
        offset_data: FontData,
        glyph_id: GlyphId,
        cx: &mut C,
    ) -> crate::control::ReadCtl<BitmapLocation, C> {
        use crate::control::font_try;
        cx.checkpoint(0)?;
        if !(self.start_glyph_index()..=self.end_glyph_index()).contains(&glyph_id) {
            return Ok(Err(ReadError::OutOfBounds));
        }
        let list = font_try!(self.try_index_subtable_list(offset_data, cx)?);
        let mut location = BitmapLocation {
            bit_depth: self.bit_depth,
            ..BitmapLocation::default()
        };
        for record in list.index_subtable_records() {
            cx.checkpoint(1)?;
            // Keep original parse-before-range-filter order, including bad preceding records.
            let subtable = font_try!(record.try_index_subtable(list.offset_data(), cx)?);
            if !(record.first_glyph_index()..=record.last_glyph_index()).contains(&glyph_id) {
                continue;
            }
            let ix = glyph_id.to_u32() as usize - record.first_glyph_index().to_u32() as usize;
            match &subtable {
                IndexSubtable::Format1(st) => {
                    location.format = st.image_format();
                    let start = font_try!(st.sbit_offsets().get(ix).ok_or(ReadError::OutOfBounds))
                        .get() as usize;
                    let end = font_try!(st.sbit_offsets().get(ix + 1).ok_or(ReadError::OutOfBounds))
                        .get() as usize;
                    let start = font_try!((st.image_data_offset() as usize)
                        .checked_add(start)
                        .ok_or(ReadError::OutOfBounds));
                    let end = font_try!((st.image_data_offset() as usize)
                        .checked_add(end)
                        .ok_or(ReadError::OutOfBounds));
                    location.data_offset = start;
                    location.data_size =
                        font_try!(end.checked_sub(start).ok_or(ReadError::OutOfBounds));
                }
                IndexSubtable::Format2(st) => {
                    location.format = st.image_format();
                    location.data_size = st.image_size() as usize;
                    let offset = font_try!(ix
                        .checked_mul(location.data_size)
                        .ok_or(ReadError::OutOfBounds));
                    location.data_offset = font_try!((st.image_data_offset() as usize)
                        .checked_add(offset)
                        .ok_or(ReadError::OutOfBounds));
                    location.metrics = Some(st.big_metrics()[0]);
                }
                IndexSubtable::Format3(st) => {
                    location.format = st.image_format();
                    let start = font_try!(st.sbit_offsets().get(ix).ok_or(ReadError::OutOfBounds))
                        .get() as usize;
                    let end = font_try!(st.sbit_offsets().get(ix + 1).ok_or(ReadError::OutOfBounds))
                        .get() as usize;
                    let start = font_try!((st.image_data_offset() as usize)
                        .checked_add(start)
                        .ok_or(ReadError::OutOfBounds));
                    let end = font_try!((st.image_data_offset() as usize)
                        .checked_add(end)
                        .ok_or(ReadError::OutOfBounds));
                    location.data_offset = start;
                    location.data_size =
                        font_try!(end.checked_sub(start).ok_or(ReadError::OutOfBounds));
                }
                IndexSubtable::Format4(st) => {
                    location.format = st.image_format();
                    let array = st.glyph_array();
                    cx.checkpoint(bitmap_search_work(array.len()))?;
                    let found =
                        array.binary_search_by(|x| x.glyph_id().to_u32().cmp(&glyph_id.to_u32()));
                    cx.checkpoint(0)?;
                    let ix = font_try!(
                        found.map_err(|_| ReadError::InvalidCollectionIndex(glyph_id.to_u32()))
                    );
                    let start = array[ix].sbit_offset() as usize;
                    let end = font_try!(array.get(ix + 1).ok_or(ReadError::OutOfBounds))
                        .sbit_offset() as usize;
                    location.data_offset = start;
                    location.data_size =
                        font_try!(end.checked_sub(start).ok_or(ReadError::OutOfBounds));
                }
                IndexSubtable::Format5(st) => {
                    location.format = st.image_format();
                    let array = st.glyph_array();
                    cx.checkpoint(bitmap_search_work(array.len()))?;
                    let found =
                        array.binary_search_by(|gid| gid.get().to_u32().cmp(&glyph_id.to_u32()));
                    cx.checkpoint(0)?;
                    let ix = font_try!(
                        found.map_err(|_| ReadError::InvalidCollectionIndex(glyph_id.to_u32()))
                    );
                    location.data_size = st.image_size() as usize;
                    let offset = font_try!(ix
                        .checked_mul(location.data_size)
                        .ok_or(ReadError::OutOfBounds));
                    location.data_offset = font_try!((st.image_data_offset() as usize)
                        .checked_add(offset)
                        .ok_or(ReadError::OutOfBounds));
                    location.metrics = Some(st.big_metrics()[0]);
                }
            }
            cx.checkpoint(0)?;
            return Ok(Ok(location));
        }
        Ok(Err(ReadError::OutOfBounds))
    }

    /// Returns the [IndexSubtableList] associated with this size.
    ///
    /// The `offset_data` parameter is provided by the `offset_data()` method
    /// of the parent `Eblc` or `Cblc` table.
    pub fn index_subtable_list<'a>(
        &self,
        offset_data: FontData<'a>,
    ) -> Result<IndexSubtableList<'a>, ReadError> {
        crate::control::ordinary(
            self.try_index_subtable_list(offset_data, &mut crate::control::OrdinaryControl::new()),
        )
    }
    pub fn try_index_subtable_list<'a, C: markdown_svg_budget::WorkControl>(
        &self,
        offset_data: FontData<'a>,
        cx: &mut C,
    ) -> crate::control::ReadCtl<IndexSubtableList<'a>, C> {
        cx.checkpoint(0)?;
        let start = self.index_subtable_list_offset() as usize;
        let end = crate::control::font_try!(start
            .checked_add(self.index_subtable_list_size() as usize)
            .ok_or(ReadError::OutOfBounds));
        let data =
            crate::control::font_try!(offset_data.slice(start..end).ok_or(ReadError::OutOfBounds));
        <IndexSubtableList<'a> as crate::control::TryFontReadWithArgs<'a>>::try_read_with_args(
            data,
            &self.number_of_index_subtables(),
            cx,
        )
    }
}

#[derive(Clone, Default)]
pub struct BitmapLocation {
    /// Format of EBDT/CBDT image data.
    pub format: u16,
    /// Offset in bytes from the start of the EBDT/CBDT table.
    pub data_offset: usize,
    /// Size of the image data in bytes.
    pub data_size: usize,
    /// Bit depth from the associated size. Required for computing image data
    /// size when unspecified.
    pub bit_depth: u8,
    /// Full metrics, if present in the EBLC/CBLC table.
    pub metrics: Option<BigGlyphMetrics>,
}

impl BitmapLocation {
    /// Returns true if the location references an empty bitmap glyph such as
    /// a space.
    pub fn is_empty(&self) -> bool {
        self.data_size == 0
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BitmapDataFormat {
    /// The full bitmap is tightly packed according to the bit depth.
    BitAligned,
    /// Each row of the data is aligned to a byte boundary.
    ByteAligned,
    Png,
}

#[derive(Clone)]
pub enum BitmapMetrics {
    Small(SmallGlyphMetrics),
    Big(BigGlyphMetrics),
}

#[derive(Clone)]
pub struct BitmapData<'a> {
    pub metrics: BitmapMetrics,
    pub content: BitmapContent<'a>,
}

#[derive(Clone)]
pub enum BitmapContent<'a> {
    Data(BitmapDataFormat, &'a [u8]),
    Composite(&'a [BdtComponent]),
}

pub(crate) fn try_bitmap_data<'a, C: markdown_svg_budget::WorkControl>(
    offset_data: FontData<'a>,
    location: &BitmapLocation,
    is_color: bool,
    cx: &mut C,
) -> crate::control::ReadCtl<BitmapData<'a>, C> {
    cx.checkpoint(1)?;
    let value = (|| -> Result<BitmapData<'a>, ReadError> {
        let mut image_data = offset_data
            .slice(
                location.data_offset
                    ..location
                        .data_offset
                        .checked_add(location.data_size)
                        .ok_or(ReadError::OutOfBounds)?,
            )
            .ok_or(ReadError::OutOfBounds)?
            .cursor();
        match location.format {
            // Small metrics, byte-aligned data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-1-small-metrics-byte-aligned-data>
            1 => {
                let metrics = read_small_metrics(&mut image_data)?;
                // The data for each row is padded to a byte boundary
                let pitch = (metrics.width as usize * location.bit_depth as usize).div_ceil(8);
                let height = metrics.height as usize;
                let data = image_data.read_array::<u8>(pitch * height)?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Small(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::ByteAligned, data),
                })
            }
            // Small metrics, bit-aligned data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-2-small-metrics-bit-aligned-data>
            2 => {
                let metrics = read_small_metrics(&mut image_data)?;
                let width = metrics.width as usize * location.bit_depth as usize;
                let height = metrics.height as usize;
                // The data is tightly packed
                let data = image_data.read_array::<u8>((width * height).div_ceil(8))?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Small(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::BitAligned, data),
                })
            }
            // Format 3 is obsolete
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-3-obsolete>
            // Format 4 is not supported
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-4-not-supported-metrics-in-eblc-compressed-data>
            // ---
            // Metrics in EBLC/CBLC, bit-aligned image data only
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-5-metrics-in-eblc-bit-aligned-image-data-only>
            5 => {
                let metrics = location.metrics.ok_or(ReadError::MalformedData(
                    "expected metrics from location table",
                ))?;
                let width = metrics.width as usize * location.bit_depth as usize;
                let height = metrics.height as usize;
                // The data is tightly packed
                let data = image_data.read_array::<u8>((width * height).div_ceil(8))?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Big(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::BitAligned, data),
                })
            }
            // Big metrics, byte-aligned data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-6-big-metrics-byte-aligned-data>
            6 => {
                let metrics = read_big_metrics(&mut image_data)?;
                // The data for each row is padded to a byte boundary
                let pitch = (metrics.width as usize * location.bit_depth as usize).div_ceil(8);
                let height = metrics.height as usize;
                let data = image_data.read_array::<u8>(pitch * height)?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Big(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::ByteAligned, data),
                })
            }
            // Big metrics, bit-aligned data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format7-big-metrics-bit-aligned-data>
            7 => {
                let metrics = read_big_metrics(&mut image_data)?;
                let width = metrics.width as usize * location.bit_depth as usize;
                let height = metrics.height as usize;
                // The data is tightly packed
                let data = image_data.read_array::<u8>((width * height).div_ceil(8))?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Big(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::BitAligned, data),
                })
            }
            // Small metrics, component data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-8-small-metrics-component-data>
            8 => {
                let metrics = read_small_metrics(&mut image_data)?;
                let _pad = image_data.read::<u8>()?;
                let count = image_data.read::<u16>()? as usize;
                let components = image_data.read_array::<BdtComponent>(count)?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Small(metrics),
                    content: BitmapContent::Composite(components),
                })
            }
            // Big metrics, component data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#format-9-big-metrics-component-data>
            9 => {
                let metrics = read_big_metrics(&mut image_data)?;
                let count = image_data.read::<u16>()? as usize;
                let components = image_data.read_array::<BdtComponent>(count)?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Big(metrics),
                    content: BitmapContent::Composite(components),
                })
            }
            // Small metrics, PNG image data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/cbdt#format-17-small-metrics-png-image-data>
            17 if is_color => {
                let metrics = read_small_metrics(&mut image_data)?;
                let data_len = image_data.read::<u32>()? as usize;
                let data = image_data.read_array::<u8>(data_len)?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Small(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::Png, data),
                })
            }
            // Big metrics, PNG image data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/cbdt#format-18-big-metrics-png-image-data>
            18 if is_color => {
                let metrics = read_big_metrics(&mut image_data)?;
                let data_len = image_data.read::<u32>()? as usize;
                let data = image_data.read_array::<u8>(data_len)?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Big(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::Png, data),
                })
            }
            // Metrics in CBLC table, PNG image data
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/cbdt#format-19-metrics-in-cblc-table-png-image-data>
            19 if is_color => {
                let metrics = location.metrics.ok_or(ReadError::MalformedData(
                    "expected metrics from location table",
                ))?;
                let data_len = image_data.read::<u32>()? as usize;
                let data = image_data.read_array::<u8>(data_len)?;
                Ok(BitmapData {
                    metrics: BitmapMetrics::Big(metrics),
                    content: BitmapContent::Data(BitmapDataFormat::Png, data),
                })
            }
            _ => Err(ReadError::MalformedData("unexpected bitmap data format")),
        }
    })();
    cx.checkpoint(0)?;
    Ok(value)
}

fn read_small_metrics(cursor: &mut Cursor) -> Result<SmallGlyphMetrics, ReadError> {
    Ok(cursor.read_array::<SmallGlyphMetrics>(1)?[0])
}

fn read_big_metrics(cursor: &mut Cursor) -> Result<BigGlyphMetrics, ReadError> {
    Ok(cursor.read_array::<BigGlyphMetrics>(1)?[0])
}

#[cfg(feature = "experimental_traverse")]
impl SbitLineMetrics {
    pub(crate) fn traversal_type<'a>(&self, data: FontData<'a>) -> FieldType<'a> {
        FieldType::Record(self.traverse(data))
    }
}

/// [IndexSubtables](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#indexsubtables) format type.
#[derive(Clone)]
pub enum IndexSubtable<'a> {
    Format1(IndexSubtable1<'a>),
    Format2(IndexSubtable2<'a>),
    Format3(IndexSubtable3<'a>),
    Format4(IndexSubtable4<'a>),
    Format5(IndexSubtable5<'a>),
}

impl<'a> IndexSubtable<'a> {
    ///Return the `FontData` used to resolve offsets for this table.
    pub fn offset_data(&self) -> FontData<'a> {
        match self {
            Self::Format1(item) => item.offset_data(),
            Self::Format2(item) => item.offset_data(),
            Self::Format3(item) => item.offset_data(),
            Self::Format4(item) => item.offset_data(),
            Self::Format5(item) => item.offset_data(),
        }
    }

    /// Format of this IndexSubTable.
    pub fn index_format(&self) -> u16 {
        match self {
            Self::Format1(item) => item.index_format(),
            Self::Format2(item) => item.index_format(),
            Self::Format3(item) => item.index_format(),
            Self::Format4(item) => item.index_format(),
            Self::Format5(item) => item.index_format(),
        }
    }

    /// Format of EBDT image data.
    pub fn image_format(&self) -> u16 {
        match self {
            Self::Format1(item) => item.image_format(),
            Self::Format2(item) => item.image_format(),
            Self::Format3(item) => item.image_format(),
            Self::Format4(item) => item.image_format(),
            Self::Format5(item) => item.image_format(),
        }
    }

    /// Offset to image data in EBDT table.
    pub fn image_data_offset(&self) -> u32 {
        match self {
            Self::Format1(item) => item.image_data_offset(),
            Self::Format2(item) => item.image_data_offset(),
            Self::Format3(item) => item.image_data_offset(),
            Self::Format4(item) => item.image_data_offset(),
            Self::Format5(item) => item.image_data_offset(),
        }
    }
}

impl ReadArgs for IndexSubtable<'_> {
    type Args = (GlyphId16, GlyphId16);
}
impl<'a> FontReadWithArgs<'a> for IndexSubtable<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        crate::control::ordinary(
            <Self as crate::control::TryFontReadWithArgs<'a>>::try_read_with_args(
                data,
                args,
                &mut crate::control::OrdinaryControl::new(),
            ),
        )
    }
}
impl crate::control::sealed::Reader for IndexSubtable<'_> {}
impl<'a> crate::control::TryFontReadWithArgs<'a> for IndexSubtable<'a> {
    fn try_read_with_args<C: markdown_svg_budget::WorkControl>(
        data: FontData<'a>,
        args: &Self::Args,
        cx: &mut C,
    ) -> crate::control::ReadCtl<Self, C> {
        cx.checkpoint(0)?;
        let format: u16 = crate::control::font_try!(data.read_at(0usize));
        Ok(match format {
            IndexSubtable1::FORMAT => Ok(Self::Format1(crate::control::font_try!(
                <IndexSubtable1 as crate::control::TryFontReadWithArgs>::try_read_with_args(
                    data, args, cx
                )?
            ))),
            IndexSubtable2::FORMAT => Ok(Self::Format2(crate::control::font_try!(
                <IndexSubtable2 as crate::control::TryFontRead>::try_read(data, cx)?
            ))),
            IndexSubtable3::FORMAT => Ok(Self::Format3(crate::control::font_try!(
                <IndexSubtable3 as crate::control::TryFontReadWithArgs>::try_read_with_args(
                    data, args, cx
                )?
            ))),
            IndexSubtable4::FORMAT => Ok(Self::Format4(crate::control::font_try!(
                <IndexSubtable4 as crate::control::TryFontRead>::try_read(data, cx)?
            ))),
            IndexSubtable5::FORMAT => Ok(Self::Format5(crate::control::font_try!(
                <IndexSubtable5 as crate::control::TryFontRead>::try_read(data, cx)?
            ))),
            other => Err(ReadError::InvalidFormat(other.into())),
        })
    }
}

impl<'a> MinByteRange<'a> for IndexSubtable<'a> {
    fn min_byte_range(&self) -> Range<usize> {
        match self {
            Self::Format1(item) => item.min_byte_range(),
            Self::Format2(item) => item.min_byte_range(),
            Self::Format3(item) => item.min_byte_range(),
            Self::Format4(item) => item.min_byte_range(),
            Self::Format5(item) => item.min_byte_range(),
        }
    }

    fn min_table_bytes(&self) -> &'a [u8] {
        match self {
            Self::Format1(item) => item.min_table_bytes(),
            Self::Format2(item) => item.min_table_bytes(),
            Self::Format3(item) => item.min_table_bytes(),
            Self::Format4(item) => item.min_table_bytes(),
            Self::Format5(item) => item.min_table_bytes(),
        }
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> IndexSubtable<'a> {
    fn dyn_inner<'b>(&'b self) -> &'b dyn SomeTable<'a> {
        match self {
            Self::Format1(table) => table,
            Self::Format2(table) => table,
            Self::Format3(table) => table,
            Self::Format4(table) => table,
            Self::Format5(table) => table,
        }
    }
}

#[cfg(feature = "experimental_traverse")]
impl std::fmt::Debug for IndexSubtable<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.dyn_inner().fmt(f)
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> SomeTable<'a> for IndexSubtable<'a> {
    fn type_name(&self) -> &str {
        self.dyn_inner().type_name()
    }
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        self.dyn_inner().get_field(idx)
    }
}

fn bitmap_search_work(n: usize) -> u64 {
    if n == 0 {
        0
    } else {
        (usize::BITS - (n - 1).leading_zeros()) as u64 + 1
    }
}
