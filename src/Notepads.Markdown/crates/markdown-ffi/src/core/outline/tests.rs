// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_core::config::Extensions;
use markdown_core::source::SourceDocument;

struct Fixture {
    cancel: *mut Owned<CoreCancel>,
    model: *mut Owned<CoreModel>,
    outline: *mut Owned<CoreOutline>,
}

impl Fixture {
    fn new(text: &str) -> Self {
        Self::with_extensions(text, Extensions::NONE)
    }

    fn with_extensions(text: &str, extensions: Extensions) -> Self {
        let mut cancel = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: distinct initialized PODs and every handle is owned by this fixture.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_cancel_create(&mut cancel, &mut error),
                0
            );
            let mut op = (*cancel).value.budget.begin((*cancel).value.token.clone());
            let document = SourceDocument::new(
                SourceKey::new([148; 16], 7).unwrap(),
                text.as_bytes(),
                &mut op,
            )
            .unwrap();
            let model = AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::with_extensions(Profile::G3, extensions),
                9,
                &mut op,
            )
            .unwrap();
            let slot = Slot::new(op.boundary_work()).unwrap();
            let model = slot.commit(CoreModel { model });
            let mut outline = ptr::null_mut();
            assert_eq!(
                notepads_markdown_outline_create(model, cancel, &mut outline, &mut error),
                0
            );
            Self {
                cancel,
                model,
                outline,
            }
        }
    }

    fn summary(&mut self) -> OutlineSummary {
        let mut output = OutlineSummary {
            header: WireHeader {
                abi_version: 1,
                struct_size: size_of::<OutlineSummary>() as u32,
            },
            ..Default::default()
        };
        let mut error = WireError::default();
        // SAFETY: exclusive live handle and independent writable initialized outputs.
        unsafe {
            assert_eq!(
                notepads_markdown_outline_get_summary(self.outline, &mut output, &mut error),
                0
            )
        };
        output
    }

    fn read<T: Copy + Default>(&mut self, table: u32, start: u64, count: usize) -> Vec<T> {
        let mut data = vec![T::default(); count];
        let mut result = WireReadResult {
            header: WireHeader::of::<WireReadResult>(),
            ..Default::default()
        };
        let mut error = WireError::default();
        // SAFETY: correctly sized/aligned independent output array and initialized PODs.
        unsafe {
            assert_eq!(
                notepads_markdown_outline_read(
                    self.outline,
                    table,
                    start,
                    count as u64,
                    size_of::<T>() as u32,
                    data.as_mut_ptr().cast(),
                    &mut result,
                    &mut error
                ),
                0
            );
        }
        assert_eq!(
            (result.written, result.next),
            (count as u64, start + count as u64)
        );
        data
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: all borrows have ended; each still-live handle is uniquely destroyed once.
        unsafe {
            assert_eq!(notepads_markdown_outline_destroy(self.outline), 0);
            assert_eq!(notepads_markdown_core_model_destroy(self.model), 0);
            assert_eq!(notepads_markdown_core_cancel_destroy(self.cancel), 0);
        }
    }
}

#[test]
fn authored_fragments_share_the_outline_blob_and_keep_logical_positions() {
    let mut f = Fixture::with_extensions(
        "# Main {#Mixed}\r\n\r\n<a name='old'></a>\r\n\r\n<p id='中文&amp;x'>Body</p>\r\n\r\n<hr id='rule'>\r\n",
        Extensions::ALL,
    );
    let summary = f.summary();
    assert_eq!((summary.headings, summary.anchors), (1, 3));
    let heading = f.read::<OutlineHeading>(1, 0, 1)[0];
    let anchors = f.read::<OutlineAnchor>(3, 0, 3);
    let bytes = f.read::<u8>(2, 0, summary.blob_bytes as usize);
    let text = |slice: OutlineBlob| {
        std::str::from_utf8(&bytes[slice.start as usize..(slice.start + slice.length) as usize])
            .unwrap()
    };
    assert_eq!(
        (text(heading.title), text(heading.anchor)),
        ("Main", "Mixed")
    );
    assert_eq!((text(anchors[0].name), anchors[0].legacy), ("old", 1));
    assert_eq!(
        (
            text(anchors[1].name),
            anchors[1].legacy,
            anchors[1].position
        ),
        ("中文&x", 0, 5)
    );
    assert_eq!(text(anchors[2].name), "rule");
    assert_ne!(anchors[2].node, 0);
}

