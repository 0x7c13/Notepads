// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Optional, same-request provenance at the existing outline paint boundary.
//! The records describe actual flatten inputs; they do not render coverage.

use markdown_svg_budget::{BudgetVec, ResourceKind, ResourcesRef};

use super::layout::PositionedGlyph;
use crate::{
    Cache, GlyphFillSource, GlyphOutlineKind, Path, PlacedGlyphSource, ProcessingContext,
    ProcessingFailure, Transform,
};

pub(super) struct Capture {
    enabled: Option<Enabled>,
}

struct Enabled {
    epoch: u64,
    owner: std::sync::Arc<dyn crate::FontReplay>,
    resources: ResourcesRef,
    records: BudgetVec<PlacedGlyphSource>,
    work_credit: usize,
}

impl Capture {
    pub(super) fn new(cache: &Cache) -> Option<Self> {
        if cache.processing.failed() {
            return None;
        }
        let Some(owner) = cache.font_replay.as_ref() else {
            return Some(Self { enabled: None });
        };
        let epoch = owner.coverage_epoch();
        if epoch == 0 {
            return Some(Self { enabled: None });
        }
        let Some(resources) = owner.request_resources() else {
            cache.processing.fail(ProcessingFailure::FontReplay);
            return None;
        };
        if !cache
            .processing
            .resources()
            .is_some_and(|current| current.is_same_request(&resources))
        {
            cache.processing.fail(ProcessingFailure::FontReplay);
            return None;
        }
        Some(Self {
            enabled: Some(Enabled {
                epoch,
                owner: owner.clone(),
                records: BudgetVec::new(resources.clone(), ResourceKind::Geometry),
                resources,
                work_credit: 0,
            }),
        })
    }

    pub(super) fn record(
        &mut self,
        glyph: &PositionedGlyph,
        representation: GlyphOutlineKind,
        local_outline_present: bool,
        contributed_to_path: bool,
        outline_transform: Transform,
        processing: &ProcessingContext,
    ) -> Option<()> {
        let Some(capture) = self.enabled.as_mut() else {
            return Some(());
        };
        if processing.failed() {
            return None;
        }
        #[cfg(test)]
        let _phase =
            tests::CapturePhaseGuard::enter(&capture.resources, tests::CapturePhase::Record);
        // Prepay before copying at most 256 members. This bounds cancellation
        // latency without a cross-language checkpoint for each actual glyph.
        if capture.work_credit == 0 {
            capture
                .resources
                .checkpoint(256)
                .map_err(|error| processing.record_resource(error))
                .ok()?;
            capture.work_credit = 256;
        }
        capture.work_credit -= 1;
        let Some(source) = glyph.coverage_source(
            representation,
            local_outline_present,
            contributed_to_path,
            outline_transform,
        ) else {
            processing.fail(ProcessingFailure::FontReplay);
            return None;
        };
        // Start a synthetic-led batch with one record; following same-paint
        // members may grow the paid vector. A separate nonempty EvenOdd path needs
        // only one record instead of the ordinary vector's four-record minimum.
        if representation == GlyphOutlineKind::Synthetic && capture.records.is_empty() {
            capture
                .records
                .try_reserve_exact(1)
                .map_err(|error| processing.record_resource(error))
                .ok()?;
        }
        capture
            .records
            .try_push(source)
            .map_err(|error| processing.record_resource(error))
            .ok()
    }

    /// Move only this real paint batch's records into its immutable paid owner.
    /// An empty/degenerate builder retains no source and creates no new Path.
    pub(super) fn bind(
        &mut self,
        path: Option<&mut Path>,
        processing: &ProcessingContext,
    ) -> Option<()> {
        let Some(capture) = self.enabled.as_mut() else {
            return Some(());
        };
        if processing.failed() {
            return None;
        }
        #[cfg(test)]
        let _phase = tests::CapturePhaseGuard::enter(&capture.resources, tests::CapturePhase::Bind);
        // Observe cancellation at every completed paint boundary, including a
        // short final prefix or a batch whose outlines were all empty.
        capture
            .resources
            .checkpoint(1)
            .map_err(|error| processing.record_resource(error))
            .ok()?;
        let records = std::mem::replace(
            &mut capture.records,
            BudgetVec::new(capture.resources.clone(), ResourceKind::Geometry),
        );
        if let Some(path) = path {
            let source = {
                #[cfg(test)]
                let _phase = tests::CapturePhaseGuard::enter(
                    &capture.resources,
                    tests::CapturePhase::Shared,
                );
                GlyphFillSource::try_new(capture.epoch, &capture.resources, records)
                    .map_err(|error| processing.record_resource(error))
                    .ok()?
            };
            path.glyph_fill_source = Some(source);
            let Some(source) = path.glyph_fill_source.as_ref() else {
                processing.fail(ProcessingFailure::FontReplay);
                return None;
            };
            if capture
                .owner
                .observe_glyph_fill_source(source, path)
                .is_err()
            {
                processing.fail(ProcessingFailure::FontReplay);
                return None;
            }
        } else {
            drop(records);
        }
        if processing.failed() {
            None
        } else {
            Some(())
        }
    }
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;
