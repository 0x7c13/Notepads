use crate::sanitize::sanitize_text;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorRenamePolicy, EditorSemanticFacts,
    EditorSemanticKind, EditorSemanticSymbol, Error, MAX_DIAGRAM_NESTING_DEPTH, OperationControl,
    OperationControlResult, ParseMetadata, Result, SourceSpan,
};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};
#[cfg(test)]
use std::cell::Cell;
use std::fmt;

#[cfg(test)]
thread_local! {
    static RAILROAD_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_railroad_syntax_construction_count() {
    RAILROAD_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn railroad_syntax_construction_count() -> usize {
    RAILROAD_SYNTAX_CONSTRUCTION_COUNT.get()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RailroadDialect {
    Ir,
    Ebnf,
    Abnf,
    Peg,
}

impl RailroadDialect {
    fn is_identifier_start(self, ch: char) -> bool {
        ch.is_ascii_alphabetic() || (self != Self::Abnf && ch == '_')
    }

    fn is_identifier_continue(self, ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '-' || (self != Self::Abnf && ch == '_')
    }

    fn diagram_type(self) -> &'static str {
        match self {
            Self::Ir => "railroad",
            Self::Ebnf => "railroadEbnf",
            Self::Abnf => "railroadAbnf",
            Self::Peg => "railroadPeg",
        }
    }

    fn header(self) -> &'static str {
        match self {
            Self::Ir => "railroad-beta",
            Self::Ebnf => "railroad-ebnf-beta",
            Self::Abnf => "railroad-abnf-beta",
            Self::Peg => "railroad-peg-beta",
        }
    }

    fn common_detail_prefix(self) -> &'static str {
        match self {
            Self::Ir => "railroad",
            Self::Ebnf => "railroad ebnf",
            Self::Abnf => "railroad abnf",
            Self::Peg => "railroad peg",
        }
    }

    fn editor_rename_policy(self) -> EditorRenamePolicy {
        match self {
            Self::Ir => EditorRenamePolicy::RailroadIrRule,
            Self::Ebnf => EditorRenamePolicy::RailroadEbnfRule,
            Self::Abnf => EditorRenamePolicy::RailroadAbnfRule,
            Self::Peg => EditorRenamePolicy::RailroadPegRule,
        }
    }
}

pub(crate) fn is_valid_editor_ir_rule_identifier(candidate: &str) -> bool {
    is_valid_railroad_rule_identifier(candidate, RailroadDialect::Ir)
        && railroad_rule_name_conflict(candidate, RailroadDialect::Ir).is_none()
}

pub(crate) fn is_valid_editor_ebnf_rule_identifier(candidate: &str) -> bool {
    is_valid_railroad_rule_identifier(candidate, RailroadDialect::Ebnf)
        && railroad_rule_name_conflict(candidate, RailroadDialect::Ebnf).is_none()
}

pub(crate) fn is_valid_editor_peg_rule_identifier(candidate: &str) -> bool {
    is_valid_railroad_rule_identifier(candidate, RailroadDialect::Peg)
        && railroad_rule_name_conflict(candidate, RailroadDialect::Peg).is_none()
}

pub(crate) fn is_valid_editor_abnf_rule_identifier(candidate: &str) -> bool {
    is_valid_railroad_rule_identifier(candidate, RailroadDialect::Abnf)
        && railroad_rule_name_conflict(candidate, RailroadDialect::Abnf).is_none()
}

fn is_valid_railroad_rule_identifier(candidate: &str, dialect: RailroadDialect) -> bool {
    let mut chars = candidate.chars();
    chars.next().is_some_and(|first| {
        dialect.is_identifier_start(first) && chars.all(|ch| dialect.is_identifier_continue(ch))
    })
}

/// A Mermaid Railroad repetition bound stored with JavaScript number semantics.
///
/// Values are non-negative integral binary64 numbers or positive infinity. Positive infinity
/// represents an unbounded maximum or an oversized ABNF bound and serializes as JSON `null`,
/// matching `JSON.stringify(Infinity)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailroadRepeatBound(f64);

impl Eq for RailroadRepeatBound {}

impl RailroadRepeatBound {
    /// The finite bound zero.
    pub const ZERO: Self = Self(0.0);
    /// The finite bound one.
    pub const ONE: Self = Self(1.0);
    /// The positive-infinity bound.
    pub const INFINITY: Self = Self(f64::INFINITY);

    /// Returns whether this bound is zero.
    pub const fn is_zero(self) -> bool {
        self.0 == 0.0
    }

    /// Returns whether this bound is one.
    pub const fn is_one(self) -> bool {
        self.0 == 1.0
    }

    /// Returns whether this bound is positive infinity.
    pub const fn is_infinite(self) -> bool {
        self.0.is_infinite()
    }

    /// Returns the validated binary64 value.
    pub const fn as_f64(self) -> f64 {
        self.0
    }
}

/// An invalid value supplied to [`RailroadRepeatBound`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RailroadRepeatBoundError {
    /// The supplied value was NaN.
    #[error("railroad repetition bound cannot be NaN")]
    NotANumber,
    /// The supplied value was negative, including negative infinity.
    #[error("railroad repetition bound must be non-negative")]
    Negative,
    /// The supplied finite value had a fractional component.
    #[error("railroad repetition bound must be an integer")]
    Fractional,
}

impl TryFrom<f64> for RailroadRepeatBound {
    type Error = RailroadRepeatBoundError;

    fn try_from(value: f64) -> std::result::Result<Self, Self::Error> {
        if value.is_nan() {
            return Err(RailroadRepeatBoundError::NotANumber);
        }
        if value.is_sign_negative() && value != 0.0 {
            return Err(RailroadRepeatBoundError::Negative);
        }
        if value == 0.0 {
            return Ok(Self::ZERO);
        }
        if value.is_infinite() {
            return Ok(Self::INFINITY);
        }
        if value.fract() != 0.0 {
            return Err(RailroadRepeatBoundError::Fractional);
        }
        Ok(Self(value))
    }
}

impl From<u64> for RailroadRepeatBound {
    fn from(value: u64) -> Self {
        Self(value as f64)
    }
}

impl Serialize for RailroadRepeatBound {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.is_infinite() {
            return serializer.serialize_none();
        }

        let integer = self.0 as u64;
        if (integer as f64).to_bits() == self.0.to_bits() {
            serializer.serialize_u64(integer)
        } else {
            serializer.serialize_f64(self.0)
        }
    }
}

impl<'de> Deserialize<'de> for RailroadRepeatBound {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RailroadRepeatBoundVisitor;

