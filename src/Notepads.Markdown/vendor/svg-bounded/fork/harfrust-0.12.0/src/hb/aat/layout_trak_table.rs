// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::hb::storage::Control;
use markdown_svg_budget::{Error, RequestControl};
use read_fonts::control::TryResolveNullableOffset;
// libm used for f32::round() method
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use core_maths::CoreFloat as _;

use read_fonts::tables::trak::TrackTableEntry;
use read_fonts::types::{BigEndian, Fixed};
use read_fonts::FontData;

use crate::hb::{buffer::hb_buffer_t, face::Scale, hb_font_t, ot_shape_plan::hb_ot_shape_plan_t};

pub fn apply(
    _plan: &hb_ot_shape_plan_t,
    face: &hb_font_t,
    scale: Scale,
    point_size: Option<f32>,
    buffer: &mut hb_buffer_t,
) -> Option<()> {
    let trak = face.aat_tables.trak.as_ref()?;
    let mut ptem = point_size.unwrap_or(0.0);
    if ptem <= 0.0 {
        ptem = 12.0; // CoreText fallback
    }

    if !buffer.have_positions {
        buffer.clear_positions();
    }

    let advance_to_add = if buffer.direction.is_horizontal() {
        scale.scale_x_f(font_call!(
            buffer,
            trak.get_h_tracking(ptem, 0.0, &face.control)
        ))
    } else {
        scale.scale_y_f(font_call!(
            buffer,
            trak.get_v_tracking(ptem, 0.0, &face.control)
        ))
    };

    foreach_grapheme!(buffer, start, end, {
        if buffer.direction.is_horizontal() {
            buffer.pos[start].x_advance += advance_to_add;
        } else {
            buffer.pos[start].y_advance += advance_to_add;
        }
    });

    Some(())
}

trait TrakExt {
    fn get_h_tracking(&self, ptem: f32, track: f32, c: &Control) -> Result<f32, Error>;
    fn get_v_tracking(&self, ptem: f32, track: f32, c: &Control) -> Result<f32, Error>;
}
impl TrakExt for read_fonts::tables::trak::Trak<'_> {
    fn get_h_tracking(&self, ptem: f32, track: f32, c: &Control) -> Result<f32, Error> {
        c.step(1)?;
        let data = match &c.0 {
            Some(r) => self
                .horiz_offset()
                .try_resolve::<read_fonts::tables::trak::TrackData, _>(
                    self.offset_data(),
                    &mut RequestControl(r),
                )?,
            None => self.horiz(),
        };
        match data.transpose().ok().flatten() {
            Some(t) => t.get_tracking(self.offset_data(), ptem, track, c),
            None => Ok(0.0),
        }
    }
    fn get_v_tracking(&self, ptem: f32, track: f32, c: &Control) -> Result<f32, Error> {
        c.step(1)?;
        let data = match &c.0 {
            Some(r) => self
                .vert_offset()
                .try_resolve::<read_fonts::tables::trak::TrackData, _>(
                    self.offset_data(),
                    &mut RequestControl(r),
                )?,
            None => self.vert(),
        };
        match data.transpose().ok().flatten() {
            Some(t) => t.get_tracking(self.offset_data(), ptem, track, c),
            None => Ok(0.0),
        }
    }
}

trait TrackDataExt {
    fn get_tracking(
        &self,
        offset_data: FontData,
        ptem: f32,
        track: f32,
        control: &Control,
    ) -> Result<f32, Error>;
}

impl TrackDataExt for read_fonts::tables::trak::TrackData<'_> {
    fn get_tracking(
        &self,
        offset_data: FontData,
        ptem: f32,
        track: f32,
        control: &Control,
    ) -> Result<f32, Error> {
        // These readers only form fixed borrowed slices; the searches below
        // are the nonconstant work and retain their original float order.
        control.step(1)?;
        let Ok(sizes) = self.size_table(offset_data) else {
            return Ok(0.0);
        };

        if sizes.is_empty() {
            return Ok(0.0);
        }

        let tracks = self.track_table();
        if tracks.is_empty() {
            return Ok(0.0);
        }

        let get_value = |entry: &TrackTableEntry| {
            let Ok(values) = entry.per_size_values(offset_data, sizes.len() as u16) else {
                return Ok(0.0);
            };
            entry.get_value(ptem, sizes, values, control)
        };

        if tracks.len() == 1 {
            return match tracks.first() {
                Some(t) => get_value(t),
                None => Ok(0.0),
            };
        }

        let mut i = 0;
        let mut j = tracks.len() - 1;

        while i + 1 < tracks.len() && tracks.get(i + 1).map_or(0.0, |t| t.track().to_f32()) <= track
        {
            control.step(1)?;
            i += 1;
        }
        while j > 0 && tracks.get(j - 1).map_or(0.0, |t| t.track().to_f32()) >= track {
            control.step(1)?;
            j -= 1;
        }

        if i == j {
            return match tracks.get(i) {
                Some(t) => get_value(t),
                None => Ok(0.0),
            };
        }

        let t0 = tracks.get(i).map_or(0.0, |t| t.track().to_f32());
        let t1 = tracks.get(j).map_or(0.0, |t| t.track().to_f32());
        let interp = if (t1 - t0).abs() < f32::EPSILON {
            0.0
        } else {
            (track - t0) / (t1 - t0)
        };

        let a = match tracks.get(i) {
            Some(t) => get_value(t)?,
            None => 0.0,
        };
        let b = match tracks.get(j) {
            Some(t) => get_value(t)?,
            None => 0.0,
        };
        Ok(a + interp * (b - a))
    }
}

trait TrackEntryExt {
    fn get_value(
        &self,
        ptem: f32,
        sizes: &[BigEndian<Fixed>],
        values: &[BigEndian<i16>],
        control: &Control,
    ) -> Result<f32, Error>;
}

impl TrackEntryExt for TrackTableEntry {
    fn get_value(
        &self,
        ptem: f32,
        sizes: &[BigEndian<Fixed>],
        values: &[BigEndian<i16>],
        control: &Control,
    ) -> Result<f32, Error> {
        control.step(1)?;
        let n = sizes.len().min(values.len());
        if n == 0 {
            return Ok(0.0);
        }

        for i in 0..n {
            control.step(1)?;
            let size_pt = sizes.get(i).map_or(0.0, |f| f.get().to_f32());
            if size_pt >= ptem {
                if i == 0 {
                    return Ok(values.first().map(|v| v.get() as f32).unwrap_or_default());
                }

                let s0 = sizes.get(i - 1).map_or(0.0, |f| f.get().to_f32());
                let s1 = size_pt;
                let v0 = values
                    .get(i - 1)
                    .map(|v| v.get() as f32)
                    .unwrap_or_default();
                let v1 = values.get(i).map(|v| v.get() as f32).unwrap_or_default();

                if (s1 - s0).abs() < f32::EPSILON {
                    return Ok((v0 + v1) * 0.5);
                }

                let t = (ptem - s0) / (s1 - s0);
                return Ok(v0 + t * (v1 - v0));
            }
        }

        Ok(values
            .get(n - 1)
            .map(|v| v.get() as f32)
            .unwrap_or_default())
    }
}
