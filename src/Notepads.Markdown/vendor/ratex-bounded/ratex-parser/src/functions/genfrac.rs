use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::{ParseNode, StyleStr};

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    // \frac, \dfrac, \tfrac, \cfrac, \binom, \dbinom, \tbinom
    define_function_full(
        map,
        &[
            "\\cfrac",
            "\\dfrac",
            "\\frac",
            "\\tfrac",
            "\\dbinom",
            "\\binom",
            "\\tbinom",
            "\\\\atopfrac",
            "\\\\bracefrac",
            "\\\\brackfrac",
        ],
        "genfrac",
        2,
        0,
        None,
        true,  // allowed_in_argument
        false, // allowed_in_text
        true,  // allowed_in_math
        false, // infix
        false, // primitive
        handle_genfrac,
    );

    // \genfrac{left}{right}{bar-size}{style}{numer}{denom}
    define_function_full(
        map,
        &["\\genfrac"],
        "genfrac",
        6,
        0,
        Some(vec![
            ArgType::Math,
            ArgType::Math,
            ArgType::Size,
            ArgType::Text,
            ArgType::Math,
            ArgType::Math,
        ]),
        true,
        false,
        true,
        false,
        false,
        handle_genfrac_full,
    );

    // Infix: \over, \choose, \atop, \brace, \brack
    define_function_full(
        map,
        &["\\over", "\\choose", "\\atop", "\\brace", "\\brack"],
        "infix",
        0,
        0,
        None,
        false,
        false,
        true,
        true, // infix
        false,
        handle_infix,
    );

    // \above{size} — infix with bar size
    define_function_full(
        map,
        &["\\above"],
        "infix",
        1,
        0,
        Some(vec![ArgType::Size]),
        false,
        false,
        true,
        true, // infix
        false,
        handle_above_infix,
    );

    // \\abovefrac — internal 3-arg handler for \above
    define_function_full(
        map,
        &["\\\\abovefrac"],
        "genfrac",
        3,
        0,
        Some(vec![ArgType::Math, ArgType::Size, ArgType::Math]),
        false,
        false,
        true,
        false,
        false,
        handle_abovefrac,
    );
}

fn handle_genfrac(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut args = args.into_iter();
    let numer = args.next().unwrap();
    let denom = args.next().unwrap();
    let func_name = &ctx.func_name;
    let mode = ctx.parser.mode;

    let (has_bar_line, left_delim, right_delim) = match func_name.as_str() {
        "\\cfrac" | "\\dfrac" | "\\frac" | "\\tfrac" => (true, None, None),
        "\\\\atopfrac" => (false, None, None),
        "\\dbinom" | "\\binom" | "\\tbinom" => (
            false,
            Some(ctx.parser.text("(")?),
            Some(ctx.parser.text(")")?),
        ),
        "\\\\bracefrac" => (
            false,
            Some(ctx.parser.text("\\{")?),
            Some(ctx.parser.text("\\}")?),
        ),
        "\\\\brackfrac" => (
            false,
            Some(ctx.parser.text("[")?),
            Some(ctx.parser.text("]")?),
        ),
        _ => (true, None, None),
    };

    let continued = func_name == "\\cfrac";

    let frac = ParseNode::GenFrac {
        mode,
        continued,
        numer: ctx.parser.boxed(numer)?,
        denom: ctx.parser.boxed(denom)?,
        has_bar_line,
        left_delim,
        right_delim,
        bar_size: None,
        loc: None,
    };

    // Wrap in styling if needed
    let style = if continued || func_name.starts_with("\\d") {
        Some(StyleStr::Display)
    } else if func_name.starts_with("\\t") {
        Some(StyleStr::Text)
    } else {
        None
    };

    match style {
        Some(s) => Ok(ParseNode::Styling {
            mode,
            style: s,
            body: ctx.parser.values([frac])?,
            loc: None,
        }),
        None => Ok(frac),
    }
}

