use crate::diagrams::scan::{split_indent, starts_with_case_insensitive, strip_line_ending};
use crate::sanitize::sanitize_text;
use crate::{
    EditorSemanticFacts, EditorSemanticKind, EditorSemanticSymbol, Error, MermaidConfig,
    ParseMetadata, Result, SourceSpan,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static KANBAN_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_kanban_syntax_construction_count() {
    KANBAN_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn kanban_syntax_construction_count() -> usize {
    KANBAN_SYNTAX_CONSTRUCTION_COUNT.get()
}

const NODE_TYPE_DEFAULT: i32 = 0;
const NODE_TYPE_ROUNDED_RECT: i32 = 1;
const NODE_TYPE_RECT: i32 = 2;
const NODE_TYPE_CIRCLE: i32 = 3;
const NODE_TYPE_CLOUD: i32 = 4;
const NODE_TYPE_BANG: i32 = 5;
const NODE_TYPE_HEXAGON: i32 = 6;

#[derive(Debug, Clone)]
struct KanbanNode {
    id: String,
    span: SourceSpan,
    level: usize,
    label: String,
    width: i64,
    padding: i64,
    parent_id: Option<String>,

    ticket: Option<String>,
    priority: Option<String>,
    assigned: Option<String>,
    icon: Option<String>,
    css_classes: Option<String>,
    shape: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KanbanDiagramRenderModel {
    #[serde(default)]
    pub nodes: Vec<KanbanRenderNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KanbanRenderNode {
    pub id: String,
    pub label: String,
    #[serde(default, rename = "isGroup")]
    pub is_group: bool,
    #[serde(default, rename = "parentId")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub ticket: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub assigned: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(skip)]
    compatibility: KanbanRenderNodeCompatibility,
}

#[derive(Debug, Clone, Default)]
struct KanbanRenderNodeCompatibility {
    level: usize,
    width: i64,
    padding: i64,
    ticket: Option<String>,
    priority: Option<String>,
    assigned: Option<String>,
    icon: Option<String>,
    css_classes: Option<String>,
    shape: Option<String>,
}

impl KanbanRenderNode {
    /// Creates a render node with no parent or optional card metadata.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Default)]
struct KanbanDb {
    nodes: Vec<KanbanNode>,
    section_indices: Vec<usize>,
    next_auto_id: i64,
}

struct KanbanSemanticSource {
    db: KanbanDb,
    editor_facts: EditorSemanticFacts,
}

struct KanbanParseFailure {
    error: Box<Error>,
    editor_facts: Box<EditorSemanticFacts>,
    span: SourceSpan,
}

impl KanbanParseFailure {
    fn into_error_and_editor_facts(self) -> (Error, EditorSemanticFacts) {
        let mut facts = *self.editor_facts;
        facts.mark_recovered_from_parse_error(
            format!("kanban parser recovered after parse error: {}", self.error),
            Some(self.span),
        );
        (*self.error, facts)
    }
}

#[derive(Debug, Clone)]
struct SpannedText {
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct KanbanNodeSpec {
    id_raw: String,
    descr_raw: String,
    ty: i32,
}

#[derive(Debug, Clone)]
struct ParsedKanbanNode {
    spec: KanbanNodeSpec,
    entity: SpannedText,
    label: Option<SpannedText>,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct KanbanMetadataField {
    key: SpannedText,
    value: Option<SpannedText>,
    is_string_value: bool,
}

#[derive(Debug, Clone)]
struct KanbanShapeData {
    text: String,
    span: SourceSpan,
    fields: Vec<KanbanMetadataField>,
}

#[derive(Debug, Clone, Copy)]
struct KanbanSourceLine<'a> {
    text: &'a str,
    start: usize,
}

#[derive(Debug)]
struct KanbanLineCursor<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> KanbanLineCursor<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    fn next(&mut self) -> Option<KanbanSourceLine<'a>> {
        if self.offset >= self.source.len() {
            return None;
        }

        let start = self.offset;
        let rest = &self.source[start..];
        let end = if let Some(newline) = rest.find('\n') {
            start + newline + 1
        } else {
            self.source.len()
        };
        self.offset = end;

        Some(KanbanSourceLine {
            text: strip_line_ending(&self.source[start..end]),
            start,
        })
    }

    fn offset(&self) -> usize {
        self.offset
    }
}

impl KanbanDb {
    fn get_section_index(&self, level: usize) -> Result<Option<usize>> {
        if self.nodes.is_empty() {
            return Ok(None);
        }

        let section_level = self.nodes[0].level;
        let mut last_section_idx: Option<usize> = None;
        for (idx, node) in self.nodes.iter().enumerate().rev() {
            if node.level == section_level && last_section_idx.is_none() {
                last_section_idx = Some(idx);
            }
            if node.level < section_level {
                return Err(Error::diagram_parse_exact(
                    "kanban",
                    format!(
                        "Items without section detected, found section (\"{}\")",
                        node.label
                    ),
                    node.span,
                ));
            }
        }

        let Some(last_section_idx) = last_section_idx else {
            return Ok(None);
        };

        if level == self.nodes[last_section_idx].level {
            return Ok(None);
        }
        Ok(Some(last_section_idx))
    }

    fn decorate_last(
        &mut self,
        class: Option<String>,
        icon: Option<String>,
        config: &MermaidConfig,
    ) {
        let Some(last) = self.nodes.last_mut() else {
            return;
        };
        if let Some(icon) = icon {
            last.icon = Some(sanitize_text(&icon, config));
        }
        if let Some(class) = class {
            last.css_classes = Some(sanitize_text(&class, config));
        }
    }

    fn add_node(
        &mut self,
        level: usize,
        spec: KanbanNodeSpec,
        span: SourceSpan,
        shape_data: Option<(KanbanShapeData, Value)>,
        config: &MermaidConfig,
    ) -> Result<()> {
        let mut padding = get_i64(config, "mindmap.padding").unwrap_or(10);
        let width = get_i64(config, "mindmap.maxNodeWidth").unwrap_or(200);
        match spec.ty {
            NODE_TYPE_ROUNDED_RECT | NODE_TYPE_RECT | NODE_TYPE_HEXAGON => {
                padding *= 2;
            }
            _ => {}
        }

        let mut id = sanitize_text(&spec.id_raw, config);
        if id.is_empty() {
            id = format!("kbn{}", self.next_auto_id);
            self.next_auto_id += 1;
        }

        let mut node = KanbanNode {
            id,
            span,
            level,
            label: sanitize_text(&spec.descr_raw, config),
            width,
            padding,
            parent_id: None,
            ticket: None,
            priority: None,
            assigned: None,
            icon: None,
            css_classes: None,
            shape: None,
        };

        if let Some((shape_data, document)) = shape_data {
            apply_shape_data(&mut node, &document, shape_data.span)?;
        }

        if let Some(section_idx) = self.get_section_index(level)? {
            node.parent_id = Some(self.nodes[section_idx].id.clone());
        } else {
            self.section_indices.push(self.nodes.len());
        }
        self.nodes.push(node);
        Ok(())
    }

    fn data_nodes_for_render(&self, config: &MermaidConfig) -> Vec<KanbanRenderNode> {
        let mut out = Vec::new();
        for &section_idx in &self.section_indices {
            let Some(section) = self.nodes.get(section_idx) else {
                continue;
            };
            out.push(KanbanRenderNode {
                id: section.id.clone(),
                label: sanitize_text(&section.label, config),
                is_group: true,
                parent_id: None,
                ticket: section.ticket.clone(),
                priority: section.priority.clone(),
                assigned: section.assigned.clone(),
                icon: section.icon.clone(),
                compatibility: KanbanRenderNodeCompatibility {
                    level: section.level,
                    width: section.width,
                    padding: section.padding,
                    ticket: section.ticket.clone(),
                    priority: section.priority.clone(),
                    assigned: section.assigned.clone(),
                    icon: section.icon.clone(),
                    css_classes: section.css_classes.clone(),
                    shape: section.shape.clone(),
                },
            });

            for item in self
                .nodes
                .iter()
                .filter(|n| n.parent_id.as_deref() == Some(&section.id))
            {
                out.push(KanbanRenderNode {
                    id: item.id.clone(),
                    label: sanitize_text(&item.label, config),
                    is_group: false,
                    parent_id: Some(section.id.clone()),
                    ticket: item.ticket.clone(),
                    priority: item.priority.clone(),
                    assigned: item.assigned.clone(),
                    icon: item.icon.clone(),
                    compatibility: KanbanRenderNodeCompatibility {
                        level: item.level,
                        width: item.width,
                        padding: item.padding,
                        ticket: item.ticket.clone(),
                        priority: item.priority.clone(),
                        assigned: item.assigned.clone(),
                        icon: item.icon.clone(),
                        css_classes: item.css_classes.clone(),
                        shape: item.shape.clone(),
                    },
                });
            }
        }
        out
    }
}

fn apply_shape_data(node: &mut KanbanNode, document: &Value, span: SourceSpan) -> Result<()> {
    let Some(obj) = document.as_object() else {
        return Ok(());
    };

    if let Some(Value::String(shape)) = obj.get("shape") {
        if shape != &shape.to_lowercase() || shape.contains('_') {
            return Err(Error::diagram_parse_exact(
                "kanban",
                format!("No such shape: {shape}. Shape names should be lowercase."),
                span,
            ));
        }
        if shape == "kanbanItem" {
            node.shape = Some(shape.clone());
        }
    }

    if let Some(Value::String(label)) = obj.get("label") {
        node.label = label.clone();
    }
    if let Some(icon) = obj.get("icon") {
        node.icon = Some(value_to_string(icon));
    }
    if let Some(assigned) = obj.get("assigned") {
        node.assigned = Some(value_to_string(assigned));
    }
    if let Some(ticket) = obj.get("ticket") {
        node.ticket = Some(value_to_string(ticket));
    }
    if let Some(priority) = obj.get("priority") {
        node.priority = Some(value_to_string(priority));
    }

    Ok(())
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn strip_inline_comment(line: &str) -> &str {
    let mut in_quote = false;
    let mut in_backtick_quote = false;

    let mut it = line.char_indices().peekable();
    while let Some((idx, ch)) = it.next() {
        if in_backtick_quote {
            if ch == '`' && it.peek().is_some_and(|(_, next)| *next == '"') {
                in_backtick_quote = false;
                it.next();
            }
            continue;
        }

        if in_quote {
            if ch == '"' {
                in_quote = false;
            }
            continue;
        }

        if ch == '"' {
            if it.peek().is_some_and(|(_, next)| *next == '`') {
                in_backtick_quote = true;
                it.next();
                continue;
            }
            in_quote = true;
            continue;
        }

        if ch == '%' && it.peek().is_some_and(|(_, next)| *next == '%') {
            return &line[..idx];
        }
    }

    line
}

fn parse_node_spec_for_render(input: &str, input_start: usize) -> Result<ParsedKanbanNode> {
    let input = input.trim_end();
    if input.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "kanban",
            "expected node",
            input_start,
        ));
    }

    if let Some((start, end)) = node_delimiter_pair_at_start(input) {
        let inner_start = input_start + start.len();
        let (inner, tail) = match extract_delimited(input, start, end) {
            Ok(parts) => parts,
            Err(message) => {
                return Err(Error::diagram_parse_insertion_point(
                    "kanban",
                    message,
                    input_start + input.len(),
                ));
            }
        };
        let label = parse_kanban_label(inner, inner_start);
        let closing_start = inner_start + inner.len();
        if !tail.trim().is_empty() {
            let tail_start = input_start + input.len() - tail.len();
            let leading = tail.len() - tail.trim_start().len();
            let trailing = tail.trim_end().len();
            let span = SourceSpan::new(tail_start + leading, tail_start + trailing);
            return Err(Error::diagram_parse_exact(
                "kanban",
                "unexpected trailing input",
                span,
            ));
        }
        let ty = node_type_for(start, end);
        return Ok(ParsedKanbanNode {
            spec: KanbanNodeSpec {
                id_raw: label.text.clone(),
                descr_raw: label.text.clone(),
                ty,
            },
            entity: label,
            label: None,
            span: SourceSpan::new(input_start, closing_start + end.len()),
        });
    }

    let (id_raw, rest) = split_node_id(input);
    let id = SpannedText {
        text: id_raw.to_string(),
        span: SourceSpan::new(input_start, input_start + id_raw.len()),
    };
    let rest = rest.trim_end();
    if rest.is_empty() {
        return Ok(ParsedKanbanNode {
            spec: KanbanNodeSpec {
                id_raw: id_raw.to_string(),
                descr_raw: id_raw.to_string(),
                ty: NODE_TYPE_DEFAULT,
            },
            entity: id,
            label: None,
            span: SourceSpan::new(input_start, input_start + input.len()),
        });
    }

    let Some((start, end)) = node_delimiter_pair_at_start(rest) else {
        return Err(Error::diagram_parse_insertion_point(
            "kanban",
            "expected node delimiter",
            input_start + id_raw.len(),
        ));
    };

    let rest_start = input_start + id_raw.len();
    let inner_start = rest_start + start.len();
    let (inner, tail) = match extract_delimited(rest, start, end) {
        Ok(parts) => parts,
        Err(message) => {
            return Err(Error::diagram_parse_insertion_point(
                "kanban",
                message,
                rest_start + rest.len(),
            ));
        }
    };
    let label = parse_kanban_label(inner, inner_start);
    let closing_start = inner_start + inner.len();
    if !tail.trim().is_empty() {
        let tail_start = rest_start + rest.len() - tail.len();
        let leading = tail.len() - tail.trim_start().len();
        let trailing = tail.trim_end().len();
        let span = SourceSpan::new(tail_start + leading, tail_start + trailing);
        return Err(Error::diagram_parse_exact(
            "kanban",
            "unexpected trailing input",
            span,
        ));
    }
    let ty = node_type_for(start, end);
    Ok(ParsedKanbanNode {
        spec: KanbanNodeSpec {
            id_raw: id_raw.to_string(),
            descr_raw: label.text.clone(),
            ty,
        },
        entity: id,
        label: Some(label),
        span: SourceSpan::new(input_start, closing_start + end.len()),
    })
}

fn parse_kanban_label(raw: &str, raw_start: usize) -> SpannedText {
    let (text, span) = if let Some(raw) = raw.strip_prefix("\"`") {
        if let Some(raw) = raw.strip_suffix("`\"") {
            let closing_start = raw_start + 2 + raw.len();
            (
                raw.to_string(),
                SourceSpan::new(raw_start + 2, closing_start),
            )
        } else {
            (
                raw.to_string(),
                SourceSpan::new(raw_start + 2, raw_start + 2 + raw.len()),
            )
        }
    } else if let Some(raw) = raw.strip_prefix('"') {
        if let Some(raw) = raw.strip_suffix('"') {
            let closing_start = raw_start + 1 + raw.len();
            (
                raw.to_string(),
                SourceSpan::new(raw_start + 1, closing_start),
            )
        } else {
            (
                raw.to_string(),
                SourceSpan::new(raw_start + 1, raw_start + 1 + raw.len()),
            )
        }
    } else {
        (
            raw.to_string(),
            SourceSpan::new(raw_start, raw_start + raw.len()),
        )
    };
    SpannedText { text, span }
}

fn split_node_id(input: &str) -> (&str, &str) {
    let bytes = input.as_bytes();
    for (idx, b) in bytes.iter().enumerate() {
        match b {
            b'(' | b')' | b'[' | b'{' | b'}' => return (&input[..idx], &input[idx..]),
            _ => {}
        }
    }
    (input, "")
}

fn node_delimiter_pair_at_start(input: &str) -> Option<(&'static str, &'static str)> {
    let pairs: &[(&str, &str)] = &[
        ("(-", "-)"),
        ("-)", "(-"),
        ("((", "))"),
        ("))", "(("),
        ("{{", "}}"),
        ("[", "]"),
        (")", "("),
        ("(", ")"),
    ];

    for (start, end) in pairs {
        if input.starts_with(start) {
            return Some((*start, *end));
        }
    }
    None
}

fn extract_delimited<'a>(
    input: &'a str,
    start: &str,
    end: &str,
) -> std::result::Result<(&'a str, &'a str), String> {
    if !input.starts_with(start) {
        return Err("expected delimiter start".to_string());
    }
    let mut in_quote = false;
    let mut in_backtick_quote = false;

    let start_len = start.len();
    let mut it = input[start_len..].char_indices().peekable();
    while let Some((off, ch)) = it.next() {
        let idx = start_len + off;

        if in_backtick_quote {
            if ch == '`' && it.peek().is_some_and(|(_, next)| *next == '"') {
                in_backtick_quote = false;
                it.next();
            }
            continue;
        }

        if in_quote {
            if ch == '"' {
                in_quote = false;
            }
            continue;
        }

        if ch == '"' {
            if it.peek().is_some_and(|(_, next)| *next == '`') {
                in_backtick_quote = true;
                it.next();
                continue;
            }
            in_quote = true;
            continue;
        }

        if input[idx..].starts_with(end) {
            let inner = &input[start_len..idx];
            let tail = &input[idx + end.len()..];
            return Ok((inner, tail));
        }
    }

    Err("unterminated node delimiter".to_string())
}

fn node_type_for(start: &str, end: &str) -> i32 {
    match start {
        "[" => NODE_TYPE_RECT,
        "(" => {
            if end == ")" {
                NODE_TYPE_ROUNDED_RECT
            } else {
                NODE_TYPE_CLOUD
            }
        }
        "((" => NODE_TYPE_CIRCLE,
        ")" => NODE_TYPE_CLOUD,
        "))" => NODE_TYPE_BANG,
        "{{" => NODE_TYPE_HEXAGON,
        _ => NODE_TYPE_DEFAULT,
    }
}

fn get_i64(cfg: &MermaidConfig, dotted_path: &str) -> Option<i64> {
    let mut cur = cfg.as_value();
    for segment in dotted_path.split('.') {
        cur = cur.as_object()?.get(segment)?;
    }
    cur.as_i64().or_else(|| cur.as_f64().map(|f| f as i64))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KanbanMetadataMode {
    Key,
    AfterKey,
    Value,
    AfterValue,
}

#[derive(Debug, Clone, Copy)]
struct KanbanMetadataQuote {
    delimiter: char,
    content_start: usize,
    is_key: bool,
}

struct KanbanMetadataCursor<'a> {
    source: &'a str,
    mode: KanbanMetadataMode,
    token_start: Option<usize>,
    current_key: Option<SpannedText>,
    quote: Option<KanbanMetadataQuote>,
    fields: Vec<KanbanMetadataField>,
}

impl<'a> KanbanMetadataCursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            mode: KanbanMetadataMode::Key,
            token_start: None,
            current_key: None,
            quote: None,
            fields: Vec::new(),
        }
    }

    fn consume(&mut self, offset: usize, ch: char) {
        if let Some(quote) = self.quote {
            if ch == quote.delimiter {
                let span = SourceSpan::new(quote.content_start, offset);
                if quote.is_key {
                    self.current_key = Some(SpannedText {
                        text: self.source[span.start..span.end].to_string(),
                        span,
                    });
                    self.mode = KanbanMetadataMode::AfterKey;
                } else {
                    self.finish_field(Some(span), true);
                    self.mode = KanbanMetadataMode::AfterValue;
                }
                self.quote = None;
            }
            return;
        }

        match self.mode {
            KanbanMetadataMode::Key => match ch {
                ':' => {
                    self.finish_key(offset);
                    self.mode = KanbanMetadataMode::Value;
                }
                ',' => self.finish_invalid_token(),
                '"' | '\'' if self.token_start.is_none() => {
                    self.quote = Some(KanbanMetadataQuote {
                        delimiter: ch,
                        content_start: offset + ch.len_utf8(),
                        is_key: true,
                    });
                }
                _ if ch.is_whitespace() && self.token_start.is_none() => {}
                _ => {
                    self.token_start.get_or_insert(offset);
                }
            },
            KanbanMetadataMode::AfterKey => match ch {
                ':' => {
                    self.mode = KanbanMetadataMode::Value;
                }
                _ if ch.is_whitespace() => {}
                _ => {
                    self.token_start = Some(offset);
                    self.mode = KanbanMetadataMode::Key;
                }
            },
            KanbanMetadataMode::Value => match ch {
                ',' => {
                    self.finish_bare_value(offset);
                    self.mode = KanbanMetadataMode::Key;
                }
                '"' | '\'' if self.token_start.is_none() => {
                    self.quote = Some(KanbanMetadataQuote {
                        delimiter: ch,
                        content_start: offset + ch.len_utf8(),
                        is_key: false,
                    });
                }
                _ if ch.is_whitespace() && self.token_start.is_none() => {}
                _ => {
                    self.token_start.get_or_insert(offset);
                }
            },
            KanbanMetadataMode::AfterValue => match ch {
                ',' => {
                    self.mode = KanbanMetadataMode::Key;
                }
                _ if ch.is_whitespace() => {}
                _ => {
                    self.token_start = Some(offset);
                    self.mode = KanbanMetadataMode::Key;
                }
            },
        }
    }

    fn line_break(&mut self, offset: usize) {
        if self.quote.is_some() {
            return;
        }
        match self.mode {
            KanbanMetadataMode::Value => self.finish_bare_value(offset),
            KanbanMetadataMode::Key => self.finish_invalid_token(),
            KanbanMetadataMode::AfterKey | KanbanMetadataMode::AfterValue => {}
        }
        if self.mode != KanbanMetadataMode::AfterKey {
            self.mode = KanbanMetadataMode::Key;
        }
    }

    fn close(&mut self, offset: usize) {
        if let Some(quote) = self.quote.take() {
            let span = SourceSpan::new(quote.content_start, offset);
            if !quote.is_key {
                self.finish_field(Some(span), true);
            }
        } else {
            match self.mode {
                KanbanMetadataMode::Value => self.finish_bare_value(offset),
                KanbanMetadataMode::Key => self.finish_invalid_token(),
                KanbanMetadataMode::AfterKey | KanbanMetadataMode::AfterValue => {}
            }
        }
    }

    fn finish_at_eof(&mut self, offset: usize) {
        if let Some(quote) = self.quote.take() {
            let span = SourceSpan::new(quote.content_start, offset);
            if !quote.is_key {
                self.finish_field(Some(span), true);
            }
            return;
        }
        match self.mode {
            KanbanMetadataMode::Value => self.finish_bare_value(offset),
            KanbanMetadataMode::Key => self.finish_invalid_token(),
            KanbanMetadataMode::AfterKey | KanbanMetadataMode::AfterValue => {}
        }
    }

    fn finish_key(&mut self, end: usize) {
        let Some(span) = self.trimmed_token_span(end) else {
            return;
        };
        self.current_key = Some(SpannedText {
            text: self.source[span.start..span.end].to_string(),
            span,
        });
    }

    fn finish_bare_value(&mut self, end: usize) {
        let span = self.trimmed_token_span(end);
        let is_string_value = span.is_some_and(|span| {
            let raw = &self.source[span.start..span.end];
            !matches!(raw, "true" | "false" | "null") && !is_kanban_inline_number(raw)
        });
        self.finish_field(span, is_string_value);
    }

    fn finish_invalid_token(&mut self) {
        self.token_start = None;
    }

    fn finish_field(&mut self, value_span: Option<SourceSpan>, is_string_value: bool) {
        let Some(key) = self.current_key.take() else {
            return;
        };
        let value = value_span.map(|span| SpannedText {
            text: self.source[span.start..span.end].to_string(),
            span,
        });
        self.fields.push(KanbanMetadataField {
            key,
            value,
            is_string_value,
        });
    }

    fn trimmed_token_span(&mut self, end: usize) -> Option<SourceSpan> {
        let start = self.token_start.take()?;
        let raw = &self.source[start..end];
        let leading = raw.len().saturating_sub(raw.trim_start().len());
        let trailing = raw.trim_end().len();
        (leading < trailing).then(|| SourceSpan::new(start + leading, start + trailing))
    }

    fn into_fields(self) -> Vec<KanbanMetadataField> {
        self.fields
    }
}

