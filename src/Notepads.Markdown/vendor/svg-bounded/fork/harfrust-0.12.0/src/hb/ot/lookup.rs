// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::hb::{
    hb_font_t,
    ot_layout::TableIndex,
    ot_layout_gsubgpos::{
        Apply, SubtableExternalCache, SubtableExternalCacheMode, WouldApply, WouldApplyContext,
        OT::hb_ot_apply_context_t,
    },
    set_digest::hb_set_digest_t,
    GlyphInfo,
};
use read_fonts::{
    tables::{
        gpos::{
            CursivePosFormat1, Gpos, MarkBasePosFormat1, MarkLigPosFormat1, MarkMarkPosFormat1,
            PairPos, PairPosFormat1, PairPosFormat2, SinglePos, SinglePosFormat1, SinglePosFormat2,
        },
        gsub::{
            AlternateSubstFormat1, ExtensionSubstFormat1, Gsub, LigatureSubstFormat1,
            MultipleSubstFormat1, ReverseChainSingleSubstFormat1, SingleSubst, SingleSubstFormat1,
            SingleSubstFormat2,
        },
        layout::{
            ChainedSequenceContext, ChainedSequenceContextFormat1, ChainedSequenceContextFormat2,
            ChainedSequenceContextFormat3, CoverageTable, Lookup, LookupFlag, SequenceContext,
            SequenceContextFormat1, SequenceContextFormat2, SequenceContextFormat3,
        },
    },
    FontData, FontRead, Offset, ReadError,
};

pub struct LookupData<'a> {
    /// Offset of the lookup from the base of the layout table.
    offset: usize,
    /// True if the lookup comes from GSUB.
    is_subst: bool,
    /// Data of the layout table.
    table_data: FontData<'a>,
}

pub trait LookupHost<'a> {
    fn lookup_count(&self) -> u16;
    fn lookup_data(&self, index: u16) -> Result<LookupData<'a>, ReadError>;
    fn try_lookup_count(&self, control: &Control) -> Result<u16, Error>;
    fn try_lookup_data(
        &self,
        index: u16,
        control: &Control,
    ) -> Result<Result<LookupData<'a>, ReadError>, Error>;
}

impl<'a> LookupHost<'a> for Gsub<'a> {
    fn try_lookup_count(&self, c: &Control) -> Result<u16, Error> {
        c.step(0)?;
        let list = match &c.0 {
            Some(r) => self.try_lookup_list(&mut markdown_svg_budget::RequestControl(r))?,
            None => self.lookup_list(),
        };
        Ok(list.map(|v| v.lookup_count()).unwrap_or_default())
    }
    fn try_lookup_data(
        &self,
        index: u16,
        c: &Control,
    ) -> Result<Result<LookupData<'a>, ReadError>, Error> {
        c.step(0)?;
        let list = match &c.0 {
            Some(r) => self.try_lookup_list(&mut markdown_svg_budget::RequestControl(r))?,
            None => self.lookup_list(),
        };
        let list = match list {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        let Some(offset) = list.lookup_offsets().get(index as usize) else {
            return Ok(Err(ReadError::OutOfBounds));
        };
        Ok(Ok(LookupData {
            offset: offset.get().to_usize() + self.lookup_list_offset().to_usize(),
            is_subst: true,
            table_data: self.offset_data(),
        }))
    }

    fn lookup_count(&self) -> u16 {
        self.lookup_list()
            .map(|list| list.lookup_count())
            .unwrap_or_default()
    }

    fn lookup_data(&self, index: u16) -> Result<LookupData<'a>, ReadError> {
        let list = self.lookup_list()?;
        let offset = list
            .lookup_offsets()
            .get(index as usize)
            .ok_or(ReadError::OutOfBounds)?
            .get()
            .to_usize()
            + self.lookup_list_offset().to_usize();
        Ok(LookupData {
            offset,
            is_subst: true,
            table_data: self.offset_data(),
        })
    }
}

