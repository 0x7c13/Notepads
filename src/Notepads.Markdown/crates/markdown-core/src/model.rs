// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Immutable model authority, granted only after native facts and independent validation.

use crate::config::{Extensions, ParseConfig};
use crate::error::CoreError;
use crate::html::HtmlProjection;
use crate::identity::{LogicalSpan, ModelIdentity};
use crate::ir::{
    AtomKind, ClaimKind, CompactIr, DetailRow, FlowId, FlowScope, HtmlEmphasis, NodeId, NodeKind,
    ObjectId, PayloadId, ScriptKind, StageKind,
};
use crate::mapping::{Affinity, MapIndex, PointQuery, PointResponse, RangeQuery, RangeResponse};
use crate::memory::{BuildGrowingVec, BuildValue, FrozenRows};
use crate::policy::{Lease, Operation, OwnedWork, SourceBudget, construction_scope};
use crate::presentation::{
    RUN_BOLD, RUN_CODE, RUN_HIGHLIGHT, RUN_ITALIC, RUN_MONOSPACE, RUN_STRIKE, RUN_UNDERLINE,
    SCRIPT_CODE_LINE_HEIGHT, SCRIPT_SHAPE_ONLY, ScriptStyle,
};
use crate::source::{ParseInput, SourceDocument, SourceSnapshot};
use std::sync::Arc;

mod html_probe;

#[cfg(test)]
mod projection_facts_tests;

enum ParseProjection {
    Accepted(AcceptedModel),
    Projected((HtmlProjection, String, Lease)),
    Fallback,
}

