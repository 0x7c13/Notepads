// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use super::{
    FlowSubGraph, FlowchartAst, StatementEvent, Stmt, SubgraphBlock, TitleKind,
    is_ecmascript_trim_char, trim_flowdb_label_text,
};
use crate::native_collections::{
    self as memory, NativeHashMap as HashMap, NativeHashSet as HashSet,
};
use crate::{OperationControl, OperationControlResult};

#[derive(Debug, Clone)]
enum StatementItem {
    Id(String),
    Dir(String),
}

pub(super) struct SubgraphBuilder {
    sub_count: usize,
    pub(super) subgraphs: Vec<FlowSubGraph>,
    pub(super) declaration_owners: Vec<usize>,
    subgraph_index: HashMap<String, usize>,
    // Exactly the retained membership union, never candidate members or group IDs.
    claimed_members: HashSet<String>,
    inherit_dir: bool,
    global_dir: Option<String>,
}

impl SubgraphBuilder {
    #[cfg(test)]
    pub(super) fn new(inherit_dir: bool, global_dir: Option<String>) -> Self {
        Self {
            sub_count: 0,
            subgraphs: Vec::new(),
            declaration_owners: Vec::new(),
            subgraph_index: HashMap::default(),
            claimed_members: HashSet::default(),
            inherit_dir,
            global_dir,
        }
    }

    pub(super) fn new_controlled(
        inherit_dir: bool,
        global_dir: Option<String>,
        control: &OperationControl,
    ) -> Self {
        Self {
            sub_count: 0,
            subgraphs: Vec::new(),
            declaration_owners: Vec::new(),
            subgraph_index: memory::new_map(control),
            claimed_members: memory::new_set(control),
            inherit_dir,
            global_dir,
        }
    }

    pub(super) fn visit_statements(
        &mut self,
        ast: &FlowchartAst,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let mut stack: Vec<Vec<StatementItem>> = Vec::new();
        for (visited, event) in ast.walk_controlled(control)?.enumerate() {
            let event = event?;
            if visited.is_multiple_of(128) {
                control.checkpoint()?;
            }
            match event {
                StatementEvent::Enter(Stmt::Subgraph(_)) => {
                    memory::push(&mut stack, Vec::new(), control)?
                }
                StatementEvent::Enter(stmt) => {
                    if let Some(items) = stack.last_mut() {
                        push_statement_items(items, stmt, control)?;
                    }
                }
                StatementEvent::Exit(subgraph) => {
                    let items = stack.pop().unwrap_or_default();
                    let id = self.eval_subgraph_from_items(subgraph, items, control)?;
                    if let Some(parent) = stack.last_mut() {
                        memory::push(parent, StatementItem::Id(id), control)?;
                    }
                }
            }
        }
        control.checkpoint()?;
        Ok(())
    }

    fn eval_subgraph_from_items(
        &mut self,
        sg: &SubgraphBlock,
        items: Vec<StatementItem>,
        control: &OperationControl,
    ) -> OperationControlResult<String> {
        let mut seen: HashSet<String> = memory::new_set(control);
        let mut members: Vec<String> = Vec::new();
        let mut dir: Option<String> = None;

        for (index, item) in items.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            match item {
                StatementItem::Dir(d) => dir = Some(d),
                StatementItem::Id(id) => {
                    if trim_flowdb_label_text(&id).is_empty() {
                        continue;
                    }
                    if memory::insert_set(&mut seen, memory::clone_owned(&id, control)?, control)? {
                        memory::push(&mut members, id, control)?;
                    }
                }
            }
        }

        let has_explicit_dir = dir.is_some();
        let dir = if dir.is_none() && self.inherit_dir {
            memory::clone_owned(&self.global_dir, control)?
        } else {
            dir
        };

        let raw_id = sg.header.raw_id.as_str();
        let (title_raw, title_kind) =
            parse_subgraph_title_parts(&sg.header.raw_title, sg.header.id_equals_title);
        let id_raw = if raw_id.starts_with('"') && raw_id.ends_with('"') {
            // Only a double-quoted header enters Mermaid's string state. Markdown backticks are
            // meaningful after that quote has been removed; bare backticks stay in the id.
            let unquoted = if raw_id.len() >= 2 {
                &raw_id[1..raw_id.len() - 1]
            } else {
                raw_id
            };
            let without_backticks =
                if unquoted.len() >= 2 && unquoted.starts_with('`') && unquoted.ends_with('`') {
                    &unquoted[1..unquoted.len() - 1]
                } else {
                    unquoted
                };
            trim_flowdb_label_text(without_backticks)
        } else {
            raw_id
        };

