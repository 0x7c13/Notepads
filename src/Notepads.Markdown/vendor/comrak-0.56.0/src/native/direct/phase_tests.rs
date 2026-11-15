// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

const INPUT: &str = "one &amp; two\n\n[r]: /u \"&quot;\"\n";

#[derive(Clone, Copy)]
enum Mode {
    Success,
    TypedError,
    IgnoredError,
    Panic,
}

struct StateDrop(Rc<Cell<bool>>);
impl Drop for StateDrop {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

struct Carry<'view, 'input> {
    lineage: LineageToken<'view, 'input>,
    mapping: MappingToken<'view, 'input>,
    claims: usize,
    _drop: StateDrop,
}

struct Compiler<'input> {
    input: &'input str,
    entered: Rc<Cell<bool>>,
    dropped: Rc<Cell<bool>>,
    before: usize,
    mode: Mode,
}

impl<'input> DirectCompiler<'input, TestHost<'input>> for Compiler<'input> {
    type Output = ();
    type State<'view> = Carry<'view, 'input>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: AstReader<'store, 'view, 'arena, 'input, Invariant<'input>, Lease>,
        host: &mut TestHost<'input>,
    ) -> Result<Self::State<'view>, Error> {
        let state = reader.with_reads(host, |access| {
            let root = access.root()?;
            let paragraph = access.node(root)?.first_child.unwrap();
            let node = access.node(paragraph)?.first_child.unwrap();
            let lineage = access.lineage(node)?;
            let mapping = access.binding(lineage)?.value;
            assert_eq!(map_text(access, mapping, self.input)?, "one & two");
            assert_eq!(access.definition_count(), 1);
            Ok(Carry {
                lineage,
                mapping,
                claims: access.claim_count(),
                _drop: StateDrop(Rc::clone(&self.dropped)),
            })
        })?;
        self.before = host.live.get();
        assert!(self.before > 0);
        Ok(state)
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: ProvenanceReader<'store, 'view, 'input, Invariant<'input>, Lease>,
        state: Self::State<'view>,
        host: &mut TestHost<'input>,
    ) -> Result<(), Error> {
        self.entered.set(true);
        assert_eq!(host.checkpoints, 1);
        // Same invocation: arena/index owners have gone, retained provenance is live.
        assert!(host.live.get() > 0);
        assert!(host.live.get() < self.before);
        reader.with_reads(host, |access| {
            let binding = access.binding(state.lineage)?;
            assert_eq!(
                access.mapping_len(binding.value)?,
                access.mapping_len(state.mapping)?
            );
            assert_eq!(map_text(access, state.mapping, self.input)?, "one & two");
            assert_eq!(access.claim_count(), state.claims);
            let mut has_real_decode = false;
            for index in 0..access.mapping_len(state.mapping)? {
                if let Some(proof) = access.mapping_atom(state.mapping, index)?.proof {
                    let info = access.proof(proof)?;
                    has_real_decode |= matches!(info.kind, TransformKind::Decode(_));
                    assert!(access.mapping_len(info.input)? > 0);
                }
            }
            assert!(has_real_decode);
            Ok(())
        })?;
        match self.mode {
            Mode::Success => Ok(()),
            Mode::TypedError => Err(Error::Raw),
            Mode::IgnoredError => {
                assert_eq!(
                    reader.with_reads(host, |a| a.with_host(|_| Err::<(), _>(Error::Raw))),
                    Err(Error::Raw)
                );
                assert_eq!(
                    reader.with_reads(host, |a| a.binding(state.lineage).map(|_| ())),
                    Err(Error::Raw)
                );
                Ok(())
            }
            Mode::Panic => {
                host.work(WorkDelta::Events(1))?;
                panic!("after real provenance access and successful same-host work");
            }
        }
    }
}

#[test]
fn original_proof_source_and_lineage_survive_actual_arena_exit_without_ast_state() {
    for mode in [Mode::Success, Mode::TypedError, Mode::IgnoredError] {
        with_input(INPUT, |input| {
            let entered = Rc::new(Cell::new(false));
            let dropped = Rc::new(Cell::new(false));
            let mut host = TestHost::new(&input);
            let compiler = Compiler {
                input: input.text,
                entered: Rc::clone(&entered),
                dropped: Rc::clone(&dropped),
                before: 0,
                mode,
            };
            let result = compile(input, &crate::Options::default(), &mut host, compiler);
            assert_eq!(
                result,
                if matches!(mode, Mode::Success) {
                    Ok(())
                } else {
                    Err(Error::Raw)
                }
            );
            assert!(entered.get());
            assert!(dropped.get());
            assert_eq!(host.live.get(), 0);
            assert_eq!(
                host.checkpoints,
                if matches!(mode, Mode::Success) { 2 } else { 1 }
            );
        });
    }
}

#[test]
fn cancellation_at_ast_exit_discards_state_without_entering_provenance() {
    with_input(INPUT, |input| {
        let entered = Rc::new(Cell::new(false));
        let dropped = Rc::new(Cell::new(false));
        let mut host = TestHost::new(&input);
        host.cancel_ast_exit = true;
        let compiler = Compiler {
            input: input.text,
            entered: Rc::clone(&entered),
            dropped: Rc::clone(&dropped),
            before: 0,
            mode: Mode::Success,
        };
        assert_eq!(
            compile(input, &crate::Options::default(), &mut host, compiler),
            Err(Error::Cancelled)
        );
        assert!(!entered.get());
        assert!(dropped.get());
        assert_eq!(host.live.get(), 0);
        assert_eq!(host.checkpoints, 1);
    });
}

#[test]
fn caught_provenance_unwind_after_successful_work_permanently_faults_original_host() {
    with_input(INPUT, |input| {
        let entered = Rc::new(Cell::new(false));
        let dropped = Rc::new(Cell::new(false));
        let mut host = TestHost::new(&input);
        let compiler = Compiler {
            input: input.text,
            entered: Rc::clone(&entered),
            dropped: Rc::clone(&dropped),
            before: 0,
            mode: Mode::Panic,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compile(input, &crate::Options::default(), &mut host, compiler)
        }));
        assert!(result.is_err());
        assert!(entered.get());
        assert!(dropped.get());
        assert_eq!(host.live.get(), 0);
        assert_eq!(host.fault, Some(Error::Invalid));
        assert_eq!(host.work(WorkDelta::Events(1)), Err(Error::Invalid));
    });
}
