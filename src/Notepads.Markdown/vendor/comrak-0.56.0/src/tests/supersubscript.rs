use super::*;

#[test]
fn superscript() {
    html_opts!(
        [extension.superscript],
        "e = mc^2^.\n",
        "<p>e = mc<sup>2</sup>.</p>\n",
    );
}

#[test]
fn superscript_negative_exponents_are_real() {
    html_opts!(
        [extension.superscript],
        "i^-2^ = -1.\n",
        "<p>i<sup>-2</sup> = -1.</p>\n",
    );
}

#[test]
fn subscript() {
    html_opts!(
        [extension.subscript],
        "f~i~ = f~i - 2~ + f~i - 1~.\n",
        "<p>f<sub>i</sub> = f<sub>i - 2</sub> + f<sub>i - 1</sub>.</p>\n",
    );
}

#[test]
fn subscript_negative_terms_are_real() {
    html_opts!(
        [extension.subscript],
        "f~i~ = f~-2 + i~ + f~-1 + i~.\n",
        "<p>f<sub>i</sub> = f<sub>-2 + i</sub> + f<sub>-1 + i</sub>.</p>\n",
    );
}
