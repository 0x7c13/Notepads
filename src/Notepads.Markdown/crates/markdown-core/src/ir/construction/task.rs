// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Fixed construction actions. Allocations and immutable lowering stay outside.

use super::*;
use crate::memory::{BuildKernel, BuildStep, BuildValue, BuildWindow, run_window};

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;

macro_rules! ready {
    ($action:expr) => {
        match $action? {
            BuildStep::Ready(value) => value,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(count) => return Ok(BuildStep::NeedCapacity(count)),
        }
    };
}

fn complete<'id, K: BuildKernel<'id>>(
    kernel: &mut K,
    op: &mut OwnedWork<'id, '_>,
) -> Result<K::Output, CoreError> {
    loop {
        match run_window(kernel, op)? {
            BuildStep::Ready(value) => return Ok(value),
            BuildStep::Full => {}
            // These drivers are selected only after their real allocations
            // exist. They never restart an already executed operation to grow.
            BuildStep::NeedCapacity(_) => return Err(op.fail(CoreError::InternalInvariant)),
        }
    }
}

#[derive(Clone, Copy)]
enum ReservePhase {
    Check,
    Scan,
    Admit,
    Write,
    Done,
}

struct Reserve<'builder, 'id, T: TableId> {
    builder: &'builder mut ExactBuilder<'id>,
    index: usize,
    phase: ReservePhase,
    _type: std::marker::PhantomData<T>,
}

impl<'id, T: TableId> BuildKernel<'id> for Reserve<'_, 'id, T> {
    type Output = DraftRef<T>;

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Self::Output>, CoreError> {
        let key = self.builder.key;
        let state = &mut self.builder.states[T::TABLE as usize];
        let mut bits = ready!(window.write_exact(&mut state.bits));
        loop {
            match self.phase {
                ReservePhase::Check => {
                    ready!(window.advance());
                    if self.index >= state.count {
                        return Err(window.fail(CoreError::InvalidProvenance));
                    }
                    self.phase = ReservePhase::Scan;
                }
                ReservePhase::Scan => {
                    let bit = ready!(bits.state(self.index, window));
                    if bit != Some(0) {
                        return Err(window.fail(CoreError::InvalidProvenance));
                    }
                    self.phase = ReservePhase::Admit;
                }
                ReservePhase::Admit => {
                    ready!(window.admit_record(LimitKind::ModelRecords, 1));
                    self.phase = ReservePhase::Write;
                }
                ReservePhase::Write => {
                    ready!(bits.replace(self.index, 1, window));
                    self.phase = ReservePhase::Done;
                }
                ReservePhase::Done => {
                    ready!(window.advance());
                    return Ok(BuildStep::Ready(DraftRef {
                        key,
                        value: T::at(self.index).map_err(|error| window.fail(error))?,
                    }));
                }
            }
        }
    }
}

pub(super) fn reserve<'id, T: TableId>(
    builder: &mut ExactBuilder<'id>,
    index: usize,
    op: &mut OwnedWork<'id, '_>,
) -> Result<DraftRef<T>, CoreError> {
    let mut kernel = Reserve::<T> {
        builder,
        index,
        phase: ReservePhase::Check,
        _type: std::marker::PhantomData,
    };
    let result = complete(&mut kernel, op);
    result.map_err(|error| op.fail(error))
}

#[derive(Clone, Copy)]
enum CheckPhase {
    Key,
    State,
    Position,
    Done,
}

struct CommitCheck<'builder, 'id> {
    builder: &'builder ExactBuilder<'id>,
    reference: AtomRef,
    pending: Option<AtomId>,
    next: usize,
    phase: CheckPhase,
    id: Option<AtomId>,
    bit: Option<u8>,
}

impl<'id> BuildKernel<'id> for CommitCheck<'_, 'id> {
    type Output = ();

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        let bits = ready!(window.read_exact(&self.builder.states[Table::Atoms as usize].bits));
        loop {
            match self.phase {
                CheckPhase::Key => {
                    self.id = Some(ready!(window.keyed_value(
                        self.builder.key.0,
                        self.reference.key.0,
                        self.reference.value,
                    )));
                    self.phase = CheckPhase::State;
                }
                CheckPhase::State => {
                    let id = self
                        .id
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    self.bit = ready!(bits.state(id.index(), window));
                    self.phase = CheckPhase::Position;
                }
                CheckPhase::Position => {
                    ready!(window.advance());
                    if self.pending != self.id
                        || self.bit != Some(1)
                        || self.id.map(TableId::index) != Some(self.next)
                        || self.next != self.builder.ir.packets.len()
                    {
                        return Err(window.fail(CoreError::InvalidProvenance));
                    }
                    self.phase = CheckPhase::Done;
                }
                CheckPhase::Done => return Ok(BuildStep::Ready(())),
            }
        }
    }
}

