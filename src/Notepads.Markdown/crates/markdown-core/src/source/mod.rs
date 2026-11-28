// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Owned original UTF-8, atomic same-revision edits, and bounded N01 input preparation.
//! This module does not parse Markdown or provide semantic source mappings.

mod normalize;
mod utf16;
mod utf8;

pub use normalize::{InputPiece, InputPieceKind, ParseInput};

use crate::error::{CoreError, LimitKind};
use crate::identity::{SourceKey, SourceSpan};
use crate::memory::{ChargedString, OwnerLease};
use crate::policy::{Operation, SourceBudget};
use std::sync::Arc;

struct SnapshotData {
    // Rust drops fields in declaration order: free S before retiring its owner count.
    text: ChargedString,
    _owner: OwnerLease,
    budget: SourceBudget,
    key: SourceKey,
}

/// Clones retain the same immutable allocation and its original quota root.
#[derive(Clone)]
pub struct SourceSnapshot(Arc<SnapshotData>);

impl SourceSnapshot {
    pub fn key(&self) -> SourceKey {
        self.0.key
    }

    pub fn text(&self) -> &str {
        self.0.text.as_str()
    }

    pub fn validate_span(&self, span: SourceSpan) -> Result<(), CoreError> {
        let start = usize::try_from(span.start()).map_err(|_| CoreError::ArithmeticOverflow)?;
        let end = usize::try_from(span.end()).map_err(|_| CoreError::ArithmeticOverflow)?;
        if end > self.text().len()
            || !self.text().is_char_boundary(start)
            || !self.text().is_char_boundary(end)
        {
            return Err(CoreError::InvalidBoundary);
        }
        Ok(())
    }

    pub(crate) fn budget(&self) -> &SourceBudget {
        &self.0.budget
    }
}

/// Exclusive current source; readers retain independent immutable snapshots.
pub struct SourceDocument {
    current: SourceSnapshot,
}

pub struct Edit<'a> {
    pub range: SourceSpan,
    pub replacement: &'a [u8],
}

pub struct EditBatch<'a> {
    pub before: SourceKey,
    pub after: SourceKey,
    pub edits: &'a [Edit<'a>],
}

impl SourceDocument {
    pub fn new_utf16(key: SourceKey, input: &[u16], op: &mut Operation) -> Result<Self, CoreError> {
        Self::new_utf16_matching(key, input, None, op)
    }

