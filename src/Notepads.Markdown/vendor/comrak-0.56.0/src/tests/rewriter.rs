use std::sync::Arc;

use super::*;

#[test]
fn image_url_rewriter() {
    html_opts_i(
        "![](http://unsafe.example.com/bad.png)",
        "<p><img src=\"https://safe.example.com?url=http://unsafe.example.com/bad.png\" alt=\"\" /></p>\n",
        true,
        |opts| {
            opts.extension.image_url_rewriter = Some(Arc::new(|url: &str| {
                format!("{}{}", "https://safe.example.com?url=", url)
            }))
        },
    );
}

#[test]
fn link_url_rewriter() {
    html_opts_i(
        "[my link](http://unsafe.example.com/bad)",
        "<p><a href=\"https://safe.example.com/norefer?url=http://unsafe.example.com/bad\">my link</a></p>\n",
        true,
        |opts| {
            opts.extension.link_url_rewriter = Some(Arc::new(|url: &str| {
                format!("{}{}", "https://safe.example.com/norefer?url=", url)
            }))
        },
    );
}

#[test]
fn can_borrow_from_context() {
    let rewrite_to = "http://example.org";

    html_opts_i(
        "[my link](http://example.com/)",
        "<p><a href=\"http://example.org\">my link</a></p>\n",
        true,
        |opts| opts.extension.link_url_rewriter = Some(Arc::new(|_: &str| rewrite_to.to_owned())),
    );
}
