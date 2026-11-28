use super::*;
use ntest::{test_case, timeout};

#[test]
fn autolink_www() {
    html_opts!(
        [extension.autolink],
        "www.autolink.com\n",
        "<p><a href=\"http://www.autolink.com\">www.autolink.com</a></p>\n",
    );
}

#[test]
fn autolink_email() {
    html_opts!(
        [extension.autolink],
        "john@smith.com\n",
        "<p><a href=\"mailto:john@smith.com\">john@smith.com</a></p>\n",
    );
}

#[test]
fn autolink_scheme() {
    html_opts!(
        [extension.autolink],
        concat!("https://google.com/search\n", "rdar://localhost.com/blah"),
        concat!(
            "<p><a href=\"https://google.com/search\">https://google.com/search</a>\n",
            "rdar://localhost.com/blah</p>\n"
        ),
    );
}

#[test]
fn autolink_scheme_multiline() {
    html_opts!(
        [extension.autolink],
        "https://google.com/search\nhttps://www.google.com/maps",
        "<p><a href=\"https://google.com/search\">https://google.com/search</a>\n<a href=\"https://www.google.com/maps\">https://www.google.com/maps</a></p>\n",
    );
}

#[test]
fn autolink_no_link_bad() {
    html_opts!(
        [extension.autolink],
        concat!("@a.b.c@. x\n", "\n", "n@. x\n"),
        concat!("<p>@a.b.c@. x</p>\n", "<p>n@. x</p>\n"),
    );
}

#[test]
fn autolink_parentheses_balanced() {
    let examples = [
        [
            "http://www.pokemon.com/Pikachu_(Electric)",
            "<p><a href=\"http://www.pokemon.com/Pikachu_(Electric)\">http://www.pokemon.com/Pikachu_(Electric)</a></p>\n",
        ],
        [
            "http://www.pokemon.com/Pikachu_((Electric)",
            "<p><a href=\"http://www.pokemon.com/Pikachu_((Electric)\">http://www.pokemon.com/Pikachu_((Electric)</a></p>\n",
        ],
        [
            "http://www.pokemon.com/Pikachu_(Electric))",
            "<p><a href=\"http://www.pokemon.com/Pikachu_(Electric)\">http://www.pokemon.com/Pikachu_(Electric)</a>)</p>\n",
        ],
        [
            "http://www.pokemon.com/Pikachu_((Electric))",
            "<p><a href=\"http://www.pokemon.com/Pikachu_((Electric))\">http://www.pokemon.com/Pikachu_((Electric))</a></p>\n",
        ],
    ];

    for example in examples {
        html_opts!([extension.autolink], example[0], example[1]);
    }

    for example in examples {
        html_opts!(
            [extension.autolink, parse.relaxed_autolinks],
            example[0],
            example[1]
        );
    }
}

#[test]
#[timeout(1000)]
fn autolink_many_trailing_parentheses() {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.autolink = true;
    parse_document(
        &arena,
        &format!("http://a.b{}", ")".repeat(100_000)),
        &options,
    );
}

#[test]
fn autolink_brackets_unbalanced() {
    html_opts!(
        [extension.autolink],
        "http://example.com/[abc]]...\n",
        "<p><a href=\"http://example.com/%5Babc%5D%5D\">http://example.com/[abc]]</a>...</p>\n",
    );
}

#[test]
fn autolink_ignore_links_in_brackets() {
    let examples = [
        ["[https://foo.com]", "<p>[https://foo.com]</p>\n"],
        ["[[https://foo.com]]", "<p>[[https://foo.com]]</p>\n"],
        [
            "[[Foo|https://foo.com]]",
            "<p>[[Foo|https://foo.com]]</p>\n",
        ],
        [
            "[<https://foo.com>]",
            "<p>[<a href=\"https://foo.com\">https://foo.com</a>]</p>\n",
        ],
    ];

    for example in examples {
        html_opts!([extension.autolink], example[0], example[1], no_roundtrip);
    }
}

