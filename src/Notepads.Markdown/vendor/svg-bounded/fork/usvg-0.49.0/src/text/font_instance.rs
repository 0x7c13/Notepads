// Copyright 2022 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

//! One lexical font/axes/location binding for the actual metrics and draw sites.
//! Request owners have no ordinary owned conversion or Clone implementation.

use markdown_svg_budget::{BudgetVec, Error, RequestControl, ResourceKind, ResourcesRef};
use skrifa::instance::{Location, LocationRef, Size};
use skrifa::outline::{AdjustedMetrics, DrawError, DrawSettings, OutlinePen};
use skrifa::raw::{control::TryTableProvider, request::CheckedFontRef, TableProvider};
use skrifa::{FontRef, MetadataProvider, Tag};

enum Axes<'f, 'r> {
    Ordinary(skrifa::AxisCollection<'f>),
    Request(skrifa::request::Axes<'f, 'r>),
}

/// Keep the actual Binary source alive across a representation callback and
/// inspect its natural retirement before the next representation can start.
pub(super) fn with_source<T>(
    database: &fontdb::Database,
    id: fontdb::ID,
    processing: &crate::ProcessingContext,
    binary_only: bool,
    mut render: impl FnMut(&[u8], u32) -> Result<Option<T>, Error>,
) -> Result<Option<T>, Error> {
    use crate::tree::owner;
    owner::check(processing, 0)?;
    let result = (|| {
        let Some((source, face_index)) = database.face_source(id) else {
            return Ok(None);
        };
        let result = if let Some(bytes) = source.binary_data() {
            render(bytes.as_ref().as_ref(), face_index)
        } else if binary_only || processing.resources().is_some() {
            Err(processing.record_resource(Error::new(
                crate::ProcessingFailure::FontReplay.into(),
                markdown_svg_budget::LimitKind::None,
            )))
        } else {
            database
                .with_face_data(id, |bytes, index| render(bytes, index))
                .unwrap_or(Ok(None))
        }
        .map_err(|error| processing.record_resource(error));
        drop(source);
        owner::check(processing, 0)?;
        result
    })()
    .map_err(|error| processing.record_resource(error));
    drop(render);
    owner::finish(processing, result)
}

/// This view borrows only the source bytes retained by with_source. Default
/// bitmap and SVG probes do not eagerly construct axes or locations.
pub(super) fn read_font<'f>(
    bytes: &'f [u8],
    index: u32,
    processing: &crate::ProcessingContext,
) -> Result<Option<FontRef<'f>>, Error> {
    Ok(match processing.resources() {
        Some(resources) => {
            FontRef::try_from_index(bytes, index, &mut RequestControl(resources))?.ok()
        }
        None => FontRef::from_index(bytes, index).ok(),
    })
}

pub(super) fn read_table<
    'f,
    T: skrifa::raw::TopLevelTable + skrifa::raw::control::TryFontRead<'f>,
