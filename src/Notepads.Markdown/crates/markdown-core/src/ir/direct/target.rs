// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One target owns provisional identities until a complete in-place permutation.
use super::*;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct WrittenAtomBlock {
    pub(super) reference: DraftRef<Rows<AtomRow>>,
}
pub(crate) struct ReservedAtom {
    born: BornAtom,
}
impl ReservedAtom {
    pub(crate) fn born(&self) -> BornAtom {
        self.born
    }
}

pub(crate) struct TargetWriter<'builder, 'work, 'id, 'op> {
    builder: &'builder mut DirectBuilder<'id>,
    work: &'work mut OwnedWork<'id, 'op>,
    target: u64,
    start: usize,
    pending: Option<BornAtom>,
    closed: bool,
}
pub(crate) struct ClosedTarget<'id> {
    #[cfg(test)]
    key: BuilderKey,
    #[cfg(test)]
    target: u64,
    #[cfg(test)]
    start: usize,
    #[cfg(test)]
    permutation: BuildGrowingVec<'id, usize>,
    block: WrittenAtomBlock,
    brand: crate::policy::ConstructionId<'id>,
}
impl<'id> ClosedTarget<'id> {
    #[cfg(test)]
    pub(crate) fn resolve(
        &self,
        born: BornAtom,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<FinalAtomRef, CoreError> {
        work.events(1)?;
        if born.key != self.key || born.target != self.target || born.index < self.start {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let rank = read_order(&self.permutation, born.index - self.start, work)?;
        let id = AtomId::from_index(self.start + rank).map_err(|e| work.fail(e))?;
        Ok(FinalAtomRef(DraftRef {
            key: self.key,
            value: id,
        }))
    }
    pub(crate) fn into_block(
        self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<WrittenAtomBlock, CoreError> {
        work.check()?;
        Ok(self.block)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn begin_target<'builder, 'work, 'op>(
        &'builder mut self,
        work: &'work mut OwnedWork<'id, 'op>,
    ) -> Result<TargetWriter<'builder, 'work, 'id, 'op>, CoreError> {
        work.events(1)?;
        if self.active_target.is_some() || self.completed_atoms != self.ir.packets.len() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        self.next_target = self
            .next_target
            .checked_add(1)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        let start = self.completed_atoms;
        self.active_target = Some((self.next_target, start));
        let target = self.next_target;
        Ok(TargetWriter {
            builder: self,
            work,
            target,
            start,
            pending: None,
            closed: false,
        })
    }
}
impl<'id> TargetWriter<'_, '_, 'id, '_> {
    pub(crate) fn fail(&mut self, error: CoreError) -> CoreError {
        self.work.fail(error)
    }

    pub(crate) fn len(&self) -> usize {
        self.builder.ir.packets.len() - self.start
    }
    pub(crate) fn reserve_next(&mut self) -> Result<ReservedAtom, CoreError> {
        self.work.check()?;
        if self.pending.is_some() {
            return Err(self.work.fail(CoreError::InvalidProvenance));
        }
        self.work.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.builder.ir.packets.reserve(self.work)?;
        let born = BornAtom {
            key: self.builder.key,
            target: self.target,
            index,
        };
        self.pending = Some(born);
        Ok(ReservedAtom { born })
    }
    pub(crate) fn local_slot(&mut self, born: BornAtom) -> Result<usize, CoreError> {
        self.builder.born(born, self.work)?;
        Ok(born.index - self.start)
    }
    pub(crate) fn with_read<T>(
        &mut self,
        action: impl FnOnce(&DirectBuilder<'id>, &mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        super::super::storage::guarded_callback(self.work, |work| action(self.builder, work))
    }
    /// Auxiliary rows are written under the live target guard and its original
    /// work capability. Neither a work borrow nor a mutable builder borrow can
    /// escape the callback. The target itself cannot be replaced or nested.
    pub(crate) fn with_rows<T>(
        &mut self,
        action: impl FnOnce(&mut DirectBuilder<'id>, &mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.work.check()?;
        let key = self.builder.key;
        let packets = self.builder.ir.packets.len();
        let completed = self.builder.completed_atoms;
        let value =
            super::super::storage::guarded_callback(self.work, |work| action(self.builder, work))?;
        if self.builder.key != key
            || self.builder.active_target != Some((self.target, self.start))
            || self.builder.ir.packets.len() != packets
            || self.builder.completed_atoms != completed
        {
            return Err(self.work.fail(CoreError::InvalidProvenance));
        }
        Ok(value)
    }
    fn commit(
        &mut self,
        ticket: ReservedAtom,
        lower: impl FnOnce(
            &DirectBuilder<'id>,
            &mut OwnedWork<'id, '_>,
        ) -> Result<AtomPacket, CoreError>,
    ) -> Result<BornAtom, CoreError> {
        let result = (|| {
            self.work.check()?;
            if self.pending != Some(ticket.born) {
                return Err(self.work.fail(CoreError::InvalidProvenance));
            }
            let id = self.builder.born(ticket.born, self.work)?;
            let packet = lower(self.builder, self.work).map_err(|e| self.work.fail(e))?;
            self.builder
                .ir
                .packets
                .write(id.index(), packet, self.work)?;
            self.pending = None;
            Ok(ticket.born)
        })();
        result.map_err(|e| self.work.fail(e))
    }
    pub(crate) fn commit_owned(
        &mut self,
        ticket: ReservedAtom,
        row: DraftOwnedAtom,
    ) -> Result<BornAtom, CoreError> {
        self.commit(ticket, |builder, work| {
            let atom = AtomFields {
                owner: builder.reference(row.owner, false, work)?,
                kind: row.kind,
                target: row.target,
                sources: builder.source_ref(row.sources, work)?,
                proof: builder.optional(row.proof, work)?,
                content: row.content.lower(builder, work)?,
            };
            let primary = PrimaryAuthority {
                phase: row.primary.phase,
                kind: row.primary.kind,
                consumed: row.primary.consumed,
                host_role: row.primary.host_role,
                host_utf8_len: row.primary.host_utf8_len,
                host_domains: builder.source_ref(row.primary.host_domains, work)?,
            };
            work.view_scan(size_of::<DraftOwnedAtom>() as u64)?;
            Ok(AtomPacket {
                atom,
                authority: AuthorityBinding::Owned(primary),
            })
        })
    }
    pub(crate) fn commit_shared(
        &mut self,
        ticket: ReservedAtom,
        row: DraftAtomFields,
        authority: AuthorityRef,
    ) -> Result<BornAtom, CoreError> {
        self.commit(ticket, |builder, work| {
            Ok(AtomPacket {
                atom: row.lower(builder, work)?,
                authority: AuthorityBinding::Shared(builder.authority_ref(authority, true, work)?),
            })
        })
    }
    /// `order` is the original semantic target order, built from the original
    /// positive/zero/domain rules. It holds identities only, never packets.
    pub(crate) fn close(
        mut self,
        order: BuildGrowingVec<'id, BornAtom>,
    ) -> Result<ClosedTarget<'id>, CoreError> {
        let result = (|| {
            self.work.check()?;
            let len = self.len();
            if self.pending.is_some() || order.len() != len {
                return Err(self.work.fail(CoreError::InvalidProvenance));
            }
            let mut permutation =
                BuildGrowingVec::new(len as u64, LimitKind::AuxiliaryCapacity, self.work)?;
            for _ in 0..len {
                permutation.push(usize::MAX, self.work)?;
            }
            for rank in 0..len {
                let born = read_order(&order, rank, self.work)?;
                let slot = self.local_slot(born)?;
                if slot >= len || read_order(&permutation, slot, self.work)? != usize::MAX {
                    return Err(self.work.fail(CoreError::InvalidProvenance));
                }
                if !self.builder.ir.packets.is_written(born.index, self.work)? {
                    return Err(self.work.fail(CoreError::InvalidProvenance));
                }
                permutation.replace(slot, rank, self.work)?;
            }
            drop(order);
            let mut visited =
                BuildGrowingVec::new(len as u64, LimitKind::AuxiliaryCapacity, self.work)?;
            for _ in 0..len {
                visited.push(0_u8, self.work)?;
            }
            for first in 0..len {
                if read_order(&visited, first, self.work)? == 1 {
                    continue;
                }
                let mut current = first;
                let mut packet = self
                    .builder
                    .ir
                    .packets
                    .get(self.start + current, self.work)?;
                loop {
                    self.work.events(1)?;
                    let rank = read_order(&permutation, current, self.work)?;
                    let displaced = self.builder.ir.packets.get(self.start + rank, self.work)?;
                    if let AuthorityBinding::Shared(claim) = &mut packet.authority {
                        *claim = fix_claim(*claim, self.start, &permutation, self.work)?;
                    }
                    self.builder.ir.packets.replace_written(
                        self.start + rank,
                        packet,
                        self.work,
                    )?;
                    visited.replace(current, 1, self.work)?;
                    current = rank;
                    if current == first {
                        break;
                    }
                    if read_order(&visited, current, self.work)? == 1 {
                        return Err(self.work.fail(CoreError::InvalidProvenance));
                    }
                    packet = displaced;
                }
            }
            drop(visited);
            self.builder
                .fixup_target(self.start, &permutation, self.work)?;
            let block = WrittenAtomBlock {
                reference: self
                    .builder
                    .scoped(Rows::new(self.start, len).map_err(|e| self.work.fail(e))?),
            };
            self.builder.completed_atoms = self.start + len;
            self.builder.active_target = None;
            Ok(ClosedTarget {
                #[cfg(test)]
                key: self.builder.key,
                #[cfg(test)]
                target: self.target,
                #[cfg(test)]
                start: self.start,
                #[cfg(test)]
                permutation,
                block,
                brand: std::marker::PhantomData,
            })
        })();
        let result = result.map_err(|e| self.work.fail(e));
        if result.is_ok() {
            self.closed = true;
        }
        result
    }
}
impl Drop for TargetWriter<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            self.work.fail(CoreError::InvalidProvenance);
        }
    }
}

