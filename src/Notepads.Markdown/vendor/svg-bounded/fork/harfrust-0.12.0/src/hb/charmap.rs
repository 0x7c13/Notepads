// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use super::storage::Control;
use crate::hb::tables::{SelectedCmapSubtable, TableRanges};
use markdown_svg_budget::{Error, RequestControl};

use super::cache::hb_cache_t;
use read_fonts::{
    tables::cmap::{Cmap, Cmap14, CmapSubtable, MapVariant},
    types::GlyphId,
    FontRef, TableProvider,
};

pub type cache_t = hb_cache_t<21, 19, 256, 32>;

#[derive(Clone)]
pub struct Charmap<'a> {
    control: Control,
    subtable: Option<(SelectedCmapSubtable, CmapSubtable<'a>)>,
    vs_subtable: Option<Cmap14<'a>>,
}

impl<'a> Charmap<'a> {
    pub(crate) fn try_new(
        font: &FontRef<'a>,
        table_ranges: &TableRanges,
        control: &Control,
    ) -> Result<Self, Error> {
        control.step(0)?;
        let Some(cmap) = table_ranges.cmap.try_resolve_table::<Cmap>(font, control)? else {
            return Ok(Self {
                control: control.clone(),
                subtable: None,
                vs_subtable: None,
            });
        };
        let subtable = if let Some(selected) = table_ranges.cmap_subtable {
            let value = match &control.0 {
                Some(r) => cmap.try_subtable(selected.index, &mut RequestControl(r))?,
                None => cmap.subtable(selected.index),
            };
            value.ok().map(|table| (selected, table))
        } else {
            None
        };
        let vs_subtable = if let Some(index) = table_ranges.cmap_vs_subtable {
            let value = match &control.0 {
                Some(r) => cmap.try_subtable(index, &mut RequestControl(r))?,
                None => cmap.subtable(index),
            };
            match value {
                Ok(CmapSubtable::Format14(table)) => Some(table),
                _ => None,
            }
        } else {
            None
        };
        control.step(0)?;
        Ok(Self {
            control: control.clone(),
            subtable,
            vs_subtable,
        })
    }

    pub fn from_tables(font: &impl TableProvider<'a>) -> Self {
        if let Ok(cmap) = font.cmap() {
            let subtable = if let Some((index, record, subtable)) = cmap.best_subtable() {
                Some((
                    SelectedCmapSubtable {
                        index,
                        is_mac_roman: record.is_mac_roman(),
                        is_symbol: record.is_symbol(),
                    },
                    subtable,
                ))
            } else {
                None
            };
            Self {
                control: Control::default(),
                subtable,
                vs_subtable: cmap.uvs_subtable().map(|(_, subtable)| subtable),
            }
        } else {
            Self {
                control: Control::default(),
                subtable: None,
                vs_subtable: None,
            }
        }
    }

    pub fn map(&self, c: u32) -> Option<GlyphId> {
        match self.try_map(c) {
            Ok(value) => value,
            Err(error) => {
                self.control.record(error);
                None
            }
        }
    }
    pub(crate) fn try_map(&self, mut c: u32) -> Result<Option<GlyphId>, Error> {
        self.control.step(1)?;
        let Some(subtable) = self.subtable.as_ref() else {
            return Ok(None);
        };
        if subtable.0.is_mac_roman && c > 0x7F {
            self.control.step(128)?;
            c = unicode_to_macroman(c);
        }
        let result = match &subtable.1 {
            CmapSubtable::Format0(table) => match &self.control.0 {
                Some(r) => table.try_map_codepoint(c, &mut RequestControl(r))?,
                None => table.map_codepoint(c),
            },
            CmapSubtable::Format6(table) => match &self.control.0 {
                Some(r) => table.try_map_codepoint(c, &mut RequestControl(r))?,
                None => table.map_codepoint(c),
            },
            CmapSubtable::Format10(table) => {
                if let Some(index) = c.checked_sub(table.start_char_code()) {
                    if index < table.num_chars() {
                        table
                            .glyph_id_array()
                            .get(index as usize)
                            .map(|gid| GlyphId::from(gid.get()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            CmapSubtable::Format4(table) => match &self.control.0 {
                Some(r) => table.try_map_codepoint(c, &mut RequestControl(r))?,
                None => table.map_codepoint(c),
            },
            CmapSubtable::Format12(table) => match &self.control.0 {
                Some(r) => table.try_map_codepoint(c, &mut RequestControl(r))?,
                None => table.map_codepoint(c),
            },
            CmapSubtable::Format13(table) => match &self.control.0 {
                Some(r) => table.try_map_codepoint(c, &mut RequestControl(r))?,
                None => table.map_codepoint(c),
            },
            _ => None,
        };
        if result.is_none() && subtable.0.is_symbol && c <= 0x00FF {
            // For symbol-encoded OpenType fonts, we duplicate the
            // U+F000..F0FF range at U+0000..U+00FF.  That's what
            // Windows seems to do, and that's hinted about at:
            // https://docs.microsoft.com/en-us/typography/opentype/spec/recom
            // under "Non-Standard (Symbol) Fonts".
            return self.try_map(0xF000 + c);
        }
        Ok(result)
    }

    pub fn map_variant(&self, c: u32, vs: u32) -> Option<GlyphId> {
        match self.try_map_variant(c, vs) {
            Ok(value) => value,
            Err(error) => {
                self.control.record(error);
                None
            }
        }
    }
    pub(crate) fn try_map_variant(&self, c: u32, vs: u32) -> Result<Option<GlyphId>, Error> {
        self.control.step(1)?;
        let Some(table) = self.vs_subtable.as_ref() else {
            return Ok(None);
        };
        let variant = match &self.control.0 {
            Some(r) => table.try_map_variant(c, vs, &mut RequestControl(r))?,
            None => table.map_variant(c, vs),
        };
        match variant {
            Some(MapVariant::UseDefault) => self.try_map(c),
            Some(MapVariant::Variant(gid)) => Ok(Some(gid)),
            None => Ok(None),
        }
    }
}

#[rustfmt::skip]
static UNICODE_TO_MACROMAN: &[u16] = &[
    0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1,
    0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8,
    0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3,
    0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC,
    0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF,
    0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8,
    0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211,
    0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x03A9, 0x00E6, 0x00F8,
    0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB,
    0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153,
    0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA,
    0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02,
    0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1,
    0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC, 0x00D3, 0x00D4,
    0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC,
    0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7,
];

fn unicode_to_macroman(c: u32) -> u32 {
    let u = c as u16;
    let Some(index) = UNICODE_TO_MACROMAN.iter().position(|m| *m == u) else {
        return 0;
    };
    (0x80 + index) as u32
}
