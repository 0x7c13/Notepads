use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\hbox"],
        "hbox",
        1,
        0,
        Some(vec![ArgType::Text]),
        false,
        true,
        true,
        false,
        true,
        handle_hbox,
    );
}

fn handle_hbox(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::HBox {
        mode: ctx.parser.mode,
        body: ParseNode::ord_argument(args.into_iter().next().unwrap()),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\hbox"],
    FunctionSpec {
        node_type: "hbox",
        num_args: 1,
        num_optional_args: 0,
        arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Text])),
        allowed_in_argument: false,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: true,
        handler: handle_hbox,
    },
)];
