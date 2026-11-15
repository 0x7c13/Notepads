// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use std::sync::Arc;

use super::*;

#[test]
fn pointy_brace_open() {
    html("<!-", "<p>&lt;!-</p>\n");
}

#[test]
fn tasklist() {
    html_opts!(
        [extension.tasklist, parse.relaxed_tasklist_matching],
        "* [*]",
        "<ul>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> </li>\n</ul>\n",
    );
}

#[test]
fn tasklist_with_classes() {
    html_opts!(
        [
            extension.tasklist,
            render.tasklist_classes,
            parse.relaxed_tasklist_matching
        ],
        "* [*]",
        "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> </li>\n</ul>\n",
    );
}

#[test]
fn table_nul() {
    html_opts!(
        [extension.table],
        "\0|.\n-|-\nZ",
        r##"<table>
<thead>
<tr>
<th>�</th>
<th>.</th>
</tr>
</thead>
<tbody>
<tr>
<td>Z</td>
<td></td>
</tr>
</tbody>
</table>
"##,
    );
}

#[test]
fn footnote_def() {
    html_opts!(
        [
            extension.autolink,
            extension.footnotes,
            render.sourcepos,
            render.hardbreaks
        ],
        "\u{15}\u{b}\r[^ ]:",
        "<p data-sourcepos=\"1:1-2:5\">\u{15}\u{b}<br data-sourcepos=\"1:3-1:3\" />\n[^ ]:</p>\n",
    );
}

#[test]
fn line_end() {
    html("\u{2}\n\\\n\t-", "<p>\u{2}\n<br />\n-</p>\n");
}

#[test]
fn bracket_match() {
    html("[;\0V\n]::g\n[;\0V\n]", "<p><a href=\":g\">;�V\n</a></p>\n");
}

#[test]
fn trailing_hyphen() {
    assert_ast_match!(
        [extension.autolink, parse.smart],
        "3@.l-",
        (document (1:1-1:5) [
            (paragraph (1:1-1:5) [
                (text (1:1-1:5) "3@.l-")
            ])
        ])
    );
}

#[test]
fn trailing_smart_endash_matches() {
    assert_ast_match!(
        [extension.autolink, parse.smart],
        "--\n"
        "--(3@.l--\n",
        (document (1:1-2:9) [
            (paragraph (1:1-2:9) [
                (text (1:1-1:2) "–")  // en-dash
                (softbreak (1:3-1:3))
                (text (2:1-2:3) "–(")  // en-dash
                (link (2:4-2:7) "mailto:3@.l" [
                    (text (2:4-2:7) "3@.l")
                ])
                (text (2:8-2:9) "–")  // en-dash
            ])
        ])
    );
}

#[test]
fn trailing_endash_matches() {
    assert_ast_match!(
        [extension.autolink],
        "–\n"
        "–(3@.l–\n",
        (document (1:1-2:11) [
            (paragraph (1:1-2:11) [
                (text (1:1-1:3) "–")  // en-dash
                (softbreak (1:4-1:4))
                (text (2:1-2:4) "–(")  // en-dash
                (link (2:5-2:8) "mailto:3@.l" [
                    (text (2:5-2:8) "3@.l")
                ])
                (text (2:9-2:11) "–")  // en-dash
            ])
        ])
    );
}

#[test]
fn no_empty_text_before_email() {
    assert_ast_match!(
        [extension.autolink],
        "a@b.c\n",
        (document (1:1-1:5) [
            (paragraph (1:1-1:5) [
                (link (1:1-1:5) "mailto:a@b.c" [
                    (text (1:1-1:5) "a@b.c")
                ])
            ])
        ])
    );
}

#[test]
fn smart_sourcepos() {
    assert_ast_match!(
        [parse.smart],
        ": _--_ **---**\n\n"
        // As above, but entered directly.
        ": _–_ **—**\n",
        (document (1:1-3:15) [
            (paragraph (1:1-1:14) [
                (text (1:1-1:2) ": ")
                (emph (1:3-1:6) [
                    (text (1:4-1:5) "–")  // en-dash
                ])
                (text (1:7-1:7) " ")
                (strong (1:8-1:14) [
                    (text (1:10-1:12) "—")  // em-dash
                ])
            ])
            (paragraph (3:1-3:15) [
                (text (3:1-3:2) ": ")
                (emph (3:3-3:7) [
                    (text (3:4-3:6) "–")  // en-dash; 3 bytes in input
                ])
                (text (3:8-3:8) " ")
                (strong (3:9-3:15) [
                    (text (3:11-3:13) "—")  // em-dash; (still) 3 bytes
                ])
            ])
        ])
    );
}

#[test]
fn linebreak_sourcepos() {
    assert_ast_match!(
        [],
        "a\\\n"
        "b\n",
        (document (1:1-2:1) [
            (paragraph (1:1-2:1) [
                (text (1:1-1:1) "a")
                (linebreak (1:2-1:3))
                (text (2:1-2:1) "b")
            ])
        ])
    );
}

