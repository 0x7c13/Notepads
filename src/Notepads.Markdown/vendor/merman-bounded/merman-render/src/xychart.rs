use crate::chart_palette::plot_color_from_palette;
use crate::config::{config_bool, config_f64_css_px as config_f64};
use crate::model::{
    XyChartDiagramLayout, XyChartDrawableElem, XyChartPathData, XyChartRectData, XyChartTextData,
};
use crate::text::{TextMeasurer, TextStyle};
use crate::theme::PresentationTheme;
use crate::{Error, Result};
use merman_core::diagrams::xychart::{
    XyChartAxisRenderModel, XyChartDiagramRenderModel, XyChartPlotRenderModel, XyChartPlotType,
};
use serde_json::Value;
use std::fmt::Write as _;

#[derive(Debug, Clone)]
struct AxisThemeConfig {
    title_color: String,
    label_color: String,
    tick_color: String,
    axis_line_color: String,
}

#[derive(Debug, Clone)]
struct AxisConfig {
    show_label: bool,
    label_font_size: f64,
    label_padding: f64,
    show_title: bool,
    title_font_size: f64,
    title_padding: f64,
    show_tick: bool,
    tick_length: f64,
    tick_width: f64,
    show_axis_line: bool,
    axis_line_width: f64,
    label_rotation: f64,
}

#[derive(Debug, Clone)]
struct ChartConfig {
    width: f64,
    height: f64,
    plot_reserved_space_percent: f64,
    show_data_label: bool,
    show_data_label_outside_bar: bool,
    show_title: bool,
    title_font_size: f64,
    title_padding: f64,
    show_legend: bool,
    legend_font_size: f64,
    legend_padding: f64,
    chart_orientation: String,
    x_axis: AxisConfig,
    y_axis: AxisConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisPosition {
    Left,
    Bottom,
    Top,
}

#[derive(Debug, Clone, Copy)]
struct Dimension {
    width: f64,
    height: f64,
}

type Point = merman_core::geom::Point;

fn pt(x: f64, y: f64) -> Point {
    merman_core::geom::point(x, y)
}

#[derive(Debug, Clone, Copy)]
struct BoundingRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn is_ref_only_object(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|m| m.len() == 1 && m.contains_key("$ref"))
}

fn default_axis_config() -> AxisConfig {
    AxisConfig {
        show_label: true,
        label_font_size: 14.0,
        label_padding: 5.0,
        show_title: true,
        title_font_size: 16.0,
        title_padding: 5.0,
        show_tick: true,
        tick_length: 5.0,
        tick_width: 2.0,
        show_axis_line: true,
        axis_line_width: 2.0,
        label_rotation: 0.0,
    }
}

fn parse_axis_config(effective_config: &Value, axis_key: &str) -> AxisConfig {
    let base = default_axis_config();
    let Some(v) = effective_config
        .get("xyChart")
        .and_then(|c| c.get(axis_key))
    else {
        return base;
    };
    // The default Mermaid config uses `$ref` placeholders (schema references) for axis configs.
    // When users override axis fields via directives/frontmatter, a deep-merge will keep the `$ref`
    // key while adding the override keys. Treat a *pure* `$ref` object as "no concrete config",
    // but do not discard user overrides just because `$ref` is present.
    if !v.is_object() || is_ref_only_object(v) {
        return base;
    }

    AxisConfig {
        show_label: config_bool(effective_config, &["xyChart", axis_key, "showLabel"])
            .unwrap_or(base.show_label),
        label_font_size: config_f64(effective_config, &["xyChart", axis_key, "labelFontSize"])
            .unwrap_or(base.label_font_size),
        label_padding: config_f64(effective_config, &["xyChart", axis_key, "labelPadding"])
            .unwrap_or(base.label_padding),
        show_title: config_bool(effective_config, &["xyChart", axis_key, "showTitle"])
            .unwrap_or(base.show_title),
        title_font_size: config_f64(effective_config, &["xyChart", axis_key, "titleFontSize"])
            .unwrap_or(base.title_font_size),
        title_padding: config_f64(effective_config, &["xyChart", axis_key, "titlePadding"])
            .unwrap_or(base.title_padding),
        show_tick: config_bool(effective_config, &["xyChart", axis_key, "showTick"])
            .unwrap_or(base.show_tick),
        tick_length: config_f64(effective_config, &["xyChart", axis_key, "tickLength"])
            .unwrap_or(base.tick_length),
        tick_width: config_f64(effective_config, &["xyChart", axis_key, "tickWidth"])
            .unwrap_or(base.tick_width),
        show_axis_line: config_bool(effective_config, &["xyChart", axis_key, "showAxisLine"])
            .unwrap_or(base.show_axis_line),
        axis_line_width: config_f64(effective_config, &["xyChart", axis_key, "axisLineWidth"])
            .unwrap_or(base.axis_line_width),
        label_rotation: config_f64(effective_config, &["xyChart", axis_key, "labelRotation"])
            .unwrap_or(base.label_rotation),
    }
}

fn parse_chart_config(effective_config: &Value, model: &XyChartDiagramRenderModel) -> ChartConfig {
    ChartConfig {
        width: config_f64(effective_config, &["xyChart", "width"]).unwrap_or(700.0),
        height: config_f64(effective_config, &["xyChart", "height"]).unwrap_or(500.0),
        plot_reserved_space_percent: config_f64(
            effective_config,
            &["xyChart", "plotReservedSpacePercent"],
        )
        .unwrap_or(50.0),
        show_data_label: config_bool(effective_config, &["xyChart", "showDataLabel"])
            .unwrap_or(false),
        show_data_label_outside_bar: config_bool(
            effective_config,
            &["xyChart", "showDataLabelOutsideBar"],
        )
        .unwrap_or(false),
        show_title: config_bool(effective_config, &["xyChart", "showTitle"]).unwrap_or(true),
        title_font_size: config_f64(effective_config, &["xyChart", "titleFontSize"])
            .unwrap_or(20.0),
        title_padding: config_f64(effective_config, &["xyChart", "titlePadding"]).unwrap_or(10.0),
        show_legend: config_bool(effective_config, &["xyChart", "showLegend"]).unwrap_or(true),
        legend_font_size: config_f64(effective_config, &["xyChart", "legendFontSize"])
            .unwrap_or(14.0)
            .max(1.0),
        legend_padding: config_f64(effective_config, &["xyChart", "legendPadding"])
            .unwrap_or(10.0)
            .max(0.0),
        chart_orientation: match model.orientation.as_str() {
            "horizontal" => "horizontal".to_string(),
            _ => "vertical".to_string(),
        },
        x_axis: parse_axis_config(effective_config, "xAxis"),
        y_axis: parse_axis_config(effective_config, "yAxis"),
    }
}

