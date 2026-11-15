// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded event-to-value adapter for VS Code 1.145.2 / yaml 2.9.0 front matter.
//! YAML scalar/key/alias semantics follow yaml 2.9.0 (ISC), Copyright Eemeli Aro
//! <eemeli@gmail.com>; see third-party-notices. No JavaScript executes here.
//! The renderer contract follows Microsoft's MIT-licensed yamlPreamble.ts:
//! https://github.com/microsoft/vscode/blob/1.145.2/extensions/markdown-language-features/src/extensions/yamlPreamble/yamlPreamble.ts

#[path = "value/scalar.rs"]
mod scalar;
#[path = "value/stringify.rs"]
mod stringify;
#[cfg(test)]
#[path = "value/tests.rs"]
mod tests;

use crate::error::CoreError;
use crate::html::citations::{MAX_REFERENCES, Reference, valid_key};
use crate::policy::Operation;
use saphyr_parser::{Event, Marker, Parser, ScalarStyle, Tag};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const MAX_BYTES: usize = 256 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 16_384;
// This absolute alias bound is an additional product limit. yaml's default 100
// is an expansion *ratio*, which is checked separately in Converter::convert.
const MAX_ALIASES: usize = 100;
const MAX_OUTPUT: usize = 1024 * 1024;
const BYTE_WINDOW: u64 = 16 * 1024;
const EVENT_WINDOW: u64 = 256;

#[derive(Debug, PartialEq)]
pub(super) struct Entry {
    pub key: String,
    pub value: DisplayValue,
}

#[derive(Debug, PartialEq)]
pub(super) enum DisplayValue {
    Text(String),
    Code(String),
    List(Vec<DisplayValue>),
}

#[derive(Debug)]
enum Value {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Rc<Value>>),
    Object(Vec<(String, Rc<Value>)>),
    Set(Vec<Rc<Value>>),
    OrderedMap(Vec<(Rc<Value>, Rc<Value>)>),
    Date(String),    // Always Date.toISOString(), before yaml.stringify's shortening.
    Binary(Vec<u8>), // Node's yaml.parse returns Buffer, rather than Uint8Array.
}

#[derive(Debug)]
enum Failure {
    Core(CoreError),
    Yaml(String),
}
impl From<CoreError> for Failure {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}
impl From<String> for Failure {
    fn from(error: String) -> Self {
        Self::Yaml(error)
    }
}
type ResultValue<T> = Result<T, Failure>;

fn limited(what: &str) -> Failure {
    Failure::Yaml(format!("Frontmatter exceeds the {what} limit"))
}

pub(super) fn scan_chunks(op: &mut Operation, mut bytes: u64) -> Result<(), CoreError> {
    while bytes != 0 {
        op.checkpoint_now()?;
        let chunk = bytes.min(BYTE_WINDOW);
        op.scan(chunk)?;
        bytes -= chunk;
    }
    Ok(())
}

pub(super) fn copy_chunks(op: &mut Operation, mut bytes: u64) -> Result<(), CoreError> {
    while bytes != 0 {
        op.checkpoint_now()?;
        let chunk = bytes.min(BYTE_WINDOW);
        op.copy(chunk)?;
        bytes -= chunk;
    }
    Ok(())
}

pub(super) fn event_chunks(op: &mut Operation, mut events: u64) -> Result<(), CoreError> {
    while events != 0 {
        op.checkpoint_now()?;
        let chunk = events.min(EVENT_WINDOW);
        op.events(chunk)?;
        events -= chunk;
    }
    Ok(())
}

fn alias_source(
    content: &str,
    start: usize,
    end: usize,
    op: &mut Operation,
) -> Result<String, CoreError> {
    let mut output = String::new();
    let mut read = 0u64;
    for (index, c) in content.chars().take(end).enumerate() {
        read += c.len_utf8() as u64;
        if index >= start {
            output
                .try_reserve(c.len_utf8())
                .map_err(|_| CoreError::OutOfMemory)?;
            output.push(c);
        }
        if read >= BYTE_WINDOW {
            scan_chunks(op, read)?;
            read = 0;
        }
    }
    scan_chunks(op, read)?;
    Ok(output)
}

