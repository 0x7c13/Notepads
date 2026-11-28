// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use super::{coverage_binary_cached, coverage_index, covered, glyph_class};
use crate::hb::aat::reader::{next_valid, offsets};
use crate::hb::buffer::GlyphInfo;
use crate::hb::ot::coverage_get;
use crate::hb::ot::{ClassDefInfo, CoverageInfo};
use crate::hb::ot_layout_gsubgpos::OT::hb_ot_apply_context_t;
use crate::hb::ot_layout_gsubgpos::{
    apply_lookup, match_always, match_backtrack, match_glyph, match_input, match_lookahead,
    may_skip_t, skipping_iterator_t, Apply, BinaryCache, ChainContextFormat2Cache,
    ContextFormat2Cache, SubtableExternalCache, SubtableExternalCacheMode, WouldApply,
    WouldApplyContext,
};
use read_fonts::tables::gsub::ClassDef;
use read_fonts::tables::layout::{
    ChainedClassSequenceRule, ChainedSequenceContextFormat1, ChainedSequenceContextFormat2,
    ChainedSequenceContextFormat3, ChainedSequenceRule, ClassSequenceRule, SequenceContextFormat1,
    SequenceContextFormat2, SequenceContextFormat3, SequenceLookupRecord, SequenceRule,
};
use read_fonts::types::{BigEndian, FixedSize, GlyphId, Offset16};
use read_fonts::{ArrayOfOffsets, FontData};

impl WouldApply for SequenceContextFormat1<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return false;
        }
        let Some(index) = coverage_index(
            font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            ),
            ctx.glyphs[0],
            &control,
        ) else {
            return false;
        };
        let index = index as usize;
        let Some(set) = font_read!(control, control.nullable_get(&self.seq_rule_sets(), index))
            .transpose()
            .ok()
            .flatten()
        else {
            return false;
        };
        let rules = set.seq_rules();
        let mut cursor = offsets(&rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            let input = rule.input_sequence();
            if ctx.glyphs.len() != input.len() + 1 {
                continue;
            }
            let mut matched = true;
            for (i, value) in input.iter().enumerate() {
                if control.step(1).is_err() {
                    return false;
                }
                let mut info = GlyphInfo {
                    glyph_id: ctx.glyphs[i + 1].into(),
                    ..GlyphInfo::default()
                };
                let matches = match_glyph(&mut info, value.get().to_u32());
                if control.step(0).is_err() {
                    return false;
                }
                if !matches {
                    matched = false;
                    break;
                }
            }
            if matched {
                return true;
            }
        }
        false
    }
}

impl Apply for SequenceContextFormat1<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let index = coverage_get(
            &font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            )
            .ok()?,
            glyph,
            &control,
        )? as usize;
        let set = font_read!(control, control.nullable_get(&self.seq_rule_sets(), index))?.ok()?;
        apply_context_rules(ctx, &set.seq_rules(), match_glyph)
    }
}

impl WouldApply for SequenceContextFormat2<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return false;
        }
        let class_def = font_read!(
            control,
            control.resolve::<ClassDef, _>(self.class_def_offset(), self.offset_data())
        )
        .ok();
        let match_fn = match_class(&class_def, &control);
        let index = glyph_class(
            font_read!(
                control,
                control.resolve::<ClassDef, _>(self.class_def_offset(), self.offset_data())
            ),
            ctx.glyphs[0],
            &control,
        ) as usize;
        let Some(set) = font_read!(
            control,
            control.nullable_get(&self.class_seq_rule_sets(), index)
        )
        .transpose()
        .ok()
        .flatten() else {
            return false;
        };
        let rules = set.class_seq_rules();
        let mut cursor = offsets(&rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            let input = rule.input_sequence();
            if ctx.glyphs.len() != input.len() + 1 {
                continue;
            }
            let mut matched = true;
            for (i, value) in input.iter().enumerate() {
                if control.step(1).is_err() {
                    return false;
                }
                let mut info = GlyphInfo {
                    glyph_id: ctx.glyphs[i + 1].into(),
                    ..GlyphInfo::default()
                };
                let matches = match_fn(&mut info, value.get() as u32);
                if control.step(0).is_err() {
                    return false;
                }
                if !matches {
                    matched = false;
                    break;
                }
            }
            if matched {
                return true;
            }
        }
        false
    }
}

