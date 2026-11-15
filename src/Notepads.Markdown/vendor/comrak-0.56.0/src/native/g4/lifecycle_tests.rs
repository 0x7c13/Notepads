// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Prewritten finite G4 native failure/cancellation/permit-drop tests.
//!
//! Both literal inputs and complete final trees below precede execution. The
//! real per-leaf driver finishes the first email commit before starting A for
//! the second paragraph/cell; its first committed AST/store remains alive.
//! No production phase observer or hooks are added. These tests do not claim
//! a dynamic observation of that intermediate phase or physical allocator free.
//!
//! Each axis/mode/input sweeps consecutive invocation cuts until its first
//! actual success, with a hard test-only attempt bound. Address-based registry
//! hashing can change invocation counts between runs: only reached cuts and
//! that first success are certified, not every allocation layout or phase.
//! The typed mode latches an error THEN raises the same cancellation flag at
//! that cut; this checks earlier typed-error precedence, not priority over a
//! cancellation that was already pending. Controlled cancellation is not a
//! natural 10ms result. No clock, product caps, fee rates or budgets are changed.
//!
//! Book/Arc/Vec/BTreeMap storage is unmetered test observer memory. Permit Drop
//! identities certify lease release, not measured physical-memory reclamation.
//! The driver exercises real AST drop, retained marking, retirement and Context
//! drop, but does not construct Core AcceptedModel/query/package outputs. No
//! panic/unwind injection, phase coverage or exhaustive work-axis claim is made.

