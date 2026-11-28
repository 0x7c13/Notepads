# Notepads Markdown third-party forks

This file explains every retained fork. [vendor/forks.json](vendor/forks.json) binds
the exact upstream archive SHA and lists every current modified, added and omitted
file with hashes. Dependency versions were retained; registry conversion is not an upgrade.
The Core and All Rust test entry points verify the listed local file hashes,
omitted paths and counts in this document. These checks run offline.

## Current dependency policy

- Keep a local fork when current product code calls an API or relies on execution/ownership behavior absent from that exact upstream release.
- Use registry dependencies and the committed lock for packages without required local changes.
- Keep the single development-only renderer compatibility fork separate from the product dependency graph.
- Keep first-party resource interfaces under crates/, not vendor/.
- Cargo.toml is the active manifest. The archive's Cargo-generated manifest is the comparison baseline for that path. Cargo.toml.orig retains the original author manifest where supplied.
- Original licenses are unchanged. The self-contained notices tree is deployed by one Consumer Content glob.

## Registry conversions

| Package | Version | Decision |
| --- | --- | --- |
| dugong | 0.9.2 | Unmodified facade or retired development-backend control fork; the current development adapter uses the original registry implementation. |
| dugong-graphlib | 0.9.2 | Unmodified facade or retired development-backend control fork; the current development adapter uses the original registry implementation. |
| merman | 0.9.2 | Unmodified facade or retired development-backend control fork; the current development adapter uses the original registry implementation. |
| adler2 | 2.0.1 | No local implementation changes; only path wiring or import ignore rules differed. |
| byteorder-lite | 0.1.0 | No local implementation changes; only path wiring or import ignore rules differed. |
| color_quant | 1.1.0 | No local implementation changes; only path wiring or import ignore rules differed. |
| flate2 | 1.1.10 | No local implementation changes; only path wiring or import ignore rules differed. |
| miniz_oxide | 0.8.9 | No local implementation changes; only path wiring or import ignore rules differed. |
| miniz_oxide | 0.9.1 | No local implementation changes; only path wiring or import ignore rules differed. |
| quick-error | 2.0.1 | No local implementation changes; only path wiring or import ignore rules differed. |
| unicode-segmentation | 1.13.3 | The official crate supplies grapheme segmentation. SVG derives its scalar roles from the pinned Unicode 17 GraphemeBreakProperty data in its own adapter. |

The main lock merges same-version path/registry duplicates for adler2, flate2,
miniz_oxide 0.9.1 and unicode-segmentation. The parsed name/version sets and enabled features were
independently compared with the previous graph. Product FFI normal/build dependencies
do not include merman, merman-render, dugong or dugong-graphlib.

## Retained forks

The following descriptions refer to current call sites, not to every public API
or every upstream feature. Test source names identify relevant regressions; they do
not claim every upstream vendor test is part of the normal module suite.

### comrak 0.56.0

Path: `vendor/comrak-0.56.0`; scope: **product**.

Expose native/direct NativeInput + DirectCompiler and Host-controlled parse execution rather than ordinary parse_document output. Preserve typed accepted value/source claims, source-map proof stages, definition winner identities and AST lifetime/retirement; charge real storage ownership and preserve first failure/cancellation. The CommonMark and GitHub profiles share this accepted-source path. The GitHub profile enables tables, tasks, strikethrough, and extended autolinks. URL candidates use the raw selector with inline ownership barriers. Email matching uses the decoded text tree; a host policy can suppress promotion in projected literal regions using the candidate's actual copied source bounds. Authored block-code terminals retain their separate source atoms.

Current consumers: `crates/markdown-core/src/parse/prepared.rs`, `crates/markdown-core/src/parse/direct.rs`, `crates/markdown-core/src/parse/direct_lifecycle.rs`, `crates/markdown-core/src/parse/direct_graph.rs`.

The configured GFM inline HTML policy keeps filtered raw delimiters in accepted source and leaves inline content outside actual HTML code bodies available to formatting and link selection. Shadow parsing uses the same policy. Raw-block projection shares the GFM boundary matcher.

The opt-in native heading identifier parser retains the original `{#id}` bytes as a heading-owned syntax claim and passes the title alone to inline parsing.

Native named footnotes use the metered label table and retain the definition identity and reference syntax as accepted provenance. Primary and strict G4 shadow parsing share the same definition winners.
Native inline footnotes retain paired delimiter spans and a borrowed body in the same sealed source buffer. The bounded matcher shares the link and opaque-token rules with inline parsing. Strict G4 treats the original note as one owner, then selects links again inside its detached body.

Native callouts extend real blockquote nodes with an authored marker, canonical kind, disclosure default and optional title paragraph. The title uses mapped inline parsing without block definition recognition.

Highlight and script scopes retain accepted delimiter ranges. Strict G4 uses the original matched delimiter pairs to bound URLs that start inside the formatted content.

