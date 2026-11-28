use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\sqrt"],
        "sqrt",
        1,
        1, // one optional arg [n]
        None,
        false,
        false,
        true,
        false,
        false,
        handle_sqrt,
    );
}

fn handle_sqrt(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let index = opt_args.into_iter().next().flatten();
    let body = args.into_iter().next().unwrap();

    Ok(ParseNode::Sqrt {
        mode: ctx.parser.mode,
        body: ctx.parser.boxed(body)?,
        index: index.map(|value| ctx.parser.boxed(value)).transpose()?,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\sqrt"],
    FunctionSpec {
        node_type: "sqrt",
        num_args: 1,
        num_optional_args: 1,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_sqrt,
    },
)];
