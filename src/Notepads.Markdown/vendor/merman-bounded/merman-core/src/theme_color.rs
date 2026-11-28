//! Source-backed color operations used by Mermaid themes and renderers.
//!
//! The behavior in this module mirrors Khroma 2.1.0, the color package pinned by Mermaid
//! 11.16.1. Parsing is intentionally implemented as a small structured parser rather than a
//! regular expression so every accepted component and separator has an explicit owner.

use ryu_js::Buffer;
use std::error::Error as StdError;
use std::fmt;

/// A Khroma-compatible color operation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorError {
    /// The input is not one of the color formats accepted by Khroma 2.1.0.
    UnsupportedFormat { input: String },
    /// A single adjustment attempted to modify RGB and HSL channels together.
    MixedColorSpaces,
}

impl fmt::Display for ColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat { input } => {
                write!(formatter, "Unsupported color format: \"{input}\"")
            }
            Self::MixedColorSpaces => {
                formatter.write_str("Cannot change both RGB and HSL channels at the same time")
            }
        }
    }
}

impl StdError for ColorError {}

/// The syntax family which produced a parsed color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSourceFormat {
    Hex,
    Rgb,
    Hsl,
    Keyword,
    ConstructedRgb,
}

/// A typed color channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChannel {
    Red,
    Green,
    Blue,
    Hue,
    Saturation,
    Lightness,
    Alpha,
}

/// RGBA evidence in Khroma's native units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbaChannels {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

/// HSLA evidence in Khroma's native units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HslaChannels {
    pub hue_degrees: f64,
    pub saturation_percent: f64,
    pub lightness_percent: f64,
    pub alpha: f64,
}

/// A typed equivalent of the channel object passed to `khroma.adjust`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ColorAdjustment {
    pub red: Option<f64>,
    pub green: Option<f64>,
    pub blue: Option<f64>,
    pub hue: Option<f64>,
    pub saturation: Option<f64>,
    pub lightness: Option<f64>,
    pub alpha: Option<f64>,
}

impl ColorAdjustment {
    pub const fn rgb(red: f64, green: f64, blue: f64) -> Self {
        Self {
            red: Some(red),
            green: Some(green),
            blue: Some(blue),
            hue: None,
            saturation: None,
            lightness: None,
            alpha: None,
        }
    }

