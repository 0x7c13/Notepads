# Third-Party Notices

<!-- This file is generated. Run `python3 scripts/verify-third-party-licenses.py --write`. -->

Merman is an independent, headless Rust implementation of Mermaid-compatible behavior. It
is not affiliated with or endorsed by Mermaid or the projects listed below.

The machine-readable source of truth is [`docs/release/THIRD_PARTY_COMPONENTS.json`](docs/release/THIRD_PARTY_COMPONENTS.json).
It records exact source revisions, local relationships, artifact scopes, and SHA-256-bound
license or notice files. This inventory is engineering evidence, not legal advice.

## Artifact Scopes

### `ascii-render`

ASCII rendering source, copied fixtures, and behavior references.

Components: `beautiful-mermaid`, `mermaid`, `mermaid-ascii`, `mermaid-rs-renderer`.

### `cli-default`

The default CLI feature closure including ELK and RaTeX math/font support.

Components: `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `eclipse-elk`, `elkjs`, `fmin`, `graphlib`, `katex-fonts`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `non-layered-tidy-tree-layout`, `ratex`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `zenuml-core`.

### `cli-release`

The published complete CLI release closure, including explicit ELK and RaTeX math/font support.

Components: `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `eclipse-elk`, `elkjs`, `fmin`, `graphlib`, `katex-fonts`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `non-layered-tidy-tree-layout`, `ratex`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `zenuml-core`.

### `elk-render`

Render artifacts that include the source-translated Eclipse ELK layered implementation.

Components: `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `eclipse-elk`, `elkjs`, `fmin`, `graphlib`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `non-layered-tidy-tree-layout`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `zenuml-core`.

### `playground-reference`

Third-party projects loaded by or used as behavioral evidence for the deployed Playground.

Components: `d3-shape`, `dompurify`, `elkjs`, `mermaid`, `non-layered-tidy-tree-layout`, `roughjs`, `sanitize-url`, `venn-js`, `zenuml-core`.

### `ratex-render`

Render artifacts that link RaTeX and embed the KaTeX TrueType font payload.

Components: `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `fmin`, `graphlib`, `katex-fonts`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `non-layered-tidy-tree-layout`, `ratex`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `zenuml-core`.

### `rust-render-base`

Headless parser and renderer artifacts without optional ELK, RaTeX, ASCII, or Typst transport features.

Components: `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `fmin`, `graphlib`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `non-layered-tidy-tree-layout`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `zenuml-core`.

### `source-archive`

Conservative repository source archive inventory covering every translated, copied, linked, embedded, fixture, and behavior-reference component recorded here.

Components: `beautiful-mermaid`, `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `eclipse-elk`, `elkjs`, `fmin`, `graphlib`, `katex-fonts`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `mermaid-ascii`, `mermaid-rs-renderer`, `monaqa-tree-sitter-mermaid`, `non-layered-tidy-tree-layout`, `pappasam-tree-sitter-mermaid`, `ratex`, `rough-rs`, `roughjs`, `sanitize-url`, `singularity-tree-sitter-mermaid`, `tree-sitter-generator`, `tree-sitter-mermaid-mermaid-12-new-families`, `tree-sitter-mermaid-mermaid-baseline`, `tree-sitter-mermaid-zenuml-baseline`, `tree-sitter-templates`, `unicode-linebreak`, `venn-js`, `wasm-minimal-protocol`, `zenuml-core`.

### `tree-sitter-mermaid-source`

The independently versioned Tree-sitter Mermaid language source package and its pinned syntax and compatibility references.

Components: `monaqa-tree-sitter-mermaid`, `pappasam-tree-sitter-mermaid`, `singularity-tree-sitter-mermaid`, `tree-sitter-generator`, `tree-sitter-mermaid-mermaid-12-new-families`, `tree-sitter-mermaid-mermaid-baseline`, `tree-sitter-mermaid-zenuml-baseline`, `tree-sitter-templates`.

### `typst-publish`

The Typst WASM publish profile, including ELK and wasm-minimal-protocol but excluding RaTeX.

Components: `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `eclipse-elk`, `elkjs`, `fmin`, `graphlib`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `non-layered-tidy-tree-layout`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `wasm-minimal-protocol`, `zenuml-core`.

### `web-analysis`

Browser analysis artifacts containing the Merman core parser, sanitizer, diagram detection, and diagnostics without renderer or ASCII components.

Components: `dompurify`, `mermaid`, `sanitize-url`, `zenuml-core`.

### `web-ascii`

Browser ASCII artifacts combining the core parser legal closure with the ASCII renderer's copied fixtures and behavior references.

Components: `beautiful-mermaid`, `dompurify`, `mermaid`, `mermaid-ascii`, `mermaid-rs-renderer`, `sanitize-url`, `zenuml-core`.

### `web-editor`

Browser editor artifacts built on the analysis and core-parser legal closure without SVG or ASCII rendering components.

Components: `dompurify`, `mermaid`, `sanitize-url`, `zenuml-core`.

### `web-full`

The published complete browser SVG renderer, including Cytoscape, ELK, RaTeX, and the embedded KaTeX font payload.

Components: `beautiful-mermaid`, `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `eclipse-elk`, `elkjs`, `fmin`, `graphlib`, `katex-fonts`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `mermaid-ascii`, `mermaid-rs-renderer`, `non-layered-tidy-tree-layout`, `ratex`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `zenuml-core`.