impl Apply for SequenceContextFormat2<'_> {
    fn apply_with_external_cache(
        &self,
        ctx: &mut hb_ot_apply_context_t,
        external_cache: &SubtableExternalCache,
    ) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let SubtableExternalCache::ContextFormat2Cache(cache) = external_cache else {
            return None;
        };
        let offset_data = self.offset_data();
        coverage_binary_cached(
            |gid| cache.coverage.index(&offset_data, gid, &control),
            glyph,
            &cache.coverage_cache,
            &control,
        )?;
        let input_class = |gid| cache.input.class(&offset_data, gid, &control);
        let index = input_class(glyph) as usize;
        let set = font_read!(
            control,
            control.nullable_get(&self.class_seq_rule_sets(), index)
        )?
        .ok()?;
        apply_context_rules(ctx, &set.class_seq_rules(), |info, value| {
            u32::from(input_class(info.as_glyph())) == value
        })
    }

    fn apply_cached(
        &self,
        ctx: &mut hb_ot_apply_context_t,
        external_cache: &SubtableExternalCache,
    ) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let SubtableExternalCache::ContextFormat2Cache(cache) = external_cache else {
            return None;
        };
        let offset_data = self.offset_data();
        coverage_binary_cached(
            |gid| cache.coverage.index(&offset_data, gid, &control),
            glyph,
            &cache.coverage_cache,
            &control,
        )?;
        let input_class = |gid| cache.input.class(&offset_data, gid, &control);
        let index =
            get_class_cached(&input_class, &mut ctx.buffer.info[ctx.buffer.idx], &control) as usize;
        let set = font_read!(
            control,
            control.nullable_get(&self.class_seq_rule_sets(), index)
        )?
        .ok()?;
        apply_context_rules(
            ctx,
            &set.class_seq_rules(),
            match_class_cached(&input_class, &control),
        )
    }

    fn cache_cost(&self) -> u32 {
        self.class_def()
            .ok()
            .map_or(0, |class_def| class_def.cost())
    }

    fn external_cache_create(
        &self,
        _mode: SubtableExternalCacheMode,
        _control: &crate::hb::storage::Control,
    ) -> SubtableExternalCache {
        let data = self.offset_data();
        SubtableExternalCache::ContextFormat2Cache(ContextFormat2Cache {
            coverage_cache: BinaryCache::new(),
            coverage: CoverageInfo::new(&data, self.coverage_offset().to_u32() as u16)
                .unwrap_or_default(),
            input: ClassDefInfo::new(&data, self.class_def_offset().to_u32() as u16)
                .unwrap_or_default(),
        })
    }
}

impl WouldApply for SequenceContextFormat3<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return false;
        }
        let coverages = self.coverages();
        if ctx.glyphs.len() != coverages.len() + 1 {
            return false;
        }
        for i in 0..coverages.len() {
            let coverage = font_read!(control, control.offset_get(&coverages, i));
            if !covered(coverage, ctx.glyphs[i + 1], &control) {
                return false;
            }
        }
        true
    }
}

impl Apply for SequenceContextFormat3<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let input_coverages = self.coverages();
        coverage_get(
            &font_read!(control, control.offset_get(&input_coverages, 0)).ok()?,
            glyph,
            &control,
        )?;
        let input = |info: &mut GlyphInfo, index: u32| {
            font_read!(
                control,
                control.offset_get(&input_coverages, index as usize + 1)
            )
            .is_ok_and(|cov| coverage_get(&cov, info.glyph_id, &control).is_some())
        };
        let mut match_end = 0;
        if match_input(
            ctx,
            input_coverages.len() as u16 - 1,
            input,
            &mut match_end,
            None,
        ) {
            ctx.buffer
                .unsafe_to_break(Some(ctx.buffer.idx), Some(match_end));
            apply_lookup(
                ctx,
                input_coverages.len() - 1,
                match_end,
                self.seq_lookup_records(),
            );
            Some(())
        } else {
            ctx.buffer
                .unsafe_to_concat(Some(ctx.buffer.idx), Some(match_end));
            None
        }
    }
}

impl WouldApply for ChainedSequenceContextFormat1<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return false;
        }
        let Some(index) = coverage_index(
            font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            ),
            ctx.glyphs[0],
            &control,
        ) else {
            return false;
        };
        let index = index as usize;
        let Some(set) = font_read!(
            control,
            control.nullable_get(&self.chained_seq_rule_sets(), index)
        )
        .transpose()
        .ok()
        .flatten() else {
            return false;
        };
        let rules = set.chained_seq_rules();
        let mut cursor = offsets(&rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            if ctx.zero_context
                && (rule.backtrack_glyph_count() != 0 || rule.lookahead_glyph_count() != 0)
            {
                continue;
            }
            let input = rule.input_sequence();
            if ctx.glyphs.len() != input.len() + 1 {
                continue;
            }
            let mut matched = true;
            for (i, value) in input.iter().enumerate() {
                if control.step(1).is_err() {
                    return false;
                }
                let mut info = GlyphInfo {
                    glyph_id: ctx.glyphs[i + 1].into(),
                    ..GlyphInfo::default()
                };
                let matches = match_glyph(&mut info, value.get().to_u32());
                if control.step(0).is_err() {
                    return false;
                }
                if !matches {
                    matched = false;
                    break;
                }
            }
            if matched {
                return true;
            }
        }
        false
    }
}

impl Apply for ChainedSequenceContextFormat1<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let index = coverage_get(
            &font_read!(
                control,
                control.resolve::<read_fonts::tables::layout::CoverageTable, _>(
                    self.coverage_offset(),
                    self.offset_data()
                )
            )
            .ok()?,
            glyph,
            &control,
        )? as usize;
        let set = font_read!(
            control,
            control.nullable_get(&self.chained_seq_rule_sets(), index)
        )?
        .ok()?;
        apply_chain_context_rules(
            ctx,
            &set.chained_seq_rules(),
            (match_glyph, match_glyph, match_glyph),
        )
    }
}