    pub const fn hsl(hue: f64, saturation: f64, lightness: f64) -> Self {
        Self {
            red: None,
            green: None,
            blue: None,
            hue: Some(hue),
            saturation: Some(saturation),
            lightness: Some(lightness),
            alpha: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorSpace {
    Rgb,
    Hsl,
}

/// Parsed Khroma color state, including source and mutation evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColor {
    raw_input: Option<String>,
    preserved_color: Option<String>,
    source_format: ColorSourceFormat,
    source_space: ColorSpace,
    mutation_space: Option<ColorSpace>,
    rgba: RgbaChannels,
    hsla: HslaChannels,
    changed: bool,
}

impl ThemeColor {
    /// Parse a color accepted by Khroma 2.1.0.
    pub fn parse(input: &str) -> Result<Self, ColorError> {
        parse_hex(input)
            .or_else(|| parse_rgb(input))
            .or_else(|| parse_hsl(input))
            .or_else(|| parse_keyword(input))
            .ok_or_else(|| ColorError::UnsupportedFormat {
                input: input.to_string(),
            })
    }

    /// Original caller input. Constructed colors have no raw input.
    pub fn raw(&self) -> Option<&str> {
        self.raw_input.as_deref()
    }

    pub const fn source_format(&self) -> ColorSourceFormat {
        self.source_format
    }

    pub const fn rgba_channels(&self) -> RgbaChannels {
        self.rgba
    }

    pub const fn hsla_channels(&self) -> HslaChannels {
        self.hsla
    }

    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Read a channel rounded the same way as `khroma.channel`.
    pub fn channel(&self, channel: ColorChannel) -> f64 {
        round_1e10(self.channel_unrounded(channel))
    }

    /// Serialize according to Khroma's raw-preservation and changed-space rules.
    pub fn stringify(&self) -> String {
        if !self.changed
            && let Some(color) = &self.preserved_color
        {
            return color.clone();
        }

        let output_space = self.mutation_space.unwrap_or(self.source_space);
        if output_space == ColorSpace::Hsl {
            return stringify_hsl(self.hsla);
        }

        if self.rgba.alpha < 1.0
            || !is_integer(self.rgba.red)
            || !is_integer(self.rgba.green)
            || !is_integer(self.rgba.blue)
        {
            stringify_rgb(self.rgba)
        } else {
            stringify_hex(self.rgba)
        }
    }

    fn constructed_rgba(rgba: RgbaChannels) -> Self {
        let hsla = rgb_to_hsl(rgba);
        Self {
            raw_input: None,
            preserved_color: None,
            source_format: ColorSourceFormat::ConstructedRgb,
            source_space: ColorSpace::Rgb,
            mutation_space: None,
            rgba,
            hsla,
            changed: false,
        }
    }

    fn channel_unrounded(&self, channel: ColorChannel) -> f64 {
        match channel {
            ColorChannel::Red => self.rgba.red,
            ColorChannel::Green => self.rgba.green,
            ColorChannel::Blue => self.rgba.blue,
            ColorChannel::Hue => self.hsla.hue_degrees,
            ColorChannel::Saturation => self.hsla.saturation_percent,
            ColorChannel::Lightness => self.hsla.lightness_percent,
            ColorChannel::Alpha => self.rgba.alpha,
        }
    }

    fn set_channel(&mut self, channel: ColorChannel, value: f64) -> Result<(), ColorError> {
        let value = clamp_channel(channel, value);
        match channel {
            ColorChannel::Red | ColorChannel::Green | ColorChannel::Blue => {
                self.select_mutation_space(ColorSpace::Rgb)?;
                match channel {
                    ColorChannel::Red => self.rgba.red = value,
                    ColorChannel::Green => self.rgba.green = value,
                    ColorChannel::Blue => self.rgba.blue = value,
                    _ => unreachable!(),
                }
                self.hsla = rgb_to_hsl(self.rgba);
            }
            ColorChannel::Hue | ColorChannel::Saturation | ColorChannel::Lightness => {
                self.select_mutation_space(ColorSpace::Hsl)?;
                match channel {
                    ColorChannel::Hue => self.hsla.hue_degrees = value,
                    ColorChannel::Saturation => self.hsla.saturation_percent = value,
                    ColorChannel::Lightness => self.hsla.lightness_percent = value,
                    _ => unreachable!(),
                }
                self.rgba = hsl_to_rgb(self.hsla);
            }
            ColorChannel::Alpha => {
                self.rgba.alpha = value;
                self.hsla.alpha = value;
            }
        }
        self.changed = true;
        Ok(())
    }

    fn adjust_channel(&mut self, channel: ColorChannel, amount: f64) -> Result<(), ColorError> {
        let current = self.channel_unrounded(channel);
        let next = clamp_channel(channel, current + amount);
        if current != next {
            self.set_channel(channel, next)?;
        }
        Ok(())
    }

    fn select_mutation_space(&mut self, space: ColorSpace) -> Result<(), ColorError> {
        if let Some(selected) = self.mutation_space
            && selected != space
        {
            return Err(ColorError::MixedColorSpaces);
        }
        self.mutation_space = Some(space);
        Ok(())
    }
}

/// Return one rounded channel from a parsed color.
pub fn channel(input: &str, color_channel: ColorChannel) -> Result<f64, ColorError> {
    Ok(ThemeColor::parse(input)?.channel(color_channel))
}

/// Construct and stringify an RGBA color using Khroma's clamp and format selection rules.
pub fn rgba(red: f64, green: f64, blue: f64, alpha: f64) -> Result<String, ColorError> {
    let color = ThemeColor::constructed_rgba(RgbaChannels {
        red: clamp_channel(ColorChannel::Red, red),
        green: clamp_channel(ColorChannel::Green, green),
        blue: clamp_channel(ColorChannel::Blue, blue),
        alpha: clamp_channel(ColorChannel::Alpha, alpha),
    });
    Ok(color.stringify())
}

/// Replace a color's alpha channel, equivalent to Khroma's color overload of `rgba`.
pub fn with_alpha(input: &str, alpha: f64) -> Result<String, ColorError> {
    let mut color = ThemeColor::parse(input)?;
    color.set_channel(ColorChannel::Alpha, alpha)?;
    Ok(color.stringify())
}

/// Apply typed channel deltas with Khroma's `adjust` semantics.
pub fn adjust(input: &str, adjustment: ColorAdjustment) -> Result<String, ColorError> {
    // Khroma parses before it inspects the adjustment object, so an invalid color wins over a
    // later mixed-space setter failure.
    let mut color = ThemeColor::parse(input)?;
    let rgb_changed = [adjustment.red, adjustment.green, adjustment.blue]
        .into_iter()
        .flatten()
        .any(is_truthy_number);
    let hsl_changed = [adjustment.hue, adjustment.saturation, adjustment.lightness]
        .into_iter()
        .flatten()
        .any(is_truthy_number);
    if rgb_changed && hsl_changed {
        return Err(ColorError::MixedColorSpaces);
    }

    if rgb_changed {
        let original = color.rgba;
        if let Some(amount) = adjustment.red.filter(|value| is_truthy_number(*value)) {
            color.set_channel(ColorChannel::Red, original.red + amount)?;
        }
        if let Some(amount) = adjustment.green.filter(|value| is_truthy_number(*value)) {
            color.set_channel(ColorChannel::Green, original.green + amount)?;
        }
        if let Some(amount) = adjustment.blue.filter(|value| is_truthy_number(*value)) {
            color.set_channel(ColorChannel::Blue, original.blue + amount)?;
        }
    } else if hsl_changed {
        let original = color.hsla;
        if let Some(amount) = adjustment.hue.filter(|value| is_truthy_number(*value)) {
            color.set_channel(ColorChannel::Hue, original.hue_degrees + amount)?;
        }
        if let Some(amount) = adjustment
            .saturation
            .filter(|value| is_truthy_number(*value))
        {
            color.set_channel(
                ColorChannel::Saturation,
                original.saturation_percent + amount,
            )?;
        }
        if let Some(amount) = adjustment
            .lightness
            .filter(|value| is_truthy_number(*value))
        {
            color.set_channel(ColorChannel::Lightness, original.lightness_percent + amount)?;
        }
    }
    if let Some(amount) = adjustment.alpha.filter(|value| is_truthy_number(*value)) {
        let original_alpha = color.rgba.alpha;
        color.set_channel(ColorChannel::Alpha, original_alpha + amount)?;
    }
    Ok(color.stringify())
}

pub fn lighten(input: &str, amount: f64) -> Result<String, ColorError> {
    let mut color = ThemeColor::parse(input)?;
    color.adjust_channel(ColorChannel::Lightness, amount)?;
    Ok(color.stringify())
}

pub fn darken(input: &str, amount: f64) -> Result<String, ColorError> {
    lighten(input, -amount)
}

pub fn transparentize(input: &str, amount: f64) -> Result<String, ColorError> {
    let mut color = ThemeColor::parse(input)?;
    color.adjust_channel(ColorChannel::Alpha, -amount)?;
    Ok(color.stringify())
}

/// Invert a color at Khroma's default 100 percent weight.
pub fn invert(input: &str) -> Result<String, ColorError> {
    invert_weighted(input, 100.0)
}

/// Invert a color with Khroma's Sass-compatible weighted mixing behavior.
pub fn invert_weighted(input: &str, weight: f64) -> Result<String, ColorError> {
    let original = ThemeColor::parse(input)?.rgba;
    let inverse = RgbaChannels {
        red: 255.0 - original.red,
        green: 255.0 - original.green,
        blue: 255.0 - original.blue,
        alpha: original.alpha,
    };
    let weight_scale = weight / 100.0;
    let weight_normalized = weight_scale * 2.0 - 1.0;
    let alpha_delta = inverse.alpha - original.alpha;
    let combined = if weight_normalized * alpha_delta == -1.0 {
        weight_normalized
    } else {
        (weight_normalized + alpha_delta) / (1.0 + weight_normalized * alpha_delta)
    };
    let weight_inverse = (combined + 1.0) / 2.0;
    let weight_original = 1.0 - weight_inverse;
    rgba(
        inverse.red * weight_inverse + original.red * weight_original,
        inverse.green * weight_inverse + original.green * weight_original,
        inverse.blue * weight_inverse + original.blue * weight_original,
        inverse.alpha * weight_scale + original.alpha * (1.0 - weight_scale),
    )
}

pub fn is_dark(input: &str) -> Result<bool, ColorError> {
    let rgba = ThemeColor::parse(input)?.rgba;
    let luminance = 0.2126 * to_linear(rgba.red)
        + 0.7152 * to_linear(rgba.green)
        + 0.0722 * to_linear(rgba.blue);
    Ok(round_1e10(luminance) < 0.5)
}

fn parse_hex(input: &str) -> Option<ThemeColor> {
    let hex = input.strip_prefix('#')?;
    if !matches!(hex.len(), 3 | 4 | 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    parse_hex_with_evidence(input, input, ColorSourceFormat::Hex)
}

fn parse_hex_with_evidence(
    raw_input: &str,
    preserved_color: &str,
    source_format: ColorSourceFormat,
) -> Option<ThemeColor> {
    let hex = preserved_color.strip_prefix('#')?;
    let short = hex.len() <= 4;
    let has_alpha = matches!(hex.len(), 4 | 8);
    let component = |index: usize| -> Option<u8> {
        if short {
            let value = u8::from_str_radix(&hex[index..index + 1], 16).ok()?;
            Some(value * 17)
        } else {
            u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()
        }
    };
    let rgba = RgbaChannels {
        red: f64::from(component(0)?),
        green: f64::from(component(1)?),
        blue: f64::from(component(2)?),
        alpha: if has_alpha {
            f64::from(component(3)?) / 255.0
        } else {
            1.0
        },
    };
    Some(ThemeColor {
        raw_input: Some(raw_input.to_string()),
        preserved_color: Some(preserved_color.to_string()),
        source_format,
        source_space: ColorSpace::Rgb,
        mutation_space: None,
        rgba,
        hsla: rgb_to_hsl(rgba),
        changed: false,
    })
}

fn parse_rgb(input: &str) -> Option<ThemeColor> {
    let body = function_body(input, "rgb", "rgba")?;
    let mut cursor = ComponentCursor::new(body);
    let red = cursor.number(false, false)?;
    let red_percent = cursor.take_char('%');
    cursor.component_separator()?;
    let green = cursor.number(false, false)?;
    let green_percent = cursor.take_char('%');
    cursor.component_separator()?;
    let blue = cursor.number(false, false)?;
    let blue_percent = cursor.take_char('%');
    let alpha = if cursor.alpha_separator() {
        let value = cursor.number(true, false)?;
        let percent = cursor.take_char('%');
        Some(if percent { value / 100.0 } else { value })
    } else {
        None
    };
    cursor.finish()?;

    let rgba = RgbaChannels {
        red: clamp_rgb(if red_percent { red * 2.55 } else { red }),
        green: clamp_rgb(if green_percent { green * 2.55 } else { green }),
        blue: clamp_rgb(if blue_percent { blue * 2.55 } else { blue }),
        alpha: clamp_alpha(alpha.unwrap_or(1.0)),
    };
    Some(ThemeColor {
        raw_input: Some(input.to_string()),
        preserved_color: Some(input.to_string()),
        source_format: ColorSourceFormat::Rgb,
        source_space: ColorSpace::Rgb,
        mutation_space: None,
        rgba,
        hsla: rgb_to_hsl(rgba),
        changed: false,
    })
}

fn parse_hsl(input: &str) -> Option<ThemeColor> {
    let body = function_body(input, "hsl", "hsla")?;
    let mut cursor = ComponentCursor::new(body);
    let mut hue = cursor.number(false, true)?;
    match cursor.hue_unit() {
        Some(HueUnit::Grad) => hue *= 0.9,
        Some(HueUnit::Rad) => hue = hue * 180.0 / std::f64::consts::PI,
        Some(HueUnit::Turn) => hue *= 360.0,
        Some(HueUnit::Degrees) | None => {}
    }
    cursor.component_separator()?;
    let saturation = cursor.number(false, true)?;
    if !cursor.take_char('%') {
        return None;
    }
    cursor.component_separator()?;
    let lightness = cursor.number(false, true)?;
    if !cursor.take_char('%') {
        return None;
    }
    let alpha = if cursor.alpha_separator() {
        let value = cursor.number(true, true)?;
        let percent = cursor.take_char('%');
        Some(if percent { value / 100.0 } else { value })
    } else {
        None
    };
    cursor.finish()?;

    let hsla = HslaChannels {
        hue_degrees: clamp_hue(hue),
        saturation_percent: clamp_percent(saturation),
        lightness_percent: clamp_percent(lightness),
        alpha: clamp_alpha(alpha.unwrap_or(1.0)),
    };
    Some(ThemeColor {
        raw_input: Some(input.to_string()),
        preserved_color: Some(input.to_string()),
        source_format: ColorSourceFormat::Hsl,
        source_space: ColorSpace::Hsl,
        mutation_space: None,
        rgba: hsl_to_rgb(hsla),
        hsla,
        changed: false,
    })
}

fn parse_keyword(input: &str) -> Option<ThemeColor> {
    let canonical = keyword_hex(&input.to_ascii_lowercase())?;
    parse_hex_with_evidence(input, canonical, ColorSourceFormat::Keyword)
}

fn function_body<'a>(input: &'a str, short: &str, long: &str) -> Option<&'a str> {
    let open = input.find('(')?;
    let name = &input[..open];
    if !name.eq_ignore_ascii_case(short) && !name.eq_ignore_ascii_case(long) {
        return None;
    }
    input
        .get(open + 1..input.len().checked_sub(1)?)
        .filter(|_| input.ends_with(')'))
}

struct ComponentCursor<'a> {
    source: &'a str,
    offset: usize,
}

