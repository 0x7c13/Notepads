use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::{Measurement, ParseNode};

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\quad",
            "\\qquad",
            "\\enspace",
            "\\thinspace",
            "\\medspace",
            "\\thickspace",
            "\\negthinspace",
            "\\negmedspace",
            "\\negthickspace",
            "\\nobreakspace",
        ],
        "spacing",
        0,
        0,
        None,
        true, // allowed_in_argument
        true, // allowed_in_text
        true,
        false,
        false,
        handle_spacing,
    );

    define_function_full(
        map,
        &["\\hspace"],
        "kern",
        1,
        0,
        Some(vec![crate::functions::ArgType::Size]),
        false,
        true,
        true,
        false,
        false,
        handle_hspace,
    );

    // \hfill — produces a kern that fills available space
    define_function_full(
        map,
        &["\\hfill"],
        "spacing",
        0,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_spacing,
    );
}

fn handle_spacing(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::SpacingNode {
        mode: ctx.parser.mode,
        text: ctx.parser.text(&ctx.func_name)?,
        loc: None,
    })
}

fn handle_hspace(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let dimension = if let ParseNode::Size { value, .. } = &args[0] {
        value.clone()
    } else {
        Measurement {
            number: 0.0,
            unit: "em".to_string(),
        }
    };

    Ok(ParseNode::Kern {
        mode: ctx.parser.mode,
        dimension,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &[
            "\\quad",
            "\\qquad",
            "\\enspace",
            "\\thinspace",
            "\\medspace",
            "\\thickspace",
            "\\negthinspace",
            "\\negmedspace",
            "\\negthickspace",
            "\\nobreakspace",
        ],
        FunctionSpec {
            node_type: "spacing",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: true,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_spacing,
        },
    ),
    (
        &["\\hspace"],
        FunctionSpec {
            node_type: "kern",
            num_args: 1,
            num_optional_args: 0,
            arg_types: Some(std::borrow::Cow::Borrowed(&[
                crate::functions::ArgType::Size,
            ])),
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_hspace,
        },
    ),
    (
        &["\\hfill"],
        FunctionSpec {
            node_type: "spacing",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_spacing,
        },
    ),
];
