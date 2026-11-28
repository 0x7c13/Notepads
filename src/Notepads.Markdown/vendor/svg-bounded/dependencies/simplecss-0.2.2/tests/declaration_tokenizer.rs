// Copyright 2019 the SimpleCSS Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

//! Declaration Tokenizer

use simplecss::*;

macro_rules! tokenize {
    ($name:ident, $text:expr, $( $token:expr ),*) => (
        #[test]
        fn $name() {
            let mut t = DeclarationTokenizer::from($text);
            $(
                assert_eq!(t.next().unwrap(), $token);
            )*

            assert!(t.next().is_none());
        }
    )
}

fn declare<'a>(name: &'a str, value: &'a str) -> Declaration<'a> {
    Declaration {
        name,
        value,
        important: false,
    }
}

fn declare_important<'a>(name: &'a str, value: &'a str) -> Declaration<'a> {
    Declaration {
        name,
        value,
        important: true,
    }
}

tokenize!(tokenize_01, "",);

tokenize!(tokenize_02, " ",);

tokenize!(tokenize_03, "/**/",);

tokenize!(tokenize_04, "color:red", declare("color", "red"));

tokenize!(tokenize_05, "color:red;", declare("color", "red"));

tokenize!(tokenize_06, "color:red ", declare("color", "red"));

tokenize!(tokenize_07, " color: red; ", declare("color", "red"));

tokenize!(tokenize_08, "  color  :  red  ; ", declare("color", "red"));

tokenize!(
    tokenize_09,
    "  color:red;;;;color:red; ",
    declare("color", "red"),
    declare("color", "red")
);

tokenize!(
    tokenize_10,
    "background: url(\"img.png\");",
    declare("background", "url(\"img.png\")")
);

tokenize!(
    tokenize_11,
    "background: url(\"{}\");",
    declare("background", "url(\"{}\")")
);

tokenize!(
    tokenize_12,
    "color: red ! important",
    declare_important("color", "red")
);

tokenize!(
    tokenize_13,
    "color: red !important",
    declare_important("color", "red")
);

tokenize!(
    tokenize_14,
    "color: red!important",
    declare_important("color", "red")
);

tokenize!(
    tokenize_15,
    "color: red !/**/important",
    declare_important("color", "red")
);

tokenize!(
    tokenize_16,
    "border: 1em solid blue",
    declare("border", "1em solid blue")
);

tokenize!(
    tokenize_17,
    "background: navy url(support/diamond.png) -2em -2em no-repeat",
    declare(
        "background",
        "navy url(support/diamond.png) -2em -2em no-repeat"
    )
);

tokenize!(tokenize_18, "/**/color:red", declare("color", "red"));

tokenize!(tokenize_19, "/* *\\/*/color: red;", declare("color", "red"));

tokenize!(
    tokenize_20,
    "/**/color/**/:/**/red/**/;/**/",
    declare("color", "red")
);

tokenize!(tokenize_21, "\ncolor\n:\nred\n;\n", declare("color", "red"));

tokenize!(tokenize_22, "{color:red}",);

tokenize!(tokenize_23, "(color:red)",);

tokenize!(tokenize_24, "[color:red]",);

tokenize!(tokenize_25, "color:",);

tokenize!(tokenize_26, "value:\"text\"", declare("value", "\"text\""));

tokenize!(tokenize_27, "value:'text'", declare("value", "'text'"));

tokenize!(tokenize_28, "color:#fff", declare("color", "#fff"));
tokenize!(tokenize_29, "color:0.5", declare("color", "0.5"));

tokenize!(tokenize_30, "color:.5", declare("color", ".5"));

tokenize!(tokenize_31, "color:#FFF", declare("color", "#FFF"));

tokenize!(
    tokenize_32,
    "content: counter(chapno, upper-roman) \". \"",
    declare("content", "counter(chapno, upper-roman) \". \"")
);

tokenize!(
    tokenize_33,
    "font-family:'Noto Serif','DejaVu Serif',serif",
    declare("font-family", "'Noto Serif','DejaVu Serif',serif")
);

tokenize!(tokenize_34, "*zoom:1;", declare("zoom", "1"));