#[test_case(
    "see [a [b] http://x.example.com/](y) now",
    "<p>see <a href=\"y\">a [b] http://x.example.com/</a> now</p>\n"
)]
#[test_case(
    "see [a [b] http://x.example.com/ ](y) now",
    "<p>see <a href=\"y\">a [b] http://x.example.com/ </a> now</p>\n"
)]
#[test_case(
    "[![badge](b.svg) http://example.com/](http://example.com/)",
    "<p><a href=\"http://example.com/\"><img src=\"b.svg\" alt=\"badge\" /> http://example.com/</a></p>\n"
)]
fn autolink_nested_brackets(markdown: &str, html: &str) {
    html_opts!([extension.autolink], markdown, html);
}

#[test]
fn autolink_relaxed_links_in_brackets() {
    let examples = [
        [
            "[https://foo.com]",
            "<p>[<a href=\"https://foo.com\">https://foo.com</a>]</p>\n",
        ],
        [
            "[[https://foo.com]]",
            "<p>[[<a href=\"https://foo.com\">https://foo.com</a>]]</p>\n",
        ],
        [
            "[[Foo|https://foo.com]]",
            "<p>[[Foo|<a href=\"https://foo.com\">https://foo.com</a>]]</p>\n",
        ],
        [
            "[<https://foo.com>]",
            "<p>[<a href=\"https://foo.com\">https://foo.com</a>]</p>\n",
        ],
        [
            "[http://foo.com/](url)",
            "<p><a href=\"url\">http://foo.com/</a></p>\n",
        ],
        ["[http://foo.com/](url", "<p>[http://foo.com/](url</p>\n"],
        [
            "[www.foo.com/](url)",
            "<p><a href=\"url\">www.foo.com/</a></p>\n",
        ],
        [
            "{https://foo.com}",
            "<p>{<a href=\"https://foo.com\">https://foo.com</a>}</p>\n",
        ],
        [
            "[this http://and.com that](url)",
            "<p><a href=\"url\">this http://and.com that</a></p>\n",
        ],
        [
            "[this <http://and.com> that](url)",
            "<p><a href=\"url\">this http://and.com that</a></p>\n",
        ],
        [
            "{this http://and.com that}(url)",
            "<p>{this <a href=\"http://and.com\">http://and.com</a> that}(url)</p>\n",
        ],
        [
            "[http://foo.com](url)\n[http://bar.com]\n\n[http://bar.com]: http://bar.com/extra",
            "<p><a href=\"url\">http://foo.com</a>\n<a href=\"http://bar.com/extra\">http://bar.com</a></p>\n",
        ],
    ];

    for example in examples {
        html_opts!(
            [extension.autolink, parse.relaxed_autolinks],
            example[0],
            example[1]
        );
    }
}

#[test]
fn autolink_relaxed_links_brackets_balanced() {
    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "http://example.com/[abc]]...\n",
        "<p><a href=\"http://example.com/%5Babc%5D\">http://example.com/[abc]</a>]...</p>\n",
    );
}

#[test]
fn autolink_relaxed_links_curly_braces_balanced() {
    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "http://example.com/{abc}}...\n",
        "<p><a href=\"http://example.com/%7Babc%7D\">http://example.com/{abc}</a>}...</p>\n",
    );
}

#[test]
fn autolink_relaxed_links_curly_parentheses_balanced() {
    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "http://example.com/(abc))...\n",
        "<p><a href=\"http://example.com/(abc)\">http://example.com/(abc)</a>)...</p>\n",
    );
}

#[test]
fn autolink_relaxed_links_schemes() {
    let examples = [
        [
            "https://foo.com",
            "<p><a href=\"https://foo.com\">https://foo.com</a></p>\n",
        ],
        [
            "smb:///Volumes/shared/foo.pdf",
            "<p><a href=\"smb:///Volumes/shared/foo.pdf\">smb:///Volumes/shared/foo.pdf</a></p>\n",
        ],
        [
            "irc://irc.freenode.net/git",
            "<p><a href=\"irc://irc.freenode.net/git\">irc://irc.freenode.net/git</a></p>\n",
        ],
        [
            "rdar://localhost.com/blah",
            "<p><a href=\"rdar://localhost.com/blah\">rdar://localhost.com/blah</a></p>\n",
        ],
    ];

    for example in examples {
        html_opts!(
            [extension.autolink, parse.relaxed_autolinks],
            example[0],
            example[1]
        );
    }
}

