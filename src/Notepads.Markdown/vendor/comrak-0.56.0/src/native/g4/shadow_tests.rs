// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Prewritten A-only tests, authored before any A parser execution.
//!
//! Include from shadow.rs with #[cfg(test)] #[path = "shadow_tests.rs"] mod tests.
//! These exercise the real Subject/ordinary resolver via shadow::run, not B/C,
//! AcceptedModel, mapping queries, canonical replay, or performance eligibility.
//! Expected owners below are handwritten UTF8 byte intervals. Definition setup
//! is an explicit numeric fixture, not a claim that the block parser ran.
//! Leaf node/buffer tags are likewise private numeric fixtures; these tests do
//! not claim that a production block graph issued a SourceToken for those tags.
//!
//! The Host observes unique PERMIT identities, admitted capacity and permit Drop.
//! It does not receive allocation addresses/deallocation callbacks. Thus it can
//! distinguish parent leases, output leases and dropped A-only leases, but cannot
//! dynamically prove physical allocator frees. The latter is a STATIC ownership
//! argument: run's lexical Subject/arena/child scope and NativeRows owned buffers.
//! Do not relabel these assertions as actual-allocation-identity coverage.

use crate::native::{
    AcceptedFact, BufferId, Context, GeneratedPrefix, Host, NativeAdmission, NativeNodeId,
    NativeViolation, PayloadId, PayloadKind, RSpan, ReferenceIdentity, StageKind, WorkDelta,
    definitions::PendingDefinition,
    g4::shadow::{Seed, run},
    g4::{LeafBrand, OwnerKind, ShadowReleased, test_values, with_leaf},
    mapped::{self, Atom, ClaimSource, HostRole, HostSnapshot, MappedValue, NativePhase},
    rows::charge_host,
    small_value::SmallValue,
    value::{AcceptedValueRef, ValueStorage, ValueText},
};
use crate::parser::{
    ResolvedReference,
    inlines::{ByteTables, RefMap},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultKind {
    Budget,
    Cancel,
    Invalid,
}
#[derive(Debug)]
struct Fault {
    kind: FaultKind,
}
type Error = Rc<Fault>;

#[derive(Default)]
struct LeaseBook {
    next: u64,
    live: BTreeMap<u64, u64>,
    dropped: Vec<u64>,
}
struct Permit {
    id: u64,
    book: Rc<RefCell<LeaseBook>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut book = self.book.borrow_mut();
        assert!(
            book.live.remove(&self.id).is_some(),
            "permit must drop exactly once"
        );
        book.dropped.push(self.id);
    }
}