struct Node {
    kind: Kind,
    tag: Option<String>,
    marker: Marker,
    source: String,
    style: ScalarStyle,
}
enum Kind {
    Scalar(Rc<Value>),
    Sequence(Vec<usize>),
    Mapping(Vec<(usize, usize)>),
    Alias(usize),
}
struct Ast {
    nodes: Vec<Node>,
    anchors: HashMap<usize, usize>,
    root: Option<usize>,
    yaml11: bool,
}
struct Frame {
    node: usize,
    key: Option<usize>,
}

/// The inner error is visible YAML/resource text; the outer error retains the
/// enclosing operation's first cancellation, deadline or budget fault.
#[derive(Default)]
pub(super) struct Metadata {
    pub entries: Vec<Entry>,
    pub references: Vec<Reference>,
}

#[cfg(test)]
pub(super) fn parse_entries(
    content: &str,
    op: &mut Operation,
) -> Result<Result<Vec<Entry>, String>, CoreError> {
    Ok(parse_metadata(content, true, false, op)?.map(|metadata| metadata.entries))
}

pub(super) fn parse_metadata(
    content: &str,
    table: bool,
    citations: bool,
    op: &mut Operation,
) -> Result<Result<Metadata, String>, CoreError> {
    let result = parse(content, table, citations, op);
    match result {
        Ok(entries) => Ok(Ok(entries)),
        Err(Failure::Yaml(message)) => {
            op.checkpoint_now()?;
            Ok(Err(message))
        }
        Err(Failure::Core(error)) => Err(op.fail(error)),
    }
}

fn parse(content: &str, table: bool, citations: bool, op: &mut Operation) -> ResultValue<Metadata> {
    op.checkpoint_now()?;
    if content.len() > MAX_BYTES {
        return Err(limited("256 KiB input"));
    }
    // Conservative scratch admission for both arenas, parser buffers, maps,
    // strings, traversal stacks and generated display. This is quota accounting
    // for bounded temporaries, not a claim to intercept every third-party alloc
    // or impose a hard process RSS limit. The caller admits retained projection.
    let scratch = content.len() * 16 + MAX_NODES * 384 + MAX_OUTPUT * 3;
    let _scratch = op.reserve_boundary(scratch as u64)?;
    let ast = parse_ast(content, op)?;
    let Some(root) = ast.root else {
        return Ok(Metadata::default());
    };
    let mut converter = Converter::new(&ast, content);
    let value = converter.convert(root, 0, op)?;
    expanded_nodes(&value, 0, &mut 0, op)?;
    let references = if citations {
        bibliography(&value, op)?
    } else {
        Vec::new()
    };
    if !table {
        return Ok(Metadata {
            entries: Vec::new(),
            references,
        });
    }
    let mut display = DisplayBudget { nodes: 0, bytes: 0 };
    let mut entries = Vec::new();
    match value.as_ref() {
        Value::Null | Value::Set(_) | Value::OrderedMap(_) | Value::Date(_) => {}
        Value::Object(pairs) => {
            entries
                .try_reserve(pairs.len())
                .map_err(|_| CoreError::OutOfMemory)?;
            for (key, value) in pairs {
                op.events(1)?;
                display.charge(key.len())?;
                entries.push(Entry {
                    key: key.clone(),
                    value: display.format(value, 0, op)?,
                });
            }
        }
        Value::Binary(bytes) => {
            entries
                .try_reserve(bytes.len())
                .map_err(|_| CoreError::OutOfMemory)?;
            for (index, byte) in bytes.iter().enumerate() {
                op.events(1)?;
                let key = index.to_string();
                let text = byte.to_string();
                display.charge(key.len() + text.len())?;
                entries.push(Entry {
                    key,
                    value: DisplayValue::Text(text),
                });
            }
        }
        _ => entries.push(Entry {
            key: String::new(),
            value: display.format(&value, 0, op)?,
        }),
    }
    copy_chunks(op, display.bytes as u64)?;
    op.checkpoint_now()?;
    Ok(Metadata {
        entries,
        references,
    })
}

fn member<'a>(
    value: &'a Value,
    key: &str,
    op: &mut Operation,
) -> Result<Option<&'a Value>, CoreError> {
    if let Value::Object(pairs) = value {
        for (name, value) in pairs {
            op.events(1)?;
            if name == key {
                return Ok(Some(value.as_ref()));
            }
        }
    }
    Ok(None)
}

