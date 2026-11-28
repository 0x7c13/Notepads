// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::engine::{self, Outcome};
use super::storage::{GrowingRows, OwnedRows, OwnedText};
use super::work::Work;
use super::*;
use crate::ir::{DetailRow, NodeId, NodeKind};

pub(super) struct Buffers {
    pub rows: OwnedRows<HighlightBlock>,
    pub spans: GrowingRows<TokenSpan>,
}

pub(super) fn build(
    model: &AcceptedModel,
    op: &mut Operation,
) -> Result<(Buffers, HighlightSummary), CoreError> {
    build_with(model, op, engine::tokenize)
}

pub(super) fn build_with(
    model: &AcceptedModel,
    op: &mut Operation,
    tokenize: impl FnMut(
        Language,
        &str,
        u32,
        &Work<'_>,
        &mut GrowingRows<TokenSpan>,
    ) -> Result<Outcome, CoreError>,
) -> Result<(Buffers, HighlightSummary), CoreError> {
    build_internal(model, None, None, op, tokenize)
}

pub(super) fn build_selected(
    model: &AcceptedModel,
    nodes: &[u32],
    cache: &mut HighlightCache,
    op: &mut Operation,
) -> Result<(Buffers, HighlightSummary), CoreError> {
    op.ensure_budget(model.budget())?;
    op.ensure_budget(cache.budget())?;
    op.check_limit(
        crate::error::LimitKind::QueryCapacity,
        nodes.len() as u64,
        MAX_BLOCKS as u64,
    )?;
    let mut previous = 0;
    for &id in nodes {
        op.checkpoint_now()?;
        op.external_read(size_of::<u32>() as u64)?;
        if id <= previous {
            return Err(op.fail(CoreError::InvalidRange));
        }
        let Some(node) = model.ir().nodes().get((id - 1) as usize) else {
            return Err(op.fail(CoreError::InvalidRange));
        };
        op.view_scan(size_of_val(node) as u64)?;
        if node.kind != NodeKind::CodeBlock || node.flow != model.main_flow_id() {
            return Err(op.fail(CoreError::InvalidRange));
        }
        previous = id;
    }
    build_internal(model, Some(nodes), Some(cache), op, engine::tokenize)
}

fn build_internal(
    model: &AcceptedModel,
    selected: Option<&[u32]>,
    mut cache: Option<&mut HighlightCache>,
    op: &mut Operation,
    mut tokenize: impl FnMut(
        Language,
        &str,
        u32,
        &Work<'_>,
        &mut GrowingRows<TokenSpan>,
    ) -> Result<Outcome, CoreError>,
) -> Result<(Buffers, HighlightSummary), CoreError> {
    op.ensure_budget(model.budget())?;
    op.checkpoint_now()?;
    let mut rows = OwnedRows::new(selected.map_or(MAX_BLOCKS, <[u32]>::len), op)?;
    let work = Work::new(op);
    let mut entry_limit = false;
    let count = selected.map_or(model.ir().nodes().len(), <[u32]>::len);
    for ordinal in 0..count {
        let index = selected.map_or(ordinal, |nodes| (nodes[ordinal] - 1) as usize);
        let node = &model.ir().nodes()[index];
        work.checkpoint(1)?;
        if node.kind != NodeKind::CodeBlock || node.flow != model.main_flow_id() {
            continue;
        }
        if rows.values.len() == MAX_BLOCKS {
            entry_limit = true;
            break;
        }
        let language = work.with_op(|op| {
            op.view_scan(size_of_val(node) as u64)?;
            let detail = node
                .detail
                .and_then(|id| model.ir().details().get(id.index()).copied())
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            let DetailRow::Code { info, .. } = detail else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            let info = if let Some(id) = info {
                let payload = model
                    .ir()
                    .payloads()
                    .get(id.index())
                    .copied()
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                model.ir().text_query(payload.value, op)?
            } else {
                ""
            };
            for chunk in info
                .as_bytes()
                .chunks(crate::policy::CHECKPOINT_BYTES as usize)
            {
                op.checkpoint_now()?;
                op.external_read(chunk.len() as u64)?;
            }
            Ok(Language::from_info(info))
        })?;
        let row = HighlightBlock {
            node: NodeId::from_index(index)?,
            logical: node.logical,
            language,
            status: BlockStatus::Pending,
            spans: SpanRange::default(),
        };
        work.with_op(|op| rows.push(row, op))?;
    }
    let mut buffers = Buffers {
        rows,
        spans: GrowingRows::new(MAX_SPANS),
    };
    let mut summary = HighlightSummary {
        status: if entry_limit {
            SnapshotStatus::EntryLimit
        } else {
            SnapshotStatus::Complete
        },
        complete_blocks: 0,
        skipped_blocks: 0,
        failed_blocks: 0,
        reused_blocks: 0,
        participating_utf8: 0,
        external_events: 0,
    };
    for index in 0..buffers.rows.values.len() {
        let block = buffers.rows.values[index];
        let start = buffers.spans.len();
        let mut reused = 0;
        let result = build_block(
            model,
            block,
            &work,
            &mut buffers.spans,
            &mut summary.participating_utf8,
            cache.as_deref_mut(),
            &mut reused,
            &mut tokenize,
        );
        let status = match result {
            Ok(status) => status,
            Err(error) => {
                buffers.spans.truncate(start);
                summary.status = SnapshotStatus::Aborted(work.fail(error));
                break;
            }
        };
        if status != BlockStatus::Complete {
            buffers.spans.truncate(start);
        }
        let end = buffers.spans.len();
        // Commit metadata only after a successful same-operation observation/write admission.
        if let Err(error) = work.with_op(|op| {
            op.checkpoint_now()?;
            op.external_write(size_of::<HighlightBlock>() as u64)
        }) {
            buffers.spans.truncate(start);
            summary.status = SnapshotStatus::Aborted(error);
            break;
        }
        buffers.rows.values[index].status = status;
        buffers.rows.values[index].spans = SpanRange {
            start: start as u32,
            len: (end - start) as u32,
        };
        summary.reused_blocks += reused;
        match status {
            BlockStatus::Complete => summary.complete_blocks += 1,
            BlockStatus::Skipped(_) => summary.skipped_blocks += 1,
            BlockStatus::Failed(_) => summary.failed_blocks += 1,
            BlockStatus::Pending => return Err(CoreError::InternalInvariant),
        }
    }
    if !matches!(summary.status, SnapshotStatus::Aborted(_))
        && let Err(error) = work.checkpoint(0)
    {
        summary.status = SnapshotStatus::Aborted(error);
    }
    summary.external_events = work.events();
    Ok((buffers, summary))
}

