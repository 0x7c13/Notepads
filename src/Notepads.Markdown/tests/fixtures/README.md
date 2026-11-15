# Fixed test inputs

Copyright (c) 2026, Jiaqi (0x7c13) Liu. MIT; see [LICENSE](../../LICENSE).
Original third-party copyrights, licenses and notices remain applicable.

The module keeps its test inputs locally. [MIGRATION.json](MIGRATION.json) binds
current retained paths, sizes and SHA-256 in `files`; `original_import` preserves
earlier identities for maintained metadata and renamed font inputs. Its separate
`archived_files` list retains complete historical records of removed research
inputs and deduplicated payloads, whose original bytes remain in the verified
external source snapshot. Original case IDs, golden results and upstream commits
remain unchanged. Historical provenance paths identify original evidence; they
are not runtime dependencies on that old repository or its artifacts.

- `mermaid-flowchart-v1` contains the original four Merman flowchart inputs from
  `research/r4-mermaid-spike/fixtures/flowchart`, checked against its frozen input
  lock. `UPSTREAM-INPUTS.lock.json` preserves that complete historical lock.
  The applicable Merman MIT/Apache-2.0, Mermaid MIT and third-party notices are in
  its `licenses` directory; the original upstream revision is preserved in the lock.
- `math-upstream-oracle-v1` contains the three prefrozen display-list JSON files
  emitted by the unmodified registry RaTeX 0.1.15 oracle. They are copied bytes,
  not regenerated output. Its `licenses` directory preserves the RaTeX and
  KaTeX MIT notices. The original paths and hashes are in the migration manifest.
- `svg-font-representations-v1` preserves the original authored font/PNG/SVG
  inputs under the module MIT license. `INPUTS.json` binds current bytes and retains original imported identities.
  The previously external `original-2x2-rgba.png` was recovered by copying its
  compressed PNG byte sequence directly from both original SBIX and CBDT fonts.
  The original font files matched their imported SHA-256, and both embedded PNG
  copies remain byte for byte equal after the namespace migration. No pixels were encoded and no fixture generator
  or renderer ran. `MIGRATION.json` labels this `EmbeddedByteRecovery`, records
  the two table offsets and the extracted 79-byte payload SHA-256, and explicitly
  states that an independent original PNG SHA-256 receipt is unavailable.
- Existing `m1*`, `svg-fonts` and adjacent `tests/baselines` retain their original
  manifests and licenses, including the shared Fontations payload mapping and Unicode
  17.0.0 conformance data. The derived TTC filename and internal family names now use Notepads naming.

The migration manifest and this index are authored MIT metadata. They describe
file preservation and input recovery; they grant no new renderer, UI or product
qualification. Diagram platform failure diagnostics go to a unique run directory
under `LOCALAPPDATA/Notepads.Build/Markdown/tests` by default. Set
`NOTEPADS_MARKDOWN_TEST_OUTPUT` to choose an output root and export SVGs from the
passing cases too. Each run uses only its current inputs and artifacts.

Research-only provenance V2/V3, experiment descriptions and acquisition generators
were archived outside the source tree. The 2026-11-15 namespace migration changed
only fixture font names, required checksums and SVG family attributes. Other font
tables and numerical expectations remain unchanged. MIGRATION.json binds current
inputs and retains their original imported identities; embedded PNG recovery
offsets describe the archived original fonts. See the module README for test entries.
