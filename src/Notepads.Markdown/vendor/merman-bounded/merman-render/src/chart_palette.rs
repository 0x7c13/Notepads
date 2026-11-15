//! Mermaid XYChart palette selection.
//!
//! Mermaid 11.16 themes provide complete, fixed `xyChart.plotColorPalette` values. XYChart only
//! splits that token and cycles through it; it does not synthesize colors from an accent or
//! background. Keeping that contract here prevents renderer-local palette heuristics from
//! diverging from theme resolution.

const DEFAULT_PALETTE: &str =
    "#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3";
const BASE_PALETTE: &str =
    "#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0";
const DARK_PALETTE: &str =
    "#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22";
const FOREST_PALETTE: &str =
    "#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176";
const NEUTRAL_PALETTE: &str =
    "#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0";

fn parse_palette(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|color| !color.is_empty())
        .map(str::to_string)
        .collect()
}

fn upstream_palette(theme_name: &str) -> &'static str {
    match theme_name {
        "dark" => DARK_PALETTE,
        "forest" => FOREST_PALETTE,
        "neutral" => NEUTRAL_PALETTE,
        "base" | "neo" | "neo-dark" | "redux" | "redux-dark" | "redux-color"
        | "redux-dark-color" => BASE_PALETTE,
        _ => DEFAULT_PALETTE,
    }
}

pub(crate) fn resolve_xychart_plot_palette(
    theme_name: &str,
    explicit_palette: Option<&str>,
) -> Vec<String> {
    if let Some(explicit_palette) = explicit_palette {
        let palette = parse_palette(explicit_palette);
        if !palette.is_empty() {
            return palette;
        }
    }
    parse_palette(upstream_palette(theme_name))
}

pub(crate) fn plot_color_from_palette(palette: &[String], plot_index: usize) -> String {
    if palette.is_empty() {
        return String::new();
    }
    let index = if plot_index == 0 {
        0
    } else {
        plot_index % palette.len()
    };
    palette[index].clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_themes_use_their_upstream_fixed_palettes() {
        let cases = [
            ("default", "#ECECFF"),
            ("base", "#FFF4DD"),
            ("dark", "#3498db"),
            ("forest", "#CDE498"),
            ("neutral", "#EEE"),
            ("redux-dark-color", "#FFF4DD"),
        ];
        for (theme, expected_first) in cases {
            let palette = resolve_xychart_plot_palette(theme, None);
            assert_eq!(palette.len(), 10, "{theme}");
            assert_eq!(palette[0], expected_first, "{theme}");
        }
    }

    #[test]
    fn explicit_plot_palette_wins_and_empty_values_use_the_theme_default() {
        assert_eq!(
            resolve_xychart_plot_palette("dark", Some("#001122, #334455")),
            vec!["#001122".to_string(), "#334455".to_string()]
        );
        assert_eq!(
            resolve_xychart_plot_palette("dark", Some(" , "))[0],
            "#3498db"
        );
    }
}