#[derive(Debug, Clone, Copy)]
enum HueUnit {
    Degrees,
    Grad,
    Rad,
    Turn,
}

impl<'a> ComponentCursor<'a> {
    fn new(source: &'a str) -> Self {
        let mut cursor = Self { source, offset: 0 };
        cursor.skip_whitespace();
        cursor
    }

    fn number(&mut self, allow_plus: bool, allow_negative_exponent: bool) -> Option<f64> {
        let start = self.offset;
        let _ = self.take_char('-') || (allow_plus && self.take_char('+'));

        let integer_digits = self.take_digits();
        let fractional_digits = if self.take_char('.') {
            let digits = self.take_digits();
            if digits == 0 {
                return None;
            }
            digits
        } else {
            0
        };
        if integer_digits == 0 && fractional_digits == 0 {
            return None;
        }

        let exponent_start = self.offset;
        if self.take_char('e') || self.take_char('E') {
            if allow_negative_exponent {
                self.take_char('-');
            }
            if self.take_digits() == 0 {
                self.offset = exponent_start;
            }
        }
        self.source[start..self.offset].parse().ok()
    }

    fn component_separator(&mut self) -> Option<()> {
        let before_whitespace = self.offset;
        self.skip_whitespace();
        let had_whitespace = self.offset > before_whitespace;
        if self.take_char(',') {
            self.skip_whitespace();
            Some(())
        } else if had_whitespace {
            Some(())
        } else {
            None
        }
    }

