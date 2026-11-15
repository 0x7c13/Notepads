use super::*;
use ntest::test_case;

#[test_case("\\@user", "<p><span data-escaped-char>@</span>user</p>\n")]
#[test_case("This\\@that", "<p>This<span data-escaped-char>@</span>that</p>\n")]
fn escaped_char_spans(markdown: &str, html: &str) {
    html_opts!([render.escaped_char_spans], markdown, html, no_roundtrip);
}

#[test_case("\\@user", "<p>@user</p>\n")]
#[test_case("This\\@that", "<p>This@that</p>\n")]
fn disabled_escaped_char_spans(markdown: &str, expected: &str) {
    html(markdown, expected);
}

#[test]
fn escaped_char_span_sourcepos() {
    assert_ast_match!(
        [render.escaped_char_spans],
        "Test \\`hello world\\` here.",
        (document (1:1-1:26) [
            (paragraph (1:1-1:26) [
                (text (1:1-1:5) "Test ")
                (escaped (1:6-1:7) [
                    (text (1:7-1:7) "`")
                ])
                (text (1:8-1:18) "hello world")
                (escaped (1:19-1:20) [
                    (text (1:20-1:20) "`")
                ])
                (text (1:21-1:26) " here.")
            ])
        ])
    );
}

#[test]
fn to_cm_ok() {
    let mut options = Options::default();
    options.render.escaped_char_spans = true;
    markdown_to_commonmark("\\]", &options);
}
