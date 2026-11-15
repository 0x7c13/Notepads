// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Visual comparison helper tests; no real HB/engine consumer qualification.

use super::*;
use crate::geometry_test_support::{cancelled, native, request, Call};
use crate::tree::owner::{TreeString, TreeVec, TryCopy};
use crate::*;
use markdown_svg_budget::{Error, FailureKind, LimitKind, Limits, WorkControl};

fn copy_span(v: &TextSpan) -> TextSpan {
    v.try_copy_in(&ProcessingContext::default()).unwrap()
}
fn span() -> TextSpan {
    TextSpan {
        computed_color: svgtypes::Color::black(),
        source_owner: 1,
        source_ancestors: TreeVec::plain(vec![1]),
        length_directives: TreeVec::plain(vec![]),
        decoration_owners: [None; 3],
        decoration_sources: [None, None, None],
        start: 0,
        end: 1,
        fill: Some(Fill::default()),
        stroke: None,
        paint_order: PaintOrder::FillAndStroke,
        font: Font {
            families: TreeVec::plain(vec![FontFamily::SansSerif]),
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
            weight: 400,
            variations: TreeVec::plain(vec![]),
        },
        font_size: NonZeroPositiveF32::new(12.0).unwrap(),
        small_caps: false,
        apply_kerning: true,
        font_optical_sizing: FontOpticalSizing::Auto,
        language: TreeString::plain("und".into()),
        direction: 0,
        bidi_scopes: TreeVec::plain(vec![]),
        decoration: TextDecoration {
            underline: None,
            overline: None,
            line_through: None,
        },
        dominant_baseline: DominantBaseline::Auto,
        alignment_baseline: AlignmentBaseline::Auto,
        baseline_shift: TreeVec::plain(vec![]),
        visible: true,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        text_length: None,
        length_adjust: LengthAdjust::Spacing,
    }
}

fn stroke() -> Stroke {
    Stroke {
        paint: Paint::Color(Color::black()),
        dasharray: None,
        dashoffset: 0.0,
        miterlimit: StrokeMiterlimit::new(4.0),
        opacity: Opacity::ONE,
        width: NonZeroPositiveF32::new(1.0).unwrap(),
        linecap: LineCap::Butt,
        linejoin: LineJoin::Miter,
        context_element: None,
    }
}

// The original effective-visual expression uses the original derived Font and
// NumberList equality. It is independent of the new controlled sequence/name
// helper and retains the original fill/stroke calculation and tail order.
fn original_visual(a: &TextSpan, b: &TextSpan) -> bool {
    let fill = match (&a.fill, &b.fill) {
        (None, None) => true,
        (Some(a), Some(b)) => a.paint == b.paint && a.opacity == b.opacity && a.rule == b.rule,
        _ => false,
    };
    let stroke = match (&a.stroke, &b.stroke) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.paint == b.paint
                && a.opacity == b.opacity
                && a.width == b.width
                && a.dasharray == b.dasharray
                && a.dashoffset == b.dashoffset
                && a.miterlimit.get() == b.miterlimit.get()
                && a.linecap == b.linecap
                && a.linejoin == b.linejoin
        }
        _ => false,
    };
    fill && stroke
        && a.computed_color == b.computed_color
        && a.paint_order == b.paint_order
        && a.visible == b.visible
        && a.font_size == b.font_size
        && a.font == b.font
        && a.dominant_baseline == b.dominant_baseline
        && a.alignment_baseline == b.alignment_baseline
        && a.baseline_shift == b.baseline_shift
}

#[derive(Default)]
struct Trace {
    work: Vec<u64>,
    fail_at: Option<usize>,
}

impl WorkControl for Trace {
    type Error = Error;
    fn checkpoint(&mut self, work: u64) -> Result<(), Error> {
        self.work.push(work);
        if self.fail_at == Some(self.work.len()) {
            Err(native(-941))
        } else {
            Ok(())
        }
    }
}

fn check(a: &TextSpan, b: &TextSpan) -> Vec<u64> {
    let expected = original_visual(a, b);
    let mut trace = Trace::default();
    assert_eq!(
        replay_visual_equivalent_with(a, b, &mut trace).unwrap(),
        expected
    );
    assert_eq!(replay_visual_equivalent(a, b), expected);
    let (r, state) = request(Limits::default());
    assert_eq!(try_replay_visual_equivalent(a, b, &r).unwrap(), expected);
    state.assert_live(&r, 0);
    trace.work
}

