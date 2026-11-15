use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\mathrm",
            "\\mathit",
            "\\mathbf",
            "\\mathnormal",
            "\\mathsfit",
            "\\mathbb",
            "\\mathcal",
            "\\mathfrak",
            "\\mathscr",
            "\\mathsf",
            "\\mathtt",
            "\\Bbb",
            "\\bold",
            "\\frak",
        ],
        "font",
        1,
        0,
        None,
        true, // allowed_in_argument
        false,
        true,
        false,
        false,
        handle_font,
    );

    // \boldsymbol and \bm produce mclass nodes
    define_function_full(
        map,
        &["\\boldsymbol", "\\bm"],
        "mclass",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_boldsymbol,
    );

    // \emph — emphasis (italic in text mode)
    define_function_full(
        map,
        &["\\emph"],
        "text",
        1,
        0,
        Some(vec![ArgType::Text]),
        false,
        true,
        true,
        false,
        false,
        handle_emph,
    );

    // Old-style font switching commands: \rm, \sf, \tt, \bf, \it, \cal
    define_function_full(
        map,
        &["\\rm", "\\sf", "\\tt", "\\bf", "\\it", "\\cal"],
        "font",
        0,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_old_font,
    );
}

fn handle_font(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = ParseNode::normalize_argument(args.into_iter().next().unwrap());

    let font_aliases: &[(&str, &str)] = &[
        ("\\Bbb", "\\mathbb"),
        ("\\bold", "\\mathbf"),
        ("\\frak", "\\mathfrak"),
        ("\\bm", "\\boldsymbol"),
    ];

    let mut func = ctx.parser.text(&ctx.func_name)?;
    for &(alias, target) in font_aliases {
        if func == alias {
            func = ctx.parser.text(target)?;
            break;
        }
    }

    let font_name = ctx
        .parser
        .text(func.strip_prefix('\\').unwrap_or(func.as_str()))?;

    Ok(ParseNode::Font {
        mode: ctx.parser.mode,
        font: font_name,
        body: ctx.parser.boxed(body)?,
        loc: None,
    })
}

fn handle_old_font(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let font_name = ctx
        .parser
        .formatted(format_args!("math{}", &ctx.func_name[1..]))?;
    let break_on = ctx.parser.clone_value(&ctx.break_on_token_text)?;
    let body = ctx.parser.parse_expression(true, break_on.as_deref())?;

    Ok(ParseNode::Font {
        mode: ctx.parser.mode,
        font: font_name,
        body: ctx.parser.boxed(ParseNode::OrdGroup {
            mode: ctx.parser.mode,
            body,
            semisimple: None,
            loc: None,
        })?,
        loc: None,
    })
}

fn handle_boldsymbol(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = args.into_iter().next().unwrap();

    Ok(ParseNode::MClass {
        mode: ctx.parser.mode,
        mclass: "mord".to_string(),
        body: vec![ParseNode::Font {
            mode: ctx.parser.mode,
            font: "boldsymbol".to_string(),
            body: ctx.parser.boxed(body)?,
            loc: None,
        }],
        is_character_box: false,
        loc: None,
    })
}

fn handle_emph(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = args.into_iter().next().unwrap();
    let body_vec = ctx.parser.ord_argument(body)?;
    Ok(ParseNode::Text {
        mode: ctx.parser.mode,
        body: body_vec,
        font: Some(ctx.parser.text("\\emph")?),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &[
            "\\mathrm",
            "\\mathit",
            "\\mathbf",
            "\\mathnormal",
            "\\mathsfit",
            "\\mathbb",
            "\\mathcal",
            "\\mathfrak",
            "\\mathscr",
            "\\mathsf",
            "\\mathtt",
            "\\Bbb",
            "\\bold",
            "\\frak",
        ],
        FunctionSpec {
            node_type: "font",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: true,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_font,
        },
    ),
    (
        &["\\boldsymbol", "\\bm"],
        FunctionSpec {
            node_type: "mclass",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_boldsymbol,
        },
    ),
    (
        &["\\emph"],
        FunctionSpec {
            node_type: "text",
            num_args: 1,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Text])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_emph,
        },
    ),
    (
        &["\\rm", "\\sf", "\\tt", "\\bf", "\\it", "\\cal"],
        FunctionSpec {
            node_type: "font",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_old_font,
        },
    ),
];
