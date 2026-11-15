// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Original SFNT and prepared-result fixtures exercise the actual preselector,
//! spacing consumer and final flatten. They are not real font/HB/UWP evidence.

use super::font_replay_color_tests::sfnt;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Debug)]
struct Proof {
    cells: Vec<ReplaySourceCell>,
    cancel: bool,
}
impl ReplayLogicalSource for Proof {
    fn cells(&self) -> &[ReplaySourceCell] {
        &self.cells
    }
    fn checkpoint(&self, _: u64) -> Result<(), ()> {
        if self.cancel {
            Err(())
        } else {
            Ok(())
        }
    }
    fn source_bidi_level(&self, _: usize) -> Option<u8> {
        Some(0)
    }
}

#[derive(Debug, Default)]
struct Stats {
    selections: AtomicUsize,
    outlines: AtomicUsize,
    live: AtomicUsize,
    bytes: AtomicUsize,
}
#[derive(Debug)]
struct Retention {
    stats: Arc<Stats>,
}
impl ReplayRetention for Retention {}
impl Drop for Retention {
    fn drop(&mut self) {
        self.stats.live.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct Owner {
    db: Arc<Database>,
    id: ID,
    synthetic: bool,
    empty: bool,
    profile_case: bool,
    cancel: bool,
    fail_outline: bool,
    fail_geometry: bool,
    reject_reservation: bool,
    stats: Arc<Stats>,
    guards: Mutex<Vec<Arc<dyn ReplayRetention>>>,
    geometry: Mutex<Vec<(u32, Transform)>>,
}
impl FontReplay for Owner {
    fn prepare(&self, text: &Text, database: &mut Arc<Database>) -> Result<ReplayText, ()> {
        *database = self.db.clone();
        let face = ReplayFace {
            id: self.id,
            synthetic_metrics: Some(ReplayMetrics {
                units_per_em: 1000,
                ascent: 800,
                descent: -200,
                x_height: 500,
                underline_position: -100,
                underline_thickness: 50,
                strike_position: 300,
            }),
        };
        let mut base = 0;
        let mut cells = Vec::new();
        let mut chunks = Vec::new();
        let mut styles = Vec::new();
        for (domain, chunk) in text.chunks().iter().enumerate() {
            let mut glyphs = Vec::new();
            for (at, ch) in chunk.text().char_indices() {
                let end = at + ch.len_utf8();
                let parent = cells.len();
                let visual = chunk
                    .spans()
                    .iter()
                    .position(|span| at >= span.start() && at < span.end())
                    .ok_or(())?;
                cells.push(ReplaySourceCell {
                    start: base + at,
                    end: base + end,
                });
                glyphs.push(ReplayGlyph {
                    identity: Some(ReplayGlyphIdentity {
                        parent,
                        part: parent,
                        part_count: 1,
                        position_domain: domain,
                        visual_span: visual,
                        semantic_em: chunk.spans()[visual].font_size().get(),
                        native_orientation: 0,
                        script_be: u32::from_be_bytes(*b"Latn"),
                    }),
                    start: at,
                    end,
                    glyph: if self.profile_case && ch != '\u{200b}' {
                        2
                    } else {
                        1
                    },
                    face,
                    geometry_origin: ReplayGeometryOrigin::FontDesignOriginYUp,
                    x_offset: 0,
                    y_offset: 0,
                    advance: if self.profile_case && ch == '\u{200b}' {
                        0
                    } else {
                        600
                    },
                    y_advance: 0,
                    fallback_scale: 1.0,
                });
            }
            chunks.push(glyphs);
            styles.push(vec![face; chunk.spans().len()]);
            base += chunk.text().len();
        }
        Ok(ReplayText {
            logical: Some(Arc::new(Proof {
                cells,
                cancel: self.cancel,
            })),
            chunks: chunks
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
            styles: styles
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
        })
    }
    fn fontdb(&self) -> Result<Arc<Database>, ()> {
        Ok(self.db.clone())
    }
    fn variations(&self, _: ID) -> Result<Arc<[FontVariation]>, ()> {
        Ok(Arc::from([]))
    }
    fn outline(&self, _: ID, glyph: u32) -> Result<ReplayOutline, ()> {
        self.stats.selections.fetch_add(1, Ordering::Relaxed);
        Ok(if self.synthetic {
            let data = if self.empty && (!self.profile_case || glyph == 1) {
                None
            } else {
                let mut builder = tiny_skia_path::PathBuilder::new();
                builder.move_to(0.0, 0.0);
                builder.line_to(500.0, 0.0);
                builder.line_to(0.0, 700.0);
                builder.close();
                Some(Arc::new(builder.finish().unwrap()))
            };
            ReplayOutline::Synthetic {
                data,
                rule: FillRule::EvenOdd,
            }
        } else {
            ReplayOutline::Ordinary
        })
    }
    fn observe_outline(&self, _: ReplayOutlineObservation<'_>) -> Result<(), ()> {
        self.stats.outlines.fetch_add(1, Ordering::Relaxed);
        if self.fail_outline {
            Err(())
        } else {
            Ok(())
        }
    }
    fn observe_geometry(&self, value: ReplayGeometryObservation<'_>) -> Result<(), ()> {
        self.geometry
            .lock()
            .unwrap()
            .push((value.representation, value.representation_transform));
        if self.fail_geometry {
            Err(())
        } else {
            Ok(())
        }
    }
    fn reserve_replay_bytes(&self, bytes: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
        if self.reject_reservation {
            return Err(());
        }
        self.stats.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.stats.live.fetch_add(1, Ordering::Relaxed);
        let guard: Arc<dyn ReplayRetention> = Arc::new(Retention {
            stats: self.stats.clone(),
        });
        self.guards.lock().unwrap().push(guard.clone());
        Ok(guard)
    }
}

fn owner(data: Vec<u8>, synthetic: bool, empty: bool) -> Owner {
    let mut db = Database::new();
    let id = db.push_face_info(fontdb::FaceInfo {
        id: ID::dummy(),
        source: fontdb::Source::Binary(Arc::new(data)),
        index: 0,
        families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
        post_script_name: String::new(),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    });
    Owner {
        db: Arc::new(db),
        id,
        synthetic,
        empty,
        profile_case: false,
        cancel: false,
        fail_outline: false,
        fail_geometry: false,
        reject_reservation: false,
        stats: Arc::new(Stats::default()),
        guards: Mutex::new(Vec::new()),
        geometry: Mutex::new(Vec::new()),
    }
}
fn options(owner: &Arc<Owner>) -> Options<'static> {
    Options {
        fontdb: owner.db.clone(),
        font_replay: Some(owner.clone()),
        ..Options::default()
    }
}
fn parse(options: &Options) -> Result<Tree, Error> {
    Tree::from_data_in_request(SOURCE.as_bytes(), options)
}
const SOURCE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><path d="M0 0H5V5Z"/><text x="10" y="50" font-family="Mock" font-size="20" letter-spacing="3">AB</text></svg>"#;

fn colr_foreground_fills(group: &Group, output: &mut Vec<(Color, f32)>) {
    for node in group.children() {
        match node {
            Node::Group(group) => colr_foreground_fills(group, output),
            Node::Text(text) => colr_foreground_fills(text.flattened(), output),
            Node::Path(path) => {
                if let Some(fill) = path.fill() {
                    if let Paint::Color(color) = fill.paint() {
                        output.push((*color, fill.opacity().get()));
                    }
                }
            }
            Node::Image(_) => panic!("COLR fixture must produce vector paths"),
        }
    }
}

#[test]
fn colr_foreground_ordinary_and_replay_tree_preserve_color_rgba_and_inheritance() {
    let data = include_bytes!("../../tests/fixtures/colr-foreground.ttf");
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><text x="10" y="40" font-family="Mock" font-size="20" color="red" fill="lime">A<tspan color="blue">A</tspan><tspan color="rgba(255,0,0,0.5)">A</tspan><tspan>A</tspan></text><text x="10" y="80" font-family="Mock" font-size="20">A</text></svg>"#;
    for replay in [false, true] {
        let owner = Arc::new(owner(data.to_vec(), false, false));
        let mut opt = options(&owner);
        if !replay {
            opt.font_replay = None;
        }
        let tree = Tree::from_data_in_request(source.as_bytes(), &opt).unwrap();
        let mut fills = Vec::new();
        colr_foreground_fills(tree.root(), &mut fills);
        let red = Color::new_rgb(255, 0, 0);
        assert_eq!(
            fills,
            vec![
                (red, 1.0),
                (Color::new_rgb(0, 0, 255), 1.0),
                (red, 128.0 / 255.0),
                (red, 1.0),
                (Color::black(), 1.0)
            ],
            "replay={replay}; glyph foreground comes from color, not fill"
        );
        assert_eq!(
            owner.stats.selections.load(Ordering::Relaxed),
            if replay { 5 } else { 0 }
        );
        assert!(!opt.processing.failed());
    }
}

fn table(data: &[u8], tag: &[u8; 4]) -> usize {
    let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    (0..count)
        .map(|i| 12 + i * 16)
        .find(|&at| &data[at..at + 4] == tag)
        .unwrap()
}
fn empty_outline(mut data: Vec<u8>) -> Vec<u8> {
    let record = table(&data, b"loca");
    let offset = u32::from_be_bytes(data[record + 8..record + 12].try_into().unwrap()) as usize;
    data[offset + 4..offset + 6].copy_from_slice(&0u16.to_be_bytes());
    data[offset + 6..offset + 8].copy_from_slice(&17u16.to_be_bytes());
    data
}

fn with_font_svg(data: Vec<u8>, document: &[u8]) -> Vec<u8> {
    let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    let mut tables = Vec::new();
    for index in 0..count {
        let at = 12 + index * 16;
        let tag: [u8; 4] = data[at..at + 4].try_into().unwrap();
        let offset = u32::from_be_bytes(data[at + 8..at + 12].try_into().unwrap()) as usize;
        let length = u32::from_be_bytes(data[at + 12..at + 16].try_into().unwrap()) as usize;
        let mut payload = data[offset..offset + length].to_vec();
        if tag == *b"SVG " {
            payload.truncate(24);
            payload[20..24].copy_from_slice(&(document.len() as u32).to_be_bytes());
            payload.extend_from_slice(document);
        }
        tables.push((tag, payload));
    }
    let mut result = data[..12].to_vec();
    let mut offset = 12 + count * 16;
    for (tag, payload) in &tables {
        result.extend(tag);
        result.extend(0u32.to_be_bytes());
        result.extend((offset as u32).to_be_bytes());
        result.extend((payload.len() as u32).to_be_bytes());
        offset += (payload.len() + 3) & !3;
    }
    for (_, payload) in tables {
        result.extend(payload);
        while result.len() % 4 != 0 {
            result.push(0);
        }
    }
    result
}

#[test]
fn actual_selection_empty_colour_and_transparent_payload_drive_same_spacing_then_flatten() {
    let empty_svg =
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><g/></svg>"#;
    let transparent_svg=br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><path d="M0 0H500V700H0Z" opacity="0"/></svg>"#;
    let cases = vec![
        (
            empty_outline(sfnt(false, false, false)),
            false,
            false,
            15.0,
            0,
        ),
        (sfnt(false, false, false), true, true, 15.0, 1),
        (sfnt(false, false, false), true, false, 18.0, 1),
        (
            empty_outline(sfnt(true, false, false)),
            false,
            false,
            18.0,
            2,
        ),
        (
            empty_outline(sfnt(false, true, false)),
            false,
            false,
            18.0,
            3,
        ),
        (
            empty_outline(sfnt(false, false, true)),
            false,
            false,
            18.0,
            4,
        ),
        (
            with_font_svg(empty_outline(sfnt(false, true, false)), empty_svg),
            false,
            false,
            18.0,
            3,
        ),
        (
            with_font_svg(empty_outline(sfnt(false, true, false)), transparent_svg),
            false,
            false,
            18.0,
            3,
        ),
    ];
    let source = SOURCE.replace(">AB</text>", ">A&#x200B;B</text>");
    for (data, synthetic, empty, expected, middle_representation) in cases {
        let mut owner = owner(data, synthetic, empty);
        owner.profile_case = true;
        let owner = Arc::new(owner);
        let options = options(&owner);
        let tree = Tree::from_data_in_request(source.as_bytes(), &options).unwrap();
        let observed = owner.geometry.lock().unwrap();
        assert_eq!(owner.stats.selections.load(Ordering::Relaxed), 3);
        assert_eq!(observed.len(), 3);
        assert_eq!(observed[1].0, middle_representation);
        assert!(
            (observed[2].1.tx - observed[0].1.tx - expected).abs() < 0.0001,
            "representation {middle_representation}: {observed:?}"
        );
        drop(observed);
        drop(tree);
    }
}

#[test]
fn preselected_repeated_glyph_uses_one_lookup_per_occurrence_and_same_final_placement() {
    let owner = Arc::new(owner(sfnt(false, false, false), true, false));
    let stats = owner.stats.clone();
    let options = options(&owner);
    let tree = parse(&options).unwrap();
    assert_eq!(stats.selections.load(Ordering::Relaxed), 2);
    let geometry = owner.geometry.lock().unwrap();
    assert_eq!(geometry.len(), 2);
    assert!(geometry.iter().all(|value| value.0 == 1));
    assert_eq!(geometry[1].1.tx - geometry[0].1.tx, 15.0);
    assert_eq!(geometry[0].1.sx, 0.02);
    assert_eq!(geometry[0].1.sy, -0.02);
    drop(geometry);
    assert!(
        stats.bytes.load(Ordering::Relaxed)
            > 2 * std::mem::size_of::<crate::text::flatten::LocalRepresentation>()
    );
    assert!(stats.live.load(Ordering::Relaxed) > 0);
    drop(tree);
    drop(options);
    drop(owner);
    assert_eq!(stats.live.load(Ordering::Relaxed), 0);
}

#[test]
fn ordinary_successful_empty_draw_is_distinct_from_absent_outline() {
    let db = owner(empty_outline(sfnt(false, false, false)), false, false);
    let processing = ProcessingContext::default();
    assert!(matches!(
        db.db
            .owned_outline(db.id, GlyphId(1), &[], None, &processing),
        Ok(crate::text::flatten::OwnedOutline::Empty)
    ));
    let mut bad = sfnt(false, false, false);
    let record = table(&bad, b"glyf");
    bad[record..record + 4].copy_from_slice(b"none");
    let bad = owner(bad, false, false);
    assert!(matches!(
        bad.db
            .owned_outline(bad.id, GlyphId(1), &[], None, &processing),
        Ok(crate::text::flatten::OwnedOutline::Unknown)
    ));
    let bad = Arc::new(bad);
    let options = options(&bad);
    assert!(parse(&options).is_err());
    assert_eq!(
        options.processing.failure(),
        Some(ProcessingFailure::FontReplay)
    );
    assert!(bad.geometry.lock().unwrap().is_empty());
}

#[test]
fn valid_colour_representation_beats_empty_outline_and_keeps_existing_priority() {
    for (colr, svg, bitmap, expected) in [
        (true, false, false, 2),
        (false, true, false, 3),
        (false, false, true, 4),
        (true, true, true, 2),
    ] {
        let owner = Arc::new(owner(empty_outline(sfnt(colr, svg, bitmap)), false, false));
        let options = options(&owner);
        let tree = parse(&options).unwrap();
        assert_eq!(owner.stats.selections.load(Ordering::Relaxed), 2);
        assert!(owner
            .geometry
            .lock()
            .unwrap()
            .iter()
            .all(|v| v.0 == expected));
        assert_eq!(owner.stats.outlines.load(Ordering::Relaxed), 0);
        drop(tree);
    }
}

#[test]
fn malformed_font_svg_and_outline_observer_failure_cannot_become_noink_or_ready() {
    let mut data = empty_outline(sfnt(false, true, false));
    let at = data.windows(4).position(|w| w == b"<svg").unwrap();
    data[at..at + 4].copy_from_slice(b"xxxx");
    let bad = Arc::new(owner(data, false, false));
    let bad_options = options(&bad);
    assert!(parse(&bad_options).is_err());
    assert_eq!(
        bad_options.processing.failure(),
        Some(ProcessingFailure::Decode)
    );
    assert!(bad.geometry.lock().unwrap().is_empty());
    let mut bad = owner(sfnt(false, false, false), false, false);
    bad.fail_outline = true;
    let bad = Arc::new(bad);
    let options = options(&bad);
    assert!(parse(&options).is_err());
    assert_eq!(
        options.processing.failure(),
        Some(ProcessingFailure::FontReplay)
    );
    assert!(bad.geometry.lock().unwrap().is_empty());
}

#[test]
fn checked_cancellation_and_failed_reservation_stop_before_selection_and_preserve_first_error() {
    for cancelled in [false, true] {
        let mut owner = owner(sfnt(false, false, false), true, false);
        owner.cancel = cancelled;
        owner.reject_reservation = !cancelled;
        let owner = Arc::new(owner);
        let options = options(&owner);
        assert!(parse(&options).is_err());
        assert_eq!(owner.stats.selections.load(Ordering::Relaxed), 0);
        assert!(owner.geometry.lock().unwrap().is_empty());
    }
    let owner = Arc::new(owner(sfnt(false, false, false), true, false));
    let options = options(&owner);
    options.processing.fail(ProcessingFailure::ImageDecode);
    assert!(parse(&options).is_err());
    assert_eq!(
        options.processing.failure(),
        Some(ProcessingFailure::ImageDecode)
    );
    assert_eq!(owner.stats.selections.load(Ordering::Relaxed), 0);
}

#[test]
fn body_failure_retires_all_representation_retention_after_request_owner_drops() {
    let mut owner = owner(sfnt(false, false, false), true, false);
    owner.fail_geometry = true;
    let owner = Arc::new(owner);
    let stats = owner.stats.clone();
    let options = options(&owner);
    assert!(parse(&options).is_err());
    assert_eq!(
        options.processing.failure(),
        Some(ProcessingFailure::FontReplay)
    );
    assert_eq!(stats.selections.load(Ordering::Relaxed), 2);
    assert!(stats.live.load(Ordering::Relaxed) > 0);
    drop(options);
    drop(owner);
    assert_eq!(stats.live.load(Ordering::Relaxed), 0);
}

// New profile counterparts. The four historical assertions above remain as
// recorded observations of the old, nonconforming SVG fixture / wrapper error.
#[test]
fn compliant_font_source_empty_and_transparent_svg_keep_same_representation_spacing() {
    let docs: [&[u8];3]=[
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><path d="M0 0h500v700H0z" fill="red"/></svg>"#,
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><g/></svg>"#,
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><path d="M0 0h500v700H0z" opacity="0"/></svg>"#,
    ];
    let source = SOURCE.replace(">AB</text>", ">A&#x200B;B</text>");
    for doc in docs {
        let data = with_font_svg(empty_outline(sfnt(false, true, false)), doc);
        let mut value = owner(data, false, false);
        value.profile_case = true;
        let value = Arc::new(value);
        let opt = options(&value);
        let tree = Tree::from_data_in_request(source.as_bytes(), &opt).unwrap();
        let observed = value.geometry.lock().unwrap();
        assert_eq!(value.stats.selections.load(Ordering::Relaxed), 3);
        assert_eq!(observed.len(), 3);
        assert_eq!(observed[1].0, 3);
        assert!((observed[2].1.tx - observed[0].1.tx - 18.0).abs() < 0.0001);
        drop(observed);
        drop(tree);
    }
}
#[test]
fn compliant_font_source_priority_keeps_colr_svg_bitmap_over_empty_outline() {
    let doc=br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><path d="M0 0h500v700H0z" fill="red"/></svg>"#;
    for (colr, svg, bitmap, expected) in [
        (true, false, false, 2),
        (false, true, false, 3),
        (false, false, true, 4),
        (true, true, true, 2),
    ] {
        let mut data = empty_outline(sfnt(colr, svg, bitmap));
        if svg {
            data = with_font_svg(data, doc);
        }
        let value = Arc::new(owner(data, false, false));
        let opt = options(&value);
        let tree = parse(&opt).unwrap();
        assert!(value
            .geometry
            .lock()
            .unwrap()
            .iter()
            .all(|g| g.0 == expected));
        drop(tree);
    }
}
#[test]
fn compliant_font_source_malformed_xml_keeps_decode_before_wrapper_and_outline_error_stays_failure()
{
    let data = with_font_svg(
        empty_outline(sfnt(false, true, false)),
        br#"<svg xmlns="http://www.w3.org/2000/svg" id="glyph1"><path"#,
    );
    let value = Arc::new(owner(data, false, false));
    let opt = options(&value);
    assert!(parse(&opt).is_err());
    assert_eq!(
        opt.processing.resource_failure().unwrap().kind,
        markdown_svg_budget::FailureKind::Decode
    );
    assert!(value.geometry.lock().unwrap().is_empty());
    let mut value = owner(sfnt(false, false, false), false, false);
    value.fail_outline = true;
    let value = Arc::new(value);
    let opt = options(&value);
    assert!(parse(&opt).is_err());
    assert_eq!(
        opt.processing.failure(),
        Some(ProcessingFailure::FontReplay)
    );
}
