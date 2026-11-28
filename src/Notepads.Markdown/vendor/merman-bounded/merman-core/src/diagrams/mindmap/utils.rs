use crate::{MermaidConfig, SourceSpan};

use super::{
    NODE_TYPE_BANG, NODE_TYPE_CIRCLE, NODE_TYPE_CLOUD, NODE_TYPE_DEFAULT, NODE_TYPE_HEXAGON,
    NODE_TYPE_RECT, NODE_TYPE_ROUNDED_RECT,
};

pub(super) fn strip_inline_comment(line: &str) -> &str {
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

pub(super) struct NodeSpec {
    pub id_raw: String,
    pub descr_raw: String,
    pub ty: i32,
    pub descr_is_markdown: bool,
    pub trace: NodeSpecTrace,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NodeSpecTrace {
    pub id_span: Option<SourceSpan>,
    pub description_span: Option<SourceSpan>,
}

pub(super) struct NodeSpecError {
    pub message: String,
    pub continuation: Option<NodeSpecContinuation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeSpecContinuation {
    AwaitingClosingDelimiter {
        expected: &'static str,
        text: Option<NodeTextContinuation>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeTextContinuation {
    Quoted,
    QuotedMarkdown,
    BareMarkdown,
}

impl NodeSpecContinuation {
    pub fn expected_closing(self) -> &'static str {
        match self {
            Self::AwaitingClosingDelimiter { expected, .. } => expected,
        }
    }

    pub fn has_open_text(self) -> bool {
        match self {
            Self::AwaitingClosingDelimiter { text, .. } => text.is_some(),
        }
    }
}

pub(super) fn parse_node_spec(input: &str) -> std::result::Result<NodeSpec, Box<NodeSpecError>> {
    let input = input.trim_end();
    if input.is_empty() {
        return Err(node_spec_error("expected node", None));
    }

    if let Some((start, end)) = node_delimiter_pair_at_start(input) {
        let (inner, tail, _) = extract_delimited(input, start, end)
            .map_err(|failure| node_spec_error(failure.message(), failure.continuation()))?;
        let parsed_text = parse_node_text(inner, start.len());
        let trace = NodeSpecTrace {
            id_span: Some(parsed_text.content),
            description_span: None,
        };
        if !tail.trim().is_empty() {
            return Err(node_spec_error("unexpected trailing input", None));
        }
        let ty = node_type_for(start, end);
        return Ok(NodeSpec {
            id_raw: parsed_text.value.clone(),
            descr_raw: parsed_text.value,
            ty,
            descr_is_markdown: parsed_text.is_markdown,
            trace,
        });
    }

    let (id_raw, rest) = split_node_id(input);
    let id_raw = id_raw.to_string();
    let rest = rest.trim_end();
    if rest.is_empty() {
        let id_end = id_raw.trim_end().len();
        return Ok(NodeSpec {
            id_raw: id_raw.clone(),
            descr_raw: id_raw,
            ty: NODE_TYPE_DEFAULT,
            descr_is_markdown: false,
            trace: NodeSpecTrace {
                id_span: Some(SourceSpan::new(0, id_end)),
                ..NodeSpecTrace::default()
            },
        });
    }

    let Some((start, end)) = node_delimiter_pair_at_start(rest) else {
        return Err(node_spec_error("expected node delimiter", None));
    };

    let rest_start = input.len() - rest.len();
    let (inner, tail, _) = extract_delimited(rest, start, end)
        .map_err(|failure| node_spec_error(failure.message(), failure.continuation()))?;
    let parsed_text = parse_node_text(inner, rest_start + start.len());
    let trace = NodeSpecTrace {
        id_span: Some(SourceSpan::new(0, input[..rest_start].trim_end().len())),
        description_span: Some(parsed_text.content),
    };
    if !tail.trim().is_empty() {
        return Err(node_spec_error("unexpected trailing input", None));
    }

    let ty = node_type_for(start, end);
    Ok(NodeSpec {
        id_raw,
        descr_raw: parsed_text.value,
        ty,
        descr_is_markdown: parsed_text.is_markdown,
        trace,
    })
}

pub(super) fn starts_node_spec(input: &str) -> bool {
    let input = input.trim_end();
    if input.is_empty() || node_delimiter_pair_at_start(input).is_some() {
        return !input.is_empty();
    }
    let (id, _) = split_node_id(input);
    !id.trim_end().is_empty()
}

fn node_spec_error(
    message: impl Into<String>,
    continuation: Option<NodeSpecContinuation>,
) -> Box<NodeSpecError> {
    Box::new(NodeSpecError {
        message: message.into(),
        continuation,
    })
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

enum DelimitedScanFailure {
    ExpectedOpening,
    AwaitingClosingDelimiter {
        expected: &'static str,
        text: Option<NodeTextContinuation>,
    },
}

impl DelimitedScanFailure {
    fn message(&self) -> &'static str {
        match self {
            Self::ExpectedOpening => "expected delimiter start",
            Self::AwaitingClosingDelimiter { .. } => "unterminated node delimiter",
        }
    }

    fn continuation(&self) -> Option<NodeSpecContinuation> {
        match *self {
            Self::ExpectedOpening => None,
            Self::AwaitingClosingDelimiter { expected, text } => {
                Some(NodeSpecContinuation::AwaitingClosingDelimiter { expected, text })
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeTextScanState {
    Plain,
    Quoted,
    QuotedMarkdown,
    BareMarkdown,
}

impl NodeTextScanState {
    fn continuation(self) -> Option<NodeTextContinuation> {
        match self {
            Self::Plain => None,
            Self::Quoted => Some(NodeTextContinuation::Quoted),
            Self::QuotedMarkdown => Some(NodeTextContinuation::QuotedMarkdown),
            Self::BareMarkdown => Some(NodeTextContinuation::BareMarkdown),
        }
    }
}

fn extract_delimited<'a>(
    input: &'a str,
    start: &'static str,
    end: &'static str,
) -> std::result::Result<(&'a str, &'a str, usize), DelimitedScanFailure> {
    if !input.starts_with(start) {
        return Err(DelimitedScanFailure::ExpectedOpening);
    }
    let mut text_state = NodeTextScanState::Plain;

    let start_len = start.len();
    let mut it = input[start_len..].char_indices().peekable();
    while let Some((off, ch)) = it.next() {
        let idx = start_len + off;

        match text_state {
            NodeTextScanState::QuotedMarkdown => {
                if ch == '`' && it.peek().is_some_and(|(_, next)| *next == '"') {
                    text_state = NodeTextScanState::Plain;
                    it.next();
                }
                continue;
            }
            NodeTextScanState::Quoted => {
                if ch == '"' {
                    text_state = NodeTextScanState::Plain;
                }
                continue;
            }
            NodeTextScanState::BareMarkdown => {
                if ch == '`' {
                    text_state = NodeTextScanState::Plain;
                    continue;
                }
                if input[idx..].starts_with(end) {
                    let inner = &input[start_len..idx];
                    let tail = &input[idx + end.len()..];
                    return Ok((inner, tail, idx));
                }
                continue;
            }
            NodeTextScanState::Plain => {}
        }

        match ch {
            '"' if it.peek().is_some_and(|(_, next)| *next == '`') => {
                text_state = NodeTextScanState::QuotedMarkdown;
                it.next();
                continue;
            }
            '"' => {
                text_state = NodeTextScanState::Quoted;
                continue;
            }
            '`' => {
                text_state = NodeTextScanState::BareMarkdown;
                continue;
            }
            _ => {}
        }

        if input[idx..].starts_with(end) {
            let inner = &input[start_len..idx];
            let tail = &input[idx + end.len()..];
            return Ok((inner, tail, idx));
        }
    }

    Err(DelimitedScanFailure::AwaitingClosingDelimiter {
        expected: end,
        text: text_state.continuation(),
    })
}

struct ParsedNodeText {
    value: String,
    content: SourceSpan,
    is_markdown: bool,
}

fn parse_node_text(raw: &str, raw_start: usize) -> ParsedNodeText {
    if let Some(inner) = raw.strip_prefix("\"`").and_then(|s| s.strip_suffix("`\"")) {
        return ParsedNodeText {
            value: inner.to_string(),
            content: SourceSpan::new(raw_start + 2, raw_start + raw.len() - 2),
            is_markdown: true,
        };
    }
    if let Some(inner) = raw.strip_prefix('\"').and_then(|s| s.strip_suffix('\"')) {
        return ParsedNodeText {
            value: inner.to_string(),
            content: SourceSpan::new(raw_start + 1, raw_start + raw.len() - 1),
            is_markdown: false,
        };
    }
    ParsedNodeText {
        value: raw.to_string(),
        content: SourceSpan::new(raw_start, raw_start + raw.len()),
        is_markdown: false,
    }
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

pub(super) fn get_i64(cfg: &MermaidConfig, dotted_path: &str) -> Option<i64> {
    let mut cur = cfg.as_value();
    for segment in dotted_path.split('.') {
        cur = cur.as_object()?.get(segment)?;
    }
    cur.as_i64().or_else(|| cur.as_f64().map(|f| f as i64))
}
