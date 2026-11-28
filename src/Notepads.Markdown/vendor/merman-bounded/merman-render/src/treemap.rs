use crate::config::json_f64;
use crate::model::{TreemapDiagramLayout, TreemapLeafLayout, TreemapSectionLayout};
use crate::{Error, Result};
use merman_core::diagrams::treemap::TreemapDiagramRenderModel;
use serde_json::Value;

pub(crate) const TREEMAP_SECTION_INNER_PADDING_PX: f64 = 10.0;
pub(crate) const TREEMAP_SECTION_HEADER_HEIGHT_PX: f64 = 25.0;

mod config;

use config::TreemapConfigView;

#[derive(Debug, Clone)]
struct HierNode {
    name: String,
    own_value: f64,
    value: f64,
    class_selector: Option<String>,
    css_compiled_styles: Option<Vec<String>>,
    parent: Option<usize>,
    children: Vec<usize>,
    depth: usize,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

fn push_node(nodes: &mut Vec<HierNode>, model: &TreemapDiagramRenderModel) -> Result<()> {
    let mut stack = vec![(&model.root, None, 0usize)];
    let mut seen = vec![false; model.nodes.len()];
    while let Some((current, parent_idx, current_depth)) = stack.pop() {
        let own_value = current.value.as_ref().and_then(json_f64).unwrap_or(0.0);
        let idx = nodes.len();
        nodes.push(HierNode {
            name: current.name.clone(),
            own_value,
            value: 0.0,
            class_selector: current.class_selector.clone(),
            css_compiled_styles: current.css_compiled_styles.clone(),
            parent: parent_idx,
            children: Vec::new(),
            depth: current_depth,
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });

        if let Some(parent_idx) = parent_idx
            && let Some(parent_node) = nodes.get_mut(parent_idx)
        {
            parent_node.children.push(idx);
        }

        if let Some(children) = current.children.as_ref() {
            for &child in children.iter().rev() {
                let Some(node) = model.nodes.get(child) else {
                    return Err(Error::InvalidModel {
                        message: "treemap child ID is out of range".to_string(),
                    });
                };
                if std::mem::replace(&mut seen[child], true) {
                    return Err(Error::InvalidModel {
                        message: "cyclic or repeated treemap child relationship".to_string(),
                    });
                }
                stack.push((node, Some(idx), current_depth.saturating_add(1)));
            }
        }
    }
    Ok(())
}

fn compute_sum(nodes: &mut [HierNode], idx: usize) -> f64 {
    let mut stack = vec![(idx, false)];
    while let Some((node_idx, visited)) = stack.pop() {
        let Some(node) = nodes.get(node_idx) else {
            continue;
        };

        if visited {
            let sum = node.own_value
                + node
                    .children
                    .iter()
                    .filter_map(|&child_idx| nodes.get(child_idx).map(|child| child.value))
                    .sum::<f64>();
            if let Some(node) = nodes.get_mut(node_idx) {
                node.value = sum;
            }
        } else {
            stack.push((node_idx, true));
            for &child_idx in node.children.iter().rev() {
                stack.push((child_idx, false));
            }
        }
    }

    nodes.get(idx).map(|node| node.value).unwrap_or(0.0)
}

fn sort_children_by_value(nodes: &mut [HierNode], idx: usize) {
    let mut stack = vec![idx];
    while let Some(node_idx) = stack.pop() {
        if node_idx >= nodes.len() {
            continue;
        }

        let mut items = nodes[node_idx]
            .children
            .iter()
            .copied()
            .enumerate()
            .map(|(pos, child)| (child, pos))
            .collect::<Vec<_>>();
        items.sort_by(|(a, a_pos), (b, b_pos)| {
            let av = nodes[*a].value;
            let bv = nodes[*b].value;
            bv.partial_cmp(&av)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a_pos.cmp(b_pos))
        });
        nodes[node_idx].children = items.into_iter().map(|(child, _pos)| child).collect();

        let children = nodes[node_idx].children.clone();
        for child_idx in children.into_iter().rev() {
            stack.push(child_idx);
        }
    }
}

fn each_before(nodes: &[HierNode], root: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(idx) = stack.pop() {
        out.push(idx);
        let children = &nodes[idx].children;
        for &c in children.iter().rev() {
            stack.push(c);
        }
    }
    out
}

fn descendants_bfs(nodes: &[HierNode], root: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut next = vec![root];
    while !next.is_empty() {
        let mut current = next;
        current.reverse();
        next = Vec::new();
        while let Some(idx) = current.pop() {
            out.push(idx);
            for &c in &nodes[idx].children {
                next.push(c);
            }
        }
    }
    out
}

