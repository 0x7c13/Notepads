// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! One owner for both path arrays. A paid clone shares immutable storage;
//! mutation first obtains a separately paid copy while the source stays live.
use alloc::vec::Vec;
use core::ops::Deref;

use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, PaidShared, ResourceKind, ResourcesRef,
};

use crate::{PathVerb, Point};

pub(crate) type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Debug, Default)]
pub(crate) struct Control(pub(crate) Option<ResourcesRef>);

impl Control {
    pub(crate) fn request(resources: &ResourcesRef) -> Self {
        Self(Some(resources.clone()))
    }

    pub(crate) fn check(&self, work: u64) -> Result<()> {
        match &self.0 {
            Some(resources) => resources.checkpoint(work),
            None => Ok(()),
        }
    }

    pub(crate) fn fail(&self, kind: FailureKind, limit: LimitKind) -> Error {
        let error = Error::new(kind, limit);
        self.0.as_ref().map_or(error, |r| r.record(error))
    }

    pub(crate) fn source(&self, storage: &GeometryStorage) -> Result<()> {
        if let (Some(request), Some(source)) = (&self.0, storage.resources()) {
            if !request.is_same_request(source) {
                return Err(self.fail(FailureKind::Invariant, LimitKind::None));
            }
        }
        self.check(0)
    }
}

#[derive(Debug)]
pub(crate) struct PathBuffers {
    verbs: BudgetVec<PathVerb>,
    points: BudgetVec<Point>,
}

#[derive(Clone, Debug)]
pub(crate) enum GeometryStorage {
    Compat {
        verbs: Vec<PathVerb>,
        points: Vec<Point>,
    },
    Paid {
        resources: ResourcesRef,
        kind: ResourceKind,
        owner: Option<PaidShared<PathBuffers>>,
    },
}

impl Default for GeometryStorage {
    fn default() -> Self {
        Self::Compat {
            verbs: Vec::new(),
            points: Vec::new(),
        }
    }
}

impl PartialEq for GeometryStorage {
    fn eq(&self, other: &Self) -> bool {
        self.verbs() == other.verbs() && self.points() == other.points()
    }
}

impl GeometryStorage {
    pub(crate) fn compat(verbs: Vec<PathVerb>, points: Vec<Point>) -> Self {
        Self::Compat { verbs, points }
    }

    pub(crate) fn with_resources(resources: &ResourcesRef, kind: ResourceKind) -> Self {
        Self::Paid {
            resources: resources.clone(),
            kind,
            owner: None,
        }
    }

    /// Explicit preallocation pays exact capacities; normal prepare keeps its
    /// amortized growth policy. Double zero remains allocation-free PaidEmpty.
    pub(crate) fn try_with_capacities(
        resources: &ResourcesRef,
        kind: ResourceKind,
        verbs: usize,
        points: usize,
    ) -> Result<Self> {
        resources.checkpoint(0)?;
        let owner = if verbs == 0 && points == 0 {
            None
        } else {
            Some(Self::make_owner(resources, kind, &[], &[], verbs, points)?)
        };
        resources.checkpoint(0)?;
        Ok(Self::Paid {
            resources: resources.clone(),
            kind,
            owner,
        })
    }

    pub(crate) fn control(&self) -> Control {
        Control(self.resources().cloned())
    }

    pub(crate) fn resources(&self) -> Option<&ResourcesRef> {
        match self {
            Self::Compat { .. } => None,
            Self::Paid { resources, .. } => Some(resources),
        }
    }