#[test]
fn effective_fields_match_original_expression_and_ignored_source_fields_stay_separate() {
    let mut a = span();
    a.stroke = Some(stroke());
    assert!(original_visual(&a, &a));
    let mutations: Vec<(&str, Box<dyn Fn(&mut TextSpan)>)> = vec![
        ("fill presence", Box::new(|b| b.fill = None)),
        (
            "fill color",
            Box::new(|b| b.fill.as_mut().unwrap().paint = Paint::Color(Color::new_rgb(1, 2, 3))),
        ),
        (
            "fill opacity",
            Box::new(|b| b.fill.as_mut().unwrap().opacity = Opacity::new(0.5).unwrap()),
        ),
        (
            "fill rule",
            Box::new(|b| b.fill.as_mut().unwrap().rule = FillRule::EvenOdd),
        ),
        ("stroke presence", Box::new(|b| b.stroke = None)),
        (
            "stroke color",
            Box::new(|b| b.stroke.as_mut().unwrap().paint = Paint::Color(Color::new_rgb(1, 2, 3))),
        ),
        (
            "stroke opacity",
            Box::new(|b| b.stroke.as_mut().unwrap().opacity = Opacity::new(0.5).unwrap()),
        ),
        (
            "stroke width",
            Box::new(|b| b.stroke.as_mut().unwrap().width = NonZeroPositiveF32::new(2.0).unwrap()),
        ),
        (
            "dash presence",
            Box::new(|b| b.stroke.as_mut().unwrap().dasharray = Some(vec![2.0, 3.0].into())),
        ),
        (
            "dash offset",
            Box::new(|b| b.stroke.as_mut().unwrap().dashoffset = 1.0),
        ),
        (
            "miter",
            Box::new(|b| b.stroke.as_mut().unwrap().miterlimit = StrokeMiterlimit::new(5.0)),
        ),
        (
            "cap",
            Box::new(|b| b.stroke.as_mut().unwrap().linecap = LineCap::Round),
        ),
        (
            "join",
            Box::new(|b| b.stroke.as_mut().unwrap().linejoin = LineJoin::Round),
        ),
        (
            "computed color",
            Box::new(|b| b.computed_color = svgtypes::Color::new_rgb(1, 2, 3)),
        ),
        (
            "paint order",
            Box::new(|b| b.paint_order = PaintOrder::StrokeAndFill),
        ),
        ("visible", Box::new(|b| b.visible = false)),
        (
            "font size",
            Box::new(|b| b.font_size = NonZeroPositiveF32::new(13.0).unwrap()),
        ),
        (
            "families",
            Box::new(|b| b.font.families = TreeVec::plain(vec![FontFamily::Serif])),
        ),
        ("font style", Box::new(|b| b.font.style = FontStyle::Italic)),
        (
            "font stretch",
            Box::new(|b| b.font.stretch = FontStretch::Expanded),
        ),
        ("font weight", Box::new(|b| b.font.weight = 500)),
        (
            "font variation",
            Box::new(|b| {
                b.font.variations = TreeVec::plain(vec![FontVariation::new(*b"wght", 400.0)])
            }),
        ),
        (
            "dominant baseline",
            Box::new(|b| b.dominant_baseline = DominantBaseline::Central),
        ),
        (
            "alignment baseline",
            Box::new(|b| b.alignment_baseline = AlignmentBaseline::Middle),
        ),
        (
            "baseline shift",
            Box::new(|b| b.baseline_shift = TreeVec::plain(vec![BaselineShift::Subscript])),
        ),
    ];
    for (name, change) in &mutations {
        let mut b = copy_span(&a);
        change(&mut b);
        assert!(!original_visual(&a, &b), "effective mutation {name}");
        check(&a, &b);
    }
    let mut b = copy_span(&a);
    b.source_owner = 2;
    b.source_ancestors = TreeVec::plain(vec![1, 2]);
    b.start = 1;
    b.end = 2;
    b.language = TreeString::plain("zh".into());
    b.small_caps = true;
    b.apply_kerning = false;
    b.font_optical_sizing = FontOpticalSizing::None;
    b.letter_spacing = 1.0;
    b.word_spacing = 2.0;
    b.text_length = Some(10.0);
    b.length_adjust = LengthAdjust::SpacingAndGlyphs;
    assert!(original_visual(&a, &b));
    check(&a, &b);
    println!(
        "VISUAL_FIELDS effectiveMutations={} sourceFieldsRemainSeparate=true",
        mutations.len()
    );
}

