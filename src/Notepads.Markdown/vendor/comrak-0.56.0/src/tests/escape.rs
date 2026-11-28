use crate::cm::{escape_inline, escape_link_destination};
use crate::{Arena, Options, entity, format_commonmark, markdown_to_html, parse_document};

#[test]
fn href_escape_multiple_matches() {
    for (input, expected) in [
        ("", ""),
        ("https://example.com/a?b=c", "https://example.com/a?b=c"),
        (
            "é<&\"'>\0尾",
            "%C3%A9%3C&amp;%22&#x27;%3E%EF%BF%BD%E5%B0%BE",
        ),
        ("%20%aF%2%zz%", "%20%aF%252%25zz%25"),
        (
            "https://[::1]/é?q=[x]&a=%20",
            "https://[::1]/%C3%A9?q=%5Bx%5D&amp;a=%20",
        ),
    ] {
        let mut output = String::new();
        crate::html::escape_href(&mut output, input, false).unwrap();
        assert_eq!(output, expected, "input: {input:?}");
    }

    for (relaxed_ipv6, expected) in [
        (false, "custom://%5B::1%5D/%C3%A9"),
        (true, "custom://[::1]/%C3%A9"),
    ] {
        let mut output = String::new();
        crate::html::escape_href(&mut output, "custom://[::1]/é", relaxed_ipv6).unwrap();
        assert_eq!(output, expected);
    }
}

/// Assert that the input text escapes to the expected result in inline context,
/// and that the expected result renders to HTML which displays the input text.
#[track_caller]
fn assert_escape_inline(input: &str, expected: &str) {
    let actual = escape_inline(input);
    assert_eq!(expected, actual);
    let mut html = markdown_to_html(expected, &Options::default());
    html = html
        .strip_prefix("<p>")
        .expect("html should be one paragraph")
        .to_string();
    html = html
        .strip_suffix("</p>\n")
        .expect("html should be one paragraph")
        .to_string();
    assert_eq!(input, entity::unescape_html(&html));
}

#[test]
fn escape_inline_baseline() {
    assert_escape_inline("abcdefg", "abcdefg");
    assert_escape_inline("*hello*", r#"\*hello\*"#);
    assert_escape_inline(
        "[A link](https://link.com)",
        r#"\[A link\]\(https://link\.com\)"#,
    );
    assert_escape_inline(
        r#"some <"complicated"> & '/problematic\' input"#,
        r#"some \<\"complicated\"\> \& '/problematic\\' input"#,
    );
}

/// Assert that the URL is escaped as expected, and that the result is rendered
/// into HTML in such a way that preserves the meaning of the input.
///
/// [link destination]: https://spec.commonmark.org/0.31.2/#link-destination
#[test]
fn escape_link_target() {
    let url = "rabbits) <cup\rcakes\n> [%7Bhya%cinth%7d](";
    let escaped = r#"<rabbits) \<cup%0Dcakes%0A\> [%7Bhya%cinth%7d](>"#;
    let decoded = "rabbits) <cup\rcakes\n> [{hya%cinth}](";

    assert_eq!(escaped, escape_link_destination(url));

    let md = format!("[link]({escaped})");
    let mut html = markdown_to_html(&md, &Options::default());
    html = html
        .strip_prefix("<p><a href=\"")
        .expect("html should be one anchor in a paragraph")
        .to_string();
    html = html
        .strip_suffix("\">link</a></p>\n")
        .expect("html should be one anchor in a paragraph")
        .to_string();

    assert_eq!(
        "rabbits)%20%3Ccup%0Dcakes%0A%3E%20%5B%7Bhya%25cinth%7d%5D(",
        html
    );
    assert_eq!(
        decoded,
        percent_encoding_rfc3986::percent_decode_str(&html)
            .unwrap()
            .decode_utf8()
            .unwrap()
    );
}

#[test]
fn escape_strikethrough() {
    assert_escape_inline("~~text~~", "\\~\\~text\\~\\~");
}

#[test]
fn escape_block_directive_colon() {
    // Block directives use ::: syntax at line start, so : is only escaped
    // when at the beginning of content
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.block_directive = true;

    let input = ":::foo bar\ncontent\n:::\n";
    let root = parse_document(&arena, input, &options);
    let mut output = String::new();
    format_commonmark(root, &options, &mut output).unwrap();
    assert!(
        output.contains(r#"\:\:\:"#) || output.contains(r#":::foo"#),
        "Output was: {}",
        output
    );
}
