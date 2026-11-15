// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Explicit cloning keeps request-owned databases out of implicit Arc COW.

use super::{Database, FaceInfo, Source};
use alloc::{collections::TryReserveError, string::String, vec::Vec};
use core::fmt;

/// A failure in the compatibility database mutation path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatCloneError {
    /// A request-owned, immutable database must use its funded factory.
    ControlledReadOnly,
    /// A compatibility allocation could not be reserved.
    Allocation,
}
impl From<TryReserveError> for CompatCloneError {
    fn from(_: TryReserveError) -> Self {
        Self::Allocation
    }
}
impl fmt::Display for CompatCloneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ControlledReadOnly => "request-owned font database is read-only",
            Self::Allocation => "font database allocation failed",
        })
    }
}
#[cfg(feature = "std")]
impl std::error::Error for CompatCloneError {}

fn clone_string(text: &str) -> Result<String, TryReserveError> {
    let mut copy = String::new();
    copy.try_reserve_exact(text.len())?;
    copy.push_str(text);
    Ok(copy)
}

#[cfg(feature = "fs")]
fn clone_path(path: &std::path::Path) -> Result<std::path::PathBuf, TryReserveError> {
    let mut copy = std::path::PathBuf::new();
    copy.try_reserve_exact(path.as_os_str().len())?;
    copy.push(path);
    Ok(copy)
}

fn clone_source(source: &Source) -> Result<Source, TryReserveError> {
    Ok(match source {
        Source::Binary(owner) => Source::Binary(owner.clone()),
        #[cfg(feature = "fs")]
        Source::File(path) => Source::File(clone_path(path)?),
        #[cfg(all(feature = "fs", feature = "memmap"))]
        Source::SharedFile(path, owner) => Source::SharedFile(clone_path(path)?, owner.clone()),
    })
}

fn clone_face(info: &FaceInfo) -> Result<FaceInfo, TryReserveError> {
    let mut families = Vec::new();
    families.try_reserve_exact(info.families.len())?;
    for (name, language) in &info.families {
        families.push((clone_string(name)?, *language));
    }
    Ok(FaceInfo {
        id: info.id,
        source: clone_source(&info.source)?,
        index: info.index,
        families,
        post_script_name: clone_string(&info.post_script_name)?,
        style: info.style,
        weight: info.weight,
        stretch: info.stretch,
        monospaced: info.monospaced,
    })
}

impl Database {
    /// Whether this database owns request-funded immutable storage.
    ///
    /// The already prepaid bootstrap database is deliberately not funded again.
    pub fn is_funded(&self) -> bool {
        #[cfg(feature = "notepads-markdown-resources")]
        {
            self.funding.is_some()
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        {
            false
        }
    }

    /// Explicitly copies an unfunded compatibility database without Arc COW.
    ///
    /// Request-owned versions are rejected before allocating. Occupied keys,
    /// vacant versions/free links and every face property remain unchanged.
    pub fn try_clone_unfunded(&self) -> Result<Self, CompatCloneError> {
        if self.is_funded() {
            return Err(CompatCloneError::ControlledReadOnly);
        }
        Ok(Self {
            faces: self.faces.try_clone_with(clone_face)?,
            family_serif: clone_string(&self.family_serif)?,
            family_sans_serif: clone_string(&self.family_sans_serif)?,
            family_cursive: clone_string(&self.family_cursive)?,
            family_fantasy: clone_string(&self.family_fantasy)?,
            family_monospace: clone_string(&self.family_monospace)?,
            #[cfg(feature = "notepads-markdown-resources")]
            funding: None,
        })
    }
}

#[cfg(feature = "notepads-markdown-resources")]
use {
    super::{Language, Stretch, Style, Weight, ID},
    alloc::sync::Arc,
    core::alloc::Layout,
    markdown_svg_budget::{
        BudgetVec, Error, FailureKind, Lease, LimitKind, ResourceKind, ResourcesRef,
    },
};

/// One actual face selected by the existing Native font contract.
#[cfg(feature = "notepads-markdown-resources")]
pub struct FaceRegistration<'a> {
    /// Already owned font bytes; cloning this owner allocates no new payload.
    pub source: Arc<dyn AsRef<[u8]> + Send + Sync>,
    /// Exact face index in that source, including TTC collections.
    pub index: u32,
    /// Diagnostic identity only; this does not reselect a font by family.
    pub family: &'a str,
}

