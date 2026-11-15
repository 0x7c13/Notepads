use crate::{OperationControl, OperationControlResult, preprocess::SourceConfigPath};
use granit_parser::{Event, Parser, ScalarStyle, Span, Tag};
use serde_json::{Map, Number, Value};
use std::collections::{HashMap, HashSet};
use std::mem::size_of;

const YAML_MATERIALIZATION_MIN_BYTES: usize = 64 * 1024;
const YAML_MATERIALIZATION_INPUT_MULTIPLIER: usize = 16;
const YAML_PARSER_MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

#[cfg(test)]
pub(crate) fn parse_yaml_value(input: &str, max_nesting_depth: usize) -> Result<Value, String> {
    let control = OperationControl::new();
    parse_yaml_value_controlled(input, max_nesting_depth, &control)
        .expect("a private parse control cannot be cancelled")
}

pub(crate) fn parse_yaml_value_controlled(
    input: &str,
    max_nesting_depth: usize,
    control: &OperationControl,
) -> OperationControlResult<Result<Value, String>> {
    let materialization_budget = input
        .len()
        .saturating_mul(YAML_MATERIALIZATION_INPUT_MULTIPLIER)
        .max(YAML_MATERIALIZATION_MIN_BYTES);
    parse_yaml_value_with_limits_controlled(
        input,
        YAML_PARSER_MAX_INPUT_BYTES,
        max_nesting_depth,
        materialization_budget,
        control,
    )
}

#[derive(Debug)]
pub(crate) struct YamlConfigKeyEvidence {
    pub(crate) path: SourceConfigPath,
    pub(crate) span: Option<std::ops::Range<usize>>,
    pub(crate) rewrite_safe: bool,
}

pub(crate) struct YamlValueCapture {
    pub(crate) value: Result<Value, String>,
    pub(crate) keys: Vec<YamlConfigKeyEvidence>,
    pub(crate) rewrite_safe: bool,
}

pub(crate) fn parse_yaml_value_capture_controlled(
    input: &str,
    max_nesting_depth: usize,
    control: &OperationControl,
) -> OperationControlResult<YamlValueCapture> {
    let materialization_budget = input
        .len()
        .saturating_mul(YAML_MATERIALIZATION_INPUT_MULTIPLIER)
        .max(YAML_MATERIALIZATION_MIN_BYTES);
    parse_yaml_value_with_limits_capture_controlled(
        input,
        YAML_PARSER_MAX_INPUT_BYTES,
        max_nesting_depth,
        materialization_budget,
        control,
    )
}

pub(crate) fn parse_yaml_value_with_limits_controlled(
    input: &str,
    max_input_bytes: usize,
    max_nesting_depth: usize,
    materialization_budget: usize,
    control: &OperationControl,
) -> OperationControlResult<Result<Value, String>> {
    Ok(parse_yaml_value_with_limits_capture_mode_controlled(
        input,
        max_input_bytes,
        max_nesting_depth,
        materialization_budget,
        false,
        control,
    )?
    .value)
}

fn parse_yaml_value_with_limits_capture_controlled(
    input: &str,
    max_input_bytes: usize,
    max_nesting_depth: usize,
    materialization_budget: usize,
    control: &OperationControl,
) -> OperationControlResult<YamlValueCapture> {
    parse_yaml_value_with_limits_capture_mode_controlled(
        input,
        max_input_bytes,
        max_nesting_depth,
        materialization_budget,
        true,
        control,
    )
}

fn parse_yaml_value_with_limits_capture_mode_controlled(
    input: &str,
    max_input_bytes: usize,
    max_nesting_depth: usize,
    materialization_budget: usize,
    capture_keys: bool,
    control: &OperationControl,
) -> OperationControlResult<YamlValueCapture> {
    control.checkpoint()?;
    if input.len() > max_input_bytes {
        return Ok(YamlValueCapture {
            value: Err(format!(
                "YAML input exceeds the safe parser budget of {max_input_bytes} bytes"
            )),
            keys: Vec::new(),
            rewrite_safe: false,
        });
    }
    let mut builder =
        YamlValueBuilder::new(max_nesting_depth, materialization_budget, capture_keys);

    for (index, event) in Parser::new_from_str(input).enumerate() {
        if index % 64 == 0 {
            control.checkpoint()?;
        }
        let (event, span) = match event {
            Ok(event) => event,
            Err(error) => {
                let (keys, _) = builder.take_capture();
                return Ok(YamlValueCapture {
                    value: Err(error.to_string()),
                    keys,
                    rewrite_safe: false,
                });
            }
        };
        if let Err(error) = builder.on_event(input, event, span) {
            let (keys, _) = builder.take_capture();
            return Ok(YamlValueCapture {
                value: Err(error),
                keys,
                rewrite_safe: false,
            });
        }
    }

    control.checkpoint()?;
    let (keys, rewrite_safe) = builder.take_capture();
    let value = builder.finish_controlled(control)?;
    Ok(YamlValueCapture {
        value,
        keys,
        rewrite_safe,
    })
}

