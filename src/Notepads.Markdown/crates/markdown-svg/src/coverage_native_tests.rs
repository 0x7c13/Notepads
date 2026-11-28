// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Independent tests of the production Rust-to-Native serialization boundary.
//! The extern callback is a finite mock, not DWrite/WIC or an OwnedJob renderer.
//! A rejected result invalidates scratch A8; publication remains the renderer's
//! responsibility. Expected command bits below are handwritten fixture data.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};
use resvg::{CoverageProvider, CoverageRequest};
use tiny_skia::{FillRule, PathBuilder};

use crate::coverage::{Batch, Callbacks, Command, NativeCoverageProvider, Output, Path};

const SESSION: u64 = 0x1234_5678_9abc_def0;
const DEVICE_REMOVED: i32 = 0x887a_0005u32 as i32;
const CLEANUP_HR: i32 = 0x8007_0006u32 as i32;
const HARNESS_HR: i32 = 0x8000_4005u32 as i32;
const MATRIX: [u32; 6] = [
    0x3fc0_0000,
    0x3e80_0000,
    0xbe00_0000,
    0x4000_0000,
    0xc188_0000,
    0x41bc_0000,
];

fn invariant() -> Error {
    Error::new(FailureKind::Invariant, LimitKind::None)
}

#[derive(Default)]
struct Account {
    state: Mutex<AccountState>,
}

#[derive(Default)]
struct AccountState {
    next: u32,
    live: BTreeMap<u32, (ResourceKind, usize, bool)>,
    allocations: Vec<(ResourceKind, usize)>,
    checkpoints: Vec<u64>,
    releases: usize,
    cancel: bool,
    fail_release: bool,
}

struct TestBackend(Arc<Account>);

impl Backend for TestBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let mut state = self.0.state.lock().map_err(|_| invariant())?;
        state.next = state.next.checked_add(1).ok_or_else(invariant)?;
        let slot = state.next;
        if state.live.insert(slot, (kind, bytes, false)).is_some() {
            return Err(invariant());
        }
        state.allocations.push((kind, bytes));
        Ticket::new(SESSION, slot, 1)
    }

    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let mut state = self.0.state.lock().map_err(|_| invariant())?;
        let (_, reserved, committed) = state.live.get_mut(&ticket.slot()).ok_or_else(invariant)?;
        if ticket.account_nonce() != SESSION
            || ticket.generation() != 1
            || bytes != *reserved
            || *committed
        {
            return Err(invariant());
        }
        *committed = true;
        Ok(())
    }

    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let mut state = self.0.state.lock().map_err(|_| invariant())?;
        if ticket.account_nonce() != SESSION || state.live.remove(&ticket.slot()).is_none() {
            return Err(invariant());
        }
        state.releases += 1;
        if state.fail_release {
            Err(Error::native(CLEANUP_HR))
        } else {
            Ok(())
        }
    }

    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        let mut state = self.0.state.lock().map_err(|_| invariant())?;
        state.checkpoints.push(work);
        if state.cancel {
            Err(Error::new(FailureKind::Cancelled, LimitKind::None))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Malformed {
    Partial,
    NotCompleted,
    UnknownReason,
    OverlappingMasks,
    Reserved,
    UnusedReason,
    WrongSize,
    WrongVersion,
}

#[derive(Clone, Copy)]
enum Mode {
    Ready,
    LastIneligible,
    Fail {
        hr: i32,
        first: Option<Error>,
        cancel: bool,
        cleanup: bool,
    },
    CancelAfterSuccess,
    Malformed(Malformed),
}

struct Snapshot {
    session: u64,
    header: [u32; 4],
    addresses: [usize; 4],
    capacity: u64,
    paths: Vec<Path>,
    commands: Vec<Command>,
    initial: Output,
    padding: [u32; 3],
    live_at_call: Vec<(ResourceKind, usize, bool)>,
    work_at_call: Vec<u64>,
}

struct Control {
    mode: Mode,
    account: Arc<Account>,
    first: FirstFailureRef,
    snapshots: Vec<Snapshot>,
    issue: Option<&'static str>,
}

thread_local! {
    // Each parallel test has its own callback mode. No ResourcesRef is retained
    // here; even these diagnostic first-slot/account clones are explicitly taken.
    static CONTROL: RefCell<Option<Control>> = const { RefCell::new(None) };
}

/// No test assertion, unwrap, or panic is allowed to cross extern "C". Report a
/// harness error as HRESULT and assert the recorded diagnostics after returning.
unsafe extern "C" fn callback(session: u64, batch: *const Batch, output: *mut Output) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| {
        match CONTROL.try_with(|cell| {
            let Ok(mut slot) = cell.try_borrow_mut() else {
                return HARNESS_HR;
            };
            let Some(control) = slot.as_mut() else {
                return HARNESS_HR;
            };
            match unsafe { callback_body(control, session, batch, output) } {
                Ok(hr) => hr,
                Err(issue) => {
                    control.issue = Some(issue);
                    HARNESS_HR
                }
            }
        }) {
            Ok(hr) => hr,
            Err(_) => HARNESS_HR,
        }
    })) {
        Ok(hr) => hr,
        Err(_) => HARNESS_HR,
    }
}