    fn alpha_separator(&mut self) -> bool {
        let start = self.offset;
        self.skip_whitespace();
        if self.take_char(',') || self.take_char('/') {
            self.skip_whitespace();
            true
        } else {
            self.offset = start;
            false
        }
    }

    fn finish(&mut self) -> Option<()> {
        self.skip_whitespace();
        (self.offset == self.source.len()).then_some(())
    }

    fn take_digits(&mut self) -> usize {
        let start = self.offset;
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_digit)
        {
            self.offset += 1;
        }
        self.offset - start
    }

    fn take_char(&mut self, expected: char) -> bool {
        if self.source[self.offset..].starts_with(expected) {
            self.offset += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn hue_unit(&mut self) -> Option<HueUnit> {
        for expected in ["grad", "turn", "rad", "deg"] {
            let Some(candidate) = self.source.get(self.offset..self.offset + expected.len()) else {
                continue;
            };
            if !candidate.eq_ignore_ascii_case(expected) {
                continue;
            }
            self.offset += expected.len();
            return Some(match candidate {
                "grad" => HueUnit::Grad,
                "rad" => HueUnit::Rad,
                "turn" => HueUnit::Turn,
                _ => HueUnit::Degrees,
            });
        }
        None
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.source[self.offset..].chars().next() {
            if !character.is_whitespace() {
                break;
            }
            self.offset += character.len_utf8();
        }
    }
}

fn rgb_to_hsl(rgba: RgbaChannels) -> HslaChannels {
    let red = rgba.red / 255.0;
    let green = rgba.green / 255.0;
    let blue = rgba.blue / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) / 2.0;
    if max == min {
        return HslaChannels {
            hue_degrees: 0.0,
            saturation_percent: 0.0,
            lightness_percent: lightness * 100.0,
            alpha: rgba.alpha,
        };
    }
    let delta = max - min;
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == red {
        ((green - blue) / delta + if green < blue { 6.0 } else { 0.0 }) * 60.0
    } else if max == green {
        ((blue - red) / delta + 2.0) * 60.0
    } else {
        ((red - green) / delta + 4.0) * 60.0
    };
    HslaChannels {
        hue_degrees: hue,
        saturation_percent: saturation * 100.0,
        lightness_percent: lightness * 100.0,
        alpha: rgba.alpha,
    }
}