fn yaml_key_span(input: &str, span: Span, style: ScalarStyle) -> Option<std::ops::Range<usize>> {
    let mut range = span.byte_range()?;
    if matches!(style, ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted) {
        let source = input.get(range.clone())?;
        let quote = match style {
            ScalarStyle::SingleQuoted => '\'',
            ScalarStyle::DoubleQuoted => '"',
            ScalarStyle::Plain | ScalarStyle::Literal | ScalarStyle::Folded => unreachable!(),
        };
        if !source.starts_with(quote) || !source.ends_with(quote) || source.len() < 2 {
            return None;
        }
        range.start = range.start.saturating_add(quote.len_utf8());
        range.end = range.end.saturating_sub(quote.len_utf8());
    }
    input.get(range.clone()).map(|_| range)
}

fn yaml_key_span_is_rewrite_safe(
    input: &str,
    span: Span,
    style: ScalarStyle,
    decoded: &str,
) -> bool {
    yaml_key_span(input, span, style)
        .and_then(|range| input.get(range))
        .is_some_and(|source| source == decoded)
}

struct YamlValueBuilder {
    stack: Vec<Frame>,
    arena: Vec<YamlNode>,
    root: Option<NodeId>,
    anchors: HashMap<usize, NodeId>,
    max_nesting_depth: usize,
    materialization_budget: usize,
    capture_keys: bool,
    key_evidence: Vec<YamlConfigKeyEvidence>,
    rewrite_safe: bool,
}

impl YamlValueBuilder {
    fn new(max_nesting_depth: usize, materialization_budget: usize, capture_keys: bool) -> Self {
        Self {
            stack: Vec::new(),
            arena: Vec::new(),
            root: None,
            anchors: HashMap::new(),
            max_nesting_depth,
            materialization_budget,
            capture_keys,
            key_evidence: Vec::new(),
            rewrite_safe: true,
        }
    }

    fn on_event(&mut self, input: &str, event: Event<'_>, span: Span) -> Result<(), String> {
        match event {
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart(..)
            | Event::DocumentEnd => Ok(()),
            Event::Comment(_, _) => {
                self.rewrite_safe = false;
                Ok(())
            }
            Event::Alias(anchor_id) => {
                self.rewrite_safe = false;
                let role = self.reserve_role()?;
                let node = self
                    .anchors
                    .get(&anchor_id)
                    .copied()
                    .ok_or_else(|| "unsupported forward YAML alias".to_string())?;
                self.complete_node(node, role, 0, false)
            }
            Event::Scalar(raw, style, anchor_id, tag) => {
                if anchor_id != 0
                    || tag.is_some()
                    || matches!(style, ScalarStyle::Literal | ScalarStyle::Folded)
                {
                    self.rewrite_safe = false;
                }
                let role = self.reserve_role()?;
                let value = scalar_to_value(raw.as_ref(), style, tag.as_deref())?;
                let key_source = self.capture_keys.then(|| YamlKeySource {
                    span: yaml_key_span(input, span, style),
                    rewrite_safe: yaml_key_span_is_rewrite_safe(input, span, style, raw.as_ref()),
                });
                let node = self.push_node(YamlNode::Scalar { value, key_source });
                self.complete_node(node, role, anchor_id, true)
            }
            Event::SequenceStart(_, anchor_id, tag) => {
                if anchor_id != 0 || tag.is_some() {
                    self.rewrite_safe = false;
                }
                let role = self.reserve_role()?;
                let path = self.path_for_role(&role);
                self.push_frame(Frame {
                    container: Container::Sequence(Vec::new()),
                    role,
                    anchor_id,
                    path,
                })
            }
            Event::MappingStart(_, anchor_id, tag) => {
                if anchor_id != 0 || tag.is_some() {
                    self.rewrite_safe = false;
                }
                let role = self.reserve_role()?;
                let path = self.path_for_role(&role);
                self.push_frame(Frame {
                    container: Container::Mapping {
                        entries: Vec::new(),
                        keys: HashSet::new(),
                        pending_key: None,
                    },
                    role,
                    anchor_id,
                    path,
                })
            }
            end_event @ (Event::SequenceEnd | Event::MappingEnd) => {
                let frame = self
                    .stack
                    .pop()
                    .ok_or_else(|| "unexpected YAML collection end".to_string())?;
                let node = match (end_event, frame.container) {
                    (Event::SequenceEnd, Container::Sequence(items)) => {
                        self.push_node(YamlNode::Sequence(items))
                    }
                    (
                        Event::MappingEnd,
                        Container::Mapping {
                            entries,
                            keys: _,
                            pending_key: _,
                        },
                    ) => self.push_node(YamlNode::Mapping(entries)),
                    (Event::SequenceEnd, _) | (Event::MappingEnd, _) => {
                        return Err("mismatched YAML collection end".to_string());
                    }
                    _ => unreachable!(),
                };
                self.complete_node(node, frame.role, frame.anchor_id, true)
            }
            _ => Err("unsupported YAML parser event".to_string()),
        }
    }

