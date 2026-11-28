// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::native::{AcceptedFact, InlineHtmlPolicy, InlinePolicy, NativeAdmission};
use std::cell::Cell;
use std::rc::Rc;

#[path = "g3_failure_tests.rs"]
mod g3_failure_tests;

#[path = "review_regression_tests.rs"]
mod review_regression_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Error {
    Invalid,
    Raw,
    Cancelled,
    OldTransport,
}

#[derive(Debug)]
struct Lease(Rc<Cell<usize>>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

struct TestHost<'input> {
    brand: Invariant<'input>,
    fault: Option<Error>,
    live: Rc<Cell<usize>>,
    cancel_tail: bool,
    cancel_ast_exit: bool,
    old_transport: usize,
    checkpoints: usize,
    scanned: u64,
    copied: u64,
    events: u64,
}

impl<'input> TestHost<'input> {
    fn new(_: &NativeInput<'input>) -> Self {
        Self {
            brand: PhantomData,
            fault: None,
            live: Rc::new(Cell::new(0)),
            cancel_tail: false,
            cancel_ast_exit: false,
            old_transport: 0,
            checkpoints: 0,
            scanned: 0,
            copied: 0,
            events: 0,
        }
    }
    fn check(&mut self) -> Result<(), Error> {
        self.fault.map_or(Ok(()), Err)
    }
}

impl Host for TestHost<'_> {
    type Error = Error;
    type Lease = Lease;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Error> {
        self.check()?;
        match delta {
            WorkDelta::Scan(bytes) => self.scanned += bytes,
            WorkDelta::Copy(bytes) => self.copied += bytes,
            WorkDelta::Events(events) => self.events += events,
        }
        Ok(())
    }
    fn admit(&mut self, _: NativeAdmission) -> Result<(), Error> {
        self.check()
    }
    fn reserve(&mut self, _: u64) -> Result<Lease, Error> {
        self.check()?;
        self.live.set(self.live.get() + 1);
        Ok(Lease(Rc::clone(&self.live)))
    }
    fn admit_actual(&mut self, _: &mut Lease, _: u64) -> Result<(), Error> {
        self.check()
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Error> {
        self.old_transport += 1;
        Err(*self.fault.get_or_insert(Error::OldTransport))
    }
    fn reject(&mut self, _: NativeViolation) -> Error {
        *self.fault.get_or_insert(Error::Invalid)
    }
}

impl<'input> DirectHost<'input> for TestHost<'input> {
    type ReadBrand = Invariant<'input>;
    fn finish<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
        match result {
            Err(error) => Err(*self.fault.get_or_insert(error)),
            Ok(value) => self.fault.map_or(Ok(value), Err),
        }
    }
    fn checkpoint_now(&mut self) -> Result<(), Error> {
        self.checkpoints += 1;
        if self.checkpoints == 1 {
            // AST-only exit retains the real maps/proofs and their permits.
            assert!(self.live.get() > 0);
            if self.cancel_ast_exit {
                self.fault.get_or_insert(Error::Cancelled);
            }
        } else {
            // The original final tail still requires all actual native owners gone.
            assert_eq!(self.checkpoints, 2);
            assert_eq!(self.live.get(), 0);
            if self.cancel_tail {
                self.fault.get_or_insert(Error::Cancelled);
            }
        }
        self.check()
    }
}

fn text<'read, 'store, 'view, 'arena, 'input>(
    view: &mut AstReadAccess<'read, 'store, 'view, 'arena, 'input, TestHost<'input>, Lease>,
    value: ValueToken<'view, 'arena, 'input>,
) -> Result<String, Error> {
    let len = view.value_len(value)?;
    let mut cursor = view.value_cursor(value, 0, len)?;
    let mut result = String::new();
    while result.len() < len {
        let (_, next) = view.with_next_value_chunk(cursor, 4, |part, _| {
            assert!(!part.is_empty());
            result.push_str(part);
            Ok(())
        })?;
        cursor = next;
    }
    Ok(result)
}

