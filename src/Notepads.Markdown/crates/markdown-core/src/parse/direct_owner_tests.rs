// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent lineage and final-model checks for the real hidden-owner cache.

use super::*;
use crate::identity::{LogicalBoundary, ModelIdentity, SourceBoundary, SourceKey};
use crate::mapping::{
    Affinity, Correspondence, MapIndex, PointInput, PointQuery, RangeInput, RangeQuery,
};
use crate::model::AcceptedModel;
use crate::parse::prepared::NativeFault;
use crate::policy::{CancellationToken, Operation, SourceBudget, SourceLimits, construction_scope};
use crate::source::SourceDocument;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::parse::direct) enum Stage {
    Positive,
    DfsEnter,
    DfsExit,
    BeforeHidden,
    BeforeNodesComplete,
    AfterNodesComplete,
    Prepared,
    Owner,
    BeforeClose,
    CloseOrder,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Failure {
    TypedThenCancel,
    PriorThenCancel,
    Cancel,
    Unwind,
}

impl Failure {
    fn error(self, stage: Stage) -> CoreError {
        match self {
            Self::TypedThenCancel => CoreError::InvalidBoundary,
            Self::Unwind
                if matches!(
                    stage,
                    Stage::BeforeNodesComplete | Stage::AfterNodesComplete
                ) =>
            {
                // No TargetWriter is active here. The enclosing original
                // PreparedCall is the first guard to close this unwind.
                CoreError::InternalInvariant
            }
            Self::PriorThenCancel | Self::Unwind => CoreError::InvalidProvenance,
            Self::Cancel => CoreError::Cancelled,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PreparedSample {
    flow: usize,
    rows: usize,
    initialized: usize,
    prepares: usize,
    capacity: u64,
}

/// A local, explicitly borrowed test case. It never owns construction storage.
pub(in crate::parse::direct) struct Case<'budget> {
    budget: &'budget SourceBudget,
    cancel: CancellationToken,
    trigger: Option<(Stage, usize, Failure)>,
    triggered: bool,
    prepared: Vec<PreparedSample>,
    owner_calls: usize,
    before_close: Vec<(usize, u64, usize, u64)>,
    overlap: Vec<(usize, u64)>,
    empty_closes: Vec<usize>,
    retired: bool,
    before_nodes_complete: Option<u64>,
}

impl<'budget> Case<'budget> {
    fn new(budget: &'budget SourceBudget, cancel: CancellationToken) -> Self {
        Self {
            budget,
            cancel,
            trigger: None,
            triggered: false,
            prepared: Vec::new(),
            owner_calls: 0,
            before_close: Vec::new(),
            overlap: Vec::new(),
            empty_closes: Vec::new(),
            retired: false,
            before_nodes_complete: None,
        }
    }

    pub(in crate::parse::direct) fn at(
        &mut self,
        stage: Stage,
        flow: usize,
        stats: OwnerStats,
        work: &mut OwnedWork<'_, '_>,
    ) -> Result<(), CoreError> {
        let live = self.budget.usage().auxiliary_capacity;
        match stage {
            Stage::Positive | Stage::DfsEnter | Stage::DfsExit | Stage::BeforeHidden => {}
            Stage::BeforeNodesComplete => {
                assert!(self.before_nodes_complete.replace(live).is_none());
            }
            Stage::AfterNodesComplete => {
                assert!(
                    live < self
                        .before_nodes_complete
                        .expect("actual completion preceded sample")
                );
            }
            Stage::Prepared => {
                assert_eq!(flow, self.prepared.len());
                assert!(stats.rows > 0 && stats.capacity > 0);
                self.prepared.push(PreparedSample {
                    flow,
                    rows: stats.rows,
                    initialized: stats.initialized,
                    prepares: stats.prepares,
                    capacity: stats.capacity,
                });
            }
            Stage::Owner => self.owner_calls += 1,
            Stage::BeforeClose => {
                assert!(stats.rows > 0 && stats.capacity > 0);
                self.before_close
                    .push((flow, live, stats.rows, stats.capacity));
            }
            Stage::CloseOrder => {
                let &(_, before, rows, capacity) = self
                    .before_close
                    .iter()
                    .find(|sample| sample.0 == flow)
                    .expect("real close must follow its own emit");
                if stats.rows > 0 {
                    assert_eq!(stats.rows, rows);
                    assert_eq!(stats.capacity, capacity);
                    let added = live
                        .checked_sub(before)
                        .expect("complete Born order has not released the live target rows");
                    if self.empty_closes.contains(&flow) {
                        assert_eq!(added, 0, "empty target needs no Born-order allocation");
                    } else {
                        assert!(added > 0, "real nonempty Born order allocates capacity");
                    }
                    self.overlap.push((flow, added));
                } else {
                    assert!(self.retired);
                    assert_eq!(stats.capacity, 0);
                }
            }
            Stage::Retired => {
                assert!(!self.retired);
                assert_eq!((stats.rows, stats.capacity), (0, 0));
                let &(_, before, _, capacity) = self
                    .before_close
                    .iter()
                    .find(|sample| sample.0 == flow)
                    .expect("last emit was observed before retirement");
                assert_eq!(before.checked_sub(live), Some(capacity));
                self.retired = true;
            }
        }
        if let Some((wanted, wanted_flow, failure)) = self.trigger
            && stage == wanted
            && flow == wanted_flow
        {
            assert!(!self.triggered);
            self.triggered = true;
            match failure {
                Failure::TypedThenCancel => {
                    self.cancel.cancel();
                    return Err(CoreError::InvalidBoundary);
                }
                Failure::PriorThenCancel => {
                    work.fail(CoreError::InvalidProvenance);
                    self.cancel.cancel();
                    return Err(CoreError::InvalidBoundary);
                }
                Failure::Cancel => {
                    self.cancel.cancel();
                    return work.check();
                }
                Failure::Unwind => panic!("hidden-owner guarded callback unwind"),
            }
        }
        Ok(())
    }

    fn assert_success(&self, flows: usize) {
        assert_eq!(self.prepared.len(), flows);
        let first = self.prepared.first().unwrap();
        assert!(first.rows > 0);
        assert_eq!(first.initialized, first.rows);
        for (index, sample) in self.prepared.iter().enumerate() {
            assert_eq!(sample.flow, index);
            assert_eq!(sample.rows, first.rows);
            assert_eq!(sample.initialized, first.rows);
            assert_eq!(sample.prepares, index + 1);
            assert_eq!(sample.capacity, first.capacity);
        }
        assert!(self.owner_calls > 0);
        assert_eq!(self.before_close.len(), flows);
        assert_eq!(self.overlap.len(), flows - 1);
        assert!(self.retired);
        assert!(!self.triggered);
        let minimum = self
            .overlap
            .iter()
            .map(|sample| sample.1)
            .min()
            .unwrap_or(0);
        let maximum = self
            .overlap
            .iter()
            .map(|sample| sample.1)
            .max()
            .unwrap_or(0);
        let sum: u64 = self.overlap.iter().map(|sample| sample.1).sum();
        eprintln!(
            "actual hidden-owner: R={} initialized={} F={} cache_capacity={} close_samples={} added_min={minimum} added_max={maximum} added_sum={sum}",
            first.rows,
            first.initialized,
            flows,
            first.capacity,
            self.overlap.len()
        );
    }
}

struct OwnerCompiler<'case, 'budget, 'id> {
    case: &'case mut Case<'budget>,
    oracle: bool,
    brand: std::marker::PhantomData<fn(&'id ()) -> &'id ()>,
}

impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for OwnerCompiler<'_, '_, 'id>
{
    type Output = b::DirectBuilder<'id>;
    type State<'view> = CoreState<'view, 'input, 'id>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| compile_ast(&AstNative::new(reader, source), work))
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
        state: Self::State<'view>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::Output, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            let native = Native { reader, source };
            let CoreState {
                mut graph,
                captured,
                mut builder,
            } = state;
            let images = metadata::emit(&native, &mut graph, &mut builder, captured, work)?;
            target::flows_impl(
                &native,
                &mut graph,
                &mut builder,
                &images,
                work,
                Some(self.case),
            )?;
            self.case
                .at(Stage::BeforeNodesComplete, 0, OwnerStats::default(), work)?;
            builder.complete_nodes(work)?;
            self.case
                .at(Stage::AfterNodesComplete, 0, OwnerStats::default(), work)?;
            if self.oracle {
                compare_every_flow(&native, &graph, &builder, work)?;
            }
            drop((images, graph));
            Ok(builder)
        })
    }
}