### `web-render`

Published complete browser SVG renderer with Cytoscape, ELK, RaTeX, and the embedded KaTeX font payload, but without analysis, ASCII, or editor APIs.

Components: `cose-base-v1`, `cose-base-v2`, `cytoscape`, `cytoscape-cose-bilkent`, `cytoscape-fcose`, `d3-shape`, `dagre`, `dompurify`, `eclipse-elk`, `elkjs`, `fmin`, `graphlib`, `katex-fonts`, `layout-base-v1`, `layout-base-v2`, `mermaid`, `non-layered-tidy-tree-layout`, `ratex`, `rough-rs`, `roughjs`, `sanitize-url`, `unicode-linebreak`, `venn-js`, `zenuml-core`.

## Components

### beautiful-mermaid (`beautiful-mermaid`)

The ASCII renderer uses this implementation as a behavior and presentation reference; its parser is not embedded.

- Version: `1.1.3`
- Source: <https://github.com/lukilabs/beautiful-mermaid>
- Source ref: `main`
- Source commit: `2ac8bbbb060ca0a65a6a21f3200bd99b1587b488`
- Source path: `.`
- Relationship: `behavior-reference`
- License expression: `MIT`
- Artifact scopes: `ascii-render`, `source-archive`, `web-ascii`, `web-full`
- Local evidence: `crates/merman-ascii`
- Legal files:
  - [`THIRD_PARTY_LICENSES/beautiful-mermaid/LICENSE`](THIRD_PARTY_LICENSES/beautiful-mermaid/LICENSE) (license, SHA-256 `f05f5a4009eae7fadfd4a55f77ffe10948f621982d56076667e68afbb54de894`)

### cose-base 1.x (`cose-base-v1`)

Manatee contains Rust translations and adaptations of CoSE layout behavior from this baseline.

- Version: `1.0.3`
- Source: <https://github.com/iVis-at-Bilkent/cose-base.git>
- Source ref: `v1.0.3`
- Source commit: `914bfe712991534af1d8b795d6f262687edc2563`
- Source path: `.`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/manatee`
- Legal files:
  - [`THIRD_PARTY_LICENSES/cose-base-v1/LICENSE`](THIRD_PARTY_LICENSES/cose-base-v1/LICENSE) (license, SHA-256 `5fb3cf4a14c3c5af6e473a192df8bca10c77754e3a0c6492c79fb92a76a5478a`)

### cose-base 2.x (`cose-base-v2`)

Manatee contains Rust translations and adaptations of the newer CoSE base behavior from this baseline.

- Version: `2.2.0`
- Source: <https://github.com/iVis-at-Bilkent/cose-base.git>
- Source ref: `v2.2.0`
- Source commit: `37f07ed2b8803211ec6c74110574cc47c156a136`
- Source path: `.`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/manatee`
- Legal files:
  - [`THIRD_PARTY_LICENSES/cose-base-v2/LICENSE`](THIRD_PARTY_LICENSES/cose-base-v2/LICENSE) (license, SHA-256 `5fb3cf4a14c3c5af6e473a192df8bca10c77754e3a0c6492c79fb92a76a5478a`)

### Cytoscape.js (`cytoscape`)

Architecture layout and styling use source-backed Cytoscape behavior and defaults.

- Version: `3.34.0`
- Source: <https://github.com/cytoscape/cytoscape.js.git>
- Source ref: `v3.34.0`
- Source commit: `22716bfb75834b56fa6679648b0abb06f4ae691c`
- Source path: `.`
- Relationship: `behavior-reference`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/manatee`, `crates/merman-render/src/architecture.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/cytoscape/LICENSE`](THIRD_PARTY_LICENSES/cytoscape/LICENSE) (license, SHA-256 `eb319c6e6f233607f71e8e2f450391751883cfc0eeb3ca7ef574c13d1d9c2203`)

### cytoscape.js-cose-bilkent (`cytoscape-cose-bilkent`)

Manatee includes source-backed CoSE-Bilkent layout behavior translated to Rust.

- Version: `4.1.0`
- Source: <https://github.com/iVis-at-Bilkent/cytoscape.js-cose-bilkent.git>
- Source ref: `v4.1.0`
- Source commit: `999090a8438b4f14788d636ef4fd7a5355e29e8c`
- Source path: `.`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/manatee`
- Legal files:
  - [`THIRD_PARTY_LICENSES/cytoscape-cose-bilkent/LICENSE`](THIRD_PARTY_LICENSES/cytoscape-cose-bilkent/LICENSE) (license, SHA-256 `440fc58a56a12814e417d2b341da89b050da052dc75bdb235607d37ec5fe74ef`)

