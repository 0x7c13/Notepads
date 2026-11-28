// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::parse::prepared::NativeFault;
use crate::policy::{CancellationToken, SourceBudget, construction_scope};
use crate::source::SourceDocument;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Clone, Copy, Debug)]
enum Rejection {
    Role,
    Owner,
    Node,
    Factory,
    EmptyFactory,
    ValueFactory,
    MetadataOrdinal,
    Incomplete,
    PartialColumn,
    CaughtUnwind,
}
impl Rejection {
    fn expected(self) -> CoreError {
        match self {
            Self::PartialColumn => CoreError::ResourceLimit {
                kind: LimitKind::ModelRecords,
                requested: 1,
                limit: 0,
            },
            Self::CaughtUnwind => CoreError::InternalInvariant,
            _ => CoreError::InvalidProvenance,
        }
    }
}
struct RejectCompiler<'state, 'id> {
    mode: Rejection,
    reached: &'state Cell<bool>,
    cancel: &'state CancellationToken,
    brand: std::marker::PhantomData<fn(&'id ()) -> &'id ()>,
}
impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for RejectCompiler<'_, 'id>
{
    type Output = ();
    type State<'view> = CoreState<'view, 'input, 'id>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            let native = AstNative::new(reader, source);
            let mut builder = b::DirectBuilder::new(work)?;
            let mut captures = CaptureBuilder::new(&native, &mut builder, work)?;
            // Definitions already copied their actual field strings to final text.
            if matches!(self.mode, Rejection::EmptyFactory) {
                assert_eq!(captures.definitions.len(), 0);
                assert_eq!(captures.values.len(), 0);
            } else {
                assert_ne!(captures.definitions.len(), 0);
                assert!(captures.values.len() >= 2);
            }
            if matches!(self.mode, Rejection::PartialColumn) {
                captures.nodes = BuildRows::new(0, LimitKind::ModelRecords, work)?;
                let result = graph::Graph::prepare(&native, &mut builder, &mut captures, work);
                assert!(matches!(result, Err(error) if error == self.mode.expected()));
                // The actual Heading column push succeeded before common-row push failed.
                assert_eq!(captures.headings.len(), 1);
                assert_eq!(captures.nodes.len(), 0);
                self.reached.set(true);
                self.cancel.cancel();
                assert_eq!(work.check(), Err(self.mode.expected()));
                return Err(self.mode.expected());
            }
            let graph = graph::Graph::prepare(&native, &mut builder, &mut captures, work)?;
            if matches!(self.mode, Rejection::CaughtUnwind) {
                let row = captures.headings.get(0, work)?;
                let before = captures.headings.len();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    call(work, |work| -> Result<(), CoreError> {
                        captures.headings.push(row, work)?;
                        panic!("capture column has really changed before unwind");
                    })
                }));
                assert!(result.is_err());
                assert_eq!(captures.headings.len(), before + 1);
                self.reached.set(true);
                self.cancel.cancel();
                assert_eq!(work.check(), Err(self.mode.expected()));
                return Err(self.mode.expected());
            }
            let captured = captures.finish(graph.metadata_len(), work)?;
            Ok(CoreState {
                graph,
                captured,
                builder,
            })
        })
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
                mut captured,
                mut builder,
            } = state;
            graph.measure(&native, &mut builder, work)?;
            builder.complete_nodes(work)?;
            let root = graph.nodes.get(0, work)?.node;
            let sources_before = builder.sources_len();
            let result = match self.mode {
                Rejection::Incomplete => captured.finish(work),
                Rejection::Factory | Rejection::EmptyFactory => {
                    let mut foreign = b::DirectBuilder::new(work)?;
                    captured
                        .next_definition(root, &mut foreign, work)
                        .map(|_| ())
                }
                Rejection::MetadataOrdinal => {
                    while captured
                        .next_definition(root, &mut builder, work)?
                        .is_some()
                    {}
                    let mut row = captured.data.nodes.get(0, work)?;
                    row.metadata += 1;
                    captured.data.nodes.replace(0, row, work)?;
                    captured.next_node(&graph, &builder, work).map(|_| ())
                }
                Rejection::ValueFactory => {
                    let definition = captured.next_definition(root, &mut builder, work)?.unwrap();
                    let mut foreign = b::DirectBuilder::new(work)?;
                    let result = target::payload(
                        &native,
                        &mut graph,
                        &mut foreign,
                        b::DraftPayloadOwner::Definition(definition.reference),
                        root,
                        PayloadRole::Destination,
                        definition.destination,
                        work,
                    )
                    .map(|_| ());
                    assert_eq!(foreign.sources_len(), 0);
                    result
                }
                Rejection::Role | Rejection::Owner | Rejection::Node => {
                    let definition = captured.next_definition(root, &mut builder, work)?.unwrap();
                    let owner = if matches!(self.mode, Rejection::Owner) {
                        b::DraftPayloadOwner::Definition(builder.reserve_definition(work)?)
                    } else {
                        b::DraftPayloadOwner::Definition(definition.reference)
                    };
                    let node = if matches!(self.mode, Rejection::Node) {
                        graph.nodes.get(1, work)?.node
                    } else {
                        root
                    };
                    let role = if matches!(self.mode, Rejection::Role) {
                        PayloadRole::Title
                    } else {
                        PayloadRole::Destination
                    };
                    target::payload(
                        &native,
                        &mut graph,
                        &mut builder,
                        owner,
                        node,
                        role,
                        definition.destination,
                        work,
                    )
                    .map(|_| ())
                }
                Rejection::PartialColumn | Rejection::CaughtUnwind => unreachable!(),
            };
            assert_eq!(result, Err(self.mode.expected()), "{:?}", self.mode);
            // Bad field/factory/ordinal fails before any new payload source emission.
            assert_eq!(builder.sources_len(), sources_before);
            assert_eq!(work.first_fault(), Some(self.mode.expected()));
            self.reached.set(true);
            self.cancel.cancel();
            assert_eq!(work.check(), Err(self.mode.expected()));
            Err(self.mode.expected())
        })
    }
}