impl WouldApply for ChainedSequenceContextFormat2<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return false;
        }
        let class_def = font_read!(
            control,
            control.resolve::<ClassDef, _>(self.input_class_def_offset(), self.offset_data())
        )
        .ok();
        let match_fn = match_class(&class_def, &control);
        let index = glyph_class(
            font_read!(
                control,
                control.resolve::<ClassDef, _>(self.input_class_def_offset(), self.offset_data())
            ),
            ctx.glyphs[0],
            &control,
        ) as usize;
        let Some(set) = font_read!(
            control,
            control.nullable_get(&self.chained_class_seq_rule_sets(), index)
        )
        .transpose()
        .ok()
        .flatten() else {
            return false;
        };
        let rules = set.chained_class_seq_rules();
        let mut cursor = offsets(&rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            if ctx.zero_context
                && (rule.backtrack_glyph_count() != 0 || rule.lookahead_glyph_count() != 0)
            {
                continue;
            }
            let input = rule.input_sequence();
            if ctx.glyphs.len() != input.len() + 1 {
                continue;
            }
            let mut matched = true;
            for (i, value) in input.iter().enumerate() {
                if control.step(1).is_err() {
                    return false;
                }
                let mut info = GlyphInfo {
                    glyph_id: ctx.glyphs[i + 1].into(),
                    ..GlyphInfo::default()
                };
                let matches = match_fn(&mut info, value.get() as u32);
                if control.step(0).is_err() {
                    return false;
                }
                if !matches {
                    matched = false;
                    break;
                }
            }
            if matched {
                return true;
            }
        }
        false
    }
}

/// Value represents glyph class.
fn match_class<'a>(
    class_def: &'a Option<ClassDef<'a>>,
    control: &'a crate::hb::storage::Control,
) -> impl Fn(&mut GlyphInfo, u32) -> bool + 'a {
    |&mut info, value| {
        class_def.as_ref().is_some_and(|class_def| {
            match control.class_get(class_def, info.as_glyph()) {
                Ok(v) => u32::from(v) == value,
                Err(e) => {
                    control.record(e);
                    false
                }
            }
        })
    }
}

fn get_class_cached(
    class_def: &impl Fn(GlyphId) -> u16,
    info: &mut GlyphInfo,
    control: &crate::hb::storage::Control,
) -> u16 {
    if control.step(0).is_err() {
        return 0;
    }
    let mut klass = info.syllable() as u16;
    if klass < 255 {
        return klass;
    }
    klass = class_def(info.as_glyph());
    if control.step(0).is_err() {
        return 0;
    }
    if klass < 255 {
        info.set_syllable(klass as u8);
    }

    klass
}

fn match_class_cached<'a>(
    class_def: impl Fn(GlyphId) -> u16 + 'a,
    control: &'a crate::hb::storage::Control,
) -> impl Fn(&mut GlyphInfo, u32) -> bool + 'a {
    move |info: &mut GlyphInfo, value| {
        u32::from(get_class_cached(&class_def, info, control)) == value
    }
}

fn get_class_cached1(
    class_def: &impl Fn(GlyphId) -> u16,
    info: &mut GlyphInfo,
    control: &crate::hb::storage::Control,
) -> u16 {
    if control.step(0).is_err() {
        return 0;
    }
    let mut klass = (info.syllable() & 0x0F) as u16;
    if klass < 15 {
        return klass;
    }

    klass = class_def(info.as_glyph());
    if control.step(0).is_err() {
        return 0;
    }

    if klass < 15 {
        info.set_syllable((info.syllable() & 0xF0) | klass as u8);
    }

    klass
}

fn match_class_cached1<'a>(
    class_def: impl Fn(GlyphId) -> u16 + 'a,
    control: &'a crate::hb::storage::Control,
) -> impl Fn(&mut GlyphInfo, u32) -> bool + 'a {
    move |info: &mut GlyphInfo, value| {
        u32::from(get_class_cached1(&class_def, info, control)) == value
    }
}

fn get_class_cached2(
    class_def: &impl Fn(GlyphId) -> u16,
    info: &mut GlyphInfo,
    control: &crate::hb::storage::Control,
) -> u16 {
    if control.step(0).is_err() {
        return 0;
    }
    let mut klass = (info.syllable() & 0xF0) as u16 >> 4;
    if klass < 15 {
        return klass;
    }
    klass = class_def(info.as_glyph());
    if control.step(0).is_err() {
        return 0;
    }
    if klass < 15 {
        info.set_syllable((info.syllable() & 0x0F) | ((klass as u8) << 4));
    }
    klass
}

fn match_class_cached2<'a>(
    class_def: impl Fn(GlyphId) -> u16 + 'a,
    control: &'a crate::hb::storage::Control,
) -> impl Fn(&mut GlyphInfo, u32) -> bool + 'a {
    move |info: &mut GlyphInfo, value| {
        u32::from(get_class_cached2(&class_def, info, control)) == value
    }
}

