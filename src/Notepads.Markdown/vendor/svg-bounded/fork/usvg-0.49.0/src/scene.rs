// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Cumulative materialized scene admission and request-owned scene allocation.
//! Owned Clone edges deliberately differ from Node::subroots: shared Arc
//! resources are aliases, while an embedded SVG Tree and Text.flattened copy.

use crate::tree::owner::{PolicyValue, TryCopy};
use crate::{Group, Image, ImageKind, Node, Path, ProcessingContext, SceneBox, Text, Tree};
use markdown_svg_budget::{Error, FailureKind, LimitKind};

const MAX_OWNED_DEPTH: usize = 128;

/// Cost of a newly materialized scene identity. Previously admitted children
/// moved into that identity are not included again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SceneCost {
    nodes: u64,
    segments: u64,
}

impl SceneCost {
    pub(crate) const NODE: Self = Self {
        nodes: 1,
        segments: 0,
    };

    /// Every new scene Path instance costs all verbs, even when its P storage
    /// is shared. Temporary/cache P geometry is not a scene Path instance.
    pub(crate) fn path(path: &Path) -> Self {
        const { assert!(usize::BITS <= u64::BITS) };
        Self {
            nodes: 1,
            segments: path.data().len() as u64,
        }
    }
}

fn check(processing: &ProcessingContext) -> Result<(), Error> {
    if let Some(error) = processing.resource_failure() {
        return Err(processing.record_resource(error));
    }
    if let Some(resources) = processing.resources() {
        resources
            .checkpoint(0)
            .map_err(|error| processing.record_resource(error))?;
    }
    Ok(())
}

fn admit(processing: &ProcessingContext, cost: SceneCost) -> Result<(), Error> {
    check(processing)?;
    if let Some(resources) = processing.resources() {
        resources
            .admit_scene(cost.nodes, cost.segments)
            .map_err(|error| processing.record_resource(error))?;
    }
    Ok(())
}

fn finish<T>(processing: &ProcessingContext, candidate: T) -> Result<T, Error> {
    match check(processing) {
        Ok(()) => Ok(candidate),
        Err(error) => {
            // The candidate remains owned until this branch. Cleanup may add
            // retirement failures, but cannot replace the already fixed first.
            drop(candidate);
            Err(processing.record_resource(error))
        }
    }
}

/// Admit only this newly materialized identity before evaluating its factory.
/// Ordinary mode keeps std Box allocation/OOM semantics. Request mode reserves
/// the real paid SceneBox before evaluating the factory; prebuilt owned values
/// enter through boxed_owned so their policy is checked before admission.
pub(crate) fn boxed<T: PolicyValue>(
    processing: &ProcessingContext,
    cost: SceneCost,
    factory: impl FnOnce() -> T,
) -> Result<SceneBox<T>, Error> {
    SceneBox::<T>::preflight(processing)?;
    admit(processing, cost)?;
    SceneBox::try_new_with(processing, || Ok(factory()))
}

/// Transfer an already constructed value only after validating its actual
/// owners, before either structural admission or the enclosing box reserve.
pub(crate) fn boxed_owned<T: PolicyValue>(
    p: &ProcessingContext,
    cost: SceneCost,
    value: T,
) -> Result<SceneBox<T>, Error> {
    SceneBox::<T>::ensure_nonzero(p)?;
    value.validate_policy(p)?;
    SceneBox::<T>::preflight(p)?;
    admit(p, cost)?;
    SceneBox::try_new_with(p, || Ok(value))
}
/// A real root with fallible field construction after its structural gate.
pub(crate) fn try_root(
    p: &ProcessingContext,
    make: impl FnOnce() -> Result<Group, Error>,
) -> Result<Group, Error> {
    admit(p, SceneCost::NODE)?;
    let value = make().map_err(|e| p.record_resource(e))?;
    value.validate_policy(p)?;
    finish(p, value)
}
/// Admit a real owned root with no enclosing Node, or a new wrapper root.
/// An unpromoted stack Group collecting already admitted children uses the
/// original Group::empty instead; it is not a separately materialized root.
pub(crate) fn root(
    processing: &ProcessingContext,
    factory: impl FnOnce() -> Group,
) -> Result<Group, Error> {
    try_root(processing, || Ok(factory()))
}

/// Box an already admitted root without creating a second scene identity.
///
/// Private call-site provenance is required: each caller must identify its
/// successful root admission or same-request parsed Tree whose root is moved.
/// Group has no runtime admission identity. This must never promote an arbitrary
/// unadmitted collecting Group; that path uses boxed(..., SceneCost::NODE, ...).
pub(crate) fn moved_root(
    processing: &ProcessingContext,
    value: Group,
) -> Result<SceneBox<Group>, Error> {
    SceneBox::<Group>::preflight(processing)?;
    value.validate_policy(processing)?;
    SceneBox::try_new_with(processing, || Ok(value))
}

/// The actual owned scene edges copied by Clone, excluding shared Arc graphs.
/// This is a private structural cost protocol, not a memory/Clone-work bound.
pub(crate) trait SceneOwned: PolicyValue {
    fn scene_cost(&self, processing: &ProcessingContext) -> Result<SceneCost, Error>;
}

/// Scan then admit the complete copied owned graph exactly once before Clone.
/// Ordinary copies preserve the original fields without scene limits. Request
/// copies validate all owners, admit the graph, and allocate through its request.
/// Temporary retirement precedes the same first-fault publication tail.
pub(crate) fn clone_owned<T: SceneOwned + TryCopy>(
    processing: &ProcessingContext,
    value: &T,
) -> Result<T, Error> {
    value.validate_policy(processing)?;
    {
        #[cfg(test)]
        let _phase = tests::ScanPhase::enter();
        check(processing)?;
    }
    if processing.resources().is_some() {
        let cost = value.scene_cost(processing)?;
        admit(processing, cost)?;
    }
    value
        .try_copy_in(processing)
        .and_then(|value| finish(processing, value))
}

