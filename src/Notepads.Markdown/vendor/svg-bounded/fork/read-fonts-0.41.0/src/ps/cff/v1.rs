// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! CFF version 1.

include!("../../../generated/generated_cff.rs");
use crate::control::{self, OrdinaryControl, PsCtl, ReadCtl, TryFontRead};
use markdown_svg_budget::WorkControl;

use crate::ps::{
    cff::{charset::Charset, dict},
    error::Error,
    string::Sid,
};

/// The [Compact Font Format](https://learn.microsoft.com/en-us/typography/opentype/spec/cff) table.
#[derive(Clone)]
pub struct Cff<'a> {
    header: CffHeader<'a>,
    names: Index<'a>,
    top_dicts: Index<'a>,
    strings: Index<'a>,
    global_subrs: Index<'a>,
}

impl<'a> Cff<'a> {
    pub fn offset_data(&self) -> FontData<'a> {
        self.header.offset_data()
    }

    pub fn header(&self) -> CffHeader<'a> {
        self.header.clone()
    }

    /// Returns the name index.
    ///
    /// This contains the PostScript names of all fonts in the font set.
    ///
    /// See "Name INDEX" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=13>
    pub fn names(&self) -> Index<'a> {
        self.names.clone()
    }

    /// Returns the PostScript name for the font in the font set at the
    /// given index.
    pub fn name(&self, index: usize) -> Option<&'a [u8]> {
        self.names.get(index).ok()
    }

    /// Returns the top dict index.
    ///
    /// This contains the top-level DICTs of all fonts in the font set. The
    /// objects here correspond to those in the name index.
    ///
    /// See "Top DICT INDEX" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=14>
    pub fn top_dicts(&self) -> Index<'a> {
        self.top_dicts.clone()
    }

    /// Returns the string index.
    ///
    /// This contains all of the strings used by fonts within the font set.
    /// They are referenced by string identifiers represented by the
    /// [`Sid`] type.
    ///
    /// See "String INDEX" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=17>
    pub fn strings(&self) -> Index<'a> {
        self.strings.clone()
    }

    /// Returns the associated string for the given identifier.
    ///
    /// If the identifier does not represent a standard string, the result is
    /// looked up in the string index.
    pub fn string(&self, id: Sid) -> Option<&'a [u8]> {
        match id.resolve_standard() {
            Ok(name) => Some(name),
            Err(ix) => self.strings.get(ix).ok(),
        }
    }

    /// Returns the global subroutine index.
    ///
    /// This contains sub-programs that are referenced by one or more
    /// charstrings in the font set.
    ///
    /// See "Local/Global Subrs INDEXes" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=25>
    pub fn global_subrs(&self) -> Index<'a> {
        self.global_subrs.clone()
    }

    /// Returns the character set associated with the top dict at the given
    /// index.
    ///
    /// See "Charsets" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=21>
    pub fn charset(&self, top_dict_index: usize) -> Result<Option<Charset<'a>>, Error> {
        control::ordinary(self.try_charset(top_dict_index, &mut OrdinaryControl::new()))
    }
    pub fn try_charset<C: WorkControl>(
        &self,
        top_dict_index: usize,
        cx: &mut C,
    ) -> PsCtl<Option<Charset<'a>>, C> {
        cx.checkpoint(0)?;
        let top_dict = control::font_try!(self.top_dicts().try_get(top_dict_index, cx)?);
        let offset_data = self.offset_data();
        let mut charset_offset: Option<usize> = None;
        let mut num_glyphs: Option<u32> = None;
        let mut entries = dict::try_entries(top_dict, None);
        while let Some(entry) = entries.try_next(cx)? {
            match entry {
                Ok(dict::Entry::Charset(offset)) => {
                    charset_offset = Some(offset);
                }
                Ok(dict::Entry::CharstringsOffset(offset)) => {
                    num_glyphs = Some(
                        control::font_try!(Index::try_read(
                            control::font_try!(offset_data
                                .split_off(offset)
                                .ok_or(ReadError::OutOfBounds)),
                            cx
                        )?)
                        .count() as u32,
                    );
                }
                // The ROS operator signifies a CID-keyed font and the charset
                // maps to CIDs rather than SIDs which we don't parse for
                // glyph names.
                // <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=28>
                Ok(dict::Entry::Ros { .. }) => {
                    return Ok(Ok(None));
                }
                _ => {}
            }
        }
        if let Some((charset_offset, num_glyphs)) = charset_offset.zip(num_glyphs) {
            Ok(Ok(Some(control::font_try!(Charset::try_new(
                offset_data,
                charset_offset,
                num_glyphs,
                cx
            )?))))
        } else {
            Ok(Ok(None))
        }
    }
}

