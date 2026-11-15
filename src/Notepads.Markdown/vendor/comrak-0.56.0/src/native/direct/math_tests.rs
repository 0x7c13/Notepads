// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::native::{DecodeKind, MathSyntax};

fn options() -> crate::Options<'static> {
    let mut options = crate::Options::default();
    options.extension.math_dollars = true;
    options.extension.math_code = true;
    options.extension.math_latex = true;
    options
}

fn source_text<'read, 'store, 'view, 'input, Phase: Copy>(
    view: &mut ReadAccess<'read, 'store, 'view, 'input, TestHost<'input>, Lease, Phase>,
    source: SourceToken<'view, 'input>,
    input: &str,
) -> Result<String, Error> {
    let mut cursor = view.source_cursor(source)?;
    let mut output = String::new();
    while let Some(piece) = view.next_source(&mut cursor)? {
        output.push_str(&input[piece.p.start..piece.p.end]);
    }
    Ok(output)
}

#[test]
fn math_actual_forms_keep_typed_value_syntax_and_full_claim() {
    for (input_text, expected, mode) in [
        (
            "$x\n&amp;\\alpha😀$",
            "x &amp;\\alpha😀",
            MathSyntax::DollarInline,
        ),
        ("$` a\n*b*&amp; `$", "a *b*&amp;", MathSyntax::CodeInline),
        ("$$ x\ny $$", " x\ny ", MathSyntax::DollarDisplay),
        ("\\( x\ny \\)", " x\ny ", MathSyntax::LatexInline),
        ("\\[ x\ny \\]", " x\ny ", MathSyntax::LatexDisplay),
    ] {
        with_input(input_text, |input| {
            let mut host = TestHost::new(&input);
            parse(input, &options(), &mut host, |mut view| {
                let root = view.root()?;
                let paragraph = view.node(root)?.first_child.unwrap();
                let node = view.node(paragraph)?.first_child.unwrap();
                let info = view.node(node)?;
                assert_eq!(info.kind, NativeKind::Math);
                let NodeAttributes::Math { syntax, literal } = info.attributes else {
                    panic!("Math");
                };
                assert_eq!(syntax, mode);
                assert_eq!(text(&mut view, literal)?, expected);
                let map = view.value_mapping(literal)?.unwrap();
                assert_eq!(map_text(&mut view, map, input_text)?, expected);
                let lineage = view.lineage(node)?;
                let binding = view.binding(lineage)?;
                assert_eq!(binding.math_syntax, Some(mode));
                let width = mode.delimiter_bytes();
                let expected_input = &input_text[width..input_text.len() - width];
                assert_eq!(
                    source_text(&mut view, binding.math_input.unwrap(), input_text)?,
                    expected_input
                );
                let raw = view.mapping_source(map)?.unwrap();
                assert_eq!(source_text(&mut view, raw, input_text)?, expected_input);
                assert_eq!(
                    source_text(&mut view, binding.syntax.unwrap(), input_text)?,
                    input_text
                );
                let mut claims = 0;
                for index in 0..view.claim_count() {
                    let claim = view.claim(index)?;
                    if let ClaimKind::Math(claim_mode) = claim.kind {
                        claims += 1;
                        assert_eq!(claim_mode, mode);
                        assert_eq!(claim.owner.id, info.id);
                        let host = claim.host.unwrap();
                        assert_eq!(host.role, HostRole::MathInput);
                        assert_eq!(host.phase, NativePhase::Inline);
                        assert_eq!(
                            source_text(&mut view, claim.source, input_text)?,
                            input_text
                        );
                    }
                }
                assert_eq!(claims, 1);
                for index in 0..view.mapping_len(map)? {
                    if let Some(proof) = view.mapping_atom(map, index)?.proof {
                        let proof = view.proof(proof)?;
                        assert!(matches!(
                            mode,
                            MathSyntax::DollarInline | MathSyntax::CodeInline
                        ));
                        assert!(matches!(
                            proof.kind,
                            TransformKind::Decode(
                                DecodeKind::CodeLineEndingToSpace | DecodeKind::CodeTrim
                            )
                        ));
                        assert_eq!(proof.host.role, HostRole::MathInput);
                        assert_eq!(proof.host.phase, NativePhase::Inline);
                    }
                }
                Ok(())
            })
            .unwrap_or_else(|error| panic!("{input_text:?}: {error:?}"));
            assert_eq!(host.live.get(), 0);
            assert_eq!(host.old_transport, 0);
        });
    }
}

