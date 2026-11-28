// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! ABI3 source requests. Each running state owns the same original operation.

use super::*;
use markdown_core::session::SourceTransaction;
use markdown_core::source::{Edit, EditBatch};

#[cfg(test)]
thread_local! {
    // Deterministic correctness control on the real source-commit/capture path.
    // The production build has no hook or alternate source implementation.
    static CANCEL_AFTER_SOURCE: std::cell::RefCell<Option<CancellationToken>> = const { std::cell::RefCell::new(None) };
}

impl CoreJob {
    fn boundary_work(&mut self) -> Result<BoundaryWork<'_>, CoreError> {
        if self.poisoned {
            return Err(CoreError::InvalidState);
        }
        match &mut self.state {
            JobState::Preparing(value) => Ok(value.boundary_work()),
            JobState::Ready(value) => Ok(value.boundary_work()),
            JobState::Complete(value) => Ok(value.boundary_work()),
            JobState::Terminal(error) => Err(*error),
            JobState::Used => Err(CoreError::InvalidState),
        }
    }

    fn terminal(&mut self, error: CoreError) -> CoreError {
        if matches!(self.state, JobState::Used) {
            return error;
        }
        let error = match self.boundary_work() {
            Ok(work) => work.first_fault().unwrap_or(error),
            Err(first) => first,
        };
        self.state = JobState::Terminal(error);
        error
    }
}

/// # Safety
/// The same exclusive live job remains owned through caught unwinding.
unsafe fn job_action(
    job: *mut Owned<CoreJob>,
    error: *mut WireError,
    body: impl FnOnce() -> Result<(), CoreError>,
) -> i32 {
    // SAFETY: action validates error storage before entering the body.
    let code = unsafe { action(error, body) };
    if code == STATUS_PANIC {
        // SAFETY: caller retains the exclusive live job after catch_unwind.
        unsafe {
            (*job).value.poisoned = true;
        }
    }
    code
}

/// # Safety
/// Exclusive live session; distinct writable outputs/error, no overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_session_start_request(
    session: *mut Owned<CoreSession>,
    initially_cancelled: u32,
    output: *mut *mut Owned<CoreJob>,
    cancel_output: *mut *mut Owned<CoreCancel>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    handle_output!(cancel_output);
    preflight!(valid_handle(session) && initially_cancelled <= 1);
    // SAFETY: caller grants exclusive session and nonoverlapping live outputs.
    let code = unsafe {
        action(error, || {
            let session = &mut (*session).value;
            if session.poisoned {
                return Err(CoreError::InvalidState);
            }
            let mut pending = session.session.start_request(initially_cancelled != 0)?;
            let token = pending.cancellation();
            let budget = session.session.budget().clone();
            let bytes = size_of::<JobScope>() + 2 * size_of::<usize>() + align_of::<JobScope>() - 1;
            let mut work = pending.boundary_work();
            let lease = work.reserve_boundary(bytes as u64)?;
            work.external_write(bytes as u64)?;
            work.checkpoint_now()?;
            let scope = Arc::new(JobScope { _control: lease });
            work.checkpoint_now()?;
            let cancel_slot = Slot::new(pending.boundary_work())?;
            let job_slot = Slot::new(pending.boundary_work())?;
            pending.boundary_work().checkpoint_now()?;
            // Both allocations have succeeded. Neither placement can fail.
            let cancel = cancel_slot.commit(CoreCancel { budget, token });
            let job = job_slot.commit(CoreJob {
                state: JobState::Preparing(pending),
                scope,
                poisoned: false,
            });
            cancel_output.write(cancel);
            output.write(job);
            Ok(())
        })
    };
    if code == STATUS_PANIC {
        // SAFETY: the exclusive live session remains owned after containment.
        unsafe { (*session).value.poisoned = true };
    }
    code
}

fn no_state() -> WireSubmitResult {
    WireSubmitResult {
        header: WireHeader::of::<WireSubmitResult>(),
        ..Default::default()
    }
}