fn handle_genfrac_full(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mode = ctx.parser.mode;
    let numer = args[4].clone();
    let denom = args[5].clone();

    let left_delim = extract_delim(&args[0]);
    let right_delim = extract_delim(&args[1]);

    let (has_bar_line, bar_size) = match &args[2] {
        ParseNode::Size {
            value, is_blank, ..
        } => {
            if *is_blank {
                (true, None)
            } else {
                let has = value.number > 0.0;
                (has, Some(value.clone()))
            }
        }
        _ => (true, None),
    };

    let style_strs = [
        StyleStr::Display,
        StyleStr::Text,
        StyleStr::Script,
        StyleStr::Scriptscript,
    ];
    let style = match &args[3] {
        ParseNode::OrdGroup { body, .. } if !body.is_empty() => {
            extract_textord_num(&body[0]).and_then(|n| style_strs.get(n).cloned())
        }
        node => extract_textord_num(node).and_then(|n| style_strs.get(n).cloned()),
    };

    let frac = ParseNode::GenFrac {
        mode,
        continued: false,
        numer: Box::new(numer),
        denom: Box::new(denom),
        has_bar_line,
        left_delim,
        right_delim,
        bar_size,
        loc: None,
    };

    match style {
        Some(s) => Ok(ParseNode::Styling {
            mode,
            style: s,
            body: vec![frac],
            loc: None,
        }),
        None => Ok(frac),
    }
}

fn extract_delim(node: &ParseNode) -> Option<String> {
    let text = match node {
        ParseNode::Atom { text, .. } => text.clone(),
        ParseNode::OrdGroup { body, .. } if body.len() == 1 => match &body[0] {
            ParseNode::Atom { text, .. } => text.clone(),
            _ => return None,
        },
        _ => return None,
    };
    if text == "." || text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn extract_textord_num(node: &ParseNode) -> Option<usize> {
    match node {
        ParseNode::TextOrd { text, .. } => text.parse().ok(),
        _ => None,
    }
}

fn handle_above_infix(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let size = match &args[0] {
        ParseNode::Size { value, .. } => Some(value.clone()),
        _ => None,
    };

    Ok(ParseNode::Infix {
        mode: ctx.parser.mode,
        replace_with: "\\\\abovefrac".to_string(),
        size,
        loc: None,
    })
}

fn handle_abovefrac(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mode = ctx.parser.mode;
    let numer = args[0].clone();
    let denom = args[2].clone();

    let bar_size = match &args[1] {
        ParseNode::Infix { size, .. } => size.clone(),
        _ => None,
    };

    let has_bar_line = bar_size.as_ref().is_some_and(|m| m.number > 0.0);

    Ok(ParseNode::GenFrac {
        mode,
        continued: false,
        numer: Box::new(numer),
        denom: Box::new(denom),
        has_bar_line,
        left_delim: None,
        right_delim: None,
        bar_size,
        loc: None,
    })
}

fn handle_infix(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let replace_with = match ctx.func_name.as_str() {
        "\\over" => "\\frac",
        "\\choose" => "\\binom",
        "\\atop" => "\\\\atopfrac",
        "\\brace" => "\\\\bracefrac",
        "\\brack" => "\\\\brackfrac",
        _ => "\\frac",
    };

    Ok(ParseNode::Infix {
        mode: ctx.parser.mode,
        replace_with: ctx.parser.text(replace_with)?,
        size: None,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &[
            "\\cfrac",
            "\\dfrac",
            "\\frac",
            "\\tfrac",
            "\\dbinom",
            "\\binom",
            "\\tbinom",
            "\\\\atopfrac",
            "\\\\bracefrac",
            "\\\\brackfrac",
        ],
        FunctionSpec {
            node_type: "genfrac",
            num_args: 2,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: true,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_genfrac,
        },
    ),
    (
        &["\\genfrac"],
        FunctionSpec {
            node_type: "genfrac",
            num_args: 6,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[
                ArgType::Math,
                ArgType::Math,
                ArgType::Size,
                ArgType::Text,
                ArgType::Math,
                ArgType::Math,
            ])),
            allowed_in_argument: true,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_genfrac_full,
        },
    ),
    (
        &["\\over", "\\choose", "\\atop", "\\brace", "\\brack"],
        FunctionSpec {
            node_type: "infix",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: true,
            primitive: false,
            handler: handle_infix,
        },
    ),
    (
        &["\\above"],
        FunctionSpec {
            node_type: "infix",
            num_args: 1,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Size])),
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: true,
            primitive: false,
            handler: handle_above_infix,
        },
    ),
    (
        &["\\\\abovefrac"],
        FunctionSpec {
            node_type: "genfrac",
            num_args: 3,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[
                ArgType::Math,
                ArgType::Size,
                ArgType::Math,
            ])),
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_abovefrac,
        },
    ),
];
