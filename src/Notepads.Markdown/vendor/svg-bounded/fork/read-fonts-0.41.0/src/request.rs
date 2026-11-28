// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-bound entry points for the private font reader fork.

use crate::control::{self, OrdinaryControl, ReadCtl, TryFontRead, TryTableProvider};
use crate::types::{
    BigEndian, Offset32, Tag, CFF_SFNT_VERSION, TRUE_SFNT_VERSION, TTC_HEADER_TAG, TT_SFNT_VERSION,
};
use crate::{
    CollectionRef, FileRef, FontData, FontRef, Offset, ReadError, TTCHeader, TableDirectory,
};
use markdown_svg_budget::{
    Error, FailureKind, LimitKind, RequestControl, ResourcesRef, WorkControl,
};

// Only this mode can use the old arbitrary-u32 TTC range arithmetic. Request
// construction selects CheckedFont, which has a font-error checked range.
trait FontMode<C: WorkControl> {
    fn offsets<'a>(header: &TTCHeader<'a>, control: &mut C) -> ReadCtl<&'a [BigEndian<u32>], C>;
}
struct PlainFont;
struct CheckedFont;
impl FontMode<OrdinaryControl> for PlainFont {
    fn offsets<'a>(
        header: &TTCHeader<'a>,
        _: &mut OrdinaryControl,
    ) -> ReadCtl<&'a [BigEndian<u32>], OrdinaryControl> {
        Ok(Ok(header.table_directory_offsets()))
    }
}
impl<C: WorkControl> FontMode<C> for CheckedFont {
    fn offsets<'a>(header: &TTCHeader<'a>, control: &mut C) -> ReadCtl<&'a [BigEndian<u32>], C> {
        control.checkpoint(0)?;
        let start = header.num_fonts_byte_range().end;
        let Some(bytes) = (header.num_fonts() as usize).checked_mul(4) else {
            return Ok(Err(ReadError::OutOfBounds));
        };
        let Some(end) = start.checked_add(bytes) else {
            return Ok(Err(ReadError::OutOfBounds));
        };
        // The original view defaults a truncated array to empty. An overflowing
        // range is malformed font data, not a resource allocation quota.
        Ok(Ok(header
            .offset_data()
            .read_array(start..end)
            .unwrap_or_default()))
    }
}

impl<'a> FileRef<'a> {
    pub fn try_new<C: WorkControl>(data: &'a [u8], control: &mut C) -> ReadCtl<Self, C> {
        control.checkpoint(0)?;
        match CollectionRef::try_new(data, control)? {
            Ok(collection) => Ok(Ok(Self::Collection(collection))),
            Err(_) => Ok(FontRef::try_new(data, control)?.map(Self::Font)),
        }
    }
}
impl<'a> CollectionRef<'a> {
    pub fn try_new<C: WorkControl>(bytes: &'a [u8], control: &mut C) -> ReadCtl<Self, C> {
        control.checkpoint(0)?;
        let data = FontData::new(bytes);
        let header = control::font_try!(TTCHeader::try_read(data, control)?);
        if header.ttc_tag() != TTC_HEADER_TAG {
            return Ok(Err(ReadError::InvalidTtc(header.ttc_tag())));
        }
        control.checkpoint(0)?;
        Ok(Ok(Self { data, header }))
    }
    pub fn try_get<C: WorkControl>(&self, index: u32, control: &mut C) -> ReadCtl<FontRef<'a>, C> {
        collection_get_mode::<CheckedFont, C>(self, index, control)
    }
}
fn collection_get_mode<'a, M: FontMode<C>, C: WorkControl>(
    collection: &CollectionRef<'a>,
    index: u32,
    control: &mut C,
) -> ReadCtl<FontRef<'a>, C> {
    control.checkpoint(0)?;
    let offsets = control::font_try!(M::offsets(&collection.header, control)?);
    let offset = control::font_try!(offsets
        .get(index as usize)
        .ok_or(ReadError::InvalidCollectionIndex(index)))
    .get() as usize;
    let data = control::font_try!(collection
        .data
        .slice(offset..)
        .ok_or(ReadError::OutOfBounds));
    let directory = control::font_try!(TableDirectory::try_read(data, control)?);
    FontRef::try_with_table_directory(collection.data, directory, Some(index), control)
}
pub(crate) fn collection_get_ordinary<'a>(
    collection: &CollectionRef<'a>,
    index: u32,
) -> Result<FontRef<'a>, ReadError> {
    control::ordinary(collection_get_mode::<PlainFont, _>(
        collection,
        index,
        &mut OrdinaryControl::new(),
    ))
}

impl TableDirectory<'_> {
    fn try_is_sorted<C: WorkControl>(&self, control: &mut C) -> Result<bool, C::Error> {
        control.checkpoint(0)?;
        let mut last_tag = Tag::new(&[0u8; 4]);
        for record in self.table_records() {
            control.checkpoint(1)?;
            let tag = record.tag();
            if tag <= last_tag {
                control.checkpoint(0)?;
                return Ok(false);
            }
            last_tag = tag;
        }
        control.checkpoint(0)?;
        Ok(true)
    }
}

