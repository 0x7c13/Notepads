use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\html@mathml"],
        "htmlmathml",
        2,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_htmlmathml,
    );

    define_function_full(
        map,
        &["\\htmlStyle"],
        "html",
        2,
        0,
        Some(vec![ArgType::Raw, ArgType::Original]),
        false,
        true,
        true,
        false,
        false,
        handle_htmlstyle,
    );
}

fn handle_htmlmathml(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut args = args.into_iter();
    let html = ctx.parser.ord_argument(args.next().unwrap())?;
    let mathml = ctx.parser.ord_argument(args.next().unwrap())?;

    Ok(ParseNode::HtmlMathMl {
        mode: ctx.parser.mode,
        html,
        mathml,
        loc: None,
    })
}

fn handle_htmlstyle(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut args = args.into_iter();
    let style = match args.next() {
        Some(ParseNode::Raw { string, .. }) => string,
        _ => return Err(ParseError::msg("Expected raw style for \\htmlStyle")),
    };
    let body = ParseNode::ord_argument(args.next().unwrap());
    let mut attributes = HashMap::new();
    attributes.insert("style".to_string(), style);

    Ok(ParseNode::Html {
        mode: ctx.parser.mode,
        attributes,
        body,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\html@mathml"],
        FunctionSpec {
            node_type: "htmlmathml",
            num_args: 2,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_htmlmathml,
        },
    ),
    (
        &["\\htmlStyle"],
        FunctionSpec {
            node_type: "html",
            num_args: 2,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[
                ArgType::Raw,
                ArgType::Original,
            ])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_htmlstyle,
        },
    ),
];
