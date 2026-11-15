// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use markdown_core::source::SourceDocument;

struct Fixture {
    model: *mut Owned<CoreModel>,
    cancel: *mut Owned<CoreCancel>,
    cache: *mut Owned<HighlightResult>,
}
impl Fixture {
    fn new(text: &str) -> Self {
        // Language-ID and ABI assertions use deterministic work units, including
        // when unoptimized builds have to load and compile cold syntax grammars.
        let cancel = new_control_cancel(SourceBudget::model_fixture()).unwrap();
        // SAFETY: fixture uniquely owns every handle and full output region.
        unsafe {
            let mut op = (*cancel).value.budget.begin((*cancel).value.token.clone());
            let source = SourceDocument::new(
                SourceKey::new([33; 16], 7).unwrap(),
                text.as_bytes(),
                &mut op,
            )
            .unwrap();
            let model =
                AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::G3), 9, &mut op)
                    .unwrap();
            let slot = Slot::new(op.boundary_work()).unwrap();
            Self {
                model: slot.commit(CoreModel { model }),
                cancel,
                cache: ptr::null_mut(),
            }
        }
    }
    fn build(&mut self) -> i32 {
        let mut error = WireError::default();
        // SAFETY: independent outputs and fixture's live model/cancel.
        unsafe {
            notepads_markdown_highlight_build(self.model, self.cancel, &mut self.cache, &mut error)
        }
    }
    fn summary(&mut self) -> HighlightSummaryWire {
        let mut output = summary();
        let mut error = WireError::default();
        // SAFETY: exclusive live result and independent initialized output.
        assert_eq!(
            unsafe { notepads_markdown_highlight_get_summary(self.cache, &mut output, &mut error) },
            0
        );
        output
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: all calls ended; destroy each uniquely owned handle once.
        unsafe {
            notepads_markdown_highlight_destroy(self.cache);
            notepads_markdown_core_model_destroy(self.model);
            notepads_markdown_core_cancel_destroy(self.cancel);
        }
    }
}
struct LexicalCache(*mut Owned<HighlightCacheResult>);
impl LexicalCache {
    fn new(fixture: &Fixture) -> Self {
        let mut output = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: live shared fixture handles and independent writable outputs.
        assert_eq!(
            unsafe {
                notepads_markdown_highlight_cache_create(
                    fixture.model,
                    fixture.cancel,
                    &mut output,
                    &mut error,
                )
            },
            0
        );
        Self(output)
    }
    fn build(&mut self, fixture: &mut Fixture, nodes: &[u32], reused: &mut u32) -> i32 {
        let mut error = WireError::default();
        // SAFETY: exclusive fixture/cache and disjoint, fully sized outputs/input.
        unsafe {
            notepads_markdown_highlight_destroy(fixture.cache);
            fixture.cache = ptr::null_mut();
            notepads_markdown_highlight_build_selected(
                fixture.model,
                fixture.cancel,
                nodes.as_ptr(),
                nodes.len() as u32,
                self.0,
                &mut fixture.cache,
                reused,
                &mut error,
            )
        }
    }
}
impl Drop for LexicalCache {
    fn drop(&mut self) {
        // SAFETY: all cache calls ended and this wrapper uniquely owns the handle.
        unsafe { notepads_markdown_highlight_cache_destroy(self.0) };
    }
}
fn code_nodes(fixture: &Fixture) -> Vec<u32> {
    // SAFETY: fixture owns a live shared model during this inspection.
    let model = unsafe { &(*fixture.model).value.model };
    model
        .ir()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.kind == markdown_core::ir::NodeKind::CodeBlock && node.flow == model.main_flow_id()
        })
        .map(|(index, _)| index as u32 + 1)
        .collect()
}
fn copy(fixture: &mut Fixture) -> (Vec<HighlightBlockWire>, Vec<HighlightSpanWire>) {
    let first = fixture.summary();
    let mut rows = vec![HighlightBlockWire::default(); first.blocks as usize];
    let mut spans = vec![HighlightSpanWire::default(); first.spans as usize];
    let mut output = summary();
    let mut error = WireError::default();
    // SAFETY: live fixture and fully sized, disjoint writable buffers.
    assert_eq!(
        unsafe {
            notepads_markdown_highlight_copy(
                fixture.cache,
                rows.as_mut_ptr(),
                rows.len() as u32,
                size_of::<HighlightBlockWire>() as u32,
                spans.as_mut_ptr(),
                spans.len() as u32,
                size_of::<HighlightSpanWire>() as u32,
                &mut output,
                &mut error,
            )
        },
        0
    );
    (rows, spans)
}
fn summary() -> HighlightSummaryWire {
    HighlightSummaryWire {
        header: HighlightHeader::of::<HighlightSummaryWire>(),
        ..Default::default()
    }
}

