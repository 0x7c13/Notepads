// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Pre-execution value-cursor tests, authored independently of candidate output.
//!
//! Bytes come from literal Markdown/grammar rules and the existing fixed tests
//! identified below, never from a map replay or a candidate run. This is one
//! finite native package, not a new model/golden corpus. The consuming cursor
//! is installed only after the enclosing read returns successfully.
//!
//! The observer Strings/Vecs are test memory. The Host rejects native reserve
//! and admission during reading; that does not detect an unregistered allocator
//! call or replace implementation ownership inspection. The two fixed unwind
//! controls at the end additionally watch real deallocation/lease order using
//! the test-only System forwarding allocator below; other tests do not arm it.
//! No costs, natural cancellation, Core models/queries or G4 are certified here.
//! Cross-session/owner compile-fail, complete AST Owned/Pending inspection and
//! allocator-cut sweeps remain the overall package's existing responsibilities.

use super::direct::{
    AstReadAccess, DirectHost, FinalView, NativeInput, NodeAttributes, NodeToken, TransformKind,
    ValueToken, parse, with_input,
};
use super::{
    AcceptedFact, AppendKind, Host, NativeAdmission, NativeKind, NativeViolation, StageKind,
    WorkDelta,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    marker::PhantomData,
    rc::Rc,
};

#[derive(Clone, Copy, Default)]
struct BackingDrop {
    pointer: usize,
    bytes: usize,
    freed: u64,
    lease_dropped: u64,
    lease_drops: u32,
}
#[derive(Clone, Copy, Default)]
struct UnwindMemory {
    armed: bool,
    sequence: u64,
    last_allocation: (usize, usize),
    backings: [BackingDrop; 2],
}
thread_local! {
    // Const Cell initialization has no heap allocation or destructor. Allocator
    // callbacks only copy this fixed state, including during panic unwinding.
    static UNWIND_MEMORY: Cell<UnwindMemory> = const { Cell::new(UnwindMemory {
        armed: false,
        sequence: 0,
        last_allocation: (0, 0),
        backings: [BackingDrop {
            pointer: 0, bytes: 0, freed: 0, lease_dropped: 0, lease_drops: 0,
        }; 2],
    }) };
}
fn observe_allocation(pointer: *mut u8, bytes: usize) {
    let _ = UNWIND_MEMORY.try_with(|cell| {
        let mut state = cell.get();
        if state.armed {
            state.last_allocation = (pointer as usize, bytes);
            cell.set(state);
        }
    });
}
fn observe_deallocation(pointer: *mut u8) {
    let _ = UNWIND_MEMORY.try_with(|cell| {
        let mut state = cell.get();
        if state.armed {
            for backing in &mut state.backings {
                if backing.pointer != 0 && backing.pointer == pointer as usize && backing.freed == 0
                {
                    state.sequence = state.sequence.saturating_add(1);
                    backing.freed = state.sequence;
                }
            }
            cell.set(state);
        }
    });
}

struct UnwindAllocator;
#[global_allocator]
static UNWIND_ALLOCATOR: UnwindAllocator = UnwindAllocator;

// SAFETY: Each operation forwards the caller's unchanged pointer/Layout/size to
// System and returns its exact result. The observer never dereferences pointers,
// allocates, logs, asserts or panics; TLS teardown is handled by try_with. This
// allocator exists only in this cfg(test) module, never in the production crate.
unsafe impl GlobalAlloc for UnwindAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller supplies System's required valid Layout.
        let pointer = unsafe { System.alloc(layout) };
        observe_allocation(pointer, layout.size());
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Identical delegation, preserving the zeroed-allocation contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        observe_allocation(pointer, layout.size());
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: System allocated the pointer under the unchanged caller Layout.
        unsafe { System.dealloc(pointer, layout) };
        observe_deallocation(pointer);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The caller satisfies GlobalAlloc's live-pointer/Layout/size
        // contract, and System receives those exact arguments.
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            if next != pointer {
                observe_deallocation(pointer);
            }
            observe_allocation(next, size);
        }
        next
    }
}

