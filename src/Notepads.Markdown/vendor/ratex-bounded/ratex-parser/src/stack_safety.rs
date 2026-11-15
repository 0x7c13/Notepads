// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use ratex_types::{
    execution::Phase,
    storage::{LocalShared, StorageError, StorageLedger},
};
use ratex_types::{
    execution::{ExecutionControl, ExecutionError, Resource},
    storage::{StorageFamily, WorkVec},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parse_node::{ArrayTag, ParseNode, ProofBranch};

pub(crate) const MAX_INPUT_DEPTH: usize = 32;
/// Bound AST, padded array cells, and rules after expanding bold shadow layers.
const MAX_LAYOUT_WORK: usize = 65_536;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DepthExceeded;

#[derive(Clone)]
pub(crate) struct DepthBudget {
    inner: DepthOwner,
}

#[derive(Clone)]
enum DepthOwner {
    Legacy(Arc<DepthState>),
    Bounded(LocalShared<DepthState>),
}
impl std::ops::Deref for DepthOwner {
    type Target = DepthState;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(value) => value,
            Self::Bounded(value) => value,
        }
    }
}

struct DepthState {
    current: AtomicUsize,
    limit: usize,
}

pub(crate) struct DepthGuard {
    state: DepthOwner,
}

impl DepthBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            inner: DepthOwner::Legacy(Arc::new(DepthState {
                current: AtomicUsize::new(0),
                limit,
            })),
        }
    }
    pub(crate) fn new_bounded(
        limit: usize,
        storage: &StorageLedger<'_>,
    ) -> Result<Self, StorageError> {
        Ok(Self {
            inner: DepthOwner::Bounded(LocalShared::try_new(
                storage,
                Phase::Parser,
                DepthState {
                    current: AtomicUsize::new(0),
                    limit,
                },
            )?),
        })
    }

    pub(crate) fn enter(&self) -> Result<DepthGuard, DepthExceeded> {
        loop {
            let current = self.inner.current.load(Ordering::Relaxed);
            if current >= self.inner.limit {
                return Err(DepthExceeded);
            }
            if self
                .inner
                .current
                .compare_exchange_weak(current, current + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(DepthGuard {
                    state: self.inner.clone(),
                });
            }
        }
    }

    pub(crate) fn ensure_additional(&self, additional: usize) -> Result<(), DepthExceeded> {
        if self.current().saturating_add(additional) > self.inner.limit {
            Err(DepthExceeded)
        } else {
            Ok(())
        }
    }

    pub(crate) fn current(&self) -> usize {
        self.inner.current.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub(crate) fn remaining(&self) -> usize {
        self.inner.limit.saturating_sub(self.current())
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        let old = self.state.current.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(old > 0);
    }
}

pub(crate) fn validate_parse_nodes(
    nodes: &[ParseNode],
    limit: usize,
) -> crate::error::ParseResult<()> {
    let (depth, work) = analyze_nodes(nodes);
    if depth > limit {
        return Err(crate::error::ParseError::recursion_limit_exceeded());
    }
    if work > MAX_LAYOUT_WORK {
        return Err(crate::error::ParseError::msg(
            "Layout resource limit exceeded",
        ));
    }
    Ok(())
}

pub(crate) fn parse_nodes_logical_depth(nodes: &[ParseNode]) -> usize {
    analyze_nodes(nodes).0
}

fn analyze_nodes(nodes: &[ParseNode]) -> (usize, usize) {
    analyze_nodes_impl(nodes, None)
}

pub(crate) fn validate_parse_nodes_controlled(
    nodes: &[ParseNode],
    limit: usize,
    control: &ExecutionControl<'_>,
) -> crate::error::ParseResult<()> {
    let (depth, work) = analyze_nodes_impl(nodes, Some(control));
    control
        .checkpoint(Phase::Validate)
        .map_err(crate::ParseError::control)?;
    if depth > limit {
        if let Some(storage) = control.storage() {
            return Err(crate::ParseError::storage(storage.fail(
                StorageError::DepthLimit {
                    phase: Phase::Validate,
                    limit: limit as u32,
                    requested: depth as u32,
                },
            )));
        }
        return Err(crate::ParseError::recursion_limit_exceeded());
    }
    if work > 8192 {
        return Err(crate::ParseError::control(control.fail(
            ExecutionError::ResourceLimit {
                phase: Phase::Validate,
                resource: Resource::LayoutNodes,
                limit: 8192,
                requested: work as u64,
            },
        )));
    }
    Ok(())
}

