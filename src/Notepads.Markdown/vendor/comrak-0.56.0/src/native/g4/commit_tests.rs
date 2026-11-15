// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Prewritten private A/B/C raw-commit oracles; NOT executed by their author.
//!
//! Intended inclusion from native/g4/mod.rs as a cfg(test) child module. This
//! file does not enable the public G4 profile, upstream autolink, or email.
//! The five tests bind the approved design section 5's three mandatory cases,
//! raw CRLF plus quote transport, and WWW/URL typed-prefix separation. Expected
//! bytes, whole AST, current mappings and claims are written from those rules,
//! never obtained from a parser run. Real node/buffer/payload/proof identities
//! are bound ONLY after the complete expected AST and parent edges pass.
//!
//! The observation point is parse_in_context's live-arena finish closure. It
//! is not Native FinalGraph acceptance, Core IR/proof lowering, an S<->target
//! query, or production eligibility. In particular CRLF below is raw native P
//! input, not a claim to have tested Core's preceding N01 S->P normalization.
//! The quote has nonzero P offsets and a real hole between two Copy pieces.
//!
//! Test-only observer Vecs are outside the producer's allocation accounting.
//! The Host observes unique permit identities, admitted capacity and Drop, not
//! physical allocation addresses. Final zero proves permit release, not dynamic
//! physical deallocation or a cancellation/failure-injection matrix. No exact
//! Scan/Events total is asserted: allocation-registry hashing can vary by run.

use crate::native::{
    AcceptedFact, AppendKind, BufferId, Context, GeneratedPrefix, Host, InlinePolicy,
    NativeAdmission, NativeNodeId, NativeViolation, PSpan, PayloadKind, RSpan, SyntaxRole,
    WorkDelta,
    g4::{ExtendedKind, test_values},
    mapped::{
        Atom, ClaimKind, ClaimSource, HostRole, MappedValue, NativePhase, NodeBinding,
        TransformKind,
    },
    rows::{NativeRows, charge_host},
};
use crate::{
    Options,
    nodes::{Node, NodeValue},
    parser::parse_in_context,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

type Error = NativeViolation;

#[derive(Default)]
struct Book {
    next: u64,
    live: BTreeMap<u64, u64>,
    dropped: u64,
}
struct Permit {
    id: u64,
    book: Rc<RefCell<Book>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut book = self.book.borrow_mut();
        assert!(book.live.remove(&self.id).is_some());
        book.dropped = book.dropped.checked_add(1).unwrap();
    }
}
#[derive(Default)]
struct TestHost {
    book: Rc<RefCell<Book>>,
    first: Option<Error>,
}
impl TestHost {
    fn ready(&self) -> Result<(), Error> {
        self.first.map_or(Ok(()), Err)
    }
}
impl Host for TestHost {
    type Error = Error;
    type Lease = Permit;
    fn work(&mut self, _: WorkDelta) -> Result<(), Error> {
        self.ready()
    }
    fn admit(&mut self, _: NativeAdmission) -> Result<(), Error> {
        self.ready()
    }
    fn reserve(&mut self, bytes: u64) -> Result<Permit, Error> {
        self.ready()?;
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
        *self.book.borrow_mut().live.get_mut(&permit.id).unwrap() = bytes;
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Error> {
        // Direct mode must use the real mappings store, never this tape branch.
        Err(self.reject(NativeViolation::InternalInvariant))
    }
    fn reject(&mut self, violation: NativeViolation) -> Error {
        *self.first.get_or_insert(violation)
    }
}

#[derive(Clone, Copy)]
enum Kind<'a> {
    Document,
    Quote,
    Paragraph,
    Emph,
    Link(&'a str),
    Text(&'a str),
    Html(&'a str),
    SoftBreak,
}
struct ExpectedNode<'a> {
    parent: Option<usize>,
    kind: Kind<'a>,
}
const fn node(parent: Option<usize>, kind: Kind<'_>) -> ExpectedNode<'_> {
    ExpectedNode { parent, kind }
}
#[derive(Clone, Copy)]
enum Domain {
    P,
    R,
}
struct ExpectedClaim {
    owner: usize,
    kind: ClaimKind,
    domain: Domain,
    start: usize,
    end: usize,
}
const fn claim(
    owner: usize,
    kind: ClaimKind,
    domain: Domain,
    start: usize,
    end: usize,
) -> ExpectedClaim {
    ExpectedClaim {
        owner,
        kind,
        domain,
        start,
        end,
    }
}
struct Oracle<'a> {
    source: &'a str,
    nodes: &'a [ExpectedNode<'a>],
    paragraph: usize,
    raw_len: usize,
    // Exact per-line append transport: R start/end, P start/end. No envelope.
    pieces: &'a [(usize, usize, usize, usize)],
    link: usize,
    label: usize,
    selection: (usize, usize),
    extended: ExtendedKind,
    copies: &'a [(usize, usize, usize)],
    soft: Option<(usize, usize, usize)>,
    claims: &'a [ExpectedClaim],
}

fn read<'r, T>(rows: &'r NativeRows<T, Permit>, index: usize, host: &mut TestHost) -> &'r T {
    rows.get(index, |n| charge_host(host, n))
        .unwrap()
        .expect("real stored row")
}
fn collect<'a>(current: Node<'a>, output: &mut Vec<Node<'a>>) {
    output.push(current);
    for child in current.children() {
        collect(child, output);
    }
}
fn raw(buffer: BufferId, start: usize, end: usize) -> RSpan {
    // Only a coordinate expectation, not a SourceToken/capability constructor.
    RSpan { buffer, start, end }
}
fn copy_atom(atom: &Atom, span: RSpan) {
    assert_eq!(atom.source, span);
    assert_eq!(atom.bytes, span.end - span.start);
    assert_eq!(atom.proof, None);
    assert!(!atom.hidden_syntax);
}
fn copy_value(value: &MappedValue, span: RSpan) {
    assert_eq!(value.bytes, span.end - span.start);
    assert_eq!(value.atoms.len(), 1);
    copy_atom(&value.atoms[0], span);
}

