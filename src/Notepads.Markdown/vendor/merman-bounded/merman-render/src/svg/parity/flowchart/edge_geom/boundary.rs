use super::super::*;

#[derive(Debug, Clone, Copy)]
pub(in crate::svg::parity::flowchart) struct BoundaryNode {
    pub(in crate::svg::parity::flowchart) x: f64,
    pub(in crate::svg::parity::flowchart) y: f64,
    pub(in crate::svg::parity::flowchart) width: f64,
    pub(in crate::svg::parity::flowchart) height: f64,
}

pub(in crate::svg::parity::flowchart) fn boundary_for_node(
    ctx: &FlowchartRenderCtx<'_>,
    node_id: &str,
    origin_x: f64,
    origin_y: f64,
) -> Option<BoundaryNode> {
    if let Some(n) = ctx.layout_nodes_by_id.get(node_id) {
        return Some(BoundaryNode {
            x: n.x + ctx.tx - origin_x,
            y: n.y + ctx.ty - origin_y,
            width: n.width,
            height: n.height,
        });
    }
    let n = ctx.layout_clusters_by_id.get(node_id)?;
    Some(BoundaryNode {
        x: n.x + ctx.tx - origin_x,
        y: n.y + ctx.ty - origin_y,
        width: n.width,
        height: n.height,
    })
}

pub(in crate::svg::parity::flowchart) fn boundary_for_cluster(
    ctx: &FlowchartRenderCtx<'_>,
    cluster_id: &str,
    origin_x: f64,
    origin_y: f64,
) -> Option<BoundaryNode> {
    let n = ctx.layout_clusters_by_id.get(cluster_id)?;
    Some(BoundaryNode {
        x: n.x + ctx.tx - origin_x,
        y: n.y + ctx.ty - origin_y,
        width: n.width,
        height: n.height,
    })
}
