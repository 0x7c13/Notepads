// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

#![cfg(not(target_arch = "riscv64"))]

use super::*;
use ntest::timeout;
use pretty_assertions::assert_eq;

// input: python3 -c 'n = 50000; print("*a_ " * n)'
#[test]
#[timeout(4000)]
fn pathological_emphases() {
    let n = 50_000;
    let input = "*a_ ".repeat(n);
    let mut exp = format!("<p>{}", input);
    // Right-most space is trimmed in output.
    exp.pop();
    exp += "</p>\n";

    html(&input, &exp);
}

#[test]
#[timeout(4000)]
fn pathological_emphasis_mod_three_1() {
    let n = 40_000;
    let input = "x**x".to_owned() + &" _x".repeat(n) + &"x* ".repeat(n);
    let exp = format!("<p>{}</p>\n", input.trim_end());

    html(&input, &exp);
}

#[test]
#[timeout(4000)]
fn pathological_emphasis_mod_three_2() {
    let n = 20_000;
    let input = "*x ".repeat(n) + &".**".repeat(n);
    let exp = format!(
        "<p>{}{}</p>\n",
        "*x ".repeat(n),
        ".<strong>.</strong>".repeat(n / 2)
    );

    html(&input, &exp);
}

// input: python3 -c 'n = 10000; print("|" + "x|" * n + "\n|" + "-|" * n)'
#[test]
#[timeout(4000)]
fn pathological_table_columns_1() {
    let n = 100_000;
    let input = format!("{}{}{}{}", "|", "x|".repeat(n), "\n|", "-|".repeat(n));
    let exp = format!("<p>{}</p>\n", input);

    html_opts!([extension.table], &input, &exp);
}

// input: python3 -c 'n = 70000; print("|" + "x|" * n + "\n|" + "-|" * n + "\n" + "a\n" * n)'
#[test]
#[timeout(4000)]
fn pathological_table_columns_2() {
    let n = 100_000;
    let input = format!(
        "{}{}{}{}{}{}",
        "|",
        "x|".repeat(n),
        "\n|",
        "-|".repeat(n),
        "\n",
        "a\n".repeat(n)
    );

    let extension = parser::options::Extension {
        table: true,
        ..Default::default()
    };

    // Not interested in the actual html, just that we don't timeout
    markdown_to_html(
        &input,
        &Options {
            extension,
            ..Default::default()
        },
    );
}