#[test]
fn exact_layout_and_upstream_global_suffix_expected() {
    assert_eq!(notepads_markdown_outline_abi_version(), 1);
    assert_eq!(
        (size_of::<OutlineSummary>(), size_of::<OutlineHeading>()),
        (72, 56)
    );
    assert_eq!(std::mem::offset_of!(OutlineHeading, title), 24);
    let mut f = Fixture::new("# a\n# a\n# a-1\n# a\n");
    let summary = f.summary();
    assert_eq!(
        (
            summary.headings,
            summary.model.revision,
            summary.model.generation
        ),
        (4, 7, 9)
    );
    let rows = f.read::<OutlineHeading>(1, 0, 4);
    let bytes = f.read::<u8>(2, 0, summary.blob_bytes as usize);
    let value = |s: OutlineBlob| {
        std::str::from_utf8(&bytes[s.start as usize..(s.start + s.length) as usize]).unwrap()
    };
    assert_eq!(
        rows.iter().map(|r| value(r.title)).collect::<Vec<_>>(),
        ["a", "a", "a-1", "a"]
    );
    assert_eq!(
        rows.iter().map(|r| value(r.anchor)).collect::<Vec<_>>(),
        ["a", "a-1", "a-1-1", "a-2"]
    );
    assert_eq!(
        rows.iter()
            .map(|r| (r.level, r.logical_start, r.logical_end))
            .collect::<Vec<_>>(),
        [(1, 0, 1), (1, 2, 3), (1, 4, 7), (1, 8, 9)]
    );
    assert!(rows.windows(2).all(|w| w[0].node != w[1].node));
}

#[test]
fn row_pages_and_byte_pages_preserve_scalar_fragments() {
    let mut f = Fixture::new(&"# 中😀\n".repeat(300));
    let summary = f.summary();
    assert_eq!(summary.headings, 300);
    let mut rows = f.read::<OutlineHeading>(1, 0, 256);
    rows.extend(f.read::<OutlineHeading>(1, 256, 44));
    let mut bytes = Vec::new();
    for start in 0..summary.blob_bytes {
        bytes.extend(f.read::<u8>(2, start, 1));
    }
    for (index, row) in rows.iter().enumerate() {
        let s = row.title;
        assert_eq!(
            std::str::from_utf8(&bytes[s.start as usize..(s.start + s.length) as usize]).unwrap(),
            "中😀"
        );
        let s = row.anchor;
        let expected = if index == 0 {
            "中".to_string()
        } else {
            format!("中-{index}")
        };
        assert_eq!(
            std::str::from_utf8(&bytes[s.start as usize..(s.start + s.length) as usize]).unwrap(),
            expected
        );
    }
}

#[test]
fn empty_outline_and_zero_count_null_output_buffer() {
    let mut f = Fixture::new("body");
    let summary = f.summary();
    assert_eq!(summary.headings, 0);
    let mut read = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        ..Default::default()
    };
    let mut error = WireError::default();
    // SAFETY: count0 explicitly admits a null buffer and all PODs are independent.
    unsafe {
        assert_eq!(
            notepads_markdown_outline_read(
                f.outline,
                1,
                0,
                0,
                56,
                ptr::null_mut(),
                &mut read,
                &mut error
            ),
            0
        );
    }
    assert_eq!((read.written, read.next, read.done), (0, 0, 1));
}

