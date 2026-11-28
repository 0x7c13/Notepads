// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_core::source::SourceDocument;
use std::mem::offset_of;

fn quote_summary(f: &Fixture) -> PreviewQuoteSummary {
    let mut summary = PreviewQuoteSummary {
        header: WireHeader::of::<PreviewQuoteSummary>(),
        ..Default::default()
    };
    // SAFETY: fixture uniquely owns a completed layout and independent PODs.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_quote_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!(summary.key, f.key);
    summary
}

fn read_quotes(f: &Fixture, start: u64, count: u64) -> Vec<PreviewQuote> {
    let mut rows = vec![PreviewQuote::default(); count as usize];
    let mut result = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        ..Default::default()
    };
    // SAFETY: writable aligned rows and independent result/error; exclusive layout.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_quote_read(
                f.layout,
                start,
                count,
                size_of::<PreviewQuote>() as u32,
                rows.as_mut_ptr(),
                &mut result,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!((result.written, result.next), (count, start + count));
    rows
}

#[test]
fn quote_geometry_keeps_item_gutter_and_complete_ancestor_envelopes_across_batches() {
    let f = Fixture::new("- > item quote\n  >\n  > > nested 中😀\n  >\n  > tail\n\n> sibling\n");
    let budget = f.budget.clone();
    let batch = f.batch(256);
    assert_eq!(batch.blocks, 4);
    let blocks = f.read::<PreviewBlock>(1, batch.blocks);
    let geometry = f.read::<PreviewBlockGeometry>(11, batch.blocks);
    for (block, extra) in blocks.iter().zip(&geometry) {
        assert_eq!((block.node, 0), (extra.node, extra.reserved));
        assert_eq!(extra.marker_x, 0.0);
    }
    assert_eq!(
        blocks.iter().map(|row| row.x).collect::<Vec<_>>(),
        [35.0, 50.0, 35.0, 15.0]
    );
    f.submit(&batch, &[10.0; 4]);
    f.finish();
    assert_eq!(quote_summary(&f).rows, 3);
    let before = read_quotes(&f, 0, 3);
    assert_eq!(
        before.iter().map(|row| row.depth).collect::<Vec<_>>(),
        [1, 2, 1]
    );
    assert_eq!(
        (before[0].x, before[0].width, before[0].y, before[0].height),
        (20.0, 380.0, 0.0, 50.0)
    );
    assert_eq!(
        (before[1].x, before[1].width, before[1].y, before[1].height),
        (35.0, 349.0, 17.0, 16.0)
    );
    assert_eq!(
        (before[2].x, before[2].y, before[2].height),
        (0.0, 54.0, 16.0)
    );
    assert!(before.iter().all(|row| row.border_width == 5.0));
    assert!(before[0].node != before[2].node);
    let mut current = PreviewSummary {
        header: prefix::<PreviewSummary>(),
        ..Default::default()
    };
    // SAFETY: independent current-batch result and live completed fixture layout.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_present(
                f.layout,
                3,
                1,
                &mut current,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!(current.blocks, 1);
    assert_eq!(read_quotes(&f, 0, 3), before);
    assert_eq!(quote_summary(&f).rows, 3);
    drop(f);
    released(&budget);
}

#[test]
fn draft_prefix_presents_exact_blocks_and_continues_the_same_layout() {
    let f = Fixture::new("> first\n>\n> > nested\n>\n> last\n\noutside");
    let first = f.batch(2);
    let mut extent = PreviewExtent {
        header: prefix::<PreviewExtent>(),
        ..Default::default()
    };
    let mut summary = PreviewSummary {
        header: prefix::<PreviewSummary>(),
        ..Default::default()
    };
    let mut error = WireError::default();
    // SAFETY: the fixture owns the exclusive layout; each initialized POD is independent.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_begin_prefix(f.layout, 12.0, &mut extent, &mut error),
            0
        );
        assert_eq!(extent.blocks, 0);
        f.submit(&first, &[10.0, 10.0]);
        assert_eq!(
            notepads_markdown_preview_begin_prefix(f.layout, 33.0, &mut extent, &mut error),
            0
        );
        assert_eq!(extent.blocks, 0);
        assert_eq!(
            notepads_markdown_preview_begin_prefix(f.layout, 12.0, &mut extent, &mut error),
            0
        );
        assert_eq!((extent.blocks, extent.height), (2, 33.0));
        assert_eq!(
            notepads_markdown_preview_present(f.layout, 0, 2, &mut summary, &mut error),
            0
        );
    }
    let prefix = f.read::<PreviewBlock>(1, 2);
    assert_eq!((prefix[1].y, prefix[1].height), (20.0, 10.0));
    assert_eq!(quote_summary(&f).rows, 2);
    let quotes = read_quotes(&f, 0, 2);
    assert_eq!(quotes[0].height, 33.0);
    let remainder = f.batch(2);
    assert_eq!((remainder.start, remainder.blocks), (2, 2));
    f.submit(&remainder, &[10.0, 10.0]);
    assert_eq!((f.finish().blocks, quote_summary(&f).rows), (4, 2));
    // SAFETY: the same initialized output and exclusive layout now hold final geometry.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_present(f.layout, 0, 2, &mut summary, &mut error),
            0
        );
    }
    let final_rows = f.read::<PreviewBlock>(1, 2);
    for (left, right) in prefix.iter().zip(final_rows.iter()) {
        assert_eq!(
            (left.node, left.y, left.height),
            (right.node, right.y, right.height)
        );
    }
    let final_quotes = read_quotes(&f, 0, 2);
    assert_eq!(final_quotes[0].height, 50.0);
    assert_eq!(final_quotes[1], quotes[1]);
}

#[test]
fn quote_directory_pages_more_than_256_rows_and_releases_all_storage() {
    let f = Fixture::new(&"> quote Ω😀\n\noutside\n\n".repeat(300));
    let budget = f.budget.clone();
    loop {
        let batch = f.batch(256);
        if batch.blocks == 0 {
            break;
        }
        f.submit(&batch, &vec![8.0; batch.blocks as usize]);
    }
    f.finish();
    assert_eq!(quote_summary(&f).rows, 300);
    let mut rows = read_quotes(&f, 0, 256);
    rows.extend(read_quotes(&f, 256, 44));
    assert_eq!(rows.len(), 300);
    assert!(
        rows.windows(2)
            .all(|pair| pair[0].node != pair[1].node && pair[0].y + pair[0].height < pair[1].y)
    );
    drop(f);
    released(&budget);
}

