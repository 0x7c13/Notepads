// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One permit per physical backing. Only narrow slot codes are rehashed.
use super::{Host, NativeViolation, WorkDelta, rows::NativeRows};

const EMPTY: u32 = 0;
const TOMBSTONE: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AllocationId {
    slot: u32,
    generation: u32,
}

struct Located {
    position: usize,
    code: u32,
    id: Option<AllocationId>,
}

enum State<L> {
    Free(Option<u32>),
    Live(L),
}
struct Slot<L> {
    pointer: usize,
    generation: u32,
    marked: bool,
    indexed: bool,
    state: State<L>,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Mutable,
    Marking,
    Prepared,
    Readonly,
}

pub(super) struct Registry<L> {
    // Data owners in Context precede this registry in destruction order.
    slots: NativeRows<Slot<L>, L>,
    index: NativeRows<u32, L>,
    pending_index: Option<NativeRows<u32, L>>,
    pending_permit: Option<L>,
    free: Option<u32>,
    entries: usize,
    tombstones: usize,
    phase: Phase,
}
impl<L> Default for Registry<L> {
    fn default() -> Self {
        Self {
            slots: NativeRows::default(),
            index: NativeRows::default(),
            pending_index: None,
            pending_permit: None,
            free: None,
            entries: 0,
            tombstones: 0,
            phase: Phase::Mutable,
        }
    }
}

fn charge<H: Host>(host: &mut H, bytes: usize) -> Result<(), H::Error> {
    host.work(WorkDelta::Events(1))?;
    host.work(WorkDelta::Scan(bytes as u64))
}
fn hash(pointer: usize) -> usize {
    let mut value = (pointer as u64) >> 3;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    (value ^ (value >> 31)) as usize
}

