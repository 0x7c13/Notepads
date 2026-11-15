// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use ratex_lexer::token::{SourceLocation, Token};
use ratex_types::execution::{ExecutionControl, ExecutionError, Phase, Resource};
use unicode_normalization::UnicodeNormalization;

use crate::error::{ParseError, ParseResult};
use crate::functions::{self, ArgType, FUNCTIONS, FunctionContext};
use crate::macro_expander::{IMPLICIT_COMMANDS, MacroExpander};
use crate::parse_node::{AtomFamily, Mode, ParseNode};
use crate::stack_safety::{DepthBudget, MAX_INPUT_DEPTH, validate_parse_nodes};

/// End-of-expression tokens.
static END_OF_EXPRESSION: &[&str] = &["}", "\\endgroup", "\\end", "\\right", "&"];

/// Maximum supported nesting for input-controlled recursive structures.
///
/// Keep this conservative: parsing is used from mobile and WebAssembly hosts whose
/// worker stacks can be substantially smaller than a desktop main-thread stack.
/// The LaTeX parser. Converts a token stream into a ParseNode AST.
///
/// Follows KaTeX's Parser.ts closely:
/// - `parse()` → parse full expression
/// - `parseExpression()` → parse a list of atoms
/// - `parseAtom()` → parse one atom with optional super/subscripts
/// - `parseGroup()` → parse a group (braced or single token)
/// - `parseFunction()` → parse a function call with arguments
/// - `parseSymbol()` → parse a single symbol
pub struct Parser<'a> {
    pub mode: Mode,
    pub gullet: MacroExpander<'a>,
    pub leftright_depth: i32,
    depth_budget: DepthBudget,
    next_token: Option<Token>,
    chinese_text_argument: bool,
    pub equation_counter: usize,
}

impl<'a> Parser<'a> {
    pub(crate) fn bounded(&self) -> bool {
        self.gullet
            .execution_control()
            .is_some_and(|control| control.storage().is_some())
    }
    pub(crate) fn unsupported(&self) -> ParseError {
        match self.gullet.execution_control() {
            Some(control) => ParseError::control(control.fail(ExecutionError::Unsupported {
                phase: Phase::Parser,
            })),
            None => ParseError::msg("Unsupported elementary math construct"),
        }
    }
    fn depth_error(&self) -> ParseError {
        if let Some(storage) = self
            .gullet
            .execution_control()
            .and_then(ExecutionControl::storage)
        {
            ParseError::storage(
                storage.fail(ratex_types::storage::StorageError::DepthLimit {
                    phase: Phase::Parser,
                    limit: 16,
                    requested: self.depth_budget.current().saturating_add(1) as u32,
                }),
            )
        } else {
            ParseError::recursion_limit_exceeded()
        }
    }
    fn admit_function(
        &self,
        name: &str,
        spec: &functions::FunctionSpec,
        builtin: bool,
    ) -> ParseResult<()> {
        if !self.bounded() {
            return Ok(());
        }
        // Closed producer set. Rejected before argument parsing, handler allocation,
        // environment registry, regex compilation, chemistry, SVG or user definitions.
        let supported = matches!(
            spec.node_type,
            "genfrac"
                | "infix"
                | "sqrt"
                | "op"
                | "styling"
                | "font"
                | "text"
                | "spacing"
                | "overline"
                | "underline"
                | "leftright"
                | "leftright-right"
                | "middle"
                | "delimsizing"
        );
        let opaque_args = spec.arg_types.as_ref().is_some_and(|types| {
            types.iter().any(|arg| {
                matches!(
                    arg,
                    ArgType::Color | ArgType::Size | ArgType::Url | ArgType::Raw | ArgType::HBox
                )
            })
        });
        let internal = builtin
            && matches!(
                name,
                "\\html@mathml"
                    | "\\mathchoice"
                    | "\\phantom"
                    | "\\vphantom"
                    | "\\smash"
                    | "\\mathllap"
                    | "\\mathrlap"
                    | "\\mathclap"
                    | "\\mathord"
                    | "\\mathbin"
                    | "\\mathrel"
                    | "\\mathopen"
                    | "\\mathclose"
                    | "\\mathpunct"
                    | "\\mathinner"
                    | "\\@char"
                    | "\\kern"
                    | "\\mkern"
                    | "\\hskip"
                    | "\\mskip"
                    | "\\relax"
                    | "\\operatorname@"
                    | "\\operatornamewithlimits"
            );
        let vector_accent = name == "\\vec" && spec.node_type == "accent";
        let brace = self.mode == Mode::Math
            && spec.node_type == "horizBrace"
            && matches!(name, "\\overbrace" | "\\underbrace");
        if ((!supported && !vector_accent && !brace) || opaque_args) && !internal {
            return Err(self.unsupported());
        }
        Ok(())
    }
    pub(crate) fn text(&self, value: &str) -> ParseResult<String> {
        if let Some(storage) = self
            .gullet
            .execution_control()
            .and_then(ExecutionControl::storage)
        {
            storage
                .control()
                .consume(Phase::Parser, Resource::GeneratedBytes, value.len() as u64)
                .map_err(ParseError::control)?;
            storage
                .try_string(Phase::Parser, value)
                .map_err(ParseError::storage)
        } else {
            Ok(value.to_owned())
        }
    }
    pub(crate) fn formatted(&self, args: std::fmt::Arguments<'_>) -> ParseResult<String> {
        if let Some(storage) = self
            .gullet
            .execution_control()
            .and_then(ExecutionControl::storage)
        {
            storage
                .try_format(Phase::Parser, args)
                .map_err(ParseError::storage)
        } else {
            Ok(args.to_string())
        }
    }
    pub(crate) fn pushed<T>(&self, out: &mut Vec<T>, value: T) -> ParseResult<()> {
        if let Some(storage) = self
            .gullet
            .execution_control()
            .and_then(ExecutionControl::storage)
        {
            storage
                .try_push(
                    Phase::Parser,
                    ratex_types::storage::StorageFamily::Ast,
                    out,
                    value,
                )
                .map_err(ParseError::storage)
        } else {
            out.push(value);
            Ok(())
        }
    }
    pub(crate) fn boxed<T>(&self, value: T) -> ParseResult<Box<T>> {
        if let Some(storage) = self
            .gullet
            .execution_control()
            .and_then(ExecutionControl::storage)
        {
            storage
                .try_box(
                    Phase::Parser,
                    ratex_types::storage::StorageFamily::Ast,
                    value,
                )
                .map_err(ParseError::storage)
        } else {
            Ok(Box::new(value))
        }
    }
    pub(crate) fn cloned_slice<T: Clone + ratex_types::storage::BoundedClone>(
        &self,
        values: &[T],
    ) -> ParseResult<Vec<T>> {
        let mut out = Vec::new();
        for value in values {
            self.pushed(&mut out, self.clone_value(value)?)?;
        }
        Ok(out)
    }
    pub(crate) fn values<T, const N: usize>(&self, values: [T; N]) -> ParseResult<Vec<T>> {
        let mut out = Vec::new();
        for value in values {
            self.pushed(&mut out, value)?;
        }
        Ok(out)
    }
    pub(crate) fn ord_argument(&self, value: ParseNode) -> ParseResult<Vec<ParseNode>> {
        match value {
            ParseNode::OrdGroup { body, .. } => Ok(body),
            value => self.values([value]),
        }
    }
    pub(crate) fn append_text(&self, out: &mut String, text: &str) -> ParseResult<()> {
        if let Some(storage) = self
            .gullet
            .execution_control()
            .and_then(ExecutionControl::storage)
        {
            storage
                .try_append(Phase::Parser, out, text)
                .map_err(ParseError::storage)
        } else {
            out.push_str(text);
            Ok(())
        }
    }
    pub(crate) fn new_bounded(
        input: &'a str,
        storage: &'a ratex_types::storage::StorageLedger<'a>,
    ) -> Result<Self, ratex_types::storage::StorageError> {
        let depth_budget = DepthBudget::new_bounded(16, storage)?;
        Ok(Self {
            mode: Mode::Math,
            gullet: MacroExpander::new_bounded(input, Mode::Math, depth_budget.clone(), storage)?,
            leftright_depth: 0,
            depth_budget,
            next_token: None,
            chinese_text_argument: false,
            equation_counter: 0,
        })
    }
    pub(crate) fn clone_value<T: Clone + ratex_types::storage::BoundedClone>(
        &self,
        value: &T,
    ) -> ParseResult<T> {
        ratex_types::storage::clone_controlled(
            self.gullet.execution_control(),
            Phase::Parser,
            ratex_types::storage::StorageFamily::Ast,
            value,
        )
        .map_err(ParseError::storage)
    }
    pub fn new(input: &'a str) -> Self {
        let depth_budget = DepthBudget::new(MAX_INPUT_DEPTH);
        Self {
            mode: Mode::Math,
            gullet: MacroExpander::new_with_budget(input, Mode::Math, depth_budget.clone()),
            leftright_depth: 0,
            depth_budget,
            next_token: None,
            chinese_text_argument: false,
            equation_counter: 0,
        }
    }