impl Apply for ChainedSequenceContextFormat2<'_> {
    fn apply_with_external_cache(
        &self,
        ctx: &mut hb_ot_apply_context_t,
        external_cache: &SubtableExternalCache,
    ) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let SubtableExternalCache::ChainContextFormat2Cache(cache) = external_cache else {
            return None;
        };
        let offset_data = self.offset_data();
        coverage_binary_cached(
            |gid| cache.coverage.index(&offset_data, gid, &control),
            glyph,
            &cache.coverage_cache,
            &control,
        )?;
        let index = cache.input.class(&offset_data, glyph, &control) as usize;
        let set = font_read!(
            control,
            control.nullable_get(&self.chained_class_seq_rule_sets(), index)
        )?
        .ok()?;
        apply_chain_context_rules(
            ctx,
            &set.chained_class_seq_rules(),
            (
                |info, val| {
                    u32::from(
                        cache
                            .backtrack
                            .class(&offset_data, info.as_glyph(), &control),
                    ) == val
                },
                |info, val| {
                    u32::from(cache.input.class(&offset_data, info.as_glyph(), &control)) == val
                },
                |info, val| {
                    u32::from(
                        cache
                            .lookahead
                            .class(&offset_data, info.as_glyph(), &control),
                    ) == val
                },
            ),
        )
    }
    fn apply_cached(
        &self,
        ctx: &mut hb_ot_apply_context_t,
        external_cache: &SubtableExternalCache,
    ) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();
        let SubtableExternalCache::ChainContextFormat2Cache(cache) = external_cache else {
            return None;
        };
        let offset_data = self.offset_data();
        coverage_binary_cached(
            |gid| cache.coverage.index(&offset_data, gid, &control),
            glyph,
            &cache.coverage_cache,
            &control,
        )?;
        let input_class = |gid| cache.input.class(&offset_data, gid, &control);
        let lookahead_class = |gid| cache.lookahead.class(&offset_data, gid, &control);
        let index = get_class_cached2(&input_class, &mut ctx.buffer.info[ctx.buffer.idx], &control)
            as usize;
        let set = font_read!(
            control,
            control.nullable_get(&self.chained_class_seq_rule_sets(), index)
        )?
        .ok()?;
        apply_chain_context_rules(
            ctx,
            &set.chained_class_seq_rules(),
            (
                |info, val| {
                    u32::from(
                        cache
                            .backtrack
                            .class(&offset_data, info.as_glyph(), &control),
                    ) == val
                },
                match_class_cached2(&input_class, &control),
                match_class_cached1(&lookahead_class, &control),
            ),
        )
    }
    fn cache_cost(&self) -> u32 {
        self.input_class_def()
            .ok()
            .map_or(0, |class_def| class_def.cost())
            + self
                .lookahead_class_def()
                .ok()
                .map_or(0, |class_def| class_def.cost())
    }

    fn external_cache_create(
        &self,
        _mode: SubtableExternalCacheMode,
        _control: &crate::hb::storage::Control,
    ) -> SubtableExternalCache {
        let data = self.offset_data();
        SubtableExternalCache::ChainContextFormat2Cache(ChainContextFormat2Cache {
            coverage_cache: BinaryCache::new(),
            coverage: CoverageInfo::new(&data, self.coverage_offset().to_u32() as u16)
                .unwrap_or_default(),
            backtrack: ClassDefInfo::new(&data, self.backtrack_class_def_offset().to_u32() as u16)
                .unwrap_or_default(),
            input: ClassDefInfo::new(&data, self.input_class_def_offset().to_u32() as u16)
                .unwrap_or_default(),
            lookahead: ClassDefInfo::new(&data, self.lookahead_class_def_offset().to_u32() as u16)
                .unwrap_or_default(),
        })
    }
}

impl WouldApply for ChainedSequenceContextFormat3<'_> {
    fn would_apply(&self, ctx: &WouldApplyContext) -> bool {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return false;
        }
        let input_coverages = self.input_coverages();
        if ctx.zero_context
            && (!self.backtrack_coverage_offsets().is_empty()
                || !self.lookahead_coverage_offsets().is_empty())
        {
            return false;
        }
        if ctx.glyphs.len() != input_coverages.len() {
            return false;
        }
        for i in 1..input_coverages.len() {
            let coverage = font_read!(control, control.offset_get(&input_coverages, i));
            if !covered(coverage, ctx.glyphs[i], &control) {
                return false;
            }
        }
        true
    }
}

impl Apply for ChainedSequenceContextFormat3<'_> {
    fn apply(&self, ctx: &mut hb_ot_apply_context_t) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let glyph = ctx.buffer.cur(0).as_glyph();

        let input_coverages = self.input_coverages();
        coverage_get(
            &font_read!(control, control.offset_get(&input_coverages, 0)).ok()?,
            glyph,
            &control,
        )?;

        let backtrack_coverages = self.backtrack_coverages();
        let lookahead_coverages = self.lookahead_coverages();

        let back = |info: &mut GlyphInfo, index: u32| {
            font_read!(
                control,
                control.offset_get(&backtrack_coverages, index as usize)
            )
            .is_ok_and(|cov| coverage_get(&cov, info.glyph_id, &control).is_some())
        };

        let ahead = |info: &mut GlyphInfo, index: u32| {
            font_read!(
                control,
                control.offset_get(&lookahead_coverages, index as usize)
            )
            .is_ok_and(|cov| coverage_get(&cov, info.glyph_id, &control).is_some())
        };

        let input = |info: &mut GlyphInfo, index: u32| {
            font_read!(
                control,
                control.offset_get(&input_coverages, index as usize + 1)
            )
            .is_ok_and(|cov| coverage_get(&cov, info.glyph_id, &control).is_some())
        };

        let mut end_index = ctx.buffer.idx;
        let mut match_end = 0;

        let input_matches = match_input(
            ctx,
            input_coverages.len() as u16 - 1,
            input,
            &mut match_end,
            None,
        );

        if input_matches {
            end_index = match_end;
        }

        if !(input_matches
            && match_lookahead(
                ctx,
                lookahead_coverages.len() as u16,
                ahead,
                match_end,
                &mut end_index,
            ))
        {
            ctx.buffer
                .unsafe_to_concat(Some(ctx.buffer.idx), Some(end_index));
            return None;
        }

        let mut start_index = ctx.buffer.out_len;

        if !match_backtrack(
            ctx,
            backtrack_coverages.len() as u16,
            back,
            &mut start_index,
        ) {
            ctx.buffer
                .unsafe_to_concat_from_outbuffer(Some(start_index), Some(end_index));
            return None;
        }

        ctx.buffer
            .unsafe_to_break_from_outbuffer(Some(start_index), Some(end_index));
        apply_lookup(
            ctx,
            input_coverages.len() - 1,
            match_end,
            self.seq_lookup_records(),
        );

        Some(())
    }
}