### cytoscape.js-fcose (`cytoscape-fcose`)

The headless Architecture layout is a modified Rust implementation of FCoSE behavior.

- Version: `2.2.0`
- Source: <https://github.com/iVis-at-Bilkent/cytoscape.js-fcose.git>
- Source ref: `v2.2.0`
- Source commit: `78afcf96512a409abc903699277ad616c02dfad9`
- Source path: `.`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/manatee`, `crates/merman-render/src/architecture.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/cytoscape-fcose/LICENSE`](THIRD_PARTY_LICENSES/cytoscape-fcose/LICENSE) (license, SHA-256 `2837634f403949215760fcdd2fa1ed0c64875d02099ecc8318c704b852f1421d`)

### d3-shape (`d3-shape`)

The SVG parity layer translates D3 curve algorithms, including basis, natural, step, cardinal, bump, and Catmull-Rom variants.

- Version: `3.2.0`
- Source: <https://github.com/d3/d3-shape.git>
- Source ref: `npm:d3-shape@3.2.0`
- Source commit: `8ec82658454750cfa29efb1e0ea514e3dd9b2297`
- Source path: `src/curve`
- Relationship: `modified`, `translated`
- License expression: `ISC`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/merman-render/src/svg/parity/curve.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/d3-shape/LICENSE`](THIRD_PARTY_LICENSES/d3-shape/LICENSE) (license, SHA-256 `faa682e3e430941f958d26180458f5934a62f58dac4d70ccdd15608c15d0f884`)

### Dagre (`dagre`)

Dugong is a modified Rust translation of Dagre's directed graph layout pipeline.

- Version: `2.0.2`
- Source: <https://github.com/dagrejs/dagre.git>
- Source ref: `v2.0.2`
- Source commit: `ba986662394f8f3ed608717194e5958f3386ce01`
- Source path: `lib`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/dugong`
- Legal files:
  - [`THIRD_PARTY_LICENSES/dagre/LICENSE`](THIRD_PARTY_LICENSES/dagre/LICENSE) (license, SHA-256 `6a349742a6cb219d5a2fc8d0844f6d89a6efc62e20c664450d884fc7ff2d6015`)

### DOMPurify (`dompurify`)

Merman selects DOMPurify's Apache-2.0 option for generated sanitizer defaults; the exact upstream Apache-2.0 license file is preserved.

- Version: `3.4.16`
- Source: <https://github.com/cure53/DOMPurify.git>
- Source ref: `3.4.16`
- Source commit: `b9b9d80f7e401771c2ccaef5f45def7eec8f27d7`
- Source path: `.`
- Relationship: `generated`, `translated`
- License expression: `(Apache-2.0 OR MPL-2.0)`
- Selected license path: `Apache-2.0`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-analysis`, `web-ascii`, `web-editor`, `web-full`, `web-render`
- Local evidence: `crates/merman-core/src/generated/dompurify_defaults.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/dompurify/LICENSE`](THIRD_PARTY_LICENSES/dompurify/LICENSE) (license, SHA-256 `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`)

### Eclipse Layout Kernel (`eclipse-elk`)

The merman-elk-layered crate contains modified Rust source translations of Eclipse ELK layered, Box, Force, Stress, Mr. Tree, Radial, Rectangle Packing, and SPOrE overlap-removal algorithms, including common and core layout utilities, under EPL-2.0.

- Version: `0.9.1`
- Source: <https://github.com/eclipse-elk/elk.git>
- Source ref: `v0.9.1`
- Source commit: `62d5909f96fad541bc101ad52dabaece6b7eab7e`
- Source path: `plugins`
- Relationship: `modified`, `translated`
- License expression: `EPL-2.0`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/merman-elk-layered`, `crates/merman-layout-elk`
- Legal files:
  - [`THIRD_PARTY_LICENSES/eclipse-elk/LICENSE.md`](THIRD_PARTY_LICENSES/eclipse-elk/LICENSE.md) (license, SHA-256 `89591d4578fb1ebd91501312a3d25f021bd865a2e436641c1cf7b1bc7e3c1617`)

### elkjs (`elkjs`)

Mermaid's ELK adapter behavior is compared against this JavaScript distribution, which is generated from Eclipse ELK sources.

- Version: `0.9.3`
- Source: <https://github.com/kieler/elkjs.git>
- Source ref: `npm:elkjs@0.9.3`
- Source commit: `a8304cf79fde75bc2ab1a89d28320f53f8637436`
- Source path: `.`
- Relationship: `behavior-reference`
- License expression: `EPL-2.0`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/merman-layout-elk`, `playground`
- Legal files:
  - [`THIRD_PARTY_LICENSES/elkjs/LICENSE.md`](THIRD_PARTY_LICENSES/elkjs/LICENSE.md) (license, SHA-256 `89591d4578fb1ebd91501312a3d25f021bd865a2e436641c1cf7b1bc7e3c1617`)