fn leaves_each_before(nodes: &[HierNode], root: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for idx in each_before(nodes, root) {
        if nodes[idx].children.is_empty() {
            out.push(idx);
        }
    }
    out
}

fn treemap_round_node(nodes: &mut [HierNode], idx: usize) {
    nodes[idx].x0 = nodes[idx].x0.round();
    nodes[idx].y0 = nodes[idx].y0.round();
    nodes[idx].x1 = nodes[idx].x1.round();
    nodes[idx].y1 = nodes[idx].y1.round();
}

fn treemap_dice(
    nodes: &mut [HierNode],
    children: &[usize],
    row_value: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    let mut x = x0;
    let k = if row_value != 0.0 {
        (x1 - x0) / row_value
    } else {
        0.0
    };
    for &child in children {
        nodes[child].y0 = y0;
        nodes[child].y1 = y1;
        nodes[child].x0 = x;
        x += nodes[child].value * k;
        nodes[child].x1 = x;
    }
}

fn treemap_slice(
    nodes: &mut [HierNode],
    children: &[usize],
    row_value: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    let mut y = y0;
    let k = if row_value != 0.0 {
        (y1 - y0) / row_value
    } else {
        0.0
    };
    for &child in children {
        nodes[child].x0 = x0;
        nodes[child].x1 = x1;
        nodes[child].y0 = y;
        y += nodes[child].value * k;
        nodes[child].y1 = y;
    }
}

fn squarify(nodes: &mut [HierNode], parent: usize, mut x0: f64, mut y0: f64, x1: f64, y1: f64) {
    const PHI: f64 = (1.0 + 2.23606797749979) / 2.0;
    let ratio = PHI;

    let children = nodes[parent].children.clone();
    if children.is_empty() {
        return;
    }

    let n = children.len();
    let mut i0 = 0usize;
    let mut i1 = 0usize;
    let mut value = nodes[parent].value;

    while i0 < n {
        let dx = x1 - x0;
        let dy = y1 - y0;

        let mut sum_value;
        loop {
            if i1 >= n {
                return;
            }
            sum_value = nodes[children[i1]].value;
            i1 += 1;
            if sum_value != 0.0 || i1 >= n {
                break;
            }
        }

        let mut min_value = sum_value;
        let mut max_value = sum_value;

        let alpha = (dy / dx).max(dx / dy) / (value * ratio);
        let mut beta = sum_value * sum_value * alpha;
        let mut min_ratio = (max_value / beta).max(beta / min_value);

        while i1 < n {
            let node_value = nodes[children[i1]].value;
            sum_value += node_value;
            if node_value < min_value {
                min_value = node_value;
            }
            if node_value > max_value {
                max_value = node_value;
            }
            beta = sum_value * sum_value * alpha;
            let new_ratio = (max_value / beta).max(beta / min_value);
            if new_ratio > min_ratio {
                sum_value -= node_value;
                break;
            }
            min_ratio = new_ratio;
            i1 += 1;
        }

        let dice = dx < dy;
        let row_children = &children[i0..i1];
        if dice {
            let y2 = if value != 0.0 {
                y0 + dy * sum_value / value
            } else {
                y1
            };
            treemap_dice(nodes, row_children, sum_value, x0, y0, x1, y2);
            y0 = y2;
        } else {
            let x2 = if value != 0.0 {
                x0 + dx * sum_value / value
            } else {
                x1
            };
            treemap_slice(nodes, row_children, sum_value, x0, y0, x2, y1);
            x0 = x2;
        }

        value -= sum_value;
        i0 = i1;
    }
}

fn position_node(
    nodes: &mut [HierNode],
    idx: usize,
    padding_stack: &mut Vec<f64>,
    padding_inner: f64,
) {
    let depth = nodes[idx].depth;
    if padding_stack.len() <= depth {
        padding_stack.resize(depth + 1, 0.0);
    }
    let mut p = padding_stack[depth];
    let mut x0 = nodes[idx].x0 + p;
    let mut y0 = nodes[idx].y0 + p;
    let mut x1 = nodes[idx].x1 - p;
    let mut y1 = nodes[idx].y1 - p;
    if x1 < x0 {
        x0 = (x0 + x1) / 2.0;
        x1 = x0;
    }
    if y1 < y0 {
        y0 = (y0 + y1) / 2.0;
        y1 = y0;
    }
    nodes[idx].x0 = x0;
    nodes[idx].y0 = y0;
    nodes[idx].x1 = x1;
    nodes[idx].y1 = y1;

    if nodes[idx].children.is_empty() {
        return;
    }

    p = padding_inner / 2.0;
    if padding_stack.len() <= depth + 1 {
        padding_stack.resize(depth + 2, 0.0);
    }
    padding_stack[depth + 1] = p;

    let has_children = true;
    let padding_top = if has_children {
        TREEMAP_SECTION_HEADER_HEIGHT_PX + TREEMAP_SECTION_INNER_PADDING_PX
    } else {
        0.0
    };
    let padding_lr = if has_children {
        TREEMAP_SECTION_INNER_PADDING_PX
    } else {
        0.0
    };
    let padding_bottom = if has_children {
        TREEMAP_SECTION_INNER_PADDING_PX
    } else {
        0.0
    };

    x0 += padding_lr - p;
    y0 += padding_top - p;
    x1 -= padding_lr - p;
    y1 -= padding_bottom - p;
    if x1 < x0 {
        x0 = (x0 + x1) / 2.0;
        x1 = x0;
    }
    if y1 < y0 {
        y0 = (y0 + y1) / 2.0;
        y1 = y0;
    }

    squarify(nodes, idx, x0, y0, x1, y1);
}