Emoji shortcodes use a metered ASCII alias scan and a typed text transformation. The complete authored alias maps atomically to the pinned Unicode sequence. The Core replay verifier checks the original alias and decoded sequence against the fixed lookup data.

Regression sources: `crates/markdown-core/src/parse/*tests.rs`, `crates/markdown-core/src/outline/tests.rs`, `crates/markdown-core/tests/source_baselines.rs`, `crates/markdown-core/tests/parser_model_ranges.rs`, `crates/markdown-core/tests/g3_models.rs`, `vendor/comrak-0.56.0/src/native/**/*tests.rs`, `vendor/comrak-0.56.0/src/tests*.rs`.

Relative to the exact release: 32 modified files, 37 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### lalrpop-util 0.23.2

Path: `vendor/merman-bounded/lalrpop-util`; scope: **product**.

Add optional ParserDefinition memory_scope/memory_error/memory_checkpoint hooks and funded state/value stack growth. Admit complete stack groups before allocation/commit, preserve the original operation first fault, and reject controlled error recovery without changing legacy grammars.

Current consumers: `vendor/merman-bounded/merman-core/src/generated/lalrpop/flowchart_grammar.rs:549-565`, `vendor/merman-bounded/merman-core/src/diagrams/flowchart_grammar.lalrpop`.

Regression sources: `vendor/merman-bounded/lalrpop-util/src/state_machine.rs::tests`, `crates/markdown-mermaid/src/layout/tests.rs`, `vendor/merman-bounded/merman-core/src/diagrams/flowchart/native.rs::tests`.

Relative to the exact release: 2 modified files, 1 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### merman-core 0.9.2

Path: `vendor/merman-bounded/merman-core`; scope: **product**.

Add parse_flowchart_native_controlled, NativeParsePolicy/NativeFeature/NativeParseError and the native typed flowchart input model consumed by the product. Carry the original MemoryScope/RegionHandle through OperationControl; meter lexical/grammar/model/semantic allocations and work, fallible clones, fixed first faults, and checkpointed entity decoding. Replace flowchart class-definition IndexMap and selected hash containers with funded deterministic native_collections types; adapt Agentflow legacy projection accordingly.

Current consumers: `crates/markdown-mermaid/src/native_state.rs:97`, `crates/markdown-mermaid/src/layout/mod.rs`, `crates/markdown-mermaid/src/funding.rs`, `crates/markdown-mermaid/src/labels/scan.rs`, `crates/markdown-ffi/src/core/diagram.rs`.

Regression sources: `crates/markdown-mermaid/src/layout/tests.rs`, `crates/markdown-mermaid/src/native_state.rs::tests`, `crates/markdown-mermaid/src/funding_tests.rs`, `vendor/merman-bounded/merman-core/src/diagrams/flowchart/native.rs::tests`, `vendor/merman-bounded/merman-core/src/native_collections.rs::tests`.

Relative to the exact release: 20 modified files, 4 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### merman-render 0.9.2

Path: `vendor/merman-bounded/merman-render`; scope: **development-test-only**.

Development/test-only compatibility with the product core model: seven Rust files accept its OrderedMap class definitions and NativeHashMap tooltips. The old local Dugong memory/work bridge was removed. Cargo.toml pins registry dugong to =0.9.2; root Cargo.lock pins dugong-graphlib to 0.9.2. Upstream render cannot accept these core collection types (17 independently reproduced E0308 errors).

Current consumers: `windows/markdown-diagram-platform/src/tests.rs:252`, `windows/markdown-diagram-platform/src/tests.rs:473`, `windows/markdown-diagram-platform/src/native_measure.rs (cfg(test) module)`, `windows/markdown-diagram-platform/src/host.rs (cfg(test) impls)`.

Regression sources: `windows/markdown-diagram-platform/src/tests.rs::real_native_probe_preserves_formatted_tspan_baseline_and_space_semantics`, `windows/markdown-diagram-platform/src/tests.rs::development_new_backend_reuses_four_original_flowchart_sources`.

Relative to the exact release: 9 modified files, 0 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Use the pinned Rust toolchain atomic try_update spelling with unchanged orderings and closure.

### ratex-font 0.1.15

Path: `vendor/ratex-bounded/ratex-font`; scope: **product**.

Replace lazily allocated process-wide symbol HashMaps with fixed read-only sorted name/codepoint indexes that preserve original first-match lookup semantics. Expose bounded_readonly_bytes and count fixed symbol/metric payload for the enclosing math asset admission.

Current consumers: `crates/markdown-math/src/render.rs:22`, `crates/markdown-math/src/fonts.rs`, `bounded RaTeX parser/layout font lookups`.

Regression sources: `vendor/ratex-bounded/ratex-font/src/symbols.rs::tests`, `crates/markdown-math/src/render.rs::tests`, `crates/markdown-math/src/fixup_tests.rs`, `crates/markdown-math/src/funding/tests.rs`, `crates/markdown-math/src/pending/tests.rs`, `crates/markdown-math/src/font_tests.rs`.

