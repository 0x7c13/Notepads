// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real renderer integration with fixed upstream and animation-policy inputs.
use super::*;
use base64::Engine;

const GIF: &[u8] = include_bytes!("fixtures/upstream-2x2.gif");
const NO_TRAILER: &[u8] = include_bytes!("fixtures/gif-d02-no-trailer.gif");
const LOOP: &[u8] = include_bytes!("fixtures/gif-d02-netscape-after-first.gif");
const SECOND_FRAME: &[u8] = include_bytes!("fixtures/gif-d02-second-metadata-no-payload.gif");
const INVALID_LZW: &[u8] = include_bytes!("fixtures/gif-d02-invalid-first-lzw.gif");
// Original OLD G007 straight RGBA, raw.rgba[512..528), SHA F38D5229... .
// Opaque pixels already equal the original f64 premultiplied representation.
const ORIGINAL_RGBA: [u8; 16] = [
    0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
];

fn data_url(mime: &str, encoded: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(encoded)
    )
}
fn image(encoded: &[u8]) -> String {
    format!(
        "<image href='{}' width='64' height='64' image-rendering='pixelated'/>",
        data_url("image/gif", encoded)
    )
}

#[test]
fn actual_gif_image_nested_svg_and_feimage_match_nonempty_compatibility_pixels() {
    let href = data_url("image/gif", GIF);
    let nested = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{}</svg>",
        image(GIF)
    );
    for source in [
        image(GIF),
        format!(
            "<image href='{}' width='64' height='64'/>",
            data_url("image/svg+xml", nested.as_bytes())
        ),
        format!(
            "<defs><filter id='f' x='0' y='0' width='64' height='64' filterUnits='userSpaceOnUse'><feImage href='{href}' width='64' height='64'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>"
        ),
    ] {
        let (expected, compat_ok, _) = render_document(&source, false, None);
        let (actual, controlled_ok, _) = render_document(&source, true, None);
        assert!(compat_ok && controlled_ok);
        assert!(actual.chunks_exact(4).any(|pixel| pixel[3] != 0));
        assert_eq!(actual, expected);
    }
}

#[test]
fn gif_bridge_preserves_first_frame_pixels_dimensions_offsets_and_binary_alpha_premultiply() {
    for encoded in [GIF, NO_TRAILER] {
        let (resources, state) = request(8 << 20);
        let raster = crate::raster_decode::decode_gif_controlled(encoded, &resources).unwrap();
        assert_eq!((raster.width(), raster.height()), (2, 2));
        assert_eq!(raster.data(), ORIGINAL_RGBA);
        drop(raster);
        resources.validate_retired().unwrap();
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
    }
    // Explicit header-only derivative of the fixed parent: logical screen10x12,
    // first frame2x2 at(3,5), GCE transparent palette index0 (white). LZW bytes
    // are unchanged. This must not create a10x12 compositor canvas.
    let mut offset_alpha = GIF.to_vec();
    offset_alpha[6..10].copy_from_slice(&[10, 0, 12, 0]);
    offset_alpha[22] = 5;
    offset_alpha[25] = 0;
    offset_alpha[28..32].copy_from_slice(&[3, 0, 5, 0]);
    let (metadata_resources, _) = request(8 << 20);
    let mut decoder =
        gif::controlled::ControlledDecoder::new(&offset_alpha, metadata_resources.clone()).unwrap();
    let info = decoder.read_info().unwrap();
    assert_eq!((info.width, info.height, info.left, info.top), (2, 2, 3, 5));
    drop(decoder);
    metadata_resources.validate_retired().unwrap();
    let (resources, state) = request(8 << 20);
    let raster = crate::raster_decode::decode_gif_controlled(&offset_alpha, &resources).unwrap();
    assert_eq!((raster.width(), raster.height()), (2, 2));
    assert_eq!(
        raster.data(),
        &[0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255]
    );
    assert!(raster
        .data()
        .chunks_exact(4)
        .all(|pixel| pixel[3] != 0 || pixel[..3] == [0, 0, 0]));
    drop(raster);
    resources.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}

#[test]
fn gif_loop_and_complete_second_metadata_are_typed_unsupported_before_pixel_success() {
    for encoded in [LOOP, SECOND_FRAME] {
        let (resources, state) = request(8 << 20);
        // If a16B output target were attempted before the animation decision,
        // the original Native refusal would win instead of Unsupported.
        state.reject_reserve_bytes.store(16, Ordering::SeqCst);
        let error = crate::raster_decode::decode_gif_controlled(encoded, &resources).unwrap_err();
        assert_eq!(error, Error::new(FailureKind::Unsupported, LimitKind::None));
        assert_eq!(resources.failure(), Some(error));
        resources.validate_retired().unwrap();
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        let (_, compat_ok, _) = render_document(&image(encoded), false, None);
        let (pixels, controlled_ok, _) = render_document(&image(encoded), true, None);
        assert!(compat_ok);
        assert!(!controlled_ok);
        assert!(pixels.iter().all(|byte| *byte == 0));
    }
}