### fmin (`fmin`)

The Venn layout kernel translates the fmin Nelder-Mead and conjugate-gradient optimization behavior.

- Version: `0.0.4`
- Source: <https://github.com/benfred/fmin.git>
- Source ref: `npm:fmin@0.0.4`
- Source commit: `6b155c9f4a6ecf73ea5d71666da8e5dcd418b18b`
- Source path: `.`
- Relationship: `modified`, `translated`
- License expression: `BSD-3-Clause`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/merman-render/src/venn.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/fmin/LICENSE`](THIRD_PARTY_LICENSES/fmin/LICENSE) (license, SHA-256 `e4503e78185bff178d3ee91835f082d05771da1b3a2d795f17e03a40251bab77`)

### Graphlib (`graphlib`)

dugong-graphlib is a modified Rust translation of the graph model used by Dagre.

- Version: `2.2.4`
- Source: <https://github.com/dagrejs/graphlib.git>
- Source ref: `v2.2.4`
- Source commit: `380d5efa1f4ab0904539f046bdba583d14ac2add`
- Source path: `lib`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/dugong-graphlib`
- Legal files:
  - [`THIRD_PARTY_LICENSES/graphlib/LICENSE`](THIRD_PARTY_LICENSES/graphlib/LICENSE) (license, SHA-256 `6a349742a6cb219d5a2fc8d0844f6d89a6efc62e20c664450d884fc7ff2d6015`)

### KaTeX fonts bundled by ratex-katex-fonts (`katex-fonts`)

The RaTeX SVG backend embeds twenty KaTeX TrueType fonts; those font bytes are licensed under OFL-1.1 rather than RaTeX's MIT code license.

- Version: `ratex-katex-fonts@0.1.14`
- Source: <https://github.com/erweixin/RaTeX.git>
- Source ref: `crates.io:ratex-katex-fonts@0.1.14`
- Source commit: `ae391d727ac615437c63c308f4538d971a84bede`
- Source path: `crates/ratex-katex-fonts/fonts`
- Relationship: `embedded`
- License expression: `OFL-1.1`
- Artifact scopes: `cli-default`, `cli-release`, `ratex-render`, `source-archive`, `web-full`, `web-render`
- Local evidence: `Cargo.lock`, `crates/merman-render/src/math.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/katex-fonts/FONT_NOTICE.txt`](THIRD_PARTY_LICENSES/katex-fonts/FONT_NOTICE.txt) (notice, SHA-256 `752ba9eff7a281f5ad789528ea078b46149b10e72296625108c26a94695ad03e`)
  - [`THIRD_PARTY_LICENSES/katex-fonts/OFL.txt`](THIRD_PARTY_LICENSES/katex-fonts/OFL.txt) (license, SHA-256 `f19c674290e5dc79f02e8efe16139ab56a06a7128876f5b1579ffd0db5bc897e`)

### layout-base 1.x (`layout-base-v1`)

Manatee translates shared layout-base geometry, graph, and force-layout primitives from this baseline.

- Version: `1.0.2`
- Source: <https://github.com/iVis-at-Bilkent/layout-base.git>
- Source ref: `v1.0.2`
- Source commit: `836898aa4a88e2794774997d7128b383108a3d5a`
- Source path: `.`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/manatee`
- Legal files:
  - [`THIRD_PARTY_LICENSES/layout-base-v1/LICENSE`](THIRD_PARTY_LICENSES/layout-base-v1/LICENSE) (license, SHA-256 `eabb762d8a95109a39c9be3247325529a5239a7aca327d909c3ccdc41f3a06bf`)

### layout-base 2.x (`layout-base-v2`)

Manatee also follows the newer layout-base behavior selected by the FCoSE dependency graph.

- Version: `2.0.1`
- Source: <https://github.com/iVis-at-Bilkent/layout-base.git>
- Source ref: `v2.0.1`
- Source commit: `3f7549940feef31416cc35ef8256282ebc4d1ecd`
- Source path: `.`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/manatee`
- Legal files:
  - [`THIRD_PARTY_LICENSES/layout-base-v2/LICENSE`](THIRD_PARTY_LICENSES/layout-base-v2/LICENSE) (license, SHA-256 `eabb762d8a95109a39c9be3247325529a5239a7aca327d909c3ccdc41f3a06bf`)

### Mermaid (`mermaid`)