fn fix_atom<'id>(
    id: AtomId,
    start: usize,
    p: &BuildGrowingVec<'id, usize>,
    w: &mut OwnedWork<'id, '_>,
) -> Result<AtomId, CoreError> {
    if id.index() < start {
        return Ok(id);
    }
    let rank = read_order(p, id.index() - start, w)?;
    AtomId::from_index(start + rank).map_err(|e| w.fail(e))
}
fn fix_claim<'id>(
    id: ClaimId,
    start: usize,
    p: &BuildGrowingVec<'id, usize>,
    w: &mut OwnedWork<'id, '_>,
) -> Result<ClaimId, CoreError> {
    match id.storage() {
        types::ClaimStorage::Primary(atom) => Ok(ClaimId::primary(fix_atom(atom, start, p, w)?)),
        types::ClaimStorage::Extra(_) => Ok(id),
    }
}
impl<'id> DirectBuilder<'id> {
    fn fixup_target(
        &mut self,
        start: usize,
        p: &BuildGrowingVec<'id, usize>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        for index in 0..self.fixups.len() {
            w.events(1)?;
            match self.fixups.get(index, w)? {
                FixupSite::Object(index) => {
                    let mut row = self.ir.objects.get(index, w)?;
                    match &mut row {
                        ObjectRow::Image { atom, .. }
                        | ObjectRow::Task { atom, .. }
                        | ObjectRow::Derived { atom, .. } => *atom = fix_atom(*atom, start, p, w)?,
                    }
                    self.ir.objects.replace_written(index, row, w)?;
                }
                FixupSite::Proof(index) => {
                    let mut row = self.ir.proofs.get(index, w)?;
                    row.atom = fix_atom(row.atom, start, p, w)?;
                    self.ir.proofs.replace_written(index, row, w)?;
                }
                FixupSite::Stage(index) => {
                    let mut row = self.ir.stages.get(index, w)?;
                    row.accepted_by = fix_claim(row.accepted_by, start, p, w)?;
                    self.ir.stages.replace_written(index, row, w)?;
                }
                FixupSite::Detail(index) => {
                    let mut row = self.ir.details.get(index, w)?;
                    let (DetailRow::Heading { claim, .. }
                    | DetailRow::List { claim, .. }
                    | DetailRow::Item { claim, .. }
                    | DetailRow::DescriptionItem { claim, .. }
                    | DetailRow::Code { claim, .. }
                    | DetailRow::Html { claim }
                    | DetailRow::Table { claim, .. }
                    | DetailRow::Row { claim, .. }
                    | DetailRow::Cell { claim, .. }
                    | DetailRow::FootnoteDefinition { claim, .. }
                    | DetailRow::FootnoteReference { claim, .. }
                    | DetailRow::FootnoteBacklink { claim, .. }
                    | DetailRow::Callout { claim, .. }
                    | DetailRow::TableOfContentsEntry { claim, .. }
                    | DetailRow::Link { claim, .. }) = &mut row;
                    *claim = fix_claim(*claim, start, p, w)?;
                    if let DetailRow::FootnoteReference {
                        closing: Some(closing),
                        ..
                    } = &mut row
                    {
                        *closing = fix_claim(*closing, start, p, w)?;
                    }
                    self.ir.details.replace_written(index, row, w)?;
                }
                FixupSite::Definition(index) => {
                    let mut row = self.ir.definitions.get(index, w)?;
                    row.claim = fix_claim(row.claim, start, p, w)?;
                    self.ir.definitions.replace_written(index, row, w)?;
                }
                FixupSite::Reference(index) => {
                    let mut row = self.ir.references.get(index, w)?;
                    row.claim = fix_claim(row.claim, start, p, w)?;
                    self.ir.references.replace_written(index, row, w)?;
                }
                FixupSite::Attribute(index) => {
                    let mut row = self.ir.attributes.get(index, w)?;
                    row.claim = fix_claim(row.claim, start, p, w)?;
                    self.ir.attributes.replace_written(index, row, w)?;
                }
            }
        }
        let limits = w
            .model_limits()
            .ok_or_else(|| w.fail(CoreError::UnsupportedConfiguration))?;
        self.fixups = BuildRows::new(
            limits.model_records.saturating_add(limits.proof_records),
            LimitKind::AuxiliaryCapacity,
            w,
        )?;
        Ok(())
    }
}

#[cfg(test)]
impl<'id> TargetWriter<'_, '_, 'id, '_> {
    pub(crate) fn text_from_chunks(
        &mut self,
        len: usize,
        chunk: impl FnMut(usize, &mut OwnedWork<'id, '_>) -> Result<BuildTextChunk, CoreError>,
    ) -> Result<TextRef, CoreError> {
        self.builder.text_from_chunks(len, self.work, chunk)
    }
}

fn read_order<'id, T: crate::memory::BuildValue>(
    rows: &BuildGrowingVec<'id, T>,
    at: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<T, CoreError> {
    work.events(1)?;
    work.scan(size_of::<T>() as u64)?;
    work.copy(size_of::<T>() as u64)?;
    super::super::storage::owned_row(rows, at, work)
}