#[test]
fn highlight_abi_real_model_roles_and_surrogates_survive_original_owner_retirement() {
    let mut fixture = Fixture::new(
        "```json\r\n{\"a\": 42, \"b\": \"中😀\"}\r\n```\r\n\r\n```unknown\nplain\n```\n",
    );
    assert_eq!(fixture.build(), 0);
    let first = fixture.summary();
    assert_eq!(
        (first.blocks, first.complete_blocks, first.skipped_blocks),
        (2, 1, 1)
    );
    let mut rows = vec![HighlightBlockWire::default(); first.blocks as usize];
    let mut spans = vec![HighlightSpanWire::default(); first.spans as usize];
    let mut output = summary();
    let mut error = WireError::default();
    // SAFETY: all pointer capacities match allocated vectors, and regions do not alias.
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(fixture.cancel), 0);
        notepads_markdown_core_model_destroy(fixture.model);
        fixture.model = ptr::null_mut();
        notepads_markdown_core_cancel_destroy(fixture.cancel);
        fixture.cancel = ptr::null_mut();
        assert_eq!(
            notepads_markdown_highlight_copy(
                fixture.cache,
                rows.as_mut_ptr(),
                rows.len() as u32,
                40,
                spans.as_mut_ptr(),
                spans.len() as u32,
                16,
                &mut output,
                &mut error
            ),
            0
        );
    }
    assert_eq!(output.model.revision, 7);
    assert_eq!(output.model.generation, 9);
    assert_eq!(
        (
            rows[0].state,
            rows[0].language,
            rows[1].state,
            rows[1].reason
        ),
        (1, 3, 2, 1)
    );
    let text = "{\"a\": 42, \"b\": \"中😀\"}\n";
    let number = text.find("42").unwrap() as u32;
    let selected =
        &spans[rows[0].spans_start as usize..(rows[0].spans_start + rows[0].spans_count) as usize];
    assert!(
        selected
            .iter()
            .any(|span| span.role == 3 && span.start_utf16 == number && span.length_utf16 == 2)
    );
    let mut boundaries = vec![0];
    let mut units = 0;
    for ch in text.chars() {
        units += ch.len_utf16() as u32;
        boundaries.push(units);
    }
    for span in selected {
        assert!(span.length_utf16 > 0 && span.role <= 6 && span.reserved == 0);
        assert!(boundaries.contains(&span.start_utf16));
        assert!(boundaries.contains(&(span.start_utf16 + span.length_utf16)));
    }
    assert_eq!(rows[0].logical_end - rows[0].logical_start, units as u64);
}

