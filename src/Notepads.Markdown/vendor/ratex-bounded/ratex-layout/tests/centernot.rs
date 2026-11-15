use ratex_layout::{layout, to_display_list, LayoutOptions};
use ratex_parser::parser::parse;
use ratex_types::display_item::DisplayItem;

fn render(source: &str) -> ratex_layout::LayoutBox {
    layout(&parse(source).unwrap(), &LayoutOptions::default())
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn centernot_accepts_braced_and_token_arguments() {
    for relation in [r"\implies", r"\impliedby", r"\iff", "=", r"\rightarrow"] {
        let braced = render(&format!(r"a\centernot{{{relation}}}b"));
        let token = render(&format!(r"a\centernot{relation} b"));
        assert_eq!(
            serde_json::to_value(to_display_list(&braced)).unwrap(),
            serde_json::to_value(to_display_list(&token)).unwrap()
        );
    }
    assert!(parse(r"\centernot").is_err());
    assert!(parse(r"\text{\centernot=}").is_err());
}

#[test]
fn centernot_centers_over_the_entire_argument_without_changing_advance() {
    let equals = render("=");
    let slash = render(r"\not");
    for body in [
        "=",
        r"\implies",
        r"\impliedby",
        r"\iff",
        r"\xrightarrow{long label}",
        r"\rightarrow\rightarrow",
        r"\mid",
        r"\frac{a}{b}",
        "",
    ] {
        let plain = render(body);
        let negated = render(&format!(r"\centernot{{{body}}}"));
        close(negated.width, plain.width);
        close(negated.height, plain.height.max(slash.height));
        close(negated.depth, plain.depth.max(slash.depth));
        let display = to_display_list(&negated);
        let DisplayItem::GlyphPath {
            x, y, char_code, ..
        } = &display.items[0]
        else {
            panic!("expected the negation glyph for {body}");
        };
        assert_eq!(*char_code, 0xE020);
        // Display-list bounds normalize left overflow only for zero-width roots.
        let expected_x = (plain.width - equals.width) / 2.0;
        close(*x, if plain.width < 0.01 { 0.0 } else { expected_x });
        close(*y, negated.height);
        // The body is painted once, rather than measuring by adding a visible copy.
        assert_eq!(display.items.len(), to_display_list(&plain).items.len() + 1);
    }
}

#[test]
fn centernot_keeps_relation_spacing_and_scripts() {
    for body in ["=", r"\implies", r"\iff", r"\mid", "ab"] {
        for style in ["", r"\textstyle", r"\scriptstyle", r"\scriptscriptstyle"] {
            for suffix in ["", "_i", "^2"] {
                let actual = render(&format!(r"{style} a\centernot{{{body}}}{suffix}b"));
                let expected = render(&format!(r"{style} a\mathrel{{{body}}}{suffix}b"));
                close(actual.width, expected.width);
            }
        }
    }
    // The long arrow's slash must move right compared with the left-aligned \not.
    let centered = to_display_list(&render(r"\centernot\implies"));
    let not = to_display_list(&render(r"\not\implies"));
    assert!(matches!((&centered.items[0], &not.items[0]),
        (DisplayItem::GlyphPath { x: centered, .. }, DisplayItem::GlyphPath { x: left, .. })
        if centered > left));
}

#[test]
fn centernot_scales_the_overlay_in_script_styles() {
    let normal = render(r"\centernot{\Longrightarrow}");
    for (style, scale) in [(r"\scriptstyle", 0.7), (r"\scriptscriptstyle", 0.5)] {
        let small = render(&format!(r"{style}\centernot{{\Longrightarrow}}"));
        close(small.width, normal.width * scale);
        close(small.height, normal.height * scale);
        close(small.depth, normal.depth * scale);
        let normal_display = to_display_list(&normal);
        let small_display = to_display_list(&small);
        let DisplayItem::GlyphPath { x: normal_x, .. } = normal_display.items[0] else {
            panic!("expected glyph");
        };
        let DisplayItem::GlyphPath {
            x, scale: actual, ..
        } = small_display.items[0]
        else {
            panic!("expected glyph");
        };
        close(x, normal_x * scale);
        close(actual, scale);
    }
}

#[test]
fn centernot_preserves_body_fonts_and_overlay_color() {
    for (source, plain) in [
        (r"\mathbf{\centernot{A}}", r"\mathbf{A}"),
        (r"\centernot{\mathbf{A}}", r"\mathbf{A}"),
        (r"\mathit{\centernot{ab}}", r"\mathit{ab}"),
        (r"\boldsymbol{\centernot{=}}", r"\boldsymbol{=}"),
    ] {
        let negated = render(source);
        let body = render(plain);
        close(negated.width, body.width);
        let negated = to_display_list(&negated);
        let plain = to_display_list(&body);
        let DisplayItem::GlyphPath {
            font, char_code, ..
        } = &negated.items[0]
        else {
            panic!("expected overlay glyph");
        };
        assert_eq!(font, "Main-Regular");
        assert_eq!(*char_code, 0xE020);
        for (actual, expected) in negated.items[1..].iter().zip(&plain.items) {
            match (actual, expected) {
                (
                    DisplayItem::GlyphPath {
                        font: a,
                        char_code: ac,
                        x: ax,
                        ..
                    },
                    DisplayItem::GlyphPath {
                        font: b,
                        char_code: bc,
                        x: bx,
                        ..
                    },
                ) => {
                    assert_eq!((a, ac), (b, bc));
                    close(*ax, *bx);
                }
                _ => panic!("expected body glyphs"),
            }
        }
    }
    let colored_body = to_display_list(&render(r"\centernot{\textcolor{blue}{=}}"));
    assert!(
        matches!(&colored_body.items[0], DisplayItem::GlyphPath { color, .. }
        if color.r == 0.0 && color.g == 0.0 && color.b == 0.0)
    );
    assert!(
        matches!(&colored_body.items[1], DisplayItem::GlyphPath { color, .. }
        if color.r == 0.0 && color.g == 0.0 && color.b == 1.0)
    );
    let colored = to_display_list(&render(r"\textcolor{red}{\centernot{=}}"));
    assert!(colored.items.iter().all(|item| matches!(item,
        DisplayItem::GlyphPath { color, .. } if color.r == 1.0 && color.g == 0.0 && color.b == 0.0)));
}