#[derive(Default)]
struct TestHost {
    book: Rc<RefCell<LeaseBook>>,
    first: Option<Error>,
    reserves: u64,
    actuals: u64,
    scans: u64,
    copies: u64,
    events: u64,
    records: u64,
    lookups: u64,
    reference_bytes: u64,
    decoded_bytes: u64,
    facts: u64,
    reserve_failure: Option<u64>,
    actual_failure: Option<u64>,
    lookup_failure: Option<FaultKind>,
}
impl TestHost {
    fn ready(&self) -> Result<(), Error> {
        self.first.clone().map_or(Ok(()), Err)
    }
    fn fail(&mut self, kind: FaultKind) -> Error {
        self.first
            .get_or_insert_with(|| Rc::new(Fault { kind }))
            .clone()
    }
    fn snapshot(&self) -> BTreeMap<u64, u64> {
        self.book.borrow().live.clone()
    }
}
impl Host for TestHost {
    type Error = Error;
    type Lease = Permit;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Error> {
        self.ready()?;
        match delta {
            WorkDelta::Scan(n) => self.scans = self.scans.checked_add(n).unwrap(),
            WorkDelta::Copy(n) => self.copies = self.copies.checked_add(n).unwrap(),
            WorkDelta::Events(n) => self.events = self.events.checked_add(n).unwrap(),
        }
        Ok(())
    }
    fn admit(&mut self, request: NativeAdmission) -> Result<(), Error> {
        self.ready()?;
        match request {
            NativeAdmission::Record(n) => self.records = self.records.checked_add(n).unwrap(),
            NativeAdmission::ReferenceLookup(n) => {
                if let Some(kind) = self.lookup_failure {
                    return Err(self.fail(kind));
                }
                self.lookups = self.lookups.checked_add(n).unwrap();
            }
            NativeAdmission::ReferenceBytes(n) => {
                self.reference_bytes = self.reference_bytes.checked_add(n).unwrap();
                // A has no reference occurrence materialization, including zero.
                return Err(self.fail(FaultKind::Invalid));
            }
            NativeAdmission::DecodedBytes(n) => {
                self.decoded_bytes = self.decoded_bytes.checked_add(n).unwrap()
            }
            NativeAdmission::Node(_) | NativeAdmission::Depth(_) => {}
        }
        Ok(())
    }
    fn reserve(&mut self, bytes: u64) -> Result<Permit, Error> {
        self.ready()?;
        self.reserves += 1;
        if self.reserve_failure == Some(self.reserves) {
            return Err(self.fail(FaultKind::Budget));
        }
        let mut book = self.book.borrow_mut();
        book.next = book.next.checked_add(1).unwrap();
        let id = book.next;
        assert!(book.live.insert(id, bytes).is_none());
        Ok(Permit {
            id,
            book: self.book.clone(),
        })
    }
    fn admit_actual(&mut self, permit: &mut Permit, bytes: u64) -> Result<(), Error> {
        self.ready()?;
        self.actuals += 1;
        *self.book.borrow_mut().live.get_mut(&permit.id).unwrap() = bytes;
        if self.actual_failure == Some(self.actuals) {
            return Err(self.fail(FaultKind::Budget));
        }
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Error> {
        self.facts += 1;
        // Numeric fixtures below publish no facts; shadow must keep all of its
        // ordinary acceptance facts private rather than leaking production events.
        Err(self.fail(FaultKind::Invalid))
    }
    fn reject(&mut self, _: NativeViolation) -> Error {
        self.fail(FaultKind::Invalid)
    }
}

fn ordinary_options() -> crate::Options<'static> {
    let mut options = crate::Options::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    // The strict G4 driver passes the ordinary configuration to A; upstream
    // autolink remains off. There is no B/C or extension guard bypass here.
    options
}

// (accepted kind, local start, local end, nearest accepted-owner parent,
//  exclusive opening-order subtree_end). The output's opening_slot is start+1.
type Expected = (OwnerKind, usize, usize, Option<usize>, usize);
fn assert_owners(
    result: &ShadowReleased<'_, Permit>,
    leaf: LeafBrand<'_>,
    expected: &[Expected],
    ctx: &mut Context<'_, TestHost>,
) {
    assert_eq!(result.owners.len(), expected.len());
    assert_eq!(
        (
            result.leaf.node,
            result.leaf.buffer,
            result.leaf.base,
            result.leaf.bytes
        ),
        (leaf.node, leaf.buffer, leaf.base, leaf.bytes)
    );
    for (index, &(kind, start, end, parent, subtree_end)) in expected.iter().enumerate() {
        let row = result
            .owners
            .get(index, |n| charge_host(ctx.host, n))
            .unwrap()
            .unwrap();
        assert_eq!(
            (row.kind, row.start, row.end, row.parent, row.subtree_end),
            (kind, start, end, parent, subtree_end)
        );
        assert_eq!(row.opening_slot, u64::try_from(start).unwrap() + 1);
        assert_eq!(
            (
                row.leaf.node,
                row.leaf.buffer,
                row.leaf.base,
                row.leaf.bytes
            ),
            (leaf.node, leaf.buffer, leaf.base, leaf.bytes)
        );
    }
}

