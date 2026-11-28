use crate::config::{config_bool, config_f64};
use serde_json::Value;

const DEFAULT_PADDING: f64 = 30.0;
const DEFAULT_USE_MAX_WIDTH: bool = true;

pub(crate) struct EventModelingConfigView<'a> {
    eventmodeling_config: &'a Value,
}

impl<'a> EventModelingConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            eventmodeling_config: effective_config
                .get("eventmodeling")
                .unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> EventModelingLayoutSettings {
        EventModelingLayoutSettings {
            padding: self
                .eventmodeling_f64("padding")
                .unwrap_or(DEFAULT_PADDING)
                .max(0.0),
            use_max_width: self
                .eventmodeling_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
        }
    }

    fn eventmodeling_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.eventmodeling_config, &[key])
    }

    fn eventmodeling_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.eventmodeling_config, &[key])
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EventModelingLayoutSettings {
    pub(crate) padding: f64,
    pub(crate) use_max_width: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn eventmodeling_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = EventModelingConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.padding, DEFAULT_PADDING);
        assert!(settings.use_max_width);
    }

    #[test]
    fn eventmodeling_layout_settings_project_configured_values() {
        let cfg = json!({
            "eventmodeling": {
                "padding": "12",
                "useMaxWidth": false
            }
        });
        let settings = EventModelingConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.padding, 12.0);
        assert!(!settings.use_max_width);
    }

    #[test]
    fn eventmodeling_layout_settings_clamp_negative_padding() {
        let cfg = json!({
            "eventmodeling": {
                "padding": -8
            }
        });
        let settings = EventModelingConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.padding, 0.0);
    }
}
