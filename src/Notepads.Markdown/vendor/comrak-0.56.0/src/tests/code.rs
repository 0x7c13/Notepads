// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn indented_codeblock_trailing_blanks_sourcepos() {
    for newline in ["\n", "\r\n", "\r"] {
        for blanks in [0, 1, 3, 1024] {
            for suffix in ["", "next"] {
                let input = format!(
                    "a{newline}{newline}     b{newline}{}{suffix}",
                    newline.repeat(blanks)
                );
                let arena = Arena::new();
                let root = parse_document(&arena, &input, &Options::default());
                let data = root.children().nth(1).unwrap().data();
                let NodeValue::CodeBlock(code) = &data.value else {
                    panic!("expected an indented code block");
                };
                assert!(!code.fenced);
                assert_eq!(
                    code.literal.owned_str().expect("legacy code block literal"),
                    " b\n",
                    "{input:?}"
                );
                assert_eq!(data.sourcepos, sourcepos!((3:5-3:6)), "{input:?}");
            }
        }
    }
}

#[test]
fn indented_codeblock_content_end_sourcepos() {
    let cases = [
        ("    b", "b\n", sourcepos!((1:5-1:5)), " \t"),
        ("\tb", "b\n", sourcepos!((1:2-1:2)), " \t"),
        ("  \t\tb", "\tb\n", sourcepos!((1:4-1:5)), " \t"),
        ("> \t\tb", "  b\n", sourcepos!((1:4-1:5)), "> \t"),
        ("- a\n\n      b", "b\n", sourcepos!((3:7-3:7)), " \t"),
        (
            "    b\n\n    c  \t",
            "b\n\nc  \t\n",
            sourcepos!((1:5-3:8)),
            " \t",
        ),
        (
            ">     b\n>\n>     c  \t",
            "b\n\nc  \t\n",
            sourcepos!((1:7-3:10)),
            "> \t",
        ),
    ];
    for (input, literal, sourcepos, blank_prefix) in cases {
        for newline in ["\n", "\r\n", "\r"] {
            let expected_literal = format!(
                "{}\n",
                literal.strip_suffix('\n').unwrap().replace('\n', newline)
            );
            for blanks in [0, 1, 3] {
                for suffix in ["", "next"] {
                    let input = format!(
                        "{}{newline}{}{suffix}",
                        input.replace('\n', newline),
                        format!("{blank_prefix}{newline}").repeat(blanks)
                    );
                    let arena = Arena::new();
                    let root = parse_document(&arena, &input, &Options::default());
                    let node = root
                        .descendants()
                        .find(|node| matches!(node.data().value, NodeValue::CodeBlock(_)))
                        .unwrap();
                    let data = node.data();
                    let NodeValue::CodeBlock(code) = &data.value else {
                        unreachable!();
                    };
                    assert_eq!(
                        code.literal.owned_str().expect("legacy code block literal"),
                        expected_literal,
                        "{input:?}"
                    );
                    assert_eq!(data.sourcepos, sourcepos, "{input:?}");
                }
            }
        }
    }
}

#[test]
fn fenced_codeblock_closed_and_unclosed_root() {
    let arena = Arena::new();
    let options = Options::default();

    let md_closed = "```\nfn this_has_a_closing_fence() {}\n```\n";
    let root = parse_document(&arena, md_closed, &options);
    let mut found = false;
    for n in root.descendants() {
        if let NodeValue::CodeBlock(ncb) = &n.data().value {
            assert!(ncb.fenced, "expected fenced code block");
            assert!(ncb.closed, "expected closed code block");
            found = true;
            break;
        }
    }
    assert!(found, "expected a code block node");

    let md_unclosed = "```\nfn this_does_not() {}\n";
    let root2 = parse_document(&arena, md_unclosed, &options);
    let mut found2 = false;
    for n in root2.descendants() {
        if let NodeValue::CodeBlock(ncb) = &n.data().value {
            assert!(ncb.fenced, "expected fenced code block");
            assert!(!ncb.closed, "expected unclosed code block");
            found2 = true;
            break;
        }
    }
    assert!(found2, "expected a code block node");
}

