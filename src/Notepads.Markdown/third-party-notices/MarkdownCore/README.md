# MarkdownCore historical third-party notices

This directory preserves the **2026-10-24 notice snapshot** in
[dependency-notices.json](dependency-notices.json). Its captured normal/build
closure contains 158 third-party packages for Windows MSVC x86, x64 and ARM64;
x86/x64 included core_detect and ARM64 did not. Build-time procedural macros
were included. This snapshot's lock hash, package versions, features, target
closures and notice hashes remain provenance for that captured graph, not a
claim that it is the complete current product closure.

Resolve the current product FFI normal/build graph from the active Cargo.toml
and [Cargo.lock](../../Cargo.lock) for the intended target. Retained-fork source
identities and modifications are recorded in [vendor/forks.json](../../vendor/forks.json)
and [FORKS.md](../../FORKS.md). Current notice coverage combines this preserved
snapshot with [NativeSvg/dependency-notices.json](../NativeSvg/dependency-notices.json),
[NativeSvg/supplemental-inventory.json](../NativeSvg/supplemental-inventory.json),
[emoji shortcode notices](../EmojiShortcodes/README.md),
[additional code-fence grammar notices](code-syntaxes/README.md), and
[registry notices for the TOML front-matter parser](front-matter-toml/dependency-notices.json).
The TOML inventory covers the production normal/build dependency graph. Inventory
membership records notice coverage for the captured packages.

The snapshot retains each captured crate release, original archive SHA256,
published crate VCS commit where supplied, declared license, enabled features
and original notice files. Each notice records a package-relative path, byte
count and SHA256 for its frozen body. Legacy latest_stable values retain their
original 2026-10-11 check date; they are not newly checked latest releases.

There are 305 package notice records in the 2026-10-24 inventory. The 41 original
notice files in the immutable preview.9 package are retained byte for byte.
This update additionally includes unicode-normalization's original COPYRIGHT
file. Existing NuGet packages are not rewritten; historical package validation
remains bound to its original source inventory.

The new NativeFlowchartV1 product source uses the bounded Merman typed parser.
Its original MIT/Apache alternatives and embedded-component notices are retained,
including its pinned Mermaid behavior references. The legacy Merman renderer
and Dugong remain development comparisons and are outside the normal/build
product closure. Mermaid diagrams use host system fonts, without redistributing
those fonts. The linked native math font notices remain in `../NativeMath/`.

## License scope

Notepads' own MIT license does not replace dependency or asset licenses.
Original texts and copyright holders are preserved in the per-package files.
In particular:

- Comrak declares BSD-2-Clause; its COPYING also preserves incorporated code
  attributions. Existing WHATWG entity-data attribution remains recorded.
- Many crates offer MIT or Apache-2.0; adler2 also offers 0BSD, and several
  crates offer Unlicense or MIT. The original alternatives are retained.
- zlib-rs is Zlib; miniz_oxide and tinyvec also include Zlib alternatives.
- finl_unicode includes Unicode-DFS-2016, and unicode-ident includes
  Unicode-3.0, in addition to their Rust-code alternatives. regex-syntax's
  Unicode-table license is separately included even though its package-level
  metadata declares MIT or Apache-2.0.
- Syntect Rust code is MIT. Its bundled Sublime Packages syntax assets use the
  separate permissive root license with readable-file/accompanying-file
  exceptions. C#, Rust and YAML have the original MIT notices described below.
- selectors is MPL-2.0. Its unmodified corresponding source archive and the
  original license-steward text are identified in
  [SOURCE-AND-LICENSE.txt](selectors-0.37.0/SOURCE-AND-LICENSE.txt).

This is a source and notice inventory, not a legal certification or a substitute
for the original terms.

## Bundled syntax assets

The vendored syntect release is 5.3.0 at commit
`e4670846ecf16d8832db6c43d531bec466214e27`. Its original
`assets/default_newlines.packdump` is the base input, SHA256
`D740B20C12E40B678B9F1012401E1969AAA5CD55F1AB329FFEB94D746B06A5C0`.
Its 75 syntaxes comprise one built-in Plain Text definition and 74 original
grammar files from Sublime Packages commit
`fa6b8629c95041bf262d4c1dab95c456a0530122`. Provenance is based on the pinned
gitlink, original generation recipe and per-file hashes; byte-identical pack
regeneration has not been claimed. The runtime loads `crates/markdown-core/assets/code-syntaxes/highlight.packdump`,
which combines that base with five additional grammar files. The
[additional grammar inventory](code-syntaxes/syntax-assets.json) records the
fixed base and runtime pack identities, source grammars and licenses.

[NOTEPADS-SYNTAX-NOTICES.txt](syntect-5.3.0/NOTEPADS-SYNTAX-NOTICES.txt)
preserves the syntect copyright, Sublime Packages root license and exceptions,
the C# grammar's 2016 author attribution, Rust directory MIT license, and YAML's
2015 FichteFoll copyright and full inline MIT text. YAML remains in the default
pack when `yaml-load` is disabled. Original Sublime root and Rust license files
are also retained separately.

[SYNTAX-ASSET-PROVENANCE.json](syntect-5.3.0/SYNTAX-ASSET-PROVENANCE.json)
binds the original Syntect base pack, source commits, notices and frozen
[SYNTAX-INPUTS.lock.json](syntect-5.3.0/SYNTAX-INPUTS.lock.json). The latter
records all 93 upstream research-source assets: 12 original scope-test fixtures,
74 grammars, two separate license files and five syntect reference files. All
93 source hashes were rechecked. The fixtures and their original expected
assertions are research inputs; their source files are not shipped as runtime
grammars. The existence of 75 syntax definitions does not qualify 75 languages
or erase known upstream fixture differences.

The vendored-source inventory and local modification description are retained
as NOTEPADS-UPSTREAM.json and NOTEPADS-PATCH.md.