#[test]
fn footnote_parser_shares_label_winners_and_keeps_original_source_claims() {
    fn check<'input>(input: NativeInput<'input>, source: &str, policy: InlinePolicy) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.footnotes = true;
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.tasklist = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let root = view.root()?;
            let paragraph = view.node(root)?.first_child.unwrap();
            let paragraph = view.node(paragraph)?;
            let winner = view.node(paragraph.next_sibling.unwrap())?;
            let duplicate = view.node(winner.next_sibling.unwrap())?;
            let url_label = view.node(duplicate.next_sibling.unwrap())?;
            assert_eq!(winner.kind, NativeKind::FootnoteDefinition);
            assert_eq!(duplicate.kind, NativeKind::FootnoteDefinition);
            assert_ne!(winner.id, duplicate.id);
            let first = view.node(winner.first_child.unwrap())?;
            assert!(first.next_sibling.is_some());
            let marker = source.find("[^éx]: Winning").unwrap();
            let lazy_line = source.find("Lazy duplicate continuation.").unwrap();
            let mut leading = None;
            let mut lazy = None;
            for index in 0..view.claim_count() {
                let claim = view.claim(index)?;
                if claim.owner.id == winner.id
                    && claim.kind == ClaimKind::Syntax(super::super::SyntaxRole::BlockWhitespace)
                {
                    let mut cursor = view.source_cursor(claim.source)?;
                    if let Some(piece) = view.next_source(&mut cursor)?
                        && piece.p.end == marker
                    {
                        leading = Some(piece.p.start);
                    }
                }
                if claim.owner.id == duplicate.id
                    && claim.kind == ClaimKind::Syntax(super::super::SyntaxRole::FootnoteContent)
                {
                    let mut cursor = view.source_cursor(claim.source)?;
                    while let Some(piece) = view.next_source(&mut cursor)? {
                        if piece.p.start == lazy_line {
                            lazy = Some(piece.p.end);
                        }
                    }
                }
            }
            assert_eq!(leading, Some(marker - 2));
            assert_eq!(
                lazy,
                Some(lazy_line + "Lazy duplicate continuation.\n".len())
            );
            let mut node = paragraph.first_child;
            let mut refs = Vec::new();
            let mut labels = String::new();
            let mut links = 0;
            let mut code = 0;
            let mut images = 0;
            while let Some(member) = node {
                let info = view.node(member)?;
                node = info.next_sibling;
                match info.kind {
                    NativeKind::FootnoteReference => {
                        let lineage = view.lineage(member)?;
                        let binding = view.binding(lineage)?;
                        let definition = view.binding(binding.footnote_definition.unwrap())?;
                        let mut cursor = view.source_cursor(binding.syntax.unwrap())?;
                        let mut syntax = String::new();
                        while let Some(piece) = view.next_source(&mut cursor)? {
                            syntax.push_str(&source[piece.p.start..piece.p.end]);
                        }
                        refs.push((syntax, definition.id));
                    }
                    NativeKind::Text => {
                        let NodeAttributes::Text(value) = info.attributes else {
                            unreachable!();
                        };
                        labels.push_str(&text(&mut view, value)?);
                    }
                    NativeKind::Link => links += 1,
                    NativeKind::InlineCode => code += 1,
                    NativeKind::Image => images += 1,
                    _ => {}
                }
            }
            assert_eq!(
                refs.iter()
                    .map(|(raw, id)| (raw.as_str(), *id))
                    .collect::<Vec<_>>(),
                [
                    ("[^Éx]", winner.id),
                    ("[^éX]", winner.id),
                    ("[^www.example.test]", url_label.id),
                ]
            );
            assert!(labels.contains("[^missing]"));
            assert!(labels.contains("again!"));
            let expected_links = if policy == InlinePolicy::Ordinary {
                1
            } else {
                2
            };
            assert_eq!(links, expected_links);
            assert_eq!(code, 1);
            assert_eq!(images, 1);
            Ok(())
        };
        compile_with_inline_policy(
            input,
            &options,
            &mut host,
            CallbackCompiler(Some(inspect)),
            policy,
        )
        .unwrap();
        assert_eq!(host.live.get(), 0);
    }
    let source = "First[^Éx] again![^éX] [^missing] `[^éx]` [^éx](/link) ![^éx](/asset) \
        [^www.example.test] https://outside.example.test.\n\
          \u{20}\u{20}[^éx]: Winning body.\n\n    Continuation.\n\n[^ÉX]: Duplicate body.\nLazy duplicate continuation.\n\n\
        [^www.example.test]: Opaque footnote label.\n";
    for policy in [InlinePolicy::Ordinary, InlinePolicy::StrictG4] {
        with_input(source, |input| check(input, source, policy));
    }
}

#[test]
fn mapped_inline_footnotes_preserve_delimiters_and_nested_parse() {
    fn check<'input>(input: NativeInput<'input>, source: &str, policy: InlinePolicy) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.footnotes = true;
        options.extension.inline_footnotes = true;
        options.extension.math_dollars = true;
        options.extension.math_latex = true;
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.tasklist = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let mut bodies = Vec::new();
            let mut links = 0;
            let mut images = 0;
            let mut named_references = 0;
            let mut stack = vec![view.root()?];
            while let Some(token) = stack.pop() {
                let node = view.node(token)?;
                if let Some(sibling) = node.next_sibling {
                    stack.push(sibling);
                }
                if let Some(child) = node.first_child {
                    stack.push(child);
                }
                match node.kind {
                    NativeKind::FootnoteReference => {
                        let lineage = view.lineage(token)?;
                        if let Some(note) = view.inline_footnote(lineage)? {
                            let binding = view.binding(lineage)?;
                            assert_eq!(view.binding(note.reference)?.id, binding.id);
                            assert_eq!(
                                view.binding(note.definition)?.id,
                                view.binding(binding.footnote_definition.unwrap())?.id
                            );
                            let mut extract = |token| {
                                let mut text = String::new();
                                let mut cursor = view.source_cursor(token)?;
                                while let Some(piece) = view.next_source(&mut cursor)? {
                                    text.push_str(&source[piece.p.start..piece.p.end]);
                                }
                                Ok(text)
                            };
                            assert_eq!(extract(note.opening)?, "^[");
                            assert_eq!(extract(note.closing)?, "]");
                            bodies.push(extract(note.body)?);
                        } else {
                            named_references += 1;
                        }
                    }
                    NativeKind::Link => links += 1,
                    NativeKind::Image => images += 1,
                    _ => {}
                }
            }
            assert_eq!(bodies.len(), 5, "{bodies:?}");
            assert!(bodies[0].contains("text \x60]\x60 and https://inside.example.test"));
            assert!(
                bodies[1].starts_with('a') && bodies[1].ends_with('b') && !bodies[1].contains('>')
            );
            assert!(bodies[2].contains("[x](u]v \"title]\")"));
            assert!(bodies[2].contains("<https://example.test/a]b>"));
            assert!(bodies[2].contains("<span title=\"]\">tag</span>"));
            assert!(bodies[2].ends_with("<code>a]b</code> $x]y$ \\(q]r\\) end"));
            assert_eq!(
                bodies[3],
                "![<code>a]b</code>](img) tail, then ![<code>early](img) chunk</code> finish"
            );
            assert_eq!(
                bodies[4],
                "![outer [inner <code>a](u](i)) extra]b</code>](u)](img) tail"
            );
            assert_eq!(images, 3);
            assert_eq!(named_references, 1);
            assert_eq!(
                links,
                if policy == InlinePolicy::StrictG4 {
                    6
                } else {
                    4
                }
            );
            Ok(())
        };
        compile_with_inline_html_policy(
            input,
            &options,
            &mut host,
            CallbackCompiler(Some(inspect)),
            policy,
            InlineHtmlPolicy::RawText,
        )
        .unwrap();
        assert_eq!(host.live.get(), 0);
    }
    let source = "One^[text \x60]\x60 and https://inside.example.test and [^known].] after https://outside.example.test.\r\n\r\n\
        > Two^[a\r\n> b] end.\r\n\r\nThird^[link [x](u]v \"title]\") angle <https://example.test/a]b> <span title=\"]\">tag</span> <code>a]b</code> $x]y$ \\(q]r\\) end] tail.\r\n\r\n\
        Fourth^[![<code>a]b</code>](img) tail, then ![<code>early](img) chunk</code> finish] after.\r\n\r\n\
        Fifth^[![outer [inner <code>a](u](i)) extra]b</code>](u)](img) tail] after.\r\n\r\n\
        \x60^[code]\x60 [label^[blocked]](/link) \\^[escaped] ^[] ^[ ] ^[&#32;] ^[&nbsp;] ^[unclosed\r\n\r\n\
        [^known]: Named.\r\n";
    for policy in [InlinePolicy::Ordinary, InlinePolicy::StrictG4] {
        with_input(source, |input| check(input, source, policy));
    }
}

