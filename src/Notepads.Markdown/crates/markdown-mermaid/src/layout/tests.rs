// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! A deterministic synthetic font tests geometry, not DirectWrite or shaping.
use super::*;
use crate::canonical::{OutlineProvider, RunMetrics, TextRun};
use markdown_memory::{AllocationAuthority, MemoryError, NativeRegion};
use std::{
    alloc::Layout as AllocationLayout,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Authority {
    used: AtomicUsize,
    allocations: AtomicUsize,
    fail: AtomicUsize,
    work: AtomicUsize,
    first: AtomicUsize,
}
impl AllocationAuthority for Authority {
    fn reserve(&self, l: AllocationLayout, _: Family) -> Result<(), MemoryError> {
        self.used
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(l.size()).filter(|n| *n <= 64 * 1024 * 1024)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::BudgetLimited)
    }
    fn release(&self, l: AllocationLayout, _: Family) {
        self.used.fetch_sub(l.size(), Ordering::SeqCst);
    }
    fn failure(&self) -> Option<MemoryError> {
        match self.first.load(Ordering::SeqCst) {
            0 => None,
            1 => Some(MemoryError::BudgetLimited),
            2 => Some(MemoryError::OutOfMemory),
            3 => Some(MemoryError::ArithmeticOverflow),
            4 => Some(MemoryError::InvalidState),
            5 => Some(MemoryError::Cancelled),
            6 => Some(MemoryError::Unfunded),
            _ => Some(MemoryError::WorkLimited),
        }
    }
    fn record_failure(&self, error: MemoryError) {
        let code = match error {
            MemoryError::BudgetLimited => 1,
            MemoryError::OutOfMemory => 2,
            MemoryError::ArithmeticOverflow => 3,
            MemoryError::InvalidState => 4,
            MemoryError::Cancelled => 5,
            MemoryError::Unfunded => 6,
            MemoryError::WorkLimited => 7,
        };
        let _ = self
            .first
            .compare_exchange(0, code, Ordering::SeqCst, Ordering::SeqCst);
    }
    fn before_allocate(&self, _: AllocationLayout, _: Family) -> Result<(), MemoryError> {
        let n = self.allocations.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.fail.load(Ordering::SeqCst) {
            Err(MemoryError::OutOfMemory)
        } else {
            Ok(())
        }
    }
    fn charge_work(&self, n: usize) -> Result<(), MemoryError> {
        self.work
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                old.checked_add(n).filter(|n| *n <= 1_000_000)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::WorkLimited)
    }
}
struct Font;
impl OutlineProvider for Font {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[run], TextDirection::Auto)
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        self.outline_spans(&[run], TextDirection::Auto, sink)
    }
}
impl RichTextProvider for Font {
    fn measure_spans(
        &self,
        runs: &[TextRun<'_>],
        _: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        let width = runs
            .iter()
            .map(|r| r.text.chars().count() as f64 * r.font.em_size / 2.0)
            .sum::<f64>();
        Ok(RunMetrics {
            advance: width,
            ascent: 12.0,
            descent: 4.0,
            x_height: 8.0,
            ink: rect(0.0, -12.0, width, 16.0),
        })
    }
    fn outline_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        let m = self.measure_spans(runs, direction)?;
        if m.advance > 0.0 {
            scene::shapes::outline(Shape::Rectangle, m.ink, sink)?;
        }
        Ok(())
    }
}
fn setup() -> (Arc<Authority>, MemoryScope, OperationControl) {
    let a = Arc::new(Authority {
        used: AtomicUsize::new(0),
        allocations: AtomicUsize::new(0),
        fail: AtomicUsize::new(0),
        work: AtomicUsize::new(0),
        first: AtomicUsize::new(0),
    });
    let s = MemoryScope::new(a.clone());
    let region = NativeRegion::try_new(&s, 8192).unwrap();
    let control = OperationControl::new().with_memory_region(region);
    (a, s, control)
}
fn parse(
    source: &str,
    control: &OperationControl,
) -> NativePayload<(FlowchartModel, FlowchartRenderContext)> {
    merman_core::parse_flowchart_native_controlled(
        source,
        &merman_core::NativeParsePolicy::default(),
        control,
    )
    .unwrap()
}
fn prepared<'a>(
    input: &'a NativePayload<(FlowchartModel, FlowchartRenderContext)>,
    scope: &'a MemoryScope,
    control: &'a OperationControl,
) -> Result<Layout<'a>, CanonicalError> {
    let (model, context) = &**input;
    let mut layout = Layout::new(
        model,
        context,
        &[],
        scope,
        control,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font,
    )?;
    layout.topology()?;
    layout.plan_segments()?;
    layout.place()?;
    layout.route()?;
    Ok(layout)
}