struct ModelData {
    // Free buffers and source pins before retiring the control reservation.
    ir: CompactIr,
    index: MapIndex,
    source: SourceSnapshot,
    html: Option<HtmlProjection>,
    inline_emphasis: Option<FrozenRows<InlineEmphasisRun>>,
    inline_scripts: Option<InlineScriptStyles>,
    disclosures: Option<FrozenRows<DisclosureRegion>>,
    identity: ModelIdentity,
    main: FlowId,
    _control: Lease,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisclosureRegion {
    pub node: NodeId,
    pub summary: NodeId,
    pub first_body_node: u32,
    pub next_node: u32,
    pub summary_start: u64,
    pub body: LogicalSpan,
    pub initially_expanded: bool,
}
impl BuildValue for DisclosureRegion {}

fn disclosure_regions(
    ir: &CompactIr,
    main: FlowId,
    op: &mut OwnedWork<'_, '_>,
) -> Result<Option<FrozenRows<DisclosureRegion>>, CoreError> {
    use comrak::native::CalloutDisclosure;
    let max = op.require_model_limits()?.model_records;
    let mut rows = BuildGrowingVec::new(max, crate::error::LimitKind::ModelRecords, op)?;
    let past_last_node = u32::try_from(ir.nodes().len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    let mut previous = None;
    for detail in ir.details() {
        op.events(1)?;
        op.scan(size_of::<DetailRow>() as u64)?;
        let DetailRow::Callout {
            disclosure,
            title: summary,
            ..
        } = *detail
        else {
            continue;
        };
        let initially_expanded = match disclosure {
            CalloutDisclosure::InitiallyOpen => true,
            CalloutDisclosure::InitiallyClosed => false,
            CalloutDisclosure::Static => continue,
        };
        op.scan((2 * size_of::<crate::ir::NodeRow>()) as u64)?;
        let header = ir.nodes()[summary.index()];
        let node = header
            .parent
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let owner = ir.nodes()[node.index()];
        if header.flow != main
            || owner.flow != main
            || previous.is_some_and(|(id, start)| id >= node || start > header.logical.start())
        {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        previous = Some((node, header.logical.start()));
        let mut ancestor = owner;
        let next_node = loop {
            if let Some(next) = ancestor.next_sibling {
                break next.value();
            }
            let Some(parent) = ancestor.parent else {
                break past_last_node;
            };
            op.events(1)?;
            op.scan(size_of::<crate::ir::NodeRow>() as u64)?;
            ancestor = ir.nodes()[parent.index()];
        };
        let body = LogicalSpan::new(header.logical.end(), owner.logical.end())
            .map_err(|error| op.fail(error))?;
        op.admit_model(crate::error::LimitKind::ModelRecords, 1)?;
        rows.push(
            DisclosureRegion {
                node,
                summary,
                first_body_node: header.next_sibling.map_or(next_node, NodeId::value),
                next_node,
                summary_start: header.logical.start(),
                body,
                initially_expanded,
            },
            op,
        )?;
    }
    if rows.len() == 0 {
        Ok(None)
    } else {
        Ok(Some(rows.freeze(op)?))
    }
}

#[derive(Clone, Copy)]
struct InlineEmphasisRun {
    start: u64,
    end: u64,
    flags: u32,
}

fn emphasis_flags(counts: [usize; HtmlEmphasis::STYLE_COUNT]) -> u32 {
    (if counts[0] != 0 { RUN_BOLD } else { 0 })
        | (if counts[1] != 0 { RUN_ITALIC } else { 0 })
        | (if counts[2] != 0 { RUN_STRIKE } else { 0 })
        | (if counts[3] != 0 { RUN_UNDERLINE } else { 0 })
        | (if counts[HtmlEmphasis::MONOSPACE_STYLE] != 0 {
            RUN_MONOSPACE
        } else {
            0
        })
        | (if counts[HtmlEmphasis::HIGHLIGHT_STYLE] != 0 {
            RUN_HIGHLIGHT
        } else {
            0
        })
}

fn inline_emphasis_runs(
    ir: &CompactIr,
    main: FlowId,
    op: &mut OwnedWork<'_, '_>,
) -> Result<Option<FrozenRows<InlineEmphasisRun>>, CoreError> {
    let mut has_emphasis = false;
    for stage in ir.stages() {
        op.events(1)?;
        if stage.kind == StageKind::HtmlEmphasisDelimiter {
            has_emphasis = true;
            break;
        }
    }
    if !has_emphasis {
        return Ok(None);
    }
    let maximum = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .model_records;
    let mut runs = BuildGrowingVec::new(maximum, crate::error::LimitKind::ModelRecords, op)?;
    let flow = ir
        .flows()
        .get(main.index())
        .ok_or(CoreError::InvalidProvenance)?;
    let mut counts = [0_usize; HtmlEmphasis::STYLE_COUNT];
    let mut start = 0;
    let mut flags = 0;
    for at in flow.atoms.indices(ir.atoms_len())? {
        let atom = ir.owned_read_atom_at(at, op)?;
        let Some(stage) = ir
            .first_stage(atom)
            .filter(|s| s.kind == StageKind::HtmlEmphasisDelimiter)
        else {
            continue;
        };
        let tag =
            HtmlEmphasis::from_parameter(stage.parameter).ok_or(CoreError::InvalidProvenance)?;
        let Some(style_index) = tag.style_index() else {
            continue;
        };
        let count = &mut counts[style_index];
        if tag.closing {
            *count -= 1;
        } else {
            *count += 1;
        }
        let next = emphasis_flags(counts);
        if flags != next {
            let end = atom.target.start;
            if flags != 0 && start < end {
                op.admit_model(crate::error::LimitKind::ModelRecords, 1)?;
                runs.push(InlineEmphasisRun { start, end, flags }, op)?;
            }
            start = end;
            flags = next;
        }
    }
    Ok(Some(runs.freeze(op)?))
}

#[derive(Clone, Copy)]
struct InlineScriptRun {
    start: u64,
    end: u64,
    style: ScriptStyle,
}

#[derive(Clone, Copy)]
pub(crate) struct InlineScriptScope {
    pub owner: NodeId,
    pub parent: NodeId,
    pub font_flags: u32,
    pub start: u64,
    pub end: u64,
    pub style: ScriptStyle,
}

#[derive(Clone, Copy)]
struct ScriptFrame {
    previous: ScriptStyle,
    scope: Option<usize>,
    markdown: Option<NodeId>,
}

impl BuildValue for InlineScriptRun {}
impl BuildValue for InlineScriptScope {}
impl BuildValue for ScriptFrame {}

struct InlineScriptStyles {
    runs: FrozenRows<InlineScriptRun>,
    scopes: FrozenRows<InlineScriptScope>,
    footnotes: bool,
}

fn script_text_owner<'id>(
    ir: &CompactIr,
    node: NodeId,
    owners: &BuildGrowingVec<'id, Option<Option<NodeId>>>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(NodeId, NodeId), CoreError> {
    op.events(1)?;
    let opening = op.read_metadata(size_of::<crate::ir::NodeRow>() as u64, || {
        ir.nodes()
            .get(node.index())
            .copied()
            .ok_or(CoreError::InvalidProvenance)
    })?;
    let parent = opening.parent.ok_or(CoreError::InvalidProvenance)?;
    let owner = crate::ir::owned_row(owners, node.index(), op)?
        .flatten()
        .ok_or(CoreError::InvalidProvenance)?;
    Ok((owner, parent))
}
/// Resolve each main-flow ancestor only once. The funded cache stores both the
/// visited bit and its Text owner; a cached ancestor ends the walk immediately.
/// The second walk assigns that owner without an unfunded depth-sized stack.
fn markdown_shape_scopes<'id>(
    ir: &CompactIr,
    first: NodeId,
    owners: &mut BuildGrowingVec<'id, Option<Option<NodeId>>>,
    scopes: &mut BuildGrowingVec<'id, InlineScriptScope>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    use crate::ir::{DetailRow, NodeKind, NodeRow};
    let mut current = first;
    let owner = loop {
        op.events(1)?;
        if let Some(owner) = crate::ir::owned_row(owners, current.index(), op)? {
            break owner;
        }
        let row = op.read_metadata(size_of::<NodeRow>() as u64, || {
            ir.nodes()
                .get(current.index())
                .copied()
                .ok_or(CoreError::InvalidProvenance)
        })?;
        if matches!(
            row.kind,
            NodeKind::Paragraph | NodeKind::Heading | NodeKind::TableCell
        ) {
            break Some(current);
        }
        let Some(parent) = row.parent else { break None };
        current = parent;
    };
    current = first;
    loop {
        op.events(1)?;
        if crate::ir::owned_row(owners, current.index(), op)?.is_some() {
            break;
        }
        let row = op.read_metadata(size_of::<NodeRow>() as u64, || {
            ir.nodes()
                .get(current.index())
                .copied()
                .ok_or(CoreError::InvalidProvenance)
        })?;
        owners.replace(current.index(), Some(owner), op)?;
        if let Some(owner) = owner {
            if !row.logical.is_empty()
                && matches!(
                    row.kind,
                    NodeKind::Emphasis
                        | NodeKind::Strong
                        | NodeKind::Strike
                        | NodeKind::Link
                        | NodeKind::InlineCode
                )
            {
                let html = if let Some(detail) = row.detail {
                    op.events(1)?;
                    op.read_metadata(size_of::<DetailRow>() as u64, || {
                        ir.details()
                            .get(detail.index())
                            .copied()
                            .ok_or(CoreError::InvalidProvenance)
                    })
                    .map(|detail| matches!(detail, DetailRow::Html { .. }))?
                } else {
                    false
                };
                if !html {
                    op.admit_model(crate::error::LimitKind::ModelRecords, 1)?;
                    scopes.push(
                        InlineScriptScope {
                            owner,
                            parent: row.parent.ok_or(CoreError::InvalidProvenance)?,
                            font_flags: SCRIPT_SHAPE_ONLY,
                            start: row.logical.start(),
                            end: row.logical.end(),
                            style: ScriptStyle::default(),
                        },
                        op,
                    )?;
                }
            }
        }
        if Some(current) == owner {
            break;
        }
        let Some(parent) = row.parent else { break };
        current = parent;
    }
    Ok(())
}
fn inline_script_styles(
    ir: &CompactIr,
    main: FlowId,
    footnotes: bool,
    markdown: bool,
    op: &mut OwnedWork<'_, '_>,
) -> Result<Option<InlineScriptStyles>, CoreError> {
    let mut found = false;
    for stage in ir.stages() {
        op.events(1)?;
        if stage.kind == StageKind::HtmlEmphasisDelimiter
            && HtmlEmphasis::from_parameter(stage.parameter)
                .and_then(HtmlEmphasis::script_kind)
                .is_some()
        {
            found = true;
            break;
        }
    }
    let mut found_markdown = false;
    if markdown {
        for row in ir.nodes() {
            op.events(1)?;
            if row.flow == main && matches!(row.kind, NodeKind::Superscript | NodeKind::Subscript) {
                found_markdown = true;
                break;
            }
        }
    }
    let mut note_numbers = false;
    if footnotes {
        for row in ir.details() {
            op.events(1)?;
            if matches!(row, DetailRow::FootnoteReference { .. }) {
                note_numbers = true;
                break;
            }
        }
    }
    if !found && !found_markdown && !note_numbers {
        return Ok(None);
    }
    let maximum = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .model_records;
    let mut runs = BuildGrowingVec::new(maximum, crate::error::LimitKind::ModelRecords, op)?;
    let mut scopes = BuildGrowingVec::<InlineScriptScope>::new(
        maximum,
        crate::error::LimitKind::ModelRecords,
        op,
    )?;
    let mut stack =
        BuildGrowingVec::<ScriptFrame>::new(maximum, crate::error::LimitKind::ModelRecords, op)?;
    let flow = ir
        .flows()
        .get(main.index())
        .ok_or(CoreError::InvalidProvenance)?;
    // No cache or shape-only rows are allocated until a real script is found.
    let mut shape_owners =
        BuildGrowingVec::new(maximum, crate::error::LimitKind::ModelRecords, op)?;
    for _ in 0..ir.nodes().len() {
        shape_owners.push(None::<Option<NodeId>>, op)?;
    }
    let mut depth = 0usize;
    let mut counts = [0_usize; HtmlEmphasis::STYLE_COUNT];
    let mut start = 0;
    let mut style = ScriptStyle::default();
    let mut previous_owner = None;
    let mut markdown_scope = None;
    for at in flow.atoms.indices(ir.atoms_len())? {
        let atom = ir.owned_read_atom_at(at, op)?;
        if previous_owner != Some(atom.owner) {
            previous_owner = Some(atom.owner);
            markdown_shape_scopes(ir, atom.owner, &mut shape_owners, &mut scopes, op)?;
            if found_markdown || style != ScriptStyle::default() {
                op.events(1)?;
                let node = op.read_metadata(size_of::<crate::ir::NodeRow>() as u64, || {
                    ir.nodes()
                        .get(atom.owner.index())
                        .copied()
                        .ok_or(CoreError::InvalidProvenance)
                })?;
                markdown_scope = match node.kind {
                    NodeKind::Superscript => Some((node.logical, ScriptKind::Superscript)),
                    NodeKind::Subscript => Some((node.logical, ScriptKind::Subscript)),
                    _ => None,
                };
                // Native Markdown code is one opaque owner with no HTML detail.
                // Retain its whole box once, including trim/line-ending atoms.
                if style != ScriptStyle::default()
                    && node.kind == NodeKind::InlineCode
                    && node.detail.is_none()
                {
                    let (owner, parent) = script_text_owner(ir, atom.owner, &shape_owners, op)?;
                    op.admit_model(crate::error::LimitKind::ModelRecords, 1)?;
                    scopes.push(
                        InlineScriptScope {
                            owner,
                            parent,
                            font_flags: (emphasis_flags(counts) & (RUN_BOLD | RUN_ITALIC))
                                | RUN_CODE
                                | SCRIPT_CODE_LINE_HEIGHT,
                            start: node.logical.start(),
                            end: node.logical.end(),
                            style,
                        },
                        op,
                    )?;
                }
            }
        }
        if note_numbers && atom.kind == AtomKind::Synthetic {
            let node = op.read_metadata(size_of::<crate::ir::NodeRow>() as u64, || {
                ir.nodes()
                    .get(atom.owner.index())
                    .copied()
                    .ok_or(CoreError::InvalidProvenance)
            })?;
            if node.kind == NodeKind::FootnoteReference {
                if style != ScriptStyle::default() && start < atom.target.start {
                    op.admit_model(crate::error::LimitKind::ModelRecords, 1)?;
                    runs.push(
                        InlineScriptRun {
                            start,
                            end: atom.target.start,
                            style,
                        },
                        op,
                    )?;
                }
                let scaled = style.nested(ScriptKind::Superscript);
                let (owner, parent) = script_text_owner(ir, atom.owner, &shape_owners, op)?;
                op.admit_model(crate::error::LimitKind::ModelRecords, 2)?;
                runs.push(
                    InlineScriptRun {
                        start: atom.target.start,
                        end: atom.target.end,
                        style: scaled,
                    },
                    op,
                )?;
                scopes.push(
                    InlineScriptScope {
                        owner,
                        parent,
                        font_flags: emphasis_flags(counts) & (RUN_BOLD | RUN_ITALIC | RUN_CODE),
                        start: atom.target.start,
                        end: atom.target.end,
                        style: scaled,
                    },
                    op,
                )?;
                start = atom.target.end;
                continue;
            }
        }
        let (closing, script_kind, font_index, code_flags, markdown_owner) =
            if let Some((logical, kind)) = markdown_scope
                && atom.kind == AtomKind::Hidden
                && ir.owned_read_authority(atom.claim, op)?.kind == ClaimKind::Delimiter
            {
                let position = atom.target.start;
                let is_end = position == logical.end();
                let is_start = position == logical.start();
                if !is_start && !is_end {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                let closing = is_end
                    && (!is_start
                        || (depth != 0
                            && crate::ir::owned_row(&stack, depth - 1, op)?.markdown
                                == Some(atom.owner)));
                (closing, Some(kind), None, 0, Some(atom.owner))
            } else {
                let Some(stage) = ir.first_stage(atom) else {
                    continue;
                };
                match stage.kind {
                    StageKind::HtmlEmphasisDelimiter => {
                        let tag = HtmlEmphasis::from_parameter(stage.parameter)
                            .ok_or(CoreError::InvalidProvenance)?;
                        (tag.closing, tag.script_kind(), tag.style_index(), 0, None)
                    }
                    StageKind::HtmlCodeDelimiter => {
                        let tag = crate::ir::HtmlCodeDelimiter::from_parameter(stage.parameter)
                            .ok_or(CoreError::InvalidProvenance)?;
                        (
                            tag.closing,
                            None,
                            None,
                            RUN_CODE
                                | if tag.has_code_line_height() {
                                    SCRIPT_CODE_LINE_HEIGHT
                                } else {
                                    0
                                },
                            None,
                        )
                    }
                    _ => continue,
                }
            };
        let code = code_flags != 0;
        if let Some(index) = font_index {
            if closing {
                counts[index] = counts[index]
                    .checked_sub(1)
                    .ok_or(CoreError::InvalidProvenance)?;
            } else {
                counts[index] += 1;
            }
        }
        if script_kind.is_some() && style != ScriptStyle::default() && start < atom.target.start {
            op.admit_model(crate::error::LimitKind::ModelRecords, 1)?;
            runs.push(
                InlineScriptRun {
                    start,
                    end: atom.target.start,
                    style,
                },
                op,
            )?;
        }
        if closing {
            depth = depth.checked_sub(1).ok_or(CoreError::InvalidProvenance)?;
            let frame = crate::ir::owned_row(&stack, depth, op)?;
            if frame.markdown != markdown_owner {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            if let Some(index) = frame.scope {
                let mut scope = crate::ir::owned_row(&scopes, index, op)?;
                scope.end = atom.target.start;
                scopes.replace(index, scope, op)?;
            }
            style = frame.previous;
        } else {
            let previous = style;
            if let Some(kind) = script_kind {
                style = style.nested(kind);
            }
            let font_container = code
                || font_index
                    .is_some_and(|index| index < 2 || index == HtmlEmphasis::MONOSPACE_STYLE);
            let font_box =
                script_kind.is_some() || (font_container && style != ScriptStyle::default());
            // Every accepted HTML pair is a true shaping container. Existing
            // font boxes already carry its endpoints; all other pairs retain
            // only the boundary, including strike and containers outside sup/sub.
            let scope = {
                let (owner, parent) = script_text_owner(ir, atom.owner, &shape_owners, op)?;
                let index = scopes.len();
                let font_flags = if font_box {
                    let monospace = if counts[HtmlEmphasis::MONOSPACE_STYLE] != 0 {
                        RUN_CODE
                    } else {
                        0
                    };
                    (emphasis_flags(counts) & (RUN_BOLD | RUN_ITALIC)) | code_flags | monospace
                } else {
                    SCRIPT_SHAPE_ONLY
                };
                op.admit_model(crate::error::LimitKind::ModelRecords, 1)?;
                scopes.push(
                    InlineScriptScope {
                        owner,
                        parent,
                        font_flags,
                        start: atom.target.start,
                        end: atom.target.start,
                        style: if font_box {
                            style
                        } else {
                            ScriptStyle::default()
                        },
                    },
                    op,
                )?;
                Some(index)
            };
            let frame = ScriptFrame {
                previous,
                scope,
                markdown: markdown_owner,
            };
            if depth == stack.len() {
                stack.push(frame, op)?;
            } else {
                stack.replace(depth, frame, op)?;
            }
            depth += 1;
        }
        if script_kind.is_some() {
            start = atom.target.start;
        }
    }
    if depth != 0 || counts != [0; HtmlEmphasis::STYLE_COUNT] {
        return Err(op.fail(CoreError::InvalidProvenance));
    }
    crate::memory::sort_build_rows_by(
        &mut scopes,
        0,
        |left, right, _| {
            Ok((left.owner.value(), left.start, left.end).cmp(&(
                right.owner.value(),
                right.start,
                right.end,
            )))
        },
        op,
    )?;
    Ok(Some(InlineScriptStyles {
        footnotes: note_numbers,
        runs: runs.freeze(op)?,
        scopes: scopes.freeze(op)?,
    }))
}
/// Clones share one validated model and the original source and quota root.
#[derive(Clone)]
pub struct AcceptedModel(Arc<ModelData>);

/// Coarse synchronous experiment context; this is not a stable product SDK.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParseStage {
    #[default]
    NotStarted,
    SourceCapture,
    Prepare,
    Build,
    Validate,
    Index,
    Publish,
    Complete,
}

impl ParseStage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotStarted => "not-started",
            Self::SourceCapture => "source-capture",
            Self::Prepare => "prepare",
            Self::Build => "build",
            Self::Validate => "validate",
            Self::Index => "index",
            Self::Publish => "publish",
            Self::Complete => "complete",
        }
    }
}