fn source_state(
    session: &SourceSession,
    accepted: bool,
    ready: Option<ModelIdentity>,
) -> WireSubmitResult {
    let source = session.snapshot();
    WireSubmitResult {
        header: WireHeader::of::<WireSubmitResult>(),
        source: source
            .as_ref()
            .map_or(WireSource::default(), |source| WireSource {
                document: source.key().document(),
                revision: source.key().revision(),
            }),
        state_valid: 1,
        has_source: u32::from(source.is_some()),
        needs_resync: u32::from(session.needs_resync()),
        source_accepted: u32::from(accepted),
        has_ready: u32::from(ready.is_some()),
        reserved: 0,
        ready: ready.map_or(WireIdentity::default(), wire_identity),
    }
}

/// Inputs/result preflight is completed by the exported entry before this call.
unsafe fn prepare(
    session: *mut Owned<CoreSession>,
    job: *mut Owned<CoreJob>,
    result: *mut WireSubmitResult,
    error: *mut WireError,
    source_action: bool,
    submit: impl FnOnce(&mut SourceTransaction<'_>) -> Result<(), CoreError>,
) -> i32 {
    // SAFETY: exported entry checked result prefix and caller guarantees full storage.
    let code = unsafe {
        action(error, || {
            result.write(no_state()); // Fixed failure diagnostic, never source authority.
            let session = &mut (*session).value;
            let job = &mut (*job).value;
            if session.poisoned || job.poisoned {
                return Err(job.terminal(CoreError::InvalidState));
            }
            let pending = match std::mem::replace(&mut job.state, JobState::Used) {
                JobState::Preparing(value) => value,
                JobState::Terminal(first) => {
                    job.state = JobState::Terminal(first);
                    return Err(first);
                }
                JobState::Used => return Err(CoreError::InvalidState),
                _ => {
                    job.state = JobState::Terminal(CoreError::InvalidState);
                    return Err(CoreError::InvalidState);
                }
            };
            let mut pending = pending;
            let matching_scope = pending.matches_scope(&session.session);
            let prepaid = {
                let mut work = pending.boundary_work();
                work.external_write(size_of::<WireSubmitResult>() as u64)
                    .and_then(|()| work.external_read(128))
            };
            if let Err(first) = prepaid {
                job.state = JobState::Terminal(first);
                return Err(first);
            }
            let mut accepted = false;
            let outcome = (|| {
                let mut transaction = pending.bind(&mut session.session)?;
                submit(&mut transaction)
                    .map_err(|error| transaction.boundary_work().boundary_failure(error))?;
                accepted = source_action;
                #[cfg(test)]
                CANCEL_AFTER_SOURCE.with(|flag| {
                    if let Some(flag) = flag.borrow_mut().take() {
                        flag.cancel();
                    }
                });
                transaction.capture()
            })();
            let identity = outcome.as_ref().ok().map(OwnedParseRequest::identity);
            if matching_scope {
                result.write(source_state(&session.session, accepted, identity));
            }
            match outcome {
                Ok(request) => {
                    job.state = JobState::Ready(request);
                    Ok(())
                }
                Err(first) => {
                    job.state = JobState::Terminal(first);
                    Err(first)
                }
            }
        })
    };
    if code == STATUS_PANIC {
        // SAFETY: exclusive handles remain live after catch_unwind.
        unsafe {
            (*session).value.poisoned = true;
            (*job).value.poisoned = true;
        }
    }
    code
}

unsafe fn prepare_preflight(
    session: *mut Owned<CoreSession>,
    job: *mut Owned<CoreJob>,
    result: *mut WireSubmitResult,
) -> Result<(), i32> {
    if !valid_handle(session) || !valid_handle(job) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller supplies a readable result prefix.
    unsafe { header(result) }
}

/// # Safety
/// Exclusive session/job, borrowed UTF8 input and descriptor, independent result/error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_replace_utf8(
    session: *mut Owned<CoreSession>,
    job: *mut Owned<CoreJob>,
    input_bytes: *const u8,
    length: u64,
    descriptor: *const WireSnapshot,
    extensions: u32,
    result: *mut WireSubmitResult,
    error: *mut WireError,
) -> i32 {
    // SAFETY: prefix reads only; input provenance is the caller's obligation.
    unsafe {
        if let Err(code) = prepare_preflight(session, job, result).and_then(|()| header(descriptor))
        {
            return code;
        }
        let descriptor = descriptor.read();
        preflight!(descriptor.reserved == 0);
        let config = match profile_with_extensions(descriptor.profile, extensions) {
            Ok(config) => config,
            Err(code) => return code,
        };
        let input = match input(input_bytes, length) {
            Ok(value) => value,
            Err(code) => return code,
        };
        prepare(session, job, result, error, true, |transaction| {
            let source = key(descriptor.source)?;
            transaction
                .resync_with_config(source, input, config)
                .map(|_| ())
        })
    }
}

