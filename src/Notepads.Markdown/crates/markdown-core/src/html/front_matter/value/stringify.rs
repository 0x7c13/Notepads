// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! yaml.stringify(JSValue).trimEnd() for the frontmatter value domain.
//! String quoting and schema decisions follow yaml 2.9.0 (ISC):
//! Copyright Eemeli Aro <eemeli@gmail.com>, https://github.com/eemeli/yaml/tree/v2.9.0/src/stringify

use super::{MAX_DEPTH, MAX_NODES, ResultValue, Value, limited, scalar as schema};
use crate::policy::Operation;
use saphyr_parser::ScalarStyle;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub(super) fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) if value.is_nan() => ".nan".to_owned(),
        Value::Number(value) if *value == f64::INFINITY => ".inf".to_owned(),
        Value::Number(value) if *value == f64::NEG_INFINITY => "-.inf".to_owned(),
        Value::Number(value) if *value == 0.0 && value.is_sign_negative() => "-0".to_owned(),
        Value::Number(value) => schema::number(*value),
        Value::Text(value) => string(value, false, ScalarStyle::Plain),
        Value::Date(value) => value.clone(),
        _ => String::new(),
    }
}

pub(super) fn string(value: &str, flow: bool, style: ScalarStyle) -> String {
    if style == ScalarStyle::SingleQuoted && !has_control(value) {
        return format!("'{}'", value.replace('\'', "''"));
    }
    if style == ScalarStyle::DoubleQuoted || has_control(value) {
        return double_quote(value);
    }
    if plain_allowed(value, flow) {
        return value.to_owned();
    }
    if value.contains('"') && !value.contains('\'') && !value.contains('\n') && !has_control(value)
    {
        format!("'{value}'")
    } else {
        double_quote(value)
    }
}

