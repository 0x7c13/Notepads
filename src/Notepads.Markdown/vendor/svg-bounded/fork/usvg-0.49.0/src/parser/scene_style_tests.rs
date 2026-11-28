// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::geometry_test_support::request;
use markdown_svg_budget::{FailureKind, LimitKind, Limits};
use std::sync::Arc;

#[test]
fn real_pattern_quota_cannot_be_converted_to_fallback_colour_or_stroke() {
    for aid in [AId::Fill, AId::Stroke] {
        for missing in [false, true] {
            let (resources, ledger) = request(Limits {
                expanded_nodes: 0,
                ..Limits::default()
            });
            let opt = crate::Options {
                processing: Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
                ..crate::Options::default()
            };
            let source = "<svg xmlns='http://www.w3.org/2000/svg'><defs><pattern id='p' width='.5' height='.5'><rect width='1' height='1'/></pattern></defs><rect id='r' width='8' height='8' color='red'/></svg>";
            let xml = roxmltree::Document::parse(source).unwrap();
            let doc = super::super::svgtree::Document::parse_tree_for_source(
                &xml,
                None,
                super::super::source_kind::SourceKind::GeneralImage,
                Some(resources.clone()),
            )
            .unwrap();
            let node = doc.element_by_id("r").unwrap();
            let state = converter::State {
                refs: crate::parser::reference::RefPath::default(),
                context_proofs: [crate::parser::reference::PaintProof::default(); 2],
                parent_clip_path: None,
                parent_markers: crate::tree::owner::TreeVec::new(&opt.processing),
                context_element: None,
                fe_image_link: false,
                view_box: tiny_skia_path::NonZeroRect::from_xywh(0.0, 0.0, 8.0, 8.0).unwrap(),
                use_size: (None, None),
                opt: &opt,
            };
            let mut cache =
                converter::Cache::with_processing(opt.processing.clone(), opt.fontdb.clone());
            let mut opacity = Opacity::ONE;
            let result = convert_paint(
                node,
                if missing {
                    "url(#absent) currentColor"
                } else {
                    "url(#p) currentColor"
                },
                aid,
                true,
                &state,
                &mut opacity,
                &mut cache,
            );
            if missing {
                assert!(
                    matches!(result, Some((Paint::Color(color), None)) if color == Color::new_rgb(255, 0, 0))
                );
                assert!(
                    resources.failure().is_none(),
                    "normal missing-server fallback stays legal"
                );
            } else {
                assert!(result.is_none(), "quota must not become a fallback colour");
                let error = resources.failure().unwrap();
                assert_eq!(
                    (error.kind, error.limit),
                    (FailureKind::Quota, LimitKind::ExpandedNodes)
                );
                assert!(resolve_fill(node, true, &state, &mut cache).is_none());
                assert!(resolve_stroke(node, true, &state, &mut cache).is_none());
            }
            drop(cache);
            drop(state);
            drop(doc);
            drop(xml);
            drop(opt);
            assert_eq!(ledger.live_len(), 0);
            resources.validate_retired().unwrap();
        }
    }
}
