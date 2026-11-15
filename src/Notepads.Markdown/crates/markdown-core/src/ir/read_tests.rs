// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::SourceKey;
use crate::model::AcceptedModel;
use crate::policy::CancellationToken;
use crate::source::SourceDocument;

fn operation(budget: &SourceBudget) -> Operation {
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[499_999_999]);
    op
}

#[test]
fn actual_ir_reads_keep_values_fees_and_root_checks() {
    for budget in [SourceBudget::model_fixture(), SourceBudget::task_fixture()] {
        let mut op = operation(&budget);
        let source = SourceDocument::new(
            SourceKey::new([0x40; 16], 1).unwrap(),
            b"# Heading\n\n[a][r] ![a *b*](u) &amp;\n\n[r]: /u \"title\"\n",
            &mut op,
        )
        .unwrap();
        let model =
            AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::C0), 1, &mut op)
                .unwrap();
        for at in 0..model.ir().atoms_len() {
            let mut wide = operation(&budget);
            let expected = model.ir().atom_at(at, &mut wide).unwrap();
            let mut narrow = operation(&budget);
            assert_eq!(model.ir().read_atom_at(at, &mut narrow).unwrap(), expected);
            assert_eq!(wide.stats(), narrow.stats());
            let mut wide = operation(&budget);
            let expected_authority = model.ir().authority(expected.claim, &mut wide).unwrap();
            let mut narrow = operation(&budget);
            assert_eq!(
                model
                    .ir()
                    .read_authority(expected.claim, &mut narrow)
                    .unwrap(),
                expected_authority
            );
            assert_eq!(wide.stats(), narrow.stats());
            let mut wide = operation(&budget);
            model.ir().atom_at(at, &mut wide).unwrap();
            let mut narrow = operation(&budget);
            assert_eq!(
                model.ir().atom_target_end(at, &mut narrow).unwrap(),
                expected.target.end
            );
            assert_eq!(wide.stats(), narrow.stats());
        }
        let foreign = SourceBudget::task_fixture();
        let mut bad = operation(&foreign);
        assert_eq!(
            model.ir().read_atom_at(usize::MAX, &mut bad),
            Err(CoreError::WrongBudget)
        );
        let mut bad = operation(&budget);
        assert_eq!(
            model.ir().read_atom_at(usize::MAX, &mut bad),
            Err(CoreError::InvalidProvenance)
        );
        let token = CancellationToken::new();
        let mut bad = budget.begin(token.clone());
        token.cancel();
        assert_eq!(
            model.ir().atom_target_end(usize::MAX, &mut bad),
            Err(CoreError::Cancelled)
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

struct TaskAtomRead<'a> {
    ir: &'a CompactIr,
    at: usize,
}

impl<'id> crate::memory::BuildKernel<'id> for TaskAtomRead<'_> {
    type Output = AtomRow;
    fn run<'scope>(
        &'scope mut self,
        window: &mut crate::memory::BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<crate::memory::BuildStep<AtomRow>, CoreError> {
        let view = match self.ir.packets.task_read(window)? {
            crate::memory::BuildStep::Ready(view) => view,
            crate::memory::BuildStep::Full => return Ok(crate::memory::BuildStep::Full),
            crate::memory::BuildStep::NeedCapacity(_) => unreachable!(),
        };
        view.record(self.at, window)
    }
}

#[test]
fn task_atom_projection_preserves_owned_shared_rows_and_readonly_failure() {
    use crate::memory::BuildStep;
    let budget = SourceBudget::task_fixture();
    let mut op = operation(&budget);
    let source = SourceDocument::new(
        SourceKey::new([0x41; 16], 1).unwrap(),
        b"# Heading\n\n[a][r] ![a *b*](u) &amp;\n\n[r]: /u \"title\"\n",
        &mut op,
    )
    .unwrap();
    let input = crate::source::ParseInput::prepare(source.snapshot(), &mut op).unwrap();
    let ir = crate::policy::construction_scope(&mut op, |mut work| {
        let ir = crate::parse::build(&input, ParseConfig::new(Profile::C0), &mut work).unwrap();
        work.finish().unwrap();
        ir
    });
    let mut owned = false;
    let mut shared = false;
    for at in 0..ir.atoms_len() {
        match ir.packets.view()[at].authority {
            AuthorityBinding::Owned(_) => owned = true,
            AuthorityBinding::Shared(_) => shared = true,
        }
        let mut wide = operation(&budget);
        let expected = ir.atom_at(at, &mut wide).unwrap();
        let mut narrow = operation(&budget);
        assert_eq!(
            run_task(&mut TaskAtomRead { ir: &ir, at }, &mut narrow),
            Ok(BuildStep::Ready(expected))
        );
        assert_eq!(narrow.stats(), None);
    }
    assert!(
        owned && shared,
        "both physical authority alternatives must be exercised"
    );
    let mut invalid = operation(&budget);
    assert_eq!(
        run_task(
            &mut TaskAtomRead {
                ir: &ir,
                at: usize::MAX
            },
            &mut invalid
        ),
        Err(CoreError::InvalidProvenance)
    );
    let foreign = SourceBudget::task_fixture();
    let mut wrong = operation(&foreign);
    assert_eq!(
        run_task(&mut TaskAtomRead { ir: &ir, at: 0 }, &mut wrong),
        Err(CoreError::WrongBudget)
    );
    let expected = ir.atom_at(0, &mut operation(&budget)).unwrap();
    assert_eq!(
        run_task(
            &mut TaskAtomRead { ir: &ir, at: 0 },
            &mut operation(&budget)
        ),
        Ok(BuildStep::Ready(expected))
    );
    drop((ir, input, source));
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

fn run_task(
    kernel: &mut TaskAtomRead<'_>,
    op: &mut Operation,
) -> Result<crate::memory::BuildStep<AtomRow>, CoreError> {
    crate::policy::construction_scope(op, |mut work| {
        work.ensure_budget(&kernel.ir.budget)?;
        let value = crate::memory::run_window(kernel, &mut work)?;
        work.finish()?;
        Ok(value)
    })
}