#[test]
fn math_crlf_and_real_container_table_transport_preserve_literal_authority() {
    for (input_text, expected, mode, expected_source, table) in [
        (
            "$a\r\nb$",
            "a b",
            MathSyntax::DollarInline,
            "$a\r\nb$",
            false,
        ),
        (
            "> \\[a\n> b\\]",
            "a\nb",
            MathSyntax::LatexDisplay,
            "\\[a\nb\\]",
            false,
        ),
        (
            "- \\(a\n  b\\)",
            "a\nb",
            MathSyntax::LatexInline,
            "\\(a\nb\\)",
            false,
        ),
        (
            "| m |\n| - |\n| $a\\|b$ |\n",
            "a|b",
            MathSyntax::DollarInline,
            "$a\\|b$",
            true,
        ),
    ] {
        with_input(input_text, |input| {
            let mut host = TestHost::new(&input);
            let mut options = options();
            if table {
                options.extension.table = true;
                options.extension.tasklist = true;
                options.extension.strikethrough = true;
            }
            parse(input, &options, &mut host, |mut view| {
                let mut stack = vec![view.root()?];
                let mut count = 0;
                while let Some(node) = stack.pop() {
                    let info = view.node(node)?;
                    if let Some(sibling) = info.next_sibling {
                        stack.push(sibling);
                    }
                    if let Some(child) = info.first_child {
                        stack.push(child);
                    }
                    let NodeAttributes::Math { syntax, literal } = info.attributes else {
                        continue;
                    };
                    count += 1;
                    assert_eq!(syntax, mode);
                    assert_eq!(text(&mut view, literal)?, expected);
                    let map = view.value_mapping(literal)?.unwrap();
                    assert_eq!(map_text(&mut view, map, input_text)?, expected);
                    let lineage = view.lineage(node)?;
                    let binding = view.binding(lineage)?;
                    assert_eq!(
                        source_text(&mut view, binding.math_input.unwrap(), input_text)?,
                        &expected_source[mode.delimiter_bytes()
                            ..expected_source.len() - mode.delimiter_bytes()]
                    );
                    let raw = view.mapping_source(map)?.unwrap();
                    assert_eq!(
                        source_text(&mut view, raw, input_text)?,
                        &expected_source[mode.delimiter_bytes()
                            ..expected_source.len() - mode.delimiter_bytes()]
                    );
                    assert_eq!(
                        source_text(&mut view, binding.syntax.unwrap(), input_text)?,
                        expected_source
                    );
                }
                assert_eq!(count, 1, "{input_text:?}");
                Ok(())
            })
            .unwrap();
            assert_eq!(host.live.get(), 0);
        });
    }
}

#[test]
fn math_rejects_owned_pending_sibling_role_and_changed_display_slots() {
    for mutation in 0..5 {
        with_input("$x$ $y$", |input| {
            let mut host = TestHost::new(&input);
            let result = parse(input, &options(), &mut host, |mut view| {
                let root = view.root()?;
                let paragraph = view.node(root)?.first_child.unwrap();
                let first = view.node(paragraph)?.first_child.unwrap();
                let space = view.node(first)?.next_sibling.unwrap();
                let second = view.node(space)?.next_sibling.unwrap();
                let sibling = match &second.node.data().value {
                    NodeValue::Math(math) => math.literal.clone(),
                    _ => panic!("second Math"),
                };
                let id = view.node(first)?.id;
                {
                    let mut data = first.node.data_mut();
                    let NodeValue::Math(math) = &mut data.value else {
                        panic!("first Math");
                    };
                    match mutation {
                        0 => math.literal = "forged".into(),
                        1 => math.literal = ValueText::pending(),
                        2 => math.literal = sibling,
                        3 => {
                            math.literal = ValueText::accepted(AcceptedValueRef::Node {
                                node: id,
                                role: NodeValueRole::InlineCode,
                            })
                        }
                        _ => math.display_math = true,
                    }
                }
                assert!(matches!(view.node(first), Err(Error::Invalid)));
                assert!(matches!(view.root(), Err(Error::Invalid)));
                Ok(())
            });
            assert_eq!(result, Err(Error::Invalid));
            assert_eq!(host.live.get(), 0);
        });
    }
}

