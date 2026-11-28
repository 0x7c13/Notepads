//! Resolve registered graph layouts before execution.
//!
//! Mermaid 12's `getRegisteredLayoutAlgorithm` falls back when a loader is absent,
//! never after a selected backend fails. Host capability policy remains a separate
//! admission check: a compiled backend denied by the caller is not an absent loader.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphLayoutBackend {
    Dagre,
    Elk,
    CoseBilkent,
}

/// The root loaders registered by Mermaid 12. Container metadata has its own allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElkRootAlgorithm {
    Layered,
    Stress,
    Force,
    MrTree,
    SporeOverlap,
    Box,
    Rectpacking,
}

impl ElkRootAlgorithm {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "elk" => Self::Layered,
            "elk.stress" => Self::Stress,
            "elk.force" => Self::Force,
            "elk.mrtree" => Self::MrTree,
            "elk.sporeOverlap" => Self::SporeOverlap,
            "elk.box" => Self::Box,
            "elk.rectpacking" => Self::Rectpacking,
            _ => return None,
        })
    }

    #[cfg(feature = "layout-elk")]
    pub(crate) fn algorithm(self) -> merman_layout_elk::Algorithm {
        use merman_layout_elk::Algorithm;
        match self {
            Self::Layered => Algorithm::Layered,
            Self::Stress => Algorithm::Stress,
            Self::Force => Algorithm::Force,
            Self::MrTree => Algorithm::MrTree,
            Self::SporeOverlap => Algorithm::SporeOverlap,
            Self::Box => Algorithm::Box,
            Self::Rectpacking => Algorithm::Rectpacking,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GraphLayoutSelection<'a> {
    pub requested: &'a str,
    pub backend: GraphLayoutBackend,
}

impl GraphLayoutSelection<'_> {
    pub(crate) fn required_capability(self) -> Option<crate::RenderCapability> {
        match self.backend {
            GraphLayoutBackend::Dagre => None,
            GraphLayoutBackend::Elk => Some(crate::RenderCapability::LayoutElk),
            GraphLayoutBackend::CoseBilkent => Some(crate::RenderCapability::LayoutCytoscape),
        }
    }

    /// The selected CoSE loader requires Mindmap's root node. Graph-family adapters
    /// must preserve that upstream failure instead of silently executing Dagre.
    pub(crate) fn validate_rootless_graph(self) -> crate::Result<()> {
        if self.backend == GraphLayoutBackend::CoseBilkent {
            return Err(crate::Error::InvalidModel {
                message: "Root node is required for the cose-bilkent layout".to_owned(),
            });
        }
        Ok(())
    }
}

/// Select a registered loader before validating the family's layout data.
pub(crate) fn resolve_graph_layout(config: &Value) -> GraphLayoutSelection<'_> {
    let requested = config.get("layout").and_then(Value::as_str).unwrap_or("");
    let backend =
        if cfg!(feature = "layout-elk") && ElkRootAlgorithm::from_name(requested).is_some() {
            GraphLayoutBackend::Elk
        } else if cfg!(feature = "layout-cytoscape") && requested == "cose-bilkent" {
            GraphLayoutBackend::CoseBilkent
        } else {
            GraphLayoutBackend::Dagre
        };
    GraphLayoutSelection { requested, backend }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registered_cose_preserves_missing_root_failure_while_unknown_layout_falls_back() {
        let config = json!({"layout": "cose-bilkent"});
        let selected = resolve_graph_layout(&config);
        if cfg!(feature = "layout-cytoscape") {
            assert_eq!(selected.backend, GraphLayoutBackend::CoseBilkent);
            assert_eq!(
                selected.required_capability(),
                Some(crate::RenderCapability::LayoutCytoscape)
            );
            assert!(
                matches!(selected.validate_rootless_graph(), Err(crate::Error::InvalidModel { message }) if message.contains("Root node is required"))
            );
        } else {
            assert_eq!(selected.backend, GraphLayoutBackend::Dagre);
            assert_eq!(selected.required_capability(), None);
            selected.validate_rootless_graph().unwrap();
        }
        let unknown = json!({"layout": "unknown-layout-probe"});
        let selected = resolve_graph_layout(&unknown);
        assert_eq!(selected.backend, GraphLayoutBackend::Dagre);
        assert_eq!(selected.required_capability(), None);
        selected.validate_rootless_graph().unwrap();
    }

    #[test]
    fn graph_layout_selection_retains_request_and_resolves_available_loader() {
        for (requested, registered_elk) in [
            ("dagre", false),
            ("elk", true),
            ("elk.stress", true),
            ("elk.force", true),
            ("elk.mrtree", true),
            ("elk.sporeOverlap", true),
            ("elk.box", true),
            ("elk.rectpacking", true),
            ("elk.layered", false),
            ("elk.radial", false),
            ("elk.sporeoverlap", false),
            ("ELK", false),
            ("unknown", false),
            ("", false),
        ] {
            let config = json!({"layout": requested});
            let selected = resolve_graph_layout(&config);
            assert_eq!(selected.requested, requested);
            let expected = if registered_elk && cfg!(feature = "layout-elk") {
                GraphLayoutBackend::Elk
            } else {
                GraphLayoutBackend::Dagre
            };
            assert_eq!(selected.backend, expected);
        }
        assert_eq!(
            resolve_graph_layout(&Value::Null).backend,
            GraphLayoutBackend::Dagre
        );
    }
}
