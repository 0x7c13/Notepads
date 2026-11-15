// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Feature-only, same-account observations. The Adapter owns the only strong
//! recorder handle; a snapshot copies bytes and never extends engine lifetime.
use crate::{
    abi::{self, Failure, Result},
    memory::{AdmittedSlice, Context, failure_from_resource},
    observation::{Observation, QualityTraceInfo},
};
use markdown_svg_budget::{Error, FailureKind, LimitKind};
use std::{
    fmt::{self, Write},
    sync::{Mutex, MutexGuard},
};

const MAXIMUM_BYTES: usize = 8 * 1024 * 1024;
const WORK_WINDOW: usize = 4096;
pub(crate) const INPUT_WORK_WINDOW: usize = 256;
const CAPACITY_ERROR: u32 = 1;
const FORMAT_ERROR: u32 = 2;
const POISON_ERROR: u32 = 3;
const WORK_ERROR: u32 = 4;

/// Final paid storage. `position` may advance during a record, but readers only
/// see `committed`: even a formatter panic cannot publish a partial JSONL row.
pub(crate) struct TraceWriter {
    bytes: AdmittedSlice<u8>,
    position: usize,
    committed: usize,
    events: u64,
    error: u32,
    pending_lines: u64,
    context: Context,
    work_credit: usize,
    work_failure: Option<Failure>,
}
impl TraceWriter {
    fn new(context: &Context, capacity: usize) -> Result<Self> {
        Ok(Self {
            bytes: AdmittedSlice::zeroed(context, abi::SCRATCH, capacity)?,
            position: 0,
            committed: 0,
            events: 0,
            error: 0,
            pending_lines: 0,
            context: context.clone(),
            work_credit: 0,
            work_failure: None,
        })
    }
    fn fail(&mut self, error: u32) -> fmt::Error {
        if self.error == 0 {
            self.error = error;
        }
        fmt::Error
    }
    /// Pay for a bounded input window even when every input is suppressed by
    /// deduplication. The caller checks before processing each window; only
    /// the final partial window can overpay, by at most 255 input items.
    pub(crate) fn checkpoint_input_window(&mut self) -> Result<()> {
        if self.error != 0 {
            return Err(self.work_failure.unwrap_or(Failure::Contract));
        }
        let payment = match self.context.failure() {
            Some(failure) => Err(failure),
            None => self.context.checkpoint(INPUT_WORK_WINDOW as u64),
        };
        if let Err(failure) = payment {
            self.work_failure = Some(failure);
            self.error = WORK_ERROR;
            return Err(failure);
        }
        Ok(())
    }
    pub(crate) fn finite(&mut self, value: f32) -> fmt::Result {
        if value.is_finite() {
            Ok(())
        } else {
            Err(self.fail(FORMAT_ERROR))
        }
    }
    pub(crate) fn quoted(&mut self, value: &str) -> fmt::Result {
        self.write_char('"')?;
        for ch in value.chars() {
            match ch {
                '"' => self.write_str("\\\"")?,
                '\\' => self.write_str("\\\\")?,
                ch if ch < ' ' => write!(self, "\\u{:04x}", ch as u32)?,
                ch => self.write_char(ch)?,
            }
        }
        self.write_char('"')
    }
    pub(crate) fn matrix(&mut self, value: tiny_skia_path::Transform) -> fmt::Result {
        let values = [value.sx, value.ky, value.kx, value.sy, value.tx, value.ty];
        for value in values {
            self.finite(value)?;
        }
        write!(
            self,
            "[{},{},{},{},{},{}]",
            values[0], values[1], values[2], values[3], values[4], values[5]
        )
    }
    pub(crate) fn matrix_bits(&mut self, value: tiny_skia_path::Transform) -> fmt::Result {
        let values = [value.sx, value.ky, value.kx, value.sy, value.tx, value.ty];
        for value in values {
            self.finite(value)?;
        }
        write!(
            self,
            "[{},{},{},{},{},{}]",
            values[0].to_bits(),
            values[1].to_bits(),
            values[2].to_bits(),
            values[3].to_bits(),
            values[4].to_bits(),
            values[5].to_bits()
        )
    }
    /// Observe the actual already-owned local path; do not draw or rebuild it.
    pub(crate) fn path(&mut self, path: Option<&tiny_skia_path::Path>) -> fmt::Result {
        let Some(path) = path else {
            return self.write_str("null");
        };
        self.write_char('[')?;
        for (index, segment) in path.segments().enumerate() {
            if index != 0 {
                self.write_char(',')?;
            }
            use tiny_skia_path::PathSegment::*;
            match segment {
                MoveTo(point) | LineTo(point) => {
                    self.finite(point.x)?;
                    self.finite(point.y)?;
                    write!(
                        self,
                        "[{},{},{}]",
                        if matches!(segment, MoveTo(_)) { 0 } else { 1 },
                        point.x,
                        point.y
                    )?;
                }
                QuadTo(a, b) => {
                    for value in [a.x, a.y, b.x, b.y] {
                        self.finite(value)?;
                    }
                    write!(self, "[2,{},{},{},{}]", a.x, a.y, b.x, b.y)?;
                }
                CubicTo(a, b, c) => {
                    for value in [a.x, a.y, b.x, b.y, c.x, c.y] {
                        self.finite(value)?;
                    }
                    write!(self, "[3,{},{},{},{},{},{}]", a.x, a.y, b.x, b.y, c.x, c.y)?;
                }
                Close => self.write_str("[4]")?,
            }
        }
        self.write_char(']')
    }
    /// Borrow the original commands without transforming or normalizing them.
    pub(crate) fn path_bits(&mut self, path: &tiny_skia_path::Path) -> fmt::Result {
        self.write_char('[')?;
        for (index, segment) in path.segments().enumerate() {
            if index % INPUT_WORK_WINDOW == 0 {
                self.checkpoint_input_window().map_err(|_| fmt::Error)?;
            }
            if index != 0 {
                self.write_char(',')?;
            }
            use tiny_skia_path::PathSegment::*;
            match segment {
                MoveTo(point) | LineTo(point) => {
                    self.finite(point.x)?;
                    self.finite(point.y)?;
                    write!(
                        self,
                        "[{},{},{}]",
                        if matches!(segment, MoveTo(_)) { 0 } else { 1 },
                        point.x.to_bits(),
                        point.y.to_bits()
                    )?;
                }
                QuadTo(a, b) => {
                    for value in [a.x, a.y, b.x, b.y] {
                        self.finite(value)?;
                    }
                    write!(
                        self,
                        "[2,{},{},{},{}]",
                        a.x.to_bits(),
                        a.y.to_bits(),
                        b.x.to_bits(),
                        b.y.to_bits()
                    )?;
                }
                CubicTo(a, b, c) => {
                    for value in [a.x, a.y, b.x, b.y, c.x, c.y] {
                        self.finite(value)?;
                    }
                    write!(
                        self,
                        "[3,{},{},{},{},{},{}]",
                        a.x.to_bits(),
                        a.y.to_bits(),
                        b.x.to_bits(),
                        b.y.to_bits(),
                        c.x.to_bits(),
                        c.y.to_bits()
                    )?;
                }
                Close => self.write_str("[4]")?,
            }
        }
        self.write_char(']')
    }
    fn info(&self) -> QualityTraceInfo {
        QualityTraceInfo {
            bytes: self.committed as u64,
            capacity: self.bytes.len() as u64,
            events: self.events,
            flags: 1 | if self.error == CAPACITY_ERROR { 2 } else { 0 },
            error: self.error,
        }
    }
}
impl Write for TraceWriter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.error != 0 {
            return Err(fmt::Error);
        }
        let mut remaining = value.as_bytes();
        while !remaining.is_empty() {
            let available = self.bytes.len() - self.position;
            if available == 0 {
                return Err(self.fail(CAPACITY_ERROR));
            }
            if self.work_credit == 0 {
                // The writer is exclusively loaned out of the mutex. A host
                // callback can therefore run here without holding that mutex.
                // Credit survives writes and records: the only unused prepay
                // is at most 4096 bytes for this entire recorder, never 8 MiB
                // per row. Small final buffers prepay only their remaining size.
                let credit = WORK_WINDOW.min(available);
                let payment = match self.context.failure() {
                    Some(failure) => Err(failure),
                    None => self.context.checkpoint(credit as u64),
                };
                if let Err(failure) = payment {
                    self.work_failure = Some(failure);
                    return Err(self.fail(WORK_ERROR));
                }
                self.work_credit = credit;
            }
            let count = remaining.len().min(available).min(self.work_credit);
            let (chunk, rest) = remaining.split_at(count);
            let end = self.position + count;
            self.bytes.as_mut_slice()[self.position..end].copy_from_slice(chunk);
            // Both the copy and line count are bounded by the prepaid window.
            // A failed record never needs a second scan of its partial output.
            self.pending_lines += chunk.iter().filter(|&&byte| byte == b'\n').count() as u64;
            self.position = end;
            self.work_credit -= count;
            remaining = rest;
        }
        Ok(())
    }
}

