// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use crate::nodes::{NodeCode, NodeValue};

use super::*;

#[test]
fn basic() {
    html(
        concat!(
            "My **document**.\n",
            "\n",
            "It's mine.\n",
            "\n",
            "> Yes.\n",
            "\n",
            "## Hi!\n",
            "\n",
            "Okay.\n"
        ),
        concat!(
            "<p>My <strong>document</strong>.</p>\n",
            "<p>It's mine.</p>\n",
            "<blockquote>\n",
            "<p>Yes.</p>\n",
            "</blockquote>\n",
            "<h2>Hi!</h2>\n",
            "<p>Okay.</p>\n"
        ),
    );
}

#[test]
fn codefence() {
    html(
        concat!("``` rust yum\n", "fn main<'a>();\n", "```\n"),
        concat!(
            "<pre><code class=\"language-rust\">fn main&lt;'a&gt;();\n",
            "</code></pre>\n"
        ),
    );
}

#[test]
fn lists() {
    html(
        concat!("2. Hello.\n", "3. Hi.\n"),
        concat!(
            "<ol start=\"2\">\n",
            "<li>Hello.</li>\n",
            "<li>Hi.</li>\n",
            "</ol>\n"
        ),
    );

    html(
        concat!("- Hello.\n", "- Hi.\n"),
        concat!("<ul>\n", "<li>Hello.</li>\n", "<li>Hi.</li>\n", "</ul>\n"),
    );
}
#[test]
fn thematic_breaks() {
    html(
        concat!("---\n", "\n", "- - -\n", "\n", "\n", "_        _   _\n"),
        concat!("<hr />\n", "<hr />\n", "<hr />\n"),
    );
}

#[test]
fn atx_heading() {
    html(
        concat!("# h1\n", "foo\n", "## h2\n", "####### foo\n"),
        concat!(
            "<h1>h1</h1>\n",
            "<p>foo</p>\n",
            "<h2>h2</h2>\n",
            "<p>####### foo</p>\n"
        ),
    );
}

#[test]
fn atx_heading_sourcepos() {
    assert_ast_match!(
        [],
        "# h1\n"
        "foo\n"
        "## h2\n"
        // https://spec.commonmark.org/0.31.2/#example-63
        "####### foo\n",
        (document (1:1-4:11) [
            (heading (1:1-1:4) [
                (text (1:3-1:4) "h1")
            ])
            (paragraph (2:1-2:3) [
                (text (2:1-2:3) "foo")
            ])
            (heading (3:1-3:5) [
                (text (3:4-3:5) "h2")
            ])
            (paragraph (4:1-4:11) [
                (text (4:1-4:11) "####### foo")
            ])
        ])
    );
}

#[test]
fn atx_heading_closed() {
    let input = concat!(
        // https://spec.commonmark.org/0.31.2/#example-71
        "## Heading ##\n",
        "   ## Heading ##\n",
        // https://spec.commonmark.org/0.31.2/#example-72
        "## Heading #########\n",
        "#### Heading ##\n",
        // https://spec.commonmark.org/0.31.2/#example-73
        "## Heading ###   \n",
    );

    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, input, &options);

    let headings = root
        .descendants()
        .filter(|n| matches!(n.data().value, NodeValue::Heading(_)));

    for heading in headings {
        if let NodeValue::Heading(ref nh) = heading.data().value {
            assert!(
                nh.closed,
                "Expected all ATX headings to be closed: {:?}",
                heading
            );
        }
    }
}

#[test]
fn atx_heading_not_closed() {
    let input = concat!(
        // https://spec.commonmark.org/0.31.2/#example-62
        "# Heading\n",
        // https://spec.commonmark.org/0.31.2/#example-74
        "### Heading ### b\n",
        // https://spec.commonmark.org/0.31.2/#example-75
        "# Heading#\n",
        // https://spec.commonmark.org/0.31.2/#example-76
        "### Heading \\###\n",
        "## Heading #\\##\n",
        "# Heading \\#\n",
    );

    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, input, &options);

    let headings = root
        .descendants()
        .filter(|n| matches!(n.data().value, NodeValue::Heading(_)));

    for heading in headings {
        if let NodeValue::Heading(ref nh) = heading.data().value {
            assert!(
                !nh.closed,
                "Expected all ATX headings to be not closed: {:?}",
                heading
            );
        }
    }
}