impl<'a> LookupHost<'a> for Gpos<'a> {
    fn try_lookup_count(&self, c: &Control) -> Result<u16, Error> {
        c.step(0)?;
        let list = match &c.0 {
            Some(r) => self.try_lookup_list(&mut markdown_svg_budget::RequestControl(r))?,
            None => self.lookup_list(),
        };
        Ok(list.map(|v| v.lookup_count()).unwrap_or_default())
    }
    fn try_lookup_data(
        &self,
        index: u16,
        c: &Control,
    ) -> Result<Result<LookupData<'a>, ReadError>, Error> {
        c.step(0)?;
        let list = match &c.0 {
            Some(r) => self.try_lookup_list(&mut markdown_svg_budget::RequestControl(r))?,
            None => self.lookup_list(),
        };
        let list = match list {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        let Some(offset) = list.lookup_offsets().get(index as usize) else {
            return Ok(Err(ReadError::OutOfBounds));
        };
        Ok(Ok(LookupData {
            offset: offset.get().to_usize() + self.lookup_list_offset().to_usize(),
            is_subst: false,
            table_data: self.offset_data(),
        }))
    }

    fn lookup_count(&self) -> u16 {
        self.lookup_list()
            .map(|list| list.lookup_count())
            .unwrap_or_default()
    }

    fn lookup_data(&self, index: u16) -> Result<LookupData<'a>, ReadError> {
        let list = self.lookup_list()?;
        let offset = list
            .lookup_offsets()
            .get(index as usize)
            .ok_or(ReadError::OutOfBounds)?
            .get()
            .to_usize()
            + self.lookup_list_offset().to_usize();
        Ok(LookupData {
            offset,
            is_subst: false,
            table_data: self.offset_data(),
        })
    }
}

#[cfg(feature = "std")]
mod cache {
    use super::{LookupHost, LookupInfo};
    use std::sync::OnceLock;

    #[derive(Default)]
    pub(crate) struct PlainLookupCache {
        lookups: Vec<OnceLock<Option<Box<LookupInfo>>>>,
    }

    impl PlainLookupCache {
        pub fn new<'a>(host: &impl LookupHost<'a>) -> Self {
            let mut lookups = Vec::new();
            lookups.resize_with(host.lookup_count() as usize, Default::default);
            Self { lookups }
        }

        pub fn get<'a>(&self, host: &impl LookupHost<'a>, index: u16) -> Option<&LookupInfo> {
            self.lookups
                .get(index as usize)?
                .get_or_init(|| {
                    host.lookup_data(index)
                        .ok()
                        .and_then(|data| LookupInfo::new(&data))
                        .map(Box::new)
                })
                .as_ref()
                .map(|v| &**v)
        }
    }
}

#[cfg(not(feature = "std"))]
mod cache {
    use super::{LookupHost, LookupInfo, Vec};

    #[derive(Default)]
    pub(crate) struct PlainLookupCache {
        lookups: Vec<Option<LookupInfo>>,
    }

    impl PlainLookupCache {
        pub fn new<'a>(host: &impl LookupHost<'a>) -> Self {
            let count = host.lookup_count();
            let mut lookups = Vec::with_capacity(count as usize);
            for i in 0..count {
                lookups.push(
                    host.lookup_data(i)
                        .ok()
                        .and_then(|data| LookupInfo::new(&data)),
                );
            }
            Self { lookups }
        }

        pub fn get<'a>(&self, _host: &impl LookupHost<'a>, index: u16) -> Option<&LookupInfo> {
            self.lookups.get(index as usize)?.as_ref()
        }
    }
}

use super::super::storage::{Control, Owned, Storage};
use markdown_svg_budget::Error;
#[cfg(feature = "std")]
type LookupCell = std::sync::OnceLock<Result<Option<Owned<LookupInfo>>, Error>>;
#[cfg(not(feature = "std"))]
type LookupCell = core::cell::OnceCell<Result<Option<Owned<LookupInfo>>, Error>>;

