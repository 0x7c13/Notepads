// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent search and real native-host controls for the R8 helpers.

use super::*;
use crate::identity::SourceKey;
use crate::parse::prepared::NativeFault;
use crate::policy::{CancellationToken, Operation, SourceBudget, construction_scope};
use crate::source::SourceDocument;

fn stable_operation(budget: &SourceBudget, cancel: CancellationToken) -> Operation {
    let mut operation = budget.begin(cancel);
    // A semantic-test clock, never evidence about elapsed performance.
    operation.set_clock_samples_for_test(&[499_999_999]);
    operation
}

// Deliberately use a linear search rather than either binary search or the
// predecessor formula. Starts are semantic source order, not backing-row order.
fn linear_original(starts: &[u64], pi: usize, selected: usize, end: u64) -> bool {
    let boundary = (pi..starts.len())
        .find(|&rank| starts[rank] >= end)
        .unwrap_or(starts.len());
    selected <= boundary
}

fn posting_rows<'id>(
    starts: &[u64],
    work: &mut OwnedWork<'id, '_>,
) -> Result<(BuildRows<'id, Posting>, BuildGrowingVec<'id, u32>), CoreError> {
    let mut postings = BuildRows::new(starts.len() as u64, LimitKind::SourcePieces, work)?;
    let mut order = BuildGrowingVec::new(starts.len() as u64, LimitKind::SourcePieces, work)?;
    for &start in starts.iter().rev() {
        postings.push(
            Posting {
                source: SourceSpan::new(start, start + 1)?,
                emission: 0,
            },
            work,
        )?;
    }
    for rank in 0..starts.len() {
        order.push((starts.len() - rank - 1) as u32, work)?;
    }
    Ok((postings, order))
}

fn compare_search(starts: &[u64], exhaustive: bool) {
    let budget = SourceBudget::task_fixture();
    let mut operation = stable_operation(&budget, CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        let (postings, order) = posting_rows(starts, &mut work)?;
        if !exhaustive {
            assert!(
                postings.page_count() > 1,
                "must exercise real page crossings"
            );
        }
        let positions: Vec<_> = if exhaustive {
            (0..=starts.len()).collect()
        } else {
            vec![0, 1, 511, 682, 683, 1024, starts.len()]
        };
        let selected: Vec<_> = if exhaustive {
            (0..=starts.len() + 1)
                .chain(std::iter::once(usize::MAX))
                .collect()
        } else {
            vec![0, 1, 511, 682, 683, 1024, 1025, starts.len(), usize::MAX]
        };
        let ends = if exhaustive {
            vec![0, 1, 2, 3, 4, u64::MAX]
        } else {
            vec![0, 1, 170, 227, 341, 367, u64::MAX]
        };
        let before = budget.usage();
        for pi in positions {
            for &selected in &selected {
                for &end in &ends {
                    assert_eq!(
                        precedes_envelope(&postings, &order, pi, selected, end, &mut work)?,
                        linear_original(starts, pi, selected, end),
                        "starts={starts:?}, pi={pi}, selected={selected}, end={end}"
                    );
                }
            }
        }
        let after = budget.usage();
        assert_eq!(after.auxiliary_capacity, before.auxiliary_capacity);
        assert_eq!(
            after.auxiliary_capacity_peak,
            before.auxiliary_capacity_peak
        );
        drop((postings, order));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        work.finish()
    })
    .unwrap();
}

#[test]
fn predecessor_matches_linear_search_for_duplicate_starts_and_paged_rows() {
    fn sorted_cases(values: &mut Vec<u64>, len: usize, minimum: u64) {
        if values.len() == len {
            compare_search(values, true);
            return;
        }
        for value in minimum..=3 {
            values.push(value);
            sorted_cases(values, len, value);
            values.pop();
        }
    }
    for len in 0..=6 {
        sorted_cases(&mut Vec::new(), len, 0);
    }
    // A Posting page holds at most 1024 rows. Reverse backing indices ensure
    // that the checked order lookup, rather than incidental row order, matters.
    let starts: Vec<_> = (0..1100).map(|rank| rank / 3).collect();
    compare_search(&starts, false);
}

#[test]
fn predecessor_observes_cancellation_on_read_and_both_no_read_exits() {
    for (pi, selected) in [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1)] {
        let budget = SourceBudget::task_fixture();
        let cancel = CancellationToken::new();
        let mut operation = stable_operation(&budget, cancel.clone());
        construction_scope(&mut operation, |mut work| {
            let (postings, order) = posting_rows(&[1], &mut work).unwrap();
            cancel.cancel();
            assert_eq!(
                precedes_envelope(&postings, &order, pi, selected, 2, &mut work),
                Err(CoreError::Cancelled)
            );
            assert_eq!(work.first_fault(), Some(CoreError::Cancelled));
            drop((postings, order));
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(CoreError::Cancelled));
        });
    }
}

