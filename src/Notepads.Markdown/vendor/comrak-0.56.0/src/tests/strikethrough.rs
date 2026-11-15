use super::*;

#[test]
fn strikethrough() {
    html_opts!(
        [extension.strikethrough],
        concat!(
            "This is ~strikethrough~.\n",
            "\n",
            "As is ~~this, okay~~?\n",
            "\n",
            "This ~text~~~~ is ~~~~curious~.\n",
        ),
        concat!(
            "<p>This is <del>strikethrough</del>.</p>\n",
            "<p>As is <del>this, okay</del>?</p>\n",
            "<p>This <del>text~~~~ is ~~~~curious</del>.</p>\n",
        ),
    );
}