fn max_text_dimension(texts: &[String], font_size: f64, measurer: &dyn TextMeasurer) -> Dimension {
    let style = TextStyle {
        font_size,
        ..Default::default()
    };
    let mut max_w: f64 = 0.0;
    let mut max_h: f64 = 0.0;
    if texts.is_empty() {
        return Dimension {
            width: 0.0,
            height: 0.0,
        };
    }
    // These metrics and the chart dimensions already share SVG user units. Mermaid 12.1
    // divides browser screen measurements by the SVG's CTM; no display scale enters this path.
    for t in texts {
        let m = measurer.measure(t, &style);
        max_w = max_w.max(m.width);
        // Mermaid XYChart uses `computeDimensionOfText(...)` which probes a `<tspan>`'s
        // `getBoundingClientRect().height`. That tends to be closer to "simple text bbox height"
        // than the slightly taller wrapped SVG `<text>.getBBox().height` heuristic.
        max_h = max_h.max(measurer.measure_svg_simple_text_bbox_height_px(t, &style));
    }
    Dimension {
        width: max_w,
        height: max_h,
    }
}

fn single_text_height(text: &str, font_size: f64, measurer: &dyn TextMeasurer) -> f64 {
    if text.trim().is_empty() {
        return 0.0;
    }
    if text.contains('\n') || text.contains("<br") {
        return max_text_dimension(&[text.to_string()], font_size, measurer).height;
    }
    let style = TextStyle {
        font_size,
        ..Default::default()
    };
    measurer.measure_svg_simple_text_bbox_height_px(text, &style)
}

const LEGEND_MARKER_TO_FONT_RATIO: f64 = 0.75;
const LEGEND_ITEM_SPACING_TO_FONT_RATIO: f64 = 0.5;
const LEGEND_MARKER_SPACING_TO_FONT_RATIO: f64 = 0.35;

fn calculate_legend_space(
    plots: &[(usize, &XyChartPlotRenderModel)],
    chart_config: &ChartConfig,
    available_space: Dimension,
    measurer: &dyn TextMeasurer,
) -> Dimension {
    if plots.is_empty() {
        return Dimension {
            width: 0.0,
            height: 0.0,
        };
    }

    let titles = plots
        .iter()
        .filter_map(|(_, plot)| plot.title.clone())
        .collect::<Vec<_>>();
    let text_dimension = max_text_dimension(&titles, chart_config.legend_font_size, measurer);
    let marker_size = chart_config.legend_font_size * LEGEND_MARKER_TO_FONT_RATIO;
    let marker_spacing = chart_config.legend_font_size * LEGEND_MARKER_SPACING_TO_FONT_RATIO;
    let item_spacing = chart_config.legend_font_size * LEGEND_ITEM_SPACING_TO_FONT_RATIO;
    let width =
        chart_config.legend_padding * 2.0 + marker_size + marker_spacing + text_dimension.width;
    let height = chart_config.legend_padding * 2.0
        + plots.len() as f64 * chart_config.legend_font_size
        + plots.len().saturating_sub(1) as f64 * item_spacing;

    if width <= available_space.width && height <= available_space.height {
        Dimension { width, height }
    } else {
        Dimension {
            width: 0.0,
            height: 0.0,
        }
    }
}

fn legend_drawable_elements(
    plots: &[(usize, &XyChartPlotRenderModel)],
    origin: Point,
    chart_config: &ChartConfig,
    legend_text_color: &str,
    plot_color_palette: &[String],
) -> Vec<XyChartDrawableElem> {
    if plots.is_empty() {
        return Vec::new();
    }

    let marker_size = chart_config.legend_font_size * LEGEND_MARKER_TO_FONT_RATIO;
    let marker_spacing = chart_config.legend_font_size * LEGEND_MARKER_SPACING_TO_FONT_RATIO;
    let item_spacing = chart_config.legend_font_size * LEGEND_ITEM_SPACING_TO_FONT_RATIO;
    let row_height = chart_config.legend_font_size + item_spacing;
    let start_x = origin.x + chart_config.legend_padding;
    let start_y = origin.y + chart_config.legend_padding;
    let mut bar_markers = Vec::new();
    let mut line_markers = Vec::new();
    let mut labels = Vec::with_capacity(plots.len());

    for (legend_index, (plot_index, plot)) in plots.iter().enumerate() {
        let color = plot_color_from_palette(plot_color_palette, *plot_index);
        match plot.plot_type {
            XyChartPlotType::Bar => bar_markers.push(XyChartRectData {
                x: start_x,
                y: start_y + legend_index as f64 * row_height,
                width: marker_size,
                height: marker_size,
                fill: color.clone(),
                stroke_fill: color.clone(),
                stroke_width: 0.0,
            }),
            XyChartPlotType::Line => {
                let marker_y = start_y + legend_index as f64 * row_height + marker_size / 2.0;
                line_markers.push(XyChartPathData {
                    path: format!(
                        "M {start_x},{marker_y} L {},{marker_y}",
                        start_x + marker_size
                    ),
                    fill: None,
                    stroke_fill: color,
                    stroke_width: 2.0,
                });
            }
        }
        labels.push(XyChartTextData {
            text: plot.title.clone().unwrap_or_default(),
            x: start_x + marker_size + marker_spacing,
            y: start_y + legend_index as f64 * row_height + marker_size / 2.0,
            fill: legend_text_color.to_string(),
            font_size: chart_config.legend_font_size,
            rotation: 0.0,
            vertical_pos: "middle".to_string(),
            horizontal_pos: "left".to_string(),
        });
    }

    vec![
        XyChartDrawableElem::Rect {
            group_texts: vec!["legend".to_string(), "markers".to_string()],
            data: bar_markers,
        },
        XyChartDrawableElem::Path {
            group_texts: vec!["legend".to_string(), "markers".to_string()],
            data: line_markers,
        },
        XyChartDrawableElem::Text {
            group_texts: vec!["legend".to_string(), "label".to_string()],
            data: labels,
        },
    ]
}