pub(crate) struct LookupCache {
    plain: Option<cache::PlainLookupCache>,
    paid: Storage<LookupCell>,
    control: Control,
}
impl Default for LookupCache {
    fn default() -> Self {
        Self {
            plain: Some(Default::default()),
            paid: Storage::default(),
            control: Control::default(),
        }
    }
}
impl LookupCache {
    pub fn new<'a>(host: &impl LookupHost<'a>) -> Self {
        Self {
            plain: Some(cache::PlainLookupCache::new(host)),
            ..Self::default()
        }
    }
    pub(crate) fn try_new<'a>(
        host: &impl LookupHost<'a>,
        control: &Control,
    ) -> Result<Self, Error> {
        if control.0.is_none() {
            return Ok(Self::new(host));
        }
        control.step(0)?;
        let mut paid = Storage::new(control);
        paid.try_resize_with(host.try_lookup_count(control)? as usize, LookupCell::new)?;
        Ok(Self {
            plain: None,
            paid,
            control: control.clone(),
        })
    }
    pub fn try_get<'a>(
        &self,
        host: &impl LookupHost<'a>,
        index: u16,
    ) -> Result<Option<&LookupInfo>, Error> {
        self.control.step(0)?;
        if let Some(plain) = &self.plain {
            return Ok(plain.get(host, index));
        }
        let Some(cell) = self.paid.get(index as usize) else {
            return Ok(None);
        };
        let entry = cell.get_or_init(|| {
            let result = (|| {
                self.control.step(1)?;
                let Ok(data) = host.try_lookup_data(index, &self.control)? else {
                    return Ok(None);
                };
                Owned::try_new_optional(&self.control, || {
                    let result = LookupInfo::new_with_control(&data, &self.control);
                    self.control.step(0)?;
                    Ok(result)
                })
            })();
            result.map_err(|e| self.control.record(e))
        });
        match entry {
            Ok(value) => Ok(value.as_deref()),
            Err(error) => Err(*error),
        }
    }
    pub fn get<'a>(&self, host: &impl LookupHost<'a>, index: u16) -> Option<&LookupInfo> {
        match self.try_get(host, index) {
            Ok(value) => value,
            Err(error) => {
                self.control.record(error);
                None
            }
        }
    }
}

fn is_extension_lookup_type(is_subst: bool, lookup_type: u8) -> bool {
    (is_subst && lookup_type == 7) || (!is_subst && lookup_type == 9)
}

fn is_reversed(
    table_data: FontData,
    lookup: &Lookup<()>,
    lookup_offset: usize,
    control: &Control,
) -> Option<bool> {
    match lookup.lookup_type() {
        // Reverse chain context
        8 => Some(true),
        // Extension table
        7 => {
            let offset = lookup_offset + lookup.subtable_offsets().first()?.get().to_usize();
            let data = table_data.split_off(offset)?;
            // Closed O(1) extension header: no target resolution or iteration here.
            control.step(1).ok()?;
            let ext = ExtensionSubstFormat1::<()>::read(data).ok()?;
            if is_extension_lookup_type(true, ext.extension_lookup_type() as u8) {
                return None;
            }
            Some(ext.extension_lookup_type() == 8)
        }
        _ => Some(false),
    }
}

/// Cached information about a lookup.
#[derive(Default)]
pub struct LookupInfo {
    pub props: u32,
    pub is_subst: bool,
    pub is_reversed: bool,
    pub digest: hb_set_digest_t,
    pub subtable_cache_user_idx: Option<usize>,
    pub subtables: Storage<SubtableInfo>,
}

