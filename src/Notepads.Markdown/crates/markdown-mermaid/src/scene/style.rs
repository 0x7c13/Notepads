// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::canonical::{FontFamily, FontPlan};

#[derive(Clone, Copy, Debug)]
pub struct ResolvedStyle {
    pub paint: Paint,
    pub color: Color,
    pub font: FontPlan,
}
impl Default for ResolvedStyle {
    fn default() -> Self {
        Self {
            paint: Paint::default(),
            color: Color::BLACK,
            font: FontPlan::default(),
        }
    }
}

pub fn color(value: &str, scope: &MemoryScope) -> Result<Option<Color>, CanonicalError> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let (rgb, alpha) = crate::canonical::parse_color_value(value, scope).map_err(|error| {
        if error == CanonicalError::UnsupportedCss {
            CanonicalError::UnsupportedProperty
        } else {
            error
        }
    })?;
    Ok(Some(Color::rgba(rgb, alpha)))
}
pub fn apply(
    style: &mut ResolvedStyle,
    source: &str,
    scope: &MemoryScope,
) -> Result<(), CanonicalError> {
    scope.charge_work(source.len())?;
    for declaration in source.split(';') {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let (key, value) = declaration
            .split_once(':')
            .ok_or(CanonicalError::UnsupportedProperty)?;
        let key = key.trim();
        let value = value.trim();
        let number = |text: &str| -> Result<f64, CanonicalError> {
            let n = text
                .trim()
                .strip_suffix("px")
                .unwrap_or(text.trim())
                .parse::<f64>()
                .map_err(|_| CanonicalError::UnsupportedProperty)?;
            if !valid(n) || n < 0.0 {
                return Err(CanonicalError::InvalidGeometry);
            }
            Ok(n)
        };
        match key {
            "fill" => style.paint.fill = color(value, scope)?,
            "stroke" => style.paint.stroke = color(value, scope)?,
            "color" => {
                style.color = color(value, scope)?.ok_or(CanonicalError::UnsupportedProperty)?
            }
            "stroke-width" => {
                style.paint.stroke_width = number(value)?;
                if style.paint.stroke_width > 32.0 {
                    return Err(CanonicalError::Limited);
                }
            }
            "opacity" => {
                style.paint.opacity = number(value)?;
                if style.paint.opacity > 1.0 {
                    return Err(CanonicalError::InvalidGeometry);
                }
            }
            "stroke-dasharray" => {
                style.paint.dash_len = 0;
                if value != "none" {
                    for part in value
                        .split(|c: char| c.is_ascii_whitespace() || c == ',')
                        .filter(|x| !x.is_empty())
                    {
                        if style.paint.dash_len == 4 {
                            return Err(CanonicalError::UnsupportedProperty);
                        }
                        style.paint.dash[style.paint.dash_len] = number(part)?;
                        style.paint.dash_len += 1;
                    }
                }
            }
            "font-size" => {
                style.font.em_size = number(value)?;
                if !(4.0..=128.0).contains(&style.font.em_size) {
                    return Err(CanonicalError::Limited);
                }
            }
            "font-family" => {
                style.font.family =
                    FontFamily::parse(value).ok_or(CanonicalError::FontUnavailable)?
            }
            "font-weight" => {
                style.font.weight = match value {
                    "normal" => 400,
                    "bold" => 700,
                    _ => value
                        .parse::<u16>()
                        .ok()
                        .filter(|v| (1..=999).contains(v))
                        .ok_or(CanonicalError::UnsupportedProperty)?,
                }
            }
            "font-style" => {
                style.font.italic = match value {
                    "normal" => false,
                    "italic" | "oblique" => true,
                    _ => return Err(CanonicalError::UnsupportedProperty),
                }
            }
            _ => return Err(CanonicalError::UnsupportedProperty),
        }
    }
    Ok(())
}