Relative to the exact release: 2 modified files, 1 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### ratex-layout 0.1.15

Path: `vendor/ratex-bounded/ratex-layout`; scope: **product**.

Add layout_with_control/layout_with_storage and controlled validation/display traversal. Bound layout boxes, auxiliary data, generated path/string output, recursion and fallible writer operations. Support the existing Chinese text run/fallback measurement bridge without allowing font/layout objects to cross FFI. Reuse bounded font-piece assembly for tall ordinary paired parentheses so valid sums do not fall back, and compose text family/weight/shape across nested commands while preserving default symbol and legacy text-font selection.

Current consumers: `crates/markdown-math/src/render.rs:169-203`, `crates/markdown-math/src/outline.rs`.

Regression sources: `vendor/ratex-bounded/ratex-layout/src/resource_limits.rs::tests`, `crates/markdown-math/src/render.rs::tests`, `crates/markdown-math/src/fixup_tests.rs`, `crates/markdown-math/src/funding/tests.rs`, `crates/markdown-math/src/pending/tests.rs`, `crates/markdown-math/src/font_tests.rs`.

Relative to the exact release: 9 modified files, 2 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### ratex-lexer 0.1.15

Path: `vendor/ratex-bounded/ratex-lexer`; scope: **product**.

Add controlled/storage-aware token production and fallible bounded token/string cloning. Thread the existing operation through lexer work/admission and preserve legacy entry points.

Current consumers: `vendor/ratex-bounded/ratex-parser/src/parser.rs`, `vendor/ratex-bounded/ratex-parser/src/macro_expander.rs`.

Regression sources: `vendor/ratex-bounded/ratex-lexer/src/lexer.rs::tests`, `crates/markdown-math/src/render.rs::tests`, `crates/markdown-math/src/fixup_tests.rs`, `crates/markdown-math/src/funding/tests.rs`, `crates/markdown-math/src/pending/tests.rs`, `crates/markdown-math/src/font_tests.rs`.

Relative to the exact release: 3 modified files, 2 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### ratex-parser 0.1.15

Path: `vendor/ratex-bounded/ratex-parser`; scope: **product**.

Add parse_with_control/parse_with_storage, bounded AST/token/string clones, storage/depth/expansion admission and checkpoints throughout parser, functions and macro expansion. Add borrowed fixed builtin macro views and bounded_readonly_bytes to avoid uncharged process-global registries. Reject unsupported bounded builders before entering unsafe/unfunded legacy construction; preserve legacy parse compatibility and elementary math behavior.

Current consumers: `crates/markdown-math/src/render.rs:164`, `crates/markdown-math/src/render.rs:22`, `vendor/ratex-bounded/ratex-layout/src/engine.rs`.

Regression sources: `vendor/ratex-bounded/ratex-parser/src/macro_expander/resource_tests.rs`, `vendor/ratex-bounded/ratex-parser/tests/*`, `crates/markdown-math/src/render.rs::tests`, `crates/markdown-math/src/fixup_tests.rs`, `crates/markdown-math/src/funding/tests.rs`, `crates/markdown-math/src/pending/tests.rs`, `crates/markdown-math/src/font_tests.rs`.

Relative to the exact release: 51 modified files, 3 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### ratex-types 0.1.15

Path: `vendor/ratex-bounded/ratex-types`; scope: **product**.

Add execution.rs: explicit borrowed cancellation/deadline, phase/resource limits, checked work and first-fault state. Add storage.rs: operation-local cumulative allocation admission with old/new overlap, storage family/slot/depth controls, fallible bounded owners and snapshots.

Current consumers: `crates/markdown-math/src/lib.rs`, `crates/markdown-math/src/render.rs`, `crates/markdown-math/src/pending.rs`, `all bounded RaTeX fork crates`.

Regression sources: `crates/markdown-math/src/render.rs::tests`, `crates/markdown-math/src/fixup_tests.rs`, `crates/markdown-math/src/funding/tests.rs`, `crates/markdown-math/src/pending/tests.rs`, `crates/markdown-math/src/font_tests.rs`.

Relative to the exact release: 1 modified files, 3 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### data-url 0.3.2

Path: `vendor/svg-bounded/dependencies/data-url-0.3.2`; scope: **product**.

Keep the fork. It adds request-funded MIME/header/parameter storage, exact decoded child admission, fallible body output, cancellation/work checkpoints inside percent/base64/header scans, and typed first-error propagation. The product usvg image/source classification paths call controlled::DataUrl::process and inspect controlled::Error::Resource; upstream has neither the feature nor this API.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/image.rs:510`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/source_kind.rs:756`.

Regression sources: `src/resource_tests.rs (paid allocation/free/refund, aggregate/child limits, reserve faults, cancellation and syntax parity)`.

Relative to the exact release: 4 modified files, 4 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### fontdb 0.24.0

