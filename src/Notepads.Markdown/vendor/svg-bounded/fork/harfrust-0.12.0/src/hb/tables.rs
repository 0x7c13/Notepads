// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use core::ops::Range;

use read_fonts::{
    tables::{
        ankr::Ankr,
        cmap::{Cmap, CmapSubtable, PlatformId},
        feat::Feat,
        gdef::Gdef,
        glyf::Glyf,
        gpos::Gpos,
        gsub::Gsub,
        gvar::Gvar,
        hmtx::Hmtx,
        hvar::Hvar,
        kern::Kern,
        kerx::Kerx,
        loca::Loca,
        morx::Morx,
        mvar::Mvar,
        trak::Trak,
        vmtx::Vmtx,
        vorg::Vorg,
        vvar::Vvar,
    },
    types::Tag,
    FontData, FontRef, TableProvider, TopLevelTable,
};

// https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#windows-platform-platform-id--3
const WINDOWS_SYMBOL_ENCODING: u16 = 0;
const WINDOWS_UNICODE_BMP_ENCODING: u16 = 1;
const WINDOWS_UNICODE_FULL_ENCODING: u16 = 10;

// https://docs.microsoft.com/en-us/typography/opentype/spec/name#platform-specific-encoding-and-language-ids-unicode-platform-platform-id--0
const UNICODE_1_0_ENCODING: u16 = 0;
const UNICODE_1_1_ENCODING: u16 = 1;
const UNICODE_ISO_ENCODING: u16 = 2;
const UNICODE_2_0_BMP_ENCODING: u16 = 3;
const UNICODE_2_0_FULL_ENCODING: u16 = 4;

//const UNICODE_VARIATION_ENCODING: u16 = 5;
const UNICODE_FULL_ENCODING: u16 = 6;

#[derive(Clone)]
pub struct TableRanges {
    pub num_glyphs: u32,
    pub units_per_em: u16,
    pub loca_long: bool,
    pub num_v_metrics: u16,
    pub num_h_metrics: u16,
    pub ascent: i16,
    pub descent: i16,
    pub loca: TableRange,
    pub glyf: TableRange,
    pub gvar: TableRange,
    pub hmtx: TableRange,
    pub hvar: TableRange,
    pub vmtx: TableRange,
    pub vvar: TableRange,
    pub vorg: TableRange,
    pub mvar: TableRange,
    pub cmap: TableRange,
    pub cmap_subtable: Option<SelectedCmapSubtable>,
    pub cmap_vs_subtable: Option<u16>,
    pub gdef: TableRange,
    pub gsub: TableRange,
    pub gpos: TableRange,
    pub morx: TableRange,
    pub kerx: TableRange,
    pub ankr: TableRange,
    pub kern: TableRange,
    pub feat: TableRange,
    pub trak: TableRange,
}

#[derive(Copy, Clone)]
pub struct SelectedCmapSubtable {
    pub index: u16,
    pub is_mac_roman: bool,
    pub is_symbol: bool,
}

