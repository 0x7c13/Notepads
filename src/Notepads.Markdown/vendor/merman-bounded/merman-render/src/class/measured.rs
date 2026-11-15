//! Class measurements shared by the Dagre and ELK projections.
//!
//! Keep source identity and insertion order here; backend labels and graph mutation belong to
//! their projections. Terminal metrics remain typed until the Dagre boundary serializes them.

use super::{ClassLayoutGraph, EdgeTerminalMetrics};
use indexmap::IndexMap;

#[derive(Default)]
pub(super) struct MeasuredNode {
    pub width: f64,
    pub height: f64,
    pub is_namespace: bool,
}

pub(super) struct MeasuredEdge {
    pub width: f64,
    pub height: f64,
    pub terminals: Option<EdgeTerminalMetrics>,
}

pub(super) struct MeasuredGraph {
    pub direction: String,
    pub nodesep: f64,
    pub ranksep: f64,
    pub nodes: IndexMap<String, MeasuredNode>,
    // Parent references may insert nodes before their declarations. Preserve hierarchy mutation
    // order separately so those placeholders do not reorder a namespace's declared children.
    pub parents: IndexMap<String, String>,
    // Source, target, and name together identify a Graphlib multiedge.
    pub edges: IndexMap<(String, String, String), MeasuredEdge>,
}

impl MeasuredGraph {
    pub fn set_node(&mut self, id: String, mut node: MeasuredNode) {
        if let Some(existing) = self.nodes.get(&id) {
            node.is_namespace |= existing.is_namespace;
        }
        self.nodes.insert(id, node);
    }

    pub fn set_parent(&mut self, child: String, parent: String) {
        // Graphlib inserts a referenced parent immediately, even before its declaration.
        self.nodes.entry(child.clone()).or_default();
        self.nodes.entry(parent.clone()).or_default();
        self.parents.insert(child, parent);
    }

    pub fn set_edge(&mut self, source: String, target: String, id: String, edge: MeasuredEdge) {
        self.nodes.entry(source.clone()).or_default();
        self.nodes.entry(target.clone()).or_default();
        self.edges.insert((source, target, id), edge);
    }

    pub fn to_dagre(&self) -> ClassLayoutGraph {
        use dugong::graphlib::{Graph, GraphOptions};
        use dugong::{EdgeLabel, GraphLabel, LabelPos, NodeLabel};

        let mut graph = Graph::new(GraphOptions {
            directed: true,
            multigraph: true,
            compound: true,
        });
        graph.set_graph(GraphLabel {
            rankdir: super::rank_dir_from(&self.direction),
            nodesep: self.nodesep,
            ranksep: self.ranksep,
            // The SVG viewport adds Mermaid's graph margins after layout.
            marginx: 0.0,
            marginy: 0.0,
            ..Default::default()
        });
        for (id, node) in &self.nodes {
            graph.set_node(
                id.clone(),
                NodeLabel {
                    width: node.width,
                    height: node.height,
                    ..Default::default()
                },
            );
        }
        for (child, parent) in &self.parents {
            graph.set_parent(child.clone(), parent.clone());
        }
        for ((source, target, id), edge) in &self.edges {
            let mut label = EdgeLabel {
                width: edge.width,
                height: edge.height,
                labelpos: LabelPos::C,
                labeloffset: 10.0,
                minlen: 1,
                weight: 1.0,
                ..Default::default()
            };
            if let Some(meta) = &edge.terminals {
                for (key, metrics) in [
                    ("startLeft", meta.start_left),
                    ("startRight", meta.start_right),
                    ("endLeft", meta.end_left),
                    ("endRight", meta.end_right),
                ] {
                    if let Some((width, height)) = metrics {
                        super::set_extras_label_metrics(&mut label.extras, key, width, height);
                    }
                }
                label
                    .extras
                    .insert("startMarker".to_string(), meta.start_marker.into());
                label
                    .extras
                    .insert("endMarker".to_string(), meta.end_marker.into());
            }
            graph.set_edge_named(
                source.clone(),
                target.clone(),
                Some(id.clone()),
                Some(label),
            );
        }
        graph
    }
}

#[cfg(test)]
mod tests {
    use super::{MeasuredGraph, MeasuredNode};
    use indexmap::IndexMap;

    #[test]
    fn forward_namespace_parents_do_not_reorder_declared_children() {
        let mut measured = MeasuredGraph {
            direction: "TB".into(),
            nodesep: 50.0,
            ranksep: 50.0,
            nodes: IndexMap::new(),
            parents: IndexMap::new(),
            edges: IndexMap::new(),
        };
        for (child, parent) in [("Child", "Parent"), ("Sibling", "Root"), ("Parent", "Root")] {
            measured.set_node(
                child.into(),
                MeasuredNode {
                    is_namespace: true,
                    ..Default::default()
                },
            );
            measured.set_parent(child.into(), parent.into());
        }
        measured.set_node(
            "Root".into(),
            MeasuredNode {
                is_namespace: true,
                ..Default::default()
            },
        );

        let graph = measured.to_dagre();
        assert_eq!(graph.node_ids(), ["Child", "Parent", "Sibling", "Root"]);
        assert_eq!(graph.children("Root"), ["Sibling", "Parent"]);
        assert_eq!(graph.parent("Child"), Some("Parent"));
        assert_eq!(graph.parent("Parent"), Some("Root"));
    }
}