    // ── Token management ────────────────────────────────────────────────

    /// Return the current lookahead token (fetching from gullet if needed).
    pub fn fetch(&mut self) -> ParseResult<Token> {
        self.gullet.checkpoint(Phase::Parser)?;
        if self.next_token.is_none() {
            self.next_token = Some(self.gullet.expand_next_token()?);
        }
        self.clone_value(self.next_token.as_ref().unwrap())
    }

    /// Discard the current lookahead token.
    pub fn consume(&mut self) {
        self.next_token = None;
    }

    /// Expect the next token to have the given text, consuming it.
    pub fn expect(&mut self, text: &str, do_consume: bool) -> ParseResult<()> {
        let tok = self.fetch()?;
        if tok.text != text {
            return Err(ParseError::new(
                self.formatted(format_args!("Expected '{}', got '{}'", text, tok.text))?,
                Some(&tok),
            ));
        }
        if do_consume {
            self.consume();
        }
        Ok(())
    }

    /// Consume spaces in math mode.
    pub fn consume_spaces(&mut self) -> ParseResult<()> {
        loop {
            let tok = self.fetch()?;
            if tok.text == " " {
                self.consume();
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Switch between "math" and "text" modes.
    pub fn switch_mode(&mut self, new_mode: Mode) {
        self.mode = new_mode;
        self.gullet.switch_mode(new_mode);
    }

    pub(crate) fn depth_budget(&self) -> &DepthBudget {
        &self.depth_budget
    }

    // ── Main parse entry ────────────────────────────────────────────────

    /// Parse the entire input and return the AST.
    pub fn parse(&mut self) -> ParseResult<Vec<ParseNode>> {
        self.gullet.begin_group();

        // The root expression itself is not input nesting.  Only recursive
        // expressions below it consume the structural-depth budget.
        let result = self.parse_expression_impl(false, None);

        match result {
            Ok(parse) => {
                let eof_result = self.expect("EOF", true);
                self.gullet.end_group();
                eof_result?;
                if let Some(control) = self
                    .gullet
                    .execution_control()
                    .filter(|control| control.storage().is_some())
                {
                    crate::stack_safety::validate_parse_nodes_controlled(&parse, 16, control)?;
                } else {
                    validate_parse_nodes(&parse, MAX_INPUT_DEPTH)?;
                }
                Ok(parse)
            }
            Err(e) => {
                self.gullet.end_groups();
                Err(e)
            }
        }
    }

    // ── Expression parsing ──────────────────────────────────────────────

    /// Parse an expression: a list of atoms.
    pub fn parse_expression(
        &mut self,
        break_on_infix: bool,
        break_on_token_text: Option<&str>,
    ) -> ParseResult<Vec<ParseNode>> {
        let _guard = self.depth_budget.enter().map_err(|_| self.depth_error())?;

        self.parse_expression_impl(break_on_infix, break_on_token_text)
    }

    fn parse_expression_impl(
        &mut self,
        break_on_infix: bool,
        break_on_token_text: Option<&str>,
    ) -> ParseResult<Vec<ParseNode>> {
        let mut body = Vec::new();

        loop {
            if self.mode == Mode::Math {
                self.consume_spaces()?;
            }

            let lex = self.fetch()?;

            if END_OF_EXPRESSION.contains(&lex.text.as_str()) {
                break;
            }
            if let Some(break_text) = break_on_token_text {
                if lex.text == break_text {
                    break;
                }
            }
            if break_on_infix {
                if let Some(func) = FUNCTIONS.get(lex.text.as_str()) {
                    if func.infix {
                        break;
                    }
                }
            }

            if let Some(control) = self.gullet.execution_control() {
                control
                    .consume(Phase::Parser, Resource::ParserSteps, 1)
                    .map_err(ParseError::control)?;
            }
            let atom = self.parse_atom(break_on_token_text)?;

            match atom {
                None => break,
                Some(node) if node.type_name() == "internal" => continue,
                Some(node) => self.pushed(&mut body, node)?,
            }
        }

        if self.mode == Mode::Text {
            self.form_ligatures(&mut body)?;
        }

        self.handle_infix_nodes(body)
    }

    /// Rewrite infix operators (e.g. \over → \frac).
    fn handle_infix_nodes(&mut self, body: Vec<ParseNode>) -> ParseResult<Vec<ParseNode>> {
        let mut over_index: Option<usize> = None;
        let mut func_name: Option<String> = None;

        for (i, node) in body.iter().enumerate() {
            if let ParseNode::Infix { replace_with, .. } = node {
                if over_index.is_some() {
                    return Err(ParseError::msg("only one infix operator per group"));
                }
                over_index = Some(i);
                func_name = Some(self.clone_value(replace_with)?);
            }
        }

        if let (Some(idx), Some(fname)) = (over_index, func_name) {
            let numer_body: Vec<ParseNode> = self.cloned_slice(&body[..idx])?;
            let denom_body: Vec<ParseNode> = self.cloned_slice(&body[idx + 1..])?;

            let numer = if numer_body.len() == 1 {
                if let ParseNode::OrdGroup { .. } = &numer_body[0] {
                    numer_body.into_iter().next().unwrap()
                } else {
                    ParseNode::OrdGroup {
                        mode: self.mode,
                        body: numer_body,
                        semisimple: None,
                        loc: None,
                    }
                }
            } else {
                ParseNode::OrdGroup {
                    mode: self.mode,
                    body: numer_body,
                    semisimple: None,
                    loc: None,
                }
            };

            let denom = if denom_body.len() == 1 {
                if let ParseNode::OrdGroup { .. } = &denom_body[0] {
                    denom_body.into_iter().next().unwrap()
                } else {
                    ParseNode::OrdGroup {
                        mode: self.mode,
                        body: denom_body,
                        semisimple: None,
                        loc: None,
                    }
                }
            } else {
                ParseNode::OrdGroup {
                    mode: self.mode,
                    body: denom_body,
                    semisimple: None,
                    loc: None,
                }
            };

            let node = if fname == "\\\\abovefrac" {
                // \above passes the infix node (with bar size) as the middle argument
                let infix_node = self.clone_value(&body[idx])?;
                self.call_function(
                    &fname,
                    self.values([numer, infix_node, denom])?,
                    Vec::new(),
                    None,
                    None,
                )?
            } else {
                self.call_function(&fname, self.values([numer, denom])?, Vec::new(), None, None)?
            };
            self.values([node])
        } else {
            Ok(body)
        }
    }

    /// Form ligatures in text mode (e.g. -- → –, --- → —).
    fn form_ligatures(&self, group: &mut Vec<ParseNode>) -> ParseResult<()> {
        let mut i = 0;
        while i + 1 < group.len() {
            let a_text = group[i].symbol_text();
            let b_text = group[i + 1].symbol_text();

            if let (Some(a), Some(b)) = (a_text, b_text) {
                if group[i].type_name() == "textord" && group[i + 1].type_name() == "textord" {
                    if a == "-" && b == "-" {
                        if i + 2 < group.len() {
                            if let Some(c) = group[i + 2].symbol_text() {
                                if c == "-" && group[i + 2].type_name() == "textord" {
                                    group[i] = ParseNode::TextOrd {
                                        mode: Mode::Text,
                                        text: self.text("---")?,
                                        loc: None,
                                    };
                                    group.remove(i + 2);
                                    group.remove(i + 1);
                                    continue;
                                }
                            }
                        }
                        group[i] = ParseNode::TextOrd {
                            mode: Mode::Text,
                            text: self.text("--")?,
                            loc: None,
                        };
                        group.remove(i + 1);
                        continue;
                    }
                    if (a == "'" || a == "`") && b == a {
                        group[i] = ParseNode::TextOrd {
                            mode: Mode::Text,
                            text: self.text(if a == "'" { "''" } else { "``" })?,
                            loc: None,
                        };
                        group.remove(i + 1);
                        continue;
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }

    // ── Atom parsing ────────────────────────────────────────────────────

    /// Parse a single atom with optional super/subscripts.
    pub fn parse_atom(
        &mut self,
        break_on_token_text: Option<&str>,
    ) -> ParseResult<Option<ParseNode>> {
        let mut base = self.parse_group("atom", break_on_token_text)?;

        if let Some(ref b) = base {
            if b.type_name() == "internal" {
                return Ok(base);
            }
        }

        if self.mode == Mode::Text {
            return Ok(base);
        }

        let mut superscript: Option<ParseNode> = None;
        let mut subscript: Option<ParseNode> = None;

        loop {
            self.consume_spaces()?;
            let lex = self.fetch()?;

            if lex.text == "\\limits" || lex.text == "\\nolimits" {
                let is_limits = lex.text == "\\limits";
                self.consume();
                if let Some(base_node) = base.as_mut() {
                    match base_node {
                        ParseNode::Op {
                            limits,
                            always_handle_sup_sub,
                            ..
                        } => {
                            *limits = is_limits;
                            *always_handle_sup_sub = Some(is_limits);
                        }
                        ParseNode::OperatorName {
                            limits,
                            always_handle_sup_sub,
                            ..
                        } => {
                            *limits = is_limits;
                            *always_handle_sup_sub = is_limits;
                        }
                        _ => {}
                    }
                }
            } else if lex.text == "^" {
                if superscript.is_some() {
                    return Err(ParseError::new("Double superscript", Some(&lex)));
                }
                superscript = Some(self.handle_sup_subscript("superscript")?);
            } else if lex.text == "_" {
                if subscript.is_some() {
                    return Err(ParseError::new("Double subscript", Some(&lex)));
                }
                subscript = Some(self.handle_sup_subscript("subscript")?);
            } else if lex.text == "'" {
                if superscript.is_some() {
                    return Err(ParseError::new("Double superscript", Some(&lex)));
                }
                let prime = ParseNode::TextOrd {
                    mode: self.mode,
                    text: self.text("\\prime")?,
                    loc: None,
                };
                let mut primes = self.values([self.clone_value(&prime)?])?;
                self.consume();
                while self.fetch()?.text == "'" {
                    self.pushed(&mut primes, self.clone_value(&prime)?)?;
                    self.consume();
                }
                if self.fetch()?.text == "^" {
                    let extra = self.handle_sup_subscript("superscript")?;
                    self.pushed(&mut primes, extra)?;
                }
                superscript = Some(ParseNode::OrdGroup {
                    mode: self.mode,
                    body: primes,
                    semisimple: None,
                    loc: None,
                });
            } else if let Some((mapped, is_sub)) = lex
                .text
                .chars()
                .next()
                .and_then(crate::unicode_sup_sub::unicode_sub_sup)
            {
                if is_sub && subscript.is_some() {
                    return Err(ParseError::new("Double subscript", Some(&lex)));
                }
                if !is_sub && superscript.is_some() {
                    return Err(ParseError::new("Double superscript", Some(&lex)));
                }
                if self.bounded() {
                    return Err(self.unsupported());
                }
                // Collect consecutive Unicode sup/sub chars of the same kind
                let mut subsup_tokens = vec![Token::new(mapped, 0, 0)];
                self.consume();
                loop {
                    let tok = self.fetch()?;
                    match tok
                        .text
                        .chars()
                        .next()
                        .and_then(crate::unicode_sup_sub::unicode_sub_sup)
                    {
                        Some((m, sub)) if sub == is_sub => {
                            subsup_tokens.insert(0, Token::new(m, 0, 0));
                            self.consume();
                        }
                        _ => break,
                    }
                }
                let body = self.subparse(subsup_tokens)?;
                let group = ParseNode::OrdGroup {
                    mode: Mode::Math,
                    body,
                    semisimple: None,
                    loc: None,
                };
                if is_sub {
                    subscript = Some(group);
                } else {
                    superscript = Some(group);
                }
            } else {
                break;
            }
        }

        if superscript.is_some() || subscript.is_some() {
            Ok(Some(ParseNode::SupSub {
                mode: self.mode,
                base: base.map(|value| self.boxed(value)).transpose()?,
                sup: superscript.map(|value| self.boxed(value)).transpose()?,
                sub: subscript.map(|value| self.boxed(value)).transpose()?,
                loc: None,
            }))
        } else {
            Ok(base)
        }
    }

    /// Handle a subscript or superscript.
    fn handle_sup_subscript(&mut self, name: &str) -> ParseResult<ParseNode> {
        let symbol_token = self.fetch()?;
        self.consume();
        self.consume_spaces()?;

        let group = self.parse_group(name, None)?;
        match group {
            Some(g) if g.type_name() != "internal" => Ok(g),
            Some(_) => {
                // Skip internal nodes, try again
                let g2 = self.parse_group(name, None)?;
                match g2 {
                    Some(node) => Ok(node),
                    None => Err(ParseError::new(
                        self.formatted(format_args!(
                            "Expected group after '{}'",
                            symbol_token.text
                        ))?,
                        Some(&symbol_token),
                    )),
                }
            }
            None => Err(ParseError::new(
                self.formatted(format_args!("Expected group after '{}'", symbol_token.text))?,
                Some(&symbol_token),
            )),
        }
    }

    // ── Group parsing ───────────────────────────────────────────────────

    /// Parse a group: braced expression, function call, or single symbol.
    pub fn parse_group(
        &mut self,
        name: &str,
        break_on_token_text: Option<&str>,
    ) -> ParseResult<Option<ParseNode>> {
        self.parse_group_impl(name, break_on_token_text, false)
    }

    pub(crate) fn parse_structural_group(
        &mut self,
        name: &str,
        break_on_token_text: Option<&str>,
    ) -> ParseResult<Option<ParseNode>> {
        let _guard = self.depth_budget.enter().map_err(|_| self.depth_error())?;
        self.parse_group_impl(name, break_on_token_text, true)
    }

    fn parse_delimiter_group(&mut self, name: &str) -> ParseResult<Option<ParseNode>> {
        let first_token = self.fetch()?;
        let text = self.clone_value(&first_token.text)?;

        if text == "{" || text == "\\begingroup" {
            return self.parse_structural_group(name, None);
        }

        let result = self.parse_symbol_inner()?;
        if result.is_none() {
            return Err(ParseError::new(
                self.formatted(format_args!("Expected delimiter as {}", name))?,
                Some(&first_token),
            ));
        }

        Ok(result)
    }

    fn parse_group_impl(
        &mut self,
        name: &str,
        break_on_token_text: Option<&str>,
        depth_already_counted: bool,
    ) -> ParseResult<Option<ParseNode>> {
        let first_token = self.fetch()?;
        let text = self.clone_value(&first_token.text)?;

        if text == "{" || text == "\\begingroup" {
            self.consume();
            let group_end = if text == "{" { "}" } else { "\\endgroup" };

            if !depth_already_counted && self.depth_budget.remaining() == 0 {
                return Err(self.depth_error());
            }

            self.gullet.begin_group();
            let result: ParseResult<_> = (|| {
                let expression = if depth_already_counted {
                    self.parse_expression_impl(false, Some(group_end))?
                } else {
                    self.parse_expression(false, Some(group_end))?
                };
                let last_token = self.fetch()?;
                self.expect(group_end, true)?;
                Ok((expression, last_token))
            })();
            self.gullet.end_group();
            let (expression, last_token) = result?;

            let loc = Some(SourceLocation::range(&first_token.loc, &last_token.loc));
            let semisimple = if text == "\\begingroup" {
                Some(true)
            } else {
                None
            };

            Ok(Some(ParseNode::OrdGroup {
                mode: self.mode,
                body: expression,
                semisimple,
                loc,
            }))
        } else {
            let result = match self.parse_function(break_on_token_text, Some(name))? {
                some @ Some(_) => some,
                None => self.parse_symbol_inner()?,
            };

            if result.is_none()
                && text.starts_with('\\')
                && !IMPLICIT_COMMANDS.contains(&text.as_str())
            {
                return Err(ParseError::new(
                    self.formatted(format_args!("Undefined control sequence: {}", text))?,
                    Some(&first_token),
                ));
            }

            Ok(result)
        }
    }

    // ── Function parsing ────────────────────────────────────────────────

    /// Try to parse a function call. Returns None if not a function.
    pub fn parse_function(
        &mut self,
        break_on_token_text: Option<&str>,
        name: Option<&str>,
    ) -> ParseResult<Option<ParseNode>> {
        let token = self.fetch()?;
        let func = self.clone_value(&token.text)?;

        if self.bounded() && func == "\\begin" {
            if self.mode != Mode::Math {
                return Err(self.unsupported());
            }
            self.consume();
            return crate::environments::parse_bounded_array(self).map(Some);
        }

        let func_data = match FUNCTIONS.get(func.as_str()) {
            Some(f) => f,
            None => return Ok(None),
        };

        self.admit_function(&func, func_data, token.is_builtin_literal)?;
        self.consume();

        if let Some(n) = name {
            if n != "atom" && !func_data.allowed_in_argument {
                return Err(ParseError::new(
                    self.formatted(format_args!(
                        "Got function '{}' with no arguments as {}",
                        func, n
                    ))?,
                    Some(&token),
                ));
            }
        }

        if self.bounded() {
            if (self.mode == Mode::Text && !func_data.allowed_in_text)
                || (self.mode == Mode::Math && !func_data.allowed_in_math)
            {
                return Err(ParseError::msg("Function is invalid in this mode"));
            }
        } else {
            functions::check_mode_compatibility(func_data, self.mode, &func, Some(&token))?;
        }

        // `\hspace*{len}` — `*` is a separate token (not part of the control word); consume it here.
        // Must use gullet peek/pop only: `parser.fetch()` without `consume()` advances the lexer and
        // leaves `{` only in `next_token`, so `parse_size_group`'s `gullet.future()` would miss the brace.
        if func == "\\hspace" {
            self.gullet.consume_spaces();
            if self.gullet.future().text == "*" {
                self.gullet.pop_token();
            }
        }

        let (args, opt_args) = self.parse_arguments(&func, func_data)?;

        self.call_function(&func, args, opt_args, Some(token), break_on_token_text)
            .map(Some)
    }

    /// Call a function handler.
    pub fn call_function(
        &mut self,
        name: &str,
        args: Vec<ParseNode>,
        opt_args: Vec<Option<ParseNode>>,
        token: Option<Token>,
        break_on_token_text: Option<&str>,
    ) -> ParseResult<ParseNode> {
        let func = FUNCTIONS
            .get(name)
            .ok_or_else(|| ParseError::msg("No function handler"))?;
        self.admit_function(
            name,
            func,
            token.as_ref().is_some_and(|token| token.is_builtin_literal),
        )?;

        let owned_name = self.text(name)?;
        let owned_break = break_on_token_text.map(|s| self.text(s)).transpose()?;
        let mut ctx = FunctionContext {
            func_name: owned_name,
            parser: self,
            token: token,
            break_on_token_text: owned_break,
        };

        (func.handler)(&mut ctx, args, opt_args)
    }

    /// Parse the arguments for a function.
    pub fn parse_arguments(
        &mut self,
        func: &str,
        func_data: &functions::FunctionSpec,
    ) -> ParseResult<(Vec<ParseNode>, Vec<Option<ParseNode>>)> {
        let total_args = func_data.num_args + func_data.num_optional_args;
        if total_args == 0 {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut args = Vec::new();
        let mut opt_args = Vec::new();

        for i in 0..total_args {
            let arg_type = func_data
                .arg_types
                .as_ref()
                .and_then(|types| types.get(i).copied());
            let is_optional = i < func_data.num_optional_args;

            let effective_type = if (func_data.primitive && arg_type.is_none())
                || (func_data.node_type == "sqrt"
                    && i == 1
                    && opt_args
                        .first()
                        .is_some_and(|o: &Option<ParseNode>| o.is_none()))
            {
                Some(ArgType::Primitive)
            } else {
                arg_type
            };
            let structural = argument_consumes_depth(func, func_data, effective_type);

            let argument_name = self.formatted(format_args!("argument to '{}'", func))?;
            let previous = self.chinese_text_argument;
            self.chinese_text_argument = (func_data.node_type == "text"
                && effective_type == Some(ArgType::Text))
                || (previous
                    && self.mode == Mode::Text
                    && matches!(effective_type, None | Some(ArgType::Original)));
            let arg =
                self.parse_group_of_type(&argument_name, effective_type, is_optional, structural);
            self.chinese_text_argument = previous;
            let arg = arg?;

            if is_optional {
                self.pushed(&mut opt_args, arg)?;
            } else if let Some(a) = arg {
                self.pushed(&mut args, a)?;
            } else {
                return Err(ParseError::msg(
                    "Null argument, please report this as a bug",
                ));
            }
        }

        Ok((args, opt_args))
    }

    /// Parse a group with a specific type.
    fn parse_group_of_type(
        &mut self,
        name: &str,
        arg_type: Option<ArgType>,
        optional: bool,
        structural: bool,
    ) -> ParseResult<Option<ParseNode>> {
        match arg_type {
            Some(ArgType::Color) => self.parse_color_group(optional),
            Some(ArgType::Size) => self.parse_size_group(optional),
            Some(ArgType::Primitive) => {
                if optional {
                    return Err(ParseError::msg("A primitive argument cannot be optional"));
                }
                let group = if structural {
                    self.parse_structural_group(name, None)?
                } else {
                    self.parse_delimiter_group(name)?
                };
                match group {
                    Some(g) => Ok(Some(g)),
                    None => Err(ParseError::new(
                        self.formatted(format_args!("Expected group as {}", name))?,
                        None,
                    )),
                }
            }
            Some(ArgType::Math) | Some(ArgType::Text) => {
                let mode = match arg_type {
                    Some(ArgType::Math) => Some(Mode::Math),
                    Some(ArgType::Text) => Some(Mode::Text),
                    _ => None,
                };
                self.parse_argument_group(optional, mode)
            }
            Some(ArgType::HBox) => {
                let group = self.parse_argument_group(optional, Some(Mode::Text))?;
                match group {
                    Some(g) => Ok(Some(ParseNode::Styling {
                        mode: g.mode(),
                        style: crate::parse_node::StyleStr::Text,
                        body: self.values([g])?,
                        loc: None,
                    })),
                    None => Ok(None),
                }
            }
            Some(ArgType::Raw) => {
                let token = self.parse_string_group("raw", optional)?;
                match token {
                    Some(t) => Ok(Some(ParseNode::Raw {
                        mode: Mode::Text,
                        string: t.text,
                        loc: None,
                    })),
                    None => Ok(None),
                }
            }
            Some(ArgType::Url) => self.parse_url_group(optional),
            None | Some(ArgType::Original) => self.parse_argument_group(optional, None),
        }
    }

    /// Parse a color group.
    fn parse_color_group(&mut self, optional: bool) -> ParseResult<Option<ParseNode>> {
        let res = self.parse_string_group("color", optional)?;
        match res {
            None => Ok(None),
            Some(token) => {
                let text = token.text.trim().to_string();
                let re = regex_lite::Regex::new(
                    r"^(#[a-fA-F0-9]{3,4}|#[a-fA-F0-9]{6}|#[a-fA-F0-9]{8}|[a-fA-F0-9]{6}|[a-zA-Z]+|\d+(\.\d+)?(,\d+(\.\d+)?)*)$",
                )
                .unwrap();

                if !re.is_match(&text) {
                    return Err(ParseError::new(
                        format!("Invalid color: '{}'", text),
                        Some(&token),
                    ));
                }
                let mut color = text;
                if regex_lite::Regex::new(r"^[0-9a-fA-F]{6}$")
                    .unwrap()
                    .is_match(&color)
                {
                    color = format!("#{}", color);
                }

                Ok(Some(ParseNode::ColorToken {
                    mode: self.mode,
                    color,
                    loc: None,
                }))
            }
        }
    }

    /// Parse a size group (e.g., "3pt", "1em").
    pub fn parse_size_group(&mut self, optional: bool) -> ParseResult<Option<ParseNode>> {
        if self.bounded() {
            return self.parse_builtin_size(optional);
        }
        let mut is_blank = false;

        self.gullet.consume_spaces();
        let res = if !optional && self.gullet.future().text != "{" {
            Some(
                self.parse_regex_group(
                    &regex_lite::Regex::new(r"^[-+]? *(?:$|\d+|\d+\.\d*|\.\d*) *[a-z]{0,2} *$")
                        .unwrap(),
                    "size",
                )?,
            )
        } else {
            self.parse_string_group("size", optional)?
        };

        let res = match res {
            Some(r) => r,
            None => return Ok(None),
        };

        let mut text = res.text.clone();
        if !optional && text.is_empty() {
            text = "0pt".to_string();
            is_blank = true;
        }

        let size_re =
            regex_lite::Regex::new(r"([-+]?) *(\d+(?:\.\d*)?|\.\d+) *([a-z]{2})").unwrap();
        let m = size_re
            .captures(&text)
            .ok_or_else(|| ParseError::new(format!("Invalid size: '{}'", text), Some(&res)))?;

        let sign = m.get(1).map_or("", |m| m.as_str());
        let magnitude = m.get(2).map_or("", |m| m.as_str());
        let unit = m.get(3).map_or("", |m| m.as_str());

        let number: f64 = format!("{}{}", sign, magnitude).parse().unwrap_or(0.0);

        if !is_valid_unit(unit) {
            return Err(ParseError::new(
                format!("Invalid unit: '{}'", unit),
                Some(&res),
            ));
        }

        Ok(Some(ParseNode::Size {
            mode: self.mode,
            value: crate::parse_node::Measurement {
                number,
                unit: unit.to_string(),
            },
            is_blank,
            loc: None,
        }))
    }

    /// Only reached for trusted static builtin kern commands. Every consumed numeric/unit
    /// token must independently retain that provenance; user macro arguments cannot inject it.
    fn parse_builtin_size(&mut self, optional: bool) -> ParseResult<Option<ParseNode>> {
        if optional {
            return Err(self.unsupported());
        }
        self.consume_spaces()?;
        let first = self.fetch()?;
        if !first.is_builtin_literal {
            return Err(self.unsupported());
        }
        let braced = first.text == "{";
        if braced {
            self.consume();
        }
        let mut bytes = [0u8; 32];
        let mut length = 0usize;
        let mut unit_length = 0usize;
        loop {
            let token = self.fetch()?;
            if !token.is_builtin_literal {
                return Err(self.unsupported());
            }
            if braced && token.text == "}" {
                self.consume();
                break;
            }
            if token.text == " " {
                self.consume();
                continue;
            }
            if token.text.len() != 1 {
                return Err(ParseError::msg("Invalid fixed builtin kern"));
            }
            let byte = token.text.as_bytes()[0];
            if !(byte.is_ascii_digit()
                || matches!(byte, b'+' | b'-' | b'.')
                || byte.is_ascii_lowercase())
                || length == bytes.len()
            {
                return Err(ParseError::msg("Invalid fixed builtin kern"));
            }
            if byte.is_ascii_lowercase() {
                unit_length += 1;
            } else if unit_length > 0 {
                return Err(ParseError::msg("Invalid fixed builtin kern unit"));
            }
            bytes[length] = byte;
            length += 1;
            self.consume();
            if !braced && unit_length == 2 {
                break;
            }
        }
        if unit_length != 2 || length <= 2 {
            return Err(ParseError::msg("Invalid fixed builtin kern"));
        }
        let text = std::str::from_utf8(&bytes[..length])
            .map_err(|_| ParseError::msg("Invalid fixed builtin kern"))?;
        let split = length - 2;
        let number = text[..split]
            .parse::<f64>()
            .map_err(|_| ParseError::msg("Invalid fixed builtin kern number"))?;
        let unit = &text[split..];
        if !number.is_finite() || number.abs() > 64.0 || !matches!(unit, "mu" | "em" | "pt") {
            return Err(self.unsupported());
        }
        Ok(Some(ParseNode::Size {
            mode: self.mode,
            value: crate::parse_node::Measurement {
                number,
                unit: self.text(unit)?,
            },
            is_blank: false,
            loc: None,
        }))
    }

    /// Parse a URL group.
    /// Temporarily disables `%` as comment character to allow `%20` etc. in URLs.
    fn parse_url_group(&mut self, optional: bool) -> ParseResult<Option<ParseNode>> {
        self.gullet.lexer.set_catcode('%', 13);
        self.gullet.lexer.set_catcode('~', 12);
        let res = self.parse_string_group("url", optional);
        self.gullet.lexer.set_catcode('%', 14);
        self.gullet.lexer.set_catcode('~', 13);
        let res = res?;
        match res {
            None => Ok(None),
            Some(token) => {
                let url = token.text;
                Ok(Some(ParseNode::Url {
                    mode: self.mode,
                    url,
                    loc: None,
                }))
            }
        }
    }

    /// Parse a string group (brace-enclosed string).
    fn parse_string_group(
        &mut self,
        _mode_name: &str,
        optional: bool,
    ) -> ParseResult<Option<Token>> {
        let arg_token = self.gullet.scan_argument(optional)?;
        let arg_token = match arg_token {
            Some(t) => t,
            None => return Ok(None),
        };

        let mut s = String::new();
        loop {
            let next = self.fetch()?;
            if next.text == "EOF" {
                break;
            }
            self.append_text(&mut s, &next.text)?;
            self.consume();
        }
        self.consume(); // consume EOF

        let mut result = arg_token;
        result.text = s;
        Ok(Some(result))
    }

    /// Parse a regex-delimited group.
    fn parse_regex_group(
        &mut self,
        regex: &regex_lite::Regex,
        mode_name: &str,
    ) -> ParseResult<Token> {
        let first_token = self.fetch()?;
        let mut last_token = self.clone_value(&first_token)?;
        let mut s = String::new();

        loop {
            let next = self.fetch()?;
            if next.text == "EOF" {
                break;
            }
            let candidate = format!("{}{}", s, next.text);
            if regex.is_match(&candidate) {
                last_token = next;
                s = candidate;
                self.consume();
            } else {
                break;
            }
        }

        if s.is_empty() {
            return Err(ParseError::new(
                format!("Invalid {}: '{}'", mode_name, first_token.text),
                Some(&first_token),
            ));
        }

        Ok(first_token.range(&last_token, s))
    }

    /// Parse an argument group (with optional mode switch).
    pub fn parse_argument_group(
        &mut self,
        optional: bool,
        mode: Option<Mode>,
    ) -> ParseResult<Option<ParseNode>> {
        let arg_token = self.gullet.scan_argument(optional)?;
        let arg_token = match arg_token {
            Some(t) => t,
            None => return Ok(None),
        };

        let outer_mode = self.mode;
        if let Some(m) = mode {
            self.switch_mode(m);
        }

        self.gullet.begin_group();
        let result: ParseResult<_> = (|| {
            let expression = self.parse_expression(false, Some("EOF"))?;
            self.expect("EOF", true)?;
            Ok(expression)
        })();
        self.gullet.end_group();
        let result_mode = self.mode;
        let expression = result;

        if mode.is_some() {
            self.switch_mode(outer_mode);
        }

        let expression = expression?;

        let result = ParseNode::OrdGroup {
            mode: result_mode,
            loc: Some(arg_token.loc.clone()),
            body: expression,
            semisimple: None,
        };

        Ok(Some(result))
    }

    // ── Symbol parsing ──────────────────────────────────────────────────

    /// Parse a single symbol (internal version that returns Result).
    fn parse_symbol_inner(&mut self) -> ParseResult<Option<ParseNode>> {
        let nucleus = self.fetch()?;
        let text = self.clone_value(&nucleus.text)?;

        if let Some(stripped) = text.strip_prefix("\\verb") {
            if let Some(control) = self
                .gullet
                .execution_control()
                .filter(|control| control.storage().is_some())
            {
                return Err(ParseError::control(control.fail(
                    ExecutionError::Unsupported {
                        phase: Phase::Parser,
                    },
                )));
            }
            self.consume();
            let (star, rest) = if let Some(r) = stripped.strip_prefix('*') {
                (true, r)
            } else {
                (false, stripped)
            };
            let chars: Vec<char> = rest.chars().collect();
            if chars.len() < 2 {
                return Err(ParseError::new("\\verb assertion failed", Some(&nucleus)));
            }
            let body: String = chars[1..chars.len() - 1].iter().collect();
            return Ok(Some(ParseNode::Verb {
                mode: Mode::Text,
                body,
                star,
                loc: Some(nucleus.loc.clone()),
            }));
        }

        let font_mode = match self.mode {
            Mode::Math => ratex_font::symbols::Mode::Math,
            Mode::Text => ratex_font::symbols::Mode::Text,
        };

        // ^ and _ are handled by parse_atom for sup/sub, not as symbol nodes
        if text == "^" || text == "_" {
            return Ok(None);
        }

        // Bare backslash (incomplete control sequence) → not a valid symbol
        if text == "\\" {
            return Ok(None);
        }

        if (self.mode == Mode::Math || self.chinese_text_argument && self.mode == Mode::Text)
            && self.bounded()
            && !nucleus.is_builtin_literal
            && is_chinese_text_run(&text)
        {
            return self.parse_chinese_text_run(text, nucleus.loc).map(Some);
        }

        if let Some(sym_info) = ratex_font::symbols::get_symbol(&text, font_mode) {
            let loc = Some(SourceLocation::range(&nucleus.loc, &nucleus.loc));
            let group = sym_info.group;

            let node = if group.is_atom() {
                let family = match group {
                    ratex_font::symbols::Group::Bin => AtomFamily::Bin,
                    ratex_font::symbols::Group::Close => AtomFamily::Close,
                    ratex_font::symbols::Group::Inner => AtomFamily::Inner,
                    ratex_font::symbols::Group::Open => AtomFamily::Open,
                    ratex_font::symbols::Group::Punct => AtomFamily::Punct,
                    ratex_font::symbols::Group::Rel => AtomFamily::Rel,
                    _ => unreachable!(),
                };
                ParseNode::Atom {
                    mode: self.mode,
                    family,
                    text,
                    loc,
                }
            } else {
                match group {
                    ratex_font::symbols::Group::MathOrd => ParseNode::MathOrd {
                        mode: self.mode,
                        text,
                        loc,
                    },
                    ratex_font::symbols::Group::TextOrd => ParseNode::TextOrd {
                        mode: self.mode,
                        text,
                        loc,
                    },
                    ratex_font::symbols::Group::OpToken => ParseNode::OpToken {
                        mode: self.mode,
                        text,
                        loc,
                    },
                    ratex_font::symbols::Group::AccentToken => ParseNode::AccentToken {
                        mode: self.mode,
                        text,
                        loc,
                    },
                    ratex_font::symbols::Group::Spacing => ParseNode::SpacingNode {
                        mode: self.mode,
                        text,
                        loc,
                    },
                    _ => ParseNode::MathOrd {
                        mode: self.mode,
                        text,
                        loc,
                    },
                }
            };

            self.consume();
            return Ok(Some(node));
        }

        // Unicode accented characters → decompose into accent nodes
        // Handles both precomposed (á U+00E1) and combining forms (a + U+0301)
        if let Some(node) = self.try_parse_unicode_accent(&text, &nucleus)? {
            self.consume();
            return Ok(Some(node));
        }

        // Non-ASCII characters without accent decomposition → treat as textord
        // KaTeX always uses mode="text" for these, regardless of current mode
        let first_char = text.chars().next();
        if let Some(ch) = first_char {
            if ch as u32 >= 0x80 {
                let node = ParseNode::TextOrd {
                    mode: Mode::Text,
                    text,
                    loc: Some(SourceLocation::range(&nucleus.loc, &nucleus.loc)),
                };
                self.consume();
                return Ok(Some(node));
            }
        }

        Ok(None)
    }

    fn parse_chinese_text_run(
        &mut self,
        mut text: String,
        mut loc: SourceLocation,
    ) -> ParseResult<ParseNode> {
        let storage = self
            .gullet
            .execution_control()
            .and_then(ExecutionControl::storage)
            .ok_or_else(|| self.unsupported())?;
        self.consume();
        let mut tail = chinese_text_run_tail(&text).ok_or_else(|| self.unsupported())?;
        loop {
            let token = self.fetch()?;
            if token.is_builtin_literal || token.loc.start != loc.end {
                break;
            }
            let mut chars = token.text.chars();
            let Some(ch) = chars.next() else { break };
            if chars.next().is_some() {
                break;
            }
            let Some(next_tail) = advance_chinese_text_run(Some(tail), ch) else {
                break;
            };
            storage
                .control()
                .consume(Phase::Parser, Resource::ParserSteps, 1)
                .map_err(ParseError::control)?;
            storage
                .try_append(Phase::Parser, &mut text, &token.text)
                .map_err(ParseError::storage)?;
            loc.end = token.loc.end;
            tail = next_tail;
            self.consume();
        }
        if tail == ChineseTextRunTail::Joiner {
            return Err(self.unsupported());
        }
        Ok(ParseNode::TextOrd {
            mode: Mode::Text,
            text,
            loc: Some(loc),
        })
    }

    /// Try to decompose a Unicode accented character into accent nodes.
    /// Returns None if no decomposition is available.
    /// Only decomposes Latin-script characters, matching KaTeX behavior.
    fn try_parse_unicode_accent(
        &self,
        text: &str,
        nucleus: &Token,
    ) -> ParseResult<Option<ParseNode>> {
        if let Some(control) = self
            .gullet
            .execution_control()
            .filter(|control| control.storage().is_some())
        {
            if text.is_ascii() {
                return Ok(None);
            }
            // General Unicode normalization/shaping is not part of the elementary slice.
            // Reject before unicode-normalization can grow its internal TinyVec buffer.
            return Err(ParseError::control(control.fail(
                ExecutionError::Unsupported {
                    phase: Phase::Parser,
                },
            )));
        }
        let nfd: String = text.nfd().collect();
        let chars: Vec<char> = nfd.chars().collect();

        if chars.len() < 2 {
            return Ok(None);
        }

        // Build from the base up through each combining mark
        let mut split_idx = chars.len() - 1;
        while split_idx > 0 && is_supported_combining_accent(chars[split_idx]) {
            split_idx -= 1;
        }

        // Verify ALL trailing chars are supported combining accents
        if split_idx == chars.len() - 1 {
            return Ok(None);
        }

        // Only decompose Latin-script base characters.
        let base_char = chars[0];
        if !is_latin_base_char(base_char) {
            return Ok(None);
        }

        let accent_count = chars.len() - split_idx - 1;
        self.depth_budget
            .ensure_additional(accent_count)
            .map_err(|_| ParseError::new("Recursion limit exceeded", Some(nucleus)))?;

        let loc = Some(SourceLocation::range(&nucleus.loc, &nucleus.loc));

        // Base: everything before the combining marks
        let mut base_str: String = chars[..split_idx + 1].iter().collect();

        // Accented i→ı and j→ȷ (dotless variants), matching KaTeX behavior
        if base_str.len() == 1 {
            match base_str.as_str() {
                "i" => base_str = "\u{0131}".to_string(), // ı
                "j" => base_str = "\u{0237}".to_string(), // ȷ
                _ => {}
            }
        }

        let font_mode = match self.mode {
            Mode::Math => ratex_font::symbols::Mode::Math,
            Mode::Text => ratex_font::symbols::Mode::Text,
        };

        let mut node = if base_str.chars().count() == 1 {
            let ch = base_str.chars().next().unwrap();
            if let Some(sym) = ratex_font::symbols::get_symbol(&base_str, font_mode) {
                match sym.group {
                    ratex_font::symbols::Group::TextOrd => ParseNode::TextOrd {
                        mode: self.mode,
                        text: base_str.clone(),
                        loc: loc.clone(),
                    },
                    _ => ParseNode::MathOrd {
                        mode: self.mode,
                        text: base_str.clone(),
                        loc: loc.clone(),
                    },
                }
            } else if (ch as u32) >= 0x80 {
                // Non-ASCII base chars always text mode (KaTeX compat)
                ParseNode::TextOrd {
                    mode: Mode::Text,
                    text: base_str.clone(),
                    loc: loc.clone(),
                }
            } else {
                ParseNode::MathOrd {
                    mode: self.mode,
                    text: base_str.clone(),
                    loc: loc.clone(),
                }
            }
        } else {
            return self
                .try_parse_unicode_accent(&base_str, nucleus)
                .map(|opt| {
                    opt.or_else(|| {
                        Some(ParseNode::TextOrd {
                            mode: Mode::Text,
                            text: base_str.clone(),
                            loc: loc.clone(),
                        })
                    })
                });
        };

        // Wrap in accent nodes from innermost to outermost
        for &combining in &chars[split_idx + 1..] {
            let label = combining_to_accent_label(combining, self.mode);
            node = ParseNode::Accent {
                mode: self.mode,
                label,
                is_stretchy: Some(false),
                is_shifty: Some(true),
                base: Box::new(node),
                loc: loc.clone(),
            };
        }

        Ok(Some(node))
    }

    /// Parse a sub-expression from the given tokens.
    pub fn subparse(&mut self, tokens: Vec<Token>) -> ParseResult<Vec<ParseNode>> {
        let old_token = self.next_token.take();

        self.gullet.push_token(Token::new("}", 0, 0));
        self.gullet.push_tokens(tokens);
        let parse = self.parse_expression(false, None)?;
        self.expect("}", true)?;

        self.next_token = old_token;
        Ok(parse)
    }
}

fn is_latin_base_char(ch: char) -> bool {
    matches!(ch,
        'A'..='Z' | 'a'..='z'
        | '\u{0131}' // ı (dotless i)
        | '\u{0237}' // ȷ (dotless j)
        | '\u{00C6}' // Æ
        | '\u{00D0}' // Ð
        | '\u{00D8}' // Ø
        | '\u{00DE}' // Þ
        | '\u{00DF}' // ß
        | '\u{00E6}' // æ
        | '\u{00F0}' // ð
        | '\u{00F8}' // ø
        | '\u{00FE}' // þ
    )
}

fn is_supported_combining_accent(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'
            | '\u{0301}'
            | '\u{0302}'
            | '\u{0303}'
            | '\u{0304}'
            | '\u{0306}'
            | '\u{0307}'
            | '\u{0308}'
            | '\u{030A}'
            | '\u{030B}'
            | '\u{030C}'
            | '\u{0327}'
    )
}

fn combining_to_accent_label(ch: char, mode: Mode) -> String {
    match mode {
        Mode::Math => match ch {
            '\u{0300}' => "\\grave".to_string(),
            '\u{0301}' => "\\acute".to_string(),
            '\u{0302}' => "\\hat".to_string(),
            '\u{0303}' => "\\tilde".to_string(),
            '\u{0304}' => "\\bar".to_string(),
            '\u{0306}' => "\\breve".to_string(),
            '\u{0307}' => "\\dot".to_string(),
            '\u{0308}' => "\\ddot".to_string(),
            '\u{030A}' => "\\mathring".to_string(),
            '\u{030B}' => "\\H".to_string(),
            '\u{030C}' => "\\check".to_string(),
            '\u{0327}' => "\\c".to_string(),
            _ => format!("\\char\"{:X}", ch as u32),
        },
        Mode::Text => match ch {
            '\u{0300}' => "\\`".to_string(),
            '\u{0301}' => "\\'".to_string(),
            '\u{0302}' => "\\^".to_string(),
            '\u{0303}' => "\\~".to_string(),
            '\u{0304}' => "\\=".to_string(),
            '\u{0306}' => "\\u".to_string(),
            '\u{0307}' => "\\.".to_string(),
            '\u{0308}' => "\\\"".to_string(),
            '\u{030A}' => "\\r".to_string(),
            '\u{030B}' => "\\H".to_string(),
            '\u{030C}' => "\\v".to_string(),
            '\u{0327}' => "\\c".to_string(),
            _ => format!("\\char\"{:X}", ch as u32),
        },
    }
}

fn is_han(ch: char) -> bool {
    matches!(ch,
        '\u{3005}' | '\u{3007}' |
        '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' |
        '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{2a6df}' |
        '\u{2a700}'..='\u{2b73f}' | '\u{2b740}'..='\u{2b81f}' |
        '\u{2b820}'..='\u{2ceaf}' | '\u{2ceb0}'..='\u{2ebef}' |
        '\u{2ebf0}'..='\u{2ee5f}' | '\u{2f800}'..='\u{2fa1f}' |
        '\u{30000}'..='\u{3134f}' | '\u{31350}'..='\u{323af}' |
        '\u{323b0}'..='\u{3347f}'
    )
}

#[doc(hidden)]
pub fn is_chinese_text_char(ch: char) -> bool {
    is_han(ch)
        || matches!(ch,
            '\u{3000}'..='\u{3003}' | '\u{3008}'..='\u{3011}' |
            '\u{3014}'..='\u{301f}' | '\u{ff01}'..='\u{ff60}' |
            '\u{ffe0}'..='\u{ffe6}'
        )
}

fn is_han_variation_selector(ch: char) -> bool {
    matches!(ch, '\u{fe00}'..='\u{fe02}' | '\u{e0100}'..='\u{e01ef}')
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChineseTextRunTail {
    Other,
    Han,
    HanVariant,
    Joiner,
}

// A joiner belongs to a literal Chinese run only between Han bases. Keep it in
// the shaped/source text, but never admit arbitrary default-ignorable controls.
fn advance_chinese_text_run(
    previous: Option<ChineseTextRunTail>,
    ch: char,
) -> Option<ChineseTextRunTail> {
    use ChineseTextRunTail::{Han, HanVariant, Joiner, Other};

    if previous == Some(Joiner) {
        return is_han(ch).then_some(Han);
    }
    if is_han(ch) {
        Some(Han)
    } else if is_chinese_text_char(ch) {
        Some(Other)
    } else if previous == Some(Han) && is_han_variation_selector(ch) {
        Some(HanVariant)
    } else if matches!(previous, Some(Han | HanVariant)) && ch == '\u{200d}' {
        Some(Joiner)
    } else {
        None
    }
}

fn chinese_text_run_tail(text: &str) -> Option<ChineseTextRunTail> {
    let mut tail = None;
    for ch in text.chars() {
        tail = Some(advance_chinese_text_run(tail, ch)?);
    }
    tail
}

/// A literal run eligible for measured Chinese text in bounded math layout.
pub fn is_chinese_text_run(text: &str) -> bool {
    chinese_text_run_tail(text).is_some_and(|tail| tail != ChineseTextRunTail::Joiner)
}

fn argument_consumes_depth(
    func_name: &str,
    func_data: &functions::FunctionSpec,
    arg_type: Option<ArgType>,
) -> bool {
    match arg_type {
        Some(ArgType::Color | ArgType::Size | ArgType::Raw | ArgType::Url) => false,
        Some(ArgType::Primitive) => {
            !matches!(func_name, "\\left" | "\\right" | "\\middle")
                && func_data.node_type != "delimsizing"
        }
        Some(ArgType::Math | ArgType::Text | ArgType::HBox | ArgType::Original) | None => true,
    }
}

fn is_valid_unit(unit: &str) -> bool {
    matches!(
        unit,
        "pt" | "mm"
            | "cm"
            | "in"
            | "bp"
            | "pc"
            | "dd"
            | "cc"
            | "nd"
            | "nc"
            | "sp"
            | "px"
            | "ex"
            | "em"
            | "mu"
    )
}

/// If the whole expression is wrapped in TeX inline/display math delimiters, parse the inside only.
/// The parser already runs in math mode; a leading `$` would otherwise hit the `$` / `\\(` "switch to math"
/// handler, which is disallowed in math mode (see `functions::math`).
fn strip_outer_math_delimiters(input: &str) -> &str {
    let s = input.trim();
    if s.len() >= 4 && s.starts_with("$$") && s.ends_with("$$") {
        return s[2..s.len() - 2].trim();
    }
    if s.len() >= 2 && s.starts_with('$') && s.ends_with('$') {
        return s[1..s.len() - 1].trim();
    }
    s
}

/// Convenience function: parse a LaTeX string and return the AST.
pub fn parse(input: &str) -> ParseResult<Vec<ParseNode>> {
    Parser::new(strip_outer_math_delimiters(input)).parse()
}

#[derive(Debug)]
pub enum ControlledParseError {
    Storage(ratex_types::storage::StorageError),
    Execution(ExecutionError),
    Parse(ParseError),
}
impl std::fmt::Display for ControlledParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => e.fmt(f),
            Self::Execution(e) => e.fmt(f),
            Self::Parse(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ControlledParseError {}

pub fn parse_with_storage<'a>(
    input: &'a str,
    storage: &'a ratex_types::storage::StorageLedger<'a>,
) -> Result<Vec<ParseNode>, ControlledParseError> {
    let control = storage.control();
    control
        .consume(Phase::Admission, Resource::InputBytes, input.len() as u64)
        .map_err(ControlledParseError::Execution)?;
    storage
        .check(Phase::Parser)
        .map_err(ControlledParseError::Storage)?;
    let mut parser = Parser::new_bounded(strip_outer_math_delimiters(input), storage)
        .map_err(ControlledParseError::Storage)?;
    let result = parser.parse();
    if let Some(error) = storage.failure() {
        return Err(ControlledParseError::Storage(error));
    }
    if let Some(error) = control.failure() {
        return Err(ControlledParseError::Execution(error));
    }
    result.map_err(ControlledParseError::Parse)
}

/// Opt-in controlled entry. The legacy API and its behavior remain available.
pub fn parse_with_control<'a>(
    input: &'a str,
    control: &'a ExecutionControl<'a>,
) -> Result<Vec<ParseNode>, ControlledParseError> {
    control
        .consume(Phase::Admission, Resource::InputBytes, input.len() as u64)
        .map_err(ControlledParseError::Execution)?;
    let mut parser = Parser::new(strip_outer_math_delimiters(input));
    parser.gullet.set_execution(control);
    let result = parser.parse();
    if let Some(error) = control.failure() {
        return Err(ControlledParseError::Execution(error));
    }
    result.map_err(ControlledParseError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_chinese_text_keeps_runs_and_exact_source_ranges() {
        use ratex_types::{execution::Limits, storage::StorageLedger};
        use std::sync::atomic::AtomicBool;

        let input = "\\text{平均值，样本（n=3）𠮷\u{e0100}}";
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let storage = StorageLedger::new(&control);
        let nodes = parse_with_storage(input, &storage).unwrap();
        let [ParseNode::Text { body, .. }] = nodes.as_slice() else {
            panic!("expected one text node");
        };
        let runs: Vec<_> = body
            .iter()
            .filter_map(|node| match node {
                ParseNode::TextOrd {
                    mode: Mode::Text,
                    text,
                    loc: Some(loc),
                } if is_chinese_text_run(text) => {
                    assert_eq!(&input[loc.start..loc.end], text);
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(runs, ["平均值，样本（", "）𠮷\u{e0100}"]);

        let input = r"\text{中{\textbf{国}}文}";
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let storage = StorageLedger::new(&control);
        let nodes = parse_with_storage(input, &storage).unwrap();
        let [ParseNode::Text { body, .. }] = nodes.as_slice() else {
            panic!("expected one text node");
        };
        assert_eq!(body.len(), 3);
        assert_eq!(body[0].symbol_text(), Some("中"));
        assert!(matches!(body[1], ParseNode::OrdGroup { .. }));
        assert_eq!(body[2].symbol_text(), Some("文"));

        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let storage = StorageLedger::new(&control);
        assert!(parse_with_storage(r"\text{\underline{中文}}", &storage).is_ok());
    }

    #[test]
    fn bounded_bare_han_is_upright_text_beside_math_atoms() {
        use ratex_types::{execution::Limits, storage::StorageLedger};
        use std::sync::atomic::AtomicBool;

        let input = "汉+x";
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let storage = StorageLedger::new(&control);
        let nodes = parse_with_storage(input, &storage).unwrap();
        let [
            ParseNode::TextOrd {
                mode: Mode::Text,
                text: han,
                loc: Some(loc),
            },
            ParseNode::Atom {
                mode: Mode::Math,
                family: AtomFamily::Bin,
                text: plus,
                ..
            },
            ParseNode::MathOrd {
                mode: Mode::Math,
                text: x,
                ..
            },
        ] = nodes.as_slice()
        else {
            panic!("expected upright Han, math binary operator and math variable");
        };
        assert_eq!(han, "汉");
        assert_eq!(&input[loc.start..loc.end], han);
        assert_eq!(plus, "+");
        assert_eq!(x, "x");

        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let storage = StorageLedger::new(&control);
        assert!(parse_with_storage(r"\text{中文}+\mathrm{中文}", &storage).is_ok());
    }

    #[test]
    fn bounded_han_joiners_preserve_the_complete_literal_source_range() {
        use ratex_types::{execution::Limits, storage::StorageLedger};
        use std::sync::atomic::AtomicBool;

        let cancelled = AtomicBool::new(false);
        for run in [
            "汉\u{200d}字",
            "汉\u{e0100}\u{200d}字",
            "汉\u{200d}字\u{e0100}",
            "汉\u{200d}字\u{200d}文",
        ] {
            assert!(is_chinese_text_run(run));
            for input in [run.to_owned(), format!("\\text{{{run}}}")] {
                let control = ExecutionControl::new(&cancelled, None, Limits::default());
                let storage = StorageLedger::new(&control);
                let nodes = parse_with_storage(&input, &storage).unwrap();
                let body = match nodes.as_slice() {
                    [ParseNode::Text { body, .. }] => body.as_slice(),
                    body => body,
                };
                let [
                    ParseNode::TextOrd {
                        mode: Mode::Text,
                        text,
                        loc: Some(loc),
                    },
                ] = body
                else {
                    panic!("expected one complete upright Chinese run: {input}");
                };
                assert_eq!(text, run);
                assert_eq!(&input[loc.start..loc.end], run);
                assert!(text.contains('\u{200d}'));
            }
        }
    }

    #[test]
    fn bounded_chinese_text_does_not_admit_other_unicode_contexts() {
        use ratex_types::{execution::Limits, storage::StorageLedger};
        use std::sync::atomic::AtomicBool;

        let cancelled = AtomicBool::new(false);
        for input in [
            "\\text{\u{200d}汉}",
            "\\text{汉\u{200d}}",
            "\\text{汉\u{200d}\u{200d}字}",
            "\\text{汉\u{200d}a}",
            "\\text{汉\u{200d}，字}",
            "\\text{汉\u{200d}{字}}",
            "\\text{汉\u{200c}字}",
            "\\text{汉\u{2060}字}",
            "\\text{汉\u{202e}字}",
            "\\text{\u{e0100}字}",
            "\\text{字\u{e0100}\u{e0101}}",
            "\\text{字\u{301}}",
            r"\text{警告⚠}",
            r"\begin{中文}x\end{中文}",
        ] {
            let control = ExecutionControl::new(&cancelled, None, Limits::default());
            let storage = StorageLedger::new(&control);
            assert!(parse_with_storage(input, &storage).is_err(), "{input}");
        }
        assert!(!is_chinese_text_run("😀"));
        assert!(!is_chinese_text_run("字a"));
    }

    #[test]
    fn test_parse_single_char() {
        let result = parse("x").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "mathord");
    }

    #[test]
    fn test_parse_strips_outer_dollar_inline_math() {
        let inner = r"C_p[\ce{H2O(l)}] = \pu{75.3 J // mol K}";
        let wrapped = format!("${inner}$");
        let a = parse(&wrapped).expect("wrapped");
        let b = parse(inner).expect("inner");
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.type_name(), y.type_name());
        }
    }

    #[test]
    fn test_parse_addition() {
        let result = parse("a+b").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].type_name(), "mathord"); // a
        assert_eq!(result[1].type_name(), "atom"); // +
        assert_eq!(result[2].type_name(), "mathord"); // b
    }

    #[test]
    fn test_parse_superscript() {
        let result = parse("x^2").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "supsub");
    }

    #[test]
    fn test_parse_subscript() {
        let result = parse("a_i").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "supsub");
    }

    #[test]
    fn test_parse_supsub() {
        let result = parse("x^2_i").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "supsub");
        if let ParseNode::SupSub { sup, sub, .. } = &result[0] {
            assert!(sup.is_some());
            assert!(sub.is_some());
        } else {
            panic!("Expected SupSub");
        }
    }

    #[test]
    fn test_parse_group() {
        let result = parse("{a+b}").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "ordgroup");
    }

    #[test]
    fn test_parse_frac() {
        let result = parse("\\frac{a}{b}").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "genfrac");
    }

    #[test]
    fn test_parse_sqrt() {
        let result = parse("\\sqrt{x}").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "sqrt");
    }

