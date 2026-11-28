// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Authored fixtures and a synthetic font test semantics and ownership.
//! These tests do not qualify DirectWrite, emoji glyph appearance or UI input.

use super::*;
use crate::{
    canonical::{CanonicalError, OutlineProvider, RunMetrics, TextRun},
    rich_text::TextDirection,
};
use markdown_memory::{AllocationAuthority, MemoryError, NativeRegion};
use std::{
    alloc::Layout,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct Authority {
    used: AtomicUsize,
    allocations: AtomicUsize,
    fail: AtomicUsize,
    work: AtomicUsize,
    first: AtomicUsize,
}
impl AllocationAuthority for Authority {
    fn reserve(&self, l: Layout, _: Family) -> Result<(), MemoryError> {
        self.used
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(l.size()).filter(|n| *n <= 64 * 1024 * 1024)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::BudgetLimited)
    }
    fn release(&self, l: Layout, _: Family) {
        self.used.fetch_sub(l.size(), Ordering::SeqCst);
    }
    fn failure(&self) -> Option<MemoryError> {
        match self.first.load(Ordering::SeqCst) {
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
            .first
            .compare_exchange(0, code, Ordering::SeqCst, Ordering::SeqCst);
    }
    fn before_allocate(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
        let n = self.allocations.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.fail.load(Ordering::SeqCst) {
            Err(MemoryError::OutOfMemory)
        } else {
            Ok(())
        }
    }
    fn charge_work(&self, units: usize) -> Result<(), MemoryError> {
        self.work
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(units).filter(|n| *n <= 1_000_000)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::WorkLimited)
    }
}

struct Font;
impl OutlineProvider for Font {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[run], TextDirection::Auto)
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        self.outline_spans(&[run], TextDirection::Auto, sink)
    }
}
impl RichTextProvider for Font {
    fn measure_spans(
        &self,
        runs: &[TextRun<'_>],
        _: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        let width = runs
            .iter()
            .map(|r| r.text.chars().count() as f64 * r.font.em_size / 2.0)
            .sum::<f64>();
        Ok(RunMetrics {
            advance: width,
            ascent: 12.0,
            descent: 4.0,
            x_height: 8.0,
            ink: rect(0.0, -12.0, width, 16.0),
        })
    }
    fn outline_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        let metrics = self.measure_spans(runs, direction)?;
        if metrics.advance > 0.0 {
            crate::scene::shapes::outline(Shape::Rectangle, metrics.ink, sink)?;
        }
        Ok(())
    }
}

struct CancellingFont<'a>(&'a OperationControl);
impl OutlineProvider for CancellingFont<'_> {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[run], TextDirection::Auto)
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        Font.outline(run, sink)
    }
}
impl RichTextProvider for CancellingFont<'_> {
    fn measure_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        self.0.cancel();
        Font.measure_spans(runs, direction)
    }
    fn outline_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        Font.outline_spans(runs, direction, sink)
    }
}

struct ExpiringOutlineFont<'a> {
    control: &'a OperationControl,
    measurements: AtomicUsize,
    outlines: AtomicUsize,
}
impl OutlineProvider for ExpiringOutlineFont<'_> {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        self.measure_spans(&[run], TextDirection::Auto)
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        self.outline_spans(&[run], TextDirection::Auto, sink)
    }
}
impl RichTextProvider for ExpiringOutlineFont<'_> {
    fn measure_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
    ) -> Result<RunMetrics, CanonicalError> {
        self.measurements.fetch_add(1, Ordering::SeqCst);
        Font.measure_spans(runs, direction)
    }
    fn outline_spans(
        &self,
        runs: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError> {
        assert!(self.measurements.load(Ordering::SeqCst) > 0);
        assert!(self.control.checkpoint().is_ok());
        self.outlines.fetch_add(1, Ordering::SeqCst);
        // Emit real partial geometry before expiring the control. The label's
        // post-outline checkpoint must unwind that scene and all label owners.
        Font.outline_spans(runs, direction, sink)?;
        assert!(self.control.set_deadline(Duration::ZERO));
        Ok(())
    }
}

fn setup() -> (Arc<Authority>, MemoryScope, OperationControl) {
    let authority = Arc::new(Authority {
        used: AtomicUsize::new(0),
        allocations: AtomicUsize::new(0),
        fail: AtomicUsize::new(0),
        work: AtomicUsize::new(0),
        first: AtomicUsize::new(0),
    });
    let scope = MemoryScope::new(authority.clone());
    let region = NativeRegion::try_new(&scope, 8192).unwrap();
    let control = OperationControl::new().with_memory_region(region);
    (authority, scope, control)
}

