use ratex_font::FontId;

/// Map surd `inner_height` from `layout_radical` (1.0 … 3.0) to the KaTeX font for U+221A.
pub fn surd_font_for_inner_height(h: f64) -> FontId {
    // Matches `select_surd_height` in engine.rs: 1.0, 1.2, 1.8, 2.4, 3.0+
    if h < 1.1 {
        FontId::MainRegular
    } else if h < 1.5 {
        FontId::Size1Regular
    } else if h < 2.1 {
        FontId::Size2Regular
    } else if h < 2.7 {
        FontId::Size3Regular
    } else {
        FontId::Size4Regular
    }
}
