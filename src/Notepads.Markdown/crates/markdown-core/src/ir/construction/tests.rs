// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::{LocalUtf8Span, LogicalSpan, SourceKey};
use crate::policy::{CancellationToken, ModelLimits, SourceLimits};
use crate::policy::{SourceBudget, construction_scope};
use crate::source::SourceDocument;

fn header<'id>(
    atoms: usize,
    extras: usize,
    op: &mut OwnedWork<'id, '_>,
) -> (ExactBuilder<'id>, NodeRef, SourceBlockRef, FlowRef) {
    let mut builder = ExactBuilder::new(
        ExactCounts {
            nodes: 1,
            flows: 1,
            atoms,
            extra_authorities: extras,
            sources: 1,
            ..ExactCounts::default()
        },
        op,
    )
    .unwrap();
    let owner = builder.reserve_node(0, op).unwrap();
    let flow = builder.reserve_flow(0, op).unwrap();
    let sources = builder
        .source_parts(&[SourceSpan::new(0, 1).unwrap()], op)
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
                source_envelope: SourceSpan::new(0, 1).unwrap(),
                logical: LogicalSpan::new(0, 1).unwrap(),
            },
            op,
        )
        .unwrap();
    let empty = builder.reserved_atom_block(0, 0, op).unwrap();
    builder
        .write_flow(
            flow,
            DraftFlowRow {
                scope: DraftFlowScope::Main,
                root: owner,
                domains: sources,
                atoms: empty.into(),
                logical_utf16: 1,
            },
            op,
        )
        .unwrap();
    (builder, owner, sources, flow)
}
fn fields(owner: NodeRef, sources: SourceBlockRef) -> DraftAtomFields {
    DraftAtomFields {
        owner,
        kind: AtomKind::Copy,
        target: TargetSpan { start: 0, end: 1 },
        sources,
        proof: None,
        content: DraftAtomContent::Empty,
    }
}
fn authority(owner: NodeRef, sources: SourceBlockRef) -> DraftAuthoritySpec {
    DraftAuthoritySpec {
        owner,
        phase: NativePhase::Block,
        kind: ClaimKind::Block,
        consumed: LocalUtf8Span::new(0, 1).unwrap(),
        sources,
        host: DraftHostRow {
            owner,
            role: HostRole::Block,
            utf8_len: 1,
            domains: sources,
        },
    }
}
fn all_released(budget: &SourceBudget) {
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn owned_packet_retains_complete_permission_and_passes_independent_validation() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(SourceKey::new([99; 16], 1).unwrap(), b"a", &mut op)
        .unwrap()
        .snapshot();
    construction_scope(&mut op, |mut op| {
        let (mut b, owner, sources, flow) = header(1, 0, &mut op);
        let ticket = b.reserve_atom(0, &mut op).unwrap();
        let primary = ticket.primary_ref();
        b.write_owned_atom(
            ticket,
            fields(owner, sources),
            authority(owner, sources),
            &mut op,
        )
        .unwrap();
        let atoms = b.reserved_atom_block(0, 1, &mut op).unwrap();
        b.patch_flow_target(flow, atoms, 1, &mut op).unwrap();
        let ir = b.finish(&mut op).unwrap();
        let atom = ir.owned_atom_at(0, &mut op).unwrap();
        assert_eq!(atom.claim, primary.local());
        let spec = ir.owned_authority(atom.claim, &mut op).unwrap();
        assert_eq!(spec.host.domains, spec.sources);
        assert_eq!(spec.consumed, LocalUtf8Span::new(0, 1).unwrap());
        assert_eq!(ir.extra_authorities_len(), 0);
        validate::validate_owned(&ir, &source, &mut op).unwrap();
        eprintln!(
            "permission layout: AtomPacket={}B align={}, ExtraAuthority={}B align={}",
            size_of::<AtomPacket>(),
            align_of::<AtomPacket>(),
            size_of::<ExtraAuthority>(),
            align_of::<ExtraAuthority>()
        );
        assert!(size_of::<AtomPacket>() <= 96);
        assert!(size_of::<ExtraAuthority>() <= 64);
        drop(ir);
        drop(source);
        all_released(&budget);
        assert_eq!(budget.usage().source_capacity, 0);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn foreign_primary_and_extra_are_rejected_even_with_equal_local_ids() {
    for extra in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let (mut a, ao, asrc, _) = header(1, usize::from(extra), &mut op);
            let (mut b, bo, bsrc, _) = header(1, usize::from(extra), &mut op);
            let foreign = if extra {
                a.write_extra_authority(authority(ao, asrc), &mut op)
                    .unwrap()
            } else {
                let t = a.reserve_atom(0, &mut op).unwrap();
                let r = t.primary_ref();
                a.write_owned_atom(t, fields(ao, asrc), authority(ao, asrc), &mut op)
                    .unwrap();
                r
            };
            if extra {
                let local = b
                    .write_extra_authority(authority(bo, bsrc), &mut op)
                    .unwrap();
                assert_eq!(local.local(), foreign.local());
            }
            let ticket = b.reserve_atom(0, &mut op).unwrap();
            assert!(matches!(
                b.write_shared_atom(ticket, fields(bo, bsrc), foreign, &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
            drop(a);
            drop(b);
            all_released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn shared_bindings_require_a_terminal_owned_permission_and_preserve_full_host() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let (mut b, owner, sources, _) = header(3, 0, &mut op);
        let first = b.reserve_atom(0, &mut op).unwrap();
        let permission = first.primary_ref();
        let mut spec = authority(owner, sources);
        spec.host.utf8_len = 17;
        spec.consumed = LocalUtf8Span::new(2, 7).unwrap();
        b.write_owned_atom(first, fields(owner, sources), spec, &mut op)
            .unwrap();
        let second = b.reserve_atom(1, &mut op).unwrap();
        let nonterminal = second.primary_ref();
        b.write_shared_atom(second, fields(owner, sources), permission, &mut op)
            .unwrap();
        assert_eq!(
            b.ir.owned_atom_at(1, &mut op).unwrap().claim,
            permission.local()
        );
        let actual = b.ir.authority(permission.local(), &mut op).unwrap();
        assert_eq!(actual.host.utf8_len, 17);
        assert_eq!(actual.consumed, LocalUtf8Span::new(2, 7).unwrap());
        let third = b.reserve_atom(2, &mut op).unwrap();
        assert!(matches!(
            b.write_shared_atom(third, fields(owner, sources), nonterminal, &mut op),
            Err(CoreError::InvalidProvenance)
        ));
        drop(b);
        all_released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn every_nested_owner_and_source_handle_must_have_the_current_scope() {
    for field in 0..6 {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let (a, ao, asrc, _) = header(0, 0, &mut op);
            let (mut b, bo, bsrc, _) = header(1, 0, &mut op);
            assert_eq!(ao.local(), bo.local());
            assert_eq!(asrc.local(), bsrc.local());
            let mut atom = fields(bo, bsrc);
            let mut spec = authority(bo, bsrc);
            match field {
                0 => atom.owner = ao,
                1 => spec.host.owner = ao,
                2 => atom.sources = asrc,
                3 => spec.host.domains = asrc,
                4 => spec.owner = ao,
                5 => spec.sources = asrc,
                _ => unreachable!(),
            }
            let ticket = b.reserve_atom(0, &mut op).unwrap();
            assert!(matches!(
                b.write_owned_atom(ticket, atom, spec, &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            drop(a);
            drop(b);
            all_released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn optional_proofs_pool_text_and_objects_reject_foreign_scopes() {
    for field in 0..4 {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let counts = ExactCounts {
                nodes: 1,
                atoms: 1,
                objects: 1,
                proofs: 1,
                text_bytes: 1,
                ..ExactCounts::default()
            };
            let mut a = ExactBuilder::new(counts, &mut op).unwrap();
            let mut b = ExactBuilder::new(counts, &mut op).unwrap();
            let ao = a.reserve_node(0, &mut op).unwrap();
            let bo = b.reserve_node(0, &mut op).unwrap();
            assert_eq!(ao.local(), bo.local());
            let asrc = a.source_parts(&[], &mut op).unwrap();
            let bsrc = b.source_parts(&[], &mut op).unwrap();
            assert_eq!(asrc.local(), bsrc.local());
            let ap = a.reserve_proof(0, &mut op).unwrap();
            let bp = b.reserve_proof(0, &mut op).unwrap();
            let ax = a.reserve_object(0, &mut op).unwrap();
            let bx = b.reserve_object(0, &mut op).unwrap();
            let at = a.text("a", &mut op).unwrap();
            let bt = b.text("a", &mut op).unwrap();
            assert_eq!(ap.local(), bp.local());
            assert_eq!(ax.local(), bx.local());
            assert_eq!(at.local(), bt.local());
            let mut atom = fields(bo, bsrc);
            match field {
                0 => atom.proof = Some(ap),
                1 => atom.content = DraftAtomContent::Pool(at),
                2 => atom.content = DraftAtomContent::Object(ax),
                3 => atom.sources = asrc,
                _ => unreachable!(),
            }
            let ticket = b.reserve_atom(0, &mut op).unwrap();
            assert!(matches!(
                b.write_owned_atom(ticket, atom, authority(bo, bsrc), &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            drop(a);
            drop(b);
            all_released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn narrow_views_charge_real_owned_shared_extra_and_rejected_selector_reads() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let (mut b, owner, sources, flow) = header(3, 1, &mut op);
        let first = b.reserve_atom(0, &mut op).unwrap();
        let primary = first.primary_ref();
        b.write_owned_atom(
            first,
            fields(owner, sources),
            authority(owner, sources),
            &mut op,
        )
        .unwrap();
        let second = b.reserve_atom(1, &mut op).unwrap();
        let nonterminal = second.primary_ref();
        b.write_shared_atom(second, fields(owner, sources), primary, &mut op)
            .unwrap();
        let extra = b
            .write_extra_authority(authority(owner, sources), &mut op)
            .unwrap();
        let third = b.reserve_atom(2, &mut op).unwrap();
        b.write_shared_atom(third, fields(owner, sources), extra, &mut op)
            .unwrap();
        let block = b.reserved_atom_block(0, 3, &mut op).unwrap();
        b.patch_flow_target(flow, block, 1, &mut op).unwrap();
        let ir = b.finish(&mut op).unwrap();
        for (index, shared) in [(0, false), (1, true), (2, true)] {
            let before = op.stats().expect("statistics enabled");
            ir.owned_atom_at(index, &mut op).unwrap();
            assert_eq!(
                op.stats().expect("statistics enabled").scanned_bytes() - before.scanned_bytes(),
                size_of::<AtomFields>() as u64
                    + types::AUTHORITY_SELECTOR_BYTES
                    + if shared {
                        size_of::<ClaimId>() as u64
                    } else {
                        0
                    }
            );
            assert_eq!(
                op.stats().expect("statistics enabled").copied_bytes() - before.copied_bytes(),
                size_of::<AtomRow>() as u64
            );
        }
        for (id, expected) in [
            (
                primary.local(),
                types::AUTHORITY_SELECTOR_BYTES
                    + (size_of::<PrimaryAuthority>()
                        + size_of::<NodeId>()
                        + size_of::<Rows<SourceSpan>>()) as u64,
            ),
            (extra.local(), size_of::<ExtraAuthority>() as u64),
        ] {
            let before = op.stats().expect("statistics enabled");
            ir.owned_authority(id, &mut op).unwrap();
            assert_eq!(
                op.stats().expect("statistics enabled").scanned_bytes() - before.scanned_bytes(),
                expected
            );
            assert_eq!(
                op.stats().expect("statistics enabled").copied_bytes() - before.copied_bytes(),
                size_of::<AuthoritySpec>() as u64
            );
        }
        let before = op.stats().expect("statistics enabled");
        assert_eq!(
            ir.owned_authority(nonterminal.local(), &mut op),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(
            op.stats().expect("statistics enabled").scanned_bytes() - before.scanned_bytes(),
            types::AUTHORITY_SELECTOR_BYTES
        );
        assert_eq!(
            op.stats().expect("statistics enabled").copied_bytes() - before.copied_bytes(),
            size_of::<AuthoritySpec>() as u64
        );
        drop(ir);
        all_released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn owned_factory_checks_complete_sources_and_owner_but_keeps_distinct_host_domains() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let mut b = ExactBuilder::new(
            ExactCounts {
                nodes: 2,
                flows: 1,
                atoms: 1,
                sources: 3,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        let owner = b.reserve_node(0, &mut op).unwrap();
        b.reserve_node(1, &mut op).unwrap();
        let source = b
            .source_parts(&[SourceSpan::new(0, 1).unwrap()], &mut op)
            .unwrap();
        let equal = b
            .source_parts(&[SourceSpan::new(0, 1).unwrap()], &mut op)
            .unwrap();
        let domain = b
            .source_parts(&[SourceSpan::new(0, 5).unwrap()], &mut op)
            .unwrap();
        let mut spec = authority(owner, equal);
        spec.host.domains = domain;
        let ticket = b.reserve_atom(0, &mut op).unwrap();
        b.write_owned_atom(ticket, fields(owner, source), spec, &mut op)
            .unwrap();
        let atom = b.ir.owned_atom_at(0, &mut op).unwrap();
        let actual = b.ir.authority(atom.claim, &mut op).unwrap();
        assert_eq!(actual.sources, source.local());
        assert_eq!(actual.host.domains, domain.local());
        drop(b);
        all_released(&budget);
        // One owner field differs in the same scope; no implicit folding is allowed.
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let mut b = ExactBuilder::new(
                ExactCounts {
                    nodes: 2,
                    atoms: 1,
                    sources: 1,
                    ..ExactCounts::default()
                },
                &mut op,
            )
            .unwrap();
            let owner = b.reserve_node(0, &mut op).unwrap();
            let other = b.reserve_node(1, &mut op).unwrap();
            let source = b
                .source_parts(&[SourceSpan::new(0, 1).unwrap()], &mut op)
                .unwrap();
            let ticket = b.reserve_atom(0, &mut op).unwrap();
            let mut spec = authority(owner, source);
            spec.host.owner = other;
            assert!(matches!(
                b.write_owned_atom(ticket, fields(owner, source), spec, &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            drop(b);
            all_released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn equal_source_envelopes_do_not_make_different_complete_grants_equal() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let mut b = ExactBuilder::new(
            ExactCounts {
                nodes: 1,
                atoms: 1,
                sources: 4,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        let owner = b.reserve_node(0, &mut op).unwrap();
        let left = b
            .source_parts(
                &[
                    SourceSpan::new(0, 1).unwrap(),
                    SourceSpan::new(3, 4).unwrap(),
                ],
                &mut op,
            )
            .unwrap();
        let right = b
            .source_parts(
                &[
                    SourceSpan::new(0, 2).unwrap(),
                    SourceSpan::new(3, 4).unwrap(),
                ],
                &mut op,
            )
            .unwrap();
        let ticket = b.reserve_atom(0, &mut op).unwrap();
        assert!(matches!(
            b.write_owned_atom(
                ticket,
                fields(owner, left),
                authority(owner, right),
                &mut op
            ),
            Err(CoreError::InvalidProvenance)
        ));
        drop(b);
        all_released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn foreign_tickets_cannot_be_consumed_or_stored_at_equal_local_ordinals() {
    for in_slot in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let (mut a, _, _, _) = header(1, 0, &mut op);
            let (mut b, owner, source, _) = header(1, 0, &mut op);
            let ticket = a.reserve_atom(0, &mut op).unwrap();
            let local = b.reserve_atom(0, &mut op).unwrap();
            assert_eq!(ticket.atom_ref().local(), local.atom_ref().local());
            if in_slot {
                let mut slots = b.atom_slots(&mut op).unwrap();
                assert_eq!(
                    slots.store(0, ticket, &mut op),
                    Err(CoreError::InvalidProvenance)
                );
                drop(slots);
            } else {
                assert!(matches!(
                    b.write_owned_atom(
                        ticket,
                        fields(owner, source),
                        authority(owner, source),
                        &mut op
                    ),
                    Err(CoreError::InvalidProvenance)
                ));
            }
            drop(a);
            drop(b);
            all_released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}

#[test]
fn tickets_slots_and_finish_reject_reissue_restore_and_incomplete_rows() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let (mut b, _, _, _) = header(1, 0, &mut op);
        let mut slots = b.atom_slots(&mut op).unwrap();
        let ticket = b.reserve_atom(0, &mut op).unwrap();
        slots.store(0, ticket, &mut op).unwrap();
        let ticket = slots.take(0, &mut op).unwrap();
        assert_eq!(
            slots.store(0, ticket, &mut op),
            Err(CoreError::InvalidProvenance)
        );
        drop(slots);
        drop(b);
        all_released(&budget);
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let (mut b, _, _, _) = header(1, 0, &mut op);
            let _unused_ticket = b.reserve_atom(0, &mut op).unwrap();
            assert!(matches!(
                b.reserve_atom(0, &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            drop(b);
            all_released(&budget);
            let mut test_operation = budget.begin(CancellationToken::new());
            construction_scope(&mut test_operation, |mut op| {
                let (b, _, _, _) = header(1, 0, &mut op);
                assert!(matches!(
                    b.finish(&mut op),
                    Err(CoreError::InvalidProvenance)
                ));
                all_released(&budget);
                let first = op.first_fault();
                assert_eq!(op.finish().err(), first);
            });
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn readonly_faults_poison_construction_and_keep_the_original_fault_after_cancel() {
    let budget = SourceBudget::model_fixture();
    let cancel = CancellationToken::new();
    let mut test_operation = budget.begin(cancel.clone());
    construction_scope(&mut test_operation, |mut op| {
        let (b, _, _, _) = header(0, 0, &mut op);
        let foreign = SourceBudget::model_fixture();
        assert_eq!(op.ensure_budget(&foreign), Err(CoreError::WrongBudget));
        assert_eq!(
            b.written_source_block(0, 1, &mut op),
            Err(CoreError::WrongBudget)
        );
        cancel.cancel();
        assert_eq!(
            b.written_source_block(0, 1, &mut op),
            Err(CoreError::WrongBudget)
        );
        assert_eq!(op.first_fault(), Some(CoreError::WrongBudget));
        drop(b);
        all_released(&budget);
        all_released(&foreign);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn count_and_capacity_failures_are_typed_and_release_every_lease() {
    let budget = SourceBudget::model_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let over = ModelLimits::default().model_records as usize + 1;
        assert!(matches!(
            ExactBuilder::new(
                ExactCounts {
                    atoms: over,
                    ..ExactCounts::default()
                },
                &mut op
            ),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::ModelRecords,
                ..
            })
        ));
        all_released(&budget);
        let budget = SourceBudget::new_with_model_limits(
            SourceLimits {
                auxiliary_capacity: 0,
                ..SourceLimits::default()
            },
            ModelLimits::default(),
        )
        .unwrap();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let mut b = ExactBuilder::new(
                ExactCounts {
                    atoms: 1,
                    ..ExactCounts::default()
                },
                &mut op,
            )
            .unwrap();
            assert!(matches!(
                b.reserve_atom(0, &mut op),
                Err(CoreError::ResourceLimit {
                    kind: LimitKind::AuxiliaryCapacity,
                    ..
                })
            ));
            let first = op.first_fault().unwrap();
            let cancel = CancellationToken::new();
            cancel.cancel();
            assert!(matches!(b.reserve_atom(0, &mut op), Err(error) if error == first));
            assert_eq!(op.first_fault(), Some(first));
            drop(b);
            all_released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}