#[test]
fn quote_directory_checks_prefix_before_write_and_honors_original_cancellation() {
    let f = Fixture::new("> body\n");
    let batch = f.batch(1);
    f.submit(&batch, &[10.0]);
    f.finish();
    let mut summary = PreviewQuoteSummary {
        header: WireHeader {
            abi_version: 99,
            struct_size: size_of::<PreviewQuoteSummary>() as u32,
        },
        rows: 998,
        ..Default::default()
    };
    let untouched = summary;
    // SAFETY: valid initialized wrong-version prefix and exclusive fixture handle.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_quote_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            STATUS_ABI_MISMATCH
        );
    }
    assert_eq!(summary, untouched);
    assert_eq!(quote_summary(&f).rows, 1);
    // SAFETY: original cancellation flag remains live independently of this layout.
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
    }
    let mut rows = [PreviewQuote {
        node: 999,
        ..Default::default()
    }];
    let mut result = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        written: 777,
        ..Default::default()
    };
    // SAFETY: aligned independent writable storage; cancellation must prevent success writes.
    unsafe {
        assert_ne!(
            notepads_markdown_preview_quote_read(
                f.layout,
                0,
                1,
                size_of::<PreviewQuote>() as u32,
                rows.as_mut_ptr(),
                &mut result,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!((rows[0].node, result.written), (999, 777));
}

#[test]
fn yaml_front_matter_modes_keep_cell_headers_source_and_legacy_tables() {
    use markdown_core::config::Extensions;
    let source = "---\ntitle: 中😀\ncount: 2\n---\n\n|A|B|\n|---|---|\n|x|y|\n";
    for mode in [0x100, 0x300, 0x500] {
        let f = Fixture::with_extensions(source, Extensions::from_bits(mode).unwrap());
        let summary = f.batch(256);
        let blocks = f.read::<PreviewBlock>(1, summary.blocks);
        let cells = f.read::<PreviewCell>(4, summary.cells);
        let tables: Vec<_> = blocks.iter().filter(|block| block.kind == 6).collect();
        assert_eq!(tables.len(), if mode == 0x100 { 2 } else { 1 });
        if mode == 0x100 {
            let front = tables[0];
            assert_eq!(
                (front.columns, front.row_count, front.cells.count),
                (2, 2, 4)
            );
            let selected = &cells
                [front.cells.start as usize..(front.cells.start + front.cells.count) as usize];
            assert_eq!(
                selected.iter().map(|cell| cell.header).collect::<Vec<_>>(),
                [1, 0, 1, 0]
            );
        }
        let body = tables.last().unwrap();
        let selected =
            &cells[body.cells.start as usize..(body.cells.start + body.cells.count) as usize];
        assert_eq!(
            selected.iter().map(|cell| cell.header).collect::<Vec<_>>(),
            [1, 1, 0, 0]
        );
        assert_eq!(
            blocks.iter().filter(|block| block.kind == 3).count(),
            usize::from(mode == 0x300)
        );
        assert_eq!(summary.key.model.extensions, mode);
        // SAFETY: fixture exclusively owns this live model; the borrowed source
        // is inspected only during this assertion and never escapes the owner.
        unsafe {
            assert_eq!((*f.model).value.model.original_source().text(), source);
        }
    }
}

#[test]
fn script_shape_boundaries_cross_256_rows_preserve_runs_and_observe_cancellation() {
    use markdown_core::config::Extensions;
    let group = "<sup>**a**__\u{301}__ [b](#same)[\u{301}](#same) `c`</sup>\n\n**ffi**";
    let f = Fixture::with_extensions(
        &std::iter::repeat_n(group, 40)
            .collect::<Vec<_>>()
            .join("\n\n"),
        Extensions::HTML_INLINE,
    );
    let original = f.batch(256);
    let mut summary = PreviewScriptSummary {
        header: WireHeader {
            abi_version: PREVIEW_SCRIPT_ABI_VERSION,
            struct_size: size_of::<PreviewScriptSummary>() as u32,
        },
        ..Default::default()
    };
    // SAFETY: fixture owns the exclusive live layout and independent POD storage.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_script_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!((summary.rows, original.texts), (320, 80));
    assert_eq!(size_of::<PreviewScriptScope>(), 40);
    let mut scopes = Vec::new();
    for start in (0..summary.rows).step_by(256) {
        scopes.extend(f.read_from::<PreviewScriptScope>(10, start, 256.min(summary.rows - start)));
    }
    for text in 0..80 {
        let mut shapes = scopes
            .iter()
            .filter(|scope| scope.text == text && scope.font_flags & 32 != 0)
            .map(|scope| {
                assert_eq!(scope.font_flags, 32);
                assert_eq!(
                    (scope.size_scale, scope.relative_em, scope.absolute_dip),
                    (1.0, 0.0, 0.0)
                );
                (scope.start, scope.end)
            })
            .collect::<Vec<_>>();
        shapes.sort_unstable();
        if text % 2 == 0 {
            assert_eq!(shapes, [(0, 1), (1, 2), (3, 4), (4, 5), (6, 7)]);
            assert_eq!(
                scopes
                    .iter()
                    .filter(|scope| scope.text == text && scope.font_flags & 32 == 0)
                    .count(),
                2
            );
        } else {
            assert_eq!(shapes, [(0, 3)]);
            assert!(
                scopes
                    .iter()
                    .filter(|scope| scope.text == text)
                    .all(|scope| scope.font_flags == 32)
            );
        }
    }
    for start in (0..original.runs).step_by(127) {
        let count = 127.min(original.runs - start);
        for current in f.read_from::<PreviewRun>(3, start, count) {
            assert_eq!(current.flags & (16 | 32), 0);
        }
    }
    let mut untouched = PreviewScriptScope {
        text: 1234,
        ..Default::default()
    };
    let mut result = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        written: 99,
        next: 99,
        ..Default::default()
    };
    // SAFETY: cancellation touches only the shared flag; the following read has
    // one independent aligned output row and result with no overlapping borrows.
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        assert_eq!(
            notepads_markdown_preview_read(
                f.layout,
                10,
                256,
                1,
                size_of::<PreviewScriptScope>() as u32,
                ptr::from_mut(&mut untouched).cast(),
                &mut result,
                &mut WireError::default()
            ),
            4
        );
    }
    assert_eq!(
        (untouched.text, result.written, result.next),
        (1234, 99, 99)
    );
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}
#[test]
fn script_code_line_boxes_cross_abi_chunks_with_expected_flags() {
    use markdown_core::config::Extensions;
    // Each paragraph has empty and nonempty HTML font boxes, and a Markdown code
    // span whose trim/line-ending atoms must still create only one scope.
    let paragraph = "<sub><code></code><kbd></kbd><samp></samp><code>x</code><kbd>y</kbd><samp>z</samp>***` a\nb `***</sub>";
    let f = Fixture::with_extensions(
        &std::iter::repeat_n(paragraph, 40)
            .collect::<Vec<_>>()
            .join("\n\n"),
        Extensions::HTML_INLINE,
    );
    let original = f.batch(256);
    let mut summary = PreviewScriptSummary {
        header: WireHeader {
            abi_version: PREVIEW_SCRIPT_ABI_VERSION,
            struct_size: size_of::<PreviewScriptSummary>() as u32,
        },
        ..Default::default()
    };
    // SAFETY: fixture exclusively owns the current layout and independent PODs.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_script_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!((summary.key, summary.start), (original.key, original.start));
    assert_eq!(summary.rows, 440);
    assert_eq!(size_of::<PreviewScriptScope>(), 40);
    assert_eq!(offset_of!(PreviewScriptScope, font_flags), 12);
    let mut scopes = Vec::new();
    for start in (0..summary.rows).step_by(127) {
        scopes.extend(f.read_from::<PreviewScriptScope>(10, start, 127.min(summary.rows - start)));
    }
    let shape_scopes = scopes
        .iter()
        .filter(|scope| scope.font_flags & 32 != 0)
        .collect::<Vec<_>>();
    assert_eq!(shape_scopes.len(), 120);
    for scope in shape_scopes {
        assert_eq!((scope.start, scope.end, scope.font_flags), (3, 6, 32));
        assert_eq!(
            (scope.size_scale, scope.relative_em, scope.absolute_dip),
            (1.0, 0.0, 0.0)
        );
    }
    for text in 0..40 {
        let mut actual = scopes
            .iter()
            .filter(|scope| scope.text == text && scope.font_flags & 32 == 0)
            .map(|scope| {
                assert_eq!(scope.absolute_dip, 1.0);
                (scope.start, scope.end, scope.font_flags)
            })
            .collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(
            actual,
            [
                (0, 0, 8),
                (0, 0, 8),
                (0, 0, 24),
                (0, 1, 24),
                (0, 6, 0),
                (1, 2, 8),
                (2, 3, 8),
                (3, 6, 27)
            ]
        );
    }
    for start in (0..original.runs).step_by(127) {
        let count = 127.min(original.runs - start);
        for current in f.read_from::<PreviewRun>(3, start, count) {
            assert_eq!(current.flags & 16, 0);
            assert!(current.flags == 8 || current.flags == 11);
        }
    }
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}
#[test]
fn script_scope_fonts_distinguish_opening_order_and_empty_code() {
    use markdown_core::config::Extensions;
    let f = Fixture::with_extensions(
        "A <b><sup></sup></b> <sup><b></b></sup> <sub><code></code></sub> **<sub></sub>**",
        Extensions::HTML_INLINE,
    );
    let original = f.batch(256);
    let mut summary = PreviewScriptSummary {
        header: WireHeader {
            abi_version: PREVIEW_SCRIPT_ABI_VERSION,
            struct_size: size_of::<PreviewScriptSummary>() as u32,
        },
        ..Default::default()
    };
    // SAFETY: the fixture exclusively owns the batch and all PODs are independent.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_script_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!(summary.key, original.key);
    let scopes = f.read::<PreviewScriptScope>(10, summary.rows);
    assert_eq!(scopes.len(), 7);
    let shapes = scopes
        .iter()
        .filter(|scope| scope.font_flags & 32 != 0)
        .collect::<Vec<_>>();
    assert_eq!(shapes.len(), 1);
    assert_eq!(
        (
            shapes[0].text,
            shapes[0].start,
            shapes[0].end,
            shapes[0].font_flags
        ),
        (0, 2, 2, 32)
    );
    assert_eq!(
        (
            shapes[0].size_scale,
            shapes[0].relative_em,
            shapes[0].absolute_dip
        ),
        (1.0, 0.0, 0.0)
    );
    let scopes = scopes
        .into_iter()
        .filter(|scope| scope.font_flags & 32 == 0)
        .collect::<Vec<_>>();
    assert_eq!(offset_of!(PreviewScriptScope, font_flags), 12);
    for (position, expected) in [
        (2, vec![1]),
        (3, vec![0, 1]),
        (4, vec![0, 24]),
        (5, vec![1]),
    ] {
        let mut flags = scopes
            .iter()
            .filter(|scope| scope.start == position)
            .map(|scope| {
                assert_eq!(scope.end, position);
                assert_eq!(scope.text, 0);
                scope.font_flags
            })
            .collect::<Vec<_>>();
        flags.sort_unstable();
        assert_eq!(flags, expected);
    }
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}
#[test]
fn script_tables_preserve_styles_and_distinct_empty_text_owners() {
    use markdown_core::config::Extensions;
    let f = Fixture::with_extensions(
        "A <sup>[B<sub>C</sub>](#target)</sup> D\n\n<sup></sup>\n\n<sub></sub>",
        Extensions::HTML_INLINE,
    );
    let original = f.batch(256);
    let current = f.read::<PreviewRun>(3, original.runs);
    assert_eq!(size_of::<PreviewRun>(), 40);
    for current in &current {
        assert!(current.size_scale.is_finite() && current.size_scale > 0.0);
        assert!(current.relative_em.is_finite() && current.absolute_dip.is_finite());
    }
    let mut summary = PreviewScriptSummary {
        header: WireHeader {
            abi_version: PREVIEW_SCRIPT_ABI_VERSION,
            struct_size: size_of::<PreviewScriptSummary>() as u32,
        },
        ..Default::default()
    };
    // SAFETY: fixture owns the current exclusive layout and independent aligned PODs.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_script_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!(summary.key, original.key);
    assert_eq!(summary.start, original.start);
    assert_eq!(summary.rows, 5);
    let all_scopes = f.read::<PreviewScriptScope>(10, summary.rows);
    let shape_scopes = all_scopes
        .iter()
        .filter(|scope| scope.font_flags & 32 != 0)
        .collect::<Vec<_>>();
    assert_eq!(shape_scopes.len(), 1);
    assert_eq!(
        (
            shape_scopes[0].text,
            shape_scopes[0].start,
            shape_scopes[0].end,
            shape_scopes[0].font_flags
        ),
        (0, 2, 4, 32)
    );
    let scopes = all_scopes
        .into_iter()
        .filter(|scope| scope.font_flags & 32 == 0)
        .collect::<Vec<_>>();
    assert_eq!(
        scopes
            .iter()
            .map(|scope| (scope.text, scope.start, scope.end))
            .collect::<Vec<_>>(),
        [(0, 2, 4), (0, 3, 4), (1, 0, 0), (2, 0, 0)]
    );
    assert!(scopes.iter().all(|scope| scope.font_flags == 0));
    let texts = f.read::<PreviewText>(2, original.texts);
    assert_eq!(texts[1].logical_start, texts[2].logical_start);
    assert_eq!(texts[1].utf16_length, 0);
    assert_eq!(texts[2].utf16_length, 0);
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn script_tables_cross_256_row_chunks_with_the_same_run_indices() {
    use markdown_core::config::Extensions;
    let f = Fixture::with_extensions(&"A<sup>x</sup>".repeat(300), Extensions::HTML_INLINE);
    let original = f.batch(256);
    let mut summary = PreviewScriptSummary {
        header: WireHeader {
            abi_version: PREVIEW_SCRIPT_ABI_VERSION,
            struct_size: size_of::<PreviewScriptSummary>() as u32,
        },
        ..Default::default()
    };
    // SAFETY: fixture owns all exclusive handles and independent summary/error storage.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_script_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!(summary.rows, 300);
    for start in (0..summary.rows).step_by(256) {
        let count = 256.min(summary.rows - start);
        let scopes = f.read_from::<PreviewScriptScope>(10, start, count);
        for (offset, scope) in scopes.iter().enumerate() {
            let ordinal = start + offset as u64;
            assert_eq!(
                (scope.text, scope.start, scope.end),
                (0, ordinal as u32 * 2 + 1, ordinal as u32 * 2 + 2)
            );
            assert_eq!(scope.absolute_dip, -1.0);
        }
    }
    for start in (0..original.runs).step_by(127) {
        let count = 127.min(original.runs - start);
        for current in f.read_from::<PreviewRun>(3, start, count) {
            assert_eq!(current.end, current.start + 1);
            if current.start.is_multiple_of(2) {
                assert_eq!(
                    (
                        current.size_scale,
                        current.relative_em,
                        current.absolute_dip
                    ),
                    (1.0, 0.0, 0.0)
                );
            } else {
                assert!((current.size_scale - 5.0 / 6.0).abs() < 1e-12);
                assert_eq!(current.absolute_dip, -1.0);
            }
        }
    }
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn script_summary_short_prefix_rejects_before_accessing_full_storage() {
    use markdown_core::config::Extensions;
    #[repr(C, align(8))]
    struct Short {
        header: WireHeader,
    }
    let f = Fixture::with_extensions("A<sup>x</sup>", Extensions::HTML_INLINE);
    let original = f.batch(256);
    let mut short = Short {
        header: WireHeader {
            abi_version: PREVIEW_SCRIPT_ABI_VERSION,
            struct_size: 8,
        },
    };
    let mut summary = PreviewScriptSummary {
        header: WireHeader {
            abi_version: PREVIEW_SCRIPT_ABI_VERSION,
            struct_size: size_of::<PreviewScriptSummary>() as u32,
        },
        ..Default::default()
    };
    // SAFETY: the rejected pointer exposes only its aligned initialized prefix.
    // Successful calls use full independent PODs and the fixture's exclusive layout.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_script_summary(
                f.layout,
                (&mut short as *mut Short).cast(),
                &mut WireError::default()
            ),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(short.header.struct_size, 8);
        assert_eq!(
            notepads_markdown_preview_script_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
        assert_eq!(summary.rows, 1);
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        assert_ne!(
            notepads_markdown_preview_script_summary(
                f.layout,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!(summary.key, original.key);
    assert_eq!(summary.rows, 1);
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}
#[test]
fn derived_batch_keeps_preview_rows_and_exports_the_pinned_original_source() {
    let mut fixture =
        Fixture::with_extensions("pre $a\r\nb$ post", markdown_core::config::Extensions::MATH);
    let original = fixture.batch(256);
    assert_eq!(size_of::<PreviewText>(), 64);
    let mut summary = DerivedSummary {
        header: WireHeader {
            abi_version: DERIVED_ABI_VERSION,
            struct_size: size_of::<DerivedSummary>() as u32,
        },
        ..Default::default()
    };
    let mut error = WireError::default();
    // SAFETY: fixture owns the exclusive batch and all independent aligned storage.
    unsafe {
        assert_eq!(
            notepads_markdown_derived_summary(fixture.layout, &mut summary, &mut error),
            0
        );
        assert_eq!(summary.rows, 1);
        assert_eq!(summary.blob_bytes, original.blob_bytes);
        assert_eq!(summary.key.model.extensions, 1);
        let mut wrong = summary;
        wrong.header.abi_version = PREVIEW_ABI_VERSION;
        assert_eq!(
            notepads_markdown_derived_summary(fixture.layout, &mut wrong, &mut error),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(wrong.rows, 1);
        let mut row = DerivedRow::default();
        let mut read = WireReadResult {
            header: WireHeader::of::<WireReadResult>(),
            written: 17,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_derived_read(
                fixture.layout,
                0,
                1,
                55,
                &mut row,
                &mut read,
                &mut error
            ),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(read.written, 17);
        assert_eq!(
            notepads_markdown_derived_read(
                fixture.layout,
                0,
                1,
                size_of::<DerivedRow>() as u32,
                &mut row,
                &mut read,
                &mut error
            ),
            0
        );
        assert_eq!(
            (row.kind, row.syntax, row.text, row.offset_utf16),
            (1, 1, 0, 4)
        );
        assert_eq!((read.written, read.next, read.done), (1, 1, 1));
        assert_eq!(notepads_markdown_core_model_destroy(fixture.model), 0);
        fixture.model = ptr::null_mut();
        // The retained preview batch owns the original source even without this handle.
        let bytes = fixture.read::<u8>(8, summary.blob_bytes);
        let value = |slice: PreviewBlob| {
            std::str::from_utf8(&bytes[slice.start as usize..(slice.start + slice.length) as usize])
                .unwrap()
        };
        assert_eq!(row.accessible_title, PreviewBlob::default());
        assert_eq!(row.accessible_description, PreviewBlob::default());
        assert_eq!(value(row.input), "a b");
        assert_eq!(value(row.export), "$a\r\nb$");
    }
}

#[test]
fn diagram_metadata_shares_input_bytes_and_leaves_the_authored_export_intact() {
    use markdown_core::config::Extensions;
    let source = "```mermaid\r\nclassDiagram\r\naccTitle: 中文\r\naccDescr {\r\n第一行\r\nSecond line\r\n}\r\nA --> B\r\n```\r\n";
    let fixture = Fixture::with_extensions(source, Extensions::MERMAID);
    let summary = fixture.batch(256);
    let mut row = DerivedRow::default();
    let mut result = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        ..Default::default()
    };
    // SAFETY: the exclusive fixture and one aligned row remain live and distinct.
    unsafe {
        assert_eq!(
            notepads_markdown_derived_read(
                fixture.layout,
                0,
                1,
                size_of::<DerivedRow>() as u32,
                &mut row,
                &mut result,
                &mut WireError::default(),
            ),
            0
        );
    }
    assert_eq!((row.kind, result.written), (2, 1));
    let bytes = fixture.read::<u8>(8, summary.blob_bytes);
    let value = |slice: PreviewBlob| {
        std::str::from_utf8(&bytes[slice.start as usize..(slice.start + slice.length) as usize])
            .unwrap()
    };
    assert_eq!(value(row.accessible_title), "中文");
    assert_eq!(value(row.accessible_description), "第一行\nSecond line");
    assert_eq!(
        value(row.input),
        source
            .split_once('\n')
            .unwrap()
            .1
            .split("```")
            .next()
            .unwrap()
            .replace("\r\n", "\n")
    );
    assert_eq!(value(row.export), source);
    assert!(row.accessible_title.start >= row.input.start);
    assert!(
        row.accessible_description.start + row.accessible_description.length
            <= row.input.start + row.input.length
    );
}

struct Fixture {
    cancel: *mut Owned<CoreCancel>,
    model: *mut Owned<CoreModel>,
    layout: *mut Owned<PreviewLayout>,
    key: PreviewKey,
    budget: SourceBudget,
}

impl Fixture {
    fn new(text: &str) -> Self {
        Self::with_extensions(text, markdown_core::config::Extensions::NONE)
    }

    fn with_extensions(text: &str, extensions: markdown_core::config::Extensions) -> Self {
        Self::with_purpose(text, extensions, PREVIEW_LAYOUT_VISIBLE)
    }

    fn with_purpose(text: &str, extensions: markdown_core::config::Extensions, purpose: u32) -> Self {
        let mut error = WireError::default();
        let mut cancel = ptr::null_mut();
        // SAFETY: all handles here are uniquely owned and every POD/buffer is independent.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_cancel_create(&mut cancel, &mut error),
                0
            );
            let budget = (*cancel).value.budget.clone();
            let mut op = budget.begin((*cancel).value.token.clone());
            let document = SourceDocument::new(
                SourceKey::new([128; 16], 1).unwrap(),
                text.as_bytes(),
                &mut op,
            )
            .unwrap();
            let accepted = AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::with_extensions(Profile::G3, extensions),
                1,
                &mut op,
            )
            .unwrap();
            let key = to_wire_key(
                LayoutKey::new(
                    accepted.identity(),
                    1,
                    2,
                    3,
                    400.0,
                    LayoutStyle::new(20.0, 4.0).unwrap(),
                )
                .unwrap(),
            );
            let slot = Slot::new(op.boundary_work()).unwrap();
            let model = slot.commit(CoreModel { model: accepted });
            let mut layout = ptr::null_mut();
            assert_eq!(
                notepads_markdown_preview_layout_create(
                    model,
                    cancel,
                    &key,
                    ptr::null(),
                    0,
                    purpose,
                    0,
                    &mut layout,
                    &mut error
                ),
                0
            );
            Self {
                cancel,
                model,
                layout,
                key,
                budget,
            }
        }
    }

    fn batch(&self, maximum: u32) -> PreviewSummary {
        let mut result = PreviewSummary {
            header: prefix::<PreviewSummary>(),
            ..Default::default()
        };
        // SAFETY: this fixture owns the sole handle and writable independent PODs.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_measure_begin(
                    self.layout,
                    maximum,
                    &mut result,
                    &mut WireError::default()
                ),
                0
            );
        }
        result
    }

    fn read<T: Default + Copy>(&self, table: u32, count: u64) -> Vec<T> {
        self.read_from(table, 0, count)
    }

    fn read_from<T: Default + Copy>(&self, table: u32, start: u64, count: u64) -> Vec<T> {
        let mut buffer = vec![T::default(); count as usize];
        let mut result = WireReadResult {
            header: WireHeader::of::<WireReadResult>(),
            ..Default::default()
        };
        // SAFETY: buffer contains count valid aligned writable T, and fixed row_size is exact.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_read(
                    self.layout,
                    table,
                    start,
                    count,
                    size_of::<T>() as u32,
                    buffer.as_mut_ptr().cast(),
                    &mut result,
                    &mut WireError::default()
                ),
                0
            );
        }
        assert_eq!(result.written, count);
        assert_eq!(result.next, start + count);
        buffer
    }

    fn submit(&self, summary: &PreviewSummary, heights: &[f64]) {
        let blocks = self.read::<PreviewBlock>(1, summary.blocks);
        assert_eq!(blocks.len(), heights.len());
        let metrics: Vec<_> = blocks
            .iter()
            .zip(heights)
            .map(|(b, h)| PreviewMetric {
                node: b.node,
                reserved: 0,
                height: *h,
            })
            .collect();
        // SAFETY: metrics and key are fully readable and nonoverlapping with layout/error.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_submit(
                    self.layout,
                    &self.key,
                    summary.start,
                    metrics.as_ptr(),
                    metrics.len() as u32,
                    &mut WireError::default()
                ),
                0
            );
        }
    }

    fn finish(&self) -> PreviewExtent {
        let mut result = PreviewExtent {
            header: prefix::<PreviewExtent>(),
            ..Default::default()
        };
        // SAFETY: live exclusive handle and writable accepted output.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_finish(
                    self.layout,
                    &mut result,
                    &mut WireError::default()
                ),
                0
            );
        }
        result
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: fixture transfers each sole handle exactly once after all calls have ended.
        unsafe {
            let _ = notepads_markdown_preview_layout_destroy(self.layout);
            let _ = notepads_markdown_core_model_destroy(self.model);
            let _ = notepads_markdown_core_cancel_destroy(self.cancel);
        }
    }
}