fn analyze_nodes_impl(
    nodes: &[ParseNode],
    control: Option<&ExecutionControl<'_>>,
) -> (usize, usize) {
    #[derive(Clone, Copy)]
    struct Visit<'a> {
        node: &'a ParseNode,
        depth: usize,
        suppress_group_cost: bool,
        weight: usize,
    }

    fn push_node<'a>(
        stack: &mut WorkVec<'_, Visit<'a>>,
        node: &'a ParseNode,
        depth: usize,
        suppress_group_cost: bool,
        weight: usize,
    ) {
        stack.push(Visit {
            node,
            depth,
            suppress_group_cost,
            weight,
        });
    }

    fn push_nodes<'a>(
        stack: &mut WorkVec<'_, Visit<'a>>,
        nodes: &'a [ParseNode],
        depth: usize,
        weight: usize,
    ) {
        for node in nodes.iter().rev() {
            push_node(stack, node, depth, false, weight);
        }
    }

    let mut max_depth = 0usize;
    let mut work = 0usize;
    let mut stack = WorkVec::new(control, Phase::Validate, StorageFamily::Auxiliary);
    push_nodes(&mut stack, nodes, 0, 1);

    while let Some(visit) = stack.pop() {
        let own_work = match visit.node {
            ParseNode::Verb { body, .. } => body.chars().count().max(1),
            ParseNode::MathOrd { text, .. }
            | ParseNode::TextOrd { text, .. }
            | ParseNode::Atom { text, .. }
            | ParseNode::OpToken { text, .. } => text.chars().count().max(1),
            ParseNode::Op {
                name: Some(name), ..
            } => name.chars().count().max(1),
            // Supplied cells are charged by the child AST walk. Charge missing
            // cells here too, before layout allocates the rectangular Array/CD
            // grid. Summing row deficits avoids overflowing rows * columns.
            // Both padding and horizontal rules inherit enclosing bold weight.
            ParseNode::Array {
                body,
                hlines_before_row,
                ..
            } => {
                let columns = body.iter().map(Vec::len).max().unwrap_or(0);
                let padded_work = body
                    .iter()
                    .fold(1usize, |work, row| work.saturating_add(columns - row.len()));
                hlines_before_row
                    .iter()
                    .fold(padded_work, |work, rules| work.saturating_add(rules.len()))
            }
            _ => 1,
        };
        work = work.saturating_add(visit.weight.saturating_mul(own_work));
        let child_weight =
            visit
                .weight
                .saturating_mul(if matches!(visit.node, ParseNode::Pmb { .. }) {
                    2
                } else {
                    1
                });
        max_depth = max_depth.max(visit.depth);
        match visit.node {
            ParseNode::Atom { .. }
            | ParseNode::MathOrd { .. }
            | ParseNode::TextOrd { .. }
            | ParseNode::OpToken { .. }
            | ParseNode::AccentToken { .. }
            | ParseNode::SpacingNode { .. }
            | ParseNode::ColorToken { .. }
            | ParseNode::Size { .. }
            | ParseNode::DelimSizing { .. }
            | ParseNode::LeftRightRight { .. }
            | ParseNode::Middle { .. }
            | ParseNode::Rule { .. }
            | ParseNode::Kern { .. }
            | ParseNode::Environment { .. }
            | ParseNode::Cr { .. }
            | ParseNode::Infix { .. }
            | ParseNode::Internal { .. }
            | ParseNode::Verb { .. }
            | ParseNode::Url { .. }
            | ParseNode::Raw { .. }
            | ParseNode::NoNumber { .. }
            | ParseNode::IncludeGraphics { .. } => {}

            ParseNode::OrdGroup { body, .. } => {
                let child_depth = if visit.suppress_group_cost {
                    visit.depth
                } else {
                    visit.depth + 1
                };
                max_depth = max_depth.max(child_depth);
                push_nodes(&mut stack, body, child_depth, child_weight);
            }
            ParseNode::SupSub { base, sup, sub, .. } => {
                if let Some(base) = base.as_deref() {
                    push_node(&mut stack, base, visit.depth, false, child_weight);
                }
                if let Some(sup) = sup.as_deref() {
                    push_node(&mut stack, sup, visit.depth + 1, true, child_weight);
                }
                if let Some(sub) = sub.as_deref() {
                    push_node(&mut stack, sub, visit.depth + 1, true, child_weight);
                }
            }
            ParseNode::GenFrac { numer, denom, .. } => {
                push_node(&mut stack, numer, visit.depth + 1, true, child_weight);
                push_node(&mut stack, denom, visit.depth + 1, true, child_weight);
            }
            ParseNode::Sqrt { body, index, .. } => {
                push_node(&mut stack, body, visit.depth + 1, true, child_weight);
                if let Some(index) = index.as_deref() {
                    push_node(&mut stack, index, visit.depth + 1, true, child_weight);
                }
            }
            ParseNode::Accent { base, .. } | ParseNode::AccentUnder { base, .. } => {
                push_node(&mut stack, base, visit.depth + 1, true, child_weight);
            }
            ParseNode::Op { body, .. } => {
                if let Some(body) = body {
                    push_nodes(&mut stack, body, visit.depth + 1, child_weight);
                }
            }
            ParseNode::OperatorName { body, .. }
            | ParseNode::Text { body, .. }
            | ParseNode::Color { body, .. }
            | ParseNode::Styling { body, .. }
            | ParseNode::Sizing { body, .. }
            | ParseNode::LeftRight { body, .. }
            | ParseNode::Phantom { body, .. }
            | ParseNode::MClass { body, .. }
            | ParseNode::Href { body, .. }
            | ParseNode::HBox { body, .. }
            | ParseNode::Pmb { body, .. }
            | ParseNode::Html { body, .. } => {
                push_nodes(&mut stack, body, visit.depth + 1, child_weight);
            }
            ParseNode::Font { body, .. }
            | ParseNode::Overline { body, .. }
            | ParseNode::Underline { body, .. }
            | ParseNode::VPhantom { body, .. }
            | ParseNode::Smash { body, .. }
            | ParseNode::HorizBrace { base: body, .. }
            | ParseNode::Enclose { body, .. }
            | ParseNode::Lap { body, .. }
            | ParseNode::RaiseBox { body, .. }
            | ParseNode::VCenter { body, .. } => {
                push_node(&mut stack, body, visit.depth + 1, true, child_weight);
            }
            ParseNode::Array { body, tags, .. } => {
                if let Some(tags) = tags {
                    for tag in tags.iter().rev() {
                        if let ArrayTag::Explicit(nodes) = tag {
                            push_nodes(&mut stack, nodes, visit.depth + 1, child_weight);
                        }
                    }
                }
                for row in body.iter().rev() {
                    push_nodes(&mut stack, row, visit.depth + 1, child_weight);
                }
            }
            ParseNode::MathChoice {
                display,
                text,
                script,
                scriptscript,
                ..
            } => {
                push_nodes(&mut stack, scriptscript, visit.depth + 1, child_weight);
                push_nodes(&mut stack, script, visit.depth + 1, child_weight);
                push_nodes(&mut stack, text, visit.depth + 1, child_weight);
                push_nodes(&mut stack, display, visit.depth + 1, child_weight);
            }
            ParseNode::XArrow { body, below, .. } => {
                push_node(&mut stack, body, visit.depth + 1, true, child_weight);
                if let Some(below) = below.as_deref() {
                    push_node(&mut stack, below, visit.depth + 1, true, child_weight);
                }
            }
            ParseNode::Tag { body, tag, .. } => {
                push_nodes(&mut stack, tag, visit.depth + 1, child_weight);
                push_nodes(&mut stack, body, visit.depth + 1, child_weight);
            }
            ParseNode::HtmlMathMl { html, mathml, .. } => {
                push_nodes(&mut stack, mathml, visit.depth + 1, child_weight);
                push_nodes(&mut stack, html, visit.depth + 1, child_weight);
            }
            ParseNode::CdLabel { label, .. } => {
                push_node(&mut stack, label, visit.depth + 1, true, child_weight);
            }
            ParseNode::CdLabelParent { fragment, .. } => {
                push_node(&mut stack, fragment, visit.depth + 1, true, child_weight);
            }
            ParseNode::CdArrow {
                label_above,
                label_below,
                ..
            } => {
                if let Some(label) = label_above.as_deref() {
                    push_node(&mut stack, label, visit.depth + 1, true, child_weight);
                }
                if let Some(label) = label_below.as_deref() {
                    push_node(&mut stack, label, visit.depth + 1, true, child_weight);
                }
            }
            ParseNode::ProofTree { tree, .. } => {
                let (depth, payload_work) = analyze_proof_branch(tree);
                max_depth = max_depth.max(visit.depth + depth);
                work = work.saturating_add(visit.weight.saturating_mul(payload_work));
            }
        }
    }

    (max_depth, work)
}