fn render(source: &str, control: &OperationControl) -> Result<OwnedScene, DiagramError> {
    render_sequence_native_controlled(
        source,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &Font,
        control,
    )
}

fn accept(output: OwnedScene, scope: &MemoryScope, control: &OperationControl) {
    assert!(output.svg.as_str().starts_with("<svg "));
    assert!(!output.svg.contains("<text"));
    assert!(!output.svg.contains("<foreignObject"));
    assert!(output.commands > 0 && output.commands <= 65_536);
    let facts = crate::canonical::ArtifactFacts {
        bounds: output.bounds,
        paths: output.paths,
        commands: output.commands,
    };
    let document = crate::canonical::accept_artifact(
        output.svg,
        facts,
        scope,
        control,
        crate::canonical::CanonicalLimits::default(),
    )
    .unwrap();
    drop(document);
}

#[test]
fn leading_bom_preserves_the_direct_sequence_scene() {
    let (authority, scope, control) = setup();
    let source = "sequenceDiagram\nA->>B: hello";
    let plain = render(source, &control).unwrap();
    let prefixed = render(&format!("\u{feff}{source}"), &control).unwrap();
    assert_eq!(prefixed.svg.as_str(), plain.svg.as_str());
    assert_eq!(prefixed.paths, plain.paths);
    assert_eq!(prefixed.commands, plain.commands);
    accept(plain, &scope, &control);
    accept(prefixed, &scope, &control);
    drop((control, scope));
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn message_labels_preserve_literal_quotes_and_final_backslashes() {
    let (authority, scope, control) = setup();
    for (raw, expected) in [
        (r#""hello" and "goodbye""#, r#""hello" and "goodbye""#),
        (r#""hello" and goodbye"#, r#""hello" and goodbye"#),
        (r"C:\", r"C:\"),
        (r#""outer \"quote\"""#, r#"outer "quote""#),
    ] {
        assert_eq!(
            common::decode_label(raw, &scope).unwrap().as_str(),
            expected
        );
        let source = format!("sequenceDiagram\nA->>B: {raw}");
        accept(render(&source, &control).unwrap(), &scope, &control);
    }
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

const LAB: &str = "sequenceDiagram\nparticipant E as Editor\nparticipant P as Preview\nE->>P: Changed document\nNote over E,P: Same revision";

#[test]
fn every_statement_is_consumed_and_entities_and_quoted_semicolons_stay_in_labels() {
    let (authority, scope, control) = setup();
    let source = "%% comment\r\nsequenceDiagram; actor A as \"A; alias\"; participant B\r\nA->>B: \"one; two\"; B-->>A: three #59; four; Note over A,B: &amp;\r\n%% final comment";
    let model = Parser::new(&scope, &control).parse(source).unwrap();
    assert_eq!(model.participants.len(), 2);
    assert_eq!(model.events.len(), 3);
    let timeline =
        Timeline::prepare(model, LayoutPolicy::default(), &Font, &scope, &control).unwrap();
    assert_eq!(
        timeline.model.participants[0]
            .label
            .as_ref()
            .unwrap()
            .text(),
        "A; alias"
    );
    assert_eq!(
        timeline.model.events[0].label.as_ref().unwrap().text(),
        "one; two"
    );
    assert_eq!(
        timeline.model.events[1].label.as_ref().unwrap().text(),
        "three ; four"
    );
    assert_eq!(timeline.model.events[2].label.as_ref().unwrap().text(), "&");
    drop(timeline);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn lab_editor_preview_message_and_note_reach_the_canonical_artifact() {
    let (authority, scope, control) = setup();
    let before = authority.used.load(Ordering::SeqCst);
    let output = render(LAB, &control).unwrap();
    assert!(output.bounds.right - output.bounds.left > 250.0);
    assert!(output.bounds.bottom - output.bounds.top > 250.0);
    accept(output, &scope, &control);
    assert_eq!(authority.used.load(Ordering::SeqCst), before);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn unicode_aliases_multiline_labels_and_implicit_participants_preserve_text() {
    let (authority, scope, control) = setup();
    let source = "sequenceDiagram\nactor U as \"用户\\n📝\"\nU->>P: مرحبا<br/>שלום 🧪\nNote over U,P: 编辑与预览 #59; ✅";
    let model = Parser::new(&scope, &control).parse(source).unwrap();
    assert_eq!(model.participants.len(), 2);
    assert!(model.participants[0].actor);
    assert_eq!(model.participants[1].id, "P");
    let timeline =
        Timeline::prepare(model, LayoutPolicy::default(), &Font, &scope, &control).unwrap();
    assert_eq!(
        timeline.model.participants[0]
            .label
            .as_ref()
            .unwrap()
            .text(),
        "用户\n📝"
    );
    assert_eq!(
        timeline.model.events[0].label.as_ref().unwrap().text(),
        "مرحبا\nשלום 🧪"
    );
    assert_eq!(
        timeline.model.events[1].label.as_ref().unwrap().text(),
        "编辑与预览 ; ✅"
    );
    accept(
        timeline
            .paint(ThemePolicy::default(), &Font, &scope, &control)
            .unwrap(),
        &scope,
        &control,
    );
    drop(timeline);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn classic_message_markers_autonumber_and_self_messages_remain_distinct() {
    let (authority, scope, control) = setup();
    let source = "sequenceDiagram; autonumber 10 5; A->B: line; B-->A: dashed; A->>B: filled; B-->>A: filled dashed; A-)B: async; B--)A: async dashed; A-xB: cross; B--xA: cross dashed; A->>A: self";
    let model = Parser::new(&scope, &control).parse(source).unwrap();
    let expected = [
        (false, Marker::None),
        (true, Marker::None),
        (false, Marker::Filled),
        (true, Marker::Filled),
        (false, Marker::Open),
        (true, Marker::Open),
        (false, Marker::Cross),
        (true, Marker::Cross),
        (false, Marker::Filled),
    ];
    for (index, (dashed, marker)) in expected.into_iter().enumerate() {
        let EventKind::Message { arrow, number, .. } = model.events[index].kind else {
            panic!("expected a message");
        };
        assert_eq!(arrow, Arrow { dashed, marker });
        assert_eq!(number, Some(10 + 5 * index as u32));
    }
    let timeline =
        Timeline::prepare(model, LayoutPolicy::default(), &Font, &scope, &control).unwrap();
    assert_eq!(
        timeline.model.events[8].label.as_ref().unwrap().text(),
        "50. self"
    );
    let output = timeline
        .paint(ThemePolicy::default(), &Font, &scope, &control)
        .unwrap();
    assert!(output.svg.contains("stroke-dasharray=\"5 4\""));
    accept(output, &scope, &control);
    drop(timeline);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn notes_span_and_align_to_the_requested_side_without_covering_neighbors() {
    let (authority, scope, control) = setup();
    let source = "sequenceDiagram\nparticipant A\nparticipant B\nparticipant C\nNote left of B: This note is on the left\nNote right of B: This note is on the right\nNote over C,A: Span in reverse order\nNote over B: A single participant";
    let model = Parser::new(&scope, &control).parse(source).unwrap();
    let timeline =
        Timeline::prepare(model, LayoutPolicy::default(), &Font, &scope, &control).unwrap();
    let b = timeline.model.participants[1].x;
    let left = note_rect(
        &timeline.model,
        &timeline.model.events[0],
        1,
        1,
        Side::Left,
        14.0,
    );
    let right = note_rect(
        &timeline.model,
        &timeline.model.events[1],
        1,
        1,
        Side::Right,
        14.0,
    );
    let over = note_rect(
        &timeline.model,
        &timeline.model.events[2],
        2,
        0,
        Side::Over,
        14.0,
    );
    assert!(left.right < b && right.left > b);
    assert!(left.left > timeline.model.participants[0].x);
    assert!(right.right < timeline.model.participants[2].x);
    assert!(
        over.left < timeline.model.participants[0].x
            && over.right > timeline.model.participants[2].x
    );
    accept(
        timeline
            .paint(ThemePolicy::default(), &Font, &scope, &control)
            .unwrap(),
        &scope,
        &control,
    );
    drop(timeline);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn activations_use_receiver_for_plus_and_sender_for_minus_and_can_nest() {
    let (authority, scope, control) = setup();
    let source = "sequenceDiagram\nA->>+B: call\nactivate B\nB->>B: self\ndeactivate B\nB-->>-A: return\nactivate A\nA->>B: still active";
    let model = Parser::new(&scope, &control).parse(source).unwrap();
    assert_eq!(model.activations.len(), 3);
    assert_eq!(model.activations[0].participant, 1);
    assert_eq!(model.activations[0].start, 0);
    assert_eq!(model.activations[0].end, 4);
    assert_eq!(model.activations[1].depth, 1);
    assert_eq!(model.activations[1].end, 3);
    assert_eq!(model.activations[2].participant, 0);
    assert_eq!(model.activations[2].end, UNFINISHED);
    let timeline =
        Timeline::prepare(model, LayoutPolicy::default(), &Font, &scope, &control).unwrap();
    assert!(timeline.event_y(4) > timeline.event_y(0));
    accept(
        timeline
            .paint(ThemePolicy::default(), &Font, &scope, &control)
            .unwrap(),
        &scope,
        &control,
    );
    drop(timeline);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn all_supported_frames_keep_nested_bounds_and_branch_labels() {
    let (authority, scope, control) = setup();
    let source = "sequenceDiagram\nparticipant A\nloop while editing\nalt valid\nA->>A: validate\nelse invalid\nopt retry\nA->>A: retry\nend\nend\npar save\nA->>A: local\nand sync\ncritical durable\nA->>A: write\noption failure\nbreak abort\nA-xA: failed\nend\nend\nend\nend";
    let model = Parser::new(&scope, &control).parse(source).unwrap();
    assert_eq!(model.frames.len(), 6);
    assert!(model.frames.iter().all(|f| f.end > f.start));
    let timeline =
        Timeline::prepare(model, LayoutPolicy::default(), &Font, &scope, &control).unwrap();
    for (index, frame) in timeline.model.frames.iter().enumerate() {
        let (left, right) = timeline.frame_sides(index);
        let label = timeline.model.events[frame.start].label.as_ref().unwrap();
        assert!(right - left >= label.width + 20.0);
    }
    for pair in timeline.model.events.windows(2) {
        assert!(pair[1].y >= pair[0].y + pair[0].height);
    }
    accept(
        timeline
            .paint(ThemePolicy::default(), &Font, &scope, &control)
            .unwrap(),
        &scope,
        &control,
    );
    drop(timeline);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn malformed_and_mismatched_syntax_fails_without_partial_output() {
    for body in [
        "",
        "A->>B",
        "A->>>B: bad",
        "A B->>C: bad",
        "Note over A,B,C: bad",
        "Note left of A,B: bad",
        "Note where A: bad",
        "participant A as",
        "participant A\nparticipant A",
        "end",
        "end trailing",
        "loop pending\nA->>B: hi",
        "opt maybe\nelse wrong\nend",
        "deactivate A",
        "autonumber 1 0",
        "autonumber 1 2 3",
        "A->>B: \"unterminated",
        "A->>B: an unbalanced \"quote; unknown directive",
        "A->>B: x; unknown trailing",
    ] {
        let (authority, scope, control) = setup();
        let before = authority.used.load(Ordering::SeqCst);
        let source = format!("sequenceDiagram\n{body}");
        let result = render(&source, &control);
        let expected = if body.ends_with("unknown trailing") {
            DiagramError::Unsupported
        } else {
            DiagramError::InvalidInput
        };
        assert_eq!(result.err(), Some(expected), "body={body}");
        assert_eq!(authority.used.load(Ordering::SeqCst), before, "body={body}");
        drop(control);
        drop(scope);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn exact_participant_event_and_nesting_limits_are_admitted() {
    let mut participants = String::from("sequenceDiagram\n");
    for index in 0..MAX_PARTICIPANTS {
        writeln!(participants, "participant P{index}").unwrap();
    }
    let mut events = String::from("sequenceDiagram\n");
    for _ in 0..MAX_EVENTS {
        events.push_str("A->>B: x\n");
    }
    let mut frames = String::from("sequenceDiagram\nparticipant A\n");
    for _ in 0..MAX_DEPTH {
        frames.push_str("loop a long condition that wraps onto more than one line\n");
    }
    frames.push_str("A->>A: innermost\n");
    for _ in 0..MAX_DEPTH {
        frames.push_str("end\n");
    }
    let mut activations = String::from("sequenceDiagram\nparticipant A\n");
    for _ in 0..MAX_DEPTH {
        activations.push_str("activate A\n");
    }
    activations.push_str("A->>A: innermost\n");
    for _ in 0..MAX_DEPTH {
        activations.push_str("deactivate A\n");
    }
    let prefix = "sequenceDiagram\nparticipant A\n%%";
    let source = format!("{prefix}{}", "x".repeat(MAX_SOURCE - prefix.len()));
    for input in [&participants, &events, &frames, &activations, &source] {
        let (authority, scope, control) = setup();
        let model = Parser::new(&scope, &control).parse(input).unwrap();
        assert!(!model.participants.is_empty());
        drop(model);
        drop(control);
        drop(scope);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn unsupported_features_are_rejected_instead_of_becoming_labels() {
    for body in [
        "box rgb(1,2,3)",
        "rect red",
        "create participant A",
        "destroy A",
        "link A: Help @ https://example.invalid",
        "links A: {}",
        "properties A: {}",
        "details A: text",
        "title Request-Response",
        "participant A@{ type: database }",
        "A<<->>B: bidirectional",
        "A--|>B: unknown marker",
        "autonumber off",
        "%%{init: {}}%%",
        "unrecognized directive",
    ] {
        let (authority, scope, control) = setup();
        let source = format!("sequenceDiagram\n{body}");
        assert_eq!(
            render(&source, &control).err(),
            Some(DiagramError::Unsupported),
            "body={body}"
        );
        drop(control);
        drop(scope);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn source_participant_event_frame_and_activation_limits_are_finite() {
    let mut participants = String::from("sequenceDiagram\n");
    for index in 0..=MAX_PARTICIPANTS {
        writeln!(participants, "participant P{index}").unwrap();
    }
    let mut events = String::from("sequenceDiagram\n");
    for _ in 0..=MAX_EVENTS {
        events.push_str("A->>B: x\n");
    }
    let mut frames = String::from("sequenceDiagram\nparticipant A\n");
    for _ in 0..=MAX_DEPTH {
        frames.push_str("loop x\n");
    }
    let mut activations = String::from("sequenceDiagram\nparticipant A\n");
    for _ in 0..=MAX_DEPTH {
        activations.push_str("activate A\n");
    }
    let source = "x".repeat(MAX_SOURCE + 1);
    for input in [&participants, &events, &frames, &activations, &source] {
        let (authority, scope, control) = setup();
        assert_eq!(
            render(input, &control).err(),
            Some(DiagramError::WorkLimited)
        );
        drop(control);
        drop(scope);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn cancellation_deadline_work_exhaustion_and_invalid_funding_are_terminal() {
    assert_eq!(
        render(LAB, &OperationControl::new()).err(),
        Some(DiagramError::InvalidFunding)
    );
    for mode in 0..4 {
        let (authority, scope, control) = setup();
        let before = authority.used.load(Ordering::SeqCst);
        let error = match mode {
            0 => {
                control.cancel();
                render(LAB, &control).err()
            }
            1 => {
                control.set_deadline(Duration::ZERO);
                render(LAB, &control).err()
            }
            2 => {
                authority.work.store(1_000_000, Ordering::SeqCst);
                render(LAB, &control).err()
            }
            _ => render_sequence_native_controlled(
                LAB,
                LayoutPolicy::default(),
                ThemePolicy::default(),
                &CancellingFont(&control),
                &control,
            )
            .err(),
        };
        assert_eq!(
            error,
            Some(match mode {
                1 => DiagramError::Deadline,
                2 => DiagramError::Memory(MemoryError::WorkLimited),
                _ => DiagramError::Cancelled,
            })
        );
        assert_eq!(authority.used.load(Ordering::SeqCst), before);
        drop(control);
        drop(scope);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn deadline_expiring_during_outline_keeps_its_reason_and_releases_partial_scene() {
    let (authority, scope, control) = setup();
    let before = authority.used.load(Ordering::SeqCst);
    let font = ExpiringOutlineFont {
        control: &control,
        measurements: AtomicUsize::new(0),
        outlines: AtomicUsize::new(0),
    };
    let result = render_sequence_native_controlled(
        LAB,
        LayoutPolicy::default(),
        ThemePolicy::default(),
        &font,
        &control,
    );
    assert_eq!(result.err(), Some(DiagramError::Deadline));
    assert!(font.measurements.load(Ordering::SeqCst) > 0);
    assert_eq!(font.outlines.load(Ordering::SeqCst), 1);
    assert_eq!(
        control.checkpoint().unwrap_err().reason,
        merman_core::CancelReason::DeadlineExceeded
    );
    assert_eq!(authority.used.load(Ordering::SeqCst), before);
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn every_allocation_failure_in_the_lab_fixture_releases_owned_storage() {
    let (authority, scope, control) = setup();
    let initial = authority.allocations.load(Ordering::SeqCst);
    drop(render(LAB, &control).unwrap());
    let attempts = authority.allocations.load(Ordering::SeqCst) - initial;
    drop(control);
    drop(scope);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    for offset in 1..=attempts {
        let (authority, scope, control) = setup();
        let before = authority.used.load(Ordering::SeqCst);
        let after_setup = authority.allocations.load(Ordering::SeqCst);
        authority.fail.store(after_setup + offset, Ordering::SeqCst);
        assert_eq!(
            render(LAB, &control).err(),
            Some(DiagramError::Memory(MemoryError::OutOfMemory)),
            "allocation offset={offset}"
        );
        assert_eq!(
            authority.used.load(Ordering::SeqCst),
            before,
            "allocation offset={offset}"
        );
        drop(control);
        drop(scope);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}