#[test]
fn wrong_headers_leave_initialized_outputs_unchanged() {
    let f = Fixture::new("# a");
    let mut summary = OutlineSummary {
        header: WireHeader {
            abi_version: 2,
            struct_size: size_of::<OutlineSummary>() as u32,
        },
        headings: 73,
        ..Default::default()
    };
    let mut error = WireError::default();
    // SAFETY: a readable initialized prefix permits rejecting before the full output write.
    unsafe {
        assert_eq!(
            notepads_markdown_outline_get_summary(f.outline, &mut summary, &mut error),
            STATUS_ABI_MISMATCH
        );
    }
    assert_eq!(summary.headings, 73);
}

#[test]
fn range_failure_latches_across_later_correct_reads() {
    let f = Fixture::new("# a");
    let mut row = OutlineHeading::default();
    let mut read = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        written: 73,
        ..Default::default()
    };
    let mut error = WireError::default();
    // SAFETY: valid independent storage; this exercises semantic range rejection only.
    unsafe {
        let failure = wire_error(CoreError::InvalidRange).status;
        assert_eq!(
            notepads_markdown_outline_read(
                f.outline,
                1,
                1,
                1,
                56,
                (&mut row as *mut OutlineHeading).cast(),
                &mut read,
                &mut error
            ),
            failure
        );
        assert_eq!(read.written, 73);
        let mut summary = OutlineSummary {
            header: WireHeader {
                abi_version: 1,
                struct_size: size_of::<OutlineSummary>() as u32,
            },
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_outline_get_summary(f.outline, &mut summary, &mut error),
            failure
        );
        assert_eq!(summary.headings, 0);
    }
}

#[test]
fn foreign_root_is_rejected_without_stealing_either_handle() {
    let mut first = Fixture::new("# first");
    let second = Fixture::new("# second");
    let mut output = first.outline;
    let mut error = WireError::default();
    // SAFETY: both real handles remain live, but intentionally name different roots.
    unsafe {
        assert_eq!(
            notepads_markdown_outline_create(first.model, second.cancel, &mut output, &mut error),
            wire_error(CoreError::WrongBudget).status
        );
    }
    assert!(output.is_null());
    assert_eq!(first.summary().headings, 1);
}

#[test]
fn retained_directory_owns_model_and_cancel_token() {
    let mut f = Fixture::new("# retained");
    // SAFETY: retire only the external handles, leaving the directory's owned pins.
    unsafe {
        assert_eq!(notepads_markdown_core_model_destroy(f.model), 0);
        f.model = ptr::null_mut();
        assert_eq!(notepads_markdown_core_cancel_destroy(f.cancel), 0);
        f.cancel = ptr::null_mut();
    }
    assert_eq!(f.summary().headings, 1);
    assert_eq!(f.read::<OutlineHeading>(1, 0, 1)[0].level, 1);
}

#[test]
fn cancellation_does_not_restart_the_original_operation() {
    let f = Fixture::new("# a");
    let mut error = WireError::default();
    let mut summary = OutlineSummary {
        header: WireHeader {
            abi_version: 1,
            struct_size: size_of::<OutlineSummary>() as u32,
        },
        headings: 73,
        ..Default::default()
    };
    // SAFETY: live cancel and directory handles, sequential calls and independent PODs.
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        let expected = wire_error(CoreError::Cancelled).status;
        assert_eq!(
            notepads_markdown_outline_get_summary(f.outline, &mut summary, &mut error),
            expected
        );
        assert_eq!(
            notepads_markdown_outline_get_summary(f.outline, &mut summary, &mut error),
            expected
        );
    }
    assert_eq!(summary.headings, 73);
}

#[test]
fn panic_is_contained_and_directory_is_poisoned() {
    let f = Fixture::new("# a");
    let mut error = WireError::default();
    // SAFETY: private test invokes the real containment gate with a valid exclusive handle.
    unsafe {
        assert_eq!(
            outline_action(f.outline, &mut error, |_| panic!("outline test fault")),
            STATUS_PANIC
        );
        let mut summary = OutlineSummary {
            header: WireHeader {
                abi_version: 1,
                struct_size: size_of::<OutlineSummary>() as u32,
            },
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_outline_get_summary(f.outline, &mut summary, &mut error),
            wire_error(CoreError::InvalidState).status
        );
    }
}