#[derive(Clone, Copy)]
struct OrdinaryMember {
    id: u32,
    ordinal: usize,
    flow: usize,
}

fn slow_owner<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    ordinary: &[OrdinaryMember],
    mut token: n::LineageToken<'view, 'input>,
    flow: usize,
    root: usize,
    count: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<usize, CoreError> {
    for _ in 0..count {
        let binding = native.read(work, |access| access.binding(token))?;
        if let Some(member) = ordinary
            .iter()
            .find(|member| member.id == binding.id.get() && member.flow == flow)
        {
            return Ok(member.ordinal);
        }
        match binding.birth_parent {
            Some(parent) => token = parent,
            None => return Ok(root),
        }
    }
    panic!("real birth-parent chain exceeded its created-node bound");
}

fn compare_every_flow<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &graph::Graph<'view, 'input, 'id>,
    builder: &b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let count = native.read(work, |access| Ok(access.created_nodes()))? as usize;
    let mut ordinary = Vec::new();
    let mut lineages = Vec::new();
    let mut image_pairs = Vec::new();
    for index in 0..graph.nodes.len() {
        let member = graph.nodes.get(index, work)?;
        let binding = native.read(work, |access| access.binding(member.lineage))?;
        let row = builder.graph_node(member.node, work)?;
        if !member.alt_root {
            ordinary.push(OrdinaryMember {
                id: binding.id.get(),
                ordinal: index,
                flow: row.flow,
            });
        } else {
            image_pairs.push((binding.id.get(), member.lineage, row.flow));
        }
        if !lineages.iter().any(|&(id, _)| id == binding.id.get()) {
            lineages.push((binding.id.get(), member.lineage));
        }
    }
    let claims = native.read(work, |access| Ok(access.claim_count()))?;
    for index in 0..claims {
        let claim = native.read(work, |access| access.claim(index))?;
        let binding = native.read(work, |access| access.binding(claim.owner))?;
        if !lineages.iter().any(|&(id, _)| id == binding.id.get()) {
            lineages.push((binding.id.get(), claim.owner));
        }
    }
    // Include genuine detached ancestors even when no surviving member owns them.
    let mut next = 0;
    while next < lineages.len() {
        let binding = native.read(work, |access| access.binding(lineages[next].1))?;
        if let Some(parent) = binding.birth_parent {
            let parent_binding = native.read(work, |access| access.binding(parent))?;
            if !lineages
                .iter()
                .any(|&(id, _)| id == parent_binding.id.get())
            {
                lineages.push((parent_binding.id.get(), parent));
            }
        }
        next += 1;
        assert!(lineages.len() <= count);
    }
    assert!(!image_pairs.is_empty());
    let mut cache = Owners::new();
    let mut comparisons = 0;
    for flow in 0..graph.flows.len() {
        let root = graph.flows.get(flow, work)?.root();
        let mut rows = cache.prepare(native, graph, flow, work)?;
        let mut path = BuildGrowingVec::new(
            work.model_limits().unwrap().native_records,
            LimitKind::NativeRecords,
            work,
        )?;
        for &(_, token) in &lineages {
            let expected = slow_owner(native, &ordinary, token, flow, root, count, work)?;
            for _ in 0..2 {
                let actual = owner(
                    native,
                    graph,
                    builder,
                    token,
                    flow,
                    (&mut rows, &mut path),
                    work,
                )?;
                assert_eq!(actual, expected, "flow={flow}");
                comparisons += 1;
            }
        }
    }
    let mut differing = 0;
    for &(id, token, alt_flow) in &image_pairs {
        let normal = ordinary.iter().find(|member| member.id == id).unwrap();
        let main_root = graph.flows.get(normal.flow, work)?.root();
        let alt_root = graph.flows.get(alt_flow, work)?.root();
        let main = slow_owner(
            native,
            &ordinary,
            token,
            normal.flow,
            main_root,
            count,
            work,
        )?;
        let alt = slow_owner(native, &ordinary, token, alt_flow, alt_root, count, work)?;
        assert_eq!(main, normal.ordinal);
        assert_eq!(alt, alt_root);
        assert_ne!(main, alt);
        differing += 1;
    }
    assert!(differing > 0);
    eprintln!(
        "independent hidden-owner oracle: {} lineages, {} flows, {comparisons} comparisons, {differing} image/alt identity splits",
        lineages.len(),
        graph.flows.len()
    );
    Ok(())
}

