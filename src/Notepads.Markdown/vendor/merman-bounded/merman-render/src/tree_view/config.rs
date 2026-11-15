use crate::config::{config_bool, config_f64, config_font_family_css, config_string};
use crate::theme::PresentationTheme;
use serde_json::Value;
use std::collections::HashMap;

const DEFAULT_ROW_INDENT: f64 = 10.0;
const DEFAULT_PADDING_X: f64 = 5.0;
const DEFAULT_PADDING_Y: f64 = 5.0;
const DEFAULT_LINE_THICKNESS: f64 = 1.0;
const DEFAULT_USE_MAX_WIDTH: bool = true;

pub(crate) struct TreeViewConfigView<'a> {
    effective_config: &'a Value,
    tree_view_config: &'a Value,
}

impl<'a> TreeViewConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            tree_view_config: effective_config.get("treeView").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> TreeViewLayoutSettings {
        let theme = PresentationTheme::new(self.effective_config).tree_view();
        TreeViewLayoutSettings {
            row_indent: self
                .tree_view_f64("rowIndent")
                .unwrap_or(DEFAULT_ROW_INDENT)
                .max(0.0),
            padding_x: self
                .tree_view_f64("paddingX")
                .unwrap_or(DEFAULT_PADDING_X)
                .max(0.0),
            padding_y: self
                .tree_view_f64("paddingY")
                .unwrap_or(DEFAULT_PADDING_Y)
                .max(0.0),
            line_thickness: self
                .tree_view_f64("lineThickness")
                .unwrap_or(DEFAULT_LINE_THICKNESS)
                .max(0.0),
            use_max_width: self
                .tree_view_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
            font_family: config_font_family_css(self.effective_config),
            label_font_size: theme.label_font_size,
            show_icons: self.tree_view_bool("showIcons").unwrap_or(false),
            default_icon_pack: self.tree_view_string("defaultIconPack").unwrap_or_default(),
            filename_icons: self.tree_view_string_map("filenameIcons"),
            extension_icons: self.tree_view_string_map("extensionIcons"),
        }
    }

    fn tree_view_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.tree_view_config, &[key])
    }

    fn tree_view_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.tree_view_config, &[key])
    }

    fn tree_view_string(&self, key: &str) -> Option<String> {
        config_string(self.tree_view_config, &[key])
    }

    fn tree_view_string_map(&self, key: &str) -> HashMap<String, String> {
        self.tree_view_config
            .get(key)
            .and_then(Value::as_object)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TreeViewLayoutSettings {
    pub(crate) row_indent: f64,
    pub(crate) padding_x: f64,
    pub(crate) padding_y: f64,
    pub(crate) line_thickness: f64,
    pub(crate) use_max_width: bool,
    pub(crate) font_family: String,
    pub(crate) label_font_size: f64,
    pub(crate) show_icons: bool,
    pub(crate) default_icon_pack: String,
    pub(crate) filename_icons: HashMap<String, String>,
    pub(crate) extension_icons: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tree_view_layout_settings_preserve_defaults_and_theme_font_size() {
        let cfg = json!({});
        let settings = TreeViewConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.row_indent, DEFAULT_ROW_INDENT);
        assert_eq!(settings.padding_x, DEFAULT_PADDING_X);
        assert_eq!(settings.padding_y, DEFAULT_PADDING_Y);
        assert_eq!(settings.line_thickness, DEFAULT_LINE_THICKNESS);
        assert!(settings.use_max_width);
        assert_eq!(
            settings.font_family,
            "\"trebuchet ms\",verdana,arial,sans-serif"
        );
        assert_eq!(settings.label_font_size, 16.0);
        assert!(!settings.show_icons);
        assert_eq!(settings.default_icon_pack, "");
        assert!(settings.filename_icons.is_empty());
        assert!(settings.extension_icons.is_empty());
    }

    #[test]
    fn tree_view_layout_settings_project_configured_values() {
        let cfg = json!({
            "treeView": {
                "rowIndent": "12",
                "paddingX": 7,
                "paddingY": 8,
                "lineThickness": 2,
                "useMaxWidth": false,
                "showIcons": true,
                "defaultIconPack": "logos",
                "filenameIcons": { "Dockerfile": "docker" },
                "extensionIcons": { ".ts": "typescript" }
            },
            "themeVariables": {
                "treeView": {
                    "labelFontSize": "21px"
                }
            }
        });
        let settings = TreeViewConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.row_indent, 12.0);
        assert_eq!(settings.padding_x, 7.0);
        assert_eq!(settings.padding_y, 8.0);
        assert_eq!(settings.line_thickness, 2.0);
        assert!(!settings.use_max_width);
        assert_eq!(settings.label_font_size, 21.0);
        assert!(settings.show_icons);
        assert_eq!(settings.default_icon_pack, "logos");
        assert_eq!(
            settings
                .filename_icons
                .get("Dockerfile")
                .map(String::as_str),
            Some("docker")
        );
        assert_eq!(
            settings.extension_icons.get(".ts").map(String::as_str),
            Some("typescript")
        );
    }

    #[test]
    fn tree_view_layout_settings_clamp_negative_geometry() {
        let cfg = json!({
            "treeView": {
                "rowIndent": -12,
                "paddingX": -7,
                "paddingY": -8,
                "lineThickness": -2
            }
        });
        let settings = TreeViewConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.row_indent, 0.0);
        assert_eq!(settings.padding_x, 0.0);
        assert_eq!(settings.padding_y, 0.0);
        assert_eq!(settings.line_thickness, 0.0);
    }
}
