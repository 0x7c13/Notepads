use crate::config::{
    config_bool, config_f64, config_f64_css_px, config_string, config_theme_or_root_font_size_px,
};
use crate::text::TextStyle;
use serde_json::Value;

const DEFAULT_LEFT_MARGIN: f64 = 150.0;
const DEFAULT_VIEWBOX_PADDING: f64 = 50.0;
const DEFAULT_USE_MAX_WIDTH: bool = false;
const DEFAULT_FONT_SIZE: f64 = 16.0;

pub(crate) struct TimelineConfigView<'a> {
    effective_config: &'a Value,
    timeline_config: &'a Value,
}

impl<'a> TimelineConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            timeline_config: effective_config.get("timeline").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> TimelineLayoutSettings {
        let text_style = self.text_style();
        TimelineLayoutSettings {
            text_style: text_style.clone(),
            layout_font_size: config_f64_css_px(self.effective_config, &["fontSize"])
                .unwrap_or(text_style.font_size)
                .max(1.0),
            left_margin: config_f64(self.timeline_config, &["leftMargin"])
                .unwrap_or(DEFAULT_LEFT_MARGIN)
                .max(0.0),
            disable_multicolor: config_bool(self.timeline_config, &["disableMulticolor"])
                .unwrap_or(false),
            viewbox_padding: config_f64(self.timeline_config, &["padding"])
                .unwrap_or(DEFAULT_VIEWBOX_PADDING)
                .max(0.0),
            use_max_width: config_bool(self.timeline_config, &["useMaxWidth"])
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
        }
    }

    fn text_style(&self) -> TextStyle {
        TextStyle {
            font_family: self.font_family(),
            font_size: config_theme_or_root_font_size_px(self.effective_config, DEFAULT_FONT_SIZE)
                .max(1.0),
            font_weight: None,
            font_style: None,
        }
    }

    fn font_family(&self) -> Option<String> {
        config_string(self.effective_config, &["themeVariables", "fontFamily"])
            .or_else(|| config_string(self.effective_config, &["fontFamily"]))
            .map(|s| s.trim().trim_end_matches(';').trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TimelineLayoutSettings {
    pub(crate) text_style: TextStyle,
    pub(crate) layout_font_size: f64,
    pub(crate) left_margin: f64,
    pub(crate) disable_multicolor: bool,
    pub(crate) viewbox_padding: f64,
    pub(crate) use_max_width: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timeline_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = TimelineConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.left_margin, DEFAULT_LEFT_MARGIN);
        assert_eq!(settings.viewbox_padding, DEFAULT_VIEWBOX_PADDING);
        assert!(!settings.disable_multicolor);
        assert!(!settings.use_max_width);
        assert_eq!(settings.text_style.font_family, None);
        assert_eq!(settings.text_style.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(settings.layout_font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn timeline_layout_settings_project_configured_values() {
        let cfg = json!({
            "fontFamily": "Inter, sans-serif",
            "themeVariables": {
                "fontSize": "20px"
            },
            "fontSize": "18px",
            "timeline": {
                "leftMargin": "180",
                "disableMulticolor": true,
                "padding": "12",
                "useMaxWidth": true
            }
        });
        let settings = TimelineConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.left_margin, 180.0);
        assert_eq!(settings.viewbox_padding, 12.0);
        assert!(settings.disable_multicolor);
        assert!(settings.use_max_width);
        assert_eq!(
            settings.text_style.font_family.as_deref(),
            Some("Inter, sans-serif")
        );
        assert_eq!(settings.text_style.font_size, 20.0);
        assert_eq!(settings.layout_font_size, 18.0);
    }
}