fn accept_scene(output: OwnedScene, scope: &MemoryScope, control: &OperationControl) {
    let facts = crate::canonical::ArtifactFacts {
        bounds: output.bounds,
        paths: output.paths,
        commands: output.commands,
    };
    let document = crate::canonical::accept_artifact(
        output.svg,
        facts,
        scope,
        control,
        crate::canonical::CanonicalLimits::default(),
    )
    .unwrap();
    drop(document);
}

#[test]
fn relationship_native_paints_compartments_members_and_dashed_cardinality_routes() {
    use crate::native_relationships::{RelationshipDiagram, render_relationship_native_controlled};

    for (family, source, without_members) in [
        (
            RelationshipDiagram::Class,
            "classDiagram\nclass Preview {\n    +show()\n}\nPreview ..|> Renderer : realizes",
            "classDiagram\nclass Preview\nPreview ..|> Renderer : realizes",
        ),
        (
            RelationshipDiagram::EntityRelationship,
            "erDiagram\nPARENT {\n    string id PK\n}\nPARENT 1 optionally to 0+ \"子项 Child\" : has",
            "erDiagram\nPARENT\nPARENT||..o{\"子项 Child\" : has",
        ),
    ] {
        let (authority, scope, control) = setup();
        let render = |source| {
            render_relationship_native_controlled(
                source,
                family,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &Font,
                &control,
            )
            .unwrap_or_else(|error| panic!("{source}: {error:?}"))
        };
        let minimum = render(without_members);
        let detailed = render(source);
        assert!(detailed.bounds.bottom > minimum.bounds.bottom, "{source}");
        assert_eq!(detailed.paths, minimum.paths + 1, "one compartment rule");
        assert_eq!(
            detailed.commands,
            minimum.commands + 7,
            "rule and member outline"
        );
        let svg = detailed.svg.as_str();
        assert!(svg.contains("stroke-dasharray="), "{source}");
        assert!(!svg.contains("<text") && !svg.contains("<foreignObject"));
        accept_scene(minimum, &scope, &control);
        accept_scene(detailed, &scope, &control);
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn relationship_labels_preserve_adjacent_names_entities_and_hard_breaks() {
    use crate::native_relationships::{RelationshipDiagram, render_relationship_native_controlled};

    let (_, _, control) = setup();
    let render = |source| {
        render_relationship_native_controlled(
            source,
            RelationshipDiagram::Class,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &control,
        )
        .unwrap_or_else(|error| panic!("{source}: {error:?}"))
    };
    let spaced = render("classDiagram\nA -- object\nA .. only");
    let compact = render("classDiagram\nA--object\nA..only");
    assert_eq!(spaced.svg.as_str(), compact.svg.as_str());
    let html = render("classDiagram\nclass A[\"A<br /> &lt;B&gt;]\"] {\n    +List<T> items\n}");
    let literal = render("classDiagram\nclass A[\"A\\n <B>]\"] {\n    +List<T> items\n}");
    assert_eq!(html.svg.as_str(), literal.svg.as_str());
}

#[test]
fn parallel_uml_multiplicities_reserve_distinct_routed_ports_on_both_ends() {
    let (authority, scope, control) = setup();
    let input = parse("flowchart TB\nA---|first| B\nA---|second| B", &control);
    let (model, context) = &*input;
    let mut layout = Layout::new(
        model,
        context,
        &[],
        &scope,
        &control,
        LayoutPolicy {
            clearance: 20.0,
            rank_gap: 64.0,
            ..LayoutPolicy::default()
        },
        ThemePolicy::default(),
        &Font,
    )
    .unwrap();
    let label = |value| {
        let mut text = FundedString::new_in(scope.clone(), Family::Text);
        text.try_push_str(value).unwrap();
        Some(text)
    };
    let styles = [
        RelationshipEdge {
            ends: RelationshipEnds::default(),
            multiplicities: [label("1"), label("*")],
        },
        RelationshipEdge {
            ends: RelationshipEnds::default(),
            multiplicities: [label("0..1"), label("1..*")],
        },
    ];
    layout.prepare_relationship_ports(&styles, &Font).unwrap();
    layout.topology().unwrap();
    layout.plan_segments().unwrap();
    layout.place().unwrap();
    layout.route().unwrap();
    let [first, second] = &*layout.edges else {
        panic!("expected two parallel edges");
    };
    for (index, (first_port, second_port)) in [
        (first.route[0], second.route[0]),
        (
            first.route[first.route.len() - 1],
            second.route[second.route.len() - 1],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let width = layout.relationship_ports[0].multiplicities[index]
            .as_ref()
            .unwrap()
            .width
            .max(
                layout.relationship_ports[1].multiplicities[index]
                    .as_ref()
                    .unwrap()
                    .width,
            );
        assert!(second_port.x - first_port.x >= width + 8.0);
        assert_eq!(first_port.y, second_port.y);
    }
    assert!((first.route[1].y - first.route[0].y).abs() >= 20.0);
    accept_scene(
        layout
            .paint_with_relationship_style(
                &Font,
                RelationshipStyle {
                    header_ends: &[],
                    edges: &styles,
                },
            )
            .unwrap(),
        &scope,
        &control,
    );
    drop(styles);
    drop(input);
    drop(control);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn actual_source_to_scene_preserves_classic_geometry_and_real_arrow_paths() {
    for source in [
        "flowchart TB\nA[Start] --> B{Choice}\nB --> C(End)",
        "graph LR\nA --> B\nC\nB --> D",
        "flowchart BT\nA --> B\nB --> A",
        "flowchart RL\nA --> A\nA --> B\nA --> B",
    ] {
        let (a, s, c) = setup();
        let input = parse(source, &c);
        let layout = prepared(&input, &s, &c).unwrap_or_else(|e| panic!("{source}: {e:?}"));
        for i in 0..layout.root {
            for j in i + 1..layout.root {
                assert!(!scene::intersects(
                    layout.cells[i].bounds,
                    layout.cells[j].bounds
                ));
            }
        }
        assert_eq!(layout.edges.len(), input.0.edges.len());
        let output = layout.paint(&Font).unwrap();
        assert!(output.svg.as_str().starts_with("<svg "));
        assert!(!output.svg.contains("<text"));
        assert!(output.paths >= input.0.nodes.len() + input.0.edges.len());
        assert!(output.commands > 0);
        let markers = input
            .0
            .edges
            .iter()
            .map(|edge| {
                usize::from(edge.start_marker != FlowEdgeMarker::None)
                    + usize::from(edge.end_marker != FlowEdgeMarker::None)
            })
            .sum::<usize>();
        let mut actual_markers = 0;
        for path in output.svg.split("<path").skip(1) {
            let path = path.split("/>").next().unwrap();
            if path.contains("fill=\"#545c70\"") {
                actual_markers += 1;
                let data = path.split(" d=\"").nth(1).unwrap();
                assert!(
                    data.starts_with('M') && data.contains('L') && data.contains('Z'),
                    "point arrow has no actual filled triangle ink"
                );
            }
        }
        assert_eq!(actual_markers, markers);
        accept_scene(output, &s, &c);
        drop(input);
        drop(c);
        drop(s);
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn ordinary_compound_cases_have_continuous_portals_and_owned_labels() {
    for source in [
        "flowchart TB\nsubgraph G[Group]\nA\nend\nA --> B",
        "flowchart TB\nsubgraph G[Group]\nA\nend\nB --> A",
        "flowchart LR\nsubgraph O[Outer]\nsubgraph I[Inner]\ndirection TB\nA\nend\nend\nsubgraph R[Right]\ndirection RL\nB\nend\nA --> B",
        "flowchart TB\nsubgraph G[Group]\nA\nend\nG --> B",
        "flowchart TB\nsubgraph G[Group]\nA\nend\nG --> A",
        "flowchart TB\nA -->|a long edge label| A\nA -->|first branch| B\nA -->|parallel branch| B",
        "flowchart TB\nsubgraph G[Group]\nA\nend\nG -->|group loop| G",
    ] {
        let (a, s, c) = setup();
        let input = parse(source, &c);
        let layout = prepared(&input, &s, &c).unwrap_or_else(|e| panic!("{source}: {e:?}"));
        for p in &layout.portals {
            assert!(layout.edges.iter().any(|e| e.route.contains(&p.point)));
        }
        for edge in &layout.edges {
            assert!(edge.route.len() >= 2);
            if edge.from == edge.to {
                assert_ne!(
                    edge.route.first(),
                    edge.route.last(),
                    "self-loop must use distinct contour ports"
                );
            }
            if edge.label.is_some() {
                let owner = layout.cells[edge.owner].bounds;
                assert!(
                    edge.label_rect.left >= owner.left
                        && edge.label_rect.right <= owner.right
                        && edge.label_rect.top >= owner.top
                        && edge.label_rect.bottom <= owner.bottom
                );
            }
        }
        let output = layout.paint(&Font).unwrap();
        assert!(output.bounds.right > output.bounds.left);
        accept_scene(output, &s, &c);
        drop(input);
        drop(c);
        drop(s);
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn nth_real_layout_allocation_rejects_without_losing_original_model_or_receipts() {
    let (a, s, c) = setup();
    let input = parse("flowchart TB\nA[Start] --> B[End]", &c);
    let before = a.allocations.load(Ordering::SeqCst);
    let result = render_flowchart_native_controlled(
        &input,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font,
        &c,
    )
    .unwrap();
    let attempts = a.allocations.load(Ordering::SeqCst) - before;
    drop(result);
    drop(input);
    drop(c);
    drop(s);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
    for nth in 1..=attempts {
        let (a, s, c) = setup();
        let input = parse("flowchart TB\nA[Start] --> B[End]", &c);
        let base = a.used.load(Ordering::SeqCst);
        let start = a.allocations.load(Ordering::SeqCst);
        a.fail.store(start + nth, Ordering::SeqCst);
        let result = render_flowchart_native_controlled(
            &input,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &c,
        );
        assert!(
            result.is_err(),
            "allocation {nth} of {attempts} unexpectedly succeeded"
        );
        assert_eq!(input.0.nodes.len(), 2);
        assert_eq!(input.0.edges.len(), 1);
        assert_eq!(a.used.load(Ordering::SeqCst), base);
        drop(input);
        drop(c);
        drop(s);
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn ordinary_twenty_five_edge_chain_fits_the_production_work_limit() {
    use std::fmt::Write;

    let mut source = String::from("flowchart TD\n");
    for i in 0..25 {
        writeln!(source, "N{i:03} --> N{:03}", i + 1).unwrap();
    }
    let (_, scope, control) = setup();
    let input = parse(&source, &control);
    let result = render_flowchart_native_controlled(
        &input,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font,
        &control,
    )
    .unwrap();
    accept_scene(result, &scope, &control);
}

#[test]
fn router_detours_actual_obstacles_and_keeps_direction_in_state() {
    let (a, s, c) = setup();
    let mut states = 0;
    let mut graph = router::ChannelGraph::new(
        &s,
        rect(0.0, 0.0, 200.0, 150.0),
        &[rect(75.0, 30.0, 50.0, 90.0)],
        &[Point { x: 20.0, y: 70.0 }, Point { x: 180.0, y: 70.0 }],
        &[],
        &mut states,
    )
    .unwrap();
    assert_eq!(states, 6 * 5 * 4);
    let route = graph
        .route(
            Point { x: 20.0, y: 70.0 },
            Point { x: 180.0, y: 70.0 },
            20.0,
            0,
        )
        .unwrap();
    assert!(route.len() >= 4);
    for pair in route.windows(2) {
        assert!(
            !((pair[0].y - 70.0).abs() < 1e-8
                && (pair[1].y - 70.0).abs() < 1e-8
                && pair[0].x.min(pair[1].x) < 125.0
                && pair[0].x.max(pair[1].x) > 75.0)
        );
    }
    drop(route);
    drop(graph);
    drop(c);
    drop(s);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}

#[test]
fn router_resolves_coordinates_at_the_deduplication_tolerance() {
    let (a, s, c) = setup();
    let mut states = 0;
    let from = Point { x: 1e-6, y: 1e-6 };
    let to = Point { x: 9.0, y: 9.0 };
    let mut graph = router::ChannelGraph::new(
        &s,
        rect(0.0, 0.0, 10.0, 10.0),
        &[],
        &[from, to],
        &[],
        &mut states,
    )
    .unwrap();
    let route = graph.route(from, to, 1.0, 0).unwrap();
    assert!(route.len() >= 2);
    assert_eq!(route[0], Point { x: 0.0, y: 0.0 });
    assert_eq!(route.last().copied(), Some(to));
    drop(route);
    drop(graph);
    drop(c);
    drop(s);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}

#[test]
fn all_fourteen_classic_parser_shapes_keep_distinct_typed_contours() {
    let definitions = [
        ("A[box]", Shape::Rectangle),
        ("A(round)", Shape::Rounded),
        ("A([stadium])", Shape::Stadium),
        ("A[[subroutine]]", Shape::Subroutine),
        ("A[(database)]", Shape::Cylinder),
        ("A((circle))", Shape::Circle),
        ("A(((double)))", Shape::DoubleCircle),
        ("A{decision}", Shape::Diamond),
        ("A{{hexagon}}", Shape::Hexagon),
        ("A[/parallel/]", Shape::Parallelogram),
        ("A[\\parallel\\]", Shape::ParallelogramAlt),
        ("A[/trapezoid\\]", Shape::Trapezoid),
        ("A[\\trapezoid/]", Shape::TrapezoidAlt),
        ("A>asymmetric]", Shape::Asymmetric),
    ];
    for (definition, shape) in definitions {
        let (a, s, c) = setup();
        let input = parse(&format!("flowchart TB\n{definition}"), &c);
        let layout = prepared(&input, &s, &c).unwrap_or_else(|e| panic!("{definition}: {e:?}"));
        assert_eq!(layout.root, 1);
        assert_eq!(layout.cells[0].shape, shape, "{definition}");
        let r = layout.cells[0].bounds;
        let middle = center(r);
        for toward in [
            Point {
                x: middle.x + 1000.0,
                y: middle.y,
            },
            Point {
                x: middle.x - 1000.0,
                y: middle.y,
            },
            Point {
                x: middle.x,
                y: middle.y + 1000.0,
            },
            Point {
                x: middle.x,
                y: middle.y - 1000.0,
            },
        ] {
            let p = scene::shapes::port(shape, r, toward).unwrap();
            assert!(
                p.x >= r.left - 1e-6
                    && p.x <= r.right + 1e-6
                    && p.y >= r.top - 1e-6
                    && p.y <= r.bottom + 1e-6
            );
        }
        let output = layout.paint(&Font).unwrap();
        assert!(output.commands >= 5);
        accept_scene(output, &s, &c);
        drop(input);
        drop(c);
        drop(s);
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn explicit_curve_and_effective_unknown_style_do_not_become_fake_success() {
    for source in [
        "flowchart TB\nA --> B\nlinkStyle 0 interpolate basis",
        "flowchart TB\nA\nstyle A filter:blur(2px)",
    ] {
        let (a, s, c) = setup();
        match merman_core::parse_flowchart_native_controlled(
            source,
            &merman_core::NativeParsePolicy::default(),
            &c,
        ) {
            Err(merman_core::NativeParseError::Unsupported {
                feature: merman_core::NativeFeature::Curve,
            }) => {}
            Ok(input) => assert!(matches!(
                prepared(&input, &s, &c),
                Err(CanonicalError::UnsupportedProperty)
            )),
            Err(error) => panic!("valid unsupported input was misclassified: {error:?}"),
        }
        drop(c);
        drop(s);
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn empty_graph_and_pre_cancelled_job_keep_source_and_return_all_real_storage() {
    let (a, s, c) = setup();
    let input = parse("flowchart TB\n", &c);
    let output = render_flowchart_native_controlled(
        &input,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font,
        &c,
    )
    .unwrap();
    assert_eq!(output.paths, 0);
    assert_eq!(output.commands, 0);
    drop(output);
    let before = a.allocations.load(Ordering::SeqCst);
    c.cancel();
    assert!(matches!(
        render_flowchart_native_controlled(
            &input,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &c
        ),
        Err(CanonicalError::Cancelled)
    ));
    assert_eq!(a.allocations.load(Ordering::SeqCst), before);
    drop(input);
    drop(c);
    drop(s);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}

#[test]
fn authored_class_style_precedence_and_rgba_survive_native_scene_admission() {
    let (a, s, c) = setup();
    let input = parse(
        "flowchart TB\nclassDef default fill:#12345680,stroke:#abcdef40,color:#00112230\nclassDef alert fill:#654321c0,font-weight:bold\nA:::alert --> B\nstyle A fill:#ffffffe0\nlinkStyle 0 stroke:#aabbcc50,stroke-width:3",
        &c,
    );
    let layout = prepared(&input, &s, &c).unwrap();
    let a_cell = layout.lookup("A").unwrap();
    let b_cell = layout.lookup("B").unwrap();
    assert_eq!(
        layout.cells[a_cell].style.paint.fill,
        Some(Color::rgba(0xffffff, 224))
    );
    assert_eq!(layout.cells[a_cell].style.font.weight, 700);
    assert_eq!(
        layout.cells[b_cell].style.paint.fill,
        Some(Color::rgba(0x123456, 128))
    );
    assert_eq!(
        layout.edges[0].style.paint.stroke,
        Some(Color::rgba(0xaabbcc, 80))
    );
    let output = layout.paint(&Font).unwrap();
    assert!(output.svg.contains("fill-opacity=\"0."));
    assert!(output.svg.contains("stroke-opacity=\"0."));
    accept_scene(output, &s, &c);
    drop(input);
    drop(c);
    drop(s);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}

#[test]
fn four_frozen_upstream_inputs_reach_new_scene_without_changing_old_oracles() {
    // Original sources and their original locks/notices are preserved under tests/fixtures.
    // This tests native flowchart geometry only; it does not rerun or qualify
    // the old Merman/resvg attempt, nor compare different-font pixels as parity.
    let fixtures = [
        (
            include_str!("../../../../tests/fixtures/mermaid-flowchart-v1/basic.mmd"),
            4,
            3,
            0,
        ),
        (
            include_str!("../../../../tests/fixtures/mermaid-flowchart-v1/class_style.mmd"),
            2,
            1,
            0,
        ),
        (
            include_str!(
                "../../../../tests/fixtures/mermaid-flowchart-v1/stress_flowchart_long_labels_punctuation_unicode_006.mmd"
            ),
            5,
            4,
            1,
        ),
        (
            include_str!(
                "../../../../tests/fixtures/mermaid-flowchart-v1/stress_flowchart_subgraph_title_unicode_and_entities_043.mmd"
            ),
            2,
            1,
            1,
        ),
    ];
    for (source, nodes, edges, groups) in fixtures {
        let (a, s, c) = setup();
        let input = parse(source, &c);
        assert_eq!(
            input
                .0
                .nodes
                .iter()
                .filter(|n| !n.is_subgraph_anchor())
                .count(),
            nodes
        );
        assert_eq!(input.0.edges.len(), edges);
        assert_eq!(input.0.subgraphs.len(), groups);
        let layout = prepared(&input, &s, &c)
            .unwrap_or_else(|error| panic!("frozen source failed: {error:?}\n{source}"));
        let output = layout.paint(&Font).unwrap();
        accept_scene(output, &s, &c);
        drop(input);
        drop(c);
        drop(s);
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn eight_nested_groups_accept_a_leaf_without_increasing_group_limit() {
    let (a, s, c) = setup();
    let mut source = String::from("flowchart TB\n");
    for depth in 0..8 {
        source.push_str(&format!("subgraph G{depth}[Group {depth}]\n"));
    }
    source.push_str("A[Leaf]\n");
    for _ in 0..8 {
        source.push_str("end\n");
    }
    let input = parse(&source, &c);
    let layout = prepared(&input, &s, &c).unwrap();
    assert_eq!(
        layout
            .cells
            .iter()
            .filter(|cell| cell.group.is_some())
            .map(|cell| cell.depth)
            .max(),
        Some(8)
    );
    let leaf = layout.lookup("A").unwrap();
    assert_eq!(layout.cells[leaf].depth, 9);
    let r = layout.cells[leaf].bounds;
    assert!(r.right > r.left && r.bottom > r.top);
    accept_scene(layout.paint(&Font).unwrap(), &s, &c);
    drop(input);
    drop(c);
    drop(s);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
    let (a, s, c) = setup();
    let ninth = source.replacen("A[Leaf]", "subgraph G8[Too deep]\nA[Leaf]\nend", 1);
    assert!(matches!(
        merman_core::parse_flowchart_native_controlled(
            &ninth,
            &merman_core::NativeParsePolicy::default(),
            &c
        ),
        Err(merman_core::NativeParseError::Memory(
            MemoryError::BudgetLimited
        ))
    ));
    drop(c);
    drop(s);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}

fn typed_markers(model: &FlowchartModel) -> Vec<StateMarker> {
    model
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(node, value)| {
            let kind = match value.id.as_str() {
                "I" => StateMarkerKind::Initial,
                "F" => StateMarkerKind::Final,
                _ => return None,
            };
            Some(StateMarker { node, kind })
        })
        .collect()
}

#[test]
fn typed_markers_are_small_before_topology_and_routes_terminate_on_the_outer_circle() {
    for direction in ["TB", "BT", "LR", "RL"] {
        for body in [
            "I((\"\")) --> F(((\"\")))",
            "I((\"\")) --> A(Ordinary)\nA --> F(((\"\")))",
            "I((\"\")) -->|first| A(Ordinary)\nI -->|parallel| A\nA -->|finish| F(((\"\")))",
            "subgraph O[Outer]\ndirection LR\nI((\"\")) --> A(Ordinary)\nend\nsubgraph S[Sibling]\nsubgraph N[Nested]\ndirection RL\nB(Other) --> F(((\"\")))\nend\nend\nA --> B",
        ] {
            let (a, s, c) = setup();
            let source = format!("flowchart {direction}\n{body}");
            let input = parse(&source, &c);
            let markers = typed_markers(&input.0);
            assert_eq!(markers.len(), 2);
            let mut layout = Layout::new(
                &input.0,
                &input.1,
                &markers,
                &s,
                &c,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &Font,
            )
            .unwrap();
            for cell in &layout.cells {
                if cell.state_marker.is_some() {
                    assert_eq!((cell.width, cell.height), (14.0, 14.0));
                } else if cell.node.is_some() {
                    assert!(cell.width >= 40.0 && cell.height >= 32.0);
                }
            }
            layout.topology().unwrap();
            layout.plan_segments().unwrap();
            layout.place().unwrap();
            layout
                .route()
                .unwrap_or_else(|error| panic!("{source}: {error:?}"));
            // route() also runs its obstacle, label, and portal validation.
            for edge in &layout.edges {
                let route = &edge.route;
                assert!(route.len() >= 2);
                assert_ne!(route[0], route[1]);
                assert_ne!(route[route.len() - 1], route[route.len() - 2]);
                assert!(route.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
                for (endpoint, point) in [(edge.from, route[0]), (edge.to, route[route.len() - 1])]
                {
                    let cell = &layout.cells[endpoint];
                    if cell.state_marker.is_some() {
                        let middle = center(cell.bounds);
                        let (dx, dy) = (point.x - middle.x, point.y - middle.y);
                        assert!((dx.hypot(dy) - 7.0).abs() < 1e-6);
                        assert!(dx.abs() < 1e-6 || dy.abs() < 1e-6);
                    }
                }
            }
            for portal in &layout.portals {
                assert!(
                    layout
                        .edges
                        .iter()
                        .any(|edge| edge.route.contains(&portal.point))
                );
            }
            for cell in &layout.cells[..layout.root] {
                if cell.parent != layout.root {
                    let owner = layout.cells[cell.parent].bounds;
                    assert!(
                        cell.bounds.left >= owner.left
                            && cell.bounds.right <= owner.right
                            && cell.bounds.top >= owner.top
                            && cell.bounds.bottom <= owner.bottom
                    );
                }
            }
            accept_scene(layout.paint(&Font).unwrap(), &s, &c);
            drop((input, c, s));
            assert_eq!(a.used.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn invalid_private_marker_metadata_rejects_without_mutating_the_parsed_model() {
    let (a, s, c) = setup();
    let input = parse(
        "flowchart TB\nsubgraph G[Group]\nI((\"\"))\nF(((\"\")))\nA[Ordinary]\nend\nG --> Z",
        &c,
    );
    let markers = typed_markers(&input.0);
    let anchor = input
        .0
        .nodes
        .iter()
        .position(|node| node.is_subgraph_anchor())
        .unwrap();
    let ordinary = input
        .0
        .nodes
        .iter()
        .position(|node| node.id == "A")
        .unwrap();
    let original = format!("{:?}", &*input);
    let base = a.used.load(Ordering::SeqCst);
    for invalid in [
        vec![StateMarker {
            node: input.0.nodes.len(),
            kind: StateMarkerKind::Initial,
        }],
        vec![markers[0], markers[0]],
        vec![markers[1], markers[0]],
        vec![StateMarker {
            node: ordinary,
            kind: StateMarkerKind::Initial,
        }],
        vec![StateMarker {
            node: markers[0].node,
            kind: StateMarkerKind::Final,
        }],
        vec![StateMarker {
            node: markers[1].node,
            kind: StateMarkerKind::Initial,
        }],
        vec![StateMarker {
            node: anchor,
            kind: StateMarkerKind::Initial,
        }],
        vec![markers[0]; 129],
    ] {
        let result = render_state_flowchart_native_controlled(
            &input,
            &invalid,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &c,
        );
        assert!(matches!(result, Err(CanonicalError::InvalidReference)));
        assert_eq!(format!("{:?}", &*input), original);
        assert_eq!(a.used.load(Ordering::SeqCst), base);
    }
    let (foreign_a, foreign_scope, foreign_control) = setup();
    assert!(matches!(
        render_state_flowchart_native_controlled(
            &input,
            &markers,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &foreign_control,
        ),
        Err(CanonicalError::Memory(MemoryError::Unfunded))
    ));
    assert_eq!(format!("{:?}", &*input), original);
    accept_scene(
        render_flowchart_native_controlled(
            &input,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &c,
        )
        .unwrap(),
        &s,
        &c,
    );
    drop((input, c, s, foreign_control, foreign_scope));
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
    assert_eq!(foreign_a.used.load(Ordering::SeqCst), 0);
}

#[test]
fn untyped_circles_doublecircles_and_circular_edge_markers_keep_generic_geometry_and_paint() {
    for (definition, shape) in [
        ("A((\"\"))", Shape::Circle),
        ("A((Label))", Shape::Circle),
        ("A(((\"\")))", Shape::DoubleCircle),
        ("A(((Label)))", Shape::DoubleCircle),
    ] {
        let (a, s, c) = setup();
        let input = parse(&format!("flowchart TB\n{definition}"), &c);
        let layout = prepared(&input, &s, &c).unwrap();
        let cell = &layout.cells[0];
        let label = cell.label.as_ref().unwrap();
        let expected = shape.dimensions(label.width, label.height, LayoutPolicy::default().padding);
        assert_eq!((cell.width, cell.height), expected);
        assert!(cell.width > 40.0);
        assert_eq!(cell.state_marker, None);
        let scene = render_flowchart_native_controlled(
            &input,
            LayoutPolicy::default(),
            ThemePolicy::default(),
            &Font,
            &c,
        )
        .unwrap();
        let path = scene
            .svg
            .split("<path ")
            .skip(1)
            .find(|path| path.starts_with("fill=\"#f1f3ff\" stroke=\"#515a85\""))
            .unwrap();
        let d = path
            .split_once(" d=\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0;
        assert_eq!(
            d.matches('C').count(),
            if shape == Shape::Circle { 4 } else { 8 }
        );
        assert_eq!(
            d.matches('M').count(),
            if shape == Shape::Circle { 1 } else { 2 }
        );
        drop((scene, layout));
        drop((input, c, s));
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
    let (a, s, c) = setup();
    let input = parse("flowchart TB\nA o--o B", &c);
    assert!(
        input
            .0
            .edges
            .iter()
            .all(|edge| edge.start_marker == FlowEdgeMarker::Circle
                && edge.end_marker == FlowEdgeMarker::Circle)
    );
    let layout = prepared(&input, &s, &c).unwrap();
    let diameter = 6.0 + layout.edges[0].style.paint.stroke_width;
    let output = render_flowchart_native_controlled(
        &input,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font,
        &c,
    )
    .unwrap();
    let circles: Vec<_> = output
        .svg
        .split("<path ")
        .skip(1)
        .filter(|path| path.starts_with("fill=\"#545c70\" stroke=\"#545c70\""))
        .collect();
    assert_eq!(circles.len(), 2);
    for path in circles {
        let data = path
            .split_once(" d=\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0;
        assert_eq!(data.matches('C').count(), 4);
        let values: Vec<f64> = data
            .split(|c: char| c.is_ascii_alphabetic() || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        for axis in [0, 1] {
            let min = values
                .iter()
                .skip(axis)
                .step_by(2)
                .copied()
                .fold(f64::INFINITY, f64::min);
            let max = values
                .iter()
                .skip(axis)
                .step_by(2)
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            assert!((max - min - diameter).abs() < 1e-6);
        }
    }
    drop((output, layout));
    drop((input, c, s));
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}