fn is_kanban_inline_number(raw: &str) -> bool {
    raw.as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'-')
        && matches!(
            serde_json::from_str::<Value>(raw),
            Ok(Value::Number(number)) if number.as_f64().is_some_and(f64::is_finite)
        )
}

fn consume_shape_data(
    lines: &mut KanbanLineCursor<'_>,
    first: &str,
    first_start: usize,
) -> Result<KanbanShapeData> {
    let Some(mut rest) = first.strip_prefix("@{") else {
        return Ok(KanbanShapeData {
            text: String::new(),
            span: SourceSpan::new(first_start, first_start),
            fields: Vec::new(),
        });
    };

    let mut out = String::new();
    let mut in_quote = false;
    let mut quoted = String::new();
    let block_start = first_start;
    let mut current_scan_start = first_start + "@{".len();
    let mut metadata = KanbanMetadataCursor::new(lines.source);

    loop {
        let it = rest.char_indices().peekable();
        for (idx, ch) in it {
            let source_offset = current_scan_start + idx;
            if !in_quote && ch == '}' {
                metadata.close(source_offset);
                let fields = metadata.into_fields();
                let after_close = &rest[idx + ch.len_utf8()..];
                let visible_tail = strip_inline_comment(after_close);
                if !visible_tail.trim().is_empty() {
                    let leading = visible_tail.len() - visible_tail.trim_start().len();
                    let trailing = visible_tail.trim_end().len();
                    let tail_start = source_offset + ch.len_utf8();
                    let span = SourceSpan::new(tail_start + leading, tail_start + trailing);
                    return Err(Error::diagram_parse_exact(
                        "kanban",
                        "unexpected trailing input",
                        span,
                    ));
                }
                return Ok(KanbanShapeData {
                    text: out,
                    span: SourceSpan::new(block_start, source_offset + ch.len_utf8()),
                    fields,
                });
            }
            metadata.consume(source_offset, ch);
            if in_quote {
                if ch == '"' {
                    out.push_str(&replace_newline_whitespace_with_br(&quoted));
                    quoted.clear();
                    out.push('"');
                    in_quote = false;
                    continue;
                }
                quoted.push(ch);
                continue;
            }

            if ch == '"' {
                out.push('"');
                in_quote = true;
                continue;
            }

            out.push(ch);
        }

        metadata.line_break(current_scan_start + rest.len());

        let Some(next_line) = lines.next() else {
            metadata.finish_at_eof(lines.offset());
            return Err(Error::diagram_parse_insertion_point(
                "kanban",
                "unterminated @{ ... } metadata block",
                lines.offset(),
            ));
        };
        if in_quote {
            quoted.push('\n');
        } else {
            out.push('\n');
        }
        rest = next_line.text;
        current_scan_start = next_line.start;
    }
}