impl TableRanges {
    pub(crate) fn try_new(
        font: &FontRef,
        control: &super::storage::Control,
    ) -> Result<Self, markdown_svg_budget::Error> {
        use read_fonts::tables::{head::Head, hhea::Hhea, maxp::Maxp, os2::Os2, vhea::Vhea};
        control.step(0)?;
        let num_glyphs = control
            .table::<Maxp>(font)?
            .map(|t| t.num_glyphs() as u32)
            .unwrap_or_default();
        let (units_per_em, loca_long) =
            control.table::<Head>(font)?.map_or((1000, false), |head| {
                (head.units_per_em(), head.index_to_loc_format() == 1)
            });
        let os2 = control.table::<Os2>(font)?;
        let hhea = control.table::<Hhea>(font)?;
        let (ascent, descent) = if let Some(os2) = &os2 {
            (os2.s_typo_ascender(), os2.s_typo_descender())
        } else if let Some(hhea) = &hhea {
            (hhea.ascender().to_i16(), hhea.descender().to_i16())
        } else {
            (0, 0)
        };
        let num_h_metrics = hhea
            .map(|hhea| hhea.number_of_h_metrics())
            .unwrap_or_default();
        let num_v_metrics = control
            .table::<Vhea>(font)?
            .map(|vhea| vhea.number_of_long_ver_metrics())
            .unwrap_or_default();
        let offset = |tag| -> Result<TableRange, markdown_svg_budget::Error> {
            control.step(
                (usize::BITS
                    - font
                        .table_directory()
                        .table_records()
                        .len()
                        .max(1)
                        .leading_zeros()
                    + 1) as u64,
            )?;
            Ok(TableRange::new(font, tag).unwrap_or_default())
        };
        let loca = offset(Loca::TAG)?;
        let glyf = offset(Glyf::TAG)?;
        let gvar = offset(Gvar::TAG)?;
        let hmtx = offset(Hmtx::TAG)?;
        let hvar = offset(Hvar::TAG)?;
        let vmtx = offset(Vmtx::TAG)?;
        let vvar = offset(Vvar::TAG)?;
        let vorg = offset(Vorg::TAG)?;
        let mvar = offset(Mvar::TAG)?;
        let cmap = offset(Cmap::TAG)?;
        let cmap_table = cmap.try_resolve_table::<Cmap>(font, control)?;
        let cmap_subtable = if let Some(cmap) = &cmap_table {
            find_best_cmap_subtable_controlled(cmap, control)?.map(
                |(index, platform, encoding, _)| SelectedCmapSubtable {
                    index,
                    is_mac_roman: platform == PlatformId::Macintosh,
                    is_symbol: platform == PlatformId::Windows
                        && encoding == WINDOWS_SYMBOL_ENCODING,
                },
            )
        } else {
            None
        };
        let mut cmap_vs_subtable = None;
        if let Some(cmap) = cmap_table {
            for (index, record) in cmap.encoding_records().iter().enumerate() {
                control.step(1)?;
                let table = match &control.0 {
                    Some(r) => record.try_subtable(
                        cmap.offset_data(),
                        &mut markdown_svg_budget::RequestControl(r),
                    )?,
                    None => record.subtable(cmap.offset_data()),
                };
                if matches!(table, Ok(CmapSubtable::Format14(_))) {
                    cmap_vs_subtable = Some(index as u16);
                    break;
                }
            }
        }
        let gdef = offset(Gdef::TAG)?;
        let gsub = offset(Gsub::TAG)?;
        let gpos = offset(Gpos::TAG)?;
        let morx = offset(Morx::TAG)?;
        let kerx = offset(Kerx::TAG)?;
        let ankr = offset(Ankr::TAG)?;
        let kern = offset(Kern::TAG)?;
        let feat = offset(Feat::TAG)?;
        let trak = offset(Trak::TAG)?;
        control.step(0)?;
        Ok(Self {
            num_glyphs,
            units_per_em,
            loca_long,
            num_v_metrics,
            num_h_metrics,
            ascent,
            descent,
            loca,
            glyf,
            gvar,
            hmtx,
            hvar,
            vmtx,
            vvar,
            vorg,
            mvar,
            cmap,
            cmap_subtable,
            cmap_vs_subtable,
            gdef,
            gsub,
            gpos,
            morx,
            kerx,
            ankr,
            kern,
            feat,
            trak,
        })
    }

    pub(crate) fn from_tables<'a>(font: &impl TableProvider<'a>) -> Self {
        let num_glyphs = font
            .maxp()
            .map(|maxp| maxp.num_glyphs() as u32)
            .unwrap_or_default();
        let (units_per_em, loca_long) = font.head().map_or((1000, false), |head| {
            (head.units_per_em(), head.index_to_loc_format() == 1)
        });
        let os2 = font.os2().ok();
        let hhea = font.hhea().ok();
        let (ascent, descent) = if let Some(os2) = &os2 {
            (os2.s_typo_ascender(), os2.s_typo_descender())
        } else if let Some(hhea) = &hhea {
            (hhea.ascender().to_i16(), hhea.descender().to_i16())
        } else {
            (0, 0) // TODO
        };
        let num_h_metrics = hhea
            .map(|hhea| hhea.number_of_h_metrics())
            .unwrap_or_default();
        let num_v_metrics = font
            .vhea()
            .map(|vhea| vhea.number_of_long_ver_metrics())
            .unwrap_or_default();
        Self {
            num_glyphs,
            units_per_em,
            loca_long,
            num_v_metrics,
            num_h_metrics,
            ascent,
            descent,
            loca: TableRange::default(),
            glyf: TableRange::default(),
            gvar: TableRange::default(),
            hmtx: TableRange::default(),
            hvar: TableRange::default(),
            vmtx: TableRange::default(),
            vvar: TableRange::default(),
            vorg: TableRange::default(),
            mvar: TableRange::default(),
            cmap: TableRange::default(),
            cmap_subtable: None,
            cmap_vs_subtable: None,
            gdef: TableRange::default(),
            gsub: TableRange::default(),
            gpos: TableRange::default(),
            morx: TableRange::default(),
            kerx: TableRange::default(),
            ankr: TableRange::default(),
            kern: TableRange::default(),
            feat: TableRange::default(),
            trak: TableRange::default(),
        }
    }
}