fn hsl_to_rgb(hsla: HslaChannels) -> RgbaChannels {
    if hsla.saturation_percent == 0.0 {
        let channel = hsla.lightness_percent * 2.55;
        return RgbaChannels {
            red: channel,
            green: channel,
            blue: channel,
            alpha: hsla.alpha,
        };
    }
    let hue = hsla.hue_degrees / 360.0;
    let saturation = hsla.saturation_percent / 100.0;
    let lightness = hsla.lightness_percent / 100.0;
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    RgbaChannels {
        red: hue_to_rgb(p, q, hue + 1.0 / 3.0) * 255.0,
        green: hue_to_rgb(p, q, hue) * 255.0,
        blue: hue_to_rgb(p, q, hue - 1.0 / 3.0) * 255.0,
        alpha: hsla.alpha,
    }
}

fn hue_to_rgb(p: f64, q: f64, mut hue: f64) -> f64 {
    if hue < 0.0 {
        hue += 1.0;
    }
    if hue > 1.0 {
        hue -= 1.0;
    }
    if hue < 1.0 / 6.0 {
        p + (q - p) * 6.0 * hue
    } else if hue < 0.5 {
        q
    } else if hue < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - hue) * 6.0
    } else {
        p
    }
}

fn stringify_hex(rgba: RgbaChannels) -> String {
    let red = decimal_to_hex(rgba.red);
    let green = decimal_to_hex(rgba.green);
    let blue = decimal_to_hex(rgba.blue);
    if rgba.alpha < 1.0 {
        format!("#{red}{green}{blue}{}", decimal_to_hex(rgba.alpha * 255.0))
    } else {
        format!("#{red}{green}{blue}")
    }
}

fn stringify_rgb(rgba: RgbaChannels) -> String {
    if rgba.alpha < 1.0 {
        format!(
            "rgba({}, {}, {}, {})",
            format_js_rounded(rgba.red),
            format_js_rounded(rgba.green),
            format_js_rounded(rgba.blue),
            format_js_rounded(rgba.alpha)
        )
    } else {
        format!(
            "rgb({}, {}, {})",
            format_js_rounded(rgba.red),
            format_js_rounded(rgba.green),
            format_js_rounded(rgba.blue)
        )
    }
}

fn stringify_hsl(hsla: HslaChannels) -> String {
    if hsla.alpha < 1.0 {
        format!(
            "hsla({}, {}%, {}%, {})",
            format_js_rounded(hsla.hue_degrees),
            format_js_rounded(hsla.saturation_percent),
            format_js_rounded(hsla.lightness_percent),
            format_js_number(hsla.alpha)
        )
    } else {
        format!(
            "hsl({}, {}%, {}%)",
            format_js_rounded(hsla.hue_degrees),
            format_js_rounded(hsla.saturation_percent),
            format_js_rounded(hsla.lightness_percent)
        )
    }
}

