// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

#[cfg(feature = "text")]
use super::font_cache::{ColrCache, OpszCache, RasterCache, SvgCache};
use super::reference::{PaintProof, RefFrames, RefPath};
use super::resource_cache::{ClipCache, FilterCache, IdSet, MaskCache, PaintCache};
use crate::tree::owner::{Origin, PolicyValue, TreeString, TreeVec, TryCopy};
use std::hash::Hasher;
use std::str::FromStr;
use std::sync::Arc;

#[cfg(feature = "text")]
use crate::{FontVariation, GlyphId};
#[cfg(feature = "text")]
use fontdb::Database;
#[cfg(feature = "text")]
use fontdb::ID;
use svgtypes::{Length, LengthUnit as Unit, PaintOrderKind, TransformOrigin};

use super::svgtree::{self, AId, EId, FromValue, SvgNode};
use super::units::{self, convert_length};
use super::{marker, Error, Options};
#[cfg(feature = "text")]
use crate::flatten::BitmapImage;
use crate::parser::paint_server::process_paint;
#[cfg(feature = "text")]
use crate::text::flatten::DatabaseExt;
use crate::*;

pub struct State<'a> {
    pub(super) refs: RefPath,
    pub(super) context_proofs: [PaintProof; 2],
    pub(crate) parent_clip_path: Option<SvgNode<'a, 'a>>,
    pub(crate) parent_markers: TreeVec<SvgNode<'a, 'a>>,
    /// Stores the resolved fill and stroke of a use node
    /// or a path element (for markers)
    pub(crate) context_element: Option<(Option<Fill>, Option<Stroke>)>,
    pub(crate) fe_image_link: bool,
    /// A viewBox of the parent SVG element.
    pub(crate) view_box: NonZeroRect,
    /// A size of the parent `use` element.
    /// Used only during nested `svg` size resolving.
    /// Width and height can be set independently.
    pub(crate) use_size: (Option<f32>, Option<f32>),
    pub(crate) opt: &'a Options<'a>,
}

impl<'a> State<'a> {
    pub(crate) fn try_clone(&self) -> std::result::Result<Self, markdown_svg_budget::Error> {
        let p = &self.opt.processing;
        if let Some(parent) = self.parent_clip_path {
            parent.validate_policy(p)?;
        }
        let result = (|| {
            let parent_markers = self.parent_markers.try_copy_in(p)?;
            let context_element = self.context_element.try_copy_in(p)?;
            // Preserve each existing input's source-validation/copy order.
            // Charge the new inline fields only once those inputs are ready,
            // before reading/copying refs or context_proofs. A rejected fee is
            // recorded before these paid temporaries leave scope.
            crate::tree::owner::check(p, 67)?;
            Ok(Self {
                refs: self.refs,
                context_proofs: self.context_proofs,
                parent_clip_path: self.parent_clip_path,
                parent_markers,
                context_element,
                fe_image_link: self.fe_image_link,
                view_box: self.view_box,
                use_size: self.use_size,
                opt: self.opt,
            })
        })();
        crate::tree::owner::finish(p, result)
    }
}
impl PolicyValue for SvgNode<'_, '_> {
    fn validate_policy(
        &self,
        p: &crate::ProcessingContext,
    ) -> std::result::Result<(), markdown_svg_budget::Error> {
        crate::tree::owner::validate_owner(p, self.resources())
    }
}
impl TryCopy for SvgNode<'_, '_> {
    fn try_copy_in(
        &self,
        p: &crate::ProcessingContext,
    ) -> std::result::Result<Self, markdown_svg_budget::Error> {
        crate::tree::owner::check(p, 0)?;
        Ok(*self)
    }
}

pub struct Cache {
    pub(super) references: RefFrames,
    pub(super) last_paint_proof: PaintProof,
    #[cfg(feature = "text")]
    pub(crate) font_replay: Option<Arc<dyn crate::FontReplay>>,
    pub(crate) processing: Arc<crate::ProcessingContext>,
    /// This fontdb is initialized from [`Options::fontdb`] and then populated
    /// over the course of conversion.
    #[cfg(feature = "text")]
    pub fontdb: Arc<Database>,

    #[cfg(feature = "text")]
    cache_outline: crate::outline_cache::OutlineCache,
    #[cfg(feature = "text")]
    cache_colr: ColrCache,
    #[cfg(feature = "text")]
    cache_svg: SvgCache,
    #[cfg(feature = "text")]
    cache_raster: RasterCache,
    #[cfg(feature = "text")]
    cache_has_opsz: OpszCache,

    pub(super) clip_paths: ClipCache,
    pub(super) masks: MaskCache,
    pub(super) filters: FilterCache,
    pub(super) paint: PaintCache,

    // used for ID generation
    all_ids: IdSet,
    linear_gradient_index: usize,
    radial_gradient_index: usize,
    pattern_index: usize,
    clip_path_index: usize,
    mask_index: usize,
    filter_index: usize,
    image_index: usize,
}

impl Cache {
    #[cfg(test)]
    pub(crate) fn new(#[cfg(feature = "text")] fontdb: Arc<Database>) -> Self {
        Self::with_processing(
            Arc::new(crate::ProcessingContext::default()),
            #[cfg(feature = "text")]
            fontdb,
        )
    }

    pub(crate) fn with_processing(
        processing: Arc<crate::ProcessingContext>,
        #[cfg(feature = "text")] fontdb: Arc<Database>,
    ) -> Self {
        #[cfg(feature = "text")]
        let cache_outline = crate::outline_cache::OutlineCache::new(processing.resources());
        if let Some(r) = processing.resources() {
            let _ = r.checkpoint(131);
        }
        Self {
            references: RefFrames::default(),
            last_paint_proof: PaintProof::default(),
            #[cfg(feature = "text")]
            font_replay: None,
            processing: processing.clone(),
            #[cfg(feature = "text")]
            fontdb,

            #[cfg(feature = "text")]
            cache_outline,
            #[cfg(feature = "text")]
            cache_colr: ColrCache::new(&processing),
            #[cfg(feature = "text")]
            cache_svg: SvgCache::new(&processing),
            #[cfg(feature = "text")]
            cache_raster: RasterCache::new(&processing),
            #[cfg(feature = "text")]
            cache_has_opsz: OpszCache::new(&processing),

            clip_paths: ClipCache::new(&processing),
            masks: MaskCache::new(&processing),
            filters: FilterCache::new(&processing),
            paint: PaintCache::new(&processing),

            all_ids: IdSet::new(&processing),
            linear_gradient_index: 0,
            radial_gradient_index: 0,
            pattern_index: 0,
            clip_path_index: 0,
            mask_index: 0,
            filter_index: 0,
            image_index: 0,
        }
    }

    pub(super) fn with_reference<T>(
        &mut self,
        node: SvgNode,
        state: &State,
        f: impl FnOnce(&State, &mut Cache) -> Option<T>,
    ) -> Option<T> {
        if state.opt.processing.resources().is_none() {
            return f(state, self);
        }
        let p = &state.opt.processing;
        let refs = state
            .refs
            .enter(node.reference_id(), p.resources())
            .map_err(|e| p.record_resource(e))
            .ok()?;
        let mut child = state.try_clone().ok()?;
        child.refs = refs;
        self.references
            .push(refs.depth(), p.resources())
            .map_err(|e| p.record_resource(e))
            .ok()?;
        let result = f(&child, self);
        if let Some(error) = p.resource_failure() {
            p.record_resource(error);
        }
        self.references.pop();
        drop(child);
        crate::tree::owner::check(p, 0).ok()?;
        result
    }
    pub(super) fn note_reference(&mut self, path: &RefPath, state: &State) -> bool {
        self.references
            .note(path.depth(), state.opt.processing.resources())
            .map_err(|e| state.opt.processing.record_resource(e))
            .is_ok()
    }
    pub(super) fn admit_context(&mut self, proof: PaintProof, state: &State) -> bool {
        if proof.span == 0 {
            return !state.opt.processing.failed();
        }
        let p = &state.opt.processing;
        let peak = match state.refs.check_span(proof.root, proof.span, p.resources()) {
            Ok(v) => v,
            Err(e) => {
                p.record_resource(e);
                return false;
            }
        };
        self.references
            .note(peak, p.resources())
            .map_err(|e| p.record_resource(e))
            .is_ok()
    }

    // TODO: macros?
    pub(crate) fn gen_linear_gradient_id(
        &mut self,
    ) -> std::result::Result<NonEmptyString, markdown_svg_budget::Error> {
        loop {
            crate::tree::owner::check(&self.processing, 1)?;
            self.linear_gradient_index += 1;
            let new_id = TreeString::try_from_fmt(
                format_args!("linearGradient{}", self.linear_gradient_index),
                &self.processing,
            )?;
            let hash = string_hash_with(&new_id, &self.processing)?;
            if !self.all_ids.contains(hash, &self.processing)? {
                return NonEmptyString::from_owned(new_id, &self.processing)?
                    .ok_or_else(|| crate::tree::owner::reject(&self.processing, None));
            }
            drop(new_id);
            crate::tree::owner::check(&self.processing, 0)?;
        }
    }

