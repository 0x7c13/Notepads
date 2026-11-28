use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\xleftarrow",
            "\\xrightarrow",
            "\\xLeftarrow",
            "\\xRightarrow",
            "\\xleftrightarrow",
            "\\xLeftrightarrow",
            "\\xhookleftarrow",
            "\\xhookrightarrow",
            "\\xmapsto",
            "\\xrightharpoondown",
            "\\xrightharpoonup",
            "\\xleftharpoondown",
            "\\xleftharpoonup",
            "\\xrightleftharpoons",
            "\\xleftrightharpoons",
            "\\xlongequal",
            "\\xtwoheadrightarrow",
            "\\xtwoheadleftarrow",
            "\\xtofrom",
            "\\xrightleftarrows",
            "\\xrightequilibrium",
            "\\xleftequilibrium",
        ],
        "xArrow",
        1,
        1,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_arrow,
    );
}

fn handle_arrow(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = args.into_iter().next().unwrap();
    let below = opt_args.into_iter().next().flatten();

    Ok(ParseNode::XArrow {
        mode: ctx.parser.mode,
        label: ctx.func_name.clone(),
        body: Box::new(body),
        below: below.map(Box::new),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &[
        "\\xleftarrow",
        "\\xrightarrow",
        "\\xLeftarrow",
        "\\xRightarrow",
        "\\xleftrightarrow",
        "\\xLeftrightarrow",
        "\\xhookleftarrow",
        "\\xhookrightarrow",
        "\\xmapsto",
        "\\xrightharpoondown",
        "\\xrightharpoonup",
        "\\xleftharpoondown",
        "\\xleftharpoonup",
        "\\xrightleftharpoons",
        "\\xleftrightharpoons",
        "\\xlongequal",
        "\\xtwoheadrightarrow",
        "\\xtwoheadleftarrow",
        "\\xtofrom",
        "\\xrightleftarrows",
        "\\xrightequilibrium",
        "\\xleftequilibrium",
    ],
    FunctionSpec {
        node_type: "xArrow",
        num_args: 1,
        num_optional_args: 1,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_arrow,
    },
)];
