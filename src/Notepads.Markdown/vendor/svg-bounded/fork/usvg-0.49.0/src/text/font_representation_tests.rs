// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real encoded font consumers; these compare complete scene output, not goldens.
use super::*;
use crate::geometry_test_support::{native, request, Call};
use markdown_svg_budget::{Error, Limits, ResourcesRef};

macro_rules! font {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../tests/fixtures/svg-font-representations-v1/",
            $name
        )) as &[u8]
    };
}
#[derive(Clone, Copy)]
struct Case {
    bytes: &'static [u8],
    family: &'static str,
    code: u32,
    kind: u8,
}
fn cases() -> [Case; 6] {
    [
        Case {
            bytes: font!("colr-normal-v1.ttf"),
            family: "Notepads.Markdown Fixture COLR Normal V1",
            code: 0xE105,
            kind: 0,
        },
        Case {
            bytes: font!("colr-var-v1.ttf"),
            family: "Notepads.Markdown Fixture COLR Var V1",
            code: 0xE105,
            kind: 0,
        },
        Case {
            bytes: font!("sbix-png-v1.ttf"),
            family: "Notepads.Markdown Fixture SBIX V1",
            code: 0xE200,
            kind: 1,
        },
        Case {
            bytes: font!("cbdt-png-v1.ttf"),
            family: "Notepads.Markdown Fixture CBDT V1",
            code: 0xE200,
            kind: 1,
        },
        Case {
            bytes: font!("svg-plain-v1.ttf"),
            family: "Notepads.Markdown Fixture SVG Plain V1",
            code: 0xE300,
            kind: 2,
        },
        Case {
            bytes: font!("svg-gzip-v1.ttf"),
            family: "Notepads.Markdown Fixture SVG Gzip V1",
            code: 0xE300,
            kind: 2,
        },
    ]
}
fn database(case: Case) -> (Arc<Database>, ID) {
    let mut db = Database::new();
    db.load_font_data(case.bytes.to_vec());
    let id = db.faces().next().expect("a real parsed face").id;
    (Arc::new(db), id)
}
fn options(db: Arc<Database>, resources: Option<&ResourcesRef>) -> Options<'static> {
    Options {
        fontdb: db,
        processing: Arc::new(match resources {
            Some(r) => ProcessingContext::with_resources(r.clone()),
            None => ProcessingContext::default(),
        }),
        ..Options::default()
    }
}
fn trace_request() -> (ResourcesRef, Arc<crate::geometry_test_support::State>) {
    let (r, state) = request(Limits::default());
    // This fixture-only log is allocated before conversion, outside the product
    // account. Long real COLR serialization can exceed the small mock log.
    state.lock().events.reserve(262144);
    (r, state)
}

#[test]
fn real_colr_png_plain_and_gzip_font_svg_keep_complete_tree_output() {
    for case in cases() {
        let (db, _) = database(case);
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'><text x='8' y='80' font-family=\"'{}'\" font-size='64' font-variation-settings=\"'wght' 650\" color='#2277bb'>&#x{:X};</text></svg>",
            case.family, case.code,
        );
        if case.family.ends_with("Gzip V1") && !cfg!(feature = "font-svg-gzip") {
            let (r, _) = trace_request();
            let opt = options(db, Some(&r));
            assert!(Tree::from_data_controlled(source.as_bytes(), &opt).is_err());
            assert_eq!(
                r.failure().unwrap().kind,
                markdown_svg_budget::FailureKind::Unsupported
            );
            println!("font-svg-gzip disabled: real encoded font SVG explicitly rejected");
            continue;
        }
        // The existing gzip decoder explicitly requires request resources.
        // Compare that real controlled format with the same uncompressed glyph
        // document; preserve ordinary gzip rejection rather than changing it.
        let (reference, reference_db) = if case.family.ends_with("Gzip V1") {
            assert!(Tree::from_data(source.as_bytes(), &options(db.clone(), None)).is_err());
            let plain_case = cases()[4];
            (
                source.replace(case.family, plain_case.family),
                database(plain_case).0,
            )
        } else {
            (source.clone(), db.clone())
        };
        let plain = Tree::from_data(reference.as_bytes(), &options(reference_db, None)).unwrap();
        let expected = plain.to_string(&crate::WriteOptions::default());
        assert!(
            expected.contains(if case.kind == 1 { "<image" } else { "<path" }),
            "{}: {expected}",
            case.family
        );
        let (r, state) = trace_request();
        let opt = options(db, Some(&r));
        let tree = Tree::from_data_controlled(source.as_bytes(), &opt).unwrap_or_else(|error| {
            panic!(
                "{}: {error:?}, {:?}",
                case.family,
                opt.processing.resource_failure()
            )
        });
        assert_eq!(
            tree.to_string(&crate::WriteOptions::default()),
            expected,
            "{}",
            case.family
        );
        assert_eq!(opt.processing.resource_failure(), None);
        println!(
            "real-font-tree {} checkpoints={} peak={}B",
            case.family,
            state.counts().checkpoint,
            state.lock().peak_bytes
        );
        drop(tree);
        drop(opt);
        assert_eq!(state.live_len(), 0);
        r.validate_retired().unwrap();
    }
}

