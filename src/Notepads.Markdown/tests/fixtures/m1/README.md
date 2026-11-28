# M1 fixture contract

[manifest.json](manifest.json) uses schema 1, data version `m1.1`. Input paths and oracle paths are relative to this directory. Every referenced file has an exact SHA256; raw input and expected bytes have Git text conversion disabled. A `null` oracle means unavailable/not checked, never passed.

Profiles are `C0` (CommonMark 0.31.2), `G3` (C0 plus tables/tasks/strikethrough), `G4` (G3 plus extended autolinks), and `X` (explicitly unselected or historical comparison). `required`, `comparison_only`, and `out_of_scope` retain both the original input/expected and a predeclared reason. Candidate feature limitations remain visible rather than changing the profile after a failed run.

Original upstream attribution, commits, file hashes, selectors and adaptation steps are recorded per fixture. Derived CommonMark/GFM data remain CC-BY-SA-4.0; the project MIT license does not replace upstream licenses. Independently authored semantic/mapping goldens are separate from original upstream HTML.

`oracles.semantic` and `oracles.mapping` may refer to an aggregate JSON file and a `cases[id=...]` selector. The original project goldens contain declarative semantic `checks`, optional readable text, exact fragments and boundary queries. Supplemental upstream adaptations contain independently converted source envelopes; envelopes are not exact mapping. Invalid UTF-8 fixtures point to an admission-error oracle and must be rejected before parsing.

`edit_sequences` contain independent old-coordinate edit/resync steps and expected accepted revision/bytes/status. A rejected step leaves the accepted source unchanged. These are source tests; ordered edit acceptance does not claim incremental parsing.

`workloads` contain exact inputs, recipes, provenance and minimum sample counts. The ordinary workload is approximately 1 MiB (1,092,000 bytes); reports must use actual byte lengths. Giant/entity/code/table synthetic cases are project pressure inputs, not upstream benchmark claims. The Toolkit specimen retains its original bytes and license; its historical rendered output is not an adopted native visual oracle.

The offline verification entry is `scripts/Import-M1Baselines.ps1 -VerifyOnly`. Research runner evidence must name the exact manifest hash and preserve failures, unsupported features and unimplemented oracle layers. Runtime/UI and product performance acceptance belong to later real package tests.
