// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE file in the project root for license information.

use crate::native_collections::OrderedMap as IndexMap;
use crate::native_collections::{
    self as memory, NativeHashMap as FxHashMap, NativeHashSet as FxHashSet,
};
use crate::{DiagramWarningFact, SourceSpan};
use serde::{Deserialize, Deserializer, Serialize};

mod native_memory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowchartModel {
    #[serde(default)]
    pub keyword: String,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "classDefs")]
    pub class_defs: IndexMap<String, Vec<String>>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default, rename = "edgeDefaults")]
    pub edge_defaults: Option<FlowEdgeDefaults>,
    #[serde(default, rename = "vertexCalls")]
    pub vertex_calls: Vec<String>,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    #[serde(default)]
    pub subgraphs: Vec<FlowSubgraph>,
    #[serde(default)]
    pub tooltips: FxHashMap<String, String>,
    #[serde(
        default,
        rename = "warningFacts",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub warning_facts: Vec<DiagramWarningFact>,
}

impl FlowchartModel {
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FlowchartRenderLabelSources {
    nodes: FxHashMap<String, String>,
    edges: FxHashMap<String, String>,
    subgraphs: Vec<Option<FlowSubgraphRenderLabelSource>>,
}

#[derive(Debug, Clone)]
struct FlowSubgraphRenderLabelSource {
    id: String,
    source: String,
}

impl FlowchartRenderLabelSources {
    pub(crate) fn new_controlled(control: &crate::OperationControl) -> Self {
        Self {
            nodes: memory::new_map(control),
            edges: memory::new_map(control),
            subgraphs: Vec::new(),
        }
    }
    pub(crate) fn node_label_for_render<'a>(&'a self, node: &'a FlowNode) -> Option<&'a str> {
        self.nodes
            .get(&node.id)
            .map(String::as_str)
            .or(node.label.as_deref())
    }

    pub(crate) fn edge_label_for_render<'a>(&'a self, edge: &'a FlowEdge) -> Option<&'a str> {
        self.edges
            .get(&edge.id)
            .map(String::as_str)
            .or(edge.label.as_deref())
    }

