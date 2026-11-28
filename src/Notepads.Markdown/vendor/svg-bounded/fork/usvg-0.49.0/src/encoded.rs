// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Encoded image ownership across resolver, scene clones and raster decoding.
use markdown_svg_budget::{
    BudgetVec, ChildAdmission, Error, FailureKind, LimitKind, PaidEncoded, PaidShared,
    ResourceKind, ResourcesRef,
};
use std::{fmt, ops::Deref, sync::Arc};

enum Storage {
    Compatibility(Arc<Vec<u8>>),
    Paid(PaidEncoded),
}

/// Shared encoded bytes. Controlled bytes keep their allocation and fee paired
/// until the final actual reader retires; no unpaid storage escape is exposed.
pub struct EncodedImageData {
    storage: Storage,
}

impl EncodedImageData {
    pub(crate) fn root_resources(&self) -> [Option<&ResourcesRef>; 2] {
        match &self.storage {
            Storage::Compatibility(_) => [None, None],
            Storage::Paid(v) => [Some(v.resources()), Some(v.as_ref().resources())],
        }
    }
    /// Borrow the original immutable encoded bytes without copying them.
    pub fn as_slice(&self) -> &[u8] {
        match &self.storage {
            Storage::Compatibility(bytes) => bytes.as_slice(),
            Storage::Paid(bytes) => bytes.as_ref(),
        }
    }

    pub(crate) fn try_from_paid(
        resources: &ResourcesRef,
        bytes: BudgetVec<u8>,
        admission: ChildAdmission,
    ) -> Result<Self, Error> {
        let p = crate::ProcessingContext::with_resources(resources.clone());
        crate::tree::owner::validate_owner(&p, Some(bytes.resources()))?;
        if !admission.matches(resources, bytes.len()) {
            return Err(resources.record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        let encoded = PaidShared::try_new(resources, ResourceKind::Scratch, bytes)?;
        Ok(Self {
            storage: Storage::Paid(encoded),
        })
    }

    pub(crate) fn try_copy_from_slice(
        resources: &ResourcesRef,
        source: &[u8],
    ) -> Result<Self, Error> {
        let admission = resources.admit_child_encoded(source.len())?;
        let mut bytes = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        bytes.try_reserve_exact(source.len())?;
        for chunk in source.chunks(256) {
            resources.checkpoint(0)?;
            resources.consume_scan_work(chunk.len())?;
            bytes.try_extend_from_slice(chunk)?;
            resources.checkpoint(0)?;
        }
        Self::try_from_paid(resources, bytes, admission)
    }

    pub(crate) fn is_admitted_for(&self, resources: &ResourcesRef) -> bool {
        match &self.storage {
            Storage::Compatibility(_) => false,
            Storage::Paid(bytes) => {
                bytes.belongs_to(resources) && bytes.as_ref().belongs_to(resources)
            }
        }
    }
    /// Check the actual shared header and byte buffer, preserving both slots.
    pub fn validate_policy(&self, target: Option<&ResourcesRef>) -> Result<(), Error> {
        let sources = match &self.storage {
            Storage::Compatibility(_) => {
                return match target {
                    None => Ok(()),
                    Some(r) => Err(r.record(Error::new(FailureKind::Invariant, LimitKind::None))),
                };
            }
            Storage::Paid(v) => [v.resources(), v.as_ref().resources()],
        };
        let existing = target
            .and_then(ResourcesRef::failure)
            .or_else(|| sources.iter().find_map(|r| r.failure()));
        let mismatch = sources
            .iter()
            .any(|r| target.is_none_or(|t| !t.is_same_request(r)));
        if mismatch || existing.is_some() {
            let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
            if mismatch {
                for r in sources {
                    r.record(invariant);
                }
            }
            let e = existing.unwrap_or(invariant);
            return Err(target.map_or(e, |r| r.record(e)));
        }
        Ok(())
    }
    /// Explicit immutable alias; no paid owner can be downgraded to an Arc.
    pub fn try_alias_in(&self, p: &crate::ProcessingContext) -> Result<Self, Error> {
        crate::tree::owner::PolicyValue::validate_policy(self, p)?;
        let value = Self {
            storage: match &self.storage {
                Storage::Compatibility(v) => Storage::Compatibility(v.clone()),
                Storage::Paid(v) => Storage::Paid(v.clone()),
            },
        };
        crate::tree::owner::finish(p, Ok(value))
    }
    /// Compatibility payload accounting only. Paid bytes already retain their
    /// own real allocation fee, independently of a representation's lifetime.
    pub(crate) fn retained_unpaid_capacity(&self) -> usize {
        match &self.storage {
            Storage::Compatibility(bytes) => bytes.capacity(),
            Storage::Paid(_) => 0,
        }
    }
}

impl From<Arc<Vec<u8>>> for EncodedImageData {
    fn from(bytes: Arc<Vec<u8>>) -> Self {
        Self {
            storage: Storage::Compatibility(bytes),
        }
    }
}
impl Deref for EncodedImageData {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl AsRef<[u8]> for EncodedImageData {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}
impl fmt::Debug for EncodedImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedImageData")
            .field("bytes", &self.len())
            .field("paid", &matches!(self.storage, Storage::Paid(_)))
            .finish()
    }
}

impl crate::tree::owner::PolicyValue for EncodedImageData {
    fn validate_policy(&self, p: &crate::ProcessingContext) -> Result<(), Error> {
        crate::tree::owner::check(p, 0)?;
        self.validate_policy(p.resources())
            .map_err(|e| p.record_resource(e))
    }
}
impl crate::tree::owner::TryCopy for EncodedImageData {
    fn try_copy_in(&self, p: &crate::ProcessingContext) -> Result<Self, Error> {
        self.try_alias_in(p)
    }
}
