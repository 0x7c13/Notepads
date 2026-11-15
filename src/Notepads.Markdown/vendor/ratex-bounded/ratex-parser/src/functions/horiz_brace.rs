use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\overbrace",
            "\\underbrace",
            "\\overbracket",
            "\\underbracket",
        ],
        "horizBrace",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_horiz_brace,
    );
}

fn handle_horiz_brace(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let base = args.into_iter().next().unwrap();
    let is_over = ctx.func_name.starts_with("\\over");

    Ok(ParseNode::HorizBrace {
        mode: ctx.parser.mode,
        label: std::mem::take(&mut ctx.func_name),
        is_over,
        base: ctx.parser.boxed(base)?,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &[
        "\\overbrace",
        "\\underbrace",
        "\\overbracket",
        "\\underbracket",
    ],
    FunctionSpec {
        node_type: "horizBrace",
        num_args: 1,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_horiz_brace,
    },
)];