fn verify(root: Node<'_>, ctx: &mut Context<'_, TestHost>, oracle: &Oracle<'_>) {
    let mut actual = Vec::new();
    collect(root, &mut actual);
    assert_eq!(
        actual.len(),
        oracle.nodes.len(),
        "complete final AST cardinality"
    );
    for (i, expected) in oracle.nodes.iter().enumerate() {
        match expected.parent {
            Some(parent) => {
                assert!(parent < i);
                assert!(actual[i].parent().unwrap().same_node(actual[parent]));
            }
            None => assert!(actual[i].parent().is_none()),
        }
        let data = actual[i].data();
        match (expected.kind, &data.value) {
            (Kind::Document, NodeValue::Document)
            | (Kind::Quote, NodeValue::BlockQuote)
            | (Kind::Paragraph, NodeValue::Paragraph)
            | (Kind::Emph, NodeValue::Emph)
            | (Kind::SoftBreak, NodeValue::SoftBreak) => {}
            (Kind::Text(text), NodeValue::Text(value)) => {
                assert_eq!(test_values::text(ctx, value).unwrap(), text)
            }
            (Kind::Html(text), NodeValue::HtmlInline(value)) => {
                assert_eq!(test_values::text(ctx, value).unwrap(), text)
            }
            (Kind::Link(url), NodeValue::Link(value)) => {
                assert_eq!(test_values::text(ctx, &value.url).unwrap(), url);
                assert!(test_values::text(ctx, &value.title).unwrap().is_empty());
            }
            _ => panic!(
                "unexpected AST value at handwritten preorder index {i}: {:?}",
                data.value
            ),
        }
    }
    // Identity binding begins after every final node/value/parent check above.
    let ids: Vec<NativeNodeId> = actual.iter().map(|n| n.data().native_id.unwrap()).collect();
    let map = &ctx.mappings;
    let host = &mut *ctx.host;
    let bindings: Vec<NodeBinding> = ids
        .iter()
        .map(|id| *read(&map.nodes, id.get() as usize - 1, host))
        .collect();
    let paragraph = &bindings[oracle.paragraph];
    let block = read(
        &map.blocks,
        paragraph.block.unwrap().get() as usize - 1,
        host,
    );
    let buffer = block.buffer.unwrap();
    let stored = read(&map.buffers, buffer.get() as usize - 1, host);
    assert_eq!(stored.owner, ids[oracle.paragraph]);
    assert!(stored.sealed);
    assert_eq!(stored.len, oracle.raw_len);
    assert_eq!(stored.pieces.len(), oracle.pieces.len());
    for (piece, &(rs, re, ps, pe)) in stored.pieces.iter().zip(oracle.pieces) {
        assert_eq!(piece.r, raw(buffer, rs, re));
        assert_eq!(piece.p, PSpan { start: ps, end: pe });
        assert_eq!(piece.kind, AppendKind::Copy);
        assert_eq!(re - rs, pe - ps);
    }
    assert_eq!(map.table_pipes.len(), 0);
    assert_eq!(ctx.definitions.len(), 0);
    assert_eq!(map.payloads.len(), 1);
    assert_eq!(map.syntax.len(), 1);

    let selected = raw(buffer, oracle.selection.0, oracle.selection.1);
    let syntax = read(
        &map.syntax,
        bindings[oracle.link].syntax.unwrap().get() as usize - 1,
        host,
    );
    assert_eq!(syntax.syntax, Some(selected));
    assert_eq!(syntax.label, Some(selected));
    assert_eq!(syntax.title, None);
    assert_eq!(syntax.definition, None);
    assert!(!syntax.reference_use);
    let extended = syntax.extended.expect("accepted Extended authority");
    assert_eq!(extended.node, ids[oracle.link]);
    assert_eq!(extended.label, ids[oracle.label]);
    assert_eq!(extended.kind, oracle.extended);
    assert_eq!(extended.raw, selected);
    assert_eq!(syntax.destination, Some(extended.destination));
    let payload = read(&map.payloads, extended.destination.get() as usize - 1, host);
    assert_eq!(payload.role, PayloadKind::Destination);
    assert_eq!(payload.raw, selected);
    assert_eq!(payload.stage, 0);
    assert_eq!(payload.stage_len, selected.end - selected.start);
    copy_value(&payload.value, selected);
    match oracle.extended {
        ExtendedKind::Www => {
            let (anchor, text) = payload.prefix.as_ref().expect("WWW has typed Http");
            assert_eq!(payload.prefix_kind, GeneratedPrefix::Http);
            assert_eq!(*anchor, raw(buffer, selected.start, selected.start));
            assert_eq!(text, "http://");
        }
        ExtendedKind::Url => assert!(payload.prefix.is_none(), "URL has no Synthetic prefix"),
        ExtendedKind::Email => panic!("raw commit fixture does not contain decoded email"),
    }
    // Every retained AST value slot is accounted for; discarded delimiter Text
    // rows may still exist in the native store and are not final AST nodes.
    for (i, binding) in bindings.iter().enumerate() {
        let has_value = oracle.copies.iter().any(|&(owner, _, _)| owner == i)
            || oracle.soft.is_some_and(|(owner, _, _)| owner == i);
        assert_eq!(binding.value.is_some(), has_value, "value slot at {i}");
        if i != oracle.link {
            assert!(binding.syntax.is_none());
        }
        assert!(binding.terminal.is_none());
    }
    for &(owner, start, end) in oracle.copies {
        let value = read(
            &map.values,
            bindings[owner].value.unwrap().get() as usize - 1,
            host,
        );
        copy_value(value, raw(buffer, start, end));
    }
    // No Escape/Code/Entity proof can hide in the suffix or the destination.
    assert_eq!(map.proofs.len(), usize::from(oracle.soft.is_some()));
    if let Some((owner, start, end)) = oracle.soft {
        let value = read(
            &map.values,
            bindings[owner].value.unwrap().get() as usize - 1,
            host,
        );
        assert_eq!(value.bytes, 1);
        assert_eq!(value.atoms.len(), 1);
        let atom = &value.atoms[0];
        assert_eq!(atom.source, raw(buffer, start, end));
        assert_eq!(atom.bytes, 1);
        assert!(!atom.hidden_syntax);
        let proof = read(&map.proofs, atom.proof.expect("SoftBreak proof"), host);
        assert_eq!(proof.kind, TransformKind::SoftBreakToSpace);
        assert_eq!(proof.stage, 1);
        assert_eq!((proof.local_start, proof.local_end), (0, end - start));
        assert_eq!(proof.result, " ");
        assert_eq!(proof.input.len(), 1);
        copy_atom(&proof.input[0], raw(buffer, start, end));
        assert_eq!(proof.host.role, HostRole::Inline);
        assert_eq!(proof.host.phase, NativePhase::Inline);
        assert_eq!(proof.host.payload_role, None);
        assert_eq!(proof.host.utf8_len, oracle.raw_len);
        assert_eq!(
            (proof.host.consumed_start, proof.host.consumed_end),
            (start, end)
        );
        match proof.host.source {
            ClaimSource::R(span) => assert_eq!(span, raw(buffer, 0, oracle.raw_len)),
            ClaimSource::P(_) => panic!("SoftBreak host must retain the actual R domain"),
        }
    }
    // Exact multiset, not a permissive subset: no Break/backslash marker or
    // surprise claim is allowed. Stored order itself must be strictly monotone.
    assert_eq!(map.claims.len(), oracle.claims.len());
    let mut matched = vec![false; oracle.claims.len()];
    let mut previous = None;
    for i in 0..map.claims.len() {
        let actual = read(&map.claims, i, host);
        if let Some(order) = previous {
            assert!(order < actual.order);
        }
        previous = Some(actual.order);
        let candidate = oracle
            .claims
            .iter()
            .enumerate()
            .position(|(j, expected)| {
                !matched[j]
                    && actual.owner == ids[expected.owner]
                    && actual.kind == expected.kind
                    && match (expected.domain, actual.source) {
                        (Domain::P, ClaimSource::P(p)) => {
                            (p.start, p.end) == (expected.start, expected.end)
                        }
                        (Domain::R, ClaimSource::R(r)) => {
                            r == raw(buffer, expected.start, expected.end)
                        }
                        _ => false,
                    }
            })
            .expect("unexpected claim source/kind/owner");
        matched[candidate] = true;
        let expected = &oracle.claims[candidate];
        let (len, role, phase) = match expected.domain {
            Domain::P => (oracle.source.len(), HostRole::Block, NativePhase::Block),
            Domain::R => (oracle.raw_len, HostRole::Inline, NativePhase::Inline),
        };
        assert_eq!(actual.host_len, len);
        assert_eq!(actual.role, role);
        assert_eq!(actual.phase, phase);
        let host_snapshot = actual.host();
        assert_eq!(host_snapshot.payload_role, None);
        assert_eq!(
            (host_snapshot.consumed_start, host_snapshot.consumed_end),
            (expected.start, expected.end)
        );
        match (expected.domain, host_snapshot.source) {
            (Domain::P, ClaimSource::P(p)) => assert_eq!(p, PSpan { start: 0, end: len }),
            (Domain::R, ClaimSource::R(r)) => assert_eq!(r, raw(buffer, 0, len)),
            _ => panic!("claim host changed domains"),
        }
    }
    assert!(matched.iter().all(|m| *m));
}

