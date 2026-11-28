use crate::model::{Bounds, TreeViewDiagramLayout, TreeViewLineLayout, TreeViewNodeLayout};
use crate::text::{TextMeasurer, TextStyle};
use crate::{Error, Result};
use merman_core::MAX_DIAGRAM_NESTING_DEPTH;
use merman_core::diagrams::tree_view::{
    TreeViewDiagramRenderModel, TreeViewNodeRenderModel as TreeViewNode,
};
use serde_json::Value;
use std::collections::HashMap;

mod config;

use config::{TreeViewConfigView, TreeViewLayoutSettings};

const TREE_VIEW_DIRECTORY_NODE_TYPE: &str = "directory";
const TREE_VIEW_FILE_NODE_TYPE: &str = "file";
const TREE_VIEW_ICON_PREFIX: &str = "mermaid-treeview";
pub(crate) const TREE_VIEW_ICON_SIZE: f64 = 14.0;
const TREE_VIEW_ICON_GAP: f64 = 4.0;
const TREE_VIEW_DESCRIPTION_GAP: f64 = 16.0;
pub(crate) const TREE_VIEW_DIRECTORY_FONT_WEIGHT: &str = "bold";
pub(crate) const TREE_VIEW_DESCRIPTION_FONT_STYLE: &str = "italic";
// Mermaid extends each highlight past the current tree width, then reserves room for its stroke.
pub(crate) const TREE_VIEW_HIGHLIGHT_RECT_EXTENSION: f64 = 8.0;
pub(crate) const TREE_VIEW_HIGHLIGHT_VIEWPORT_CLEARANCE: f64 = 2.0;
pub(crate) const TREE_VIEW_HIGHLIGHT_WIDTH_GROWTH: f64 =
    TREE_VIEW_HIGHLIGHT_RECT_EXTENSION + TREE_VIEW_HIGHLIGHT_VIEWPORT_CLEARANCE;

pub(crate) fn layout_tree_view_diagram_typed(
    model: &TreeViewDiagramRenderModel,
    effective_config: &Value,
    measurer: &dyn TextMeasurer,
) -> Result<TreeViewDiagramLayout> {
    let cfg = TreeViewConfigView::new(effective_config).layout_settings();
    validate_tree_view_render_depth(&model.root)?;
    let label_style = TextStyle {
        font_family: Some(cfg.font_family.clone()),
        font_size: cfg.label_font_size,
        ..Default::default()
    };
    let mut directory_label_style = label_style.clone();
    directory_label_style.font_weight = Some(TREE_VIEW_DIRECTORY_FONT_WEIGHT.to_string());
    let mut description_style = label_style.clone();
    description_style.font_style = Some(TREE_VIEW_DESCRIPTION_FONT_STYLE.to_string());
    let mut ctx = LayoutCtx {
        cfg: cfg.clone(),
        measurer,
        label_style,
        directory_label_style,
        description_style,
        total_height: 0.0,
        total_width: 0.0,
        nodes: Vec::new(),
        lines: Vec::new(),
    };

    layout_tree(&mut ctx, &model.root);

    let min_x = -ctx.cfg.line_thickness / 2.0;
    align_tree_view_descriptions(&mut ctx);
    let highlighted_node_count = ctx
        .nodes
        .iter()
        .filter(|node| is_tree_view_highlight_class(node.css_class.as_deref()))
        .count();
    let total_width =
        ctx.total_width.max(1.0) + highlighted_node_count as f64 * TREE_VIEW_HIGHLIGHT_WIDTH_GROWTH;
    let total_height = ctx.total_height.max(1.0);
    Ok(TreeViewDiagramLayout {
        bounds: Some(Bounds {
            min_x,
            min_y: 0.0,
            max_x: total_width,
            max_y: total_height,
        }),
        total_width,
        total_height,
        row_indent: ctx.cfg.row_indent,
        padding_x: ctx.cfg.padding_x,
        padding_y: ctx.cfg.padding_y,
        line_thickness: ctx.cfg.line_thickness,
        use_max_width: ctx.cfg.use_max_width,
        label_font_size: ctx.cfg.label_font_size,
        nodes: ctx.nodes,
        lines: ctx.lines,
    })
}

fn validate_tree_view_render_depth(root: &TreeViewNode) -> Result<()> {
    let mut stack = vec![(root, 0usize)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DIAGRAM_NESTING_DEPTH {
            return Err(Error::InvalidModel {
                message: format!("treeView nesting depth exceeds {MAX_DIAGRAM_NESTING_DEPTH}"),
            });
        }
        for child in &node.children {
            stack.push((child, depth.saturating_add(1)));
        }
    }
    Ok(())
}

