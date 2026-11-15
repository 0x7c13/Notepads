// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::{Mode, ParseNode, StyleStr};

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\(", "$"],
        "styling",
        0,
        0,
        None,
        false,
        true,
        false,
        false,
        false,
        handle_math_switch,
    );

    define_function_full(
        map,
        &["\\)", "\\]"],
        "text",
        0,
        0,
        None,
        false,
        true,
        false,
        false,
        false,
        handle_math_close,
    );
}

fn handle_math_switch(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let outer_mode = ctx.parser.mode;
    ctx.parser.switch_mode(Mode::Math);
    let close = if ctx.func_name == "\\(" { "\\)" } else { "$" };
    let body = ctx.parser.parse_expression(false, Some(close))?;
    ctx.parser.expect(close, true)?;
    ctx.parser.switch_mode(outer_mode);

    Ok(ParseNode::Styling {
        mode: ctx.parser.mode,
        style: StyleStr::Text,
        body,
        loc: None,
    })
}

fn handle_math_close(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    if ctx.parser.bounded() {
        // Keep exact known diagnostics without allocating after a controlled failure.
        return Err(ParseError::msg(match ctx.func_name.as_str() {
            "\\)" => "Mismatched \\)",
            "\\]" => "Mismatched \\]",
            _ => "Mismatched math delimiter",
        }));
    }
    Err(ParseError::msg(format!("Mismatched {}", ctx.func_name)))
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\(", "$"],
        FunctionSpec {
            node_type: "styling",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: false,
            infix: false,
            primitive: false,
            handler: handle_math_switch,
        },
    ),
    (
        &["\\)", "\\]"],
        FunctionSpec {
            node_type: "text",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: false,
            infix: false,
            primitive: false,
            handler: handle_math_close,
        },
    ),
];