unsafe fn callback_body(
    control: &mut Control,
    session: u64,
    batch: *const Batch,
    output: *mut Output,
) -> Result<i32, &'static str> {
    if batch.is_null() || output.is_null() {
        return Err("null callback header");
    }
    // SAFETY: the production synchronous adapter owns these complete headers.
    let input = unsafe { &*batch };
    if input.size != 48 || input.version != 1 || input.path_count > 32 || input.command_count > 4096
    {
        return Err("unexpected callback header");
    }
    let paths = if input.path_count == 0 {
        if !input.paths.is_null() {
            return Err("non-null empty paths");
        }
        &[][..]
    } else {
        if input.paths.is_null() {
            return Err("null nonempty paths");
        }
        // SAFETY: the adapter keeps exactly path_count initialized wire paths.
        unsafe { std::slice::from_raw_parts(input.paths, input.path_count as usize) }
    };
    let commands = if input.command_count == 0 {
        if !input.commands.is_null() {
            return Err("non-null empty commands");
        }
        &[][..]
    } else {
        if input.commands.is_null() {
            return Err("null nonempty commands");
        }
        // SAFETY: the adapter keeps command_count initialized wire commands.
        unsafe { std::slice::from_raw_parts(input.commands, input.command_count as usize) }
    };
    #[cfg(target_pointer_width = "32")]
    let padding = [
        input.paths_padding,
        input.commands_padding,
        input.a8_padding,
    ];
    #[cfg(target_pointer_width = "64")]
    let padding = [0; 3];
    let (live_at_call, work_at_call) = {
        let state = control.account.state.lock().map_err(|_| "account lock")?;
        (
            state.live.values().copied().collect(),
            state.checkpoints.clone(),
        )
    };
    control.snapshots.push(Snapshot {
        session,
        header: [
            input.size,
            input.version,
            input.path_count,
            input.command_count,
        ],
        addresses: [
            input.paths as usize,
            input.commands as usize,
            input.a8 as usize,
            output as usize,
        ],
        capacity: input.a8_capacity,
        paths: paths.to_vec(),
        commands: commands.to_vec(),
        // SAFETY: the adapter initialized every field of this output header.
        initial: unsafe { *output },
        padding,
        live_at_call,
        work_at_call,
    });

    if let Mode::Fail {
        hr,
        first,
        cancel,
        cleanup,
    } = control.mode
    {
        if let Some(error) = first {
            control.first.record(error);
        }
        let mut state = control.account.state.lock().map_err(|_| "account lock")?;
        state.cancel = cancel;
        state.fail_release = cleanup;
        return Ok(hr);
    }

    let capacity = usize::try_from(input.a8_capacity).map_err(|_| "arena capacity")?;
    let arena = if capacity == 0 {
        if !input.a8.is_null() {
            return Err("non-null empty arena");
        }
        &mut [][..]
    } else {
        if input.a8.is_null() {
            return Err("null nonempty arena");
        }
        // SAFETY: the adapter lends its exclusive mutable arena for this call.
        unsafe { std::slice::from_raw_parts_mut(input.a8, capacity) }
    };
    for (index, path) in paths.iter().enumerate() {
        if matches!(control.mode, Mode::LastIneligible) && index == 31 {
            continue;
        }
        let start = usize::try_from(path.a8_offset).map_err(|_| "offset")?;
        let stride = usize::try_from(path.stride).map_err(|_| "stride")?;
        for row in 0..path.height as usize {
            let row_start = row
                .checked_mul(stride)
                .and_then(|n| start.checked_add(n))
                .ok_or("row overflow")?;
            for column in 0..path.width as usize {
                let offset = row_start.checked_add(column).ok_or("column overflow")?;
                *arena.get_mut(offset).ok_or("pixel outside arena")? = 0x40 + index as u8;
            }
        }
    }
    let mut result = Output {
        size: 152,
        version: 1,
        ready_mask: match input.path_count {
            32 => u32::MAX,
            count => (1u32 << count) - 1,
        },
        completed: 1,
        ..Output::default()
    };
    match control.mode {
        Mode::LastIneligible => {
            result.ready_mask &= 0x7fff_ffff;
            result.ineligible_mask = 0x8000_0000;
            result.reasons[31] = 3;
        }
        Mode::CancelAfterSuccess => {
            let mut state = control.account.state.lock().map_err(|_| "account lock")?;
            state.cancel = true;
            state.fail_release = true;
        }
        Mode::Malformed(reason) => match reason {
            Malformed::Partial => result.ready_mask = 0,
            Malformed::NotCompleted => result.completed = 0,
            Malformed::UnknownReason => {
                result.ready_mask = 0;
                result.ineligible_mask = 1;
                result.reasons[0] = 4;
            }
            Malformed::OverlappingMasks => {
                result.ineligible_mask = 1;
                result.reasons[0] = 1;
            }
            Malformed::Reserved => result.reserved = 1,
            Malformed::UnusedReason => result.reasons[31] = 1,
            Malformed::WrongSize => result.size = 151,
            Malformed::WrongVersion => result.version = 2,
        },
        Mode::Ready | Mode::Fail { .. } => {}
    }
    // SAFETY: the adapter lends one initialized writable Output for this call.
    unsafe { *output = result };
    Ok(0)
}

