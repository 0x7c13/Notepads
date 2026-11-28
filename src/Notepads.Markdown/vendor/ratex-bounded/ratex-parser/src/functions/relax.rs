use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\relax"],
        "internal",
        0,
        0,
        None,
        true, // allowed_in_argument
        true, // allowed_in_text
        true,
        false,
        false,
        handle_relax,
    );
}

fn handle_relax(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Internal {
        mode: ctx.parser.mode,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\relax"],
    FunctionSpec {
        node_type: "internal",
        num_args: 0,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: true,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_relax,
    },
)];
