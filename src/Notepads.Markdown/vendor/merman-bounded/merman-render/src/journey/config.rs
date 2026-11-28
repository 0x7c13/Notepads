use crate::config::{config_bool, config_f64, config_string, config_string_vec};
use crate::text::TextStyle;
use serde_json::Value;

const DEFAULT_LEFT_MARGIN: f64 = 150.0;
const DEFAULT_MAX_LABEL_WIDTH: f64 = 360.0;
const DEFAULT_BOX_TEXT_MARGIN: f64 = 5.0;
const DEFAULT_DIAGRAM_MARGIN_X: f64 = 50.0;
const DEFAULT_DIAGRAM_MARGIN_Y: f64 = 10.0;
const DEFAULT_TASK_MARGIN: f64 = 50.0;
const DEFAULT_CELL_WIDTH: f64 = 150.0;
const DEFAULT_CELL_HEIGHT: f64 = 50.0;
const DEFAULT_TASK_FONT_SIZE: f64 = 14.0;
const DEFAULT_TASK_FONT_FAMILY: &str = "\"Open Sans\", sans-serif";
const DEFAULT_TITLE_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial, sans-serif";
const DEFAULT_TITLE_FONT_SIZE: &str = "4ex";
const DEFAULT_TITLE_COLOR: &str = "";
const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_ACTOR_COLOURS: [&str; 6] = [
    "#8FBC8F", "#7CFC00", "#00FFFF", "#20B2AA", "#B0E0E6", "#FFFFE0",
];
const DEFAULT_SECTION_FILLS: [&str; 7] = [
    "#191970", "#8B008B", "#4B0082", "#2F4F4F", "#800000", "#8B4513", "#00008B",
];

pub(crate) struct JourneyConfigView<'a> {
    journey_config: &'a Value,
}

