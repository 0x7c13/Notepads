use crate::Error;
use crate::text::{TextMeasurer, TextMetrics, TextStyle};
use crate::wardley::{
    WardleyArrowDirection, WardleyFontWeight, WardleyNodeShapeLayout, WardleySourceOverlayLayout,
    layout_wardley_diagram_typed,
};
use merman_core::diagrams::wardley::{
    WardleyAcceleratorRenderModel, WardleyAnnotationRenderModel, WardleyAxesRenderModel,
    WardleyDeacceleratorRenderModel, WardleyDiagramRenderModel, WardleyFlowDirection,
    WardleyLinkRenderModel, WardleyNodeRenderModel, WardleyNoteRenderModel,
    WardleyPipelineRenderModel, WardleyPointRenderModel, WardleySizeRenderModel,
    WardleySourceStrategy, WardleyTrendRenderModel,
};
use serde_json::json;
use std::cell::RefCell;

const EPSILON: f64 = 1.0e-9;

#[derive(Default)]
struct PrimitiveProbe {
    generic_calls: RefCell<Vec<String>>,
    computed_calls: RefCell<Vec<(String, f64)>>,
    raw_height_calls: RefCell<Vec<(String, f64)>>,
}

impl TextMeasurer for PrimitiveProbe {
    fn measure(&self, text: &str, _style: &TextStyle) -> TextMetrics {
        self.generic_calls.borrow_mut().push(text.to_string());
        TextMetrics {
            width: 999.0,
            height: 999.0,
            line_count: 1,
        }
    }

    fn measure_svg_text_computed_length_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.computed_calls
            .borrow_mut()
            .push((text.to_string(), style.font_size));
        if text.starts_with("2.") { 80.0 } else { 40.0 }
    }

    fn measure_svg_raw_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.raw_height_calls
            .borrow_mut()
            .push((text.to_string(), style.font_size));
        if text.starts_with("2.") { 24.0 } else { 10.0 }
    }
}

fn node(id: &str, x: f64, y: f64) -> WardleyNodeRenderModel {
    WardleyNodeRenderModel {
        id: id.to_string(),
        label: id.to_string(),
        x,
        y,
        class_name: None,
        label_offset_x: None,
        label_offset_y: None,
        in_pipeline: false,
        is_pipeline_parent: false,
        inertia: None,
        source_strategy: None,
    }
}