fn replace_newline_whitespace_with_br(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            while it.peek().is_some_and(|c| c.is_whitespace()) {
                it.next();
            }
            out.push_str("<br/>");
            continue;
        }
        out.push(ch);
    }
    out
}

fn split_node_and_shape_data(
    lines: &mut KanbanLineCursor<'_>,
    rest: &str,
    rest_start: usize,
) -> Result<(String, Option<KanbanShapeData>)> {
    let mut in_quote = false;
    let mut in_backtick_quote = false;
    let mut it = rest.char_indices().peekable();
    while let Some((idx, ch)) = it.next() {
        if in_backtick_quote {
            if ch == '`' && it.peek().is_some_and(|(_, next)| *next == '"') {
                in_backtick_quote = false;
                it.next();
            }
            continue;
        }

        if in_quote {
            if ch == '"' {
                in_quote = false;
            }
            continue;
        }

        if ch == '"' {
            if it.peek().is_some_and(|(_, next)| *next == '`') {
                in_backtick_quote = true;
                it.next();
                continue;
            }
            in_quote = true;
            continue;
        }

        if rest[idx..].starts_with("@{") {
            let node_part = rest[..idx].trim_end().to_string();
            let shape_data = consume_shape_data(lines, &rest[idx..], rest_start + idx)?;
            return Ok((node_part, Some(shape_data)));
        }
    }

    Ok((rest.trim_end().to_string(), None))
}