fn d3_ticks(start: f64, stop: f64, count: usize) -> Vec<f64> {
    fn tick_spec(start: f64, stop: f64, count: f64) -> Option<(i64, i64, f64)> {
        if count <= 0.0 {
            return None;
        }

        let step = (stop - start) / count.max(0.0);
        if !step.is_finite() || step == 0.0 {
            return None;
        }
        let power = step.log10().floor();
        let error = step / 10f64.powf(power);
        let e10 = 50f64.sqrt();
        let e5 = 10f64.sqrt();
        let e2 = 2f64.sqrt();
        let factor = if error >= e10 {
            10.0
        } else if error >= e5 {
            5.0
        } else if error >= e2 {
            2.0
        } else {
            1.0
        };

        let (i1, i2, inc) = if power < 0.0 {
            let inc = 10f64.powf(-power) / factor;
            let mut i1 = (start * inc).round() as i64;
            let mut i2 = (stop * inc).round() as i64;
            if (i1 as f64) / inc < start {
                i1 += 1;
            }
            if (i2 as f64) / inc > stop {
                i2 -= 1;
            }
            (i1, i2, -inc)
        } else {
            let inc = 10f64.powf(power) * factor;
            let mut i1 = (start / inc).round() as i64;
            let mut i2 = (stop / inc).round() as i64;
            if (i1 as f64) * inc < start {
                i1 += 1;
            }
            if (i2 as f64) * inc > stop {
                i2 -= 1;
            }
            (i1, i2, inc)
        };

        if i2 < i1 && (0.5..2.0).contains(&count) {
            return tick_spec(start, stop, count * 2.0);
        }

        if !inc.is_finite() {
            return None;
        }
        if inc == 0.0 {
            return None;
        }

        Some((i1, i2, inc))
    }

    if !start.is_finite() || !stop.is_finite() {
        return Vec::new();
    }
    let count = count as f64;
    if count <= 0.0 {
        return Vec::new();
    }
    if start == stop {
        return vec![start];
    }

    let reverse = stop < start;
    let (a, b) = if reverse {
        (stop, start)
    } else {
        (start, stop)
    };
    let Some((i1, i2, inc)) = tick_spec(a, b, count) else {
        return Vec::new();
    };
    if i2 < i1 {
        return Vec::new();
    }

    let n = (i2 - i1 + 1).max(0) as usize;
    let mut out = Vec::with_capacity(n);

    if reverse {
        if inc < 0.0 {
            for i in 0..n {
                out.push((i2 - i as i64) as f64 / -inc);
            }
        } else {
            for i in 0..n {
                out.push((i2 - i as i64) as f64 * inc);
            }
        }
    } else if inc < 0.0 {
        for i in 0..n {
            out.push((i1 + i as i64) as f64 / -inc);
        }
    } else {
        for i in 0..n {
            out.push((i1 + i as i64) as f64 * inc);
        }
    }

    out
}

fn build_tick_values(kind: &AxisKind, axis_position: AxisPosition) -> Vec<String> {
    match kind {
        AxisKind::Band { categories } => categories.clone(),
        AxisKind::Linear { domain } => {
            let (mut a, mut b) = *domain;
            if matches!(axis_position, AxisPosition::Left) {
                std::mem::swap(&mut a, &mut b);
            }
            d3_ticks(a, b, 10)
                .into_iter()
                .map(|v| format!("{v}"))
                .collect()
        }
    }
}

#[derive(Debug, Clone)]
enum AxisKind {
    Band { categories: Vec<String> },
    Linear { domain: (f64, f64) },
}

#[derive(Debug, Clone)]
struct Axis {
    kind: AxisKind,
    axis_config: AxisConfig,
    axis_theme: AxisThemeConfig,
    axis_position: AxisPosition,
    tick_values: Vec<String>,
    bounding_rect: BoundingRect,
    range: (f64, f64),
    show_title: bool,
    show_label: bool,
    show_tick: bool,
    show_axis_line: bool,
    outer_padding: f64,
    title: String,
    title_text_height: f64,
    label_dimension: Dimension,
}

impl Axis {
    fn new(
        kind: AxisKind,
        axis_config: AxisConfig,
        axis_theme: AxisThemeConfig,
        title: String,
    ) -> Self {
        Self {
            kind,
            axis_config,
            axis_theme,
            axis_position: AxisPosition::Left,
            tick_values: Vec::new(),
            bounding_rect: BoundingRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            range: (0.0, 10.0),
            show_title: false,
            show_label: false,
            show_tick: false,
            show_axis_line: false,
            outer_padding: 0.0,
            title,
            title_text_height: 0.0,
            label_dimension: Dimension {
                width: 0.0,
                height: 0.0,
            },
        }
    }

    fn set_axis_position(&mut self, pos: AxisPosition) {
        self.axis_position = pos;
        self.tick_values = build_tick_values(&self.kind, self.axis_position);
        let range = self.range;
        self.set_range(range);
    }

    fn set_range(&mut self, range: (f64, f64)) {
        self.range = range;
        if matches!(self.axis_position, AxisPosition::Left) {
            self.bounding_rect.height = range.1 - range.0;
        } else {
            self.bounding_rect.width = range.1 - range.0;
        }
    }

    fn set_bounding_box_xy(&mut self, pt: Point) {
        self.bounding_rect.x = pt.x;
        self.bounding_rect.y = pt.y;
    }

    fn get_range(&self) -> (f64, f64) {
        (
            self.range.0 + self.outer_padding,
            self.range.1 - self.outer_padding,
        )
    }

    fn tick_values(&self) -> &[String] {
        &self.tick_values
    }

    fn tick_distance(&self) -> f64 {
        let (a, b) = self.get_range();
        let span = (a - b).abs();
        if self.tick_values.is_empty() {
            return 0.0;
        }
        span / (self.tick_values.len() as f64)
    }

    fn get_scale_value(&self, value: &str) -> f64 {
        match &self.kind {
            AxisKind::Band { categories } => {
                let (a, b) = self.get_range();
                let n = categories.len();
                if n == 0 {
                    return a;
                }
                if n == 1 {
                    return a + (b - a) * 0.5;
                }
                let step = (b - a) / ((n - 1) as f64);
                let idx = categories.iter().position(|c| c == value).unwrap_or(0);
                a + step * (idx as f64)
            }
            AxisKind::Linear { domain } => {
                let Ok(v) = value.parse::<f64>() else {
                    return self.get_range().0;
                };
                if v.is_nan() {
                    return f64::NAN;
                }
                let (mut d0, mut d1) = *domain;
                if matches!(self.axis_position, AxisPosition::Left) {
                    std::mem::swap(&mut d0, &mut d1);
                }
                let (r0, r1) = self.get_range();
                if d0 == d1 {
                    return r0 + (r1 - r0) * 0.5;
                }
                let t = (v - d0) / (d1 - d0);
                r0 + t * (r1 - r0)
            }
        }
    }

    fn get_scale_number(&self, value: Option<f64>) -> f64 {
        let Some(value) = value else {
            return self.get_scale_value("NaN");
        };

        match &self.kind {
            AxisKind::Linear { domain } => {
                if value.is_nan() {
                    return f64::NAN;
                }
                let (mut d0, mut d1) = *domain;
                if matches!(self.axis_position, AxisPosition::Left) {
                    std::mem::swap(&mut d0, &mut d1);
                }
                let (r0, r1) = self.get_range();
                if d0 == d1 {
                    return r0 + (r1 - r0) * 0.5;
                }
                let t = (value - d0) / (d1 - d0);
                r0 + t * (r1 - r0)
            }
            AxisKind::Band { .. } => self.get_scale_value(&format!("{value}")),
        }
    }

