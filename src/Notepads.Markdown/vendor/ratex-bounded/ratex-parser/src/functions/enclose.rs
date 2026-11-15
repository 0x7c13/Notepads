use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\fbox"],
        "enclose",
        1,
        0,
        Some(vec![ArgType::HBox]),
        false,
        true,
        true,
        false,
        false,
        handle_fbox,
    );

    define_function_full(
        map,
        &[
            "\\cancel",
            "\\bcancel",
            "\\xcancel",
            "\\phase",
            "\\slashed",
            "\\centernot",
        ],
        "enclose",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_cancel,
    );

    // \sout originates as a LaTeX text-mode command but KaTeX accepts it in
    // both modes (allowedInText: true, no argType override → arg inherits
    // surrounding mode). We mirror that so `\text{\sout{abc}}` parses while
    // `\sout{x^2}` still keeps its argument in math mode.
    define_function_full(
        map,
        &["\\sout"],
        "enclose",
        1,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_cancel,
    );

    define_function_full(
        map,
        &["\\angl"],
        "enclose",
        1,
        0,
        Some(vec![ArgType::HBox]),
        false,
        false,
        true,
        false,
        false,
        handle_angl,
    );
}

fn handle_fbox(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Enclose {
        mode: ctx.parser.mode,
        label: "\\fbox".to_string(),
        background_color: None,
        border_color: None,
        body: Box::new(args.into_iter().next().unwrap()),
        loc: None,
    })
}

fn handle_cancel(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Enclose {
        mode: ctx.parser.mode,
        label: ctx.func_name.clone(),
        background_color: None,
        border_color: None,
        body: Box::new(args.into_iter().next().unwrap()),
        loc: None,
    })
}

fn handle_angl(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Enclose {
        mode: ctx.parser.mode,
        label: "\\angl".to_string(),
        background_color: None,
        border_color: None,
        body: Box::new(args.into_iter().next().unwrap()),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\fbox"],
        FunctionSpec {
            node_type: "enclose",
            num_args: 1,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::HBox])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_fbox,
        },
    ),
    (
        &[
            "\\cancel",
            "\\bcancel",
            "\\xcancel",
            "\\phase",
            "\\slashed",
            "\\centernot",
        ],
        FunctionSpec {
            node_type: "enclose",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_cancel,
        },
    ),
    (
        &["\\sout"],
        FunctionSpec {
            node_type: "enclose",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_cancel,
        },
    ),
    (
        &["\\angl"],
        FunctionSpec {
            node_type: "enclose",
            num_args: 1,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::HBox])),
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_angl,
        },
    ),
];
