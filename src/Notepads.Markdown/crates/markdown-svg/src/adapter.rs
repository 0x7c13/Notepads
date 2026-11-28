// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use crate::{memory::Context, search_work::SearchWork};
use crate::{
    abi::{self, Failure, Result},
    draft::DraftOwner,
    font::{FontStore, OwnedPlan},
    geometry::OwnedGeometry,
    shape::MetricsStore,
};
use markdown_svg_budget::ResourcesRef;
use std::sync::{Arc, Mutex};

struct State {
    database: Arc<fontdb::Database>,
    fonts: FontStore,
    geometry: markdown_svg_budget::BudgetVec<OwnedGeometry>,
    metrics: MetricsStore,
    next_draft: u64,
    glyphs: u64,
    observation: crate::observation::ObservationSink,
    // Drop the observer's paid bytes/strong owner before collecting its Weak fee.
    retained: Arc<crate::memory::RetainedPool>,
}
pub struct Adapter {
    callbacks: Context,
    resources: ResourcesRef,
    state: Mutex<State>,
}
impl std::fmt::Debug for Adapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontReplayAdapter")
            .field("session", &self.callbacks.session)
            .finish_non_exhaustive()
    }
}
impl Adapter {
    #[cfg(feature = "quality-observation")]
    pub(crate) fn enable_quality_observation(&self, capacity: usize) -> Result<()> {
        // Installation happens only in the creating worker's New state. Build
        // the paid owner outside the Adapter lock; no diagnostic Arc escapes.
        let retained = {
            let state = self
                .state
                .lock()
                .map_err(|_| self.callbacks.record_failure(Failure::Contract))?;
            if state.observation.is_enabled() {
                return Err(Failure::Contract);
            }
            state.retained.clone()
        };
        let recorder = retained.share(crate::quality_trace::QualityRecorder::new(
            &self.callbacks,
            capacity,
        )?)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| self.callbacks.record_failure(Failure::Contract))?;
        state.observation.install(recorder)
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn quality_trace_info(&self) -> Result<crate::QualityTraceInfo> {
        self.state
            .lock()
            .map_err(|_| Failure::Contract)?
            .observation
            .info()
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn copy_quality_trace(&self, destination: &mut [u8]) -> Result<usize> {
        self.state
            .lock()
            .map_err(|_| Failure::Contract)?
            .observation
            .copy(destination)
    }

    pub fn new(callbacks: &Context, resources: &ResourcesRef) -> Result<Self> {
        Ok(Self {
            callbacks: callbacks.clone(),
            resources: resources.clone(),
            state: Mutex::new(State {
                database: Arc::new(fontdb::Database::with_families([
                    "Times New Roman".to_owned(),
                    "Arial".to_owned(),
                    "Comic Sans MS".to_owned(),
                    "Impact".to_owned(),
                    "Courier New".to_owned(),
                ])),
                fonts: FontStore::new(resources),
                geometry: markdown_svg_budget::BudgetVec::new(
                    resources.clone(),
                    markdown_svg_budget::ResourceKind::Scratch,
                ),
                metrics: MetricsStore::new(),
                next_draft: 0,
                glyphs: 0,
                retained: crate::memory::RetainedPool::new(callbacks, resources)?,
                observation: crate::observation::ObservationSink::default(),
            }),
        })
    }
    fn prepare_inner(
        &self,
        state: &mut State,
        text: &usvg::Text,
        output_database: &mut Arc<fontdb::Database>,
    ) -> Result<usvg::ReplayText> {
        self.callbacks.checkpoint(1)?;
        state.next_draft = state
            .next_draft
            .checked_add(1)
            .ok_or_else(|| self.callbacks.record_failure(Failure::Contract))?;
        let draft = DraftOwner::build(&self.callbacks, &self.resources, text, state.next_draft)?;
        let plan = OwnedPlan::prepare(&self.callbacks, &draft.raw(), &draft.text)?;
        plan.validate_request(&draft)
            .map_err(|error| self.callbacks.record_failure(error))?;
        let (fonts, database) = state.fonts.stage_registration(
            &self.callbacks,
            &state.retained,
            &plan,
            &state.database,
        )?;
        // stage_registration completed every fallible registration operation,
        // including the final shared-fault/cancellation check.
        state.fonts = fonts;
        if let Some(database) = database {
            state.database = database;
        }
        *output_database = state.database.clone();
        #[cfg(feature = "quality-observation")]
        if state.observation.is_enabled() {
            use std::fmt::Write;
            state.observation.record(|trace| {
                write!(trace, "{{\"kind\":\"actualLogicalDraft\",\"draftId\":{},\"elementId\":", draft.id).map_err(|_| Failure::Contract)?;
                trace.quoted(text.id()).map_err(|_| Failure::Contract)?;
                trace.write_str(",\"text\":").map_err(|_| Failure::Contract)?;
                trace.quoted(&draft.text).map_err(|_| Failure::Contract)?;
                write!(trace, ",\"styles\":{},\"logicalSpans\":{},\"wholeParagraphBidi\":true,\"nativePlanGeneration\":{},\"styleInputs\":[", draft.styles.len(), draft.spans.len(), plan.generation).map_err(|_| Failure::Contract)?;
                for (index, style) in draft.request_styles.iter().enumerate() {
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"logicalUtf8\":[{},{}],\"family\":", style.utf8_start, style.utf8_length).map_err(|_| Failure::Contract)?;
                    let start = style.family_start as usize;
                    let end = start.checked_add(style.family_length as usize).ok_or(Failure::Contract)?;
                    let family = core::str::from_utf8(draft.strings.get(start..end).ok_or(Failure::Contract)?).map_err(|_| Failure::Contract)?;
                    trace.quoted(family).map_err(|_| Failure::Contract)?;
                    write!(trace, ",\"weight\":{},\"style\":{},\"stretch\":{},\"size\":{},\"direction\":{}}}", style.weight, style.style, style.stretch, style.size, style.direction).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"chunksUtf8\":[").map_err(|_| Failure::Contract)?;
                for (index, chunk) in draft.chunks.iter().enumerate() {
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "[{},{}]", chunk.start, chunk.end - chunk.start).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"paragraphsUtf8\":[").map_err(|_| Failure::Contract)?;
                for (index, paragraph) in draft.paragraphs.iter().enumerate() {
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "[{},{}]", paragraph.start, paragraph.end - paragraph.start).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("]}\n").map_err(|_| Failure::Contract)
            })?;
            state
                .fonts
                .observe_files(&self.callbacks, &state.observation, draft.id)?;
        }
        let prepared = crate::shape::shape(
            &self.callbacks,
            &self.resources,
            &state.retained,
            &draft,
            &plan,
            &state.fonts,
            &mut state.metrics,
            &state.observation,
            text,
        )?;
        state
            .observation
            .shaped(draft.id, plan.generation, &prepared)?;
        #[cfg(feature = "quality-observation")]
        if state.observation.is_enabled() {
            use std::fmt::Write;
            state.observation.record(|trace| {
                write!(trace, "{{\"kind\":\"actualPreparedShape\",\"draftId\":{},\"nativePlanGeneration\":{},\"coordinateSpace\":\"chunk-local-and-whole-text-UTF8\",\"chunks\":[", draft.id, plan.generation).map_err(|_| Failure::Contract)?;
                for (index, glyphs) in prepared.chunks.iter().enumerate() {
                    let chunk = draft.chunks.get(index).ok_or(Failure::Contract)?;
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"domain\":{index},\"wholeTextUtf8\":[{},{}],\"glyphs\":[", chunk.start, chunk.end - chunk.start).map_err(|_| Failure::Contract)?;
                    for (glyph_index, glyph) in glyphs.iter().enumerate() {
                        let identity = glyph.identity.as_ref().ok_or(Failure::Contract)?;
                        let (face, orientation) = state.fonts.descriptor(glyph.face.id)?;
                        let start = chunk.start.checked_add(glyph.start).ok_or(Failure::Contract)?;
                        let end = chunk.start.checked_add(glyph.end).ok_or(Failure::Contract)?;
                        if start >= end || end > chunk.end { return Err(Failure::Contract); }
                        if glyph_index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                        write!(trace, "{{\"id\":{},\"faceTicket\":{},\"orientation\":{orientation},\"chunkUtf8\":[{},{}],\"wholeTextUtf8\":[{start},{}],\"parent\":{},\"part\":{},\"partCount\":{},\"positionDomain\":{},\"visualSpan\":{},\"semanticEm\":{},\"nativeOrientation\":{},\"scriptISO15924\":{},\"advance\":[{},{}],\"offset\":[{},{}],\"fallbackScale\":{}}}", glyph.glyph, face.ticket, glyph.start, glyph.end - glyph.start, end - start, identity.parent, identity.part, identity.part_count, identity.position_domain, identity.visual_span, identity.semantic_em, identity.native_orientation, identity.script_be, glyph.advance, glyph.y_advance, glyph.x_offset, glyph.y_offset, glyph.fallback_scale).map_err(|_| Failure::Contract)?;
                    }
                    trace.write_str("]}").map_err(|_| Failure::Contract)?;
                }
                trace.write_str("]}\n").map_err(|_| Failure::Contract)
            })?;
        }
        let mut keys = markdown_svg_budget::BudgetVec::new(
            self.resources.clone(),
            markdown_svg_budget::ResourceKind::Scratch,
        );
        let mut key_work = SearchWork::new(&self.resources)
            .map_err(crate::memory::failure_from_resource)?;
        for glyph in prepared.chunks.iter().flatten() {
            key_work.charge().map_err(crate::memory::failure_from_resource)?;
            state.glyphs = state
                .glyphs
                .checked_add(1)
                .ok_or_else(|| self.callbacks.record_failure(Failure::Contract))?;
            let (face, orientation) = state
                .fonts
                .descriptor(glyph.face.id)
                .map_err(|error| self.callbacks.record_failure(error))?;
            if face.simulations != 0 {
                let key = abi::GlyphKey {
                    face_ticket: face.ticket,
                    glyph_id: glyph.glyph,
                    orientation,
                    reserved: 0,
                };
                #[cfg(test)]
                let retained_key_phase = crate::search_work::phase::Scope::new(crate::search_work::phase::Stage::RetainedKey, &self.resources);
                let already = key_work
                    .find(state.geometry.iter().flat_map(|batch| batch.paths.iter()), |path| path.key == key)
                    .map_err(crate::memory::failure_from_resource)?
                    .is_some();
                #[cfg(test)]
                retained_key_phase.finish();
                #[cfg(test)]
                let pending_key_phase = crate::search_work::phase::Scope::new(crate::search_work::phase::Stage::PendingKey, &self.resources);
                if !already && key_work
                    .find(keys.iter(), |candidate| **candidate == key)
                    .map_err(crate::memory::failure_from_resource)?
                    .is_none()
                {
                    keys.try_push(key)
                        .map_err(crate::memory::failure_from_resource)?;
                }
                #[cfg(test)]
                pending_key_phase.finish();
            }
        }
        key_work.finish().map_err(crate::memory::failure_from_resource)?;
        for keys in keys.chunks(4096) {
            #[cfg(feature = "quality-observation")]
            let batch = OwnedGeometry::fetch_observed(
                &self.callbacks,
                &self.resources,
                &state.retained,
                keys,
                &state.observation,
            )?;
            #[cfg(not(feature = "quality-observation"))]
            let batch =
                OwnedGeometry::fetch(&self.callbacks, &self.resources, &state.retained, keys)?;
            for path in batch.paths.iter() {
                if path.path.is_none()
                    && state
                        .metrics
                        .has_ink(path.key)
                        .map_err(|error| self.callbacks.record_failure(error))?
                {
                    return Err(self.callbacks.record_failure(Failure::Contract));
                }
            }
            state
                .geometry
                .try_push(batch)
                .map_err(crate::memory::failure_from_resource)?;
        }
        Ok(prepared)
    }
    pub fn counts(&self) -> Result<(u64, u64)> {
        let state = self
            .state
            .lock()
            .map_err(|_| self.callbacks.record_failure(Failure::Contract))?;
        Ok((state.next_draft, state.glyphs))
    }
    pub fn failure(&self) -> Option<Failure> {
        self.callbacks.failure()
    }
    fn observe(
        &self,
        callback: impl FnOnce(&mut State) -> Result<()>,
    ) -> core::result::Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| {
            self.callbacks.record_failure(Failure::Contract);
        })?;
        if self.failure().is_some() {
            return Err(());
        }
        match callback(&mut state) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.callbacks.record_failure(error);
                Err(())
            }
        }
    }
}
impl usvg::FontReplay for Adapter {
    fn coverage_epoch(&self) -> u64 {
        self.callbacks.session
    }
    fn observe_glyph_fill_source(
        &self,
        source: &usvg::GlyphFillSource,
        path: &usvg::Path,
    ) -> core::result::Result<(), ()> {
        #[cfg(not(feature = "quality-observation"))]
        {
            let _ = (source, path);
            Ok(())
        }
        #[cfg(feature = "quality-observation")]
        {
            use std::fmt::Write;
            self.observe(|state| {
                if !state.observation.is_enabled() {
                    return Ok(());
                }
                if source.epoch() != self.callbacks.session
                    || !source.belongs_to(&self.resources)
                    || path.glyph_fill_source().is_none_or(|value| !std::ptr::eq(value, source))
                {
                    return Err(self.callbacks.record_failure(Failure::Contract));
                }
                state.observation.record(|trace| {
                    write!(trace, "{{\"kind\":\"glyphFillSource\",\"epoch\":{},\"recordCapacityBytes\":{},\"glyphs\":[",
                        source.epoch(), source.retained_record_bytes()).map_err(|_| Failure::Contract)?;
                    for (index, glyph) in source.glyphs().iter().enumerate() {
                        let (face, orientation) = state.fonts.descriptor(glyph.font)?;
                        if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                        write!(trace, "{{\"faceTicket\":{},\"orientation\":{},\"gid\":{},\"upem\":{},\"emBits\":{},\"representation\":{},\"localOutlinePresent\":{},\"contributedToPath\":{},\"sourceSpan\":",
                            face.ticket, orientation, glyph.glyph, glyph.units_per_em,
                            glyph.font_size.to_bits(), u32::from(glyph.representation == usvg::GlyphOutlineKind::Synthetic),
                            glyph.local_outline_present, glyph.contributed_to_path).map_err(|_| Failure::Contract)?;
                        match glyph.source_span {
                            Some(value) => write!(trace, "{value}"),
                            None => trace.write_str("null"),
                        }.map_err(|_| Failure::Contract)?;
                        write!(trace, ",\"layoutSourceRange\":[{},{}],\"sourceIdentity\":",
                            glyph.layout_source_range[0], glyph.layout_source_range[1]).map_err(|_| Failure::Contract)?;
                        match glyph.source_identity {
                            Some(value) => write!(trace, "{{\"parent\":{},\"part\":{},\"partCount\":{},\"positionDomain\":{},\"visualSpan\":{},\"semanticEmBits\":{},\"nativeOrientation\":{},\"scriptISO15924\":{}}}",
                                value.parent,value.part,value.part_count,value.position_domain,value.visual_span,
                                value.semantic_em.to_bits(),value.native_orientation,value.script_be),
                            None => trace.write_str("null"),
                        }.map_err(|_| Failure::Contract)?;
                        write!(trace, ",\"geometryOrigin\":{},\"placementPolicy\":{}",
                            glyph.geometry_origin as u32,glyph.placement_policy as u32).map_err(|_| Failure::Contract)?;
                        for (name, matrix) in [
                            ("glyphMatrixBits",glyph.glyph_transform),
                            ("clusterMatrixBits",glyph.cluster_transform),
                            ("spanMatrixBits",glyph.span_transform),
                            ("outlineMatrixBits",glyph.outline_transform),
                        ] {
                            write!(trace, ",\"{name}\":[{},{},{},{},{},{}]",matrix.sx.to_bits(),matrix.ky.to_bits(),
                                matrix.kx.to_bits(),matrix.sy.to_bits(),matrix.tx.to_bits(),matrix.ty.to_bits()).map_err(|_| Failure::Contract)?;
                        }
                        trace.write_char('}').map_err(|_| Failure::Contract)?;
                    }
                    trace.write_str("],\"flattenPath\":{\"absMatrixBits\":").map_err(|_| Failure::Contract)?;
                    trace.matrix_bits(path.abs_transform()).map_err(|_| Failure::Contract)?;
                    write!(trace, ",\"visible\":{},\"hasStroke\":{},\"fillRule\":", path.is_visible(), path.stroke().is_some())
                        .map_err(|_| Failure::Contract)?;
                    match path.fill() {
                        Some(fill) => write!(trace, "{}", match fill.rule() {
                            usvg::FillRule::NonZero => 1,
                            usvg::FillRule::EvenOdd => 0,
                        }),
                        None => trace.write_str("null"),
                    }.map_err(|_| Failure::Contract)?;
                    write!(trace, ",\"paintOrder\":\"{}\",\"commandsBits\":", match path.paint_order() {
                        usvg::PaintOrder::FillAndStroke => "fill-stroke",
                        usvg::PaintOrder::StrokeAndFill => "stroke-fill",
                    }).map_err(|_| Failure::Contract)?;
                    trace.path_bits(path.data()).map_err(|_| Failure::Contract)?;
                    trace.write_str("}}\n").map_err(|_| Failure::Contract)
                })
            })
        }
    }
    fn request_resources(&self) -> Option<ResourcesRef> {
        Some(self.resources.clone())
    }