impl LookupInfo {
    pub fn new(data: &LookupData) -> Option<Self> {
        Self::new_with_control(data, &Control::default())
    }
    pub(crate) fn new_with_control(data: &LookupData, control: &Control) -> Option<Self> {
        if control.step(0).is_err() {
            return None;
        }
        let mut info = Self {
            is_subst: data.is_subst,
            subtables: Storage::new(control),
            ..Default::default()
        };
        let lookup_data = data.table_data.split_off(data.offset)?;
        let lookup: Lookup<()> = control.read(lookup_data)?;
        let lookup_type = lookup.lookup_type();
        let lookup_flag = lookup.lookup_flag();
        info.props = u32::from(lookup.lookup_flag().to_bits());
        if lookup_flag.to_bits() & LookupFlag::USE_MARK_FILTERING_SET.to_bits() != 0 {
            info.props |= (lookup.mark_filtering_set().unwrap_or_default() as u32) << 16;
        }
        if data.is_subst {
            info.is_reversed =
                is_reversed(data.table_data, &lookup, data.offset, control).unwrap_or_default();
        }
        let mut subtable_cache_user_cost = 0;
        info.subtables
            .try_reserve(lookup.sub_table_count() as usize)
            .ok()?;
        for (idx, subtable_offset) in lookup.subtable_offsets().iter().enumerate() {
            if control.step(1).is_err() {
                return None;
            }
            let cache_mode = if idx < 8 {
                SubtableExternalCacheMode::Full
            } else {
                SubtableExternalCacheMode::Small
            };
            let subtable_offset = subtable_offset.get().to_usize() + data.offset;
            if let Some((subtable_info, cache_cost)) = SubtableInfo::new(
                data.table_data,
                subtable_offset as u32,
                data.is_subst,
                lookup_type as u8,
                cache_mode,
                control,
            ) {
                control.step(0).ok()?;
                info.digest.union(&subtable_info.digest);
                if cache_cost > subtable_cache_user_cost {
                    info.subtable_cache_user_idx = Some(info.subtables.len());
                    subtable_cache_user_cost = cache_cost;
                }
                info.subtables.push(subtable_info);
            }
        }
        info.subtables.shrink_to_fit();
        Some(info)
    }

    pub fn props(&self) -> u32 {
        self.props
    }

    pub fn is_reverse(&self) -> bool {
        self.is_reversed
    }

    pub fn digest(&self) -> &hb_set_digest_t {
        &self.digest
    }
}

impl LookupInfo {
    #[inline]
    pub(crate) fn apply(
        &self,
        ctx: &mut hb_ot_apply_context_t,
        table_data: &[u8],
        use_hot_subtable_cache: bool,
    ) -> Option<()> {
        if !ctx.buffer.check_storage(0) {
            return None;
        }
        let glyph = ctx.buffer.cur(0).glyph_id;
        for (subtable_idx, subtable_info) in self.subtables.iter().enumerate() {
            if !ctx.buffer.check_storage(1) {
                return None;
            }
            if !subtable_info.digest.may_have(glyph) {
                continue;
            }
            let is_cached =
                use_hot_subtable_cache && (self.subtable_cache_user_idx == Some(subtable_idx));
            let applied = subtable_info.apply(ctx, table_data, is_cached);
            if !ctx.buffer.check_storage(0) {
                return None;
            }
            if applied.is_some() {
                return Some(());
            }
        }
        None
    }

    pub(crate) fn cache_enter(&self, ctx: &mut hb_ot_apply_context_t) -> bool {
        let Some(idx) = self.subtable_cache_user_idx else {
            return false;
        };
        let Some(subtable_info) = self.subtables.get(idx) else {
            return false;
        };
        if matches!(
            subtable_info.kind,
            SubtableKind::ContextFormat2 | SubtableKind::ChainedContextFormat2
        ) {
            cache_enter(ctx)
        } else {
            false
        }
    }
    pub(crate) fn cache_leave(&self, ctx: &mut hb_ot_apply_context_t) {
        let Some(idx) = self.subtable_cache_user_idx else {
            return;
        };
        let Some(subtable_info) = self.subtables.get(idx) else {
            return;
        };
        if matches!(
            subtable_info.kind,
            SubtableKind::ContextFormat2 | SubtableKind::ChainedContextFormat2
        ) {
            cache_leave(ctx);
        }
    }
}