pub(crate) struct QualityRecorder {
    storage: Mutex<Option<TraceWriter>>,
    context: Context,
}

/// The slot is empty only while this unique, synchronous loan owns its writer.
/// Restoration never calls the host, including during formatter unwind.
struct WriterLoan<'a> {
    storage: &'a Mutex<Option<TraceWriter>>,
    writer: Option<TraceWriter>,
}
impl Drop for WriterLoan<'_> {
    fn drop(&mut self) {
        let Some(mut writer) = self.writer.take() else {
            return;
        };
        writer.position = writer.committed;
        writer.pending_lines = 0;
        if std::thread::panicking() && writer.error == 0 {
            writer.error = POISON_ERROR;
        }
        let mut storage = match self.storage.lock() {
            Ok(storage) => storage,
            Err(poisoned) => poisoned.into_inner(),
        };
        // Only loan creation can take the writer, and it cannot take an empty
        // slot. Thus this assignment moves into None and destroys no storage.
        *storage = Some(writer);
    }
}
impl QualityRecorder {
    pub(crate) fn new(context: &Context, capacity: usize) -> Result<Self> {
        if let Some(failure) = context.failure() {
            return Err(failure);
        }
        context.checkpoint(1)?;
        if !(1..=MAXIMUM_BYTES).contains(&capacity) {
            return Err(context.record_failure(Failure::Contract));
        }
        Ok(Self {
            storage: Mutex::new(Some(TraceWriter::new(context, capacity)?)),
            context: context.clone(),
        })
    }
    fn snapshot_storage(&self) -> MutexGuard<'_, Option<TraceWriter>> {
        match self.storage.lock() {
            Ok(storage) => storage,
            Err(poisoned) => {
                let mut storage = poisoned.into_inner();
                if let Some(writer) = storage.as_mut() {
                    if writer.error == 0 {
                        writer.error = POISON_ERROR;
                    }
                }
                storage
            }
        }
    }
    fn loan_writer(&self) -> Result<WriterLoan<'_>> {
        let writer = self.snapshot_storage().take().ok_or(Failure::Contract)?;
        Ok(WriterLoan {
            storage: &self.storage,
            writer: Some(writer),
        })
    }
    fn record_failure(&self, error: u32, cause: Option<Failure>) -> Failure {
        if error == CAPACITY_ERROR {
            failure_from_resource(
                self.context
                    .record_resource(Error::new(FailureKind::Quota, LimitKind::Capacity)),
            )
        } else {
            self.context
                .record_failure(cause.unwrap_or(Failure::Contract))
        }
    }
}

