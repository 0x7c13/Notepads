// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real controlled parse/layout/flatten/render tests with a synchronous fake coverage service.
//! This file does not qualify a Native producer, sampling quality, or complete budget coverage.

use super::*;
use markdown_svg_budget::{FailureKind, LimitKind};
use resvg::{
    CoverageBatchResult, CoverageConfig, CoverageProvider, CoverageRequest, CoverageRoute,
};
use std::sync::atomic::AtomicBool;

#[derive(Debug)]
struct RendererReplay {
    base: Arc<MockReplay>,
    outline: Path,
    rule: FillRule,
}

impl FontReplay for RendererReplay {
    fn coverage_epoch(&self) -> u64 {
        77
    }
    fn request_resources(&self) -> Option<ResourcesRef> {
        Some(self.base.resources.clone())
    }
    fn reserve_replay_bytes(&self, _: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
        Ok(Arc::new(MockRetention))
    }
    fn fontdb(&self) -> Result<Arc<fontdb::Database>, ()> {
        Ok(self.base.database.clone())
    }
    fn variations(&self, _: fontdb::ID) -> Result<Arc<[FontVariation]>, ()> {
        Ok(Arc::from([]))
    }
    fn outline(&self, _: fontdb::ID, _: u32) -> Result<ReplayOutline, ()> {
        Ok(ReplayOutline::Synthetic {
            data: Some(Arc::new(self.outline.clone())),
            rule: self.rule,
        })
    }
    fn prepare(
        &self,
        text: &usvg::Text,
        database: &mut Arc<fontdb::Database>,
    ) -> Result<ReplayText, ()> {
        *database = self.base.database.clone();
        let face = ReplayFace {
            id: self.base.font,
            synthetic_metrics: None,
        };
        assert_eq!(
            text.chunks().len(),
            1,
            "these fixtures retain one real text chunk"
        );
        let chunk = &text.chunks()[0];
        let cells = (0..chunk.text().len())
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
                assert_eq!(byte, b'A');
                let visual_span = chunk
                    .spans()
                    .iter()
                    .position(|span| index >= span.start() && index < span.end())
                    .expect("the fixture glyph belongs to one actual semantic style span");
                ReplayGlyph {
                    identity: Some(ReplayGlyphIdentity {
                        parent: index,
                        part: index,
                        part_count: 1,
                        position_domain: 0,
                        visual_span,
                        semantic_em: chunk.spans()[visual_span].font_size().get(),
                        native_orientation: 0,
                        script_be: u32::from_be_bytes(*b"Latn"),
                    }),
                    start: index,
                    end: index + 1,
                    glyph: 2,
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
            logical: Some(Arc::new(Proof(cells))),
            chunks: vec![glyphs.into()].into(),
            styles: vec![vec![face; chunk.spans().len()].into()].into(),
        })
    }
}

type Command = (u32, [u32; 6]);

fn commands(path: &Path) -> Vec<Command> {
    path.segments()
        .map(|segment| {
            let mut bits = [0; 6];
            let verb = match segment {
                PathSegment::MoveTo(p) => {
                    bits[..2].copy_from_slice(&[p.x.to_bits(), p.y.to_bits()]);
                    0
                }
                PathSegment::LineTo(p) => {
                    bits[..2].copy_from_slice(&[p.x.to_bits(), p.y.to_bits()]);
                    1
                }
                PathSegment::QuadTo(a, b) => {
                    bits[..4].copy_from_slice(&[
                        a.x.to_bits(),
                        a.y.to_bits(),
                        b.x.to_bits(),
                        b.y.to_bits(),
                    ]);
                    2
                }
                PathSegment::CubicTo(a, b, c) => {
                    bits.copy_from_slice(&[
                        a.x.to_bits(),
                        a.y.to_bits(),
                        b.x.to_bits(),
                        b.y.to_bits(),
                        c.x.to_bits(),
                        c.y.to_bits(),
                    ]);
                    3
                }
                PathSegment::Close => 4,
            };
            (verb, bits)
        })
        .collect()
}

#[derive(Debug)]
struct Observation {
    commands: Vec<Command>,
    matrix: [u32; 6],
    density: [u32; 2],
    size: [u32; 2],
    rule: tiny_skia::FillRule,
    live_a8: usize,
}

#[derive(Clone, Copy)]
enum Mode {
    Geometry,
    Constant(u8),
    Ineligible(u32),
    PartialFailure(Error),
    Incomplete,
}