#[test]
fn callout_markers_keep_quote_ancestry_and_authored_inline_titles() {
    use crate::native::{CalloutDisclosure, CalloutKind};
    let source = "> [!TiP] Read *标题*\r\n> Body.\r\n\r\n\
        > > [!FAQ]- [r]: /url\r\n> > Nested.\r\n\r\n\
        > [!IMPORTANT]\r\n> Untitled.\r\n\r\n\
        >    [!SUCCESS] Three columns after the quote prefix.\r\n> Wide marker.\r\n\r\n\
        > [!unknown] stays a quote.\r\n\r\n\
        >     [!NOTE] stays a code block.\r\n\r\n\
        >\t\t[!NOTE] stays code with tabs.\r\n";
    fn check<'input>(input: NativeInput<'input>, source: &str) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.alerts = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let mut stack = vec![view.root()?];
            let mut found = Vec::new();
            let mut code = 0;
            while let Some(token) = stack.pop() {
                let node = view.node(token)?;
                if let Some(sibling) = node.next_sibling {
                    stack.push(sibling);
                }
                if let Some(child) = node.first_child {
                    stack.push(child);
                }
                if node.kind == NativeKind::CodeBlock {
                    code += 1;
                }
                if node.kind != NativeKind::BlockQuote {
                    continue;
                }
                let lineage = view.lineage(token)?;
                let Some(accepted) = view.callout(lineage)? else {
                    continue;
                };
                let mut marker = String::new();
                let mut cursor = view.source_cursor(accepted.marker)?;
                while let Some(piece) = view.next_source(&mut cursor)? {
                    marker.push_str(&source[piece.p.start..piece.p.end]);
                }
                if let Some(title) = accepted.title {
                    let first = view.node(node.first_child.unwrap())?;
                    assert_eq!(first.kind, NativeKind::Paragraph);
                    assert_eq!(first.id, view.binding(title)?.id);
                    assert!(first.next_sibling.is_some());
                    let title = view.node(first.first_child.unwrap())?;
                    assert_eq!(title.kind, NativeKind::Text);
                    if accepted.kind == CalloutKind::Tip {
                        let emphasis = view.node(title.next_sibling.unwrap())?;
                        assert_eq!(emphasis.kind, NativeKind::Emphasis);
                    }
                }
                found.push((
                    marker,
                    accepted.kind,
                    accepted.disclosure,
                    accepted.title.is_some(),
                ));
            }
            assert_eq!(
                found,
                [
                    (
                        "[!TiP]".into(),
                        CalloutKind::Tip,
                        CalloutDisclosure::Static,
                        true
                    ),
                    (
                        "[!FAQ]-".into(),
                        CalloutKind::Question,
                        CalloutDisclosure::InitiallyClosed,
                        true
                    ),
                    (
                        "[!IMPORTANT]".into(),
                        CalloutKind::Important,
                        CalloutDisclosure::Static,
                        false
                    ),
                    (
                        "[!SUCCESS]".into(),
                        CalloutKind::Success,
                        CalloutDisclosure::Static,
                        true
                    ),
                ]
            );
            assert_eq!(code, 2);
            Ok(())
        };
        compile(input, &options, &mut host, CallbackCompiler(Some(inspect))).unwrap();
        assert_eq!(host.live.get(), 0);
    }
    with_input(source, |input| check(input, source));
}

