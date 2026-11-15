use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    // \textcolor[model]{color}{body}  (model is optional, MathJax extension)
    define_function_full(
        map,
        &["\\textcolor"],
        "color",
        2,
        1,
        Some(vec![ArgType::Raw, ArgType::Color, ArgType::Original]),
        false,
        true, // allowed_in_text
        true,
        false,
        false,
        handle_textcolor,
    );

    // \color[model]{color}  (model is optional, MathJax extension)
    define_function_full(
        map,
        &["\\color"],
        "color",
        1,
        1,
        Some(vec![ArgType::Raw, ArgType::Color]),
        false,
        true,
        true,
        false,
        false,
        handle_color,
    );

    // \colorbox{color}{body}
    define_function_full(
        map,
        &["\\colorbox"],
        "enclose",
        2,
        0,
        Some(vec![ArgType::Color, ArgType::Text]),
        false,
        true,
        true,
        false,
        false,
        handle_colorbox,
    );

    // \fcolorbox{bordercolor}{bgcolor}{body}
    define_function_full(
        map,
        &["\\fcolorbox"],
        "enclose",
        3,
        0,
        Some(vec![ArgType::Color, ArgType::Color, ArgType::Text]),
        false,
        true,
        true,
        false,
        false,
        handle_fcolorbox,
    );
}

fn handle_textcolor(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let color = encode_color(extract_color(&args[0])?, &opt_args);
    let body = ParseNode::ord_argument(args[1].clone());

    Ok(ParseNode::Color {
        mode: ctx.parser.mode,
        color,
        body,
        loc: None,
    })
}

fn handle_color(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let color = encode_color(extract_color(&args[0])?, &opt_args);
    let break_on = ctx.break_on_token_text.clone();
    let body = ctx.parser.parse_expression(true, break_on.as_deref())?;

    Ok(ParseNode::Color {
        mode: ctx.parser.mode,
        color,
        body,
        loc: None,
    })
}

fn handle_colorbox(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let color = extract_color(&args[0])?;
    let body = args[1].clone();

    Ok(ParseNode::Enclose {
        mode: ctx.parser.mode,
        label: "\\colorbox".to_string(),
        background_color: Some(color),
        border_color: None,
        body: Box::new(body),
        loc: None,
    })
}

fn handle_fcolorbox(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let border_color = extract_color(&args[0])?;
    let bg_color = extract_color(&args[1])?;
    let body = args[2].clone();

    Ok(ParseNode::Enclose {
        mode: ctx.parser.mode,
        label: "\\fcolorbox".to_string(),
        background_color: Some(bg_color),
        border_color: Some(border_color),
        body: Box::new(body),
        loc: None,
    })
}

fn extract_color(node: &ParseNode) -> ParseResult<String> {
    if let ParseNode::ColorToken { color, .. } = node {
        Ok(color.clone())
    } else {
        Err(ParseError::msg("Expected color token"))
    }
}

/// When a MathJax color model opt-arg is present, encode as `[MODEL]value`.
fn encode_color(value: String, opt_args: &[Option<ParseNode>]) -> String {
    if let Some(Some(ParseNode::Raw { string: model, .. })) = opt_args.first() {
        let model = model.trim();
        if !model.is_empty() {
            return format!("[{}]{}", model, value);
        }
    }
    value
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\textcolor"],
        FunctionSpec {
            node_type: "color",
            num_args: 2,
            num_optional_args: 1,
            arg_types: Some(std::borrow::Cow::Borrowed(&[
                ArgType::Raw,
                ArgType::Color,
                ArgType::Original,
            ])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_textcolor,
        },
    ),
    (
        &["\\color"],
        FunctionSpec {
            node_type: "color",
            num_args: 1,
            num_optional_args: 1,
            arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Raw, ArgType::Color])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_color,
        },
    ),
    (
        &["\\colorbox"],
        FunctionSpec {
            node_type: "enclose",
            num_args: 2,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Color, ArgType::Text])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_colorbox,
        },
    ),
    (
        &["\\fcolorbox"],
        FunctionSpec {
            node_type: "enclose",
            num_args: 3,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[
                ArgType::Color,
                ArgType::Color,
                ArgType::Text,
            ])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_fcolorbox,
        },
    ),
];