use crate::native::{
    AcceptedFact, Context, DecodeKind, FinalPhase, Host, InlinePolicy, NativeAdmission,
    NativeViolation, WorkDelta,
    g4::{ExtendedKind, test_values},
    mapped::TransformKind,
    rows::{NativeRows, charge_host},
};
use crate::{
    Options,
    nodes::{Node, NodeValue, TableAlignment},
    parser::parse_in_context,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Axis {
    Reserve,
    Actual,
    Copy,
}
impl Axis {
    fn index(self) -> usize {
        match self {
            Self::Reserve => 0,
            Self::Actual => 1,
            Self::Copy => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Cancel,
    TypedThenCancel,
}
#[derive(Clone, Copy, Debug)]
struct Cut {
    axis: Axis,
    at: usize,
    mode: Mode,
}
#[derive(Debug, Eq, PartialEq)]
enum Fault {
    Injected(Axis, usize),
    Cancelled,
    Rejected(NativeViolation),
}
type Error = Arc<Fault>;

#[derive(Default)]
struct Book {
    next: u64,
    live: BTreeMap<u64, u64>,
    dropped: u64,
}
struct Permit {
    id: u64,
    book: Rc<RefCell<Book>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut book = self.book.borrow_mut();
        assert!(book.live.remove(&self.id).is_some(), "double/unknown drop");
        book.dropped = book.dropped.checked_add(1).unwrap();
    }
}
struct TestHost {
    book: Rc<RefCell<Book>>,
    first_fault: Option<Error>,
    cancelled: Arc<AtomicBool>,
    cut: Option<Cut>,
    calls: [usize; 3],
    reached: bool,
    verifying: bool,
}
impl TestHost {
    fn new(cut: Option<Cut>) -> Self {
        Self {
            book: Rc::new(RefCell::new(Book::default())),
            first_fault: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            cut,
            calls: [0; 3],
            reached: false,
            verifying: false,
        }
    }
    fn fail(&mut self, fault: Fault) -> Error {
        self.first_fault
            .get_or_insert_with(|| Arc::new(fault))
            .clone()
    }
    fn checkpoint(&mut self) -> Result<(), Error> {
        if let Some(first) = &self.first_fault {
            return Err(first.clone());
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(self.fail(Fault::Cancelled));
        }
        Ok(())
    }
    fn touch(&mut self, axis: Axis) -> Result<(), Error> {
        self.checkpoint()?;
        self.calls[axis.index()] = self.calls[axis.index()].checked_add(1).unwrap();
        if let Some(cut) = self.cut
            && cut.axis == axis
            && cut.at == self.calls[axis.index()]
        {
            assert!(!self.reached);
            self.reached = true;
            if cut.mode == Mode::TypedThenCancel {
                let _ = self.fail(Fault::Injected(axis, cut.at));
            }
            self.cancelled.store(true, Ordering::Release);
        }
        self.checkpoint()
    }
}
impl Host for TestHost {
    type Error = Error;
    type Lease = Permit;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Error> {
        match delta {
            // The fixture's own oracle reads are not producer failure cuts.
            WorkDelta::Copy(_) if !self.verifying => self.touch(Axis::Copy),
            WorkDelta::Copy(_) => self.checkpoint(),
            WorkDelta::Events(_) | WorkDelta::Scan(_) => self.checkpoint(),
        }
    }
    fn admit(&mut self, _: NativeAdmission) -> Result<(), Error> {
        self.checkpoint()
    }
    fn reserve(&mut self, bytes: u64) -> Result<Permit, Error> {
        self.touch(Axis::Reserve)?;
        let mut book = self.book.borrow_mut();
        book.next = book.next.checked_add(1).unwrap();
        let id = book.next;
        assert!(book.live.insert(id, bytes).is_none());
        Ok(Permit {
            id,
            book: self.book.clone(),
        })
    }
    fn admit_actual(&mut self, permit: &mut Permit, bytes: u64) -> Result<(), Error> {
        // The caller already owns the real allocation and this exact permit.
        // Failing here must unwind both, including before registry registration.
        assert!(Rc::ptr_eq(&self.book, &permit.book));
        assert!(self.book.borrow().live.contains_key(&permit.id));
        self.touch(Axis::Actual)?;
        *self.book.borrow_mut().live.get_mut(&permit.id).unwrap() = bytes;
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Error> {
        // Strict direct parsing must never fall back to an acceptance tape.
        Err(self.reject(NativeViolation::InternalInvariant))
    }
    fn reject(&mut self, violation: NativeViolation) -> Error {
        self.fail(Fault::Rejected(violation))
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Document,
    Paragraph,
    Table,
    Header,
    Cell,
    Link(&'static str),
    Text(&'static str),
}
struct Expected {
    parent: Option<usize>,
    kind: Kind,
}
const fn node(parent: Option<usize>, kind: Kind) -> Expected {
    Expected { parent, kind }
}
struct Case {
    name: &'static str,
    source: &'static str,
    tree: &'static [Expected],
    links: [(usize, usize); 2],
}
const CASES: [Case; 2] = [
    Case {
        name: "two leaves, first decoded commit retained",
        source: "a&#64;b.com\n\nz@c.net",
        tree: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Paragraph),
            node(Some(1), Kind::Link("mailto:a@b.com")),
            node(Some(2), Kind::Text("a@b.com")),
            node(Some(0), Kind::Paragraph),
            node(Some(4), Kind::Link("mailto:z@c.net")),
            node(Some(5), Kind::Text("z@c.net")),
        ],
        links: [(2, 3), (5, 6)],
    },
    Case {
        name: "two cells, first decoded commit retained",
        source: "| a&#64;b.com | z@c.net |\n| --- | --- |",
        tree: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Table),
            node(Some(1), Kind::Header),
            node(Some(2), Kind::Cell),
            node(Some(3), Kind::Link("mailto:a@b.com")),
            node(Some(4), Kind::Text("a@b.com")),
            node(Some(2), Kind::Cell),
            node(Some(6), Kind::Link("mailto:z@c.net")),
            node(Some(7), Kind::Text("z@c.net")),
        ],
        links: [(4, 5), (7, 8)],
    },
];

fn walk<'a>(node: Node<'a>, output: &mut Vec<Node<'a>>) {
    output.push(node);
    for child in node.children() {
        walk(child, output);
    }
}
fn row<'a, T>(
    rows: &'a NativeRows<T, Permit>,
    at: usize,
    host: &mut TestHost,
) -> Result<&'a T, Error> {
    rows.get(at, |bytes| charge_host(host, bytes))?
        .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))
}
fn verify(root: Node<'_>, ctx: &mut Context<'_, TestHost>, case: &Case) -> Result<(), Error> {
    let mut nodes = Vec::new();
    walk(root, &mut nodes);
    assert_eq!(nodes.len(), case.tree.len(), "{}: complete AST", case.name);
    for (index, expected) in case.tree.iter().enumerate() {
        match expected.parent {
            Some(parent) => assert!(nodes[index].parent().unwrap().same_node(nodes[parent])),
            None => assert!(nodes[index].parent().is_none()),
        }
        let data = nodes[index].data();
        match (expected.kind, &data.value) {
            (Kind::Document, NodeValue::Document)
            | (Kind::Paragraph, NodeValue::Paragraph)
            | (Kind::Cell, NodeValue::TableCell) => {}
            (Kind::Header, NodeValue::TableRow(header)) => assert!(*header),
            (Kind::Table, NodeValue::Table(table)) => {
                assert_eq!(table.num_columns, 2);
                assert_eq!(table.num_rows, 1);
                assert_eq!(table.num_nonempty_cells, 2);
                assert_eq!(table.alignments.as_slice(), &[TableAlignment::None; 2]);
            }
            (Kind::Link(url), NodeValue::Link(link)) => {
                assert_eq!(test_values::text(ctx, &link.url)?, url);
                assert!(test_values::text(ctx, &link.title)?.is_empty());
            }
            (Kind::Text(text), NodeValue::Text(actual)) => {
                assert_eq!(test_values::text(ctx, actual)?, text)
            }
            _ => panic!("{}: node {index} is {:?}", case.name, data.value),
        }
    }
    // Bind IDs only after the independently written tree/value/parent checks.
    let map = &ctx.mappings;
    let host = &mut *ctx.host;
    assert_eq!(
        map.proofs.len(),
        1,
        "one parent Entity; shadow stays private"
    );
    assert_eq!(map.payloads.len(), 2);
    assert_eq!(ctx.definitions.len(), 0);
    let mut buffers = Vec::new();
    let mut destinations = Vec::new();
    for (ordinal, &(link, label)) in case.links.iter().enumerate() {
        let link_id = nodes[link].data().native_id.unwrap();
        let label_id = nodes[label].data().native_id.unwrap();
        let binding = row(&map.nodes, link_id.get() as usize - 1, host)?;
        let syntax = row(
            &map.syntax,
            binding.syntax.unwrap().get() as usize - 1,
            host,
        )?;
        let extended = syntax.extended.expect("real email acceptance");
        assert_eq!(extended.kind, ExtendedKind::Email);
        assert_eq!(extended.node, link_id);
        assert_eq!(extended.label, label_id);
        assert_eq!(extended.raw.start, 0);
        assert_eq!(extended.raw.end, if ordinal == 0 { 11 } else { 7 });
        assert_eq!(syntax.syntax, Some(extended.raw));
        assert_eq!(syntax.label, Some(extended.raw));
        assert_eq!(syntax.destination, Some(extended.destination));
        assert_eq!(syntax.title, None);
        assert_eq!(syntax.definition, None);
        assert!(!syntax.reference_use);
        buffers.push(extended.raw.buffer);
        destinations.push(extended.destination);
    }
    assert_ne!(buffers[0], buffers[1], "leaf/cell buffers stay distinct");
    assert_ne!(destinations[0], destinations[1]);
    let proof = row(&map.proofs, 0, host)?;
    assert_eq!(proof.kind, TransformKind::Decode(DecodeKind::Entity));
    assert_eq!(proof.result, "@");
    assert_eq!(proof.stage, 1);
    assert_eq!((proof.local_start, proof.local_end), (0, 5));
    assert_eq!(proof.input.len(), 1);
    let input = proof.input[0];
    assert_eq!(input.source.buffer, buffers[0]);
    assert_eq!((input.source.start, input.source.end), (1, 6));
    assert_eq!(input.bytes, 5);
    assert_eq!(input.proof, None);
    assert!(!input.hidden_syntax);
    Ok(())
}

