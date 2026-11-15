//! Controlled browser measurements isolate the endpoint renderer from font/layout drift.
//! The companion integration test runs the provider; xtask binds this matrix to source hashes.

use super::svg_emit::{FlowchartSvgModelRequest, render_flowchart_svg_model};
use crate::environment::RenderEnvironment;
use crate::model::{FlowchartLayout, LayoutPoint};
use crate::svg::{SvgDebugOptions, SvgExecution, SvgRenderOptions};
use crate::text::{DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle, WrapMode};
use merman_core::{Engine, ParseOptions, RenderSemanticModel};
use serde_json::Value;
use std::path::Path;

struct BrowserLabelBoxes {
    raw: std::collections::HashMap<String, TextMetrics>,
    svg_markdown: std::collections::HashMap<String, TextMetrics>,
}
impl TextMeasurer for BrowserLabelBoxes {
    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
        self.raw
            .get(text)
            .copied()
            .unwrap_or_else(|| DeterministicTextMeasurer::default().measure(text, style))
    }
    fn measure_wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        mode: WrapMode,
    ) -> TextMetrics {
        self.raw
            .get(text)
            .copied()
            .or_else(|| {
                // SVG Markdown measures the complete plain label after tokenization/wrapping.
                // Reuse the captured final bbox for that full text, not for word-width probes.
                (mode != WrapMode::HtmlLike && max_width.is_none())
                    .then(|| {
                        self.svg_markdown
                            .get(&text.split_whitespace().collect::<Vec<_>>().join(" "))
                            .copied()
                    })
                    .flatten()
            })
            .unwrap_or_else(|| {
                DeterministicTextMeasurer::default().measure_wrapped(text, style, max_width, mode)
            })
    }
}

