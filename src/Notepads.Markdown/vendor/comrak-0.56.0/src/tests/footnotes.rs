use super::*;

#[test]
fn footnote_label_containing_bracket_is_not_a_reference() {
    html_opts!(
        [extension.footnotes, render.r#unsafe],
        "x [^a[b]<c>] y",
        "<p>x [^a[b]<c>] y</p>\n",
    );
}

#[test]
fn footnote_label_with_escaped_bracket_is_not_a_reference() {
    html_opts!(
        [extension.footnotes, render.r#unsafe],
        "x [^a\\[b]<c>] y",
        "<p>x [^a[b]<c>] y</p>\n",
    );
}

#[test]
fn footnotes() {
    html_opts!(
        [extension.footnotes],
        concat!(
            "Here is a[^nowhere] footnote reference,[^1] and another.[^longnote]\n",
            "\n",
            "This is another note.[^note] And footnote[^longnote] is referenced again.\n",
            "\n",
            "[^note]: Hi.\n",
            "\n",
            "[^1]: Here is the footnote.\n",
            "\n",
            "[^longnote]: Here's one with multiple blocks.\n",
            "\n",
            "    Subsequent paragraphs are indented.\n",
            "\n",
            "        code\n",
            "\n",
            "This is regular content.\n",
            "\n",
            "[^unused]: This is not used.\n"
        ),
        concat!(
            "<p>Here is a[^nowhere] footnote reference,<sup class=\"footnote-ref\"><a href=\"#fn-1\" \
             id=\"fnref-1\" data-footnote-ref>1</a></sup> and another.<sup class=\"footnote-ref\"><a \
             href=\"#fn-longnote\" id=\"fnref-longnote\" data-footnote-ref>2</a></sup></p>\n",
            "<p>This is another note.<sup class=\"footnote-ref\"><a \
             href=\"#fn-note\" id=\"fnref-note\" data-footnote-ref>3</a></sup> And footnote<sup class=\"footnote-ref\"><a \
             href=\"#fn-longnote\" id=\"fnref-longnote-2\" data-footnote-ref>2</a></sup> is referenced again.</p>\n",
            "<p>This is regular content.</p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-1\">\n",
            "<p>Here is the footnote. <a href=\"#fnref-1\" \
             class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n",
            "</li>\n",
            "<li id=\"fn-longnote\">\n",
            "<p>Here's one with multiple blocks.</p>\n",
            "<p>Subsequent paragraphs are indented.</p>\n",
            "<pre><code>code\n",
            "</code></pre>\n",
            "<a href=\"#fnref-longnote\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"2\" aria-label=\"Back to reference 2\">↩</a> \
             <a href=\"#fnref-longnote-2\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"2-2\" aria-label=\"Back to reference 2-2\">↩<sup class=\"footnote-ref\">2</sup></a>\n",
            "</li>\n",
            "<li id=\"fn-note\">\n",
            "<p>Hi. <a href=\"#fnref-note\" \
             class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"3\" aria-label=\"Back to reference 3\">↩</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n"
        ),
    );
}

#[test]
fn footnote_does_not_eat_exclamation() {
    html_opts!(
        [extension.footnotes],
        concat!("Here's my footnote![^a]\n", "\n", "[^a]: Yep.\n"),
        concat!(
            "<p>Here's my footnote!<sup class=\"footnote-ref\"><a href=\"#fn-a\" id=\"fnref-a\" data-footnote-ref>1</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-a\">\n",
            "<p>Yep. <a href=\"#fnref-a\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n"
        ),
    );
}

#[test]
fn footnote_in_table() {
    html_opts!(
        [extension.table, extension.footnotes],
        concat!(
            "A footnote in a paragraph[^1]\n",
            "\n",
            "| Column1   | Column2 |\n",
            "| --------- | ------- |\n",
            "| foot [^1] | note    |\n",
            "\n",
            "[^1]: a footnote\n",
        ),
        concat!(
            "<p>A footnote in a paragraph<sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\" data-footnote-ref>1</a></sup></p>\n",
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>Column1</th>\n",
            "<th>Column2</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td>foot <sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1-2\" data-footnote-ref>1</a></sup></td>\n",
            "<td>note</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-1\">\n",
            "<p>a footnote <a href=\"#fnref-1\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a> <a href=\"#fnref-1-2\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1-2\" aria-label=\"Back to reference 1-2\">↩<sup class=\"footnote-ref\">2</sup></a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n",
        )
    );
}

#[test]
fn footnote_with_superscript() {
    html_opts!(
        [extension.superscript, extension.footnotes],
        concat!(
            "Here is a footnote reference.[^1]\n",
            "\n",
            "Here is a longer footnote reference.[^ref]\n",
            "\n",
            "e = mc^2^.\n",
            "\n",
            "[^1]: Here is the footnote.\n",
            "[^ref]: Here is another footnote.\n",
        ),
        concat!(
            "<p>Here is a footnote reference.<sup class=\"footnote-ref\"><a href=\"#fn-1\" \
             id=\"fnref-1\" data-footnote-ref>1</a></sup></p>\n",
            "<p>Here is a longer footnote reference.<sup class=\"footnote-ref\"><a href=\"#fn-ref\" \
             id=\"fnref-ref\" data-footnote-ref>2</a></sup></p>\n",
            "<p>e = mc<sup>2</sup>.</p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-1\">\n",
            "<p>Here is the footnote. <a href=\"#fnref-1\" \
             class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n",
            "</li>\n",
            "<li id=\"fn-ref\">\n",
            "<p>Here is another footnote. <a href=\"#fnref-ref\" \
             class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"2\" aria-label=\"Back to reference 2\">↩</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n"
        ),
    );
}

#[test]
fn footnote_escapes_name() {
    html_opts!(
        [extension.footnotes],
        concat!(
            "Here is a footnote reference.[^😄ref]\n",
            "\n",
            "[^😄ref]: Here is the footnote.\n",
        ),
        concat!(
            "<p>Here is a footnote reference.<sup class=\"footnote-ref\"><a href=\"#fn-%F0%9F%98%84ref\" id=\"fnref-%F0%9F%98%84ref\" data-footnote-ref>1</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-%F0%9F%98%84ref\">\n",
            "<p>Here is the footnote. <a href=\"#fnref-%F0%9F%98%84ref\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n"
        ),
    );
}

#[test]
fn footnote_case_insensitive_and_case_preserving() {
    html_opts!(
        [extension.footnotes],
        concat!(
            "Here is a footnote reference.[^AB] and [^ab]\n",
            "\n",
            "[^aB]: Here is the footnote.\n",
        ),
        concat!(
            "<p>Here is a footnote reference.<sup class=\"footnote-ref\"><a href=\"#fn-aB\" id=\"fnref-aB\" data-footnote-ref>1</a></sup> and <sup class=\"footnote-ref\"><a href=\"#fn-aB\" id=\"fnref-aB-2\" data-footnote-ref>1</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-aB\">\n",
            "<p>Here is the footnote. <a href=\"#fnref-aB\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a> <a href=\"#fnref-aB-2\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1-2\" aria-label=\"Back to reference 1-2\">↩<sup class=\"footnote-ref\">2</sup></a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n"
        ),
    );
}

#[test]
fn footnote_name_parsed_into_multiple_nodes() {
    html_opts!(
        [extension.footnotes],
        concat!("Foo.[^_ab]\n", "\n", "[^_ab]: Here is the footnote.\n",),
        concat!(
            "<p>Foo.<sup class=\"footnote-ref\"><a href=\"#fn-_ab\" id=\"fnref-_ab\" data-footnote-ref>1</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-_ab\">\n",
            "<p>Here is the footnote. <a href=\"#fnref-_ab\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n"
        ),
    );
}

#[test]
fn footnote_invalid_with_missing_name() {
    html_opts!(
        [extension.footnotes],
        "Foo.[^]\n\n[^]: Here is the footnote.\n",
        "<p>Foo.[^]</p>\n<p>[^]: Here is the footnote.</p>\n"
    );
}

#[test]
fn footnote_does_not_allow_spaces_in_name() {
    html_opts!(
        [extension.footnotes],
        "Foo.[^one two]\n\n[^one two]: Here is the footnote.\n",
        "<p>Foo.[^one two]</p>\n<p>[^one two]: Here is the footnote.</p>\n"
    );
}

#[test]
fn footnote_does_not_expand_emphasis_in_name() {
    html_opts!(
        [extension.footnotes],
        "Foo[^**one**]\n[^**one**]: bar\n",
        concat!(
            "<p>Foo<sup class=\"footnote-ref\"><a href=\"#fn-**one**\" id=\"fnref-**one**\" data-footnote-ref>1</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-**one**\">\n",
            "<p>bar <a href=\"#fnref-**one**\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n"
        ),
    );
}

#[test]
fn sourcepos() {
    assert_ast_match!(
        [extension.footnotes],
        "Here is a footnote reference.[^1]\n"
        "\n"
        "[^1]: Here is the footnote.\n"
        "\n"
        "Here is a longer footnote reference.[^ref]\n"
        "\n"
        "[^ref]: Here is another footnote.\n",
        (document (1:1-7:33) [
            (paragraph (1:1-1:33) [
                (text (1:1-1:29) "Here is a footnote reference.")
                (footnote_reference (1:30-1:33))
            ])
            (paragraph (5:1-5:42) [
                (text (5:1-5:36) "Here is a longer footnote reference.")
                (footnote_reference (5:37-5:42))
            ])
            (footnote_definition (3:1-3:27) [
                (paragraph (3:7-3:27) [
                    (text (3:7-3:27) "Here is the footnote.")
                ])
            ])
            (footnote_definition (7:1-7:33) [
                (paragraph (7:9-7:33) [
                    (text (7:9-7:33) "Here is another footnote.")
                ])
            ])
        ])
    );
}

#[test]
fn leave_footnote_definitions() {
    assert_ast_match!(
        [extension.footnotes, parse.leave_footnote_definitions],
        "Here is a footnote reference.[^1]\n"
        "\n"
        "[^1]: Here is the footnote.\n"
        "\n"
        "Here is a longer footnote reference.[^ref]\n"
        "\n"
        "[^ref]: Here is another footnote.\n",
        (document (1:1-7:33) [
            (paragraph (1:1-1:33) [
                (text (1:1-1:29) "Here is a footnote reference.")
                (footnote_reference (1:30-1:33))
            ])
            (footnote_definition (3:1-3:27) [
                (paragraph (3:7-3:27) [
                    (text (3:7-3:27) "Here is the footnote.")
                ])
            ])
            (paragraph (5:1-5:42) [
                (text (5:1-5:36) "Here is a longer footnote reference.")
                (footnote_reference (5:37-5:42))
            ])
            (footnote_definition (7:1-7:33) [
                (paragraph (7:9-7:33) [
                    (text (7:9-7:33) "Here is another footnote.")
                ])
            ])
        ])
    );
}