    pub(crate) fn subgraph_title_for_render<'a>(
        &'a self,
        declaration_ordinal: usize,
        subgraph: &'a FlowSubgraph,
    ) -> &'a str {
        self.subgraphs
            .get(declaration_ordinal)
            .and_then(Option::as_ref)
            .filter(|source| source.id == subgraph.id)
            .map(|source| source.source.as_str())
            .unwrap_or(subgraph.title.as_str())
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn insert_node(
        &mut self,
        id: String,
        source: String,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        memory::insert(&mut self.nodes, id, source, control)?;
        Ok(())
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn insert_edge(
        &mut self,
        id: String,
        source: String,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        memory::insert(&mut self.edges, id, source, control)?;
        Ok(())
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn insert_subgraph(
        &mut self,
        id: String,
        declaration_ordinal: usize,
        source: Option<String>,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        if let Some(source) = source {
            if self.subgraphs.len() <= declaration_ordinal {
                let additional = declaration_ordinal
                    .checked_add(1)
                    .and_then(|length| length.checked_sub(self.subgraphs.len()))
                    .ok_or_else(|| {
                        memory::failure(control, markdown_memory::MemoryError::ArithmeticOverflow)
                    })?;
                memory::reserve(&mut self.subgraphs, additional, control)?;
                self.subgraphs.resize_with(declaration_ordinal + 1, || None);
            }
            self.subgraphs[declaration_ordinal] =
                Some(FlowSubgraphRenderLabelSource { id, source });
        }
        Ok(())
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.nodes
            .iter()
            .chain(&self.edges)
            .fold(0usize, |total, (id, label)| {
                total.saturating_add(id.len()).saturating_add(label.len())
            })
            .saturating_add(
                self.subgraphs
                    .iter()
                    .flatten()
                    .fold(0usize, |total, source| {
                        total
                            .saturating_add(source.id.len())
                            .saturating_add(source.source.len())
                    }),
            )
    }
}

/// Parser-owned CSS provenance used only while rendering a Flowchart model.
#[derive(Debug, Clone, Default)]
pub(crate) struct FlowchartRenderStyleSources {
    subgraph_vertices: IndexMap<String, FlowSubgraphVertexStyleSource>,
}

impl FlowchartRenderStyleSources {
    pub(crate) fn new_controlled(control: &crate::OperationControl) -> Self {
        Self {
            subgraph_vertices: control
                .memory_region()
                .map(|region| IndexMap::new_in(region.clone()))
                .unwrap_or_default(),
        }
    }
    /// Returns the CSS sources exposed by Mermaid for the rendered subgraph node.
    pub(crate) fn effective_subgraph_css<'a>(
        &'a self,
        declaration_ordinal: usize,
        subgraph: &'a FlowSubgraph,
    ) -> (&'a [String], &'a [String]) {
        self.subgraph_vertices
            .get(&subgraph.id)
            .filter(|source| source.declaration_ordinal == declaration_ordinal)
            .map_or_else(
                || (subgraph.classes.as_slice(), subgraph.styles.as_slice()),
                |source| {
                    (
                        source.style.classes.as_slice(),
                        source.style.styles.as_slice(),
                    )
                },
            )
    }

    /// Returns whether parsing observed a FlowDB vertex with this subgraph ID.
    pub(crate) fn contains_subgraph_vertex(&self, id: &str) -> bool {
        self.subgraph_vertices.contains_key(id)
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn insert(
        &mut self,
        id: String,
        declaration_ordinal: usize,
        style: FlowSubgraphVertexStyle,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        memory::result(
            self.subgraph_vertices.try_insert(
                id,
                FlowSubgraphVertexStyleSource {
                    declaration_ordinal,
                    style,
                },
                control,
            ),
            control,
        )?;
        Ok(())
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.subgraph_vertices
            .iter()
            .fold(0usize, |total, (id, source)| {
                source
                    .style
                    .classes
                    .iter()
                    .chain(&source.style.styles)
                    .fold(total.saturating_add(id.len()), |subtotal, value| {
                        subtotal.saturating_add(value.len())
                    })
            })
    }
}

/// Parser-owned render facts that are intentionally absent from [`FlowchartModel`].
#[doc(hidden)]
#[derive(Debug, Clone, Default)]
pub struct FlowchartRenderContext {
    labels: FlowchartRenderLabelSources,
    styles: FlowchartRenderStyleSources,
    collapsed_subgraphs: FxHashSet<String>,
    collapsed_replacements: FxHashMap<String, String>,
    subgraph_color_ordinals: FxHashMap<String, usize>,
    node_dom_indices: FxHashMap<String, usize>,
}

impl FlowchartRenderContext {
    #[cfg(feature = "diagram-agentflow")]
    pub(crate) fn new(
        labels: FlowchartRenderLabelSources,
        styles: FlowchartRenderStyleSources,
        collapsed_subgraphs: FxHashSet<String>,
        subgraphs: &[FlowSubgraph],
    ) -> Self {
        Self::new_controlled(
            labels,
            styles,
            collapsed_subgraphs,
            subgraphs,
            &crate::OperationControl::new(),
        )
        .expect("legacy context construction has no scoped cancellation")
    }

    pub(crate) fn new_controlled(
        labels: FlowchartRenderLabelSources,
        styles: FlowchartRenderStyleSources,
        collapsed_subgraphs: FxHashSet<String>,
        subgraphs: &[FlowSubgraph],
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Self> {
        let collapsed_replacements =
            build_collapsed_replacements(&collapsed_subgraphs, subgraphs, control)?;
        Ok(Self {
            labels,
            styles,
            collapsed_subgraphs,
            collapsed_replacements,
            subgraph_color_ordinals: build_subgraph_color_ordinals(subgraphs, control)?,
            node_dom_indices: memory::new_map(control),
        })
    }

    #[cfg(feature = "diagram-agentflow")]
    pub(crate) fn set_node_dom_indices(
        &mut self,
        indices: impl IntoIterator<Item = (String, usize)>,
    ) {
        self.node_dom_indices = indices.into_iter().collect();
    }

    /// Returns a parser-assigned DOM ordinal when the family owns node registration.
    #[doc(hidden)]
    pub fn node_dom_index(&self, id: &str) -> Option<usize> {
        self.node_dom_indices.get(id).copied()
    }

    #[cfg(feature = "diagram-agentflow")]
    pub(crate) fn set_collapsed_replacements(
        &mut self,
        replacements: impl IntoIterator<Item = (String, String)>,
    ) {
        self.collapsed_replacements = replacements.into_iter().collect();
    }

    #[cfg(feature = "diagram-agentflow")]
    pub(crate) fn set_subgraph_color_ordinals(
        &mut self,
        ordinals: impl IntoIterator<Item = (String, usize)>,
    ) {
        self.subgraph_color_ordinals = ordinals.into_iter().collect();
    }

    /// Returns the container's palette ordinal, independent of model storage order.
    #[doc(hidden)]
    pub fn subgraph_color_ordinal(&self, id: &str) -> Option<usize> {
        self.subgraph_color_ordinals.get(id).copied()
    }

    #[doc(hidden)]
    pub fn node_label_for_render<'a>(&'a self, node: &'a FlowNode) -> Option<&'a str> {
        self.labels.node_label_for_render(node)
    }

    #[doc(hidden)]
    pub fn edge_label_for_render<'a>(&'a self, edge: &'a FlowEdge) -> Option<&'a str> {
        self.labels.edge_label_for_render(edge)
    }

    #[doc(hidden)]
    pub fn subgraph_title_for_render<'a>(
        &'a self,
        declaration_ordinal: usize,
        subgraph: &'a FlowSubgraph,
    ) -> &'a str {
        self.labels
            .subgraph_title_for_render(declaration_ordinal, subgraph)
    }

    #[doc(hidden)]
    pub fn effective_subgraph_css<'a>(
        &'a self,
        declaration_ordinal: usize,
        subgraph: &'a FlowSubgraph,
    ) -> (&'a [String], &'a [String]) {
        self.styles
            .effective_subgraph_css(declaration_ordinal, subgraph)
    }

