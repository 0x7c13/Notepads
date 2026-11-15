// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual controlled SVG parsing through the stock, non-FontReplay bidi path.

use crate::{FontResolver, Group, Node, Options, ProcessingContext, Tree};
use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    BeforeConstructor,
    BeforeVisualRuns,
    AfterVisualRuns,
}

impl Stage {
    fn index(self) -> usize {
        match self {
            Self::BeforeConstructor => 0,
            Self::BeforeVisualRuns => 1,
            Self::AfterVisualRuns => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Reserve,
    Commit,
    Checkpoint,
    ConstructorOwnerRelease,
    VisualResultRelease,
}

#[derive(Clone, Copy, Debug)]
struct Fault {
    invocation: usize,
    stage: Stage,
    operation: Operation,
    ordinal: usize,
    error: Error,
}

#[derive(Clone, Debug, Default)]
struct StageCounts {
    reserves: Vec<usize>,
    commits: Vec<usize>,
    checkpoints: Vec<u64>,
    constructor_owner_releases: Vec<usize>,
    visual_result_releases: Vec<usize>,
}

impl StageCounts {
    fn count(&self, operation: Operation) -> usize {
        match operation {
            Operation::Reserve => self.reserves.len(),
            Operation::Commit => self.commits.len(),
            Operation::Checkpoint => self.checkpoints.len(),
            Operation::ConstructorOwnerRelease => self.constructor_owner_releases.len(),
            Operation::VisualResultRelease => self.visual_result_releases.len(),
        }
    }

    fn record(&mut self, operation: Operation, amount: u64) -> usize {
        match operation {
            Operation::Reserve => self.reserves.push(amount as usize),
            Operation::Commit => self.commits.push(amount as usize),
            Operation::Checkpoint => self.checkpoints.push(amount),
            Operation::ConstructorOwnerRelease => {
                self.constructor_owner_releases.push(amount as usize)
            }
            Operation::VisualResultRelease => self.visual_result_releases.push(amount as usize),
        }
        self.count(operation)
    }
}

#[derive(Clone, Copy, Debug)]
struct Live {
    kind: ResourceKind,
    bytes: usize,
    origin: Option<Stage>,
    committed: bool,
}

#[derive(Default)]
struct Ledger {
    next: u32,
    live: BTreeMap<u32, Live>,
    // Canonical failed-release tickets remain owned here; never retry/refund.
    failed_release_tickets: Vec<Ticket>,
    stage: Option<Stage>,
    first_invocation: bool,
    gates: [usize; 3],
    counts: [StageCounts; 3],
    fault: Option<Fault>,
    hit: Option<Fault>,
    post_fault_reserves: usize,
    post_fault_commits: usize,
    post_fault_checkpoints: usize,
    geometry_reserves: usize,
    geometry_commits: usize,
    geometry_releases: usize,
}

impl Ledger {
    /// The two entry gates delimit the actual unicode-bidi calls. The last
    /// stage remains installed while callers consume the paid visual owners;
    /// unrelated later geometry must not become a bidi fault ordinal.
    fn record_bidi(
        &mut self,
        kind: ResourceKind,
        operation: Operation,
        amount: u64,
    ) -> Result<(), Error> {
        if self.hit.is_some()
            || (kind == ResourceKind::Scratch && self.stage != Some(Stage::AfterVisualRuns))
        {
            self.record(operation, amount)
        } else {
            Ok(())
        }
    }

    fn record(&mut self, operation: Operation, amount: u64) -> Result<(), Error> {
        if let Some(hit) = self.hit {
            match operation {
                Operation::Reserve => self.post_fault_reserves += 1,
                Operation::Commit => self.post_fault_commits += 1,
                Operation::Checkpoint => self.post_fault_checkpoints += 1,
                _ => return Ok(()), // Complete real cleanup after the first fault.
            }
            return Err(hit.error);
        }
        if !self.first_invocation {
            return Ok(());
        }
        let Some(stage) = self.stage else {
            return Ok(());
        };
        let ordinal = self.counts[stage.index()].record(operation, amount);
        if let Some(fault) = self.fault {
            if fault.stage == stage && fault.operation == operation && fault.ordinal == ordinal {
                self.hit = Some(fault);
                return Err(fault.error);
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Probe {
    ledger: Mutex<Ledger>,
    fallback_calls: AtomicUsize,
    primary_calls: AtomicUsize,
}

struct Host(Arc<Probe>);

impl Backend for Host {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        assert!(matches!(
            kind,
            ResourceKind::Scratch | ResourceKind::Geometry
        ));
        let mut ledger = self.0.ledger.lock().unwrap();
        ledger.record_bidi(kind, Operation::Reserve, bytes as u64)?;
        if kind == ResourceKind::Geometry {
            ledger.geometry_reserves += 1;
        }
        ledger.next += 1;
        let slot = ledger.next;
        let origin = if kind == ResourceKind::Scratch && ledger.first_invocation {
            ledger.stage
        } else {
            None
        };
        assert!(ledger
            .live
            .insert(
                slot,
                Live {
                    kind,
                    bytes,
                    origin,
                    committed: false
                }
            )
            .is_none());
        Ticket::new(712, slot, 1)
    }

    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let mut ledger = self.0.ledger.lock().unwrap();
        let live = ledger.live[&ticket.slot()];
        assert_eq!(live.bytes, bytes);
        ledger.record_bidi(live.kind, Operation::Commit, bytes as u64)?;
        if live.kind == ResourceKind::Geometry {
            ledger.geometry_commits += 1;
        }
        ledger.live.get_mut(&ticket.slot()).unwrap().committed = true;
        Ok(())
    }

    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let mut ledger = self.0.ledger.lock().unwrap();
        let live = ledger.live[&ticket.slot()];
        let operation = match (live.origin, ledger.stage, ledger.first_invocation) {
            (Some(Stage::BeforeConstructor), Some(Stage::BeforeVisualRuns), true) => {
                Some(Operation::ConstructorOwnerRelease)
            }
            (Some(Stage::BeforeVisualRuns), Some(Stage::AfterVisualRuns), true) => {
                Some(Operation::VisualResultRelease)
            }
            _ => None,
        };
        if let Some(operation) = operation {
            if let Err(error) = ledger.record(operation, live.bytes as u64) {
                assert!(
                    live.committed,
                    "release fault must follow real allocation admission"
                );
                ledger.failed_release_tickets.push(ticket);
                return Err(error);
            }
        }
        assert!(ledger.live.remove(&ticket.slot()).is_some());
        if live.kind == ResourceKind::Geometry {
            ledger.geometry_releases += 1;
        }
        Ok(())
    }

    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.0.ledger.lock().unwrap().record_bidi(
            ResourceKind::Scratch,
            Operation::Checkpoint,
            work,
        )
    }
}

struct Hook {
    request: ResourcesRef,
    probe: Arc<Probe>,
}

thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

struct HookGuard;

impl HookGuard {
    fn install(request: &ResourcesRef, probe: &Arc<Probe>) -> Self {
        HOOK.with(|slot| {
            let mut slot = slot.borrow_mut();
            assert!(slot.is_none());
            *slot = Some(Hook {
                request: request.clone(),
                probe: probe.clone(),
            });
        });
        Self
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

/// Observation/arming only. The backend rejects the real following operation;
/// the gate does not pay work, record a fault, or change the production result.
pub(super) fn gate(stage: Stage, request: &ResourcesRef) {
    HOOK.with(|slot| {
        let slot = slot.borrow();
        let Some(hook) = slot.as_ref() else { return };
        assert!(request.is_same_request(&hook.request));
        let mut ledger = hook.probe.ledger.lock().unwrap();
        ledger.gates[stage.index()] += 1;
        ledger.stage = Some(stage);
        let target_invocation = ledger.fault.map_or(1, |fault| fault.invocation);
        ledger.first_invocation = ledger.gates[stage.index()] == target_invocation;
    });
}

fn request(fault: Option<Fault>) -> (ResourcesRef, Arc<Probe>) {
    let probe = Arc::new(Probe::default());
    probe.ledger.lock().unwrap().fault = fault;
    let request = ResourcesRef::new_prepaid(
        Host(probe.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    (request, probe)
}

fn options_with_fallback(
    request: &ResourcesRef,
    probe: &Arc<Probe>,
    reshape_fallback: bool,
) -> Options<'static> {
    // Existing fixed fontdb test font: real cmap/outline data, no system fonts.
    let mut db = fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf").to_vec(),
    );
    let id = db.faces().next().expect("Tuffy fixture face").id;
    let fallback_id = if reshape_fallback {
        db.load_font_data(
            include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf").to_vec(),
        );
        let second = db
            .faces()
            .find(|face| face.id != id)
            .expect("second Tuffy face")
            .id;
        assert_ne!(id, second);
        Some(second)
    } else {
        None
    };
    let primary = probe.clone();
    let fallback = probe.clone();
    Options {
        fontdb: Arc::new(db),
        font_replay: None,
        processing: Arc::new(ProcessingContext::with_resources(request.clone())),
        font_resolver: FontResolver {
            select_font: crate::FontSelectionFn::Owned(Box::new(move |_, _| {
                primary.primary_calls.fetch_add(1, Ordering::Relaxed);
                Some(id)
            })),
            select_fallback: crate::FallbackSelectionFn::Owned(Box::new(move |_, used, _| {
                fallback.fallback_calls.fetch_add(1, Ordering::Relaxed);
                fallback_id.filter(|id| !used.contains(id))
            })),
        },
        ..Options::default()
    }
}

fn options(request: &ResourcesRef, probe: &Arc<Probe>) -> Options<'static> {
    options_with_fallback(request, probe, false)
}

fn document() -> String {
    let text = "A(AB) אב 12 \u{2066}BA\u{2069} 😀 ".repeat(48);
    assert!(text.chars().count() > 600);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='800' height='80'><text x='4' y='30' font-family='Tuffy' font-size='16'>{text}</text></svg>"
    )
}

fn rendered_paths(group: &Group) -> usize {
    group
        .children()
        .iter()
        .map(|node| match node {
            Node::Path(path) => usize::from(!path.data().is_empty()),
            Node::Group(group) => rendered_paths(group),
            Node::Text(text) => rendered_paths(text.flattened()),
            Node::Image(_) => 0,
        })
        .sum()
}

fn assert_retired(request: &ResourcesRef, probe: &Probe) {
    assert!(probe.ledger.lock().unwrap().live.is_empty());
    assert!(probe
        .ledger
        .lock()
        .unwrap()
        .failed_release_tickets
        .is_empty());
    let stats = request.statistics();
    assert_eq!(stats.scratch_bytes, 0);
    assert_eq!(stats.live_leases, 0);
    assert_eq!(stats.release_failures, 0);
    request.validate_retired().unwrap();
    let ledger = probe.ledger.lock().unwrap();
    assert_eq!(ledger.geometry_reserves, ledger.geometry_commits);
    assert_eq!(ledger.geometry_reserves, ledger.geometry_releases);
}

fn success() -> [StageCounts; 3] {
    let (request, probe) = request(None);
    let options = options(&request, &probe);
    assert!(options.font_replay.is_none());
    let hook = HookGuard::install(&request, &probe);
    let tree = Tree::from_data_controlled(document().as_bytes(), &options).unwrap();
    assert!(tree.font_replay.is_none());
    assert!(
        rendered_paths(tree.root()) > 0,
        "stock text must produce real outline paths"
    );
    assert!(Arc::ptr_eq(tree.processing_context(), &options.processing));
    assert!(probe.primary_calls.load(Ordering::Relaxed) > 0);
    assert!(
        probe.fallback_calls.load(Ordering::Relaxed) > 0,
        "missing-glyph fallback positive control"
    );
    assert!(request.failure().is_none());
    let counts = {
        let ledger = probe.ledger.lock().unwrap();
        assert!(ledger.gates.iter().all(|count| *count > 0));
        assert!(ledger.hit.is_none());
        ledger.counts.clone()
    };
    // Actual first-stage full-level copying crossed more than one 256-item window.
    assert!(
        counts[1]
            .checkpoints
            .iter()
            .filter(|work| **work == 256)
            .count()
            >= 2
    );
    assert!(!counts[1].constructor_owner_releases.is_empty());
    assert!(!counts[2].visual_result_releases.is_empty());
    for stage in [Stage::BeforeConstructor, Stage::BeforeVisualRuns] {
        let c = &counts[stage.index()];
        println!(
            "STOCK_BIDI_BASELINE stage={stage:?} reserves={} commits={} checkpoints={} positive_work={} max_work={} constructor_owner_releases={} visual_result_releases={}",
            c.reserves.len(),
            c.commits.len(),
            c.checkpoints.len(),
            c.checkpoints.iter().filter(|work| **work != 0).count(),
            c.checkpoints.iter().copied().max().unwrap_or(0),
            c.constructor_owner_releases.len(),
            c.visual_result_releases.len()
        );
    }
    println!(
        "STOCK_BIDI_BASELINE stage=AfterVisualRuns visual_result_releases={}",
        counts[2].visual_result_releases.len()
    );
    drop(tree);
    drop(hook);
    drop(options);
    assert_retired(&request, &probe);
    let ledger = probe.ledger.lock().unwrap();
    assert!(
        ledger.geometry_reserves > 0,
        "actual outline paths must retain their new Geometry tickets"
    );
    println!(
        "STOCK_BIDI_OTHER_GEOMETRY reserves={} commits={} releases={} live=0",
        ledger.geometry_reserves, ledger.geometry_commits, ledger.geometry_releases
    );
    counts
}

fn run_fault(fault: Fault) {
    let (request, probe) = request(Some(fault));
    let options = options_with_fallback(&request, &probe, fault.invocation == 2);
    assert!(options.font_replay.is_none());
    let hook = HookGuard::install(&request, &probe);
    let result = Tree::from_data_controlled(document().as_bytes(), &options);
    assert!(
        result.is_err(),
        "fault was converted to a successful Tree: {fault:?}"
    );
    assert_eq!(request.failure(), Some(fault.error));
    assert_eq!(options.processing.resource_failure(), Some(fault.error));
    match result.as_ref().unwrap_err() {
        crate::Error::ProcessingFailure(error) => assert_eq!(error.typed_failure, fault.error),
        crate::Error::Resource(error) => assert_eq!(*error, fault.error),
        error => panic!("bidi resource error lost its typed transport: {error:?}"),
    }
    assert_eq!(
        probe.fallback_calls.load(Ordering::Relaxed),
        fault.invocation - 1,
        "no missing-glyph fallback after bidi failure"
    );
    {
        let ledger = probe.ledger.lock().unwrap();
        let hit = ledger
            .hit
            .expect("fault must hit its targeted real bidi operation");
        assert_eq!(
            (hit.stage, hit.operation, hit.ordinal),
            (fault.stage, fault.operation, fault.ordinal)
        );
        assert_eq!(ledger.gates[0], fault.invocation);
        assert_eq!(
            ledger.gates[1],
            fault.invocation - 1 + usize::from(fault.stage != Stage::BeforeConstructor)
        );
        assert_eq!(
            ledger.gates[2],
            fault.invocation - 1 + usize::from(fault.stage == Stage::AfterVisualRuns)
        );
        assert_eq!(ledger.post_fault_reserves, 0);
        assert_eq!(ledger.post_fault_commits, 0);
        assert_eq!(ledger.post_fault_checkpoints, 0);
    }
    // Later wrappers must not replace the original kind/limit/native record.
    options
        .processing
        .fail(crate::ProcessingFailure::FontReplay);
    assert_eq!(request.record(Error::native(-7999)), fault.error);
    assert_eq!(options.processing.resource_failure(), Some(fault.error));
    drop(result);
    drop(hook);
    drop(options);
    if matches!(
        fault.operation,
        Operation::ConstructorOwnerRelease | Operation::VisualResultRelease
    ) {
        let ledger = probe.ledger.lock().unwrap();
        assert_eq!(ledger.failed_release_tickets.len(), 1);
        assert_eq!(ledger.live.len(), 1);
        let ticket = &ledger.failed_release_tickets[0];
        assert_eq!(ledger.live[&ticket.slot()].kind, ResourceKind::Scratch);
        assert_eq!(ledger.geometry_reserves, ledger.geometry_commits);
        assert_eq!(ledger.geometry_reserves, ledger.geometry_releases);
        let bytes = ledger.live[&ticket.slot()].bytes as u64;
        let stats = request.statistics();
        assert_eq!(stats.release_failures, 1);
        assert_eq!(stats.live_leases, 1);
        assert_eq!(stats.scratch_bytes, bytes);
        drop(ledger);
        assert!(
            request.validate_retired().is_err(),
            "failed release is not successful retirement"
        );
        assert_eq!(request.failure(), Some(fault.error));
    } else {
        assert_retired(&request, &probe);
    }
}

#[test]
fn actual_stock_bidi_success_and_every_first_stage_reserve_commit_fault() {
    let counts = success();
    let mut cases = 0;
    for stage in [Stage::BeforeConstructor, Stage::BeforeVisualRuns] {
        for operation in [Operation::Reserve, Operation::Commit] {
            let count = counts[stage.index()].count(operation);
            assert!(count > 0);
            for ordinal in 1..=count {
                let error = match operation {
                    Operation::Reserve => Error::new(FailureKind::Quota, LimitKind::Scratch),
                    Operation::Commit => Error::native(-7131),
                    _ => unreachable!(),
                };
                run_fault(Fault {
                    invocation: 1,
                    stage,
                    operation,
                    ordinal,
                    error,
                });
                cases += 1;
            }
            println!(
                "STOCK_BIDI_FULL_MATRIX stage={stage:?} operation={operation:?} actual_ordinal_denominator={count} executed={count}"
            );
        }
    }
    println!("STOCK_BIDI_FULL_MATRIX total_cases={cases}");
}

#[test]
fn actual_stock_bidi_later_work_windows_preserve_cancelled_and_work_quota() {
    let counts = success();
    let mut cases = 0;
    for stage in [Stage::BeforeConstructor, Stage::BeforeVisualRuns] {
        let work = &counts[stage.index()].checkpoints;
        assert!(work.len() > 3);
        let mut selected = BTreeSet::from([2, 3, work.len() / 2, work.len()]);
        for (index, _) in work
            .iter()
            .enumerate()
            .filter(|(_, work)| **work == 256)
            .take(2)
        {
            selected.insert(index + 1);
        }
        for ordinal in selected.iter().copied() {
            assert!(ordinal > 1 && ordinal <= work.len());
            for error in [
                Error::new(FailureKind::Cancelled, LimitKind::None),
                Error::new(FailureKind::Quota, LimitKind::Work),
            ] {
                run_fault(Fault {
                    invocation: 1,
                    stage,
                    operation: Operation::Checkpoint,
                    ordinal,
                    error,
                });
                cases += 1;
            }
        }
        println!(
            "STOCK_BIDI_SELECTED_WORK stage={stage:?} actual_checkpoint_denominator={} selected_ordinals={selected:?} typed_variants=2",
            work.len()
        );
    }
    println!("STOCK_BIDI_SELECTED_WORK total_cases={cases}");
}

fn failed_release_case(stage: Stage, operation: Operation) {
    let counts = success();
    let count = counts[stage.index()].count(operation);
    assert!(count > 0);
    // One actual admitted allocation from this release family. Allocation
    // reserve/commit ordinals are exhaustively exercised by the matrix above.
    run_fault(Fault {
        invocation: 1,
        stage,
        operation,
        ordinal: 1,
        error: Error::native(-7132),
    });
    println!(
        "STOCK_BIDI_RELEASE stage={stage:?} operation={operation:?} actual_release_denominator={count} selected_ordinal=1 retained_failed_ticket=1 successful_retirement=false"
    );
}

#[test]
fn actual_stock_bidi_failed_constructor_owner_release_is_not_tree_success() {
    failed_release_case(Stage::BeforeVisualRuns, Operation::ConstructorOwnerRelease);
}

#[test]
fn actual_stock_bidi_failed_visual_result_release_is_not_fallback_success() {
    failed_release_case(Stage::AfterVisualRuns, Operation::VisualResultRelease);
}

#[test]
fn actual_stock_bidi_fallback_reshape_propagates_second_invocation_control_errors() {
    for stage in [Stage::BeforeConstructor, Stage::BeforeVisualRuns] {
        for error in [
            Error::new(FailureKind::Cancelled, LimitKind::None),
            Error::new(FailureKind::Quota, LimitKind::Work),
        ] {
            run_fault(Fault {
                invocation: 2,
                stage,
                operation: Operation::Checkpoint,
                ordinal: 3,
                error,
            });
        }
    }
    println!(
        "STOCK_BIDI_FALLBACK_RESHAPE face_count=2 stage_invocation=2 checkpoint_ordinal=3 typed_variants=2 executed=4"
    );
}

#[test]
fn actual_stock_controlled_empty_svg_text_and_tspan_keep_successful_empty_results() {
    for body in ["", "<text/>", "<text><tspan/></text>"] {
        let (request, probe) = request(None);
        let options = options(&request, &probe);
        let hook = HookGuard::install(&request, &probe);
        let source =
            format!("<svg xmlns='http://www.w3.org/2000/svg' width='80' height='40'>{body}</svg>");
        let tree = Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
        assert!(tree.font_replay.is_none());
        assert_eq!(rendered_paths(tree.root()), 0);
        assert!(request.failure().is_none());
        assert_eq!(probe.ledger.lock().unwrap().gates, [0; 3]);
        drop(tree);
        drop(hook);
        drop(options);
        assert_retired(&request, &probe);
    }
    println!("STOCK_BIDI_EMPTY_INPUTS executed=3");
}

#[test]
fn replay_production_collector_observes_paid_permutation_release_failure() {
    // This calls the actual replay collector with an actual paid permutation.
    // It is a helper test; the stock tests above are the real Tree pipeline.
    struct ReleaseHost {
        inner: Host,
        armed: Arc<std::sync::atomic::AtomicBool>,
        failed: Arc<Mutex<Option<Ticket>>>,
    }
    impl Backend for ReleaseHost {
        fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
            self.inner.reserve(kind, bytes)
        }
        fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
            self.inner.allocation_committed(ticket, bytes)
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
            if self.armed.swap(false, Ordering::Relaxed) {
                assert!(self.failed.lock().unwrap().replace(ticket).is_none());
                return Err(Error::native(-7342));
            }
            self.inner.release_after_free(ticket)
        }
        fn checkpoint(&self, work: u64) -> Result<(), Error> {
            self.inner.checkpoint(work)
        }
    }
    for reject in [false, true] {
        let probe = Arc::new(Probe::default());
        let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let failed = Arc::new(Mutex::new(None));
        let request = ResourcesRef::new_prepaid(
            ReleaseHost {
                inner: Host(probe.clone()),
                armed: armed.clone(),
                failed: failed.clone(),
            },
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        let levels = [
            unicode_bidi::Level::ltr(),
            unicode_bidi::Level::rtl(),
            unicode_bidi::Level::rtl(),
            unicode_bidi::Level::ltr(),
        ];
        let order = unicode_bidi::controlled::try_reorder_visual(&levels, &request).unwrap();
        assert_eq!(request.statistics().live_leases, 1);
        armed.store(reject, Ordering::Relaxed);
        let mut values = [Some(10), Some(20), Some(30), Some(40)];
        let result =
            super::BidiOwnedSlice::Funded(order).collect_visual(&mut values, Some(&request));
        assert!(values.iter().all(Option::is_none));
        if reject {
            let error = Error::native(-7342);
            assert_eq!(result.err(), Some(error));
            assert_eq!(request.failure(), Some(error));
            assert_eq!(
                request.record(Error::new(FailureKind::Quota, LimitKind::Work)),
                error
            );
            assert_eq!(request.statistics().live_leases, 1);
            assert_eq!(request.statistics().release_failures, 1);
            assert!(request.validate_retired().is_err());
            assert!(failed.lock().unwrap().is_some());
            assert_eq!(probe.ledger.lock().unwrap().live.len(), 1);
        } else {
            assert_eq!(result.unwrap(), [10, 30, 20, 40]);
            assert_retired(&request, &probe);
            assert!(failed.lock().unwrap().is_none());
        }
    }
    println!(
        "REPLAY_BIDI_COLLECTOR actual_paid_permutation=true success=1 release_error=1 failed_ticket_retained=true real_tree_pipeline=false"
    );
}
