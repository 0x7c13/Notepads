use super::super::timing::RenderTiming;
use super::context::{ClassEmitCheckpoint, ClassRenderDetails};
use super::groups::{
    ClassSplitEdgeGroupsRenderContext, ClassSplitEdgeGroupsRenderState,
    render_class_split_edge_groups,
};
use super::interface::{
    ClassInterfaceRenderContext, ClassInterfaceRenderState, render_class_interface_node,
};
use super::label::class_apply_inline_styles;
use super::namespace::{
    ClassNamespaceClusterGroupContext, class_namespace_root_offset,
    render_class_namespace_cluster_group, render_class_namespace_clusters_in_root,
};
use super::node::{
    ClassHtmlNodeBodyContext, ClassNodeBasicContainerContext, ClassNodeRenderPosition,
    ClassNodeRenderState, ClassNodeShellContext, ClassSvgNodeBodyContext,
    render_class_html_node_body, render_class_node_basic_container, render_class_node_shell_open,
    render_class_svg_node_body,
};
use super::note::{ClassNoteRenderContext, ClassNoteRenderState, render_class_note_node};
use super::settings::ClassRenderSettings;
use super::*;
use super::{ClassSvgInterface, ClassSvgNode, ClassSvgNote};
use crate::model::{Bounds, ClassDiagramLayout, ClassRenderItem, ClassRenderRootId, LayoutEdge};
use crate::{Error, Result};
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
struct ClassNodeRootOffsets {
    namespace_root_dx: f64,
    namespace_root_dy: f64,
    in_namespace_root: bool,
}

pub(super) struct ClassNodesRenderState<'a> {
    pub(super) out: &'a mut String,
    pub(super) content_bounds: &'a mut Option<Bounds>,
    pub(super) detail: &'a mut ClassRenderDetails,
    pub(super) sanitize_config: &'a mut Option<merman_core::MermaidConfig>,
    pub(super) borrowed_sanitize_config: Option<&'a merman_core::MermaidConfig>,
}

pub(super) struct ClassNodesRenderContext<'a> {
    pub(super) layout: &'a ClassDiagramLayout,
    pub(super) class_nodes_by_id: &'a FxHashMap<&'a str, &'a ClassSvgNode>,
    pub(super) class_color_indices: &'a FxHashMap<&'a str, usize>,
    pub(super) note_by_id: &'a FxHashMap<&'a str, &'a ClassSvgNote>,
    pub(super) iface_by_id: &'a FxHashMap<&'a str, &'a ClassSvgInterface>,
    pub(super) settings: &'a ClassRenderSettings,
    pub(super) effective_config: &'a serde_json::Value,
    pub(super) diagram_id: SvgDiagramId<'a>,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) mermaid_config: Option<&'a merman_core::MermaidConfig>,
    pub(super) math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub(super) content_tx: f64,
    pub(super) content_ty: f64,
    pub(super) timing: RenderTiming,
    pub(super) emit: ClassEmitCheckpoint<'a>,
}

