#[cfg(any(feature = "diagram-class", feature = "diagram-sequence"))]
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
#[cfg(feature = "diagram-class")]
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn from_points(points: impl IntoIterator<Item = (f64, f64)>) -> Option<Self> {
        let mut it = points.into_iter();
        let (x0, y0) = it.next()?;
        let mut b = Self {
            min_x: x0,
            min_y: y0,
            max_x: x0,
            max_y: y0,
        };
        for (x, y) in it {
            b.min_x = b.min_x.min(x);
            b.min_y = b.min_y.min(y);
            b.max_x = b.max_x.max(x);
            b.max_y = b.max_y.max(y);
        }
        Some(b)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutLabel {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-class")]
pub struct ClassNodeRowMetrics {
    pub members: Vec<crate::text::TextMetrics>,
    pub methods: Vec<crate::text::TextMetrics>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-class")]
pub struct ClassPreparedHtmlLabel {
    pub metrics: crate::text::TextMetrics,
    pub max_width_px: i64,
    pub xhtml: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-class")]
pub struct ClassPreparedHtmlNodeLabels {
    pub title: ClassPreparedHtmlLabel,
    pub annotation: Option<ClassPreparedHtmlLabel>,
    pub members: Vec<ClassPreparedHtmlLabel>,
    pub methods: Vec<ClassPreparedHtmlLabel>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-class")]
pub enum ClassNodeLabelPlan {
    RowMetrics(ClassNodeRowMetrics),
    PreparedHtml(ClassPreparedHtmlNodeLabels),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_cluster: bool,
    #[serde(skip)]
    pub label_width: Option<f64>,
    #[serde(skip)]
    pub label_height: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutCluster {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Mermaid cluster "diff" value used during cluster positioning.
    pub diff: f64,
    /// Mermaid cluster "offsetY" value: title bbox height minus half padding.
    pub offset_y: f64,
    pub title: String,
    pub title_label: LayoutLabel,
    pub requested_dir: Option<String>,
    pub effective_dir: String,
    pub padding: f64,
    pub title_margin_top: f64,
    pub title_margin_bottom: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub from_cluster: Option<String>,
    #[serde(default)]
    pub to_cluster: Option<String>,
    pub points: Vec<LayoutPoint>,
    pub label: Option<LayoutLabel>,
    #[serde(default)]
    pub start_label_left: Option<LayoutLabel>,
    #[serde(default)]
    pub start_label_right: Option<LayoutLabel>,
    #[serde(default)]
    pub end_label_left: Option<LayoutLabel>,
    #[serde(default)]
    pub end_label_right: Option<LayoutLabel>,
    /// Optional SVG marker id for marker-start (e.g. ER cardinality markers).
    #[serde(default)]
    pub start_marker: Option<String>,
    /// Optional SVG marker id for marker-end (e.g. ER cardinality markers).
    #[serde(default)]
    pub end_marker: Option<String>,
    /// Optional SVG dash pattern (e.g. identifying vs non-identifying ER relationships).
    #[serde(default)]
    pub stroke_dasharray: Option<String>,
}

/// Post-paint edge geometry, recovered from the same compute phase that emits the SVG.
///
/// `LayoutEdge::points` is the ELK port polyline *before* the endpoints are clipped to the
/// endpoint shapes' ink. Clipping happens during paint, and consumers that need the geometry that
/// is actually on screen currently have to render a whole SVG and recover it from the
/// `data-points` attribute, which is why this projection exists.
///
/// Coordinates are in the **same space as `LayoutEdge::points`** (layout coordinates), so a
/// consumer can treat this as a refinement of the layout projection rather than a second,
/// unrelated geometry system.
#[derive(Debug, Clone, Serialize)]
pub struct EdgePaintGeometry {
    /// Matches `LayoutEdge::id`.
    pub id: String,
    /// Edge polyline after clipping to the endpoint shapes' ink, and after any swimlane line
    /// hops. This is the exact point list emitted as the `data-points` attribute.
    ///
    /// `None` when the renderer produced no path for this edge (for example an edge with fewer
    /// than two layout points).
    pub points: Option<Vec<LayoutPoint>>,
    /// Center of the edge label after paint-time positioning, in layout coordinates.
    ///
    /// `None` when the edge has no positioned label.
    pub label_position: Option<LayoutPoint>,
}

/// Versioned container for [`EdgePaintGeometry`].
///
/// Versioned independently of the crate, so consumers can pin a payload shape without pinning a
/// merman release — the same contract `svg-plan-json` establishes for the capability plan.
#[derive(Debug, Clone, Serialize)]
pub struct EdgePaintGeometryOutput {
    schema_version: u32,
    diagram_type: String,
    edges: Vec<EdgePaintGeometry>,
}

impl EdgePaintGeometryOutput {
    /// Bumped whenever the payload shape changes incompatibly.
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn new(diagram_type: String, edges: Vec<EdgePaintGeometry>) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            diagram_type,
            edges,
        }
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn diagram_type(&self) -> &str {
        &self.diagram_type
    }

    pub fn edges(&self) -> &[EdgePaintGeometry] {
        &self.edges
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-block")]
pub struct BlockDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    #[serde(
        default,
        rename = "shapeGeometries",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub shape_geometries: Vec<crate::block::BlockShapeGeometry>,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-requirement")]
pub struct RequirementDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-architecture")]
pub struct ArchitectureDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    /// Local Cytoscape child contribution phases for Architecture services.
    ///
    /// These bounds are exposed for source-backed group/content audits. SVG renderers should use
    /// their own phase-specific logic instead of treating this as a generic root-bounds source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cytoscape_service_bounds: Vec<ArchitectureCytoscapeServiceBounds>,
    /// Final FCoSE compound rectangles exposed for source-backed Architecture audits.
    ///
    /// Renderers should not treat these as Mermaid SVG group rects without a phase-specific
    /// source audit; they are layout-engine rects, not browser `node.boundingBox()` values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fcose_compound_bounds: Vec<ArchitectureCompoundBounds>,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-architecture")]
pub struct ArchitectureCompoundBounds {
    pub id: String,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-architecture")]
pub struct ArchitectureCytoscapeServiceBounds {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_group: Option<String>,
    pub body_bounds: Bounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_bounds: Option<Bounds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_metrics: Option<ArchitectureCytoscapeServiceLabelMetrics>,
    pub union_bounds: Bounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-architecture")]
pub struct ArchitectureCytoscapeServiceLabelMetrics {
    pub text_width: f64,
    pub half_width: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-mindmap")]
pub struct MindmapDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub bounds: Option<Bounds>,
    /// Swimlane decoration owned by the selected renderer, not semantic Mindmap nodes.
    #[serde(skip)]
    pub swimlane_lanes: Vec<LayoutCluster>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-sankey")]
pub struct SankeyNodeLayout {
    pub id: String,
    pub index: usize,
    pub depth: usize,
    pub height: usize,
    pub layer: usize,
    pub value: f64,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-sankey")]
pub struct SankeyLinkLayout {
    pub index: usize,
    pub source: String,
    pub target: String,
    pub value: f64,
    pub width: f64,
    pub y0: f64,
    pub y1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-sankey")]
pub struct SankeyDiagramLayout {
    pub bounds: Option<Bounds>,
    pub width: f64,
    pub height: f64,
    pub node_width: f64,
    pub node_padding: f64,
    pub nodes: Vec<SankeyNodeLayout>,
    pub links: Vec<SankeyLinkLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-radar")]
pub struct RadarAxisLayout {
    pub label: String,
    pub angle: f64,
    pub line_x2: f64,
    pub line_y2: f64,
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-radar")]
pub struct RadarGraticuleShapeLayout {
    pub kind: String,
    pub r: Option<f64>,
    #[serde(default)]
    pub points: Vec<LayoutPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-radar")]
pub struct RadarCurveLayout {
    pub label: String,
    pub class_index: i64,
    #[serde(default)]
    pub points: Vec<LayoutPoint>,
    pub path_d: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-radar")]
pub struct RadarLegendItemLayout {
    pub label: String,
    pub class_index: i64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-radar")]
pub struct RadarDiagramLayout {
    pub bounds: Option<Bounds>,
    pub svg_width: f64,
    pub svg_height: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub radius: f64,
    pub axis_label_factor: f64,
    pub title_y: f64,
    #[serde(default)]
    pub axes: Vec<RadarAxisLayout>,
    #[serde(default)]
    pub graticules: Vec<RadarGraticuleShapeLayout>,
    #[serde(default)]
    pub curves: Vec<RadarCurveLayout>,
    #[serde(default)]
    pub legend_items: Vec<RadarLegendItemLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-treemap")]
pub struct TreemapSectionLayout {
    pub name: String,
    pub depth: i64,
    pub value: f64,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    #[serde(default)]
    pub class_selector: Option<String>,
    #[serde(default)]
    pub css_compiled_styles: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-treemap")]
pub struct TreemapLeafLayout {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub parent_name: Option<String>,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    #[serde(default)]
    pub class_selector: Option<String>,
    #[serde(default)]
    pub css_compiled_styles: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-treemap")]
pub struct TreemapDiagramLayout {
    pub title_height: f64,
    pub width: f64,
    pub height: f64,
    pub use_max_width: bool,
    pub diagram_padding: f64,
    pub show_values: bool,
    pub value_format: String,
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub sections: Vec<TreemapSectionLayout>,
    #[serde(default)]
    pub leaves: Vec<TreemapLeafLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-venn")]
pub struct VennCircleLayout {
    pub set: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-venn")]
pub struct VennAreaLayout {
    pub sets: Vec<String>,
    pub size: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub text_x: f64,
    pub text_y: f64,
    pub text_disjoint: bool,
    #[serde(default)]
    pub circles: Vec<VennCircleLayout>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-venn")]
pub struct VennTextDebugCellLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-venn")]
pub struct VennTextAreaLayout {
    pub sets: Vec<String>,
    pub center_x: f64,
    pub center_y: f64,
    pub inner_radius: f64,
    pub font_size: f64,
    #[serde(default)]
    pub debug_cells: Vec<VennTextDebugCellLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-venn")]
pub struct VennTextNodeLayout {
    pub sets: Vec<String>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-venn")]
pub struct VennDiagramLayout {
    pub bounds: Option<Bounds>,
    pub width: f64,
    pub height: f64,
    pub diagram_height: f64,
    pub title_height: f64,
    pub scale: f64,
    pub padding: f64,
    pub use_max_width: bool,
    pub use_debug_layout: bool,
    #[serde(default)]
    pub areas: Vec<VennAreaLayout>,
    #[serde(default)]
    pub text_areas: Vec<VennTextAreaLayout>,
    #[serde(default)]
    pub text_nodes: Vec<VennTextNodeLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-xychart")]
pub struct XyChartRectData {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    #[serde(rename = "strokeFill")]
    pub stroke_fill: String,
    #[serde(rename = "strokeWidth")]
    pub stroke_width: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-xychart")]
pub struct XyChartTextData {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub fill: String,
    #[serde(rename = "fontSize")]
    pub font_size: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(rename = "verticalPos")]
    pub vertical_pos: String,
    #[serde(rename = "horizontalPos")]
    pub horizontal_pos: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-xychart")]
pub struct XyChartPathData {
    pub path: String,
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(rename = "strokeFill")]
    pub stroke_fill: String,
    #[serde(rename = "strokeWidth")]
    pub stroke_width: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg(feature = "diagram-xychart")]
pub enum XyChartDrawableElem {
    #[serde(rename = "rect")]
    Rect {
        #[serde(rename = "groupTexts")]
        group_texts: Vec<String>,
        data: Vec<XyChartRectData>,
    },
    #[serde(rename = "text")]
    Text {
        #[serde(rename = "groupTexts")]
        group_texts: Vec<String>,
        data: Vec<XyChartTextData>,
    },
    #[serde(rename = "path")]
    Path {
        #[serde(rename = "groupTexts")]
        group_texts: Vec<String>,
        data: Vec<XyChartPathData>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-xychart")]
pub struct XyChartDiagramLayout {
    pub width: f64,
    pub height: f64,
    #[serde(rename = "chartOrientation")]
    pub chart_orientation: String,
    #[serde(rename = "showDataLabel")]
    pub show_data_label: bool,
    #[serde(rename = "showDataLabelOutsideBar", default)]
    pub show_data_label_outside_bar: bool,
    #[serde(rename = "backgroundColor")]
    pub background_color: String,
    #[serde(rename = "labelData")]
    pub label_data: Vec<String>,
    #[serde(default)]
    pub drawables: Vec<XyChartDrawableElem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-quadrant-chart")]
pub struct QuadrantChartTextData {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub fill: String,
    #[serde(rename = "fontSize")]
    pub font_size: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(rename = "verticalPos")]
    pub vertical_pos: String,
    #[serde(rename = "horizontalPos")]
    pub horizontal_pos: String,
}

#[cfg(feature = "diagram-quadrant-chart")]
pub type QuadrantChartAxisLabelData = QuadrantChartTextData;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-quadrant-chart")]
pub struct QuadrantChartQuadrantData {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub text: QuadrantChartTextData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-quadrant-chart")]
pub struct QuadrantChartBorderLineData {
    #[serde(rename = "strokeWidth")]
    pub stroke_width: f64,
    #[serde(rename = "strokeFill")]
    pub stroke_fill: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-quadrant-chart")]
pub struct QuadrantChartPointData {
    pub x: f64,
    pub y: f64,
    pub fill: String,
    pub radius: f64,
    #[serde(rename = "strokeColor")]
    pub stroke_color: String,
    #[serde(rename = "strokeWidth")]
    pub stroke_width: String,
    pub text: QuadrantChartTextData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-quadrant-chart")]
pub struct QuadrantChartDiagramLayout {
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub title: Option<QuadrantChartTextData>,
    pub quadrants: Vec<QuadrantChartQuadrantData>,
    #[serde(rename = "borderLines")]
    pub border_lines: Vec<QuadrantChartBorderLineData>,
    pub points: Vec<QuadrantChartPointData>,
    #[serde(rename = "axisLabels")]
    pub axis_labels: Vec<QuadrantChartAxisLabelData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub struct FlowchartLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub clusters: Vec<LayoutCluster>,
    pub bounds: Option<Bounds>,
    /// Mermaid's DOM insertion order for each extracted root graph (`""` = top-level root).
    #[serde(skip)]
    pub dom_node_order_by_root: std::collections::HashMap<String, Vec<String>>,
    /// Whether SVG emission follows Mermaid's ELK adapter DOM structure.
    #[serde(skip)]
    pub uses_elk_adapter_dom: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-mindmap"
))]
pub enum SwimlaneDirection {
    #[serde(rename = "TB")]
    Tb,
    #[serde(rename = "LR")]
    Lr,
    #[serde(rename = "BT")]
    Bt,
    #[serde(rename = "RL")]
    Rl,
}

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-mindmap"
))]
impl SwimlaneDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tb => "TB",
            Self::Lr => "LR",
            Self::Bt => "BT",
            Self::Rl => "RL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-mindmap"
))]
pub struct SwimlaneTitleRect {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-mindmap"
))]
pub struct SwimlaneNodeLayout {
    pub id: String,
    pub label: String,
    pub label_type: String,
    pub shape: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub top_lane_id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub label_width: f64,
    pub label_height: f64,
    pub layer: usize,
    pub order: usize,
    pub is_edge_label: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-mindmap"
))]
pub struct SwimlaneLaneLayout {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub padding: f64,
    /// Layout-phase label width. SVG rendering must remeasure the emitted title with its
    /// `SvgBBox` measurer rather than treating this as a browser `<text>.getBBox()` fact.
    pub title_label_width: f64,
    /// Layout-phase label height; see `title_label_width` for the phase boundary.
    pub title_label_height: f64,
    #[serde(default)]
    pub content_top: Option<f64>,
    #[serde(default)]
    pub title_rect: Option<SwimlaneTitleRect>,
    #[serde(default)]
    pub requested_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-mindmap"
))]
pub struct SwimlaneEdgeLayout {
    pub id: String,
    pub from: String,
    pub to: String,
    pub points: Vec<LayoutPoint>,
    #[serde(default)]
    pub label_node_id: Option<String>,
    pub reversed_for_layout: bool,
    pub curve: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-mindmap"
))]
pub struct SwimlaneLayout {
    pub direction: SwimlaneDirection,
    pub nodes: Vec<SwimlaneNodeLayout>,
    pub lanes: Vec<SwimlaneLaneLayout>,
    pub edges: Vec<SwimlaneEdgeLayout>,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-state")]
pub struct StateDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub clusters: Vec<LayoutCluster>,
    pub bounds: Option<Bounds>,
    /// Registered ELK layout uses its own edge painting projection.
    #[serde(skip)]
    pub uses_elk_adapter_dom: bool,
    #[serde(skip)]
    pub elk_edge_paths: std::collections::HashMap<String, Vec<LayoutPoint>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-class")]
pub struct ClassDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub clusters: Vec<LayoutCluster>,
    pub bounds: Option<Bounds>,
    /// Whether SVG emission follows Mermaid's registered ELK renderer DOM structure.
    #[serde(skip)]
    pub uses_elk_adapter_dom: bool,
    #[serde(skip)]
    pub class_label_plans_by_id: FxHashMap<String, Arc<ClassNodeLabelPlan>>,
    /// DOM ownership produced by the same recursive Dagre preparation that owns layout.
    ///
    /// Compatibility layout JSON remains flat, while SVG rendering consumes this typed tree so it
    /// never has to reconstruct extracted namespace roots from the semantic model.
    #[serde(skip)]
    pub render_tree: ClassRenderTree,
}

#[derive(Debug, Clone, Default)]
#[cfg(feature = "diagram-class")]
pub struct ClassRenderRoot {
    pub namespace_id: Option<String>,
    pub cluster_ids: Vec<String>,
    pub edge_ids: Vec<String>,
    pub items: Vec<ClassRenderItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg(feature = "diagram-class")]
pub struct ClassRenderRootId(pub usize);

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-class")]
pub struct ClassRenderTree {
    pub roots: Vec<ClassRenderRoot>,
    pub top: ClassRenderRootId,
}

#[cfg(feature = "diagram-class")]
impl Default for ClassRenderTree {
    fn default() -> Self {
        Self {
            roots: vec![ClassRenderRoot::default()],
            top: ClassRenderRootId(0),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-class")]
pub enum ClassRenderItem {
    Node(String),
    Subgraph(ClassRenderRootId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-er")]
pub struct ErDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    /// Operation-owned entity geometry shared by layout and SVG emission.
    #[serde(default, skip)]
    pub(crate) prepared_entities: std::collections::HashMap<String, crate::er::ErEntityMeasure>,
    /// SVG-facing edge projection.
    ///
    /// Dagre needs self-loop helper nodes and three internal edge segments to produce stable
    /// ranks. Mermaid's common painter merges those segments back into one visible edge. Keep
    /// that projection beside the canonical layout without serializing it into layout artifacts.
    #[serde(default, skip)]
    pub(crate) render_edges: Vec<LayoutEdge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clusters: Vec<LayoutCluster>,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-sequence")]
pub struct SequenceBlockLayout {
    pub start_y: f64,
    pub stop_y: f64,
    /// Mermaid expands every active sequence frame by its nesting depth.
    /// These coordinates are captured during layout replay rather than inferred from SVG nodes.
    pub start_x: Option<f64>,
    pub stop_x: Option<f64>,
    pub section_ys_by_id: FxHashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-sequence")]
pub struct SequenceDiagramLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub clusters: Vec<LayoutCluster>,
    pub bounds: Option<Bounds>,
    /// Canonical vertical geometry captured while replaying Mermaid's sequence layout cursor.
    ///
    /// This remains outside compatibility layout JSON because it is renderer-owned phase data.
    #[serde(skip)]
    pub block_layouts_by_id: FxHashMap<String, SequenceBlockLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-info")]
pub struct InfoDiagramLayout {
    pub bounds: Option<Bounds>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-packet")]
pub struct PacketBlockLayout {
    pub start: i64,
    pub end: i64,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-packet")]
pub struct PacketWordLayout {
    pub blocks: Vec<PacketBlockLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-packet")]
pub struct PacketDiagramLayout {
    pub bounds: Option<Bounds>,
    pub width: f64,
    pub height: f64,
    pub row_height: f64,
    pub padding_x: f64,
    pub padding_y: f64,
    pub bit_width: f64,
    pub bits_per_row: i64,
    pub show_bits: bool,
    pub words: Vec<PacketWordLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-pie")]
pub struct PieSliceLayout {
    pub label: String,
    pub value: f64,
    pub start_angle: f64,
    pub end_angle: f64,
    pub is_full_circle: bool,
    pub percent: i64,
    pub text_x: f64,
    pub text_y: f64,
    pub fill: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-pie")]
pub struct PieLegendItemLayout {
    pub label: String,
    pub value: f64,
    pub fill: String,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-pie")]
pub struct PieDiagramLayout {
    pub bounds: Option<Bounds>,
    #[serde(default)]
    pub title: Option<String>,
    pub center_x: f64,
    pub center_y: f64,
    pub radius: f64,
    pub outer_radius: f64,
    pub legend_x: f64,
    pub legend_start_y: f64,
    pub legend_step_y: f64,
    pub slices: Vec<PieSliceLayout>,
    pub legend_items: Vec<PieLegendItemLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-timeline")]
pub struct TimelineNodeLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Width used for wrapping (excluding padding).
    pub content_width: f64,
    /// Padding used to compute `width` (Mermaid 11.12.2 uses 20).
    pub padding: f64,
    pub section_class: String,
    pub label: String,
    /// Wrapped lines as rendered into `<tspan>` nodes.
    pub label_lines: Vec<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-timeline")]
pub struct TimelineLineLayout {
    pub kind: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-timeline")]
pub struct TimelineTaskLayout {
    pub node: TimelineNodeLayout,
    #[serde(default)]
    pub connectors: Vec<TimelineLineLayout>,
    pub events: Vec<TimelineNodeLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-timeline")]
pub struct TimelineSectionLayout {
    pub node: TimelineNodeLayout,
    pub tasks: Vec<TimelineTaskLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-timeline")]
pub struct TimelineDiagramLayout {
    pub direction: merman_core::diagrams::timeline::TimelineDirection,
    pub bounds: Option<Bounds>,
    pub left_margin: f64,
    pub base_x: f64,
    pub base_y: f64,
    /// `svg.node().getBBox().width` computed *before* title/activity line are inserted.
    pub pre_title_box_width: f64,
    pub sections: Vec<TimelineSectionLayout>,
    #[serde(default)]
    pub orphan_tasks: Vec<TimelineTaskLayout>,
    pub activity_line: TimelineLineLayout,
    pub title: Option<String>,
    pub title_x: f64,
    pub title_y: f64,
    #[serde(default = "crate::timeline::default_use_max_width")]
    pub use_max_width: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub struct JourneyActorLegendLineLayout {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub tspan_x: f64,
    pub text_margin: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub struct JourneyActorLegendItemLayout {
    pub actor: String,
    pub pos: i64,
    pub color: String,
    pub circle_cx: f64,
    pub circle_cy: f64,
    pub circle_r: f64,
    pub label_lines: Vec<JourneyActorLegendLineLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub enum JourneyMouthKind {
    Smile,
    Sad,
    Ambivalent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub struct JourneyTaskActorCircleLayout {
    pub actor: String,
    pub pos: i64,
    pub color: String,
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub struct JourneyTaskLayout {
    pub index: i64,
    pub section: String,
    pub task: String,
    pub score: i64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub num: i64,
    pub people: Vec<String>,
    pub actor_circles: Vec<JourneyTaskActorCircleLayout>,
    pub line_id: String,
    pub line_x1: f64,
    pub line_y1: f64,
    pub line_x2: f64,
    pub line_y2: f64,
    pub face_cx: f64,
    pub face_cy: Option<f64>,
    pub mouth: JourneyMouthKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub struct JourneySectionLayout {
    pub section: String,
    pub num: i64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub task_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub struct JourneyLineLayout {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-journey")]
pub struct JourneyDiagramLayout {
    pub bounds: Option<Bounds>,
    pub left_margin: f64,
    pub max_actor_label_width: f64,
    pub width: f64,
    pub height: f64,
    pub svg_height: f64,
    #[serde(default = "crate::journey::default_use_max_width")]
    pub use_max_width: bool,
    pub title: Option<String>,
    pub title_x: f64,
    pub title_y: f64,
    pub actor_legend: Vec<JourneyActorLegendItemLayout>,
    pub sections: Vec<JourneySectionLayout>,
    pub tasks: Vec<JourneyTaskLayout>,
    pub activity_line: JourneyLineLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-kanban")]
pub struct KanbanSectionLayout {
    pub id: String,
    pub label: String,
    pub index: i64,
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub rect_y: f64,
    pub rect_height: f64,
    pub rx: f64,
    pub ry: f64,
    pub label_width: f64,
    #[serde(skip)]
    pub label_height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-kanban")]
pub struct KanbanItemLayout {
    pub id: String,
    pub label: String,
    pub parent_id: String,
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rx: f64,
    pub ry: f64,
    #[serde(default)]
    pub ticket: Option<String>,
    #[serde(default)]
    pub assigned: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-kanban")]
pub struct KanbanDiagramLayout {
    pub bounds: Option<Bounds>,
    pub section_width: f64,
    pub padding: f64,
    pub max_label_height: f64,
    pub viewbox_padding: f64,
    #[serde(default = "crate::kanban::default_use_max_width")]
    pub use_max_width: bool,
    pub sections: Vec<KanbanSectionLayout>,
    pub items: Vec<KanbanItemLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-git-graph")]
pub struct GitGraphBranchLayout {
    pub name: String,
    pub index: i64,
    pub pos: f64,
    pub bbox_width: f64,
    pub bbox_height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-git-graph")]
pub struct GitGraphCommitLayout {
    pub id: String,
    pub message: String,
    pub seq: i64,
    pub commit_type: i64,
    #[serde(default)]
    pub custom_type: Option<i64>,
    #[serde(default)]
    pub custom_id: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub parents: Vec<String>,
    pub branch: String,
    pub pos: f64,
    pub pos_with_offset: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-git-graph")]
pub struct GitGraphArrowLayout {
    pub from: String,
    pub to: String,
    pub class_index: i64,
    pub d: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-git-graph")]
pub struct GitGraphDiagramLayout {
    pub bounds: Option<Bounds>,
    pub direction: String,
    pub rotate_commit_label: bool,
    pub show_branches: bool,
    pub show_commit_label: bool,
    pub parallel_commits: bool,
    pub diagram_padding: f64,
    pub max_pos: f64,
    pub branches: Vec<GitGraphBranchLayout>,
    pub commits: Vec<GitGraphCommitLayout>,
    pub arrows: Vec<GitGraphArrowLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-tree-view")]
pub struct TreeViewNodeLayout {
    pub id: i64,
    pub level: i64,
    pub name: String,
    #[serde(rename = "nodeType")]
    pub node_type: String,
    #[serde(default, rename = "cssClass", skip_serializing_if = "Option::is_none")]
    pub css_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub depth: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub label_x: f64,
    pub label_y: f64,
    pub label_width: f64,
    pub label_height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_width: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-tree-view")]
pub struct TreeViewLineLayout {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub stroke_width: f64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-tree-view")]
pub struct TreeViewDiagramLayout {
    pub bounds: Option<Bounds>,
    pub total_width: f64,
    pub total_height: f64,
    pub row_indent: f64,
    pub padding_x: f64,
    pub padding_y: f64,
    pub line_thickness: f64,
    pub use_max_width: bool,
    pub label_font_size: f64,
    pub nodes: Vec<TreeViewNodeLayout>,
    pub lines: Vec<TreeViewLineLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaLineLayout {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub class_name: String,
    pub marker_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaTextLayout {
    pub text: String,
    pub lines: Vec<String>,
    pub class_name: String,
    pub x: f64,
    pub y: f64,
    pub anchor: String,
    pub line_height: f64,
    pub font_size: f64,
    pub bbox: Bounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaLabelBoxLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaCauseLabelGroupLayout {
    pub label_box: IshikawaLabelBoxLayout,
    pub label: IshikawaTextLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaSubGroupLayout {
    pub line: IshikawaLineLayout,
    pub label: IshikawaTextLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaBranchLayout {
    pub line: IshikawaLineLayout,
    pub label_group: IshikawaCauseLabelGroupLayout,
    pub sub_groups: Vec<IshikawaSubGroupLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaPairLayout {
    pub upper: IshikawaBranchLayout,
    pub lower: Option<IshikawaBranchLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaHeadLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub path_d: String,
    pub label: IshikawaTextLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaDiagramLayout {
    pub bounds: Option<Bounds>,
    pub total_width: f64,
    pub total_height: f64,
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub padding: f64,
    pub use_max_width: bool,
    pub font_size: f64,
    pub head: Option<IshikawaHeadLayout>,
    pub spine: Option<IshikawaLineLayout>,
    #[serde(default)]
    pub pairs: Vec<IshikawaPairLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-event-modeling")]
pub struct EventModelingSwimlaneLayout {
    pub index: i64,
    pub label: String,
    pub namespace: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-event-modeling")]
pub struct EventModelingBoxLayout {
    pub index: usize,
    pub frame_name: String,
    pub frame_kind: String,
    pub model_entity_type: String,
    pub entity_identifier: String,
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub stroke: String,
    pub swimlane_index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-event-modeling")]
pub struct EventModelingRelationLayout {
    pub source_frame: String,
    pub target_frame: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub stroke: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-event-modeling")]
pub struct EventModelingDiagramLayout {
    pub bounds: Option<Bounds>,
    pub total_width: f64,
    pub total_height: f64,
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub padding: f64,
    pub use_max_width: bool,
    pub swimlanes: Vec<EventModelingSwimlaneLayout>,
    pub boxes: Vec<EventModelingBoxLayout>,
    pub relations: Vec<EventModelingRelationLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-cynefin")]
pub struct CynefinDomainLayout {
    pub name: String,
    pub cx: f64,
    pub cy: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-cynefin")]
pub struct CynefinItemLayout {
    pub domain: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text_x: f64,
    pub text_y: f64,
    pub overflow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-cynefin")]
pub struct CynefinTransitionLayout {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub cpx: f64,
    pub cpy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-cynefin")]
pub struct CynefinDiagramLayout {
    pub bounds: Option<Bounds>,
    pub width: f64,
    pub height: f64,
    pub padding: f64,
    pub total_width: f64,
    pub total_height: f64,
    pub use_max_width: bool,
    pub show_domain_descriptions: bool,
    pub boundary_amplitude: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<f64>,
    pub domain_layouts: Vec<CynefinDomainLayout>,
    pub items: Vec<CynefinItemLayout>,
    pub transitions: Vec<CynefinTransitionLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-railroad")]
pub struct RailroadDiagramLayout {
    pub bounds: Option<Bounds>,
    pub diagram_type: String,
    pub width: f64,
    pub height: f64,
    pub use_max_width: bool,
    pub rules: Vec<RailroadRuleLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-railroad")]
pub struct RailroadRuleLayout {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub baseline_y: f64,
    pub name_width: f64,
    pub definition_x: f64,
    pub start_marker_x: f64,
    pub end_marker_x: f64,
    pub marker_radius: f64,
    pub elements: Vec<RailroadElementLayout>,
    pub paths: Vec<RailroadPathLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-railroad")]
pub struct RailroadElementLayout {
    pub kind: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text_x: f64,
    pub text_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-railroad")]
pub struct RailroadPathLayout {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    pub d: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttAxisTickLayout {
    pub time_ms: i64,
    pub x: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttExcludeRangeLayout {
    pub id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttSectionTitleLayout {
    pub section: String,
    pub index: i64,
    pub x: f64,
    pub y: f64,
    pub dy_em: f64,
    pub lines: Vec<String>,
    pub class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttRowLayout {
    pub index: i64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttTaskLabelLayout {
    pub id: String,
    pub text: String,
    pub font_size: f64,
    pub width: f64,
    pub x: f64,
    pub y: f64,
    pub class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttTaskBarLayout {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rx: f64,
    pub ry: f64,
    pub class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttTaskLayout {
    pub id: String,
    pub task: String,
    pub section: String,
    pub task_type: String,
    pub order: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    #[serde(default)]
    pub render_end_ms: Option<i64>,
    pub milestone: bool,
    pub vert: bool,
    pub bar: GanttTaskBarLayout,
    pub label: GanttTaskLabelLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-gantt")]
pub struct GanttDiagramLayout {
    pub bounds: Option<Bounds>,
    pub width: f64,
    pub height: f64,
    pub left_padding: f64,
    pub right_padding: f64,
    pub top_padding: f64,
    pub grid_line_start_padding: f64,
    pub bar_height: f64,
    pub bar_gap: f64,
    pub title_top_margin: f64,
    pub font_size: f64,
    pub section_font_size: f64,
    pub number_section_styles: i64,
    pub display_mode: String,
    pub date_format: String,
    pub axis_format: String,
    #[serde(default)]
    pub tick_interval: Option<String>,
    pub top_axis: bool,
    pub today_marker: String,
    pub categories: Vec<String>,
    pub rows: Vec<GanttRowLayout>,
    pub section_titles: Vec<GanttSectionTitleLayout>,
    pub tasks: Vec<GanttTaskLayout>,
    pub excludes: Vec<GanttExcludeRangeLayout>,
    #[serde(default)]
    pub has_excludes_layer: bool,
    pub bottom_ticks: Vec<GanttAxisTickLayout>,
    #[serde(default)]
    pub top_ticks: Vec<GanttAxisTickLayout>,
    pub title: Option<String>,
    pub title_x: f64,
    pub title_y: f64,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-c4")]
pub(crate) struct C4TextRowLayout {
    /// Source words emitted into one Mermaid `createText` outer tspan.
    ///
    /// Keeping the source words (rather than only a joined string) preserves the same
    /// whitespace/token boundaries for layout and SVG emission.
    pub(crate) words: Vec<String>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-c4")]
pub(crate) struct C4TextRenderPlan {
    /// Wrapped source-word rows consumed by the C4 SVG emitter.
    pub(crate) rows: Vec<C4TextRowLayout>,
    /// The section bbox x offset relative to the centered outer tspan anchor.
    pub(crate) bbox_x: f64,
    /// The section bbox y offset produced by Mermaid's createText DOM shape.
    pub(crate) bbox_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-c4")]
pub struct C4TextBlockLayout {
    pub text: String,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub line_count: usize,
    /// Unified C4 labels carry the exact wrapped rows used during measurement. Legacy C4
    /// boundary and relationship labels leave this unset and continue using their historical
    /// text path.
    /// This is an in-memory render artifact; the public layout projection intentionally keeps
    /// the stable historical C4 layout schema and does not expose tokenization details.
    #[serde(skip)]
    pub(crate) render_plan: Option<C4TextRenderPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-c4")]
pub struct C4ImageLayout {
    pub width: f64,
    pub height: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-c4")]
pub struct C4ShapeLayout {
    pub alias: String,
    pub parent_boundary: String,
    pub type_c4_shape: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    pub image: C4ImageLayout,
    pub type_block: C4TextBlockLayout,
    pub label: C4TextBlockLayout,
    #[serde(default)]
    pub ty: Option<C4TextBlockLayout>,
    #[serde(default)]
    pub techn: Option<C4TextBlockLayout>,
    #[serde(default)]
    pub descr: Option<C4TextBlockLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-c4")]
pub struct C4BoundaryLayout {
    pub alias: String,
    pub parent_boundary: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub image: C4ImageLayout,
    pub label: C4TextBlockLayout,
    #[serde(default)]
    pub ty: Option<C4TextBlockLayout>,
    #[serde(default)]
    pub descr: Option<C4TextBlockLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "diagram-c4")]
pub struct C4RelLayout {
    pub from: String,
    pub to: String,
    pub rel_type: String,
    pub start_point: LayoutPoint,
    pub end_point: LayoutPoint,
    #[serde(default)]
    pub offset_x: Option<i64>,
    #[serde(default)]
    pub offset_y: Option<i64>,
    pub label: C4TextBlockLayout,
    #[serde(default)]
    pub techn: Option<C4TextBlockLayout>,
    #[serde(default)]
    pub descr: Option<C4TextBlockLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[cfg(feature = "diagram-c4")]
pub struct C4DiagramLayout {
    pub bounds: Option<Bounds>,
    pub width: f64,
    pub height: f64,
    #[serde(default = "crate::c4::default_use_max_width")]
    pub use_max_width: bool,
    pub container_width: f64,
    pub container_height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_available_width: Option<f64>,
    pub c4_type: String,
    pub title: Option<String>,
    pub boundaries: Vec<C4BoundaryLayout>,
    pub shapes: Vec<C4ShapeLayout>,
    pub rels: Vec<C4RelLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDiagramLayout {
    pub viewbox_width: f64,
    pub viewbox_height: f64,
    pub max_width_px: f64,
}