    #[test]
    fn test_parse_sqrt_optional() {
        let result = parse("\\sqrt[3]{x}").unwrap();
        assert_eq!(result.len(), 1);
        if let ParseNode::Sqrt { index, .. } = &result[0] {
            assert!(index.is_some());
        } else {
            panic!("Expected Sqrt");
        }
    }

    #[test]
    fn test_parse_nested() {
        let result = parse("\\frac{\\sqrt{a^2+b^2}}{c}").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name(), "genfrac");
    }

    #[test]
    fn test_parse_empty() {
        let result = parse("").unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_double_superscript_error() {
        let result = parse("x^2^3");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unclosed_brace_error() {
        let result = parse("{x");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_output() {
        let result = parse("x^2").unwrap();
        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("supsub"));
    }
}

// Notepads Markdown finite growth-closure checks: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
#[cfg(test)]
mod bounded_error_fixup2_tests {
    use super::*;
    use ratex_types::{execution::Limits, storage::StorageLedger};
    use std::{borrow::Cow, sync::atomic::AtomicBool};

    #[test]
    fn fixup2_registered_error_handlers_borrow_exact_diagnostics_without_growth() {
        for (input, expected) in [
            (r"\hline", r"\hline valid only within array environment"),
            (
                r"\hdashline",
                r"\hdashline valid only within array environment",
            ),
            (r"\text{\)}", r"Mismatched \)"),
            (r"\text{\]}", r"Mismatched \]"),
        ] {
            let cancel = AtomicBool::new(false);
            let control = ExecutionControl::new(&cancel, None, Limits::default());
            let storage = StorageLedger::new(&control);
            let error = parse_with_storage(input, &storage).unwrap_err();
            let ControlledParseError::Parse(error) = error else {
                panic!("{input}: unexpected control failure");
            };
            assert_eq!(error.message, expected, "{input}");
            assert!(matches!(error.message, Cow::Borrowed(_)), "{input}");
            assert!(storage.failure().is_none(), "{input}");
            let legacy = parse(input).unwrap_err();
            assert_eq!(legacy.message, expected, "{input}");
            assert!(matches!(legacy.message, Cow::Owned(_)), "{input}");
        }
    }
}
