use crate::text::TextStyle;

pub(super) struct SequenceRenderSettings {
    pub(super) force_menus: bool,
    pub(super) mirror_actors: bool,
    pub(super) diagram_margin_x: f64,
    pub(super) box_margin: f64,
    pub(super) actor_height: f64,
    pub(super) box_text_margin: f64,
    pub(super) message_align: String,
    pub(super) label_box_height: f64,
    pub(super) label_box_width: f64,
    pub(super) right_angles: bool,
    pub(super) wrap_padding: f64,
    pub(super) note_margin: f64,
    pub(super) sequence_width: f64,
    pub(super) activation_width: f64,
    pub(super) actor_wrap_width: f64,
    pub(super) rect_default_fill: String,
    pub(super) loop_text_style: TextStyle,
    pub(super) actor_text_style: TextStyle,
    pub(super) note_text_style: TextStyle,
}

impl SequenceRenderSettings {
    pub(super) fn from_effective_config(effective_config: &serde_json::Value) -> Self {
        let config = crate::sequence::config::SequenceConfigView::new(effective_config);

        let force_menus = config
            .sequence_config_bool("forceMenus")
            .or_else(|| config.root_bool("forceMenus"))
            .unwrap_or(false);
        let mirror_actors = config.sequence_bool("mirrorActors", true);
        let diagram_margin_x = config.sequence_json_number_min("diagramMarginX", 50.0, 0.0);
        let box_margin = config.sequence_json_number("boxMargin").unwrap_or(10.0);
        let actor_height = config.sequence_json_number_min("height", 65.0, 1.0);
        let box_text_margin = config.sequence_json_number("boxTextMargin").unwrap_or(5.0);
        let message_align = config
            .sequence_string("messageAlign")
            .unwrap_or_else(|| "center".to_string());
        let label_box_height = config
            .sequence_json_number("labelBoxHeight")
            .unwrap_or(20.0);
        let label_box_width = config
            .sequence_json_number_min("labelBoxWidth", 50.0, 0.0)
            .max(50.0);
        let right_angles = config.sequence_bool("rightAngles", false);
        let wrap_padding = config.sequence_json_number("wrapPadding").unwrap_or(10.0);
        let note_margin = config.sequence_json_number("noteMargin").unwrap_or(10.0);
        let sequence_width = config.sequence_json_number_min("width", 150.0, 1.0);
        let activation_width = config.sequence_json_number_min("activationWidth", 10.0, 1.0);

        let loop_text_style =
            config.text_style("messageFontFamily", "messageFontSize", "messageFontWeight");
        let actor_text_style =
            config.text_style("actorFontFamily", "actorFontSize", "actorFontWeight");
        let note_text_style = config.text_style("noteFontFamily", "noteFontSize", "noteFontWeight");
        let actor_wrap_width = (sequence_width - 2.0 * wrap_padding).max(1.0);
        let rect_default_fill =
            crate::config::config_string(effective_config, &["themeVariables", "rectBkgColor"])
                .filter(|fill| !fill.is_empty())
                .or_else(|| {
                    crate::config::config_string(effective_config, &["themeVariables", "actorBkg"])
                        .filter(|fill| !fill.is_empty())
                })
                .unwrap_or_else(|| "rgba(128, 128, 128, 0.5)".to_string());

        Self {
            force_menus,
            mirror_actors,
            diagram_margin_x,
            box_margin,
            actor_height,
            box_text_margin,
            message_align,
            label_box_height,
            label_box_width,
            right_angles,
            wrap_padding,
            note_margin,
            sequence_width,
            activation_width,
            actor_wrap_width,
            rect_default_fill,
            loop_text_style,
            actor_text_style,
            note_text_style,
        }
    }
}

