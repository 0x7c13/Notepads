use super::*;

#[test]
fn subscript() {
    html_opts!([extension.subscript], "H~2~O\n", "<p>H<sub>2</sub>O</p>\n",);
}

#[test]
fn strikethrough_and_subscript() {
    html_opts!(
        [extension.subscript, extension.strikethrough],
        "~~H~2~O~~\n",
        "<p><del>H<sub>2</sub>O</del></p>\n",
    );
}

#[test]
fn no_strikethrough_when_only_subscript() {
    html_opts!(
        [extension.subscript],
        "~~H~2~O~~\n",
        "<p>~~H<sub>2</sub>O~~</p>\n",
    );
}