struct Provider {
    epoch: u64,
    mode: Mode,
    account: Arc<Account>,
    calls: Mutex<Vec<Vec<Observation>>>,
    routes: Mutex<Vec<CoverageRoute>>,
}

impl Provider {
    fn new(account: Arc<Account>, mode: Mode) -> Self {
        Self {
            epoch: 77,
            mode,
            account,
            calls: Mutex::new(Vec::new()),
            routes: Mutex::new(Vec::new()),
        }
    }
}

impl CoverageProvider for Provider {
    fn epoch(&self) -> u64 {
        self.epoch
    }
    fn observe_route(&self, _: &usvg::Path, route: CoverageRoute) {
        self.routes.lock().unwrap().push(route);
    }
    fn cover_batch(
        &self,
        requests: &[CoverageRequest<'_>],
        arena: &mut [u8],
        resources: &ResourcesRef,
    ) -> Result<CoverageBatchResult, Error> {
        assert!(!requests.is_empty() && requests.len() <= 32);
        let live = self.account.live.lock().unwrap();
        let observation = requests
            .iter()
            .map(|request| {
                assert_eq!((request.target_x, request.target_y), (0, 0));
                assert_eq!(request.stride, request.width as usize);
                assert_eq!(
                    request.length,
                    request.width as usize * request.height as usize
                );
                Observation {
                    commands: commands(request.path),
                    matrix: request.matrix_bits,
                    density: [request.s_bits, request.t_bits],
                    size: [request.width, request.height],
                    rule: request.fill_rule,
                    live_a8: live.values().filter(|&&bytes| bytes == arena.len()).count(),
                }
            })
            .collect();
        drop(live);
        self.calls.lock().unwrap().push(observation);
        resources.checkpoint(0)?;
        let mask = if requests.len() == 32 {
            u32::MAX
        } else {
            (1u32 << requests.len()) - 1
        };
        let mut result = CoverageBatchResult {
            ready_mask: mask,
            ineligible_mask: 0,
            reasons: [0; 32],
            completed: true,
        };
        match self.mode {
            Mode::Geometry => {
                for request in requests {
                    let m = request.matrix_bits.map(f32::from_bits);
                    let mut coverage = tiny_skia::Mask::new(request.width, request.height).unwrap();
                    coverage.fill_path(
                        request.path,
                        request.fill_rule,
                        true,
                        Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]),
                    );
                    arena[request.a8_offset..request.a8_offset + request.length]
                        .copy_from_slice(coverage.data());
                }
            }
            Mode::Constant(value) => arena.fill(value),
            Mode::Ineligible(reason) => {
                result.ready_mask = 0;
                result.ineligible_mask = mask;
                result.reasons[..requests.len()].fill(reason);
            }
            Mode::PartialFailure(error) => {
                let count = arena.len().min(17);
                arena[..count].fill(255);
                return Err(error);
            }
            Mode::Incomplete => {
                arena.fill(255);
                result.completed = false;
            }
        }
        Ok(result)
    }
}

fn parse(r: &ResourcesRef, body: &str, rule: FillRule) -> usvg::Tree {
    let base = replay(r, true, true);
    let mut options = usvg::Options::default();
    options.fontdb = base.database.clone();
    options.font_replay = Some(Arc::new(RendererReplay {
        base,
        outline: rectangle(),
        rule,
    }));
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(r.clone()));
    usvg::Tree::from_data_controlled(
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48">{body}</svg>"#)
            .as_bytes(),
        &options,
    )
    .unwrap()
}

fn text(contents: &str, attributes: &str) -> String {
    format!(
        r#"<text x="10" y="30" font-family="Mock" font-size="20" fill="red" fill-opacity="0.5" {attributes}>{contents}</text>"#
    )
}

fn render(
    tree: &usvg::Tree,
    provider: &Provider,
    display: f64,
    transform: Transform,
) -> Result<tiny_skia::Pixmap, usvg::ProcessingError> {
    let mut output = tiny_skia::Pixmap::new(48, 48).unwrap();
    resvg::render_in_request_with_coverage(
        tree,
        transform,
        &mut output.as_mut(),
        Some(CoverageConfig::new(provider, display, 48.0, 1.0)),
    )?;
    Ok(output)
}