impl LookupInfo {
    pub fn would_apply(&self, face: &hb_font_t, ctx: &WouldApplyContext) -> Option<bool> {
        if face.control.step(0).is_err() {
            return None;
        }
        let glyph = ctx.glyphs[0];
        if !self.digest.may_have(glyph.into()) {
            return Some(false);
        }
        let table_index = if self.is_subst {
            TableIndex::GSUB
        } else {
            TableIndex::GPOS
        };
        let table_data = face.ot_tables.table_data(table_index)?;
        for subtable_info in &self.subtables {
            if face.control.step(1).is_err() {
                return None;
            }
            if !subtable_info.digest.may_have(glyph.into()) {
                continue;
            }
            let Some(data) = table_data.get(subtable_info.offset as usize..) else {
                continue;
            };
            let data = FontData::new(data);
            let result = match subtable_info.kind {
                SubtableKind::SingleSubst1 => face
                    .control
                    .read::<SingleSubstFormat1>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::SingleSubst2 => face
                    .control
                    .read::<SingleSubstFormat2>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::MultipleSubst1 => face
                    .control
                    .read::<MultipleSubstFormat1>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::AlternateSubst1 => face
                    .control
                    .read::<AlternateSubstFormat1>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::LigatureSubst1 => face
                    .control
                    .read::<LigatureSubstFormat1>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::ReverseChainContext => face
                    .control
                    .read::<ReverseChainSingleSubstFormat1>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::ContextFormat1 => face
                    .control
                    .read::<SequenceContextFormat1>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::ContextFormat2 => face
                    .control
                    .read::<SequenceContextFormat2>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::ContextFormat3 => face
                    .control
                    .read::<SequenceContextFormat3>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::ChainedContextFormat1 => face
                    .control
                    .read::<ChainedSequenceContextFormat1>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::ChainedContextFormat2 => face
                    .control
                    .read::<ChainedSequenceContextFormat2>(data)
                    .map(|t| t.would_apply(ctx)),
                SubtableKind::ChainedContextFormat3 => face
                    .control
                    .read::<ChainedSequenceContextFormat3>(data)
                    .map(|t| t.would_apply(ctx)),
                _ => continue,
            };
            if face.control.step(0).is_err() {
                return None;
            }
            if result == Some(true) {
                return Some(true);
            }
        }
        None
    }
}

/// Cached information about a subtable.
pub struct SubtableInfo {
    /// The fully resolved type of the subtable.
    pub kind: SubtableKind,
    /// Byte offset to the subtable from the base of the GSUB or GPOS
    /// table.
    pub offset: u32,
    pub digest: hb_set_digest_t,
    pub apply_fns: [SubtableApplyFn; 2],
    pub external_cache: SubtableExternalCache,
}

pub type SubtableApplyFn =
    fn(&mut hb_ot_apply_context_t, &SubtableExternalCache, &[u8]) -> Option<()>;

impl SubtableInfo {
    #[inline]
    pub(crate) fn apply(
        &self,
        ctx: &mut hb_ot_apply_context_t,
        table_data: &[u8],
        is_cached: bool,
    ) -> Option<()> {
        if !ctx.buffer.check_storage(0) {
            return None;
        }
        let subtable_data = table_data.get(self.offset as usize..)?;
        let result = self.apply_fns[is_cached as usize](ctx, &self.external_cache, subtable_data);
        if !ctx.buffer.check_storage(0) {
            return None;
        }
        result
    }
}

macro_rules! apply_fns {
    ($apply:ident, $apply_cached:ident, $ty:ident) => {
        fn $apply(
            ctx: &mut hb_ot_apply_context_t,
            external_cache: &SubtableExternalCache,
            table_data: &[u8],
        ) -> Option<()> {
            let t = ctx.face.control.read::<$ty>(FontData::new(table_data))?;
            t.apply_with_external_cache(ctx, external_cache)
        }

        fn $apply_cached(
            ctx: &mut hb_ot_apply_context_t,
            external_cache: &SubtableExternalCache,
            table_data: &[u8],
        ) -> Option<()> {
            let t = ctx.face.control.read::<$ty>(FontData::new(table_data))?;
            t.apply_cached(ctx, external_cache)
        }
    };
}