fn point(x: f64, y: f64) -> WardleyPointRenderModel {
    WardleyPointRenderModel { x, y }
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= EPSILON,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn config_size_and_title_precedence_are_resolved_before_svg() {
    let defaults = layout_wardley_diagram_typed(
        &WardleyDiagramRenderModel::default(),
        None,
        &json!({}),
        &PrimitiveProbe::default(),
    )
    .expect("default wardley layout");
    assert_eq!(defaults.width, 900.0);
    assert_eq!(defaults.height, 600.0);
    assert_eq!(defaults.padding, 48.0);
    assert_eq!(defaults.node_radius, 6.0);
    assert_eq!(defaults.node_label_offset, 8.0);
    assert_eq!(defaults.axis_font_size, 12.0);
    assert_eq!(defaults.label_font_size, 10.0);
    assert_close(defaults.square_size, 9.6);
    assert!(defaults.use_max_width);
    assert!(defaults.grid.is_empty());

    let mut model = WardleyDiagramRenderModel {
        title: Some("Body title".to_string()),
        size: Some(WardleySizeRenderModel {
            width: 640.0,
            height: 480.0,
        }),
        ..Default::default()
    };
    model.nodes.push(node("configured", 50.0, 50.0));
    let config = json!({
        "wardley-beta": {
            "width": 1000,
            "height": 700,
            "padding": 40,
            "nodeRadius": 10,
            "nodeLabelOffset": 12,
            "axisFontSize": 15,
            "labelFontSize": 13,
            "showGrid": true,
            "useMaxWidth": false
        }
    });

    let layout = layout_wardley_diagram_typed(
        &model,
        Some("Metadata title"),
        &config,
        &PrimitiveProbe::default(),
    )
    .expect("wardley layout");

    assert_eq!(layout.width, 640.0);
    assert_eq!(layout.height, 480.0);
    assert!(!layout.use_max_width);
    assert_eq!(layout.padding, 40.0);
    assert_eq!(layout.chart_width, 560.0);
    assert_eq!(layout.chart_height, 400.0);
    assert_eq!(layout.node_radius, 10.0);
    assert_eq!(layout.node_label_offset, 12.0);
    assert_eq!(layout.axis_font_size, 15.0);
    assert_eq!(layout.label_font_size, 13.0);
    assert_eq!(layout.square_size, 16.0);
    assert_eq!(layout.grid.len(), 3);
    assert_eq!(
        layout.title.as_ref().map(|title| title.text.as_str()),
        Some("Body title")
    );
    assert_eq!(
        layout.title.as_ref().map(|title| title.font_size),
        Some(15.75)
    );
    assert_close(layout.nodes[0].label_layout.x, 332.0);
    assert_close(layout.nodes[0].label_layout.y, 228.0);
    serde_json::to_value(&layout).expect("layout must serialize");

    model.title = None;
    let fallback = layout_wardley_diagram_typed(
        &model,
        Some("Metadata title"),
        &json!({}),
        &PrimitiveProbe::default(),
    )
    .expect("wardley fallback title layout");
    assert_eq!(
        fallback.title.as_ref().map(|title| title.text.as_str()),
        Some("Metadata title")
    );
    assert_eq!(
        fallback.width, 640.0,
        "diagram size still overrides defaults"
    );
}

#[test]
fn stages_use_complete_custom_boundaries_or_equal_distribution() {
    let custom = WardleyDiagramRenderModel {
        axes: WardleyAxesRenderModel {
            stages: vec!["A".into(), "B".into(), "C".into()],
            stage_boundaries: vec![0.2, 0.7, 1.0],
            ..Default::default()
        },
        ..Default::default()
    };
    let custom_layout =
        layout_wardley_diagram_typed(&custom, None, &json!({}), &PrimitiveProbe::default())
            .expect("custom stage layout");
    assert_eq!(custom_layout.stages.len(), 3);
    assert_close(custom_layout.stages[0].start_x, 48.0);
    assert_close(custom_layout.stages[0].end_x, 208.8);
    assert_close(custom_layout.stages[1].start_x, 208.8);
    assert_close(custom_layout.stages[1].end_x, 610.8);
    assert!(custom_layout.stages[0].divider.is_none());
    assert_close(
        custom_layout.stages[1]
            .divider
            .as_ref()
            .expect("second divider")
            .x1,
        208.8,
    );

    let mut equal = custom;
    equal.axes.stage_boundaries = vec![0.2];
    let equal_layout =
        layout_wardley_diagram_typed(&equal, None, &json!({}), &PrimitiveProbe::default())
            .expect("equal stage layout");
    assert_close(equal_layout.stages[0].end_x, 316.0);
    assert_close(equal_layout.stages[1].end_x, 584.0);
    assert_close(equal_layout.stages[2].end_x, 852.0);
}

#[test]
fn pipelines_sort_children_relocate_parent_and_filter_child_parent_links() {
    let mut parent = node("parent", 50.0, 50.0);
    parent.is_pipeline_parent = true;
    let model = WardleyDiagramRenderModel {
        nodes: vec![
            parent,
            node("c3", 80.0, 40.0),
            node("c1", 20.0, 40.0),
            node("c2", 50.0, 40.0),
            node("outside", 90.0, 80.0),
        ],
        pipelines: vec![WardleyPipelineRenderModel {
            node_id: "parent".into(),
            component_ids: vec!["c3".into(), "c1".into(), "missing".into(), "c2".into()],
        }],
        links: vec![
            WardleyLinkRenderModel {
                source: "c1".into(),
                target: "parent".into(),
                dashed: false,
                label: None,
                flow: None,
            },
            WardleyLinkRenderModel {
                source: "parent".into(),
                target: "outside".into(),
                dashed: false,
                label: None,
                flow: None,
            },
            WardleyLinkRenderModel {
                source: "missing".into(),
                target: "outside".into(),
                dashed: false,
                label: None,
                flow: None,
            },
        ],
        ..Default::default()
    };

    let layout = layout_wardley_diagram_typed(&model, None, &json!({}), &PrimitiveProbe::default())
        .expect("pipeline layout");
    assert_eq!(layout.pipeline_links.len(), 2);
    assert_eq!(
        layout
            .pipeline_links
            .iter()
            .map(|link| (link.source.as_str(), link.target.as_str()))
            .collect::<Vec<_>>(),
        [("c1", "c2"), ("c2", "c3")]
    );
    let pipeline = &layout.pipeline_boxes[0];
    assert_close(pipeline.rect.x, 193.8);
    assert_close(pipeline.rect.y, 338.4);
    assert_close(pipeline.rect.width, 512.4);
    assert_close(pipeline.rect.height, 24.0);
    let parent = layout
        .nodes
        .iter()
        .find(|node| node.id == "parent")
        .expect("pipeline parent");
    assert_close(parent.position.x, 450.0);
    assert_close(parent.position.y, 336.8);
    assert_eq!(layout.links.len(), 1);
    assert_eq!(layout.links[0].source, "parent");
    assert!(
        layout.links[0].line.y1 < 336.8,
        "link uses relocated parent"
    );
}

#[test]
fn links_clip_endpoints_place_readable_labels_and_precompute_flow_markers() {
    let model = WardleyDiagramRenderModel {
        nodes: vec![node("a", 80.0, 30.0), node("b", 20.0, 70.0)],
        links: vec![
            WardleyLinkRenderModel {
                source: "a".into(),
                target: "b".into(),
                dashed: true,
                label: Some("flow".into()),
                flow: Some(WardleyFlowDirection::Forward),
            },
            WardleyLinkRenderModel {
                source: "a".into(),
                target: "b".into(),
                dashed: false,
                label: None,
                flow: Some(WardleyFlowDirection::Backward),
            },
            WardleyLinkRenderModel {
                source: "a".into(),
                target: "b".into(),
                dashed: false,
                label: None,
                flow: Some(WardleyFlowDirection::Bidirectional),
            },
            WardleyLinkRenderModel {
                source: "a".into(),
                target: "b".into(),
                dashed: false,
                label: None,
                flow: None,
            },
        ],
        ..Default::default()
    };
    let layout = layout_wardley_diagram_typed(&model, None, &json!({}), &PrimitiveProbe::default())
        .expect("link layout");
    let first = &layout.links[0];
    let a = &layout.nodes[0].position;
    let b = &layout.nodes[1].position;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let distance = dx.hypot(dy);
    assert_close(first.line.x1, a.x + dx / distance * 6.0);
    assert_close(first.line.y1, a.y + dy / distance * 6.0);
    assert_close(first.line.x2, b.x - dx / distance * 6.0);
    assert_close(first.line.y2, b.y - dy / distance * 6.0);
    assert_eq!((first.markers.start, first.markers.end), (false, true));
    assert_eq!(
        layout
            .links
            .iter()
            .map(|link| (link.markers.start, link.markers.end))
            .collect::<Vec<_>>(),
        [(false, true), (true, false), (true, true), (false, false)]
    );
    let label = first.label.as_ref().expect("link label");
    assert_close(label.x, (a.x + b.x) / 2.0 + dy / distance * 8.0);
    assert_close(label.y, (a.y + b.y) / 2.0 - dx / distance * 8.0);
    let expected_angle = dy.atan2(dx).to_degrees() + 180.0;
    assert_close(
        label.rotation.as_ref().expect("label rotation").degrees,
        expected_angle,
    );
}

#[test]
fn zero_distance_links_are_rejected_instead_of_serializing_nan() {
    let model = WardleyDiagramRenderModel {
        nodes: vec![node("a", 50.0, 50.0), node("b", 50.0, 50.0)],
        links: vec![WardleyLinkRenderModel {
            source: "a".into(),
            target: "b".into(),
            dashed: false,
            label: None,
            flow: None,
        }],
        ..Default::default()
    };
    let error = layout_wardley_diagram_typed(&model, None, &json!({}), &PrimitiveProbe::default())
        .expect_err("coincident link endpoints must fail");
    assert!(matches!(error, Error::InvalidModel { .. }));
    assert!(error.to_string().contains("a"));
    assert!(error.to_string().contains("b"));
}

#[test]
fn strategies_market_inertia_and_labels_are_fully_positioned() {
    let mut anchor = node("anchor", 10.0, 90.0);
    anchor.class_name = Some("anchor".into());
    let mut build = node("build", 30.0, 60.0);
    build.source_strategy = Some(WardleySourceStrategy::Build);
    build.inertia = Some(true);
    let mut buy = node("buy", 40.0, 60.0);
    buy.source_strategy = Some(WardleySourceStrategy::Buy);
    let mut outsource = node("outsource", 50.0, 60.0);
    outsource.source_strategy = Some(WardleySourceStrategy::Outsource);
    outsource.label_offset_x = Some(2);
    outsource.label_offset_y = Some(3);
    let mut market = node("market", 70.0, 60.0);
    market.source_strategy = Some(WardleySourceStrategy::Market);
    let mut parent = node("parent", 90.0, 60.0);
    parent.is_pipeline_parent = true;
    parent.inertia = Some(true);
    let model = WardleyDiagramRenderModel {
        nodes: vec![anchor, build, buy, outsource, market, parent],
        ..Default::default()
    };
    let layout = layout_wardley_diagram_typed(&model, None, &json!({}), &PrimitiveProbe::default())
        .expect("node layout");

    let anchor = &layout.nodes[0];
    assert!(matches!(anchor.shape, WardleyNodeShapeLayout::Anchor));
    assert_close(anchor.label_layout.x, anchor.position.x);
    assert_close(anchor.label_layout.y, anchor.position.y - 3.0);
    assert_eq!(anchor.label_layout.font_weight, WardleyFontWeight::Bold);

    let build = &layout.nodes[1];
    assert!(matches!(
        build.source_overlay,
        Some(WardleySourceOverlayLayout::Build { .. })
    ));
    assert_close(build.label_layout.x, build.position.x + 18.0);
    assert_close(build.label_layout.y, build.position.y - 18.0);
    let inertia = build.inertia.as_ref().expect("build inertia");
    assert_close(inertia.x1, build.position.x + 37.0);
    assert_close(inertia.y1, build.position.y - 6.0);
    assert_close(inertia.y2, build.position.y + 6.0);

    assert!(matches!(
        layout.nodes[2].source_overlay,
        Some(WardleySourceOverlayLayout::Buy { .. })
    ));
    let outsource = &layout.nodes[3];
    assert!(matches!(
        outsource.source_overlay,
        Some(WardleySourceOverlayLayout::Outsource { .. })
    ));
    assert_close(outsource.label_layout.x, outsource.position.x + 2.0);
    assert_close(outsource.label_layout.y, outsource.position.y + 3.0);

    let market = &layout.nodes[4];
    let WardleySourceOverlayLayout::Market {
        outer_circle,
        connectors,
        dots,
    } = market.source_overlay.as_ref().expect("market overlay")
    else {
        panic!("expected market overlay");
    };
    assert_eq!(connectors.len(), 3);
    assert_eq!(dots.len(), 3);
    assert_eq!(outer_circle.radius, 12.0);
    assert_close(dots[0].radius, 4.2);
    assert_close(dots[0].center.y, market.position.y - 7.2);

    let parent = &layout.nodes[5];
    assert!(matches!(
        parent.shape,
        WardleyNodeShapeLayout::PipelineSquare { .. }
    ));
    let inertia = parent.inertia.as_ref().expect("parent inertia");
    assert_close(inertia.x1, parent.position.x + 19.8);
    assert_close(inertia.y2 - inertia.y1, 9.6);
}

#[test]
fn annotations_use_exact_dom_primitives_source_buffer_and_clamping() {
    let probe = PrimitiveProbe::default();
    let model = WardleyDiagramRenderModel {
        annotations: vec![
            WardleyAnnotationRenderModel {
                number: 2,
                coordinates: vec![point(10.0, 20.0), point(30.0, 40.0)],
                text: Some("long".into()),
            },
            WardleyAnnotationRenderModel {
                number: 1,
                coordinates: vec![point(50.0, 60.0)],
                text: Some("short".into()),
            },
            WardleyAnnotationRenderModel {
                number: 3,
                coordinates: vec![point(70.0, 80.0)],
                text: Some(String::new()),
            },
        ],
        annotations_box: Some(point(95.0, 0.0)),
        ..Default::default()
    };
    let layout =
        layout_wardley_diagram_typed(&model, None, &json!({}), &probe).expect("annotation layout");
    assert_eq!(layout.annotations.len(), 3);
    assert_eq!(layout.annotations[0].segments.len(), 1);
    assert_eq!(layout.annotations[0].points[0].radius, 10.0);

    let annotation_box = layout.annotations_box.as_ref().expect("annotations box");
    let rect = annotation_box.rect.as_ref().expect("annotations box rect");
    assert_eq!(annotation_box.lines.len(), 2);
    assert_eq!(annotation_box.lines[0].text, "1. short");
    assert_eq!(annotation_box.lines[1].text, "2. long");
    assert_eq!(annotation_box.max_text_width, 80.0);
    assert_eq!(annotation_box.max_text_height, 24.0);
    assert_eq!(rect.width, 205.0, "80 + 2 * 10 + pinned 105 buffer");
    assert_eq!(rect.height, 64.0);
    assert_eq!(rect.x, 647.0);
    assert_eq!(rect.y, 488.0);
    assert_eq!(annotation_box.lines[0].x, 657.0);
    assert_eq!(annotation_box.lines[0].y, 514.0);
    assert_eq!(annotation_box.lines[1].y, 530.0);
    assert!(probe.generic_calls.borrow().is_empty());
    assert_eq!(
        probe.computed_calls.borrow().as_slice(),
        [
            ("1. short".to_string(), 11.0),
            ("2. long".to_string(), 11.0)
        ]
    );
    assert_eq!(
        probe.raw_height_calls.borrow().as_slice(),
        [
            ("1. short".to_string(), 11.0),
            ("2. long".to_string(), 11.0)
        ]
    );
}

#[test]
fn notes_trends_and_acceleration_arrows_are_projected_once() {
    let model = WardleyDiagramRenderModel {
        nodes: vec![node("origin", 10.0, 20.0)],
        trends: vec![
            WardleyTrendRenderModel {
                node_id: "origin".into(),
                target_x: 50.0,
                target_y: 60.0,
            },
            WardleyTrendRenderModel {
                node_id: "origin".into(),
                target_x: 10.0,
                target_y: 20.0,
            },
        ],
        notes: vec![WardleyNoteRenderModel {
            text: "A note".into(),
            x: 25.0,
            y: 75.0,
        }],
        accelerators: vec![WardleyAcceleratorRenderModel {
            name: "Fast".into(),
            x: 20.0,
            y: 30.0,
        }],
        deaccelerators: vec![WardleyDeacceleratorRenderModel {
            name: "Slow".into(),
            x: 60.0,
            y: 70.0,
        }],
        ..Default::default()
    };
    let layout = layout_wardley_diagram_typed(&model, None, &json!({}), &PrimitiveProbe::default())
        .expect("miscellaneous wardley layout");

    assert_eq!(layout.trends.len(), 2);
    let trend = &layout.trends[0];
    assert_close(trend.target.x, 450.0);
    assert_close(trend.target.y, 249.6);
    let dx = trend.target.x - trend.origin.x;
    let dy = trend.target.y - trend.origin.y;
    let distance = dx.hypot(dy);
    assert_close(trend.line.x2, trend.target.x - dx / distance * 8.0);
    assert_close(trend.line.y2, trend.target.y - dy / distance * 8.0);
    assert_eq!(layout.trends[1].line.x2, layout.trends[1].target.x);
    assert_eq!(layout.trends[1].line.y2, layout.trends[1].target.y);

    assert_eq!(layout.notes[0].text.text, "A note");
    assert_eq!(layout.notes[0].text.font_size, 11.0);
    assert_eq!(layout.notes[0].text.font_weight, WardleyFontWeight::Bold);

    let accelerator = &layout.accelerators[0];
    assert_eq!(accelerator.direction, WardleyArrowDirection::Right);
    assert_eq!(accelerator.width, 60.0);
    assert_eq!(accelerator.height, 30.0);
    assert_eq!(accelerator.head_width, 20.0);
    assert_eq!(accelerator.path.len(), 7);
    assert_eq!(accelerator.path[0].x, accelerator.origin.x);
    assert_eq!(accelerator.path[0].y, accelerator.origin.y - 15.0);
    assert_eq!(accelerator.label.x, accelerator.origin.x + 30.0);
    assert_eq!(accelerator.label.y, accelerator.origin.y + 30.0);

    let deaccelerator = &layout.deaccelerators[0];
    assert_eq!(deaccelerator.direction, WardleyArrowDirection::Left);
    assert_eq!(deaccelerator.path[0].x, deaccelerator.origin.x + 60.0);
    assert_eq!(deaccelerator.path[3].x, deaccelerator.origin.x);
    assert_eq!(deaccelerator.label.x, deaccelerator.origin.x + 30.0);
    assert_eq!(deaccelerator.label.y, deaccelerator.origin.y + 30.0);
}