fn decimal_to_hex(value: f64) -> String {
    let rounded = js_round(value).clamp(0.0, 255.0) as u8;
    format!("{rounded:02x}")
}

fn round_1e10(value: f64) -> f64 {
    js_round(value * 10_000_000_000.0) / 10_000_000_000.0
}

fn js_round(value: f64) -> f64 {
    if !value.is_finite() || value.fract() == 0.0 {
        return value;
    }
    (value + 0.5).floor()
}

fn format_js_rounded(value: f64) -> String {
    format_js_number(round_1e10(value))
}

fn format_js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value == f64::INFINITY {
        return "Infinity".to_string();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let mut buffer = Buffer::new();
    buffer.format_finite(value).to_string()
}

fn clamp_channel(channel: ColorChannel, value: f64) -> f64 {
    match channel {
        ColorChannel::Red | ColorChannel::Green | ColorChannel::Blue => clamp_rgb(value),
        ColorChannel::Hue => clamp_hue(value),
        ColorChannel::Saturation | ColorChannel::Lightness => clamp_percent(value),
        ColorChannel::Alpha => clamp_alpha(value),
    }
}

fn clamp_rgb(value: f64) -> f64 {
    value.clamp(0.0, 255.0)
}

fn clamp_hue(value: f64) -> f64 {
    value % 360.0
}

fn clamp_percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

fn clamp_alpha(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn is_integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0
}