fn construct_kanban_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<KanbanSemanticSource, KanbanParseFailure> {
    construct_kanban_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_kanban_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<KanbanSemanticSource, KanbanParseFailure>> {
    control.checkpoint()?;
    #[cfg(test)]
    KANBAN_SYNTAX_CONSTRUCTION_COUNT.set(KANBAN_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    let mut db = KanbanDb::default();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut first_failure = None;

    let mut lines = KanbanLineCursor::new(code);
    let header_tail = loop {
        control.checkpoint()?;
        let Some(line) = lines.next() else {
            let span = SourceSpan::new(lines.offset(), lines.offset());
            return Ok(Err(KanbanParseFailure {
                error: Box::new(Error::diagram_parse_insertion_point(
                    meta.diagram_type.clone(),
                    "expected kanban header",
                    span.start,
                )),
                editor_facts: Box::new(editor_facts),
                span,
            }));
        };
        let visible = strip_inline_comment(line.text);
        let visible_start = visible.len() - visible.trim_start().len();
        let visible_end = visible.trim_end().len();
        if visible_start >= visible_end {
            continue;
        }
        let trimmed = &visible[visible_start..visible_end];
        let trimmed_start = line.start + visible_start;
        let after_keyword = trimmed
            .get("kanban".len()..)
            .filter(|_| starts_with_case_insensitive(trimmed, "kanban"))
            .filter(|after| {
                after.is_empty() || after.chars().next().is_some_and(char::is_whitespace)
            });
        if let Some(after_keyword) = after_keyword {
            let keyword_end = trimmed_start + "kanban".len();
            if !after_keyword.trim().is_empty() {
                break Some((after_keyword.to_string(), keyword_end));
            }
            break None;
        }
        let span = SourceSpan::new(trimmed_start, trimmed_start + trimmed.len());
        return Ok(Err(KanbanParseFailure {
            error: Box::new(Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                "expected kanban header",
                span,
            )),
            editor_facts: Box::new(editor_facts),
            span,
        }));
    };

    control.checkpoint()?;
    if let Some((tail, tail_start)) = &header_tail
        && let Err(error) = parse_kanban_statement(
            &mut lines,
            KanbanStatementContext {
                db: &mut db,
                facts: &mut editor_facts,
                meta,
                control,
            },
            tail,
            *tail_start,
        )?
    {
        let fallback = SourceSpan::new(*tail_start, *tail_start + tail.len());
        record_kanban_failure(&mut first_failure, error, fallback);
    }
    control.checkpoint()?;

    while let Some(source_line) = lines.next() {
        control.checkpoint()?;
        if let Err(error) = parse_kanban_statement(
            &mut lines,
            KanbanStatementContext {
                db: &mut db,
                facts: &mut editor_facts,
                meta,
                control,
            },
            source_line.text,
            source_line.start,
        )? {
            let fallback = SourceSpan::new(
                source_line.start,
                source_line.start + source_line.text.len(),
            );
            record_kanban_failure(&mut first_failure, error, fallback);
        }
    }

    if let Some((error, span)) = first_failure {
        Ok(Err(KanbanParseFailure {
            error: Box::new(error),
            editor_facts: Box::new(editor_facts),
            span,
        }))
    } else {
        control.checkpoint()?;
        Ok(Ok(KanbanSemanticSource { db, editor_facts }))
    }
}

fn record_kanban_failure(
    first_failure: &mut Option<(Error, SourceSpan)>,
    error: Error,
    fallback: SourceSpan,
) {
    if first_failure.is_none() {
        let span = kanban_error_span(&error, fallback);
        *first_failure = Some((error, span));
    }
}

struct KanbanStatementContext<'a> {
    db: &'a mut KanbanDb,
    facts: &'a mut EditorSemanticFacts,
    meta: &'a ParseMetadata,
    control: &'a crate::OperationControl,
}