        let mut id: Option<String> = {
            let trimmed = trim_flowdb_label_text(&id_raw);
            if trimmed.is_empty() {
                None
            } else {
                Some(memory::owned_string(trimmed, control)?)
            }
        };

        // Mirror Mermaid `FlowDB.addSubGraph(...)`:
        // `if (_id === _title && /\\s/.exec(_title.text)) id = undefined;`
        //
        // The important nuance is that this checks the untrimmed title token (including any
        // extra whitespace that may have been captured into the header).
        if sg.header.id_equals_title && sg.header.raw_title.chars().any(is_ecmascript_trim_char) {
            id = None;
        }

        let id = match id {
            Some(id) => id,
            None => memory::format_owned(format_args!("subGraph{}", self.sub_count), control)?,
        };
        if control.memory_region().is_some() && id.len() > 128 {
            return Err(memory::failure(
                control,
                markdown_memory::MemoryError::BudgetLimited,
            ));
        }
        let title = memory::owned_string(trim_flowdb_label_text(title_raw), control)?;
        let label_type = memory::owned_string(
            match title_kind {
                TitleKind::Text => "text",
                TitleKind::String => "string",
                TitleKind::Markdown => "markdown",
            },
            control,
        )?;

        self.sub_count += 1;

        // Mermaid 12.1 keeps the first completed declaration as the single group owner.
        // Replay still visits every declaration, so retain its canonical owner separately.
        let owner = if let Some(&index) = self.subgraph_index.get(&id) {
            index
        } else {
            if control.memory_region().is_some() && self.subgraphs.len() >= 32 {
                return Err(memory::failure(
                    control,
                    markdown_memory::MemoryError::BudgetLimited,
                ));
            }
            let index = self.subgraphs.len();
            memory::insert(
                &mut self.subgraph_index,
                memory::clone_owned(&id, control)?,
                index,
                control,
            )?;
            memory::push(
                &mut self.subgraphs,
                FlowSubGraph {
                    id: memory::clone_owned(&id, control)?,
                    nodes: Vec::new(),
                    title,
                    classes: Vec::new(),
                    styles: Vec::new(),
                    dir,
                    has_explicit_dir,
                    label_type,
                    metadata: None,
                },
                control,
            )?;
            index
        };
        for (index, member) in members.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            if member != id && !self.claimed_members.contains(member.as_str()) {
                // Update both indexes together, including on an interrupted completion.
                memory::push(
                    &mut self.subgraphs[owner].nodes,
                    memory::clone_owned(&member, control)?,
                    control,
                )?;
                memory::insert_set(&mut self.claimed_members, member, control)?;
            }
        }
        memory::push(&mut self.declaration_owners, owner, control)?;

        Ok(id)
    }
}

