// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
mod data;
pub mod font_id;
pub mod math_alpha;
pub mod metrics;
pub mod symbols;

pub use font_id::FontId;
pub use math_alpha::{font_and_metric_for_mathematical_alphanumeric, katex_ttf_glyph_char};
pub use metrics::{
    CharMetrics, MATH_CONSTANTS_BY_SIZE, MathConstants, get_char_metrics,
    get_char_metrics_for_mode, get_char_metrics_with_fallback, get_global_metrics,
};
pub use symbols::{
    Group, Mode, SymbolFont, SymbolInfo, SymbolRenderSpec, get_math_symbol, get_symbol,
    get_symbol_render_spec, get_text_symbol,
};

/// Conservative, finite readonly payload charged as fixed assets by the bounded adapter.
/// Counts aliases and repeated spelling separately; compiled code/pages are outside this value.
pub const fn bounded_readonly_bytes() -> usize {
    use data::metrics_data::*;
    let tables = [
        AMS_REGULAR,
        CALIGRAPHIC_REGULAR,
        FRAKTUR_REGULAR,
        FRAKTUR_BOLD,
        MAIN_BOLD,
        MAIN_BOLDITALIC,
        MAIN_ITALIC,
        MAIN_REGULAR,
        MATH_BOLDITALIC,
        MATH_ITALIC,
        SANSSERIF_BOLD,
        SANSSERIF_ITALIC,
        SANSSERIF_REGULAR,
        SCRIPT_REGULAR,
        SIZE1_REGULAR,
        SIZE2_REGULAR,
        SIZE3_REGULAR,
        SIZE4_REGULAR,
        TYPEWRITER_REGULAR,
    ];
    let mut sum = size_of_val(&tables)
        + size_of_val(data::symbols_data::SYMBOLS)
        + symbols::bounded_index_bytes()
        + size_of_val(&metrics::MATH_CONSTANTS_BY_SIZE)
        + 4096;
    assert!(data::symbols_data::SYMBOLS.len() <= 4096);
    let mut index = 0;
    while index < tables.len() {
        sum += size_of_val(tables[index]);
        index += 1;
    }
    index = 0;
    while index < data::symbols_data::SYMBOLS.len() {
        let row = &data::symbols_data::SYMBOLS[index];
        sum += row.0.len() + row.3.len();
        index += 1;
    }
    sum
}
const _: () = assert!(bounded_readonly_bytes() <= 2 * 1024 * 1024);