#[test]
fn original_float_rules_preserve_nan_signed_zero_and_order() {
    let nan = f32::from_bits(0x7fc0_1234);
    let different_nan = f32::from_bits(0x7fc0_1235);
    let mut a = span();
    a.font.variations = TreeVec::plain(vec![
        FontVariation::new(*b"wght", nan),
        FontVariation::new(*b"wdth", -0.0),
    ]);
    let mut b = copy_span(&a);
    assert!(original_visual(&a, &b));
    check(&a, &b);
    b.font.variations[0].value = different_nan;
    assert!(!original_visual(&a, &b));
    check(&a, &b);
    b = copy_span(&a);
    b.font.variations[1].value = 0.0;
    assert!(!original_visual(&a, &b));
    check(&a, &b);
    b = copy_span(&a);
    b.font.variations.reverse();
    assert!(!original_visual(&a, &b));
    check(&a, &b);
    a = span();
    a.stroke = Some(stroke());
    a.stroke.as_mut().unwrap().dasharray = Some(vec![-0.0, 2.0].into());
    b = copy_span(&a);
    b.stroke.as_mut().unwrap().dasharray = Some(vec![0.0, 2.0].into());
    assert!(original_visual(&a, &b));
    check(&a, &b);
    a.stroke.as_mut().unwrap().dasharray = Some(vec![nan, 2.0].into());
    b = copy_span(&a);
    assert!(!original_visual(&a, &b));
    check(&a, &b);
    a = span();
    a.baseline_shift = TreeVec::plain(vec![BaselineShift::Number(-0.0)]);
    b = copy_span(&a);
    b.baseline_shift = TreeVec::plain(vec![BaselineShift::Number(0.0)]);
    assert!(original_visual(&a, &b));
    check(&a, &b);
    a.baseline_shift = TreeVec::plain(vec![BaselineShift::Number(nan)]);
    b = copy_span(&a);
    assert!(!original_visual(&a, &b));
    check(&a, &b);
    a = span();
    a.stroke = Some(stroke());
    a.stroke.as_mut().unwrap().dashoffset = nan;
    b = copy_span(&a);
    assert!(!original_visual(&a, &b));
    check(&a, &b);
}

#[test]
fn equal_resource_fields_do_not_replace_original_paint_identity_and_utf8_name_rules() {
    fn gradient() -> crate::SharedOwner<LinearGradient> {
        crate::SharedOwner::try_new(
            LinearGradient {
                base: BaseGradient {
                    id: crate::tree::NonEmptyString::new("same-id".into()).unwrap(),
                    units: crate::tree::Units::UserSpaceOnUse,
                    transform: Transform::identity(),
                    spread_method: SpreadMethod::Pad,
                    stops: TreeVec::plain(vec![
                        Stop {
                            offset: NormalizedF32::ZERO,
                            color: Color::black(),
                            opacity: Opacity::ONE,
                        },
                        Stop {
                            offset: NormalizedF32::ONE,
                            color: Color::new_rgb(255, 255, 255),
                            opacity: Opacity::ONE,
                        },
                    ]),
                },
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            },
            None,
            markdown_svg_budget::ResourceKind::Scratch,
        )
        .unwrap()
    }
    let mut a = span();
    a.fill.as_mut().unwrap().paint = Paint::LinearGradient(gradient());
    let mut b = copy_span(&a);
    assert!(original_visual(&a, &b));
    check(&a, &b);
    b.fill.as_mut().unwrap().paint = Paint::LinearGradient(gradient());
    assert!(!original_visual(&a, &b));
    check(&a, &b);
    a = span();
    a.font.families = TreeVec::plain(vec![FontFamily::Named(TreeString::plain(
        "\u{4e2d}".repeat(257),
    ))]);
    b = copy_span(&a);
    assert_eq!(check(&a, &b), vec![0, 1, 1, 256, 256, 256, 3, 0]);
    b.font.families = TreeVec::plain(vec![FontFamily::Named(TreeString::plain(format!(
        "{}\u{6587}",
        "\u{4e2d}".repeat(256)
    )))]);
    assert!(!original_visual(&a, &b));
    check(&a, &b);
}

#[derive(Clone, Copy, Debug)]
enum Long {
    Dash,
    Families,
    NamedBytes,
    Variations,
    Baseline,
}