fn build_block(
    model: &AcceptedModel,
    block: HighlightBlock,
    work: &Work<'_>,
    spans: &mut GrowingRows<TokenSpan>,
    participating: &mut u64,
    cache: Option<&mut HighlightCache>,
    reused: &mut u32,
    tokenize: &mut impl FnMut(
        Language,
        &str,
        u32,
        &Work<'_>,
        &mut GrowingRows<TokenSpan>,
    ) -> Result<Outcome, CoreError>,
) -> Result<BlockStatus, CoreError> {
    work.checkpoint(0)?;
    let Some(language) = block.language else {
        return Ok(BlockStatus::Skipped(SkipReason::UnknownLanguage));
    };
    // UTF8 byte length is at least its UTF16 length for every Unicode scalar.
    if block.logical.len() > MAX_BLOCK_UTF8 as u64 {
        return Ok(BlockStatus::Skipped(SkipReason::BlockBytes));
    }
    let mut count = Counts::default();
    work.with_op(|op| {
        crate::reader::text::visit(
            model,
            model.main_flow_id(),
            block.logical,
            op,
            |value, op| count.add(value, op, work),
        )
    })?;
    if count.line_bytes != 0 {
        count.lines += 1;
    }
    if count.lines > MAX_BLOCK_LINES {
        count.limited = Some(SkipReason::BlockLines);
    }
    if let Some(reason) = count.limited {
        return Ok(BlockStatus::Skipped(reason));
    }
    let total = participating
        .checked_add(count.bytes as u64)
        .ok_or(CoreError::ArithmeticOverflow)?;
    if total > MAX_PARTICIPATING_UTF8 as u64 {
        return Ok(BlockStatus::Skipped(SkipReason::DocumentBytes));
    }
    *participating = total;
    let mut code = work.with_op(|op| OwnedText::new(count.bytes, op))?;
    work.with_op(|op| {
        crate::reader::text::visit(
            model,
            model.main_flow_id(),
            block.logical,
            op,
            |value, op| {
                work.charge(op, 1)?;
                code.append(value, op)
            },
        )
    })?;
    if code.text.len() != count.bytes {
        return work.with_op(|op| Err(op.fail(CoreError::InvalidProvenance)));
    }
    let expected = u32::try_from(block.logical.len()).map_err(|_| CoreError::ArithmeticOverflow)?;
    let start = spans.len();
    if let Some(cache) = cache {
        if let Some(result) = cache.copy(language, &code.text, work, spans)? {
            if result == Outcome::Complete {
                *reused += 1;
            }
            return Ok(match result {
                Outcome::Complete => BlockStatus::Complete,
                Outcome::SpanLimit => BlockStatus::Skipped(SkipReason::SpanCount),
                Outcome::Failed(reason) => BlockStatus::Failed(reason),
            });
        }
        let result = tokenize(language, &code.text, expected, work, spans)?;
        if result == Outcome::Complete {
            cache.insert(language, code, &spans.values()[start..], work)?;
        }
        return Ok(match result {
            Outcome::Complete => BlockStatus::Complete,
            Outcome::SpanLimit => BlockStatus::Skipped(SkipReason::SpanCount),
            Outcome::Failed(reason) => BlockStatus::Failed(reason),
        });
    }
    let result = tokenize(language, &code.text, expected, work, spans)?;
    Ok(match result {
        Outcome::Complete => BlockStatus::Complete,
        Outcome::SpanLimit => BlockStatus::Skipped(SkipReason::SpanCount),
        Outcome::Failed(reason) => BlockStatus::Failed(reason),
    })
}

#[derive(Default)]
struct Counts {
    bytes: usize,
    lines: usize,
    line_bytes: usize,
    limited: Option<SkipReason>,
}
impl Counts {
    fn add(&mut self, value: &str, op: &mut Operation, work: &Work<'_>) -> Result<(), CoreError> {
        if self.limited.is_some() {
            return Ok(());
        }
        work.charge(op, 1)?;
        self.bytes = self
            .bytes
            .checked_add(value.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        if self.bytes > MAX_BLOCK_UTF8 {
            self.limited = Some(SkipReason::BlockBytes);
            return Ok(());
        }
        for chunk in value.as_bytes().chunks(4096) {
            op.checkpoint_now()?;
            op.external_read(chunk.len() as u64)?;
            for byte in chunk {
                self.line_bytes += 1;
                if self.line_bytes > MAX_LINE_UTF8 {
                    self.limited = Some(SkipReason::LineBytes);
                    return Ok(());
                }
                if *byte == b'\n' {
                    self.lines += 1;
                    self.line_bytes = 0;
                    if self.lines > MAX_BLOCK_LINES {
                        self.limited = Some(SkipReason::BlockLines);
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}