fn released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(
        (
            usage.source_capacity,
            usage.auxiliary_capacity,
            usage.source_owners
        ),
        (0, 0, 0)
    );
}

#[test]
fn tight_list_spacing_round_trips_extent_viewport_and_original_logical_ranges() {
    let f = Fixture::new("- a\n- b\n\nTail");
    let batch = f.batch(256);
    assert_eq!(batch.blocks, 3);
    let original = f.read::<PreviewBlock>(1, batch.blocks);
    f.submit(&batch, &[10.0, 10.0, 10.0]);
    assert_eq!(f.finish().height, 34.0);
    let viewport = PreviewViewport {
        header: prefix::<PreviewViewport>(),
        key: f.key,
        y: 10.0,
        height: 1.0,
        overscan: 0.0,
    };
    let mut range = PreviewRange {
        header: prefix::<PreviewRange>(),
        ..Default::default()
    };
    let mut summary = PreviewSummary {
        header: prefix::<PreviewSummary>(),
        ..Default::default()
    };
    // SAFETY: the fixture owns the live layout; all full POD outputs are independent.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_visible(
                f.layout,
                &viewport,
                &mut range,
                &mut WireError::default()
            ),
            0
        );
        assert_eq!((range.start, range.count), (1, 1));
        assert_eq!(
            notepads_markdown_preview_present(
                f.layout,
                range.start,
                1,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    let visible = f.read::<PreviewBlock>(1, summary.blocks);
    assert_eq!((visible[0].y, visible[0].height), (10.0, 10.0));
    assert_eq!(
        (
            visible[0].node,
            visible[0].logical_start,
            visible[0].logical_end
        ),
        (
            original[1].node,
            original[1].logical_start,
            original[1].logical_end
        )
    );
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn actual_g3_projection_metrics_and_visible_reprojection_round_trip() {
    let f = Fixture::new("## 标题🎸\n\n**粗体** [link](https://example.test/)\n\n- [x] done\n");
    assert_eq!(f.budget.elapsed_limit_ns(), Some(5_000_000_000));
    let batch = f.batch(256);
    assert_eq!(batch.blocks, 3);
    let blocks = f.read::<PreviewBlock>(1, batch.blocks);
    assert_eq!((blocks[0].kind, blocks[0].heading_level), (2, 2));
    let texts = f.read::<PreviewText>(2, batch.texts);
    let bytes = f.read::<u8>(8, batch.blob_bytes);
    let title = texts[blocks[0].texts.start as usize];
    assert_eq!(
        &bytes[title.utf8.start as usize..(title.utf8.start + title.utf8.length) as usize],
        "标题🎸".as_bytes()
    );
    assert_eq!(title.utf16_length, 4);
    let runs = f.read::<PreviewRun>(3, batch.runs);
    assert!(runs.iter().any(|r| r.flags & 1 != 0));
    assert!(runs.iter().any(|r| r.link != u32::MAX));
    let decorations = f.read::<PreviewDecoration>(5, batch.decorations);
    assert!(decorations.iter().any(|r| r.kind == 3 && r.checked == 1));
    f.submit(&batch, &[20.0, 30.0, 40.0]);
    let extent = f.finish();
    assert_eq!((extent.blocks, extent.height), (3, 98.0));
    let viewport = PreviewViewport {
        header: prefix::<PreviewViewport>(),
        key: f.key,
        y: 24.0,
        height: 1.0,
        overscan: 0.0,
    };
    let mut range = PreviewRange {
        header: prefix::<PreviewRange>(),
        ..Default::default()
    };
    let mut summary = PreviewSummary {
        header: prefix::<PreviewSummary>(),
        ..Default::default()
    };
    // SAFETY: live exclusive layout and disjoint full PODs.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_visible(
                f.layout,
                &viewport,
                &mut range,
                &mut WireError::default()
            ),
            0
        );
        assert_eq!((range.start, range.count), (1, 1));
        assert_eq!(
            notepads_markdown_preview_present(
                f.layout,
                range.start,
                range.count as u32,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    let visible = f.read::<PreviewBlock>(1, summary.blocks);
    assert_eq!((visible[0].y, visible[0].height), (24.0, 30.0));
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn empty_model_has_a_finished_empty_frame_and_valid_zero_range() {
    let f = Fixture::new("");
    assert_eq!(f.batch(1).blocks, 0);
    assert_eq!(f.finish().height, 0.0);
    let viewport = PreviewViewport {
        header: prefix::<PreviewViewport>(),
        key: f.key,
        y: 0.0,
        height: 100.0,
        overscan: 20.0,
    };
    let mut range = PreviewRange {
        header: prefix::<PreviewRange>(),
        ..Default::default()
    };
    let mut summary = PreviewSummary {
        header: prefix::<PreviewSummary>(),
        ..Default::default()
    };
    // SAFETY: independent PODs and exclusive handle.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_visible(
                f.layout,
                &viewport,
                &mut range,
                &mut WireError::default()
            ),
            0
        );
        assert_eq!((range.start, range.count), (0, 0));
        assert_eq!(
            notepads_markdown_preview_present(
                f.layout,
                0,
                0,
                &mut summary,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!(summary.blocks, 0);
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn bad_prefix_reads_only_eight_bytes_and_does_not_fault_live_layout() {
    let f = Fixture::new("body");
    let prefix_only = WireHeader {
        abi_version: 99,
        struct_size: size_of::<PreviewSummary>() as u32,
    };
    let mut error = WireError::default();
    // SAFETY: mismatching initialized prefix is the only readable/writable POD part.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_measure_begin(
                f.layout,
                1,
                ptr::from_ref(&prefix_only).cast_mut().cast(),
                &mut error
            ),
            STATUS_ABI_MISMATCH
        );
    }
    assert_eq!(f.batch(1).blocks, 1);
}

#[test]
fn rejected_key_and_budget_do_not_publish_created_handle() {
    let f = Fixture::new("body");
    let mut key = f.key;
    key.model.revision += 1;
    let mut output = ptr::dangling_mut();
    let mut error = WireError::default();
    // SAFETY: fully initialized inputs and separate writable output/error.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_layout_create(
                f.model,
                f.cancel,
                &key,
                ptr::null(),
                0,
                PREVIEW_LAYOUT_VISIBLE,
                0,
                &mut output,
                &mut error
            ),
            10
        );
    }
    assert!(output.is_null());
    let g = Fixture::new("another");
    // SAFETY: live but intentionally unrelated budget-root handles, diagnosed before construction.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_layout_create(
                f.model,
                g.cancel,
                &f.key,
                ptr::null(),
                0,
                PREVIEW_LAYOUT_VISIBLE,
                0,
                &mut output,
                &mut error
            ),
            14
        );
    }
    assert!(output.is_null());
    assert_eq!(f.batch(1).blocks, 1);
}