fn long(kind: Long) -> TextSpan {
    let mut a = span();
    match kind {
        Long::Dash => {
            let mut s = stroke();
            s.dasharray = Some(vec![1.0; 600].into());
            a.stroke = Some(s);
        }
        Long::Families => {
            a.font.families = TreeVec::plain(
                std::iter::repeat_with(|| FontFamily::Serif)
                    .take(600)
                    .collect::<Vec<_>>(),
            )
        }
        Long::NamedBytes => {
            a.font.families =
                TreeVec::plain(vec![FontFamily::Named(TreeString::plain("a".repeat(769)))])
        }
        Long::Variations => {
            a.font.variations = TreeVec::plain(
                (0..600)
                    .map(|i| FontVariation::new((i as u32).to_be_bytes(), i as f32))
                    .collect::<Vec<_>>(),
            )
        }
        Long::Baseline => a.baseline_shift = TreeVec::plain(vec![BaselineShift::Number(1.0); 600]),
    }
    a
}

#[test]
fn all_real_generic_comparison_checkpoints_stop_without_visiting_the_suffix() {
    for kind in [
        Long::Dash,
        Long::Families,
        Long::NamedBytes,
        Long::Variations,
        Long::Baseline,
    ] {
        let a = long(kind);
        let b = copy_span(&a);
        let baseline = check(&a, &b);
        for at in 1..=baseline.len() {
            let mut trace = Trace {
                work: vec![],
                fail_at: Some(at),
            };
            assert_eq!(
                replay_visual_equivalent_with(&a, &b, &mut trace).err(),
                Some(native(-941))
            );
            assert_eq!(trace.work, baseline[..at], "{kind:?} ordinal={at}");
        }
        println!("VISUAL_CHECKPOINT {kind:?} count={}", baseline.len());
    }
}

#[test]
fn long_effective_tail_and_length_differences_match_original_fields() {
    for kind in [
        Long::Dash,
        Long::Families,
        Long::NamedBytes,
        Long::Variations,
        Long::Baseline,
    ] {
        let a = long(kind);
        let mut tail = copy_span(&a);
        let mut shorter = copy_span(&a);
        match kind {
            Long::Dash => {
                let mut values = a
                    .stroke
                    .as_ref()
                    .unwrap()
                    .dasharray
                    .as_ref()
                    .unwrap()
                    .to_vec();
                values[599] = 2.0;
                tail.stroke.as_mut().unwrap().dasharray = Some(values.into());
                shorter.stroke.as_mut().unwrap().dasharray = Some(vec![1.0; 599].into());
            }
            Long::Families => {
                tail.font.families[599] = FontFamily::SansSerif;
                shorter.font.families.pop();
            }
            Long::NamedBytes => {
                tail.font.families = TreeVec::plain(vec![FontFamily::Named(TreeString::plain(
                    format!("{}b", "a".repeat(768)),
                ))]);
                shorter.font.families =
                    TreeVec::plain(vec![FontFamily::Named(TreeString::plain("a".repeat(768)))]);
            }
            Long::Variations => {
                tail.font.variations[599].value = -1.0;
                shorter.font.variations.pop();
            }
            Long::Baseline => {
                tail.baseline_shift[599] = BaselineShift::Subscript;
                shorter.baseline_shift.pop();
            }
        }
        assert!(!original_visual(&a, &tail));
        check(&a, &tail);
        assert!(!original_visual(&a, &shorter));
        let trace = check(&a, &shorter);
        let expected = match kind {
            Long::Dash | Long::Families => vec![0, 1, 0],
            _ => vec![0, 1, 1, 0],
        };
        assert_eq!(trace, expected, "{kind:?} length mismatch skips contents");
    }
}

#[test]
fn actual_request_work_and_cancel_reject_second_256_item_or_byte_window() {
    for kind in [
        Long::Dash,
        Long::Families,
        Long::NamedBytes,
        Long::Variations,
        Long::Baseline,
    ] {
        let a = long(kind);
        let b = copy_span(&a);
        let baseline = check(&a, &b);
        let at = match kind {
            Long::Dash | Long::Families => 259,
            Long::NamedBytes => 5,
            Long::Variations | Long::Baseline => 260,
        };
        let prior_work: u64 = baseline[..at - 1].iter().sum();
        assert_eq!(
            baseline[at - 1],
            if matches!(kind, Long::NamedBytes) {
                256
            } else {
                1
            }
        );
        for quota in [false, true] {
            let mut limits = Limits::default();
            if quota {
                limits.work = prior_work;
            }
            let (r, state) = request(limits);
            let expected = if quota {
                Error::new(FailureKind::Quota, LimitKind::Work)
            } else {
                cancelled()
            };
            if !quota {
                state.arm(Call::Checkpoint, at, expected);
            }
            assert_eq!(
                try_replay_visual_equivalent(&a, &b, &r).err(),
                Some(expected)
            );
            assert_eq!(r.failure(), Some(expected));
            let count = state.counts().checkpoint;
            assert_eq!(count, at - usize::from(quota));
            assert_eq!(
                r.statistics().work,
                prior_work + if quota { 0 } else { baseline[at - 1] }
            );
            assert_eq!(
                try_replay_visual_equivalent(&span(), &span(), &r).err(),
                Some(expected)
            );
            assert_eq!(state.counts().checkpoint, count);
            state.assert_live(&r, 0);
            println!(
                "VISUAL_WINDOW {kind:?} work={quota} checkpoint={at} prefixWork={prior_work} completedPrefix=256"
            );
        }
    }
}

