//! Mermaid 12 Usecase measurements and shared graph-layout projection.

use crate::Result;
use crate::elk_edge_geometry;
use crate::model::{Bounds, LayoutEdge, LayoutNode};
use crate::resources::OperationWorkMeter;
use crate::text::{TextMeasurer, TextMetrics, TextStyle};
use merman_core::diagrams::usecase::{UsecaseDiagramRenderModel, UsecaseLabelType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

mod dagre;
mod layout;
mod measure;

pub(crate) use measure::styles as compiled_styles;
pub(crate) use measure::text_style;

/// Mermaid's nonMarkdownToHTML/nonMarkdownToLines treat a literal \n as a line break.
/// Keep the source label intact for accessibility while sharing its rendered text with measurement.
pub(crate) fn normalize_plain_label_line_breaks(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains("\\n") {
        std::borrow::Cow::Owned(text.replace("\\n", "\n"))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsecaseDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone)]
pub(crate) struct UsecaseLabelPlan {
    pub text: String,
    pub label_type: UsecaseLabelType,
    pub metrics: TextMetrics,
    pub style: TextStyle,
    pub max_width: Option<f64>,
    pub styles: indexmap::IndexMap<String, String>,
    pub math_html: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct UsecaseJsonRowPlan {
    pub key: UsecaseLabelPlan,
    pub value: UsecaseLabelPlan,
    pub accessible_key: String,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct UsecaseJsonTablePlan {
    pub rows: Vec<UsecaseJsonRowPlan>,
    pub key_width: f64,
    pub value_width: f64,
    pub title_height: f64,
    pub border_width: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct UsecaseNodePlan {
    pub id: String,
    pub parent: Option<String>,
    pub source_label: String,
    pub label: UsecaseLabelPlan,
    pub stereotype: Option<UsecaseLabelPlan>,
    pub folded_stereotype: bool,
    pub width: f64,
    pub height: f64,
    pub is_boundary: bool,
    pub package: bool,
    pub ellipse: bool,
    pub table: Option<UsecaseJsonTablePlan>,
    pub dagre_helper: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct UsecaseEdgePlan {
    pub id: String,
    pub original_id: Option<String>,
    pub self_loop_node: Option<String>,
    pub dagre_recursive: bool,
    pub source: String,
    pub target: String,
    pub label: Option<UsecaseLabelPlan>,
    pub minlen: usize,
    pub start_marker: Option<String>,
    pub end_marker: Option<String>,
    pub dotted: bool,
    pub internal: bool,
}

#[derive(Debug)]
pub(crate) struct UsecasePreparedArtifact {
    pub(crate) layout: UsecaseDiagramLayout,
    pub(crate) nodes: Vec<UsecaseNodePlan>,
    pub(crate) edges: Vec<UsecaseEdgePlan>,
    edge_paths: std::collections::HashMap<String, Vec<crate::model::LayoutPoint>>,
}

impl UsecasePreparedArtifact {
    pub(crate) fn layout(&self) -> &UsecaseDiagramLayout {
        &self.layout
    }

    pub(crate) fn edge_points(&self, edge: &LayoutEdge) -> Vec<crate::model::LayoutPoint> {
        self.edge_paths
            .get(&edge.id)
            .cloned()
            .unwrap_or_else(|| edge.points.clone())
    }
}

/// The literal source Mermaid passes to createText, before selecting its math branch.
pub(crate) fn create_text_source(text: &str, kind: UsecaseLabelType) -> std::borrow::Cow<'_, str> {
    match kind {
        UsecaseLabelType::Markdown => std::borrow::Cow::Borrowed(text),
        UsecaseLabelType::Text => std::borrow::Cow::Owned(
            normalize_plain_label_line_breaks(text)
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;"),
        ),
    }
}

pub(crate) fn requires_math(
    model: &UsecaseDiagramRenderModel,
    config: &merman_core::MermaidConfig,
) -> bool {
    if config.as_value().get("htmlLabels").and_then(Value::as_bool) == Some(false) {
        return false;
    }
    let has_math = |(text, kind): (&str, UsecaseLabelType)| {
        let sanitized = merman_core::sanitize::sanitize_text(text, config);
        crate::math::contains_delimited_math(&create_text_source(&sanitized, kind))
    };
    let mut labels = model
        .nodes
        .iter()
        .flat_map(|node| {
            std::iter::once((node.label.as_str(), node.label_type)).chain(
                node.stereotype
                    .as_deref()
                    .map(|text| (text, UsecaseLabelType::Text)),
            )
        })
        .chain(
            model
                .boundaries
                .iter()
                .map(|boundary| (boundary.label.as_str(), boundary.label_type)),
        )
        .chain(
            model
                .notes
                .iter()
                .map(|note| (note.label.as_str(), note.label_type)),
        )
        .chain(model.relationships.iter().filter_map(|edge| {
            edge.label
                .as_deref()
                .map(|text| (text, edge.label_type.unwrap_or_default()))
        }));
    if labels.any(has_math) {
        return true;
    }
    // JSON cells show the complete leaf path, not each object key in isolation.
    let mut pending: Vec<_> = model
        .json_nodes
        .iter()
        .map(|node| (node, &node.value, String::new()))
        .collect();
    while let Some((node, value, path)) = pending.pop() {
        let display_has_math = |text: &str| {
            node.display_string(text)
                .is_ok_and(|text| has_math((&text, UsecaseLabelType::Text)))
        };
        match value {
            Value::Array(values)
                if !values.is_empty()
                    && values
                        .iter()
                        .all(|value| !value.is_array() && !value.is_object()) =>
            {
                if has_math((&path, UsecaseLabelType::Text))
                    || values
                        .iter()
                        .filter_map(Value::as_str)
                        .any(display_has_math)
                {
                    return true;
                }
            }
            Value::Array(values) if !values.is_empty() => {
                pending.extend(
                    values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| (node, value, format!("{path}[{index}]"))),
                );
            }
            Value::Object(values) if !values.is_empty() => {
                pending.extend(values.iter().filter_map(|(key, value)| {
                    let key = node.display_string(key).ok()?;
                    let path = if path.is_empty() {
                        key.into_owned()
                    } else {
                        format!("{path}.{key}")
                    };
                    Some((node, value, path))
                }));
            }
            _ => {
                if has_math((&path, UsecaseLabelType::Text))
                    || value.as_str().is_some_and(display_has_math)
                {
                    return true;
                }
            }
        }
    }
    false
}

pub(crate) fn prepare_usecase_diagram(
    model: &UsecaseDiagramRenderModel,
    effective_config: &Value,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn crate::math::MathRenderer + Send + Sync)>,
    work_meter: Arc<OperationWorkMeter>,
    #[cfg(feature = "layout-elk")] operation_seed: merman_layout_elk::ElkOperationSeed,
) -> Result<UsecasePreparedArtifact> {
    let mut work = crate::layout_work::OperationLayoutWorkControl::new(work_meter);
    let (mut nodes, mut edges) =
        measure::measure(model, effective_config, measurer, math_renderer, &mut work)?;
    if crate::layout_backend::resolve_graph_layout(effective_config).backend
        == crate::layout_backend::GraphLayoutBackend::Dagre
    {
        dagre::expand_self_loops(&mut nodes, &mut edges, effective_config, &mut work)?;
    }
    let mut layout = layout::layout(
        model,
        effective_config,
        &nodes,
        &mut edges,
        &mut work,
        #[cfg(feature = "layout-elk")]
        operation_seed,
    )?;
    let edge_paths =
        layout::prepare_edge_paths(&mut layout, &nodes, &edges, effective_config, &mut work)?;
    Ok(UsecasePreparedArtifact {
        layout,
        nodes,
        edges,
        edge_paths,
    })
}
