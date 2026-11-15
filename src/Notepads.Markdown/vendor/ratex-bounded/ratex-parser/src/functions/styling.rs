use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::{ParseNode, StyleStr};

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\displaystyle",
            "\\textstyle",
            "\\scriptstyle",
            "\\scriptscriptstyle",
        ],
        "styling",
        0,
        0,
        None,
        true, // allowed_in_argument
        true,
        true,
        false,
        false,
        handle_styling,
    );
}

fn handle_styling(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let break_on = ctx.parser.clone_value(&ctx.break_on_token_text)?;
    let body = ctx.parser.parse_expression(true, break_on.as_deref())?;

    let style = match ctx.func_name.as_str() {
        "\\displaystyle" => StyleStr::Display,
        "\\textstyle" => StyleStr::Text,
        "\\scriptstyle" => StyleStr::Script,
        "\\scriptscriptstyle" => StyleStr::Scriptscript,
        _ => StyleStr::Text,
    };

    Ok(ParseNode::Styling {
        mode: ctx.parser.mode,
        style,
        body,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &[
        "\\displaystyle",
        "\\textstyle",
        "\\scriptstyle",
        "\\scriptscriptstyle",
    ],
    FunctionSpec {
        node_type: "styling",
        num_args: 0,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: true,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_styling,
    },
)];
