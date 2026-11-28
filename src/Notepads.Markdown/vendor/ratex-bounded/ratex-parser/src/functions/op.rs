use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    // Limits, symbols (big operators) — includes single-char Unicode equivalents
    define_function_full(
        map,
        &[
            "\\coprod",
            "\\bigvee",
            "\\bigwedge",
            "\\biguplus",
            "\\bigcap",
            "\\bigcup",
            "\\intop",
            "\\prod",
            "\\sum",
            "\\bigotimes",
            "\\bigoplus",
            "\\bigodot",
            "\\bigsqcup",
            "\\smallint",
            "\u{220F}",
            "\u{2210}",
            "\u{2211}",
            "\u{22C0}",
            "\u{22C1}",
            "\u{22C2}",
            "\u{22C3}",
            "\u{2A00}",
            "\u{2A01}",
            "\u{2A02}",
            "\u{2A04}",
            "\u{2A06}",
        ],
        "op",
        0,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_op_symbol_limits,
    );

    // No limits, not symbols (trig/log functions)
    define_function_full(
        map,
        &[
            "\\arcsin", "\\arccos", "\\arctan", "\\arctg", "\\arcctg", "\\arg", "\\ch", "\\cos",
            "\\cosec", "\\cosh", "\\cot", "\\cotg", "\\coth", "\\csc", "\\ctg", "\\cth", "\\deg",
            "\\dim", "\\exp", "\\hom", "\\ker", "\\lg", "\\ln", "\\log", "\\sec", "\\sin",
            "\\sinh", "\\sh", "\\tan", "\\tanh", "\\tg", "\\th",
        ],
        "op",
        0,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_op_text_nolimits,
    );

    // Limits, not symbols (det, gcd, lim, etc.)
    define_function_full(
        map,
        &[
            "\\det", "\\gcd", "\\inf", "\\lim", "\\max", "\\min", "\\Pr", "\\sup",
        ],
        "op",
        0,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_op_text_limits,
    );

    // No limits, symbols (integrals) — including single-char Unicode equivalents.
    define_function_full(
        map,
        &[
            "\\int", "\\iint", "\\iiint", "\\oint", "\\oiint", "\\oiiint", "\u{222B}", "\u{222C}",
            "\u{222D}", "\u{222E}", "\u{222F}", "\u{2230}",
        ],
        "op",
        0,
        0,
        None,
        true, // allowed_in_argument
        false,
        true,
        false,
        false,
        handle_op_symbol_nolimits,
    );

    // \mathop
    define_function_full(
        map,
        &["\\mathop"],
        "op",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        true, // primitive
        handle_mathop,
    );
}

fn single_char_big_op(c: &str) -> Option<&'static str> {
    match c {
        "\u{220F}" => Some("\\prod"),
        "\u{2210}" => Some("\\coprod"),
        "\u{2211}" => Some("\\sum"),
        "\u{22C0}" => Some("\\bigwedge"),
        "\u{22C1}" => Some("\\bigvee"),
        "\u{22C2}" => Some("\\bigcap"),
        "\u{22C3}" => Some("\\bigcup"),
        "\u{2A00}" => Some("\\bigodot"),
        "\u{2A01}" => Some("\\bigoplus"),
        "\u{2A02}" => Some("\\bigotimes"),
        "\u{2A04}" => Some("\\biguplus"),
        "\u{2A06}" => Some("\\bigsqcup"),
        _ => None,
    }
}

fn single_char_integral(c: &str) -> Option<&'static str> {
    match c {
        "\u{222B}" => Some("\\int"),
        "\u{222C}" => Some("\\iint"),
        "\u{222D}" => Some("\\iiint"),
        "\u{222E}" => Some("\\oint"),
        "\u{222F}" => Some("\\oiint"),
        "\u{2230}" => Some("\\oiiint"),
        _ => None,
    }
}

