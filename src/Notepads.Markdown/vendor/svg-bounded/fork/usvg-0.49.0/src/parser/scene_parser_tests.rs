// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_svg_budget::{
    Backend, Error as BudgetError, FailureKind, FirstFailureRef, LimitKind, Limits, ResourcesRef,
    Statistics, Ticket,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
pub(crate) struct LedgerState {
    used: AtomicUsize,
    tickets: AtomicUsize,
}
struct Ledger(Arc<LedgerState>);
impl Backend for Ledger {
    fn reserve(
        &self,
        _: markdown_svg_budget::ResourceKind,
        bytes: usize,
    ) -> Result<Ticket, BudgetError> {
        let id = self.0.tickets.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.used.fetch_add(bytes, Ordering::SeqCst);
        Ticket::new(bytes as u64, id as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), BudgetError> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), BudgetError> {
        self.0
            .used
            .fetch_sub(ticket.account_nonce() as usize, Ordering::SeqCst);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), BudgetError> {
        Ok(())
    }
}
pub(crate) fn request(
    nodes: u32,
    segments: u32,
) -> (ResourcesRef, Arc<LedgerState>, Arc<ProcessingContext>) {
    request_with_limits(Limits {
        expanded_nodes: nodes,
        converted_segments: segments,
        ..Limits::default()
    })
}
fn request_with_limits(limits: Limits) -> (ResourcesRef, Arc<LedgerState>, Arc<ProcessingContext>) {
    let ledger = Arc::new(LedgerState::default());
    let resources = ResourcesRef::new_prepaid(
        Ledger(ledger.clone()),
        FirstFailureRef::new_prepaid(),
        limits,
    );
    let processing = Arc::new(ProcessingContext::with_resources(resources.clone()));
    (resources, ledger, processing)
}
fn document(body: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='32' height='32'>{body}</svg>"
    )
}
fn parse(source: &str, processing: &Arc<ProcessingContext>) -> Result<Tree, Error> {
    let options = Options {
        processing: processing.clone(),
        ..Options::default()
    };
    Tree::from_data_controlled(source.as_bytes(), &options)
}
pub(crate) fn retire(resources: &ResourcesRef, ledger: &LedgerState) {
    assert_eq!(ledger.used.load(Ordering::SeqCst), 0);
    assert_eq!(resources.statistics().live_leases, 0);
    assert_eq!(resources.statistics().scratch_bytes, 0);
    assert_eq!(resources.statistics().depth, 0);
    assert_eq!(resources.statistics().release_failures, 0);
}
fn successful_counts(source: &str) -> Statistics {
    let (resources, ledger, processing) = request(131_072, 1_048_576);
    let tree = parse(source, &processing).unwrap();
    #[cfg(feature = "writer")]
    {
        let ordinary = Tree::from_data(source.as_bytes(), &Options::default()).unwrap();
        assert_eq!(
            ordinary.to_string(&crate::WriteOptions::default()),
            tree.to_string(&crate::WriteOptions::default())
        );
    }
    let counts = resources.statistics();
    assert_eq!(processing.counts().2, 0);
    assert_eq!(processing.counts().3, 0);
    drop(tree);
    retire(&resources, &ledger);
    assert_eq!(resources.statistics().expanded_nodes, counts.expanded_nodes);
    assert_eq!(
        resources.statistics().converted_segments,
        counts.converted_segments
    );
    drop(processing);
    resources.validate_retired().unwrap();
    counts
}
fn boundary(source: &str, nodes: u32, segments: u32, failure: Option<LimitKind>) {
    let (resources, ledger, processing) = request(nodes, segments);
    let tree = parse(source, &processing);
    match failure {
        Some(limit) => {
            assert!(tree.is_err(), "nodes={nodes}, segments={segments}");
            let error = processing.resource_failure().unwrap();
            assert_eq!(error.kind, FailureKind::Quota);
            assert_eq!(error.limit, limit);
        }
        None => assert!(tree.is_ok(), "nodes={nodes}, segments={segments}: {tree:?}"),
    }
    assert_eq!(processing.counts().2, 0, "no next conversion after quota");
    assert_eq!(processing.counts().3, 0);
    drop(tree);
    retire(&resources, &ledger);
    drop(processing);
    resources.validate_retired().unwrap();
}