struct Fixture {
    provider: NativeCoverageProvider,
    resources: ResourcesRef,
    account: Arc<Account>,
}

impl Fixture {
    fn new(mode: Mode) -> Self {
        let account = Arc::new(Account::default());
        let first = FirstFailureRef::new_prepaid();
        let resources = ResourcesRef::new_prepaid(
            TestBackend(account.clone()),
            first.clone(),
            Limits::default(),
        );
        CONTROL.with(|cell| {
            assert!(cell.borrow().is_none());
            *cell.borrow_mut() = Some(Control {
                mode,
                account: account.clone(),
                first,
                snapshots: Vec::new(),
                issue: None,
            });
        });
        Self {
            provider: NativeCoverageProvider::new(callbacks(), SESSION).unwrap(),
            resources,
            account,
        }
    }

    fn finish(&self, failed_refunds: u64) -> Control {
        let control = CONTROL.with(|cell| cell.borrow_mut().take().unwrap());
        assert_eq!(control.issue, None);
        let state = self.account.state.lock().unwrap();
        assert!(state.live.is_empty());
        assert_eq!(state.releases, state.allocations.len());
        let failed_scratch_bytes: usize = if state.fail_release {
            state
                .allocations
                .iter()
                .filter(|(kind, _)| *kind == ResourceKind::Scratch)
                .map(|(_, bytes)| *bytes)
                .sum()
        } else {
            0
        };
        drop(state);
        let statistics = self.resources.statistics();
        // The original Lease contract intentionally keeps a failed refund in
        // its ledger, although the test backend saw physical storage retire.
        // Never label that failed cleanup a successful retirement.
        assert_eq!(statistics.live_leases, failed_refunds);
        assert_eq!(statistics.scratch_bytes, failed_scratch_bytes as u64);
        assert_eq!(statistics.release_failures, failed_refunds);
        if failed_refunds == 0 {
            self.resources.validate_retired().unwrap();
        }
        control
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Clear on assertion unwind too, so a reused test worker cannot inherit
        // another test's mode or temporary account/first-fault reference.
        let _ = CONTROL.try_with(|cell| {
            if let Ok(mut slot) = cell.try_borrow_mut() {
                slot.take();
            }
        });
    }
}

fn callbacks() -> Callbacks {
    Callbacks {
        size: 24,
        version: 1,
        session: SESSION,
        cover_batch: Some(callback),
        ..Callbacks::default()
    }
}

fn original_path() -> tiny_skia::Path {
    let mut path = PathBuilder::new();
    path.move_to(-0.0, 1.25);
    path.line_to(2.5, -3.0);
    path.quad_to(4.0, -5.5, 6.25, 7.75);
    path.cubic_to(8.0, 9.5, -10.25, 11.5, 12.75, -13.0);
    path.close();
    path.finish().unwrap()
}

