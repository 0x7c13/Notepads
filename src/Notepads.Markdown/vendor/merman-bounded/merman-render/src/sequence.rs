use crate::Result;
use crate::math::MathRenderer;
use crate::model::{LayoutCluster, LayoutNode, SequenceDiagramLayout};
use crate::resources::OperationWorkMeter;
#[cfg(all(test, feature = "diagram-sequence"))]
use crate::resources::RenderResourcePolicy;
use crate::text::TextMeasurer;
use merman_core::diagrams::sequence::{
    SequenceControlKind, SequenceDiagramRenderModel, SequenceMessage,
};
use merman_core::{MermaidConfig, OperationPhase};
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::collections::HashMap;

const SEQUENCE_ACTOR_LAYOUT_WORK_UNITS: usize = 12;
const SEQUENCE_MESSAGE_LAYOUT_WORK_UNITS: usize = 8;
const SEQUENCE_BOX_LAYOUT_WORK_UNITS: usize = 3;
const SEQUENCE_BOX_MEMBERSHIP_WORK_UNITS: usize = 2;
const SEQUENCE_TEXT_LINEAR_REPLAY_WORK_UNITS_PER_BYTE: usize = 8;
const SEQUENCE_LAYOUT_CHECKPOINT_INTERVAL: usize = 64;

mod activation;
mod actors;
mod block_steps;
pub(crate) mod config;
mod constants;
mod message_metrics;
mod messages;
mod metrics;
mod notes;
mod orchestration;
mod rect;
mod root_bounds;

pub(crate) use activation::{sequence_activation_stack_bounds, sequence_activation_start_x};
pub(crate) use constants::{
    SEQUENCE_FRAME_GEOM_PAD_PX, SEQUENCE_FRAME_SIDE_PAD_PX, SEQUENCE_GLYPH_BAND_HEIGHT,
    SEQUENCE_MESSAGE_WRAP_PADDING_SIDES, SEQUENCE_SELF_MESSAGE_FRAME_EXTRA_Y_PX,
    SequenceActorBands, sequence_actor_popup_min_width, sequence_actor_popup_panel_height,
    sequence_actor_popup_rect_height,
};
pub(crate) use metrics::{
    SequenceDrawnTextNode, SequenceMathHeightMode, measure_sequence_drawn_line_height,
    measure_sequence_math_label, sequence_drawn_text_first_y, sequence_drawn_text_style,
    sequence_drawn_text_y, sequence_inline_font_family, wrap_sequence_label_like_mermaid_lines,
};
pub(crate) use notes::sequence_note_final_wrapped_lines;

use actors::{SequenceActorLayoutPlan, SequenceActorLayoutPlanContext, plan_sequence_actors};
use block_steps::{BlockStepPlanContext, calculate_sequence_block_widths};
use config::SequenceLayoutSettings;
use message_metrics::SequenceMessageMetricSidecar;
use orchestration::{SequenceLayoutGraph, SequenceLayoutGraphContext, build_sequence_layout_graph};
use rect::{SequenceRectStackBoundsContext, sequence_rect_stack_x_bounds};
use root_bounds::{SequenceRootBoundsContext, sequence_root_bounds};

/// Phase-aware cancellation projection for Sequence text and math callbacks.
///
/// Text measurement traits remain infallible for host compatibility. Sequence-owned orchestration
/// therefore checks the operation immediately before and after every opaque callback and inside
/// each label-local loop, returning cancellation through the surrounding fallible render path.
#[derive(Clone, Copy)]
pub(crate) struct SequenceTextCheckpoints<'a> {
    work_meter: &'a OperationWorkMeter,
    phase: OperationPhase,
}

impl<'a> SequenceTextCheckpoints<'a> {
    pub(crate) const fn for_phase(
        work_meter: &'a OperationWorkMeter,
        phase: OperationPhase,
    ) -> Self {
        Self { work_meter, phase }
    }

    pub(crate) fn checkpoint(self) -> Result<()> {
        self.work_meter.checkpoint(self.phase).map_err(Into::into)
    }
}

