// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Independent fixed-geometry pixel regressions through controlled SVG parsing,
//! real layout/flatten and resvg. Replay shaping/Native geometry are test mocks;
//! this does not claim Native shaping, allocation coverage or product qualification.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use markdown_svg_budget::{
    Backend, Error, FirstFailureRef, Limits, ResourceKind, ResourcesRef, Ticket,
};
use tiny_skia_path::{Path, PathBuilder, PathSegment, Transform};
use usvg::fontdb;
use usvg::{
    FillRule, FontReplay, FontVariation, GlyphOutlineKind, ReplayFace, ReplayGeometryObservation,
    ReplayGeometryOrigin, ReplayGlyph, ReplayGlyphIdentity, ReplayLogicalSource, ReplayOutline,
    ReplayPlacementPolicy, ReplayRetention, ReplaySourceCell, ReplayText,
};

#[derive(Debug, Default)]
struct Account {
    next: AtomicU32,
    live: Mutex<BTreeMap<u32, usize>>,
}

struct TestBackend(Arc<Account>);

impl Backend for TestBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let slot = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
        assert!(self.0.live.lock().unwrap().insert(slot, bytes).is_none());
        Ticket::new(1, slot, 1)
    }

    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        assert_eq!(self.0.live.lock().unwrap()[&ticket.slot()], bytes);
        Ok(())
    }

    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        assert!(self.0.live.lock().unwrap().remove(&ticket.slot()).is_some());
        Ok(())
    }

    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        Ok(())
    }
}

fn resources() -> (ResourcesRef, Arc<Account>) {
    let account = Arc::new(Account::default());
    (
        ResourcesRef::new_prepaid(
            TestBackend(account.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        ),
        account,
    )
}

#[derive(Debug)]
struct Proof(Vec<ReplaySourceCell>);

impl ReplayLogicalSource for Proof {
    fn cells(&self) -> &[ReplaySourceCell] {
        &self.0
    }

    fn checkpoint(&self, _: u64) -> Result<(), ()> {
        Ok(())
    }

    fn source_bidi_level(&self, _: usize) -> Option<u8> {
        Some(0)
    }
}

#[derive(Debug)]
struct MockRetention;
impl ReplayRetention for MockRetention {}

#[derive(Clone, Copy, Debug)]
struct Geometry {
    glyph: u32,
    representation: u32,
    transform: Transform,
}

#[derive(Debug)]
struct MockReplay {
    database: Arc<fontdb::Database>,
    font: fontdb::ID,
    resources: ResourcesRef,
    logical: bool,
    capture: bool,
    empty_evenodd: bool,
    observed: Mutex<Vec<Geometry>>,
}

fn rectangle() -> Path {
    let mut path = PathBuilder::new();
    path.move_to(0.0, 0.0);
    path.line_to(500.0, 0.0);
    path.line_to(500.0, 700.0);
    path.line_to(0.0, 700.0);
    path.close();
    path.finish().unwrap()
}

fn assert_local_rectangle(path: &Path) {
    // Validate the actual ordinary reader as well as the mock Synthetic shape.
    // Do not derive the oracle from this path or its observed placement.
    let mut points = Vec::new();
    let mut closes = 0;
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(point) | PathSegment::LineTo(point) => {
                points.push((point.x, point.y));
            }
            PathSegment::Close => closes += 1,
            _ => panic!("the independently decoded fixture has no curve segments"),
        }
    }
    if points.last() == points.first() {
        points.pop();
    }
    assert_eq!(closes, 1);
    assert_eq!(points.len(), 4);
    for expected in [(0.0, 0.0), (500.0, 0.0), (500.0, 700.0), (0.0, 700.0)] {
        assert!(
            points.contains(&expected),
            "actual local outline: {points:?}"
        );
    }
    for index in 0..4 {
        let (a, b) = (points[index], points[(index + 1) % 4]);
        assert!(
            a.0 == b.0 || a.1 == b.1,
            "rectangle edges must be axis aligned"
        );
    }
}

