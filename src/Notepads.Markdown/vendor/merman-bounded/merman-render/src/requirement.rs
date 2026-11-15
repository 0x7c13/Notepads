use crate::environment::{BuiltinTextMeasurementOperationCarrier, TextMeasurementOperation};
use crate::layout_work::OperationLayoutWorkControl;
use crate::model::{
    Bounds, LayoutEdge, LayoutLabel, LayoutNode, LayoutPoint, RequirementDiagramLayout,
};
#[cfg(test)]
use crate::resources::RenderResourcePolicy;
use crate::resources::{ModelComplexity, OperationWorkMeter};
use crate::text::{TextMeasurer, TextStyle, WrapMode};
use crate::{Error, Result};
use dugong::graphlib::{EdgeKey, Graph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, LabelPos, NodeLabel, RankDir};
use merman_core::diagrams::requirement::{
    RequirementDiagramRenderModel, RequirementRenderElement, RequirementRenderNode,
};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

mod config;
#[cfg(feature = "layout-elk")]
mod elk;

pub(crate) use config::RequirementConfigView;

fn requirement_layout_work_units(model: &RequirementDiagramRenderModel) -> usize {
    let source_node_count = model
        .requirements
        .len()
        .saturating_add(model.elements.len());
    let source_edge_count = model.relationships.len();
    let self_loop_count = model
        .relationships
        .iter()
        .filter(|relationship| relationship.src == relationship.dst)
        .count();
    // Mermaid expands every self-loop into two labelRect nodes and three layout edges.
    let node_count = source_node_count.saturating_add(self_loop_count.saturating_mul(2));
    let edge_count = source_edge_count.saturating_add(self_loop_count.saturating_mul(2));
    // Dugong's ranking, crossing, and routing phases repeatedly inspect graph incidence. Charge a
    // deterministic V*E upper bound before constructing the graph rather than exposing a
    // Requirement-specific public threshold.
    node_count
        .saturating_add(edge_count)
        .saturating_add(node_count.saturating_mul(edge_count))
}

fn normalize_dir(direction: &str) -> String {
    match direction.trim().to_uppercase().as_str() {
        "TB" | "TD" => "TB".to_string(),
        "BT" => "BT".to_string(),
        "LR" => "LR".to_string(),
        "RL" => "RL".to_string(),
        other => other.to_string(),
    }
}

