// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
pub mod layout;
pub mod layout_common;
pub mod layout_kerx_table;
pub mod layout_morx_table;
pub mod layout_trak_table;
pub mod map;
pub(crate) mod reader;

use crate::hb::aat::layout_kerx_table::KerxSubtableCache;
use crate::hb::aat::layout_morx_table::MorxSubtableCache;
use crate::hb::kerning::KernSubtableCache;
use crate::hb::ot::OtTables;
use crate::hb::storage::{Control, Storage as Vec};
use crate::hb::tables::TableRanges;
use read_fonts::{
    tables::{ankr::Ankr, feat::Feat, kern::Kern, kerx::Kerx, morx::Morx, trak::Trak},
    FontRef, TableProvider,
};

#[derive(Default)]
pub struct AatCache {
    pub morx: Vec<MorxSubtableCache>,
    pub kern: Vec<KernSubtableCache>,
    pub kerx: Vec<KerxSubtableCache>,
}

impl AatCache {
    pub fn new<'a>(font: &impl TableProvider<'a>) -> Self {
        Self::from_tables(
            font.maxp()
                .map(|v| v.num_glyphs() as u32)
                .unwrap_or_default(),
            font.morx().ok(),
            font.kern().ok(),
            font.kerx().ok(),
            &Control::default(),
        )
        .unwrap()
    }
    pub(crate) fn try_new(
        font: &FontRef<'_>,
        control: &Control,
    ) -> Result<Self, markdown_svg_budget::Error> {
        let num = control
            .table::<read_fonts::tables::maxp::Maxp>(font)?
            .map(|v| v.num_glyphs() as u32)
            .unwrap_or_default();
        let morx = control.table::<Morx>(font)?;
        let kern = control.table::<Kern>(font)?;
        let kerx = control.table::<Kerx>(font)?;
        Self::from_tables(num, morx, kern, kerx, control)
    }
    fn from_tables(
        num_glyphs: u32,
        morx: Option<Morx>,
        kern: Option<Kern>,
        kerx: Option<Kerx>,
        control: &Control,
    ) -> Result<Self, markdown_svg_budget::Error> {
        use reader::Cursor;
        control.step(0)?;
        let mut cache = Self {
            morx: Vec::new(control),
            kern: Vec::new(control),
            kerx: Vec::new(control),
        };
        if let Some(morx) = morx {
            let mut chains = reader::chains(&morx, control)?;
            while let Some(chain) = chains.next(control)? {
                let Ok(chain) = chain else {
                    continue;
                };
                let mut subtables = reader::morx_subtables(&chain, control)?;
                while let Some(subtable) = subtables.next(control)? {
                    let Ok(subtable) = subtable else {
                        continue;
                    };
                    cache.morx.try_reserve(1)?;
                    let value = MorxSubtableCache::try_new(&subtable, num_glyphs, control)?;
                    cache.morx.try_push(value)?;
                }
            }
        }
        if let Some(kern) = kern {
            let mut subtables = reader::kern_subtables(&kern, control)?;
            while let Some(subtable) = subtables.next(control)? {
                let Ok(subtable) = subtable else {
                    continue;
                };
                cache.kern.try_reserve(1)?;
                let value = KernSubtableCache::try_new(&subtable, num_glyphs, control)?;
                cache.kern.try_push(value)?;
            }
        }
        if let Some(kerx) = kerx {
            let mut subtables = reader::kerx_subtables(&kerx, control)?;
            while let Some(subtable) = subtables.next(control)? {
                let Ok(subtable) = subtable else {
                    continue;
                };
                cache.kerx.try_reserve(1)?;
                let value = KerxSubtableCache::try_new(&subtable, num_glyphs, control)?;
                cache.kerx.try_push(value)?;
            }
        }
        control.step(0)?;
        Ok(cache)
    }
}

#[derive(Clone, Default)]
pub struct AatTables<'a> {
    pub morx: Option<(Morx<'a>, &'a [MorxSubtableCache])>,
    pub ankr: Option<Ankr<'a>>,
    pub kern: Option<(Kern<'a>, &'a [KernSubtableCache])>,
    pub kerx: Option<(Kerx<'a>, &'a [KerxSubtableCache])>,
    pub trak: Option<Trak<'a>>,
    pub feat: Option<Feat<'a>>,
}

use crate::hb::algs::HB_CODEPOINT_ENCODE3 as encode3;

/// Blocklist specific broken morx tables identified by the combination of
/// morx, GSUB, and GDEF table lengths.
fn is_morx_blocklisted(morx_len: u32, gsub_len: u32, gdef_len: u32) -> bool {
    const BLOCKLIST: &[u64] = &[
        // AALMAGHRIBI.ttf — https://github.com/harfbuzz/harfbuzz/issues/4108
        encode3(19892, 2794, 340),
    ];
    let key = encode3(morx_len, gsub_len, gdef_len);
    BLOCKLIST.contains(&key)
}

impl<'a> AatTables<'a> {
    pub(crate) fn try_new(
        font: &FontRef<'a>,
        cache: &'a AatCache,
        table_ranges: &TableRanges,
        control: &Control,
    ) -> Result<Self, markdown_svg_budget::Error> {
        control.step(0)?;
        let morx = if is_morx_blocklisted(
            table_ranges.morx.len(),
            table_ranges.gsub.len(),
            table_ranges.gdef.len(),
        ) {
            None
        } else {
            table_ranges
                .morx
                .try_resolve_table(font, control)?
                .map(|table| (table, cache.morx.as_slice()))
        };
        let ankr = table_ranges.ankr.try_resolve_table(font, control)?;
        let kern = table_ranges
            .kern
            .try_resolve_table(font, control)?
            .map(|table| (table, cache.kern.as_slice()));
        let kerx = table_ranges
            .kerx
            .try_resolve_table(font, control)?
            .map(|table| (table, cache.kerx.as_slice()));
        let trak = table_ranges.trak.try_resolve_table(font, control)?;
        let feat = table_ranges.feat.try_resolve_table(font, control)?;
        control.step(0)?;
        Ok(Self {
            morx,
            ankr,
            kern,
            kerx,
            trak,
            feat,
        })
    }

    pub fn from_tables(
        font: &impl TableProvider<'a>,
        ot_tables: &OtTables,
        cache: &'a AatCache,
    ) -> Self {
        let morx = if let Ok(morx) = font.morx() {
            let gsub_len = ot_tables
                .gsub
                .as_ref()
                .map_or(0, |table| table.table.offset_data().len() as u32);
            let gdef_len = ot_tables
                .gdef
                .table
                .as_ref()
                .map_or(0, |table| table.offset_data().len() as u32);
            if is_morx_blocklisted(morx.offset_data().len() as u32, gsub_len, gdef_len) {
                None
            } else {
                Some((morx, cache.morx.as_slice()))
            }
        } else {
            None
        };
        let ankr = font.ankr().ok();
        let kern = font.kern().ok().map(|table| (table, cache.kern.as_slice()));
        let kerx = font.kerx().ok().map(|table| (table, cache.kerx.as_slice()));
        let trak = font.trak().ok();
        let feat = font.feat().ok();
        Self {
            morx,
            ankr,
            kern,
            kerx,
            trak,
            feat,
        }
    }
}