fn field(value: Option<&Value>, maximum_bytes: usize) -> Option<&str> {
    let Some(Value::Text(value)) = value else {
        return None;
    };
    if value.len() > maximum_bytes {
        return None;
    }
    let value = value.trim();
    (!value.is_empty() && !value.chars().any(char::is_control)).then_some(value)
}

fn record(value: &Value, key: &str, op: &mut Operation) -> Result<Option<Reference>, CoreError> {
    let Some(title) = field(member(value, "title", op)?, 512) else {
        return Ok(None);
    };
    let Some(Value::Array(authors)) = member(value, "author", op)? else {
        return Ok(None);
    };
    if authors.is_empty() || authors.len() > 16 {
        return Ok(None);
    }
    let mut names = Vec::new();
    let mut byline = String::new();
    for author in authors {
        let family = field(member(author, "family", op)?, 128);
        let literal = field(member(author, "literal", op)?, 128);
        let Some(name) = literal.or(family) else {
            return Ok(None);
        };
        if !byline.is_empty() {
            byline.push_str(", ");
        }
        if literal.is_none()
            && let Some(given) = field(member(author, "given", op)?, 128)
        {
            byline.push_str(given);
            byline.push(' ');
        }
        byline.push_str(name);
        names.push(name);
    }
    let issued = member(value, "issued", op)?;
    let year = if let Some(issued) = issued
        && let Some(Value::Array(dates)) = member(issued, "date-parts", op)?
        && let Some(Value::Array(parts)) = dates.first().map(Rc::as_ref)
        && let Some(Value::Number(year)) = parts.first().map(Rc::as_ref)
        && year.fract() == 0.0
        && (1.0..=9999.0).contains(year)
    {
        (*year as u16).to_string()
    } else {
        "n.d.".to_owned()
    };
    let author = match names.as_slice() {
        [name] => (*name).to_owned(),
        [left, right] => format!("{left} & {right}"),
        [first, ..] => format!("{first} et al."),
        [] => return Ok(None),
    };
    copy_chunks(
        op,
        (key.len() + author.len() + byline.len() + year.len() + title.len()) as u64,
    )?;
    Ok(Some(Reference {
        key: key.to_owned(),
        author,
        byline,
        year,
        title: title.to_owned(),
    }))
}