#[test]
fn fenced_codeblock_closed_and_unclosed_in_blockquote() {
    let arena = Arena::new();
    let options = Options::default();

    let md_closed = "> ```\n> fn this_has_a_closing_fence() {}\n> ```\n";
    let root = parse_document(&arena, md_closed, &options);
    let mut found = false;
    for n in root.descendants() {
        if let NodeValue::CodeBlock(ncb) = &n.data().value {
            assert!(ncb.fenced, "expected fenced code block in blockquote");
            assert!(ncb.closed, "expected closed code block in blockquote");
            found = true;
            break;
        }
    }
    assert!(found, "expected a code block node");

    let md_unclosed = "> ```\n> paragraph\n";
    let root2 = parse_document(&arena, md_unclosed, &options);
    let mut found2 = false;
    for n in root2.descendants() {
        if let NodeValue::CodeBlock(ncb) = &n.data().value {
            assert!(ncb.fenced, "expected fenced code block in blockquote");
            assert!(!ncb.closed, "expected unclosed code block in blockquote");
            found2 = true;
            break;
        }
    }
    assert!(found2, "expected a code block node");
}

#[test]
fn fenced_codeblock_closed_sourcepos() {
    assert_ast_match!(
        [],
        "```\ncode\n```",
        (document (1:1-3:3) [
            (code_block (1:1-3:3) "code\n")
        ])
    );
}