pub(super) fn check_commit<'id>(
    builder: &ExactBuilder<'id>,
    reference: AtomRef,
    pending: Option<AtomId>,
    next: usize,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut kernel = CommitCheck {
        builder,
        reference,
        pending,
        next,
        phase: CheckPhase::Key,
        id: None,
        bit: None,
    };
    complete(&mut kernel, op)
}

struct CommitWrite<'builder, 'writer, 'id> {
    builder: &'builder mut ExactBuilder<'id>,
    pending: &'writer mut Option<AtomId>,
    committed: &'writer mut usize,
    index: usize,
    packet: AtomPacket,
    control_ready: bool,
}

impl<'id> BuildKernel<'id> for CommitWrite<'_, '_, 'id> {
    type Output = ();

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        let mut packets = ready!(window.write_exact(&mut self.builder.ir.packets));
        let mut bits =
            ready!(window.write_exact(&mut self.builder.states[Table::Atoms as usize].bits,));
        if !self.control_ready {
            ready!(window.advance());
            self.control_ready = true;
        }
        ready!(packets.push_and_mark(self.packet, &mut bits, self.index, window));
        // Nothing fallible, allocating, observing, or publishing lies between
        // the physical group and the matching private writer completion.
        *self.pending = None;
        *self.committed += 1;
        Ok(BuildStep::Ready(()))
    }
}

pub(super) fn commit_packet<'id>(
    builder: &mut ExactBuilder<'id>,
    pending: &mut Option<AtomId>,
    committed: &mut usize,
    index: usize,
    packet: AtomPacket,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut kernel = CommitWrite {
        builder,
        pending,
        committed,
        index,
        packet,
        control_ready: false,
    };
    let result = complete(&mut kernel, op);
    result.map_err(|error| op.fail(error))
}

pub(super) struct RowTarget<'builder, 'id, R: BuildValue> {
    pub rows: &'builder mut BuildExactRows<'id, R>,
    pub states: &'builder mut BuildExactRows<'id, u8>,

    pub key: BuilderKey,
}

#[derive(Clone, Copy)]
enum RowPhase {
    Push,
    Key,
    State,
    Write,
    Done,
}

struct WriteRow<'builder, 'id, T: TableId, R: BuildValue> {
    target: RowTarget<'builder, 'id, R>,
    reference: DraftRef<T>,
    row: R,
    phase: RowPhase,
}

impl<'id, T: TableId, R: BuildValue> BuildKernel<'id> for WriteRow<'_, 'id, T, R> {
    type Output = DraftRef<T>;

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Self::Output>, CoreError> {
        let mut rows = ready!(window.write_exact(self.target.rows));
        let mut states = ready!(window.write_exact(self.target.states));
        loop {
            match self.phase {
                RowPhase::Push => {
                    ready!(rows.push(self.row, window));
                    self.phase = RowPhase::Key;
                }
                RowPhase::Key => {
                    ready!(window.keyed_value(
                        self.target.key.0,
                        self.reference.key.0,
                        self.reference.value,
                    ));
                    self.phase = RowPhase::State;
                }
                RowPhase::State => {
                    let bit = ready!(states.state(self.reference.value.index(), window));
                    // complete_row first requires >=1 and then exactly 1.
                    // The same immutable copied byte satisfies both checks.
                    if bit != Some(1) {
                        return Err(window.fail(CoreError::InvalidProvenance));
                    }
                    self.phase = RowPhase::Write;
                }
                RowPhase::Write => {
                    ready!(states.replace(self.reference.value.index(), 2, window));
                    self.phase = RowPhase::Done;
                }
                RowPhase::Done => return Ok(BuildStep::Ready(self.reference)),
            }
        }
    }
}

pub(super) fn write_row<'id, T: TableId, R: BuildValue>(
    target: RowTarget<'_, 'id, R>,
    reference: DraftRef<T>,
    row: R,
    op: &mut OwnedWork<'id, '_>,
) -> Result<DraftRef<T>, CoreError> {
    let mut kernel = WriteRow {
        target,
        reference,
        row,
        phase: RowPhase::Push,
    };
    let result = complete(&mut kernel, op);
    result.map_err(|error| op.fail(error))
}
