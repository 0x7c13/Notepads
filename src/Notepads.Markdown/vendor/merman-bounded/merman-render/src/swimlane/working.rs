use crate::model::{LayoutPoint, SwimlaneDirection, SwimlaneTitleRect};
use indexmap::IndexMap;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkingNodeKind {
    Group,
    Content,
    EdgeLabel,
    Dummy,
}

#[derive(Debug, Clone)]
pub(super) struct WorkingNode {
    pub id: String,
    pub label: String,
    pub label_type: String,
    pub shape: String,
    pub kind: WorkingNodeKind,
    pub parent_id: Option<String>,
    pub top_lane_id: Option<String>,
    pub requested_dir: Option<String>,
    pub padding: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub label_width: f64,
    pub label_height: f64,
    pub layer: usize,
    pub order: usize,
    pub content_top: Option<f64>,
    pub title_rect: Option<SwimlaneTitleRect>,
}

impl WorkingNode {
    pub fn is_group(&self) -> bool {
        self.kind == WorkingNodeKind::Group
    }

    pub fn is_layout_node(&self) -> bool {
        matches!(
            self.kind,
            WorkingNodeKind::Content | WorkingNodeKind::EdgeLabel | WorkingNodeKind::Dummy
        )
    }
}

#[derive(Debug, Clone)]
pub(super) struct WorkingEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub reference_id: String,
    pub label_node_id: Option<String>,
    pub reversed_for_layout: bool,
    pub points: Vec<LayoutPoint>,
}

impl WorkingEdge {
    pub fn layout_key(&self) -> String {
        format!("{}:{}->{}", self.id, self.from, self.to)
    }
}

#[derive(Debug, Clone)]
pub(super) struct WorkingLayout {
    pub direction: SwimlaneDirection,
    pub nodes: IndexMap<String, WorkingNode>,
    pub graph_edges: Vec<WorkingEdge>,
    pub original_edges: Vec<WorkingEdge>,
    pub top_lane_order: Vec<String>,
}

impl WorkingLayout {
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub fn refresh_top_lane_ids(&mut self) {
        let parents: HashMap<String, Option<String>> = self
            .nodes
            .iter()
            .map(|(id, node)| (id.clone(), node.parent_id.clone()))
            .collect();
        let groups: HashSet<String> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.is_group())
            .map(|(id, _)| id.clone())
            .collect();

        for node in self.nodes.values_mut() {
            let mut current = node.parent_id.clone();
            let mut top = None;
            let mut visited = HashSet::new();
            while let Some(parent) = current {
                if !visited.insert(parent.clone()) || !groups.contains(&parent) {
                    break;
                }
                top = Some(parent.clone());
                current = parents.get(&parent).and_then(Clone::clone);
            }
            node.top_lane_id = top;
        }
    }

    pub fn top_lane_of(&self, id: &str) -> Option<&str> {
        self.nodes
            .get(id)
            .and_then(|node| node.top_lane_id.as_deref())
    }

    pub fn children_of<'a>(&'a self, parent_id: &'a str) -> impl Iterator<Item = &'a WorkingNode> {
        self.nodes
            .values()
            .filter(move |node| node.parent_id.as_deref() == Some(parent_id))
    }
}
