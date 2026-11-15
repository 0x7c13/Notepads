// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use std::collections::HashMap;

use ratex_lexer::token::Token;

use crate::error::{ParseError, ParseResult};
use crate::macro_expander::MacroDefinition;
use crate::parse_node::{
    AlignSpec, AlignType, ArrayTag, Measurement, Mode, ParseNode, ProofBranch, ProofLineStyle,
    StyleStr,
};
use crate::parser::Parser;
use crate::stack_safety::parse_nodes_logical_depth;

// ── Environment registry ─────────────────────────────────────────────────

pub struct EnvContext<'a, 'b> {
    pub mode: Mode,
    pub env_name: String,
    pub parser: &'a mut Parser<'b>,
}

pub type EnvHandler = fn(
    ctx: &mut EnvContext,
    args: Vec<ParseNode>,
    opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode>;

pub struct EnvSpec {
    pub num_args: usize,
    pub num_optional_args: usize,
    pub handler: EnvHandler,
}

pub static ENVIRONMENTS: std::sync::LazyLock<HashMap<&'static str, EnvSpec>> =
    std::sync::LazyLock::new(|| {
        let mut map = HashMap::new();
        register_array(&mut map);
        register_matrix(&mut map);
        register_cases(&mut map);
        register_align(&mut map);
        register_gathered(&mut map);
        register_equation(&mut map);
        register_smallmatrix(&mut map);
        register_alignat(&mut map);
        register_subarray(&mut map);
        register_cd(&mut map);
        register_prooftree(&mut map);
        map
    });

// ── ArrayConfig ──────────────────────────────────────────────────────────

#[derive(Default)]
pub struct ArrayConfig {
    pub hskip_before_and_after: Option<bool>,
    pub add_jot: Option<bool>,
    pub cols: Option<Vec<AlignSpec>>,
    pub arraystretch: Option<f64>,
    pub col_separation_type: Option<String>,
    pub single_row: bool,
    pub empty_single_row: bool,
    pub max_num_cols: Option<usize>,
    pub leqno: Option<bool>,
    pub auto_number: bool,
}

// ── parseArray ───────────────────────────────────────────────────────────

/// Pull a trailing `\\tag{…}` or `\\nonumber`/`\\notag` off the last cell of a row.
/// Returns `Auto(true)` when the row is eligible for auto-numbering.
/// The `auto_number` parameter controls the default when no marker is found.
fn extract_trailing_tag_from_last_cell(
    row: &mut [ParseNode],
    auto_number: bool,
) -> ParseResult<ArrayTag> {
    let default_tag = if auto_number {
        ArrayTag::Auto(true)
    } else {
        ArrayTag::Auto(false)
    };
    let Some(last) = row.last_mut() else {
        return Ok(default_tag);
    };

    let inner: &mut ParseNode = match last {
        ParseNode::Styling { body, .. } => {
            if body.len() != 1 {
                return Ok(default_tag);
            }
            &mut body[0]
        }
        _ => last,
    };

    let obody = match inner {
        ParseNode::OrdGroup { body, .. } => body,
        _ => return Ok(default_tag),
    };

    // Look for \\tag
    let tag_indices: Vec<usize> = obody
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n, ParseNode::Tag { .. }))
        .map(|(i, _)| i)
        .collect();

    // Look for \\nonumber / \\notag
    let nonumber_indices: Vec<usize> = obody
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n, ParseNode::NoNumber { .. }))
        .map(|(i, _)| i)
        .collect();

    // Can't have both \\tag and \\nonumber in the same row
    if !tag_indices.is_empty() && !nonumber_indices.is_empty() {
        return Err(ParseError::msg(
            "Cannot use both \\tag and \\nonumber in the same row",
        ));
    }

    // Handle \\tag
    if !tag_indices.is_empty() {
        if tag_indices.len() > 1 {
            return Err(ParseError::msg("Multiple \\tag in a row"));
        }
        let idx = tag_indices[0];
        if idx != obody.len() - 1 {
            return Err(ParseError::msg(
                "\\tag must appear at the end of the row after the equation body",
            ));
        }
        match obody.pop() {
            Some(ParseNode::Tag { tag, .. }) => {
                if tag.is_empty() {
                    Ok(ArrayTag::Auto(false))
                } else {
                    Ok(ArrayTag::Explicit(tag))
                }
            }
            _ => Ok(default_tag),
        }
    } else if !nonumber_indices.is_empty() {
        // Handle \\nonumber / \\notag
        if nonumber_indices.len() > 1 {
            return Err(ParseError::msg("Multiple \\nonumber in a row"));
        }
        let idx = nonumber_indices[0];
        if idx != obody.len() - 1 {
            return Err(ParseError::msg(
                "\\nonumber must appear at the end of the row",
            ));
        }
        obody.pop(); // discard the NoNumber node
        Ok(ArrayTag::Auto(false))
    } else {
        // Neither \\tag nor \\nonumber
        Ok(default_tag)
    }
}

fn get_hlines(parser: &mut Parser) -> ParseResult<Vec<bool>> {
    let mut hline_info = Vec::new();
    parser.consume_spaces()?;

    let mut nxt = parser.fetch()?.text.clone();
    if nxt == "\\relax" {
        parser.consume();
        parser.consume_spaces()?;
        nxt = parser.fetch()?.text.clone();
    }
    while nxt == "\\hline" || nxt == "\\hdashline" {
        parser.consume();
        hline_info.push(nxt == "\\hdashline");
        parser.consume_spaces()?;
        nxt = parser.fetch()?.text.clone();
    }
    Ok(hline_info)
}

