use super::*;
use pretty_assertions::assert_eq;

#[test]
fn sourcepos_with_differently_indented_rows() {
    assert_ast_match!(
        [extension.table],
        "p\n"
        "      |a|b|\n"
        "|-|-|\n"
        "|c|d|\n"
        "  |e|\n"
        "#\n"
        ,
        (document (1:1-6:1) [
            (paragraph (1:1-1:1) [
                (text (1:1-1:1) "p")
            ])
            (table (2:7-5:5) [
                (table_row (2:7-2:11) [
                    (table_cell (2:8-2:8) [(text (2:8-2:8) "a")])
                    (table_cell (2:10-2:10) [(text (2:10-2:10) "b")])
                ])
                (table_row (4:1-4:5) [
                    (table_cell (4:2-4:2) [(text (4:2-4:2) "c")])
                    (table_cell (4:4-4:4) [(text (4:4-4:4) "d")])
                ])
                (table_row (5:3-5:5) [
                    (table_cell (5:4-5:4) [(text (5:4-5:4) "e")])
                    (table_cell (5:5-5:5))
                ])
            ])
            (heading (6:1-6:1))
        ])
    );
}

#[test]
fn sourcepos_with_indented_rows_in_containers() {
    let mut options = Options::default();
    options.extension.table = true;
    for newline in ["\n", "\r\n", "\r"] {
        for (first, prefix) in [("", ""), ("> ", "> "), ("- ", "  ")] {
            for header_indent in 0..=3 {
                for row_indent in 0..=3 {
                    for body in [false, true] {
                        let mut input = format!(
                            "{first}p{newline}{prefix}{}|a|b|{newline}{prefix}|-|-|{newline}",
                            " ".repeat(header_indent),
                        );
                        if body {
                            input.push_str(&format!(
                                "{prefix}{}|c|{newline}",
                                " ".repeat(row_indent),
                            ));
                        }
                        input.push_str(&format!("{prefix}#{newline}"));
                        let arena = Arena::new();
                        let root = parse_document(&arena, &input, &options);
                        let table = root
                            .descendants()
                            .find(|node| matches!(node.data().value, NodeValue::Table(..)))
                            .unwrap();
                        let end_column = prefix.len() + if body { row_indent + 3 } else { 5 };
                        assert_eq!(
                            table.data().sourcepos,
                            (
                                2,
                                prefix.len() + header_indent + 1,
                                if body { 4 } else { 3 },
                                end_column
                            )
                                .into(),
                            "{input:?}",
                        );
                        if body {
                            let row = table.last_child().unwrap();
                            let start_column = prefix.len() + row_indent + 1;
                            assert_eq!(
                                row.data().sourcepos,
                                (4, start_column, 4, end_column).into(),
                                "{input:?}"
                            );
                            let cell = row.first_child().unwrap();
                            assert_eq!(
                                cell.data().sourcepos,
                                (4, start_column + 1, 4, start_column + 1).into(),
                                "{input:?}"
                            );
                            assert_eq!(
                                cell.first_child().unwrap().data().sourcepos,
                                cell.data().sourcepos,
                                "{input:?}"
                            );
                            assert_eq!(
                                row.last_child().unwrap().data().sourcepos,
                                (4, end_column, 4, end_column).into(),
                                "{input:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn table() {
    html_opts!(
        [extension.table],
        concat!("| a | b |\n", "|---|:-:|\n", "| c | d |\n"),
        concat!(
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>a</th>\n",
            "<th align=\"center\">b</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td>c</td>\n",
            "<td align=\"center\">d</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n"
        ),
    );
}

#[test]
fn table_regression() {
    html_opts!(
        [extension.table],
        concat!("123\n", "456\n", "| a | b |\n", "| ---| --- |\n", "d | e\n"),
        concat!(
            "<p>123\n",
            "456</p>\n",
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>a</th>\n",
            "<th>b</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td>d</td>\n",
            "<td>e</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n"
        ),
    );
}

#[test]
fn table_misparse_1() {
    html_opts!([extension.table], "a\n-b", "<p>a\n-b</p>\n");
}

#[test]
fn table_misparse_2() {
    html_opts!([extension.table], "a\n-b\n-c", "<p>a\n-b\n-c</p>\n");
}

#[test]
fn nested_tables_1() {
    html_opts!(
        [extension.table],
        concat!("- p\n", "\n", "    |a|b|\n", "    |-|-|\n", "    |c|d|\n",),
        concat!(
            "<ul>\n",
            "<li>\n",
            "<p>p</p>\n",
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>a</th>\n",
            "<th>b</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td>c</td>\n",
            "<td>d</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n",
            "</li>\n",
            "</ul>\n",
        ),
    );
}

#[test]
fn nested_tables_2() {
    html_opts!(
        [extension.table],
        concat!("- |a|b|\n", "  |-|-|\n", "  |c|d|\n",),
        concat!(
            "<ul>\n",
            "<li>\n",
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>a</th>\n",
            "<th>b</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td>c</td>\n",
            "<td>d</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n",
            "</li>\n",
            "</ul>\n",
        ),
    );
}

#[test]
fn nested_tables_3() {
    html_opts!(
        [extension.table],
        concat!("> |a|b|\n", "> |-|-|\n", "> |c|d|\n",),
        concat!(
            "<blockquote>\n",
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>a</th>\n",
            "<th>b</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td>c</td>\n",
            "<td>d</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n",
            "</blockquote>\n",
        ),
    );
}

#[test]
fn sourcepos_with_preceding_para_no_spaces() {
    assert_ast_match!(
        [extension.table],
        "123\n"
        "456\n"
        "| a | b |\n"
        "| - | - |\n"
        "| c | d |\n"
        ,
        (document (1:1-5:9) [
            (paragraph (1:1-2:3) [
                (text (1:1-1:3) "123")
                (softbreak (1:4-1:4))
                (text (2:1-2:3) "456")
            ])
            (table (3:1-5:9) [
                (table_row (3:1-3:9) [
                    (table_cell (3:2-3:4) [
                        (text (3:3-3:3) "a")
                    ])
                    (table_cell (3:6-3:8) [
                        (text (3:7-3:7) "b")
                    ])
                ])
                (table_row (5:1-5:9) [
                    (table_cell (5:2-5:4) [
                        (text (5:3-5:3) "c")
                    ])
                    (table_cell (5:6-5:8) [
                        (text (5:7-5:7) "d")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_preceding_para_spaces_before_table() {
    assert_ast_match!(
        [extension.table],
        "123\n"
        "456\n"
        " | a | b |\n"
        " | - | - |\n"
        " | c | d |\n"
        ,
        (document (1:1-5:10) [
            (paragraph (1:1-2:3) [
                (text (1:1-1:3) "123")
                (softbreak (1:4-1:4))
                (text (2:1-2:3) "456")
            ])
            (table (3:2-5:10) [
                (table_row (3:2-3:10) [
                    (table_cell (3:3-3:5) [
                        (text (3:4-3:4) "a")
                    ])
                    (table_cell (3:7-3:9) [
                        (text (3:8-3:8) "b")
                    ])
                ])
                (table_row (5:2-5:10) [
                    (table_cell (5:3-5:5) [
                        (text (5:4-5:4) "c")
                    ])
                    (table_cell (5:7-5:9) [
                        (text (5:8-5:8) "d")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_preceding_para_spaces_before_para() {
    assert_ast_match!(
        [extension.table],
        " 123\n"
        "  456\n"
        "| a | b |\n"
        "| - | - |\n"
        "| c | d |\n"
        ,
        (document (1:1-5:9) [
            (paragraph (1:2-2:5) [
                (text (1:2-1:4) "123")
                (softbreak (1:5-1:5))
                (text (2:3-2:5) "456")
            ])
            (table (3:1-5:9) [
                (table_row (3:1-3:9) [
                    (table_cell (3:2-3:4) [
                        (text (3:3-3:3) "a")
                    ])
                    (table_cell (3:6-3:8) [
                        (text (3:7-3:7) "b")
                    ])
                ])
                (table_row (5:1-5:9) [
                    (table_cell (5:2-5:4) [
                        (text (5:3-5:3) "c")
                    ])
                    (table_cell (5:6-5:8) [
                        (text (5:7-5:7) "d")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_preceding_para_spaces_before_both() {
    assert_ast_match!(
        [extension.table],
        " 123\n"
        "  456\n"
        " | a | b |\n"
        " | - | - |\n"
        " | c | d |\n"
        ,
        (document (1:1-5:10) [
            (paragraph (1:2-2:5) [
                (text (1:2-1:4) "123")
                (softbreak (1:5-1:5))
                (text (2:3-2:5) "456")
            ])
            (table (3:2-5:10) [
                (table_row (3:2-3:10) [
                    (table_cell (3:3-3:5) [
                        (text (3:4-3:4) "a")
                    ])
                    (table_cell (3:7-3:9) [
                        (text (3:8-3:8) "b")
                    ])
                ])
                (table_row (5:2-5:10) [
                    (table_cell (5:3-5:5) [
                        (text (5:4-5:4) "c")
                    ])
                    (table_cell (5:7-5:9) [
                        (text (5:8-5:8) "d")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_preceding_para_mixed_line_endings() {
    assert_ast_match!(
        [extension.table],
        "|-\r\nb\n|-",
        (document (1:1-3:2) [
            (paragraph (1:1-1:2) [
                (text (1:1-1:2) "|-")
            ])
            (table (2:1-3:2) [
                (table_row (2:1-2:1) [
                    (table_cell (2:1-2:1) [
                        (text (2:1-2:1) "b")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_preceding_para_line_endings() {
    let mut options = Options::default();
    options.extension.table = true;

    for line_ending in ["\n", "\r", "\r\n"] {
        for (input, expected) in [
            ("a|b\nc\n ---:|\n", (1, 1, 1, 3)),
            ("  abc\nc\n|-", (1, 3, 1, 5)),
            ("abc\n  xyz\nc\n|-", (1, 1, 2, 5)),
            ("é中文\nc\n|-", (1, 1, 1, 8)),
            ("> abc\n> c\n> |-", (1, 3, 1, 5)),
            ("    code\n\nabc\nc\n|-", (3, 1, 3, 3)),
        ] {
            let input = input.replace('\n', line_ending);
            let arena = Arena::new();
            let root = parse_document(&arena, &input, &options);
            let paragraph = root
                .descendants()
                .find(|node| matches!(node.data().value, NodeValue::Paragraph))
                .unwrap();

            assert_eq!(paragraph.data().sourcepos, expected.into(), "{input:?}");
            assert!(matches!(
                paragraph.next_sibling().unwrap().data().value,
                NodeValue::Table(..)
            ));
        }
    }
}

#[test]
fn table_crlf() {
    assert_ast_match!(
        [extension.table],
        "| a | b |\r\n"
        "| - | - |\r\n"
        "| c | d |\r\n"
        ,
        (document (1:1-3:9) [
            (table (1:1-3:9) [
                (table_row (1:1-1:9) [
                    (table_cell (1:2-1:4) [
                        (text (1:3-1:3) "a")
                    ])
                    (table_cell (1:6-1:8) [
                        (text (1:7-1:7) "b")
                    ])
                ])
                (table_row (3:1-3:9) [
                    (table_cell (3:2-3:4) [
                        (text (3:3-3:3) "c")
                    ])
                    (table_cell (3:6-3:8) [
                        (text (3:7-3:7) "d")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn table_cr() {
    assert_ast_match!(
        [extension.table],
        "| a | b |\r"
        "| - | - |\r"
        "| c | d |\r"
        ,
        (document (1:1-3:9) [
            (table (1:1-3:9) [
                (table_row (1:1-1:9) [
                    (table_cell (1:2-1:4) [
                        (text (1:3-1:3) "a")
                    ])
                    (table_cell (1:6-1:8) [
                        (text (1:7-1:7) "b")
                    ])
                ])
                (table_row (3:1-3:9) [
                    (table_cell (3:2-3:4) [
                        (text (3:3-3:3) "c")
                    ])
                    (table_cell (3:6-3:8) [
                        (text (3:7-3:7) "d")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn table_no_body_no_eol_at_eof() {
    assert_ast_match!(
        [extension.table],
        "| a |\n"
        "| - |"
        ,
        (document (1:1-2:5) [
            (table (1:1-2:5) [
                (table_row (1:1-1:5) [
                    (table_cell (1:2-1:4) [
                        (text (1:3-1:3) "a")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn table_with_trailing_para() {
    assert_ast_match!(
        [extension.table],
        "| a |\n"
        "| - |\n"
        "\n"
        "ok"
        ,
        (document (1:1-4:2) [
            (table (1:1-2:5) [
                (table_row (1:1-1:5) [
                    (table_cell (1:2-1:4) [
                        (text (1:3-1:3) "a")
                    ])
                ])
            ])
            (paragraph (4:1-4:2) [
                (text (4:1-4:2) "ok")
            ])
        ])
    );
}

#[test]
fn table_with_trailing_para_and_leading_whitespace() {
    assert_ast_match!(
        [extension.table],
        "  | a  |\n"
        "  | - |\n"
        "\n"
        " ok"
        ,
        (document (1:1-4:3) [
            (table (1:3-2:7) [
                (table_row (1:3-1:8) [
                    (table_cell (1:4-1:7) [
                        (text (1:5-1:5) "a")
                    ])
                ])
            ])
            (paragraph (4:2-4:3) [
                (text (4:2-4:3) "ok")
            ])
        ])
    );
}

#[test]
fn table_missing_cell_sourcepos() {
    assert_ast_match!(
        [extension.table],
        "|a|b|\n"
        "|-|-|\n"
        "|c|\n"
        ,
        (document (1:1-3:3) [
            (table (1:1-3:3) [
                (table_row (1:1-1:5) [
                    (table_cell (1:2-1:2) [
                        (text (1:2-1:2) "a")
                    ])
                    (table_cell (1:4-1:4) [
                        (text (1:4-1:4) "b")
                    ])
                ])
                (table_row (3:1-3:3) [
                    (table_cell (3:2-3:2) [
                        (text (3:2-3:2) "c")
                    ])
                    (table_cell (3:3-3:3)
                    )
                ])
            ])
        ])
    );
}

#[test]
fn table_missing_cell_with_text_sourcepos() {
    assert_ast_match!(
        [extension.table],
        "|a|b|\n"
        "|-|-|\n"
        "|c|d\n"
        ,
        (document (1:1-3:4) [
            (table (1:1-3:4) [
                (table_row (1:1-1:5) [
                    (table_cell (1:2-1:2) [
                        (text (1:2-1:2) "a")
                    ])
                    (table_cell (1:4-1:4) [
                        (text (1:4-1:4) "b")
                    ])
                ])
                (table_row (3:1-3:4) [
                    (table_cell (3:2-3:2) [
                        (text (3:2-3:2) "c")
                    ])
                    (table_cell (3:4-3:4) [
                        (text (3:4-3:4) "d")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn table_missing_cells_sourcepos() {
    assert_ast_match!(
        [extension.table],
        "|a|b|c|\n"
        "|-|-|-|\n"
        "|d|\n"
        ,
        (document (1:1-3:3) [
            (table (1:1-3:3) [
                (table_row (1:1-1:7) [
                    (table_cell (1:2-1:2) [
                        (text (1:2-1:2) "a")
                    ])
                    (table_cell (1:4-1:4) [
                        (text (1:4-1:4) "b")
                    ])
                    (table_cell (1:6-1:6) [
                        (text (1:6-1:6) "c")
                    ])
                ])
                (table_row (3:1-3:3) [
                    (table_cell (3:2-3:2) [
                        (text (3:2-3:2) "d")
                    ])
                    (table_cell (3:3-3:3)
                    )
                    (table_cell (3:3-3:3)
                    )
                ])
            ])
        ])
    );
}
