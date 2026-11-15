// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::{LocalUtf8Span, LogicalSpan, ModelIdentity, SourceKey};
use crate::mapping::MapIndex;
use crate::policy::{CancellationToken, SourceBudget, construction_scope};
use crate::source::SourceDocument;

#[test]
fn builder_binding_rejects_foreign_empty_builders_without_creating_text_or_rows() {
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        let left = DirectBuilder::new(&mut work).unwrap();
        let right = DirectBuilder::new(&mut work).unwrap();
        let binding = left.binding(&mut work).unwrap();
        assert_ne!(binding, right.binding(&mut work).unwrap());
        left.check_binding(binding, &mut work).unwrap();
        assert_eq!(left.ir.nodes.len(), 0);
        assert_eq!(right.ir.nodes.len(), 0);
        assert_eq!(
            right.check_binding(binding, &mut work),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(
            left.check_binding(binding, &mut work),
            Err(CoreError::InvalidProvenance)
        );
        drop((left, right));
        assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
    });
    released(&budget);
}

fn node_seed(flow: FlowRef) -> DraftNodeSeed {
    DraftNodeSeed {
        parent: None,
        flow,
        kind: NodeKind::Document,
        detail: None,
        initial: None,
    }
}

fn header<'id>(
    length: u64,
    work: &mut OwnedWork<'id, '_>,
) -> (DirectBuilder<'id>, NodeRef, SourceBlockRef, FlowRef) {
    let mut builder = DirectBuilder::new(work).unwrap();
    let flow = builder.reserve_flow(work).unwrap();
    let node = builder.reserve_node(node_seed(flow), work).unwrap();
    let source = builder
        .source_parts(&[SourceSpan::new(0, length).unwrap()], work)
        .unwrap();
    builder
        .write_node(
            node,
            DraftNodeRow {
                parent: None,
                first_child: None,
                next_sibling: None,
                flow,
                kind: NodeKind::Document,
                detail: None,
                source_envelope: SourceSpan::new(0, length).unwrap(),
                logical: LogicalSpan::new(0, length).unwrap(),
            },
            work,
        )
        .unwrap();
    (builder, node, source, flow)
}
fn owned(node: NodeRef, source: SourceBlockRef, start: u64, end: u64) -> DraftOwnedAtom {
    DraftOwnedAtom {
        owner: node,
        kind: AtomKind::Copy,
        target: TargetSpan { start, end },
        sources: source,
        proof: None,
        content: DraftAtomContent::Empty,
        primary: DraftPrimaryAuthority {
            phase: NativePhase::Block,
            kind: ClaimKind::Block,
            consumed: LocalUtf8Span::new(start, end).unwrap(),
            host_role: HostRole::Block,
            host_utf8_len: end,
            host_domains: source,
        },
    }
}
fn order<'id>(
    writer: &mut TargetWriter<'_, '_, 'id, '_>,
    values: &[BornAtom],
) -> BuildGrowingVec<'id, BornAtom> {
    writer
        .with_read(|_, work| {
            let mut order =
                BuildGrowingVec::new(values.len() as u64, LimitKind::AuxiliaryCapacity, work)?;
            for value in values {
                order.push(*value, work)?;
            }
            Ok(order)
        })
        .unwrap()
}
fn released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

