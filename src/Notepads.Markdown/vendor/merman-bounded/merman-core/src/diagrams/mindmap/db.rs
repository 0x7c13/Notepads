use super::render_model::{MindmapDiagramRenderEdge, MindmapDiagramRenderNode};
use super::utils::get_i64;
use super::{
    NODE_TYPE_BANG, NODE_TYPE_CIRCLE, NODE_TYPE_CLOUD, NODE_TYPE_DEFAULT, NODE_TYPE_HEXAGON,
    NODE_TYPE_RECT, NODE_TYPE_ROUNDED_RECT,
};
use crate::sanitize::sanitize_text;
use crate::{Error, MermaidConfig, OperationControl, OperationControlResult, Result};

const MINDMAP_SECTION_COUNT: usize = 11;

#[derive(Debug, Clone, Copy)]
pub(super) struct MindmapParseConfig {
    padding: i64,
    max_node_width: i64,
}

impl MindmapParseConfig {
    pub(super) fn from_config(config: &MermaidConfig) -> Self {
        Self {
            padding: get_i64(config, "mindmap.padding").unwrap_or(10),
            max_node_width: get_i64(config, "mindmap.maxNodeWidth").unwrap_or(200),
        }
    }
}

fn mindmap_look(config: &MermaidConfig) -> String {
    config.get_str("look").unwrap_or("classic").to_string()
}

fn mindmap_default_shape(config: &MermaidConfig) -> &'static str {
    let theme = config
        .get_str("theme")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if theme.contains("redux") {
        "rounded"
    } else {
        "defaultMindmapNode"
    }
}

fn shape_from_type(ty: i32, default_shape: &'static str) -> &'static str {
    match ty {
        NODE_TYPE_CIRCLE => "mindmapCircle",
        NODE_TYPE_RECT => "rect",
        NODE_TYPE_ROUNDED_RECT => "rounded",
        NODE_TYPE_CLOUD => "cloud",
        NODE_TYPE_BANG => "bang",
        NODE_TYPE_HEXAGON => "hexagon",
        NODE_TYPE_DEFAULT => default_shape,
        _ => "rect",
    }
}

#[derive(Debug, Clone)]
pub(super) struct MindmapNode {
    pub(super) id: i32,
    pub(super) node_id: String,
    pub(super) level: i32,
    pub(super) descr: String,
    pub(super) ty: i32,
    pub(super) children: Vec<i32>,
    pub(super) width: i64,
    pub(super) padding: i64,
    pub(super) section: Option<i32>,
    pub(super) height: Option<i64>,
    pub(super) class: Option<String>,
    pub(super) icon: Option<String>,
    pub(super) x: Option<f64>,
    pub(super) y: Option<f64>,
    pub(super) is_root: bool,
}

fn mindmap_node_css_classes(node: &MindmapNode) -> String {
    let mut css = vec!["mindmap-node".to_string()];
    if node.is_root {
        css.push("section-root".to_string());
        css.push("section--1".to_string());
    } else if let Some(section) = node.section {
        css.push(format!("section-{section}"));
    }
    if let Some(cls) = &node.class {
        css.push(cls.clone());
    }
    css.join(" ")
}

fn mindmap_render_node(
    node: &MindmapNode,
    look: &str,
    default_shape: &'static str,
) -> MindmapDiagramRenderNode {
    MindmapDiagramRenderNode {
        id: node.id.to_string(),
        dom_id: format!("node_{}", node.id),
        label: node.descr.clone(),
        // Mermaid's `flattenNodes()` marks every layout node as Markdown. The parser's quoted
        // description flag only controls sanitization while the node enters this DB.
        label_type: "markdown".to_string(),
        is_group: false,
        shape: shape_from_type(node.ty, default_shape).to_string(),
        width: node.width as f64,
        height: node.height.unwrap_or(0) as f64,
        padding: node.padding as f64,
        css_classes: mindmap_node_css_classes(node),
        css_styles: Vec::new(),
        look: look.to_string(),
        icon: node.icon.clone(),
        x: node.x,
        y: node.y,
        level: node.level as i64,
        node_id: node.node_id.clone(),
        node_type: node.ty,
        section: node.section,
    }
}

fn mindmap_edge_classes(parent: &MindmapNode, child: &MindmapNode) -> String {
    let mut classes = "edge".to_string();
    if let Some(section) = child.section {
        classes.push_str(&format!(" section-edge-{section}"));
    }
    let edge_depth = parent.level + 1;
    classes.push_str(&format!(" edge-depth-{edge_depth}"));
    classes
}

fn mindmap_render_edge(
    parent: &MindmapNode,
    child: &MindmapNode,
    look: &str,
) -> MindmapDiagramRenderEdge {
    MindmapDiagramRenderEdge {
        id: format!("edge_{}_{}", parent.id, child.id),
        start: parent.id.to_string(),
        end: child.id.to_string(),
        edge_type: "normal".to_string(),
        curve: "basis".to_string(),
        thickness: "normal".to_string(),
        look: look.to_string(),
        classes: mindmap_edge_classes(parent, child),
        depth: parent.level as i64,
        section: child.section,
    }
}

#[derive(Debug, Default)]
pub(super) struct MindmapDb {
    pub(super) nodes: Vec<MindmapNode>,
    base_level: Option<i32>,
    ancestry: Vec<(i32, usize)>,
}

pub(super) struct MindmapNodeInput<'a> {
    pub(super) indent_level: i32,
    pub(super) id_raw: &'a str,
    pub(super) descr_raw: &'a str,
    pub(super) descr_is_markdown: bool,
    pub(super) ty: i32,
    pub(super) diagram_type: &'a str,
}