#[test]
fn actual_sparse_groups_and_paged_record_layout_have_no_empty_per_node_owners() {
    use crate::native::mapped::*;
    use crate::native::rows::NativeRows;
    fn layout<T>(name: &str) {
        let size = std::mem::size_of::<T>();
        let width = NativeRows::<T, Lease>::width();
        assert!(width > 0 && width <= 1024 && size * width <= 16_384);
        println!(
            "native layout {name}: row={size} width={width} page={} descriptor={}",
            size * width,
            NativeRows::<T, Lease>::descriptor_bytes()
        );
    }
    layout::<NodeBinding>("node");
    layout::<MappedValue>("value");
    layout::<BlockBinding>("block");
    layout::<SyntaxBinding>("syntax");
    layout::<MathBinding>("math");
    layout::<TerminalBinding>("terminal");
    layout::<BufferMapping>("buffer");
    layout::<PayloadBinding>("payload");
    layout::<Proof>("proof");
    layout::<Claim>("claim");
    layout::<crate::native::definitions::DefinitionRecord>("definition");
    with_input(
        "# h\n\nplain [x](u) **z** ` a `\n\n```\nf\n```\n\n    tail",
        |input| {
            let mut host = TestHost::new(&input);
            parse(input, &crate::Options::default(), &mut host, |view| {
                let m = view.mappings;
                assert_eq!(m.nodes.len(), view.created_nodes as usize);
                for count in [
                    m.values.len(),
                    m.blocks.len(),
                    m.syntax.len(),
                    m.terminals.len(),
                ] {
                    assert!(
                        count > 0 && count < m.nodes.len(),
                        "not sparsely issued: {count}/{}",
                        m.nodes.len()
                    );
                }
                println!(
                    "native sparse nodes={} values={} blocks={} syntax={} terminals={}",
                    m.nodes.len(),
                    m.values.len(),
                    m.blocks.len(),
                    m.syntax.len(),
                    m.terminals.len()
                );
                Ok(())
            })
            .unwrap();
            assert_eq!(host.live.get(), 0);
        },
    );
}

fn map_text<'read, 'store, 'view, 'input, Phase: Copy>(
    view: &mut ReadAccess<'read, 'store, 'view, 'input, TestHost<'input>, Lease, Phase>,
    map: MappingToken<'view, 'input>,
    p: &str,
) -> Result<String, Error> {
    let mut out = String::new();
    if let Some(prefix) = view.mapping_prefix(map)? {
        view.with_prefix_chunk_next(prefix.token, 0, prefix.bytes, 4096, |chunk, _| {
            out.push_str(chunk);
            Ok(())
        })?;
    }
    for index in 0..view.mapping_len(map)? {
        let atom = view.mapping_atom(map, index)?;
        if atom.hidden_syntax {
            assert_eq!(atom.bytes, 0);
            continue;
        }
        if let Some(proof) = atom.proof {
            let info = view.proof(proof)?;
            assert_eq!(info.output_len, atom.bytes);
            let mut at = 0;
            while at < info.output_len {
                view.with_proof_chunk_next(proof, at, info.output_len, 4, |chunk, _| {
                    at += chunk.len();
                    out.push_str(chunk);
                    Ok(())
                })?;
            }
        } else {
            let mut cursor = view.source_cursor(atom.source)?;
            while let Some(piece) = view.next_source(&mut cursor)? {
                match piece.kind {
                    super::super::AppendKind::Copy => out.push_str(&p[piece.p.start..piece.p.end]),
                    super::super::AppendKind::PartialTab { .. } => {
                        for _ in piece.r_start..piece.r_end {
                            out.push(' ');
                        }
                    }
                    super::super::AppendKind::CodeTerminal => out.push('\n'),
                    super::super::AppendKind::TableEscapedPipe => {
                        assert_eq!(&p[piece.p.start..piece.p.end], "\\|");
                        assert_eq!(piece.r_end - piece.r_start, 1);
                        out.push('|');
                    }
                }
            }
        }
    }
    Ok(out)
}

#[path = "phase_tests.rs"]
mod phases;

#[path = "math_tests.rs"]
mod math_tests;

