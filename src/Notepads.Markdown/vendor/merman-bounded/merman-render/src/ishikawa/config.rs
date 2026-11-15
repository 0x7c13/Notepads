use crate::config::{config_bool, config_css_number_or_string, config_f64, config_f64_css_px};
use serde_json::Value;

const DEFAULT_PADDING: f64 = 20.0;
const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_FONT_SIZE: f64 = 14.0;

pub(crate) struct IshikawaConfigView<'a> {
    effective_config: &'a Value,
    ishikawa_config: &'a Value,
}

impl<'a> IshikawaConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            ishikawa_config: effective_config.get("ishikawa").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> IshikawaLayoutSettings {
        IshikawaLayoutSettings {
            padding: self
                .ishikawa_f64("diagramPadding")
                .unwrap_or(DEFAULT_PADDING)
                .max(0.0),
            use_max_width: self
                .ishikawa_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
            font_size: config_f64_css_px(self.effective_config, &["fontSize"])
                .unwrap_or(DEFAULT_FONT_SIZE)
                .max(1.0),
        }
    }

    pub(crate) fn render_settings(&self) -> IshikawaRenderSettings {
        IshikawaRenderSettings {
            font_size_css: config_css_number_or_string(self.effective_config, &["fontSize"]),
        }
    }

    fn ishikawa_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.ishikawa_config, &[key])
    }

    fn ishikawa_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.ishikawa_config, &[key])
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IshikawaLayoutSettings {
    pub(crate) padding: f64,
    pub(crate) use_max_width: bool,
    pub(crate) font_size: f64,
}

pub(crate) struct IshikawaRenderSettings {
    pub(crate) font_size_css: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ishikawa_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = IshikawaConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.padding, DEFAULT_PADDING);
        assert!(settings.use_max_width);
        assert_eq!(settings.font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn ishikawa_layout_settings_project_configured_values() {
        let cfg = json!({
            "fontSize": "18px",
            "ishikawa": {
                "diagramPadding": "12",
                "useMaxWidth": false
            }
        });
        let settings = IshikawaConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.padding, 12.0);
        assert!(!settings.use_max_width);
        assert_eq!(settings.font_size, 18.0);
    }

    #[test]
    fn ishikawa_layout_settings_clamp_geometry() {
        let cfg = json!({
            "fontSize": 0,
            "ishikawa": {
                "diagramPadding": -12
            }
        });
        let settings = IshikawaConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.padding, 0.0);
        assert_eq!(settings.font_size, 1.0);
    }

    #[test]
    fn ishikawa_render_settings_preserve_css_font_size_spelling() {
        let cfg = json!({
            "fontSize": "18px !important;"
        });
        let settings = IshikawaConfigView::new(&cfg).render_settings();

        assert_eq!(settings.font_size_css.as_deref(), Some("18px !important"));
    }
}
