//! Shared helpers for state diagram layout.

use super::StateNode;
pub(super) use crate::config::config_f64;
use crate::config::{
    config_diagram_look, config_effective_html_labels, config_f64_css_px, config_string,
    config_string_or_first_array,
};
use crate::text::TextStyle;
use crate::text::WrapMode;
use dugong::{GraphLabel, RankDir};
use serde_json::Value;

const DEFAULT_STATE_NODE_SPACING: f64 = 50.0;
const DEFAULT_STATE_RANK_SPACING: f64 = 50.0;
const DEFAULT_STATE_PADDING: f64 = 8.0;
const DEFAULT_STATE_TITLE_TOP_MARGIN: f64 = 25.0;
// Mermaid state defaults set minNodeWidth to 120 in config.schema.yaml.
const DEFAULT_STATE_LABEL_MIN_WIDTH: f64 = 120.0;
const DEFAULT_HTML_LABEL_WRAPPING_WIDTH: f64 = 200.0;
const DEFAULT_STATE_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial, sans-serif";

pub(super) fn state_node_is_effective_group(n: &StateNode) -> bool {
    n.is_group && n.shape != "note"
}

pub(super) fn normalize_dir(direction: &str) -> String {
    match direction.trim().to_uppercase().as_str() {
        "TB" | "TD" => "TB".to_string(),
        "BT" => "BT".to_string(),
        "LR" => "LR".to_string(),
        "RL" => "RL".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn rank_dir_from(direction: &str) -> RankDir {
    match normalize_dir(direction).as_str() {
        "TB" => RankDir::TB,
        "BT" => RankDir::BT,
        "LR" => RankDir::LR,
        "RL" => RankDir::RL,
        _ => RankDir::TB,
    }
}

pub(super) fn value_to_label_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .first()
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => "".to_string(),
    }
}

pub(super) fn decode_html_entities_once(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains('ﬂ') || text.contains('¶') || text.contains('#') {
        return merman_core::entities::decode_mermaid_entities_to_unicode(text);
    }

    fn decode_html_entity(entity: &str) -> Option<char> {
        match entity {
            "nbsp" => Some(' '),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "#39" => Some('\''),
            "colon" => Some(':'),
            "equals" => Some('='),
            _ => {
                if let Some(hex) = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                {
                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    None
                }
            }
        }
    }

    if !text.contains('&') {
        return std::borrow::Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while let Some(rel) = text[i..].find('&') {
        let amp = i + rel;
        out.push_str(&text[i..amp]);
        let tail = &text[amp + 1..];
        if let Some(semi_rel) = tail.find(';') {
            let semi = amp + 1 + semi_rel;
            let entity = &text[amp + 1..semi];
            if let Some(decoded) = decode_html_entity(entity) {
                out.push(decoded);
            } else {
                out.push_str(&text[amp..=semi]);
            }
            i = semi + 1;
            continue;
        }
        out.push('&');
        i = amp + 1;
    }
    out.push_str(&text[i..]);
    std::borrow::Cow::Owned(out)
}

pub(crate) fn state_text_style(effective_config: &Value) -> TextStyle {
    StateConfigView::new(effective_config).text_style()
}

pub(crate) struct StateConfigView<'a> {
    effective_config: &'a Value,
    flowchart_config: &'a Value,
    state_config: &'a Value,
}