impl FontReplay for MockReplay {
    fn coverage_epoch(&self) -> u64 {
        if self.capture { 77 } else { 0 }
    }

    fn request_resources(&self) -> Option<ResourcesRef> {
        Some(self.resources.clone())
    }

    fn reserve_replay_bytes(&self, _: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
        // This isolated test producer deliberately mocks replay retention;
        // production parser/path/capture/raster allocations use real Resources.
        Ok(Arc::new(MockRetention))
    }

    fn fontdb(&self) -> Result<Arc<fontdb::Database>, ()> {
        Ok(self.database.clone())
    }

    fn variations(&self, _: fontdb::ID) -> Result<Arc<[FontVariation]>, ()> {
        Ok(Arc::from([]))
    }

    fn prepare(
        &self,
        text: &usvg::Text,
        database: &mut Arc<fontdb::Database>,
    ) -> Result<ReplayText, ()> {
        *database = self.database.clone();
        assert_eq!(
            text.chunks().len(),
            1,
            "absolute per-character positioning is forbidden"
        );
        let chunk = &text.chunks()[0];
        assert_eq!(chunk.spans().len(), 1);
        let face = ReplayFace {
            id: self.font,
            synthetic_metrics: None,
        };
        let cells: Vec<_> = (0..chunk.text().len())
            .map(|start| ReplaySourceCell {
                start,
                end: start + 1,
            })
            .collect();
        let glyphs: Vec<_> = chunk
            .text()
            .bytes()
            .enumerate()
            .map(|(index, byte)| {
                assert!(matches!(byte, b'A' | b'S'));
                ReplayGlyph {
                    identity: self.logical.then_some(ReplayGlyphIdentity {
                        parent: index,
                        part: index,
                        part_count: 1,
                        position_domain: 0,
                        visual_span: 0,
                        semantic_em: 20.0,
                        native_orientation: 0,
                        script_be: u32::from_be_bytes(*b"Latn"),
                    }),
                    start: index,
                    end: index + 1,
                    glyph: if byte == b'S' { 2 } else { 1 },
                    face,
                    geometry_origin: ReplayGeometryOrigin::FontDesignOriginYUp,
                    x_offset: 0,
                    y_offset: 0,
                    advance: 600,
                    y_advance: 0,
                    fallback_scale: 1.0,
                }
            })
            .collect();
        Ok(ReplayText {
            logical: self
                .logical
                .then(|| Arc::new(Proof(cells)) as Arc<dyn ReplayLogicalSource>),
            chunks: vec![glyphs.into()].into(),
            styles: vec![vec![face].into()].into(),
        })
    }

    fn outline(&self, font: fontdb::ID, glyph: u32) -> Result<ReplayOutline, ()> {
        assert_eq!(font, self.font);
        Ok(match glyph {
            1 => ReplayOutline::Ordinary,
            2 => ReplayOutline::Synthetic {
                data: (!self.empty_evenodd).then(|| Arc::new(rectangle())),
                rule: if self.empty_evenodd {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                },
            },
            _ => panic!("unexpected replay glyph"),
        })
    }

    fn observe_geometry(&self, value: ReplayGeometryObservation<'_>) -> Result<(), ()> {
        assert_eq!(value.font, self.font);
        assert_eq!(value.element_id, "subject");
        assert_eq!(value.units_per_em, 1000);
        assert_eq!(value.font_size, 20.0);
        assert_eq!(
            value.geometry_origin,
            ReplayGeometryOrigin::FontDesignOriginYUp
        );
        assert_eq!(
            value.placement_policy,
            ReplayPlacementPolicy::UnchangedHorizontal
        );
        let empty = self.empty_evenodd && value.glyph == 2;
        assert_eq!(
            value.fill_rule,
            if empty {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            }
        );
        assert_eq!(value.fill_opacity, Some(0.5));
        assert!(value.visible);
        if empty {
            assert!(value.path.is_none());
        } else {
            assert_local_rectangle(
                value
                    .path
                    .expect("the actual selected outline must have ink"),
            );
        }
        self.observed.lock().unwrap().push(Geometry {
            glyph: value.glyph,
            representation: value.representation,
            transform: value.representation_transform,
        });
        Ok(())
    }
}