/// All of a context rule's fields, parsed in a single pass.
///
/// The generated getters re-derive the positions of all preceding fields on
/// every call, so fetching fields individually in the rule-matching loops
/// re-reads the leading counts many times over; this parses the whole rule
/// once instead. Glyph ids and classes are both read as raw u16s.
#[derive(Clone, Copy, Default)]
struct ParsedRule<'a> {
    backtrack: &'a [BigEndian<u16>],
    input: &'a [BigEndian<u16>],
    lookahead: &'a [BigEndian<u16>],
    records: &'a [SequenceLookupRecord],
}

impl<'a> ParsedRule<'a> {
    /// Parse a SequenceRule or ClassSequenceRule.
    fn from_rule_data(data: FontData<'a>) -> Option<Self> {
        let glyph_count: u16 = data.read_at(0).ok()?;
        let record_count: u16 = data.read_at(2).ok()?;
        let input_end = 4 + (glyph_count as usize).saturating_sub(1) * u16::RAW_BYTE_LEN;
        let records_end = input_end + record_count as usize * SequenceLookupRecord::RAW_BYTE_LEN;
        Some(ParsedRule {
            input: data.read_array(4..input_end).ok()?,
            records: data.read_array(input_end..records_end).ok()?,
            ..ParsedRule::default()
        })
    }

    /// Parse a ChainedSequenceRule or ChainedClassSequenceRule.
    fn from_chain_rule_data(data: FontData<'a>) -> Option<Self> {
        let backtrack_count: u16 = data.read_at(0).ok()?;
        let backtrack_end = 2 + backtrack_count as usize * u16::RAW_BYTE_LEN;
        let input_count: u16 = data.read_at(backtrack_end).ok()?;
        let input_end =
            backtrack_end + 2 + (input_count as usize).saturating_sub(1) * u16::RAW_BYTE_LEN;
        let lookahead_count: u16 = data.read_at(input_end).ok()?;
        let lookahead_end = input_end + 2 + lookahead_count as usize * u16::RAW_BYTE_LEN;
        let record_count: u16 = data.read_at(lookahead_end).ok()?;
        let records_end =
            lookahead_end + 2 + record_count as usize * SequenceLookupRecord::RAW_BYTE_LEN;
        Some(ParsedRule {
            backtrack: data.read_array(2..backtrack_end).ok()?,
            input: data.read_array(backtrack_end + 2..input_end).ok()?,
            lookahead: data.read_array(input_end + 2..lookahead_end).ok()?,
            records: data.read_array(lookahead_end + 2..records_end).ok()?,
        })
    }

    /// Match this rule's input sequence and apply its lookup records.
    ///
    /// Backtrack/lookahead are not consulted; chain rules go through
    /// [`apply_chain_with_sequences`] instead.
    fn apply(
        &self,
        ctx: &mut hb_ot_apply_context_t,
        match_func: &impl Fn(&mut GlyphInfo, u32) -> bool,
    ) -> Option<()> {
        let control = ctx.control().clone();
        if control.step(0).is_err() {
            return Default::default();
        }
        let inputs = self.input;
        let match_func = |info: &mut GlyphInfo, index| {
            inputs
                .get(index as usize)
                .is_some_and(|value| match_func(info, value.get() as u32))
        };

        let mut match_end = 0;

        if match_input(ctx, inputs.len() as _, match_func, &mut match_end, None) {
            ctx.buffer
                .unsafe_to_break(Some(ctx.buffer.idx), Some(match_end));
            apply_lookup(ctx, inputs.len(), match_end, self.records);
            return Some(());
        }
        None
    }
}

trait ContextRule<'a>: read_fonts::control::TryFontRead<'a> + 'a {
    /// Parse all of the rule's fields in one pass.
    fn parse(&self) -> ParsedRule<'a>;

    /// The first glyph/class of the input sequence, or `None` if the input
    /// sequence is empty.
    ///
    /// Much cheaper than a full parse; used by the skip-ahead loops, which
    /// discard most of the rules they visit.
    fn first_input(&self) -> Option<u16>;
}

/// `first_input` for SequenceRule/ClassSequenceRule.
///
/// The glyph count is at offset 0 and the input sequence starts at offset 4,
/// after the lookup record count. (The input sequence's first entry covers
/// the *second* glyph of the matched sequence.)
fn plain_rule_first_input(data: &FontData) -> Option<u16> {
    let glyph_count: u16 = data.read_at(0).ok()?;
    if glyph_count <= 1 {
        return None;
    }
    data.read_at(4).ok()
}

/// `first_input` for ChainedSequenceRule/ChainedClassSequenceRule.
///
/// The input glyph count follows the variable-length backtrack sequence, and
/// the input sequence follows it directly.
fn chain_rule_first_input(data: &FontData) -> Option<u16> {
    let backtrack_count: u16 = data.read_at(0).ok()?;
    let count_pos = 2 + backtrack_count as usize * u16::RAW_BYTE_LEN;
    let input_count: u16 = data.read_at(count_pos).ok()?;
    if input_count <= 1 {
        return None;
    }
    data.read_at(count_pos + 2).ok()
}