fn assert_only_parent_and_output(
    host: &TestHost,
    parent: &BTreeMap<u64, u64>,
    last_parent_id: u64,
    output_present: bool,
) {
    let live = host.snapshot();
    for (&id, &bytes) in parent {
        assert_eq!(live.get(&id), Some(&bytes), "parent permit changed");
    }
    let additional = live.keys().filter(|id| !parent.contains_key(*id)).count();
    // These finite controls produce at most two owners, so NativeRows needs one
    // output page and one directory. All child registry/arena/stack leases gone.
    assert_eq!(additional, if output_present { 2 } else { 0 });
    let book = host.book.borrow();
    assert!(
        book.next > last_parent_id,
        "new A/output permits must have been issued"
    );
    assert!(
        book.dropped.iter().any(|&id| id > last_parent_id),
        "A scratch leases must have dropped"
    );
}

fn check(raw: &str, base: usize, line: usize, offsets: &[usize], expected: &[Expected]) {
    let options = ordinary_options();
    let tables = ByteTables::new(&options);
    let refmap = RefMap::new();
    let mut host = TestHost::default();
    {
        let mut parent = Context::new(&mut host, base.checked_add(raw.len()).unwrap());
        let marker = parent.string("parent remains live while A runs").unwrap();
        let marker_pointer = marker.as_ptr();
        let before = parent.host.snapshot();
        let last_parent_id = parent.host.book.borrow().next;
        with_leaf(
            NativeNodeId::from_raw(41).unwrap(),
            BufferId::from_raw(17).unwrap(),
            base,
            raw.len(),
            |leaf| {
                let result = run(
                    leaf,
                    Seed {
                        raw,
                        offsets,
                        line,
                        in_paragraph: true,
                        in_table_cell: false,
                        options: &options,
                        refmap: &refmap,
                        tables: &tables,
                    },
                    &mut parent,
                )
                .unwrap();
                assert_owners(&result, leaf, expected, &mut parent);
                assert_only_parent_and_output(
                    parent.host,
                    &before,
                    last_parent_id,
                    !expected.is_empty(),
                );
                assert_eq!(marker.as_ptr(), marker_pointer);
                assert_eq!(marker, "parent remains live while A runs");
                assert!(!parent.is_shadow());
                drop(result);
                assert_eq!(
                    parent.host.snapshot(),
                    before,
                    "output ledger leases actually dropped"
                );
                assert_eq!(parent.host.facts, 0);
                assert_eq!(parent.host.reference_bytes, 0);
                assert!(parent.host.scans > 0 && parent.host.copies > 0 && parent.host.events > 0);
            },
        );
        drop(marker); // actual parent String before its registry/permits
    }
    assert!(host.snapshot().is_empty());
    assert!(host.first.is_none());
}

#[test]
fn real_ordinary_owners_early_future_and_incomplete_syntax() {
    use OwnerKind::{Code, Html, Image, Link};
    check("`www.a.com`", 0, 1, &[0], &[(Code, 0, 11, None, 1)]);
    check("<b>", 0, 1, &[0], &[(Html, 0, 3, None, 1)]);
    check("[x](/u)", 0, 1, &[0], &[(Link, 0, 7, None, 1)]);
    check("![x](/u)", 0, 1, &[0], &[(Image, 0, 8, None, 1)]);
    check("[x](/u \"t\")", 0, 1, &[0], &[(Link, 0, 11, None, 1)]);
    // A records this future Code; only B is allowed to invalidate it later.
    check("www.a.com/`x`", 0, 1, &[0], &[(Code, 10, 13, None, 1)]);
    check("[ www.a.com", 0, 1, &[0], &[]);
    check("`unterminated", 0, 1, &[0], &[]);
    check("<b", 0, 1, &[0], &[]);
    // No outer Image was accepted; the real inner Link has no owner parent.
    check("![a [b](/u)", 0, 1, &[0], &[(Link, 4, 11, None, 1)]);
}

