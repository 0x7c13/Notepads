use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\text",
            "\\textrm",
            "\\textsf",
            "\\texttt",
            "\\textnormal",
            "\\textbf",
            "\\textit",
            "\\textmd",
            "\\textup",
            "\\textsl",
            "\\textsc",
        ],
        "text",
        1,
        0,
        Some(vec![ArgType::Text]),
        true, // allowed_in_argument
        true, // allowed_in_text
        true,
        false,
        false,
        handle_text,
    );
}

fn handle_text(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = args.into_iter().next().unwrap();
    let body_vec = ctx.parser.ord_argument(body)?;

    let font = Some(ctx.parser.text(&ctx.func_name)?);

    Ok(ParseNode::Text {
        mode: ctx.parser.mode,
        body: body_vec,
        font,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &[
        "\\text",
        "\\textrm",
        "\\textsf",
        "\\texttt",
        "\\textnormal",
        "\\textbf",
        "\\textit",
        "\\textmd",
        "\\textup",
        "\\textsl",
        "\\textsc",
    ],
    FunctionSpec {
        node_type: "text",
        num_args: 1,
        num_optional_args: 0,
        arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Text])),
        allowed_in_argument: true,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_text,
    },
)];