>(
    font: &FontRef<'f>,
    resources: &ResourcesRef,
) -> Result<Result<T, skrifa::raw::ReadError>, Error> {
    let mut cx = RequestControl(resources);
    let Some(data) = font.try_table_data(T::TAG, &mut cx)? else {
        return Ok(Err(skrifa::raw::ReadError::TableIsMissing(T::TAG)));
    };
    let table = T::try_read(data, &mut cx)?;
    resources.checkpoint(0)?;
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry_test_support::{cancelled, request, Call};
    use markdown_svg_budget::Limits;

    #[derive(Default)]
    struct Pen(Vec<(u8, Vec<u32>)>);
    impl OutlinePen for Pen {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push((0, vec![x.to_bits(), y.to_bits()]));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push((1, vec![x.to_bits(), y.to_bits()]));
        }
        fn quad_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
            self.0
                .push((2, [x0, y0, x1, y1].map(f32::to_bits).to_vec()));
        }
        fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
            self.0
                .push((3, [x0, y0, x1, y1, x2, y2].map(f32::to_bits).to_vec()));
        }
        fn close(&mut self) {
            self.0.push((4, Vec::new()));
        }
    }

    fn fonts() -> [&'static [u8]; 3] {
        [
            include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf"),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/vazirmatn_var_trimmed.ttf"
            )),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/charstring_path_ops.ttf"
            )),
        ]
    }

    #[test]
    fn real_glyf_variable_and_cff_instances_keep_metrics_axes_and_every_path_bit() {
        for bytes in fonts() {
            let variations = [crate::FontVariation::new(*b"wght", 700.0)];
            let plain = FontInstance::new(bytes, 0, &variations, None)
                .unwrap()
                .unwrap();
            let (resources, state) = request(Limits::default());
            {
                let checked = FontInstance::new(bytes, 0, &variations, Some(&resources))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    format!("{:?}", plain.metrics().unwrap()),
                    format!("{:?}", checked.metrics().unwrap())
                );
                assert_eq!(
                    plain.units_per_em().unwrap(),
                    checked.units_per_em().unwrap()
                );
                assert_eq!(plain.location().coords(), checked.location().coords());
                assert_eq!(
                    plain.script_offsets(3, 5).unwrap(),
                    checked.script_offsets(3, 5).unwrap()
                );
                assert_eq!(plain.axes().len(), checked.axes().len());
                for index in 0..plain.axes().len() {
                    let a = plain.axes().try_get(index).unwrap().unwrap();
                    let b = checked.axes().try_get(index).unwrap().unwrap();
                    assert_eq!(a.tag(), b.tag());
                    assert_eq!(
                        [a.min_value(), a.default_value(), a.max_value()].map(f32::to_bits),
                        [b.min_value(), b.default_value(), b.max_value()].map(f32::to_bits)
                    );
                }
                // Use every available glyph, including successful empty paths.
                let count = plain.font.maxp().unwrap().num_glyphs();
                for id in 0..u32::from(count) {
                    // This test compares complete output, not checkpoint logs.
                    // Bound its mock event storage per glyph while keeping all
                    // counters, tickets and the same cumulative request limits.
                    state.lock().events.clear();
                    let mut a = Pen::default();
                    let mut b = Pen::default();
                    let left = plain.draw(skrifa::GlyphId::new(id), &mut a).unwrap();
                    let right = checked.draw(skrifa::GlyphId::new(id), &mut b).unwrap();
                    assert_eq!(format!("{left:?}"), format!("{right:?}"), "glyph {id}");
                    assert_eq!(a.0, b.0, "glyph {id}");
                }
            }
            resources.checkpoint(0).unwrap();
            assert_eq!(state.live_len(), 0);
            resources.validate_retired().unwrap();
        }
    }

    #[test]
    fn cancellation_never_becomes_missing_font_and_draw_stops_before_next_pen_call() {
        let bytes = fonts()[0];
        let (resources, state) = request(Limits::default());
        state.arm(Call::Checkpoint, 2, cancelled());
        assert!(
            matches!(FontInstance::new(bytes, 0, &[], Some(&resources)), Err(e) if e == cancelled())
        );
        assert_eq!(state.live_len(), 0);

        let (resources, state) = request(Limits::default());
        {
            let font = FontInstance::new(bytes, 0, &[], Some(&resources))
                .unwrap()
                .unwrap();
            let mut pen = Pen::default();
            state.arm(Call::Checkpoint, 1, cancelled());
            assert!(
                matches!(font.draw(skrifa::GlyphId::new(1), &mut pen), Err(e) if e == cancelled())
            );
            assert!(pen.0.is_empty());
            assert_eq!(resources.failure(), Some(cancelled()));
        }
        assert_eq!(state.live_len(), 0);
        assert_eq!(resources.statistics().scratch_bytes, 0);
    }
}
enum Coordinates {
    Ordinary(Location),
    Request(skrifa::request::Location),
}

pub(super) struct FontInstance<'f, 'r> {
    font: FontRef<'f>,
    checked: Option<CheckedFontRef<'f, 'r>>,
    axes: Axes<'f, 'r>,
    location: Coordinates,
    resources: Option<&'r ResourcesRef>,
}

impl<'f, 'r> FontInstance<'f, 'r> {
    pub(super) fn new(
        bytes: &'f [u8],
        index: u32,
        variations: &[crate::FontVariation],
        resources: Option<&'r ResourcesRef>,
    ) -> Result<Option<Self>, Error> {
        let (font, checked, axes, location) = match resources {
            Some(resources) => {
                resources.checkpoint(0)?;
                let Ok(font) =
                    FontRef::try_from_index(bytes, index, &mut RequestControl(resources))?
                else {
                    return Ok(None);
                };
                let Ok(checked) = CheckedFontRef::from_index(bytes, index, resources)? else {
                    return Ok(None);
                };
                let axes = skrifa::request::Axes::try_new(&font, resources)?;
                let mut settings = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
                settings.try_reserve_exact(variations.len())?;
                for variation in variations {
                    resources.checkpoint(1)?;
                    settings.try_push(skrifa::setting::VariationSetting::new(
                        Tag::from_be_bytes(variation.tag),
                        variation.value,
                    ))?;
                }
                let location = axes.try_location(&settings)?;
                drop(settings);
                resources.checkpoint(0)?;
                (
                    font,
                    Some(checked),
                    Axes::Request(axes),
                    Coordinates::Request(location),
                )
            }
            None => {
                let Ok(font) = FontRef::from_index(bytes, index) else {
                    return Ok(None);
                };
                let axes = font.axes();
                let location = axes.location(
                    variations
                        .iter()
                        .map(|v| (Tag::from_be_bytes(v.tag), v.value)),
                );
                (
                    font,
                    None,
                    Axes::Ordinary(axes),
                    Coordinates::Ordinary(location),
                )
            }
        };
        Ok(Some(Self {
            font,
            checked,
            axes,
            location,
            resources,
        }))
    }

