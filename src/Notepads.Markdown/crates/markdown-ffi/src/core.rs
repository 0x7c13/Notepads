// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private model ABI v3. All pointer validity and call ordering are caller obligations.

mod ownership;
mod wire;
pub use ownership::Owned;
use ownership::{Slot, release};
pub use wire::*;

use crate::{STATUS_ABI_MISMATCH, STATUS_INVALID_ARGUMENT, STATUS_PANIC, contain_panic};
use markdown_core::config::{ParseConfig, Profile};
use markdown_core::error::{CoreError, LimitKind};
use markdown_core::identity::*;
use markdown_core::ir::{AtomKind, TextTarget};
use markdown_core::mapping::*;
use markdown_core::model::AcceptedModel;
use markdown_core::policy::{
    BoundaryReservation, BoundaryWork, CancellationToken, Operation, SourceBudget,
};
use markdown_core::reader::MainReader;
use markdown_core::session::{
    OwnedCompletedParse, OwnedParseRequest, PendingSourceRequest, SourceSession,
};
use std::alloc::{Layout, alloc, dealloc};
use std::mem::{align_of, size_of};
use std::ptr::{self, NonNull};
use std::sync::Arc;

pub const CORE_ABI_VERSION: u32 = 3;

pub struct CoreCancel {
    budget: SourceBudget,
    token: CancellationToken,
}
pub struct CoreSession {
    session: SourceSession,
    poisoned: bool,
}
pub struct CoreJob {
    state: JobState,
    scope: Arc<JobScope>,
    poisoned: bool,
}
struct JobScope {
    _control: BoundaryReservation,
}
enum JobState {
    Preparing(PendingSourceRequest),
    Ready(OwnedParseRequest),
    Complete(OwnedCompletedParse),
    Terminal(CoreError),
    Used,
}
pub struct CoreAllocation {
    scope: Arc<JobScope>,
    reservation: BoundaryReservation,
    requested: u64,
    admitted: bool,
}
pub struct CoreModel {
    model: AcceptedModel,
}
/// Independently sampled tracked root capacities, not SDK/OS/GPU memory or a joint peak.
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct WireBudgetUsage {
    pub header: WireHeader,
    pub source_capacity: u64,
    pub auxiliary_capacity: u64,
}
const _: () = assert!(size_of::<WireBudgetUsage>() == 24);
pub struct CoreStream {
    reader: MainReader,
    poisoned: bool,
}
enum QueryData {
    Point(PointResponse),
    Range(RangeResponse),
}
pub struct CoreQuery {
    data: QueryData,
    model: AcceptedModel,
    op: Operation,
    poisoned: bool,
}

fn profile(value: u32) -> Result<ParseConfig, i32> {
    Ok(ParseConfig::new(match value {
        1 => Profile::C0,
        2 => Profile::G3,
        _ => return Err(STATUS_INVALID_ARGUMENT),
    }))
}

fn profile_with_extensions(value: u32, extensions: u32) -> Result<ParseConfig, i32> {
    let base = profile(value)?;
    let extensions = markdown_core::config::Extensions::from_bits(extensions)
        .map_err(|_| STATUS_INVALID_ARGUMENT)?;
    Ok(ParseConfig::with_extensions(base.profile(), extensions))
}

fn key(value: WireSource) -> Result<SourceKey, CoreError> {
    SourceKey::new(value.document, value.revision)
}
fn identity(value: WireIdentity) -> Result<ModelIdentity, CoreError> {
    let config = profile_with_extensions(value.profile, value.extensions)
        .map_err(|_| CoreError::UnsupportedConfiguration)?;
    ModelIdentity::new(
        SourceKey::new(value.document, value.revision)?,
        value.generation,
        config,
    )
}
fn wire_identity(model: ModelIdentity) -> WireIdentity {
    WireIdentity {
        document: model.source().document(),
        revision: model.source().revision(),
        generation: model.generation(),
        profile: match model.config().profile() {
            Profile::C0 => 1,
            Profile::G3 => 2,
        },
        extensions: model.config().extensions().bits(),
    }
}

/// Only the initialized 8-byte prefix is read until exact ABI/size match.
unsafe fn header<T>(pointer: *const T) -> Result<(), i32> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller guarantees a readable prefix, aligned for full T.
    let prefix = unsafe { pointer.cast::<WireHeader>().read() };
    if prefix.abi_version != CORE_ABI_VERSION || prefix.struct_size as usize != size_of::<T>() {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}