fn ordinary_fixture() -> Vec<u8> {
    let mut bytes = include_bytes!(
        "../../../vendor/svg-bounded/fork/usvg-0.49.0/tests/fixtures/colr-foreground.ttf"
    )
    .to_vec();
    // The unchanged original's gid1/gid2 each encode exactly one contour:
    // on-curve (0,0),(500,0),(500,700),(0,700), head.unitsPerEm=1000.
    // Hide color tags only in this in-memory copy. COLD/CPAK remain ordered;
    // ordinary outlines, metrics, directory offsets and payloads stay original.
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    let mut hidden = 0;
    for record in (0..count).map(|index| 12 + index * 16) {
        if &bytes[record..record + 4] == b"COLR" {
            bytes[record..record + 4].copy_from_slice(b"COLD");
            hidden += 1;
        } else if &bytes[record..record + 4] == b"CPAL" {
            bytes[record..record + 4].copy_from_slice(b"CPAK");
            hidden += 1;
        }
    }
    assert_eq!(hidden, 2);
    bytes
}

fn replay(resources: &ResourcesRef, logical: bool, capture: bool) -> Arc<MockReplay> {
    let mut database = fontdb::Database::new();
    let font = database.push_face_info(fontdb::FaceInfo {
        id: fontdb::ID::dummy(),
        source: fontdb::Source::Binary(Arc::new(ordinary_fixture())),
        index: 0,
        families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
        post_script_name: String::new(),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    });
    Arc::new(MockReplay {
        database: Arc::new(database),
        font,
        resources: resources.clone(),
        logical,
        capture,
        empty_evenodd: false,
        observed: Mutex::new(Vec::new()),
    })
}

struct Case {
    name: &'static str,
    text: &'static str,
    dx: &'static str,
    second_x: f32,
    oracle: &'static str,
    stroke: bool,
    stroke_first: bool,
}

// These user-space paths are handwritten, independent of flattened commands.
const ONE: &str = "M10 30H20V16H10Z";
const DUPLICATE: &str = "M10 30H20V16H10Z M10 30H20V16H10Z";
const PARTIAL: &str = "M10 30H20V16H10Z M14 30H24V16H14Z";
const SEPARATE: &str = "M10 30H20V16H10Z M28 30H38V16H28Z";

fn paint(case: &Case) -> String {
    format!(
        r##"fill="#ff0000" fill-opacity="0.5" fill-rule="nonzero" stroke="{}" stroke-width="2" stroke-opacity="0.5" paint-order="{}""##,
        if case.stroke { "#0000ff" } else { "none" },
        if case.stroke_first {
            "stroke fill"
        } else {
            "fill stroke"
        },
    )
}

fn pixels(tree: &usvg::Tree) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(48, 48).unwrap();
    resvg::render_in_request(tree, Transform::identity(), &mut pixmap.as_mut()).unwrap();
    pixmap.data().to_vec()
}

