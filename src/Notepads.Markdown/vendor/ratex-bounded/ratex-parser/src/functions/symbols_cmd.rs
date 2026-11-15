use std::collections::HashMap;

use crate::functions::FunctionSpec;

pub fn register(_map: &mut HashMap<&'static str, FunctionSpec>) {
    // Symbol commands like \alpha, \beta, \infty, etc. are NOT registered as functions.
    // They are handled by Parser::parse_symbol_inner via the ratex-font symbol table lookup.
    // This matches KaTeX's behavior where symbols.js defines these characters
    // and Parser.parseSymbol() resolves them.
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[];