    pub(crate) fn validate_policy(&self, target: Option<&ResourcesRef>) -> Result<()> {
        let sources = match self {
            Self::Compat { .. } => {
                return match target {
                    None => Ok(()),
                    Some(target) => {
                        Err(target.record(Error::new(FailureKind::Invariant, LimitKind::None)))
                    }
                };
            }
            Self::Paid {
                resources, owner, ..
            } => [
                Some(resources),
                owner.as_ref().map(PaidShared::resources),
                owner.as_ref().map(|owner| owner.verbs.resources()),
                owner.as_ref().map(|owner| owner.points.resources()),
            ],
        };
        let existing = target
            .and_then(ResourcesRef::failure)
            .or_else(|| sources.iter().flatten().find_map(|source| source.failure()));
        let mismatch = sources
            .iter()
            .flatten()
            .any(|source| target.is_none_or(|target| !source.is_same_request(target)));
        if mismatch || existing.is_some() {
            let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
            if mismatch {
                for source in sources.iter().flatten() {
                    source.record(invariant);
                }
            }
            let error = existing.unwrap_or(invariant);
            return Err(target.map_or(error, |target| target.record(error)));
        }
        Ok(())
    }

    pub(crate) fn kind(&self) -> Option<ResourceKind> {
        match self {
            Self::Compat { .. } => None,
            Self::Paid { kind, .. } => Some(*kind),
        }
    }

    pub(crate) fn retained_unpaid_capacity_bytes(&self) -> Option<usize> {
        match self {
            Self::Paid { .. } => Some(0),
            Self::Compat { verbs, points } => verbs
                .capacity()
                .checked_mul(core::mem::size_of::<PathVerb>())?
                .checked_add(
                    points
                        .capacity()
                        .checked_mul(core::mem::size_of::<Point>())?,
                ),
        }
    }

    pub(crate) fn verbs(&self) -> &[PathVerb] {
        match self {
            Self::Compat { verbs, .. } => verbs,
            Self::Paid { owner, .. } => owner.as_ref().map_or(&[], |owner| owner.verbs.as_slice()),
        }
    }

    pub(crate) fn points(&self) -> &[Point] {
        match self {
            Self::Compat { points, .. } => points,
            Self::Paid { owner, .. } => owner.as_ref().map_or(&[], |owner| owner.points.as_slice()),
        }
    }

    fn make_owner(
        resources: &ResourcesRef,
        kind: ResourceKind,
        verbs: &[PathVerb],
        points: &[Point],
        verb_capacity: usize,
        point_capacity: usize,
    ) -> Result<PaidShared<PathBuffers>> {
        PaidShared::try_new_with(resources, kind, || {
            let mut output = PathBuffers {
                verbs: BudgetVec::new(resources.clone(), kind),
                points: BudgetVec::new(resources.clone(), kind),
            };
            output.verbs.try_reserve_exact(verb_capacity)?;
            output.points.try_reserve_exact(point_capacity)?;
            for chunk in verbs.chunks(256) {
                resources.checkpoint(chunk.len() as u64)?;
                output.verbs.extend_reserved_copy(chunk);
            }
            for chunk in points.chunks(256) {
                resources.checkpoint(chunk.len() as u64)?;
                output.points.extend_reserved_copy(chunk);
            }
            resources.checkpoint(0)?;
            Ok(output)
        })
    }

    pub(crate) fn try_copy_in(&self, resources: &ResourcesRef, kind: ResourceKind) -> Result<Self> {
        let control = Control::request(resources);
        control.source(self)?;
        if self.kind() == Some(kind) {
            return Ok(self.clone());
        }
        let owner = if self.verbs().is_empty() && self.points().is_empty() {
            None
        } else {
            Some(Self::make_owner(
                resources,
                kind,
                self.verbs(),
                self.points(),
                self.verbs().len(),
                self.points().len(),
            )?)
        };
        resources.checkpoint(0)?;
        Ok(Self::Paid {
            resources: resources.clone(),
            kind,
            owner,
        })
    }