#[test]
fn gif_dimension_gate_rejects_before_target_or_size_dependent_buffers() {
    let mut encoded = GIF.to_vec();
    encoded[32..34].copy_from_slice(&4097u16.to_le_bytes());
    encoded[34..36].copy_from_slice(&4096u16.to_le_bytes());
    let (resources, state) = request(8 << 20);
    let error = crate::raster_decode::decode_gif_controlled(&encoded, &resources).unwrap_err();
    // Required existing16Mi-pixel policy; Root first records the old50M-byte
    // MemoryLimit ordering failure, then implements the reviewed narrow fix.
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::Capacity));
    assert_eq!(resources.failure(), Some(error));
    // Fixed LZW dictionaries may exist; image-sized output/scratch must not.
    assert!(state.peak.load(Ordering::SeqCst) < 1 << 20);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn gif_native_target_refusal_preserves_first_error_and_a_fresh_request_succeeds() {
    let (resources, state) = request(8 << 20);
    state.reject_reserve_bytes.store(16, Ordering::SeqCst);
    let error = crate::raster_decode::decode_gif_controlled(GIF, &resources).unwrap_err();
    assert_eq!(error, Error::native(-778));
    assert_eq!(resources.failure(), Some(error));
    resources.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    let (fresh, _) = request(8 << 20);
    let raster = crate::raster_decode::decode_gif_controlled(GIF, &fresh).unwrap();
    assert_eq!(raster.data(), ORIGINAL_RGBA);
    drop(raster);
    fresh.validate_retired().unwrap();
}

#[test]
fn gif_late_release_rejects_ready_and_preserves_an_earlier_image_decode_error() {
    for (encoded, ordinary_first) in [(GIF, false), (INVALID_LZW, true)] {
        let (normal_resources, normal_state) = request(8 << 20);
        let normal = crate::raster_decode::decode_gif_controlled(encoded, &normal_resources);
        if ordinary_first {
            assert_eq!(normal.as_ref().unwrap_err().kind, FailureKind::ImageDecode);
        } else {
            assert!(normal.is_ok());
        }
        let decoder_releases = normal_state.release_calls.load(Ordering::SeqCst);
        assert!(decoder_releases > 0);
        drop(normal);
        normal_resources.validate_retired().unwrap();
        let (resources, state) = request(8 << 20);
        state
            .reject_release_after
            .store(decoder_releases - 1, Ordering::SeqCst);
        let error = crate::raster_decode::decode_gif_controlled(encoded, &resources).unwrap_err();
        let expected = if ordinary_first {
            Error::new(FailureKind::ImageDecode, LimitKind::None)
        } else {
            Error::native(-777)
        };
        assert_eq!(error, expected);
        assert_eq!(resources.failure(), Some(expected));
        assert!(state.rejected_releases.load(Ordering::SeqCst) > 0);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        // Parent backend retires its diagnostic counter before returning Err;
        // it is the real shared account that must reject a successful refund.
        assert!(resources.statistics().scratch_bytes > 0);
        assert!(resources.validate_retired().is_err());
    }
}

#[test]
fn gif_bridge_all_observed_checkpoint_cancellations_reject_and_retire() {
    let (resources, state) = request(8 << 20);
    let raster = crate::raster_decode::decode_gif_controlled(GIF, &resources).unwrap();
    let checkpoints = state.calls.load(Ordering::SeqCst);
    drop(raster);
    resources.validate_retired().unwrap();
    assert!(checkpoints > 5);
    let mut cancelled_after_reserve = 0;
    for call in 0..checkpoints {
        let (resources, state) = request(8 << 20);
        state.cancel_at.store(call, Ordering::SeqCst);
        let error = crate::raster_decode::decode_gif_controlled(GIF, &resources).unwrap_err();
        assert_eq!(error, Error::new(FailureKind::Cancelled, LimitKind::Work));
        assert_eq!(resources.failure(), Some(error));
        cancelled_after_reserve += usize::from(state.next.load(Ordering::SeqCst) > 0);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
    }
    assert!(cancelled_after_reserve > 0);
}