#[test]
fn scene_parser_fixed_path_roots_skip_and_group_move_costs() {
    for (body, expected) in [
        ("", (1, 0)),
        ("<path d='M1 1L20 1L20 20Z'/>", (2, 4)),
        (
            "<path d='M1 1L20 1L20 20Z' fill='none' stroke='none'/>",
            (2, 4),
        ),
        ("<g><path d='M1 1L20 1L20 20Z'/></g>", (3, 4)),
        ("<rect width='0' height='5'/>", (1, 0)),
        ("<path display='none' d='M1 1L20 1L20 20Z'/>", (1, 0)),
        ("<rect width='8' height='8'/>", (2, 5)),
    ] {
        let source = document(body);
        let counts = successful_counts(&source);
        assert_eq!(
            (counts.expanded_nodes, counts.converted_segments),
            expected,
            "{body}"
        );
        boundary(&source, expected.0 as u32, expected.1 as u32, None);
        boundary(
            &source,
            expected.0 as u32 - 1,
            expected.1 as u32,
            Some(LimitKind::ExpandedNodes),
        );
        if expected.1 != 0 {
            boundary(
                &source,
                expected.0 as u32,
                expected.1 as u32 - 1,
                Some(LimitKind::ConvertedSegments),
            );
        }
    }
}