// Compute both quantities together: separate recursive depth/work walks would
// themselves double the work at each proof tree nested inside a conclusion.
fn analyze_proof_branch(branch: &ProofBranch) -> (usize, usize) {
    struct Frame<'a> {
        branch: &'a ProofBranch,
        next_premise: usize,
        max_premise_depth: usize,
        work: usize,
    }

    let mut stack = vec![Frame {
        branch,
        next_premise: 0,
        max_premise_depth: 0,
        work: 1,
    }];
    let mut completed_child_depth = None;

    while let Some(frame) = stack.last_mut() {
        if let Some((depth, work)) = completed_child_depth.take() {
            frame.max_premise_depth = frame.max_premise_depth.max(depth);
            frame.work = frame.work.saturating_add(work);
            continue;
        }

        if frame.next_premise < frame.branch.premises.len() {
            let child = &frame.branch.premises[frame.next_premise];
            frame.next_premise += 1;
            stack.push(Frame {
                branch: child,
                next_premise: 0,
                max_premise_depth: 0,
                work: 1,
            });
            continue;
        }

        let mut payload_depth = 0;
        for payload in [
            Some(frame.branch.conclusion.as_slice()),
            frame.branch.left_label.as_deref(),
            frame.branch.right_label.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let (depth, work) = analyze_nodes(payload);
            payload_depth = payload_depth.max(depth);
            frame.work = frame.work.saturating_add(work);
        }
        let work = frame.work;
        let depth = 1 + payload_depth.max(frame.max_premise_depth);
        stack.pop();
        if stack.is_empty() {
            return (depth, work);
        }
        completed_child_depth = Some((depth, work));
    }

    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_node::{ArrayTag, Mode};

    fn nested_ord_group(depth: usize) -> ParseNode {
        let mut node = ParseNode::MathOrd {
            mode: Mode::Math,
            text: "x".to_string(),
            loc: None,
        };
        for _ in 0..depth {
            node = ParseNode::OrdGroup {
                mode: Mode::Math,
                body: vec![node],
                semisimple: None,
                loc: None,
            };
        }
        node
    }

    #[test]
    fn logical_depth_includes_explicit_array_tags() {
        let array = ParseNode::Array {
            mode: Mode::Math,
            body: vec![],
            row_gaps: vec![],
            hlines_before_row: vec![],
            cols: None,
            col_separation_type: None,
            hskip_before_and_after: None,
            add_jot: None,
            arraystretch: 1.0,
            tags: Some(vec![ArrayTag::Explicit(vec![nested_ord_group(3)])]),
            leqno: None,
            is_cd: None,
            loc: None,
        };

        assert_eq!(parse_nodes_logical_depth(&[array]), 4);
    }
}