impl<'a> JourneyConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            journey_config: effective_config.get("journey").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> JourneyLayoutSettings {
        JourneyLayoutSettings {
            left_margin_base: self
                .journey_f64("leftMargin")
                .unwrap_or(DEFAULT_LEFT_MARGIN)
                .max(0.0),
            max_label_width: self
                .journey_f64("maxLabelWidth")
                .unwrap_or(DEFAULT_MAX_LABEL_WIDTH)
                .max(1.0),
            box_text_margin: self
                .journey_f64("boxTextMargin")
                .unwrap_or(DEFAULT_BOX_TEXT_MARGIN)
                .max(0.0),
            diagram_margin_x: self
                .journey_f64("diagramMarginX")
                .unwrap_or(DEFAULT_DIAGRAM_MARGIN_X)
                .max(0.0),
            diagram_margin_y: self
                .journey_f64("diagramMarginY")
                .unwrap_or(DEFAULT_DIAGRAM_MARGIN_Y)
                .max(0.0),
            task_margin: self
                .journey_f64("taskMargin")
                .unwrap_or(DEFAULT_TASK_MARGIN)
                .max(0.0),
            cell_width: self
                .journey_f64("width")
                .unwrap_or(DEFAULT_CELL_WIDTH)
                .max(1.0),
            cell_height: self
                .journey_f64("height")
                .unwrap_or(DEFAULT_CELL_HEIGHT)
                .max(1.0),
            actor_colours: self.actor_colours(),
            section_fills: self.section_fills(),
            use_max_width: self.use_max_width(),
        }
    }

    pub(crate) fn render_settings(&self) -> JourneyRenderSettings {
        JourneyRenderSettings {
            task_text_style: self.task_text_style(),
            title_font_size: config_string(self.journey_config, &["titleFontSize"])
                .unwrap_or_else(|| DEFAULT_TITLE_FONT_SIZE.to_string()),
            title_font_family: config_string(self.journey_config, &["titleFontFamily"])
                .unwrap_or_else(|| DEFAULT_TITLE_FONT_FAMILY.to_string()),
            title_color: config_string(self.journey_config, &["titleColor"])
                .unwrap_or_else(|| DEFAULT_TITLE_COLOR.to_string()),
        }
    }

    fn journey_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.journey_config, &[key])
    }

    fn journey_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.journey_config, &[key])
    }

    fn actor_colours(&self) -> Vec<String> {
        let actor_colours = config_string_vec(self.journey_config, &["actorColours"]);
        if actor_colours.is_empty() {
            DEFAULT_ACTOR_COLOURS
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            actor_colours
        }
    }

    fn section_fills(&self) -> Vec<String> {
        let section_fills = config_string_vec(self.journey_config, &["sectionFills"]);
        if section_fills.is_empty() {
            DEFAULT_SECTION_FILLS
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            section_fills
        }
    }

    fn task_text_style(&self) -> TextStyle {
        TextStyle {
            font_family: Some(
                config_string(self.journey_config, &["taskFontFamily"])
                    .unwrap_or_else(|| DEFAULT_TASK_FONT_FAMILY.to_string()),
            ),
            font_size: self
                .journey_f64("taskFontSize")
                .unwrap_or(DEFAULT_TASK_FONT_SIZE)
                .max(1.0),
            font_weight: None,
            font_style: None,
        }
    }

    fn use_max_width(&self) -> bool {
        self.journey_bool("useMaxWidth")
            .unwrap_or(DEFAULT_USE_MAX_WIDTH)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct JourneyLayoutSettings {
    pub(crate) left_margin_base: f64,
    pub(crate) max_label_width: f64,
    pub(crate) box_text_margin: f64,
    pub(crate) diagram_margin_x: f64,
    pub(crate) diagram_margin_y: f64,
    pub(crate) task_margin: f64,
    pub(crate) cell_width: f64,
    pub(crate) cell_height: f64,
    pub(crate) actor_colours: Vec<String>,
    pub(crate) section_fills: Vec<String>,
    pub(crate) use_max_width: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct JourneyRenderSettings {
    pub(crate) task_text_style: TextStyle,
    pub(crate) title_font_size: String,
    pub(crate) title_font_family: String,
    pub(crate) title_color: String,
}

pub(crate) const fn default_use_max_width() -> bool {
    DEFAULT_USE_MAX_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn journey_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = JourneyConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.left_margin_base, DEFAULT_LEFT_MARGIN);
        assert_eq!(settings.max_label_width, DEFAULT_MAX_LABEL_WIDTH);
        assert_eq!(settings.box_text_margin, DEFAULT_BOX_TEXT_MARGIN);
        assert_eq!(settings.diagram_margin_x, DEFAULT_DIAGRAM_MARGIN_X);
        assert_eq!(settings.diagram_margin_y, DEFAULT_DIAGRAM_MARGIN_Y);
        assert_eq!(settings.task_margin, DEFAULT_TASK_MARGIN);
        assert_eq!(settings.cell_width, DEFAULT_CELL_WIDTH);
        assert_eq!(settings.cell_height, DEFAULT_CELL_HEIGHT);
        assert!(settings.use_max_width);
        assert_eq!(settings.actor_colours.len(), DEFAULT_ACTOR_COLOURS.len());
        assert_eq!(settings.section_fills.len(), DEFAULT_SECTION_FILLS.len());
    }

    #[test]
    fn journey_layout_settings_project_configured_values() {
        let cfg = json!({
            "journey": {
                "leftMargin": 180,
                "maxLabelWidth": 250,
                "boxTextMargin": 8,
                "diagramMarginX": 60,
                "diagramMarginY": 12,
                "taskMargin": 90,
                "width": 170,
                "height": 70,
                "taskFontSize": 20,
                "taskFontFamily": "Inter, sans-serif",
                "actorColours": ["#111", "#222"],
                "sectionFills": ["#333", "#444"],
                "useMaxWidth": false
            }
        });
        let settings = JourneyConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.left_margin_base, 180.0);
        assert_eq!(settings.max_label_width, 250.0);
        assert_eq!(settings.box_text_margin, 8.0);
        assert_eq!(settings.diagram_margin_x, 60.0);
        assert_eq!(settings.diagram_margin_y, 12.0);
        assert_eq!(settings.task_margin, 90.0);
        assert_eq!(settings.cell_width, 170.0);
        assert_eq!(settings.cell_height, 70.0);
        assert_eq!(settings.actor_colours, vec!["#111", "#222"]);
        assert_eq!(settings.section_fills, vec!["#333", "#444"]);
        assert!(!settings.use_max_width);
    }

    #[test]
    fn journey_render_settings_project_values() {
        let cfg = json!({
            "journey": {
                "taskFontSize": 18,
                "taskFontFamily": "Inter, sans-serif",
                "titleFontSize": "3.5ex",
                "titleFontFamily": "Georgia, serif",
                "titleColor": "#123456"
            }
        });
        let settings = JourneyConfigView::new(&cfg).render_settings();

        assert_eq!(settings.task_text_style.font_size, 18.0);
        assert_eq!(
            settings.task_text_style.font_family.as_deref(),
            Some("Inter, sans-serif")
        );
        assert_eq!(settings.title_font_size, "3.5ex");
        assert_eq!(settings.title_font_family, "Georgia, serif");
        assert_eq!(settings.title_color, "#123456");
    }
}