#[test]
fn echaw() {
    assert_ast_match!(
        [extension.autolink],
        "<U@.J<AA@.J",
        (document (1:1-1:11) [
            (paragraph (1:1-1:11) [
                (text (1:1-1:1) "<")
                (link (1:2-1:5) "mailto:U@.J" [
                    (text (1:2-1:5) "U@.J")
                ])
                (text (1:6-1:6) "<")
                (link (1:7-1:11) "mailto:AA@.J" [
                    (text (1:7-1:11) "AA@.J")
                ])
            ])
        ])
    );
}

#[test]
fn echaw2() {
    assert_ast_match!(
        [extension.autolink, parse.smart],
        ":C@.t'C@.t",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (text (1:1-1:1) ":")
                (link (1:2-1:5) "mailto:C@.t" [
                    (text (1:2-1:5) "C@.t")
                ])
                (text (1:6-1:6) "’")
                (link (1:7-1:10) "mailto:C@.t" [
                    (text (1:7-1:10) "C@.t")
                ])
            ])
        ])
    );
}

#[test]
fn echaw3() {
    assert_ast_match!(
        [extension.autolink, parse.smart],
        "c@.r\0\t\r"
        "z  \n"
        " f@.x",
        (document (1:1-3:5) [
            (paragraph (1:1-3:5) [
                (link (1:1-1:4) "mailto:c@.r" [
                    (text (1:1-1:4) "c@.r")
                ])
                (text (1:5-1:5) "\x00")
                // !! Spaces at EOL are trimmed.
                // See parser::inlines::Subject::parse_inline's final case.
                (softbreak (1:7-1:7))
                (text (2:1-2:1) "z")
                (linebreak (2:2-2:4))
                (link (3:2-3:5) "mailto:f@.x" [
                    (text (3:2-3:5) "f@.x")
                ])
            ])
        ])
    );
}

#[test]
fn echaw4() {
    // &UnderBar; resolves to a plain ASCII underscore "_".
    assert_ast_match!(
        [extension.autolink, parse.smart],
        "-@&UnderBar;.e--",
        (document (1:1-1:16) [
            (paragraph (1:1-1:16) [
                (link (1:1-1:14) "mailto:-@_.e" [
                    (text (1:1-1:14) "-@_.e")
                ])
                (text (1:15-1:16) "–")  // en-dash
            ])
        ])
    );
}

#[test]
fn echaw5() {
    assert_ast_match!(
        [],
        "_#___@e.u",
        (document (1:1-1:9) [
            (paragraph (1:1-1:9) [
                (emph (1:1-1:3) [
                    (text (1:2-1:2) "#")
                ])
                (text (1:4-1:9) "__@e.u")
            ])
        ])
    );
}

#[test]
fn echaw6() {
    assert_ast_match!(
        [extension.autolink],
        "_#___@e.u",
        (document (1:1-1:9) [
            (paragraph (1:1-1:9) [
                (emph (1:1-1:3) [
                    (text (1:2-1:2) "#")
                ])
                (link (1:4-1:9) "mailto:__@e.u" [
                    (text (1:4-1:9) "__@e.u")
                ])
            ])
        ])
    );
}

#[test]
fn echaw7() {
    assert_ast_match!(
        [extension.autolink],
        "&#65;i@i.a",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (link (1:1-1:10) "mailto:Ai@i.a" [
                    (text (1:1-1:10) "Ai@i.a")
                ])
            ])
        ])
    );
}

#[test]
fn echaw8() {
    // fuzz/artifacts/all_options/minimized-from-57c3eaf5e03b3fd7fa971b0db6143ee3c21a7452
    assert_ast_match!(
        [extension.autolink, extension.tasklist],
        "- [x] &Xfr;-<A@.N",
        (document (1:1-1:17) [
            (list (1:1-1:17) [
                (taskitem (1:1-1:17) [
                    (paragraph (1:7-1:17) [
                        (text (1:7-1:13) "𝔛-<")
                        (link (1:14-1:17) "mailto:A@.N" [
                            (text (1:14-1:17) "A@.N")
                        ])
                    ])
                ])
            ])
        ]),
    );
}

#[test]
fn echaw9() {
    // fuzz/artifacts/all_options/minimized-from-8a07a44ba1f971ec39d0c14d377c78c2535c6fd5
    assert_ast_match!(
        [extension.tasklist],
        "-\t[ ]&NewLine;",
        (document (1:1-1:14) [
            (list (1:1-1:14) [
                (taskitem (1:1-1:14))
            ])
        ]),
    );
}

