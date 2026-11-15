// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Replays the original independently authored IR controls through the facts path.
//! The transfer helper copies raw fixture rows, never candidate-derived expected results.

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::ModelIdentity;
use crate::ir::validate::validate_for_index_owned as validate_for_index;
use crate::mapping::MapIndex;
use crate::policy::construction_scope;
use crate::policy::{ModelLimits, SourceLimits};

struct Migrated {
    ir: CompactIr,
    source: SourceSnapshot,
    budget: SourceBudget,
}

fn operation(budget: &SourceBudget) -> Operation {
    let mut op = budget.begin(CancellationToken::new());
    assert_eq!(op.first_fault(), None);
    if op.uses_task_access() {
        op.set_clock_samples_for_test(&[499_999_999]);
    }
    op
}

fn identity(source: &SourceSnapshot) -> ModelIdentity {
    ModelIdentity::new(source.key(), 1, ParseConfig::new(Profile::C0)).unwrap()
}

fn assert_released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

fn transfer(ir: &CompactIr, source: &SourceSnapshot, budget: SourceBudget) -> Migrated {
    let mut op = operation(&budget);
    let source = SourceDocument::new(source.key(), source.text().as_bytes(), &mut op)
        .unwrap()
        .snapshot();
    let mut builder = IrBuilder::new(&mut op).unwrap();
    // Preserve every ID, row, source coordinate, bad edge and unused entry exactly.
    // Only allocation ownership changes to the fresh finite test root.
    macro_rules! rows {
        ($($field:ident),+ $(,)?) => {$(
            for row in ir.$field.view() {
                builder.ir.$field.push(*row, &mut op).unwrap();
            }
        )+};
    }
    rows!(
        nodes,
        flows,
        packets,
        extra_authorities,
        details,
        objects,
        payloads,
        definitions,
        references,
        proofs,
        primitives,
        stages,
        attributes,
        sources,
        alignments,
    );
    ir.text
        .for_each_segment(|text| builder.ir.text.append(text, &mut op).unwrap());
    Migrated {
        ir: builder.ir.snapshot(&mut op).unwrap(),
        source,
        budget,
    }
}

