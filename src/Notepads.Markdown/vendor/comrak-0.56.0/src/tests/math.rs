use super::*;
use ntest::test_case;

#[test_case("$2+2$", "<p><math>2+2</math></p>\n")]
#[test_case(
    "$\\alpha$$\\beta$",
    "<p><math>\\alpha</math><math>\\beta</math></p>\n"
)]
#[test_case("$2+2$$2+2$", "<p><math>2+2</math><math>2+2</math></p>\n")]
#[test_case("$22 and $2+2$", "<p>$22 and <math>2+2</math></p>\n")]
#[test_case("$a!$", "<p><math>a!</math></p>\n")]
#[test_case("$x$", "<p><math>x</math></p>\n")]
#[test_case("$1+2\\$$", "<p><math>1+2\\$</math></p>\n")]
#[test_case("$1+\\$2$", "<p><math>1+\\$2</math></p>\n")]
#[test_case("$1+\\%2$", "<p><math>1+\\%2</math></p>\n")]
#[test_case(
    "$22+1$ and $22 + a^2$",
    "<p><math>22+1</math> and <math>22 + a^2</math></p>\n"
)]
#[test_case(
    "$2+2$ $22 and dollars$22 $2+2$",
    "<p><math>2+2</math> $22 and dollars$22 <math>2+2</math></p>\n"
)]
#[test_case(
    "$1/2$ &lt;b&gt;test&lt;/b&gt;",
    "<p><math>1/2</math> &lt;b&gt;test&lt;/b&gt;</p>\n"
)]
fn math_dollars_inline(markdown: &str, html: &str) {
    let result = html
        .replace("<math>", "<span data-math-style=\"inline\">")
        .replace("</math>", "</span>");

    html_opts!([extension.math_dollars], markdown, &result);
}

#[test_case("$$2+2$$", "<p><math>2+2</math></p>\n")]
#[test_case("$$   2+2  $$", "<p><math>   2+2  </math></p>\n")]
#[test_case("$22 and $$2+2$$", "<p>$22 and <math>2+2</math></p>\n")]
#[test_case("$$a!$$", "<p><math>a!</math></p>\n")]
#[test_case("$$x$$", "<p><math>x</math></p>\n")]
#[test_case("$$20,000 and $$30,000", "<p><math>20,000 and </math>30,000</p>\n")]
#[test_case("test $$\n2+2\n$$", "<p>test <math>\n2+2\n</math></p>\n")]
#[test_case(
    "$$22+1$$ and $$22 + a^2$$",
    "<p><math>22+1</math> and <math>22 + a^2</math></p>\n"
)]
#[test_case(
    "$$2+2$$ $22 and dollars$22 $$2+2$$",
    "<p><math>2+2</math> $22 and dollars$22 <math>2+2</math></p>\n"
)]
#[test_case(
    "dollars$22 and $$a^2 + b^2 = c^2$$",
    "<p>dollars$22 and <math>a^2 + b^2 = c^2</math></p>\n"
)]
fn math_dollars_inline_display(markdown: &str, html: &str) {
    let result = html
        .replace("<math>", "<span data-math-style=\"display\">")
        .replace("</math>", "</span>");

    html_opts!([extension.math_dollars], markdown, &result);
}

#[test_case("$$\n2+2\n4+4\n$$", "<p><math>\n2+2\n4+4\n</math></p>\n")]
fn math_dollars_block(markdown: &str, html: &str) {
    let result = html
        .replace("<math>", "<span data-math-style=\"display\">")
        .replace("</math>", "</span>");

    html_opts!([extension.math_dollars], markdown, &result);
}

#[test_case("\\(2+2\\)", "<p><math>2+2</math></p>\n")]
#[test_case(
    "Calc \\(x^2\\) and \\(y^2\\)",
    "<p>Calc <math>x^2</math> and <math>y^2</math></p>\n"
)]
#[test_case("\\(\\alpha + \\beta\\)", "<p><math>\\alpha + \\beta</math></p>\n")]
fn math_latex_inline(markdown: &str, html: &str) {
    let result = html
        .replace("<math>", "<span data-math-style=\"inline\">")
        .replace("</math>", "</span>");

    html_opts!([extension.math_latex], markdown, &result);
}

#[test_case("\\[2+2\\]", "<p><math>2+2</math></p>\n")]
#[test_case("\\[\n2+2\n\\]", "<p><math>\n2+2\n</math></p>\n")]
fn math_latex_display(markdown: &str, html: &str) {
    let result = html
        .replace("<math>", "<span data-math-style=\"display\">")
        .replace("</math>", "</span>");

    html_opts!([extension.math_latex], markdown, &result);
}