fn has_control(value: &str) -> bool {
    value
        .chars()
        .any(|c| matches!(c, '\0'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}'..='\u{9f}'))
}
fn plain_allowed(value: &str, flow: bool) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    if matches!(
        first,
        '\n' | '\t'
            | ' '
            | ','
            | '['
            | ']'
            | '{'
            | '}'
            | '#'
            | '&'
            | '*'
            | '!'
            | '|'
            | '>'
            | '\''
            | '"'
            | '%'
            | '@'
            | '`'
    ) {
        return false;
    }
    if value == "-"
        || value == "?"
        || value.starts_with("- ")
        || value.starts_with("? ")
        || value.starts_with("-\t")
        || value.starts_with("?\t")
    {
        return false;
    }
    if value.ends_with(['\n', '\t', ' ', ':'])
        || value.contains('\n')
        || value.contains(": ")
        || value.contains(":\t")
    {
        return false;
    }
    if value.contains(" #") || value.contains("\t#") {
        return false;
    }
    if flow && value.contains(['[', ']', '{', '}', ',']) {
        return false;
    }
    matches!(
        schema::resolve(value, ScalarStyle::Plain, None, false),
        Ok(Value::Text(_))
    )
}
fn double_quote(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for c in value.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => result.push_str("\\0"),
            '\u{7}' => result.push_str("\\a"),
            '\u{8}' => result.push_str("\\b"),
            '\u{b}' => result.push_str("\\v"),
            '\u{c}' => result.push_str("\\f"),
            '\u{1b}' => result.push_str("\\e"),
            '\u{85}' => result.push_str("\\N"),
            c if c <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&c) => {
                result.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn identity(value: &Rc<Value>) -> usize {
    Rc::as_ptr(value) as usize
}
fn is_object(value: &Value) -> bool {
    matches!(
        value,
        Value::Array(_)
            | Value::Object(_)
            | Value::Set(_)
            | Value::OrderedMap(_)
            | Value::Date(_)
            | Value::Binary(_)
    )
}
fn block(value: &Value) -> bool {
    match value {
        Value::Array(v) | Value::Set(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::OrderedMap(v) => !v.is_empty(),
        Value::Binary(_) => true,
        _ => false,
    }
}

pub(super) fn value(value: &Rc<Value>, limit: usize, op: &mut Operation) -> ResultValue<String> {
    let mut writer = Writer {
        output: String::new(),
        limit,
        refs: HashMap::new(),
        anchors: HashMap::new(),
        emitted: HashSet::new(),
        nodes: 0,
    };
    writer.count(value, 0, &mut HashSet::new(), op)?;
    writer.nodes = 0;
    writer.emit(value, 0, 0, op)?;
    // JS trimEnd includes Unicode whitespace, not only ASCII whitespace.
    let length = writer.output.trim_end_matches(js_whitespace).len();
    writer.output.truncate(length);
    Ok(writer.output)
}
fn js_whitespace(c: char) -> bool {
    matches!(c, '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

struct Writer {
    output: String,
    limit: usize,
    refs: HashMap<usize, usize>,
    anchors: HashMap<usize, usize>,
    emitted: HashSet<usize>,
    nodes: usize,
}
impl Writer {
    fn push(&mut self, text: &str) -> ResultValue<()> {
        if self.output.len().saturating_add(text.len()) > self.limit {
            return Err(limited("1 MiB generated output"));
        }
        self.output
            .try_reserve(text.len())
            .map_err(|_| crate::error::CoreError::OutOfMemory)?;
        self.output.push_str(text);
        Ok(())
    }
    fn newline(&mut self, indent: usize) -> ResultValue<()> {
        self.push("\n")?;
        self.push(&" ".repeat(indent))
    }
    fn visit(&mut self, depth: usize, op: &mut Operation) -> ResultValue<()> {
        op.events(1)?;
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(limited("16,384 stringified node"));
        }
        if depth > MAX_DEPTH {
            return Err(limited("64 level stringify"));
        }
        Ok(())
    }
    fn count(
        &mut self,
        value: &Rc<Value>,
        depth: usize,
        seen: &mut HashSet<usize>,
        op: &mut Operation,
    ) -> ResultValue<()> {
        self.visit(depth, op)?;
        if is_object(value) {
            *self.refs.entry(identity(value)).or_default() += 1;
            if !seen.insert(identity(value)) {
                return Ok(());
            }
        }
        match value.as_ref() {
            Value::Array(values) | Value::Set(values) => {
                for value in values {
                    self.count(value, depth + 1, seen, op)?;
                }
            }
            Value::Object(values) => {
                for (_, value) in values {
                    self.count(value, depth + 1, seen, op)?;
                }
            }
            Value::OrderedMap(values) => {
                for (key, value) in values {
                    self.count(key, depth + 1, seen, op)?;
                    self.count(value, depth + 1, seen, op)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn alias(&self, value: &Rc<Value>) -> bool {
        self.emitted.contains(&identity(value)) && self.anchors.contains_key(&identity(value))
    }
    fn needs_anchor(&self, value: &Rc<Value>) -> bool {
        self.refs.get(&identity(value)).is_some_and(|&n| n > 1)
    }
    fn emit(
        &mut self,
        value: &Rc<Value>,
        indent: usize,
        depth: usize,
        op: &mut Operation,
    ) -> ResultValue<()> {
        self.visit(depth, op)?;
        if self.needs_anchor(value) {
            let id = identity(value);
            let next = self.anchors.len() + 1;
            let anchor = *self.anchors.entry(id).or_insert(next);
            if !self.emitted.insert(id) {
                return self.push(&format!("*a{anchor}"));
            }
            self.push(&format!("&a{anchor}"))?;
            if block(value) {
                self.newline(indent)?;
            } else {
                self.push(" ")?;
            }
        }
        match value.as_ref() {
            Value::Array(values) | Value::Set(values) => {
                if values.is_empty() {
                    return self.push("[]");
                }
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        self.newline(indent)?;
                    }
                    self.push("-")?;
                    // yaml places an unanchored nested collection immediately
                    // after '- ', with its continuation indented by two.
                    self.push(" ")?;
                    self.emit(value, indent + 2, depth + 1, op)?;
                }
                Ok(())
            }
            Value::Object(values) => {
                if values.is_empty() {
                    return self.push("{}");
                }
                for (index, (key, value)) in values.iter().enumerate() {
                    if index != 0 {
                        self.newline(indent)?;
                    }
                    self.push(&string(key, true, ScalarStyle::Plain))?;
                    self.push(":")?;
                    self.child(value, indent + 2, depth + 1, op)?;
                }
                Ok(())
            }
            Value::OrderedMap(values) => {
                if values.is_empty() {
                    return self.push("{}");
                }
                for (index, (key, value)) in values.iter().enumerate() {
                    if index != 0 {
                        self.newline(indent)?;
                    }
                    if is_object(key) && !matches!(key.as_ref(), Value::Date(_)) {
                        self.push("? ")?;
                        self.emit(key, indent + 2, depth + 1, op)?;
                        self.newline(indent)?;
                        self.push(":")?;
                    } else {
                        self.push(&scalar(key))?;
                        self.push(":")?;
                    }
                    self.child(value, indent + 2, depth + 1, op)?;
                }
                Ok(())
            }
            Value::Binary(bytes) => {
                self.push("type: Buffer")?;
                self.newline(indent)?;
                self.push("data:")?;
                if bytes.is_empty() {
                    return self.push(" []");
                }
                for byte in bytes {
                    self.visit(depth + 2, op)?;
                    self.newline(indent + 2)?;
                    self.push(&format!("- {byte}"))?;
                }
                Ok(())
            }
            Value::Text(text) if text.contains('\n') && !has_control(text) => {
                self.block_string(text, indent)
            }
            Value::Text(_) => {
                let text = scalar(value);
                let start = self
                    .output
                    .rsplit('\n')
                    .next()
                    .unwrap_or("")
                    .chars()
                    .count()
                    .saturating_sub(indent.saturating_sub(2));
                self.push(&fold(&text, indent, start, text.starts_with('"')))
            }
            _ => self.push(&scalar(value)),
        }
    }
    fn child(
        &mut self,
        value: &Rc<Value>,
        indent: usize,
        depth: usize,
        op: &mut Operation,
    ) -> ResultValue<()> {
        if block(value) && !self.alias(value) && !self.needs_anchor(value) {
            self.newline(indent)?;
        } else {
            self.push(" ")?;
        }
        self.emit(value, indent, depth, op)
    }
    fn block_string(&mut self, text: &str, indent: usize) -> ResultValue<()> {
        if text.trim_matches([' ', '\t', '\n']).is_empty()
            || text
                .rsplit('\n')
                .next()
                .is_some_and(|s| !s.is_empty() && s.trim_matches([' ', '\t']).is_empty())
        {
            return self.push(&double_quote(text));
        }
        let trailing = text.len() - text.trim_end_matches('\n').len();
        let starts_space = text.trim_start_matches('\n').starts_with(' ');
        self.push("|")?;
        if starts_space {
            self.push(if indent == 0 { "1" } else { "2" })?;
        }
        self.push(if trailing == 0 {
            "-"
        } else if trailing == 1 {
            ""
        } else {
            "+"
        })?;
        let body = if trailing != 0 {
            &text[..text.len() - 1]
        } else {
            text
        };
        for line in body.split('\n') {
            self.newline(if indent == 0 { 2 } else { indent })?;
            self.push(line)?;
        }
        Ok(())
    }
}

// Adapted from yaml 2.9.0 stringify/foldFlowLines.js (ISC, Eemeli Aro).
// Rust scalar boundaries remain intact when folding non-BMP UTF-8 text.
fn fold(text: &str, indent: usize, at_start: usize, quoted: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let step = 21usize.max(81usize.saturating_sub(indent));
    if chars.len() <= step {
        return text.to_owned();
    }
    let mut end = 80usize.saturating_sub(at_start);
    let mut folds = Vec::new();
    let mut escaped = HashSet::new();
    let mut split = None;
    let mut previous = '\0';
    let mut escape_start = 0usize;
    let mut escape_end = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let mut c = chars[i];
        if quoted && c == '\\' {
            escape_start = i;
            i += match chars.get(i + 1) {
                Some('x') => 3,
                Some('u') => 5,
                Some('U') => 9,
                _ => 1,
            };
            escape_end = i;
        }
        if c == '\n' {
            end = i + indent + step;
            split = None;
        } else {
            if c == ' '
                && !matches!(previous, '\0' | ' ' | '\n' | '\t')
                && chars
                    .get(i + 1)
                    .is_some_and(|c| !matches!(c, ' ' | '\n' | '\t'))
            {
                split = Some(i);
            }
            if i >= end {
                if let Some(at) = split.take() {
                    folds.push(at);
                    end = at + step;
                } else if quoted {
                    while matches!(previous, ' ' | '\t') {
                        previous = c;
                        i += 1;
                        c = chars.get(i).copied().unwrap_or('\0');
                    }
                    let at = if i > escape_end + 1 {
                        i.saturating_sub(2)
                    } else {
                        let Some(at) = escape_start.checked_sub(1) else {
                            return text.to_owned();
                        };
                        at
                    };
                    if !escaped.insert(at) {
                        return text.to_owned();
                    }
                    folds.push(at);
                    end = at + step;
                    split = None;
                }
            }
        }
        previous = c;
        i += 1;
    }
    let Some(&first) = folds.first() else {
        return text.to_owned();
    };
    let mut output: String = chars[..first].iter().collect();
    for (index, &at) in folds.iter().enumerate() {
        let end = folds.get(index + 1).copied().unwrap_or(chars.len());
        if quoted && escaped.contains(&at) {
            output.push(chars[at]);
            output.push('\\');
        }
        output.push('\n');
        output.push_str(&" ".repeat(indent));
        output.extend(chars[at + 1..end].iter());
    }
    output
}