#[test]
fn highlight_abi_exports_extended_language_ids() {
    let mut fixture = Fixture::new(concat!(
        "```c\nint value = 42;\n```\n\n```Rscript\nx <- 42\n```\n\n```scala\nval x = 42\n```\n\n",
        "```objc\nNSString *label = @\"preview\";\n```\n\n",
        "```objc++\nstd::vector<int> widths = {42};\n```\n\n",
        "```php\nfunction title() { return 'preview'; }\n```\n\n",
        "```html+php\n<p><?= title() ?></p>\n```\n\n",
        "```latex\n\\section{Preview}\n```\n\n",
        "```clj\n(defn label [] \"preview\")\n```\n\n",
        "```erl\nlabel() -> \"preview\".\n```\n\n",
        "```svg\n<svg viewBox=\"0 0 40 40\" />\n```\n\n",
        "```gradle\nplugins { id 'java' }\n```\n\n",
        "```ocaml\nlet label = \"Preview\"\n```\n\n",
        "```common-lisp\n(defun label () \"Preview\")\n```\n\n",
        "```tcl\nset label \"Preview\"\n```\n\n",
        "```patch\n@@ -1 +1 @@\n-before\n+after\n```\n\n",
        "```properties\n# Preview\npreview.name=Preview\n```\n",
    ));
    assert_eq!(fixture.build(), 0);
    let first = fixture.summary();
    let languages = [
        19, 20, 21, 26, 27, 28, 29, 30, 31, 32, 16, 33, 34, 35, 36, 12, 37,
    ];
    let count = languages.len() as u32;
    assert_eq!((first.blocks, first.complete_blocks), (count, count));
    let mut rows = vec![HighlightBlockWire::default(); languages.len()];
    let mut spans = vec![HighlightSpanWire::default(); first.spans as usize];
    let mut output = summary();
    let mut error = WireError::default();
    // SAFETY: The live fixture and independent output buffers match the advertised capacities.
    unsafe {
        assert_eq!(
            notepads_markdown_highlight_copy(
                fixture.cache,
                rows.as_mut_ptr(),
                rows.len() as u32,
                40,
                spans.as_mut_ptr(),
                spans.len() as u32,
                16,
                &mut output,
                &mut error
            ),
            0
        );
    }
    for (row, expected) in rows.iter().zip(languages) {
        assert_eq!((row.language, row.state, row.reason), (expected, 1, 0));
        assert!(row.spans_count > 0);
    }
}

#[test]
fn highlight_abi_shell_language_and_aliases_survive_owner_retirement() {
    let body = "# 中😀\nprintf '%s\\n' 'Markdown literal * * *'\nif true; then\n    echo \"中😀 $value\"\nfi\n";
    let source: String = ["sh", "bash", "SHELL"]
        .into_iter()
        .map(|alias| format!("```{alias}\r\n{}```\r\n\r\n", body.replace('\n', "\r\n")))
        .collect();
    let mut fixture = Fixture::new(&source);
    assert_eq!(fixture.build(), 0);
    let first = fixture.summary();
    assert_eq!(
        (
            first.blocks,
            first.complete_blocks,
            first.skipped_blocks,
            first.failed_blocks
        ),
        (3, 3, 0, 0)
    );
    let mut rows = vec![HighlightBlockWire::default(); first.blocks as usize];
    let mut spans = vec![HighlightSpanWire::default(); first.spans as usize];
    let mut output = summary();
    let mut error = WireError::default();
    // SAFETY: the fixture owns all live handles and disjoint, fully sized output buffers.
    // The snapshot's accepted-model pin remains live after its original handles retire.
    unsafe {
        notepads_markdown_core_model_destroy(fixture.model);
        fixture.model = ptr::null_mut();
        notepads_markdown_core_cancel_destroy(fixture.cancel);
        fixture.cancel = ptr::null_mut();
        assert_eq!(
            notepads_markdown_highlight_copy(
                fixture.cache,
                rows.as_mut_ptr(),
                rows.len() as u32,
                40,
                spans.as_mut_ptr(),
                spans.len() as u32,
                16,
                &mut output,
                &mut error
            ),
            0
        );
    }
    let canonical =
        &spans[rows[0].spans_start as usize..(rows[0].spans_start + rows[0].spans_count) as usize];
    let position = |needle: &str| body[..body.find(needle).unwrap()].encode_utf16().count() as u32;
    for (needle, role) in [
        ("中", 1),
        ("printf", 6),
        ("Markdown literal", 2),
        ("if true", 4),
    ] {
        assert!(
            canonical.iter().any(|span| span.role == role
                && span.start_utf16 <= position(needle)
                && position(needle) < span.start_utf16 + span.length_utf16),
            "{needle}"
        );
    }
    let mut boundaries = vec![0];
    let mut units = 0;
    for scalar in body.chars() {
        units += scalar.len_utf16() as u32;
        boundaries.push(units);
    }
    for row in &rows {
        assert_eq!((row.language, row.state, row.reason), (7, 1, 0));
        assert_eq!(row.logical_end - row.logical_start, units as u64);
        let tokens = &spans[row.spans_start as usize..(row.spans_start + row.spans_count) as usize];
        assert_eq!(tokens.len(), canonical.len());
        for (actual, expected) in tokens.iter().zip(canonical) {
            assert_eq!(
                (
                    actual.start_utf16,
                    actual.length_utf16,
                    actual.role,
                    actual.reserved
                ),
                (
                    expected.start_utf16,
                    expected.length_utf16,
                    expected.role,
                    0
                )
            );
            assert!(boundaries.contains(&actual.start_utf16));
            assert!(boundaries.contains(&(actual.start_utf16 + actual.length_utf16)));
        }
    }
}