#[test]
fn viewport_rejects_nested_key_prefix_without_writing_or_faulting_layout() {
    let f = Fixture::new("body");
    let batch = f.batch(1);
    f.submit(&batch, &[20.0]);
    f.finish();
    let mut viewport = PreviewViewport {
        header: prefix::<PreviewViewport>(),
        key: f.key,
        y: 0.0,
        height: 10.0,
        overscan: 0.0,
    };
    let mut output = PreviewRange {
        header: prefix::<PreviewRange>(),
        start: 123,
        count: 456,
    };
    for bad_prefix in [
        WireHeader {
            abi_version: 99,
            ..prefix::<PreviewKey>()
        },
        WireHeader {
            struct_size: 8,
            ..prefix::<PreviewKey>()
        },
    ] {
        viewport.key.header = bad_prefix;
        // SAFETY: full independent PODs are live; only the nested prefix is invalid.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_visible(
                    f.layout,
                    &viewport,
                    &mut output,
                    &mut WireError::default()
                ),
                STATUS_ABI_MISMATCH
            );
        }
        assert_eq!((output.start, output.count), (123, 456));
    }
    viewport.key = f.key;
    // SAFETY: matching current key proves prefix rejection did not consume the layout.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_visible(
                f.layout,
                &viewport,
                &mut output,
                &mut WireError::default()
            ),
            0
        );
    }
    assert_eq!((output.start, output.count), (0, 1));
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn failed_read_preserves_success_pod_and_latches_first_fault() {
    let f = Fixture::new("body");
    f.batch(1);
    let mut buffer = PreviewBlock::default();
    let mut result = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        written: 123,
        ..Default::default()
    };
    let mut error = WireError::default();
    // SAFETY: buffer is writable; wrong row_size must reject before writing any row.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_read(
                f.layout,
                1,
                0,
                1,
                1,
                ptr::from_mut(&mut buffer).cast(),
                &mut result,
                &mut error
            ),
            16
        );
        assert_eq!(result.written, 123);
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        let mut summary = PreviewSummary {
            header: prefix::<PreviewSummary>(),
            blocks: 99,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_preview_measure_begin(f.layout, 1, &mut summary, &mut error),
            16
        );
        assert_eq!(summary.blocks, 99);
    }
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn metrics_are_exact_and_last_invalid_height_never_commits() {
    let f = Fixture::new("one\n\ntwo");
    let batch = f.batch(2);
    let blocks = f.read::<PreviewBlock>(1, batch.blocks);
    let metrics = [
        PreviewMetric {
            node: blocks[0].node,
            reserved: 0,
            height: 20.0,
        },
        PreviewMetric {
            node: blocks[1].node,
            reserved: 0,
            height: f64::NAN,
        },
    ];
    let mut error = WireError::default();
    // SAFETY: complete two-row metric storage with deliberately invalid final height.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_submit(f.layout, &f.key, 0, metrics.as_ptr(), 2, &mut error),
            15
        );
        let LayoutState::Draft(draft) = &(*f.layout).value.state else {
            panic!("failed draft state changed");
        };
        assert_eq!(draft.completed_blocks(), 0);
        let mut extent = PreviewExtent {
            header: prefix::<PreviewExtent>(),
            blocks: 99,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_preview_finish(f.layout, &mut extent, &mut error),
            15
        );
        assert_eq!(extent.blocks, 99);
    }
}