#[test]
fn scene_parser_resource_and_expansion_routes_stop_at_each_small_limit() {
    let fixtures = [
        (
            "clip-shared",
            "<defs><clipPath id='c' clipPathUnits='userSpaceOnUse'><rect width='7' height='7'/></clipPath></defs><rect width='8' height='8' clip-path='url(#c)'/><rect x='10' width='8' height='8' clip-path='url(#c)'/>",
        ),
        (
            "mask-empty",
            "<defs><mask id='m'><rect width='8' height='8'/></mask></defs><path d='M0 0L8 0' mask='url(#m)'/>",
        ),
        (
            "mask-wrapper",
            "<defs><mask id='m' maskContentUnits='objectBoundingBox'><rect width='1' height='1'/></mask></defs><rect width='8' height='8' mask='url(#m)'/>",
        ),
        (
            "pattern-viewbox",
            "<defs><pattern id='p' patternUnits='userSpaceOnUse' width='8' height='8' viewBox='0 0 4 4'><rect width='4' height='4'/></pattern></defs><rect width='20' height='20' fill='url(#p)'/>",
        ),
        (
            "pattern-shared-objects",
            "<defs><pattern id='p' width='.5' height='.5' patternContentUnits='objectBoundingBox'><rect width='.5' height='.5'/></pattern></defs><rect width='10' height='10' fill='url(#p)'/><rect x='12' width='15' height='15' fill='url(#p)'/>",
        ),
        (
            "pattern-nested",
            "<defs><pattern id='inner' width='.5' height='.5' patternContentUnits='objectBoundingBox'><rect width='.5' height='.5'/></pattern><pattern id='outer' patternUnits='userSpaceOnUse' width='8' height='8'><rect width='8' height='8' fill='url(#inner)'/></pattern></defs><rect width='20' height='20' fill='url(#outer)'/>",
        ),
        (
            "nested-svg",
            "<svg x='1' y='1' width='12' height='12' viewBox='0 0 4 4'><rect width='4' height='4'/></svg>",
        ),
        (
            "use-symbol",
            "<defs><symbol id='s' viewBox='0 0 4 4'><rect width='4' height='4'/></symbol></defs><use xlink:href='#s' width='12' height='12'/><use x='14' xlink:href='#s' width='12' height='12'/>",
        ),
        (
            "marker-split",
            "<defs><marker id='m' markerWidth='3' markerHeight='3'><path d='M0 0L3 1L0 3Z'/></marker></defs><path d='M1 1L20 1L20 20Z' fill='red' stroke='blue' paint-order='fill markers stroke' marker-start='url(#m)'/>",
        ),
        (
            "feimage-link",
            "<defs><rect id='r' width='8' height='8'/><filter id='f'><feImage xlink:href='#r'/></filter></defs><rect width='16' height='16' filter='url(#f)'/>",
        ),
        (
            "feimage-embedded",
            "<defs><filter id='f'><feImage xlink:href='data:image/svg+xml,%3Csvg%20xmlns=%27http://www.w3.org/2000/svg%27%20width=%278%27%20height=%278%27%3E%3Crect%20width=%278%27%20height=%278%27/%3E%3C/svg%3E'/></filter></defs><rect width='16' height='16' filter='url(#f)'/>",
        ),
        (
            "image-svg-slice",
            "<image width='16' height='12' preserveAspectRatio='xMidYMid slice' xlink:href='data:image/svg+xml,%3Csvg%20xmlns=%27http://www.w3.org/2000/svg%27%20width=%278%27%20height=%278%27%3E%3Crect%20width=%278%27%20height=%278%27/%3E%3C/svg%3E'/>",
        ),
    ];
    for (name, body) in fixtures {
        let source = document(body);
        let counts = successful_counts(&source);
        let expected = match name {
            "clip-shared" => (7, 15),
            "mask-empty" => (4, 2),
            "mask-wrapper" => (6, 10),
            "pattern-viewbox" => (5, 10),
            "pattern-shared-objects" | "pattern-nested" => (9, 20),
            "nested-svg" => (6, 10),
            "use-symbol" => (13, 20),
            "marker-split" => (8, 17),
            "feimage-link" => (5, 10),
            "feimage-embedded" | "image-svg-slice" => (8, 10),
            _ => unreachable!(),
        };
        assert_eq!(
            (counts.expanded_nodes, counts.converted_segments),
            expected,
            "{name}"
        );
        println!(
            "scene-fixture {name}: nodes={}, segments={}",
            counts.expanded_nodes, counts.converted_segments
        );
        for nodes in 0..=counts.expanded_nodes as u32 {
            boundary(
                &source,
                nodes,
                1_048_576,
                (u64::from(nodes) < counts.expanded_nodes).then_some(LimitKind::ExpandedNodes),
            );
        }
        for segments in 0..=counts.converted_segments as u32 {
            boundary(
                &source,
                131_072,
                segments,
                (u64::from(segments) < counts.converted_segments)
                    .then_some(LimitKind::ConvertedSegments),
            );
        }
    }
}

// Actual default boundaries, limited to optimized runs. These use the test
// backend's structural/work account, not an OS/RSS or wall-clock qualification.
#[cfg(not(debug_assertions))]
#[test]
fn scene_parser_default_converted_segments_limit_minus_equal_plus_one() {
    for verbs in [1_048_575usize, 1_048_576, 1_048_577] {
        let lines = verbs - 1;
        let mut data = String::from("M0 0");
        data.push_str(&"L1 1L0 0".repeat(lines / 2));
        if lines % 2 != 0 {
            data.push_str("L1 1");
        }
        let source = document(&format!("<path d='{data}'/>"));
        let (resources, ledger, processing) = request(131_072, 1_048_576);
        let tree = parse(&source, &processing);
        if verbs <= 1_048_576 {
            assert!(tree.is_ok());
            assert_eq!(resources.statistics().expanded_nodes, 2);
            assert_eq!(resources.statistics().converted_segments, verbs as u64);
        } else {
            assert!(tree.is_err());
            assert_eq!(
                processing.resource_failure().unwrap().limit,
                LimitKind::ConvertedSegments
            );
            assert_eq!(resources.statistics().expanded_nodes, 1);
            assert_eq!(resources.statistics().converted_segments, 0);
        }
        println!(
            "default-segments input-verbs={verbs}, admitted={}",
            resources.statistics().converted_segments
        );
        drop(tree);
        retire(&resources, &ledger);
        drop(processing);
        resources.validate_retired().unwrap();
    }
}

