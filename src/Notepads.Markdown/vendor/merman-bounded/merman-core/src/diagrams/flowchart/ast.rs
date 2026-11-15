// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use super::LexError;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use super::{Edge, Node, SubgraphHeader};
use crate::{EditorExpectedSyntax, SourceSpan};

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(
    not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")),
    allow(
        dead_code,
        reason = "Agentflow shares presentation lexing but does not consume Flowchart grammar and recovery payloads."
    )
)]
pub(crate) struct FlowchartDirectiveEditorEvidence {
    expected_syntax: [Option<EditorExpectedSyntax>; 3],
}

impl FlowchartDirectiveEditorEvidence {
    pub(crate) fn new(
        directive: EditorExpectedSyntax,
        first: Option<EditorExpectedSyntax>,
        following: Option<EditorExpectedSyntax>,
    ) -> Self {
        Self {
            expected_syntax: [Some(directive), first, following],
        }
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn iter(&self) -> impl Iterator<Item = EditorExpectedSyntax> + '_ {
        self.expected_syntax.iter().flatten().copied()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FlowchartClickEditorEvidence {
    action: Option<EditorExpectedSyntax>,
    payload: Option<EditorExpectedSyntax>,
}

impl FlowchartClickEditorEvidence {
    pub(crate) fn new(action: Option<SourceSpan>, payload: Option<SourceSpan>) -> Self {
        Self {
            action: action.map(|span| {
                EditorExpectedSyntax::new(crate::EditorExpectedSyntaxKind::InteractionAction, span)
            }),
            payload: payload.map(|span| {
                EditorExpectedSyntax::new(crate::EditorExpectedSyntaxKind::Payload, span)
            }),
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = EditorExpectedSyntax> + '_ {
        [self.action, self.payload].into_iter().flatten()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StyleStmt {
    pub target: String,
    pub target_span: Option<SourceSpan>,
    pub styles: Vec<String>,
    pub styles_text: Option<String>,
    pub styles_span: Option<SourceSpan>,
    pub editor_evidence: FlowchartDirectiveEditorEvidence,
}

#[derive(Debug, Clone)]
pub(crate) struct ClassDefStmt {
    pub ids: Vec<String>,
    pub id_spans: Vec<SourceSpan>,
    pub styles: Vec<String>,
    pub styles_text: Option<String>,
    pub styles_span: Option<SourceSpan>,
    pub editor_evidence: FlowchartDirectiveEditorEvidence,
}

#[derive(Debug, Clone)]
pub(crate) struct ClassAssignStmt {
    pub targets: Vec<String>,
    pub target_spans: Vec<SourceSpan>,
    pub class_name: String,
    pub class_name_span: Option<SourceSpan>,
    pub editor_evidence: FlowchartDirectiveEditorEvidence,
}

#[derive(Debug, Clone)]
pub(crate) enum ClickAction {
    Callback,
    Link {
        href: String,
        target: Option<String>,
    },
}

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")),
    allow(
        dead_code,
        reason = "Agentflow shares presentation lexing but does not consume Flowchart grammar and recovery payloads."
    )
)]
pub(crate) struct ClickStmt {
    pub ids: Vec<String>,
    pub id_spans: Vec<SourceSpan>,
    pub tooltip: Option<String>,
    pub action: ClickAction,
    pub editor_evidence: FlowchartDirectiveEditorEvidence,
    pub interaction_evidence: FlowchartClickEditorEvidence,
    pub recovery_error: Option<LexError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkStylePos {
    Default,
    Index(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct LinkStyleStmt {
    pub positions: Vec<LinkStylePos>,
    pub interpolate: Option<String>,
    pub styles: Vec<String>,
}

#[derive(Debug)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct FlowchartAst {
    pub keyword: String,
    pub direction: Option<String>,
    pub header_span: SourceSpan,
    pub statements: StatementArena,
    pub root: StatementList,
}

#[derive(Debug, Clone)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct SubgraphBlock {
    pub header: SubgraphHeader,
    pub statements: StatementList,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) enum Stmt {
    Chain {
        node_groups: Vec<Vec<Node>>,
        edge_groups: Vec<Vec<Edge>>,
    },
    Node(StatementNode),
    Subgraph(SubgraphBlock),
    Direction(String),
    Style(StyleStmt),
    ClassDef(ClassDefStmt),
    ClassAssign(ClassAssignStmt),
    Click(ClickStmt),
    LinkStyle(LinkStyleStmt),
    ShapeData {
        target: String,
        target_span: Option<SourceSpan>,
        yaml: String,
    },
}

#[derive(Debug)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) enum StatementNode {
    Legacy(Box<Node>),
    Funded(markdown_memory::FundedBox<Node>),
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl AsRef<Node> for StatementNode {
    fn as_ref(&self) -> &Node {
        match self {
            Self::Legacy(value) => value.as_ref(),
            Self::Funded(value) => value.as_ref(),
        }
    }
}
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl std::ops::Deref for StatementNode {
    type Target = Node;
    fn deref(&self) -> &Node {
        self.as_ref()
    }
}
#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
impl Clone for StatementNode {
    fn clone(&self) -> Self {
        match self {
            Self::Legacy(node) => Self::Legacy(node.clone()),
            // The only original arena Clone test is explicitly guarded against
            // native storage. Production has no infallible Clone for this type.
            Self::Funded(_) => unreachable!("native statement clones must be fallible"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct StatementId(usize);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct StatementList {
    head: Option<StatementId>,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
struct StatementRecord {
    statement: Stmt,
    next: Option<StatementId>,
}

/// Each parser fragment owns only IDs; the arena owns every statement exactly once.
#[derive(Debug, Default)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct StatementArena {
    records: Vec<StatementRecord>,
    // Drop order: the record backing is freed before the matching receipt.
    records_owner: Option<markdown_memory::OwnerTicket>,
    construction_steps: usize,
    native_edges: usize,
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
impl Clone for StatementArena {
    fn clone(&self) -> Self {
        // Original upstream tests clone legacy unbudgeted arenas. Production
        // native arenas deliberately have no infallible Clone implementation.
        assert!(
            self.records_owner.is_none(),
            "funded arenas require an explicit fallible clone"
        );
        Self {
            records: self.records.clone(),
            records_owner: None,
            construction_steps: self.construction_steps,
            native_edges: self.native_edges,
        }
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
// Generated grammar actions retain the original lexer token and diagnostic span in errors.
#[allow(clippy::result_large_err)]
impl StatementArena {
    fn memory_failure(
        control: &crate::OperationControl,
        error: markdown_memory::MemoryError,
    ) -> lalrpop_util::ParseError<usize, super::Tok, LexError> {
        if let Some(scope) = control.memory_scope() {
            scope.record_failure(error);
        }
        control.cancel();
        lalrpop_util::ParseError::User {
            error: LexError::terminal(),
        }
    }

    pub(crate) fn owned_keyword(
        text: &str,
        control: &crate::OperationControl,
    ) -> Result<String, lalrpop_util::ParseError<usize, super::Tok, LexError>> {
        match control.memory_region() {
            Some(region) => region
                .try_string(text, markdown_memory::Family::Parse)
                .map_err(|error| Self::memory_failure(control, error)),
            None => Ok(text.to_owned()),
        }
    }

    pub(crate) fn vector_push<T>(
        mut values: Vec<T>,
        value: T,
        control: &crate::OperationControl,
    ) -> Result<Vec<T>, lalrpop_util::ParseError<usize, super::Tok, LexError>> {
        if let Some(region) = control.memory_region() {
            region
                .try_push(&mut values, value, markdown_memory::Family::Parse)
                .map_err(|error| Self::memory_failure(control, error))?;
        } else {
            values.push(value);
        }
        Ok(values)
    }

    pub(crate) fn vector_one<T>(
        value: T,
        control: &crate::OperationControl,
    ) -> Result<Vec<T>, lalrpop_util::ParseError<usize, super::Tok, LexError>> {
        Self::vector_push(Vec::new(), value, control)
    }

    pub(crate) fn vector_prepend<T>(
        first: T,
        mut rest: Vec<T>,
        control: &crate::OperationControl,
    ) -> Result<Vec<T>, lalrpop_util::ParseError<usize, super::Tok, LexError>> {
        let mut values = Vec::new();
        if let Some(region) = control.memory_region() {
            let count = rest.len().checked_add(1).ok_or_else(|| {
                Self::memory_failure(control, markdown_memory::MemoryError::ArithmeticOverflow)
            })?;
            region
                .try_reserve_vec(&mut values, count, markdown_memory::Family::Parse)
                .map_err(|error| Self::memory_failure(control, error))?;
        } else {
            values.reserve(rest.len().saturating_add(1));
        }
        values.push(first);
        values.append(&mut rest);
        Ok(values)
    }

    pub(crate) fn push_group(
        &mut self,
        nodes: Vec<Node>,
        control: &crate::OperationControl,
    ) -> Result<StatementId, lalrpop_util::ParseError<usize, super::Tok, LexError>> {
        let node_groups = Self::vector_one(nodes, control)?;
        self.push(
            Stmt::Chain {
                node_groups,
                edge_groups: Vec::new(),
            },
            control,
        )
    }

    pub(crate) fn chain(
        &mut self,
        start: Vec<Node>,
        segments: Vec<(
            Option<String>,
            super::LinkToken,
            Option<super::LabeledText>,
            Vec<Node>,
        )>,
        control: &crate::OperationControl,
    ) -> Result<
        (Vec<Vec<Node>>, Vec<Vec<Edge>>),
        lalrpop_util::ParseError<usize, super::Tok, LexError>,
    > {
        use markdown_memory::PlanClone;
        let mut node_groups = Self::vector_one(start, control)?;
        let mut edge_groups = Vec::new();
        for (edge_id, link, label, next_group) in segments {
            let previous = &node_groups[node_groups.len() - 1];
            let fanout = previous
                .len()
                .checked_mul(next_group.len())
                .ok_or_else(|| {
                    Self::memory_failure(control, markdown_memory::MemoryError::ArithmeticOverflow)
                })?;
            let native_total = self.native_edges.checked_add(fanout).ok_or_else(|| {
                Self::memory_failure(control, markdown_memory::MemoryError::ArithmeticOverflow)
            })?;
            if control.memory_region().is_some() && native_total > 256 {
                return Err(Self::memory_failure(
                    control,
                    markdown_memory::MemoryError::BudgetLimited,
                ));
            }
            let make_id = |from: &Node, to: &Node| {
                if from.id == previous[previous.len() - 1].id && to.id == next_group[0].id {
                    edge_id.as_ref()
                } else {
                    None
                }
            };
            // Preserve the upstream construction checkpoint count and statement IDs.
            for _ in 0..fanout {
                self.construction_checkpoint(control)?;
            }
            let segment_edges = if let Some(region) = control.memory_region() {
                region
                    .scope()
                    .charge_work(fanout)
                    .map_err(|error| Self::memory_failure(control, error))?;
                region
                    .try_build_recipe(
                        markdown_memory::Family::Parse,
                        |plan| {
                            plan.array::<Edge>(fanout)?;
                            for from in previous {
                                for to in &next_group {
                                    from.id.plan_clone(plan)?;
                                    to.id.plan_clone(plan)?;
                                    make_id(from, to)
                                        .map(|value| value.plan_clone(plan))
                                        .transpose()?;
                                    link.plan_clone(plan)?;
                                    label
                                        .as_ref()
                                        .map(|value| value.text.plan_clone(plan))
                                        .transpose()?;
                                }
                            }
                            Ok(())
                        },
                        |group| {
                            let mut values = group.vector::<Edge>(fanout)?;
                            for from in previous {
                                for to in &next_group {
                                    values.push(Edge {
                                        from: from.id.clone_prepared(group)?,
                                        to: to.id.clone_prepared(group)?,
                                        id: make_id(from, to)
                                            .map(|value| value.clone_prepared(group))
                                            .transpose()?,
                                        link: link.clone_prepared(group)?,
                                        label: label
                                            .as_ref()
                                            .map(|value| value.text.clone_prepared(group))
                                            .transpose()?,
                                        label_type: label
                                            .as_ref()
                                            .map(|value| value.kind.clone())
                                            .unwrap_or(super::TitleKind::Text),
                                        label_span: label.as_ref().and_then(|value| value.span),
                                        label_selection: label
                                            .as_ref()
                                            .and_then(|value| value.selection),
                                        style: Vec::new(),
                                        classes: Vec::new(),
                                        interpolate: None,
                                        is_user_defined_id: false,
                                        animate: None,
                                        animation: None,
                                    });
                                }
                            }
                            Ok(values)
                        },
                    )
                    .map_err(|error| Self::memory_failure(control, error))?
            } else {
                let mut values = Vec::new();
                for from in previous {
                    for to in &next_group {
                        values.push(Edge {
                            from: from.id.clone(),
                            to: to.id.clone(),
                            id: make_id(from, to).cloned(),
                            link: link.clone(),
                            label: label.as_ref().map(|value| value.text.clone()),
                            label_type: label
                                .as_ref()
                                .map(|value| value.kind.clone())
                                .unwrap_or(super::TitleKind::Text),
                            label_span: label.as_ref().and_then(|value| value.span),
                            label_selection: label.as_ref().and_then(|value| value.selection),
                            style: Vec::new(),
                            classes: Vec::new(),
                            interpolate: None,
                            is_user_defined_id: false,
                            animate: None,
                            animation: None,
                        });
                    }
                }
                values
            };
            edge_groups = Self::vector_push(edge_groups, segment_edges, control)?;
            node_groups = Self::vector_push(node_groups, next_group, control)?;
            if control.memory_region().is_some() {
                self.native_edges = native_total;
            }
        }
        Ok((node_groups, edge_groups))
    }

    pub(crate) fn construction_checkpoint(
        &mut self,
        control: &crate::OperationControl,
    ) -> std::result::Result<(), lalrpop_util::ParseError<usize, super::Tok, LexError>> {
        if self.construction_steps.is_multiple_of(128) {
            control
                .checkpoint()
                .map_err(|error| lalrpop_util::ParseError::User {
                    error: if control.memory_scope().is_some() {
                        LexError::terminal()
                    } else {
                        LexError::new(error.to_string())
                    },
                })?;
        }
        self.construction_steps = self.construction_steps.saturating_add(1);
        Ok(())
    }

    fn plan_record_slot(
        &self,
        control: &crate::OperationControl,
    ) -> Result<Option<(markdown_memory::GrowthTicket, usize)>, markdown_memory::MemoryError> {
        use markdown_memory::{Family, MemoryError};
        let Some(scope) = control.memory_scope() else {
            return if self.records_owner.is_none() {
                Ok(None)
            } else {
                Err(MemoryError::InvalidState)
            };
        };
        if let Some(owner) = &self.records_owner {
            if !owner.scope().same_root(scope) {
                return Err(MemoryError::InvalidState);
            }
        } else if self.records.capacity() != 0 {
            return Err(MemoryError::Unfunded);
        }
        if self.records.len() < self.records.capacity() {
            return Ok(None);
        }
        let needed = self
            .records
            .len()
            .checked_add(1)
            .ok_or(MemoryError::ArithmeticOverflow)?;
        let capacity = needed
            .max(self.records.capacity().checked_mul(2).unwrap_or(needed))
            .max(4);
        let ticket = scope.reserve_array::<StatementRecord>(capacity, Family::Parse)?;
        Ok(Some((ticket, capacity)))
    }

    fn commit_record_slot(&mut self, prepared: Option<markdown_memory::FundedVec<StatementRecord>>) {
        if let Some(prepared) = prepared {
            let (mut replacement, new_owner) = prepared.into_parts();
            replacement.append(&mut self.records);
            let retired = std::mem::replace(&mut self.records, replacement);
            let retired_owner = std::mem::replace(&mut self.records_owner, new_owner);
            drop(retired);
            drop(retired_owner);
        }
    }

    fn prepare_record_slot(
        &mut self,
        control: &crate::OperationControl,
    ) -> Result<(), markdown_memory::MemoryError> {
        let prepared = self
            .plan_record_slot(control)?
            .map(|(ticket, capacity)| markdown_memory::FundedVec::try_with_prepared(ticket, capacity))
            .transpose()?;
        self.commit_record_slot(prepared);
        Ok(())
    }

    pub(crate) fn push_node(
        &mut self,
        node: Node,
        control: &crate::OperationControl,
    ) -> std::result::Result<StatementId, lalrpop_util::ParseError<usize, super::Tok, LexError>>
    {
        let Some(scope) = control.memory_scope() else {
            return self.push(Stmt::Node(StatementNode::Legacy(Box::new(node))), control);
        };
        self.construction_checkpoint(control)?;
        let prepared = (|| {
            // Reserve both complete layouts before materializing either block.
            let record_plan = self.plan_record_slot(control)?;
            let node_ticket = scope.reserve(
                std::alloc::Layout::new::<Node>(),
                markdown_memory::Family::Parse,
            )?;
            let records = record_plan
                .map(|(ticket, capacity)| {
                    markdown_memory::FundedVec::try_with_prepared(ticket, capacity)
                })
                .transpose()?;
            let node = markdown_memory::FundedBox::try_with_prepared(node_ticket, node)?;
            if let Some(error) = scope.failure() {
                return Err(error);
            }
            control
                .checkpoint()
                .map_err(|_| markdown_memory::MemoryError::Cancelled)?;
            Ok((records, node))
        })();
        let (records, node) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                scope.record_failure(error);
                control.cancel();
                return Err(lalrpop_util::ParseError::User {
                    error: LexError::terminal(),
                });
            }
        };
        self.commit_record_slot(records);
        let id = StatementId(self.records.len());
        self.records.push(StatementRecord {
            statement: Stmt::Node(StatementNode::Funded(node)),
            next: None,
        });
        Ok(id)
    }

    pub(crate) fn push(
        &mut self,
        statement: Stmt,
        control: &crate::OperationControl,
    ) -> std::result::Result<StatementId, lalrpop_util::ParseError<usize, super::Tok, LexError>>
    {
        self.construction_checkpoint(control)?;
        if let Err(error) = self.prepare_record_slot(control) {
            if let Some(scope) = control.memory_scope() {
                scope.record_failure(error);
            }
            control.cancel();
            return Err(lalrpop_util::ParseError::User {
                error: LexError::terminal(),
            });
        }
        let id = StatementId(self.records.len());
        self.records.push(StatementRecord {
            statement,
            next: None,
        });
        Ok(id)
    }

    /// Right-recursive reductions attach a statement without moving their suffix.
    pub(crate) fn prepend(
        &mut self,
        id: StatementId,
        rest: StatementList,
        control: &crate::OperationControl,
    ) -> std::result::Result<StatementList, lalrpop_util::ParseError<usize, super::Tok, LexError>>
    {
        self.construction_checkpoint(control)?;
        self.records[id.0].next = rest.head;
        Ok(StatementList { head: Some(id) })
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl FlowchartAst {
    pub(crate) fn walk_controlled<'a>(
        &'a self,
        control: &'a crate::OperationControl,
    ) -> crate::OperationControlResult<ControlledStatementWalk<'a>> {
        let frames = match control.memory_scope() {
            Some(scope) => {
                control.checkpoint()?;
                let mut frames =
                    markdown_memory::FundedVec::new_in(scope.clone(), markdown_memory::Family::Work);
                if let Err(error) = frames.try_push(StatementWalkFrame {
                    next: self.root.head,
                    finish: None,
                }) {
                    scope.record_failure(error);
                    control.cancel();
                    return Err(crate::OperationCancelled {
                        phase: crate::OperationPhase::Semantic,
                        reason: crate::CancelReason::Requested,
                    });
                }
                ControlledWalkFrames::Funded(frames)
            }
            None => ControlledWalkFrames::Legacy(vec![StatementWalkFrame {
                next: self.root.head,
                finish: None,
            }]),
        };
        Ok(ControlledStatementWalk {
            arena: &self.statements,
            frames,
            control,
            failed: false,
        })
    }

    pub(crate) fn walk(&self) -> StatementWalk<'_> {
        StatementWalk {
            arena: &self.statements,
            frames: vec![StatementWalkFrame {
                next: self.root.head,
                finish: None,
            }],
        }
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) enum StatementEvent<'a> {
    Enter(&'a Stmt),
    Exit(&'a SubgraphBlock),
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
struct StatementWalkFrame {
    next: Option<StatementId>,
    finish: Option<StatementId>,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct StatementWalk<'a> {
    arena: &'a StatementArena,
    frames: Vec<StatementWalkFrame>,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl<'a> Iterator for StatementWalk<'a> {
    type Item = StatementEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let frame = self.frames.last_mut()?;
            if let Some(id) = frame.next {
                let record = &self.arena.records[id.0];
                frame.next = record.next;
                if let Stmt::Subgraph(subgraph) = &record.statement {
                    self.frames.push(StatementWalkFrame {
                        next: subgraph.statements.head,
                        finish: Some(id),
                    });
                }
                return Some(StatementEvent::Enter(&record.statement));
            }
            let frame = self.frames.pop()?;
            if let Some(id) = frame.finish
                && let Stmt::Subgraph(subgraph) = &self.arena.records[id.0].statement
            {
                return Some(StatementEvent::Exit(subgraph));
            }
        }
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
enum ControlledWalkFrames {
    Legacy(Vec<StatementWalkFrame>),
    Funded(markdown_memory::FundedVec<StatementWalkFrame>),
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl ControlledWalkFrames {
    fn last_mut(&mut self) -> Option<&mut StatementWalkFrame> {
        match self {
            Self::Legacy(frames) => frames.last_mut(),
            Self::Funded(frames) => frames.as_mut_slice().last_mut(),
        }
    }
    fn pop(&mut self) -> Option<StatementWalkFrame> {
        match self {
            Self::Legacy(frames) => frames.pop(),
            Self::Funded(frames) => frames.pop(),
        }
    }
    fn try_push(&mut self, frame: StatementWalkFrame) -> Result<(), markdown_memory::MemoryError> {
        match self {
            Self::Legacy(frames) => {
                frames.push(frame);
                Ok(())
            }
            Self::Funded(frames) => frames.try_push(frame),
        }
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct ControlledStatementWalk<'a> {
    arena: &'a StatementArena,
    frames: ControlledWalkFrames,
    control: &'a crate::OperationControl,
    failed: bool,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl<'a> Iterator for ControlledStatementWalk<'a> {
    type Item = crate::OperationControlResult<StatementEvent<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        loop {
            let frame = self.frames.last_mut()?;
            if let Some(id) = frame.next {
                let record = &self.arena.records[id.0];
                frame.next = record.next;
                if let Stmt::Subgraph(subgraph) = &record.statement {
                    if let Err(error) = self.frames.try_push(StatementWalkFrame {
                        next: subgraph.statements.head,
                        finish: Some(id),
                    }) {
                        if let Some(scope) = self.control.memory_scope() {
                            scope.record_failure(error);
                        }
                        self.control.cancel();
                        self.failed = true;
                        return Some(Err(crate::OperationCancelled {
                            phase: crate::OperationPhase::Semantic,
                            reason: crate::CancelReason::Requested,
                        }));
                    }
                }
                return Some(Ok(StatementEvent::Enter(&record.statement)));
            }
            let frame = self.frames.pop()?;
            if let Some(id) = frame.finish {
                if let Stmt::Subgraph(subgraph) = &self.arena.records[id.0].statement {
                    return Some(Ok(StatementEvent::Exit(subgraph)));
                }
            }
        }
    }
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
mod tests {
    use super::*;
    use crate::{MermaidConfig, OperationControl, ParseMetadata};

    fn metadata() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    #[test]
    fn flowchart_statement_arena_keeps_wide_carrier_work_linear() {
        for width in [1, 32, 1024] {
            let mut source = String::from("flowchart TB\n");
            for index in 0..width {
                source.push_str(&format!("n{index}\n"));
            }
            let ast = super::super::parse_flowchart_ast(&source, &metadata()).unwrap();
            assert_eq!(ast.statements.records.len(), width);
            assert_eq!(ast.statements.construction_steps, width * 2);
            let ids = ast
                .walk()
                .map(|event| match event {
                    StatementEvent::Enter(Stmt::Node(node)) => node.id.clone(),
                    _ => panic!("expected a flat node statement"),
                })
                .collect::<Vec<_>>();
            assert_eq!(
                ids,
                (0..width)
                    .map(|index| format!("n{index}"))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn flowchart_statement_walker_preserves_enter_and_completion_order() {
        let source = "flowchart TB\nsubgraph outer\nA\nsubgraph inner\nB\nend\nC\nend\nD\n";
        let ast = super::super::parse_flowchart_ast(source, &metadata()).unwrap();
        let events = ast
            .walk()
            .map(|event| match event {
                StatementEvent::Enter(Stmt::Subgraph(subgraph)) => {
                    format!("enter:{}", subgraph.header.raw_id)
                }
                StatementEvent::Exit(subgraph) => format!("exit:{}", subgraph.header.raw_id),
                StatementEvent::Enter(Stmt::Node(node)) => node.id.clone(),
                _ => panic!("unexpected statement"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            [
                "enter:outer",
                "A",
                "enter:inner",
                "B",
                "exit:inner",
                "C",
                "exit:outer",
                "D"
            ]
        );
        assert_eq!(ast.statements.records.len(), 6);
        assert_eq!(ast.statements.construction_steps, 12);
    }

    #[test]
    fn flowchart_nested_parser_completion_observes_cancellation_with_flat_fragments() {
        let mut source = String::from("flowchart TB\n");
        for index in 0..512 {
            source.push_str(&format!("subgraph s{index}\n"));
        }
        source.push_str("leaf\n");
        source.push_str(&"end\n".repeat(512));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);
        let mut statements = StatementArena::default();
        let result = super::super::flowchart_grammar::FlowchartAstParser::new().parse(
            &mut statements,
            &control,
            super::super::Lexer::new(&source),
        );
        assert!(matches!(result, Err(lalrpop_util::ParseError::User { .. })));
        assert!(control.is_cancelled());
        assert_eq!(statements.construction_steps, 256);
        assert_eq!(statements.records.len(), 128);
        assert_eq!(
            statements
                .records
                .iter()
                .filter(|record| matches!(record.statement, Stmt::Subgraph(_)))
                .count(),
            127
        );
        // Both the retained arena and discarded parser symbols have flat ownership.
        drop(statements.clone());
        drop(statements);
        super::super::parse_flowchart_ast("flowchart TB\nA-->B\n", &metadata()).unwrap();
    }
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
mod funded_arena_tests {
    use super::*;
    use crate::{MermaidConfig, OperationControl, ParseMetadata};
    use markdown_memory::{AllocationAuthority, Family, MemoryError, MemoryScope};
    use std::alloc::Layout;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Authority {
        used: AtomicUsize,
        calls: AtomicUsize,
        limit: usize,
        fault: AtomicUsize,
        fail_call: usize,
    }
    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
            let result = self
                .used
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                    used.checked_add(layout.size())
                        .filter(|total| *total <= self.limit)
                })
                .map(|_| ())
                .map_err(|_| MemoryError::BudgetLimited);
            if result.is_err() {
                self.record_failure(MemoryError::BudgetLimited);
            }
            result
        }
        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }
        fn failure(&self) -> Option<MemoryError> {
            match self.fault.load(Ordering::SeqCst) {
                0 => None,
                2 => Some(MemoryError::OutOfMemory),
                _ => Some(MemoryError::BudgetLimited),
            }
        }
        fn record_failure(&self, error: MemoryError) {
            let value = if error == MemoryError::OutOfMemory {
                2
            } else {
                1
            };
            let _ = self
                .fault
                .compare_exchange(0, value, Ordering::SeqCst, Ordering::SeqCst);
        }
        fn before_allocate(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call == self.fail_call {
                self.record_failure(MemoryError::OutOfMemory);
                return Err(MemoryError::OutOfMemory);
            }
            Ok(())
        }
    }
    fn control(limit: usize) -> (Arc<Authority>, OperationControl) {
        control_failing(limit, usize::MAX)
    }
    fn control_failing(limit: usize, fail_call: usize) -> (Arc<Authority>, OperationControl) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            limit,
            fault: AtomicUsize::new(0),
            fail_call,
        });
        let scope = MemoryScope::new(authority.clone());
        (authority, OperationControl::new().with_memory_scope(scope))
    }

    #[test]
    fn arena_zero_budget_rejects_before_backing_allocation_and_without_diagnostic_string() {
        let (authority, control) = control(0);
        let mut arena = StatementArena::default();
        let error = arena
            .push(Stmt::Direction(String::new()), &control)
            .unwrap_err();
        let lalrpop_util::ParseError::User { error } = error else {
            panic!("fixed native rejection");
        };
        assert!(error.message.is_empty());
        assert!(error.expected_syntax.is_empty());
        assert!(arena.records.is_empty());
        assert_eq!(arena.records.capacity(), 0);
        assert_eq!(authority.calls.load(Ordering::SeqCst), 0);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert!(control.is_cancelled());
    }

    #[test]
    fn arena_failed_growth_preserves_ids_records_capacity_and_old_charge() {
        let block = 4 * size_of::<StatementRecord>();
        let (authority, control) = control(block + 8 * size_of::<StatementRecord>() - 1);
        let mut arena = StatementArena::default();
        for index in 0..4 {
            assert_eq!(
                arena
                    .push(Stmt::Direction(String::new()), &control)
                    .unwrap(),
                StatementId(index)
            );
        }
        assert_eq!(authority.calls.load(Ordering::SeqCst), 1);
        assert!(
            arena
                .push(Stmt::Direction(String::new()), &control)
                .is_err()
        );
        assert_eq!(arena.records.len(), 4);
        assert_eq!(arena.records.capacity(), 4);
        assert_eq!(authority.calls.load(Ordering::SeqCst), 1);
        assert_eq!(authority.used.load(Ordering::SeqCst), block);
        drop(arena);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn controlled_walk_reports_budget_failure_as_error_instead_of_successful_eof() {
        let metadata = ParseMetadata {
            diagram_type: "flowchart-v2".to_owned(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let ast=super::super::parse_flowchart_ast("flowchart TB\nsubgraph a\nsubgraph b\nsubgraph c\nsubgraph d\nsubgraph e\nX\nend\nend\nend\nend\nend\n",&metadata).unwrap();
        // The fixture AST is a legacy immutable input; only this independent
        // traversal's actual frame backing belongs to the tested scope.
        let (authority, control) = control(4 * size_of::<StatementWalkFrame>());
        let mut walk = ast.walk_controlled(&control).unwrap();
        let mut saw_error = false;
        for event in walk.by_ref() {
            if event.is_err() {
                saw_error = true;
                break;
            }
        }
        assert!(saw_error);
        assert!(walk.next().is_none());
        assert!(control.is_cancelled());
        assert_eq!(authority.calls.load(Ordering::SeqCst), 1);
        drop(walk);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
    fn empty_node() -> Node {
        Node {
            id: String::new(),
            provenance: super::super::FlowNodeProvenance::Authored,
            syntax: super::super::FlowNodeSyntax::BareReference,
            id_span: None,
            label: None,
            label_type: super::super::TitleKind::Text,
            label_span: None,
            label_selection: None,
            shape: None,
            shape_data: None,
            icon: None,
            form: None,
            pos: None,
            img: None,
            constraint: None,
            asset_width: None,
            asset_height: None,
            styles: Vec::new(),
            classes: Vec::new(),
            link: None,
            link_target: None,
            have_callback: false,
        }
    }

    #[test]
    fn node_group_budget_rejection_precedes_record_and_box_allocators() {
        let limit = 4 * size_of::<StatementRecord>() + size_of::<Node>() - 1;
        let (authority, control) = control(limit);
        let mut arena = StatementArena::default();
        assert!(arena.push_node(empty_node(), &control).is_err());
        assert_eq!(authority.calls.load(Ordering::SeqCst), 0);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert!(arena.records.is_empty());
        assert_eq!(arena.records.capacity(), 0);
    }

    #[test]
    fn node_box_second_allocation_failure_rolls_back_prepared_record_backing() {
        let (authority, control) = control_failing(usize::MAX, 2);
        let mut arena = StatementArena::default();
        assert!(arena.push_node(empty_node(), &control).is_err());
        assert_eq!(authority.failure(), Some(MemoryError::OutOfMemory));
        assert_eq!(authority.calls.load(Ordering::SeqCst), 2);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert!(arena.records.is_empty());
        assert_eq!(arena.records.capacity(), 0);
    }

    #[test]
    fn committed_native_node_keeps_both_backings_funded_until_arena_drop() {
        let total = 4 * size_of::<StatementRecord>() + size_of::<Node>();
        let (authority, control) = control(total);
        let mut arena = StatementArena::default();
        assert_eq!(
            arena.push_node(empty_node(), &control).unwrap(),
            StatementId(0)
        );
        assert!(matches!(
            arena.records[0].statement,
            Stmt::Node(StatementNode::Funded(_))
        ));
        assert_eq!(authority.used.load(Ordering::SeqCst), total);
        drop(arena);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}
