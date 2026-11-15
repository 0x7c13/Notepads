use crate::config::{config_bool, config_f64, config_string, normalize_css_font_family};
use crate::text::{TextStyle, WrapMode};
use serde_json::Value;

const DEFAULT_FLOWCHART_FONT_FAMILY: &str = r#""trebuchet ms",verdana,arial,sans-serif"#;
const DEFAULT_NODE_SPACING: f64 = 50.0;
const DEFAULT_RANK_SPACING: f64 = 50.0;
const DEFAULT_NODE_PADDING: f64 = 15.0;
const DEFAULT_STATE_PADDING: f64 = 8.0;
const DEFAULT_WRAPPING_WIDTH: f64 = 200.0;
const DEFAULT_DIAGRAM_PADDING: f64 = 8.0;
const DEFAULT_TITLE_TOP_MARGIN: f64 = 25.0;
const FIXED_CLUSTER_PADDING: f64 = 8.0;

// Mermaid `createText(...)` defaults its `width` argument to 200. Flowchart edge labels and
// markdown subgraph titles rely on that default instead of `flowchart.wrappingWidth`.
const FLOWCHART_FIXED_LABEL_WRAP_WIDTH: f64 = 200.0;

pub(crate) struct FlowchartConfigView<'a> {
    effective_config: &'a Value,
    flowchart_config: &'a Value,
    state_config: &'a Value,
    theme_variables: &'a Value,
}