#[cfg(not(debug_assertions))]
#[test]
fn scene_parser_expanded_nodes_limit_minus_equal_plus_one_without_scratch_interference() {
    // root + source Path + marker outer + clip root/Path + two Nodes per mid.
    for (lines, empty_group, expected_nodes) in [
        (65_534usize, false, 131_071u64),
        (65_534, true, 131_072),
        (65_535, false, 131_073),
    ] {
        // Isolate the node quota from the independent allocation quota. The
        // original64MiB case is retained in the separate default-budget test.
        let (resources, ledger, processing) = request_with_limits(Limits {
            expanded_nodes: 131_072,
            converted_segments: 1_048_576,
            scratch_bytes: 128 * 1024 * 1024,
            ..Limits::default()
        });
        let tree = parse(&marker_boundary_document(lines, empty_group), &processing);
        if expected_nodes <= 131_072 {
            assert!(
                tree.is_ok(),
                "actual={:?}; statistics={:?}",
                tree.as_ref().err(),
                resources.statistics()
            );
            assert_eq!(resources.statistics().expanded_nodes, expected_nodes);
        } else {
            assert!(tree.is_err());
            assert_eq!(
                processing.resource_failure().unwrap().limit,
                LimitKind::ExpandedNodes
            );
            assert_eq!(resources.statistics().expanded_nodes, 131_072);
        }
        println!(
            "default-nodes expected-materialized={expected_nodes}, admitted={}",
            resources.statistics().expanded_nodes
        );
        drop(tree);
        retire(&resources, &ledger);
        drop(processing);
        resources.validate_retired().unwrap();
    }
}

#[cfg(not(debug_assertions))]
fn marker_boundary_document(lines: usize, empty_group: bool) -> String {
    let mut data = String::from("M0 0");
    data.push_str(&"L1 1L0 0".repeat(lines / 2));
    if lines % 2 != 0 {
        data.push_str("L1 1");
    }
    let body = format!(
        "<defs><marker id='m' markerWidth='3' markerHeight='3' orient='0'><path d='M0 0L3 1L0 3Z'/></marker></defs><path d='{data}' marker-mid='url(#m)'/>{}",
        if empty_group { "<g/>" } else { "" }
    );
    document(&body)
}

#[cfg(all(not(debug_assertions), target_arch = "x86_64"))]
#[test]
fn scene_parser_default_scratch_precedes_node_limit_for_large_marker_expansion() {
    // The old default-budget positive assumption failed. Keep that exact first
    // input and actual Scratch failure; independent maxima are not simultaneous.
    let (resources, ledger, processing) = request(131_072, 1_048_576);
    let tree = parse(&marker_boundary_document(65_534, false), &processing);
    assert!(tree.is_err());
    let error = processing.resource_failure().unwrap();
    assert_eq!(
        (error.kind, error.limit),
        (FailureKind::Quota, LimitKind::Scratch)
    );
    assert!(resources.statistics().expanded_nodes < 131_072);
    drop(tree);
    retire(&resources, &ledger);
    drop(processing);
    resources.validate_retired().unwrap();
}

#[test]
fn scene_parser_shared_geometry_split_has_two_scene_paths() {
    let source = document(
        "<defs><marker id='m' markerWidth='3' markerHeight='3'><path d='M0 0L3 1L0 3Z'/></marker></defs><path d='M1 1L20 1L20 20Z' fill='red' stroke='blue' paint-order='fill markers stroke' marker-start='url(#m)'/>",
    );
    let (resources, ledger, processing) = request(131_072, 1_048_576);
    let tree = parse(&source, &processing).unwrap();
    let mut paths = Vec::new();
    fn collect<'a>(group: &'a Group, output: &mut Vec<&'a Path>) {
        for child in group.children() {
            match child {
                Node::Group(group) => collect(group, output),
                Node::Path(path) if path.data().len() == 4 && path.data().points()[0].x == 1.0 => {
                    output.push(path)
                }
                _ => (),
            }
        }
    }
    collect(tree.root(), &mut paths);
    assert_eq!(paths.len(), 2);
    assert_eq!(
        paths[0].data().verbs().as_ptr(),
        paths[1].data().verbs().as_ptr()
    );
    assert_eq!(resources.statistics().converted_segments, 17); // Source 4+4, marker 4, viewport clip 5.
    drop(tree);
    retire(&resources, &ledger);
    drop(processing);
    resources.validate_retired().unwrap();
}