fn all_fill_paths<'a>(group: &'a usvg::Group, output: &mut Vec<&'a usvg::Path>) {
    for node in group.children() {
        match node {
            usvg::Node::Path(path) if path.glyph_fill_source().is_some() => output.push(path),
            usvg::Node::Group(group) => all_fill_paths(group, output),
            usvg::Node::Text(text) => all_fill_paths(text.flattened(), output),
            _ => {}
        }
    }
}

#[test]
fn whole_flatten_path_and_actual_fractional_matrix_reach_real_renderer_consumer() {
    let (r, account) = resources();
    let tree = parse(&r, &text("AA", "dx=\"0 -12\""), FillRule::NonZero);
    let mut paths = Vec::new();
    all_fill_paths(tree.root(), &mut paths);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].glyph_fill_source().unwrap().glyphs().len(), 2);
    let expected_commands = commands(paths[0].data());
    assert_eq!(
        expected_commands
            .iter()
            .filter(|(verb, _)| *verb == 4)
            .count(),
        2
    );
    let provider = Provider::new(account.clone(), Mode::Geometry);
    let matrix = Transform::from_row(0.75, -0.125, 0.25, 1.125, 3.5, -1.25);
    let output = render(&tree, &provider, 48.0, matrix).unwrap();
    let mut original = tiny_skia::Pixmap::new(48, 48).unwrap();
    resvg::render_in_request(&tree, matrix, &mut original.as_mut()).unwrap();
    assert_eq!(output.data(), original.data());
    let calls = provider.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].len(), 1);
    assert_eq!(calls[0][0].commands, expected_commands);
    assert_eq!(
        calls[0][0].matrix,
        [
            matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty
        ]
        .map(f32::to_bits)
    );
    assert_eq!(calls[0][0].density, [4f32.to_bits(), 1f32.to_bits()]);
    drop(calls);
    drop(paths);
    drop(tree);
    drop(provider);
    drop(r);
    assert!(account.live.lock().unwrap().is_empty());
}

#[test]
fn thirty_two_and_thirty_three_whole_fills_use_coarse_batches() {
    for count in [32, 33] {
        let (r, account) = resources();
        // Alternating fill colors force real flatten paint boundaries without groups.
        let spans: String = (0..count)
            .map(|index| {
                format!(
                    r##"<tspan fill="{}">A</tspan>"##,
                    if index % 2 == 0 { "#ff0000" } else { "#0000ff" }
                )
            })
            .collect();
        let tree = parse(&r, &text(&spans, ""), FillRule::NonZero);
        let mut paths = Vec::new();
        all_fill_paths(tree.root(), &mut paths);
        assert_eq!(
            paths.len(),
            count,
            "the real flatten paint boundaries must exist before testing batching"
        );
        let provider = Provider::new(account.clone(), Mode::Geometry);
        let _ = render(&tree, &provider, 48.0, Transform::identity()).unwrap();
        let sizes: Vec<_> = provider
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect();
        assert_eq!(sizes, if count == 32 { vec![32] } else { vec![32, 1] });
        drop(tree);
        drop(provider);
        drop(r);
        assert!(account.live.lock().unwrap().is_empty());
    }
}

#[test]
fn same_target_pixels_different_display_dip_freeze_distinct_root_density() {
    let (r, account) = resources();
    let tree = parse(&r, &text("A", ""), FillRule::NonZero);
    let small = Provider::new(account.clone(), Mode::Geometry);
    let large = Provider::new(account.clone(), Mode::Geometry);
    assert_eq!(
        render(&tree, &small, 48.0, Transform::identity())
            .unwrap()
            .data(),
        render(&tree, &large, 96.0, Transform::identity())
            .unwrap()
            .data()
    );
    assert_eq!(
        small.calls.lock().unwrap()[0][0].density,
        [4f32.to_bits(), 1f32.to_bits()]
    );
    assert_eq!(
        large.calls.lock().unwrap()[0][0].density,
        [8f32.to_bits(), 1f32.to_bits()]
    );
    drop(tree);
    drop(small);
    drop(large);
    drop(r);
    assert!(account.live.lock().unwrap().is_empty());
}

