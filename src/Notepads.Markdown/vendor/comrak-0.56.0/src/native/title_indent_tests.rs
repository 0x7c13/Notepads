// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

fn title_indent_setup(ctx: &mut Context<'_, MemoryHost>, role: PayloadKind) -> PayloadId {
    ctx.direct = true;
    let document = ctx.node(None, NativeKind::Document).unwrap();
    let paragraph = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
    let buffer = ctx.buffer(paragraph).unwrap();
    ctx.fact(AcceptedFact::Append {
        span: RSpan {
            buffer,
            start: 0,
            end: 13,
        },
        source: PSpan { start: 0, end: 13 },
        kind: AppendKind::Copy,
    })
    .unwrap();
    let payload = ctx
        .payload(
            role,
            RSpan {
                buffer,
                start: 0,
                end: 13,
            },
        )
        .unwrap();
    ctx.fact(AcceptedFact::Stage {
        payload,
        stage: 1,
        kind: StageKind::Trim,
        input: DSpan {
            payload,
            stage: 0,
            start: 1,
            end: 12,
        },
        output_start: 0,
        output_end: 11,
        value: "a\n    bcd  ",
        hidden_runs: &[],
    })
    .unwrap();
    payload
}

fn title_indent_stage<'a>(payload: PayloadId, runs: &'a [DSpan]) -> AcceptedFact<'a> {
    AcceptedFact::Stage {
        payload,
        stage: 2,
        kind: StageKind::ReferenceTitleIndent,
        input: DSpan {
            payload,
            stage: 1,
            start: 0,
            end: 11,
        },
        output_start: 0,
        output_end: 7,
        value: "a\nbcd  ",
        hidden_runs: runs,
    }
}

#[test]
fn title_indent_transport_rejects_wrong_role_phase_identity_and_geometry() {
    for case in 0..11 {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        let error = {
            let mut ctx = Context::new(&mut host, 13);
            let payload = title_indent_setup(
                &mut ctx,
                if case == 0 {
                    PayloadKind::Destination
                } else {
                    PayloadKind::Title
                },
            );
            let valid = DSpan {
                payload,
                stage: 1,
                start: 2,
                end: 6,
            };
            let mut runs = vec![valid];
            match case {
                1 => runs[0].payload = PayloadId::from_raw(2).unwrap(),
                2 => runs[0].stage = 0,
                3 => runs[0].end = 12,
                4 => runs[0].end = runs[0].start,
                5 => runs.push(valid),
                6 => {
                    runs[0].start = 6;
                    runs[0].end = 8;
                    runs.push(DSpan {
                        start: 2,
                        end: 4,
                        ..valid
                    });
                }
                _ => {}
            }
            if case == 7 {
                ctx.fact(title_indent_stage(payload, &runs)).unwrap();
            }
            let mut fact = title_indent_stage(payload, &runs);
            if let AcceptedFact::Stage {
                stage,
                input,
                kind,
                output_start,
                ..
            } = &mut fact
            {
                match case {
                    8 => *stage = 3,
                    9 => input.end = 10,
                    10 => {
                        *kind = StageKind::Entity;
                        *output_start = 1;
                    }
                    _ => {}
                }
            }
            let error = ctx.fact(fact).unwrap_err();
            assert_eq!(ctx.final_phase, FinalPhase::Failed);
            ctx.host.cancelled.store(true, Ordering::Release);
            let later = ctx
                .fact(AcceptedFact::PayloadEnd { payload, value: "" })
                .unwrap_err();
            assert!(Arc::ptr_eq(&error, &later));
            error
        };
        host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
        assert_eq!(gauge.borrow().current, 0);
    }
}

#[test]
fn title_indent_transport_restores_original_row_on_allocation_or_copy_failure() {
    for case in 0..3 {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        let error = {
            let mut ctx = Context::new(&mut host, 13);
            let payload = title_indent_setup(&mut ctx, PayloadKind::Title);
            match case {
                0 => ctx.host.reserve_fault_at = Some(ctx.host.reserve_calls + 1),
                1 => ctx.host.actual_fault_at = Some(ctx.host.actual_calls + 1),
                _ => ctx.host.cancel_copy_at = Some(ctx.host.copy_calls + 1),
            }
            let runs = [DSpan {
                payload,
                stage: 1,
                start: 2,
                end: 6,
            }];
            let error = ctx.fact(title_indent_stage(payload, &runs)).unwrap_err();
            assert_eq!(ctx.final_phase, FinalPhase::Failed);
            // Observe the physical row without polling a Host already closed
            // by the original fault. Production has no such recovery read.
            let row = ctx
                .mappings
                .payloads
                .get(payload.get() as usize - 1, |_| {
                    Ok::<(), std::convert::Infallible>(())
                })
                .unwrap()
                .unwrap();
            assert_eq!(row.value.bytes, 11);
            assert_eq!(row.stage, 1);
            assert_eq!(row.value.atoms.len(), 3);
            let later = ctx
                .fact(AcceptedFact::PayloadEnd { payload, value: "" })
                .unwrap_err();
            assert!(Arc::ptr_eq(&error, &later));
            error
        };
        host.assert_original(
            &error,
            match case {
                0 => Fault::Reserve,
                1 => Fault::Actual,
                _ => Fault::Cancelled,
            },
        );
        assert_eq!(gauge.borrow().current, 0);
    }
}