/// Non-billing cancellation projection shared by Sequence derived-geometry passes.
///
/// Main preparation owns the `Layout` phase. SVG-only frame-width reconstruction runs after
/// emission has begun and therefore carries the `Emit` phase through the same bounded loops.
#[derive(Clone, Copy)]
pub(super) struct SequenceLayoutCheckpoints<'a> {
    work_meter: &'a OperationWorkMeter,
    phase: OperationPhase,
}

impl<'a> SequenceLayoutCheckpoints<'a> {
    const fn new(work_meter: &'a OperationWorkMeter) -> Self {
        Self::for_phase(work_meter, OperationPhase::Layout)
    }

    const fn for_phase(work_meter: &'a OperationWorkMeter, phase: OperationPhase) -> Self {
        Self { work_meter, phase }
    }

    pub(super) fn checkpoint(self) -> Result<()> {
        self.work_meter.checkpoint(self.phase).map_err(Into::into)
    }

    pub(super) fn checkpoint_loop(self, iteration: usize) -> Result<()> {
        if iteration.is_multiple_of(SEQUENCE_LAYOUT_CHECKPOINT_INTERVAL) {
            self.checkpoint()?;
        }
        Ok(())
    }

    pub(super) const fn text(self) -> SequenceTextCheckpoints<'a> {
        SequenceTextCheckpoints::for_phase(self.work_meter, self.phase)
    }
}

/// Source-derived box title and spacing retained from actor layout.
#[derive(Debug)]
pub(crate) struct SequenceBoxLayout {
    pub(crate) label: Option<String>,
    pub(crate) margin: f64,
    pub(crate) x: Option<f64>,
    pub(crate) width: f64,
}

/// Private Sequence render artifact that keeps operation-owned measurements attached to layout.
///
/// Do not expose or detach this from the paired semantic model: the metric sidecar is valid only
/// for that immutable model, text style, and built-in measurement route.
#[derive(Debug)]
pub(crate) struct SequencePreparedArtifact {
    layout: SequenceDiagramLayout,
    message_metrics: SequenceMessageMetricSidecar,
    box_layouts: Vec<SequenceBoxLayout>,
    box_title_height: f64,
    box_height: f64,
}

impl SequencePreparedArtifact {
    pub(crate) fn box_layouts(&self) -> &[SequenceBoxLayout] {
        &self.box_layouts
    }

    pub(crate) fn box_height(&self) -> f64 {
        self.box_height
    }

    pub(crate) fn box_title_height(&self) -> f64 {
        self.box_title_height
    }

