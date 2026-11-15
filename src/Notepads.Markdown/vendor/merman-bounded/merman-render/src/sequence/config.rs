use crate::text::TextStyle;
use serde_json::Value;

pub(crate) struct SequenceConfigView<'a> {
    effective_config: &'a Value,
    sequence_config: &'a Value,
}

impl<'a> SequenceConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            sequence_config: effective_config.get("sequence").unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn sequence_bool(&self, key: &str, default: bool) -> bool {
        self.sequence_config_bool(key).unwrap_or(default)
    }

    pub(crate) fn sequence_config_bool(&self, key: &str) -> Option<bool> {
        self.sequence_config.get(key).and_then(Value::as_bool)
    }

    fn sequence_json_number_or(&self, key: &str, default: f64) -> f64 {
        self.sequence_json_number(key).unwrap_or(default)
    }

    pub(crate) fn sequence_json_number(&self, key: &str) -> Option<f64> {
        self.sequence_config.get(key).and_then(Value::as_f64)
    }

    pub(crate) fn sequence_json_number_min(&self, key: &str, default: f64, min: f64) -> f64 {
        self.sequence_json_number_or(key, default).max(min)
    }

    pub(crate) fn root_bool(&self, key: &str) -> Option<bool> {
        self.effective_config.get(key).and_then(Value::as_bool)
    }

    pub(crate) fn sequence_string(&self, key: &str) -> Option<String> {
        crate::config::config_string(self.sequence_config, &[key])
    }

    fn parse_font_weight(value: &Value) -> Option<String> {
        use cssparser::ToCss;

        let raw = match value {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => return None,
        };
        let mut parser = cssparser::Parser::new(&raw);
        let token = parser.next().ok()?;
        let css = match token {
            cssparser::Token::Number { value, .. } if (1.0..=1000.0).contains(value) => {
                token.to_css_string()
            }
            cssparser::Token::Ident(value)
                if [
                    "normal",
                    "bold",
                    "bolder",
                    "lighter",
                    "inherit",
                    "initial",
                    "unset",
                    "revert",
                    "revert-layer",
                ]
                .iter()
                .any(|keyword| value.eq_ignore_ascii_case(keyword)) =>
            {
                value.to_ascii_lowercase()
            }
            _ => return None,
        };
        parser.is_exhausted().then_some(css)
    }

    pub(crate) fn font_weight(&self, key: &str) -> Option<String> {
        // sequenceRenderer.setConf selects a truthy global value before CSSOM validation.
        // A rejected global value must not reveal the overridden family setting.
        let selected = self.font_value("fontWeight", key);
        match selected {
            Some(value) => Self::parse_font_weight(value),
            None => Some("400".to_string()),
        }
    }

    fn sequence_compat_f64(&self, key: &str, default: f64) -> f64 {
        crate::config::config_f64(self.sequence_config, &[key]).unwrap_or(default)
    }

    fn sequence_compat_f64_min(&self, key: &str, default: f64, min: f64) -> f64 {
        self.sequence_compat_f64(key, default).max(min)
    }

    fn font_value(&self, root_key: &str, sequence_key: &str) -> Option<&Value> {
        // sequenceRenderer.setConf overrides each family only for a truthy global value.
        self.effective_config
            .get(root_key)
            .filter(|value| crate::config::json_value_is_truthy(value))
            .or_else(|| self.sequence_config.get(sequence_key))
    }

    pub(crate) fn text_style(
        &self,
        family_key: &str,
        size_key: &str,
        weight_key: &str,
    ) -> TextStyle {
        let font_family = self
            .font_value("fontFamily", family_key)
            .and_then(Value::as_str)
            .map(str::to_owned);
        // svgDraw.parseFontSize accepts numeric strings and explicit px lengths as well
        // as numbers. Resolve these identically for layout and SVG emission.
        let font_size = self
            .font_value("fontSize", size_key)
            .and_then(crate::config::json_f64_css_px)
            .unwrap_or(16.0);
        let font_weight = self.font_weight(weight_key);

        TextStyle {
            font_family,
            font_size,
            font_weight,
            font_style: None,
        }
    }
}

