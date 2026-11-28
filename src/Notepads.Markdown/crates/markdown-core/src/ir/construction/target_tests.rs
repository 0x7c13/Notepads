// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Construction state controls, including independently validated complete IRs.

use super::*;
use crate::identity::{LocalUtf8Span, LogicalSpan, SourceKey};
use crate::policy::CancellationToken;
use crate::policy::{SourceBudget, construction_scope};
use crate::source::SourceDocument;

pub(super) fn header<'id>(
    mut counts: ExactCounts,
    bytes: u64,
    logical: u64,
    op: &mut OwnedWork<'id, '_>,
) -> (ExactBuilder<'id>, NodeRef, SourceBlockRef, FlowRef) {
    counts.nodes = 1;
    counts.flows = 1;
    counts.sources = 1;
    let mut builder = ExactBuilder::new(counts, op).unwrap();
    let nodes = builder.reserve_nodes(op).unwrap();
    let flows = builder.reserve_flows(op).unwrap();
    let owner = nodes.at(0, op).unwrap();
    let flow = flows.at(0, op).unwrap();
    let sources = builder
        .source_parts(&[SourceSpan::new(0, bytes).unwrap()], op)
        .unwrap();
    builder
        .write_node(
            owner,
            DraftNodeRow {
                parent: None,
                first_child: None,
                next_sibling: None,
                flow,
                kind: NodeKind::Document,
                detail: None,
                source_envelope: SourceSpan::new(0, bytes).unwrap(),
                logical: LogicalSpan::new(0, logical).unwrap(),
            },
            op,
        )
        .unwrap();
    (builder, owner, sources, flow)
}

pub(super) fn owned(owner: NodeRef, sources: SourceBlockRef, length: u64) -> DraftOwnedAtom {
    DraftOwnedAtom {
        owner,
        kind: AtomKind::Copy,
        target: TargetSpan {
            start: 0,
            end: length,
        },
        sources,
        proof: None,
        content: DraftAtomContent::Empty,
        primary: DraftPrimaryAuthority {
            phase: NativePhase::Block,
            kind: ClaimKind::Block,
            consumed: LocalUtf8Span::new(0, length).unwrap(),
            host_role: HostRole::Block,
            host_utf8_len: length,
            host_domains: sources,
        },
    }
}

fn flow_row(
    owner: NodeRef,
    domains: SourceBlockRef,
    atoms: WrittenAtomBlock,
    logical_utf16: u64,
) -> DraftFlowRow {
    DraftFlowRow {
        scope: DraftFlowScope::Main,
        root: owner,
        domains,
        atoms: atoms.into(),
        logical_utf16,
    }
}

pub(super) fn released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