#[test]
fn ordinary_four_thousand_dip_rect_has_no_coverage_preflight_or_native_work() {
    let (r, account) = resources();
    let mut options = usvg::Options::default();
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(r.clone()));
    let tree = usvg::Tree::from_data_controlled(br#"<svg xmlns="http://www.w3.org/2000/svg" width="3" height="2"><rect width="3" height="2" fill="red"/></svg>"#, &options).unwrap();
    let provider = Provider::new(account.clone(), Mode::Geometry);
    let mut output = tiny_skia::Pixmap::new(3, 2).unwrap();
    resvg::render_in_request_with_coverage(
        &tree,
        Transform::identity(),
        &mut output.as_mut(),
        Some(CoverageConfig::new(&provider, 4000.0, 3.0, 1.0)),
    )
    .unwrap();
    assert_eq!(output.data(), [255, 0, 0, 255].repeat(6));
    assert!(provider.calls.lock().unwrap().is_empty());
    drop(tree);
    drop(provider);
    drop(options);
    drop(r);
    assert!(account.live.lock().unwrap().is_empty());
}

#[test]
fn explicit_non_aa_route_validates_epoch_before_using_original_target_path() {
    let (r, account) = resources();
    let tree = parse(
        &r,
        &text("A", r#"text-rendering="optimizeSpeed""#),
        FillRule::NonZero,
    );
    let provider = Provider::new(account.clone(), Mode::Geometry);
    let _ = render(&tree, &provider, 48.0, Transform::identity()).unwrap();
    assert!(provider.calls.lock().unwrap().is_empty());
    assert_eq!(
        *provider.routes.lock().unwrap(),
        [CoverageRoute::NonAntialiased]
    );
    let mut wrong = Provider::new(account.clone(), Mode::Geometry);
    wrong.epoch = 78;
    assert_eq!(
        render(&tree, &wrong, 48.0, Transform::identity())
            .unwrap_err()
            .typed_failure
            .kind,
        FailureKind::Invariant
    );
    assert!(wrong.calls.lock().unwrap().is_empty());
    assert!(wrong.routes.lock().unwrap().is_empty());
    drop(tree);
    drop(provider);
    drop(wrong);
    drop(r);
    assert!(account.live.lock().unwrap().is_empty());
}

#[test]
fn wrong_epoch_stroke_first_pattern_rejects_before_recursive_surface_allocation() {
    let (r, account) = resources();
    let source = format!(
        r##"<defs><pattern id="stroke-pattern" patternUnits="userSpaceOnUse" width="4" height="4"><rect width="4" height="4" fill="blue"/></pattern></defs>{}"##,
        text(
            "A",
            r##"stroke="url(#stroke-pattern)" stroke-width="2" paint-order="stroke fill""##
        ),
    );
    let tree = parse(&r, &source, FillRule::NonZero);
    let mut paths = Vec::new();
    all_fill_paths(tree.root(), &mut paths);
    assert_eq!(paths.len(), 1, "this is one actual flattened text fill");
    let path = paths[0];
    assert!(path.is_visible());
    assert!(path.fill().is_some());
    assert_eq!(path.paint_order(), usvg::PaintOrder::StrokeAndFill);
    assert!(matches!(
        path.stroke().unwrap().paint(),
        usvg::Paint::Pattern(_)
    ));
    let source = path.glyph_fill_source().unwrap();
    assert_eq!(source.epoch(), 77);
    assert!(source.belongs_to(&r));
    let mut provider = Provider::new(account.clone(), Mode::Geometry);
    provider.epoch = 78;
    let mut output = tiny_skia::Pixmap::new(48, 48).unwrap();
    let config = CoverageConfig::new(&provider, 48.0, 48.0, 1.0);
    let reservations_before = account.next.load(Ordering::Relaxed);
    let error = resvg::render_in_request_with_coverage(
        &tree,
        Transform::identity(),
        &mut output.as_mut(),
        Some(config),
    )
    .unwrap_err();
    assert_eq!(
        error.typed_failure,
        Error::new(FailureKind::Invariant, LimitKind::None)
    );
    assert_eq!(
        account.next.load(Ordering::Relaxed),
        reservations_before,
        "wrong source must reject before a recursive stroke pattern reserves a surface"
    );
    assert!(provider.calls.lock().unwrap().is_empty());
    assert!(provider.routes.lock().unwrap().is_empty());
    assert!(
        output.data().iter().all(|&byte| byte == 0),
        "invalid source cannot reach paint"
    );
    drop(paths);
    drop(tree);
    drop(provider);
    drop(r);
    assert!(account.live.lock().unwrap().is_empty());
}

#[test]
fn evenodd_and_clip_clear_consume_geometry_coverage_at_actual_fill() {
    for count in [1, 2] {
        let (r, account) = resources();
        let content = text(if count == 1 { "A" } else { "AA" }, "dx=\"0 -12\"");
        let source = format!(
            r##"<defs><clipPath id="clip">{content}</clipPath></defs><rect width="48" height="48" fill="blue" clip-path="url(#clip)"/>"##
        );
        let tree = parse(&r, &source, FillRule::EvenOdd);
        let provider = Provider::new(account.clone(), Mode::Constant(128));
        let output = render(&tree, &provider, 48.0, Transform::identity()).unwrap();
        let calls = provider.calls.lock().unwrap();
        // Nonempty Synthetic EvenOdd glyphs deliberately remain separate fills.
        // This checks both one Clear and the original two-fill failure fixture.
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].len(), count);
        assert!(
            calls[0]
                .iter()
                .all(|r| r.rule == tiny_skia::FillRule::EvenOdd)
        );
        // Independent exact rational alpha union: 255 * (1 - (127/255)^n).
        // One Clear is exactly 128. Two distinct Clears ideally yield about
        // 191.75; allow one byte for the original pipeline's intermediate
        // quantization. Separate fills are not one whole-path parity operation.
        let denominator = 255u32.pow(count as u32 - 1);
        let numerator = 255u32.pow(count as u32) - 127u32.pow(count as u32);
        let expected = ((numerator + denominator / 2) / denominator) as u8;
        let actual = &output.data()[4 * (24 * 48 + 15)..][..4];
        assert_eq!(&actual[..2], &[0, 0]);
        assert_eq!(actual[2], actual[3]);
        assert!(actual[3].abs_diff(expected) <= if count == 1 { 0 } else { 1 });
        drop(calls);
        drop(tree);
        drop(provider);
        drop(r);
        assert!(account.live.lock().unwrap().is_empty());
    }
}

