// Copyright 2017 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

/*!
[resvg](https://github.com/linebender/resvg) is an SVG rendering library.
*/

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::identity_op)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::wrong_self_convention)]

pub use tiny_skia;
pub use usvg;

mod clip;
mod coverage;
pub use coverage::{
    CoverageBatchResult, CoverageConfig, CoverageIneligibleReason, CoverageProvider,
    CoverageRequest, CoverageRoute,
};
mod filter;
mod geom;
mod image;
mod mask;
mod path;
#[cfg(feature = "raster-images")]
mod png_decode;
mod raster;
#[cfg(feature = "raster-images")]
mod raster_decode;
mod render;

/// Renders a tree onto the pixmap.
///
/// `transform` will be used as a root transform.
/// Can be used to position SVG inside the `pixmap`.
///
/// The produced content is in the sRGB color space.
pub fn render(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Result<(), usvg::ProcessingFailure> {
    render_observed(tree, transform, pixmap)
        .map(|_| ())
        .map_err(|e| e.kind)
}

/// Fresh top-level render with fixed, copy-only diagnostic counts.
pub fn render_observed(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Result<(usize, usize, usize, usize), usvg::ProcessingError> {
    let processing = std::sync::Arc::new(usvg::ProcessingContext::default());
    render_in_context(tree, transform, pixmap, processing.clone())?;
    Ok(processing.counts())
}

// All nested SVG/feImage/group calls inherit this one active request owner.
pub(crate) fn render_in_context(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    processing: std::sync::Arc<usvg::ProcessingContext>,
) -> Result<(), usvg::ProcessingError> {
    render_in_context_with_coverage(tree, transform, pixmap, processing, None)
}

// Embedded SVGs inherit the root density freeze and provider, even on new surfaces.
pub(crate) fn render_in_context_with_coverage(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    processing: std::sync::Arc<usvg::ProcessingContext>,
    coverage: Option<&CoverageConfig<'_>>,
) -> Result<(), usvg::ProcessingError> {
    processing.checked()?;
    let max_bbox = max_filter_bbox(pixmap.width(), pixmap.height());
    let ctx = render::Context {
        max_bbox,
        processing,
        coverage,
    };
    render::render_nodes(tree.root(), &ctx, transform, pixmap);
    ctx.processing.checked()
}

/// Renders a node onto the pixmap.
///
/// `transform` will be used as a root transform.
/// Can be used to position SVG inside the `pixmap`.
///
/// The expected pixmap size can be retrieved from `usvg::Node::abs_layer_bounding_box()`.
///
/// Returns `None` when `node` has a zero size.
///
/// The produced content is in the sRGB color space.
pub fn render_node(
    node: &usvg::Node,
    mut transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Result<Option<()>, usvg::ProcessingError> {
    let Some(bbox) = node.abs_layer_bounding_box() else {
        return Ok(None);
    };

    let max_bbox = max_filter_bbox(pixmap.width(), pixmap.height());

    transform = transform.pre_translate(-bbox.x(), -bbox.y());

    let ctx = render::Context {
        max_bbox,
        processing: std::sync::Arc::new(usvg::ProcessingContext::default()),
        coverage: None,
    };
    render::render_node(node, &ctx, transform, pixmap);
    ctx.processing.checked()?;
    Ok(Some(()))
}

pub(crate) trait OptionLog {
    fn log_none<F: FnOnce()>(self, f: F) -> Self;
}

impl<T> OptionLog for Option<T> {
    #[inline]
    fn log_none<F: FnOnce()>(self, f: F) -> Self {
        self.or_else(|| {
            f();
            None
        })
    }
}

fn max_filter_bbox(width: u32, height: u32) -> tiny_skia::IntRect {
    tiny_skia::IntRect::from_xywh(
        i32::try_from(width).unwrap_or(i32::MAX).saturating_mul(-2),
        i32::try_from(height).unwrap_or(i32::MAX).saturating_mul(-2),
        width.saturating_mul(5),
        height.saturating_mul(5),
    )
    .unwrap_or_else(|| {
        tiny_skia::IntRect::from_ltrb(i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2)
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn max_filter_bbox_is_clamped() {
        let bbox = super::max_filter_bbox(u32::MAX, u32::MAX);
        assert_eq!(bbox.left(), i32::MIN / 2);
        assert_eq!(bbox.top(), i32::MIN / 2);
        assert_eq!(bbox.right(), i32::MAX / 2);
        assert_eq!(bbox.bottom(), i32::MAX / 2);
    }
}

/// Isolated font replay entry: preserve the parse request's existing first-fault
/// owner across rendering. Ordinary top-level APIs retain their prior behavior.
pub fn render_in_request(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Result<(), usvg::ProcessingError> {
    render_in_request_with_coverage(tree, transform, pixmap, None)
}

/// Render with one optional request-borrowed whole-fill coverage service.
/// `None` retains the original target-density path. Any failure invalidates the
/// whole request; the caller must not publish a partially written pixmap.
pub fn render_in_request_with_coverage(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    coverage: Option<CoverageConfig<'_>>,
) -> Result<(), usvg::ProcessingError> {
    render_in_context_with_coverage(
        tree,
        transform,
        pixmap,
        tree.processing_context().clone(),
        coverage.as_ref(),
    )
}
