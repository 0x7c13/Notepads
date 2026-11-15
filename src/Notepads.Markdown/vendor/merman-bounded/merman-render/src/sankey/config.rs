use crate::config::{config_bool, config_f64, config_string};
use serde_json::{Map, Value};

pub(super) const DEFAULT_NODE_WIDTH_PX: f64 = 10.0;
pub(super) const DEFAULT_NODE_PADDING_BASE_PX: f64 = 12.0;
pub(super) const NODE_PADDING_SHOW_VALUES_EXTRA_PX: f64 = 15.0;

const DEFAULT_WIDTH: f64 = 600.0;
const DEFAULT_HEIGHT: f64 = 400.0;
const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_SHOW_VALUES: bool = true;
const DEFAULT_LINK_COLOR: &str = "gradient";
const DEFAULT_LABEL_STYLE: &str = "legacy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeAlign {
    Left,
    Right,
    Justify,
    Center,
}

pub(crate) struct SankeyConfigView<'a> {
    effective_config: &'a Value,
    sankey_config: &'a Value,
    has_sankey_config: bool,
}

impl<'a> SankeyConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        let sankey_config = effective_config.get("sankey").unwrap_or(&Value::Null);
        Self {
            effective_config,
            sankey_config,
            has_sankey_config: effective_config
                .get("sankey")
                .is_some_and(|cfg| !has_ref_object(cfg)),
        }
    }

    pub(crate) fn layout_settings(&self) -> SankeyLayoutSettings {
        let show_values = self.show_values();
        let node_padding_base = self
            .configured_f64("nodePadding")
            .unwrap_or(DEFAULT_NODE_PADDING_BASE_PX);
        SankeyLayoutSettings {
            width: self.root_f64("width").unwrap_or(DEFAULT_WIDTH),
            height: self.root_f64("height").unwrap_or(DEFAULT_HEIGHT),
            node_align: self.node_align(),
            node_width: self
                .configured_f64("nodeWidth")
                .unwrap_or(DEFAULT_NODE_WIDTH_PX),
            node_padding: sankey_node_padding_px_with_base(node_padding_base, show_values),
        }
    }

    pub(crate) fn render_settings(&self) -> SankeyRenderSettings<'a> {
        SankeyRenderSettings {
            use_max_width: self
                .configured_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
            show_values: self.show_values(),
            prefix: self.configured_string("prefix").unwrap_or_default(),
            suffix: self.configured_string("suffix").unwrap_or_default(),
            link_color: self
                .configured_string("linkColor")
                .unwrap_or_else(|| DEFAULT_LINK_COLOR.to_string()),
            outlined_labels: self
                .configured_string("labelStyle")
                .unwrap_or_else(|| DEFAULT_LABEL_STYLE.to_string())
                == "outlined",
            node_colors: self.configured_object("nodeColors"),
        }
    }

    fn show_values(&self) -> bool {
        self.configured_bool("showValues")
            .unwrap_or(DEFAULT_SHOW_VALUES)
    }

    fn node_align(&self) -> NodeAlign {
        match self.configured_string("nodeAlignment").as_deref() {
            Some("left") => NodeAlign::Left,
            Some("right") => NodeAlign::Right,
            Some("center") => NodeAlign::Center,
            _ => NodeAlign::Justify,
        }
    }

    fn root_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.effective_config, &["sankey", key])
    }

    fn configured_bool(&self, key: &str) -> Option<bool> {
        self.has_sankey_config
            .then(|| config_bool(self.sankey_config, &[key]))
            .flatten()
    }

    fn configured_f64(&self, key: &str) -> Option<f64> {
        self.has_sankey_config
            .then(|| config_f64(self.sankey_config, &[key]))
            .flatten()
    }

    fn configured_string(&self, key: &str) -> Option<String> {
        self.has_sankey_config
            .then(|| config_string(self.sankey_config, &[key]))
            .flatten()
    }

    fn configured_object(&self, key: &str) -> Option<&'a Map<String, Value>> {
        self.has_sankey_config
            .then(|| self.sankey_config.get(key)?.as_object())
            .flatten()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SankeyLayoutSettings {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(super) node_align: NodeAlign,
    pub(crate) node_width: f64,
    pub(crate) node_padding: f64,
}