impl<'a> StateConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            flowchart_config: effective_config.get("flowchart").unwrap_or(&Value::Null),
            state_config: effective_config.get("state").unwrap_or(&Value::Null),
        }
    }

    pub(super) fn layout_settings(&self, direction: &str) -> StateLayoutSettings {
        let html_labels = config_effective_html_labels(self.effective_config);
        StateLayoutSettings {
            graph: GraphLabel {
                rankdir: rank_dir_from(direction),
                nodesep: self.layout_spacing("nodeSpacing", DEFAULT_STATE_NODE_SPACING),
                ranksep: self.layout_spacing("rankSpacing", DEFAULT_STATE_RANK_SPACING),
                marginx: 8.0,
                marginy: 8.0,
                ..Default::default()
            },
            html_labels,
            wrap_mode: state_wrap_mode(html_labels),
            wrapping_width: self.html_label_wrapping_width(),
            state_padding: self.state_padding(),
            label_min_width: self.state_label_min_width(),
            text_style: self.text_style(),
        }
    }

    pub(crate) fn render_settings(&self) -> StateRenderSettings {
        let html_labels = config_effective_html_labels(self.effective_config);
        StateRenderSettings {
            title_top_margin: self.title_top_margin(),
            diagram_look: config_diagram_look(self.effective_config)
                .as_str()
                .to_string(),
            hand_drawn_seed: self
                .effective_config
                .get("handDrawnSeed")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            html_labels,
            html_label_wrapping_width: self.html_label_wrapping_width(),
            label_min_width: self.state_label_min_width(),
            state_padding: self.state_padding(),
            security_level_loose: self.root_string("securityLevel").as_deref() == Some("loose"),
            text_style: self.text_style(),
        }
    }

    pub(crate) fn title_top_margin(&self) -> f64 {
        self.state_compat_f64("titleTopMargin")
            .unwrap_or(DEFAULT_STATE_TITLE_TOP_MARGIN)
            .max(0.0)
    }

    pub(crate) fn text_style(&self) -> TextStyle {
        // Mermaid state diagram v2 uses HTML labels (foreignObject) by default, inheriting the
        // global `#id{font-size: ...}` rule (defaults to 16px). The 10px
        // `g.stateGroup text{font-size:10px}` rule applies to SVG `<text>` elements, not HTML
        // labels.
        let font_family = config_string_or_first_array(self.effective_config, &["fontFamily"])
            .or_else(|| {
                config_string_or_first_array(
                    self.effective_config,
                    &["themeVariables", "fontFamily"],
                )
            })
            .or_else(|| Some(DEFAULT_STATE_FONT_FAMILY.to_string()));
        // Mermaid CLI baselines show state labels inheriting the SVG root font-size rule
        // (`themeVariables.fontSize`, typically a `"NNpx"` string).
        let font_size =
            crate::config::config_theme_or_root_font_size_px(self.effective_config, 16.0).max(1.0);
        TextStyle {
            font_family,
            font_size,
            font_weight: None,
            font_style: None,
        }
    }

    pub(crate) fn state_label_min_width(&self) -> f64 {
        self.state_compat_f64("minNodeWidth")
            .unwrap_or(DEFAULT_STATE_LABEL_MIN_WIDTH)
            .max(0.0)
    }

    pub(crate) fn html_label_wrapping_width(&self) -> f64 {
        config_f64_css_px(self.state_config, &["wrappingWidth"])
            .filter(|width| *width != 0.0)
            .or_else(|| config_f64_css_px(self.flowchart_config, &["wrappingWidth"]))
            .unwrap_or(DEFAULT_HTML_LABEL_WRAPPING_WIDTH)
            .max(0.0)
    }

    pub(crate) fn state_padding(&self) -> f64 {
        self.state_compat_f64("padding")
            .unwrap_or(DEFAULT_STATE_PADDING)
            .max(0.0)
    }

    fn root_string(&self, key: &str) -> Option<String> {
        config_string(self.effective_config, &[key])
    }

    fn root_compat_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.effective_config, &[key])
    }

    fn flowchart_compat_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.flowchart_config, &[key])
    }

    fn state_compat_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.state_config, &[key])
    }

    fn layout_spacing(&self, key: &str, default: f64) -> f64 {
        let renderer_spacing = self
            .state_compat_f64(key)
            .filter(|value| *value != 0.0)
            .unwrap_or(default);
        [
            self.root_compat_f64(key),
            Some(renderer_spacing),
            self.flowchart_compat_f64(key),
        ]
        .into_iter()
        .flatten()
        .find(|value| *value != 0.0)
        .unwrap_or(default)
    }
}

pub(super) struct StateLayoutSettings {
    pub(super) graph: GraphLabel,
    pub(super) html_labels: bool,
    pub(super) wrap_mode: WrapMode,
    pub(super) wrapping_width: f64,
    pub(super) state_padding: f64,
    pub(super) label_min_width: f64,
    pub(super) text_style: TextStyle,
}

pub(crate) struct StateRenderSettings {
    pub(crate) title_top_margin: f64,
    pub(crate) diagram_look: String,
    pub(crate) hand_drawn_seed: f64,
    pub(crate) html_labels: bool,
    pub(crate) html_label_wrapping_width: f64,
    pub(crate) label_min_width: f64,
    pub(crate) state_padding: f64,
    pub(crate) security_level_loose: bool,
    pub(crate) text_style: TextStyle,
}