Merman independently implements Mermaid-compatible behavior while translating selected algorithms, generating defaults, copying architecture icon data, and retaining upstream fixtures and snapshots.

- Version: `12.1.0`
- Source: <https://github.com/mermaid-js/mermaid.git>
- Source ref: `mermaid@12.1.0`
- Source commit: `21f72f07ea22c0af48a3149c550654e80d8e40cb`
- Source path: `packages/mermaid`
- Relationship: `behavior-reference`, `copied`, `fixtures`, `generated`, `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `ascii-render`, `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-analysis`, `web-ascii`, `web-editor`, `web-full`, `web-render`
- Local evidence: `crates/merman-core/src`, `crates/merman-render/src`, `crates/merman-render/src/svg/parity/architecture/icons.rs`, `fixtures`
- Legal files:
  - [`THIRD_PARTY_LICENSES/mermaid/LICENSE`](THIRD_PARTY_LICENSES/mermaid/LICENSE) (license, SHA-256 `ec9fb67dcb25eccc416ed56e1aab819222c805a2a4bfe4cb19e7556bf2ffde80`)

### mermaid-ascii (`mermaid-ascii`)

The ASCII renderer translates algorithms and retains a documented set of copied upstream fixtures from this source.

- Version: `git-6fffb8e`
- Source: <https://github.com/AlexanderGrooff/mermaid-ascii>
- Source ref: `master`
- Source commit: `6fffb8e2714acab2c4cb41c78894fabbc62cee56`
- Source path: `.`
- Relationship: `fixtures`, `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `ascii-render`, `source-archive`, `web-ascii`, `web-full`
- Local evidence: `crates/merman-ascii`, `crates/merman-ascii/tests/testdata/mermaid-ascii`
- Legal files:
  - [`THIRD_PARTY_LICENSES/mermaid-ascii/LICENSE`](THIRD_PARTY_LICENSES/mermaid-ascii/LICENSE) (license, SHA-256 `2568bfc33918af28d45f8ba672c93c8fbdcdfbc430b8ba86228b8ea1c2469b31`)

### mermaid-rs-renderer (`mermaid-rs-renderer`)

This Rust renderer is retained as an ASCII and headless rendering behavior reference.

- Version: `0.2.0`
- Source: <https://github.com/1jehuang/mermaid-rs-renderer>
- Source ref: `master`
- Source commit: `859253415e69dce28bd65cd5a7c1d1ae8b39f4a1`
- Source path: `.`
- Relationship: `behavior-reference`
- License expression: `MIT`
- Artifact scopes: `ascii-render`, `source-archive`, `web-ascii`, `web-full`
- Local evidence: `crates/merman-ascii`
- Legal files:
  - [`THIRD_PARTY_LICENSES/mermaid-rs-renderer/LICENSE`](THIRD_PARTY_LICENSES/mermaid-rs-renderer/LICENSE) (license, SHA-256 `57ed7943c34463678a150769d4a4f6c95d2a190fe2c1977f74bc883492c94b86`)

### monaqa/tree-sitter-mermaid (`monaqa-tree-sitter-mermaid`)

The language package uses this grammar as the fixed downstream editor compatibility reference; it is not the public CST schema authority.

- Version: `0.0.2`
- Source: <https://github.com/monaqa/tree-sitter-mermaid.git>
- Source ref: `master`
- Source commit: `90ae195b31933ceb9d079abfa8a3ad0a36fee4cc`
- Source path: `.`
- Relationship: `behavior-reference`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid`
- Legal files:
  - [`THIRD_PARTY_LICENSES/tree-sitter-mermaid-monaqa/LICENSE`](THIRD_PARTY_LICENSES/tree-sitter-mermaid-monaqa/LICENSE) (license, SHA-256 `40e46efcec726e70cc95c553ac377674f0a5d2eec6089483ba237af7dae4c54c`)

### non-layered-tidy-tree-layout (`non-layered-tidy-tree-layout`)

Merman's Mindmap tidy-tree layout is a modified Rust translation of the non-layered tidy-tree contour and threading algorithm bundled by Mermaid's tidy-tree adapter.

- Version: `2.0.2`
- Source: <https://github.com/stetrevor/non-layered-tidy-tree-layout.git>
- Source ref: `v2.0.2`
- Source commit: `63852fb7646eb079ce0e1976044d54545de273e8`
- Source path: `src`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/merman-render/src/mindmap/tidy_tree.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/non-layered-tidy-tree-layout/LICENSE`](THIRD_PARTY_LICENSES/non-layered-tidy-tree-layout/LICENSE) (license, SHA-256 `9e618475934f16f1966a1f47cacea53ec0e8fbf7cb0054a5063b9446794a25df`)

### pappasam/tree-sitter-mermaid (`pappasam-tree-sitter-mermaid`)

