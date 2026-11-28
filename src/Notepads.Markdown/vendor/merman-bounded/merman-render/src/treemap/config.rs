use crate::config::{config_bool, config_f64, config_string};
use serde_json::Value;

const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_PADDING: f64 = 10.0;
const DEFAULT_DIAGRAM_PADDING: f64 = 8.0;
const DEFAULT_SHOW_VALUES: bool = true;
const DEFAULT_NODE_WIDTH: f64 = 100.0;
const DEFAULT_NODE_HEIGHT: f64 = 40.0;
const DEFAULT_VALUE_FORMAT: &str = ",";

pub(crate) struct TreemapConfigView<'a> {
    treemap_config: &'a Value,
}

impl<'a> TreemapConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            treemap_config: effective_config.get("treemap").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> TreemapLayoutSettings {
        // Mermaid treemap defaults live in `defaultConfig.ts`, not in the YAML schema.
        // Keep these in sync with:
        // - `repo-ref/mermaid/packages/mermaid/src/defaultConfig.ts`
        // - `repo-ref/mermaid/packages/mermaid/src/diagrams/treemap/renderer.ts`
        TreemapLayoutSettings {
            use_max_width: self
                .treemap_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
            padding: self
                .treemap_f64("padding")
                .unwrap_or(DEFAULT_PADDING)
                .max(0.0),
            diagram_padding: self
                .treemap_f64("diagramPadding")
                .unwrap_or(DEFAULT_DIAGRAM_PADDING)
                .max(0.0),
            show_values: self
                .treemap_bool("showValues")
                .unwrap_or(DEFAULT_SHOW_VALUES),
            node_width: self.treemap_f64("nodeWidth").unwrap_or(DEFAULT_NODE_WIDTH),
            node_height: self
                .treemap_f64("nodeHeight")
                .unwrap_or(DEFAULT_NODE_HEIGHT),
            value_format: self
                .treemap_string("valueFormat")
                .unwrap_or_else(|| DEFAULT_VALUE_FORMAT.to_string()),
        }
    }

    fn treemap_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.treemap_config, &[key])
    }

    fn treemap_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.treemap_config, &[key])
    }

    fn treemap_string(&self, key: &str) -> Option<String> {
        config_string(self.treemap_config, &[key])
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TreemapLayoutSettings {
    pub(crate) use_max_width: bool,
    pub(crate) padding: f64,
    pub(crate) diagram_padding: f64,
    pub(crate) show_values: bool,
    pub(crate) node_width: f64,
    pub(crate) node_height: f64,
    pub(crate) value_format: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn treemap_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = TreemapConfigView::new(&cfg).layout_settings();

        assert!(settings.use_max_width);
        assert_eq!(settings.padding, DEFAULT_PADDING);
        assert_eq!(settings.diagram_padding, DEFAULT_DIAGRAM_PADDING);
        assert!(settings.show_values);
        assert_eq!(settings.node_width, DEFAULT_NODE_WIDTH);
        assert_eq!(settings.node_height, DEFAULT_NODE_HEIGHT);
        assert_eq!(settings.value_format, DEFAULT_VALUE_FORMAT);
    }

    #[test]
    fn treemap_layout_settings_project_configured_values() {
        let cfg = json!({
            "treemap": {
                "useMaxWidth": false,
                "padding": "12",
                "diagramPadding": 9,
                "showValues": false,
                "nodeWidth": 80,
                "nodeHeight": "50",
                "valueFormat": "$0,0"
            }
        });
        let settings = TreemapConfigView::new(&cfg).layout_settings();

        assert!(!settings.use_max_width);
        assert_eq!(settings.padding, 12.0);
        assert_eq!(settings.diagram_padding, 9.0);
        assert!(!settings.show_values);
        assert_eq!(settings.node_width, 80.0);
        assert_eq!(settings.node_height, 50.0);
        assert_eq!(settings.value_format, "$0,0");
    }

    #[test]
    fn treemap_layout_settings_clamp_padding_but_preserve_node_size_fallback_semantics() {
        let cfg = json!({
            "treemap": {
                "padding": -12,
                "diagramPadding": -9,
                "nodeWidth": -80,
                "nodeHeight": 0
            }
        });
        let settings = TreemapConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.padding, 0.0);
        assert_eq!(settings.diagram_padding, 0.0);
        assert_eq!(settings.node_width, -80.0);
        assert_eq!(settings.node_height, 0.0);
    }
}
