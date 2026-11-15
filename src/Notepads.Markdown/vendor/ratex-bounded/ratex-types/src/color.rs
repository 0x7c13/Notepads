use serde::{Deserialize, Serialize};

fn parse_csv_f32(s: &str, expected: usize) -> Option<Vec<f32>> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != expected {
        return None;
    }
    parts.iter().map(|p| p.trim().parse::<f32>().ok()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Parse a hex color string like "#ff0000", "#f00", "#ff000080", or "#f008".
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                Some(Self::rgb(
                    (r * 17) as f32 / 255.0,
                    (g * 17) as f32 / 255.0,
                    (b * 17) as f32 / 255.0,
                ))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                let a = u8::from_str_radix(&hex[3..4], 16).ok()?;
                Some(Self::new(
                    (r * 17) as f32 / 255.0,
                    (g * 17) as f32 / 255.0,
                    (b * 17) as f32 / 255.0,
                    (a * 17) as f32 / 255.0,
                ))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::rgb(
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                ))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::new(
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    a as f32 / 255.0,
                ))
            }
            _ => None,
        }
    }

    /// Parse a color from a MathJax color model + value string.
    /// Model is case-sensitive per MathJax spec:
    ///   RGB  → integers 0-255  e.g. "178,34,34"
    ///   rgb  → floats 0-1      e.g. "0.7,0.13,0.13"
    ///   HTML → hex RRGGBB      e.g. "B22222"
    ///   gray → float 0-1       e.g. "0.5"
    ///   cmyk → floats 0-1      e.g. "0,0.8,0.8,0"
    pub fn from_model(model: &str, value: &str) -> Option<Self> {
        match model {
            "RGB" => {
                let p = parse_csv_f32(value, 3)?;
                Some(Self::rgb(p[0] / 255.0, p[1] / 255.0, p[2] / 255.0))
            }
            "rgb" => {
                let p = parse_csv_f32(value, 3)?;
                Some(Self::rgb(p[0], p[1], p[2]))
            }
            "HTML" => Self::from_hex(value),
            "gray" => {
                let g: f32 = value.trim().parse().ok()?;
                Some(Self::rgb(g, g, g))
            }
            "cmyk" => {
                let p = parse_csv_f32(value, 4)?;
                let (c, m, y, k) = (p[0], p[1], p[2], p[3]);
                Some(Self::rgb(
                    (1.0 - c) * (1.0 - k),
                    (1.0 - m) * (1.0 - k),
                    (1.0 - y) * (1.0 - k),
                ))
            }
            _ => None,
        }
    }

    /// Parse any supported color string: hex, named, or `[MODEL]value` (MathJax model syntax).
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(rest) = s.strip_prefix('[') {
            if let Some(bracket_end) = rest.find(']') {
                let model = &rest[..bracket_end];
                let value = &rest[bracket_end + 1..];
                return Self::from_model(model, value);
            }
        }
        Self::from_hex(s).or_else(|| Self::from_name(s))
    }

    /// Parse a named CSS color (subset used by KaTeX).
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "red" => Some(Self::rgb(1.0, 0.0, 0.0)),
            // CSS `green` is #008000; `lime` is #00ff00 (HTML 4 / CSS1 standard colors).
            "green" => Some(Self::rgb(0.0, 0.502, 0.0)),
            "lime" => Some(Self::rgb(0.0, 1.0, 0.0)),
            "blue" => Some(Self::rgb(0.0, 0.0, 1.0)),
            "maroon" => Some(Self::rgb(0.502, 0.0, 0.0)),
            "navy" => Some(Self::rgb(0.0, 0.0, 0.502)),
            "olive" => Some(Self::rgb(0.502, 0.502, 0.0)),
            "silver" => Some(Self::rgb(0.753, 0.753, 0.753)),
            "white" => Some(Self::WHITE),
            "black" => Some(Self::BLACK),
            "orange" => Some(Self::rgb(1.0, 0.647, 0.0)),
            "yellow" => Some(Self::rgb(1.0, 1.0, 0.0)),
            "purple" => Some(Self::rgb(0.502, 0.0, 0.502)),
            // CSS: `aqua` ≡ `cyan` (#00ffff); common in `\colorbox`/xcolor examples.
            "aqua" | "cyan" => Some(Self::rgb(0.0, 1.0, 1.0)),
            // CSS: `fuchsia` ≡ `magenta`
            "fuchsia" | "magenta" => Some(Self::rgb(1.0, 0.0, 1.0)),
            "gray" | "grey" => Some(Self::rgb(0.502, 0.502, 0.502)),
            "brown" => Some(Self::rgb(0.647, 0.165, 0.165)),
            "pink" => Some(Self::rgb(1.0, 0.753, 0.796)),
            "teal" => Some(Self::rgb(0.0, 0.502, 0.502)),
            "transparent" => Some(Self::new(0.0, 0.0, 0.0, 0.0)),
            _ => None,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if (self.a - 1.0).abs() < f32::EPSILON {
            write!(
                f,
                "#{:02x}{:02x}{:02x}",
                (self.r * 255.0) as u8,
                (self.g * 255.0) as u8,
                (self.b * 255.0) as u8,
            )
        } else {
            write!(
                f,
                "rgba({}, {}, {}, {:.2})",
                (self.r * 255.0) as u8,
                (self.g * 255.0) as u8,
                (self.b * 255.0) as u8,
                self.a,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_black() {
        let c = Color::default();
        assert_eq!(c, Color::BLACK);
    }

    #[test]
    fn test_from_hex_6() {
        let c = Color::from_hex("#ff0000").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
        assert!(c.g.abs() < 0.01);
        assert!(c.b.abs() < 0.01);
        assert!((c.a - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_from_hex_3() {
        let c = Color::from_hex("#f00").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
        assert!(c.g.abs() < 0.01);
        assert!(c.b.abs() < 0.01);
    }

    #[test]
    fn test_from_hex_4() {
        let c = Color::from_hex("#f008").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
        assert!(c.g.abs() < 0.01);
        assert!(c.b.abs() < 0.01);
        assert!((c.a - 136.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_from_hex_no_hash() {
        let c = Color::from_hex("00ff00").unwrap();
        assert!(c.r.abs() < 0.01);
        assert!((c.g - 1.0).abs() < 0.01);
        assert!(c.b.abs() < 0.01);
    }

    #[test]
    fn test_from_hex_8() {
        let c = Color::from_hex("#ff000010").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
        assert!(c.g.abs() < 0.01);
        assert!(c.b.abs() < 0.01);
        assert!((c.a - 16.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_non_ascii_color_returns_none() {
        assert!(Color::parse("😀").is_none());
        assert!(Color::from_hex("ééé").is_none());
    }

    #[test]
    fn test_from_name() {
        assert!(Color::from_name("red").is_some());
        assert!(Color::from_name("Blue").is_some());
        let aqua = Color::from_name("aqua").unwrap();
        let cyan = Color::from_name("cyan").unwrap();
        assert_eq!(aqua, cyan);
        assert_eq!(
            Color::from_name("transparent"),
            Some(Color::new(0.0, 0.0, 0.0, 0.0))
        );
        assert!(Color::from_name("lime").is_some());
        assert!(Color::from_name("maroon").is_some());
        assert!(Color::from_name("navy").is_some());
        assert!(Color::from_name("olive").is_some());
        assert!(Color::from_name("silver").is_some());
        assert!(Color::from_name("nonexistent").is_none());
    }

    #[test]
    fn test_display_rgb() {
        let c = Color::rgb(1.0, 0.0, 0.0);
        assert_eq!(c.to_string(), "#ff0000");
    }

    #[test]
    fn test_display_rgba() {
        let c = Color::new(1.0, 0.0, 0.0, 0.5);
        assert_eq!(c.to_string(), "rgba(255, 0, 0, 0.50)");
    }

    #[test]
    fn test_serde_roundtrip() {
        let c = Color::rgb(0.5, 0.25, 0.75);
        let json = serde_json::to_string(&c).unwrap();
        let c2: Color = serde_json::from_str(&json).unwrap();
        assert!((c.r - c2.r).abs() < f32::EPSILON);
        assert!((c.g - c2.g).abs() < f32::EPSILON);
        assert!((c.b - c2.b).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_hex() {
        let c = Color::parse("#0000ff").unwrap();
        assert!(c.r.abs() < 0.01);
        assert!(c.g.abs() < 0.01);
        assert!((c.b - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_name() {
        let c = Color::parse("red").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_from_model_rgb() {
        let c = Color::from_model("RGB", "178,34,34").unwrap();
        assert!((c.r - 178.0 / 255.0).abs() < 0.01);
        assert!((c.g - 34.0 / 255.0).abs() < 0.01);
        assert!((c.b - 34.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_from_model_rgb_lower() {
        let c = Color::from_model("rgb", "0.7,0.13,0.13").unwrap();
        assert!((c.r - 0.7).abs() < 0.01);
        assert!((c.g - 0.13).abs() < 0.01);
    }

    #[test]
    fn test_from_model_html() {
        let c = Color::from_model("HTML", "B22222").unwrap();
        assert!((c.r - 178.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_from_model_gray() {
        let c = Color::from_model("gray", "0.5").unwrap();
        assert!((c.r - 0.5).abs() < 0.01);
        assert!((c.g - 0.5).abs() < 0.01);
        assert!((c.b - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_from_model_cmyk() {
        // cmyk 0,0.8,0.8,0 → r=1, g=0.2, b=0.2
        let c = Color::from_model("cmyk", "0,0.8,0.8,0").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
        assert!((c.g - 0.2).abs() < 0.01);
        assert!((c.b - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_parse_model_encoded() {
        let c = Color::parse("[RGB]178,34,34").unwrap();
        assert!((c.r - 178.0 / 255.0).abs() < 0.01);
    }
}
