// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real FontFuncs forwarding with the original glyph-domain guard. Observation
//! rows and serialization do not belong to a production shape operation.
#[cfg(feature = "quality-observation")]
use crate::{
    abi::Failure, memory::failure_from_resource, observation::ObservationSink,
    quality_trace::INPUT_WORK_WINDOW,
};
use crate::{abi::Result, font::NativeFontFuncs, memory::Context};
use harfrust::{
    GlyphExtents, GlyphId,
    font::{BuiltinFontFuncs, FontFuncs},
};
#[cfg(feature = "quality-observation")]
use markdown_svg_budget::{BudgetVec, ResourceKind, ResourcesRef};
#[cfg(feature = "quality-observation")]
use std::fmt::Write;

#[cfg(feature = "quality-observation")]
#[derive(Clone, Copy, Default)]
struct NominalRow {
    consumed_mask: u32,
    sampled: bool,
    emitted: bool,
    advance: [i32; 2],
    origin: [i32; 2],
    extents: [i32; 4],
    has_extents: bool,
}

pub struct ObservedFuncs<'a> {
    native: Option<NativeFontFuncs<'a>>,
    glyph_count: u32,
    fault: bool,
    first_failure: markdown_svg_budget::FirstFailureRef,
    #[cfg(feature = "quality-observation")]
    observation_rows: Option<BudgetVec<NominalRow>>,
    #[cfg(feature = "quality-observation")]
    observation_emitted: bool,
}
impl<'a> ObservedFuncs<'a> {
    pub fn new(
        context: &Context,
        glyph_count: u32,
        native: Option<NativeFontFuncs<'a>>,
    ) -> Result<Self> {
        Ok(Self {
            native,
            glyph_count,
            fault: false,
            first_failure: context.first_failure_ref(),
            #[cfg(feature = "quality-observation")]
            observation_rows: None,
            #[cfg(feature = "quality-observation")]
            observation_emitted: false,
        })
    }
    fn check(&mut self, glyph: GlyphId) {
        if glyph.to_u32() >= self.glyph_count {
            self.fault = true;
            self.first_failure
                .record(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Invariant,
                    markdown_svg_budget::LimitKind::None,
                ));
        }
    }
    pub fn failed(&self) -> bool {
        self.fault || self.native.as_ref().is_some_and(|native| native.fault)
    }
    #[cfg(feature = "quality-observation")]
    fn diagnostic_failure(&mut self) -> Failure {
        self.fault = true;
        failure_from_resource(
            self.first_failure
                .record(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Invariant,
                    markdown_svg_budget::LimitKind::None,
                )),
        )
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn enable_observation(&mut self, resources: &ResourcesRef) -> Result<()> {
        if self.observation_rows.is_some() || self.glyph_count == 0 || self.glyph_count > 65535 {
            return Err(self.diagnostic_failure());
        }
        let mut rows = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        rows.try_reserve_exact(self.glyph_count as usize)
            .map_err(failure_from_resource)?;
        for index in 0..self.glyph_count {
            if index % 256 == 0 {
                resources.checkpoint(256).map_err(failure_from_resource)?;
            }
            rows.try_push(NominalRow::default())
                .map_err(failure_from_resource)?;
        }
        self.observation_rows = Some(rows);
        Ok(())
    }
    #[cfg(feature = "quality-observation")]
    fn sample(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId, consumed_mask: u32) {
        let Some(rows) = &mut self.observation_rows else {
            return;
        };
        let Some(row) = rows.get_mut(glyph.to_u32() as usize) else {
            self.diagnostic_failure();
            return;
        };
        // Only the real FontFuncs entry contributes a consumption bit. Calls
        // below sample the same backend directly and do not re-enter this layer.
        row.consumed_mask |= consumed_mask;
        if row.sampled {
            return;
        }
        let (width, height, origin, extents) = if let Some(native) = &mut self.native {
            (
                native.advance_width(builtin, glyph),
                native.advance_height(builtin, glyph),
                native.vertical_origin(builtin, glyph),
                native.extents(builtin, glyph),
            )
        } else {
            (
                builtin.advance_width(glyph),
                builtin.advance_height(glyph),
                builtin.vertical_origin(glyph),
                builtin.extents(glyph),
            )
        };
        row.advance = [width, height];
        row.origin = [origin.0, origin.1];
        if let Some(value) = extents {
            row.extents = [value.x_bearing, value.y_bearing, value.width, value.height];
            row.has_extents = true;
        }
        row.sampled = true;
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn emit_used(
        &mut self,
        sink: &ObservationSink,
        draft: u64,
        face: u32,
        orientation: u32,
        gids: impl Iterator<Item = u32>,
    ) -> Result<()> {
        if !sink.is_enabled() {
            return Ok(());
        }
        if self.observation_emitted || self.observation_rows.is_none() || orientation > 1 {
            return Err(self.diagnostic_failure());
        }
        self.observation_emitted = true;
        let source = if self.native.is_some() {
            "Native-table-real-FontFuncs"
        } else {
            "HarfRust-Builtin-real-FontFuncs"
        };
        let rows = self
            .observation_rows
            .as_mut()
            .expect("checked observation table");
        sink.record(|trace| {
            write!(trace,"{{\"kind\":\"actualNominalBatch\",\"draftId\":{draft},\"faceTicket\":{face},\"orientation\":{orientation},\"source\":\"{source}\",\"maskBits\":\"advanceWidth=1,advanceHeight=2,verticalOrigin=4,extents=8\",\"rows\":[").map_err(|_| Failure::Contract)?;
            let mut comma = false;
            for (index, gid) in gids.enumerate() {
                if index % INPUT_WORK_WINDOW == 0 {
                    trace.checkpoint_input_window()?;
                }
                let row = rows.get_mut(gid as usize).ok_or(Failure::Contract)?;
                if row.emitted { continue }
                row.emitted = true;
                if comma { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                comma = true;
                write!(trace,"{{\"glyph\":{gid},\"hbConsumedMask\":{},\"sameBackendSampled\":{},\"advance\":[{},{}],\"verticalOrigin\":[{},{}],\"extents\":",row.consumed_mask,row.sampled,row.advance[0],row.advance[1],row.origin[0],row.origin[1]).map_err(|_| Failure::Contract)?;
                if row.has_extents {
                    write!(trace,"[{},{},{},{}]",row.extents[0],row.extents[1],row.extents[2],row.extents[3]).map_err(|_| Failure::Contract)?;
                } else { trace.write_str("null").map_err(|_| Failure::Contract)?; }
                trace.write_char('}').map_err(|_| Failure::Contract)?;
            }
            trace.write_str("]}\n").map_err(|_| Failure::Contract)
        })
    }
}
impl FontFuncs for ObservedFuncs<'_> {
    fn nominal_glyph(&mut self, builtin: &BuiltinFontFuncs, c: u32) -> Option<GlyphId> {
        let value = builtin.nominal_glyph(c);
        if let Some(glyph) = value {
            self.check(glyph);
            #[cfg(feature = "quality-observation")]
            self.sample(builtin, glyph, 0);
        }
        value
    }
    fn variant_glyph(&mut self, builtin: &BuiltinFontFuncs, c: u32, vs: u32) -> Option<GlyphId> {
        let value = builtin.variant_glyph(c, vs);
        if let Some(glyph) = value {
            self.check(glyph);
            #[cfg(feature = "quality-observation")]
            self.sample(builtin, glyph, 0);
        }
        value
    }
    fn advance_width(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> i32 {
        self.check(glyph);
        #[cfg(feature = "quality-observation")]
        self.sample(builtin, glyph, 1);
        self.native.as_mut().map_or_else(
            || builtin.advance_width(glyph),
            |native| native.advance_width(builtin, glyph),
        )
    }
    fn advance_height(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> i32 {
        self.check(glyph);
        #[cfg(feature = "quality-observation")]
        self.sample(builtin, glyph, 2);
        self.native.as_mut().map_or_else(
            || builtin.advance_height(glyph),
            |native| native.advance_height(builtin, glyph),
        )
    }
    fn vertical_origin(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> (i32, i32) {
        self.check(glyph);
        #[cfg(feature = "quality-observation")]
        self.sample(builtin, glyph, 4);
        self.native.as_mut().map_or_else(
            || builtin.vertical_origin(glyph),
            |native| native.vertical_origin(builtin, glyph),
        )
    }
    fn extents(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> Option<GlyphExtents> {
        self.check(glyph);
        #[cfg(feature = "quality-observation")]
        self.sample(builtin, glyph, 8);
        self.native.as_mut().map_or_else(
            || builtin.extents(glyph),
            |native| native.extents(builtin, glyph),
        )
    }
}

impl ObservedFuncs<'_> {
    fn try_check(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: GlyphId,
    ) -> core::result::Result<(), markdown_svg_budget::Error> {
        builtin.resources().checkpoint(0)?;
        self.check(glyph);
        if self.fault {
            return Err(builtin
                .resources()
                .record(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Invariant,
                    markdown_svg_budget::LimitKind::None,
                )));
        }
        Ok(())
    }

    #[cfg(feature = "quality-observation")]
    fn try_sample(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: GlyphId,
        consumed_mask: u32,
    ) -> core::result::Result<(), markdown_svg_budget::Error> {
        let Some(rows) = &mut self.observation_rows else {
            return Ok(());
        };
        let Some(row) = rows.get_mut(glyph.to_u32() as usize) else {
            self.fault = true;
            return Err(builtin
                .resources()
                .record(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Invariant,
                    markdown_svg_budget::LimitKind::None,
                )));
        };
        row.consumed_mask |= consumed_mask;
        if row.sampled {
            return Ok(());
        }
        // Sample the very same checked backend; these calls do not re-enter
        // ObservedFuncs or manufacture additional consumption bits.
        let (width, height, origin, extents) = if let Some(native) = &mut self.native {
            (
                harfrust::request::FontFuncs::advance_width(native, builtin, glyph)?,
                harfrust::request::FontFuncs::advance_height(native, builtin, glyph)?,
                harfrust::request::FontFuncs::vertical_origin(native, builtin, glyph)?,
                harfrust::request::FontFuncs::extents(native, builtin, glyph)?,
            )
        } else {
            (
                builtin.advance_width(glyph)?,
                builtin.advance_height(glyph)?,
                builtin.vertical_origin(glyph)?,
                builtin.extents(glyph)?,
            )
        };
        row.advance = [width, height];
        row.origin = [origin.0, origin.1];
        if let Some(value) = extents {
            row.extents = [value.x_bearing, value.y_bearing, value.width, value.height];
            row.has_extents = true;
        }
        row.sampled = true;
        builtin.resources().checkpoint(0)
    }
}

impl harfrust::request::FontFuncs for ObservedFuncs<'_> {
    fn nominal_glyph(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        c: u32,
    ) -> core::result::Result<Option<GlyphId>, markdown_svg_budget::Error> {
        let value = builtin.nominal_glyph(c)?;
        if let Some(glyph) = value {
            self.try_check(builtin, glyph)?;
            #[cfg(feature = "quality-observation")]
            self.try_sample(builtin, glyph, 0)?;
        }
        Ok(value)
    }
    fn variant_glyph(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        c: u32,
        vs: u32,
    ) -> core::result::Result<Option<GlyphId>, markdown_svg_budget::Error> {
        let value = builtin.variant_glyph(c, vs)?;
        if let Some(glyph) = value {
            self.try_check(builtin, glyph)?;
            #[cfg(feature = "quality-observation")]
            self.try_sample(builtin, glyph, 0)?;
        }
        Ok(value)
    }
    fn advance_width(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: GlyphId,
    ) -> core::result::Result<i32, markdown_svg_budget::Error> {
        self.try_check(builtin, glyph)?;
        #[cfg(feature = "quality-observation")]
        self.try_sample(builtin, glyph, 1)?;
        if let Some(native) = &mut self.native {
            harfrust::request::FontFuncs::advance_width(native, builtin, glyph)
        } else {
            builtin.advance_width(glyph)
        }
    }
    fn advance_height(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: GlyphId,
    ) -> core::result::Result<i32, markdown_svg_budget::Error> {
        self.try_check(builtin, glyph)?;
        #[cfg(feature = "quality-observation")]
        self.try_sample(builtin, glyph, 2)?;
        if let Some(native) = &mut self.native {
            harfrust::request::FontFuncs::advance_height(native, builtin, glyph)
        } else {
            builtin.advance_height(glyph)
        }
    }
    fn vertical_origin(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: GlyphId,
    ) -> core::result::Result<(i32, i32), markdown_svg_budget::Error> {
        self.try_check(builtin, glyph)?;
        #[cfg(feature = "quality-observation")]
        self.try_sample(builtin, glyph, 4)?;
        if let Some(native) = &mut self.native {
            harfrust::request::FontFuncs::vertical_origin(native, builtin, glyph)
        } else {
            builtin.vertical_origin(glyph)
        }
    }
    fn extents(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: GlyphId,
    ) -> core::result::Result<Option<GlyphExtents>, markdown_svg_budget::Error> {
        self.try_check(builtin, glyph)?;
        #[cfg(feature = "quality-observation")]
        self.try_sample(builtin, glyph, 8)?;
        if let Some(native) = &mut self.native {
            harfrust::request::FontFuncs::extents(native, builtin, glyph)
        } else {
            builtin.extents(glyph)
        }
    }
}