#[test]
fn predecessor_keeps_a_real_row_read_fault_ahead_of_later_cancel() {
    let budget = SourceBudget::task_fixture();
    let cancel = CancellationToken::new();
    let mut operation = stable_operation(&budget, cancel.clone());
    construction_scope(&mut operation, |mut work| {
        let postings = BuildRows::new(1, LimitKind::SourcePieces, &mut work).unwrap();
        let mut order = BuildGrowingVec::new(1, LimitKind::SourcePieces, &mut work).unwrap();
        order.push(u32::MAX, &mut work).unwrap();
        // The original predicate needs no posting access on these two exits.
        assert_eq!(
            precedes_envelope(&postings, &order, 0, 0, 2, &mut work),
            Ok(true)
        );
        assert_eq!(
            precedes_envelope(&postings, &order, 0, 2, 2, &mut work),
            Ok(false)
        );
        let error = precedes_envelope(&postings, &order, 0, 1, 2, &mut work).unwrap_err();
        assert_eq!(work.first_fault(), Some(error));
        cancel.cancel();
        for selected in [0, 1, 2, usize::MAX] {
            assert_eq!(
                precedes_envelope(&postings, &order, 0, selected, 2, &mut work),
                Err(error)
            );
        }
        drop((postings, order));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(error));
    });
}

#[derive(Default, Debug)]
struct HostObservations {
    claims: usize,
    full_p: usize,
    cacheable_r: usize,
    uncached_r: usize,
    shorter_r_snapshots: usize,
    first_r_cache_allocations: usize,
    distinct_r_buffer_pairs: usize,
    same_owner_distinct_r_buffer_pairs: usize,
    different_owner_r_buffer_pairs: usize,
    same_geometry_r_pairs: usize,
    distinct_hidden_authorities_sharing_domains: usize,
    stopped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    None,
    CancelMiss,
    CancelHit,
    FirstFaultMiss,
    FirstFaultHit,
}

impl Stop {
    fn after_hit(self) -> bool {
        matches!(self, Self::CancelHit | Self::FirstFaultHit)
    }

    fn first_fault(self) -> bool {
        matches!(self, Self::FirstFaultMiss | Self::FirstFaultHit)
    }
}

struct HostCompiler<'case, 'id> {
    observed: &'case mut HostObservations,
    stop: Stop,
    cancel: &'case CancellationToken,
    brand: std::marker::PhantomData<fn(&'id ()) -> &'id ()>,
}

impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for HostCompiler<'_, 'id>
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
            inspect_hosts(
                &native,
                &mut graph,
                &mut builder,
                self.observed,
                (self.stop, self.cancel),
                work,
            )?;
            let images = metadata::emit(&native, &mut graph, &mut builder, captured, work)?;
            target::flows(&native, &mut graph, &mut builder, &images, work)?;
            builder.complete_nodes(work)?;
            drop((images, graph));
            Ok(builder)
        })
    }
}

fn source_sequence<'id>(
    builder: &b::DirectBuilder<'id>,
    block: b::SourceBlockRef,
    work: &mut OwnedWork<'id, '_>,
) -> Result<Vec<SourceSpan>, CoreError> {
    let mut sequence = Vec::new();
    for at in 0..builder.source_len(block, work)? {
        sequence.push(builder.source_at(block, at, work)?);
    }
    Ok(sequence)
}

fn stop_host<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    host: n::HostDomain<'view, 'input>,
    control: (Stop, &CancellationToken),
    work: &mut OwnedWork<'id, '_>,
) -> CoreError {
    let (stop, cancel) = control;
    let expected = if stop.first_fault() {
        work.fail(CoreError::InvalidBoundary)
    } else {
        CoreError::Cancelled
    };
    cancel.cancel();
    assert_eq!(
        host_domains(native, graph, builder, host, work),
        Err(expected)
    );
    assert_eq!(work.first_fault(), Some(expected));
    assert_eq!(work.check(), Err(expected));
    expected
}