    pub(crate) fn prepare(&mut self, verbs: usize, points: usize) -> Result<()> {
        match self {
            Self::Compat {
                verbs: v,
                points: p,
            } => {
                v.reserve(verbs);
                p.reserve(points);
                Ok(())
            }
            Self::Paid {
                resources,
                kind,
                owner,
            } => {
                resources.checkpoint(0)?;
                if owner.is_none() && verbs == 0 && points == 0 {
                    return Ok(());
                }
                let shared = owner
                    .as_mut()
                    .is_some_and(|owner| owner.get_mut_if_unique().is_none());
                if shared {
                    let source = owner.as_ref().expect("shared path storage");
                    let copy = Self::make_owner(
                        resources,
                        *kind,
                        &source.verbs,
                        &source.points,
                        source.verbs.capacity(),
                        source.points.capacity(),
                    )?;
                    let old = owner.replace(copy);
                    drop(old);
                    resources.checkpoint(0)?;
                } else if owner.is_none() {
                    *owner = Some(Self::make_owner(resources, *kind, &[], &[], 0, 0)?);
                }
                let output = owner
                    .as_mut()
                    .expect("paid path storage")
                    .get_mut_if_unique()
                    .expect("exclusive path mutation");
                output.verbs.try_reserve(verbs)?;
                output.points.try_reserve(points)?;
                resources.checkpoint(0)
            }
        }
    }

    /// A bounded command commit after prepare and the caller's work payment.
    pub(crate) fn append_reserved(&mut self, verbs: &[PathVerb], points: &[Point]) {
        match self {
            Self::Compat {
                verbs: v,
                points: p,
            } => {
                assert!(verbs.len() <= v.capacity() - v.len());
                assert!(points.len() <= p.capacity() - p.len());
                for &verb in verbs {
                    v.push(verb);
                }
                for &point in points {
                    p.push(point);
                }
            }
            Self::Paid { owner, .. } => {
                let output = owner
                    .as_mut()
                    .expect("prepared path storage")
                    .get_mut_if_unique()
                    .expect("exclusive path mutation");
                output.verbs.extend_reserved_copy(verbs);
                output.points.extend_reserved_copy(points);
            }
        }
    }

    pub(crate) fn points_mut(&mut self) -> &mut [Point] {
        match self {
            Self::Compat { points, .. } => points,
            Self::Paid { owner, .. } => owner
                .as_mut()
                .expect("prepared path storage")
                .get_mut_if_unique()
                .expect("exclusive path mutation")
                .points
                .as_mut_slice(),
        }
    }

    pub(crate) fn clear(&mut self) -> Result<()> {
        match self {
            Self::Compat { verbs, points } => {
                verbs.clear();
                points.clear();
                Ok(())
            }
            Self::Paid {
                resources, owner, ..
            } => {
                resources.checkpoint(0)?;
                if let Some(values) = owner.as_mut().and_then(PaidShared::get_mut_if_unique) {
                    values.verbs.clear();
                    values.points.clear();
                } else {
                    drop(owner.take());
                }
                resources.checkpoint(0)
            }
        }
    }
}

/// Scratch vectors used by contour measurement. Only Copy elements are admitted
/// so no uncharged element Clone or destructor hides in an append operation.
#[derive(Debug)]
pub(crate) enum ScratchVec<T: Copy> {
    Compat(Vec<T>),
    Paid(BudgetVec<T>),
}

impl<T: Copy> ScratchVec<T> {
    pub(crate) fn new(control: &Control) -> Self {
        match &control.0 {
            Some(resources) => Self::Paid(BudgetVec::new(resources.clone(), ResourceKind::Scratch)),
            None => Self::Compat(Vec::new()),
        }
    }

    pub(crate) fn push(&mut self, value: T) -> Result<()> {
        match self {
            Self::Compat(values) => {
                values.push(value);
                Ok(())
            }
            Self::Paid(values) => values.try_push(value),
        }
    }
}

impl<T: Copy> Deref for ScratchVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Compat(values) => values,
            Self::Paid(values) => values.as_slice(),
        }
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

#[cfg(test)]
pub(crate) fn record_dash_builder_start() {
    tests::record_dash_builder_start();
}

#[cfg(test)]
pub(crate) fn record_dash_contour_start() {
    tests::record_dash_contour_start();
}