#[test]
fn canceled_layout_cannot_resume_with_new_page_or_allocation() {
    let f = Fixture::new("body");
    let mut summary = PreviewSummary {
        header: prefix::<PreviewSummary>(),
        ..Default::default()
    };
    let mut allocation = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: live handles, shared atomic cancellation only, exclusive following layout calls.
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        assert_eq!(
            notepads_markdown_preview_measure_begin(f.layout, 1, &mut summary, &mut error),
            4
        );
        assert_eq!(
            notepads_markdown_preview_reserve(f.layout, 1, &mut allocation, &mut error),
            4
        );
    }
    assert!(allocation.is_null());
    let budget = f.budget.clone();
    drop(f);
    released(&budget);
}

#[test]
fn native_reservations_check_root_capacity_and_last_free() {
    let f = Fixture::new("body");
    let g = Fixture::new("other");
    let mut allocation = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: independent live handles, reservation is retained until its one explicit destroy.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_reserve(f.layout, 64, &mut allocation, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_preview_admit(f.layout, allocation, 128, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_preview_admit(g.layout, allocation, 256, &mut error),
            14
        );
        assert_eq!(notepads_markdown_preview_allocation_destroy(allocation), 0);
    }
    assert_eq!(f.batch(1).blocks, 1);
    let first = f.budget.clone();
    let second = g.budget.clone();
    drop((f, g));
    released(&first);
    released(&second);
}

