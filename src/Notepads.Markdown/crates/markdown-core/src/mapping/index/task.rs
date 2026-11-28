// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Fixed index postings and atom reconstruction share one scoped window. First
//! allocation remains a whole protected call with no surviving physical view.

use super::{AtomId, AtomKind, AtomRow, CompactIr, IndexDraft, Posting, Rows, TargetPosting};
use crate::error::{CoreError, LimitKind};
use crate::memory::{BuildExactRows, BuildKernel, BuildStep, BuildValue, BuildWindow, run_window};
use crate::policy::OwnedWork;
use crate::source::SourceSnapshot;

pub(super) fn populate<'id>(
    mut index: IndexDraft<'id>,
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> Result<IndexDraft<'id>, CoreError> {
    let result = index.populate(ir, source, false, op);
    result.map_err(|error| op.fail(error))?;
    Ok(index)
}

// These records contain only scalar source/target coordinates and integer IDs.
impl BuildValue for Posting {}
impl BuildValue for TargetPosting {}

#[derive(Clone, Copy)]
enum Pending {
    Target { hidden: bool, row: TargetPosting },
    Source { synthetic: bool, row: Posting },
}

#[derive(Clone, Copy)]
enum Outside {
    LazyPush(Pending),
    Done,
}
impl BuildValue for Outside {}

#[derive(Clone, Copy)]
enum Stage {
    ReadAtom,
    AtomEvent,
    Target,
    Admit,
    Push,
    Members,
    MemberEvent,
    MemberRead,
    AtomDone,
}

struct Postings<'a, 'id> {
    index: &'a mut IndexDraft<'id>,

    ir: &'a CompactIr,
    position: usize,
    end: usize,
    source_count: usize,
    length: u64,
    target_end: u64,
    atom: Option<AtomRow>,
    member: usize,
    member_end: usize,
    pending: Option<Pending>,
    stage: Stage,
    finished: bool,
}

impl<'id> Postings<'_, 'id> {
    fn after_push(&mut self, pending: Pending) {
        self.pending = None;
        self.stage = match pending {
            Pending::Target { .. } => Stage::Members,
            Pending::Source { .. } => {
                self.member += 1;
                Stage::MemberEvent
            }
        };
    }
}