        impl<'de> Visitor<'de> for RailroadRepeatBoundVisitor {
            type Value = RailroadRepeatBound;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a non-negative integer or null")
            }

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RailroadRepeatBound::INFINITY)
            }

            fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RailroadRepeatBound::INFINITY)
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RailroadRepeatBound::from(value))
            }

            fn visit_u128<E>(self, value: u128) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                RailroadRepeatBound::try_from(value as f64).map_err(E::custom)
            }

            fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                RailroadRepeatBound::try_from(value as f64).map_err(E::custom)
            }

            fn visit_i128<E>(self, value: i128) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                RailroadRepeatBound::try_from(value as f64).map_err(E::custom)
            }

            fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                RailroadRepeatBound::try_from(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(RailroadRepeatBoundVisitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RailroadDiagramModel {
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub rules: Vec<RailroadRuleModel>,
    #[serde(skip_serializing)]
    title_span: Option<SourceSpan>,
    #[serde(skip_serializing)]
    acc_title_span: Option<SourceSpan>,
    #[serde(skip_serializing)]
    acc_descr_span: Option<SourceSpan>,
}

pub type RailroadDiagramRenderModel = RailroadDiagramModel;

impl RailroadDiagramModel {
    fn new() -> Self {
        Self {
            acc_title: None,
            acc_descr: None,
            title: None,
            rules: Vec::new(),
            title_span: None,
            acc_title_span: None,
            acc_descr_span: None,
        }
    }

    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
        for rule in &mut self.rules {
            rule.name = sanitize_text(&rule.name, config);
            sanitize_ast_node(&mut rule.definition, config);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RailroadRuleModel {
    pub name: String,
    pub definition: RailroadAstNode,
    #[serde(skip_serializing, default = "zero_span")]
    name_span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum RailroadAstNode {
    #[serde(rename = "terminal")]
    Terminal {
        value: String,
        #[serde(skip_serializing, default = "zero_span")]
        span: SourceSpan,
        #[serde(skip_serializing, default = "zero_span")]
        selection: SourceSpan,
    },
    #[serde(rename = "nonterminal")]
    NonTerminal {
        name: String,
        #[serde(skip_serializing, default = "zero_span")]
        span: SourceSpan,
        #[serde(skip_serializing, default = "zero_span")]
        selection: SourceSpan,
    },
    #[serde(rename = "sequence")]
    Sequence {
        elements: Vec<RailroadAstNode>,
        #[serde(skip_serializing, default = "zero_span")]
        span: SourceSpan,
    },
    #[serde(rename = "choice")]
    Choice {
        alternatives: Vec<RailroadAstNode>,
        #[serde(skip_serializing, default = "zero_span")]
        span: SourceSpan,
    },
    #[serde(rename = "optional")]
    Optional {
        element: Box<RailroadAstNode>,
        #[serde(skip_serializing, default = "zero_span")]
        span: SourceSpan,
    },
    #[serde(rename = "repetition")]
    Repetition {
        element: Box<RailroadAstNode>,
        min: RailroadRepeatBound,
        #[serde(default = "default_repeat_max")]
        max: RailroadRepeatBound,
        #[serde(skip_serializing_if = "Option::is_none")]
        separator: Option<Box<RailroadAstNode>>,
        #[serde(skip_serializing, default = "zero_span")]
        span: SourceSpan,
    },
    #[serde(rename = "special")]
    Special {
        text: String,
        #[serde(skip_serializing, default = "zero_span")]
        span: SourceSpan,
        #[serde(skip_serializing, default = "zero_span")]
        selection: SourceSpan,
    },
}

fn default_repeat_max() -> RailroadRepeatBound {
    RailroadRepeatBound::INFINITY
}

fn zero_span() -> SourceSpan {
    SourceSpan::new(0, 0)
}

impl RailroadAstNode {
    fn span(&self) -> SourceSpan {
        match self {
            Self::Terminal { span, .. }
            | Self::NonTerminal { span, .. }
            | Self::Sequence { span, .. }
            | Self::Choice { span, .. }
            | Self::Optional { span, .. }
            | Self::Repetition { span, .. }
            | Self::Special { span, .. } => *span,
        }
    }

    fn selection(&self) -> SourceSpan {
        match self {
            Self::Terminal { selection, .. }
            | Self::NonTerminal { selection, .. }
            | Self::Special { selection, .. } => *selection,
            Self::Sequence { span, .. }
            | Self::Choice { span, .. }
            | Self::Optional { span, .. }
            | Self::Repetition { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommonFieldKind {
    Title,
    AccTitle,
    AccDescr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    String(String),
    SpecialSequence(String),
    NumVal(String),
    Repeat(String),
    Number(String),
    Common(ParsedCommonField),
    Symbol(char),
    ColonColonEq,
    LeftArrow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    span: SourceSpan,
    selection: SourceSpan,
}

pub(crate) fn parse_railroad(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_railroad_for_dialect(code, meta, RailroadDialect::Ir)
}

pub(crate) fn parse_railroad_ebnf(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_railroad_for_dialect(code, meta, RailroadDialect::Ebnf)
}

pub(crate) fn parse_railroad_abnf(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_railroad_for_dialect(code, meta, RailroadDialect::Abnf)
}

pub(crate) fn parse_railroad_peg(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_railroad_for_dialect(code, meta, RailroadDialect::Peg)
}

pub(crate) fn parse_railroad_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<RailroadDiagramRenderModel>> {
    parse_railroad_model_for_render_dialect_controlled(code, meta, RailroadDialect::Ir, control)
}

pub(crate) fn parse_railroad_ebnf_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<RailroadDiagramRenderModel>> {
    parse_railroad_model_for_render_dialect_controlled(code, meta, RailroadDialect::Ebnf, control)
}

pub(crate) fn parse_railroad_abnf_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<RailroadDiagramRenderModel>> {
    parse_railroad_model_for_render_dialect_controlled(code, meta, RailroadDialect::Abnf, control)
}

pub(crate) fn parse_railroad_peg_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<RailroadDiagramRenderModel>> {
    parse_railroad_model_for_render_dialect_controlled(code, meta, RailroadDialect::Peg, control)
}

pub(crate) fn parse_railroad_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    parse_railroad_json_and_editor_facts_for_dialect(code, meta, RailroadDialect::Ir, control)
}

pub(crate) fn parse_railroad_ebnf_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    parse_railroad_json_and_editor_facts_for_dialect(code, meta, RailroadDialect::Ebnf, control)
}

pub(crate) fn parse_railroad_abnf_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    parse_railroad_json_and_editor_facts_for_dialect(code, meta, RailroadDialect::Abnf, control)
}

pub(crate) fn parse_railroad_peg_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    parse_railroad_json_and_editor_facts_for_dialect(code, meta, RailroadDialect::Peg, control)
}

struct RailroadSemanticSource {
    model: RailroadDiagramModel,
    editor_facts: EditorSemanticFacts,
}

impl RailroadSemanticSource {
    fn editor_facts(&self) -> EditorSemanticFacts {
        self.editor_facts.clone()
    }

    fn into_compat_json(mut self, meta: &ParseMetadata) -> Result<Value> {
        self.model.sanitize_common_db_fields(&meta.effective_config);
        render_model_to_compat_json(&self.model, meta)
    }

    fn into_render_model(mut self, meta: &ParseMetadata) -> RailroadDiagramRenderModel {
        self.model.sanitize_common_db_fields(&meta.effective_config);
        self.model
    }
}

struct RailroadParseFailure {
    error: Box<Error>,
    editor_facts: Box<EditorSemanticFacts>,
}

struct RailroadLexerOutcome {
    tokens: Vec<Token>,
    first_error: Option<Error>,
}

struct RailroadParserOutcome {
    model: RailroadDiagramModel,
    trace: RailroadParserTrace,
    first_error: Option<Error>,
}

#[derive(Default)]
struct RailroadParserTrace {
    peg_predicates: Vec<RailroadPegPredicateTrace>,
}

struct RailroadPegPredicateTrace {
    span: SourceSpan,
    inner: RailroadAstNode,
}

fn parse_railroad_for_dialect(
    code: &str,
    meta: &ParseMetadata,
    dialect: RailroadDialect,
) -> Result<Value> {
    parse_railroad_semantic_source(code, meta, dialect)?.into_compat_json(meta)
}

fn parse_railroad_model_for_render_dialect_controlled(
    code: &str,
    meta: &ParseMetadata,
    dialect: RailroadDialect,
    control: &OperationControl,
) -> OperationControlResult<Result<RailroadDiagramRenderModel>> {
    let construction = construct_railroad_semantic_source(code, meta, dialect, control)?;
    match construction {
        Ok(source) => Ok(Ok(source.into_render_model(meta))),
        Err(failure) => Ok(Err(*failure.error)),
    }
}

fn parse_railroad_json_and_editor_facts_for_dialect(
    code: &str,
    meta: &ParseMetadata,
    dialect: RailroadDialect,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = construct_railroad_semantic_source(code, meta, dialect, control)?;
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |source| {
            let editor_facts = source.editor_facts();
            (source.into_compat_json(meta), editor_facts)
        },
        |failure| (*failure.error, *failure.editor_facts),
    );
    control.checkpoint()?;
    Ok(parsed)
}

pub(crate) fn render_model_to_compat_json(
    model: &RailroadDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    Ok(json!({
        "type": meta.diagram_type,
        "title": &model.title,
        "accTitle": &model.acc_title,
        "accDescr": &model.acc_descr,
        "rules": &model.rules,
    }))
}

fn parse_railroad_semantic_source(
    code: &str,
    meta: &ParseMetadata,
    dialect: RailroadDialect,
) -> Result<RailroadSemanticSource> {
    construct_railroad_semantic_source(code, meta, dialect, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
        .map_err(|failure| *failure.error)
}

fn construct_railroad_semantic_source(
    code: &str,
    meta: &ParseMetadata,
    dialect: RailroadDialect,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<RailroadSemanticSource, RailroadParseFailure>> {
    #[cfg(test)]
    RAILROAD_SYNTAX_CONSTRUCTION_COUNT.set(RAILROAD_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    let lexer =
        Lexer::new(code, dialect, meta.diagram_type.as_str()).tokenize_recovering(control)?;
    control.checkpoint()?;
    let parser = RailroadParser::new(
        lexer.tokens,
        code.len(),
        meta.diagram_type.as_str(),
        dialect,
    )
    .parse_recovering(control)?;
    let mut editor_facts = editor_facts_from_model(&parser.model, dialect, &parser.trace);
    control.checkpoint()?;

    if let Some(error) = earliest_railroad_error(lexer.first_error, parser.first_error) {
        let span = railroad_error_span(&error, SourceSpan::new(0, code.len()));
        editor_facts.mark_recovered_from_parse_error(
            format!(
                "{} parser recovered after parse error: {error}",
                dialect.diagram_type()
            ),
            Some(span),
        );
        return Ok(Err(RailroadParseFailure {
            error: Box::new(error),
            editor_facts: Box::new(editor_facts),
        }));
    }

    control.checkpoint()?;
    Ok(Ok(RailroadSemanticSource {
        model: parser.model,
        editor_facts,
    }))
}

fn earliest_railroad_error(left: Option<Error>, right: Option<Error>) -> Option<Error> {
    match (left, right) {
        (Some(left), Some(right)) => {
            if railroad_error_span(&left, SourceSpan::new(usize::MAX, usize::MAX)).start
                <= railroad_error_span(&right, SourceSpan::new(usize::MAX, usize::MAX)).start
            {
                Some(left)
            } else {
                Some(right)
            }
        }
        (Some(error), None) | (None, Some(error)) => Some(error),
        (None, None) => None,
    }
}

fn railroad_error_span(error: &Error, fallback: SourceSpan) -> SourceSpan {
    match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.span().unwrap_or(fallback),
        _ => fallback,
    }
}

fn is_railroad_ir_keyword(value: &str) -> bool {
    matches!(
        value,
        "sequence"
            | "choice"
            | "optional"
            | "oneOrMore"
            | "zeroOrMore"
            | "terminal"
            | "nonterminal"
            | "special"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RailroadRuleNameConflict {
    TitleToken,
    HeaderKeyword,
    IrConstructor,
}

fn railroad_rule_name_conflict(
    candidate: &str,
    dialect: RailroadDialect,
) -> Option<RailroadRuleNameConflict> {
    if candidate.starts_with("title") {
        return Some(RailroadRuleNameConflict::TitleToken);
    }
    if candidate == dialect.header() {
        return Some(RailroadRuleNameConflict::HeaderKeyword);
    }
    if dialect == RailroadDialect::Ir && is_railroad_ir_keyword(candidate) {
        return Some(RailroadRuleNameConflict::IrConstructor);
    }
    None
}

fn sanitize_ast_node(node: &mut RailroadAstNode, config: &crate::MermaidConfig) {
    match node {
        RailroadAstNode::Terminal { value, .. } => *value = sanitize_text(value, config),
        RailroadAstNode::NonTerminal { name, .. } => *name = sanitize_text(name, config),
        RailroadAstNode::Sequence { elements, .. } => {
            for element in elements {
                sanitize_ast_node(element, config);
            }
        }
        RailroadAstNode::Choice { alternatives, .. } => {
            for alternative in alternatives {
                sanitize_ast_node(alternative, config);
            }
        }
        RailroadAstNode::Optional { element, .. } => sanitize_ast_node(element, config),
        RailroadAstNode::Repetition {
            element, separator, ..
        } => {
            sanitize_ast_node(element, config);
            if let Some(separator) = separator {
                sanitize_ast_node(separator, config);
            }
        }
        RailroadAstNode::Special { text, .. } => *text = sanitize_text(text, config),
    }
}

struct RailroadParser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    input_len: usize,
    diagram_type: &'a str,
    dialect: RailroadDialect,
    trace: RailroadParserTrace,
}

/// Tracks owned AST depth independently from parser/group depth. A leaf has depth zero,
/// preserving the established boundary of 256 enclosing nodes.
struct RailroadConstructedNode {
    node: RailroadAstNode,
    depth: usize,
}

impl RailroadConstructedNode {
    fn leaf(node: RailroadAstNode) -> Self {
        Self { node, depth: 0 }
    }

    fn span(&self) -> SourceSpan {
        self.node.span()
    }

    fn selection(&self) -> SourceSpan {
        self.node.selection()
    }
}

#[derive(Clone, Copy)]
enum RailroadIrExpressionFrameKind {
    Optional,
    OneOrMore,
    ZeroOrMore,
    Sequence,
    Choice,
}

impl RailroadIrExpressionFrameKind {
    fn is_variadic(self) -> bool {
        matches!(self, Self::Sequence | Self::Choice)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Optional => "optional",
            Self::OneOrMore => "oneOrMore",
            Self::ZeroOrMore => "zeroOrMore",
            Self::Sequence => "sequence",
            Self::Choice => "choice",
        }
    }
}

struct RailroadIrExpressionFrame {
    kind: RailroadIrExpressionFrameKind,
    start: usize,
    nesting_depth: usize,
    elements: Vec<RailroadConstructedNode>,
}

#[derive(Clone, Copy)]
enum RailroadEbnfFrameKind {
    Root,
    Group { start: usize },
    Optional { start: usize },
    Repetition { start: usize },
}

impl RailroadEbnfFrameKind {
    fn closing_symbol(self) -> Option<char> {
        match self {
            Self::Root => None,
            Self::Group { .. } => Some(')'),
            Self::Optional { .. } => Some(']'),
            Self::Repetition { .. } => Some('}'),
        }
    }

    fn closing_message(self) -> &'static str {
        match self {
            Self::Root => "expected EBNF expression",
            Self::Group { .. } => "expected ')' after EBNF group",
            Self::Optional { .. } => "expected ']' after EBNF optional group",
            Self::Repetition { .. } => "expected '}' after EBNF repetition",
        }
    }
}

enum RailroadEbnfFrameState {
    ExpectPrimary,
    Pending(RailroadConstructedNode),
    ExpectExceptionRhs {
        left: RailroadConstructedNode,
        dash_span: SourceSpan,
    },
}

struct RailroadEbnfFrame {
    kind: RailroadEbnfFrameKind,
    nesting_depth: usize,
    alternatives: Vec<Vec<RailroadConstructedNode>>,
    sequence: Vec<RailroadConstructedNode>,
    state: RailroadEbnfFrameState,
}

#[derive(Clone, Copy)]
enum RailroadAbnfFrameKind {
    Root,
    Group { start: usize },
    Optional { start: usize },
}

impl RailroadAbnfFrameKind {
    fn closing_symbol(self) -> Option<char> {
        match self {
            Self::Root => None,
            Self::Group { .. } => Some(')'),
            Self::Optional { .. } => Some(']'),
        }
    }

    fn closing_message(self) -> &'static str {
        match self {
            Self::Root => "expected ABNF element",
            Self::Group { .. } => "expected ')' after ABNF group",
            Self::Optional { .. } => "expected ']' after ABNF optional group",
        }
    }
}

struct RailroadAbnfFrame {
    kind: RailroadAbnfFrameKind,
    nesting_depth: usize,
    alternatives: Vec<Vec<RailroadConstructedNode>>,
    sequence: Vec<RailroadConstructedNode>,
    pending_repeat: Option<SpannedText>,
}

#[derive(Clone, Copy)]
enum RailroadPegFrameKind {
    Root,
    Group { start: usize },
}

impl RailroadPegFrameKind {
    fn closing_symbol(self) -> Option<char> {
        match self {
            Self::Root => None,
            Self::Group { .. } => Some(')'),
        }
    }

    fn closing_message(self) -> &'static str {
        match self {
            Self::Root => "expected PEG expression",
            Self::Group { .. } => "expected ')' after PEG group",
        }
    }
}

struct RailroadPegFrame {
    kind: RailroadPegFrameKind,
    nesting_depth: usize,
    alternatives: Vec<Vec<RailroadConstructedNode>>,
    sequence: Vec<RailroadConstructedNode>,
    pending_prefix: Option<(char, SourceSpan)>,
}

impl<'a> RailroadParser<'a> {
    fn new(
        tokens: Vec<Token>,
        input_len: usize,
        diagram_type: &'a str,
        dialect: RailroadDialect,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            input_len,
            diagram_type,
            dialect,
            trace: RailroadParserTrace::default(),
        }
    }

    fn parse_recovering(
        mut self,
        control: &OperationControl,
    ) -> OperationControlResult<RailroadParserOutcome> {
        control.checkpoint()?;
        let mut model = RailroadDiagramModel::new();
        let mut first_error = None;
        if let Err(error) = self.expect_header() {
            return Ok(RailroadParserOutcome {
                model,
                trace: self.trace,
                first_error: Some(error),
            });
        }

        while let Ok(Some(field)) = self.take_common_field() {
            control.checkpoint()?;
            match field.kind {
                CommonFieldKind::Title => {
                    model.title = Some(field.value);
                    model.title_span = Some(field.span);
                }
                CommonFieldKind::AccTitle => {
                    model.acc_title = Some(field.value);
                    model.acc_title_span = Some(field.span);
                }
                CommonFieldKind::AccDescr => {
                    model.acc_descr = Some(field.value);
                    model.acc_descr_span = Some(field.span);
                }
            }
        }

        while !self.is_eof() {
            control.checkpoint()?;
            let checkpoint = self.pos;
            match self.parse_rule() {
                Ok(rule) => model.rules.push(rule),
                Err(error) => {
                    first_error.get_or_insert(error);
                    self.recover_to_next_rule(checkpoint, control)?;
                }
            }
        }

        control.checkpoint()?;
        Ok(RailroadParserOutcome {
            model,
            trace: self.trace,
            first_error,
        })
    }

    fn recover_to_next_rule(
        &mut self,
        checkpoint: usize,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let start = checkpoint.saturating_add(1).min(self.tokens.len());
        for index in start..self.tokens.len().saturating_sub(1) {
            if (index - start).is_multiple_of(128) {
                control.checkpoint()?;
            }
            if matches!(self.tokens[index].kind, TokenKind::Ident(_))
                && self.assignment_follows(index + 1)
            {
                self.pos = index;
                return Ok(());
            }
        }
        self.pos = self.tokens.len();
        Ok(())
    }

    fn assignment_follows(&self, index: usize) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| match self.dialect {
                RailroadDialect::Peg => matches!(token.kind, TokenKind::LeftArrow),
                RailroadDialect::Ebnf => {
                    matches!(token.kind, TokenKind::Symbol('=') | TokenKind::ColonColonEq)
                }
                RailroadDialect::Ir | RailroadDialect::Abnf => {
                    matches!(token.kind, TokenKind::Symbol('='))
                }
            })
    }

    fn expect_header(&mut self) -> Result<()> {
        let token = self.take().ok_or_else(|| {
            self.error_at_current(format!("expected {} header", self.dialect.header()))
        })?;
        let TokenKind::Ident(value) = &token.kind else {
            return Err(
                self.error_at_token(&token, format!("expected {} header", self.dialect.header()))
            );
        };
        if value != self.dialect.header() {
            return Err(
                self.error_at_token(&token, format!("expected {} header", self.dialect.header()))
            );
        }
        Ok(())
    }

    fn take_common_field(&mut self) -> Result<Option<SpannedCommonField>> {
        let Some(token) = self.peek() else {
            return Ok(None);
        };
        let TokenKind::Common(field) = &token.kind else {
            return Ok(None);
        };
        let out = SpannedCommonField {
            kind: field.kind,
            value: field.value.text.clone(),
            span: field.value.selection,
        };
        self.pos += 1;
        Ok(Some(out))
    }

    fn parse_rule(&mut self) -> Result<RailroadRuleModel> {
        let name = self.expect_rule_name()?;
        match self.dialect {
            RailroadDialect::Peg => self.expect_left_arrow("expected '<-' after PEG rule name")?,
            RailroadDialect::Ebnf => {
                if !(self.take_symbol('=') || self.take_colon_colon_eq()) {
                    return Err(self.error_at_current("expected '=' or '::=' after EBNF rule name"));
                }
            }
            RailroadDialect::Ir | RailroadDialect::Abnf => {
                self.expect_symbol('=', "expected '=' after railroad rule name")?;
            }
        }

        let definition = match self.dialect {
            RailroadDialect::Ir => self.parse_ir_expression(0)?,
            RailroadDialect::Ebnf => self.parse_ebnf_choice(0)?,
            RailroadDialect::Abnf => self.parse_abnf_alternation(0)?,
            RailroadDialect::Peg => self.parse_peg_ordered_choice(0)?,
        };
        self.expect_symbol(';', "expected ';' after railroad rule definition")?;

        Ok(RailroadRuleModel {
            name: name.text,
            definition: definition.node,
            name_span: name.selection,
        })
    }

    fn parse_ir_expression(&mut self, nesting_depth: usize) -> Result<RailroadConstructedNode> {
        let mut frames: Vec<RailroadIrExpressionFrame> = Vec::new();
        let mut pending = None;

        loop {
            if let Some(node) = pending.take() {
                let Some(frame) = frames.last_mut() else {
                    return Ok(node);
                };
                frame.elements.push(node);

                if frame.kind.is_variadic() && self.take_symbol(',') {
                    continue;
                }

                let frame = frames.pop().expect("non-empty Railroad IR frame stack");
                pending = Some(self.finish_ir_expression_frame(frame)?);
                continue;
            }

            let expression_depth = frames
                .last()
                .map(|frame: &RailroadIrExpressionFrame| frame.nesting_depth.saturating_add(1))
                .unwrap_or(nesting_depth);
            self.ensure_nesting_depth(expression_depth)?;

            let function = self.expect_ident("expected railroad expression")?;
            self.expect_symbol('(', "expected '(' after railroad expression name")?;
            let kind = match function.text.as_str() {
                "optional" => Some(RailroadIrExpressionFrameKind::Optional),
                "oneOrMore" => Some(RailroadIrExpressionFrameKind::OneOrMore),
                "zeroOrMore" => Some(RailroadIrExpressionFrameKind::ZeroOrMore),
                "sequence" => Some(RailroadIrExpressionFrameKind::Sequence),
                "choice" => Some(RailroadIrExpressionFrameKind::Choice),
                "terminal" => {
                    let value = self.expect_string("expected string argument for terminal")?;
                    self.expect_symbol(')', "expected ')' after terminal argument")?;
                    pending = Some(RailroadConstructedNode::leaf(RailroadAstNode::Terminal {
                        value: value.text,
                        span: value.span,
                        selection: value.selection,
                    }));
                    None
                }
                "nonterminal" => {
                    let name = self.expect_string("expected string argument for nonterminal")?;
                    self.expect_symbol(')', "expected ')' after nonterminal argument")?;
                    pending = Some(RailroadConstructedNode::leaf(
                        RailroadAstNode::NonTerminal {
                            name: name.text,
                            span: name.span,
                            selection: name.selection,
                        },
                    ));
                    None
                }
                "special" => {
                    let text = self.expect_string("expected string argument for special")?;
                    self.expect_symbol(')', "expected ')' after special argument")?;
                    pending = Some(RailroadConstructedNode::leaf(RailroadAstNode::Special {
                        text: text.text,
                        span: text.span,
                        selection: text.selection,
                    }));
                    None
                }
                _ => {
                    return Err(self.error_at_span(
                        function.span,
                        format!("unsupported railroad expression: {}", function.text),
                    ));
                }
            };

            if let Some(kind) = kind {
                if kind.is_variadic() && self.check_symbol(')') {
                    return Err(self.error_at_current(format!(
                        "{} requires at least one argument",
                        kind.name()
                    )));
                }
                frames.push(RailroadIrExpressionFrame {
                    kind,
                    start: function.span.start,
                    nesting_depth: expression_depth,
                    elements: Vec::new(),
                });
            }
        }
    }

    fn finish_ir_expression_frame(
        &mut self,
        mut frame: RailroadIrExpressionFrame,
    ) -> Result<RailroadConstructedNode> {
        let end = self.expect_symbol(
            ')',
            format!("expected ')' after {} arguments", frame.kind.name()),
        )?;
        let span = SourceSpan::new(frame.start, end.span.end);
        if matches!(frame.kind, RailroadIrExpressionFrameKind::Sequence) {
            return self.collapse_sequence(frame.elements, span);
        }
        if matches!(frame.kind, RailroadIrExpressionFrameKind::Choice) {
            return self.collapse_choice(frame.elements, span);
        }
        let depth = self.next_constructed_depth(frame.elements.iter().map(|node| node.depth))?;
        let element = Box::new(
            frame
                .elements
                .pop()
                .expect("unary Railroad IR frame has one argument")
                .node,
        );
        let node = match frame.kind {
            RailroadIrExpressionFrameKind::Optional => RailroadAstNode::Optional { element, span },
            RailroadIrExpressionFrameKind::OneOrMore => RailroadAstNode::Repetition {
                element,
                min: RailroadRepeatBound::ONE,
                max: RailroadRepeatBound::INFINITY,
                separator: None,
                span,
            },
            RailroadIrExpressionFrameKind::ZeroOrMore => RailroadAstNode::Repetition {
                element,
                min: RailroadRepeatBound::ZERO,
                max: RailroadRepeatBound::INFINITY,
                separator: None,
                span,
            },
            RailroadIrExpressionFrameKind::Sequence | RailroadIrExpressionFrameKind::Choice => {
                unreachable!("variadic Railroad IR frames collapse before unary wrapping")
            }
        };
        Ok(RailroadConstructedNode { node, depth })
    }

    fn parse_ebnf_choice(&mut self, nesting_depth: usize) -> Result<RailroadConstructedNode> {
        let mut frames = vec![RailroadEbnfFrame {
            kind: RailroadEbnfFrameKind::Root,
            nesting_depth,
            alternatives: Vec::new(),
            sequence: Vec::new(),
            state: RailroadEbnfFrameState::ExpectPrimary,
        }];
        let mut completed = None;

        loop {
            if let Some(node) = completed.take() {
                let frame = frames
                    .last_mut()
                    .expect("Railroad EBNF parent frame exists for completed group");
                self.accept_ebnf_primary(frame, node)?;
                continue;
            }

            let state_is_pending = matches!(
                &frames
                    .last()
                    .expect("Railroad EBNF frame stack is non-empty")
                    .state,
                RailroadEbnfFrameState::Pending(_)
            );

            if state_is_pending {
                let wrapped_depth =
                    if self.check_symbol('?') || self.check_symbol('*') || self.check_symbol('+') {
                        let frame = frames.last().expect("Railroad EBNF frame exists");
                        let RailroadEbnfFrameState::Pending(node) = &frame.state else {
                            unreachable!("pending Railroad EBNF state was checked above")
                        };
                        self.next_constructed_depth([node.depth])?
                    } else {
                        0
                    };
                if self.take_symbol('?') {
                    let end = self.previous_end();
                    let frame = frames.last_mut().expect("Railroad EBNF frame exists");
                    let node = Self::take_ebnf_pending(frame)?;
                    frame.state = RailroadEbnfFrameState::Pending(RailroadConstructedNode {
                        depth: wrapped_depth,
                        node: RailroadAstNode::Optional {
                            span: SourceSpan::new(node.span().start, end),
                            element: Box::new(node.node),
                        },
                    });
                    continue;
                }
                if self.take_symbol('*') {
                    let end = self.previous_end();
                    let frame = frames.last_mut().expect("Railroad EBNF frame exists");
                    let node = Self::take_ebnf_pending(frame)?;
                    frame.state = RailroadEbnfFrameState::Pending(RailroadConstructedNode {
                        depth: wrapped_depth,
                        node: RailroadAstNode::Repetition {
                            span: SourceSpan::new(node.span().start, end),
                            element: Box::new(node.node),
                            min: RailroadRepeatBound::ZERO,
                            max: RailroadRepeatBound::INFINITY,
                            separator: None,
                        },
                    });
                    continue;
                }
                if self.take_symbol('+') {
                    let end = self.previous_end();
                    let frame = frames.last_mut().expect("Railroad EBNF frame exists");
                    let node = Self::take_ebnf_pending(frame)?;
                    frame.state = RailroadEbnfFrameState::Pending(RailroadConstructedNode {
                        depth: wrapped_depth,
                        node: RailroadAstNode::Repetition {
                            span: SourceSpan::new(node.span().start, end),
                            element: Box::new(node.node),
                            min: RailroadRepeatBound::ONE,
                            max: RailroadRepeatBound::INFINITY,
                            separator: None,
                        },
                    });
                    continue;
                }
                if self.take_symbol('-') {
                    let dash_span = self.previous_span();
                    let frame = frames.last_mut().expect("Railroad EBNF frame exists");
                    let left = Self::take_ebnf_pending(frame)?;
                    frame.state = RailroadEbnfFrameState::ExpectExceptionRhs { left, dash_span };
                    continue;
                }
                if self.take_symbol(',') {
                    let frame = frames.last_mut().expect("Railroad EBNF frame exists");
                    let node = Self::take_ebnf_pending(frame)?;
                    frame.sequence.push(node);
                    continue;
                }
                if self.take_symbol('|') {
                    let frame = frames.last_mut().expect("Railroad EBNF frame exists");
                    let node = Self::take_ebnf_pending(frame)?;
                    frame.sequence.push(node);
                    frame.alternatives.push(std::mem::take(&mut frame.sequence));
                    continue;
                }

                let kind = frames.last().expect("Railroad EBNF frame exists").kind;
                if let Some(closing) = kind.closing_symbol() {
                    if self.check_symbol(closing) {
                        let end = self.expect_symbol(closing, kind.closing_message())?;
                        let frame = frames.pop().expect("Railroad EBNF frame exists");
                        completed = Some(self.finish_ebnf_frame(frame, Some(end.span))?);
                        continue;
                    }
                } else if self.check_symbol(';') || self.is_eof() {
                    let frame = frames.pop().expect("Railroad EBNF root frame exists");
                    return self.finish_ebnf_frame(frame, None);
                }

                if self.is_ebnf_primary_start() {
                    let frame = frames.last_mut().expect("Railroad EBNF frame exists");
                    let node = Self::take_ebnf_pending(frame)?;
                    frame.sequence.push(node);
                    continue;
                }

                return Err(self.error_at_current(kind.closing_message()));
            }

            let (frame_depth, expects_exception_rhs) = {
                let frame = frames
                    .last()
                    .expect("Railroad EBNF frame stack is non-empty");
                (
                    frame.nesting_depth,
                    matches!(
                        &frame.state,
                        RailroadEbnfFrameState::ExpectExceptionRhs { .. }
                    ),
                )
            };
            self.ensure_nesting_depth(frame_depth)?;

            if self.take_symbol('(') {
                let start = self.previous_span().start;
                frames.push(RailroadEbnfFrame {
                    kind: RailroadEbnfFrameKind::Group { start },
                    nesting_depth: self.next_nesting_depth(frame_depth)?,
                    alternatives: Vec::new(),
                    sequence: Vec::new(),
                    state: RailroadEbnfFrameState::ExpectPrimary,
                });
                continue;
            }
            if self.take_symbol('[') {
                let start = self.previous_span().start;
                frames.push(RailroadEbnfFrame {
                    kind: RailroadEbnfFrameKind::Optional { start },
                    nesting_depth: self.next_nesting_depth(frame_depth)?,
                    alternatives: Vec::new(),
                    sequence: Vec::new(),
                    state: RailroadEbnfFrameState::ExpectPrimary,
                });
                continue;
            }
            if self.take_symbol('{') {
                let start = self.previous_span().start;
                frames.push(RailroadEbnfFrame {
                    kind: RailroadEbnfFrameKind::Repetition { start },
                    nesting_depth: self.next_nesting_depth(frame_depth)?,
                    alternatives: Vec::new(),
                    sequence: Vec::new(),
                    state: RailroadEbnfFrameState::ExpectPrimary,
                });
                continue;
            }

            let node = if let Some(value) = self.take_string() {
                RailroadAstNode::Terminal {
                    value: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else if let Some(value) = self.take_special_sequence() {
                RailroadAstNode::Special {
                    text: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else if let Some(value) = self.take_ident() {
                RailroadAstNode::NonTerminal {
                    name: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else if expects_exception_rhs {
                return Err(self.error_at_current("expected EBNF primary"));
            } else {
                return Err(self.error_at_current("expected EBNF expression"));
            };
            let frame = frames.last_mut().expect("Railroad EBNF frame exists");
            self.accept_ebnf_primary(frame, RailroadConstructedNode::leaf(node))?;
        }
    }

    fn accept_ebnf_primary(
        &self,
        frame: &mut RailroadEbnfFrame,
        node: RailroadConstructedNode,
    ) -> Result<()> {
        let state = std::mem::replace(&mut frame.state, RailroadEbnfFrameState::ExpectPrimary);
        frame.state = match state {
            RailroadEbnfFrameState::ExpectPrimary => RailroadEbnfFrameState::Pending(node),
            RailroadEbnfFrameState::ExpectExceptionRhs { left, dash_span } => {
                let depth = self.next_constructed_depth([left.depth, node.depth])?;
                let span = SourceSpan::new(left.span().start, node.span().end);
                RailroadEbnfFrameState::Pending(RailroadConstructedNode {
                    depth,
                    node: RailroadAstNode::Sequence {
                        elements: vec![
                            left.node,
                            RailroadAstNode::Terminal {
                                value: "-".to_string(),
                                span: dash_span,
                                selection: dash_span,
                            },
                            node.node,
                        ],
                        span,
                    },
                })
            }
            RailroadEbnfFrameState::Pending(_) => {
                unreachable!("Railroad EBNF accepts a primary only after committing the prior term")
            }
        };
        Ok(())
    }

    fn take_ebnf_pending(frame: &mut RailroadEbnfFrame) -> Result<RailroadConstructedNode> {
        match std::mem::replace(&mut frame.state, RailroadEbnfFrameState::ExpectPrimary) {
            RailroadEbnfFrameState::Pending(node) => Ok(node),
            RailroadEbnfFrameState::ExpectPrimary
            | RailroadEbnfFrameState::ExpectExceptionRhs { .. } => {
                Err(Error::diagram_parse_fallback(
                    "railroadEbnf".to_string(),
                    "internal EBNF parser state expected a completed term".to_string(),
                ))
            }
        }
    }

    fn finish_ebnf_frame(
        &mut self,
        mut frame: RailroadEbnfFrame,
        closing_span: Option<SourceSpan>,
    ) -> Result<RailroadConstructedNode> {
        let node = Self::take_ebnf_pending(&mut frame)?;
        frame.sequence.push(node);
        frame.alternatives.push(std::mem::take(&mut frame.sequence));
        let alternatives = frame
            .alternatives
            .into_iter()
            .map(|sequence| self.collapse_nonempty_sequence(sequence))
            .collect::<Result<Vec<_>>>()?;
        let element = self.collapse_nonempty_choice(alternatives)?;

        match frame.kind {
            RailroadEbnfFrameKind::Root => Ok(element),
            RailroadEbnfFrameKind::Group { start } => {
                let end = closing_span.expect("EBNF group has a closing span");
                Ok(with_outer_span(element, SourceSpan::new(start, end.end)))
            }
            RailroadEbnfFrameKind::Optional { start } => {
                let end = closing_span.expect("EBNF optional group has a closing span");
                let depth = self.next_constructed_depth([element.depth])?;
                Ok(RailroadConstructedNode {
                    depth,
                    node: RailroadAstNode::Optional {
                        element: Box::new(element.node),
                        span: SourceSpan::new(start, end.end),
                    },
                })
            }
            RailroadEbnfFrameKind::Repetition { start } => {
                let end = closing_span.expect("EBNF repetition has a closing span");
                let depth = self.next_constructed_depth([element.depth])?;
                Ok(RailroadConstructedNode {
                    depth,
                    node: RailroadAstNode::Repetition {
                        element: Box::new(element.node),
                        min: RailroadRepeatBound::ZERO,
                        max: RailroadRepeatBound::INFINITY,
                        separator: None,
                        span: SourceSpan::new(start, end.end),
                    },
                })
            }
        }
    }

    fn parse_abnf_alternation(&mut self, nesting_depth: usize) -> Result<RailroadConstructedNode> {
        let mut frames = vec![RailroadAbnfFrame {
            kind: RailroadAbnfFrameKind::Root,
            nesting_depth,
            alternatives: Vec::new(),
            sequence: Vec::new(),
            pending_repeat: None,
        }];
        let mut completed = None;

        loop {
            if let Some(node) = completed.take() {
                let frame = frames
                    .last_mut()
                    .expect("Railroad ABNF parent frame exists for completed group");
                self.append_abnf_element(frame, node)?;
                continue;
            }

            let (kind, frame_depth, has_elements, has_pending_repeat) = {
                let frame = frames
                    .last()
                    .expect("Railroad ABNF frame stack is non-empty");
                (
                    frame.kind,
                    frame.nesting_depth,
                    !frame.sequence.is_empty(),
                    frame.pending_repeat.is_some(),
                )
            };

            if let Some(closing) = kind.closing_symbol() {
                if self.check_symbol(closing) {
                    if !has_elements || has_pending_repeat {
                        return Err(self.error_at_current("expected ABNF element"));
                    }
                    let end = self.expect_symbol(closing, kind.closing_message())?;
                    let frame = frames.pop().expect("Railroad ABNF frame exists");
                    completed = Some(self.finish_abnf_frame(frame, Some(end.span))?);
                    continue;
                }
            } else if self.check_symbol(';') || self.is_eof() {
                if !has_elements || has_pending_repeat {
                    return Err(self.error_at_current("expected ABNF element"));
                }
                let frame = frames.pop().expect("Railroad ABNF root frame exists");
                return self.finish_abnf_frame(frame, None);
            }

            if self.take_symbol('/') {
                if !has_elements || has_pending_repeat {
                    return Err(self.error_at_current("expected ABNF element"));
                }
                let frame = frames.last_mut().expect("Railroad ABNF frame exists");
                frame.alternatives.push(std::mem::take(&mut frame.sequence));
                continue;
            }

            if !self.is_abnf_element_start() {
                if kind.closing_symbol().is_none() && has_elements && !has_pending_repeat {
                    let frame = frames.pop().expect("Railroad ABNF root frame exists");
                    return self.finish_abnf_frame(frame, None);
                }
                return Err(self.error_at_current(kind.closing_message()));
            }

            self.ensure_nesting_depth(frame_depth)?;
            let repeat = self.take_abnf_repeat();
            if self.take_symbol('(') {
                let start = self.previous_span().start;
                frames
                    .last_mut()
                    .expect("Railroad ABNF frame exists")
                    .pending_repeat = repeat;
                frames.push(RailroadAbnfFrame {
                    kind: RailroadAbnfFrameKind::Group { start },
                    nesting_depth: self.next_nesting_depth(frame_depth)?,
                    alternatives: Vec::new(),
                    sequence: Vec::new(),
                    pending_repeat: None,
                });
                continue;
            }
            if self.take_symbol('[') {
                let start = self.previous_span().start;
                frames
                    .last_mut()
                    .expect("Railroad ABNF frame exists")
                    .pending_repeat = repeat;
                frames.push(RailroadAbnfFrame {
                    kind: RailroadAbnfFrameKind::Optional { start },
                    nesting_depth: self.next_nesting_depth(frame_depth)?,
                    alternatives: Vec::new(),
                    sequence: Vec::new(),
                    pending_repeat: None,
                });
                continue;
            }

            let primary = if let Some(value) = self.take_string() {
                RailroadAstNode::Terminal {
                    value: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else if let Some(value) = self.take_num_val() {
                RailroadAstNode::Terminal {
                    value: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else if let Some(value) = self.take_ident() {
                RailroadAstNode::NonTerminal {
                    name: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else {
                return Err(self.error_at_current("expected ABNF primary"));
            };
            let frame = frames.last_mut().expect("Railroad ABNF frame exists");
            frame.pending_repeat = repeat;
            self.append_abnf_element(frame, RailroadConstructedNode::leaf(primary))?;
        }
    }

    fn append_abnf_element(
        &self,
        frame: &mut RailroadAbnfFrame,
        primary: RailroadConstructedNode,
    ) -> Result<()> {
        let repeat = frame.pending_repeat.take();
        let element = self.apply_abnf_repeat(repeat, primary)?;
        frame.sequence.push(element);
        Ok(())
    }

    fn apply_abnf_repeat(
        &self,
        repeat: Option<SpannedText>,
        primary: RailroadConstructedNode,
    ) -> Result<RailroadConstructedNode> {
        let Some(repeat) = repeat else {
            return Ok(primary);
        };
        let (min, max) = parse_abnf_repeat_bounds(&repeat.text)
            .ok_or_else(|| self.error_at_span(repeat.span, "invalid ABNF repetition bound"))?;
        let span = SourceSpan::new(repeat.span.start, primary.span().end);
        let depth = self.next_constructed_depth([primary.depth])?;
        if min.is_zero() && max.is_one() {
            return Ok(RailroadConstructedNode {
                depth,
                node: RailroadAstNode::Optional {
                    element: Box::new(primary.node),
                    span,
                },
            });
        }
        Ok(RailroadConstructedNode {
            depth,
            node: RailroadAstNode::Repetition {
                element: Box::new(primary.node),
                min,
                max,
                separator: None,
                span,
            },
        })
    }

    fn finish_abnf_frame(
        &mut self,
        mut frame: RailroadAbnfFrame,
        closing_span: Option<SourceSpan>,
    ) -> Result<RailroadConstructedNode> {
        frame.alternatives.push(std::mem::take(&mut frame.sequence));
        let alternatives = frame
            .alternatives
            .into_iter()
            .map(|sequence| self.collapse_nonempty_sequence(sequence))
            .collect::<Result<Vec<_>>>()?;
        let element = self.collapse_nonempty_choice(alternatives)?;

        match frame.kind {
            RailroadAbnfFrameKind::Root => Ok(element),
            RailroadAbnfFrameKind::Group { start } => {
                let end = closing_span.expect("ABNF group has a closing span");
                Ok(with_outer_span(element, SourceSpan::new(start, end.end)))
            }
            RailroadAbnfFrameKind::Optional { start } => {
                let end = closing_span.expect("ABNF optional group has a closing span");
                let depth = self.next_constructed_depth([element.depth])?;
                Ok(RailroadConstructedNode {
                    depth,
                    node: RailroadAstNode::Optional {
                        element: Box::new(element.node),
                        span: SourceSpan::new(start, end.end),
                    },
                })
            }
        }
    }

    fn parse_peg_ordered_choice(
        &mut self,
        nesting_depth: usize,
    ) -> Result<RailroadConstructedNode> {
        let mut frames = vec![RailroadPegFrame {
            kind: RailroadPegFrameKind::Root,
            nesting_depth,
            alternatives: Vec::new(),
            sequence: Vec::new(),
            pending_prefix: None,
        }];
        let mut completed = None;

        loop {
            if let Some(node) = completed.take() {
                let frame = frames
                    .last_mut()
                    .expect("Railroad PEG parent frame exists for completed group");
                self.append_peg_element(frame, node)?;
                continue;
            }

            let (kind, frame_depth, has_elements, has_pending_prefix) = {
                let frame = frames
                    .last()
                    .expect("Railroad PEG frame stack is non-empty");
                (
                    frame.kind,
                    frame.nesting_depth,
                    !frame.sequence.is_empty(),
                    frame.pending_prefix.is_some(),
                )
            };

            if let Some(closing) = kind.closing_symbol() {
                if self.check_symbol(closing) {
                    if !has_elements || has_pending_prefix {
                        return Err(self.error_at_current("expected PEG expression"));
                    }
                    let end = self.expect_symbol(closing, kind.closing_message())?;
                    let frame = frames.pop().expect("Railroad PEG frame exists");
                    completed = Some(self.finish_peg_frame(frame, Some(end.span))?);
                    continue;
                }
            } else if self.check_symbol(';') || self.is_eof() {
                if !has_elements || has_pending_prefix {
                    return Err(self.error_at_current("expected PEG expression"));
                }
                let frame = frames.pop().expect("Railroad PEG root frame exists");
                return self.finish_peg_frame(frame, None);
            }

            if self.take_symbol('/') {
                if !has_elements || has_pending_prefix {
                    return Err(self.error_at_current("expected PEG expression"));
                }
                let frame = frames.last_mut().expect("Railroad PEG frame exists");
                frame.alternatives.push(std::mem::take(&mut frame.sequence));
                continue;
            }

            if !self.is_peg_prefix_start() {
                if kind.closing_symbol().is_none() && has_elements && !has_pending_prefix {
                    let frame = frames.pop().expect("Railroad PEG root frame exists");
                    return self.finish_peg_frame(frame, None);
                }
                return Err(self.error_at_current(kind.closing_message()));
            }

            self.ensure_nesting_depth(frame_depth)?;
            let prefix = if self.take_symbol('&') {
                Some(('&', self.previous_span()))
            } else if self.take_symbol('!') {
                Some(('!', self.previous_span()))
            } else {
                None
            };

            if self.take_symbol('(') {
                let start = self.previous_span().start;
                frames
                    .last_mut()
                    .expect("Railroad PEG frame exists")
                    .pending_prefix = prefix;
                frames.push(RailroadPegFrame {
                    kind: RailroadPegFrameKind::Group { start },
                    nesting_depth: self.next_nesting_depth(frame_depth)?,
                    alternatives: Vec::new(),
                    sequence: Vec::new(),
                    pending_prefix: None,
                });
                continue;
            }

            let primary = if let Some(value) = self.take_string() {
                RailroadAstNode::Terminal {
                    value: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else if let Some(value) = self.take_ident() {
                RailroadAstNode::NonTerminal {
                    name: value.text,
                    span: value.span,
                    selection: value.selection,
                }
            } else if self.take_symbol('.') {
                let span = self.previous_span();
                RailroadAstNode::Special {
                    text: ".".to_string(),
                    span,
                    selection: span,
                }
            } else {
                return Err(self.error_at_current("expected PEG primary"));
            };
            let frame = frames.last_mut().expect("Railroad PEG frame exists");
            frame.pending_prefix = prefix;
            self.append_peg_element(frame, RailroadConstructedNode::leaf(primary))?;
        }
    }

    fn append_peg_element(
        &mut self,
        frame: &mut RailroadPegFrame,
        primary: RailroadConstructedNode,
    ) -> Result<()> {
        let suffix = self.apply_peg_suffix(primary)?;
        let element = self.apply_peg_prefix(frame.pending_prefix.take(), suffix);
        frame.sequence.push(element);
        Ok(())
    }

    fn apply_peg_suffix(
        &mut self,
        primary: RailroadConstructedNode,
    ) -> Result<RailroadConstructedNode> {
        let depth = if self.check_symbol('?') || self.check_symbol('*') || self.check_symbol('+') {
            self.next_constructed_depth([primary.depth])?
        } else {
            return Ok(primary);
        };
        if self.take_symbol('?') {
            return Ok(RailroadConstructedNode {
                depth,
                node: RailroadAstNode::Optional {
                    span: SourceSpan::new(primary.span().start, self.previous_end()),
                    element: Box::new(primary.node),
                },
            });
        }
        if self.take_symbol('*') {
            return Ok(RailroadConstructedNode {
                depth,
                node: RailroadAstNode::Repetition {
                    span: SourceSpan::new(primary.span().start, self.previous_end()),
                    element: Box::new(primary.node),
                    min: RailroadRepeatBound::ZERO,
                    max: RailroadRepeatBound::INFINITY,
                    separator: None,
                },
            });
        }
        if self.take_symbol('+') {
            return Ok(RailroadConstructedNode {
                depth,
                node: RailroadAstNode::Repetition {
                    span: SourceSpan::new(primary.span().start, self.previous_end()),
                    element: Box::new(primary.node),
                    min: RailroadRepeatBound::ONE,
                    max: RailroadRepeatBound::INFINITY,
                    separator: None,
                },
            });
        }
        unreachable!("a supported Railroad PEG suffix was checked before wrapping")
    }

    fn apply_peg_prefix(
        &mut self,
        prefix: Option<(char, SourceSpan)>,
        suffix: RailroadConstructedNode,
    ) -> RailroadConstructedNode {
        let Some((operator, span)) = prefix else {
            return suffix;
        };
        let label = format!("{operator}{}", node_to_label(&suffix.node));
        let predicate_span = SourceSpan::new(span.start, suffix.span().end);
        self.trace.peg_predicates.push(RailroadPegPredicateTrace {
            span: predicate_span,
            inner: suffix.node.clone(),
        });
        RailroadConstructedNode::leaf(RailroadAstNode::Special {
            text: label,
            span: predicate_span,
            selection: SourceSpan::new(span.start, suffix.selection().end),
        })
    }

    fn finish_peg_frame(
        &mut self,
        mut frame: RailroadPegFrame,
        closing_span: Option<SourceSpan>,
    ) -> Result<RailroadConstructedNode> {
        frame.alternatives.push(std::mem::take(&mut frame.sequence));
        let alternatives = frame
            .alternatives
            .into_iter()
            .map(|sequence| self.collapse_nonempty_sequence(sequence))
            .collect::<Result<Vec<_>>>()?;
        let element = self.collapse_nonempty_choice(alternatives)?;

        match frame.kind {
            RailroadPegFrameKind::Root => Ok(element),
            RailroadPegFrameKind::Group { start } => {
                let end = closing_span.expect("PEG group has a closing span");
                Ok(with_outer_span(element, SourceSpan::new(start, end.end)))
            }
        }
    }

    fn is_ebnf_primary_start(&self) -> bool {
        matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::String(_))
                | Some(TokenKind::SpecialSequence(_))
                | Some(TokenKind::Ident(_))
                | Some(TokenKind::Symbol('(' | '[' | '{'))
        )
    }

    fn is_abnf_element_start(&self) -> bool {
        matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::Repeat(_))
                | Some(TokenKind::Number(_))
                | Some(TokenKind::String(_))
                | Some(TokenKind::NumVal(_))
                | Some(TokenKind::Ident(_))
                | Some(TokenKind::Symbol('(' | '['))
        )
    }

    fn is_peg_prefix_start(&self) -> bool {
        matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::Symbol('&' | '!'))
                | Some(TokenKind::String(_))
                | Some(TokenKind::Ident(_))
                | Some(TokenKind::Symbol('(' | '.'))
        )
    }

    fn take_abnf_repeat(&mut self) -> Option<SpannedText> {
        let token = self.peek()?;
        match &token.kind {
            TokenKind::Repeat(value) | TokenKind::Number(value) => {
                let out = SpannedText {
                    text: value.clone(),
                    span: token.span,
                    selection: token.selection,
                };
                self.pos += 1;
                Some(out)
            }
            _ => None,
        }
    }

    fn expect_ident(&mut self, message: impl Into<String>) -> Result<SpannedText> {
        self.take_ident()
            .ok_or_else(|| self.error_at_current(message))
    }

    fn expect_rule_name(&mut self) -> Result<SpannedText> {
        let name = self.expect_ident("expected railroad rule name")?;
        if let Some(conflict) = railroad_rule_name_conflict(&name.text, self.dialect) {
            let reason = match conflict {
                RailroadRuleNameConflict::TitleToken => "is tokenized as railroad title metadata",
                RailroadRuleNameConflict::HeaderKeyword => "is the railroad dialect header",
                RailroadRuleNameConflict::IrConstructor => "is reserved for railroad expressions",
            };
            return Err(self.error_at_span(name.span, format!("`{}` {reason}", name.text)));
        }
        Ok(name)
    }

    fn ensure_nesting_depth(&self, nesting_depth: usize) -> Result<()> {
        if nesting_depth > MAX_DIAGRAM_NESTING_DEPTH {
            return Err(self.error_at_current(format!(
                "railroad nesting depth exceeds {MAX_DIAGRAM_NESTING_DEPTH}"
            )));
        }
        Ok(())
    }

    fn next_nesting_depth(&self, nesting_depth: usize) -> Result<usize> {
        let next = nesting_depth.saturating_add(1);
        self.ensure_nesting_depth(next)?;
        Ok(next)
    }

    fn next_constructed_depth(&self, depths: impl IntoIterator<Item = usize>) -> Result<usize> {
        let next = depths.into_iter().max().unwrap_or(0).saturating_add(1);
        self.ensure_nesting_depth(next)?;
        Ok(next)
    }

    fn collapse_sequence(
        &self,
        elements: Vec<RailroadConstructedNode>,
        span: SourceSpan,
    ) -> Result<RailroadConstructedNode> {
        if elements.len() == 1 {
            return Ok(elements.into_iter().next().expect("one element"));
        }
        let depth = self.next_constructed_depth(elements.iter().map(|node| node.depth))?;
        Ok(RailroadConstructedNode {
            depth,
            node: RailroadAstNode::Sequence {
                elements: elements.into_iter().map(|node| node.node).collect(),
                span,
            },
        })
    }

    fn collapse_choice(
        &self,
        alternatives: Vec<RailroadConstructedNode>,
        span: SourceSpan,
    ) -> Result<RailroadConstructedNode> {
        if alternatives.len() == 1 {
            return Ok(alternatives.into_iter().next().expect("one alternative"));
        }
        let depth = self.next_constructed_depth(alternatives.iter().map(|node| node.depth))?;
        Ok(RailroadConstructedNode {
            depth,
            node: RailroadAstNode::Choice {
                alternatives: alternatives.into_iter().map(|node| node.node).collect(),
                span,
            },
        })
    }

    fn collapse_nonempty_sequence(
        &self,
        elements: Vec<RailroadConstructedNode>,
    ) -> Result<RailroadConstructedNode> {
        let start = elements
            .first()
            .expect("Railroad sequence has an element")
            .span()
            .start;
        let end = elements
            .last()
            .expect("Railroad sequence has an element")
            .span()
            .end;
        self.collapse_sequence(elements, SourceSpan::new(start, end))
    }

    fn collapse_nonempty_choice(
        &self,
        alternatives: Vec<RailroadConstructedNode>,
    ) -> Result<RailroadConstructedNode> {
        let start = alternatives
            .first()
            .expect("Railroad choice has an alternative")
            .span()
            .start;
        let end = alternatives
            .last()
            .expect("Railroad choice has an alternative")
            .span()
            .end;
        self.collapse_choice(alternatives, SourceSpan::new(start, end))
    }

    fn take_ident(&mut self) -> Option<SpannedText> {
        let token = self.peek()?;
        let TokenKind::Ident(value) = &token.kind else {
            return None;
        };
        let out = SpannedText {
            text: value.clone(),
            span: token.span,
            selection: token.selection,
        };
        self.pos += 1;
        Some(out)
    }

    fn expect_string(&mut self, message: impl Into<String>) -> Result<SpannedText> {
        self.take_string()
            .ok_or_else(|| self.error_at_current(message))
    }

    fn take_string(&mut self) -> Option<SpannedText> {
        let token = self.peek()?;
        let TokenKind::String(value) = &token.kind else {
            return None;
        };
        let out = SpannedText {
            text: value.clone(),
            span: token.span,
            selection: token.selection,
        };
        self.pos += 1;
        Some(out)
    }

    fn take_special_sequence(&mut self) -> Option<SpannedText> {
        let token = self.peek()?;
        let TokenKind::SpecialSequence(value) = &token.kind else {
            return None;
        };
        let out = SpannedText {
            text: value.clone(),
            span: token.span,
            selection: token.selection,
        };
        self.pos += 1;
        Some(out)
    }

    fn take_num_val(&mut self) -> Option<SpannedText> {
        let token = self.peek()?;
        let TokenKind::NumVal(value) = &token.kind else {
            return None;
        };
        let out = SpannedText {
            text: value.clone(),
            span: token.span,
            selection: token.selection,
        };
        self.pos += 1;
        Some(out)
    }

    fn expect_symbol(&mut self, symbol: char, message: impl Into<String>) -> Result<Token> {
        if self.take_symbol(symbol) {
            Ok(self.previous_token().expect("previous token").clone())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn take_symbol(&mut self, symbol: char) -> bool {
        if self.check_symbol(symbol) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn check_symbol(&self, symbol: char) -> bool {
        matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::Symbol(ch)) if *ch == symbol
        )
    }

    fn take_colon_colon_eq(&mut self) -> bool {
        if matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::ColonColonEq)
        ) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_left_arrow(&mut self, message: impl Into<String>) -> Result<()> {
        if matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::LeftArrow)
        ) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error_at_current(message))
        }
    }

    fn take(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(token)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn previous_token(&self) -> Option<&Token> {
        self.pos.checked_sub(1).and_then(|pos| self.tokens.get(pos))
    }

    fn previous_span(&self) -> SourceSpan {
        self.previous_token()
            .map(|token| token.span)
            .unwrap_or_else(|| SourceSpan::new(self.input_len, self.input_len))
    }

    fn previous_end(&self) -> usize {
        self.previous_span().end
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn error_at_current(&self, message: impl Into<String>) -> Error {
        if let Some(token) = self.peek() {
            self.error_at_token(token, message)
        } else {
            Error::diagram_parse_insertion_point(self.diagram_type, message, self.input_len)
        }
    }

    fn error_at_token(&self, token: &Token, message: impl Into<String>) -> Error {
        Error::diagram_parse_exact(self.diagram_type, message, token.span)
    }

    fn error_at_span(&self, span: SourceSpan, message: impl Into<String>) -> Error {
        Error::diagram_parse_exact(self.diagram_type, message, span)
    }
}

#[derive(Debug, Clone)]
struct SpannedCommonField {
    kind: CommonFieldKind,
    value: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpannedText {
    text: String,
    span: SourceSpan,
    selection: SourceSpan,
}

struct Lexer<'a> {
    input: &'a str,
    dialect: RailroadDialect,
    diagram_type: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str, dialect: RailroadDialect, diagram_type: &'a str) -> Self {
        Self {
            input,
            dialect,
            diagram_type,
            pos: 0,
        }
    }

    fn tokenize_recovering(
        mut self,
        control: &OperationControl,
    ) -> OperationControlResult<RailroadLexerOutcome> {
        let mut tokens = Vec::new();
        let mut first_error = None;
        loop {
            control.checkpoint()?;
            match self.skip_trivia(control) {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    first_error.get_or_insert(error);
                    if self.is_eof() {
                        break;
                    }
                    self.advance_char();
                    continue;
                }
            }
            if self.is_eof() {
                break;
            }
            match self.take_common_field() {
                Ok(Some(token)) => {
                    tokens.push(token);
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                    if self.is_eof() {
                        break;
                    }
                    self.advance_char();
                    continue;
                }
            }
            match self.take_string() {
                Ok(Some(token)) => {
                    tokens.push(token);
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                    if self.is_eof() {
                        break;
                    }
                    self.advance_char();
                    continue;
                }
            }
            if self.dialect == RailroadDialect::Ebnf
                && let Some(token) = self.take_ebnf_special_sequence()
            {
                tokens.push(token);
                continue;
            }
            if self.dialect == RailroadDialect::Abnf {
                if let Some(token) = self.take_abnf_num_val() {
                    tokens.push(token);
                    continue;
                }
                if let Some(token) = self.take_abnf_repeat_or_number() {
                    tokens.push(token);
                    continue;
                }
            }
            if let Some(token) = self.take_identifier() {
                tokens.push(token);
                continue;
            }
            if let Some(token) = self.take_compound_symbol() {
                tokens.push(token);
                continue;
            }
            if let Some(token) = self.take_single_symbol() {
                tokens.push(token);
                continue;
            }

            let start = self.pos;
            let ch = self.current_char().expect("not eof");
            first_error.get_or_insert_with(|| {
                Error::diagram_parse_exact(
                    self.diagram_type,
                    format!("unexpected railroad token `{ch}`"),
                    SourceSpan::new(start, start + ch.len_utf8()),
                )
            });
            self.advance_char();
        }

        control.checkpoint()?;
        Ok(RailroadLexerOutcome {
            tokens,
            first_error,
        })
    }

    fn skip_trivia(&mut self, control: &OperationControl) -> Result<bool> {
        loop {
            let before = self.pos;
            let mut last_checkpoint = self.pos;
            while self.current_char().is_some_and(char::is_whitespace) {
                self.advance_char();
                if self.pos.saturating_sub(last_checkpoint) >= 4096 {
                    if control.is_cancelled() {
                        return Ok(false);
                    }
                    last_checkpoint = self.pos;
                }
            }
            if control.is_cancelled() {
                return Ok(false);
            }

            if self.starts_with("%%{") {
                if let Some(end) = self.remaining().find("}%%") {
                    self.pos += end + "}%%".len();
                    continue;
                }
                self.pos = self.input.len();
                continue;
            }
            if self.starts_with("%%") {
                self.skip_to_line_end();
                continue;
            }
            if matches!(self.dialect, RailroadDialect::Ir | RailroadDialect::Ebnf)
                && self.starts_with("/*")
            {
                let result = self.skip_until("*/", "unterminated railroad block comment");
                result?;
                continue;
            }
            if self.dialect == RailroadDialect::Ebnf && self.starts_with("(*") {
                let result = self.skip_until("*)", "unterminated EBNF comment");
                result?;
                continue;
            }
            if self.dialect == RailroadDialect::Peg && self.starts_with("#") {
                self.skip_to_line_end();
                continue;
            }
            if self.dialect == RailroadDialect::Abnf && self.abnf_semicolon_starts_comment() {
                self.skip_to_line_end();
                continue;
            }
            if self.skip_yaml_fence() {
                continue;
            }

            if self.pos == before {
                break;
            }
        }
        Ok(!self.is_eof())
    }

    fn take_common_field(&mut self) -> Result<Option<Token>> {
        let start = self.pos;
        let rest = self.remaining();
        let trimmed = rest.trim_start_matches([' ', '\t']);
        let leading = rest.len() - trimmed.len();
        if leading > 0 && rest[..leading].contains(['\r', '\n']) {
            return Ok(None);
        }
        let token_start = start + leading;
        let title_end = token_start + "title".len();
        if self.input[token_start..].starts_with("title")
            && self.input[title_end..]
                .chars()
                .next()
                .is_some_and(|ch| !matches!(ch, ' ' | '\t' | '\r' | '\n'))
        {
            self.pos = title_end;
            let span = SourceSpan::new(token_start, title_end);
            return Ok(Some(Token {
                kind: TokenKind::Common(ParsedCommonField {
                    kind: CommonFieldKind::Title,
                    value: SpannedText {
                        text: String::new(),
                        span: SourceSpan::new(title_end, title_end),
                        selection: SourceSpan::new(title_end, title_end),
                    },
                }),
                span,
                selection: SourceSpan::new(title_end, title_end),
            }));
        }
        let line_end = self.input[token_start..]
            .find(['\r', '\n'])
            .map(|rel| token_start + rel)
            .unwrap_or(self.input.len());
        let line = &self.input[token_start..line_end];

        if let Some(field) = parse_common_field_line(line, token_start, self.dialect) {
            self.pos = line_end;
            let selection = field.value.selection;
            return Ok(Some(Token {
                kind: TokenKind::Common(field),
                span: SourceSpan::new(token_start, line_end),
                selection,
            }));
        }

        if line.trim_start().starts_with("accDescr")
            && line.contains('{')
            && !line.contains('}')
            && let Some(end_rel) = self.input[line_end..].find('}')
        {
            let end = line_end + end_rel + 1;
            let full = &self.input[token_start..end];
            if let Some(field) = parse_common_field_block(full, token_start) {
                self.pos = end;
                let selection = field.value.selection;
                return Ok(Some(Token {
                    kind: TokenKind::Common(field),
                    span: SourceSpan::new(token_start, end),
                    selection,
                }));
            }
        }

        Ok(None)
    }

    fn take_string(&mut self) -> Result<Option<Token>> {
        let Some(quote) = self.current_char() else {
            return Ok(None);
        };
        if !matches!(quote, '"' | '\'') {
            return Ok(None);
        }
        if self.dialect == RailroadDialect::Abnf && quote != '"' {
            return Ok(None);
        }

        let start = self.pos;
        self.advance_char();
        let content_start = self.pos;
        let mut value = String::new();
        let mut escaped = false;
        while let Some(ch) = self.current_char() {
            let ch_start = self.pos;
            self.advance_char();
            if self.dialect != RailroadDialect::Abnf && is_railroad_string_line_terminator(ch) {
                return Err(Error::diagram_parse_exact(
                    self.diagram_type,
                    "physical line breaks are not allowed in railroad string literals",
                    SourceSpan::new(ch_start, self.pos),
                ));
            }
            if self.dialect != RailroadDialect::Abnf && escaped {
                match ch {
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    other => value.push(other),
                }
                escaped = false;
                continue;
            }
            if self.dialect != RailroadDialect::Abnf && ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                return Ok(Some(Token {
                    kind: TokenKind::String(value),
                    span: SourceSpan::new(start, self.pos),
                    selection: SourceSpan::new(content_start, ch_start),
                }));
            }
            value.push(ch);
        }

        Err(Error::diagram_parse_insertion_point(
            self.diagram_type,
            "unterminated railroad string literal",
            start,
        ))
    }

    fn take_ebnf_special_sequence(&mut self) -> Option<Token> {
        if !self.starts_with("?") {
            return None;
        }
        let start = self.pos;
        let rest = &self.remaining()[1..];
        let end_rel = rest.find('?')?;
        let content = &rest[..end_rel];
        if content.contains(';') || content.trim().is_empty() {
            return None;
        }
        self.pos += 1 + end_rel + 1;
        Some(Token {
            kind: TokenKind::SpecialSequence(content.trim().to_string()),
            span: SourceSpan::new(start, self.pos),
            selection: SourceSpan::new(start + 1, start + 1 + content.len()),
        })
    }

    fn take_abnf_num_val(&mut self) -> Option<Token> {
        if !self.starts_with("%") {
            return None;
        }
        let start = self.pos;
        let bytes = self.input.as_bytes();
        let mut pos = start + 1;
        let base = *bytes.get(pos)?;
        if !matches!(base, b'x' | b'X' | b'd' | b'D' | b'b' | b'B') {
            return None;
        }
        pos += 1;
        let digits_start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_hexdigit() {
            pos += 1;
        }
        if pos == digits_start {
            return None;
        }
        while pos < bytes.len() && matches!(bytes[pos], b'-' | b'.') {
            pos += 1;
            let chunk_start = pos;
            while pos < bytes.len() && bytes[pos].is_ascii_hexdigit() {
                pos += 1;
            }
            if pos == chunk_start {
                return None;
            }
        }
        self.pos = pos;
        Some(Token {
            kind: TokenKind::NumVal(self.input[start..pos].to_string()),
            span: SourceSpan::new(start, pos),
            selection: SourceSpan::new(start, pos),
        })
    }

    fn take_abnf_repeat_or_number(&mut self) -> Option<Token> {
        let start = self.pos;
        let bytes = self.input.as_bytes();
        let mut pos = start;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos < bytes.len() && bytes[pos] == b'*' {
            pos += 1;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            self.pos = pos;
            return Some(Token {
                kind: TokenKind::Repeat(self.input[start..pos].to_string()),
                span: SourceSpan::new(start, pos),
                selection: SourceSpan::new(start, pos),
            });
        }

        if pos > start {
            self.pos = pos;
            return Some(Token {
                kind: TokenKind::Number(self.input[start..pos].to_string()),
                span: SourceSpan::new(start, pos),
                selection: SourceSpan::new(start, pos),
            });
        }

        if bytes.get(start) == Some(&b'*') {
            self.pos += 1;
            while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            return Some(Token {
                kind: TokenKind::Repeat(self.input[start..self.pos].to_string()),
                span: SourceSpan::new(start, self.pos),
                selection: SourceSpan::new(start, self.pos),
            });
        }

        None
    }

    fn take_identifier(&mut self) -> Option<Token> {
        let start = self.pos;
        let first = self.current_char()?;
        if !self.dialect.is_identifier_start(first) {
            return None;
        }

        self.advance_char();
        while let Some(ch) = self.current_char() {
            if !self.dialect.is_identifier_continue(ch) {
                break;
            }
            self.advance_char();
        }

        Some(Token {
            kind: TokenKind::Ident(self.input[start..self.pos].to_string()),
            span: SourceSpan::new(start, self.pos),
            selection: SourceSpan::new(start, self.pos),
        })
    }

    fn take_compound_symbol(&mut self) -> Option<Token> {
        let start = self.pos;
        if self.starts_with("::=") {
            self.pos += 3;
            return Some(Token {
                kind: TokenKind::ColonColonEq,
                span: SourceSpan::new(start, self.pos),
                selection: SourceSpan::new(start, self.pos),
            });
        }
        if self.starts_with("<-") {
            self.pos += 2;
            return Some(Token {
                kind: TokenKind::LeftArrow,
                span: SourceSpan::new(start, self.pos),
                selection: SourceSpan::new(start, self.pos),
            });
        }
        None
    }

    fn take_single_symbol(&mut self) -> Option<Token> {
        let ch = self.current_char()?;
        let symbols = match self.dialect {
            RailroadDialect::Ir => "=;,()",
            RailroadDialect::Ebnf => "=;,|()[]{}?*+-",
            RailroadDialect::Abnf => "=;/()[]",
            RailroadDialect::Peg => ";/()&!?.*+",
        };
        if !symbols.contains(ch) {
            return None;
        }

        let start = self.pos;
        self.advance_char();
        Some(Token {
            kind: TokenKind::Symbol(ch),
            span: SourceSpan::new(start, self.pos),
            selection: SourceSpan::new(start, self.pos),
        })
    }

    fn skip_until(&mut self, needle: &str, message: &'static str) -> Result<()> {
        let start = self.pos;
        let Some(end) = self.remaining().find(needle) else {
            self.pos = self.input.len();
            return Err(Error::diagram_parse_insertion_point(
                self.diagram_type,
                message,
                start,
            ));
        };
        self.pos += end + needle.len();
        Ok(())
    }

    fn skip_to_line_end(&mut self) {
        let rel = self
            .remaining()
            .find(['\r', '\n'])
            .unwrap_or(self.remaining().len());
        self.pos += rel;
    }

    fn abnf_semicolon_starts_comment(&self) -> bool {
        if !self.starts_with(";") {
            return false;
        }
        let line_start = self.input[..self.pos]
            .rfind(['\r', '\n'])
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.input[line_start..self.pos].trim().is_empty()
    }

    fn skip_yaml_fence(&mut self) -> bool {
        if !self.starts_with("---") || !self.at_line_start_after_indent() {
            return false;
        }

        let mut cursor = self.pos + "---".len();
        while self.input[cursor..]
            .chars()
            .next()
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
        {
            cursor += self.input[cursor..]
                .chars()
                .next()
                .expect("checked above")
                .len_utf8();
        }
        if self.input[cursor..].starts_with("\r\n") {
            cursor += 2;
        } else if self.input[cursor..].starts_with('\n') {
            cursor += 1;
        } else {
            return false;
        }

        loop {
            let line_end = self.input[cursor..]
                .find(['\r', '\n'])
                .map(|relative| cursor + relative)
                .unwrap_or(self.input.len());
            if self.input[cursor..].starts_with("---") {
                let marker_end = cursor + "---".len();
                if self.input[marker_end..]
                    .chars()
                    .next()
                    .is_none_or(|ch| ch.is_whitespace())
                {
                    self.pos = marker_end;
                    return true;
                }
            }
            if line_end == self.input.len() {
                return false;
            }
            cursor = line_end
                + if self.input[line_end..].starts_with("\r\n") {
                    2
                } else {
                    1
                };
        }
    }

    fn at_line_start_after_indent(&self) -> bool {
        let line_start = self.input[..self.pos]
            .rfind(['\r', '\n'])
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.input[line_start..self.pos]
            .chars()
            .all(|ch| matches!(ch, ' ' | '\t'))
    }

    fn starts_with(&self, literal: &str) -> bool {
        self.remaining().starts_with(literal)
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn current_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.current_char() {
            self.pos += ch.len_utf8();
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedCommonField {
    kind: CommonFieldKind,
    value: SpannedText,
}

fn parse_common_field_line(
    line: &str,
    line_start: usize,
    dialect: RailroadDialect,
) -> Option<ParsedCommonField> {
    let stripped = strip_inline_comment_aware(line);
    parse_title_spanned(stripped, line_start, dialect)
        .or_else(|| parse_acc_title_spanned(stripped, line_start))
        .or_else(|| parse_acc_descr_spanned(stripped, line_start))
}

fn parse_common_field_block(block: &str, block_start: usize) -> Option<ParsedCommonField> {
    let trimmed = block.trim_start();
    let leading = block.len() - trimmed.len();
    let keyword_start = block_start + leading;
    let keyword_end = keyword_start + "accDescr".len();
    let after_keyword = trimmed.strip_prefix("accDescr")?;
    let rest = after_keyword.trim_start();
    let rest_start = keyword_end + (after_keyword.len() - rest.len());
    let body = rest.strip_prefix('{')?;
    let opening = SourceSpan::new(rest_start, rest_start + 1);
    let close_rel = body.find('}')?;
    let raw = &body[..close_rel];
    let (_, value_span) = trimmed_value_span(raw, opening.end);
    Some(ParsedCommonField {
        kind: CommonFieldKind::AccDescr,
        value: SpannedText {
            text: normalize_multiline_common(raw),
            span: value_span,
            selection: value_span,
        },
    })
}

fn parse_title_spanned(
    line: &str,
    line_start: usize,
    dialect: RailroadDialect,
) -> Option<ParsedCommonField> {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();
    let keyword_start = line_start + leading;
    let keyword_end = keyword_start + "title".len();
    if trimmed == "title" {
        return Some(ParsedCommonField {
            kind: CommonFieldKind::Title,
            value: SpannedText {
                text: String::new(),
                span: SourceSpan::new(keyword_end, keyword_end),
                selection: SourceSpan::new(keyword_end, keyword_end),
            },
        });
    }
    let rest = trimmed.strip_prefix("title")?;
    let ws = rest.chars().next()?;
    if !matches!(ws, ' ' | '\t') {
        return None;
    }
    let (raw, raw_span) = trimmed_value_span(rest, keyword_end);
    let collapsed = collapse_common_spaces(raw);
    let decoded = decode_wrapped_quoted_title(&collapsed, dialect);
    let (value, selection) = if let Some(value) = decoded {
        let opening = SourceSpan::new(raw_span.start, raw_span.start + 1);
        let closing = SourceSpan::new(raw_span.end - 1, raw_span.end);
        (value, SourceSpan::new(opening.end, closing.start))
    } else {
        (collapsed, raw_span)
    };
    Some(ParsedCommonField {
        kind: CommonFieldKind::Title,
        value: SpannedText {
            text: value,
            span: raw_span,
            selection,
        },
    })
}

fn parse_acc_title_spanned(line: &str, line_start: usize) -> Option<ParsedCommonField> {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();
    let keyword_start = line_start + leading;
    let keyword_end = keyword_start + "accTitle".len();
    let after_keyword = trimmed.strip_prefix("accTitle")?;
    let rest = after_keyword.trim_start_matches([' ', '\t']);
    let colon_start = keyword_end + (after_keyword.len() - rest.len());
    let value_region = rest.strip_prefix(':')?;
    let (raw, value_span) = trimmed_value_span(value_region, colon_start + 1);
    Some(ParsedCommonField {
        kind: CommonFieldKind::AccTitle,
        value: SpannedText {
            text: collapse_common_spaces(raw),
            span: value_span,
            selection: value_span,
        },
    })
}

fn parse_acc_descr_spanned(line: &str, line_start: usize) -> Option<ParsedCommonField> {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();
    let keyword_start = line_start + leading;
    let keyword_end = keyword_start + "accDescr".len();
    let after_keyword = trimmed.strip_prefix("accDescr")?;
    let rest = after_keyword.trim_start_matches([' ', '\t']);
    let delimiter_start = keyword_end + (after_keyword.len() - rest.len());
    let (raw, value_span) = if let Some(value_region) = rest.strip_prefix(':') {
        let (raw, value_span) = trimmed_value_span(value_region, delimiter_start + 1);
        (raw, value_span)
    } else {
        let body = rest.strip_prefix('{')?;
        let close_rel = body.find('}')?;
        let (raw, value_span) = trimmed_value_span(&body[..close_rel], delimiter_start + 1);
        (raw, value_span)
    };
    Some(ParsedCommonField {
        kind: CommonFieldKind::AccDescr,
        value: SpannedText {
            text: collapse_common_spaces(raw),
            span: value_span,
            selection: value_span,
        },
    })
}

fn trimmed_value_span(value: &str, base_offset: usize) -> (&str, SourceSpan) {
    let trimmed_start = value.trim_start();
    if trimmed_start.is_empty() {
        let insertion = base_offset + value.len();
        return ("", SourceSpan::new(insertion, insertion));
    }
    let leading = value.len() - trimmed_start.len();
    let trimmed = trimmed_start.trim_end();
    let start = base_offset + leading;
    (trimmed, SourceSpan::new(start, start + trimmed.len()))
}

fn collapse_common_spaces(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_space = false;
    for ch in value.chars() {
        if matches!(ch, ' ' | '\t') {
            if !previous_space {
                out.push(' ');
                previous_space = true;
            }
        } else {
            out.push(ch);
            previous_space = false;
        }
    }
    out
}

fn normalize_multiline_common(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for line in value
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !normalized.is_empty() {
            normalized.push('\n');
        }
        normalized.push_str(line);
    }
    normalized
}

fn decode_wrapped_quoted_title(value: &str, dialect: RailroadDialect) -> Option<String> {
    if !((value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\'')))
    {
        return None;
    }
    if dialect == RailroadDialect::Abnf {
        return Some(value[1..value.len() - 1].to_string());
    }
    decode_escaped_quoted_string(value).map(|text| text.text)
}

fn is_railroad_string_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn decode_escaped_quoted_string(value: &str) -> Option<SpannedText> {
    let mut pos = 0usize;
    take_quoted_string_in(value, &mut pos, 0, RailroadDialect::Ir)
        .ok()
        .flatten()
        .filter(|_| pos == value.len())
}

fn take_quoted_string_in(
    input: &str,
    pos: &mut usize,
    base_offset: usize,
    dialect: RailroadDialect,
) -> Result<Option<SpannedText>> {
    let Some(quote) = input[*pos..].chars().next() else {
        return Ok(None);
    };
    if !matches!(quote, '"' | '\'') {
        return Ok(None);
    }
    if dialect == RailroadDialect::Abnf && quote != '"' {
        return Ok(None);
    }

    let start = base_offset + *pos;
    *pos += quote.len_utf8();
    let content_start = base_offset + *pos;
    let mut text = String::new();
    let mut escaped = false;
    while let Some(ch) = input[*pos..].chars().next() {
        let ch_start = base_offset + *pos;
        *pos += ch.len_utf8();
        if dialect != RailroadDialect::Abnf && escaped {
            match ch {
                'n' => text.push('\n'),
                'r' => text.push('\r'),
                't' => text.push('\t'),
                other => text.push(other),
            }
            escaped = false;
            continue;
        }
        if dialect != RailroadDialect::Abnf && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Ok(Some(SpannedText {
                text,
                span: SourceSpan::new(start, base_offset + *pos),
                selection: SourceSpan::new(content_start, ch_start),
            }));
        }
        text.push(ch);
    }
    Ok(None)
}

fn strip_inline_comment_aware(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut iter = line.char_indices().peekable();
    while let Some((idx, ch)) = iter.next() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '%' if !in_single
                && !in_double
                && iter.peek().is_some_and(|(_, next)| *next == '%') =>
            {
                return &line[..idx];
            }
            _ => {}
        }
    }
    line
}

fn parse_abnf_repeat_bounds(repeat: &str) -> Option<(RailroadRepeatBound, RailroadRepeatBound)> {
    let parse_bound = |bound: &str| {
        bound
            .parse::<f64>()
            .ok()
            .and_then(|value| RailroadRepeatBound::try_from(value).ok())
    };

    if let Some((min, max)) = repeat.split_once('*') {
        let min = if min.is_empty() {
            RailroadRepeatBound::ZERO
        } else {
            parse_bound(min)?
        };
        let max = if max.is_empty() {
            RailroadRepeatBound::INFINITY
        } else {
            parse_bound(max)?
        };
        return Some((min, max));
    }

    let exact = parse_bound(repeat)?;
    Some((exact, exact))
}

fn with_outer_span(mut node: RailroadConstructedNode, span: SourceSpan) -> RailroadConstructedNode {
    match &mut node.node {
        RailroadAstNode::Terminal { span: inner, .. }
        | RailroadAstNode::NonTerminal { span: inner, .. }
        | RailroadAstNode::Sequence { span: inner, .. }
        | RailroadAstNode::Choice { span: inner, .. }
        | RailroadAstNode::Optional { span: inner, .. }
        | RailroadAstNode::Repetition { span: inner, .. }
        | RailroadAstNode::Special { span: inner, .. } => *inner = span,
    }
    node
}

fn node_to_label(node: &RailroadAstNode) -> String {
    match node {
        RailroadAstNode::Terminal { value, .. } => format!("\"{value}\""),
        RailroadAstNode::NonTerminal { name, .. } => name.clone(),
        RailroadAstNode::Special { text, .. } => text.clone(),
        _ => "(...)".to_string(),
    }
}

fn editor_facts_from_model(
    model: &RailroadDiagramModel,
    dialect: RailroadDialect,
    trace: &RailroadParserTrace,
) -> EditorSemanticFacts {
    let mut facts = EditorSemanticFacts::new();
    let detail_prefix = dialect.common_detail_prefix();
    let rename_policy = dialect.editor_rename_policy();

    if let (Some(title), Some(span)) = (&model.title, model.title_span) {
        facts.push_directive_prefix("title");
        push_payload_fact(
            &mut facts,
            title.clone(),
            span,
            format!("{detail_prefix} title"),
        );
    }
    if let (Some(acc_title), Some(span)) = (&model.acc_title, model.acc_title_span) {
        facts.push_directive_prefix("accTitle");
        push_payload_fact(
            &mut facts,
            acc_title.clone(),
            span,
            format!("{detail_prefix} accessibility title"),
        );
    }
    if let (Some(acc_descr), Some(span)) = (&model.acc_descr, model.acc_descr_span) {
        facts.push_directive_prefix("accDescr");
        push_payload_fact(
            &mut facts,
            acc_descr.clone(),
            span,
            format!("{detail_prefix} accessibility description"),
        );
    }

    for rule in &model.rules {
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::NodeIdentifier,
            rule.name_span,
        ));
        facts.push_symbol(
            EditorSemanticSymbol::new(
                rule.name.clone(),
                Some(format!("{detail_prefix} rule")),
                EditorSemanticKind::Function,
                rule.name_span,
                rule.name_span,
            )
            .with_rename_policy(rename_policy),
        );
        push_ast_facts(
            &mut facts,
            &rule.definition,
            detail_prefix,
            trace,
            rename_policy,
        );
    }

    facts
}