impl Observation for QualityRecorder {
    fn metrics(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayMetricsObservation<'_>,
    ) -> Result<()> {
        self.record(&mut |trace| {
            for number in [value.metrics.ascent, value.metrics.descent, value.metrics.leading] {
                trace.finite(number).map_err(|_| Failure::Contract)?;
            }
            let metrics=&value.metrics;
            write!(trace,"{{\"kind\":\"actualSkrifaMetrics\",\"faceTicket\":{},\"orientation\":{},\"faceIndex\":{},\"metricsUnitsPerEm\":{},\"realMetricsLocation\":true,\"ascent\":{},\"descent\":{},\"leading\":{},\"designArgument\":[",face.ticket,orientation,value.face_index,metrics.units_per_em,metrics.ascent,metrics.descent,metrics.leading).map_err(|_|Failure::Contract)?;
            for (i,axis) in value.variations.iter().enumerate() {
                trace.finite(axis.value).map_err(|_| Failure::Contract)?;
                if i!=0 { trace.write_char(',').map_err(|_|Failure::Contract)?; }
                write!(trace,"{{\"tagBE\":{},\"value\":{}}}",u32::from_be_bytes(axis.tag),axis.value).map_err(|_|Failure::Contract)?;
            }
            trace.write_str("],\"actualNormalizedF2Dot14\":[").map_err(|_|Failure::Contract)?;
            for (i,coord) in value.location.coords().iter().enumerate() {
                if i!=0 { trace.write_char(',').map_err(|_|Failure::Contract)?; }
                write!(trace,"{}",coord.to_bits()).map_err(|_|Failure::Contract)?;
            }
            trace.write_str("],\"fvarAxes\":[").map_err(|_|Failure::Contract)?;
            for i in 0..value.axes.len() {
                let axis = value.axes.try_get(i).map_err(failure_from_resource)?.ok_or(Failure::Contract)?;
                for number in [axis.min_value(), axis.default_value(), axis.max_value()] {
                    trace.finite(number).map_err(|_| Failure::Contract)?;
                }
                if i!=0 { trace.write_char(',').map_err(|_|Failure::Contract)?; }
                write!(trace,"{{\"tagBE\":{},\"minimum\":{},\"default\":{},\"maximum\":{}}}",u32::from_be_bytes(axis.tag().to_be_bytes()),axis.min_value(),axis.default_value(),axis.max_value()).map_err(|_|Failure::Contract)?;
            }
            trace.write_str("]}\n").map_err(|_|Failure::Contract)
        })
    }
    fn layout_metrics(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayLayoutMetricsObservation,
    ) -> Result<()> {
        self.record(&mut |trace| {
            let v=value.values;
            writeln!(trace,"{{\"kind\":\"actualLayoutMetrics\",\"faceTicket\":{},\"orientation\":{},\"unitsPerEm\":{},\"syntheticOverrideConsumed\":{},\"ascent\":{},\"descent\":{},\"xHeight\":{},\"underlinePosition\":{},\"underlineThickness\":{},\"strikePosition\":{}}}",face.ticket,orientation,value.units_per_em,value.synthetic,v[0],v[1],v[2],v[3],v[4],v[5]).map_err(|_|Failure::Contract)
        })
    }
    fn outline(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayOutlineObservation<'_>,
    ) -> Result<()> {
        self.record(&mut |trace| {
            write!(trace,"{{\"kind\":\"actualSkrifaOutline\",\"faceTicket\":{},\"orientation\":{},\"glyph\":{},\"faceIndex\":{},\"outlineUnitsPerEm\":{},\"realDrawSettingsLocation\":true,\"designArgument\":[",face.ticket,orientation,value.glyph,value.face_index,value.units_per_em).map_err(|_|Failure::Contract)?;
            for (i,axis) in value.variations.iter().enumerate() {
                trace.finite(axis.value).map_err(|_| Failure::Contract)?;
                if i!=0 { trace.write_char(',').map_err(|_|Failure::Contract)?; }
                write!(trace,"{{\"tagBE\":{},\"value\":{}}}",u32::from_be_bytes(axis.tag),axis.value).map_err(|_|Failure::Contract)?;
            }
            trace.write_str("],\"actualNormalizedF2Dot14\":[").map_err(|_|Failure::Contract)?;
            for (i,coord) in value.location.coords().iter().enumerate() {
                if i!=0 { trace.write_char(',').map_err(|_|Failure::Contract)?; }
                write!(trace,"{}",coord.to_bits()).map_err(|_|Failure::Contract)?;
            }
            trace.write_str("],\"fvarAxes\":[").map_err(|_|Failure::Contract)?;
            for i in 0..value.axes.len() {
                let axis = value.axes.try_get(i).map_err(failure_from_resource)?.ok_or(Failure::Contract)?;
                for number in [axis.min_value(), axis.default_value(), axis.max_value()] {
                    trace.finite(number).map_err(|_| Failure::Contract)?;
                }
                if i!=0 { trace.write_char(',').map_err(|_|Failure::Contract)?; }
                write!(trace,"{{\"tagBE\":{},\"minimum\":{},\"default\":{},\"maximum\":{}}}",u32::from_be_bytes(axis.tag().to_be_bytes()),axis.min_value(),axis.default_value(),axis.max_value()).map_err(|_|Failure::Contract)?;
            }
            trace.write_str("],\"commandsLocalYUp\":").map_err(|_|Failure::Contract)?;
            trace.path(value.path).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"commandKind\":\"Move0-Line1-Quad2-Cubic3-Close4\",\"cacheWitness\":\"real-draw-on-cache-miss\"}\n").map_err(|_|Failure::Contract)
        })
    }
    fn geometry(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayGeometryObservation<'_>,
    ) -> Result<()> {
        self.record(&mut |trace| {
            trace.finite(value.font_size).map_err(|_| Failure::Contract)?;
            for number in [value.engine_metrics_midpoint, value.fill_opacity].into_iter().flatten() {
                trace.finite(number).map_err(|_| Failure::Contract)?;
            }
            write!(trace,"{{\"kind\":\"actualFlattenGlyph\",\"faceTicket\":{},\"orientation\":{},\"glyph\":{},\"elementId\":",face.ticket,orientation,value.glyph).map_err(|_|Failure::Contract)?;
            trace.quoted(value.element_id).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"clusterText\":").map_err(|_|Failure::Contract)?;
            trace.quoted(value.cluster_text).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"geometryOrigin\":").map_err(|_|Failure::Contract)?;
            trace.quoted(match value.geometry_origin {
                usvg::ReplayGeometryOrigin::FontDesignOriginYUp => "FontDesignOriginYUp",
                usvg::ReplayGeometryOrigin::NativeSidewaysRunPenNormalized => "NativeSidewaysRunPenNormalized",
            }).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"placementPolicy\":").map_err(|_|Failure::Contract)?;
            trace.quoted(match value.placement_policy {
                usvg::ReplayPlacementPolicy::UnchangedHorizontal => "UnchangedHorizontal",
                usvg::ReplayPlacementPolicy::EngineMetricsMidpointUpright => "EngineMetricsMidpointUpright",
                usvg::ReplayPlacementPolicy::EngineMetricsMidpointRotated => "EngineMetricsMidpointRotated",
                usvg::ReplayPlacementPolicy::NativeSidewaysInverse90 => "NativeSidewaysInverse90",
            }).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"engineMetricsMidpointDIP\":").map_err(|_|Failure::Contract)?;
            if let Some(midpoint) = value.engine_metrics_midpoint {
                write!(trace,"{midpoint}").map_err(|_|Failure::Contract)?;
            } else { trace.write_str("null").map_err(|_|Failure::Contract)?; }
            write!(trace,",\"representation\":{},\"layoutUnitsPerEm\":{},\"fontSize\":{},\"glyphMatrix\":",value.representation,value.units_per_em,value.font_size).map_err(|_|Failure::Contract)?;
            trace.matrix(value.glyph_transform).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"clusterMatrix\":").map_err(|_|Failure::Contract)?;
            trace.matrix(value.cluster_transform).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"spanMatrix\":").map_err(|_|Failure::Contract)?;
            trace.matrix(value.span_transform).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"consumedRepresentationMatrix\":").map_err(|_|Failure::Contract)?;
            trace.matrix(value.representation_transform).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"sceneMatrix\":").map_err(|_|Failure::Contract)?;
            trace.matrix(value.scene_transform).map_err(|_|Failure::Contract)?;
            trace.write_str(",\"combinedFinalMatrix\":").map_err(|_|Failure::Contract)?;
            trace.matrix(value.scene_transform.pre_concat(value.representation_transform)).map_err(|_|Failure::Contract)?;
            write!(trace,",\"visible\":{},\"fillRule\":{},\"spanFillOpacity\":",value.visible,if value.fill_rule==usvg::FillRule::EvenOdd {0}else{1}).map_err(|_|Failure::Contract)?;
            if let Some(opacity)=value.fill_opacity { write!(trace,"{opacity}").map_err(|_|Failure::Contract)?; }
            else { trace.write_str("null").map_err(|_|Failure::Contract)?; }
            trace.write_str(",\"commandsLocalYUp\":").map_err(|_|Failure::Contract)?;
            trace.path(value.path).map_err(|_|Failure::Contract)?;
            writeln!(trace,",\"colorLeafGeometryObserved\":false,\"actualFlattenConsumption\":true}}").map_err(|_|Failure::Contract)
        })
    }
    fn record(&self, action: &mut dyn FnMut(&mut TraceWriter) -> Result<()>) -> Result<()> {
        if let Some(failure) = self.context.failure() {
            return Err(failure);
        }
        self.context.checkpoint(1)?;
        let (error, cause) = {
            let mut loan = self
                .loan_writer()
                .map_err(|failure| self.context.record_failure(failure))?;
            // A loan always contains its writer until its Drop returns it.
            let storage = loan.writer.as_mut().ok_or(Failure::Contract)?;
            if storage.error != 0 {
                let error = storage.error;
                let cause = storage.work_failure;
                drop(loan);
                return Err(self.record_failure(error, cause));
            }
            storage.position = storage.committed;
            storage.pending_lines = 0;
            let result = action(storage);
            let mut cause = result.err();
            if cause.is_none() && storage.error == 0 {
                let last = storage
                    .position
                    .checked_sub(1)
                    .and_then(|index| storage.bytes.as_slice().get(index));
                if last != Some(&b'\n') || storage.pending_lines == 0 {
                    storage.error = FORMAT_ERROR;
                    cause = Some(Failure::Contract);
                } else if let Some(events) = storage.events.checked_add(storage.pending_lines) {
                    storage.events = events;
                    storage.committed = storage.position;
                } else {
                    storage.error = FORMAT_ERROR;
                    cause = Some(Failure::Contract);
                }
            }
            if cause.is_some() && storage.error == 0 {
                storage.error = FORMAT_ERROR;
            }
            // A checkpoint error is the original typed Context fault. Do not
            // replace it with the formatter's projection of fmt::Error.
            (storage.error, storage.work_failure.or(cause))
        };
        if error != 0 {
            return Err(self.record_failure(error, cause));
        }
        if let Some(failure) = self.context.failure() {
            return Err(failure);
        }
        Ok(())
    }
    fn info(&self) -> Result<QualityTraceInfo> {
        // Diagnostic reads remain possible after a work fault/cancellation.
        let storage = self.snapshot_storage();
        Ok(storage.as_ref().ok_or(Failure::Contract)?.info())
    }
    fn copy(&self, destination: &mut [u8]) -> Result<usize> {
        let storage = self.snapshot_storage();
        let storage = storage.as_ref().ok_or(Failure::Contract)?;
        if destination.len() < storage.committed {
            // Preflight the entire copy; diagnostics never latch another fault.
            return Err(Failure::Contract);
        }
        destination[..storage.committed]
            .copy_from_slice(&storage.bytes.as_slice()[..storage.committed]);
        Ok(storage.committed)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::memory::Host;
    use markdown_svg_budget::{ResourceKind, Ticket};
    use std::{
        collections::BTreeMap,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        },
    };

    #[derive(Default)]
    pub(crate) struct TestHost {
        next: AtomicU32,
        live: Mutex<BTreeMap<u32, (usize, bool)>>,
        pub(crate) committed: AtomicU64,
        pub(crate) released: AtomicU64,
        pub(crate) checkpoints: AtomicU64,
        pub(crate) work: AtomicU64,
        pub(crate) maximum_work: AtomicU64,
        pub(crate) reject_reserve: AtomicBool,
    }
    impl TestHost {
        pub(crate) fn assert_empty(&self) {
            assert!(self.live.lock().unwrap().is_empty());
        }
    }
    impl Host for TestHost {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> std::result::Result<Ticket, Error> {
            if self.reject_reserve.load(Ordering::Relaxed) {
                return Err(Error::native(-61));
            }
            let slot = self.next.fetch_add(1, Ordering::Relaxed);
            self.live.lock().unwrap().insert(slot, (bytes, false));
            Ticket::new(1, slot, 1)
        }
        fn allocation_committed(
            &self,
            ticket: &Ticket,
            bytes: usize,
        ) -> std::result::Result<(), Error> {
            let mut live = self.live.lock().unwrap();
            let row = live.get_mut(&ticket.slot()).unwrap();
            assert!(bytes <= row.0 && !row.1);
            row.1 = true;
            self.committed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn release_after_free(&self, ticket: Ticket) -> std::result::Result<(), Error> {
            assert!(self.live.lock().unwrap().remove(&ticket.slot()).is_some());
            self.released.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn checkpoint(&self, work: u64) -> std::result::Result<(), Error> {
            self.checkpoints.fetch_add(1, Ordering::Relaxed);
            let maximum = self.maximum_work.load(Ordering::Relaxed);
            self.work
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    current
                        .checked_add(work)
                        .filter(|&total| maximum == 0 || total <= maximum)
                })
                .map_err(|_| Error::new(FailureKind::Cancelled, LimitKind::None))?;
            Ok(())
        }
    }
    unsafe extern "C" fn no_plan(_: u64, _: *const abi::Draft, _: *mut abi::Plan) -> i32 {
        -1
    }
    unsafe extern "C" fn no_font(_: u64, _: u32, _: *mut u8, _: u64, _: *mut u64) -> i32 {
        -1
    }
    unsafe extern "C" fn no_metrics(
        _: u64,
        _: u32,
        _: u32,
        _: *mut abi::GlyphMetrics,
        _: u32,
        _: *mut u32,
    ) -> i32 {
        -1
    }
    unsafe extern "C" fn no_prepare(
        _: u64,
        _: *const abi::GlyphKey,
        _: u32,
        _: *mut abi::GeometryReceipt,
    ) -> i32 {
        -1
    }
    unsafe extern "C" fn no_geometry(
        _: u64,
        _: u64,
        _: *mut abi::GlyphPath,
        _: u32,
        _: *mut abi::Command,
        _: u32,
        _: *mut u32,
        _: *mut u32,
    ) -> i32 {
        -1
    }
    unsafe extern "C" fn no_retire(_: u64, _: u64) -> i32 {
        -1
    }
    pub(crate) fn context() -> (Context, Arc<TestHost>) {
        let host = Arc::new(TestHost::default());
        let callbacks = abi::Callbacks {
            size: size_of::<abi::Callbacks>() as u32,
            version: abi::FONT_CALLBACKS_VERSION,
            session: 1,
            plan: Some(no_plan),
            fill_font: Some(no_font),
            fill_metrics: Some(no_metrics),
            prepare_geometry: Some(no_prepare),
            fill_geometry: Some(no_geometry),
            retire_geometry: Some(no_retire),
        };
        let context =
            Context::new(callbacks, host.clone(), Arc::new(AtomicBool::new(false))).unwrap();
        (context, host)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{TestHost, context};
    use super::*;
    use markdown_svg_budget::{Lease, Limits, ResourceKind, ResourcesRef};
    use std::sync::atomic::Ordering;

    // The frozen builder makes exactly these two Vec allocations. Keep the
    // actual Path before its leases so unwind also frees storage before refund.
    struct PaidPath {
        path: tiny_skia_path::Path,
        _storage: [Lease; 2],
    }
    fn paid_path(
        resources: &ResourcesRef,
        verbs: usize,
        points: usize,
        append: impl FnOnce(&mut tiny_skia_path::PathBuilder),
    ) -> PaidPath {
        let mut storage = [
            resources
                .reserve(
                    ResourceKind::Geometry,
                    std::alloc::Layout::array::<tiny_skia_path::PathVerb>(verbs)
                        .unwrap()
                        .size(),
                )
                .unwrap(),
            resources
                .reserve(
                    ResourceKind::Geometry,
                    std::alloc::Layout::array::<tiny_skia_path::Point>(points)
                        .unwrap()
                        .size(),
                )
                .unwrap(),
        ];
        let mut builder = tiny_skia_path::PathBuilder::with_capacity(verbs, points);
        for fee in &mut storage {
            fee.allocation_committed().unwrap();
        }
        append(&mut builder);
        assert_eq!(builder.len(), verbs);
        let path = builder.finish().unwrap();
        assert_eq!(path.points().len(), points);
        PaidPath {
            path,
            _storage: storage,
        }
    }
    fn all_commands(resources: &ResourcesRef, contours: usize) -> PaidPath {
        paid_path(resources, contours * 5, contours * 7, |builder| {
            for _ in 0..contours {
                builder.move_to(0.0, -0.0);
                builder.line_to(1.0, f32::from_bits(0x3f80_0001));
                builder.quad_to(-1.0, 2.0, 3.0, 4.0);
                builder.cubic_to(-2.0, -3.0, 0.5, -0.5, 8.0, -8.0);
                builder.close();
            }
        })
    }
    fn zero_lines(resources: &ResourcesRef) -> PaidPath {
        paid_path(resources, 513, 513, |builder| {
            builder.move_to(0.0, 0.0);
            for _ in 0..512 {
                builder.line_to(0.0, 0.0);
            }
        })
    }
    fn assert_resources_released(context: &Context, resources: ResourcesRef, host: &TestHost) {
        resources.validate_retired().unwrap();
        drop(resources);
        host.assert_empty();
        assert_eq!(
            host.committed.load(Ordering::Relaxed),
            host.released.load(Ordering::Relaxed)
        );
        context.validate_released().unwrap();
        context.validate_scope_retired().unwrap();
    }

    #[test]
    fn bits_helpers_preserve_all_commands_repeated_contours_and_exact_f32_values() {
        let (context, host) = context();
        let resources = context.make_resources(Limits::default());
        let path = all_commands(&resources, 2);
        let recorder = QualityRecorder::new(&context, 1024).unwrap();
        recorder
            .record(&mut |trace| {
                trace
                    .write_str("{\"matrixBits\":")
                    .map_err(|_| Failure::Contract)?;
                trace
                    .matrix_bits(tiny_skia_path::Transform {
                        sx: 2.0,
                        ky: -0.5,
                        kx: 0.25,
                        sy: 3.0,
                        tx: -4.0,
                        ty: 8.0,
                    })
                    .map_err(|_| Failure::Contract)?;
                trace
                    .write_str(",\"commandsBits\":")
                    .map_err(|_| Failure::Contract)?;
                trace.path_bits(&path.path).map_err(|_| Failure::Contract)?;
                trace.write_str("}\n").map_err(|_| Failure::Contract)
            })
            .unwrap();
        // Independent IEEE-754 constants, including adjacent values near 1.0
        // and both zero signs; neither expected commands nor bits use the writer.
        let expected = concat!(
            r#"{"matrixBits":[1073741824,3204448256,1048576000,1077936128,3229614080,1090519040],"commandsBits":["#,
            "[0,0,2147483648],[1,1065353216,1065353217],",
            "[2,3212836864,1073741824,1077936128,1082130432],",
            "[3,3221225472,3225419776,1056964608,3204448256,1090519040,3238002688],[4],",
            "[0,0,2147483648],[1,1065353216,1065353217],",
            "[2,3212836864,1073741824,1077936128,1082130432],",
            "[3,3221225472,3225419776,1056964608,3204448256,1090519040,3238002688],[4]]}\n"
        );
        let mut copied = [0; 1024];
        let length = recorder.copy(&mut copied).unwrap();
        assert_eq!(&copied[..length], expected.as_bytes());
        assert_eq!(recorder.info().unwrap().events, 1);
        drop(path);
        drop(recorder);
        assert_resources_released(&context, resources, &host);
    }
    #[test]
    fn matrix_bits_rejects_nonfinite_values_before_writing_any_bits() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let (context, host) = context();
            let recorder = QualityRecorder::new(&context, 256).unwrap();
            recorder
                .record(&mut |trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
                .unwrap();
            assert_eq!(
                recorder.record(&mut |trace| {
                    trace
                        .write_str("{\"matrixBits\":")
                        .map_err(|_| Failure::Contract)?;
                    let before = trace.position;
                    let result = trace.matrix_bits(tiny_skia_path::Transform {
                        ty: invalid,
                        ..tiny_skia_path::Transform::identity()
                    });
                    assert_eq!(trace.position, before);
                    result.map_err(|_| Failure::Contract)
                }),
                Err(Failure::Contract)
            );
            let info = recorder.info().unwrap();
            assert_eq!((info.bytes, info.events, info.error), (3, 1, FORMAT_ERROR));
            let mut copied = [0xA5; 8];
            assert_eq!(recorder.copy(&mut copied).unwrap(), 3);
            assert_eq!(&copied[..3], b"{}\n");
            assert_eq!(&copied[3..], &[0xA5; 5]);
            drop(recorder);
            host.assert_empty();
            context.validate_released().unwrap();
            context.validate_scope_retired().unwrap();
        }
    }
    #[test]
    fn path_bits_capacity_failure_inside_field_keeps_prefix_and_typed_first_fault() {
        let (context, host) = context();
        let resources = context.make_resources(Limits::default());
        let path = all_commands(&resources, 1);
        let recorder = QualityRecorder::new(&context, 128).unwrap();
        recorder
            .record(&mut |trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
            .unwrap();
        let mut processed = 0;
        let mut failed_inside_path = false;
        let first = recorder.record(&mut |trace| {
            trace
                .write_str("{\"flattenPath\":{\"commandsBits\":")
                .map_err(|_| Failure::Contract)?;
            let field_start = trace.position;
            let result = trace.path_bits(&path.path);
            processed = trace.position;
            failed_inside_path = result.is_err();
            assert!(
                trace.bytes.as_slice()[field_start..processed]
                    .starts_with(b"[[0,0,2147483648],[1,1065353216,1065353217],")
            );
            result.map_err(|_| Failure::Contract)?;
            trace.write_str("}}\n").map_err(|_| Failure::Contract)
        });
        assert!(failed_inside_path);
        assert_eq!(processed, 128);
        let fault = context.resource_failure().unwrap();
        assert_eq!(
            (fault.kind, fault.limit),
            (FailureKind::Quota, LimitKind::Capacity)
        );
        assert_eq!(first, Err(failure_from_resource(fault)));
        assert_eq!(recorder.record(&mut |_| Err(Failure::Native(-99))), first);
        assert_eq!(context.resource_failure(), Some(fault));
        let info = recorder.info().unwrap();
        assert_eq!(
            (info.bytes, info.events, info.flags, info.error),
            (3, 1, 3, CAPACITY_ERROR)
        );
        let mut copied = [0xA5; 8];
        assert_eq!(recorder.copy(&mut copied).unwrap(), 3);
        assert_eq!(&copied[..3], b"{}\n");
        assert_eq!(&copied[3..], &[0xA5; 5]);
        drop(path);
        drop(recorder);
        assert_resources_released(&context, resources, &host);
    }
    #[test]
    fn path_bits_prepays_input_windows_and_reuses_bounded_output_credit() {
        let (context, host) = context();
        let resources = context.make_resources(Limits::default());
        let path = zero_lines(&resources);
        let recorder = QualityRecorder::new(&context, WORK_WINDOW * 3).unwrap();
        recorder
            .record(&mut |trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
            .unwrap();
        let before = host.work.load(Ordering::Relaxed);
        let before_checks = host.checkpoints.load(Ordering::Relaxed);
        let field = "{\"flattenPath\":{\"commandsBits\":";
        recorder
            .record(&mut |trace| {
                trace.write_str(field).map_err(|_| Failure::Contract)?;
                trace.path_bits(&path.path).map_err(|_| Failure::Contract)?;
                trace.write_str("}}\n").map_err(|_| Failure::Contract)
            })
            .unwrap();
        // 513 original commands occupy three input windows. The 4105-byte
        // command array crosses one output window after the prefix's prepay.
        assert_eq!(
            host.work.load(Ordering::Relaxed) - before,
            1 + 3 * INPUT_WORK_WINDOW as u64 + WORK_WINDOW as u64
        );
        assert_eq!(host.checkpoints.load(Ordering::Relaxed) - before_checks, 5);
        let info = recorder.info().unwrap();
        assert_eq!(info.bytes, (3 + field.len() + 4105 + 3) as u64);
        assert_eq!((info.events, info.error), (2, 0));
        drop(path);
        drop(recorder);
        assert_resources_released(&context, resources, &host);
    }
    #[test]
    fn path_bits_cancels_at_second_input_window_and_preserves_prefix_and_first_fault() {
        let (context, host) = context();
        let resources = context.make_resources(Limits::default());
        let path = zero_lines(&resources);
        let recorder = QualityRecorder::new(&context, WORK_WINDOW * 3).unwrap();
        recorder
            .record(&mut |trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
            .unwrap();
        let before = host.work.load(Ordering::Relaxed);
        let before_checks = host.checkpoints.load(Ordering::Relaxed);
        host.maximum_work
            .store(before + 1 + INPUT_WORK_WINDOW as u64, Ordering::Relaxed);
        let field = "{\"flattenPath\":{\"commandsBits\":";
        let mut processed = 0;
        let mut failed_inside_path = false;
        let first = recorder.record(&mut |trace| {
            trace.write_str(field).map_err(|_| Failure::Contract)?;
            let result = trace.path_bits(&path.path);
            processed = trace.position;
            failed_inside_path = result.is_err();
            result.map_err(|_| Failure::Contract)?;
            trace.write_str("}}\n").map_err(|_| Failure::Contract)
        });
        assert!(failed_inside_path);
        // Exactly the first 256 commands were serialized. The next input
        // checkpoint fails before a second output prepayment can be requested.
        assert_eq!(processed, 3 + field.len() + 2048);
        assert!(processed < WORK_WINDOW);
        assert_eq!(
            host.work.load(Ordering::Relaxed) - before,
            1 + INPUT_WORK_WINDOW as u64
        );
        assert_eq!(host.checkpoints.load(Ordering::Relaxed) - before_checks, 3);
        let fault = context.resource_failure().unwrap();
        assert_eq!(
            (fault.kind, fault.limit),
            (FailureKind::Cancelled, LimitKind::None)
        );
        assert_eq!(first, Err(failure_from_resource(fault)));
        assert_eq!(recorder.record(&mut |_| Err(Failure::Native(-99))), first);
        assert_eq!(context.resource_failure(), Some(fault));
        let info = recorder.info().unwrap();
        assert_eq!(
            (info.bytes, info.events, info.flags, info.error),
            (3, 1, 1, WORK_ERROR)
        );
        let mut copied = [0xA5; 8];
        assert_eq!(recorder.copy(&mut copied).unwrap(), 3);
        assert_eq!(&copied[..3], b"{}\n");
        assert_eq!(&copied[3..], &[0xA5; 5]);
        drop(path);
        drop(recorder);
        assert_resources_released(&context, resources, &host);
    }

    #[test]
    fn capacity_error_keeps_only_complete_prefix_and_same_first_fault() {
        let (context, host) = context();
        let recorder = QualityRecorder::new(&context, 32).unwrap();
        recorder
            .record(&mut |trace| {
                trace
                    .write_str("{\"ok\":1}\n")
                    .map_err(|_| Failure::Contract)
            })
            .unwrap();
        let first = recorder.record(&mut |trace| {
            trace
                .write_str("{\"tooLong\":\"")
                .map_err(|_| Failure::Contract)?;
            trace
                .write_str("this observation does not fit the final paid buffer")
                .map_err(|_| Failure::Contract)?;
            trace.write_str("\"}\n").map_err(|_| Failure::Contract)
        });
        assert!(first.is_err());
        let fault = context.resource_failure().unwrap();
        assert_eq!(fault.kind, FailureKind::Quota);
        assert_eq!(fault.limit, LimitKind::Capacity);
        assert_eq!(recorder.record(&mut |_| Err(Failure::Native(-77))), first);
        assert_eq!(context.resource_failure(), Some(fault));
        let before = host.checkpoints.load(Ordering::Relaxed);
        let info = recorder.info().unwrap();
        assert_eq!(
            (info.bytes, info.events, info.flags, info.error),
            (9, 1, 3, CAPACITY_ERROR)
        );
        let mut small = [0xA5; 8];
        assert!(recorder.copy(&mut small).is_err());
        assert_eq!(small, [0xA5; 8]);
        let mut copied = [0xA5; 16];
        assert_eq!(recorder.copy(&mut copied).unwrap(), 9);
        assert_eq!(&copied[..9], b"{\"ok\":1}\n");
        assert_eq!(&copied[9..], &[0xA5; 7]);
        assert_eq!(host.checkpoints.load(Ordering::Relaxed), before);
        assert_eq!(context.resource_failure(), Some(fault));
        drop(recorder);
        host.assert_empty();
        assert_eq!(
            host.committed.load(Ordering::Relaxed),
            host.released.load(Ordering::Relaxed)
        );
        context.validate_released().unwrap();
    }
    #[test]
    fn record_error_rolls_back_and_preserves_its_original_work_error() {
        let (context, host) = context();
        let recorder = QualityRecorder::new(&context, 64).unwrap();
        let before = host.work.load(Ordering::Relaxed);
        assert_eq!(
            recorder.record(&mut |trace| {
                trace
                    .write_str("{\"uncommitted\":")
                    .map_err(|_| Failure::Contract)?;
                Err(Failure::Native(-73))
            }),
            Err(Failure::Native(-73))
        );
        // Formatting failure cannot skip work already prepaid before writing.
        assert_eq!(host.work.load(Ordering::Relaxed) - before, 1 + 64);
        assert_eq!(context.failure(), Some(Failure::Native(-73)));
        assert_eq!(
            (
                recorder.info().unwrap().bytes,
                recorder.info().unwrap().events
            ),
            (0, 0)
        );
        let mut copied = [0xB6; 8];
        assert_eq!(recorder.copy(&mut copied).unwrap(), 0);
        assert_eq!(copied, [0xB6; 8]);
        assert_eq!(
            recorder.record(&mut |_| Err(Failure::Contract)),
            Err(Failure::Native(-73))
        );
        drop(recorder);
        host.assert_empty();
    }
    #[test]
    fn successful_records_prepay_bounded_windows_and_reuse_remaining_credit() {
        let (context, host) = context();
        let recorder = QualityRecorder::new(&context, WORK_WINDOW * 3).unwrap();
        let before = host.work.load(Ordering::Relaxed);
        let before_checks = host.checkpoints.load(Ordering::Relaxed);
        let body = "x".repeat(WORK_WINDOW * 2 + 17);
        recorder
            .record(&mut |trace| {
                trace
                    .write_str("{\"payload\":")
                    .map_err(|_| Failure::Contract)?;
                trace.quoted(&body).map_err(|_| Failure::Contract)?;
                trace.write_str("}\n").map_err(|_| Failure::Contract)
            })
            .unwrap();
        let info = recorder.info().unwrap();
        let paid = host.work.load(Ordering::Relaxed) - before;
        assert_eq!(paid, 1 + (WORK_WINDOW * 3) as u64);
        assert_eq!(host.checkpoints.load(Ordering::Relaxed) - before_checks, 4);
        assert!(paid - 1 >= info.bytes);
        assert!(paid - 1 - info.bytes < WORK_WINDOW as u64);
        let after = host.work.load(Ordering::Relaxed);
        recorder
            .record(&mut |trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
            .unwrap();
        assert_eq!(host.work.load(Ordering::Relaxed) - after, 1);
        assert_eq!(recorder.info().unwrap().events, 2);
        drop(recorder);
        host.assert_empty();
    }
    #[test]
    fn overflowing_record_charges_processed_windows_before_latching_capacity_fault() {
        let (context, host) = context();
        let capacity = WORK_WINDOW * 2 + 17;
        let recorder = QualityRecorder::new(&context, capacity).unwrap();
        recorder
            .record(&mut |trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
            .unwrap();
        let before = host.work.load(Ordering::Relaxed);
        let mut processed = 0;
        let body = "x".repeat(capacity * 2);
        assert!(
            recorder
                .record(&mut |trace| {
                    let result = trace.write_str(&body);
                    processed = trace.position;
                    result.map_err(|_| Failure::Contract)
                })
                .is_err()
        );
        assert_eq!(processed, capacity);
        // The prefix already paid for the first window. Every subsequent byte
        // processed before overflow is paid even though this row is rolled back.
        assert_eq!(
            host.work.load(Ordering::Relaxed) - before,
            1 + (capacity - WORK_WINDOW) as u64
        );
        let fault = context.resource_failure().unwrap();
        assert_eq!(
            (fault.kind, fault.limit),
            (FailureKind::Quota, LimitKind::Capacity)
        );
        let info = recorder.info().unwrap();
        assert_eq!(
            (info.bytes, info.events, info.error),
            (3, 1, CAPACITY_ERROR)
        );
        let mut copied = [0xA5; 8];
        assert_eq!(recorder.copy(&mut copied).unwrap(), 3);
        assert_eq!(&copied[..3], b"{}\n");
        assert_eq!(&copied[3..], &[0xA5; 5]);
        drop(recorder);
        host.assert_empty();
    }
    #[test]
    fn cancellation_inside_quoted_record_preserves_paid_work_and_committed_prefix() {
        let (context, host) = context();
        let recorder = QualityRecorder::new(&context, WORK_WINDOW * 4).unwrap();
        recorder
            .record(&mut |trace| trace.write_str("{}\n").map_err(|_| Failure::Contract))
            .unwrap();
        let before = host.work.load(Ordering::Relaxed);
        host.maximum_work
            .store(before + 1 + WORK_WINDOW as u64, Ordering::Relaxed);
        let body = "x".repeat(WORK_WINDOW * 3);
        let mut processed = 0;
        let result = recorder.record(&mut |trace| {
            let result = trace.quoted(&body);
            processed = trace.position;
            result.map_err(|_| Failure::Contract)
        });
        let fault = context.resource_failure().unwrap();
        assert_eq!(
            (fault.kind, fault.limit),
            (FailureKind::Cancelled, LimitKind::None)
        );
        assert_eq!(result, Err(failure_from_resource(fault)));
        assert_eq!(processed, WORK_WINDOW * 2);
        assert_eq!(
            host.work.load(Ordering::Relaxed) - before,
            1 + WORK_WINDOW as u64
        );
        assert!(host.work.load(Ordering::Relaxed) >= processed as u64);
        let checks = host.checkpoints.load(Ordering::Relaxed);
        let info = recorder.info().unwrap();
        assert_eq!((info.bytes, info.events, info.error), (3, 1, WORK_ERROR));
        let mut small = [0xA5; 2];
        assert_eq!(recorder.copy(&mut small), Err(Failure::Contract));
        assert_eq!(small, [0xA5; 2]);
        let mut copied = [0xA5; 8];
        assert_eq!(recorder.copy(&mut copied).unwrap(), 3);
        assert_eq!(&copied[..3], b"{}\n");
        assert_eq!(host.checkpoints.load(Ordering::Relaxed), checks);
        assert_eq!(
            context.record_failure(Failure::Native(-99)),
            result.unwrap_err()
        );
        assert_eq!(context.resource_failure(), Some(fault));
        drop(recorder);
        host.assert_empty();
    }
    #[test]
    fn active_loan_rejects_snapshots_without_locking_or_exposing_partial_rows() {
        let (context, host) = context();
        let recorder = QualityRecorder::new(&context, 32).unwrap();
        recorder
            .record(&mut |trace| {
                trace.write_str("{\"ok\":").map_err(|_| Failure::Contract)?;
                assert!(recorder.info().is_err());
                let mut copied = [0xA5; 8];
                assert_eq!(recorder.copy(&mut copied), Err(Failure::Contract));
                assert_eq!(copied, [0xA5; 8]);
                assert_eq!(context.failure(), None);
                trace.write_str("1}\n").map_err(|_| Failure::Contract)
            })
            .unwrap();
        let info = recorder.info().unwrap();
        assert_eq!((info.bytes, info.events, info.error), (9, 1, 0));
        drop(recorder);
        host.assert_empty();
    }
    #[test]
    fn panic_snapshot_recovers_committed_prefix_without_publishing_partial_row() {
        let (context, host) = context();
        let recorder = QualityRecorder::new(&context, 64).unwrap();
        recorder
            .record(&mut |trace| {
                trace
                    .write_str("{\"ok\":1}\n")
                    .map_err(|_| Failure::Contract)
            })
            .unwrap();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = recorder.record(&mut |trace| {
                trace.write_str("{\"partial\":").unwrap();
                panic!("formatter panic before record commit")
            });
        }));
        assert!(caught.is_err());
        let info = recorder.info().unwrap();
        assert_eq!((info.bytes, info.events, info.error), (9, 1, POISON_ERROR));
        let mut copied = [0xA5; 16];
        assert_eq!(recorder.copy(&mut copied).unwrap(), 9);
        assert_eq!(&copied[..9], b"{\"ok\":1}\n");
        // Reading diagnostics itself does not invent or replace a work fault.
        assert_eq!(context.failure(), None);
        drop(recorder);
        host.assert_empty();
    }
    #[test]
    fn borrowed_json_helpers_escape_strings_and_refuse_nonfinite_matrix() {
        let (context, host) = context();
        let recorder = QualityRecorder::new(&context, 512).unwrap();
        recorder
            .record(&mut |trace| {
                trace
                    .write_str("{\"text\":")
                    .map_err(|_| Failure::Contract)?;
                trace
                    .quoted("a\"\n\\\0中文")
                    .map_err(|_| Failure::Contract)?;
                trace
                    .write_str(",\"matrix\":")
                    .map_err(|_| Failure::Contract)?;
                trace
                    .matrix(tiny_skia_path::Transform::identity())
                    .map_err(|_| Failure::Contract)?;
                trace
                    .write_str(",\"path\":")
                    .map_err(|_| Failure::Contract)?;
                trace.path(None).map_err(|_| Failure::Contract)?;
                trace.write_str("}\n").map_err(|_| Failure::Contract)
            })
            .unwrap();
        let info = recorder.info().unwrap();
        let mut copied = [0; 512];
        let length = recorder.copy(&mut copied).unwrap();
        let text = std::str::from_utf8(&copied[..length]).unwrap();
        assert!(text.contains("a\\\"\\u000a\\\\\\u0000中文"));
        assert!(text.ends_with("\"path\":null}\n"));
        let mut invalid = tiny_skia_path::Transform::identity();
        invalid.sx = f32::NAN;
        assert!(
            recorder
                .record(&mut |trace| trace.matrix(invalid).map_err(|_| Failure::Contract))
                .is_err()
        );
        assert_eq!(recorder.info().unwrap().bytes, info.bytes);
        assert_eq!(recorder.info().unwrap().error, FORMAT_ERROR);
        drop(recorder);
        host.assert_empty();
    }
    #[test]
    fn rejected_reservation_never_admits_or_releases_unallocated_storage() {
        let (context, host) = context();
        host.reject_reserve.store(true, Ordering::Relaxed);
        assert!(matches!(
            QualityRecorder::new(&context, 32),
            Err(Failure::Native(-61))
        ));
        host.assert_empty();
        assert_eq!(host.committed.load(Ordering::Relaxed), 0);
        assert_eq!(host.released.load(Ordering::Relaxed), 0);
        assert_eq!(context.failure(), Some(Failure::Native(-61)));
    }
}