    pub(crate) fn gen_radial_gradient_id(
        &mut self,
    ) -> std::result::Result<NonEmptyString, markdown_svg_budget::Error> {
        loop {
            crate::tree::owner::check(&self.processing, 1)?;
            self.radial_gradient_index += 1;
            let new_id = TreeString::try_from_fmt(
                format_args!("radialGradient{}", self.radial_gradient_index),
                &self.processing,
            )?;
            let hash = string_hash_with(&new_id, &self.processing)?;
            if !self.all_ids.contains(hash, &self.processing)? {
                return NonEmptyString::from_owned(new_id, &self.processing)?
                    .ok_or_else(|| crate::tree::owner::reject(&self.processing, None));
            }
            drop(new_id);
            crate::tree::owner::check(&self.processing, 0)?;
        }
    }

    pub(crate) fn gen_pattern_id(
        &mut self,
    ) -> std::result::Result<NonEmptyString, markdown_svg_budget::Error> {
        loop {
            crate::tree::owner::check(&self.processing, 1)?;
            self.pattern_index += 1;
            let new_id = TreeString::try_from_fmt(
                format_args!("pattern{}", self.pattern_index),
                &self.processing,
            )?;
            let hash = string_hash_with(&new_id, &self.processing)?;
            if !self.all_ids.contains(hash, &self.processing)? {
                return NonEmptyString::from_owned(new_id, &self.processing)?
                    .ok_or_else(|| crate::tree::owner::reject(&self.processing, None));
            }
            drop(new_id);
            crate::tree::owner::check(&self.processing, 0)?;
        }
    }

    pub(crate) fn gen_clip_path_id(
        &mut self,
    ) -> std::result::Result<NonEmptyString, markdown_svg_budget::Error> {
        loop {
            crate::tree::owner::check(&self.processing, 1)?;
            self.clip_path_index += 1;
            let new_id = TreeString::try_from_fmt(
                format_args!("clipPath{}", self.clip_path_index),
                &self.processing,
            )?;
            let hash = string_hash_with(&new_id, &self.processing)?;
            if !self.all_ids.contains(hash, &self.processing)? {
                return NonEmptyString::from_owned(new_id, &self.processing)?
                    .ok_or_else(|| crate::tree::owner::reject(&self.processing, None));
            }
            drop(new_id);
            crate::tree::owner::check(&self.processing, 0)?;
        }
    }

    pub(crate) fn gen_mask_id(
        &mut self,
    ) -> std::result::Result<NonEmptyString, markdown_svg_budget::Error> {
        loop {
            crate::tree::owner::check(&self.processing, 1)?;
            self.mask_index += 1;
            let new_id = TreeString::try_from_fmt(
                format_args!("mask{}", self.mask_index),
                &self.processing,
            )?;
            let hash = string_hash_with(&new_id, &self.processing)?;
            if !self.all_ids.contains(hash, &self.processing)? {
                return NonEmptyString::from_owned(new_id, &self.processing)?
                    .ok_or_else(|| crate::tree::owner::reject(&self.processing, None));
            }
            drop(new_id);
            crate::tree::owner::check(&self.processing, 0)?;
        }
    }

    pub(crate) fn gen_filter_id(
        &mut self,
    ) -> std::result::Result<NonEmptyString, markdown_svg_budget::Error> {
        loop {
            crate::tree::owner::check(&self.processing, 1)?;
            self.filter_index += 1;
            let new_id = TreeString::try_from_fmt(
                format_args!("filter{}", self.filter_index),
                &self.processing,
            )?;
            let hash = string_hash_with(&new_id, &self.processing)?;
            if !self.all_ids.contains(hash, &self.processing)? {
                return NonEmptyString::from_owned(new_id, &self.processing)?
                    .ok_or_else(|| crate::tree::owner::reject(&self.processing, None));
            }
            drop(new_id);
            crate::tree::owner::check(&self.processing, 0)?;
        }
    }

    pub(crate) fn gen_image_id(
        &mut self,
    ) -> std::result::Result<NonEmptyString, markdown_svg_budget::Error> {
        loop {
            crate::tree::owner::check(&self.processing, 1)?;
            self.image_index += 1;
            let new_id = TreeString::try_from_fmt(
                format_args!("image{}", self.image_index),
                &self.processing,
            )?;
            let hash = string_hash_with(&new_id, &self.processing)?;
            if !self.all_ids.contains(hash, &self.processing)? {
                return NonEmptyString::from_owned(new_id, &self.processing)?
                    .ok_or_else(|| crate::tree::owner::reject(&self.processing, None));
            }
            drop(new_id);
            crate::tree::owner::check(&self.processing, 0)?;
        }
    }

    #[cfg(feature = "text")]
    pub(crate) fn font_options(&self) -> Option<Options<'static>> {
        Options::font_source_options(
            self.processing.clone(),
            self.fontdb.clone(),
            self.font_replay.clone(),
        )
        .ok()
    }
    #[cfg(feature = "text")]
    pub(crate) fn fontdb_svg(&mut self, font: ID, glyph: GlyphId) -> Option<Node> {
        if self.processing.failed() || !self.font_replay_color_allowed(font, glyph) {
            return None;
        }
        let key = (font, glyph);
        if let Some(value) = self.cache_svg.get(&key, &self.processing).ok()? {
            return crate::scene::clone_owned(&self.processing, value)
                .ok()
                .flatten();
        }
        let options = self.font_options()?;
        let value = self
            .fontdb
            .svg(font, glyph, svgtypes::Color::black(), &options);
        drop(options);
        crate::tree::owner::check(&self.processing, 0).ok()?;
        if self.processing.failed() {
            return None;
        }
        let value = value
            .map_err(|error| self.processing.record_resource(error))
            .ok()?;
        let cached = crate::scene::clone_owned(&self.processing, &value).ok()?;
        self.cache_svg.insert(key, cached, &self.processing).ok()?;
        value
    }
    #[cfg(feature = "text")]
    pub(crate) fn fontdb_svg_with_color(
        &mut self,
        font: ID,
        glyph: GlyphId,
        color: svgtypes::Color,
    ) -> Option<Node> {
        if color == svgtypes::Color::black() {
            return self.fontdb_svg(font, glyph);
        }
        if self.processing.failed() || !self.font_replay_color_allowed(font, glyph) {
            return None;
        }
        // No new memo/cache: this occurrence has a distinct source colour.
        let options = self.font_options()?;
        let candidate = self.fontdb.svg(font, glyph, color, &options);
        drop(options);
        crate::tree::owner::check(&self.processing, 0).ok()?;
        candidate
            .map_err(|error| self.processing.record_resource(error))
            .ok()?
    }
    #[cfg(feature = "text")]
    pub(crate) fn fontdb_raster(&mut self, font: ID, glyph: GlyphId) -> Option<BitmapImage> {
        if self.processing.failed() || !self.font_replay_color_allowed(font, glyph) {
            return None;
        }
        let key = (font, glyph);
        if let Some(value) = self.cache_raster.get(&key, &self.processing).ok()? {
            return value.try_copy_in(&self.processing).ok()?;
        }
        let value = self.fontdb.raster(font, glyph, &self.processing);
        crate::tree::owner::check(&self.processing, 0).ok()?;
        let value = value
            .map_err(|error| self.processing.record_resource(error))
            .ok()?;
        self.cache_raster
            .insert(
                key,
                value.try_copy_in(&self.processing).ok()?,
                &self.processing,
            )
            .ok()?;
        value
    }

    /// Every color entry (including cache hits) shares this gate before consuming
    /// the regular font payload. A real synthetic color glyph is not qualified
    /// until replay preserves its representation; never silently use its outline.
    #[cfg(feature = "text")]
    fn font_replay_color_allowed(&mut self, font: ID, glyph: GlyphId) -> bool {
        let Some(owner) = self.font_replay.as_ref() else {
            return true;
        };
        let allowed = match owner.outline(font, glyph.0) {
            Ok(crate::ReplayOutline::Ordinary) => true,
            Ok(crate::ReplayOutline::Synthetic { .. }) => {
                match self.fontdb.has_color_glyph(font, glyph, &self.processing) {
                    Ok(value) => value == Some(false),
                    Err(error) => {
                        self.processing.record_resource(error);
                        return false;
                    }
                }
            }
            Err(()) => false,
        };
        if !allowed {
            self.processing.fail(crate::ProcessingFailure::FontReplay);
        }
        allowed
    }

    #[cfg(feature = "text")]
    pub(crate) fn fontdb_outline(
        &mut self,
        font: ID,
        glyph: GlyphId,
        variations: &[FontVariation],
    ) -> Option<tiny_skia_path::Path> {
        let (query, cached) = match self.cache_outline.lookup(font, glyph, variations) {
            Ok(value) => value,
            Err(error) => {
                self.processing.record_resource(error);
                return None;
            }
        };
        match cached {
            Some(cache_hit) => cache_hit,
            None => {
                let lookup = self.fontdb.outline(
                    font,
                    glyph,
                    variations,
                    self.font_replay.as_deref(),
                    &self.processing,
                );
                if self.processing.failed() {
                    return None;
                }
                if let Err(error) = self.cache_outline.insert(query, lookup.clone()) {
                    self.processing.record_resource(error);
                    return None;
                }
                lookup
            }
        }
    }

    #[cfg(feature = "text")]
    pub(crate) fn fontdb_colr(
        &mut self,
        font: ID,
        glyph: GlyphId,
        variations: &[FontVariation],
        foreground_color: svgtypes::Color,
    ) -> Option<Tree> {
        if self.processing.failed() {
            return None;
        }
        if !self.font_replay_color_allowed(font, glyph) {
            return None;
        }
        // FFFF palette entries depend on the full computed color, including
        // alpha. SVG fill/outer opacity are separate paint and group state.
        let color = [
            foreground_color.red,
            foreground_color.green,
            foreground_color.blue,
            foreground_color.alpha,
        ];
        let (key, hit) = self
            .cache_colr
            .get(font, glyph, variations, color, &self.processing)
            .ok()?;
        if let Some(hit) = hit {
            return crate::scene::clone_owned(&self.processing, hit).ok()?;
        }
        let options = self.font_options()?;
        let lookup = self
            .fontdb
            .colr(font, glyph, variations, foreground_color, &options)
            .map_err(|e| self.processing.record_resource(e));
        drop(options);
        crate::tree::owner::check(&self.processing, 0).ok()?;
        let lookup = lookup.ok()?;
        let cached = crate::scene::clone_owned(&self.processing, &lookup).ok()?;
        self.cache_colr.insert(key, cached, &self.processing).ok()?;
        lookup
    }

    #[cfg(feature = "text")]
    pub(crate) fn has_opsz_axis(&mut self, font: ID) -> bool {
        if crate::tree::owner::check(&self.processing, 0).is_err() {
            return false;
        }
        match self.cache_has_opsz.get(&font, &self.processing) {
            Ok(Some(&value)) => return value,
            Err(_) => return false,
            Ok(None) => {}
        }
        let value = self.fontdb.has_opsz_axis(font, &self.processing);
        if crate::tree::owner::check(&self.processing, 0).is_err() {
            return false;
        }
        if self
            .cache_has_opsz
            .insert(font, value, &self.processing)
            .is_err()
        {
            return false;
        }
        value
    }
}