#[test]
fn native_reservation_outlives_all_model_layout_and_cancel_handles() {
    let f = Fixture::new("body");
    let budget = f.budget.clone();
    let mut allocation = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: the sole allocation handle remains valid independently of its layout.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_reserve(f.layout, 64, &mut allocation, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_preview_admit(f.layout, allocation, 128, &mut error),
            0
        );
    }
    drop(f);
    let retained = budget.usage();
    assert_eq!((retained.source_capacity, retained.source_owners), (0, 0));
    assert!(retained.auxiliary_capacity >= 128);
    // SAFETY: native bytes have conceptually been freed; retire the one remaining lease.
    unsafe {
        assert_eq!(notepads_markdown_preview_allocation_destroy(allocation), 0);
    }
    released(&budget);
}

#[test]
fn post_completion_viewport_tokens_do_not_cancel_the_shared_document() {
    let f = Fixture::new("one\n\ntwo");
    let batch = f.batch(2);
    f.submit(&batch, &[20.0, 20.0]);
    f.finish();
    for y in [0.0, 24.0, 0.0, 24.0] {
        let viewport = PreviewViewport {
            header: prefix::<PreviewViewport>(),
            key: f.key,
            y,
            height: 1.0,
            overscan: 0.0,
        };
        let mut range = PreviewRange {
            header: prefix::<PreviewRange>(),
            ..Default::default()
        };
        let mut summary = PreviewSummary {
            header: prefix::<PreviewSummary>(),
            ..Default::default()
        };
        // SAFETY: scrolling uses the same complete layout without canceling its shared flag.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_visible(
                    f.layout,
                    &viewport,
                    &mut range,
                    &mut WireError::default()
                ),
                0
            );
            assert_eq!(
                notepads_markdown_preview_present(
                    f.layout,
                    range.start,
                    range.count as u32,
                    &mut summary,
                    &mut WireError::default()
                ),
                0
            );
        }
        assert_eq!(summary.blocks, 1);
    }
}

#[test]
fn semantic_projection_rejects_measurement_publication_without_mutating_outputs() {
    for operation in 0..3 {
        // Empty semantic projections used to finish successfully. A populated
        // batch also proves submit rejects its purpose before interpreting the key.
        let f = Fixture::with_purpose(
            if operation == 2 { "body" } else { "" },
            markdown_core::config::Extensions::NONE,
            PREVIEW_LAYOUT_SEMANTICS,
        );
        let budget = f.budget.clone();
        let mut extent = PreviewExtent {
            header: prefix::<PreviewExtent>(),
            blocks: 777,
            height: 888.0,
            ..Default::default()
        };
        let before = extent;
        let mut error = WireError::default();
        // SAFETY: fixture owns each live handle; all PODs are independent and initialized.
        unsafe {
            if operation == 2 {
                let mut summary = PreviewSummary {
                    header: prefix::<PreviewSummary>(),
                    ..Default::default()
                };
                assert_eq!(
                    notepads_markdown_preview_semantic_batch(
                        f.layout,
                        0,
                        1,
                        &mut summary,
                        &mut error
                    ),
                    0
                );
            }
            let metric = PreviewMetric {
                node: 0,
                reserved: 0,
                height: f64::NAN,
            };
            let mut stale_key = f.key;
            stale_key.layout_generation += 1;
            let status = match operation {
                0 => notepads_markdown_preview_finish(f.layout, &mut extent, &mut error),
                1 => notepads_markdown_preview_begin_prefix(f.layout, 0.0, &mut extent, &mut error),
                _ => notepads_markdown_preview_submit(
                    f.layout,
                    &stale_key,
                    0,
                    &metric,
                    1,
                    &mut error,
                ),
            };
            assert_eq!(status, wire_error(CoreError::InvalidState).status);
            assert_eq!(error.status, status);
            assert_eq!(extent, before);
        }
        drop(f);
        released(&budget);
    }
}

#[test]
fn model_funding_requires_the_same_root_and_a_live_allocation_token() {
    let first = Fixture::new("body");
    let foreign = Fixture::new("other");
    let mut allocation = ptr::dangling_mut();
    let mut error = WireError::default();
    // SAFETY: independent live fixture handles and exclusive allocation output.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_model_reserve(
                first.model,
                foreign.cancel,
                64,
                &mut allocation,
                &mut error
            ),
            14
        );
        assert!(allocation.is_null());
        assert_eq!(
            notepads_markdown_preview_model_reserve(
                first.model,
                first.cancel,
                64,
                &mut allocation,
                &mut error
            ),
            0
        );
        assert_eq!(
            notepads_markdown_preview_model_admit(
                foreign.model,
                foreign.cancel,
                allocation,
                128,
                &mut error
            ),
            14
        );
        assert_eq!(
            notepads_markdown_preview_model_admit(
                first.model,
                first.cancel,
                allocation,
                128,
                &mut error
            ),
            0
        );
        assert_eq!(notepads_markdown_core_cancel_request(first.cancel), 0);
        assert_eq!(
            notepads_markdown_preview_model_admit(
                first.model,
                first.cancel,
                allocation,
                256,
                &mut error
            ),
            4
        );
        assert_eq!(notepads_markdown_preview_allocation_destroy(allocation), 0);
    }
    let first_budget = first.budget.clone();
    let foreign_budget = foreign.budget.clone();
    drop((first, foreign));
    released(&first_budget);
    released(&foreign_budget);
}

#[test]
fn independent_model_funding_survives_source_cancel_and_all_other_handles() {
    let fixture = Fixture::new("body");
    let budget = fixture.budget.clone();
    let mut funding = ptr::null_mut();
    let mut allocation = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: the new funding flag is independent of the canceled source flag.
    unsafe {
        assert_eq!(
            notepads_markdown_core_model_cancel_create(fixture.model, &mut funding, &mut error),
            0
        );
        assert_eq!(notepads_markdown_core_cancel_request(fixture.cancel), 0);
        assert_eq!(
            notepads_markdown_preview_model_reserve(
                fixture.model,
                funding,
                64,
                &mut allocation,
                &mut error
            ),
            0
        );
        assert_eq!(
            notepads_markdown_preview_model_admit(
                fixture.model,
                funding,
                allocation,
                192,
                &mut error
            ),
            0
        );
        assert_eq!(notepads_markdown_core_cancel_destroy(funding), 0);
    }
    drop(fixture);
    let retained = budget.usage();
    assert_eq!((retained.source_capacity, retained.source_owners), (0, 0));
    assert!(retained.auxiliary_capacity >= 192);
    // SAFETY: allocation is the only remaining owned handle; native bytes retire first.
    unsafe {
        assert_eq!(notepads_markdown_preview_allocation_destroy(allocation), 0);
    }
    released(&budget);
}

#[test]
fn task_decoration_exposes_main_utf16_and_rejects_previous_preview_abi() {
    let fixture = Fixture::new("- [x] 中文\n");
    let batch = fixture.batch(1);
    let decorations = fixture.read::<PreviewDecoration>(5, batch.decorations);
    assert_eq!(decorations.len(), 1);
    let task = decorations[0];
    assert_eq!((task.logical_start, task.logical_end), (0, 1));
    assert_eq!((task.action_start, task.action_end), (2, 5));
    let mut old_prefix = PreviewSummary {
        header: WireHeader {
            abi_version: 1,
            ..prefix::<PreviewSummary>()
        },
        blocks: 999,
        ..Default::default()
    };
    // SAFETY: initialized old-version prefix must reject without consuming this batch.
    unsafe {
        assert_eq!(
            notepads_markdown_preview_measure_begin(
                fixture.layout,
                1,
                &mut old_prefix,
                &mut WireError::default()
            ),
            STATUS_ABI_MISMATCH
        );
    }
    assert_eq!(old_prefix.blocks, 999);
    assert_eq!(fixture.batch(1).blocks, 1);
    assert_eq!(size_of::<PreviewDecoration>(), 80);
}