    #[doc(hidden)]
    pub fn contains_subgraph_vertex(&self, id: &str) -> bool {
        self.styles.contains_subgraph_vertex(id)
    }

    /// Returns whether the parser attached Mermaid's collapsed view to this subgraph.
    #[doc(hidden)]
    pub fn is_subgraph_collapsed(&self, id: &str) -> bool {
        self.collapsed_subgraphs.contains(id)
    }

    /// Returns the visible collapsed subgraph that replaces this node, when the node is hidden
    /// beneath a collapsed subgraph. The visible collapsed subgraph itself is not replaced.
    #[doc(hidden)]
    pub fn collapsed_replacement(&self, id: &str) -> Option<&str> {
        self.collapsed_replacements.get(id).map(String::as_str)
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.labels
            .retained_bytes()
            .saturating_add(self.styles.retained_bytes())
            .saturating_add(
                self.collapsed_subgraphs
                    .iter()
                    .map(String::len)
                    .sum::<usize>(),
            )
            .saturating_add(self.collapsed_replacements.iter().fold(
                0usize,
                |total, (id, replacement)| {
                    total
                        .saturating_add(id.len())
                        .saturating_add(replacement.len())
                },
            ))
            .saturating_add(
                self.subgraph_color_ordinals
                    .keys()
                    .chain(self.node_dom_indices.keys())
                    .fold(0usize, |total, id| {
                        total
                            .saturating_add(id.len())
                            .saturating_add(std::mem::size_of::<usize>())
                    }),
            )
    }
}