#[test]
fn disabled_math_extensions_preserve_plain_text_and_code() {
    with_input("$x$ \\(y\\) $`z`$", |input| {
        let mut host = TestHost::new(&input);
        parse(input, &crate::Options::default(), &mut host, |mut view| {
            let mut stack = vec![view.root()?];
            while let Some(node) = stack.pop() {
                let info = view.node(node)?;
                assert_ne!(info.kind, NativeKind::Math);
                if let NodeAttributes::Text(value) = info.attributes {
                    let map = view.value_mapping(value)?.unwrap();
                    assert!(view.mapping_source(map)?.is_none());
                }
                if let Some(sibling) = info.next_sibling {
                    stack.push(sibling);
                }
                if let Some(child) = info.first_child {
                    stack.push(child);
                }
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(host.live.get(), 0);
    });
}

struct RetainedMath<'view, 'input> {
    lineage: LineageToken<'view, 'input>,
    mapping: MappingToken<'view, 'input>,
    syntax: SourceToken<'view, 'input>,
    mode: MathSyntax,
}

struct MathCompiler<'input> {
    input: &'input str,
    entered: Rc<Cell<bool>>,
}

impl<'input> DirectCompiler<'input, TestHost<'input>> for MathCompiler<'input> {
    type Output = usize;
    type State<'view> = Vec<RetainedMath<'view, 'input>>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: AstReader<'store, 'view, 'arena, 'input, Invariant<'input>, Lease>,
        host: &mut TestHost<'input>,
    ) -> Result<Self::State<'view>, Error> {
        reader.with_reads(host, |view| {
            let mut stack = vec![view.root()?];
            let mut output = Vec::new();
            while let Some(node) = stack.pop() {
                let info = view.node(node)?;
                if let Some(sibling) = info.next_sibling {
                    stack.push(sibling);
                }
                if let Some(child) = info.first_child {
                    stack.push(child);
                }
                let NodeAttributes::Math {
                    syntax: mode,
                    literal,
                } = info.attributes
                else {
                    continue;
                };
                let lineage = view.lineage(node)?;
                let binding = view.binding(lineage)?;
                output.push(RetainedMath {
                    lineage,
                    mapping: view.value_mapping(literal)?.unwrap(),
                    syntax: binding.syntax.unwrap(),
                    mode,
                });
            }
            assert_eq!(output.len(), 4);
            Ok(output)
        })
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: ProvenanceReader<'store, 'view, 'input, Invariant<'input>, Lease>,
        state: Self::State<'view>,
        host: &mut TestHost<'input>,
    ) -> Result<usize, Error> {
        self.entered.set(true);
        assert_eq!(host.checkpoints, 1);
        assert!(host.live.get() > 0);
        reader.with_reads(host, |view| {
            for (entry, expected) in state.iter().zip(["a b", " x\ny ", " x\ny ", " x\ny "]) {
                let binding = view.binding(entry.lineage)?;
                assert_eq!(binding.math_syntax, Some(entry.mode));
                let raw = view.mapping_source(entry.mapping)?.unwrap();
                assert_eq!(
                    source_text(view, raw, self.input)?,
                    source_text(view, binding.math_input.unwrap(), self.input)?
                );
                assert_eq!(
                    source_text(view, binding.syntax.unwrap(), self.input)?,
                    source_text(view, entry.syntax, self.input)?
                );
                assert_eq!(map_text(view, binding.value, self.input)?, expected);
                assert_eq!(map_text(view, entry.mapping, self.input)?, expected);
                for index in 0..view.mapping_len(entry.mapping)? {
                    if let Some(token) = view.mapping_atom(entry.mapping, index)?.proof {
                        let proof = view.proof(token)?;
                        assert_eq!(proof.host.role, HostRole::MathInput);
                        assert_eq!(entry.mode, MathSyntax::DollarInline);
                    }
                }
            }
            Ok(state.len())
        })
    }
}

