use super::*;

#[test]
fn subtext() {
    html_opts!(
        [extension.subtext],
        "-# Some Subtext\n",
        "<p><sub>Some Subtext</sub></p>\n",
    );
}
