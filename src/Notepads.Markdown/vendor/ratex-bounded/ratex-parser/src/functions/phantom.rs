use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\phantom"],
        "phantom",
        1,
        0,
        None,
        true,
        true,
        true,
        false,
        false,
        handle_phantom,
    );

    define_function_full(
        map,
        &["\\vphantom"],
        "vphantom",
        1,
        0,
        None,
        true,
        true,
        true,
        false,
        false,
        handle_vphantom,
    );

    // \hphantom is defined as a macro: \smash{\phantom{#1}}
    // (registered in macro_expander builtins)

    define_function_full(
        map,
        &["\\smash"],
        "smash",
        1,
        1,
        None,
        true,
        true,
        true,
        false,
        false,
        handle_smash,
    );
}

fn handle_phantom(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = ctx.parser.ord_argument(args.into_iter().next().unwrap())?;
    Ok(ParseNode::Phantom {
        mode: ctx.parser.mode,
        body,
        loc: None,
    })
}

fn handle_vphantom(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    Ok(ParseNode::VPhantom {
        mode: ctx.parser.mode,
        body: ctx.parser.boxed(args.into_iter().next().unwrap())?,
        loc: None,
    })
}

fn handle_smash(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut smash_height = false;
    let mut smash_depth = false;

    if let Some(Some(ParseNode::OrdGroup { body, .. })) = opt_args.first() {
        for node in body {
            let text = node.symbol_text().unwrap_or("");
            match text {
                "t" => smash_height = true,
                "b" => smash_depth = true,
                _ => {
                    smash_height = false;
                    smash_depth = false;
                    break;
                }
            }
        }
    } else {
        smash_height = true;
        smash_depth = true;
    }

    Ok(ParseNode::Smash {
        mode: ctx.parser.mode,
        body: ctx.parser.boxed(args.into_iter().next().unwrap())?,
        smash_height,
        smash_depth,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\phantom"],
        FunctionSpec {
            node_type: "phantom",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: true,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_phantom,
        },
    ),
    (
        &["\\vphantom"],
        FunctionSpec {
            node_type: "vphantom",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: true,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_vphantom,
        },
    ),
    (
        &["\\smash"],
        FunctionSpec {
            node_type: "smash",
            num_args: 1,
            num_optional_args: 1,
            arg_types: None,
            allowed_in_argument: true,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_smash,
        },
    ),
];