fn render_subject(case: &Case, logical: bool, capture: bool) -> Vec<u8> {
    let (resources, account) = resources();
    let owner = replay(&resources, logical, capture);
    let mut options = usvg::Options::default();
    options.fontdb = owner.database.clone();
    options.font_replay = Some(owner.clone());
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    // Advance is 600/1000*20=12 DIP. Relative dx keeps all glyphs in one chunk.
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><text id="subject" x="10" y="30" dx="{}" font-family="Mock" font-size="20" {}>{}</text></svg>"#,
        case.dx,
        paint(case),
        case.text,
    );
    let tree = usvg::Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
    let usvg::Node::Text(text) = tree.node_by_id("subject").unwrap() else {
        panic!("the controlled source must retain its actual Text node");
    };
    assert_eq!(text.chunks().len(), 1);
    assert_eq!(text.layouted().len(), 1);
    let glyphs = text.layouted()[0].positioned_glyphs();
    let observed = owner.observed.lock().unwrap();
    assert_eq!(glyphs.len(), case.text.len());
    assert_eq!(observed.len(), glyphs.len());
    for (index, (glyph, geometry)) in glyphs.iter().zip(observed.iter()).enumerate() {
        let synthetic = case.text.as_bytes()[index] == b'S';
        assert_eq!(glyph.font, owner.font);
        assert_eq!(glyph.id.0, if synthetic { 2 } else { 1 });
        assert_eq!(geometry.glyph, glyph.id.0);
        assert_eq!(geometry.representation, u32::from(synthetic));
        let x = if index == 0 { 10.0 } else { case.second_x };
        let matrix = geometry.transform;
        assert_eq!(
            [
                matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty
            ],
            [0.02, 0.0, 0.0, -0.02, x, 30.0],
            "{}: actual placement",
            case.name
        );
        assert_eq!(glyph.outline_transform(), matrix);
    }
    drop(observed);
    let mut members = Vec::new();
    for node in text.flattened().children() {
        let usvg::Node::Path(path) = node else {
            panic!("the fixture has only outline paths")
        };
        if capture {
            let source = path
                .glyph_fill_source()
                .expect("capture enabled on the same request");
            assert!(source.belongs_to(&resources));
            assert_eq!(source.epoch(), 77);
            members.extend_from_slice(source.glyphs());
        } else {
            assert!(path.glyph_fill_source().is_none());
        }
    }
    if capture {
        assert_eq!(members.len(), glyphs.len());
        for (index, member) in members.iter().enumerate() {
            let synthetic = case.text.as_bytes()[index] == b'S';
            assert_eq!(
                member.representation,
                if synthetic {
                    GlyphOutlineKind::Synthetic
                } else {
                    GlyphOutlineKind::Ordinary
                }
            );
            assert!(member.local_outline_present && member.contributed_to_path);
            assert_eq!(member.source_identity.is_some(), logical);
            assert_eq!(member.layout_source_range, [index, index + 1]);
            if let Some(identity) = member.source_identity {
                assert_eq!(identity.parent, index);
                assert_eq!(identity.position_domain, 0);
                assert_eq!(identity.visual_span, 0);
            }
        }
    }
    let result = pixels(&tree);
    drop(tree);
    drop(options);
    drop(owner);
    resources.validate_retired().unwrap();
    assert!(account.live.lock().unwrap().is_empty());
    result
}

fn render_oracle(case: &Case) -> Vec<u8> {
    let (resources, account) = resources();
    let mut options = usvg::Options::default();
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    assert!(options.font_replay.is_none());
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><path id="oracle" d="{}" {}/></svg>"#,
        case.oracle,
        paint(case),
    );
    let tree = usvg::Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
    let usvg::Node::Path(_) = tree.node_by_id("oracle").unwrap() else {
        panic!("the independent oracle must be one actual Path");
    };
    assert_eq!(tree.root().children().len(), 1);
    let result = pixels(&tree);
    assert_eq!(
        &result[(23 * 48 + 16) * 4..(23 * 48 + 16) * 4 + 4],
        &[128, 0, 0, 128]
    );
    assert_eq!(&result[..4], &[0, 0, 0, 0]);
    drop(tree);
    drop(options);
    resources.validate_retired().unwrap();
    assert!(account.live.lock().unwrap().is_empty());
    result
}