fn image_text(count: usize) -> String {
    std::iter::repeat_n("![**中** &amp; \\*😀](u)", count)
        .collect::<Vec<_>>()
        .join(" ")
}

fn stable_operation(budget: &SourceBudget, cancel: CancellationToken) -> Operation {
    let mut operation = budget.begin(cancel);
    // This is a semantic test clock, never evidence of elapsed performance.
    operation.set_clock_samples_for_test(&[499_999_999]);
    operation
}

fn logical(model: &AcceptedModel, flow: usize, operation: &mut Operation) -> String {
    let ir = model.ir();
    let block = ir.flows()[flow].atoms;
    let mut value = String::new();
    for index in block.start()..block.start() + block.len() {
        let atom = ir.atom_at(index, operation).unwrap();
        assert_eq!(ir.nodes()[atom.owner.index()].flow.index(), flow);
        match atom.kind {
            AtomKind::Copy => {
                for span in atom.sources.slice(ir.sources()).unwrap() {
                    value.push_str(
                        &model.source().text()[span.start() as usize..span.end() as usize],
                    );
                }
            }
            AtomKind::Transform | AtomKind::Synthetic => {
                let AtomContent::Pool(text) = atom.content else {
                    panic!("actual text atom has no owned text");
                };
                value.push_str(ir.text(text).unwrap());
            }
            AtomKind::Object => value.push('\u{fffc}'),
            AtomKind::Hidden => {
                assert_eq!(atom.target.start, atom.target.end);
                let domains = ir.flows()[flow].domains.slice(ir.sources()).unwrap();
                for source in atom.sources.slice(ir.sources()).unwrap() {
                    assert!(domains.iter().any(|domain| {
                        domain.start() <= source.start() && source.end() <= domain.end()
                    }));
                }
            }
        }
    }
    value
}