#[test]
fn static_html_checkbox_has_no_synthetic_source_edit_authority() {
    use markdown_core::config::Extensions;

    for (input, checked) in [
        (
            "<label><input type=checkbox checked disabled> 中😀 label</label>\n",
            1,
        ),
        ("<label><input type=checkbox disabled> empty</label>\n", 0),
        (
            "<LABEL><INPUT TYPE='check&#98;ox' checked='false' disabled='false'> bool</LABEL>\n",
            1,
        ),
    ] {
        let fixture = Fixture::with_extensions(input, Extensions::HTML_BLOCKS);
        let budget = fixture.budget.clone();
        let batch = fixture.batch(256);
        let decorations = fixture.read::<PreviewDecoration>(5, batch.decorations);
        assert_eq!(decorations.len(), 1, "{input}");
        let row = decorations[0];
        assert_eq!((row.kind, row.checked), (4, checked), "{input}");
        assert_ne!(row.object, u32::MAX);
        assert_eq!(row.logical_end - row.logical_start, 1);
        assert_eq!(row.has_action, 0);
        assert_eq!(
            (row.action_start, row.action_end, row.expected, row.reserved),
            (0, 0, 0, 0)
        );
        drop(fixture);
        released(&budget);
    }

    // Authored GFM retains its original three-byte edit capability. The new
    // static kind cannot silently disable normal task interactions.
    let fixture = Fixture::new("- [x] ordinary\n");
    let batch = fixture.batch(1);
    let row = fixture.read::<PreviewDecoration>(5, batch.decorations)[0];
    assert_eq!((row.kind, row.checked, row.has_action), (3, 1, 1));
    assert_eq!((row.action_start, row.action_end), (2, 5));
    assert_eq!(row.expected, u32::from_le_bytes([b'[', b'x', b']', 0]));

    // The private kind extension deliberately preserves the ABI3 wire shape.
    assert_eq!(
        (
            size_of::<PreviewDecoration>(),
            align_of::<PreviewDecoration>()
        ),
        (80, 8)
    );
    assert_eq!(offset_of!(PreviewDecoration, has_action), 36);
    assert_eq!(offset_of!(PreviewDecoration, logical_start), 64);
    let header = include_str!("../../../../../include/notepads/markdown/preview.h");
    assert!(
        header.contains("#define NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX 4u")
    );
}

#[test]
fn html_table_rowspans_pair_covered_slots_with_the_current_batch() {
    let source = "<table><tbody><tr><td rowspan=0 colspan=2>中😀</td><td>B</td></tr><tr><td>C</td></tr></tbody></table>";
    let fixture = Fixture::with_extensions(source, markdown_core::config::Extensions::HTML_BLOCKS);
    let summary = fixture.batch(256);
    let cells = fixture.read::<PreviewCell>(4, summary.cells);
    let html = fixture.read::<PreviewHtmlCell>(PREVIEW_HTML_CELLS, summary.cells);
    assert_eq!(
        cells
            .iter()
            .zip(&html)
            .map(|(cell, meta)| (
                cell.row,
                cell.column,
                meta.colspan,
                meta.rowspan,
                meta.flags
            ))
            .collect::<Vec<_>>(),
        [
            (0, 0, 2, 2, 1),
            (0, 1, 0, 0, 3),
            (0, 2, 1, 1, 1),
            (1, 0, 0, 0, 3),
            (1, 1, 0, 0, 3),
            (1, 2, 1, 1, 1)
        ]
    );
    assert!(
        cells
            .iter()
            .zip(&html)
            .all(|(cell, meta)| cell.node == meta.node
                && meta.reserved == 0
                && meta.reserved_geometry == 0)
    );
}

#[test]
fn html_table_pairs_real_spans_caption_and_original_th_facts() {
    use markdown_core::config::Extensions;
    let source = "<table>\r\n<caption>HTML 表格与跨列</caption>\r\n<thead>\r\n<tr><th scope=col>列一</th><th scope=col>列二</th></tr>\r\n</thead>\r\n<tbody>\r\n<tr><td colspan=2>跨两列，&amp; 实体测试</td></tr>\r\n<tr><td>R2C1</td><td><code>x &lt; y</code></td></tr>\r\n</tbody>\r\n</table>\r\n";
    let f = Fixture::with_extensions(source, Extensions::HTML_BLOCKS);
    let budget = f.budget.clone();
    let summary = f.batch(256);
    let blocks = f.read::<PreviewBlock>(1, summary.blocks);
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        (blocks[0].kind, blocks[0].columns, blocks[0].row_count),
        (6, 2, 4)
    );
    let cells = f.read::<PreviewCell>(4, summary.cells);
    let html = f.read::<PreviewHtmlCell>(PREVIEW_HTML_CELLS, summary.cells);
    let texts = f.read::<PreviewText>(2, summary.texts);
    let blob = f.read::<u8>(8, summary.blob_bytes);
    assert_eq!(cells.len(), 8);
    assert_eq!(html.len(), cells.len());
    assert_eq!(
        html.iter()
            .map(|r| (r.colspan, r.flags))
            .collect::<Vec<_>>(),
        [
            (2, 5),
            (0, 7),
            (1, 1),
            (1, 1),
            (2, 1),
            (0, 3),
            (1, 1),
            (1, 1)
        ]
    );
    assert_eq!(
        cells.iter().map(|r| r.header).collect::<Vec<_>>(),
        [0, 0, 1, 1, 0, 0, 0, 0]
    );
    let values: Vec<_> = cells
        .iter()
        .map(|cell| {
            let text = texts[cell.text as usize];
            std::str::from_utf8(
                &blob[text.utf8.start as usize..(text.utf8.start + text.utf8.length) as usize],
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        values,
        [
            "HTML 表格与跨列",
            "",
            "列一",
            "列二",
            "跨两列，& 实体测试",
            "",
            "R2C1",
            "x < y"
        ]
    );
    for (index, (cell, meta)) in cells.iter().zip(&html).enumerate() {
        assert_eq!((cell.node, 0), (meta.node, meta.reserved));
        assert_eq!(
            (cell.row, cell.column),
            (index as u32 / 2, index as u32 % 2)
        );
        assert_eq!(cell.missing, 0);
        if meta.flags & PREVIEW_HTML_CELL_COVERED != 0 {
            let text = texts[cell.text as usize];
            assert_eq!(text.logical_start, text.logical_end);
            assert_eq!(
                (text.utf16_length, text.runs.count, text.images.count),
                (0, 0, 0)
            );
        }
    }
    assert_eq!(cells[0].alignment, 2);
    // SAFETY: this fixture owns the model and the snapshot borrow is local.
    unsafe {
        assert_eq!((*f.model).value.model.original_source().text(), source);
    }
    f.submit(&summary, &[100.0]);
    f.finish();
    drop(f);
    released(&budget);
}

#[test]
fn html_table_preserves_empty_cells_and_headerless_rows() {
    use markdown_core::config::Extensions;
    for source in [
        "<table><caption></caption><tr><td></td></tr></table>\n",
        "<table><tr><td></td><th>中😀</th></tr><tr><td colspan=2></td></tr></table>\n",
    ] {
        let f = Fixture::with_extensions(source, Extensions::HTML_BLOCKS);
        let budget = f.budget.clone();
        let summary = f.batch(256);
        let blocks = f.read::<PreviewBlock>(1, summary.blocks);
        assert_eq!((blocks.len(), blocks[0].kind), (1, 6));
        let cells = f.read::<PreviewCell>(4, summary.cells);
        let html = f.read::<PreviewHtmlCell>(PREVIEW_HTML_CELLS, summary.cells);
        assert_eq!(
            cells[0].header, 0,
            "caption/td must not inherit synthetic GFM header"
        );
        if source.contains("caption") {
            assert_eq!(
                html.iter()
                    .map(|r| (r.colspan, r.flags))
                    .collect::<Vec<_>>(),
                [(1, 5), (1, 1)]
            );
        } else {
            assert_eq!(
                cells.iter().map(|r| r.header).collect::<Vec<_>>(),
                [0, 1, 0, 0]
            );
            assert_eq!(
                html.iter()
                    .map(|r| (r.colspan, r.flags))
                    .collect::<Vec<_>>(),
                [(1, 1), (1, 1), (2, 1), (0, 3)]
            );
        }
        drop(f);
        released(&budget);
    }
}

#[test]
fn html_table_pages_with_cells_and_keeps_gfm_yaml_defaults() {
    use markdown_core::config::Extensions;
    let rows = (0..25)
        .map(|_| format!("<tr>{}</tr>\n", "<td>x</td>".repeat(12)))
        .collect::<String>();
    let source = format!("<table>\n{rows}</table>\n");
    let f = Fixture::with_extensions(&source, Extensions::HTML_BLOCKS);
    let budget = f.budget.clone();
    let summary = f.batch(256);
    assert_eq!(summary.cells, 300);
    let mut cells = f.read::<PreviewCell>(4, 256);
    cells.extend(f.read_from::<PreviewCell>(4, 256, 44));
    let mut html = f.read::<PreviewHtmlCell>(PREVIEW_HTML_CELLS, 256);
    html.extend(f.read_from::<PreviewHtmlCell>(PREVIEW_HTML_CELLS, 256, 44));
    for (cell, meta) in cells.iter().zip(&html) {
        assert_eq!(
            (cell.node, 1, PREVIEW_HTML_CELL_HTML, 0),
            (meta.node, meta.colspan, meta.flags, meta.reserved)
        );
        assert_eq!(cell.header, 0);
    }
    assert!(
        f.read_from::<PreviewHtmlCell>(PREVIEW_HTML_CELLS, 300, 0)
            .is_empty()
    );
    drop(f);
    released(&budget);

    let f = Fixture::with_extensions(
        "---\ntitle: 中😀\ncount: 2\n---\n\n|A|B|\n|---|---|\n|x|y|\n",
        Extensions::from_bits(0x100).unwrap(),
    );
    let summary = f.batch(256);
    let cells = f.read::<PreviewCell>(4, summary.cells);
    let html = f.read::<PreviewHtmlCell>(PREVIEW_HTML_CELLS, summary.cells);
    assert_eq!(cells.len(), 8);
    for (cell, meta) in cells.iter().zip(&html) {
        assert_eq!(
            (cell.node, 1, 0, 0),
            (meta.node, meta.colspan, meta.flags, meta.reserved)
        );
    }
}

#[test]
fn html_table_checks_row_size_range_unknown_id_and_cancellation() {
    let row_bytes = size_of::<PreviewHtmlCell>() as u32;
    for (table, start, count, row_size, expected) in [
        (PREVIEW_HTML_CELLS, 0, 1, 32, CoreError::InvalidRange),
        (PREVIEW_HTML_CELLS, 0, 257, row_bytes, CoreError::InvalidRange),
        (PREVIEW_HTML_CELLS, 4, 1, row_bytes, CoreError::InvalidRange),
        (PREVIEW_HTML_CELLS, u64::MAX, 1, row_bytes, CoreError::ArithmeticOverflow),
        (16, 0, 1, row_bytes, CoreError::UnknownId),
    ] {
        let f = Fixture::new("|a|b|\n|---|---|\n|x|y|\n");
        let budget = f.budget.clone();
        f.batch(1);
        let mut buffer = [PreviewHtmlCell {
            node: 999,
            colspan: 999,
            flags: 999,
            reserved: 999,
            rowspan: 999,
            reserved_geometry: 999,
        }; 257];
        let before = buffer;
        let mut result = WireReadResult {
            header: WireHeader::of::<WireReadResult>(),
            written: 777,
            next: 888,
            ..Default::default()
        };
        let untouched = result;
        // SAFETY: independent aligned buffer has room for every rejected count.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_read(
                    f.layout,
                    table,
                    start,
                    count,
                    row_size,
                    buffer.as_mut_ptr().cast(),
                    &mut result,
                    &mut WireError::default()
                ),
                wire_error(expected).status
            );
        }
        assert_eq!(buffer, before);
        assert_eq!(result, untouched);
        drop(f);
        released(&budget);
    }
    let f = Fixture::new("|a|b|\n|---|---|\n|x|y|\n");
    f.batch(1);
    let mut row = PreviewHtmlCell {
        node: 999,
        ..Default::default()
    };
    let before = row;
    let mut result = WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        written: 777,
        ..Default::default()
    };
    // SAFETY: original live cancellation flag and independent result/buffer.
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        assert_eq!(
            notepads_markdown_preview_read(
                f.layout,
                PREVIEW_HTML_CELLS,
                0,
                1,
                row_bytes,
                ptr::from_mut(&mut row).cast(),
                &mut result,
                &mut WireError::default()
            ),
            wire_error(CoreError::Cancelled).status
        );
    }
    assert_eq!(row, before);
    assert_eq!(result.written, 777);
    assert_eq!(
        (size_of::<PreviewHtmlCell>(), align_of::<PreviewHtmlCell>()),
        (24, 8)
    );
    assert_eq!(size_of::<PreviewCell>(), 32);
    assert_eq!(size_of::<PreviewSummary>(), 184);
    assert_eq!(
        PREVIEW_HTML_CELL_HTML | PREVIEW_HTML_CELL_COVERED | PREVIEW_HTML_CELL_CAPTION,
        PREVIEW_HTML_CELL_MASK
    );
    let header = include_str!("../../../../../include/notepads/markdown/preview.h");
    assert!(header.contains("#define NOTEPADS_MARKDOWN_PREVIEW_HTML_CELLS 12u"));
}

