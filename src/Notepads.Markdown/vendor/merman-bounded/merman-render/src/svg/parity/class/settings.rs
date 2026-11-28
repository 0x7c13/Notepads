use super::*;

pub(super) struct ClassRenderSettings {
    pub(super) diagram_use_html_labels: bool,
    pub(super) edge_use_html_labels: bool,
    pub(super) font_size_css: String,
    pub(super) wrap_probe_font_size: f64,
    pub(super) html_calc_text_style: TextStyle,
    pub(super) line_height: f64,
    pub(super) class_padding: f64,
    pub(super) text_style: TextStyle,
    pub(super) viewport_padding: f64,
    pub(super) hide_empty_members_box: bool,
    pub(super) default_node_fill: String,
    pub(super) default_node_stroke: String,
    pub(super) security_level_loose: bool,
    pub(super) look: String,
    pub(super) hand_drawn_seed: roughr::core::RoughRandomness,
}

impl ClassRenderSettings {
    pub(super) fn from_config(
        effective_config: &serde_json::Value,
        hand_drawn_seed: roughr::core::RoughRandomness,
    ) -> Self {
        let config = crate::class::config::ClassConfigView::new(effective_config);

        let diagram_use_html_labels = config.render_diagram_html_labels();
        let edge_use_html_labels = config.render_edge_html_labels();
        let font_size = config.render_font_size();
        let font_size_css = config.render_font_size_css();
        let wrap_probe_font_size = config.wrap_probe_font_size();
        let html_calc_text_style = config.html_calculate_text_style();
        let line_height = font_size * 1.5;
        let class_padding = config.render_class_padding();
        let text_style = config.render_text_style(font_size);
        let viewport_padding = config.render_viewport_padding();
        let hide_empty_members_box = config.hide_empty_members_box();
        let default_node_fill = config.default_node_fill();
        let default_node_stroke = config.default_node_stroke();
        let security_level_loose = effective_config
            .get("securityLevel")
            .and_then(serde_json::Value::as_str)
            == Some("loose");
        let look = config.diagram_look();
        Self {
            diagram_use_html_labels,
            edge_use_html_labels,
            font_size_css,
            wrap_probe_font_size,
            html_calc_text_style,
            line_height,
            class_padding,
            text_style,
            viewport_padding,
            hide_empty_members_box,
            default_node_fill,
            default_node_stroke,
            security_level_loose,
            look,
            hand_drawn_seed,
        }
    }
}