#[test]
fn real_mutations_keep_actual_value_and_current_mapping_together() {
    for p in [
        "",
        "x",
        "中文😀é",
        "a\n---\n",
        "***x*** y &NotEqualTilde; &amp;\\* z\n",
        "` a\nb `\n",
        "    x\n\n",
        "``` a&amp;\\* \nx\n```\n",
        "<a@b.example>\n",
        "[x](a&amp;b \"c\\*d\")\n",
        "[r]: /a \"x\"\n[r]: /b \"&amp;\"\n\n[r] [r]\n",
        "![a **b**](u \"t&amp;\")\n",
        "> \tcode\n",
        "<b>x</b>\n",
        "before *a **b** c* ![outer ![inner](v) tail](u) ![](empty) after\n\nend",
        r#"![a&amp;b](<u\&amp;x> "t&amp;\!")"#,
        r#"[x](u "&bsol;&ast;")"#,
        r#"[x](u "&bsol;&ast;&amp;tail")"#,
        "`\na\n`",
        "&amp;&amp;",
        "&bsol;*",
        "&amp;amp;",
        "[x]()",
        "[x](u \"\")",
        "``` lang\nx",
        "    a",
        "a  \nb",
        "a\\\nb",
        "unfinished [`a *",
    ] {
        with_input(p, |input| {
            let mut host = TestHost::new(&input);
            let result = parse(input, &crate::Options::default(), &mut host, |mut view| {
                let mut stack = vec![view.root()?];
                while let Some(node) = stack.pop() {
                    let info = view.node(node)?;
                    if let Some(next) = info.next_sibling {
                        stack.push(next);
                    }
                    if let Some(child) = info.first_child {
                        stack.push(child);
                    }
                    let lineage = view.lineage(node)?;
                    let binding = view.binding(lineage)?;
                    let mut values = Vec::new();
                    match info.attributes {
                        NodeAttributes::Text(v) | NodeAttributes::Html(v) => values.push(v),
                        NodeAttributes::Code { info, literal, .. } => {
                            values.push(info);
                            values.push(literal);
                        }
                        NodeAttributes::InlineCode { literal, .. } => values.push(literal),
                        NodeAttributes::Link { destination, title } => {
                            values.push(destination);
                            values.push(title);
                        }
                        _ => {}
                    }
                    for value in values {
                        let actual = text(&mut view, value)?;
                        if let Some(map) = view.value_mapping(value)? {
                            let mut mapped = map_text(&mut view, map, p)?;
                            if binding.terminal_synthetic && matches!(value.field, Field::Code) {
                                mapped.push('\n');
                            }
                            assert_eq!(mapped, actual, "input {p:?}, node {:?}", info.id);
                        } else {
                            assert!(actual.is_empty());
                        }
                    }
                }
                for index in 0..view.definition_count() {
                    let def = view.definition(index)?;
                    for value in [Some(def.destination), def.title].into_iter().flatten() {
                        let actual = text(&mut view, value)?;
                        let map = view
                            .value_mapping(value)?
                            .expect("definition payload binding");
                        assert_eq!(map_text(&mut view, map, p)?, actual);
                    }
                }
                Ok(())
            });
            assert_eq!(result, Ok(()), "input {p:?}");
            assert_eq!(host.live.get(), 0);
            assert_eq!(host.old_transport, 0);
            assert_eq!(host.checkpoints, 2);
        });
    }
}