The language package modifies selected grammar helpers and Flowchart/Mindmap seeds; metadata/derivations.json records each local path and source range.

- Version: `0.1.0`
- Source: <https://github.com/pappasam/tree-sitter-mermaid.git>
- Source ref: `main`
- Source commit: `1a11e2d8cf11afcfdb768f537c1a9bde294c24f9`
- Source path: `.`
- Relationship: `behavior-reference`, `modified`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid`
- Legal files:
  - [`THIRD_PARTY_LICENSES/tree-sitter-mermaid-pappasam/LICENSE`](THIRD_PARTY_LICENSES/tree-sitter-mermaid-pappasam/LICENSE) (license, SHA-256 `66f5a051ab96d2bb3ecccf32f6db1d97245dfcee2f0563de3e3267e827717061`)

### RaTeX (`ratex`)

Optional math rendering links the RaTeX 0.1.14 crate family; its separately licensed embedded fonts are recorded as their own component.

- Version: `0.1.14`
- Source: <https://github.com/erweixin/RaTeX.git>
- Source ref: `crates.io:ratex-*@0.1.14`
- Source commit: `ae391d727ac615437c63c308f4538d971a84bede`
- Source path: `crates`
- Relationship: `linked`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `ratex-render`, `source-archive`, `web-full`, `web-render`
- Local evidence: `Cargo.lock`, `crates/merman-render/src/math.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/ratex/LICENSE`](THIRD_PARTY_LICENSES/ratex/LICENSE) (license, SHA-256 `f65e10eaa978c50a58c5e792110b4be5014b865edcb2ade49076bfcc98fa92b0`)
  - [`THIRD_PARTY_LICENSES/ratex/THIRD_PARTY_NOTICES.txt`](THIRD_PARTY_LICENSES/ratex/THIRD_PARTY_NOTICES.txt) (notice, SHA-256 `e3cf753ddd8012543a3297b62b4cf5450db1d5a3ae7ddd7afae593db927c2980`)

### rough-rs roughr (`rough-rs`)

roughr-merman is a modified in-tree fork of the rough-rs roughr crate.

- Version: `0.12.0`
- Source: <https://github.com/orhanbalci/rough-rs.git>
- Source ref: `roughr@0.12.0`
- Source commit: `b1c2d96c944da4e74275aa09892be14e2d54445a`
- Source path: `roughr`
- Relationship: `copied`, `modified`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/roughr`
- Legal files:
  - [`THIRD_PARTY_LICENSES/rough-rs/LICENSE`](THIRD_PARTY_LICENSES/rough-rs/LICENSE) (license, SHA-256 `0bef4264af5b4af4de4b01700f27afb7bdaf7498949692ff272ebf24489b0531`)

### Rough.js (`roughjs`)

The roughr fork aligns its randomization and drawing-operation semantics with Rough.js as used by Mermaid.

- Version: `4.6.6`
- Source: <https://github.com/pshihn/rough.git>
- Source ref: `npm:roughjs@4.6.6`
- Source commit: `56a2762171b1294d643501e8d14f120db6b27bd7`
- Source path: `src`
- Relationship: `behavior-reference`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/roughr`
- Legal files:
  - [`THIRD_PARTY_LICENSES/roughjs/LICENSE`](THIRD_PARTY_LICENSES/roughjs/LICENSE) (license, SHA-256 `dca9a392272606ac748ac0976a2a1133f14eef841c27beaa51a844d53c56a09d`)

### sanitize-url (`sanitize-url`)

Merman's URL sanitization behavior is a source-backed Rust translation of sanitize-url.

- Version: `7.1.1`
- Source: <https://github.com/braintree/sanitize-url.git>
- Source ref: `v7.1.1`
- Source commit: `b1e8d50e4066a9af00fa042176676374747f754b`
- Source path: `src`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-analysis`, `web-ascii`, `web-editor`, `web-full`, `web-render`
- Local evidence: `crates/merman-core/src/utils.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/sanitize-url/LICENSE`](THIRD_PARTY_LICENSES/sanitize-url/LICENSE) (license, SHA-256 `0984740e0c3d725c8044dec7edcefe1dbce180ef5a7bc710c251e19607000158`)

### singularity-parser-mermaid (`singularity-tree-sitter-mermaid`)

The language package retains this implementation as an additional behavior reference for grammar and query coverage.