fn bibliography(value: &Value, op: &mut Operation) -> Result<Vec<Reference>, CoreError> {
    let Some(Value::Array(records)) = member(value, "references", op)? else {
        return Ok(Vec::new());
    };
    if records.len() > MAX_REFERENCES {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    for value in records {
        if let Some(key) = field(member(value, "id", op)?, 128).filter(|key| valid_key(key)) {
            candidates.push((key, record(value, key, op)?));
        }
    }
    event_chunks(op, (candidates.len() * 8) as u64)?;
    candidates.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut references = Vec::new();
    let mut prior = None;
    let mut candidates = candidates.into_iter().peekable();
    while let Some((key, record)) = candidates.next() {
        op.events(1)?;
        if prior != Some(key)
            && candidates.peek().is_none_or(|(next, _)| *next != key)
            && let Some(record) = record
        {
            references.push(record);
        }
        prior = Some(key);
    }
    Ok(references)
}

fn parse_ast(content: &str, op: &mut Operation) -> ResultValue<Ast> {
    let yaml11 = content
        .lines()
        .take_while(|l| l.starts_with('%') || l.starts_with('#') || l.trim().is_empty())
        .any(|line| line.trim() == "%YAML 1.1");
    let mut ast = Ast {
        nodes: Vec::new(),
        anchors: HashMap::new(),
        root: None,
        yaml11,
    };
    let mut stack: Vec<Frame> = Vec::new();
    let mut aliases = 0usize;
    let mut documents = 0usize;
    // The char iterator is deliberately monitored inside the third-party
    // scanner, including a single long scalar/comment between two events.
    let operation = RefCell::new(op);
    let iterator_fault = Cell::new(None);
    let mut chars = content.chars();
    let mut consumed = 0u64;
    let iter = std::iter::from_fn(|| {
        if iterator_fault.get().is_some() {
            return None;
        }
        if consumed >= 1024 {
            let mut operation = operation.borrow_mut();
            if let Err(error) =
                scan_chunks(&mut operation, consumed).and_then(|()| operation.checkpoint_now())
            {
                iterator_fault.set(Some(error));
                return None;
            }
            consumed = 0;
        }
        let c = chars.next()?;
        consumed += c.len_utf8() as u64;
        // markdown-it normalize replaces literal NUL before the frontmatter rule.
        Some(if c == '\0' { '\u{fffd}' } else { c })
    });
    let mut parser = Parser::new_from_iter(iter);
    loop {
        operation.borrow_mut().checkpoint_now()?;
        let event = parser.next();
        if let Some(error) = iterator_fault.get() {
            return Err(error.into());
        }
        operation.borrow_mut().checkpoint_now()?;
        let Some(event) = event else {
            break;
        };
        operation.borrow_mut().events(1)?;
        let (event, span) = event.map_err(|error| Failure::Yaml(error.to_string()))?;
        let (kind, tag, anchor, source, style) = match event {
            Event::StreamStart | Event::StreamEnd | Event::Nothing => continue,
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err(Failure::Yaml(
                        "Source contains multiple documents; please use YAML.parseAllDocuments()"
                            .to_owned(),
                    ));
                }
                continue;
            }
            Event::DocumentEnd => continue,
            Event::SequenceEnd | Event::MappingEnd => {
                let frame = stack.pop().ok_or(CoreError::InternalInvariant)?;
                if frame.key.is_some() {
                    return Err(CoreError::InternalInvariant.into());
                }
                continue;
            }
            Event::Alias(id) => {
                aliases += 1;
                if aliases > MAX_ALIASES {
                    return Err(limited("100 alias"));
                }
                let source = alias_source(
                    content,
                    span.start.index(),
                    span.end.index(),
                    &mut operation.borrow_mut(),
                )?;
                (Kind::Alias(id), None, 0, source, ScalarStyle::Plain)
            }
            Event::Scalar(source, style, anchor, tag) => {
                let tag = tag.as_deref().map(tag_name);
                let value = scalar::resolve(&source, style, tag.as_deref(), yaml11)?;
                (
                    Kind::Scalar(Rc::new(value)),
                    tag,
                    anchor,
                    source.into_owned(),
                    style,
                )
            }
            Event::SequenceStart(anchor, tag) => (
                Kind::Sequence(Vec::new()),
                tag.as_deref().map(tag_name),
                anchor,
                String::new(),
                ScalarStyle::Plain,
            ),
            Event::MappingStart(anchor, tag) => (
                Kind::Mapping(Vec::new()),
                tag.as_deref().map(tag_name),
                anchor,
                String::new(),
                ScalarStyle::Plain,
            ),
        };
        if ast.nodes.len() >= MAX_NODES {
            return Err(limited("16,384 YAML node"));
        }
        let collection = matches!(kind, Kind::Sequence(_) | Kind::Mapping(_));
        if collection && stack.len() >= MAX_DEPTH {
            return Err(limited("64 level nesting"));
        }
        let id = ast.nodes.len();
        ast.nodes
            .try_reserve(1)
            .map_err(|_| CoreError::OutOfMemory)?;
        ast.nodes.push(Node {
            kind,
            tag,
            marker: span.start,
            source,
            style,
        });
        if anchor != 0 {
            ast.anchors.insert(anchor, id);
        }
        if let Some(frame) = stack.last_mut() {
            match &mut ast.nodes[frame.node].kind {
                Kind::Sequence(items) => {
                    items.try_reserve(1).map_err(|_| CoreError::OutOfMemory)?;
                    items.push(id);
                }
                Kind::Mapping(pairs) => {
                    if let Some(key) = frame.key.take() {
                        pairs.try_reserve(1).map_err(|_| CoreError::OutOfMemory)?;
                        pairs.push((key, id));
                    } else {
                        frame.key = Some(id);
                    }
                }
                _ => return Err(CoreError::InternalInvariant.into()),
            }
        } else if ast.root.replace(id).is_some() {
            return Err(CoreError::InternalInvariant.into());
        }
        if collection {
            stack.push(Frame {
                node: id,
                key: None,
            });
        }
    }
    drop(parser);
    scan_chunks(&mut operation.borrow_mut(), consumed)?;
    operation.borrow_mut().checkpoint_now()?;
    Ok(ast)
}

fn tag_name(tag: &Tag) -> String {
    format!("{}{}", tag.handle, tag.suffix)
}