pub(super) fn render_class_render_tree(
    state: ClassNodesRenderState<'_>,
    ctx: &ClassNodesRenderContext<'_>,
    edge_ctx: &ClassSplitEdgeGroupsRenderContext<'_>,
) -> Result<()> {
    let ClassNodesRenderState {
        out,
        content_bounds,
        detail,
        sanitize_config,
        borrowed_sanitize_config,
    } = state;
    let layout_nodes_by_id = ctx
        .layout
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<FxHashMap<_, _>>();
    let clusters_by_id = ctx
        .layout
        .clusters
        .iter()
        .map(|cluster| (cluster.id.as_str(), cluster))
        .collect::<HashMap<_, _>>();
    let edges_by_id = ctx
        .layout
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect::<HashMap<_, _>>();
    validate_class_render_tree(ctx, &layout_nodes_by_id, &clusters_by_id, &edges_by_id)?;

    enum RenderFrame<'a> {
        Enter {
            root_id: ClassRenderRootId,
            parent_origin: (f64, f64),
        },
        Node {
            id: &'a str,
            origin: (f64, f64),
            in_namespace_root: bool,
        },
        Close {
            in_namespace_root: bool,
        },
    }

    let mut stack = vec![RenderFrame::Enter {
        root_id: ctx.layout.render_tree.top,
        parent_origin: (0.0, 0.0),
    }];
    while let Some(frame) = stack.pop() {
        match frame {
            RenderFrame::Enter {
                root_id,
                parent_origin,
            } => {
                let root = ctx
                    .layout
                    .render_tree
                    .roots
                    .get(root_id.0)
                    .expect("validated Class render root id");
                let namespace_id = root.namespace_id.as_deref();
                let origin = namespace_id
                    .map(|id| {
                        clusters_by_id
                            .get(id)
                            .copied()
                            .expect("validated Class namespace root cluster")
                    })
                    .map(class_namespace_root_offset)
                    .unwrap_or((0.0, 0.0));
                let in_namespace_root = namespace_id.is_some();

                if let Some(namespace_id) = namespace_id {
                    let _ = write!(
                        out,
                        r#"<g class="root" transform="translate({}, {})">"#,
                        fmt(origin.0 - parent_origin.0),
                        fmt(origin.1 - parent_origin.1)
                    );
                    render_class_namespace_clusters_in_root(
                        out,
                        content_bounds,
                        &clusters_by_id,
                        &root
                            .cluster_ids
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>(),
                        ClassNamespaceClusterGroupContext {
                            diagram_id: ctx.diagram_id,
                            content_tx: ctx.content_tx,
                            content_ty: ctx.content_ty,
                            bounds_dx: 0.0,
                            bounds_dy: 0.0,
                            use_html_labels: ctx.settings.edge_use_html_labels,
                            look: ctx.settings.look.as_str(),
                            mermaid_config: ctx.mermaid_config,
                            math_renderer: ctx.math_renderer,
                            timing: ctx.timing,
                            emit: ctx.emit,
                        },
                        namespace_id,
                        origin.0,
                        origin.1,
                    )?;
                } else {
                    let clusters = root
                        .cluster_ids
                        .iter()
                        .map(|id| {
                            clusters_by_id
                                .get(id.as_str())
                                .copied()
                                .expect("validated Class render cluster")
                                .clone()
                        })
                        .collect::<Vec<_>>();
                    detail.clusters += render_class_namespace_cluster_group(
                        out,
                        content_bounds,
                        &clusters,
                        ClassNamespaceClusterGroupContext {
                            diagram_id: ctx.diagram_id,
                            content_tx: ctx.content_tx,
                            content_ty: ctx.content_ty,
                            bounds_dx: 0.0,
                            bounds_dy: 0.0,
                            use_html_labels: ctx.settings.edge_use_html_labels,
                            look: ctx.settings.look.as_str(),
                            mermaid_config: ctx.mermaid_config,
                            math_renderer: ctx.math_renderer,
                            timing: ctx.timing,
                            emit: ctx.emit,
                        },
                    )?;
                }

                let edges = root
                    .edge_ids
                    .iter()
                    .map(|id| {
                        edges_by_id
                            .get(id.as_str())
                            .copied()
                            .expect("validated Class render edge")
                            .clone()
                    })
                    .collect::<Vec<_>>();
                let split = render_class_split_edges_for_namespace(
                    content_bounds,
                    detail,
                    edge_ctx,
                    &edges,
                    origin.0,
                    origin.1,
                    in_namespace_root,
                )?;
                out.push_str(&split.edge_paths);
                out.push_str(&split.edge_labels);
                out.push_str(r#"<g class="nodes">"#);

                stack.push(RenderFrame::Close { in_namespace_root });
                for item in root.items.iter().rev() {
                    match item {
                        ClassRenderItem::Node(id) => stack.push(RenderFrame::Node {
                            id,
                            origin,
                            in_namespace_root,
                        }),
                        ClassRenderItem::Subgraph(child) => stack.push(RenderFrame::Enter {
                            root_id: *child,
                            parent_origin: origin,
                        }),
                    }
                }
            }
            RenderFrame::Node {
                id,
                origin,
                in_namespace_root,
            } => render_class_node_id(
                ClassNodesRenderState {
                    out,
                    content_bounds,
                    detail,
                    sanitize_config,
                    borrowed_sanitize_config,
                },
                ctx,
                &layout_nodes_by_id,
                id,
                ClassNodeRootOffsets {
                    namespace_root_dx: origin.0,
                    namespace_root_dy: origin.1,
                    in_namespace_root,
                },
            )?,
            RenderFrame::Close { in_namespace_root } => {
                out.push_str("</g>");
                if in_namespace_root {
                    out.push_str("</g>");
                }
            }
        }
    }
    Ok(())
}

