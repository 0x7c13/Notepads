use ratex_layout::{layout, to_display_list, LayoutOptions};
use ratex_parser::parser::parse;
use ratex_types::display_item::DisplayItem;

#[test]
fn ce_co2_line_emits_cjk_regular_for_simplified_chinese() {
    let expr = r"\ce{CO2 + C -> 2 CO} \quad \text{二氧化碳}";
    let ast = parse(expr).unwrap();
    let lbox = layout(&ast, &LayoutOptions::default());
    let dl = to_display_list(&lbox);
    let mut rows = Vec::new();
    for item in &dl.items {
        if let DisplayItem::GlyphPath {
            font, char_code, ..
        } = item
        {
            if *char_code >= 0x4E00 {
                let ch = char::from_u32(*char_code).unwrap();
                rows.push(format!("{font} U+{:04X} {ch}", char_code));
            }
        }
    }
    for (font, cp, expected) in [
        ("CJK-Regular", 0x4E8C_u32, '二'),
        ("CJK-Regular", 0x6C27_u32, '氧'),
        ("CJK-Regular", 0x5316_u32, '化'),
        ("CJK-Regular", 0x78B3_u32, '碳'),
    ] {
        let line = format!("{font} U+{cp:04X} {expected}");
        assert!(
            rows.iter().any(|r| r == &line),
            "missing {line}, got {:?}",
            rows
        );
    }
}