#[cfg(test)]
mod layout_resource_tests {
    use super::*;
    use crate::parse;
    fn pmb(body: &str, n: usize) -> String {
        format!("{}{}{}", r"\pmb{".repeat(n), body, "}".repeat(n))
    }
    #[test]
    fn expanded_work_boundary_counts_siblings_and_large_leaves() {
        let body = parse(&pmb("x", 15)).unwrap();
        assert_eq!(analyze_nodes(&body).1, MAX_LAYOUT_WORK - 1);
        assert!(parse(&(pmb("x", 15) + "x")).is_ok());
        assert!(
            parse(&(pmb("x", 15) + "xx"))
                .unwrap_err()
                .to_string()
                .contains("Layout resource limit exceeded")
        );
        assert!(
            parse(&pmb("x", 16))
                .unwrap_err()
                .to_string()
                .contains("Layout resource limit exceeded")
        );
        let verb = format!(r"\verb|{}|", "x".repeat(100));
        assert!(
            parse(&pmb(&verb, 10))
                .unwrap_err()
                .to_string()
                .contains("Layout resource limit exceeded")
        );
    }
    #[test]
    fn wrappers_and_proof_payloads_cannot_hide_expansion() {
        let large = pmb("x", 16);
        for input in [
            format!(r"\color{{red}}{{{large}}}"),
            format!(r"\begin{{matrix}}{large}\end{{matrix}}"),
            format!(r"\begin{{prooftree}}\AxiomC{{{large}}}\end{{prooftree}}"),
            format!(r"\left({large}\middle|y\right)"),
        ] {
            assert!(
                parse(&input)
                    .unwrap_err()
                    .to_string()
                    .contains("Layout resource limit exceeded")
            );
        }
    }

