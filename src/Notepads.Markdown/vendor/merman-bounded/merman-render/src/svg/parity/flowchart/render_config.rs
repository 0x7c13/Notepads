//! Flowchart render configuration preparation.

use crate::config::config_f64;
use crate::flowchart::FlowchartConfigView;
use crate::presentation::FlowchartPresentationPolicy;
use crate::text::{TextStyle, WrapMode};

pub(in crate::svg::parity::flowchart) struct FlowchartRenderConfig {
    pub font_family: String,
    pub font_size: f64,
    pub wrapping_width: f64,
    pub node_html_labels: bool,
    pub edge_html_labels: bool,
    pub swimlane_title_html_labels: bool,
    pub node_wrap_mode: WrapMode,
    pub edge_wrap_mode: WrapMode,
    pub diagram_padding: f64,
    pub use_max_width: bool,
    pub title_top_margin: f64,
    pub node_padding: f64,
    pub text_style: TextStyle,
    pub html_label_text_style: TextStyle,
    pub default_edge_interpolate: String,
    pub default_edge_style: Vec<String>,
    pub node_border_color: String,
    pub node_fill_color: String,
    pub edge_corner_radius: f64,
    pub edge_label_padding: f64,
    pub compact_edge_corners: bool,
}

pub(in crate::svg::parity::flowchart) fn prepare_flowchart_render_config(
    model: &crate::flowchart::FlowchartModel,
    effective_config_value: &serde_json::Value,
    is_elk_layout: bool,
    presentation_policy: Option<FlowchartPresentationPolicy>,
) -> FlowchartRenderConfig {
    let config = FlowchartConfigView::new(effective_config_value);
    let font_family = config.font_family();
    let font_size = config.render_font_size();
    let wrapping_width = config.render_wrapping_width();
    let node_html_labels = config.node_html_labels();
    let edge_html_labels = config.effective_html_labels();
    let swimlane_title_html_labels = config.swimlane_title_html_labels();
    let node_wrap_mode = config.node_wrap_mode();
    let edge_wrap_mode = config.edge_wrap_mode();
    let diagram_padding = config.render_diagram_padding();
    let use_max_width = config.render_use_max_width();
    let title_top_margin = config.render_title_top_margin();
    let node_padding = config.render_node_padding();
    let text_style = config.render_text_style(&font_family, font_size);
    let html_label_text_style = config.html_label_measurement_base_style(&text_style);

    let cfg_curve = if is_elk_layout {
        Some("rounded".to_string())
    } else {
        config.render_curve()
    };
    let default_edge_interpolate = model
        .edge_defaults
        .as_ref()
        .and_then(|d| d.interpolate.as_deref())
        .or(cfg_curve.as_deref())
        .unwrap_or("basis")
        .to_string();
    let default_edge_style = model
        .edge_defaults
        .as_ref()
        .map(|d| d.style.clone())
        .unwrap_or_default();

    let node_border_color = config.theme_token("nodeBorder", "#9370DB");
    let node_fill_color = config.theme_token("mainBkg", "#ECECFF");
    let node_corner_radius = config_f64(effective_config_value, &["themeVariables", "radius"])
        .unwrap_or(5.0)
        .max(0.0);
    // Mermaid's edge renderer passes a fixed radius of 5 to generateRoundedPath;
    // the theme radius controls node corners. An explicit presentation may override it.
    let edge_corner_radius = presentation_policy
        .map(|policy| policy.edge_corner_radius.unwrap_or(node_corner_radius))
        .unwrap_or(5.0)
        .max(0.0);
    let presentation_policy = presentation_policy.unwrap_or_default();
    let edge_label_padding = presentation_policy.edge_label_padding.max(0.0);
    let compact_edge_corners = presentation_policy.compact_edge_corners;

    FlowchartRenderConfig {
        font_family,
        font_size,
        wrapping_width,
        node_html_labels,
        edge_html_labels,
        swimlane_title_html_labels,
        node_wrap_mode,
        edge_wrap_mode,
        diagram_padding,
        use_max_width,
        title_top_margin,
        node_padding,
        text_style,
        html_label_text_style,
        default_edge_interpolate,
        default_edge_style,
        node_border_color,
        node_fill_color,
        edge_corner_radius,
        edge_label_padding,
        compact_edge_corners,
    }
}