/// Single-threaded output only. It never determines admission or publication.
#[derive(Debug, Default)]
pub struct ParseProgress {
    stage: ParseStage,
    #[cfg(test)]
    deadline: Option<progress_tests::Deadline>,
}

impl ParseProgress {
    pub fn begin_source_capture(&mut self) {
        self.stage = ParseStage::SourceCapture;
    }

    pub fn stage(&self) -> ParseStage {
        self.stage
    }
}

fn set_stage(progress: &mut Option<&mut ParseProgress>, stage: ParseStage) {
    if let Some(progress) = progress {
        progress.stage = stage;
        #[cfg(test)]
        progress.deadline_at_stage_for_test(stage);
    }
}

impl AcceptedModel {
    pub fn has_footnotes(&self) -> bool {
        self.0
            .inline_scripts
            .as_ref()
            .is_some_and(|scripts| scripts.footnotes)
    }

    /// Parse and accept atomically; faults never yield a queryable partial model.
    pub fn parse(
        source: SourceSnapshot,
        config: ParseConfig,
        generation: u64,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        Self::parse_inner(source, config, generation, op, None)
    }

    /// Experimental coarse context output; no callback, clock or lower-layer observer.
    pub fn experimental_parse_with_progress(
        source: SourceSnapshot,
        config: ParseConfig,
        generation: u64,
        op: &mut Operation,
        progress: &mut ParseProgress,
    ) -> Result<Self, CoreError> {
        Self::parse_inner(source, config, generation, op, Some(progress))
    }