    fn recalculate_outer_padding_to_draw_bar(&mut self) {
        const BAR_WIDTH_TO_TICK_WIDTH_RATIO: f64 = 0.7;
        let target = BAR_WIDTH_TO_TICK_WIDTH_RATIO * self.tick_distance();
        if target > self.outer_padding * 2.0 {
            self.outer_padding = (target / 2.0).floor();
        }
    }

    fn normalized_label_rotation_rad(&self) -> f64 {
        let rotation = self.axis_config.label_rotation;
        if (-90.0..=90.0).contains(&rotation) {
            rotation.to_radians()
        } else {
            0.0
        }
    }

    fn label_rotation_offset(&self, reference: AxisLabelDimensionRef) -> f64 {
        let rotation = self.normalized_label_rotation_rad();
        if rotation == 0.0 {
            return 0.0;
        }
        let dimension = match reference {
            AxisLabelDimensionRef::Width => self.label_dimension.width,
            AxisLabelDimensionRef::Height => self.label_dimension.height,
        };
        rotation.sin() * dimension / 2.0
    }

    fn calculate_space(&mut self, available: Dimension, measurer: &dyn TextMeasurer) -> Dimension {
        self.show_title = false;
        self.show_label = false;
        self.show_tick = false;
        self.show_axis_line = false;
        self.outer_padding = 0.0;
        self.title_text_height = 0.0;
        self.label_dimension = Dimension {
            width: 0.0,
            height: 0.0,
        };

        if matches!(self.axis_position, AxisPosition::Left) {
            let mut available_width = available.width;

            if self.axis_config.show_axis_line && available_width > self.axis_config.axis_line_width
            {
                available_width -= self.axis_config.axis_line_width;
                self.show_axis_line = true;
            }

            if self.axis_config.show_label {
                let ticks = self.tick_values();
                let dim = max_text_dimension(ticks, self.axis_config.label_font_size, measurer);
                self.label_dimension = dim;
                let max_padding = 0.2 * available.height;
                self.outer_padding = (dim.height / 2.0).min(max_padding);
                let width_required = dim.width + self.axis_config.label_padding * 2.0;
                if width_required <= available_width {
                    available_width -= width_required;
                    self.show_label = true;
                }
            }

            if self.axis_config.show_tick && available_width >= self.axis_config.tick_length {
                self.show_tick = true;
                available_width -= self.axis_config.tick_length;
            }

            if self.axis_config.show_title && !self.title.is_empty() {
                let title_height =
                    single_text_height(&self.title, self.axis_config.title_font_size, measurer);
                let width_required = title_height + self.axis_config.title_padding * 2.0;
                self.title_text_height = title_height;
                if width_required <= available_width {
                    available_width -= width_required;
                    self.show_title = true;
                }
            }

            self.bounding_rect.width = available.width - available_width;
            self.bounding_rect.height = available.height;
            Dimension {
                width: self.bounding_rect.width,
                height: self.bounding_rect.height,
            }
        } else {
            let mut available_height = available.height;

            if self.axis_config.show_axis_line
                && available_height > self.axis_config.axis_line_width
            {
                available_height -= self.axis_config.axis_line_width;
                self.show_axis_line = true;
            }

            if self.axis_config.show_label {
                let ticks = self.tick_values();
                let dim = max_text_dimension(ticks, self.axis_config.label_font_size, measurer);
                self.label_dimension = dim;
                let max_padding = 0.2 * available.width;
                self.outer_padding = (dim.width / 2.0).min(max_padding);
                let mut height_required = dim.height;
                let rotation = self.normalized_label_rotation_rad();
                if matches!(self.axis_position, AxisPosition::Bottom) && rotation != 0.0 {
                    height_required = height_required.max(
                        (rotation.sin() * dim.width).abs() + (rotation.cos() * dim.height).abs(),
                    );
                }
                height_required += self.axis_config.label_padding * 2.0;
                if height_required <= available_height {
                    available_height -= height_required;
                    self.show_label = true;
                }
            }

            if self.axis_config.show_tick && available_height >= self.axis_config.tick_length {
                self.show_tick = true;
                available_height -= self.axis_config.tick_length;
            }

            if self.axis_config.show_title && !self.title.is_empty() {
                let title_height =
                    single_text_height(&self.title, self.axis_config.title_font_size, measurer);
                let height_required = title_height + self.axis_config.title_padding * 2.0;
                self.title_text_height = title_height;
                if height_required <= available_height {
                    available_height -= height_required;
                    self.show_title = true;
                }
            }

            self.bounding_rect.width = available.width;
            self.bounding_rect.height = available.height - available_height;
            Dimension {
                width: self.bounding_rect.width,
                height: self.bounding_rect.height,
            }
        }
    }

    fn drawable_elements(&self) -> Vec<XyChartDrawableElem> {
        match self.axis_position {
            AxisPosition::Left => self.drawable_elements_for_left_axis(),
            AxisPosition::Bottom => self.drawable_elements_for_bottom_axis(),
            AxisPosition::Top => self.drawable_elements_for_top_axis(),
        }
    }

    fn drawable_elements_for_left_axis(&self) -> Vec<XyChartDrawableElem> {
        let mut out: Vec<XyChartDrawableElem> = Vec::new();
        if self.show_axis_line {
            let x = self.bounding_rect.x + self.bounding_rect.width
                - self.axis_config.axis_line_width / 2.0;
            out.push(XyChartDrawableElem::Path {
                group_texts: vec!["left-axis".to_string(), "axisl-line".to_string()],
                data: vec![XyChartPathData {
                    path: format!(
                        "M {x},{} L {x},{} ",
                        self.bounding_rect.y,
                        self.bounding_rect.y + self.bounding_rect.height
                    ),
                    fill: None,
                    stroke_fill: self.axis_theme.axis_line_color.clone(),
                    stroke_width: self.axis_config.axis_line_width,
                }],
            });
        }
        if self.show_label {
            let x = self.bounding_rect.x + self.bounding_rect.width
                - (if self.show_label {
                    self.axis_config.label_padding
                } else {
                    0.0
                })
                - (if self.show_tick {
                    self.axis_config.tick_length
                } else {
                    0.0
                })
                - (if self.show_axis_line {
                    self.axis_config.axis_line_width
                } else {
                    0.0
                });
            let ticks = self.tick_values();
            out.push(XyChartDrawableElem::Text {
                group_texts: vec!["left-axis".to_string(), "label".to_string()],
                data: ticks
                    .iter()
                    .map(|t| XyChartTextData {
                        text: t.clone(),
                        x,
                        y: self.get_scale_value(t),
                        fill: self.axis_theme.label_color.clone(),
                        font_size: self.axis_config.label_font_size,
                        rotation: 0.0,
                        vertical_pos: "middle".to_string(),
                        horizontal_pos: "right".to_string(),
                    })
                    .collect(),
            });
        }
        if self.show_tick {
            let x = self.bounding_rect.x + self.bounding_rect.width
                - (if self.show_axis_line {
                    self.axis_config.axis_line_width
                } else {
                    0.0
                });
            let ticks = self.tick_values();
            out.push(XyChartDrawableElem::Path {
                group_texts: vec!["left-axis".to_string(), "ticks".to_string()],
                data: ticks
                    .iter()
                    .map(|t| {
                        let y = self.get_scale_value(t);
                        XyChartPathData {
                            path: format!("M {x},{y} L {},{y}", x - self.axis_config.tick_length),
                            fill: None,
                            stroke_fill: self.axis_theme.tick_color.clone(),
                            stroke_width: self.axis_config.tick_width,
                        }
                    })
                    .collect(),
            });
        }
        if self.show_title {
            out.push(XyChartDrawableElem::Text {
                group_texts: vec!["left-axis".to_string(), "title".to_string()],
                data: vec![XyChartTextData {
                    text: self.title.clone(),
                    x: self.bounding_rect.x + self.axis_config.title_padding,
                    y: self.bounding_rect.y + self.bounding_rect.height / 2.0,
                    fill: self.axis_theme.title_color.clone(),
                    font_size: self.axis_config.title_font_size,
                    rotation: 270.0,
                    vertical_pos: "top".to_string(),
                    horizontal_pos: "center".to_string(),
                }],
            });
        }
        out
    }

