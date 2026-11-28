# Third-Party Notices

This file records source translated, copied, generated, or embedded in `crates/merman-core`.
It is generated from `docs/release/THIRD_PARTY_COMPONENTS.json`.

## DOMPurify

Merman selects DOMPurify's Apache-2.0 option for generated sanitizer defaults; the exact upstream Apache-2.0 license file is preserved.

- Version: `3.4.16`
- Source: https://github.com/cure53/DOMPurify.git @ `b9b9d80f7e401771c2ccaef5f45def7eec8f27d7`
- Relationship: `generated`, `translated`
- License: `(Apache-2.0 OR MPL-2.0)`
- Legal file: `THIRD_PARTY_LICENSES/dompurify/LICENSE`

## Mermaid

Merman independently implements Mermaid-compatible behavior while translating selected algorithms, generating defaults, copying architecture icon data, and retaining upstream fixtures and snapshots.

- Version: `12.1.0`
- Source: https://github.com/mermaid-js/mermaid.git @ `21f72f07ea22c0af48a3149c550654e80d8e40cb`
- Relationship: `behavior-reference`, `copied`, `fixtures`, `generated`, `modified`, `translated`
- License: `MIT`
- Legal file: `THIRD_PARTY_LICENSES/mermaid/LICENSE`

## sanitize-url

Merman's URL sanitization behavior is a source-backed Rust translation of sanitize-url.

- Version: `7.1.1`
- Source: https://github.com/braintree/sanitize-url.git @ `b1e8d50e4066a9af00fa042176676374747f754b`
- Relationship: `modified`, `translated`
- License: `MIT`
- Legal file: `THIRD_PARTY_LICENSES/sanitize-url/LICENSE`

## ZenUML Core

Merman's ZenUML grammar, model, renderer, emoji/icon data, and behavior probes follow the admitted ZenUML Core 3.50.1 source baseline.

- Version: `3.50.1`
- Source: https://github.com/mermaid-js/zenuml-core.git @ `38404ccc14243ed54ab45b804b2eb6f2ca73af36`
- Relationship: `behavior-reference`, `copied`, `modified`, `translated`
- License: `MIT`
- Legal file: `THIRD_PARTY_LICENSES/zenuml-core/LICENSE`