#[test]
fn sourcepos_correctly_restores_context() {
    assert_ast_match!(
        [],
        "ab _cde_ f@g.ee h*ijklm* n",
        (document (1:1-1:26) [
            (paragraph (1:1-1:26) [
                (text (1:1-1:3) "ab ")
                (emph (1:4-1:8) [
                    (text (1:5-1:7) "cde")
                ])
                (text (1:9-1:17) " f@g.ee h")
                (emph (1:18-1:24) [
                    (text (1:19-1:23) "ijklm")
                ])
                (text (1:25-1:26) " n")
            ])
        ])
    );

    assert_ast_match!(
        [extension.autolink],
        "ab _cde_ f@g.ee h*ijklm* n",
        (document (1:1-1:26) [
            (paragraph (1:1-1:26) [
                (text (1:1-1:3) "ab ")
                (emph (1:4-1:8) [
                    (text (1:5-1:7) "cde")
                ])
                (text (1:9-1:9) " ")
                (link (1:10-1:15) "mailto:f@g.ee" [
                    (text (1:10-1:15) "f@g.ee")
                ])
                (text (1:16-1:17) " h")
                (emph (1:18-1:24) [
                    (text (1:19-1:23) "ijklm")
                ])
                (text (1:25-1:26) " n")
            ])
        ])
    );
}

#[test]
fn autolink_cmark_edge_382() {
    html_opts!(
        [extension.autolink],
        "See &lt;&lt;&lt;http://example.com/&gt;&gt;&gt;",
        "<p>See &lt;&lt;&lt;<a href=\"http://example.com/\">http://example.com/</a>&gt;&gt;&gt;</p>\n",
    );
}

#[test]
fn autolink_cmark_edge_388() {
    html_opts!(
        [extension.autolink],
        "http://example.com/src/_mocks_/vscode.js",
        "<p><a href=\"http://example.com/src/_mocks_/vscode.js\">http://example.com/src/_mocks_/vscode.js</a></p>\n",
    );
}

#[test]
fn autolink_cmark_edge_423() {
    html_opts!(
        [extension.autolink, extension.strikethrough],
        concat!(
            "Here's an autolink: ",
            "https://www.unicode.org/review/pri453/feedback.html#:~:text=Fri%20Jun%2024%2009:56:01%20CDT%202022",
            " and another one ",
            "https://www.unicode.org/review/pri453/feedback.html#:~:text=Fri%20Jun%2024%2009:56:01%20CDT%202022",
            ".",
        ),
        concat!(
            "<p>Here's an autolink: ",
            r#"<a href="https://www.unicode.org/review/pri453/feedback.html#:~:text=Fri%20Jun%2024%2009:56:01%20CDT%202022">"#,
            "https://www.unicode.org/review/pri453/feedback.html#:~:text=Fri%20Jun%2024%2009:56:01%20CDT%202022",
            "</a> and another one ",
            r#"<a href="https://www.unicode.org/review/pri453/feedback.html#:~:text=Fri%20Jun%2024%2009:56:01%20CDT%202022">"#,
            "https://www.unicode.org/review/pri453/feedback.html#:~:text=Fri%20Jun%2024%2009:56:01%20CDT%202022",
            "</a>.</p>\n",
        ),
    );
}

#[test]
fn autolink_cmark_edge_58() {
    html_opts!(
        [extension.autolink, extension.superscript],
        "https://www.wolframalpha.com/input/?i=x^2+(y-(x^2)^(1/3))^2=1",
        concat!(
            "<p>",
            r#"<a href="https://www.wolframalpha.com/input/?i=x%5E2+(y-(x%5E2)%5E(1/3))%5E2=1">"#,
            "https://www.wolframalpha.com/input/?i=x^2+(y-(x^2)^(1/3))^2=1",
            "</a></p>\n",
        ),
    );
}