#[test]
fn highlight_abi_bad_prefix_and_insufficient_copy_do_not_poison_or_publish_summary() {
    let mut fixture = Fixture::new("```rust\nlet value = 42;\n```\n");
    assert_eq!(fixture.build(), 0);
    let mut prefix = HighlightHeader {
        abi_version: 0,
        struct_size: size_of::<HighlightSummaryWire>() as u32,
    };
    let mut error = WireError::default();
    // SAFETY: mismatching prefix authorizes only this actual eight-byte region.
    assert_eq!(
        unsafe {
            notepads_markdown_highlight_get_summary(
                fixture.cache,
                (&mut prefix as *mut HighlightHeader).cast(),
                &mut error,
            )
        },
        STATUS_ABI_MISMATCH
    );
    let first = fixture.summary();
    let mut output = summary();
    output.spans = 99;
    // SAFETY: zero capacities permit null arrays; summary/error are full independent buffers.
    unsafe {
        assert_eq!(
            notepads_markdown_highlight_copy(
                fixture.cache,
                ptr::null_mut(),
                0,
                40,
                ptr::null_mut(),
                0,
                16,
                &mut output,
                &mut error
            ),
            9
        );
        assert_eq!(output.spans, 99);
        assert_eq!(error.requested, 1);
        assert_eq!(
            notepads_markdown_highlight_copy(
                fixture.cache,
                ptr::null_mut(),
                0,
                39,
                ptr::null_mut(),
                0,
                16,
                &mut output,
                &mut error
            ),
            STATUS_ABI_MISMATCH
        );
    }
    assert_eq!(fixture.summary().spans, first.spans);
    // SAFETY: exclusive test result can simulate a previously contained panic gate.
    unsafe {
        (*fixture.cache).value.poisoned = true;
    }
    assert_eq!(
        unsafe { notepads_markdown_highlight_get_summary(fixture.cache, &mut output, &mut error) },
        5
    );
    assert_eq!(output.spans, 99);
}