impl TopLevelTable for Cff<'_> {
    const TAG: Tag = Tag::new(b"CFF ");
}

impl<'a> FontRead<'a> for Cff<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        control::ordinary(Self::try_read(data, &mut OrdinaryControl::new()))
    }
}
impl control::sealed::Reader for Cff<'_> {}
impl<'a> TryFontRead<'a> for Cff<'a> {
    fn try_read<C: WorkControl>(data: FontData<'a>, cx: &mut C) -> ReadCtl<Self, C> {
        cx.checkpoint(1)?;
        let header = control::font_try!(CffHeader::try_read(data, cx)?);
        let mut data = FontData::new(header.trailing_data());
        let names = control::font_try!(Index::try_read(data, cx)?);
        data = control::font_try!(data
            .split_off(control::font_try!(names.try_size_in_bytes(cx)?))
            .ok_or(ReadError::OutOfBounds));
        let top_dicts = control::font_try!(Index::try_read(data, cx)?);
        data = control::font_try!(data
            .split_off(control::font_try!(top_dicts.try_size_in_bytes(cx)?))
            .ok_or(ReadError::OutOfBounds));
        let strings = control::font_try!(Index::try_read(data, cx)?);
        data = control::font_try!(data
            .split_off(control::font_try!(strings.try_size_in_bytes(cx)?))
            .ok_or(ReadError::OutOfBounds));
        let global_subrs = control::font_try!(Index::try_read(data, cx)?);
        cx.checkpoint(0)?;
        Ok(Ok(Self {
            header,
            names,
            top_dicts,
            strings,
            global_subrs,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FontRef, TableProvider};

    #[test]
    fn read_noto_serif_display_cff() {
        let font = FontRef::new(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED).unwrap();
        let cff = font.cff().unwrap();
        assert_eq!(cff.header().major(), 1);
        assert_eq!(cff.header().minor(), 0);
        assert_eq!(cff.top_dicts().count(), 1);
        assert_eq!(cff.names().count(), 1);
        assert_eq!(cff.global_subrs.count(), 17);
        let name = cff.names().get(0).unwrap();
        assert_eq!(name, b"NotoSerifDisplay-Regular");
        assert_eq!(cff.strings().count(), 5);
        // Version
        assert_eq!(cff.string(Sid::new(391)).unwrap(), b"2.9");
        // Notice
        assert_eq!(
            cff.string(Sid::new(392)).unwrap(),
            b"Noto is a trademark of Google LLC."
        );
        // Copyright
        assert_eq!(
            cff.string(Sid::new(393)).unwrap(),
            b"Copyright 2022 The Noto Project Authors https:github.comnotofontslatin-greek-cyrillic"
        );
        // FullName
        assert_eq!(
            cff.string(Sid::new(394)).unwrap(),
            b"Noto Serif Display Regular"
        );
        // FamilyName
        assert_eq!(cff.string(Sid::new(395)).unwrap(), b"Noto Serif Display");
    }

    #[test]
    fn glyph_names() {
        test_glyph_names(
            font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
            &[".notdef", "i", "j", "k", "l"],
        );
    }

    #[test]
    fn icons_glyph_names() {
        test_glyph_names(font_test_data::MATERIAL_ICONS_SUBSET, &[".notdef", "_10k"]);
    }

    fn test_glyph_names(font_data: &[u8], expected_names: &[&str]) {
        let font = FontRef::new(font_data).unwrap();
        let cff = font.cff().unwrap();
        let charset = cff.charset(0).unwrap().unwrap();
        let sid_to_string = |sid| std::str::from_utf8(cff.string(sid).unwrap()).unwrap();
        let names_by_lookup = (0..charset.num_glyphs())
            .map(|gid| sid_to_string(charset.string_id(GlyphId::new(gid)).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(names_by_lookup, expected_names);
        let names_by_iter = charset
            .iter()
            .map(|(_gid, sid)| sid_to_string(sid))
            .collect::<Vec<_>>();
        assert_eq!(names_by_iter, expected_names);
    }
}