Path: `vendor/svg-bounded/dependencies/fontdb-0.24.0`; scope: **product**.

Keep the fork. It adds explicit fallible compatibility clones, immutable request-funded database snapshots and face registration with paid metadata/source owners. The adapter uses Database::try_extend_in/FaceRegistration and bootstrap storage layout; usvg rejects mutation of funded databases using try_clone_unfunded. The feature and APIs are absent upstream.

Current consumers: `crates/markdown-svg/src/font.rs:471`, `crates/markdown-svg/src/font.rs:484`, `crates/markdown-svg/src/job.rs:60`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/options.rs:423`.

Regression sources: `src/funded/tests.rs (metadata/keys/free-list parity, real free-before-refund, every allocation failure, wrong request and overlapping snapshots)`.

Relative to the exact release: 3 modified files, 2 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### gif 0.14.2

Path: `vendor/svg-bounded/dependencies/gif-0.14.2`; scope: **product**.

Keep the fork. It adds controlled first-frame decoding/metadata storage, request-paid palette/output/LZW buffers, work/cancel checkpoints and typed resource errors. resvg calls gif::controlled::ControlledDecoder; it propagates the resources feature to the modified weezl decoder. Upstream cannot satisfy that feature/API or memory/work contract.

Current consumers: `vendor/svg-bounded/fork/resvg-0.49.0/src/raster_decode.rs:62`.

Regression sources: `fork/resvg-0.49.0/src/raster/gif_tests.rs (controlled first-frame semantics, faulty/truncated GIF corpus, resource failure/cancellation retirement)`.

Relative to the exact release: 5 modified files, 3 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### image-webp 0.2.4

Path: `vendor/svg-bounded/dependencies/image-webp-0.2.4`; scope: **product**.

Keep the fork. It adds a controlled decoder and paid/checkpointed VP8, lossless Huffman/transform, arithmetic, alpha and YUV storage/work paths with resource-aware errors. resvg calls controlled::ControlledDecoder, absent upstream. Byteorder-lite and quick-error can independently return to registry without losing these changes.

Current consumers: `vendor/svg-bounded/fork/resvg-0.49.0/src/raster_decode.rs:89`.

Regression sources: `src/lossless_transform.rs controlled_regression_tests (single-column predictor work), fork/resvg-0.49.0/src/raster/webp_tests.rs (actual raster parity and fault/cancel paths)`.

Relative to the exact release: 10 modified files, 2 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Suppress dead_code only on retained ordinary constructors and decoded header fields in optimized builds; controlled paths and other lints remain enabled.

### simplecss 0.2.2

Path: `vendor/svg-bounded/dependencies/simplecss-0.2.2`; scope: **product**.

Keep the fork. It adds budget-backed stylesheet/selector arenas, cancellable tokenization and matching/backtracking, typed allocation/work/cancel failures, and controlled declaration/font extent parsing. usvg explicitly selects controlled::StyleSheet and controlled::DeclarationTokenizer for paid requests; the feature/module are absent upstream.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/svgtree/css.rs:11`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/svgtree/css.rs:16`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/svgtree/css.rs:69`.

Regression sources: `tests/controlled.rs (ordinary cascade parity, paid allocations/refunds, cancellable scans/backtracking, font extents and first errors)`.

Relative to the exact release: 5 modified files, 5 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### slotmap 1.1.1

Path: `vendor/svg-bounded/dependencies/slotmap-1.1.1`; scope: **product**.

Keep the fork. fontdb funds the actual slot array via initial_storage_layout, try_clone_with_extra_in and try_insert_with_key_noalloc, preserving IDs/holes/free links while rejecting a wrong request before allocating. Ordinary upstream try_insert_with_key/try_reserve does not provide this paid storage contract.

Current consumers: `vendor/svg-bounded/dependencies/fontdb-0.24.0/src/lib.rs:182`, `vendor/svg-bounded/dependencies/fontdb-0.24.0/src/funded.rs:338`, `vendor/svg-bounded/dependencies/fontdb-0.24.0/src/funded.rs:366`.

Regression sources: `dependencies/fontdb-0.24.0/src/funded/tests.rs (actual funded slot allocations, clone metadata/holes and reserve/commit rollback)`.

Relative to the exact release: 3 modified files, 0 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### svgtypes 0.16.1

Path: `vendor/svg-bounded/dependencies/svgtypes-0.16.1`; scope: **product**.

Keep the fork. It adds controlled numeric/length/path/point/font-family parsers with caller-typed checkpoint failures, paid path simplification/arc queues and sink-based font names. usvg calls ControlledSimplifyingPathParser, ControlledPointsParser and parse_*_list_with_control. These APIs are absent upstream; it also deliberately selects the controlled kurbo fork.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/shapes.rs:103`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/shapes.rs:296`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/units.rs:87`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/svgtree/mod.rs:1141`.

