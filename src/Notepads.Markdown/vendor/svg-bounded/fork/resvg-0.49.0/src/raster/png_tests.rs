// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use base64::Engine;

fn encoded_png(width: u32, height: u32) -> Vec<u8> {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|i| [i as u8, 90, 200, 128])
        .collect();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    output
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn body(encoded: &[u8]) -> String {
    format!(
        "<image href='{}' width='64' height='64' image-rendering='pixelated'/>",
        data_url("image/png", encoded)
    )
}

fn parsed(encoded: &[u8]) -> (usvg::Tree, ResourcesRef, Arc<State>) {
    let (resources, state) = request(8 << 20);
    let mut options = usvg::Options::default();
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{}</svg>",
        body(encoded)
    );
    let tree = usvg::Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
    (tree, resources, state)
}

#[test]
fn actual_png_svg_nested_svg_and_feimage_match_compatibility_pixels() {
    let encoded = encoded_png(17, 19);
    let href = data_url("image/png", &encoded);
    let nested = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{}</svg>",
        body(&encoded)
    );
    let cases = [
        body(&encoded),
        format!(
            "<image href='{}' width='64' height='64'/>",
            data_url("image/svg+xml", nested.as_bytes())
        ),
        format!(
            "<defs><filter id='f' x='0' y='0' width='64' height='64' filterUnits='userSpaceOnUse'><feImage href='{href}' x='0' y='0' width='64' height='64'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>"
        ),
    ];
    for (index, source) in cases.iter().enumerate() {
        let (expected, compat_ok, _) = render_document(source, false, None);
        let (actual, controlled_ok, _) = render_document(source, true, None);
        assert!(compat_ok && controlled_ok, "case {index}");
        assert!(actual.iter().any(|value| *value != 0), "case {index}");
        assert_eq!(actual, expected, "case {index}");
    }
}

#[test]
fn actual_renderer_rejects_missing_iend_in_both_entry_paths() {
    let mut encoded = encoded_png(17, 19);
    assert_eq!(&encoded[encoded.len() - 8..encoded.len() - 4], b"IEND");
    encoded.truncate(encoded.len() - 12);
    let (_, compatibility_ok, _) = render_document(&body(&encoded), false, None);
    let (_, controlled_ok, _) = render_document(&body(&encoded), true, None);
    assert!(!compatibility_ok);
    assert!(!controlled_ok);
}

#[test]
fn controlled_renderer_rejects_bad_adler_without_changing_compatibility() {
    let mut encoded = encoded_png(17, 19);
    let mut offset = 8;
    let mut changed = false;
    while offset < encoded.len() {
        let length = u32::from_be_bytes(encoded[offset..offset + 4].try_into().unwrap()) as usize;
        if &encoded[offset + 4..offset + 8] == b"IDAT" {
            encoded[offset + 8 + length - 1] ^= 1;
            let mut crc = !0u32;
            for byte in &encoded[offset + 4..offset + 8 + length] {
                crc ^= *byte as u32;
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
                }
            }
            encoded[offset + 8 + length..offset + 12 + length]
                .copy_from_slice(&(!crc).to_be_bytes());
            changed = true;
            break;
        }
        offset += length + 12;
    }
    assert!(changed);
    let (_, compatibility_ok, _) = render_document(&body(&encoded), false, None);
    let (_, controlled_ok, _) = render_document(&body(&encoded), true, None);
    assert!(compatibility_ok);
    assert!(!controlled_ok);
}

#[test]
fn image_kind_entry_rejects_oversized_ihdr_before_raster_allocation() {
    let mut encoded = Vec::new();
    let encoder = png::Encoder::new(&mut encoded, 4097, 4096);
    drop(encoder.write_header().unwrap());
    let image = usvg::ImageKind::PNG(Arc::new(encoded).into());
    let (resources, state) = request(8 << 20);
    let ctx = crate::render::Context {
        max_bbox: crate::max_filter_bbox(64, 64),
        processing: Arc::new(usvg::ProcessingContext::with_resources(resources.clone())),
        coverage: None,
    };
    let mut target = tiny_skia::Pixmap::new(64, 64).unwrap();
    crate::image::render_inner(
        &image,
        &ctx,
        tiny_skia::Transform::identity(),
        usvg::ImageRendering::Pixelated,
        &mut target.as_mut(),
    );
    let error = ctx.processing.checked().unwrap_err();
    assert_eq!(error.typed_failure.kind, FailureKind::Quota);
    assert_eq!(error.typed_failure.limit, LimitKind::Capacity);
    assert!(state.peak.load(Ordering::SeqCst) < 1 << 20);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    drop(ctx);
    resources.validate_retired().unwrap();
}

#[test]
fn renderer_keeps_native_target_reserve_failure_and_fresh_request_recovers() {
    let encoded = encoded_png(17, 19);
    let (tree, resources, state) = parsed(&encoded);
    state
        .reject_reserve_bytes
        .store(17 * 19 * 4, Ordering::SeqCst);
    let mut target = tiny_skia::Pixmap::new(64, 64).unwrap();
    let error = crate::render_in_request(
        &tree,
        tiny_skia::Transform::identity(),
        &mut target.as_mut(),
    )
    .unwrap_err();
    assert_eq!(error.typed_failure, Error::native(-778));
    drop(tree);
    resources.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    let (_, fresh_ok, _) = render_document(&body(&encoded), true, None);
    assert!(fresh_ok);
}

#[test]
fn renderer_sees_native_paid_target_release_failure_before_success() {
    let (tree, resources, state) = parsed(&encoded_png(17, 19));
    state
        .reject_release_bytes
        .store(17 * 19 * 4, Ordering::SeqCst);
    let mut target = tiny_skia::Pixmap::new(64, 64).unwrap();
    let error = crate::render_in_request(
        &tree,
        tiny_skia::Transform::identity(),
        &mut target.as_mut(),
    )
    .unwrap_err();
    assert_eq!(error.typed_failure, Error::native(-777));
    assert_eq!(state.rejected_releases.load(Ordering::SeqCst), 1);
    assert!(target.data().iter().any(|value| *value != 0));
    drop(tree);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert!(resources.validate_retired().is_err());
    assert_eq!(resources.statistics().release_failures, 1);
}

#[test]
fn renderer_cancel_sweep_never_turns_a_png_failure_into_success() {
    let encoded = encoded_png(17, 19);
    let (tree, resources, state) = parsed(&encoded);
    let start = state.calls.load(Ordering::SeqCst);
    let mut target = tiny_skia::Pixmap::new(64, 64).unwrap();
    crate::render_in_request(
        &tree,
        tiny_skia::Transform::identity(),
        &mut target.as_mut(),
    )
    .unwrap();
    let count = state.calls.load(Ordering::SeqCst) - start;
    drop(tree);
    resources.validate_retired().unwrap();
    assert!(count > 5);
    let mut after_actual_reserve = 0;
    for offset in 0..count {
        let (tree, resources, state) = parsed(&encoded);
        let before = state.next.load(Ordering::SeqCst);
        state.cancel_at.store(
            state.calls.load(Ordering::SeqCst) + offset,
            Ordering::SeqCst,
        );
        let error = crate::render_in_request(
            &tree,
            tiny_skia::Transform::identity(),
            &mut target.as_mut(),
        )
        .unwrap_err();
        assert_eq!(error.typed_failure.kind, FailureKind::Cancelled);
        after_actual_reserve += usize::from(state.next.load(Ordering::SeqCst) > before);
        drop(tree);
        resources.validate_retired().unwrap();
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
    }
    assert!(after_actual_reserve > 0);
}
