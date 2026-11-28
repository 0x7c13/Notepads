// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_memory::AllocationAuthority;
use std::{
    alloc::Layout,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Authority {
    used: AtomicUsize,
    peak: AtomicUsize,
    work: AtomicUsize,
    work_limit: usize,
    limit: usize,
}
impl AllocationAuthority for Authority {
    fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
        let previous = self
            .used
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                used.checked_add(layout.size()).filter(|n| *n <= self.limit)
            })
            .map_err(|_| MemoryError::BudgetLimited)?;
        self.peak
            .fetch_max(previous + layout.size(), Ordering::SeqCst);
        Ok(())
    }
    fn release(&self, layout: Layout, _: Family) {
        self.used.fetch_sub(layout.size(), Ordering::SeqCst);
    }
    fn failure(&self) -> Option<MemoryError> {
        None
    }
    fn charge_work(&self, units: usize) -> Result<(), MemoryError> {
        self.work
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                old.checked_add(units).filter(|n| *n <= self.work_limit)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::WorkLimited)
    }
}
struct Font;
impl OutlineProvider for Font {
    fn measure(&self, run: TextRun<'_>) -> Result<RunMetrics, CanonicalError> {
        let width = run.text.chars().count() as f64 * run.font.em_size / 2.0;
        Ok(RunMetrics {
            advance: width,
            ascent: 8.0,
            descent: 2.0,
            x_height: 5.0,
            ink: if width == 0.0 {
                Rect::default()
            } else {
                Rect {
                    left: 0.0,
                    top: -8.0,
                    right: width,
                    bottom: 2.0,
                }
            },
        })
    }
    fn outline(&self, run: TextRun<'_>, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
        if run.text.is_empty() {
            return Ok(());
        }
        let width = self.measure(run)?.advance;
        for c in [
            PathCommand::move_to(0.0, -8.0),
            PathCommand::line_to(width, -8.0),
            PathCommand::line_to(width, 2.0),
            PathCommand::line_to(0.0, 2.0),
            PathCommand::close(),
        ] {
            sink.push(c)?;
        }
        Ok(())
    }
}
fn scope(limit: usize) -> (Arc<Authority>, MemoryScope) {
    let authority = Arc::new(Authority {
        used: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        work: AtomicUsize::new(0),
        work_limit: usize::MAX,
        limit,
    });
    let scope = MemoryScope::new(authority.clone());
    (authority, scope)
}
fn document(value: &str, scope: &MemoryScope) -> CanonicalDocument {
    compile(
        value,
        scope,
        &OperationControl::new().with_memory_scope(scope.clone()),
        &Font,
        CanonicalLimits::default(),
    )
    .expect("valid native fixture")
}