// Runs the real grammar once. No preliminary run supplies cut counts or output
// expected values. As in direct::compile, marking precedes the actual AST drop
// and retirement follows it. There is no Core finalizer or published model here.
fn attempt(case: &Case, host: &mut TestHost) -> Result<(), Error> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = false;
    let mut ctx = Context::new_direct(host, case.source);
    ctx.inline_policy = InlinePolicy::StrictG4;
    let result = parse_in_context(case.source, &options, &mut ctx, |root, ctx| {
        ctx.flush_scan()?;
        ctx.mappings.g3.validate(ctx.host)?;
        ctx.final_phase = FinalPhase::Active;
        ctx.host.verifying = true;
        let verified = verify(root, ctx, case);
        ctx.host.verifying = false;
        verified?;
        ctx.prepare_ast_retirement()
    })
    .and_then(|()| ctx.retire_ast())
    .and_then(|()| ctx.host.checkpoint());
    if let Err(first) = &result {
        assert!(Arc::ptr_eq(first, ctx.host.first_fault.as_ref().unwrap()));
        let before = ctx.host.book.borrow().live.clone();
        let mut forbidden = String::new();
        let later = ctx.append(&mut forbidden, "after fault").unwrap_err();
        assert!(
            Arc::ptr_eq(first, &later),
            "append must retain the first fault"
        );
        assert!(forbidden.is_empty());
        assert_eq!(forbidden.capacity(), 0);
        assert_eq!(ctx.host.book.borrow().live, before);
    }
    // Context's real mappings/definitions/registry drop before returning.
    drop(ctx);
    result
}