impl<L> Registry<L> {
    #[cfg(test)]
    pub(super) fn test_index_counts(&self) -> (usize, usize) {
        (self.entries, self.tombstones)
    }
    #[cfg(test)]
    pub(super) fn test_bucket(pointer: usize, length: usize) -> usize {
        hash(pointer) & (length - 1)
    }
    #[cfg(test)]
    pub(super) fn test_counts(&self) -> (usize, usize, Option<usize>) {
        let mut live = 0;
        for at in 0..self.slots.len() {
            if matches!(
                self.slots
                    .get(at, |_| Ok::<_, ()>(()))
                    .unwrap()
                    .unwrap()
                    .state,
                State::Live(_)
            ) {
                live += 1;
            }
        }
        (
            live,
            self.index.len(),
            self.pending_index.as_ref().map(NativeRows::len),
        )
    }
    pub(super) fn mutable<H: Host<Lease = L>>(&self, host: &mut H) -> Result<(), H::Error> {
        if self.phase != Phase::Mutable
            || self.pending_index.is_some()
            || self.pending_permit.is_some()
        {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        Ok(())
    }
    fn slot<H: Host<Lease = L>>(&self, at: u32, host: &mut H) -> Result<&Slot<L>, H::Error> {
        self.slots
            .get(at as usize, |n| charge(host, n))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))
    }
    fn slot_mut<H: Host<Lease = L>>(
        &mut self,
        at: u32,
        host: &mut H,
    ) -> Result<&mut Slot<L>, H::Error> {
        self.slots
            .get_mut(at as usize, |n| charge(host, n))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))
    }
    fn write_code<H: Host<Lease = L>>(
        index: &mut NativeRows<u32, L>,
        at: usize,
        code: u32,
        host: &mut H,
    ) -> Result<(), H::Error> {
        host.work(WorkDelta::Copy(4))?;
        *index
            .get_mut(at, |n| charge(host, n))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))? = code;
        Ok(())
    }
    // Returns the exact existing slot or an insertion position. No slot handle escapes.
    fn locate<H: Host<Lease = L>>(
        &self,
        pointer: usize,
        host: &mut H,
    ) -> Result<Located, H::Error> {
        if self.index.len() == 0 {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        let mut at = hash(pointer) & (self.index.len() - 1);
        let mut first_free = None;
        let mut visited = 0;
        while visited < self.index.len() {
            let page = self
                .index
                .page_tail(at, |n| charge(host, n + std::mem::size_of::<u32>()))?
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            for (offset, &code) in page.iter().take(self.index.len() - visited).enumerate() {
                if offset != 0 {
                    charge(host, std::mem::size_of::<u32>())?;
                }
                match code {
                    EMPTY => {
                        return Ok(Located {
                            position: first_free.unwrap_or(at),
                            code: if first_free.is_some() {
                                TOMBSTONE
                            } else {
                                EMPTY
                            },
                            id: None,
                        });
                    }
                    TOMBSTONE => {
                        first_free.get_or_insert(at);
                    }
                    _ => {
                        let slot = self.slot(code - 2, host)?;
                        if !slot.indexed || !matches!(slot.state, State::Live(_)) {
                            return Err(host.reject(NativeViolation::InvalidProvenance));
                        }
                        if slot.pointer == pointer {
                            return Ok(Located {
                                position: at,
                                code,
                                id: Some(AllocationId {
                                    slot: code - 2,
                                    generation: slot.generation,
                                }),
                            });
                        }
                    }
                }
                visited += 1;
                at = (at + 1) & (self.index.len() - 1);
            }
        }
        Err(host.reject(NativeViolation::InvalidProvenance))
    }
    fn vacant<H: Host<Lease = L>>(
        index: &NativeRows<u32, L>,
        pointer: usize,
        host: &mut H,
    ) -> Result<usize, H::Error> {
        let mut at = hash(pointer) & (index.len() - 1);
        let mut visited = 0;
        while visited < index.len() {
            let page = index
                .page_tail(at, |n| charge(host, n + std::mem::size_of::<u32>()))?
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            for (offset, &code) in page.iter().take(index.len() - visited).enumerate() {
                if offset != 0 {
                    charge(host, std::mem::size_of::<u32>())?;
                }
                if code == EMPTY {
                    return Ok(at);
                }
                visited += 1;
                at = (at + 1) & (index.len() - 1);
            }
        }
        Err(host.reject(NativeViolation::InvalidProvenance))
    }
    fn find_code<H: Host<Lease = L>>(
        index: &NativeRows<u32, L>,
        pointer: usize,
        expected: u32,
        host: &mut H,
    ) -> Result<usize, H::Error> {
        if index.len() == 0 {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        let mut at = hash(pointer) & (index.len() - 1);
        let mut visited = 0;
        while visited < index.len() {
            let page = index
                .page_tail(at, |n| charge(host, n + std::mem::size_of::<u32>()))?
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            for (offset, &code) in page.iter().take(index.len() - visited).enumerate() {
                if offset != 0 {
                    charge(host, std::mem::size_of::<u32>())?;
                }
                if code == expected {
                    return Ok(at);
                }
                if code == EMPTY {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                }
                visited += 1;
                at = (at + 1) & (index.len() - 1);
            }
        }
        Err(host.reject(NativeViolation::InvalidProvenance))
    }
    pub(super) fn prepare<H: Host<Lease = L>>(
        &mut self,
        indexed: bool,
        host: &mut H,
    ) -> Result<(), H::Error> {
        self.mutable(host)?;
        if self.free.is_none() {
            let at = u32::try_from(self.slots.len())
                .ok()
                .filter(|n| *n <= u32::MAX - 2)
                .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
            self.slots.push(
                Slot {
                    pointer: 0,
                    generation: 0,
                    marked: false,
                    indexed: false,
                    state: State::Free(None),
                },
                host,
            )?;
            self.free = Some(at);
        }
        if !indexed {
            return Ok(());
        }
        let occupied = self
            .entries
            .checked_add(self.tombstones)
            .and_then(|v| v.checked_mul(2))
            .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
        if occupied < self.index.len() {
            return Ok(());
        }
        let count = if self.index.len() != 0 && self.entries <= self.index.len() / 4 {
            self.index.len()
        } else {
            self.index
                .len()
                .max(8)
                .checked_mul(2)
                .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?
        };
        self.pending_index = Some(NativeRows::default());
        self.pending_index
            .as_mut()
            .unwrap()
            .extend_repeated(EMPTY, count, host)?;
        let mut old_at = 0;
        while old_at < self.index.len() {
            let page = self
                .index
                .page_tail(old_at, |n| charge(host, n + std::mem::size_of::<u32>()))?
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            for (offset, &code) in page.iter().enumerate() {
                if offset != 0 {
                    charge(host, std::mem::size_of::<u32>())?;
                }
                if code < 2 {
                    continue;
                }
                let slot = self.slot(code - 2, host)?;
                if !slot.indexed || !matches!(slot.state, State::Live(_)) {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                }
                let at = Self::vacant(self.pending_index.as_ref().unwrap(), slot.pointer, host)?;
                Self::write_code(self.pending_index.as_mut().unwrap(), at, code, host)?;
            }
            old_at += page.len();
        }
        let old = std::mem::replace(&mut self.index, self.pending_index.take().unwrap());
        drop(old);
        self.tombstones = 0;
        Ok(())
    }
    pub(super) fn retain<H: Host<Lease = L>>(
        &mut self,
        pointer: usize,
        lease: L,
        indexed: bool,
        host: &mut H,
    ) -> Result<AllocationId, H::Error> {
        // Hold the original permit before any fallible probe. The backing is
        // still held by the caller and must unwind before this registry.
        if self.pending_permit.is_some() {
            // Correct callers prepare before obtaining a new backing.
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        self.pending_permit = Some(lease);
        if self.phase != Phase::Mutable || pointer == 0 || self.pending_index.is_some() {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        let index = if indexed {
            let found = self.locate(pointer, host)?;
            if found.id.is_some() {
                return Err(host.reject(NativeViolation::InvalidProvenance));
            }
            Some((found.position, found.code == TOMBSTONE))
        } else {
            None
        };
        let at = self
            .free
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        let target = self
            .slots
            .get_mut(at as usize, |n| charge(host, n))?
            .unwrap();
        let next = match target.state {
            State::Free(next) => next,
            State::Live(_) => return Err(host.reject(NativeViolation::InvalidProvenance)),
        };
        let generation = target
            .generation
            .checked_add(1)
            .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
        host.work(WorkDelta::Copy(std::mem::size_of::<Slot<L>>() as u64))?;
        // All fallible lookup/charging precedes this mutation's commit.
        let cell = if let Some((position, _)) = index {
            host.work(WorkDelta::Copy(4))?;
            Some(self.index.get_mut(position, |n| charge(host, n))?.unwrap())
        } else {
            None
        };
        *target = Slot {
            pointer,
            generation,
            marked: false,
            indexed,
            state: State::Live(self.pending_permit.take().unwrap()),
        };
        if let Some(cell) = cell {
            *cell = at + 2;
        }
        self.free = next;
        if let Some((_, tombstone)) = index {
            self.entries += 1;
            if tombstone {
                self.tombstones -= 1;
            }
        }
        Ok(AllocationId {
            slot: at,
            generation,
        })
    }
    pub(super) fn release_pointer<H: Host<Lease = L>>(
        &mut self,
        pointer: usize,
        host: &mut H,
    ) -> Result<(), H::Error> {
        if pointer == 0 {
            return Ok(());
        }
        self.mutable(host)?;
        let found = self.locate(pointer, host)?;
        let id = found
            .id
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        self.release_at(id, pointer, Some(found.position), host)
    }
    // Direct-ID owners do not have address-index entries.
    pub(super) fn release<H: Host<Lease = L>>(
        &mut self,
        id: AllocationId,
        pointer: usize,
        host: &mut H,
    ) -> Result<(), H::Error> {
        self.mutable(host)?;
        self.release_at(id, pointer, None, host)
    }
    fn release_at<H: Host<Lease = L>>(
        &mut self,
        id: AllocationId,
        pointer: usize,
        position: Option<usize>,
        host: &mut H,
    ) -> Result<(), H::Error> {
        host.work(WorkDelta::Copy(std::mem::size_of::<Slot<L>>() as u64))?;
        let slot = self
            .slots
            .get_mut(id.slot as usize, |n| charge(host, n))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        if slot.generation != id.generation
            || slot.pointer != pointer
            || slot.indexed != position.is_some()
            || !matches!(slot.state, State::Live(_))
        {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        // The checked position remains private to this same mutation; no user
        // callback or registry mutation occurs between locate and this borrow.
        if let Some(position) = position {
            // With exactly one entry and no tombstone every other cell is
            // EMPTY. Clearing this last cell cannot cut any probe chain.
            // All nonempty or previously tombstoned tables retain tombstones.
            let empty = self.entries == 1 && self.tombstones == 0;
            Self::write_code(
                &mut self.index,
                position,
                if empty { EMPTY } else { TOMBSTONE },
                host,
            )?;
            self.entries -= 1;
            if !empty {
                self.tombstones += 1;
            }
        }
        slot.pointer = 0;
        slot.marked = false;
        slot.indexed = false;
        slot.state = State::Free(self.free);
        self.free = Some(id.slot);
        Ok(())
    }
    pub(super) fn begin_mark<H: Host<Lease = L>>(&mut self, host: &mut H) -> Result<(), H::Error> {
        self.mutable(host)?;
        self.phase = Phase::Marking;
        Ok(())
    }
    pub(super) fn mark<H: Host<Lease = L>>(
        &mut self,
        id: AllocationId,
        pointer: usize,
        host: &mut H,
    ) -> Result<(), H::Error> {
        if self.phase != Phase::Marking {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        host.work(WorkDelta::Copy(1))?;
        let slot = self.slot_mut(id.slot, host)?;
        if slot.generation != id.generation
            || slot.pointer != pointer
            || !matches!(slot.state, State::Live(_))
        {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        slot.marked = true;
        Ok(())
    }
    pub(super) fn mark_string<H: Host<Lease = L>>(
        &mut self,
        value: &String,
        host: &mut H,
    ) -> Result<(), H::Error> {
        if value.capacity() == 0 {
            return Ok(());
        }
        let pointer = value.as_ptr() as usize;
        let found = self.locate(pointer, host)?;
        let id = found
            .id
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        self.mark(id, pointer, host)
    }
    pub(super) fn finish_mark<H: Host<Lease = L>>(&mut self, host: &mut H) -> Result<(), H::Error> {
        if self.phase != Phase::Marking {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        self.phase = Phase::Prepared;
        Ok(())
    }
    pub(super) fn retire<H: Host<Lease = L>>(&mut self, host: &mut H) -> Result<(), H::Error> {
        if self.phase != Phase::Prepared {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        let mut at = 0;
        while at < self.slots.len() {
            let page = self
                .slots
                .page_tail_mut(at, |n| charge(host, n))?
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            for (offset, slot) in page.iter_mut().enumerate() {
                if offset != 0 {
                    host.work(WorkDelta::Events(1))?;
                }
                if !slot.marked && matches!(slot.state, State::Live(_)) {
                    if slot.indexed {
                        // This actual mutable slot already proves pointer/live
                        // identity. The private index carries exactly its slot
                        // code, so finding that code need not reread this slot.
                        let code = u32::try_from(at + offset)
                            .ok()
                            .and_then(|n| n.checked_add(2))
                            .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
                        let position = Self::find_code(&self.index, slot.pointer, code, host)?;
                        Self::write_code(&mut self.index, position, TOMBSTONE, host)?;
                    }
                    host.work(WorkDelta::Copy(std::mem::size_of::<Slot<L>>() as u64))?;
                    slot.state = State::Free(None);
                    slot.pointer = 0;
                    slot.indexed = false;
                }
            }
            at += page.len();
        }
        drop(std::mem::take(&mut self.index));
        self.entries = 0;
        self.tombstones = 0;
        self.free = None;
        self.phase = Phase::Readonly;
        Ok(())
    }
}