#[test]
fn picture_sources_keep_one_fallback_image_and_typed_color_variants() {
    use markdown_core::config::Extensions;

    let input = "<picture>\n<source media='(prefers-color-scheme: dark)' srcset='night.png 1x'>\n\
        <source media='( PREFERS-COLOR-SCHEME : LIGHT )' srcset='sun&amp;sky.png'>\n\
        <img src='plain.png' alt='Default photo' width='72' height='54'>\n</picture>\n\n\
        <table><tr><td><picture><source media='(prefers-color-scheme: dark)' srcset='//bad/path'>\
        <source media='(prefers-color-scheme: light)' srcset='small.png 1x, large.png 2x'>\
        <img src='cell.png' alt='Cell photo'></picture></td></tr></table>\n\n\
        Start [<picture><source media='(prefers-color-scheme: dark)' srcset='inline-dark.png'>\
        <img src='inline.png' alt='Inline photo'></picture>](#end) end.\n";
    let extensions =
        Extensions::from_bits(Extensions::HTML_BLOCKS.bits() | Extensions::HTML_INLINE.bits())
            .unwrap();
    let f = Fixture::with_extensions(input, extensions);
    let batch = f.batch(8);
    assert_eq!(batch.images, 3);
    let images = f.read::<PreviewImage>(6, batch.images);
    let bytes = f.read::<u8>(8, batch.blob_bytes);
    let value = |blob: PreviewBlob| {
        std::str::from_utf8(&bytes[blob.start as usize..(blob.start + blob.length) as usize])
            .unwrap()
    };
    let first = images[0];
    assert_eq!((first.width, first.height), (72, 54));
    assert_eq!(value(first.destination), "plain.png");
    assert_eq!(value(first.alt), "Default photo");
    assert_eq!(value(first.light_source), "sun&sky.png");
    assert_eq!(value(first.dark_source), "night.png");
    let second = images[1];
    assert_eq!(value(second.destination), "cell.png");
    assert_eq!(value(second.alt), "Cell photo");
    assert_eq!(
        (second.light_source.length, second.dark_source.length),
        (0, 0)
    );
    let third = images[2];
    assert_eq!(value(third.destination), "inline.png");
    assert_eq!(value(third.alt), "Inline photo");
    assert_eq!(value(third.dark_source), "inline-dark.png");
}

#[test]
fn ruby_units_bind_base_and_annotation_to_their_own_text_rows() {
    use markdown_core::config::Extensions;

    let source =
        "前😀 <ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby> 後\n\n<ruby>茶<rt>ちゃ</rt></ruby> fin";
    let off = Fixture::new(source);
    assert_eq!(off.batch(8).ruby_units, 0);

    let f = Fixture::with_extensions(source, Extensions::HTML_INLINE);
    let batch = f.batch(8);
    assert_eq!((batch.texts, batch.ruby_units), (2, 3));
    assert_eq!(
        f.read::<PreviewRubyUnit>(PREVIEW_RUBY_UNITS, batch.ruby_units),
        [
            PreviewRubyUnit {
                text: 0,
                start: 4,
                base_end: 5,
                end: 7
            },
            PreviewRubyUnit {
                text: 0,
                start: 7,
                base_end: 8,
                end: 9
            },
            PreviewRubyUnit {
                text: 1,
                start: 0,
                base_end: 1,
                end: 3
            },
        ],
    );
    assert_eq!(size_of::<PreviewRubyUnit>(), 16);
}

#[test]
fn wire_layouts_and_preview_factory_keep_legacy_abi_separate() {
    assert_eq!(notepads_markdown_preview_abi_version(), 3);
    assert_eq!(notepads_markdown_core_abi_version(), 3);
    assert_eq!(offset_of!(PreviewKey, width), 72);
    assert_eq!(offset_of!(PreviewBlock, texts), 80);
    assert_eq!(size_of::<PreviewSummary>(), 184);
    assert_eq!(align_of::<PreviewMetric>(), 8);
    let mut cancel = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: writable handle/error storage; old factory remains explicitly 500ms.
    unsafe {
        assert_eq!(
            notepads_markdown_core_cancel_create(ptr::null(), &mut cancel, &mut error),
            0
        );
        assert_eq!((*cancel).value.budget.elapsed_limit_ns(), Some(500_000_000));
        assert_eq!(notepads_markdown_core_cancel_destroy(cancel), 0);
    }
}