#[test]
fn actual_one_sixteen_and_128_images_keep_final_values_flows_and_mapping() {
    for count in [1, 16, 128] {
        let text = image_text(count);
        let budget = SourceBudget::task_fixture();
        let mut operation = stable_operation(&budget, CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([132; 16], count as u64).unwrap(),
            text.as_bytes(),
            &mut operation,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::new(Profile::C0),
            1,
            &mut operation,
        )
        .unwrap();
        assert_eq!(model.ir().objects().len(), count);
        assert_eq!(model.ir().flows().len(), count + 1);
        let expected = std::iter::repeat_n("\u{fffc}", count)
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(logical(&model, 0, &mut operation), expected);
        assert_eq!(model.ir().flows()[0].logical_utf16, (2 * count - 1) as u64);
        for (image, object) in model.ir().objects().iter().enumerate() {
            let ObjectRow::Image { alt, .. } = *object else {
                panic!("C0 fixture contains images only");
            };
            assert_eq!(logical(&model, alt.index(), &mut operation), "中 & *😀");
            assert_eq!(model.ir().flows()[alt.index()].logical_utf16, 7);
            let base = image * ("![**中** &amp; \\*😀](u)".len() + 1) + 2;
            let entity = SourceSpan::new((base + 8) as u64, (base + 13) as u64).unwrap();
            for (affinity, chosen) in [(Affinity::Before, 2), (Affinity::After, 3)] {
                let mapped = model
                    .map_point(
                        &PointQuery {
                            model: model.identity(),
                            input: PointInput::Source {
                                target: TextTarget::Flow(alt),
                                byte: SourceBoundary::new((base + 10) as u64),
                            },
                            affinity,
                        },
                        &mut operation,
                    )
                    .unwrap();
                assert_eq!(mapped.results()[0].chosen.value(), chosen);
                assert_eq!(mapped.results()[0].correspondence, Correspondence::Atomic);
            }
            let hidden = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(alt),
                            byte: SourceBoundary::new((base + 1) as u64),
                        },
                        affinity: Affinity::After,
                    },
                    &mut operation,
                )
                .unwrap();
            assert_eq!(hidden.results()[0].chosen.value(), 0);
            assert_eq!(hidden.results()[0].correspondence, Correspondence::Hidden);
            let range = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Flow {
                            flow: alt,
                            span: LogicalSpan::new(2, 3).unwrap(),
                        },
                        empty_affinity: Affinity::Before,
                    },
                    &mut operation,
                )
                .unwrap();
            assert!(
                range
                    .contributions()
                    .iter()
                    .any(|part| part.source == entity)
            );
            for input in [
                PointInput::Flow {
                    flow: alt,
                    utf16: LogicalBoundary::new(6),
                },
                PointInput::Source {
                    target: TextTarget::Flow(alt),
                    byte: SourceBoundary::new((base + 17) as u64),
                },
            ] {
                let mut negative = stable_operation(&budget, CancellationToken::new());
                assert!(matches!(
                    model.map_point(
                        &PointQuery {
                            model: model.identity(),
                            input,
                            affinity: Affinity::After,
                        },
                        &mut negative,
                    ),
                    Err(CoreError::InvalidBoundary)
                ));
            }
        }
        assert_eq!(operation.first_fault(), None);
        drop((model, source));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
    }
}