fn valid_handle<T>(pointer: *const Owned<T>) -> bool {
    !pointer.is_null() && (pointer as usize).is_multiple_of(align_of::<Owned<T>>())
}

unsafe fn input<'a>(bytes: *const u8, length: u64) -> Result<&'a [u8], i32> {
    if length > isize::MAX as u64 || (bytes.is_null() && length != 0) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    if length == 0 {
        return Ok(&[]);
    }
    // SAFETY: caller guarantees a readable borrowed input, with length <= isize::MAX.
    Ok(unsafe { std::slice::from_raw_parts(bytes, length as usize) })
}

fn limit_code(kind: LimitKind) -> u32 {
    match kind {
        LimitKind::SourceBytes => 1,
        LimitKind::SourceCapacity => 2,
        LimitKind::SourceOwners => 3,
        LimitKind::ParseBytes => 4,
        LimitKind::InputPieces => 5,
        LimitKind::Edits => 6,
        LimitKind::InsertBytes => 7,
        LimitKind::AuxiliaryCapacity => 8,
        LimitKind::Work => 9,
        LimitKind::NativeRecords => 10,
        LimitKind::ModelRecords => 11,
        LimitKind::SourcePieces => 12,
        LimitKind::ProofRecords => 13,
        LimitKind::Depth => 14,
        LimitKind::DecodedBytes => 15,
        LimitKind::ReferenceLookups => 16,
        LimitKind::ReferenceBytes => 17,
        LimitKind::IndexRecords => 18,
        LimitKind::QueryRequests => 19,
        LimitKind::QueryRecords => 20,
        LimitKind::QueryCapacity => 21,
    }
}

fn wire_error(error: CoreError) -> WireError {
    let mut result = WireError::default();
    result.status = match error {
        CoreError::InvalidState => 5,
        CoreError::StaleResult => 10,
        CoreError::InvalidRevision => 12,
        CoreError::WrongDocument => 13,
        CoreError::WrongBudget => 14,
        CoreError::UnknownId => 17,
        CoreError::OutsideScope => 18,
        CoreError::InvalidRange => 16,
        CoreError::UnsupportedConfiguration => 8,
        CoreError::InvalidProvenance => 19,
        CoreError::InvalidBoundary => 15,
        CoreError::InvalidEdits => 21,
        CoreError::ArithmeticOverflow => 20,
        CoreError::OutOfMemory => 7,
        CoreError::Cancelled => 4,
        CoreError::InternalInvariant => 22,
        CoreError::DeadlineExceeded {
            elapsed_ns,
            limit_ns,
        } => {
            result.requested = elapsed_ns;
            result.limit = limit_ns;
            24
        }
        CoreError::ClockFailure => 25,
        CoreError::InvalidUtf16 { offset } => {
            result.offset = offset;
            26
        }
        CoreError::WrongRevision { expected, actual } => {
            result.expected = expected;
            result.actual = actual;
            12
        }
        CoreError::WrongGeneration { expected, actual } => {
            result.expected = expected;
            result.actual = actual;
            23
        }
        CoreError::ResyncRequired { expected, actual } => {
            result.expected = expected;
            result.actual = actual;
            11
        }
        CoreError::InvalidUtf8 { offset } => {
            result.offset = offset;
            3
        }
        CoreError::InvalidEditUtf8 { edit_index, offset } => {
            result.edit_index = edit_index;
            result.offset = offset;
            3
        }
        CoreError::ResourceLimit {
            kind,
            requested,
            limit,
        } => {
            result.limit_kind = limit_code(kind);
            result.requested = requested;
            result.limit = limit;
            9
        }
    };
    result
}

/// Preflight errors do not mutate error storage. Matching error storage is required.
unsafe fn action(error: *mut WireError, work: impl FnOnce() -> Result<(), CoreError>) -> i32 {
    // SAFETY: callers of action supply the ABI's readable/writable error prefix/full layout.
    if let Err(code) = unsafe { header(error) } {
        return code;
    }
    let mut diagnostic = WireError::default();
    let code = contain_panic(|| match work() {
        Ok(()) => 0,
        Err(fault) => {
            diagnostic = wire_error(fault);
            diagnostic.status
        }
    });
    diagnostic.status = code;
    // SAFETY: matching header commits the caller to full writable nonoverlapping error storage.
    unsafe { error.write(diagnostic) };
    code
}