enum Representation {
    Colr(Tree),
    Bitmap(BitmapImage),
    Svg(Node),
}
impl Representation {
    fn verify(&self) {
        match self {
            Self::Colr(tree) => assert!(!tree.root().children().is_empty()),
            Self::Bitmap(image) => assert_eq!(
                (image.image.size.width(), image.image.size.height()),
                (2.0, 2.0)
            ),
            Self::Svg(Node::Group(group)) => assert!(!group.children().is_empty()),
            Self::Svg(_) => panic!("font SVG must yield its converted root"),
        }
    }
}
fn representation(
    case: Case,
    db: &Database,
    id: ID,
    options: &Options,
) -> Result<Option<Representation>, Error> {
    match case.kind {
        0 => db
            .colr(
                id,
                GlyphId(2 + (case.code - 0xE100)),
                &[],
                svgtypes::Color::black(),
                options,
            )
            .map(|value| value.map(Representation::Colr)),
        1 => db
            .raster(id, GlyphId(2), &options.processing)
            .map(|value| value.map(Representation::Bitmap)),
        _ => db
            .svg(id, GlyphId(2), svgtypes::Color::black(), options)
            .map(|value| value.map(Representation::Svg)),
    }
}

#[test]
fn real_representation_work_allocations_and_last_temporary_refund_abort_typed() {
    for case in cases() {
        if case.family.ends_with("Gzip V1") && !cfg!(feature = "font-svg-gzip") {
            continue; // Feature-off rejection is exercised by the full-tree test above.
        }
        let (db, id) = database(case);
        let (r, state) = trace_request();
        let opt = options(db.clone(), Some(&r));
        let value = representation(case, &db, id, &opt).unwrap();
        assert!(value.is_some(), "{}", case.family);
        value.as_ref().unwrap().verify();
        let counts = state.counts();
        drop(value);
        drop(opt);
        r.validate_retired().unwrap();
        let faults = [
            (Call::Reserve, 1),
            (Call::Reserve, counts.reserve),
            (Call::Commit, 1),
            (Call::Commit, counts.commit),
            (Call::Checkpoint, 1),
            (Call::Checkpoint, counts.checkpoint / 2),
            (Call::Release, 1),
            (Call::Release, counts.release),
        ];
        for (call, ordinal) in faults.into_iter().filter(|(call, n)| {
            *n > 0
                && *n
                    <= match call {
                        Call::Reserve => counts.reserve,
                        Call::Commit => counts.commit,
                        Call::Checkpoint => counts.checkpoint,
                        Call::Release => counts.release,
                    }
        }) {
            let (r, state) = trace_request();
            let opt = options(db.clone(), Some(&r));
            state.arm(call, ordinal, native(-0x7F11));
            assert!(
                matches!(representation(case, &db, id, &opt), Err(error) if error == native(-0x7F11)),
                "{} {call:?}/{ordinal}",
                case.family
            );
            assert_eq!(opt.processing.resource_failure(), Some(native(-0x7F11)));
            assert!(
                state
                    .lock()
                    .events
                    .iter()
                    .any(|v| v.call == call && v.ordinal == ordinal),
                "must hit actual fault"
            );
            drop(opt);
            // Failed physical refunds retain their canonical ticket and bytes.
            if call != Call::Release {
                assert_eq!(state.live_len(), 0);
            }
        }
    }
}

#[test]
fn rejected_zero_size_png_retires_before_returning_missing_or_the_next_representation() {
    for case in &cases()[2..4] {
        let mut bytes = case.bytes.to_vec();
        let start = bytes
            .windows(8)
            .position(|v| v == b"\x89PNG\r\n\x1a\n")
            .unwrap();
        // A malformed source, not a decode oracle: the original raster path
        // copies PNG bytes before rejecting its zero-width placement metadata.
        bytes[start + 16..start + 20].copy_from_slice(&0u32.to_be_bytes());
        if case.family.contains("CBDT") {
            // Format17 has its own placement width in SmallGlyphMetrics.
            bytes[start - 8] = 0;
        }
        let mut db = Database::new();
        db.load_font_data(bytes);
        let id = db.faces().next().unwrap().id;
        let db = Arc::new(db);
        let (r, state) = trace_request();
        let opt = options(db.clone(), Some(&r));
        assert!(db
            .raster(id, GlyphId(2), &opt.processing)
            .unwrap()
            .is_none());
        let releases = state.counts().release;
        assert!(
            releases >= 2,
            "real encoded buffer and shared header retired"
        );
        drop(opt);
        r.validate_retired().unwrap();
        for ordinal in [1, releases] {
            let (r, state) = trace_request();
            let opt = options(db.clone(), Some(&r));
            state.arm(Call::Release, ordinal, native(-0x7F11));
            assert!(
                matches!(db.raster(id, GlyphId(2), &opt.processing), Err(e) if e == native(-0x7F11))
            );
            assert_eq!(r.failure(), Some(native(-0x7F11)));
            assert!(r.statistics().scratch_bytes > 0);
        }
    }
}

