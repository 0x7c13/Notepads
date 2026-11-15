//! Flowchart render-time edge and helper-node preparation.

use std::borrow::Cow;
use std::collections::BTreeSet;

use rustc_hash::FxHashSet;

pub(in crate::svg::parity::flowchart) struct FlowchartRenderInputs<'a> {
    pub render_edges: Vec<Cow<'a, crate::flowchart::FlowEdge>>,
    pub extra_nodes: Vec<crate::flowchart::FlowNode>,
}

pub(in crate::svg::parity::flowchart) fn prepare_flowchart_render_inputs<'a>(
    model: &'a crate::flowchart::FlowchartModel,
    render_context: &crate::flowchart::FlowchartRenderContext,
    uses_elk_adapter_dom: bool,
) -> FlowchartRenderInputs<'a> {
    let projected_edges = model
        .edges
        .iter()
        .filter_map(|edge| crate::flowchart::project_flowchart_edge(edge, render_context))
        .collect::<Vec<_>>();
    if uses_elk_adapter_dom {
        return FlowchartRenderInputs {
            render_edges: projected_edges.into_iter().map(Cow::Owned).collect(),
            extra_nodes: Vec::new(),
        };
    }

    // Mermaid 11.16 keeps the helper nodes used by Dagre, but merges their three layout segments
    // back into the original logical self-loop before rendering.
    let mut self_loop_label_node_ids: BTreeSet<String> = BTreeSet::new();
    for edge in &projected_edges {
        if edge.from != edge.to {
            continue;
        }
        self_loop_label_node_ids.insert(format!("{}---{}---1", edge.from, edge.from));
        self_loop_label_node_ids.insert(format!("{}---{}---2", edge.from, edge.from));
    }
    let mut render_edges: Vec<Cow<'a, crate::flowchart::FlowEdge>> =
        projected_edges.into_iter().map(Cow::Owned).collect();

    // Mermaid's `adjustClustersAndEdges(graph)` rewrites edges that connect directly to cluster
    // nodes by removing and re-adding them (after swapping endpoints to anchor nodes). This has a
    // visible side-effect: those edges end up later in `graph.edges()` insertion order, so the
    // DOM emitted under `.edgePaths` / `.edgeLabels` matches that stable partition.
    let cluster_ids_with_children: FxHashSet<&str> = model
        .subgraphs
        .iter()
        .filter(|sg| !sg.nodes.is_empty())
        .filter(|sg| !render_context.is_subgraph_collapsed(sg.id.as_str()))
        .filter(|sg| {
            render_context
                .collapsed_replacement(sg.id.as_str())
                .is_none()
        })
        .map(|sg| sg.id.as_str())
        .collect();
    if !cluster_ids_with_children.is_empty() && render_edges.len() >= 2 {
        let mut normal: Vec<Cow<'a, crate::flowchart::FlowEdge>> =
            Vec::with_capacity(render_edges.len());
        let mut cluster: Vec<Cow<'a, crate::flowchart::FlowEdge>> = Vec::new();
        for e in render_edges {
            let edge = e.as_ref();
            if cluster_ids_with_children.contains(edge.from.as_str())
                || cluster_ids_with_children.contains(edge.to.as_str())
            {
                cluster.push(e);
            } else {
                normal.push(e);
            }
        }
        normal.extend(cluster);
        render_edges = normal;
    }

    // `getEdgesToRender` first emits every regular Graphlib edge, then appends merged self-loop
    // groups in first-seen order. Preserve that 11.16 DOM ordering independently of model order.
    if render_edges.len() >= 2 {
        let mut regular = Vec::with_capacity(render_edges.len());
        let mut self_loops = Vec::new();
        for edge in render_edges {
            if edge.from == edge.to {
                self_loops.push(edge);
            } else {
                regular.push(edge);
            }
        }
        regular.extend(self_loops);
        render_edges = regular;
    }

    let mut extra_nodes: Vec<crate::flowchart::FlowNode> =
        Vec::with_capacity(self_loop_label_node_ids.len());
    for id in &self_loop_label_node_ids {
        extra_nodes.push(crate::flowchart::FlowNode {
            id: id.clone(),
            provenance: Default::default(),
            label: Some(String::new()),
            label_type: None,
            layout_shape: None,
            shape: None,
            icon: None,
            form: None,
            pos: None,
            img: None,
            constraint: None,
            asset_width: None,
            asset_height: None,
            classes: Vec::new(),
            styles: Vec::new(),
            have_callback: false,
            link: None,
            link_target: None,
        });
    }

    FlowchartRenderInputs {
        render_edges,
        extra_nodes,
    }
}