#[test]
fn actual_cache_initializes_once_and_matches_independent_owner_in_every_flow() {
    for count in [1, 16, 128] {
        let budget = SourceBudget::task_fixture();
        let mut operation = stable_operation(&budget, CancellationToken::new());
        let text = image_text(count);
        let source = SourceDocument::new(
            SourceKey::new([133; 16], count as u64).unwrap(),
            text.as_bytes(),
            &mut operation,
        )
        .unwrap()
        .snapshot();
        let input = ParseInput::prepare(source.clone(), &mut operation).unwrap();
        let cancel = CancellationToken::new();
        let mut case = Case::new(&budget, cancel.clone());
        let mut operation = stable_operation(&budget, cancel);
        construction_scope(&mut operation, |mut work| {
            let builder = with_prepared(&input, &mut work, |input, mut host| {
                let result = n::compile(
                    input,
                    &options::c0(),
                    &mut host,
                    OwnerCompiler {
                        case: &mut case,
                        oracle: true,
                        brand: std::marker::PhantomData,
                    },
                );
                host.resolve(result)
            })
            .unwrap();
            let ir = builder.finish(&mut work).unwrap();
            let identity =
                ModelIdentity::new(source.key(), 1, ParseConfig::new(Profile::C0)).unwrap();
            let facts =
                ir::validate::validate_for_index_owned(&ir, &source, identity, &mut work).unwrap();
            let (index, _) = MapIndex::build_verified(facts).unwrap();
            drop((index, ir));
            work.finish().unwrap();
        });
        case.assert_success(count + 1);
        drop((input, source));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
    }
}

#[test]
fn real_second_cache_page_failure_releases_partially_initialized_rows() {
    assert_eq!(size_of::<OwnerEntry>(), 8);
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits {
            auxiliary_capacity: 10_000,
            ..SourceLimits::default()
        },
        crate::policy::ModelLimits::default(),
    )
    .unwrap();
    let mut operation = budget.begin(CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        let mut owners = Owners::new();
        let error = owners.initialize(1050, &mut work).unwrap_err();
        assert!(matches!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::AuxiliaryCapacity,
                requested,
                limit: 10_000,
            } if requested > 10_000
        ));
        assert_eq!(owners.initialized, 1024);
        assert!(owners.rows.is_none());
        assert!(budget.usage().auxiliary_capacity_peak >= 8192);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.first_fault(), Some(error));
        assert_eq!(work.check(), Err(error));
        assert_eq!(work.finish(), Err(error));
    });
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn private_cache_stamps_and_invalid_ordinals_preserve_original_first_fault() {
    for bad in 0..5 {
        let budget = SourceBudget::model_fixture();
        let cancel = CancellationToken::new();
        let mut operation = budget.begin(cancel.clone());
        construction_scope(&mut operation, |mut work| {
            let mut owners = Owners::new();
            owners.initialize(2, &mut work).unwrap();
            let stats = owners.stats();
            let rows = owners.rows.as_mut().unwrap();
            let mut flow = FlowOwners {
                rows,
                flow: 0,
                members: 2,
                stats,
                identity: std::marker::PhantomData,
            };
            flow.put(0, 1, &mut work).unwrap();
            assert_eq!(flow.get(0, &mut work).unwrap(), Some(1));
            flow.flow = 1;
            assert_eq!(flow.get(0, &mut work).unwrap(), None);
            flow.put(0, 0, &mut work).unwrap();
            assert_eq!(flow.get(0, &mut work).unwrap(), Some(0));
            let error = match bad {
                0 => flow.get(2, &mut work).unwrap_err(),
                1 => flow.put(2, 0, &mut work).unwrap_err(),
                2 => flow.put(0, 2, &mut work).unwrap_err(),
                3 => flow.put(0, u32::MAX as usize, &mut work).unwrap_err(),
                4 => {
                    flow.rows
                        .replace(
                            0,
                            OwnerEntry {
                                flow: 1,
                                owner: u32::MAX,
                            },
                            &mut work,
                        )
                        .unwrap();
                    flow.get(0, &mut work).unwrap_err()
                }
                _ => unreachable!(),
            };
            assert_eq!(
                error,
                match bad {
                    0 | 1 => CoreError::InternalInvariant,
                    3 => CoreError::ArithmeticOverflow,
                    _ => CoreError::InvalidProvenance,
                }
            );
            cancel.cancel();
            assert_eq!(work.first_fault(), Some(error));
            assert_eq!(work.check(), Err(error));
            drop(owners);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(error));
        });
    }
    for value in [u32::MAX as usize, usize::MAX] {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            assert_eq!(
                owner_ordinal(value, &mut work),
                Err(CoreError::ArithmeticOverflow)
            );
            assert!(work.first_fault().is_some());
        });
    }
}

