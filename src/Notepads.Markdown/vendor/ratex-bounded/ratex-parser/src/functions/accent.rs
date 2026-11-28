use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::{Mode, ParseNode};

static NON_STRETCHY: &[&str] = &[
    "\\acute",
    "\\grave",
    "\\ddot",
    "\\tilde",
    "\\bar",
    "\\breve",
    "\\check",
    "\\hat",
    "\\vec",
    "\\dot",
    "\\mathring",
    "\\dddot",
    "\\ddddot",
];

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    // Math-mode accents
    define_function_full(
        map,
        &[
            "\\acute",
            "\\grave",
            "\\ddot",
            "\\tilde",
            "\\bar",
            "\\breve",
            "\\check",
            "\\hat",
            "\\vec",
            "\\dot",
            "\\mathring",
            "\\dddot",
            "\\ddddot",
            "\\widecheck",
            "\\widehat",
            "\\widetilde",
            "\\overrightarrow",
            "\\overleftarrow",
            "\\Overrightarrow",
            "\\overleftrightarrow",
            "\\overgroup",
            "\\overlinesegment",
            "\\overleftharpoon",
            "\\overrightharpoon",
        ],
        "accent",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_accent,
    );

    // Text-mode accents
    define_function_full(
        map,
        &[
            "\\'",
            "\\`",
            "\\^",
            "\\~",
            "\\=",
            "\\u",
            "\\.",
            "\\\"",
            "\\c",
            "\\r",
            "\\H",
            "\\v",
            "\\textcircled",
        ],
        "accent",
        1,
        0,
        Some(vec![ArgType::Primitive]),
        false,
        true, // allowed_in_text
        true, // allowed_in_math
        false,
        false,
        handle_text_accent,
    );

    // Under-accents
    define_function_full(
        map,
        &[
            "\\underleftarrow",
            "\\underrightarrow",
            "\\underleftrightarrow",
            "\\undergroup",
            "\\underlinesegment",
            "\\utilde",
        ],
        "accentUnder",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_accent_under,
    );
}

fn handle_accent(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let base = ParseNode::normalize_argument(args.into_iter().next().unwrap());
    let func_name = &ctx.func_name;
    let is_stretchy = !NON_STRETCHY.contains(&func_name.as_str());
    let is_shifty = !is_stretchy
        || func_name == "\\widehat"
        || func_name == "\\widetilde"
        || func_name == "\\widecheck";

    Ok(ParseNode::Accent {
        mode: ctx.parser.mode,
        label: std::mem::take(&mut ctx.func_name),
        is_stretchy: Some(is_stretchy),
        is_shifty: Some(is_shifty),
        base: ctx.parser.boxed(base)?,
        loc: None,
    })
}

fn handle_text_accent(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let base = args.into_iter().next().unwrap();
    // Text accents always produce text-mode nodes, even in math mode
    let mode = Mode::Text;

    Ok(ParseNode::Accent {
        mode,
        label: ctx.func_name.clone(),
        is_stretchy: Some(false),
        is_shifty: Some(true),
        base: Box::new(base),
        loc: None,
    })
}

fn handle_accent_under(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let base = args.into_iter().next().unwrap();

    Ok(ParseNode::AccentUnder {
        mode: ctx.parser.mode,
        label: ctx.func_name.clone(),
        is_stretchy: Some(true),
        is_shifty: Some(false),
        base: Box::new(base),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &[
            "\\acute",
            "\\grave",
            "\\ddot",
            "\\tilde",
            "\\bar",
            "\\breve",
            "\\check",
            "\\hat",
            "\\vec",
            "\\dot",
            "\\mathring",
            "\\dddot",
            "\\ddddot",
            "\\widecheck",
            "\\widehat",
            "\\widetilde",
            "\\overrightarrow",
            "\\overleftarrow",
            "\\Overrightarrow",
            "\\overleftrightarrow",
            "\\overgroup",
            "\\overlinesegment",
            "\\overleftharpoon",
            "\\overrightharpoon",
        ],
        FunctionSpec {
            node_type: "accent",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_accent,
        },
    ),
    (
        &[
            "\\'",
            "\\`",
            "\\^",
            "\\~",
            "\\=",
            "\\u",
            "\\.",
            "\\\"",
            "\\c",
            "\\r",
            "\\H",
            "\\v",
            "\\textcircled",
        ],
        FunctionSpec {
            node_type: "accent",
            num_args: 1,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Primitive])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_text_accent,
        },
    ),
    (
        &[
            "\\underleftarrow",
            "\\underrightarrow",
            "\\underleftrightarrow",
            "\\undergroup",
            "\\underlinesegment",
            "\\utilde",
        ],
        FunctionSpec {
            node_type: "accentUnder",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_accent_under,
        },
    ),
];