fn run(oracle: Oracle<'_>) {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.autolink = false;
    let mut host = TestHost::default();
    let book = host.book.clone();
    {
        let mut ctx = Context::new_direct(&mut host, oracle.source);
        ctx.inline_policy = InlinePolicy::StrictG4;
        parse_in_context(oracle.source, &options, &mut ctx, |root, ctx| {
            verify(root, ctx, &oracle);
            assert!(!book.borrow().live.is_empty());
            Ok(())
        })
        .unwrap();
        // The parser arena is gone, but accepted mappings still own permits.
        assert!(!book.borrow().live.is_empty());
    }
    assert!(host.first.is_none());
    let book = book.borrow();
    assert!(book.next > 0);
    assert!(book.live.is_empty());
    assert_eq!(book.dropped, book.next, "every issued permit dropped once");
}

#[test]
fn raw_backslash_belongs_to_www_and_html_starts_in_the_next_gap() {
    run(Oracle {
        source: "www.a.com/\\<b>",
        nodes: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Paragraph),
            node(Some(1), Kind::Link("http://www.a.com/\\")),
            node(Some(2), Kind::Text("www.a.com/\\")),
            node(Some(1), Kind::Html("<b>")),
        ],
        paragraph: 1,
        raw_len: 14,
        pieces: &[(0, 14, 0, 14)],
        link: 2,
        label: 3,
        selection: (0, 11),
        extended: ExtendedKind::Www,
        copies: &[(3, 0, 11), (4, 11, 14)],
        soft: None,
        claims: &[claim(2, ClaimKind::Link, Domain::R, 0, 11)],
    });
}