    fn reserve_replay_bytes(
        &self,
        bytes: usize,
    ) -> core::result::Result<Arc<dyn usvg::ReplayRetention>, ()> {
        let mut output = None;
        self.observe(|state| {
            output = Some(state.retained.retain(bytes)?);
            Ok(())
        })?;
        output.ok_or_else(|| {
            self.callbacks.record_failure(Failure::Contract);
        })
    }
    fn unsupported(&self, _reason: &'static str) -> core::result::Result<(), ()> {
        self.observe(|_| {
            self.callbacks
                .record_resource(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Unsupported,
                    markdown_svg_budget::LimitKind::None,
                ));
            Err(self.failure().unwrap_or(Failure::Contract))
        })
    }

    fn observe_metrics(
        &self,
        value: usvg::ReplayMetricsObservation<'_>,
    ) -> core::result::Result<(), ()> {
        self.observe(|state| {
            let (face, orientation) = state.fonts.descriptor(value.font)?;
            state.observation.metrics(face, orientation, value)?;
            Ok(())
        })
    }
    fn observe_layout_metrics(
        &self,
        value: usvg::ReplayLayoutMetricsObservation,
    ) -> core::result::Result<(), ()> {
        self.observe(|state| {
            let (face, orientation) = state.fonts.descriptor(value.font)?;
            state.observation.layout_metrics(face, orientation, value)?;
            Ok(())
        })
    }
    fn observe_outline(
        &self,
        value: usvg::ReplayOutlineObservation<'_>,
    ) -> core::result::Result<(), ()> {
        self.observe(|state| {
            let (face, orientation) = state.fonts.descriptor(value.font)?;
            state.observation.outline(face, orientation, value)?;
            Ok(())
        })
    }
    fn observe_geometry(
        &self,
        value: usvg::ReplayGeometryObservation<'_>,
    ) -> core::result::Result<(), ()> {
        self.observe(|state| {
            let (face, orientation) = state.fonts.descriptor(value.font)?;
            // This was a semantic guard in the former serialization helper.
            for transform in [
                value.glyph_transform,
                value.cluster_transform,
                value.span_transform,
                value.representation_transform,
                value.scene_transform,
                value
                    .scene_transform
                    .pre_concat(value.representation_transform),
            ] {
                if [
                    transform.sx,
                    transform.ky,
                    transform.kx,
                    transform.sy,
                    transform.tx,
                    transform.ty,
                ]
                .iter()
                .any(|v| !v.is_finite())
                {
                    return Err(self.callbacks.record_failure(Failure::Contract));
                }
            }
            state.observation.geometry(face, orientation, value)?;
            Ok(())
        })
    }
    fn prepare(
        &self,
        text: &usvg::Text,
        database: &mut Arc<fontdb::Database>,
    ) -> core::result::Result<usvg::ReplayText, ()> {
        let mut state = self.state.lock().map_err(|_| {
            self.callbacks.record_failure(Failure::Contract);
        })?;
        if self.failure().is_some() {
            return Err(());
        }
        match self.prepare_inner(&mut state, text, database) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.callbacks.record_failure(error);
                Err(())
            }
        }
    }
    fn variations(&self, font: fontdb::ID) -> core::result::Result<Arc<[usvg::FontVariation]>, ()> {
        let state = self.state.lock().map_err(|_| {
            self.callbacks.record_failure(Failure::Contract);
        })?;
        if self.failure().is_some() {
            return Err(());
        }
        state.fonts.variations(font).map_err(|error| {
            self.callbacks.record_failure(error);
        })
    }
    fn fontdb(&self) -> core::result::Result<Arc<fontdb::Database>, ()> {
        let state = self.state.lock().map_err(|_| {
            self.callbacks.record_failure(Failure::Contract);
        })?;
        if self.failure().is_some() {
            return Err(());
        }
        Ok(state.database.clone())
    }
    fn outline(
        &self,
        font: fontdb::ID,
        glyph: u32,
    ) -> core::result::Result<usvg::ReplayOutline, ()> {
        let state = self.state.lock().map_err(|_| {
            self.callbacks.record_failure(Failure::Contract);
        })?;
        if self.failure().is_some() {
            return Err(());
        }
        let (face, orientation) = state.fonts.descriptor(font).map_err(|error| {
            self.callbacks.record_failure(error);
        })?;
        if face.simulations == 0 {
            return Ok(usvg::ReplayOutline::Ordinary);
        }
        let key = abi::GlyphKey {
            face_ticket: face.ticket,
            glyph_id: glyph,
            orientation,
            reserved: 0,
        };
        #[cfg(test)]
        let outline_phase = crate::search_work::phase::Scope::new(
            crate::search_work::phase::Stage::OutlineLookup,
            &self.resources,
        );
        let mut search = SearchWork::new(&self.resources).map_err(|error| {
            self.callbacks.record_resource(error);
        })?;
        let path = search
            .find(
                state.geometry.iter().flat_map(|batch| batch.paths.iter()),
                |path| path.key == key,
            )
            .map_err(|error| {
                self.callbacks.record_resource(error);
            })?
            .ok_or_else(|| {
                self.callbacks.record_failure(Failure::Contract);
            })?;
        search.finish().map_err(|error| {
            self.callbacks.record_resource(error);
        })?;
        #[cfg(test)]
        outline_phase.finish();
        Ok(usvg::ReplayOutline::Synthetic {
            data: path.path.clone(),
            rule: if path.fill_rule == 0 {
                usvg::FillRule::EvenOdd
            } else {
                usvg::FillRule::NonZero
            },
        })
    }
}
