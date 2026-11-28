// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Node fields have one owner from initialization through immutable publication.

use super::storage::guarded_callback;
use super::{DetailId, FlowId, NodeId, NodeKind, NodeRow};
use crate::error::{CoreError, LimitKind};
use crate::identity::SourceSpan;
use crate::memory::{BuildRows, FrozenPagedRows};
use crate::policy::OwnedWork;

#[cfg(test)]
mod tests;

const PENDING: u8 = 0;
const WRITTEN: u8 = 1;
const INITIAL: u8 = 2;

pub(super) struct NodeRows<'id> {
    values: BuildRows<'id, NodeRow>,
    states: BuildRows<'id, u8>,
    pending: usize,
}

/// Only a checked consuming freeze can create this actual completed page owner.
pub(super) struct CompletedNodePages {
    pages: FrozenPagedRows<NodeRow>,
}

/// One consuming transition retires node construction state before its final
/// consumers. The unavailable variant owns no rows and grants no read access.
pub(super) enum NodeStorage<'id> {
    Building(NodeRows<'id>),
    Completed(CompletedNodePages),
    Unavailable(usize),
}

impl CompletedNodePages {
    pub(super) fn into_pages(self) -> FrozenPagedRows<NodeRow> {
        self.pages
    }

    fn read(&self, index: usize, work: &mut OwnedWork<'_, '_>) -> Result<NodeRow, CoreError> {
        work.check()?;
        work.events(1)?;
        work.read_metadata(size_of::<NodeRow>() as u64, || {
            self.pages
                .get(index)
                .copied()
                .ok_or(CoreError::InvalidProvenance)
        })
    }
}

