// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Request-owned errors for an isolated prototype; no allocation/work-budget qualification.

use markdown_svg_budget::{Error as ResourceError, FailureKind, FirstFailureRef, ResourcesRef};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Fixed error reasons for the prototype's implemented resource-error paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ProcessingFailure {
    /// The configured resolver rejected an external resource.
    ExternalResource = 1,
    /// The configured resolver rejected an embedded type.
    UnsupportedImageData = 2,
    /// Image header dimensions could not be read.
    InvalidImageHeader = 3,
    /// A real selected raster codec failed.
    ImageDecode = 4,
    /// An embedded data URL was malformed.
    MalformedImageData = 5,
    /// Nested SVG conversion failed.
    NestedImageDecode = 6,
    /// A returned raster allocation failed; no OOM-recovery guarantee.
    RasterAllocation = 7,
    /// The filter's existing Result reported an invalid region.
    InvalidFilterRegion = 8,
    /// Isolated actual-font replay failed; never skip or substitute text.
    FontReplay = 9,
    /// The first underlying platform failure, preserving its HRESULT.
    Native = 10,
    /// A shared request resource limit was reached before allocation.
    ResourceQuota = 11,
    /// A fallible owned allocation failed.
    Allocation = 12,
    /// The current request was cancelled.
    Cancelled = 13,
    /// A controlled source could not be decoded.
    Decode = 14,
    /// A valid but unsupported source capability was requested.
    Unsupported = 15,
    /// A private ownership or checked-capacity invariant failed.
    Invariant = 16,
}

/// Copy-only failed-request receipt. It holds no tree, source, callback or context owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessingError {
    /// First fixed failure.
    pub kind: ProcessingFailure,
    /// The same indivisible first record used by Native, parser and resources.
    pub typed_failure: ResourceError,
    /// Conversion/render/late-conversion/late-render entry counts at return.
    pub counts: (usize, usize, usize, usize),
    /// Conversion/render entry counts when the first failure was recorded.
    pub counts_at_fault: (usize, usize),
    /// Bbox/paint/cache totals and after-fault totals, observed without suppressing work.
    pub mask_post: [usize; 6],
    /// Marker bbox total and after-fault total.
    pub marker_post: [usize; 2],
}

/// One shared owner per parse/render request, never process-global or reset for reuse.
#[derive(Debug)]
pub struct ProcessingContext {
    first: FirstFailureRef,
    resources: Option<ResourcesRef>,
    fault_counts_observed: AtomicBool,
    conversions: AtomicUsize,
    renders: AtomicUsize,
    late_conversions: AtomicUsize,
    late_renders: AtomicUsize,
    conversions_at_fault: AtomicUsize,
    renders_at_fault: AtomicUsize,
    mask_bbox: AtomicUsize,
    mask_paint: AtomicUsize,
    mask_cache: AtomicUsize,
    mask_bbox_after_fault: AtomicUsize,
    mask_paint_after_fault: AtomicUsize,
    mask_cache_after_fault: AtomicUsize,
    marker_bbox: AtomicUsize,
    marker_bbox_after_fault: AtomicUsize,
}

impl Default for ProcessingContext {
    fn default() -> Self {
        // Compatibility fixtures have no resource backend. The controlled entry
        // always supplies an externally prepaid shared request via with_resources.
        Self::with_parts(FirstFailureRef::new_prepaid(), None)
    }
}