fn is_truthy_number(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

fn to_linear(channel: f64) -> f64 {
    let normalized = channel / 255.0;
    if channel > 0.03928 {
        ((normalized + 0.055) / 1.055).powf(2.4)
    } else {
        normalized / 12.92
    }
}

fn keyword_hex(keyword: &str) -> Option<&'static str> {
    Some(match keyword {
        "aliceblue" => "#f0f8ff",
        "antiquewhite" => "#faebd7",
        "aqua" | "cyanaqua" => "#00ffff",
        "aquamarine" => "#7fffd4",
        "azure" => "#f0ffff",
        "beige" => "#f5f5dc",
        "bisque" => "#ffe4c4",
        "black" => "#000000",
        "blanchedalmond" => "#ffebcd",
        "blue" => "#0000ff",
        "blueviolet" => "#8a2be2",
        "brown" => "#a52a2a",
        "burlywood" => "#deb887",
        "cadetblue" => "#5f9ea0",
        "chartreuse" => "#7fff00",
        "chocolate" => "#d2691e",
        "coral" => "#ff7f50",
        "cornflowerblue" => "#6495ed",
        "cornsilk" => "#fff8dc",
        "crimson" => "#dc143c",
        "darkblue" => "#00008b",
        "darkcyan" => "#008b8b",
        "darkgoldenrod" => "#b8860b",
        "darkgray" | "darkgrey" => "#a9a9a9",
        "darkgreen" => "#006400",
        "darkkhaki" => "#bdb76b",
        "darkmagenta" => "#8b008b",
        "darkolivegreen" => "#556b2f",
        "darkorange" => "#ff8c00",
        "darkorchid" => "#9932cc",
        "darkred" => "#8b0000",
        "darksalmon" => "#e9967a",
        "darkseagreen" => "#8fbc8f",
        "darkslateblue" => "#483d8b",
        "darkslategray" | "darkslategrey" => "#2f4f4f",
        "darkturquoise" => "#00ced1",
        "darkviolet" => "#9400d3",
        "deeppink" => "#ff1493",
        "deepskyblue" => "#00bfff",
        "dimgray" | "dimgrey" => "#696969",
        "dodgerblue" => "#1e90ff",
        "firebrick" => "#b22222",
        "floralwhite" => "#fffaf0",
        "forestgreen" => "#228b22",
        "fuchsia" | "magenta" => "#ff00ff",
        "gainsboro" => "#dcdcdc",
        "ghostwhite" => "#f8f8ff",
        "gold" => "#ffd700",
        "goldenrod" => "#daa520",
        "gray" | "grey" => "#808080",
        "green" => "#008000",
        "greenyellow" => "#adff2f",
        "honeydew" => "#f0fff0",
        "hotpink" => "#ff69b4",
        "indianred" => "#cd5c5c",
        "indigo" => "#4b0082",
        "ivory" => "#fffff0",
        "khaki" => "#f0e68c",
        "lavender" => "#e6e6fa",
        "lavenderblush" => "#fff0f5",
        "lawngreen" => "#7cfc00",
        "lemonchiffon" => "#fffacd",
        "lightblue" => "#add8e6",
        "lightcoral" => "#f08080",
        "lightcyan" => "#e0ffff",
        "lightgoldenrodyellow" => "#fafad2",
        "lightgray" | "lightgrey" => "#d3d3d3",
        "lightgreen" => "#90ee90",
        "lightpink" => "#ffb6c1",
        "lightsalmon" => "#ffa07a",
        "lightseagreen" => "#20b2aa",
        "lightskyblue" => "#87cefa",
        "lightslategray" | "lightslategrey" => "#778899",
        "lightsteelblue" => "#b0c4de",
        "lightyellow" => "#ffffe0",
        "lime" => "#00ff00",
        "limegreen" => "#32cd32",
        "linen" => "#faf0e6",
        "maroon" => "#800000",
        "mediumaquamarine" => "#66cdaa",
        "mediumblue" => "#0000cd",
        "mediumorchid" => "#ba55d3",
        "mediumpurple" => "#9370db",
        "mediumseagreen" => "#3cb371",
        "mediumslateblue" => "#7b68ee",
        "mediumspringgreen" => "#00fa9a",
        "mediumturquoise" => "#48d1cc",
        "mediumvioletred" => "#c71585",
        "midnightblue" => "#191970",
        "mintcream" => "#f5fffa",
        "mistyrose" => "#ffe4e1",
        "moccasin" => "#ffe4b5",
        "navajowhite" => "#ffdead",
        "navy" => "#000080",
        "oldlace" => "#fdf5e6",
        "olive" => "#808000",
        "olivedrab" => "#6b8e23",
        "orange" => "#ffa500",
        "orangered" => "#ff4500",
        "orchid" => "#da70d6",
        "palegoldenrod" => "#eee8aa",
        "palegreen" => "#98fb98",
        "paleturquoise" => "#afeeee",
        "palevioletred" => "#db7093",
        "papayawhip" => "#ffefd5",
        "peachpuff" => "#ffdab9",
        "peru" => "#cd853f",
        "pink" => "#ffc0cb",
        "plum" => "#dda0dd",
        "powderblue" => "#b0e0e6",
        "purple" => "#800080",
        "rebeccapurple" => "#663399",
        "red" => "#ff0000",
        "rosybrown" => "#bc8f8f",
        "royalblue" => "#4169e1",
        "saddlebrown" => "#8b4513",
        "salmon" => "#fa8072",
        "sandybrown" => "#f4a460",
        "seagreen" => "#2e8b57",
        "seashell" => "#fff5ee",
        "sienna" => "#a0522d",
        "silver" => "#c0c0c0",
        "skyblue" => "#87ceeb",
        "slateblue" => "#6a5acd",
        "slategray" | "slategrey" => "#708090",
        "snow" => "#fffafa",
        "springgreen" => "#00ff7f",
        "tan" => "#d2b48c",
        "teal" => "#008080",
        "thistle" => "#d8bfd8",
        "transparent" => "#00000000",
        "turquoise" => "#40e0d0",
        "violet" => "#ee82ee",
        "wheat" => "#f5deb3",
        "white" => "#ffffff",
        "whitesmoke" => "#f5f5f5",
        "yellow" => "#ffff00",
        "yellowgreen" => "#9acd32",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_khroma_color_syntax_without_normalizing_raw_values() {
        let cases = [
            ("#abc", ColorSourceFormat::Hex, [170.0, 187.0, 204.0, 1.0]),
            (
                "#abcd",
                ColorSourceFormat::Hex,
                [170.0, 187.0, 204.0, 221.0 / 255.0],
            ),
            (
                "#10203040",
                ColorSourceFormat::Hex,
                [16.0, 32.0, 48.0, 64.0 / 255.0],
            ),
            (
                "rgb(10, 20, 30)",
                ColorSourceFormat::Rgb,
                [10.0, 20.0, 30.0, 1.0],
            ),
            (
                "RGB(10% 20% 30% / 40%)",
                ColorSourceFormat::Rgb,
                [25.5, 51.0, 76.5, 0.4],
            ),
            ("rgb( 1,2,3)", ColorSourceFormat::Rgb, [1.0, 2.0, 3.0, 1.0]),
        ];
        for (input, format, expected) in cases {
            let color = ThemeColor::parse(input).unwrap();
            assert_eq!(color.raw(), Some(input));
            assert_eq!(color.source_format(), format);
            let actual = color.rgba_channels();
            assert_eq!(
                [actual.red, actual.green, actual.blue, actual.alpha],
                expected
            );
            assert_eq!(color.stringify(), input);
        }
    }

    #[test]
    fn parses_hsl_angles_alpha_and_keywords_with_khroma_evidence() {
        let turn = ThemeColor::parse("hsla(.5turn 50% 25% / .75)").unwrap();
        assert_eq!(turn.source_format(), ColorSourceFormat::Hsl);
        assert_eq!(
            turn.hsla_channels(),
            HslaChannels {
                hue_degrees: 180.0,
                saturation_percent: 50.0,
                lightness_percent: 25.0,
                alpha: 0.75,
            }
        );
        assert_eq!(turn.stringify(), "hsla(.5turn 50% 25% / .75)");
        assert_eq!(
            ThemeColor::parse("hsl( 1,2%,3%)").unwrap().stringify(),
            "hsl( 1,2%,3%)"
        );
        assert_eq!(
            channel("hsl(1turn 50% 50%)", ColorChannel::Hue).unwrap(),
            0.0
        );
        assert_eq!(
            channel("hsl(1TURN 50% 50%)", ColorChannel::Hue).unwrap(),
            1.0
        );
        assert_eq!(
            channel("hsl(100grad 50% 50%)", ColorChannel::Hue).unwrap(),
            90.0
        );
        assert_eq!(
            channel("hsl(100GRAD 50% 50%)", ColorChannel::Hue).unwrap(),
            100.0
        );

        let keyword = ThemeColor::parse("ReD").unwrap();
        assert_eq!(keyword.raw(), Some("ReD"));
        assert_eq!(keyword.source_format(), ColorSourceFormat::Keyword);
        assert_eq!(keyword.stringify(), "#ff0000");
        assert_eq!(
            ThemeColor::parse("transparent").unwrap().stringify(),
            "#00000000"
        );
    }

    #[test]
    fn rejects_non_khroma_forms() {
        for input in [
            " #fff",
            "#12",
            "rgb(1 2)",
            "rgb(1 2 3 0.5)",
            "hsl(1 2 3)",
            "unknown",
        ] {
            assert!(
                matches!(
                    ThemeColor::parse(input),
                    Err(ColorError::UnsupportedFormat { .. })
                ),
                "{input}"
            );
        }
    }

    #[test]
    fn matches_khroma_operation_oracle_matrix() {
        assert_eq!(
            lighten("#123456", 10.0).unwrap(),
            "hsl(210, 65.3846153846%, 30.3921568627%)"
        );
        assert_eq!(
            darken("hsl(20, 30%, 40%)", 50.0).unwrap(),
            "hsl(20, 30%, 0%)"
        );
        assert_eq!(
            adjust("#102030", ColorAdjustment::rgb(5.0, 10.0, 15.0)).unwrap(),
            "#152a3f"
        );
        assert_eq!(
            adjust("rgb(18 52 86 / .5)", ColorAdjustment::hsl(-120.0, 0.0, 5.0)).unwrap(),
            "hsla(90, 65.3846153846%, 25.3921568627%, 0.5)"
        );
        assert_eq!(invert("#123456").unwrap(), "#edcba9");
        assert_eq!(invert("hsl(0, 100%, 50%)").unwrap(), "#00ffff");
        assert!(is_dark("#123456").unwrap());
        assert!(!is_dark("white").unwrap());
        assert_eq!(
            transparentize("rgb(1, 2, 3)", 0.25).unwrap(),
            "rgba(1, 2, 3, 0.75)"
        );
    }

    #[test]
    fn channel_and_rgba_match_khroma_rounding_and_clamping() {
        assert_eq!(channel("#abcdef", ColorChannel::Hue).unwrap(), 210.0);
        assert_eq!(
            channel("rgb(10% 20% 30%)", ColorChannel::Red).unwrap(),
            25.5
        );
        assert_eq!(rgba(255.0, 255.0, 255.0, 70.0).unwrap(), "#ffffff");
        assert_eq!(rgba(255.0, 255.0, 255.0, 50.0).unwrap(), "#ffffff");
        assert_eq!(
            rgba(1.25, 2.5, 3.75, 0.5).unwrap(),
            "rgba(1.25, 2.5, 3.75, 0.5)"
        );
        assert_eq!(
            with_alpha("hsl(20, 30%, 40%)", 0.25).unwrap(),
            "hsla(20, 30%, 40%, 0.25)"
        );
        assert_eq!(with_alpha("#fff", 1.0).unwrap(), "#ffffff");
    }

    #[test]
    fn preserves_changed_space_and_nan_behavior() {
        assert_eq!(lighten("#fff", 0.0).unwrap(), "#fff");
        assert_eq!(
            lighten("#ECECFF", f64::NAN).unwrap(),
            "hsl(240, 100%, NaN%)"
        );
        assert_eq!(
            adjust("red", ColorAdjustment::default()).unwrap(),
            "#ff0000"
        );
        assert_eq!(
            adjust(
                "#fff",
                ColorAdjustment {
                    red: Some(1.0),
                    ..ColorAdjustment::default()
                }
            )
            .unwrap(),
            "#ffffff"
        );
        assert_eq!(
            lighten("hsl(20, 30%, 100%)", 10.0).unwrap(),
            "hsl(20, 30%, 100%)"
        );
        let mixed = ColorAdjustment {
            red: Some(1.0),
            hue: Some(1.0),
            ..ColorAdjustment::default()
        };
        assert_eq!(adjust("#fff", mixed), Err(ColorError::MixedColorSpaces));
        assert!(matches!(
            adjust("nope", mixed),
            Err(ColorError::UnsupportedFormat { .. })
        ));
        assert_eq!(
            ColorError::UnsupportedFormat {
                input: "nope".to_string()
            }
            .to_string(),
            "Unsupported color format: \"nope\""
        );
        assert_eq!(
            ColorError::MixedColorSpaces.to_string(),
            "Cannot change both RGB and HSL channels at the same time"
        );
    }
}