impl MindmapDb {
    pub(super) fn clear(&mut self) {
        self.nodes.clear();
        self.base_level = None;
        self.ancestry.clear();
    }

    pub(super) fn get_mindmap(&self) -> Option<&MindmapNode> {
        self.nodes.first()
    }

    #[cfg(test)]
    pub(super) fn add_node(
        &mut self,
        input: MindmapNodeInput<'_>,
        config: &MermaidConfig,
        parse_config: MindmapParseConfig,
    ) -> Result<()> {
        let control = OperationControl::new();
        self.add_node_controlled(input, config, parse_config, &control)
            .expect("a private parse control cannot be cancelled")
    }

    pub(super) fn add_node_controlled(
        &mut self,
        input: MindmapNodeInput<'_>,
        config: &MermaidConfig,
        parse_config: MindmapParseConfig,
        control: &OperationControl,
    ) -> OperationControlResult<Result<()>> {
        control.checkpoint()?;
        let mut level = input.indent_level;
        let is_root;
        if self.nodes.is_empty() {
            self.base_level = Some(level);
            level = 0;
            is_root = true;
        } else if let Some(base) = self.base_level {
            level -= base;
            is_root = false;
        } else {
            is_root = false;
        }

        let mut padding = parse_config.padding;
        let width = parse_config.max_node_width;

        match input.ty {
            NODE_TYPE_ROUNDED_RECT | NODE_TYPE_RECT | NODE_TYPE_HEXAGON => {
                padding *= 2;
            }
            _ => {}
        }

        let id = self.nodes.len() as i32;
        control.checkpoint()?;
        let node = MindmapNode {
            id,
            node_id: sanitize_text(input.id_raw, config),
            level,
            descr: if input.descr_is_markdown {
                input.descr_raw.to_string()
            } else {
                sanitize_text(input.descr_raw, config)
            },
            ty: input.ty,
            children: Vec::new(),
            width,
            padding,
            section: None,
            height: None,
            class: None,
            icon: None,
            x: None,
            y: None,
            is_root,
        };

        let mut popped = 0usize;
        while self
            .ancestry
            .last()
            .is_some_and(|(ancestor_level, _)| *ancestor_level >= level)
        {
            if popped.is_multiple_of(128) {
                control.checkpoint()?;
            }
            self.ancestry.pop();
            popped = popped.saturating_add(1);
        }

        if let Some((_, parent_idx)) = self.ancestry.last().copied() {
            self.nodes[parent_idx].children.push(id);
            self.nodes.push(node);
            self.ancestry.push((level, id as usize));
            control.checkpoint()?;
            return Ok(Ok(()));
        }

        if is_root {
            self.nodes.push(node);
            self.ancestry.push((level, id as usize));
            control.checkpoint()?;
            return Ok(Ok(()));
        }

        Ok(Err(Error::diagram_parse_fallback(
            input.diagram_type.to_string(),
            format!(
                "There can be only one root. No parent could be found for (\"{}\")",
                node.descr
            ),
        )))
    }

    pub(super) fn decorate_last(
        &mut self,
        class: Option<String>,
        icon: Option<String>,
        config: &MermaidConfig,
    ) {
        let Some(last) = self.nodes.last_mut() else {
            return;
        };
        if let Some(icon) = icon {
            last.icon = Some(sanitize_text(&icon, config));
        }
        if let Some(class) = class {
            last.class = Some(sanitize_text(&class, config));
        }
    }

    #[cfg(test)]
    pub(super) fn assign_sections(&mut self, node_id: i32, section: Option<i32>) {
        let control = OperationControl::new();
        self.assign_sections_controlled(node_id, section, &control)
            .expect("a private parse control cannot be cancelled");
    }

