use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &[
            "\\mathord",
            "\\mathbin",
            "\\mathrel",
            "\\mathopen",
            "\\mathclose",
            "\\mathpunct",
            "\\mathinner",
        ],
        "mclass",
        1,
        0,
        None,
        false,
        false,
        true,
        false,
        true,
        handle_mclass,
    );

    define_function_full(
        map,
        &["\\stackrel", "\\overset", "\\underset"],
        "mclass",
        2,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_stackrel,
    );

    // \@binrel{x}{body} — infer mbin/mrel/mord from x, wrap body
    define_function_full(
        map,
        &["\\@binrel"],
        "mclass",
        2,
        0,
        None,
        false,
        false,
        true,
        false,
        false,
        handle_binrel,
    );
}

fn handle_mclass(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mclass = ctx
        .parser
        .formatted(format_args!("m{}", &ctx.func_name[5..]))?;
    let body = ctx.parser.ord_argument(args.into_iter().next().unwrap())?;
    let is_char_box = body.len() == 1 && body[0].is_symbol_node();

    Ok(ParseNode::MClass {
        mode: ctx.parser.mode,
        mclass,
        body,
        is_character_box: is_char_box,
        loc: None,
    })
}

fn binrel_class(arg: &ParseNode) -> String {
    let atom = if let ParseNode::OrdGroup { body, .. } = arg {
        if !body.is_empty() { &body[0] } else { arg }
    } else {
        arg
    };

    if let ParseNode::Atom { family, .. } = atom {
        match family {
            crate::parse_node::AtomFamily::Bin => return "mbin".to_string(),
            crate::parse_node::AtomFamily::Rel => return "mrel".to_string(),
            _ => {}
        }
    }
    "mord".to_string()
}

fn handle_binrel(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut args = args.into_iter();
    let atom = args.next().unwrap();
    let body = args.next().unwrap();
    let mclass = binrel_class(&atom);

    Ok(ParseNode::MClass {
        mode: ctx.parser.mode,
        mclass,
        body: ParseNode::ord_argument(body),
        is_character_box: false,
        loc: None,
    })
}

fn handle_stackrel(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut args = args.into_iter();
    let shifted_arg = args.next().unwrap();
    let base_arg = args.next().unwrap();

    let mclass = if ctx.func_name == "\\stackrel" {
        "mrel".to_string()
    } else {
        binrel_class(&base_arg)
    };

    let op_node = ParseNode::Op {
        mode: ctx.parser.mode,
        limits: true,
        always_handle_sup_sub: Some(true),
        suppress_base_shift: Some(ctx.func_name != "\\stackrel"),
        parent_is_sup_sub: false,
        symbol: false,
        name: None,
        body: Some(ParseNode::ord_argument(base_arg)),
        loc: None,
    };

    let (sup, sub) = if ctx.func_name == "\\underset" {
        (None, Some(Box::new(shifted_arg)))
    } else {
        (Some(Box::new(shifted_arg)), None)
    };

    let supsub = ParseNode::SupSub {
        mode: ctx.parser.mode,
        base: Some(Box::new(op_node)),
        sup,
        sub,
        loc: None,
    };

    Ok(ParseNode::MClass {
        mode: ctx.parser.mode,
        mclass,
        body: vec![supsub],
        is_character_box: false,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &[
            "\\mathord",
            "\\mathbin",
            "\\mathrel",
            "\\mathopen",
            "\\mathclose",
            "\\mathpunct",
            "\\mathinner",
        ],
        FunctionSpec {
            node_type: "mclass",
            num_args: 1,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_mclass,
        },
    ),
    (
        &["\\stackrel", "\\overset", "\\underset"],
        FunctionSpec {
            node_type: "mclass",
            num_args: 2,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_stackrel,
        },
    ),
    (
        &["\\@binrel"],
        FunctionSpec {
            node_type: "mclass",
            num_args: 2,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: false,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_binrel,
        },
    ),
];