struct Converter<'a> {
    ast: &'a Ast,
    content: &'a str,
    cache: Vec<Option<Rc<Value>>>,
    active: Vec<bool>,
    anchor_counts: Vec<usize>,
    anchor_weights: Vec<usize>,
    work: usize,
    merged_key_bytes: usize,
}
impl<'a> Converter<'a> {
    fn new(ast: &'a Ast, content: &'a str) -> Self {
        Self {
            ast,
            content,
            cache: vec![None; ast.nodes.len()],
            active: vec![false; ast.nodes.len()],
            anchor_counts: vec![1; ast.nodes.len()],
            anchor_weights: vec![0; ast.nodes.len()],
            work: 0,
            merged_key_bytes: 0,
        }
    }
    fn convert(&mut self, id: usize, depth: usize, op: &mut Operation) -> ResultValue<Rc<Value>> {
        op.events(1)?;
        self.work += 1;
        if self.work > MAX_NODES {
            return Err(limited("16,384 expanded node"));
        }
        if depth > MAX_DEPTH {
            return Err(limited("64 level expansion"));
        }
        if self.active[id] {
            return Err(Failure::Yaml(
                "Cyclic YAML aliases are not supported in frontmatter".to_owned(),
            ));
        }
        if let Some(value) = &self.cache[id] {
            return Ok(value.clone());
        }
        self.active[id] = true;
        let node = &self.ast.nodes[id];
        let value = match &node.kind {
            Kind::Scalar(value) => value.clone(),
            Kind::Alias(anchor) => {
                let target = *self
                    .ast
                    .anchors
                    .get(anchor)
                    .ok_or_else(|| Failure::Yaml("Unresolved YAML alias".to_owned()))?;
                let value = self.convert(target, depth + 1, op)?;
                self.anchor_counts[target] += 1;
                if self.anchor_weights[target] == 0 {
                    self.anchor_weights[target] = self.alias_weight(target, 0, op)?;
                }
                if self.anchor_counts[target].saturating_mul(self.anchor_weights[target]) > 100 {
                    return Err(Failure::Yaml(
                        "Excessive alias count indicates a resource exhaustion attack".to_owned(),
                    ));
                }
                value
            }
            Kind::Sequence(items) => {
                let tag = node.tag.as_deref();
                if matches!(
                    tag,
                    Some("tag:yaml.org,2002:omap" | "tag:yaml.org,2002:pairs")
                ) {
                    let mut pairs: Vec<(Rc<Value>, Rc<Value>)> = Vec::with_capacity(items.len());
                    let mut keys = HashSet::with_capacity(items.len());
                    for item in items {
                        let (key_id, key, value) = match &self.ast.nodes[*item].kind {
                            Kind::Mapping(map) => {
                                if map.len() > 1 {
                                    return Err(Failure::Yaml(
                                        "Each pair must have its own sequence indicator".to_owned(),
                                    ));
                                }
                                if let Some(&(key, value)) = map.first() {
                                    (
                                        key,
                                        self.convert(key, depth + 1, op)?,
                                        self.convert(value, depth + 1, op)?,
                                    )
                                } else {
                                    (*item, Rc::new(Value::Null), Rc::new(Value::Null))
                                }
                            }
                            _ => (
                                *item,
                                self.convert(*item, depth + 1, op)?,
                                Rc::new(Value::Null),
                            ),
                        };
                        if tag == Some("tag:yaml.org,2002:omap") {
                            if !keys.insert(map_identity(&key)) {
                                return Err(Failure::Yaml(
                                    "Ordered maps must not include duplicate keys".to_owned(),
                                ));
                            }
                            pairs.push((key, value));
                        } else {
                            let key = self.property_key(key_id, &key, op)?;
                            pairs.push((Rc::new(Value::Text(key)), value));
                        }
                    }
                    if tag == Some("tag:yaml.org,2002:omap") {
                        Rc::new(Value::OrderedMap(pairs))
                    } else {
                        Rc::new(Value::Array(
                            pairs
                                .into_iter()
                                .map(|(key, value)| {
                                    let Value::Text(key) = key.as_ref() else {
                                        unreachable!()
                                    };
                                    Rc::new(Value::Object(vec![(key.clone(), value)]))
                                })
                                .collect(),
                        ))
                    }
                } else {
                    let mut values = Vec::with_capacity(items.len());
                    for item in items {
                        values.push(self.convert(*item, depth + 1, op)?);
                    }
                    Rc::new(Value::Array(values))
                }
            }
            Kind::Mapping(pairs) => {
                self.check_duplicates(pairs, op)?;
                if node.tag.as_deref() == Some("tag:yaml.org,2002:set") {
                    let mut values: Vec<Rc<Value>> = Vec::with_capacity(pairs.len());
                    let mut keys = HashSet::with_capacity(pairs.len());
                    for &(key, value) in pairs {
                        if !matches!(&self.ast.nodes[value].kind, Kind::Scalar(v) if matches!(v.as_ref(), Value::Null))
                        {
                            return Err(Failure::Yaml(
                                "Set items must all have null values".to_owned(),
                            ));
                        }
                        let key = self.convert(key, depth + 1, op)?;
                        if keys.insert(map_identity(&key)) {
                            values.push(key);
                        }
                    }
                    Rc::new(Value::Set(values))
                } else {
                    let mut values = Vec::with_capacity(pairs.len());
                    let mut indices = HashMap::with_capacity(pairs.len());
                    for &(key_id, value_id) in pairs {
                        if self.merge_key(key_id) {
                            let source = self.convert(value_id, depth + 1, op)?;
                            let sources = match source.as_ref() {
                                Value::Array(items) => items.as_slice(),
                                _ => std::slice::from_ref(&source),
                            };
                            for source in sources {
                                let Value::Object(source) = source.as_ref() else {
                                    return Err(Failure::Yaml(
                                        "Merge sources must be maps or map aliases".to_owned(),
                                    ));
                                };
                                for (key, value) in source {
                                    op.events(1)?;
                                    self.work += 1;
                                    if self.work > MAX_NODES {
                                        return Err(limited("16,384 expanded node"));
                                    }
                                    if !indices.contains_key(key) {
                                        // Both the lookup index and the retained
                                        // merged map own a key. Admit their full
                                        // expansion before either clone exists.
                                        self.merged_key_bytes = self
                                            .merged_key_bytes
                                            .saturating_add(key.len().saturating_mul(2));
                                        if self.merged_key_bytes > MAX_OUTPUT {
                                            return Err(limited("1 MiB merged key"));
                                        }
                                        indices
                                            .try_reserve(1)
                                            .map_err(|_| CoreError::OutOfMemory)?;
                                        values
                                            .try_reserve(1)
                                            .map_err(|_| CoreError::OutOfMemory)?;
                                        let mut index_key = String::new();
                                        let mut value_key = String::new();
                                        index_key
                                            .try_reserve_exact(key.len())
                                            .map_err(|_| CoreError::OutOfMemory)?;
                                        value_key
                                            .try_reserve_exact(key.len())
                                            .map_err(|_| CoreError::OutOfMemory)?;
                                        copy_chunks(op, key.len() as u64 * 2)?;
                                        index_key.push_str(key);
                                        value_key.push_str(key);
                                        indices.insert(index_key, values.len());
                                        values.push((value_key, value.clone()));
                                    }
                                }
                            }
                            continue;
                        }
                        let key = self.convert(key_id, depth + 1, op)?;
                        let key = self.property_key(key_id, &key, op)?;
                        let value = self.convert(value_id, depth + 1, op)?;
                        if let Some(&index) = indices.get(&key) {
                            values[index] = (key, value);
                        } else {
                            indices.insert(key.clone(), values.len());
                            values.push((key, value));
                        }
                    }
                    // JS OwnPropertyKeys puts canonical array-index strings first.
                    // sort_by_key is stable, preserving other insertion order.
                    values.sort_by_key(|(key, _)| array_index(key).map_or((1, 0), |i| (0, i)));
                    Rc::new(Value::Object(values))
                }
            }
        };
        self.active[id] = false;
        self.cache[id] = Some(value.clone());
        Ok(value)
    }

