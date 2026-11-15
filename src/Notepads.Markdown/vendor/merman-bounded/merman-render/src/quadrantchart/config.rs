use crate::config::{config_bool, config_f64, config_string};
use serde_json::Value;

const DEFAULT_CHART_WIDTH: f64 = 500.0;
const DEFAULT_CHART_HEIGHT: f64 = 500.0;
const DEFAULT_TITLE_PADDING: f64 = 10.0;
const DEFAULT_TITLE_FONT_SIZE: f64 = 20.0;
const DEFAULT_QUADRANT_PADDING: f64 = 5.0;
const DEFAULT_X_AXIS_LABEL_PADDING: f64 = 5.0;
const DEFAULT_Y_AXIS_LABEL_PADDING: f64 = 5.0;
const DEFAULT_X_AXIS_LABEL_FONT_SIZE: f64 = 16.0;
const DEFAULT_Y_AXIS_LABEL_FONT_SIZE: f64 = 16.0;
const DEFAULT_QUADRANT_LABEL_FONT_SIZE: f64 = 16.0;
const DEFAULT_QUADRANT_TEXT_TOP_PADDING: f64 = 5.0;
const DEFAULT_POINT_TEXT_PADDING: f64 = 5.0;
const DEFAULT_POINT_LABEL_FONT_SIZE: f64 = 12.0;
const DEFAULT_POINT_RADIUS: f64 = 5.0;
const DEFAULT_X_AXIS_POSITION: &str = "top";
const DEFAULT_Y_AXIS_POSITION: &str = "left";
const DEFAULT_QUADRANT_INTERNAL_BORDER_STROKE_WIDTH: f64 = 1.0;
const DEFAULT_QUADRANT_EXTERNAL_BORDER_STROKE_WIDTH: f64 = 2.0;
const DEFAULT_USE_MAX_WIDTH: bool = true;

pub(crate) struct QuadrantChartConfigView<'a> {
    quadrant_config: &'a Value,
    has_render_config: bool,
}