#[cfg(all(test, feature = "quality-observation"))]
mod tests {
    use super::*;
    use crate::{
        memory::RetainedPool,
        observation::ObservationSink,
        quality_trace::{QualityRecorder, test_support::context},
    };
    use read_fonts::TableProvider;
    use std::{cell::Cell, collections::BTreeMap, sync::atomic::Ordering};

    fn font_bytes() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("markdown-math/assets/fonts/KaTeX_Main-Regular.ttf");
        std::fs::read(path).unwrap()
    }
    fn text_buffer() -> harfrust::UnicodeBuffer {
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str("office ffi A\u{301} x+y");
        buffer.guess_segment_properties();
        buffer
    }
    fn same_shape(a: &harfrust::GlyphBuffer, b: &harfrust::GlyphBuffer) {
        assert_eq!(a.glyph_infos().len(), b.glyph_infos().len());
        for (a, b) in a.glyph_infos().iter().zip(b.glyph_infos()) {
            assert_eq!(
                (a.glyph_id, a.cluster, a.unsafe_to_break()),
                (b.glyph_id, b.cluster, b.unsafe_to_break())
            );
        }
        for (a, b) in a.glyph_positions().iter().zip(b.glyph_positions()) {
            assert_eq!(
                (a.x_advance, a.y_advance, a.x_offset, a.y_offset),
                (b.x_advance, b.y_advance, b.x_offset, b.y_offset)
            );
        }
    }
    // This outer layer witnesses only calls made by the real shaper. Backend
    // sampling inside ObservedFuncs cannot manufacture entries in this map.
    struct Witness<'a, 'font> {
        inner: &'a mut ObservedFuncs<'font>,
        masks: BTreeMap<u32, u32>,
    }
    impl FontFuncs for Witness<'_, '_> {
        fn nominal_glyph(&mut self, builtin: &BuiltinFontFuncs, ch: u32) -> Option<GlyphId> {
            let value = self.inner.nominal_glyph(builtin, ch);
            if let Some(glyph) = value {
                self.masks.entry(glyph.to_u32()).or_default();
            }
            value
        }
        fn variant_glyph(
            &mut self,
            builtin: &BuiltinFontFuncs,
            ch: u32,
            vs: u32,
        ) -> Option<GlyphId> {
            let value = self.inner.variant_glyph(builtin, ch, vs);
            if let Some(glyph) = value {
                self.masks.entry(glyph.to_u32()).or_default();
            }
            value
        }
        fn advance_width(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> i32 {
            *self.masks.entry(glyph.to_u32()).or_default() |= 1;
            self.inner.advance_width(builtin, glyph)
        }
        fn advance_height(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> i32 {
            *self.masks.entry(glyph.to_u32()).or_default() |= 2;
            self.inner.advance_height(builtin, glyph)
        }
        fn vertical_origin(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> (i32, i32) {
            *self.masks.entry(glyph.to_u32()).or_default() |= 4;
            self.inner.vertical_origin(builtin, glyph)
        }
        fn extents(&mut self, builtin: &BuiltinFontFuncs, glyph: GlyphId) -> Option<GlyphExtents> {
            *self.masks.entry(glyph.to_u32()).or_default() |= 8;
            self.inner.extents(builtin, glyph)
        }
    }
    impl harfrust::request::FontFuncs for Witness<'_, '_> {
        fn nominal_glyph(
            &mut self,
            b: &harfrust::request::BuiltinFontFuncs<'_, '_>,
            ch: u32,
        ) -> core::result::Result<Option<GlyphId>, markdown_svg_budget::Error> {
            let value = harfrust::request::FontFuncs::nominal_glyph(self.inner, b, ch)?;
            if let Some(glyph) = value {
                self.masks.entry(glyph.to_u32()).or_default();
            }
            Ok(value)
        }
        fn variant_glyph(
            &mut self,
            b: &harfrust::request::BuiltinFontFuncs<'_, '_>,
            ch: u32,
            vs: u32,
        ) -> core::result::Result<Option<GlyphId>, markdown_svg_budget::Error> {
            let value = harfrust::request::FontFuncs::variant_glyph(self.inner, b, ch, vs)?;
            if let Some(glyph) = value {
                self.masks.entry(glyph.to_u32()).or_default();
            }
            Ok(value)
        }
        fn advance_width(
            &mut self,
            b: &harfrust::request::BuiltinFontFuncs<'_, '_>,
            glyph: GlyphId,
        ) -> core::result::Result<i32, markdown_svg_budget::Error> {
            *self.masks.entry(glyph.to_u32()).or_default() |= 1;
            harfrust::request::FontFuncs::advance_width(self.inner, b, glyph)
        }
        fn advance_height(
            &mut self,
            b: &harfrust::request::BuiltinFontFuncs<'_, '_>,
            glyph: GlyphId,
        ) -> core::result::Result<i32, markdown_svg_budget::Error> {
            *self.masks.entry(glyph.to_u32()).or_default() |= 2;
            harfrust::request::FontFuncs::advance_height(self.inner, b, glyph)
        }
        fn vertical_origin(
            &mut self,
            b: &harfrust::request::BuiltinFontFuncs<'_, '_>,
            glyph: GlyphId,
        ) -> core::result::Result<(i32, i32), markdown_svg_budget::Error> {
            *self.masks.entry(glyph.to_u32()).or_default() |= 4;
            harfrust::request::FontFuncs::vertical_origin(self.inner, b, glyph)
        }
        fn extents(
            &mut self,
            b: &harfrust::request::BuiltinFontFuncs<'_, '_>,
            glyph: GlyphId,
        ) -> core::result::Result<Option<GlyphExtents>, markdown_svg_budget::Error> {
            *self.masks.entry(glyph.to_u32()).or_default() |= 8;
            harfrust::request::FontFuncs::extents(self.inner, b, glyph)
        }
    }

    #[test]
    fn request_provider_preserves_every_glyph_word_and_real_consumption_bits() {
        let bytes = font_bytes();
        let ordinary_font = harfrust::FontRef::from_index(&bytes, 0).unwrap();
        let ordinary_data = harfrust::ShaperData::new(&ordinary_font);
        for text in [
            "office ffi A\u{301} x+y",
            "ABC 123 אבג",
            "中文 😀 \u{2066}abc\u{2069}",
        ] {
            let mut plain = harfrust::UnicodeBuffer::new();
            plain.push_str(text);
            plain.set_pre_context("before ");
            plain.set_post_context(" after");
            plain.guess_segment_properties();
            let expected = ordinary_data
                .shaper(&ordinary_font)
                .build()
                .shape(plain, harfrust::ShapeOptions::new());

            let (context, host) = context();
            let resources = context.make_resources(markdown_svg_budget::Limits::default());
            {
                let font = harfrust::FontRef::try_from_index(
                    &bytes,
                    0,
                    &mut markdown_svg_budget::RequestControl(&resources),
                )
                .unwrap()
                .unwrap();
                let data = harfrust::request::Data::try_new(&font, &resources).unwrap();
                let mut buffer = harfrust::request::Buffer::try_new(&resources).unwrap();
                buffer.try_push_str(text).unwrap();
                buffer.try_set_pre_context("before ").unwrap();
                buffer.try_set_post_context(" after").unwrap();
                buffer.try_guess_segment_properties().unwrap();
                let mut funcs = ObservedFuncs::new(
                    &context,
                    u32::from(ordinary_font.maxp().unwrap().num_glyphs()),
                    None,
                )
                .unwrap();
                funcs.enable_observation(&resources).unwrap();
                let mut witness = Witness {
                    inner: &mut funcs,
                    masks: BTreeMap::new(),
                };
                let actual = harfrust::request::try_shape(
                    &resources,
                    &data,
                    None,
                    buffer,
                    harfrust::ShapeOptions::new().request_font_funcs(Some(&mut witness)),
                    None,
                )
                .unwrap();
                // Debug is derived on both Pod types and includes their private
                // mask/vars/attachment words as well as all public coordinates.
                assert_eq!(
                    format!("{:?}", actual.glyph_infos()),
                    format!("{:?}", expected.glyph_infos())
                );
                assert_eq!(
                    format!("{:?}", actual.glyph_positions()),
                    format!("{:?}", expected.glyph_positions())
                );
                let masks = std::mem::take(&mut witness.masks);
                drop(witness);
                for (gid, row) in funcs
                    .observation_rows
                    .as_ref()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.sampled)
                {
                    assert_eq!(
                        row.consumed_mask,
                        masks.get(&(gid as u32)).copied().unwrap_or(0)
                    );
                }
                assert!(!funcs.failed());
            }
            resources.checkpoint(0).unwrap();
            resources.validate_retired().unwrap();
            host.assert_empty();
            context.validate_released().unwrap();
        }
    }

    #[test]
    fn request_provider_domain_failure_remains_typed_after_all_shape_owners_drop() {
        let bytes = font_bytes();
        let (context, host) = context();
        let resources = context.make_resources(markdown_svg_budget::Limits::default());
        {
            let font = harfrust::FontRef::try_from_index(
                &bytes,
                0,
                &mut markdown_svg_budget::RequestControl(&resources),
            )
            .unwrap()
            .unwrap();
            let data = harfrust::request::Data::try_new(&font, &resources).unwrap();
            let mut buffer = harfrust::request::Buffer::try_new(&resources).unwrap();
            buffer.try_push_str("office").unwrap();
            buffer.try_guess_segment_properties().unwrap();
            let mut funcs = ObservedFuncs::new(&context, 1, None).unwrap();
            let result = harfrust::request::try_shape(
                &resources,
                &data,
                None,
                buffer,
                harfrust::ShapeOptions::new().request_font_funcs(Some(&mut funcs)),
                None,
            );
            assert!(result.is_err());
            assert!(funcs.failed());
            let failure = resources.failure().unwrap();
            assert_eq!(failure.kind, markdown_svg_budget::FailureKind::Invariant);
            assert_eq!(context.resource_failure(), Some(failure));
        }
        assert_eq!(resources.statistics().live_leases, 0);
        assert_eq!(resources.statistics().scratch_bytes, 0);
        host.assert_empty();
        context.validate_released().unwrap();
    }

    #[test]
    fn real_hb_output_is_unchanged_and_sampling_does_not_invent_consumed_bits() {
        let (context, host) = context();
        let resources = context.make_resources(markdown_svg_budget::Limits::default());
        let retained = RetainedPool::new(&context, &resources).unwrap();
        let recorder = retained
            .share(QualityRecorder::new(&context, 32768).unwrap())
            .unwrap();
        let mut sink = ObservationSink::default();
        sink.install(recorder).unwrap();
        let bytes = font_bytes();
        let font = harfrust::FontRef::from_index(&bytes, 0).unwrap();
        let data = harfrust::ShaperData::new(&font);
        let shaper = data.shaper(&font).build();
        let baseline = shaper.shape(text_buffer(), harfrust::ShapeOptions::new());
        let mut funcs =
            ObservedFuncs::new(&context, u32::from(font.maxp().unwrap().num_glyphs()), None)
                .unwrap();
        funcs.enable_observation(&resources).unwrap();
        let mut witness = Witness {
            inner: &mut funcs,
            masks: BTreeMap::new(),
        };
        let actual = shaper.shape(
            text_buffer(),
            harfrust::ShapeOptions::new().font_funcs(Some(&mut witness)),
        );
        let masks = std::mem::take(&mut witness.masks);
        drop(witness);
        same_shape(&baseline, &actual);
        assert!(!funcs.failed());
        let rows = funcs.observation_rows.as_ref().unwrap();
        assert!(rows.iter().any(|row| row.sampled && row.consumed_mask != 0));
        for (gid, row) in rows.iter().enumerate().filter(|(_, row)| row.sampled) {
            assert_eq!(
                row.consumed_mask,
                masks.get(&(gid as u32)).copied().unwrap_or(0)
            );
        }
        funcs
            .emit_used(
                &sink,
                1,
                2,
                0,
                actual.glyph_infos().iter().map(|info| info.glyph_id),
            )
            .unwrap();
        let info = sink.info().unwrap();
        assert_eq!(info.events, 1);
        let mut copied = vec![0; info.bytes as usize];
        assert_eq!(sink.copy(&mut copied).unwrap(), copied.len());
        let record = std::str::from_utf8(&copied).unwrap();
        assert!(record.contains("\"actualNominalBatch\"") && record.contains("\"orientation\":0"));
        assert!(record.contains("\"sameBackendSampled\":true"));
        let before = copied.clone();
        let mut no_capacity = [];
        assert!(sink.copy(&mut no_capacity).is_err());
        assert_eq!(sink.copy(&mut copied).unwrap(), copied.len());
        assert_eq!(copied, before);
        drop(actual);
        drop(funcs);
        drop(sink);
        drop(retained);
        assert_eq!(resources.statistics().live_leases, 0);
        drop(resources);
        host.assert_empty();
        context.validate_released().unwrap();
    }
    #[test]
    fn disabled_diagnostics_keep_real_hb_forwarding_without_host_allocations() {
        let (context, host) = context();
        let bytes = font_bytes();
        let font = harfrust::FontRef::from_index(&bytes, 0).unwrap();
        let data = harfrust::ShaperData::new(&font);
        let shaper = data.shaper(&font).build();
        let baseline = shaper.shape(text_buffer(), harfrust::ShapeOptions::new());
        let mut funcs =
            ObservedFuncs::new(&context, u32::from(font.maxp().unwrap().num_glyphs()), None)
                .unwrap();
        let actual = shaper.shape(
            text_buffer(),
            harfrust::ShapeOptions::new().font_funcs(Some(&mut funcs)),
        );
        same_shape(&baseline, &actual);
        assert!(!funcs.failed() && funcs.observation_rows.is_none());
        host.assert_empty();
        assert_eq!(host.committed.load(Ordering::Relaxed), 0);
        assert_eq!(host.released.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn all_65535_duplicate_gids_are_charged_while_emitting_one_unchanged_row() {
        let (context, host) = context();
        let resources = context.make_resources(markdown_svg_budget::Limits::default());
        let retained = RetainedPool::new(&context, &resources).unwrap();
        let recorder = retained
            .share(QualityRecorder::new(&context, 4096).unwrap())
            .unwrap();
        let mut sink = ObservationSink::default();
        sink.install(recorder).unwrap();
        let mut funcs = ObservedFuncs::new(&context, 2, None).unwrap();
        funcs.enable_observation(&resources).unwrap();
        let consumed = funcs.observation_rows.as_ref().unwrap()[1].consumed_mask;
        let seen = Cell::new(0usize);
        let before_work = host.work.load(Ordering::Relaxed);
        let before_checks = host.checkpoints.load(Ordering::Relaxed);
        funcs
            .emit_used(
                &sink,
                1,
                2,
                0,
                std::iter::repeat_n(1, 65535).inspect(|_| seen.set(seen.get() + 1)),
            )
            .unwrap();
        assert_eq!(seen.get(), 65535);
        // One record gate, one output window, and 256 separately prepaid
        // input windows. Suppressed rows never bypass the input work gate.
        assert_eq!(
            host.work.load(Ordering::Relaxed) - before_work,
            1 + 4096 + 65536
        );
        assert_eq!(
            host.checkpoints.load(Ordering::Relaxed) - before_checks,
            258
        );
        let rows = funcs.observation_rows.as_ref().unwrap();
        assert_eq!(rows.iter().filter(|row| row.emitted).count(), 1);
        assert_eq!(rows[1].consumed_mask, consumed);
        let info = sink.info().unwrap();
        assert_eq!((info.events, info.error), (1, 0));
        let mut copied = vec![0; info.bytes as usize];
        sink.copy(&mut copied).unwrap();
        let record = std::str::from_utf8(&copied).unwrap();
        assert_eq!(record.matches("\"glyph\":1,").count(), 1);
        assert!(record.contains("\"hbConsumedMask\":0,"));
        drop(funcs);
        drop(sink);
        drop(retained);
        assert_eq!(resources.statistics().live_leases, 0);
        drop(resources);
        host.assert_empty();
        context.validate_released().unwrap();
    }
    #[test]
    fn duplicate_gid_scan_cancels_midway_without_losing_paid_work_or_complete_prefix() {
        let (context, host) = context();
        let resources = context.make_resources(markdown_svg_budget::Limits::default());
        let retained = RetainedPool::new(&context, &resources).unwrap();
        let recorder = retained
            .share(QualityRecorder::new(&context, 4096).unwrap())
            .unwrap();
        let mut sink = ObservationSink::default();
        sink.install(recorder).unwrap();
        let mut funcs = ObservedFuncs::new(&context, 2, None).unwrap();
        funcs.enable_observation(&resources).unwrap();
        sink.record(|trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
            .unwrap();
        let seen = Cell::new(0usize);
        let before = host.work.load(Ordering::Relaxed);
        host.maximum_work.store(
            before + 1 + (INPUT_WORK_WINDOW * 3) as u64,
            Ordering::Relaxed,
        );
        let result = funcs.emit_used(
            &sink,
            1,
            2,
            0,
            std::iter::repeat_n(1, 65535).inspect(|_| seen.set(seen.get() + 1)),
        );
        let fault = context.resource_failure().unwrap();
        assert_eq!(
            (fault.kind, fault.limit),
            (
                markdown_svg_budget::FailureKind::Cancelled,
                markdown_svg_budget::LimitKind::None
            )
        );
        assert_eq!(result, Err(failure_from_resource(fault)));
        // The for-loop obtains the next GID before checking its new window,
        // but never processes it or accesses its row after cancellation.
        assert_eq!(seen.get(), INPUT_WORK_WINDOW * 3 + 1);
        assert_eq!(
            host.work.load(Ordering::Relaxed) - before,
            1 + (INPUT_WORK_WINDOW * 3) as u64
        );
        let rows = funcs.observation_rows.as_ref().unwrap();
        assert_eq!(rows.iter().filter(|row| row.emitted).count(), 1);
        assert_eq!(rows[1].consumed_mask, 0);
        let before_reads = host.checkpoints.load(Ordering::Relaxed);
        let info = sink.info().unwrap();
        assert_eq!((info.bytes, info.events, info.error), (3, 1, 4));
        let mut copied = [0xA5; 8];
        assert_eq!(sink.copy(&mut copied).unwrap(), 3);
        assert_eq!(&copied[..3], b"{}\n");
        assert_eq!(&copied[3..], &[0xA5; 5]);
        assert_eq!(host.checkpoints.load(Ordering::Relaxed), before_reads);
        assert_eq!(context.resource_failure(), Some(fault));
        drop(funcs);
        drop(sink);
        drop(retained);
        assert_eq!(resources.statistics().live_leases, 0);
        drop(resources);
        host.assert_empty();
        context.validate_released().unwrap();
    }
}