#[test]
fn autolink_failing_spec_image() {
    html_opts!(
        [extension.autolink],
        "![http://inline.com/image](http://inline.com/image)",
        "<p><img src=\"http://inline.com/image\" alt=\"http://inline.com/image\" /></p>\n",
    );
}

#[test]
fn autolink_failing_spec_underscores() {
    html_opts!(
        [extension.autolink],
        "Underscores not allowed in host name www.xxx.yyy._zzz",
        "<p>Underscores not allowed in host name www.xxx.yyy._zzz</p>\n",
    );
}

#[test]
fn autolink_fuzz_leading_colon() {
    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "://-",
        "<p><a href=\"://-\">://-</a></p>\n",
        no_roundtrip,
    );
}

#[test]
fn autolink_fuzz_we() {
    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "we://w",
        "<p><a href=\"we://w\">we://w</a></p>\n",
        no_roundtrip,
    );
}

#[test]
fn autolink_sourcepos() {
    assert_ast_match!(
        [extension.autolink],
        "a  www.makea.fish  x\n"
        "\n"
        "b  https://www.com  y\n"
        "\n"
        "c  foo@www.com  z\n"
        ,
        (document (1:1-5:17) [
            (paragraph (1:1-1:20) [
                (text (1:1-1:3) "a  ")
                (link (1:4-1:17) "http://www.makea.fish" [
                    (text (1:4-1:17) "www.makea.fish")
                ])
                (text (1:18-1:20) "  x")
            ])
            (paragraph (3:1-3:21) [
                (text (3:1-3:3) "b  ")
                (link (3:4-3:18) "https://www.com" [
                    (text (3:4-3:18) "https://www.com")
                ])
                (text (3:19-3:21) "  y")
            ])
            (paragraph (5:1-5:17) [
                (text (5:1-5:3) "c  ")
                (link (5:4-5:14) "mailto:foo@www.com" [
                    (text (5:4-5:14) "foo@www.com")
                ])
                (text (5:15-5:17) "  z")
            ])
        ])
    );
}

#[test]
fn autolink_consecutive_email() {
    assert_ast_match!(
        [extension.autolink],
        "scyther@pokemon.com/beedrill@pokemon.com",
        (document (1:1-1:40) [
            (paragraph (1:1-1:40) [
                (link (1:1-1:19) "mailto:scyther@pokemon.com" [
                    (text (1:1-1:19) "scyther@pokemon.com")
                ])
                (text (1:20-1:20) "/")
                (link (1:21-1:40) "mailto:beedrill@pokemon.com" [
                    (text (1:21-1:40) "beedrill@pokemon.com")
                ])
            ])
        ])
    );
}