    pub(crate) fn layout(&self) -> &SequenceDiagramLayout {
        &self.layout
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SequenceLayoutWorkShape {
    actors: usize,
    messages: usize,
    boxes: usize,
    box_memberships: usize,
    text_work: usize,
}

impl SequenceLayoutWorkShape {
    fn work_units(self) -> Option<usize> {
        // Sequence layout has a fixed number of actor and message passes: measurement, spacing,
        // geometry construction, frame propagation, and final bounds. Box membership is scanned
        // twice by Mermaid-compatible actor margin and actor-to-box assignment. Nested frame
        // accumulators are propagated to their parent once, so block depth does not multiply the
        // per-message term.
        self.actors
            .checked_mul(SEQUENCE_ACTOR_LAYOUT_WORK_UNITS)?
            .checked_add(
                self.messages
                    .checked_mul(SEQUENCE_MESSAGE_LAYOUT_WORK_UNITS)?,
            )?
            .checked_add(self.boxes.checked_mul(SEQUENCE_BOX_LAYOUT_WORK_UNITS)?)?
            .checked_add(
                self.box_memberships
                    .checked_mul(SEQUENCE_BOX_MEMBERSHIP_WORK_UNITS)?,
            )
            .and_then(|work| work.checked_add(self.text_work))
    }
}

fn sequence_label_text_work_units(byte_len: usize, wraps: bool) -> Option<usize> {
    // `str::len()` gives an O(1), conservative upper bound for scalar/word scans. The linear
    // term covers the fixed layout, bounds, and SVG replay envelope. Wrapped labels additionally
    // pay a squared-byte envelope before any measurement callback because Mermaid-compatible
    // wrapping repeatedly materializes and measures growing word/line prefixes.
    let linear = byte_len.checked_mul(SEQUENCE_TEXT_LINEAR_REPLAY_WORK_UNITS_PER_BYTE)?;
    let wrapping = if wraps {
        byte_len.checked_mul(byte_len)?
    } else {
        0
    };
    linear.checked_add(wrapping)
}

fn sequence_message_text_work_units(message: &SequenceMessage) -> Option<usize> {
    if let Some(semantics) = message.control_semantics() {
        if semantics.kind == SequenceControlKind::Rect {
            return sequence_label_text_work_units(message.message_text().len(), false);
        }
        if !semantics.consumes_text() {
            return Some(0);
        }
        let byte_len = message.message_text().len().checked_add(2)?;
        return sequence_label_text_work_units(byte_len, true);
    }
    sequence_label_text_work_units(message.message_text().len(), message.wrap)
}

fn sequence_text_work_units_controlled(
    model: &SequenceDiagramRenderModel,
    diagram_title: Option<&str>,
    checkpoints: SequenceLayoutCheckpoints<'_>,
) -> Result<Option<usize>> {
    let mut work_units = 0usize;
    let mut label_index = 0usize;
    let mut include = |label_work: Option<usize>| -> Result<bool> {
        checkpoints.checkpoint_loop(label_index)?;
        label_index = label_index.saturating_add(1);
        let Some(label_work) = label_work else {
            return Ok(false);
        };
        let Some(next) = work_units.checked_add(label_work) else {
            return Ok(false);
        };
        work_units = next;
        Ok(true)
    };

    for actor_id in &model.actor_order {
        if let Some(actor) = model.actors.get(actor_id) {
            if !include(sequence_label_text_work_units(
                actor.description.len(),
                actor.wrap,
            ))? {
                return Ok(None);
            }
            for (label, url) in &actor.links {
                let Some(url) = url.as_str() else {
                    continue;
                };
                let Some(link_bytes) = label.len().checked_add(url.len()) else {
                    return Ok(None);
                };
                if !include(sequence_label_text_work_units(link_bytes, false))? {
                    return Ok(None);
                }
            }
        }
    }
    for message in &model.messages {
        if !include(sequence_message_text_work_units(message))? {
            return Ok(None);
        }
    }
    for sequence_box in &model.boxes {
        if let Some(name) = sequence_box.name.as_deref()
            && !include(sequence_label_text_work_units(name.len(), false))?
        {
            return Ok(None);
        }
    }
    if let Some(title) = sequence_render_title(model.title.as_deref(), diagram_title)
        && !include(sequence_label_text_work_units(title.len(), false))?
    {
        return Ok(None);
    }

    checkpoints.checkpoint()?;
    Ok(Some(work_units))
}

#[cfg(all(test, feature = "diagram-sequence"))]
fn sequence_layout_work_units(model: &SequenceDiagramRenderModel) -> Option<usize> {
    let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
    sequence_layout_work_units_controlled(model, None, SequenceLayoutCheckpoints::new(&meter))
        .ok()
        .flatten()
}

fn sequence_layout_work_units_controlled(
    model: &SequenceDiagramRenderModel,
    diagram_title: Option<&str>,
    checkpoints: SequenceLayoutCheckpoints<'_>,
) -> Result<Option<usize>> {
    let mut box_memberships = 0usize;
    let mut membership_index = 0usize;
    for sequence_box in &model.boxes {
        for _ in &sequence_box.actor_keys {
            checkpoints.checkpoint_loop(membership_index)?;
            membership_index = membership_index.saturating_add(1);
            let Some(next) = box_memberships.checked_add(1) else {
                return Ok(None);
            };
            box_memberships = next;
        }
    }
    checkpoints.checkpoint()?;

    let Some(text_work) = sequence_text_work_units_controlled(model, diagram_title, checkpoints)?
    else {
        return Ok(None);
    };

    Ok(SequenceLayoutWorkShape {
        actors: model.actor_order.len(),
        messages: model.messages.len(),
        boxes: model.boxes.len(),
        box_memberships,
        text_work,
    }
    .work_units())
}

pub(crate) fn bracketize_sequence_block_label(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "\u{200B}".to_string();
    }
    format!("[{trimmed}]")
}

pub(crate) fn sequence_block_label_wrap_width(block_width: f64, wrap_padding: f64) -> f64 {
    (block_width - 2.0 * wrap_padding).max(1.0)
}

pub(crate) fn sequence_block_widths_for_render(
    model: &SequenceDiagramRenderModel,
    prepared: &SequencePreparedArtifact,
    nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    work_meter: &OperationWorkMeter,
) -> Result<FxHashMap<String, f64>> {
    let checkpoints = SequenceLayoutCheckpoints::for_phase(work_meter, OperationPhase::Emit);
    checkpoints.checkpoint()?;
    let settings = SequenceLayoutSettings::from_effective_config(effective_config.as_value());
    // SVG frame emission reconstructs Mermaid's `calculateLoopBounds` after Rust layout has
    // already completed. Only the built-in operation route may carry its earlier message bounds
    // across this private split; host and custom routes deliberately replay every callback.
    let message_metrics = prepared
        .message_metrics
        .view(model, &settings.msg_text_style, measurer);
    let mut actor_index = HashMap::with_capacity(model.actor_order.len());
    for (actor_position, actor_id) in model.actor_order.iter().enumerate() {
        checkpoints.checkpoint_loop(actor_position)?;
        actor_index.insert(actor_id.as_str(), actor_position);
    }
    let mut actor_centers_x = Vec::with_capacity(model.actor_order.len());
    let mut actor_widths = Vec::with_capacity(model.actor_order.len());
    for (actor_position, actor_id) in model.actor_order.iter().enumerate() {
        checkpoints.checkpoint_loop(actor_position)?;
        let node_id = format!("actor-top-{actor_id}");
        let Some(node) = nodes_by_id.get(node_id.as_str()).copied() else {
            return Ok(FxHashMap::default());
        };
        actor_centers_x.push(node.x);
        actor_widths.push(node.width);
    }

    let widths = calculate_sequence_block_widths(BlockStepPlanContext {
        model,
        actor_index: &actor_index,
        actor_centers_x: &actor_centers_x,
        actor_widths: &actor_widths,
        actor_margin: settings.actor_margin,
        activation_width: settings.activation_width,
        box_margin: settings.box_margin,
        box_text_margin: settings.box_text_margin,
        label_box_height: settings.label_box_height,
        label_box_width: settings.label_box_width,
        sequence_default_width: settings.sequence_default_width,
        wrap_padding: settings.wrap_padding,
        note_margin: settings.note_margin,
        is_neo: settings.is_neo,
        measurer,
        msg_text_style: &settings.msg_text_style,
        note_text_style: &settings.note_text_style,
        math_config: effective_config,
        math_renderer,
        message_metrics,
        checkpoints,
    })?;
    let mut widths_by_id = FxHashMap::default();
    for (index, (id, width)) in widths.into_iter().enumerate() {
        checkpoints.checkpoint_loop(index)?;
        widths_by_id.insert(id, width);
    }
    checkpoints.checkpoint()?;
    Ok(widths_by_id)
}

/// Prepares a Sequence model under the cumulative work meter owned by the render operation.
pub(crate) fn prepare_sequence_diagram_typed_with_title_and_work_meter(
    model: &SequenceDiagramRenderModel,
    diagram_title: Option<&str>,
    effective_config: &Value,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    work_meter: &OperationWorkMeter,
) -> Result<SequencePreparedArtifact> {
    let checkpoints = SequenceLayoutCheckpoints::new(work_meter);
    checkpoints.checkpoint()?;
    work_meter.preflight_sequence_complexity(model, OperationPhase::Layout)?;
    checkpoints.checkpoint()?;
    let work_units = sequence_layout_work_units_controlled(model, diagram_title, checkpoints)?
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    work_meter.charge(work_units)?;

    let math_config = MermaidConfig::from_value(effective_config.clone());
    let settings = SequenceLayoutSettings::from_effective_config(effective_config);
    checkpoints.checkpoint()?;

    let SequenceActorLayoutPlan {
        actor_index,
        actor_widths,
        actor_base_heights,
        actor_text_heights,
        actor_centers_x,
        box_layouts,
        box_title_height,
        actor_top_offset_y,
        max_actor_layout_height,
        has_boxes,
        message_metrics,
    } = plan_sequence_actors(SequenceActorLayoutPlanContext {
        model,
        measurer,
        actor_text_style: &settings.actor_text_style,
        note_text_style: &settings.note_text_style,
        msg_text_style: &settings.msg_text_style,
        math_config: &math_config,
        math_renderer,
        actor_width_min: settings.sequence_default_width,
        actor_height: settings.actor_height,
        is_neo: settings.is_neo,
        actor_margin: settings.actor_margin,
        box_margin: settings.box_margin,
        box_text_margin: settings.box_text_margin,
        wrap_padding: settings.wrap_padding,
        checkpoints,
    })?;
    let message_metric_view = message_metrics.view(model, &settings.msg_text_style, measurer);

    let clusters: Vec<LayoutCluster> = Vec::new();

    let SequenceLayoutGraph {
        mut nodes,
        edges,
        block_layouts_by_id,
        bottom_box_top_y,
        box_height,
        bounds_start_x,
        bounds_stop_x,
    } = build_sequence_layout_graph(SequenceLayoutGraphContext {
        model,
        actor_index: &actor_index,
        actor_centers_x: &actor_centers_x,
        actor_widths: &actor_widths,
        actor_base_heights: &actor_base_heights,
        actor_text_heights: &actor_text_heights,
        actor_top_offset_y,
        max_actor_layout_height,
        sequence_default_width: settings.sequence_default_width,
        actor_height: settings.actor_height,
        actor_margin: settings.actor_margin,
        box_margin: settings.box_margin,
        note_margin: settings.note_margin,
        box_text_margin: settings.box_text_margin,
        label_box_height: settings.label_box_height,
        label_box_width: settings.label_box_width,
        right_angles: settings.right_angles,
        is_neo: settings.is_neo,
        wrap_padding: settings.wrap_padding,
        mirror_actors: settings.mirror_actors,
        activation_width: settings.activation_width,
        measurer,
        msg_text_style: &settings.msg_text_style,
        note_text_style: &settings.note_text_style,
        math_config: &math_config,
        math_renderer,
        message_metrics: message_metric_view,
        checkpoints,
    })?;

    let rect_x_bounds = sequence_rect_stack_x_bounds(SequenceRectStackBoundsContext {
        model,
        actor_index: &actor_index,
        actor_centers_x: &actor_centers_x,
        edges: &edges,
        nodes: &nodes,
        actor_width_min: settings.sequence_default_width,
        box_margin: settings.box_margin,
        checkpoints,
    })?;
    if !rect_x_bounds.is_empty() {
        for (node_index, n) in nodes.iter_mut().enumerate() {
            checkpoints.checkpoint_loop(node_index)?;
            let Some(start_id) = n.id.strip_prefix("rect-") else {
                continue;
            };
            let Some((min_x, max_x)) = rect_x_bounds.get(start_id).copied() else {
                continue;
            };
            n.x = (min_x + max_x) / 2.0;
            n.width = (max_x - min_x).max(1.0);
        }
    }

    let bounds = Some(sequence_root_bounds(SequenceRootBoundsContext {
        model,
        diagram_title,
        nodes: &nodes,
        edges: &edges,
        bounds_start_x,
        bounds_stop_x,
        actor_index: &actor_index,
        actor_centers_x: &actor_centers_x,
        box_layouts: &box_layouts,
        actor_width_min: settings.sequence_default_width,
        actor_height: settings.actor_height,
        bottom_box_top_y,
        diagram_margin_x: settings.diagram_margin_x,
        diagram_margin_y: settings.diagram_margin_y,
        bottom_margin_adj: settings.bottom_margin_adj,
        box_margin: settings.box_margin,
        wrap_padding: settings.wrap_padding,
        has_boxes,
        mirror_actors: settings.mirror_actors,
        measurer,
        actor_text_style: &settings.actor_text_style,
        msg_text_style: &settings.msg_text_style,
        math_config: &math_config,
        math_renderer,
        message_metrics: message_metric_view,
        checkpoints,
    })?);
    checkpoints.checkpoint()?;

    Ok(SequencePreparedArtifact {
        layout: SequenceDiagramLayout {
            nodes,
            edges,
            clusters,
            bounds,
            block_layouts_by_id,
        },
        message_metrics,
        box_layouts,
        box_title_height,
        box_height,
    })
}

pub(crate) fn sequence_render_title<'a>(
    model_title: Option<&'a str>,
    diagram_title: Option<&'a str>,
) -> Option<&'a str> {
    if model_title.is_none_or(|t| t.trim().is_empty())
        && let Some(title) = diagram_title.map(str::trim).filter(|t| !t.is_empty())
    {
        return Some(title);
    }
    model_title
}