#[test]
fn nested_image_link_open_order_and_nonzero_base_are_handwritten() {
    use OwnerKind::{Code, Image, Link};
    // 0 !,1 [,2 a,3 SP,4 [,5 b,6 ],7 (,8 /,9 u,10 ),11 ],
    // 12 (,13 /,14 v,15 ). The Image is emitted after its child by the resolver,
    // but the returned ledger must be in opening order, not fact arrival order.
    check(
        "![a [b](/u)](/v)",
        0,
        1,
        &[0],
        &[(Image, 0, 16, None, 2), (Link, 4, 11, Some(0), 2)],
    );
    check(
        "`x` [y](/u)",
        37,
        9,
        &[4],
        &[(Code, 0, 3, None, 1), (Link, 4, 11, None, 2)],
    );
    check("a\n`b`", 37, 9, &[2, 4], &[(Code, 2, 5, None, 1)]);
}

/// Explicit fixture scope: two accepted definition records, not block parsing.
/// Raw conceptual rows are `[r]: /winner "title"\n` (21B) and
/// `[r]: /loser "other"\n` (20B), each in its own source buffer. Clean values
/// equal those ASCII spellings. Payload/proof records below move actual String
/// owners into the current representation. Their source spans and empty proof
/// inputs remain numeric fixtures, not R/P transport, block-cut or production
/// provenance proofs. A must only borrow winner/key/identity, never these values.
/// RefMap::insert itself decides first-winner once and preserves both rows.
fn install_references(parent: &mut Context<'_, TestHost>, refmap: &mut RefMap) {
    install_reference_values(parent, refmap, [("/winner", "title"), ("/loser", "other")]);
}

fn install_reference_values(
    parent: &mut Context<'_, TestHost>,
    refmap: &mut RefMap,
    values: [(&str, &str); 2],
) {
    parent.direct = true;
    for (index, (destination, title)) in values.into_iter().enumerate() {
        let ordinal = u32::try_from(index).unwrap() + 1;
        // Independent ASCII fixture spelling: [r]: URL "TITLE" + LF.
        let raw_len = 9 + destination.len() + title.len();
        let id = parent.definition().unwrap();
        assert_eq!(id.get(), ordinal);
        let buffer = BufferId::from_raw(100 + ordinal).unwrap();
        parent.pending_definition = Some(PendingDefinition {
            id,
            syntax: RSpan {
                buffer,
                start: 0,
                end: raw_len,
            },
            label: RSpan {
                buffer,
                start: 1,
                end: 2,
            },
        });
        let key = parent.string("r").unwrap();
        let destination_id = PayloadId::from_raw(ordinal * 2 - 1).unwrap();
        let title_id = PayloadId::from_raw(ordinal * 2).unwrap();
        let url = fixture_payload(
            parent,
            destination_id,
            PayloadKind::Destination,
            destination,
            buffer,
        );
        let title = fixture_payload(parent, title_id, PayloadKind::Title, title, buffer);
        let accepted = refmap
            .map
            .insert(
                key,
                ResolvedReference {
                    url,
                    title,
                    native: Some(ReferenceIdentity {
                        definition: id,
                        destination: destination_id,
                        title: Some(title_id),
                    }),
                },
                parent,
            )
            .unwrap();
        assert_eq!(accepted, ordinal == 1);
    }
    assert_eq!(parent.definitions.len(), 2);
    parent.direct = false;
    // Shadow must not spend the compatibility reference fuel at all.
    refmap.max_ref_size = 0;
}

