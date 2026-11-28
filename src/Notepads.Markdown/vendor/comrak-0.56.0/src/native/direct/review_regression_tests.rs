// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use super::*;

#[test]
fn root_footnotes_follow_closed_lists_rules_and_toc_markers() {
    fn check<'input>(input: NativeInput<'input>, source: &str, policy: InlinePolicy) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.footnotes = true;
        options.extension.description_lists = true;
        options.extension.native_table_of_contents = true;
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.tasklist = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let root = view.root()?;
            let root_id = view.node(root)?.id;
            let mut stack = vec![root];
            let mut definitions = 0;
            let mut references = 0;
            while let Some(token) = stack.pop() {
                let row = view.node(token)?;
                if let Some(child) = row.first_child {
                    stack.push(child);
                }
                if let Some(next) = row.next_sibling {
                    stack.push(next);
                }
                if row.kind == NativeKind::FootnoteDefinition {
                    assert_eq!(view.node(row.parent.unwrap())?.id, root_id);
                    definitions += 1;
                } else if row.kind == NativeKind::FootnoteReference {
                    references += 1;
                }
            }
            assert_eq!((definitions, references), (1, 1), "{source:?}");
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

    for block in ["- item", "Term\n: details", "---", "[TOC]"] {
        let source = format!("Body[^n].\n\n{block}\n\n[^n]: Note.\n");
        for policy in [InlinePolicy::Ordinary, InlinePolicy::StrictG4] {
            with_input(&source, |input| check(input, &source, policy));
        }
    }
}

#[test]
fn description_details_do_not_reparent_an_authored_callout_title() {
    fn check<'input>(input: NativeInput<'input>, source: &str) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.alerts = true;
        options.extension.description_lists = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let root = view.root()?;
            let quote = view.node(root)?.first_child.unwrap();
            let quote_id = view.node(quote)?.id;
            let lineage = view.lineage(quote)?;
            let callout = view.callout(lineage)?.unwrap();
            let title = view.node(quote)?.first_child.unwrap();
            let title_row = view.node(title)?;
            assert_eq!(title_row.kind, NativeKind::Paragraph);
            assert_eq!(view.node(title_row.parent.unwrap())?.id, quote_id);
            assert_eq!(view.binding(callout.title.unwrap())?.id, title_row.id);
            let body = view.node(title_row.next_sibling.unwrap())?;
            assert_eq!(
                body.kind,
                if source.contains("Term") {
                    NativeKind::DescriptionList
                } else {
                    NativeKind::Paragraph
                }
            );
            Ok(())
        };
        compile(input, &options, &mut host, CallbackCompiler(Some(inspect))).unwrap();
        assert_eq!(host.live.get(), 0);
    }

    for source in [
        "> [!NOTE] Title\n> : detail\n",
        "> [!NOTE] Title\n>\n> : detail\n",
        "> [!NOTE] Title\n>\n> Term\n> : detail\n",
    ] {
        with_input(source, |input| check(input, source));
    }
}

#[test]
fn strict_heading_wikilinks_keep_escaped_pipes_out_of_the_table_grammar() {
    fn check<'input>(input: NativeInput<'input>) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.native_wiki_links = true;
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.tasklist = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let root = view.root()?;
            let heading = view.node(root)?.first_child.unwrap();
            let link = view.node(heading)?.first_child.unwrap();
            let row = view.node(link)?;
            assert_eq!(row.kind, NativeKind::Link);
            assert!(row.next_sibling.is_none());
            let lineage = view.lineage(link)?;
            assert_eq!(
                view.binding(lineage)?.wiki,
                Some(crate::native::WikiForm::Link)
            );
            let NodeAttributes::Link { destination, .. } = row.attributes else {
                unreachable!();
            };
            assert_eq!(text(&mut view, destination)?, "a\\|b");
            let label = view.node(row.first_child.unwrap())?;
            let NodeAttributes::Text(value) = label.attributes else {
                unreachable!();
            };
            assert_eq!(text(&mut view, value)?, "https://example.com");
            Ok(())
        };
        compile_with_inline_policy(
            input,
            &options,
            &mut host,
            CallbackCompiler(Some(inspect)),
            InlinePolicy::StrictG4,
        )
        .unwrap();
        assert_eq!(host.live.get(), 0);
    }

    let source = "# [[a\\|b| https://example.com]]\n";
    with_input(source, check);
}

