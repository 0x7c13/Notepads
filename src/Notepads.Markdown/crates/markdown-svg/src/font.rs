// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Bounded logical-plan copies and one immutable Binary owner per actual file.
use crate::memory::Context;
use crate::{
    abi::{self, Failure, Result},
    memory::{AdmittedSlice, FontBytes},
};
use markdown_svg_budget::{Error as ResourceError, FailureKind, LimitKind};
use std::sync::Arc;

fn font_parse_failure(context: &Context) -> Failure {
    // The callback contract was validated separately. Unreadable actual font
    // content is unavailable font data; retain that typed cause before staged
    // owners leave scope and before the compatibility HRESULT projection.
    crate::memory::failure_from_resource(
        context.record_resource(ResourceError::new(FailureKind::FontReplay, LimitKind::None)),
    )
}

pub struct OwnedPlan {
    pub generation: u64,
    pub files: AdmittedSlice<abi::File>,
    pub faces: AdmittedSlice<abi::Face>,
    pub axes: AdmittedSlice<abi::Axis>,
    pub runs: AdmittedSlice<abi::LogicalRun>,
    pub strings: AdmittedSlice<u8>,
}
impl OwnedPlan {
    pub fn prepare(callbacks: &Context, draft: &abi::Draft, text: &str) -> Result<Self> {
        callbacks.checkpoint(1)?;
        let mut view = abi::Plan {
            size: size_of::<abi::Plan>() as u32,
            version: abi::ABI_VERSION,
            generation: 0,
            files: core::ptr::null(),
            file_count: 0,
            faces: core::ptr::null(),
            face_count: 0,
            axes: core::ptr::null(),
            axis_count: 0,
            runs: core::ptr::null(),
            run_count: 0,
            strings: core::ptr::null(),
            strings_length: 0,
        };
        // SAFETY: caller owns the entire bounded draft and output struct for this
        // synchronous callback. Native returns only metadata borrowed by ABI2 epoch.
        callbacks.native_success(unsafe {
            callbacks.plan.unwrap()(callbacks.session, draft, &mut view)
        })?;
        if view.size as usize != size_of::<abi::Plan>()
            || view.version != abi::ABI_VERSION
            || view.generation == 0
            || view.file_count > 16
            || view.face_count > 32
            || view.axis_count > 4096
            || view.run_count > 65535
            || view.strings_length > 1048576
        {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        // SAFETY: metadata counts are bounded above; Native guarantees readable
        // frozen views until the next plan ENTRY. Reserve/copy/checkpoint does not
        // expire views. Each destination is admitted and independently owned.
        let plan = unsafe {
            Self {
                generation: view.generation,
                files: AdmittedSlice::copy_native(callbacks, view.files, view.file_count as usize)?,
                faces: AdmittedSlice::copy_native(callbacks, view.faces, view.face_count as usize)?,
                axes: AdmittedSlice::copy_native(callbacks, view.axes, view.axis_count as usize)?,
                runs: AdmittedSlice::copy_native(callbacks, view.runs, view.run_count as usize)?,
                strings: AdmittedSlice::copy_native(
                    callbacks,
                    view.strings,
                    view.strings_length as usize,
                )?,
            }
        };
        plan.validate(callbacks, text)?;
        Ok(plan)
    }
    pub fn validate_request(&self, draft: &crate::draft::DraftOwner) -> Result<()> {
        let mut style = 0;
        let mut span = 0;
        for run in self.runs.as_slice() {
            let start = run.utf8_start as usize;
            let end = start + run.utf8_length as usize;
            while draft.request_styles[style].utf8_start + draft.request_styles[style].utf8_length
                <= run.utf8_start
            {
                style += 1;
            }
            while draft.spans[span].utf8_start + draft.spans[span].utf8_length <= run.utf8_start {
                span += 1;
            }
            let request = draft.request_styles[style];
            let logical = draft.spans[span];
            if end > (request.utf8_start + request.utf8_length) as usize
                || end > (logical.utf8_start + logical.utf8_length) as usize
                || run.script_be != logical.script_be
                || run.bidi_level != logical.bidi_level
                || run.orientation != logical.orientation
                || self.language(run)?.as_bytes()
                    != &draft.strings[logical.language_start as usize
                        ..(logical.language_start + logical.language_length) as usize]
            {
                return Err(Failure::Contract);
            }
        }
        Ok(())
    }
    pub fn shaping_runs(
        &self,
        callbacks: &Context,
        draft: &crate::draft::DraftOwner,
    ) -> Result<(AdmittedSlice<abi::LogicalRun>, usize)> {
        let mut output: AdmittedSlice<abi::LogicalRun> =
            AdmittedSlice::zeroed(callbacks, abi::SCRATCH, self.runs.len())?;
        let mut count = 0;
        let mut work = crate::memory::WorkBatch::new(callbacks);
        for run in self.runs.as_slice() {
            work.charge(1)?;
            let same = if count == 0 {
                false
            } else {
                let previous = &output.as_slice()[count - 1];
                let key = |at: u32| {
                    (
                        draft
                            .request_styles
                            .partition_point(|s| s.utf8_start + s.utf8_length <= at),
                        draft
                            .spans
                            .partition_point(|s| s.utf8_start + s.utf8_length <= at),
                        draft
                            .chunks
                            .partition_point(|chunk| chunk.end <= at as usize),
                    )
                };
                previous.utf8_start + previous.utf8_length == run.utf8_start
                    && previous.face_ticket == run.face_ticket
                    && previous.script_be == run.script_be
                    && previous.bidi_level == run.bidi_level
                    && previous.orientation == run.orientation
                    && previous.fallback_scale.to_bits() == run.fallback_scale.to_bits()
                    && self
                        .language(previous)
                        .map_err(|error| callbacks.record_failure(error))?
                        == self
                            .language(run)
                            .map_err(|error| callbacks.record_failure(error))?
                    && key(previous.utf8_start) == key(run.utf8_start)
            };
            if same {
                output.as_mut_slice()[count - 1].utf8_length += run.utf8_length;
            } else {
                output.as_mut_slice()[count] = *run;
                count += 1;
            }
        }
        Ok((output, count))
    }
    pub fn language(&self, run: &abi::LogicalRun) -> Result<&str> {
        let start = run.language_start as usize;
        let end = start
            .checked_add(run.language_length as usize)
            .ok_or(Failure::Contract)?;
        let bytes = self
            .strings
            .as_slice()
            .get(start..end)
            .ok_or(Failure::Contract)?;
        let value = std::str::from_utf8(bytes).map_err(|_| Failure::Contract)?;
        if value.is_empty() || value.contains('\0') || value.len() > 255 {
            return Err(Failure::Contract);
        }
        Ok(value)
    }
    fn validate(&self, callbacks: &Context, text: &str) -> Result<()> {
        for (i, file) in self.files.as_slice().iter().enumerate() {
            if file.ticket == 0
                || file.collection_faces == 0
                || !(12..=67108864).contains(&file.byte_length)
                || self.files.as_slice()[..i]
                    .iter()
                    .any(|previous| previous.ticket == file.ticket)
            {
                return Err(callbacks.record_failure(Failure::Contract));
            }
        }
        for (i, face) in self.faces.as_slice().iter().enumerate() {
            let file = self
                .files
                .as_slice()
                .iter()
                .find(|file| file.ticket == face.file_ticket)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            let end = face
                .axis_start
                .checked_add(face.axis_count)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?
                as usize;
            if face.ticket == 0
                || face.face_index >= file.collection_faces
                || face.simulations & !3 != 0
                || !(16..=16384).contains(&face.units_per_em)
                || face.metrics.units_per_em != face.units_per_em
                || !(1..=65535).contains(&face.glyph_count)
                || end > self.axes.len()
                || self.faces.as_slice()[..i]
                    .iter()
                    .any(|previous| previous.ticket == face.ticket)
            {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let axes = &self.axes.as_slice()[face.axis_start as usize..end];
            for (i, axis) in axes.iter().enumerate() {
                if !axis.value.is_finite()
                    || axes[..i]
                        .iter()
                        .any(|previous| previous.tag_be == axis.tag_be)
                {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
            }
        }
        let mut covered = 0;
        for run in self.runs.as_slice() {
            let start = run.utf8_start as usize;
            let end = start
                .checked_add(run.utf8_length as usize)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            if start != covered
                || start == end
                || !self
                    .faces
                    .as_slice()
                    .iter()
                    .any(|face| face.ticket == run.face_ticket)
                || run.bidi_level > 125
                || run.orientation > 1
                || !run.fallback_scale.is_finite()
                || run.fallback_scale <= 0.0
                || harfrust::Script::from_iso15924_tag(harfrust::Tag::from_be_bytes(
                    run.script_be.to_be_bytes(),
                ))
                .is_none()
            {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            self.language(run)
                .map_err(|error| callbacks.record_failure(error))?;
            covered = end;
        }
        if covered != text.len() {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct StoredFile {
    metadata: abi::File,
    owner: Arc<FontBytes>,
    // The owner is immutable. This diagnostic witness follows that exact Arc
    // through staging clones; a newly filled owner always starts unverified.
    #[cfg(feature = "quality-observation")]
    observed_sha256: Option<[u8; 32]>,
}
#[derive(Clone)]
struct StoredFace {
    ticket: u32,
    orientation: u32,
    id: fontdb::ID,
    metadata: abi::Face,
    axes: Arc<[usvg::FontVariation]>,
}
#[derive(Clone)]
pub struct FontStore {
    files: [Option<StoredFile>; 16],
    faces: [Option<StoredFace>; 64],
    resources: markdown_svg_budget::ResourcesRef,
}

/// The longest diagnostic name is `actual-face-4294967295-4294967295`.
/// Formatting stays on the stack; fontdb pays only for the final String.
#[derive(Clone, Copy)]
struct ActualFaceName {
    bytes: [u8; 33],
    len: usize,
}
impl ActualFaceName {
    const EMPTY: Self = Self {
        bytes: [0; 33],
        len: 0,
    };
    fn new(ticket: u32, orientation: u32) -> Self {
        let mut name = Self::EMPTY;
        name.bytes[..12].copy_from_slice(b"actual-face-");
        name.len = 12;
        name.number(ticket);
        name.bytes[name.len] = b'-';
        name.len += 1;
        name.number(orientation);
        name
    }
    fn number(&mut self, mut value: u32) {
        let mut digits = [0; 10];
        let mut count = 0;
        loop {
            digits[count] = b'0' + (value % 10) as u8;
            count += 1;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        for digit in digits[..count].iter().rev() {
            self.bytes[self.len] = *digit;
            self.len += 1;
        }
    }
    fn as_str(&self) -> &str {
        // Every byte above is copied ASCII or an ASCII decimal digit.
        core::str::from_utf8(&self.bytes[..self.len]).expect("ASCII face diagnostic")
    }
}
impl FontStore {
    pub fn new(resources: &markdown_svg_budget::ResourcesRef) -> Self {
        Self {
            files: core::array::from_fn(|_| None),
            faces: core::array::from_fn(|_| None),
            resources: resources.clone(),
        }
    }
    /// Stage a whole registration without mutating any published snapshot. The
    /// fixed arrays clone only Arc handles; no duplicate-text COW allocation is
    /// needed. A missing database result means its original Arc must be kept.
    pub fn stage_registration(
        &self,
        callbacks: &Context,
        retained: &Arc<crate::memory::RetainedPool>,
        plan: &OwnedPlan,
        db: &fontdb::Database,
    ) -> Result<(Self, Option<Arc<fontdb::Database>>)> {
        let resources = &self.resources;
        if let Some(error) = callbacks.failure() {
            return Err(error);
        }
        resources
            .checkpoint(1)
            .map_err(crate::memory::failure_from_resource)?;
        // Check every known file's identity before filling any new file, even
        // when all runs use faces which have already been registered.
        for file in plan.files.as_slice() {
            if let Some(stored) = self
                .files
                .iter()
                .flatten()
                .find(|stored| stored.metadata.ticket == file.ticket)
            {
                if stored.metadata.byte_length != file.byte_length
                    || stored.metadata.collection_faces != file.collection_faces
                    || stored.metadata.sha256 != file.sha256
                {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
            }
        }
        let mut staged = self.clone();
        for file in plan.files.as_slice() {
            resources
                .checkpoint(1)
                .map_err(crate::memory::failure_from_resource)?;
            if staged
                .files
                .iter()
                .flatten()
                .any(|stored| stored.metadata.ticket == file.ticket)
            {
                continue;
            }
            let slot = staged
                .files
                .iter_mut()
                .find(|slot| slot.is_none())
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            *slot = Some(StoredFile {
                metadata: *file,
                owner: retained.share(FontBytes::fill(callbacks, file)?)?,
                #[cfg(feature = "quality-observation")]
                observed_sha256: None,
            });
        }
        let mut pending_slots = [0; 64];
        let mut names = [ActualFaceName::EMPTY; 64];
        let mut additional = 0;
        for run in plan.runs.as_slice() {
            resources
                .checkpoint(1)
                .map_err(crate::memory::failure_from_resource)?;
            if staged
                .faces
                .iter()
                .flatten()
                .any(|face| face.ticket == run.face_ticket && face.orientation == run.orientation)
            {
                continue;
            }
            let face = plan
                .faces
                .as_slice()
                .iter()
                .find(|face| face.ticket == run.face_ticket)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            let slot = staged
                .faces
                .iter()
                .position(|slot| slot.is_none())
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            let file = staged
                .files
                .iter()
                .flatten()
                .find(|file| file.metadata.ticket == face.file_ticket)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            // Parse only the specified actual TTC face. Never auto-register every
            // face in a collection, never infer the index from a Native file id.
            let font = harfrust::FontRef::from_index(file.owner.as_ref().as_ref(), face.face_index)
                .map_err(|_| font_parse_failure(callbacks))?;
            use read_fonts::TableProvider;
            if font
                .maxp()
                .map_err(|_| font_parse_failure(callbacks))?
                .num_glyphs() as u32
                != face.glyph_count
                || font
                    .head()
                    .map_err(|_| font_parse_failure(callbacks))?
                    .units_per_em() as u32
                    != face.units_per_em
            {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            staged.faces[slot] = Some(StoredFace {
                ticket: face.ticket,
                orientation: run.orientation,
                id: fontdb::ID::dummy(),
                metadata: *face,
                axes: retained.variations(
                    &plan.axes.as_slice()
                        [face.axis_start as usize..(face.axis_start + face.axis_count) as usize],
                )?,
            });
            pending_slots[additional] = slot;
            names[additional] = ActualFaceName::new(face.ticket, run.orientation);
            additional += 1;
        }
        let database = if additional == 0 {
            None
        } else {
            let mut ids = [fontdb::ID::dummy(); 64];
            let database = db
                .try_extend_in(
                    resources,
                    additional,
                    |index| {
                        let face = staged.faces[pending_slots[index]]
                            .as_ref()
                            .expect("staged registration face");
                        let file = staged
                            .files
                            .iter()
                            .flatten()
                            .find(|file| file.metadata.ticket == face.metadata.file_ticket)
                            .expect("validated registration file");
                        Ok(fontdb::FaceRegistration {
                            source: file.owner.clone(),
                            index: face.metadata.face_index,
                            family: names[index].as_str(),
                        })
                    },
                    &mut ids[..additional],
                )
                .map_err(crate::memory::failure_from_resource)?;
            for (slot, id) in pending_slots[..additional].iter().zip(&ids) {
                staged.faces[*slot]
                    .as_mut()
                    .expect("staged registration face")
                    .id = *id;
            }
            Some(retained.share(database)?)
        };
        // No published state or output changes before the final shared-fault
        // and cancellation check. Caller commits with only infallible moves.
        resources
            .checkpoint(1)
            .map_err(crate::memory::failure_from_resource)?;
        Ok((staged, database))
    }
    pub fn variations(&self, id: fontdb::ID) -> Result<Arc<[usvg::FontVariation]>> {
        self.faces
            .iter()
            .flatten()
            .find(|face| face.id == id)
            .map(|face| face.axes.clone())
            .ok_or(Failure::Contract)
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn observe_files(
        &mut self,
        callbacks: &Context,
        observation: &crate::observation::ObservationSink,
        draft: u64,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::fmt::Write;
        if !observation.is_enabled() {
            return Ok(());
        }
        for file in self.files.iter_mut().flatten() {
            callbacks.checkpoint(1)?;
            let bytes: &[u8] = file.owner.as_ref().as_ref();
            if bytes.len() as u64 != file.metadata.byte_length {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let actual = if let Some(verified) = file.observed_sha256 {
                verified
            } else {
                let mut hash = Sha256::new();
                for chunk in bytes.chunks(65536) {
                    callbacks.checkpoint(chunk.len() as u64)?;
                    hash.update(chunk);
                }
                let mut actual = [0; 32];
                actual.copy_from_slice(&hash.finalize());
                if actual != file.metadata.sha256 {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                file.observed_sha256 = Some(actual);
                actual
            };
            let owner = Arc::as_ptr(&file.owner) as usize;
            observation.record(|trace| {
                write!(trace, "{{\"kind\":\"actualFontFile\",\"draftId\":{draft},\"ticket\":{},\"bytes\":{},\"collectionFaces\":{},\"fontOwnerIdentity\":{owner},\"sha256\":\"", file.metadata.ticket, bytes.len(), file.metadata.collection_faces).map_err(|_| Failure::Contract)?;
                for byte in actual {
                    write!(trace, "{byte:02x}").map_err(|_| Failure::Contract)?;
                }
                trace.write_str("\",\"nativeDeclaredSha256\":\"").map_err(|_| Failure::Contract)?;
                for byte in file.metadata.sha256 {
                    write!(trace, "{byte:02x}").map_err(|_| Failure::Contract)?;
                }
                trace.write_str("\",\"rustOwnerSha256Verified\":true,\"windowsBytesWrittenToDisk\":false}\n").map_err(|_| Failure::Contract)
            })?;
        }
        Ok(())
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn observed_file_identity(
        &self,
        ticket: u32,
    ) -> Result<(abi::File, [u8; 32], usize)> {
        let file = self
            .files
            .iter()
            .flatten()
            .find(|file| file.metadata.ticket == ticket)
            .ok_or(Failure::Contract)?;
        Ok((
            file.metadata,
            file.observed_sha256.ok_or(Failure::Contract)?,
            Arc::as_ptr(&file.owner) as usize,
        ))
    }
    pub fn descriptor(&self, id: fontdb::ID) -> Result<(abi::Face, u32)> {
        self.faces
            .iter()
            .flatten()
            .find(|face| face.id == id)
            .map(|face| (face.metadata, face.orientation))
            .ok_or(Failure::Contract)
    }
    pub fn bytes(&self, id: fontdb::ID) -> Result<(Arc<FontBytes>, u32)> {
        let (face, _) = self.descriptor(id)?;
        self.files
            .iter()
            .flatten()
            .find(|file| file.metadata.ticket == face.file_ticket)
            .map(|file| (file.owner.clone(), face.face_index))
            .ok_or(Failure::Contract)
    }
    pub fn replay_face(&self, id: fontdb::ID) -> Result<usvg::ReplayFace> {
        let (face, _) = self.descriptor(id)?;
        let value = face.metrics;
        Ok(usvg::ReplayFace {
            id,
            synthetic_metrics: (face.simulations != 0).then_some(usvg::ReplayMetrics {
                units_per_em: value.units_per_em,
                ascent: value.ascent,
                descent: value.descent.checked_neg().ok_or(Failure::Contract)?,
                x_height: value.x_height,
                underline_position: value.underline_position,
                underline_thickness: value.underline_thickness,
                strike_position: value.strike_position,
            }),
        })
    }
    pub fn id(&self, ticket: u32, orientation: u32) -> Result<fontdb::ID> {
        self.faces
            .iter()
            .flatten()
            .find(|face| face.ticket == ticket && face.orientation == orientation)
            .map(|face| face.id)
            .ok_or(Failure::Contract)
    }
}

pub struct NativeMetrics {
    rows: AdmittedSlice<abi::GlyphMetrics>,
    _lifetime: crate::memory::OwnerWitness,
}
impl NativeMetrics {
    pub fn fill(callbacks: &Context, face: &abi::Face, orientation: u32) -> Result<Self> {
        if face.simulations == 0 || orientation > 1 {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut rows = AdmittedSlice::zeroed(callbacks, abi::METRICS, face.glyph_count as usize)?;
        let mut written = 0;
        // SAFETY: one exact admitted final table, populated by coarse Native chunks.
        callbacks.native_success(unsafe {
            callbacks.fill_metrics.unwrap()(
                callbacks.session,
                face.ticket,
                orientation,
                rows.as_mut_ptr(),
                face.glyph_count,
                &mut written,
            )
        })?;
        if written != face.glyph_count {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        for row in rows.as_slice() {
            for value in [
                row.advance_width as i64,
                -(row.advance_height as i64),
                row.advance_width as i64 / 2,
                row.vertical_origin_y as i64 - row.top_side_bearing as i64,
                row.advance_width as i64
                    - row.left_side_bearing as i64
                    - row.right_side_bearing as i64,
                -(row.advance_height as i64
                    - row.top_side_bearing as i64
                    - row.bottom_side_bearing as i64),
            ] {
                i32::try_from(value).map_err(|_| callbacks.record_failure(Failure::Contract))?;
            }
        }
        Ok(Self {
            rows,
            _lifetime: crate::memory::OwnerWitness::new(
                callbacks,
                crate::memory::OwnerKind::Metrics,
            ),
        })
    }
    pub fn has_ink(&self, glyph: u32) -> Result<bool> {
        let row = self
            .rows
            .as_slice()
            .get(glyph as usize)
            .ok_or(Failure::Contract)?;
        let width =
            row.advance_width as i64 - row.left_side_bearing as i64 - row.right_side_bearing as i64;
        let height = row.advance_height as i64
            - row.top_side_bearing as i64
            - row.bottom_side_bearing as i64;
        Ok(width != 0 && height != 0)
    }
    pub fn font_funcs(&self) -> NativeFontFuncs<'_> {
        NativeFontFuncs {
            rows: self.rows.as_slice(),
            fault: false,
        }
    }
}
pub struct NativeFontFuncs<'a> {
    rows: &'a [abi::GlyphMetrics],
    pub fault: bool,
}
impl NativeFontFuncs<'_> {
    fn row(&mut self, gid: u32) -> abi::GlyphMetrics {
        if let Some(row) = self.rows.get(gid as usize) {
            *row
        } else {
            self.fault = true;
            abi::GlyphMetrics::default()
        }
    }
}
impl NativeFontFuncs<'_> {
    fn advance_width(&mut self, glyph: harfrust::GlyphId) -> i32 {
        self.row(glyph.to_u32()).advance_width as i32
    }
    fn advance_height(&mut self, glyph: harfrust::GlyphId) -> i32 {
        (-(self.row(glyph.to_u32()).advance_height as i64)) as i32
    }
    fn vertical_origin(&mut self, glyph: harfrust::GlyphId) -> (i32, i32) {
        let row = self.row(glyph.to_u32());
        ((row.advance_width / 2) as i32, row.vertical_origin_y)
    }
    fn extents(&mut self, glyph: harfrust::GlyphId) -> Option<harfrust::GlyphExtents> {
        let row = self.row(glyph.to_u32());
        Some(harfrust::GlyphExtents {
            x_bearing: row.left_side_bearing,
            y_bearing: (row.vertical_origin_y as i64 - row.top_side_bearing as i64) as i32,
            width: (row.advance_width as i64
                - row.left_side_bearing as i64
                - row.right_side_bearing as i64) as i32,
            height: (-(row.advance_height as i64
                - row.top_side_bearing as i64
                - row.bottom_side_bearing as i64)) as i32,
        })
    }
}

#[cfg(test)]
mod metric_tests {
    use super::*;

    #[test]
    fn native_extents_negate_height_before_narrowing() {
        let rows = [abi::GlyphMetrics {
            advance_height: 0x8000_0000,
            ..Default::default()
        }];
        let mut funcs = NativeFontFuncs {
            rows: &rows,
            fault: false,
        };
        let extents = funcs.extents(harfrust::GlyphId::new(0)).unwrap();
        assert_eq!(extents.height, i32::MIN);
        assert!(!funcs.fault);
    }
}

impl harfrust::font::FontFuncs for NativeFontFuncs<'_> {
    fn advance_width(
        &mut self,
        _: &harfrust::font::BuiltinFontFuncs,
        glyph: harfrust::GlyphId,
    ) -> i32 {
        self.advance_width(glyph)
    }
    fn advance_height(
        &mut self,
        _: &harfrust::font::BuiltinFontFuncs,
        glyph: harfrust::GlyphId,
    ) -> i32 {
        self.advance_height(glyph)
    }
    fn vertical_origin(
        &mut self,
        _: &harfrust::font::BuiltinFontFuncs,
        glyph: harfrust::GlyphId,
    ) -> (i32, i32) {
        self.vertical_origin(glyph)
    }
    fn extents(
        &mut self,
        _: &harfrust::font::BuiltinFontFuncs,
        glyph: harfrust::GlyphId,
    ) -> Option<harfrust::GlyphExtents> {
        self.extents(glyph)
    }
}

impl NativeFontFuncs<'_> {
    fn try_metric<T>(
        &mut self,
        resources: &markdown_svg_budget::ResourcesRef,
        glyph: harfrust::GlyphId,
        metric: impl FnOnce(&mut Self, harfrust::GlyphId) -> T,
    ) -> core::result::Result<T, markdown_svg_budget::Error> {
        resources.checkpoint(0)?;
        if glyph.to_u32() as usize >= self.rows.len() {
            self.fault = true;
            return Err(resources.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Invariant,
                markdown_svg_budget::LimitKind::None,
            )));
        }
        let value = metric(self, glyph);
        resources.checkpoint(0)?;
        Ok(value)
    }
}

impl harfrust::request::FontFuncs for NativeFontFuncs<'_> {
    fn advance_width(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: harfrust::GlyphId,
    ) -> core::result::Result<i32, markdown_svg_budget::Error> {
        self.try_metric(builtin.resources(), glyph, Self::advance_width)
    }
    fn advance_height(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: harfrust::GlyphId,
    ) -> core::result::Result<i32, markdown_svg_budget::Error> {
        self.try_metric(builtin.resources(), glyph, Self::advance_height)
    }
    fn vertical_origin(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: harfrust::GlyphId,
    ) -> core::result::Result<(i32, i32), markdown_svg_budget::Error> {
        self.try_metric(builtin.resources(), glyph, Self::vertical_origin)
    }
    fn extents(
        &mut self,
        builtin: &harfrust::request::BuiltinFontFuncs<'_, '_>,
        glyph: harfrust::GlyphId,
    ) -> core::result::Result<Option<harfrust::GlyphExtents>, markdown_svg_budget::Error> {
        self.try_metric(builtin.resources(), glyph, Self::extents)
    }
}