#[test]
fn fenced_codeblock_unclosed_sourcepos() {
    assert_ast_match!(
        [],
        "```\ncode\n",
        (document (1:1-2:4) [
            (code_block (1:1-2:4) "code\n")
        ])
    );

    assert_ast_match!(
        [],
        "```\n> code\n",
        (document (1:1-2:6) [
            (code_block (1:1-2:6) "> code\n")
        ])
    );

    assert_ast_match!(
        [],
        "> ```\nparagraph\n",
        (document (1:1-2:9) [
            (block_quote (1:1-1:5) [
                (code_block (1:3-1:5) "")
            ])
            (paragraph (2:1-2:9) [
                (text (2:1-2:9) "paragraph")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "> ```\n> code\n",
        (document (1:1-2:6) [
            (block_quote (1:1-2:6) [
                (code_block (1:3-2:6) "code\n")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "* ```\nparagraph\n",
        (document (1:1-2:9) [
            (list (1:1-1:5) [
                (item (1:1-1:5) [
                    (code_block (1:3-1:5) "")
                ])
            ])
            (paragraph (2:1-2:9) [
                (text (2:1-2:9) "paragraph")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "```\n* code\n",
        (document (1:1-2:6) [
            (code_block (1:1-2:6) "* code\n")
        ])
    );

    assert_ast_match!(
        [],
        "* ```\n* paragraph\n",
        (document (1:1-2:11) [
            (list (1:1-2:11) [
                (item (1:1-1:5) [
                    (code_block (1:3-1:5) "")
                ])
                (item (2:1-2:11) [
                    (paragraph (2:3-2:11) [
                        (text (2:3-2:11) "paragraph")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn closed_list_between_fenced_codeblocks_sourcepos() {
    assert_ast_match!(
        [],
        "```\n"
        "code\n"
        "```\n"
        "- list\n"
        "```\n"
        "code\n"
        "```\n",
        (document (1:1-7:3) [
            (code_block (1:1-3:3) "code\n")
            (list (4:1-4:6) [
                (item (4:1-4:6) [
                    (paragraph (4:3-4:6) [
                        (text (4:3-4:6) "list")
                    ])
                ])
            ])
            (code_block (5:1-7:3) "code\n")
        ])
    );
}

#[test]
fn closed_list_before_fenced_codeblocks_sourcepos() {
    assert_ast_match!(
        [],
        "- list\n"
        "```\n"
        "code\n"
        "```\n",
        (document (1:1-4:3) [
            (list (1:1-1:6) [
                (item (1:1-1:6) [
                    (paragraph (1:3-1:6) [
                        (text (1:3-1:6) "list")
                    ])
                ])
            ])
            (code_block (2:1-4:3) "code\n")
        ])
    );
}

#[test]
fn list_ending_in_table_sourcepos() {
    assert_ast_match!(
        [extension.table],
        "- | a | b |\n"
        "  |---|---|\n"
        "  | c | dd |\n"
        "\n"
        "foo\n",
        (document (1:1-5:3) [
            (list (1:1-3:12) [
                (item (1:1-3:12) [
                    (table (1:3-3:12) [
                        (table_row (1:3-1:11) [
                            (table_cell (1:4-1:6) [
                                (text (1:5-1:5) "a")
                            ])
                            (table_cell (1:8-1:10) [
                                (text (1:9-1:9) "b")
                            ])
                        ])
                        (table_row (3:3-3:12) [
                            (table_cell (3:4-3:6) [
                                (text (3:5-3:5) "c")
                            ])
                            (table_cell (3:8-3:11) [
                                (text (3:9-3:10) "dd")
                            ])
                        ])
                    ])
                ])
            ])
            (paragraph (5:1-5:3) [
                (text (5:1-5:3) "foo")
            ])
        ])
    );
}

#[test]
fn list_ending_in_unclosed_multiline_block_quote_sourcepos() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        "- >>>\n"
        "foo\n"
        "bar\n",
        (document (1:1-3:3) [
            (list (1:1-2:3) [
                (item (1:1-1:5) [
                    (multiline_block_quote (1:3-2:3))
                ])
            ])
            (paragraph (2:1-3:3) [
                (text (2:1-2:3) "foo")
                (softbreak (2:4-2:4))
                (text (3:1-3:3) "bar")
            ])
        ])
    );
}

#[test]
fn closed_list_after_fenced_codeblocks_sourcepos() {
    assert_ast_match!(
        [],
        "```\n"
        "code\n"
        "```\n"
        "- list\n",
        (document (1:1-4:6) [
            (code_block (1:1-3:3) "code\n")
            (list (4:1-4:6) [
                (item (4:1-4:6) [
                    (paragraph (4:3-4:6) [
                        (text (4:3-4:6) "list")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn nested_list_between_fenced_codeblocks_sourcepos() {
    assert_ast_match!(
        [],
        "```\n"
        "code\n"
        "```\n"
        "1. list\n"
        "    * nested list\n"
        "```\n"
        "code\n"
        "```\n",
        (document (1:1-8:3) [
            (code_block (1:1-3:3) "code\n")
            (list (4:1-5:17) [
                (item (4:1-5:17) [
                    (paragraph (4:4-4:7) [
                        (text (4:4-4:7) "list")
                    ])
                    (list (5:5-5:17) [
                        (item (5:5-5:17) [
                            (paragraph (5:7-5:17) [
                                (text (5:7-5:17) "nested list")
                            ])
                        ])
                    ])
                ])
            ])
            (code_block (6:1-8:3) "code\n")
        ])
    );
}

#[test]
fn nested_list_before_fenced_codeblock_sourcepos() {
    assert_ast_match!(
        [],
        "1. list\n"
        "    * nested list\n"
        "```\n"
        "code\n"
        "```\n",
        (document (1:1-5:3) [
            (list (1:1-2:17) [
                (item (1:1-2:17) [
                    (paragraph (1:4-1:7) [
                        (text (1:4-1:7) "list")
                    ])
                    (list (2:5-2:17) [
                        (item (2:5-2:17) [
                            (paragraph (2:7-2:17) [
                                (text (2:7-2:17) "nested list")
                            ])
                        ])
                    ])
                ])
            ])
            (code_block (3:1-5:3) "code\n")
        ])
    );
}

#[test]
fn nested_list_after_fenced_codeblock_sourcepos() {
    assert_ast_match!(
        [],
        "```\n"
        "code\n"
        "```\n"
        "1. list\n"
        "    * nested list\n",
        (document (1:1-5:17) [
            (code_block (1:1-3:3) "code\n")
            (list (4:1-5:17) [
                (item (4:1-5:17) [
                    (paragraph (4:4-4:7) [
                        (text (4:4-4:7) "list")
                    ])
                    (list (5:5-5:17) [
                        (item (5:5-5:17) [
                            (paragraph (5:7-5:17) [
                                (text (5:7-5:17) "nested list")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}