#[test]
fn highlight_abi_wrong_root_or_canceled_build_publishes_no_handle() {
    let mut fixture = Fixture::new("```json\n42\n```\n");
    let mut foreign = Fixture::new("other");
    let mut error = WireError::default();
    // SAFETY: separate live source roots; build must reject without publishing a result.
    unsafe {
        assert_eq!(
            notepads_markdown_highlight_build(
                fixture.model,
                foreign.cancel,
                &mut fixture.cache,
                &mut error
            ),
            wire_error(CoreError::WrongBudget).status
        );
        assert!(fixture.cache.is_null());
        assert_eq!(notepads_markdown_core_cancel_request(fixture.cancel), 0);
        assert_eq!(fixture.build(), 4);
        assert!(fixture.cache.is_null());
        // No source is poisoned by a foreign flag: its own independent build still works.
        assert_eq!(foreign.build(), 0);
    }
}

#[test]
fn highlight_abi_large_token_copy_is_charged_in_legal_work_chunks() {
    let line = "1,".repeat(1400);
    let code = format!("```json\n[{line}\n{line}\n{line}0]\n```\n");
    let mut fixture = Fixture::new(&code);
    assert_eq!(fixture.build(), 0);
    let first = fixture.summary();
    assert_eq!(first.complete_blocks, 1);
    assert!(first.spans as usize * size_of::<HighlightSpanWire>() > 65_536);
    let mut rows = vec![HighlightBlockWire::default(); first.blocks as usize];
    let mut spans = vec![HighlightSpanWire::default(); first.spans as usize];
    let mut output = summary();
    let mut error = WireError::default();
    // SAFETY: exact full capacities and independent initialized output buffers.
    assert_eq!(
        unsafe {
            notepads_markdown_highlight_copy(
                fixture.cache,
                rows.as_mut_ptr(),
                rows.len() as u32,
                40,
                spans.as_mut_ptr(),
                spans.len() as u32,
                16,
                &mut output,
                &mut error,
            )
        },
        0
    );
    assert_eq!(output.spans, first.spans);
}