impl ProcessingContext {
    fn with_parts(first: FirstFailureRef, resources: Option<ResourcesRef>) -> Self {
        Self {
            first,
            resources,
            fault_counts_observed: AtomicBool::new(false),
            conversions: AtomicUsize::new(0),
            renders: AtomicUsize::new(0),
            late_conversions: AtomicUsize::new(0),
            late_renders: AtomicUsize::new(0),
            conversions_at_fault: AtomicUsize::new(0),
            renders_at_fault: AtomicUsize::new(0),
            mask_bbox: AtomicUsize::new(0),
            mask_paint: AtomicUsize::new(0),
            mask_cache: AtomicUsize::new(0),
            mask_bbox_after_fault: AtomicUsize::new(0),
            mask_paint_after_fault: AtomicUsize::new(0),
            mask_cache_after_fault: AtomicUsize::new(0),
            marker_bbox: AtomicUsize::new(0),
            marker_bbox_after_fault: AtomicUsize::new(0),
        }
    }
    /// Bind the one root request before the first XML token. Nested documents
    /// clone this ProcessingContext; they never reset this failure or budget.
    pub fn with_resources(resources: ResourcesRef) -> Self {
        Self::with_parts(resources.fault_slot().clone(), Some(resources))
    }
    /// Borrow the same request capability; no second budget is created.
    pub fn resources(&self) -> Option<&ResourcesRef> {
        self.resources.as_ref()
    }
    /// Retrieve the original fixed record without erasing its Native HRESULT.
    pub fn resource_failure(&self) -> Option<ResourceError> {
        self.first.get()
    }
    /// Record the complete typed failure across renderer crates.
    pub fn record_resource(&self, error: ResourceError) -> ResourceError {
        let first = self.first.record(error);
        self.observe_fault_counts();
        first
    }
    fn observe_fault_counts(&self) {
        if self.first.get().is_some()
            && self
                .fault_counts_observed
                .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            self.conversions_at_fault
                .store(self.conversions.load(Ordering::Relaxed), Ordering::Relaxed);
            self.renders_at_fault
                .store(self.renders.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }
    /// Preserve the first complete typed error; later wrappers cannot replace it.
    pub fn fail(&self, failure: ProcessingFailure) {
        self.record_resource(ResourceError::new(
            failure.into(),
            markdown_svg_budget::LimitKind::None,
        ));
    }

    // Observation only: these methods never guard/skip/fail. The control build
    // must still execute the original post-fault work so the counters expose it.
    pub(crate) fn observe_mask_bbox(&self) {
        self.mask_bbox.fetch_add(1, Ordering::Relaxed);
        if self.failed() {
            self.mask_bbox_after_fault.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(crate) fn observe_mask_paint(&self) {
        self.mask_paint.fetch_add(1, Ordering::Relaxed);
        if self.failed() {
            self.mask_paint_after_fault.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(crate) fn observe_mask_cache(&self) {
        self.mask_cache.fetch_add(1, Ordering::Relaxed);
        if self.failed() {
            self.mask_cache_after_fault.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(crate) fn observe_marker_bbox(&self) {
        self.marker_bbox.fetch_add(1, Ordering::Relaxed);
        if self.failed() {
            self.marker_bbox_after_fault.fetch_add(1, Ordering::Relaxed);
        }
    }
    /// Named mask postprocessing totals, then corresponding after-fault totals.
    pub fn mask_post_counts(&self) -> [usize; 6] {
        [
            self.mask_bbox.load(Ordering::Relaxed),
            self.mask_paint.load(Ordering::Relaxed),
            self.mask_cache.load(Ordering::Relaxed),
            self.mask_bbox_after_fault.load(Ordering::Relaxed),
            self.mask_paint_after_fault.load(Ordering::Relaxed),
            self.mask_cache_after_fault.load(Ordering::Relaxed),
        ]
    }
    /// Marker bbox total and after-fault total.
    pub fn marker_post_counts(&self) -> [usize; 2] {
        [
            self.marker_bbox.load(Ordering::Relaxed),
            self.marker_bbox_after_fault.load(Ordering::Relaxed),
        ]
    }

    /// Retrieve the compatible fixed category of the authoritative first error.
    pub fn failure(&self) -> Option<ProcessingFailure> {
        self.resource_failure().map(|error| error.kind.into())
    }

    /// Whether all further request work must short-circuit.
    pub fn failed(&self) -> bool {
        let failed = self.first.get().is_some();
        if failed {
            self.observe_fault_counts();
        }
        failed
    }

    /// The checked outer request result.
    pub fn result(&self) -> Result<(), ProcessingFailure> {
        self.failure().map_or(Ok(()), Err)
    }

    /// A copy-only failure snapshot without retaining this owner.
    pub fn error(&self) -> Option<ProcessingError> {
        self.failure().map(|kind| ProcessingError {
            kind,
            typed_failure: self
                .first
                .get()
                .expect("failure was read from the same immutable first slot"),
            counts: self.counts(),
            counts_at_fault: self.counts_at_fault(),
            mask_post: self.mask_post_counts(),
            marker_post: self.marker_post_counts(),
        })
    }

    /// Checked result retaining the first fixed failure and diagnostic snapshot.
    pub fn checked(&self) -> Result<(), ProcessingError> {
        self.error().map_or(Ok(()), Err)
    }

    pub(crate) fn enter_conversion(&self) -> bool {
        if self.failed() {
            self.late_conversions.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.conversions.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Diagnostic renderer entry, not work charging or a cancellation checkpoint.
    pub fn enter_render(&self) -> bool {
        if self.failed() {
            self.late_renders.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.renders.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Conversion entries, render entries, late conversion entries, late render entries.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        (
            self.conversions.load(Ordering::Relaxed),
            self.renders.load(Ordering::Relaxed),
            self.late_conversions.load(Ordering::Relaxed),
            self.late_renders.load(Ordering::Relaxed),
        )
    }

    /// Conversion and render entry counts when the first error was recorded.
    pub fn counts_at_fault(&self) -> (usize, usize) {
        (
            self.conversions_at_fault.load(Ordering::Relaxed),
            self.renders_at_fault.load(Ordering::Relaxed),
        )
    }
}

impl From<ProcessingFailure> for FailureKind {
    fn from(value: ProcessingFailure) -> Self {
        match value {
            ProcessingFailure::ExternalResource => FailureKind::ExternalResource,
            ProcessingFailure::UnsupportedImageData => FailureKind::UnsupportedImageData,
            ProcessingFailure::InvalidImageHeader => FailureKind::InvalidImageHeader,
            ProcessingFailure::ImageDecode => FailureKind::ImageDecode,
            ProcessingFailure::MalformedImageData => FailureKind::MalformedImageData,
            ProcessingFailure::NestedImageDecode => FailureKind::NestedImageDecode,
            ProcessingFailure::RasterAllocation => FailureKind::RasterAllocation,
            ProcessingFailure::InvalidFilterRegion => FailureKind::InvalidFilterRegion,
            ProcessingFailure::FontReplay => FailureKind::FontReplay,
            ProcessingFailure::Native => FailureKind::Native,
            ProcessingFailure::ResourceQuota => FailureKind::Quota,
            ProcessingFailure::Allocation => FailureKind::Allocation,
            ProcessingFailure::Cancelled => FailureKind::Cancelled,
            ProcessingFailure::Decode => FailureKind::Decode,
            ProcessingFailure::Unsupported => FailureKind::Unsupported,
            ProcessingFailure::Invariant => FailureKind::Invariant,
        }
    }
}
impl From<FailureKind> for ProcessingFailure {
    fn from(value: FailureKind) -> Self {
        match value {
            FailureKind::ExternalResource => ProcessingFailure::ExternalResource,
            FailureKind::UnsupportedImageData => ProcessingFailure::UnsupportedImageData,
            FailureKind::InvalidImageHeader => ProcessingFailure::InvalidImageHeader,
            FailureKind::ImageDecode => ProcessingFailure::ImageDecode,
            FailureKind::MalformedImageData => ProcessingFailure::MalformedImageData,
            FailureKind::NestedImageDecode => ProcessingFailure::NestedImageDecode,
            FailureKind::RasterAllocation => ProcessingFailure::RasterAllocation,
            FailureKind::InvalidFilterRegion => ProcessingFailure::InvalidFilterRegion,
            FailureKind::FontReplay => ProcessingFailure::FontReplay,
            FailureKind::Native => ProcessingFailure::Native,
            FailureKind::Quota => ProcessingFailure::ResourceQuota,
            FailureKind::Allocation => ProcessingFailure::Allocation,
            FailureKind::Cancelled => ProcessingFailure::Cancelled,
            FailureKind::Decode => ProcessingFailure::Decode,
            FailureKind::Unsupported => ProcessingFailure::Unsupported,
            FailureKind::Invariant => ProcessingFailure::Invariant,
        }
    }
}

#[cfg(test)]
mod shared_failure_tests {
    use super::*;
    use markdown_svg_budget::{Backend, Error, Limits, ResourceKind, Ticket};
    struct Reject;
    impl Backend for Reject {
        fn reserve(&self, _: ResourceKind, _: usize) -> Result<Ticket, Error> {
            Err(Error::native(-123))
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
            Ok(())
        }
        fn release_after_free(&self, _: Ticket) -> Result<(), Error> {
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), Error> {
            Err(Error::native(-456))
        }
    }
    #[test]
    fn actual_backend_hresult_survives_font_wrapper_and_processing_snapshot() {
        let resources =
            ResourcesRef::new_prepaid(Reject, FirstFailureRef::new_prepaid(), Limits::default());
        let processing = ProcessingContext::with_resources(resources.clone());
        assert_eq!(
            resources
                .reserve(ResourceKind::Scratch, 16)
                .unwrap_err()
                .native_hr,
            Some(-123)
        );
        processing.fail(ProcessingFailure::FontReplay);
        let error = processing.error().unwrap();
        assert_eq!(error.kind, ProcessingFailure::Native);
        assert_eq!(error.typed_failure.native_hr, Some(-123));
        assert_eq!(processing.resource_failure(), resources.failure());
        drop(processing);
        resources.validate_retired().unwrap();
    }
    #[test]
    fn earlier_source_semantics_are_not_overwritten_by_later_platform_failure() {
        let resources =
            ResourcesRef::new_prepaid(Reject, FirstFailureRef::new_prepaid(), Limits::default());
        let processing = ProcessingContext::with_resources(resources.clone());
        processing.fail(ProcessingFailure::ExternalResource);
        resources.record(Error::native(-123));
        processing.fail(ProcessingFailure::FontReplay);
        let first = processing.error().unwrap();
        assert_eq!(first.kind, ProcessingFailure::ExternalResource);
        assert_eq!(first.typed_failure.native_hr, None);
        assert_eq!(processing.resource_failure(), resources.failure());
        drop(processing);
        resources.validate_retired().unwrap();
    }
}
