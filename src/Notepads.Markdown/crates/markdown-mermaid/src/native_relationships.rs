// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::{
    DiagramError,
    layout::{
        LayoutPolicy, RelationshipEdge, RelationshipEnds, RelationshipStyle, RelationshipTerminal,
        ThemePolicy, render_flowchart_with_relationship_style,
    },
    native_common::{
        FlowchartEncoder, canonical, check, decode_label, decode_label_text, quoted_prefix, scope,
    },
    rich_text::RichTextProvider,
    scene::OwnedScene,
};
use markdown_memory::{Family, FundedString, FundedVec, MemoryError, MemoryScope};
use merman_core::{
    NativeParseError, NativeParsePolicy, OperationControl, parse_flowchart_native_controlled,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipDiagram {
    Class,
    EntityRelationship,
}

struct Node<'a> {
    id: &'a str,
    label: &'a str,
    members: FundedVec<&'a str>,
}

struct Edge<'a> {
    from: usize,
    to: usize,
    start_label: Option<&'a str>,
    end_label: Option<&'a str>,
    label: &'a str,
    dashed: bool,
    terminals: RelationshipEnds,
}

struct Graph<'a, 'c> {
    diagram: RelationshipDiagram,
    scope: MemoryScope,
    control: &'c OperationControl,
    direction: &'static str,
    nodes: FundedVec<Node<'a>>,
    edges: FundedVec<Edge<'a>>,
    body: Option<usize>,
}

