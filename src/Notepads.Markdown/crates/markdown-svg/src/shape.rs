// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real HB shaping of exact actual file/face/axes instances. Native nominal tables
//! are FontFuncs input only; GSUB/GPOS final advances and offsets stay untouched.
use crate::memory::failure_from_resource;
use crate::{
    abi::{self, Failure, Result},
    draft::DraftOwner,
    font::{FontStore, NativeMetrics, OwnedPlan},
};
use crate::{memory::Context, search_work::SearchWork};
use markdown_svg_budget::{
    BudgetVec, Error as ResourceError, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};
#[cfg(feature = "quality-observation")]
use std::fmt::Write;
use std::sync::Arc;

pub struct MetricEntry {
    ticket: u32,
    orientation: u32,
    owner: Arc<NativeMetrics>,
}
pub struct MetricsStore {
    entries: [Option<MetricEntry>; 64],
}
impl MetricsStore {
    pub fn new() -> Self {
        Self {
            entries: core::array::from_fn(|_| None),
        }
    }
    pub fn has_ink(&self, key: abi::GlyphKey) -> Result<bool> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.ticket == key.face_ticket && entry.orientation == key.orientation)
            .ok_or(Failure::Contract)?
            .owner
            .has_ink(key.glyph_id)
    }
    fn get(
        &mut self,
        callbacks: &Context,
        retained: &Arc<crate::memory::RetainedPool>,
        face: &abi::Face,
        orientation: u32,
    ) -> Result<Option<Arc<NativeMetrics>>> {
        if face.simulations == 0 {
            return Ok(None);
        }
        if let Some(value) = self
            .entries
            .iter()
            .flatten()
            .find(|entry| entry.ticket == face.ticket && entry.orientation == orientation)
        {
            return Ok(Some(value.owner.clone()));
        }
        let owner = retained.share(NativeMetrics::fill(callbacks, face, orientation)?)?;
        *self
            .entries
            .iter_mut()
            .find(|entry| entry.is_none())
            .ok_or_else(|| callbacks.record_failure(Failure::Contract))? = Some(MetricEntry {
            ticket: face.ticket,
            orientation,
            owner: owner.clone(),
        });
        Ok(Some(owner))
    }
}
struct OwnedBidiOrder {
    values: BudgetVec<usize>,
}
impl OwnedBidiOrder {
    fn new(resources: &ResourcesRef, levels: &[unicode_bidi::Level]) -> Result<Self> {
        #[cfg(test)]
        let bidi_stage =
            crate::draft::bidi_phase::begin(crate::draft::bidi_phase::Kind::Reorder, resources);
        let values = unicode_bidi::controlled::try_reorder_visual(levels, resources)
            .map_err(failure_from_resource)?;
        #[cfg(test)]
        bidi_stage.complete(values.len());
        Ok(Self { values })
    }
}
struct RunShape {
    native: abi::LogicalRun,
    start: usize,
    end: usize,
    level: unicode_bidi::Level,
    glyphs: BudgetVec<usvg::ReplayGlyph>,
}
#[cfg(test)]
pub(crate) mod font_phase {
    use markdown_svg_budget::ResourcesRef;
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum Stage {
        EnterRun,
        BeforeOwnersDrop,
    }
    type Hook = fn(Stage, &ResourcesRef);
    std::thread_local! {
        static HOOK: std::cell::Cell<Option<Hook>> = const { std::cell::Cell::new(None) };
    }
    pub(crate) fn install(hook: Option<Hook>) -> Option<Hook> {
        HOOK.with(|h| h.replace(hook))
    }
    pub(crate) fn observe(stage: Stage, r: &ResourcesRef) {
        HOOK.with(|h| {
            if let Some(hook) = h.get() {
                hook(stage, r);
            }
        });
    }
}
fn ink_exempt(text: &str) -> bool {
    text.chars().all(|ch|ch.is_whitespace() || matches!(ch,'\u{200C}'|'\u{200D}'|'\u{2060}'|'\u{FE0E}'|'\u{FE0F}'|'\u{202A}'..='\u{202E}'|'\u{2066}'..='\u{2069}'))
}
/// One real raw HB coverage component must have a provable visual owner. This
/// gate never guesses per-glyph ownership from GID/order/count or disables rlig.
pub(crate) fn visual_owner_for_range(
    resources: &ResourcesRef,
    text: &usvg::Text,
    domain: usize,
    base: usize,
    range: std::ops::Range<usize>,
) -> Result<usize> {
    resources.checkpoint(0).map_err(failure_from_resource)?;
    let semantic = text.chunks().get(domain).ok_or(Failure::Contract)?;
    let start = range.start.checked_sub(base).ok_or(Failure::Contract)?;
    let end = range.end.checked_sub(base).ok_or(Failure::Contract)?;
    if start >= end || end > semantic.text().len() {
        return Err(Failure::Contract);
    }
    let mut visual = 0;
    let mut count = semantic.spans().len();
    while count != 0 {
        let half = count / 2;
        let middle = visual + half;
        resources.checkpoint(1).map_err(failure_from_resource)?;
        if semantic.spans()[middle].end() <= start {
            visual = middle + 1;
            count -= half + 1;
        } else {
            count = half;
        }
    }
    let source = semantic.spans().get(visual).ok_or(Failure::Contract)?;
    for other in &semantic.spans()[visual + 1..] {
        resources.checkpoint(1).map_err(failure_from_resource)?;
        if other.start() >= end {
            break;
        }
        if !usvg::try_replay_visual_equivalent(source, other, resources)
            .map_err(failure_from_resource)?
        {
            return Err(crate::grapheme::unsupported());
        }
    }
    resources.checkpoint(0).map_err(failure_from_resource)?;
    Ok(visual)
}