apply_fns!(single_subst1, single_subst1_cached, SingleSubstFormat1);
apply_fns!(single_subst2, single_subst2_cached, SingleSubstFormat2);
apply_fns!(
    multiple_subst1,
    multiple_subst1_cached,
    MultipleSubstFormat1
);
apply_fns!(
    alternate_subst1,
    alternate_subst1_cached,
    AlternateSubstFormat1
);
apply_fns!(
    ligature_subst1,
    ligature_subst1_cached,
    LigatureSubstFormat1
);
apply_fns!(single_pos1, single_pos1_cached, SinglePosFormat1);
apply_fns!(single_pos2, single_pos2_cached, SinglePosFormat2);
apply_fns!(pair_pos1, pair_pos1_cached, PairPosFormat1);
apply_fns!(pair_pos2, pair_pos2_cached, PairPosFormat2);
apply_fns!(cursive_pos1, cursive_pos1_cached, CursivePosFormat1);
apply_fns!(mark_base_pos1, mark_base_pos1_cached, MarkBasePosFormat1);
apply_fns!(mark_mark_pos1, mark_mark_pos1_cached, MarkMarkPosFormat1);
apply_fns!(mark_lig_pos1, mark_lig_pos1_cached, MarkLigPosFormat1);
apply_fns!(context1, context1_cached, SequenceContextFormat1);
apply_fns!(context2, context2_cached, SequenceContextFormat2);
apply_fns!(context3, context3_cached, SequenceContextFormat3);
apply_fns!(
    chained_context1,
    chained_context1_cached,
    ChainedSequenceContextFormat1
);
apply_fns!(
    chained_context2,
    chained_context2_cached,
    ChainedSequenceContextFormat2
);
apply_fns!(
    chained_context3,
    chained_context3_cached,
    ChainedSequenceContextFormat3
);
apply_fns!(
    rev_chain_single_subst1,
    rev_chain_single_subst1_cached,
    ReverseChainSingleSubstFormat1
);

/// All possible subtables in a lookup.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SubtableKind {
    SingleSubst1,
    SingleSubst2,
    MultipleSubst1,
    AlternateSubst1,
    LigatureSubst1,
    SinglePos1,
    SinglePos2,
    PairPos1,
    PairPos2,
    CursivePos1,
    MarkBasePos1,
    MarkMarkPos1,
    MarkLigPos1,
    ContextFormat1,
    ContextFormat2,
    ContextFormat3,
    ChainedContextFormat1,
    ChainedContextFormat2,
    ChainedContextFormat3,
    ReverseChainContext,
}