impl<'id> BuildKernel<'id> for Postings<'_, 'id> {
    type Output = Outside;

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Outside>, CoreError> {
        let sources = match self.ir.sources.task_read(window)? {
            BuildStep::Ready(view) => view,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => {
                return Err(window.fail(CoreError::InternalInvariant));
            }
        };
        let packets = match self.ir.packets.task_read(window)? {
            BuildStep::Ready(view) => view,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
        };
        macro_rules! write_view {
            ($field:ident) => {
                if self.index.$field.has_allocation() {
                    Some(match window.write_exact(&mut self.index.$field)? {
                        BuildStep::Ready(view) => view,
                        BuildStep::Full => return Ok(BuildStep::Full),
                        BuildStep::NeedCapacity(_) => {
                            return Err(window.fail(CoreError::InternalInvariant));
                        }
                    })
                } else {
                    None
                }
            };
        }
        let mut target = write_view!(target);
        let mut hidden = write_view!(hidden);
        let mut source = write_view!(source);
        let mut synthetic = write_view!(synthetic);
        loop {
            match self.stage {
                Stage::ReadAtom => {
                    if matches!(window.advance()?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    if self.position == self.end {
                        if self.target_end != self.length {
                            return Err(window.fail(CoreError::InvalidProvenance));
                        }
                        return Ok(BuildStep::Ready(Outside::Done));
                    }
                    self.atom = Some(match packets.record(self.position, window)? {
                        BuildStep::Ready(atom) => atom,
                        BuildStep::Full => return Ok(BuildStep::Full),
                        BuildStep::NeedCapacity(_) => {
                            return Err(window.fail(CoreError::InternalInvariant));
                        }
                    });
                    self.stage = Stage::AtomEvent;
                }
                Stage::AtomEvent => {
                    if matches!(window.event()?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    self.stage = Stage::Target;
                }
                Stage::Target => {
                    if matches!(window.advance()?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    let atom = self
                        .atom
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    let hidden = atom.target.end <= atom.target.start;
                    if hidden {
                        if atom.kind != AtomKind::Hidden {
                            return Err(window.fail(CoreError::InvalidProvenance));
                        }
                    } else {
                        if atom.target.start != self.target_end {
                            return Err(window.fail(CoreError::InvalidProvenance));
                        }
                        self.target_end = atom.target.end;
                    }
                    self.pending = Some(Pending::Target {
                        hidden,
                        row: TargetPosting {
                            start: atom.target.start,
                            end: atom.target.end,
                            atom: AtomId::from_index(self.position).map_err(|e| window.fail(e))?,
                        },
                    });
                    self.stage = Stage::Admit;
                }
                Stage::Admit => {
                    let pending = self
                        .pending
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    let allocated = match pending {
                        Pending::Target { hidden: true, .. } => hidden.is_some(),
                        Pending::Target { hidden: false, .. } => target.is_some(),
                        Pending::Source {
                            synthetic: true, ..
                        } => synthetic.is_some(),
                        Pending::Source {
                            synthetic: false, ..
                        } => source.is_some(),
                    };
                    if !allocated {
                        if matches!(window.advance()?, BuildStep::Full) {
                            return Ok(BuildStep::Full);
                        }
                        // No admission or write has happened. The complete old
                        // push runs once, including its lazy allocation.
                        return Ok(BuildStep::Ready(Outside::LazyPush(pending)));
                    }
                    if matches!(
                        window.admit_record(LimitKind::IndexRecords, 1)?,
                        BuildStep::Full
                    ) {
                        return Ok(BuildStep::Full);
                    }
                    self.stage = Stage::Push;
                }
                Stage::Push => {
                    let pending = self
                        .pending
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    let result = match pending {
                        Pending::Target {
                            hidden: is_hidden,
                            row,
                        } => {
                            let view = if is_hidden { &mut hidden } else { &mut target };
                            let view = view
                                .as_mut()
                                .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                            view.push(row, window)?
                        }
                        Pending::Source {
                            synthetic: is_synthetic,
                            row,
                        } => {
                            let view = if is_synthetic {
                                &mut synthetic
                            } else {
                                &mut source
                            };
                            let view = view
                                .as_mut()
                                .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                            view.push(row, window)?
                        }
                    };
                    match result {
                        BuildStep::Ready(()) => {
                            self.pending = None;
                            self.stage = match pending {
                                Pending::Target { .. } => Stage::Members,
                                Pending::Source { .. } => {
                                    self.member += 1;
                                    Stage::MemberEvent
                                }
                            };
                        }
                        BuildStep::Full => return Ok(BuildStep::Full),
                        BuildStep::NeedCapacity(_) => {
                            return Err(window.fail(CoreError::InternalInvariant));
                        }
                    }
                }
                Stage::Members => {
                    if matches!(window.advance()?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    let atom = self
                        .atom
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    let range = atom
                        .sources
                        .indices(self.source_count)
                        .map_err(|e| window.fail(e))?;
                    self.member = range.start;
                    self.member_end = range.end;
                    self.stage = Stage::MemberEvent;
                }
                Stage::MemberEvent => {
                    if self.member == self.member_end {
                        if matches!(window.advance()?, BuildStep::Full) {
                            return Ok(BuildStep::Full);
                        }
                        self.stage = Stage::AtomDone;
                    } else {
                        if matches!(window.event()?, BuildStep::Full) {
                            return Ok(BuildStep::Full);
                        }
                        self.stage = Stage::MemberRead;
                    }
                }
                Stage::MemberRead => {
                    let span = match sources.get(self.member, window)? {
                        BuildStep::Ready(Some(span)) => span,
                        BuildStep::Ready(None) => {
                            return Err(window.fail(CoreError::InvalidProvenance));
                        }
                        BuildStep::Full => return Ok(BuildStep::Full),
                        BuildStep::NeedCapacity(_) => {
                            return Err(window.fail(CoreError::InternalInvariant));
                        }
                    };
                    let atom = self
                        .atom
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    let synthetic = span.is_empty();
                    if synthetic && atom.kind != AtomKind::Synthetic {
                        return Err(window.fail(CoreError::InvalidProvenance));
                    }
                    self.pending = Some(Pending::Source {
                        synthetic,
                        row: Posting {
                            span,
                            atom: AtomId::from_index(self.position).map_err(|e| window.fail(e))?,
                            part: u32::try_from(self.member - atom.sources.start())
                                .map_err(|_| window.fail(CoreError::ArithmeticOverflow))?,
                        },
                    });
                    self.stage = Stage::Admit;
                }
                Stage::AtomDone => {
                    if matches!(window.advance()?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    self.position += 1;
                    self.atom = None;
                    self.stage = Stage::ReadAtom;
                }
            }
        }
    }
}

pub(super) fn postings<'id>(
    index: &mut IndexDraft<'id>,
    ir: &CompactIr,
    atoms: Rows<AtomRow>,
    length: u64,

    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let range = atoms.indices(ir.atoms_len()).map_err(|e| op.fail(e))?;
    let mut cursor = Postings {
        index,

        ir,
        position: range.start,
        end: range.end,
        source_count: ir.sources().len(),
        length,
        target_end: 0,
        atom: None,
        member: 0,
        member_end: 0,
        pending: None,
        stage: Stage::ReadAtom,
        finished: false,
    };
    let result = (|| {
        loop {
            match run_window(&mut cursor, op)? {
                BuildStep::Full => {}
                BuildStep::NeedCapacity(_) => return Err(op.fail(CoreError::InternalInvariant)),
                BuildStep::Ready(Outside::Done) => {
                    cursor.finished = true;
                    return Ok(());
                }
                BuildStep::Ready(Outside::LazyPush(pending)) => {
                    match pending {
                        Pending::Target { hidden, row } => {
                            let rows = if hidden {
                                &mut cursor.index.hidden
                            } else {
                                &mut cursor.index.target
                            };
                            super::push(rows, row, op)?;
                        }
                        Pending::Source { synthetic, row } => {
                            let rows = if synthetic {
                                &mut cursor.index.synthetic
                            } else {
                                &mut cursor.index.source
                            };
                            super::push(rows, row, op)?;
                        }
                    }
                    cursor.after_push(pending);
                }
            }
        }
    })();
    result.map_err(|error| op.fail(error))
}

struct SourcePartition<'a, 'id> {
    rows: &'a BuildExactRows<'id, Posting>,

    index: usize,
    end: usize,
    prior_end: Option<u64>,
    event_done: bool,
}

impl<'id> BuildKernel<'id> for SourcePartition<'_, 'id> {
    type Output = ();

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        let rows = match window.read_exact(self.rows)? {
            BuildStep::Ready(view) => view,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
        };
        while self.index < self.end {
            if !self.event_done {
                if matches!(window.event()?, BuildStep::Full) {
                    return Ok(BuildStep::Full);
                }
                self.event_done = true;
            }
            let row = match rows.get(self.index, window)? {
                BuildStep::Ready(Some(row)) => row,
                BuildStep::Ready(None) => return Err(window.fail(CoreError::InvalidProvenance)),
                BuildStep::Full => return Ok(BuildStep::Full),
                BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
            };
            if self.prior_end.is_some_and(|end| row.span.start() < end) {
                return Err(window.fail(CoreError::InvalidProvenance));
            }
            self.prior_end = Some(row.span.end());
            self.index += 1;
            self.event_done = false;
        }
        Ok(BuildStep::Ready(()))
    }
}

pub(super) fn check_source_partition<'id>(
    rows: &BuildExactRows<'id, Posting>,
    start: usize,

    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut cursor = SourcePartition {
        rows,

        index: start,
        end: rows.len(),
        prior_end: None,
        event_done: false,
    };
    loop {
        match run_window(&mut cursor, op)? {
            BuildStep::Ready(()) => return Ok(()),
            BuildStep::Full => {}
            BuildStep::NeedCapacity(_) => return Err(op.fail(CoreError::InternalInvariant)),
        }
    }
}

#[cfg(test)]
mod tests;
