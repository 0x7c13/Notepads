//! A [Naming Table](
//! https://docs.microsoft.com/en-us/typography/opentype/spec/name) implementation.

use super::parser::{FromData, LazyArray16, Offset, Offset16, Stream};
use super::Language;

/// A list of [name ID](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-ids)'s.
pub mod name_id {
    #![allow(missing_docs)]

    pub const FAMILY: u16 = 1;
    pub const POST_SCRIPT_NAME: u16 = 6;
    pub const TYPOGRAPHIC_FAMILY: u16 = 16;
}

/// A [platform ID](https://docs.microsoft.com/en-us/typography/opentype/spec/name#platform-ids).
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlatformId {
    Unicode,
    Macintosh,
    Iso,
    Windows,
    Custom,
}

impl FromData for PlatformId {
    const SIZE: usize = 2;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        match u16::parse(data)? {
            0 => Some(PlatformId::Unicode),
            1 => Some(PlatformId::Macintosh),
            2 => Some(PlatformId::Iso),
            3 => Some(PlatformId::Windows),
            4 => Some(PlatformId::Custom),
            _ => None,
        }
    }
}

#[inline]
fn is_unicode_encoding(platform_id: PlatformId, encoding_id: u16) -> bool {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/name#windows-encoding-ids
    const WINDOWS_SYMBOL_ENCODING_ID: u16 = 0;
    const WINDOWS_UNICODE_BMP_ENCODING_ID: u16 = 1;

    match platform_id {
        PlatformId::Unicode => true,
        PlatformId::Windows => matches!(
            encoding_id,
            WINDOWS_SYMBOL_ENCODING_ID | WINDOWS_UNICODE_BMP_ENCODING_ID
        ),
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct NameRecord {
    platform_id: PlatformId,
    encoding_id: u16,
    language_id: u16,
    name_id: u16,
    length: u16,
    offset: Offset16,
}

impl FromData for NameRecord {
    const SIZE: usize = 12;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        let mut s = Stream::new(data);
        Some(NameRecord {
            platform_id: s.read::<PlatformId>()?,
            encoding_id: s.read::<u16>()?,
            language_id: s.read::<u16>()?,
            name_id: s.read::<u16>()?,
            length: s.read::<u16>()?,
            offset: s.read::<Offset16>()?,
        })
    }
}

/// A [Name Record](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-records).
#[derive(Clone, Copy)]
pub struct Name<'a> {
    /// A platform ID.
    pub platform_id: PlatformId,
    /// A platform-specific encoding ID.
    pub encoding_id: u16,
    /// A language ID.
    pub language_id: u16,
    /// A [Name ID](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-ids).
    ///
    /// A predefined list of ID's can be found in the [`name_id`](name_id/index.html) module.
    pub name_id: u16,
    /// A raw name data.
    ///
    /// Can be in any encoding. Can be empty.
    pub name: &'a [u8],
}

impl<'a> Name<'a> {
    /// Checks that the current Name data has a Unicode encoding.
    #[inline]
    pub fn is_unicode(&self) -> bool {
        is_unicode_encoding(self.platform_id, self.encoding_id)
    }

    /// Returns a Name language.
    pub fn language(&self) -> Language {
        if self.platform_id == PlatformId::Windows {
            Language::windows_language(self.language_id)
        } else if self.platform_id == PlatformId::Macintosh
            && self.encoding_id == 0
            && self.language_id == 0
        {
            Language::English_UnitedStates
        } else {
            Language::Unknown
        }
    }
}

/// A list of face names.
#[derive(Clone, Copy, Default)]
pub struct Names<'a> {
    records: LazyArray16<'a, NameRecord>,
    storage: &'a [u8],
}

impl<'a> Names<'a> {
    /// Returns a name at index.
    pub fn get(&self, index: u16) -> Option<Name<'a>> {
        let record = self.records.get(index)?;
        let name_start = record.offset.to_usize();
        let name_end = name_start + usize::from(record.length);
        let name = self.storage.get(name_start..name_end)?;
        Some(Name {
            platform_id: record.platform_id,
            encoding_id: record.encoding_id,
            language_id: record.language_id,
            name_id: record.name_id,
            name,
        })
    }

    /// Returns a number of name records.
    pub fn len(&self) -> u16 {
        self.records.len()
    }
}

impl core::fmt::Debug for Names<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "Names {{ ... }}")
    }
}

impl<'a> IntoIterator for Names<'a> {
    type Item = Name<'a>;
    type IntoIter = NamesIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        NamesIter {
            names: self,
            index: 0,
        }
    }
}

/// An iterator over face names.
#[derive(Clone, Copy)]
pub struct NamesIter<'a> {
    names: Names<'a>,
    index: u16,
}

impl<'a> Iterator for NamesIter<'a> {
    type Item = Name<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.names.len() {
            self.index += 1;
            self.names.get(self.index - 1)
        } else {
            None
        }
    }

    #[inline]
    fn count(self) -> usize {
        usize::from(self.names.len().saturating_sub(self.index))
    }
}

/// A [Naming Table](
/// https://docs.microsoft.com/en-us/typography/opentype/spec/name).
#[derive(Clone, Copy, Default, Debug)]
pub struct Table<'a> {
    /// A list of names.
    pub names: Names<'a>,
}

impl<'a> Table<'a> {
    /// Parses a table from raw data.
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/name#naming-table-format-1
        const LANG_TAG_RECORD_SIZE: u16 = 4;

        let mut s = Stream::new(data);
        let version = s.read::<u16>()?;
        let count = s.read::<u16>()?;
        let storage_offset = s.read::<Offset16>()?.to_usize();

        if version == 0 {
            // Do nothing.
        } else if version == 1 {
            let lang_tag_count = s.read::<u16>()?;
            let lang_tag_len = lang_tag_count.checked_mul(LANG_TAG_RECORD_SIZE)?;
            s.advance(usize::from(lang_tag_len)); // langTagRecords
        } else {
            // Unsupported version.
            return None;
        }

        let records = s.read_array16::<NameRecord>(count)?;

        if s.offset() < storage_offset {
            s.advance(storage_offset - s.offset());
        }

        let storage = s.tail()?;

        Some(Table {
            names: Names { records, storage },
        })
    }
}
