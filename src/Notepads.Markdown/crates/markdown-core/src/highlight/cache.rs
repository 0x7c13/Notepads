// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::engine::Outcome;
use super::storage::{GrowingRows, OwnedRows, OwnedText};
use super::work::Work;
use super::{Language, MAX_BLOCKS, TokenSpan};
use crate::error::{CoreError, LimitKind};
use crate::model::AcceptedModel;
use crate::policy::{CHECKPOINT_BYTES, SourceBudget};

/// Maximum combined retained text/token capacities, independently of snapshot storage.
pub const MAX_CACHE_BYTES: usize = 512 * 1024;

struct Entry {
    text: OwnedText,
    spans: OwnedRows<TokenSpan>,
    language: Language,
    age: u8,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.text.text.capacity() + self.spans.values.capacity() * size_of::<TokenSpan>()
    }
}

/// Serialized, root-funded exact lexical reuse across viewport requests and edits.
/// Entries retain no source/model, cancellation flag, node ID, or logical position.
/// Only complete results are retained; operation-dependent skips/failures are retried.
pub struct HighlightCache {
    // Release allocations/reservations before their root owner.
    entries: [Option<Entry>; MAX_BLOCKS],
    budget: SourceBudget,
}

impl HighlightCache {
    pub fn new(model: &AcceptedModel) -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            budget: model.budget().clone(),
        }
    }

    pub(super) fn budget(&self) -> &SourceBudget {
        &self.budget
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.entries.iter().flatten().map(Entry::bytes).sum()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.iter().flatten().count()
    }

    fn touch(&mut self, index: usize) {
        let previous_age = self.entries[index]
            .as_ref()
            .map_or(u8::MAX, |entry| entry.age);
        for (slot, entry) in self.entries.iter_mut().enumerate() {
            if let Some(entry) = entry {
                if slot == index {
                    entry.age = 0;
                } else if entry.age < previous_age {
                    entry.age = entry.age.saturating_add(1);
                }
            }
        }
    }

    pub(super) fn copy(
        &mut self,
        language: Language,
        code: &str,
        work: &Work<'_>,
        output: &mut GrowingRows<TokenSpan>,
    ) -> Result<Option<Outcome>, CoreError> {
        let mut found = None;
        for (index, entry) in self.entries.iter().enumerate() {
            work.checkpoint(1)?;
            let Some(entry) = entry else { continue };
            if entry.language != language || entry.text.text.len() != code.len() {
                continue;
            }
            let mut equal = true;
            for (old, new) in entry
                .text
                .text
                .as_bytes()
                .chunks(4096)
                .zip(code.as_bytes().chunks(4096))
            {
                work.with_op(|op| {
                    op.checkpoint_now()?;
                    op.external_read((old.len() + new.len()) as u64)
                })?;
                if old != new {
                    equal = false;
                    break;
                }
            }
            if equal {
                found = Some(index);
                break;
            }
        }
        let Some(index) = found else { return Ok(None) };
        self.touch(index);
        let entry = self.entries[index]
            .as_ref()
            .ok_or(CoreError::InternalInvariant)?;
        for chunk in entry
            .spans
            .values
            .chunks((CHECKPOINT_BYTES as usize / size_of::<TokenSpan>()).max(1))
        {
            work.with_op(|op| op.external_read(size_of_val(chunk) as u64))?;
            for &span in chunk {
                if !work.with_op(|op| output.push(span, op))? {
                    return Ok(Some(Outcome::SpanLimit));
                }
            }
        }
        Ok(Some(Outcome::Complete))
    }

    fn make_room(&mut self, bytes: usize) {
        while self.retained_bytes() + bytes > MAX_CACHE_BYTES
            || self.entries.iter().all(Option::is_some)
        {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| entry.as_ref().map(|entry| (index, entry.age)))
                .max_by_key(|&(_, age)| age);
            let Some((index, _)) = oldest else { break };
            self.entries[index] = None;
        }
    }

    pub(super) fn insert(
        &mut self,
        language: Language,
        text: OwnedText,
        spans: &[TokenSpan],
        work: &Work<'_>,
    ) -> Result<(), CoreError> {
        let bytes = text
            .text
            .capacity()
            .checked_add(size_of_val(spans))
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        if bytes > MAX_CACHE_BYTES {
            return Ok(());
        }
        work.checkpoint(1)?;
        // Retention is optional. Its allocation alone has an independent operation,
        // sharing this root and the exact request cancellation flag. Charge its
        // admission event and all data-copy work to the original request; only
        // optional OOM/quota rejection stays off that request's first fault.
        // Keep existing entries until the candidate is fully admitted.
        let cancel = work.with_op(|op| {
            op.external_events(1)?;
            Ok(op.cancellation())
        })?;
        let mut allocation = self.budget.begin(cancel);
        let owned = OwnedRows::new(spans.len(), &mut allocation);
        work.checkpoint(0)?;
        let mut owned = match owned {
            Ok(owned) => owned,
            Err(
                CoreError::OutOfMemory
                | CoreError::ResourceLimit {
                    kind: LimitKind::AuxiliaryCapacity,
                    ..
                },
            ) => return Ok(()),
            Err(error) => return Err(work.fail(error)),
        };
        for chunk in spans.chunks((CHECKPOINT_BYTES as usize / size_of::<TokenSpan>()).max(1)) {
            work.with_op(|op| op.external_read(size_of_val(chunk) as u64))?;
            for &span in chunk {
                work.with_op(|op| owned.push(span, op))?;
            }
        }
        let entry = Entry {
            text,
            spans: owned,
            language,
            age: u8::MAX,
        };
        if entry.bytes() > MAX_CACHE_BYTES {
            return Ok(());
        }
        self.make_room(entry.bytes());
        let index = self
            .entries
            .iter()
            .position(Option::is_none)
            .ok_or(CoreError::InternalInvariant)?;
        self.entries[index] = Some(entry);
        self.touch(index);
        Ok(())
    }
}
