use crate::Result;
use crate::config::config_string;
use crate::model::{Bounds, PieDiagramLayout, PieLegendItemLayout, PieSliceLayout};
use crate::text::{TextMeasurer, TextStyle};
use merman_core::diagrams::pie::{PieDiagramRenderModel, PieRenderSection};

pub(crate) const PIE_LEGEND_RECT_SIZE_PX: f64 = 18.0;
pub(crate) const PIE_LEGEND_SPACING_PX: f64 = 4.0;

mod config;

pub(crate) use config::{PieConfigView, PieLegendPosition};

#[derive(Debug, Clone)]
struct ColorScale {
    palette: Vec<String>,
    mapping: std::collections::HashMap<String, usize>,
    next: usize,
}

impl ColorScale {
    fn default_palette() -> Vec<String> {
        // This fallback is only for direct layout callers. Normal rendering consumes the final
        // resolved `pie1..pie12` theme variables produced by merman-core.
        [
            "#ECECFF",
            "#ffffde",
            "hsl(80, 100%, 56.2745098039%)",
            "hsl(240, 100%, 86.2745098039%)",
            "hsl(60, 100%, 63.5294117647%)",
            "hsl(80, 100%, 76.2745098039%)",
            "hsl(300, 100%, 76.2745098039%)",
            "hsl(180, 100%, 56.2745098039%)",
            "hsl(0, 100%, 56.2745098039%)",
            "hsl(300, 100%, 56.2745098039%)",
            "hsl(150, 100%, 56.2745098039%)",
            "hsl(0, 100%, 66.2745098039%)",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }

    fn from_config(effective_config: &serde_json::Value) -> Self {
        let mut palette = Self::default_palette();
        for (idx, color) in palette.iter_mut().enumerate() {
            let key = format!("pie{}", idx + 1);
            if let Some(value) = config_string(effective_config, &["themeVariables", &key]) {
                *color = value;
            }
        }
        Self {
            palette,
            mapping: std::collections::HashMap::new(),
            next: 0,
        }
    }

    fn color_for(&mut self, label: &str) -> String {
        if let Some(idx) = self.mapping.get(label).copied() {
            return self.palette[idx % self.palette.len()].clone();
        }
        let idx = self.next;
        self.next += 1;
        self.mapping.insert(label.to_string(), idx);
        self.palette[idx % self.palette.len()].clone()
    }
}

fn polar_xy(radius: f64, angle: f64) -> (f64, f64) {
    // Mermaid pie charts use a "12 o'clock is zero" convention with y increasing downwards.
    let x = radius * angle.sin();
    let y = -radius * angle.cos();
    (x, y)
}

fn fmt_number(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    if v.abs() < 0.0005 {
        return "0".to_string();
    }
    let mut r = (v * 1000.0).round() / 1000.0;
    if r.abs() < 0.0005 {
        r = 0.0;
    }
    let mut s = format!("{r:.3}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_string() } else { s }
}

pub(crate) fn layout_pie_diagram_typed(
    model: &PieDiagramRenderModel,
    diagram_title: Option<&str>,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<PieDiagramLayout> {
    let _ = (
        model.title.as_deref(),
        model.acc_title.as_deref(),
        model.acc_descr.as_deref(),
    );

    // Mermaid@11.16 `packages/mermaid/src/diagrams/pie/pieRenderer.ts` constants.
    let margin: f64 = 40.0;
    let legend_rect_size = PIE_LEGEND_RECT_SIZE_PX;
    let legend_spacing = PIE_LEGEND_SPACING_PX;

    let center: f64 = 225.0;
    let radius: f64 = 185.0;
    let outer_radius = radius + 1.0;
    let cfg = PieConfigView::new(effective_config).layout_settings();
    let title = model.title.as_deref().or(diagram_title);
    let label_radius = radius.max(0.0) * cfg.text_position;
    let legend_step_y: f64 = legend_rect_size + legend_spacing;
    let legend_position = cfg.legend_position;
    let total_legend_height = (model.sections.len() as f64) * legend_step_y;
    let centered_legend_start_y = -(legend_step_y * (model.sections.len().max(1) as f64)) / 2.0;
    let legend_start_y = match legend_position {
        PieLegendPosition::Top => -radius,
        PieLegendPosition::Bottom => radius + legend_step_y,
        _ => centered_legend_start_y,
    };

    let total: f64 = model
        .sections
        .iter()
        .filter(|s| s.value.is_finite() && s.value >= 0.0)
        .map(|s| s.value)
        .sum();

    let mut color_scale = ColorScale::from_config(effective_config);
    for sec in &model.sections {
        let _ = color_scale.color_for(&sec.label);
    }

    let mut slices: Vec<PieSliceLayout> = Vec::new();
    if total.is_finite() && total > 0.0 {
        // Mermaid@11.16 `packages/mermaid/src/diagrams/pie/pieRenderer.ts`:
        //
        // - filter out values < 1% (based on the original total)
        // - preserve input order before D3 pie() computes angles (`sort(null)`)
        // - angles are normalized over the filtered set (so drawn slices fill the whole circle)
        // - percentage labels are still computed using the original total
        let pie_sections: Vec<&PieRenderSection> = model
            .sections
            .iter()
            .filter(|s| s.value.is_finite() && s.value > 0.0)
            .filter(|s| (s.value / total) * 100.0 >= 1.0)
            .collect();

        let pie_total: f64 = pie_sections.iter().map(|s| s.value).sum();
        if !pie_sections.is_empty() && pie_total.is_finite() && pie_total > 0.0 {
            if pie_sections.len() == 1 {
                let s = pie_sections[0];
                let fill = color_scale.color_for(&s.label);
                let (tx, ty) = polar_xy(label_radius, std::f64::consts::PI);
                let percent = ((100.0 * (s.value / total)).max(0.0)).round() as i64;
                slices.push(PieSliceLayout {
                    label: s.label.clone(),
                    value: s.value,
                    start_angle: 0.0,
                    end_angle: std::f64::consts::TAU,
                    is_full_circle: true,
                    percent,
                    text_x: tx,
                    text_y: ty,
                    fill,
                });
            } else {
                let mut start = 0.0;
                for s in pie_sections {
                    let frac = (s.value / pie_total).max(0.0);
                    let delta = (frac * std::f64::consts::TAU).max(0.0);
                    let end = start + delta;
                    let mid = (start + end) / 2.0;
                    let (tx, ty) = polar_xy(label_radius, mid);
                    let fill = color_scale.color_for(&s.label);
                    let percent = ((100.0 * (s.value / total)).max(0.0)).round() as i64;
                    if percent != 0 {
                        slices.push(PieSliceLayout {
                            label: s.label.clone(),
                            value: s.value,
                            start_angle: start,
                            end_angle: end,
                            is_full_circle: false,
                            percent,
                            text_x: tx,
                            text_y: ty,
                            fill,
                        });
                    }
                    start = end;
                }
            }
        }
    }

    // Lock the color scale domain based on the drawn slices first, then compute legend colors in
    // the original section order (this matches Mermaid's zero-slice behavior).
    let mut legend_items: Vec<PieLegendItemLayout> = Vec::new();
    for (i, sec) in model.sections.iter().enumerate() {
        let y = legend_start_y + (i as f64) * legend_step_y;
        let fill = color_scale.color_for(&sec.label);
        legend_items.push(PieLegendItemLayout {
            label: sec.label.clone(),
            value: sec.value,
            fill,
            y,
        });
    }

    let legend_style = TextStyle {
        font_family: None,
        font_size: 17.0,
        font_weight: None,
        font_style: None,
    };
    let title_style = TextStyle {
        font_family: None,
        font_size: 25.0,
        font_weight: None,
        font_style: None,
    };
    let mut max_legend_width: f64 = 0.0;
    for sec in &model.sections {
        let label = if model.show_data {
            format!("{} [{}]", sec.label, fmt_number(sec.value))
        } else {
            sec.label.clone()
        };
        let trimmed = label.trim_end();
        // Mermaid 11.16 measures pie legend text via a single SVG `<text>` node's
        // `getBoundingClientRect().width`.
        let w = if trimmed.is_empty() {
            0.0
        } else {
            measurer.measure_svg_text_bounding_client_rect_width_px(trimmed, &legend_style)
        };
        max_legend_width = max_legend_width.max(w);
    }

    let title_width = title
        .map(|title| measurer.measure_svg_text_bounding_client_rect_width_px(title, &title_style))
        .unwrap_or(0.0);

    let base_w: f64 = center * 2.0;
    let legend_extra_width = legend_rect_size + legend_spacing + max_legend_width;
    let centered_legend_x = -max_legend_width / 2.0 - (legend_rect_size + legend_spacing);

    let chart_and_legend_width = match legend_position {
        PieLegendPosition::Top => (base_w + margin).max(1.0),
        PieLegendPosition::Bottom => (base_w + margin).max(1.0),
        PieLegendPosition::Left => (base_w + margin + legend_extra_width).max(1.0),
        PieLegendPosition::Center => (base_w + margin).max(1.0),
        PieLegendPosition::Right => {
            if model.sections.is_empty() {
                f64::NEG_INFINITY
            } else {
                (base_w + margin + legend_extra_width).max(1.0)
            }
        }
    };

    let height = match legend_position {
        PieLegendPosition::Top | PieLegendPosition::Bottom => {
            (base_w + total_legend_height).max(1.0)
        }
        PieLegendPosition::Left | PieLegendPosition::Center | PieLegendPosition::Right => {
            base_w.max(1.0)
        }
    };

    let legend_x = match legend_position {
        PieLegendPosition::Top | PieLegendPosition::Bottom | PieLegendPosition::Center => {
            centered_legend_x
        }
        PieLegendPosition::Left => -radius - (legend_rect_size + legend_spacing),
        PieLegendPosition::Right => 12.0 * legend_rect_size,
    };

    let title_left = base_w / 2.0 - title_width / 2.0;
    let title_right = base_w / 2.0 + title_width / 2.0;
    let min_x = title_left.min(0.0);
    let max_x = chart_and_legend_width.max(title_right);
    let max_x = if model.sections.is_empty() {
        max_x.max(base_w)
    } else {
        max_x
    };

    Ok(PieDiagramLayout {
        bounds: Some(Bounds {
            min_x,
            min_y: 0.0,
            max_x,
            max_y: height,
        }),
        title: title.map(str::to_owned),
        center_x: center,
        center_y: center,
        radius,
        outer_radius,
        legend_x,
        legend_start_y,
        legend_step_y,
        slices,
        legend_items,
    })
}

#[cfg(test)]
mod tests {
    use crate::text::{TextMeasurer, TextMetrics, TextStyle};
    use merman_core::diagrams::pie::{PieDiagramRenderModel, PieRenderSection};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingBoundingClientRectMeasurer {
        calls: Mutex<Vec<String>>,
    }

    impl TextMeasurer for RecordingBoundingClientRectMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            panic!("pie legend and title must use the source-backed browser primitive")
        }

        fn measure_svg_simple_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            panic!("getBBox must not stand in for getBoundingClientRect")
        }

        fn measure_svg_text_bounding_client_rect_width_px(
            &self,
            text: &str,
            _style: &TextStyle,
        ) -> f64 {
            self.calls
                .lock()
                .expect("measurement calls")
                .push(text.to_string());
            match text {
                "Legend" => 123.456_789,
                "Title" => 1_000.123_456,
                "  Title  " => 1_100.123_456,
                "\u{a0}Title\u{a0}" => 1_200.123_456,
                other => panic!("unexpected measurement: {other}"),
            }
        }
    }

