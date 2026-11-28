// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
#[cfg(feature = "raster-images")]
#[path = "gif_tests.rs"]
mod gif_integration;
#[cfg(feature = "raster-images")]
#[path = "jpeg_tests.rs"]
mod jpeg_integration;
#[cfg(feature = "raster-images")]
#[path = "png_tests.rs"]
mod png_integration;
#[cfg(feature = "raster-images")]
#[path = "webp_tests.rs"]
mod webp_integration;
use markdown_svg_budget::{Backend, FirstFailureRef, Limits, Ticket};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

struct State {
    first: FirstFailureRef,
    quota: u64,
    used: AtomicU64,
    peak: AtomicU64,
    next: AtomicU64,
    calls: AtomicU64,
    cancel_at: AtomicU64,
    arm_cancel_on_commit_bytes: AtomicU64,
    arm_cancel_after_commit: AtomicU64,
    cancellation_armed_at: AtomicU64,
    capture_ticket_bytes: AtomicU64,
    capture_matches: AtomicU64,
    captured_ticket: AtomicU64,
    reject_captured_ticket: AtomicU64,
    reject_release_ticket: AtomicU64,
    trace_mode: AtomicU64,
    reject_release_after: AtomicU64,
    release_calls: AtomicU64,
    rejected_releases: AtomicU64,
    reject_reserve_bytes: AtomicU64,
    reject_release_bytes: AtomicU64,
    sizes: [AtomicU64; 1024],
    kinds: [AtomicU64; 1024],
}
#[derive(Clone)]
struct TestBackend(Arc<State>);
impl Backend for TestBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        if bytes as u64 == self.0.reject_reserve_bytes.load(Ordering::SeqCst) {
            return Err(Error::native(-778));
        }
        let old = self
            .0
            .used
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                used.checked_add(bytes as u64)
                    .filter(|new| *new <= self.0.quota)
            })
            .map_err(|_| Error::new(FailureKind::Quota, LimitKind::Bytes))?;
        self.0.peak.fetch_max(old + bytes as u64, Ordering::SeqCst);
        let id = self.0.next.fetch_add(1, Ordering::SeqCst);
        assert!(id < self.0.sizes.len() as u64);
        self.0.sizes[id as usize].store(bytes as u64, Ordering::SeqCst);
        self.0.kinds[id as usize].store(kind as u64, Ordering::SeqCst);
        if self.0.trace_mode.load(Ordering::SeqCst) != 0 {
            println!(
                "OWNER_ANCHOR mode={} event=reserve ticket={} bytes={} kind={kind:?} first={:?}",
                self.0.trace_mode.load(Ordering::SeqCst),
                id + 1,
                bytes,
                self.0.first.get()
            );
        }
        Ticket::new(1, (id + 1) as u32, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let slot = u64::from(ticket.slot());
        let kind = self.0.kinds[(slot - 1) as usize].load(Ordering::SeqCst);
        if self.0.trace_mode.load(Ordering::SeqCst) != 0 {
            println!("OWNER_ANCHOR mode={} event=commit ticket={slot} bytes={bytes} kind={kind} calls={} first={:?}", self.0.trace_mode.load(Ordering::SeqCst), self.0.calls.load(Ordering::SeqCst), self.0.first.get());
        }
        if bytes as u64 == self.0.capture_ticket_bytes.load(Ordering::SeqCst) {
            assert_eq!(kind, ResourceKind::Scratch as u64);
            self.0.capture_matches.fetch_add(1, Ordering::SeqCst);
            if self
                .0
                .captured_ticket
                .compare_exchange(0, slot, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
                && self.0.reject_captured_ticket.load(Ordering::SeqCst) != 0
            {
                self.0.reject_release_ticket.store(slot, Ordering::SeqCst);
            }
        }
        if bytes as u64 == self.0.arm_cancel_on_commit_bytes.load(Ordering::SeqCst) {
            let calls = self.0.calls.load(Ordering::SeqCst);
            if self
                .0
                .cancellation_armed_at
                .compare_exchange(u64::MAX, calls, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                self.0.cancel_at.store(
                    calls + self.0.arm_cancel_after_commit.load(Ordering::SeqCst),
                    Ordering::SeqCst,
                );
            }
        }
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let slot = u64::from(ticket.slot());
        let bytes = self.0.sizes[(u64::from(ticket.slot()) - 1) as usize].swap(0, Ordering::SeqCst);
        assert_ne!(bytes, 0);
        self.0.used.fetch_sub(bytes, Ordering::SeqCst);
        let rejected = self.0.release_calls.fetch_add(1, Ordering::SeqCst)
            >= self.0.reject_release_after.load(Ordering::SeqCst)
            || bytes == self.0.reject_release_bytes.load(Ordering::SeqCst)
            || slot == self.0.reject_release_ticket.load(Ordering::SeqCst);
        if self.0.trace_mode.load(Ordering::SeqCst) != 0 {
            println!("OWNER_ANCHOR mode={} event=release ticket={slot} bytes={bytes} kind={} rejected={rejected} first={:?}", self.0.trace_mode.load(Ordering::SeqCst), self.0.kinds[(slot-1) as usize].load(Ordering::SeqCst), self.0.first.get());
        }
        if rejected {
            self.0.rejected_releases.fetch_add(1, Ordering::SeqCst);
            Err(Error::native(-777))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        let call = self.0.calls.fetch_add(1, Ordering::SeqCst);
        if call >= self.0.cancel_at.load(Ordering::SeqCst) {
            if self.0.trace_mode.load(Ordering::SeqCst) != 0 {
                println!(
                    "OWNER_ANCHOR mode={} event=cancel ticket={} relativeCalls={} first={:?}",
                    self.0.trace_mode.load(Ordering::SeqCst),
                    self.0.captured_ticket.load(Ordering::SeqCst),
                    call + 1 - self.0.cancellation_armed_at.load(Ordering::SeqCst),
                    self.0.first.get()
                );
            }
            Err(Error::new(FailureKind::Cancelled, LimitKind::Work))
        } else {
            Ok(())
        }
    }
}
fn request(quota: u64) -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State {
        first: FirstFailureRef::new_prepaid(),
        quota,
        used: AtomicU64::new(0),
        peak: AtomicU64::new(0),
        next: AtomicU64::new(0),
        calls: AtomicU64::new(0),
        cancel_at: AtomicU64::new(u64::MAX),
        arm_cancel_on_commit_bytes: AtomicU64::new(u64::MAX),
        arm_cancel_after_commit: AtomicU64::new(0),
        cancellation_armed_at: AtomicU64::new(u64::MAX),
        capture_ticket_bytes: AtomicU64::new(u64::MAX),
        capture_matches: AtomicU64::new(0),
        captured_ticket: AtomicU64::new(0),
        reject_captured_ticket: AtomicU64::new(0),
        reject_release_ticket: AtomicU64::new(0),
        trace_mode: AtomicU64::new(0),
        reject_release_after: AtomicU64::new(u64::MAX),
        release_calls: AtomicU64::new(0),
        rejected_releases: AtomicU64::new(0),
        reject_reserve_bytes: AtomicU64::new(u64::MAX),
        reject_release_bytes: AtomicU64::new(u64::MAX),
        sizes: std::array::from_fn(|_| AtomicU64::new(0)),
        kinds: std::array::from_fn(|_| AtomicU64::new(0)),
    });
    let resources = ResourcesRef::new_prepaid(
        TestBackend(state.clone()),
        state.first.clone(),
        Limits::default(),
    );
    (resources, state)
}

