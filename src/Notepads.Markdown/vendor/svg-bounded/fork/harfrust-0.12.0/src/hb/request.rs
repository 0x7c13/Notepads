// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-bound shaping handles. No paid ordinary wrapper can be extracted.

use super::{
    buffer::hb_buffer_t,
    face::{ShapeOptions, ShaperData},
    ot_shape::hb_ot_shape_planner_t,
    ot_shape_plan::{features_equivalent, hb_ot_shape_plan_t},
    storage::{Control, Inline},
};
use crate::{
    BufferClusterLevel, BufferFlags, Direction, Feature, GlyphInfo, GlyphPosition, Script,
    Variation,
};
use markdown_svg_budget::{Error, FailureKind, LimitKind, RequestControl, ResourcesRef};
use read_fonts::{
    control::TryTableProvider,
    types::{F2Dot14, Fixed},
    FontRef,
};

/// A borrowed view of this request's checked built-in font functions.
pub struct BuiltinFontFuncs<'call, 'font> {
    pub(crate) inner: &'call super::font_funcs::BuiltinFontFuncs<'font>,
    pub(crate) resources: &'call ResourcesRef,
}
impl BuiltinFontFuncs<'_, '_> {
    /// Returns the original request used by this callback.
    pub fn resources(&self) -> &ResourcesRef {
        self.resources
    }
    /// Maps a scalar value, preserving a missing glyph separately from failure.
    pub fn nominal_glyph(&self, c: u32) -> Result<Option<crate::GlyphId>, Error> {
        self.resources.checkpoint(0)?;
        self.inner.try_nominal_glyph(c)
    }
    /// Maps a Unicode variation sequence.
    pub fn variant_glyph(&self, c: u32, vs: u32) -> Result<Option<crate::GlyphId>, Error> {
        self.resources.checkpoint(0)?;
        self.inner.try_variant_glyph(c, vs)
    }
    /// Returns an unscaled horizontal advance in font units.
    pub fn advance_width(&self, g: crate::GlyphId) -> Result<i32, Error> {
        self.resources.checkpoint(0)?;
        self.inner.try_advance_width(g)
    }
    /// Returns an unscaled vertical advance in font units.
    pub fn advance_height(&self, g: crate::GlyphId) -> Result<i32, Error> {
        self.resources.checkpoint(0)?;
        self.inner.try_advance_height(g)
    }
    /// Returns an unscaled vertical origin in font units.
    pub fn vertical_origin(&self, g: crate::GlyphId) -> Result<(i32, i32), Error> {
        self.resources.checkpoint(0)?;
        self.inner.try_vertical_origin(g)
    }
    /// Returns unscaled ink extents when available.
    pub fn extents(&self, g: crate::GlyphId) -> Result<Option<crate::GlyphExtents>, Error> {
        self.resources.checkpoint(0)?;
        self.inner.try_extents(g)
    }
}

/// Typed callbacks used only by request shaping. As with the ordinary trait,
/// custom providers return metrics in the configured scale's coordinate space.
/// A provider must record a newly created error in `BuiltinFontFuncs::resources()`
/// before retiring callback-local request owners. This preserves the operation’s
/// first error if their cleanup also fails.
pub trait FontFuncs {
    /// Maps a scalar to its nominal glyph.
    fn nominal_glyph(
        &mut self,
        b: &BuiltinFontFuncs<'_, '_>,
        c: u32,
    ) -> Result<Option<crate::GlyphId>, Error> {
        b.nominal_glyph(c)
    }
    /// Maps a Unicode variation sequence.
    fn variant_glyph(
        &mut self,
        b: &BuiltinFontFuncs<'_, '_>,
        c: u32,
        vs: u32,
    ) -> Result<Option<crate::GlyphId>, Error> {
        b.variant_glyph(c, vs)
    }
    /// Returns the horizontal advance.
    fn advance_width(
        &mut self,
        b: &BuiltinFontFuncs<'_, '_>,
        g: crate::GlyphId,
    ) -> Result<i32, Error> {
        b.advance_width(g)
    }
    /// Returns the vertical advance.
    fn advance_height(
        &mut self,
        b: &BuiltinFontFuncs<'_, '_>,
        g: crate::GlyphId,
    ) -> Result<i32, Error> {
        b.advance_height(g)
    }
    /// Returns the vertical origin.
    fn vertical_origin(
        &mut self,
        b: &BuiltinFontFuncs<'_, '_>,
        g: crate::GlyphId,
    ) -> Result<(i32, i32), Error> {
        b.vertical_origin(g)
    }
    /// Returns glyph extents when present.
    fn extents(
        &mut self,
        b: &BuiltinFontFuncs<'_, '_>,
        g: crate::GlyphId,
    ) -> Result<Option<crate::GlyphExtents>, Error> {
        b.extents(g)
    }
}

