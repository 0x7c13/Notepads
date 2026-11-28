// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Product wrapper regressions; reuse the crate's existing test allocator.
use super::{Options, source_kind};
use crate::geometry_test_support::{AllocationObservation, Call, native, request};
use markdown_svg_budget::{FailureKind, Limits, ResourcesRef};
use std::{
    io::{Read, Write},
    sync::Arc,
};

fn options(resources: &ResourcesRef) -> Options<'static> {
    Options {
        processing: Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
        ..Options::default()
    }
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}
fn rich_header(base: &[u8], field: usize) -> Vec<u8> {
    assert_eq!(base[3], 0);
    let mut bytes = base[..10].to_vec();
    bytes[3] = 2 | 4 | 8 | 16;
    bytes.extend((field.min(u16::MAX as usize) as u16).to_le_bytes());
    bytes.extend(std::iter::repeat_n(b'E', field.min(u16::MAX as usize)));
    bytes.extend(std::iter::repeat_n(b'N', field));
    bytes.push(0);
    bytes.extend(std::iter::repeat_n(b'C', field));
    bytes.push(0);
    let mut crc = flate2::Crc::new();
    crc.update(&bytes);
    bytes.extend((crc.sum() as u16).to_le_bytes());
    bytes.extend(&base[10..]);
    bytes
}

#[test]
fn actual_font_gzip_wrapper_retains_only_paid_state_and_output_for_long_headers() {
    let document = b"<svg xmlns='http://www.w3.org/2000/svg'><g id='glyph2'/></svg>";
    let plain = gzip(document);
    for bytes in [plain.clone(), rich_header(&plain, 65535)] {
        let (r, state) = request(Limits::default());
        let opt = options(&r);
        let observed = AllocationObservation::begin();
        let decoded = source_kind::decode_font_gzip(&bytes, &opt).unwrap();
        assert_eq!(decoded.as_slice(), document);
        drop(decoded);
        let allocations = observed.finish();
        assert_eq!(allocations.alloc + allocations.zeroed, 2);
        assert_eq!(allocations.realloc, 0);
        assert_eq!(allocations.dealloc, 2);
        let counts = state.counts();
        assert_eq!((counts.reserve, counts.commit, counts.release), (2, 2, 2));
        assert_eq!(state.live_len(), 0);
        assert_eq!(r.statistics().scratch_bytes, 0);
    }
}

#[test]
fn first_actual_header_rejection_performs_no_decoder_allocation_or_reservation() {
    let bytes = rich_header(&gzip(b"hello"), 65535);
    let (r, state) = request(Limits::default());
    let opt = options(&r);
    let original = native(-19071);
    // decode entry has one zero-work check, followed by the first 10-byte header.
    state.arm(Call::Checkpoint, 2, original);
    let observed = AllocationObservation::begin();
    let result = source_kind::decode_font_gzip(&bytes, &opt);
    let allocations = observed.finish();
    assert!(result.is_err());
    assert_eq!(r.failure(), Some(original));
    assert_eq!(
        allocations.alloc + allocations.zeroed + allocations.realloc,
        0
    );
    assert_eq!(state.counts().reserve, 0);
    assert_eq!(state.counts().commit, 0);
}

#[test]
fn every_wrapper_boundary_retains_original_fault_and_stops_later_work() {
    let bytes = rich_header(&gzip(b"<svg><g id='glyph2'/></svg>"), 17);
    let (r, baseline) = request(Limits::default());
    let opt = options(&r);
    let decoded = source_kind::decode_font_gzip(&bytes, &opt).unwrap();
    let internal_releases = baseline.counts().release;
    drop(decoded);
    let count = baseline.counts();
    assert_eq!(
        count.release,
        internal_releases + 1,
        "one caller-owned output"
    );
    for (call, total) in [
        (Call::Reserve, count.reserve),
        (Call::Commit, count.commit),
        (Call::Checkpoint, count.checkpoint),
        (Call::Release, count.release),
    ] {
        for ordinal in 1..=total {
            let (r, state) = request(Limits::default());
            let opt = options(&r);
            let original = native(-19071);
            state.arm(call, ordinal, original);
            let result = source_kind::decode_font_gzip(&bytes, &opt);
            if call == Call::Release && ordinal > internal_releases {
                // The output's ownership was successfully transferred. Its later
                // caller-side refund cannot retroactively change decode's return.
                let output = result.expect("fault belongs to caller-owned output retirement");
                assert_eq!(r.failure(), None);
                drop(output);
                assert_eq!(r.checkpoint(0), Err(original));
            } else {
                assert!(result.is_err(), "{call:?}/{ordinal}");
                drop(result);
            }
            assert_eq!(r.failure(), Some(original));
            let ledger = state.lock();
            let at = ledger
                .events
                .iter()
                .position(|event| event.call == call && event.ordinal == ordinal)
                .unwrap();
            assert!(
                ledger.events[at + 1..]
                    .iter()
                    .all(|event| event.call == Call::Release)
            );
        }
    }
}

#[test]
fn wrapper_framing_matches_previous_flate2_multi_member_contract() {
    let base = gzip(b"<svg><g id='glyph2'/></svg>");
    let mut cases = vec![
        base.clone(),
        rich_header(&base, 0),
        rich_header(&base, 65535),
        rich_header(&base, 65536),
    ];
    cases.extend((0..base.len()).map(|end| base[..end].to_vec()));
    for at in base.len() - 8..base.len() {
        let mut bytes = base.clone();
        bytes[at] ^= 1;
        cases.push(bytes);
    }
    for flag in 0..=255 {
        let mut bytes = base.clone();
        bytes[3] = flag;
        cases.push(bytes);
    }
    let mut multi = gzip(b"");
    multi.extend(&base);
    multi.extend(gzip(b""));
    cases.push(multi.clone());
    multi.push(0);
    cases.push(multi);
    for bytes in cases {
        let mut reference = Vec::new();
        let expected =
            flate2::bufread::MultiGzDecoder::new(bytes.as_slice()).read_to_end(&mut reference);
        let (r, state) = request(Limits::default());
        let opt = options(&r);
        let actual = source_kind::decode_font_gzip(&bytes, &opt);
        match (expected, &actual) {
            (Ok(_), Ok(bytes)) => assert_eq!(reference.as_slice(), bytes.as_slice()),
            (Err(_), Err(_)) => assert_eq!(r.failure().unwrap().kind, FailureKind::Decode),
            (expected, actual) => panic!("{expected:?} vs {actual:?}"),
        }
        drop(actual);
        assert_eq!(state.live_len(), 0);
        assert_eq!(r.statistics().scratch_bytes, 0);
    }
}