fn request(path: &tiny_skia::Path) -> CoverageRequest<'_> {
    CoverageRequest {
        path,
        fill_rule: FillRule::EvenOdd,
        matrix_bits: MATRIX,
        s_bits: 0x40c0_0000, // 6
        t_bits: 0x3fc0_0000, // 1.5
        target_x: 0,
        target_y: 0,
        width: 2,
        height: 2,
        a8_offset: 3,
        stride: 4,
        length: 6,
    }
}

fn assert_initial_output(snapshot: &Snapshot) {
    assert_eq!(snapshot.session, SESSION);
    assert_eq!(snapshot.initial.size, 152);
    assert_eq!(snapshot.initial.version, 1);
    assert_eq!(snapshot.initial.ready_mask, 0);
    assert_eq!(snapshot.initial.ineligible_mask, 0);
    assert_eq!(snapshot.initial.reasons, [0; 32]);
    assert_eq!(snapshot.initial.completed, 0);
    assert_eq!(snapshot.initial.reserved, 0);
    assert_eq!(snapshot.padding, [0; 3]);
}

#[test]
fn native_coverage_layout_and_callback_session_are_exact() {
    assert_eq!((size_of::<Command>(), align_of::<Command>()), (32, 8));
    assert_eq!((size_of::<Path>(), align_of::<Path>()), (88, 8));
    assert_eq!((size_of::<Batch>(), align_of::<Batch>()), (48, 8));
    assert_eq!((size_of::<Output>(), align_of::<Output>()), (152, 8));
    assert_eq!((size_of::<Callbacks>(), align_of::<Callbacks>()), (24, 8));
    assert_eq!(std::mem::offset_of!(Output, reasons), 16);
    assert_eq!(std::mem::offset_of!(Output, completed), 144);
    assert_eq!(std::mem::offset_of!(Path, matrix_bits), 16);
    assert_eq!(std::mem::offset_of!(Path, a8_offset), 64);
    assert_eq!(std::mem::offset_of!(Batch, paths), 16);
    assert_eq!(std::mem::offset_of!(Batch, commands), 24);
    assert_eq!(std::mem::offset_of!(Batch, a8), 32);
    assert_eq!(std::mem::offset_of!(Batch, a8_capacity), 40);
    assert_eq!(std::mem::offset_of!(Callbacks, cover_batch), 16);
    assert_eq!(
        NativeCoverageProvider::new(callbacks(), SESSION)
            .unwrap()
            .epoch(),
        SESSION
    );
    for changed in 0..5 {
        let mut table = callbacks();
        match changed {
            0 => table.size = 23,
            1 => table.version = 2,
            2 => table.session = 0,
            3 => table.session = SESSION + 1,
            4 => table.cover_batch = None,
            _ => unreachable!(),
        }
        assert!(
            matches!(NativeCoverageProvider::new(table, SESSION), Err(error) if error == invariant())
        );
    }
    #[cfg(target_pointer_width = "32")]
    {
        let mut table = callbacks();
        table.callback_padding = 1;
        assert!(
            matches!(NativeCoverageProvider::new(table, SESSION), Err(error) if error == invariant())
        );
    }
}

