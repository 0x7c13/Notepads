// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::super::converter::scene_parser_tests::{request, retire};
use super::*;

#[test]
fn scene_parser_pattern_update_quota_cannot_return_new_pattern_owner() {
    // Real production helper; the separate Tree fixture covers its caller.
    // This assertion cannot hide behind convert's later first-slot guard.
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><defs><pattern id='inner' width='.5' height='.5' patternContentUnits='objectBoundingBox'><rect width='.5' height='.5'/></pattern><pattern id='outer' patternUnits='userSpaceOnUse' width='8' height='8'><rect width='8' height='8' fill='url(#inner)'/></pattern></defs></svg>";
    for limit in [4, 6] {
        let (resources, ledger, processing) = request(limit, 1_048_576);
        let xml = roxmltree::Document::parse(source).unwrap();
        let doc = super::super::svgtree::Document::parse_tree_for_source(
            &xml,
            None,
            super::super::source_kind::SourceKind::GeneralImage,
            Some(resources.clone()),
        )
        .unwrap();
        let node = doc.element_by_id("outer").unwrap();
        let options = Options {
            processing: processing.clone(),
            ..Options::default()
        };
        let state = converter::State {
            refs: crate::parser::reference::RefPath::default(),
            context_proofs: [crate::parser::reference::PaintProof::default(); 2],
            parent_clip_path: None,
            parent_markers: crate::tree::owner::TreeVec::new(&options.processing),
            context_element: None,
            fe_image_link: false,
            view_box: NonZeroRect::from_xywh(0.0, 0.0, 32.0, 32.0).unwrap(),
            use_size: (None, None),
            opt: &options,
        };
        let mut cache = converter::Cache::with_processing(
            processing.clone(),
            #[cfg(feature = "text")]
            options.fontdb.clone(),
        );
        let candidate = convert_pattern(node, &state, &mut cache);
        println!(
            "pattern-helper node-limit={limit}, candidate-owner={}, admitted={}",
            candidate.is_some(),
            resources.statistics().expanded_nodes
        );
        assert_eq!(
            processing.resource_failure().unwrap().limit,
            markdown_svg_budget::LimitKind::ExpandedNodes
        );
        assert_eq!(resources.statistics().expanded_nodes, u64::from(limit));
        assert!(
            candidate.is_none(),
            "no Arc<Pattern> after failed nested update"
        );
        drop(candidate);
        drop(cache);
        drop(state);
        drop(doc);
        drop(xml);
        drop(options);
        drop(processing);
        retire(&resources, &ledger);
        resources.validate_retired().unwrap();
    }
}