    #[test]
    fn array_rules_count_toward_expanded_work_boundary() {
        for rule in [r"\hline ", r"\hdashline "] {
            let array =
                |count| format!(r"\begin{{array}}{{c}}{}x\end{{array}}", rule.repeat(count));
            // 13 bold nodes contribute 8191 units; the array, cell style,
            // cell group, glyph and three rules contribute 7 * 8192 units.
            let accepted = pmb(&array(3), 13);
            assert_eq!(
                analyze_nodes(&parse(&accepted).unwrap()).1,
                MAX_LAYOUT_WORK - 1
            );
            assert!(parse(&(accepted.clone() + "x")).is_ok());
            assert!(
                parse(&(accepted + "xx"))
                    .unwrap_err()
                    .to_string()
                    .contains("Layout resource limit exceeded")
            );
            // First rejected rule count, reported exploit, and larger input.
            for count in [4, 64, 300] {
                assert!(
                    parse(&pmb(&array(count), 13))
                        .unwrap_err()
                        .to_string()
                        .contains("Layout resource limit exceeded")
                );
            }
        }
    }

    #[test]
    fn array_rules_count_at_every_row_boundary() {
        let plain = parse(r"\begin{array}{c}x\\y\end{array}").unwrap();
        let ruled =
            parse(r"\begin{array}{c}\hline\hdashline x\\\hline y\\\hdashline\hline\end{array}")
                .unwrap();
        assert_eq!(analyze_nodes(&ruled).1, analyze_nodes(&plain).1 + 5);
    }

    fn sparse_grid(cd: bool, n: usize) -> String {
        if cd {
            format!(
                r"\begin{{CD}}{}{}\end{{CD}}",
                "@.".repeat(n),
                r"\\x".repeat(n - 1)
            )
        } else {
            format!(
                r"\begin{{matrix}}{}x{}\end{{matrix}}",
                "&".repeat(n - 1),
                r"\\x".repeat(n - 1)
            )
        }
    }

    #[test]
    fn padded_array_and_cd_work_boundaries_are_checked_before_layout() {
        for (cd, n, expected_work) in [(false, 254, 65_278), (true, 181, 65_522)] {
            let input = sparse_grid(cd, n);
            assert_eq!(analyze_nodes(&parse(&input).unwrap()).1, expected_work);
            let exact = input + &"x".repeat(MAX_LAYOUT_WORK - expected_work);
            assert_eq!(analyze_nodes(&parse(&exact).unwrap()).1, MAX_LAYOUT_WORK);
            for rejected in [exact + "x", sparse_grid(cd, n + 1), sparse_grid(cd, 500)] {
                assert!(
                    parse(&rejected)
                        .unwrap_err()
                        .to_string()
                        .contains("Layout resource limit exceeded")
                );
            }
        }
    }

    #[test]
    fn padded_cells_share_bold_and_sibling_budgets() {
        for cd in [false, true] {
            let small = sparse_grid(cd, 2);
            assert!(parse(&pmb(&small, 12)).is_ok());
            assert!(
                parse(&pmb(&small, 13))
                    .unwrap_err()
                    .to_string()
                    .contains("Layout resource limit exceeded")
            );
            let sibling = sparse_grid(cd, 150);
            assert!(parse(&sibling).is_ok());
            assert!(
                parse(&sibling.repeat(3))
                    .unwrap_err()
                    .to_string()
                    .contains("Layout resource limit exceeded")
            );
        }
    }
}
