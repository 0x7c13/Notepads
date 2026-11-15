use super::*;

#[test]
fn header_id_prefix() {
    html_opts_i(
        concat!(
            "# Hi.\n",
            "## Hi 1.\n",
            "### Hi.\n",
            "#### Hello.\n",
            "##### Hi.\n",
            "###### Hello.\n",
            "# Isn't it grand?"
        ),
        concat!(
            "<h1 id=\"user-content-hi\">Hi.<a href=\"#hi\" aria-label=\"Link to heading 'Hi.'\" data-heading-content=\"Hi.\" class=\"anchor\"></a></h1>\n",
            "<h2 id=\"user-content-hi-1\">Hi 1.<a href=\"#hi-1\" aria-label=\"Link to heading 'Hi 1.'\" data-heading-content=\"Hi 1.\" class=\"anchor\"></a></h2>\n",
            "<h3 id=\"user-content-hi-2\">Hi.<a href=\"#hi-2\" aria-label=\"Link to heading 'Hi.'\" data-heading-content=\"Hi.\" class=\"anchor\"></a></h3>\n",
            "<h4 id=\"user-content-hello\">Hello.<a href=\"#hello\" aria-label=\"Link to heading 'Hello.'\" data-heading-content=\"Hello.\" class=\"anchor\"></a></h4>\n",
            "<h5 id=\"user-content-hi-3\">Hi.<a href=\"#hi-3\" aria-label=\"Link to heading 'Hi.'\" data-heading-content=\"Hi.\" class=\"anchor\"></a></h5>\n",
            "<h6 id=\"user-content-hello-1\">Hello.<a href=\"#hello-1\" aria-label=\"Link to heading 'Hello.'\" data-heading-content=\"Hello.\" class=\"anchor\"></a></h6>\n",
            "<h1 id=\"user-content-isnt-it-grand\">Isn't it grand?<a href=\"#isnt-it-grand\" aria-label=\"Link to heading 'Isn't it grand?'\" data-heading-content=\"Isn't it grand?\" class=\"anchor\"></a></h1>\n"
        ),
        true,
        |opts| opts.extension.header_id_prefix = Some("user-content-".to_owned()),
    );
}

#[test]
fn header_id_prefix_duplicate_with_literal_suffix() {
    html_opts_i(
        concat!("# a\n", "# a\n", "# a-1\n", "# a\n"),
        concat!(
            "<h1 id=\"user-content-a\">a<a href=\"#a\" aria-label=\"Link to heading 'a'\" data-heading-content=\"a\" class=\"anchor\"></a></h1>\n",
            "<h1 id=\"user-content-a-1\">a<a href=\"#a-1\" aria-label=\"Link to heading 'a'\" data-heading-content=\"a\" class=\"anchor\"></a></h1>\n",
            "<h1 id=\"user-content-a-1-1\">a-1<a href=\"#a-1-1\" aria-label=\"Link to heading 'a-1'\" data-heading-content=\"a-1\" class=\"anchor\"></a></h1>\n",
            "<h1 id=\"user-content-a-2\">a<a href=\"#a-2\" aria-label=\"Link to heading 'a'\" data-heading-content=\"a\" class=\"anchor\"></a></h1>\n"
        ),
        true,
        |opts| opts.extension.header_id_prefix = Some("user-content-".to_owned()),
    );
}

#[test]
fn header_id_prefix_deep_nesting() {
    let input = "# ".to_string() + &"*a _".repeat(60_000) + "x" + &"_ b*".repeat(60_000);

    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || {
            let mut options = Options::default();
            options.extension.header_id_prefix = Some("user-content-".to_owned());
            let arena = Arena::new();
            let root = parse_document(&arena, &input, &options);
            let mut output = String::new();
            html::format_document(root, &options, &mut output).unwrap();
            assert!(output.contains("<em>"));
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn header_ids_prefix_in_href() {
    html_opts_i(
        concat!(
            "# Hi.\n",
            "## Hi 1.\n",
            "### Hi.\n",
            "#### Hello.\n",
            "##### Hi.\n",
            "###### Hello.\n",
            "# Isn't it grand?"
        ),
        concat!(
            "<h1 id=\"user-content-hi\">Hi.<a href=\"#user-content-hi\" aria-label=\"Link to heading 'Hi.'\" data-heading-content=\"Hi.\" class=\"anchor\"></a></h1>\n",
            "<h2 id=\"user-content-hi-1\">Hi 1.<a href=\"#user-content-hi-1\" aria-label=\"Link to heading 'Hi 1.'\" data-heading-content=\"Hi 1.\" class=\"anchor\"></a></h2>\n",
            "<h3 id=\"user-content-hi-2\">Hi.<a href=\"#user-content-hi-2\" aria-label=\"Link to heading 'Hi.'\" data-heading-content=\"Hi.\" class=\"anchor\"></a></h3>\n",
            "<h4 id=\"user-content-hello\">Hello.<a href=\"#user-content-hello\" aria-label=\"Link to heading 'Hello.'\" data-heading-content=\"Hello.\" class=\"anchor\"></a></h4>\n",
            "<h5 id=\"user-content-hi-3\">Hi.<a href=\"#user-content-hi-3\" aria-label=\"Link to heading 'Hi.'\" data-heading-content=\"Hi.\" class=\"anchor\"></a></h5>\n",
            "<h6 id=\"user-content-hello-1\">Hello.<a href=\"#user-content-hello-1\" aria-label=\"Link to heading 'Hello.'\" data-heading-content=\"Hello.\" class=\"anchor\"></a></h6>\n",
            "<h1 id=\"user-content-isnt-it-grand\">Isn't it grand?<a href=\"#user-content-isnt-it-grand\" aria-label=\"Link to heading 'Isn't it grand?'\" data-heading-content=\"Isn't it grand?\" class=\"anchor\"></a></h1>\n"
        ),
        true,
        |opts| {
            opts.extension.header_id_prefix = Some("user-content-".to_owned());
            opts.extension.header_id_prefix_in_href = true;
        },
    );
}

#[test]
fn header_id_prefix_in_href_without_prefix() {
    // When header_id_prefix is None, header_id_prefix_in_href has no effect
    html_opts_i("# Hi.\n", "<h1>Hi.</h1>\n", true, |opts| {
        opts.extension.header_id_prefix_in_href = true;
    });
}