#[test]
fn autolink_many_emails() {
    std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let arena = Arena::new();
            let mut options = Options::default();
            options.extension.autolink = true;
            parse_document(&arena, &"a@b.co ".repeat(10_000), &options);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn autolink_consecutive_email_smart() {
    assert_ast_match!(
        [extension.autolink, parse.smart],
        "scyther@pokemon.com--beedrill@pokemon.com",
        (document (1:1-1:41) [
            (paragraph (1:1-1:41) [
                (link (1:1-1:19) "mailto:scyther@pokemon.com" [
                    (text (1:1-1:19) "scyther@pokemon.com")
                ])
                (text (1:20-1:21) "–") // en-dash
                (link (1:22-1:41) "mailto:beedrill@pokemon.com" [
                    (text (1:22-1:41) "beedrill@pokemon.com")
                ])
            ])
        ])
    );
}

#[test]
fn autolink_not_generated_inside_links() {
    html_opts!(
        [extension.autolink],
        "[Contact user@example.com for help](http://support.example.com)",
        "<p><a href=\"http://support.example.com\">Contact user@example.com for help</a></p>\n",
    );

    html_opts!(
        [extension.autolink],
        "[Visit https://www.example.com](http://example.com)",
        "<p><a href=\"http://example.com\">Visit https://www.example.com</a></p>\n",
    );

    html_opts!(
        [extension.autolink],
        "[Check out www.example.com](http://example.com)",
        "<p><a href=\"http://example.com\">Check out www.example.com</a></p>\n",
    );
}

#[test]
fn autolink_not_generated_inside_images() {
    // Test that emails inside image alt text don't become autolinks
    html_opts!(
        [extension.autolink],
        "![Contact user@example.com](image.png)",
        "<p><img src=\"image.png\" alt=\"Contact user@example.com\" /></p>\n",
    );

    html_opts!(
        [extension.autolink],
        "![Visit https://www.example.com](image.png)",
        "<p><img src=\"image.png\" alt=\"Visit https://www.example.com\" /></p>\n",
    );

    html_opts!(
        [extension.autolink],
        "![Check www.example.com](image.png)",
        "<p><img src=\"image.png\" alt=\"Check www.example.com\" /></p>\n",
    );
}

#[test]
fn autolink_not_generated_inside_wikilinks() {
    html_opts!(
        [extension.autolink, extension.wikilinks_title_after_pipe],
        "[[http://example.com|Contact user@example.com]]",
        "<p><a href=\"http://example.com\" data-wikilink=\"true\">Contact user@example.com</a></p>\n",
    );

    html_opts!(
        [extension.autolink, extension.wikilinks_title_after_pipe],
        "[[http://example.com|Visit https://www.example.com]]",
        "<p><a href=\"http://example.com\" data-wikilink=\"true\">Visit https://www.example.com</a></p>\n",
    );

    html_opts!(
        [extension.autolink, extension.wikilinks_title_before_pipe],
        "[[Check www.example.com|http://example.com]]",
        "<p><a href=\"http://example.com\" data-wikilink=\"true\">Check www.example.com</a></p>\n",
    );
}

#[test]
fn ipv6_host_scoped() {
    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "scoped https://[fe80::1ff:fe23:4567:890a%25eth2]",
        "<p>scoped <a href=\"https://[fe80::1ff:fe23:4567:890a%25eth2]\">https://[fe80::1ff:fe23:4567:890a%25eth2]</a></p>\n",
    );
}

#[test]
fn ipv6_relaxed() {
    html_opts!(
        [extension.autolink],
        "hi: nex://[fe80::1ff:fe23:4567:890a%25eth2]/z/x",
        "<p>hi: nex://[fe80::1ff:fe23:4567:890a%25eth2]/z/x</p>\n",
    );

    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "hi: nex://[fe80::1ff:fe23:4567:890a%25eth2]/z/x",
        "<p>hi: <a href=\"nex://[fe80::1ff:fe23:4567:890a%25eth2]/z/x\">nex://[fe80::1ff:fe23:4567:890a%25eth2]/z/x</a></p>\n",
    );
}

#[test]
fn autolink_bare_www() {
    html_opts!(
        [extension.autolink],
        "foo www. foo",
        "<p>foo www. foo</p>\n",
    );

    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "foo www. foo",
        "<p>foo <a href=\"http://www\">www</a>. foo</p>\n",
    );
}

#[test]
fn autolink_bare_scheme() {
    html_opts!(
        [extension.autolink],
        "foo http:// foo",
        "<p>foo http:// foo</p>\n",
    );

    html_opts!(
        [extension.autolink, parse.relaxed_autolinks],
        "foo http:// foo",
        "<p>foo <a href=\"http://\">http://</a> foo</p>\n",
    );
}

#[test_case(
    "see http://localhost/x now",
    "<p>see <a href=\"http://localhost/x\">http://localhost/x</a> now</p>\n"
)]
#[test_case(
    "see http://localhost:3000/admin now",
    "<p>see <a href=\"http://localhost:3000/admin\">http://localhost:3000/admin</a> now</p>\n"
)]
#[test_case(
    "see http://user:pass@www.example.com/ now",
    "<p>see <a href=\"http://user:pass@www.example.com/\">http://user:pass@www.example.com/</a> now</p>\n"
)]
#[test_case("http://x", "<p><a href=\"http://x\">http://x</a></p>\n")]
fn autolink_short_domains(markdown: &str, html: &str) {
    html_opts!([extension.autolink], markdown, html);
}