    #[test]
    fn pie_legend_geometry_constants_match_mermaid() {
        assert_eq!(super::PIE_LEGEND_RECT_SIZE_PX, 18.0);
        assert_eq!(super::PIE_LEGEND_SPACING_PX, 4.0);
    }

    #[test]
    fn pie_legend_and_title_use_exact_bounding_client_rect_results() {
        let measurer = RecordingBoundingClientRectMeasurer::default();
        let mut legend_model = PieDiagramRenderModel::default();
        legend_model.sections = vec![PieRenderSection {
            label: "Legend".to_string(),
            value: 1.0,
        }];
        let legend_layout =
            super::layout_pie_diagram_typed(&legend_model, None, &serde_json::json!({}), &measurer)
                .expect("legend layout");
        let legend_max_x = legend_layout.bounds.expect("legend bounds").max_x;
        assert!((legend_max_x - (512.0 + 123.456_789)).abs() < 1e-12);

        let mut title_model = PieDiagramRenderModel::default();
        title_model.title = Some("Title".to_string());
        let title_layout = super::layout_pie_diagram_typed(
            &title_model,
            None,
            &serde_json::json!({"pie": {"legendPosition": "top"}}),
            &measurer,
        )
        .expect("title layout");
        let title_max_x = title_layout.bounds.expect("title bounds").max_x;
        assert!((title_max_x - (225.0 + 1_000.123_456 / 2.0)).abs() < 1e-12);
        assert_eq!(
            *measurer.calls.lock().expect("measurement calls"),
            ["Legend".to_string(), "Title".to_string()]
        );
    }