#[test]
fn relaxed_autolink_email_in_footnote() {
    assert_ast_match!(
        [
            extension.autolink,
            extension.footnotes,
            parse.relaxed_autolinks
        ],
        "[^a@b.c\nA]:\n",
        (document (1:1-2:3) [
            (paragraph (1:1-2:3) [
                (text (1:1-1:2) "[^")
                (link (1:3-1:7) "mailto:a@b.c" [
                    (text (1:3-1:7) "a@b.c")
                ])
                (softbreak (1:8-1:8))
                (text (2:1-2:3) "A]:")
            ])
        ]),
    );
}

#[test]
fn nul_in_link() {
    html("![](\\#\0)", "<p><img src=\"#%EF%BF%BD\" alt=\"\" /></p>\n");
}

#[test]
fn truncate2() {
    html("~~~ ú", "<pre><code class=\"language-ú\"></code></pre>\n");
}

#[test]
fn commonmark_big_backtick_inline_code() {
    commonmark(
        "\0` ```````````````````````````````` <`",
        "�`  ```````````````````````````````` < `\n",
        None,
    );
}

#[test]
fn commonmark_table_of_fun() {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.spoiler = true;
    opts.render.experimental_minimize_commonmark = true;
    commonmark(
        "\0I|-|\r-|-|\r\u{c}||~|||",
        "| �I | - |\n| --- | --- |\n| &#12;\\||~\\|| |  |\n",
        Some(&opts),
    );
}

#[test]
fn tasklist_and_escapes_do_not_play() {
    assert_ast_match!(
        [],
        "\\!",
        (document (1:1-1:2) [
            (paragraph (1:1-1:2) [
                (text (1:2-1:2) "!")
            ])
        ])
    );

    assert_ast_match!(
        [parse.escaped_char_spans],
        "\\!",
        (document (1:1-1:2) [
            (paragraph (1:1-1:2) [
                (escaped (1:1-1:2) [
                    (text (1:2-1:2) "!")
                ])
            ])
        ])
    );

    assert_ast_match!(
        [],
        "+ \\[x]",
        (document (1:1-1:6) [
            (list (1:1-1:6) [
                (item (1:1-1:6) [
                    (paragraph (1:3-1:6) [
                        (text (1:4-1:6) "[x]")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.escaped_char_spans],
        "+ \\[x]",
        (document (1:1-1:6) [
            (list (1:1-1:6) [
                (item (1:1-1:6) [
                    (paragraph (1:3-1:6) [
                        (escaped (1:3-1:4) [
                            (text (1:4-1:4) "[")
                        ])
                        (text (1:5-1:6) "x]")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [extension.tasklist, parse.relaxed_tasklist_matching],
        "+ \\[\\:]",
        (document (1:1-1:7) [
            (list (1:1-1:7) [
                (item (1:1-1:7) [
                    (paragraph (1:3-1:7) [
                        (text (1:4-1:7) "[:]")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [extension.tasklist, parse.relaxed_tasklist_matching, parse.escaped_char_spans],
        "+ \\[\\:]",
        (document (1:1-1:7) [
            (list (1:1-1:7) [
                (item (1:1-1:7) [
                    (paragraph (1:3-1:7) [
                        (escaped (1:3-1:4) [
                            (text (1:4-1:4) "[")
                        ])
                        (escaped (1:5-1:6) [
                            (text (1:6-1:6) ":")
                        ])
                        (text (1:7-1:7) "]")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn dont_try_this_at_home() {
    assert_ast_match!(
        [extension.alerts, extension.table, parse.ignore_setext],
        "\u{3}\n--\nT",
        (document (1:1-3:1) [
            (table (1:1-3:1) [
                (table_row (1:1-1:1) [
                    (table_cell (1:1-1:1) [
                        (text (1:1-1:1) "\u{3}")
                    ])
                ])
                (table_row (3:1-3:1) [
                    (table_cell (3:1-3:1) [
                        (text (3:1-3:1) "T")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn cargo_run_no_cargo_road() {
    assert_ast_match!(
        [],
        "\t",
        (document (1:1-1:1))
    );
}

#[test]
fn oye_siri_prende_las_luces() {
    assert_ast_match!(
        [],
        "* ",
        (document (1:1-1:2) [
            (list (1:1-1:2) [
                (item (1:1-1:2))
            ])
        ])
    );
}

#[test]
fn cursed_lands() {
    let mut opts = Options::default();
    opts.extension.autolink = true;
    opts.extension.footnotes = true;
    opts.parse.smart = true;
    opts.parse.relaxed_autolinks = true;

    markdown_to_html("[^a@b.c--d]", &opts);
}

#[test]
fn inquisitor() {
    let mut opts = Options::default();
    opts.parse.broken_link_callback = Some(Arc::new(|link_ref: options::BrokenLinkReference| {
        Some(ResolvedReference::new(
            link_ref.normalized.to_string(),
            link_ref.original.to_string(),
        ))
    }));
    opts.render.width = 45;

    markdown_to_commonmark("\0\0\0\0\0\0[\\`\r\n;][ww=\\-", &opts);
}