#[test]
fn duplicate_and_unused_definitions_keep_original_cleaned_values() {
    with_input("[r]: /a \"x\"\n[r]: /b \"&amp;\"\n", |input| {
        let mut host = TestHost::new(&input);
        parse(input, &crate::Options::default(), &mut host, |mut view| {
            assert_eq!(view.definition_count(), 2);
            let a = view.definition(0)?;
            let b = view.definition(1)?;
            assert!(a.winner);
            assert!(!b.winner);
            assert_eq!(text(&mut view, a.normalized_key)?, "r");
            assert_eq!(text(&mut view, b.normalized_key)?, "r");
            assert_eq!(text(&mut view, a.destination)?, "/a");
            assert_eq!(text(&mut view, b.destination)?, "/b");
            assert_eq!(text(&mut view, b.title.unwrap())?, "&");
            let map = view.value_mapping(b.title.unwrap())?.unwrap();
            let mut entity = 0;
            for index in 0..view.mapping_len(map)? {
                if let Some(p) = view.mapping_atom(map, index)?.proof
                    && view.proof(p)?.kind
                        == TransformKind::Payload(super::super::StageKind::Entity)
                {
                    entity += 1;
                }
            }
            assert_eq!(entity, 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(host.live.get(), 0);
    });
}

#[test]
fn raw_error_and_ignored_error_preserve_first_fault() {
    with_input("x", |input| {
        let mut host = TestHost::new(&input);
        let result = parse(input, &crate::Options::default(), &mut host, |mut view| {
            assert_eq!(
                view.with_host(|_| Err::<(), _>(Error::Raw)),
                Err(Error::Raw)
            );
            assert_eq!(
                view.with_host(|_| Err::<(), _>(Error::Cancelled)),
                Err(Error::Raw)
            );
            assert_eq!(view.with_host(|_| Ok(())), Err(Error::Raw));
            assert!(matches!(view.root(), Err(Error::Raw)));
            Ok(())
        });
        assert_eq!(result, Err(Error::Raw));
        assert_eq!(host.live.get(), 0);
    });
}

#[test]
fn callback_success_cannot_skip_real_cleanup_or_tail_cancellation() {
    struct Output(Rc<Cell<bool>>);
    impl Drop for Output {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    with_input("[r]: /unused \"&amp;\"\n", |input| {
        let mut host = TestHost::new(&input);
        host.cancel_tail = true;
        let result = parse(input, &crate::Options::default(), &mut host, |_| {
            Ok(Output(Rc::clone(&dropped)))
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(dropped.get());
        assert_eq!(host.live.get(), 0);
        assert_eq!(host.checkpoints, 2);
    });
}

#[test]
fn invalid_utf8_endpoint_latches_before_another_read() {
    with_input("中文😀", |input| {
        let mut host = TestHost::new(&input);
        let result = parse(input, &crate::Options::default(), &mut host, |mut view| {
            let root = view.root()?;
            let paragraph = view.node(root)?.first_child.unwrap();
            let text_node = view.node(paragraph)?.first_child.unwrap();
            let NodeAttributes::Text(value) = view.node(text_node)?.attributes else {
                panic!("text")
            };
            assert_eq!(text(&mut view, value)?, "中文😀");
            let mut called = false;
            assert_eq!(
                view.value_cursor(value, 0, 1).and_then(|cursor| {
                    view.with_next_value_chunk(cursor, 4, |_, _| {
                        called = true;
                        Ok(())
                    })
                    .map(|_| ())
                }),
                Err(Error::Invalid)
            );
            assert!(!called);
            assert_eq!(
                view.value_cursor(value, 0, 10).map(|_| ()),
                Err(Error::Invalid)
            );
            Ok(())
        });
        assert_eq!(result, Err(Error::Invalid));
        assert_eq!(host.live.get(), 0);
    });
}

#[test]
fn reference_occurrence_node_rejects_owned_pending_and_wrong_payload_slots() {
    // Both occurrences reuse the same accepted definition payloads in Core.
    // Reading the real node must still validate its own destination/title.
    for occurrence in 0..2 {
        for title in [false, true] {
            for storage in 0..3 {
                with_input("[r]: /a \"x\"\n\n[r] [r]", |input| {
                    let mut host = TestHost::new(&input);
                    let result = parse(input, &crate::Options::default(), &mut host, |mut view| {
                        let root = view.root()?;
                        let paragraph = view.node(root)?.first_child.unwrap();
                        let first = view.node(paragraph)?.first_child.unwrap();
                        let selected = if occurrence == 0 {
                            first
                        } else {
                            let space = view.node(first)?.next_sibling.unwrap();
                            view.node(space)?.next_sibling.unwrap()
                        };
                        {
                            let mut ast = selected.node.data_mut();
                            let NodeValue::Link(link) = &mut ast.value else {
                                panic!("reference Link");
                            };
                            let wrong = if title {
                                link.url.clone()
                            } else {
                                link.title.clone()
                            };
                            let replacement = match storage {
                                0 => ValueText::from("forged Owned"),
                                1 => ValueText::pending(),
                                _ => wrong,
                            };
                            if title {
                                link.title = replacement;
                            } else {
                                link.url = replacement;
                            }
                        }
                        assert!(matches!(view.node(selected), Err(Error::Invalid)));
                        assert!(matches!(view.root(), Err(Error::Invalid)));
                        Ok(()) // Swallowing does not clear the operation's first fault.
                    });
                    assert_eq!(result, Err(Error::Invalid));
                    assert_eq!(host.live.get(), 0);
                });
            }
        }
    }
}

#[test]
fn empty_buffer_and_existing_buffer_end_have_distinct_anchors() {
    // The empty fenced body retains its real accepted zero-width append at
    // the start of the body, not an invented P0 or the end of the document.
    for (p, expected) in [("```\n```\n", 4), ("x\n", 2)] {
        with_input(p, |input| {
            let mut host = TestHost::new(&input);
            parse(input, &crate::Options::default(), &mut host, |mut view| {
                let root = view.root()?;
                let block = view.node(root)?.first_child.unwrap();
                let lineage = view.lineage(block)?;
                let binding = view.binding(lineage)?;
                let mut source = binding.buffer.unwrap();
                let super::super::mapped::ClaimSource::R(ref mut r) = source.source else {
                    panic!("buffer")
                };
                r.start = r.end;
                let mut cursor = view.source_cursor(source)?;
                let anchor = view.next_source(&mut cursor)?.unwrap();
                assert_eq!(
                    anchor.p,
                    super::super::PSpan {
                        start: expected,
                        end: expected
                    },
                    "{p:?}"
                );
                assert!(view.next_source(&mut cursor)?.is_none());
                Ok(())
            })
            .unwrap();
            assert_eq!(host.live.get(), 0);
        });
    }
}

#[test]
fn callback_unwind_releases_native_and_poison_cannot_be_ignored() {
    with_input("x", |input| {
        let mut host = TestHost::new(&input);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parse(input, &crate::Options::default(), &mut host, |mut view| {
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    view.with_host::<()>(|_| panic!("intentional native callback unwind"))
                }));
                assert!(panic.is_err());
                assert!(matches!(view.root(), Err(Error::Invalid)));
                Ok(())
            })
        }));
        assert_eq!(result.unwrap(), Err(Error::Invalid));
        assert_eq!(host.live.get(), 0);
    });
    with_input("[r]: /unused", |input| {
        let mut host = TestHost::new(&input);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parse::<_, ()>(input, &crate::Options::default(), &mut host, |_| {
                panic!("intentional finalization unwind")
            })
        }));
        assert!(result.is_err());
        assert_eq!(host.fault, Some(Error::Invalid));
        assert_eq!(host.live.get(), 0);
    });
}

