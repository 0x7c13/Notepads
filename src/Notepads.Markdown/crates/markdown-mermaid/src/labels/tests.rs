// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::canonical::{OutlineProvider, RunMetrics};
use markdown_memory::AllocationAuthority;
use std::{
    alloc::Layout,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use unicode_segmentation::UnicodeSegmentation;

struct ScanAuthority {
    live: AtomicUsize,
    work: AtomicUsize,
    limit: usize,
    first: std::sync::Mutex<Option<MemoryError>>,
    cancel: OperationControl,
    cancel_at: usize,
}
impl AllocationAuthority for ScanAuthority {
    fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
        self.live.fetch_add(layout.size(), Ordering::SeqCst);
        Ok(())
    }
    fn release(&self, layout: Layout, _: Family) {
        self.live.fetch_sub(layout.size(), Ordering::SeqCst);
    }
    fn failure(&self) -> Option<MemoryError> {
        *self.first.lock().unwrap()
    }
    fn record_failure(&self, error: MemoryError) {
        self.first.lock().unwrap().get_or_insert(error);
    }
    fn charge_work(&self, units: usize) -> Result<(), MemoryError> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        let next = self.work.load(Ordering::SeqCst) + units;
        if next > self.limit {
            self.record_failure(MemoryError::WorkLimited);
            return Err(MemoryError::WorkLimited);
        }
        self.work.store(next, Ordering::SeqCst);
        if next >= self.cancel_at {
            self.cancel.cancel();
        }
        Ok(())
    }
}
fn scan_setup(
    limit: usize,
    cancel_at: usize,
) -> (Arc<ScanAuthority>, MemoryScope, OperationControl) {
    let cancel = OperationControl::new();
    let authority = Arc::new(ScanAuthority {
        live: AtomicUsize::new(0),
        work: AtomicUsize::new(0),
        limit,
        first: std::sync::Mutex::new(None),
        cancel: cancel.clone(),
        cancel_at,
    });
    let scope = MemoryScope::new(authority.clone());
    (authority, scope.clone(), cancel.with_memory_scope(scope))
}

fn assert_scan_work_limited(raw: &str) {
    let (authority, scope, control) = scan_setup(100_000, usize::MAX);
    // Stop at scanning: wrapping has its own substantial, correctly metered
    // work and must not make the old scanner pass this regression by accident.
    let result = super::scan::parse(raw, true, FontPlan::default(), &scope, &control);
    assert!(matches!(
        result,
        Err(CanonicalError::Memory(MemoryError::WorkLimited))
    ));
    assert!(authority.work.load(Ordering::SeqCst) <= 100_000);
    assert_eq!(authority.failure(), Some(MemoryError::WorkLimited));
    assert_eq!(authority.live.load(Ordering::SeqCst), 0);
}

#[test]
fn unmatched_angle_suffix_searches_pay_before_their_scans() {
    assert_scan_work_limited(&"<".repeat(8192));
}

#[test]
fn unmatched_hash_suffix_searches_pay_before_their_scans() {
    assert_scan_work_limited(&"#".repeat(8192));
}

#[test]
fn long_candidate_bodies_are_admitted_before_classification() {
    assert_scan_work_limited(&format!("{}>", "<".repeat(8192)));
}

