// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
mod bounded_clone;
pub mod environments;
pub mod error;
pub(crate) mod expansion_budget;
pub mod functions;
pub mod macro_expander;
pub mod mhchem;
pub mod parse_node;
pub mod parser;
pub(crate) mod stack_safety;
pub mod unicode_sup_sub;

#[cfg(test)]
mod tests;

pub use error::{ParseError, ParseResult};
pub use parse_node::{Mode, ParseNode};
pub use parser::{Parser, parse};

pub use parser::parse_with_storage;
pub use parser::{ControlledParseError, parse_with_control};

/// Fixed registry/name/template storage; includes duplicate aliases conservatively.
pub const fn bounded_readonly_bytes() -> usize {
    functions::bounded_readonly_bytes() + macro_expander::bounded_readonly_bytes()
}