pub fn render_relationship_native_controlled(
    source: &str,
    diagram: RelationshipDiagram,
    policy: LayoutPolicy,
    theme: ThemePolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<OwnedScene, DiagramError> {
    let graph = Graph::parse(source, diagram, control)?;
    let mut output = FlowchartEncoder::new(&graph.scope, control);
    output.put("flowchart ")?;
    output.put(graph.direction)?;
    output.put("\n")?;
    let mut header_ends = FundedVec::new_in(graph.scope.clone(), Family::Graph);
    for (index, node) in graph.nodes.iter().enumerate() {
        output.id("r", index)?;
        output.put("[\"")?;
        let header_end = encode_label(&mut output, node.label, &graph.scope, control)?;
        header_ends.try_push((!node.members.is_empty()).then_some(header_end))?;
        for member in &node.members {
            output.put("<br/>")?;
            encode_label(&mut output, member, &graph.scope, control)?;
        }
        output.put("\"]\n")?;
    }
    let mut edges = FundedVec::new_in(graph.scope.clone(), Family::Graph);
    for edge in &graph.edges {
        check(control, 1)?;
        output.id("r", edge.from)?;
        output.put(if edge.dashed { " -.-" } else { " ---" })?;
        if !edge.label.is_empty() {
            output.put("|\"")?;
            encode_label(&mut output, edge.label, &graph.scope, control)?;
            output.put("\"|")?;
        }
        output.put(" ")?;
        output.id("r", edge.to)?;
        output.put("\n")?;
        edges.try_push(RelationshipEdge {
            ends: edge.terminals,
            multiplicities: [
                encode_multiplicity(edge.start_label, &graph.scope, control)?,
                encode_multiplicity(edge.end_label, &graph.scope, control)?,
            ],
        })?;
    }
    let output = output.finish();
    let model =
        parse_flowchart_native_controlled(output.as_str(), &NativeParsePolicy::default(), control)
            .map_err(|error| match error {
                NativeParseError::Memory(error) => DiagramError::Memory(error),
                NativeParseError::Cancelled => {
                    check(control, 0).err().unwrap_or(DiagramError::Cancelled)
                }
                NativeParseError::Unsupported { .. } => DiagramError::Unsupported,
                NativeParseError::Syntax { .. } | NativeParseError::Model => DiagramError::Backend,
            })?;

    if model.0.edges.len() != graph.edges.len() || model.0.nodes.len() != graph.nodes.len() {
        return Err(DiagramError::Backend);
    }
    for (index, node) in model.0.nodes.iter().enumerate() {
        check(control, node.id.len() + 1)?;
        if generated_id(&node.id) != Some(index) {
            return Err(DiagramError::Backend);
        }
    }
    for (received, expected) in model.0.edges.iter().zip(graph.edges.iter()) {
        check(control, received.from.len() + received.to.len() + 1)?;
        if generated_id(&received.from) != Some(expected.from)
            || generated_id(&received.to) != Some(expected.to)
        {
            return Err(DiagramError::Backend);
        }
    }
    render_flowchart_with_relationship_style(
        &model,
        &[],
        RelationshipStyle {
            header_ends: &header_ends,
            edges: &edges,
        },
        LayoutPolicy {
            // A routed edge needs a straight segment through the optional
            // cardinality circle and the longer UML diamond at either end.
            clearance: policy.clearance.max(20.0),
            rank_gap: policy.rank_gap.max(64.0),
            ..policy
        },
        theme,
        provider,
        control,
    )
    .map_err(|error| canonical(error, control))
}

fn encode_label(
    output: &mut FlowchartEncoder<'_>,
    raw: &str,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<usize, DiagramError> {
    let normalized = decode_label(raw, scope)?;
    let label = decode_label_text(normalized.as_str(), scope, control, false)?;
    let mut breaks = 0;
    for (index, line) in label.split('\n').enumerate() {
        breaks = index;
        if index != 0 {
            output.put("<br/>")?;
        }
        output.label(line)?;
    }
    Ok(breaks)
}

fn encode_multiplicity(
    raw: Option<&str>,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<Option<FundedString>, DiagramError> {
    raw.map(|value| {
        if value.len() > 128 {
            return limited(scope);
        }
        let mut encoder = FlowchartEncoder::new(scope, control);
        encode_label(&mut encoder, value, scope, control)?;
        Ok(encoder.finish())
    })
    .transpose()
}

fn generated_id(id: &str) -> Option<usize> {
    id.strip_prefix('r')?.parse().ok()
}

fn limited<T>(scope: &MemoryScope) -> Result<T, DiagramError> {
    scope.record_failure(MemoryError::WorkLimited);
    Err(DiagramError::WorkLimited)
}

impl<'a, 'c> Graph<'a, 'c> {
    fn parse(
        source: &'a str,
        diagram: RelationshipDiagram,
        control: &'c OperationControl,
    ) -> Result<Self, DiagramError> {
        let scope = scope(control)?;
        if source.len() > 16 * 1024 {
            return limited(&scope);
        }
        let mut graph = Self {
            diagram,
            control,
            direction: "TB",
            nodes: FundedVec::new_in(scope.clone(), Family::Parse),
            edges: FundedVec::new_in(scope.clone(), Family::Parse),
            scope,
            body: None,
        };
        let mut header = false;
        for line in source.trim_start_matches('\u{feff}').lines() {
            check(control, line.len() + 1)?;
            let line = line.trim();
            if line.starts_with("%%{") {
                return Err(DiagramError::Unsupported);
            }
            if line.is_empty() || line.starts_with("%%") {
                continue;
            }
            if !header {
                header = match diagram {
                    RelationshipDiagram::Class => {
                        matches!(line, "classDiagram" | "classDiagram-v2")
                    }
                    RelationshipDiagram::EntityRelationship => line == "erDiagram",
                };
                if !header {
                    return Err(DiagramError::InvalidInput);
                }
                continue;
            }
            graph.statement(line.trim_end_matches(';').trim_end())?;
        }
        if !header || graph.nodes.is_empty() || graph.body.is_some() {
            return Err(DiagramError::InvalidInput);
        }
        Ok(graph)
    }

    fn statement(&mut self, line: &'a str) -> Result<(), DiagramError> {
        if let Some(index) = self.body {
            let (member, close) = line
                .strip_suffix('}')
                .map_or((line, false), |line| (line, true));
            if !member.trim().is_empty() {
                self.member(index, member.trim())?;
            }
            if close {
                self.body = None;
            }
            return Ok(());
        }
        if let Some(direction) = keyword(line, "direction") {
            self.direction = match direction {
                "TB" | "TD" => "TB",
                "BT" => "BT",
                "LR" => "LR",
                "RL" => "RL",
                _ => return Err(DiagramError::InvalidInput),
            };
            return Ok(());
        }
        let declaration = (self.diagram == RelationshipDiagram::Class)
            .then(|| keyword(line, "class"))
            .flatten();
        let (name, rest) = self.identifier(declaration.unwrap_or(line))?;
        let from = self.node(name)?;
        if declaration.is_some() || rest.trim().is_empty() || rest.trim_start().starts_with('[') {
            return self.declaration(from, rest.trim());
        }
        if self.diagram == RelationshipDiagram::Class
            && let Some(member) = rest.trim_start().strip_prefix(':')
        {
            return self.member(from, member.trim());
        }
        if self.diagram == RelationshipDiagram::EntityRelationship
            && rest.trim_start().starts_with('{')
        {
            return self.declaration(from, rest.trim());
        }
        self.relationship(from, rest.trim())
    }

    fn declaration(&mut self, index: usize, mut rest: &'a str) -> Result<(), DiagramError> {
        if let Some(alias) = rest.strip_prefix('[') {
            let alias = alias.trim_start();
            let (label, after) = match quoted_prefix(alias)? {
                (Some(label), after) => (
                    label,
                    after.strip_prefix(']').ok_or(DiagramError::InvalidInput)?,
                ),
                (None, _) => alias.split_once(']').ok_or(DiagramError::InvalidInput)?,
            };
            if label.trim().is_empty() {
                return Err(DiagramError::InvalidInput);
            }
            self.nodes[index].label = label.trim();
            rest = after.trim_start();
        }
        if rest.is_empty() {
            return Ok(());
        }
        let body = rest
            .strip_prefix('{')
            .ok_or(DiagramError::Unsupported)?
            .trim();
        if let Some(member) = body.strip_suffix('}') {
            if !member.trim().is_empty() {
                self.member(index, member.trim())?;
            }
        } else if body.is_empty() {
            self.body = Some(index);
        } else {
            return Err(DiagramError::InvalidInput);
        }
        Ok(())
    }

    fn node(&mut self, id: &'a str) -> Result<usize, DiagramError> {
        if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
            return Err(DiagramError::InvalidInput);
        }
        for (index, node) in self.nodes.iter().enumerate() {
            check(self.control, node.id.len().min(id.len()) + 1)?;
            if node.id == id {
                return Ok(index);
            }
        }
        if self.nodes.len() == 128 {
            return limited(&self.scope);
        }
        let index = self.nodes.len();
        self.nodes.try_push(Node {
            id,
            label: id,
            members: FundedVec::new_in(self.scope.clone(), Family::Parse),
        })?;
        Ok(index)
    }

    fn member(&mut self, index: usize, member: &'a str) -> Result<(), DiagramError> {
        if member.is_empty() || member.contains('{') || member.len() > 512 {
            return Err(DiagramError::InvalidInput);
        }
        let rows = &mut self.nodes[index].members;
        if rows.len() == 32 {
            return limited(&self.scope);
        }
        rows.try_push(member)?;
        Ok(())
    }

    fn relationship(&mut self, from: usize, rest: &'a str) -> Result<(), DiagramError> {
        let (start_label, rest) = if self.diagram == RelationshipDiagram::Class {
            quoted_prefix(rest)?
        } else {
            (None, rest)
        };
        let (terminals, dashed, rest) = match self.diagram {
            RelationshipDiagram::Class => class_connector(rest),
            RelationshipDiagram::EntityRelationship => entity_connector(rest),
        }
        .ok_or(DiagramError::Unsupported)?;
        let (end_label, rest) = if self.diagram == RelationshipDiagram::Class {
            quoted_prefix(rest)?
        } else {
            (None, rest)
        };
        let (name, rest) = self.identifier(rest)?;
        let to = self.node(name)?;
        let label = match rest.trim() {
            "" if self.diagram == RelationshipDiagram::Class => "",
            rest => rest
                .strip_prefix(':')
                .ok_or(DiagramError::InvalidInput)?
                .trim(),
        };
        if self.diagram == RelationshipDiagram::EntityRelationship && label.is_empty() {
            return Err(DiagramError::InvalidInput);
        }
        if self.edges.len() == 256 {
            return limited(&self.scope);
        }
        self.edges.try_push(Edge {
            from,
            to,
            start_label,
            end_label,
            label,
            dashed,
            terminals,
        })?;
        Ok(())
    }

    fn identifier(&self, raw: &'a str) -> Result<(&'a str, &'a str), DiagramError> {
        if self.diagram == RelationshipDiagram::EntityRelationship
            && let (Some(quoted), rest) = quoted_prefix(raw)?
        {
            return Ok((&quoted[1..quoted.len() - 1], rest));
        }
        identifier(raw).ok_or(DiagramError::InvalidInput)
    }
}

fn keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

fn identifier(raw: &str) -> Option<(&str, &str)> {
    let raw = raw.trim_start();
    if let Some(raw) = raw.strip_prefix('`') {
        return raw.split_once('`');
    }
    let end = raw
        .find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | '$' | '.')))
        .unwrap_or(raw.len());
    // A dash or dot that belongs to an adjacent relationship closes the ID.
    let end = raw[..end]
        .find("--")
        .into_iter()
        .chain(raw[..end].find(".."))
        .min()
        .unwrap_or(end);
    (end != 0).then(|| raw.split_at(end))
}

