// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-bound font consumers. No Deref/Clone escape to ordinary owned APIs.

use crate::control::{
    storage::{PaidSmall, SmallBuffer},
    FontControl,
};
use crate::{
    outline::{AdjustedMetrics, DrawError, DrawSettings, OutlineGlyphFormat, OutlinePen},
    FontRef, GlyphId, OutlineGlyph, OutlineGlyphCollection,
};
use markdown_svg_budget::{Error, RequestControl, ResourcesRef};

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "request_metadata_tests.rs"]
mod metadata_tests;

#[path = "request_bitmap.rs"]
mod bitmap;
pub use bitmap::{BitmapDataRef, BitmapGlyph, BitmapMetadata, BitmapStrikes};

#[path = "request_color.rs"]
mod color;
pub use color::{ColorGlyph, ColorGlyphs, ColorPainter, ColorPalette, ColorPalettes};

/// The original Copy metrics, with both coordinate scanning and MVAR lookups
/// using the caller's policy. The coordinate borrow is independent of font life.
pub fn try_metrics(
    font: &FontRef<'_>,
    size: crate::instance::Size,
    location: crate::instance::LocationRef<'_>,
    resources: &ResourcesRef,
) -> Result<crate::metrics::Metrics, Error> {
    resources.checkpoint(0)?;
    let metrics =
        crate::metrics::Metrics::try_new(font, size, location, &mut RequestControl(resources))?;
    resources.checkpoint(0)?;
    Ok(metrics)
}

/// The font view, coordinate borrow, and resource policy have independent
/// lifetimes. Only the actually used bounds consumer is exposed here.
pub struct GlyphMetrics<'f, 'c, 'r> {
    data: crate::metrics::GlyphMetricsData<'f>,
    coords: &'c [crate::instance::NormalizedCoord],
    resources: &'r ResourcesRef,
}
impl<'f, 'c, 'r> GlyphMetrics<'f, 'c, 'r> {
    pub fn try_new(
        font: &FontRef<'f>,
        size: crate::instance::Size,
        location: crate::instance::LocationRef<'c>,
        resources: &'r ResourcesRef,
    ) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let (data, coords) = crate::metrics::GlyphMetricsData::try_new(
            font,
            size,
            location,
            &mut RequestControl(resources),
        )?;
        resources.checkpoint(0)?;
        Ok(Self {
            data,
            coords,
            resources,
        })
    }
    pub fn try_bounds(
        &self,
        glyph_id: GlyphId,
    ) -> Result<Option<crate::metrics::BoundingBox>, Error> {
        self.resources.checkpoint(0)?;
        let bounds =
            self.data
                .try_bounds(self.coords, glyph_id, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(bounds)
    }
    pub fn try_left_side_bearing(&self, glyph_id: GlyphId) -> Result<Option<f32>, Error> {
        self.resources.checkpoint(0)?;
        let bearing = self.data.try_left_side_bearing(
            self.coords,
            glyph_id,
            &mut RequestControl(self.resources),
        )?;
        self.resources.checkpoint(0)?;
        Ok(bearing)
    }
}