    fn parse_inner(
        source: SourceSnapshot,
        config: ParseConfig,
        generation: u64,
        op: &mut Operation,
        mut progress: Option<&mut ParseProgress>,
    ) -> Result<Self, CoreError> {
        if config.extensions().contains(Extensions::FRONT_MATTER)
            || config.extensions().contains(Extensions::HTML_BLOCKS)
            || config.extensions().contains(Extensions::CALLOUTS)
            || config.extensions().contains(Extensions::HTML_INLINE)
        {
            set_stage(&mut progress, ParseStage::Prepare);
            op.checkpoint_now()?;
            op.ensure_budget(source.budget())?;
        }
        let front = crate::html::front_matter::prepare(&source, config, op)?;
        let html_blocks = config.extensions().contains(Extensions::HTML_BLOCKS);
        let callouts = config.extensions().contains(Extensions::CALLOUTS);
        let (html_candidate, directives) = if html_blocks || callouts {
            html_probe::block_candidates(source.text(), html_blocks, callouts, op)?
        } else {
            (false, false)
        };
        let inline_html_region = config.profile() == crate::config::Profile::G3
            && config.extensions().contains(Extensions::HTML_INLINE)
            && (!html_blocks || html_candidate)
            && html_probe::contains_inline_region(source.text(), op)?;
        if front.is_none() && !html_candidate && !inline_html_region && !directives {
            return Self::parse_once(source, config, generation, op, progress);
        }
        // The preliminary grammar selects HTML fragments and citation text.
        // Final IR, independent validation and the index run after projection.
        let citations = front.as_ref().is_some_and(|front| front.has_citations());
        let prepared = if html_candidate || inline_html_region || citations || directives {
            set_stage(&mut progress, ParseStage::Prepare);
            let input = match &front {
                Some(front) => front.mask(&source, op)?,
                None => source.clone(),
            };
            construction_scope(op, |mut work| {
                work.ensure_budget(input.budget())?;
                let identity = ModelIdentity::new(input.key(), generation, config)
                    .map_err(|error| work.fail(error))?;
                let prepared = ParseInput::prepare_owned(input.clone(), &mut work)?;
                set_stage(&mut progress, ParseStage::Build);
                let output = crate::parse::build_or_project(
                    &prepared,
                    config,
                    &source,
                    front,
                    inline_html_region,
                    directives,
                    &mut work,
                )
                .map_err(|error| work.fail(error))?;
                drop(prepared);
                let result = match output {
                    crate::parse::ProjectionBuild::Plain(ir) => ParseProjection::Accepted(
                        Self::accept_owned(input, identity, ir, &mut work, &mut progress)
                            .map_err(|error| work.fail(error))?,
                    ),
                    crate::parse::ProjectionBuild::Projected(projection) => {
                        ParseProjection::Projected(projection)
                    }
                    crate::parse::ProjectionBuild::MaskedFallback => ParseProjection::Fallback,
                };
                #[cfg(test)]
                if matches!(result, ParseProjection::Accepted(_))
                    && let Some(progress) = &mut progress
                {
                    progress.deadline_at_finish_for_test();
                }
                work.finish()?;
                Ok::<_, CoreError>(result)
            })?
        } else {
            match HtmlProjection::build(None, config, &source, front, op)? {
                Some(projection) => ParseProjection::Projected(projection),
                None => ParseProjection::Fallback,
            }
        };
        let (mut html, text, reservation) = match prepared {
            ParseProjection::Accepted(model) => {
                set_stage(&mut progress, ParseStage::Complete);
                return Ok(model);
            }
            ParseProjection::Projected(projection) => projection,
            ParseProjection::Fallback => {
                return Self::parse_once(source, config, generation, op, progress);
            }
        };
        let projected = SourceDocument::new(html.original.key(), text.as_bytes(), op)?.snapshot();
        drop(text);
        drop(reservation);
        let mut model = Self::parse_with_projection_once(
            projected,
            config,
            generation,
            Some(&html),
            op,
            progress,
        )?;
        html.bind_static_checkboxes(&model, op)?;
        html.bind_tables(&model, op)?;
        html.bind_destinations(&model, op)?;
        html.finish_styles(&model, op)?;
        html.finish_ruby(&model, op)?;
        html.finish_anchors(&model, op)?;
        let disclosures = html.bind_disclosures(&model, op)?;
        let data = Arc::get_mut(&mut model.0).expect("newly parsed model is exclusive");
        if let Some(disclosures) = disclosures {
            data.disclosures = Some(disclosures);
        }
        data.html = Some(html);
        Ok(model)
    }

