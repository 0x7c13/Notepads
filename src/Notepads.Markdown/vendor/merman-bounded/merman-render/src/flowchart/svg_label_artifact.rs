//! Operation-local preparation for non-Markdown Flowchart SVG labels.

use std::borrow::Cow;
#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
#[cfg(test)]
use std::sync::Mutex;

use rustc_hash::FxHashMap;

use crate::environment::{BuiltinTextMeasurementOperationCarrier, TextMeasurementOperation};
use crate::text::{TextMeasurer, TextMetrics, TextStyle, WrapMode};

use super::label::{
    FlowchartLabelMetricsRequest, FlowchartSvgLabelSource, FlowchartSvgWidthMode,
    flowchart_label_metrics_for_layout,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FlowchartSvgLabelOwner {
    Node(usize),
    EmptySubgraphNode(usize),
    Edge(usize),
    SubgraphTitle(usize),
    SwimlaneNode(usize),
    SwimlaneEdgeLabel(usize),
}

impl FlowchartSvgLabelOwner {
    fn semantic_index(self) -> usize {
        match self {
            Self::Node(index)
            | Self::EmptySubgraphNode(index)
            | Self::Edge(index)
            | Self::SubgraphTitle(index)
            | Self::SwimlaneNode(index)
            | Self::SwimlaneEdgeLabel(index) => index,
        }
    }
}

#[derive(Debug)]
struct FlowchartSvgLabelRoleSlots<T> {
    entries: Vec<(usize, T)>,
}

impl<T> Default for FlowchartSvgLabelRoleSlots<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> FlowchartSvgLabelRoleSlots<T> {
    fn get(&self, index: usize) -> Option<&T> {
        self.entries
            .binary_search_by_key(&index, |(entry_index, _)| *entry_index)
            .ok()
            .map(|position| &self.entries[position].1)
    }

    fn insert(&mut self, index: usize, value: T) {
        if self
            .entries
            .last()
            .is_none_or(|(last_index, _)| *last_index < index)
        {
            self.entries.push((index, value));
            return;
        }

        match self
            .entries
            .binary_search_by_key(&index, |(entry_index, _)| *entry_index)
        {
            Ok(position) => self.entries[position].1 = value,
            Err(position) => self.entries.insert(position, (index, value)),
        }
    }

    fn remove(&mut self, index: usize) -> Option<T> {
        self.entries
            .binary_search_by_key(&index, |(entry_index, _)| *entry_index)
            .ok()
            .map(|position| self.entries.remove(position).1)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    fn into_iter(self) -> impl Iterator<Item = (usize, T)> {
        self.entries.into_iter()
    }
}

/// Compact semantic-owner storage. Entries are sorted by semantic index within each role,
/// so sparse source indices do not allocate placeholders for labels that do not exist.
#[derive(Debug)]
struct FlowchartSvgLabelSlots<T> {
    nodes: FlowchartSvgLabelRoleSlots<T>,
    empty_subgraph_nodes: FlowchartSvgLabelRoleSlots<T>,
    edges: FlowchartSvgLabelRoleSlots<T>,
    subgraph_titles: FlowchartSvgLabelRoleSlots<T>,
    swimlane_nodes: FlowchartSvgLabelRoleSlots<T>,
    swimlane_edge_labels: FlowchartSvgLabelRoleSlots<T>,
}

impl<T> Default for FlowchartSvgLabelSlots<T> {
    fn default() -> Self {
        Self {
            nodes: FlowchartSvgLabelRoleSlots::default(),
            empty_subgraph_nodes: FlowchartSvgLabelRoleSlots::default(),
            edges: FlowchartSvgLabelRoleSlots::default(),
            subgraph_titles: FlowchartSvgLabelRoleSlots::default(),
            swimlane_nodes: FlowchartSvgLabelRoleSlots::default(),
            swimlane_edge_labels: FlowchartSvgLabelRoleSlots::default(),
        }
    }
}

impl<T> FlowchartSvgLabelSlots<T> {
    fn get(&self, owner: FlowchartSvgLabelOwner) -> Option<&T> {
        match owner {
            FlowchartSvgLabelOwner::Node(index) => self.nodes.get(index),
            FlowchartSvgLabelOwner::EmptySubgraphNode(index) => {
                self.empty_subgraph_nodes.get(index)
            }
            FlowchartSvgLabelOwner::Edge(index) => self.edges.get(index),
            FlowchartSvgLabelOwner::SubgraphTitle(index) => self.subgraph_titles.get(index),
            FlowchartSvgLabelOwner::SwimlaneNode(index) => self.swimlane_nodes.get(index),
            FlowchartSvgLabelOwner::SwimlaneEdgeLabel(index) => {
                self.swimlane_edge_labels.get(index)
            }
        }
    }

    fn insert(&mut self, owner: FlowchartSvgLabelOwner, value: T) {
        match owner {
            FlowchartSvgLabelOwner::Node(index) => self.nodes.insert(index, value),
            FlowchartSvgLabelOwner::EmptySubgraphNode(index) => {
                self.empty_subgraph_nodes.insert(index, value)
            }
            FlowchartSvgLabelOwner::Edge(index) => self.edges.insert(index, value),
            FlowchartSvgLabelOwner::SubgraphTitle(index) => {
                self.subgraph_titles.insert(index, value)
            }
            FlowchartSvgLabelOwner::SwimlaneNode(index) => self.swimlane_nodes.insert(index, value),
            FlowchartSvgLabelOwner::SwimlaneEdgeLabel(index) => {
                self.swimlane_edge_labels.insert(index, value)
            }
        }
    }

    fn remove(&mut self, owner: FlowchartSvgLabelOwner) -> Option<T> {
        match owner {
            FlowchartSvgLabelOwner::Node(index) => self.nodes.remove(index),
            FlowchartSvgLabelOwner::EmptySubgraphNode(index) => {
                self.empty_subgraph_nodes.remove(index)
            }
            FlowchartSvgLabelOwner::Edge(index) => self.edges.remove(index),
            FlowchartSvgLabelOwner::SubgraphTitle(index) => self.subgraph_titles.remove(index),
            FlowchartSvgLabelOwner::SwimlaneNode(index) => self.swimlane_nodes.remove(index),
            FlowchartSvgLabelOwner::SwimlaneEdgeLabel(index) => {
                self.swimlane_edge_labels.remove(index)
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        [
            &self.nodes,
            &self.empty_subgraph_nodes,
            &self.edges,
            &self.subgraph_titles,
            &self.swimlane_nodes,
            &self.swimlane_edge_labels,
        ]
        .into_iter()
        .map(|slots| slots.len())
        .sum()
    }

    fn for_each(self, mut visit: impl FnMut(FlowchartSvgLabelOwner, T)) {
        for (index, value) in self.nodes.into_iter() {
            visit(FlowchartSvgLabelOwner::Node(index), value);
        }
        for (index, value) in self.empty_subgraph_nodes.into_iter() {
            visit(FlowchartSvgLabelOwner::EmptySubgraphNode(index), value);
        }
        for (index, value) in self.edges.into_iter() {
            visit(FlowchartSvgLabelOwner::Edge(index), value);
        }
        for (index, value) in self.subgraph_titles.into_iter() {
            visit(FlowchartSvgLabelOwner::SubgraphTitle(index), value);
        }
        for (index, value) in self.swimlane_nodes.into_iter() {
            visit(FlowchartSvgLabelOwner::SwimlaneNode(index), value);
        }
        for (index, value) in self.swimlane_edge_labels.into_iter() {
            visit(FlowchartSvgLabelOwner::SwimlaneEdgeLabel(index), value);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlowchartSvgTextStyleKey {
    font_family: Option<String>,
    font_size_bits: u64,
    font_weight: Option<String>,
    font_style: Option<String>,
}

impl FlowchartSvgTextStyleKey {
    fn new(style: &TextStyle) -> Self {
        Self {
            font_family: style.font_family.clone(),
            font_size_bits: canonical_f64_bits(style.font_size),
            font_weight: style.font_weight.clone(),
            font_style: style.font_style.clone(),
        }
    }

    fn matches(&self, style: &TextStyle) -> bool {
        self.font_family.as_deref() == style.font_family.as_deref()
            && self.font_size_bits == canonical_f64_bits(style.font_size)
            && self.font_weight.as_deref() == style.font_weight.as_deref()
            && self.font_style.as_deref() == style.font_style.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlowchartSvgLabelBinding {
    wrap_style: FlowchartSvgTextStyleKey,
    metrics_style: FlowchartSvgTextStyleKey,
    max_width_bits: Option<u64>,
    break_long_words: bool,
    width_mode: FlowchartSvgWidthMode,
    computed_length_carrier: BuiltinTextMeasurementOperationCarrier,
    wrapped_carrier: BuiltinTextMeasurementOperationCarrier,
}

#[derive(Debug, Clone, Copy)]
struct FlowchartSvgLabelBindingRequest<'a> {
    wrap_style: &'a TextStyle,
    metrics_style: &'a TextStyle,
    max_width_bits: Option<u64>,
    break_long_words: bool,
    width_mode: FlowchartSvgWidthMode,
    computed_length_carrier: BuiltinTextMeasurementOperationCarrier,
    wrapped_carrier: BuiltinTextMeasurementOperationCarrier,
}

impl<'a> FlowchartSvgLabelBindingRequest<'a> {
    fn for_measurer(
        measurer: &dyn TextMeasurer,
        wrap_style: &'a TextStyle,
        metrics_style: &'a TextStyle,
        max_width_px: Option<f64>,
        break_long_words: bool,
        width_mode: FlowchartSvgWidthMode,
    ) -> Option<Self> {
        Some(Self {
            wrap_style,
            metrics_style,
            max_width_bits: normalized_width_bits(max_width_px),
            break_long_words,
            width_mode,
            computed_length_carrier: measurer
                .builtin_operation_carrier(TextMeasurementOperation::ComputedLength)?,
            wrapped_carrier: measurer
                .builtin_operation_carrier(TextMeasurementOperation::Wrapped)?,
        })
    }

    fn into_owned(self) -> FlowchartSvgLabelBinding {
        FlowchartSvgLabelBinding {
            wrap_style: FlowchartSvgTextStyleKey::new(self.wrap_style),
            metrics_style: FlowchartSvgTextStyleKey::new(self.metrics_style),
            max_width_bits: self.max_width_bits,
            break_long_words: self.break_long_words,
            width_mode: self.width_mode,
            computed_length_carrier: self.computed_length_carrier,
            wrapped_carrier: self.wrapped_carrier,
        }
    }
}

impl FlowchartSvgLabelBinding {
    fn matches(&self, request: &FlowchartSvgLabelBindingRequest<'_>) -> bool {
        self.wrap_style.matches(request.wrap_style)
            && self.metrics_style.matches(request.metrics_style)
            && self.max_width_bits == request.max_width_bits
            && self.break_long_words == request.break_long_words
            && self.width_mode == request.width_mode
            && self.computed_length_carrier == request.computed_length_carrier
            && self.wrapped_carrier == request.wrapped_carrier
    }
}

fn canonical_f64_bits(value: f64) -> u64 {
    if value == 0.0 {
        0.0_f64.to_bits()
    } else if value.is_nan() {
        f64::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

fn normalized_width_bits(width: Option<f64>) -> Option<u64> {
    width
        .filter(|width| width.is_finite() && *width > 0.0)
        .map(canonical_f64_bits)
}

/// A fully measured label bound to one exact style, width, wrapping mode, and built-in route.
#[derive(Debug)]
pub(crate) struct PreparedFlowchartSvgLabel {
    binding: FlowchartSvgLabelBinding,
    wrapped_lines: Vec<Vec<String>>,
    metrics: TextMetrics,
}

#[derive(Debug)]
struct FlowchartSvgLabelSourceEntry {
    raw_source: Box<str>,
    source: FlowchartSvgLabelSource,
}

impl FlowchartSvgLabelSourceEntry {
    fn new(raw_source: &str) -> Self {
        Self {
            raw_source: raw_source.into(),
            source: FlowchartSvgLabelSource::new(raw_source),
        }
    }

    fn matches(&self, raw_source: &str) -> bool {
        self.raw_source.as_ref() == raw_source
    }
}

impl PreparedFlowchartSvgLabel {
    pub(crate) fn wrapped_lines(&self) -> &[Vec<String>] {
        &self.wrapped_lines
    }

    pub(crate) fn metrics(&self) -> TextMetrics {
        self.metrics
    }

    fn matches(&self, binding: &FlowchartSvgLabelBindingRequest<'_>) -> bool {
        self.binding.matches(binding)
    }
}

/// Mutable preparation state used only while one Flowchart family artifact is laid out.
///
/// Pure source projections are retained for every eligible label. Measured results are retained
/// only when both operations resolve directly to built-in routes, so host callbacks, host
/// fallback, and opaque custom measurers keep their original layout-to-render request sequence and
/// measurement report while avoiding repeated tokenization.
#[derive(Debug, Default)]
pub(crate) struct FlowchartSvgLabelSidecarBuilder {
    pending: RefCell<PendingFlowchartSvgLabels>,
    #[cfg(test)]
    prepared_hits: Cell<usize>,
}

#[derive(Debug, Default)]
struct PendingFlowchartSvgLabels {
    sources: FlowchartSvgLabelSlots<FlowchartSvgLabelSourceEntry>,
    prepared: FlowchartSvgLabelSlots<PreparedFlowchartSvgLabel>,
    render_ids: FlowchartSvgLabelSlots<Box<str>>,
}

impl FlowchartSvgLabelSidecarBuilder {
    pub(crate) fn measure_for_layout(
        &self,
        owner: FlowchartSvgLabelOwner,
        render_id: &str,
        request: FlowchartLabelMetricsRequest<'_>,
        break_long_words: bool,
        width_mode: FlowchartSvgWidthMode,
    ) -> TextMetrics {
        let metrics_style = request.style;
        self.measure_for_layout_with_metrics_style(
            owner,
            render_id,
            request,
            metrics_style,
            break_long_words,
            width_mode,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn measure_for_layout_with_metrics_style(
        &self,
        owner: FlowchartSvgLabelOwner,
        render_id: &str,
        request: FlowchartLabelMetricsRequest<'_>,
        metrics_style: &TextStyle,
        break_long_words: bool,
        width_mode: FlowchartSvgWidthMode,
    ) -> TextMetrics {
        if !supports_svg_source_preparation(&request) {
            return flowchart_label_metrics_for_layout(FlowchartLabelMetricsRequest {
                style: metrics_style,
                ..request
            });
        }

        let binding = FlowchartSvgLabelBindingRequest::for_measurer(
            request.measurer,
            request.style,
            metrics_style,
            request.max_width_px,
            break_long_words,
            width_mode,
        );

        let measured_from_existing_source = {
            let pending = self.pending.borrow();
            let source = pending
                .render_ids
                .get(owner)
                .filter(|pending_render_id| pending_render_id.as_ref() == render_id)
                .and_then(|_| pending.sources.get(owner))
                .filter(|source| source.matches(request.raw_label));

            if let (Some(_), Some(binding)) = (source, binding.as_ref())
                && let Some(metrics) = pending
                    .prepared
                    .get(owner)
                    .filter(|prepared| prepared.matches(binding))
                    .map(PreparedFlowchartSvgLabel::metrics)
            {
                #[cfg(test)]
                self.prepared_hits
                    .set(self.prepared_hits.get().saturating_add(1));
                return metrics;
            }

            source.map(|source| {
                let wrapped_lines = source.source.wrapped_lines(
                    request.measurer,
                    request.style,
                    request.max_width_px,
                    break_long_words,
                );
                let metrics = source.source.metrics_from_wrapped(
                    request.measurer,
                    metrics_style,
                    &wrapped_lines,
                    width_mode,
                );
                (wrapped_lines, metrics)
            })
        };

        let (new_source, wrapped_lines, metrics) = measured_from_existing_source.map_or_else(
            || {
                let source = FlowchartSvgLabelSourceEntry::new(request.raw_label);
                let wrapped_lines = source.source.wrapped_lines(
                    request.measurer,
                    request.style,
                    request.max_width_px,
                    break_long_words,
                );
                let metrics = source.source.metrics_from_wrapped(
                    request.measurer,
                    metrics_style,
                    &wrapped_lines,
                    width_mode,
                );
                (Some(source), wrapped_lines, metrics)
            },
            |(wrapped_lines, metrics)| (None, wrapped_lines, metrics),
        );

        let mut pending = self.pending.borrow_mut();
        pending.render_ids.insert(owner, render_id.into());
        if let Some(source) = new_source {
            pending.sources.insert(owner, source);
            pending.prepared.remove(owner);
        }
        if let Some(binding) = binding {
            pending.prepared.insert(
                owner,
                PreparedFlowchartSvgLabel {
                    binding: binding.into_owned(),
                    wrapped_lines,
                    metrics,
                },
            );
        } else {
            pending.prepared.remove(owner);
        }
        metrics
    }

    pub(crate) fn finish(self) -> FlowchartSvgLabelSidecar {
        let pending = self.pending.into_inner();
        FlowchartSvgLabelSidecar::new(pending.sources, pending.prepared, pending.render_ids)
    }

    #[cfg(test)]
    pub(crate) fn prepared_count(&self) -> usize {
        self.pending.borrow().prepared.len()
    }

    #[cfg(test)]
    pub(crate) fn prepared_hit_count(&self) -> usize {
        self.prepared_hits.get()
    }
}

pub(crate) fn measure_flowchart_svg_label_for_layout(
    sidecar: Option<&FlowchartSvgLabelSidecarBuilder>,
    owner: Option<FlowchartSvgLabelOwner>,
    render_id: Option<&str>,
    request: FlowchartLabelMetricsRequest<'_>,
    width_mode: FlowchartSvgWidthMode,
) -> TextMetrics {
    match sidecar.zip(owner).zip(render_id) {
        Some(((sidecar, owner), render_id)) => {
            sidecar.measure_for_layout(owner, render_id, request, true, width_mode)
        }
        None => measure_svg_label_without_sidecar(request, width_mode),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn measure_flowchart_svg_label_for_layout_with_metrics_style(
    sidecar: Option<&FlowchartSvgLabelSidecarBuilder>,
    owner: Option<FlowchartSvgLabelOwner>,
    render_id: Option<&str>,
    request: FlowchartLabelMetricsRequest<'_>,
    metrics_style: &TextStyle,
    width_mode: FlowchartSvgWidthMode,
) -> TextMetrics {
    match sidecar.zip(owner).zip(render_id) {
        Some(((sidecar, owner), render_id)) => sidecar.measure_for_layout_with_metrics_style(
            owner,
            render_id,
            request,
            metrics_style,
            true,
            width_mode,
        ),
        None => {
            measure_svg_label_without_sidecar_with_metrics_style(request, metrics_style, width_mode)
        }
    }
}

fn supports_svg_source_preparation(request: &FlowchartLabelMetricsRequest<'_>) -> bool {
    request.wrap_mode == WrapMode::SvgLike && request.label_type != "markdown"
}

fn measure_svg_label_without_sidecar(
    request: FlowchartLabelMetricsRequest<'_>,
    width_mode: FlowchartSvgWidthMode,
) -> TextMetrics {
    let metrics_style = request.style;
    measure_svg_label_without_sidecar_with_metrics_style(request, metrics_style, width_mode)
}

fn measure_svg_label_without_sidecar_with_metrics_style(
    request: FlowchartLabelMetricsRequest<'_>,
    metrics_style: &TextStyle,
    width_mode: FlowchartSvgWidthMode,
) -> TextMetrics {
    if !supports_svg_source_preparation(&request) {
        return flowchart_label_metrics_for_layout(FlowchartLabelMetricsRequest {
            style: metrics_style,
            ..request
        });
    }
    let source = FlowchartSvgLabelSource::new(request.raw_label);
    let wrapped_lines =
        source.wrapped_lines(request.measurer, request.style, request.max_width_px, true);
    source.metrics_from_wrapped(request.measurer, metrics_style, &wrapped_lines, width_mode)
}

/// Immutable render-side index. It is private to a single prepared Flowchart family artifact.
#[derive(Debug, Default)]
pub(crate) struct FlowchartSvgLabelSidecar {
    sources: FlowchartSvgLabelSlots<FlowchartSvgLabelSourceEntry>,
    prepared: FlowchartSvgLabelSlots<PreparedFlowchartSvgLabel>,
    node_owner_by_id: FxHashMap<String, FlowchartSvgLabelOwner>,
    empty_subgraph_owner_by_id: FxHashMap<String, FlowchartSvgLabelOwner>,
    edge_owner_by_id: FxHashMap<String, FlowchartSvgLabelOwner>,
    subgraph_title_owner_by_id: FxHashMap<String, FlowchartSvgLabelOwner>,
    swimlane_node_owner_by_id: FxHashMap<String, FlowchartSvgLabelOwner>,
    swimlane_edge_owner_by_id: FxHashMap<String, FlowchartSvgLabelOwner>,
    #[cfg(test)]
    prepared_hits_by_owner: Mutex<FxHashMap<FlowchartSvgLabelOwner, usize>>,
}

impl FlowchartSvgLabelSidecar {
    fn new(
        sources: FlowchartSvgLabelSlots<FlowchartSvgLabelSourceEntry>,
        prepared: FlowchartSvgLabelSlots<PreparedFlowchartSvgLabel>,
        render_ids: FlowchartSvgLabelSlots<Box<str>>,
    ) -> Self {
        let mut sidecar = Self {
            sources,
            prepared,
            ..Self::default()
        };
        render_ids.for_each(|owner, render_id| {
            let render_id = render_id.into_string();
            match owner {
                FlowchartSvgLabelOwner::Node(_) => {
                    insert_last_owner(&mut sidecar.node_owner_by_id, render_id, owner);
                }
                FlowchartSvgLabelOwner::EmptySubgraphNode(_) => {
                    insert_last_owner(&mut sidecar.empty_subgraph_owner_by_id, render_id, owner);
                }
                FlowchartSvgLabelOwner::Edge(_) => {
                    insert_last_owner(&mut sidecar.edge_owner_by_id, render_id, owner);
                }
                FlowchartSvgLabelOwner::SubgraphTitle(_) => {
                    // Parsed models have unique group ids. Preserve the canonical first title
                    // owner for public typed models that explicitly contain duplicate groups.
                    insert_first_owner(&mut sidecar.subgraph_title_owner_by_id, render_id, owner);
                }
                FlowchartSvgLabelOwner::SwimlaneNode(_) => {
                    insert_last_owner(&mut sidecar.swimlane_node_owner_by_id, render_id, owner);
                }
                FlowchartSvgLabelOwner::SwimlaneEdgeLabel(_) => {
                    insert_last_owner(&mut sidecar.swimlane_edge_owner_by_id, render_id, owner);
                }
            }
        });
        sidecar
    }

    pub(crate) fn node_owner(
        &self,
        node_id: &str,
        swimlane: bool,
    ) -> Option<FlowchartSvgLabelOwner> {
        if swimlane {
            self.swimlane_node_owner_by_id.get(node_id).copied()
        } else {
            self.empty_subgraph_owner_by_id
                .get(node_id)
                .or_else(|| self.node_owner_by_id.get(node_id))
                .copied()
        }
    }

    pub(crate) fn edge_owner(
        &self,
        edge_id: &str,
        swimlane: bool,
    ) -> Option<FlowchartSvgLabelOwner> {
        if swimlane {
            self.swimlane_edge_owner_by_id.get(edge_id).copied()
        } else {
            self.edge_owner_by_id.get(edge_id).copied()
        }
    }

    pub(crate) fn subgraph_title_owner(&self, subgraph_id: &str) -> Option<FlowchartSvgLabelOwner> {
        self.subgraph_title_owner_by_id.get(subgraph_id).copied()
    }

    fn prepared(
        &self,
        owner: FlowchartSvgLabelOwner,
        raw_source: &str,
        binding: &FlowchartSvgLabelBindingRequest<'_>,
    ) -> Option<(&FlowchartSvgLabelSource, &PreparedFlowchartSvgLabel)> {
        let source = self
            .sources
            .get(owner)
            .filter(|source| source.matches(raw_source));
        let prepared = source.map(|source| &source.source).zip(
            self.prepared
                .get(owner)
                .filter(|prepared| prepared.matches(binding)),
        );
        #[cfg(test)]
        if prepared.is_some() {
            let mut hits = self
                .prepared_hits_by_owner
                .lock()
                .expect("prepared-hit test observer lock");
            let owner_hits = hits.entry(owner).or_default();
            *owner_hits = owner_hits.saturating_add(1);
        }
        prepared
    }

    fn source(
        &self,
        owner: FlowchartSvgLabelOwner,
        raw_source: &str,
    ) -> Option<&FlowchartSvgLabelSource> {
        self.sources
            .get(owner)
            .filter(|source| source.matches(raw_source))
            .map(|source| &source.source)
    }

    #[cfg(test)]
    pub(crate) fn prepared_hit_count(&self, owner: FlowchartSvgLabelOwner) -> usize {
        self.prepared_hits_by_owner
            .lock()
            .expect("prepared-hit test observer lock")
            .get(&owner)
            .copied()
            .unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepared_metrics(
        &self,
        owner: FlowchartSvgLabelOwner,
        raw_source: &str,
        measurer: &dyn TextMeasurer,
        style: &TextStyle,
        max_width_px: Option<f64>,
        break_long_words: bool,
        width_mode: FlowchartSvgWidthMode,
    ) -> Option<TextMetrics> {
        let binding = FlowchartSvgLabelBindingRequest::for_measurer(
            measurer,
            style,
            style,
            max_width_px,
            break_long_words,
            width_mode,
        )?;
        self.prepared(owner, raw_source, &binding)
            .map(|(_, prepared)| prepared.metrics())
    }
}

fn insert_last_owner(
    index: &mut FxHashMap<String, FlowchartSvgLabelOwner>,
    id: String,
    owner: FlowchartSvgLabelOwner,
) {
    match index.get(id.as_str()).copied() {
        Some(current) if current.semantic_index() > owner.semantic_index() => {}
        _ => {
            index.insert(id, owner);
        }
    }
}

fn insert_first_owner(
    index: &mut FxHashMap<String, FlowchartSvgLabelOwner>,
    id: String,
    owner: FlowchartSvgLabelOwner,
) {
    index.entry(id).or_insert(owner);
}

pub(crate) enum FlowchartSvgLabelRenderPlan<'a> {
    Prepared {
        source: &'a FlowchartSvgLabelSource,
        measured: &'a PreparedFlowchartSvgLabel,
    },
    Source {
        source: Cow<'a, FlowchartSvgLabelSource>,
        measurer: &'a dyn TextMeasurer,
        style: &'a TextStyle,
        max_width_px: Option<f64>,
        break_long_words: bool,
    },
}

impl<'a> FlowchartSvgLabelRenderPlan<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        sidecar: Option<&'a FlowchartSvgLabelSidecar>,
        owner: Option<FlowchartSvgLabelOwner>,
        raw_source: &str,
        measurer: &'a dyn TextMeasurer,
        style: &'a TextStyle,
        max_width_px: Option<f64>,
        break_long_words: bool,
        width_mode: FlowchartSvgWidthMode,
    ) -> Self {
        Self::new_with_metrics_style(
            sidecar,
            owner,
            raw_source,
            measurer,
            style,
            style,
            max_width_px,
            break_long_words,
            width_mode,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_metrics_style(
        sidecar: Option<&'a FlowchartSvgLabelSidecar>,
        owner: Option<FlowchartSvgLabelOwner>,
        raw_source: &str,
        measurer: &'a dyn TextMeasurer,
        wrap_style: &'a TextStyle,
        metrics_style: &TextStyle,
        max_width_px: Option<f64>,
        break_long_words: bool,
        width_mode: FlowchartSvgWidthMode,
    ) -> Self {
        let binding = FlowchartSvgLabelBindingRequest::for_measurer(
            measurer,
            wrap_style,
            metrics_style,
            max_width_px,
            break_long_words,
            width_mode,
        );
        if let Some((source, measured)) = sidecar
            .zip(owner)
            .zip(binding.as_ref())
            .and_then(|((sidecar, owner), binding)| sidecar.prepared(owner, raw_source, binding))
        {
            Self::Prepared { source, measured }
        } else {
            let source = sidecar
                .zip(owner)
                .and_then(|(sidecar, owner)| sidecar.source(owner, raw_source))
                .map_or_else(
                    || Cow::Owned(FlowchartSvgLabelSource::new(raw_source)),
                    Cow::Borrowed,
                );
            Self::Source {
                source,
                measurer,
                style: wrap_style,
                max_width_px,
                break_long_words,
            }
        }
    }

    pub(crate) fn plain_text(&self) -> &str {
        match self {
            Self::Prepared { source, .. } => source.plain_text(),
            Self::Source { source, .. } => source.plain_text(),
        }
    }

    pub(crate) fn wrapped_lines(&self) -> Cow<'_, [Vec<String>]> {
        match self {
            Self::Prepared { measured, .. } => Cow::Borrowed(measured.wrapped_lines()),
            Self::Source {
                source,
                measurer,
                style,
                max_width_px,
                break_long_words,
            } => {
                Cow::Owned(source.wrapped_lines(*measurer, style, *max_width_px, *break_long_words))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use crate::environment::{RenderEnvironment, TextMeasurementPhase};
    use crate::text::{TextMeasurer, TextMetrics, TextStyle, WrapMode};
    use merman_core::MermaidConfig;

    use super::*;

    fn report_call_count(session: &crate::environment::RenderSession) -> u64 {
        session
            .text_measurement_report()
            .entries()
            .iter()
            .map(crate::environment::TextMeasurementSummary::count)
            .sum()
    }

    #[derive(Debug, Clone, PartialEq)]
    enum OpaqueMeasurementCall {
        Measure {
            text: String,
            font_size_bits: u64,
        },
        ComputedLength {
            text: String,
            font_size_bits: u64,
        },
        Wrapped {
            text: String,
            font_size_bits: u64,
            max_width_bits: Option<u64>,
            wrap_mode: WrapMode,
        },
    }

    struct StatefulOpaqueTraceMeasurer {
        calls: RefCell<Vec<OpaqueMeasurementCall>>,
    }

    impl StatefulOpaqueTraceMeasurer {
        fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
            }
        }

        fn stateful_width(&self, text: &str, style: &TextStyle) -> f64 {
            text.chars().count() as f64 * style.font_size * 0.5
                + self.calls.borrow().len() as f64 * 0.125
        }

        fn snapshot(&self) -> Vec<OpaqueMeasurementCall> {
            self.calls.borrow().clone()
        }
    }

    impl TextMeasurer for StatefulOpaqueTraceMeasurer {
        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            self.calls
                .borrow_mut()
                .push(OpaqueMeasurementCall::Measure {
                    text: text.to_string(),
                    font_size_bits: style.font_size.to_bits(),
                });
            TextMetrics {
                width: self.stateful_width(text, style),
                height: style.font_size,
                line_count: 1,
            }
        }

        fn measure_svg_text_computed_length_px(&self, text: &str, style: &TextStyle) -> f64 {
            self.calls
                .borrow_mut()
                .push(OpaqueMeasurementCall::ComputedLength {
                    text: text.to_string(),
                    font_size_bits: style.font_size.to_bits(),
                });
            self.stateful_width(text, style)
        }

        fn measure_wrapped(
            &self,
            text: &str,
            style: &TextStyle,
            max_width: Option<f64>,
            wrap_mode: WrapMode,
        ) -> TextMetrics {
            self.calls
                .borrow_mut()
                .push(OpaqueMeasurementCall::Wrapped {
                    text: text.to_string(),
                    font_size_bits: style.font_size.to_bits(),
                    max_width_bits: max_width.map(f64::to_bits),
                    wrap_mode,
                });
            let width = self.stateful_width(text, style);
            TextMetrics {
                width: max_width.map_or(width, |max_width| width.min(max_width)),
                height: text.lines().count().max(1) as f64 * style.font_size,
                line_count: text.lines().count().max(1),
            }
        }
    }

    fn opaque_roundtrip_trace(
        with_sidecar: bool,
    ) -> (TextMetrics, Vec<Vec<String>>, Vec<OpaqueMeasurementCall>) {
        let measurer = StatefulOpaqueTraceMeasurer::new();
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = with_sidecar.then(FlowchartSvgLabelSidecarBuilder::default);
        let owner = FlowchartSvgLabelOwner::Node(0);
        let raw_label = "alpha $$x$$ beta gamma delta";
        let metrics = measure_flowchart_svg_label_for_layout(
            builder.as_ref(),
            with_sidecar.then_some(owner),
            with_sidecar.then_some("node"),
            FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label,
                label_type: "text",
                style: &style,
                max_width_px: Some(96.0),
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: Some(&crate::math::NoopMathRenderer),
            },
            FlowchartSvgWidthMode::ComputedLength,
        );

        let sidecar = builder.map(FlowchartSvgLabelSidecarBuilder::finish);
        let resolved_owner = sidecar
            .as_ref()
            .and_then(|sidecar| sidecar.node_owner("node", false));
        let plan = FlowchartSvgLabelRenderPlan::new(
            sidecar.as_ref(),
            resolved_owner,
            raw_label,
            &measurer,
            &style,
            Some(96.0),
            true,
            FlowchartSvgWidthMode::ComputedLength,
        );
        let wrapped_lines = plan.wrapped_lines().into_owned();
        (metrics, wrapped_lines, measurer.snapshot())
    }

    #[test]
    fn opaque_stateful_trace_is_identical_with_and_without_the_sidecar() {
        let (control_metrics, control_lines, control_trace) = opaque_roundtrip_trace(false);
        let (sidecar_metrics, sidecar_lines, sidecar_trace) = opaque_roundtrip_trace(true);

        assert_eq!(
            sidecar_metrics.width.to_bits(),
            control_metrics.width.to_bits()
        );
        assert_eq!(
            sidecar_metrics.height.to_bits(),
            control_metrics.height.to_bits()
        );
        assert_eq!(sidecar_metrics.line_count, control_metrics.line_count);
        assert_eq!(sidecar_lines, control_lines);
        assert_eq!(sidecar_trace, control_trace);
    }

    #[test]
    fn routed_builtin_measurement_is_reused_during_svg_emission() {
        let environment = RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let layout_measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let owner = FlowchartSvgLabelOwner::SubgraphTitle(0);

        let metrics = builder.measure_for_layout(
            owner,
            "group",
            FlowchartLabelMetricsRequest {
                measurer: &layout_measurer,
                raw_label: "alpha beta gamma",
                label_type: "text",
                style: &style,
                max_width_px: Some(60.0),
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: None,
            },
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        assert!(metrics.width > 0.0);
        assert_eq!(builder.prepared_count(), 1);

        let sidecar = builder.finish();
        let resolved_owner = sidecar.subgraph_title_owner("group");
        assert_eq!(resolved_owner, Some(owner));
        let calls_before_render = report_call_count(&session);
        let render_measurer = session.text_measurer(TextMeasurementPhase::SvgBBox);
        let plan = FlowchartSvgLabelRenderPlan::new(
            Some(&sidecar),
            resolved_owner,
            "alpha beta gamma",
            &render_measurer,
            &style,
            Some(60.0),
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        assert!(matches!(plan, FlowchartSvgLabelRenderPlan::Prepared { .. }));
        assert!(!plan.wrapped_lines().is_empty());
        assert_eq!(report_call_count(&session), calls_before_render);

        let reused_metrics = sidecar
            .prepared_metrics(
                owner,
                "alpha beta gamma",
                &render_measurer,
                &style,
                Some(60.0),
                true,
                FlowchartSvgWidthMode::Bbox,
            )
            .expect("prepared metrics");
        assert_eq!(reused_metrics.width.to_bits(), metrics.width.to_bits());
        assert_eq!(reused_metrics.height.to_bits(), metrics.height.to_bits());
        assert_eq!(reused_metrics.line_count, metrics.line_count);
        assert_eq!(report_call_count(&session), calls_before_render);
    }

    #[test]
    fn svg_math_source_keeps_computed_length_preparation() {
        let environment = RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let owner = FlowchartSvgLabelOwner::Node(0);

        builder.measure_for_layout(
            owner,
            "node",
            FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label: "left $$x$$ right",
                label_type: "text",
                style: &style,
                max_width_px: Some(120.0),
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: Some(&crate::math::NoopMathRenderer),
            },
            true,
            FlowchartSvgWidthMode::ComputedLength,
        );
        assert_eq!(builder.prepared_count(), 1);

        let sidecar = builder.finish();
        assert!(matches!(
            FlowchartSvgLabelRenderPlan::new(
                Some(&sidecar),
                Some(owner),
                "left $$x$$ right",
                &measurer,
                &style,
                Some(120.0),
                true,
                FlowchartSvgWidthMode::ComputedLength,
            ),
            FlowchartSvgLabelRenderPlan::Prepared { .. }
        ));
        assert!(matches!(
            FlowchartSvgLabelRenderPlan::new(
                Some(&sidecar),
                Some(owner),
                "left $$x$$ right",
                &measurer,
                &style,
                Some(120.0),
                true,
                FlowchartSvgWidthMode::Bbox,
            ),
            FlowchartSvgLabelRenderPlan::Source { .. }
        ));
    }

    #[test]
    fn prepared_measurement_requires_an_exact_parameter_binding() {
        let environment = RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let owner = FlowchartSvgLabelOwner::SubgraphTitle(0);
        builder.measure_for_layout(
            owner,
            "group",
            FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label: "bound label",
                label_type: "text",
                style: &style,
                max_width_px: Some(80.0),
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: None,
            },
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        let sidecar = builder.finish();

        let width_mismatch = FlowchartSvgLabelRenderPlan::new(
            Some(&sidecar),
            Some(owner),
            "bound label",
            &measurer,
            &style,
            Some(81.0),
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        assert!(matches!(
            width_mismatch,
            FlowchartSvgLabelRenderPlan::Source { .. }
        ));

        let mut different_style = style.clone();
        different_style.font_size += 1.0;
        let style_mismatch = FlowchartSvgLabelRenderPlan::new(
            Some(&sidecar),
            Some(owner),
            "bound label",
            &measurer,
            &different_style,
            Some(80.0),
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        assert!(matches!(
            style_mismatch,
            FlowchartSvgLabelRenderPlan::Source { .. }
        ));

        let mode_mismatch = FlowchartSvgLabelRenderPlan::new(
            Some(&sidecar),
            Some(owner),
            "bound label",
            &measurer,
            &style,
            Some(80.0),
            true,
            FlowchartSvgWidthMode::ComputedLength,
        );
        assert!(matches!(
            mode_mismatch,
            FlowchartSvgLabelRenderPlan::Source { .. }
        ));
    }

    struct StreamingOpaqueMeasurer {
        wrapped_calls: Cell<usize>,
    }

    impl TextMeasurer for StreamingOpaqueMeasurer {
        #[allow(private_interfaces)]
        fn begin_svg_text_computed_length(
            &self,
            style: &TextStyle,
        ) -> Option<crate::environment::BuiltinSvgComputedLength> {
            Some(crate::environment::BuiltinSvgComputedLength::deterministic(
                style,
            ))
        }

        fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: text.chars().count() as f64 * style.font_size * 0.5,
                height: style.font_size,
                line_count: 1,
            }
        }

        fn measure_wrapped(
            &self,
            text: &str,
            style: &TextStyle,
            _max_width: Option<f64>,
            _wrap_mode: WrapMode,
        ) -> TextMetrics {
            self.wrapped_calls.set(self.wrapped_calls.get() + 1);
            self.measure(text, style)
        }
    }

    #[test]
    fn streaming_state_without_builtin_carriers_is_not_cached() {
        let measurer = StreamingOpaqueMeasurer {
            wrapped_calls: Cell::new(0),
        };
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let owner = FlowchartSvgLabelOwner::Node(0);
        builder.measure_for_layout(
            owner,
            "node",
            FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label: "opaque callback contract",
                label_type: "text",
                style: &style,
                max_width_px: Some(100.0),
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: None,
            },
            true,
            FlowchartSvgWidthMode::Bbox,
        );

        assert!(measurer.wrapped_calls.get() > 0);
        assert_eq!(builder.prepared_count(), 0);
        let sidecar = builder.finish();
        let calls_before_render = measurer.wrapped_calls.get();
        let plan = FlowchartSvgLabelRenderPlan::new(
            Some(&sidecar),
            Some(owner),
            "opaque callback contract",
            &measurer,
            &style,
            Some(100.0),
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        assert!(matches!(
            plan,
            FlowchartSvgLabelRenderPlan::Source {
                source: Cow::Borrowed(_),
                ..
            }
        ));
        let _ = plan.wrapped_lines();
        assert_eq!(
            measurer.wrapped_calls.get(),
            calls_before_render,
            "the computed-length carrier owns this route; no opaque wrapped callback is expected"
        );
    }

    #[test]
    fn duplicate_subgraph_ids_resolve_to_the_first_semantic_owner() {
        let environment = RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();

        for (index, title) in ["first", "second"].into_iter().enumerate() {
            builder.measure_for_layout(
                FlowchartSvgLabelOwner::SubgraphTitle(index),
                "dup",
                FlowchartLabelMetricsRequest {
                    measurer: &measurer,
                    raw_label: title,
                    label_type: "text",
                    style: &style,
                    max_width_px: None,
                    wrap_mode: WrapMode::SvgLike,
                    config: &config,
                    math_renderer: None,
                },
                true,
                FlowchartSvgWidthMode::Bbox,
            );
        }

        let sidecar = builder.finish();
        assert_eq!(
            sidecar.subgraph_title_owner("dup"),
            Some(FlowchartSvgLabelOwner::SubgraphTitle(0))
        );
    }

    #[test]
    fn self_loop_render_id_resolves_to_the_original_edge_owner() {
        let environment = RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let owner = FlowchartSvgLabelOwner::Edge(3);

        builder.measure_for_layout(
            owner,
            "L_A_A_0",
            FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label: "self loop label",
                label_type: "text",
                style: &style,
                max_width_px: Some(200.0),
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: None,
            },
            true,
            FlowchartSvgWidthMode::Bbox,
        );

        assert_eq!(builder.finish().edge_owner("L_A_A_0", false), Some(owner));
    }

    #[test]
    fn sparse_semantic_indices_retain_only_real_label_entries() {
        let environment = RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let owner = FlowchartSvgLabelOwner::Edge(100_000);

        builder.measure_for_layout(
            owner,
            "sparse-edge",
            FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label: "only the final semantic edge has a label",
                label_type: "text",
                style: &style,
                max_width_px: Some(120.0),
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: None,
            },
            true,
            FlowchartSvgWidthMode::Bbox,
        );

        {
            let pending = builder.pending.borrow();
            assert_eq!(pending.sources.edges.entries.len(), 1);
            assert_eq!(pending.prepared.edges.entries.len(), 1);
            assert_eq!(pending.render_ids.edges.entries.len(), 1);
            assert_eq!(pending.prepared.len(), 1);
        }

        let sidecar = builder.finish();
        assert_eq!(sidecar.sources.edges.entries.len(), 1);
        assert_eq!(sidecar.prepared.edges.entries.len(), 1);
        assert_eq!(sidecar.edge_owner("sparse-edge", false), Some(owner));
    }

    #[test]
    fn edge_preparation_wraps_with_base_style_and_measures_with_final_style() {
        let environment = RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let config = MermaidConfig::default();
        let wrap_style = TextStyle::default();
        let mut metrics_style = wrap_style.clone();
        metrics_style.font_size *= 2.0;
        let raw_label = "alpha beta gamma delta epsilon zeta eta theta";
        let max_width = Some(96.0);
        let source = FlowchartSvgLabelSource::new(raw_label);
        let expected_lines = source.wrapped_lines(&measurer, &wrap_style, max_width, true);
        let prematurely_styled_lines =
            source.wrapped_lines(&measurer, &metrics_style, max_width, true);
        assert_ne!(expected_lines, prematurely_styled_lines);
        let expected_metrics = source.metrics_from_wrapped(
            &measurer,
            &metrics_style,
            &expected_lines,
            FlowchartSvgWidthMode::Bbox,
        );

        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let owner = FlowchartSvgLabelOwner::Edge(0);
        let metrics = builder.measure_for_layout_with_metrics_style(
            owner,
            "edge",
            FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label,
                label_type: "text",
                style: &wrap_style,
                max_width_px: max_width,
                wrap_mode: WrapMode::SvgLike,
                config: &config,
                math_renderer: None,
            },
            &metrics_style,
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        assert_eq!(metrics.width.to_bits(), expected_metrics.width.to_bits());
        assert_eq!(metrics.height.to_bits(), expected_metrics.height.to_bits());
        assert_eq!(metrics.line_count, expected_metrics.line_count);

        let sidecar = builder.finish();
        let plan = FlowchartSvgLabelRenderPlan::new_with_metrics_style(
            Some(&sidecar),
            Some(owner),
            raw_label,
            &measurer,
            &wrap_style,
            &metrics_style,
            max_width,
            true,
            FlowchartSvgWidthMode::Bbox,
        );
        assert!(matches!(plan, FlowchartSvgLabelRenderPlan::Prepared { .. }));
        assert_eq!(plan.wrapped_lines().as_ref(), expected_lines.as_slice());
    }
}
