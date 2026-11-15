use serde::{Deserialize, Serialize};

/// SVG-style path commands for drawing glyph outlines, radical signs,
/// large delimiters, etc.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PathCommand {
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
    CubicTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    QuadTo {
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_roundtrip() {
        let cmds = vec![
            PathCommand::MoveTo { x: 0.0, y: 0.0 },
            PathCommand::LineTo { x: 10.0, y: 0.0 },
            PathCommand::CubicTo {
                x1: 10.0,
                y1: 5.0,
                x2: 5.0,
                y2: 10.0,
                x: 0.0,
                y: 10.0,
            },
            PathCommand::QuadTo {
                x1: 5.0,
                y1: 5.0,
                x: 0.0,
                y: 0.0,
            },
            PathCommand::Close,
        ];
        let json = serde_json::to_string(&cmds).unwrap();
        let cmds2: Vec<PathCommand> = serde_json::from_str(&json).unwrap();
        assert_eq!(cmds, cmds2);
    }
}