#[test]
fn highlight_abi_selected_late_blocks_reuse_and_survive_all_input_owner_retirement() {
    let text: String = (0..72)
        .map(|index| format!("```json\n{{\"value\": {index}}}\n```\n\n"))
        .collect();
    let mut fixture = Fixture::new(&text);
    let nodes = code_nodes(&fixture);
    assert_eq!(nodes.len(), 72);
    let selected = &nodes[68..71];
    let mut lexical = LexicalCache::new(&fixture);
    let mut reused = u32::MAX;
    assert_eq!(lexical.build(&mut fixture, selected, &mut reused), 0);
    assert_eq!(reused, 0);
    let first = fixture.summary();
    assert_eq!(
        (first.blocks, first.complete_blocks, first.entry_limited),
        (3, 3, 0)
    );
    let (rows, spans) = copy(&mut fixture);
    assert_eq!(
        rows.iter().map(|row| row.node).collect::<Vec<_>>(),
        selected
    );
    assert_eq!(lexical.build(&mut fixture, selected, &mut reused), 0);
    assert_eq!(reused, 3);
    assert_eq!(fixture.summary().spans, first.spans);
    // SAFETY: borrow the live model only long enough to retain the budget root.
    let budget = unsafe { (*fixture.model).value.model.budget().clone() };
    // SAFETY: fixture transfers each live model/cancel once. Selected snapshot owns
    // its accepted model independently of the retired source handles and cache.
    unsafe {
        notepads_markdown_core_model_destroy(fixture.model);
        fixture.model = ptr::null_mut();
        notepads_markdown_core_cancel_destroy(fixture.cancel);
        fixture.cancel = ptr::null_mut();
    }
    let (again, tokens) = copy(&mut fixture);
    for (actual, expected) in again.iter().zip(&rows) {
        assert_eq!(
            (
                actual.node,
                actual.logical_start,
                actual.logical_end,
                actual.spans_count
            ),
            (
                expected.node,
                expected.logical_start,
                expected.logical_end,
                expected.spans_count
            )
        );
    }
    for (actual, expected) in tokens.iter().zip(&spans) {
        assert_eq!(
            (
                actual.start_utf16,
                actual.length_utf16,
                actual.role,
                actual.reserved
            ),
            (
                expected.start_utf16,
                expected.length_utf16,
                expected.role,
                expected.reserved
            )
        );
    }
    // SAFETY: the immutable snapshot is uniquely owned and all reads ended.
    unsafe {
        notepads_markdown_highlight_destroy(fixture.cache);
        fixture.cache = ptr::null_mut();
    }
    assert_eq!(budget.usage().source_owners, 0);
    assert_eq!(budget.usage().source_capacity, 0);
    assert!(budget.usage().auxiliary_capacity > 0);
    drop(lexical);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn highlight_abi_selected_cache_reuses_text_with_new_model_coordinates() {
    let mut fixture = Fixture::new("```json\n{\"value\": 42}\n```\n");
    let mut lexical = LexicalCache::new(&fixture);
    let nodes = code_nodes(&fixture);
    let mut reused = u32::MAX;
    assert_eq!(lexical.build(&mut fixture, &nodes, &mut reused), 0);
    assert_eq!(reused, 0);
    let (before, tokens_before) = copy(&mut fixture);
    // SAFETY: same root, new accepted revision. Only the model handle is replaced;
    // the first snapshot keeps its model alive until the next selected build.
    unsafe {
        let cancel = &(*fixture.cancel).value;
        let mut op = cancel.budget.begin(cancel.token.clone());
        let source = SourceDocument::new(
            SourceKey::new([33; 16], 8).unwrap(),
            b"A preceding paragraph.\n\n```json\n{\"value\": 42}\n```\n",
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::new(Profile::G3),
            10,
            &mut op,
        )
        .unwrap();
        let slot = Slot::new(op.boundary_work()).unwrap();
        notepads_markdown_core_model_destroy(fixture.model);
        fixture.model = slot.commit(CoreModel { model });
    }
    let nodes = code_nodes(&fixture);
    assert_eq!(lexical.build(&mut fixture, &nodes, &mut reused), 0);
    assert_eq!(reused, 1);
    let model = fixture.summary().model;
    assert_eq!((model.revision, model.generation), (8, 10));
    let (after, tokens_after) = copy(&mut fixture);
    assert_ne!(after[0].node, before[0].node);
    assert!(after[0].logical_start > before[0].logical_start);
    assert_eq!(tokens_before.len(), tokens_after.len());
    for (actual, expected) in tokens_after.iter().zip(tokens_before) {
        assert_eq!(
            (actual.start_utf16, actual.length_utf16, actual.role),
            (expected.start_utf16, expected.length_utf16, expected.role)
        );
    }
}

#[test]
fn highlight_abi_selected_invalid_shapes_and_nodes_do_not_poison_or_publish() {
    let mut fixture = Fixture::new("paragraph\n\n```json\n42\n```\n\n```json\n43\n```\n");
    let nodes = code_nodes(&fixture);
    let mut lexical = LexicalCache::new(&fixture);
    let mut error = WireError::default();
    let mut reused = 99;
    let misaligned_nodes = nodes.as_ptr().cast::<u8>().wrapping_add(1).cast::<u32>();
    let misaligned_reused = (&mut reused as *mut u32)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<u32>();
    for (input, count, output) in [
        (nodes.as_ptr(), 0, &mut reused as *mut u32),
        (nodes.as_ptr(), 65, &mut reused as *mut u32),
        (ptr::null(), 1, &mut reused as *mut u32),
        (misaligned_nodes, 1, &mut reused as *mut u32),
        (nodes.as_ptr(), 1, ptr::null_mut()),
        (nodes.as_ptr(), 1, misaligned_reused),
    ] {
        // SAFETY: invalid shapes must be rejected before dereferencing arrays or
        // outputs. Actual handle/error storage is valid and independently owned.
        assert_eq!(
            unsafe {
                notepads_markdown_highlight_build_selected(
                    fixture.model,
                    fixture.cancel,
                    input,
                    count,
                    lexical.0,
                    &mut fixture.cache,
                    output,
                    &mut error,
                )
            },
            STATUS_INVALID_ARGUMENT
        );
        assert!(fixture.cache.is_null());
        assert_eq!(reused, 99);
    }
    for invalid in [
        vec![0],
        vec![1],
        vec![u32::MAX],
        vec![nodes[0], nodes[0]],
        vec![nodes[1], nodes[0]],
    ] {
        assert_eq!(
            lexical.build(&mut fixture, &invalid, &mut reused),
            wire_error(CoreError::InvalidRange).status
        );
        assert!(fixture.cache.is_null());
        assert_eq!(reused, 99);
    }
    assert_eq!(lexical.build(&mut fixture, &nodes, &mut reused), 0);
    assert_eq!((fixture.summary().complete_blocks, reused), (2, 0));
    // SAFETY: simulate a contained prior panic's poison gate on an exclusive cache.
    unsafe { (*lexical.0).value.poisoned = true };
    reused = 99;
    assert_eq!(
        lexical.build(&mut fixture, &nodes, &mut reused),
        wire_error(CoreError::InvalidState).status
    );
    assert!(fixture.cache.is_null());
    assert_eq!(reused, 99);
}

#[test]
fn highlight_abi_selected_wrong_roots_cancellation_and_bad_error_prefix_are_nonpublishing() {
    let mut fixture = Fixture::new("```json\n42\n```\n");
    let foreign = Fixture::new("```json\n42\n```\n");
    let nodes = code_nodes(&fixture);
    let mut lexical = LexicalCache::new(&fixture);
    let foreign_cache = LexicalCache::new(&foreign);
    let mut error = WireError::default();
    let mut reused = 99;
    let mut output = ptr::null_mut();
    // SAFETY: all handles are live, but the intentionally mixed roots must fail.
    unsafe {
        assert_eq!(
            notepads_markdown_highlight_cache_create(
                fixture.model,
                foreign.cancel,
                &mut output,
                &mut error
            ),
            wire_error(CoreError::WrongBudget).status
        );
        assert!(output.is_null());
        for (cancel, cache) in [
            (foreign.cancel, lexical.0),
            (fixture.cancel, foreign_cache.0),
        ] {
            assert_eq!(
                notepads_markdown_highlight_build_selected(
                    fixture.model,
                    cancel,
                    nodes.as_ptr(),
                    nodes.len() as u32,
                    cache,
                    &mut fixture.cache,
                    &mut reused,
                    &mut error,
                ),
                wire_error(CoreError::WrongBudget).status
            );
            assert!(fixture.cache.is_null());
            assert_eq!(reused, 99);
        }
        let mut prefix = WireHeader {
            abi_version: 0,
            struct_size: size_of::<WireError>() as u32,
        };
        assert_eq!(
            notepads_markdown_highlight_build_selected(
                fixture.model,
                fixture.cancel,
                nodes.as_ptr(),
                nodes.len() as u32,
                lexical.0,
                &mut fixture.cache,
                &mut reused,
                (&mut prefix as *mut WireHeader).cast(),
            ),
            STATUS_ABI_MISMATCH
        );
        assert!(fixture.cache.is_null());
        assert_eq!(reused, 99);
        assert_eq!(notepads_markdown_core_cancel_request(fixture.cancel), 0);
        assert_eq!(
            lexical.build(&mut fixture, &nodes, &mut reused),
            wire_error(CoreError::Cancelled).status
        );
        assert!(fixture.cache.is_null());
        assert_eq!(reused, 99);
        assert!(!(*lexical.0).value.poisoned);
        assert_eq!(
            notepads_markdown_highlight_cache_create(
                fixture.model,
                fixture.cancel,
                &mut output,
                &mut error
            ),
            wire_error(CoreError::Cancelled).status
        );
        assert!(output.is_null());
    }
}