fn handle_op_symbol_limits(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let name = ctx
        .parser
        .text(single_char_big_op(&ctx.func_name).unwrap_or(&ctx.func_name))?;
    Ok(ParseNode::Op {
        mode: ctx.parser.mode,
        limits: true,
        always_handle_sup_sub: None,
        suppress_base_shift: None,
        parent_is_sup_sub: false,
        symbol: true,
        name: Some(name),
        body: None,
        loc: None,
    })
}

fn handle_op_text_nolimits(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Op {
        mode: ctx.parser.mode,
        limits: false,
        always_handle_sup_sub: None,
        suppress_base_shift: None,
        parent_is_sup_sub: false,
        symbol: false,
        name: Some(ctx.parser.text(&ctx.func_name)?),
        body: None,
        loc: None,
    })
}

fn handle_op_text_limits(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::Op {
        mode: ctx.parser.mode,
        limits: true,
        always_handle_sup_sub: None,
        suppress_base_shift: None,
        parent_is_sup_sub: false,
        symbol: false,
        name: Some(ctx.parser.text(&ctx.func_name)?),
        body: None,
        loc: None,
    })
}

fn handle_op_symbol_nolimits(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let name = ctx
        .parser
        .text(single_char_integral(&ctx.func_name).unwrap_or(&ctx.func_name))?;
    Ok(ParseNode::Op {
        mode: ctx.parser.mode,
        limits: false,
        always_handle_sup_sub: None,
        suppress_base_shift: None,
        parent_is_sup_sub: false,
        symbol: true,
        name: Some(name),
        body: None,
        loc: None,
    })
}

fn handle_mathop(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = ctx.parser.ord_argument(args.into_iter().next().unwrap())?;
    Ok(ParseNode::Op {
        mode: ctx.parser.mode,
        limits: false,
        always_handle_sup_sub: None,
        suppress_base_shift: None,
        parent_is_sup_sub: false,
        symbol: false,
        name: None,
        body: Some(body),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &[
            "\\coprod",
            "\\bigvee",
            "\\bigwedge",
            "\\biguplus",
            "\\bigcap",
            "\\bigcup",
            "\\intop",
            "\\prod",
            "\\sum",
            "\\bigotimes",
            "\\bigoplus",
            "\\bigodot",
            "\\bigsqcup",
            "\\smallint",
            "\u{220F}",
            "\u{2210}",
            "\u{2211}",
            "\u{22C0}",
            "\u{22C1}",
            "\u{22C2}",
            "\u{22C3}",
            "\u{2A00}",
            "\u{2A01}",
            "\u{2A02}",
            "\u{2A04}",
            "\u{2A06}",
        ],
        FunctionSpec {
            node_type: "op",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_op_symbol_limits,
        },
    ),
    (
        &[
            "\\arcsin", "\\arccos", "\\arctan", "\\arctg", "\\arcctg", "\\arg", "\\ch", "\\cos",
            "\\cosec", "\\cosh", "\\cot", "\\cotg", "\\coth", "\\csc", "\\ctg", "\\cth", "\\deg",
            "\\dim", "\\exp", "\\hom", "\\ker", "\\lg", "\\ln", "\\log", "\\sec", "\\sin",
            "\\sinh", "\\sh", "\\tan", "\\tanh", "\\tg", "\\th",
        ],
        FunctionSpec {
            node_type: "op",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_op_text_nolimits,
        },
    ),
    (
        &[
            "\\det", "\\gcd", "\\inf", "\\lim", "\\max", "\\min", "\\Pr", "\\sup",
        ],
        FunctionSpec {
            node_type: "op",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_op_text_limits,
        },
    ),
    (
        &[
            "\\int", "\\iint", "\\iiint", "\\oint", "\\oiint", "\\oiiint", "\u{222B}", "\u{222C}",
            "\u{222D}", "\u{222E}", "\u{222F}", "\u{2230}",
        ],
        FunctionSpec {
            node_type: "op",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: true,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_op_symbol_nolimits,
        },
    ),
    (
        &["\\mathop"],
        FunctionSpec {
            node_type: "op",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_mathop,
        },
    ),
];
