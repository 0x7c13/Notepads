// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use base64::Engine;

const JPEG: &[u8] = include_bytes!("fixtures/upstream-progressive-dri-420-16x16.jpg");

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn body() -> String {
    format!(
        "<image href='{}' width='64' height='64' image-rendering='pixelated'/>",
        data_url("image/jpeg", JPEG)
    )
}

#[test]
fn actual_jpeg_image_nested_svg_and_feimage_match_compatibility_pixels() {
    let href = data_url("image/jpeg", JPEG);
    let nested = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{}</svg>",
        body()
    );
    for source in [
        body(),
        format!("<image href='{}' width='64' height='64'/>", data_url("image/svg+xml", nested.as_bytes())),
        format!("<defs><filter id='f' x='0' y='0' width='64' height='64' filterUnits='userSpaceOnUse'><feImage href='{href}' width='64' height='64'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>"),
    ] {
        let (expected, compatibility_ok, _) = render_document(&source, false, None);
        let (actual, controlled_ok, _) = render_document(&source, true, None);
        assert!(compatibility_ok && controlled_ok);
        assert!(actual.chunks_exact(4).any(|pixel| pixel[3] != 0));
        assert_eq!(actual, expected);
    }
}

#[test]
fn jpeg_target_refusal_keeps_native_error_and_a_fresh_request_succeeds() {
    let (resources, state) = request(8 << 20);
    state
        .reject_reserve_bytes
        .store(16 * 16 * 4, Ordering::SeqCst);
    let error = crate::raster_decode::decode_jpeg_controlled(JPEG, &resources).unwrap_err();
    assert_eq!(error, Error::native(-778));
    resources.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    let (fresh, _) = request(8 << 20);
    let image = crate::raster_decode::decode_jpeg_controlled(JPEG, &fresh).unwrap();
    assert_eq!((image.width(), image.height()), (16, 16));
    assert!(image.data().chunks_exact(4).all(|pixel| pixel[3] == 255));
    drop(image);
    fresh.validate_retired().unwrap();
}

#[test]
fn jpeg_decoder_last_release_failure_cannot_return_a_ready_raster() {
    let (resources, state) = request(8 << 20);
    let image = crate::raster_decode::decode_jpeg_controlled(JPEG, &resources).unwrap();
    let decoder_releases = state.release_calls.load(Ordering::SeqCst);
    assert!(decoder_releases > 0);
    drop(image);
    resources.validate_retired().unwrap();

    let (resources, state) = request(8 << 20);
    state
        .reject_release_after
        .store(decoder_releases - 1, Ordering::SeqCst);
    let error = crate::raster_decode::decode_jpeg_controlled(JPEG, &resources).unwrap_err();
    assert_eq!(error, Error::native(-777));
    assert!(state.rejected_releases.load(Ordering::SeqCst) > 0);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert!(resources.validate_retired().is_err());
}

#[test]
fn jpeg_dimension_gate_rejects_before_target_or_coefficient_buffers() {
    let mut encoded = JPEG.to_vec();
    let sof = encoded
        .windows(2)
        .position(|bytes| bytes == [0xff, 0xc2])
        .unwrap();
    encoded[sof + 5..sof + 7].copy_from_slice(&4096u16.to_be_bytes());
    encoded[sof + 7..sof + 9].copy_from_slice(&4097u16.to_be_bytes());
    let (resources, state) = request(8 << 20);
    let error = crate::raster_decode::decode_jpeg_controlled(&encoded, &resources).unwrap_err();
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::Capacity));
    assert!(state.peak.load(Ordering::SeqCst) < 1 << 20);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn jpeg_bridge_cancellation_sweep_never_publishes_and_retires_allocations() {
    let (resources, state) = request(8 << 20);
    let image = crate::raster_decode::decode_jpeg_controlled(JPEG, &resources).unwrap();
    let checkpoints = state.calls.load(Ordering::SeqCst);
    drop(image);
    resources.validate_retired().unwrap();
    assert!(checkpoints > 5);
    for call in 0..checkpoints {
        let (resources, state) = request(8 << 20);
        state.cancel_at.store(call, Ordering::SeqCst);
        let error = crate::raster_decode::decode_jpeg_controlled(JPEG, &resources).unwrap_err();
        assert_eq!(error.kind, FailureKind::Cancelled);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn invalid_jpeg_records_image_decode_and_retires_storage() {
    let (resources, state) = request(8 << 20);
    let error = crate::raster_decode::decode_jpeg_controlled(&JPEG[..100], &resources).unwrap_err();
    assert_eq!(error.kind, FailureKind::ImageDecode);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}
