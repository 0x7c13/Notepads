// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Exercise the actual complete builder across the native destruction boundary.

use super::*;
use crate::identity::{ModelIdentity, SourceKey};
use crate::mapping::MapIndex;
use crate::model::AcceptedModel;
use crate::parse::prepared::NativeFault;
use crate::policy::{CancellationToken, SourceBudget, construction_scope};
use crate::source::SourceDocument;
use native::Host;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Clone, Copy, Debug)]
enum TailMode {
    Success,
    TypedThenCancel,
    PriorFaultThenCancel,
    Cancel,
    Unwind,
    AstTypedThenCancel,
    AstPriorFaultThenCancel,
    AstCancel,
    AstUnwind,
}
impl TailMode {
    fn stops_before_flow(self) -> bool {
        matches!(
            self,
            Self::AstTypedThenCancel
                | Self::AstPriorFaultThenCancel
                | Self::AstCancel
                | Self::AstUnwind
        )
    }
    fn unwinds(self) -> bool {
        matches!(self, Self::Unwind | Self::AstUnwind)
    }
}

struct TailState<'budget> {
    budget: &'budget SourceBudget,
    auxiliary_baseline: u64,
    builder_capacity: Cell<Option<u64>>,
    ast_core_capacity: Cell<Option<u64>>,
    ast_native_capacity: Cell<Option<u64>>,
    retired_capacity: Cell<Option<u64>>,
    callback_complete: Cell<bool>,
    provenance_entered: Cell<bool>,
    tail_entered: Cell<bool>,
    cancel: CancellationToken,
    mode: TailMode,
}

/// Forward every operation to the real binding and its original NativeLease.
/// Inject only after the real AST-retirement or final native checkpoint succeeds.
struct TailHost<'state, 'budget, B> {
    binding: B,
    state: &'state TailState<'budget>,
}

impl<'input, 'source, 'work, 'id, 'op> Host
    for TailHost<'_, '_, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
{
    type Error = NativeFault<'input, 'id>;
    type Lease = NativeLease<'id>;

    fn work(&mut self, delta: native::WorkDelta) -> Result<(), Self::Error> {
        self.binding.work(delta)
    }

    fn admit(&mut self, request: native::NativeAdmission) -> Result<(), Self::Error> {
        self.binding.admit(request)
    }

    fn reserve(&mut self, bytes: u64) -> Result<Self::Lease, Self::Error> {
        self.binding.reserve(bytes)
    }

    fn admit_actual(&mut self, lease: &mut Self::Lease, bytes: u64) -> Result<(), Self::Error> {
        self.binding.admit_actual(lease, bytes)
    }

    fn fact(&mut self, fact: native::AcceptedFact<'_>) -> Result<(), Self::Error> {
        self.binding.fact(fact)
    }

    fn reject(&mut self, violation: native::NativeViolation) -> Self::Error {
        self.binding.reject(violation)
    }
}

impl<'input, 'source, 'work, 'id, 'op> n::DirectHost<'input>
    for TailHost<'_, '_, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
{
    type ReadBrand = ConstructionId<'id>;

    fn finish<T>(&mut self, result: Result<T, Self::Error>) -> Result<T, Self::Error> {
        self.binding.finish(result)
    }

    fn checkpoint_now(&mut self) -> Result<(), Self::Error> {
        self.binding.checkpoint_now()?;
        if !self.state.callback_complete.get() {
            // The driver reaches this checkpoint only after the real AST arenas
            // and grammar-only permits have retired. Core state remains live.
            let capacity = self.state.budget.usage().auxiliary_capacity;
            assert!(
                capacity
                    < self.state.ast_native_capacity.get().unwrap()
                        + self.state.ast_core_capacity.get().unwrap()
            );
            assert!(
                self.state
                    .retired_capacity
                    .replace(Some(capacity))
                    .is_none()
            );
            return match self.state.mode {
                TailMode::AstTypedThenCancel => {
                    self.state.cancel.cancel();
                    self.binding.carry(Err(CoreError::InvalidBoundary))
                }
                TailMode::AstPriorFaultThenCancel => {
                    self.binding
                        .reject(native::NativeViolation::InvalidProvenance);
                    self.state.cancel.cancel();
                    self.binding.carry(Err(CoreError::InvalidBoundary))
                }
                TailMode::AstCancel => {
                    self.state.cancel.cancel();
                    self.binding.checkpoint_now()
                }
                TailMode::AstUnwind => panic!("complete graph at actual AST retirement"),
                _ => Ok(()),
            };
        }
        assert!(!self.state.tail_entered.replace(true));
        // Both samples are from this invocation. Compilation only reads native
        // state, and drops Graph/images before the complete builder returns.
        // The remaining capacity is exactly the retained pins plus that builder.
        assert_eq!(
            self.state.budget.usage().auxiliary_capacity,
            self.state.auxiliary_baseline + self.state.builder_capacity.get().unwrap()
        );
        match self.state.mode {
            TailMode::Success => Ok(()),
            TailMode::AstTypedThenCancel
            | TailMode::AstPriorFaultThenCancel
            | TailMode::AstCancel
            | TailMode::AstUnwind => panic!("AST failure reached native tail"),
            TailMode::TypedThenCancel => {
                self.state.cancel.cancel();
                self.binding.carry(Err(CoreError::InvalidBoundary))
            }
            TailMode::PriorFaultThenCancel => {
                self.binding
                    .reject(native::NativeViolation::InvalidProvenance);
                self.state.cancel.cancel();
                self.binding.carry(Err(CoreError::InvalidBoundary))
            }
            TailMode::Cancel => {
                self.state.cancel.cancel();
                self.binding.checkpoint_now()
            }
            TailMode::Unwind => panic!("complete owned builder at actual native tail"),
        }
    }
}