fn fixture_payload(
    parent: &mut Context<'_, TestHost>,
    payload: PayloadId,
    role: PayloadKind,
    text: &str,
    buffer: BufferId,
) -> ValueText {
    let result = parent.string(text).unwrap();
    let bytes = result.len();
    let span = RSpan {
        buffer,
        start: 0,
        end: 1,
    };
    let proof = parent.mappings.proofs.len();
    parent
        .mappings
        .proofs
        .push(
            mapped::Proof {
                kind: mapped::TransformKind::Payload(StageKind::Entity),
                stage: 1,
                host: HostSnapshot {
                    source: ClaimSource::R(span),
                    utf8_len: 1,
                    consumed_start: 0,
                    consumed_end: 1,
                    payload_role: Some(role),
                    role: HostRole::Inline,
                    phase: NativePhase::Inline,
                },
                input: SmallValue::new(),
                local_start: 0,
                local_end: 1,
                result,
            },
            parent.host,
        )
        .unwrap();
    let mut value = MappedValue::default();
    value.bytes = bytes;
    value
        .atoms
        .push(
            Atom {
                source: span,
                bytes,
                proof: Some(proof),
                hidden_syntax: false,
            },
            &mut parent.mapping_access(),
        )
        .unwrap();
    assert_eq!(payload.get() as usize, parent.mappings.payloads.len() + 1);
    parent
        .mappings
        .payloads
        .push(
            mapped::PayloadBinding {
                role,
                raw: span,
                value,
                stage: 1,
                stage_len: bytes,
                prefix: None,
                prefix_kind: GeneratedPrefix::Http,
            },
            parent.host,
        )
        .unwrap();
    ValueText::accepted(AcceptedValueRef::Payload { payload, role })
}

fn fixture_values(parent: &mut Context<'_, TestHost>, index: usize) -> (ValueText, ValueText) {
    let row = parent
        .definitions
        .get(index, |n| charge_host(parent.host, n))
        .unwrap()
        .unwrap();
    assert!(matches!(row.value.url.repr, ValueStorage::Accepted(_)));
    assert!(matches!(row.value.title.repr, ValueStorage::Accepted(_)));
    (row.value.url.clone(), row.value.title.clone())
}

fn fixture_result_pointer(parent: &mut Context<'_, TestHost>, value: &ValueText) -> usize {
    let ValueStorage::Accepted(AcceptedValueRef::Payload { payload, role }) = &value.repr else {
        panic!("numeric definition fixture must keep an Accepted payload");
    };
    let binding = parent
        .mappings
        .payloads
        .get(payload.get() as usize - 1, |n| charge_host(parent.host, n))
        .unwrap()
        .unwrap();
    assert_eq!(binding.role, *role);
    assert!(binding.prefix.is_none());
    assert_eq!(binding.value.atoms.len(), 1);
    let atom = binding.value.atoms[0];
    assert!(!atom.hidden_syntax);
    let proof = parent
        .mappings
        .proofs
        .get(atom.proof.unwrap(), |n| charge_host(parent.host, n))
        .unwrap()
        .unwrap();
    assert_eq!(proof.result.len(), atom.bytes);
    proof.result.as_ptr() as usize
}

fn assert_definitions(parent: &mut Context<'_, TestHost>) -> [(usize, usize); 2] {
    let mut pointers = [(0, 0); 2];
    for (index, (url, title, winner)) in [("/winner", "title", true), ("/loser", "other", false)]
        .into_iter()
        .enumerate()
    {
        let (url_value, title_value) = fixture_values(parent, index);
        let actual_url = test_values::text(parent, &url_value).unwrap();
        let actual_title = test_values::text(parent, &title_value).unwrap();
        pointers[index] = (
            fixture_result_pointer(parent, &url_value),
            fixture_result_pointer(parent, &title_value),
        );
        let row = parent
            .definitions
            .get(index, |n| charge_host(parent.host, n))
            .unwrap()
            .unwrap();
        assert_eq!(row.key, "r");
        assert_eq!(
            (actual_url.as_str(), actual_title.as_str(), row.winner),
            (url, title, winner)
        );
        assert_eq!(row.metadata.id.get() as usize, index + 1);
        let identity = row.value.native.unwrap();
        assert_eq!(identity.definition, row.metadata.id);
        assert_eq!(identity.destination.get() as usize, index * 2 + 1);
        assert_eq!(identity.title.unwrap().get() as usize, index * 2 + 2);
    }
    pointers
}

