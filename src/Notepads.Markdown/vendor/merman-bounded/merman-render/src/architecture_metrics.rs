use crate::model::Bounds;
use crate::text::{TextMeasurer, TextStyle};

pub(crate) const ARCHITECTURE_SERVICE_LABEL_BOTTOM_EXTENSION_PX: f64 = 18.0;
pub(crate) const ARCHITECTURE_CREATE_TEXT_DEFAULT_WRAP_WIDTH_PX: f64 = 200.0;
const CYTOSCAPE_LEAF_BODY_BORDER_WIDTH_PX: f64 = 0.0;
const CYTOSCAPE_NODE_LABEL_MARGIN_OF_ERROR_PX: f64 = 2.0;
const CYTOSCAPE_PARENT_BODY_BORDER_WIDTH_PX: f64 = 1.0;
const CYTOSCAPE_FINAL_ELEMENT_BBOX_EXPANSION_PX: f64 = 1.0;

#[derive(Debug, Clone)]
pub(crate) struct ArchitectureServiceBoundsEstimate {
    // Actual emitted icon bounds used when grouped service labels should not affect root getBBox.
    pub(crate) emitted_icon_bounds: Bounds,
    // Approximation of Mermaid's final SVG getBBox() for top-level services.
    pub(crate) svg_root_bounds: Bounds,
    // Explicit Cytoscape child contribution phases for compound sizing.
    pub(crate) cytoscape_group_child_contribution: ArchitectureCytoscapeChildContributionBounds,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchitectureCytoscapeCanvasLabelMetrics {
    pub(crate) width: f64,
    pub(crate) half_width: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchitectureCytoscapeEdgeLabelMetrics {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchitectureCytoscapeChildLabelBounds {
    pub(crate) metrics: ArchitectureCytoscapeCanvasLabelMetrics,
    pub(crate) half_width: f64,
    pub(crate) bottom_extension_px: f64,
}

impl ArchitectureCytoscapeChildLabelBounds {
    fn bounds_for_icon(&self, icon_bounds: &Bounds) -> Bounds {
        let center_x = (icon_bounds.min_x + icon_bounds.max_x) / 2.0;
        Bounds {
            min_x: center_x - self.half_width,
            min_y: icon_bounds.min_y,
            max_x: center_x + self.half_width,
            max_y: icon_bounds.max_y + self.bottom_extension_px,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ArchitectureCytoscapeChildContributionBounds {
    // Raw icon bounds are retained for the emitted SVG geometry.
    pub(crate) body_bounds: Bounds,
    pub(crate) label_bounds: Option<Bounds>,
    // Compound sizing unions the cached body and label bounds without the final element expansion.
    pub(crate) union_bounds: Bounds,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ArchitectureNodeBBoxExtras {
    pub(crate) left: f64,
    pub(crate) right: f64,
    pub(crate) top: f64,
    pub(crate) bottom: f64,
}

pub(crate) fn architecture_cytoscape_canvas_label_metrics(
    label: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
) -> ArchitectureCytoscapeCanvasLabelMetrics {
    let width = label
        .split('\n')
        .map(|line| {
            measurer
                .measure_canvas_text_width_px(line, style)
                .max(0.0)
                .ceil()
        })
        .fold(0.0_f64, f64::max);
    let half_width = width / 2.0;
    ArchitectureCytoscapeCanvasLabelMetrics { width, half_width }
}

pub(crate) fn architecture_cytoscape_edge_label_metrics(
    label: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
) -> ArchitectureCytoscapeEdgeLabelMetrics {
    ArchitectureCytoscapeEdgeLabelMetrics {
        width: architecture_cytoscape_canvas_label_metrics(label, measurer, style).width,
        height: style.font_size.max(1.0),
    }
}

pub(crate) fn architecture_create_text_middle_bbox_y_range_px(
    text: &str,
    style: &TextStyle,
    line_count: usize,
    measurer: &dyn TextMeasurer,
) -> (f64, f64) {
    let first_line_height = measurer
        .measure_svg_tspan_text_bbox_height_px(text, style)
        .max(0.0);
    let extra_lines = line_count.max(1).saturating_sub(1) as f64;
    let height = first_line_height + style.font_size.max(1.0) * extra_lines * 1.1;
    let min_y = measurer.measure_svg_create_text_middle_bbox_y_offset_px(text, style);
    (min_y, min_y + height)
}

pub(crate) fn architecture_cytoscape_node_label_bottom_extension_px(font_size_px: f64) -> f64 {
    font_size_px.max(1.0) + CYTOSCAPE_NODE_LABEL_MARGIN_OF_ERROR_PX
}

pub(crate) fn architecture_svg_group_bbox_padding_px(padding_px: f64) -> f64 {
    padding_px.max(0.0)
        + CYTOSCAPE_PARENT_BODY_BORDER_WIDTH_PX / 2.0
        + CYTOSCAPE_FINAL_ELEMENT_BBOX_EXPANSION_PX
}

fn union_bounds(a: &Bounds, b: &Bounds) -> Bounds {
    Bounds {
        min_x: a.min_x.min(b.min_x),
        min_y: a.min_y.min(b.min_y),
        max_x: a.max_x.max(b.max_x),
        max_y: a.max_y.max(b.max_y),
    }
}

fn expand_bounds(bounds: &Bounds, amount: f64) -> Bounds {
    Bounds {
        min_x: bounds.min_x - amount,
        min_y: bounds.min_y - amount,
        max_x: bounds.max_x + amount,
        max_y: bounds.max_y + amount,
    }
}

pub(crate) fn architecture_cytoscape_child_contribution_bounds(
    icon_bounds: &Bounds,
    label_bounds: Option<&ArchitectureCytoscapeChildLabelBounds>,
) -> ArchitectureCytoscapeChildContributionBounds {
    let body_bounds = icon_bounds.clone();
    let body_bbox_bounds = expand_bounds(&body_bounds, CYTOSCAPE_FINAL_ELEMENT_BBOX_EXPANSION_PX);
    let label_bounds = label_bounds.map(|label| label.bounds_for_icon(&body_bounds));
    let union_bounds = label_bounds
        .as_ref()
        .map(|label| union_bounds(&body_bbox_bounds, label))
        .unwrap_or_else(|| body_bbox_bounds.clone());

    ArchitectureCytoscapeChildContributionBounds {
        body_bounds,
        label_bounds,
        union_bounds,
    }
}

fn architecture_cytoscape_final_element_bounds(
    icon_bounds: &Bounds,
    label_bounds: Option<&ArchitectureCytoscapeChildLabelBounds>,
) -> Bounds {
    let label_bounds = label_bounds.map(|label| label.bounds_for_icon(icon_bounds));
    let element_bounds = label_bounds
        .as_ref()
        .map(|label| union_bounds(icon_bounds, label))
        .unwrap_or_else(|| icon_bounds.clone());
    expand_bounds(&element_bounds, CYTOSCAPE_FINAL_ELEMENT_BBOX_EXPANSION_PX)
}

pub(crate) fn architecture_cytoscape_child_label_bounds(
    title: Option<&str>,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    font_size_px: f64,
) -> Option<ArchitectureCytoscapeChildLabelBounds> {
    let title = title.map(str::trim).filter(|t| !t.is_empty())?;
    let metrics = architecture_cytoscape_canvas_label_metrics(title, measurer, style);
    Some(ArchitectureCytoscapeChildLabelBounds {
        metrics,
        half_width: metrics.half_width + CYTOSCAPE_NODE_LABEL_MARGIN_OF_ERROR_PX,
        bottom_extension_px: architecture_cytoscape_node_label_bottom_extension_px(font_size_px),
    })
}

pub(crate) fn architecture_measure_cytoscape_final_node_bbox_extras(
    title: Option<&str>,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    icon_size: f64,
    font_size_px: f64,
) -> ArchitectureNodeBBoxExtras {
    let half_icon = icon_size / 2.0;
    let half_leaf_border = CYTOSCAPE_LEAF_BODY_BORDER_WIDTH_PX / 2.0;
    let body_bounds = Bounds {
        min_x: -half_icon - half_leaf_border,
        min_y: -half_icon - half_leaf_border,
        max_x: half_icon + half_leaf_border,
        max_y: half_icon + half_leaf_border,
    };
    let label_bounds =
        architecture_cytoscape_child_label_bounds(title, measurer, style, font_size_px);
    let element_bounds =
        architecture_cytoscape_final_element_bounds(&body_bounds, label_bounds.as_ref());
    architecture_cytoscape_bbox_extras(&element_bounds, half_icon)
}

pub(crate) fn architecture_measure_cytoscape_compound_child_bbox_extras(
    title: Option<&str>,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    icon_size: f64,
    font_size_px: f64,
) -> ArchitectureNodeBBoxExtras {
    let half_icon = icon_size / 2.0;
    let half_leaf_border = CYTOSCAPE_LEAF_BODY_BORDER_WIDTH_PX / 2.0;
    let body_bounds = Bounds {
        min_x: -half_icon - half_leaf_border,
        min_y: -half_icon - half_leaf_border,
        max_x: half_icon + half_leaf_border,
        max_y: half_icon + half_leaf_border,
    };
    let label_bounds =
        architecture_cytoscape_child_label_bounds(title, measurer, style, font_size_px);
    let contribution =
        architecture_cytoscape_child_contribution_bounds(&body_bounds, label_bounds.as_ref());
    architecture_cytoscape_bbox_extras(&contribution.union_bounds, half_icon)
}

fn architecture_cytoscape_bbox_extras(
    bounds: &Bounds,
    half_icon: f64,
) -> ArchitectureNodeBBoxExtras {
    let half_w = bounds.max_x.abs().max(bounds.min_x.abs());
    let half_w = (half_w * 2.0).round() / 2.0;
    let top = (-bounds.min_y - half_icon).max(0.0);
    let bottom = (bounds.max_y - half_icon).max(0.0);

    let extra_lr = (half_w - half_icon).max(0.0);
    ArchitectureNodeBBoxExtras {
        left: extra_lr,
        right: extra_lr,
        top,
        bottom,
    }
}

pub(crate) fn architecture_node_bbox_extras_to_manatee(
    extras: ArchitectureNodeBBoxExtras,
) -> manatee::BoundsExtras {
    manatee::BoundsExtras {
        left: extras.left,
        right: extras.right,
        top: extras.top,
        bottom: extras.bottom,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn architecture_estimate_service_bounds<TLine>(
    x: f64,
    y: f64,
    icon_size_px: f64,
    arch_font_size_px: f64,
    title: Option<&str>,
    text_measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
    compound_text_style: &TextStyle,
    wrap_svg_words_to_lines: impl Fn(&str, f64, &dyn TextMeasurer, &TextStyle) -> Vec<TLine>,
    svg_line_plain_text: impl Fn(&TLine) -> String,
    measure_svg_text_bbox_x: impl Fn(&str, &TextStyle) -> (f64, f64),
) -> ArchitectureServiceBoundsEstimate {
    let emitted_icon_bounds = Bounds {
        min_x: x,
        min_y: y,
        max_x: x + icon_size_px,
        max_y: y + icon_size_px,
    };
    let mut svg_root_bounds = emitted_icon_bounds.clone();
    let mut cytoscape_group_child_contribution =
        architecture_cytoscape_child_contribution_bounds(&emitted_icon_bounds, None);
    if let Some(title) = title.map(str::trim).filter(|t| !t.is_empty()) {
        let lines = wrap_svg_words_to_lines(title, icon_size_px * 1.5, text_measurer, text_style);
        let mut bbox_left_root = 0.0f64;
        let mut bbox_right_root = 0.0f64;
        let mut first_line_text = None;
        for line in &lines {
            let s = svg_line_plain_text(line);
            let (l, r) = measure_svg_text_bbox_x(s.as_str(), text_style);
            bbox_left_root = bbox_left_root.max(l);
            bbox_right_root = bbox_right_root.max(r);
            first_line_text.get_or_insert(s);
        }
        let line_count_root = lines.len().max(1);
        let (_, label_extra_bottom_root) = architecture_create_text_middle_bbox_y_range_px(
            first_line_text.as_deref().unwrap_or(title),
            text_style,
            line_count_root,
            text_measurer,
        );

        let cx = x + icon_size_px / 2.0;
        let text_left_root = cx - bbox_left_root;
        let text_right_root = cx + bbox_right_root;
        let text_bottom_root = y + icon_size_px + label_extra_bottom_root;

        svg_root_bounds = Bounds {
            min_x: svg_root_bounds.min_x.min(text_left_root),
            min_y: svg_root_bounds.min_y,
            max_x: svg_root_bounds.max_x.max(text_right_root),
            max_y: svg_root_bounds.max_y.max(text_bottom_root),
        };
        if let Some(cytoscape_label_bounds) = architecture_cytoscape_child_label_bounds(
            Some(title),
            text_measurer,
            compound_text_style,
            arch_font_size_px,
        ) {
            cytoscape_group_child_contribution = architecture_cytoscape_child_contribution_bounds(
                &emitted_icon_bounds,
                Some(&cytoscape_label_bounds),
            );
        }
    }

    ArchitectureServiceBoundsEstimate {
        emitted_icon_bounds,
        svg_root_bounds,
        cytoscape_group_child_contribution,
    }
}

#[cfg(test)]
mod tests {
    struct CanvasProbe;

    impl crate::text::TextMeasurer for CanvasProbe {
        fn measure(
            &self,
            _text: &str,
            _style: &crate::text::TextStyle,
        ) -> crate::text::TextMetrics {
            panic!("Architecture labels must use the Canvas2D operation")
        }

        fn measure_canvas_text_width_px(&self, text: &str, _style: &crate::text::TextStyle) -> f64 {
            match text {
                "edge" => 32.2,
                "short" => 40.1,
                "long" => 100.2,
                "widest" => 94.2,
                other => panic!("unexpected canvas probe: {other}"),
            }
        }
    }

    fn cytoscape_text_style(font_size: f64) -> crate::text::TextStyle {
        crate::text::TextStyle {
            font_family: Some("Helvetica Neue,Helvetica,sans-serif".to_string()),
            font_size,
            font_weight: None,
            font_style: None,
        }
    }

    struct CreateTextVerticalProbe;

    impl crate::text::TextMeasurer for CreateTextVerticalProbe {
        fn measure(
            &self,
            _text: &str,
            _style: &crate::text::TextStyle,
        ) -> crate::text::TextMetrics {
            panic!("Architecture createText bounds must use exact SVG operations")
        }

        fn measure_svg_tspan_text_bbox_height_px(
            &self,
            text: &str,
            _style: &crate::text::TextStyle,
        ) -> f64 {
            assert_eq!(text, "First line");
            19.0
        }

        fn measure_svg_create_text_middle_bbox_y_offset_px(
            &self,
            text: &str,
            _style: &crate::text::TextStyle,
        ) -> f64 {
            assert_eq!(text, "First line");
            5.1875
        }
    }

    #[test]
    fn architecture_text_phases_match_mermaid_source_and_operation_metrics() {
        let style = crate::text::TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        assert_eq!(
            super::architecture_create_text_middle_bbox_y_range_px(
                "First line",
                &style,
                1,
                &CreateTextVerticalProbe,
            ),
            (5.1875, 24.1875)
        );
        let multiline = super::architecture_create_text_middle_bbox_y_range_px(
            "First line",
            &style,
            2,
            &CreateTextVerticalProbe,
        );
        assert!((multiline.0 - 5.1875).abs() < 1e-9);
        assert!((multiline.1 - 41.7875).abs() < 1e-9);
        assert_eq!(
            super::architecture_cytoscape_node_label_bottom_extension_px(16.0),
            18.0
        );
        assert_eq!(
            super::architecture_cytoscape_node_label_bottom_extension_px(12.0),
            14.0
        );
        assert_eq!(super::ARCHITECTURE_SERVICE_LABEL_BOTTOM_EXTENSION_PX, 18.0);
        assert_eq!(super::ARCHITECTURE_CREATE_TEXT_DEFAULT_WRAP_WIDTH_PX, 200.0);
        assert_eq!(super::CYTOSCAPE_LEAF_BODY_BORDER_WIDTH_PX, 0.0);
        assert_eq!(super::CYTOSCAPE_NODE_LABEL_MARGIN_OF_ERROR_PX, 2.0);
        assert_eq!(super::CYTOSCAPE_PARENT_BODY_BORDER_WIDTH_PX, 1.0);
        assert_eq!(super::CYTOSCAPE_FINAL_ELEMENT_BBOX_EXPANSION_PX, 1.0);
    }

    #[test]
    fn architecture_node_bbox_extras_convert_to_manatee_bounds_extras() {
        let extras = super::ArchitectureNodeBBoxExtras {
            left: 1.5,
            right: 2.5,
            top: 3.5,
            bottom: 4.5,
        };
        let mapped = super::architecture_node_bbox_extras_to_manatee(extras);
        assert_eq!(mapped.left, 1.5);
        assert_eq!(mapped.right, 2.5);
        assert_eq!(mapped.top, 3.5);
        assert_eq!(mapped.bottom, 4.5);
    }

    #[test]
    fn architecture_leaf_bbox_extras_keep_body_label_and_final_expansion_separate() {
        let style = cytoscape_text_style(16.0);

        let no_label = super::architecture_measure_cytoscape_final_node_bbox_extras(
            None,
            &CanvasProbe,
            &style,
            80.0,
            16.0,
        );
        assert_eq!(no_label.left, 1.0);
        assert_eq!(no_label.right, 1.0);
        assert_eq!(no_label.top, 1.0);
        assert_eq!(no_label.bottom, 1.0);

        let short_label = super::architecture_measure_cytoscape_final_node_bbox_extras(
            Some("short"),
            &CanvasProbe,
            &style,
            80.0,
            16.0,
        );
        assert_eq!(short_label.left, 1.0);
        assert_eq!(short_label.right, 1.0);
        assert_eq!(short_label.top, 1.0);
        assert_eq!(short_label.bottom, 19.0);

        let long_label = super::architecture_measure_cytoscape_final_node_bbox_extras(
            Some("long"),
            &CanvasProbe,
            &style,
            80.0,
            16.0,
        );
        assert_eq!(long_label.left, 13.5);
        assert_eq!(long_label.right, 13.5);
        assert_eq!(long_label.top, 1.0);
        assert_eq!(long_label.bottom, 19.0);
    }

    #[test]
    fn architecture_canvas_label_metrics_use_canvas_width_and_cytoscape_ceiling() {
        let style = cytoscape_text_style(16.0);
        let metrics = super::architecture_cytoscape_canvas_label_metrics(
            "short\nwidest",
            &CanvasProbe,
            &style,
        );
        assert_eq!(metrics.width, 95.0);
        assert_eq!(metrics.half_width, 47.5);
    }

    #[test]
    fn architecture_edge_label_metrics_use_canvas_ceiling_and_single_line_height() {
        let style = cytoscape_text_style(16.0);
        let metrics =
            super::architecture_cytoscape_edge_label_metrics("edge", &CanvasProbe, &style);

        assert_eq!(metrics.width, 33.0);
        assert_eq!(metrics.height, 16.0);
    }

    #[test]
    fn architecture_cytoscape_child_label_bounds_centralize_compound_child_label_phase() {
        let style = crate::text::TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 12.0,
            font_weight: None,
            font_style: None,
        };
        let measurer = crate::text::DeterministicTextMeasurer::default();

        let label_bounds = super::architecture_cytoscape_child_label_bounds(
            Some("API gateway"),
            &measurer,
            &style,
            12.0,
        )
        .expect("non-empty title has Cytoscape child label bounds");
        let direct_metrics =
            super::architecture_cytoscape_canvas_label_metrics("API gateway", &measurer, &style);

        assert_eq!(label_bounds.metrics.width, direct_metrics.width);
        assert_eq!(
            label_bounds.half_width,
            direct_metrics.half_width + super::CYTOSCAPE_NODE_LABEL_MARGIN_OF_ERROR_PX
        );
        assert_eq!(label_bounds.bottom_extension_px, 14.0);
        assert!(
            super::architecture_cytoscape_child_label_bounds(Some("   "), &measurer, &style, 12.0)
                .is_none()
        );
    }

    #[test]
    fn architecture_cytoscape_child_label_bounds_extend_icon_bounds_by_phase() {
        let label_bounds = super::ArchitectureCytoscapeChildLabelBounds {
            metrics: super::ArchitectureCytoscapeCanvasLabelMetrics {
                width: 96.0,
                half_width: 48.0,
            },
            half_width: 50.0,
            bottom_extension_px: 18.0,
        };
        let icon_bounds = crate::model::Bounds {
            min_x: 10.0,
            min_y: 20.0,
            max_x: 90.0,
            max_y: 100.0,
        };

        let bounds = label_bounds.bounds_for_icon(&icon_bounds);
        assert_eq!(bounds.min_x, 0.0);
        assert_eq!(bounds.min_y, 20.0);
        assert_eq!(bounds.max_x, 100.0);
        assert_eq!(bounds.max_y, 118.0);
    }

    #[test]
    fn architecture_cytoscape_child_contribution_bounds_preserve_body_label_union_phases() {
        let icon_bounds = crate::model::Bounds {
            min_x: 10.0,
            min_y: 20.0,
            max_x: 90.0,
            max_y: 100.0,
        };

        let without_label =
            super::architecture_cytoscape_child_contribution_bounds(&icon_bounds, None);
        assert_eq!(without_label.body_bounds.min_x, icon_bounds.min_x);
        assert_eq!(without_label.body_bounds.max_y, icon_bounds.max_y);
        assert!(without_label.label_bounds.is_none());
        assert_eq!(without_label.union_bounds.min_x, icon_bounds.min_x - 1.0);
        assert_eq!(without_label.union_bounds.max_y, icon_bounds.max_y + 1.0);

        let label_bounds = super::ArchitectureCytoscapeChildLabelBounds {
            metrics: super::ArchitectureCytoscapeCanvasLabelMetrics {
                width: 96.0,
                half_width: 48.0,
            },
            half_width: 50.0,
            bottom_extension_px: 18.0,
        };
        let with_label = super::architecture_cytoscape_child_contribution_bounds(
            &icon_bounds,
            Some(&label_bounds),
        );

        let child_label = with_label
            .label_bounds
            .as_ref()
            .expect("label phase is preserved");
        assert_eq!(child_label.min_x, 0.0);
        assert_eq!(child_label.max_y, 118.0);
        assert_eq!(with_label.body_bounds.min_x, icon_bounds.min_x);
        assert_eq!(with_label.union_bounds.min_x, 0.0);
        assert_eq!(with_label.union_bounds.max_y, 118.0);
    }

    #[test]
    fn architecture_compound_child_and_final_element_bounds_split_label_phases() {
        let style = cytoscape_text_style(18.0);
        let icon_bounds = crate::model::Bounds {
            min_x: -20.0,
            min_y: -20.0,
            max_x: 20.0,
            max_y: 20.0,
        };

        for (title, expected_child, expected_final, expected_child_extras) in [
            (
                None,
                (-21.0, -21.0, 21.0, 21.0),
                (-21.0, -21.0, 21.0, 21.0),
                (1.0, 1.0, 1.0),
            ),
            (
                Some("short"),
                (-22.5, -21.0, 22.5, 40.0),
                (-23.5, -21.0, 23.5, 41.0),
                (2.5, 1.0, 20.0),
            ),
            (
                Some("widest"),
                (-49.5, -21.0, 49.5, 40.0),
                (-50.5, -21.0, 50.5, 41.0),
                (29.5, 1.0, 20.0),
            ),
        ] {
            let label_bounds =
                super::architecture_cytoscape_child_label_bounds(title, &CanvasProbe, &style, 18.0);
            let child = super::architecture_cytoscape_child_contribution_bounds(
                &icon_bounds,
                label_bounds.as_ref(),
            );
            let final_element = super::architecture_cytoscape_final_element_bounds(
                &icon_bounds,
                label_bounds.as_ref(),
            );

            assert_eq!(
                (
                    child.union_bounds.min_x,
                    child.union_bounds.min_y,
                    child.union_bounds.max_x,
                    child.union_bounds.max_y,
                ),
                expected_child,
                "unexpected compound child bounds for {title:?}"
            );
            assert_eq!(
                (
                    final_element.min_x,
                    final_element.min_y,
                    final_element.max_x,
                    final_element.max_y,
                ),
                expected_final,
                "unexpected final element bounds for {title:?}"
            );

            let child_extras = super::architecture_measure_cytoscape_compound_child_bbox_extras(
                title,
                &CanvasProbe,
                &style,
                40.0,
                18.0,
            );
            assert_eq!(
                (child_extras.left, child_extras.top, child_extras.bottom,),
                expected_child_extras,
                "unexpected compound child extras for {title:?}"
            );
            assert_eq!(child_extras.right, child_extras.left);
        }
    }

    #[test]
    fn architecture_svg_group_bbox_padding_follows_border_and_final_expansion_phases() {
        assert_eq!(super::architecture_svg_group_bbox_padding_px(0.0), 1.5);
        assert_eq!(super::architecture_svg_group_bbox_padding_px(12.0), 13.5);
        assert_eq!(super::architecture_svg_group_bbox_padding_px(-7.0), 1.5);
    }
}