/// # Safety
/// Exclusive session/job, aligned UTF16 input of the specified number of units, and disjoint descriptor/result/error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_replace_utf16(
    session: *mut Owned<CoreSession>,
    job: *mut Owned<CoreJob>,
    input_units: *const u16,
    units: u64,
    descriptor: *const WireSnapshot,
    extensions: u32,
    result: *mut WireSubmitResult,
    error: *mut WireError,
) -> i32 {
    // SAFETY: checked readable prefixes and caller-owned input span.
    unsafe {
        if let Err(code) = prepare_preflight(session, job, result).and_then(|()| header(descriptor))
        {
            return code;
        }
        preflight!(units <= isize::MAX as u64 / 2);
        preflight!(
            (units == 0 || !input_units.is_null())
                && (input_units as usize).is_multiple_of(align_of::<u16>())
        );
        let descriptor = descriptor.read();
        preflight!(descriptor.reserved == 0);
        let config = match profile_with_extensions(descriptor.profile, extensions) {
            Ok(config) => config,
            Err(code) => return code,
        };
        let input = if units == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(input_units, units as usize)
        };
        prepare(session, job, result, error, true, |transaction| {
            transaction
                .resync_utf16_with_config(key(descriptor.source)?, input, config)
                .map(|_| ())
        })
    }
}

struct EditScratch<'a> {
    values: Vec<Edit<'a>>,
    _lease: BoundaryReservation,
}