fn invariant(r: &ResourcesRef) -> Error {
    r.record(Error::new(FailureKind::Invariant, LimitKind::None))
}

#[derive(Clone, Copy)]
struct FontKey<'f> {
    bytes: &'f [u8],
    directory: &'f [u8],
    index: Option<u32>,
}
impl<'f> FontKey<'f> {
    fn new(font: &FontRef<'f>) -> Self {
        Self {
            bytes: font.data().as_bytes(),
            directory: font.table_directory().offset_data().as_bytes(),
            index: font.ttc_index(),
        }
    }
    fn same(self, other: Self) -> bool {
        core::ptr::eq(self.bytes, other.bytes)
            && core::ptr::eq(self.directory, other.directory)
            && self.index == other.index
    }
}

/// Font data and caches belonging to a single request and font identity.
pub struct Data<'f> {
    font: FontRef<'f>,
    key: FontKey<'f>,
    pub(crate) data: ShaperData,
    resources: ResourcesRef,
}
impl<'f> Data<'f> {
    /// Builds request caches for this exact borrowed font identity.
    pub fn try_new(font: &FontRef<'f>, r: &ResourcesRef) -> Result<Self, Error> {
        r.checkpoint(0)?;
        let data = ShaperData::new_with_control(font, &Control(Some(r.clone())))?;
        r.checkpoint(0)?;
        Ok(Self {
            font: font.clone(),
            key: FontKey::new(font),
            data,
            resources: r.clone(),
        })
    }
}

/// Normalized variation coordinates. No ordinary Clone or owned view exists.
pub struct Instance<'f> {
    key: FontKey<'f>,
    coords: Inline<F2Dot14, 11>,
    feature_variations: [Option<u32>; 2],
    resources: ResourcesRef,
}
impl<'f> Instance<'f> {
    /// Normalizes the variations using the data handle’s font and request.
    pub fn try_from_variations<V>(data: &Data<'f>, vars: V) -> Result<Self, Error>
    where
        V: IntoIterator,
        V::Item: Into<Variation>,
    {
        let r = &data.resources;
        let checked = read_fonts::request::CheckedFontRef::from_index(
            data.key.bytes,
            data.key.index.unwrap_or(0),
            r,
        )?
        .map_err(|_| invariant(r))?;
        let mut cx = RequestControl(r);
        let mut coords = Inline::new(&Control(Some(r.clone())));
        if let Ok(fvar) = checked.try_fvar(&mut cx)? {
            coords.try_resize(fvar.axis_count() as usize, F2Dot14::ZERO)?;
            let avar = checked.try_avar(&mut cx)?.ok();
            fvar.try_user_to_normalized(
                avar.as_ref(),
                vars.into_iter()
                    .map(Into::into)
                    .map(|v| (v.tag, Fixed::from_f64(v.value as _))),
                &mut coords,
                r,
            )?;
            let mut all_default = true;
            for chunk in coords.chunks(256) {
                r.checkpoint(chunk.len() as u64)?;
                all_default &= chunk.iter().all(|v| *v == F2Dot14::ZERO);
            }
            if all_default {
                coords.clear();
            }
        } else {
            // No fvar still consumes the owned argument before the success tail.
            drop(vars);
        }
        r.checkpoint(0)?;
        let mut feature_variations = [None; 2];
        if !coords.is_empty() {
            if let Ok(t) = checked.try_expect_table::<read_fonts::tables::gsub::Gsub>(&mut cx)? {
                feature_variations[0] = super::ot::LayoutTable::Gsub(t, Control(Some(r.clone())))
                    .feature_variation_index_controlled(&coords, &Control(Some(r.clone())));
            }
            if let Ok(t) = checked.try_expect_table::<read_fonts::tables::gpos::Gpos>(&mut cx)? {
                feature_variations[1] = super::ot::LayoutTable::Gpos(t, Control(Some(r.clone())))
                    .feature_variation_index_controlled(&coords, &Control(Some(r.clone())));
            }
        }
        r.checkpoint(0)?;
        Ok(Self {
            key: data.key,
            coords,
            feature_variations,
            resources: r.clone(),
        })
    }
    /// Borrows the normalized variation coordinates.
    pub fn coords(&self) -> &[F2Dot14] {
        &self.coords
    }
}

/// Input buffer whose storage always remains attached to its request.
pub struct Buffer {
    pub(crate) core: hb_buffer_t,
    resources: ResourcesRef,
}
impl Buffer {
    /// Creates an empty input buffer in this request.
    pub fn try_new(r: &ResourcesRef) -> Result<Self, Error> {
        r.checkpoint(0)?;
        Ok(Self {
            core: hb_buffer_t::new_for_storage_test(r),
            resources: r.clone(),
        })
    }
    /// Returns the number of initialized glyph records.
    pub fn len(&self) -> usize {
        self.core.len
    }
    /// Returns whether the buffer contains no glyph records.
    pub fn is_empty(&self) -> bool {
        self.core.len == 0
    }
    /// Appends text while preserving UTF-8 byte cluster positions.
    pub fn try_push_str(&mut self, text: &str) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.push_str(text);
        self.resources.checkpoint(0)
    }
    /// Sets the bounded Unicode context before this buffer, in original order.
    pub fn try_set_pre_context(&mut self, text: &str) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.set_pre_context(text);
        self.resources.checkpoint(0)
    }
    /// Sets the bounded Unicode context following this buffer.
    pub fn try_set_post_context(&mut self, text: &str) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.set_post_context(text);
        self.resources.checkpoint(0)
    }
    /// Sets the shaping direction after checking the original request.
    pub fn try_set_direction(&mut self, direction: Direction) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.direction = direction;
        Ok(())
    }
    /// Sets the script used to choose the original shaping algorithm.
    pub fn try_set_script(&mut self, script: Script) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.script = Some(script);
        Ok(())
    }
    /// Sets the original buffer flags.
    pub fn try_set_flags(&mut self, flags: BufferFlags) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.flags = flags;
        Ok(())
    }
    /// Sets the cluster grouping policy.
    pub fn try_set_cluster_level(&mut self, level: BufferClusterLevel) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.cluster_level = match level {
            BufferClusterLevel::MonotoneGraphemes => 0,
            BufferClusterLevel::MonotoneCharacters => 1,
            BufferClusterLevel::Characters => 2,
            BufferClusterLevel::Graphemes => 3,
        };
        Ok(())
    }
    /// Fills unset script, language, and direction from the original buffer rules.
    pub fn try_guess_segment_properties(&mut self) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.guess_segment_properties();
        self.resources.checkpoint(0)
    }
    /// Copies and canonicalizes the language in request storage.
    pub fn try_set_language(&mut self, language: Option<&str>) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        let mut bytes = Inline::new(&Control(Some(self.resources.clone())));
        if let Some(language) = language.filter(|s| !s.is_empty()) {
            bytes.try_reserve(language.len())?;
            for chunk in language.as_bytes().chunks(256) {
                self.resources.checkpoint(chunk.len() as u64)?;
                for &byte in chunk {
                    bytes.try_push(if byte == b'_' {
                        b'-'
                    } else {
                        byte.to_ascii_lowercase()
                    })?;
                }
            }
        }
        self.core.request_language = (!bytes.is_empty()).then_some(bytes);
        self.resources.checkpoint(0)
    }
    /// Borrows canonical UTF-8 bytes without scanning or allocating an owned language.
    pub fn language_bytes(&self) -> Option<&[u8]> {
        self.core.request_language.as_deref()
    }
    /// Clears initialized records while retaining paid capacity and the original failure slot.
    pub fn try_clear(&mut self) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        self.core.clear();
        self.resources.checkpoint(0)
    }
}