fn validate_raw_glyph(
    text: &str,
    range: std::ops::Range<usize>,
    glyph: u32,
    glyph_count: u32,
    y_advance: i32,
) -> core::result::Result<(), ResourceError> {
    if range.start >= range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
        || glyph >= glyph_count
    {
        return Err(ResourceError::new(FailureKind::Invariant, LimitKind::None));
    }
    if y_advance != 0 {
        return Err(ResourceError::new(
            FailureKind::Unsupported,
            LimitKind::None,
        ));
    }
    if glyph == 0 && !ink_exempt(&text[range]) {
        return Err(ResourceError::new(FailureKind::FontReplay, LimitKind::None));
    }
    Ok(())
}

pub(crate) fn shape(
    callbacks: &Context,
    resources: &ResourcesRef,
    retained: &Arc<crate::memory::RetainedPool>,
    draft: &DraftOwner,
    plan: &OwnedPlan,
    fonts: &FontStore,
    metrics: &mut MetricsStore,
    observation: &crate::observation::ObservationSink,
    text: &usvg::Text,
) -> Result<usvg::ReplayText> {
    #[cfg(not(feature = "quality-observation"))]
    let _ = observation;
    let mut search = SearchWork::new(resources).map_err(failure_from_resource)?;
    let mut shaped = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    shaped
        .try_reserve_exact(plan.runs.len())
        .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
    let (shaping_runs, shaping_count) = plan.shaping_runs(callbacks, draft)?;
    for run in &shaping_runs.as_slice()[..shaping_count] {
        resources.checkpoint(0).map_err(failure_from_resource)?;
        #[cfg(test)]
        font_phase::observe(font_phase::Stage::EnterRun, resources);
        callbacks.checkpoint(1)?;
        {
            let start = run.utf8_start as usize;
            let end = start + run.utf8_length as usize;
            let id = fonts
                .id(run.face_ticket, run.orientation)
                .map_err(|error| callbacks.record_failure(error))?;
            let (descriptor, _) = fonts
                .descriptor(id)
                .map_err(|error| callbacks.record_failure(error))?;
            let (bytes, index) = fonts
                .bytes(id)
                .map_err(|error| callbacks.record_failure(error))?;
            let mut font_control = markdown_svg_budget::RequestControl(resources);
            let font = harfrust::FontRef::try_from_index(
                bytes.as_ref().as_ref(),
                index,
                &mut font_control,
            )
            .map_err(failure_from_resource)?
            .map_err(|_| callbacks.record_failure(Failure::Contract))?;
            let checked_font = read_fonts::request::CheckedFontRef::from_index(
                bytes.as_ref().as_ref(),
                index,
                resources,
            )
            .map_err(failure_from_resource)?
            .map_err(|_| callbacks.record_failure(Failure::Contract))?;
            let axes = fonts
                .variations(id)
                .map_err(|error| callbacks.record_failure(error))?;
            let mut variations = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            variations
                .try_reserve_exact(axes.len())
                .map_err(failure_from_resource)?;
            for axis in axes.iter() {
                variations
                    .try_push(harfrust::Variation {
                        tag: harfrust::Tag::from_be_bytes(axis.tag),
                        value: axis.value,
                    })
                    .map_err(failure_from_resource)?;
            }
            let data = harfrust::request::Data::try_new(&font, resources)
                .map_err(failure_from_resource)?;
            let instance =
                harfrust::request::Instance::try_from_variations(&data, variations.iter())
                    .map_err(failure_from_resource)?;
            // These are the actual constructor argument and the same instance later
            // passed to the shaper, not a reconstruction from intended Native axes.
            use read_fonts::control::TryTableProvider;
            let head_upem = checked_font
                .try_head(&mut font_control)
                .map_err(failure_from_resource)?
                .map_err(|_| callbacks.record_failure(Failure::Contract))?
                .units_per_em();
            if head_upem == 0 {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let fvar_records = if checked_font
                .try_data_for_tag(read_fonts::types::Tag::new(b"fvar"), &mut font_control)
                .map_err(failure_from_resource)?
                .is_some()
            {
                let fvar = checked_font
                    .try_fvar(&mut font_control)
                    .map_err(failure_from_resource)?
                    .map_err(|_| callbacks.record_failure(Failure::Contract))?;
                let records = fvar
                    .try_axes(&mut font_control)
                    .map_err(failure_from_resource)?
                    .map_err(|_| callbacks.record_failure(Failure::Contract))?;
                if records.len() > 4096 {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                Some(records)
            } else {
                None
            };
            #[cfg(not(feature = "quality-observation"))]
            let _ = fvar_records;
            #[cfg(feature = "quality-observation")]
            if observation.is_enabled() {
                let (file, digest, owner) = fonts.observed_file_identity(descriptor.file_ticket)?;
                if owner != Arc::as_ptr(&bytes) as usize
                    || file.byte_length != bytes.as_ref().as_ref().len() as u64
                {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                observation.record(|trace| {
                write!(trace, "{{\"kind\":\"actualFontInstance\",\"draftId\":{},\"logicalUtf8\":[{start},{}],\"faceTicket\":{},\"fileTicket\":{},\"fontOwnerIdentity\":{owner},\"faceIndex\":{index},\"simulations\":{},\"orientation\":{},\"nativeUnitsPerEm\":{},\"headUnitsPerEm\":{head_upem},\"rustOwnerSha256\":\"", draft.id, end - start, descriptor.ticket, descriptor.file_ticket, descriptor.simulations, run.orientation, descriptor.units_per_em).map_err(|_| Failure::Contract)?;
                for byte in digest { write!(trace, "{byte:02x}").map_err(|_| Failure::Contract)?; }
                trace.write_str("\",\"nativeAxes\":[").map_err(|_| Failure::Contract)?;
                for (i, axis) in axes.iter().enumerate() {
                    if i != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"tagBE\":{},\"value\":{}}}", u32::from_be_bytes(axis.tag), axis.value).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"hbSetVariationsArgument\":[").map_err(|_| Failure::Contract)?;
                for (i, axis) in variations.iter().enumerate() {
                    if i != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"tagBE\":{},\"value\":{}}}", u32::from_be_bytes(axis.tag.to_be_bytes()), axis.value).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"hbActualNormalizedF2Dot14\":[").map_err(|_| Failure::Contract)?;
                for (i, coord) in instance.coords().iter().enumerate() {
                    if i != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{}", coord.to_bits()).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"fvarAxes\":[").map_err(|_| Failure::Contract)?;
                if let Some(records) = fvar_records {
                    for (i, axis) in records.iter().enumerate() {
                        if i != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                        write!(trace, "{{\"tagBE\":{},\"minimum\":{},\"default\":{},\"maximum\":{}}}", u32::from_be_bytes(axis.axis_tag().to_be_bytes()), axis.min_value().to_f64() as f32, axis.default_value().to_f64() as f32, axis.max_value().to_f64() as f32).map_err(|_| Failure::Contract)?;
                    }
                }
                writeln!(trace, "],\"hbDefaultLocationElided\":{},\"nativeCanonicalAxesAreNotFvarAxes\":true,\"normalizedSource\":\"same-ShaperInstance-coords\",\"actualFontRefOwnerVerified\":true}}", instance.coords().is_empty()).map_err(|_| Failure::Contract)
            })?;
                let m = descriptor.metrics;
                observation.record(|trace| {
                writeln!(trace, "{{\"kind\":\"actualNativeGlobalMetrics\",\"draftId\":{},\"faceTicket\":{},\"orientation\":{},\"unitsPerEm\":{},\"ascent\":{},\"descentPositive\":{},\"lineGap\":{},\"capHeight\":{},\"xHeight\":{},\"underlinePosition\":{},\"underlineThickness\":{},\"strikePosition\":{},\"strikeThickness\":{}}}", draft.id, descriptor.ticket, run.orientation, m.units_per_em, m.ascent, m.descent, m.line_gap, m.cap_height, m.x_height, m.underline_position, m.underline_thickness, m.strike_position, m.strike_thickness).map_err(|_| Failure::Contract)
            })?;
            }
            let mut buffer =
                harfrust::request::Buffer::try_new(resources).map_err(failure_from_resource)?;
            buffer
                .try_push_str(&draft.text[start..end])
                .map_err(failure_from_resource)?;
            buffer
                .try_set_pre_context(&draft.text[..start])
                .map_err(failure_from_resource)?;
            buffer
                .try_set_post_context(&draft.text[end..])
                .map_err(failure_from_resource)?;
            buffer
                .try_set_direction(if run.bidi_level & 1 == 0 {
                    harfrust::Direction::LeftToRight
                } else {
                    harfrust::Direction::RightToLeft
                })
                .map_err(failure_from_resource)?;
            buffer
                .try_set_script(
                    harfrust::Script::from_iso15924_tag(harfrust::Tag::from_be_bytes(
                        run.script_be.to_be_bytes(),
                    ))
                    .ok_or_else(|| callbacks.record_failure(Failure::Contract))?,
                )
                .map_err(failure_from_resource)?;
            buffer
                .try_set_language(Some(
                    plan.language(run)
                        .map_err(|error| callbacks.record_failure(error))?,
                ))
                .map_err(failure_from_resource)?;
            #[cfg(test)]
            let run_style_phase = crate::search_work::phase::Scope::new(
                crate::search_work::phase::Stage::RunStyle,
                resources,
            );
            let style = search
                .find(draft.styles.iter(), |style| {
                    start >= style.utf8_start as usize
                        && start < (style.utf8_start + style.utf8_length) as usize
                })
                .map_err(failure_from_resource)?
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            #[cfg(test)]
            run_style_phase.finish();
            #[cfg(test)]
            let source_span_phase = crate::search_work::phase::Scope::new(
                crate::search_work::phase::Stage::SourceSpan,
                resources,
            );
            let source_span = search
                .find(
                    text.chunks()
                        .iter()
                        .flat_map(|chunk| chunk.spans())
                        .zip(draft.styles.iter()),
                    |(_, entry)| entry.utf8_start == style.utf8_start,
                )
                .map_err(failure_from_resource)?
                .map(|(span, _)| span)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            #[cfg(test)]
            source_span_phase.finish();
            let mut features = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            if source_span.small_caps() {
                features
                    .try_push(harfrust::Feature::new(harfrust::Tag::new(b"smcp"), 1, ..))
                    .map_err(failure_from_resource)?;
            }
            if !source_span.apply_kerning() {
                features
                    .try_push(harfrust::Feature::new(harfrust::Tag::new(b"kern"), 0, ..))
                    .map_err(failure_from_resource)?;
            }
            if source_span.replay_needs_unligated_tracking(
                core::str::from_utf8(&run.script_be.to_be_bytes())
                    .map_err(|_| callbacks.record_failure(Failure::Contract))?,
            ) {
                for tag in [b"liga", b"clig", b"dlig", b"hlig", b"calt"] {
                    features
                        .try_push(harfrust::Feature::new(harfrust::Tag::new(tag), 0, ..))
                        .map_err(failure_from_resource)?;
                }
            }
            let nominal = metrics.get(callbacks, retained, &descriptor, run.orientation)?;
            let mut funcs = crate::nominal::ObservedFuncs::new(
                callbacks,
                descriptor.glyph_count,
                nominal.as_ref().map(|owner| owner.font_funcs()),
            )?;
            #[cfg(feature = "quality-observation")]
            if observation.is_enabled() {
                funcs.enable_observation(resources)?;
            }
            let options = harfrust::ShapeOptions::new()
                .features(&features)
                .request_font_funcs(Some(&mut funcs));
            let output = harfrust::request::try_shape(
                resources,
                &data,
                Some(&instance),
                buffer,
                options,
                None,
            )
            .map_err(failure_from_resource)?;
            if funcs.failed() {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let infos = output.glyph_infos();
            if infos.len() > 65535 {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let positions = output.glyph_positions();
            let mut boundaries = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            boundaries
                .try_reserve_exact(infos.len() + 1)
                .map_err(failure_from_resource)?;
            for info in infos {
                boundaries
                    .try_push(info.cluster as usize)
                    .map_err(failure_from_resource)?;
            }
            boundaries
                .try_push(end - start)
                .map_err(failure_from_resource)?;
            boundaries.sort_unstable();
            #[cfg(feature = "quality-observation")]
            if observation.is_enabled() {
                observation.record(|trace| {
                write!(trace, "{{\"kind\":\"actualGlyphRun\",\"draftId\":{},\"logicalUtf8\":[{start},{}],\"faceTicket\":{},\"faceIndex\":{index},\"simulations\":{},\"bidiLevel\":{},\"orientation\":{},\"fallbackScale\":{},\"scriptISO15924\":{},\"rawGlyphCount\":{},\"rawPositionCount\":{},\"language\":", draft.id, end - start, run.face_ticket, descriptor.simulations, run.bidi_level, run.orientation, run.fallback_scale, run.script_be, infos.len(), positions.len()).map_err(|_| Failure::Contract)?;
                trace.quoted(plan.language(run)?).map_err(|_| Failure::Contract)?;
                trace.write_str(",\"glyphs\":[").map_err(|_| Failure::Contract)?;
                for (offset, (info, pos)) in infos.iter().zip(positions).enumerate() {
                    if offset != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"id\":{},\"rawCluster\":{},\"clusterUtf8\":", info.glyph_id, info.cluster).map_err(|_| Failure::Contract)?;
                    let local = info.cluster as usize;
                    let next = boundaries.get(boundaries.partition_point(|&value| value <= local)).copied();
                    match next.filter(|&next| local < next && next <= end - start) {
                        Some(next) => write!(trace, "[{},{}]", start + local, next - local).map_err(|_| Failure::Contract)?,
                        None => trace.write_str("null").map_err(|_| Failure::Contract)?,
                    }
                    write!(trace, ",\"advance\":[{},{}],\"offset\":[{},{}],\"unsafeToBreak\":{}}}", pos.x_advance, pos.y_advance, pos.x_offset, pos.y_offset, info.unsafe_to_break()).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"realHarfRustGsubGpos\":true,\"beforeSourceValidationAndVisualReorder\":true,\"coalescedNativeCoveragePreserved\":true}\n").map_err(|_| Failure::Contract)
            })?;
                funcs.emit_used(
                    observation,
                    draft.id,
                    run.face_ticket,
                    run.orientation,
                    infos.iter().map(|info| info.glyph_id),
                )?;
            }
            let mut glyphs = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            glyphs
                .try_reserve_exact(infos.len())
                .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
            for (info, pos) in infos.iter().zip(positions) {
                let local = info.cluster as usize;
                let next = *boundaries
                    .get(boundaries.partition_point(|&value| value <= local))
                    .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
                if local >= next || next > end - start {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                validate_raw_glyph(
                    &draft.text,
                    start + local..start + next,
                    info.glyph_id,
                    descriptor.glyph_count,
                    pos.y_advance,
                )
                .map_err(|error| failure_from_resource(callbacks.record_resource(error)))?;
                let glyph = usvg::ReplayGlyph {
                    identity: None,
                    start: start + local,
                    end: start + next,
                    glyph: info.glyph_id,
                    face: fonts
                        .replay_face(id)
                        .map_err(|error| callbacks.record_failure(error))?,
                    geometry_origin: if descriptor.simulations != 0 && run.orientation == 1 {
                        usvg::ReplayGeometryOrigin::NativeSidewaysRunPenNormalized
                    } else {
                        usvg::ReplayGeometryOrigin::FontDesignOriginYUp
                    },
                    x_offset: pos.x_offset,
                    y_offset: pos.y_offset,
                    advance: pos.x_advance,
                    y_advance: pos.y_advance,
                    fallback_scale: run.fallback_scale,
                };
                glyphs.try_push(glyph).map_err(failure_from_resource)?;
            }
            shaped
                .try_push(RunShape {
                    native: *run,
                    start,
                    end,
                    level: unicode_bidi::Level::new(run.bidi_level as u8)
                        .map_err(|_| callbacks.record_failure(Failure::Contract))?,
                    glyphs,
                })
                .map_err(failure_from_resource)?;
            #[cfg(test)]
            font_phase::observe(font_phase::Stage::BeforeOwnersDrop, resources);
        }
        resources.checkpoint(0).map_err(failure_from_resource)?;
    }
    let ranges = shaped
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.start..glyph.end));
    let candidate = crate::logical::OwnedLogicalPlan::prepare_with_parts(
        callbacks,
        resources,
        draft,
        ranges,
        Some(plan),
    )?;
    let checked = candidate.check(draft, retained)?;
    for run in shaped.iter_mut() {
        let native = run.native;
        for glyph in run.glyphs.iter_mut() {
            let domain = draft
                .chunks
                .partition_point(|chunk| chunk.end <= glyph.start);
            let chunk = draft
                .chunks
                .get(domain)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            if glyph.end > chunk.end {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let semantic = &text.chunks()[domain];
            let visual = visual_owner_for_range(
                resources,
                text,
                domain,
                chunk.start,
                glyph.start..glyph.end,
            )
            .map_err(|error| crate::grapheme::record_semantic_failure(callbacks, error))?;
            let source_span = &semantic.spans()[visual];
            let source_context = &draft.contexts[draft
                .contexts
                .partition_point(|context| context.range.end <= glyph.start)];
            if source_context.parent
                != draft
                    .source_index
                    .parent(glyph.start)
                    .map_err(|error| callbacks.record_failure(error))?
                || source_context.resolved.orientation != native.orientation
                || source_context.resolved.script.short_name().as_bytes()
                    != native.script_be.to_be_bytes()
            {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let parts = checked.font_parts(glyph.start, glyph.end)?;
            glyph.identity = Some(usvg::ReplayGlyphIdentity {
                parent: checked.parent(glyph.start, glyph.end)?,
                part: parts.start,
                part_count: parts.len(),
                position_domain: domain,
                visual_span: visual,
                semantic_em: source_span.font_size().get(),
                native_orientation: native.orientation,
                script_be: native.script_be,
            });
        }
    }
    let mut chunks = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    let mut styles = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    let mut style_offset = 0;
    for (chunk_index, chunk) in draft.chunks.iter().enumerate() {
        let mut glyphs = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        let mut included = 0;
        // Paragraph order remains logical even in a base-RTL chunk; only runs
        // within each whole paragraph's chunk intersection are visually reordered.
        for paragraph in draft.paragraphs.iter() {
            search.charge().map_err(failure_from_resource)?;
            let start = chunk.start.max(paragraph.start);
            let end = chunk.end.min(paragraph.end);
            if start >= end {
                continue;
            }
            let mut relevant = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            #[cfg(test)]
            let relevant_phase = crate::search_work::phase::Scope::new(
                crate::search_work::phase::Stage::RelevantRuns,
                resources,
            );
            for run in shaped.iter() {
                search.charge().map_err(failure_from_resource)?;
                if run.start >= start && run.end <= end {
                    relevant.try_push(run).map_err(failure_from_resource)?;
                }
            }
            #[cfg(test)]
            relevant_phase.finish();
            let mut levels = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            for run in relevant.iter() {
                levels.try_push(run.level).map_err(failure_from_resource)?;
            }
            let order = OwnedBidiOrder::new(resources, &levels)?;
            for &index in order.values.iter() {
                included += 1;
                for glyph in relevant[index].glyphs.iter() {
                    let mut glyph = glyph.clone();
                    glyph.start -= chunk.start;
                    glyph.end -= chunk.start;
                    glyphs.try_push(glyph).map_err(failure_from_resource)?;
                }
            }
            drop(order);
            resources.checkpoint(0).map_err(failure_from_resource)?;
        }
        #[cfg(test)]
        crate::draft::bidi_phase::observe_shape_retired(resources);
        #[cfg(test)]
        let included_phase = crate::search_work::phase::Scope::new(
            crate::search_work::phase::Stage::IncludedRuns,
            resources,
        );
        let mut expected_runs = 0;
        for run in shaped.iter() {
            search.charge().map_err(failure_from_resource)?;
            if run.start >= chunk.start && run.end <= chunk.end {
                expected_runs += 1;
            }
        }
        #[cfg(test)]
        included_phase.finish();
        if included != expected_runs {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut style_faces = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        for span in text.chunks()[chunk_index].spans() {
            let start = chunk.start + span.start();
            #[cfg(test)]
            let style_face_phase = crate::search_work::phase::Scope::new(
                crate::search_work::phase::Stage::StyleFace,
                resources,
            );
            let run = search
                .find(plan.runs.as_slice().iter(), |run| {
                    start >= run.utf8_start as usize
                        && start < (run.utf8_start + run.utf8_length) as usize
                })
                .map_err(failure_from_resource)?
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            #[cfg(test)]
            style_face_phase.finish();
            style_faces
                .try_push(
                    fonts
                        .replay_face(
                            fonts
                                .id(run.face_ticket, run.orientation)
                                .map_err(|error| callbacks.record_failure(error))?,
                        )
                        .map_err(|error| callbacks.record_failure(error))?,
                )
                .map_err(failure_from_resource)?;
            style_offset += 1;
        }
        chunks
            .try_push(glyphs.into())
            .map_err(failure_from_resource)?;
        styles
            .try_push(style_faces.into())
            .map_err(failure_from_resource)?;
    }
    if style_offset != draft.styles.len() {
        return Err(callbacks.record_failure(Failure::Contract));
    }
    search.finish().map_err(failure_from_resource)?;
    Ok(usvg::ReplayText {
        logical: Some(checked),
        chunks: chunks.into(),
        styles: styles.into(),
    })
}

#[cfg(test)]
mod n04_glyph_gate_tests {
    use super::*;
    #[test]
    fn suppression_and_empty_representation_never_relax_gid0_or_raw_source_guard() {
        assert!(harfrust::notepads_markdown_probe::shaping_suppression_candidate('\u{200b}'));
        assert_eq!(
            validate_raw_glyph("\u{200b}", 0..3, 0, 100, 0),
            Err(ResourceError::new(FailureKind::FontReplay, LimitKind::None))
        );
        assert_eq!(
            validate_raw_glyph("A", 0..1, 0, 100, 0),
            Err(ResourceError::new(FailureKind::FontReplay, LimitKind::None))
        );
        assert_eq!(
            validate_raw_glyph("A", 0..1, 100, 100, 0),
            Err(ResourceError::new(FailureKind::Invariant, LimitKind::None))
        );
        assert_eq!(
            validate_raw_glyph("A\u{301}", 0..2, 3, 100, 0),
            Err(ResourceError::new(FailureKind::Invariant, LimitKind::None))
        );
        assert!(validate_raw_glyph("A\u{301}", 0..3, 3, 100, 0).is_ok());
        assert_eq!(
            validate_raw_glyph("A", 0..1, 3, 100, 1),
            Err(ResourceError::new(
                FailureKind::Unsupported,
                LimitKind::None
            ))
        );
        assert!(
            validate_raw_glyph("\u{200d}", 0..3, 0, 100, 0).is_ok(),
            "original narrow ink_exempt is preserved, never expanded"
        );
    }
}