impl<'a> ContextRule<'a> for SequenceRule<'a> {
    fn parse(&self) -> ParsedRule<'a> {
        ParsedRule::from_rule_data(self.offset_data()).unwrap_or_default()
    }

    fn first_input(&self) -> Option<u16> {
        plain_rule_first_input(&self.offset_data())
    }
}

impl<'a> ContextRule<'a> for ClassSequenceRule<'a> {
    fn parse(&self) -> ParsedRule<'a> {
        ParsedRule::from_rule_data(self.offset_data()).unwrap_or_default()
    }

    fn first_input(&self) -> Option<u16> {
        plain_rule_first_input(&self.offset_data())
    }
}

impl<'a> ContextRule<'a> for ChainedSequenceRule<'a> {
    fn parse(&self) -> ParsedRule<'a> {
        ParsedRule::from_chain_rule_data(self.offset_data()).unwrap_or_default()
    }

    fn first_input(&self) -> Option<u16> {
        chain_rule_first_input(&self.offset_data())
    }
}

impl<'a> ContextRule<'a> for ChainedClassSequenceRule<'a> {
    fn parse(&self) -> ParsedRule<'a> {
        ParsedRule::from_chain_rule_data(self.offset_data()).unwrap_or_default()
    }

    fn first_input(&self) -> Option<u16> {
        chain_rule_first_input(&self.offset_data())
    }
}

fn apply_context_rules<'a, 'b, R: ContextRule<'a>>(
    ctx: &mut hb_ot_apply_context_t,
    rules: &'b ArrayOfOffsets<'a, R, Offset16>,
    match_func: impl Fn(&mut GlyphInfo, u32) -> bool,
) -> Option<()> {
    let control = ctx.control().clone();
    if control.step(0).is_err() {
        return Default::default();
    }
    let hb_loop_control = ctx.buffer.control();
    if let Err(error) = hb_loop_control.step(0) {
        ctx.buffer.stop_storage(error);
        return Default::default();
    }
    // TODO: In HarfBuzz, the following condition makes NotoNastaliqUrdu
    // faster. But our lookup code is slower, so NOT using this condition
    // makes us faster.  Reconsider when lookup code is faster.
    //if rules.len() <= 4 {
    if false {
        let mut cursor = offsets(rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            if let Err(error) = hb_loop_control.step(1) {
                ctx.buffer.stop_storage(error);
                return Default::default();
            }
            if rule.parse().apply(ctx, &match_func).is_some() {
                return Some(());
            }
        }
        return None;
    }
    // This version is optimized for speed by matching the first & second
    // components of the rule here, instead of calling into the matching code.
    //
    // We use the iter_context instead of iter_input, to avoid skipping
    // default-ignorables and such.
    //
    // Related: https://github.com/harfbuzz/harfbuzz/issues/4813
    let mut skippy_iter = skipping_iterator_t::with_match_fn(ctx, true, Some(match_always));
    skippy_iter.reset(skippy_iter.buffer.idx);
    skippy_iter.set_glyph_data(0);
    let mut unsafe_to = None;
    let unsafe_to1;
    let mut unsafe_to2 = 0;
    let mut second = None;
    let first = if skippy_iter.next(None) {
        let g1 = skippy_iter.index();
        if skippy_iter.may_skip(&skippy_iter.buffer.info[g1]) != may_skip_t::SKIP_NO {
            // Can't use the fast path if eg. the next char is a default-ignorable
            // or other skippable.
            let mut cursor = offsets(rules);
            while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
                if let Err(error) = hb_loop_control.step(1) {
                    ctx.buffer.stop_storage(error);
                    return Default::default();
                }
                if rule.parse().apply(ctx, &match_func).is_some() {
                    return Some(());
                }
            }
            return None;
        }
        unsafe_to1 = skippy_iter.index() + 1;
        g1
    } else {
        // Failed to match a next glyph. Only try applying rules that have no
        // further impact.
        let mut cursor = offsets(rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            let rule = rule.parse();
            if !(rule.input.len() <= 1) {
                continue;
            }
            if let Err(error) = hb_loop_control.step(1) {
                ctx.buffer.stop_storage(error);
                return Default::default();
            }
            if rule.apply(ctx, &match_func).is_some() {
                return Some(());
            }
        }
        return None;
    };
    let matched = skippy_iter.next(None);
    let g2 = skippy_iter.index();
    if matched {
        second = Some(g2);
        unsafe_to2 = skippy_iter.index() + 1;
        if skippy_iter.may_skip(&skippy_iter.buffer.info[g2]) != may_skip_t::SKIP_NO {
            // Can't use the fast path if eg. the next char is a default-ignorable
            // or other skippable.
            let mut cursor = offsets(rules);
            while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
                if let Err(error) = hb_loop_control.step(1) {
                    ctx.buffer.stop_storage(error);
                    return Default::default();
                }
                if rule.parse().apply(ctx, &match_func).is_some() {
                    return Some(());
                }
            }
            return None;
        }
    }
    let mut rules_iter = offsets(rules);
    let mut rule_box =
        font_read!(control, next_valid(&mut rules_iter, &control)).map(|r| r.parse());
    while let Some(rule) = rule_box {
        if let Err(error) = hb_loop_control.step(1) {
            ctx.buffer.stop_storage(error);
            return Default::default();
        }
        let inputs = rule.input;
        let match_func2 = |info: &mut GlyphInfo, index| {
            if let Some(value) = inputs.get(index as usize).map(|v| v.get()) {
                match_func(info, u32::from(value))
            } else {
                false
            }
        };
        if inputs.len() <= 1 || match_func2(&mut ctx.buffer.info[first], 0) {
            if second.is_none()
                || (inputs.len() <= 2 || match_func2(&mut ctx.buffer.info[second.unwrap()], 1))
            {
                if rule.apply(ctx, &match_func).is_some() {
                    if let Some(unsafe_to) = unsafe_to {
                        ctx.buffer
                            .unsafe_to_concat(Some(ctx.buffer.idx), Some(unsafe_to));
                    }
                    return Some(());
                }
            } else {
                unsafe_to = Some(unsafe_to2);
            }
            rule_box =
                font_read!(control, next_valid(&mut rules_iter, &control)).map(|r| r.parse());
        } else {
            if unsafe_to.is_none() {
                unsafe_to = Some(unsafe_to1);
            }

            // Skip ahead to next possible first glyph match.
            let first_glyph_value = inputs.first().unwrap().get();
            loop {
                if let Err(error) = hb_loop_control.step(1) {
                    ctx.buffer.stop_storage(error);
                    return Default::default();
                }
                let Some(next_rule) = font_read!(control, next_valid(&mut rules_iter, &control))
                else {
                    rule_box = None;
                    break;
                };
                if next_rule.first_input() != Some(first_glyph_value) {
                    rule_box = Some(next_rule.parse());
                    break;
                }
            }
        }
    }
    if let Some(unsafe_to) = unsafe_to {
        ctx.buffer
            .unsafe_to_concat(Some(ctx.buffer.idx), Some(unsafe_to));
    }
    None
}

