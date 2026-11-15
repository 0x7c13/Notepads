// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Complete nested recipes for the upstream flowchart value types. These
//! recipes never admit a shallow clone of already funded nested storage.

use super::*;
use markdown_memory::{CloneGroup, ClonePlan, MemoryError, PlanClone};

macro_rules! value_recipe {
    ($kind:ty { $($field:ident),* $(,)? }) => {
        impl PlanClone for $kind {
            fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError> {
                $(self.$field.plan_clone(plan)?;)*
                Ok(())
            }

            fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError> {
                Ok(Self { $($field: self.$field.clone_prepared(group)?),* })
            }
        }
    };
}

macro_rules! scalar_recipe {
    ($($kind:ty),* $(,)?) => {
        $(impl PlanClone for $kind {
            fn plan_clone(&self, _: &mut ClonePlan) -> Result<(), MemoryError> { Ok(()) }
            fn clone_prepared(&self, _: &mut CloneGroup) -> Result<Self, MemoryError> {
                // These concrete enums and spans have no allocation-bearing fields.
                Ok(self.clone())
            }
        })*
    };
}

scalar_recipe!(
    SourceSpan,
    FlowNodeProvenance,
    FlowEdgeMarker,
    FlowEdgeStroke,
    FlowEdgeVisibility,
    TitleKind
);

value_recipe!(DiagramWarningFact {
    rule_id,
    message,
    span,
    fix_span
});
value_recipe!(FlowEdgeDefaults { interpolate, style });
value_recipe!(FlowNode {
    id,
    provenance,
    label,
    label_type,
    layout_shape,
    shape,
    icon,
    form,
    pos,
    img,
    constraint,
    asset_width,
    asset_height,
    classes,
    styles,
    link,
    link_target,
    have_callback,
});
value_recipe!(FlowEdge {
    id,
    from,
    to,
    label,
    label_type,
    edge_type,
    arrow,
    start_marker,
    end_marker,
    is_user_defined_id,
    stroke,
    stroke_kind,
    visibility,
    interpolate,
    classes,
    style,
    animate,
    animation,
    length,
});
value_recipe!(FlowSubgraphVertexStyle { classes, styles });
value_recipe!(FlowSubgraphVertexStyleSource {
    declaration_ordinal,
    style
});
value_recipe!(FlowSubgraphRenderLabelSource { id, source });
value_recipe!(FlowchartRenderLabelSources {
    nodes,
    edges,
    subgraphs
});
value_recipe!(FlowchartRenderStyleSources { subgraph_vertices });
value_recipe!(FlowchartRenderContext {
    labels,
    styles,
    collapsed_subgraphs,
    collapsed_replacements,
    subgraph_color_ordinals,
    node_dom_indices,
});
value_recipe!(FlowchartModel {
    keyword,
    acc_descr,
    acc_title,
    class_defs,
    direction,
    edge_defaults,
    vertex_calls,
    nodes,
    edges,
    subgraphs,
    tooltips,
    warning_facts,
});

impl PlanClone for FlowSubgraph {
    fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError> {
        // Advanced metadata still uses the legacy serde-owned object. The
        // native profile must reject it before construction, never shallow
        // clone or pretend a length grant owns its private tree backing.
        if self.metadata.is_some() {
            return Err(MemoryError::Unfunded);
        }
        self.id.plan_clone(plan)?;
        self.title.plan_clone(plan)?;
        self.dir.plan_clone(plan)?;
        self.label_type.plan_clone(plan)?;
        self.classes.plan_clone(plan)?;
        self.styles.plan_clone(plan)?;
        self.nodes.plan_clone(plan)
    }

    fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError> {
        if self.metadata.is_some() {
            return Err(MemoryError::Unfunded);
        }
        Ok(Self {
            id: self.id.clone_prepared(group)?,
            title: self.title.clone_prepared(group)?,
            dir: self.dir.clone_prepared(group)?,
            has_explicit_dir: self.has_explicit_dir,
            label_type: self.label_type.clone_prepared(group)?,
            classes: self.classes.clone_prepared(group)?,
            styles: self.styles.clone_prepared(group)?,
            nodes: self.nodes.clone_prepared(group)?,
            metadata: None,
        })
    }
}
value_recipe!(LabeledText {
    text,
    kind,
    span,
    selection
});
value_recipe!(SubgraphHeader {
    raw_id,
    header_span,
    raw_id_span,
    raw_title,
    title_kind,
    id_equals_title,
});
value_recipe!(LinkToken {
    end,
    start_marker,
    end_marker,
    stroke_kind,
    visibility,
    length
});

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod parser_values {
    use super::*;

    scalar_recipe!(FlowNodeSyntax);
    value_recipe!(Node {
        id,
        provenance,
        syntax,
        id_span,
        label,
        label_type,
        label_span,
        label_selection,
        shape,
        shape_data,
        icon,
        form,
        pos,
        img,
        constraint,
        asset_width,
        asset_height,
        styles,
        classes,
        link,
        link_target,
        have_callback,
    });
    value_recipe!(Edge {
        from,
        to,
        id,
        link,
        label,
        label_type,
        label_span,
        label_selection,
        style,
        classes,
        interpolate,
        is_user_defined_id,
        animate,
        animation,
    });
    value_recipe!(EdgeDefaults { style, interpolate });
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_memory::{AllocationAuthority, Family, MemoryScope, OwnedPayload};
    use std::{
        alloc::Layout,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    struct Authority {
        used: AtomicUsize,
        reserves: AtomicUsize,
        allocates: AtomicUsize,
        fail_reserve: usize,
        fail_allocate: usize,
    }

    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Model {
                let site = self.reserves.fetch_add(1, Ordering::SeqCst) + 1;
                if site == self.fail_reserve {
                    return Err(MemoryError::BudgetLimited);
                }
            }
            self.used.fetch_add(layout.size(), Ordering::SeqCst);
            Ok(())
        }
        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }
        fn failure(&self) -> Option<MemoryError> {
            None
        }
        fn before_allocate(&self, _: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Model {
                let site = self.allocates.fetch_add(1, Ordering::SeqCst) + 1;
                if site == self.fail_allocate {
                    return Err(MemoryError::OutOfMemory);
                }
            }
            Ok(())
        }
    }

    fn scope(fail_reserve: usize, fail_allocate: usize) -> (MemoryScope, Arc<Authority>) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            reserves: AtomicUsize::new(0),
            allocates: AtomicUsize::new(0),
            fail_reserve,
            fail_allocate,
        });
        (MemoryScope::new(authority.clone()), authority)
    }

    fn edge() -> FlowEdge {
        FlowEdge {
            id: "L_A_B_0".into(),
            from: "A".into(),
            to: "B".into(),
            label: Some("继续".into()),
            label_type: Some("text".into()),
            edge_type: Some("arrow_point".into()),
            arrow: "-->".into(),
            start_marker: FlowEdgeMarker::None,
            end_marker: FlowEdgeMarker::Point,
            is_user_defined_id: false,
            stroke: Some("normal".into()),
            stroke_kind: FlowEdgeStroke::Normal,
            visibility: FlowEdgeVisibility::Visible,
            interpolate: None,
            classes: vec!["accent".into(), "wide".into()],
            style: vec!["stroke:#123456".into()],
            animate: Some(false),
            animation: None,
            length: 1,
        }
    }

    #[test]
    fn deep_edge_clone_keeps_numeric_identity_style_order_and_owned_receipts() {
        let original = edge();
        let (scope, authority) = scope(0, 0);
        let cloned = OwnedPayload::try_clone(&scope, Family::Model, &original).unwrap();
        assert_eq!(cloned.id, "L_A_B_0");
        assert_eq!(cloned.label.as_deref(), Some("继续"));
        assert_eq!(cloned.classes, ["accent", "wide"]);
        assert_eq!(cloned.style, ["stroke:#123456"]);
        assert_ne!(cloned.id.as_ptr(), original.id.as_ptr());
        assert_ne!(cloned.classes[0].as_ptr(), original.classes[0].as_ptr());
        assert!(cloned.owner_blocks() > 10);
        assert!(authority.used.load(Ordering::SeqCst) > 0);
        drop(cloned);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert_eq!(original.id, "L_A_B_0");
    }

    #[test]
    fn every_edge_nested_reservation_failure_precedes_all_payload_allocators() {
        let original = edge();
        let (first_scope, authority) = scope(0, 0);
        drop(OwnedPayload::try_clone(&first_scope, Family::Model, &original).unwrap());
        let sites = authority.reserves.load(Ordering::SeqCst);
        for site in 1..=sites {
            let (scope, authority) = scope(site, 0);
            assert!(matches!(
                OwnedPayload::try_clone(&scope, Family::Model, &original),
                Err(MemoryError::BudgetLimited)
            ));
            assert_eq!(authority.allocates.load(Ordering::SeqCst), 0);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
            assert_eq!(original.classes, ["accent", "wide"]);
        }
    }

    #[test]
    fn every_partial_edge_allocator_failure_frees_values_before_returning_owners() {
        let original = edge();
        let (first_scope, authority) = scope(0, 0);
        drop(OwnedPayload::try_clone(&first_scope, Family::Model, &original).unwrap());
        let sites = authority.allocates.load(Ordering::SeqCst);
        for site in 1..=sites {
            let (scope, authority) = scope(0, site);
            assert!(matches!(
                OwnedPayload::try_clone(&scope, Family::Model, &original),
                Err(MemoryError::OutOfMemory)
            ));
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
            assert_eq!(original.label.as_deref(), Some("继续"));
        }
    }
}