#[test]
fn reference_winner_is_shared_readonly_without_occurrence_payload() {
    let options = ordinary_options();
    let tables = ByteTables::new(&options);
    let mut host = TestHost::default();
    {
        let mut parent = Context::new(&mut host, 128);
        let mut refmap = RefMap::new();
        install_references(&mut parent, &mut refmap);
        let pointers = assert_definitions(&mut parent);
        let before = parent.host.snapshot();
        let last_parent_id = parent.host.book.borrow().next;
        let raw = "[x][r] ![y][r]";
        // Actual resolver must use winner #1; shadow_winner rejects loser #2.
        // Both owner extents are derived from raw, independent of URL contents.
        with_leaf(
            NativeNodeId::from_raw(41).unwrap(),
            BufferId::from_raw(17).unwrap(),
            23,
            14,
            |leaf| {
                let result = run(
                    leaf,
                    Seed {
                        raw,
                        offsets: &[0],
                        line: 1,
                        in_paragraph: true,
                        in_table_cell: false,
                        options: &options,
                        refmap: &refmap,
                        tables: &tables,
                    },
                    &mut parent,
                )
                .unwrap();
                assert_owners(
                    &result,
                    leaf,
                    &[
                        (OwnerKind::Link, 0, 6, None, 1),
                        (OwnerKind::Image, 7, 14, None, 2),
                    ],
                    &mut parent,
                );
                assert_only_parent_and_output(parent.host, &before, last_parent_id, true);
                assert_eq!(parent.host.lookups, 2);
                assert_eq!(parent.host.reference_bytes, 0);
                assert_eq!(parent.host.facts, 0);
                assert!(
                    parent.host.decoded_bytes > 0,
                    "ordinary label work is still charged"
                );
                assert_eq!(assert_definitions(&mut parent), pointers);
                drop(result);
                assert_eq!(parent.host.snapshot(), before);
            },
        );
        // A second invocation uses the same frozen map and still succeeds with
        // zero compatibility fuel; A did not consume or replace the winner.
        let raw = "[r]";
        with_leaf(
            NativeNodeId::from_raw(42).unwrap(),
            BufferId::from_raw(18).unwrap(),
            0,
            3,
            |leaf| {
                let result = run(
                    leaf,
                    Seed {
                        raw,
                        offsets: &[0],
                        line: 1,
                        in_paragraph: true,
                        in_table_cell: false,
                        options: &options,
                        refmap: &refmap,
                        tables: &tables,
                    },
                    &mut parent,
                )
                .unwrap();
                assert_owners(
                    &result,
                    leaf,
                    &[(OwnerKind::Link, 0, 3, None, 1)],
                    &mut parent,
                );
                drop(result);
                assert_eq!(parent.host.snapshot(), before);
            },
        );
        assert_eq!(parent.host.lookups, 3);
        assert_eq!(assert_definitions(&mut parent), pointers);
        refmap.release_owned(&mut parent).unwrap();
    }
    assert!(host.snapshot().is_empty());
    assert!(host.first.is_none());
}

#[test]
fn unknown_reference_is_a_miss_not_a_shadow_failure() {
    check("[x][missing]", 11, 1, &[0], &[]);
}

#[test]
fn winner_lookup_cancel_and_budget_preserve_parent_and_first_fault() {
    for kind in [FaultKind::Cancel, FaultKind::Budget] {
        let options = ordinary_options();
        let tables = ByteTables::new(&options);
        let mut host = TestHost::default();
        {
            let mut parent = Context::new(&mut host, 128);
            let mut refmap = RefMap::new();
            install_references(&mut parent, &mut refmap);
            let before = parent.host.snapshot();
            let last_parent_id = parent.host.book.borrow().next;
            parent.host.lookup_failure = Some(kind);
            with_leaf(
                NativeNodeId::from_raw(41).unwrap(),
                BufferId::from_raw(17).unwrap(),
                0,
                6,
                |leaf| {
                    let error = match run(
                        leaf,
                        Seed {
                            raw: "[x][r]",
                            offsets: &[0],
                            line: 1,
                            in_paragraph: true,
                            in_table_cell: false,
                            options: &options,
                            refmap: &refmap,
                            tables: &tables,
                        },
                        &mut parent,
                    ) {
                        Ok(_) => panic!("injected lookup failure must not return an owner ledger"),
                        Err(error) => error,
                    };
                    assert_eq!(error.kind, kind);
                    assert_eq!(parent.host.snapshot(), before);
                    let book = parent.host.book.borrow();
                    assert!(book.next > last_parent_id);
                    assert!(book.dropped.iter().any(|&id| id > last_parent_id));
                    drop(book);
                    assert_eq!(parent.host.reference_bytes, 0);
                    assert_eq!(parent.host.facts, 0);
                    assert!(Rc::ptr_eq(&error, parent.host.first.as_ref().unwrap()));
                    assert!(Rc::ptr_eq(
                        &error,
                        &parent.host.reject(NativeViolation::InternalInvariant)
                    ));
                    let repeated = parent.event().unwrap_err();
                    assert!(Rc::ptr_eq(&error, &repeated));
                },
            );
            // On a latched error, drop actual refmap storage before parent.
            // Do not call a fallible cleanup operation and reset the first fault.
            drop(refmap);
        }
        assert!(host.snapshot().is_empty());
        assert_eq!(host.first.as_ref().unwrap().kind, kind);
    }
}