#[test]
fn encoded_copy_checks_every_256_byte_window_and_never_publishes_after_cancel() {
    let mut counts = Vec::new();
    for length in [256, 257, 512, 513] {
        let input = vec![0x71u8; length];
        let (r, state) = trace_request();
        let data = crate::EncodedImageData::try_copy_from_slice(&r, &input).unwrap();
        assert_eq!(data.as_slice(), input);
        counts.push(state.counts().checkpoint);
        drop(data);
        r.validate_retired().unwrap();
    }
    assert_eq!(counts[1], counts[0] + 2);
    assert_eq!(counts[3], counts[2] + 2);
    for ordinal in 1..=counts[3] {
        let (r, state) = trace_request();
        state.arm(Call::Checkpoint, ordinal, native(-0x7F11));
        assert!(
            matches!(crate::EncodedImageData::try_copy_from_slice(&r, &[0x71; 513]), Err(e) if e == native(-0x7F11))
        );
        assert_eq!(r.failure(), Some(native(-0x7F11)));
        assert_eq!(state.live_len(), 0);
        assert_eq!(
            state.counts().checkpoint,
            ordinal,
            "no later backend action"
        );
    }
}

#[test]
fn actual_color_probe_is_per_glyph_and_stops_on_existing_failure() {
    for case in cases() {
        let (db, id) = database(case);
        let (r, state) = trace_request();
        let opt = options(db, Some(&r));
        let glyph = if case.kind == 0 { 7 } else { 2 };
        assert_eq!(
            opt.fontdb
                .has_color_glyph(id, GlyphId(glyph), &opt.processing)
                .unwrap(),
            Some(true)
        );
        assert_eq!(
            opt.fontdb
                .has_color_glyph(id, GlyphId(1), &opt.processing)
                .unwrap(),
            Some(false)
        );
        r.record(native(-0x7F11));
        let before = state.counts().checkpoint;
        assert_eq!(
            opt.fontdb
                .has_color_glyph(id, GlyphId(glyph), &opt.processing),
            Err(native(-0x7F11))
        );
        assert_eq!(state.counts().checkpoint, before);
        drop(opt);
        assert_eq!(state.live_len(), 0);
    }
}

#[test]
fn sweep_fonts_are_explicitly_unsupported_without_partial_tree_or_fallback() {
    use markdown_svg_budget::{FailureKind, LimitKind};
    let unsupported = Error::new(FailureKind::Unsupported, LimitKind::None);
    for case in &cases()[..2] {
        let (db, id) = database(*case);
        let (r, state) = trace_request();
        let opt = options(db.clone(), Some(&r));
        assert!(
            matches!(db.colr(id, GlyphId(14), &[], svgtypes::Color::black(), &opt), Err(e) if e == unsupported)
        );
        let releases = state.counts().release;
        assert!(releases > 0);
        assert_eq!(r.failure(), Some(unsupported));
        drop(opt);
        r.validate_retired().unwrap();

        // The operation's Unsupported error precedes a real final cleanup fault.
        let (r, state) = trace_request();
        let opt = options(db.clone(), Some(&r));
        state.arm(Call::Release, releases, native(-0x7F11));
        assert!(
            matches!(db.colr(id, GlyphId(14), &[], svgtypes::Color::black(), &opt), Err(e) if e == unsupported)
        );
        assert_eq!(r.failure(), Some(unsupported));
        assert!(state.live_len() > 0, "failed refund must remain charged");
        drop(opt);

        let (r, state) = trace_request();
        let opt = options(db.clone(), Some(&r));
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='180' height='100'><text y='80' font-family=\"'{}'\" font-size='64'>&#xE100;&#xE10C;&#xE101;</text></svg>",
            case.family,
        );
        assert!(Tree::from_data_controlled(source.as_bytes(), &opt).is_err());
        assert_eq!(opt.processing.resource_failure(), Some(unsupported));
        assert_eq!(
            (opt.processing.counts().2, opt.processing.counts().3),
            (0, 0)
        );
        drop(opt);
        assert_eq!(state.live_len(), 0);
        r.validate_retired().unwrap();

        let (r, state) = trace_request();
        let opt = options(db.clone(), Some(&r));
        r.record(native(-0x7F11));
        let calls = state.counts().checkpoint;
        assert!(
            matches!(db.colr(id, GlyphId(14), &[], svgtypes::Color::black(), &opt), Err(e) if e == native(-0x7F11))
        );
        assert_eq!(r.failure(), Some(native(-0x7F11)));
        assert_eq!(state.counts().checkpoint, calls);
    }
}