Regression sources: `src/numeric_control.rs, src/path.rs controlled_path_tests, src/points.rs controlled_tests, src/font_sink_tests.rs (float/grammar parity, cancellation, paid queues and typed first errors)`.

Relative to the exact release: 8 modified files, 2 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Suppress dead_code only on retained upstream CSS token helpers in optimized builds; other lints remain enabled.

### tiny-skia 0.12.0

Path: `vendor/svg-bounded/dependencies/tiny-skia-0.12.0`; scope: **product**.

Keep the fork. It adds CheckedPaint/gradient/pattern ownership and fallible request-controlled raster paths, paying/checkpointing real edge, sort, pipeline and hairline work/storage and preserving sticky errors across tiles. resvg uses CheckedPaint and *_checked raster APIs; these are absent upstream. It also routes to local controlled tiny-skia-path and png.

Current consumers: `vendor/svg-bounded/fork/resvg-0.49.0/src/path.rs:162`, `vendor/svg-bounded/fork/resvg-0.49.0/src/path.rs:216`, `vendor/svg-bounded/fork/resvg-0.49.0/src/path.rs:261`, `vendor/svg-bounded/fork/resvg-0.49.0/src/image.rs:290`.

Regression sources: `src/scan/raster_control_tests.rs, src/pipeline/request_paint_tests.rs, src/pipeline/gradient_index_tests.rs (pixel parity, paid scratch/pipeline failures, wrong request, refund tails and hairline cancellation)`.

Relative to the exact release: 24 modified files, 4 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Suppress dead_code only on retained ordinary raster adapters and callback type in optimized builds; controlled paths and other lints remain enabled.

### tiny-skia-path 0.12.0

Path: `vendor/svg-bounded/dependencies/tiny-skia-path-0.12.0`; scope: **product**.

Keep the fork. Path/path-builder/dash/stroker storage is request-funded, fallible, COW/owner aware and cancellable; paths validate same-request ownership. usvg/resvg and the adapter call try_with_capacity, try_from_rect, try_stroke and StrokeDash::try_new_frozen. These APIs and request-policy behavior are absent upstream.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/tree/mod.rs:727`, `vendor/svg-bounded/fork/usvg-0.49.0/src/tree/mod.rs:1713`, `vendor/svg-bounded/fork/resvg-0.49.0/src/render.rs:212`, `crates/markdown-svg/src/geometry.rs:174`.

Regression sources: `src/storage_tests.rs (exact paid layouts, COW/role copies, foreign request rejection, stroke/dash parity and reserve/commit/checkpoint/refund faults)`.

Relative to the exact release: 7 modified files, 2 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### weezl 0.1.12

Path: `vendor/svg-bounded/dependencies/weezl-0.1.12`; scope: **product**.

Keep the fork. It adds decode::ControlledDecoder and storage::{Control,Buffer,...}, so GIF LZW tables/work/output remain paid and fallible within the decoder. gif directly uses these public fork APIs. It also removes the upstream bin/lzw.rs development binary; this does not affect the needed library contract.

Current consumers: `vendor/svg-bounded/dependencies/gif-0.14.2/src/storage.rs:6`, `vendor/svg-bounded/dependencies/gif-0.14.2/src/reader/decoder.rs:363`, `vendor/svg-bounded/dependencies/gif-0.14.2/src/reader/converter.rs:169`.

Regression sources: `fork/resvg-0.49.0/src/raster/gif_tests.rs (integration through actual controlled LZW decode)`.

Relative to the exact release: 5 modified files, 1 added files, 1 omitted files. Exact paths and both hashes are in the ledger.

Use the equivalent Code::MAX constant instead of the deprecated max_value function.

### zune-core 0.5.3

Path: `vendor/svg-bounded/dependencies/zune-core-0.5.3`; scope: **product**.

Keep the fork. zune-jpeg uses the added bytestream::storage::{Control,Buffer,...}, controlled reader methods and sticky resource errors for all JPEG input/storage. Restoring upstream breaks both the resources feature and concrete dependent APIs.

Current consumers: `vendor/svg-bounded/dependencies/zune-jpeg-0.5.15/src/storage.rs:7`, `vendor/svg-bounded/dependencies/zune-jpeg-0.5.15/src/errors.rs:68`, `vendor/svg-bounded/dependencies/zune-jpeg-0.5.15/src/decoder.rs:224`.

Regression sources: `fork/resvg-0.49.0/src/raster/jpeg_tests.rs (integration through controlled byte stream)`.

Relative to the exact release: 7 modified files, 1 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Remove an invalid crate-level macro_use attribute; module macro imports remain unchanged.

### zune-jpeg 0.5.15

Path: `vendor/svg-bounded/dependencies/zune-jpeg-0.5.15`; scope: **product**.

Keep the fork. It threads resource control through JPEG header/entropy/Huffman, progressive state, MCU, upsample, color-convert and scalar/SIMD paths and exposes controlled::ControlledDecoder plus typed resource errors. resvg calls that decoder; this is extensive implementation work, not a path-only vendoring change.

Current consumers: `vendor/svg-bounded/fork/resvg-0.49.0/src/raster_decode.rs:142`.

Regression sources: `fork/resvg-0.49.0/src/raster/jpeg_tests.rs (actual baseline/progressive decode, raster parity, resource/retirement cases)`.

Relative to the exact release: 28 modified files, 3 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Remove an invalid crate-level macro_use attribute; module macro imports remain unchanged.

### harfrust 0.12.0

Path: `vendor/svg-bounded/fork/harfrust-0.12.0`; scope: **product**.

Adds request-bound Data/Instance/Buffer/Plan/Output, fallible FontFuncs and try_shape; threads explicit control through actual GSUB/GPOS/AAT, normalization, shaping and glyph buffers; preserves typed first failure and paid ownership. Adds read-only enclosing-mark and shaping-suppression probes against existing private Unicode tables.

Current consumers: `crates/markdown-svg/src/shape.rs`, `crates/markdown-svg/src/font.rs`, `crates/markdown-svg/src/grapheme.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/text/layout.rs`.

Regression sources: `src/hb/request_tests.rs`, `src/hb/buffer_control_tests.rs`, `src/hb/glyph_array_tests.rs`, `crates/markdown-svg/src/grapheme.rs`, `crates/markdown-svg/src/font_shape_tail_cases.rs`.

Relative to the exact release: 56 modified files, 8 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### hashbrown 0.17.1

Path: `vendor/svg-bounded/fork/hashbrown-0.17.1`; scope: **product**.

Adds controlled lookup/reserve/insert/iteration to HashMap and the existing RawTable algorithm, with fallible hash/equality/control callbacks, checkpoints during probe/control-byte fill/migration, and ownership-safe failure handling. Ordinary and controlled operations share kernels.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/outline_cache.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/font_cache.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/resource_cache.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/tree/validate.rs`.