pub(super) struct SequenceLayoutSettings {
    pub(super) diagram_margin_x: f64,
    pub(super) diagram_margin_y: f64,
    pub(super) bottom_margin_adj: f64,
    pub(super) box_margin: f64,
    pub(super) actor_margin: f64,
    pub(super) sequence_default_width: f64,
    pub(super) actor_height: f64,
    pub(super) wrap_padding: f64,
    pub(super) note_margin: f64,
    pub(super) box_text_margin: f64,
    pub(super) label_box_height: f64,
    pub(super) label_box_width: f64,
    pub(super) mirror_actors: bool,
    pub(super) right_angles: bool,
    pub(super) is_neo: bool,
    pub(super) activation_width: f64,
    pub(super) actor_text_style: TextStyle,
    pub(super) note_text_style: TextStyle,
    pub(super) msg_text_style: TextStyle,
}

impl SequenceLayoutSettings {
    pub(super) fn from_effective_config(effective_config: &Value) -> Self {
        let config = SequenceConfigView::new(effective_config);

        let diagram_margin_x = config.sequence_compat_f64("diagramMarginX", 50.0);
        let diagram_margin_y = config.sequence_compat_f64("diagramMarginY", 10.0);
        let bottom_margin_adj = config.sequence_compat_f64("bottomMarginAdj", 1.0);
        let box_margin = config.sequence_compat_f64("boxMargin", 10.0);
        let actor_margin = config.sequence_compat_f64("actorMargin", 50.0);
        let sequence_default_width = config.sequence_compat_f64("width", 150.0);
        let actor_height = config.sequence_compat_f64("height", 65.0);
        let wrap_padding = config.sequence_compat_f64("wrapPadding", 10.0);
        let note_margin = config.sequence_compat_f64("noteMargin", 10.0);
        let box_text_margin = config.sequence_compat_f64("boxTextMargin", 5.0);
        let label_box_height = config.sequence_compat_f64("labelBoxHeight", 20.0);
        let label_box_width = config.sequence_compat_f64("labelBoxWidth", 50.0);
        let mirror_actors = config.sequence_bool("mirrorActors", true);
        let right_angles = config.sequence_bool("rightAngles", false);
        let is_neo = crate::config::config_diagram_look(effective_config).is_neo();
        let activation_width = config.sequence_compat_f64_min("activationWidth", 10.0, 1.0);

        let actor_text_style =
            config.text_style("actorFontFamily", "actorFontSize", "actorFontWeight");
        let note_text_style = config.text_style("noteFontFamily", "noteFontSize", "noteFontWeight");
        let msg_text_style =
            config.text_style("messageFontFamily", "messageFontSize", "messageFontWeight");

        Self {
            diagram_margin_x,
            diagram_margin_y,
            bottom_margin_adj,
            box_margin,
            actor_margin,
            sequence_default_width,
            actor_height,
            wrap_padding,
            note_margin,
            box_text_margin,
            label_box_height,
            label_box_width,
            mirror_actors,
            right_angles,
            is_neo,
            activation_width,
            actor_text_style,
            note_text_style,
            msg_text_style,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn note_weight_matches_source_numeric_string_and_global_precedence() {
        for (config, expected) in [
            (json!({}), Some("400")),
            (json!({"sequence": {"noteFontWeight": 700}}), Some("700")),
            (
                json!({"sequence": {"noteFontWeight": "bold"}}),
                Some("bold"),
            ),
            (
                json!({"fontWeight": 500, "sequence": {"noteFontWeight": 700}}),
                Some("500"),
            ),
            (
                json!({"fontWeight": "600", "sequence": {"noteFontWeight": 700}}),
                Some("600"),
            ),
            (
                json!({"fontWeight": 0, "sequence": {"noteFontWeight": 700}}),
                Some("700"),
            ),
            (
                json!({"fontWeight": "", "sequence": {"noteFontWeight": 700}}),
                Some("700"),
            ),
            (
                json!({"sequence": {"noteFontWeight": "700; font-style: italic"}}),
                None,
            ),
            (
                json!({"sequence": {"noteFontWeight": "700 !important"}}),
                None,
            ),
            (
                json!({"sequence": {"noteFontWeight": "bolder"}}),
                Some("bolder"),
            ),
            (
                json!({"sequence": {"noteFontWeight": "inherit"}}),
                Some("inherit"),
            ),
            (
                json!({"sequence": {"noteFontWeight": 700.5}}),
                Some("700.5"),
            ),
            (json!({"sequence": {"noteFontWeight": 0}}), None),
        ] {
            let settings = SequenceLayoutSettings::from_effective_config(&config);
            assert_eq!(
                settings.note_text_style.font_weight.as_deref(),
                expected,
                "{config}"
            );
        }
    }

    #[test]
    fn actor_weight_uses_one_selected_value_for_layout_and_rendering() {
        for (root, expected) in [
            (json!("bogus"), None),
            (json!(true), None),
            (json!("700; font-style: italic"), None),
            (json!("bolder"), Some("bolder")),
            (json!("inherit"), Some("inherit")),
            (json!(false), Some("700")),
            (json!(0), Some("700")),
            (json!(""), Some("700")),
        ] {
            let config = json!({
                "fontWeight": root,
                "sequence": {
                    "actorFontWeight": 700,
                    "noteFontWeight": 700,
                    "messageFontWeight": 700
                }
            });
            let view = SequenceConfigView::new(&config);
            let settings = SequenceLayoutSettings::from_effective_config(&config);
            for weight in [
                view.font_weight("actorFontWeight"),
                settings.actor_text_style.font_weight,
                settings.note_text_style.font_weight,
                settings.msg_text_style.font_weight,
            ] {
                assert_eq!(weight.as_deref(), expected, "{config}");
            }
        }
    }

    #[test]
    fn sequence_weights_preserve_static_css_keywords() {
        for weight in [
            "normal",
            "bold",
            "bolder",
            "lighter",
            "inherit",
            "initial",
            "unset",
            "revert",
            "revert-layer",
        ] {
            assert_eq!(
                SequenceConfigView::parse_font_weight(&json!(weight.to_uppercase())).as_deref(),
                Some(weight),
            );
        }
    }

    #[test]
    fn sequence_layout_settings_preserve_layout_numeric_string_config() {
        let cfg = json!({
            "look": "neo",
            "fontFamily": "Global, Arial",
            "fontSize": "22",
            "fontWeight": "700",
            "sequence": {
                "width": "240",
                "height": "80",
                "activationWidth": "0.25",
                "mirrorActors": false,
                "messageFontFamily": "Message",
                "messageFontSize": "18",
                "messageFontWeight": "300"
            }
        });

        let settings = SequenceLayoutSettings::from_effective_config(&cfg);

        assert_eq!(settings.sequence_default_width, 240.0);
        assert_eq!(settings.actor_height, 80.0);
        assert_eq!(settings.activation_width, 1.0);
        assert!(!settings.mirror_actors);
        assert!(settings.is_neo);
        assert_eq!(
            settings.msg_text_style.font_family.as_deref(),
            Some("Global, Arial")
        );
        assert_eq!(settings.msg_text_style.font_size, 22.0);
        assert_eq!(settings.msg_text_style.font_weight.as_deref(), Some("700"));
    }

    #[test]
    fn sequence_layout_settings_use_family_font_fallbacks_without_global_font() {
        let cfg = json!({
            "sequence": {
                "actorFontFamily": "Actor",
                "actorFontSize": "19",
                "actorFontWeight": "500",
                "noteFontFamily": "Note",
                "noteFontSize": 20,
                "noteFontWeight": "600",
                "messageFontFamily": "Message",
                "messageFontSize": 21,
                "messageFontWeight": "700"
            }
        });

        let settings = SequenceLayoutSettings::from_effective_config(&cfg);

        assert_eq!(
            settings.actor_text_style.font_family.as_deref(),
            Some("Actor")
        );
        assert_eq!(settings.actor_text_style.font_size, 19.0);
        assert_eq!(
            settings.actor_text_style.font_weight.as_deref(),
            Some("500")
        );
        assert_eq!(
            settings.note_text_style.font_family.as_deref(),
            Some("Note")
        );
        assert_eq!(settings.note_text_style.font_size, 20.0);
        assert_eq!(settings.note_text_style.font_weight.as_deref(), Some("600"));
        assert_eq!(
            settings.msg_text_style.font_family.as_deref(),
            Some("Message")
        );
        assert_eq!(settings.msg_text_style.font_size, 21.0);
        assert_eq!(settings.msg_text_style.font_weight.as_deref(), Some("700"));
    }
}