fn parse_kanban_statement(
    lines: &mut KanbanLineCursor<'_>,
    context: KanbanStatementContext<'_>,
    source: &str,
    source_start: usize,
) -> crate::OperationControlResult<Result<()>> {
    let KanbanStatementContext {
        db,
        facts,
        meta,
        control,
    } = context;
    control.checkpoint()?;
    let line = strip_inline_comment(source).trim_end();
    if line.trim().is_empty() {
        return Ok(Ok(()));
    }
    let (indent, rest) = split_indent(line);
    let rest = rest.trim_end();
    if rest.is_empty() {
        return Ok(Ok(()));
    }
    let rest_start = source_start + line.len() - rest.len();

    if starts_with_case_insensitive(rest, "::icon(") {
        let value_start = "::icon(".len();
        let suffix = &rest[value_start..];
        let close = suffix
            .char_indices()
            .find_map(|(offset, ch)| (ch == ')').then_some(offset));
        let raw_value = close.map_or(suffix, |end| &suffix[..end]);
        let leading = raw_value.len() - raw_value.trim_start().len();
        let trailing = raw_value.trim_end().len();
        let icon = SpannedText {
            text: raw_value[leading.min(trailing)..trailing].to_string(),
            span: SourceSpan::new(
                rest_start + value_start + leading.min(trailing),
                rest_start + value_start + trailing,
            ),
        };
        let Some(close) = close else {
            return Ok(Err(Error::diagram_parse_insertion_point(
                "kanban",
                "unterminated icon decoration",
                rest_start + rest.len(),
            )));
        };
        let close_start = rest_start + value_start + close;
        let after_close = &suffix[close + 1..];
        let visible_tail = strip_inline_comment(after_close);
        if !visible_tail.trim().is_empty() {
            let leading = visible_tail.len() - visible_tail.trim_start().len();
            let trailing = visible_tail.trim_end().len();
            let tail_start = close_start + 1;
            let span = SourceSpan::new(tail_start + leading, tail_start + trailing);
            return Ok(Err(Error::diagram_parse_exact(
                "kanban",
                "unexpected trailing input",
                span,
            )));
        }
        facts.push_directive_prefix("icon");
        if icon.text.is_empty() {
            return Ok(Ok(()));
        }
        db.decorate_last(None, Some(icon.text.clone()), &meta.effective_config);
        facts.push_symbol(EditorSemanticSymbol::payload(
            icon.text,
            Some("kanban icon".to_string()),
            EditorSemanticKind::String,
            icon.span,
            icon.span,
        ));
        return Ok(Ok(()));
    }

    if let Some(after) = rest.strip_prefix(":::") {
        let class_raw = after.trim();
        let leading = after.len() - after.trim_start().len();
        let class_name = SpannedText {
            text: class_raw.to_string(),
            span: SourceSpan::new(
                rest_start + 3 + leading,
                rest_start + 3 + leading + class_raw.len(),
            ),
        };
        db.decorate_last(Some(class_name.text.clone()), None, &meta.effective_config);
        if !class_name.text.is_empty() {
            facts.push_directive_prefix(":::");
            facts.push_symbol(EditorSemanticSymbol::payload(
                class_name.text,
                Some("kanban class".to_string()),
                EditorSemanticKind::Class,
                class_name.span,
                class_name.span,
            ));
        }
        return Ok(Ok(()));
    }

    let (node_part, shape_data) = match split_node_and_shape_data(lines, rest, rest_start) {
        Ok(parsed) => parsed,
        Err(error) => return Ok(Err(error)),
    };
    if node_part.trim().is_empty() {
        return Ok(Ok(()));
    }
    let parsed = match parse_node_spec_for_render(&node_part, rest_start) {
        Ok(parsed) => parsed,
        Err(error) => return Ok(Err(error)),
    };
    let fact_kind = kanban_node_editor_kind(parsed.spec.ty);
    if let Some(shape_data) = &shape_data {
        push_kanban_metadata_facts(facts, &shape_data.fields);
    }
    let shape_data = match shape_data {
        Some(shape_data) => {
            let document = match crate::inline_config::parse_mermaid_inline_object_controlled(
                &shape_data.text,
                control,
            )? {
                Ok(document) => document,
                Err(error) => {
                    return Ok(Err(Error::diagram_parse_exact(
                        "kanban",
                        error,
                        shape_data.span,
                    )));
                }
            };
            Some((shape_data, document))
        }
        None => None,
    };
    if let Err(error) = db.add_node(
        indent,
        parsed.spec,
        parsed.span,
        shape_data,
        &meta.effective_config,
    ) {
        return Ok(Err(error));
    }
    let is_section = db.nodes.last().is_some_and(|node| node.parent_id.is_none());
    if is_section {
        facts.push_symbol(EditorSemanticSymbol::outline(
            parsed.entity.text.clone(),
            Some("kanban section".to_string()),
            EditorSemanticKind::Namespace,
            parsed.entity.span,
            parsed.entity.span,
        ));
    } else {
        facts.push_symbol(EditorSemanticSymbol::new(
            parsed.entity.text.clone(),
            Some("kanban item".to_string()),
            fact_kind,
            parsed.entity.span,
            parsed.entity.span,
        ));
    }
    if let Some(label) = parsed.label {
        facts.push_symbol(EditorSemanticSymbol::payload(
            label.text,
            Some("kanban label".to_string()),
            EditorSemanticKind::String,
            label.span,
            label.span,
        ));
    }
    control.checkpoint()?;
    Ok(Ok(()))
}