/// The real production compiler, observed at its two actual phase calls.
/// The returned production state owns capture/Graph/builder, never a borrowed
/// AST, source witness, or Work. Observations stay at the real native boundaries.
struct TailCompiler<'id>(CoreCompiler<'id>);

impl<'input, 'source, 'work, 'id, 'op, 'state, 'budget>
    n::DirectCompiler<
        'input,
        TailHost<'state, 'budget, PreparedBinding<'input, 'source, 'work, 'id, 'op>>,
    > for TailCompiler<'id>
{
    type Output = b::DirectBuilder<'id>;
    type State<'view> = CoreState<'view, 'input, 'id>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut TailHost<'state, 'budget, PreparedBinding<'input, 'source, 'work, 'id, 'op>>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        let before = host.state.budget.usage().auxiliary_capacity;
        assert!(before > host.state.auxiliary_baseline);
        host.state.ast_native_capacity.set(Some(before));
        let state = self.0.ast(reader, &mut host.binding)?;
        let capacity = host
            .state
            .budget
            .usage()
            .auxiliary_capacity
            .checked_sub(before)
            .unwrap();
        assert!(capacity > 0);
        host.state.ast_core_capacity.set(Some(capacity));
        Ok(state)
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
        state: Self::State<'view>,
        host: &mut TailHost<'state, 'budget, PreparedBinding<'input, 'source, 'work, 'id, 'op>>,
    ) -> Result<Self::Output, NativeFault<'input, 'id>> {
        let before = host.state.budget.usage().auxiliary_capacity;
        assert_eq!(host.state.retired_capacity.get(), Some(before));
        assert!(!host.state.provenance_entered.replace(true));
        let builder = self.0.provenance(reader, state, &mut host.binding)?;
        // Original retained native storage does not mutate during either core
        // phase. Subtract that same-invocation live native amount, not a peak.
        let native = before
            .checked_sub(host.state.ast_core_capacity.get().unwrap())
            .unwrap();
        let capacity = host
            .state
            .budget
            .usage()
            .auxiliary_capacity
            .checked_sub(native)
            .unwrap();
        assert!(capacity > 0);
        host.state.builder_capacity.set(Some(capacity));
        host.state.callback_complete.set(true);
        Ok(builder)
    }
}

#[test]
fn complete_builder_survives_only_successful_native_tail_and_freezes_after_cleanup() {
    let text = "# heading\r\n\r\n*x &amp; y* [use][a] ![alt](img)\r\n\r\n[a]: /target \"title\"\r\n\r\n```lang\r\ncode\r\n```\r\n";
    check_native_tail_boundaries(text, Profile::C0);
}

#[test]
fn github_builder_retires_autolinks_and_other_extensions_at_real_native_boundaries() {
    let text = "prefix\r\n|a|\r\n|---|\r\n\r\n- [x] ~~label~~ ![www.alt.example](img)\r\n\r\n> www.example.org https://example.net/a(b).\r\n> name&#64;example.com [use][a]\r\n\r\n[a]: /target \"title\"\r\n";
    check_native_tail_boundaries(text, Profile::G3);
}