#[test]
fn partial_shadow_allocation_failure_releases_only_child_leases() {
    for fail_actual in [false, true] {
        let options = ordinary_options();
        let tables = ByteTables::new(&options);
        let refmap = RefMap::new();
        let mut host = TestHost::default();
        {
            let mut parent = Context::new(&mut host, 32);
            let marker = parent.string("parent allocation").unwrap();
            let before = parent.host.snapshot();
            let last_parent_id = parent.host.book.borrow().next;
            // Inject at the second A reservation/actual-capacity admission so
            // there is a previously successful A capacity admission to unwind.
            if fail_actual {
                parent.host.actual_failure = Some(parent.host.actuals + 2);
            } else {
                parent.host.reserve_failure = Some(parent.host.reserves + 2);
            }
            with_leaf(
                NativeNodeId::from_raw(41).unwrap(),
                BufferId::from_raw(17).unwrap(),
                0,
                3,
                |leaf| {
                    let error = match run(
                        leaf,
                        Seed {
                            raw: "`x`",
                            offsets: &[0],
                            line: 1,
                            in_paragraph: true,
                            in_table_cell: false,
                            options: &options,
                            refmap: &refmap,
                            tables: &tables,
                        },
                        &mut parent,
                    ) {
                        Ok(_) => {
                            panic!("injected capacity failure must not return an owner ledger")
                        }
                        Err(error) => error,
                    };
                    assert_eq!(error.kind, FaultKind::Budget);
                    assert_eq!(parent.host.snapshot(), before);
                    assert!(
                        parent
                            .host
                            .book
                            .borrow()
                            .dropped
                            .iter()
                            .any(|&id| id > last_parent_id)
                    );
                    assert_eq!(marker, "parent allocation");
                    assert!(Rc::ptr_eq(
                        &error,
                        &parent.host.reject(NativeViolation::InvalidProvenance)
                    ));
                },
            );
            drop(marker);
        }
        assert!(host.snapshot().is_empty());
    }
}

#[test]
fn mismatched_leaf_length_rejects_before_shadow_storage() {
    let options = ordinary_options();
    let tables = ByteTables::new(&options);
    let refmap = RefMap::new();
    let mut host = TestHost::default();
    {
        let mut parent = Context::new(&mut host, 3);
        let before = parent.host.snapshot();
        let allocations_before = parent.host.book.borrow().next;
        with_leaf(
            NativeNodeId::from_raw(41).unwrap(),
            BufferId::from_raw(17).unwrap(),
            0,
            2,
            |leaf| {
                assert!(
                    matches!(run(leaf, Seed { raw: "`x`", offsets: &[0], line: 1, in_paragraph: true, in_table_cell: false, options: &options, refmap: &refmap, tables: &tables }, &mut parent), Err(error) if error.kind == FaultKind::Invalid)
                );
            },
        );
        assert_eq!(parent.host.snapshot(), before);
        assert_eq!(parent.host.book.borrow().next, allocations_before);
    }
    assert!(host.snapshot().is_empty());
}