#[test]
fn clone_requires_old_and_new_full_storage_and_preserves_old_on_refusal() {
    let (resources, state) = request(16_384 * 2 - 1);
    let mut original = Raster::try_create(64, 64, Some(&resources)).unwrap();
    original.fill(tiny_skia::Color::from_rgba8(20, 40, 60, 255));
    let before = original.data().to_vec();
    assert_eq!(state.used.load(Ordering::SeqCst), 16_384);
    assert!(original.try_clone().is_err());
    assert_eq!(original.data(), before);
    assert_eq!(state.peak.load(Ordering::SeqCst), 16_384);
    drop(original);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert_eq!(resources.statistics().scratch_bytes, 0);
}

#[test]
fn shared_handles_do_not_double_pay_and_unique_take_does_not_copy_pixels() {
    let (resources, state) = request(1 << 20);
    let original = Raster::try_create(64, 64, Some(&resources)).unwrap();
    let pointer = original.data().as_ptr();
    let first = SharedRaster::try_new(original).unwrap();
    let paid = state.used.load(Ordering::SeqCst);
    assert!(paid > 16_384);
    let second = first.clone();
    assert_eq!(state.used.load(Ordering::SeqCst), paid);
    drop(first);
    assert_eq!(state.used.load(Ordering::SeqCst), paid);
    let unique = second.take().unwrap();
    assert_eq!(unique.data().as_ptr(), pointer);
    assert_eq!(state.used.load(Ordering::SeqCst), 16_384);
    drop(unique);
    resources.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}