fn push_statement_items(
    out: &mut Vec<StatementItem>,
    stmt: &Stmt,
    control: &OperationControl,
) -> OperationControlResult<()> {
    match stmt {
        Stmt::Chain {
            node_groups,
            edge_groups,
        } => {
            // Mermaid FlowDB's subgraph membership list is based on the Jison
            // `vertexStatement.nodes` shape, which prepends the last node in a chain first
            // (e.g. `a-->b` yields `[b, a]`).
            //
            // For node-only group statements (e.g. `A & B`), there are no edges and the list
            // preserves the input order.
            if edge_groups.is_empty() {
                for (index, n) in node_groups.iter().flatten().enumerate() {
                    if index % 128 == 0 {
                        control.checkpoint()?;
                    }
                    memory::push(
                        out,
                        StatementItem::Id(memory::clone_owned(&n.id, control)?),
                        control,
                    )?;
                }
            } else {
                for (index, n) in node_groups.iter().flatten().rev().enumerate() {
                    if index % 128 == 0 {
                        control.checkpoint()?;
                    }
                    memory::push(
                        out,
                        StatementItem::Id(memory::clone_owned(&n.id, control)?),
                        control,
                    )?;
                }
            }
        }
        Stmt::Node(n) => memory::push(
            out,
            StatementItem::Id(memory::clone_owned(&n.id, control)?),
            control,
        )?,
        Stmt::Direction(d) => memory::push(
            out,
            StatementItem::Dir(memory::clone_owned(d, control)?),
            control,
        )?,
        Stmt::ShapeData { target, .. } => memory::push(
            out,
            StatementItem::Id(memory::clone_owned(target, control)?),
            control,
        )?,
        Stmt::Subgraph(_)
        | Stmt::Style(_)
        | Stmt::ClassDef(_)
        | Stmt::ClassAssign(_)
        | Stmt::Click(_)
        | Stmt::LinkStyle(_) => {}
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn subgraphs_exist(subgraphs: &[FlowSubGraph], node_id: &str) -> bool {
    subgraphs
        .iter()
        .any(|sg| sg.nodes.iter().any(|n| n == node_id))
}

fn parse_subgraph_title_parts(raw_title: &str, id_equals_title: bool) -> (&str, TitleKind) {
    let quoted = raw_title.starts_with('"') && raw_title.ends_with('"');
    let unquoted = if quoted {
        // Keep flowchart subgraph titles raw (strip only surrounding quotes).
        // This matches upstream and avoids mangling backslash-heavy labels.
        if raw_title.len() >= 2 {
            &raw_title[1..raw_title.len() - 1]
        } else {
            raw_title
        }
    } else {
        raw_title
    };

    if quoted {
        let is_markdown =
            unquoted.len() >= 2 && unquoted.starts_with('`') && unquoted.ends_with('`');
        if is_markdown {
            return (
                trim_flowdb_label_text(&unquoted[1..unquoted.len() - 1]),
                TitleKind::Markdown,
            );
        }
    }

    if !id_equals_title && quoted {
        return (trim_flowdb_label_text(unquoted), TitleKind::String);
    }

    (trim_flowdb_label_text(unquoted), TitleKind::Text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MermaidConfig, ParseMetadata};

    #[test]
    fn flowchart_claimed_members_equal_completed_canonical_membership() {
        let meta = ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let control = OperationControl::new();
        let mut builder = SubgraphBuilder::new(false, Some("TB".to_string()));
        for body in [
            "subgraph X\nX\nend\n",
            "subgraph owner\nX & A\nend\n",
            "subgraph X\nB\nend\n",
            "subgraph sibling\nX & A & B & C\nend\n",
        ] {
            let ast =
                super::super::parse_flowchart_ast(&format!("flowchart TB\n{body}"), &meta).unwrap();
            builder.visit_statements(&ast, &control).unwrap();
            let retained = builder
                .subgraphs
                .iter()
                .flat_map(|subgraph| subgraph.nodes.iter().cloned())
                .collect::<HashSet<_>>();
            assert_eq!(builder.claimed_members, retained);
        }
        assert_eq!(builder.subgraphs.len(), 3);
        assert_eq!(builder.declaration_owners, [0, 1, 0, 2]);
        assert_eq!(builder.subgraphs[0].nodes, ["B"]);
        assert_eq!(builder.subgraphs[1].nodes, ["X", "A"]);
        assert_eq!(builder.subgraphs[2].nodes, ["C"]);
    }

    #[test]
    fn flowchart_interrupted_membership_completion_keeps_only_retained_claims() {
        let meta = ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let nodes = (0..256)
            .map(|index| format!("n{index}"))
            .collect::<Vec<_>>()
            .join(" & ");
        let source = format!("flowchart TB\nsubgraph group\n{nodes}\nend\n");
        let ast = super::super::parse_flowchart_ast(&source, &meta).unwrap();
        let control = OperationControl::new();
        control.cancel_after_checkpoints(6);
        let mut builder = SubgraphBuilder::new(false, Some("TB".to_string()));
        assert!(matches!(
            builder.visit_statements(&ast, &control),
            Err(crate::OperationCancelled { .. })
        ));
        assert_eq!(builder.subgraphs.len(), 1);
        assert_eq!(builder.subgraphs[0].nodes.len(), 128);
        assert!(builder.declaration_owners.is_empty());
        assert_eq!(
            builder.claimed_members,
            builder.subgraphs[0]
                .nodes
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        );
    }
}