#[test]
fn atx_heading_closed_sourcepos() {
    assert_ast_match!(
        [parse.escaped_char_spans],
        // https://spec.commonmark.org/0.31.2/#example-71
        "## Heading ##\n"
        "   ## Heading ##\n"
        // https://spec.commonmark.org/0.31.2/#example-72
        "## Heading #########\n"
        "#### Heading ##\n"
        // Not part of the spec, but for completeness
        "######### Not heading ##\n"
        // https://spec.commonmark.org/0.31.2/#example-73
        "## Heading ###   \n"
        // https://spec.commonmark.org/0.31.2/#example-74
        "### Heading ### b\n"
        // https://spec.commonmark.org/0.31.2/#example-75
        "# Heading#\n"
        // https://spec.commonmark.org/0.31.2/#example-76
        "### Heading \\###\n"
        "## Heading #\\##\n"
        "# Heading \\#\n",
        (document (1:1-11:12) [
            (heading (1:1-1:13) [
                (text (1:4-1:10) "Heading")
            ])
            (heading (2:4-2:16) [
                (text (2:7-2:13) "Heading")
            ])
            (heading (3:1-3:20) [
                (text (3:4-3:10) "Heading")
            ])
            (heading (4:1-4:15) [
                (text (4:6-4:12) "Heading")
            ])
            (paragraph (5:1-5:24) [
                (text (5:1-5:24) "######### Not heading ##")
            ])
            (heading (6:1-6:17) [
                (text (6:4-6:10) "Heading")
            ])
            (heading (7:1-7:17) [
                (text (7:5-7:17) "Heading ### b")
            ])
            (heading (8:1-8:10) [
                (text (8:3-8:10) "Heading#")
            ])
            (heading (9:1-9:16) [
                (text (9:5-9:12) "Heading ")
                (escaped (9:13-9:14) [
                    (text (9:14-9:14) "#")
                ])
                (text (9:15-9:16) "##")
            ])
            (heading (10:1-10:15) [
                (text (10:4-10:12) "Heading #")
                (escaped (10:13-10:14) [
                    (text (10:14-10:14) "#")
                ])
                (text (10:15-10:15) "#")
            ])
            (heading (11:1-11:12) [
                (text (11:3-11:10) "Heading ")
                (escaped (11:11-11:12) [
                    (text (11:12-11:12) "#")
                ])
            ])
        ])
    );
}

#[test]
fn setext_heading() {
    html(
        concat!("Hi\n", "==\n", "\n", "Ok\n", "-----\n"),
        concat!("<h1>Hi</h1>\n", "<h2>Ok</h2>\n"),
    );
}

#[test]
fn setext_heading_sourcepos() {
    assert_ast_match!(
        [],
        "Header\n"
        "---\n"
        "this",
        (document (1:1-3:4) [
            (heading (1:1-2:3) [
                (text (1:1-1:6) "Header")
            ])
            (paragraph (3:1-3:4) [
                (text (3:1-3:4) "this")
            ])
        ])
    );
}

#[test]
fn ignore_setext_heading() {
    html_opts!(
        [parse.ignore_setext],
        "text text\n---",
        "<p>text text</p>\n<hr />\n",
    );
}

#[test]
fn figure_with_caption_with_title() {
    html_opts!(
        [render.figure_with_caption],
        "![image](https://example.com/image.png \"this is an image\")\n",
        "<p><figure><img src=\"https://example.com/image.png\" alt=\"image\" title=\"this is an image\" /><figcaption>this is an image</figcaption></figure></p>\n",
    );
}

#[test]
fn figure_with_caption_without_title() {
    html_opts!(
        [render.figure_with_caption],
        "![image](https://example.com/image.png)\n",
        "<p><figure><img src=\"https://example.com/image.png\" alt=\"image\" /></figure></p>\n",
    );
}