fn inspect_hosts<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    observed: &mut HostObservations,
    control: (Stop, &CancellationToken),
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let (stop, cancel) = control;
    let count = native.read(work, |access| Ok(access.claim_count()))?;
    let mut seen: Vec<(
        u32,
        n::SourceToken<'view, 'input>,
        b::SourceBlockRef,
        Vec<SourceSpan>,
    )> = Vec::new();
    let mut first_cacheable_r = true;
    for index in 0..count {
        let claim = native.read(work, |access| access.claim(index))?;
        let host = claim.host.expect("real native claim has its accepted host");
        let domain = native.read(work, |access| access.source_domain(host.source))?;
        let owner = native.read(work, |access| access.source_owner(host.source))?;
        let same_range = native.read(work, |access| {
            access.same_source(host.source, domain.source)
        })?;
        let full = host.utf8_len == domain.utf8_len && same_range;
        let binding = owner
            .map(|owner| native.read(work, |access| access.binding(owner)))
            .transpose()?;
        let cacheable = if full {
            match binding {
                None => true,
                Some(binding) => match binding.buffer {
                    Some(buffer) => {
                        native.read(work, |access| access.same_source(buffer, domain.source))?
                    }
                    None => false,
                },
            }
        } else {
            false
        };
        observed.claims += 1;
        if owner.is_none() && full {
            observed.full_p += 1;
        } else if cacheable {
            observed.cacheable_r += 1;
        } else {
            observed.uncached_r += 1;
        }
        let shorter_snapshot = owner.is_some() && host.utf8_len < domain.utf8_len;
        if shorter_snapshot {
            observed.shorter_r_snapshots += 1;
            assert!(!same_range);
        }
        // This is the original projection, with the actual accepted token. No
        // token or host_len is fabricated to stand in for an earlier snapshot.
        let original = native.sources(host.source, builder, work)?.0;
        let expected = source_sequence(builder, original, work)?;
        if shorter_snapshot {
            let complete = native.sources(domain.source, builder, work)?.0;
            assert_ne!(expected, source_sequence(builder, complete, work)?);
        }
        if cacheable && owner.is_some() && stop != Stop::None && !stop.after_hit() {
            observed.stopped += 1;
            return Err(stop_host(
                native,
                graph,
                builder,
                host,
                (stop, cancel),
                work,
            ));
        }
        let rows_before = builder.sources_len();
        let before = work.frozen_budget().usage();
        let first = host_domains(native, graph, builder, host, work)?;
        assert_eq!(source_sequence(builder, first, work)?, expected);
        let rows_after_first = builder.sources_len();
        let after_first = work.frozen_budget().usage();
        if cacheable && owner.is_some() && first_cacheable_r {
            // Graph::prepare has not called a host consumer. This first real R
            // cache entry must allocate an owned hosts page under this budget.
            assert_eq!(rows_after_first - rows_before, expected.len());
            assert!(after_first.auxiliary_capacity > before.auxiliary_capacity);
            assert!(after_first.auxiliary_capacity_peak >= after_first.auxiliary_capacity);
            observed.first_r_cache_allocations += 1;
            first_cacheable_r = false;
        }
        let again = host_domains(native, graph, builder, host, work)?;
        assert_eq!(source_sequence(builder, again, work)?, expected);
        if cacheable {
            assert_eq!(first, again);
            assert_eq!(builder.sources_len(), rows_after_first);
            assert_eq!(
                work.frozen_budget().usage().auxiliary_capacity,
                after_first.auxiliary_capacity
            );
        } else {
            assert_ne!(
                first, again,
                "historical or uncacheable domain must fall back"
            );
            assert_eq!(rows_after_first - rows_before, expected.len());
            assert_eq!(builder.sources_len() - rows_after_first, expected.len());
        }
        if let Some(binding) = binding {
            for (previous_owner, previous_domain, previous_block, previous_sources) in &seen {
                if !native.read(work, |access| {
                    access.same_source(*previous_domain, domain.source)
                })? {
                    // Compare final full domains, so inequality proves distinct
                    // R buffers rather than two ranges of a single R buffer.
                    assert_ne!(*previous_block, first);
                    observed.distinct_r_buffer_pairs += 1;
                    if previous_sources == &expected {
                        observed.same_geometry_r_pairs += 1;
                    }
                    if *previous_owner == binding.id.get() {
                        observed.same_owner_distinct_r_buffer_pairs += 1;
                    } else {
                        observed.different_owner_r_buffer_pairs += 1;
                    }
                }
            }
            seen.push((binding.id.get(), domain.source, first, expected));
        }
        if cacheable && owner.is_some() && stop != Stop::None && stop.after_hit() {
            observed.stopped += 1;
            return Err(stop_host(
                native,
                graph,
                builder,
                host,
                (stop, cancel),
                work,
            ));
        }
    }
    Ok(())
}

fn check_hidden_authorities<'id>(
    ir: &CompactIr,
    observed: &mut HostObservations,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut hidden: Vec<(ClaimId, AuthoritySpec)> = Vec::new();
    for index in 0..ir.atoms_len() {
        let atom = ir.owned_atom_at(index, work)?;
        if atom.kind != AtomKind::Hidden {
            continue;
        }
        let authority = ir.owned_authority(atom.claim, work)?;
        for (previous_claim, previous) in &hidden {
            assert_ne!(
                *previous_claim, atom.claim,
                "hidden permissions stay per occurrence"
            );
            if previous.host.domains == authority.host.domains
                && previous.consumed != authority.consumed
            {
                observed.distinct_hidden_authorities_sharing_domains += 1;
            }
        }
        hidden.push((atom.claim, authority));
    }
    Ok(())
}