fn rank_dir_from(direction: &str) -> RankDir {
    match normalize_dir(direction).as_str() {
        "TB" => RankDir::TB,
        "BT" => RankDir::BT,
        "LR" => RankDir::LR,
        "RL" => RankDir::RL,
        _ => RankDir::TB,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RequirementLabelMetrics {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) max_width_px: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequirementLabelMeasurementBinding {
    font_family: String,
    font_size_bits: u64,
    calculation_font_family: String,
    calculation_font_size_bits: u64,
    html_labels: bool,
    edge_html_labels: bool,
    dimensions_carrier: BuiltinTextMeasurementOperationCarrier,
    wrapped_carrier: BuiltinTextMeasurementOperationCarrier,
}

impl RequirementLabelMeasurementBinding {
    fn for_measurer(
        settings: &config::RequirementLayoutSettings,
        measurer: &dyn TextMeasurer,
    ) -> Option<Self> {
        // Only the operation-owned built-in route can prove that suppressing the SVG-stage
        // request is unobservable. Host and custom measurers deliberately return no carrier.
        Some(Self {
            font_family: settings.font_family.clone(),
            font_size_bits: settings.font_size.to_bits(),
            calculation_font_family: settings.calculation_font_family.clone(),
            calculation_font_size_bits: settings.calculation_font_size.to_bits(),
            html_labels: settings.html_labels,
            edge_html_labels: settings.edge_html_labels,
            dimensions_carrier: measurer.builtin_operation_carrier(
                TextMeasurementOperation::MermaidCalculateTextDimensions,
            )?,
            wrapped_carrier: measurer
                .builtin_operation_carrier(TextMeasurementOperation::Wrapped)?,
        })
    }

    fn matches(
        &self,
        settings: &config::RequirementLayoutSettings,
        measurer: &dyn TextMeasurer,
    ) -> bool {
        self.font_family == settings.font_family
            && self.font_size_bits == settings.font_size.to_bits()
            && self.calculation_font_family == settings.calculation_font_family
            && self.calculation_font_size_bits == settings.calculation_font_size.to_bits()
            && self.html_labels == settings.html_labels
            && self.edge_html_labels == settings.edge_html_labels
            && measurer
                .builtin_operation_carrier(TextMeasurementOperation::MermaidCalculateTextDimensions)
                == Some(self.dimensions_carrier)
            && measurer.builtin_operation_carrier(TextMeasurementOperation::Wrapped)
                == Some(self.wrapped_carrier)
    }
}

#[derive(Debug)]
pub(crate) struct RequirementPreparedArtifact {
    layout: RequirementDiagramLayout,
    render_layout: Option<RequirementDiagramLayout>,
    nodes: HashMap<String, RequirementNodeRenderPlan>,
    edges: HashMap<EdgeKey, RequirementEdgeLabelPlan>,
    measurement_binding: Option<RequirementLabelMeasurementBinding>,
}

impl RequirementPreparedArtifact {
    pub(crate) fn layout(&self) -> &RequirementDiagramLayout {
        &self.layout
    }

    pub(crate) fn uses_elk(&self) -> bool {
        self.render_layout.is_some()
    }

    pub(crate) fn render_parts(
        &self,
    ) -> (
        &RequirementDiagramLayout,
        &HashMap<String, RequirementNodeRenderPlan>,
        &HashMap<EdgeKey, RequirementEdgeLabelPlan>,
    ) {
        (
            self.render_layout.as_ref().unwrap_or(&self.layout),
            &self.nodes,
            &self.edges,
        )
    }

    pub(crate) fn label_measurements_for_render<'a>(
        &self,
        effective_config: &Value,
        measurer: &'a dyn TextMeasurer,
    ) -> RequirementRenderLabelMeasurements<'a> {
        let settings = RequirementConfigView::new(effective_config).layout_settings();
        let reuse_prepared = self
            .measurement_binding
            .as_ref()
            .is_some_and(|binding| binding.matches(&settings, measurer));

        RequirementRenderLabelMeasurements {
            measurer,
            styles: (!reuse_prepared).then(|| requirement_measurement_styles(&settings)),
            reuse_prepared,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum RequirementNodeRenderPlan {
    Semantic(RequirementNodeLabelPlan),
    EdgeLabelAnchor,
}

#[derive(Debug, Clone)]
pub(crate) struct RequirementNodeLabelPlan {
    pub(crate) lines: Vec<RequirementNodeLabelLine>,
    pub(crate) divider_y_offset: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct RequirementNodeLabelLine {
    pub(crate) display_text: String,
    pub(crate) metrics: RequirementLabelMetrics,
    pub(crate) y_offset: f64,
    pub(crate) source_index: usize,
    pub(crate) measurement_bold: bool,
    pub(crate) bold: bool,
    pub(crate) keep_centered: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RequirementEdgeLabelPlan {
    pub(crate) relationship_type: String,
    pub(crate) display_text: String,
    pub(crate) rendered_id: String,
    pub(crate) has_label: bool,
    pub(crate) marker_start: bool,
    pub(crate) marker_end: bool,
}

pub(crate) struct RequirementRenderLabelMeasurements<'a> {
    measurer: &'a dyn TextMeasurer,
    styles: Option<RequirementMeasurementStyles>,
    reuse_prepared: bool,
}

impl RequirementRenderLabelMeasurements<'_> {
    fn resolve(
        &self,
        display_text: &str,
        measurement_bold: bool,
        prepared: RequirementLabelMetrics,
    ) -> Option<RequirementLabelMetrics> {
        if self.reuse_prepared {
            return Some(prepared);
        }
        let styles = self
            .styles
            .as_ref()
            .expect("opaque Requirement measurements retain their styles");

        // Mermaid 11.16's `requirementBox.addText` performs `calculateTextWidth`/`createText`
        // during SVG emission. Replaying that request keeps host callback order and provenance
        // observable whenever the private built-in binding cannot prove safe reuse.
        measure_requirement_label_metrics(
            self.measurer,
            styles,
            display_text,
            display_text,
            measurement_bold,
            true,
        )
    }

    pub(crate) fn node_plan_for_render<'a>(
        &self,
        prepared: &'a RequirementNodeLabelPlan,
    ) -> Option<Cow<'a, RequirementNodeLabelPlan>> {
        if self.reuse_prepared {
            return Some(Cow::Borrowed(prepared));
        }

        let lines = prepared
            .lines
            .iter()
            .map(|line| {
                let metrics =
                    self.resolve(&line.display_text, line.measurement_bold, line.metrics)?;
                Some((
                    line.source_index,
                    RequirementLabelSpec {
                        display_text: line.display_text.clone(),
                        measurement_bold: line.measurement_bold,
                        render_bold: line.bold,
                        keep_centered: line.keep_centered,
                    },
                    metrics,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let (_, _, plan) =
            requirement_box_layout_from_measured_lines(lines, 20.0, 20.0).into_node_plan();
        Some(Cow::Owned(plan))
    }

    pub(crate) fn measure_edge_label_for_render(
        &self,
        edge: &RequirementEdgeLabelPlan,
    ) -> Option<()> {
        if self.reuse_prepared {
            return Some(());
        }
        let styles = self
            .styles
            .as_ref()
            .expect("opaque Requirement measurements retain their styles");

        measure_requirement_label_metrics(
            self.measurer,
            styles,
            &edge.display_text,
            &edge.display_text,
            false,
            false,
        )
        .map(drop)
    }
}

#[derive(Debug)]
struct RequirementLabelSpec {
    display_text: String,
    measurement_bold: bool,
    render_bold: bool,
    keep_centered: bool,
}

pub(crate) fn requirement_styles_force_bold(css_styles: &[String]) -> bool {
    css_styles.iter().any(|raw| {
        let s = raw.trim().trim_end_matches(';');
        let Some((key, value)) = s.split_once(':') else {
            return false;
        };
        if !key.trim().eq_ignore_ascii_case("font-weight") {
            return false;
        }
        let value = value
            .split_once("!important")
            .map(|(v, _)| v)
            .unwrap_or(value)
            .trim()
            .to_ascii_lowercase();
        value == "bold"
            || value == "bolder"
            || value
                .parse::<u16>()
                .map(|weight| weight >= 600)
                .unwrap_or(false)
    })
}

pub(crate) fn calculate_text_width_like_mermaid_px(
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    text: &str,
) -> i64 {
    crate::text::measure_mermaid_text_dimensions(measurer, text, style).width
}

fn measure_requirement_label_metrics(
    measurer: &dyn TextMeasurer,
    styles: &RequirementMeasurementStyles,
    display_text: &str,
    calculation_text: &str,
    bold: bool,
    is_node: bool,
) -> Option<RequirementLabelMetrics> {
    if display_text.trim().is_empty() {
        return None;
    }

    let text_style = if bold {
        &styles.html_bold
    } else {
        &styles.html_regular
    };
    let html_labels = if is_node {
        styles.html_labels
    } else {
        styles.edge_html_labels
    };
    let max_width_px = if is_node {
        (calculate_text_width_like_mermaid_px(measurer, &styles.calculation, calculation_text) + 50)
            .max(0)
    } else {
        200
    };
    let max_width = (max_width_px > 0).then_some(max_width_px as f64);
    let measured = if html_labels {
        crate::text::measure_markdown_with_inline_styles(
            measurer,
            calculation_text,
            text_style,
            max_width,
            WrapMode::HtmlLike,
        )
    } else {
        crate::text::measure_wrapped_markdown_with_inline_styles(
            measurer,
            calculation_text,
            text_style,
            max_width,
            WrapMode::SvgLike,
        )
    };
    // requirementBox.addText adds six pixels to SVG node-label bounds after getBBox.
    // Relationship labels use the shared edge label path and do not receive this padding.
    let extra_height = if html_labels {
        0.0
    } else if is_node {
        6.0
    } else {
        4.0
    };
    let height = (measured.height + extra_height).max(1.0);
    let width = (measured.width + if !is_node && !html_labels { 4.0 } else { 0.0 }).max(1.0);

    Some(RequirementLabelMetrics {
        width,
        height,
        max_width_px,
    })
}

#[derive(Debug, Clone)]
struct RequirementBoxLayout {
    width: f64,
    height: f64,
    lines: Vec<RequirementNodeLabelLine>,
    divider_y_offset: Option<f64>,
}

impl RequirementBoxLayout {
    fn into_node_plan(self) -> (f64, f64, RequirementNodeLabelPlan) {
        (
            self.width,
            self.height,
            RequirementNodeLabelPlan {
                lines: self.lines,
                divider_y_offset: self.divider_y_offset,
            },
        )
    }
}

fn requirement_box_layout(
    measurer: &dyn TextMeasurer,
    styles: &RequirementMeasurementStyles,
    lines: Vec<RequirementLabelSpec>,
    gap: f64,
    padding: f64,
) -> RequirementBoxLayout {
    // Mirrors Mermaid `requirementBox.ts` label stacking and bbox-based sizing.
    let measured_lines = lines
        .into_iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let metrics = measure_requirement_label_metrics(
                measurer,
                styles,
                &line.display_text,
                &line.display_text,
                line.measurement_bold,
                true,
            )?;
            Some((idx, line, metrics))
        })
        .collect();
    requirement_box_layout_from_measured_lines(measured_lines, gap, padding)
}

fn requirement_box_layout_from_measured_lines(
    lines: Vec<(usize, RequirementLabelSpec, RequirementLabelMetrics)>,
    gap: f64,
    padding: f64,
) -> RequirementBoxLayout {
    let mut max_w: f64 = 0.0;
    let mut min_y = 0.0;
    let mut max_y = 0.0;
    let mut y_offset = 0.0;
    let mut prepared_lines = Vec::with_capacity(lines.len());
    let mut type_height = 0.0;
    let mut name_height = 0.0;
    let mut has_body = false;

    for (idx, line, metrics) in lines {
        max_w = max_w.max(metrics.width);

        if idx == 0 {
            min_y = -metrics.height / 2.0;
            max_y = metrics.height / 2.0;
            type_height = metrics.height;
            y_offset = metrics.height;
        } else if idx == 1 {
            let top = -metrics.height / 2.0 + y_offset;
            let bottom = metrics.height / 2.0 + y_offset;
            min_y = min_y.min(top);
            max_y = max_y.max(bottom);
            name_height = metrics.height;
            y_offset += metrics.height + gap;
        } else {
            let top = -metrics.height / 2.0 + y_offset;
            let bottom = metrics.height / 2.0 + y_offset;
            min_y = min_y.min(top);
            max_y = max_y.max(bottom);
            y_offset += metrics.height;
            has_body = true;
        }

        let line_y_offset = match idx {
            0 => 0.0,
            1 => type_height,
            _ => y_offset - metrics.height,
        };
        prepared_lines.push(RequirementNodeLabelLine {
            display_text: line.display_text,
            metrics,
            y_offset: line_y_offset,
            source_index: idx,
            measurement_bold: line.measurement_bold,
            bold: line.render_bold,
            keep_centered: line.keep_centered,
        });
    }

    let bbox_h = (max_y - min_y).max(1.0);

    RequirementBoxLayout {
        width: (max_w + padding).max(1.0),
        height: (bbox_h + padding).max(1.0),
        lines: prepared_lines,
        divider_y_offset: has_body.then_some(type_height + name_height + gap),
    }
}

fn requirement_edge_id(src: &str, dst: &str, idx: usize) -> String {
    format!("{src}-{dst}-{idx}")
}

fn requirement_edge_key(src: &str, dst: &str, index: usize) -> EdgeKey {
    EdgeKey::new(src, dst, Some(requirement_edge_id(src, dst, index)))
}

fn requirement_layout_edge_label(width: f64, height: f64) -> EdgeLabel {
    EdgeLabel {
        width,
        height,
        labelpos: LabelPos::C,
        // Dagre defaults to 10 when unspecified.
        labeloffset: 10.0,
        minlen: 1,
        weight: 1.0,
        ..Default::default()
    }
}

fn insert_requirement_layout_edge(
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    plans: &mut HashMap<EdgeKey, RequirementEdgeLabelPlan>,
    from: &str,
    to: &str,
    graph_name: &str,
    label: EdgeLabel,
    plan: RequirementEdgeLabelPlan,
) {
    graph.set_edge_named(from, to, Some(graph_name), Some(label));
    plans.insert(EdgeKey::new(from, to, Some(graph_name)), plan);
}

fn prefixed_nonempty_line(prefix: &str, value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        String::new()
    } else {
        format!("{prefix}{value}")
    }
}

fn node_label_spec(
    display_text: String,
    measurement_bold: bool,
    render_bold: bool,
    keep_centered: bool,
) -> RequirementLabelSpec {
    RequirementLabelSpec {
        display_text,
        measurement_bold,
        render_bold,
        keep_centered,
    }
}

fn requirement_node_label_specs(node: &RequirementRenderNode) -> Vec<RequirementLabelSpec> {
    let style_bold = requirement_styles_force_bold(&node.css_styles);
    vec![
        node_label_spec(
            format!("&lt;&lt;{}&gt;&gt;", node.node_type),
            style_bold,
            false,
            true,
        ),
        node_label_spec(node.name.clone(), true, true, true),
        node_label_spec(
            prefixed_nonempty_line("ID: ", &node.requirement_id),
            style_bold,
            false,
            false,
        ),
        node_label_spec(
            prefixed_nonempty_line("Text: ", &node.text),
            style_bold,
            false,
            false,
        ),
        node_label_spec(
            prefixed_nonempty_line("Risk: ", &node.risk),
            style_bold,
            false,
            false,
        ),
        node_label_spec(
            prefixed_nonempty_line("Verification: ", &node.verify_method),
            style_bold,
            false,
            false,
        ),
    ]
}

fn element_node_label_specs(node: &RequirementRenderElement) -> Vec<RequirementLabelSpec> {
    let style_bold = requirement_styles_force_bold(&node.css_styles);
    vec![
        node_label_spec(
            "&lt;&lt;Element&gt;&gt;".to_string(),
            style_bold,
            false,
            true,
        ),
        node_label_spec(node.name.clone(), true, true, true),
        node_label_spec(
            prefixed_nonempty_line("Type: ", &node.element_type),
            style_bold,
            false,
            false,
        ),
        node_label_spec(
            prefixed_nonempty_line("Doc Ref: ", &node.doc_ref),
            style_bold,
            false,
            false,
        ),
    ]
}

struct RequirementMeasurementStyles {
    html_regular: TextStyle,
    html_bold: TextStyle,
    calculation: TextStyle,
    html_labels: bool,
    edge_html_labels: bool,
}

fn requirement_measurement_styles(
    settings: &config::RequirementLayoutSettings,
) -> RequirementMeasurementStyles {
    let font_family = Some(settings.font_family.clone());
    RequirementMeasurementStyles {
        html_labels: settings.html_labels,
        edge_html_labels: settings.edge_html_labels,
        html_regular: TextStyle {
            font_family: font_family.clone(),
            font_size: settings.font_size,
            font_weight: None,
            font_style: None,
        },
        html_bold: TextStyle {
            font_family,
            font_size: settings.font_size,
            font_weight: Some("bold".to_string()),
            font_style: None,
        },
        calculation: TextStyle {
            font_family: Some(settings.calculation_font_family.clone()),
            font_size: settings.calculation_font_size,
            font_weight: None,
            font_style: None,
        },
    }
}

/// Lays out a Requirement model under the resource policy owned by the render operation.
#[cfg(test)]
pub(crate) fn layout_requirement_diagram_typed_with_resource_policy(
    model: &RequirementDiagramRenderModel,
    effective_config: &Value,
    text_measurer: &dyn TextMeasurer,
    resource_limits: RenderResourcePolicy,
) -> Result<RequirementPreparedArtifact> {
    let work_meter = Arc::new(OperationWorkMeter::new(resource_limits));
    layout_requirement_diagram_typed_with_work_meter(
        model,
        effective_config,
        text_measurer,
        work_meter,
        #[cfg(feature = "layout-elk")]
        merman_layout_elk::ElkOperationSeed::from_operation_seed(std::num::NonZeroU64::MIN),
    )
}

/// Lays out a Requirement model under the cumulative work meter owned by the render operation.
pub(crate) fn layout_requirement_diagram_typed_with_work_meter(
    model: &RequirementDiagramRenderModel,
    effective_config: &Value,
    text_measurer: &dyn TextMeasurer,
    work_meter: Arc<OperationWorkMeter>,
    #[cfg(feature = "layout-elk")] operation_seed: merman_layout_elk::ElkOperationSeed,
) -> Result<RequirementPreparedArtifact> {
    work_meter
        .policy()
        .check_model_complexity(ModelComplexity::from_requirement(model))?;
    work_meter.charge(requirement_layout_work_units(model))?;
    let mut work_control = OperationLayoutWorkControl::new(work_meter);
    let selection = crate::layout_backend::resolve_graph_layout(effective_config);
    selection.validate_rootless_graph()?;
    let backend = selection.backend;
    let direction = if model.direction.trim().is_empty() {
        normalize_dir("TB")
    } else {
        normalize_dir(&model.direction)
    };

    let cfg = RequirementConfigView::new(effective_config).layout_settings();
    let measurement_binding = RequirementLabelMeasurementBinding::for_measurer(&cfg, text_measurer);
    let styles = requirement_measurement_styles(&cfg);

    let padding = 20.0;
    let gap = 20.0;
    let mut requirement_node_labels = HashMap::new();
    let mut element_node_labels = HashMap::new();
    let mut edge_labels = HashMap::new();
    let mut edge_label_anchor_nodes = HashSet::new();

    let mut g = Graph::<NodeLabel, EdgeLabel, GraphLabel>::new(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    g.set_graph(GraphLabel {
        rankdir: rank_dir_from(&direction),
        nodesep: cfg.nodesep,
        ranksep: cfg.ranksep,
        marginx: 8.0,
        marginy: 8.0,
        ..Default::default()
    });

    for r in &model.requirements {
        if r.name.trim().is_empty() {
            return Err(Error::InvalidModel {
                message: format!("missing requirement name label for {}", r.name),
            });
        }

        let box_layout = requirement_box_layout(
            text_measurer,
            &styles,
            requirement_node_label_specs(r),
            gap,
            padding,
        );
        let (width, height, label_plan) = box_layout.into_node_plan();
        g.set_node(
            r.name.clone(),
            NodeLabel {
                width,
                height,
                ..Default::default()
            },
        );
        requirement_node_labels.insert(r.name.clone(), label_plan);
    }

    for e in &model.elements {
        if e.name.trim().is_empty() {
            return Err(Error::InvalidModel {
                message: format!("missing element name label for {}", e.name),
            });
        }

        let box_layout = requirement_box_layout(
            text_measurer,
            &styles,
            element_node_label_specs(e),
            gap,
            padding,
        );
        let (width, height, label_plan) = box_layout.into_node_plan();
        g.set_node(
            e.name.clone(),
            NodeLabel {
                width,
                height,
                ..Default::default()
            },
        );
        element_node_labels.insert(e.name.clone(), label_plan);
    }

    // Mermaid assigns one monotonically increasing edge counter across the complete
    // relationship list, including parallel relationships and mixed relationship kinds.
    for (relationship_index, rel) in model.relationships.iter().enumerate() {
        if !g.has_node(&rel.src) {
            return Err(Error::InvalidModel {
                message: format!("relationship src node not found: {}", rel.src),
            });
        }
        if !g.has_node(&rel.dst) {
            return Err(Error::InvalidModel {
                message: format!("relationship dst node not found: {}", rel.dst),
            });
        }

        let label_display = format!("&lt;&lt;{}&gt;&gt;", rel.rel_type);
        let label_calculation = label_display.clone();
        let metrics = measure_requirement_label_metrics(
            text_measurer,
            &styles,
            &label_display,
            &label_calculation,
            false,
            false,
        )
        .ok_or_else(|| Error::InvalidModel {
            message: format!(
                "missing relationship label for {} -> {} ({})",
                rel.src, rel.dst, rel.rel_type
            ),
        })?;

        let is_contains = rel.rel_type == "contains";
        if rel.src == rel.dst && backend == crate::layout_backend::GraphLayoutBackend::Dagre {
            // The pinned Dagre renderer replaces a self-loop with two measured labelRect nodes and
            // three named edges. Requirement intentionally keeps those segments separate in SVG.
            let first_anchor = format!("{}---{}---1", rel.src, rel.src);
            let second_anchor = format!("{}---{}---2", rel.src, rel.src);
            for anchor in [&first_anchor, &second_anchor] {
                g.set_node(
                    anchor.clone(),
                    NodeLabel {
                        width: 0.1,
                        height: 0.1,
                        ..Default::default()
                    },
                );
                edge_label_anchor_nodes.insert(anchor.clone());
            }

            let first_graph_name = format!("{}-cyclic-special-0", rel.src);
            let middle_graph_name = format!("{}-cyclic-special-1", rel.src);
            let last_graph_name = format!("{}-cyclic-special-2", rel.src);
            insert_requirement_layout_edge(
                &mut g,
                &mut edge_labels,
                &rel.src,
                &first_anchor,
                &first_graph_name,
                requirement_layout_edge_label(0.0, 0.0),
                RequirementEdgeLabelPlan {
                    relationship_type: rel.rel_type.clone(),
                    display_text: String::new(),
                    rendered_id: format!("{}-cyclic-special-1", rel.src),
                    has_label: false,
                    marker_start: is_contains,
                    marker_end: false,
                },
            );
            insert_requirement_layout_edge(
                &mut g,
                &mut edge_labels,
                &first_anchor,
                &second_anchor,
                &middle_graph_name,
                requirement_layout_edge_label(metrics.width.max(0.0), metrics.height.max(0.0)),
                RequirementEdgeLabelPlan {
                    relationship_type: rel.rel_type.clone(),
                    display_text: label_display,
                    rendered_id: format!("{}-cyclic-special-mid", rel.src),
                    has_label: true,
                    marker_start: false,
                    marker_end: false,
                },
            );
            insert_requirement_layout_edge(
                &mut g,
                &mut edge_labels,
                &second_anchor,
                &rel.dst,
                &last_graph_name,
                requirement_layout_edge_label(0.0, 0.0),
                RequirementEdgeLabelPlan {
                    relationship_type: rel.rel_type.clone(),
                    display_text: String::new(),
                    rendered_id: last_graph_name.clone(),
                    has_label: false,
                    marker_start: false,
                    marker_end: !is_contains,
                },
            );
        } else {
            let edge_key = requirement_edge_key(&rel.src, &rel.dst, relationship_index);
            let rendered_id = edge_key
                .name
                .clone()
                .expect("Requirement edges are always named");
            insert_requirement_layout_edge(
                &mut g,
                &mut edge_labels,
                &edge_key.v,
                &edge_key.w,
                &rendered_id,
                requirement_layout_edge_label(metrics.width.max(0.0), metrics.height.max(0.0)),
                RequirementEdgeLabelPlan {
                    relationship_type: rel.rel_type.clone(),
                    display_text: label_display,
                    rendered_id: rendered_id.clone(),
                    has_label: true,
                    marker_start: is_contains,
                    marker_end: !is_contains,
                },
            );
        }
    }

    match backend {
        #[cfg(feature = "layout-elk")]
        crate::layout_backend::GraphLayoutBackend::Elk => {
            elk::layout(&mut g, effective_config, operation_seed, &mut work_control)?;
        }
        _ => dugong::layout_controlled(&mut g, &mut work_control)
            .map_err(|error| work_control.map_dugong_error(error))?,
    }

    let mut out_nodes: Vec<LayoutNode> = Vec::new();
    for v in g.nodes() {
        let Some(n) = g.node(v) else {
            continue;
        };
        let (Some(cx), Some(cy)) = (n.x, n.y) else {
            continue;
        };
        out_nodes.push(LayoutNode {
            id: v.to_string(),
            x: cx - n.width / 2.0,
            y: cy - n.height / 2.0,
            width: n.width,
            height: n.height,
            is_cluster: false,
            label_width: None,
            label_height: None,
        });
    }

    let mut out_edges: Vec<LayoutEdge> = Vec::new();
    let mut prepared_edge_labels = HashMap::new();
    for ek in g.edge_keys() {
        let Some(e) = g.edge_by_key(&ek) else {
            continue;
        };
        let prepared_label = edge_labels.remove(&ek).ok_or_else(|| Error::InvalidModel {
            message: format!(
                "missing prepared Requirement edge label for {} -> {} ({:?})",
                ek.v, ek.w, ek.name
            ),
        })?;

        let points = e
            .points
            .iter()
            .map(|p| LayoutPoint { x: p.x, y: p.y })
            .collect::<Vec<_>>();

        let label = match (e.x, e.y) {
            (Some(x), Some(y)) if e.width > 0.0 && e.height > 0.0 => Some(LayoutLabel {
                x,
                y,
                width: e.width,
                height: e.height,
            }),
            _ => None,
        };

        let layout_edge_id = ek
            .name
            .clone()
            .unwrap_or_else(|| format!("{}-{}", ek.v, ek.w));
        out_edges.push(LayoutEdge {
            id: layout_edge_id,
            from: ek.v.clone(),
            to: ek.w.clone(),
            from_cluster: None,
            to_cluster: None,
            points,
            label,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        });
        if prepared_edge_labels.insert(ek, prepared_label).is_some() {
            return Err(Error::InvalidModel {
                message: "duplicate prepared Requirement edge identity after layout".to_string(),
            });
        }
    }

    fn bounds_for_nodes_edges(nodes: &[LayoutNode], edges: &[LayoutEdge]) -> Option<Bounds> {
        if nodes.is_empty() && edges.is_empty() {
            return None;
        }
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for n in nodes {
            min_x = min_x.min(n.x);
            min_y = min_y.min(n.y);
            max_x = max_x.max(n.x + n.width);
            max_y = max_y.max(n.y + n.height);
        }
        for e in edges {
            for p in &e.points {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
            if let Some(l) = &e.label {
                min_x = min_x.min(l.x - l.width / 2.0);
                max_x = max_x.max(l.x + l.width / 2.0);
                min_y = min_y.min(l.y - l.height / 2.0);
                max_y = max_y.max(l.y + l.height / 2.0);
            }
        }

        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            return None;
        }
        Some(Bounds {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    let bounds = bounds_for_nodes_edges(&out_nodes, &out_edges);
    let mut prepared_node_labels = HashMap::with_capacity(out_nodes.len());
    for node in &out_nodes {
        let plan = if edge_label_anchor_nodes.contains(&node.id) {
            RequirementNodeRenderPlan::EdgeLabelAnchor
        } else {
            // Preserve the renderer's historical requirement-before-element precedence when both
            // semantic collections contain the same node name.
            let prepared_label = requirement_node_labels
                .remove(&node.id)
                .or_else(|| element_node_labels.remove(&node.id))
                .ok_or_else(|| Error::InvalidModel {
                    message: format!("missing prepared Requirement node label for {}", node.id),
                })?;
            RequirementNodeRenderPlan::Semantic(prepared_label)
        };
        if prepared_node_labels.insert(node.id.clone(), plan).is_some() {
            return Err(Error::InvalidModel {
                message: format!("duplicate prepared Requirement node identity: {}", node.id),
            });
        }
    }
    let layout = RequirementDiagramLayout {
        nodes: out_nodes,
        edges: out_edges,
        bounds,
    };
    #[cfg(feature = "layout-elk")]
    let render_layout = if backend == crate::layout_backend::GraphLayoutBackend::Elk {
        Some(elk::render_layout(
            &layout,
            &g,
            effective_config,
            &mut work_control,
        )?)
    } else {
        None
    };
    #[cfg(not(feature = "layout-elk"))]
    let render_layout = None;
    Ok(RequirementPreparedArtifact {
        layout,
        render_layout,
        nodes: prepared_node_labels,
        edges: prepared_edge_labels,
        measurement_binding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{DeterministicTextMeasurer, TextMetrics};
    use std::cell::Cell;

    struct FamilySelectionMeasurer;

    impl TextMeasurer for FamilySelectionMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: 0.0,
                height: 10.0,
                line_count: 1,
            }
        }

        fn measure_svg_simple_text_bbox_width_px(&self, _text: &str, style: &TextStyle) -> f64 {
            if style.font_family.as_deref() == Some("sans-serif") {
                200.0
            } else {
                100.0
            }
        }

        fn measure_svg_simple_text_bbox_height_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            10.0
        }
    }

    #[derive(Default)]
    struct PhaseHeightMeasurer {
        inner: DeterministicTextMeasurer,
        render_phase: Cell<bool>,
    }

    impl TextMeasurer for PhaseHeightMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            self.inner.measure(text, style)
        }

        fn measure_mermaid_calculate_text_dimensions(
            &self,
            text: &str,
            style: &TextStyle,
        ) -> TextMetrics {
            self.inner
                .measure_mermaid_calculate_text_dimensions(text, style)
        }

        fn measure_wrapped(
            &self,
            text: &str,
            style: &TextStyle,
            max_width: Option<f64>,
            wrap_mode: WrapMode,
        ) -> TextMetrics {
            let mut metrics = self
                .inner
                .measure_wrapped(text, style, max_width, wrap_mode);
            if self.render_phase.get() {
                metrics.height += 12.0;
            }
            metrics
        }
    }

    #[test]
    fn requirement_layout_work_counts_prototype_named_requirements_and_elements() {
        for (requirement_name, element_name) in
            [("__proto__", "constructor"), ("constructor", "__proto__")]
        {
            let mut model: RequirementDiagramRenderModel =
                serde_json::from_value(serde_json::json!({
                    "requirements": [{ "name": requirement_name, "type": "Requirement" }],
                    "elements": [{ "name": element_name, "type": "System" }],
                }))
                .expect("valid Requirement model");
            assert_eq!(requirement_layout_work_units(&model), 2);
            model.relationships.push(
                merman_core::diagrams::requirement::RequirementRenderRelationship {
                    rel_type: "satisfies".to_string(),
                    src: element_name.to_string(),
                    dst: requirement_name.to_string(),
                },
            );
            assert_eq!(requirement_layout_work_units(&model), 5);
        }
    }

    #[test]
    fn requirement_styles_detect_bold_font_weight() {
        assert!(super::requirement_styles_force_bold(&[
            "fill:#f9f".to_string(),
            " font-weight:bold".to_string(),
        ]));
        assert!(super::requirement_styles_force_bold(&[
            "font-weight: 700 !important".to_string(),
        ]));
        assert!(!super::requirement_styles_force_bold(&[
            "font-weight: normal".to_string(),
            "stroke:blue".to_string(),
        ]));
    }

    #[test]
    fn requirement_calculate_text_width_uses_mermaid_dimension_selection() {
        let style = TextStyle {
            font_family: Some("configured".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };

        assert_eq!(
            calculate_text_width_like_mermaid_px(&FamilySelectionMeasurer, &style, "label"),
            100
        );
    }

    #[test]
    fn requirement_box_wraps_with_root_font_probe_before_group_bbox_padding() {
        let measurer = DeterministicTextMeasurer::default();
        let family = Some(crate::config::MERMAID_DEFAULT_FONT_FAMILY_CSS.to_string());
        let regular = TextStyle {
            font_family: family.clone(),
            font_size: 24.0,
            font_weight: None,
            font_style: None,
        };
        let bold = TextStyle {
            font_family: family.clone(),
            font_size: 24.0,
            font_weight: Some("bold".to_string()),
            font_style: None,
        };
        let calculation = TextStyle {
            font_family: family,
            font_size: 10.0,
            font_weight: None,
            font_style: None,
        };
        let body = "Text: font size precedence should be deterministic";
        let lines = vec![
            node_label_spec(
                "&lt;&lt;Requirement&gt;&gt;".to_string(),
                false,
                false,
                true,
            ),
            node_label_spec("req_font_size".to_string(), true, true, true),
            node_label_spec("ID: req_font_size".to_string(), false, false, false),
            node_label_spec(body.to_string(), false, false, false),
            node_label_spec("Risk: Low".to_string(), false, false, false),
            node_label_spec("Verification: Test".to_string(), false, false, false),
        ];

        let styles = RequirementMeasurementStyles {
            html_regular: regular.clone(),
            html_bold: bold,
            calculation: calculation.clone(),
            html_labels: true,
            edge_html_labels: true,
        };
        let text = measure_requirement_label_metrics(&measurer, &styles, body, body, false, true)
            .expect("text line should be measured");
        let layout = requirement_box_layout(&measurer, &styles, lines, 20.0, 20.0);

        let expected_max_width =
            calculate_text_width_like_mermaid_px(&measurer, &calculation, body) + 50;
        assert_eq!(text.max_width_px, expected_max_width);
        assert_eq!(text.width, expected_max_width as f64);
        assert!(
            text.height > regular.font_size,
            "the constrained requirement body must wrap"
        );
        assert_eq!(layout.width, text.width + 20.0);
        assert!(layout.height > text.height + 20.0);
        assert_eq!(layout.lines.len(), 6);
        assert_eq!(layout.lines[0].y_offset, 0.0);
        assert_eq!(layout.lines[1].y_offset, layout.lines[0].metrics.height);
        assert_eq!(layout.divider_y_offset, Some(layout.lines[2].y_offset));
    }

    #[test]
    fn opaque_render_measurements_rebuild_node_stacking_offsets() {
        let measurer = PhaseHeightMeasurer::default();
        let styles = RequirementMeasurementStyles {
            html_regular: TextStyle::default(),
            html_bold: TextStyle {
                font_weight: Some("bold".to_string()),
                ..TextStyle::default()
            },
            calculation: TextStyle::default(),
            html_labels: true,
            edge_html_labels: true,
        };
        let prepared = requirement_box_layout(
            &measurer,
            &styles,
            vec![
                node_label_spec(
                    "&lt;&lt;Requirement&gt;&gt;".to_string(),
                    false,
                    false,
                    true,
                ),
                node_label_spec("requirement-a".to_string(), true, true, true),
                node_label_spec("ID: REQ-1".to_string(), false, false, false),
            ],
            20.0,
            20.0,
        )
        .into_node_plan()
        .2;
        let prepared_second_offset = prepared.lines[1].y_offset;

        measurer.render_phase.set(true);
        let measurements = RequirementRenderLabelMeasurements {
            measurer: &measurer,
            styles: Some(styles),
            reuse_prepared: false,
        };
        let rendered = measurements
            .node_plan_for_render(&prepared)
            .expect("opaque labels remain measurable");

        assert!(matches!(rendered, Cow::Owned(_)));
        assert!(rendered.lines[0].metrics.height > prepared.lines[0].metrics.height);
        assert_eq!(rendered.lines[1].y_offset, rendered.lines[0].metrics.height);
        assert_ne!(rendered.lines[1].y_offset, prepared_second_offset);
        assert_eq!(
            rendered.divider_y_offset,
            Some(rendered.lines[0].metrics.height + rendered.lines[1].metrics.height + 20.0)
        );
    }
    #[test]
    fn requirement_svg_label_measurements_keep_node_and_edge_padding_distinct() {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let config = serde_json::json!({"htmlLabels":false});
        let styles =
            requirement_measurement_styles(&RequirementConfigView::new(&config).layout_settings());
        let text = "two words";
        let node =
            measure_requirement_label_metrics(&measurer, &styles, text, text, false, true).unwrap();
        let edge = measure_requirement_label_metrics(&measurer, &styles, text, text, false, false)
            .unwrap();
        let raw = crate::text::measure_wrapped_markdown_with_inline_styles(
            &measurer,
            text,
            &styles.html_regular,
            Some(200.0),
            WrapMode::SvgLike,
        );
        assert_eq!(node.width, raw.width);
        assert_eq!(node.height, raw.height + 6.0);
        assert_eq!(edge.width, raw.width + 4.0);
        assert_eq!(edge.height, raw.height + 4.0);
    }
}