fn font_extent_check(source: &str, expected: &[Declaration<'_>]) {
    let actual: Vec<_> = DeclarationTokenizer::from(source).collect();
    assert_eq!(actual, expected);
    for declaration in actual {
        let offset = declaration.value.as_ptr() as usize - source.as_ptr() as usize;
        assert!(offset + declaration.value.len() <= source.len());
        assert_eq!(
            &source[offset..offset + declaration.value.len()],
            declaration.value
        );
    }
}

#[test]
fn font_extent_family_and_shorthand_preserve_escaped_raw_lists() {
    font_extent_check(
        r#"font-family:"\53 ERIF", \73 erif, ACME\ Sans;fill:red"#,
        &[
            declare("font-family", r#""\53 ERIF", \73 erif, ACME\ Sans"#),
            declare("fill", "red"),
        ],
    );
    font_extent_check(
        r"font:italic 12px ACME\,Inc, serif!important;fill:blue",
        &[
            declare_important("font", r"italic 12px ACME\,Inc, serif"),
            declare("fill", "blue"),
        ],
    );
}

#[test]
fn font_extent_protects_quoted_commented_and_escaped_delimiters() {
    let value = r#""A; } !", ACME\;Inc, A\}B, A\!B, Noto/* ; } !important */Sans"#;
    let source = format!("font-family:{value};fill:red");
    font_extent_check(
        &source,
        &[declare("font-family", value), declare("fill", "red")],
    );
}

#[test]
fn font_extent_quote_parity_crlf_and_unicode_keep_original_bytes() {
    let value = r#""A\\", "A\\\"B", 'C\\', 'C\\\'D'"#;
    let source = format!("font-family:{value};fill:red");
    font_extent_check(
        &source,
        &[declare("font-family", value), declare("fill", "red")],
    );
    let value = "\"A\\\r\nB\", \\4E2D \\6587, \u{80}Face, \u{a0}Face, 𐐀Name\u{2003}";
    let source = format!("font-family:{value}  ;fill:red");
    font_extent_check(
        &source,
        &[declare("font-family", value), declare("fill", "red")],
    );
    font_extent_check(
        "font-family:\\000041  ;fill:red",
        &[declare("font-family", "\\000041 "), declare("fill", "red")],
    );
}

#[test]
fn font_extent_unclosed_string_and_comment_retain_complete_tail() {
    for value in [
        "First, \"unfinished; } !important;fill:red",
        "First, 'dangling\\",
        "First/* unfinished; } !important;fill:red  ",
        "First, \"bad\nstring; } !important;fill:red",
    ] {
        let source = format!("font-family:{value}");
        font_extent_check(&source, &[declare("font-family", value)]);
    }
}

#[test]
fn font_extent_important_suffix_keeps_legacy_recognition() {
    font_extent_check(
        r#"font-family:A\!important, "B!important"/* !important */ !/**/important;fill:red"#,
        &[
            declare_important(
                "font-family",
                r#"A\!important, "B!important"/* !important */"#,
            ),
            declare("fill", "red"),
        ],
    );
    // These preserve the existing suffix behavior; this scope does not add
    // case-insensitive recognition or repair the legacy invalid-suffix rules.
    font_extent_check(
        "font-family:Arial !IMPORTANT;fill:red",
        &[declare("font-family", "Arial")],
    );
    font_extent_check(
        "font-family:Arial!importantTrailing;fill:red",
        &[declare_important("font-family", "Arial")],
    );
}

#[test]
fn font_extent_dispatch_keeps_other_property_term_scanning() {
    font_extent_check(
        "fill:red;opacity:.5!important;stroke:blue",
        &[
            declare("fill", "red"),
            declare_important("opacity", ".5"),
            declare("stroke", "blue"),
        ],
    );
    font_extent_check(r"fill:ACME\,Inc;stroke:blue", &[declare("fill", "ACME")]);
}

//tokenize!(tokenize_, "@unsupported { splines: reticulating } color: green",
//    declare("color", "green")
//);

//tokenize!(tokenize_, "/*\\*/*/color: red;", declare("color", "red"));

//tokenize!(tokenize_, "\"this is a string]}\"\"[{\\\"'\";  /*should be parsed as a string but be ignored*/
//    {{}}[]'';                     /*should be parsed as nested blocks and a string but be ignored*/
//    color: red;", declare("color", "red"));