struct LayoutCtx<'a> {
    cfg: TreeViewLayoutSettings,
    measurer: &'a dyn TextMeasurer,
    label_style: TextStyle,
    directory_label_style: TextStyle,
    description_style: TextStyle,
    total_height: f64,
    total_width: f64,
    nodes: Vec<TreeViewNodeLayout>,
    lines: Vec<TreeViewLineLayout>,
}

enum LayoutFrame<'a> {
    Enter {
        node: &'a TreeViewNode,
        depth: usize,
    },
    Exit {
        node: &'a TreeViewNode,
        node_index: usize,
    },
}

fn layout_tree(ctx: &mut LayoutCtx<'_>, root: &TreeViewNode) {
    let mut stack = vec![LayoutFrame::Enter {
        node: root,
        depth: 0,
    }];
    let mut node_indices: HashMap<*const TreeViewNode, usize> = HashMap::new();

    while let Some(frame) = stack.pop() {
        match frame {
            LayoutFrame::Enter { node, depth } => {
                let node_index = push_node_layout(ctx, node, depth);
                node_indices.insert(std::ptr::from_ref(node), node_index);
                stack.push(LayoutFrame::Exit { node, node_index });
                for child in node.children.iter().rev() {
                    stack.push(LayoutFrame::Enter {
                        node: child,
                        depth: depth.saturating_add(1),
                    });
                }
            }
            LayoutFrame::Exit { node, node_index } => {
                if let Some(last_child) = node.children.last()
                    && let Some(last_child_idx) =
                        node_indices.get(&std::ptr::from_ref(last_child)).copied()
                {
                    push_vertical_line(ctx, node_index, last_child_idx);
                }
            }
        }
    }
}

fn push_node_layout(ctx: &mut LayoutCtx<'_>, node: &TreeViewNode, depth: usize) -> usize {
    let indent = depth as f64 * (ctx.cfg.row_indent + ctx.cfg.padding_x);
    let resolved_icon = resolve_tree_view_node_icon(node, &ctx.cfg);
    let icon_offset = if resolved_icon.is_some() {
        TREE_VIEW_ICON_SIZE + TREE_VIEW_ICON_GAP
    } else {
        0.0
    };
    let label_style = if node.node_type == TREE_VIEW_DIRECTORY_NODE_TYPE {
        &ctx.directory_label_style
    } else {
        &ctx.label_style
    };
    let label_width = tree_view_label_bbox_width_px(ctx.measurer, &node.name, label_style);
    let label_height = ctx
        .measurer
        .measure_svg_raw_text_bbox_height_px(&node.name, label_style)
        .max(0.0);
    let height = label_height + ctx.cfg.padding_y * 2.0;
    let width = label_width + ctx.cfg.padding_x * 2.0 + icon_offset;
    let y = ctx.total_height;
    let idx = ctx.nodes.len();

    ctx.nodes.push(TreeViewNodeLayout {
        id: node.id,
        level: node.level,
        name: node.name.clone(),
        node_type: node.node_type.clone(),
        css_class: node.css_class.clone(),
        icon: node.icon.clone(),
        resolved_icon,
        description: node.description.clone(),
        depth,
        x: indent,
        y,
        width,
        height,
        label_x: indent + ctx.cfg.padding_x + icon_offset,
        label_y: y + height / 2.0,
        label_width,
        label_height,
        description_x: None,
        description_width: None,
    });
    ctx.lines.push(TreeViewLineLayout {
        x1: indent - ctx.cfg.row_indent,
        y1: y + height / 2.0,
        x2: indent,
        y2: y + height / 2.0,
        stroke_width: ctx.cfg.line_thickness,
        kind: "horizontal".to_string(),
    });

    ctx.total_width = ctx.total_width.max(indent + width);
    ctx.total_height += height;

    idx
}

fn align_tree_view_descriptions(ctx: &mut LayoutCtx<'_>) {
    if !ctx.nodes.iter().any(|node| node.description.is_some()) {
        return;
    }
    let max_label_right = ctx
        .nodes
        .iter()
        .map(|node| node.label_x + node.label_width)
        .fold(0.0, f64::max);
    let description_x = max_label_right + TREE_VIEW_DESCRIPTION_GAP;
    for node in &mut ctx.nodes {
        let Some(description) = &node.description else {
            continue;
        };
        let description_width =
            tree_view_label_bbox_width_px(ctx.measurer, description, &ctx.description_style);
        node.description_x = Some(description_x);
        node.description_width = Some(description_width);
        ctx.total_width = ctx
            .total_width
            .max(description_x + description_width + ctx.cfg.padding_x);
    }
}