#[cfg(all(test, feature = "diagram-sequence"))]
mod resource_tests {
    use super::{
        SEQUENCE_MESSAGE_LAYOUT_WORK_UNITS, SequenceLayoutWorkShape,
        prepare_sequence_diagram_typed_with_title_and_work_meter, sequence_block_widths_for_render,
        sequence_label_text_work_units, sequence_layout_work_units,
        sequence_message_text_work_units,
    };
    use crate::Error;
    use crate::resources::{
        OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
    };
    use crate::text::{DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle};
    use merman_core::{
        Engine, MermaidConfig, OperationControl, OperationPhase, ParseOptions, RenderSemanticModel,
    };
    use rustc_hash::FxHashMap;
    use serde_json::json;
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;
    use std::fmt::Write;

    #[derive(Default)]
    struct CountingTextMeasurer {
        calls: Cell<usize>,
        inner: DeterministicTextMeasurer,
    }

    impl TextMeasurer for CountingTextMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            self.calls.set(self.calls.get() + 1);
            self.inner.measure(text, style)
        }
    }

    struct CancellingTextMeasurer {
        texts_after_cancellation: RefCell<HashSet<String>>,
        trigger_occurrences: Cell<usize>,
        trigger: String,
        cancel_after_occurrence: usize,
        control: OperationControl,
        inner: DeterministicTextMeasurer,
    }

    impl TextMeasurer for CancellingTextMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            if self.control.is_cancelled() {
                self.texts_after_cancellation
                    .borrow_mut()
                    .insert(text.to_string());
            }
            if text == self.trigger {
                let occurrences = self.trigger_occurrences.get() + 1;
                self.trigger_occurrences.set(occurrences);
                if occurrences == self.cancel_after_occurrence {
                    self.control.cancel();
                }
            }
            self.inner.measure(text, style)
        }
    }

    fn sequence_model(source: &str) -> merman_core::diagrams::sequence::SequenceDiagramRenderModel {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        let RenderSemanticModel::Sequence(model) = parsed.model() else {
            panic!("expected Sequence model");
        };
        model.clone()
    }

    #[test]
    fn nested_sequence_frames_have_linear_layout_work() {
        let flat = sequence_model("sequenceDiagram\nA->>B: hi\n");
        let nested =
            sequence_model("sequenceDiagram\nloop outer\nloop inner\nA->>B: hi\nend\nend\n");
        let flat_text_work = flat
            .messages
            .iter()
            .map(|message| sequence_message_text_work_units(message).unwrap())
            .sum::<usize>();
        let nested_text_work = nested
            .messages
            .iter()
            .map(|message| sequence_message_text_work_units(message).unwrap())
            .sum::<usize>();

        assert_eq!(
            sequence_layout_work_units(&nested).unwrap()
                - sequence_layout_work_units(&flat).unwrap(),
            (nested.messages.len() - flat.messages.len()) * SEQUENCE_MESSAGE_LAYOUT_WORK_UNITS
                + nested_text_work
                - flat_text_work
        );
    }

    #[test]
    fn control_labels_budget_the_renderer_owned_brackets_and_wrapping() {
        let model = sequence_model("sequenceDiagram\nloop a long control label\nA->>B: hi\nend\n");
        let control = model
            .messages
            .iter()
            .find(|message| message.control_semantics().is_some())
            .expect("loop control record");
        assert!(!control.wrap);

        let bracketed_bytes = control.message_text().len() + 2;
        assert_eq!(
            sequence_message_text_work_units(control),
            sequence_label_text_work_units(bracketed_bytes, true)
        );
    }

    #[test]
    fn invisible_control_records_do_not_budget_label_measurement_work() {
        let model = sequence_model(
            "sequenceDiagram\nrect rgb(240,240,240)\nA->>B: hi\nend\nloop visible\nA->>B: hi\nend\n",
        );
        let controls = model
            .messages
            .iter()
            .filter_map(|message| {
                message
                    .control_semantics()
                    .map(|semantics| (message, semantics))
            })
            .collect::<Vec<_>>();

        assert!(controls.len() >= 4);
        for (message, semantics) in controls {
            let work = sequence_message_text_work_units(message)
                .expect("control label work should remain representable");
            if !semantics.consumes_text() {
                assert_eq!(work, 0, "{semantics:?}");
            } else {
                assert!(work > 0, "{semantics:?}");
            }
        }
    }

    #[test]
    fn actor_link_labels_and_urls_are_included_in_sequence_work() {
        let without_link = sequence_model("sequenceDiagram\nparticipant A\n");
        let with_link = sequence_model(
            "sequenceDiagram\nparticipant A\nlink A: Documentation @ https://example.test/docs\n",
        );

        assert!(
            sequence_layout_work_units(&with_link).unwrap()
                > sequence_layout_work_units(&without_link).unwrap()
        );
    }

    #[test]
    fn participant_only_sequence_is_reported_and_budgeted() {
        let model = sequence_model(include_str!(
            "../../../fixtures/sequence/upstream_pkgtests_sequencediagram_spec_094.mmd"
        ));
        let expected_work = sequence_layout_work_units(&model).unwrap();
        assert!(expected_work > 0);

        let measurer = CountingTextMeasurer::default();
        let narrow_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, expected_work - 1)
            .unwrap();
        let narrow_meter = OperationWorkMeter::new(narrow_policy);
        let error = prepare_sequence_diagram_typed_with_title_and_work_meter(
            &model,
            None,
            &json!({}),
            &measurer,
            None,
            &narrow_meter,
        )
        .unwrap_err();
        let Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.actual, expected_work);
        assert_eq!(narrow_meter.used(), 0);
        assert_eq!(measurer.calls.get(), 0);

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, expected_work)
            .unwrap();
        let exact_meter = OperationWorkMeter::new(exact_policy);
        prepare_sequence_diagram_typed_with_title_and_work_meter(
            &model,
            None,
            &json!({}),
            &measurer,
            None,
            &exact_meter,
        )
        .unwrap();
        assert_eq!(exact_meter.used(), expected_work);
        assert!(measurer.calls.get() > 0);
    }

    #[test]
    fn wrapped_sequence_text_is_admitted_before_measurement_callbacks() {
        let mut model = sequence_model("sequenceDiagram\nparticipant A\n");
        let actor = model.actors.get_mut("A").expect("participant A");
        actor.description = "x".repeat(130);
        actor.wrap = true;
        let expected_work = sequence_layout_work_units(&model).unwrap();

        let measurer = CountingTextMeasurer::default();
        let narrow_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, expected_work - 1)
            .unwrap();
        let narrow_meter = OperationWorkMeter::new(narrow_policy);
        let error = prepare_sequence_diagram_typed_with_title_and_work_meter(
            &model,
            None,
            &json!({}),
            &measurer,
            None,
            &narrow_meter,
        )
        .unwrap_err();
        let Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };

        assert_eq!(error.actual, expected_work);
        assert_eq!(narrow_meter.used(), 0);
        assert_eq!(measurer.calls.get(), 0);

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, expected_work)
            .unwrap();
        let exact_meter = OperationWorkMeter::new(exact_policy);
        prepare_sequence_diagram_typed_with_title_and_work_meter(
            &model,
            None,
            &json!({}),
            &measurer,
            None,
            &exact_meter,
        )
        .unwrap();

        assert_eq!(exact_meter.used(), expected_work);
        assert!(measurer.calls.get() > 0);
    }

    #[test]
    fn duplicate_participants_follow_the_normalized_mermaid_actor_order() {
        let single = sequence_model(include_str!(
            "../../../fixtures/sequence/upstream_pkgtests_sequencediagram_spec_094.mmd"
        ));
        let repeated = sequence_model(include_str!(
            "../../../fixtures/sequence/upstream_duplicate_participants_single_actor_spec.mmd"
        ));

        assert_eq!(single.actor_order, repeated.actor_order);
        assert_eq!(
            sequence_layout_work_units(&single),
            sequence_layout_work_units(&repeated)
        );
    }

    #[test]
    fn sequence_svg_frame_width_reconstruction_observes_mid_emit_cancellation() {
        const SIGNAL_COUNT: usize = 130;

        let mut source =
            String::from("sequenceDiagram\nparticipant A\nparticipant B\nloop outer\n");
        for index in 0..SIGNAL_COUNT {
            let label = if index == 0 {
                "trigger".to_string()
            } else {
                format!("message-{index}")
            };
            writeln!(&mut source, "A->>B: $${label}$$").unwrap();
        }
        source.push_str("end\n");
        let model = sequence_model(&source);
        let preparation_meter =
            OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let prepared = prepare_sequence_diagram_typed_with_title_and_work_meter(
            &model,
            None,
            &json!({}),
            &DeterministicTextMeasurer::default(),
            None,
            &preparation_meter,
        )
        .unwrap();

        let control = OperationControl::new();
        let measurer = CancellingTextMeasurer {
            texts_after_cancellation: RefCell::new(HashSet::new()),
            trigger_occurrences: Cell::new(0),
            trigger: "$$trigger$$".to_string(),
            cancel_after_occurrence: 1,
            control: control.clone(),
            inner: DeterministicTextMeasurer::default(),
        };
        let work_meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control,
        );
        let nodes_by_id: FxHashMap<&str, &crate::model::LayoutNode> = prepared
            .layout()
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();

        let error = sequence_block_widths_for_render(
            &model,
            &prepared,
            &nodes_by_id,
            &MermaidConfig::from_value(json!({})),
            &measurer,
            None,
            &work_meter,
        )
        .unwrap_err();
        let Error::Cancelled(error) = error else {
            panic!("expected Sequence frame planning cancellation");
        };

        assert_eq!(error.phase, OperationPhase::Emit);
        assert!(measurer.trigger_occurrences.get() >= 1);
        assert!(
            measurer.texts_after_cancellation.borrow().len() < 64,
            "frame planning exceeded the cooperative checkpoint cadence after cancellation"
        );
        assert_eq!(work_meter.used(), 0);
    }

    #[test]
    fn actor_and_box_membership_work_curves_are_linear() {
        for actors in [1usize, 32, 1_024] {
            assert_eq!(
                SequenceLayoutWorkShape {
                    actors,
                    messages: 0,
                    boxes: 0,
                    box_memberships: 0,
                    text_work: 0,
                }
                .work_units(),
                Some(actors * super::SEQUENCE_ACTOR_LAYOUT_WORK_UNITS)
            );
        }

        let one_membership = SequenceLayoutWorkShape {
            actors: 2,
            messages: 0,
            boxes: 1,
            box_memberships: 1,
            text_work: 0,
        }
        .work_units()
        .unwrap();
        let many_memberships = SequenceLayoutWorkShape {
            actors: 2,
            messages: 0,
            boxes: 1,
            box_memberships: 65,
            text_work: 0,
        }
        .work_units()
        .unwrap();
        assert_eq!(
            many_memberships - one_membership,
            64 * super::SEQUENCE_BOX_MEMBERSHIP_WORK_UNITS
        );
    }

    #[test]
    fn sequence_layout_work_shape_fails_closed_on_overflow() {
        assert_eq!(
            SequenceLayoutWorkShape {
                actors: usize::MAX,
                messages: 0,
                boxes: 0,
                box_memberships: 0,
                text_work: 0,
            }
            .work_units(),
            None
        );
    }
}