    fn take_capture(&mut self) -> (Vec<YamlConfigKeyEvidence>, bool) {
        (std::mem::take(&mut self.key_evidence), self.rewrite_safe)
    }

    fn path_for_role(&self, role: &Role) -> Option<SourceConfigPath> {
        if !self.capture_keys {
            return None;
        }
        match role {
            Role::MappingValue(MappingKey::String { path, .. }) => path.clone(),
            Role::Root => Some(SourceConfigPath::root()),
            // Collection-valued mapping keys are discarded by materialization. Their descendants
            // must not publish paths as if they belonged to the surrounding object.
            Role::MappingKey => None,
            Role::SequenceItem | Role::MappingValue(MappingKey::Ignored) => None,
        }
    }

    fn push_node(&mut self, node: YamlNode) -> NodeId {
        let id = NodeId(self.arena.len());
        self.arena.push(node);
        id
    }

    fn push_frame(&mut self, frame: Frame) -> Result<(), String> {
        if self.stack.len() >= self.max_nesting_depth {
            return Err(format!(
                "config nesting exceeds {} levels",
                self.max_nesting_depth
            ));
        }
        self.stack.push(frame);
        Ok(())
    }

    fn reserve_role(&mut self) -> Result<Role, String> {
        let Some(parent) = self.stack.last_mut() else {
            if self.root.is_some() {
                return Err("multiple YAML documents are not supported".to_string());
            }
            return Ok(Role::Root);
        };

        match &mut parent.container {
            Container::Sequence(_) => Ok(Role::SequenceItem),
            Container::Mapping { pending_key, .. } => match pending_key.take() {
                Some(key) => Ok(Role::MappingValue(key)),
                None => Ok(Role::MappingKey),
            },
        }
    }