macro_rules! preflight {
    ($condition:expr) => {
        if !$condition {
            return STATUS_INVALID_ARGUMENT;
        }
    };
}
macro_rules! handle_output {
    ($pointer:expr) => {{
        preflight!(!$pointer.is_null() && ($pointer as usize).is_multiple_of(align_of::<usize>()));
        // SAFETY: caller supplies writable pointer storage not overlapping other arguments.
        unsafe { $pointer.write(ptr::null_mut()) };
    }};
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_core_abi_version() -> u32 {
    CORE_ABI_VERSION
}

fn new_control_cancel(budget: SourceBudget) -> Result<*mut Owned<CoreCancel>, CoreError> {
    let mut op = budget.begin_cancellable(false)?;
    let token = op.cancellation();
    let slot = Slot::new(op.boundary_work())?;
    Ok(slot.commit(CoreCancel { budget, token }))
}

/// # Safety
/// Session may be null, otherwise live and read-only for this call. Output/error meet ABI rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_cancel_create(
    session: *const Owned<CoreSession>,
    output: *mut *mut Owned<CoreCancel>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(session.is_null() || valid_handle(session));
    // SAFETY: checked nonnull handles are live by caller obligation; action validates error first.
    unsafe {
        action(error, || {
            let budget = if session.is_null() {
                SourceBudget::task_fixture()
            } else {
                (*session).value.session.budget().clone()
            };
            if !session.is_null() && (*session).value.poisoned {
                return Err(CoreError::InvalidState);
            }
            output.write(new_control_cancel(budget)?);
            Ok(())
        })
    }
}

/// # Safety
/// A live immutable model; output/error are independent writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_model_cancel_create(
    model: *const Owned<CoreModel>,
    output: *mut *mut Owned<CoreCancel>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model));
    // SAFETY: model is live for the entire synchronous read.
    unsafe {
        action(error, || {
            let budget = (*model).value.model.budget().clone();
            output.write(new_control_cancel(budget)?);
            Ok(())
        })
    }
}

/// # Safety
/// A live shared prototype token; creates a fresh flag in the same stable root.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_cancel_create_like(
    prototype: *const Owned<CoreCancel>,
    output: *mut *mut Owned<CoreCancel>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(prototype));
    // SAFETY: prototype is immutable/live, new independent outputs are exclusively writable.
    unsafe {
        action(error, || {
            let budget = (*prototype).value.budget.clone();
            output.write(new_control_cancel(budget)?);
            Ok(())
        })
    }
}

/// # Safety
/// Live cancellation handle; concurrent calls use only shared atomic references.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_cancel_request(
    cancel: *const Owned<CoreCancel>,
) -> i32 {
    preflight!(valid_handle(cancel));
    // SAFETY: caller keeps handle alive; shared ref touches only AtomicBool.
    unsafe { (*cancel).value.token.cancel() };
    0
}

/// # Safety
/// Live shared cancel handle; output and error storage are writable and nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_session_create(
    cancel: *const Owned<CoreCancel>,
    config: u32,
    extensions: u32,
    output: *mut *mut Owned<CoreSession>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(cancel));
    let config = match profile_with_extensions(config, extensions) {
        Ok(v) => v,
        Err(code) => return code,
    };
    // SAFETY: live shared cancel; newly created session remains private until success.
    unsafe {
        action(error, || {
            let mut op = (*cancel).value.budget.begin((*cancel).value.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let session = SourceSession::transactional(
                config,
                (*cancel).value.budget.clone(),
                (*cancel).value.token.clone(),
            )?;
            output.write(slot.commit(CoreSession {
                session,
                poisoned: false,
            }));
            Ok(())
        })
    }
}

mod requests;
pub use requests::*;

/// # Safety
/// Exclusive session and a live same-root cancel handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_session_detach(
    session: *mut Owned<CoreSession>,
    cancel: *const Owned<CoreCancel>,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(session) && valid_handle(cancel));
    // SAFETY: exclusive session and separate shared cancellation state.
    let code = unsafe {
        action(error, || {
            let session = &mut (*session).value;
            let cancel = &(*cancel).value;
            if session.poisoned {
                return Err(CoreError::InvalidState);
            }
            if !cancel.budget.same_root(session.session.budget()) {
                return Err(CoreError::WrongBudget);
            }
            session
                .session
                .begin_transaction(cancel.token.clone())
                .detach()
        })
    };
    if code == STATUS_PANIC {
        unsafe {
            (*session).value.poisoned = true;
        }
    }
    code
}