#[test_case("http://-foo.com", "<p>http://-foo.com</p>\n")]
#[test_case("http://.foo", "<p>http://.foo</p>\n")]
#[test_case("foo http://. foo", "<p>foo http://. foo</p>\n")]
fn autolink_domain_leading_invalid_char(markdown: &str, html: &str) {
    html_opts!([extension.autolink], markdown, html);
}

#[test_case(
    "see HTTP://www.example.com/ now",
    "<p>see <a href=\"HTTP://www.example.com/\">HTTP://www.example.com/</a> now</p>\n"
)]
#[test_case(
    "see Http://www.example.com/ now",
    "<p>see <a href=\"Http://www.example.com/\">Http://www.example.com/</a> now</p>\n"
)]
#[test_case(
    "FTP://ftp.example.com/file",
    "<p><a href=\"FTP://ftp.example.com/file\">FTP://ftp.example.com/file</a></p>\n"
)]
fn autolink_scheme_case_insensitive(markdown: &str, html: &str) {
    html_opts!([extension.autolink], markdown, html);
}

#[test]
fn autolink_non_special_w_colon_dollar_sourcepos() {
    assert_ast_match!(
        [extension.autolink],
        "foo www.example.com bar wwww.example.com xwww.example.com awww w ww www\nsee http://example.com/x and https://a.b/c, ftp://x.y/z; then a: b :// c ://d wa://foo\nhello\\wwa://bar.baz and \\www.x.y plus :// and :/ and http:/ and w\nprice $5 and $$ and $ x [^www] ref [^a:b] and [^$w]\ntail w\ntail w  \nnext :\ntail www.\nend ://\n",
        (document (1:1-9:7) [
            (paragraph (1:1-9:7) [
                (text (1:1-1:4) "foo ")
                (link (1:5-1:19) "http://www.example.com" [
                    (text (1:5-1:19) "www.example.com")
                ])
                (text (1:20-1:71) " bar wwww.example.com xwww.example.com awww w ww www")
                (softbreak (1:72-1:72))
                (text (2:1-2:4) "see ")
                (link (2:5-2:24) "http://example.com/x" [
                    (text (2:5-2:24) "http://example.com/x")
                ])
                (text (2:25-2:29) " and ")
                (link (2:30-2:42) "https://a.b/c" [
                    (text (2:30-2:42) "https://a.b/c")
                ])
                (text (2:43-2:44) ", ")
                (link (2:45-2:55) "ftp://x.y/z" [
                    (text (2:45-2:55) "ftp://x.y/z")
                ])
                (text (2:56-2:86) "; then a: b :// c ://d wa://foo")
                (softbreak (2:87-2:87))
                (text (3:1-3:65) "hello\\wwa://bar.baz and \\www.x.y plus :// and :/ and http:/ and w")
                (softbreak (3:66-3:66))
                (text (4:1-4:51) "price $5 and $$ and $ x [^www] ref [^a:b] and [^$w]")
                (softbreak (4:52-4:52))
                (text (5:1-5:6) "tail w")
                (softbreak (5:7-5:7))
                (text (6:1-6:6) "tail w")
                (linebreak (6:7-6:9))
                (text (7:1-7:6) "next :")
                (softbreak (7:7-7:7))
                (text (8:1-8:9) "tail www.")
                (softbreak (8:10-8:10))
                (text (9:1-9:7) "end ://")
            ])
        ])
    );

    assert_ast_match!(
        [extension.autolink],
        "end w",
        (document (1:1-1:5) [
            (paragraph (1:1-1:5) [
                (text (1:1-1:5) "end w")
            ])
        ])
    );

    assert_ast_match!(
        [extension.autolink],
        "end :",
        (document (1:1-1:5) [
            (paragraph (1:1-1:5) [
                (text (1:1-1:5) "end :")
            ])
        ])
    );

    assert_ast_match!(
        [extension.autolink],
        "end $",
        (document (1:1-1:5) [
            (paragraph (1:1-1:5) [
                (text (1:1-1:5) "end $")
            ])
        ])
    );

    assert_ast_match!(
        [extension.autolink],
        "a !$ b \\w c w\\ d\n",
        (document (1:1-1:16) [
            (paragraph (1:1-1:16) [
                (text (1:1-1:16) "a !$ b \\w c w\\ d")
            ])
        ])
    );
}