pub(crate) fn layout_treemap_diagram_typed(
    model: &TreemapDiagramRenderModel,
    diagram_title: Option<&str>,
    effective_config: &Value,
    _measurer: &dyn crate::text::TextMeasurer,
) -> Result<TreemapDiagramLayout> {
    let cfg = TreemapConfigView::new(effective_config).layout_settings();
    let title = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| {
            diagram_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
        })
        .map(str::to_owned);

    let title_height = if title.is_some() { 30.0 } else { 0.0 };

    let width = if cfg.node_width > 0.0 {
        cfg.node_width * TREEMAP_SECTION_INNER_PADDING_PX
    } else {
        960.0
    };
    let height = if cfg.node_height > 0.0 {
        cfg.node_height * TREEMAP_SECTION_INNER_PADDING_PX
    } else {
        500.0
    };

    let mut nodes: Vec<HierNode> = Vec::new();
    push_node(&mut nodes, model)?;
    if nodes.is_empty() {
        return Err(Error::InvalidModel {
            message: "treemap root produced no nodes".to_string(),
        });
    }
    let root_idx = 0usize;

    compute_sum(&mut nodes, root_idx);
    sort_children_by_value(&mut nodes, root_idx);

    nodes[root_idx].x0 = 0.0;
    nodes[root_idx].y0 = 0.0;
    nodes[root_idx].x1 = width;
    nodes[root_idx].y1 = height;

    let mut padding_stack = vec![0.0];
    for idx in each_before(&nodes, root_idx) {
        position_node(&mut nodes, idx, &mut padding_stack, cfg.padding.max(0.0));
    }

    for idx in each_before(&nodes, root_idx) {
        treemap_round_node(&mut nodes, idx);
    }

    let branch_nodes = descendants_bfs(&nodes, root_idx)
        .into_iter()
        .filter(|&idx| !nodes[idx].children.is_empty())
        .collect::<Vec<_>>();

    let leaf_nodes = leaves_each_before(&nodes, root_idx);

    let mut sections = Vec::new();
    for idx in &branch_nodes {
        let n = &nodes[*idx];
        sections.push(TreemapSectionLayout {
            name: n.name.clone(),
            depth: n.depth as i64,
            value: n.value,
            x0: n.x0,
            y0: n.y0,
            x1: n.x1,
            y1: n.y1,
            class_selector: n.class_selector.clone(),
            css_compiled_styles: n.css_compiled_styles.clone(),
        });
    }

    let mut leaves = Vec::new();
    for idx in &leaf_nodes {
        let n = &nodes[*idx];
        leaves.push(TreemapLeafLayout {
            name: n.name.clone(),
            value: n.value,
            parent_name: n.parent.map(|p| nodes[p].name.clone()),
            x0: n.x0,
            y0: n.y0,
            x1: n.x1,
            y1: n.y1,
            class_selector: n.class_selector.clone(),
            css_compiled_styles: n.css_compiled_styles.clone(),
        });
    }

    Ok(TreemapDiagramLayout {
        title_height,
        width,
        height,
        use_max_width: cfg.use_max_width,
        diagram_padding: cfg.diagram_padding.max(0.0),
        show_values: cfg.show_values,
        value_format: cfg.value_format,
        acc_title: model.acc_title.clone(),
        acc_descr: model.acc_descr.clone(),
        title,
        sections,
        leaves,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn treemap_geometry_constants_match_mermaid() {
        assert_eq!(super::TREEMAP_SECTION_INNER_PADDING_PX, 10.0);
        assert_eq!(super::TREEMAP_SECTION_HEADER_HEIGHT_PX, 25.0);
    }
}