Regression sources: `tests/notepads_markdown_controlled.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/outline_cache_tests.rs`.

Relative to the exact release: 5 modified files, 1 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### kurbo 0.13.1

Path: `vendor/svg-bounded/fork/kurbo-0.13.1`; scope: **product**.

Adds controlled PathSeg/CubicBez arclength and inverse arclength, shares original quadrature/subdivision/ITP kernels with NoControl ordinary APIs, and defines domain/overflow/stagnation handling for tiny epsilon and adjacent floating-point brackets.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/text/layout.rs`.

Regression sources: `src/common.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/text/geometry_layout_tests.rs`.

Relative to the exact release: 5 modified files, 0 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### read-fonts 0.41.0

Path: `vendor/svg-bounded/fork/read-fonts-0.41.0`; scope: **product**.

Adds sealed TryFontRead/TryFontReadWithArgs/TryComputeSize/TryVarSize/TryTableProvider and CheckedFontRef; threads caller WorkControl through borrowed font readers, arrays/offsets, generated table constructors/dynamic ranges, CFF/PostScript execution, glyf/gvar/VARC/bitmap/color metadata and variation normalization. Outer resource failures remain distinct from inner font errors.

Current consumers: `crates/markdown-svg/src/shape.rs`, `vendor/svg-bounded/fork/harfrust-0.12.0/src/hb/request.rs`, `vendor/svg-bounded/fork/skrifa-0.44.0/src/control.rs`.

Regression sources: `src/control_tests.rs`, `src/ps/cff/prefix_control_tests.rs`, `src/tables/representation_tests.rs`, `src/tables/representation_range_tests.rs`.

Relative to the exact release: 108 modified files, 7 added files, 1 omitted files. Exact paths and both hashes are in the ledger.

### resvg 0.49.0

Path: `vendor/svg-bounded/fork/resvg-0.49.0`; scope: **product**.

Adds render_in_request(_with_coverage) sharing usvg ProcessingContext, paid raster/scratch owners and decode paths, checks actual render/filter/clip/mask/image work, and exposes optional request-borrowed whole-fill coverage. Raster decoders preserve typed first failure and admitted output/storage lifetimes.

Current consumers: `crates/markdown-svg/src/job.rs`, `crates/markdown-svg/src/coverage.rs`.

Regression sources: `src/raster/tests.rs`, `src/raster/geometry_tests.rs`, `src/raster/png_tests.rs`, `src/raster/gif_tests.rs`, `src/raster/jpeg_tests.rs`, `src/raster/webp_tests.rs`, `src/path/paint_tests.rs`, `crates/markdown-svg/src/renderer_coverage_tests.rs`.

Relative to the exact release: 22 modified files, 30 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### roxmltree 0.21.1

Path: `vendor/svg-bounded/fork/roxmltree-0.21.1`; scope: **product**.

Adds optional notepads-markdown-resources support to DOM parsing/storage/tokenization, paid vectors and owned decoded/entity strings, ResourcesRef in ParsingOptions and typed Error::Resource; controls actual XML scans and ownership while retaining ordinary mode. Controlled namespace inheritance, prefix lookup and sorted insertion now prepay comparison/UTF-8 byte scans through shared request credits, poll bounded comparison windows, and move Copy namespace indices through the bounded paid slice helper. Namespace order and identities are preserved, and resource first faults propagate without an UnknownNamespace fallback. Regular markdown-svg tests cover long common prefixes, reverse insertion, namespace semantics, default-work rejection and cancellation/native faults. The short normalization-stream vendor regression measures added normalization work against an equal-DOM raw-text control so paid DOM vector migration is not mislabeled as scanning.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/mod.rs`.

