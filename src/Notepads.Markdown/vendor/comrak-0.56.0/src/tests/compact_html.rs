use super::*;

#[test]
fn compact_paragraph() {
    html_opts!(
        [render.compact_html],
        "Hello world.\n",
        "<p>Hello world.</p>",
    );
}

#[test]
fn compact_multiple_paragraphs() {
    html_opts!(
        [render.compact_html],
        "Paragraph one.\n\nParagraph two.\n",
        "<p>Paragraph one.</p><p>Paragraph two.</p>",
    );
}

#[test]
fn compact_heading() {
    html_opts!(
        [render.compact_html],
        "# Hello\n\nWorld.\n",
        "<h1>Hello</h1><p>World.</p>",
    );
}

#[test]
fn compact_list() {
    html_opts!(
        [render.compact_html],
        "- one\n- two\n- three\n",
        "<ul><li>one</li><li>two</li><li>three</li></ul>",
    );
}

#[test]
fn compact_ordered_list() {
    html_opts!(
        [render.compact_html],
        "1. one\n2. two\n3. three\n",
        "<ol><li>one</li><li>two</li><li>three</li></ol>",
    );
}

#[test]
fn compact_blockquote() {
    html_opts!(
        [render.compact_html],
        "> quoted\n",
        "<blockquote><p>quoted</p></blockquote>",
    );
}

#[test]
fn compact_code_block() {
    html_opts!(
        [render.compact_html],
        "```\nhello\n```\n",
        "<pre><code>hello\n</code></pre>",
    );
}

#[test]
fn compact_thematic_break() {
    html_opts!([render.compact_html], "---\n", "<hr />",);
}

#[test]
fn compact_table() {
    html_opts!(
        [extension.table, render.compact_html],
        "| a | b |\n|---|---|\n| c | d |\n",
        "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td>c</td><td>d</td></tr></tbody></table>",
    );
}

#[test]
fn compact_default_off() {
    html_opts!(
        [render.compact_html = false],
        "# Hello\n\nWorld.\n",
        "<h1>Hello</h1>\n<p>World.</p>\n",
    );
}