#[test]
fn funded_suffix_windows_keep_literal_and_entity_semantics() {
    for (raw, expected) in [
        (
            format!("{}>", "<".repeat(64)),
            format!("{}>", "<".repeat(64)),
        ),
        (
            format!("#{};", "a".repeat(64)),
            format!("#{};", "a".repeat(64)),
        ),
        (
            format!("#{};", "a".repeat(65)),
            format!("#{};", "a".repeat(65)),
        ),
        (
            format!("#{};", "中".repeat(22)),
            format!("#{};", "中".repeat(22)),
        ),
        ("x #35; y".to_owned(), "x # y".to_owned()),
    ] {
        let (authority, scope, control) = scan_setup(1_000_000, usize::MAX);
        let value =
            compile_label(&raw, true, LabelPolicy::default(), &scope, &control, &Font).unwrap();
        assert_eq!(value.text(), expected);
        drop(value);
        assert_eq!(authority.live.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn suffix_scan_cancellation_is_windowed_and_existing_first_fault_wins() {
    let raw = "<".repeat(8192);
    // The old scanner completes all of this input below 100,000 charged
    // units. Only paying its suffix scans reaches this cancellation trigger.
    let (authority, scope, control) = scan_setup(1_000_000, 100_000);
    let result = super::scan::parse(&raw, false, FontPlan::default(), &scope, &control);
    assert!(matches!(result, Err(CanonicalError::Cancelled)));
    assert!(authority.work.load(Ordering::SeqCst) <= 100_000 + 256);
    assert_eq!(authority.live.load(Ordering::SeqCst), 0);

    let (authority, scope, control) = scan_setup(1_000_000, usize::MAX);
    scope.record_failure(MemoryError::BudgetLimited);
    let result = super::scan::parse(&raw, false, FontPlan::default(), &scope, &control);
    assert!(matches!(
        result,
        Err(CanonicalError::Memory(MemoryError::BudgetLimited))
    ));
    assert_eq!(authority.work.load(Ordering::SeqCst), 0);
    assert_eq!(authority.live.load(Ordering::SeqCst), 0);
}

struct Authority {
    used: AtomicUsize,
    limit: usize,
}
impl AllocationAuthority for Authority {
    fn reserve(&self, l: Layout, _: Family) -> Result<(), MemoryError> {
        self.used
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(l.size()).filter(|n| *n <= self.limit)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::BudgetLimited)
    }
    fn release(&self, l: Layout, _: Family) {
        self.used.fetch_sub(l.size(), Ordering::SeqCst);
    }
    fn failure(&self) -> Option<MemoryError> {
        None
    }
}
struct Font;
impl OutlineProvider for Font {
    fn measure(&self, r: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[r], TextDirection::Auto)
    }
    fn outline(&self, r: TextRun<'_>, s: &mut dyn PathSink) -> Result<(), CanonicalError> {
        self.outline_spans(&[r], TextDirection::Auto, s)
    }
}
impl RichTextProvider for Font {
    fn measure_spans(
        &self,
        spans: &[TextRun<'_>],
        _: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        // Deliberately non-additive shaping, across a format-run boundary too.
        let text = spans.iter().map(|s| s.text).collect::<String>();
        let width = text.graphemes(true).count() as f64 * 10.0
            - if text.contains("AV") { 5.0 } else { 0.0 };
        Ok(RunMetrics {
            advance: width,
            ascent: 8.0,
            descent: 2.0,
            x_height: 5.0,
            ink: if width == 0.0 {
                Rect::default()
            } else {
                Rect {
                    left: -1.0,
                    top: -8.0,
                    right: width + 1.0,
                    bottom: 2.0,
                }
            },
        })
    }
    fn outline_spans(
        &self,
        r: &[TextRun<'_>],
        d: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        let m = self.measure_spans(r, d)?;
        for c in [
            PathCommand::move_to(m.ink.left, m.ink.top),
            PathCommand::line_to(m.ink.right, m.ink.top),
            PathCommand::line_to(m.ink.right, m.ink.bottom),
            PathCommand::line_to(m.ink.left, m.ink.bottom),
            PathCommand::close(),
        ] {
            sink.push(c)?;
        }
        Ok(())
    }
}
fn setup(limit: usize) -> (Arc<Authority>, MemoryScope, OperationControl) {
    let authority = Arc::new(Authority {
        used: AtomicUsize::new(0),
        limit,
    });
    let scope = MemoryScope::new(authority.clone());
    let control = OperationControl::new().with_memory_scope(scope.clone());
    (authority, scope, control)
}
fn label(
    raw: &str,
    md: bool,
    policy: LabelPolicy,
    scope: &MemoryScope,
    control: &OperationControl,
) -> CompiledLabel {
    compile_label(raw, md, policy, scope, control, &Font).unwrap()
}

#[test]
fn literal_backslashes_and_plain_markers_survive() {
    let (a, s, c) = setup(8 * 1024 * 1024);
    let v = label(
        r"C:\Temp\nabla\n **text** `tick`",
        false,
        LabelPolicy {
            wrap_width: 1000.0,
            ..Default::default()
        },
        &s,
        &c,
    );
    assert_eq!(v.text(), r"C:\Temp\nabla\n **text** `tick`");
    assert_eq!(v.line_count(), 1);
    drop(v);
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}

#[test]
fn markup_precedes_entity_decode_and_real_breaks_remain_distinct() {
    let (_, s, c) = setup(8 * 1024 * 1024);
    let v = label(
        "a<br>b\r\nc &lt;br&gt; &ast;x&ast; #quot; &#10; d",
        true,
        LabelPolicy {
            wrap_width: 1000.0,
            ..Default::default()
        },
        &s,
        &c,
    );
    assert_eq!(v.line_count(), 3);
    assert_eq!(v.text(), "a\nb\nc <br> *x* \" d");
    assert!(
        v.spans
            .iter()
            .all(|v| v.font.weight == 400 && !v.font.italic)
    );
}

#[test]
fn delimiter_flanking_nested_styles_and_unpaired_text_are_preserved() {
    let (_, s, c) = setup(8 * 1024 * 1024);
    let v = label(
        "***both*** a_b_c *tail",
        true,
        LabelPolicy {
            wrap_width: 1000.0,
            ..Default::default()
        },
        &s,
        &c,
    );
    assert_eq!(v.text(), "both a_b_c *tail");
    assert!(
        v.spans
            .iter()
            .any(|p| &v.text()[p.range.clone()] == "both" && p.font.weight == 700 && p.font.italic)
    );
}

#[test]
fn whole_rich_line_is_measured_and_outlined_without_adding_run_widths() {
    let (_, s, c) = setup(8 * 1024 * 1024);
    let v = label("**A**V", true, LabelPolicy::default(), &s, &c);
    assert_eq!(v.width, 17.0, "15 advance plus actual left/right ink");
    struct Sink(Vec<PathCommand>);
    impl PathSink for Sink {
        fn push(&mut self, c: PathCommand) -> Result<(), CanonicalError> {
            self.0.push(c);
            Ok(())
        }
    }
    let mut sink = Sink(Vec::new());
    v.outline_at(&Font, 10.0, 20.0, &mut sink).unwrap();
    assert_eq!((sink.0[0].x1, sink.0[0].y1), (10.0, 20.0));
    assert_eq!((sink.0[1].x1, sink.0[1].y1), (27.0, 20.0));
}

#[test]
fn unicode_wrap_keeps_graphemes_and_preserves_single_oversized_grapheme() {
    let (_, s, c) = setup(8 * 1024 * 1024);
    let v = label(
        "e\u{301}中\u{301}文",
        false,
        LabelPolicy {
            wrap_width: 12.0,
            ..Default::default()
        },
        &s,
        &c,
    );
    assert_eq!(v.line_count(), 3);
    assert_eq!(v.width, 12.0);
    let wide = label(
        "e\u{301}",
        false,
        LabelPolicy {
            wrap_width: 3.0,
            ..Default::default()
        },
        &s,
        &c,
    );
    assert_eq!(wide.line_count(), 1);
    assert_eq!(wide.width, 12.0);
}

#[test]
fn unsupported_complete_structures_do_not_turn_isolated_symbols_into_errors() {
    let (_, s, c) = setup(8 * 1024 * 1024);
    for raw in [
        "[x](https://example.com)",
        "![x](a.png)",
        "`code`",
        "$$x$$",
        "<b>x</b>",
        r"[x\]](https://example.com/(a))",
    ] {
        assert_eq!(
            compile_label(raw, true, LabelPolicy::default(), &s, &c, &Font).err(),
            Some(CanonicalError::UnsupportedElement),
            "{raw}"
        );
    }
    assert_eq!(
        label("[ $ < > a < b > c", true, LabelPolicy::default(), &s, &c).text(),
        "[ $ < > a < b > c"
    );
}

#[test]
fn empty_and_trailing_hard_lines_have_real_line_metrics() {
    let (_, s, c) = setup(8 * 1024 * 1024);
    let empty = label("", false, LabelPolicy::default(), &s, &c);
    assert_eq!(empty.height, 10.0);
    let lines = label("a\n\n", false, LabelPolicy::default(), &s, &c);
    assert_eq!(lines.line_count(), 3);
    assert_eq!(lines.height, 30.0);
}

#[test]
fn funding_and_cancel_failure_leave_no_label_storage_alive() {
    let (a, s, c) = setup(0);
    assert_eq!(
        compile_label("abc", true, LabelPolicy::default(), &s, &c, &Font).err(),
        Some(CanonicalError::Memory(MemoryError::BudgetLimited))
    );
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
    c.cancel();
    assert_eq!(
        compile_label("abc", true, LabelPolicy::default(), &s, &c, &Font).err(),
        Some(CanonicalError::Cancelled)
    );
}

#[test]
fn unicode_first_strong_ignores_isolate_contents() {
    use crate::rich_text::resolve_direction;
    let p = FontPlan::default();
    assert_eq!(
        resolve_direction(
            &[TextRun {
                text: "123 שלום (42)",
                font: p
            }],
            TextDirection::Auto
        ),
        TextDirection::Rtl
    );
    assert_eq!(
        resolve_direction(
            &[TextRun {
                text: "\u{2067}مرحبا\u{2069} A",
                font: p
            }],
            TextDirection::Auto
        ),
        TextDirection::Ltr
    );
}

fn line_texts(label: &CompiledLabel) -> Vec<&str> {
    label
        .lines
        .iter()
        .map(|line| &label.text()[line.range.clone()])
        .collect()
}

#[test]
fn overflowing_separator_keeps_the_last_fitting_word_on_the_line() {
    let (authority, scope, control) = setup(8 * 1024 * 1024);
    for (raw, width, expected) in [
        ("abcdefgh ijklmnop q", 172.0, vec!["abcdefgh ijklmnop", "q"]),
        ("abcdefgh ijklmnop", 172.0, vec!["abcdefgh ijklmnop"]),
        ("  ab   cd ", 32.0, vec![" ab", "cd"]),
        ("   ", 3.0, vec![""]),
        ("abcdef", 22.0, vec!["ab", "cd", "ef"]),
        (
            "e\u{301}中\u{301} 文",
            22.0,
            vec!["e\u{301}中\u{301}", "文"],
        ),
        ("e\u{301}", 3.0, vec!["e\u{301}"]),
    ] {
        let actual = label(
            raw,
            false,
            LabelPolicy {
                wrap_width: width,
                ..Default::default()
            },
            &scope,
            &control,
        );
        assert_eq!(line_texts(&actual), expected, "{raw:?}");
        if raw == "abcdefgh ijklmnop q" {
            assert_eq!(actual.lines[0].range, 0..17);
            assert_eq!(actual.lines[1].range, 18..19);
        }
        drop(actual);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[derive(Debug)]
struct DirectionCall {
    text: String,
    direction: TextDirection,
}

#[derive(Default)]
struct DirectionFont {
    measured: std::cell::RefCell<Vec<DirectionCall>>,
    outlined: std::cell::RefCell<Vec<DirectionCall>>,
}

impl OutlineProvider for DirectionFont {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[run], TextDirection::Auto)
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        self.outline_spans(&[run], TextDirection::Auto, sink)
    }
}

impl RichTextProvider for DirectionFont {
    fn measure_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        self.measured.borrow_mut().push(DirectionCall {
            text: spans.iter().map(|span| span.text).collect(),
            direction,
        });
        Font.measure_spans(spans, direction)
    }
    fn outline_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        self.outlined.borrow_mut().push(DirectionCall {
            text: spans.iter().map(|span| span.text).collect(),
            direction,
        });
        Font.outline_spans(spans, direction, sink)
    }
}

struct IgnorePaths;
impl PathSink for IgnorePaths {
    fn push(&mut self, _: PathCommand) -> Result<(), CanonicalError> {
        Ok(())
    }
}

#[test]
fn emitted_lines_reuse_only_exact_measurements() {
    let (_, scope, control) = setup(8 * 1024 * 1024);
    for (text, wrap_width, calls) in [
        ("abcd", 22.0, &["a", "ab", "abc", "c", "cd"][..]),
        (
            "ab cd",
            32.0,
            &["a", "ab", "ab ", "ab c", "ab", "c", "cd"][..],
        ),
        ("中", 1.0, &["中"][..]),
    ] {
        let font = DirectionFont::default();
        let label = compile_label(
            text,
            false,
            LabelPolicy {
                wrap_width,
                ..Default::default()
            },
            &scope,
            &control,
            &font,
        )
        .unwrap();
        let measured = font.measured.borrow();
        assert!(
            measured
                .iter()
                .map(|call| call.text.as_str())
                .eq(calls.iter().copied()),
            "{text}: {measured:?}"
        );
        assert_eq!(label.width, if text == "中" { 12.0 } else { 22.0 });
    }
}

#[test]
fn soft_lines_keep_the_explicit_paragraph_direction_for_every_probe_and_outline() {
    let (authority, scope, control) = setup(8 * 1024 * 1024);
    for (raw, expected_direction) in [
        ("שלום abcdefgh (12)", TextDirection::Rtl),
        ("**123** *שלום* abcdefgh (12)", TextDirection::Rtl),
        ("\u{2067}مرحبا\u{2069} **A** bcdef (12)", TextDirection::Ltr),
    ] {
        for requested in [TextDirection::Auto, TextDirection::Ltr, TextDirection::Rtl] {
            let font = DirectionFont::default();
            let actual = compile_label(
                raw,
                true,
                LabelPolicy {
                    wrap_width: 52.0,
                    direction: requested,
                    ..Default::default()
                },
                &scope,
                &control,
                &font,
            )
            .unwrap();
            let expected = if requested == TextDirection::Auto {
                expected_direction
            } else {
                requested
            };
            assert!(actual.line_count() > 1);
            assert!(actual.lines.iter().all(|line| line.direction == expected));
            actual
                .outline_at(&font, 0.0, 0.0, &mut IgnorePaths)
                .unwrap();
            let measured = font.measured.borrow();
            let outlined = font.outlined.borrow();
            assert!(measured.len() > actual.line_count());
            assert!(measured.iter().all(|call| call.direction == expected));
            assert_eq!(outlined.len(), actual.line_count());
            assert!(outlined.iter().all(|call| call.direction == expected));
            assert_eq!(
                outlined
                    .iter()
                    .map(|call| call.text.as_str())
                    .collect::<Vec<_>>(),
                line_texts(&actual)
            );
            drop(actual);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn normalized_explicit_breaks_resolve_new_paragraphs_and_empty_lines() {
    let (authority, scope, control) = setup(8 * 1024 * 1024);
    for separator in ["\n", "\r\n", "<br>"] {
        let font = DirectionFont::default();
        let raw = format!("שלום abcdefgh (12){separator}XYZ -34{separator}");
        let actual = compile_label(
            &raw,
            false,
            LabelPolicy {
                wrap_width: 52.0,
                ..Default::default()
            },
            &scope,
            &control,
            &font,
        )
        .unwrap();
        actual
            .outline_at(&font, 0.0, 0.0, &mut IgnorePaths)
            .unwrap();
        for calls in [&font.measured, &font.outlined] {
            let calls = calls.borrow();
            let boundary = calls
                .iter()
                .position(|call| call.text.starts_with('X'))
                .unwrap();
            assert!(boundary > 0);
            assert!(
                calls[..boundary]
                    .iter()
                    .all(|call| call.direction == TextDirection::Rtl)
            );
            assert!(
                calls[boundary..]
                    .iter()
                    .all(|call| call.direction == TextDirection::Ltr)
            );
            assert!(calls.last().unwrap().text.is_empty());
        }
        let boundary = actual.text().find('\n').unwrap();
        for line in &actual.lines {
            assert_eq!(
                line.direction,
                if line.range.start < boundary {
                    TextDirection::Rtl
                } else {
                    TextDirection::Ltr
                }
            );
        }
        drop(actual);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn resolved_line_storage_keeps_the_existing_limit_and_releases_on_failure() {
    let (authority, scope, control) = setup(8 * 1024 * 1024);
    let at_limit = label(
        "\n".repeat(1023).as_str(),
        false,
        LabelPolicy::default(),
        &scope,
        &control,
    );
    assert_eq!(at_limit.line_count(), 1024);
    drop(at_limit);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    assert_eq!(
        compile_label(
            &"\n".repeat(1024),
            false,
            LabelPolicy::default(),
            &scope,
            &control,
            &Font
        )
        .err(),
        Some(CanonicalError::Limited)
    );
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}