fn push_kanban_metadata_facts(facts: &mut EditorSemanticFacts, fields: &[KanbanMetadataField]) {
    for field in fields {
        facts.push_symbol(EditorSemanticSymbol::payload(
            field.key.text.clone(),
            Some("kanban metadata key".to_string()),
            EditorSemanticKind::Property,
            field.key.span,
            field.key.span,
        ));
        if field.is_string_value
            && let Some(value) = &field.value
        {
            facts.push_symbol(EditorSemanticSymbol::payload(
                value.text.clone(),
                Some(format!("kanban metadata {}", field.key.text)),
                EditorSemanticKind::String,
                value.span,
                value.span,
            ));
        }
    }
}

fn kanban_node_editor_kind(ty: i32) -> EditorSemanticKind {
    if matches!(
        ty,
        NODE_TYPE_CIRCLE | NODE_TYPE_CLOUD | NODE_TYPE_BANG | NODE_TYPE_HEXAGON
    ) {
        EditorSemanticKind::Object
    } else {
        EditorSemanticKind::Variable
    }
}

fn kanban_error_span(error: &Error, fallback: SourceSpan) -> SourceSpan {
    match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.span().unwrap_or(fallback),
        _ => fallback,
    }
}

pub(crate) fn parse_kanban(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let source = construct_kanban_semantic_source(code, meta).map_err(|failure| *failure.error)?;
    let model = kanban_db_into_render_model(&source.db, meta);
    render_model_to_compat_json(&model, meta)
}

pub(crate) fn parse_kanban_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    control.checkpoint()?;
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construct_kanban_semantic_source_controlled(code, meta, control)?,
        |source| {
            let model = kanban_db_into_render_model(&source.db, meta);
            (
                render_model_to_compat_json(&model, meta),
                source.editor_facts,
            )
        },
        KanbanParseFailure::into_error_and_editor_facts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

fn kanban_db_into_render_model(db: &KanbanDb, meta: &ParseMetadata) -> KanbanDiagramRenderModel {
    KanbanDiagramRenderModel {
        nodes: db.data_nodes_for_render(&meta.effective_config),
    }
}

pub(crate) fn render_model_to_compat_json(
    model: &KanbanDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let mut out = Map::with_capacity(6);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("sections".to_string(), kanban_sections_to_json(model));
    out.insert(
        "nodes".to_string(),
        Value::Array(kanban_nodes_to_json(model, &meta.effective_config)),
    );
    out.insert("edges".to_string(), Value::Array(Vec::new()));
    out.insert("other".to_string(), Value::Object(Map::new()));
    out.insert(
        "config".to_string(),
        crate::config::clone_value_nonrecursive(meta.effective_config.as_value()),
    );
    Ok(Value::Object(out))
}

fn kanban_sections_to_json(model: &KanbanDiagramRenderModel) -> Value {
    let sections = model
        .nodes
        .iter()
        .filter(|node| node.is_group)
        .map(|node| {
            let compat = &node.compatibility;
            let mut out = Map::new();
            out.insert("id".to_string(), json!(&node.id));
            out.insert("label".to_string(), json!(&node.label));
            out.insert("level".to_string(), json!(compat.level));
            out.insert("width".to_string(), json!(compat.width));
            out.insert("padding".to_string(), json!(compat.padding));
            out.insert("isGroup".to_string(), json!(false));
            for (key, value) in [
                ("ticket", &compat.ticket),
                ("priority", &compat.priority),
                ("assigned", &compat.assigned),
                ("icon", &compat.icon),
                ("cssClasses", &compat.css_classes),
                ("shape", &compat.shape),
            ] {
                if let Some(value) = value {
                    out.insert(key.to_string(), json!(value));
                }
            }
            Value::Object(out)
        })
        .collect();
    Value::Array(sections)
}

