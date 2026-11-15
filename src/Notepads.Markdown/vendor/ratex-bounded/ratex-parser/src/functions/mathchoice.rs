use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\mathchoice"],
        "mathchoice",
        4,
        0,
        None,
        false,
        false,
        true,
        false,
        true,
        handle_mathchoice,
    );
}

fn handle_mathchoice(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut args = args.into_iter();
    let display = ctx.parser.ord_argument(args.next().unwrap())?;
    let text = ctx.parser.ord_argument(args.next().unwrap())?;
    let script = ctx.parser.ord_argument(args.next().unwrap())?;
    let scriptscript = ctx.parser.ord_argument(args.next().unwrap())?;

    Ok(ParseNode::MathChoice {
        mode: ctx.parser.mode,
        display,
        text,
        script,
        scriptscript,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\mathchoice"],
    FunctionSpec {
        node_type: "mathchoice",
        num_args: 4,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: true,
        handler: handle_mathchoice,
    },
)];
