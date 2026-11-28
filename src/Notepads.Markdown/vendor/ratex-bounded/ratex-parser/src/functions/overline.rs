use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\overline"],
        "overline",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_overline,
    );

    define_function_full(
        map,
        &["\\underline"],
        "underline",
        1,
        0,
        None,
        false,
        true, // allowed_in_text
        true,
        false,
        false,
        handle_underline,
    );
}

fn handle_overline(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Overline {
        mode: ctx.parser.mode,
        body: ctx.parser.boxed(args.into_iter().next().unwrap())?,
        loc: None,
    })
}

fn handle_underline(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Underline {
        mode: ctx.parser.mode,
        body: ctx.parser.boxed(args.into_iter().next().unwrap())?,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\overline"],
        FunctionSpec {
            node_type: "overline",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_overline,
        },
    ),
    (
        &["\\underline"],
        FunctionSpec {
            node_type: "underline",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_underline,
        },
    ),
];
