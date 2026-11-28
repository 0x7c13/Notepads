use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\\\", "\\newline"],
        "cr",
        0,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_cr,
    );
}

fn handle_cr(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    // Only parse optional [size] if next token is directly "[" (no space)
    let size = if ctx.parser.gullet.future().text == "[" {
        ctx.parser.parse_size_group(true)?
    } else {
        None
    };

    let size = size.and_then(|node| {
        if let ParseNode::Size { value, .. } = node {
            Some(value)
        } else {
            None
        }
    });

    let new_line = true;

    Ok(ParseNode::Cr {
        mode: ctx.parser.mode,
        new_line,
        size,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\\\", "\\newline"],
    FunctionSpec {
        node_type: "cr",
        num_args: 0,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_cr,
    },
)];