fn build_subgraph_color_ordinals(
    subgraphs: &[FlowSubgraph],
    control: &crate::OperationControl,
) -> crate::OperationControlResult<FxHashMap<String, usize>> {
    let mut ids = memory::new_set(control);
    for graph in subgraphs {
        memory::insert_set(&mut ids, graph.id.as_str(), control)?;
    }
    let mut parents = memory::new_map(control);
    for graph in subgraphs {
        for child in &graph.nodes {
            if ids.contains(child.as_str()) {
                memory::insert(&mut parents, child.as_str(), graph.id.as_str(), control)?;
            }
        }
    }
    let mut children: FxHashMap<&str, Vec<&str>> = memory::new_map(control);
    for graph in subgraphs {
        if let Some(&parent) = parents.get(graph.id.as_str()) {
            let child_ids = memory::entry_default(&mut children, parent, control)?;
            memory::push(child_ids, graph.id.as_str(), control)?;
        }
    }
    // Jison stores containers when they close. A containment preorder restores declaration
    // order and keeps each slot stable when the same container is rendered collapsed.
    let mut pending = Vec::new();
    for graph in subgraphs
        .iter()
        .rev()
        .filter(|graph| !parents.contains_key(graph.id.as_str()))
    {
        memory::push(&mut pending, graph.id.as_str(), control)?;
    }
    let mut ordinals = memory::new_map(control);
    while let Some(id) = pending.pop() {
        if ordinals.contains_key(id) {
            continue;
        }
        let ordinal = ordinals.len();
        memory::insert(
            &mut ordinals,
            memory::owned_string(id, control)?,
            ordinal,
            control,
        )?;
        if let Some(children) = children.get(id) {
            for child in children.iter().rev() {
                memory::push(&mut pending, *child, control)?;
            }
        }
    }
    Ok(ordinals)
}

fn build_collapsed_replacements(
    collapsed_subgraphs: &FxHashSet<String>,
    subgraphs: &[FlowSubgraph],
    control: &crate::OperationControl,
) -> crate::OperationControlResult<FxHashMap<String, String>> {
    let mut subgraph_ids = memory::new_set(control);
    for graph in subgraphs {
        memory::insert_set(&mut subgraph_ids, graph.id.as_str(), control)?;
    }
    let mut parent_by_id: FxHashMap<&str, &str> = memory::new_map(control);
    for subgraph in subgraphs {
        for child in &subgraph.nodes {
            if subgraph_ids.contains(child.as_str()) {
                memory::insert(
                    &mut parent_by_id,
                    child.as_str(),
                    subgraph.id.as_str(),
                    control,
                )?;
            }
        }
    }

    fn outermost_collapsed<'a>(
        id: &'a str,
        collapsed_subgraphs: &FxHashSet<String>,
        parent_by_id: &FxHashMap<&'a str, &'a str>,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Option<&'a str>> {
        let mut current = Some(id);
        let mut result = None;
        let mut seen: FxHashSet<&str> = memory::new_set(control);
        while let Some(candidate) = current {
            if !memory::insert_set(&mut seen, candidate, control)? {
                break;
            }
            if collapsed_subgraphs.contains(candidate) {
                result = Some(candidate);
            }
            current = parent_by_id.get(candidate).copied();
        }
        Ok(result)
    }

    let mut replacements = memory::new_map(control);
    for subgraph in subgraphs {
        let Some(ancestor) = outermost_collapsed(
            subgraph.id.as_str(),
            collapsed_subgraphs,
            &parent_by_id,
            control,
        )?
        else {
            continue;
        };
        if subgraph.id != ancestor {
            memory::insert(
                &mut replacements,
                memory::clone_owned(&subgraph.id, control)?,
                memory::owned_string(ancestor, control)?,
                control,
            )?;
        }
        for child in &subgraph.nodes {
            if child != ancestor {
                memory::insert(
                    &mut replacements,
                    memory::clone_owned(child, control)?,
                    memory::owned_string(ancestor, control)?,
                    control,
                )?;
            }
        }
    }
    Ok(replacements)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEdgeDefaults {
    #[serde(default)]
    pub interpolate: Option<String>,
    #[serde(default)]
    pub style: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    /// Records whether this node was authored by the user or synthesized solely as a subgraph
    /// routing endpoint.
    ///
    /// Renderers must use this fact instead of inferring provenance from the node's visible
    /// fields. A bare authored node can otherwise be indistinguishable from an endpoint anchor.
    #[serde(default, skip_serializing_if = "FlowNodeProvenance::is_authored")]
    pub provenance: FlowNodeProvenance,
    pub label: Option<String>,
    #[serde(default, rename = "labelType")]
    pub label_type: Option<String>,
    #[serde(rename = "layoutShape")]
    pub layout_shape: Option<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub form: Option<String>,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub img: Option<String>,
    #[serde(default)]
    pub constraint: Option<String>,
    #[serde(default, rename = "assetWidth")]
    pub asset_width: Option<f64>,
    #[serde(default, rename = "assetHeight")]
    pub asset_height: Option<f64>,
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default, rename = "linkTarget")]
    pub link_target: Option<String>,
    #[serde(default, rename = "haveCallback")]
    pub have_callback: bool,
}

