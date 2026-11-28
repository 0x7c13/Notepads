use std::collections::HashMap;

use crate::functions::FunctionSpec;

pub fn register(_map: &mut HashMap<&'static str, FunctionSpec>) {
    // \verb is handled directly in Parser::parse_symbol_inner
    // because it has special lexing requirements.
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[];
