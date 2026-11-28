// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#[test]
fn grammar_delta_copy_anchor_reads_only_an_accepted_single_copy_piece() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut ctx = Context::new(&mut host, 4);
        ctx.direct = true;
        let document = ctx.node(None, NativeKind::Document).unwrap();
        let paragraph = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
        let buffer = ctx.buffer(paragraph).unwrap();
        ctx.fact(AcceptedFact::Append {
            span: RSpan {
                buffer,
                start: 0,
                end: 2,
            },
            source: PSpan { start: 2, end: 4 },
            kind: AppendKind::Copy,
        })
        .unwrap();
        let before = ctx.host.scan_bytes;
        assert_eq!(
            ctx.accepted_copy_piece(RSpan {
                buffer,
                start: 1,
                end: 2
            })
            .unwrap(),
            PSpan { start: 3, end: 4 }
        );
        assert!(ctx.host.scan_bytes > before);
        assert_eq!(ctx.final_phase, FinalPhase::Grammar);
        assert!(ctx.host.first_fault.is_none());
        assert!(gauge.borrow().current > 0);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn grammar_delta_copy_anchor_rejects_noncopy_invalid_and_cross_piece_spans() {
    for case in 0..6 {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        let error = {
            let mut ctx = Context::new(&mut host, 4);
            ctx.direct = true;
            let document = ctx.node(None, NativeKind::Document).unwrap();
            let paragraph = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
            let buffer = ctx.buffer(paragraph).unwrap();
            if case == 0 {
                ctx.fact(AcceptedFact::Append {
                    span: RSpan {
                        buffer,
                        start: 0,
                        end: 2,
                    },
                    source: PSpan { start: 0, end: 1 },
                    kind: AppendKind::PartialTab {
                        spaces: 2,
                        column: 2,
                    },
                })
                .unwrap();
            } else {
                for at in 0..2 {
                    ctx.fact(AcceptedFact::Append {
                        span: RSpan {
                            buffer,
                            start: at,
                            end: at + 1,
                        },
                        source: PSpan {
                            start: at,
                            end: at + 1,
                        },
                        kind: AppendKind::Copy,
                    })
                    .unwrap();
                }
            }
            let span = match case {
                0 => RSpan {
                    buffer,
                    start: 0,
                    end: 1,
                },
                1 => RSpan {
                    buffer,
                    start: 0,
                    end: 2,
                },
                2 => RSpan {
                    buffer,
                    start: 1,
                    end: 1,
                },
                3 => RSpan {
                    buffer,
                    start: 2,
                    end: 3,
                },
                4 => RSpan {
                    buffer: BufferId::from_raw(99).unwrap(),
                    start: 0,
                    end: 1,
                },
                _ => RSpan {
                    buffer,
                    start: 1,
                    end: 0,
                },
            };
            let error = ctx.accepted_copy_piece(span).unwrap_err();
            assert_eq!(ctx.final_phase, FinalPhase::Failed);
            ctx.host.cancelled.store(true, Ordering::Release);
            let later = ctx
                .accepted_copy_piece(RSpan {
                    buffer,
                    start: 0,
                    end: 1,
                })
                .unwrap_err();
            assert!(Arc::ptr_eq(&error, &later));
            error
        };
        host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
        assert_eq!(gauge.borrow().current, 0);
    }
}

#[test]
fn grammar_delta_copy_anchor_latches_real_scan_cancellation_and_releases_owners() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    let error = {
        let mut ctx = Context::new(&mut host, 1);
        ctx.direct = true;
        let document = ctx.node(None, NativeKind::Document).unwrap();
        let paragraph = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
        let buffer = ctx.buffer(paragraph).unwrap();
        ctx.fact(AcceptedFact::Append {
            span: RSpan {
                buffer,
                start: 0,
                end: 1,
            },
            source: PSpan { start: 0, end: 1 },
            kind: AppendKind::Copy,
        })
        .unwrap();
        ctx.host.cancel_scan_at = Some(ctx.host.scan_calls + 1);
        let error = ctx
            .accepted_copy_piece(RSpan {
                buffer,
                start: 0,
                end: 1,
            })
            .unwrap_err();
        assert_eq!(ctx.final_phase, FinalPhase::Failed);
        let later = ctx.node(Some(document), NativeKind::Paragraph).unwrap_err();
        assert!(Arc::ptr_eq(&error, &later));
        error
    };
    host.assert_original(&error, Fault::Cancelled);
    assert_eq!(gauge.borrow().current, 0);
}
