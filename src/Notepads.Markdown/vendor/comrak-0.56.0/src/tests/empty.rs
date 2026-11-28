use super::*;

#[test]
fn no_empty_link() {
    html_opts!(
        [render.ignore_empty_links],
        "[](https://example.com/evil-link-for-seo-spam)",
        "<p>[](https://example.com/evil-link-for-seo-spam)</p>\n",
    );

    html_opts!(
        [render.ignore_empty_links],
        "[    ](https://example.com/evil-link-for-seo-spam)",
        "<p>[    ](https://example.com/evil-link-for-seo-spam)</p>\n",
    );
}

#[test]
fn empty_image_allowed() {
    html_opts!(
        [render.ignore_empty_links],
        "![   ](https://example.com/evil-link-for-seo-spam)",
        "<p><img src=\"https://example.com/evil-link-for-seo-spam\" alt=\"   \" /></p>\n",
    );
}

#[test]
fn image_inside_link_allowed() {
    html_opts!(
        [render.ignore_empty_links],
        "[![](https://example.com/image.png)](https://example.com/)",
        "<p><a href=\"https://example.com/\"><img src=\"https://example.com/image.png\" alt=\"\" /></a></p>\n",
    );
}