#[test]
fn trimmed_star_closes_the_original_whole_leaf_delimiter_stack() {
    run(Oracle {
        source: "*www.a.com/\\*",
        nodes: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Paragraph),
            node(Some(1), Kind::Emph),
            node(Some(2), Kind::Link("http://www.a.com/\\")),
            node(Some(3), Kind::Text("www.a.com/\\")),
        ],
        paragraph: 1,
        raw_len: 13,
        pieces: &[(0, 13, 0, 13)],
        link: 3,
        label: 4,
        selection: (1, 12),
        extended: ExtendedKind::Www,
        copies: &[(4, 1, 12)],
        soft: None,
        claims: &[
            claim(3, ClaimKind::Link, Domain::R, 1, 12),
            claim(2, ClaimKind::Delimiter, Domain::R, 0, 1),
            claim(2, ClaimKind::Delimiter, Domain::R, 12, 13),
        ],
    });
}

#[test]
fn gap_lf_is_soft_and_does_not_reconsume_the_selected_backslash() {
    run(Oracle {
        source: "www.a.com/\\\nz",
        nodes: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Paragraph),
            node(Some(1), Kind::Link("http://www.a.com/\\")),
            node(Some(2), Kind::Text("www.a.com/\\")),
            node(Some(1), Kind::SoftBreak),
            node(Some(1), Kind::Text("z")),
        ],
        paragraph: 1,
        raw_len: 13,
        pieces: &[(0, 12, 0, 12), (12, 13, 12, 13)],
        link: 2,
        label: 3,
        selection: (0, 11),
        extended: ExtendedKind::Www,
        copies: &[(3, 0, 11), (5, 12, 13)],
        soft: Some((4, 11, 12)),
        claims: &[claim(2, ClaimKind::Link, Domain::R, 0, 11)],
    });
}