fn push_ast_facts(
    facts: &mut EditorSemanticFacts,
    node: &RailroadAstNode,
    detail_prefix: &str,
    trace: &RailroadParserTrace,
    rename_policy: EditorRenamePolicy,
) {
    match node {
        RailroadAstNode::Terminal {
            value, selection, ..
        } => {
            push_payload_fact(
                facts,
                value.clone(),
                *selection,
                format!("{detail_prefix} terminal"),
            );
        }
        RailroadAstNode::NonTerminal {
            name,
            span,
            selection,
        } => {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::NodeIdentifier,
                *selection,
            ));
            facts.push_symbol(
                EditorSemanticSymbol::reference(
                    name.clone(),
                    Some(format!("{detail_prefix} nonterminal reference")),
                    EditorSemanticKind::Function,
                    *span,
                    *selection,
                )
                .with_rename_policy(rename_policy),
            );
        }
        RailroadAstNode::Special {
            text,
            span,
            selection,
        } => {
            if let Some(predicate) = trace
                .peg_predicates
                .iter()
                .find(|predicate| predicate.span == *span)
            {
                push_ast_facts(facts, &predicate.inner, detail_prefix, trace, rename_policy);
            } else {
                push_payload_fact(
                    facts,
                    text.clone(),
                    *selection,
                    format!("{detail_prefix} special"),
                );
            }
        }
        RailroadAstNode::Sequence { elements, .. } => {
            for element in elements {
                push_ast_facts(facts, element, detail_prefix, trace, rename_policy);
            }
        }
        RailroadAstNode::Choice { alternatives, .. } => {
            for alternative in alternatives {
                push_ast_facts(facts, alternative, detail_prefix, trace, rename_policy);
            }
        }
        RailroadAstNode::Optional { element, .. } => {
            push_ast_facts(facts, element, detail_prefix, trace, rename_policy)
        }
        RailroadAstNode::Repetition {
            element, separator, ..
        } => {
            push_ast_facts(facts, element, detail_prefix, trace, rename_policy);
            if let Some(separator) = separator {
                push_ast_facts(facts, separator, detail_prefix, trace, rename_policy);
            }
        }
    }
}

