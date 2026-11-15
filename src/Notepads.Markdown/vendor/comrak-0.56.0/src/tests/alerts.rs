use super::*;

#[test]
fn alerts() {
    html_opts!(
        [extension.alerts],
        concat!("> [!note]\n", "> Pay attention\n",),
        concat!(
            "<div class=\"markdown-alert markdown-alert-note\">\n",
            "<p class=\"markdown-alert-title\">Note</p>\n",
            "<p>Pay attention</p>\n",
            "</div>\n",
        ),
    );
}

#[test]
fn multiline_alerts() {
    html_opts!(
        [extension.alerts, extension.multiline_block_quotes],
        concat!(">>> [!note]\n", "Pay attention\n", ">>>",),
        concat!(
            "<div class=\"markdown-alert markdown-alert-note\">\n",
            "<p class=\"markdown-alert-title\">Note</p>\n",
            "<p>Pay attention</p>\n",
            "</div>\n",
        ),
    );
}

#[test]
fn semantic_alerts() {
    html_opts!(
        [
            extension.alerts = true,
            render.alert_style = options::AlertStyleType::Semantic
        ],
        concat!("> [!note]\n", "> Pay attention\n",),
        concat!(
            "<aside class=\"admonition note\">\n",
            "<p class=\"admonition-title\">Note</p>\n",
            "<p>Pay attention</p>\n",
            "</aside>\n",
        ),
    );
}

#[test]
fn sourcepos() {
    assert_ast_match!(
        [extension.alerts],
        "> [!note]\n"
        "> Pay attention\n",
        (document (1:1-2:15) [
            (alert (1:1-2:15) [
                (paragraph (2:3-2:15) [
                    (text (2:3-2:15) "Pay attention")
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_in_list() {
    assert_ast_match!(
        [extension.alerts],
        "- item one\n"
        "\n"
        "  > [!note]\n"
        "  > Pay attention\n",
        (document (1:1-4:17) [
            (list (1:1-4:17) [
                (item (1:1-4:17) [
                    (paragraph (1:3-1:10) [
                        (text (1:3-1:10) "item one")
                    ])
                    (alert (3:3-4:17) [
                        (paragraph (4:5-4:17) [
                            (text (4:5-4:17) "Pay attention")
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_multiline() {
    assert_ast_match!(
        [extension.alerts, extension.multiline_block_quotes],
        ">>> [!note]\n"
        "Pay attention\n"
        ">>>\n",
        (document (1:1-3:3) [
            (alert (1:1-2:13) [
                (paragraph (2:1-2:13) [
                    (text (2:1-2:13) "Pay attention")
                ])
            ])
        ])
    );
}

#[test]
fn surprise_eof() {
    assert_ast_match!(
        [extension.alerts],
        "> [!note]\n",
        (document (1:1-1:9) [
            (alert (1:1-1:9))
        ])
    );

    assert_ast_match!(
        [extension.alerts],
        "> [!note]",
        (document (1:1-1:9) [
            (alert (1:1-1:9))
        ])
    );
}