#[test]
fn autolink_non_special_w_colon_dollar_no_extensions() {
    assert_ast_match!(
        [],
        "foo www.example.com bar wwww.example.com xwww.example.com awww w ww www\nsee http://example.com/x and https://a.b/c, ftp://x.y/z; then a: b :// c ://d wa://foo\nhello\\wwa://bar.baz and \\www.x.y plus :// and :/ and http:/ and w\nprice $5 and $$ and $ x [^www] ref [^a:b] and [^$w]\ntail w\ntail w  \nnext :\ntail www.\nend ://\n",
        (document (1:1-9:7) [
            (paragraph (1:1-9:7) [
                (text (1:1-1:71) "foo www.example.com bar wwww.example.com xwww.example.com awww w ww www")
                (softbreak (1:72-1:72))
                (text (2:1-2:86) "see http://example.com/x and https://a.b/c, ftp://x.y/z; then a: b :// c ://d wa://foo")
                (softbreak (2:87-2:87))
                (text (3:1-3:65) "hello\\wwa://bar.baz and \\www.x.y plus :// and :/ and http:/ and w")
                (softbreak (3:66-3:66))
                (text (4:1-4:51) "price $5 and $$ and $ x [^www] ref [^a:b] and [^$w]")
                (softbreak (4:52-4:52))
                (text (5:1-5:6) "tail w")
                (softbreak (5:7-5:7))
                (text (6:1-6:6) "tail w")
                (linebreak (6:7-6:9))
                (text (7:1-7:6) "next :")
                (softbreak (7:7-7:7))
                (text (8:1-8:9) "tail www.")
                (softbreak (8:10-8:10))
                (text (9:1-9:7) "end ://")
            ])
        ])
    );
}

#[test]
fn autolink_non_special_w_colon_dollar_footnotes_math() {
    assert_ast_match!(
        [extension.autolink, extension.footnotes, extension.math_dollars, parse.relaxed_autolinks],
        "see [^www] ref [^a:b] and [^$w]\n\n[^www]: one\n[^a:b]: two\n[^$w]: three\n",
        (document (1:1-5:12) [
            (paragraph (1:1-1:31) [
                (text (1:1-1:4) "see ")
                (footnote_reference (1:5-1:10))
                (text (1:11-1:15) " ref ")
                (footnote_reference (1:16-1:21))
                (text (1:22-1:26) " and ")
                (footnote_reference (1:27-1:31))
            ])
            (footnote_definition (3:1-3:11) [
                (paragraph (3:9-3:11) [
                    (text (3:9-3:11) "one")
                ])
            ])
            (footnote_definition (4:1-4:11) [
                (paragraph (4:9-4:11) [
                    (text (4:9-4:11) "two")
                ])
            ])
            (footnote_definition (5:1-5:12) [
                (paragraph (5:8-5:12) [
                    (text (5:8-5:12) "three")
                ])
            ])
        ])
    );
}

#[test]
fn autolink_with_unicode_isolation() {
    html_opts!(
        [extension.autolink],
        "https://www.example.com/",
        "<p><a href=\"https://www.example.com/\">https://www.example.com/</a></p>\n",
    );

    html_opts!(
        [extension.autolink],
        "\u{2068}https://www.example.com/\u{2069}",
        "<p>\u{2068}<a href=\"https://www.example.com/\">https://www.example.com/</a>\u{2069}</p>\n",
    );
}
