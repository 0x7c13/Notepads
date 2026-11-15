// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded native state diagrams. State syntax is classified before labels are
//! escaped into generated flowchart syntax; authored labels never become code.
//! The funded native flowchart producer remains the only layout/scene backend.

use crate::{
    DiagramError,
    layout::{
        LayoutPolicy, StateMarker, StateMarkerKind, ThemePolicy,
        render_state_flowchart_native_controlled,
    },
    native_common::{FlowchartEncoder, canonical, check, decode_label_text, scope},
    rich_text::RichTextProvider,
    scene::OwnedScene,
};
use markdown_memory::{Family, FundedString, FundedVec, MemoryError, MemoryScope};
use merman_core::{NativeParseError, NativeParsePolicy, OperationControl};

const SOURCE_LIMIT: usize = 16 * 1024;
const NODE_LIMIT: usize = 128;
const EDGE_LIMIT: usize = 256;
const GROUP_LIMIT: usize = 32;
const DEPTH_LIMIT: usize = 8;
const ROOT: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    State,
    Start,
    Final,
    Choice,
    Group(usize),
}

struct Node<'a> {
    id: &'a str,
    parent: usize,
    kind: Kind,
    label: FundedString,
    described: bool,
    declared: bool,
}

struct Group {
    parent: usize,
    direction: Option<&'static str>,
}

struct Edge {
    from: usize,
    to: usize,
    label: FundedString,
}

struct State<'a, 'c> {
    scope: MemoryScope,
    control: &'c OperationControl,
    nodes: FundedVec<Node<'a>>,
    groups: FundedVec<Group>,
    edges: FundedVec<Edge>,
    direction: Option<&'static str>,
    stack: [usize; DEPTH_LIMIT + 1],
    depth: usize,
}

fn limited<T>(scope: &MemoryScope) -> Result<T, DiagramError> {
    scope.record_failure(MemoryError::WorkLimited);
    Err(DiagramError::WorkLimited)
}

fn parse_error(error: NativeParseError, control: &OperationControl) -> DiagramError {
    match error {
        NativeParseError::Memory(error) => DiagramError::Memory(error),
        NativeParseError::Cancelled => check(control, 0).err().unwrap_or(DiagramError::Cancelled),
        NativeParseError::Unsupported { .. } => DiagramError::Unsupported,
        NativeParseError::Syntax { .. } | NativeParseError::Model => DiagramError::InvalidInput,
    }
}