/// # Safety
/// A checked borrowed rows span and blob. Scratch owns no input beyond synchronous return.
unsafe fn edits<'a>(
    descriptor: WireEditBatch,
    rows: *const WireEditRow,
    blob: &'a [u8],
    work: &mut BoundaryWork<'_>,
) -> Result<EditScratch<'a>, CoreError> {
    let limit = work.limits().max_edits;
    if descriptor.count > limit {
        return Err(work.boundary_failure(CoreError::ResourceLimit {
            kind: LimitKind::Edits,
            requested: descriptor.count,
            limit,
        }));
    }
    let count = usize::try_from(descriptor.count)
        .map_err(|_| work.boundary_failure(CoreError::ArithmeticOverflow))?;
    let bytes = count
        .checked_mul(size_of::<Edit<'_>>())
        .ok_or_else(|| work.boundary_failure(CoreError::ArithmeticOverflow))?;
    let mut lease = work.reserve_boundary(bytes as u64)?;
    let mut values = Vec::new();
    work.checkpoint_now()?;
    values
        .try_reserve_exact(count)
        .map_err(|_| work.boundary_failure(CoreError::OutOfMemory))?;
    let actual = values
        .capacity()
        .checked_mul(size_of::<Edit<'_>>())
        .ok_or_else(|| work.boundary_failure(CoreError::ArithmeticOverflow))?;
    work.admit_actual(&mut lease, actual as u64)?;
    let mut scratch = EditScratch {
        values,
        _lease: lease,
    };
    for index in 0..count {
        work.external_read(size_of::<WireEditRow>() as u64)?;
        work.external_write(size_of::<Edit<'_>>() as u64)?;
        work.external_events(1)?;
        // SAFETY: checked count/row_size span and caller's borrow guarantee.
        let row = unsafe { rows.add(index).read() };
        let end = row
            .replacement_offset
            .checked_add(row.replacement_length)
            .filter(|&end| end <= blob.len() as u64)
            .ok_or_else(|| work.boundary_failure(CoreError::InvalidRange))?;
        let range =
            SourceSpan::new(row.start, row.end).map_err(|error| work.boundary_failure(error))?;
        scratch.values.push(Edit {
            range,
            replacement: &blob[row.replacement_offset as usize..end as usize],
        });
    }
    work.checkpoint_now()?;
    Ok(scratch)
}

/// # Safety
/// Exclusive session/job; descriptor/rows/blob borrow only until return; output independent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_apply(
    session: *mut Owned<CoreSession>,
    job: *mut Owned<CoreJob>,
    descriptor: *const WireEditBatch,
    rows: *const WireEditRow,
    replacement_bytes: *const u8,
    replacement_length: u64,
    result: *mut WireSubmitResult,
    error: *mut WireError,
) -> i32 {
    // SAFETY: caller supplies valid prefixes; payload span arithmetic checked before slice creation.
    unsafe {
        if let Err(code) = prepare_preflight(session, job, result).and_then(|()| header(descriptor))
        {
            return code;
        }
        let descriptor = descriptor.read();
        preflight!(
            descriptor.reserved == 0 && descriptor.row_size as usize == size_of::<WireEditRow>()
        );
        preflight!(descriptor.count <= isize::MAX as u64 / size_of::<WireEditRow>() as u64);
        preflight!(
            (descriptor.count == 0 || !rows.is_null())
                && (rows as usize).is_multiple_of(align_of::<WireEditRow>())
        );
        let blob = match input(replacement_bytes, replacement_length) {
            Ok(value) => value,
            Err(code) => return code,
        };
        prepare(session, job, result, error, true, |transaction| {
            let scratch = edits(descriptor, rows, blob, &mut transaction.boundary_work())?;
            transaction
                .apply(EditBatch {
                    before: key(descriptor.before)?,
                    after: key(descriptor.after)?,
                    edits: &scratch.values,
                })
                .map(|_| ())
        })
    }
}

/// # Safety
/// Exclusive live session/job and checked independent result/error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_capture_current(
    session: *mut Owned<CoreSession>,
    job: *mut Owned<CoreJob>,
    result: *mut WireSubmitResult,
    error: *mut WireError,
) -> i32 {
    // SAFETY: no payload beyond the checked result prefix.
    unsafe {
        if let Err(code) = prepare_preflight(session, job, result) {
            return code;
        }
        prepare(session, job, result, error, false, |_| Ok(()))
    }
}

/// # Safety
/// Exclusive job. Cancellation acts only through its separate shared flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_run(
    job: *mut Owned<CoreJob>,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(job));
    // SAFETY: the caller grants a unique borrow for the whole parse.
    let code = unsafe {
        action(error, || {
            let job = &mut (*job).value;
            if job.poisoned {
                return Err(CoreError::InvalidState);
            }
            let state = std::mem::replace(&mut job.state, JobState::Used);
            match state {
                JobState::Ready(request) => match request.execute() {
                    Ok(completed) => {
                        job.state = JobState::Complete(completed);
                        Ok(())
                    }
                    Err(first) => {
                        job.state = JobState::Terminal(first);
                        Err(first)
                    }
                },
                JobState::Terminal(first) => {
                    job.state = JobState::Terminal(first);
                    Err(first)
                }
                JobState::Used => Err(CoreError::InvalidState),
                _ => {
                    job.state = JobState::Terminal(CoreError::InvalidState);
                    Err(CoreError::InvalidState)
                }
            }
        })
    };
    if code == STATUS_PANIC {
        unsafe {
            (*job).value.poisoned = true;
        }
    }
    code
}

