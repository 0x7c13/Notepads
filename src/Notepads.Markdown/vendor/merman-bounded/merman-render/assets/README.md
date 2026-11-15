# Merman Render Assets

This directory contains renderer support files whose loading and provenance are owned by `merman-render`.

- `katex_flowchart_probe.cjs` is a runtime file loaded relative to `CARGO_MANIFEST_DIR` by the optional Node.js KaTeX probe used for HTML and math measurement audits.
- `zenuml/*.svg` contains source-backed ZenUML symbols embedded into the Rust library with `include_str!`; [`zenuml/README.md`](zenuml/README.md) records their exact upstream version, hashes, and license.

New assets must state whether they are loaded at runtime or embedded at compile time, remain within this crate's ownership boundary, and carry the required upstream provenance and license material.
