use super::*;

#[test]
fn description_lists_loose() {
    html_opts!(
        [extension.description_lists],
        concat!(
            "Term 1\n",
            "\n",
            ": Definition 1\n",
            "\n",
            "Term 2 with *inline markup*\n",
            "\n",
            ": Definition 2\n"
        ),
        concat!(
            "<dl>\n",
            "<dt>Term 1</dt>\n",
            "<dd>\n",
            "<p>Definition 1</p>\n",
            "</dd>\n",
            "<dt>Term 2 with <em>inline markup</em></dt>\n",
            "<dd>\n",
            "<p>Definition 2</p>\n",
            "</dd>\n",
            "</dl>\n",
        ),
    );

    html_opts!(
        [extension.description_lists],
        concat!(
            "* Nested\n",
            "\n",
            "    Term 1\n\n",
            "    :   Definition 1\n\n",
            "    Term 2 with *inline markup*\n\n",
            "    :   Definition 2\n\n"
        ),
        concat!(
            "<ul>\n",
            "<li>\n",
            "<p>Nested</p>\n",
            "<dl>\n",
            "<dt>Term 1</dt>\n",
            "<dd>\n",
            "<p>Definition 1</p>\n",
            "</dd>\n",
            "<dt>Term 2 with <em>inline markup</em></dt>\n",
            "<dd>\n",
            "<p>Definition 2</p>\n",
            "</dd>\n",
            "</dl>\n",
            "</li>\n",
            "</ul>\n",
        ),
    );
}

#[test]
fn description_lists_tight() {
    html_opts!(
        [extension.description_lists],
        concat!(
            "Term 1\n",
            ": Definition 1\n",
            "\n",
            "Term 2 with *inline markup*\n",
            ": Definition 2\n"
        ),
        concat!(
            "<dl>\n",
            "<dt>Term 1</dt>\n",
            "<dd>Definition 1</dd>\n",
            "<dt>Term 2 with <em>inline markup</em></dt>\n",
            "<dd>Definition 2</dd>\n",
            "</dl>\n",
        ),
        no_roundtrip,
    );

    html_opts!(
        [extension.description_lists],
        concat!(
            "* Nested\n",
            "\n",
            "    Term 1\n",
            "    :   Definition 1\n\n",
            "    Term 2 with *inline markup*\n",
            "    :   Definition 2\n\n"
        ),
        concat!(
            "<ul>\n",
            "<li>\n",
            "<p>Nested</p>\n",
            "<dl>\n",
            "<dt>Term 1</dt>\n",
            "<dd>Definition 1</dd>\n",
            "<dt>Term 2 with <em>inline markup</em></dt>\n",
            "<dd>Definition 2</dd>\n",
            "</dl>\n",
            "</li>\n",
            "</ul>\n",
        ),
        no_roundtrip,
    );
}
#[test]
fn description_lists_edge_cases() {
    html_opts!([extension.description_lists], ":", "<p>:</p>\n",);

    html_opts!([extension.description_lists], ": foo", "<p>: foo</p>\n",);

    html_opts!([extension.description_lists], "a\n:", "<p>a\n:</p>\n",);

    html_opts!(
        [extension.description_lists],
        concat!("- foo\n", "- : bar\n", "  - baz\n",),
        concat!(
            "<ul>\n",
            "<li>foo</li>\n",
            "<li>: bar\n",
            "<ul>\n",
            "<li>baz</li>\n",
            "</ul>\n",
            "</li>\n",
            "</ul>\n",
        ),
    );
}

#[test]
fn sourcepos() {
    assert_ast_match!(
        [extension.description_lists],
        "ta\n"
        "\n"
        ": da\n"
        "\n"
        "t*b*\n"
        "\n"
        ": d*b*\n"
        "\n"
        "tc\n"
        "\n"
        ": dc\n",
        (document (1:1-11:4) [
        (description_list (1:1-11:4) [
            (description_item (1:1-3:4) [
                (description_term (1:1-1:2) [
                        (paragraph (1:1-1:2) [
                            (text (1:1-1:2) "ta")
                        ])
                    ])
                    (description_details (3:1-3:4) [
                        (paragraph (3:3-3:4) [
                            (text (3:3-3:4) "da")
                        ])
                    ])
                ])
                (description_item (5:1-7:6) [
                    (description_term (5:1-5:4) [
                        (paragraph (5:1-5:4) [
                            (text (5:1-5:1) "t")
                            (emph (5:2-5:4) [
                                (text (5:3-5:3) "b")
                            ])
                        ])
                    ])
                    (description_details (7:1-7:6) [
                        (paragraph (7:3-7:6) [
                            (text (7:3-7:3) "d")
                            (emph (7:4-7:6) [
                                (text (7:5-7:5) "b")
                            ])
                        ])
                    ])
                ])
                (description_item (9:1-11:4) [
                    (description_term (9:1-9:2) [
                        (paragraph (9:1-9:2) [
                            (text (9:1-9:2) "tc")
                        ])
                    ])
                    (description_details (11:1-11:4) [
                        (paragraph (11:3-11:4) [
                            (text (11:3-11:4) "dc")
                        ])
                    ])
                ])
            ])
        ])
    );
}
