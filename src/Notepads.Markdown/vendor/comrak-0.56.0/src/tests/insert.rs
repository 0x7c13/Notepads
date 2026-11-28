use super::*;

#[test]
fn insert() {
    html_opts!(
        [extension.insert],
        concat!(
            "This is ++important!++.\n",
            "\n",
            "This is ++very important++ OK?\n",
            "\n",
            "++It is a \n shrubbery++\n",
            "\n",
            "Vendo ++Opel Corsa **em bom estado**++\n",
            "\n",
            "Ceci n'est pas +important+\n"
        ),
        concat!(
            "<p>This is <ins>important!</ins>.</p>\n",
            "<p>This is <ins>very important</ins> OK?</p>\n",
            "<p><ins>It is a\nshrubbery</ins></p>\n",
            "<p>Vendo <ins>Opel Corsa <strong>em bom estado</strong></ins></p>\n",
            "<p>Ceci n'est pas +important+</p>\n"
        ),
    );
}