    fn check_duplicates(&self, pairs: &[(usize, usize)], op: &mut Operation) -> ResultValue<()> {
        // yaml's uniqueKeys checks direct scalar values with JS strict equality,
        // not scalar syntax or collection representation. Distinct collections
        // and distinct alias nodes remain distinct at this pre-coercion stage.
        let mut seen = HashMap::<String, ()>::with_capacity(pairs.len());
        for &(key, _) in pairs {
            op.events(1)?;
            if self.merge_key(key) {
                continue;
            }
            let Kind::Scalar(value) = &self.ast.nodes[key].kind else {
                continue;
            };
            let identity = match value.as_ref() {
                Value::Null => Some("n".to_owned()),
                Value::Bool(b) => Some(format!("b{b}")),
                Value::Number(n) if !n.is_nan() => Some(format!("d{}", scalar::number(*n))),
                Value::Text(s) => Some(format!("s{s}")),
                _ => None,
            };
            if let Some(identity) = identity
                && seen.insert(identity, ()).is_some()
            {
                return Err(Failure::Yaml(located_error(
                    "Map keys must be unique",
                    self.ast.nodes[key].marker,
                    self.content,
                )));
            }
        }
        Ok(())
    }
    fn merge_key(&self, id: usize) -> bool {
        let node = &self.ast.nodes[id];
        node.tag.as_deref() == Some("tag:yaml.org,2002:merge")
            || self.ast.yaml11
                && node.style == ScalarStyle::Plain
                && matches!(&node.kind, Kind::Scalar(value) if matches!(value.as_ref(), Value::Text(s) if s == "<<"))
    }
    fn alias_weight(&self, id: usize, depth: usize, op: &mut Operation) -> ResultValue<usize> {
        op.events(1)?;
        if depth > MAX_DEPTH {
            return Err(limited("64 level alias"));
        }
        match &self.ast.nodes[id].kind {
            Kind::Scalar(_) => Ok(1),
            Kind::Alias(anchor) => {
                let target = *self
                    .ast
                    .anchors
                    .get(anchor)
                    .ok_or(CoreError::InternalInvariant)?;
                Ok(self.anchor_counts[target].saturating_mul(self.anchor_weights[target]))
            }
            Kind::Sequence(items) => {
                let mut max = 0;
                for item in items {
                    max = max.max(self.alias_weight(*item, depth + 1, op)?);
                }
                Ok(max)
            }
            Kind::Mapping(pairs) => {
                let mut max = 0;
                for &(key, value) in pairs {
                    max = max
                        .max(self.alias_weight(key, depth + 1, op)?)
                        .max(self.alias_weight(value, depth + 1, op)?);
                }
                Ok(max)
            }
        }
    }
    fn property_key(&self, id: usize, value: &Value, op: &mut Operation) -> ResultValue<String> {
        match value {
            Value::Null => Ok(String::new()),
            Value::Bool(value) => Ok(value.to_string()),
            Value::Number(value) => Ok(scalar::number(*value)),
            Value::Text(value) => Ok(value.clone()),
            _ => self.flow_key(id, 0, op),
        }
    }
    fn flow_key(&self, id: usize, depth: usize, op: &mut Operation) -> ResultValue<String> {
        op.events(1)?;
        if depth > MAX_DEPTH {
            return Err(limited("64 level complex key"));
        }
        let node = &self.ast.nodes[id];
        let output = match &node.kind {
            Kind::Scalar(value) => {
                let text = match value.as_ref() {
                    Value::Text(text) => stringify::string(text, true, node.style),
                    Value::Number(number) => scalar::key_number(
                        *number,
                        &node.source,
                        node.tag.as_deref(),
                        self.ast.yaml11,
                    ),
                    _ => stringify::scalar(value),
                };
                match node.tag.as_deref() {
                    Some(tag) => format!(
                        "{} {text}",
                        tag.strip_prefix("tag:yaml.org,2002:")
                            .map_or_else(|| tag.to_owned(), |s| format!("!!{s}"))
                    ),
                    None => text,
                }
            }
            Kind::Alias(_) => node.source.clone(),
            Kind::Sequence(items) => {
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    parts.push(self.flow_key(*item, depth + 1, op)?);
                }
                if parts.is_empty() {
                    "[]".to_owned()
                } else {
                    format!("[ {} ]", parts.join(", "))
                }
            }
            Kind::Mapping(pairs) => {
                let mut parts = Vec::with_capacity(pairs.len());
                for &(key, value) in pairs {
                    parts.push(format!(
                        "{}: {}",
                        self.flow_key(key, depth + 1, op)?,
                        self.flow_key(value, depth + 1, op)?
                    ));
                }
                if parts.is_empty() {
                    "{}".to_owned()
                } else {
                    format!("{{ {} }}", parts.join(", "))
                }
            }
        };
        if output.len() > MAX_OUTPUT {
            return Err(limited("1 MiB complex key"));
        }
        Ok(output)
    }
}