#[derive(Clone, Copy, Debug)]
enum PrepareFailure {
    Repeat,
    Skip,
    Retired,
    OtherMainNode,
    ForeignBuilder,
    Sentinel,
    Overflow,
}

struct BoundaryCompiler<'id> {
    failure: PrepareFailure,
    brand: std::marker::PhantomData<fn(&'id ()) -> &'id ()>,
}

impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for BoundaryCompiler<'id>
{
    type Output = ();
    type State<'view> = CoreState<'view, 'input, 'id>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| compile_ast(&AstNative::new(reader, source), work))
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
        state: Self::State<'view>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<(), NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            let native = Native { reader, source };
            let CoreState {
                mut graph,
                captured,
                mut builder,
            } = state;
            let objects = metadata::emit(&native, &mut graph, &mut builder, captured, work)?;
            target::flows(&native, &mut graph, &mut builder, &objects, work)?;
            builder.complete_nodes(work)?;
            drop(objects);
            assert!(graph.flows.len() > 1);
            let mut owners = Owners::new();
            let next = match self.failure {
                PrepareFailure::Skip => 1,
                PrepareFailure::Sentinel => u32::MAX as usize,
                PrepareFailure::Overflow => usize::MAX,
                _ => {
                    owners.prepare(&native, &graph, 0, work)?;
                    1
                }
            };
            let mut foreign = None;
            match self.failure {
                PrepareFailure::Repeat => {
                    assert!(owners.prepare(&native, &graph, 0, work).is_err());
                    return Err(work.first_fault().unwrap());
                }
                PrepareFailure::Retired => owners.retire(),
                PrepareFailure::OtherMainNode | PrepareFailure::ForeignBuilder => {
                    let root = graph.flows.get(0, work)?.root();
                    let mut member = graph.nodes.get(root, work)?;
                    member.node = match self.failure {
                        PrepareFailure::OtherMainNode => graph.nodes.get(root + 1, work)?.node,
                        PrepareFailure::ForeignBuilder => {
                            let mut other = b::DirectBuilder::new(work)?;
                            let flow = other.reserve_flow(work)?;
                            let node = other.reserve_node(
                                b::DraftNodeSeed {
                                    parent: None,
                                    flow,
                                    kind: NodeKind::Document,
                                    detail: None,
                                    initial: Some(SourceSpan::new(0, 0)?),
                                },
                                work,
                            )?;
                            foreign = Some(other);
                            node
                        }
                        _ => unreachable!(),
                    };
                    graph.nodes.replace(root, member, work)?;
                }
                _ => {}
            }
            assert!(owners.prepare(&native, &graph, next, work).is_err());
            drop(foreign);
            Err(work.first_fault().unwrap())
        })
    }
}

#[test]
fn actual_scoped_main_root_and_flow_sequence_cannot_reuse_or_reopen_cache() {
    for failure in [
        PrepareFailure::Repeat,
        PrepareFailure::Skip,
        PrepareFailure::Retired,
        PrepareFailure::OtherMainNode,
        PrepareFailure::ForeignBuilder,
        PrepareFailure::Sentinel,
        PrepareFailure::Overflow,
    ] {
        let budget = SourceBudget::task_fixture();
        let mut preparation = stable_operation(&budget, CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([135; 16], 1).unwrap(),
            b"a ![**alt**](u)",
            &mut preparation,
        )
        .unwrap()
        .snapshot();
        let input = ParseInput::prepare(source.clone(), &mut preparation).unwrap();
        let baseline = budget.usage();
        let cancel = CancellationToken::new();
        let mut operation = stable_operation(&budget, cancel.clone());
        construction_scope(&mut operation, |mut work| {
            let error = with_prepared(&input, &mut work, |input, mut host| {
                let result = n::compile(
                    input,
                    &options::c0(),
                    &mut host,
                    BoundaryCompiler {
                        failure,
                        brand: std::marker::PhantomData,
                    },
                );
                host.resolve(result)
            })
            .unwrap_err();
            let expected = if matches!(failure, PrepareFailure::Sentinel | PrepareFailure::Overflow)
            {
                CoreError::ArithmeticOverflow
            } else {
                CoreError::InvalidProvenance
            };
            assert_eq!(error, expected, "{failure:?}");
            cancel.cancel();
            assert_eq!(work.check(), Err(expected));
            assert_eq!(work.finish(), Err(expected));
        });
        assert_eq!(
            budget.usage().auxiliary_capacity,
            baseline.auxiliary_capacity
        );
        assert_eq!(budget.usage().source_capacity, baseline.source_capacity);
        assert_eq!(budget.usage().source_owners, baseline.source_owners);
        drop((input, source));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
    }
}

