use super::*;

#[test]
fn highlight() {
    html_opts!(
        [extension.highlight],
        concat!(
            "This is ==important!==.\n",
            "\n",
            "This is ==very important== OK?\n",
            "\n",
            "==It is a \n shrubbery==\n",
            "\n",
            "Vendo ==Opel Corsa **em bom estado**==\n",
            "\n",
            "Ceci n'est pas =important=\n"
        ),
        concat!(
            "<p>This is <mark>important!</mark>.</p>\n",
            "<p>This is <mark>very important</mark> OK?</p>\n",
            "<p><mark>It is a\nshrubbery</mark></p>\n",
            "<p>Vendo <mark>Opel Corsa <strong>em bom estado</strong></mark></p>\n",
            "<p>Ceci n'est pas =important=</p>\n"
        ),
    );
}