/// # Safety
/// Distinct exclusive session/job. Output Slot is allocated before model installation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_install(
    session: *mut Owned<CoreSession>,
    job: *mut Owned<CoreJob>,
    output: *mut *mut Owned<CoreModel>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(session) && valid_handle(job));
    // SAFETY: caller guarantees nonoverlapping live handles/output/error.
    let code = unsafe {
        action(error, || {
            let session = &mut (*session).value;
            let job = &mut (*job).value;
            if session.poisoned || job.poisoned {
                return Err(job.terminal(CoreError::InvalidState));
            }
            let state = std::mem::replace(&mut job.state, JobState::Used);
            let mut completed = match state {
                JobState::Complete(value) => value,
                JobState::Terminal(first) => {
                    job.state = JobState::Terminal(first);
                    return Err(first);
                }
                JobState::Used => return Err(CoreError::InvalidState),
                _ => {
                    job.state = JobState::Terminal(CoreError::InvalidState);
                    return Err(CoreError::InvalidState);
                }
            };
            let slot = match Slot::new(completed.boundary_work()) {
                Ok(slot) => slot,
                Err(first) => {
                    job.state = JobState::Terminal(first);
                    return Err(first);
                }
            };
            match session.session.install_owned(completed) {
                Ok(model) => {
                    output.write(slot.commit(CoreModel { model }));
                    Ok(())
                }
                Err(first) => {
                    job.state = JobState::Terminal(first);
                    Err(first)
                }
            }
        })
    };
    if code == STATUS_PANIC {
        unsafe {
            (*session).value.poisoned = true;
            (*job).value.poisoned = true;
        }
    }
    code
}

/// # Safety
/// Exclusive job and independent handle output. The caller frees native memory before destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_reserve_allocation(
    job: *mut Owned<CoreJob>,
    requested: u64,
    output: *mut *mut Owned<CoreAllocation>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(job));
    // SAFETY: unique job and output, allocation ownership transfers only on success.
    unsafe {
        job_action(job, error, || {
            let job = &mut (*job).value;
            let scope = job.scope.clone();
            let outcome = (|| {
                let mut work = job.boundary_work()?;
                work.checkpoint_now()?;
                if requested == 0 {
                    return Err(work.boundary_failure(CoreError::InvalidRange));
                }
                let reservation = work.reserve_boundary(requested)?;
                let slot = Slot::new(work)?;
                Ok(slot.commit(CoreAllocation {
                    scope,
                    reservation,
                    requested,
                    admitted: false,
                }))
            })();
            match outcome {
                Ok(value) => {
                    output.write(value);
                    Ok(())
                }
                Err(first) => Err(job.terminal(first)),
            }
        })
    }
}

/// # Safety
/// Unique live job/allocation, same scope; the allocation remains caller-owned on every error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_admit_allocation(
    job: *mut Owned<CoreJob>,
    allocation: *mut Owned<CoreAllocation>,
    actual: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(job) && valid_handle(allocation));
    // SAFETY: independent unique handles; no native memory ownership is taken here.
    unsafe {
        job_action(job, error, || {
            let job = &mut (*job).value;
            let allocation = &mut (*allocation).value;
            let matching = Arc::ptr_eq(&job.scope, &allocation.scope);
            let outcome = (|| {
                let mut work = job.boundary_work()?;
                work.checkpoint_now()?;
                if !matching {
                    return Err(work.boundary_failure(CoreError::OutsideScope));
                }
                if allocation.admitted {
                    return Err(work.boundary_failure(CoreError::InvalidState));
                }
                if actual < allocation.requested {
                    return Err(work.boundary_failure(CoreError::InvalidRange));
                }
                work.admit_actual(&mut allocation.reservation, actual)?;
                allocation.admitted = true;
                Ok(())
            })();
            outcome.map_err(|first| job.terminal(first))
        })
    }
}

/// # Safety
/// Exclusive job; called only after a real native allocation failure, with its requested capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_core_job_allocation_failed(
    job: *mut Owned<CoreJob>,
    requested: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(job));
    // SAFETY: unique job; fixed diagnostic only, no new allocation/operation.
    unsafe {
        job_action(job, error, || {
            let job = &mut (*job).value;
            let first = match job.boundary_work() {
                Ok(mut work) => {
                    let error = work.checkpoint_now().err().unwrap_or(if requested == 0 {
                        CoreError::InvalidRange
                    } else {
                        CoreError::OutOfMemory
                    });
                    work.boundary_failure(error)
                }
                Err(first) => first,
            };
            Err(job.terminal(first))
        })
    }
}

#[cfg(test)]
mod tests;