#[test]
fn shared_write_copy_keeps_the_other_reader_alive_and_fully_paid() {
    let (resources, state) = request(1 << 20);
    let first =
        SharedRaster::try_new(Raster::try_create(64, 64, Some(&resources)).unwrap()).unwrap();
    let header_and_old = state.used.load(Ordering::SeqCst);
    let second = first.clone();
    let mut copy = second.take().unwrap();
    assert_ne!(first.data().as_ptr(), copy.data().as_ptr());
    assert_eq!(state.used.load(Ordering::SeqCst), header_and_old + 16_384);
    copy.fill(tiny_skia::Color::WHITE);
    assert!(first.data().iter().all(|b| *b == 0));
    drop(copy);
    assert_eq!(state.used.load(Ordering::SeqCst), header_and_old);
    drop(first);
    resources.validate_retired().unwrap();
}

#[test]
fn cancel_inside_pixel_fill_keeps_partial_storage_paid_then_fresh_request_succeeds() {
    let (resources, state) = request(1 << 20);
    let mut image = Raster::try_create(128, 128, Some(&resources)).unwrap();
    let initial = state.calls.load(Ordering::SeqCst);
    state.cancel_at.store(initial + 1, Ordering::SeqCst);
    image.fill(tiny_skia::Color::WHITE);
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
    assert!(image.data()[..16_384].iter().all(|v| *v == 255));
    assert!(image.data()[16_384..].iter().all(|v| *v == 0));
    assert_eq!(state.used.load(Ordering::SeqCst), 65_536);
    drop(image);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    let (fresh, _) = request(1 << 20);
    let mut complete = Raster::try_create(128, 128, Some(&fresh)).unwrap();
    complete.fill(tiny_skia::Color::WHITE);
    assert!(complete.data().iter().all(|v| *v == 255));
    drop(complete);
    fresh.validate_retired().unwrap();
}

#[test]
fn mask_borrows_paid_storage_and_matches_original_alpha_and_luminance() {
    let (resources, state) = request(1 << 20);
    let mut image = Raster::try_create(32, 16, Some(&resources)).unwrap();
    image.fill(tiny_skia::Color::from_rgba8(17, 99, 231, 128));
    for kind in [tiny_skia::MaskType::Alpha, tiny_skia::MaskType::Luminance] {
        let original = tiny_skia::Mask::from_pixmap(image.as_ref(), kind);
        let mut paid = RasterMask::try_from_pixmap(&image, kind).unwrap();
        assert_eq!(paid.as_ref().data(), original.data());
        paid.invert().unwrap();
        assert!(paid
            .as_ref()
            .data()
            .iter()
            .zip(original.data())
            .all(|(a, b)| *a == 255 - *b));
    }
    drop(image);
    resources.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}