impl<'a> FlowchartConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        Self {
            effective_config,
            flowchart_config: effective_config.get("flowchart").unwrap_or(&Value::Null),
            state_config: effective_config.get("state").unwrap_or(&Value::Null),
            theme_variables: effective_config
                .get("themeVariables")
                .unwrap_or(&Value::Null),
        }
    }

    pub(crate) fn layout_settings(&self) -> FlowchartLayoutSettings {
        let node_html_labels = self.node_html_labels();
        let edge_html_labels = self.effective_html_labels();
        let cluster_html_labels = edge_html_labels;
        let node_wrap_mode = flowchart_wrap_mode(node_html_labels);
        let edge_wrap_mode = flowchart_wrap_mode(edge_html_labels);
        let cluster_wrap_mode = flowchart_wrap_mode(cluster_html_labels);
        let title_margin_top = self.layout_subgraph_title_margin("top");
        let title_margin_bottom = self.layout_subgraph_title_margin("bottom");
        let title_total_margin = title_margin_top + title_margin_bottom;
        let text_style = self.layout_text_style();
        let html_label_text_style = self.html_label_measurement_base_style(&text_style);

        FlowchartLayoutSettings {
            nodesep: self.dagre_spacing_or_default("nodeSpacing", DEFAULT_NODE_SPACING),
            ranksep: self.dagre_spacing_or_default("rankSpacing", DEFAULT_RANK_SPACING),
            node_padding: self
                .flowchart_compat_f64("padding")
                .unwrap_or(DEFAULT_NODE_PADDING),
            state_padding: self
                .state_compat_f64("padding")
                .unwrap_or(DEFAULT_STATE_PADDING),
            wrapping_width: self
                .flowchart_compat_f64("wrappingWidth")
                .unwrap_or(DEFAULT_WRAPPING_WIDTH),
            edge_label_wrapping_width: FLOWCHART_FIXED_LABEL_WRAP_WIDTH,
            cluster_title_wrapping_width: FLOWCHART_FIXED_LABEL_WRAP_WIDTH,
            edge_html_labels,
            node_wrap_mode,
            edge_wrap_mode,
            cluster_wrap_mode,
            cluster_padding: FIXED_CLUSTER_PADDING,
            title_margin_top,
            title_margin_bottom,
            title_total_margin,
            y_shift: title_total_margin / 2.0,
            inherit_dir: self.flowchart_bool("inheritDir").unwrap_or(false),
            text_style,
            html_label_text_style,
        }
    }

    pub(crate) fn font_family(&self) -> String {
        let theme_font_family = self
            .theme_string("fontFamily")
            .map(|s| normalize_css_font_family(&s));
        let top_font_family = self
            .root_string("fontFamily")
            .map(|s| normalize_css_font_family(&s));

        match (top_font_family, theme_font_family) {
            (Some(top), Some(theme)) if theme == DEFAULT_FLOWCHART_FONT_FAMILY => top,
            (_, Some(theme)) => theme,
            (Some(top), None) => top,
            (None, None) => DEFAULT_FLOWCHART_FONT_FAMILY.to_string(),
        }
    }

    pub(crate) fn render_font_size(&self) -> f64 {
        self.theme_font_size_px().unwrap_or(16.0).max(1.0)
    }

    pub(crate) fn render_wrapping_width(&self) -> f64 {
        self.flowchart_compat_f64("wrappingWidth")
            .unwrap_or(DEFAULT_WRAPPING_WIDTH)
            .max(1.0)
    }

    pub(crate) fn render_diagram_padding(&self) -> f64 {
        self.flowchart_compat_f64("diagramPadding")
            .unwrap_or(DEFAULT_DIAGRAM_PADDING)
            .max(0.0)
    }

    pub(crate) fn render_use_max_width(&self) -> bool {
        self.flowchart_bool("useMaxWidth").unwrap_or(true)
    }

    pub(crate) fn render_title_top_margin(&self) -> f64 {
        self.flowchart_compat_f64("titleTopMargin")
            .unwrap_or(DEFAULT_TITLE_TOP_MARGIN)
            .max(0.0)
    }

    pub(crate) fn render_node_padding(&self) -> f64 {
        self.flowchart_compat_f64("padding")
            .unwrap_or(DEFAULT_NODE_PADDING)
            .max(0.0)
    }

    pub(crate) fn render_subgraph_title_y_shift(&self) -> f64 {
        let top = self.layout_subgraph_title_margin("top").max(0.0);
        let bottom = self.layout_subgraph_title_margin("bottom").max(0.0);
        (top + bottom) / 2.0
    }

    pub(crate) fn render_curve(&self) -> Option<String> {
        self.flowchart_string("curve")
    }

    pub(crate) fn render_text_style(&self, font_family: &str, font_size: f64) -> TextStyle {
        TextStyle {
            font_family: Some(font_family.to_string()),
            font_size,
            font_weight: None,
            font_style: None,
        }
    }

    pub(crate) fn node_html_labels(&self) -> bool {
        // Mermaid 11.15 node shapes use `evaluate(getConfig()?.htmlLabels)` in labelHelper, while
        // edge and cluster labels use `getEffectiveHtmlLabels(...)` and still honor the deprecated
        // `flowchart.htmlLabels` fallback.
        self.root_bool("htmlLabels").unwrap_or(true)
    }

    pub(crate) fn effective_html_labels(&self) -> bool {
        self.root_bool("htmlLabels")
            .or_else(|| self.flowchart_bool("htmlLabels"))
            .unwrap_or(true)
    }

    pub(crate) fn swimlane_title_html_labels(&self) -> bool {
        // Mermaid 11.16's dedicated Swimlane cluster renderer bypasses the root-level setting and
        // calls `evaluate(siteConfig.flowchart.htmlLabels)` directly.
        match self.flowchart_config.get("htmlLabels") {
            None => true,
            Some(Value::Bool(false) | Value::Null) => false,
            Some(Value::Number(value)) => value.as_f64() != Some(0.0),
            Some(Value::String(value)) => !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "false" | "null" | "0"
            ),
            Some(_) => true,
        }
    }

    pub(crate) fn node_wrap_mode(&self) -> WrapMode {
        flowchart_wrap_mode(self.node_html_labels())
    }

    pub(crate) fn edge_wrap_mode(&self) -> WrapMode {
        flowchart_wrap_mode(self.effective_html_labels())
    }

    pub(crate) fn html_label_measurement_base_style(&self, render_style: &TextStyle) -> TextStyle {
        let mut style = render_style.clone();
        // Mermaid serializes numeric themeVariables.fontSize into CSS without a unit
        // (`font-size:24`), which does not affect foreignObject HTML labels in Chromium. A CSS px
        // string (`"20px"`) is valid and does affect those labels.
        style.font_size = self
            .theme_variables
            .get("fontSize")
            .and_then(Value::as_str)
            .and_then(|raw| {
                let raw = raw.trim();
                if !raw.to_ascii_lowercase().ends_with("px") {
                    return None;
                }
                crate::mermaid_style::parse_css_font_size_px(raw, render_style.font_size)
            })
            .unwrap_or(16.0);
        style
    }

    pub(crate) fn theme_token(&self, key: &str, fallback: &str) -> String {
        self.theme_string(key)
            .unwrap_or_else(|| fallback.to_string())
    }

    fn layout_text_style(&self) -> TextStyle {
        TextStyle {
            font_family: Some(self.font_family()),
            font_size: self.theme_font_size_px().unwrap_or(16.0),
            font_weight: self.root_string("fontWeight"),
            font_style: None,
        }
    }

    fn dagre_spacing_or_default(&self, key: &str, default: f64) -> f64 {
        let Some(raw) = self.flowchart_config.get(key) else {
            return default;
        };

        // Mermaid Flowchart assigns `conf?.nodeSpacing || 50` and `conf?.rankSpacing || 50`,
        // so numeric zero falls back to the default instead of becoming a real Dagre separation.
        if raw.is_number() {
            return raw
                .as_f64()
                .filter(|value| *value != 0.0)
                .unwrap_or(default);
        }

        self.flowchart_compat_f64(key).unwrap_or(default)
    }

    fn theme_font_size_px(&self) -> Option<f64> {
        self.theme_variables
            .get("fontSize")
            .and_then(parse_font_size_px)
    }

    fn layout_subgraph_title_margin(&self, key: &str) -> f64 {
        self.flowchart_config
            .get("subGraphTitleMargin")
            .and_then(|margin| margin.get(key))
            .and_then(crate::config::json_f64)
            .unwrap_or(0.0)
    }

    fn root_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.effective_config, &[key])
    }

    fn flowchart_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.flowchart_config, &[key])
    }

    fn root_string(&self, key: &str) -> Option<String> {
        config_string(self.effective_config, &[key])
    }

    fn theme_string(&self, key: &str) -> Option<String> {
        config_string(self.theme_variables, &[key])
    }

    fn flowchart_string(&self, key: &str) -> Option<String> {
        config_string(self.flowchart_config, &[key])
    }

    fn flowchart_compat_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.flowchart_config, &[key])
    }

    fn state_compat_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.state_config, &[key])
    }
}

