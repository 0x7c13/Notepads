// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
#[cfg(test)]
use crate::memory::{BuildExactRead, BuildExactRows, BuildExactText, FrozenRows, FrozenText};
use crate::memory::{BuildStep, BuildValue, BuildWindow};
use crate::policy::OwnedWork;

/// Slot state lives beside its actual final value; there is no parallel ledger.
#[derive(Clone, Copy)]
pub(crate) enum FinalSlot<T: Copy> {
    Reserved,
    Written(T),
}
impl<T: BuildValue> BuildValue for FinalSlot<T> {}
impl<T: crate::memory::BuildRecord> crate::memory::BuildRecord for FinalSlot<T> {
    type Output = T::Output;
    fn project(
        &self,
        index: usize,
        work: &mut crate::policy::ConstructionCharges<'_, '_, '_>,
    ) -> Result<Self::Output, CoreError> {
        match self {
            Self::Written(value) => value.project(index, work),
            Self::Reserved => Err(CoreError::InvalidProvenance),
        }
    }
}

/// Production contains only final pages. Exact storage is a historical test fixture.
enum RowStorage<T: Copy> {
    #[cfg(test)]
    Exact(FrozenRows<T>),
    Paged(crate::memory::FrozenPagedRows<FinalSlot<T>>),
    CompletedPaged(crate::memory::FrozenPagedRows<T>),
}
pub(crate) struct ModelRows<T: Copy>(RowStorage<T>);
impl ModelRows<NodeRow> {
    pub(super) fn completed_nodes(pages: super::node_rows::CompletedNodePages) -> Self {
        Self(RowStorage::CompletedPaged(pages.into_pages()))
    }
}
impl<T: Copy> ModelRows<T> {
    #[cfg(test)]
    pub(super) fn exact(rows: FrozenRows<T>) -> Self {
        Self(RowStorage::Exact(rows))
    }
    pub(super) fn paged(rows: crate::memory::FrozenPagedRows<FinalSlot<T>>) -> Self {
        Self(RowStorage::Paged(rows))
    }
    pub(crate) fn len(&self) -> usize {
        match &self.0 {
            #[cfg(test)]
            RowStorage::Exact(rows) => rows.len(),
            RowStorage::Paged(rows) => rows.len(),
            RowStorage::CompletedPaged(rows) => rows.len(),
        }
    }
    pub(crate) fn view(&self) -> super::RowView<'_, T> {
        match &self.0 {
            #[cfg(test)]
            RowStorage::Exact(rows) => rows.as_slice().into(),
            RowStorage::Paged(rows) => super::RowView::paged(rows),
            RowStorage::CompletedPaged(rows) => super::RowView::completed_paged(rows),
        }
    }
}
pub(crate) enum ModelRead<'scope, 'id, T: BuildValue> {
    #[cfg(test)]
    Exact(BuildExactRead<'scope, 'id, T>),
    Paged(crate::memory::BuildFrozenPagedRead<'scope, 'id, FinalSlot<T>>),
    CompletedPaged(crate::memory::BuildFrozenPagedRead<'scope, 'id, T>),
}
impl<T: BuildValue> ModelRows<T> {
    pub(crate) fn task_read<'scope, 'id>(
        &'scope self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<ModelRead<'scope, 'id, T>>, CoreError> {
        macro_rules! bind {
            ($read:expr, $arm:ident) => {
                match $read? {
                    BuildStep::Ready(view) => Ok(BuildStep::Ready(ModelRead::$arm(view))),
                    BuildStep::Full => Ok(BuildStep::Full),
                    BuildStep::NeedCapacity(n) => Ok(BuildStep::NeedCapacity(n)),
                }
            };
        }
        match &self.0 {
            #[cfg(test)]
            RowStorage::Exact(rows) => bind!(window.read_frozen(rows), Exact),
            RowStorage::Paged(rows) => bind!(window.read_frozen_paged(rows), Paged),
            RowStorage::CompletedPaged(rows) => {
                bind!(window.read_frozen_paged(rows), CompletedPaged)
            }
        }
    }
}
impl<'scope, 'id, T: BuildValue> ModelRead<'scope, 'id, T> {
    pub(crate) fn get(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Option<T>>, CoreError> {
        match self {
            #[cfg(test)]
            Self::Exact(view) => view.get(index, window),
            Self::CompletedPaged(view) => view.get(index, window),
            Self::Paged(view) => match view.get(index, window)? {
                BuildStep::Ready(Some(FinalSlot::Written(value))) => {
                    Ok(BuildStep::Ready(Some(value)))
                }
                BuildStep::Ready(Some(FinalSlot::Reserved)) => {
                    Err(window.fail(CoreError::InvalidProvenance))
                }
                BuildStep::Ready(None) => Ok(BuildStep::Ready(None)),
                BuildStep::Full => Ok(BuildStep::Full),
                BuildStep::NeedCapacity(n) => Ok(BuildStep::NeedCapacity(n)),
            },
        }
    }
}
impl<'scope, 'id, T: crate::memory::BuildRecord> ModelRead<'scope, 'id, T> {
    pub(crate) fn record(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<T::Output>, CoreError> {
        match self {
            #[cfg(test)]
            Self::Exact(view) => view.record(index, window),
            Self::Paged(view) => view.record(index, window),
            Self::CompletedPaged(view) => view.record(index, window),
        }
    }
}
enum TextStorage {
    #[cfg(test)]
    Exact(FrozenText),
    Segmented(super::segmented_text::SegmentedText),
}
pub(crate) struct ModelText(TextStorage);
impl ModelText {
    #[cfg(test)]
    pub(super) fn exact(text: FrozenText) -> Self {
        Self(TextStorage::Exact(text))
    }
    pub(super) fn segmented(text: super::segmented_text::SegmentedText) -> Self {
        Self(TextStorage::Segmented(text))
    }
    pub(crate) fn text(&self, value: TextSlice) -> Result<&str, CoreError> {
        match &self.0 {
            #[cfg(test)]
            TextStorage::Exact(text) => {
                let end = value
                    .start()
                    .checked_add(value.len())
                    .ok_or(CoreError::ArithmeticOverflow)?;
                text.as_str()
                    .get(value.start()..end)
                    .ok_or(CoreError::InvalidProvenance)
            }
            TextStorage::Segmented(text) => text.text(value),
        }
    }
    pub(super) fn text_charged(
        &self,
        value: TextSlice,
        work: &mut super::segmented_text::TextWork<'_, '_, '_>,
    ) -> Result<&str, CoreError> {
        match &self.0 {
            #[cfg(test)]
            TextStorage::Exact(text) => {
                let end = value
                    .start()
                    .checked_add(value.len())
                    .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
                work.boundaries(text.as_str(), value.start(), end)?;
                text.as_str()
                    .get(value.start()..end)
                    .ok_or_else(|| work.fail(CoreError::InvalidProvenance))
            }
            TextStorage::Segmented(text) => text.text_charged(value, work),
        }
    }
    #[cfg(test)]
    pub(crate) fn directory_record_bytes(&self) -> usize {
        match &self.0 {
            TextStorage::Exact(_) => 0,
            TextStorage::Segmented(text) => text.directory_record_bytes(),
        }
    }
    #[cfg(test)]
    pub(crate) fn for_each_segment(&self, mut action: impl FnMut(&str)) {
        match &self.0 {
            #[cfg(test)]
            TextStorage::Exact(text) => action(text.as_str()),
            TextStorage::Segmented(text) => {
                for segment in text.segments() {
                    action(segment);
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) struct DraftIr<'id> {
    pub(crate) nodes: BuildExactRows<'id, NodeRow>,
    pub(crate) flows: BuildExactRows<'id, FlowRow>,
    pub(crate) packets: BuildExactRows<'id, AtomPacket>,
    pub(crate) extra_authorities: BuildExactRows<'id, ExtraAuthority>,
    pub(crate) details: BuildExactRows<'id, DetailRow>,
    pub(crate) objects: BuildExactRows<'id, ObjectRow>,
    pub(crate) payloads: BuildExactRows<'id, PayloadRow>,
    pub(crate) definitions: BuildExactRows<'id, DefinitionRow>,
    pub(crate) references: BuildExactRows<'id, ReferenceUseRow>,
    pub(crate) proofs: BuildExactRows<'id, ProofRow>,
    pub(crate) primitives: BuildExactRows<'id, PrimitiveRow>,
    pub(crate) stages: BuildExactRows<'id, StageRow>,
    pub(crate) attributes: BuildExactRows<'id, AttributeRow>,
    pub(crate) sources: BuildExactRows<'id, SourceSpan>,
    pub(crate) alignments: BuildExactRows<'id, Alignment>,
    pub(crate) text: BuildExactText<'id>,
}
#[cfg(test)]
impl<'id> DraftIr<'id> {
    pub(crate) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<CompactIr, CoreError> {
        Ok(CompactIr {
            budget: work.frozen_budget(),
            nodes: ModelRows::exact(self.nodes.freeze(work)?),
            flows: ModelRows::exact(self.flows.freeze(work)?),
            packets: ModelRows::exact(self.packets.freeze(work)?),
            extra_authorities: ModelRows::exact(self.extra_authorities.freeze(work)?),
            details: ModelRows::exact(self.details.freeze(work)?),
            objects: ModelRows::exact(self.objects.freeze(work)?),
            payloads: ModelRows::exact(self.payloads.freeze(work)?),
            definitions: ModelRows::exact(self.definitions.freeze(work)?),
            references: ModelRows::exact(self.references.freeze(work)?),
            proofs: ModelRows::exact(self.proofs.freeze(work)?),
            primitives: ModelRows::exact(self.primitives.freeze(work)?),
            stages: ModelRows::exact(self.stages.freeze(work)?),
            attributes: ModelRows::exact(self.attributes.freeze(work)?),
            sources: ModelRows::exact(self.sources.freeze(work)?),
            alignments: ModelRows::exact(self.alignments.freeze(work)?),
            text: ModelText::exact(self.text.freeze(work)?),
        })
    }
    pub(crate) fn authority(
        &self,
        id: ClaimId,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<AuthoritySpec, CoreError> {
        work.events(1)?;
        work.view_copy(size_of::<AuthoritySpec>() as u64)?;
        match id.storage() {
            types::ClaimStorage::Extra(id) => self
                .extra_authorities
                .get(id.index(), work)?
                .map(|row| row.value)
                .ok_or_else(|| work.fail(CoreError::InvalidProvenance)),
            types::ClaimStorage::Primary(id) => {
                let packet = self
                    .packets
                    .get(id.index(), work)?
                    .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
                let AuthorityBinding::Owned(primary) = packet.authority else {
                    return Err(work.fail(CoreError::InvalidProvenance));
                };
                Ok(AuthoritySpec {
                    owner: packet.atom.owner,
                    sources: packet.atom.sources,
                    phase: primary.phase,
                    kind: primary.kind,
                    consumed: primary.consumed,
                    host: HostRow {
                        owner: packet.atom.owner,
                        role: primary.host_role,
                        utf8_len: primary.host_utf8_len,
                        domains: primary.host_domains,
                    },
                })
            }
        }
    }
}

/// One copied construction record; no mutable allocation or slice escapes.
pub(crate) fn owned_row<'id, T: BuildValue>(
    rows: &crate::memory::BuildGrowingVec<'id, T>,
    index: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<T, CoreError> {
    // Scratch algorithms retain their existing semantic fees at the caller.
    // The physical access is still confined to a checked finite window and
    // returns exactly one closed BuildValue, never a storage borrow.
    struct Read<'a, 'id, T: BuildValue> {
        rows: &'a crate::memory::BuildGrowingVec<'id, T>,
        index: usize,
    }
    impl<'id, T: BuildValue> crate::memory::BuildKernel<'id> for Read<'_, 'id, T> {
        type Output = T;
        fn run<'scope>(
            &'scope mut self,
            window: &mut BuildWindow<'scope, '_, 'id, '_>,
        ) -> Result<BuildStep<T>, CoreError> {
            let rows = match window.read_growing(self.rows)? {
                BuildStep::Ready(rows) => rows,
                BuildStep::Full => return Ok(BuildStep::Full),
                BuildStep::NeedCapacity(n) => return Ok(BuildStep::NeedCapacity(n)),
            };
            match rows.get(self.index, window)? {
                BuildStep::Ready(Some(value)) => Ok(BuildStep::Ready(value)),
                BuildStep::Ready(None) => Err(window.fail(CoreError::InvalidProvenance)),
                BuildStep::Full => Ok(BuildStep::Full),
                BuildStep::NeedCapacity(n) => Ok(BuildStep::NeedCapacity(n)),
            }
        }
    }
    let mut read = Read { rows, index };
    loop {
        match crate::memory::run_window(&mut read, work)? {
            BuildStep::Ready(value) => return Ok(value),
            BuildStep::Full => {}
            BuildStep::NeedCapacity(_) => return Err(work.fail(CoreError::InternalInvariant)),
        }
    }
}

#[cfg(test)]
impl<'id> DraftIr<'id> {
    pub(super) fn owned_atom_at(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<AtomRow, CoreError> {
        let packet = self
            .packets
            .get(index, work)?
            .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
        let claim = match packet.authority {
            AuthorityBinding::Owned(_) => {
                ClaimId::primary(AtomId::from_index(index).map_err(|e| work.fail(e))?)
            }
            AuthorityBinding::Shared(id) => id,
        };
        let atom = packet.atom;
        Ok(AtomRow {
            owner: atom.owner,
            kind: atom.kind,
            target: atom.target,
            sources: atom.sources,
            proof: atom.proof,
            content: atom.content,
            claim,
        })
    }
}

// A callback can unwind while its caller catches the panic and retains the
// construction. Latch abandonment into the original work before that catch resumes.
struct CallbackWork<'work, 'id, 'op> {
    work: &'work mut OwnedWork<'id, 'op>,
    returned: bool,
}
impl Drop for CallbackWork<'_, '_, '_> {
    fn drop(&mut self) {
        if !self.returned {
            self.work.fail(CoreError::InvalidProvenance);
        }
    }
}
pub(super) fn guarded_callback<'id, T>(
    work: &mut OwnedWork<'id, '_>,
    action: impl FnOnce(&mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
) -> Result<T, CoreError> {
    work.check()?;
    let mut guard = CallbackWork {
        work,
        returned: false,
    };
    let value = action(guard.work).map_err(|error| guard.work.fail(error))?;
    guard.work.check()?;
    guard.returned = true;
    Ok(value)
}