    pub(super) fn location(&self) -> LocationRef<'_> {
        match &self.location {
            Coordinates::Ordinary(location) => location.into(),
            Coordinates::Request(location) => location.as_ref(),
        }
    }

    pub(super) fn validated_location(&self) -> Result<LocationRef<'_>, Error> {
        if let (Coordinates::Request(location), Some(resources)) = (&self.location, self.resources)
        {
            location.validate_resources(resources)?;
        }
        Ok(self.location())
    }

    pub(super) fn font(&self) -> &FontRef<'f> {
        &self.font
    }

    pub(super) fn axes(&self) -> crate::ReplayAxes<'_> {
        match &self.axes {
            Axes::Ordinary(axes) => crate::ReplayAxes::Ordinary(axes.clone()),
            Axes::Request(axes) => crate::ReplayAxes::Request(axes),
        }
    }

    pub(super) fn metrics(&self) -> Result<skrifa::metrics::Metrics, Error> {
        match self.resources {
            Some(resources) => skrifa::request::try_metrics(
                &self.font,
                Size::unscaled(),
                self.location(),
                resources,
            ),
            None => Ok(self.font.metrics(Size::unscaled(), self.location())),
        }
    }

    pub(super) fn units_per_em(&self) -> Result<Option<u16>, Error> {
        Ok(match (&self.checked, self.resources) {
            (Some(font), Some(resources)) => font
                .try_head(&mut RequestControl(resources))?
                .ok()
                .map(|head| head.units_per_em()),
            _ => self.font.head().ok().map(|head| head.units_per_em()),
        })
    }

    pub(super) fn script_offsets(&self, mut sub: i16, mut sup: i16) -> Result<(i16, i16), Error> {
        use skrifa::raw::tables::mvar::tags::{SBYO, SPYO};
        let location = self.location();
        let coords = location.coords();
        match (&self.checked, self.resources) {
            (Some(font), Some(resources)) => {
                let mut cx = RequestControl(resources);
                if let Ok(os2) = font.try_os2(&mut cx)? {
                    sub = os2.y_subscript_y_offset();
                    sup = os2.y_superscript_y_offset();
                }
                if let (Ok(mvar), true) = (font.try_mvar(&mut cx)?, !coords.is_empty()) {
                    sub += mvar
                        .try_metric_delta(SBYO, coords, &mut cx)?
                        .unwrap_or_default()
                        .to_f32()
                        .round() as i16;
                    sup += mvar
                        .try_metric_delta(SPYO, coords, &mut cx)?
                        .unwrap_or_default()
                        .to_f32()
                        .round() as i16;
                }
                resources.checkpoint(0)?;
            }
            _ => {
                if let Ok(os2) = self.font.os2() {
                    sub = os2.y_subscript_y_offset();
                    sup = os2.y_superscript_y_offset();
                }
                if let (Ok(mvar), true) = (self.font.mvar(), !coords.is_empty()) {
                    sub += mvar
                        .metric_delta(SBYO, coords)
                        .unwrap_or_default()
                        .to_f32()
                        .round() as i16;
                    sup += mvar
                        .metric_delta(SPYO, coords)
                        .unwrap_or_default()
                        .to_f32()
                        .round() as i16;
                }
            }
        }
        Ok((sub, sup))
    }

    pub(super) fn draw(
        &self,
        glyph: skrifa::GlyphId,
        pen: &mut impl OutlinePen,
    ) -> Result<Option<Result<AdjustedMetrics, DrawError>>, Error> {
        let settings = DrawSettings::unhinted(Size::unscaled(), self.location());
        match self.resources {
            Some(resources) => {
                let outlines = skrifa::request::Outlines::try_new(&self.font, resources)?;
                let result = match outlines.try_get(glyph)? {
                    Some(outline) => Some(outline.draw(settings, pen)?),
                    None => None,
                };
                drop(outlines);
                resources.checkpoint(0)?;
                Ok(result)
            }
            None => Ok(self
                .font
                .outline_glyphs()
                .get(glyph)
                .map(|outline| outline.draw(settings, pen))),
        }
    }
}