impl SubtableInfo {
    fn new(
        table_data: FontData,
        subtable_offset: u32,
        is_subst: bool,
        lookup_type: u8,
        cache_mode: SubtableExternalCacheMode,
        control: &Control,
    ) -> Option<(Self, u32)> {
        control.step(1).ok()?;
        let data = table_data.split_off(subtable_offset as usize)?;
        let maybe_external_cache = |s: &dyn Apply| s.external_cache_create(cache_mode, control);
        let (kind, (external_cache, cache_cost, coverage), apply_fns): (
            SubtableKind,
            (SubtableExternalCache, u32, CoverageTable),
            [SubtableApplyFn; 2],
        ) = match (is_subst, lookup_type) {
            (true, 1) => match control.read::<SingleSubst>(data)? {
                SingleSubst::Format1(s) => (
                    SubtableKind::SingleSubst1,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [single_subst1, single_subst1_cached as _],
                ),
                SingleSubst::Format2(s) => (
                    SubtableKind::SingleSubst2,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [single_subst2, single_subst2_cached as _],
                ),
            },
            (false, 1) => match control.read::<SinglePos>(data)? {
                SinglePos::Format1(s) => (
                    SubtableKind::SinglePos1,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [single_pos1, single_pos1_cached as _],
                ),
                SinglePos::Format2(s) => (
                    SubtableKind::SinglePos2,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [single_pos2, single_pos2_cached as _],
                ),
            },
            (true, 2) => (
                SubtableKind::MultipleSubst1,
                control.read::<MultipleSubstFormat1>(data).and_then(|t| {
                    Some((
                        maybe_external_cache(&t),
                        t.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(t.coverage_offset(), t.offset_data())
                        )
                        .ok()?,
                    ))
                })?,
                [multiple_subst1, multiple_subst1_cached as _],
            ),
            (false, 2) => match control.read::<PairPos>(data)? {
                PairPos::Format1(s) => (
                    SubtableKind::PairPos1,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [pair_pos1, pair_pos1_cached as _],
                ),
                PairPos::Format2(s) => (
                    SubtableKind::PairPos2,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [pair_pos2, pair_pos2_cached as _],
                ),
            },
            (true, 3) => (
                SubtableKind::AlternateSubst1,
                control.read::<AlternateSubstFormat1>(data).and_then(|t| {
                    Some((
                        maybe_external_cache(&t),
                        t.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(t.coverage_offset(), t.offset_data())
                        )
                        .ok()?,
                    ))
                })?,
                [alternate_subst1, alternate_subst1_cached as _],
            ),
            (false, 3) => (
                SubtableKind::CursivePos1,
                control.read::<CursivePosFormat1>(data).and_then(|t| {
                    Some((
                        maybe_external_cache(&t),
                        t.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(t.coverage_offset(), t.offset_data())
                        )
                        .ok()?,
                    ))
                })?,
                [cursive_pos1, cursive_pos1_cached as _],
            ),
            (true, 4) => (
                SubtableKind::LigatureSubst1,
                control.read::<LigatureSubstFormat1>(data).and_then(|t| {
                    Some((
                        maybe_external_cache(&t),
                        t.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(t.coverage_offset(), t.offset_data())
                        )
                        .ok()?,
                    ))
                })?,
                [ligature_subst1, ligature_subst1_cached as _],
            ),
            (false, 4) => (
                SubtableKind::MarkBasePos1,
                control.read::<MarkBasePosFormat1>(data).and_then(|t| {
                    Some((
                        maybe_external_cache(&t),
                        t.cache_cost(),
                        font_read!(
                            control,
                            control.resolve::<CoverageTable, _>(
                                t.mark_coverage_offset(),
                                t.offset_data()
                            )
                        )
                        .ok()?,
                    ))
                })?,
                [mark_base_pos1, mark_base_pos1_cached as _],
            ),
            (true, 5) | (false, 7) => match control.read::<SequenceContext>(data)? {
                SequenceContext::Format1(s) => (
                    SubtableKind::ContextFormat1,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [context1, context1_cached as _],
                ),
                SequenceContext::Format2(s) => (
                    SubtableKind::ContextFormat2,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [context2, context2_cached as _],
                ),
                SequenceContext::Format3(s) => (
                    SubtableKind::ContextFormat3,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(control, control.offset_get(&s.coverages(), 0)).ok()?,
                    ),
                    [context3, context3_cached as _],
                ),
            },
            (false, 5) => (
                SubtableKind::MarkLigPos1,
                control.read::<MarkLigPosFormat1>(data).and_then(|t| {
                    Some((
                        maybe_external_cache(&t),
                        t.cache_cost(),
                        font_read!(
                            control,
                            control.resolve::<CoverageTable, _>(
                                t.mark_coverage_offset(),
                                t.offset_data()
                            )
                        )
                        .ok()?,
                    ))
                })?,
                [mark_lig_pos1, mark_lig_pos1_cached as _],
            ),
            (true, 6) | (false, 8) => match control.read::<ChainedSequenceContext>(data)? {
                ChainedSequenceContext::Format1(s) => (
                    SubtableKind::ChainedContextFormat1,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [chained_context1, chained_context1_cached as _],
                ),
                ChainedSequenceContext::Format2(s) => (
                    SubtableKind::ChainedContextFormat2,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(
                            control,
                            control
                                .resolve::<CoverageTable, _>(s.coverage_offset(), s.offset_data())
                        )
                        .ok()?,
                    ),
                    [chained_context2, chained_context2_cached as _],
                ),
                ChainedSequenceContext::Format3(s) => (
                    SubtableKind::ChainedContextFormat3,
                    (
                        maybe_external_cache(&s),
                        s.cache_cost(),
                        font_read!(control, control.offset_get(&s.input_coverages(), 0)).ok()?,
                    ),
                    [chained_context3, chained_context3_cached as _],
                ),
            },
            (true, 7) | (false, 9) => {
                // Closed fixed extension header; one tail target, never another extension.
                control.step(1).ok()?;
                let ext = ExtensionSubstFormat1::<'_, ()>::read(data).ok()?;
                let ext_type = ext.extension_lookup_type() as u8;
                if is_extension_lookup_type(is_subst, ext_type) {
                    return None;
                }
                let ext_offset = ext.extension_offset().to_u32();
                return Self::new(
                    table_data,
                    subtable_offset.checked_add(ext_offset)?,
                    is_subst,
                    ext_type,
                    cache_mode,
                    control,
                );
            }
            (false, 6) => (
                SubtableKind::MarkMarkPos1,
                control.read::<MarkMarkPosFormat1>(data).and_then(|t| {
                    Some((
                        maybe_external_cache(&t),
                        t.cache_cost(),
                        font_read!(
                            control,
                            control.resolve::<CoverageTable, _>(
                                t.mark1_coverage_offset(),
                                t.offset_data()
                            )
                        )
                        .ok()?,
                    ))
                })?,
                [mark_mark_pos1, mark_mark_pos1_cached as _],
            ),
            (true, 8) => (
                SubtableKind::ReverseChainContext,
                control
                    .read::<ReverseChainSingleSubstFormat1>(data)
                    .and_then(|t| {
                        Some((
                            maybe_external_cache(&t),
                            t.cache_cost(),
                            font_read!(
                                control,
                                control.resolve::<CoverageTable, _>(
                                    t.coverage_offset(),
                                    t.offset_data()
                                )
                            )
                            .ok()?,
                        ))
                    })?,
                [rev_chain_single_subst1, rev_chain_single_subst1_cached as _],
            ),
            _ => return None,
        };
        let mut digest = hb_set_digest_t::new();
        if let Err(error) = digest.add_coverage_controlled(&coverage, control) {
            control.record(error);
            return None;
        }
        Some((
            SubtableInfo {
                kind,
                offset: subtable_offset,
                digest,
                apply_fns,
                external_cache,
            },
            cache_cost,
        ))
    }
}