#[test]
fn flowchart_browser_measured_terminals_preserve_upstream_geometry() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let matrix: Value = serde_json::from_str(include_str!(
        "../../../../../../fixtures/_verification/flowchart-elk-browser-measurements.json"
    ))
    .unwrap();
    let mut mismatches = Vec::new();
    let number =
        regex::Regex::new(r"[-+]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][-+]?[0-9]+)?").unwrap();
    for fixture in matrix["entries"].as_array().unwrap() {
        let name = fixture["fixture"].as_str().unwrap();
        let source =
            std::fs::read_to_string(root.join("fixtures/flowchart").join(format!("{name}.mmd")))
                .unwrap();
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(&source, ParseOptions::default())
            .unwrap()
            .unwrap();
        let render_context = parsed.flowchart_render_context().unwrap().clone();
        let environment = RenderEnvironment::deterministic();
        let artifact = crate::family::prepare(
            parsed.clone(),
            &crate::LayoutOptions::default(),
            environment.begin_session().unwrap(),
        )
        .unwrap();
        let mut layout: FlowchartLayout = serde_json::from_value(
            artifact.layout_json().unwrap()["layout"]["FlowchartV2"].clone(),
        )
        .unwrap();
        // The public JSON projection omits this internal renderer ownership flag.
        layout.uses_elk_adapter_dom = true;
        let (metadata, semantic) = parsed.into_parts();
        let RenderSemanticModel::Flowchart(model) = semantic else {
            panic!("Flowchart")
        };
        for measured in fixture["nodes"].as_array().unwrap() {
            let id = measured["id"].as_str().unwrap();
            if let Some(center) = measured["center"].as_array() {
                for node in layout.nodes.iter_mut().filter(|n| n.id == id) {
                    node.x = center[0].as_f64().unwrap();
                    node.y = center[1].as_f64().unwrap();
                    if let Some(size) = measured["size"].as_array() {
                        node.width = size[0].as_f64().unwrap();
                        node.height = size[1].as_f64().unwrap();
                    }
                }
            }
        }
        for measured in fixture["edges"].as_array().unwrap() {
            let edge = layout
                .edges
                .iter_mut()
                .find(|e| e.id == measured["id"].as_str().unwrap())
                .unwrap();
            edge.points = measured["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| LayoutPoint {
                    x: p[0].as_f64().unwrap(),
                    y: p[1].as_f64().unwrap(),
                })
                .collect();
        }
        let request = SvgRenderOptions {
            diagram_id: Some(name.to_owned()),
            ..SvgRenderOptions::default()
        };
        let debug = SvgDebugOptions::default();
        let mut labels = std::collections::HashMap::new();
        let mut svg_markdown = std::collections::HashMap::new();
        for node in fixture["nodes"].as_array().unwrap() {
            let Some(text) = node["label_box"]["text"].as_str() else {
                continue;
            };
            let normalized = text
                .replace("<br>", "\n")
                .replace("<br/>", "\n")
                .replace("<br />", "\n");
            let metrics = TextMetrics {
                width: node["label_box"]["size"][0].as_f64().unwrap(),
                height: node["label_box"]["size"][1].as_f64().unwrap(),
                line_count: normalized.lines().count(),
            };
            if model.nodes.iter().any(|n| {
                n.id == node["id"].as_str().unwrap() && n.label_type.as_deref() == Some("markdown")
            }) {
                let key = crate::text::mermaid_markdown_to_lines(text, true)
                    .into_iter()
                    .flatten()
                    .map(|(word, _)| {
                        crate::entities::decode_svg_text_content_entities(&word).into_owned()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                svg_markdown.insert(key, metrics);
            }
            for key in [text.to_string(), normalized] {
                if let Some(previous) = labels.insert(key.clone(), metrics) {
                    assert_eq!(
                        (previous.width, previous.height, previous.line_count),
                        (metrics.width, metrics.height, metrics.line_count),
                        "{name}: ambiguous captured label {key}"
                    );
                }
            }
        }
        use crate::environment::{
            MeasurementProfileId, TextMeasurementPolicy, TextMeasurementProfile,
            TextMeasurementProfileIdentity,
        };
        let profile = TextMeasurementProfile::new(
            TextMeasurementProfileIdentity::new(
                MeasurementProfileId::new("test.flowchart-browser-label-boxes").unwrap(),
                "fixture",
            )
            .unwrap(),
            std::sync::Arc::new(BrowserLabelBoxes {
                raw: labels,
                svg_markdown,
            }),
        );
        let session = environment
            .with_text_measurement_policy(TextMeasurementPolicy::uniform(profile))
            .begin_session()
            .unwrap();
        let execution = SvgExecution::new(&request, &debug, &session).unwrap();
        let sidecar = crate::flowchart::FlowchartSvgLabelSidecar::default();
        let svg = render_flowchart_svg_model(
            FlowchartSvgModelRequest {
                layout: &layout,
                swimlane_layout: None,
                model: &model,
                render_context: &render_context,
                effective_config: &metadata.effective_config,
                diagram_type: metadata.diagram_type.as_str(),
                diagram_title: None,
                presentation_policy: None,
                svg_label_sidecar: &sidecar,
            },
            &execution,
            None,
        )
        .unwrap()
        .to_string();
        let reference = std::fs::read_to_string(
            root.join("fixtures/upstream-svgs/flowchart")
                .join(format!("{name}.svg")),
        )
        .unwrap();
        let actual = roxmltree::Document::parse(&svg).unwrap();
        let expected = roxmltree::Document::parse(&reference).unwrap();
        let identities = |document: &roxmltree::Document<'_>| {
            let mut ids = document
                .descendants()
                .filter_map(|n| {
                    n.attribute("id")
                        .map(|id| (n.tag_name().name().to_owned(), id.to_owned()))
                })
                .collect::<Vec<_>>();
            ids.sort();
            ids
        };
        assert_eq!(
            identities(&actual),
            identities(&expected),
            "{name}: DOM identities"
        );
        if name.ends_with("newshapesset6_lr_md_html_false_094") {
            // Joining outer lines restores only the whitespace introduced by wrapping.
            // Inner spans retain their literal text, including escaped <strong> markup.
            let labels = |document: &roxmltree::Document<'_>| {
                document
                    .descendants()
                    .filter(|n| n.has_tag_name("text"))
                    .map(|text| {
                        text.children()
                            .filter(|n| n.is_element())
                            .map(|line| {
                                line.descendants()
                                    .filter(|n| n.is_text())
                                    .filter_map(|n| n.text())
                                    .collect::<String>()
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect::<Vec<_>>()
            };
            let expected_labels = labels(&expected);
            assert_eq!(expected_labels.len(), 4, "{name}: SVG labels");
            assert_eq!(
                labels(&actual),
                expected_labels,
                "{name}: wrapped label content"
            );
        }
        for edge in expected
            .descendants()
            .filter(|n| n.attribute("data-edge") == Some("true"))
        {
            let id = edge.attribute("data-id").unwrap();
            let observed = actual
                .descendants()
                .find(|n| {
                    n.attribute("data-id") == Some(id) && n.attribute("data-edge") == Some("true")
                })
                .unwrap();
            for attribute in ["data-et", "data-look", "marker-start", "marker-end"] {
                assert_eq!(
                    observed.attribute(attribute),
                    edge.attribute(attribute),
                    "{name}/{id}: {attribute}"
                );
            }
            let commands = |n: roxmltree::Node<'_, '_>| {
                n.attribute("d")
                    .unwrap()
                    .chars()
                    .filter(char::is_ascii_uppercase)
                    .collect::<String>()
            };
            let coordinates = |n: roxmltree::Node<'_, '_>| {
                number
                    .find_iter(n.attribute("d").unwrap())
                    .map(|m| m.as_str().parse::<f64>().unwrap())
                    .collect::<Vec<_>>()
            };
            let observed_coordinates = coordinates(observed);
            let expected_coordinates = coordinates(edge);
            // The writer serializes coordinates to three decimal places.
            let same_coordinates = observed_coordinates.len() == expected_coordinates.len()
                && observed_coordinates
                    .iter()
                    .zip(&expected_coordinates)
                    .all(|(actual, expected)| (actual - expected).abs() <= 0.000_501);
            // RoughJS control points also depend on browser text/shape path geometry.
            // Keep hand-drawn command topology and marker identity checked, while the
            // companion provider test checks its numeric pre-paint routes separately.
            let check_coordinates = edge.attribute("data-look") != Some("handDrawn");
            if commands(observed) != commands(edge) || (check_coordinates && !same_coordinates) {
                mismatches.push(format!(
                    "{name}/{id}: expected={} actual={}\n expected_d={}\n actual_d={}",
                    commands(edge),
                    commands(observed),
                    edge.attribute("d").unwrap(),
                    observed.attribute("d").unwrap()
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
