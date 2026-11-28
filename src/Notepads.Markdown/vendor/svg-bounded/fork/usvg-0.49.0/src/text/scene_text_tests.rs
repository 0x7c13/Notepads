// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::geometry_test_support::request;
use markdown_svg_budget::{FailureKind, LimitKind, Limits};

fn options() -> Options<'static> {
    let mut db = Database::new();
    db.load_font_data(
        include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf").to_vec(),
    );
    Options {
        fontdb: Arc::new(db),
        font_family: "Tuffy".into(),
        ..Options::default()
    }
}

// An independent walk of the actual output, excluding Arc resources. The one
// discarded Text.flattened placeholder per successful text is accounted below.
fn output(group: &Group) -> (u64, u64, u64) {
    let mut counts = (1, 0, 0);
    for child in group.children() {
        let part = match child {
            Node::Group(group) => output(group),
            Node::Path(path) => (1, path.data().len() as u64, 0),
            Node::Text(text) => {
                let (nodes, segments, texts) = output(text.flattened());
                (nodes + 1, segments, texts + 1)
            }
            Node::Image(_) => panic!("these fixed text fixtures contain no images"),
        };
        counts.0 += part.0;
        counts.1 += part.1;
        counts.2 += part.2;
    }
    counts
}

#[test]
fn real_tuffy_text_and_all_decorations_admit_output_and_discarded_placeholder_once() {
    for content in [
        "<text x='2' y='30' font-size='16'>AB</text>",
        "<text x='2' y='30' font-size='16' text-decoration='overline underline line-through'>AB</text>",
        "<text x='2' y='30' font-size='16'>A<tspan fill='red'>B</tspan></text>",
    ] {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='120' height='80'>{content}</svg>"
        );
        let ordinary = Tree::from_data(source.as_bytes(), &options()).unwrap();
        let expected = output(ordinary.root());
        assert_eq!(expected.2, 1, "a real successfully shaped Tuffy text");
        assert!(expected.1 > 0);
        let nodes = expected.0 + expected.2;
        let segments = expected.1;
        for (node_limit, segment_limit, failure) in [
            (nodes - 1, segments, Some(LimitKind::ExpandedNodes)),
            (nodes, segments, None),
            (nodes + 1, segments + 1, None),
            (nodes, segments - 1, Some(LimitKind::ConvertedSegments)),
        ] {
            let (resources, state) = request(Limits {
                expanded_nodes: node_limit as u32,
                converted_segments: segment_limit as u32,
                ..Limits::default()
            });
            let processing = Arc::new(ProcessingContext::with_resources(resources.clone()));
            let opt = Options {
                processing: processing.clone(),
                ..options()
            };
            let candidate = Tree::from_data_controlled(source.as_bytes(), &opt);
            match failure {
                Some(limit) => {
                    assert!(
                        candidate.is_err(),
                        "{content}: {node_limit}/{segment_limit}"
                    );
                    let error = processing.resource_failure().unwrap();
                    assert_eq!((error.kind, error.limit), (FailureKind::Quota, limit));
                }
                None => {
                    let tree = candidate.as_ref().unwrap();
                    assert_eq!(output(tree.root()), expected);
                    assert_eq!(
                        tree.to_string(&crate::WriteOptions::default()),
                        ordinary.to_string(&crate::WriteOptions::default())
                    );
                    let actual = resources.statistics();
                    assert_eq!(
                        (actual.expanded_nodes, actual.converted_segments),
                        (nodes, segments)
                    );
                }
            }
            assert_eq!(processing.counts().2, 0, "no conversion after quota");
            assert_eq!(processing.counts().3, 0);
            drop((candidate, opt, processing));
            assert_eq!(state.live_len(), 0);
            resources.validate_retired().unwrap();
        }
    }
}
