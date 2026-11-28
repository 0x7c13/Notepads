// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Self-authored pie cases and a deterministic synthetic outline provider.
//! These test geometry and admission, not native font shaping or device UI.

use super::*;
use crate::{
    canonical::{
        ArtifactFacts, CanonicalError, CanonicalLimits, OutlineProvider, RunMetrics, TextRun,
        accept_artifact,
    },
    rich_text::TextDirection,
    scene::shapes,
};
use markdown_memory::{AllocationAuthority, NativeRegion};
use std::{
    alloc::Layout,
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct Authority {
    used: AtomicUsize,
    allocations: AtomicUsize,
    fail_at: AtomicUsize,
    failure: AtomicUsize,
    work: AtomicUsize,
    work_limit: AtomicUsize,
}

impl AllocationAuthority for Authority {
    fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
        self.used
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                used.checked_add(layout.size())
                    .filter(|value| *value <= 64 * 1024 * 1024)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::BudgetLimited)
    }
    fn release(&self, layout: Layout, _: Family) {
        self.used.fetch_sub(layout.size(), Ordering::SeqCst);
    }
    fn failure(&self) -> Option<MemoryError> {
        match self.failure.load(Ordering::SeqCst) {
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
            .failure
            .compare_exchange(0, code, Ordering::SeqCst, Ordering::SeqCst);
    }
    fn before_allocate(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
        if self.allocations.fetch_add(1, Ordering::SeqCst) + 1
            == self.fail_at.load(Ordering::SeqCst)
        {
            Err(MemoryError::OutOfMemory)
        } else {
            Ok(())
        }
    }
    fn charge_work(&self, units: usize) -> Result<(), MemoryError> {
        self.work
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |work| {
                work.checked_add(units)
                    .filter(|value| *value <= self.work_limit.load(Ordering::SeqCst))
            })
            .map(|_| ())
            .map_err(|_| MemoryError::WorkLimited)
    }
}

struct Font<'a> {
    cancel_on_outline: Option<&'a OperationControl>,
}

struct CaptureFont {
    outlined: RefCell<FundedString>,
}
impl OutlineProvider for CaptureFont {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[run], TextDirection::Auto)
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        self.outline_spans(&[run], TextDirection::Auto, sink)
    }
}
impl RichTextProvider for CaptureFont {
    fn measure_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        Font {
            cancel_on_outline: None,
        }
        .measure_spans(runs, direction)
    }
    fn outline_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        let mut outlined = self.outlined.borrow_mut();
        for run in runs {
            outlined.try_push_str(run.text)?;
        }
        outlined.try_push('\n')?;
        drop(outlined);
        Font {
            cancel_on_outline: None,
        }
        .outline_spans(runs, direction, sink)
    }
}
impl OutlineProvider for Font<'_> {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[run], TextDirection::Auto)
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        self.outline_spans(&[run], TextDirection::Auto, sink)
    }
}
impl RichTextProvider for Font<'_> {
    fn measure_spans(
        &self,
        runs: &[TextRun<'_>],
        _: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        let width = runs
            .iter()
            .map(|run| run.text.chars().count() as f64 * run.font.em_size / 2.0)
            .sum();
        Ok(RunMetrics {
            advance: width,
            ascent: 12.0,
            descent: 4.0,
            x_height: 8.0,
            ink: Rect {
                left: 0.0,
                top: -12.0,
                right: width,
                bottom: 4.0,
            },
        })
    }
    fn outline_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        if let Some(control) = self.cancel_on_outline {
            control.cancel();
        }
        let metrics = self.measure_spans(runs, direction)?;
        if metrics.advance > 0.0 {
            shapes::outline(Shape::Rectangle, metrics.ink, sink)?;
        }
        Ok(())
    }
}

fn setup() -> (Arc<Authority>, MemoryScope, OperationControl) {
    let authority = Arc::new(Authority {
        used: AtomicUsize::new(0),
        allocations: AtomicUsize::new(0),
        fail_at: AtomicUsize::new(usize::MAX),
        failure: AtomicUsize::new(0),
        work: AtomicUsize::new(0),
        work_limit: AtomicUsize::new(1_000_000),
    });
    let funded = MemoryScope::new(authority.clone());
    let region = NativeRegion::try_new(&funded, 8192).unwrap();
    let control = OperationControl::new().with_memory_region(region);
    (authority, funded, control)
}

fn render(source: &str, control: &OperationControl) -> Result<OwnedScene, DiagramError> {
    render_pie_native_controlled(
        source,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font {
            cancel_on_outline: None,
        },
        control,
    )
}