- Version: `0.9.1`
- Source: <https://github.com/singularity-ng/singularity-parser-mermaid.git>
- Source ref: `main`
- Source commit: `f5ac2752fbf0f74f9c836014b87e511303d2abae`
- Source path: `.`
- Relationship: `behavior-reference`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid`
- Legal files:
  - [`THIRD_PARTY_LICENSES/tree-sitter-mermaid-singularity/LICENSE`](THIRD_PARTY_LICENSES/tree-sitter-mermaid-singularity/LICENSE) (license, SHA-256 `601f9a3a5d582af11bd0386a3352435f8765b1d35bee882e5dc7ebb29cf3b540`)

### Tree-sitter (`tree-sitter-generator`)

The language package uses the pinned generator and copies its generated support headers.

- Version: `0.27.1`
- Source: <https://github.com/tree-sitter/tree-sitter.git>
- Source ref: `v0.27.1`
- Source commit: `10eb5ac119eec9cb4267a8e035066473983f5f3a`
- Source path: `.`
- Relationship: `copied`, `generated`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid`
- Legal files:
  - [`THIRD_PARTY_LICENSES/tree-sitter/LICENSE`](THIRD_PARTY_LICENSES/tree-sitter/LICENSE) (license, SHA-256 `c5cfb43042b6b72045f4ba997834d0a7786d2793d91680868b5815b39f14fc78`)

### Mermaid (tree-sitter-mermaid Agentflow and Usecase additions) (`tree-sitter-mermaid-mermaid-12-new-families`)

The language package translates Agentflow and Usecase syntax from the pinned Mermaid 12.0.0 source and carries source-backed editor corpus cases. Existing diagram-family derivations retain their separately recorded Mermaid 11.16.1 source identity.

- Version: `12.0.0`
- Source: <https://github.com/mermaid-js/mermaid.git>
- Source ref: `mermaid@12.0.0`
- Source commit: `98a0945418c76238f15df2afaddbba4272656c3b`
- Source path: `packages/mermaid`
- Relationship: `behavior-reference`, `fixtures`, `translated`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid/grammar/families/agentflow.js`, `distribution/tree-sitter-mermaid/grammar/families/usecase.js`, `distribution/tree-sitter-mermaid/test/corpus/families/agentflow.txt`, `distribution/tree-sitter-mermaid/test/corpus/families/usecase.txt`, `distribution/tree-sitter-mermaid/test/fixtures/family-roots.json`
- Legal files:
  - [`THIRD_PARTY_LICENSES/mermaid/LICENSE`](THIRD_PARTY_LICENSES/mermaid/LICENSE) (license, SHA-256 `ec9fb67dcb25eccc416ed56e1aab819222c805a2a4bfe4cb19e7556bf2ffde80`)

### Mermaid (tree-sitter-mermaid baseline) (`tree-sitter-mermaid-mermaid-baseline`)

The language package retains the exact Mermaid 11.16.1 syntax baseline for its original 35 diagram families and their representative fixtures. Agentflow and Usecase additions are attributed separately to Mermaid 12.0.0; this component intentionally does not move with the repository baseline.

- Version: `11.16.1`
- Source: <https://github.com/mermaid-js/mermaid.git>
- Source ref: `mermaid@11.16.1`
- Source commit: `7ecca0cd7f1658ef74f4e7e91f925724ef403bbf`
- Source path: `packages/mermaid`
- Relationship: `behavior-reference`, `fixtures`, `translated`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid`
- Legal files:
  - [`THIRD_PARTY_LICENSES/mermaid/LICENSE`](THIRD_PARTY_LICENSES/mermaid/LICENSE) (license, SHA-256 `ec9fb67dcb25eccc416ed56e1aab819222c805a2a4bfe4cb19e7556bf2ffde80`)

### ZenUML Core (tree-sitter-mermaid baseline) (`tree-sitter-mermaid-zenuml-baseline`)

The language package follows the exact ZenUML Core 3.50.1 companion baseline and carries its representative wrapper fixture; this component intentionally does not move with the repository baseline.