#[test]
fn reference_payload_values_keep_winner_and_release_without_materialization() {
    // Independent allocations may have different Registry pointer-hash probe
    // costs. Assert semantic admissions and ownership for each fixed fixture;
    // never use equality of address-sensitive cumulative work as an oracle.
    for values in [
        [("/winner", "title"), ("/loser", "other")],
        [
            (
                "/destination-longer-than-original",
                "a visibly longer title",
            ),
            ("/different-losing-destination", "a different losing title"),
        ],
    ] {
        let options = ordinary_options();
        let tables = ByteTables::new(&options);
        let mut host = TestHost::default();
        {
            let mut parent = Context::new(&mut host, 256);
            let mut refmap = RefMap::new();
            install_reference_values(&mut parent, &mut refmap, values);
            let parent_permits = parent.host.snapshot();
            let last_parent_id = parent.host.book.borrow().next;
            let lookups = parent.host.lookups;
            let decoded = parent.host.decoded_bytes;
            let scans = parent.host.scans;
            let copies = parent.host.copies;
            let pointers: [(usize, usize); 2] = std::array::from_fn(|index| {
                let (url, title) = fixture_values(&mut parent, index);
                (
                    fixture_result_pointer(&mut parent, &url),
                    fixture_result_pointer(&mut parent, &title),
                )
            });
            with_leaf(
                NativeNodeId::from_raw(41).unwrap(),
                BufferId::from_raw(17).unwrap(),
                0,
                3,
                |leaf| {
                    let result = run(
                        leaf,
                        Seed {
                            raw: "[r]",
                            offsets: &[0],
                            line: 1,
                            in_paragraph: true,
                            in_table_cell: false,
                            options: &options,
                            refmap: &refmap,
                            tables: &tables,
                        },
                        &mut parent,
                    )
                    .unwrap();
                    assert_owners(
                        &result,
                        leaf,
                        &[(OwnerKind::Link, 0, 3, None, 1)],
                        &mut parent,
                    );
                    assert_only_parent_and_output(
                        parent.host,
                        &parent_permits,
                        last_parent_id,
                        true,
                    );
                    assert_eq!(parent.host.lookups.checked_sub(lookups), Some(1));
                    assert_eq!(parent.host.reference_bytes, 0);
                    assert_eq!(parent.host.facts, 0);
                    assert!(parent.host.decoded_bytes > decoded);
                    assert!(parent.host.scans > scans && parent.host.copies > copies);
                    assert_eq!(parent.definitions.len(), 2);
                    for (index, &(url, title)) in values.iter().enumerate() {
                        let (url_value, title_value) = fixture_values(&mut parent, index);
                        let actual_url = test_values::text(&mut parent, &url_value).unwrap();
                        let actual_title = test_values::text(&mut parent, &title_value).unwrap();
                        let actual_pointers = (
                            fixture_result_pointer(&mut parent, &url_value),
                            fixture_result_pointer(&mut parent, &title_value),
                        );
                        let row = parent
                            .definitions
                            .get(index, |n| charge_host(parent.host, n))
                            .unwrap()
                            .unwrap();
                        assert_eq!(row.key, "r");
                        assert_eq!(actual_url, url);
                        assert_eq!(actual_title, title);
                        assert_eq!(row.winner, index == 0);
                        assert_eq!(row.metadata.id.get() as usize, index + 1);
                        let identity = row.value.native.unwrap();
                        assert_eq!(identity.definition, row.metadata.id);
                        assert_eq!(identity.destination.get() as usize, index * 2 + 1);
                        assert_eq!(identity.title.unwrap().get() as usize, index * 2 + 2);
                        assert_eq!(actual_pointers, pointers[index]);
                    }
                    drop(result);
                    assert_eq!(parent.host.snapshot(), parent_permits);
                    assert!(parent.host.first.is_none());
                },
            );
            refmap.release_owned(&mut parent).unwrap();
        }
        assert!(host.snapshot().is_empty());
        assert!(host.first.is_none());
    }
}