/// Reusable, checked plan. Runtime core and key have unique owners.
pub struct Plan<'f> {
    key: FontKey<'f>,
    core: hb_ot_shape_plan_t,
    language: Inline<u8, 8>,
    features: Inline<Feature, 4>,
    coords: Inline<F2Dot14, 11>,
    feature_variations: [Option<u32>; 2],
    resources: ResourcesRef,
}
impl<'f> Plan<'f> {
    /// Builds a reusable plan bound to this font, request, and complete shaping key.
    pub fn try_new(
        data: &Data<'f>,
        instance: Option<&Instance<'f>>,
        buffer: &Buffer,
        features: &[Feature],
    ) -> Result<Self, Error> {
        let r = &data.resources;
        validate(r, data, instance, buffer)?;
        if buffer.core.direction == Direction::Invalid {
            return Err(invariant(r));
        }
        let shaper = data
            .data
            .shaper(&data.font)
            .instance_view(instance.map(|i| (&*i.coords, i.feature_variations)))
            .try_build()?;
        let mut planner = hb_ot_shape_planner_t::new(
            &shaper,
            buffer.core.direction,
            buffer.core.script,
            buffer.core.request_language.as_deref(),
        );
        planner.collect_features(features);
        r.checkpoint(0)?;
        let core = planner.try_compile(features)?;
        r.checkpoint(0)?;
        let control = Control(Some(r.clone()));
        let mut language = Inline::new(&control);
        if let Some(s) = buffer.language_bytes() {
            language.try_extend(s)?;
        }
        let mut key_features = Inline::new_with_value(
            &control,
            Feature {
                tag: crate::Tag::new(b"    "),
                value: 0,
                start: 0,
                end: 0,
            },
        );
        key_features.try_extend(features)?;
        let mut coords = Inline::new(&control);
        if let Some(instance) = instance {
            coords.try_extend(&instance.coords)?;
        }
        r.checkpoint(0)?;
        Ok(Self {
            key: data.key,
            core,
            language,
            features: key_features,
            coords,
            feature_variations: instance.map(|i| i.feature_variations).unwrap_or_default(),
            resources: r.clone(),
        })
    }
    fn matches(
        &self,
        r: &ResourcesRef,
        data: &Data<'f>,
        instance: Option<&Instance<'f>>,
        buffer: &Buffer,
        features: &[Feature],
    ) -> Result<bool, Error> {
        r.checkpoint(0)?;
        if !r.is_same_request(&self.resources)
            || !self.key.same(data.key)
            || self.core.direction != buffer.core.direction
            || self.core.script != buffer.core.script
        {
            return Ok(false);
        }
        let language = buffer.language_bytes().unwrap_or_default();
        if self.language.len() != language.len()
            || self.coords.len() != instance.map_or(0, |i| i.coords.len())
            || self.features.len() != features.len()
        {
            return Ok(false);
        }
        for (a, b) in self.language.chunks(256).zip(language.chunks(256)) {
            r.checkpoint(a.len() as u64)?;
            if a != b {
                return Ok(false);
            }
        }
        let coords = instance.map_or(&[][..], |i| &*i.coords);
        for (a, b) in self.coords.chunks(256).zip(coords.chunks(256)) {
            r.checkpoint(a.len() as u64)?;
            if a != b {
                return Ok(false);
            }
        }
        for (a, b) in self.features.chunks(256).zip(features.chunks(256)) {
            r.checkpoint(a.len() as u64)?;
            if !features_equivalent(a, b) {
                return Ok(false);
            }
        }
        Ok(self.feature_variations == instance.map(|i| i.feature_variations).unwrap_or_default())
    }
}

