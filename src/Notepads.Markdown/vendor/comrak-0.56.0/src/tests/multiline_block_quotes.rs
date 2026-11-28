use super::*;

#[test]
fn multiline_block_quotes() {
    html_opts!(
        [extension.multiline_block_quotes],
        concat!(">>>\n", "Paragraph 1\n", "\n", "Paragraph 2\n", ">>>\n",),
        concat!(
            "<blockquote>\n",
            "<p>Paragraph 1</p>\n",
            "<p>Paragraph 2</p>\n",
            "</blockquote>\n",
        ),
    );

    html_opts!(
        [extension.multiline_block_quotes],
        concat!(
            "- item one\n",
            "\n",
            "  >>>\n",
            "  Paragraph 1\n",
            "\n",
            "  Paragraph 2\n",
            "  >>>\n",
            "- item two\n"
        ),
        concat!(
            "<ul>\n",
            "<li>\n",
            "<p>item one</p>\n",
            "<blockquote>\n",
            "<p>Paragraph 1</p>\n",
            "<p>Paragraph 2</p>\n",
            "</blockquote>\n",
            "</li>\n",
            "<li>\n",
            "<p>item two</p>\n",
            "</li>\n",
            "</ul>\n",
        ),
    );
}

#[test]
fn sourcepos() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        "- item one\n"
        "\n"
        "  >>>\n"
        "  Paragraph 1\n"
        "  >>>\n"
        "- item two\n",
        (document (1:1-6:10) [
            (list (1:1-6:10) [
                (item (1:1-5:5) [
                    (paragraph (1:3-1:10) [
                        (text (1:3-1:10) "item one")
                    ])
                    (multiline_block_quote (3:3-5:5) [
                        (paragraph (4:3-4:13) [
                            (text (4:3-4:13) "Paragraph 1")
                        ])
                    ])
                ])
                (item (6:1-6:10) [
                    (paragraph (6:3-6:10) [
                        (text (6:3-6:10) "item two")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_block_quote() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        ">>>\n"
        "> a\n"
        ">>>\n",
        (document (1:1-3:3) [
            (multiline_block_quote (1:1-3:3) [
                (block_quote (2:1-2:3) [
                    (paragraph (2:3-2:3) [
                        (text (2:3-2:3) "a")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_block_quote_and_para() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        ">>>\n"
        "> a\n"
        "\n"
        "b\n"
        ">>>\n",
        (document (1:1-5:3) [
            (multiline_block_quote (1:1-5:3) [
                (block_quote (2:1-2:3) [
                    (paragraph (2:3-2:3) [
                        (text (2:3-2:3) "a")
                    ])
                ])
                (paragraph (4:1-4:1) [
                    (text (4:1-4:1) "b")
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_list() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        ">>>\n"
        "* a\n"
        ">>>\n",
        (document (1:1-3:3) [
            (multiline_block_quote (1:1-3:3) [
                (list (2:1-2:3) [
                    (item (2:1-2:3) [
                        (paragraph (2:3-2:3) [
                            (text (2:3-2:3) "a")
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_list_and_para() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        ">>>\n"
        "* a\n"
        "\n"
        "b\n"
        ">>>\n",
        (document (1:1-5:3) [
            (multiline_block_quote (1:1-5:3) [
                (list (2:1-2:3) [
                    (item (2:1-2:3) [
                        (paragraph (2:3-2:3) [
                            (text (2:3-2:3) "a")
                        ])
                    ])
                ])
                (paragraph (4:1-4:1) [
                    (text (4:1-4:1) "b")
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_block_quote_with_list() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        ">>>\n"
        "> * a\n"
        ">>>\n",
        (document (1:1-3:3) [
            (multiline_block_quote (1:1-3:3) [
                (block_quote (2:1-2:5) [
                    (list (2:3-2:5) [
                        (item (2:3-2:5) [
                            (paragraph (2:5-2:5) [
                                (text (2:5-2:5) "a")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_block_quote_with_list_and_para() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        ">>>\n"
        "> * a\n"
        "\n"
        "b\n"
        ">>>\n",
        (document (1:1-5:3) [
            (multiline_block_quote (1:1-5:3) [
                (block_quote (2:1-2:5) [
                    (list (2:3-2:5) [
                        (item (2:3-2:5) [
                            (paragraph (2:5-2:5) [
                                (text (2:5-2:5) "a")
                            ])
                        ])
                    ])
                ])
                (paragraph (4:1-4:1) [
                    (text (4:1-4:1) "b")
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_with_block_quote_with_block_quote_and_list() {
    assert_ast_match!(
        [extension.multiline_block_quotes],
        ">>>\n"
        ">> a\n"
        ">> * b\n"
        ">>>\n",
        (document (1:1-4:3) [
            (multiline_block_quote (1:1-4:3) [
                (block_quote (2:1-3:6) [
                    (block_quote (2:2-3:6) [
                        (paragraph (2:4-2:4) [
                            (text (2:4-2:4) "a")
                        ])
                        (list (3:4-3:6) [
                            (item (3:4-3:6) [
                                (paragraph (3:6-3:6) [
                                    (text (3:6-3:6) "b")
                                ])
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn html_block_in_blockquote_in_mbq() {
    html_opts!(
        [extension.multiline_block_quotes, render.r#unsafe],
        concat!(">>>\n", "<div>test</div>\n", ">>>\n"),
        concat!("<blockquote>\n", "<div>test</div>\n", "</blockquote>\n",),
    );

    html_opts!(
        [extension.multiline_block_quotes, render.r#unsafe],
        concat!(">>>\n", "> <div>test</div>\n", ">>>\n"),
        concat!(
            "<blockquote>\n",
            "<blockquote>\n",
            "<div>test</div>\n",
            "</blockquote>\n",
            "</blockquote>\n",
        ),
    );
}

#[test]
fn mbq_as_list_item_with_list_item() {
    html_opts!(
        [extension.multiline_block_quotes, render.r#unsafe],
        concat!(" -  >>>\n", "    - foo\n", "    >>>"),
        concat!(
            "<ul>\n",
            "<li>\n",
            "<blockquote>\n",
            "<ul>\n",
            "<li>foo</li>\n",
            "</ul>\n",
            "</blockquote>\n",
            "</li>\n",
            "</ul>\n",
        ),
    );
}