/// Called by the original Manual::validate, after recording its original result.
/// Its operation, Some statistics, first fault, quotas and assertions stay untouched.
pub(super) fn assert_legacy_semantics(
    ir: &super::manual::ManualIr,
    source: &SourceSnapshot,
    expected: Result<(), CoreError>,
) {
    if !matches!(
        expected,
        Ok(()) | Err(CoreError::InvalidProvenance | CoreError::InvalidBoundary)
    ) {
        // Earlier cancellation/root/quota cases have dedicated facts-route controls below.
        return;
    }
    let budget = SourceBudget::task_fixture();
    let frozen = ir.snapshot(&mut operation(&ir.budget)).unwrap();
    let migrated = transfer(&frozen, source, budget.clone());
    drop(frozen);
    let baseline = budget.usage().auxiliary_capacity;
    let mut op = operation(&budget);
    construction_scope(&mut op, |mut op| {
        let actual = match validate_for_index(
            &migrated.ir,
            &migrated.source,
            identity(&migrated.source),
            &mut op,
        ) {
            Ok(facts) => MapIndex::build_verified(facts).map(drop),
            Err(error) => Err(error),
        };
        assert_eq!(
            actual, expected,
            "original independent IR control in facts route"
        );
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        drop(migrated);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
    drop(op);
    assert_released(&budget);
}

fn copy_fixture(text: &str, budget: SourceBudget) -> Migrated {
    let mut original = Manual::new(text);
    original.copy(
        span(0, text.len() as u64),
        0,
        text.encode_utf16().count() as u64,
    );
    original.seal(text.encode_utf16().count() as u64);
    transfer(
        &original.builder.ir.snapshot(&mut original.op).unwrap(),
        &original.source,
        budget,
    )
}

#[test]
fn final_sparse_buffers_move_once_and_index_does_not_rescan_text() {
    let budget = SourceBudget::task_fixture();
    let c = copy_fixture(&"a".repeat(3073), budget.clone());
    let baseline = budget.usage().auxiliary_capacity;
    let mut op = operation(&budget);
    construction_scope(&mut op, |mut op| {
        let facts = validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op).unwrap();
        let storage = facts.storage_for_test();
        assert_eq!((storage.1, storage.3), (1, 3));
        let (index, main) = MapIndex::build_verified(facts).unwrap();
        assert_eq!(main, flow(0));
        assert_eq!(index.storage_for_test(), storage);
        assert_eq!(index.scalar_steps_for_test(), 0);
        drop(index);
        assert_eq!(budget.usage().auxiliary_capacity, baseline);

        // Same handwritten source/IR under the legacy reader really executes all scalar steps.
        let legacy = transfer(&c.ir, &c.source, SourceBudget::model_fixture());
        let mut old_op = operation(&legacy.budget);
        let old_index = MapIndex::build(
            &legacy.ir,
            &legacy.source,
            identity(&legacy.source),
            &mut old_op,
        )
        .unwrap();
        assert_eq!(old_index.scalar_steps_for_test(), 3073);
        assert_eq!(old_index.storage_for_test().3, 3);
        let legacy_budget = legacy.budget.clone();
        drop(old_index);
        drop(legacy);
        drop(old_op);
        assert_released(&legacy_budget);
        drop(c);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
    drop(op);
    assert_released(&budget);
}

#[test]
fn copy_index_rows_and_prior_rows_share_one_admission_without_move_fee() {
    for limit in [9, 8] {
        let budget = SourceBudget::task_with_limits_for_test(
            SourceLimits::default(),
            ModelLimits {
                index_records: limit,
                ..ModelLimits::default()
            },
        );
        let c = copy_fixture(&"a".repeat(3073), budget.clone());
        let baseline = budget.usage().auxiliary_capacity;
        let mut op = operation(&budget);
        op.admit_model(LimitKind::IndexRecords, 2).unwrap();
        construction_scope(&mut op, |mut op| {
            let facts = validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op).unwrap();
            // 1 target directory + 1 source + 1 positive target + 1 sparse + 3 checkpoints.
            let result = MapIndex::build_verified(facts);
            if limit == 9 {
                let (index, _) = result.unwrap();
                let expected = CoreError::ResourceLimit {
                    kind: LimitKind::IndexRecords,
                    requested: 10,
                    limit: 9,
                };
                assert_eq!(op.admit_model(LimitKind::IndexRecords, 1), Err(expected));
                drop(index);
            } else {
                let error = result.err().unwrap();
                assert_eq!(
                    error,
                    CoreError::ResourceLimit {
                        kind: LimitKind::IndexRecords,
                        requested: 9,
                        limit: 8,
                    }
                );
                assert_eq!(op.first_fault(), Some(error));
            }
            assert_eq!(budget.usage().auxiliary_capacity, baseline);
            drop(c);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
        drop(op);
        assert_released(&budget);
    }
}

#[test]
fn actual_c0_all_seven_index_tables_and_prior_rows_have_exact_admission() {
    use crate::source::ParseInput;

    const PRIOR: u64 = 3;
    // A real C0 emphasis pair supplies Hidden atoms; the paragraph boundary
    // supplies the valid one-byte Synthetic LF; the first body supplies two
    // actual 1024B interior checkpoints. No hand-built long Synthetic is used.
    let body = "a".repeat(2050);
    let markdown = format!("*{body}*\n\nsecond");
    let expected_main = format!("{body}\nsecond");
    let config = ParseConfig::new(Profile::C0);

    let reference = SourceBudget::task_fixture();
    let mut reference_op = operation(&reference);
    reference_op
        .admit_model(LimitKind::IndexRecords, PRIOR)
        .unwrap();
    let reference_source = SourceDocument::new(
        SourceKey::new([7; 16], 1).unwrap(),
        markdown.as_bytes(),
        &mut reference_op,
    )
    .unwrap()
    .snapshot();
    let reference_input = ParseInput::prepare(reference_source.clone(), &mut reference_op).unwrap();
    let (expected, exact) = construction_scope(&mut reference_op, |mut reference_op| {
        let reference_ir =
            crate::parse::build(&reference_input, config, &mut reference_op).unwrap();
        assert_eq!(reference_ir.flows().len(), 1);
        assert!(reference_ir.payloads().is_empty());
        assert_eq!(reference_ir.flows()[0].logical_utf16, 2057);

        // Count the fixed tables independently from the actual final IR. The two
        // sparse interiors are fixed by the authored 2050 ASCII body, not inferred
        // from the facts counter that is being checked.
        let mut expected = [1, 0, 0, 0, 0, reference_ir.atoms_len(), 2];
        let mut main_text = String::new();
        for at in 0..reference_ir.atoms_len() {
            let atom = reference_ir.owned_atom_at(at, &mut reference_op).unwrap();
            expected[if atom.target.start < atom.target.end {
                3
            } else {
                4
            }] += 1;
            let sources = atom.sources.slice(reference_ir.sources()).unwrap();
            for source in sources {
                expected[if source.is_empty() { 2 } else { 1 }] += 1;
            }
            match atom.kind {
                AtomKind::Copy => main_text.push_str(
                    reference_source
                        .text()
                        .get(sources[0].start() as usize..sources[0].end() as usize)
                        .unwrap(),
                ),
                AtomKind::Synthetic | AtomKind::Transform => {
                    let AtomContent::Pool(value) = atom.content else {
                        panic!("text atom must contain actual pool bytes");
                    };
                    let text = reference_ir.text(value).unwrap();
                    if atom.kind == AtomKind::Synthetic {
                        assert_eq!(text, "\n");
                    }
                    main_text.push_str(text);
                }
                AtomKind::Hidden => {}
                AtomKind::Object => panic!("the authored C0 input has no object"),
            }
        }
        assert_eq!(main_text, expected_main);
        assert!(expected.iter().all(|count| *count > 0));
        let total = expected.iter().map(|count| *count as u64).sum::<u64>();
        let exact = PRIOR + total;
        let reference_facts = validate_for_index(
            &reference_ir,
            &reference_source,
            identity(&reference_source),
            &mut reference_op,
        )
        .unwrap();
        assert_eq!(reference_facts.counts_for_test(), expected);
        eprintln!("actual C0 seven-table counts={expected:?}, prior={PRIOR}, exact_max={exact}");
        let (reference_index, main) = MapIndex::build_verified(reference_facts).unwrap();
        assert_eq!(main, flow(0));
        drop(reference_index);
        drop(reference_ir);
        drop(reference_input);
        drop(reference_source);
        reference_op.finish().unwrap();
        (expected, exact)
    });
    drop(reference_op);
    assert_released(&reference);

    for limit in [exact, exact - 1] {
        let budget = SourceBudget::task_with_limits_for_test(
            SourceLimits::default(),
            ModelLimits {
                index_records: limit,
                ..ModelLimits::default()
            },
        );
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        op.set_clock_samples_for_test(&[499_999_999]);
        op.admit_model(LimitKind::IndexRecords, PRIOR).unwrap();
        let source = SourceDocument::new(
            SourceKey::new([7; 16], 1).unwrap(),
            markdown.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let input = ParseInput::prepare(source.clone(), &mut op).unwrap();
        construction_scope(&mut op, |mut op| {
            let ir = crate::parse::build(&input, config, &mut op).unwrap();
            let baseline = budget.usage();
            let facts = validate_for_index(&ir, &source, identity(&source), &mut op).unwrap();
            assert_eq!(facts.counts_for_test(), expected);
            let storage = facts.storage_for_test();
            let result = MapIndex::build_verified(facts);
            let error = if limit == exact {
                let (index, main) = result.unwrap();
                assert_eq!(main, flow(0));
                assert_eq!(index.storage_for_test(), storage);
                assert_eq!(index.scalar_steps_for_test(), 0);
                // Exact success detects duplicate seed admission; this next row
                // detects omitted admission in any of the seven nonempty tables.
                let error = op.admit_model(LimitKind::IndexRecords, 1).unwrap_err();
                drop(index);
                error
            } else {
                result.err().unwrap()
            };
            assert_eq!(
                error,
                CoreError::ResourceLimit {
                    kind: LimitKind::IndexRecords,
                    requested: exact + u64::from(limit == exact),
                    limit,
                }
            );
            assert_eq!(op.first_fault(), Some(error));
            cancel.cancel();
            assert_eq!(op.check(), Err(error));
            let after = budget.usage();
            assert_eq!(after.auxiliary_capacity, baseline.auxiliary_capacity);
            assert_eq!(after.source_capacity, baseline.source_capacity);
            assert_eq!(after.source_owners, baseline.source_owners);
            drop(ir);
            drop(input);
            drop(source);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
        drop(op);
        assert_released(&budget);
    }
}

#[test]
fn validator_checkpoint_quota_fails_before_delivery_and_releases_seed() {
    let budget = SourceBudget::task_with_limits_for_test(
        SourceLimits::default(),
        ModelLimits {
            index_records: 3,
            ..ModelLimits::default()
        },
    );
    let c = copy_fixture(&"a".repeat(3073), budget.clone());
    let baseline = budget.usage().auxiliary_capacity;
    let mut op = operation(&budget);
    construction_scope(&mut op, |mut op| {
        let error = validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op)
            .err()
            .unwrap();
        assert_eq!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::IndexRecords,
                requested: 4,
                limit: 3,
            }
        );
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        assert_eq!(op.first_fault(), Some(error));
        drop(c);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
    drop(op);
    assert_released(&budget);
}

#[test]
fn completed_facts_keep_original_operation_at_index_cancel_and_deadline() {
    for deadline in [false, true] {
        let budget = SourceBudget::task_fixture();
        let c = copy_fixture(&"a".repeat(3073), budget.clone());
        let baseline = budget.usage().auxiliary_capacity;
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        op.set_clock_samples_for_test(&[499_999_999]);
        construction_scope(&mut op, |mut op| {
            let mut facts =
                validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op).unwrap();
            assert!(budget.usage().auxiliary_capacity > baseline);
            let expected = if deadline {
                facts.set_clock_samples_for_test(&[500_000_000]);
                CoreError::DeadlineExceeded {
                    elapsed_ns: 500_000_000,
                    limit_ns: 500_000_000,
                }
            } else {
                cancel.cancel();
                CoreError::Cancelled
            };
            assert_eq!(MapIndex::build_verified(facts).err(), Some(expected));
            assert_eq!(op.first_fault(), Some(expected));
            assert_eq!(budget.usage().auxiliary_capacity, baseline);
            drop(c);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
        drop(op);
        assert_released(&budget);
    }
}

#[test]
fn invalid_late_proof_discards_already_generated_checkpoints() {
    let mut original = Manual::new(&"a".repeat(3073));
    original.copy(span(0, 3073), 0, 3073);
    original.seal(3073);
    let empty = original.builder.text("", &mut original.op).unwrap();
    original
        .builder
        .proof(
            ProofRow {
                atom: AtomId::from_index(0).unwrap(),
                primitives: rows(0, 0),
                stages: rows(0, 0),
                value: empty,
            },
            &mut original.op,
        )
        .unwrap();
    let budget = SourceBudget::task_fixture();
    let c = transfer(
        &original.builder.ir.snapshot(&mut original.op).unwrap(),
        &original.source,
        budget.clone(),
    );
    let baseline = budget.usage().auxiliary_capacity;
    let mut op = operation(&budget);
    construction_scope(&mut op, |mut op| {
        assert_eq!(
            validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op).err(),
            Some(CoreError::InvalidProvenance)
        );
        assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        drop(c);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
    drop(op);
    assert_released(&budget);
}

#[test]
fn long_unproved_transform_does_not_turn_width_facts_into_authority() {
    let text = "a".repeat(3073);
    let mut original = Manual::new(&text);
    original.transform(
        span(0, 3073),
        (0, 3073),
        &text,
        PrimitiveKind::Copy,
        None,
        ClaimKind::Code,
    );
    original.seal(3073);
    let budget = SourceBudget::task_fixture();
    let c = transfer(
        &original.builder.ir.snapshot(&mut original.op).unwrap(),
        &original.source,
        budget.clone(),
    );
    let baseline = budget.usage().auxiliary_capacity;
    let mut op = operation(&budget);
    construction_scope(&mut op, |mut op| {
        assert_eq!(
            validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op).err(),
            Some(CoreError::InvalidProvenance)
        );
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        drop(c);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
    drop(op);
    assert_released(&budget);
}

#[test]
fn nearest_window_node_boundaries_preserve_astral_and_cjk_rejections() {
    for (position, valid) in [(3073, true), (3074, false), (3075, true), (3076, true)] {
        let text = format!("{}😀中z{}", "a".repeat(3073), "b".repeat(2049));
        let length = text.encode_utf16().count() as u64;
        let mut original = Manual::new(&text);
        original.copy(span(0, text.len() as u64), 0, length);
        let child = original
            .builder
            .node(
                NodeRow {
                    parent: Some(node(0)),
                    first_child: None,
                    next_sibling: None,
                    flow: flow(0),
                    kind: NodeKind::Text,
                    detail: None,
                    source_envelope: span(3073, 3081),
                    logical: logical(3073, position),
                },
                &mut original.op,
            )
            .unwrap();
        let mut root = original.builder.ir.nodes()[0];
        root.first_child = Some(child);
        original
            .builder
            .ir
            .nodes
            .replace(0, root, &mut original.op)
            .unwrap();
        original.seal(length);
        let budget = SourceBudget::task_fixture();
        let c = transfer(
            &original.builder.ir.snapshot(&mut original.op).unwrap(),
            &original.source,
            budget.clone(),
        );
        let mut op = operation(&budget);
        construction_scope(&mut op, |mut op| {
            let result =
                validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op).map(drop);
            assert_eq!(
                result,
                if valid {
                    Ok(())
                } else {
                    Err(CoreError::InvalidProvenance)
                }
            );
            drop(c);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
        drop(op);
        assert_released(&budget);
    }
}

#[test]
fn raw_index_builder_cannot_bypass_new_validation_and_identity_is_exact() {
    let budget = SourceBudget::task_fixture();
    let c = copy_fixture("😀中", budget.clone());
    let mut op = operation(&budget);
    assert_eq!(
        MapIndex::build(&c.ir, &c.source, identity(&c.source), &mut op).err(),
        Some(CoreError::UnsupportedConfiguration)
    );
    for (key, expected) in [
        (
            SourceKey::new([8; 16], 1).unwrap(),
            CoreError::WrongDocument,
        ),
        (
            SourceKey::new([7; 16], 2).unwrap(),
            CoreError::WrongRevision {
                expected: 2,
                actual: 1,
            },
        ),
    ] {
        let mut op = operation(&budget);
        let id = ModelIdentity::new(key, 1, ParseConfig::new(Profile::C0)).unwrap();
        construction_scope(&mut op, |mut op| {
            assert_eq!(
                validate_for_index(&c.ir, &c.source, id, &mut op).err(),
                Some(expected)
            );
            assert_eq!(op.first_fault(), Some(expected));
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
    let foreign = SourceBudget::task_fixture();
    let mut other = operation(&foreign);
    construction_scope(&mut other, |mut other| {
        assert_eq!(
            validate_for_index(&c.ir, &c.source, identity(&c.source), &mut other).err(),
            Some(CoreError::WrongBudget)
        );
        let _ = other.finish();
    });
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut cancelled = budget.begin(cancel);
    construction_scope(&mut cancelled, |mut cancelled| {
        assert_eq!(
            validate_for_index(&c.ir, &c.source, identity(&c.source), &mut cancelled).err(),
            Some(CoreError::Cancelled)
        );
        let _ = cancelled.finish();
    });
    drop(c);
    drop((op, other, cancelled));
    assert_released(&foreign);
    assert_released(&budget);
}

#[test]
fn checkpoint_old_and_new_allocations_overlap_and_failure_keeps_first_fault() {
    use crate::ir::validation_output::Checkpoint;

    let budget = SourceBudget::task_fixture();
    let c = copy_fixture(&"a".repeat(3073), budget.clone());
    let baseline = budget.usage().auxiliary_capacity;
    // During the third checkpoint, the sparse directory, ownership byte and
    // one source-part temporary coexist with old(2)+new(4) checkpoint slots.
    let fixed = size_of::<Rows<Checkpoint>>() + size_of::<u8>() + size_of::<SourceSpan>();
    let overlap = (fixed + 6 * size_of::<Checkpoint>()) as u64;
    let blocker = budget
        .reserve_auxiliary(budget.limits().auxiliary_capacity - baseline - overlap + 1)
        .unwrap();
    let held = budget.usage().auxiliary_capacity;
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    op.set_clock_samples_for_test(&[499_999_999]);
    construction_scope(&mut op, |mut op| {
        let error = validate_for_index(&c.ir, &c.source, identity(&c.source), &mut op)
            .err()
            .unwrap();
        assert_eq!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::AuxiliaryCapacity,
                requested: budget.limits().auxiliary_capacity + 1,
                limit: budget.limits().auxiliary_capacity,
            }
        );
        assert_eq!(budget.usage().auxiliary_capacity, held);
        cancel.cancel();
        assert_eq!(op.check(), Err(error));
        drop(blocker);
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        drop(c);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
    drop(op);
    assert_released(&budget);
}