pub(super) fn render_class_elk_adapter_dom(
    state: ClassNodesRenderState<'_>,
    ctx: &ClassNodesRenderContext<'_>,
    edge_ctx: &ClassSplitEdgeGroupsRenderContext<'_>,
) -> Result<()> {
    let ClassNodesRenderState {
        out,
        content_bounds,
        detail,
        sanitize_config,
        borrowed_sanitize_config,
    } = state;
    let layout_nodes_by_id = ctx
        .layout
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<FxHashMap<_, _>>();
    let clusters_by_id = ctx
        .layout
        .clusters
        .iter()
        .map(|cluster| (cluster.id.as_str(), cluster))
        .collect::<HashMap<_, _>>();
    // Consume the prepared paint routes, including terminal straightening and label updates.
    let edges_by_id = edge_ctx
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect::<HashMap<_, _>>();
    validate_class_render_tree(ctx, &layout_nodes_by_id, &clusters_by_id, &edges_by_id)?;

    let root = ctx
        .layout
        .render_tree
        .roots
        .get(ctx.layout.render_tree.top.0)
        .expect("validated Class ELK render root");
    if root.namespace_id.is_some()
        || root
            .items
            .iter()
            .any(|item| matches!(item, ClassRenderItem::Subgraph(_)))
    {
        return Err(Error::InvalidModel {
            message: "Class ELK adapter requires one flat render root".to_string(),
        });
    }

    // Mermaid 12 ELK uses the common layout painter. It inserts one root and four
    // sibling groups in createGraph.ts order: clusters, edge paths, edge labels, nodes.
    // Namespace fills must stay behind relation paths and their markers.
    let edges = root
        .edge_ids
        .iter()
        .map(|id| {
            edges_by_id
                .get(id.as_str())
                .copied()
                .expect("validated Class ELK render edge")
                .clone()
        })
        .collect::<Vec<_>>();
    let split = render_class_split_edges_for_namespace(
        content_bounds,
        detail,
        edge_ctx,
        &edges,
        0.0,
        0.0,
        false,
    )?;
    detail.clusters += render_class_namespace_cluster_group(
        out,
        content_bounds,
        &ctx.layout.clusters,
        ClassNamespaceClusterGroupContext {
            diagram_id: ctx.diagram_id,
            content_tx: ctx.content_tx,
            content_ty: ctx.content_ty,
            bounds_dx: 0.0,
            bounds_dy: 0.0,
            use_html_labels: ctx.settings.edge_use_html_labels,
            look: ctx.settings.look.as_str(),
            mermaid_config: ctx.mermaid_config,
            math_renderer: ctx.math_renderer,
            timing: ctx.timing,
            emit: ctx.emit,
        },
    )?;
    out.push_str(&split.edge_paths);
    out.push_str(&split.edge_labels);

    out.push_str(r#"<g class="nodes">"#);
    for item in &root.items {
        let ClassRenderItem::Node(id) = item else {
            unreachable!("Class ELK render root was validated as flat")
        };
        render_class_node_id(
            ClassNodesRenderState {
                out,
                content_bounds,
                detail,
                sanitize_config,
                borrowed_sanitize_config,
            },
            ctx,
            &layout_nodes_by_id,
            id,
            ClassNodeRootOffsets {
                namespace_root_dx: 0.0,
                namespace_root_dy: 0.0,
                in_namespace_root: false,
            },
        )?;
    }
    out.push_str("</g>");
    Ok(())
}

fn validate_class_render_tree(
    ctx: &ClassNodesRenderContext<'_>,
    layout_nodes_by_id: &FxHashMap<&str, &crate::model::LayoutNode>,
    clusters_by_id: &HashMap<&str, &crate::model::LayoutCluster>,
    edges_by_id: &HashMap<&str, &LayoutEdge>,
) -> Result<()> {
    let tree = &ctx.layout.render_tree;
    if tree.roots.is_empty() || tree.top.0 >= tree.roots.len() {
        return Err(Error::InvalidModel {
            message: format!(
                "invalid Class render tree top {} for {} roots",
                tree.top.0,
                tree.roots.len()
            ),
        });
    }
    if layout_nodes_by_id.len() != ctx.layout.nodes.len()
        || clusters_by_id.len() != ctx.layout.clusters.len()
        || edges_by_id.len() != ctx.layout.edges.len()
    {
        return Err(Error::InvalidModel {
            message: "duplicate identifiers in Class layout artifact".to_string(),
        });
    }

    enum ValidationFrame {
        Enter(ClassRenderRootId),
        Exit(ClassRenderRootId),
    }

    let mut root_state = vec![0_u8; tree.roots.len()];
    let mut owned_nodes = HashSet::new();
    let mut owned_clusters = HashSet::new();
    let mut owned_edges = HashSet::new();
    let mut stack = vec![ValidationFrame::Enter(tree.top)];
    while let Some(frame) = stack.pop() {
        match frame {
            ValidationFrame::Enter(root_id) => {
                let Some(root) = tree.roots.get(root_id.0) else {
                    return Err(Error::InvalidModel {
                        message: format!("missing Class render root {}", root_id.0),
                    });
                };
                match root_state[root_id.0] {
                    1 => {
                        return Err(Error::InvalidModel {
                            message: format!("cycle in Class render tree at root {}", root_id.0),
                        });
                    }
                    2 => {
                        return Err(Error::InvalidModel {
                            message: format!("Class render root {} has multiple owners", root_id.0),
                        });
                    }
                    _ => {}
                }
                root_state[root_id.0] = 1;
                stack.push(ValidationFrame::Exit(root_id));

                if let Some(namespace_id) = root.namespace_id.as_deref()
                    && !clusters_by_id.contains_key(namespace_id)
                {
                    return Err(Error::InvalidModel {
                        message: format!(
                            "Class render root {} references missing namespace cluster {namespace_id}",
                            root_id.0
                        ),
                    });
                }
                for cluster_id in &root.cluster_ids {
                    if !clusters_by_id.contains_key(cluster_id.as_str()) {
                        return Err(Error::InvalidModel {
                            message: format!(
                                "Class render root {} references missing cluster {cluster_id}",
                                root_id.0
                            ),
                        });
                    }
                    if !owned_clusters.insert(cluster_id.as_str()) {
                        return Err(Error::InvalidModel {
                            message: format!(
                                "Class cluster {cluster_id} has multiple render owners"
                            ),
                        });
                    }
                }
                for edge_id in &root.edge_ids {
                    if !edges_by_id.contains_key(edge_id.as_str()) {
                        return Err(Error::InvalidModel {
                            message: format!(
                                "Class render root {} references missing edge {edge_id}",
                                root_id.0
                            ),
                        });
                    }
                    if !owned_edges.insert(edge_id.as_str()) {
                        return Err(Error::InvalidModel {
                            message: format!("Class edge {edge_id} has multiple render owners"),
                        });
                    }
                }
                for item in root.items.iter().rev() {
                    match item {
                        ClassRenderItem::Node(node_id) => {
                            let Some(node) = layout_nodes_by_id.get(node_id.as_str()) else {
                                return Err(Error::InvalidModel {
                                    message: format!(
                                        "Class render root {} references missing node {node_id}",
                                        root_id.0
                                    ),
                                });
                            };
                            if node.is_cluster {
                                return Err(Error::InvalidModel {
                                    message: format!(
                                        "Class render item {node_id} is a cluster, not a leaf node"
                                    ),
                                });
                            }
                            if !ctx.class_nodes_by_id.contains_key(node_id.as_str())
                                && !ctx.note_by_id.contains_key(node_id.as_str())
                                && !ctx.iface_by_id.contains_key(node_id.as_str())
                            {
                                return Err(Error::InvalidModel {
                                    message: format!(
                                        "Class render node {node_id} has no semantic node payload"
                                    ),
                                });
                            }
                            if !owned_nodes.insert(node_id.as_str()) {
                                return Err(Error::InvalidModel {
                                    message: format!(
                                        "Class node {node_id} has multiple render owners"
                                    ),
                                });
                            }
                        }
                        ClassRenderItem::Subgraph(child_id) => {
                            if child_id.0 >= tree.roots.len() {
                                return Err(Error::InvalidModel {
                                    message: format!(
                                        "Class render root {} references missing child root {}",
                                        root_id.0, child_id.0
                                    ),
                                });
                            }
                            stack.push(ValidationFrame::Enter(*child_id));
                        }
                    }
                }
            }
            ValidationFrame::Exit(root_id) => root_state[root_id.0] = 2,
        }
    }

    if let Some(unattached) = root_state.iter().position(|state| *state == 0) {
        return Err(Error::InvalidModel {
            message: format!("unattached Class render root {unattached}"),
        });
    }
    for node in &ctx.layout.nodes {
        if !node.is_cluster && !owned_nodes.contains(node.id.as_str()) {
            return Err(Error::InvalidModel {
                message: format!("Class node {} has no render owner", node.id),
            });
        }
    }
    for cluster in &ctx.layout.clusters {
        if !owned_clusters.contains(cluster.id.as_str()) {
            return Err(Error::InvalidModel {
                message: format!("Class cluster {} has no render owner", cluster.id),
            });
        }
    }
    for edge in &ctx.layout.edges {
        if !owned_edges.contains(edge.id.as_str()) {
            return Err(Error::InvalidModel {
                message: format!("Class edge {} has no render owner", edge.id),
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_class_split_edges_for_namespace(
    content_bounds: &mut Option<Bounds>,
    detail: &mut ClassRenderDetails,
    edge_ctx: &ClassSplitEdgeGroupsRenderContext<'_>,
    edges: &[LayoutEdge],
    root_dx: f64,
    root_dy: f64,
    in_namespace_root: bool,
) -> Result<super::groups::ClassSplitEdgeGroups> {
    let local_ctx = ClassSplitEdgeGroupsRenderContext {
        edges,
        missing_section_points: edge_ctx.missing_section_points,
        work_meter: edge_ctx.work_meter,
        line_hop_paths: edge_ctx.line_hop_paths,
        relations_by_id: edge_ctx.relations_by_id,
        relation_index_by_id: edge_ctx.relation_index_by_id,
        diagram_marker_class: edge_ctx.diagram_marker_class,
        diagram_id: edge_ctx.diagram_id,
        content_tx: if in_namespace_root {
            // The recursive Class root's x origin already folds in Dagre's fixed graph margin.
            // Adding the top-level content translation again moves only the edge route 8px to the
            // right of nodes rendered in the same local coordinate frame.
            -root_dx
        } else {
            edge_ctx.content_tx
        },
        content_ty: if in_namespace_root {
            edge_ctx.content_ty - root_dy
        } else {
            edge_ctx.content_ty
        },
        edge_use_html_labels: edge_ctx.edge_use_html_labels,
        text_measurer: edge_ctx.text_measurer,
        terminal_text_style: edge_ctx.terminal_text_style,
        mermaid_config: edge_ctx.mermaid_config,
        math_renderer: edge_ctx.math_renderer,
        look: edge_ctx.look,
        hand_drawn_seed: edge_ctx.hand_drawn_seed.clone(),
        timing: edge_ctx.timing,
        uses_elk_adapter_dom: edge_ctx.uses_elk_adapter_dom,
        edge_paths_class: edge_ctx.edge_paths_class,
        emit: edge_ctx.emit,
    };
    render_class_split_edge_groups(
        ClassSplitEdgeGroupsRenderState {
            content_bounds,
            detail,
        },
        &local_ctx,
        if in_namespace_root { root_dx } else { 0.0 },
        if in_namespace_root { root_dy } else { 0.0 },
    )
}

fn render_class_node_id(
    state: ClassNodesRenderState<'_>,
    ctx: &ClassNodesRenderContext<'_>,
    layout_nodes_by_id: &FxHashMap<&str, &crate::model::LayoutNode>,
    id: &str,
    offsets: ClassNodeRootOffsets,
) -> Result<()> {
    let ClassNodesRenderState {
        out,
        content_bounds,
        detail,
        sanitize_config,
        borrowed_sanitize_config,
    } = state;
    let settings = ctx.settings;

    let n = layout_nodes_by_id
        .get(id)
        .copied()
        .expect("validated Class render node id");

    let node_tx = if offsets.in_namespace_root {
        n.x - offsets.namespace_root_dx
    } else {
        n.x + ctx.content_tx
    };
    let node_ty = if offsets.in_namespace_root {
        n.y + ctx.content_ty - offsets.namespace_root_dy
    } else {
        n.y + ctx.content_ty
    };
    let node_bounds_tx = node_tx + offsets.namespace_root_dx;
    let node_bounds_ty = node_ty + offsets.namespace_root_dy;
    let position = ClassNodeRenderPosition {
        node_tx,
        node_ty,
        node_bounds_tx,
        node_bounds_ty,
    };

    if let Some(note) = ctx.note_by_id.get(n.id.as_str()).copied() {
        let stats = render_class_note_node(
            ClassNoteRenderState {
                out,
                content_bounds,
                sanitize_config,
                borrowed_sanitize_config,
            },
            note,
            n,
            position,
            &ClassNoteRenderContext {
                diagram_id: ctx.diagram_id,
                effective_config: ctx.effective_config,
                measurer: ctx.measurer,
                text_style: &settings.text_style,
                line_height: settings.line_height,
                use_html_labels: settings.diagram_use_html_labels
                    || crate::math::contains_delimited_math(&note.text),
                mermaid_config: ctx.mermaid_config,
                math_renderer: ctx.math_renderer,
                look: settings.look.as_str(),
                hand_drawn_seed: settings.hand_drawn_seed.clone(),
                timing: ctx.timing,
                emit: ctx.emit,
            },
        )?;
        detail.notes_sanitize += stats.notes_sanitize;
        detail.path_bounds += stats.path_bounds;
        detail.path_bounds_calls += stats.path_bounds_calls;
        return Ok(());
    }

    if let Some(iface) = ctx.iface_by_id.get(n.id.as_str()).copied() {
        render_class_interface_node(
            ClassInterfaceRenderState {
                out,
                content_bounds,
            },
            iface,
            n,
            position,
            &ClassInterfaceRenderContext {
                diagram_id: ctx.diagram_id,
                measurer: ctx.measurer,
                text_style: &settings.text_style,
                use_html_labels: settings.diagram_use_html_labels,
                wrapping_width: crate::class::config::ClassConfigView::new(ctx.effective_config)
                    .interface_wrapping_width(),
                look: settings.look.as_str(),
                mermaid_config: ctx.mermaid_config,
                math_renderer: ctx.math_renderer,
                emit: ctx.emit,
            },
        )?;
        return Ok(());
    }

    let node = ctx
        .class_nodes_by_id
        .get(n.id.as_str())
        .copied()
        .expect("validated Class semantic node payload");

    let node_inline_styles = class_apply_inline_styles(node);
    let node_style_attr = node_inline_styles.style_attr.as_str();
    let node_fill = node_inline_styles
        .fill
        .unwrap_or(settings.default_node_fill.as_str());
    let node_stroke = node_inline_styles
        .stroke
        .unwrap_or(settings.default_node_stroke.as_str());
    let node_stroke_width = node_inline_styles
        .stroke_width
        .unwrap_or("1.3")
        .trim_end_matches("px")
        .trim();
    let node_stroke_dasharray = node_inline_styles.stroke_dasharray.unwrap_or("0 0");

    let node_link_open = render_class_node_shell_open(
        out,
        node,
        position,
        &ClassNodeShellContext {
            diagram_id: ctx.diagram_id,
            emit: ctx.emit,
            look: settings.look.as_str(),
            security_level_loose: settings.security_level_loose,
            color_index: ctx.class_color_indices.get(n.id.as_str()).copied(),
            palette_size: super::css::class_palette_size(ctx.effective_config),
        },
    )?;
    let basic_container = render_class_node_basic_container(
        ClassNodeRenderState {
            out,
            content_bounds,
        },
        node,
        n,
        position,
        &ClassNodeBasicContainerContext {
            diagram_id: ctx.diagram_id,
            node_style_attr,
            node_fill,
            node_stroke,
            node_stroke_width,
            node_stroke_dasharray,
            look: settings.look.as_str(),
            hand_drawn_seed: settings.hand_drawn_seed.clone(),
            timing: ctx.timing,
        },
    );
    detail.path_bounds += basic_container.stats.path_bounds;
    detail.path_bounds_calls += basic_container.stats.path_bounds_calls;

    if settings.diagram_use_html_labels || crate::class::class_node_requires_math(node) {
        let html_stats = render_class_html_node_body(
            ClassNodeRenderState {
                out,
                content_bounds,
            },
            position,
            node,
            basic_container.geometry,
            ctx.layout
                .class_label_plans_by_id
                .get(n.id.as_str())
                .map(|plan| plan.as_ref()),
            &ClassHtmlNodeBodyContext {
                measurer: ctx.measurer,
                text_style: &settings.text_style,
                html_calc_text_style: &settings.html_calc_text_style,
                line_height: settings.line_height,
                class_padding: settings.class_padding,
                hide_empty_members_box: settings.hide_empty_members_box,
                node_style_attr,
                node_stroke,
                node_stroke_width,
                node_stroke_dasharray,
                look: settings.look.as_str(),
                use_gradient: config_bool(ctx.effective_config, &["themeVariables", "useGradient"])
                    .unwrap_or(false),
                mermaid_config: ctx.mermaid_config,
                math_renderer: ctx.math_renderer,
                timing: ctx.timing,
            },
        );
        detail.path_bounds += html_stats.path_bounds;
        detail.path_bounds_calls += html_stats.path_bounds_calls;
    } else {
        let svg_stats = render_class_svg_node_body(
            ClassNodeRenderState {
                out,
                content_bounds,
            },
            position,
            node,
            basic_container.geometry,
            &ClassSvgNodeBodyContext {
                measurer: ctx.measurer,
                text_style: &settings.text_style,
                wrap_probe_font_size: settings.wrap_probe_font_size,
                class_padding: settings.class_padding,
                hide_empty_members_box: settings.hide_empty_members_box,
                node_style_attr,
                node_stroke,
                node_stroke_width,
                node_stroke_dasharray,
                look: settings.look.as_str(),
                use_gradient: config_bool(ctx.effective_config, &["themeVariables", "useGradient"])
                    .unwrap_or(false),
                timing: ctx.timing,
            },
        );
        detail.path_bounds += svg_stats.path_bounds;
        detail.path_bounds_calls += svg_stats.path_bounds_calls;
    }

    out.push_str("</g>");
    if node_link_open {
        out.push_str("</a>");
    }
    Ok(())
}
