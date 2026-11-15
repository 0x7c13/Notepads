use crate::config::{
    DiagramLook, MERMAID_DEFAULT_FONT_FAMILY_CSS, config_bool, config_diagram_look, config_f64,
    config_f64_css_px, config_font_family_or_first_array_css, config_string_or_first_array,
    config_theme_or_root_font_size_px, normalize_css_font_family,
};
use serde_json::Value;

const DEFAULT_NODE_SPACING: f64 = 50.0;
const DEFAULT_RANK_SPACING: f64 = 50.0;
const DEFAULT_FONT_SIZE: f64 = 16.0;
const DEFAULT_VIEWPORT_PADDING: f64 = 8.0;
const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_TITLE_TOP_MARGIN: f64 = 25.0;

pub(crate) struct RequirementConfigView<'a> {
    effective_config: &'a Value,
    requirement_config: &'a Value,
    state_config: &'a Value,
}

impl<'a> RequirementConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            requirement_config: effective_config.get("requirement").unwrap_or(&Value::Null),
            state_config: effective_config.get("state").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> RequirementLayoutSettings {
        RequirementLayoutSettings {
            nodesep: self.layout_spacing("nodeSpacing", DEFAULT_NODE_SPACING),
            ranksep: self.layout_spacing("rankSpacing", DEFAULT_RANK_SPACING),
            font_family: self.font_family(),
            font_size: self.font_size(),
            calculation_font_family: self.calculation_font_family(),
            calculation_font_size: self.calculation_font_size(),
            html_labels: config_bool(self.effective_config, &["htmlLabels"]).unwrap_or(true),
            edge_html_labels: crate::config::config_effective_html_labels(self.effective_config),
        }
    }

    pub(crate) fn render_settings(&self) -> RequirementRenderSettings<'a> {
        RequirementRenderSettings {
            look: config_diagram_look(self.effective_config),
            html_labels: config_bool(self.effective_config, &["htmlLabels"]).unwrap_or(true),
            edge_html_labels: crate::config::config_effective_html_labels(self.effective_config),
            viewport_padding: DEFAULT_VIEWPORT_PADDING,
            use_max_width: self
                .requirement_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
            hand_drawn_seed: self
                .effective_config
                .get("handDrawnSeed")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            title_top_margin: self
                .config_f64(&["state", "titleTopMargin"])
                .unwrap_or(DEFAULT_TITLE_TOP_MARGIN),
            font_family: self.font_family(),
            font_size: self.font_size(),
        }
    }

    fn config_f64(&self, path: &[&str]) -> Option<f64> {
        config_f64(self.effective_config, path)
    }

    fn root_f64(&self, key: &str) -> Option<f64> {
        self.config_f64(&[key])
    }

    fn state_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.state_config, &[key])
    }

    fn layout_spacing(&self, key: &str, default: f64) -> f64 {
        let renderer_spacing = self.state_f64(key).unwrap_or(default);
        [
            self.root_f64(key),
            Some(renderer_spacing),
            self.config_f64(&["flowchart", key]),
        ]
        .into_iter()
        .flatten()
        .find(|value| *value != 0.0)
        .unwrap_or(default)
    }

    fn requirement_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.requirement_config, &[key])
    }

    fn font_family(&self) -> String {
        config_font_family_or_first_array_css(self.effective_config)
    }

    fn font_size(&self) -> f64 {
        config_theme_or_root_font_size_px(self.effective_config, DEFAULT_FONT_SIZE)
    }

    fn calculation_font_family(&self) -> String {
        let raw = config_string_or_first_array(self.effective_config, &["fontFamily"])
            .unwrap_or_else(|| MERMAID_DEFAULT_FONT_FAMILY_CSS.to_string());
        let normalized = normalize_css_font_family(&raw);
        if normalized.is_empty() {
            MERMAID_DEFAULT_FONT_FAMILY_CSS.to_string()
        } else {
            normalized
        }
    }

    fn calculation_font_size(&self) -> f64 {
        config_f64_css_px(self.effective_config, &["fontSize"]).unwrap_or(DEFAULT_FONT_SIZE)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RequirementLayoutSettings {
    pub(crate) nodesep: f64,
    pub(crate) ranksep: f64,
    pub(crate) font_family: String,
    pub(crate) font_size: f64,
    pub(crate) calculation_font_family: String,
    pub(crate) calculation_font_size: f64,
    pub(crate) html_labels: bool,
    pub(crate) edge_html_labels: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RequirementRenderSettings<'a> {
    pub(crate) look: DiagramLook<'a>,
    pub(crate) html_labels: bool,
    pub(crate) edge_html_labels: bool,
    pub(crate) viewport_padding: f64,
    pub(crate) use_max_width: bool,
    pub(crate) hand_drawn_seed: f64,
    pub(crate) title_top_margin: f64,
    pub(crate) font_family: String,
    pub(crate) font_size: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requirement_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = RequirementConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.nodesep, DEFAULT_NODE_SPACING);
        assert_eq!(settings.ranksep, DEFAULT_RANK_SPACING);
        assert_eq!(
            settings.font_family,
            crate::config::MERMAID_DEFAULT_FONT_FAMILY_CSS
        );
        assert_eq!(settings.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(
            settings.calculation_font_family,
            crate::config::MERMAID_DEFAULT_FONT_FAMILY_CSS
        );
        assert_eq!(settings.calculation_font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn requirement_layout_settings_keep_root_spacing_precedence() {
        let cfg = json!({
            "nodeSpacing": "0",
            "rankSpacing": 70,
            "state": {
                "nodeSpacing": 0,
                "rankSpacing": 60
            },
            "flowchart": {
                "nodeSpacing": 11,
                "rankSpacing": 12
            },
            "themeVariables": {
                "fontFamily": ["Courier New", "serif"],
                "fontSize": "18px"
            }
        });
        let settings = RequirementConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.nodesep, 11.0);
        assert_eq!(settings.ranksep, 70.0);
        assert_eq!(settings.font_family, "Courier New");
        assert_eq!(settings.font_size, 18.0);
        assert_eq!(
            settings.calculation_font_family,
            crate::config::MERMAID_DEFAULT_FONT_FAMILY_CSS
        );
        assert_eq!(settings.calculation_font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn requirement_layout_settings_default_renderer_spacing_precedes_flowchart_spacing() {
        let cfg = json!({
            "flowchart": {
                "nodeSpacing": "33",
                "rankSpacing": 44
            }
        });
        let settings = RequirementConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.nodesep, DEFAULT_NODE_SPACING);
        assert_eq!(settings.ranksep, DEFAULT_RANK_SPACING);
    }

    #[test]
    fn requirement_layout_settings_use_flowchart_spacing_when_renderer_spacing_is_zero() {
        let cfg = json!({
            "state": {
                "nodeSpacing": 0,
                "rankSpacing": 0
            },
            "flowchart": {
                "nodeSpacing": "33",
                "rankSpacing": 44
            }
        });
        let settings = RequirementConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.nodesep, 33.0);
        assert_eq!(settings.ranksep, 44.0);
    }

    #[test]
    fn requirement_render_settings_project_values() {
        let cfg = json!({
            "look": "neo",
            "handDrawnSeed": 7,
            "fontFamily": "Inter, sans-serif",
            "fontSize": "20px",
            "requirement": {
                "useMaxWidth": false
            }
        });
        let settings = RequirementConfigView::new(&cfg).render_settings();

        assert_eq!(settings.look.as_str(), "neo");
        assert_eq!(settings.viewport_padding, DEFAULT_VIEWPORT_PADDING);
        assert!(!settings.use_max_width);
        assert_eq!(settings.hand_drawn_seed, 7.0);
        assert_eq!(settings.title_top_margin, DEFAULT_TITLE_TOP_MARGIN);
        assert_eq!(settings.font_family, "Inter,sans-serif");
        assert_eq!(settings.font_size, 20.0);
    }

    #[test]
    fn requirement_title_margin_uses_the_state_config_namespace() {
        let cfg = json!({
            "titleTopMargin": 91,
            "requirement": { "titleTopMargin": 72 },
            "state": { "titleTopMargin": 33 }
        });

        let settings = RequirementConfigView::new(&cfg).render_settings();

        assert_eq!(settings.title_top_margin, 33.0);
    }

    #[test]
    fn requirement_text_calculation_keeps_root_font_size_separate_from_theme_font_size() {
        let cfg = json!({
            "fontFamily": "Trebuchet MS, sans-serif",
            "fontSize": 10,
            "themeVariables": {
                "fontFamily": "Courier New, monospace",
                "fontSize": "24px"
            }
        });
        let settings = RequirementConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.font_family, "Courier New,monospace");
        assert_eq!(settings.font_size, 24.0);
        assert_eq!(settings.calculation_font_family, "Trebuchet MS,sans-serif");
        assert_eq!(settings.calculation_font_size, 10.0);
    }
    #[test]
    fn requirement_html_label_modes_follow_node_and_edge_source_precedence() {
        for (config, node_html, edge_html) in [
            (
                json!({"htmlLabels": false, "flowchart": {"htmlLabels": true}}),
                false,
                false,
            ),
            (
                json!({"htmlLabels": true, "flowchart": {"htmlLabels": false}}),
                true,
                true,
            ),
            (json!({"flowchart": {"htmlLabels": false}}), true, false),
        ] {
            let view = RequirementConfigView::new(&config);
            let layout = view.layout_settings();
            let render = view.render_settings();
            assert_eq!(layout.html_labels, node_html);
            assert_eq!(render.html_labels, node_html);
            assert_eq!(layout.edge_html_labels, edge_html);
            assert_eq!(render.edge_html_labels, edge_html);
        }
    }
}