// TODO: is there a simpler way?
fn string_hash_with(
    s: &str,
    p: &crate::ProcessingContext,
) -> std::result::Result<u64, markdown_svg_budget::Error> {
    crate::tree::owner::check(p, 0)?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    // Fixed-toolchain str::Hash byte stream, including its prefix-free ff.
    for bytes in s.as_bytes().chunks(256) {
        crate::tree::owner::check(p, bytes.len() as u64)?;
        hash.write(bytes);
    }
    crate::tree::owner::check(p, 1)?;
    hash.write_u8(0xff);
    Ok(hash.finish())
}

impl<'a, 'input: 'a> SvgNode<'a, 'input> {
    pub(crate) fn convert_length(
        &self,
        aid: AId,
        object_units: Units,
        state: &State,
        def: Length,
    ) -> f32 {
        units::convert_length(
            self.attribute(aid).unwrap_or(def),
            *self,
            aid,
            object_units,
            state,
        )
    }

    pub fn convert_user_length(&self, aid: AId, state: &State, def: Length) -> f32 {
        self.convert_length(aid, Units::UserSpaceOnUse, state, def)
    }

    pub fn parse_viewbox(&self) -> Option<NonZeroRect> {
        let vb: svgtypes::ViewBox = self.attribute(AId::ViewBox)?;
        NonZeroRect::from_xywh(vb.x as f32, vb.y as f32, vb.w as f32, vb.h as f32)
    }

    pub fn resolve_length(&self, aid: AId, state: &State, def: f32) -> f32 {
        debug_assert!(
            !matches!(aid, AId::BaselineShift | AId::FontSize),
            "{} cannot be resolved via this function",
            aid
        );

        if let Some(n) = self.ancestors().find(|n| n.has_attribute(aid)) {
            if let Some(length) = n.attribute(aid) {
                return units::convert_user_length(length, n, aid, state);
            }
        }

        def
    }

    pub fn resolve_valid_length(
        &self,
        aid: AId,
        state: &State,
        def: f32,
    ) -> Option<NonZeroPositiveF32> {
        let n = self.resolve_length(aid, state, def);
        NonZeroPositiveF32::new(n)
    }

    pub(crate) fn try_convert_length(
        &self,
        aid: AId,
        object_units: Units,
        state: &State,
    ) -> Option<f32> {
        Some(units::convert_length(
            self.attribute(aid)?,
            *self,
            aid,
            object_units,
            state,
        ))
    }

    pub fn has_valid_transform(&self, aid: AId) -> bool {
        // Do not use Node::attribute::<Transform>, because it will always
        // return a valid transform.

        let attr = match self.attribute(aid) {
            Some(attr) => attr,
            None => return true,
        };

        let ts = match svgtypes::Transform::from_str(attr) {
            Ok(v) => v,
            Err(_) => return true,
        };

        let ts = Transform::from_row(
            ts.a as f32,
            ts.b as f32,
            ts.c as f32,
            ts.d as f32,
            ts.e as f32,
            ts.f as f32,
        );
        ts.is_valid()
    }

    pub fn is_visible_element(&self, opt: &crate::Options) -> bool {
        self.attribute(AId::Display) != Some("none")
            && self.has_valid_transform(AId::Transform)
            && super::switch::is_condition_passed(*self, opt)
    }
}

pub trait SvgColorExt {
    fn split_alpha(self) -> (Color, Opacity);
}

impl SvgColorExt for svgtypes::Color {
    fn split_alpha(self) -> (Color, Opacity) {
        (
            Color::new_rgb(self.red, self.green, self.blue),
            Opacity::new_u8(self.alpha),
        )
    }
}

