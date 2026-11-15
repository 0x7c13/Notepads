use std::collections::HashMap;

use crate::error::{ParseError, ParseResult};
use crate::functions::{FunctionContext, FunctionSpec, define_function_full};
use crate::macro_expander::MacroDefinition;
use crate::parse_node::ParseNode;

static GLOBAL_MAP: &[(&str, &str)] = &[
    ("\\global", "\\global"),
    ("\\long", "\\\\globallong"),
    ("\\\\globallong", "\\\\globallong"),
    ("\\def", "\\gdef"),
    ("\\gdef", "\\gdef"),
    ("\\edef", "\\xdef"),
    ("\\xdef", "\\xdef"),
    ("\\let", "\\\\globallet"),
    ("\\futurelet", "\\\\globalfuture"),
];

fn global_version(name: &str) -> Option<&'static str> {
    GLOBAL_MAP.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn check_control_sequence(text: &str) -> ParseResult<()> {
    if matches!(text, "\\" | "{" | "}" | "$" | "&" | "#" | "^" | "_" | "EOF") {
        return Err(ParseError::msg("Expected a control sequence"));
    }
    Ok(())
}

pub fn register(map: &mut HashMap<&'static str, FunctionSpec>) {
    // Prefix commands: \global, \long
    define_function_full(
        map,
        &["\\global", "\\long", "\\\\globallong"],
        "internal",
        0,
        0,
        None,
        false,
        true,
        true,
        false,
        false,
        handle_prefix,
    );

    // \def, \gdef, \edef, \xdef
    define_function_full(
        map,
        &["\\def", "\\gdef", "\\edef", "\\xdef"],
        "internal",
        0,
        0,
        None,
        false,
        true,
        true,
        false,
        true,
        handle_def,
    );

    // \let
    define_function_full(
        map,
        &["\\let", "\\\\globallet"],
        "internal",
        0,
        0,
        None,
        false,
        true,
        true,
        false,
        true,
        handle_let,
    );

    // \futurelet
    define_function_full(
        map,
        &["\\futurelet", "\\\\globalfuture"],
        "internal",
        0,
        0,
        None,
        false,
        true,
        true,
        false,
        true,
        handle_futurelet,
    );
}

fn handle_prefix(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let mut force_global = matches!(ctx.func_name.as_str(), "\\global" | "\\\\globallong");

    loop {
        ctx.parser.consume_spaces()?;
        let mut next_tok = ctx.parser.fetch()?.clone();
        let token_text = next_tok.text.clone();

        match token_text.as_str() {
            "\\global" => {
                force_global = true;
                ctx.parser.consume();
            }
            "\\long" => {
                ctx.parser.consume();
            }
            "\\\\globallong" => {
                force_global = true;
                ctx.parser.consume();
            }
            _ => {
                if let Some(global_ver) = global_version(&token_text) {
                    if force_global {
                        next_tok.text = global_ver.to_string();
                        ctx.parser.gullet.push_token(next_tok);
                        ctx.parser.consume();
                    }
                    let result = ctx.parser.parse_function(None, None)?;
                    return match result {
                        Some(node) => Ok(node),
                        None => Err(ParseError::msg("Invalid token after macro prefix")),
                    };
                }
                return Err(ParseError::msg("Invalid token after macro prefix"));
            }
        }
    }
}

