use crate::Result;
use crate::model::{
    QuadrantChartAxisLabelData, QuadrantChartBorderLineData, QuadrantChartDiagramLayout,
    QuadrantChartPointData, QuadrantChartQuadrantData, QuadrantChartTextData,
};
use crate::text::TextMeasurer;
use crate::theme::PresentationTheme;
use merman_core::diagrams::quadrant_chart::QuadrantChartRenderModel;
use serde_json::Value;

mod config;

pub(crate) use config::QuadrantChartConfigView;

fn default_quadrant_theme(effective_config: &Value) -> crate::theme::QuadrantChartTheme {
    PresentationTheme::new(effective_config).quadrantchart()
}

fn scale_linear(domain: (f64, f64), range: (f64, f64), v: f64) -> f64 {
    let (d0, d1) = domain;
    let (r0, r1) = range;
    if d1 == d0 {
        return r0;
    }
    let t = (v - d0) / (d1 - d0);
    r0 + t * (r1 - r0)
}

pub(crate) fn layout_quadrantchart_diagram_typed(
    model: &QuadrantChartRenderModel,
    diagram_title: Option<&str>,
    effective_config: &Value,
    _text_measurer: &dyn TextMeasurer,
) -> Result<QuadrantChartDiagramLayout> {
    let cfg = QuadrantChartConfigView::new(effective_config).layout_settings();
    let theme = default_quadrant_theme(effective_config);

    let title_text = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| {
            diagram_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
        })
        .unwrap_or("");
    let show_title = !title_text.is_empty();

    let show_x_axis = !model.axes.x_axis_left_text.trim().is_empty()
        || !model.axes.x_axis_right_text.trim().is_empty();
    let show_y_axis = !model.axes.y_axis_top_text.trim().is_empty()
        || !model.axes.y_axis_bottom_text.trim().is_empty();

    let x_axis_position = if model.points.is_empty() {
        cfg.x_axis_position.as_str()
    } else {
        "bottom"
    };

    let x_axis_space_calc = cfg.x_axis_label_padding * 2.0 + cfg.x_axis_label_font_size;
    let x_axis_space_top = if x_axis_position == "top" && show_x_axis {
        x_axis_space_calc
    } else {
        0.0
    };
    let x_axis_space_bottom = if x_axis_position == "bottom" && show_x_axis {
        x_axis_space_calc
    } else {
        0.0
    };

    let y_axis_space_calc = cfg.y_axis_label_padding * 2.0 + cfg.y_axis_label_font_size;
    let y_axis_space_left = if cfg.y_axis_position == "left" && show_y_axis {
        y_axis_space_calc
    } else {
        0.0
    };
    let y_axis_space_right = if cfg.y_axis_position == "right" && show_y_axis {
        y_axis_space_calc
    } else {
        0.0
    };

    let title_space_top = if show_title {
        cfg.title_font_size + cfg.title_padding * 2.0
    } else {
        0.0
    };

    let quadrant_left = cfg.quadrant_padding + y_axis_space_left;
    let quadrant_top = cfg.quadrant_padding + x_axis_space_top + title_space_top;
    let quadrant_width =
        cfg.chart_width - cfg.quadrant_padding * 2.0 - y_axis_space_left - y_axis_space_right;
    let quadrant_height = cfg.chart_height
        - cfg.quadrant_padding * 2.0
        - x_axis_space_top
        - x_axis_space_bottom
        - title_space_top;
    let quadrant_half_width = quadrant_width / 2.0;
    let quadrant_half_height = quadrant_height / 2.0;

    let mut quadrants: Vec<QuadrantChartQuadrantData> = vec![
        QuadrantChartQuadrantData {
            x: quadrant_left + quadrant_half_width,
            y: quadrant_top,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme.quadrant1_fill.clone(),
            text: QuadrantChartTextData {
                text: model.quadrants.quadrant1_text.clone(),
                fill: theme.quadrant1_text_fill.clone(),
                x: 0.0,
                y: 0.0,
                font_size: cfg.quadrant_label_font_size,
                vertical_pos: "center".to_string(),
                horizontal_pos: "middle".to_string(),
                rotation: 0.0,
            },
        },
        QuadrantChartQuadrantData {
            x: quadrant_left,
            y: quadrant_top,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme.quadrant2_fill.clone(),
            text: QuadrantChartTextData {
                text: model.quadrants.quadrant2_text.clone(),
                fill: theme.quadrant2_text_fill.clone(),
                x: 0.0,
                y: 0.0,
                font_size: cfg.quadrant_label_font_size,
                vertical_pos: "center".to_string(),
                horizontal_pos: "middle".to_string(),
                rotation: 0.0,
            },
        },
        QuadrantChartQuadrantData {
            x: quadrant_left,
            y: quadrant_top + quadrant_half_height,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme.quadrant3_fill.clone(),
            text: QuadrantChartTextData {
                text: model.quadrants.quadrant3_text.clone(),
                fill: theme.quadrant3_text_fill.clone(),
                x: 0.0,
                y: 0.0,
                font_size: cfg.quadrant_label_font_size,
                vertical_pos: "center".to_string(),
                horizontal_pos: "middle".to_string(),
                rotation: 0.0,
            },
        },
        QuadrantChartQuadrantData {
            x: quadrant_left + quadrant_half_width,
            y: quadrant_top + quadrant_half_height,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme.quadrant4_fill.clone(),
            text: QuadrantChartTextData {
                text: model.quadrants.quadrant4_text.clone(),
                fill: theme.quadrant4_text_fill.clone(),
                x: 0.0,
                y: 0.0,
                font_size: cfg.quadrant_label_font_size,
                vertical_pos: "center".to_string(),
                horizontal_pos: "middle".to_string(),
                rotation: 0.0,
            },
        },
    ];
    for q in &mut quadrants {
        q.text.x = q.x + q.width / 2.0;
        if model.points.is_empty() {
            q.text.y = q.y + q.height / 2.0;
            q.text.horizontal_pos = "middle".to_string();
        } else {
            q.text.y = q.y + cfg.quadrant_text_top_padding;
            q.text.horizontal_pos = "top".to_string();
        }
    }

    let draw_x_axis_labels_in_middle = !model.axes.x_axis_right_text.trim().is_empty();
    let draw_y_axis_labels_in_middle = !model.axes.y_axis_top_text.trim().is_empty();

    let mut axis_labels: Vec<QuadrantChartAxisLabelData> = Vec::new();
    if !model.axes.x_axis_left_text.trim().is_empty() && show_x_axis {
        axis_labels.push(QuadrantChartAxisLabelData {
            text: model.axes.x_axis_left_text.clone(),
            fill: theme.quadrant_x_axis_text_fill.clone(),
            x: quadrant_left
                + if draw_x_axis_labels_in_middle {
                    quadrant_half_width / 2.0
                } else {
                    0.0
                },
            y: if x_axis_position == "top" {
                cfg.x_axis_label_padding + title_space_top
            } else {
                cfg.x_axis_label_padding + quadrant_top + quadrant_height + cfg.quadrant_padding
            },
            font_size: cfg.x_axis_label_font_size,
            vertical_pos: if draw_x_axis_labels_in_middle {
                "center".to_string()
            } else {
                "left".to_string()
            },
            horizontal_pos: "top".to_string(),
            rotation: 0.0,
        });
    }
    if !model.axes.x_axis_right_text.trim().is_empty() && show_x_axis {
        axis_labels.push(QuadrantChartAxisLabelData {
            text: model.axes.x_axis_right_text.clone(),
            fill: theme.quadrant_x_axis_text_fill.clone(),
            x: quadrant_left
                + quadrant_half_width
                + if draw_x_axis_labels_in_middle {
                    quadrant_half_width / 2.0
                } else {
                    0.0
                },
            y: if x_axis_position == "top" {
                cfg.x_axis_label_padding + title_space_top
            } else {
                cfg.x_axis_label_padding + quadrant_top + quadrant_height + cfg.quadrant_padding
            },
            font_size: cfg.x_axis_label_font_size,
            vertical_pos: if draw_x_axis_labels_in_middle {
                "center".to_string()
            } else {
                "left".to_string()
            },
            horizontal_pos: "top".to_string(),
            rotation: 0.0,
        });
    }
    if !model.axes.y_axis_bottom_text.trim().is_empty() && show_y_axis {
        axis_labels.push(QuadrantChartAxisLabelData {
            text: model.axes.y_axis_bottom_text.clone(),
            fill: theme.quadrant_y_axis_text_fill.clone(),
            x: if cfg.y_axis_position == "left" {
                cfg.y_axis_label_padding
            } else {
                cfg.y_axis_label_padding + quadrant_left + quadrant_width + cfg.quadrant_padding
            },
            y: quadrant_top + quadrant_height
                - if draw_y_axis_labels_in_middle {
                    quadrant_half_height / 2.0
                } else {
                    0.0
                },
            font_size: cfg.y_axis_label_font_size,
            vertical_pos: if draw_y_axis_labels_in_middle {
                "center".to_string()
            } else {
                "left".to_string()
            },
            horizontal_pos: "top".to_string(),
            rotation: -90.0,
        });
    }
    if !model.axes.y_axis_top_text.trim().is_empty() && show_y_axis {
        axis_labels.push(QuadrantChartAxisLabelData {
            text: model.axes.y_axis_top_text.clone(),
            fill: theme.quadrant_y_axis_text_fill.clone(),
            x: if cfg.y_axis_position == "left" {
                cfg.y_axis_label_padding
            } else {
                cfg.y_axis_label_padding + quadrant_left + quadrant_width + cfg.quadrant_padding
            },
            y: quadrant_top + quadrant_half_height
                - if draw_y_axis_labels_in_middle {
                    quadrant_half_height / 2.0
                } else {
                    0.0
                },
            font_size: cfg.y_axis_label_font_size,
            vertical_pos: if draw_y_axis_labels_in_middle {
                "center".to_string()
            } else {
                "left".to_string()
            },
            horizontal_pos: "top".to_string(),
            rotation: -90.0,
        });
    }

    let half_external_border_width = cfg.quadrant_external_border_stroke_width / 2.0;
    let border_lines = vec![
        QuadrantChartBorderLineData {
            stroke_fill: theme.quadrant_external_border_stroke_fill.clone(),
            stroke_width: cfg.quadrant_external_border_stroke_width,
            x1: quadrant_left - half_external_border_width,
            y1: quadrant_top,
            x2: quadrant_left + quadrant_width + half_external_border_width,
            y2: quadrant_top,
        },
        QuadrantChartBorderLineData {
            stroke_fill: theme.quadrant_external_border_stroke_fill.clone(),
            stroke_width: cfg.quadrant_external_border_stroke_width,
            x1: quadrant_left + quadrant_width,
            y1: quadrant_top + half_external_border_width,
            x2: quadrant_left + quadrant_width,
            y2: quadrant_top + quadrant_height - half_external_border_width,
        },
        QuadrantChartBorderLineData {
            stroke_fill: theme.quadrant_external_border_stroke_fill.clone(),
            stroke_width: cfg.quadrant_external_border_stroke_width,
            x1: quadrant_left - half_external_border_width,
            y1: quadrant_top + quadrant_height,
            x2: quadrant_left + quadrant_width + half_external_border_width,
            y2: quadrant_top + quadrant_height,
        },
        QuadrantChartBorderLineData {
            stroke_fill: theme.quadrant_external_border_stroke_fill.clone(),
            stroke_width: cfg.quadrant_external_border_stroke_width,
            x1: quadrant_left,
            y1: quadrant_top + half_external_border_width,
            x2: quadrant_left,
            y2: quadrant_top + quadrant_height - half_external_border_width,
        },
        QuadrantChartBorderLineData {
            stroke_fill: theme.quadrant_internal_border_stroke_fill.clone(),
            stroke_width: cfg.quadrant_internal_border_stroke_width,
            x1: quadrant_left + quadrant_half_width,
            y1: quadrant_top + half_external_border_width,
            x2: quadrant_left + quadrant_half_width,
            y2: quadrant_top + quadrant_height - half_external_border_width,
        },
        QuadrantChartBorderLineData {
            stroke_fill: theme.quadrant_internal_border_stroke_fill.clone(),
            stroke_width: cfg.quadrant_internal_border_stroke_width,
            x1: quadrant_left + half_external_border_width,
            y1: quadrant_top + quadrant_half_height,
            x2: quadrant_left + quadrant_width - half_external_border_width,
            y2: quadrant_top + quadrant_half_height,
        },
    ];

    let mut points: Vec<QuadrantChartPointData> = Vec::new();
    for p in &model.points {
        let class_styles = p
            .class_name
            .as_deref()
            .and_then(|name| model.classes.get(name));

        let radius = p
            .styles
            .radius
            .map(|v| v as f64)
            .or_else(|| class_styles.and_then(|c| c.radius.map(|v| v as f64)))
            .unwrap_or(cfg.point_radius);
        let fill = p
            .styles
            .color
            .clone()
            .or_else(|| class_styles.and_then(|c| c.color.clone()))
            .unwrap_or_else(|| theme.quadrant_point_fill.clone());
        let stroke_color = p
            .styles
            .stroke_color
            .clone()
            .or_else(|| class_styles.and_then(|c| c.stroke_color.clone()))
            .unwrap_or_else(|| theme.quadrant_point_fill.clone());
        let stroke_width = p
            .styles
            .stroke_width
            .clone()
            .or_else(|| class_styles.and_then(|c| c.stroke_width.clone()))
            .unwrap_or_else(|| "0px".to_string());

        let x = scale_linear(
            (0.0, 1.0),
            (quadrant_left, quadrant_width + quadrant_left),
            p.x,
        );
        let y = scale_linear(
            (0.0, 1.0),
            (quadrant_height + quadrant_top, quadrant_top),
            p.y,
        );
        points.push(QuadrantChartPointData {
            x,
            y,
            fill: fill.clone(),
            radius,
            stroke_color,
            stroke_width,
            text: QuadrantChartTextData {
                text: p.text.clone(),
                fill: theme.quadrant_point_text_fill.clone(),
                x,
                y: y + cfg.point_text_padding,
                font_size: cfg.point_label_font_size,
                vertical_pos: "center".to_string(),
                horizontal_pos: "top".to_string(),
                rotation: 0.0,
            },
        });
    }

    let title = if show_title {
        Some(QuadrantChartTextData {
            text: title_text.to_string(),
            fill: theme.quadrant_title_fill,
            font_size: cfg.title_font_size,
            horizontal_pos: "top".to_string(),
            vertical_pos: "center".to_string(),
            rotation: 0.0,
            y: cfg.title_padding,
            x: cfg.chart_width / 2.0,
        })
    } else {
        None
    };

    Ok(QuadrantChartDiagramLayout {
        width: cfg.chart_width,
        height: cfg.chart_height,
        title,
        quadrants,
        border_lines,
        points,
        axis_labels,
    })
}