    pub(crate) fn is_front_matter_table(&self, node: NodeId) -> bool {
        self.0.html.as_ref().is_some_and(|projection| {
            projection.is_front_matter_table(self.ir().nodes()[node.index()].source_envelope)
        })
    }

    pub(crate) fn html_anchors(&self) -> &[crate::html::HtmlAnchor] {
        self.0
            .html
            .as_ref()
            .map_or(&[], |projection| projection.anchors())
    }

    pub(crate) fn citation_region(
        &self,
        span: crate::identity::SourceSpan,
        op: &mut Operation,
    ) -> Result<bool, CoreError> {
        self.0
            .html
            .as_ref()
            .map_or(Ok(false), |projection| projection.citation_region(span, op))
    }

    pub(crate) fn citation_destination(
        &self,
        destination: &str,
        op: &mut Operation,
    ) -> Result<Option<u64>, CoreError> {
        self.0.html.as_ref().map_or(Ok(None), |projection| {
            projection.citation_destination(destination, op)
        })
    }

    pub(crate) fn is_static_html_checkbox(&self, object: ObjectId) -> bool {
        self.0
            .html
            .as_ref()
            .is_some_and(|projection| projection.is_static_html_checkbox(object))
    }

    pub(crate) fn html_table_cell(
        &self,
        table: NodeId,
        cell: NodeId,
        op: &mut Operation,
    ) -> Result<Option<crate::html::CellFacts>, CoreError> {
        match &self.0.html {
            Some(projection) => projection.table_cell(table, cell, op),
            None => Ok(None),
        }
    }

