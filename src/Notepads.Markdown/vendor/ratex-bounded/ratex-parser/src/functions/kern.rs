use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{ArgType, FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::{Measurement, ParseNode};

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\kern", "\\mkern", "\\hskip", "\\mskip"],
        "kern",
        1,
        0,
        Some(vec![ArgType::Size]),
        true, // allowed_in_argument
        true,
        true,
        false,
        false,
        handle_kern,
    );
}

fn handle_kern(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let dimension = match args.into_iter().next().unwrap() {
        ParseNode::Size { value, .. } => value,
        _ => Measurement {
            number: 0.0,
            unit: ctx.parser.text("em")?,
        },
    };

    Ok(ParseNode::Kern {
        mode: ctx.parser.mode,
        dimension,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\kern", "\\mkern", "\\hskip", "\\mskip"],
    FunctionSpec {
        node_type: "kern",
        num_args: 1,
        num_optional_args: 0,
        arg_types: Some(std::borrow::Cow::Borrowed(&[ArgType::Size])),
        allowed_in_argument: true,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_kern,
    },
)];
