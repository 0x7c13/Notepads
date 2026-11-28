use crate::config::{config_bool, config_f64_or};
use serde_json::Value;

const DEFAULT_WIDTH: f64 = 600.0;
const DEFAULT_HEIGHT: f64 = 600.0;
const DEFAULT_MARGIN_LEFT: f64 = 50.0;
const DEFAULT_MARGIN_RIGHT: f64 = 50.0;
const DEFAULT_MARGIN_TOP: f64 = 50.0;
const DEFAULT_MARGIN_BOTTOM: f64 = 50.0;
const DEFAULT_AXIS_SCALE_FACTOR: f64 = 1.0;
const DEFAULT_AXIS_LABEL_FACTOR: f64 = 1.05;
const DEFAULT_CURVE_TENSION: f64 = 0.17;
const DEFAULT_USE_MAX_WIDTH: bool = true;

pub(crate) struct RadarConfigView<'a> {
    radar_config: &'a Value,
}

impl<'a> RadarConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            radar_config: effective_config.get("radar").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> RadarLayoutSettings {
        RadarLayoutSettings {
            width: self.radar_f64("width", DEFAULT_WIDTH),
            height: self.radar_f64("height", DEFAULT_HEIGHT),
            margin_left: self.radar_f64("marginLeft", DEFAULT_MARGIN_LEFT),
            margin_right: self.radar_f64("marginRight", DEFAULT_MARGIN_RIGHT),
            margin_top: self.radar_f64("marginTop", DEFAULT_MARGIN_TOP),
            margin_bottom: self.radar_f64("marginBottom", DEFAULT_MARGIN_BOTTOM),
            axis_scale_factor: self.radar_f64("axisScaleFactor", DEFAULT_AXIS_SCALE_FACTOR),
            axis_label_factor: self.radar_f64("axisLabelFactor", DEFAULT_AXIS_LABEL_FACTOR),
            curve_tension: self.radar_f64("curveTension", DEFAULT_CURVE_TENSION),
        }
    }

    pub(crate) fn render_settings(&self) -> RadarRenderSettings {
        RadarRenderSettings {
            use_max_width: config_bool(self.radar_config, &["useMaxWidth"])
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
        }
    }

    fn radar_f64(&self, key: &str, default: f64) -> f64 {
        config_f64_or(self.radar_config, &[key], default)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RadarLayoutSettings {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) margin_left: f64,
    pub(crate) margin_right: f64,
    pub(crate) margin_top: f64,
    pub(crate) margin_bottom: f64,
    pub(crate) axis_scale_factor: f64,
    pub(crate) axis_label_factor: f64,
    pub(crate) curve_tension: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RadarRenderSettings {
    pub(crate) use_max_width: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn radar_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = RadarConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.width, DEFAULT_WIDTH);
        assert_eq!(settings.height, DEFAULT_HEIGHT);
        assert_eq!(settings.margin_left, DEFAULT_MARGIN_LEFT);
        assert_eq!(settings.margin_right, DEFAULT_MARGIN_RIGHT);
        assert_eq!(settings.margin_top, DEFAULT_MARGIN_TOP);
        assert_eq!(settings.margin_bottom, DEFAULT_MARGIN_BOTTOM);
        assert_eq!(settings.axis_scale_factor, DEFAULT_AXIS_SCALE_FACTOR);
        assert_eq!(settings.axis_label_factor, DEFAULT_AXIS_LABEL_FACTOR);
        assert_eq!(settings.curve_tension, DEFAULT_CURVE_TENSION);
    }

    #[test]
    fn radar_layout_settings_project_configured_values() {
        let cfg = json!({
            "radar": {
                "width": "640",
                "height": 360,
                "marginLeft": "11",
                "marginRight": 12,
                "marginTop": "13",
                "marginBottom": 14,
                "axisScaleFactor": "0.9",
                "axisLabelFactor": 1.2,
                "curveTension": "0.21"
            }
        });
        let settings = RadarConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.width, 640.0);
        assert_eq!(settings.height, 360.0);
        assert_eq!(settings.margin_left, 11.0);
        assert_eq!(settings.margin_right, 12.0);
        assert_eq!(settings.margin_top, 13.0);
        assert_eq!(settings.margin_bottom, 14.0);
        assert_eq!(settings.axis_scale_factor, 0.9);
        assert_eq!(settings.axis_label_factor, 1.2);
        assert_eq!(settings.curve_tension, 0.21);
    }

    #[test]
    fn radar_render_settings_project_use_max_width() {
        let cfg = json!({
            "radar": {
                "useMaxWidth": false
            }
        });
        let settings = RadarConfigView::new(&cfg).render_settings();

        assert!(!settings.use_max_width);
    }
}
