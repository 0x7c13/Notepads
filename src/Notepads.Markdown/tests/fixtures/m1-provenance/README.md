# M1-E04 basic research oracle

Data version `m1-provenance.1` is frozen as `approved_basic_oracle` after independent Astra review of the 26 cases and all 87 BoundaryRelations. This is approval of expected data; no candidate parser, normalizer or mapper result is asserted.

`manifest.json` fixes the source inputs, expected JSON, notices, arithmetic ledger and verification files by SHA256. All 26 cases are `required_oracle`. Their layer remains `normalization` (3), `mapping_control` (1), or `parser_projection` (22); artificial control traces do not count as Markdown parser compatibility assertions.

The reviewed contract is `artifacts/evidence/m1/e04-plan/approved-research-contract-v3.md`, SHA256 `F7D453D7B71C700643D40A9DF7ACBA3EC25D57E3383F2CF53E4D038B317F5933`. It retains its historical submission-status header; actual approval, including separator anchors and optional Object source-edit actions, is recorded in `docs/design/M1-RESEARCH-REVIEW.md`.

The three pending scopes remain required, with `required_scope=true` and `blocks_production_gate=true`:

- G4 entity/escape priority and exact destination origins.
- Long failed URL bounded lookahead and cancellation/cost controls.
- Fenced code trailing/EOF provenance and terminal LF policy.

A future runner must preserve these obligations and report them as unresolved. Skipping pending records cannot release production G02. Basic oracle approval also does not close trace rejection controls, range queries, cancellation, cost, or product integration/UI acceptance.

The expected values were manually derived before implementation from pinned normative rules and the explicit research policy. `tools/Check-IndependentData.ps1` only checks already authored numbers, scalar boundaries, partitions, file identity and string identity; it cannot recognize Markdown, normalize input, calculate BoundaryRelations, or freeze an oracle. To reproduce the counting ledger:

```powershell
./tests/fixtures/m1-provenance/tools/Check-IndependentData.ps1
```

The original 34-file draft checkpoint is retained at `artifacts/evidence/m1/e04-b01/draft-1/checkpoint.json`, SHA256 `7EEA6D73509AA1F3BA814D2F67581B81F33DE17DF3EAC021D57483961C3F0D84`. `freeze-metadata-check.json` verifies that this freeze changes metadata only: complete case contents excluding status/required metadata, all 26 input bytes, and counting-ledger values remain unchanged. No parser output was used.

The existing `m1.1` fixtures and evidence remain immutable; old task/table null values remain null. See `NOTICE.md` for MIT original material and the CC-BY-SA-4.0 CommonMark example adaptation. Original fixture input bytes are excluded from Git EOL conversion.