#[test]
fn inline_note_closers_follow_code_math_and_escaped_bracket_openers() {
    fn check<'input>(
        input: NativeInput<'input>,
        source: &str,
        expected: &str,
        dollars: bool,
        policy: InlinePolicy,
    ) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.footnotes = true;
        options.extension.inline_footnotes = true;
        options.extension.math_code = true;
        options.extension.math_dollars = dollars;
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.tasklist = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let root = view.root()?;
            let paragraph = view.node(root)?.first_child.unwrap();
            let reference = view.node(paragraph)?.first_child.unwrap();
            assert_eq!(view.node(reference)?.kind, NativeKind::FootnoteReference);
            let lineage = view.lineage(reference)?;
            let note = view.inline_footnote(lineage)?.unwrap();
            let mut cursor = view.source_cursor(note.body)?;
            let mut body = String::new();
            while let Some(piece) = view.next_source(&mut cursor)? {
                body.push_str(&source[piece.p.start..piece.p.end]);
            }
            assert_eq!(body, expected, "{source:?}");
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

    for (source, expected, dollars) in [
        ("^[a $`x$y]z`$ b]", "a $`x$y]z`$ b", true),
        ("^[a $`x`y]z`$ b]", "a $`x`y]z`$ b", false),
        ("^[a \\^[b](c]d) e]", "a \\^[b](c]d) e", false),
        (
            "^[[outer \\![inner](u)](x]y) end]",
            "[outer \\![inner](u)](x",
            false,
        ),
    ] {
        for policy in [InlinePolicy::Ordinary, InlinePolicy::StrictG4] {
            with_input(source, |input| {
                check(input, source, expected, dollars, policy)
            });
        }
    }
}

#[test]
fn image_boundary_probes_share_caret_images_and_keep_alt_footnotes_literal() {
    fn check<'input>(
        input: NativeInput<'input>,
        source: &str,
        expected_code: Option<&str>,
        expected_images: usize,
    ) {
        let mut host = TestHost::new(&input);
        let mut options = crate::Options::default();
        options.extension.footnotes = true;
        let inspect = |mut view: FinalView<'_, '_, '_, 'input, TestHost<'input>>| {
            let mut stack = vec![view.root()?];
            let mut code = Vec::new();
            let mut images = 0;
            while let Some(token) = stack.pop() {
                let row = view.node(token)?;
                if let Some(child) = row.first_child {
                    stack.push(child);
                }
                if let Some(next) = row.next_sibling {
                    stack.push(next);
                }
                if let NodeAttributes::InlineCode { literal, .. } = row.attributes {
                    code.push(text(&mut view, literal)?);
                } else if row.kind == NativeKind::Image {
                    images += 1;
                }
            }
            assert_eq!(
                code.iter().map(String::as_str).collect::<Vec<_>>(),
                expected_code.into_iter().collect::<Vec<_>>(),
                "{source:?}"
            );
            assert_eq!(images, expected_images, "{source:?}");
            Ok(())
        };
        compile_with_inline_html_policy(
            input,
            &options,
            &mut host,
            CallbackCompiler(Some(inspect)),
            InlinePolicy::Ordinary,
            InlineHtmlPolicy::RawText,
        )
        .unwrap();
        assert_eq!(host.live.get(), 0);
    }

    for (source, expected_code, expected_images) in [
        (
            "![outer [link <code>![^x](u)](v](outer)w)</code>",
            Some("![^x](u)](v](outer)w)"),
            0,
        ),
        (
            "![outer [link <code>[^x]](v](outer)w)</code>\n\n[^x]: Named.\n",
            Some("[^x]](v](outer)w)"),
            0,
        ),
    ] {
        with_input(source, |input| {
            check(input, source, expected_code, expected_images)
        });
    }
}