#[test]
fn final_pages_pass_independent_validation_and_original_index_routes_in_all_modes() {
    let factories: [fn() -> SourceBudget; 2] =
        [SourceBudget::model_fixture, SourceBudget::task_fixture];
    for make in factories {
        for text in ["", "a"] {
            let budget = make();
            let mut operation = budget.begin(CancellationToken::new());
            let source = SourceDocument::new(
                SourceKey::new([103; 16], 1).unwrap(),
                text.as_bytes(),
                &mut operation,
            )
            .unwrap()
            .snapshot();
            construction_scope(&mut operation, |mut work| {
                let len = text.len() as u64;
                let (mut builder, node, domains, flow) = header(len, &mut work);
                let mut target = builder.begin_target(&mut work).unwrap();
                let born = if text.is_empty() {
                    None
                } else {
                    let ticket = target.reserve_next().unwrap();
                    Some(
                        target
                            .commit_owned(ticket, owned(node, domains, 0, len))
                            .unwrap(),
                    )
                };
                let order = order(&mut target, born.as_slice());
                let block = target.close(order).unwrap().into_block(&mut work).unwrap();
                builder
                    .write_flow(
                        flow,
                        DraftFlowRow {
                            scope: DraftFlowScope::Main,
                            root: node,
                            domains,
                            atoms: block.into(),
                            logical_utf16: len,
                        },
                        &mut work,
                    )
                    .unwrap();
                let ir = builder.finish(&mut work).unwrap();
                let identity =
                    ModelIdentity::new(source.key(), 1, ParseConfig::new(Profile::C0)).unwrap();
                let index = if work.uses_verified_index() {
                    let facts =
                        validate::validate_for_index_owned(&ir, &source, identity, &mut work)
                            .unwrap();
                    MapIndex::build_verified(facts).unwrap().0
                } else {
                    validate::validate_owned(&ir, &source, &mut work).unwrap();
                    MapIndex::build_owned(&ir, &source, identity, &mut work).unwrap()
                };
                assert_eq!(ir.atoms_len(), usize::from(!text.is_empty()));
                drop((index, ir, source));
                work.finish().unwrap();
            });
            released(&budget);
        }
    }
}

#[test]
fn reserved_slots_are_not_readable_and_unwritten_rows_cannot_freeze() {
    for read in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut builder = DirectBuilder::new(&mut work).unwrap();
            let flow = builder.reserve_flow(&mut work).unwrap();
            let node = builder.reserve_node(node_seed(flow), &mut work).unwrap();
            if read {
                assert_eq!(
                    builder.read_node(node, &mut work),
                    Err(CoreError::InvalidProvenance)
                );
                drop(builder);
            } else {
                assert!(matches!(
                    builder.finish(&mut work),
                    Err(CoreError::InvalidProvenance)
                ));
            }
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[test]
fn foreign_row_handles_and_duplicate_writes_fail_before_publish() {
    for foreign in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let (mut builder, node, _, flow) = header(1, &mut work);
            let mut other = DirectBuilder::new(&mut work).unwrap();
            let reference = if foreign {
                let other_flow = other.reserve_flow(&mut work).unwrap();
                other
                    .reserve_node(node_seed(other_flow), &mut work)
                    .unwrap()
            } else {
                node
            };
            assert_eq!(
                builder.write_node(
                    reference,
                    DraftNodeRow {
                        parent: None,
                        first_child: None,
                        next_sibling: None,
                        flow,
                        kind: NodeKind::Document,
                        detail: None,
                        source_envelope: SourceSpan::new(0, 1).unwrap(),
                        logical: LogicalSpan::new(0, 1).unwrap(),
                    },
                    &mut work
                ),
                Err(CoreError::InvalidProvenance)
            );
            drop((builder, other));
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[test]
fn live_target_callback_errors_and_ignored_faults_latch_before_drop() {
    for ignored in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut builder = DirectBuilder::new(&mut work).unwrap();
            let mut writer = builder.begin_target(&mut work).unwrap();
            let result = writer.with_rows(|_, work| {
                if ignored {
                    work.fail(CoreError::ArithmeticOverflow);
                    Ok(())
                } else {
                    Err(CoreError::ArithmeticOverflow)
                }
            });
            assert_eq!(result, Err(CoreError::ArithmeticOverflow));
            assert!(matches!(
                writer.reserve_next(),
                Err(CoreError::ArithmeticOverflow)
            ));
            drop(writer);
            drop(builder);
            assert_eq!(work.finish(), Err(CoreError::ArithmeticOverflow));
        });
        released(&budget);
    }
}

