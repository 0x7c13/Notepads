use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\raisebox"],
        "raisebox",
        2,
        0,
        Some(vec![ArgType::Size, ArgType::HBox]),
        false,
        true,
        true,
        false,
        false,
        handle_raisebox,
    );
}

fn handle_raisebox(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut args = args.into_iter();
    let dy = match args.next() {
        Some(ParseNode::Size { value, .. }) => value,
        _ => return Err(ParseError::msg("Expected size for \\raisebox")),
    };
    let body = args.next().unwrap();

    Ok(ParseNode::RaiseBox {
        mode: ctx.parser.mode,
        dy,
        body: Box::new(body),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\raisebox"],
    FunctionSpec {
        node_type: "raisebox",
        num_args: 2,
        num_optional_args: 0,
        arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Size, ArgType::HBox])),
        allowed_in_argument: false,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_raisebox,
    },
)];
