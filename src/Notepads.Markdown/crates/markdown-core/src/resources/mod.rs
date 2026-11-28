// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One control's image admission policy. Source replacement retires table rows,
//! never outstanding work or byte charges. Native owners must free their actual
//! payload before dropping the last matching reservation.
//!
//! The fixed table and at most 1024 reservation control blocks are bounded policy
//! storage, separate from model-root allocation funding. Byte gauges describe
//! reserved native capacities, not decoder internals, GPU memory or process RSS.

use crate::identity::SourceKey;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

pub mod derived;
pub mod svg_size;
pub mod svg_work;
mod variant;
mod windowed;
pub use variant::{
    ImageTarget, ImageVariantMetadata, RASTER_MAXIMUM_SIDE, RasterPlan, SvgImageMetadata,
};
use windowed::{CapacityFailure, ProcessImageAuthority};
pub use windowed::{
    CapacityNotify, ProcessImageUsage, process_image_usage, set_process_image_capacity_notify,
};

pub const MAX_ENTRIES: usize = 256;
pub const MAX_ACTIVE_REQUESTS: u32 = 2;
pub const MAX_RESERVATIONS: u32 = 1024;
pub const MAX_ENCODED_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_ENCODED_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_DECODED_BYTES: u64 = 256 * 1024 * 1024;
// Process capacity is independent of one control's live/current-source owners.
// Keep these in sync with the native image pressure contract.
pub const MAX_PROCESS_ENCODED_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_PROCESS_DECODED_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 8192;
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceLimit {
    Entries,
    ActiveRequests,
    Reservations,
    EncodedBytes,
    EncodedImageBytes,
    DecodedBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceError {
    InvalidEpoch,
    StaleResult,
    UnknownObject,
    InvalidState,
    InvalidSize,
    InvalidMetadata,
    WrongReservation,
    NotAdmitted,
    OutOfMemory,
    ArithmeticOverflow,
    SnapshotTooSmall {
        required: usize,
    },
    Limit {
        kind: ResourceLimit,
        requested: u64,
        limit: u64,
    },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ResourceError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceEpoch {
    pub source: SourceKey,
    pub source_generation: u64,
    pub provider_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestKey {
    pub epoch: SourceEpoch,
    pub object: u32,
    pub request: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImageState {
    #[default]
    Unrequested,
    Pending,
    Ready,
    Unavailable,
    Failed,
    Limited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    Unavailable,
    Cancelled,
    Provider,
    Decode,
    Unsupported,
    Limited,
    Released,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageFailure {
    pub kind: FailureKind,
    /// Platform status for diagnostics, not an authority or parsed UI message.
    pub code: i32,
}

// Existing private SVG failure transport: Decode, with no resource limit.
// The FFI behavior test binds this value to the actual SVG budget enums.
const SVG_DECODE_WITHOUT_LIMIT_DETAIL: u32 = 5 << 16;

/// Intrinsic dimensions precede EXIF orientation. Decoded dimensions describe
/// the already-oriented, PBGRA8 payload; stride includes actual row padding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageMetadata {
    pub intrinsic_width: u32,
    pub intrinsic_height: u32,
    pub decoded_width: u32,
    pub decoded_height: u32,
    pub orientation: u32,
    pub stride: u32,
}

impl ImageMetadata {
    pub fn required_capacity(self) -> Result<u64, ResourceError> {
        if !(1..=8).contains(&self.orientation)
            || self.intrinsic_width == 0
            || self.intrinsic_height == 0
            || self.decoded_width == 0
            || self.decoded_height == 0
            || self.intrinsic_width > MAX_IMAGE_DIMENSION
            || self.intrinsic_height > MAX_IMAGE_DIMENSION
            || self.decoded_width > MAX_IMAGE_DIMENSION
            || self.decoded_height > MAX_IMAGE_DIMENSION
        {
            return Err(ResourceError::InvalidMetadata);
        }
        let pixels = u64::from(self.intrinsic_width)
            .checked_mul(u64::from(self.intrinsic_height))
            .ok_or(ResourceError::ArithmeticOverflow)?;
        let decoded = u64::from(self.decoded_width)
            .checked_mul(u64::from(self.decoded_height))
            .ok_or(ResourceError::ArithmeticOverflow)?;
        let row = self
            .decoded_width
            .checked_mul(4)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        let (width, height) = if self.orientation >= 5 {
            (self.intrinsic_height, self.intrinsic_width)
        } else {
            (self.intrinsic_width, self.intrinsic_height)
        };
        if pixels > MAX_IMAGE_PIXELS
            || decoded > MAX_IMAGE_PIXELS
            || self.stride < row
            || self.decoded_width > width
            || self.decoded_height > height
        {
            return Err(ResourceError::InvalidMetadata);
        }
        u64::from(self.stride)
            .checked_mul(u64::from(self.decoded_height))
            .ok_or(ResourceError::ArithmeticOverflow)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntrySnapshot {
    pub object: u32,
    pub state: ImageState,
    pub request: Option<RequestKey>,
    pub metadata: Option<ImageVariantMetadata>,
    pub failure: Option<ImageFailure>,
    pub desired_target: Option<ImageTarget>,
    pub published_target: Option<ImageTarget>,
    pub pending_request: Option<RequestKey>,
    pub failure_detail: u32,
    pub last_attempt_generation: u64,
    /// Computed at snapshot time from the physical process generation and fit.
    pub retry_eligible: bool,
    pub capacity_wait: bool,
}

impl EntrySnapshot {
    pub fn target_satisfied(self) -> bool {
        if self.state != ImageState::Ready {
            return false;
        }
        match (self.metadata, self.published_target, self.desired_target) {
            (Some(ImageVariantMetadata::Svg(metadata)), Some(published), Some(desired)) => {
                published.font_generation == desired.font_generation && metadata.satisfies(desired)
            }
            (Some(ImageVariantMetadata::Raster(metadata)), Some(_), Some(desired)) => {
                metadata.raster_satisfies(desired)
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceUsage {
    pub active_requests: u32,
    pub reservations: u32,
    pub encoded_bytes: u64,
    pub decoded_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotHeader {
    pub epoch: Option<SourceEpoch>,
    pub state_generation: u64,
    pub written: usize,
    pub usage: ResourceUsage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteKind {
    Encoded,
    Decoded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmissionKind {
    Raster,
    Svg,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KnownKind {
    Raster,
    Svg,
}

struct Candidate {
    kind: AdmissionKind,
    key: RequestKey,
    target: ImageTarget,
}

struct Entry {
    known_kind: Option<KnownKind>,
    snapshot: EntrySnapshot,
    candidate: Option<Candidate>,
    ready_reservation: u64,
    capacity_failure: Option<CapacityFailure>,
    live_requests: u32,
    live_reservations: u32,
}

struct State {
    epoch: Option<SourceEpoch>,
    entries: Vec<Option<Entry>>,
    count: usize,
    generation: u64,
    generation_exhausted: bool,
    last_request: u64,
    last_reservation: u64,
    usage: ResourceUsage,
    svg_work_live: u64,
}

impl State {
    fn next_generation(&self) -> Result<u64, ResourceError> {
        if self.generation_exhausted {
            return Err(ResourceError::ArithmeticOverflow);
        }
        self.generation
            .checked_add(1)
            .ok_or(ResourceError::ArithmeticOverflow)
    }

    fn changed_on_drop(&mut self) {
        match self.generation.checked_add(1) {
            Some(next) if !self.generation_exhausted => self.generation = next,
            _ => self.generation_exhausted = true,
        }
    }

    fn find(&self, object: u32) -> Option<usize> {
        self.entries[..self.count].iter().position(|entry| {
            entry
                .as_ref()
                .is_some_and(|row| row.snapshot.object == object)
        })
    }

    fn pending(&self, key: RequestKey) -> Result<usize, ResourceError> {
        if self.epoch != Some(key.epoch) {
            return Err(ResourceError::StaleResult);
        }
        let index = self.find(key.object).ok_or(ResourceError::UnknownObject)?;
        let row = self.entries[index].as_ref().expect("occupied prefix");
        let Some(candidate) = &row.candidate else {
            return Err(if row.snapshot.request == Some(key) {
                ResourceError::InvalidState
            } else {
                ResourceError::StaleResult
            });
        };
        if candidate.key != key || Some(candidate.target) != row.snapshot.desired_target {
            return Err(ResourceError::StaleResult);
        }
        Ok(index)
    }
}

struct Shared {
    state: Mutex<State>,
    process: Arc<ProcessImageAuthority>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // No client code runs under this lock. Recovery also lets RAII owners
        // return their charges if an unrelated internal panic poisoned it.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Clones refer to the same control ledger; construct once per control.
#[derive(Clone)]
pub struct ImageResourcePolicy {
    shared: Arc<Shared>,
}

impl ImageResourcePolicy {
    /// The large table allocation is fallible. Arc's small control allocation
    /// retains the core's existing global allocator/OOM boundary.
    pub fn try_new() -> Result<Self, ResourceError> {
        Self::try_new_inner(ProcessImageAuthority::shared())
    }

    fn try_new_inner(process: Arc<ProcessImageAuthority>) -> Result<Self, ResourceError> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(MAX_ENTRIES)
            .map_err(|_| ResourceError::OutOfMemory)?;
        entries.resize_with(MAX_ENTRIES, || None);
        Ok(Self {
            shared: Arc::new(Shared {
                process,
                state: Mutex::new(State {
                    epoch: None,
                    entries,
                    count: 0,
                    generation: 0,
                    generation_exhausted: false,
                    last_request: 0,
                    last_reservation: 0,
                    usage: ResourceUsage::default(),
                    svg_work_live: 0,
                }),
            }),
        })
    }

    #[cfg(test)]
    fn new() -> Self {
        Self::try_new_inner(Arc::new(ProcessImageAuthority {
            state: Mutex::new(ProcessImageUsage::default()),
        }))
        .expect("test resource policy allocation")
    }

    pub const fn table_request_bytes() -> usize {
        MAX_ENTRIES * std::mem::size_of::<Option<Entry>>()
    }

    /// Actual Vec capacity, for the integrator's separate model-root funding.
    /// No operation grows this table after try_new.
    pub fn table_capacity_bytes(&self) -> usize {
        self.shared.lock().entries.capacity() * std::mem::size_of::<Option<Entry>>()
    }

    /// Strict source advance, or same source/key with provider advance. Retires
    /// current rows only; old permits and byte owners keep their shared charges.
    pub fn begin_source(&self, epoch: SourceEpoch) -> Result<(), ResourceError> {
        let mut state = self.shared.lock();
        if epoch.source_generation == 0 || epoch.provider_generation == 0 {
            return Err(ResourceError::InvalidEpoch);
        }
        if let Some(old) = state.epoch
            && (epoch.provider_generation < old.provider_generation
                || epoch.source_generation < old.source_generation
                || (epoch.source_generation == old.source_generation
                    && (epoch.source != old.source
                        || epoch.provider_generation <= old.provider_generation)))
        {
            return Err(ResourceError::InvalidEpoch);
        }
        let next = state.next_generation()?;
        state.entries.fill_with(|| None);
        state.count = 0;
        state.epoch = Some(epoch);
        state.generation = next;
        Ok(())
    }

    /// Idempotent registration never restarts a Pending/Ready/failed entry.
    pub fn register(&self, object: u32) -> Result<EntrySnapshot, ResourceError> {
        let mut state = self.shared.lock();
        if state.epoch.is_none() {
            return Err(ResourceError::InvalidState);
        }
        if let Some(index) = state.find(object) {
            return Ok(state.entries[index]
                .as_ref()
                .expect("occupied prefix")
                .snapshot);
        }
        if state.count == MAX_ENTRIES {
            return Err(limit(
                ResourceLimit::Entries,
                MAX_ENTRIES as u64 + 1,
                MAX_ENTRIES as u64,
            ));
        }
        let next = state.next_generation()?;
        let snapshot = EntrySnapshot {
            object,
            ..EntrySnapshot::default()
        };
        let index = state.count;
        state.entries[index] = Some(Entry {
            known_kind: None,
            snapshot,
            candidate: None,
            ready_reservation: 0,
            capacity_failure: None,
            live_requests: 0,
            live_reservations: 0,
        });
        state.count += 1;
        state.generation = next;
        Ok(snapshot)
    }

    pub fn start(&self, object: u32) -> Result<RequestPermit, ResourceError> {
        self.start_inner(object, AdmissionKind::Raster)
    }

    pub fn start_variant(&self, object: u32) -> Result<RequestPermit, ResourceError> {
        self.start_inner(object, AdmissionKind::Svg)
    }

    pub fn start_unknown(&self, object: u32) -> Result<RequestPermit, ResourceError> {
        self.start_inner(object, AdmissionKind::Auto)
    }

    fn start_inner(
        &self,
        object: u32,
        kind: AdmissionKind,
    ) -> Result<RequestPermit, ResourceError> {
        let mut state = self.shared.lock();
        let epoch = state.epoch.ok_or(ResourceError::InvalidState)?;
        let index = state.find(object).ok_or(ResourceError::UnknownObject)?;
        let entry = state.entries[index].as_ref().expect("occupied prefix");
        let target = entry
            .snapshot
            .desired_target
            .ok_or(ResourceError::InvalidState)?;
        if (kind == AdmissionKind::Auto && entry.known_kind.is_some())
            || (kind == AdmissionKind::Svg && entry.known_kind == Some(KnownKind::Raster))
            || (kind == AdmissionKind::Raster && entry.known_kind == Some(KnownKind::Svg))
            || entry.candidate.is_some()
            || (entry.snapshot.last_attempt_generation >= target.generation
                && !self.shared.retry_eligible(&state, entry))
            || entry.snapshot.target_satisfied()
        {
            return Err(ResourceError::InvalidState);
        }
        let active = state
            .usage
            .active_requests
            .checked_add(1)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        if active > MAX_ACTIVE_REQUESTS {
            return Err(limit(
                ResourceLimit::ActiveRequests,
                u64::from(active),
                u64::from(MAX_ACTIVE_REQUESTS),
            ));
        }
        let request = state
            .last_request
            .checked_add(1)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        let generation = state.next_generation()?;
        let key = RequestKey {
            epoch,
            object,
            request,
        };
        let entry = state.entries[index].as_mut().expect("occupied prefix");
        if entry.ready_reservation == 0 {
            entry.snapshot.state = ImageState::Pending;
            entry.snapshot.request = Some(key);
        }
        entry.snapshot.pending_request = Some(key);
        entry.snapshot.failure = None;
        entry.snapshot.failure_detail = 0;
        entry.capacity_failure = None;
        entry.live_requests += 1;
        entry.candidate = Some(Candidate { kind, key, target });
        entry.snapshot.last_attempt_generation = target.generation;
        state.last_request = request;
        state.usage.active_requests = active;
        state.generation = generation;
        Ok(RequestPermit {
            shared: self.shared.clone(),
            key,
            target,
            svg_account: AtomicU8::new(0),
        })
    }

    /// Publication does not terminate OpenAsync or release its active permit.
    pub fn complete_ready(
        &self,
        key: RequestKey,
        metadata: ImageMetadata,
        decoded: &ByteReservation,
    ) -> Result<(), ResourceError> {
        self.complete_variant_ready(key, metadata.into(), decoded)
    }

    pub fn complete_variant_ready(
        &self,
        key: RequestKey,
        metadata: ImageVariantMetadata,
        decoded: &ByteReservation,
    ) -> Result<(), ResourceError> {
        self.complete_variant_ready_inner(key, metadata, decoded, false)
    }

    /// Bind independently retained immutable pixels to a fresh current request.
    /// The native caller must verify their exact current content/render identity;
    /// this route transfers no former request or publication authority.
    pub fn complete_reused_variant(
        &self,
        key: RequestKey,
        metadata: ImageVariantMetadata,
        decoded: &ByteReservation,
    ) -> Result<(), ResourceError> {
        self.complete_variant_ready_inner(key, metadata, decoded, true)
    }

    fn complete_variant_ready_inner(
        &self,
        key: RequestKey,
        metadata: ImageVariantMetadata,
        decoded: &ByteReservation,
        reused: bool,
    ) -> Result<(), ResourceError> {
        let mut state = self.shared.lock();
        let index = state.pending(key)?;
        if !Arc::ptr_eq(&self.shared, &decoded.owner.shared)
            || (!reused && decoded.owner.key != key)
            || decoded.owner.kind != ByteKind::Decoded
        {
            return Err(ResourceError::WrongReservation);
        }
        if !decoded.owner.admitted.load(Ordering::Acquire) {
            return Err(ResourceError::NotAdmitted);
        }
        let candidate = state.entries[index]
            .as_ref()
            .expect("occupied prefix")
            .candidate
            .as_ref()
            .expect("pending candidate");
        let target = candidate.target;
        let known_kind = match (metadata, candidate.kind) {
            (ImageVariantMetadata::Raster(_), AdmissionKind::Raster | AdmissionKind::Auto) => {
                KnownKind::Raster
            }
            (ImageVariantMetadata::Svg(_), AdmissionKind::Svg | AdmissionKind::Auto) => {
                KnownKind::Svg
            }
            _ => return Err(ResourceError::InvalidMetadata),
        };
        if metadata.required_capacity(target)? > decoded.owner.bytes.load(Ordering::Acquire) {
            return Err(ResourceError::InvalidMetadata);
        }
        let generation = state.next_generation()?;
        let row = state.entries[index].as_mut().expect("occupied prefix");
        row.known_kind = Some(known_kind);
        row.snapshot.state = ImageState::Ready;
        row.snapshot.request = Some(key);
        row.snapshot.metadata = Some(metadata);
        row.snapshot.published_target = Some(target);
        row.snapshot.pending_request = None;
        row.snapshot.failure = None;
        row.snapshot.failure_detail = 0;
        row.candidate = None;
        row.capacity_failure = None;
        row.ready_reservation = decoded.owner.id;
        state.generation = generation;
        Ok(())
    }

    pub fn complete_failure(
        &self,
        key: RequestKey,
        failure: ImageFailure,
    ) -> Result<(), ResourceError> {
        self.complete_variant_failure(key, failure, 0)
    }

    pub fn complete_variant_failure(
        &self,
        key: RequestKey,
        failure: ImageFailure,
        detail: u32,
    ) -> Result<(), ResourceError> {
        let mut state = self.shared.lock();
        let index = state.pending(key)?;
        let generation = state.next_generation()?;
        let entry = state.entries[index].as_mut().expect("occupied prefix");
        entry.candidate = None;
        entry.snapshot.pending_request = None;
        if entry.ready_reservation == 0 {
            entry.snapshot.state = match failure.kind {
                FailureKind::Unavailable => ImageState::Unavailable,
                FailureKind::Limited => ImageState::Limited,
                _ => ImageState::Failed,
            };
        }
        entry.snapshot.failure = Some(failure);
        entry.snapshot.failure_detail = detail;
        if failure.kind != FailureKind::Limited {
            entry.capacity_failure = None;
        }
        state.generation = generation;
        Ok(())
    }

    /// Only the desired value is coalesced. Superseded permits retain their
    /// physical slot and charges until their actual owners are destroyed.
    pub fn desire_variant(&self, object: u32, target: ImageTarget) -> Result<(), ResourceError> {
        target.validate()?;
        let mut state = self.shared.lock();
        if state.epoch.is_none() {
            return Err(ResourceError::InvalidState);
        }
        let index = state.find(object);
        if let Some(index) = index {
            let row = state.entries[index].as_ref().expect("occupied prefix");
            if let Some(old) = row.snapshot.desired_target {
                if old == target {
                    return Ok(());
                }
                if target.generation <= old.generation
                    || target.font_generation < old.font_generation
                {
                    return Err(ResourceError::InvalidEpoch);
                }
                // Geometry cannot repair a missing/unsupported immutable source
                // or a deterministic SVG decode failure under the same fonts.
                // Keep both desired and last-attempt unchanged; advancing just
                // desired would incorrectly advertise a new render to the host.
                if matches!(
                    row.snapshot.state,
                    ImageState::Unavailable | ImageState::Failed
                ) && row.snapshot.metadata.is_none()
                    && row.snapshot.pending_request.is_none()
                    && row.candidate.is_none()
                    && row.snapshot.last_attempt_generation == old.generation
                    && row
                        .snapshot
                        .failure
                        .is_some_and(|failure| match failure.kind {
                            FailureKind::Unavailable => true,
                            FailureKind::Unsupported => row.snapshot.failure_detail == 0,
                            FailureKind::Decode => {
                                row.snapshot.failure_detail == SVG_DECODE_WITHOUT_LIMIT_DETAIL
                                    && old.font_generation == target.font_generation
                            }
                            _ => false,
                        })
                {
                    return Ok(());
                }
            }
        } else if state.count == MAX_ENTRIES {
            return Err(limit(
                ResourceLimit::Entries,
                MAX_ENTRIES as u64 + 1,
                MAX_ENTRIES as u64,
            ));
        }
        let generation = state.next_generation()?;
        let index = index.unwrap_or(state.count);
        if state.entries[index].is_none() {
            state.entries[index] = Some(Entry {
                known_kind: None,
                snapshot: EntrySnapshot {
                    object,
                    ..EntrySnapshot::default()
                },
                candidate: None,
                ready_reservation: 0,
                capacity_failure: None,
                live_requests: 0,
                live_reservations: 0,
            });
            state.count += 1;
        }
        let entry = state.entries[index].as_mut().expect("occupied prefix");
        let font_changed = entry.known_kind != Some(KnownKind::Raster)
            && entry
                .snapshot
                .published_target
                .is_some_and(|old| old.font_generation != target.font_generation);
        entry.candidate = None;
        entry.snapshot.pending_request = None;
        if font_changed {
            entry.snapshot.metadata = None;
            entry.snapshot.published_target = None;
            entry.ready_reservation = 0;
        }
        if entry.ready_reservation == 0 {
            entry.snapshot.request = None;
            entry.snapshot.state = ImageState::Unrequested;
        }
        entry.snapshot.desired_target = Some(target);
        entry.capacity_failure = None;
        state.generation = generation;
        Ok(())
    }

    /// Read the authoritative generation without copying image entries.
    pub fn state_generation(&self) -> Result<u64, ResourceError> {
        let state = self.shared.lock();
        if state.generation_exhausted {
            return Err(ResourceError::ArithmeticOverflow);
        }
        Ok(state.generation)
    }

    /// All-or-nothing, allocation-free batch in stable registration order.
    pub fn snapshot(&self, output: &mut [EntrySnapshot]) -> Result<SnapshotHeader, ResourceError> {
        let state = self.shared.lock();
        if state.generation_exhausted {
            return Err(ResourceError::ArithmeticOverflow);
        }
        if output.len() < state.count {
            return Err(ResourceError::SnapshotTooSmall {
                required: state.count,
            });
        }
        for (target, entry) in output.iter_mut().zip(&state.entries[..state.count]) {
            let entry = entry.as_ref().expect("occupied prefix");
            *target = entry.snapshot;
            target.retry_eligible = self.shared.retry_eligible(&state, entry);
            target.capacity_wait = entry.capacity_failure.is_some()
                && entry
                    .snapshot
                    .failure
                    .is_some_and(|failure| failure.kind == FailureKind::Limited);
        }
        Ok(SnapshotHeader {
            epoch: state.epoch,
            state_generation: state.generation,
            written: state.count,
            usage: state.usage,
        })
    }

    pub fn usage(&self) -> ResourceUsage {
        self.shared.lock().usage
    }
}

fn limit(kind: ResourceLimit, requested: u64, maximum: u64) -> ResourceError {
    ResourceError::Limit {
        kind,
        requested,
        limit: maximum,
    }
}

/// Keep alive until the actual provider/read/decode operation has terminated,
/// even after cancellation/source replacement. Dropping is the terminal ack.
#[must_use]
pub struct RequestPermit {
    shared: Arc<Shared>,
    key: RequestKey,
    target: ImageTarget,
    svg_account: AtomicU8,
}

impl fmt::Debug for RequestPermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestPermit")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl RequestPermit {
    pub fn key(&self) -> RequestKey {
        self.key
    }

    pub fn target(&self) -> ImageTarget {
        self.target
    }

    pub fn reserve_encoded(&self, bytes: u64) -> Result<ByteReservation, ResourceError> {
        self.reserve(ByteKind::Encoded, bytes)
    }

    pub fn reserve_decoded(&self, bytes: u64) -> Result<ByteReservation, ResourceError> {
        self.reserve(ByteKind::Decoded, bytes)
    }

    fn reserve(&self, kind: ByteKind, bytes: u64) -> Result<ByteReservation, ResourceError> {
        let mut state = self.shared.lock();
        let index = state.pending(self.key)?;
        if bytes == 0 {
            return Err(ResourceError::InvalidSize);
        }
        if kind == ByteKind::Encoded && bytes > MAX_ENCODED_IMAGE_BYTES {
            return Err(limit(
                ResourceLimit::EncodedImageBytes,
                bytes,
                MAX_ENCODED_IMAGE_BYTES,
            ));
        }
        let count = state
            .usage
            .reservations
            .checked_add(1)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        if count > MAX_RESERVATIONS {
            return Err(limit(
                ResourceLimit::Reservations,
                u64::from(count),
                u64::from(MAX_RESERVATIONS),
            ));
        }
        let current = match kind {
            ByteKind::Encoded => state.usage.encoded_bytes,
            ByteKind::Decoded => state.usage.decoded_bytes,
        };
        let total = current
            .checked_add(bytes)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        let mut process = self.shared.process.lock();
        let global_total = match kind {
            ByteKind::Encoded => process.encoded_bytes.checked_add(bytes),
            ByteKind::Decoded => process.decoded_bytes.checked_add(bytes),
        }
        .ok_or(ResourceError::ArithmeticOverflow)?;
        let capacity_error = check_bytes(kind, total)
            .err()
            .or_else(|| check_process_bytes(kind, global_total).err());
        if let Some(error) = capacity_error {
            // Only a genuine byte-capacity denial is retryable. Oversized
            // individual inputs, OOM and metadata validation never set this.
            if check_bytes(kind, bytes).is_ok() {
                state.entries[index]
                    .as_mut()
                    .expect("occupied prefix")
                    .capacity_failure = Some(CapacityFailure {
                    kind,
                    bytes,
                    generation: process.generation,
                });
            }
            return Err(error);
        }
        let id = state
            .last_reservation
            .checked_add(1)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        let owner = Arc::new(BytesOwner {
            shared: self.shared.clone(),
            key: self.key,
            id,
            kind,
            bytes: AtomicU64::new(bytes),
            admitted: AtomicBool::new(false),
        });
        match kind {
            ByteKind::Encoded => state.usage.encoded_bytes = total,
            ByteKind::Decoded => state.usage.decoded_bytes = total,
        }
        match kind {
            ByteKind::Encoded => process.encoded_bytes = global_total,
            ByteKind::Decoded => process.decoded_bytes = global_total,
        }
        state.entries[index]
            .as_mut()
            .expect("occupied prefix")
            .live_reservations += 1;
        state.last_reservation = id;
        state.usage.reservations = count;
        Ok(ByteReservation { owner })
    }
}

impl Drop for RequestPermit {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.usage.active_requests = state
            .usage
            .active_requests
            .checked_sub(1)
            .expect("live request charge");
        if state.epoch == Some(self.key.epoch)
            && let Some(index) = state.find(self.key.object)
        {
            let entry = state.entries[index].as_mut().expect("occupied prefix");
            entry.live_requests = entry
                .live_requests
                .checked_sub(1)
                .expect("live row request");
            if entry
                .candidate
                .as_ref()
                .is_some_and(|candidate| candidate.key == self.key)
            {
                entry.candidate = None;
                entry.snapshot.pending_request = None;
                if entry.ready_reservation == 0 {
                    entry.snapshot.state = ImageState::Failed;
                }
                entry.snapshot.failure = Some(ImageFailure {
                    kind: FailureKind::Cancelled,
                    code: 0,
                });
                entry.snapshot.failure_detail = 0;
                entry.capacity_failure = None;
                state.changed_on_drop();
            }
        }
    }
}

fn check_bytes(kind: ByteKind, bytes: u64) -> Result<(), ResourceError> {
    check_byte_limits(kind, bytes, MAX_ENCODED_BYTES, MAX_DECODED_BYTES)
}

fn check_process_bytes(kind: ByteKind, bytes: u64) -> Result<(), ResourceError> {
    check_byte_limits(
        kind,
        bytes,
        MAX_PROCESS_ENCODED_BYTES,
        MAX_PROCESS_DECODED_BYTES,
    )
}

fn check_byte_limits(
    kind: ByteKind,
    bytes: u64,
    encoded: u64,
    decoded: u64,
) -> Result<(), ResourceError> {
    let (which, maximum) = match kind {
        ByteKind::Encoded => (ResourceLimit::EncodedBytes, encoded),
        ByteKind::Decoded => (ResourceLimit::DecodedBytes, decoded),
    };
    if bytes > maximum {
        Err(limit(which, bytes, maximum))
    } else {
        Ok(())
    }
}

struct BytesOwner {
    shared: Arc<Shared>,
    key: RequestKey,
    id: u64,
    kind: ByteKind,
    bytes: AtomicU64,
    admitted: AtomicBool,
}

/// Clones share one reservation. Last Drop returns the charge on any thread;
/// free the native payload first, including on failed admit_actual.
#[derive(Clone)]
#[must_use]
pub struct ByteReservation {
    owner: Arc<BytesOwner>,
}

impl fmt::Debug for ByteReservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ByteReservation")
            .field("key", &self.owner.key)
            .field("kind", &self.owner.kind)
            .field("reserved_bytes", &self.reserved_bytes())
            .finish_non_exhaustive()
    }
}

impl ByteReservation {
    pub fn key(&self) -> RequestKey {
        self.owner.key
    }

    pub fn kind(&self) -> ByteKind {
        self.owner.kind
    }

    pub fn reserved_bytes(&self) -> u64 {
        self.owner.bytes.load(Ordering::Acquire)
    }

    pub fn actual_bytes(&self) -> Option<u64> {
        self.owner
            .admitted
            .load(Ordering::Acquire)
            .then(|| self.reserved_bytes())
    }

    pub fn is_same_request(&self, request: &RequestPermit) -> bool {
        Arc::ptr_eq(&self.owner.shared, &request.shared) && self.owner.key == request.key
    }

    /// Reconciles one already-reserved allocation, exactly once. A stale epoch
    /// can reconcile its existing allocation but cannot reserve another one or
    /// publish. Failure leaves the original charge and all state unchanged.
    pub fn admit_actual(&self, actual: u64) -> Result<(), ResourceError> {
        let mut state = self.owner.shared.lock();
        if self.owner.admitted.load(Ordering::Acquire) {
            return Err(ResourceError::InvalidState);
        }
        if actual == 0 {
            return Err(ResourceError::InvalidSize);
        }
        if self.owner.kind == ByteKind::Encoded && actual > MAX_ENCODED_IMAGE_BYTES {
            return Err(limit(
                ResourceLimit::EncodedImageBytes,
                actual,
                MAX_ENCODED_IMAGE_BYTES,
            ));
        }
        let previous = self.owner.bytes.load(Ordering::Acquire);
        let current = match self.owner.kind {
            ByteKind::Encoded => state.usage.encoded_bytes,
            ByteKind::Decoded => state.usage.decoded_bytes,
        };
        let total = current
            .checked_sub(previous)
            .and_then(|remaining| remaining.checked_add(actual))
            .ok_or(ResourceError::ArithmeticOverflow)?;
        check_bytes(self.owner.kind, total)?;
        let mut process = self.owner.shared.process.lock();
        let current = match self.owner.kind {
            ByteKind::Encoded => process.encoded_bytes,
            ByteKind::Decoded => process.decoded_bytes,
        };
        let global_total = current
            .checked_sub(previous)
            .and_then(|value| value.checked_add(actual))
            .ok_or(ResourceError::ArithmeticOverflow)?;
        check_process_bytes(self.owner.kind, global_total)?;
        match self.owner.kind {
            ByteKind::Encoded => process.encoded_bytes = global_total,
            ByteKind::Decoded => process.decoded_bytes = global_total,
        }
        match self.owner.kind {
            ByteKind::Encoded => state.usage.encoded_bytes = total,
            ByteKind::Decoded => state.usage.decoded_bytes = total,
        }
        self.owner.bytes.store(actual, Ordering::Release);
        self.owner.admitted.store(true, Ordering::Release);
        Ok(())
    }
}

impl Drop for BytesOwner {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        let amount = self.bytes.load(Ordering::Acquire);
        let mut process = self.shared.process.lock();
        match self.kind {
            ByteKind::Encoded => {
                process.encoded_bytes = process
                    .encoded_bytes
                    .checked_sub(amount)
                    .expect("live process encoded charge")
            }
            ByteKind::Decoded => {
                process.decoded_bytes = process
                    .decoded_bytes
                    .checked_sub(amount)
                    .expect("live process decoded charge")
            }
        }
        // Saturation closes same-target implicit retries rather than
        // wrapping and mistaking an ancient denial for a fresh one.
        process.generation = process.generation.saturating_add(1);
        let capacity_generation = process.generation;
        match self.kind {
            ByteKind::Encoded => {
                state.usage.encoded_bytes = state
                    .usage
                    .encoded_bytes
                    .checked_sub(amount)
                    .expect("live encoded charge")
            }
            ByteKind::Decoded => {
                state.usage.decoded_bytes = state
                    .usage
                    .decoded_bytes
                    .checked_sub(amount)
                    .expect("live decoded charge")
            }
        }
        state.usage.reservations = state
            .usage
            .reservations
            .checked_sub(1)
            .expect("live byte reservation");
        if state.epoch == Some(self.key.epoch)
            && let Some(index) = state.find(self.key.object)
        {
            let row = state.entries[index].as_mut().expect("occupied prefix");
            row.live_reservations = row
                .live_reservations
                .checked_sub(1)
                .expect("live row reservation");
        }
        // One physical allocation may back several freshly keyed objects. Its
        // birth epoch only owns the reservation count, never the current rows.
        let mut changed = false;
        let ready_count = if self.kind == ByteKind::Decoded {
            state.count
        } else {
            0
        };
        for row in state.entries[..ready_count].iter_mut().flatten() {
            if row.snapshot.state == ImageState::Ready && row.ready_reservation == self.id {
                if let Some(candidate) = &row.candidate {
                    row.snapshot.state = ImageState::Pending;
                    row.snapshot.request = Some(candidate.key);
                    row.snapshot.last_attempt_generation = candidate.target.generation;
                } else {
                    row.snapshot.state = ImageState::Unrequested;
                    row.snapshot.request = None;
                    row.snapshot.last_attempt_generation = 0;
                    row.snapshot.failure_detail = 0;
                    row.capacity_failure = None;
                }
                row.snapshot.published_target = None;
                row.snapshot.failure = Some(ImageFailure {
                    kind: FailureKind::Released,
                    code: 0,
                });
                row.ready_reservation = 0;
                changed = true;
            }
        }
        if changed {
            state.changed_on_drop();
        }
        drop(process);
        drop(state);
        self.shared.process.notify_capacity(capacity_generation);
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod variant_tests;
