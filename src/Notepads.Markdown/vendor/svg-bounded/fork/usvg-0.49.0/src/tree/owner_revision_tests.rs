// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual root-before-allocation regressions from the V2 independent review.
use super::owner::PolicyValue;
use super::*;
use crate::geometry_test_support::{native, request, AllocationObservation, Call};
use crate::ProcessingContext;
use markdown_svg_budget::{FailureKind, Limits};
use std::sync::Arc;

#[test]
fn root_provenance_and_source_native_precede_zero_scratch_walk_allocation() {
    for plain in [false, true] {
        for kind in 0..3 {
            let (source, source_state) = request(Default::default());
            let (target, target_state) = request(Limits {
                scratch_bytes: 0,
                ..Limits::default()
            });
            let mut options = crate::Options::default();
            if !plain {
                options.processing = Arc::new(ProcessingContext::with_resources(source.clone()));
            }
            let tree=Tree::from_data_in_request(b"<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><path d='M0 0L8 0L8 8Z'/></svg>",&options).unwrap();
            if !plain {
                source.record(native(-22201));
            }
            let p = ProcessingContext::with_resources(target.clone());
            let observation = AllocationObservation::begin();
            let result = match kind {
                0 => tree.validate_policy(&p),
                1 => tree.root().validate_policy(&p),
                _ => tree.root().children()[0].validate_policy(&p),
            };
            let allocations = observation.finish();
            let expected = if plain {
                markdown_svg_budget::Error::new(
                    FailureKind::Invariant,
                    markdown_svg_budget::LimitKind::None,
                )
            } else {
                native(-22201)
            };
            assert_eq!(result.unwrap_err(), expected, "plain={plain}, kind={kind}");
            assert_eq!(target_state.counts().reserve, 0);
            assert_eq!(
                target_state.counts().checkpoint,
                0,
                "no target callback before rejecting the root"
            );
            assert_eq!(
                allocations.alloc + allocations.zeroed + allocations.realloc,
                0
            );
            if !plain {
                assert_eq!(source.failure(), Some(native(-22201)));
            }
            drop((tree, options, p));
            assert_eq!(source_state.live_len(), 0);
            assert_eq!(target_state.live_len(), 0);
            source.validate_retired().unwrap();
            target.validate_retired().unwrap();
        }
    }
}

#[test]
fn source_native_precedes_fresh_target_callback_fault_at_typed_tree_copy_entry() {
    for target_prior in [None, Some(native(-22205))] {
        let (source, source_state) = request(Default::default());
        let (target, target_state) = request(Limits {
            scratch_bytes: 0,
            ..Limits::default()
        });
        let options = crate::Options {
            processing: Arc::new(ProcessingContext::with_resources(source.clone())),
            ..crate::Options::default()
        };
        let tree=Tree::from_data_in_request(b"<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><path d='M0 0L4 4'/></svg>",&options).unwrap();
        source.record(native(-22204));
        if let Some(e) = target_prior {
            target.record(e);
        }
        target_state.arm(Call::Checkpoint, 1, native(-22206));
        let p = ProcessingContext::with_resources(target.clone());
        assert_eq!(
            tree.try_clone_in(&p).unwrap_err(),
            target_prior.unwrap_or(native(-22204))
        );
        assert_eq!(target_state.counts().checkpoint, 0);
        assert_eq!(target_state.counts().reserve, 0);
        assert_eq!(source.failure(), Some(native(-22204)));
        assert_eq!(
            target.failure(),
            Some(target_prior.unwrap_or(native(-22204)))
        );
        drop((tree, options, p));
        assert_eq!(source_state.live_len(), 0);
        assert_eq!(target_state.live_len(), 0);
        source.validate_retired().unwrap();
        target.validate_retired().unwrap();
    }
}