#[test]
fn math_actual_provenance_survives_arena_exit_and_cancelled_tails_release_owners() {
    let source = "$a\nb$ $$ x\ny $$ \\( x\ny \\) \\[ x\ny \\]";
    for (cancel_ast_exit, cancel_tail) in [(false, false), (true, false), (false, true)] {
        with_input(source, |input| {
            let mut host = TestHost::new(&input);
            host.cancel_ast_exit = cancel_ast_exit;
            host.cancel_tail = cancel_tail;
            let entered = Rc::new(Cell::new(false));
            let compiler = MathCompiler {
                input: source,
                entered: Rc::clone(&entered),
            };
            let result = compile(input, &options(), &mut host, compiler);
            assert_eq!(
                result,
                if cancel_ast_exit || cancel_tail {
                    Err(Error::Cancelled)
                } else {
                    Ok(4)
                }
            );
            assert_eq!(entered.get(), !cancel_ast_exit);
            assert_eq!(host.live.get(), 0);
            assert_eq!(host.old_transport, 0);
        });
    }
}

fn parse_math_strict<'input, T>(
    input: NativeInput<'input>,
    options: &crate::Options,
    host: &mut TestHost<'input>,
    body: impl for<'store, 'view, 'arena> FnOnce(
        FinalView<'store, 'view, 'arena, 'input, TestHost<'input>>,
    ) -> Result<T, Error>,
) -> Result<T, Error> {
    compile_with_inline_policy(
        input,
        options,
        host,
        CallbackCompiler(Some(body)),
        crate::native::InlinePolicy::StrictG4,
    )
}

#[test]
fn math_strict_g4_protects_full_syntax_and_preserves_external_links_in_all_modes() {
    for (math, expected, mode) in [
        (
            "$x https://inside.example/z y$",
            "x https://inside.example/z y",
            MathSyntax::DollarInline,
        ),
        (
            "$$x https://inside.example/z y$$",
            "x https://inside.example/z y",
            MathSyntax::DollarDisplay,
        ),
        (
            "\\(x https://inside.example/z y\\)",
            "x https://inside.example/z y",
            MathSyntax::LatexInline,
        ),
        (
            "\\[x https://inside.example/z y\\]",
            "x https://inside.example/z y",
            MathSyntax::LatexDisplay,
        ),
    ] {
        for container in 0..3 {
            let body = match container {
                0 => format!("{math}{math}"),
                1 => format!("[{math}{math}](/explicit)"),
                _ => format!("![{math}{math}](/image)"),
            };
            let source = format!("https://outside.example/x {body} https://end.example/y");
            with_input(&source, |input| {
                let mut host = TestHost::new(&input);
                let mut options = options();
                options.extension.table = true;
                options.extension.tasklist = true;
                options.extension.strikethrough = true;
                parse_math_strict(input, &options, &mut host, |mut view| {
                    let mut stack = vec![view.root()?];
                    let mut maths = 0;
                    let mut extended = Vec::new();
                    while let Some(node) = stack.pop() {
                        let info = view.node(node)?;
                        if let Some(sibling) = info.next_sibling {
                            stack.push(sibling);
                        }
                        if let Some(child) = info.first_child {
                            stack.push(child);
                        }
                        let lineage = view.lineage(node)?;
                        if let NodeAttributes::Math { syntax, literal } = info.attributes {
                            maths += 1;
                            assert_eq!(syntax, mode);
                            assert_eq!(text(&mut view, literal)?, expected);
                            let binding = view.binding(lineage)?;
                            assert_eq!(
                                source_text(&mut view, binding.syntax.unwrap(), &source)?,
                                math
                            );
                            assert!(view.extended_for_lineage(lineage)?.is_none());
                        }
                        if let Some(token) = view.extended_for_lineage(lineage)? {
                            let item = view.extended(token)?;
                            assert_eq!(item.kind, crate::native::g4::ExtendedKind::Url);
                            extended.push(source_text(&mut view, item.syntax, &source)?);
                        }
                    }
                    assert_eq!(maths, 2);
                    assert_eq!(
                        extended,
                        ["https://outside.example/x", "https://end.example/y"]
                    );
                    Ok(())
                })
                .unwrap();
                assert_eq!(host.live.get(), 0);
                assert_eq!(host.old_transport, 0);
            });
        }
    }
}