#[test]
fn adjacent_entities_stay_separate_and_connected_metadata_retains_ancestors() {
    with_input("&amp;&amp;", |input| {
        let mut host = TestHost::new(&input);
        parse(input, &crate::Options::default(), &mut host, |mut view| {
            let root = view.root()?;
            let paragraph = view.node(root)?.first_child.unwrap();
            let node = view.node(paragraph)?.first_child.unwrap();
            let NodeAttributes::Text(value) = view.node(node)?.attributes else {
                panic!("text")
            };
            let map = view.value_mapping(value)?.unwrap();
            assert_eq!(view.mapping_len(map)?, 2);
            let left = view.mapping_atom(map, 0)?;
            let right = view.mapping_atom(map, 1)?;
            assert!(view.same_source(left.source, left.source)?);
            assert!(!view.same_source(left.source, right.source)?);
            let left_host = view.proof(left.proof.unwrap())?.host.source;
            let right_host = view.proof(right.proof.unwrap())?.host.source;
            assert!(view.same_source(left_host, right_host)?);
            let paragraph_id = view.node(paragraph)?.id;
            for i in 0..2 {
                let atom = view.mapping_atom(map, i)?;
                assert_eq!(atom.bytes, 1);
                let proof = view.proof(atom.proof.unwrap())?;
                assert_eq!(
                    proof.kind,
                    TransformKind::Decode(super::super::DecodeKind::Entity)
                );
                assert_eq!(proof.host.role, HostRole::Inline);
                assert_eq!(proof.host.phase, NativePhase::Inline);
                assert_eq!(proof.host.utf8_len, 10);
                assert_eq!(
                    (proof.host.consumed_start, proof.host.consumed_end),
                    (i * 5, (i + 1) * 5)
                );
                assert_eq!(view.mapping_len(proof.input)?, 1);
                let owner = view.source_owner(atom.source)?.unwrap();
                assert_eq!(view.binding(owner)?.id, paragraph_id);
            }
            Ok(())
        })
        .unwrap();
    });
    with_input(r#"[x](u "&bsol;&ast;")"#, |input| {
        let mut host = TestHost::new(&input);
        parse(input, &crate::Options::default(), &mut host, |mut view| {
            let root = view.root()?;
            let paragraph = view.node(root)?.first_child.unwrap();
            let node = view.node(paragraph)?.first_child.unwrap();
            let NodeAttributes::Link { title, .. } = view.node(node)?.attributes else {
                panic!("link")
            };
            assert_eq!(text(&mut view, title)?, "*");
            let map = view.value_mapping(title)?.unwrap();
            let mut transformed = None;
            for i in 0..view.mapping_len(map)? {
                let atom = view.mapping_atom(map, i)?;
                if let Some(proof) = atom.proof {
                    assert!(transformed.replace(proof).is_none());
                }
            }
            let proof = view.proof(transformed.unwrap())?;
            assert_eq!(
                proof.kind,
                TransformKind::Payload(super::super::StageKind::Escape)
            );
            assert_eq!(proof.host.role, HostRole::Title);
            assert_eq!(proof.host.phase, NativePhase::Metadata);
            assert_eq!(proof.host.utf8_len, 2);
            assert_eq!((proof.host.consumed_start, proof.host.consumed_end), (0, 2));
            assert_eq!(view.mapping_len(proof.input)?, 2);
            for i in 0..2 {
                let ancestor = view.mapping_atom(proof.input, i)?.proof.unwrap();
                assert_eq!(
                    view.proof(ancestor)?.kind,
                    TransformKind::Payload(super::super::StageKind::Entity)
                );
            }
            Ok(())
        })
        .unwrap();
    });
}

#[test]
fn source_identity_keeps_r_buffers_and_p_domain_distinct() {
    with_input("&amp;\n\n&amp;", |input| {
        let mut host = TestHost::new(&input);
        parse(input, &crate::Options::default(), &mut host, |mut view| {
            let root = view.root()?;
            let first = view.node(root)?.first_child.unwrap();
            let second = view.node(first)?.next_sibling.unwrap();
            let mut sources = Vec::new();
            for paragraph in [first, second] {
                let node = view.node(paragraph)?.first_child.unwrap();
                let NodeAttributes::Text(value) = view.node(node)?.attributes else {
                    panic!("text")
                };
                let map = view.value_mapping(value)?.unwrap();
                sources.push(view.mapping_atom(map, 0)?.source);
            }
            // Both actual R spans are 0..5, but belong to different buffers.
            assert!(!view.same_source(sources[0], sources[1])?);
            let mut cursor = view.source_cursor(sources[0])?;
            let actual_p = view.next_source(&mut cursor)?.unwrap().p;
            let p = view.source_p(actual_p);
            assert!(view.same_source(p, p)?);
            assert!(!view.same_source(sources[0], p)?);
            view.with_host(|host| {
                host.fault = Some(Error::Raw);
                Ok(())
            })
            .unwrap_err();
            assert_eq!(view.same_source(p, p), Err(Error::Raw));
            Ok(())
        })
        .unwrap_err();
        assert_eq!(host.live.get(), 0);
    });
}

#[test]
fn payload_raw_domain_survives_empty_value_and_title_trimming() {
    for (input_text, expected) in [("[x]()", ""), ("[x](u \"&amp;\")", "\"&amp;\"")] {
        with_input(input_text, |input| {
            let mut host = TestHost::new(&input);
            parse(input, &crate::Options::default(), &mut host, |mut view| {
                let root = view.root()?;
                let paragraph = view.node(root)?.first_child.unwrap();
                let link = view.node(paragraph)?.first_child.unwrap();
                let NodeAttributes::Link { destination, title } = view.node(link)?.attributes
                else {
                    panic!("link")
                };
                let value = if expected.is_empty() {
                    destination
                } else {
                    title
                };
                let map = view.value_mapping(value)?.unwrap();
                let source = view.mapping_source(map)?.unwrap();
                let mut cursor = view.source_cursor(source)?;
                let mut raw = String::new();
                let mut pieces = 0;
                while let Some(piece) = view.next_source(&mut cursor)? {
                    assert_eq!(piece.kind, super::super::AppendKind::Copy);
                    raw.push_str(&input_text[piece.p.start..piece.p.end]);
                    pieces += 1;
                }
                assert_eq!(raw, expected);
                assert!(pieces > 0);
                assert_eq!(
                    text(&mut view, value)?,
                    if expected.is_empty() { "" } else { "&" }
                );
                Ok(())
            })
            .unwrap();
            assert_eq!(host.live.get(), 0);
        });
    }
}