#[test]
fn native_coverage_preserves_original_verbs_bits_matrix_and_strided_output() {
    let fixture = Fixture::new(Mode::Ready);
    let path = original_path();
    let mut arena = [0xa5; 15];
    let arena_address = arena.as_mut_ptr() as usize;
    let result = fixture
        .provider
        .cover_batch(&[request(&path)], &mut arena, &fixture.resources)
        .unwrap();
    assert_eq!(
        (result.ready_mask, result.ineligible_mask, result.completed),
        (1, 0, true)
    );
    let control = fixture.finish(0);
    assert_eq!(control.snapshots.len(), 1);
    let snapshot = &control.snapshots[0];
    assert_initial_output(snapshot);
    assert_eq!(snapshot.header, [48, 1, 1, 5]);
    assert_eq!(snapshot.capacity, 15);
    assert_eq!(snapshot.addresses[2], arena_address);
    assert_eq!(snapshot.commands.len(), 5);
    // Independent literal oracle; deliberately includes negative zero, a Quad,
    // and a Close with no point payload. Never call the production encoder here.
    let expected = [
        (0, [0x8000_0000, 0x3fa0_0000, 0, 0, 0, 0]),
        (1, [0x4020_0000, 0xc040_0000, 0, 0, 0, 0]),
        (
            2,
            [0x4080_0000, 0xc0b0_0000, 0x40c8_0000, 0x40f8_0000, 0, 0],
        ),
        (
            3,
            [
                0x4100_0000,
                0x4118_0000,
                0xc124_0000,
                0x4138_0000,
                0x414c_0000,
                0xc150_0000,
            ],
        ),
        (4, [0; 6]),
    ];
    for (command, (verb, bits)) in snapshot.commands.iter().zip(expected) {
        assert_eq!(
            (command.verb, command.reserved, command.xy_bits),
            (verb, 0, bits)
        );
    }
    let wire = &snapshot.paths[0];
    assert_eq!(
        (
            wire.command_start,
            wire.command_count,
            wire.fill_rule,
            wire.reserved
        ),
        (0, 5, 1, 0)
    );
    assert_eq!(wire.matrix_bits, MATRIX);
    assert_eq!((wire.s_bits, wire.t_bits), (0x40c0_0000, 0x3fc0_0000));
    assert_eq!(
        (wire.target_x, wire.target_y, wire.width, wire.height),
        (0, 0, 2, 2)
    );
    assert_eq!((wire.a8_offset, wire.stride, wire.length), (3, 4, 6));
    let lengths = [88, 160, 15, 152];
    for first in 0..4 {
        assert_ne!(snapshot.addresses[first], 0);
        for second in 0..first {
            let a = snapshot.addresses[first];
            let b = snapshot.addresses[second];
            assert!(a + lengths[first] <= b || b + lengths[second] <= a);
        }
    }
    assert_eq!(
        snapshot.live_at_call,
        [
            (ResourceKind::Scratch, 88, true),
            (ResourceKind::Geometry, 160, true)
        ]
    );
    assert_eq!(snapshot.work_at_call.iter().sum::<u64>(), 6);
    assert_eq!(fixture.resources.statistics().work, 6);
    let state = fixture.account.state.lock().unwrap();
    assert_eq!(
        state.allocations,
        [(ResourceKind::Scratch, 88), (ResourceKind::Geometry, 160)]
    );
    for (index, actual) in arena.iter().copied().enumerate() {
        assert_eq!(
            actual,
            if [3, 4, 7, 8].contains(&index) {
                0x40
            } else {
                0xa5
            }
        );
    }
}

#[test]
fn native_coverage_one_batch_preserves_32nd_bit_and_disjoint_outputs() {
    for mode in [Mode::Ready, Mode::LastIneligible] {
        let fixture = Fixture::new(mode);
        let path = original_path();
        let requests: Vec<_> = (0..32)
            .map(|index| CoverageRequest {
                fill_rule: if index % 2 == 0 {
                    FillRule::Winding
                } else {
                    FillRule::EvenOdd
                },
                width: 1,
                height: 1,
                a8_offset: index * 2,
                stride: 1,
                length: 1,
                ..request(&path)
            })
            .collect();
        let mut arena = [0xa5; 64];
        let result = fixture
            .provider
            .cover_batch(&requests, &mut arena, &fixture.resources)
            .unwrap();
        let last_ineligible = matches!(mode, Mode::LastIneligible);
        assert_eq!(
            result.ready_mask,
            if last_ineligible {
                0x7fff_ffff
            } else {
                u32::MAX
            }
        );
        assert_eq!(
            result.ineligible_mask,
            if last_ineligible { 0x8000_0000 } else { 0 }
        );
        for index in 0..32 {
            assert_eq!(
                result.reasons[index],
                if last_ineligible && index == 31 { 3 } else { 0 }
            );
            assert_eq!(
                arena[index * 2],
                if last_ineligible && index == 31 {
                    0xa5
                } else {
                    0x40 + index as u8
                }
            );
            assert_eq!(arena[index * 2 + 1], 0xa5);
        }
        let control = fixture.finish(0);
        assert_eq!(control.snapshots.len(), 1, "32 paths cross Native once");
        let snapshot = &control.snapshots[0];
        assert_initial_output(snapshot);
        assert_eq!(snapshot.header, [48, 1, 32, 160]);
        for (index, path) in snapshot.paths.iter().enumerate() {
            assert_eq!(
                (path.command_start, path.command_count),
                ((index * 5) as u32, 5)
            );
            assert_eq!(path.fill_rule, (index % 2) as u32);
            assert_eq!(
                (path.a8_offset, path.stride, path.length),
                ((index * 2) as u64, 1, 1)
            );
        }
        assert_eq!(
            snapshot.live_at_call,
            [
                (ResourceKind::Scratch, 2816, true),
                (ResourceKind::Geometry, 5120, true)
            ]
        );
        assert_eq!(fixture.resources.statistics().work, 192);
    }
}

