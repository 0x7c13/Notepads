use super::*;

#[test]
fn underline() {
    html_opts!(
        [extension.underline],
        "__underlined text__\n",
        "<p><u>underlined text</u></p>\n",
    );
}

#[test]
fn underline_sourcepos() {
    assert_ast_match!(
        [extension.underline],
        "__this__\n",
        (document (1:1-1:8) [
            (paragraph (1:1-1:8) [
                (underline (1:1-1:8) [
                    (text (1:3-1:6) "this")
                ])
            ])
        ])
    );
}
#[test]
fn underline_sourcepos_emphasis() {
    assert_ast_match!(
        [extension.underline],
        "___this___\n",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (emph (1:1-1:10) [
                    (underline (1:2-1:9) [
                        (text (1:4-1:7) "this")
                    ])
                ])
            ])
        ])
    );
}
