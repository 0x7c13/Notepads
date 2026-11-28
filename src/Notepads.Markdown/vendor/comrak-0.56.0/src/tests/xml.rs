use super::*;

#[test]
fn base() {
    let input = concat!(
        "foo *bar*\n",
        "\n",
        "paragraph 2\n",
        "\n",
        "```\n",
        "code\n",
        "```\n",
    );

    xml(
        input,
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE document SYSTEM \"CommonMark.dtd\">\n",
            "<document xmlns=\"http://commonmark.org/xml/1.0\">\n",
            "  <paragraph>\n",
            "    <text xml:space=\"preserve\">foo </text>\n",
            "    <emph>\n",
            "      <text xml:space=\"preserve\">bar</text>\n",
            "    </emph>\n",
            "  </paragraph>\n",
            "  <paragraph>\n",
            "    <text xml:space=\"preserve\">paragraph 2</text>\n",
            "  </paragraph>\n",
            "  <code_block xml:space=\"preserve\">code\n",
            "</code_block>\n",
            "</document>\n",
        ),
    );
}

#[test]
fn sourcepos() {
    let input = concat!(
        "foo *bar*\n",
        "\n",
        "paragraph 2\n",
        "\n",
        "```\n",
        "code\n",
        "```\n",
    );

    xml_opts(
        input,
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE document SYSTEM \"CommonMark.dtd\">\n",
            "<document sourcepos=\"1:1-7:3\" xmlns=\"http://commonmark.org/xml/1.0\">\n",
            "  <paragraph sourcepos=\"1:1-1:9\">\n",
            "    <text sourcepos=\"1:1-1:4\" xml:space=\"preserve\">foo </text>\n",
            "    <emph sourcepos=\"1:5-1:9\">\n",
            "      <text sourcepos=\"1:6-1:8\" xml:space=\"preserve\">bar</text>\n",
            "    </emph>\n",
            "  </paragraph>\n",
            "  <paragraph sourcepos=\"3:1-3:11\">\n",
            "    <text sourcepos=\"3:1-3:11\" xml:space=\"preserve\">paragraph 2</text>\n",
            "  </paragraph>\n",
            "  <code_block sourcepos=\"5:1-7:3\" xml:space=\"preserve\">code\n",
            "</code_block>\n",
            "</document>\n",
        ),
        |opts| opts.render.sourcepos = true,
    );
}