#[cfg(feature = "notepads-markdown-resources")]
impl fmt::Debug for FaceRegistration<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FaceRegistration")
            .field("index", &self.index)
            .field("family", &self.family)
            .finish_non_exhaustive()
    }
}

// Slots are initialized before any payload allocation. Installing a lease can
// therefore never fail because a shared first fault arrived during construction.
// In particular, try_push(fee) would drop fee before its live String/slot array
// on an error, even when capacity had already been reserved.
#[cfg(feature = "notepads-markdown-resources")]
struct Funding {
    resources: ResourcesRef,
    fees: BudgetVec<Option<Lease>>,
    next: usize,
}

#[cfg(feature = "notepads-markdown-resources")]
impl Funding {
    fn new(resources: &ResourcesRef, count: usize) -> Result<Self, Error> {
        let mut fees = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        fees.try_reserve_exact(count)?;
        for _ in 0..count {
            resources.checkpoint(1)?;
            fees.try_push(None)?;
        }
        Ok(Self {
            resources: resources.clone(),
            fees,
            next: 0,
        })
    }

    fn slot(&mut self) -> Result<usize, Error> {
        if self.next == self.fees.len() {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        let index = self.next;
        self.next += 1;
        Ok(index)
    }

    fn reserve(&mut self, layout: Layout) -> Result<usize, Error> {
        let index = self.slot()?;
        let fee = self
            .resources
            .reserve(ResourceKind::Scratch, layout.size())?;
        // index was checked and initialized by slot(), before reserving.
        self.fees.as_mut_slice()[index] = Some(fee);
        Ok(index)
    }

    fn committed(&mut self, index: usize) -> Result<(), Error> {
        self.fees.as_mut_slice()[index]
            .as_mut()
            .expect("reserved font database storage")
            .allocation_committed()
    }

    fn text(&mut self, text: &str) -> Result<String, Error> {
        let layout = Layout::array::<u8>(text.len()).map_err(|_| {
            self.resources
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
        })?;
        let index = self.reserve(layout)?;
        let mut value = String::new();
        value.try_reserve_exact(text.len()).map_err(|_| {
            self.resources
                .record(Error::new(FailureKind::Allocation, LimitKind::Capacity))
        })?;
        if value.capacity() != text.len() {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        self.committed(index)?;
        let mut rest = text;
        while !rest.is_empty() {
            let mut end = rest.len().min(65_536);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            self.resources.consume_scan_work(end)?;
            value.push_str(&rest[..end]);
            rest = &rest[end..];
        }
        Ok(value)
    }

    fn families(&mut self, names: &[(String, Language)]) -> Result<Vec<(String, Language)>, Error> {
        let mut values = self.family_storage(names.len())?;
        for (name, language) in names {
            self.resources.checkpoint(1)?;
            values.push((self.text(name)?, *language));
        }
        Ok(values)
    }

    fn family_storage(&mut self, count: usize) -> Result<Vec<(String, Language)>, Error> {
        let layout = Layout::array::<(String, Language)>(count).map_err(|_| {
            self.resources
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
        })?;
        let index = self.reserve(layout)?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| {
            self.resources
                .record(Error::new(FailureKind::Allocation, LimitKind::Capacity))
        })?;
        if values.capacity() != count {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        self.committed(index)?;
        Ok(values)
    }

    fn face(&mut self, info: &FaceInfo) -> Result<FaceInfo, Error> {
        self.resources.checkpoint(1)?;
        let owner = match &info.source {
            Source::Binary(owner) => owner,
            #[cfg(feature = "fs")]
            _ => {
                return Err(self
                    .resources
                    .record(Error::new(FailureKind::Invariant, LimitKind::None)));
            }
        };
        Ok(FaceInfo {
            id: info.id,
            source: Source::Binary(owner.clone()),
            index: info.index,
            families: self.families(&info.families)?,
            post_script_name: self.text(&info.post_script_name)?,
            style: info.style,
            weight: info.weight,
            stretch: info.stretch,
            monospaced: info.monospaced,
        })
    }
}

#[cfg(feature = "notepads-markdown-resources")]
impl Database {
    /// Builds a new immutable version with exact, request-funded metadata.
    ///
    /// Existing keys, free links, face properties and Binary source owners are
    /// preserved. `ids` must have exactly `additional` elements; partial output
    /// on failure is private staging data and must never be published. The
    /// factory is called exactly once per new face, in registration order.
    /// The caller separately pays/tracks the new containing Arc allocation.
    pub fn try_extend_in<'a, F>(
        &self,
        resources: &ResourcesRef,
        additional: usize,
        mut face: F,
        ids: &mut [ID],
    ) -> Result<Database, Error>
    where
        F: FnMut(usize) -> Result<FaceRegistration<'a>, Error>,
    {
        if ids.len() != additional
            || self
                .funding
                .as_ref()
                .is_some_and(|fees| !fees.belongs_to(resources))
        {
            return Err(resources.record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        resources.checkpoint(1)?;
        // Slot array + five generic names; two slots per new face (family
        // array/name). Empty allocations still occupy an initialized None/
        // zero-byte lease slot, but never create a backend reservation.
        let mut count = additional
            .checked_mul(2)
            .and_then(|count| count.checked_add(6))
            .ok_or_else(|| resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity)))?;
        for old in self.faces() {
            resources.checkpoint(1)?;
            if !matches!(&old.source, Source::Binary(_)) {
                return Err(resources.record(Error::new(FailureKind::Invariant, LimitKind::None)));
            }
            count = count
                .checked_add(old.families.len())
                .and_then(|count| count.checked_add(2))
                .ok_or_else(|| {
                    resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity))
                })?;
        }
        // This owner is declared before every new raw payload, so its fees
        // drop after all partial payloads on every Err and unwind path.
        let mut funding = Funding::new(resources, count)?;
        let slot_fee_index = funding.slot()?;
        let (mut faces, slot_fee) =
            self.faces
                .try_clone_with_extra_in(additional, resources, |old| funding.face(old))?;
        // No fallible operation or callback between receiving and installing
        // the already-paired slot-map lease in a preinitialized fee slot.
        funding.fees.as_mut_slice()[slot_fee_index] = Some(slot_fee);
        let family_serif = funding.text(&self.family_serif)?;
        let family_sans_serif = funding.text(&self.family_sans_serif)?;
        let family_cursive = funding.text(&self.family_cursive)?;
        let family_fantasy = funding.text(&self.family_fantasy)?;
        let family_monospace = funding.text(&self.family_monospace)?;
        for (index, output) in ids.iter_mut().enumerate() {
            resources.checkpoint(1)?;
            let registration = face(index).map_err(|error| resources.record(error))?;
            let mut families = funding.family_storage(1)?;
            families.push((
                funding.text(registration.family)?,
                Language::English_UnitedStates,
            ));
            let info = FaceInfo {
                id: ID::dummy(),
                source: Source::Binary(registration.source),
                index: registration.index,
                families,
                post_script_name: String::new(),
                style: Style::Normal,
                weight: Weight::NORMAL,
                stretch: Stretch::Normal,
                monospaced: false,
            };
            let id = faces.try_insert_with_key_noalloc(resources, |key| {
                Ok(FaceInfo {
                    id: ID(key),
                    ..info
                })
            })?;
            *output = ID(id);
        }
        resources.checkpoint(1)?;
        Ok(Database {
            faces,
            family_serif,
            family_sans_serif,
            family_cursive,
            family_fantasy,
            family_monospace,
            funding: Some(funding.fees),
        })
    }
}

#[cfg(test)]
mod tests;