#[test]
fn scene_parser_same_request_never_resets_on_tree_retirement() {
    let (resources, ledger, processing) = request(1, 0);
    let source = document("");
    drop(parse(&source, &processing).unwrap());
    retire(&resources, &ledger);
    assert!(parse(&source, &processing).is_err());
    assert_eq!(
        processing.resource_failure().unwrap().limit,
        LimitKind::ExpandedNodes
    );
    assert_eq!(resources.statistics().expanded_nodes, 1);
    assert_eq!(resources.statistics().converted_segments, 0);
    assert!(parse(&source, &processing).is_err());
    assert_eq!(resources.statistics().expanded_nodes, 1);
    retire(&resources, &ledger);
    drop(processing);
    resources.validate_retired().unwrap();
}

#[cfg(feature = "text")]
#[test]
fn scene_parser_font_svg_colr_cache_miss_and_hit_clone_owned_graphs() {
    for svg in [false, true] {
        let (source_cache, _owner, id) = super::font_replay_color_tests::mock_data(
            super::font_replay_color_tests::sfnt(!svg, svg, false),
            false,
        );
        let (resources, ledger, processing) = request(131_072, 1_048_576);
        let mut cache = Cache::with_processing(processing.clone(), source_cache.fontdb.clone());
        let first_cost;
        let second_cost;
        if svg {
            drop(cache.fontdb_svg(id, GlyphId(1)).unwrap());
            first_cost = resources.statistics();
            drop(cache.fontdb_svg(id, GlyphId(1)).unwrap());
            second_cost = resources.statistics();
            assert_eq!(cache.cache_svg.len(), 1);
        } else {
            drop(
                cache
                    .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
                    .unwrap(),
            );
            first_cost = resources.statistics();
            drop(
                cache
                    .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
                    .unwrap(),
            );
            second_cost = resources.statistics();
            assert_eq!(cache.cache_colr.len(), 1);
        }
        assert!(first_cost.expanded_nodes > 0 && first_cost.converted_segments > 0);
        assert_eq!(
            first_cost.expanded_nodes * 3,
            second_cost.expanded_nodes * 2
        );
        assert_eq!(
            first_cost.converted_segments * 3,
            second_cost.converted_segments * 2
        );
        drop(cache);
        retire(&resources, &ledger);
        drop(processing);
        resources.validate_retired().unwrap();
        let (resources, ledger, processing) =
            request(second_cost.expanded_nodes as u32 - 1, 1_048_576);
        let mut cache = Cache::with_processing(processing.clone(), source_cache.fontdb.clone());
        if svg {
            assert!(cache.fontdb_svg(id, GlyphId(1)).is_some());
            assert!(cache.fontdb_svg(id, GlyphId(1)).is_none());
        } else {
            assert!(cache
                .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
                .is_some());
            assert!(cache
                .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
                .is_none());
        }
        assert_eq!(
            processing.resource_failure().unwrap().limit,
            LimitKind::ExpandedNodes
        );
        assert_eq!(
            resources.statistics().expanded_nodes,
            first_cost.expanded_nodes
        );
        assert_eq!(
            resources.statistics().converted_segments,
            first_cost.converted_segments
        );
        drop(cache);
        retire(&resources, &ledger);
        drop(processing);
        resources.validate_retired().unwrap();
    }
}