/// Converts an input `Document` into a `Tree`.
///
/// # Errors
///
/// - If `Document` doesn't have an SVG node - returns an empty tree.
/// - If `Document` doesn't have a valid size - returns `Error::InvalidSize`.
pub(crate) fn convert_doc(svg_doc: &svgtree::Document, opt: &Options) -> Result<Tree, Error> {
    opt.processing.checked().map_err(Error::ProcessingFailure)?;
    let svg = svg_doc.root_element();
    let controlled_image = opt.processing.resources().is_some()
        && matches!(
            opt.source_kind,
            super::source_kind::SourceKind::GeneralImage
        );
    let (size, natural_size, restore_viewbox) = if controlled_image {
        let natural_size = resolve_natural_size(&svg)?;
        if natural_size.0 > 8192.0
            || natural_size.1 > 8192.0
            || natural_size.0 * natural_size.1 > 16.0 * 1024.0 * 1024.0
        {
            let error = opt
                .processing
                .record_resource(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Quota,
                    markdown_svg_budget::LimitKind::Capacity,
                ));
            return Err(Error::Resource(error));
        }
        let size = Size::from_wh(natural_size.0 as f32, natural_size.1 as f32)
            .ok_or(Error::InvalidSize)?;
        // The explicit viewport drives the actual root mapping. Never replace
        // natural metadata with drawing bounds, even when viewBox is absent.
        (size, natural_size, false)
    } else {
        let (size, restore_viewbox) = resolve_svg_size(&svg, opt);
        let size = size?;
        (
            size,
            (f64::from(size.width()), f64::from(size.height())),
            restore_viewbox,
        )
    };
    let view_box = ViewBox {
        rect: svg
            .parse_viewbox()
            .unwrap_or_else(|| size.to_non_zero_rect(0.0, 0.0)),
        aspect: svg.attribute(AId::PreserveAspectRatio).unwrap_or_default(),
    };

    let background_color = svg
        .attribute::<&str>(AId::BackgroundColor)
        .and_then(|s| svgtypes::Paint::from_str(s).ok())
        .and_then(|paint| match paint {
            svgtypes::Paint::Color(c) => Some(c),
            _ => None,
        });

    let mut tree = Tree {
        source_kind: opt.source_kind,
        size,
        natural_size,
        processing: opt.processing.clone(),
        root: crate::scene::root(&opt.processing, || Group::empty_in(&opt.processing))
            .map_err(Error::Resource)?,
        linear_gradients: TreeVec::new(&opt.processing),
        radial_gradients: TreeVec::new(&opt.processing),
        patterns: TreeVec::new(&opt.processing),
        clip_paths: TreeVec::new(&opt.processing),
        masks: TreeVec::new(&opt.processing),
        filters: TreeVec::new(&opt.processing),
        #[cfg(feature = "text")]
        fontdb: opt.fontdb.clone(),
        #[cfg(feature = "text")]
        font_replay: opt.font_replay.clone(),
    };

    if !svg.is_visible_element(opt) {
        return Ok(tree);
    }

    let state = State {
        refs: RefPath::paid_root(svg_doc.reference_root(), opt.processing.resources())
            .map_err(Error::Resource)?,
        context_proofs: [PaintProof::default(); 2],
        parent_clip_path: None,
        context_element: None,
        parent_markers: TreeVec::new(&opt.processing),
        fe_image_link: false,
        view_box: view_box.rect,
        use_size: (None, None),
        opt,
    };

    let mut cache = Cache::with_processing(
        opt.processing.clone(),
        #[cfg(feature = "text")]
        opt.fontdb.clone(),
    );

    opt.processing.checked().map_err(Error::ProcessingFailure)?;

    #[cfg(feature = "text")]
    {
        cache.font_replay = opt.font_replay.clone();
    }

    for node in svg_doc.descendants() {
        if let Some(tag) = node.tag_name() {
            if matches!(
                tag,
                EId::ClipPath
                    | EId::Filter
                    | EId::LinearGradient
                    | EId::Mask
                    | EId::Pattern
                    | EId::RadialGradient
                    | EId::Image
            ) {
                if !node.element_id().is_empty() {
                    cache
                        .all_ids
                        .insert(
                            string_hash_with(node.element_id(), &opt.processing)
                                .map_err(Error::Resource)?,
                            &opt.processing,
                        )
                        .map_err(Error::Resource)?;
                }
            }
        }
    }

    // The root transform wraps the viewBox mapping, so its origin is resolved
    // against the viewport rather than the viewBox.
    let root_state = State {
        view_box: size.to_non_zero_rect(0.0, 0.0),
        ..state.try_clone().map_err(Error::Resource)?
    };
    let svg_ts = svg.resolve_transform(AId::Transform, &root_state);
    let root_ts = svg_ts.pre_concat(view_box.to_transform(tree.size()));
    if root_ts.is_identity() && background_color.is_none() {
        convert_children(svg_doc.root(), &state, &mut cache, &mut tree.root);
    } else {
        let mut g = Group::empty_in(&opt.processing);

        if let Some(background_color) = background_color {
            if let Some(path) =
                background_path(background_color, view_box.rect.to_rect(), &opt.processing)
                    .map_err(|error| Error::Resource(opt.processing.record_resource(error)))?
            {
                let cost = crate::scene::SceneCost::path(&path);
                let path = crate::scene::boxed_owned(&opt.processing, cost, path)
                    .map_err(Error::Resource)?;
                g.children
                    .try_push(Node::Path(path), &opt.processing)
                    .map_err(Error::Resource)?;
            }
        }

        g.transform = root_ts;
        g.abs_transform = root_ts;
        convert_children(svg_doc.root(), &state, &mut cache, &mut g);
        opt.processing.checked().map_err(Error::ProcessingFailure)?;
        g.calculate_bounding_boxes();
        let group = crate::scene::boxed_owned(&opt.processing, crate::scene::SceneCost::NODE, g)
            .map_err(Error::Resource)?;
        tree.root
            .children
            .try_push(Node::Group(group), &opt.processing)
            .map_err(Error::Resource)?;
    }

    opt.processing.checked().map_err(Error::ProcessingFailure)?;

    // Clear cache to make sure that all `Arc<T>` objects have a single strong reference.
    cache
        .clip_paths
        .clear(&opt.processing)
        .map_err(Error::Resource)?;
    cache
        .masks
        .clear(&opt.processing)
        .map_err(Error::Resource)?;
    cache
        .filters
        .clear(&opt.processing)
        .map_err(Error::Resource)?;
    cache
        .paint
        .clear(&opt.processing)
        .map_err(Error::Resource)?;

    opt.processing.checked().map_err(Error::ProcessingFailure)?;

    super::paint_server::update_paint_servers(
        &mut tree.root,
        Transform::default(),
        None,
        None,
        &mut cache,
    );
    opt.processing.checked().map_err(Error::ProcessingFailure)?;
    tree.collect_paint_servers().map_err(Error::Resource)?;
    tree.root
        .collect_clip_paths(&mut tree.clip_paths, &opt.processing)
        .map_err(Error::Resource)?;
    tree.root
        .collect_masks(&mut tree.masks, &opt.processing)
        .map_err(Error::Resource)?;
    tree.root
        .collect_filters(&mut tree.filters, &opt.processing)
        .map_err(Error::Resource)?;
    tree.root.calculate_bounding_boxes();

    // The fontdb might have been mutated and we want to apply these changes to
    // the tree's fontdb.
    #[cfg(feature = "text")]
    {
        tree.fontdb = if let Some(owner) = cache.font_replay.as_ref() {
            owner.fontdb().map_err(|_| {
                cache.processing.fail(crate::ProcessingFailure::FontReplay);
                Error::ProcessingFailure(cache.processing.error().expect("recorded first failure"))
            })?
        } else {
            cache.fontdb
        };
    }

    if restore_viewbox {
        calculate_svg_bbox(&mut tree);
        tree.natural_size = (f64::from(tree.size.width()), f64::from(tree.size.height()));
    }

    opt.processing.checked().map_err(Error::ProcessingFailure)?;
    tree.validate_policy(&opt.processing)
        .map_err(Error::Resource)?;
    Ok(tree)
}

fn background_path(
    background_color: svgtypes::Color,
    area: Rect,
    processing: &ProcessingContext,
) -> Result<Option<Path>, markdown_svg_budget::Error> {
    let path = super::shapes::rect_to_path(area, processing)?;
    let fill = Fill {
        paint: Paint::Color(Color::new_rgb(
            background_color.red,
            background_color.green,
            background_color.blue,
        )),
        opacity: NormalizedF32::new(background_color.alpha as f32 / 255.0)
            .expect("byte alpha is normalized"),
        ..Default::default()
    };
    let data = crate::geometry::SharedOwner::try_new(
        path,
        processing.resources(),
        markdown_svg_budget::ResourceKind::Geometry,
    )?;
    let candidate = Path::new_shared(
        "",
        true,
        None,
        None,
        PaintOrder::default(),
        ShapeRendering::default(),
        data,
        Transform::default(),
        processing.resources(),
    )?;
    Ok(candidate.map(|mut path| {
        path.fill = Some(fill);
        path
    }))
}

#[cfg(all(test, feature = "text"))]
#[path = "../background_geometry_tests.rs"]
mod background_geometry_tests;

/// Resolve the controlled root on the already cascaded node, before f32 geometry.
/// Nested svg elements continue through use_node's parent-viewport conversion.
fn resolve_natural_size(svg: &SvgNode) -> Result<(f64, f64), Error> {
    fn length(svg: &SvgNode, aid: AId) -> Result<Option<Length>, Error> {
        if svg.has_attribute(aid) {
            svg.attribute(aid).map(Some).ok_or(Error::InvalidSize)
        } else {
            Ok(None)
        }
    }

    fn convert(length: Length, percent_base: f64, font_size: f64) -> f64 {
        let n = length.number;
        match length.unit {
            Unit::None | Unit::Px => n,
            Unit::Em => n * font_size,
            Unit::Ex => n * font_size / 2.0,
            Unit::In => n * 96.0,
            Unit::Cm => n * 96.0 / 2.54,
            Unit::Mm => n * 96.0 / 25.4,
            Unit::Pt => n * 96.0 / 72.0,
            Unit::Pc => n * 96.0 / 6.0,
            Unit::Percent => n / 100.0 * percent_base,
        }
    }

    let view_box = if svg.has_attribute(AId::ViewBox) {
        let raw: &str = svg.attribute(AId::ViewBox).ok_or(Error::InvalidSize)?;
        let mut numbers = svgtypes::NumberListParser::from(raw);
        let mut values = [0.0; 4];
        for value in &mut values {
            *value = numbers
                .next()
                .ok_or(Error::InvalidSize)?
                .map_err(|_| Error::InvalidSize)?;
        }
        if numbers.next().is_some() {
            return Err(Error::InvalidSize);
        }
        let [x, y, w, h] = values;
        let vb = svgtypes::ViewBox::new(x, y, w, h);
        if ![vb.x, vb.y, vb.w, vb.h].iter().all(|v| v.is_finite())
            || vb.w <= 0.0
            || vb.h <= 0.0
            || svg.parse_viewbox().is_none()
        {
            return Err(Error::InvalidSize);
        }
        Some(vb)
    } else {
        None
    };
    // Root em/ex dimensions use the same cascaded font-size as root content.
    // The parent value is the controlled user-agent default, 16px at 96 DPI.
    let font_size = if let Some(size) = svg.attribute::<Length>(AId::FontSize) {
        convert(size, 16.0, 16.0)
    } else {
        let factor = match svg.attribute::<&str>(AId::FontSize) {
            Some("xx-small") => -3,
            Some("x-small") => -2,
            Some("small" | "smaller") => -1,
            Some("large" | "larger") => 1,
            Some("x-large") => 2,
            Some("xx-large") => 3,
            _ => 0,
        };
        16.0 * 1.2_f64.powi(factor)
    };
    let width = length(svg, AId::Width)?;
    let height = length(svg, AId::Height)?;
    let base = view_box.map_or((300.0, 150.0), |vb| (vb.w, vb.h));
    let w = width.map_or(base.0, |v| convert(v, base.0, font_size));
    let h = height.map_or(base.1, |v| convert(v, base.1, font_size));
    let size = match (view_box, width, height) {
        (Some(vb), Some(_), None) => (w, w * (vb.h / vb.w)),
        (Some(vb), None, Some(_)) => (h * (vb.w / vb.h), h),
        _ => (w, h),
    };
    if !size.0.is_finite() || !size.1.is_finite() || size.0 <= 0.0 || size.1 <= 0.0 {
        return Err(Error::InvalidSize);
    }
    Ok(size)
}