pub(crate) struct FlowchartLayoutSettings {
    pub(crate) nodesep: f64,
    pub(crate) ranksep: f64,
    pub(crate) node_padding: f64,
    pub(crate) state_padding: f64,
    pub(crate) wrapping_width: f64,
    pub(crate) edge_label_wrapping_width: f64,
    pub(crate) cluster_title_wrapping_width: f64,
    pub(crate) edge_html_labels: bool,
    pub(crate) node_wrap_mode: WrapMode,
    pub(crate) edge_wrap_mode: WrapMode,
    pub(crate) cluster_wrap_mode: WrapMode,
    pub(crate) cluster_padding: f64,
    pub(crate) title_margin_top: f64,
    pub(crate) title_margin_bottom: f64,
    pub(crate) title_total_margin: f64,
    pub(crate) y_shift: f64,
    pub(crate) inherit_dir: bool,
    pub(crate) text_style: TextStyle,
    pub(crate) html_label_text_style: TextStyle,
}

fn flowchart_wrap_mode(html_labels: bool) -> WrapMode {
    if html_labels {
        WrapMode::HtmlLike
    } else {
        WrapMode::SvgLike
    }
}

fn parse_font_size_px(v: &Value) -> Option<f64> {
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    if let Some(n) = v.as_i64() {
        return Some(n as f64);
    }
    if let Some(n) = v.as_u64() {
        return Some(n as f64);
    }
    let s = v.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    let mut num = String::new();
    for (idx, ch) in s.chars().enumerate() {
        if ch.is_ascii_digit() {
            num.push(ch);
            continue;
        }
        if idx == 0 && (ch == '-' || ch == '+') {
            num.push(ch);
            continue;
        }
        break;
    }
    if num.trim().is_empty() {
        return None;
    }
    num.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flowchart_layout_settings_preserve_dagre_spacing_compatibility() {
        let cfg = json!({
            "flowchart": {
                "nodeSpacing": 0,
                "rankSpacing": "70"
            }
        });

        let settings = FlowchartConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.nodesep, 50.0);
        assert_eq!(settings.ranksep, 70.0);
    }

    #[test]
    fn flowchart_layout_settings_preserve_html_label_split_semantics() {
        let cfg = json!({
            "htmlLabels": false,
            "flowchart": {
                "htmlLabels": true
            }
        });

        let config = FlowchartConfigView::new(&cfg);
        let settings = config.layout_settings();

        assert!(!config.node_html_labels());
        assert!(!config.effective_html_labels());
        assert_eq!(settings.node_wrap_mode, WrapMode::SvgLike);
        assert_eq!(settings.edge_wrap_mode, WrapMode::SvgLike);
        assert!(!settings.edge_html_labels);

        let cfg = json!({
            "flowchart": {
                "htmlLabels": false
            }
        });
        let config = FlowchartConfigView::new(&cfg);
        let settings = config.layout_settings();

        assert!(config.node_html_labels());
        assert!(!config.effective_html_labels());
        assert_eq!(settings.node_wrap_mode, WrapMode::HtmlLike);
        assert_eq!(settings.edge_wrap_mode, WrapMode::SvgLike);
    }

    #[test]
    fn swimlane_title_html_labels_follow_mermaid_evaluate_semantics() {
        for false_like in [
            json!(false),
            Value::Null,
            json!(0),
            json!(0.0),
            json!(-0.0),
            json!("false"),
            json!("null"),
            json!("0"),
        ] {
            let cfg = json!({ "flowchart": { "htmlLabels": false_like } });
            assert!(
                !FlowchartConfigView::new(&cfg).swimlane_title_html_labels(),
                "{cfg}"
            );
        }

        for true_like in [json!(true), json!(1), json!(-1), json!("true"), json!("1")] {
            let cfg = json!({ "flowchart": { "htmlLabels": true_like } });
            assert!(
                FlowchartConfigView::new(&cfg).swimlane_title_html_labels(),
                "{cfg}"
            );
        }
    }

    #[test]
    fn flowchart_font_family_preserves_theme_default_root_override() {
        let cfg = json!({
            "fontFamily": "Root Sans, Arial",
            "themeVariables": {
                "fontFamily": "\"trebuchet ms\", verdana, arial, sans-serif"
            }
        });

        let config = FlowchartConfigView::new(&cfg);

        assert_eq!(config.font_family(), "Root Sans,Arial");

        let cfg = json!({
            "fontFamily": "Root Sans",
            "themeVariables": {
                "fontFamily": "Theme Sans, Arial"
            }
        });

        assert_eq!(
            FlowchartConfigView::new(&cfg).font_family(),
            "Theme Sans,Arial"
        );
    }

    #[test]
    fn flowchart_font_size_keeps_legacy_leading_integer_parsing() {
        let cfg = json!({
            "themeVariables": {
                "fontSize": "24.5px"
            }
        });
        let settings = FlowchartConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.text_style.font_size, 24.0);
        assert_eq!(settings.html_label_text_style.font_size, 24.5);
    }

    #[test]
    fn flowchart_render_settings_keep_numeric_boundaries() {
        let cfg = json!({
            "themeVariables": {
                "fontSize": 0,
                "mainBkg": "#112233",
                "nodeBorder": "#445566"
            },
            "flowchart": {
                "wrappingWidth": 0,
                "diagramPadding": "-2",
                "useMaxWidth": false,
                "titleTopMargin": "-4",
                "padding": "-6",
                "curve": "linear",
                "subGraphTitleMargin": {
                    "top": "-3",
                    "bottom": "8"
                }
            }
        });
        let config = FlowchartConfigView::new(&cfg);

        assert_eq!(config.render_font_size(), 1.0);
        assert_eq!(config.render_wrapping_width(), 1.0);
        assert_eq!(config.render_diagram_padding(), 0.0);
        assert!(!config.render_use_max_width());
        assert_eq!(config.render_title_top_margin(), 0.0);
        assert_eq!(config.render_node_padding(), 0.0);
        assert_eq!(config.render_curve().as_deref(), Some("linear"));
        assert_eq!(config.render_subgraph_title_y_shift(), 4.0);
        assert_eq!(config.theme_token("mainBkg", "#ECECFF"), "#112233");
        assert_eq!(config.theme_token("nodeBorder", "#9370DB"), "#445566");
    }

    #[test]
    fn flowchart_layout_settings_keep_fixed_label_wrap_widths() {
        let cfg = json!({
            "flowchart": {
                "wrappingWidth": 320
            }
        });

        let settings = FlowchartConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.wrapping_width, 320.0);
        assert_eq!(
            settings.edge_label_wrapping_width,
            FLOWCHART_FIXED_LABEL_WRAP_WIDTH
        );
        assert_eq!(
            settings.cluster_title_wrapping_width,
            FLOWCHART_FIXED_LABEL_WRAP_WIDTH
        );
    }
}