impl<'a> QuadrantChartConfigView<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        let quadrant_config = effective_config
            .get("quadrantChart")
            .unwrap_or(&Value::Null);
        Self {
            quadrant_config,
            has_render_config: effective_config
                .get("quadrantChart")
                .is_some_and(|cfg| !has_ref_object(cfg)),
        }
    }

    pub(crate) fn layout_settings(&self) -> QuadrantChartLayoutSettings {
        QuadrantChartLayoutSettings {
            chart_width: self
                .quadrant_f64("chartWidth")
                .unwrap_or(DEFAULT_CHART_WIDTH),
            chart_height: self
                .quadrant_f64("chartHeight")
                .unwrap_or(DEFAULT_CHART_HEIGHT),
            title_padding: self
                .quadrant_f64("titlePadding")
                .unwrap_or(DEFAULT_TITLE_PADDING),
            title_font_size: self
                .quadrant_f64("titleFontSize")
                .unwrap_or(DEFAULT_TITLE_FONT_SIZE),
            quadrant_padding: self
                .quadrant_f64("quadrantPadding")
                .unwrap_or(DEFAULT_QUADRANT_PADDING),
            x_axis_label_padding: self
                .quadrant_f64("xAxisLabelPadding")
                .unwrap_or(DEFAULT_X_AXIS_LABEL_PADDING),
            y_axis_label_padding: self
                .quadrant_f64("yAxisLabelPadding")
                .unwrap_or(DEFAULT_Y_AXIS_LABEL_PADDING),
            x_axis_label_font_size: self
                .quadrant_f64("xAxisLabelFontSize")
                .unwrap_or(DEFAULT_X_AXIS_LABEL_FONT_SIZE),
            y_axis_label_font_size: self
                .quadrant_f64("yAxisLabelFontSize")
                .unwrap_or(DEFAULT_Y_AXIS_LABEL_FONT_SIZE),
            quadrant_label_font_size: self
                .quadrant_f64("quadrantLabelFontSize")
                .unwrap_or(DEFAULT_QUADRANT_LABEL_FONT_SIZE),
            quadrant_text_top_padding: self
                .quadrant_f64("quadrantTextTopPadding")
                .unwrap_or(DEFAULT_QUADRANT_TEXT_TOP_PADDING),
            point_text_padding: self
                .quadrant_f64("pointTextPadding")
                .unwrap_or(DEFAULT_POINT_TEXT_PADDING),
            point_label_font_size: self
                .quadrant_f64("pointLabelFontSize")
                .unwrap_or(DEFAULT_POINT_LABEL_FONT_SIZE),
            point_radius: self
                .quadrant_f64("pointRadius")
                .unwrap_or(DEFAULT_POINT_RADIUS),
            x_axis_position: self
                .quadrant_string("xAxisPosition")
                .unwrap_or_else(|| DEFAULT_X_AXIS_POSITION.to_string()),
            y_axis_position: self
                .quadrant_string("yAxisPosition")
                .unwrap_or_else(|| DEFAULT_Y_AXIS_POSITION.to_string()),
            quadrant_internal_border_stroke_width: self
                .quadrant_f64("quadrantInternalBorderStrokeWidth")
                .unwrap_or(DEFAULT_QUADRANT_INTERNAL_BORDER_STROKE_WIDTH),
            quadrant_external_border_stroke_width: self
                .quadrant_f64("quadrantExternalBorderStrokeWidth")
                .unwrap_or(DEFAULT_QUADRANT_EXTERNAL_BORDER_STROKE_WIDTH),
        }
    }

    pub(crate) fn render_settings(&self) -> QuadrantChartRenderSettings {
        QuadrantChartRenderSettings {
            use_max_width: self
                .render_bool("useMaxWidth")
                .unwrap_or(DEFAULT_USE_MAX_WIDTH),
        }
    }

    fn quadrant_bool(&self, key: &str) -> Option<bool> {
        config_bool(self.quadrant_config, &[key])
    }

    fn quadrant_f64(&self, key: &str) -> Option<f64> {
        config_f64(self.quadrant_config, &[key])
    }

    fn quadrant_string(&self, key: &str) -> Option<String> {
        config_string(self.quadrant_config, &[key])
    }

    fn render_bool(&self, key: &str) -> Option<bool> {
        self.has_render_config
            .then(|| self.quadrant_bool(key))
            .flatten()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QuadrantChartLayoutSettings {
    pub(crate) chart_width: f64,
    pub(crate) chart_height: f64,
    pub(crate) title_padding: f64,
    pub(crate) title_font_size: f64,
    pub(crate) quadrant_padding: f64,
    pub(crate) x_axis_label_padding: f64,
    pub(crate) y_axis_label_padding: f64,
    pub(crate) x_axis_label_font_size: f64,
    pub(crate) y_axis_label_font_size: f64,
    pub(crate) quadrant_label_font_size: f64,
    pub(crate) quadrant_text_top_padding: f64,
    pub(crate) point_text_padding: f64,
    pub(crate) point_label_font_size: f64,
    pub(crate) point_radius: f64,
    pub(crate) x_axis_position: String,
    pub(crate) y_axis_position: String,
    pub(crate) quadrant_internal_border_stroke_width: f64,
    pub(crate) quadrant_external_border_stroke_width: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct QuadrantChartRenderSettings {
    pub(crate) use_max_width: bool,
}

fn has_ref_object(v: &Value) -> bool {
    v.as_object().is_some_and(|m| m.contains_key("$ref"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quadrantchart_layout_settings_preserve_defaults() {
        let cfg = json!({});
        let settings = QuadrantChartConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.chart_width, DEFAULT_CHART_WIDTH);
        assert_eq!(settings.chart_height, DEFAULT_CHART_HEIGHT);
        assert_eq!(settings.title_padding, DEFAULT_TITLE_PADDING);
        assert_eq!(settings.title_font_size, DEFAULT_TITLE_FONT_SIZE);
        assert_eq!(settings.quadrant_padding, DEFAULT_QUADRANT_PADDING);
        assert_eq!(settings.x_axis_label_padding, DEFAULT_X_AXIS_LABEL_PADDING);
        assert_eq!(settings.y_axis_label_padding, DEFAULT_Y_AXIS_LABEL_PADDING);
        assert_eq!(
            settings.x_axis_label_font_size,
            DEFAULT_X_AXIS_LABEL_FONT_SIZE
        );
        assert_eq!(
            settings.y_axis_label_font_size,
            DEFAULT_Y_AXIS_LABEL_FONT_SIZE
        );
        assert_eq!(
            settings.quadrant_label_font_size,
            DEFAULT_QUADRANT_LABEL_FONT_SIZE
        );
        assert_eq!(
            settings.quadrant_text_top_padding,
            DEFAULT_QUADRANT_TEXT_TOP_PADDING
        );
        assert_eq!(settings.point_text_padding, DEFAULT_POINT_TEXT_PADDING);
        assert_eq!(
            settings.point_label_font_size,
            DEFAULT_POINT_LABEL_FONT_SIZE
        );
        assert_eq!(settings.point_radius, DEFAULT_POINT_RADIUS);
        assert_eq!(settings.x_axis_position, DEFAULT_X_AXIS_POSITION);
        assert_eq!(settings.y_axis_position, DEFAULT_Y_AXIS_POSITION);
        assert_eq!(
            settings.quadrant_internal_border_stroke_width,
            DEFAULT_QUADRANT_INTERNAL_BORDER_STROKE_WIDTH
        );
        assert_eq!(
            settings.quadrant_external_border_stroke_width,
            DEFAULT_QUADRANT_EXTERNAL_BORDER_STROKE_WIDTH
        );
    }

    #[test]
    fn quadrantchart_layout_settings_project_configured_values() {
        let cfg = json!({
            "quadrantChart": {
                "chartWidth": 640,
                "chartHeight": "360",
                "titlePadding": 12,
                "titleFontSize": "22",
                "quadrantPadding": 8,
                "xAxisLabelPadding": 9,
                "yAxisLabelPadding": "10",
                "xAxisLabelFontSize": 17,
                "yAxisLabelFontSize": "18",
                "quadrantLabelFontSize": 19,
                "quadrantTextTopPadding": "6",
                "pointTextPadding": 7,
                "pointLabelFontSize": "13",
                "pointRadius": 11,
                "xAxisPosition": "bottom",
                "yAxisPosition": "right",
                "quadrantInternalBorderStrokeWidth": "3",
                "quadrantExternalBorderStrokeWidth": 4
            }
        });
        let settings = QuadrantChartConfigView::new(&cfg).layout_settings();

        assert_eq!(settings.chart_width, 640.0);
        assert_eq!(settings.chart_height, 360.0);
        assert_eq!(settings.title_padding, 12.0);
        assert_eq!(settings.title_font_size, 22.0);
        assert_eq!(settings.quadrant_padding, 8.0);
        assert_eq!(settings.x_axis_label_padding, 9.0);
        assert_eq!(settings.y_axis_label_padding, 10.0);
        assert_eq!(settings.x_axis_label_font_size, 17.0);
        assert_eq!(settings.y_axis_label_font_size, 18.0);
        assert_eq!(settings.quadrant_label_font_size, 19.0);
        assert_eq!(settings.quadrant_text_top_padding, 6.0);
        assert_eq!(settings.point_text_padding, 7.0);
        assert_eq!(settings.point_label_font_size, 13.0);
        assert_eq!(settings.point_radius, 11.0);
        assert_eq!(settings.x_axis_position, "bottom");
        assert_eq!(settings.y_axis_position, "right");
        assert_eq!(settings.quadrant_internal_border_stroke_width, 3.0);
        assert_eq!(settings.quadrant_external_border_stroke_width, 4.0);
    }

    #[test]
    fn quadrantchart_render_settings_project_use_max_width() {
        let cfg = json!({
            "quadrantChart": {
                "useMaxWidth": false
            }
        });
        let settings = QuadrantChartConfigView::new(&cfg).render_settings();

        assert!(!settings.use_max_width);
    }

    #[test]
    fn quadrantchart_ref_config_uses_render_default_but_keeps_layout_projection() {
        let cfg = json!({
            "quadrantChart": {
                "$ref": "#/defs/quadrantChart",
                "useMaxWidth": false,
                "chartWidth": 640
            }
        });
        let view = QuadrantChartConfigView::new(&cfg);

        assert!(view.render_settings().use_max_width);
        assert_eq!(view.layout_settings().chart_width, 640.0);
    }
}
