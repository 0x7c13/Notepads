use super::super::timing::RenderTiming;
use super::ClassSvgRelation;
use super::context::{ClassEmitCheckpoint, ClassRenderDetails};
use super::edge::{
    ClassEdgeGroupsRenderContext, ClassEdgeGroupsRenderState, render_class_edge_groups,
};
use crate::Result;
use crate::model::{Bounds, LayoutEdge, LayoutPoint};
use crate::svg::parity::SvgDiagramId;
use crate::text::{TextMeasurer, TextStyle};
use rustc_hash::FxHashMap;

pub(super) struct ClassSplitEdgeGroupsRenderState<'a> {
    pub(super) content_bounds: &'a mut Option<Bounds>,
    pub(super) detail: &'a mut ClassRenderDetails,
}

pub(super) struct ClassSplitEdgeGroupsRenderContext<'a> {
    pub(super) edges: &'a [LayoutEdge],
    pub(super) missing_section_points: &'a FxHashMap<&'a str, Vec<LayoutPoint>>,
    pub(super) work_meter: &'a crate::resources::OperationWorkMeter,
    pub(super) line_hop_paths: &'a std::collections::HashMap<&'a str, String>,
    pub(super) relations_by_id: &'a FxHashMap<&'a str, &'a ClassSvgRelation>,
    pub(super) relation_index_by_id: &'a FxHashMap<&'a str, usize>,
    pub(super) diagram_marker_class: &'a str,
    pub(super) diagram_id: SvgDiagramId<'a>,
    pub(super) content_tx: f64,
    pub(super) content_ty: f64,
    pub(super) edge_use_html_labels: bool,
    pub(super) text_measurer: &'a dyn TextMeasurer,
    pub(super) terminal_text_style: &'a TextStyle,
    pub(super) mermaid_config: Option<&'a merman_core::MermaidConfig>,
    pub(super) math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub(super) look: &'a str,
    pub(super) hand_drawn_seed: roughr::core::RoughRandomness,
    pub(super) timing: RenderTiming,
    pub(super) uses_elk_adapter_dom: bool,
    pub(super) edge_paths_class: &'static str,
    pub(super) emit: ClassEmitCheckpoint<'a>,
}

pub(super) struct ClassSplitEdgeGroups {
    pub(super) edge_paths: String,
    pub(super) edge_labels: String,
}

pub(super) fn render_class_split_edge_groups(
    state: ClassSplitEdgeGroupsRenderState<'_>,
    ctx: &ClassSplitEdgeGroupsRenderContext<'_>,
    bounds_dx: f64,
    bounds_dy: f64,
) -> Result<ClassSplitEdgeGroups> {
    let ClassSplitEdgeGroupsRenderState {
        content_bounds,
        detail,
    } = state;

    let mut edge_paths = String::new();
    let mut edge_labels = String::new();
    render_class_edge_groups(
        ClassEdgeGroupsRenderState {
            edge_paths: &mut edge_paths,
            edge_labels: &mut edge_labels,
            content_bounds,
            detail,
        },
        &ClassEdgeGroupsRenderContext {
            edges: ctx.edges,
            missing_section_points: ctx.missing_section_points,
            work_meter: ctx.work_meter,
            line_hop_paths: ctx.line_hop_paths,
            relations_by_id: ctx.relations_by_id,
            relation_index_by_id: ctx.relation_index_by_id,
            diagram_marker_class: ctx.diagram_marker_class,
            diagram_id: ctx.diagram_id,
            content_tx: ctx.content_tx,
            content_ty: ctx.content_ty,
            bounds_dx,
            bounds_dy,
            edge_use_html_labels: ctx.edge_use_html_labels,
            text_measurer: ctx.text_measurer,
            terminal_text_style: ctx.terminal_text_style,
            mermaid_config: ctx.mermaid_config,
            math_renderer: ctx.math_renderer,
            look: ctx.look,
            hand_drawn_seed: ctx.hand_drawn_seed.clone(),
            timing: ctx.timing,
            uses_elk_adapter_dom: ctx.uses_elk_adapter_dom,
            edge_paths_class: ctx.edge_paths_class,
            emit: ctx.emit,
        },
    )?;
    Ok(ClassSplitEdgeGroups {
        edge_paths,
        edge_labels,
    })
}
