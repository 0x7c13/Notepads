use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\mathllap", "\\mathrlap", "\\mathclap"],
        "lap",
        1,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_lap,
    );
}

fn handle_lap(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let alignment = ctx.parser.text(&ctx.func_name[5..])?;

    Ok(ParseNode::Lap {
        mode: ctx.parser.mode,
        alignment,
        body: ctx.parser.boxed(args.into_iter().next().unwrap())?,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\mathllap", "\\mathrlap", "\\mathclap"],
    FunctionSpec {
        node_type: "lap",
        num_args: 1,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_lap,
    },
)];
