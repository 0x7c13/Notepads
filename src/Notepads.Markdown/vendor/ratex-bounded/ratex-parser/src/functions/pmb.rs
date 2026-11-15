use std::collections::HashMap;

use crate::error::ParseResult;
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::ParseNode;

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\pmb"],
        "pmb",
        1,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_pmb,
    );
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

fn handle_pmb(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let body = args.into_iter().next().unwrap();

    Ok(ParseNode::Pmb {
        mode: ctx.parser.mode,
        mclass: binrel_class(&body),
        body: ParseNode::ord_argument(body),
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\pmb"],
    FunctionSpec {
        node_type: "pmb",
        num_args: 1,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: false,
        allowed_in_text: true,
        allowed_in_math: true,
        infix: false,
        primitive: false,
        handler: handle_pmb,
    },
)];