fn state_wrap_mode(html_labels: bool) -> WrapMode {
    if html_labels {
        WrapMode::HtmlLike
    } else {
        WrapMode::SvgLike
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_html_label_wrapping_width_honors_number_and_px_string() {
        let state_override =
            json!({"state": {"wrappingWidth": 120}, "flowchart": {"wrappingWidth": 320}});
        assert_eq!(
            StateConfigView::new(&state_override).html_label_wrapping_width(),
            120.0
        );

        let numeric = json!({
            "flowchart": {
                "wrappingWidth": 320
            }
        });
        assert_eq!(
            StateConfigView::new(&numeric).html_label_wrapping_width(),
            320.0
        );

        let px_string = json!({
            "flowchart": {
                "wrappingWidth": "280px"
            }
        });
        assert_eq!(
            StateConfigView::new(&px_string).html_label_wrapping_width(),
            280.0
        );

        let fallback = json!({});
        assert_eq!(
            StateConfigView::new(&fallback).html_label_wrapping_width(),
            200.0
        );
    }

    #[test]
    fn state_layout_settings_use_root_html_labels_before_deprecated_flowchart_fallback() {
        let root_false = json!({
            "htmlLabels": false,
            "flowchart": { "htmlLabels": true }
        });
        let settings = StateConfigView::new(&root_false).layout_settings("TB");
        assert!(!settings.html_labels);
        assert_eq!(settings.wrap_mode, WrapMode::SvgLike);

        let root_true = json!({
            "htmlLabels": true,
            "flowchart": { "htmlLabels": false }
        });
        let settings = StateConfigView::new(&root_true).layout_settings("TB");
        assert!(settings.html_labels);
        assert_eq!(settings.wrap_mode, WrapMode::HtmlLike);

        let deprecated_false = json!({
            "flowchart": { "htmlLabels": false }
        });
        let settings = StateConfigView::new(&deprecated_false).layout_settings("TB");
        assert!(!settings.html_labels);
        assert_eq!(settings.wrap_mode, WrapMode::SvgLike);
    }

    #[test]
    fn state_layout_settings_use_root_spacing_before_renderer_spacing() {
        let cfg = json!({
            "nodeSpacing": 90,
            "rankSpacing": "91",
            "state": {
                "nodeSpacing": 70,
                "rankSpacing": 80
            },
            "flowchart": {
                "nodeSpacing": 11,
                "rankSpacing": 12
            }
        });
        let settings = StateConfigView::new(&cfg).layout_settings("TB");

        assert_eq!(settings.graph.nodesep, 90.0);
        assert_eq!(settings.graph.ranksep, 91.0);
    }

    #[test]
    fn state_layout_settings_project_dagre_wrap_padding_and_text_style() {
        let cfg = json!({
            "fontFamily": "Root Sans",
            "themeVariables": {
                "fontFamily": "Theme Sans",
                "fontSize": "24px"
            },
            "flowchart": {
                "htmlLabels": false,
                "wrappingWidth": "260px"
            },
            "state": {
                "nodeSpacing": "70",
                "rankSpacing": 80,
                "padding": "9"
            }
        });

        let settings = StateConfigView::new(&cfg).layout_settings("LR");

        assert_eq!(settings.graph.rankdir, RankDir::LR);
        assert_eq!(settings.graph.nodesep, 70.0);
        assert_eq!(settings.graph.ranksep, 80.0);
        assert_eq!(settings.wrap_mode, WrapMode::SvgLike);
        assert_eq!(settings.wrapping_width, 260.0);
        assert_eq!(settings.state_padding, 9.0);
        assert_eq!(
            settings.text_style.font_family.as_deref(),
            Some("Root Sans")
        );
        assert_eq!(settings.text_style.font_size, 24.0);
    }

    #[test]
    fn state_min_node_width_uses_mermaid_default_but_preserves_explicit_zero() {
        assert_eq!(
            StateConfigView::new(&json!({})).state_label_min_width(),
            120.0
        );
        assert_eq!(
            StateConfigView::new(&json!({"state": {"minNodeWidth": 0}})).state_label_min_width(),
            0.0
        );
        assert_eq!(
            StateConfigView::new(&json!({"state": {"minNodeWidth": 132}})).state_label_min_width(),
            132.0
        );
    }

    #[test]
    fn state_render_settings_project_svg_only_config() {
        let cfg = json!({
            "look": "neo",
            "handDrawnSeed": 42,
            "securityLevel": "loose",
            "htmlLabels": false,
            "flowchart": {
                "wrappingWidth": 0
            },
            "state": {
                "padding": "-4",
                "titleTopMargin": "-8"
            }
        });

        let settings = StateConfigView::new(&cfg).render_settings();

        assert_eq!(settings.diagram_look, "neo");
        assert_eq!(settings.hand_drawn_seed, 42.0);
        assert!(!settings.html_labels);
        assert!(settings.security_level_loose);
        assert_eq!(settings.html_label_wrapping_width, 0.0);
        assert_eq!(settings.state_padding, 0.0);
        assert_eq!(settings.title_top_margin, 0.0);
    }

    #[test]
    fn state_entity_decode_handles_mermaid_placeholders_and_colon_entity() {
        assert_eq!(
            super::decode_html_entities_once("test({ fooﬂ°colon¶ß 'far' })"),
            "test({ foo: 'far' })"
        );
        assert_eq!(
            super::decode_html_entities_once("test({ foo&colon; 'far' })"),
            "test({ foo: 'far' })"
        );
    }
}