    fn complete_node(
        &mut self,
        node: NodeId,
        role: Role,
        anchor_id: usize,
        source_addressable: bool,
    ) -> Result<(), String> {
        if anchor_id != 0 {
            self.anchors.insert(anchor_id, node);
        }

        match role {
            Role::Root => {
                self.root = Some(node);
                Ok(())
            }
            Role::SequenceItem => {
                let Some(Frame {
                    container: Container::Sequence(items),
                    ..
                }) = self.stack.last_mut()
                else {
                    return Err("YAML sequence item had no parent sequence".to_string());
                };
                items.push(node);
                Ok(())
            }
            Role::MappingKey => {
                let mut key = node_to_mapping_key(&self.arena, node);
                if self.capture_keys {
                    let parent_path = self.stack.last().and_then(|frame| frame.path.clone());
                    match (&mut key, source_addressable, parent_path) {
                        (
                            MappingKey::String {
                                name,
                                span,
                                rewrite_safe,
                                path,
                            },
                            true,
                            Some(parent_path),
                        ) => {
                            let key_path = parent_path.child(name.clone());
                            self.key_evidence.push(YamlConfigKeyEvidence {
                                path: key_path.clone(),
                                span: span.clone(),
                                rewrite_safe: *rewrite_safe,
                            });
                            *path = Some(key_path);
                            if name.as_str() == "<<" || !*rewrite_safe {
                                self.rewrite_safe = false;
                            }
                        }
                        (MappingKey::Ignored, true, _) | (_, false, _) => {
                            self.rewrite_safe = false;
                        }
                        (MappingKey::String { .. }, true, None) => {}
                    }
                }
                let Some(Frame {
                    container: Container::Mapping { pending_key, .. },
                    ..
                }) = self.stack.last_mut()
                else {
                    return Err("YAML mapping key had no parent mapping".to_string());
                };
                *pending_key = Some(key);
                Ok(())
            }
            Role::MappingValue(key) => {
                let Some(Frame {
                    container: Container::Mapping { entries, keys, .. },
                    ..
                }) = self.stack.last_mut()
                else {
                    return Err("YAML mapping value had no parent mapping".to_string());
                };
                match key {
                    MappingKey::String { name, .. } => {
                        if !keys.insert(name.clone()) {
                            return Err("duplicated mapping key".to_string());
                        }
                        entries.push((name, node));
                    }
                    MappingKey::Ignored => {}
                }
                Ok(())
            }
        }
    }