#[test]
fn real_capture_rejects_mismatches_partial_columns_and_caught_unwind_before_payload() {
    for mode in [
        Rejection::Role,
        Rejection::Owner,
        Rejection::Node,
        Rejection::Factory,
        Rejection::EmptyFactory,
        Rejection::ValueFactory,
        Rejection::MetadataOrdinal,
        Rejection::Incomplete,
        Rejection::PartialColumn,
        Rejection::CaughtUnwind,
    ] {
        let budget = SourceBudget::task_fixture();
        let cancel = CancellationToken::new();
        let mut operation = budget.begin(cancel.clone());
        let text = if matches!(mode, Rejection::EmptyFactory) {
            ""
        } else {
            "[x]: /real \"title\"\n\n# heading\n\n[x]\n"
        };
        let source = SourceDocument::new(
            SourceKey::new([114; 16], 1).unwrap(),
            text.as_bytes(),
            &mut operation,
        )
        .unwrap()
        .snapshot();
        let reached = Cell::new(false);
        construction_scope(&mut operation, |mut work| {
            let input = ParseInput::prepare_owned(source.clone(), &mut work).unwrap();
            let result = with_prepared(&input, &mut work, |input, mut binding| {
                let result = n::compile(
                    input,
                    &options::c0(),
                    &mut binding,
                    RejectCompiler {
                        mode,
                        reached: &reached,
                        cancel: &cancel,
                        brand: std::marker::PhantomData,
                    },
                );
                binding.resolve(result)
            });
            assert_eq!(result, Err(mode.expected()), "{mode:?}");
            assert!(
                reached.get(),
                "actual capture boundary not reached: {mode:?}"
            );
            assert_eq!(work.first_fault(), Some(mode.expected()));
            drop(input);
            assert_eq!(work.finish(), Err(mode.expected()));
        });
        drop(source);
        let usage = budget.usage();
        assert_eq!(
            (
                usage.source_capacity,
                usage.auxiliary_capacity,
                usage.source_owners
            ),
            (0, 0, 0),
            "{mode:?}"
        );
    }
}

#[test]
fn capture_callback_typed_and_ignored_errors_lock_before_later_cancellation() {
    for ignored in [false, true] {
        let budget = SourceBudget::task_fixture();
        let cancel = CancellationToken::new();
        let mut operation = budget.begin(cancel.clone());
        construction_scope(&mut operation, |mut work| {
            let result = call(&mut work, |work| {
                if ignored {
                    work.fail(CoreError::InvalidBoundary);
                    cancel.cancel();
                    Ok(())
                } else {
                    cancel.cancel();
                    Err(CoreError::InvalidBoundary)
                }
            });
            assert_eq!(result, Err(CoreError::InvalidBoundary));
            assert_eq!(work.check(), Err(CoreError::InvalidBoundary));
            assert_eq!(work.finish(), Err(CoreError::InvalidBoundary));
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn capture_columns_report_actual_layout_without_a_maximum_attribute_enum_per_node() {
    println!(
        "capture columns: node={} heading={} list={} item={} code={} link={} inline={} definition={} value={} issued={}",
        size_of::<Node>(),
        size_of::<Heading>(),
        size_of::<List<'_, '_>>(),
        size_of::<Item<'_, '_>>(),
        size_of::<Code>(),
        size_of::<Link>(),
        size_of::<Inline>(),
        size_of::<Definition<'_, '_>>(),
        size_of::<Value<'_, '_>>(),
        size_of::<CapturedValue<'_, '_>>()
    );
    assert_eq!(size_of::<Node>(), 4 * size_of::<u32>());
}

#[test]
fn retained_markers_and_list_fallback_keep_original_attribute_source_bytes() {
    let text = "## heading\n\n3. first\n4. second\n\n```rust\nx\n```\n";
    for budget in [SourceBudget::model_fixture(), SourceBudget::task_fixture()] {
        let mut operation = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([115; 16], 1).unwrap(),
            text.as_bytes(),
            &mut operation,
        )
        .unwrap()
        .snapshot();
        let model = crate::model::AcceptedModel::parse(
            source.clone(),
            ParseConfig::new(Profile::C0),
            1,
            &mut operation,
        )
        .unwrap();
        let ir = model.ir();
        let selected: Vec<_> = ir
            .attributes()
            .iter()
            .filter_map(|row| {
                if !matches!(
                    row.kind,
                    AttributeKind::HeadingLevel
                        | AttributeKind::ListMarker
                        | AttributeKind::ItemMarker
                        | AttributeKind::CodeFence
                ) {
                    return None;
                }
                let spans = row.sources.slice(ir.sources()).unwrap();
                assert_eq!(spans.len(), 1);
                let span = spans[0];
                Some((row.kind, &text[span.start() as usize..span.end() as usize]))
            })
            .collect();
        assert_eq!(
            selected,
            [
                (AttributeKind::HeadingLevel, "##"),
                (AttributeKind::ListMarker, "3."),
                (AttributeKind::ItemMarker, "3."),
                (AttributeKind::ItemMarker, "4."),
                (AttributeKind::CodeFence, "```"),
            ]
        );
        drop((model, source));
        let usage = budget.usage();
        assert_eq!(
            (
                usage.source_capacity,
                usage.auxiliary_capacity,
                usage.source_owners
            ),
            (0, 0, 0)
        );
    }
}