pub(crate) struct SankeyRenderSettings<'a> {
    pub(crate) use_max_width: bool,
    pub(crate) show_values: bool,
    pub(crate) prefix: String,
    pub(crate) suffix: String,
    pub(crate) link_color: String,
    pub(crate) outlined_labels: bool,
    pub(crate) node_colors: Option<&'a Map<String, Value>>,
}

pub(super) fn sankey_node_padding_px_with_base(base: f64, show_values: bool) -> f64 {
    base + if show_values {
        NODE_PADDING_SHOW_VALUES_EXTRA_PX
    } else {
        0.0
    }
}

fn has_ref_object(v: &Value) -> bool {
    v.as_object().is_some_and(|m| m.contains_key("$ref"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sankey_layout_settings_preserve_defaults_and_node_padding_extra() {
        let cfg = json!({});
        let settings = SankeyConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.width, 600.0);
        assert_eq!(settings.height, 400.0);
        assert_eq!(settings.node_align, NodeAlign::Justify);
        assert_eq!(settings.node_width, DEFAULT_NODE_WIDTH_PX);
        assert_eq!(
            settings.node_padding,
            sankey_node_padding_px_with_base(DEFAULT_NODE_PADDING_BASE_PX, true)
        );
    }

    #[test]
    fn sankey_layout_settings_project_configured_geometry() {
        let cfg = json!({
            "sankey": {
                "width": 700,
                "height": "500",
                "showValues": false,
                "nodeAlignment": "center",
                "nodeWidth": 24,
                "nodePadding": 18
            }
        });
        let settings = SankeyConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.width, 700.0);
        assert_eq!(settings.height, 500.0);
        assert_eq!(settings.node_align, NodeAlign::Center);
        assert_eq!(settings.node_width, 24.0);
        assert_eq!(settings.node_padding, 18.0);
    }

    #[test]
    fn sankey_render_settings_project_labels_links_and_colors() {
        let cfg = json!({
            "sankey": {
                "useMaxWidth": false,
                "showValues": false,
                "prefix": "$",
                "suffix": " USD",
                "linkColor": "source",
                "labelStyle": "outlined",
                "nodeColors": {
                    "A": "#112233"
                }
            }
        });
        let settings = SankeyConfigView::new(&cfg).render_settings();

        assert!(!settings.use_max_width);
        assert!(!settings.show_values);
        assert_eq!(settings.prefix, "$");
        assert_eq!(settings.suffix, " USD");
        assert_eq!(settings.link_color, "source");
        assert!(settings.outlined_labels);
        assert_eq!(
            settings
                .node_colors
                .and_then(|colors| colors.get("A"))
                .and_then(Value::as_str),
            Some("#112233")
        );
    }

    #[test]
    fn sankey_ref_config_uses_legacy_defaults_instead_of_child_values() {
        let cfg = json!({
            "sankey": {
                "$ref": "#/defs/sankey",
                "useMaxWidth": false,
                "showValues": false,
                "nodeWidth": 24,
                "nodePadding": 18,
                "prefix": "$",
                "linkColor": "source",
                "labelStyle": "outlined",
                "nodeColors": { "A": "#112233" }
            }
        });

        let layout = SankeyConfigView::new(&cfg).layout_settings();
        let render = SankeyConfigView::new(&cfg).render_settings();

        assert_eq!(layout.node_width, DEFAULT_NODE_WIDTH_PX);
        assert_eq!(
            layout.node_padding,
            sankey_node_padding_px_with_base(DEFAULT_NODE_PADDING_BASE_PX, true)
        );
        assert!(render.use_max_width);
        assert!(render.show_values);
        assert_eq!(render.prefix, "");
        assert_eq!(render.link_color, DEFAULT_LINK_COLOR);
        assert!(!render.outlined_labels);
        assert!(render.node_colors.is_none());
    }
}
