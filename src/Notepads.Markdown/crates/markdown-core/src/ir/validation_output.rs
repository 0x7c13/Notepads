// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private products of independent validation. No producer can grant this authority.

use super::{AtomId, CompactIr, FlowId, Rows};
use crate::error::{CoreError, LimitKind};
use crate::identity::{ModelIdentity, SourceSpan};
use crate::memory::{BuildExactRows, BuildGrowingVec};
use crate::policy::{OwnedWork, ScalarBatchEnd, ScalarChargeOrder};
use crate::source::SourceSnapshot;
use std::ops::ControlFlow;

pub(crate) const SPARSE_BYTES: u64 = 1024;

#[derive(Clone, Copy)]
pub(crate) struct Checkpoint {
    pub(crate) byte: u64,
    pub(crate) utf16: u64,
}

impl crate::memory::BuildValue for Checkpoint {}

#[derive(Default)]
pub(crate) struct FixedIndexCounts {
    pub(crate) targets: usize,
    pub(crate) source: usize,
    pub(crate) synthetic: usize,
    pub(crate) target: usize,
    pub(crate) hidden: usize,
    pub(crate) sparse: usize,
}

impl FixedIndexCounts {
    pub(crate) fn total<'id>(&self, op: &mut OwnedWork<'id, '_>) -> Result<u64, CoreError> {
        let mut total = 0_u64;
        for count in [
            self.targets,
            self.source,
            self.synthetic,
            self.target,
            self.hidden,
            self.sparse,
        ] {
            op.events(1)?;
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        }
        Ok(total)
    }

    pub(crate) fn total_with_checkpoints<'id>(
        &self,
        checkpoints: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<u64, CoreError> {
        let fixed = self.total(op)?;
        op.events(1)?;
        fixed
            .checked_add(checkpoints as u64)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))
    }
}

fn increment(value: &mut usize) -> Result<(), CoreError> {
    *value = value.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
    Ok(())
}

/// Unpublished final storage. It gains no query authority before full validation.
pub(super) struct DraftFacts<'id> {
    counts: FixedIndexCounts,
    sparse: BuildExactRows<'id, Rows<Checkpoint>>,
    checkpoints: BuildGrowingVec<'id, Checkpoint>,
}

impl<'id> DraftFacts<'id> {
    pub(super) fn new(atoms: usize, op: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        let maximum = op
            .model_limits()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?
            .index_records;
        let mut sparse = BuildExactRows::new(atoms, op)?;
        for _ in 0..atoms {
            op.admit_model(LimitKind::IndexRecords, 1)?;
            sparse.push(Rows::new(0, 0).map_err(|e| op.fail(e))?, op)?;
        }
        Ok(Self {
            counts: FixedIndexCounts {
                sparse: atoms,
                ..FixedIndexCounts::default()
            },
            sparse,
            checkpoints: BuildGrowingVec::new(maximum, LimitKind::IndexRecords, op)?,
        })
    }

    pub(super) fn source(&mut self, span: SourceSpan) -> Result<(), CoreError> {
        increment(if span.is_empty() {
            &mut self.counts.synthetic
        } else {
            &mut self.counts.source
        })
    }

    pub(super) fn atom(&mut self, positive: bool) -> Result<(), CoreError> {
        increment(if positive {
            &mut self.counts.target
        } else {
            &mut self.counts.hidden
        })
    }

    pub(super) fn target(&mut self) -> Result<(), CoreError> {
        increment(&mut self.counts.targets)
    }

    pub(super) fn checkpoint_len(&self) -> usize {
        self.checkpoints.len()
    }

    pub(super) fn checkpoint(
        &mut self,
        value: Checkpoint,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        op.admit_model(LimitKind::IndexRecords, 1)?;
        self.checkpoints.push(value, op)
    }

    pub(super) fn atom_checkpoints(
        &mut self,
        id: AtomId,
        begin: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let length = self
            .checkpoints
            .len()
            .checked_sub(begin)
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        self.sparse.replace(
            id.index(),
            Rows::new(begin, length).map_err(|e| op.fail(e))?,
            op,
        )
    }

