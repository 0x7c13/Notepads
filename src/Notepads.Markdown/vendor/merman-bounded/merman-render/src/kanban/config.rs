use crate::config::{
    DiagramLook, config_bool, config_diagram_look, config_f64, config_string,
    config_theme_or_root_font_size_px,
};
use crate::text::TextStyle;
use serde_json::Value;

const DEFAULT_SECTION_WIDTH: f64 = 200.0;
const DEFAULT_VIEWBOX_PADDING: f64 = 8.0;
const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial, sans-serif";
const DEFAULT_FONT_SIZE: f64 = 16.0;

pub(crate) struct KanbanConfigView<'a> {
    effective_config: &'a Value,
    kanban_config: &'a Value,
    mindmap_config: &'a Value,
}

impl<'a> KanbanConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            kanban_config: effective_config.get("kanban").unwrap_or(&Value::Null),
            mindmap_config: effective_config.get("mindmap").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> KanbanLayoutSettings {
        KanbanLayoutSettings {
            section_width: self.section_width(),
            viewbox_padding: self.viewbox_padding(),
            use_max_width: self.use_max_width(),
            text_style: self.text_style(),
        }
    }

    pub(crate) fn look(&self) -> DiagramLook<'a> {
        config_diagram_look(self.effective_config)
    }

    pub(crate) fn ticket_base_url(&self) -> Option<String> {
        config_string(self.kanban_config, &["ticketBaseUrl"]).filter(|url| !url.is_empty())
    }

    fn section_width(&self) -> f64 {
        let width =
            config_f64(self.kanban_config, &["sectionWidth"]).unwrap_or(DEFAULT_SECTION_WIDTH);
        if width == 0.0 {
            DEFAULT_SECTION_WIDTH
        } else {
            width.max(1.0)
        }
    }

    fn viewbox_padding(&self) -> f64 {
        config_f64(self.mindmap_config, &["padding"])
            .or_else(|| config_f64(self.kanban_config, &["padding"]))
            .unwrap_or(DEFAULT_VIEWBOX_PADDING)
            .max(0.0)
    }

    fn use_max_width(&self) -> bool {
        config_bool(self.mindmap_config, &["useMaxWidth"])
            .or_else(|| config_bool(self.kanban_config, &["useMaxWidth"]))
            .unwrap_or(DEFAULT_USE_MAX_WIDTH)
    }

    fn text_style(&self) -> TextStyle {
        let font_family = config_string(self.effective_config, &["fontFamily"])
            .or_else(|| config_string(self.effective_config, &["themeVariables", "fontFamily"]))
            .unwrap_or_else(|| DEFAULT_FONT_FAMILY.to_string());
        TextStyle {
            font_family: Some(font_family),
            font_size: config_theme_or_root_font_size_px(self.effective_config, DEFAULT_FONT_SIZE)
                .max(1.0),
            font_weight: None,
            font_style: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct KanbanLayoutSettings {
    pub(crate) section_width: f64,
    pub(crate) viewbox_padding: f64,
    pub(crate) use_max_width: bool,
    pub(crate) text_style: TextStyle,
}

pub(crate) const fn default_use_max_width() -> bool {
    DEFAULT_USE_MAX_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kanban_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = KanbanConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.section_width, DEFAULT_SECTION_WIDTH);
        assert_eq!(settings.viewbox_padding, DEFAULT_VIEWBOX_PADDING);
        assert!(settings.use_max_width);
        assert_eq!(settings.text_style.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(
            settings.text_style.font_family.as_deref(),
            Some(DEFAULT_FONT_FAMILY)
        );
    }

    #[test]
    fn kanban_layout_settings_project_geometry_and_font() {
        let cfg = json!({
            "fontFamily": "Inter, sans-serif",
            "fontSize": "20px",
            "kanban": {
                "sectionWidth": "240",
                "padding": "12",
                "useMaxWidth": false
            }
        });
        let settings = KanbanConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.section_width, 240.0);
        assert_eq!(settings.viewbox_padding, 12.0);
        assert!(!settings.use_max_width);
        assert_eq!(
            settings.text_style.font_family.as_deref(),
            Some("Inter, sans-serif")
        );
        assert_eq!(settings.text_style.font_size, 20.0);
    }

    #[test]
    fn kanban_layout_settings_mirror_mindmap_viewport_precedence() {
        let cfg = json!({
            "mindmap": {
                "padding": 5,
                "useMaxWidth": false
            },
            "kanban": {
                "padding": 12,
                "useMaxWidth": true
            }
        });
        let settings = KanbanConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.viewbox_padding, 5.0);
        assert!(!settings.use_max_width);
    }

    #[test]
    fn kanban_layout_settings_zero_section_width_falls_back_like_mermaid() {
        let cfg = json!({
            "kanban": {
                "sectionWidth": 0
            }
        });
        let settings = KanbanConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.section_width, DEFAULT_SECTION_WIDTH);
    }

    #[test]
    fn kanban_config_projects_look_and_ticket_base_url() {
        let cfg = json!({
            "look": "neo",
            "kanban": {
                "ticketBaseUrl": "https://example.invalid/#TICKET#"
            }
        });
        let config = KanbanConfigView::new(&cfg);

        assert_eq!(config.look().as_str(), "neo");
        assert_eq!(
            config.ticket_base_url().as_deref(),
            Some("https://example.invalid/#TICKET#")
        );

        let whitespace_cfg = json!({
            "kanban": {
                "ticketBaseUrl": "   "
            }
        });
        assert_eq!(
            KanbanConfigView::new(&whitespace_cfg)
                .ticket_base_url()
                .as_deref(),
            Some("   ")
        );
    }
}
