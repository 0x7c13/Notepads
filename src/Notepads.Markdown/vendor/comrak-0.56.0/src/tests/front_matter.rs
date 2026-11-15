use pretty_assertions::assert_eq;

use crate::{Arena, Options, format_commonmark, parse_document};

use super::*;

#[test]
fn round_trip_one_field() {
    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let input = "---\nlayout: post\n---\nText\n";
    let root = parse_document(&arena, input, &options);
    let mut buf = String::new();
    format_commonmark(root, &options, &mut buf).unwrap();
    assert_eq!(buf, input);
}

#[test]
fn round_trip_wide_delimiter() {
    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("\u{04fc}".to_owned());
    let arena = Arena::new();
    let input = "\u{04fc}\nlayout: post\n\u{04fc}\nText\n";
    let root = parse_document(&arena, input, &options);
    let mut buf = String::new();
    format_commonmark(root, &options, &mut buf).unwrap();
    assert_eq!(buf, input);
}

#[test]
fn ast() {
    assert_ast_match!(
        [extension.front_matter_delimiter = Some("q".to_owned())],
        "q\nlayout: post\nq\nText\n",
        (document (1:1-4:4) [
            (frontmatter (1:1-3:1) "q\nlayout: post\nq\n")
            (paragraph (4:1-4:4) [
                (text (4:1-4:4) "Text")
            ])
        ])
    );
}

#[test]
fn ast_blank_line() {
    assert_ast_match!(
        [extension.front_matter_delimiter = Some("---".to_owned())],
        r#"---
a: b
---

hello world
"#,
        (document (1:1-5:11) [
            (frontmatter (1:1-3:3) "---\na: b\n---\n\n")
            (paragraph (5:1-5:11) [
                (text (5:1-5:11) "hello world")
            ])
        ])
    );
}

#[test]
fn ast_carriage_return() {
    assert_ast_match!(
        [extension.front_matter_delimiter = Some("q".to_owned())],
        "q\r\nlayout: post\r\nq\r\nText\r\n",
        (document (1:1-4:4) [
            (frontmatter (1:1-3:1) "q\r\nlayout: post\r\nq\r\n")
            (paragraph (4:1-4:4) [
                (text (4:1-4:4) "Text")
            ])
        ])
    );
}

#[test]
fn ast_lone_carriage_returns() {
    assert_ast_match!(
        [extension.front_matter_delimiter = Some("---".to_owned())],
        "---\n\r\r\r\u{fffd}\n---\n<details>\n<d\u{fffd}\u{fffd}\u{fffd}\n\r\r</details>",
        (document (1:1-11:10) [
            (frontmatter (1:1-6:3) "---\n\r\r\r\u{fffd}\n---\n")
            (html_block (7:1-8:11) "<details>\n<d\u{fffd}\u{fffd}\u{fffd}\n")
            (html_block (11:1-11:10) "</details>")
        ])
    );
}

#[test]
fn ast_mixed_line_endings_with_blank_line() {
    assert_ast_match!(
        [extension.front_matter_delimiter = Some("---".to_owned())],
        "---\r\none\rtwo\nthree\r\n---\r\n\r\n# 中文\n\nTail\n",
        (document (1:1-9:4) [
            (frontmatter (1:1-5:3) "---\r\none\rtwo\nthree\r\n---\r\n\r\n")
            (heading (7:1-7:8) [
                (text (7:3-7:8) "中文")
            ])
            (paragraph (9:1-9:4) [
                (text (9:1-9:4) "Tail")
            ])
        ])
    );
}

#[test]
fn ast_wide_delimiter() {
    assert_ast_match!(
        [extension.front_matter_delimiter = Some("\u{04fc}".to_owned())],
        "\u{04fc}\nlayout: post\n\u{04fc}\nText\n",
        (document (1:1-4:4) [
            (frontmatter (1:1-3:2) "\u{04fc}\nlayout: post\n\u{04fc}\n")
            (paragraph (4:1-4:4) [
                (text (4:1-4:4) "Text")
            ])
        ])
    );
}

#[test]
fn trailing_space_open() {
    let input = "--- \nlayout: post\n---\nText\n";

    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let root = parse_document(&arena, input, &options);

    let found = root
        .descendants()
        .find(|n| node_matches!(n, NodeValue::FrontMatter(..)));

    assert!(found.is_none(), "no FrontMatter expected");
}

#[test]
fn leading_space_open() {
    let input = " ---\nlayout: post\n---\nText\n";

    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let root = parse_document(&arena, input, &options);

    let found = root
        .descendants()
        .find(|n| node_matches!(n, NodeValue::FrontMatter(..)));

    assert!(found.is_none(), "no FrontMatter expected");
}

#[test]
fn leading_space_close() {
    let input = "---\nlayout: post\n ---\nText\n";

    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let root = parse_document(&arena, input, &options);

    let found = root
        .descendants()
        .find(|n| node_matches!(n, NodeValue::FrontMatter(..)));

    assert!(found.is_none(), "no FrontMatter expected");
}

#[test]
fn trailing_space_close() {
    let input = "---\nlayout: post\n--- \nText\n";

    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let root = parse_document(&arena, input, &options);

    let found = root
        .descendants()
        .find(|n| node_matches!(n, NodeValue::FrontMatter(..)));

    assert!(found.is_none(), "no FrontMatter expected");
}

#[test]
fn second_line() {
    let input = "\n---\nlayout: post\n ---\nText\n";

    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let root = parse_document(&arena, input, &options);

    let found = root
        .descendants()
        .find(|n| node_matches!(n, NodeValue::FrontMatter(..)));

    assert!(found.is_none(), "no FrontMatter expected");
}

#[test]
fn fm_only_with_trailing_newline() {
    let input = "---\nfoo: bar\n---\n";

    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let root = parse_document(&arena, input, &options);

    let found = root
        .descendants()
        .find(|n| node_matches!(n, NodeValue::FrontMatter(..)));

    assert!(found.is_some(), "front matter expected");
}

#[test]
fn fm_only_without_trailing_newline() {
    let input = "---\nfoo: bar\n---";

    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_owned());
    let arena = Arena::new();
    let root = parse_document(&arena, input, &options);

    let found = root
        .descendants()
        .find(|n| node_matches!(n, NodeValue::FrontMatter(..)));

    assert!(found.is_some(), "front matter expected");
}