impl FlowNode {
    pub fn is_subgraph_anchor(&self) -> bool {
        matches!(self.provenance, FlowNodeProvenance::SubgraphAnchor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FlowNodeProvenance {
    #[default]
    Authored,
    SubgraphAnchor,
}

impl FlowNodeProvenance {
    fn is_authored(&self) -> bool {
        matches!(self, Self::Authored)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
/// Marker attached to one semantic endpoint of a Flowchart edge.
pub enum FlowEdgeMarker {
    #[default]
    None,
    Point,
    Circle,
    Cross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
/// Visible stroke pattern, independent from whether the edge is painted.
pub enum FlowEdgeStroke {
    #[default]
    Normal,
    Dotted,
    Thick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
/// Whether the edge is painted or retained only as a layout constraint.
pub enum FlowEdgeVisibility {
    #[default]
    Visible,
    Invisible,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    #[serde(default, rename = "labelType")]
    pub label_type: Option<String>,
    #[serde(default, rename = "type")]
    /// Mermaid-compatible aggregate edge type retained for legacy JSON consumers.
    pub edge_type: Option<String>,
    #[serde(default)]
    /// Authored full/end operator retained for compatibility and source inspection.
    pub arrow: String,
    #[serde(default, rename = "startMarker")]
    /// Typed marker owned by the source endpoint.
    pub start_marker: FlowEdgeMarker,
    #[serde(default, rename = "endMarker")]
    /// Typed marker owned by the target endpoint.
    pub end_marker: FlowEdgeMarker,
    #[serde(default, rename = "isUserDefinedId")]
    pub is_user_defined_id: bool,
    #[serde(default)]
    /// Mermaid-compatible stroke string retained for legacy JSON consumers.
    pub stroke: Option<String>,
    #[serde(default, rename = "strokeKind")]
    pub stroke_kind: FlowEdgeStroke,
    #[serde(default)]
    pub visibility: FlowEdgeVisibility,
    #[serde(default)]
    pub interpolate: Option<String>,
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub style: Vec<String>,
    #[serde(default)]
    pub animate: Option<bool>,
    #[serde(default)]
    pub animation: Option<String>,
    pub length: usize,
}

#[derive(Deserialize)]
struct FlowEdgeWire {
    id: String,
    from: String,
    to: String,
    label: Option<String>,
    #[serde(default, rename = "labelType")]
    label_type: Option<String>,
    #[serde(default, rename = "type")]
    edge_type: Option<String>,
    #[serde(default)]
    arrow: String,
    #[serde(default, rename = "startMarker")]
    start_marker: Option<FlowEdgeMarker>,
    #[serde(default, rename = "endMarker")]
    end_marker: Option<FlowEdgeMarker>,
    #[serde(default, rename = "isUserDefinedId")]
    is_user_defined_id: bool,
    #[serde(default)]
    stroke: Option<String>,
    #[serde(default, rename = "strokeKind")]
    stroke_kind: Option<FlowEdgeStroke>,
    #[serde(default)]
    visibility: Option<FlowEdgeVisibility>,
    #[serde(default)]
    interpolate: Option<String>,
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    style: Vec<String>,
    #[serde(default)]
    animate: Option<bool>,
    #[serde(default)]
    animation: Option<String>,
    length: usize,
}

impl<'de> Deserialize<'de> for FlowEdge {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = FlowEdgeWire::deserialize(deserializer)?;
        let label_is_present = wire.label.is_some();
        let (legacy_start_marker, legacy_end_marker) = markers_from_compatibility_fields(
            &wire.arrow,
            wire.edge_type.as_deref(),
            label_is_present,
        );
        let legacy_stroke_kind = stroke_from_compatibility_field(wire.stroke.as_deref());
        let legacy_visibility = visibility_from_compatibility_field(wire.stroke.as_deref());

        Ok(Self {
            id: wire.id,
            from: wire.from,
            to: wire.to,
            label: wire.label,
            label_type: wire.label_type,
            edge_type: wire.edge_type,
            arrow: wire.arrow,
            start_marker: wire.start_marker.unwrap_or(legacy_start_marker),
            end_marker: wire.end_marker.unwrap_or(legacy_end_marker),
            is_user_defined_id: wire.is_user_defined_id,
            stroke: wire.stroke,
            stroke_kind: wire.stroke_kind.unwrap_or(legacy_stroke_kind),
            visibility: wire.visibility.unwrap_or(legacy_visibility),
            interpolate: wire.interpolate,
            classes: wire.classes,
            style: wire.style,
            animate: wire.animate,
            animation: wire.animation,
            length: wire.length,
        })
    }
}

fn markers_from_compatibility_fields(
    arrow: &str,
    edge_type: Option<&str>,
    label_is_present: bool,
) -> (FlowEdgeMarker, FlowEdgeMarker) {
    let arrow = arrow.trim();
    let compatibility_marker = edge_type.and_then(marker_from_compatibility_edge_type);
    let end_marker = compatibility_marker.unwrap_or_else(|| {
        arrow
            .chars()
            .next_back()
            .and_then(marker_from_end_char)
            .unwrap_or_default()
    });
    let start_marker = if edge_type.is_some_and(|edge_type| edge_type.starts_with("double_")) {
        compatibility_marker.unwrap_or_default()
    } else if label_is_present {
        // Mermaid's split-label lexer can leave the label's final `o` or `x` at the beginning of
        // the compatibility `arrow` field (for example `--No-->` becomes `o-->`). Only an
        // unlabeled edge proves that `arrow` contains the complete authored operator.
        FlowEdgeMarker::None
    } else {
        arrow
            .chars()
            .next()
            .and_then(marker_from_start_char)
            .unwrap_or_default()
    };
    (start_marker, end_marker)
}

fn marker_from_start_char(ch: char) -> Option<FlowEdgeMarker> {
    match ch {
        '<' => Some(FlowEdgeMarker::Point),
        'o' => Some(FlowEdgeMarker::Circle),
        'x' => Some(FlowEdgeMarker::Cross),
        _ => None,
    }
}

fn marker_from_end_char(ch: char) -> Option<FlowEdgeMarker> {
    match ch {
        '>' => Some(FlowEdgeMarker::Point),
        'o' => Some(FlowEdgeMarker::Circle),
        'x' => Some(FlowEdgeMarker::Cross),
        _ => None,
    }
}

fn marker_from_compatibility_edge_type(edge_type: &str) -> Option<FlowEdgeMarker> {
    match edge_type.strip_prefix("double_").unwrap_or(edge_type) {
        "arrow" | "arrow_point" => Some(FlowEdgeMarker::Point),
        "arrow_circle" => Some(FlowEdgeMarker::Circle),
        "arrow_cross" => Some(FlowEdgeMarker::Cross),
        "arrow_open" => Some(FlowEdgeMarker::None),
        _ => None,
    }
}

fn stroke_from_compatibility_field(stroke: Option<&str>) -> FlowEdgeStroke {
    match stroke {
        Some("dotted") => FlowEdgeStroke::Dotted,
        Some("thick") => FlowEdgeStroke::Thick,
        _ => FlowEdgeStroke::Normal,
    }
}

fn visibility_from_compatibility_field(stroke: Option<&str>) -> FlowEdgeVisibility {
    if stroke == Some("invisible") {
        FlowEdgeVisibility::Invisible
    } else {
        FlowEdgeVisibility::Visible
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowSubgraph {
    pub id: String,
    pub title: String,
    pub dir: Option<String>,
    // Kept as parser-internal provenance for the ASCII adapter. Mermaid 11.17.2 no longer
    // exposes this rollback-era field in the render model or compatibility JSON.
    #[serde(skip)]
    pub has_explicit_dir: bool,
    #[serde(default, rename = "labelType")]
    pub label_type: Option<String>,
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    pub nodes: Vec<String>,
    /// Opaque subgraph metadata forwarded to layout engines, including `algorithm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FlowSubgraphVertexStyle {
    pub(crate) classes: Vec<String>,
    pub(crate) styles: Vec<String>,
}

#[derive(Debug, Clone)]
struct FlowSubgraphVertexStyleSource {
    declaration_ordinal: usize,
    style: FlowSubgraphVertexStyle,
}

#[derive(Debug, Clone)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct Node {
    pub id: String,
    pub provenance: FlowNodeProvenance,
    pub syntax: FlowNodeSyntax,
    pub id_span: Option<SourceSpan>,
    pub label: Option<String>,
    pub label_type: TitleKind,
    pub label_span: Option<SourceSpan>,
    pub label_selection: Option<SourceSpan>,
    pub shape: Option<String>,
    pub shape_data: Option<String>,
    pub icon: Option<String>,
    pub form: Option<String>,
    pub pos: Option<String>,
    pub img: Option<String>,
    pub constraint: Option<String>,
    pub asset_width: Option<f64>,
    pub asset_height: Option<f64>,
    pub styles: Vec<String>,
    pub classes: Vec<String>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub have_callback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) enum FlowNodeSyntax {
    BareReference,
    ExplicitDefinition,
}

#[derive(Debug, Clone)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct Edge {
    pub from: String,
    pub to: String,
    pub id: Option<String>,
    pub link: LinkToken,
    pub label: Option<String>,
    pub label_type: TitleKind,
    pub label_span: Option<SourceSpan>,
    pub label_selection: Option<SourceSpan>,
    pub style: Vec<String>,
    pub classes: Vec<String>,
    pub interpolate: Option<String>,
    pub is_user_defined_id: bool,
    pub animate: Option<bool>,
    pub animation: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")),
    allow(
        dead_code,
        reason = "Agentflow shares presentation lexing but does not consume Flowchart grammar and recovery payloads."
    )
)]
pub(crate) struct LinkToken {
    pub end: String,
    pub start_marker: FlowEdgeMarker,
    pub end_marker: FlowEdgeMarker,
    pub stroke_kind: FlowEdgeStroke,
    pub visibility: FlowEdgeVisibility,
    pub length: usize,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl LinkToken {
    pub(crate) const fn compatibility_edge_type(&self) -> &'static str {
        match (self.start_marker, self.end_marker) {
            (FlowEdgeMarker::Point, FlowEdgeMarker::Point) => "double_arrow_point",
            (FlowEdgeMarker::Circle, FlowEdgeMarker::Circle) => "double_arrow_circle",
            (FlowEdgeMarker::Cross, FlowEdgeMarker::Cross) => "double_arrow_cross",
            (_, FlowEdgeMarker::Point) => "arrow_point",
            (_, FlowEdgeMarker::Circle) => "arrow_circle",
            (_, FlowEdgeMarker::Cross) => "arrow_cross",
            (_, FlowEdgeMarker::None) => "arrow_open",
        }
    }

    pub(crate) const fn compatibility_stroke(&self) -> &'static str {
        if matches!(self.visibility, FlowEdgeVisibility::Invisible) {
            return "invisible";
        }
        match self.stroke_kind {
            FlowEdgeStroke::Normal => "normal",
            FlowEdgeStroke::Dotted => "dotted",
            FlowEdgeStroke::Thick => "thick",
        }
    }
}

#[derive(Debug, Clone)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct EdgeDefaults {
    pub style: Vec<String>,
    pub interpolate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TitleKind {
    Text,
    String,
    Markdown,
}

#[derive(Debug, Clone)]
pub(crate) struct LabeledText {
    pub text: String,
    pub kind: TitleKind,
    pub span: Option<SourceSpan>,
    pub selection: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubgraphHeader {
    pub raw_id: String,
    pub header_span: Option<SourceSpan>,
    pub raw_id_span: Option<SourceSpan>,
    pub raw_title: String,
    pub title_kind: TitleKind,
    pub id_equals_title: bool,
}

impl Default for SubgraphHeader {
    fn default() -> Self {
        Self {
            raw_id: String::new(),
            header_span: None,
            raw_id_span: None,
            raw_title: String::new(),
            title_kind: TitleKind::Text,
            id_equals_title: true,
        }
    }
}