fn assert_released(host: &TestHost) {
    let book = host.book.borrow();
    assert!(book.live.is_empty(), "native permit identities leaked");
    assert_eq!(
        book.dropped, book.next,
        "every issued permit actually dropped"
    );
}

fn sweep(axis: Axis) {
    const MAX_CUT: usize = 8192;
    for case in &CASES {
        for mode in [Mode::Cancel, Mode::TypedThenCancel] {
            let mut failures = 0;
            let mut success = None;
            for at in 1..=MAX_CUT {
                let mut host = TestHost::new(Some(Cut { axis, at, mode }));
                let result = attempt(case, &mut host);
                assert_released(&host);
                match result {
                    Err(error) => {
                        assert!(host.reached, "unrelated failure: {axis:?}/{at}: {error:?}");
                        assert_eq!(host.calls[axis.index()], at);
                        assert!(host.cancelled.load(Ordering::Acquire));
                        let expected = match mode {
                            Mode::Cancel => Fault::Cancelled,
                            Mode::TypedThenCancel => Fault::Injected(axis, at),
                        };
                        assert_eq!(*error, expected);
                        assert!(Arc::ptr_eq(&error, host.first_fault.as_ref().unwrap()));
                        let later = host.checkpoint().unwrap_err();
                        assert!(Arc::ptr_eq(&error, &later));
                        let rejected = host.reject(NativeViolation::InternalInvariant);
                        assert!(Arc::ptr_eq(&error, &rejected));
                        failures += 1;
                    }
                    Ok(()) => {
                        assert!(!host.reached, "reached failure was ignored");
                        assert!(host.first_fault.is_none());
                        assert!(!host.cancelled.load(Ordering::Acquire));
                        assert!(host.calls[axis.index()] < at);
                        assert!(host.book.borrow().next > 0);
                        success = Some((at, host.calls));
                        break;
                    }
                }
            }
            assert!(failures > 0, "{axis:?}/{mode:?}: no reached failing cut");
            let (at, calls) = success.unwrap_or_else(|| {
                panic!(
                    "{}: {axis:?}/{mode:?} no success by cut {MAX_CUT}",
                    case.name
                )
            });
            eprintln!(
                "G4 finite {} {axis:?}/{mode:?}: reached failures={failures}, \
                 first success cut={at}, actual reserve/actual/copy calls={calls:?}; \
                 address-sensitive, not all layouts or natural cancellation",
                case.name,
            );
        }
    }
}

#[test]
fn two_leaf_and_cell_success_keep_exact_tree_and_one_original_entity() {
    for case in &CASES {
        let mut host = TestHost::new(None);
        attempt(case, &mut host).unwrap();
        assert!(host.first_fault.is_none());
        assert!(!host.cancelled.load(Ordering::Acquire));
        assert!(host.calls.iter().all(|&count| count > 0));
        assert!(host.book.borrow().next > 0);
        assert_released(&host);
    }
}

#[test]
fn reserve_cuts_keep_first_fault_and_drop_all_issued_permits() {
    sweep(Axis::Reserve);
}

#[test]
fn actual_allocation_cuts_keep_first_fault_and_drop_all_issued_permits() {
    sweep(Axis::Actual);
}

#[test]
fn copy_cuts_keep_first_fault_and_drop_all_issued_permits() {
    sweep(Axis::Copy);
}