fn class_connector(raw: &str) -> Option<(RelationshipEnds, bool, &str)> {
    use RelationshipTerminal::*;
    let (start, raw) = [
        ("<|", Triangle),
        ("<", Arrow),
        ("*", Diamond),
        ("o", OpenDiamond),
    ]
    .into_iter()
    .find_map(|(symbol, terminal)| raw.strip_prefix(symbol).map(|rest| (terminal, rest)))
    .unwrap_or((None, raw));
    let (dashed, raw) = relationship_stroke(raw)?;
    let (end, rest) = [
        ("|>", Triangle),
        (">", Arrow),
        ("*", Diamond),
        ("o", OpenDiamond),
    ]
    .into_iter()
    .find_map(|(symbol, terminal)| {
        let rest = raw.strip_prefix(symbol)?;
        (symbol != "o" || rest.is_empty() || rest.starts_with(char::is_whitespace))
            .then_some((terminal, rest))
    })
    .unwrap_or((None, raw));
    Some((RelationshipEnds { start, end }, dashed, rest.trim_start()))
}

fn entity_connector(raw: &str) -> Option<(RelationshipEnds, bool, &str)> {
    use RelationshipTerminal::*;
    let (start, raw) = er_cardinality(
        raw,
        [
            ("||", ExactlyOne),
            ("|o", OptionalOne),
            ("}|", AtLeastOne),
            ("}o", OptionalMany),
        ],
    )?;
    let (dashed, raw) = relationship_stroke(raw.trim_start()).or_else(|| {
        let raw = raw.trim_start();
        keyword(raw, "optionally to")
            .map(|rest| (true, rest))
            .or_else(|| keyword(raw, "to").map(|rest| (false, rest)))
    })?;
    let (end, rest) = er_cardinality(
        raw.trim_start(),
        [
            ("||", ExactlyOne),
            ("o|", OptionalOne),
            ("|{", AtLeastOne),
            ("o{", OptionalMany),
        ],
    )?;
    Some((RelationshipEnds { start, end }, dashed, rest.trim_start()))
}

fn er_cardinality<'a>(
    raw: &'a str,
    symbols: [(&str, RelationshipTerminal); 4],
) -> Option<(RelationshipTerminal, &'a str)> {
    use RelationshipTerminal::*;
    symbols
        .into_iter()
        .find_map(|(symbol, terminal)| raw.strip_prefix(symbol).map(|rest| (terminal, rest)))
        .or_else(|| {
            [
                ("one or zero", OptionalOne),
                ("zero or one", OptionalOne),
                ("one or more", AtLeastOne),
                ("one or many", AtLeastOne),
                ("many(1)", AtLeastOne),
                ("1+", AtLeastOne),
                ("zero or more", OptionalMany),
                ("zero or many", OptionalMany),
                ("many(0)", OptionalMany),
                ("0+", OptionalMany),
                ("only one", ExactlyOne),
                ("1", ExactlyOne),
            ]
            .into_iter()
            .find_map(|(alias, terminal)| keyword(raw, alias).map(|rest| (terminal, rest)))
        })
}

fn relationship_stroke(raw: &str) -> Option<(bool, &str)> {
    raw.strip_prefix("--")
        .map(|rest| (false, rest))
        .or_else(|| raw.strip_prefix("..").map(|rest| (true, rest)))
}