#[test]
fn generic_sequence_reports_the_exact_already_compared_prefix_and_lengths_skip_work() {
    let a: Vec<u32> = (0..600).collect();
    let mut b = a.clone();
    b[599] = 999;
    let mut control = Trace::default();
    let mut compared = 0;
    assert!(!visual_sequence_equal(&a, &b, &mut control, |a, b, _| {
        compared += 1;
        Ok(a == b)
    })
    .unwrap());
    assert_eq!(compared, 600);
    let mut control = Trace {
        work: vec![],
        fail_at: Some(257),
    };
    let mut compared = 0;
    assert_eq!(
        visual_sequence_equal(&a, &b, &mut control, |a, b, _| {
            compared += 1;
            Ok(a == b)
        })
        .err(),
        Some(native(-941))
    );
    assert_eq!(compared, 256);
    assert_eq!(control.work, vec![1; 257]);
    let mut control = Trace {
        work: vec![],
        fail_at: Some(1),
    };
    let mut compared = 0;
    assert!(
        !visual_sequence_equal(&a, &b[..599], &mut control, |a, b, _| {
            compared += 1;
            Ok(a == b)
        })
        .unwrap()
    );
    assert_eq!(compared, 0);
    assert!(control.work.is_empty());
    let left = format!("{}x", "a".repeat(768));
    let right = format!("{}y", "a".repeat(768));
    let mut control = Trace::default();
    assert!(!visual_name_equal(&left, &right, &mut control).unwrap());
    assert_eq!(control.work, vec![256, 256, 256, 1]);
    let mut control = Trace {
        work: vec![],
        fail_at: Some(2),
    };
    assert_eq!(
        visual_name_equal(&left, &right, &mut control).err(),
        Some(native(-941))
    );
    assert_eq!(control.work, vec![256, 256]);
    assert_eq!(&left.as_bytes()[..256], &right.as_bytes()[..256]);
    println!("VISUAL_PREFIX actualSequenceComparisons=256 nextItem=257 nameCompletedBytes=256");
}

#[test]
fn fill_false_still_computes_stroke_and_later_tail_order_stays_original() {
    let a = long(Long::Dash);
    let mut b = copy_span(&a);
    b.fill.as_mut().unwrap().paint = Paint::Color(Color::new_rgb(1, 2, 3));
    let baseline = check(&a, &b);
    assert_eq!(baseline, [vec![0, 1], vec![1; 600], vec![0]].concat());
    let mut control = Trace {
        work: vec![],
        fail_at: Some(259),
    };
    assert_eq!(
        replay_visual_equivalent_with(&a, &b, &mut control).err(),
        Some(native(-941))
    );
    let (r, state) = request(Limits::default());
    state.arm(Call::Checkpoint, 259, cancelled());
    assert_eq!(
        try_replay_visual_equivalent(&a, &b, &r).err(),
        Some(cancelled())
    );
    assert_eq!(state.counts().checkpoint, 259);
    for tail in 0..4 {
        let a = long(Long::Families);
        let mut b = copy_span(&a);
        match tail {
            0 => b.computed_color = svgtypes::Color::new_rgb(1, 2, 3),
            1 => b.paint_order = PaintOrder::StrokeAndFill,
            2 => b.visible = false,
            _ => b.font_size = NonZeroPositiveF32::new(13.0).unwrap(),
        }
        assert_eq!(check(&a, &b), vec![0, 1, 0]);
    }
    let mut a = long(Long::Variations);
    a.font.families = TreeVec::plain(vec![FontFamily::Named(TreeString::plain("a".repeat(512)))]);
    let mut b = copy_span(&a);
    b.font.style = FontStyle::Italic;
    assert_eq!(check(&a, &b), vec![0, 1, 1, 256, 256, 0]);
    let a = long(Long::Baseline);
    let mut b = copy_span(&a);
    b.alignment_baseline = AlignmentBaseline::Middle;
    assert_eq!(check(&a, &b), vec![0, 1, 1, 0]);
}