fn check_pixels(logical: bool) {
    let cases = [
        Case {
            name: "single control",
            text: "S",
            dx: "0",
            second_x: 0.0,
            oracle: ONE,
            stroke: false,
            stroke_first: false,
        },
        Case {
            name: "duplicate synthetic",
            text: "SS",
            dx: "0 -12",
            second_x: 10.0,
            oracle: DUPLICATE,
            stroke: false,
            stroke_first: false,
        },
        Case {
            name: "partial synthetic",
            text: "SS",
            dx: "0 -8",
            second_x: 14.0,
            oracle: PARTIAL,
            stroke: false,
            stroke_first: false,
        },
        Case {
            name: "ordinary then synthetic",
            text: "AS",
            dx: "0 -8",
            second_x: 14.0,
            oracle: PARTIAL,
            stroke: false,
            stroke_first: false,
        },
        Case {
            name: "synthetic then ordinary",
            text: "SA",
            dx: "0 -8",
            second_x: 14.0,
            oracle: PARTIAL,
            stroke: false,
            stroke_first: false,
        },
        Case {
            name: "separate control",
            text: "SS",
            dx: "0 6",
            second_x: 28.0,
            oracle: SEPARATE,
            stroke: false,
            stroke_first: false,
        },
        Case {
            name: "synthetic fill then stroke",
            text: "SS",
            dx: "0 -8",
            second_x: 14.0,
            oracle: PARTIAL,
            stroke: true,
            stroke_first: false,
        },
        Case {
            name: "synthetic stroke then fill",
            text: "SS",
            dx: "0 -8",
            second_x: 14.0,
            oracle: PARTIAL,
            stroke: true,
            stroke_first: true,
        },
        Case {
            name: "mixed fill then stroke",
            text: "AS",
            dx: "0 -8",
            second_x: 14.0,
            oracle: PARTIAL,
            stroke: true,
            stroke_first: false,
        },
        Case {
            name: "mixed stroke then fill",
            text: "SA",
            dx: "0 -8",
            second_x: 14.0,
            oracle: PARTIAL,
            stroke: true,
            stroke_first: true,
        },
    ];
    let mut failures = Vec::new();
    for case in &cases {
        let expected = render_oracle(case);
        let disabled = render_subject(case, logical, false);
        let enabled = render_subject(case, logical, true);
        assert_eq!(
            disabled, enabled,
            "{}: capture must not change pixels",
            case.name
        );
        let differences = disabled
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .filter(|(actual, oracle)| actual != oracle)
            .count();
        if differences != 0 {
            failures.push(format!("{}: {differences}/2304 pixels differ", case.name));
        }
    }
    assert!(
        failures.is_empty(),
        "logical={logical}: independent single-Path oracle\n{}",
        failures.join("\n")
    );
}

#[test]
fn synthetic_nonzero_selected_complete_pixels_match_single_path() {
    check_pixels(true);
}

#[test]
fn synthetic_nonzero_compat_none_none_complete_pixels_match_single_path() {
    check_pixels(false);
}

fn render_empty_evenodd_subject(
    case: &Case,
    logical: bool,
    capture: bool,
    positions: &[f32],
) -> (Vec<u8>, usize, Vec<usvg::PlacedGlyphSource>) {
    let (resources, account) = resources();
    let mut owner = replay(&resources, logical, capture);
    Arc::get_mut(&mut owner).unwrap().empty_evenodd = true;
    let mut options = usvg::Options::default();
    options.fontdb = owner.database.clone();
    options.font_replay = Some(owner.clone());
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><text id="subject" x="10" y="30" dx="{}" font-family="Mock" font-size="20" {}>{}</text></svg>"#,
        case.dx,
        paint(case),
        case.text,
    );
    let tree = usvg::Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
    let usvg::Node::Text(text) = tree.node_by_id("subject").unwrap() else {
        panic!("the empty-member subject must retain its Text node");
    };
    assert_eq!(text.chunks().len(), 1);
    assert_eq!(text.layouted().len(), 1);
    let observed = owner.observed.lock().unwrap();
    assert_eq!(observed.len(), positions.len());
    for ((geometry, &x), ch) in observed.iter().zip(positions).zip(case.text.chars()) {
        assert_eq!(geometry.glyph, if ch == 'S' { 2 } else { 1 });
        assert_eq!(geometry.representation, u32::from(ch == 'S'));
        let matrix = geometry.transform;
        assert_eq!(
            [
                matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty
            ],
            [0.02, 0.0, 0.0, -0.02, x, 30.0],
        );
    }
    drop(observed);
    let mut members = Vec::new();
    let mut paths = 0;
    for node in text.flattened().children() {
        let usvg::Node::Path(path) = node else {
            panic!("the subject has only outline paths");
        };
        paths += 1;
        if capture {
            let source = path.glyph_fill_source().unwrap();
            assert!(source.belongs_to(&resources));
            assert_eq!(source.epoch(), 77);
            members.extend_from_slice(source.glyphs());
        } else {
            assert!(path.glyph_fill_source().is_none());
        }
    }
    let actual = pixels(&tree);
    drop(tree);
    drop(options);
    drop(owner);
    resources.validate_retired().unwrap();
    assert!(account.live.lock().unwrap().is_empty());
    (actual, paths, members)
}