fn resolve_svg_size(svg: &SvgNode, opt: &Options) -> (Result<Size, Error>, bool) {
    let mut state = State {
        refs: RefPath::default(),
        context_proofs: [PaintProof::default(); 2],
        parent_clip_path: None,
        context_element: None,
        parent_markers: TreeVec::new(&opt.processing),
        fe_image_link: false,
        view_box: NonZeroRect::from_xywh(0.0, 0.0, 100.0, 100.0).unwrap(),
        use_size: (None, None),
        opt,
    };

    if let super::source_kind::SourceKind::FontGlyph { units_per_em, .. } = opt.source_kind {
        let em = units_per_em as f32;
        state.view_box = NonZeroRect::from_xywh(0.0, 0.0, em, em).unwrap();
        let def = Length::new(em as f64, Unit::None);
        let width = svg.convert_user_length(AId::Width, &state, def);
        let height = svg.convert_user_length(AId::Height, &state, def);
        return (
            Size::from_wh(width, height).ok_or(Error::InvalidSize),
            false,
        );
    }
    let def = Length::new(100.0, Unit::Percent);
    let mut width: Length = svg.attribute(AId::Width).unwrap_or(def);
    let mut height: Length = svg.attribute(AId::Height).unwrap_or(def);

    let view_box = svg.parse_viewbox();

    let restore_viewbox =
        if (width.unit == Unit::Percent || height.unit == Unit::Percent) && view_box.is_none() {
            // Apply the percentages to the fallback size.
            if width.unit == Unit::Percent {
                width = Length::new(
                    (width.number / 100.0) * state.opt.default_size.width() as f64,
                    Unit::None,
                );
            }

            if height.unit == Unit::Percent {
                height = Length::new(
                    (height.number / 100.0) * state.opt.default_size.height() as f64,
                    Unit::None,
                );
            }

            true
        } else {
            false
        };

    let size = if let Some(vbox) = view_box {
        state.view_box = vbox;

        let w = if width.unit == Unit::Percent {
            vbox.width() * (width.number as f32 / 100.0)
        } else {
            svg.convert_user_length(AId::Width, &state, def)
        };

        let h = if height.unit == Unit::Percent {
            vbox.height() * (height.number as f32 / 100.0)
        } else {
            svg.convert_user_length(AId::Height, &state, def)
        };

        // If exactly one of width and height is specified, compute the missing
        // dimension from the specified one and the viewBox aspect ratio.
        match (
            svg.attribute::<Length>(AId::Width),
            svg.attribute::<Length>(AId::Height),
        ) {
            (Some(_), None) => Size::from_wh(w, vbox.height() * w / vbox.width()),
            (None, Some(_)) => Size::from_wh(vbox.width() * h / vbox.height(), h),
            (_, _) => Size::from_wh(w, h),
        }
    } else {
        Size::from_wh(
            svg.convert_user_length(AId::Width, &state, def),
            svg.convert_user_length(AId::Height, &state, def),
        )
    };

    (size.ok_or(Error::InvalidSize), restore_viewbox)
}

/// Calculates SVG's size and viewBox in case there were not set.
///
/// Simply iterates over all nodes and calculates a bounding box.
fn calculate_svg_bbox(tree: &mut Tree) {
    let bbox = tree.root.abs_bounding_box();
    if let Some(size) = Size::from_wh(bbox.right(), bbox.bottom()) {
        tree.size = size;
    }
}

#[inline(never)]
pub(crate) fn convert_children(
    parent_node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    for node in parent_node.children() {
        if state.opt.processing.failed() {
            return;
        }
        convert_element(node, state, cache, parent);
        if state.opt.processing.failed() {
            return;
        }
    }
}

#[inline(never)]
pub(crate) fn convert_element(node: SvgNode, state: &State, cache: &mut Cache, parent: &mut Group) {
    if !state.opt.processing.enter_conversion() {
        return;
    }
    let tag_name = match node.tag_name() {
        Some(v) => v,
        None => return,
    };

    if !tag_name.is_graphic() && !matches!(tag_name, EId::G | EId::Switch | EId::Svg) {
        return;
    }

    if !node.is_visible_element(state.opt) {
        return;
    }

    if tag_name == EId::Use {
        super::use_node::convert(node, state, cache, parent);
        return;
    }

    if tag_name == EId::Switch {
        super::switch::convert(node, state, cache, parent);
        return;
    }

    // A nested `svg` is handled just like `use`: it must not be wrapped in an
    // additional `convert_group`, otherwise its `transform` (and other group
    // properties) would be applied twice, since `convert_svg`
    // already creates its own group. The outermost `svg` (no parent element)
    // is intentionally not handled here - it's processed directly in
    // `convert_doc`.
    if tag_name == EId::Svg && node.parent_element().is_some() {
        super::use_node::convert_svg(node, state, cache, parent);

        return;
    }

    if let Some(g) = convert_group(node, state, false, cache, parent, &|cache, g| {
        convert_element_impl(tag_name, node, state, cache, g);
    }) {
        let Ok(group) =
            crate::scene::boxed_owned(&state.opt.processing, crate::scene::SceneCost::NODE, g)
        else {
            return;
        };
        if parent
            .children
            .try_push(Node::Group(group), &state.opt.processing)
            .is_err()
        {
            return;
        };
    }
}

#[inline(never)]
fn convert_element_impl(
    tag_name: EId,
    node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    match tag_name {
        EId::Rect
        | EId::Circle
        | EId::Ellipse
        | EId::Line
        | EId::Polyline
        | EId::Polygon
        | EId::Path => match super::shapes::convert(node, state) {
            Ok(Some(path)) => convert_path(node, path, state, cache, parent),
            Ok(None) => (),
            Err(error) => {
                state.opt.processing.record_resource(error);
                return;
            }
        },
        EId::Image => {
            super::image::convert(node, state, cache, parent);
        }
        EId::Text => {
            #[cfg(feature = "text")]
            {
                super::text::convert(node, state, cache, parent);
            }
        }
        EId::Svg => {
            // Only the outermost `svg` reaches this point; nested `svg` elements are
            // handled earlier in `convert_element`. The root `svg` itself is
            // skipped and only its children are converted.
            convert_children(node, state, cache, parent);
        }
        EId::G => {
            convert_children(node, state, cache, parent);
        }
        _ => {}
    }
}

// `clipPath` can have only shape and `text` children.
//
// `line` doesn't impact rendering because stroke is always disabled
// for `clipPath` children.
#[inline(never)]
pub(crate) fn convert_clip_path_elements(
    clip_node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    for node in clip_node.children() {
        if state.opt.processing.failed() {
            return;
        }
        let tag_name = match node.tag_name() {
            Some(v) => v,
            None => continue,
        };

        if !tag_name.is_graphic() {
            continue;
        }

        if !node.is_visible_element(state.opt) {
            continue;
        }

        if tag_name == EId::Use {
            super::use_node::convert(node, state, cache, parent);
            continue;
        }

        if let Some(g) = convert_group(node, state, false, cache, parent, &|cache, g| {
            convert_clip_path_elements_impl(tag_name, node, state, cache, g);
        }) {
            let Ok(group) =
                crate::scene::boxed_owned(&state.opt.processing, crate::scene::SceneCost::NODE, g)
            else {
                return;
            };
            if parent
                .children
                .try_push(Node::Group(group), &state.opt.processing)
                .is_err()
            {
                return;
            };
        }
    }
}

