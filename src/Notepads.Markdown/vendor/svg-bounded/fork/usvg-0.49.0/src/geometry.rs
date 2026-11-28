// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private request adapters and shared ownership for geometry/representations.
use std::sync::Arc;

pub(crate) use markdown_svg_budget::RequestControl;
use markdown_svg_budget::{Error, PaidShared, ResourceKind, ResourcesRef};

/// Geometry comes from finite f32 paths. A domain escape violates the proven
/// producer envelope; control failures retain their original typed record.
pub(crate) fn math_error(
    processing: &crate::ProcessingContext,
    error: kurbo::common::MathError<Error>,
) -> Error {
    processing.record_resource(match error {
        kurbo::common::MathError::Control(error) => error,
        kurbo::common::MathError::Domain(_) => Error::new(
            markdown_svg_budget::FailureKind::Invariant,
            markdown_svg_budget::LimitKind::None,
        ),
    })
}

/// Ordinary sources retain Arc semantics. Request owners expose only borrowed
/// payloads and explicit policy-checked aliases; no context-free paid Clone.
#[derive(Debug)]
pub struct SharedOwner<T> {
    inner: SharedStorage<T>,
}
#[derive(Debug)]
enum SharedStorage<T> {
    Compat(Arc<T>),
    Paid(PaidShared<T>),
}
impl<T> std::ops::Deref for SharedOwner<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match &self.inner {
            SharedStorage::Compat(v) => v,
            SharedStorage::Paid(v) => v,
        }
    }
}
impl<T> AsRef<T> for SharedOwner<T> {
    fn as_ref(&self) -> &T {
        self
    }
}
impl<T> SharedOwner<T> {
    /// Adversarial test fixture only: real paid header around a deliberately
    /// mismatched payload. Production constructors always validate first.
    #[cfg(test)]
    pub(crate) fn test_with_header_unchecked(
        value: T,
        r: &ResourcesRef,
        kind: ResourceKind,
    ) -> Result<Self, Error> {
        PaidShared::try_new(r, kind, value).map(|v| Self {
            inner: SharedStorage::Paid(v),
        })
    }
    #[cfg(test)]
    pub(crate) fn test_alias_header_only(&self) -> Self {
        Self {
            inner: match &self.inner {
                SharedStorage::Compat(v) => SharedStorage::Compat(v.clone()),
                SharedStorage::Paid(v) => SharedStorage::Paid(v.clone()),
            },
        }
    }
    pub(crate) fn from_compat(value: Arc<T>) -> Self {
        Self {
            inner: SharedStorage::Compat(value),
        }
    }
    pub(crate) fn source_resources(&self) -> Option<&ResourcesRef> {
        match &self.inner {
            SharedStorage::Compat(_) => None,
            SharedStorage::Paid(v) => Some(v.resources()),
        }
    }
    pub(crate) fn is_paid(&self) -> bool {
        self.source_resources().is_some()
    }
    #[cfg(test)]
    pub(crate) fn belongs_to(&self, r: &ResourcesRef) -> bool {
        self.source_resources()
            .is_some_and(|s| s.is_same_request(r))
    }
    pub(crate) fn identity(&self) -> usize {
        self.as_ref() as *const T as usize
    }
    /// Whether two immutable references name the same shared allocation.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.inner, &other.inner) {
            (SharedStorage::Compat(a), SharedStorage::Compat(b)) => Arc::ptr_eq(a, b),
            (SharedStorage::Paid(a), SharedStorage::Paid(b)) => a.ptr_eq(b),
            _ => false,
        }
    }
    pub(crate) fn validate_header(&self, p: &crate::ProcessingContext) -> Result<(), Error> {
        crate::tree::owner::validate_owner(p, self.source_resources())
    }
}
impl<T> SharedOwner<T> {
    pub(crate) fn try_new_with(
        p: &crate::ProcessingContext,
        kind: ResourceKind,
        make: impl FnOnce() -> Result<T, Error>,
    ) -> Result<Self, Error>
    where
        T: crate::tree::owner::PolicyValue,
    {
        crate::tree::owner::check(p, 0)?;
        let result = match p.resources() {
            Some(r) => PaidShared::try_new_with(r, kind, || {
                let value = make().map_err(|e| p.record_resource(e))?;
                value.validate_policy(p)?;
                Ok(value)
            })
            .map(|v| Self {
                inner: SharedStorage::Paid(v),
            }),
            None => {
                let value = make().map_err(|e| p.record_resource(e))?;
                value.validate_policy(p)?;
                Ok(Self::from_compat(Arc::new(value)))
            }
        };
        crate::tree::owner::finish(p, result)
    }
    pub(crate) fn try_new(
        value: T,
        resources: Option<&ResourcesRef>,
        kind: ResourceKind,
    ) -> Result<Self, Error>
    where
        T: crate::tree::owner::PolicyValue,
    {
        let p = match resources {
            Some(r) => crate::ProcessingContext::with_resources(r.clone()),
            None => crate::ProcessingContext::default(),
        };
        value.validate_policy(&p)?;
        let result = match resources {
            Some(r) => PaidShared::try_new(r, kind, value).map(|v| Self {
                inner: SharedStorage::Paid(v),
            }),
            None => Ok(Self::from_compat(Arc::new(value))),
        };
        crate::tree::owner::finish(&p, result)
    }
    pub(crate) fn try_alias_in(&self, p: &crate::ProcessingContext) -> Result<Self, Error>
    where
        T: crate::tree::owner::PolicyValue,
    {
        use crate::tree::owner::PolicyValue;
        self.validate_policy(p)?;
        let candidate = Self {
            inner: match &self.inner {
                SharedStorage::Compat(v) => SharedStorage::Compat(v.clone()),
                SharedStorage::Paid(v) => SharedStorage::Paid(v.clone()),
            },
        };
        crate::tree::owner::finish(p, Ok(candidate))
    }
    pub(crate) fn try_get_mut_if_unique(
        &mut self,
        p: &crate::ProcessingContext,
    ) -> Result<Option<&mut T>, Error>
    where
        T: crate::tree::owner::PolicyValue,
    {
        use crate::tree::owner::PolicyValue;
        self.validate_policy(p)?;
        Ok(match &mut self.inner {
            SharedStorage::Compat(v) => Arc::get_mut(v),
            SharedStorage::Paid(v) => v.get_mut_if_unique(),
        })
    }
    pub(crate) fn try_unwrap_in(
        self,
        p: &crate::ProcessingContext,
    ) -> Result<Result<T, Self>, Error>
    where
        T: crate::tree::owner::PolicyValue,
    {
        use crate::tree::owner::PolicyValue;
        self.validate_policy(p)?;
        let result = match self.inner {
            SharedStorage::Compat(v) => Arc::try_unwrap(v).map_err(|v| Self {
                inner: SharedStorage::Compat(v),
            }),
            SharedStorage::Paid(v) => v.try_unwrap().map_err(|v| Self {
                inner: SharedStorage::Paid(v),
            }),
        };
        crate::tree::owner::finish(p, Ok(result))
    }
}
impl<T: crate::tree::owner::PolicyValue> crate::tree::owner::PolicyValue for SharedOwner<T> {
    fn validate_policy(&self, p: &crate::ProcessingContext) -> Result<(), Error> {
        self.validate_header(p)?;
        self.as_ref().validate_policy(p)
    }
}
impl<T: crate::tree::owner::PolicyValue> crate::tree::owner::TryCopy for SharedOwner<T> {
    fn try_copy_in(&self, p: &crate::ProcessingContext) -> Result<Self, Error> {
        self.try_alias_in(p)
    }
}

/// Import the callback's explicit outline source into the selected request.
/// This is a deep P import, not a Plain scene/Tree migration or paid alias.
pub(crate) fn import_outline(
    data: Option<Arc<tiny_skia_path::Path>>,
    p: &crate::ProcessingContext,
) -> Result<Option<SharedOwner<tiny_skia_path::Path>>, Error> {
    crate::tree::owner::check(p, 0)?;
    data.map(|data| -> Result<_, Error> {
        // The primary marker chooses the source policy; it is never itself
        // proof that the opaque header/arrays have that policy.
        let target = if data.request_resources().is_some() {
            p.resources()
        } else {
            None
        };
        data.validate_policy(target)
            .map_err(|e| p.record_resource(e))?;
        match p.resources() {
            None => Ok(SharedOwner::from_compat(data)),
            Some(r) => {
                let copied = data
                    .as_ref()
                    .try_copy_in(r, ResourceKind::Geometry)
                    .map_err(|e| p.record_resource(e));
                drop(data);
                crate::tree::owner::check(p, 0)?;
                SharedOwner::try_new(copied?, Some(r), ResourceKind::Geometry)
            }
        }
    })
    .transpose()
    .and_then(|v| crate::tree::owner::finish(p, Ok(v)))
}
