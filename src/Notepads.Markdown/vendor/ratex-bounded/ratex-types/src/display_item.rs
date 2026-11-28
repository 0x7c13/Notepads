use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::path_command::PathCommand;

/// The final output of the layout engine: a flat list of drawing commands
/// with absolute coordinates, ready for platform renderers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
    pub width: f64,
    pub height: f64,
    pub depth: f64,
}

/// A single drawing instruction with absolute position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DisplayItem {
    /// Draw a glyph outline at the given position.
    GlyphPath {
        x: f64,
        y: f64,
        scale: f64,
        font: String,
        char_code: u32,
        color: Color,
    },
    /// Draw a horizontal line (fraction bars, overlines, etc.).
    Line {
        x: f64,
        y: f64,
        width: f64,
        thickness: f64,
        color: Color,
        /// If true, render as a dashed line (for \hdashline).
        #[serde(default)]
        dashed: bool,
    },
    /// Draw a filled rectangle (\colorbox backgrounds).
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        color: Color,
    },
    /// Draw an arbitrary SVG-style path (radical signs, large delimiters).
    Path {
        x: f64,
        y: f64,
        commands: Vec<PathCommand>,
        fill: bool,
        color: Color,
    },
}

impl DisplayList {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            width: 0.0,
            height: 0.0,
            depth: 0.0,
        }
    }

    pub fn total_height(&self) -> f64 {
        self.height + self.depth
    }
}

impl Default for DisplayList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_list_new() {
        let dl = DisplayList::new();
        assert!(dl.items.is_empty());
        assert!((dl.width - 0.0).abs() < f64::EPSILON);
        assert!((dl.total_height() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_serde_roundtrip() {
        let dl = DisplayList {
            items: vec![
                DisplayItem::Line {
                    x: 0.0,
                    y: 5.0,
                    width: 10.0,
                    thickness: 0.04,
                    color: Color::BLACK,
                    dashed: false,
                },
                DisplayItem::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                    color: Color::rgb(1.0, 0.0, 0.0),
                },
                DisplayItem::Path {
                    x: 0.0,
                    y: 0.0,
                    commands: vec![
                        PathCommand::MoveTo { x: 0.0, y: 0.0 },
                        PathCommand::LineTo { x: 5.0, y: 5.0 },
                        PathCommand::Close,
                    ],
                    fill: false,
                    color: Color::BLACK,
                },
            ],
            width: 10.0,
            height: 5.0,
            depth: 5.0,
        };

        let json = serde_json::to_string(&dl).unwrap();
        let dl2: DisplayList = serde_json::from_str(&json).unwrap();

        assert_eq!(dl.items.len(), dl2.items.len());
        assert!((dl.width - dl2.width).abs() < f64::EPSILON);
        assert!((dl.height - dl2.height).abs() < f64::EPSILON);
        assert!((dl.depth - dl2.depth).abs() < f64::EPSILON);
    }

    #[test]
    fn test_total_height() {
        let dl = DisplayList {
            items: vec![],
            width: 10.0,
            height: 3.0,
            depth: 2.0,
        };
        assert!((dl.total_height() - 5.0).abs() < f64::EPSILON);
    }
}
