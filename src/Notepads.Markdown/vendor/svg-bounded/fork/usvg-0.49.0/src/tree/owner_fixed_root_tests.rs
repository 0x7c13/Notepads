// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Fixed root-source pairs from the V3 independent repair review.
use super::owner::{Origin, PolicyValue};
use super::*;
use crate::geometry_test_support::{native, request, AllocationObservation, Call};
use crate::{Options, ProcessingContext};
use markdown_svg_budget::Limits;
use std::sync::Arc;

const XML: &[u8] =
    b"<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><path d='M0 0L4 4'/></svg>";
fn parse(r: Option<&markdown_svg_budget::ResourcesRef>) -> (Tree, Options<'static>) {
    let mut options = Options::default();
    if let Some(r) = r {
        options.processing = Arc::new(ProcessingContext::with_resources(r.clone()));
    }
    let tree = Tree::from_data_in_request(XML, &options).unwrap();
    (tree, options)
}
enum Subject {
    Tree(Tree),
    Node(Node),
    Group(Group),
}
impl Subject {
    fn check(
        &self,
        p: &ProcessingContext,
        copy: bool,
    ) -> Result<(), markdown_svg_budget::Error> {
        match self {
            Self::Tree(v) => {
                if copy {
                    v.try_clone_in(p).map(drop)
                } else {
                    v.validate_policy(p)
                }
            }
            Self::Node(v) => {
                if copy {
                    v.try_clone_in(p).map(drop)
                } else {
                    v.validate_policy(p)
                }
            }
            Self::Group(v) => {
                if copy {
                    v.try_clone_in(p).map(drop)
                } else {
                    v.validate_policy(p)
                }
            }
        }
    }
}

#[test]
fn fixed_root_pair_captures_real_sources_before_same_tag_target_callback() {
    for kind in 0..3 {
        for copy in [false, true] {
            for target_old in [None, Some(native(-22303))] {
                let (source, source_state) = request(Default::default());
                let (source_tree, source_options) = parse(Some(&source));
                // Both tags/headers are built by real funded requests. The
                // tested entry must add zero target callbacks/allocations.
                let mut root_a = source_tree
                    .into_root_in(&source_options.processing)
                    .unwrap();
                let (b, b_state) = request(Default::default());
                let (mut tree_b, options_b) = parse(Some(&b));
                let pb = ProcessingContext::with_resources(b.clone());
                let subject = match kind {
                    0 => {
                        std::mem::swap(&mut tree_b.root, &mut root_a);
                        drop(root_a);
                        Subject::Tree(tree_b)
                    }
                    1 => {
                        let root_b = tree_b.into_root_in(&pb).unwrap();
                        let mut header_b = crate::scene::moved_root(&pb, root_b).unwrap();
                        let old = std::mem::replace(header_b.as_mut(), root_a);
                        drop(old);
                        Subject::Node(Node::Group(header_b))
                    }
                    _ => {
                        drop(tree_b);
                        root_a.origin = Origin::new(&pb);
                        Subject::Group(root_a)
                    }
                };
                // B is both the apparent first tag and actual target. Its
                // real root/header is B, while A's real payload owners remain.
                source.record(native(-22301));
                if let Some(e) = target_old {
                    b.record(e);
                }
                let before = b_state.counts();
                b_state.arm(Call::Checkpoint, 1, native(-22302));
                let observation = AllocationObservation::begin();
                let result = subject.check(&pb, copy);
                let allocations = observation.finish();
                assert_eq!(
                    result.unwrap_err(),
                    target_old.unwrap_or(native(-22301)),
                    "kind={kind},copy={copy}"
                );
                assert_eq!(b_state.counts().checkpoint, before.checkpoint);
                assert_eq!(b_state.counts().reserve, before.reserve);
                assert_eq!(
                    allocations.alloc + allocations.zeroed + allocations.realloc,
                    0
                );
                assert_eq!(source.failure(), Some(native(-22301)));
                drop((subject, pb, source_options, options_b));
                assert_eq!(source_state.live_len(), 0);
                assert_eq!(b_state.live_len(), 0);
                source.validate_retired().unwrap();
                b.validate_retired().unwrap();
            }
        }
    }
}

#[test]
fn plain_tree_context_native_precedes_some_identity_invariant_and_new_fault() {
    for copy in [false, true] {
        for target_old in [None, Some(native(-22313))] {
            let (tree, options) = parse(None);
            tree.processing_context().record_resource(native(-22311));
            let (target, state) = request(Limits {
                scratch_bytes: 0,
                ..Limits::default()
            });
            if let Some(e) = target_old {
                target.record(e);
            }
            state.arm(Call::Checkpoint, 1, native(-22312));
            let p = ProcessingContext::with_resources(target.clone());
            let observation = AllocationObservation::begin();
            let result = if copy {
                tree.try_clone_in(&p).map(drop)
            } else {
                tree.validate_policy(&p)
            };
            let allocations = observation.finish();
            assert_eq!(result.unwrap_err(), target_old.unwrap_or(native(-22311)));
            assert_eq!(state.counts().checkpoint, 0);
            assert_eq!(state.counts().reserve, 0);
            assert_eq!(
                allocations.alloc + allocations.zeroed + allocations.realloc,
                0
            );
            assert_eq!(
                tree.processing_context().resource_failure(),
                Some(native(-22311))
            );
            drop((tree, options, p));
            assert_eq!(state.live_len(), 0);
            target.validate_retired().unwrap();
        }
    }
}