impl<'id> NodeStorage<'id> {
    pub(super) fn new(max: u64, work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        NodeRows::new(max, work).map(Self::Building)
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::Building(rows) => rows.len(),
            Self::Completed(rows) => rows.pages.len(),
            Self::Unavailable(len) => *len,
        }
    }

    fn building(&self, work: &mut OwnedWork<'id, '_>) -> Result<&NodeRows<'id>, CoreError> {
        work.check()?;
        match self {
            Self::Building(rows) => Ok(rows),
            _ => Err(work.fail(CoreError::InvalidProvenance)),
        }
    }

    fn building_mut(
        &mut self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<&mut NodeRows<'id>, CoreError> {
        work.check()?;
        match self {
            Self::Building(rows) => Ok(rows),
            _ => Err(work.fail(CoreError::InvalidProvenance)),
        }
    }

    pub(super) fn reserve(
        &mut self,
        seed: NodeRow,
        has_initial: bool,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<usize, CoreError> {
        self.building_mut(work)?.reserve(seed, has_initial, work)
    }

    pub(super) fn graph_row(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<NodeRow, CoreError> {
        work.check()?;
        match self {
            Self::Building(rows) => rows.graph_row(index, work),
            Self::Completed(rows) => rows.read(index, work),
            Self::Unavailable(_) => Err(work.fail(CoreError::InvalidProvenance)),
        }
    }

    pub(super) fn initial(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<SourceSpan>, CoreError> {
        self.building(work)?.initial(index, work)
    }

    pub(super) fn set_initial(
        &mut self,
        index: usize,
        initial: Option<SourceSpan>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        self.building_mut(work)?.set_initial(index, initial, work)
    }

    pub(super) fn write(
        &mut self,
        index: usize,
        row: NodeRow,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        self.building_mut(work)?.write(index, row, work)
    }

    pub(super) fn is_written(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        work.check()?;
        match self {
            Self::Building(rows) => rows.is_written(index, work),
            Self::Completed(rows) => {
                work.events(1)?;
                work.view_scan(size_of::<usize>() as u64)?;
                if index >= rows.pages.len() {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                Ok(true)
            }
            Self::Unavailable(_) => Err(work.fail(CoreError::InvalidProvenance)),
        }
    }

    pub(super) fn get(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<NodeRow, CoreError> {
        work.check()?;
        match self {
            Self::Building(rows) => rows.get(index, work),
            Self::Completed(rows) => rows.read(index, work),
            Self::Unavailable(_) => Err(work.fail(CoreError::InvalidProvenance)),
        }
    }

    pub(super) fn complete(&mut self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        guarded_callback(work, |work| {
            work.check()?;
            if !matches!(self, Self::Building(_)) {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let len = self.len();
            let old = std::mem::replace(self, Self::Unavailable(len));
            let Self::Building(rows) = old else {
                return Err(work.fail(CoreError::InternalInvariant));
            };
            let pages = rows.freeze(work)?;
            *self = Self::Completed(pages);
            work.checkpoint_now()
        })
    }

    pub(super) fn freeze(
        self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<CompletedNodePages, CoreError> {
        guarded_callback(work, |work| match self {
            Self::Building(rows) => rows.freeze(work),
            Self::Completed(rows) => {
                work.checkpoint_now()?;
                Ok(rows)
            }
            Self::Unavailable(_) => Err(work.fail(CoreError::InvalidProvenance)),
        })
    }
}

impl<'id> NodeRows<'id> {
    pub(super) fn new(max: u64, work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        Ok(Self {
            values: BuildRows::new(max, LimitKind::ModelRecords, work)?,
            states: BuildRows::new(max, LimitKind::ModelRecords, work)?,
            pending: 0,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    fn state(&self, index: usize, work: &mut OwnedWork<'id, '_>) -> Result<u8, CoreError> {
        work.check()?;
        if self.values.len() != self.states.len() || index >= self.len() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let state = self.states.get(index, work)?;
        if !matches!(state, PENDING | WRITTEN | INITIAL) {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(state)
    }

    pub(super) fn reserve(
        &mut self,
        seed: NodeRow,
        has_initial: bool,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<usize, CoreError> {
        guarded_callback(work, |work| {
            if self.values.len() != self.states.len() {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let pending = self
                .pending
                .checked_add(1)
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
            let index = self.values.push(seed, work)?;
            let state = if has_initial { INITIAL } else { PENDING };
            if self.states.push(state, work)? != index {
                return Err(work.fail(CoreError::InternalInvariant));
            }
            self.pending = pending;
            Ok(index)
        })
    }

    pub(super) fn is_written(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        Ok(self.state(index, work)? == WRITTEN)
    }

    /// Construction-only copied fields. This does not grant final read authority.
    pub(super) fn graph_row(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<NodeRow, CoreError> {
        self.state(index, work)?;
        self.values.get(index, work)
    }

    pub(super) fn initial(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<SourceSpan>, CoreError> {
        match self.state(index, work)? {
            PENDING => Ok(None),
            INITIAL => Ok(Some(self.values.get(index, work)?.source_envelope)),
            _ => Err(work.fail(CoreError::InvalidProvenance)),
        }
    }

    pub(super) fn set_initial(
        &mut self,
        index: usize,
        initial: Option<SourceSpan>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        guarded_callback(work, |work| {
            if self.state(index, work)? == WRITTEN {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let mut row = self.values.get(index, work)?;
            row.source_envelope = initial.unwrap_or(SourceSpan::new(0, 0)?);
            self.values.replace(index, row, work)?;
            self.states.replace(
                index,
                if initial.is_some() { INITIAL } else { PENDING },
                work,
            )
        })
    }

    pub(super) fn write(
        &mut self,
        index: usize,
        row: NodeRow,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        guarded_callback(work, |work| {
            if self.state(index, work)? == WRITTEN {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let seed = self.values.get(index, work)?;
            work.view_scan(
                (size_of::<Option<NodeId>>()
                    + size_of::<FlowId>()
                    + size_of::<NodeKind>()
                    + size_of::<Option<DetailId>>()) as u64,
            )?;
            if (row.parent, row.flow, row.kind, row.detail)
                != (seed.parent, seed.flow, seed.kind, seed.detail)
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let pending = self
                .pending
                .checked_sub(1)
                .ok_or_else(|| work.fail(CoreError::InternalInvariant))?;
            self.values.replace(index, row, work)?;
            self.states.replace(index, WRITTEN, work)?;
            self.pending = pending;
            Ok(())
        })
    }

    pub(super) fn get(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<NodeRow, CoreError> {
        if !self.is_written(index, work)? {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        self.values.get(index, work)
    }

    pub(super) fn ensure_written(&self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        work.check()?;
        work.events(1)?;
        if self.values.len() != self.states.len() || self.pending != 0 {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        for index in 0..self.states.len() {
            if self.state(index, work)? != WRITTEN {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
        }
        Ok(())
    }

    pub(super) fn freeze(
        self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<CompletedNodePages, CoreError> {
        guarded_callback(work, |work| {
            self.ensure_written(work)?;
            let Self { values, states, .. } = self;
            let pages = values.freeze(work)?;
            drop(states);
            work.checkpoint_now()?;
            Ok(CompletedNodePages { pages })
        })
    }
}
