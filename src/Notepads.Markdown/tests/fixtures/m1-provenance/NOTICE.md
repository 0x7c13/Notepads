# M1-E04 independent provenance fixtures

The 26-case basic research oracle is independently approved and frozen as m1-provenance.1 (approved_basic_oracle). Oracle approval does not report any candidate parser, normalizer or mapper result. The three pending required scopes remain production-gate blockers.

Original Notepads examples, the data format, and the counting checker are provided under the root MIT license: Copyright (c) 2026, Jiaqi (0x7c13) Liu.

`e04.container.partial-tab-code` adapts the tab example in CommonMark 0.31.2, by John MacFarlane. It is licensed under CC-BY-SA-4.0. The specification represents tabs with U+2192; this derived input decodes those placeholders to U+0009 and retains the LF. Its manually authored native projection, mapping and query expectations are also identified as CC-BY-SA-4.0 derived data. The original specification and original notices are retained unchanged in `tests/baselines/commonmark-0.31.2/`.

Pinned source: [CommonMark 0.31.2 spec.txt](https://github.com/commonmark/commonmark-spec/blob/9103e341a973013013bb1a80e13567007c5cef6f/spec.txt), section `Tabs`, example containing `>` followed by two tab placeholders and `foo`.

The fixed normative rule references also include [GFM 0.29-gfm spec.txt](https://github.com/github/cmark-gfm/blob/587a12bb54d95ac37241377e6ddc93ea0e45439b/test/spec.txt), by John MacFarlane and GitHub contributors. The GFM original specification and complete COPYING remain unchanged in `tests/baselines/gfm-0.29-gfm/`.

The complete CC-BY-SA-4.0 license is retained in `tests/baselines/licenses/CC-BY-SA-4.0.txt`. Original upstream notices are authoritative; the project MIT license does not relicense adapted upstream material. The manifest records the collection as `MIT AND CC-BY-SA-4.0` and records each case's own provenance/license.

No candidate parser or implementation under test generated the expected data. The counting checker only validates authored spans, scalar boundaries, UTF-8/UTF-16 counts, file identity and payload/fragment string identity; it does not interpret Markdown or calculate BoundaryRelations.