/// Supports ordinary/aliased states, descriptions, choice states, transitions,
/// composite states and scoped initial/final states. Notes, fork/join bars,
/// concurrent regions and styling are explicitly rejected until their native
/// semantics are implemented.
pub fn render_state_native_controlled(
    source: &str,
    policy: LayoutPolicy,
    theme: ThemePolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<OwnedScene, DiagramError> {
    let state = State::parse(source, control)?;
    let translated = state.flowchart(theme)?;
    let input = merman_core::parse_flowchart_native_controlled(
        translated.as_str(),
        &NativeParsePolicy {
            inherit_dir: true,
            html_labels: false,
        },
        control,
    )
    .map_err(|error| parse_error(error, control))?;
    // Translation preserves generated IDs, while composite traversal can change
    // the parsed node order. Pass actual model indexes through the typed scene
    // hook; ordinary flowchart DoubleCircle paint remains unchanged.
    let mut markers = FundedVec::new_in(state.scope.clone(), Family::Graph);
    for (index, node) in input.0.nodes.iter().enumerate() {
        check(control, node.id.len().saturating_add(1))?;
        let authored = node
            .id
            .strip_prefix('s')
            .and_then(|value| value.parse::<usize>().ok())
            .and_then(|index| state.nodes.get(index));
        let kind = match authored.map(|node| node.kind) {
            Some(Kind::Start) => StateMarkerKind::Initial,
            Some(Kind::Final) => StateMarkerKind::Final,
            _ => continue,
        };
        markers.try_push(StateMarker { node: index, kind })?;
    }
    render_state_flowchart_native_controlled(&input, &markers, policy, theme, provider, control)
        .map_err(|error| canonical(error, control))
}

impl<'a, 'c> State<'a, 'c> {
    fn parse(source: &'a str, control: &'c OperationControl) -> Result<Self, DiagramError> {
        let scope = scope(control)?;
        check(control, 1)?;
        if source.len() > SOURCE_LIMIT {
            return limited(&scope);
        }
        let mut state = Self {
            nodes: FundedVec::new_in(scope.clone(), Family::Parse),
            groups: FundedVec::new_in(scope.clone(), Family::Parse),
            edges: FundedVec::new_in(scope.clone(), Family::Parse),
            direction: None,
            stack: [ROOT; DEPTH_LIMIT + 1],
            depth: 0,
            scope,
            control,
        };
        let mut header = false;
        let mut cursor = 0;
        let source = source.trim_start_matches('\u{feff}');
        while cursor < source.len() {
            let (line, next) = statement(source, cursor, control)?;
            cursor = next;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if !header {
                if !matches!(line, "stateDiagram" | "stateDiagram-v2") {
                    return Err(if line.starts_with("---") || line.starts_with("%%{") {
                        DiagramError::Unsupported
                    } else {
                        DiagramError::InvalidInput
                    });
                }
                header = true;
                continue;
            }
            state.statement(line)?;
        }
        if !header || state.depth != 0 || state.nodes.is_empty() {
            return Err(DiagramError::InvalidInput);
        }
        Ok(state)
    }

    fn current(&self) -> usize {
        self.stack[self.depth]
    }

    fn node(&mut self, id: &'a str, pseudo: Option<Kind>) -> Result<usize, DiagramError> {
        let kind = pseudo.unwrap_or(Kind::State);
        if pseudo.is_none() {
            valid_id(id)?;
        } else if id != "[*]" {
            return Err(DiagramError::InvalidInput);
        }
        check(self.control, self.nodes.len().saturating_add(id.len()))?;
        let parent = self.current();
        if let Some(index) = self.nodes.iter().position(|node| {
            node.parent == parent && node.id == id && (pseudo.is_none() || node.kind == kind)
        }) {
            return Ok(index);
        }
        if self.nodes.len() >= NODE_LIMIT {
            return limited(&self.scope);
        }
        let label = FundedString::try_from_str(
            &self.scope,
            Family::Text,
            if pseudo.is_some() { "" } else { id },
        )?;
        let index = self.nodes.len();
        self.nodes.try_push(Node {
            id,
            parent,
            kind,
            label,
            described: false,
            declared: false,
        })?;
        Ok(index)
    }

    fn endpoint(&mut self, id: &'a str, from: bool) -> Result<usize, DiagramError> {
        self.node(
            id,
            (id == "[*]").then_some(if from { Kind::Start } else { Kind::Final }),
        )
    }

    fn description(&mut self, node: usize, raw: &str, append: bool) -> Result<(), DiagramError> {
        if self.nodes[node].kind == Kind::Choice {
            return Err(DiagramError::Unsupported);
        }
        let text = decoded(raw, &self.scope, self.control)?;
        let node = &mut self.nodes[node];
        if append && node.described {
            node.label.try_push('\n')?;
        } else {
            node.label.clear();
        }
        node.label.try_push_str(&text)?;
        node.described = true;
        Ok(())
    }

    fn statement(&mut self, line: &'a str) -> Result<(), DiagramError> {
        check(self.control, line.len().saturating_add(1))?;
        if line == "}" {
            if self.depth == 0 {
                return Err(DiagramError::InvalidInput);
            }
            self.depth -= 1;
            return Ok(());
        }
        if line.starts_with("%%{")
            || line == "--"
            || [
                "note", "end note", "classDef", "class", "style", "click", "accTitle", "accDescr",
            ]
            .iter()
            .any(|word| keyword(line, word).is_some())
        {
            return Err(DiagramError::Unsupported);
        }
        if let Some(raw) = keyword(line, "direction") {
            let direction = match raw.trim() {
                "TB" | "TD" => "TB",
                "BT" => "BT",
                "LR" => "LR",
                "RL" => "RL",
                _ => return Err(DiagramError::InvalidInput),
            };
            let current = self.current();
            let slot = if current == ROOT {
                &mut self.direction
            } else {
                &mut self.groups[current].direction
            };
            if slot.is_some() {
                return Err(DiagramError::InvalidInput);
            }
            *slot = Some(direction);
            return Ok(());
        }
        if let Some(raw) = keyword(line, "state") {
            return self.declaration(raw.trim());
        }
        if matches!(line, "stateDiagram" | "stateDiagram-v2") {
            return Err(DiagramError::InvalidInput);
        }
        if line
            .split_once(':')
            .is_some_and(|(_, raw)| raw.starts_with("::"))
        {
            return Err(DiagramError::Unsupported);
        }
        // A colon classifies the remainder as label text before inspecting
        // arrows. Thus an arrow in a state description cannot create an edge.
        let (body, description) = split_description(line);
        if let Some((from, to)) = body.split_once("-->") {
            let from = from.trim();
            let to = to.trim();
            if to.contains("-->") {
                return Err(DiagramError::InvalidInput);
            }
            let from = self.endpoint(from, true)?;
            let to = self.endpoint(to, false)?;
            if self.edges.len() >= EDGE_LIMIT {
                return limited(&self.scope);
            }
            let label = decoded(description.unwrap_or(""), &self.scope, self.control)?;
            self.edges.try_push(Edge { from, to, label })?;
            return Ok(());
        }
        let id = body.trim();
        let node = self.node(id, None)?;
        if let Some(raw) = description {
            self.description(node, raw, true)?;
        }
        Ok(())
    }

    fn declaration(&mut self, raw: &'a str) -> Result<(), DiagramError> {
        let (raw, composite) = if let Some(raw) = raw.strip_suffix('{') {
            (raw.trim_end(), true)
        } else {
            (raw, false)
        };
        let (id, label, modifier) = if raw.starts_with('"') {
            let end = quoted_end(raw).ok_or(DiagramError::InvalidInput)?;
            let label = &raw[..=end];
            let after = raw[end + 1..].trim_start();
            let alias = keyword(after, "as").ok_or(DiagramError::InvalidInput)?;
            let (id, modifier) = id_modifier(alias.trim());
            (id, Some(label), modifier)
        } else {
            let (id, modifier) = id_modifier(raw);
            (id, None, modifier)
        };
        if modifier == "<<fork>>" || modifier == "<<join>>" {
            return Err(DiagramError::Unsupported);
        }
        if !modifier.is_empty() && modifier != "<<choice>>" {
            return Err(if modifier.starts_with("<<") {
                DiagramError::Unsupported
            } else {
                DiagramError::InvalidInput
            });
        }
        if composite && !modifier.is_empty() {
            return Err(DiagramError::InvalidInput);
        }
        if label.is_some() && !modifier.is_empty() {
            return Err(DiagramError::Unsupported);
        }
        let node = self.node(id, None)?;
        if self.nodes[node].declared || matches!(self.nodes[node].kind, Kind::Group(_)) {
            return Err(DiagramError::InvalidInput);
        }
        self.nodes[node].declared = true;
        if let Some(label) = label {
            self.description(node, label, false)?;
        }
        if composite {
            if self.groups.len() >= GROUP_LIMIT || self.depth >= DEPTH_LIMIT {
                return limited(&self.scope);
            }
            let group = self.groups.len();
            self.groups.try_push(Group {
                parent: self.current(),
                direction: None,
            })?;
            self.nodes[node].kind = Kind::Group(group);
            self.depth += 1;
            self.stack[self.depth] = group;
        } else if modifier == "<<choice>>" {
            self.nodes[node].kind = Kind::Choice;
            // The identifier of a choice is an anchor, not visible text.
            self.nodes[node].label.clear();
        }
        Ok(())
    }

    fn group_direction(&self, mut group: usize) -> Result<&'static str, DiagramError> {
        for _ in 0..DEPTH_LIMIT {
            check(self.control, 1)?;
            let value = &self.groups[group];
            if let Some(direction) = value.direction {
                return Ok(direction);
            }
            if value.parent == ROOT {
                return Ok(self.direction.unwrap_or("TB"));
            }
            group = value.parent;
        }
        Err(DiagramError::InvalidInput)
    }

    fn flowchart(&self, theme: ThemePolicy) -> Result<FundedString, DiagramError> {
        let mut out = FlowchartEncoder::new(&self.scope, self.control);
        out.put("flowchart ")?;
        out.put(self.direction.unwrap_or("TB"))?;
        out.put("\n")?;
        // Explicit frames keep even maximum-depth input off the Rust stack.
        let mut scopes = [ROOT; DEPTH_LIMIT + 1];
        let mut positions = [0; DEPTH_LIMIT + 1];
        let mut depth = 0;
        loop {
            check(self.control, 1)?;
            let current = scopes[depth];
            let mut next = positions[depth];
            while next < self.nodes.len() && self.nodes[next].parent != current {
                check(self.control, 1)?;
                next += 1;
            }
            if next == self.nodes.len() {
                if depth == 0 {
                    break;
                }
                out.put("end\n")?;
                depth -= 1;
                continue;
            }
            positions[depth] = next + 1;
            let node = &self.nodes[next];
            if let Kind::Group(group) = node.kind {
                if self.groups[group].parent != current || depth >= DEPTH_LIMIT {
                    return Err(DiagramError::InvalidInput);
                }
                out.put("subgraph ")?;
                out.id("s", next)?;
                out.put("[\"")?;
                out.label(&node.label)?;
                out.put("\"]\n")?;
                // native flowchart inherits its global direction. State
                // composites inherit their immediate parent instead, so
                // resolve that difference before entering the shared parser.
                out.put("direction ")?;
                out.put(self.group_direction(group)?)?;
                out.put("\n")?;
                depth += 1;
                scopes[depth] = group;
                positions[depth] = 0;
                continue;
            }
            out.id("s", next)?;
            let (open, close) = match node.kind {
                Kind::State => ("(\"", "\")\n"),
                Kind::Start => ("((\"", "\"))\n"),
                Kind::Final => ("(((\"", "\")))\n"),
                Kind::Choice => ("{\"", "\"}\n"),
                Kind::Group(_) => return Err(DiagramError::InvalidInput),
            };
            out.put(open)?;
            out.label(&node.label)?;
            out.put(close)?;
        }
        for (index, node) in self.nodes.iter().enumerate() {
            check(self.control, 1)?;
            if node.kind == Kind::Start {
                out.put("style ")?;
                out.id("s", index)?;
                out.put(" fill:")?;
                out.color(theme.text.rgb)?;
                out.put(",stroke:")?;
                out.color(theme.text.rgb)?;
                out.put("\n")?;
            }
        }
        for edge in &self.edges {
            check(self.control, 1)?;
            out.id("s", edge.from)?;
            if edge.label.is_empty() {
                out.put(" --> ")?;
            } else {
                out.put(" -->|\"")?;
                out.label(&edge.label)?;
                out.put("\"| ")?;
            }
            out.id("s", edge.to)?;
            out.put("\n")?;
        }
        Ok(out.finish())
    }
}