    pub(crate) fn new_utf16_matching(
        key: SourceKey,
        input: &[u16],
        matching: Option<&SourceSnapshot>,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        op.checkpoint_now()?;
        // Every valid u16 contributes at least one UTF8 byte. This also bounds
        // the input before either pass, without guessing an expansion factor.
        check_source_length(input.len(), op)?;
        let mut length = 0_usize;
        let mut equal = true;
        utf16::visit(input, op, |chunk, op| {
            let end = length
                .checked_add(chunk.len())
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            check_source_length(end, op)?;
            if let Some(current) = matching {
                op.scan(chunk.len() as u64)?;
                equal &= current.text().as_bytes().get(length..end) == Some(chunk.as_bytes());
            }
            length = end;
            Ok(())
        })?;
        if let Some(current) = matching
            && (!equal || current.text().len() != length)
        {
            // The caller matches snapshots by key. Equal revisions here signal
            // that the request reused the current revision for different text.
            return Err(op.fail(CoreError::WrongRevision {
                expected: current.key().revision(),
                actual: key.revision(),
            }));
        }
        let owner = OwnerLease::new(op)?;
        let mut text = ChargedString::source(length, op)?;
        utf16::visit(input, op, |chunk, op| text.append(chunk, op))?;
        if text.as_str().len() != length {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        Ok(Self {
            current: snapshot(key, text, owner, op)?,
        })
    }

    pub fn new(key: SourceKey, utf8: &[u8], op: &mut Operation) -> Result<Self, CoreError> {
        op.checkpoint_now()?;
        check_source_length(utf8.len(), op)?;
        let owner = OwnerLease::new(op)?;
        let mut text = ChargedString::source(utf8.len(), op)?;
        utf8::visit(utf8, None, op, |chunk, op| text.append(chunk, op))?;
        Ok(Self {
            current: snapshot(key, text, owner, op)?,
        })
    }

    pub fn snapshot(&self) -> SourceSnapshot {
        self.current.clone()
    }

    pub fn apply(
        &mut self,
        batch: EditBatch<'_>,
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        op.checkpoint_now()?;
        op.ensure_budget(self.current.budget())?;
        self.check_keys(&batch, op)?;
        let limits = *op.budget().limits();
        let edit_count =
            u64::try_from(batch.edits.len()).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(LimitKind::Edits, edit_count, limits.max_edits)?;

        let mut previous = None;
        let mut removed = 0_u64;
        let mut inserted = 0_u64;
        for (index, edit) in batch.edits.iter().enumerate() {
            op.events(1)?;
            if let Some((start, end)) = previous
                && (edit.range.start() < end || edit.range.start() == start)
            {
                return Err(op.fail(CoreError::InvalidEdits));
            }
            self.current
                .validate_span(edit.range)
                .map_err(|error| op.fail(error))?;
            previous = Some((edit.range.start(), edit.range.end()));
            removed = removed
                .checked_add(edit.range.len())
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            inserted = inserted
                .checked_add(
                    u64::try_from(edit.replacement.len())
                        .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                )
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            op.check_limit(LimitKind::InsertBytes, inserted, limits.max_insert_bytes)?;
            let index = u64::try_from(index).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            utf8::visit(edit.replacement, Some(index), op, |_, _| Ok(()))?;
        }
        let final_len = u64::try_from(self.current.text().len())
            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?
            .checked_sub(removed)
            .and_then(|remaining| remaining.checked_add(inserted))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(LimitKind::SourceBytes, final_len, limits.max_source_bytes)?;
        let final_len =
            usize::try_from(final_len).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;

        // Every record and replacement has been validated. Build one after-S in source order.
        let owner = OwnerLease::new(op)?;
        let mut text = ChargedString::source(final_len, op)?;
        let mut cursor = 0;
        for (index, edit) in batch.edits.iter().enumerate() {
            op.events(1)?;
            let start = usize::try_from(edit.range.start())
                .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            let end = usize::try_from(edit.range.end())
                .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            utf8::copy(&self.current.text()[cursor..start], &mut text, op)?;
            let index = u64::try_from(index).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            // The second bounded validation is required to obtain safe &str slices; it is metered.
            utf8::visit(edit.replacement, Some(index), op, |chunk, op| {
                text.append(chunk, op)
            })?;
            cursor = end;
        }
        utf8::copy(&self.current.text()[cursor..], &mut text, op)?;
        if text.as_str().len() != final_len {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let next = snapshot(batch.after, text, owner, op)?;
        op.checkpoint_now()?;
        self.current = next.clone();
        Ok(next)
    }

    fn check_keys(&self, batch: &EditBatch<'_>, op: &mut Operation) -> Result<(), CoreError> {
        let current = self.current.key();
        if batch.before.document() != current.document()
            || batch.after.document() != current.document()
        {
            return Err(op.fail(CoreError::WrongDocument));
        }
        if batch.before.revision() < current.revision() {
            return Err(op.fail(CoreError::WrongRevision {
                expected: current.revision(),
                actual: batch.before.revision(),
            }));
        }
        if batch.before.revision() > current.revision() {
            return Err(op.fail(CoreError::ResyncRequired {
                expected: current.revision(),
                actual: batch.before.revision(),
            }));
        }
        if batch.after.revision() <= current.revision() {
            return Err(op.fail(CoreError::InvalidRevision));
        }
        Ok(())
    }
}

fn check_source_length(length: usize, op: &mut Operation) -> Result<(), CoreError> {
    let length = u64::try_from(length).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
    let limit = op.budget().limits().max_source_bytes;
    op.check_limit(LimitKind::SourceBytes, length, limit)
}

fn snapshot(
    key: SourceKey,
    text: ChargedString,
    owner: OwnerLease,
    op: &mut Operation,
) -> Result<SourceSnapshot, CoreError> {
    // Also preserve buffer-before-owner retirement if the pre-allocation
    // checkpoint fails before an Arc exists to own this draft.
    let data = SnapshotData {
        text,
        _owner: owner,
        budget: op.budget().clone(),
        key,
    };
    op.checkpoint_now()?;
    let snapshot = SourceSnapshot(Arc::new(data));
    op.checkpoint_now()?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests;
