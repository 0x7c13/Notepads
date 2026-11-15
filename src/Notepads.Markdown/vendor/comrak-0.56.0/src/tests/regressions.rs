use super::*;

#[test]
fn pointy_brace() {
    html_opts!(
        [render.r#unsafe],
        concat!(
            "URI autolink: <https://www.pixiv.net>\n",
            "\n",
            "Email autolink: <bill@microsoft.com>\n",
            "\n",
            "* Inline <em>tag</em> **ha**.\n",
            "* Inline <!-- comment --> **ha**.\n",
            "* Inline <? processing instruction ?> **ha**.\n",
            "* Inline <!DECLARATION OKAY> **ha**.\n",
            "* Inline <![CDATA[ok]ha **ha** ]]> **ha**.\n"
        ),
        concat!(
            "<p>URI autolink: <a \
             href=\"https://www.pixiv.net\">https://www.pixiv.net</a></p>\n",
            "<p>Email autolink: <a \
             href=\"mailto:bill@microsoft.com\">bill@microsoft.com</a></p>\n",
            "<ul>\n",
            "<li>Inline <em>tag</em> <strong>ha</strong>.</li>\n",
            "<li>Inline <!-- comment --> <strong>ha</strong>.</li>\n",
            "<li>Inline <? processing instruction ?> <strong>ha</strong>.</li>\n",
            "<li>Inline <!DECLARATION OKAY> <strong>ha</strong>.</li>\n",
            "<li>Inline <![CDATA[ok]ha **ha** ]]> <strong>ha</strong>.</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn no_control_characters_in_reference_links() {
    html(
        "[A]:\u{1b}\n\nX [A] Y\n",
        "<p>[A]:\u{1b}</p>\n<p>X [A] Y</p>\n",
    )
}

#[test]
fn link_entity_regression() {
    html(
        "[link](&#x6A&#x61&#x76&#x61&#x73&#x63&#x72&#x69&#x70&#x74&#x3A&#x61&#x6C&#x65&#x72&#x74&#x28&#x27&#x58&#x53&#x53&#x27&#x29)",
        "<p><a href=\"&amp;#x6A&amp;#x61&amp;#x76&amp;#x61&amp;#x73&amp;#x63&amp;#x72&amp;#x69&amp;#x70&amp;#x74&amp;#x3A&amp;#x61&amp;#x6C&amp;#x65&amp;#x72&amp;#x74&amp;#x28&amp;#x27&amp;#x58&amp;#x53&amp;#x53&amp;#x27&amp;#x29\">link</a></p>\n",
    );
}

#[test]
fn regression_back_to_back_ranges() {
    html(
        "**bold*****bold+italic***",
        "<p><strong>bold</strong><em><strong>bold+italic</strong></em></p>\n",
    );
}

#[test]
fn no_panic_on_empty_bookended_atx_headers() {
    html("#  #", "<h1></h1>\n");
}

#[test]
fn no_stack_smash_html() {
    let s: String = ">".repeat(150_000);
    let arena = Arena::new();
    let root = parse_document(&arena, &s, &Options::default());
    let mut output = String::new();
    html::format_document(root, &Options::default(), &mut output).unwrap()
}

#[test]
fn no_stack_smash_cm() {
    let s: String = ">".repeat(150_000);
    let arena = Arena::new();
    let root = parse_document(&arena, &s, &Options::default());
    let mut output = String::new();
    cm::format_document(root, &Options::default(), &mut output).unwrap()
}

#[test]
fn cm_autolink_regression() {
    // Testing that the cm renderer handles this case without crashing
    html("<a+c:dd>", "<p><a href=\"a+c:dd\">a+c:dd</a></p>\n");
}

#[test]
fn regression_424() {
    html(
        "*text* [link](#section)",
        "<p><em>text</em> <a href=\"#section\">link</a></p>\n",
    );
}

#[test]
fn example_61() {
    html(
        r##"
`Foo
----
`

<a title="a lot
---
of dashes"/>
"##,
        r##"<h2>`Foo</h2>
<p>`</p>
<h2>&lt;a title=&quot;a lot</h2>
<p>of dashes&quot;/&gt;</p>
"##,
    );
}

#[test]
fn nul_at_eof() {
    html("foo\0", "<p>foo\u{fffd}</p>\n");
    html("foo\0nl", "<p>foo\u{fffd}nl</p>\n");
    html("foo\0nl\0", "<p>foo\u{fffd}nl\u{fffd}</p>\n");
}

#[test]
fn sourcepos_para() {
    html_opts!(
        [render.sourcepos],
        "abc\ndef\n\nghi\n",
        "<p data-sourcepos=\"1:1-2:3\">abc\ndef</p>\n<p data-sourcepos=\"4:1-4:3\">ghi</p>\n",
    );
}

#[test]
#[cfg(feature = "shortcodes")]
fn gemoji() {
    html_opts!([extension.shortcodes], ":x:", "<p>❌</p>\n");
}

#[test]
fn sourcepos_lone_backtick() {
    assert_ast_match!(
        [],
        "``\n",
        (document (1:1-1:2) [
            (paragraph (1:1-1:2) [
                (text (1:1-1:2) "``")
            ])
        ])
    );
}

#[test]
fn sourcepos_link_items() {
    assert_ast_match!(
        [],
        "- ab\n"
        "- cdef\n"
        "\n"
        "\n"
        "g\n"
        ,
        (document (1:1-5:1) [
            (list (1:1-2:6) [
                (item (1:1-1:4) [
                    (paragraph (1:3-1:4) [
                        (text (1:3-1:4) "ab")
                    ])
                ])
                (item (2:1-2:6) [
                    (paragraph (2:3-2:6) [
                        (text (2:3-2:6) "cdef")
                    ])
                ])
            ])
            (paragraph (5:1-5:1) [
                (text (5:1-5:1) "g")
            ])
        ])
    );
}

#[test]
fn assorted_links() {
    assert_ast_match!(
        [extension.autolink],
        r#"hello <https://example.com/fooo> world
hello [foo](https://example.com) world
hello [foo] world
hello [bar][bar] world
hello https://example.com/foo world
hello www.example.com world
hello foo@example.com world

[foo]: https://example.com
[bar]: https://example.com"#,
        (document (1:1-10:26) [
            (paragraph (1:1-7:27) [
                (text (1:1-1:6) "hello ")
                (link (1:7-1:32) "https://example.com/fooo" [
                    (text (1:8-1:31) "https://example.com/fooo")
                ])
                (text (1:33-1:38) " world")
                (softbreak (1:39-1:39))
                (text (2:1-2:6) "hello ")
                (link (2:7-2:32) "https://example.com" [
                    (text (2:8-2:10) "foo")
                ])
                (text (2:33-2:38) " world")
                (softbreak (2:39-2:39))
                (text (3:1-3:6) "hello ")
                (link (3:7-3:11) "https://example.com" [
                    (text (3:8-3:10) "foo")
                ])
                (text (3:12-3:17) " world")
                (softbreak (3:18-3:18))
                (text (4:1-4:6) "hello ")
                (link (4:7-4:16) "https://example.com" [
                    (text (4:8-4:10) "bar")
                ])
                (text (4:17-4:22) " world")
                (softbreak (4:23-4:23))
                (text (5:1-5:6) "hello ")
                (link (5:7-5:29) "https://example.com/foo" [
                    (text (5:7-5:29) "https://example.com/foo")
                ])
                (text (5:30-5:35) " world")
                (softbreak (5:36-5:36))
                (text (6:1-6:6) "hello ")
                (link (6:7-6:21) "http://www.example.com" [
                    (text (6:7-6:21) "www.example.com")
                ])
                (text (6:22-6:27) " world")
                (softbreak (6:28-6:28))
                (text (7:1-7:6) "hello ")
                (link (7:7-7:21) "mailto:foo@example.com" [
                    (text (7:7-7:21) "foo@example.com")
                ])
                (text (7:22-7:27) " world")
            ])
        ])
    );
}

#[test]
fn autolink_between_escaped_chars() {
    assert_ast_match!(
       [extension.autolink],
       "a.b-c\\_d@a.b\\_\n",
       (document (1:1-1:14) [
           (paragraph (1:1-1:14) [
               (text (1:1-1:7) "a.b-c_")
               (link (1:8-1:12) "mailto:d@a.b" [
                    (text (1:8-1:12) "d@a.b")
                ])
               (text (1:14-1:14) "_")
           ])
       ])
    );
}

#[test]
fn sourcepos_newline_kinds() {
    assert_ast_match!(
        [],
        "LF\n"
        "CRLF\r\n"
        "CR\r"
        "end.",
        (document (1:1-4:4) [
            (paragraph (1:1-4:4) [
                (text (1:1-1:2) "LF")
                (softbreak (1:3-1:3))
                (text (2:1-2:4) "CRLF")
                (softbreak (2:5-2:6))
                (text (3:1-3:2) "CR")
                (softbreak (3:3-3:3))
                (text (4:1-4:4) "end.")
            ])
        ])
    );
}

#[test]
fn hex_entity_digit_limit() {
    // 6 hex digits: valid
    html("&#x000041;\n", "<p>A</p>\n");

    // 7 hex digits: invalid, should be literal
    html("&#x0000041;\n", "<p>&amp;#x0000041;</p>\n");

    // 7 decimal digits: valid (decimal 100 = 'd')
    html("&#0000100;\n", "<p>d</p>\n");
}

#[test]
fn tab_indented_fence_in_list_keeps_columns() {
    // The list marker consumes two of the tab's four columns, so the fence
    // offset must be measured in columns. Counting bytes used to under-count
    // it by one and leave the rest of the tab behind as a leading space,
    // rendering `<pre><code> x`. commonmark.js (the dingus) renders `x`.
    html(
        "- a\n\n\t```\n\tx\n\t```\n",
        concat!(
            "<ul>\n",
            "<li>\n",
            "<p>a</p>\n",
            "<pre><code>x\n",
            "</code></pre>\n",
            "</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn tab_indented_fence_in_block_quote_keeps_columns() {
    // Same partial-tab case, with a block quote instead of a list marker.
    html(
        "> \t```\n> \tx\n> \t```\n",
        concat!(
            "<blockquote>\n",
            "<pre><code>x\n",
            "</code></pre>\n",
            "</blockquote>\n"
        ),
    );
}