/// Clone and box one already fully admitted copied graph, without charging its
/// outer identity again. Callers must not wrap the result in boxed(NODE).
pub(crate) fn clone_boxed<T: SceneOwned + TryCopy>(
    processing: &ProcessingContext,
    value: &T,
) -> Result<SceneBox<T>, Error> {
    SceneBox::<T>::ensure_nonzero(processing)?;
    value.validate_policy(processing)?;
    SceneBox::<T>::preflight(processing)?;
    // Validate/admit the whole graph before allocating the outer box.
    if processing.resources().is_some() {
        admit(processing, value.scene_cost(processing)?)?;
    }
    SceneBox::try_new_with(processing, || value.try_copy_in(processing))
}

struct Scan<'a> {
    processing: &'a ProcessingContext,
    cost: SceneCost,
}

impl<'a> Scan<'a> {
    fn new(processing: &'a ProcessingContext) -> Self {
        Self {
            processing,
            cost: SceneCost::default(),
        }
    }

    fn quota(&self, limit: LimitKind) -> Error {
        self.processing
            .record_resource(Error::new(FailureKind::Quota, limit))
    }

    fn identity(&mut self, depth: usize, cost: SceneCost) -> Result<(), Error> {
        if depth > MAX_OWNED_DEPTH {
            return Err(self.quota(LimitKind::XmlDepth));
        }
        if let Some(resources) = self.processing.resources() {
            resources
                .checkpoint(1)
                .map_err(|error| self.processing.record_resource(error))?;
        }
        let nodes = self
            .cost
            .nodes
            .checked_add(cost.nodes)
            .ok_or_else(|| self.quota(LimitKind::ExpandedNodes))?;
        let segments = self
            .cost
            .segments
            .checked_add(cost.segments)
            .ok_or_else(|| self.quota(LimitKind::ConvertedSegments))?;
        self.cost = SceneCost { nodes, segments };
        Ok(())
    }

    fn group(&mut self, value: &Group, depth: usize) -> Result<(), Error> {
        self.identity(depth, SceneCost::NODE)?;
        for child in value.children() {
            self.node(child, depth + 1)?;
        }
        Ok(())
    }

    fn path(&mut self, value: &Path, depth: usize) -> Result<(), Error> {
        self.identity(depth, SceneCost::path(value))
    }

    fn image(&mut self, value: &Image, depth: usize) -> Result<(), Error> {
        self.identity(depth, SceneCost::NODE)?;
        self.image_kind(value.kind(), depth + 1)
    }

    fn image_kind(&mut self, value: &ImageKind, depth: usize) -> Result<(), Error> {
        match value {
            ImageKind::SVG(tree) => self.group(tree.root(), depth),
            ImageKind::JPEG(_) | ImageKind::PNG(_) | ImageKind::GIF(_) | ImageKind::WEBP(_) => {
                Ok(())
            }
        }
    }

    fn text(&mut self, value: &Text, depth: usize) -> Result<(), Error> {
        self.identity(depth, SceneCost::NODE)?;
        self.group(value.flattened(), depth + 1)
    }

    fn node(&mut self, value: &Node, depth: usize) -> Result<(), Error> {
        // Node/Box/Group are one scene level, not three implementation frames.
        match value {
            Node::Group(value) => self.group(value, depth),
            Node::Path(value) => self.path(value, depth),
            Node::Image(value) => self.image(value, depth),
            Node::Text(value) => self.text(value, depth),
        }
    }
}

macro_rules! owned {
    ($ty:ty, $method:ident) => {
        impl SceneOwned for $ty {
            fn scene_cost(&self, processing: &ProcessingContext) -> Result<SceneCost, Error> {
                self.validate_policy(processing)?;
                #[cfg(test)]
                let _phase = tests::ScanPhase::enter();
                let mut scan = Scan::new(processing);
                scan.$method(self, 1)?;
                Ok(scan.cost)
            }
        }
    };
}

owned!(Node, node);
owned!(Group, group);
owned!(Path, path);
owned!(Image, image);
owned!(ImageKind, image_kind);
owned!(Text, text);

impl SceneOwned for Tree {
    fn scene_cost(&self, processing: &ProcessingContext) -> Result<SceneCost, Error> {
        self.root().scene_cost(processing)
    }
}

macro_rules! rooted {
    ($t:ty) => {
        impl SceneOwned for $t {
            fn scene_cost(&self, p: &ProcessingContext) -> Result<SceneCost, Error> {
                self.validate_policy(p)?;
                self.root().scene_cost(p)
            }
        }
    };
}
rooted!(crate::Pattern);
rooted!(crate::ClipPath);
rooted!(crate::Mask);
impl<T: SceneOwned> SceneOwned for Option<T> {
    fn scene_cost(&self, processing: &ProcessingContext) -> Result<SceneCost, Error> {
        match self {
            Some(value) => value.scene_cost(processing),
            None => Ok(SceneCost::default()),
        }
    }
}

impl<T: SceneOwned> SceneOwned for SceneBox<T> {
    fn scene_cost(&self, processing: &ProcessingContext) -> Result<SceneCost, Error> {
        self.as_ref().scene_cost(processing)
    }
}

#[cfg(test)]
#[path = "scene_tests.rs"]
mod tests;

const _: () = {
    assert!(std::mem::size_of::<Group>() > 0);
    assert!(std::mem::size_of::<Path>() > 0);
    assert!(std::mem::size_of::<Image>() > 0);
    assert!(std::mem::size_of::<Text>() > 0);
};