fn push_vertical_line(ctx: &mut LayoutCtx<'_>, node_index: usize, last_child_idx: usize) {
    let Some(current) = ctx.nodes.get(node_index) else {
        return;
    };
    let Some(last_child) = ctx.nodes.get(last_child_idx) else {
        return;
    };
    let current_x = current.x;
    let current_y = current.y;
    let current_height = current.height;
    let last_child_y = last_child.y;
    let last_child_height = last_child.height;

    ctx.lines.push(TreeViewLineLayout {
        x1: current_x + ctx.cfg.padding_x,
        y1: current_y + current_height,
        x2: current_x + ctx.cfg.padding_x,
        y2: last_child_y + last_child_height / 2.0 + ctx.cfg.line_thickness / 2.0,
        stroke_width: ctx.cfg.line_thickness,
        kind: "vertical".to_string(),
    });
}

fn tree_view_label_bbox_width_px(
    measurer: &dyn TextMeasurer,
    label: &str,
    style: &TextStyle,
) -> f64 {
    measurer
        .measure_svg_raw_text_bbox_width_px(label, style)
        .max(0.0)
}

fn resolve_tree_view_node_icon(
    node: &TreeViewNode,
    cfg: &TreeViewLayoutSettings,
) -> Option<String> {
    if node.icon.as_deref() == Some("none") {
        return None;
    }
    if let Some(icon) = node.icon.as_deref().filter(|icon| !icon.is_empty()) {
        return Some(qualify_tree_view_icon(icon, &cfg.default_icon_pack));
    }
    if !cfg.show_icons {
        return None;
    }
    if node.node_type == TREE_VIEW_FILE_NODE_TYPE
        && let Some(icon) = detect_tree_view_file_icon(&node.name, cfg)
    {
        if icon == "none" {
            return None;
        }
        return Some(qualify_tree_view_icon(&icon, &cfg.default_icon_pack));
    }
    let fallback = if node.node_type == TREE_VIEW_DIRECTORY_NODE_TYPE {
        "folder"
    } else {
        "file"
    };
    Some(format!("{TREE_VIEW_ICON_PREFIX}:{fallback}"))
}

fn detect_tree_view_file_icon(name: &str, cfg: &TreeViewLayoutSettings) -> Option<String> {
    if let Some(icon) = cfg.filename_icons.get(name).filter(|icon| !icon.is_empty()) {
        return Some(icon.clone());
    }
    let dot_idx = name.rfind('.')?;
    if dot_idx == 0 {
        return None;
    }
    let ext = name[dot_idx..].to_ascii_lowercase();
    cfg.extension_icons
        .get(&ext)
        .or_else(|| cfg.extension_icons.get(&ext[1..]))
        .filter(|icon| !icon.is_empty())
        .cloned()
}

fn qualify_tree_view_icon(icon: &str, default_icon_pack: &str) -> String {
    if icon.contains(':') {
        return icon.to_string();
    }
    if matches!(icon, "file" | "folder") || default_icon_pack.is_empty() {
        return format!("{TREE_VIEW_ICON_PREFIX}:{icon}");
    }
    format!("{default_icon_pack}:{icon}")
}

pub(crate) fn is_tree_view_highlight_class(css_class: Option<&str>) -> bool {
    css_class.is_some_and(|class| class.split_whitespace().any(|part| part == "highlight"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{RenderEnvironment, TextMeasurementPhase};

    #[test]
    fn layout_rejects_typed_model_beyond_nesting_limit() {
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let mut child = TreeViewNode {
            id: (MAX_DIAGRAM_NESTING_DEPTH + 1) as i64,
            level: (MAX_DIAGRAM_NESTING_DEPTH + 1) as i64,
            name: "leaf".to_string(),
            children: Vec::new(),
            ..Default::default()
        };
        for depth in (0..=MAX_DIAGRAM_NESTING_DEPTH).rev() {
            child = TreeViewNode {
                id: depth as i64,
                level: depth as i64,
                name: format!("n{depth}"),
                children: vec![child],
                ..Default::default()
            };
        }

        let model = TreeViewDiagramRenderModel {
            root: TreeViewNode {
                children: vec![child],
                ..Default::default()
            },
            ..Default::default()
        };

        let error =
            layout_tree_view_diagram_typed(&model, &Value::Object(Default::default()), &measurer)
                .unwrap_err();
        assert!(
            error.to_string().contains("treeView nesting depth exceeds"),
            "{error}"
        );
    }
}