#[test]
fn abandoned_pending_and_unwound_targets_poison_original_owner() {
    for panic in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut builder = DirectBuilder::new(&mut work).unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut writer = builder.begin_target(&mut work).unwrap();
                let _ticket = writer.reserve_next().unwrap();
                if panic {
                    panic!("direct target unwind control");
                }
            }));
            assert_eq!(result.is_err(), panic);
            assert!(matches!(
                builder.begin_target(&mut work),
                Err(CoreError::InvalidProvenance)
            ));
            drop(builder);
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[test]
fn duplicate_or_incomplete_permutation_never_yields_closed_target() {
    for duplicate in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let (mut builder, node, sources, _) = header(2, &mut work);
            let mut writer = builder.begin_target(&mut work).unwrap();
            let ticket = writer.reserve_next().unwrap();
            let first = writer
                .commit_owned(ticket, owned(node, sources, 0, 1))
                .unwrap();
            let ticket = writer.reserve_next().unwrap();
            writer
                .commit_owned(ticket, owned(node, sources, 1, 2))
                .unwrap();
            let values = if duplicate {
                vec![first, first]
            } else {
                vec![first]
            };
            let order = order(&mut writer, &values);
            assert!(matches!(
                writer.close(order),
                Err(CoreError::InvalidProvenance)
            ));
            drop(builder);
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[test]
fn cancellation_before_close_keeps_cancelled_as_first_fault() {
    let budget = SourceBudget::model_fixture();
    let token = CancellationToken::new();
    let mut operation = budget.begin(token.clone());
    construction_scope(&mut operation, |mut work| {
        let (mut builder, node, source, _) = header(1, &mut work);
        let mut writer = builder.begin_target(&mut work).unwrap();
        let ticket = writer.reserve_next().unwrap();
        let born = writer
            .commit_owned(ticket, owned(node, source, 0, 1))
            .unwrap();
        let order = order(&mut writer, &[born]);
        token.cancel();
        assert!(matches!(writer.close(order), Err(CoreError::Cancelled)));
        drop(builder);
        assert_eq!(work.finish(), Err(CoreError::Cancelled));
    });
    released(&budget);
}

#[test]
fn cross_page_permutation_fixes_every_stored_atom_and_primary_claim_site() {
    // This is a construction-state control, not a semantic fixture: reserved
    // graph rows intentionally remain unpublished. Each fixup uses real writers.
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        let count = 300;
        assert!(count * size_of::<storage::FinalSlot<AtomPacket>>() > 16384);
        let (mut builder, node, sources, flow) = header(count as u64, &mut work);
        let object = builder.reserve_object(&mut work).unwrap();
        let proof = builder.reserve_proof(&mut work).unwrap();
        let detail = builder.reserve_detail(&mut work).unwrap();
        let footnote_detail = builder.reserve_detail(&mut work).unwrap();
        let definition = builder.reserve_definition(&mut work).unwrap();
        let reference = builder.reserve_reference(&mut work).unwrap();
        let payload = builder.reserve_payload(&mut work).unwrap();
        let mut writer = builder.begin_target(&mut work).unwrap();
        let mut born: Vec<BornAtom> = Vec::new();
        for index in 0..count {
            let ticket = writer.reserve_next().unwrap();
            let value = if index == 1 {
                writer
                    .commit_shared(
                        ticket,
                        DraftAtomFields {
                            owner: node,
                            kind: AtomKind::Copy,
                            target: TargetSpan {
                                start: index as u64,
                                end: index as u64 + 1,
                            },
                            sources,
                            proof: None,
                            content: DraftAtomContent::Empty,
                        },
                        born[0].primary(),
                    )
                    .unwrap()
            } else {
                writer
                    .commit_owned(ticket, owned(node, sources, index as u64, index as u64 + 1))
                    .unwrap()
            };
            born.push(value);
        }
        writer
            .with_rows(|builder, work| {
                let text = builder.text("", work)?;
                let primitives = builder.primitive_parts(&[], work)?;
                let stages = builder.stage_parts(
                    &[DraftStageRow {
                        kind: StageKind::AsciiTrim,
                        input: LocalUtf8Span::new(0, 0)?,
                        output: text,
                        parameter: 0,
                        accepted_by: born[0].primary(),
                    }],
                    work,
                )?;
                builder.write_proof(
                    proof,
                    DraftProofRow {
                        atom: born[0].into(),
                        primitives,
                        stages,
                        value: text,
                    },
                    work,
                )?;
                builder.write_object(
                    object,
                    DraftObjectRow::Image {
                        node,
                        atom: born[0].into(),
                        alt: flow,
                        destination: payload,
                        title: None,
                        reference: None,
                        wiki_size: None,
                    },
                    work,
                )?;
                builder.write_detail(
                    detail,
                    DraftDetailRow::Html {
                        claim: born[0].primary(),
                    },
                    work,
                )?;
                builder.write_detail(
                    footnote_detail,
                    DraftDetailRow::FootnoteReference {
                        definition: node,
                        ordinal: 1,
                        occurrence: 1,
                        claim: born[0].primary(),
                        closing: Some(born[2].primary()),
                    },
                    work,
                )?;
                builder.write_definition(
                    definition,
                    DraftDefinitionRow {
                        syntax: sources,
                        label: payload,
                        destination: payload,
                        title: None,
                        key: text,
                        winner: true,
                        claim: born[0].primary(),
                    },
                    work,
                )?;
                builder.write_reference(
                    reference,
                    DraftReferenceUseRow {
                        node,
                        syntax: sources,
                        winner: definition,
                        key: text,
                        destination: payload,
                        title: None,
                        claim: born[0].primary(),
                    },
                    work,
                )?;
                builder.attribute(
                    DraftAttributeRow {
                        owner: node,
                        kind: AttributeKind::HeadingLevel,
                        claim: born[0].primary(),
                        sources,
                    },
                    work,
                )?;
                Ok(())
            })
            .unwrap();
        let reversed: Vec<_> = born.iter().rev().copied().collect();
        let order = order(&mut writer, &reversed);
        let closed = writer.close(order).unwrap();
        let final_atom = closed.resolve(born[0], &mut work).unwrap().0.value;
        assert_eq!(final_atom.index(), count - 1);
        let claim = ClaimId::primary(final_atom);
        for (index, original) in reversed.iter().enumerate() {
            let packet = builder.ir.packets.get(index, &mut work).unwrap();
            assert_eq!(packet.atom.target.start, original.index as u64);
        }
        assert_eq!(
            builder
                .ir
                .packets
                .get(count - 2, &mut work)
                .unwrap()
                .authority,
            AuthorityBinding::Shared(claim)
        );
        assert!(
            matches!(builder.read_object(object, &mut work).unwrap(), ObjectRow::Image { atom, .. } if atom == final_atom)
        );
        assert_eq!(
            builder.read_proof(proof, &mut work).unwrap().atom,
            final_atom
        );
        assert_eq!(
            builder.ir.stages.get(0, &mut work).unwrap().accepted_by,
            claim
        );
        assert_eq!(
            builder.read_detail(detail, &mut work).unwrap(),
            DetailRow::Html { claim }
        );
        let closing = ClaimId::primary(closed.resolve(born[2], &mut work).unwrap().0.value);
        assert_eq!(
            builder.read_detail(footnote_detail, &mut work).unwrap(),
            DetailRow::FootnoteReference {
                definition: node.value,
                ordinal: 1,
                occurrence: 1,
                claim,
                closing: Some(closing),
            }
        );
        assert_eq!(
            builder
                .read_definition(definition, &mut work)
                .unwrap()
                .claim,
            claim
        );
        assert_eq!(
            builder.read_reference(reference, &mut work).unwrap().claim,
            claim
        );
        assert_eq!(
            builder.ir.attributes.get(0, &mut work).unwrap().claim,
            claim
        );
        drop((closed, builder));
        work.finish().unwrap();
    });
    released(&budget);
}