fn apply_chain_with_sequences<
    F1: Fn(&mut GlyphInfo, u32) -> bool,
    F2: Fn(&mut GlyphInfo, u32) -> bool,
    F3: Fn(&mut GlyphInfo, u32) -> bool,
>(
    ctx: &mut hb_ot_apply_context_t,
    rule: &ParsedRule<'_>,
    match_funcs: &(F1, F2, F3),
) -> Option<()> {
    let control = ctx.control().clone();
    if control.step(0).is_err() {
        return Default::default();
    }
    let input = rule.input;
    let f3 = |info: &mut GlyphInfo, index| {
        input
            .get(index as usize)
            .is_some_and(|value| match_funcs.1(info, value.get() as u32))
    };

    let mut end_index = ctx.buffer.idx;
    let mut match_end = 0;

    let input_matches = match_input(ctx, input.len() as u16, f3, &mut match_end, None);

    if input_matches {
        end_index = match_end;
    } else {
        ctx.buffer
            .unsafe_to_concat(Some(ctx.buffer.idx), Some(end_index));
        return None;
    }

    let lookahead = rule.lookahead;
    let f2 = |info: &mut GlyphInfo, index| {
        lookahead
            .get(index as usize)
            .is_some_and(|value| match_funcs.2(info, value.get() as u32))
    };

    if !match_lookahead(ctx, lookahead.len() as u16, f2, match_end, &mut end_index) {
        ctx.buffer
            .unsafe_to_concat(Some(ctx.buffer.idx), Some(end_index));
        return None;
    }

    let mut start_index = ctx.buffer.out_len;

    let backtrack = rule.backtrack;
    let f1 = |info: &mut GlyphInfo, index| {
        backtrack
            .get(index as usize)
            .is_some_and(|value| match_funcs.0(info, value.get() as u32))
    };

    if !match_backtrack(ctx, backtrack.len() as u16, f1, &mut start_index) {
        ctx.buffer
            .unsafe_to_concat_from_outbuffer(Some(start_index), Some(end_index));
        return None;
    }

    ctx.buffer
        .unsafe_to_break_from_outbuffer(Some(start_index), Some(end_index));
    apply_lookup(ctx, input.len(), match_end, rule.records);

    Some(())
}

fn apply_chain_context_rules<
    'a,
    'b,
    R: ContextRule<'a>,
    F1: Fn(&mut GlyphInfo, u32) -> bool,
    F2: Fn(&mut GlyphInfo, u32) -> bool,
    F3: Fn(&mut GlyphInfo, u32) -> bool,