    #[test]
    fn empty_pie_bounds_still_contain_the_painted_circle() {
        let measurer = RecordingBoundingClientRectMeasurer::default();
        for config in [
            serde_json::json!({}),
            serde_json::json!({"pie": {"legendPosition": "top"}}),
        ] {
            let layout = super::layout_pie_diagram_typed(
                &PieDiagramRenderModel::default(),
                None,
                &config,
                &measurer,
            )
            .expect("empty pie layout");
            let bounds = layout.bounds.expect("empty pie bounds");
            assert!(bounds.min_x <= layout.center_x - layout.outer_radius);
            assert!(bounds.max_x >= layout.center_x * 2.0);
        }
    }

    #[test]
    fn pie_frontmatter_title_preserves_boundary_whitespace_for_layout_measurement() {
        for (title, expected_max_x) in [
            ("  Title  ", 225.0 + 1_100.123_456 / 2.0),
            ("\u{a0}Title\u{a0}", 225.0 + 1_200.123_456 / 2.0),
        ] {
            let measurer = RecordingBoundingClientRectMeasurer::default();
            let layout = super::layout_pie_diagram_typed(
                &PieDiagramRenderModel::default(),
                Some(title),
                &serde_json::json!({"pie": {"legendPosition": "top"}}),
                &measurer,
            )
            .expect("title layout");

            assert_eq!(layout.title.as_deref(), Some(title));
            let max_x = layout.bounds.expect("title bounds").max_x;
            assert!((max_x - expected_max_x).abs() < 1e-12);
            assert_eq!(
                *measurer.calls.lock().expect("measurement calls"),
                [title.to_string()]
            );
        }
    }
}