fn push_payload_fact(
    facts: &mut EditorSemanticFacts,
    value: String,
    selection: SourceSpan,
    detail: String,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        value,
        Some(detail),
        EditorSemanticKind::String,
        selection,
        selection,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditorSemanticCompleteness, EditorSemanticRole, MermaidConfig, ParseMetadata};

    fn meta(dialect: RailroadDialect) -> ParseMetadata {
        ParseMetadata {
            diagram_type: dialect.diagram_type().to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    fn combined_editor_facts(source: &str, dialect: RailroadDialect) -> EditorSemanticFacts {
        let parser: crate::family::CombinedSemanticParser = match dialect {
            RailroadDialect::Ir => parse_railroad_json_and_editor_facts,
            RailroadDialect::Ebnf => parse_railroad_ebnf_json_and_editor_facts,
            RailroadDialect::Abnf => parse_railroad_abnf_json_and_editor_facts,
            RailroadDialect::Peg => parse_railroad_peg_json_and_editor_facts,
        };
        crate::family::test_support::editor_facts(parser, source, &meta(dialect))
    }

    fn nested_rule_source(dialect: RailroadDialect, nesting_depth: usize) -> String {
        let (assignment, terminal, groups): (&str, &str, &[(&str, &str)]) = match dialect {
            RailroadDialect::Ir => ("=", "terminal(\"value\")", &[("optional(", ")")]),
            RailroadDialect::Ebnf => ("=", "\"value\"", &[("(", ")"), ("[", "]"), ("{", "}")]),
            RailroadDialect::Abnf => ("=", "\"value\"", &[("(", ")"), ("[", "]")]),
            RailroadDialect::Peg => ("<-", "\"value\"", &[("(", ")")]),
        };
        let group_width = groups
            .iter()
            .map(|(opener, closer)| opener.len() + closer.len())
            .max()
            .unwrap_or_default();
        let mut source = String::with_capacity(
            dialect.header().len() + terminal.len() + group_width * nesting_depth,
        );
        source.push_str(dialect.header());
        source.push_str("\nentry ");
        source.push_str(assignment);
        source.push(' ');
        let mut closers = Vec::with_capacity(nesting_depth);
        for index in 0..nesting_depth {
            let (opener, closer) = groups[index % groups.len()];
            source.push_str(opener);
            closers.push(closer);
        }
        source.push_str(terminal);
        for closer in closers.into_iter().rev() {
            source.push_str(closer);
        }
        source.push_str(" ;\nafter ");
        source.push_str(assignment);
        source.push_str(if dialect == RailroadDialect::Ir {
            " terminal(\"later\") ;\n"
        } else {
            " \"later\" ;\n"
        });
        source
    }

    #[test]
    fn decodes_escaped_strings_for_non_abnf_dialects() {
        let mut pos = 0usize;
        let value = take_quoted_string_in(r#""a\n\t\"b\"""#, &mut pos, 0, RailroadDialect::Ir)
            .unwrap()
            .unwrap();
        assert_eq!(value.text, "a\n\t\"b\"");
    }

    #[test]
    fn yaml_fences_require_a_valid_opening_line() {
        let valid = concat!(
            "railroad-beta\n",
            "---\n",
            "title: valid metadata\n",
            "---\n",
            "entry = terminal(\"value\") ;\n",
        );
        assert!(
            parse_railroad_semantic_source(valid, &meta(RailroadDialect::Ir), RailroadDialect::Ir)
                .is_ok()
        );

        let invalid = concat!(
            "railroad-beta\n",
            "---not-yaml\n",
            "title: should remain source text\n",
            "---\n",
            "entry = terminal(\"value\") ;\n",
        );
        assert!(
            parse_railroad_semantic_source(
                invalid,
                &meta(RailroadDialect::Ir),
                RailroadDialect::Ir,
            )
            .is_err(),
            "an invalid opening YAML delimiter must not be hidden"
        );
    }

    #[test]
    fn non_abnf_strings_reject_physical_line_breaks() {
        let cases = [
            (
                RailroadDialect::Ir,
                "railroad-beta\nentry = terminal(\"left\\\nright\") ;\n",
            ),
            (
                RailroadDialect::Ebnf,
                "railroad-ebnf-beta\nentry = \"left\\\nright\" ;\n",
            ),
            (
                RailroadDialect::Peg,
                "railroad-peg-beta\nentry <- \"left\\\nright\" ;\n",
            ),
        ];
        for (dialect, source) in cases {
            let error = match parse_railroad_semantic_source(source, &meta(dialect), dialect) {
                Ok(_) => {
                    panic!("non-ABNF railroad strings must reject a physical line break");
                }
                Err(error) => error,
            };
            assert!(
                error
                    .to_string()
                    .contains("physical line breaks are not allowed"),
                "{}: {error}",
                dialect.diagram_type()
            );
        }

        let abnf = "railroad-abnf-beta\nentry = \"left\\\nright\" ;\n";
        assert!(
            parse_railroad_semantic_source(
                abnf,
                &meta(RailroadDialect::Abnf),
                RailroadDialect::Abnf,
            )
            .is_ok(),
            "ABNF string terminals intentionally retain physical line breaks"
        );
    }

    #[test]
    fn abnf_strings_slice_without_escape_decoding() {
        let mut pos = 0usize;
        let value = take_quoted_string_in(r#""a\n""#, &mut pos, 0, RailroadDialect::Abnf)
            .unwrap()
            .unwrap();
        assert_eq!(value.text, r#"a\n"#);
    }

    #[test]
    fn parses_repeat_bounds() {
        assert_eq!(
            parse_abnf_repeat_bounds("*"),
            Some((RailroadRepeatBound::ZERO, RailroadRepeatBound::INFINITY))
        );
        assert_eq!(
            parse_abnf_repeat_bounds("1*"),
            Some((RailroadRepeatBound::ONE, RailroadRepeatBound::INFINITY))
        );
        assert_eq!(
            parse_abnf_repeat_bounds("*2"),
            Some((RailroadRepeatBound::ZERO, 2.into()))
        );
        assert_eq!(
            parse_abnf_repeat_bounds("1*2"),
            Some((RailroadRepeatBound::ONE, 2.into()))
        );
        assert_eq!(parse_abnf_repeat_bounds("3"), Some((3.into(), 3.into())));
        assert_eq!(
            parse_abnf_repeat_bounds("18446744073709551615"),
            Some((u64::MAX.into(), u64::MAX.into()))
        );
        assert_eq!(
            parse_abnf_repeat_bounds("18446744073709551616"),
            Some((u64::MAX.into(), u64::MAX.into()))
        );
        assert_eq!(
            parse_abnf_repeat_bounds("00000000000000000002*00000000000000000003"),
            Some((2.into(), 3.into()))
        );
        let huge = "9".repeat(400);
        let (min, max) = parse_abnf_repeat_bounds(&huge).expect("all-digit repeat bound");
        assert!(min.is_infinite());
        assert!(max.is_infinite());
    }

    #[test]
    fn normalizes_multiline_accessibility_descriptions_like_mermaid() {
        let source = concat!(
            "railroad-beta\r\n",
            "accDescr {\r\n",
            "  first\r\n",
            "\r\n",
            "\r\n",
            "  second\r\n",
            "}\r\n",
            "entry = terminal(\"value\") ;\r\n",
        );

        let parsed =
            parse_railroad_semantic_source(source, &meta(RailroadDialect::Ir), RailroadDialect::Ir)
                .unwrap_or_else(|error| panic!("railroad fixture failed: {error}"));
        assert_eq!(parsed.model.acc_descr.as_deref(), Some("first\nsecond"));
    }

    #[test]
    fn ir_constructor_keywords_cannot_be_rule_names_and_recover() {
        for keyword in [
            "sequence",
            "choice",
            "optional",
            "oneOrMore",
            "zeroOrMore",
            "terminal",
            "nonterminal",
            "special",
        ] {
            let source = format!(
                "railroad-beta\n{keyword} = terminal(\"invalid\") ;\nafter = terminal(\"later\") ;\n"
            );
            let error = match parse_railroad_semantic_source(
                &source,
                &meta(RailroadDialect::Ir),
                RailroadDialect::Ir,
            ) {
                Ok(_) => panic!("{keyword} unexpectedly parsed as a Railroad IR rule name"),
                Err(error) => error,
            };
            assert!(
                error
                    .to_string()
                    .contains("is reserved for railroad expressions"),
                "{error}"
            );

            let facts = combined_editor_facts(&source, RailroadDialect::Ir);
            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert!(facts.symbols.iter().any(|symbol| symbol.name == "after"));
        }
    }

    #[test]
    fn rule_names_follow_dialect_keyword_token_precedence() {
        for dialect in [
            RailroadDialect::Ir,
            RailroadDialect::Ebnf,
            RailroadDialect::Abnf,
            RailroadDialect::Peg,
        ] {
            let (assignment, expression) = match dialect {
                RailroadDialect::Ir => ("=", "terminal(\"value\")"),
                RailroadDialect::Ebnf | RailroadDialect::Abnf => ("=", "\"value\""),
                RailroadDialect::Peg => ("<-", "\"value\""),
            };

            let split_title = format!(
                "{}\ntitlecase {assignment} {expression} ;\n",
                dialect.header()
            );
            let parsed = parse_railroad_semantic_source(&split_title, &meta(dialect), dialect)
                .unwrap_or_else(|error| {
                    panic!(
                        "{} title prefix fixture failed: {error}",
                        dialect.diagram_type()
                    )
                });
            assert_eq!(parsed.model.title.as_deref(), Some(""));
            assert_eq!(parsed.model.rules[0].name, "case");

            let title_metadata =
                format!("{}\ntitle {assignment} {expression} ;\n", dialect.header());
            let parsed = parse_railroad_semantic_source(&title_metadata, &meta(dialect), dialect)
                .unwrap_or_else(|error| {
                    panic!(
                        "{} title metadata fixture failed: {error}",
                        dialect.diagram_type()
                    )
                });
            assert!(parsed.model.rules.is_empty());

            let header_rule = format!(
                "{}\n{} {assignment} {expression} ;\n",
                dialect.header(),
                dialect.header()
            );
            assert!(
                parse_railroad_semantic_source(&header_rule, &meta(dialect), dialect).is_err(),
                "{} accepted its header as a rule name",
                dialect.diagram_type()
            );

            for valid in ["Title".to_string(), format!("{}x", dialect.header())] {
                let source = format!(
                    "{}\n{valid} {assignment} {expression} ;\n",
                    dialect.header()
                );
                let parsed = parse_railroad_semantic_source(&source, &meta(dialect), dialect)
                    .unwrap_or_else(|error| {
                        panic!("{} rejected {valid:?}: {error}", dialect.diagram_type())
                    });
                assert_eq!(parsed.model.rules[0].name, valid);
            }

            for invalid in ["title", "titlecase", dialect.header()] {
                assert!(
                    railroad_rule_name_conflict(invalid, dialect).is_some(),
                    "{} rename validation accepted {invalid:?}",
                    dialect.diagram_type()
                );
            }
        }
    }

    #[test]
    fn railroad_parsers_bound_nested_expressions_and_recover_later_rules() {
        for dialect in [
            RailroadDialect::Ir,
            RailroadDialect::Ebnf,
            RailroadDialect::Abnf,
            RailroadDialect::Peg,
        ] {
            let max_depth = nested_rule_source(dialect, crate::MAX_DIAGRAM_NESTING_DEPTH);
            let parsed = parse_railroad_semantic_source(&max_depth, &meta(dialect), dialect)
                .unwrap_or_else(|error| {
                    panic!(
                        "{} max-depth fixture failed: {error}",
                        dialect.diagram_type()
                    )
                });
            assert_eq!(parsed.model.rules.len(), 2, "{}", dialect.diagram_type());

            let compatibility_json =
                parse_railroad_for_dialect(&max_depth, &meta(dialect), dialect).unwrap_or_else(
                    |error| {
                        panic!(
                            "{} max-depth compatibility conversion failed: {error}",
                            dialect.diagram_type()
                        )
                    },
                );
            assert_eq!(
                compatibility_json["rules"].as_array().map(Vec::len),
                Some(2),
                "{}",
                dialect.diagram_type()
            );

            let excessive_depth = nested_rule_source(dialect, crate::MAX_DIAGRAM_NESTING_DEPTH + 1);
            let error =
                match parse_railroad_semantic_source(&excessive_depth, &meta(dialect), dialect) {
                    Ok(_) => panic!("{} accepted excessive nesting", dialect.diagram_type()),
                    Err(error) => error,
                };
            assert!(
                error.to_string().contains("railroad nesting depth exceeds"),
                "{}: {error}",
                dialect.diagram_type()
            );

            let facts = combined_editor_facts(&excessive_depth, dialect);
            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert!(
                facts.symbols.iter().any(|symbol| symbol.name == "after"),
                "{} did not retain the rule after the bounded parse failure",
                dialect.diagram_type()
            );
        }
    }

    #[test]
    fn railroad_cancellation_disposes_a_completed_boundary_rule_for_every_dialect() {
        for dialect in [
            RailroadDialect::Ir,
            RailroadDialect::Ebnf,
            RailroadDialect::Abnf,
            RailroadDialect::Peg,
        ] {
            let source = nested_rule_source(dialect, MAX_DIAGRAM_NESTING_DEPTH);
            let tokens = Lexer::new(&source, dialect, dialect.diagram_type())
                .tokenize_recovering(&OperationControl::new())
                .unwrap();
            assert!(tokens.first_error.is_none());
            // The lexer visits every token once, then checks EOF and its final outcome.
            // The next three checkpoints enter construction, enter parsing, and admit entry.
            // Cancellation at the next rule checkpoint owns the completed maximum-depth rule.
            let control = OperationControl::new();
            control.cancel_after_checkpoints(tokens.tokens.len() + 5);
            assert!(
                construct_railroad_semantic_source(&source, &meta(dialect), dialect, &control)
                    .is_err()
            );
            let after = parse_railroad_semantic_source(
                &format!(
                    "{}\nafter {} {} ;\n",
                    dialect.header(),
                    if dialect == RailroadDialect::Peg {
                        "<-"
                    } else {
                        "="
                    },
                    if dialect == RailroadDialect::Ir {
                        "terminal(\"ok\")"
                    } else {
                        "\"ok\""
                    }
                ),
                &meta(dialect),
                dialect,
            )
            .unwrap();
            assert_eq!(after.model.rules.len(), 1);
        }
    }

    #[test]
    fn railroad_editor_facts_use_dialect_specific_rename_policies() {
        let cases = [
            (
                RailroadDialect::Ir,
                "railroad-beta\nentry = terminal(\"value\") ;\n",
                EditorRenamePolicy::RailroadIrRule,
            ),
            (
                RailroadDialect::Ebnf,
                "railroad-ebnf-beta\nentry = \"value\" ;\n",
                EditorRenamePolicy::RailroadEbnfRule,
            ),
            (
                RailroadDialect::Abnf,
                "railroad-abnf-beta\nentry = \"value\" ;\n",
                EditorRenamePolicy::RailroadAbnfRule,
            ),
            (
                RailroadDialect::Peg,
                "railroad-peg-beta\nentry <- \"value\" ;\n",
                EditorRenamePolicy::RailroadPegRule,
            ),
        ];

        for (dialect, source, expected_policy) in cases {
            let facts = combined_editor_facts(source, dialect);
            let entry = facts
                .symbols
                .iter()
                .find(|symbol| symbol.name == "entry")
                .unwrap_or_else(|| {
                    panic!(
                        "{} is missing its entry rule symbol",
                        dialect.diagram_type()
                    )
                });
            assert_eq!(entry.rename_policy, expected_policy);
        }
    }

    #[test]
    fn railroad_rule_definitions_and_forward_references_have_distinct_roles() {
        let cases = [
            (
                RailroadDialect::Ir,
                concat!(
                    "railroad-beta\n",
                    "entry = nonterminal(\"later\") ;\n",
                    "later = terminal(\"value\") ;\n",
                ),
            ),
            (
                RailroadDialect::Ebnf,
                "railroad-ebnf-beta\nentry = later ;\nlater = \"value\" ;\n",
            ),
            (
                RailroadDialect::Abnf,
                "railroad-abnf-beta\nentry = later ;\nlater = \"value\" ;\n",
            ),
            (
                RailroadDialect::Peg,
                "railroad-peg-beta\nentry <- later ;\nlater <- \"value\" ;\n",
            ),
        ];

        for (dialect, source) in cases {
            let facts = combined_editor_facts(source, dialect);
            assert!(facts.symbols.iter().any(|symbol| {
                symbol.name == "entry" && symbol.role == EditorSemanticRole::Entity
            }));
            assert!(facts.symbols.iter().any(|symbol| {
                symbol.name == "value" && symbol.role == EditorSemanticRole::Payload
            }));
            let later: Vec<_> = facts
                .symbols
                .iter()
                .filter(|symbol| symbol.name == "later")
                .collect();

            assert_eq!(later.len(), 2, "{}", dialect.diagram_type());
            assert_eq!(
                later[0].role,
                EditorSemanticRole::Reference,
                "{}",
                dialect.diagram_type()
            );
            assert_eq!(
                later[1].role,
                EditorSemanticRole::Entity,
                "{}",
                dialect.diagram_type()
            );
            assert_eq!(
                later[0].rename_policy,
                later[1].rename_policy,
                "{}",
                dialect.diagram_type()
            );
        }
    }

    #[test]
    fn typed_render_models_project_exact_compatibility_json_for_every_alias() {
        let cases = [
            ("railroad", "railroad-beta\nentry = terminal(\"value\") ;\n"),
            ("railroadEbnf", "railroad-ebnf-beta\nentry = \"value\" ;\n"),
            ("railroadAbnf", "railroad-abnf-beta\nentry = \"value\" ;\n"),
            ("railroadPeg", "railroad-peg-beta\nentry <- \"value\" ;\n"),
        ];
        let engine = crate::Engine::new();

        for (diagram_type, source) in cases {
            let compat = engine
                .parse_diagram_sync(source, crate::ParseOptions::strict())
                .unwrap()
                .unwrap();
            let typed = engine
                .parse_diagram_for_render_model_sync(source, crate::ParseOptions::strict())
                .unwrap()
                .unwrap();
            let crate::RenderSemanticModel::Railroad(model) = typed.model() else {
                panic!("expected Railroad render model for {diagram_type}");
            };

            assert_eq!(typed.metadata().diagram_type, diagram_type);
            assert_eq!(
                render_model_to_compat_json(model, typed.metadata()).unwrap(),
                compat.model
            );
        }
    }

    #[test]
    fn recovery_keeps_later_rules_for_parser_and_lexer_errors() {
        let cases = [
            (
                RailroadDialect::Ebnf,
                concat!(
                    "railroad-ebnf-beta\r\n",
                    "before = \"ok\" ;\r\n",
                    "broken = ( \"x\" ;\r\n",
                    "after = \"later\" ;\r\n",
                ),
            ),
            (
                RailroadDialect::Peg,
                concat!(
                    "railroad-peg-beta\r\n",
                    "before <- \"ok\" ;\r\n",
                    "@\r\n",
                    "after <- \"later\" ;\r\n",
                ),
            ),
        ];

        for (dialect, source) in cases {
            assert!(parse_railroad_semantic_source(source, &meta(dialect), dialect).is_err());

            reset_railroad_syntax_construction_count();
            let facts = combined_editor_facts(source, dialect);
            assert_eq!(railroad_syntax_construction_count(), 1);
            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert!(facts.symbols.iter().any(|symbol| symbol.name == "after"));
            assert!(facts.symbols.iter().any(|symbol| symbol.name == "later"));
            assert_eq!(facts.diagnostics.len(), 1);
        }
    }
}