fn check_empty_evenodd_pixels(logical: bool) {
    // These positions and the two-rectangle oracle are independent of the
    // subject's actual flattened commands. S has advance but no ink.
    let placements: [(&str, &str, &[f32]); 4] = [
        ("ASA", "0 0 -24", &[10.0, 22.0, 10.0]),
        ("ASSA", "0 0 0 -36", &[10.0, 22.0, 34.0, 10.0]),
        ("SAA", "0 -12 -12", &[10.0, 10.0, 10.0]),
        ("AAS", "0 -12 0", &[10.0, 10.0, 22.0]),
    ];
    let mut failures = Vec::new();
    for (text, dx, positions) in placements {
        for (stroke, stroke_first) in [(false, false), (true, false), (true, true)] {
            let case = Case {
                name: "empty EvenOdd member",
                text,
                dx,
                second_x: 0.0,
                oracle: DUPLICATE,
                stroke,
                stroke_first,
            };
            let expected = render_oracle(&case);
            for capture in [false, true] {
                let (actual, paths, members) =
                    render_empty_evenodd_subject(&case, logical, capture, positions);
                let differences = actual
                    .chunks_exact(4)
                    .zip(expected.chunks_exact(4))
                    .filter(|(actual, oracle)| actual != oracle)
                    .count();
                if differences != 0 {
                    failures.push(format!(
                        "{text} logical={logical} capture={capture} stroke={stroke} stroke_first={stroke_first}: {differences}/2304 pixels differ; interior={:?}",
                        &actual[(23 * 48 + 16) * 4..(23 * 48 + 16) * 4 + 4],
                    ));
                    continue;
                }
                if !failures.is_empty() {
                    // Finish all pixel diagnostics before any structural
                    // failure can hide a preceding rendering difference.
                    continue;
                }
                // Pixel differences are reported before structural checks, so
                // the original defect is a real rendering failure.
                assert_eq!(paths, 1);
                assert_eq!(members.len(), if capture { text.len() } else { 0 });
                for (index, (member, ch)) in members.iter().zip(text.chars()).enumerate() {
                    let ink = ch != 'S';
                    assert_eq!(member.local_outline_present, ink);
                    assert_eq!(member.contributed_to_path, ink);
                    assert_eq!(
                        member.representation,
                        if ink {
                            GlyphOutlineKind::Ordinary
                        } else {
                            GlyphOutlineKind::Synthetic
                        },
                    );
                    assert_eq!(member.layout_source_range, [index, index + 1]);
                    assert_eq!(member.source_identity.is_some(), logical);
                    if let Some(identity) = member.source_identity {
                        assert_eq!(identity.parent, index);
                        assert_eq!(identity.position_domain, 0);
                        assert_eq!(identity.visual_span, 0);
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "independent empty-member pixel oracle\n{}",
        failures.join("\n")
    );
}

#[test]
fn synthetic_empty_evenodd_selected_complete_pixels_match_single_path() {
    check_empty_evenodd_pixels(true);
}

#[test]
fn synthetic_empty_evenodd_compat_complete_pixels_match_single_path() {
    check_empty_evenodd_pixels(false);
}

#[path = "renderer_coverage_tests.rs"]
mod coverage_renderer_tests;