#[test]
fn raw_crlf_in_quote_preserves_two_copy_pieces_and_the_container_hole() {
    // P: prefix0:2, label2:13, CRLF13:15, prefix15:17, z17:18.
    // R: label0:11, CRLF11:13, z13:14. SoftBreak consumes only CRLF.
    run(Oracle {
        source: "> www.a.com/\\\r\n> z",
        nodes: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Quote),
            node(Some(1), Kind::Paragraph),
            node(Some(2), Kind::Link("http://www.a.com/\\")),
            node(Some(3), Kind::Text("www.a.com/\\")),
            node(Some(2), Kind::SoftBreak),
            node(Some(2), Kind::Text("z")),
        ],
        paragraph: 2,
        raw_len: 14,
        pieces: &[(0, 13, 2, 15), (13, 14, 17, 18)],
        link: 3,
        label: 4,
        selection: (0, 11),
        extended: ExtendedKind::Www,
        copies: &[(4, 0, 11), (6, 13, 14)],
        soft: Some((5, 11, 13)),
        claims: &[
            claim(
                1,
                ClaimKind::Syntax(SyntaxRole::QuotePrefix),
                Domain::P,
                0,
                2,
            ),
            claim(
                1,
                ClaimKind::Syntax(SyntaxRole::QuotePrefix),
                Domain::P,
                15,
                17,
            ),
            claim(3, ClaimKind::Link, Domain::R, 0, 11),
        ],
    });
}

#[test]
fn www_has_typed_http_but_case_insensitive_url_keeps_its_raw_destination() {
    run(Oracle {
        source: "www.a.com",
        nodes: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Paragraph),
            node(Some(1), Kind::Link("http://www.a.com")),
            node(Some(2), Kind::Text("www.a.com")),
        ],
        paragraph: 1,
        raw_len: 9,
        pieces: &[(0, 9, 0, 9)],
        link: 2,
        label: 3,
        selection: (0, 9),
        extended: ExtendedKind::Www,
        copies: &[(3, 0, 9)],
        soft: None,
        claims: &[claim(2, ClaimKind::Link, Domain::R, 0, 9)],
    });
    run(Oracle {
        source: "HTTPS://a.b",
        nodes: &[
            node(None, Kind::Document),
            node(Some(0), Kind::Paragraph),
            node(Some(1), Kind::Link("HTTPS://a.b")),
            node(Some(2), Kind::Text("HTTPS://a.b")),
        ],
        paragraph: 1,
        raw_len: 11,
        pieces: &[(0, 11, 0, 11)],
        link: 2,
        label: 3,
        selection: (0, 11),
        extended: ExtendedKind::Url,
        copies: &[(3, 0, 11)],
        soft: None,
        claims: &[claim(2, ClaimKind::Link, Domain::R, 0, 11)],
    });
}
