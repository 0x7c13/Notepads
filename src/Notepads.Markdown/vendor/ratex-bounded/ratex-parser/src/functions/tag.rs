use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::parse_node::{Mode, ParseNode};

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    define_function_full(
        map,
        &["\\tag"],
        "tag",
        0,
        0,
        None,
        true,
        false,
        true,
        false,
        true,
        handle_tag,
    );
}

fn handle_tag(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    ctx.parser.consume_spaces()?;
    let star = if ctx.parser.fetch()?.text == "*" {
        ctx.parser.consume();
        true
    } else {
        false
    };

    let arg = ctx
        .parser
        .parse_structural_group("\\tag", None)?
        .ok_or_else(|| ParseError::msg("\\tag requires an argument"))?;

    let inner = match arg {
        ParseNode::OrdGroup { body, .. } => body,
        other => vec![other],
    };

    let tag = if star {
        inner
    } else {
        let mut v = Vec::with_capacity(inner.len() + 2);
        v.push(ParseNode::MathOrd {
            mode: Mode::Math,
            text: "(".to_string(),
            loc: None,
        });
        v.extend(inner);
        v.push(ParseNode::MathOrd {
            mode: Mode::Math,
            text: ")".to_string(),
            loc: None,
        });
        v
    };

    Ok(ParseNode::Tag {
        mode: ctx.parser.mode,
        body: vec![],
        tag,
        loc: None,
    })
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[(
    &["\\tag"],
    FunctionSpec {
        node_type: "tag",
        num_args: 0,
        num_optional_args: 0,
        arg_types: None,
        allowed_in_argument: true,
        allowed_in_text: false,
        allowed_in_math: true,
        infix: false,
        primitive: true,
        handler: handle_tag,
    },
)];