    fn drawable_elements_for_bottom_axis(&self) -> Vec<XyChartDrawableElem> {
        let mut out: Vec<XyChartDrawableElem> = Vec::new();
        if self.show_axis_line {
            let y = self.bounding_rect.y + self.axis_config.axis_line_width / 2.0;
            out.push(XyChartDrawableElem::Path {
                group_texts: vec!["bottom-axis".to_string(), "axis-line".to_string()],
                data: vec![XyChartPathData {
                    path: format!(
                        "M {},{y} L {},{y}",
                        self.bounding_rect.x,
                        self.bounding_rect.x + self.bounding_rect.width
                    ),
                    fill: None,
                    stroke_fill: self.axis_theme.axis_line_color.clone(),
                    stroke_width: self.axis_config.axis_line_width,
                }],
            });
        }
        if self.show_label {
            let ticks = self.tick_values();
            out.push(XyChartDrawableElem::Text {
                group_texts: vec!["bottom-axis".to_string(), "label".to_string()],
                data: ticks
                    .iter()
                    .map(|t| XyChartTextData {
                        text: t.clone(),
                        x: self.get_scale_value(t)
                            + self.label_rotation_offset(AxisLabelDimensionRef::Height),
                        y: self.bounding_rect.y
                            + self.axis_config.label_padding
                            + (if self.show_tick {
                                self.axis_config.tick_length
                            } else {
                                0.0
                            })
                            + (if self.show_axis_line {
                                self.axis_config.axis_line_width
                            } else {
                                0.0
                            })
                            + self
                                .label_rotation_offset(AxisLabelDimensionRef::Width)
                                .abs(),
                        fill: self.axis_theme.label_color.clone(),
                        font_size: self.axis_config.label_font_size,
                        rotation: self.normalized_label_rotation_rad().to_degrees(),
                        vertical_pos: "top".to_string(),
                        horizontal_pos: "center".to_string(),
                    })
                    .collect(),
            });
        }
        if self.show_tick {
            let y = self.bounding_rect.y
                + (if self.show_axis_line {
                    self.axis_config.axis_line_width
                } else {
                    0.0
                });
            let ticks = self.tick_values();
            out.push(XyChartDrawableElem::Path {
                group_texts: vec!["bottom-axis".to_string(), "ticks".to_string()],
                data: ticks
                    .iter()
                    .map(|t| {
                        let x = self.get_scale_value(t);
                        XyChartPathData {
                            path: format!("M {x},{y} L {x},{}", y + self.axis_config.tick_length),
                            fill: None,
                            stroke_fill: self.axis_theme.tick_color.clone(),
                            stroke_width: self.axis_config.tick_width,
                        }
                    })
                    .collect(),
            });
        }
        if self.show_title {
            out.push(XyChartDrawableElem::Text {
                group_texts: vec!["bottom-axis".to_string(), "title".to_string()],
                data: vec![XyChartTextData {
                    text: self.title.clone(),
                    x: self.range.0 + (self.range.1 - self.range.0) / 2.0,
                    y: self.bounding_rect.y + self.bounding_rect.height
                        - self.axis_config.title_padding
                        - self.title_text_height,
                    fill: self.axis_theme.title_color.clone(),
                    font_size: self.axis_config.title_font_size,
                    rotation: 0.0,
                    vertical_pos: "top".to_string(),
                    horizontal_pos: "center".to_string(),
                }],
            });
        }
        out
    }

    fn drawable_elements_for_top_axis(&self) -> Vec<XyChartDrawableElem> {
        let mut out: Vec<XyChartDrawableElem> = Vec::new();
        if self.show_axis_line {
            let y = self.bounding_rect.y + self.bounding_rect.height
                - self.axis_config.axis_line_width / 2.0;
            out.push(XyChartDrawableElem::Path {
                group_texts: vec!["top-axis".to_string(), "axis-line".to_string()],
                data: vec![XyChartPathData {
                    path: format!(
                        "M {},{y} L {},{y}",
                        self.bounding_rect.x,
                        self.bounding_rect.x + self.bounding_rect.width
                    ),
                    fill: None,
                    stroke_fill: self.axis_theme.axis_line_color.clone(),
                    stroke_width: self.axis_config.axis_line_width,
                }],
            });
        }
        if self.show_label {
            let ticks = self.tick_values();
            out.push(XyChartDrawableElem::Text {
                group_texts: vec!["top-axis".to_string(), "label".to_string()],
                data: ticks
                    .iter()
                    .map(|t| XyChartTextData {
                        text: t.clone(),
                        x: self.get_scale_value(t),
                        y: self.bounding_rect.y
                            + (if self.show_title {
                                self.title_text_height + self.axis_config.title_padding * 2.0
                            } else {
                                0.0
                            })
                            + self.axis_config.label_padding,
                        fill: self.axis_theme.label_color.clone(),
                        font_size: self.axis_config.label_font_size,
                        rotation: 0.0,
                        vertical_pos: "top".to_string(),
                        horizontal_pos: "center".to_string(),
                    })
                    .collect(),
            });
        }
        if self.show_tick {
            let y = self.bounding_rect.y;
            let ticks = self.tick_values();
            out.push(XyChartDrawableElem::Path {
                group_texts: vec!["top-axis".to_string(), "ticks".to_string()],
                data: ticks
                    .iter()
                    .map(|t| {
                        let x = self.get_scale_value(t);
                        let y0 = y + self.bounding_rect.height
                            - (if self.show_axis_line {
                                self.axis_config.axis_line_width
                            } else {
                                0.0
                            });
                        let y1 = y + self.bounding_rect.height
                            - self.axis_config.tick_length
                            - (if self.show_axis_line {
                                self.axis_config.axis_line_width
                            } else {
                                0.0
                            });
                        XyChartPathData {
                            path: format!("M {x},{y0} L {x},{y1}"),
                            fill: None,
                            stroke_fill: self.axis_theme.tick_color.clone(),
                            stroke_width: self.axis_config.tick_width,
                        }
                    })
                    .collect(),
            });
        }
        if self.show_title {
            out.push(XyChartDrawableElem::Text {
                group_texts: vec!["top-axis".to_string(), "title".to_string()],
                data: vec![XyChartTextData {
                    text: self.title.clone(),
                    x: self.bounding_rect.x + self.bounding_rect.width / 2.0,
                    y: self.bounding_rect.y + self.axis_config.title_padding,
                    fill: self.axis_theme.title_color.clone(),
                    font_size: self.axis_config.title_font_size,
                    rotation: 0.0,
                    vertical_pos: "top".to_string(),
                    horizontal_pos: "center".to_string(),
                }],
            });
        }
        out
    }
}