- Version: `3.50.1`
- Source: <https://github.com/mermaid-js/zenuml-core.git>
- Source ref: `v3.50.1`
- Source commit: `38404ccc14243ed54ab45b804b2eb6f2ca73af36`
- Source path: `.`
- Relationship: `behavior-reference`, `fixtures`, `translated`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid`
- Legal files:
  - [`THIRD_PARTY_LICENSES/zenuml-core/LICENSE`](THIRD_PARTY_LICENSES/zenuml-core/LICENSE) (license, SHA-256 `d4a77cbf1dc0975cd4be7266972dc6d3a6c6d68d43235384d6e4b6f12934e978`)

### Tree-sitter binding templates (`tree-sitter-templates`)

The language package retains modified C, Rust, and Node binding templates and the WASM build helper from Tree-sitter 0.26.12.

- Version: `0.26.12`
- Source: <https://github.com/tree-sitter/tree-sitter.git>
- Source ref: `v0.26.12`
- Source commit: `808e4b1fc06e269a107c4bd8bd936cc6fde18b00`
- Source path: `.`
- Relationship: `copied`, `modified`
- License expression: `MIT`
- Artifact scopes: `source-archive`, `tree-sitter-mermaid-source`
- Local evidence: `distribution/tree-sitter-mermaid`
- Legal files:
  - [`THIRD_PARTY_LICENSES/tree-sitter/LICENSE`](THIRD_PARTY_LICENSES/tree-sitter/LICENSE) (license, SHA-256 `c5cfb43042b6b72045f4ba997834d0a7786d2793d91680868b5815b39f14fc78`)

### unicode-linebreak (`unicode-linebreak`)

merman-unicode-linebreak is an Apache-2.0 fork of unicode-linebreak 0.1.5, preserving its published Unicode 15.0 generated tables and extending character iteration for cooperative cancellation. UPSTREAM.json records the exact v0.1.5 Git revision and crates.io archive SHA-256 3b09c83c3c29d37506a3e260c08c03743a6bb66a9cd432c6934ab501a190571f.

- Version: `0.1.5`
- Source: <https://github.com/axelf4/unicode-linebreak>
- Source ref: `v0.1.5`
- Source commit: `829adeed3c07ed160c1c721b61806eb902a4c1f0`
- Source path: `.`
- Relationship: `copied`, `modified`
- License expression: `Apache-2.0`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/merman-unicode-linebreak`
- Legal files:
  - [`THIRD_PARTY_LICENSES/unicode-linebreak/LICENSE`](THIRD_PARTY_LICENSES/unicode-linebreak/LICENSE) (license, SHA-256 `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4`)

### @upsetjs/venn.js (`venn-js`)

The Venn family uses a modified Rust translation of the venn.js geometry and layout kernel.

- Version: `2.0.0`
- Source: <https://github.com/upsetjs/venn.js.git>
- Source ref: `v2.0.0`
- Source commit: `350c835aab4a92a7570963c28f725cf9f6e5f258`
- Source path: `src`
- Relationship: `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-full`, `web-render`
- Local evidence: `crates/merman-core/src/diagrams/venn.rs`, `crates/merman-render/src/venn.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/venn-js/LICENSE`](THIRD_PARTY_LICENSES/venn-js/LICENSE) (license, SHA-256 `6a3508febf2cfccfee96597394543a6154a4bb0b1f91f28404be42c09e9fcb54`)

### wasm-minimal-protocol (`wasm-minimal-protocol`)

The Typst WASM transport links wasm-minimal-protocol; its upstream license file is the Unlicense text.

- Version: `0.2.1`
- Source: <https://github.com/typst-community/wasm-minimal-protocol.git>
- Source ref: `wasm-minimal-protocol-0.2.1`
- Source commit: `b595eb790d9926dd91afc37d98c2e1fd3102cb2a`
- Source path: `crates/macro`
- Relationship: `linked`
- License expression: `Unlicense`
- Artifact scopes: `source-archive`, `typst-publish`
- Local evidence: `Cargo.lock`, `crates/merman-typst-plugin`
- Legal files:
  - [`THIRD_PARTY_LICENSES/wasm-minimal-protocol/LICENSE`](THIRD_PARTY_LICENSES/wasm-minimal-protocol/LICENSE) (license, SHA-256 `6b0382b16279f26ff69014300541967a356a666eb0b91b422f6862f6b7dad17e`)

### ZenUML Core (`zenuml-core`)

Merman's ZenUML grammar, model, renderer, emoji/icon data, and behavior probes follow the admitted ZenUML Core 3.50.1 source baseline.

- Version: `3.50.1`
- Source: <https://github.com/mermaid-js/zenuml-core.git>
- Source ref: `v3.50.1`
- Source commit: `38404ccc14243ed54ab45b804b2eb6f2ca73af36`
- Source path: `.`
- Relationship: `behavior-reference`, `copied`, `modified`, `translated`
- License expression: `MIT`
- Artifact scopes: `cli-default`, `cli-release`, `elk-render`, `playground-reference`, `ratex-render`, `rust-render-base`, `source-archive`, `typst-publish`, `web-analysis`, `web-ascii`, `web-editor`, `web-full`, `web-render`
- Local evidence: `crates/merman-core/src/diagrams/zenuml`, `crates/merman-render/assets/zenuml`, `crates/merman-render/src/zenuml.rs`
- Legal files:
  - [`THIRD_PARTY_LICENSES/zenuml-core/LICENSE`](THIRD_PARTY_LICENSES/zenuml-core/LICENSE) (license, SHA-256 `d4a77cbf1dc0975cd4be7266972dc6d3a6c6d68d43235384d6e4b6f12934e978`)

## Additional Generated Inventories

- `THIRD_PARTY_LICENSES/rust-cargo-dependencies.json`: cargo-about normalized runtime dependency closure (required).

## Verification

The verifier is offline and fails closed on unknown schema fields, lock drift, missing or
unregistered files, SHA-256 mismatches, invalid artifact scopes, and notice drift:

```bash
python3 scripts/verify-third-party-licenses.py
```