impl<'a> FontRef<'a> {
    pub fn try_new<C: WorkControl>(bytes: &'a [u8], control: &mut C) -> ReadCtl<Self, C> {
        control.checkpoint(0)?;
        let data = FontData::new(bytes);
        let directory = control::font_try!(TableDirectory::try_read(data, control)?);
        Self::try_with_table_directory(data, directory, None, control)
    }
    pub fn try_from_index<C: WorkControl>(
        data: &'a [u8],
        index: u32,
        control: &mut C,
    ) -> ReadCtl<Self, C> {
        from_index_mode::<CheckedFont, C>(data, index, control)
    }
    fn try_with_table_directory<C: WorkControl>(
        data: FontData<'a>,
        table_directory: TableDirectory<'a>,
        ttc_index: Option<u32>,
        control: &mut C,
    ) -> ReadCtl<Self, C> {
        control.checkpoint(0)?;
        if ![TT_SFNT_VERSION, CFF_SFNT_VERSION, TRUE_SFNT_VERSION]
            .contains(&table_directory.sfnt_version())
        {
            return Ok(Err(ReadError::InvalidSfnt(table_directory.sfnt_version())));
        }
        let table_directory_sorted = table_directory.try_is_sorted(control)?;
        control.checkpoint(0)?;
        Ok(Ok(Self {
            data,
            table_directory,
            ttc_index: ttc_index.unwrap_or_default(),
            in_ttc: ttc_index.is_some(),
            table_directory_sorted,
        }))
    }
    pub fn try_table_data<C: WorkControl>(
        &self,
        tag: Tag,
        control: &mut C,
    ) -> Result<Option<FontData<'a>>, C::Error> {
        control.checkpoint(0)?;
        let records = self.table_directory.table_records();
        let entry = if self.table_directory_sorted {
            // Fixed Rust 1.99.1 slice::binary_search_by performs exactly
            // ceil(log2(n))+1 comparisons for n>0, including Equal. SFNT's
            // count is u16: <=17 pure comparisons, <=272 record bytes. Pay this
            // short bounded window before calling the original std kernel.
            let comparisons = if records.is_empty() {
                0
            } else {
                (usize::BITS - (records.len() - 1).leading_zeros()) as u64 + 1
            };
            control.checkpoint(comparisons)?;
            records
                .binary_search_by(|record| record.tag.get().cmp(&tag))
                .ok()
        } else {
            let mut found = None;
            for (index, record) in records.iter().enumerate() {
                control.checkpoint(1)?;
                if record.tag.get() == tag {
                    found = Some(index);
                    break;
                }
            }
            found
        };
        control.checkpoint(0)?;
        Ok(entry
            .and_then(|index| records.get(index))
            .and_then(|record| {
                let start = Offset32::new(record.offset()).non_null()?;
                let length = record.length() as usize;
                self.data.slice(start..start.checked_add(length)?)
            }))
    }
}
fn from_index_mode<'a, M: FontMode<C>, C: WorkControl>(
    data: &'a [u8],
    index: u32,
    control: &mut C,
) -> ReadCtl<FontRef<'a>, C> {
    control.checkpoint(0)?;
    let file = control::font_try!(FileRef::try_new(data, control)?);
    match file {
        FileRef::Font(font) => Ok(if index == 0 {
            Ok(font)
        } else {
            Err(ReadError::InvalidCollectionIndex(index))
        }),
        FileRef::Collection(collection) => collection_get_mode::<M, C>(&collection, index, control),
    }
}
pub(crate) fn from_index_ordinary<'a>(
    data: &'a [u8],
    index: u32,
) -> Result<FontRef<'a>, ReadError> {
    control::ordinary(from_index_mode::<PlainFont, _>(
        data,
        index,
        &mut OrdinaryControl::new(),
    ))
}

/// A mode-restricted provider. No Deref or ordinary TableProvider implementation.
pub struct CheckedFontRef<'a, 'r> {
    font: FontRef<'a>,
    resources: &'r ResourcesRef,
}
impl<'a, 'r> CheckedFontRef<'a, 'r> {
    pub fn from_index(
        bytes: &'a [u8],
        index: u32,
        resources: &'r ResourcesRef,
    ) -> Result<Result<Self, ReadError>, Error> {
        Ok(
            FontRef::try_from_index(bytes, index, &mut RequestControl(resources))?
                .map(|font| Self { font, resources }),
        )
    }
    pub fn data(&self) -> FontData<'a> {
        self.font.data()
    }
    pub fn ttc_index(&self) -> Option<u32> {
        self.font.ttc_index()
    }
    pub fn table_directory(&self) -> &TableDirectory<'a> {
        self.font.table_directory()
    }
}
impl control::sealed::Provider for CheckedFontRef<'_, '_> {}
impl<'a, 'r> TryTableProvider<'a, RequestControl<'r>> for CheckedFontRef<'a, 'r> {
    fn try_data_for_tag(
        &self,
        tag: Tag,
        control: &mut RequestControl<'r>,
    ) -> Result<Option<FontData<'a>>, Error> {
        if !self.resources.is_same_request(control.0) {
            return Err(control
                .0
                .record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        self.font.try_table_data(tag, control)
    }
}
