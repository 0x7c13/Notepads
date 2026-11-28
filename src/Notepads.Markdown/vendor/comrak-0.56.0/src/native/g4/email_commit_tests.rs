// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Prewritten real-parser email commit assertions; author has not executed them.
//! Exact tree/value oracles precede binding real native identities. Decode proof
//! expectations derive from frozen Q3/Q4 and literal spelling, never a replay.
//! This is the live parse_in_context closure, not Core FinalGraph/Q-model/query
//! acceptance. The fixture Host below is copied from commit_tests, not production.
//! Its permit identities/Drop prove lease zero, not observed physical allocator
//! frees, cancellation coverage, or a performance result. Observer Vec/Set memory
//! is test fixture memory. G3 cases verify email/delimiter claims and exact cell/
//! paragraph transport; their other block claims remain covered by existing G3
//! tests, rather than being silently labeled complete G3 model coverage here.

use crate::native::{
    AcceptedFact, AppendKind, BufferId, Context, DecodeKind, GeneratedPrefix, Host, InlinePolicy,
    NativeAdmission, NativeNodeId, NativeViolation, PSpan, PayloadKind, RSpan, WorkDelta,
    g4::{ExtendedKind, test_values},
    mapped::{Atom, ClaimKind, ClaimSource, HostRole, MappedValue, NativePhase, TransformKind},
    rows::{NativeRows, charge_host},
};
use crate::{
    Options,
    nodes::{ListType, Node, NodeValue, TableAlignment},
    parser::parse_in_context,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

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
enum K<'a> {
    Doc,
    Para,
    Emph,
    Strike,
    List,
    Item,
    Checkbox,
    Table,
    Header,
    Cell,
    Text(&'a str),
    Link(&'a str),
    Image(&'a str),
    Code(&'a str),
    Html(&'a str),
    Soft,
    Hard,
}
struct N<'a> {
    parent: Option<usize>,
    kind: K<'a>,
}
const fn n(parent: Option<usize>, kind: K<'_>) -> N<'_> {
    N { parent, kind }
}
struct A<'a> {
    start: usize,
    end: usize,
    text: &'a str,
    decode: Option<DecodeKind>,
}
const fn c(start: usize, end: usize, text: &str) -> A<'_> {
    A {
        start,
        end,
        text,
        decode: None,
    }
}
const fn t(start: usize, end: usize, text: &str, decode: DecodeKind) -> A<'_> {
    A {
        start,
        end,
        text,
        decode: Some(decode),
    }
}
struct M<'a> {
    node: usize,
    atoms: &'a [A<'a>],
}
struct E {
    link: usize,
    label: usize,
    start: usize,
    end: usize,
}
struct Case<'a> {
    source: &'a str,
    tree: &'a [N<'a>],
    block: usize,
    raw_len: usize,
    maps: &'a [M<'a>],
    emails: &'a [E],
    proofs: usize,
    payloads: usize,
    // If true the only claims are exactly the email Link claims and these
    // delimiter claims. False is explicit limited coverage of existing syntax.
    all_claims: bool,
    delimiters: &'a [(usize, usize, usize)],
    pieces: &'a [(usize, usize, usize, usize, AppendKind)],
    table_pipes: usize,
}
fn row<'a, T>(rows: &'a NativeRows<T, Permit>, at: usize, host: &mut TestHost) -> &'a T {
    rows.get(at, |bytes| charge_host(host, bytes))
        .unwrap()
        .unwrap()
}
fn walk<'a>(node: Node<'a>, output: &mut Vec<Node<'a>>) {
    output.push(node);
    for child in node.children() {
        walk(child, output);
    }
}
fn r(buffer: BufferId, start: usize, end: usize) -> RSpan {
    RSpan { buffer, start, end }
}
fn verify_atoms(
    value: &MappedValue,
    expected: &[A<'_>],
    buffer: BufferId,
    raw_len: usize,
    proofs: &NativeRows<crate::native::mapped::Proof, Permit>,
    host: &mut TestHost,
) -> Vec<Option<usize>> {
    assert_eq!(value.atoms.len(), expected.len());
    assert_eq!(
        value.bytes,
        expected.iter().map(|a| a.text.len()).sum::<usize>()
    );
    let mut identities = Vec::new();
    for (atom, expected) in value.atoms.iter().zip(expected) {
        let source = r(buffer, expected.start, expected.end);
        assert_eq!(atom.source, source);
        assert_eq!(atom.bytes, expected.text.len());
        assert!(!atom.hidden_syntax);
        identities.push(atom.proof);
        match expected.decode {
            None => {
                assert_eq!(atom.proof, None);
                assert_eq!(expected.end - expected.start, expected.text.len());
            }
            Some(kind) => {
                let proof = row(
                    proofs,
                    atom.proof.expect("original accepted decoder proof"),
                    host,
                );
                assert_eq!(proof.kind, TransformKind::Decode(kind));
                assert_eq!(proof.stage, 1);
                assert_eq!(proof.result, expected.text);
                assert_eq!(
                    (proof.local_start, proof.local_end),
                    (0, expected.end - expected.start)
                );
                assert_eq!(
                    proof.input.len(),
                    1,
                    "minimal birth, not a reconstructed wide proof"
                );
                let input: &Atom = &proof.input[0];
                assert_eq!(input.source, source);
                assert_eq!(input.bytes, expected.end - expected.start);
                assert_eq!(input.proof, None);
                assert!(!input.hidden_syntax);
                assert_eq!(proof.host.role, HostRole::Inline);
                assert_eq!(proof.host.phase, NativePhase::Inline);
                assert_eq!(proof.host.payload_role, None);
                assert_eq!(proof.host.utf8_len, raw_len);
                assert_eq!(
                    (proof.host.consumed_start, proof.host.consumed_end),
                    (expected.start, expected.end)
                );
                match proof.host.source {
                    ClaimSource::R(source) => assert_eq!(source, r(buffer, 0, raw_len)),
                    ClaimSource::P(_) => panic!("decoder's original R host lost"),
                }
            }
        }
    }
    identities
}
fn verify(root: Node<'_>, ctx: &mut Context<'_, TestHost>, case: &Case<'_>) {
    let mut nodes = Vec::new();
    walk(root, &mut nodes);
    assert_eq!(nodes.len(), case.tree.len(), "complete final tree");
    for (index, expected) in case.tree.iter().enumerate() {
        match expected.parent {
            Some(parent) => assert!(nodes[index].parent().unwrap().same_node(nodes[parent])),
            None => assert!(nodes[index].parent().is_none()),
        }
        let data = nodes[index].data();
        match (expected.kind, &data.value) {
            (K::Doc, NodeValue::Document)
            | (K::Para, NodeValue::Paragraph)
            | (K::Emph, NodeValue::Emph)
            | (K::Strike, NodeValue::Strikethrough)
            | (K::Item, NodeValue::Item(_))
            | (K::Cell, NodeValue::TableCell)
            | (K::Soft, NodeValue::SoftBreak)
            | (K::Hard, NodeValue::LineBreak) => {}
            (K::Text(text), NodeValue::Text(actual)) => {
                assert_eq!(test_values::text(ctx, actual).unwrap(), text)
            }
            (K::Html(text), NodeValue::HtmlInline(actual)) => {
                assert_eq!(test_values::text(ctx, actual).unwrap(), text)
            }
            (K::Link(url), NodeValue::Link(actual)) | (K::Image(url), NodeValue::Image(actual)) => {
                assert_eq!(test_values::text(ctx, &actual.url).unwrap(), url);
                assert!(test_values::text(ctx, &actual.title).unwrap().is_empty());
            }
            (K::Code(text), NodeValue::Code(actual)) => {
                assert_eq!(test_values::text(ctx, &actual.literal).unwrap(), text);
                assert_eq!(actual.num_backticks, 1);
            }
            (K::List, NodeValue::List(actual)) => {
                assert_eq!(actual.list_type, ListType::Bullet);
                assert!(actual.tight);
                assert!(actual.is_task_list);
            }
            (K::Checkbox, NodeValue::TaskCheckbox(actual)) => assert!(!actual.checked),
            (K::Table, NodeValue::Table(actual)) => {
                assert_eq!(actual.num_columns, 1);
                assert_eq!(actual.num_rows, 1);
                assert_eq!(actual.num_nonempty_cells, 1);
                assert_eq!(actual.alignments, vec![TableAlignment::None]);
            }
            (K::Header, NodeValue::TableRow(header)) => assert!(*header),
            _ => panic!("unexpected final node {index}: {:?}", data.value),
        }
    }
    // All values/edges pass before any actual node, buffer or payload ID binds.
    let ids: Vec<NativeNodeId> = nodes
        .iter()
        .map(|node| node.data().native_id.unwrap())
        .collect();
    let map = &ctx.mappings;
    let host = &mut *ctx.host;
    let bindings: Vec<_> = ids
        .iter()
        .map(|id| *row(&map.nodes, id.get() as usize - 1, host))
        .collect();
    let block = row(
        &map.blocks,
        bindings[case.block].block.unwrap().get() as usize - 1,
        host,
    );
    let buffer = block.buffer.unwrap();
    let stored = row(&map.buffers, buffer.get() as usize - 1, host);
    assert_eq!(stored.owner, ids[case.block]);
    assert!(stored.sealed);
    assert_eq!(stored.len, case.raw_len);
    if !case.pieces.is_empty() {
        assert_eq!(stored.pieces.len(), case.pieces.len());
        for (piece, &(rs, re, ps, pe, kind)) in stored.pieces.iter().zip(case.pieces) {
            assert_eq!(piece.r, r(buffer, rs, re));
            assert_eq!(piece.p, PSpan { start: ps, end: pe });
            assert_eq!(piece.kind, kind);
        }
    }
    assert_eq!(map.table_pipes.len(), case.table_pipes);
    assert_eq!(
        map.proofs.len(),
        case.proofs,
        "email must not run another decoder"
    );
    assert_eq!(map.payloads.len(), case.payloads);
    assert_eq!(ctx.definitions.len(), 0);
    let mut proof_births = BTreeSet::new();
    let mut expected_births = 0;
    for expected in case.maps {
        let value = row(
            &map.values,
            bindings[expected.node].value.unwrap().get() as usize - 1,
            host,
        );
        for (identity, atom) in verify_atoms(
            value,
            expected.atoms,
            buffer,
            case.raw_len,
            &map.proofs,
            host,
        )
        .into_iter()
        .zip(expected.atoms)
        {
            if atom.decode.is_some() {
                expected_births += 1;
                assert!(proof_births.insert(identity.unwrap()));
            }
        }
    }
    assert_eq!(proof_births.len(), expected_births);
    let mut extended_count = 0;
    for at in 0..map.syntax.len() {
        if let Some(extended) = row(&map.syntax, at, host).extended {
            assert_eq!(extended.kind, ExtendedKind::Email);
            assert!(case.emails.iter().any(|e| ids[e.link] == extended.node));
            extended_count += 1;
        }
    }
    assert_eq!(extended_count, case.emails.len());
    let mut prefix_count = 0;
    for at in 0..map.payloads.len() {
        let payload = row(&map.payloads, at, host);
        if payload.prefix.is_some() {
            assert_eq!(payload.prefix_kind, GeneratedPrefix::Mailto);
            prefix_count += 1;
        }
    }
    assert_eq!(
        prefix_count,
        case.emails.len(),
        "no orphan or duplicate generated prefix"
    );
    for email in case.emails {
        let source = r(buffer, email.start, email.end);
        let syntax = row(
            &map.syntax,
            bindings[email.link].syntax.unwrap().get() as usize - 1,
            host,
        );
        assert_eq!(syntax.syntax, Some(source));
        assert_eq!(syntax.label, Some(source));
        assert_eq!(syntax.title, None);
        assert_eq!(syntax.definition, None);
        assert!(!syntax.reference_use);
        let extended = syntax.extended.unwrap();
        assert_eq!(extended.node, ids[email.link]);
        assert_eq!(extended.label, ids[email.label]);
        assert_eq!(extended.raw, source);
        assert_eq!(extended.kind, ExtendedKind::Email);
        assert_eq!(syntax.destination, Some(extended.destination));
        let payload = row(&map.payloads, extended.destination.get() as usize - 1, host);
        assert_eq!(payload.role, PayloadKind::Destination);
        assert_eq!(payload.raw, source);
        assert_eq!(payload.stage, 0);
        let expected = case.maps.iter().find(|m| m.node == email.label).unwrap();
        let label = row(
            &map.values,
            bindings[email.label].value.unwrap().get() as usize - 1,
            host,
        );
        assert_eq!(payload.stage_len, label.bytes);
        let destination_ids = verify_atoms(
            &payload.value,
            expected.atoms,
            buffer,
            case.raw_len,
            &map.proofs,
            host,
        );
        assert_eq!(
            destination_ids,
            label.atoms.iter().map(|a| a.proof).collect::<Vec<_>>(),
            "same original proof IDs, no replacement proof"
        );
        for (a, b) in label.atoms.iter().zip(payload.value.atoms.iter()) {
            assert_eq!(a.source, b.source);
            assert_eq!(a.bytes, b.bytes);
            assert_eq!(a.proof, b.proof);
            assert_eq!(a.hidden_syntax, b.hidden_syntax);
        }
        let (anchor, text) = payload.prefix.as_ref().unwrap();
        assert_eq!(payload.prefix_kind, GeneratedPrefix::Mailto);
        assert_eq!(*anchor, r(buffer, email.start, email.start));
        assert_eq!(text, "mailto:");
        assert!(
            bindings[email.label].syntax.is_none(),
            "label must not carry a second Link authority"
        );
    }
    let mut found_email = vec![0usize; case.emails.len()];
    let mut found_delimiter = vec![0usize; case.delimiters.len()];
    let mut previous = None;
    for at in 0..map.claims.len() {
        let claim = row(&map.claims, at, host);
        if let Some(order) = previous {
            assert!(order < claim.order);
        }
        previous = Some(claim.order);
        let expected = if let Some((index, email)) = case
            .emails
            .iter()
            .enumerate()
            .find(|(_, e)| ids[e.link] == claim.owner)
        {
            assert_eq!(claim.kind, ClaimKind::Link);
            found_email[index] += 1;
            Some((email.start, email.end))
        } else if claim.kind == ClaimKind::Delimiter {
            let (index, &(_, start, end)) = case
                .delimiters
                .iter()
                .enumerate()
                .find(|(_, (owner, start, end))| {
                    claim.owner == ids[*owner]
                        && matches!(claim.source, ClaimSource::R(s) if s == r(buffer, *start, *end))
                })
                .expect("unexpected delimiter claim");
            found_delimiter[index] += 1;
            Some((start, end))
        } else {
            assert!(!case.all_claims, "unexpected extra claim");
            None
        };
        if let Some((start, end)) = expected {
            match claim.source {
                ClaimSource::R(s) => assert_eq!(s, r(buffer, start, end)),
                _ => panic!("wrong email/delimiter domain"),
            }
            assert_eq!(claim.role, HostRole::Inline);
            assert_eq!(claim.phase, NativePhase::Inline);
            assert_eq!(claim.host_len, case.raw_len);
            let domain = claim.host();
            assert_eq!(domain.payload_role, None);
            assert_eq!((domain.consumed_start, domain.consumed_end), (start, end));
            match domain.source {
                ClaimSource::R(s) => assert_eq!(s, r(buffer, 0, case.raw_len)),
                _ => panic!("wrong host"),
            }
        }
    }
    assert!(found_email.iter().all(|&n| n == 1));
    assert!(found_delimiter.iter().all(|&n| n == 1));
    if case.all_claims {
        assert_eq!(map.claims.len(), case.emails.len() + case.delimiters.len());
    }
}
fn run(case: Case<'_>) {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = false;
    let mut host = TestHost::default();
    let book = host.book.clone();
    {
        let mut ctx = Context::new_direct(&mut host, case.source);
        ctx.inline_policy = InlinePolicy::StrictG4;
        parse_in_context(case.source, &options, &mut ctx, |root, ctx| {
            verify(root, ctx, &case);
            Ok(())
        })
        .unwrap();
        assert!(!book.borrow().live.is_empty());
    }
    assert!(host.first.is_none());
    let book = book.borrow();
    assert!(book.next > 0);
    assert!(book.live.is_empty());
    assert_eq!(book.dropped, book.next);
}

#[test]
fn q3_q4_escape_and_two_entities_keep_separate_original_proofs() {
    for (source, label, url, atoms, proofs) in [
        (
            "a&#64;b.com",
            "a@b.com",
            "mailto:a@b.com",
            vec![
                c(0, 1, "a"),
                t(1, 6, "@", DecodeKind::Entity),
                c(6, 11, "b.com"),
            ],
            1,
        ),
        (
            "&fjlig;@a.com",
            "fj@a.com",
            "mailto:fj@a.com",
            vec![t(0, 7, "fj", DecodeKind::Entity), c(7, 13, "@a.com")],
            1,
        ),
        (
            "a\\_b@c.com",
            "a_b@c.com",
            "mailto:a_b@c.com",
            vec![
                c(0, 1, "a"),
                t(1, 3, "_", DecodeKind::Escape),
                c(3, 10, "b@c.com"),
            ],
            1,
        ),
        (
            "a&#95;&#98;@c.com",
            "a_b@c.com",
            "mailto:a_b@c.com",
            vec![
                c(0, 1, "a"),
                t(1, 6, "_", DecodeKind::Entity),
                t(6, 11, "b", DecodeKind::Entity),
                c(11, 17, "@c.com"),
            ],
            2,
        ),
    ] {
        run(Case {
            source,
            tree: &[
                n(None, K::Doc),
                n(Some(0), K::Para),
                n(Some(1), K::Link(url)),
                n(Some(2), K::Text(label)),
            ],
            block: 1,
            raw_len: source.len(),
            maps: &[M {
                node: 3,
                atoms: &atoms,
            }],
            emails: &[E {
                link: 2,
                label: 3,
                start: 0,
                end: source.len(),
            }],
            proofs,
            payloads: 1,
            all_claims: true,
            delimiters: &[],
            pieces: &[(0, source.len(), 0, source.len(), AppendKind::Copy)],
            table_pipes: 0,
        });
    }
}

#[test]
fn repeated_at_and_two_matches_preserve_literal_suffix_and_middle() {
    run(Case {
        source: "a@b.com@c.com",
        tree: &[
            n(None, K::Doc),
            n(Some(0), K::Para),
            n(Some(1), K::Link("mailto:a@b.com")),
            n(Some(2), K::Text("a@b.com")),
            n(Some(1), K::Text("@c.com")),
        ],
        block: 1,
        raw_len: 13,
        maps: &[
            M {
                node: 3,
                atoms: &[c(0, 7, "a@b.com")],
            },
            M {
                node: 4,
                atoms: &[c(7, 13, "@c.com")],
            },
        ],
        emails: &[E {
            link: 2,
            label: 3,
            start: 0,
            end: 7,
        }],
        proofs: 0,
        payloads: 1,
        all_claims: true,
        delimiters: &[],
        pieces: &[(0, 13, 0, 13, AppendKind::Copy)],
        table_pipes: 0,
    });
    run(Case {
        source: "a@b.com z@c.net",
        tree: &[
            n(None, K::Doc),
            n(Some(0), K::Para),
            n(Some(1), K::Link("mailto:a@b.com")),
            n(Some(2), K::Text("a@b.com")),
            n(Some(1), K::Text(" ")),
            n(Some(1), K::Link("mailto:z@c.net")),
            n(Some(5), K::Text("z@c.net")),
        ],
        block: 1,
        raw_len: 15,
        maps: &[
            M {
                node: 3,
                atoms: &[c(0, 7, "a@b.com")],
            },
            M {
                node: 4,
                atoms: &[c(7, 8, " ")],
            },
            M {
                node: 6,
                atoms: &[c(8, 15, "z@c.net")],
            },
        ],
        emails: &[
            E {
                link: 2,
                label: 3,
                start: 0,
                end: 7,
            },
            E {
                link: 5,
                label: 6,
                start: 8,
                end: 15,
            },
        ],
        proofs: 0,
        payloads: 2,
        all_claims: true,
        delimiters: &[],
        pieces: &[(0, 15, 0, 15, AppendKind::Copy)],
        table_pipes: 0,
    });
}

#[test]
fn emphasis_allows_its_own_run_but_never_joins_across_structure() {
    run(Case {
        source: "*a@b.com*",
        tree: &[
            n(None, K::Doc),
            n(Some(0), K::Para),
            n(Some(1), K::Emph),
            n(Some(2), K::Link("mailto:a@b.com")),
            n(Some(3), K::Text("a@b.com")),
        ],
        block: 1,
        raw_len: 9,
        maps: &[M {
            node: 4,
            atoms: &[c(1, 8, "a@b.com")],
        }],
        emails: &[E {
            link: 3,
            label: 4,
            start: 1,
            end: 8,
        }],
        proofs: 0,
        payloads: 1,
        all_claims: true,
        delimiters: &[(2, 0, 1), (2, 8, 9)],
        pieces: &[(0, 9, 0, 9, AppendKind::Copy)],
        table_pipes: 0,
    });
    run(Case {
        source: "a*b*@c.com",
        tree: &[
            n(None, K::Doc),
            n(Some(0), K::Para),
            n(Some(1), K::Text("a")),
            n(Some(1), K::Emph),
            n(Some(3), K::Text("b")),
            n(Some(1), K::Text("@c.com")),
        ],
        block: 1,
        raw_len: 10,
        maps: &[
            M {
                node: 2,
                atoms: &[c(0, 1, "a")],
            },
            M {
                node: 4,
                atoms: &[c(2, 3, "b")],
            },
            M {
                node: 5,
                atoms: &[c(4, 10, "@c.com")],
            },
        ],
        emails: &[],
        proofs: 0,
        payloads: 0,
        all_claims: true,
        delimiters: &[(3, 1, 2), (3, 3, 4)],
        pieces: &[(0, 10, 0, 10, AppendKind::Copy)],
        table_pipes: 0,
    });
}

#[test]
fn existing_link_image_code_html_and_break_are_email_barriers() {
    for (source, children, proofs, payloads) in [
        (
            "[a@b.com](/u)",
            vec![n(Some(1), K::Link("/u")), n(Some(2), K::Text("a@b.com"))],
            0,
            1,
        ),
        (
            "![a@b.com](/u)",
            vec![n(Some(1), K::Image("/u")), n(Some(2), K::Text("a@b.com"))],
            0,
            1,
        ),
        ("`a@b.com`", vec![n(Some(1), K::Code("a@b.com"))], 0, 0),
        (
            "x <i a='a@b.com'> y",
            vec![
                n(Some(1), K::Text("x ")),
                n(Some(1), K::Html("<i a='a@b.com'>")),
                n(Some(1), K::Text(" y")),
            ],
            0,
            0,
        ),
        (
            "a@\nb.com",
            vec![
                n(Some(1), K::Text("a@")),
                n(Some(1), K::Soft),
                n(Some(1), K::Text("b.com")),
            ],
            1,
            0,
        ),
        (
            "a@\\\nb.com",
            vec![
                n(Some(1), K::Text("a@")),
                n(Some(1), K::Hard),
                n(Some(1), K::Text("b.com")),
            ],
            0,
            0,
        ),
    ] {
        let mut tree = vec![n(None, K::Doc), n(Some(0), K::Para)];
        tree.extend(children);
        run(Case {
            source,
            tree: &tree,
            block: 1,
            raw_len: source.len(),
            maps: &[],
            emails: &[],
            proofs,
            payloads,
            all_claims: false,
            delimiters: &[],
            pieces: &[],
            table_pipes: 0,
        });
    }
}

#[test]
fn table_pipe_transport_stays_with_cell_and_does_not_enter_email() {
    // P header: outer0, space1, x2, backslash3/pipe4, email5:12,
    // space12, outer13, LF14; delimiter row15:22. Cell D/R = x|a@b.com.
    run(Case {
        source: "| x\\|a@b.com |\n| --- |",
        tree: &[
            n(None, K::Doc),
            n(Some(0), K::Table),
            n(Some(1), K::Header),
            n(Some(2), K::Cell),
            n(Some(3), K::Text("x|")),
            n(Some(3), K::Link("mailto:a@b.com")),
            n(Some(5), K::Text("a@b.com")),
        ],
        block: 3,
        raw_len: 9,
        maps: &[
            M {
                node: 4,
                atoms: &[c(0, 2, "x|")],
            },
            M {
                node: 6,
                atoms: &[c(2, 9, "a@b.com")],
            },
        ],
        emails: &[E {
            link: 5,
            label: 6,
            start: 2,
            end: 9,
        }],
        proofs: 0,
        payloads: 1,
        all_claims: false,
        delimiters: &[],
        pieces: &[
            (0, 1, 2, 3, AppendKind::Copy),
            (1, 2, 3, 5, AppendKind::TableEscapedPipe),
            (2, 9, 5, 12, AppendKind::Copy),
        ],
        table_pipes: 1,
    });
}

#[test]
fn task_then_strike_preserves_original_nonzero_paragraph_base() {
    // P '- '0:2, task2:6, strike6:8/email8:15/strike15:17.
    // Paragraph buffer retains R0:15, task consumes R0:4, label R6:13.
    run(Case {
        source: "- [ ] ~~a@b.com~~",
        tree: &[
            n(None, K::Doc),
            n(Some(0), K::List),
            n(Some(1), K::Item),
            n(Some(2), K::Checkbox),
            n(Some(2), K::Para),
            n(Some(4), K::Strike),
            n(Some(5), K::Link("mailto:a@b.com")),
            n(Some(6), K::Text("a@b.com")),
        ],
        block: 4,
        raw_len: 15,
        maps: &[M {
            node: 7,
            atoms: &[c(6, 13, "a@b.com")],
        }],
        emails: &[E {
            link: 6,
            label: 7,
            start: 6,
            end: 13,
        }],
        proofs: 0,
        payloads: 1,
        all_claims: false,
        delimiters: &[(5, 4, 6), (5, 13, 15)],
        pieces: &[(0, 15, 2, 17, AppendKind::Copy)],
        table_pipes: 0,
    });
}