fn render_document(
    body: &str,
    controlled: bool,
    cancel_turbulence_after_setup: Option<u64>,
) -> (Vec<u8>, bool, u64) {
    let source =
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{body}</svg>");
    let (resources, state) = request(8 << 20);
    let mut options = usvg::Options::default();
    if controlled {
        options.processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    }
    let tree = if controlled {
        usvg::Tree::from_data_controlled(source.as_bytes(), &options).unwrap()
    } else {
        usvg::Tree::from_data(source.as_bytes(), &options).unwrap()
    };
    assert_eq!(tree.processing_context().resources().is_some(), controlled);
    let reserve_calls_before_render = state.next.load(Ordering::SeqCst);
    let work_calls_before_render = state.calls.load(Ordering::SeqCst);
    if let Some(additional) = cancel_turbulence_after_setup {
        // Arm at the real turbulence gradient allocation, after source drawing
        // and its paid pipelines. Its [[f64; 2]; 514] x 4 payload is 32896B.
        // Initialization needs fewer than 100 checkpoints; the million-octave
        // loop must therefore observe the injected cancellation itself.
        state
            .arm_cancel_after_commit
            .store(additional, Ordering::SeqCst);
        state
            .arm_cancel_on_commit_bytes
            .store(4 * 514 * 2 * 8, Ordering::SeqCst);
        state
            .capture_ticket_bytes
            .store(4 * 514 * 2 * 8, Ordering::SeqCst);
        state.trace_mode.store(2, Ordering::SeqCst);
    }
    let mut target = tiny_skia::Pixmap::new(64, 64).unwrap();
    let succeeded = crate::render_in_request(
        &tree,
        tiny_skia::Transform::identity(),
        &mut target.as_mut(),
    )
    .is_ok();
    let calls = state.calls.load(Ordering::SeqCst);
    if controlled {
        assert!(state.next.load(Ordering::SeqCst) > reserve_calls_before_render);
        assert!(calls > work_calls_before_render);
    }
    drop(tree);
    drop(options);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert_eq!(resources.statistics().scratch_bytes, 0);
    let scoped_calls = if cancel_turbulence_after_setup.is_some() {
        let armed_at = state.cancellation_armed_at.load(Ordering::SeqCst);
        assert_ne!(armed_at, u64::MAX, "real turbulence buffer was committed");
        assert_eq!(state.capture_matches.load(Ordering::SeqCst), 1);
        assert_ne!(state.captured_ticket.load(Ordering::SeqCst), 0);
        calls - armed_at
    } else {
        calls - work_calls_before_render
    };
    (target.take(), succeeded, scoped_calls)
}

#[test]
fn paid_layers_masks_and_filter_graph_match_the_original_compatibility_pixels() {
    let cases = [
        "<g opacity='.6'><rect x='3' y='4' width='39' height='41' fill='#379'/></g>",
        "<defs><clipPath id='c'><circle cx='25' cy='25' r='20'/></clipPath></defs><g clip-path='url(#c)'><rect width='60' height='60' fill='red'/></g>",
        "<defs><mask id='m'><rect width='64' height='64' fill='white'/><circle cx='20' cy='30' r='10' fill='#484'/></mask></defs><rect width='60' height='60' mask='url(#m)' fill='blue'/>",
        "<defs><filter id='f'><feGaussianBlur stdDeviation='2'/><feOffset dx='2' dy='3'/></filter></defs><rect x='10' y='10' width='30' height='25' filter='url(#f)' fill='#937'/>",
        "<defs><filter id='f'><feFlood flood-color='#957' result='color'/><feComposite in='color' in2='SourceAlpha' operator='in'/><feMerge><feMergeNode/><feMergeNode in='SourceGraphic'/></feMerge></filter></defs><rect x='10' y='10' width='30' height='25' filter='url(#f)' fill='#379'/>",
        "<defs><filter id='f'><feTurbulence baseFrequency='.03' numOctaves='3' seed='7'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>",
        "<defs><filter id='f'><feConvolveMatrix order='3' kernelMatrix='0 1 0 1 1 1 0 1 0'/><feMorphology operator='dilate' radius='2'/></filter></defs><rect x='8' y='8' width='32' height='32' fill='#65a' filter='url(#f)'/>",
        "<defs><filter id='f'><feTurbulence baseFrequency='.012 .017' numOctaves='5' seed='-10' stitchTiles='stitch'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>",
        "<defs><filter id='f'><feGaussianBlur stdDeviation='0 3'/></filter></defs><rect x='10' y='10' width='30' height='25' filter='url(#f)' fill='#937'/>",
    ];
    for (index, body) in cases.iter().enumerate() {
        let (original, original_ok, _) = render_document(body, false, None);
        let (paid, paid_ok, _) = render_document(body, true, None);
        assert!(original_ok && paid_ok, "case {index}");
        assert_eq!(paid, original, "case {index}");
    }
}