#[test]
fn native_coverage_zero_paths_are_valid_and_33_paths_never_call_native() {
    let fixture = Fixture::new(Mode::Ready);
    let result = fixture
        .provider
        .cover_batch(&[], &mut [], &fixture.resources)
        .unwrap();
    assert_eq!(
        (result.ready_mask, result.ineligible_mask, result.completed),
        (0, 0, true)
    );
    let control = fixture.finish(0);
    assert_eq!(control.snapshots.len(), 1);
    let snapshot = &control.snapshots[0];
    assert_initial_output(snapshot);
    assert_eq!(snapshot.header, [48, 1, 0, 0]);
    assert_eq!(&snapshot.addresses[..3], &[0; 3]);
    assert_eq!(snapshot.capacity, 0);
    assert!(snapshot.live_at_call.is_empty());
    assert_eq!(fixture.resources.statistics().work, 0);
    drop(control);
    drop(fixture);

    let fixture = Fixture::new(Mode::Ready);
    let path = original_path();
    let requests = [request(&path); 33];
    let mut arena = [0xa5; 15];
    let error = fixture
        .provider
        .cover_batch(&requests, &mut arena, &fixture.resources)
        .unwrap_err();
    assert_eq!(error, invariant());
    assert_eq!(arena, [0xa5; 15]);
    assert!(fixture.finish(0).snapshots.is_empty());
    assert!(fixture.account.state.lock().unwrap().allocations.is_empty());
}

#[test]
fn native_coverage_raw_hresult_is_preserved_and_positive_success_code_is_rejected() {
    for (hr, expected) in [(DEVICE_REMOVED, DEVICE_REMOVED), (1, 0x8000_ffffu32 as i32)] {
        let fixture = Fixture::new(Mode::Fail {
            hr,
            first: None,
            cancel: false,
            cleanup: false,
        });
        let path = original_path();
        let mut arena = [0xa5; 15];
        let error = fixture
            .provider
            .cover_batch(&[request(&path)], &mut arena, &fixture.resources)
            .unwrap_err();
        assert_eq!(error, Error::native(expected));
        assert_eq!(fixture.resources.failure(), Some(error));
        assert_eq!(arena, [0xa5; 15]);
        let control = fixture.finish(0);
        assert_eq!(control.snapshots.len(), 1);
        assert_initial_output(&control.snapshots[0]);
    }
}

#[test]
fn native_coverage_shared_typed_first_fault_wins_derived_hr_cancel_and_refund() {
    for first in [
        Error::new(FailureKind::Quota, LimitKind::Bytes),
        Error::new(FailureKind::Cancelled, LimitKind::None),
    ] {
        {
            let fixture = Fixture::new(Mode::Ready);
            fixture.resources.fault_slot().record(first);
            let path = original_path();
            let mut arena = [0xa5; 15];
            let error = fixture
                .provider
                .cover_batch(&[request(&path)], &mut arena, &fixture.resources)
                .unwrap_err();
            assert_eq!(error, first);
            assert!(fixture.finish(0).snapshots.is_empty());
            assert!(fixture.account.state.lock().unwrap().allocations.is_empty());
            assert_eq!(arena, [0xa5; 15]);
        }
        let fixture = Fixture::new(Mode::Fail {
            hr: DEVICE_REMOVED,
            first: Some(first),
            cancel: true,
            cleanup: true,
        });
        let path = original_path();
        let mut arena = [0xa5; 15];
        let error = fixture
            .provider
            .cover_batch(&[request(&path)], &mut arena, &fixture.resources)
            .unwrap_err();
        assert_eq!(error, first);
        assert_eq!(fixture.resources.failure(), Some(first));
        let control = fixture.finish(2);
        assert_eq!(control.snapshots.len(), 1);
        assert_eq!(control.first.get(), Some(first));
        assert_eq!(arena, [0xa5; 15]);
    }
}