#[test]
fn representation_ineligible_preserves_original_pixels_and_records_fixed_reason() {
    let (r, account) = resources();
    let tree = parse(&r, &text("A", ""), FillRule::NonZero);
    let provider = Provider::new(account.clone(), Mode::Ineligible(2));
    let output = render(&tree, &provider, 48.0, Transform::identity()).unwrap();
    assert_eq!(output.data(), pixels(&tree));
    assert_eq!(
        *provider.routes.lock().unwrap(),
        [CoverageRoute::RepresentationIneligible(
            resvg::CoverageIneligibleReason::PrecisionDomain
        )]
    );
    drop(tree);
    drop(provider);
    drop(r);
    assert!(account.live.lock().unwrap().is_empty());
}

#[test]
fn native_typed_failure_cancellation_and_partial_results_never_fallback_or_publish() {
    for mode in [
        Mode::PartialFailure(Error::native(-12345)),
        Mode::PartialFailure(Error::new(FailureKind::Cancelled, LimitKind::None)),
        Mode::Incomplete,
        Mode::Ineligible(99),
    ] {
        let (r, account) = resources();
        let tree = parse(&r, &text("A", ""), FillRule::NonZero);
        let provider = Provider::new(account.clone(), mode);
        let mut output = tiny_skia::Pixmap::new(48, 48).unwrap();
        let before: Vec<_> = account.live.lock().unwrap().values().copied().collect();
        let error = resvg::render_in_request_with_coverage(
            &tree,
            Transform::identity(),
            &mut output.as_mut(),
            Some(CoverageConfig::new(&provider, 48.0, 48.0, 1.0)),
        )
        .unwrap_err();
        assert_eq!(
            error.typed_failure,
            match mode {
                Mode::PartialFailure(error) => error,
                _ => Error::new(FailureKind::Invariant, LimitKind::None),
            }
        );
        assert!(
            output.data().iter().all(|&byte| byte == 0),
            "partial callback output must never reach paint"
        );
        assert!(provider.routes.lock().unwrap().is_empty());
        let after: Vec<_> = account.live.lock().unwrap().values().copied().collect();
        assert_eq!(
            before, after,
            "all request A8 and metadata allocations retire after failure"
        );
        drop(tree);
        drop(provider);
        drop(r);
        assert!(account.live.lock().unwrap().is_empty());
    }
}

struct ReleaseFaultBackend {
    account: Arc<Account>,
    armed: Arc<AtomicBool>,
}