    fn parse_once(
        source: SourceSnapshot,
        config: ParseConfig,
        generation: u64,
        op: &mut Operation,
        progress: Option<&mut ParseProgress>,
    ) -> Result<Self, CoreError> {
        Self::parse_with_projection_once(source, config, generation, None, op, progress)
    }

    fn parse_with_projection_once(
        source: SourceSnapshot,
        config: ParseConfig,
        generation: u64,
        projection: Option<&HtmlProjection>,
        op: &mut Operation,
        mut progress: Option<&mut ParseProgress>,
    ) -> Result<Self, CoreError> {
        construction_scope(op, |mut work| {
            let model = Self::parse_owned(
                source,
                config,
                generation,
                projection,
                &mut work,
                &mut progress,
            )
            .map_err(|error| work.fail(error))?;
            #[cfg(test)]
            if let Some(progress) = &mut progress {
                progress.deadline_at_finish_for_test();
            }
            work.finish()?;
            set_stage(&mut progress, ParseStage::Complete);
            Ok(model)
        })
    }

    fn parse_owned(
        source: SourceSnapshot,
        config: ParseConfig,
        generation: u64,
        projection: Option<&HtmlProjection>,
        op: &mut OwnedWork<'_, '_>,
        progress: &mut Option<&mut ParseProgress>,
    ) -> Result<Self, CoreError> {
        set_stage(progress, ParseStage::Prepare);
        op.checkpoint_now()?;
        op.ensure_budget(source.budget())?;
        let identity =
            ModelIdentity::new(source.key(), generation, config).map_err(|error| op.fail(error))?;
        let input = ParseInput::prepare_owned(source.clone(), op)?;
        set_stage(progress, ParseStage::Build);
        op.checkpoint_now()?;
        let ir = match projection {
            Some(projection) => {
                crate::parse::build_with_projection(&input, config, Some(projection), op)?
            }
            None => crate::parse::build(&input, config, op)?,
        };
        // The IR owns its data; native views and grammar temporaries are gone.
        // Keep the model source pin, but release private normalized input now.
        drop(input);
        Self::accept_owned(source, identity, ir, op, progress)
    }

    fn accept_owned(
        source: SourceSnapshot,
        identity: ModelIdentity,
        ir: CompactIr,
        op: &mut OwnedWork<'_, '_>,
        progress: &mut Option<&mut ParseProgress>,
    ) -> Result<Self, CoreError> {
        let config = identity.config();
        set_stage(progress, ParseStage::Validate);
        op.checkpoint_now()?;
        let (index, verified_main) = if op.uses_verified_index() {
            let facts = crate::ir::validate::validate_for_index_owned(&ir, &source, identity, op)?;
            set_stage(progress, ParseStage::Index);
            // Facts owns the same operation borrow; its consumer checks entry.
            let (index, main) = MapIndex::build_verified(facts)?;
            (index, Some(main))
        } else {
            crate::ir::validate::validate_owned(&ir, &source, op)?;
            set_stage(progress, ParseStage::Index);
            op.checkpoint_now()?;
            (MapIndex::build_owned(&ir, &source, identity, op)?, None)
        };
        set_stage(progress, ParseStage::Publish);
        op.checkpoint_now()?;
        let main = if let Some(main) = verified_main {
            main
        } else {
            let mut main = None;
            for (ordinal, flow) in ir.flows().iter().enumerate() {
                op.events(1)?;
                if flow.scope == FlowScope::Main {
                    let id = FlowId::from_index(ordinal).map_err(|e| op.fail(e))?;
                    if main.replace(id).is_some() {
                        return Err(op.fail(CoreError::InternalInvariant));
                    }
                }
            }
            main.ok_or_else(|| op.fail(CoreError::InternalInvariant))?
        };
        let inline_emphasis = if config.extensions().contains(Extensions::HTML_INLINE) {
            inline_emphasis_runs(&ir, main, op)?
        } else {
            None
        };
        let footnotes = config.extensions().contains(Extensions::FOOTNOTES);
        let scripts = config.extensions().contains(Extensions::INLINE_SCRIPTS);
        let inline_scripts =
            if config.extensions().contains(Extensions::HTML_INLINE) || footnotes || scripts {
                inline_script_styles(&ir, main, footnotes, scripts, op)?
            } else {
                None
            };
        let disclosures = if config.extensions().contains(Extensions::CALLOUTS) {
            disclosure_regions(&ir, main, op)?
        } else {
            None
        };
        // Account for the Arc payload and header layout, not allocator overhead/RSS.
        let control_bytes = size_of::<ModelData>()
            .checked_add(2 * size_of::<usize>())
            .and_then(|n| n.checked_add(align_of::<ModelData>() - 1))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let control = op.reserve_auxiliary(control_bytes as u64)?;
        op.checkpoint_now()?;
        let model = Self(Arc::new(ModelData {
            ir,
            index,
            source,
            html: None,
            inline_emphasis,
            inline_scripts,
            disclosures,
            identity,
            main,
            _control: control,
        }));
        // parse_inner still owns this private value. OwnedWork::finish performs
        // the final same-flag Acquire/clock check before it can escape.
        Ok(model)
    }

    pub fn identity(&self) -> ModelIdentity {
        self.0.identity
    }
    /// Source for IR spans and payloads. HTML preview may project it from the
    /// host input exposed by original_source().
    pub fn source(&self) -> &SourceSnapshot {
        &self.0.source
    }
    pub fn ir(&self) -> &CompactIr {
        &self.0.ir
    }

    pub fn disclosure_regions(&self) -> &[DisclosureRegion] {
        self.0
            .disclosures
            .as_ref()
            .map_or(&[], FrozenRows::as_slice)
    }