/// # Safety
/// Live model; output initialized prefix/full writable POD, error independent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_model_summary(
    model: *const Owned<CoreModel>,
    output: *mut WireModelSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(model));
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: immutable model and exclusive full output/error by ABI contract.
    unsafe {
        action(error, || {
            let model = &(*model).value.model;
            let ir = model.ir();
            let main = ir
                .flows()
                .get(model.main_flow_id().index())
                .ok_or(CoreError::InternalInvariant)?;
            output.write(WireModelSummary {
                header: WireHeader::of::<WireModelSummary>(),
                identity: wire_identity(model.identity()),
                main_flow_id: model.main_flow_id().value(),
                has_footnotes: u32::from(model.has_footnotes()),
                nodes: ir.nodes().len() as u64,
                flows: ir.flows().len() as u64,
                objects: ir.objects().len() as u64,
                payloads: ir.payloads().len() as u64,
                atoms: ir.atoms_len() as u64,
                main_utf16: main.logical_utf16,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Live immutable model, initialized output prefix and independent writable error/output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_model_budget_usage(
    model: *const Owned<CoreModel>,
    output: *mut WireBudgetUsage,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(model));
    // SAFETY: the caller guarantees a readable initialized output prefix.
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: the model remains pinned; querying gauges needs no new allocation,
    // operation or cancellation authority and does not alter any first fault.
    unsafe {
        action(error, || {
            let usage = (*model).value.model.budget().usage();
            output.write(WireBudgetUsage {
                header: WireHeader::of::<WireBudgetUsage>(),
                source_capacity: usage.source_capacity,
                auxiliary_capacity: usage.auxiliary_capacity,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Live shared model/cancel handles; exclusively writable nonoverlapping output/error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_model_clone(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    output: *mut *mut Owned<CoreModel>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel));
    // SAFETY: shared handles are live, independent output is exclusive.
    unsafe {
        action(error, || {
            let model = &(*model).value.model;
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            let mut op = cancel.budget.begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            output.write(slot.commit(CoreModel {
                model: model.clone(),
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Model/cancel are live shared handles; newly created stream is returned only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_stream_create(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    output: *mut *mut Owned<CoreStream>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel));
    // SAFETY: caller keeps both handles alive during the shared call.
    unsafe {
        action(error, || {
            let model = &(*model).value.model;
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            let mut op = cancel.budget.begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let reader = MainReader::new(model.clone(), op)?;
            output.write(slot.commit(CoreStream {
                reader,
                poisoned: false,
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive stream; 4..65536 writable buffer bytes, independent initialized result/error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_stream_read(
    stream: *mut Owned<CoreStream>,
    buffer: *mut u8,
    capacity: u64,
    output: *mut WireReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(stream) && !buffer.is_null() && (4..=65_536).contains(&capacity));
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: checked finite capacity, exclusive stream and writable distinct buffer/PODs.
    let code = unsafe {
        action(error, || {
            let stream = &mut (*stream).value;
            if stream.poisoned {
                return Err(CoreError::InvalidState);
            }
            stream
                .reader
                .boundary_result(size_of::<WireReadResult>() as u64)?;
            let result = stream
                .reader
                .read_into(std::slice::from_raw_parts_mut(buffer, capacity as usize))?;
            output.write(WireReadResult {
                header: WireHeader::of::<WireReadResult>(),
                written: result.written as u64,
                next: result.utf16,
                done: u32::from(result.done),
                reserved: 0,
            });
            Ok(())
        })
    };
    if code == STATUS_PANIC {
        unsafe {
            (*stream).value.poisoned = true;
        }
    }
    code
}

macro_rules! destroy {
    ($function:ident,$value:ty) => {
        /// # Safety
        /// Null or a live uniquely owned matching handle, after all calls have ended.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $function(handle: *mut Owned<$value>) -> i32 {
            if handle.is_null() {
                return 0;
            }
            preflight!(valid_handle(handle));
            contain_panic(|| {
                // SAFETY: caller transfers the sole allocation ownership.
                unsafe { release(handle) };
                0
            })
        }
    };
}
destroy!(notepads_markdown_core_cancel_destroy, CoreCancel);
destroy!(notepads_markdown_core_session_destroy, CoreSession);
destroy!(notepads_markdown_core_job_destroy, CoreJob);
destroy!(notepads_markdown_core_allocation_destroy, CoreAllocation);
destroy!(notepads_markdown_core_model_destroy, CoreModel);
destroy!(notepads_markdown_core_stream_destroy, CoreStream);
destroy!(notepads_markdown_core_query_destroy, CoreQuery);

mod preview;
pub use preview::*;
mod outline;
pub use outline::*;

mod images;
pub use images::*;

mod highlight;
pub use highlight::*;
mod math;
pub use math::*;
mod diagram;
pub use diagram::*;

fn target(model: &AcceptedModel, kind: u32, id: u32) -> Result<TextTarget, CoreError> {
    match kind {
        1 => Ok(TextTarget::Flow(model.flow_id(id)?)),
        2 => Ok(TextTarget::Payload(model.payload_id(id)?)),
        _ => Err(CoreError::UnknownId),
    }
}
fn affinity(value: u32) -> Result<Affinity, CoreError> {
    match value {
        0 => Ok(Affinity::Before),
        1 => Ok(Affinity::After),
        _ => Err(CoreError::InvalidBoundary),
    }
}
fn point_query(model: &AcceptedModel, wire: WireQuery) -> Result<PointQuery, CoreError> {
    if wire.start != wire.end {
        return Err(CoreError::InvalidRange);
    }
    let target = target(model, wire.target_kind, wire.target_id)?;
    let input = match (wire.domain, target) {
        (1, target) => PointInput::Source {
            target,
            byte: SourceBoundary::new(wire.start),
        },
        (2, TextTarget::Flow(flow)) => PointInput::Flow {
            flow,
            utf16: LogicalBoundary::new(wire.start),
        },
        (3, TextTarget::Payload(payload)) => PointInput::Payload {
            payload,
            utf8: PayloadBoundary::new(wire.start),
        },
        _ => return Err(CoreError::InvalidBoundary),
    };
    Ok(PointQuery {
        model: identity(wire.identity)?,
        input,
        affinity: affinity(wire.affinity)?,
    })
}
fn range_query(model: &AcceptedModel, wire: WireQuery) -> Result<RangeQuery, CoreError> {
    let target = target(model, wire.target_kind, wire.target_id)?;
    let input = match (wire.domain, target) {
        (1, target) => RangeInput::Source {
            target,
            span: SourceSpan::new(wire.start, wire.end)?,
        },
        (2, TextTarget::Flow(flow)) => RangeInput::Flow {
            flow,
            span: LogicalSpan::new(wire.start, wire.end)?,
        },
        (3, TextTarget::Payload(payload)) => RangeInput::Payload {
            payload,
            span: PayloadSpan::new(wire.start, wire.end)?,
        },
        _ => return Err(CoreError::InvalidBoundary),
    };
    Ok(RangeQuery {
        model: identity(wire.identity)?,
        input,
        empty_affinity: affinity(wire.affinity)?,
    })
}

/// # Safety
/// Live shared model/cancel. Each descriptor has readable initialized prefix,
/// and after a matching prefix its full layout. count<=256; outputs do not overlap.
unsafe fn query_create(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    queries: *const WireQuery,
    count: u64,
    ranges: bool,
    output: *mut *mut Owned<CoreQuery>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel) && count <= 256);
    preflight!(
        count == 0
            || (!queries.is_null() && (queries as usize).is_multiple_of(align_of::<WireQuery>()))
    );
    // Validate every initialized prefix without constructing references to uninitialized bodies.
    for i in 0..count as usize {
        // SAFETY: caller provides the batch's readable prefixes; finite pointer arithmetic.
        if let Err(code) = unsafe { header(queries.add(i)) } {
            return code;
        }
    }
    // SAFETY: all matching descriptors commit the caller to initialized full layouts.
    unsafe {
        action(error, || {
            let model = &(*model).value.model;
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            let mut op = cancel.budget.begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let flow = model.main_flow_id();
            let data = if ranges {
                let default = RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Flow {
                        flow,
                        span: LogicalSpan::new(0, 0)?,
                    },
                    empty_affinity: Affinity::Before,
                };
                let mut batch = [default; 256];
                op.external_read(count * size_of::<WireHeader>() as u64)?;
                op.external_write(size_of_val(&batch) as u64)?;
                for (i, entry) in batch.iter_mut().take(count as usize).enumerate() {
                    op.external_events(1)?;
                    op.external_read(size_of::<WireQuery>() as u64)?;
                    let wire = queries.add(i).read();
                    op.external_write(size_of::<RangeQuery>() as u64)?;
                    *entry = range_query(model, wire).map_err(|e| op.boundary_failure(e))?;
                }
                QueryData::Range(model.map_ranges(&batch[..count as usize], &mut op)?)
            } else {
                let default = PointQuery {
                    model: model.identity(),
                    input: PointInput::Flow {
                        flow,
                        utf16: LogicalBoundary::new(0),
                    },
                    affinity: Affinity::Before,
                };
                let mut batch = [default; 256];
                op.external_read(count * size_of::<WireHeader>() as u64)?;
                op.external_write(size_of_val(&batch) as u64)?;
                for (i, entry) in batch.iter_mut().take(count as usize).enumerate() {
                    op.external_events(1)?;
                    op.external_read(size_of::<WireQuery>() as u64)?;
                    let wire = queries.add(i).read();
                    op.external_write(size_of::<PointQuery>() as u64)?;
                    *entry = point_query(model, wire).map_err(|e| op.boundary_failure(e))?;
                }
                QueryData::Point(model.map_points(&batch[..count as usize], &mut op)?)
            };
            op.checkpoint()?;
            output.write(slot.commit(CoreQuery {
                data,
                model: model.clone(),
                op,
                poisoned: false,
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Shared live model/cancel, <=256 complete matching query descriptors and independent outputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_map_points(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    queries: *const WireQuery,
    count: u64,
    output: *mut *mut Owned<CoreQuery>,
    error: *mut WireError,
) -> i32 {
    // SAFETY: forwarded explicit pointer/call ordering contract.
    unsafe { query_create(model, cancel, queries, count, false, output, error) }
}
/// # Safety
/// Shared live model/cancel, <=256 complete matching query descriptors and independent outputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_map_ranges(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    queries: *const WireQuery,
    count: u64,
    output: *mut *mut Owned<CoreQuery>,
    error: *mut WireError,
) -> i32 {
    // SAFETY: forwarded explicit pointer/call ordering contract.
    unsafe { query_create(model, cancel, queries, count, true, output, error) }
}

fn rows<T: Copy>(value: markdown_core::ir::Rows<T>) -> WireRows {
    WireRows {
        start: value.start() as u32,
        count: value.len() as u32,
    }
}
fn wire_target(value: TextTarget) -> (u32, u32) {
    match value {
        TextTarget::Flow(v) => (1, v.value()),
        TextTarget::Payload(v) => (2, v.value()),
    }
}
fn wire_direction(value: Direction) -> u32 {
    match value {
        Direction::SourceToTarget => 1,
        Direction::TargetToSource => 2,
    }
}
fn wire_effects(value: Effects) -> u32 {
    u32::from(value.atomic)
        | u32::from(value.hidden) << 1
        | u32::from(value.synthetic) << 2
        | u32::from(value.object) << 3
}
fn wire_relation(value: Correspondence) -> u32 {
    match value {
        Correspondence::Exact => 0,
        Correspondence::Atomic => 1,
        Correspondence::Hidden => 2,
        Correspondence::Synthetic => 3,
        Correspondence::Object => 4,
    }
}
fn wire_kind(value: AtomKind) -> u32 {
    match value {
        AtomKind::Copy => 0,
        AtomKind::Transform => 1,
        AtomKind::Hidden => 2,
        AtomKind::Synthetic => 3,
        AtomKind::Object => 4,
    }
}
fn wire_position(value: Position) -> WirePosition {
    WirePosition {
        domain: match value {
            Position::Source(_) => 1,
            Position::Flow(_) => 2,
            Position::Payload(_) => 3,
        },
        reserved: 0,
        value: value.value(),
    }
}
fn wire_span(value: RelatedSpan) -> WireSpan {
    WireSpan {
        domain: match value {
            RelatedSpan::Source(_) => 1,
            RelatedSpan::Flow(_) => 2,
            RelatedSpan::Payload(_) => 3,
        },
        reserved: 0,
        start: value.start(),
        end: value.end(),
    }
}
fn wire_related(value: Related) -> WireRelated {
    WireRelated {
        spans: rows(value.spans),
        anchors: rows(value.anchors),
        has_envelope: u32::from(value.envelope.is_some()),
        reserved: 0,
        envelope: value.envelope.map(wire_span).unwrap_or_default(),
    }
}
fn wire_point(value: PointResult) -> WirePoint {
    let (target_kind, target_id) = wire_target(value.target);
    WirePoint {
        target_kind,
        target_id,
        direction: wire_direction(value.direction),
        effects: wire_effects(value.effects),
        chosen: wire_position(value.chosen),
        related: wire_related(value.related),
        correspondence: wire_relation(value.correspondence),
        reserved: 0,
        contributors: rows(value.contributors),
    }
}
fn wire_range(value: RangeResult) -> WireRange {
    match value {
        RangeResult::Boundary(v) => WireRange {
            kind: 1,
            reserved: 0,
            rows: rows(v),
        },
        RangeResult::Selection(v) => WireRange {
            kind: 2,
            reserved: 0,
            rows: rows(v),
        },
    }
}
fn wire_selection(value: Selection) -> WireSelection {
    let (target_kind, target_id) = wire_target(value.target);
    WireSelection {
        target_kind,
        target_id,
        direction: wire_direction(value.direction),
        effects: wire_effects(value.effects),
        expanded: u32::from(value.expanded),
        reserved: 0,
        start: wire_point(value.start),
        end: wire_point(value.end),
        related: wire_related(value.related),
        contributions: rows(value.contributions),
    }
}
fn wire_contributor(value: Contributor) -> WireContributor {
    WireContributor {
        atom: value.atom.value(),
        source_part: value.source_part,
        kind: wire_kind(value.kind),
        correspondence: wire_relation(value.relation),
        source_start: value.source.start(),
        source_end: value.source.end(),
        target_start: value.target.start,
        target_end: value.target.end,
    }
}
fn wire_contribution(value: Contribution) -> WireContribution {
    WireContribution {
        atom: value.atom.value(),
        source_part: value.source_part,
        kind: wire_kind(value.kind),
        expanded: u32::from(value.expanded),
        source_start: value.source.start(),
        source_end: value.source.end(),
        target_start: value.target.start,
        target_end: value.target.end,
    }
}

fn summary<T: Copy>(
    data: &QueryResponse<T>,
    model: &AcceptedModel,
    kind: u32,
    points: usize,
    ranges: usize,
) -> WireQuerySummary {
    WireQuerySummary {
        header: WireHeader::of::<WireQuerySummary>(),
        identity: wire_identity(model.identity()),
        kind,
        reserved: 0,
        points: points as u64,
        ranges: ranges as u64,
        selections: data.selections().len() as u64,
        spans: data.spans().len() as u64,
        anchors: data.anchors().len() as u64,
        contributors: data.contributors().len() as u64,
        contributions: data.contributions().len() as u64,
    }
}
/// # Safety
/// Exclusive live query; independent initialized output/error prefixes and full writable layouts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_query_summary(
    query: *mut Owned<CoreQuery>,
    output: *mut WireQuerySummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(query));
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: exclusive mutable operation counters, immutable owned response.
    let code = unsafe {
        action(error, || {
            let query = &mut (*query).value;
            if query.poisoned {
                return Err(CoreError::InvalidState);
            }
            query.op.checkpoint()?;
            query
                .op
                .external_write(size_of::<WireQuerySummary>() as u64)?;
            let result = match &query.data {
                QueryData::Point(v) => summary(v, &query.model, 1, v.results().len(), 0),
                QueryData::Range(v) => {
                    summary(v, &query.model, 2, v.boundaries().len(), v.results().len())
                }
            };
            output.write(result);
            Ok(())
        })
    };
    if code == STATUS_PANIC {
        unsafe {
            (*query).value.poisoned = true;
        }
    }
    code
}

unsafe fn export_rows<A: Copy, B>(
    values: &[A],
    start: u64,
    count: u64,
    buffer: *mut u8,
    convert: fn(A) -> B,
    op: &mut Operation,
) -> Result<WireReadResult, CoreError> {
    op.checkpoint()?;
    let start =
        usize::try_from(start).map_err(|_| op.boundary_failure(CoreError::ArithmeticOverflow))?;
    let count =
        usize::try_from(count).map_err(|_| op.boundary_failure(CoreError::ArithmeticOverflow))?;
    let end = start
        .checked_add(count)
        .ok_or_else(|| op.boundary_failure(CoreError::ArithmeticOverflow))?;
    let input = values
        .get(start..end)
        .ok_or_else(|| op.boundary_failure(CoreError::InvalidRange))?;
    for (i, value) in input.iter().enumerate() {
        op.external_events(1)?;
        op.external_read(size_of::<A>() as u64)?;
        op.external_write(size_of::<B>() as u64)?;
        // SAFETY: caller provided count fully writable B slots of checked size/alignment.
        unsafe { buffer.cast::<B>().add(i).write(convert(*value)) };
    }
    op.checkpoint()?;
    Ok(WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        written: count as u64,
        next: end as u64,
        done: u32::from(end == values.len()),
        reserved: 0,
    })
}

unsafe fn read_query<T: Copy>(
    data: &QueryResponse<T>,
    table: u32,
    start: u64,
    count: u64,
    buffer: *mut u8,
    op: &mut Operation,
) -> Result<WireReadResult, CoreError> {
    // SAFETY: caller already validated the table row size, finite count and aligned writable memory.
    unsafe {
        match table {
            3 => export_rows(data.selections(), start, count, buffer, wire_selection, op),
            4 => export_rows(data.spans(), start, count, buffer, wire_span, op),
            5 => export_rows(data.anchors(), start, count, buffer, wire_position, op),
            6 => export_rows(
                data.contributors(),
                start,
                count,
                buffer,
                wire_contributor,
                op,
            ),
            7 => export_rows(
                data.contributions(),
                start,
                count,
                buffer,
                wire_contribution,
                op,
            ),
            _ => Err(op.boundary_failure(CoreError::InvalidRange)),
        }
    }
}

/// # Safety
/// Exclusive live query; table rows use exact wire row_size, buffer aligned8 and
/// writable for count*row_size bytes; <=256 rows. All output regions distinct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_query_read(
    query: *mut Owned<CoreQuery>,
    table: u32,
    start: u64,
    count: u64,
    row_size: u32,
    buffer: *mut u8,
    output: *mut WireReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(query) && count <= 256);
    let expected = match table {
        1 => size_of::<WirePoint>(),
        2 => size_of::<WireRange>(),
        3 => size_of::<WireSelection>(),
        4 => size_of::<WireSpan>(),
        5 => size_of::<WirePosition>(),
        6 => size_of::<WireContributor>(),
        7 => size_of::<WireContribution>(),
        _ => return STATUS_INVALID_ARGUMENT,
    };
    preflight!(row_size as usize == expected);
    preflight!(count == 0 || (!buffer.is_null() && (buffer as usize).is_multiple_of(8)));
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: exact finite aligned table storage; query/operation uniquely borrowed.
    let code = unsafe {
        action(error, || {
            let query = &mut (*query).value;
            if query.poisoned {
                return Err(CoreError::InvalidState);
            }
            query
                .op
                .external_write(size_of::<WireReadResult>() as u64)?;
            let result = match &query.data {
                QueryData::Point(v) => {
                    if table == 1 {
                        export_rows(v.results(), start, count, buffer, wire_point, &mut query.op)
                    } else if table == 2 {
                        export_rows(
                            &[] as &[RangeResult],
                            start,
                            count,
                            buffer,
                            wire_range,
                            &mut query.op,
                        )
                    } else {
                        read_query(v, table, start, count, buffer, &mut query.op)
                    }
                }
                QueryData::Range(v) => {
                    if table == 1 {
                        export_rows(
                            v.boundaries(),
                            start,
                            count,
                            buffer,
                            wire_point,
                            &mut query.op,
                        )
                    } else if table == 2 {
                        export_rows(v.results(), start, count, buffer, wire_range, &mut query.op)
                    } else {
                        read_query(v, table, start, count, buffer, &mut query.op)
                    }
                }
            }?;
            output.write(result);
            Ok(())
        })
    };
    if code == STATUS_PANIC {
        unsafe {
            (*query).value.poisoned = true;
        }
    }
    code
}

#[cfg(test)]
mod tests;