fn d_cell_style(env_name: &str) -> Option<StyleStr> {
    if env_name.starts_with('d') {
        Some(StyleStr::Display)
    } else {
        Some(StyleStr::Text)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoundedArrayEnvironment {
    Matrix(Option<(&'static str, &'static str)>),
    Cases,
    Aligned,
}

pub(crate) fn parse_bounded_array(parser: &mut Parser) -> ParseResult<ParseNode> {
    use ratex_types::execution::{Phase, Resource};
    use ratex_types::storage::StorageFamily;

    let environment = read_bounded_array_name(parser, false)?;
    let aligned = environment == BoundedArrayEnvironment::Aligned;
    let control = parser
        .gullet
        .execution_control()
        .ok_or_else(|| ParseError::msg("Array requires bounded parsing"))?;
    let storage = control
        .storage()
        .ok_or_else(|| ParseError::msg("Array requires bounded storage"))?;
    let mut body = Vec::new();
    let mut row = Vec::new();
    loop {
        control
            .consume(Phase::Parser, Resource::ParserSteps, 1)
            .map_err(ParseError::control)?;
        let mut contents = parser.parse_expression(false, Some("\\\\"))?;
        let delimiter = parser.fetch()?;
        let at_end = delimiter.text == "\\end";
        let empty_terminal_row = at_end && row.is_empty() && contents.is_empty();
        if !empty_terminal_row || (aligned && body.is_empty()) {
            if aligned && row.len() % 2 == 1 {
                // Make a leading relation or binary operator in the right-hand
                // cell space against an empty ordinary atom, as in amsmath.
                storage
                    .try_reserve(Phase::Parser, StorageFamily::Ast, &mut contents, 1)
                    .map_err(ParseError::storage)?;
                contents.insert(
                    0,
                    ParseNode::OrdGroup {
                        mode: Mode::Math,
                        body: Vec::new(),
                        semisimple: None,
                        loc: None,
                    },
                );
            }
            let cell = ParseNode::OrdGroup {
                mode: Mode::Math,
                body: contents,
                semisimple: None,
                loc: None,
            };
            let styled = ParseNode::Styling {
                mode: Mode::Math,
                style: if aligned {
                    StyleStr::Display
                } else {
                    StyleStr::Text
                },
                body: parser.values([cell])?,
                loc: None,
            };
            parser.pushed(&mut row, styled)?;
        }
        match delimiter.text.as_str() {
            "&" => parser.consume(),
            "\\\\" => {
                parser.consume();
                if parser.gullet.future().text == "[" {
                    return Err(parser.unsupported());
                }
                parser.pushed(&mut body, std::mem::take(&mut row))?;
            }
            "\\end" => {
                parser.consume();
                if read_bounded_array_name(parser, true)? != environment {
                    return Err(ParseError::msg("Mismatched array environment"));
                }
                if !row.is_empty() {
                    parser.pushed(&mut body, row)?;
                }
                break;
            }
            _ => return Err(ParseError::msg("Expected an array column, row, or end")),
        }
    }
    let array = ParseNode::Array {
        mode: Mode::Math,
        body,
        row_gaps: Vec::new(),
        hlines_before_row: Vec::new(),
        cols: None,
        col_separation_type: match environment {
            BoundedArrayEnvironment::Cases => Some(parser.text("cases")?),
            BoundedArrayEnvironment::Aligned => Some(parser.text("align")?),
            BoundedArrayEnvironment::Matrix(_) => None,
        },
        hskip_before_and_after: Some(false),
        add_jot: aligned.then_some(true),
        arraystretch: if environment == BoundedArrayEnvironment::Cases {
            1.2
        } else {
            1.0
        },
        tags: None,
        leqno: None,
        is_cd: None,
        loc: None,
    };
    let delimiters = match environment {
        BoundedArrayEnvironment::Matrix(delimiters) => delimiters,
        BoundedArrayEnvironment::Cases => Some(("\\{", ".")),
        BoundedArrayEnvironment::Aligned => None,
    };
    match delimiters {
        Some((left, right)) => Ok(ParseNode::LeftRight {
            mode: Mode::Math,
            body: parser.values([array])?,
            left: parser.text(left)?,
            right: parser.text(right)?,
            right_color: None,
            loc: None,
        }),
        None => Ok(array),
    }
}

fn read_bounded_array_name(
    parser: &mut Parser,
    closing: bool,
) -> ParseResult<BoundedArrayEnvironment> {
    use BoundedArrayEnvironment::{Aligned, Cases, Matrix};

    parser.expect("{", true)?;
    let wrong_name = |parser: &Parser<'_>| {
        if closing {
            ParseError::msg("Mismatched array environment")
        } else {
            parser.unsupported()
        }
    };
    let token = parser.fetch()?;
    let matrix_suffix = ["m", "a", "t", "r", "i", "x", "}"];
    let (environment, suffix): (_, &[_]) = match token.text.as_str() {
        "m" => (Matrix(None), &matrix_suffix[1..]),
        "p" => (Matrix(Some(("(", ")"))), &matrix_suffix),
        "b" => (Matrix(Some(("[", "]"))), &matrix_suffix),
        "B" => (Matrix(Some(("\\{", "\\}"))), &matrix_suffix),
        "v" => (Matrix(Some(("|", "|"))), &matrix_suffix),
        "V" => (Matrix(Some(("\\Vert", "\\Vert"))), &matrix_suffix),
        "c" => (Cases, &["a", "s", "e", "s", "}"]),
        "a" => (Aligned, &["l", "i", "g", "n", "e", "d", "}"]),
        "EOF" => return Err(ParseError::msg("Unterminated array environment name")),
        _ => return Err(wrong_name(parser)),
    };
    parser.consume();
    for &letter in suffix {
        let token = parser.fetch()?;
        if token.text == "EOF" {
            return Err(ParseError::msg("Unterminated array environment name"));
        }
        if token.text != letter {
            return Err(wrong_name(parser));
        }
        parser.consume();
    }
    Ok(environment)
}

pub fn parse_array(
    parser: &mut Parser,
    config: ArrayConfig,
    style: Option<StyleStr>,
) -> ParseResult<ParseNode> {
    parser.gullet.begin_group();

    if !config.single_row {
        parser.gullet.set_text_macro("\\cr", "\\\\\\relax");
    }

    let arraystretch = config.arraystretch.unwrap_or_else(|| {
        // Check if \arraystretch is defined as a macro (e.g., via \def\arraystretch{1.5})
        if let Some(def) = parser.gullet.get_macro("\\arraystretch") {
            let s = match def {
                MacroDefinition::Text(s) => s.clone(),
                MacroDefinition::Tokens { tokens, .. } => {
                    // Tokens are stored in reverse order (stack convention for expansion)
                    tokens
                        .iter()
                        .rev()
                        .map(|t| t.text.as_str())
                        .collect::<String>()
                }
                MacroDefinition::Function(_) => String::new(),
                MacroDefinition::StaticText(text) => (*text).to_string(),
            };
            s.parse::<f64>().unwrap_or(1.0)
        } else {
            1.0
        }
    });

    parser.gullet.begin_group();

    let mut row: Vec<ParseNode> = Vec::new();
    let mut body: Vec<Vec<ParseNode>> = Vec::new();
    let mut row_tags: Vec<ArrayTag> = Vec::new();
    let mut row_gaps: Vec<Option<Measurement>> = Vec::new();
    let mut hlines_before_row: Vec<Vec<bool>> = Vec::new();

    hlines_before_row.push(get_hlines(parser)?);

    loop {
        let break_token = if config.single_row { "\\end" } else { "\\\\" };
        let cell_body = parser.parse_expression(false, Some(break_token))?;
        parser.gullet.end_group();
        parser.gullet.begin_group();

        let mut cell = ParseNode::OrdGroup {
            mode: parser.mode,
            body: cell_body,
            semisimple: None,
            loc: None,
        };

        if let Some(s) = style {
            cell = ParseNode::Styling {
                mode: parser.mode,
                style: s,
                body: vec![cell],
                loc: None,
            };
        }

        row.push(cell.clone());
        let next = parser.fetch()?.text.clone();

        if next == "&" {
            if let Some(max) = config.max_num_cols {
                if row.len() >= max {
                    return Err(ParseError::msg("Too many tab characters: &"));
                }
            }
            parser.consume();
        } else if next == "\\end" {
            // Check for trailing empty row and remove it
            let is_empty_trailing = if let Some(s) = style {
                if s == StyleStr::Text || s == StyleStr::Display {
                    if let ParseNode::Styling { body: ref sb, .. } = cell {
                        if let Some(ParseNode::OrdGroup { body: ref ob, .. }) = sb.first() {
                            ob.is_empty()
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else if let ParseNode::OrdGroup { body: ref ob, .. } = cell {
                ob.is_empty()
            } else {
                false
            };

            let row_tag = extract_trailing_tag_from_last_cell(&mut row, config.auto_number)?;
            row_tags.push(row_tag);
            body.push(row);

            if is_empty_trailing && (body.len() > 1 || !config.empty_single_row) {
                body.pop();
                row_tags.pop();
            }

            if hlines_before_row.len() < body.len() + 1 {
                hlines_before_row.push(vec![]);
            }
            break;
        } else if next == "\\\\" {
            parser.consume();
            let size = if parser.gullet.future().text != " " {
                parser.parse_size_group(true)?
            } else {
                None
            };
            let gap = size.and_then(|s| {
                if let ParseNode::Size { value, .. } = s {
                    Some(value)
                } else {
                    None
                }
            });
            row_gaps.push(gap);

            let row_tag = extract_trailing_tag_from_last_cell(&mut row, config.auto_number)?;
            row_tags.push(row_tag);
            body.push(row);
            hlines_before_row.push(get_hlines(parser)?);
            row = Vec::new();
        } else {
            return Err(ParseError::msg(format!(
                "Expected & or \\\\ or \\cr or \\end, got '{}'",
                next
            )));
        }
    }

    parser.gullet.end_group();
    parser.gullet.end_group();

    // Post-process row tags for auto-numbering
    let tags = if config.auto_number {
        let mut processed: Vec<ArrayTag> = Vec::with_capacity(row_tags.len());
        let mut any_visible = false;
        for raw_tag in &row_tags {
            match raw_tag {
                ArrayTag::Explicit(nodes) if !nodes.is_empty() => {
                    // Explicit \\tag{...}: step counter, keep tag content as-is
                    parser.equation_counter += 1;
                    processed.push(ArrayTag::Explicit(nodes.clone()));
                    any_visible = true;
                }
                ArrayTag::Explicit(_) => {
                    // Empty \\tag{}: treat as suppressed
                    processed.push(ArrayTag::Auto(false));
                }
                ArrayTag::Auto(true) => {
                    // Auto-number this row: step counter, generate "(N)"
                    parser.equation_counter += 1;
                    let num_str = parser.equation_counter.to_string();
                    let tag_nodes = vec![
                        ParseNode::MathOrd {
                            mode: Mode::Math,
                            text: "(".to_string(),
                            loc: None,
                        },
                        ParseNode::MathOrd {
                            mode: Mode::Math,
                            text: num_str,
                            loc: None,
                        },
                        ParseNode::MathOrd {
                            mode: Mode::Math,
                            text: ")".to_string(),
                            loc: None,
                        },
                    ];
                    processed.push(ArrayTag::Explicit(tag_nodes));
                    any_visible = true;
                }
                ArrayTag::Auto(false) => {
                    // Suppressed by \\nonumber or empty \\tag{}: no counter step, no tag
                    processed.push(ArrayTag::Auto(false));
                }
            }
        }
        if any_visible { Some(processed) } else { None }
    } else {
        // Not an auto-numbering environment: keep original behavior
        if row_tags
            .iter()
            .any(|t| matches!(t, ArrayTag::Explicit(nodes) if !nodes.is_empty()))
        {
            Some(row_tags)
        } else {
            None
        }
    };

    Ok(ParseNode::Array {
        mode: parser.mode,
        body,
        row_gaps,
        hlines_before_row,
        cols: config.cols,
        col_separation_type: config.col_separation_type,
        hskip_before_and_after: config.hskip_before_and_after,
        add_jot: config.add_jot,
        arraystretch,
        tags,
        leqno: config.leqno,
        is_cd: None,
        loc: None,
    })
}

// ── array / darray ───────────────────────────────────────────────────────

fn register_array(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_array(
        ctx: &mut EnvContext,
        args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        let colalign = match &args[0] {
            ParseNode::OrdGroup { body, .. } => body.clone(),
            other if other.is_symbol_node() => vec![other.clone()],
            _ => return Err(ParseError::msg("Invalid column alignment for array")),
        };

        let mut cols = Vec::new();
        for nde in &colalign {
            let ca = nde
                .symbol_text()
                .ok_or_else(|| ParseError::msg("Expected column alignment character"))?;
            match ca {
                "l" | "c" | "r" => cols.push(AlignSpec {
                    align_type: AlignType::Align,
                    align: Some(ca.to_string()),
                    pregap: None,
                    postgap: None,
                }),
                "|" => cols.push(AlignSpec {
                    align_type: AlignType::Separator,
                    align: Some("|".to_string()),
                    pregap: None,
                    postgap: None,
                }),
                ":" => cols.push(AlignSpec {
                    align_type: AlignType::Separator,
                    align: Some(":".to_string()),
                    pregap: None,
                    postgap: None,
                }),
                _ => return Err(ParseError::msg(format!("Unknown column alignment: {}", ca))),
            }
        }

        let max_num_cols = cols.len();
        let config = ArrayConfig {
            cols: Some(cols),
            hskip_before_and_after: Some(true),
            max_num_cols: Some(max_num_cols),
            ..Default::default()
        };
        parse_array(ctx.parser, config, d_cell_style(&ctx.env_name))
    }

    for name in &["array", "darray"] {
        map.insert(
            name,
            EnvSpec {
                num_args: 1,
                num_optional_args: 0,
                handler: handle_array,
            },
        );
    }
}

// ── matrix variants ──────────────────────────────────────────────────────

fn register_matrix(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_matrix(
        ctx: &mut EnvContext,
        _args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        let base_name = ctx.env_name.replace('*', "");
        let delimiters: Option<(&str, &str)> = match base_name.as_str() {
            "matrix" => None,
            "pmatrix" => Some(("(", ")")),
            "bmatrix" => Some(("[", "]")),
            "Bmatrix" => Some(("\\{", "\\}")),
            "vmatrix" => Some(("|", "|")),
            "Vmatrix" => Some(("\\Vert", "\\Vert")),
            _ => None,
        };

        let mut col_align = "c".to_string();

        // mathtools starred matrix: parse optional [l|c|r] alignment
        if ctx.env_name.ends_with('*') {
            ctx.parser.gullet.consume_spaces();
            if ctx.parser.gullet.future().text == "[" {
                ctx.parser.gullet.pop_token();
                ctx.parser.gullet.consume_spaces();
                let align_tok = ctx.parser.gullet.pop_token();
                if !"lcr".contains(align_tok.text.as_str()) {
                    return Err(ParseError::new(
                        "Expected l or c or r".to_string(),
                        Some(&align_tok),
                    ));
                }
                col_align = align_tok.text.clone();
                ctx.parser.gullet.consume_spaces();
                let close = ctx.parser.gullet.pop_token();
                if close.text != "]" {
                    return Err(ParseError::new("Expected ]".to_string(), Some(&close)));
                }
            }
        }

        let config = ArrayConfig {
            hskip_before_and_after: Some(false),
            cols: Some(vec![AlignSpec {
                align_type: AlignType::Align,
                align: Some(col_align.clone()),
                pregap: None,
                postgap: None,
            }]),
            ..Default::default()
        };

        let mut res = parse_array(ctx.parser, config, d_cell_style(&ctx.env_name))?;

        // Fix cols to match actual number of columns
        if let ParseNode::Array {
            ref body,
            ref mut cols,
            ..
        } = res
        {
            let num_cols = body.iter().map(|r| r.len()).max().unwrap_or(0);
            *cols = Some(
                (0..num_cols)
                    .map(|_| AlignSpec {
                        align_type: AlignType::Align,
                        align: Some(col_align.to_string()),
                        pregap: None,
                        postgap: None,
                    })
                    .collect(),
            );
        }

        match delimiters {
            Some((left, right)) => Ok(ParseNode::LeftRight {
                mode: ctx.mode,
                body: vec![res],
                left: left.to_string(),
                right: right.to_string(),
                right_color: None,
                loc: None,
            }),
            None => Ok(res),
        }
    }

    for name in &[
        "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix", "matrix*", "pmatrix*",
        "bmatrix*", "Bmatrix*", "vmatrix*", "Vmatrix*",
    ] {
        map.insert(
            name,
            EnvSpec {
                num_args: 0,
                num_optional_args: 0,
                handler: handle_matrix,
            },
        );
    }
}

// ── cases / dcases / rcases / drcases ────────────────────────────────────

fn register_cases(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_cases(
        ctx: &mut EnvContext,
        _args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        let config = ArrayConfig {
            arraystretch: Some(1.2),
            cols: Some(vec![
                AlignSpec {
                    align_type: AlignType::Align,
                    align: Some("l".to_string()),
                    pregap: Some(0.0),
                    postgap: Some(1.0),
                },
                AlignSpec {
                    align_type: AlignType::Align,
                    align: Some("l".to_string()),
                    pregap: Some(0.0),
                    postgap: Some(0.0),
                },
            ]),
            ..Default::default()
        };

        let res = parse_array(ctx.parser, config, d_cell_style(&ctx.env_name))?;

        let (left, right) = if ctx.env_name.contains('r') {
            (".", "\\}")
        } else {
            ("\\{", ".")
        };

        Ok(ParseNode::LeftRight {
            mode: ctx.mode,
            body: vec![res],
            left: left.to_string(),
            right: right.to_string(),
            right_color: None,
            loc: None,
        })
    }

    for name in &["cases", "dcases", "rcases", "drcases"] {
        map.insert(
            name,
            EnvSpec {
                num_args: 0,
                num_optional_args: 0,
                handler: handle_cases,
            },
        );
    }
}

// ── align / align* / aligned / split / alignat / alignat* / alignedat ────

fn handle_aligned(
    ctx: &mut EnvContext,
    args: Vec<ParseNode>,
    _opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode> {
    let is_split = ctx.env_name == "split";
    let is_alignat = ctx.env_name.contains("at");
    let sep_type = if is_alignat { "alignat" } else { "align" };
    let auto_number = !ctx.env_name.ends_with('*')
        && !is_split
        && ctx.env_name != "aligned"
        && ctx.env_name != "alignedat";

    let config = ArrayConfig {
        add_jot: Some(true),
        empty_single_row: true,
        col_separation_type: Some(sep_type.to_string()),
        max_num_cols: if is_split { Some(2) } else { None },
        auto_number,
        ..Default::default()
    };

    let mut res = parse_array(ctx.parser, config, Some(StyleStr::Display))?;

    // Extract explicit column count from first arg (alignat only)
    let mut num_maths = 0usize;
    let mut explicit_cols = 0usize;
    if let Some(ParseNode::OrdGroup { body, .. }) = args.first() {
        let mut arg_str = String::new();
        for node in body {
            if let Some(t) = node.symbol_text() {
                arg_str.push_str(t);
            }
        }
        if let Ok(n) = arg_str.parse::<usize>() {
            num_maths = n;
            explicit_cols = n * 2;
        }
    }
    let is_aligned = explicit_cols == 0;

    // Determine actual number of columns
    let mut num_cols = if let ParseNode::Array { ref body, .. } = res {
        body.iter().map(|r| r.len()).max().unwrap_or(0)
    } else {
        0
    };

    if let ParseNode::Array {
        body: ref mut array_body,
        ..
    } = res
    {
        for row in array_body.iter_mut() {
            // Prepend empty group at every even-indexed cell (2nd, 4th, ...)
            let mut i = 1;
            while i < row.len() {
                if let ParseNode::Styling {
                    body: ref mut styling_body,
                    ..
                } = row[i]
                {
                    if let Some(ParseNode::OrdGroup {
                        body: ref mut og_body,
                        ..
                    }) = styling_body.first_mut()
                    {
                        og_body.insert(
                            0,
                            ParseNode::OrdGroup {
                                mode: ctx.mode,
                                body: vec![],
                                semisimple: None,
                                loc: None,
                            },
                        );
                    }
                }
                i += 2;
            }

            if !is_aligned {
                let cur_maths = row.len() / 2;
                if num_maths < cur_maths {
                    return Err(ParseError::msg(format!(
                        "Too many math in a row: expected {}, but got {}",
                        num_maths, cur_maths
                    )));
                }
            } else if num_cols < row.len() {
                num_cols = row.len();
            }
        }
    }

    if !is_aligned {
        num_cols = explicit_cols;
    }

    let mut cols = Vec::new();
    for i in 0..num_cols {
        let (align, pregap) = if i % 2 == 1 {
            ("l", 0.0)
        } else if i > 0 && is_aligned {
            ("r", 1.0)
        } else {
            ("r", 0.0)
        };
        cols.push(AlignSpec {
            align_type: AlignType::Align,
            align: Some(align.to_string()),
            pregap: Some(pregap),
            postgap: Some(0.0),
        });
    }

    if let ParseNode::Array {
        cols: ref mut array_cols,
        col_separation_type: ref mut array_sep_type,
        ..
    } = res
    {
        *array_cols = Some(cols);
        *array_sep_type = Some(if is_aligned { "align" } else { "alignat" }.to_string());
    }

    Ok(res)
}

fn register_align(map: &mut HashMap<&'static str, EnvSpec>) {
    for name in &["align", "align*", "aligned", "split"] {
        map.insert(
            name,
            EnvSpec {
                num_args: 0,
                num_optional_args: 0,
                handler: handle_aligned,
            },
        );
    }
}

// ── gathered / gather / gather* ──────────────────────────────────────────

fn register_gathered(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_gathered(
        ctx: &mut EnvContext,
        _args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        let auto_number = !ctx.env_name.ends_with('*') && ctx.env_name != "gathered";
        let config = ArrayConfig {
            cols: Some(vec![AlignSpec {
                align_type: AlignType::Align,
                align: Some("c".to_string()),
                pregap: None,
                postgap: None,
            }]),
            add_jot: Some(true),
            col_separation_type: Some("gather".to_string()),
            empty_single_row: true,
            auto_number,
            ..Default::default()
        };
        parse_array(ctx.parser, config, Some(StyleStr::Display))
    }

    for name in &["gathered", "gather", "gather*"] {
        map.insert(
            name,
            EnvSpec {
                num_args: 0,
                num_optional_args: 0,
                handler: handle_gathered,
            },
        );
    }
}

// ── equation / equation* ─────────────────────────────────────────────────

fn register_equation(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_equation(
        ctx: &mut EnvContext,
        _args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        let auto_number = !ctx.env_name.ends_with('*');
        let config = ArrayConfig {
            empty_single_row: true,
            single_row: true,
            max_num_cols: Some(1),
            auto_number,
            ..Default::default()
        };
        parse_array(ctx.parser, config, Some(StyleStr::Display))
    }

    for name in &["equation", "equation*"] {
        map.insert(
            name,
            EnvSpec {
                num_args: 0,
                num_optional_args: 0,
                handler: handle_equation,
            },
        );
    }
}

// ── smallmatrix ──────────────────────────────────────────────────────────

fn register_smallmatrix(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_smallmatrix(
        ctx: &mut EnvContext,
        _args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        let config = ArrayConfig {
            arraystretch: Some(0.5),
            ..Default::default()
        };
        let mut res = parse_array(ctx.parser, config, Some(StyleStr::Script))?;
        if let ParseNode::Array {
            ref mut col_separation_type,
            ..
        } = res
        {
            *col_separation_type = Some("small".to_string());
        }
        Ok(res)
    }

    map.insert(
        "smallmatrix",
        EnvSpec {
            num_args: 0,
            num_optional_args: 0,
            handler: handle_smallmatrix,
        },
    );
}

// ── alignat / alignat* / alignedat ──────────────────────────────────────

fn register_alignat(map: &mut HashMap<&'static str, EnvSpec>) {
    for name in &["alignat", "alignat*", "alignedat"] {
        map.insert(
            name,
            EnvSpec {
                num_args: 1,
                num_optional_args: 0,
                handler: handle_aligned,
            },
        );
    }
}

// ── CD (amscd commutative diagrams) ──────────────────────────────────────

fn register_cd(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_cd(
        ctx: &mut EnvContext,
        _args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        // Collect all raw tokens until \end
        let mut raw: Vec<Token> = Vec::new();
        loop {
            let tok = ctx.parser.gullet.future().clone();
            if tok.text == "\\end" || tok.text == "EOF" {
                break;
            }
            ctx.parser.gullet.pop_token();
            raw.push(tok);
        }

        // Split into rows at \\ or \cr
        let rows = cd_split_rows(raw);

        let mut body: Vec<Vec<ParseNode>> = Vec::new();
        let mut row_gaps: Vec<Option<Measurement>> = Vec::new();
        let mut hlines_before_row: Vec<Vec<bool>> = Vec::new();
        hlines_before_row.push(vec![]);

        for row_toks in rows {
            // Skip purely-whitespace rows
            if row_toks.iter().all(|t| t.text == " ") {
                continue;
            }
            let cells = cd_parse_row(ctx.parser, row_toks)?;
            if !cells.is_empty() {
                body.push(cells);
                row_gaps.push(None);
                hlines_before_row.push(vec![]);
            }
        }

        if body.is_empty() {
            body.push(vec![]);
            hlines_before_row.push(vec![]);
        }

        Ok(ParseNode::Array {
            mode: ctx.mode,
            body,
            row_gaps,
            hlines_before_row,
            cols: None,
            col_separation_type: Some("CD".to_string()),
            hskip_before_and_after: Some(false),
            add_jot: None,
            arraystretch: 1.0,
            tags: None,
            leqno: None,
            is_cd: Some(true),
            loc: None,
        })
    }

    map.insert(
        "CD",
        EnvSpec {
            num_args: 0,
            num_optional_args: 0,
            handler: handle_cd,
        },
    );
}

/// Split a flat token list into rows at `\\` or `\cr` boundaries.
fn cd_split_rows(tokens: Vec<Token>) -> Vec<Vec<Token>> {
    let mut rows: Vec<Vec<Token>> = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    for tok in tokens {
        if tok.text == "\\\\" || tok.text == "\\cr" {
            rows.push(current);
            current = Vec::new();
        } else {
            current.push(tok);
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

/// Collect tokens from `tokens[start..]` up to (but not including) the first
/// token whose text equals `delimiter`.  Returns (collected_tokens, tokens_consumed).
/// `tokens_consumed` includes the delimiter itself if found.
fn cd_collect_until(tokens: &[Token], start: usize, delimiter: &str) -> (Vec<Token>, usize) {
    let mut result = Vec::new();
    let mut i = start;
    while i < tokens.len() {
        if tokens[i].text == delimiter {
            i += 1; // consume the delimiter
            break;
        }
        result.push(tokens[i].clone());
        i += 1;
    }
    (result, i - start)
}

/// Collect tokens from `tokens[start..]` up to (but not including) the next `@`.
fn cd_collect_until_at(tokens: &[Token], start: usize) -> (Vec<Token>, usize) {
    let mut result = Vec::new();
    let mut i = start;
    while i < tokens.len() && tokens[i].text != "@" {
        result.push(tokens[i].clone());
        i += 1;
    }
    (result, i - start)
}

/// Use the parser to parse a token slice as a math OrdGroup.
/// Tokens must be in forward order; this function reverses them internally for subparse().
fn cd_parse_tokens(parser: &mut Parser, tokens: Vec<Token>) -> ParseResult<ParseNode> {
    // Filter pure whitespace
    let has_content = tokens.iter().any(|t| t.text != " ");
    if !has_content {
        return Ok(ParseNode::OrdGroup {
            mode: parser.mode,
            body: vec![],
            semisimple: None,
            loc: None,
        });
    }
    // subparse() expects tokens in reverse order (stack convention)
    let mut rev = tokens;
    rev.reverse();
    let body = parser.subparse(rev)?;
    Ok(ParseNode::OrdGroup {
        mode: parser.mode,
        body,
        semisimple: None,
        loc: None,
    })
}

/// Parse one row of a CD environment from its raw token list.
/// Returns the list of ParseNode cells for the grid row.
fn cd_parse_row(parser: &mut Parser, row_tokens: Vec<Token>) -> ParseResult<Vec<ParseNode>> {
    let toks = &row_tokens;
    let n = toks.len();
    let mut cells: Vec<ParseNode> = Vec::new();
    let mut i = 0usize;

    while i < n {
        // Skip spaces at start of each cell
        while i < n && toks[i].text == " " {
            i += 1;
        }
        if i >= n {
            break;
        }

        if toks[i].text == "@" {
            i += 1; // consume `@`
            if i >= n {
                return Err(ParseError::msg("Unexpected end of CD row after @"));
            }
            let dir = toks[i].text.clone();
            i += 1; // consume direction char

            let mode = parser.mode;
            let arrow = match dir.as_str() {
                ">" | "<" => {
                    let (above_toks, c1) = cd_collect_until(toks, i, &dir);
                    i += c1;
                    let (below_toks, c2) = cd_collect_until(toks, i, &dir);
                    i += c2;
                    let label_above = cd_parse_tokens(parser, above_toks)?;
                    let label_below = cd_parse_tokens(parser, below_toks)?;
                    ParseNode::CdArrow {
                        mode,
                        direction: if dir == ">" { "right" } else { "left" }.to_string(),
                        label_above: Some(Box::new(label_above)),
                        label_below: Some(Box::new(label_below)),
                        loc: None,
                    }
                }
                "V" | "A" => {
                    let (left_toks, c1) = cd_collect_until(toks, i, &dir);
                    i += c1;
                    let (right_toks, c2) = cd_collect_until(toks, i, &dir);
                    i += c2;
                    let label_above = cd_parse_tokens(parser, left_toks)?;
                    let label_below = cd_parse_tokens(parser, right_toks)?;
                    ParseNode::CdArrow {
                        mode,
                        direction: if dir == "V" { "down" } else { "up" }.to_string(),
                        label_above: Some(Box::new(label_above)),
                        label_below: Some(Box::new(label_below)),
                        loc: None,
                    }
                }
                "=" => ParseNode::CdArrow {
                    mode,
                    direction: "horiz_eq".to_string(),
                    label_above: None,
                    label_below: None,
                    loc: None,
                },
                "|" => ParseNode::CdArrow {
                    mode,
                    direction: "vert_eq".to_string(),
                    label_above: None,
                    label_below: None,
                    loc: None,
                },
                "." => ParseNode::CdArrow {
                    mode,
                    direction: "none".to_string(),
                    label_above: None,
                    label_below: None,
                    loc: None,
                },
                _ => return Err(ParseError::msg(format!("Unknown CD directive: @{}", dir))),
            };
            cells.push(arrow);
        } else {
            // Object cell: collect until next `@`
            let (obj_toks, consumed) = cd_collect_until_at(toks, i);
            i += consumed;
            let obj = cd_parse_tokens(parser, obj_toks)?;
            cells.push(obj);
        }
    }

    // Post-process: structure cells into the (2n-1) grid pattern.
    Ok(cd_structure_row(cells, parser.mode))
}

/// Given the raw parsed cells of one CD row, produce the correctly-structured grid row.
///
/// Object rows already alternate: obj, h-arrow, obj, h-arrow, …, obj.
/// Arrow rows contain only CdArrow nodes (plus whitespace OrdGroups which we strip),
/// and need empty OrdGroup fillers inserted between consecutive arrows.
fn cd_structure_row(cells: Vec<ParseNode>, mode: Mode) -> Vec<ParseNode> {
    // Detect arrow row: all cells are either CdArrow or empty OrdGroup
    let is_arrow_row = cells.iter().all(|c| match c {
        ParseNode::CdArrow { .. } => true,
        ParseNode::OrdGroup { body, .. } => body.is_empty(),
        _ => false,
    }) && cells.iter().any(|c| matches!(c, ParseNode::CdArrow { .. }));

    if is_arrow_row {
        let arrows: Vec<ParseNode> = cells
            .into_iter()
            .filter(|c| matches!(c, ParseNode::CdArrow { .. }))
            .collect();

        if arrows.is_empty() {
            return vec![];
        }

        let empty = || ParseNode::OrdGroup {
            mode,
            body: vec![],
            semisimple: None,
            loc: None,
        };

        let mut result = Vec::with_capacity(arrows.len() * 2 - 1);
        for (idx, arrow) in arrows.into_iter().enumerate() {
            if idx > 0 {
                result.push(empty());
            }
            result.push(arrow);
        }
        result
    } else {
        // Object row: already in correct format
        cells
    }
}

// ── subarray ────────────────────────────────────────────────────────────

fn register_subarray(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_subarray(
        ctx: &mut EnvContext,
        args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        let colalign = match &args[0] {
            ParseNode::OrdGroup { body, .. } => body.clone(),
            other if other.is_symbol_node() => vec![other.clone()],
            _ => return Err(ParseError::msg("Invalid column alignment for subarray")),
        };

        let mut cols = Vec::new();
        for nde in &colalign {
            let ca = nde
                .symbol_text()
                .ok_or_else(|| ParseError::msg("Expected column alignment character"))?;
            match ca {
                "l" | "c" => cols.push(AlignSpec {
                    align_type: AlignType::Align,
                    align: Some(ca.to_string()),
                    pregap: None,
                    postgap: None,
                }),
                _ => return Err(ParseError::msg(format!("Unknown column alignment: {}", ca))),
            }
        }

        if cols.len() > 1 {
            return Err(ParseError::msg("{subarray} can contain only one column"));
        }

        let config = ArrayConfig {
            cols: Some(cols),
            hskip_before_and_after: Some(false),
            arraystretch: Some(0.5),
            ..Default::default()
        };

        let res = parse_array(ctx.parser, config, Some(StyleStr::Script))?;

        if let ParseNode::Array { ref body, .. } = res {
            if !body.is_empty() && body[0].len() > 1 {
                return Err(ParseError::msg("{subarray} can contain only one column"));
            }
        }

        Ok(res)
    }

    map.insert(
        "subarray",
        EnvSpec {
            num_args: 1,
            num_optional_args: 0,
            handler: handle_subarray,
        },
    );
}

// ── prooftree (bussproofs subset) ───────────────────────────────────────
//
// Implemented: \AxiomC, \UnaryInfC, \BinaryInfC, \TrinaryInfC, \QuaternaryInfC,
//   \QuinaryInfC (and their abbreviations \AXC, \UIC, \BIC, \TIC),
//   \LeftLabel/\RightLabel (and \LL, \RL), \fCenter,
//   line styles: \solidLine, \dashedLine, \noLine (and \always* variants),
//   \rootAtTop, \rootAtBottom (and \always* variants).
//
// Not yet implemented: \InsertBetweenHyps, \ScoreTree, \Cell, \noCell.

fn register_prooftree(map: &mut HashMap<&'static str, EnvSpec>) {
    fn handle_prooftree(
        ctx: &mut EnvContext,
        _args: Vec<ParseNode>,
        _opt_args: Vec<Option<ParseNode>>,
    ) -> ParseResult<ParseNode> {
        parse_prooftree(ctx.parser)
    }

    map.insert(
        "prooftree",
        EnvSpec {
            num_args: 0,
            num_optional_args: 0,
            handler: handle_prooftree,
        },
    );
}

fn proof_command_arity(name: &str) -> Option<usize> {
    match name {
        "\\UnaryInfC" | "\\UnaryInf" | "\\UIC" => Some(1),
        "\\BinaryInfC" | "\\BinaryInf" | "\\BIC" => Some(2),
        "\\TrinaryInfC" | "\\TrinaryInf" | "\\TIC" => Some(3),
        "\\QuaternaryInfC" | "\\QuaternaryInf" => Some(4),
        "\\QuinaryInfC" | "\\QuinaryInf" => Some(5),
        _ => None,
    }
}

fn parse_prooftree_arg(parser: &mut Parser, command: &str) -> ParseResult<Vec<ParseNode>> {
    let arg = parser
        .parse_argument_group(false, None)?
        .ok_or_else(|| ParseError::msg(format!("Expected argument for {}", command)))?;
    Ok(ParseNode::ord_argument(arg))
}

fn parse_prooftree(parser: &mut Parser) -> ParseResult<ParseNode> {
    // Track structural depth alongside each branch.  A prooftree is assembled
    // iteratively, so expression recursion alone cannot protect this tree shape.
    let mut stack: Vec<(ProofBranch, usize)> = Vec::new();
    let mut left_label: Option<Vec<ParseNode>> = None;
    let mut right_label: Option<Vec<ParseNode>> = None;
    let mut next_line_style = ProofLineStyle::Solid;
    let mut default_line_style = ProofLineStyle::Solid;
    let mut next_root_at_top = false;
    let mut default_root_at_top = false;

    loop {
        parser.consume_spaces()?;
        let token = parser.fetch()?;
        let command = token.text.clone();

        if command == "\\end" {
            break;
        }
        parser.consume();

        match command.as_str() {
            "\\AxiomC" | "\\Axiom" | "\\AXC" => {
                let conclusion = parse_prooftree_arg(parser, &command)?;
                let depth = 1 + parse_nodes_logical_depth(&conclusion);
                parser
                    .depth_budget()
                    .ensure_additional(depth)
                    .map_err(|_| ParseError::recursion_limit_exceeded())?;
                stack.push((
                    ProofBranch {
                        conclusion,
                        premises: Vec::new(),
                        left_label: None,
                        right_label: None,
                        line_style: ProofLineStyle::None,
                        root_at_top: false,
                    },
                    depth,
                ));
            }
            "\\LeftLabel" | "\\LL" => {
                left_label = Some(parse_prooftree_arg(parser, &command)?);
            }
            "\\RightLabel" | "\\RL" => {
                right_label = Some(parse_prooftree_arg(parser, &command)?);
            }
            "\\singleLine" | "\\solidLine" => {
                next_line_style = ProofLineStyle::Solid;
            }
            "\\dashedLine" => {
                next_line_style = ProofLineStyle::Dashed;
            }
            "\\noLine" => {
                next_line_style = ProofLineStyle::None;
            }
            "\\alwaysSingleLine" | "\\alwaysSolidLine" => {
                default_line_style = ProofLineStyle::Solid;
                next_line_style = ProofLineStyle::Solid;
            }
            "\\alwaysDashedLine" => {
                default_line_style = ProofLineStyle::Dashed;
                next_line_style = ProofLineStyle::Dashed;
            }
            "\\alwaysNoLine" => {
                default_line_style = ProofLineStyle::None;
                next_line_style = ProofLineStyle::None;
            }
            "\\rootAtTop" => {
                next_root_at_top = true;
            }
            "\\rootAtBottom" => {
                next_root_at_top = false;
            }
            "\\alwaysRootAtTop" => {
                default_root_at_top = true;
                next_root_at_top = true;
            }
            "\\alwaysRootAtBottom" => {
                default_root_at_top = false;
                next_root_at_top = false;
            }
            name if proof_command_arity(name).is_some() => {
                let arity = proof_command_arity(name).unwrap();
                if stack.len() < arity {
                    return Err(ParseError::msg(format!(
                        "{} needs {} premise(s), but only {} available",
                        name,
                        arity,
                        stack.len()
                    )));
                }
                let conclusion = parse_prooftree_arg(parser, name)?;
                let start = stack.len() - arity;
                let premise_entries = stack.split_off(start);
                let payload_depth = parse_nodes_logical_depth(&conclusion)
                    .max(
                        left_label
                            .as_deref()
                            .map(parse_nodes_logical_depth)
                            .unwrap_or(0),
                    )
                    .max(
                        right_label
                            .as_deref()
                            .map(parse_nodes_logical_depth)
                            .unwrap_or(0),
                    );
                let premise_depth = premise_entries
                    .iter()
                    .map(|(_, depth)| *depth)
                    .max()
                    .unwrap_or(0);
                let depth = 1 + payload_depth.max(premise_depth);
                parser
                    .depth_budget()
                    .ensure_additional(depth)
                    .map_err(|_| ParseError::recursion_limit_exceeded())?;
                let premises = premise_entries
                    .into_iter()
                    .map(|(branch, _)| branch)
                    .collect();
                stack.push((
                    ProofBranch {
                        conclusion,
                        premises,
                        left_label: left_label.take(),
                        right_label: right_label.take(),
                        line_style: next_line_style.clone(),
                        root_at_top: next_root_at_top,
                    },
                    depth,
                ));
                next_line_style = default_line_style.clone();
                next_root_at_top = default_root_at_top;
            }
            _ => {
                return Err(ParseError::msg(format!(
                    "{} valid only as a supported bussproofs command within prooftree",
                    command
                )));
            }
        }
    }

    if stack.len() != 1 {
        return Err(ParseError::msg(format!(
            "prooftree ended with {} proof stack item(s), expected 1",
            stack.len()
        )));
    }

    if left_label.is_some() || right_label.is_some() {
        return Err(ParseError::msg(
            "prooftree has a \\LeftLabel or \\RightLabel without a following inference command",
        ));
    }

    Ok(ParseNode::ProofTree {
        mode: parser.mode,
        tree: stack.pop().unwrap().0,
        loc: None,
    })
}