fn handle_def(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let name_tok = ctx.parser.gullet.pop_token();
    let name = name_tok.text.clone();
    check_control_sequence(&name)?;

    let mut num_args = 0usize;
    let mut insert_brace = false;

    // Parse parameter text: read tokens until `{`
    while ctx.parser.gullet.future().text != "{" {
        let tok = ctx.parser.gullet.pop_token();
        if tok.text == "#" {
            if ctx.parser.gullet.future().text == "{" {
                insert_brace = true;
                break;
            }
            let num_tok = ctx.parser.gullet.pop_token();
            let n: usize = num_tok.text.parse().map_err(|_| {
                ParseError::msg(format!("Invalid argument number \"{}\"", num_tok.text))
            })?;
            if n != num_args + 1 {
                return Err(ParseError::msg(format!(
                    "Argument number \"{}\" out of order",
                    n
                )));
            }
            num_args += 1;
        } else if tok.is_eof() {
            return Err(ParseError::msg("Expected a macro definition"));
        }
        // Delimiter tokens between parameters are consumed but not stored
        // (simplified: we don't support delimited macros with inter-parameter text)
    }

    let arg = ctx.parser.gullet.consume_arg(None)?;
    let mut tokens = arg.tokens;

    if insert_brace {
        let brace_tok = ratex_lexer::token::Token::new("{", 0, 0);
        tokens.push(brace_tok);
    }

    if ctx.func_name == "\\edef" || ctx.func_name == "\\xdef" {
        tokens.reverse();
        tokens = ctx.parser.gullet.expand_tokens(tokens)?;
    }

    let is_global_def = ctx.func_name == "\\gdef" || ctx.func_name == "\\xdef";
    let def = MacroDefinition::Tokens { tokens, num_args };
    if is_global_def {
        ctx.parser.gullet.set_macro_global(name, def);
    } else {
        ctx.parser.gullet.set_macro(name, def);
    }

    Ok(ParseNode::Internal {
        mode: ctx.parser.mode,
        loc: None,
    })
}

fn handle_let(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let name_tok = ctx.parser.gullet.pop_token();
    let name = name_tok.text.clone();
    check_control_sequence(&name)?;

    ctx.parser.gullet.consume_spaces();

    // Consume optional `=`
    let mut tok = ctx.parser.gullet.pop_token();
    if tok.text == "=" {
        tok = ctx.parser.gullet.pop_token();
        if tok.text == " " {
            tok = ctx.parser.gullet.pop_token();
        }
    }

    let is_global = ctx.func_name == "\\\\globallet";
    let_command(ctx, &name, tok, is_global);

    Ok(ParseNode::Internal {
        mode: ctx.parser.mode,
        loc: None,
    })
}

fn handle_futurelet(
    ctx: &mut FunctionContext,
    _args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let name_tok = ctx.parser.gullet.pop_token();
    let name = name_tok.text.clone();
    check_control_sequence(&name)?;

    let middle = ctx.parser.gullet.pop_token();
    let tok = ctx.parser.gullet.pop_token();

    let is_global = ctx.func_name == "\\\\globalfuture";
    let_command(ctx, &name, tok.clone(), is_global);

    ctx.parser.gullet.push_token(tok);
    ctx.parser.gullet.push_token(middle);

    Ok(ParseNode::Internal {
        mode: ctx.parser.mode,
        loc: None,
    })
}

fn let_command(
    ctx: &mut FunctionContext,
    name: &str,
    mut tok: ratex_lexer::token::Token,
    global: bool,
) {
    if ctx.parser.gullet.alias_macro(name, &tok.text, global) {
        return;
    }
    tok.noexpand = true;
    let def = MacroDefinition::Tokens {
        tokens: vec![tok],
        num_args: 0,
    };

    if global {
        ctx.parser.gullet.set_macro_global(name.to_string(), def);
    } else {
        ctx.parser.gullet.set_macro(name.to_string(), def);
    }
}

// Notepads Markdown fixed registry view: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Generated from original registration expressions; functions are not called at runtime.
pub(super) static BOUNDED_REGISTRATIONS: &[(&[&str], FunctionSpec)] = &[
    (
        &["\\global", "\\long", "\\\\globallong"],
        FunctionSpec {
            node_type: "internal",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: false,
            handler: handle_prefix,
        },
    ),
    (
        &["\\def", "\\gdef", "\\edef", "\\xdef"],
        FunctionSpec {
            node_type: "internal",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_def,
        },
    ),
    (
        &["\\let", "\\\\globallet"],
        FunctionSpec {
            node_type: "internal",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_let,
        },
    ),
    (
        &["\\futurelet", "\\\\globalfuture"],
        FunctionSpec {
            node_type: "internal",
            num_args: 0,
            num_optional_args: 0,
            arg_types: None,
            allowed_in_argument: false,
            allowed_in_text: true,
            allowed_in_math: true,
            infix: false,
            primitive: true,
            handler: handle_futurelet,
        },
    ),
];