fn keyword<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    line.strip_prefix(word).filter(|rest| {
        rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with(':')
    })
}

fn valid_id(id: &str) -> Result<(), DiagramError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|value| value.is_alphanumeric() || matches!(value, '_' | '-' | '.'))
    {
        return Err(if id.contains(":::") {
            DiagramError::Unsupported
        } else {
            DiagramError::InvalidInput
        });
    }
    Ok(())
}

fn id_modifier(raw: &str) -> (&str, &str) {
    if let Some(position) = raw.find(char::is_whitespace) {
        (&raw[..position], raw[position..].trim())
    } else if let Some(position) = raw.find("<<") {
        (&raw[..position], &raw[position..])
    } else {
        (raw, "")
    }
}

fn split_description(line: &str) -> (&str, Option<&str>) {
    if let Some((body, raw)) = line.split_once(':') {
        (body, Some(raw.trim()))
    } else {
        (line, None)
    }
}

fn quoted_end(raw: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, value) in raw.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if value == '\\' {
            escaped = true;
        } else if value == '"' {
            return Some(index);
        }
    }
    None
}

/// Comments are removed only outside quotes, and a configuration directive is
/// never mistaken for a harmless comment. Actual newlines inside quotes stay
/// in the same statement and are subsequently emitted only as label content.
fn statement<'a>(
    source: &'a str,
    begin: usize,
    control: &OperationControl,
) -> Result<(&'a str, usize), DiagramError> {
    let mut quoted = false;
    let mut escaped = false;
    let mut chars = source[begin..].char_indices().peekable();
    while let Some((offset, value)) = chars.next() {
        check(control, value.len_utf8())?;
        let index = begin + offset;
        if quoted {
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == '"' {
                quoted = false;
            }
        } else if value == '"' {
            quoted = true;
        } else if value == '\n' || value == '\r' {
            return Ok((&source[begin..index], index + value.len_utf8()));
        } else if value == '%' && chars.peek().is_some_and(|(_, next)| *next == '%') {
            if source[index..].starts_with("%%{") {
                return Err(DiagramError::Unsupported);
            }
            let end = source[index..]
                .find(['\n', '\r'])
                .map_or(source.len(), |end| index + end);
            check(control, end - index)?;
            return Ok((&source[begin..index], end));
        }
    }
    if quoted {
        return Err(DiagramError::InvalidInput);
    }
    Ok((&source[begin..], source.len()))
}

