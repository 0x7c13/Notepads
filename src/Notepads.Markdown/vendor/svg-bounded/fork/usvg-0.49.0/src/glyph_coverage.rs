// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual outline members retained at the original flatten paint boundary.
//! No Native object or font owner is retained, and no coverage is rendered here.

use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, PaidShared, ResourceKind, ResourcesRef,
};

/// The selected outline representation genuinely consumed by flatten.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlyphOutlineKind {
    /// The ordinary Rust outline reader supplied the local path.
    Ordinary,
    /// The Native synthetic-outline batch supplied the local path and fill rule.
    Synthetic,
}

/// Exact layout inputs and outcomes of one attempted outline contribution.
/// Source context is not an HB run identifier; no range is promoted to whole-text coordinates.
#[derive(Clone, Copy, Debug)]
pub struct PlacedGlyphSource {
    /// Actual generational font database identity within this request.
    pub font: fontdb::ID,
    /// Actual glyph id.
    pub glyph: u16,
    /// Actual font units per em used by layout.
    pub units_per_em: u16,
    /// Final semantic em, including fallback scaling.
    pub font_size: f32,
    /// The selected ordinary or synthetic representation.
    pub representation: GlyphOutlineKind,
    /// The selected local outline was Some, before transformation.
    pub local_outline_present: bool,
    /// The transformed outline really contributed to the associated path builder.
    pub contributed_to_path: bool,
    /// Original source span carried by the actual positioned glyph.
    pub source_span: Option<usize>,
    /// Unchanged local layout source range; not a whole semantic Text UTF8 range.
    pub layout_source_range: [usize; 2],
    /// Existing checked source membership, without inventing a new run identity.
    pub source_identity: Option<crate::ReplayGlyphIdentity>,
    /// Origin convention actually applied to the selected local geometry.
    pub geometry_origin: crate::ReplayGeometryOrigin,
    /// Final layout placement convention.
    pub placement_policy: crate::ReplayPlacementPolicy,
    /// The actual glyph-local matrix, without a reconstructed baseline.
    pub glyph_transform: crate::Transform,
    /// The actual cluster matrix.
    pub cluster_transform: crate::Transform,
    /// The actual span matrix.
    pub span_transform: crate::Transform,
    /// Exact local outline to Text transform consumed by flatten, including its Y flip.
    pub outline_transform: crate::Transform,
}

#[derive(Debug)]
struct GlyphFillBatch {
    epoch: u64,
    glyphs: BudgetVec<PlacedGlyphSource>,
}

/// Immutable same-request members of one actual outline Path fill batch.
/// Cloning shares its admitted storage and cannot extend a Native font session.
#[derive(Debug)]
pub struct GlyphFillSource {
    batch: PaidShared<GlyphFillBatch>,
}

impl GlyphFillSource {
    pub(crate) fn try_new(
        epoch: u64,
        resources: &ResourcesRef,
        glyphs: BudgetVec<PlacedGlyphSource>,
    ) -> Result<Self, Error> {
        if epoch == 0 || glyphs.is_empty() || !glyphs.belongs_to(resources) {
            return Err(resources.record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        Ok(Self {
            batch: PaidShared::try_new(
                resources,
                ResourceKind::Geometry,
                GlyphFillBatch { epoch, glyphs },
            )?,
        })
    }

    /// Original opaque, nonzero font session ticket. This is not a pointer or lifetime lease.
    pub fn epoch(&self) -> u64 {
        self.batch.epoch
    }

    /// Exact attempted member order, including explicit noncontributing/empty outlines.
    pub fn glyphs(&self) -> &[PlacedGlyphSource] {
        self.batch.glyphs.as_slice()
    }

    /// Check the real resource owner separately from the numeric session ticket.
    pub fn belongs_to(&self, resources: &ResourcesRef) -> bool {
        self.batch.glyphs.belongs_to(resources)
    }

    /// Actual retained record capacity; excludes separately charged shared metadata.
    pub fn retained_record_bytes(&self) -> usize {
        self.batch.glyphs.capacity() * std::mem::size_of::<PlacedGlyphSource>()
    }
}

impl crate::tree::owner::PolicyValue for GlyphFillSource {
    fn validate_policy(&self, p: &crate::ProcessingContext) -> Result<(), Error> {
        crate::tree::owner::validate_owner(p, Some(self.batch.resources()))?;
        crate::tree::owner::validate_owner(p, Some(self.batch.glyphs.resources()))
    }
}
impl crate::tree::owner::TryCopy for GlyphFillSource {
    fn try_copy_in(&self, p: &crate::ProcessingContext) -> Result<Self, Error> {
        crate::tree::owner::PolicyValue::validate_policy(self, p)?;
        crate::tree::owner::finish(
            p,
            Ok(Self {
                batch: self.batch.clone(),
            }),
        )
    }
}