fn cache_enter(ctx: &mut hb_ot_apply_context_t) -> bool {
    if !ctx.buffer.try_allocate_var(GlyphInfo::SYLLABLE_VAR) {
        return false;
    }
    for info in &mut ctx.buffer.info {
        if let Err(error) = ctx.face.control.step(1) {
            ctx.buffer.stop_storage(error);
            return false;
        }
        info.set_syllable(255);
    }
    ctx.new_syllables = Some(255);
    true
}

fn cache_leave(ctx: &mut hb_ot_apply_context_t) {
    ctx.new_syllables = None;
    ctx.buffer.deallocate_var(GlyphInfo::SYLLABLE_VAR);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup_with_recursive_extension(lookup_type: u16) -> [u8; 16] {
        let mut data = [0; 16];
        // Lookup table.
        data[0..2].copy_from_slice(&lookup_type.to_be_bytes());
        data[4..6].copy_from_slice(&1u16.to_be_bytes());
        data[6..8].copy_from_slice(&8u16.to_be_bytes());
        // ExtensionSubstFormat1/ExtensionPosFormat1 subtable.
        data[8..10].copy_from_slice(&1u16.to_be_bytes());
        data[10..12].copy_from_slice(&lookup_type.to_be_bytes());
        data
    }

    #[test]
    fn gsub_extension_lookup_cannot_target_extension_lookup() {
        let data = lookup_with_recursive_extension(7);
        let lookup = LookupData {
            offset: 0,
            is_subst: true,
            table_data: FontData::new(&data),
        };
        let info = LookupInfo::new(&lookup).unwrap();

        assert!(info.subtables.is_empty());
    }

    #[test]
    fn gpos_extension_lookup_cannot_target_extension_lookup() {
        let data = lookup_with_recursive_extension(9);
        let lookup = LookupData {
            offset: 0,
            is_subst: false,
            table_data: FontData::new(&data),
        };
        let info = LookupInfo::new(&lookup).unwrap();

        assert!(info.subtables.is_empty());
    }
}