#[test]
fn controlled_css_cascade_reaches_real_renderer_with_identical_blue_pixels() {
    let source = "<style>.a{fill:red}g>.a{fill:blue!important}</style><g opacity='.5'><rect class='a' width='48' height='48'/></g>";
    let (expected, compat_ok, _) = render_document(source, false, None);
    let (actual, controlled_ok, _) = render_document(source, true, None);
    assert!(compat_ok && controlled_ok);
    assert_eq!(actual, expected);
    let offset = (32 * 64 + 32) * 4;
    assert_eq!(&actual[offset..offset + 4], &[0, 0, 128, 128]);
}

#[test]
fn paid_numeric_dash_filter_and_generated_css_values_keep_real_pixels() {
    let cases = [
        "<g opacity='.7'><path d='M4 4C20 55 40 2 58 58' fill='none' stroke='#369' stroke-width='3' stroke-dasharray='2 3 4' stroke-dashoffset='1'/></g>",
        "<defs><filter id='f' color-interpolation-filters='sRGB'><feColorMatrix type='matrix' values='1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0'/><feComponentTransfer><feFuncR type='table' tableValues='0 .4 1'/><feFuncG type='discrete' tableValues='0 .7 1'/></feComponentTransfer><feConvolveMatrix order='3' kernelMatrix='0 0 0 0 1 0 0 0 0' preserveAlpha='true'/></filter></defs><g opacity='.8'><rect x='8' y='8' width='40' height='40' fill='#47a' filter='url(#f)'/></g>",
        "<g opacity='.6' filter='grayscale(.25) sepia(.5) invert(.1) opacity(.75)'><rect x='8' y='8' width='40' height='40' fill='#47a'/></g>",
    ];
    for (index, source) in cases.iter().enumerate() {
        let (expected, compat_ok, _) = render_document(source, false, None);
        let (actual, controlled_ok, _) = render_document(source, true, None);
        assert!(compat_ok && controlled_ok, "case {index}");
        assert_eq!(actual, expected, "case {index}");
        assert!(actual.iter().any(|value| *value != 0), "case {index}");
    }
}

#[test]
fn real_long_turbulence_octaves_cancel_without_publishing_success() {
    let body = "<defs><filter id='f'><feTurbulence baseFrequency='.03' numOctaves='1000000' seed='7'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>";
    let (_, succeeded, calls) = render_document(body, true, Some(100));
    assert!(!succeeded);
    assert!(calls <= 150);
    let (_, fresh_succeeded, _) = render_document(
        "<g opacity='.5'><rect width='60' height='60' fill='red'/></g>",
        true,
        None,
    );
    assert!(fresh_succeeded);
}