#[inline(never)]
fn convert_clip_path_elements_impl(
    tag_name: EId,
    node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    match tag_name {
        EId::Rect | EId::Circle | EId::Ellipse | EId::Polyline | EId::Polygon | EId::Path => {
            match super::shapes::convert(node, state) {
                Ok(Some(path)) => convert_path(node, path, state, cache, parent),
                Ok(None) => (),
                Err(error) => {
                    state.opt.processing.record_resource(error);
                    return;
                }
            }
        }
        EId::Text => {
            #[cfg(feature = "text")]
            {
                super::text::convert(node, state, cache, parent);
            }
        }
        _ => {
            log::warn!("'{}' is no a valid 'clip-path' child.", tag_name);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Isolation {
    Auto,
    Isolate,
}

impl Default for Isolation {
    fn default() -> Self {
        Self::Auto
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Isolation {
    fn parse(_: SvgNode, _: AId, value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Isolation::Auto),
            "isolate" => Some(Isolation::Isolate),
            _ => None,
        }
    }
}

// TODO: explain
pub(crate) fn convert_group(
    node: SvgNode,
    state: &State,
    force: bool,
    cache: &mut Cache,
    parent: &mut Group,
    collect_children: &dyn Fn(&mut Cache, &mut Group),
) -> Option<Group> {
    if state.opt.processing.failed() {
        return None;
    }
    // A `clipPath` child cannot have an opacity.
    let opacity = if state.parent_clip_path.is_none() {
        node.attribute::<Opacity>(AId::Opacity)
            .unwrap_or(Opacity::ONE)
    } else {
        Opacity::ONE
    };

    let is_root_svg = node.tag_name() == Some(EId::Svg) && node.parent_element().is_none();
    let transform = if is_root_svg {
        // Already applied outside the viewBox mapping in `convert_doc`.
        Transform::default()
    } else {
        node.resolve_transform(AId::Transform, state)
    };
    let blend_mode: BlendMode = node.attribute(AId::MixBlendMode).unwrap_or_default();
    let isolation: Isolation = node.attribute(AId::Isolation).unwrap_or_default();
    let isolate = isolation == Isolation::Isolate;

    // Nodes generated by markers must not have an ID. Otherwise we would have duplicates.
    let is_g_or_use = matches!(node.tag_name(), Some(EId::G) | Some(EId::Use));
    let id = TreeString::try_from_str(
        if is_g_or_use && state.parent_markers.is_empty() {
            node.element_id()
        } else {
            ""
        },
        &state.opt.processing,
    )
    .ok()?;

    let abs_transform = parent.abs_transform.pre_concat(transform);
    let dummy = Rect::from_xywh(0.0, 0.0, 0.0, 0.0).unwrap();
    let mut g = Group {
        origin: Origin::new(&state.opt.processing),
        id,
        transform,
        abs_transform,
        opacity,
        blend_mode,
        isolate,
        clip_path: None,
        mask: None,
        filters: TreeVec::new(&state.opt.processing),
        is_context_element: false,
        bounding_box: dummy,
        abs_bounding_box: dummy,
        stroke_bounding_box: dummy,
        abs_stroke_bounding_box: dummy,
        layer_bounding_box: NonZeroRect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap(),
        abs_layer_bounding_box: NonZeroRect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap(),
        children: TreeVec::new(&state.opt.processing),
    };
    collect_children(cache, &mut g);
    if state.opt.processing.failed() {
        return None;
    }

    // We need to know group's bounding box before converting
    // clipPaths, masks and filters.
    let object_bbox = g.calculate_object_bbox();

    // `mask` and `filter` cannot be set on `clipPath` children.
    // But `clip-path` can.

    let mut clip_path = None;
    if let Some(link) = node.attribute::<SvgNode>(AId::ClipPath) {
        clip_path = super::clippath::convert(link, state, object_bbox, cache);
        if state.opt.processing.failed() {
            return None;
        }
        if clip_path.is_none() {
            return None;
        }
    }

    let mut mask = None;
    if state.parent_clip_path.is_none() {
        if let Some(link) = node.attribute::<SvgNode>(AId::Mask) {
            mask = super::mask::convert(link, state, object_bbox, cache);
            if state.opt.processing.failed() {
                return None;
            }
            if mask.is_none() {
                return None;
            }
        }
    }

    let filters = {
        let mut filters = TreeVec::new(&state.opt.processing);
        if state.parent_clip_path.is_none() {
            if node.attribute(AId::Filter) == Some("none") {
                // Do nothing.
            } else if node.has_attribute(AId::Filter) {
                if let Ok(f) = super::filter::convert(node, state, object_bbox, cache) {
                    filters = f;
                } else {
                    // A filter that not a link or a filter with a link to a non existing element.
                    //
                    // Unlike `clip-path` and `mask`, when a `filter` link is invalid
                    // then the whole element should be ignored.
                    //
                    // This is kinda an undefined behaviour.
                    // In most cases, Chrome, Firefox and rsvg will ignore such elements,
                    // but in some cases Chrome allows it. Not sure why.
                    // Inkscape (0.92) simply ignores such attributes, rendering element as is.
                    // Batik (1.12) crashes.
                    //
                    // Test file: e-filter-051.svg
                    return None;
                }
            }
        }

        filters
    };

    if state.opt.processing.failed() {
        return None;
    }
    let required = opacity.get().approx_ne_ulps(&1.0, 4)
        || clip_path.is_some()
        || mask.is_some()
        || !filters.is_empty()
        || !transform.is_identity()
        || blend_mode != BlendMode::Normal
        || isolate
        || is_g_or_use
        || force;

    if !required {
        parent
            .children
            .try_append(&mut g.children, &state.opt.processing)
            .ok()?;
        return None;
    }

    g.clip_path = clip_path;
    g.mask = mask;
    g.filters = filters;

    // Must be called after we set Group::filters
    g.calculate_bounding_boxes();

    Some(g)
}

fn convert_path(
    node: SvgNode,
    tiny_skia_path: crate::geometry::SharedOwner<tiny_skia_path::Path>,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    debug_assert!(tiny_skia_path.len() >= 2);
    if tiny_skia_path.len() < 2 {
        return;
    }

    let has_bbox = tiny_skia_path.bounds().width() > 0.0 && tiny_skia_path.bounds().height() > 0.0;
    let mut fill = super::style::resolve_fill(node, has_bbox, state, cache);
    if state.opt.processing.failed() {
        return;
    }
    let fill_proof = cache.last_paint_proof;
    let mut stroke = super::style::resolve_stroke(node, has_bbox, state, cache);
    let stroke_proof = cache.last_paint_proof;
    if state.opt.processing.failed() {
        return;
    }
    let visibility: Visibility = node.find_attribute(AId::Visibility).unwrap_or_default();
    let mut visible = visibility == Visibility::Visible;
    let rendering_mode: ShapeRendering = node
        .find_attribute(AId::ShapeRendering)
        .unwrap_or(state.opt.shape_rendering);

    // TODO: handle `markers` before `stroke`
    let raw_paint_order: svgtypes::PaintOrder =
        node.find_attribute(AId::PaintOrder).unwrap_or_default();
    let paint_order = svg_paint_order_to_usvg(raw_paint_order);
    let path_transform = parent.abs_transform;

    // If a path doesn't have a fill or a stroke then it's invisible.
    // By setting `visibility` to `hidden` we are disabling rendering of this path.
    if fill.is_none() && stroke.is_none() {
        visible = false;
    }

    if let Some(fill) = fill.as_mut() {
        if let Some(ContextElement::PathNode(context_transform, context_bbox)) =
            fill.context_element
        {
            process_paint(
                &mut fill.paint,
                true,
                context_transform,
                context_bbox.map(|r| r.to_rect()),
                path_transform,
                tiny_skia_path.bounds(),
                cache,
            );
            fill.context_element = None;
        }
    }

    if state.opt.processing.failed() {
        return;
    }

    if let Some(stroke) = stroke.as_mut() {
        if let Some(ContextElement::PathNode(context_transform, context_bbox)) =
            stroke.context_element
        {
            process_paint(
                &mut stroke.paint,
                true,
                context_transform,
                context_bbox.map(|r| r.to_rect()),
                path_transform,
                tiny_skia_path.bounds(),
                cache,
            );
            stroke.context_element = None;
        }
    }

    if state.opt.processing.failed() {
        return;
    }

    let mut marker = None;
    if marker::is_valid(node) && visibility == Visibility::Visible {
        let mut marker_group = Group {
            abs_transform: parent.abs_transform,
            ..Group::empty_in(&state.opt.processing)
        };

        let Ok(mut marker_state) = state.try_clone() else {
            return;
        };

        let bbox = match state.opt.processing.resources() {
            Some(resources) => match tiny_skia_path.try_compute_tight_bounds(resources) {
                Ok(value) => value.and_then(|r| r.to_non_zero_rect()),
                Err(error) => {
                    state.opt.processing.record_resource(error);
                    return;
                }
            },
            None => tiny_skia_path
                .compute_tight_bounds()
                .and_then(|r| r.to_non_zero_rect()),
        };

        let Ok(fill) = fill.try_copy_in(&state.opt.processing) else {
            return;
        };
        let fill = fill.map(|mut f| {
            f.context_element = Some(ContextElement::PathNode(path_transform, bbox));
            f
        });

        let Ok(stroke) = stroke.try_copy_in(&state.opt.processing) else {
            return;
        };
        let stroke = stroke.map(|mut s| {
            s.context_element = Some(ContextElement::PathNode(path_transform, bbox));
            s
        });

        marker_state.context_element = Some((fill, stroke));
        marker_state.context_proofs = [fill_proof, stroke_proof];

        marker::convert(
            node,
            &tiny_skia_path,
            &marker_state,
            cache,
            &mut marker_group,
        );
        if state.opt.processing.failed() {
            return;
        }
        state.opt.processing.observe_marker_bbox();
        marker_group.calculate_bounding_boxes();
        marker = Some(marker_group);
    }

    // Nodes generated by markers must not have an ID. Otherwise we would have duplicates.
    let id = if state.parent_markers.is_empty() {
        node.element_id()
    } else {
        ""
    };

    let path = Path::new_shared(
        id,
        visible,
        fill,
        stroke,
        paint_order,
        rendering_mode,
        tiny_skia_path,
        path_transform,
        state.opt.processing.resources(),
    );

    let path = match path {
        Ok(Some(v)) => v,
        Ok(None) => return,
        Err(error) => {
            state.opt.processing.record_resource(error);
            return;
        }
    };

    match (raw_paint_order.order, marker) {
        ([PaintOrderKind::Markers, _, _], Some(markers_node)) => {
            let Ok(group) = crate::scene::boxed_owned(
                &state.opt.processing,
                crate::scene::SceneCost::NODE,
                markers_node,
            ) else {
                return;
            };
            if parent
                .children
                .try_push(Node::Group(group), &state.opt.processing)
                .is_err()
            {
                return;
            };
            let Ok(path) = crate::scene::clone_boxed(&state.opt.processing, &path) else {
                return;
            };
            if parent
                .children
                .try_push(Node::Path(path), &state.opt.processing)
                .is_err()
            {
                return;
            };
        }
        ([first, PaintOrderKind::Markers, last], Some(markers_node)) => {
            append_single_paint_path(first, &path, parent, &state.opt.processing);
            if state.opt.processing.failed() {
                return;
            }
            let Ok(group) = crate::scene::boxed_owned(
                &state.opt.processing,
                crate::scene::SceneCost::NODE,
                markers_node,
            ) else {
                return;
            };
            if parent
                .children
                .try_push(Node::Group(group), &state.opt.processing)
                .is_err()
            {
                return;
            };
            append_single_paint_path(last, &path, parent, &state.opt.processing);
        }
        ([_, _, PaintOrderKind::Markers], Some(markers_node)) => {
            let Ok(path) = crate::scene::clone_boxed(&state.opt.processing, &path) else {
                return;
            };
            if parent
                .children
                .try_push(Node::Path(path), &state.opt.processing)
                .is_err()
            {
                return;
            };
            let Ok(group) = crate::scene::boxed_owned(
                &state.opt.processing,
                crate::scene::SceneCost::NODE,
                markers_node,
            ) else {
                return;
            };
            if parent
                .children
                .try_push(Node::Group(group), &state.opt.processing)
                .is_err()
            {
                return;
            };
        }
        _ => {
            let Ok(path) = crate::scene::clone_boxed(&state.opt.processing, &path) else {
                return;
            };
            if parent
                .children
                .try_push(Node::Path(path), &state.opt.processing)
                .is_err()
            {
                return;
            };
        }
    }
}

fn append_single_paint_path(
    paint_order_kind: PaintOrderKind,
    path: &Path,
    parent: &mut Group,
    processing: &crate::ProcessingContext,
) {
    if processing.failed() {
        return;
    }
    match paint_order_kind {
        PaintOrderKind::Fill => {
            if path.fill.is_some() {
                let Ok(mut fill_path) = crate::scene::clone_boxed(processing, path) else {
                    return;
                };
                fill_path.as_mut().stroke = None;
                fill_path.as_mut().id = TreeString::new(processing);
                if parent
                    .children
                    .try_push(Node::Path(fill_path), processing)
                    .is_err()
                {
                    return;
                };
            }
        }
        PaintOrderKind::Stroke => {
            if path.stroke.is_some() {
                let Ok(mut stroke_path) = crate::scene::clone_boxed(processing, path) else {
                    return;
                };
                stroke_path.as_mut().fill = None;
                stroke_path.as_mut().id = TreeString::new(processing);
                if parent
                    .children
                    .try_push(Node::Path(stroke_path), processing)
                    .is_err()
                {
                    return;
                };
            }
        }
        _ => {}
    }
}

pub fn svg_paint_order_to_usvg(order: svgtypes::PaintOrder) -> PaintOrder {
    match (order.order[0], order.order[1]) {
        (svgtypes::PaintOrderKind::Stroke, _) => PaintOrder::StrokeAndFill,
        (svgtypes::PaintOrderKind::Markers, svgtypes::PaintOrderKind::Stroke) => {
            PaintOrder::StrokeAndFill
        }
        _ => PaintOrder::FillAndStroke,
    }
}

impl SvgNode<'_, '_> {
    pub(crate) fn resolve_transform(&self, transform_aid: AId, state: &State) -> Transform {
        let mut transform: Transform = self.attribute(transform_aid).unwrap_or_default();
        let transform_origin = self
            .attribute::<TransformOrigin>(AId::TransformOrigin)
            .or_else(|| {
                // The root SVG uses the CSS initial origin; other SVG elements use (0, 0).
                if self.tag_name() == Some(EId::Svg) && self.parent_element().is_none() {
                    Some(TransformOrigin::new(
                        Length::new(50.0, Unit::Percent),
                        Length::new(50.0, Unit::Percent),
                        Length::zero(),
                    ))
                } else {
                    None
                }
            });

        if let Some(transform_origin) = transform_origin {
            let dx = convert_length(
                transform_origin.x_offset,
                *self,
                AId::Width,
                Units::UserSpaceOnUse,
                state,
            );
            let dy = convert_length(
                transform_origin.y_offset,
                *self,
                AId::Height,
                Units::UserSpaceOnUse,
                state,
            );
            transform = Transform::default()
                .pre_translate(dx, dy)
                .pre_concat(transform)
                .pre_translate(-dx, -dy);
        }

        transform
    }
}

#[cfg(all(test, feature = "text"))]
#[path = "font_replay_color_tests.rs"]
mod font_replay_color_tests;

#[cfg(all(test, feature = "text"))]
pub(crate) use font_replay_color_tests::sfnt as glyph_coverage_sfnt;

#[cfg(all(test, feature = "text"))]
#[path = "representation_tests.rs"]
mod representation_tests;

#[cfg(test)]
#[path = "scene_parser_tests.rs"]
pub(crate) mod scene_parser_tests;

#[cfg(all(test, feature = "text"))]
#[path = "font_source_tests.rs"]
mod font_source_tests;

#[cfg(all(test, feature = "text"))]
mod natural_size_tests {
    use super::*;
    use markdown_svg_budget::{
        Backend, Error as ResourceError, FirstFailureRef, Limits, ResourceKind, ResourcesRef,
        Ticket,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    struct TestBackend(AtomicU32);
    impl Backend for TestBackend {
        fn reserve(&self, _: ResourceKind, _: usize) -> Result<Ticket, ResourceError> {
            Ticket::new(1, self.0.fetch_add(1, Ordering::Relaxed) + 1, 1)
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
            Ok(())
        }
        fn release_after_free(&self, _: Ticket) -> Result<(), ResourceError> {
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), ResourceError> {
            Ok(())
        }
    }

    fn controlled(source: &str) -> Result<(Tree, ResourcesRef), Error> {
        let resources = ResourcesRef::new_prepaid(
            TestBackend(AtomicU32::new(0)),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        let processing = Arc::new(crate::ProcessingContext::with_resources(resources.clone()));
        let db = Arc::new(fontdb::Database::with_families([
            "Times New Roman".into(),
            "Arial".into(),
            "Comic Sans MS".into(),
            "Impact".into(),
            "Courier New".into(),
        ]));
        let options = Options::with_resources_and_fontdb(
            processing,
            db,
            "Times New Roman".into(),
            vec!["en".into()],
        );
        let result = Tree::from_data_controlled(source.as_bytes(), &options);
        drop(options);
        match result {
            Ok(tree) => Ok((tree, resources)),
            Err(error) => {
                assert_eq!(resources.statistics().scratch_bytes, 0);
                if matches!(error, Error::InvalidSize) {
                    assert_eq!(
                        resources.failure().unwrap().kind,
                        markdown_svg_budget::FailureKind::Decode
                    );
                } else if let Error::Resource(record) = &error {
                    assert_eq!(resources.failure(), Some(*record));
                }
                resources.validate_retired().unwrap();
                Err(error)
            }
        }
    }

    fn check(source: &str, expected: (f64, f64)) {
        let (tree, resources) = controlled(source).unwrap();
        assert_eq!(tree.natural_size(), expected, "{source}");
        assert_eq!(tree.size().width(), expected.0 as f32);
        assert_eq!(tree.size().height(), expected.1 as f32);
        drop(tree);
        resources.validate_retired().unwrap();
    }

    #[test]
    fn fractional_natural_dip_is_separate_from_f32_viewport() {
        check("<svg width='100.5' height='50.25'/>", (100.5, 50.25));
        let (tree, resources) =
            controlled("<svg width='100.1000000001' height='50.05000000005'/>").unwrap();
        assert_eq!(tree.natural_size(), (100.1000000001, 50.05000000005));
        assert_ne!(tree.natural_size().0, f64::from(tree.size().width()));
        drop(tree);
        resources.validate_retired().unwrap();
    }

    #[test]
    fn physical_units_and_root_font_units_use_96_dpi_and_font16() {
        for source in [
            "<svg width='1in' height='72pt'/>",
            "<svg width='2.54cm' height='25.4mm'/>",
            "<svg width='6pc' height='96px'/>",
            "<svg width='6em' height='12ex'/>",
        ] {
            let (tree, resources) = controlled(source).unwrap();
            let natural = tree.natural_size();
            assert!((natural.0 - 96.0).abs() <= 2e-13, "{source}");
            assert!((natural.1 - 96.0).abs() <= 2e-13, "{source}");
            assert_eq!(tree.size(), Size::from_wh(96.0, 96.0).unwrap());
            drop(tree);
            resources.validate_retired().unwrap();
        }
        check(
            "<svg style='font-size:200%' width='3em' height='6ex'/>",
            (96.0, 96.0),
        );
    }

    #[test]
    fn root_viewbox_single_axis_percent_and_fallback_rules() {
        for (source, expected) in [
            ("<svg viewBox='5 7 100.5 50.25'/>", (100.5, 50.25)),
            ("<svg viewBox='5 7 200 100' width='25'/>", (25.0, 12.5)),
            ("<svg viewBox='5 7 200 100' height='25'/>", (50.0, 25.0)),
            (
                "<svg viewBox='5 7 200 100' width='50%' height='25%'/>",
                (100.0, 25.0),
            ),
            ("<svg viewBox='5 7 200 100' width='50%'/>", (100.0, 50.0)),
            ("<svg/>", (300.0, 150.0)),
            ("<svg width='50%' height='25%'/>", (150.0, 37.5)),
            ("<svg width='80'/>", (80.0, 150.0)),
            ("<svg height='80'/>", (300.0, 80.0)),
        ] {
            check(source, expected);
        }
    }

    #[test]
    fn dimensions_use_the_same_css_cascade_as_actual_tree() {
        check(
            "<svg font-size='8' width='6.28125em' height='3.140625em' style='font-size:16px'/>",
            (100.5, 50.25),
        );
        check(
            "<svg font-size='8' width='6.28125em' height='3.140625em'><style>svg { font-size: 16px; }</style></svg>",
            (100.5, 50.25),
        );
    }

    #[test]
    fn drawing_bbox_does_not_replace_controlled_fallback_viewport() {
        check("<svg><rect width='20' height='10'/></svg>", (300.0, 150.0));
        let legacy = Tree::from_str(
            "<svg><rect width='20' height='10'/></svg>",
            &Options::default(),
        )
        .unwrap();
        assert_eq!(legacy.natural_size(), (20.0, 10.0));
        assert_eq!(legacy.size(), Size::from_wh(20.0, 10.0).unwrap());
        assert_eq!(Options::default().font_size, 12.0);
        assert_eq!(
            Options::default().default_size,
            Size::from_wh(100.0, 100.0).unwrap()
        );
    }

    #[test]
    fn invalid_root_dimensions_and_viewboxes_are_decode_errors() {
        for source in [
            "<svg width='0'/>",
            "<svg height='-1'/>",
            "<svg width='NaN'/>",
            "<svg width='1e309'/>",
            "<svg viewBox='0 0 0 1'/>",
            "<svg viewBox='0 0 -1 1'/>",
            "<svg viewBox='0 0 1 1 9'/>",
            "<svg viewBox='0 0 1 1 garbage'/>",
            "<svg viewBox='0 0 1 1e100'/>",
        ] {
            assert!(
                matches!(controlled(source), Err(Error::InvalidSize)),
                "{source}"
            );
        }
    }

    #[test]
    fn actual_viewport_requires_finite_positive_f32_and_retains_tiny_natural_value() {
        check("<svg width='1e-40' height='1e-40'/>", (1e-40, 1e-40));
        check("<svg width='8192' height='2048'/>", (8192.0, 2048.0));
        check("<svg width='4096' height='4096'/>", (4096.0, 4096.0));
        assert!(matches!(
            controlled("<svg width='1e-100'/>"),
            Err(Error::InvalidSize)
        ));
        // Inspect natural f64 before viewport rounding can lose the excess.
        assert_eq!(8192.00000001_f64 as f32, 8192.0);
        for source in [
            "<svg width='8192.00000001' height='1'/>",
            "<svg height='1e100'/>",
            "<svg width='8192' height='2048.000000001'/>",
        ] {
            assert!(
                matches!(controlled(source), Err(Error::Resource(error))
                if error.kind == markdown_svg_budget::FailureKind::Quota
                    && error.limit == markdown_svg_budget::LimitKind::Capacity),
                "{source}"
            );
        }
    }

    #[test]
    fn root_mapping_and_nested_svg_keep_the_actual_parent_viewport() {
        for (source, width, height) in [
            (
                "<svg><rect width='100%' height='100%'/></svg>",
                300.0,
                150.0,
            ),
            (
                "<svg><svg width='50%' height='50%'><rect width='100%' height='100%'/></svg></svg>",
                150.0,
                75.0,
            ),
            (
                "<svg viewBox='5 7 200 100' width='100'><rect x='5' y='7' width='200' height='100'/></svg>",
                100.0,
                50.0,
            ),
        ] {
            let (tree, resources) = controlled(source).unwrap();
            let bbox = tree.root().abs_bounding_box();
            assert_eq!(bbox.x(), 0.0, "{source}");
            assert_eq!(bbox.y(), 0.0, "{source}");
            assert_eq!(bbox.width(), width, "{source}");
            assert_eq!(bbox.height(), height, "{source}");
            drop(tree);
            resources.validate_retired().unwrap();
        }
    }

    #[test]
    fn controlled_constructor_moves_existing_owners_and_families() {
        let resources = ResourcesRef::new_prepaid(
            TestBackend(AtomicU32::new(0)),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        let processing = Arc::new(crate::ProcessingContext::with_resources(resources));
        let families = ["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        let db = Arc::new(fontdb::Database::with_families(families));
        let family = "funded family".to_owned();
        let family_ptr = family.as_ptr();
        let languages = vec!["funded-language".to_owned()];
        let languages_ptr = languages.as_ptr();
        let options =
            Options::with_resources_and_fontdb(processing.clone(), db.clone(), family, languages);
        assert!(Arc::ptr_eq(&processing, &options.processing));
        assert!(Arc::ptr_eq(&db, &options.fontdb));
        assert_eq!(options.font_family.as_ptr(), family_ptr);
        assert_eq!(options.languages.as_ptr(), languages_ptr);
        for (family, name) in [
            (fontdb::Family::Serif, "a"),
            (fontdb::Family::SansSerif, "b"),
            (fontdb::Family::Cursive, "c"),
            (fontdb::Family::Fantasy, "d"),
            (fontdb::Family::Monospace, "e"),
        ] {
            assert_eq!(options.fontdb.family_name(&family), name);
        }
        assert_eq!((options.dpi, options.font_size), (96.0, 16.0));
        assert_eq!(options.default_size, Size::from_wh(300.0, 150.0).unwrap());
    }

    #[test]
    fn font_source_constructor_keeps_default_semantics_and_pays_real_buffers() {
        let resources = ResourcesRef::new_prepaid(
            TestBackend(AtomicU32::new(0)),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        let processing = Arc::new(crate::ProcessingContext::with_resources(resources.clone()));
        let db = Arc::new(fontdb::Database::with_families(Default::default()));
        let options = Options::font_source_options(processing.clone(), db.clone(), None).unwrap();
        let defaults = Options::default();
        assert!(Arc::ptr_eq(&processing, &options.processing));
        assert!(Arc::ptr_eq(&db, &options.fontdb));
        assert_eq!(options.font_family, defaults.font_family);
        assert_eq!(options.languages, defaults.languages);
        assert_eq!(options.font_size, defaults.font_size);
        assert_eq!(options.default_size, defaults.default_size);
        assert_eq!(
            resources.statistics().scratch_bytes as usize,
            options.font_family.capacity()
                + options.languages[0].capacity()
                + options.languages.capacity() * std::mem::size_of::<String>()
        );
        assert_eq!(resources.statistics().live_leases, 1);
        drop(options);
        assert_eq!(resources.statistics().scratch_bytes, 0);
        drop(processing);
        resources.validate_retired().unwrap();
    }
}