    fn finish_controlled(
        self,
        control: &OperationControl,
    ) -> OperationControlResult<Result<Value, String>> {
        if !self.stack.is_empty() {
            return Ok(Err("incomplete YAML document".to_string()));
        }

        let Some(root) = self.root else {
            return Ok(Ok(Value::Null));
        };
        materialize_yaml_controlled(
            &self.arena,
            root,
            self.max_nesting_depth,
            self.materialization_budget,
            control,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeId(usize);

enum YamlNode {
    Scalar {
        value: Value,
        key_source: Option<YamlKeySource>,
    },
    Sequence(Vec<NodeId>),
    Mapping(Vec<(String, NodeId)>),
}

struct Frame {
    container: Container,
    role: Role,
    anchor_id: usize,
    path: Option<SourceConfigPath>,
}

struct YamlKeySource {
    span: Option<std::ops::Range<usize>>,
    rewrite_safe: bool,
}

enum Container {
    Sequence(Vec<NodeId>),
    Mapping {
        entries: Vec<(String, NodeId)>,
        keys: HashSet<String>,
        pending_key: Option<MappingKey>,
    },
}

enum Role {
    Root,
    SequenceItem,
    MappingKey,
    MappingValue(MappingKey),
}

enum MappingKey {
    String {
        name: String,
        span: Option<std::ops::Range<usize>>,
        rewrite_safe: bool,
        path: Option<SourceConfigPath>,
    },
    Ignored,
}

fn node_to_mapping_key(arena: &[YamlNode], node: NodeId) -> MappingKey {
    match &arena[node.0] {
        YamlNode::Scalar {
            value: Value::String(key),
            key_source,
        } => MappingKey::String {
            name: key.clone(),
            span: key_source.as_ref().and_then(|source| source.span.clone()),
            rewrite_safe: key_source.as_ref().is_none_or(|source| source.rewrite_safe),
            path: None,
        },
        YamlNode::Scalar {
            value: Value::Number(key),
            key_source,
        } => MappingKey::String {
            name: key.to_string(),
            span: key_source.as_ref().and_then(|source| source.span.clone()),
            rewrite_safe: false,
            path: None,
        },
        YamlNode::Scalar {
            value: Value::Bool(true),
            key_source,
        } => MappingKey::String {
            name: "true".to_string(),
            span: key_source.as_ref().and_then(|source| source.span.clone()),
            rewrite_safe: false,
            path: None,
        },
        YamlNode::Scalar {
            value: Value::Bool(false),
            key_source,
        } => MappingKey::String {
            name: "false".to_string(),
            span: key_source.as_ref().and_then(|source| source.span.clone()),
            rewrite_safe: false,
            path: None,
        },
        YamlNode::Scalar {
            value: Value::Null,
            key_source,
        } => MappingKey::String {
            name: "null".to_string(),
            span: key_source.as_ref().and_then(|source| source.span.clone()),
            rewrite_safe: false,
            path: None,
        },
        YamlNode::Scalar {
            value: Value::Array(_) | Value::Object(_),
            ..
        }
        | YamlNode::Sequence(_)
        | YamlNode::Mapping(_) => MappingKey::Ignored,
    }
}

enum MaterializeTask {
    Visit { node: NodeId, depth: usize },
    CompleteSequence { len: usize },
    CompleteMapping { node: NodeId, len: usize },
}

fn materialize_yaml_controlled(
    arena: &[YamlNode],
    root: NodeId,
    max_nesting_depth: usize,
    materialization_budget: usize,
    control: &OperationControl,
) -> OperationControlResult<Result<Value, String>> {
    control.checkpoint()?;
    let mut tasks = vec![MaterializeTask::Visit {
        node: root,
        depth: 0,
    }];
    let mut values = Vec::new();
    let mut materialized_bytes = 0usize;
    let mut visited = 0usize;

    while let Some(task) = tasks.pop() {
        if visited.is_multiple_of(64)
            && let Err(cancelled) = control.checkpoint()
        {
            drop_materialized_values(values);
            return Err(cancelled);
        }
        visited = visited.saturating_add(1);
        match task {
            MaterializeTask::Visit { node, depth } => {
                if depth > max_nesting_depth {
                    drop_materialized_values(values);
                    return Ok(Err(format!(
                        "config nesting exceeds {max_nesting_depth} levels after resolving YAML aliases"
                    )));
                }

                let yaml_node = &arena[node.0];
                materialized_bytes =
                    materialized_bytes.saturating_add(yaml_node_materialized_cost(yaml_node));
                if materialized_bytes > materialization_budget {
                    drop_materialized_values(values);
                    return Ok(Err(
                        "YAML aliases expand beyond the safe materialization budget".to_string(),
                    ));
                }

                match yaml_node {
                    YamlNode::Scalar { value, .. } => {
                        values.push(crate::config::clone_value_nonrecursive(value));
                    }
                    YamlNode::Sequence(items) => {
                        tasks.push(MaterializeTask::CompleteSequence { len: items.len() });
                        for (index, child) in items.iter().rev().enumerate() {
                            if index % 64 == 0
                                && let Err(cancelled) = control.checkpoint()
                            {
                                drop_materialized_values(values);
                                return Err(cancelled);
                            }
                            tasks.push(MaterializeTask::Visit {
                                node: *child,
                                depth: depth.saturating_add(1),
                            });
                        }
                    }
                    YamlNode::Mapping(entries) => {
                        tasks.push(MaterializeTask::CompleteMapping {
                            node,
                            len: entries.len(),
                        });
                        for (index, (_, child)) in entries.iter().rev().enumerate() {
                            if index % 64 == 0
                                && let Err(cancelled) = control.checkpoint()
                            {
                                drop_materialized_values(values);
                                return Err(cancelled);
                            }
                            tasks.push(MaterializeTask::Visit {
                                node: *child,
                                depth: depth.saturating_add(1),
                            });
                        }
                    }
                }
            }
            MaterializeTask::CompleteSequence { len } => {
                let start = values.len().saturating_sub(len);
                let items = values.split_off(start);
                values.push(Value::Array(items));
            }
            MaterializeTask::CompleteMapping { node, len } => {
                let start = values.len().saturating_sub(len);
                let children = values.split_off(start);
                let YamlNode::Mapping(entries) = &arena[node.0] else {
                    drop_materialized_values(values);
                    drop_materialized_values(children);
                    return Ok(Err("invalid YAML materialization state".to_string()));
                };
                let mut map = Map::with_capacity(len);
                let mut children = children.into_iter();
                for (index, (key, _)) in entries.iter().enumerate() {
                    if index % 64 == 0
                        && let Err(cancelled) = control.checkpoint()
                    {
                        drop_materialized_values(values);
                        drop_materialized_values(children.collect());
                        crate::config::drop_value_nonrecursive(Value::Object(map));
                        return Err(cancelled);
                    }
                    let Some(value) = children.next() else {
                        drop_materialized_values(values);
                        drop_materialized_values(children.collect());
                        crate::config::drop_value_nonrecursive(Value::Object(map));
                        return Ok(Err("invalid YAML materialization state".to_string()));
                    };
                    map.insert(key.clone(), value);
                }
                values.push(Value::Object(map));
            }
        }
    }

    if values.len() != 1 {
        drop_materialized_values(values);
        return Ok(Err("invalid YAML materialization state".to_string()));
    }
    if let Err(cancelled) = control.checkpoint() {
        drop_materialized_values(values);
        return Err(cancelled);
    }
    Ok(Ok(values.pop().unwrap_or(Value::Null)))
}

fn yaml_node_materialized_cost(node: &YamlNode) -> usize {
    match node {
        YamlNode::Scalar {
            value: Value::String(value),
            ..
        } => size_of::<Value>().saturating_add(value.len()),
        YamlNode::Scalar {
            value:
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_),
            ..
        } => size_of::<Value>(),
        YamlNode::Sequence(items) => size_of::<Value>()
            .saturating_add(size_of::<Vec<Value>>())
            .saturating_add(items.len().saturating_mul(size_of::<Value>())),
        YamlNode::Mapping(entries) => entries.iter().fold(
            size_of::<Value>()
                .saturating_add(size_of::<Map<String, Value>>())
                .saturating_add(
                    entries.len().saturating_mul(
                        size_of::<String>()
                            .saturating_add(size_of::<Value>())
                            .saturating_add(size_of::<usize>().saturating_mul(4)),
                    ),
                ),
            |total, (key, _)| total.saturating_add(key.len()),
        ),
    }
}

fn drop_materialized_values(values: Vec<Value>) {
    for value in values {
        crate::config::drop_value_nonrecursive(value);
    }
}

fn scalar_to_value(raw: &str, style: ScalarStyle, tag: Option<&Tag>) -> Result<Value, String> {
    if let Some(core_suffix) = tag.and_then(Tag::core_suffix) {
        return scalar_to_tagged_value(raw, core_suffix);
    }

    if style != ScalarStyle::Plain {
        return Ok(Value::String(raw.to_string()));
    }

    if is_yaml_null(raw) {
        return Ok(Value::Null);
    }
    if let Some(value) = parse_yaml_bool(raw) {
        return Ok(Value::Bool(value));
    }
    if let Some(number) = parse_yaml_int(raw) {
        return Ok(Value::Number(number));
    }
    if let Some(number) = parse_yaml_float(raw) {
        return Ok(Value::Number(number));
    }

    Ok(Value::String(raw.to_string()))
}

fn scalar_to_tagged_value(raw: &str, core_suffix: &str) -> Result<Value, String> {
    match core_suffix {
        "str" => Ok(Value::String(raw.to_string())),
        "null" if is_yaml_null(raw) || raw.is_empty() => Ok(Value::Null),
        "bool" => parse_yaml_bool(raw)
            .map(Value::Bool)
            .ok_or_else(|| format!("invalid YAML bool scalar: {raw:?}")),
        "int" => parse_yaml_int(raw)
            .map(Value::Number)
            .ok_or_else(|| format!("invalid YAML integer scalar: {raw:?}")),
        "float" => parse_yaml_float(raw)
            .map(Value::Number)
            .ok_or_else(|| format!("invalid YAML float scalar: {raw:?}")),
        _ => Ok(Value::String(raw.to_string())),
    }
}

fn is_yaml_null(raw: &str) -> bool {
    matches!(raw, "" | "~" | "null" | "Null" | "NULL")
}

fn parse_yaml_bool(raw: &str) -> Option<bool> {
    match raw {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

fn parse_yaml_int(raw: &str) -> Option<Number> {
    let cleaned = raw.replace('_', "");
    let (negative, body) = match cleaned.as_bytes().first()? {
        b'-' => (true, &cleaned[1..]),
        b'+' => (false, &cleaned[1..]),
        _ => (false, cleaned.as_str()),
    };
    if body.is_empty() {
        return None;
    }

    if body == "0" {
        return Some(Number::from(0));
    }

    if let Some(digits) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
        return parse_nondecimal_int(digits, 2, negative);
    }
    if let Some(digits) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
        return parse_nondecimal_int(digits, 8, negative);
    }
    if let Some(digits) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        return parse_nondecimal_int(digits, 16, negative);
    }

    if !body.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    parse_decimal_int(body, negative)
}

fn parse_yaml_float(raw: &str) -> Option<Number> {
    let cleaned = raw.replace('_', "");
    let (negative, body) = match cleaned.as_bytes().first()? {
        b'-' => (true, &cleaned[1..]),
        b'+' => (false, &cleaned[1..]),
        _ => (false, cleaned.as_str()),
    };
    if body.is_empty() {
        return None;
    }

    let value = match body {
        ".inf" | ".Inf" | ".INF" => Some(f64::INFINITY),
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
        _ => {
            if !is_yaml_float_body(body) {
                return None;
            }
            body.parse::<f64>().ok()
        }
    }?;

    let value = if negative { -value } else { value };
    Number::from_f64(value)
}

fn parse_decimal_int(body: &str, negative: bool) -> Option<Number> {
    let unsigned = body.parse::<u128>().ok()?;
    if negative {
        let signed = i128::try_from(unsigned).ok()?.checked_neg()?;
        if let Ok(value) = i64::try_from(signed) {
            Some(Number::from(value))
        } else {
            Number::from_f64(signed as f64)
        }
    } else if let Ok(value) = u64::try_from(unsigned) {
        Some(Number::from(value))
    } else {
        Number::from_f64(unsigned as f64)
    }
}

fn parse_nondecimal_int(digits: &str, radix: u32, negative: bool) -> Option<Number> {
    if digits.is_empty() || !digits.chars().all(|ch| ch.is_digit(radix)) {
        return None;
    }
    let unsigned = u128::from_str_radix(digits, radix).ok()?;
    if negative {
        let signed = i128::try_from(unsigned).ok()?.checked_neg()?;
        if let Ok(value) = i64::try_from(signed) {
            Some(Number::from(value))
        } else {
            Number::from_f64(signed as f64)
        }
    } else if let Ok(value) = u64::try_from(unsigned) {
        Some(Number::from(value))
    } else {
        Number::from_f64(unsigned as f64)
    }
}

fn is_yaml_float_body(body: &str) -> bool {
    let mut chars = body.chars().peekable();
    let mut saw_digit = false;

    let mut int_digits = 0usize;
    while chars.peek().is_some_and(|ch| ch.is_ascii_digit()) {
        chars.next();
        saw_digit = true;
        int_digits += 1;
    }

    let mut frac_digits = 0usize;
    if chars.peek() == Some(&'.') {
        chars.next();
        while chars.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            chars.next();
            saw_digit = true;
            frac_digits += 1;
        }
        if int_digits == 0 && frac_digits == 0 {
            return false;
        }
    } else if int_digits == 0 && chars.peek() != Some(&'.') {
        return false;
    }