#[test]
fn marker_is_first_accepted_and_definition_domain_is_full_r_buffer() {
    with_input("    a\n    b\n", |input| {
        let mut host = TestHost::new(&input);
        parse(input, &crate::Options::default(), &mut host, |mut view| {
            let root = view.root()?;
            let code = view.node(root)?.first_child.unwrap();
            let lineage = view.lineage(code)?;
            let marker = view.binding(lineage)?.marker.unwrap();
            let domain = view.source_domain(marker)?;
            assert_eq!(domain.utf8_len, 12);
            assert_eq!((domain.consumed_start, domain.consumed_end), (0, 4));
            let mut cursor = view.source_cursor(marker)?;
            assert_eq!(
                view.next_source(&mut cursor)?.unwrap().p,
                super::super::PSpan { start: 0, end: 4 }
            );
            assert!(view.next_source(&mut cursor)?.is_none());
            Ok(())
        })
        .unwrap();
    });
    with_input("[r]: /u\nx", |input| {
        let mut host = TestHost::new(&input);
        parse(input, &crate::Options::default(), &mut host, |mut view| {
            let definition = view.definition(0)?;
            let domain = view.source_domain(definition.syntax)?;
            assert_eq!(domain.utf8_len, 9);
            assert_eq!((domain.consumed_start, domain.consumed_end), (0, 8));
            let mut cursor = view.source_cursor(domain.source)?;
            let mut length = 0;
            while let Some(piece) = view.next_source(&mut cursor)? {
                length += piece.r_end - piece.r_start;
            }
            assert_eq!(length, 9);
            Ok(())
        })
        .unwrap();
    });
}

#[test]
fn code_trim_keeps_two_zero_output_edge_proofs_and_line_ending_proof() {
    // The product's PreparedBinding supplies the LF case after N01; the CRLF
    // case additionally checks the native grammar entry itself, without S/N01.
    for input_text in ["` a\nb `", "` a\r\nb `"] {
        with_input(input_text, |input| {
            let mut host = TestHost::new(&input);
            parse(input, &crate::Options::default(), &mut host, |mut view| {
                let root = view.root()?;
                let paragraph = view.node(root)?.first_child.unwrap();
                let node = view.node(paragraph)?.first_child.unwrap();
                let NodeAttributes::InlineCode { literal, .. } = view.node(node)?.attributes else {
                    panic!("code")
                };
                assert_eq!(text(&mut view, literal)?, "a b");
                let map = view.value_mapping(literal)?.unwrap();
                assert_eq!(map_text(&mut view, map, input_text)?, "a b");
                assert_eq!(view.mapping_len(map)?, 5);
                for (index, kind, start, end, output) in [
                    (0, super::super::DecodeKind::CodeTrim, 1, 2, ""),
                    (
                        2,
                        super::super::DecodeKind::CodeLineEndingToSpace,
                        3,
                        input_text.len() - 3,
                        " ",
                    ),
                    (
                        4,
                        super::super::DecodeKind::CodeTrim,
                        input_text.len() - 2,
                        input_text.len() - 1,
                        "",
                    ),
                ] {
                    let atom = view.mapping_atom(map, index)?;
                    assert!(!atom.hidden_syntax);
                    assert_eq!(atom.bytes, output.len());
                    let token = atom.proof.unwrap();
                    let info = view.proof(token)?;
                    assert_eq!(info.kind, TransformKind::Decode(kind));
                    assert_eq!((info.local_start, info.local_end), (0, end - start));
                    assert_eq!(info.output_len, output.len());
                    assert_eq!(info.host.utf8_len, input_text.len());
                    assert_eq!(
                        (info.host.consumed_start, info.host.consumed_end),
                        (start, end)
                    );
                    assert_eq!(view.mapping_len(info.input)?, 1);
                    let ancestor = view.mapping_atom(info.input, 0)?;
                    assert_eq!(ancestor.bytes, end - start);
                    assert!(ancestor.proof.is_none());
                    let mut cursor = view.source_cursor(atom.source)?;
                    assert_eq!(
                        view.next_source(&mut cursor)?.unwrap().p,
                        super::super::PSpan { start, end }
                    );
                    assert!(view.next_source(&mut cursor)?.is_none());
                    view.with_proof_chunk(token, 0, output.len(), |value, _| {
                        assert_eq!(value, output);
                        Ok(())
                    })?;
                }
                Ok(())
            })
            .unwrap();
            assert_eq!(host.live.get(), 0);
        });
    }
}

#[test]
fn sequential_code_transforms_do_not_recopy_the_completed_prefix() {
    let mut charged = Vec::new();
    for count in [256, 1024] {
        let body = "a\n".repeat(count);
        let input_text = format!("` {body} `");
        with_input(&input_text, |input| {
            let mut host = TestHost::new(&input);
            parse(input, &crate::Options::default(), &mut host, |mut view| {
                let root = view.root()?;
                let paragraph = view.node(root)?.first_child.unwrap();
                let node = view.node(paragraph)?.first_child.unwrap();
                let NodeAttributes::InlineCode { literal, .. } = view.node(node)?.attributes else {
                    panic!("code")
                };
                let map = view.value_mapping(literal)?.unwrap();
                assert_eq!(view.mapping_len(map)?, 2 * count + 2);
                assert_eq!(text(&mut view, literal)?, "a ".repeat(count));
                assert_eq!(map_text(&mut view, map, &input_text)?, "a ".repeat(count));
                Ok(())
            })
            .unwrap();
            assert_eq!(host.live.get(), 0);
            let total = host.scanned + host.copied + host.events;
            eprintln!(
                "native sequential transforms {count}: scan={} copy={} events={} sum={total}",
                host.scanned, host.copied, host.events
            );
            charged.push(total);
        });
    }
    // Four times the accepted transformations, with room for real geometric
    // allocation growth and hash probes, must not recopy every prior prefix.
    assert!(charged[1] < charged[0] * 6);
}