fn kanban_nodes_to_json(model: &KanbanDiagramRenderModel, config: &MermaidConfig) -> Vec<Value> {
    let look = config.get_str("look").unwrap_or("classic");
    model
        .nodes
        .iter()
        .map(|node| {
            if node.is_group {
                json!({
                    "id": node.id,
                    "label": node.label,
                    "isGroup": true,
                    "ticket": node.ticket,
                    "shape": "kanbanSection",
                    "level": node.compatibility.level,
                    "look": look,
                })
            } else {
                json!({
                    "id": node.id,
                    "parentId": node.parent_id,
                    "label": node.label,
                    "isGroup": false,
                    "ticket": node.ticket,
                    "priority": node.priority,
                    "assigned": node.assigned,
                    "icon": node.icon,
                    "shape": "kanbanItem",
                    "level": node.compatibility.level,
                    "rx": 5,
                    "ry": 5,
                    "cssStyles": ["text-align: left"],
                })
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn parse_kanban_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<KanbanDiagramRenderModel> {
    let source = construct_kanban_semantic_source(code, meta).map_err(|failure| *failure.error)?;
    Ok(kanban_db_into_render_model(&source.db, meta))
}

pub(crate) fn parse_kanban_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<KanbanDiagramRenderModel>> {
    let construction = construct_kanban_semantic_source_controlled(code, meta, control)?;
    match construction {
        Ok(source) => {
            control.checkpoint()?;
            Ok(Ok(kanban_db_into_render_model(&source.db, meta)))
        }
        Err(failure) => Ok(Err(*failure.error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorSemanticCompleteness, Engine, MermaidConfig, ParseDiagnosticSpanKind, ParseMetadata,
        ParseOptions,
    };
    use futures::executor::block_on;

    fn parse(text: &str) -> crate::ManagedSemanticJson {
        let engine = Engine::new();
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model
    }

    fn parse_err(text: &str) -> crate::ParseDiagnostic {
        let engine = Engine::new();
        match block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err() {
            Error::DiagramParse { diagnostic, .. } => diagnostic,
            other => panic!("expected kanban parse error, got {other:?}"),
        }
    }

    fn meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "kanban".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    #[test]
    fn combined_parse_constructs_once_and_preserves_all_projections() {
        let text = concat!(
            "kanban\r\n",
            "  backlog\r\n",
            "    task1@{ ticket: MC-1 }\r\n",
            "    ::icon(star)\r\n",
            "    :::highlight\r\n",
        );
        let mut meta = meta();
        meta.effective_config = MermaidConfig::from_value(serde_json::json!({
            "look": "handDrawn",
            "theme": "forest"
        }));
        let expected_json = parse_kanban(text, &meta).unwrap();
        let expected_model = parse_kanban_model_for_render(text, &meta).unwrap();

        reset_kanban_syntax_construction_count();
        let (json, facts) = crate::family::test_support::into_result(
            parse_kanban_json_and_editor_facts(text, &meta, &crate::OperationControl::new()),
        )
        .unwrap();

        assert_eq!(kanban_syntax_construction_count(), 1);
        assert_eq!(json, expected_json);
        assert!(!facts.symbols.is_empty());
        assert_eq!(
            render_model_to_compat_json(&expected_model, &meta).unwrap(),
            json
        );
        assert_eq!(json["type"], "kanban");
        assert_eq!(json["config"], meta.effective_config.as_value().clone());
        assert_eq!(json["nodes"][0]["look"], "handDrawn");
        assert!(json["nodes"][0]["ticket"].is_null());

        for name in ["backlog", "task1", "star", "highlight"] {
            let start = text.find(name).unwrap();
            assert!(
                facts.symbols.iter().any(|symbol| {
                    symbol.name == name
                        && symbol.selection == SourceSpan::new(start, start + name.len())
                }),
                "missing exact Kanban fact for {name:?}"
            );
        }
    }

    #[test]
    fn malformed_editor_input_recovers_from_one_construction() {
        let text = "kanban\n  backlog[Backlog]\n    broken[Open\n";
        reset_kanban_syntax_construction_count();
        let facts = crate::family::test_support::editor_facts(
            parse_kanban_json_and_editor_facts,
            text,
            &meta(),
        );

        assert_eq!(kanban_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "backlog"));
        assert_eq!(facts.diagnostics.len(), 1);
    }

    #[test]
    fn kanban_recovery_keeps_first_error_prefix_and_later_safe_statements() {
        let text = concat!(
            "kanban\r\n",
            "  todo[\"之前\"]\r\n",
            "    broken[\"Open 🤓\r\n",
            "    invalid[Valid] trailing\r\n",
            "    later[\"后来\"]@{ ticket: 42, active: false }\r\n",
        );
        let strict_diagnostic = parse_err(text);
        let broken_end = text.find("\r\n    invalid").unwrap();
        assert_eq!(
            strict_diagnostic.span(),
            Some(SourceSpan::new(broken_end, broken_end))
        );

        reset_kanban_syntax_construction_count();
        let facts = crate::family::test_support::editor_facts(
            parse_kanban_json_and_editor_facts,
            text,
            &meta(),
        );
        assert_eq!(kanban_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert_eq!(facts.diagnostics.len(), 1);
        assert_eq!(facts.diagnostics[0].span, strict_diagnostic.span());
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "todo"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "later"));
        assert!(!facts.symbols.iter().any(|symbol| symbol.name == "invalid"));
    }

    fn sections(model: &Value) -> Vec<Value> {
        model["sections"].as_array().cloned().unwrap_or_default()
    }

    fn data_nodes(model: &Value) -> Vec<Value> {
        model["nodes"].as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn parse_kanban_editor_facts_expose_parser_backed_spans() {
        let engine = Engine::new();
        let text = "kanban\n    root\n      child1\n    :::highlight\n";
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("kanban", text)
            .unwrap()
            .unwrap();

        assert!(facts.symbols.iter().any(|symbol| symbol.name == "root"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "child1"));
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "highlight")
        );
    }

    #[test]
    fn kanban_recovered_editor_fact_diagnostics_are_english() {
        let engine = Engine::new();
        let text = "kanban\n  broken[Open\n";
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("kanban", text)
            .unwrap()
            .unwrap();

        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .starts_with("kanban parser recovered after parse error:")
        }));
        assert!(facts.diagnostics.iter().all(|diagnostic| {
            !diagnostic
                .message
                .chars()
                .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch))
        }));
    }

    #[test]
    fn kanban_unterminated_node_delimiter_reports_insertion_point() {
        let text = "kanban\n  root[Open\n";
        let diagnostic = parse_err(text);
        let offset = text.trim_end().len();

        assert_eq!(diagnostic.message(), "unterminated node delimiter");
        assert_eq!(diagnostic.span(), Some(SourceSpan::new(offset, offset)));
        assert_eq!(
            diagnostic.span_kind(),
            ParseDiagnosticSpanKind::InsertionPoint
        );
    }

    #[test]
    fn kanban_trailing_node_input_reports_exact_span() {
        let text = "kanban\n  root[Root] extra\n";
        let diagnostic = parse_err(text);
        let offset = text.find("extra").unwrap();

        assert_eq!(diagnostic.message(), "unexpected trailing input");
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(offset, offset + "extra".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn kanban_unterminated_metadata_reports_eof_insertion_point() {
        let text = "kanban\n  root@{ icon: star\n";
        let diagnostic = parse_err(text);

        assert_eq!(diagnostic.message(), "unterminated @{ ... } metadata block");
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(text.len(), text.len()))
        );
        assert_eq!(
            diagnostic.span_kind(),
            ParseDiagnosticSpanKind::InsertionPoint
        );
    }

    #[test]
    fn kanban_invalid_shape_metadata_reports_exact_metadata_span() {
        let text = "kanban\n  root@{ shape: bad_shape }\n";
        let diagnostic = parse_err(text);
        let offset = text.find("@{").unwrap();

        assert!(diagnostic.message().contains("No such shape"));
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(
                offset,
                offset + "@{ shape: bad_shape }".len()
            ))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn knbn_1_simple_root() {
        let model = parse("kanban\n    root");
        let sections = sections(&model);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0]["label"].as_str().unwrap(), "root");
    }

    #[test]
    fn knbn_2_hierarchy_two_children() {
        let model = parse("kanban\n    root\n      child1\n      child2\n");
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0]["label"].as_str().unwrap(), "root");
        assert_eq!(children.len(), 2);
        assert_eq!(children[0]["label"].as_str().unwrap(), "child1");
        assert_eq!(children[1]["label"].as_str().unwrap(), "child2");
    }

    #[test]
    fn knbn_3_shape_without_id() {
        let model = parse("kanban\n    (root)");
        let sections = sections(&model);
        assert_eq!(sections[0]["label"].as_str().unwrap(), "root");
    }

    #[test]
    fn knbn_4_does_not_distinguish_deeper_levels() {
        let model = parse("kanban\n    root\n      child1\n        leaf1\n      child2");
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections.len(), 1);
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn knbn_5_multiple_sections() {
        let model = parse("kanban\n    section1\n    section2");
        let sections = sections(&model);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0]["label"].as_str().unwrap(), "section1");
        assert_eq!(sections[1]["label"].as_str().unwrap(), "section2");
    }

    #[test]
    fn knbn_6_real_root_in_wrong_place_is_error() {
        let engine = Engine::new();
        let text = "kanban\n          root\n        fakeRoot\n    realRootWrongPlace";
        let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
        assert!(
            err.to_string()
                .contains("Items without section detected, found section (\"fakeRoot\")")
        );
    }

    #[test]
    fn knbn_7_id_and_label_rect() {
        let model = parse("kanban\n    root[The root]\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(sections[0]["label"].as_str().unwrap(), "The root");
    }

    #[test]
    fn knbn_8_child_id_and_label() {
        let model = parse("kanban\n    root\n      theId(child1)");
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections[0]["label"].as_str().unwrap(), "root");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["label"].as_str().unwrap(), "child1");
        assert_eq!(children[0]["id"].as_str().unwrap(), "theId");
    }

    #[test]
    fn knbn_9_child_id_and_label_without_indent_on_root() {
        let model = parse("kanban\nroot\n      theId(child1)");
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections[0]["label"].as_str().unwrap(), "root");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["label"].as_str().unwrap(), "child1");
        assert_eq!(children[0]["id"].as_str().unwrap(), "theId");
    }

    #[test]
    fn knbn_13_set_icon_for_node() {
        let model = parse("kanban\n    root[The root]\n    ::icon(bomb)\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(sections[0]["label"].as_str().unwrap(), "The root");
        assert_eq!(sections[0]["icon"].as_str().unwrap(), "bomb");
    }

    #[test]
    fn empty_icon_decoration_is_an_upstream_compatible_noop() {
        let text = "kanban\n    root[The root]\n    ::icon()\n";
        let model = parse(text);
        let sections = sections(&model);
        assert!(sections[0].get("icon").is_none());

        let (_, facts) = crate::family::test_support::into_result(
            parse_kanban_json_and_editor_facts(text, &meta(), &crate::OperationControl::new()),
        )
        .unwrap();
        assert!(facts.directive_prefixes.iter().any(|value| value == "icon"));
        assert!(facts.symbols.iter().all(|symbol| symbol.name != "icon"));
    }

    #[test]
    fn knbn_14_set_classes_for_node() {
        let model = parse("kanban\n    root[The root]\n    :::m-4 p-8\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(sections[0]["label"].as_str().unwrap(), "The root");
        assert_eq!(sections[0]["cssClasses"].as_str().unwrap(), "m-4 p-8");
    }

    #[test]
    fn knbn_15_set_classes_and_icon_classes_first() {
        let model = parse("kanban\n    root[The root]\n    :::m-4 p-8\n    ::icon(bomb)\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["cssClasses"].as_str().unwrap(), "m-4 p-8");
        assert_eq!(sections[0]["icon"].as_str().unwrap(), "bomb");
    }

    #[test]
    fn knbn_16_set_classes_and_icon_icon_first() {
        let model = parse("kanban\n    root[The root]\n    ::icon(bomb)\n    :::m-4 p-8\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["cssClasses"].as_str().unwrap(), "m-4 p-8");
        assert_eq!(sections[0]["icon"].as_str().unwrap(), "bomb");
    }

    #[test]
    fn knbn_17_node_syntax_in_description() {
        let model = parse("kanban\n    root[\"String containing []\"]\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(
            sections[0]["label"].as_str().unwrap(),
            "String containing []"
        );
    }

    #[test]
    fn knbn_18_node_syntax_in_child_description() {
        let model = parse(
            "kanban\n    root[\"String containing []\"]\n      child1[\"String containing ()\"]\n",
        );
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(
            sections[0]["label"].as_str().unwrap(),
            "String containing []"
        );
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0]["label"].as_str().unwrap(),
            "String containing ()"
        );
    }

    #[test]
    fn knbn_19_child_after_class_assignment() {
        let model = parse(
            "kanban\n  root(Root)\n    Child(Child)\n    :::hot\n      a(a)\n      b[New Stuff]",
        );
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(sections[0]["label"].as_str().unwrap(), "Root");
        assert_eq!(children.len(), 3);
        assert_eq!(children[0]["id"].as_str().unwrap(), "Child");
        assert_eq!(children[1]["id"].as_str().unwrap(), "a");
        assert_eq!(children[2]["id"].as_str().unwrap(), "b");
    }

    #[test]
    fn knbn_20_empty_rows() {
        let model =
            parse("kanban\n  root(Root)\n    Child(Child)\n      a(a)\n\n      b[New Stuff]");
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(sections[0]["label"].as_str().unwrap(), "Root");
        assert_eq!(children.len(), 3);
        assert_eq!(children[0]["id"].as_str().unwrap(), "Child");
        assert_eq!(children[1]["id"].as_str().unwrap(), "a");
        assert_eq!(children[2]["id"].as_str().unwrap(), "b");
    }

    #[test]
    fn knbn_22_inline_comment_at_end_of_line() {
        let model = parse(
            "kanban\n  root(Root)\n    Child(Child)\n      a(a) %% This is a comment\n      b[New Stuff]",
        );
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(children.len(), 3);
        assert_eq!(children[0]["id"].as_str().unwrap(), "Child");
        assert_eq!(children[1]["id"].as_str().unwrap(), "a");
        assert_eq!(children[2]["id"].as_str().unwrap(), "b");
    }

    #[test]
    fn knbn_23_rows_with_only_spaces_should_not_interfere() {
        let model = parse("kanban\nroot\n A\n \n\n B");
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0]["id"].as_str().unwrap(), "A");
        assert_eq!(children[1]["id"].as_str().unwrap(), "B");
    }

    #[test]
    fn knbn_24_rows_above_header() {
        let model = parse("\n \nkanban\nroot\n A\n \n\n B");
        let sections = sections(&model);
        let nodes = data_nodes(&model);
        let section_id = sections[0]["id"].as_str().unwrap();
        let children: Vec<Value> = nodes
            .into_iter()
            .filter(|n| n["parentId"].as_str() == Some(section_id))
            .collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0]["id"].as_str().unwrap(), "A");
        assert_eq!(children[1]["id"].as_str().unwrap(), "B");
    }

    #[test]
    fn knbn_30_priority_metadata() {
        let model = parse("kanban\n        root@{ priority: high }\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["id"].as_str().unwrap(), "root");
        assert_eq!(sections[0]["priority"].as_str().unwrap(), "high");
    }

    #[test]
    fn knbn_31_assigned_metadata() {
        let model = parse("kanban\n        root@{ assigned: knsv }\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["assigned"].as_str().unwrap(), "knsv");
    }

    #[test]
    fn knbn_32_icon_metadata() {
        let model = parse("kanban\n        root@{ icon: star }\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["icon"].as_str().unwrap(), "star");
    }

    #[test]
    fn knbn_34_multiline_metadata() {
        let model = parse(
            "kanban\n        root@{\n          icon: star\n          assigned: knsv\n        }\n",
        );
        let sections = sections(&model);
        assert_eq!(sections[0]["icon"].as_str().unwrap(), "star");
        assert_eq!(sections[0]["assigned"].as_str().unwrap(), "knsv");
    }

    #[test]
    fn knbn_35_inline_metadata_multiple_pairs() {
        let model = parse("kanban\n        root@{ icon: star, assigned: knsv }\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["icon"].as_str().unwrap(), "star");
        assert_eq!(sections[0]["assigned"].as_str().unwrap(), "knsv");
    }

    #[test]
    fn knbn_36_label_override_metadata() {
        let model = parse("kanban\n        root@{ icon: star, label: 'fix things' }\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["label"].as_str().unwrap(), "fix things");
    }

    #[test]
    fn knbn_37_ticket_metadata() {
        let model = parse("kanban\n        root@{ ticket: MC-1234 }\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["ticket"].as_str().unwrap(), "MC-1234");
    }

    #[test]
    fn kanban_get_data_sanitizes_labels_again() {
        let model = parse("kanban\n    root[<b>x</b>]");
        let nodes = data_nodes(&model);
        assert_eq!(nodes[0]["label"].as_str().unwrap(), "<b>x</b>");
    }

    #[test]
    fn kanban_shape_data_rewrites_newline_whitespace_in_double_quotes() {
        let model = parse("kanban\n  root@{ label: \"line1\n      line2\" }\n");
        let sections = sections(&model);
        assert_eq!(sections[0]["label"].as_str().unwrap(), "line1<br/>line2");
    }
}