#[test]
fn nested_and_empty_alt_lineages_match_oracle_and_complete_original_validation() {
    let text = "![outer **中** &amp; \\*😀 ![inner](v) tail](u) ![](empty)";
    let budget = SourceBudget::task_fixture();
    let mut preparation = stable_operation(&budget, CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([136; 16], 1).unwrap(),
        text.as_bytes(),
        &mut preparation,
    )
    .unwrap()
    .snapshot();
    let input = ParseInput::prepare(source.clone(), &mut preparation).unwrap();
    let cancel = CancellationToken::new();
    let mut case = Case::new(&budget, cancel.clone());
    case.empty_closes.push(2);
    let mut operation = stable_operation(&budget, cancel);
    construction_scope(&mut operation, |mut work| {
        let builder = with_prepared(&input, &mut work, |input, mut host| {
            let result = n::compile(
                input,
                &options::c0(),
                &mut host,
                OwnerCompiler {
                    case: &mut case,
                    oracle: true,
                    brand: std::marker::PhantomData,
                },
            );
            host.resolve(result)
        })
        .unwrap();
        let ir = builder.finish(&mut work).unwrap();
        let identity = ModelIdentity::new(source.key(), 1, ParseConfig::new(Profile::C0)).unwrap();
        let facts =
            ir::validate::validate_for_index_owned(&ir, &source, identity, &mut work).unwrap();
        let (index, _) = MapIndex::build_verified(facts).unwrap();
        assert_eq!(ir.flows().len(), 4);
        assert!(ir.flows().iter().any(|flow| flow.atoms.is_empty()));
        drop((index, ir));
        work.finish().unwrap();
    });
    case.assert_success(4);
    drop((input, source));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

#[test]
fn g3_table_task_and_nested_empty_alts_finish_rows_before_hidden() {
    let text =
        "| A | B |\n| - | - |\n| ![outer ![inner](v) tail](u) | ![](empty) |\n\n- [x] ![task](t)\n";
    let budget = SourceBudget::task_fixture();
    let mut preparation = stable_operation(&budget, CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([137; 16], 1).unwrap(),
        text.as_bytes(),
        &mut preparation,
    )
    .unwrap()
    .snapshot();
    let input = ParseInput::prepare(source.clone(), &mut preparation).unwrap();
    let cancel = CancellationToken::new();
    let mut case = Case::new(&budget, cancel.clone());
    // Main queues outer, empty and task images before outer's nested image.
    case.empty_closes.push(2);
    let mut operation = stable_operation(&budget, cancel);
    construction_scope(&mut operation, |mut work| {
        let builder = with_prepared(&input, &mut work, |input, mut host| {
            let result = n::compile(
                input,
                &options::g3(),
                &mut host,
                OwnerCompiler {
                    case: &mut case,
                    oracle: true,
                    brand: std::marker::PhantomData,
                },
            );
            host.resolve(result)
        })
        .unwrap();
        let ir = builder.finish(&mut work).unwrap();
        let identity = ModelIdentity::new(source.key(), 1, ParseConfig::new(Profile::G3)).unwrap();
        let facts =
            ir::validate::validate_for_index_owned(&ir, &source, identity, &mut work).unwrap();
        let (index, _) = MapIndex::build_verified(facts).unwrap();
        assert_eq!(ir.flows().len(), 5);
        // Main is A, tab, B, LF, object, tab, object, LF, checkbox,
        // task-label space, object. Image Alt roots emit no own object.
        for (flow, expected) in ir.flows().iter().zip([11, 12, 0, 4, 5]) {
            assert_eq!(flow.logical_utf16, expected);
        }
        assert_eq!(ir.objects().len(), 5);
        assert_eq!(
            ir.objects()
                .iter()
                .filter(|object| matches!(object, ObjectRow::Task { checked: true, .. }))
                .count(),
            1
        );
        assert_eq!(
            ir.nodes()
                .iter()
                .filter(|row| row.kind == NodeKind::TableCell)
                .count(),
            4
        );
        drop((index, ir));
        work.finish().unwrap();
    });
    case.assert_success(5);
    drop((input, source));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

#[test]
fn actual_owner_and_close_faults_leave_old_model_readable_and_release_all_new_storage() {
    let text =
        "# heading\n\n![**中** &amp; \\*😀](u \"title\") ![outer ![inner](v) tail](w) ![](empty)";
    for (stage, flow) in [
        (Stage::Positive, 1),
        (Stage::DfsEnter, 0),
        (Stage::DfsEnter, 1),
        (Stage::DfsExit, 0),
        (Stage::DfsExit, 1),
        (Stage::BeforeHidden, 1),
        (Stage::BeforeNodesComplete, 0),
        (Stage::AfterNodesComplete, 0),
        (Stage::Prepared, 1),
        (Stage::Owner, 1),
        (Stage::CloseOrder, 0),
    ] {
        for failure in [
            Failure::TypedThenCancel,
            Failure::PriorThenCancel,
            Failure::Cancel,
            Failure::Unwind,
        ] {
            let budget = SourceBudget::task_fixture();
            let mut preparation = stable_operation(&budget, CancellationToken::new());
            let source = SourceDocument::new(
                SourceKey::new([134; 16], 1).unwrap(),
                text.as_bytes(),
                &mut preparation,
            )
            .unwrap()
            .snapshot();
            let input = ParseInput::prepare(source.clone(), &mut preparation).unwrap();
            let mut previous = stable_operation(&budget, CancellationToken::new());
            let old_model = AcceptedModel::parse(
                source.clone(),
                ParseConfig::new(Profile::C0),
                1,
                &mut previous,
            )
            .unwrap();
            let identity = old_model.identity();
            let title = old_model
                .ir()
                .payloads()
                .iter()
                .find(|row| row.role == PayloadRole::Title)
                .unwrap()
                .value;
            assert_eq!(old_model.ir().text(title).unwrap(), "title");
            let baseline = budget.usage();
            let cancel = CancellationToken::new();
            let mut case = Case::new(&budget, cancel.clone());
            // Three Main images queue before the nested inner image. The
            // third Main image is ![](empty), so its FIFO Alt flow is 3.
            assert_eq!(old_model.ir().flows().len(), 5);
            assert_eq!(old_model.ir().flows()[3].logical_utf16, 0);
            case.empty_closes.push(3);
            case.trigger = Some((stage, flow, failure));
            let mut operation = stable_operation(&budget, cancel);
            construction_scope(&mut operation, |mut work| {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    with_prepared(&input, &mut work, |input, mut host| {
                        let result = n::compile(
                            input,
                            &options::c0(),
                            &mut host,
                            OwnerCompiler {
                                case: &mut case,
                                oracle: false,
                                brand: std::marker::PhantomData,
                            },
                        );
                        host.resolve(result)
                    })
                }));
                assert!(case.triggered, "{stage:?}/{failure:?}");
                match result {
                    Ok(Err(error)) => {
                        assert_ne!(failure, Failure::Unwind);
                        assert_eq!(error, failure.error(stage));
                    }
                    Err(payload) => {
                        assert_eq!(failure, Failure::Unwind);
                        assert_eq!(
                            payload.downcast_ref::<&str>().copied(),
                            Some("hidden-owner guarded callback unwind")
                        );
                    }
                    Ok(Ok(_)) => panic!("failed construction returned a new builder"),
                }
                assert_eq!(work.first_fault(), Some(failure.error(stage)));
                assert_eq!(work.check(), Err(failure.error(stage)));
                assert_eq!(work.finish(), Err(failure.error(stage)));
            });
            let after = budget.usage();
            assert_eq!(after.auxiliary_capacity, baseline.auxiliary_capacity);
            assert_eq!(after.source_capacity, baseline.source_capacity);
            assert_eq!(after.source_owners, baseline.source_owners);
            assert_eq!(old_model.identity(), identity);
            assert_eq!(old_model.ir().text(title).unwrap(), "title");
            assert_eq!(old_model.source().text(), text);
            let mut query = stable_operation(&budget, CancellationToken::new());
            assert!(!logical(&old_model, 0, &mut query).is_empty());
            drop((old_model, input, source));
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(budget.usage().source_capacity, 0);
            assert_eq!(budget.usage().source_owners, 0);
        }
    }
}