    pub(super) fn logical_boundary(
        &self,
        id: AtomId,
        text: &str,
        wanted: u64,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let rows = self
            .sparse
            .get(id.index(), op)?
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        rows.indices(self.checkpoints.len())
            .map_err(|error| op.fail(error))?;
        let (mut left, mut right) = (0, rows.len());
        while left < right {
            op.events(1)?;
            let middle = left + (right - left) / 2;
            if super::storage::owned_row(&self.checkpoints, rows.start() + middle, op)?.utf16
                <= wanted
            {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        let checkpoint = if left == 0 {
            Checkpoint { byte: 0, utf16: 0 }
        } else {
            super::storage::owned_row(&self.checkpoints, rows.start() + left - 1, op)?
        };
        if checkpoint.utf16 == wanted {
            return Ok(());
        }
        let start =
            usize::try_from(checkpoint.byte).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let end = usize::try_from(if left < rows.len() {
            super::storage::owned_row(&self.checkpoints, rows.start() + left, op)?.byte
        } else {
            text.len() as u64
        })
        .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        if end < start || end - start > SPARSE_BYTES as usize + 3 {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        op.scan(
            u64::from(start > 0 && start < text.len()) + u64::from(end > 0 && end < text.len()),
        )?;
        let window = text
            .get(start..end)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let mut units = checkpoint.utf16;
        let mut scalars = window.chars();
        loop {
            match op.visit_scalar_batch(
                &mut scalars,
                ScalarChargeOrder::ScanThenEvent,
                |scalar| {
                    units = units
                        .checked_add(scalar.len_utf16() as u64)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    if units >= wanted {
                        if units != wanted {
                            return Err(CoreError::InvalidProvenance);
                        }
                        Ok(ControlFlow::Break(()))
                    } else {
                        Ok(ControlFlow::Continue(()))
                    }
                },
            )? {
                ScalarBatchEnd::Full => {}
                ScalarBatchEnd::Break => return Ok(()),
                ScalarBatchEnd::Exhausted => return Err(op.fail(CoreError::InvalidProvenance)),
            }
        }
    }

    pub(super) fn complete(&self, op: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        self.sparse.complete(op)
    }

    pub(super) fn seal<'ir, 'src, 'call, 'op>(
        self,
        ir: &'ir CompactIr,
        source: &'src SourceSnapshot,
        identity: ModelIdentity,
        main: FlowId,
        op: &'call mut OwnedWork<'id, 'op>,
        _seal: super::validate::ValidationSeal,
    ) -> VerifiedFacts<'ir, 'src, 'call, 'id, 'op> {
        VerifiedFacts {
            parts: VerifiedParts {
                ir,
                source,
                identity,
                op,
                main,
                counts: self.counts,
                sparse: self.sparse,
                checkpoints: self.checkpoints,
            },
        }
    }
}

/// Nonclone authority tied to the exact immutable inputs and the original operation.
pub(crate) struct VerifiedFacts<'ir, 'src, 'call, 'id, 'op> {
    parts: VerifiedParts<'ir, 'src, 'call, 'id, 'op>,
}

pub(crate) struct VerifiedParts<'ir, 'src, 'call, 'id, 'op> {
    pub(crate) ir: &'ir CompactIr,
    pub(crate) source: &'src SourceSnapshot,
    pub(crate) identity: ModelIdentity,
    pub(crate) op: &'call mut OwnedWork<'id, 'op>,
    pub(crate) main: FlowId,
    pub(crate) counts: FixedIndexCounts,
    pub(crate) sparse: BuildExactRows<'id, Rows<Checkpoint>>,
    pub(crate) checkpoints: BuildGrowingVec<'id, Checkpoint>,
}

impl<'ir, 'src, 'call, 'id, 'op> VerifiedFacts<'ir, 'src, 'call, 'id, 'op> {
    pub(crate) fn into_parts(self) -> VerifiedParts<'ir, 'src, 'call, 'id, 'op> {
        self.parts
    }
}

#[cfg(test)]
impl VerifiedFacts<'_, '_, '_, '_, '_> {
    pub(crate) fn counts_for_test(&self) -> [usize; 7] {
        [
            self.parts.counts.targets,
            self.parts.counts.source,
            self.parts.counts.synthetic,
            self.parts.counts.target,
            self.parts.counts.hidden,
            self.parts.counts.sparse,
            self.parts.checkpoints.len(),
        ]
    }

    pub(crate) fn storage_for_test(&self) -> (usize, usize, usize, usize) {
        (
            self.parts.sparse.storage_identity_for_test().0,
            self.parts.sparse.len(),
            self.parts.checkpoints.storage_identity_for_test().0,
            self.parts.checkpoints.len(),
        )
    }

    pub(crate) fn set_clock_samples_for_test(&mut self, samples: &[u64]) {
        self.parts.op.set_clock_samples_for_test(samples);
    }
}