fn decoded(
    raw: &str,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<FundedString, DiagramError> {
    let raw = raw.trim();
    let raw = if raw.starts_with('"') {
        let end = quoted_end(raw).ok_or(DiagramError::InvalidInput)?;
        if end + 1 == raw.len() {
            &raw[1..end]
        } else {
            raw
        }
    } else {
        raw
    };
    let mut text = FundedString::new_in(scope.clone(), Family::Text);
    let mut chars = raw.chars();
    while let Some(value) = chars.next() {
        check(control, value.len_utf8())?;
        if value != '\\' {
            text.try_push(value)?;
            continue;
        }
        match chars.next() {
            Some('n') => text.try_push('\n')?,
            Some('r') => text.try_push('\r')?,
            Some('t') => text.try_push('\t')?,
            Some('"') => text.try_push('"')?,
            Some('\\') => text.try_push('\\')?,
            Some(value) => {
                text.try_push('\\')?;
                text.try_push(value)?;
            }
            None => text.try_push('\\')?,
        }
    }
    decode_label_text(&text, scope, control, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canonical::{CanonicalError, OutlineProvider, PathSink, Rect, RunMetrics, TextRun},
        rich_text::TextDirection,
        scene::shapes::{self, Shape},
    };
    use markdown_memory::{AllocationAuthority, NativeRegion};
    use std::{
        alloc::Layout,
        cell::{Cell, RefCell},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    struct Authority {
        used: AtomicUsize,
        allocations: AtomicUsize,
        fail_at: AtomicUsize,
        work: AtomicUsize,
        work_limit: AtomicUsize,
        failure: AtomicUsize,
    }

    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
            self.used
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    value
                        .checked_add(layout.size())
                        .filter(|n| *n <= 64 * 1024 * 1024)
                })
                .map(|_| ())
                .map_err(|_| MemoryError::BudgetLimited)
        }

        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }

        fn before_allocate(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
            let index = self.allocations.fetch_add(1, Ordering::SeqCst) + 1;
            if index == self.fail_at.load(Ordering::SeqCst) {
                Err(MemoryError::OutOfMemory)
            } else {
                Ok(())
            }
        }

        fn charge_work(&self, work: usize) -> Result<(), MemoryError> {
            self.work
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    value
                        .checked_add(work)
                        .filter(|n| *n <= self.work_limit.load(Ordering::SeqCst))
                })
                .map(|_| ())
                .map_err(|_| MemoryError::WorkLimited)
        }

        fn record_failure(&self, error: MemoryError) {
            let value = match error {
                MemoryError::BudgetLimited => 1,
                MemoryError::OutOfMemory => 2,
                MemoryError::ArithmeticOverflow => 3,
                MemoryError::InvalidState => 4,
                MemoryError::Cancelled => 5,
                MemoryError::Unfunded => 6,
                MemoryError::WorkLimited => 7,
            };
            let _ = self
                .failure
                .compare_exchange(0, value, Ordering::SeqCst, Ordering::SeqCst);
        }

        fn failure(&self) -> Option<MemoryError> {
            match self.failure.load(Ordering::SeqCst) {
                0 => None,
                1 => Some(MemoryError::BudgetLimited),
                2 => Some(MemoryError::OutOfMemory),
                3 => Some(MemoryError::ArithmeticOverflow),
                4 => Some(MemoryError::InvalidState),
                5 => Some(MemoryError::Cancelled),
                6 => Some(MemoryError::Unfunded),
                _ => Some(MemoryError::WorkLimited),
            }
        }
    }

    fn setup() -> (Arc<Authority>, OperationControl) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
            fail_at: AtomicUsize::new(0),
            work: AtomicUsize::new(0),
            work_limit: AtomicUsize::new(4_000_000),
            failure: AtomicUsize::new(0),
        });
        let scope = MemoryScope::new(authority.clone());
        let region = NativeRegion::try_new(&scope, 8192).unwrap();
        let control = OperationControl::new().with_memory_region(region);
        authority.allocations.store(0, Ordering::SeqCst);
        (authority, control)
    }

    struct Font;
    impl OutlineProvider for Font {
        fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
            self.measure_spans(&[run], TextDirection::Auto)
        }
        fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
            self.outline_spans(&[run], TextDirection::Auto, sink)
        }
    }
    impl RichTextProvider for Font {
        fn measure_spans(
            &self,
            runs: &[TextRun<'_>],
            _: TextDirection,
        ) -> Result<RunMetrics, CanonicalError> {
            let width = runs
                .iter()
                .map(|run| run.text.chars().count() as f64 * 7.0)
                .sum();
            Ok(RunMetrics {
                advance: width,
                ascent: 12.0,
                descent: 4.0,
                x_height: 8.0,
                ink: Rect {
                    left: 0.0,
                    top: -12.0,
                    right: width,
                    bottom: 4.0,
                },
            })
        }
        fn outline_spans(
            &self,
            runs: &[TextRun<'_>],
            direction: TextDirection,
            sink: &mut dyn PathSink,
        ) -> Result<(), CanonicalError> {
            let metrics = self.measure_spans(runs, direction)?;
            if metrics.advance > 0.0 {
                shapes::outline(Shape::Rectangle, metrics.ink, sink)?;
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct CapturedFont {
        outlined: RefCell<Vec<String>>,
    }

    impl OutlineProvider for CapturedFont {
        fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
            self.measure_spans(&[run], TextDirection::Auto)
        }

        fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
            self.outline_spans(&[run], TextDirection::Auto, sink)
        }
    }

    impl RichTextProvider for CapturedFont {
        fn measure_spans(
            &self,
            runs: &[TextRun<'_>],
            direction: TextDirection,
        ) -> Result<RunMetrics, CanonicalError> {
            Font.measure_spans(runs, direction)
        }

        fn outline_spans(
            &self,
            runs: &[TextRun<'_>],
            direction: TextDirection,
            sink: &mut dyn PathSink,
        ) -> Result<(), CanonicalError> {
            self.outlined
                .borrow_mut()
                .push(runs.iter().map(|run| run.text).collect());
            Font.outline_spans(runs, direction, sink)
        }
    }

    // Self-authored examples use the documented state syntax oracle:
    // https://mermaid.js.org/syntax/stateDiagram.html (read 2026-11-13).
    // The vendored merman/tests/ascii_api.rs also fixes the initial-state form.
    #[test]
    fn state_syntax_oracle_scenes_use_the_existing_canonical_pipeline() {
        for source in [
            "stateDiagram-v2\n[*] --> Loading\nLoading --> Ready: loaded\nReady --> [*]",
            "stateDiagram\nA --> A: retry\nA --> B\nB --> A: restart",
            "stateDiagram-v2\nstate \"準備\\nReady\" as R\n[*] --> R\nR --> [*]",
            "stateDiagram-v2\n[*] --> Session\nstate Session {\n direction LR\n [*] --> Idle\n Idle --> Done\n Done --> [*]\n}\nSession --> [*]",
            "stateDiagram-v2\nstate decision <<choice>>\n[*] --> A\nA --> decision\ndecision --> B: yes\ndecision --> C: no",
            "stateDiagram-v2\n[*] --> [*]",
        ] {
            let (authority, control) = setup();
            let scene = render_state_native_controlled(
                source,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &Font,
                &control,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{source}: {error:?}; charged work={}, scope failure={:?}",
                    authority.work.load(Ordering::SeqCst),
                    control.memory_scope().unwrap().failure()
                )
            });
            assert!(scene.paths > 0);
            assert!(!scene.svg.contains("<text"));
            let facts = crate::canonical::ArtifactFacts {
                bounds: scene.bounds,
                paths: scene.paths,
                commands: scene.commands,
            };
            let accepted = crate::canonical::accept_artifact(
                scene.svg,
                facts,
                control.memory_scope().unwrap(),
                &control,
                crate::canonical::CanonicalLimits::default(),
            )
            .unwrap();
            drop(accepted);
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn leading_bom_preserves_the_direct_state_scene() {
        let (authority, control) = setup();
        let source = "stateDiagram-v2\n[*] --> A\nA --> B: ready";
        let render = |source| {
            render_state_native_controlled(
                source,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &Font,
                &control,
            )
            .unwrap()
        };
        let plain = render(source);
        let prefixed = render(&format!("\u{feff}{source}"));
        assert_eq!(prefixed.svg.as_str(), plain.svg.as_str());
        assert_eq!(prefixed.paths, plain.paths);
        assert_eq!(prefixed.commands, plain.commands);
        drop((plain, prefixed, control));
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn descriptions_and_transitions_preserve_quoted_words_with_trailing_text() {
        for (raw, expected) in [
            (r#""hello" and "goodbye""#, r#""hello" and "goodbye""#),
            (r#""hello" and goodbye"#, r#""hello" and goodbye"#),
            (r#""whole \"quote\"""#, r#"whole "quote""#),
        ] {
            let (authority, control) = setup();
            let source = format!("stateDiagram-v2\nA: {raw}\nA --> B: {raw}");
            let state = State::parse(&source, &control).unwrap();
            assert_eq!(state.nodes.len(), 2);
            assert_eq!(state.edges.len(), 1);
            assert_eq!(state.nodes[0].label.as_str(), expected);
            assert_eq!(state.edges[0].label.as_str(), expected);
            let font = CapturedFont::default();
            let scene = render_state_native_controlled(
                &source,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &font,
                &control,
            )
            .unwrap();
            assert!(font.outlined.borrow().iter().any(|text| text == expected));
            drop((scene, state));
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
        let (authority, control) = setup();
        for source in [
            "stateDiagram-v2\nA: \"unterminated",
            "stateDiagram-v2\nA --> B: \"unterminated",
            "stateDiagram-v2\nstate \"hello\" and goodbye as A",
        ] {
            assert!(matches!(
                State::parse(source, &control),
                Err(DiagramError::InvalidInput)
            ));
        }
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn decoded_descriptions_never_reenter_mermaid_or_markup_recognition() {
        let (authority, control) = setup();
        let source = "stateDiagram-v2\nstate \"&lt;br&gt; &amp; #65; #quot; ] --> Evil %%{init}%% \\\\ 中文\" as R\nR: 次の行\nR --> Done: \"x | ] --> Sneak &lt;b&gt; &amp;\"";
        let state = State::parse(source, &control).unwrap();
        assert_eq!(state.nodes.len(), 2);
        assert_eq!(state.edges.len(), 1);
        let translated = state.flowchart(ThemePolicy::default()).unwrap();
        let input = merman_core::parse_flowchart_native_controlled(
            &translated,
            &NativeParsePolicy::default(),
            &control,
        )
        .unwrap();
        assert_eq!(input.0.nodes.len(), 2);
        assert_eq!(input.0.edges.len(), 1);
        assert_eq!(
            input.0.nodes[0].label.as_deref(),
            Some("<br> & A \" ] --> Evil %%{init}%% \\ 中文\n次の行")
        );
        assert_eq!(
            input.0.edges[0].label.as_deref(),
            Some("x | ] --> Sneak <b> &")
        );
        let scene = render_state_native_controlled(
            source,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &control,
        )
        .unwrap();
        drop((scene, input, translated, state));
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn node_and_transition_entities_and_breaks_reach_the_actual_outline_provider() {
        for (source, expected) in [
            (
                "stateDiagram-v2\nstate \"A&NewLine;B#NewLine;C&#10;D&Tab;E#Tab;F&#9;G\\nRaw<br>Break\" as R",
                &["A B C D E F G", "Raw", "Break"][..],
            ),
            (
                "stateDiagram-v2\nstate \"Ready #35; 1\" as R",
                &["Ready # 1"][..],
            ),
            (
                "stateDiagram-v2\nstate \"Ready<br/>Now\" as R",
                &["Ready", "Now"][..],
            ),
            (
                "stateDiagram-v2\nA --> B: \"go #35; &amp; #quot;done#quot;<BR />now\"",
                &["go # & \"done\"", "now"][..],
            ),
            (
                "stateDiagram-v2\nstate \"Keep #35;35; &amp;lt;br&amp;gt; &lt;br&gt;\" as R",
                &["Keep #35; &lt;br&gt; <br>"][..],
            ),
        ] {
            let (authority, control) = setup();
            let provider = CapturedFont::default();
            let scene = render_state_native_controlled(
                source,
                LayoutPolicy {
                    wrap_width: 400.0,
                    ..LayoutPolicy::default()
                },
                ThemePolicy::default(),
                &provider,
                &control,
            )
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
            let outlined = provider.outlined.borrow();
            for expected in expected {
                assert!(
                    outlined.iter().any(|line| line == expected),
                    "{source}: {outlined:?}"
                );
            }
            assert!(!scene.svg.contains("<text"));
            drop(outlined);
            drop((scene, control));
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }

    fn path_bounds(path: &str) -> Rect {
        let data = path
            .split_once(" d=\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0;
        let numbers: Vec<f64> = data
            .split(|c: char| c.is_ascii_alphabetic() || c.is_whitespace() || c == ',')
            .filter(|value| !value.is_empty())
            .map(|value| value.parse().unwrap())
            .collect();
        let mut bounds = Rect {
            left: f64::INFINITY,
            top: f64::INFINITY,
            right: f64::NEG_INFINITY,
            bottom: f64::NEG_INFINITY,
        };
        for pair in numbers.chunks_exact(2) {
            bounds.left = bounds.left.min(pair[0]);
            bounds.right = bounds.right.max(pair[0]);
            bounds.top = bounds.top.min(pair[1]);
            bounds.bottom = bounds.bottom.max(pair[1]);
        }
        bounds
    }

    fn assert_extent(bounds: Rect, diameter: f64) {
        assert!((bounds.right - bounds.left - diameter).abs() < 1e-6);
        assert!((bounds.bottom - bounds.top - diameter).abs() < 1e-6);
    }

    #[test]
    fn rendered_initial_and_final_markers_keep_absolute_geometry_in_all_directions_and_policies() {
        let theme = ThemePolicy {
            text: crate::scene::Color::rgb(0x123456),
            ..ThemePolicy::default()
        };
        for direction in ["TB", "BT", "LR", "RL"] {
            for large in [false, true] {
                let (authority, control) = setup();
                let mut policy = LayoutPolicy::default();
                if large {
                    policy.padding = 47.0;
                    policy.font.em_size = 36.0;
                }
                let source = format!("stateDiagram-v2\ndirection {direction}\n[*] --> [*]");
                let original = source.clone();
                let scene = render_state_native_controlled(&source, policy, theme, &Font, &control)
                    .unwrap();
                let circles: Vec<_> = scene
                    .svg
                    .split("<path ")
                    .skip(1)
                    .filter(|path| {
                        path.split_once(" d=\"")
                            .is_some_and(|(_, d)| d.contains('C'))
                    })
                    .collect();
                assert_eq!(circles.len(), 3, "initial, final outer and final inner");
                assert!(circles[0].starts_with("fill=\"#123456\" stroke=\"#123456\""));
                assert!(circles[1].starts_with("fill=\"#f1f3ff\" stroke=\"#515a85\""));
                assert!(circles[2].starts_with("fill=\"#123456\" stroke=\"none\""));
                let initial = path_bounds(circles[0]);
                let outer = path_bounds(circles[1]);
                let inner = path_bounds(circles[2]);
                assert_extent(initial, 14.0);
                assert_extent(outer, 14.0);
                assert_extent(inner, 5.0);
                assert!((outer.right - inner.right - 4.5).abs() < 1e-6);
                assert!((outer.left + outer.right - inner.left - inner.right).abs() < 1e-6);
                assert!((outer.top + outer.bottom - inner.top - inner.bottom).abs() < 1e-6);
                assert_eq!(source, original);
                drop(circles);
                drop((scene, control));
                assert_eq!(authority.used.load(Ordering::SeqCst), 0);
            }
        }
    }

    #[test]
    fn ordinary_flowchart_doublecircle_keeps_its_compound_fill() {
        let (authority, control) = setup();
        let theme = ThemePolicy {
            text: crate::scene::Color::rgb(0x123456),
            ..ThemePolicy::default()
        };
        let input = merman_core::parse_flowchart_native_controlled(
            "flowchart TB\nF(((\"\")))",
            &NativeParsePolicy::default(),
            &control,
        )
        .unwrap();
        let flow = crate::layout::render_flowchart_native_controlled(
            &input,
            LayoutPolicy::default(),
            theme,
            &Font,
            &control,
        )
        .unwrap();
        assert!(flow.svg.split("<path ").skip(1).any(|path| {
            path.starts_with("fill=\"#f1f3ff\" stroke=\"#515a85\"")
                && path
                    .split_once(" d=\"")
                    .is_some_and(|(_, d)| d.matches('C').count() == 8)
        }));
        assert!(!flow.svg.split("<path ").skip(1).any(|path| {
            path.starts_with("fill=\"#123456\" stroke=\"none\"")
                && path
                    .split_once(" d=\"")
                    .is_some_and(|(_, d)| d.starts_with('M'))
        }));
        drop((flow, input, control));
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn composite_scopes_distinguish_local_names_and_initial_and_final_nodes() {
        let (authority, control) = setup();
        let source = "stateDiagram-v2\nstate Outer {\n[*] --> A\nA --> [*]\nstate Inner {\ndirection RL\n[*] --> A\nA --> [*]\n}\nA --> Inner\n}\nstate Other {\n[*] --> A\nA --> [*]\n}\nOuter --> Other";
        let state = State::parse(source, &control).unwrap();
        assert_eq!(state.groups.len(), 3);
        assert_eq!(state.nodes.iter().filter(|node| node.id == "A").count(), 3);
        assert_eq!(
            state
                .nodes
                .iter()
                .filter(|node| node.kind == Kind::Start)
                .count(),
            3
        );
        assert_eq!(
            state
                .nodes
                .iter()
                .filter(|node| node.kind == Kind::Final)
                .count(),
            3
        );
        for edge in &state.edges {
            assert_ne!(state.nodes[edge.from].kind, Kind::Final);
            assert_ne!(state.nodes[edge.to].kind, Kind::Start);
        }
        let translated = state.flowchart(ThemePolicy::default()).unwrap();
        let input = merman_core::parse_flowchart_native_controlled(
            &translated,
            &NativeParsePolicy {
                inherit_dir: true,
                html_labels: false,
            },
            &control,
        )
        .unwrap();
        assert_eq!(input.0.subgraphs.len(), 3);
        assert_eq!(input.0.edges.len(), state.edges.len());
        assert!(
            input.0.nodes.iter().enumerate().any(|(parsed, node)| {
                node.id
                    .strip_prefix('s')
                    .and_then(|id| id.parse::<usize>().ok())
                    .is_some_and(|authored| {
                        parsed != authored
                            && matches!(state.nodes[authored].kind, Kind::Start | Kind::Final)
                    })
            }),
            "fixture must reorder an actual marker between authored and parsed indexes"
        );
        assert!(
            input
                .0
                .subgraphs
                .iter()
                .any(|group| group.dir.as_deref() == Some("RL"))
        );
        let scene = render_state_native_controlled(
            source,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &control,
        )
        .unwrap();
        let mut outer_markers = 0;
        let mut inner_disks = 0;
        let mut ordinary_states = 0;
        for path in scene.svg.split("<path ").skip(1).filter(|path| {
            path.split_once(" d=\"")
                .is_some_and(|(_, d)| d.contains('C'))
        }) {
            let bounds = path_bounds(path);
            let width = bounds.right - bounds.left;
            if (width - 14.0).abs() < 1e-6 {
                assert_extent(bounds, 14.0);
                outer_markers += 1;
            } else if (width - 5.0).abs() < 1e-6 {
                assert_extent(bounds, 5.0);
                assert!(path.starts_with("fill=\"#202432\" stroke=\"none\""));
                inner_disks += 1;
            } else {
                assert!(width >= 40.0);
                ordinary_states += 1;
            }
        }
        assert_eq!((outer_markers, inner_disks, ordinary_states), (6, 3, 3));
        drop(scene);
        drop((input, translated, state));
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn all_four_directions_and_nested_override_are_preserved() {
        for direction in ["TB", "BT", "LR", "RL"] {
            let source = format!(
                "stateDiagram\ndirection {direction}\nstate Box {{\ndirection RL\na --> b\nstate Nested {{\nx --> y\n}}\n}}\nBox --> z"
            );
            let (authority, control) = setup();
            let state = State::parse(&source, &control).unwrap();
            let translated = state.flowchart(ThemePolicy::default()).unwrap();
            let input = merman_core::parse_flowchart_native_controlled(
                &translated,
                &NativeParsePolicy {
                    inherit_dir: true,
                    html_labels: false,
                },
                &control,
            )
            .unwrap();
            assert_eq!(input.0.direction.as_deref(), Some(direction));
            assert!(
                input
                    .0
                    .subgraphs
                    .iter()
                    .all(|group| group.dir.as_deref() == Some("RL"))
            );
            drop((input, translated, state));
            let scene = render_state_native_controlled(
                &source,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &Font,
                &control,
            )
            .unwrap();
            drop((scene, control));
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn incomplete_or_ambiguous_syntax_is_invalid_and_features_are_explicitly_unsupported() {
        for source in [
            "",
            "stateDiagram-v2",
            "stateDiagram-v2\n}",
            "stateDiagram-v2\nstate A {\nB",
            "stateDiagram-v2\nA -->",
            "stateDiagram-v2\nA --> B --> C",
            "stateDiagram-v2\nstate \"missing quote as A",
            "stateDiagram-v2\nstate \"x\" as A trailing",
            "stateDiagram-v2\ndirection NO\nA",
            "stateDiagram-v2\nstate A\nstate A",
            "stateDiagram-v2\nstate A {\nB\n}\nstate A {\nC\n}",
            "stateDiagram-v2\nA --> B; C --> D",
            "stateDiagram-v2\nA\ndirection LR junk",
        ] {
            let (authority, control) = setup();
            assert!(
                matches!(
                    State::parse(source, &control),
                    Err(DiagramError::InvalidInput)
                ),
                "{source}"
            );
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
        for source in [
            "stateDiagram-v2\nstate F <<fork>>",
            "stateDiagram-v2\nstate J <<join>>",
            "stateDiagram-v2\nstate C <<unknown>>",
            "stateDiagram-v2\nA\nnote right of A: note",
            "stateDiagram-v2\nstate A {\nB\n--\nC\n}",
            "stateDiagram-v2\nclassDef foo fill:red\nA",
            "stateDiagram-v2\nA:::foo --> B",
            "stateDiagram-v2\nA --> B:::foo",
            "stateDiagram-v2\n%%{init: {}}%%\nA",
            "---\ntitle: x\n---\nstateDiagram\nA",
            "stateDiagram-v2\nstate \"Ready<b>Now</b>\" as R",
            "stateDiagram-v2\nA --> B: <img src='x'>",
        ] {
            let (authority, control) = setup();
            assert!(
                matches!(
                    State::parse(source, &control),
                    Err(DiagramError::Unsupported)
                ),
                "{source}"
            );
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn authored_and_translated_limits_are_checked_before_growth() {
        let nodes = format!(
            "stateDiagram-v2\n{}",
            (0..129).map(|n| format!("N{n}\n")).collect::<String>()
        );
        let edges = format!("stateDiagram-v2\n{}", "A --> B\n".repeat(257));
        let groups = format!(
            "stateDiagram-v2\n{}",
            (0..33)
                .map(|n| format!("state G{n} {{\nN\n}}\n"))
                .collect::<String>()
        );
        let nesting = format!(
            "stateDiagram-v2\n{}A\n{}",
            (0..9)
                .map(|n| format!("state G{n} {{\n"))
                .collect::<String>(),
            "}\n".repeat(9)
        );
        let bytes = format!("stateDiagram-v2\nA: {}", "x".repeat(SOURCE_LIMIT));
        for source in [&nodes, &edges, &groups, &nesting, &bytes] {
            let (authority, control) = setup();
            assert!(matches!(
                State::parse(source, &control),
                Err(DiagramError::WorkLimited)
            ));
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
        let expanded = format!("stateDiagram-v2\nA: {}", "#".repeat(5000));
        let (authority, control) = setup();
        let state = State::parse(&expanded, &control).unwrap();
        assert!(matches!(
            state.flowchart(ThemePolicy::default()),
            Err(DiagramError::WorkLimited)
        ));
        drop(state);
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancellation_deadline_and_work_failure_release_all_receipts() {
        let source = "stateDiagram-v2\n[*] --> A\nA --> [*]";
        let (authority, control) = setup();
        control.cancel();
        assert!(matches!(
            State::parse(source, &control),
            Err(DiagramError::Cancelled)
        ));
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);

        let (authority, control) = setup();
        let control = control.with_absolute_deadline(Instant::now() - Duration::from_secs(1));
        assert!(matches!(
            State::parse(source, &control),
            Err(DiagramError::Deadline)
        ));
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);

        let (authority, control) = setup();
        authority.work_limit.store(8, Ordering::SeqCst);
        assert!(matches!(
            State::parse(source, &control),
            Err(DiagramError::Memory(MemoryError::WorkLimited))
        ));
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn every_parser_and_translation_allocation_failure_is_released() {
        const SOURCE: &str = "stateDiagram-v2\nstate \"準備<br/>Ready #35; &amp;\" as R\nstate Composite {\n[*] --> A\nA --> [*]\n}\nR --> Composite: enter #quot;now#quot;";
        let (authority, control) = setup();
        let state = State::parse(SOURCE, &control).unwrap();
        let translated = state.flowchart(ThemePolicy::default()).unwrap();
        let sites = authority.allocations.load(Ordering::SeqCst);
        drop((translated, state));
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert!(sites > 0);
        for site in 1..=sites {
            let (authority, control) = setup();
            authority.fail_at.store(site, Ordering::SeqCst);
            let result = State::parse(SOURCE, &control)
                .and_then(|state| state.flowchart(ThemePolicy::default()));
            assert!(
                matches!(result, Err(DiagramError::Memory(MemoryError::OutOfMemory))),
                "site {site}"
            );
            drop((result, control));
            assert_eq!(authority.used.load(Ordering::SeqCst), 0, "site {site}");
        }
    }

    #[test]
    fn every_full_state_render_allocation_failure_retires_metadata_layout_and_scene() {
        const SOURCE: &str = "stateDiagram-v2\n[*] --> Ready\nstate Composite {\n[*] --> A\nA --> [*]\n}\nReady --> Composite: enter\nComposite --> [*]";
        let source = SOURCE.to_owned();
        let (authority, control) = setup();
        let scene = render_state_native_controlled(
            &source,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &control,
        )
        .unwrap();
        let sites = authority.allocations.load(Ordering::SeqCst);
        assert!(scene.paths > 0);
        drop((scene, control));
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert!(sites > 0);
        for site in 1..=sites {
            let (authority, control) = setup();
            authority.fail_at.store(site, Ordering::SeqCst);
            let result = render_state_native_controlled(
                &source,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &Font,
                &control,
            );
            assert!(
                matches!(result, Err(DiagramError::Memory(MemoryError::OutOfMemory))),
                "full render allocation {site}/{sites}"
            );
            assert_eq!(source, SOURCE);
            drop((result, control));
            assert_eq!(
                authority.used.load(Ordering::SeqCst),
                0,
                "full render allocation {site}/{sites}"
            );
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Interrupt {
        Cancel,
        Deadline,
        Work,
    }

    struct InterruptFont<'a> {
        authority: &'a Authority,
        control: &'a OperationControl,
        action: Interrupt,
        during_paint: bool,
        triggered: Cell<bool>,
    }

    impl InterruptFont<'_> {
        fn interrupt(&self, during_paint: bool) {
            if self.during_paint != during_paint || self.triggered.replace(true) {
                return;
            }
            match self.action {
                Interrupt::Cancel => self.control.cancel(),
                Interrupt::Deadline => {
                    self.control
                        .clone()
                        .with_absolute_deadline(Instant::now() - Duration::from_secs(1));
                }
                Interrupt::Work => {
                    self.authority
                        .work_limit
                        .store(self.authority.work.load(Ordering::SeqCst), Ordering::SeqCst);
                }
            }
        }
    }

    impl OutlineProvider for InterruptFont<'_> {
        fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
            self.measure_spans(&[run], TextDirection::Auto)
        }
        fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
            self.outline_spans(&[run], TextDirection::Auto, sink)
        }
    }

    impl RichTextProvider for InterruptFont<'_> {
        fn measure_spans(
            &self,
            runs: &[TextRun<'_>],
            direction: TextDirection,
        ) -> Result<RunMetrics, CanonicalError> {
            self.interrupt(false);
            Font.measure_spans(runs, direction)
        }
        fn outline_spans(
            &self,
            runs: &[TextRun<'_>],
            direction: TextDirection,
            sink: &mut dyn PathSink,
        ) -> Result<(), CanonicalError> {
            self.interrupt(true);
            Font.outline_spans(runs, direction, sink)
        }
    }

    #[test]
    fn full_state_render_preserves_cancellation_deadline_and_work_identity_after_metadata() {
        const SOURCE: &str = "stateDiagram-v2\n[*] --> Ready\nReady --> [*]";
        for during_paint in [false, true] {
            for action in [Interrupt::Cancel, Interrupt::Deadline, Interrupt::Work] {
                let (authority, control) = setup();
                let provider = InterruptFont {
                    authority: &authority,
                    control: &control,
                    action,
                    during_paint,
                    triggered: Cell::new(false),
                };
                let result = render_state_native_controlled(
                    SOURCE,
                    LayoutPolicy::default(),
                    ThemePolicy::default(),
                    &provider,
                    &control,
                );
                assert!(provider.triggered.get(), "{action:?}, paint={during_paint}");
                assert!(
                    matches!(
                        (&action, &result),
                        (Interrupt::Cancel, Err(DiagramError::Cancelled))
                            | (Interrupt::Deadline, Err(DiagramError::Deadline))
                            | (
                                Interrupt::Work,
                                Err(DiagramError::Memory(MemoryError::WorkLimited))
                            )
                    ),
                    "{action:?}, paint={during_paint}: {:?}",
                    result.err()
                );
                drop(provider);
                drop(control);
                assert_eq!(authority.used.load(Ordering::SeqCst), 0);
            }
        }
        let (authority, control) = setup();
        control.cancel();
        let allocations = authority.allocations.load(Ordering::SeqCst);
        assert!(matches!(
            render_state_native_controlled(
                SOURCE,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &Font,
                &control,
            ),
            Err(DiagramError::Cancelled)
        ));
        assert_eq!(authority.allocations.load(Ordering::SeqCst), allocations);
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}