struct UnwindAllocationScope;
impl UnwindAllocationScope {
    fn arm() -> Self {
        UNWIND_MEMORY.with(|cell| {
            assert!(!cell.get().armed, "unwind controls cannot nest");
            cell.set(UnwindMemory {
                armed: true,
                ..UnwindMemory::default()
            });
        });
        Self
    }
    fn watch(slot: usize, bytes: usize) -> usize {
        UNWIND_MEMORY.with(|cell| {
            let mut state = cell.get();
            assert!(state.armed);
            let (pointer, actual) = state.last_allocation;
            assert_ne!(pointer, 0);
            assert_eq!(actual, bytes, "watch the actual String allocation");
            assert_eq!(state.backings[slot].pointer, 0);
            state.backings[slot] = BackingDrop {
                pointer,
                bytes,
                ..BackingDrop::default()
            };
            cell.set(state);
            pointer
        })
    }
    fn snapshot() -> UnwindMemory {
        UNWIND_MEMORY.with(Cell::get)
    }
}
impl Drop for UnwindAllocationScope {
    fn drop(&mut self) {
        UNWIND_MEMORY.with(|cell| {
            let mut state = cell.get();
            state.armed = false;
            cell.set(state);
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnwindCut {
    DRunGrowth,
    MappingCommitted,
}
struct UnwindArm {
    cut: UnwindCut,
    copied_first: bool,
    partial_inserted: bool,
    fired: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Error {
    Invalid,
    Typed,
    Cancelled,
    ReadAllocation,
    ReadAdmission,
    UnexpectedFact,
}
struct Lease(Rc<Cell<usize>>, u64, Option<usize>);
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(slot) = self.2 {
            let _ = UNWIND_MEMORY.try_with(|cell| {
                let mut state = cell.get();
                if state.armed
                    && let Some(backing) = state.backings.get_mut(slot)
                {
                    state.sequence = state.sequence.saturating_add(1);
                    backing.lease_dropped = state.sequence;
                    backing.lease_drops = backing.lease_drops.saturating_add(1);
                    cell.set(state);
                }
            });
        }
        self.0.set(self.0.get().checked_sub(1).unwrap());
    }
}
// Test-only observation, armed by the real lookup admission. The synchronous
// RefMap path then admits ReferenceBytes and materializes URL before title.
// Only the first three positive decoded admissions belong to this witness;
// later grammar outputs are deliberately not attributed to the reference use.
struct ReferenceObservation {
    reject_second: bool,
    windows: Vec<Vec<u64>>,
    reference_seen: bool,
}
struct TestHost<'input> {
    brand: PhantomData<fn(&'input ()) -> &'input ()>,
    fault: Option<Error>,
    live: Rc<Cell<usize>>,
    readonly: bool,
    cancelled: bool,
    lookups: u64,
    reference_bytes: u64,
    reference_observation: Option<ReferenceObservation>,
    unwind: Option<UnwindArm>,
}
impl<'input> TestHost<'input> {
    fn new(_: &NativeInput<'input>) -> Self {
        Self {
            brand: PhantomData,
            fault: None,
            live: Rc::new(Cell::new(0)),
            readonly: false,
            cancelled: false,
            lookups: 0,
            reference_bytes: 0,
            reference_observation: None,
            unwind: None,
        }
    }
    fn fail(&mut self, error: Error) -> Error {
        *self.fault.get_or_insert(error)
    }
    fn check(&mut self) -> Result<(), Error> {
        if let Some(error) = self.fault {
            Err(error)
        } else if self.cancelled {
            Err(self.fail(Error::Cancelled))
        } else {
            Ok(())
        }
    }
    fn panic_cut(&mut self) -> ! {
        let arm = self.unwind.as_mut().unwrap();
        assert!(!arm.fired);
        arm.fired = true;
        assert_eq!(self.fail(Error::Typed), Error::Typed);
        self.cancelled = true;
        panic!("intentional native value mutation unwind");
    }
}
impl Host for TestHost<'_> {
    type Error = Error;
    type Lease = Lease;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Error> {
        self.check()?;
        if let Some(arm) = &mut self.unwind {
            if arm.cut == UnwindCut::DRunGrowth && matches!(delta, WorkDelta::Copy(73)) {
                assert_ne!(UnwindAllocationScope::snapshot().backings[0].pointer, 0);
                arm.copied_first = true;
            }
            if arm.cut == UnwindCut::MappingCommitted && arm.partial_inserted {
                self.panic_cut();
            }
        }
        Ok(())
    }
    fn admit(&mut self, admission: NativeAdmission) -> Result<(), Error> {
        self.check()?;
        if self.readonly {
            return Err(self.fail(Error::ReadAdmission));
        }
        match admission {
            NativeAdmission::ReferenceLookup(n) => {
                self.lookups += n;
                if let Some(observation) = &mut self.reference_observation {
                    assert_eq!(n, 1);
                    if let Some(previous) = observation.windows.last() {
                        assert_eq!(previous.as_slice(), &[65_535, 5, 6]);
                    }
                    observation.windows.push(Vec::with_capacity(3));
                    observation.reference_seen = false;
                }
            }
            NativeAdmission::ReferenceBytes(n) => {
                self.reference_bytes += n;
                if let Some(observation) = &mut self.reference_observation {
                    assert!(!observation.windows.is_empty());
                    assert!(!observation.reference_seen);
                    assert_eq!(n, 65_546);
                    observation.reference_seen = true;
                }
            }
            NativeAdmission::DecodedBytes(n) if n > 0 => {
                let mut reject = false;
                if let Some(observation) = &mut self.reference_observation
                    && let Some(window) = observation.windows.last_mut()
                    && window.len() < 3
                {
                    assert!(observation.reference_seen);
                    window.push(n);
                    reject = observation.reject_second && window.len() == 2;
                }
                if reject {
                    // Lock the typed error first; simultaneous cancellation
                    // must not replace it at an inner or outer finish.
                    let error = self.fail(Error::Typed);
                    self.cancelled = true;
                    return Err(error);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn reserve(&mut self, bytes: u64) -> Result<Lease, Error> {
        self.check()?;
        if self.readonly {
            return Err(self.fail(Error::ReadAllocation));
        }
        self.live.set(self.live.get() + 1);
        Ok(Lease(self.live.clone(), bytes, None))
    }
    fn admit_actual(&mut self, lease: &mut Lease, bytes: u64) -> Result<(), Error> {
        self.check()?;
        if self.readonly {
            return Err(self.fail(Error::ReadAllocation));
        }
        if let Some(arm) = &self.unwind {
            let watched = match (arm.cut, lease.1) {
                (UnwindCut::DRunGrowth, 73) => Some(0),
                (UnwindCut::DRunGrowth, 146) => Some(1),
                (UnwindCut::MappingCommitted, 79) => Some(0),
                _ => None,
            };
            if let Some(slot) = watched {
                assert_eq!(bytes, lease.1);
                UnwindAllocationScope::watch(slot, bytes as usize);
                lease.2 = Some(slot);
                if arm.cut == UnwindCut::DRunGrowth && slot == 1 {
                    assert!(arm.copied_first, "first D-run backing copied before growth");
                    self.panic_cut();
                }
            }
        }
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Error> {
        Err(self.fail(Error::UnexpectedFact))
    }
    fn reject(&mut self, _: NativeViolation) -> Error {
        self.fail(Error::Invalid)
    }
}
impl<'input> DirectHost<'input> for TestHost<'input> {
    type ReadBrand = PhantomData<fn(&'input ()) -> &'input ()>;
    fn finish<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
        match result {
            Err(error) => Err(self.fail(error)),
            Ok(value) => self.check().map(|()| value),
        }
    }
    fn checkpoint_now(&mut self) -> Result<(), Error> {
        self.check()
    }
}

type View<'call, 'store, 'view, 'arena, 'input> =
    AstReadAccess<'call, 'store, 'view, 'arena, 'input, TestHost<'input>, Lease>;

fn run(
    source: &str,
    g3: bool,
    body: impl for<'store, 'view, 'arena, 'input> FnOnce(
        &mut FinalView<'store, 'view, 'arena, 'input, TestHost<'input>>,
    ) -> Result<(), Error>,
) -> Result<(), Error> {
    with_input(source, |input| {
        let mut options = crate::Options::default();
        options.extension.table = g3;
        options.extension.tasklist = g3;
        options.extension.strikethrough = g3;
        let mut host = TestHost::new(&input);
        let result = parse(input, &options, &mut host, |mut view| {
            view.host.readonly = true;
            let result = body(&mut view);
            view.host.readonly = false;
            result
        });
        assert_eq!(
            host.live.get(),
            0,
            "native leases after actual session drop"
        );
        match &result {
            Ok(()) => assert!(host.fault.is_none()),
            Err(error) => assert_eq!(host.fault, Some(*error)),
        }
        result
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Field {
    Text,
    Literal,
    Info,
    Html,
    Destination,
    Title,
}
#[derive(Debug)]
struct Slot {
    path: Vec<usize>,
    field: Field,
    text: String,
    mapped: bool,
}
type Expected<'a> = (&'a [usize], Field, &'a str, bool);

fn at<'view, 'arena, 'input>(
    view: &mut View<'_, '_, 'view, 'arena, 'input>,
    path: &[usize],
) -> Result<NodeToken<'view, 'arena, 'input>, Error> {
    let mut node = view.root()?;
    for &index in path {
        node = view.node(node)?.first_child.expect("expected child");
        for _ in 0..index {
            node = view.node(node)?.next_sibling.expect("expected sibling");
        }
    }
    Ok(node)
}
fn field<'view, 'arena, 'input>(
    view: &mut View<'_, '_, 'view, 'arena, 'input>,
    path: &[usize],
    field: Field,
) -> Result<ValueToken<'view, 'arena, 'input>, Error> {
    let node = at(view, path)?;
    Ok(match (view.node(node)?.attributes, field) {
        (NodeAttributes::Text(value), Field::Text) | (NodeAttributes::Html(value), Field::Html) => {
            value
        }
        (NodeAttributes::Code { literal, .. }, Field::Literal)
        | (NodeAttributes::InlineCode { literal, .. }, Field::Literal) => literal,
        (NodeAttributes::Code { info, .. }, Field::Info) => info,
        (NodeAttributes::Link { destination, .. }, Field::Destination) => destination,
        (NodeAttributes::Link { title, .. }, Field::Title) => title,
        _ => panic!("expected {field:?} at {path:?}"),
    })
}
fn read_range<'view, 'arena, 'input>(
    view: &mut View<'_, '_, 'view, 'arena, 'input>,
    token: ValueToken<'view, 'arena, 'input>,
    start: usize,
    upper: usize,
    maximum: usize,
) -> Result<String, Error> {
    let mut cursor = view.value_cursor(token, start, upper)?;
    assert_eq!(cursor.position(), start);
    let mut text = String::new();
    loop {
        let before = cursor.position();
        let (part, next) = view.with_next_value_chunk(cursor, maximum, |part, _| {
            assert!(!part.is_empty(), "end must return None, not an empty chunk");
            assert!(part.len() <= maximum);
            text.push_str(part);
            Ok(part.len())
        })?;
        cursor = next;
        match part {
            Some(bytes) => {
                assert_eq!(cursor.position(), before + bytes);
                assert!(cursor.position() <= upper);
            }
            None => {
                assert_eq!(cursor.position(), upper);
                assert_eq!(text.len(), upper - start);
                // End is stable and never invokes the body, including 0..0.
                let (none, end) = view.with_next_value_chunk::<()>(cursor, maximum, |_, _| {
                    panic!("callback after end")
                })?;
                assert!(none.is_none());
                assert_eq!(end.position(), upper);
                return Ok(text);
            }
        }
    }
}
fn read_value<'view, 'arena, 'input>(
    view: &mut View<'_, '_, 'view, 'arena, 'input>,
    token: ValueToken<'view, 'arena, 'input>,
) -> Result<String, Error> {
    let len = view.value_len(token)?;
    let small = read_range(view, token, 0, len, 4)?;
    assert_eq!(read_range(view, token, 0, len, 256)?, small);
    assert_eq!(read_range(view, token, 0, len, 4096)?, small);
    Ok(small)
}
fn collect<'view, 'arena, 'input>(
    view: &mut View<'_, '_, 'view, 'arena, 'input>,
    node: NodeToken<'view, 'arena, 'input>,
    path: &mut Vec<usize>,
    output: &mut Vec<Slot>,
) -> Result<(), Error> {
    let info = view.node(node)?;
    let mut push = |field, token| -> Result<(), Error> {
        output.push(Slot {
            path: path.clone(),
            field,
            text: read_value(view, token)?,
            mapped: view.value_mapping(token)?.is_some(),
        });
        Ok(())
    };
    match info.attributes {
        NodeAttributes::Text(value) => push(Field::Text, value)?,
        NodeAttributes::Html(value) => push(Field::Html, value)?,
        NodeAttributes::Code { info, literal, .. } => {
            push(Field::Info, info)?;
            push(Field::Literal, literal)?;
        }
        NodeAttributes::InlineCode { literal, .. } => push(Field::Literal, literal)?,
        NodeAttributes::Link { destination, title } => {
            push(Field::Destination, destination)?;
            push(Field::Title, title)?;
        }
        _ => {}
    }
    let mut child = info.first_child;
    let mut ordinal = 0;
    while let Some(node) = child {
        path.push(ordinal);
        collect(view, node, path, output)?;
        path.pop();
        child = view.node(node)?.next_sibling;
        ordinal += 1;
    }
    Ok(())
}
fn check_values(source: &str, g3: bool, expected: &[Expected<'_>]) {
    run(source, g3, |view| {
        let mut slots = Vec::new();
        let root = view.root()?;
        collect(view, root, &mut Vec::new(), &mut slots)?;
        assert_eq!(slots.len(), expected.len(), "all value fields: {source:?}");
        for (actual, &(path, field, text, mapped)) in slots.iter().zip(expected) {
            assert_eq!(actual.path, path);
            assert_eq!(actual.field, field);
            assert_eq!(actual.text, text, "{source:?} {path:?} {field:?}");
            assert_eq!(actual.mapped, mapped);
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn raw_multiple_pieces_and_partial_tab_stay_single_backing_chunks() {
    check_values(
        "```\na\nb\n```",
        false,
        &[
            (&[0], Field::Info, "", false),
            (&[0], Field::Literal, "a\nb\n", true),
        ],
    );
    // List continuation consumes 2 columns, code indent another 4. Of the
    // second tab's columns 4..8, two remain: original literal b\n  c\n.
    let source = "- a\n\n      b\n\t\tc\n";
    check_values(
        source,
        false,
        &[
            (&[0, 0, 0, 0], Field::Text, "a", true),
            (&[0, 0, 1], Field::Info, "", false),
            (&[0, 0, 1], Field::Literal, "b\n  c\n", true),
        ],
    );
    run(source, false, |view| {
        let token = field(view, &[0, 0, 1], Field::Literal)?;
        let mapping = view.value_mapping(token)?.unwrap();
        let mut partial = 0;
        for i in 0..view.mapping_len(mapping)? {
            let atom = view.mapping_atom(mapping, i)?;
            let mut source = view.source_cursor(atom.source)?;
            while let Some(piece) = view.next_source(&mut source)? {
                if let AppendKind::PartialTab { spaces, .. } = piece.kind {
                    assert_eq!(spaces, 2);
                    partial += 1;
                }
            }
        }
        assert_eq!(partial, 1, "fixture must exercise real PartialTab");
        // These boundaries derive from the two source lines, accepted spaces,
        // and the final real LF substitution, not from observed map bytes.
        let boundaries = [2, 4, 5, 6];
        let mut cursor = view.value_cursor(token, 0, 6)?;
        let mut result = String::new();
        while cursor.position() < 6 {
            let before = cursor.position();
            let limit = *boundaries.iter().find(|&&end| end > before).unwrap();
            let (size, next) = view.with_next_value_chunk(cursor, 4096, |part, _| {
                assert!(!part.is_empty());
                assert!(
                    before + part.len() <= limit,
                    "crossed actual backing boundary"
                );
                result.push_str(part);
                Ok(part.len())
            })?;
            assert!(size.is_some());
            cursor = next;
        }
        assert_eq!(result, "b\n  c\n");
        Ok(())
    })
    .unwrap();
}

#[test]
fn g3_pipe_empty_missing_cells_and_task_base_keep_fixed_values() {
    let table = "| a\\|b | |\n| --- | --- |\n| c |";
    check_values(
        table,
        true,
        &[
            (&[0, 0, 0, 0], Field::Text, "a|b", true),
            (&[0, 1, 0, 0], Field::Text, "c", true),
        ],
    );
    run(table, true, |view| {
        for path in [&[0, 0, 1][..], &[0, 1, 1][..]] {
            let node = at(view, path)?;
            let info = view.node(node)?;
            assert_eq!(info.kind, NativeKind::TableCell);
            assert!(info.first_child.is_none());
        }
        Ok(())
    })
    .unwrap();
    let task = "- [ ] \n- [x] a\\*b";
    check_values(task, true, &[(&[0, 1, 1, 0], Field::Text, "a*b", true)]);
    run(task, true, |view| {
        let item = at(view, &[0, 0])?;
        let checkbox = view.node(item)?.first_child.unwrap();
        let info = view.node(checkbox)?;
        assert_eq!(info.kind, NativeKind::TaskCheckbox);
        assert!(
            info.next_sibling.is_none(),
            "empty task must not retain paragraph"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn entity_escape_whole_atom_keeps_suffix_and_zero_title_atoms() {
    // Existing finite design witness: &nvlt; = U+003C U+20D2. Escape consumes
    // only backslash + '<'; the current whole proof must still include U+20D2.
    let source = r#"[x](u "\&nvlt;")"#;
    check_values(
        source,
        false,
        &[
            (&[0, 0], Field::Destination, "u", true),
            (&[0, 0], Field::Title, "<\u{20d2}", true),
            (&[0, 0, 0], Field::Text, "x", true),
        ],
    );
    run(source, false, |view| {
        let title = field(view, &[0, 0], Field::Title)?;
        let mapping = view.value_mapping(title)?.unwrap();
        assert_eq!(view.mapping_len(mapping)?, 3);
        assert_eq!(view.mapping_atom(mapping, 0)?.bytes, 0);
        assert_eq!(view.mapping_atom(mapping, 2)?.bytes, 0);
        let current = view.mapping_atom(mapping, 1)?;
        assert_eq!(current.bytes, 4);
        let proof = view.proof(current.proof.unwrap())?;
        assert_eq!(proof.kind, TransformKind::Payload(StageKind::Escape));
        assert_eq!(proof.output_len, 4);
        assert_eq!(view.mapping_len(proof.input)?, 2);
        let entity = view.mapping_atom(proof.input, 1)?.proof.unwrap();
        assert_eq!(
            view.proof(entity)?.kind,
            TransformKind::Payload(StageKind::Entity)
        );
        // A subrange may split the current UTF-8 value at a scalar boundary;
        // it must not split the provenance atom or mutate its ancestors.
        assert_eq!(read_range(view, title, 1, 4, 4)?, "\u{20d2}");
        assert_eq!(view.mapping_len(mapping)?, 3);
        Ok(())
    })
    .unwrap();
}

#[test]
fn zero_values_keep_present_mapping_distinct_from_absent_fields() {
    for (source, present) in [(r#"[x](u "")"#, true), ("[x](u)", false)] {
        check_values(
            source,
            false,
            &[
                (&[0, 0], Field::Destination, "u", true),
                (&[0, 0], Field::Title, "", present),
                (&[0, 0, 0], Field::Text, "x", true),
            ],
        );
    }
    check_values(
        "``` &#32; \nx\n```",
        false,
        &[
            (&[0], Field::Info, "", true),
            (&[0], Field::Literal, "x\n", true),
        ],
    );
    check_values(
        "```\n```",
        false,
        &[
            (&[0], Field::Info, "", false),
            (&[0], Field::Literal, "", true),
        ],
    );
}

#[test]
fn code_trim_crlf_real_and_synthetic_terminals_use_original_bytes() {
    // Exact existing native/direct/tests.rs code_trim_keeps_two_zero_output_edge_proofs...
    // expectations, read through the new cursor; no replay supplies these bytes.
    for source in ["` a\nb `", "` a\r\nb `"] {
        check_values(source, false, &[(&[0, 0], Field::Literal, "a b", true)]);
        run(source, false, |view| {
            let value = field(view, &[0, 0], Field::Literal)?;
            let map = view.value_mapping(value)?.unwrap();
            assert_eq!(view.mapping_len(map)?, 5);
            for index in [0, 4] {
                let atom = view.mapping_atom(map, index)?;
                assert_eq!(atom.bytes, 0);
                assert_eq!(view.proof(atom.proof.unwrap())?.output_len, 0);
            }
            Ok(())
        })
        .unwrap();
    }
    for (source, expected) in [
        ("    a\n", "a\n"),
        ("    a\r\n", "a\n"),
        ("    a", "a\n"),
        ("```\na\n```", "a\n"),
        ("```\na", "a\n"),
        ("```\r\na\r\n```", "a\r\n"),
    ] {
        // NativeInput here deliberately includes CRLF: fenced body preserves
        // it; indented terminal appends LF. Product S->P normalization is separate.
        check_values(
            source,
            false,
            &[
                (&[0], Field::Info, "", false),
                (&[0], Field::Literal, expected, true),
            ],
        );
    }
}

#[test]
fn long_inline_and_block_code_cross_chunk_limits_without_materialization() {
    // Fixed 1024 repetitions, derived before parsing: each inline CRLF -> one
    // space, outer spaces trim once; unterminated fenced EOF adds exactly LF.
    let source = format!("` {} `", "中a\r\n".repeat(1024));
    let expected = "中a ".repeat(1024);
    check_values(
        &source,
        false,
        &[(&[0, 0], Field::Literal, &expected, true)],
    );
    let source = format!("```\n{}tail", "中a\n".repeat(1024));
    let expected = format!("{}tail\n", "中a\n".repeat(1024));
    check_values(
        &source,
        false,
        &[
            (&[0], Field::Info, "", false),
            (&[0], Field::Literal, &expected, true),
        ],
    );
}

#[test]
fn definitions_winner_loser_unused_and_two_uses_keep_key_and_admissions() {
    // Extends the exact /a "x", /b "&amp;" winner/loser values in the
    // existing native/direct/tests.rs duplicate_and_unused_definitions test.
    let source = "[r]: /a \"x\"\n[r]: /b \"&amp;\"\n[u]: /unused\n\n[r] [r]";
    check_values(
        source,
        false,
        &[
            (&[0, 0], Field::Destination, "/a", true),
            (&[0, 0], Field::Title, "x", true),
            (&[0, 0, 0], Field::Text, "r", true),
            (&[0, 1], Field::Text, " ", true),
            (&[0, 2], Field::Destination, "/a", true),
            (&[0, 2], Field::Title, "x", true),
            (&[0, 2, 0], Field::Text, "r", true),
        ],
    );
    run(source, false, |view| {
        assert_eq!(view.definition_count(), 3);
        for (index, (key, destination, title, winner)) in [
            ("r", "/a", Some("x"), true),
            ("r", "/b", Some("&"), false),
            ("u", "/unused", None, true),
        ]
        .into_iter()
        .enumerate()
        {
            let definition = view.definition(index)?;
            assert_eq!(definition.winner, winner);
            assert_eq!(read_value(view, definition.normalized_key)?, key);
            assert!(view.value_mapping(definition.normalized_key)?.is_none());
            assert_eq!(read_value(view, definition.destination)?, destination);
            match (definition.title, title) {
                (Some(token), Some(expected)) => assert_eq!(read_value(view, token)?, expected),
                (None, None) => {}
                _ => panic!("title presence"),
            }
        }
        assert_eq!(view.host.lookups, 2);
        assert_eq!(view.host.reference_bytes, 6, "two uses each pay /a + x");
        Ok(())
    })
    .unwrap();
}

#[test]
fn reference_title_indent_and_angle_mailto_prefix_are_not_redecoded() {
    // The accepted-reference-title pre-Entity stage removes indentation after
    // LF, preserving the LF itself, as fixed in native/title_indent_tests.rs.
    check_values(
        "[r]: /a \"a\n    b\"\n\n[r]",
        false,
        &[
            (&[0, 0], Field::Destination, "/a", true),
            (&[0, 0], Field::Title, "a\nb", true),
            (&[0, 0, 0], Field::Text, "r", true),
        ],
    );
    check_values(
        "<a@example.com>",
        false,
        &[
            (&[0, 0], Field::Destination, "mailto:a@example.com", true),
            (&[0, 0], Field::Title, "", false),
            (&[0, 0, 0], Field::Text, "a@example.com", true),
        ],
    );
}

#[test]
fn delimiter_remainder_escaped_join_and_html_fields_keep_their_bytes() {
    check_values(
        "***x** ~~a\\*b~~",
        true,
        &[
            (&[0, 0], Field::Text, "*", true),
            (&[0, 1, 0], Field::Text, "x", true),
            (&[0, 2], Field::Text, " ", true),
            (&[0, 3, 0], Field::Text, "a*b", true),
        ],
    );
    check_values(
        "a\\*b &amp; c",
        false,
        &[(&[0, 0], Field::Text, "a*b & c", true)],
    );
    check_values(
        "<i>x</i>",
        false,
        &[
            (&[0, 0], Field::Html, "<i>", true),
            (&[0, 1], Field::Text, "x", true),
            (&[0, 2], Field::Html, "</i>", true),
        ],
    );
    check_values(
        "<div>\nx\n</div>\n",
        false,
        &[(&[0], Field::Html, "<div>\nx\n</div>\n", true)],
    );
}

#[test]
fn utf8_empty_ranges_invalid_endpoints_and_invalid_maximum_latch() {
    run("中😀x", false, |view| {
        let token = field(view, &[0, 0], Field::Text)?;
        assert_eq!(view.value_len(token)?, 8);
        for (start, end, expected) in [(0, 8, "中😀x"), (3, 7, "😀"), (3, 3, ""), (8, 8, "")] {
            assert_eq!(read_range(view, token, start, end, 4)?, expected);
        }
        Ok(())
    })
    .unwrap();
    for (start, end) in [(1, 8), (0, 4), (3, 6), (8, 7), (0, 9), (9, 9)] {
        let result = run("中😀x", false, |view| {
            let token = field(view, &[0, 0], Field::Text)?;
            assert!(matches!(
                view.value_cursor(token, start, end),
                Err(Error::Invalid)
            ));
            assert_eq!(view.value_len(token), Err(Error::Invalid));
            Ok(()) // The caller cannot hide a previously latched error.
        });
        assert_eq!(result, Err(Error::Invalid));
    }
    for maximum in [3, 4097] {
        let result = run("中😀x", false, |view| {
            let token = field(view, &[0, 0], Field::Text)?;
            let cursor = view.value_cursor(token, 0, 8)?;
            let result = view.with_next_value_chunk::<()>(cursor, maximum, |_, _| {
                panic!("invalid maximum must reject before callback")
            });
            assert!(matches!(result, Err(Error::Invalid)));
            assert_eq!(view.value_len(token), Err(Error::Invalid));
            Ok(())
        });
        assert_eq!(result, Err(Error::Invalid));
    }
}

#[test]
fn consuming_cursor_failure_and_caught_unwind_never_return_a_successor() {
    for typed in [false, true] {
        let result = run("abc", false, |view| {
            let token = field(view, &[0, 0], Field::Text)?;
            let cursor = view.value_cursor(token, 0, 3)?;
            let result = view.with_next_value_chunk(cursor, 4, |part, host| {
                assert_eq!(part, "abc");
                host.cancelled = true;
                if typed { Err(Error::Typed) } else { Ok(()) }
            });
            let expected = if typed {
                Error::Typed
            } else {
                Error::Cancelled
            };
            assert!(matches!(result, Err(error) if error == expected));
            assert_eq!(view.value_len(token), Err(expected));
            Ok(())
        });
        assert_eq!(
            result,
            Err(if typed {
                Error::Typed
            } else {
                Error::Cancelled
            })
        );
    }
    let result = run("abc", false, |view| {
        let token = field(view, &[0, 0], Field::Text)?;
        let cursor = view.value_cursor(token, 0, 3)?;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            view.with_next_value_chunk::<()>(cursor, 4, |_, _| panic!("intentional chunk unwind"))
        }));
        assert!(caught.is_err());
        assert_eq!(view.value_len(token), Err(Error::Invalid));
        Ok(())
    });
    assert_eq!(result, Err(Error::Invalid));
}

#[test]
fn outer_read_finish_failure_does_not_install_the_returned_cursor() {
    let result = run("abc", false, |view| {
        let token = field(view, &[0, 0], Field::Text)?;
        view.with_reader(|reader, host| {
            let mut installed =
                Some(reader.with_reads(host, |access| access.value_cursor(token, 0, 3))?);
            let result = reader.with_reads(host, |access| {
                let current = installed.take().unwrap();
                let next = access.with_next_value_chunk(current, 4, |part, _| {
                    assert_eq!(part, "abc");
                    Ok(())
                })?;
                assert_eq!(next.1.position(), 3);
                // The inner cursor call succeeded. Only the enclosing read's
                // final check now sees cancellation, so Core must not install it.
                access.host.cancelled = true;
                Ok(next)
            });
            match result {
                Ok((_, cursor)) => installed = Some(cursor),
                Err(error) => assert_eq!(error, Error::Cancelled),
            }
            assert!(installed.is_none());
            Ok(())
        })
    });
    assert_eq!(result, Err(Error::Cancelled));
}

#[test]
fn reference_uses_keep_decoded_order_utf8_chunks_and_first_typed_fault() {
    // Independent old Context::decode_append rule: cap at 65_536 bytes, then
    // retreat to a UTF-8 boundary. '/' + 65_534 ASCII bytes ends at 65_535;
    // the four-byte emoji straddles 65_536, leaving emoji + 'z' as five bytes.
    // Title t + emoji + x is six bytes. Sharing accepted values must preserve
    // these URL-before-title semantic admissions for EACH actual use.
    let destination = format!("/{}😀z", "a".repeat(65_534));
    let title = "t😀x";
    assert_eq!(destination.len(), 65_540);
    assert!(destination.is_char_boundary(65_535));
    assert!(!destination.is_char_boundary(65_536));
    assert_eq!(title.len(), 6);
    // The original bare URL scanner accepts these non-whitespace UTF-8 bytes;
    // the quoted title has no escapes or newlines. Two shortcut uses are
    // separated by a space, so neither consumes the other's label.
    let source = format!("[r]: {destination} \"{title}\"\n\n[r] [r]");
    for reject_second in [false, true] {
        with_input(&source, |input| {
            let mut host = TestHost::new(&input);
            host.reference_observation = Some(ReferenceObservation {
                reject_second,
                windows: Vec::new(),
                reference_seen: false,
            });
            let mut finalized = false;
            let result = parse(input, &crate::Options::default(), &mut host, |mut view| {
                assert!(
                    !reject_second,
                    "rejected materialization cannot publish AST"
                );
                finalized = true;
                view.host.readonly = true;
                for path in [&[0, 0][..], &[0, 2][..]] {
                    let url = field(&mut view, path, Field::Destination)?;
                    assert_eq!(view.value_len(url)?, 65_540);
                    assert_eq!(read_range(&mut view, url, 0, 65_540, 4096)?, destination);
                    let value = field(&mut view, path, Field::Title)?;
                    assert_eq!(read_value(&mut view, value)?, title);
                }
                view.host.readonly = false;
                Ok(())
            });
            // No mock release: every permit must have reached actual Lease::drop
            // by parse's return, on both complete AST and interrupted use paths.
            assert_eq!(host.live.get(), 0);
            assert_eq!(finalized, !reject_second);
            let observation = host.reference_observation.as_ref().unwrap();
            if reject_second {
                assert_eq!(result, Err(Error::Typed));
                assert_eq!(host.fault, Some(Error::Typed));
                assert!(host.cancelled);
                assert_eq!(host.lookups, 1);
                assert_eq!(host.reference_bytes, 65_546);
                assert_eq!(observation.windows, vec![vec![65_535, 5]]);
                assert_eq!(host.work(WorkDelta::Events(1)), Err(Error::Typed));
                assert!(matches!(host.reserve(1), Err(Error::Typed)));
                assert_eq!(host.finish(Ok(())), Err(Error::Typed));
                assert_eq!(host.live.get(), 0);
            } else {
                assert_eq!(result, Ok(()));
                assert_eq!(host.fault, None);
                assert!(!host.cancelled);
                assert_eq!(host.lookups, 2);
                assert_eq!(host.reference_bytes, 131_092);
                assert_eq!(observation.windows, vec![vec![65_535, 5, 6]; 2]);
            }
        });
    }
}

// Integration controls below were authored by the integrator before execution.
// Their local observer Strings are test data; no readonly production API gains
// a materializer. These controls do not certify full G4 models or memory peaks.
#[test]
fn g4_grammar_transport_reads_real_values_and_no_at_does_not_allocate() {
    for (source, expected) in [("literal😀", None), ("x&#64;", Some("x@"))] {
        with_input(source, |input| {
            let mut host = TestHost::new(&input);
            {
                let mut context = super::Context::new_direct(&mut host, source);
                context.inline_policy = super::InlinePolicy::StrictG4;
                let mut options = crate::Options::default();
                options.extension.table = true;
                options.extension.tasklist = true;
                options.extension.strikethrough = true;
                crate::parser::parse_in_context(source, &options, &mut context, |root, context| {
                    let paragraph = root.first_child().unwrap();
                    let node = paragraph.first_child().unwrap();
                    assert!(node.next_sibling().is_none());
                    let ast = node.data();
                    let crate::nodes::NodeValue::Text(value) = &ast.value else {
                        panic!("fixed non-email text")
                    };
                    let id = ast.native_id.unwrap();
                    let before = value.repr.clone();
                    // The no-@ path must succeed even when reserve/admission
                    // are rejected; ordinary read work remains enabled.
                    context.host.readonly = expected.is_none();
                    let temporary = context.g4_email_text(id, value)?;
                    context.host.readonly = false;
                    assert_eq!(temporary.as_deref(), expected);
                    assert_eq!(value.repr, before, "the actual AST slot stays accepted");
                    if let Some(temporary) = temporary {
                        context.discard_string(temporary)?;
                    }
                    Ok(())
                })
                .unwrap();
            }
            assert!(host.fault.is_none());
            assert_eq!(host.live.get(), 0);
        });
    }
}

#[test]
fn g4_grammar_transport_rejection_closes_phase_and_keeps_first_fault() {
    with_input("x&#64;", |input| {
        let mut host = TestHost::new(&input);
        {
            let mut context = super::Context::new_direct(&mut host, "x&#64;");
            context.inline_policy = super::InlinePolicy::StrictG4;
            let mut options = crate::Options::default();
            options.extension.table = true;
            options.extension.tasklist = true;
            options.extension.strikethrough = true;
            let result = crate::parser::parse_in_context(
                "x&#64;",
                &options,
                &mut context,
                |root, context| {
                    let node = root.first_child().unwrap().first_child().unwrap();
                    let ast = node.data();
                    let crate::nodes::NodeValue::Text(value) = &ast.value else {
                        panic!("fixed non-email text")
                    };
                    context.host.readonly = true;
                    let error = context
                        .g4_email_text(ast.native_id.unwrap(), value)
                        .unwrap_err();
                    assert_eq!(error, Error::ReadAllocation);
                    assert_eq!(context.final_phase, super::FinalPhase::Failed);
                    context.host.cancelled = true;
                    assert_eq!(context.event(), Err(Error::ReadAllocation));
                    Err::<(), _>(error)
                },
            );
            assert_eq!(result, Err(Error::ReadAllocation));
        }
        assert_eq!(host.fault, Some(Error::ReadAllocation));
        assert_eq!(host.live.get(), 0);
    });
}

fn actual_mutation_unwind(cut: UnwindCut) {
    // A 73-byte raw backing, then Entity '@', then raw tail. There is no
    // dot-containing email domain, so the original selector leaves one Text.
    // MappingAccess first allocates/copies 73 bytes, then must grow to 146.
    let source = format!("{}&#64;tail", "a".repeat(73));
    with_input(&source, |input| {
        let mut host = TestHost::new(&input);
        let mut watched = None;
        {
            let mut context = super::Context::new_direct(&mut host, &source);
            context.inline_policy = super::InlinePolicy::StrictG4;
            let mut options = crate::Options::default();
            options.extension.table = true;
            options.extension.tasklist = true;
            options.extension.strikethrough = true;
            let result = crate::parser::parse_in_context(
                &source,
                &options,
                &mut context,
                |root, context| {
                    let node = root.first_child().unwrap().first_child().unwrap();
                    assert!(node.next_sibling().is_none());
                    let ast = node.data();
                    let crate::nodes::NodeValue::Text(value) = &ast.value else {
                        panic!("fixed non-email Text before arming the real cut")
                    };
                    let id = ast.native_id.unwrap();
                    let before = value.repr.clone();
                    let binding = context
                        .mappings
                        .nodes
                        .get(id.get() as usize - 1, |n| {
                            super::rows::charge_host(context.host, n)
                        })?
                        .unwrap();
                    let mapping = context
                        .mappings
                        .values
                        .get(binding.value.unwrap().get() as usize - 1, |n| {
                            super::rows::charge_host(context.host, n)
                        })?
                        .unwrap();
                    assert_eq!(mapping.bytes, 78);
                    assert_eq!(mapping.atoms[0].bytes, 73);
                    let raw = mapping.atoms[0].source;
                    let proofs_before = context.mappings.proofs.len();
                    // Keep registry preparation outside the armed region. The
                    // Host also binds each watch to the exact System allocation
                    // size, so a directory growth cannot impersonate a D-run.
                    context.ensure_slot()?;
                    watched = Some(UnwindAllocationScope::arm());
                    context.host.unwind = Some(UnwindArm {
                        cut,
                        copied_first: false,
                        partial_inserted: false,
                        fired: false,
                    });
                    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        match cut {
                            UnwindCut::DRunGrowth => context.g4_email_text(id, value),
                            UnwindCut::MappingCommitted => {
                                context.mutate_mappings(|mappings, access| -> Result<Option<String>, Error> {
                                    // This is deliberately an incomplete mutation,
                                    // never a successful proof/grammar oracle. Its
                                    // actual String moves into the real Context
                                    // store before the Host callback unwinds.
                                    let result = access.string("ppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppp")?;
                                    assert_eq!(result.len(), 79);
                                    assert_eq!(result.as_ptr() as usize, UnwindAllocationScope::snapshot().backings[0].pointer);
                                    mappings.proofs.push(super::mapped::Proof {
                                        kind: super::mapped::TransformKind::Decode(super::DecodeKind::Entity),
                                        stage: 1,
                                        host: super::mapped::HostSnapshot {
                                            source: super::mapped::ClaimSource::R(raw),
                                            utf8_len: raw.end,
                                            consumed_start: raw.start,
                                            consumed_end: raw.end,
                                            payload_role: None,
                                            role: super::mapped::HostRole::Inline,
                                            phase: super::mapped::NativePhase::Inline,
                                        },
                                        input: super::small_value::SmallValue::new(),
                                        local_start: raw.start,
                                        local_end: raw.end,
                                        result,
                                    }, access.host)?;
                                    assert_eq!(mappings.proofs.len(), proofs_before + 1);
                                    access.host.unwind.as_mut().unwrap().partial_inserted = true;
                                    access.host.work(WorkDelta::Events(1))?;
                                    Ok(None)
                                })
                            }
                        }
                    }));
                    let panic =
                        caught.expect_err("real armed mutation must unwind, never return a value");
                    assert_eq!(
                        panic.downcast_ref::<&str>().copied(),
                        Some("intentional native value mutation unwind")
                    );
                    let arm = context.host.unwind.as_ref().unwrap();
                    assert!(arm.fired);
                    assert_eq!(context.final_phase, super::FinalPhase::Failed);
                    assert_eq!(context.host.fault, Some(Error::Typed));
                    assert!(context.host.cancelled);
                    assert_eq!(value.repr, before);
                    let memory = UnwindAllocationScope::snapshot();
                    match cut {
                        UnwindCut::DRunGrowth => {
                            assert!(arm.copied_first);
                            assert_eq!(memory.backings[0].bytes, 73);
                            assert!(
                                memory.backings[0].freed > 0,
                                "partial D-run has really freed on unwind"
                            );
                            assert_eq!(
                                memory.backings[0].lease_drops, 0,
                                "registered lease stays in Context until its drop"
                            );
                            let grown = memory.backings[1];
                            assert_eq!(grown.bytes, 146);
                            assert!(grown.freed > 0 && grown.freed < grown.lease_dropped);
                            assert_eq!(
                                grown.lease_drops, 1,
                                "unregistered grown buffer drops before its local lease"
                            );
                        }
                        UnwindCut::MappingCommitted => {
                            assert!(arm.partial_inserted);
                            assert_eq!(context.mappings.proofs.len(), proofs_before + 1);
                            // An unmetered test-only observation after first fault;
                            // no failed Host work or production read is resumed.
                            let partial = context
                                .mappings
                                .proofs
                                .get(proofs_before, |_| Ok::<_, Error>(()))?
                                .unwrap();
                            assert_eq!(partial.result.len(), 79);
                            assert!(partial.result.bytes().all(|b| b == b'p'));
                            assert_eq!(
                                partial.result.as_ptr() as usize,
                                memory.backings[0].pointer
                            );
                            assert_eq!(
                                memory.backings[0].freed, 0,
                                "partial owner remains in the actual Context"
                            );
                            assert_eq!(memory.backings[0].lease_drops, 0);
                        }
                    }
                    // An error latched by the actual Host before the late
                    // cancellation remains first. Caught unwind cannot reopen
                    // either grammar admission or the D-run entry.
                    assert_eq!(context.require_grammar(), Err(Error::Typed));
                    assert_eq!(context.g4_email_text(id, value), Err(Error::Typed));
                    assert_eq!(context.event(), Err(Error::Typed));
                    assert!(matches!(context.host.reserve(1), Err(Error::Typed)));
                    Err::<(), _>(Error::Typed)
                },
            );
            assert_eq!(result, Err(Error::Typed));
            assert_eq!(context.final_phase, super::FinalPhase::Failed);
        }
        // The same watched pointers were observed by System::dealloc, not
        // inferred from lease-zero, capacity, AST detach, or a mock drop flag.
        let memory = UnwindAllocationScope::snapshot();
        let count = if cut == UnwindCut::DRunGrowth { 2 } else { 1 };
        for backing in memory.backings.iter().take(count) {
            assert_ne!(backing.pointer, 0);
            assert!(backing.freed > 0 && backing.freed < backing.lease_dropped);
            assert_eq!(backing.lease_drops, 1);
        }
        assert_eq!(host.live.get(), 0);
        assert_eq!(host.fault, Some(Error::Typed));
        assert!(host.cancelled);
        assert_eq!(host.finish(Ok(())), Err(Error::Typed));
        drop(watched.take().expect("real parser finish armed the cut"));
    });
}

#[test]
fn g4_grammar_transport_growth_unwind_frees_real_buffers_before_their_leases() {
    actual_mutation_unwind(UnwindCut::DRunGrowth);
}

#[test]
fn narrow_mapping_partial_owner_unwind_stays_in_context_until_actual_drop() {
    actual_mutation_unwind(UnwindCut::MappingCommitted);
}

#[test]
fn consumed_table_lines_never_grant_blank_line_authority_but_real_blanks_do() {
    // Fixed source/expected pairs are independent of the parser's emitted
    // claims. All offsets here are P/LF bytes; the separate full model oracle
    // exercises CRLF transport and every delimiter atom/authority/query.
    for (source, blank, indent) in [
        ("|h|\n|--|\n|a|\n\n", (13, 14), None),
        ("> |h|\n> |--|\n> |a|\n>\n", (19, 21), None),
        ("- |h|\n  |--|\n  |a|\n\n", (19, 20), None),
        ("|h|\n|--|\n  |a|\n\n", (15, 16), Some((9, 11))),
    ] {
        with_input(source, |input| {
            let mut host = TestHost::new(&input);
            {
                let mut context = super::Context::new_direct(&mut host, source);
                let mut options = crate::Options::default();
                options.extension.table = true;
                options.extension.tasklist = true;
                options.extension.strikethrough = true;
                crate::parser::parse_in_context(source, &options, &mut context, |_, context| {
                    let mut blanks = Vec::new();
                    let mut delimiters = 0;
                    let mut indents = Vec::new();
                    for index in 0..context.mappings.claims.len() {
                        let claim = context
                            .mappings
                            .claims
                            .get(index, |bytes| super::rows::charge_host(context.host, bytes))?
                            .unwrap();
                        match claim.kind {
                            super::mapped::ClaimKind::Syntax(super::SyntaxRole::BlankLine) => {
                                let super::mapped::ClaimSource::P(span) = claim.source else {
                                    panic!("blank authority must retain its P source")
                                };
                                blanks.push((span.start, span.end));
                            }
                            super::mapped::ClaimKind::Syntax(super::SyntaxRole::TableDelimiter) => {
                                delimiters += 1;
                            }
                            super::mapped::ClaimKind::Syntax(
                                super::SyntaxRole::BlockWhitespace,
                            ) => {
                                let kind = context.mappings.g3.kind(claim.owner, |bytes| {
                                    super::rows::charge_host(context.host, bytes)
                                })?;
                                if kind == Some(super::NativeKind::TableRow) {
                                    let super::mapped::ClaimSource::P(span) = claim.source else {
                                        panic!("accepted row indentation keeps its P source")
                                    };
                                    indents.push((span.start, span.end));
                                }
                            }
                            _ => {}
                        }
                    }
                    assert_eq!(delimiters, 1, "actual table must be accepted");
                    assert_eq!(
                        indents,
                        indent.into_iter().collect::<Vec<_>>(),
                        "only accepted row indentation has Row authority"
                    );
                    assert_eq!(
                        blanks,
                        vec![blank],
                        "only the real blank source is authorized"
                    );
                    Ok(())
                })
                .unwrap();
            }
            assert_eq!(host.live.get(), 0);
            assert_eq!(host.fault, None);
        });
    }
}
