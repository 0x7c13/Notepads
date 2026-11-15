use crate::config::{config_bool, config_f64, config_string};
use serde_json::Value;

const DEFAULT_TEXT_POSITION: f64 = 0.75;
const DEFAULT_DONUT_HOLE: f64 = 0.0;
const DEFAULT_LEGEND_POSITION: &str = "right";
const DEFAULT_USE_MAX_WIDTH: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PieLegendPosition {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

pub(crate) struct PieConfigView<'a> {
    pie_config: &'a Value,
}

impl<'a> PieConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            pie_config: effective_config.get("pie").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> PieLayoutSettings {
        PieLayoutSettings {
            text_position: self
                .pie_f64("textPosition")
                .unwrap_or(DEFAULT_TEXT_POSITION),
            legend_position: self.legend_position(),
        }
    }

    pub(crate) fn render_settings(&self) -> PieRenderSettings {
        PieRenderSettings {
            donut_hole: self.donut_hole(),
            legend_position: self.legend_position(),
            use_max_width: self
                .pie_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
        }
    }

    fn donut_hole(&self) -> f64 {
        let donut_hole = self.pie_f64("donutHole").unwrap_or(DEFAULT_DONUT_HOLE);
        if donut_hole > 0.0 && donut_hole <= 0.9 {
            donut_hole
        } else {
            DEFAULT_DONUT_HOLE
        }
    }

    fn legend_position(&self) -> PieLegendPosition {
        match self.pie_string("legendPosition").as_deref() {
            Some("top") => PieLegendPosition::Top,
            Some("bottom") => PieLegendPosition::Bottom,
            Some("left") => PieLegendPosition::Left,
            Some("center") => PieLegendPosition::Center,
            Some("right") => PieLegendPosition::Right,
            _ => match DEFAULT_LEGEND_POSITION {
                "top" => PieLegendPosition::Top,
                "bottom" => PieLegendPosition::Bottom,
                "left" => PieLegendPosition::Left,
                "center" => PieLegendPosition::Center,
                _ => PieLegendPosition::Right,
            },
        }
    }

    fn pie_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.pie_config, &[key])
    }

    fn pie_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.pie_config, &[key])
    }

    fn pie_string(&self, key: &str) -> Option<String> {
        config_string(self.pie_config, &[key])
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PieLayoutSettings {
    pub(crate) text_position: f64,
    pub(crate) legend_position: PieLegendPosition,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PieRenderSettings {
    pub(crate) donut_hole: f64,
    pub(crate) legend_position: PieLegendPosition,
    pub(crate) use_max_width: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pie_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = PieConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.text_position, DEFAULT_TEXT_POSITION);
        assert_eq!(settings.legend_position, PieLegendPosition::Right);
    }

    #[test]
    fn pie_layout_settings_project_configured_values() {
        let cfg = json!({
            "pie": {
                "textPosition": "0.5",
                "legendPosition": "top"
            }
        });
        let settings = PieConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.text_position, 0.5);
        assert_eq!(settings.legend_position, PieLegendPosition::Top);
    }

    #[test]
    fn pie_render_settings_project_and_normalize_values() {
        let cfg = json!({
            "pie": {
                "donutHole": "0.4",
                "legendPosition": "left",
                "useMaxWidth": false
            }
        });
        let settings = PieConfigView::new(&cfg).render_settings();

        assert_eq!(settings.donut_hole, 0.4);
        assert_eq!(settings.legend_position, PieLegendPosition::Left);
        assert!(!settings.use_max_width);
    }

    #[test]
    fn pie_render_settings_reject_invalid_donut_hole() {
        for donut_hole in [0.0, -0.1, 1.0] {
            let cfg = json!({
                "pie": {
                    "donutHole": donut_hole
                }
            });
            let settings = PieConfigView::new(&cfg).render_settings();

            assert_eq!(settings.donut_hole, DEFAULT_DONUT_HOLE);
        }
    }
}