#[test]
fn actual_table_reservations_and_completed_owned_block_validate() {
    for text in ["", "a"] {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([101; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        construction_scope(&mut op, |mut op| {
            let count = usize::from(!text.is_empty());
            let len = text.len() as u64;
            let (mut builder, owner, sources, flow) = header(
                ExactCounts {
                    atoms: count,
                    ..ExactCounts::default()
                },
                len,
                len,
                &mut op,
            );
            let mut writer = builder.begin_target(count, &mut op).unwrap();
            if count != 0 {
                let ticket = writer.reserve_next().unwrap();
                writer
                    .commit_owned(ticket, owned(owner, sources, len))
                    .unwrap();
            }
            let block = writer.close().unwrap();
            builder
                .write_flow(flow, flow_row(owner, sources, block, len), &mut op)
                .unwrap();
            let ir = builder.finish(&mut op).unwrap();
            validate::validate_owned(&ir, &source, &mut op).unwrap();
            assert_eq!(ir.atoms_len(), count);
            assert_eq!(ir.flows()[0].atoms.len(), count);
            drop((ir, source));
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn table_reservations_reject_duplicates_including_empty_tables() {
    for count in [0, 1] {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let mut builder = ExactBuilder::new(
                ExactCounts {
                    nodes: count,
                    ..ExactCounts::default()
                },
                &mut op,
            )
            .unwrap();
            let table = builder.reserve_nodes(&mut op).unwrap();
            if count != 0 {
                let reference = table.at(0, &mut op).unwrap();
                assert_eq!(
                    builder
                        .reference(reference, false, &mut op)
                        .unwrap()
                        .index(),
                    0
                );
            }
            assert!(matches!(
                builder.reserve_nodes(&mut op),
                Err(CoreError::InvalidProvenance)
            ));
            assert!(matches!(
                builder.begin_target(0, &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            drop((table, builder));
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn table_handles_reject_foreign_roots_and_foreign_builders() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let mut first = ExactBuilder::new(
            ExactCounts {
                nodes: 1,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        let table = first.reserve_nodes(&mut op).unwrap();
        let reference = table.at(0, &mut op).unwrap();
        let mut second = ExactBuilder::new(
            ExactCounts {
                nodes: 1,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        second.reserve_nodes(&mut op).unwrap();
        assert_eq!(
            second.reference(reference, false, &mut op),
            Err(CoreError::InvalidProvenance)
        );
        let foreign_budget = SourceBudget::model_fixture();
        // Another owner cannot type-check here. The original owner keeps its first error.
        assert_eq!(
            op.ensure_budget(&foreign_budget),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(table.at(0, &mut op), Err(CoreError::InvalidProvenance));
        drop((table, first, second));
        released(&budget);
        released(&foreign_budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn a_second_outstanding_ticket_poison_is_not_overwritten_by_cancel() {
    let budget = SourceBudget::model_fixture();
    let cancel = CancellationToken::new();
    let mut test_operation = budget.begin(cancel.clone());
    construction_scope(&mut test_operation, |mut op| {
        let (mut builder, owner, sources, _) = header(
            ExactCounts {
                atoms: 2,
                ..ExactCounts::default()
            },
            1,
            1,
            &mut op,
        );
        let mut writer = builder.begin_target(2, &mut op).unwrap();
        let ticket = writer.reserve_next().unwrap();
        assert!(matches!(
            writer.reserve_next(),
            Err(CoreError::InvalidProvenance)
        ));
        cancel.cancel();
        assert_eq!(
            writer.commit_owned(ticket, owned(owner, sources, 1)),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(writer.close(), Err(CoreError::InvalidProvenance));
        assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
        drop(builder);
        released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn abandoned_or_incomplete_target_cannot_resume_with_same_owner() {
    for abandoned in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let mut builder = ExactBuilder::new(
                ExactCounts {
                    atoms: 1,
                    ..ExactCounts::default()
                },
                &mut op,
            )
            .unwrap();
            let writer = builder.begin_target(1, &mut op).unwrap();
            if abandoned {
                drop(writer);
            } else {
                assert_eq!(writer.close(), Err(CoreError::InvalidProvenance));
            }
            assert!(matches!(
                builder.begin_target(1, &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            drop(builder);
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn foreign_ticket_and_foreign_completed_empty_block_are_rejected() {
    for block in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let atoms = usize::from(!block);
            let (mut first, _, _, _) = header(
                ExactCounts {
                    atoms,
                    ..ExactCounts::default()
                },
                1,
                1,
                &mut op,
            );
            let (mut second, owner, sources, flow) = header(
                ExactCounts {
                    atoms,
                    ..ExactCounts::default()
                },
                1,
                1,
                &mut op,
            );
            if block {
                let foreign = first.begin_target(0, &mut op).unwrap().close().unwrap();
                assert!(matches!(
                    second.write_flow(flow, flow_row(owner, sources, foreign, 0), &mut op),
                    Err(CoreError::InvalidProvenance)
                ));
            } else {
                let foreign = first.reserve_atom(0, &mut op).unwrap();
                let mut writer = second.begin_target(1, &mut op).unwrap();
                let local = writer.reserve_next().unwrap();
                assert_eq!(local.atom_ref().local(), foreign.atom_ref().local());
                assert_eq!(
                    writer.commit_owned(foreign, owned(owner, sources, 1)),
                    Err(CoreError::InvalidProvenance)
                );
                drop((local, writer));
            }
            drop((first, second));
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn single_owned_input_still_checks_every_independent_foreign_reference() {
    for field in 0..6 {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let counts = ExactCounts {
                atoms: 1,
                proofs: 1,
                objects: 1,
                text_bytes: 1,
                ..ExactCounts::default()
            };
            let (mut first, foreign_owner, foreign_sources, _) = header(counts, 1, 1, &mut op);
            let (mut second, owner, sources, _) = header(counts, 1, 1, &mut op);
            let foreign_proof = first.reserve_proof(0, &mut op).unwrap();
            second.reserve_proof(0, &mut op).unwrap();
            let foreign_object = first
                .reserve_objects(&mut op)
                .unwrap()
                .at(0, &mut op)
                .unwrap();
            second.reserve_objects(&mut op).unwrap();
            let foreign_text = first.text("a", &mut op).unwrap();
            second.text("a", &mut op).unwrap();
            let mut value = owned(owner, sources, 1);
            match field {
                0 => value.owner = foreign_owner,
                1 => value.sources = foreign_sources,
                2 => value.primary.host_domains = foreign_sources,
                3 => value.proof = Some(foreign_proof),
                4 => value.content = DraftAtomContent::Pool(foreign_text),
                5 => value.content = DraftAtomContent::Object(foreign_object),
                _ => unreachable!(),
            }
            let mut writer = second.begin_target(1, &mut op).unwrap();
            let ticket = writer.reserve_next().unwrap();
            assert_eq!(
                writer.commit_owned(ticket, value),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(writer.close(), Err(CoreError::InvalidProvenance));
            drop((first, second));
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn committed_ticket_cannot_be_forged_back_into_an_outstanding_slot() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let (mut builder, owner, sources, _) = header(
            ExactCounts {
                atoms: 2,
                ..ExactCounts::default()
            },
            1,
            1,
            &mut op,
        );
        let mut writer = builder.begin_target(2, &mut op).unwrap();
        let first = writer.reserve_next().unwrap();
        let old_reference = first.atom_ref();
        writer
            .commit_owned(first, owned(owner, sources, 1))
            .unwrap();
        let _pending = writer.reserve_next().unwrap();
        // Only this private negative control can manufacture a ticket. A matching
        // scope is insufficient: both its actual state and pending position differ.
        let duplicate = ReservedAtom {
            reference: old_reference,
        };
        assert_eq!(
            writer.commit_owned(duplicate, owned(owner, sources, 1)),
            Err(CoreError::InvalidProvenance)
        );
        drop(writer);
        drop(builder);
        released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn auxiliary_binding_failure_poison_reaches_commit_close_and_drop() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let (mut builder, owner, sources, _) = header(
            ExactCounts {
                atoms: 1,
                ..ExactCounts::default()
            },
            1,
            1,
            &mut op,
        );
        let mut writer = builder.begin_target(1, &mut op).unwrap();
        let ticket = writer.reserve_next().unwrap();
        assert_eq!(
            writer.with_read(|b, op| b.written_source_block(0, 2, op)),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(
            writer.commit_owned(ticket, owned(owner, sources, 1)),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(writer.close(), Err(CoreError::InvalidProvenance));
        assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
        drop(builder);
        released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn auxiliary_primary_and_secondary_stages_replay_and_cancel_before_commit() {
    for (cancel_before_commit, connected) in [(false, true), (true, true), (false, false)] {
        let budget = SourceBudget::model_fixture();
        let cancel = CancellationToken::new();
        let mut test_operation = budget.begin(cancel.clone());
        let raw: &[u8] = if connected { b"\t" } else { b"\t&#65;" };
        let logical = if connected { 3 } else { 5 };
        let source = SourceDocument::new(
            SourceKey::new([102; 16], 1).unwrap(),
            raw,
            &mut test_operation,
        )
        .unwrap()
        .snapshot();
        construction_scope(&mut test_operation, |mut op| {
            let (mut builder, owner, sources, flow) = header(
                ExactCounts {
                    atoms: 1,
                    extra_authorities: 1,
                    proofs: 1,
                    primitives: 1,
                    stages: 2,
                    text_bytes: if connected { 7 } else { 10 },
                    ..ExactCounts::default()
                },
                raw.len() as u64,
                logical,
                &mut op,
            );
            let spaces = builder.text("    ", &mut op).unwrap();
            let secondary_output = builder
                .text(if connected { "" } else { "A" }, &mut op)
                .unwrap();
            let expected_value = if connected { "   " } else { "    A" };
            let value = builder.text(expected_value, &mut op).unwrap();
            let mut writer = builder.begin_target(1, &mut op).unwrap();
            let ticket = writer.reserve_next().unwrap();
            let proof = writer.reserve_proof(0).unwrap();
            writer
                .primitive_parts(&[PrimitiveRow {
                    source: SourceSpan::new(0, raw.len() as u64).unwrap(),
                    kind: PrimitiveKind::Copy,
                }])
                .unwrap();
            let primitives = writer.written_primitive_block(0, 1).unwrap();
            writer
                .stage_parts(&[DraftStageRow {
                    kind: StageKind::PartialTab,
                    input: LocalUtf8Span::new(0, 1).unwrap(),
                    output: spaces,
                    parameter: 0,
                    accepted_by: ticket.primary_ref(),
                }])
                .unwrap();
            let secondary = writer
                .write_extra_authority(DraftAuthoritySpec {
                    owner,
                    phase: NativePhase::Inline,
                    kind: if connected {
                        ClaimKind::Code
                    } else {
                        ClaimKind::Entity
                    },
                    consumed: if connected {
                        LocalUtf8Span::new(0, 1).unwrap()
                    } else {
                        LocalUtf8Span::new(4, 9).unwrap()
                    },
                    sources,
                    host: DraftHostRow {
                        owner,
                        role: HostRole::Inline,
                        utf8_len: if connected { 4 } else { 9 },
                        domains: sources,
                    },
                })
                .unwrap();
            writer
                .stage_parts(&[DraftStageRow {
                    kind: if connected {
                        StageKind::CodeEdgeTrim
                    } else {
                        StageKind::NumericEntity
                    },
                    input: if connected {
                        LocalUtf8Span::new(0, 1).unwrap()
                    } else {
                        LocalUtf8Span::new(4, 9).unwrap()
                    },
                    output: secondary_output,
                    parameter: 0,
                    accepted_by: secondary,
                }])
                .unwrap();
            let stages = writer.written_stage_block(0, 2).unwrap();
            writer
                .write_proof(
                    proof,
                    DraftProofRow {
                        atom: ticket.atom_ref(),
                        primitives,
                        stages,
                        value,
                    },
                )
                .unwrap();
            let mut atom = owned(owner, sources, raw.len() as u64);
            atom.kind = AtomKind::Transform;
            atom.target.end = logical;
            atom.content = DraftAtomContent::Pool(value);
            atom.proof = Some(proof);
            atom.primary.kind = ClaimKind::ContainerPrefix;
            if cancel_before_commit {
                cancel.cancel();
                assert_eq!(writer.commit_owned(ticket, atom), Err(CoreError::Cancelled));
                assert_eq!(writer.close(), Err(CoreError::Cancelled));
                assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
                drop((builder, source));
            } else {
                writer.commit_owned(ticket, atom).unwrap();
                let block = writer.close().unwrap();
                builder
                    .write_flow(flow, flow_row(owner, sources, block, logical), &mut op)
                    .unwrap();
                let ir = builder.finish(&mut op).unwrap();
                if connected {
                    validate::validate_owned(&ir, &source, &mut op).unwrap();
                } else {
                    // Two independent conversions cannot be hidden in one proof:
                    // preserve the original failed setup as a minimality control.
                    assert_eq!(
                        validate::validate_owned(&ir, &source, &mut op),
                        Err(CoreError::InvalidProvenance)
                    );
                }
                assert_eq!(ir.text(ir.proofs()[0].value).unwrap(), expected_value);
                assert_eq!(ir.extra_authorities_len(), 1);
                assert_ne!(ir.stages()[0].accepted_by, ir.stages()[1].accepted_by);
                drop((ir, source));
            }
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn shared_terminal_extra_validates_but_a_shared_primary_cannot_chain() {
    for chain in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(SourceKey::new([103; 16], 1).unwrap(), b"a", &mut op)
            .unwrap()
            .snapshot();
        construction_scope(&mut op, |mut op| {
            let (mut builder, owner, sources, flow) = header(
                ExactCounts {
                    atoms: if chain { 2 } else { 1 },
                    extra_authorities: 1,
                    ..ExactCounts::default()
                },
                1,
                1,
                &mut op,
            );
            let mut writer = builder
                .begin_target(if chain { 2 } else { 1 }, &mut op)
                .unwrap();
            let extra = writer
                .write_extra_authority(DraftAuthoritySpec {
                    owner,
                    sources,
                    phase: NativePhase::Block,
                    kind: ClaimKind::Block,
                    consumed: LocalUtf8Span::new(0, 1).unwrap(),
                    host: DraftHostRow {
                        owner,
                        role: HostRole::Block,
                        utf8_len: 1,
                        domains: sources,
                    },
                })
                .unwrap();
            let first = writer.reserve_next().unwrap();
            let shared_primary = first.primary_ref();
            let fields = DraftAtomFields {
                owner,
                kind: AtomKind::Copy,
                target: TargetSpan { start: 0, end: 1 },
                sources,
                proof: None,
                content: DraftAtomContent::Empty,
            };
            writer.commit_shared(first, fields, extra).unwrap();
            if chain {
                let second = writer.reserve_next().unwrap();
                assert_eq!(
                    writer.commit_shared(second, fields, shared_primary),
                    Err(CoreError::InvalidProvenance)
                );
                drop(writer);
                drop((builder, source));
            } else {
                let block = writer.close().unwrap();
                builder
                    .write_flow(flow, flow_row(owner, sources, block, 1), &mut op)
                    .unwrap();
                let ir = builder.finish(&mut op).unwrap();
                validate::validate_owned(&ir, &source, &mut op).unwrap();
                assert_eq!(ir.owned_atom_at(0, &mut op).unwrap().claim, extra.local());
                drop((ir, source));
            }
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}
