use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\operatorname@", "\\operatornamewithlimits"],
        "operatorname",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_operatorname,
    );
}

fn handle_operatorname(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = ctx.parser.ord_argument(args.into_iter().next().unwrap())?;

    let always_handle_sup_sub = ctx.func_name == "\\operatornamewithlimits";

    Ok(ParseNode::OperatorName {
        mode: ctx.parser.mode,
        body,
        always_handle_sup_sub,
        limits: false,
        parent_is_sup_sub: false,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\operatorname@", "\\operatornamewithlimits"],
    FunctionSpec {
        node_type: "operatorname",
        num_args: 1,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_operatorname,
    },
)];