#[test_case("`\\(2+2\\)`", "<p><code>\\(2+2\\)</code></p>\n")]
#[test_case(
    "```text\n\\[2+2\\]\n```",
    "<pre><code class=\"language-text\">\\[2+2\\]\n</code></pre>\n"
)]
#[test_case("\\(2+2", "<p>(2+2</p>\n")]
#[test_case("\\[2+2", "<p>[2+2</p>\n")]
#[test_case("\\(x\\]", "<p>(x]</p>\n")]
#[test_case("\\[x\\)", "<p>[x)</p>\n")]
#[test_case("\\(\\)", "<p>()</p>\n")]
#[test_case("\\[\\]", "<p>[]</p>\n")]
fn math_latex_unrecognized_syntax(markdown: &str, html: &str) {
    html_opts!([extension.math_latex], markdown, html);
}

#[test_case("$`2+2`$", "<p><math>2+2</math></p>\n")]
#[test_case("$22 and $`2+2`$", "<p>$22 and <math>2+2</math></p>\n")]
#[test_case("$`1+\\$2`$", "<p><math>1+\\$2</math></p>\n")]
#[test_case(
    "$`22+1`$ and $`22 + a^2`$",
    "<p><math>22+1</math> and <math>22 + a^2</math></p>\n"
)]
#[test_case(
    "$`2+2`$ $22 and dollars$22 $`2+2`$",
    "<p><math>2+2</math> $22 and dollars$22 <math>2+2</math></p>\n"
)]
fn math_code_inline(markdown: &str, html: &str) {
    let result = html
        .replace("<math>", "<code data-math-style=\"inline\">")
        .replace("</math>", "</code>");

    html_opts!([extension.math_code], markdown, &result);
}

#[test_case("```math\n2+2\n4+4\n```", "<pre><math>2+2\n4+4\n</math></pre>\n")]
fn math_code_block(markdown: &str, html: &str) {
    let result = html
        .replace(
            "<math>",
            "<code class=\"language-math\" data-math-style=\"display\">",
        )
        .replace("</math>", "</code>");

    html_opts!([extension.math_dollars], markdown, &result);
}

#[test_case("`2+2`", "<p><code>2+2</code></p>\n")]
#[test_case("test `2+2`$ test", "<p>test <code>2+2</code>$ test</p>\n")]
#[test_case("$20,000 and $30,000", "<p>$20,000 and $30,000</p>\n")]
#[test_case("$20,000 in $USD", "<p>$20,000 in $USD</p>\n")]
#[test_case("$ a^2 $", "<p>$ a^2 $</p>\n")]
#[test_case("$\n$", "<p>$\n$</p>\n")]
#[test_case("$$$", "<p>$$$</p>\n")]
#[test_case("`$1+2$`", "<p><code>$1+2$</code></p>\n")]
#[test_case("`$$1+2$$`", "<p><code>$$1+2$$</code></p>\n")]
#[test_case("`$\\$1+2$$`", "<p><code>$\\$1+2$$</code></p>\n")]
#[test_case("test $`2+2` test", "<p>test $<code>2+2</code> test</p>\n")]
fn math_unrecognized_syntax(markdown: &str, html: &str) {
    html_opts!(
        [extension.math_dollars, extension.math_code],
        markdown,
        html
    );
}

#[test_case("$`$", "<p data-sourcepos=\"1:1-1:3\">$`$</p>\n")]
fn math_unrecognized_syntax_non_roundtrip(markdown: &str, html: &str) {
    html_opts!(
        [
            extension.math_dollars,
            extension.math_code,
            render.sourcepos
        ],
        markdown,
        html,
        no_roundtrip
    );
}

#[test]
fn sourcepos() {
    assert_ast_match!(
        [extension.math_dollars, extension.math_code],
        "$x^2$ and $$y^2$$ and $`z^2`$\n"
        "\n"
        "$$\n"
        "a^2\n"
        "$$\n"
        "\n"
        "```math\n"
        "b^2\n"
        "```\n",
        (document (1:1-9:3) [
            (paragraph (1:1-1:29) [
                (math (1:1-1:5) "x^2")
                (text (1:6-1:10) " and ")
                (math (1:11-1:17) "y^2")
                (text (1:18-1:22) " and ")
                (math (1:23-1:29) "z^2")
            ])
            (paragraph (3:1-5:2) [
                (math (3:1-5:2) "\na^2\n")
            ])
            (code_block (7:1-9:3) info:"math" "b^2\n")
        ])
    );
}

#[test]
fn math_latex_sourcepos() {
    assert_ast_match!(
        [extension.math_latex],
        "\\(x^2\\) and \\[y^2\\]\n"
        "\n"
        "\\[\n"
        "a^2\n"
        "\\]\n",
        (document (1:1-5:2) [
            (paragraph (1:1-1:19) [
                (math (1:1-1:7) "x^2")
                (text (1:8-1:12) " and ")
                (math (1:13-1:19) "y^2")
            ])
            (paragraph (3:1-5:2) [
                (math (3:1-5:2) "\na^2\n")
            ])
        ])
    );
}