#[test]
fn deadline_at_each_close_clock_boundary_prevents_publish_and_releases_partial_permutation() {
    fn run(at: Option<usize>) -> (usize, bool) {
        let budget = SourceBudget::task_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        operation.set_clock_samples_for_test(&[100_000_000]);
        let observed = construction_scope(&mut operation, |mut work| {
            let (mut builder, node, source, _) = header(300, &mut work);
            let mut writer = builder.begin_target(&mut work).unwrap();
            let mut born = Vec::new();
            for index in 0..300 {
                let ticket = writer.reserve_next().unwrap();
                born.push(
                    writer
                        .commit_owned(ticket, owned(node, source, index, index + 1))
                        .unwrap(),
                );
            }
            born.reverse();
            let order = order(&mut writer, &born);
            writer
                .with_read(|_, work| {
                    let mut samples = vec![100_000_000; at.unwrap_or(1)];
                    if let Some(at) = at {
                        samples[at - 1] = 500_000_000;
                    }
                    work.set_clock_samples_for_test(&samples);
                    Ok(())
                })
                .unwrap();
            let result = writer.close(order);
            let reads = work.clock_read_count_for_test();
            let changed = builder
                .ir
                .packets
                .raw_value_for_test(0)
                .unwrap()
                .atom
                .target
                .start
                != 0;
            if at.is_some() {
                let error = CoreError::DeadlineExceeded {
                    elapsed_ns: 500_000_000,
                    limit_ns: 500_000_000,
                };
                assert!(matches!(result, Err(value) if value == error));
                assert!(matches!(builder.begin_target(&mut work), Err(value) if value == error));
                drop(builder);
                assert_eq!(work.finish(), Err(error));
            } else {
                drop(result.unwrap());
                drop(builder);
                work.finish().unwrap();
            }
            (reads, changed)
        });
        released(&budget);
        observed
    }
    let (reads, changed) = run(None);
    assert!(changed);
    assert!(reads > 0);
    let mut saw_partial_permutation = false;
    for at in 1..=reads {
        saw_partial_permutation |= run(Some(at)).1;
    }
    assert!(saw_partial_permutation);
}