impl Backend for ReleaseFaultBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        TestBackend(self.account.clone()).reserve(kind, bytes)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        TestBackend(self.account.clone()).allocation_committed(ticket, bytes)
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        // The payload really freed before this callback. Retain the failed
        // refund receipt in ResourcesRef, rather than pretending retirement.
        TestBackend(self.account.clone()).release_after_free(ticket)?;
        if self.armed.load(Ordering::Relaxed) {
            Err(Error::native(-67890))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        Ok(())
    }
}

struct WorkAndRefundFailure {
    armed: Arc<AtomicBool>,
}

impl CoverageProvider for WorkAndRefundFailure {
    fn epoch(&self) -> u64 {
        77
    }
    fn cover_batch(
        &self,
        requests: &[CoverageRequest<'_>],
        arena: &mut [u8],
        _: &ResourcesRef,
    ) -> Result<CoverageBatchResult, Error> {
        assert_eq!(requests.len(), 1);
        arena[0] = 255;
        self.armed.store(true, Ordering::Relaxed);
        // Deliberately unrecorded work error: the consumer must latch it
        // before its own A8/metadata destructors invoke the failing refunds.
        Err(Error::native(-12345))
    }
}

#[test]
fn original_native_work_error_precedes_failed_batch_cleanup_refunds() {
    let account = Arc::new(Account::default());
    let armed = Arc::new(AtomicBool::new(false));
    let r = ResourcesRef::new_prepaid(
        ReleaseFaultBackend {
            account: account.clone(),
            armed: armed.clone(),
        },
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    let tree = parse(&r, &text("A", ""), FillRule::NonZero);
    let provider = WorkAndRefundFailure { armed };
    let mut output = tiny_skia::Pixmap::new(48, 48).unwrap();
    let error = resvg::render_in_request_with_coverage(
        &tree,
        Transform::identity(),
        &mut output.as_mut(),
        Some(CoverageConfig::new(&provider, 48.0, 48.0, 1.0)),
    )
    .unwrap_err();
    assert_eq!(error.typed_failure, Error::native(-12345));
    assert_eq!(r.failure(), Some(Error::native(-12345)));
    assert!(
        r.statistics().release_failures >= 2,
        "both paid batch allocations attempted their real refund"
    );
    assert!(output.data().iter().all(|&byte| byte == 0));
    drop(tree);
    drop(provider);
    assert!(
        account.live.lock().unwrap().is_empty(),
        "payloads really freed"
    );
    assert!(
        r.statistics().live_leases > 0,
        "failed refunds must remain failed retirement receipts"
    );
    assert!(
        r.validate_retired().is_err(),
        "cleanup failures are never complete retirement"
    );
    assert_eq!(r.failure(), Some(Error::native(-12345)));
}

#[test]
fn isolated_negative_origin_layer_inherits_density_and_current_surface_matrix() {
    let (r, account) = resources();
    let source = format!(
        r#"<g opacity="0.5" transform="translate(-13.25 -3.5)">{}</g>"#,
        text("A", "")
    );
    let tree = parse(&r, &source, FillRule::NonZero);
    let mut paths = Vec::new();
    all_fill_paths(tree.root(), &mut paths);
    assert_eq!(paths.len(), 1);
    let bounds = paths[0].data().bounds();
    assert_eq!(
        [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()],
        [10.0, 16.0, 20.0, 30.0]
    );
    let provider = Provider::new(account.clone(), Mode::Geometry);
    let output = render(&tree, &provider, 96.0, Transform::identity()).unwrap();
    assert_eq!(output.data(), pixels(&tree));
    let calls = provider.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0][0].density, [8f32.to_bits(), 1f32.to_bits()]);
    // Handwritten glyph rectangle [10,16,20,30] becomes [-3.25,12.5,6.75,26.5].
    // The original outward two-pixel layer expansion gives origin (-6,10),
    // size (14,18), hence M = translate(6,-10) * translate(-13.25,-3.5).
    // The former -7.25 input happened to give ibbox.x == 0 and could not test x origin.
    assert_eq!(calls[0][0].size, [14, 18]);
    assert_eq!(
        calls[0][0].matrix,
        [1.0f32, 0.0, 0.0, 1.0, -7.25, -13.5].map(f32::to_bits)
    );
    assert_eq!(calls[0][0].live_a8, 1);
    drop(calls);
    drop(tree);
    drop(provider);
    drop(r);
    assert!(account.live.lock().unwrap().is_empty());
}