#[test]
fn turbulence_extreme_seed_and_stitch_growth_preserve_output_without_integer_panics() {
    for body in [
        "<defs><filter id='f'><feTurbulence baseFrequency='.03' numOctaves='3' seed='-2147483648'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>",
        "<defs><filter id='f'><feTurbulence baseFrequency='.01 .03' numOctaves='40' seed='7' stitchTiles='stitch'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>",
    ] {
        let (compatibility, compatibility_ok, _) = render_document(body, false, None);
        let (controlled, controlled_ok, _) = render_document(body, true, None);
        assert!(compatibility_ok && controlled_ok);
        assert_eq!(controlled, compatibility);
        assert!(controlled.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}

#[test]
fn astronomical_blur_keeps_transparent_boundary_normalization_without_integer_overflow() {
    for deviation in ["1e20", "1e20 0", "0 1e20", "3e38"] {
        let body = format!(
            "<defs><filter id='f'><feGaussianBlur stdDeviation='{deviation}'/></filter></defs><rect x='16' y='16' width='32' height='32' fill='red' filter='url(#f)'/>"
        );
        let (compatibility, compatibility_ok, _) = render_document(&body, false, None);
        let (controlled, controlled_ok, _) = render_document(&body, true, None);
        assert!(compatibility_ok && controlled_ok);
        assert_eq!(controlled, compatibility);
        // Each nonzero axis has denominator > 510 * 64, so even an all-255
        // line rounds to zero with the original transparent edge convention.
        assert!(controlled.iter().all(|channel| *channel == 0));
    }
}

#[test]
fn second_primitive_semantic_error_precedes_failing_paid_result_cleanup() {
    let (resources, state) = request(8 << 20);
    let mut options = usvg::Options::default();
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><defs><filter id='f' filterUnits='userSpaceOnUse' primitiveUnits='userSpaceOnUse' x='0' y='0' width='64' height='64'><feFlood flood-color='red' result='paid'/><feFlood x='1e38' y='0' width='1e38' height='64'/></filter></defs><rect width='64' height='64' filter='url(#f)'/></svg>";
    let tree = usvg::Tree::from_data_controlled(source, &options).unwrap();
    fn filter(group: &usvg::Group) -> Option<&usvg::filter::Filter> {
        if let Some(value) = group.filters().first() {
            return Some(value);
        }
        group.children().iter().find_map(|node| match node {
            usvg::Node::Group(group) => filter(group),
            _ => None,
        })
    }
    let filter = filter(tree.root()).unwrap();
    assert_eq!(filter.primitives().len(), 2);
    assert!(filter.primitives()[1].rect().x() > 1e37);
    let mut pixels = Raster::try_create(64, 64, Some(&resources)).unwrap();
    let context = crate::render::Context {
        max_bbox: tiny_skia::IntRect::from_xywh(0, 0, 64, 64).unwrap(),
        processing: tree.processing_context().clone(),
        coverage: None,
    };
    let allocations_before = state.next.load(Ordering::SeqCst);
    // The first primitive's unique header transfer and clipping pipelines retire
    // before the second primitive. Reject its retained 64x64 RGBA raster instead
    // of relying on an ordinal that changes when real paid metadata is added.
    state
        .capture_ticket_bytes
        .store(64 * 64 * 4, Ordering::SeqCst);
    state.reject_captured_ticket.store(1, Ordering::SeqCst);
    state.trace_mode.store(1, Ordering::SeqCst);
    crate::filter::apply(
        filter,
        &context,
        tiny_skia::Transform::from_scale(2.0, 2.0),
        &mut pixels,
    );
    // First flood produced both paid pixels and shared header before the second
    // primitive failed; at least one physical owner then called failing cleanup.
    assert!(state.next.load(Ordering::SeqCst) >= allocations_before + 2);
    assert!(state.rejected_releases.load(Ordering::SeqCst) > 0);
    assert_eq!(state.capture_matches.load(Ordering::SeqCst), 1);
    assert_ne!(state.captured_ticket.load(Ordering::SeqCst), 0);
    assert_eq!(
        resources.failure().unwrap().kind,
        FailureKind::InvalidFilterRegion
    );
    assert_eq!(resources.failure().unwrap().native_hr, None);
    drop(pixels);
    drop(context);
    drop(tree);
    drop(options);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(resources.statistics().live_leases, 1);
    assert_eq!(resources.statistics().scratch_bytes, 64 * 64 * 4);
}