#[test]
fn caught_callback_unwind_after_real_rows_cannot_resume_live_target() {
    for mutable in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut builder = DirectBuilder::new(&mut work).unwrap();
            let mut writer = builder.begin_target(&mut work).unwrap();
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if mutable {
                    let _: Result<(), CoreError> = writer.with_rows(|builder, work| {
                        builder.source_parts(&[SourceSpan::new(0, 1)?], work)?;
                        panic!("unwind after actual final source write");
                    });
                } else {
                    let _: Result<(), CoreError> =
                        writer.with_read(|_, _| panic!("read callback unwind"));
                }
            }));
            assert!(caught.is_err());
            assert!(matches!(
                writer.reserve_next(),
                Err(CoreError::InvalidProvenance)
            ));
            drop(writer);
            assert_eq!(builder.ir.sources.len(), usize::from(mutable));
            drop(builder);
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[test]
fn node_seed_rejects_foreign_parent_flow_and_detail_before_reserving_values() {
    for field in 0..3 {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut builder = DirectBuilder::new(&mut work).unwrap();
            let flow = builder.reserve_flow(&mut work).unwrap();
            let mut other = DirectBuilder::new(&mut work).unwrap();
            let other_flow = other.reserve_flow(&mut work).unwrap();
            let other_node = other
                .reserve_node(node_seed(other_flow), &mut work)
                .unwrap();
            let other_detail = other.reserve_detail(&mut work).unwrap();
            // All foreign references really exist in their own builder.
            assert_eq!(other.graph_node(other_node, &mut work).unwrap().flow, 0);
            let mut seed = node_seed(flow);
            match field {
                0 => seed.parent = Some(other_node),
                1 => seed.flow = other_flow,
                _ => seed.detail = Some(other_detail),
            }
            assert_eq!(
                builder.reserve_node(seed, &mut work),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(builder.ir.nodes.len(), 0);
            assert_eq!(
                builder.reserve_node(node_seed(flow), &mut work),
                Err(CoreError::InvalidProvenance)
            );
            drop((builder, other));
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}