Regression sources: `src/budget_tests.rs`, `tests/paid_strings.rs`.

Relative to the exact release: 6 modified files, 3 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Attribute duplicate detection meters comparison events and bounded URI/local-name equality, preserves length/pointer fast paths, polls before duplicate errors and completion, and retains first faults through cleanup.

### skrifa 0.44.0

Path: `vendor/svg-bounded/fork/skrifa-0.44.0`; scope: **product**.

Adds request-bound metrics/charmap/axes/location/outlines/bitmap/color/palette facades and shared controlled font kernels. Threads resource control through CFF/glyf/VARC, variation, COLR graph traversal/paint, bitmap selection and metadata; storage uses paid buffers and preserves request ownership.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/text/font_instance.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/text/flatten.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/text/colr.rs`.

Regression sources: `src/request_tests.rs`, `src/request_metadata_tests.rs`, `src/request_bitmap_tests.rs`, `src/request_color_tests.rs`, `src/color/control_tests.rs`, `src/outline/varc/control_tests.rs`, `src/representation_control_tests.rs`.

Relative to the exact release: 19 modified files, 22 added files, 1 omitted files. Exact paths and both hashes are in the ledger.

### unicode-bidi 0.3.18

Path: `vendor/svg-bounded/fork/unicode-bidi-0.3.18`; scope: **product**.

Adds optional controlled BidiInfo/paragraph/reorder APIs over shared explicit/implicit/prepare/UTF-16 algorithms, with paid results/scratch, explicit checkpoints, checked ranges and original error propagation. Raises local MSRV to project 1.99.

Current consumers: `crates/markdown-svg/src/draft.rs`, `crates/markdown-svg/src/shape.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/text/layout.rs`.

Regression sources: `src/controlled_tests.rs`, `src/storage_tests.rs`, `src/algorithm_tests.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/text/bidi_resource_tests.rs`.

Relative to the exact release: 7 modified files, 5 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Restrict alloc macro imports to tests that use the macros.

### usvg 0.49.0

Path: `vendor/svg-bounded/fork/usvg-0.49.0`; scope: **product**.

Adds controlled parse and ProcessingContext/ResourcesRef propagation, explicit source-kind/reference/CSS/numeric policy, paid immutable scene/tree/cache owners, font replay/coverage/geometry/representation lifetimes, checked text/font/COLR/bitmap/SVG-glyph integration, and fallible copying/writing. Existing parser/tree/text/writer kernels consume these capabilities. The controlled OpenType gzip SVG path now uses the same locked registry miniz_oxide public streaming kernel with an exact paid InflateState, borrowed bounded header scans, multi-member/CRC framing, and state retirement before output publication; it no longer constructs hidden flate2 decoder/header heap. Ordinary DEFLATE implementation and locked versions are unchanged.

Current consumers: `crates/markdown-svg/src/job.rs`, `crates/markdown-svg/src/adapter.rs`.

Regression sources: `src/parser/scene_parser_tests.rs`, `src/parser/reference_tests.rs`, `src/parser/svgtree/resource_tests.rs`, `src/tree/owner_tests.rs`, `src/tree/owner_revision_tests.rs`, `src/tree/owner_fixed_root_tests.rs`, `src/text/font_consumer_tests.rs`, `src/text/font_representation_tests.rs`, `src/text/representation_retention_tests.rs`, `src/text/geometry_layout_tests.rs`, `src/text/bidi_resource_tests.rs`, `src/visual_control_tests.rs`.

Relative to the exact release: 30 modified files, 59 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Warning cleanup removes unused local adapters, gates fixture-only helpers/imports with cfg(test), documents the processing context, preserves explicit record duplication, and discards Copy cache results without drop; the root workspace alone selects the resolver.

### xmlwriter 0.1.0

Path: `vendor/svg-bounded/fork/xmlwriter-0.1.0`; scope: **product**.

Refactors the writer into one policy-selected engine and adds RequestWriter with fallible paid output/depth storage, formatting/escaping/copy checkpoints, typed first failure, and retirement-aware finish. Ordinary writer delegates to the same engine.

Current consumers: `vendor/svg-bounded/fork/usvg-0.49.0/src/text/colr_storage.rs`.

Regression sources: `tests/request.rs`, `vendor/svg-bounded/fork/usvg-0.49.0/src/parser/font_replay_color_tests.rs`.

Relative to the exact release: 2 modified files, 4 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### fdeflate 0.3.7

Path: `vendor/svg-bounded/png/fdeflate-0.3.7`; scope: **product**.

Keep the fork. It adds Decompressor::new_controlled, paid Huffman/table storage, work/cancel checks and DecompressionError::Resources. PNG calls and matches these exact APIs; upstream lacks both the feature and the resource error variant.

Current consumers: `vendor/svg-bounded/png/png-0.18.1/src/controlled.rs:43`, `vendor/svg-bounded/png/png-0.18.1/src/controlled.rs:61`, `vendor/svg-bounded/png/png-0.18.1/src/decoder/zlib.rs:266`.

Regression sources: `fork/resvg-0.49.0/src/raster/png_tests.rs (integration through controlled fdeflate inflate)`.

Relative to the exact release: 3 modified files, 0 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

### png 0.18.1

Path: `vendor/svg-bounded/png/png-0.18.1`; scope: **product**.

Keep the fork. It adds controlled::ControlledDecoder/check_image_pixels, paid decoder/header/text/row storage and cancellable inflate/unfilter/Adam7/transform paths. Exact-cap ICC inflation consumes later EOB/trailer windows with one paid probe byte and rejects any output beyond the unchanged cap. resvg calls the controlled APIs and PNG calls the controlled fdeflate API. Registry flate2/miniz_oxide can independently satisfy their unchanged ordinary API use.

Current consumers: `vendor/svg-bounded/fork/resvg-0.49.0/src/png_decode.rs:26`, `vendor/svg-bounded/fork/resvg-0.49.0/src/png_decode.rs:30`.

Regression sources: `fork/resvg-0.49.0/src/raster/png_tests.rs (actual controlled decoding, metadata/inflate/interlace/palette and fault/cancel cases)`, `png/png-0.18.1/src/controlled.rs (exact inflated cap, later trailer, one-byte-over and paid retirement regression)`.

Relative to the exact release: 13 modified files, 2 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

Suppress dead_code only on the retained x86_64 Paeth helper when building other architectures.

### syntect 5.3.0

Path: `vendor/syntect`; scope: **product**.

Add borrowed per-block ExecutionControl/ExecutionMonitor with cooperative parser/include/cache/capture/scope checkpoints. Add parse_line_with_control, typed cancellation/deadline/work/resource/backend regex failures and terminal-control poisoning/reuse rejection. Use fallible once-cell compilation and propagate real regex compile/search failures without treating failure as no match.

Current consumers: `crates/markdown-core/src/highlight/engine.rs:107-111`, `crates/markdown-core/src/highlight/work.rs:72`.

Regression sources: `crates/markdown-core/src/highlight/tests.rs`, `vendor/syntect/src/parsing/execution.rs`, `vendor/syntect/src/parsing/parser.rs`, `vendor/syntect/src/parsing/regex.rs`.

Relative to the exact release: 4 modified files, 5 added files, 0 omitted files. Exact paths and both hashes are in the ledger.

## First-party libraries

`crates/markdown-memory` is the module-owned diagram allocation interface.
Its crate name and imports changed during relocation; its allocation/receipt/region
implementation was independently compared byte-for-byte after accounting for the import name.
`crates/markdown-svg-budget` is likewise a module-owned resource interface.
Neither is an upstream fork.

## Execution and maintenance limits

Controls are cooperative checkpoints and explicit accounting, not hard wall-time
preemption or a process RSS guarantee. In particular, opaque regex calls and OS
font/graphics operations have separate limits. Preserve first-failure priority,
source identity, output parity and owner retirement when updating a fork.

For an update, compare the selected official archive and its SHA, review the listed
local files, rebuild the owning layer and then validate the shared FFI/Windows/host
boundary affected by its output. Do not replace frozen expected output with current
renderer output. SVG's scalar-role adapter records the exact Unicode 17 data source
and tests the resulting classification independently of grapheme boundaries.

## Fixed test names

Fixture font name records and their SVG family inputs use Notepads.Markdown.
The namespace migration changed only name tables and required checksums; every
other font table, glyph, metric, embedded image and numerical expectation was
preserved. Current manifests bind the renamed bytes and retain original imported
hashes. Public schemas, attributions and sidecars use Notepads. Internal Rust packages use markdown-*; the product C ABI retains its notepads_markdown_* prefix.

## Validation entries

Use `scripts/Test-Rust.ps1` with Core/Interop/Svg/Math/Diagrams/All. All includes the
independent math workspace. `scripts/Test-FixedAssets.ps1` validates 918 frozen source
inputs, 11 code-syntax inputs, and current SVG, emoji and TOML notices. It
verifies deployed notice bytes when a layout is supplied. The module-root and
math lockfiles govern the product graph. The independent code-syntax pack
generator uses `scripts/code-syntax-pack/Cargo.lock`. Product and diagnostics
outputs remain separate.