    if let Some(&ch) = chars.peek()
        && (ch == 'e' || ch == 'E')
    {
        chars.next();
        if matches!(chars.peek(), Some('+') | Some('-')) {
            chars.next();
        }
        let mut exp_digits = 0usize;
        while chars.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            chars.next();
            exp_digits += 1;
        }
        if exp_digits == 0 {
            return false;
        }
        saw_digit = true;
    }

    saw_digit && chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OperationCancelled;
    use serde_json::json;

    fn capture_yaml(input: &str) -> YamlValueCapture {
        parse_yaml_value_capture_controlled(input, 16, &OperationControl::new())
            .expect("a private parse control cannot be cancelled")
    }

    #[test]
    fn controlled_yaml_parse_stops_before_consuming_events() {
        let control = OperationControl::new();
        control.cancel();

        let result = parse_yaml_value_controlled("value: 1\n", 16, &control);

        assert!(matches!(result, Err(OperationCancelled { .. })));
    }

    #[test]
    fn parses_nested_yaml_without_recursion() {
        let value = parse_yaml_value(
            r#"
config:
  theme: base
  flowchart:
    htmlLabels: true
  values: [1, 0x10, false, null]
"#,
            16,
        )
        .expect("yaml parses");

        assert_eq!(
            value,
            json!({
                "config": {
                    "theme": "base",
                    "flowchart": {
                        "htmlLabels": true
                    },
                    "values": [1, 16, false, null]
                }
            })
        );
    }

    #[test]
    fn ignores_complex_mapping_keys() {
        let value = parse_yaml_value(
            r#"
? [non, string, key]
: ignored
plain: retained
"#,
            16,
        )
        .expect("yaml parses");

        assert_eq!(value, json!({ "plain": "retained" }));
    }

    #[test]
    fn aliases_share_parser_storage_but_materialize_independent_json_values() {
        let value = parse_yaml_value(
            r#"
base: &base
  theme: forest
  flowchart:
    htmlLabels: true
first: *base
second: *base
"#,
            16,
        )
        .expect("yaml aliases parse");

        assert_eq!(value["first"], value["base"]);
        assert_eq!(value["second"], value["base"]);
    }

    #[test]
    fn key_evidence_does_not_flatten_mapping_paths_through_sequences() {
        let captured = capture_yaml(
            r#"
config:
  - lazyLoadedDiagrams: true
plain:
  lazyLoadedDiagrams: false
"#,
        );
        captured.value.expect("yaml parses");

        assert!(
            captured
                .keys
                .iter()
                .any(|key| key.path.matches(&["plain", "lazyLoadedDiagrams"]))
        );
        assert!(!captured.keys.iter().any(|key| {
            key.path.matches(&["config", "lazyLoadedDiagrams"])
                || key.path.matches(&["lazyLoadedDiagrams"])
        }));
    }

    #[test]
    fn complex_mapping_keys_do_not_publish_descendant_config_paths() {
        let captured = capture_yaml(
            r#"
config:
  ? { flowchart: { htmlLabels: false } }
  : ignored
plain:
  htmlLabels: true
"#,
        );
        captured.value.expect("yaml parses");

        assert!(
            captured
                .keys
                .iter()
                .any(|key| key.path.matches(&["plain", "htmlLabels"]))
        );
        assert!(!captured.keys.iter().any(|key| {
            key.path.matches(&["config", "flowchart"])
                || key.path.matches(&["config", "flowchart", "htmlLabels"])
        }));
        assert!(!captured.rewrite_safe);
    }

    #[test]
    fn alias_keys_and_values_do_not_publish_anchor_provenance_at_the_use_site() {
        let captured = capture_yaml(
            r#"
name: &key lazyLoadedDiagrams
? *key
: true
defaults: &defaults
  htmlLabels: false
config: *defaults
"#,
        );
        captured.value.expect("yaml aliases parse");

        assert!(captured.keys.iter().any(|key| key.path.matches(&["name"])));
        assert!(
            captured
                .keys
                .iter()
                .any(|key| key.path.matches(&["defaults", "htmlLabels"]))
        );
        assert!(
            captured
                .keys
                .iter()
                .any(|key| key.path.matches(&["config"]))
        );
        assert!(!captured.keys.iter().any(|key| {
            key.path.matches(&["lazyLoadedDiagrams"]) || key.path.matches(&["config", "htmlLabels"])
        }));
        assert!(!captured.rewrite_safe);
    }

    #[test]
    fn rejects_exponential_alias_materialization() {
        let mut yaml = String::from("a0: &a0 [x]\n");
        for level in 1..=18 {
            yaml.push_str(&format!(
                "a{level}: &a{level} [*a{}, *a{}]\n",
                level - 1,
                level - 1
            ));
        }
        yaml.push_str("root: *a18\n");

        let error = parse_yaml_value(&yaml, 64).unwrap_err();
        assert!(
            error.contains("safe materialization budget"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn caller_can_apply_a_stricter_yaml_materialization_budget() {
        let control = OperationControl::new();
        let result = parse_yaml_value_with_limits_controlled(
            "values: [one, two, three, four]\n",
            1024,
            16,
            8,
            &control,
        )
        .expect("active control");

        let error = result.expect_err("the caller budget must reject materialization");
        assert!(
            error.contains("safe materialization budget"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn caller_can_bound_the_third_party_yaml_parser_input() {
        let control = OperationControl::new();
        let result = parse_yaml_value_with_limits_controlled(
            "value: a-very-long-single-token\n",
            8,
            16,
            64 * 1024,
            &control,
        )
        .expect("active control");

        let error = result.expect_err("the parser-input budget must reject the token");
        assert!(error.contains("safe parser budget"));
    }

    #[test]
    fn alias_expansion_respects_the_same_nesting_limit_as_source_collections() {
        let mut yaml = String::from("a0: &a0 [x]\n");
        for level in 1..=8 {
            yaml.push_str(&format!("a{level}: &a{level} [*a{}]\n", level - 1));
        }
        yaml.push_str("root: *a8\n");

        let error = parse_yaml_value(&yaml, 4).unwrap_err();
        assert!(
            error.contains("after resolving YAML aliases"),
            "unexpected error: {error}"
        );
    }
}