fn run_host_case(text: &str, stop: Stop) -> (HostObservations, Result<(), CoreError>) {
    let budget = SourceBudget::task_fixture();
    let cancel = CancellationToken::new();
    let mut operation = stable_operation(&budget, cancel.clone());
    let source = SourceDocument::new(
        SourceKey::new([148; 16], 1).unwrap(),
        text.as_bytes(),
        &mut operation,
    )
    .unwrap()
    .snapshot();
    let mut observed = HostObservations::default();
    let result = construction_scope(&mut operation, |mut work| {
        let input = ParseInput::prepare_owned(source.clone(), &mut work)?;
        let builder = with_prepared(&input, &mut work, |input, mut host| {
            let result = n::compile(
                input,
                &options::g3(),
                &mut host,
                HostCompiler {
                    observed: &mut observed,
                    stop,
                    cancel: &cancel,
                    brand: std::marker::PhantomData,
                },
            );
            host.resolve(result)
        })?;
        assert_eq!(stop, Stop::None, "requested real R branch was not reached");
        let ir = builder.finish(&mut work)?;
        ir::validate::validate_owned(&ir, &source, &mut work)?;
        check_hidden_authorities(&ir, &mut observed, &mut work)?;
        drop((ir, input));
        work.finish()
    });
    if let Err(error) = result {
        assert_eq!(operation.first_fault(), Some(error));
    }
    drop(source);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    (observed, result)
}

#[test]
fn real_claim_host_projection_keeps_source_sequences_and_occurrence_authorities() {
    let fixtures = [
        "# 中😀\r\n\r\nalpha *one* &amp; beta **two** \\*tail\\*\r\n",
        "[r]: /u \"title\"\r\n\r\n- [x] [中][r] ![outer ![inner](v)](u)\r\n\r\n> quote\r\n>\r\n>     \tcode\r\n",
        "| A | B |\n| - | - |\n| a\\|b | **中** |\n| ![](u) | ![x](v) |\n",
        "prefix *中*\n| A | B |\n| - | - |\n| x | y |\n\n- [ ] label\n  continuation\n",
        "[r]: /u\nheading\n---\n\n[r]\n",
        "[r]: /u\n---\ncontinuation *body*\n",
        "[r]: /u\n===\ncontinuation *body*\n",
        "[r]: /u\n---\n| h |\n| - |\n| v |\n",
        "```text\r\nzero\0中\rbody\r\n```\r\n\r\n    indented\n\n\n",
    ];
    let mut total_claims = 0;
    let mut full_p = 0;
    let mut cacheable_r = 0;
    let mut uncached_r = 0;
    let mut snapshots = 0;
    let mut allocated = 0;
    let mut distinct_buffers = 0;
    let mut shared_authorities = 0;
    for text in fixtures {
        let (observed, result) = run_host_case(text, Stop::None);
        result.unwrap();
        eprintln!("R8 actual claim-host coverage: {observed:?}");
        total_claims += observed.claims;
        full_p += observed.full_p;
        cacheable_r += observed.cacheable_r;
        uncached_r += observed.uncached_r;
        snapshots += observed.shorter_r_snapshots;
        allocated += observed.first_r_cache_allocations;
        distinct_buffers += observed.distinct_r_buffer_pairs;
        shared_authorities += observed.distinct_hidden_authorities_sharing_domains;
    }
    assert!(total_claims > 0 && full_p > 0 && cacheable_r > 0);
    assert!(
        uncached_r > 0,
        "table/split hosts must exercise original projection fallback"
    );
    assert!(allocated > 0 && distinct_buffers > 0 && shared_authorities > 0);
    // A real shorter accepted snapshot, when present, was compared against both
    // the old projection and the different final full-buffer projection above.
    // Zero is an explicit coverage gap, never a synthetic-token substitute.
    eprintln!("R8 naturally grown R snapshot coverage: {snapshots}; zero remains unverified");
}

#[test]
fn real_host_cache_miss_and_hit_keep_first_fault_cancel_and_release() {
    for stop in [
        Stop::CancelMiss,
        Stop::CancelHit,
        Stop::FirstFaultMiss,
        Stop::FirstFaultHit,
    ] {
        let (observed, result) = run_host_case("alpha *one* &amp; beta **two**\n", stop);
        let expected = if stop.first_fault() {
            CoreError::InvalidBoundary
        } else {
            CoreError::Cancelled
        };
        assert_eq!(observed.stopped, 1, "actual branch must execute: {stop:?}");
        assert_eq!(result, Err(expected));
    }
}