/// Preserve CSSOM's rejected-family behavior while emitting the same size and weight we measure.
pub(super) fn sequence_text_style_attribute(style: &TextStyle) -> String {
    let mut css = String::new();
    if let Some(family) = crate::sequence::sequence_inline_font_family(style) {
        css.push_str("font-family: ");
        css.push_str(&family);
        css.push_str("; ");
    }
    css.push_str(&format!("font-size: {}px;", style.font_size));
    if let Some(weight) = style.font_weight.as_deref() {
        css.push_str(" font-weight: ");
        css.push_str(weight);
        css.push(';');
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn note_weight_reaches_inline_style_without_changing_message_weight() {
        for weight in [json!(700), json!("700")] {
            let settings = SequenceRenderSettings::from_effective_config(&json!({
                "sequence": {"noteFontWeight": weight},
                "themeVariables": {"noteFontWeight": 600},
            }));
            assert_eq!(settings.note_text_style.font_weight.as_deref(), Some("700"));
            assert_eq!(settings.loop_text_style.font_weight.as_deref(), Some("400"));
            assert!(
                sequence_text_style_attribute(&settings.note_text_style)
                    .contains("font-weight: 700;")
            );
        }
        let settings = SequenceRenderSettings::from_effective_config(&json!({
            "sequence": {"noteFontWeight": "700; font-style: italic"}
        }));
        let css = sequence_text_style_attribute(&settings.note_text_style);
        assert!(!css.contains("font-weight"), "{css}");
        assert!(!css.contains("font-style"), "{css}");
    }

    #[test]
    fn sequence_render_settings_parse_fonts_without_changing_geometry_numeric_types() {
        let cfg = json!({
            "fontSize": "22",
            "sequence": {
                "width": "240",
                "wrapPadding": "12",
                "activationWidth": "0.25",
                "messageFontSize": "18"
            }
        });

        let settings = SequenceRenderSettings::from_effective_config(&cfg);

        assert_eq!(settings.sequence_width, 150.0);
        assert_eq!(settings.wrap_padding, 10.0);
        assert_eq!(settings.activation_width, 10.0);
        assert_eq!(settings.loop_text_style.font_size, 22.0);
        assert_eq!(settings.actor_wrap_width, 130.0);
    }

    #[test]
    fn sequence_render_settings_apply_number_precedence_and_clamps() {
        let cfg = json!({
            "fontFamily": "Inter, sans-serif",
            "fontSize": 22,
            "forceMenus": true,
            "sequence": {
                "forceMenus": false,
                "mirrorActors": false,
                "diagramMarginX": -5,
                "boxMargin": -1,
                "height": 0,
                "boxTextMargin": -1,
                "messageAlign": "left",
                "labelBoxHeight": -1,
                "rightAngles": true,
                "wrapPadding": -2,
                "noteMargin": -3,
                "width": 0.5,
                "activationWidth": 0,
                "messageFontSize": 18
            }
        });

        let settings = SequenceRenderSettings::from_effective_config(&cfg);

        assert!(!settings.force_menus);
        assert!(!settings.mirror_actors);
        assert_eq!(settings.diagram_margin_x, 0.0);
        assert_eq!(settings.box_margin, -1.0);
        assert_eq!(settings.actor_height, 1.0);
        assert_eq!(settings.box_text_margin, -1.0);
        assert_eq!(settings.message_align, "left");
        assert_eq!(settings.label_box_height, -1.0);
        assert!(settings.right_angles);
        assert_eq!(settings.wrap_padding, -2.0);
        assert_eq!(settings.note_margin, -3.0);
        assert_eq!(settings.sequence_width, 1.0);
        assert_eq!(settings.activation_width, 1.0);
        assert_eq!(settings.loop_text_style.font_size, 22.0);
        assert_eq!(settings.actor_wrap_width, 5.0);
        assert_eq!(
            settings.loop_text_style.font_family.as_deref(),
            Some("Inter, sans-serif")
        );
        assert_eq!(settings.loop_text_style.font_size, 22.0);
        assert_eq!(
            settings.note_text_style.font_family.as_deref(),
            Some("Inter, sans-serif")
        );
    }

    #[test]
    fn sequence_render_settings_fall_back_to_root_force_menus() {
        let cfg = json!({
            "forceMenus": true,
            "sequence": {}
        });

        let settings = SequenceRenderSettings::from_effective_config(&cfg);

        assert!(settings.force_menus);
    }

    #[test]
    fn sequence_render_settings_choose_bare_rect_fill_fallbacks() {
        for (theme_variables, expected) in [
            (
                json!({
                    "rectBkgColor": "#112233",
                    "actorBkg": "#445566",
                }),
                "#112233",
            ),
            (json!({ "actorBkg": "#445566" }), "#445566"),
            (json!({}), "rgba(128, 128, 128, 0.5)"),
        ] {
            let settings = SequenceRenderSettings::from_effective_config(&json!({
                "themeVariables": theme_variables,
            }));
            assert_eq!(settings.rect_default_fill, expected);
        }
    }
    #[test]
    fn drawn_style_emits_valid_inline_family_and_preserves_inheritance_for_rejected_values() {
        for family in [None, Some("Inline Font, monospace"), Some("Inline Font; ")] {
            let style = TextStyle {
                font_family: family.map(str::to_owned),
                font_size: 18.0,
                font_weight: Some("400".to_string()),
                font_style: None,
            };
            let css = super::sequence_text_style_attribute(&style);
            assert!(css.contains("font-size: 18px; font-weight: 400;"));
            assert_eq!(
                css.contains("font-family:"),
                family == Some("Inline Font, monospace")
            );
        }
    }
    #[test]
    fn font_family_serialization_rejects_declaration_injection_but_keeps_quoted_semicolons() {
        for (family, accepted) in [
            ("Arial; font-style: italic", false),
            (r#""Semi;Colon", serif"#, true),
            ("'unterminated", true),
        ] {
            let style = TextStyle {
                font_family: Some(family.to_string()),
                ..Default::default()
            };
            let css = super::sequence_text_style_attribute(&style);
            assert_eq!(css.contains("font-family:"), accepted);
            assert!(!css.contains("font-style:"));
            if family == "'unterminated" {
                assert!(
                    css.starts_with("font-family: \"unterminated\"; font-size:"),
                    "{css}"
                );
            }
        }
    }
}
