use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\@char"],
        "textord",
        1,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_at_char,
    );
}

fn handle_at_char(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let arg = args.into_iter().next().unwrap();
    let body = ctx.parser.ord_argument(arg)?;

    let mut number_str = String::new();
    for node in &body {
        if let Some(text) = node.symbol_text() {
            ctx.parser.append_text(&mut number_str, text)?;
        }
    }

    let code: u32 = number_str
        .parse()
        .map_err(|_| ParseError::msg("Invalid builtin scalar number"))?;
    let scalar = char::from_u32(code).ok_or_else(|| ParseError::msg("Invalid builtin scalar"))?;
    let mut buffer = [0u8; 4];
    let text = ctx.parser.text(scalar.encode_utf8(&mut buffer))?;

    // Match KaTeX `src/functions/char.ts`: `\@char` always yields `textord`, even when the
    // codepoint is also listed as `\\sum` / op-token in `symbols.js`. `\char"2211` must not
    // become a large `Size1` operator (that glyph is much wider than KaTeX’s textord path).
    Ok(ParseNode::TextOrd {
        mode: ctx.parser.mode,
        text,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\@char"],
    FunctionSpec {
        node_type: "textord",
        num_args: 1,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_at_char,
    },
)];