#[derive(Debug, Clone, Copy)]
enum AxisLabelDimensionRef {
    Width,
    Height,
}

fn line_path(points: &[(f64, f64)]) -> Option<String> {
    let (first, rest) = points.split_first()?;
    let mut out = String::with_capacity(points.len().saturating_mul(24).max(24));
    let _ = write!(&mut out, "M{},{}", first.0, first.1);
    if rest.is_empty() {
        out.push('Z');
        return Some(out);
    }
    for p in rest {
        let _ = write!(&mut out, "L{},{}", p.0, p.1);
    }
    Some(out)
}

/// Lays out a typed XYChart render model without a compatibility-JSON round trip.
pub(crate) fn layout_xychart_diagram_typed(
    model: &XyChartDiagramRenderModel,
    diagram_title: Option<&str>,
    effective_config: &Value,
    text_measurer: &dyn TextMeasurer,
) -> Result<XyChartDiagramLayout> {
    if model
        .orientation
        .as_str()
        .split_whitespace()
        .next()
        .is_some_and(|t| t != "vertical" && t != "horizontal" && !t.is_empty())
    {
        return Err(Error::InvalidModel {
            message: format!("unexpected xychart orientation: {}", model.orientation),
        });
    }

    let chart_cfg = parse_chart_config(effective_config, model);
    let theme_cfg = PresentationTheme::new(effective_config).xychart();

    let title = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| {
            diagram_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
        })
        .unwrap_or_default()
        .to_owned();
    let mut chart_width = (chart_cfg.width * chart_cfg.plot_reserved_space_percent / 100.0).floor();
    let mut chart_height =
        (chart_cfg.height * chart_cfg.plot_reserved_space_percent / 100.0).floor();
    let mut available_width = chart_cfg.width - chart_width;
    let mut available_height = chart_cfg.height - chart_height;
    let title_dimension = max_text_dimension(
        std::slice::from_ref(&title),
        chart_cfg.title_font_size,
        text_measurer,
    );
    let title_height = title_dimension.height + 2.0 * chart_cfg.title_padding;
    let show_chart_title =
        chart_cfg.show_title && !title.is_empty() && title_height <= available_height;

    let mut drawables: Vec<XyChartDrawableElem> =
        Vec::with_capacity(model.plots.len().saturating_mul(2).saturating_add(4));

    let (x_axis_kind, x_axis_title) = match &model.x_axis {
        XyChartAxisRenderModel::Band { title, categories } => (
            AxisKind::Band {
                categories: categories.clone(),
            },
            title.clone(),
        ),
        XyChartAxisRenderModel::Linear { title, min, max } => (
            AxisKind::Linear {
                domain: (min.unwrap_or(0.0), max.unwrap_or(1.0)),
            },
            title.clone(),
        ),
    };
    let (y_axis_kind, y_axis_title) = match &model.y_axis {
        XyChartAxisRenderModel::Band { title, categories } => (
            AxisKind::Band {
                categories: categories.clone(),
            },
            title.clone(),
        ),
        XyChartAxisRenderModel::Linear { title, min, max } => (
            AxisKind::Linear {
                domain: (min.unwrap_or(0.0), max.unwrap_or(1.0)),
            },
            title.clone(),
        ),
    };

    let x_axis_theme = AxisThemeConfig {
        title_color: theme_cfg.x_axis_title_color.clone(),
        label_color: theme_cfg.x_axis_label_color.clone(),
        tick_color: theme_cfg.x_axis_tick_color.clone(),
        axis_line_color: theme_cfg.x_axis_line_color.clone(),
    };
    let y_axis_theme = AxisThemeConfig {
        title_color: theme_cfg.y_axis_title_color.clone(),
        label_color: theme_cfg.y_axis_label_color.clone(),
        tick_color: theme_cfg.y_axis_tick_color.clone(),
        axis_line_color: theme_cfg.y_axis_line_color.clone(),
    };

    let mut x_axis = Axis::new(
        x_axis_kind,
        chart_cfg.x_axis.clone(),
        x_axis_theme,
        x_axis_title,
    );
    let mut y_axis = Axis::new(
        y_axis_kind,
        chart_cfg.y_axis.clone(),
        y_axis_theme,
        y_axis_title,
    );

    let mut legend_plots = if chart_cfg.show_legend {
        model
            .plots
            .iter()
            .enumerate()
            .filter(|(_, plot)| plot.title.as_deref().is_some_and(|title| !title.is_empty()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let legend_origin;

    let plot_rect = if chart_cfg.chart_orientation == "horizontal" {
        let title_y_end = if show_chart_title { title_height } else { 0.0 };
        available_height = (available_height - title_y_end).max(0.0);

        x_axis.set_axis_position(AxisPosition::Left);
        let space_used_x = x_axis.calculate_space(
            Dimension {
                width: available_width,
                height: available_height,
            },
            text_measurer,
        );
        available_width = (available_width - space_used_x.width).max(0.0);
        let plot_x = space_used_x.width;

        y_axis.set_axis_position(AxisPosition::Top);
        let space_used_y = y_axis.calculate_space(
            Dimension {
                width: available_width,
                height: available_height,
            },
            text_measurer,
        );
        available_height = (available_height - space_used_y.height).max(0.0);
        let plot_y = title_y_end + space_used_y.height;

        let legend_space = calculate_legend_space(
            &legend_plots,
            &chart_cfg,
            Dimension {
                width: available_width,
                height: chart_height,
            },
            text_measurer,
        );
        if legend_space.width == 0.0 && legend_space.height == 0.0 {
            legend_plots.clear();
        }
        available_width = (available_width - legend_space.width).max(0.0);

        if available_width > 0.0 {
            chart_width += available_width;
        }
        if available_height > 0.0 {
            chart_height += available_height;
        }

        let plot_rect = BoundingRect {
            x: plot_x,
            y: plot_y,
            width: chart_width,
            height: chart_height,
        };
        legend_origin = pt(
            plot_x + chart_width,
            plot_y + ((chart_height - legend_space.height) / 2.0).max(0.0),
        );

        y_axis.set_range((plot_x, plot_x + chart_width));
        y_axis.set_bounding_box_xy(pt(plot_x, title_y_end));
        x_axis.set_range((plot_y, plot_y + chart_height));
        x_axis.set_bounding_box_xy(pt(0.0, plot_y));
        plot_rect
    } else {
        let plot_y = if show_chart_title { title_height } else { 0.0 };
        available_height = (available_height - plot_y).max(0.0);

        x_axis.set_axis_position(AxisPosition::Bottom);
        let space_used_x = x_axis.calculate_space(
            Dimension {
                width: available_width,
                height: available_height,
            },
            text_measurer,
        );
        available_height = (available_height - space_used_x.height).max(0.0);

        y_axis.set_axis_position(AxisPosition::Left);
        let space_used_y = y_axis.calculate_space(
            Dimension {
                width: available_width,
                height: available_height,
            },
            text_measurer,
        );
        let plot_x = space_used_y.width;
        available_width = (available_width - space_used_y.width).max(0.0);

        let legend_space = calculate_legend_space(
            &legend_plots,
            &chart_cfg,
            Dimension {
                width: available_width,
                height: chart_height,
            },
            text_measurer,
        );
        if legend_space.width == 0.0 && legend_space.height == 0.0 {
            legend_plots.clear();
        }
        available_width = (available_width - legend_space.width).max(0.0);

        if available_width > 0.0 {
            chart_width += available_width;
        }
        if available_height > 0.0 {
            chart_height += available_height;
        }

        let plot_rect = BoundingRect {
            x: plot_x,
            y: plot_y,
            width: chart_width,
            height: chart_height,
        };
        legend_origin = pt(
            plot_x + chart_width,
            plot_y + ((chart_height - legend_space.height) / 2.0).max(0.0),
        );

        x_axis.set_range((plot_x, plot_x + chart_width));
        x_axis.set_bounding_box_xy(pt(plot_x, plot_y + chart_height));
        y_axis.set_range((plot_y, plot_y + chart_height));
        y_axis.set_bounding_box_xy(pt(0.0, plot_y));
        plot_rect
    };

    if show_chart_title {
        // The title owns a row above both plot and legend. Center it over the plot,
        // allowing a wide title to span legend columns while staying inside the chart.
        let half_width = title_dimension.width.min(chart_cfg.width) / 2.0;
        let title_x = (plot_rect.x + plot_rect.width / 2.0)
            .max(half_width)
            .min(chart_cfg.width - half_width);
        drawables.push(XyChartDrawableElem::Text {
            group_texts: vec!["chart-title".to_string()],
            data: vec![XyChartTextData {
                text: title,
                x: title_x,
                y: title_height / 2.0,
                fill: theme_cfg.title_color.clone(),
                font_size: chart_cfg.title_font_size,
                rotation: 0.0,
                vertical_pos: "middle".to_string(),
                horizontal_pos: "center".to_string(),
            }],
        });
    }

    if model
        .plots
        .iter()
        .any(|p| matches!(p.plot_type, XyChartPlotType::Bar))
    {
        x_axis.recalculate_outer_padding_to_draw_bar();
    }

    for (plot_index, plot) in model.plots.iter().enumerate() {
        let color = plot_color_from_palette(&theme_cfg.plot_color_palette, plot_index);

        match plot.plot_type {
            XyChartPlotType::Bar => {
                let bar_padding_percent = 0.05;
                let bar_width = (x_axis.outer_padding * 2.0).min(x_axis.tick_distance())
                    * (1.0 - bar_padding_percent);
                let bar_width_half = bar_width / 2.0;

                let mut rects: Vec<XyChartRectData> = Vec::with_capacity(plot.data.len());
                for (cat, value) in &plot.data {
                    let x = x_axis.get_scale_value(cat);
                    let y = y_axis.get_scale_number(*value);
                    if chart_cfg.chart_orientation == "horizontal" {
                        rects.push(XyChartRectData {
                            x: plot_rect.x,
                            y: x - bar_width_half,
                            width: y - plot_rect.x,
                            height: bar_width,
                            fill: color.clone(),
                            stroke_fill: color.clone(),
                            stroke_width: 0.0,
                        });
                    } else {
                        rects.push(XyChartRectData {
                            x: x - bar_width_half,
                            y,
                            width: bar_width,
                            height: plot_rect.y + plot_rect.height - y,
                            fill: color.clone(),
                            stroke_fill: color.clone(),
                            stroke_width: 0.0,
                        });
                    }
                }

                drawables.push(XyChartDrawableElem::Rect {
                    group_texts: vec!["plot".to_string(), format!("bar-plot-{plot_index}")],
                    data: rects,
                });
            }
            XyChartPlotType::Line => {
                let mut points: Vec<(f64, f64)> = Vec::with_capacity(plot.data.len());
                for (cat, value) in &plot.data {
                    let x = x_axis.get_scale_value(cat);
                    let y = y_axis.get_scale_number(*value);
                    points.push(if chart_cfg.chart_orientation == "horizontal" {
                        (y, x)
                    } else {
                        (x, y)
                    });
                }
                if let Some(path) = line_path(&points) {
                    let line_color = color.clone();
                    drawables.push(XyChartDrawableElem::Path {
                        group_texts: vec!["plot".to_string(), format!("line-plot-{plot_index}")],
                        data: vec![XyChartPathData {
                            path,
                            fill: None,
                            stroke_fill: line_color.clone(),
                            stroke_width: 2.0,
                        }],
                    });
                    if !plot.point_labels.is_empty() {
                        let label_offset = 10.0;
                        let font_size = 12.0;
                        let labels = points
                            .iter()
                            .enumerate()
                            .filter_map(|(idx, (px, py))| {
                                let label = plot.point_labels.get(idx)?;
                                if label.is_empty() {
                                    return None;
                                }
                                let (x, y, vertical_pos, horizontal_pos) =
                                    if chart_cfg.chart_orientation == "horizontal" {
                                        (
                                            px + label_offset,
                                            *py,
                                            "middle".to_string(),
                                            "left".to_string(),
                                        )
                                    } else {
                                        (
                                            *px,
                                            py - label_offset,
                                            "middle".to_string(),
                                            "center".to_string(),
                                        )
                                    };
                                Some(XyChartTextData {
                                    text: label.clone(),
                                    x,
                                    y,
                                    fill: line_color.clone(),
                                    font_size,
                                    rotation: 0.0,
                                    vertical_pos,
                                    horizontal_pos,
                                })
                            })
                            .collect::<Vec<_>>();
                        if !labels.is_empty() {
                            drawables.push(XyChartDrawableElem::Text {
                                group_texts: vec![
                                    "plot".to_string(),
                                    format!("line-plot-{plot_index}"),
                                    "labels".to_string(),
                                ],
                                data: labels,
                            });
                        }
                    }
                }
            }
        }
    }

    drawables.extend(legend_drawable_elements(
        &legend_plots,
        legend_origin,
        &chart_cfg,
        &theme_cfg.legend_text_color,
        &theme_cfg.plot_color_palette,
    ));

    drawables.extend(x_axis.drawable_elements());
    drawables.extend(y_axis.drawable_elements());

    let label_data = if chart_cfg.show_data_label {
        model
            .plots
            .first()
            .map(|p| {
                p.data
                    .iter()
                    .map(|(_, y)| {
                        y.map(|v| format!("{v}"))
                            .unwrap_or_else(|| "null".to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(XyChartDiagramLayout {
        width: chart_cfg.width,
        height: chart_cfg.height,
        chart_orientation: chart_cfg.chart_orientation,
        show_data_label: chart_cfg.show_data_label,
        show_data_label_outside_bar: chart_cfg.show_data_label_outside_bar,
        background_color: theme_cfg.background_color,
        label_data,
        drawables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextMetrics;
    use kurbo::Shape;
    use serde_json::json;

    struct UserUnitMeasurer;

    impl TextMeasurer for UserUnitMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: text.encode_utf16().count() as f64 * style.font_size,
                height: style.font_size,
                line_count: 1,
            }
        }
    }

    fn layout(title: &str, orientation: &str, height: f64, legend: bool) -> XyChartDiagramLayout {
        let model = XyChartDiagramRenderModel {
            orientation: orientation.to_string(),
            title: Some(title.to_string()),
            acc_title: None,
            acc_descr: None,
            x_axis: XyChartAxisRenderModel::Band {
                title: String::new(),
                categories: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            },
            y_axis: XyChartAxisRenderModel::Linear {
                title: "A fairly wide y-axis title".to_string(),
                min: Some(0.0),
                max: Some(100000.0),
            },
            plots: vec![XyChartPlotRenderModel {
                plot_type: XyChartPlotType::Bar,
                title: Some("series".to_string()),
                values: vec![10.0, 20.0, 30.0],
                data: vec![
                    ("a".to_string(), Some(10.0)),
                    ("b".to_string(), Some(20.0)),
                    ("c".to_string(), Some(30.0)),
                ],
                point_labels: Vec::new(),
            }],
            display: Default::default(),
        };
        layout_xychart_diagram_typed(
            &model,
            None,
            &json!({ "xyChart": {
                "width": 700, "height": height, "showLegend": legend
            }}),
            &UserUnitMeasurer,
        )
        .unwrap()
    }

    fn title(layout: &XyChartDiagramLayout) -> Option<&XyChartTextData> {
        layout.drawables.iter().find_map(|drawable| match drawable {
            XyChartDrawableElem::Text { group_texts, data } if group_texts == &["chart-title"] => {
                data.first()
            }
            _ => None,
        })
    }

    fn plot_span(layout: &XyChartDiagramLayout) -> (f64, f64) {
        let axis = if layout.chart_orientation == "horizontal" {
            "top-axis"
        } else {
            "bottom-axis"
        };
        let path = layout
            .drawables
            .iter()
            .find_map(|drawable| match drawable {
                XyChartDrawableElem::Path { group_texts, data }
                    if group_texts == &[axis, "axis-line"] =>
                {
                    data.first()
                }
                _ => None,
            })
            .expect("axis spanning the plot");
        let bounds = kurbo::BezPath::from_svg(&path.path).unwrap().bounding_box();
        (bounds.x0, bounds.x1)
    }

    #[test]
    fn xychart_title_centers_on_plot_for_both_orientations_and_legend_modes() {
        for orientation in ["vertical", "horizontal"] {
            for legend in [false, true] {
                let layout = layout("Title", orientation, 500.0, legend);
                let (start, end) = plot_span(&layout);
                assert!(start > 0.0);
                assert!((title(&layout).unwrap().x - (start + end) / 2.0).abs() < 1e-9);
                if legend {
                    assert!(end < layout.width);
                } else {
                    assert_eq!(end, layout.width);
                }
            }
        }
    }

    #[test]
    fn xychart_wide_title_clamps_to_chart_and_can_span_legend_columns() {
        let wide = "x".repeat(32);
        let without_legend = layout(&wide, "vertical", 500.0, false);
        assert_eq!(title(&without_legend).unwrap().x, 380.0);

        let with_legend = layout(&wide, "vertical", 500.0, true);
        let (start, end) = plot_span(&with_legend);
        let label = title(&with_legend).unwrap();
        assert_eq!(label.x, (start + end) / 2.0);
        assert!(label.x + 320.0 > end);
        assert!(label.x + 320.0 <= with_legend.width);
        let legend_marker = with_legend
            .drawables
            .iter()
            .find_map(|drawable| match drawable {
                XyChartDrawableElem::Rect { group_texts, data }
                    if group_texts == &["legend", "markers"] =>
                {
                    data.first()
                }
                _ => None,
            })
            .unwrap();
        assert!(label.y + 10.0 <= legend_marker.y - 10.0);

        let oversized = layout(&"x".repeat(40), "vertical", 500.0, false);
        assert_eq!(title(&oversized).unwrap().x, oversized.width / 2.0);
    }

    #[test]
    fn xychart_title_needs_space_after_the_plot_reservation() {
        for orientation in ["vertical", "horizontal"] {
            let short = layout("Title", orientation, 60.0, false);
            assert!(title(&short).is_none());
            let mut bars = 0;
            for drawable in &short.drawables {
                if let XyChartDrawableElem::Rect { group_texts, data } = drawable
                    && group_texts == &["plot", "bar-plot-0"]
                {
                    bars += data.len();
                    for bar in data {
                        assert!(bar.y + bar.height <= short.height + 1e-9);
                    }
                }
            }
            assert_eq!(bars, 3);
            assert!(title(&layout("Title", orientation, 100.0, false)).is_some());
        }
    }

    #[test]
    fn xychart_measurement_keeps_svg_user_units_without_display_rescaling() {
        let dimensions = max_text_dimension(&["CPU".to_string()], 14.0, &UserUnitMeasurer);
        assert_eq!(dimensions.width, 42.0);
        assert_eq!(dimensions.height, 14.0);
    }
}