#[test]
fn matching_generator_html_only_declarations_do_not_reject_svg_labels() {
    let (authority, scope) = scope(8 * 1024 * 1024);
    let source = r##"<svg id="d" viewBox="0 0 100 50" style="max-width:100px;"><style>#d .node .label{text-align:center}#d .edgeLabel{background-color:red;text-align:center}#d .edgeLabel rect{opacity:0.5;background-color:blue;fill:green}</style><g class="node"><g class="label"><text x="10" y="20">hello</text></g></g><g class="edgeLabel"><rect width="10" height="5"/></g></svg>"##;
    let output = document(source, &scope);
    let text = std::str::from_utf8(output.as_bytes()).unwrap();
    assert!(text.contains("rgba(0,128,0,1)"));
    assert!(text.contains("opacity=\"0.5\""));
    assert!(!text.contains("<text"));
    assert!(!text.contains("text-align"));
    assert_eq!(output.text_runs, 1);
    assert!(authority.used.load(Ordering::SeqCst) > 0);
    drop(output);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn actual_html_unknown_paint_and_external_resources_are_rejected() {
    let (_, scope) = scope(8 * 1024 * 1024);
    for (source, expected) in [
        (
            r#"<svg viewBox="0 0 10 10"><foreignObject/></svg>"#,
            CanonicalError::UnsupportedElement,
        ),
        (
            r#"<svg viewBox="0 0 10 10"><g style="filter:blur(2px)"><rect width="1" height="1"/></g></svg>"#,
            CanonicalError::UnsupportedProperty,
        ),
        (
            r#"<svg viewBox="0 0 10 10"><use href="https://example.com/a.svg"/></svg>"#,
            CanonicalError::InvalidReference,
        ),
        (
            r#"<!DOCTYPE svg><svg viewBox="0 0 10 10"/>"#,
            CanonicalError::MalformedXml,
        ),
        (
            r#"<svg viewBox="0 0 10 10"><g transform="scale(1000000000)"><g transform="scale(1000000000)"><rect width="1" height="1"/></g></g></svg>"#,
            CanonicalError::InvalidGeometry,
        ),
        (
            "<svg viewBox=\"0 0 10 10\"><text>\0</text></svg>",
            CanonicalError::MalformedXml,
        ),
    ] {
        assert_eq!(
            compile(
                source,
                &scope,
                &OperationControl::new().with_memory_scope(scope.clone()),
                &Font,
                CanonicalLimits::default()
            )
            .err(),
            Some(expected)
        );
    }
}

#[test]
fn group_alpha_visibility_and_use_inheritance_keep_their_distinct_semantics() {
    let (_, scope) = scope(8 * 1024 * 1024);
    let source = r##"<svg viewBox="0 0 100 100"><defs><g id="shape"><rect width="10" height="10"/></g></defs><g opacity="0.5" visibility="hidden"><use href="#shape" fill="red" visibility="visible"/></g></svg>"##;
    let output = document(source, &scope);
    let text = std::str::from_utf8(output.as_bytes()).unwrap();
    assert!(text.contains("opacity=\"0.5\""));
    assert!(text.contains("fill=\"rgba(255,0,0,1)\""));
    assert!(text.contains("fill-opacity=\"1\""));
}

#[test]
fn relative_font_size_is_resolved_after_cascade_against_parent_font() {
    let (_, scope) = scope(8 * 1024 * 1024);
    let source = r##"<svg id="d" viewBox="0 0 100 100" font-size="10"><style>#d .s{font-size:4em}#d .s{font-size:2em}</style><text class="s" x="10" y="20">x</text></svg>"##;
    let output = document(source, &scope);
    let text = std::str::from_utf8(output.as_bytes()).unwrap();
    // One glyph at 20 DIP is 10 DIP wide under this independent test font.
    assert!(text.contains("L10 -8"));
    assert!(!text.contains("L40 -8"));
}

#[test]
fn markers_keep_negative_reference_and_default_viewport_clip() {
    let (_, scope) = scope(8 * 1024 * 1024);
    let source = r##"<svg viewBox="0 0 100 100"><defs><marker id="head" markerUnits="userSpaceOnUse" markerWidth="10" markerHeight="10" viewBox="0 0 10 10" refX="-1" refY="5" orient="auto"><circle cx="5" cy="5" r="5"/></marker></defs><path d="M10 20 L40 20" marker-start="url(#head)"/></svg>"##;
    let output = document(source, &scope);
    let text = std::str::from_utf8(output.as_bytes()).unwrap();
    assert!(text.contains("matrix(1 0 0 1 11 15)"));
    assert!(text.contains("clip-path=\"url(#m0)\""));
    assert!(!text.contains("<marker"));
}

#[test]
fn xml_scalars_and_path_curve_end_tangents_have_independent_expected_values() {
    let (_, scope) = scope(8 * 1024 * 1024);
    let decoded = xml::decode("α&amp;&#x4E2D;", &scope).unwrap();
    assert_eq!(decoded.as_str(), "α&中");
    assert!(xml::decode("&#xFFFE;", &scope).is_err());
    let path = geometry::path_info("M0 0 C0 10 10 10 10 0", 16).unwrap();
    assert_eq!(path.start_tangent, (0.0, 10.0));
    assert_eq!(path.end_tangent, (0.0, -10.0));
    assert!(geometry::path_info("M0 0 A1 1 0 2 0 10 10", 16).is_err());
}

#[test]
fn quota_rejection_and_cancellation_do_not_publish_or_keep_allocations() {
    let (authority, scope) = scope(0);
    assert_eq!(
        compile(
            r#"<svg viewBox="0 0 10 10"/>"#,
            &scope,
            &OperationControl::new().with_memory_scope(scope.clone()),
            &Font,
            CanonicalLimits::default()
        )
        .err(),
        Some(CanonicalError::Memory(MemoryError::BudgetLimited))
    );
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    let control = OperationControl::new().with_memory_scope(scope.clone());
    control.cancel();
    assert_eq!(
        compile(
            "not XML",
            &scope,
            &control,
            &Font,
            CanonicalLimits::default()
        )
        .err(),
        Some(CanonicalError::Cancelled)
    );
}

#[test]
fn work_budget_exhaustion_after_real_allocation_releases_the_entire_candidate() {
    let authority = Arc::new(Authority {
        used: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        work: AtomicUsize::new(0),
        work_limit: 128,
        limit: 8 * 1024 * 1024,
    });
    let scope = MemoryScope::new(authority.clone());
    let control = OperationControl::new().with_memory_scope(scope.clone());
    let result = compile(
        r#"<svg viewBox="0 0 10 10"><rect width="5" height="5"/></svg>"#,
        &scope,
        &control,
        &Font,
        CanonicalLimits::default(),
    );
    assert_eq!(
        result.err(),
        Some(CanonicalError::Memory(MemoryError::WorkLimited))
    );
    assert!(
        authority.peak.load(Ordering::SeqCst) > 0,
        "real funded candidate storage existed before exhaustion"
    );
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn generator_type_position_and_negated_class_selectors_preserve_real_paint() {
    let (_, scope) = scope(8 * 1024 * 1024);
    let source = r##"<svg id="d" viewBox="0 0 10 10"><style>#d g:not(.swimlane) path:first-of-type{fill:red}#d g path:nth-of-type(2){fill:blue}#d [data-look="neo"][data-color-id="color-0"].swimlane path:nth-of-type(2){filter:url(#shadow)}</style><g><path d="M0 0L1 1"/><circle r="1"/><path d="M1 1L2 2"/></g></svg>"##;
    let output = document(source, &scope);
    let text = std::str::from_utf8(output.as_bytes()).unwrap();
    assert!(text.contains("fill=\"rgba(255,0,0,1)\""));
    assert!(text.contains("fill=\"rgba(0,0,255,1)\""));
    let active = r##"<svg id="d" viewBox="0 0 10 10"><style>#d path:nth-of-type(2){filter:url(#shadow)}</style><path d="M0 0L1 1"/><path d="M1 1L2 2"/></svg>"##;
    assert_eq!(
        compile(
            active,
            &scope,
            &OperationControl::new().with_memory_scope(scope.clone()),
            &Font,
            CanonicalLimits::default()
        )
        .err(),
        Some(CanonicalError::UnsupportedProperty)
    );
    let equal_rank = r##"<svg id="d" viewBox="0 0 10 10"><style>#d g:not(.absent) path{fill:red}#d g.present path{fill:green}</style><g class="present"><path d="M0 0L1 1"/></g></svg>"##;
    let output = document(equal_rank, &scope);
    let text = std::str::from_utf8(output.as_bytes()).unwrap();
    assert!(
        text.contains("fill=\"rgba(0,128,0,1)\""),
        ":not argument contributes its class specificity, without a second pseudo-class increment"
    );
}

const TYPED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 10 10"><path fill="#000000" stroke="#112233" stroke-width="2" opacity="0.5" fill-opacity="0.8" stroke-opacity="0.4" stroke-linejoin="round" stroke-linecap="round" d="M2 2 L8 8 "/></svg>"##;
fn typed(
    value: &str,
    scope: &MemoryScope,
    commands: usize,
) -> Result<CanonicalDocument, CanonicalError> {
    accept_artifact(
        FundedString::try_from_str(scope, Family::Svg, value)?,
        ArtifactFacts {
            bounds: Rect {
                left: 0.0,
                top: 0.0,
                right: 10.0,
                bottom: 10.0,
            },
            paths: 1,
            commands,
        },
        scope,
        &OperationControl::new().with_memory_scope(scope.clone()),
        CanonicalLimits::default(),
    )
}

#[test]
fn independent_typed_artifact_checks_real_geometry_counts_alpha_and_owner_lifetime() {
    let (authority, scope) = scope(8 * 1024 * 1024);
    let artifact = typed(TYPED, &scope, 2).unwrap();
    assert_eq!(artifact.width, 10.0);
    assert_eq!(artifact.path_commands, 2);
    assert_eq!(artifact.as_bytes(), TYPED.as_bytes());
    assert!(authority.used.load(Ordering::SeqCst) > 0);
    drop(artifact);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    let empty = TYPED.replace("M2 2 L8 8 ", "");
    drop(typed(&empty, &scope, 0).unwrap());
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn independent_typed_artifact_rejects_numeric_reference_and_structure_faults() {
    let (authority, scope) = scope(8 * 1024 * 1024);
    for malformed in [
        TYPED.replace("L8 8", "L12 12"),
        TYPED.replace("L8 8", "LNaN 8"),
        TYPED.replace("M2 2 L8 8 ", "é"),
        TYPED.replace("M2 2 L8 8 ", "L2 2"),
        TYPED.replace("M2 2 L8 8 ", "M2 2 L8 8 9"),
        TYPED.replace("#000000", "url(https://example.com/x)"),
        TYPED.replace("stroke-opacity=\"0.4\"", "stroke-opacity=\"1.4\""),
        TYPED.replace("<path ", "<path transform=\"scale(2)\" "),
        TYPED.replace("</svg>", "</svg><svg/>"),
    ] {
        assert!(typed(&malformed, &scope, 2).is_err(), "{malformed}");
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
    assert!(typed(TYPED, &scope, 3).is_err());
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn independent_typed_artifact_refuses_a_different_allocation_authority() {
    let (first, source) = scope(8 * 1024 * 1024);
    let (_, destination) = scope(8 * 1024 * 1024);
    let bytes = FundedString::try_from_str(&source, Family::Svg, TYPED).unwrap();
    assert_eq!(
        accept_artifact(
            bytes,
            ArtifactFacts {
                bounds: Rect {
                    left: 0.0,
                    top: 0.0,
                    right: 10.0,
                    bottom: 10.0
                },
                paths: 1,
                commands: 2
            },
            &destination,
            &OperationControl::new().with_memory_scope(destination.clone()),
            CanonicalLimits::default()
        )
        .err(),
        Some(CanonicalError::Memory(MemoryError::Unfunded))
    );
    assert_eq!(first.used.load(Ordering::SeqCst), 0);
}

#[test]
fn unreferenced_fixed_generator_filter_definition_is_inert_but_real_filters_are_rejected() {
    let (_, scope) = scope(8 * 1024 * 1024);
    let value = r##"<svg id="d" viewBox="0 0 10 10"><defs><filter id="d-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"/></filter></defs><rect width="5" height="5"/></svg>"##;
    let output = document(value, &scope);
    assert!(
        !std::str::from_utf8(output.as_bytes())
            .unwrap()
            .contains("filter")
    );
    let active = value.replace("<rect ", "<rect filter=\"url(#d-drop-shadow)\" ");
    assert_eq!(
        compile(
            &active,
            &scope,
            &OperationControl::new().with_memory_scope(scope.clone()),
            &Font,
            CanonicalLimits::default()
        )
        .err(),
        Some(CanonicalError::UnsupportedProperty)
    );
    let unknown = value.replace("dx=\"4\"", "dx=\"9\"");
    assert_eq!(
        compile(
            &unknown,
            &scope,
            &OperationControl::new().with_memory_scope(scope.clone()),
            &Font,
            CanonicalLimits::default()
        )
        .err(),
        Some(CanonicalError::UnsupportedElement)
    );
}