#[test]
fn native_coverage_work_hr_or_post_callback_cancel_precedes_cleanup_failure() {
    for (mode, expected) in [
        (
            Mode::Fail {
                hr: DEVICE_REMOVED,
                first: None,
                cancel: true,
                cleanup: true,
            },
            Error::native(DEVICE_REMOVED),
        ),
        (
            Mode::CancelAfterSuccess,
            Error::new(FailureKind::Cancelled, LimitKind::None),
        ),
    ] {
        let fixture = Fixture::new(mode);
        let path = original_path();
        let mut arena = [0xa5; 15];
        let error = fixture
            .provider
            .cover_batch(&[request(&path)], &mut arena, &fixture.resources)
            .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(fixture.resources.failure(), Some(expected));
        let control = fixture.finish(2);
        assert_eq!(control.snapshots.len(), 1);
        let state = fixture.account.state.lock().unwrap();
        let calls_before = control.snapshots[0].work_at_call.len();
        assert_eq!(
            state.checkpoints.len() - calls_before,
            usize::from(matches!(mode, Mode::CancelAfterSuccess))
        );
        if matches!(mode, Mode::CancelAfterSuccess) {
            assert_eq!(
                arena[3], 0x40,
                "temporary output existed before cancellation"
            );
        }
    }
}

#[test]
fn native_coverage_malformed_success_never_returns_consumable_a8() {
    for malformed in [
        Malformed::Partial,
        Malformed::NotCompleted,
        Malformed::UnknownReason,
        Malformed::OverlappingMasks,
        Malformed::Reserved,
        Malformed::UnusedReason,
        Malformed::WrongSize,
        Malformed::WrongVersion,
    ] {
        let fixture = Fixture::new(Mode::Malformed(malformed));
        let path = original_path();
        let mut arena = [0xa5; 15];
        let result =
            fixture
                .provider
                .cover_batch(&[request(&path)], &mut arena, &fixture.resources);
        assert!(
            matches!(result, Err(error) if error == invariant()),
            "{malformed:?}: {result:?}"
        );
        // The service can have written scratch pixels; Err is the contract that
        // forbids the enclosing renderer from publishing/consuming any of them.
        assert_eq!(arena[3], 0x40);
        assert_eq!(arena[0], 0xa5);
        assert_eq!(fixture.resources.failure(), Some(invariant()));
        let control = fixture.finish(0);
        assert_eq!(control.snapshots.len(), 1);
        assert_initial_output(&control.snapshots[0]);
    }
}

#[test]
fn native_coverage_invalid_density_matrix_spans_and_padding_alias_reject_before_callback() {
    let path = original_path();
    for case in 0..10 {
        let fixture = Fixture::new(Mode::Ready);
        let mut requests = vec![request(&path)];
        match case {
            0 => requests[0].matrix_bits[3] = 0x7f80_0000,
            1 => requests[0].s_bits = 0x3f80_0000,
            2 => requests[0].t_bits = 0,
            3 => requests[0].target_x = 1,
            4 => requests[0].stride = 1,
            5 => requests[0].length = 5,
            6 => requests[0].a8_offset = usize::MAX,
            7 => {
                requests.push(CoverageRequest {
                    s_bits: 0x4100_0000,
                    a8_offset: 9,
                    ..request(&path)
                });
            }
            8 => {
                // Logical pixel sets {0,10} and {1} do not intersect. The
                // complete first span [0,11) still aliases its padding byte.
                requests[0] = CoverageRequest {
                    width: 1,
                    height: 2,
                    stride: 10,
                    length: 11,
                    a8_offset: 0,
                    ..request(&path)
                };
                requests.push(CoverageRequest {
                    width: 1,
                    height: 1,
                    stride: 1,
                    length: 1,
                    a8_offset: 1,
                    ..request(&path)
                });
            }
            9 => requests[0].s_bits = 0x7fc0_0000,
            _ => unreachable!(),
        }
        let mut arena = [0xa5; 15];
        let result = fixture
            .provider
            .cover_batch(&requests, &mut arena, &fixture.resources);
        assert!(
            matches!(result, Err(error) if error == invariant()),
            "case {case}: {result:?}"
        );
        assert_eq!(arena, [0xa5; 15]);
        assert!(fixture.finish(0).snapshots.is_empty(), "case {case}");
    }
}