fn map_identity(value: &Rc<Value>) -> String {
    match value.as_ref() {
        Value::Null => "n".to_owned(),
        Value::Bool(value) => format!("b{value}"),
        Value::Number(value) => format!("d{}", scalar::number(*value)),
        Value::Text(value) => format!("s{value}"),
        _ => format!("o{:p}", Rc::as_ptr(value)),
    }
}

fn expanded_nodes(
    value: &Rc<Value>,
    depth: usize,
    count: &mut usize,
    op: &mut Operation,
) -> ResultValue<()> {
    op.events(1)?;
    *count += 1;
    if *count > MAX_NODES {
        return Err(limited("16,384 expanded node"));
    }
    if depth > MAX_DEPTH {
        return Err(limited("64 level expansion"));
    }
    match value.as_ref() {
        Value::Array(values) | Value::Set(values) => {
            for value in values {
                expanded_nodes(value, depth + 1, count, op)?;
            }
        }
        Value::Object(values) => {
            for (_, value) in values {
                expanded_nodes(value, depth + 1, count, op)?;
            }
        }
        Value::OrderedMap(values) => {
            for (key, value) in values {
                expanded_nodes(key, depth + 1, count, op)?;
                expanded_nodes(value, depth + 1, count, op)?;
            }
        }
        Value::Binary(bytes) => {
            event_chunks(op, bytes.len() as u64)?;
            *count = count.saturating_add(bytes.len());
            if *count > MAX_NODES {
                return Err(limited("16,384 expanded node"));
            }
        }
        _ => {}
    }
    Ok(())
}
fn array_index(key: &str) -> Option<u32> {
    match key.as_bytes() {
        [b'0'] | [b'1'..=b'9', ..] => key.parse().ok().filter(|&index| index != u32::MAX),
        _ => None,
    }
}
fn located_error(message: &str, marker: Marker, content: &str) -> String {
    let line = marker.line();
    let column = marker.col() + 1;
    let lines: Vec<&str> = content.lines().collect();
    let mut context = String::new();
    if line >= 2
        && let Some(previous) = lines.get(line - 2)
    {
        context.push_str(previous);
        context.push('\n');
    }
    context.push_str(lines.get(line.saturating_sub(1)).copied().unwrap_or(""));
    context.push('\n');
    context.push_str(&" ".repeat(marker.col()));
    context.push_str("^\n");
    format!("{message} at line {line}, column {column}:\n\n{context}")
}

