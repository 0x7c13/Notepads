// Guard downstream exhaustive matches and struct literals against API breaks.
use ratex_layout::layout_box::BoxContent;
fn inspect(c: &BoxContent) {
    match c {
        BoxContent::HBox(..) => (),
        BoxContent::VBox(..) => (),
        BoxContent::Glyph { .. } => (),
        BoxContent::GlyphRun { .. } => (),
        BoxContent::Rule { .. } => (),
        BoxContent::Kern => (),
        BoxContent::Fraction { .. } => (),
        BoxContent::SupSub { .. } => (),
        BoxContent::Radical { .. } => (),
        BoxContent::OpLimits { .. } => (),
        BoxContent::Accent { .. } => (),
        BoxContent::LeftRight { .. } => (),
        BoxContent::Array { .. } => (),
        BoxContent::SvgPath { .. } => (),
        BoxContent::Framed { .. } => (),
        BoxContent::RaiseBox { .. } => (),
        BoxContent::Scaled { .. } => (),
        BoxContent::Angl { .. } => (),
        BoxContent::Overline { .. } => (),
        BoxContent::Underline { .. } => (),
        BoxContent::ProofTree { .. } => (),
        BoxContent::Empty => (),
    }
}
#[test]
fn public_layout_can_be_constructed_and_exhaustively_matched() {
    let box_ = ratex_layout::LayoutBox {
        width: 0.0,
        height: 0.0,
        depth: 0.0,
        content: BoxContent::Empty,
        color: ratex_types::color::Color::BLACK,
    };
    inspect(&box_.content);
    ratex_layout::validate_layout(&box_).unwrap();
    let _: ratex_types::display_item::DisplayList = ratex_layout::to_display_list(&box_);
}

#[test]
fn ragged_array_and_cd_keep_rectangular_cells_and_display_output() {
    use ratex_layout::{layout, to_display_list, validate_layout, LayoutOptions};
    for (ragged, explicit, expected_rows, expected_cols) in [
        (
            r"\begin{matrix}a&b&c\\d\end{matrix}",
            r"\begin{matrix}a&b&c\\d&{}&{}\end{matrix}",
            2,
            3,
        ),
        (
            r"\begin{array}{l|r|c}a&b&c\\\hline d\end{array}",
            r"\begin{array}{l|r|c}a&b&c\\\hline d&{}&{}\end{array}",
            2,
            3,
        ),
        (
            r"\begin{CD}A@>>>B\\@VVV\\C\end{CD}",
            r"\begin{CD}A@>>>B\\@VVV @.\\C@.{}\end{CD}",
            3,
            3,
        ),
    ] {
        let tree = layout(
            &ratex_parser::parse(ragged).unwrap(),
            &LayoutOptions::default(),
        );
        validate_layout(&tree).unwrap();
        let reference = layout(
            &ratex_parser::parse(explicit).unwrap(),
            &LayoutOptions::default(),
        );
        assert_eq!(
            serde_json::to_value(to_display_list(&tree)).unwrap(),
            serde_json::to_value(to_display_list(&reference)).unwrap(),
            "{ragged}"
        );
        let BoxContent::HBox(ref children) = tree.content else {
            panic!("expected expression")
        };
        let BoxContent::Array {
            cells, col_widths, ..
        } = &children[0].content
        else {
            panic!("expected array: {ragged}")
        };
        assert_eq!(cells.len(), expected_rows);
        assert_eq!(col_widths.len(), expected_cols);
        for row in cells {
            assert_eq!(row.len(), col_widths.len());
        }
        // Legacy callers can directly index a trailing column in a short row.
        let trailing = &cells[expected_rows - 1][expected_cols - 1];
        assert!(matches!(trailing.content, BoxContent::Empty));
        assert_eq!(
            (trailing.width, trailing.height, trailing.depth),
            (0.0, 0.0, 0.0)
        );
    }
}

#[test]
fn v014_options_support_full_literals_and_exhaustive_patterns() {
    use ratex_layout::{layout, LayoutOptions};
    use ratex_types::{color::Color, math_style::MathStyle};

    // Deliberately omit struct update syntax and `..` in the pattern: adding
    // any public or private field (or non_exhaustive) must fail this external test.
    let options = LayoutOptions {
        style: MathStyle::Text,
        color: Color::BLACK,
        align_relation_spacing: None,
        leftright_delim_height: None,
        inter_glyph_kern_em: 0.0,
        explicit_size_multiplier: 1.0,
    };
    let LayoutOptions {
        style,
        color,
        align_relation_spacing,
        leftright_delim_height,
        inter_glyph_kern_em,
        explicit_size_multiplier,
    } = options.clone();
    assert_eq!(style, MathStyle::Text);
    assert_eq!(color, Color::BLACK);
    assert_eq!(align_relation_spacing, None);
    assert_eq!(leftright_delim_height, None);
    assert_eq!(inter_glyph_kern_em, 0.0);
    assert_eq!(explicit_size_multiplier, 1.0);
    let old_entry: fn(&[ratex_parser::ParseNode], &LayoutOptions) -> ratex_layout::LayoutBox =
        layout;
    let tree = old_entry(&ratex_parser::parse("x").unwrap(), &options);
    ratex_layout::validate_layout(&tree).unwrap();
}