    pub(super) fn assign_sections_controlled(
        &mut self,
        node_id: i32,
        section: Option<i32>,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let mut stack = vec![(node_id, section)];
        let mut visited = 0usize;
        while let Some((node_id, section)) = stack.pop() {
            if visited.is_multiple_of(128) {
                control.checkpoint()?;
            }
            visited = visited.saturating_add(1);
            let Ok(node_idx) = usize::try_from(node_id) else {
                continue;
            };
            let Some(node) = self.nodes.get_mut(node_idx) else {
                continue;
            };
            let node_level = node.level;
            if node_level == 0 {
                node.section = None;
            } else {
                node.section = section;
            }

            let child_count = node.children.len();
            for index in (0..child_count).rev() {
                if index % 128 == 0 {
                    control.checkpoint()?;
                }
                let child_id = self.nodes[node_idx].children[index];
                let child_section = if node_level == 0 {
                    Some((index % MINDMAP_SECTION_COUNT) as i32)
                } else {
                    section
                };
                stack.push((child_id, child_section));
            }
        }
        control.checkpoint()
    }

    pub(super) fn to_layout_nodes_for_render_controlled(
        &self,
        root_id: i32,
        config: &MermaidConfig,
        control: &OperationControl,
    ) -> OperationControlResult<Vec<MindmapDiagramRenderNode>> {
        let mut out = Vec::new();
        let look = mindmap_look(config);
        let default_shape = mindmap_default_shape(config);
        let mut stack = vec![root_id];
        let mut visited = 0usize;
        while let Some(node_id) = stack.pop() {
            if visited.is_multiple_of(128) {
                control.checkpoint()?;
            }
            visited = visited.saturating_add(1);
            let Ok(node_idx) = usize::try_from(node_id) else {
                continue;
            };
            let Some(node) = self.nodes.get(node_idx) else {
                continue;
            };

            out.push(mindmap_render_node(node, &look, default_shape));

            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
        control.checkpoint()?;
        Ok(out)
    }

    pub(super) fn to_edges_for_render_controlled(
        &self,
        root_id: i32,
        config: &MermaidConfig,
        control: &OperationControl,
    ) -> OperationControlResult<Vec<MindmapDiagramRenderEdge>> {
        struct EdgeFrame {
            node_id: i32,
            next_child_index: usize,
        }

        let mut edges = Vec::new();
        let look = mindmap_look(config);
        let mut stack = vec![EdgeFrame {
            node_id: root_id,
            next_child_index: 0,
        }];
        let mut visited = 0usize;
        while let Some(frame) = stack.last_mut() {
            if visited.is_multiple_of(128) {
                control.checkpoint()?;
            }
            visited = visited.saturating_add(1);
            let Ok(node_idx) = usize::try_from(frame.node_id) else {
                stack.pop();
                continue;
            };
            let Some(node) = self.nodes.get(node_idx) else {
                stack.pop();
                continue;
            };
            let Some(child_id) = node.children.get(frame.next_child_index).copied() else {
                stack.pop();
                continue;
            };
            frame.next_child_index += 1;

            let Ok(child_idx) = usize::try_from(child_id) else {
                continue;
            };
            let Some(child) = self.nodes.get(child_idx) else {
                continue;
            };

            edges.push(mindmap_render_edge(node, child, &look));
            stack.push(EdgeFrame {
                node_id: child_id,
                next_child_index: 0,
            });
        }
        control.checkpoint()?;
        Ok(edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_child_sections_wrap_after_eleven_slots() {
        let config = MermaidConfig::empty_object();
        let parse_config = MindmapParseConfig::from_config(&config);
        let mut db = MindmapDb::default();
        db.add_node(
            MindmapNodeInput {
                indent_level: 0,
                id_raw: "root",
                descr_raw: "root",
                descr_is_markdown: false,
                ty: NODE_TYPE_DEFAULT,
                diagram_type: "mindmap",
            },
            &config,
            parse_config,
        )
        .expect("root node");

        for index in 0..15 {
            let id = format!("child-{index}");
            db.add_node(
                MindmapNodeInput {
                    indent_level: 1,
                    id_raw: &id,
                    descr_raw: &id,
                    descr_is_markdown: false,
                    ty: NODE_TYPE_DEFAULT,
                    diagram_type: "mindmap",
                },
                &config,
                parse_config,
            )
            .expect("root child");
        }

        db.assign_sections(0, None);
        let sections: Vec<_> = db.nodes.iter().skip(1).map(|node| node.section).collect();

        assert_eq!(
            sections,
            vec![
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                Some(9),
                Some(10),
                Some(0),
                Some(1),
                Some(2),
                Some(3),
            ]
        );
    }
}