fn released(authority: &Authority, funded: MemoryScope, control: OperationControl) {
    drop(control);
    drop(funded);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

fn accepted(scene: OwnedScene, funded: &MemoryScope, control: &OperationControl) {
    assert!(scene.bounds.left.is_finite() && scene.bounds.top.is_finite());
    assert!(scene.bounds.right > scene.bounds.left && scene.bounds.bottom > scene.bounds.top);
    assert!(scene.paths > 0 && scene.commands > 0 && scene.commands <= 65_536);
    let facts = ArtifactFacts {
        bounds: scene.bounds,
        paths: scene.paths,
        commands: scene.commands,
    };
    let document = accept_artifact(
        scene.svg,
        facts,
        funded,
        control,
        CanonicalLimits::default(),
    )
    .unwrap();
    drop(document);
}

#[test]
fn leading_bom_preserves_the_direct_pie_scene() {
    let (authority, funded, control) = setup();
    let source = "pie\n\"Text\": 2\n\"Images\": 1";
    let plain = render(source, &control).unwrap();
    let prefixed = render(&format!("\u{feff}{source}"), &control).unwrap();
    assert_eq!(prefixed.svg.as_str(), plain.svg.as_str());
    assert_eq!(prefixed.paths, plain.paths);
    assert_eq!(prefixed.commands, plain.commands);
    accepted(plain, &funded, &control);
    accepted(prefixed, &funded, &control);
    released(&authority, funded, control);
}

#[test]
fn lab_title_show_data_unicode_percentages_and_canonical_output() {
    let (authority, funded, control) = setup();
    let source = "pie showData title 实验室 Lab\naccTitle: 可访问图表\naccDescr: 文本、图片与公式的比例\n\"文本\": 62\n\"图片\": 25\n\"公式\": 13\n";
    let pie = Pie::parse(source, &funded, &control).unwrap();
    assert!(pie.show_data);
    assert_eq!(pie.title.as_ref().unwrap().as_str(), "实验室 Lab");
    assert_eq!(pie.entries.len(), 3);
    let fractions = pie.fractions(&funded, &control).unwrap();
    for (actual, expected) in fractions.iter().zip([0.62f64, 0.25, 0.13]) {
        assert!((*actual - expected).abs() < 1.0e-14);
    }
    let provider = CaptureFont {
        outlined: RefCell::new(FundedString::new_in(funded.clone(), Family::Text)),
    };
    let scene = render_pie_native_controlled(
        source,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &provider,
        &control,
    )
    .unwrap();
    assert!(provider.outlined.borrow().contains("实验室 Lab"));
    assert!(provider.outlined.borrow().contains("文本 — 62.0% [62]"));
    assert!(provider.outlined.borrow().contains("图片 — 25.0% [25]"));
    assert!(provider.outlined.borrow().contains("公式 — 13.0% [13]"));
    assert!(!provider.outlined.borrow().contains("可访问图表"));
    // Paths contain typed outlines, never authored text, text tags or scripts.
    assert!(!scene.svg.contains("<text") && !scene.svg.contains("文本"));
    assert!(scene.svg.contains('C'));
    accepted(scene, &funded, &control);
    drop(provider);
    drop(fractions);
    drop(pie);
    released(&authority, funded, control);
}

#[test]
fn vendor_quotes_escapes_comments_and_first_duplicate_win() {
    let (authority, funded, control) = setup();
    let pie = Pie::parse("%% heading\npie showData\n  title First\ntitle  Final   title\n\"A\\\"\\n\\\\%%\": 2.50 %% comment\n'A\\\"n\\\\%%': 9\n\"漢字🙂\": 7.5\n", &funded, &control).unwrap();
    assert_eq!(pie.title.as_ref().unwrap().as_str(), "Final title");
    assert_eq!(pie.entries[0].name.as_str(), "A\"n\\%%");
    assert_eq!(pie.entries[0].raw_value, "2.50");
    assert_eq!(pie.entries[0].value, 2.5);
    assert_eq!(pie.entries.len(), 2);
    drop(pie);
    assert_eq!(
        Pie::parse("pie\n\"A\": 1\n\"A\": -2", &funded, &control).err(),
        Some(DiagramError::InvalidInput)
    );
    accepted(
        render("pie\n\"漢字🙂\": 7.5", &control).unwrap(),
        &funded,
        &control,
    );
    released(&authority, funded, control);
}

#[test]
fn empty_all_zero_and_full_circle_are_finite_and_admitted() {
    for source in [
        "pie",
        "pie title Empty\n",
        "pie\n\"A\": 0\n\"B\": -0",
        "pie\n\"A\": 100",
        "pie\n\"Zero\": 0\n\"Only\": 1\n\"Also zero\": 0",
    ] {
        let (authority, funded, control) = setup();
        let scene = render(source, &control).unwrap();
        assert!(!scene.svg.contains("NaN") && !scene.svg.contains("inf"));
        if source.contains("100") {
            assert_eq!(scene.svg.matches('C').count(), 4);
        }
        accepted(scene, &funded, &control);
        released(&authority, funded, control);
    }
}

#[test]
fn normalization_does_not_overflow_or_erase_subnormal_peers() {
    let (authority, funded, control) = setup();
    let mut large = FundedString::new_in(funded.clone(), Family::Text);
    large.try_push_str("pie\n\"A\": ").unwrap();
    for _ in 0..308 {
        large.try_push('9').unwrap();
    }
    large.try_push_str("\n\"B\": ").unwrap();
    for _ in 0..308 {
        large.try_push('9').unwrap();
    }
    let pie = Pie::parse(&large, &funded, &control).unwrap();
    let fractions = pie.fractions(&funded, &control).unwrap();
    assert_eq!(fractions.as_slice(), &[0.5, 0.5]);
    drop(fractions);
    drop(pie);
    accepted(render(&large, &control).unwrap(), &funded, &control);
    let mut tiny = FundedString::new_in(funded.clone(), Family::Text);
    tiny.try_push_str("pie\n\"A\": 0.").unwrap();
    for _ in 0..323 {
        tiny.try_push('0').unwrap();
    }
    tiny.try_push_str("5\n\"B\": 0.").unwrap();
    for _ in 0..323 {
        tiny.try_push('0').unwrap();
    }
    tiny.try_push('5').unwrap();
    let pie = Pie::parse(&tiny, &funded, &control).unwrap();
    assert!(pie.entries[0].value > 0.0 && pie.entries[0].value.is_subnormal());
    let fractions = pie.fractions(&funded, &control).unwrap();
    assert_eq!(fractions.as_slice(), &[0.5, 0.5]);
    drop(fractions);
    drop(pie);
    accepted(render(&tiny, &control).unwrap(), &funded, &control);
    drop(large);
    drop(tiny);
    released(&authority, funded, control);
}

#[test]
fn invalid_numbers_syntax_and_control_characters_are_rejected() {
    for source in [
        "",
        "piep",
        "pie extra",
        "pie\n\"A\": -1",
        "pie\n\"A\": NaN",
        "pie\n\"A\": inf",
        "pie\n\"A\": Infinity",
        "pie\n\"A\": 1e3",
        "pie\n\"A\": +2",
        "pie\n\"A\": .5",
        "pie\n\"A\": 1.",
        "pie\n\"A\": 01",
        "pie\n\"A\": 1 trailing",
        "pie\n\"A\" 1",
        "pie\n\"A\": 1\nunknown",
        "pie\n\"A\u{0}\": 1",
        "pie\n\"A\u{7f}\": 1",
        "pie\n\"unclosed: 1",
        "pie\naccTitle missing-colon",
    ] {
        let (authority, funded, control) = setup();
        assert_eq!(
            render(source, &control).err(),
            Some(DiagramError::InvalidInput),
            "{source:?}"
        );
        released(&authority, funded, control);
    }
    let (authority, funded, control) = setup();
    let mut overflow = FundedString::new_in(funded.clone(), Family::Text);
    overflow.try_push_str("pie\n\"A\": ").unwrap();
    for _ in 0..400 {
        overflow.try_push('9').unwrap();
    }
    assert_eq!(
        render(&overflow, &control).err(),
        Some(DiagramError::InvalidInput)
    );
    drop(overflow);
    released(&authority, funded, control);
}

#[test]
fn unsupported_features_fail_explicitly_and_metadata_is_nonvisual() {
    for source in [
        "%%{init: {}}%%\npie\n\"A\": 1",
        "pie\naccDescr { block }",
        "pie\naccDescr\n{ block }",
        "pie\nstyle A fill:red",
        "pie\nclassDef red fill:red",
    ] {
        let (authority, funded, control) = setup();
        assert_eq!(
            render(source, &control).err(),
            Some(DiagramError::Unsupported)
        );
        released(&authority, funded, control);
    }
    let (authority, funded, control) = setup();
    let plain = render("pie\n\"A\": 1", &control).unwrap();
    let metadata = render(
        "pie\naccTitle: Hidden title\naccDescr: Hidden description\n\"A\": 1",
        &control,
    )
    .unwrap();
    assert_eq!(plain.svg, metadata.svg);
    drop(plain);
    drop(metadata);
    released(&authority, funded, control);
}

#[test]
fn row_source_and_work_limits_leave_no_storage() {
    for rows in [128, 129] {
        let (authority, funded, control) = setup();
        let mut source = FundedString::new_in(funded.clone(), Family::Text);
        source.try_push_str("pie\n").unwrap();
        for _ in 0..rows {
            source.try_push_str("\"Duplicate\": 1\n").unwrap();
        }
        let parsed = Pie::parse(&source, &funded, &control);
        if rows == 128 {
            assert_eq!(parsed.unwrap().entries.len(), 1);
        } else {
            assert_eq!(parsed.err(), Some(DiagramError::WorkLimited));
        }
        drop(source);
        released(&authority, funded, control);
    }
    let (authority, funded, control) = setup();
    let mut source = FundedString::new_in(funded.clone(), Family::Text);
    source.try_push_str("pie\n%%").unwrap();
    for _ in 0..SOURCE_LIMIT {
        source.try_push('a').unwrap();
    }
    assert_eq!(
        render(&source, &control).err(),
        Some(DiagramError::WorkLimited)
    );
    drop(source);
    released(&authority, funded, control);
    let (authority, funded, control) = setup();
    authority.work_limit.store(10, Ordering::SeqCst);
    assert_eq!(
        render("pie\n\"A\": 1", &control).err(),
        Some(DiagramError::Memory(MemoryError::WorkLimited))
    );
    released(&authority, funded, control);
}

#[test]
fn cancellation_deadline_and_unfunded_control_do_not_publish() {
    assert_eq!(
        render("pie", &OperationControl::new()).err(),
        Some(DiagramError::InvalidFunding)
    );
    let (authority, funded, control) = setup();
    control.cancel();
    assert_eq!(
        render("pie\n\"A\": 1", &control).err(),
        Some(DiagramError::Cancelled)
    );
    released(&authority, funded, control);
    let (authority, funded, control) = setup();
    let result = render_pie_native_controlled(
        "pie title Cancel late\n\"A\": 1",
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font {
            cancel_on_outline: Some(&control),
        },
        &control,
    );
    assert_eq!(result.err(), Some(DiagramError::Cancelled));
    released(&authority, funded, control);
    let (authority, funded, control) = setup();
    let control = control.with_deadline(Duration::ZERO);
    assert_eq!(render("pie", &control).err(), Some(DiagramError::Deadline));
    released(&authority, funded, control);
}

#[test]
fn each_injected_allocation_failure_returns_all_paid_storage() {
    let source = "pie showData title Lab\n\"文本\": 62\n\"图片\": 25\n\"公式\": 13";
    let (authority, funded, control) = setup();
    let baseline = authority.allocations.load(Ordering::SeqCst);
    drop(render(source, &control).unwrap());
    let count = authority.allocations.load(Ordering::SeqCst) - baseline;
    released(&authority, funded, control);
    for offset in 1..=count {
        let (authority, funded, control) = setup();
        authority.fail_at.store(
            authority.allocations.load(Ordering::SeqCst) + offset,
            Ordering::SeqCst,
        );
        assert_eq!(
            render(source, &control).err(),
            Some(DiagramError::Memory(MemoryError::OutOfMemory)),
            "allocation {offset}"
        );
        released(&authority, funded, control);
    }
}

#[test]
fn dark_and_high_contrast_themes_keep_themed_text_and_contrasting_slice_text() {
    for theme in [
        ThemePolicy {
            text: Color::WHITE,
            background: Color::BLACK,
            ..ThemePolicy::default()
        },
        ThemePolicy {
            text: Color::rgb(0xffff00),
            background: Color::BLACK,
            ..ThemePolicy::default()
        },
    ] {
        let (authority, funded, control) = setup();
        let scene = render_pie_native_controlled(
            "pie title Theme\n\"A\": 62\n\"B\": 25\n\"C\": 13",
            LayoutPolicy::default(),
            theme,
            &Font {
                cancel_on_outline: None,
            },
            &control,
        )
        .unwrap();
        let mut color = FundedString::new_in(funded.clone(), Family::Text);
        write!(color, "fill=\"#{:06x}\"", theme.text.rgb).unwrap();
        assert!(scene.svg.contains(color.as_str()));
        assert!(scene.svg.contains("fill=\"#000000\"") || scene.svg.contains("fill=\"#ffffff\""));
        accepted(scene, &funded, &control);
        drop(color);
        released(&authority, funded, control);
    }
}