/// Checked axis views retain only the original font bytes and the caller's
/// request. Axis values are fixed records, with the original scalar methods.
pub struct Axes<'f, 'r> {
    inner: crate::AxisCollection<'f>,
    resources: &'r ResourcesRef,
}
impl<'f, 'r> Axes<'f, 'r> {
    pub fn try_new(font: &FontRef<'f>, resources: &'r ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let inner = crate::AxisCollection::try_new(font, &mut RequestControl(resources))?;
        resources.checkpoint(0)?;
        Ok(Self { inner, resources })
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn try_get(&self, index: usize) -> Result<Option<crate::Axis>, Error> {
        self.inner
            .try_get(index, &mut RequestControl(self.resources))
    }
    pub fn try_get_by_tag(&self, tag: crate::Tag) -> Result<Option<crate::Axis>, Error> {
        self.inner
            .try_get_by_tag(tag, &mut RequestControl(self.resources))
    }
    pub fn try_location(
        &self,
        settings: &[crate::setting::VariationSetting],
    ) -> Result<Location, Error> {
        self.resources.checkpoint(0)?;
        let mut cx = RequestControl(self.resources);
        let mut coords: PaidSmall<crate::instance::NormalizedCoord, 8> = cx.small()?;
        coords.try_resize_and_fill(self.inner.len(), crate::instance::NormalizedCoord::ZERO)?;
        self.inner
            .try_location_to_slice(settings.iter().copied(), &mut coords, &mut cx)?;
        self.resources.checkpoint(0)?;
        Ok(Location { coords })
    }
}

/// An original inline8 location; any spill stays paid until the last owner is
/// destroyed. Borrowing does not claim a compile-time font/FontKey brand:
/// the U/E holder must keep and validate its original font binding.
pub struct Location {
    coords: PaidSmall<crate::instance::NormalizedCoord, 8>,
}

/// The original SK subtable selection and symbol semantics, with every reader
/// and map using the retained policy. No ordinary Charmap is exposed.
pub struct Charmap<'f, 'r> {
    inner: crate::charmap::Charmap<'f>,
    resources: &'r ResourcesRef,
}
impl<'f, 'r> Charmap<'f, 'r> {
    pub fn try_new(font: &FontRef<'f>, resources: &'r ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let inner = crate::charmap::Charmap::try_new(font, &mut RequestControl(resources))?;
        resources.checkpoint(0)?;
        Ok(Self { inner, resources })
    }
    pub fn has_map(&self) -> bool {
        self.inner.has_map()
    }
    pub fn is_symbol(&self) -> bool {
        self.inner.is_symbol()
    }
    pub fn has_variant_map(&self) -> bool {
        self.inner.has_variant_map()
    }
    pub fn try_map(&self, ch: u32) -> Result<Option<GlyphId>, Error> {
        self.inner.try_map(ch, &mut RequestControl(self.resources))
    }
    pub fn try_map_variant(
        &self,
        ch: u32,
        selector: u32,
    ) -> Result<Option<crate::charmap::MapVariant>, Error> {
        self.inner
            .try_map_variant(ch, selector, &mut RequestControl(self.resources))
    }
}
impl Location {
    pub fn coords(&self) -> &[crate::instance::NormalizedCoord] {
        &self.coords
    }
    pub fn as_ref(&self) -> crate::instance::LocationRef<'_> {
        crate::instance::LocationRef::new(self.coords())
    }
    pub fn belongs_to(&self, resources: &ResourcesRef) -> bool {
        self.coords.resources().is_same_request(resources)
    }
    /// Validate both accounts and their original first failures before a caller
    /// borrows an unbranded LocationRef from this request-owned location.
    pub fn validate_resources(&self, resources: &ResourcesRef) -> Result<(), Error> {
        crate::control::validate_resources(self.coords.resources(), resources)
    }
}

/// A collection tied to the caller's original resource account. Missing font
/// tables retain a valid empty collection; only get reports a missing glyph.
pub struct Outlines<'f, 'r> {
    inner: OutlineGlyphCollection<'f>,
    resources: &'r ResourcesRef,
}
impl<'f, 'r> Outlines<'f, 'r> {
    pub fn try_new(font: &FontRef<'f>, resources: &'r ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let inner = OutlineGlyphCollection::try_new(font, &mut RequestControl(resources))?;
        resources.checkpoint(0)?;
        Ok(Self { inner, resources })
    }

    pub fn format(&self) -> Option<OutlineGlyphFormat> {
        self.inner.format()
    }

    pub fn try_get(&self, glyph_id: GlyphId) -> Result<Option<Glyph<'f, 'r>>, Error> {
        self.resources.checkpoint(0)?;
        let inner = self
            .inner
            .try_get(glyph_id, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(inner.map(|inner| Glyph {
            inner,
            resources: self.resources,
        }))
    }
}

/// A glyph view with its policy retained. Drawing runs the original shared
/// outline kernel; supported request settings are unhinted with no user buffer.
pub struct Glyph<'f, 'r> {
    inner: OutlineGlyph<'f>,
    resources: &'r ResourcesRef,
}
impl<'f> Glyph<'f, '_> {
    pub fn format(&self) -> OutlineGlyphFormat {
        self.inner.format()
    }
    pub fn glyph_id(&self) -> GlyphId {
        self.inner.glyph_id()
    }

    /// Outside Err is the original request failure. A malformed glyph remains
    /// an inside DrawError; it can never swallow a budget/cancellation failure.
    pub fn draw<'s>(
        &self,
        settings: impl Into<DrawSettings<'s>>,
        pen: &mut impl OutlinePen,
    ) -> Result<Result<AdjustedMetrics, DrawError>, Error>
    where
        'f: 's,
    {
        self.inner.try_draw_request(settings, pen, self.resources)
    }
}