#[test]
fn html_block_1() {
    html_opts!(
        [render.r#unsafe],
        concat!(
            "<script>\n",
            "*ok* </script> *ok*\n",
            "\n",
            "*ok*\n",
            "\n",
            "*ok*\n",
            "\n",
            "<pre x>\n",
            "*ok*\n",
            "</style>\n",
            "*ok*\n",
            "<style>\n",
            "*ok*\n",
            "</style>\n",
            "\n",
            "*ok*\n"
        ),
        concat!(
            "<script>\n",
            "*ok* </script> *ok*\n",
            "<p><em>ok</em></p>\n",
            "<p><em>ok</em></p>\n",
            "<pre x>\n",
            "*ok*\n",
            "</style>\n",
            "<p><em>ok</em></p>\n",
            "<style>\n",
            "*ok*\n",
            "</style>\n",
            "<p><em>ok</em></p>\n"
        ),
    );
}

#[test]
fn html_block_2() {
    html_opts!(
        [render.r#unsafe],
        concat!("   <!-- abc\n", "\n", "ok --> *hi*\n", "*hi*\n"),
        concat!(
            "   <!-- abc\n",
            "\n",
            "ok --> *hi*\n",
            "<p><em>hi</em></p>\n"
        ),
    );
}

#[test]
fn html_block_3() {
    html_opts!(
        [render.r#unsafe],
        concat!(" <? o\n", "k ?> *a*\n", "*a*\n"),
        concat!(" <? o\n", "k ?> *a*\n", "<p><em>a</em></p>\n"),
    );
}

#[test]
fn html_block_4() {
    html_opts!(
        [render.r#unsafe],
        concat!("<!X >\n", "ok\n", "<!X\n", "um > h\n", "ok\n"),
        concat!("<!X >\n", "<p>ok</p>\n", "<!X\n", "um > h\n", "<p>ok</p>\n"),
    );
}

#[test]
fn html_block_5() {
    html_opts!(
        [render.r#unsafe],
        concat!(
            "<![CDATA[\n",
            "\n",
            "hm >\n",
            "*ok*\n",
            "]]> *ok*\n",
            "*ok*\n"
        ),
        concat!(
            "<![CDATA[\n",
            "\n",
            "hm >\n",
            "*ok*\n",
            "]]> *ok*\n",
            "<p><em>ok</em></p>\n"
        ),
    );
}

#[test]
fn html_block_6() {
    html_opts!(
        [render.r#unsafe],
        concat!(" </table>\n", "*x*\n", "\n", "ok\n", "\n", "<li\n", "*x*\n"),
        concat!(" </table>\n", "*x*\n", "<p>ok</p>\n", "<li\n", "*x*\n"),
    );
}

#[test]
fn html_block_7() {
    html_opts!(
        [render.r#unsafe],
        concat!(
            "<a b >\n",
            "ok\n",
            "\n",
            "<a b=>\n",
            "ok\n",
            "\n",
            "<a b \n",
            "<a b> c\n",
            "ok\n"
        ),
        concat!(
            "<a b >\n",
            "ok\n",
            "<p>&lt;a b=&gt;\n",
            "ok</p>\n",
            "<p>&lt;a b\n",
            "<a b> c\n",
            "ok</p>\n"
        ),
    );

    html_opts!(
        [render.r#unsafe],
        concat!("<a b c=x d='y' z=\"f\" >\n", "ok\n", "\n", "ok\n"),
        concat!("<a b c=x d='y' z=\"f\" >\n", "ok\n", "<p>ok</p>\n"),
    );
}

#[test]
fn backticks() {
    html(
        "Some `code\\` yep.\n",
        "<p>Some <code>code\\</code> yep.</p>\n",
    );
}

#[test]
fn backticks_empty_with_newline_should_be_space() {
    html("`\n`", "<p><code> </code></p>\n");
}

#[test]
fn blockquote_hard_linebreak_space() {
    html(">\\\n A", "<blockquote>\n<p><br />\nA</p>\n</blockquote>\n");
}

#[test]
fn blockquote_hard_linebreak_nonlazy_space() {
    html(
        "> A\\\n> B",
        "<blockquote>\n<p>A<br />\nB</p>\n</blockquote>\n",
    );
}

#[test]
fn backticks_num() {
    let input = "Some `code1`. More ``` code2 ```.\n";

    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, input, &options);

    let code1 = NodeValue::Code(NodeCode {
        num_backticks: 1,
        literal: "code1".to_string().into(),
    });
    assert_node_eq(root, &[0, 1], &code1);

    let code2 = NodeValue::Code(NodeCode {
        num_backticks: 3,
        literal: "code2".to_string().into(),
    });
    assert_node_eq(root, &[0, 3], &code2);
}

#[test]
fn backslashes() {
    html(
        concat!(
            "Some \\`fake code\\`.\n",
            "\n",
            "Some fake linebreaks:\\\n",
            "Yes.\\\n",
            "See?\n",
            "\n",
            "Ga\\rbage.\n"
        ),
        concat!(
            "<p>Some `fake code`.</p>\n",
            "<p>Some fake linebreaks:<br />\n",
            "Yes.<br />\n",
            "See?</p>\n",
            "<p>Ga\\rbage.</p>\n"
        ),
    );
}

#[test]
fn entities() {
    html(
        concat!(
            "This is &amp;, &copy;, &trade;, \\&trade;, &xyz;, &NotEqualTilde;.\n",
            "\n",
            "&#8734; &#x221e;\n"
        ),
        concat!(
            "<p>This is &amp;, ©, ™, &amp;trade;, &amp;xyz;, \u{2242}\u{338}.</p>\n",
            "<p>∞ ∞</p>\n"
        ),
    );
}

#[test]
fn links() {
    html(
        concat!(
            "Where are you [going](https://microsoft.com (today))?\n",
            "\n",
            "[Where am I?](/here)\n"
        ),
        concat!(
            "<p>Where are you <a href=\"https://microsoft.com\" \
             title=\"today\">going</a>?</p>\n",
            "<p><a href=\"/here\">Where am I?</a></p>\n"
        ),
    );
    html(
        concat!(
            r"Where are you [going](#1\.-link (today))?",
            "\n",
            "\n",
            "[Where am I?](/here)\n"
        ),
        concat!(
            "<p>Where are you <a href=\"#1.-link\" \
             title=\"today\">going</a>?</p>\n",
            "<p><a href=\"/here\">Where am I?</a></p>\n"
        ),
    );
    html(
        r"[Link Text](\\\\)",
        concat!(r##"<p><a href="%5C%5C">Link Text</a></p>"##, '\n'),
    );
    html(
        r"[Link Text](\\\\\\\\\\)",
        concat!(r##"<p><a href="%5C%5C%5C%5C%5C">Link Text</a></p>"##, '\n'),
    );
    html(
        r"[Link Text](\\\\ (title))",
        concat!(
            r##"<p><a href="%5C%5C" title="title">Link Text</a></p>"##,
            '\n'
        ),
    );
    html(
        r"[Link Text](\#)",
        concat!(r##"<p><a href="#">Link Text</a></p>"##, '\n'),
    );
}

#[test]
fn images() {
    html(
        "I am ![eating [things](/url)](http://i.imgur.com/QqK1vq7.png).\n",
        "<p>I am <img src=\"http://i.imgur.com/QqK1vq7.png\" alt=\"eating things\" />.</p>\n",
    );
}

#[test]
fn reference_links() {
    html(
        concat!(
            "This [is] [legit], [very][honestly] legit.\n",
            "\n",
            "[legit]: ok\n",
            "[honestly]: sure \"hm\"\n"
        ),
        "<p>This [is] <a href=\"ok\">legit</a>, <a href=\"sure\" title=\"hm\">very</a> legit.</p>\n",
    );
}

#[test]
fn reference_links_casefold() {
    html(
        concat!("[ẞ]\n", "\n", "[SS]: /url	\n",),
        "<p><a href=\"/url\">ẞ</a></p>\n",
    );
}

#[test]
fn safety() {
    html(
        concat!(
            "[data:image/png](data:image/png/x)\n\n",
            "[data:image/gif](data:image/gif/x)\n\n",
            "[data:image/jpeg](data:image/jpeg/x)\n\n",
            "[data:image/webp](data:image/webp/x)\n\n",
            "[data:malicious](data:malicious/x)\n\n",
            "[javascript:malicious](javascript:malicious)\n\n",
            "[vbscript:malicious](vbscript:malicious)\n\n",
            "[file:malicious](file:malicious)\n\n",
        ),
        concat!(
            "<p><a href=\"data:image/png/x\">data:image/png</a></p>\n",
            "<p><a href=\"data:image/gif/x\">data:image/gif</a></p>\n",
            "<p><a href=\"data:image/jpeg/x\">data:image/jpeg</a></p>\n",
            "<p><a href=\"data:image/webp/x\">data:image/webp</a></p>\n",
            "<p><a href=\"\">data:malicious</a></p>\n",
            "<p><a href=\"\">javascript:malicious</a></p>\n",
            "<p><a href=\"\">vbscript:malicious</a></p>\n",
            "<p><a href=\"\">file:malicious</a></p>\n",
        ),
    )
}

#[test]
fn link_backslash_requires_punct() {
    // Test should probably be in the spec.
    html("[a](\\ b)", "<p>[a](\\ b)</p>\n");
}

#[test]
fn nul_replacement_1() {
    html("a\0b", "<p>a\u{fffd}b</p>\n");
}

#[test]
fn nul_replacement_2() {
    html("a\0b\0c", "<p>a\u{fffd}b\u{fffd}c</p>\n");
}

#[test]
fn nul_replacement_3() {
    html("a\0\nb", "<p>a\u{fffd}\nb</p>\n");
}

#[test]
fn nul_replacement_4() {
    html("a\0\r\nb", "<p>a\u{fffd}\nb</p>\n");
}

#[test]
fn nul_replacement_5() {
    html("a\r\n\0b", "<p>a\n\u{fffd}b</p>\n");
}

#[test]
fn case_insensitive_safety() {
    html(
        "[a](javascript:a) [b](Javascript:b) [c](jaVascript:c) [d](data:xyz) [e](Data:xyz) [f](vbscripT:f) [g](FILE:g)\n",
        "<p><a href=\"\">a</a> <a href=\"\">b</a> <a href=\"\">c</a> <a href=\"\">d</a> <a href=\"\">e</a> <a href=\"\">f</a> <a href=\"\">g</a></p>\n",
    );
}

#[test]
fn link_sourcepos_baseline() {
    assert_ast_match!(
        [],
        "[ABCD](/)\n",
        (document (1:1-1:9) [
            (paragraph (1:1-1:9) [
                (link (1:1-1:9) "/" [
                    (text (1:2-1:5) "ABCD")
                ])
            ])
        ])
    );
}

// https://github.com/kivikakk/comrak/issues/301
#[test]
fn link_sourcepos_newline() {
    assert_ast_match!(
        [],
        "[AB\nCD](/)\n",
        (document (1:1-2:6) [
            (paragraph (1:1-2:6) [
                (link (1:1-2:6) "/" [
                    (text (1:2-1:3) "AB")
                    (softbreak (1:4-1:4))
                    (text (2:1-2:2) "CD")
                ])
            ])
        ])
    );
}

// https://github.com/kivikakk/comrak/issues/572
#[test]
fn link_sourcepos_newline_n_between_dest_and_title() {
    assert_ast_match!(
        [],
        "[link](   /uri\n  \"title\"  )\n",
        (document (1:1-2:12) [
            (paragraph (1:1-2:12) [
                (link (1:1-1:14) "/uri" [
                    (text (1:2-1:5) "link")
                ])
            ])
        ])
    );
}

#[test]
fn link_sourcepos_newline_r_between_dest_and_title() {
    assert_ast_match!(
        [],
        "[link](   /uri\r  \"title\"  )\r",
        (document (1:1-2:12) [
            (paragraph (1:1-2:12) [
                (link (1:1-1:14) "/uri" [
                    (text (1:2-1:5) "link")
                ])
            ])
        ])
    );
}

#[test]
fn link_sourcepos_newline_rn_between_dest_and_title() {
    assert_ast_match!(
        [],
        "[link](   /uri\r\n  \"title\"  )\r\n",
        (document (1:1-2:12) [
            (paragraph (1:1-2:12) [
                (link (1:1-1:14) "/uri" [
                    (text (1:2-1:5) "link")
                ])
            ])
        ])
    );
}

#[test]
fn link_sourcepos_truffle() {
    assert_ast_match!(
        [],
        "- A\n[![B](/B.png)](/B)\n",
        (document (1:1-2:18) [
            (list (1:1-2:18) [
                (item (1:1-2:18) [
                    (paragraph (1:3-2:18) [
                        (text (1:3-1:3) "A")
                        (softbreak (1:4-1:4))
                        (link (2:1-2:18) "/B" [
                            (image (2:2-2:13) "/B.png" [
                                (text (2:4-2:4) "B")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn link_sourcepos_truffle_twist() {
    assert_ast_match!(
        [],
        "- A\n  [![B](/B.png)](/B)\n",
        (document (1:1-2:20) [
            (list (1:1-2:20) [
                (item (1:1-2:20) [
                    (paragraph (1:3-2:20) [
                        (text (1:3-1:3) "A")
                        (softbreak (1:4-1:4))
                        (link (2:3-2:20) "/B" [
                            (image (2:4-2:15) "/B.png" [
                                (text (2:6-2:6) "B")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn link_sourcepos_truffle_bergamot() {
    assert_ast_match!(
        [],
        "- A\n   [![B](/B.png)](/B)\n",
        (document (1:1-2:21) [
            (list (1:1-2:21) [
                (item (1:1-2:21) [
                    (paragraph (1:3-2:21) [
                        (text (1:3-1:3) "A")
                        (softbreak (1:4-1:4))
                        (link (2:4-2:21) "/B" [
                            (image (2:5-2:16) "/B.png" [
                                (text (2:7-2:7) "B")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn paragraph_sourcepos_multiline() {
    assert_ast_match!(
        [],
        "  A\n"
        "   B\n",
        (document (1:1-2:4) [
            (paragraph (1:3-2:4) [
                (text (1:3-1:3) "A")
                (softbreak (1:4-1:4))
                (text (2:4-2:4) "B")
            ])
        ])
    );
}

#[test]
fn listitem_sourcepos_multiline() {
    assert_ast_match!(
        [],
        "- A\n"
        "B\n",
        (document (1:1-2:1) [
            (list (1:1-2:1) [
                (item (1:1-2:1) [
                    (paragraph (1:3-2:1) [
                        (text (1:3-1:3) "A")
                        (softbreak (1:4-1:4))
                        (text (2:1-2:1) "B")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn listitem_sourcepos_multiline_2() {
    assert_ast_match!(
        [],
        "- A\n"
        "   B\n"
        "-  C\n"
        " D",
        (document (1:1-4:2) [
            (list (1:1-4:2) [
                (item (1:1-2:4) [
                    (paragraph (1:3-2:4) [
                        (text (1:3-1:3) "A")
                        (softbreak (1:4-1:4))
                        (text (2:4-2:4) "B")
                    ])
                ])
                (item (3:1-4:2) [
                    (paragraph (3:4-4:2) [
                        (text (3:4-3:4) "C")
                        (softbreak (3:5-3:5))
                        (text (4:2-4:2) "D")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn emphasis_sourcepos_double_1() {
    assert_ast_match!(
        [],
        "_**this**_\n",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (emph (1:1-1:10) [
                    (strong (1:2-1:9) [
                        (text (1:4-1:7) "this")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn emphasis_sourcepos_double_2() {
    assert_ast_match!(
        [],
        "**_this_**\n",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (strong (1:1-1:10) [
                    (emph (1:3-1:8) [
                        (text (1:4-1:7) "this")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn emphasis_sourcepos_double_3() {
    assert_ast_match!(
        [],
        "___this___\n",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (emph (1:1-1:10) [
                    (strong (1:2-1:9) [
                        (text (1:4-1:7) "this")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn emphasis_sourcepos_double_4() {
    assert_ast_match!(
        [],
        "***this***\n",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (emph (1:1-1:10) [
                    (strong (1:2-1:9) [
                        (text (1:4-1:7) "this")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn ipv6_host_unescaped() {
    html(
        "test <http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/test>",
        "<p>test <a href=\"http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/test\">http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/test</a></p>\n",
    );

    html(
        "[henwo](https://[2402:1f00:89aa:300::5%25eth0]:9443?target=<yes>)",
        "<p><a href=\"https://[2402:1f00:89aa:300::5%25eth0]:9443?target=%3Cyes%3E\">henwo</a></p>\n",
    );
}

#[test]
fn link_ref_definition_priority() {
    html(
        "[foo]\n\n[foo]: first\n[foo]: second\n",
        "<p><a href=\"first\">foo</a></p>\n",
    );
}

#[test]
fn link_ref_definition_empty() {
    html("[foo]:\n\n[foo]", "<p>[foo]:</p>\n<p>[foo]</p>\n");
}
