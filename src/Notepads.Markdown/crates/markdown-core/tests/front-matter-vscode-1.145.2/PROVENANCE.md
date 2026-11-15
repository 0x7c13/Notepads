# Fixed front matter compatibility inputs

The 61 authored inputs and expected token/rendered-value contract were captured by executing the unmodified VS Code 1.145.2 front matter rule and renderer with its locked `yaml` 2.9.0 and `markdown-it` 14.3.2 dependencies. The development harness removes TypeScript annotations and supplies configuration/localization, without changing the parsing or rendering functions. Upstream package tarball integrity was verified before execution.

- VS Code commit: `c4453f8ea430795f941c1b2ff6c9321135a74a02`.
- Source: `extensions/markdown-language-features/src/extensions/yamlPreamble/yamlPreamble.ts`, Microsoft, MIT.
- `oracle-contract.json` SHA256: `375141132B25A6D7836DDDD5AEEADD5ADECE5B8BAD874EE98B8DC86639FCB250`.
- Runtime configuration: table (default), codeBlock, hide; recognized raw content comes from the actual token metadata. JSON strings are strings, including ISO timestamps.

The original contract is immutable. Additional regression inputs belong in separately named tests/replay records. The native renderer deliberately rejects cyclic/over-budget structures visibly instead of reproducing the upstream renderer's recursive-array overflow. Front matter nested in a first-line quote/list is left as ordinary Markdown, instead of reproducing markdown-it's block-rule reentry accident. The source mapping is explicitly atomic for the whole preamble; body mapping remains unchanged. YAML syntax diagnostic wording comes from the native parser and is not a promise of byte-identical upstream messages.

Notepads-authored tests: Copyright (c) 2026, Jiaqi (0x7c13) Liu. See the module LICENSE. Upstream notices are shipped under `third-party-notices/MarkdownCore/{vscode-yaml-preamble-1.145.2,yaml-2.9.0}`.