fn validate<'f>(
    r: &ResourcesRef,
    data: &Data<'f>,
    instance: Option<&Instance<'f>>,
    buffer: &Buffer,
) -> Result<(), Error> {
    r.checkpoint(0)?;
    if !r.is_same_request(&data.resources)
        || !r.is_same_request(&buffer.resources)
        || !buffer.core.successful
    {
        return Err(invariant(r));
    }
    if let Some(i) = instance {
        if !r.is_same_request(&i.resources) || !i.key.same(data.key) {
            return Err(invariant(r));
        }
    }
    Ok(())
}

/// Shaped glyph storage. All borrowed views are tied only to this output.
pub struct Output {
    core: hb_buffer_t,
    resources: ResourcesRef,
}
impl Output {
    /// Returns the number of initialized glyph records.
    pub fn len(&self) -> usize {
        self.core.len
    }
    /// Returns whether the buffer contains no glyph records.
    pub fn is_empty(&self) -> bool {
        self.core.len == 0
    }
    /// Borrows every glyph information record from this output.
    pub fn glyph_infos(&self) -> &[GlyphInfo] {
        &self.core.info[..self.core.len]
    }
    /// Borrows every glyph position record from this output.
    pub fn glyph_positions(&self) -> &[GlyphPosition] {
        &self.core.pos[..self.core.len]
    }
    /// Clears initialized records while retaining paid capacity and the original failure slot.
    pub fn try_clear(mut self) -> Result<Buffer, Error> {
        self.resources.checkpoint(0)?;
        self.core.clear();
        self.resources.checkpoint(0)?;
        Ok(Buffer {
            core: self.core,
            resources: self.resources,
        })
    }
}

/// Shapes with the font bound into Data and exactly the owners' original request.
pub fn try_shape<'f>(
    r: &ResourcesRef,
    data: &Data<'f>,
    instance: Option<&Instance<'f>>,
    buffer: Buffer,
    options: ShapeOptions<'_>,
    plan: Option<&Plan<'f>>,
) -> Result<Output, Error> {
    validate(r, data, instance, &buffer)?;
    if options.plan.is_some() || options.font_funcs.is_some() {
        return Err(invariant(r));
    }
    let shaper = data
        .data
        .shaper(&data.font)
        .instance_view(instance.map(|i| (&*i.coords, i.feature_variations)))
        .try_build()?;
    let core = if let Some(plan) = plan {
        if !plan.matches(r, data, instance, &buffer, options.features)? {
            return Err(invariant(r));
        }
        shaper.shape_core(&plan.core, buffer.core, options)
    } else {
        // Scope guarantees that the automatic plan retires before Output or
        // the next caller action can be published.
        let plan = Plan::try_new(data, instance, &buffer, options.features)?;
        let core = shaper.shape_core(&plan.core, buffer.core, options);
        drop(plan);
        core
    };
    r.checkpoint(0)?;
    if !core.successful {
        return Err(invariant(r));
    }
    Ok(Output {
        core,
        resources: r.clone(),
    })
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
