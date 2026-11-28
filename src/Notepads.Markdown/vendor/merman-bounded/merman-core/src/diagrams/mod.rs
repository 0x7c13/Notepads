//! Public data models for Mermaid diagram families.
//!
//! Each family module requires its corresponding `diagram-*` feature. The `flowchart` module
//! is shared by `diagram-flowchart`, `diagram-swimlane`, and `diagram-agentflow`;
//! `error_diagram` is always available.
//!
//! Built-in family constructors are intentionally crate-private. External callers must parse
//! through [`crate::Engine`], which owns preprocessing, detection, configuration, and the closed
//! [`crate::DiagramParseSnapshot`] contract. This prevents callers from constructing metadata by
//! hand and invoking a semantic, render-model, or editor-facts parser independently.
//!
//! ```compile_fail,E0603
//! use merman_core::diagrams::flowchart::parse_flowchart;
//! ```
//!
//! Built-in parser pointers are not exposed through the public registry either:
//!
//! ```compile_fail,E0624
//! use merman_core::DiagramRegistry;
//!
//! let registry = DiagramRegistry::pinned_mermaid_baseline();
//! let _parser = registry.get("flowchart-v2");
//! ```

#[cfg(any(
    feature = "diagram-class",
    feature = "diagram-er",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-sequence",
    feature = "diagram-state"
))]
macro_rules! include_checked_in_lalrpop_parser {
    ($(#[$attr:meta])* $name:ident, $file:literal) => {
        #[rustfmt::skip]
        #[allow(clippy::extra_unused_lifetimes)]
        #[allow(clippy::needless_lifetimes)]
        #[allow(clippy::let_unit_value)]
        #[allow(clippy::just_underscores_and_digits)]
        $(#[$attr])*
        mod $name {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/generated/lalrpop/",
                $file
            ));
        }
    };
}

#[cfg(feature = "diagram-agentflow")]
pub mod agentflow;
#[cfg(feature = "diagram-architecture")]
pub mod architecture;
#[cfg(feature = "diagram-block")]
pub mod block;
#[cfg(feature = "diagram-c4")]
pub mod c4;
#[cfg(feature = "diagram-class")]
pub mod class;
#[cfg(feature = "diagram-cynefin")]
pub mod cynefin;
#[cfg(feature = "diagram-er")]
pub mod er;
pub mod error_diagram;
#[cfg(feature = "diagram-event-modeling")]
pub mod eventmodeling;
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub mod flowchart;
#[cfg(feature = "diagram-gantt")]
pub mod gantt;
#[cfg(feature = "diagram-git-graph")]
pub mod git_graph;
#[cfg(feature = "diagram-info")]
pub mod info;
#[cfg(feature = "diagram-ishikawa")]
pub mod ishikawa;
#[cfg(any(
    feature = "diagram-class",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
mod jison_unicode;
#[cfg(feature = "diagram-journey")]
pub mod journey;
#[cfg(feature = "diagram-kanban")]
pub mod kanban;
#[cfg(any(
    test,
    feature = "diagram-architecture",
    feature = "diagram-cynefin",
    feature = "diagram-event-modeling",
    feature = "diagram-git-graph",
    feature = "diagram-info",
    feature = "diagram-packet",
    feature = "diagram-pie",
    feature = "diagram-radar",
    feature = "diagram-venn",
    feature = "diagram-treemap",
    feature = "diagram-tree-view",
    feature = "diagram-wardley",
    feature = "diagram-zenuml"
))]
#[allow(
    dead_code,
    reason = "Shared parser facilities have different consumers in each family selection."
)]
pub(crate) mod langium_common;
#[cfg(feature = "diagram-mindmap")]
pub mod mindmap;
#[cfg(feature = "diagram-packet")]
pub mod packet;
#[cfg(feature = "diagram-pie")]
pub mod pie;
#[cfg(feature = "diagram-quadrant-chart")]
pub mod quadrant_chart;
#[cfg(feature = "diagram-radar")]
pub mod radar;
#[cfg(feature = "diagram-railroad")]
pub mod railroad;
#[cfg(feature = "diagram-requirement")]
pub mod requirement;
#[cfg(feature = "diagram-sankey")]
pub mod sankey;
// Common metadata normalization also uses ECMAScript scanning without diagram features.
#[allow(
    dead_code,
    reason = "Shared parser facilities have different consumers in each family selection."
)]
pub(crate) mod scan;
#[cfg(feature = "diagram-sequence")]
pub mod sequence;
#[cfg(any(
    feature = "diagram-agentflow",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane"
))]
pub(crate) mod shapes;
#[cfg(feature = "diagram-state")]
pub mod state;
#[cfg(feature = "diagram-timeline")]
pub mod timeline;
#[cfg(feature = "diagram-tree-view")]
pub mod tree_view;
#[cfg(feature = "diagram-treemap")]
pub mod treemap;
#[cfg(feature = "diagram-usecase")]
pub mod usecase;
#[cfg(feature = "diagram-venn")]
pub mod venn;
#[cfg(feature = "diagram-wardley")]
pub mod wardley;
#[cfg(feature = "diagram-xychart")]
pub mod xychart;
#[cfg(feature = "diagram-zenuml")]
pub mod zenuml;