#[derive(Copy, Clone, Default, Debug)]
pub struct TableRange(u32, u32);

impl TableRange {
    fn new(font: &FontRef, tag: Tag) -> Option<Self> {
        let records = font.table_directory().table_records();
        records
            .binary_search_by_key(&tag, |rec| rec.tag())
            .ok()
            .and_then(|ix| records.get(ix))
            .map(|rec| Self(rec.offset(), rec.length()))
    }

    pub fn resolve(self) -> Option<Range<usize>> {
        let start = self.0 as usize;
        (start != 0).then_some(start..start.wrapping_add(self.1 as usize))
    }

    pub fn resolve_data<'a>(self, font: &FontRef<'a>) -> Option<FontData<'a>> {
        font.data().slice(self.resolve()?)
    }

    pub(crate) fn try_resolve_table<'a, T: read_fonts::control::TryFontRead<'a>>(
        self,
        font: &FontRef<'a>,
        control: &super::storage::Control,
    ) -> Result<Option<T>, markdown_svg_budget::Error> {
        control.step(0)?;
        let Some(data) = self.resolve_data(font) else {
            return Ok(None);
        };
        match &control.0 {
            None => Ok(T::read(data).ok()),
            Some(r) => T::try_read(data, &mut markdown_svg_budget::RequestControl(r))
                .map(|result| result.ok()),
        }
    }

    pub fn len(self) -> u32 {
        self.1
    }
}

fn find_best_cmap_subtable_controlled<'a>(
    cmap: &Cmap<'a>,
    control: &super::storage::Control,
) -> Result<Option<(u16, PlatformId, u16, CmapSubtable<'a>)>, markdown_svg_budget::Error> {
    for (platform, encoding) in [
        (PlatformId::Windows, WINDOWS_SYMBOL_ENCODING),
        (PlatformId::Windows, WINDOWS_UNICODE_FULL_ENCODING),
        (PlatformId::Unicode, UNICODE_FULL_ENCODING),
        (PlatformId::Unicode, UNICODE_2_0_FULL_ENCODING),
        (PlatformId::Windows, WINDOWS_UNICODE_BMP_ENCODING),
        (PlatformId::Unicode, UNICODE_2_0_BMP_ENCODING),
        (PlatformId::Unicode, UNICODE_ISO_ENCODING),
        (PlatformId::Unicode, UNICODE_1_1_ENCODING),
        (PlatformId::Unicode, UNICODE_1_0_ENCODING),
        (PlatformId::Macintosh, 0),
    ] {
        control.step(1)?;
        for (index, record) in cmap.encoding_records().iter().enumerate() {
            control.step(1)?;
            if record.platform_id() != platform || record.encoding_id() != encoding {
                continue;
            }
            let parsed = match &control.0 {
                Some(r) => record.try_subtable(
                    cmap.offset_data(),
                    &mut markdown_svg_budget::RequestControl(r),
                )?,
                None => record.subtable(cmap.offset_data()),
            };
            if let Ok(table) = parsed {
                if matches!(
                    table,
                    CmapSubtable::Format0(_)
                        | CmapSubtable::Format4(_)
                        | CmapSubtable::Format6(_)
                        | CmapSubtable::Format10(_)
                        | CmapSubtable::Format12(_)
                        | CmapSubtable::Format13(_)
                ) {
                    return Ok(Some((index as u16, platform, encoding, table)));
                }
            }
        }
    }
    control.step(0)?;
    Ok(None)
}