fn check_native_tail_boundaries(text: &str, profile: Profile) {
    for mode in [
        TailMode::Success,
        TailMode::TypedThenCancel,
        TailMode::PriorFaultThenCancel,
        TailMode::Cancel,
        TailMode::Unwind,
        TailMode::AstTypedThenCancel,
        TailMode::AstPriorFaultThenCancel,
        TailMode::AstCancel,
        TailMode::AstUnwind,
    ] {
        let budget = SourceBudget::task_fixture();
        let config = ParseConfig::new(profile);
        let mut preparation = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([87; 16], 1).unwrap(),
            text.as_bytes(),
            &mut preparation,
        )
        .unwrap()
        .snapshot();
        let input = ParseInput::prepare(source.clone(), &mut preparation).unwrap();
        let mut previous = budget.begin(CancellationToken::new());
        let old_model = AcceptedModel::parse(source.clone(), config, 1, &mut previous).unwrap();
        let old_identity = old_model.identity();
        let old_atoms = old_model.ir().atoms_len();
        let old_title = old_model
            .ir()
            .payloads()
            .iter()
            .find(|row| row.role == crate::ir::PayloadRole::Title)
            .unwrap()
            .value;
        assert_eq!(old_model.ir().text(old_title).unwrap(), "title");
        let baseline = budget.usage();
        let cancel = CancellationToken::new();
        let state = TailState {
            budget: &budget,
            auxiliary_baseline: baseline.auxiliary_capacity,
            builder_capacity: Cell::new(None),
            ast_core_capacity: Cell::new(None),
            ast_native_capacity: Cell::new(None),
            retired_capacity: Cell::new(None),
            callback_complete: Cell::new(false),
            provenance_entered: Cell::new(false),
            tail_entered: Cell::new(false),
            cancel: cancel.clone(),
            mode,
        };
        let mut operation = budget.begin(cancel);
        construction_scope(&mut operation, |mut work| {
            let result = catch_unwind(AssertUnwindSafe(|| {
                with_prepared(&input, &mut work, |native_input, binding| {
                    let mut host = TailHost {
                        binding,
                        state: &state,
                    };
                    let (options, policy) = native_configuration(config);
                    let result = n::compile_with_inline_policy(
                        native_input,
                        &options,
                        &mut host,
                        TailCompiler(CoreCompiler(std::marker::PhantomData, config)),
                        policy,
                    );
                    host.binding.resolve(result)
                })
            }));
            assert!(state.retired_capacity.get().is_some(), "{mode:?}");
            assert_eq!(
                state.provenance_entered.get(),
                !mode.stops_before_flow(),
                "{mode:?}"
            );
            assert_eq!(
                state.callback_complete.get(),
                !mode.stops_before_flow(),
                "{mode:?}"
            );
            assert_eq!(
                state.tail_entered.get(),
                !mode.stops_before_flow(),
                "{mode:?}"
            );
            if matches!(mode, TailMode::Success) {
                let builder = result.unwrap().unwrap();
                assert_eq!(work.first_fault(), None);
                // This is the first freeze, after real native cleanup/tail and
                // the whole PreparedFactory have both completed successfully.
                work.checkpoint_now().unwrap();
                let ir = builder.finish(&mut work).unwrap();
                let identity = ModelIdentity::new(source.key(), 2, config).unwrap();
                let facts =
                    ir::validate::validate_for_index_owned(&ir, &source, identity, &mut work)
                        .unwrap();
                let (index, _) = MapIndex::build_verified(facts).unwrap();
                drop((index, ir));
                work.finish().unwrap();
            } else {
                let expected = match mode {
                    TailMode::TypedThenCancel | TailMode::AstTypedThenCancel => {
                        CoreError::InvalidBoundary
                    }
                    TailMode::PriorFaultThenCancel | TailMode::AstPriorFaultThenCancel => {
                        CoreError::InvalidProvenance
                    }
                    TailMode::Cancel | TailMode::AstCancel => CoreError::Cancelled,
                    TailMode::Unwind | TailMode::AstUnwind => CoreError::InternalInvariant,
                    TailMode::Success => unreachable!(),
                };
                match result {
                    Ok(Err(error)) => {
                        assert!(!mode.unwinds());
                        assert_eq!(error, expected);
                    }
                    Err(payload) => {
                        assert!(mode.unwinds());
                        assert_eq!(
                            payload.downcast_ref::<&str>().copied(),
                            Some(if mode.stops_before_flow() {
                                "complete graph at actual AST retirement"
                            } else {
                                "complete owned builder at actual native tail"
                            })
                        );
                    }
                    Ok(Ok(_)) => panic!("failed tail returned its private builder: {mode:?}"),
                }
                assert_eq!(work.first_fault(), Some(expected));
                assert_eq!(work.finish(), Err(expected));
            }
        });
        let after = budget.usage();
        assert_eq!(after.auxiliary_capacity, baseline.auxiliary_capacity);
        assert_eq!(after.source_capacity, baseline.source_capacity);
        assert_eq!(after.source_owners, baseline.source_owners);
        assert_eq!(old_model.identity(), old_identity);
        assert_eq!(old_model.ir().atoms_len(), old_atoms);
        // Real final text pinned by the previously accepted model survives both
        // capture-state destruction and post-provenance native-tail failures.
        assert_eq!(old_model.ir().text(old_title).unwrap(), "title");
        assert_eq!(old_model.source().text(), text);
        drop((old_model, input, source));
        let final_usage = budget.usage();
        assert_eq!(final_usage.auxiliary_capacity, 0);
        assert_eq!(final_usage.source_capacity, 0);
        assert_eq!(final_usage.source_owners, 0);
    }
}
