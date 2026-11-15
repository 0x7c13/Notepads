//! SVG coordinate mappings used to validate readable label geometry.

use super::context::attribute;
use crate::svg::pipeline::checkpoint_loop;
use svgtypes::{
    Align, AspectRatio, Length, LengthUnit, TransformListParser, TransformListToken, ViewBox,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Affine {
    pub(super) a: f64,
    pub(super) b: f64,
    pub(super) c: f64,
    pub(super) d: f64,
    pub(super) e: f64,
    pub(super) f: f64,
}

type ViewportMapping = (Affine, Option<(f64, f64)>);

impl Default for Affine {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
}

impl Affine {
    pub(super) fn then_local(self, local: Self) -> Option<Self> {
        let result = Self {
            a: self.a * local.a + self.c * local.b,
            b: self.b * local.a + self.d * local.b,
            c: self.a * local.c + self.c * local.d,
            d: self.b * local.c + self.d * local.d,
            e: self.a * local.e + self.c * local.f + self.e,
            f: self.b * local.e + self.d * local.f + self.f,
        };
        result.is_finite().then_some(result)
    }

    fn is_finite(self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .into_iter()
            .all(f64::is_finite)
    }

    pub(super) fn maps_finite_rect(self, x: f64, y: f64, width: f64, height: f64) -> bool {
        [
            (x, y),
            (x + width, y),
            (x, y + height),
            (x + width, y + height),
        ]
        .into_iter()
        .all(|(x, y)| {
            (self.a * x + self.c * y + self.e).is_finite()
                && (self.b * x + self.d * y + self.f).is_finite()
        })
    }

    pub(super) fn from_tag<E>(
        tag: &str,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<Option<Self>, E> {
        let Some(transform) = attribute(tag, "transform", checkpoint)? else {
            return Ok(Some(Self::default()));
        };
        let mut cumulative = Self::default();
        for (iteration, token) in TransformListParser::from(transform.as_ref()).enumerate() {
            checkpoint_loop(iteration, checkpoint)?;
            let local = match token {
                Ok(TransformListToken::Matrix { a, b, c, d, e, f }) => Self { a, b, c, d, e, f },
                Ok(TransformListToken::Translate { tx, ty }) => Self {
                    e: tx,
                    f: ty,
                    ..Default::default()
                },
                Ok(TransformListToken::Scale { sx, sy }) => Self {
                    a: sx,
                    d: sy,
                    ..Default::default()
                },
                Ok(TransformListToken::Rotate { angle }) => {
                    let (sin, cos) = angle.to_radians().sin_cos();
                    Self {
                        a: cos,
                        b: sin,
                        c: -sin,
                        d: cos,
                        ..Default::default()
                    }
                }
                Ok(TransformListToken::SkewX { angle }) => Self {
                    c: angle.to_radians().tan(),
                    ..Default::default()
                },
                Ok(TransformListToken::SkewY { angle }) => Self {
                    b: angle.to_radians().tan(),
                    ..Default::default()
                },
                // An invalid transform attribute is ignored as a whole in SVG.
                // Never retain a parsed prefix or manufacture partial coordinates.
                Err(_) => return Ok(Some(Self::default())),
            };
            let Some(next) = cumulative.then_local(local) else {
                return Ok(None);
            };
            cumulative = next;
        }
        checkpoint()?;
        Ok(Some(cumulative))
    }
}

fn length(raw: &str, reference: Option<f64>) -> Option<f64> {
    let value: Length = raw.parse().ok()?;
    let value = match value.unit {
        LengthUnit::None | LengthUnit::Px => value.number,
        LengthUnit::Percent => value.number * reference? / 100.0,
        LengthUnit::In => value.number * 96.0,
        LengthUnit::Cm => value.number * 96.0 / 2.54,
        LengthUnit::Mm => value.number * 96.0 / 25.4,
        LengthUnit::Pt => value.number * 96.0 / 72.0,
        LengthUnit::Pc => value.number * 16.0,
        LengthUnit::Em | LengthUnit::Ex => return None,
    };
    value.is_finite().then_some(value)
}

fn view_box<E>(
    tag: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<ViewBox>, E> {
    Ok(attribute(tag, "viewBox", checkpoint)?
        .and_then(|raw| raw.parse::<ViewBox>().ok())
        .filter(|view_box| {
            [view_box.x, view_box.y, view_box.w, view_box.h]
                .into_iter()
                .all(f64::is_finite)
        }))
}

pub(super) fn root_viewport<E>(
    tag: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<(f64, f64)>, E> {
    if let Some(view_box) = view_box(tag, checkpoint)? {
        return Ok(Some((view_box.w, view_box.h)));
    }
    let width = attribute(tag, "width", checkpoint)?.and_then(|raw| length(raw.as_ref(), None));
    let height = attribute(tag, "height", checkpoint)?.and_then(|raw| length(raw.as_ref(), None));
    Ok(width.zip(height))
}

pub(super) fn nested_viewport<E>(
    tag: &str,
    parent: Option<(f64, f64)>,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<ViewportMapping>, E> {
    let x = attribute(tag, "x", checkpoint)?.map_or(Some(0.0), |raw| {
        length(raw.as_ref(), parent.map(|size| size.0))
    });
    let y = attribute(tag, "y", checkpoint)?.map_or(Some(0.0), |raw| {
        length(raw.as_ref(), parent.map(|size| size.1))
    });
    let width_attr = attribute(tag, "width", checkpoint)?;
    let height_attr = attribute(tag, "height", checkpoint)?;
    let width = width_attr.as_deref().map_or_else(
        || parent.map(|size| size.0),
        |raw| length(raw, parent.map(|size| size.0)),
    );
    let height = height_attr.as_deref().map_or_else(
        || parent.map(|size| size.1),
        |raw| length(raw, parent.map(|size| size.1)),
    );
    let Some((x, y)) = x.zip(y) else {
        return Ok(None);
    };
    if (width_attr.is_some() && width.is_none()) || (height_attr.is_some() && height.is_none()) {
        return Ok(None);
    }
    let translation = Affine {
        e: x,
        f: y,
        ..Default::default()
    };
    let view_box = view_box(tag, checkpoint)?;
    if attribute(tag, "viewBox", checkpoint)?.is_some() && view_box.is_none() {
        return Ok(None);
    }
    let Some(view_box) = view_box else {
        return Ok(Some((translation, width.zip(height))));
    };
    let Some((width, height)) = width
        .zip(height)
        .filter(|(width, height)| *width >= 0.0 && *height >= 0.0)
    else {
        return Ok(None);
    };
    let aspect = match attribute(tag, "preserveAspectRatio", checkpoint)? {
        Some(raw) => match raw.parse::<AspectRatio>() {
            Ok(value) => value,
            Err(_) => return Ok(None),
        },
        None => AspectRatio::default(),
    };
    Ok(view_box_mapping(view_box, width, height, x, y, aspect)
        .map(|mapping| (mapping, Some((view_box.w, view_box.h)))))
}

pub(super) fn root_mapping<E>(
    tag: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<Affine>, E> {
    let Some(view_box) = view_box(tag, checkpoint)? else {
        return Ok(Some(Affine::default()));
    };
    let width = attribute(tag, "width", checkpoint)?.and_then(|raw| length(raw.as_ref(), None));
    let height = attribute(tag, "height", checkpoint)?.and_then(|raw| length(raw.as_ref(), None));
    let Some((width, height)) = width.zip(height) else {
        return Ok(Some(Affine::default()));
    };
    let aspect = attribute(tag, "preserveAspectRatio", checkpoint)?
        .and_then(|raw| raw.parse::<AspectRatio>().ok())
        .unwrap_or_default();
    Ok(view_box_mapping(view_box, width, height, 0.0, 0.0, aspect))
}

fn view_box_mapping(
    view_box: ViewBox,
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    aspect: AspectRatio,
) -> Option<Affine> {
    let (mut sx, mut sy) = (width / view_box.w, height / view_box.h);
    if aspect.align != Align::None {
        let scale = if aspect.slice { sx.max(sy) } else { sx.min(sy) };
        sx = scale;
        sy = scale;
    }
    let align_x = match aspect.align {
        Align::XMidYMin | Align::XMidYMid | Align::XMidYMax => 0.5,
        Align::XMaxYMin | Align::XMaxYMid | Align::XMaxYMax => 1.0,
        _ => 0.0,
    };
    let align_y = match aspect.align {
        Align::XMinYMid | Align::XMidYMid | Align::XMaxYMid => 0.5,
        Align::XMinYMax | Align::XMidYMax | Align::XMaxYMax => 1.0,
        _ => 0.0,
    };
    let mapping = Affine {
        a: sx,
        d: sy,
        e: x - view_box.x * sx + (width - view_box.w * sx) * align_x,
        f: y - view_box.y * sy + (height - view_box.h * sy) * align_y,
        ..Default::default()
    };
    mapping.is_finite().then_some(mapping)
}

#[cfg(test)]
mod tests {
    use super::Affine;

    #[test]
    fn transform_list_tokens_keep_cancellation_checkpoints() {
        let tag = format!(r#"<g transform="{}"/>"#, "translate(1 2) ".repeat(1024));
        let mut calls = 0;
        let result = Affine::from_tag(&tag, &mut || {
            calls += 1;
            if calls >= 4 { Err("cancelled") } else { Ok(()) }
        });
        assert_eq!(result, Err("cancelled"));
        assert_eq!(calls, 4);
    }

    #[test]
    fn finite_matrices_cannot_overflow_the_transformed_label_box() {
        let matrix = Affine {
            a: 1e308,
            ..Default::default()
        };
        assert!(!matrix.maps_finite_rect(0.0, 0.0, 40.0, 20.0));
    }
}