>(
    ctx: &mut hb_ot_apply_context_t,
    rules: &'b ArrayOfOffsets<'a, R, Offset16>,
    match_funcs: (F1, F2, F3),
) -> Option<()> {
    let control = ctx.control().clone();
    if control.step(0).is_err() {
        return Default::default();
    }
    let hb_loop_control = ctx.buffer.control();
    if let Err(error) = hb_loop_control.step(0) {
        ctx.buffer.stop_storage(error);
        return Default::default();
    }
    if rules.len() <= 4 {
        let mut cursor = offsets(rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            if let Err(error) = hb_loop_control.step(1) {
                ctx.buffer.stop_storage(error);
                return Default::default();
            }
            if apply_chain_with_sequences(ctx, &rule.parse(), &match_funcs).is_some() {
                return Some(());
            }
        }
        return None;
    }
    // This version is optimized for speed by matching the first & second
    // components of the rule here, instead of calling into the matching code.
    //
    // We use the iter_context instead of iter_input, to avoid skipping
    // default-ignorables and such.
    //
    // Related: https://github.com/harfbuzz/harfbuzz/issues/4813
    let mut skippy_iter = skipping_iterator_t::with_match_fn(ctx, true, Some(match_always));
    skippy_iter.reset(skippy_iter.buffer.idx);
    skippy_iter.set_glyph_data(0);
    let mut unsafe_to = None;
    let unsafe_to1;
    let mut unsafe_to2 = 0;
    let mut second = None;
    let first = if skippy_iter.next(None) {
        let g1 = skippy_iter.index();
        if skippy_iter.may_skip(&skippy_iter.buffer.info[g1]) != may_skip_t::SKIP_NO {
            // Can't use the fast path if eg. the next char is a default-ignorable
            // or other skippable.
            let mut cursor = offsets(rules);
            while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
                if let Err(error) = hb_loop_control.step(1) {
                    ctx.buffer.stop_storage(error);
                    return Default::default();
                }
                if apply_chain_with_sequences(ctx, &rule.parse(), &match_funcs).is_some() {
                    return Some(());
                }
            }
            return None;
        }
        unsafe_to1 = skippy_iter.index() + 1;
        g1
    } else {
        // Failed to match a next glyph. Only try applying rules that have no
        // further impact.
        let mut cursor = offsets(rules);
        while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
            let rule = rule.parse();
            if !(rule.input.len() <= 1 && rule.lookahead.is_empty()) {
                continue;
            }
            if let Err(error) = hb_loop_control.step(1) {
                ctx.buffer.stop_storage(error);
                return Default::default();
            }
            if apply_chain_with_sequences(ctx, &rule, &match_funcs).is_some() {
                return Some(());
            }
        }
        return None;
    };
    let matched = skippy_iter.next(None);
    let g2 = skippy_iter.index();
    if matched {
        second = Some(g2);
        unsafe_to2 = skippy_iter.index() + 1;
        if skippy_iter.may_skip(&skippy_iter.buffer.info[g2]) != may_skip_t::SKIP_NO {
            // Can't use the fast path if eg. the next char is a default-ignorable
            // or other skippable.
            let mut cursor = offsets(rules);
            while let Some(rule) = font_read!(control, next_valid(&mut cursor, &control)) {
                if let Err(error) = hb_loop_control.step(1) {
                    ctx.buffer.stop_storage(error);
                    return Default::default();
                }
                if apply_chain_with_sequences(ctx, &rule.parse(), &match_funcs).is_some() {
                    return Some(());
                }
            }
            return None;
        }
    }
    let mut rules_iter = offsets(rules);
    let mut rule_box =
        font_read!(control, next_valid(&mut rules_iter, &control)).map(|r| r.parse());
    while let Some(rule) = rule_box {
        if let Err(error) = hb_loop_control.step(1) {
            ctx.buffer.stop_storage(error);
            return Default::default();
        }
        let input = rule.input;
        let lookahead = rule.lookahead;
        let match_input = |info: &mut GlyphInfo, index: usize| {
            input
                .get(index)
                .is_some_and(|v| match_funcs.1(info, u32::from(v.get())))
        };
        let match_lookahead = |info: &mut GlyphInfo, index: usize| {
            lookahead
                .get(index)
                .is_some_and(|v| match_funcs.2(info, u32::from(v.get())))
        };
        let len_p1 = input.len() + 1;
        let matched_first = if len_p1 > 1 {
            match_input(&mut ctx.buffer.info[first], 0)
        } else {
            lookahead.is_empty() || match_lookahead(&mut ctx.buffer.info[first], 0)
        };
        if matched_first {
            let matched_second = if let Some(second) = second {
                if len_p1 > 2 {
                    match_input(&mut ctx.buffer.info[second], 1)
                } else {
                    (lookahead.len() <= 2 - len_p1)
                        || match_lookahead(&mut ctx.buffer.info[second], 2 - len_p1)
                }
            } else {
                true
            };
            if matched_second {
                if apply_chain_with_sequences(ctx, &rule, &match_funcs).is_some() {
                    if let Some(unsafe_to) = unsafe_to {
                        ctx.buffer
                            .unsafe_to_concat(Some(ctx.buffer.idx), Some(unsafe_to));
                    }
                    return Some(());
                }
            } else {
                unsafe_to = Some(unsafe_to2);
            }

            rule_box =
                font_read!(control, next_valid(&mut rules_iter, &control)).map(|r| r.parse());
        } else {
            if unsafe_to.is_none() {
                unsafe_to = Some(unsafe_to1);
            }

            if len_p1 > 1 {
                // Skip ahead to next possible first glyph match.
                let first_glyph_value = input.first().unwrap().get();
                loop {
                    if let Err(error) = hb_loop_control.step(1) {
                        ctx.buffer.stop_storage(error);
                        return Default::default();
                    }
                    let Some(next_rule) =
                        font_read!(control, next_valid(&mut rules_iter, &control))
                    else {
                        rule_box = None;
                        break;
                    };
                    if next_rule.first_input() != Some(first_glyph_value) {
                        rule_box = Some(next_rule.parse());
                        break;
                    }
                }
            } else {
                rule_box =
                    font_read!(control, next_valid(&mut rules_iter, &control)).map(|r| r.parse());
            }
        }
    }
    if let Some(unsafe_to) = unsafe_to {
        ctx.buffer
            .unsafe_to_concat(Some(ctx.buffer.idx), Some(unsafe_to));
    }
    None
}
