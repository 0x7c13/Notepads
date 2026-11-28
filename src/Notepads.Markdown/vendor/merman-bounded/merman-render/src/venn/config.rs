use crate::config::{config_bool, config_f64};
use serde_json::Value;

pub(super) const DEFAULT_SVG_WIDTH: f64 = 800.0;
pub(super) const DEFAULT_SVG_HEIGHT: f64 = 450.0;
pub(super) const DEFAULT_PADDING: f64 = 15.0;

const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_USE_DEBUG_LAYOUT: bool = false;

pub(crate) struct VennConfigView<'a> {
    venn_config: &'a Value,
}

impl<'a> VennConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            venn_config: effective_config.get("venn").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> VennLayoutSettings {
        VennLayoutSettings {
            width: self.venn_f64("width").unwrap_or(DEFAULT_SVG_WIDTH).max(1.0),
            height: self
                .venn_f64("height")
                .unwrap_or(DEFAULT_SVG_HEIGHT)
                .max(1.0),
            padding: self.venn_f64("padding").unwrap_or(DEFAULT_PADDING).max(0.0),
            use_max_width: self
                .venn_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
            use_debug_layout: self
                .venn_bool("useDebugLayout")
                .unwrap_or(DEFAULT_USE_DEBUG_LAYOUT),
        }
    }

    fn venn_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.venn_config, &[key])
    }

    fn venn_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.venn_config, &[key])
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VennLayoutSettings {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) padding: f64,
    pub(crate) use_max_width: bool,
    pub(crate) use_debug_layout: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn venn_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = VennConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.width, DEFAULT_SVG_WIDTH);
        assert_eq!(settings.height, DEFAULT_SVG_HEIGHT);
        assert_eq!(settings.padding, DEFAULT_PADDING);
        assert!(settings.use_max_width);
        assert!(!settings.use_debug_layout);
    }

    #[test]
    fn venn_layout_settings_project_configured_values() {
        let cfg = json!({
            "venn": {
                "width": "640",
                "height": 360,
                "padding": 20,
                "useMaxWidth": false,
                "useDebugLayout": true
            }
        });
        let settings = VennConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.width, 640.0);
        assert_eq!(settings.height, 360.0);
        assert_eq!(settings.padding, 20.0);
        assert!(!settings.use_max_width);
        assert!(settings.use_debug_layout);
    }

    #[test]
    fn venn_layout_settings_clamp_geometry() {
        let cfg = json!({
            "venn": {
                "width": -640,
                "height": 0,
                "padding": -20
            }
        });
        let settings = VennConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.width, 1.0);
        assert_eq!(settings.height, 1.0);
        assert_eq!(settings.padding, 0.0);
    }
}
