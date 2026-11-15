use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

struct DelimInfo {
    mclass: &'static str,
    size: u8,
}

fn delim_info(name: &str) -> Option<DelimInfo> {
    match name {
        "\\bigl" => Some(DelimInfo {
            mclass: "mopen",
            size: 1,
        }),
        "\\Bigl" => Some(DelimInfo {
            mclass: "mopen",
            size: 2,
        }),
        "\\biggl" => Some(DelimInfo {
            mclass: "mopen",
            size: 3,
        }),
        "\\Biggl" => Some(DelimInfo {
            mclass: "mopen",
            size: 4,
        }),
        "\\bigr" => Some(DelimInfo {
            mclass: "mclose",
            size: 1,
        }),
        "\\Bigr" => Some(DelimInfo {
            mclass: "mclose",
            size: 2,
        }),
        "\\biggr" => Some(DelimInfo {
            mclass: "mclose",
            size: 3,
        }),
        "\\Biggr" => Some(DelimInfo {
            mclass: "mclose",
            size: 4,
        }),
        "\\bigm" => Some(DelimInfo {
            mclass: "mrel",
            size: 1,
        }),
        "\\Bigm" => Some(DelimInfo {
            mclass: "mrel",
            size: 2,
        }),
        "\\biggm" => Some(DelimInfo {
            mclass: "mrel",
            size: 3,
        }),
        "\\Biggm" => Some(DelimInfo {
            mclass: "mrel",
            size: 4,
        }),
        "\\big" => Some(DelimInfo {
            mclass: "mord",
            size: 1,
        }),
        "\\Big" => Some(DelimInfo {
            mclass: "mord",
            size: 2,
        }),
        "\\bigg" => Some(DelimInfo {
            mclass: "mord",
            size: 3,
        }),
        "\\Bigg" => Some(DelimInfo {
            mclass: "mord",
            size: 4,
        }),
        _ => None,
    }
}

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\bigl", "\\Bigl", "\\biggl", "\\Biggl", "\\bigr", "\\Bigr", "\\biggr", "\\Biggr",
            "\\bigm", "\\Bigm", "\\biggm", "\\Biggm", "\\big", "\\Big", "\\bigg", "\\Bigg",
        ],
        "delimsizing",
        1,
        0,
        Some(vec![ArgType::Primitive]),
        false,
        false,
        true,
        false,
        false,
        handle_delimsizing,
    );
}

fn handle_delimsizing(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let delim = check_delimiter(&args[0], &ctx.func_name, ctx.parser)?;
    let info = delim_info(&ctx.func_name).unwrap();

    Ok(ParseNode::DelimSizing {
        mode: ctx.parser.mode,
        size: info.size,
        mclass: ctx.parser.text(info.mclass)?,
        delim,
        loc: None,
    })
}

fn check_delimiter(
    node: &ParseNode,
    func_name: &str,
    parser: &crate::parser::Parser<'_>,
) -> ParseResult<String> {
    if let Some(text) = node.symbol_text() {
        if is_valid_delimiter(text) {
            return parser.text(text);
        }
        return Err(ParseError::msg(parser.formatted(format_args!(
            "Invalid delimiter '{}' after '{}'",
            text, func_name
        ))?));
    }
    Err(ParseError::msg(parser.formatted(format_args!(
        "Invalid delimiter type '{}' after '{}'",
        node.type_name(),
        func_name
    ))?))
}

fn is_valid_delimiter(text: &str) -> bool {
    matches!(
        text,
        "(" | "\\lparen"
            | ")"
            | "\\rparen"
            | "["
            | "\\lbrack"
            | "]"
            | "\\rbrack"
            | "\\{"
            | "\\lbrace"
            | "\\}"
            | "\\rbrace"
            | "\\lfloor"
            | "\\rfloor"
            | "\\lceil"
            | "\\rceil"
            | "<"
            | ">"
            | "\\langle"
            | "\\rangle"
            | "\\lt"
            | "\\gt"
            | "\\lvert"
            | "\\rvert"
            | "\\lVert"
            | "\\rVert"
            | "\\lgroup"
            | "\\rgroup"
            | "\\lmoustache"
            | "\\rmoustache"
            | "/"
            | "\\backslash"
            | "|"
            | "\\vert"
            | "\\|"
            | "\\Vert"
            | "\\uparrow"
            | "\\Uparrow"
            | "\\downarrow"
            | "\\Downarrow"
            | "\\updownarrow"
            | "\\Updownarrow"
            | "."
    )
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &[
        "\\bigl", "\\Bigl", "\\biggl", "\\Biggl", "\\bigr", "\\Bigr", "\\biggr", "\\Biggr",
        "\\bigm", "\\Bigm", "\\biggm", "\\Biggm", "\\big", "\\Big", "\\bigg", "\\Bigg",
    ],
    FunctionSpec {
        node_type: "delimsizing",
        num_args: 1,
        num_optional_args: 0,
        arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Primitive])),
        allowed_in_argument: false,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_delimsizing,
    },
)];