struct DisplayBudget {
    nodes: usize,
    bytes: usize,
}
impl DisplayBudget {
    fn charge(&mut self, bytes: usize) -> ResultValue<()> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > MAX_OUTPUT {
            return Err(limited("1 MiB generated output"));
        }
        Ok(())
    }
    fn format(
        &mut self,
        value: &Rc<Value>,
        depth: usize,
        op: &mut Operation,
    ) -> ResultValue<DisplayValue> {
        op.events(1)?;
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(limited("16,384 displayed node"));
        }
        if depth > MAX_DEPTH {
            return Err(limited("64 level display"));
        }
        match value.as_ref() {
            Value::Array(items) => {
                let mut output = Vec::with_capacity(items.len());
                for item in items {
                    output.push(self.format(item, depth + 1, op)?);
                }
                Ok(DisplayValue::List(output))
            }
            Value::Null => Ok(DisplayValue::Text(String::new())),
            Value::Object(_)
            | Value::Set(_)
            | Value::OrderedMap(_)
            | Value::Date(_)
            | Value::Binary(_) => {
                let output = stringify::value(value, MAX_OUTPUT - self.bytes, op)?;
                self.charge(output.len())?;
                Ok(DisplayValue::Code(output))
            }
            _ => {
                let output = match value.as_ref() {
                    Value::Text(value) => value.clone(),
                    Value::Bool(value) => value.to_string(),
                    Value::Number(value) => scalar::number(*value),
                    _ => unreachable!(),
                };
                self.charge(output.len())?;
                Ok(DisplayValue::Text(output))
            }
        }
    }
}
