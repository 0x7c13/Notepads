// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Borrow-only policy traversal. Shared graphs use an explicit paid worklist;
//! the old owned scene depth limit is never applied to shared resource edges.
use super::owner::{self, PolicyValue, Result};
use super::*;
use markdown_svg_budget::{
    BucketAllocator, BudgetVec, Error, FailureKind, LimitKind, RequestControl, ResourceKind,
    WorkControl,
};
use std::{
    collections::{hash_map::RandomState, HashSet},
    hash::BuildHasher,
};
#[derive(Clone, Copy)]
pub(crate) enum Frame<'a> {
    Tree(&'a Tree),
    Node(&'a Node),
    Group(&'a Group),
    Path(&'a Path),
    Image(&'a Image),
    ImageKind(&'a ImageKind),
    Text(&'a Text),
    Paint(&'a Paint),
    Fill(&'a Fill),
    Stroke(&'a Stroke),
    BaseGradient(&'a BaseGradient),
    Linear(&'a LinearGradient),
    Radial(&'a RadialGradient),
    Pattern(&'a Pattern),
    Clip(&'a ClipPath),
    Mask(&'a Mask),
    Filter(&'a filter::Filter),
    Primitive(&'a filter::Primitive),
    FilterKind(&'a filter::Kind),
    Font(&'a Font),
    Chunk(&'a TextChunk),
    TextSpan(&'a TextSpan),
    Flow(&'a TextFlow),
    TextPath(&'a TextPath),
    DecorationStyle(&'a TextDecorationStyle),
    DecorationOwner(&'a DecorationOwnerStyle),
    #[cfg(feature = "text")]
    LayoutSpan(&'a crate::layout::Span),
    #[cfg(feature = "text")]
    PositionedGlyph(&'a crate::layout::PositionedGlyph),
    #[cfg(feature = "text")]
    Representation(&'a crate::text::representation::LocalRepresentation),
    #[cfg(feature = "text")]
    Payload(&'a crate::text::representation::LocalPayload),
}

// Fixed immediate root sources only. No account, clock or owning cleanup is
// introduced here, and no dynamic child/list traversal happens before work is paid.
struct RootPolicy<'a, 'p> {
    target: &'p crate::ProcessingContext,
    sources: [Option<&'a markdown_svg_budget::ResourcesRef>; 32],
    source_count: usize,
    contexts: [Option<&'a crate::ProcessingContext>; 2],
    context_count: usize,
    mismatch: bool,
}
impl<'a, 'p> RootPolicy<'a, 'p> {
    fn new(target: &'p crate::ProcessingContext) -> Self {
        Self {
            target,
            sources: [None; 32],
            source_count: 0,
            contexts: [None; 2],
            context_count: 0,
            mismatch: false,
        }
    }
    fn owner(&mut self, source: Option<&'a markdown_svg_budget::ResourcesRef>) {
        self.mismatch |= !match (self.target.resources(), source) {
            (None, None) => true,
            (Some(a), Some(b)) => a.is_same_request(b),
            _ => false,
        };
        if let Some(source) = source {
            if self.source_count == self.sources.len() {
                self.mismatch = true;
                return;
            }
            self.sources[self.source_count] = Some(source);
            self.source_count += 1;
        }
    }
    fn context(&mut self, p: &'a crate::ProcessingContext) {
        if self.context_count == self.contexts.len() {
            self.mismatch = true;
            return;
        }
        self.contexts[self.context_count] = Some(p);
        self.context_count += 1;
        self.owner(p.resources());
    }
    fn group(&mut self, v: &'a Group) {
        self.owner(v.origin.source_resources());
        self.owner(v.id.source_resources());
        self.owner(v.children.source_resources());
        self.owner(v.filters.source_resources());
        if let Some(x) = &v.clip_path {
            self.owner(x.source_resources());
        }
        if let Some(x) = &v.mask {
            self.owner(x.source_resources());
        }
    }
    fn tree(&mut self, v: &'a Tree) {
        self.context(&v.processing);
        self.group(&v.root);
        self.owner(v.linear_gradients.source_resources());
        self.owner(v.radial_gradients.source_resources());
        self.owner(v.patterns.source_resources());
        self.owner(v.clip_paths.source_resources());
        self.owner(v.masks.source_resources());
        self.owner(v.filters.source_resources());
    }
    fn paint(&mut self, v: &'a Paint) {
        match v {
            Paint::Color(_) => {}
            Paint::LinearGradient(x) => self.owner(x.source_resources()),
            Paint::RadialGradient(x) => self.owner(x.source_resources()),
            Paint::Pattern(x) => self.owner(x.source_resources()),
        }
    }
    fn path(&mut self, v: &'a Path) {
        self.owner(v.origin.source_resources());
        self.owner(v.id.source_resources());
        self.owner(v.data.source_resources());
        self.owner(v.data.request_resources());
        if let Some(x) = &v.fill {
            self.paint(&x.paint);
        }
        if let Some(x) = &v.stroke {
            self.paint(&x.paint);
        }
    }
    fn image_kind(&mut self, v: &'a ImageKind) {
        match v {
            ImageKind::SVG(x) => self.tree(x),
            ImageKind::JPEG(x) | ImageKind::PNG(x) | ImageKind::GIF(x) | ImageKind::WEBP(x) => {
                for source in x.root_resources() {
                    self.owner(source);
                }
            }
        }
    }
    fn image(&mut self, v: &'a Image) {
        self.owner(v.origin.source_resources());
        self.owner(v.id.source_resources());
        self.image_kind(&v.kind);
    }
    fn text(&mut self, v: &'a Text) {
        self.owner(v.origin.source_resources());
        self.owner(v.id.source_resources());
        self.owner(v.chunks.source_resources());
        self.owner(v.flattened.source_resources());
        self.group(&v.flattened);
        #[cfg(feature = "text")]
        self.owner(v.layouted.source_resources());
    }
    fn node(&mut self, v: &'a Node) {
        match v {
            Node::Group(x) => {
                self.owner(x.source_resources());
                self.group(x);
            }
            Node::Path(x) => {
                self.owner(x.source_resources());
                self.path(x);
            }
            Node::Image(x) => {
                self.owner(x.source_resources());
                self.image(x);
            }
            Node::Text(x) => {
                self.owner(x.source_resources());
                self.text(x);
            }
        }
    }
    fn font(&mut self, v: &'a Font) {
        self.owner(v.families.source_resources());
        self.owner(v.variations.source_resources());
    }
    fn finish(self) -> Result<()> {
        // Read every fixed source's old slot before recording identity errors.
        // The source context matters even for a Plain Tree with no ResourcesRef.
        let target_old = self.target.resource_failure();
        let mut context_old = None;
        for source in self.contexts[..self.context_count].iter().flatten() {
            let prior = source.resource_failure();
            if context_old.is_none() {
                context_old = prior;
            }
        }
        let mut resource_old = None;
        for source in self.sources[..self.source_count].iter().flatten() {
            let prior = source.failure();
            if resource_old.is_none() {
                resource_old = prior;
            }
        }
        let old = target_old.or(context_old).or(resource_old);
        if self.mismatch || old.is_some() {
            let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
            let chosen = self.target.record_resource(old.unwrap_or(invariant));
            if self.mismatch {
                for source in self.contexts[..self.context_count].iter().flatten() {
                    source.record_resource(invariant);
                }
                for source in self.sources[..self.source_count].iter().flatten() {
                    source.record(invariant);
                }
            }
            return Err(chosen);
        }
        Ok(())
    }
}
impl<'a> Frame<'a> {
    /// Capture the bounded immediate U tags/headers before any callback. Opaque
    /// P/Frozen ownership still uses its own complete, callback-free validator;
    /// unknown descendant faults are not assigned a global error order.
    fn preflight(self, p: &crate::ProcessingContext) -> Result<()> {
        let mut roots = RootPolicy::new(p);
        match self {
            Self::Tree(v) => roots.tree(v),
            Self::Group(v) => roots.group(v),
            Self::Path(v) => roots.path(v),
            Self::Image(v) => roots.image(v),
            Self::Text(v) => roots.text(v),
            Self::Node(v) => roots.node(v),
            Self::ImageKind(v) => roots.image_kind(v),
            Self::Paint(v) => roots.paint(v),
            Self::Fill(v) => roots.paint(&v.paint),
            Self::Stroke(v) => roots.paint(&v.paint),
            Self::BaseGradient(v) => {
                roots.owner(v.id.0.source_resources());
                roots.owner(v.stops.source_resources());
            }
            Self::Linear(v) => {
                roots.owner(v.base.id.0.source_resources());
                roots.owner(v.base.stops.source_resources());
            }
            Self::Radial(v) => {
                roots.owner(v.base.id.0.source_resources());
                roots.owner(v.base.stops.source_resources());
            }
            Self::Pattern(v) => {
                roots.owner(v.id.0.source_resources());
                roots.group(&v.root);
            }
            Self::Clip(v) => {
                roots.owner(v.id.0.source_resources());
                roots.group(&v.root);
                if let Some(x) = &v.clip_path {
                    roots.owner(x.source_resources());
                }
            }
            Self::Mask(v) => {
                roots.owner(v.id.0.source_resources());
                roots.group(&v.root);
                if let Some(x) = &v.mask {
                    roots.owner(x.source_resources());
                }
            }
            Self::Filter(v) => {
                roots.owner(v.id.0.source_resources());
                roots.owner(v.primitives.source_resources());
            }
            Self::Primitive(v) => roots.owner(v.result.source_resources()),
            Self::Font(v) => roots.font(v),
            Self::Chunk(v) => {
                roots.owner(v.text.source_resources());
                roots.owner(v.spans.source_resources());
                roots.owner(v.source_addresses.source_resources());
            }
            Self::TextSpan(v) => {
                roots.owner(v.source_ancestors.source_resources());
                roots.owner(v.language.source_resources());
                roots.owner(v.bidi_scopes.source_resources());
                roots.owner(v.baseline_shift.source_resources());
                roots.font(&v.font);
            }
            Self::TextPath(v) => {
                roots.owner(v.id.0.source_resources());
                roots.owner(v.path.source_resources());
                roots.owner(v.path.request_resources());
            }
            Self::DecorationOwner(v) => {
                roots.owner(v.language.source_resources());
                roots.owner(v.scopes.source_resources());
                roots.owner(v.baseline_shift.source_resources());
                roots.font(&v.font);
            }
            #[cfg(feature = "text")]
            Self::LayoutSpan(v) => {
                roots.owner(v.variations.source_resources());
                roots.owner(v.positioned_glyphs.source_resources());
            }
            #[cfg(feature = "text")]
            Self::PositionedGlyph(v) => {
                roots.owner(v.text.source_resources());
                if let Some(x) = &v.local_representation {
                    roots.owner(x.source_resources());
                }
            }
            _ => {}
        }
        roots.finish()?;
        match self {
            Self::Path(v) => v
                .data
                .as_ref()
                .validate_policy(p.resources())
                .map_err(|e| p.record_resource(e)),
            Self::Text(v) => {
                v.dx.validate_policy(p.resources())
                    .map_err(|e| p.record_resource(e))?;
                v.dy.validate_policy(p.resources())
                    .map_err(|e| p.record_resource(e))?;
                v.rotate
                    .validate_policy(p.resources())
                    .map_err(|e| p.record_resource(e))
            }
            Self::Stroke(v) => PolicyValue::validate_policy(&v.dasharray, p),
            _ => Ok(()),
        }
    }
}

enum Frames<'a> {
    Plain(Vec<Frame<'a>>),
    Paid(BudgetVec<Frame<'a>>),
}
enum Visited {
    Plain(HashSet<(usize, u8)>),
    Paid(hashbrown::HashMap<(usize, u8), (), RandomState, BucketAllocator>),
}
pub(crate) struct Walker<'a, 'p> {
    frames: Frames<'a>,
    seen: Visited,
    pub(crate) processing: &'p crate::ProcessingContext,
}
impl<'a, 'p> Walker<'a, 'p> {
    pub(crate) fn push(&mut self, frame: Frame<'a>) -> Result<()> {
        frame.preflight(self.processing)?;
        owner::check(self.processing, 1)?;
        match &mut self.frames {
            Frames::Plain(v) => {
                v.push(frame);
                Ok(())
            }
            Frames::Paid(v) => v
                .try_push(frame)
                .map_err(|e| self.processing.record_resource(e)),
        }
    }
    fn seen(&mut self, key: (usize, u8)) -> Result<bool> {
        let p = self.processing;
        owner::check(p, 1)?;
        match &mut self.seen {
            Visited::Plain(v) => Ok(!v.insert(key)),
            Visited::Paid(v) => {
                let r = v.allocator().resources().clone();
                let builder = v.hasher().clone();
                let hash = builder.hash_one(key);
                if v.try_get_with_control(hash, &key, &mut RequestControl(&r), |a, b, c| {
                    c.checkpoint(1)?;
                    Ok(a == b)
                })?
                .is_some()
                {
                    return Ok(true);
                }
                v.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(builder.hash_one(*key))
                })
                .map_err(|e| map_error(p, e))?;
                v.try_insert_with_control(
                    hash,
                    key,
                    (),
                    &mut RequestControl(&r),
                    |a, b, c| {
                        c.checkpoint(1)?;
                        Ok(a == b)
                    },
                    |key, c| {
                        c.checkpoint(1)?;
                        Ok(builder.hash_one(*key))
                    },
                )
                .map_err(|e| map_error(p, e))?;
                Ok(false)
            }
        }
    }
    fn shared<T>(&mut self, v: &'a SharedOwner<T>, tag: u8, frame: Frame<'a>) -> Result<()> {
        v.validate_header(self.processing)?;
        frame.preflight(self.processing)?;
        if !self.seen((v.identity(), tag))? {
            self.push(frame)?;
        }
        Ok(())
    }
    fn path_data(&mut self, data: &SharedOwner<tiny_skia_path::Path>) -> Result<()> {
        data.validate_header(self.processing)?;
        if !self.seen((data.identity(), 1))? {
            PolicyValue::validate_policy(data.as_ref(), self.processing)?;
        }
        Ok(())
    }
    #[cfg(feature = "text")]
    pub(crate) fn representation(
        &mut self,
        v: &'a SharedOwner<crate::text::representation::LocalRepresentation>,
    ) -> Result<()>
    where
        Self: Sized,
    {
        self.shared(v, 10, Frame::Representation(v))
    }
    fn paint(&mut self, v: &'a Paint) -> Result<()> {
        self.push(Frame::Paint(v))
    }
    fn step(&mut self, frame: Frame<'a>) -> Result<()> {
        let p = self.processing;
        owner::check(p, 1)?;
        match frame {
            Frame::Tree(v) => {
                if let Some(e) = v.processing.resource_failure() {
                    return Err(p.record_resource(e));
                }
                owner::validate_owner(p, v.processing.resources())?;
                v.linear_gradients.validate_container(p)?;
                for x in &v.linear_gradients {
                    self.shared(x, 2, Frame::Linear(x))?;
                }
                v.radial_gradients.validate_container(p)?;
                for x in &v.radial_gradients {
                    self.shared(x, 3, Frame::Radial(x))?;
                }
                v.patterns.validate_container(p)?;
                for x in &v.patterns {
                    self.shared(x, 4, Frame::Pattern(x))?;
                }
                v.clip_paths.validate_container(p)?;
                for x in &v.clip_paths {
                    self.shared(x, 5, Frame::Clip(x))?;
                }
                v.masks.validate_container(p)?;
                for x in &v.masks {
                    self.shared(x, 6, Frame::Mask(x))?;
                }
                v.filters.validate_container(p)?;
                for x in &v.filters {
                    self.shared(x, 7, Frame::Filter(x))?;
                }
                self.push(Frame::Group(&v.root))?;
            }
            Frame::Node(v) => match v {
                Node::Group(v) => {
                    v.validate_header(p)?;
                    self.push(Frame::Group(v))?;
                }
                Node::Path(v) => {
                    v.validate_header(p)?;
                    self.push(Frame::Path(v))?;
                }
                Node::Image(v) => {
                    v.validate_header(p)?;
                    self.push(Frame::Image(v))?;
                }
                Node::Text(v) => {
                    v.validate_header(p)?;
                    self.push(Frame::Text(v))?;
                }
            },
            Frame::Group(v) => {
                v.origin.validate(p)?;
                v.id.validate_policy(p)?;
                if let Some(x) = &v.clip_path {
                    self.shared(x, 5, Frame::Clip(x))?;
                }
                if let Some(x) = &v.mask {
                    self.shared(x, 6, Frame::Mask(x))?;
                }
                v.filters.validate_container(p)?;
                for x in &v.filters {
                    self.shared(x, 7, Frame::Filter(x))?;
                }
                v.children.validate_container(p)?;
                for x in &v.children {
                    self.push(Frame::Node(x))?;
                }
            }
            Frame::Path(v) => {
                v.origin.validate(p)?;
                v.id.validate_policy(p)?;
                self.path_data(&v.data)?;
                #[cfg(feature = "text")]
                v.glyph_fill_source.validate_policy(p)?;
                if let Some(v) = &v.fill {
                    self.push(Frame::Fill(v))?;
                }
                if let Some(v) = &v.stroke {
                    self.push(Frame::Stroke(v))?;
                }
            }
            Frame::Image(v) => {
                v.origin.validate(p)?;
                v.id.validate_policy(p)?;
                self.push(Frame::ImageKind(&v.kind))?;
            }
            Frame::ImageKind(v) => match v {
                ImageKind::SVG(v) => self.push(Frame::Tree(v))?,
                ImageKind::JPEG(v) | ImageKind::PNG(v) | ImageKind::GIF(v) | ImageKind::WEBP(v) => {
                    PolicyValue::validate_policy(v, p)?
                }
            },
            Frame::Text(v) => {
                v.origin.validate(p)?;
                v.id.validate_policy(p)?;
                v.dx.validate_policy(p.resources())
                    .map_err(|e| p.record_resource(e))?;
                v.dy.validate_policy(p.resources())
                    .map_err(|e| p.record_resource(e))?;
                v.rotate
                    .validate_policy(p.resources())
                    .map_err(|e| p.record_resource(e))?;
                v.chunks.validate_container(p)?;
                for x in &v.chunks {
                    self.push(Frame::Chunk(x))?;
                }
                v.flattened.validate_header(p)?;
                self.push(Frame::Group(&v.flattened))?;
                #[cfg(feature = "text")]
                {
                    v.layouted.validate_container(p)?;
                    for x in &v.layouted {
                        self.push(Frame::LayoutSpan(x))?;
                    }
                }
            }
            Frame::Paint(v) => match v {
                Paint::Color(_) => {}
                Paint::LinearGradient(v) => self.shared(v, 2, Frame::Linear(v))?,
                Paint::RadialGradient(v) => self.shared(v, 3, Frame::Radial(v))?,
                Paint::Pattern(v) => self.shared(v, 4, Frame::Pattern(v))?,
            },
            Frame::Fill(v) => self.paint(&v.paint)?,
            Frame::Stroke(v) => {
                PolicyValue::validate_policy(&v.dasharray, p)?;
                self.paint(&v.paint)?;
            }
            Frame::BaseGradient(v) => {
                v.id.validate_policy(p)?;
                v.stops.validate_container(p)?;
            }
            Frame::Linear(v) => self.push(Frame::BaseGradient(&v.base))?,
            Frame::Radial(v) => self.push(Frame::BaseGradient(&v.base))?,
            Frame::Pattern(v) => {
                v.id.validate_policy(p)?;
                self.push(Frame::Group(&v.root))?;
            }
            Frame::Clip(v) => {
                v.id.validate_policy(p)?;
                if let Some(x) = &v.clip_path {
                    self.shared(x, 5, Frame::Clip(x))?;
                }
                self.push(Frame::Group(&v.root))?;
            }
            Frame::Mask(v) => {
                v.id.validate_policy(p)?;
                if let Some(x) = &v.mask {
                    self.shared(x, 6, Frame::Mask(x))?;
                }
                self.push(Frame::Group(&v.root))?;
            }
            Frame::Filter(v) => {
                v.id.validate_policy(p)?;
                v.primitives.validate_container(p)?;
                for x in &v.primitives {
                    self.push(Frame::Primitive(x))?;
                }
            }
            Frame::Primitive(v) => {
                v.result.validate_policy(p)?;
                self.push(Frame::FilterKind(&v.kind))?;
            }
            Frame::FilterKind(v) => match v {
                filter::Kind::Image(v) => self.push(Frame::Group(&v.root))?,
                filter::Kind::Blend(v) => v.validate_policy(p)?,
                filter::Kind::ColorMatrix(v) => v.validate_policy(p)?,
                filter::Kind::ComponentTransfer(v) => v.validate_policy(p)?,
                filter::Kind::Composite(v) => v.validate_policy(p)?,
                filter::Kind::ConvolveMatrix(v) => v.validate_policy(p)?,
                filter::Kind::DiffuseLighting(v) => v.validate_policy(p)?,
                filter::Kind::DisplacementMap(v) => v.validate_policy(p)?,
                filter::Kind::DropShadow(v) => v.validate_policy(p)?,
                filter::Kind::Flood(v) => v.validate_policy(p)?,
                filter::Kind::GaussianBlur(v) => v.validate_policy(p)?,
                filter::Kind::Merge(v) => v.validate_policy(p)?,
                filter::Kind::Morphology(v) => v.validate_policy(p)?,
                filter::Kind::Offset(v) => v.validate_policy(p)?,
                filter::Kind::SpecularLighting(v) => v.validate_policy(p)?,
                filter::Kind::Tile(v) => v.validate_policy(p)?,
                filter::Kind::Turbulence(v) => v.validate_policy(p)?,
            },
            Frame::Font(v) => {
                v.families.validate_container(p)?;
                for x in &v.families {
                    x.validate_policy(p)?;
                }
                v.variations.validate_container(p)?;
            }
            Frame::Chunk(v) => {
                v.source_addresses.validate_container(p)?;
                v.text.validate_policy(p)?;
                v.spans.validate_container(p)?;
                for x in &v.spans {
                    self.push(Frame::TextSpan(x))?;
                }
                self.push(Frame::Flow(&v.text_flow))?;
            }
            Frame::TextSpan(v) => {
                v.source_ancestors.validate_container(p)?;
                #[cfg(feature = "text")]
                v.length_directives.validate_container(p)?;
                v.language.validate_policy(p)?;
                v.bidi_scopes.validate_container(p)?;
                v.baseline_shift.validate_container(p)?;
                for source in v.decoration_sources.iter().flatten() {
                    self.shared(source, 8, Frame::DecorationOwner(source))?;
                }
                self.push(Frame::Font(&v.font))?;
                if let Some(x) = &v.fill {
                    self.push(Frame::Fill(x))?;
                }
                if let Some(x) = &v.stroke {
                    self.push(Frame::Stroke(x))?;
                }
                for x in [
                    &v.decoration.underline,
                    &v.decoration.overline,
                    &v.decoration.line_through,
                ]
                .into_iter()
                .flatten()
                {
                    self.push(Frame::DecorationStyle(x))?;
                }
            }
            Frame::Flow(v) => {
                if let TextFlow::Path(v) = v {
                    self.shared(v, 9, Frame::TextPath(v))?;
                }
            }
            Frame::TextPath(v) => {
                v.id.validate_policy(p)?;
                self.path_data(&v.path)?;
            }
            Frame::DecorationStyle(v) => {
                if let Some(x) = &v.fill {
                    self.push(Frame::Fill(x))?;
                }
                if let Some(x) = &v.stroke {
                    self.push(Frame::Stroke(x))?;
                }
            }
            Frame::DecorationOwner(v) => {
                v.language.validate_policy(p)?;
                v.scopes.validate_container(p)?;
                v.baseline_shift.validate_container(p)?;
                self.push(Frame::Font(&v.font))?;
            }
            #[cfg(feature = "text")]
            Frame::LayoutSpan(v) => v.queue_policy(self)?,
            #[cfg(feature = "text")]
            Frame::PositionedGlyph(v) => v.queue_policy(self)?,
            #[cfg(feature = "text")]
            Frame::Representation(v) => self.push(Frame::Payload(&v.payload))?,
            #[cfg(feature = "text")]
            Frame::Payload(v) => {
                use crate::text::representation::LocalPayload;
                match v {
                    LocalPayload::Colr(v) => self.push(Frame::Group(v))?,
                    LocalPayload::Svg(v) => self.push(Frame::Node(v))?,
                    LocalPayload::Bitmap(v) => self.push(Frame::Image(&v.image))?,
                    LocalPayload::Ordinary(v) => {
                        if let Some(v) = v {
                            PolicyValue::validate_policy(v, p)?;
                        }
                    }
                    LocalPayload::Synthetic { data, .. } => {
                        if let Some(v) = data {
                            self.path_data(v)?;
                        }
                    }
                }
            }
        }
        owner::check(p, 0)
    }
}
fn map_error(p: &crate::ProcessingContext, e: hashbrown::ControlError<Error>) -> Error {
    p.record_resource(match e {
        hashbrown::ControlError::Control(e) => e,
        hashbrown::ControlError::Reserve(hashbrown::TryReserveError::CapacityOverflow) => {
            Error::new(FailureKind::Quota, LimitKind::Capacity)
        }
        hashbrown::ControlError::Reserve(hashbrown::TryReserveError::AllocError { .. }) => {
            Error::new(FailureKind::Allocation, LimitKind::Capacity)
        }
    })
}
pub(crate) fn run(frame: Frame<'_>, p: &crate::ProcessingContext) -> Result<()> {
    frame.preflight(p)?;
    owner::check(p, 0)?;
    let (frames, seen) = match p.resources() {
        None => (Frames::Plain(Vec::new()), Visited::Plain(HashSet::new())),
        Some(r) => (
            Frames::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
            Visited::Paid(hashbrown::HashMap::with_hasher_in(
                RandomState::new(),
                BucketAllocator::new(r.clone(), ResourceKind::Scratch),
            )),
        ),
    };
    let mut walk = Walker {
        frames,
        seen,
        processing: p,
    };
    let result = (|| {
        walk.push(frame)?;
        loop {
            owner::check(p, 1)?;
            let frame = match &mut walk.frames {
                Frames::Plain(v) => v.pop(),
                Frames::Paid(v) => v.pop(),
            };
            let Some(frame) = frame else {
                break;
            };
            walk.step(frame)?;
        }
        Ok(())
    })()
    .map_err(|e| p.record_resource(e));
    drop(walk);
    owner::finish(p, result)
}
