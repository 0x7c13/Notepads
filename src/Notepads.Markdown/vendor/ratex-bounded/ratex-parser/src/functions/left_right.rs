use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\left"],
        "leftright",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        true, // primitive
        handle_left,
    );

    define_function_full(
        map,
        &["\\right"],
        "leftright-right",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        true, // primitive
        handle_right,
    );

    define_function_full(
        map,
        &["\\middle"],
        "middle",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        true,
        handle_middle,
    );
}

fn handle_left(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let delim = get_delim_text(&args[0], ctx.parser)?;

    ctx.parser.leftright_depth += 1;
    let body = ctx.parser.parse_expression(false, None);
    ctx.parser.leftright_depth -= 1;
    let body = body?;

    // Expect \right, but don't consume it yet
    ctx.parser.expect("\\right", false)?;
    // Parse \right as a function, which returns a leftright-right node
    let right_node = ctx.parser.parse_function(None, None)?;

    let (right, right_color) = match right_node {
        Some(ParseNode::LeftRightRight { delim, color, .. }) => (delim, color),
        _ => {
            return Err(ParseError::msg("Expected \\right after \\left"));
        }
    };

    Ok(ParseNode::LeftRight {
        mode: ctx.parser.mode,
        body,
        left: delim,
        right,
        right_color,
        loc: None,
    })
}

fn handle_right(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let delim = get_delim_text(&args[0], ctx.parser)?;

    Ok(ParseNode::LeftRightRight {
        mode: ctx.parser.mode,
        delim,
        color: None,
        loc: None,
    })
}

fn handle_middle(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let delim = get_delim_text(&args[0], ctx.parser)?;

    if ctx.parser.leftright_depth <= 0 {
        return Err(ParseError::msg(
            "\\middle must be within \\left and \\right",
        ));
    }

    Ok(ParseNode::Middle {
        mode: ctx.parser.mode,
        delim,
        loc: None,
    })
}

fn get_delim_text(node: &ParseNode, parser: &crate::parser::Parser<'_>) -> ParseResult<String> {
    if let Some(text) = node.symbol_text() {
        return parser.text(text);
    }
    Err(ParseError::msg("Invalid delimiter type"))
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\left"],
        FunctionSpec {
            node_type: "leftright",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_left,
        },
    ),
    (
        &["\\right"],
        FunctionSpec {
            node_type: "leftright-right",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_right,
        },
    ),
    (
        &["\\middle"],
        FunctionSpec {
            node_type: "middle",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_middle,
        },
    ),
];