    /// The source submitted by the host. The IR and its map use the projected
    /// source, so host queries must convert at the boundary.
    pub fn original_source(&self) -> &SourceSnapshot {
        self.0
            .html
            .as_ref()
            .map_or(self.source(), |html| &html.original)
    }

    pub fn original_source_byte(&self, byte: u64, affinity: Affinity) -> u64 {
        self.0
            .html
            .as_ref()
            .map_or(byte, |html| html.from_preview(byte, affinity))
    }

    pub fn html_alignment(&self, node: NodeId) -> u32 {
        self.0
            .html
            .as_ref()
            .map_or(0, |html| html.alignment(self, node))
    }

    pub(crate) fn html_picture_sources(&self, node: NodeId) -> [Option<&str>; 2] {
        self.0
            .html
            .as_ref()
            .map_or([None, None], |html| html.picture_sources(self, node))
    }

    pub fn html_image_size(&self, node: NodeId) -> (u32, u32) {
        self.0
            .html
            .as_ref()
            .map_or((0, 0), |html| html.image_size(self, node))
    }

    pub(crate) fn inline_emphasis_flags(
        &self,
        at: u64,
        op: &mut Operation,
    ) -> Result<u32, CoreError> {
        let Some(runs) = &self.0.inline_emphasis else {
            return Ok(0);
        };
        let runs = runs.as_slice();
        let (mut left, mut right) = (0, runs.len());
        while left < right {
            op.events(1)?;
            let mid = left + (right - left) / 2;
            if runs[mid].end <= at {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        Ok(runs
            .get(left)
            .filter(|run| run.start <= at)
            .map_or(0, |run| run.flags))
    }

    pub(crate) fn inline_script_style(
        &self,
        at: u64,
        op: &mut Operation,
    ) -> Result<ScriptStyle, CoreError> {
        let Some(scripts) = &self.0.inline_scripts else {
            return Ok(ScriptStyle::default());
        };
        let runs = scripts.runs.as_slice();
        let (mut left, mut right) = (0, runs.len());
        while left < right {
            op.events(1)?;
            let mid = left + (right - left) / 2;
            if runs[mid].end <= at {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        Ok(runs
            .get(left)
            .filter(|run| run.start <= at)
            .map_or_else(ScriptStyle::default, |run| run.style))
    }

    pub(crate) fn inline_script_scopes(
        &self,
        owner: NodeId,
        op: &mut Operation,
    ) -> Result<&[InlineScriptScope], CoreError> {
        let Some(scripts) = &self.0.inline_scripts else {
            return Ok(&[]);
        };
        let scopes = scripts.scopes.as_slice();
        let (mut left, mut right) = (0, scopes.len());
        while left < right {
            op.events(1)?;
            let mid = left + (right - left) / 2;
            if scopes[mid].owner.value() < owner.value() {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        let mut end = left;
        while end < scopes.len() {
            op.events(1)?;
            if scopes[end].owner != owner {
                break;
            }
            end += 1;
        }
        Ok(&scopes[left..end])
    }
    pub(crate) fn html_ruby(&self, span: LogicalSpan) -> &[crate::html::RubyDisplayUnit] {
        self.0.html.as_ref().map_or(&[], |html| html.ruby(span))
    }

    pub(crate) fn html_styles(&self, span: LogicalSpan) -> &[(std::ops::Range<u64>, u32)] {
        self.0.html.as_ref().map_or(&[], |html| html.styles(span))
    }

    pub fn budget(&self) -> &SourceBudget {
        self.0.source.budget()
    }

    pub fn main_flow_id(&self) -> FlowId {
        self.0.main
    }

    pub(crate) fn flow_atom_covering(
        &self,
        flow: FlowId,
        position: u64,
        op: &mut Operation,
    ) -> Result<crate::ir::AtomId, CoreError> {
        self.0.index.flow_atom_covering(flow, position, op)
    }

    pub fn flow_id(&self, value: u32) -> Result<FlowId, CoreError> {
        let index = value.checked_sub(1).ok_or(CoreError::UnknownId)? as usize;
        self.0.ir.flows().get(index).ok_or(CoreError::UnknownId)?;
        FlowId::from_index(index)
    }

    pub fn payload_id(&self, value: u32) -> Result<PayloadId, CoreError> {
        let index = value.checked_sub(1).ok_or(CoreError::UnknownId)? as usize;
        self.0
            .ir
            .payloads()
            .get(index)
            .ok_or(CoreError::UnknownId)?;
        PayloadId::from_index(index)
    }

    pub fn map_point(
        &self,
        query: &PointQuery,
        op: &mut Operation,
    ) -> Result<PointResponse, CoreError> {
        self.map_points(std::slice::from_ref(query), op)
    }

    pub fn map_points(
        &self,
        queries: &[PointQuery],
        op: &mut Operation,
    ) -> Result<PointResponse, CoreError> {
        if let Some(html) = &self.0.html {
            return self.0.index.map_projected_points(
                &self.0.ir,
                &self.0.source,
                queries,
                html,
                op,
            );
        }
        self.0
            .index
            .map_points(&self.0.ir, &self.0.source, queries, op)
    }

    pub fn map_range(
        &self,
        query: &RangeQuery,
        op: &mut Operation,
    ) -> Result<RangeResponse, CoreError> {
        self.map_ranges(std::slice::from_ref(query), op)
    }

    pub fn map_ranges(
        &self,
        queries: &[RangeQuery],
        op: &mut Operation,
    ) -> Result<RangeResponse, CoreError> {
        if let Some(html) = &self.0.html {
            return self.0.index.map_projected_ranges(
                &self.0.ir,
                &self.0.source,
                queries,
                html,
                op,
            );
        }
        self.0
            .index
            .map_ranges(&self.0.ir, &self.0.source, queries, op)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod progress_tests;

#[cfg(test)]
mod native_math_tests;

#[cfg(test)]
mod native_diagram_tests;

#[cfg(test)]
mod shape_boundary_tests {
    use super::*;
    use crate::config::Profile;
    use crate::error::LimitKind;
    use crate::identity::SourceKey;
    use crate::policy::{CancellationToken, WorkStats};

    fn model(budget: &SourceBudget, input: &str) -> AcceptedModel {
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([131; 16], 1).unwrap(),
            input.as_bytes(),
            &mut op,
        )
        .unwrap();
        AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
            1,
            &mut op,
        )
        .unwrap()
    }

    #[test]
    fn shape_boundaries_nonscript_gate_needs_no_metadata_storage_or_records() {
        let budget = SourceBudget::model_fixture();
        let model = model(&budget, "*[**a &amp; b**](#x)* `c` <b>d</b> <code>e</code>");
        let mut op = budget.begin(CancellationToken::new());
        op.admit_model(
            LimitKind::ModelRecords,
            budget.model_limits().unwrap().model_records,
        )
        .unwrap();
        let held = op
            .reserve_boundary(
                budget.limits().auxiliary_capacity - budget.usage().auxiliary_capacity,
            )
            .unwrap();
        let before = op.stats().unwrap();
        assert!(
            construction_scope(&mut op, |mut work| inline_script_styles(
                &model.0.ir,
                model.0.main,
                false,
                false,
                &mut work
            ))
            .unwrap()
            .is_none()
        );
        let after = op.stats().unwrap();
        assert_eq!(
            after.metadata_scanned_bytes(),
            before.metadata_scanned_bytes()
        );
        assert_eq!(
            after.metadata_copied_bytes(),
            before.metadata_copied_bytes()
        );
        drop((held, op, model));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
    #[test]
    fn shape_boundaries_admit_each_record_and_release_failed_construction() {
        let budget = SourceBudget::preview();
        let model = model(&budget, &format!("{} <sup>x</sup>", "**a** ".repeat(300)));
        let baseline = budget.usage().auxiliary_capacity;
        let mut op = budget.begin(CancellationToken::new());
        let maximum = budget.model_limits().unwrap().model_records;
        op.admit_model(LimitKind::ModelRecords, maximum - 2)
            .unwrap();
        assert!(
            matches!(construction_scope(&mut op, |mut work| inline_script_styles(&model.0.ir, model.0.main, false, false, &mut work)), Err(CoreError::ResourceLimit { kind: LimitKind::ModelRecords, requested, limit }) if requested == maximum + 1 && limit == maximum)
        );
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        drop((op, model));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn shape_boundaries_checkpoint_after_many_rows_and_unwind_on_deadline_or_cancel() {
        let budget = SourceBudget::preview();
        let model = model(
            &budget,
            &format!("{} <sup>x</sup>", "**a** [b](#x) ".repeat(300)),
        );
        let baseline = budget.usage().auxiliary_capacity;
        let mut reference = budget.begin(CancellationToken::new());
        reference.set_clock_samples_for_test(&[4_000_000_000]);
        let rows = construction_scope(&mut reference, |mut work| {
            inline_script_styles(&model.0.ir, model.0.main, false, false, &mut work)
        })
        .unwrap()
        .unwrap();
        assert_eq!(rows.scopes.len(), 601);
        let checks = reference.clock_read_count_for_test();
        assert!(checks > 8);
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        let mut failure = budget.begin(CancellationToken::new());
        let mut samples = vec![4_000_000_000; checks];
        samples[checks - 2] = 10_000_000_000;
        failure.set_clock_samples_for_test(&samples);
        assert!(matches!(
            construction_scope(&mut failure, |mut work| inline_script_styles(
                &model.0.ir,
                model.0.main,
                false,
                false,
                &mut work
            )),
            Err(CoreError::DeadlineExceeded { .. })
        ));
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut failure = budget.begin(cancel);
        assert!(matches!(
            construction_scope(&mut failure, |mut work| inline_script_styles(
                &model.0.ir,
                model.0.main,
                false,
                false,
                &mut work
            )),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
        drop((failure, reference, model));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn shape_boundaries_cache_shared_ancestors_with_linear_metered_work() {
        fn measure(repeats: usize) -> (usize, WorkStats) {
            let budget = SourceBudget::model_fixture();
            // Many siblings under the same nested Markdown parents; their Text
            // leaves and repeated atoms must stop at an already visited ancestor.
            let depth = repeats / 16;
            let input = format!(
                "{}{}{} <sup>x</sup>",
                "*a ".repeat(depth),
                "a&amp;b\\*c ".repeat(repeats),
                " z*".repeat(depth)
            );
            let model = model(&budget, &input);
            let mut op = budget.begin(CancellationToken::new());
            let count = construction_scope(&mut op, |mut work| {
                let ir = &model.0.ir;
                let maximum = work.model_limits().unwrap().model_records;
                let mut owners = BuildGrowingVec::new(maximum, LimitKind::ModelRecords, &mut work)?;
                for _ in 0..ir.nodes().len() {
                    owners.push(None::<Option<NodeId>>, &mut work)?;
                }
                let mut scopes = BuildGrowingVec::new(maximum, LimitKind::ModelRecords, &mut work)?;
                let flow = ir.flows().get(model.0.main.index()).unwrap();
                for at in flow.atoms.indices(ir.atoms_len())? {
                    let atom = ir.owned_read_atom_at(at, &mut work)?;
                    markdown_shape_scopes(ir, atom.owner, &mut owners, &mut scopes, &mut work)?;
                }
                Ok::<_, CoreError>(scopes.len())
            })
            .unwrap();
            let stats = op.stats().unwrap();
            assert!(
                stats.metadata_scanned_bytes() > 0
                    && stats.metadata_copied_bytes() > 0
                    && stats.events() > 0
            );
            drop((op, model));
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            (count, stats)
        }
        let small = measure(256);
        let large = measure(512);
        assert_eq!((small.0, large.0), (16, 32));
        assert!(large.1.total() > small.1.total());
        assert!(large.1.total() < small.1.total() * 22 / 10);
        assert!(large.1.metadata_scanned_bytes() < small.1.metadata_scanned_bytes() * 22 / 10);
    }
}