#[test]
#[timeout(4000)]
fn pathological_table_autocompleted_cells() {
    let c = 2_000;
    let r = 2_000;
    let input = format!(
        "{}\n{}\n{}",
        "|".repeat(c + 1),
        "|-".repeat(c) + "|",
        "|a\n".repeat(r)
    );

    let options = Options {
        extension: parser::options::Extension {
            table: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let arena = crate::Arena::new();
    let root = parse_document(&arena, &input, &options);
    let table = root
        .descendants()
        .find(|n| matches!(n.data().value, NodeValue::Table(..)))
        .expect("table node missing");
    let num_rows = table
        .children()
        .filter(|n| matches!(n.data().value, NodeValue::TableRow(..)))
        .count();
    assert_eq!(num_rows, 252);
    assert!(matches!(
        table.next_sibling(),
        Some(n) if matches!(n.data().value, NodeValue::Paragraph)
    ));
}

// input: python3 -c 'n = 10000; print("[^1]:" * n + "\n" * n)'
#[test]
#[timeout(4000)]
fn pathological_footnotes() {
    let n = 10_000;
    let input = format!("{}{}", "[^1]:".repeat(n), "\n".repeat(n));
    let exp = "";

    html_opts!([extension.footnotes], &input, exp);
}

#[test]
fn pathological_recursion() {
    let n = 5_000;
    let input = format!("{}{}", "*a **a ".repeat(n), " a** a*".repeat(n));
    let exp = format!(
        "<p>{}{}</p>\n",
        "<em>a <strong>a ".repeat(n),
        " a</strong> a</em>".repeat(n)
    );

    // The footnote code happened to be the cause of the pathological recursion, but it didn't
    // actually depend on any footnotes being present; it was the search that caused it.
    html_opts!([extension.footnotes], &input, &exp);
}

#[test]
fn pathological_recursion_inline_footnotes() {
    let n = 5_000;
    let input = format!("{}{}", "^[a ".repeat(n), " b]".repeat(n));
    let exp = format!(
        "{}{}{}{}",
        concat!(
            "<p><sup class=\"footnote-ref\"><a href=\"#fn-__inline_1\" id=\"fnref-__inline_1\" data-footnote-ref>1</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n<ol>\n",
            "<li id=\"fn-__inline_1\">\n<p>a <sup class=\"footnote-ref\"><a href=\"#fn-__inline_2\" id=\"fnref-__inline_2\" data-footnote-ref>5</a></sup> b <a href=\"#fnref-__inline_1\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n</li>\n",
            "<li id=\"fn-__inline_5\">\n<p>a ",
        ),
        "^[a ".repeat(4995),
        " b]".repeat(4995),
        concat!(
            " b <a href=\"#fnref-__inline_5\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"2\" aria-label=\"Back to reference 2\">↩</a></p>\n</li>\n",
            "<li id=\"fn-__inline_4\">\n<p>a <sup class=\"footnote-ref\"><a href=\"#fn-__inline_5\" id=\"fnref-__inline_5\" data-footnote-ref>2</a></sup> b <a href=\"#fnref-__inline_4\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"3\" aria-label=\"Back to reference 3\">↩</a></p>\n</li>\n",
            "<li id=\"fn-__inline_3\">\n<p>a <sup class=\"footnote-ref\"><a href=\"#fn-__inline_4\" id=\"fnref-__inline_4\" data-footnote-ref>3</a></sup> b <a href=\"#fnref-__inline_3\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"4\" aria-label=\"Back to reference 4\">↩</a></p>\n</li>\n",
            "<li id=\"fn-__inline_2\">\n<p>a <sup class=\"footnote-ref\"><a href=\"#fn-__inline_3\" id=\"fnref-__inline_3\" data-footnote-ref>4</a></sup> b <a href=\"#fnref-__inline_2\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"5\" aria-label=\"Back to reference 5\">↩</a></p>\n</li>\n",
            "</ol>\n</section>\n"
        )
    );

    html_opts!(
        [extension.footnotes, extension.inline_footnotes],
        &input,
        &exp,
        no_roundtrip
    );
}

#[test]
#[timeout(4000)]
fn pathological_footnote_label_reconstruction() {
    let n = 20_000;
    let input = "[^".repeat(n) + &"]".repeat(n);
    let exp = format!("<p>{}</p>\n", input);

    html_opts!([extension.footnotes], &input, &exp);
}

#[test]
#[timeout(4000)]
fn pathological_description_lists() {
    let n = 20_000;
    let input = "t\n: d\n\n".repeat(n);
    let exp = format!("<dl>\n{}</dl>\n", "<dt>t</dt>\n<dd>d</dd>\n".repeat(n));

    html_opts!([extension.description_lists], &input, &exp, no_roundtrip);
}

#[test]
#[timeout(4000)]
fn pathological_relaxed_autolink_url_dead_zone() {
    let n = 40_000;
    let input = "://a".repeat(n) + "](";
    let exp = format!("<p>{}</p>\n", input);

    html_opts!([extension.autolink, parse.relaxed_autolinks], &input, &exp);
}

#[test]
#[timeout(4000)]
fn pathological_relaxed_autolink_www_dead_zone() {
    let n = 40_000;
    let input = "~www.a".repeat(n) + "](";
    let exp = format!("<p>{}</p>\n", input);

    html_opts!([extension.autolink, parse.relaxed_autolinks], &input, &exp);
}

#[test]
#[timeout(4000)]
fn pathological_header_ids() {
    let n = 20_000;
    let input = "# a\n".repeat(n);
    let mut exp = String::new();
    for i in 0..n {
        let id = if i == 0 {
            "a".to_string()
        } else {
            format!("a-{}", i)
        };
        exp.push_str(&format!(
            "<h1 id=\"{id}\">a<a href=\"#{id}\" aria-label=\"Link to heading 'a'\" data-heading-content=\"a\" class=\"anchor\"></a></h1>\n"
        ));
    }

    html_opts!(
        [extension.header_id_prefix = Some("".to_owned())],
        &input,
        &exp,
        no_roundtrip
    );
}
