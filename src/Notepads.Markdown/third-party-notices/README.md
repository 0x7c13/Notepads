<!-- Historical source-adoption notes follow. Current product name: Notepads.Markdown. 
Original versions, upstream identities, license text and qualification epochs are preserved; 
statements about candidate enablement below describe their original adoption epoch. 
Current support and validation are documented in the module README and docs/markdown. -->

# Third-party notices

Notepads's original code is MIT licensed. Third-party files retain their original terms.

The optional `quality-observation` test feature uses RustCrypto sha2 0.11.1
with default features disabled to hash the actual Rust font owner. Seven new
MIT OR Apache-2.0 packages and their unchanged notices/archive checksums are
listed in `SvgQuality/dependency-notices.json`. The original 238 lock entries
retain their versions and checksums. These diagnostic dependencies are not
enabled by the default product build and have no new UWP/AOT qualification.
Version/license provenance is the [published sha2 package](https://crates.io/crates/sha2/0.11.1)
and [upstream RustCrypto source](https://github.com/RustCrypto/hashes/tree/master/sha2).

| Component | Version | Provenance | Use |
| --- | --- | --- | --- |
| Rust standard library | 1.99.0 (delivered M0); 1.99.1 (isolated UWP candidate) | Official rustup toolchains `share/doc/rust/COPYRIGHT-library.html` and `licenses/` | The complete upstream library notices are retained; their bytes match across these versions |
| Microsoft.Windows.CppWinRT | 3.0.260818.1 | Official NuGet package `LICENSE` | Generated/native projection support, MIT |
| Microsoft.Windows.CsWinRT | 2.3.1 | Official NuGet package `LICENSE` and `NOTICE.txt` | Generated C# projection/runtime, MIT plus upstream notices |

The isolated Rust 1.99.1 UWP standard-library candidate changes the UWP random
backend to BCryptGenRandom and disables the default panic console output
(`None::<io::Sink>`). Two additional files contain only UWP conditional-compilation
cleanup for the unused path import and ProcessPrng declaration. These four source
overrides retain the upstream terms and identify the modifications. This candidate
has built for three UWP targets. The exact C0 .8 package has completed the fixed-host x86/x64 AppContainer Native AOT checks; ARM64 was built only, and later source changes require new package qualification. Long-term adoption of this custom standard library and
the loss of default panic/backtrace/abort console text have not been accepted.

The independent Playground also consumes Microsoft.UI.Xaml 2.8.7, Microsoft.Web.WebView2 SDK 1.0.4258.31, and Microsoft Windows/.NET SDK tools under their package terms. These are separate dependencies; their binary package terms are not replaced by Notepads's MIT license. The control does not render Markdown through WebView2. The WebView2 SDK is a WinUI 2 transitive dependency.

Versions were initially checked on 2026-10-07. The official NuGet version lists for CppWinRT, CsWinRT, Microsoft.UI.Xaml and WebView2 were rechecked on 2026-10-11; the pinned versions remain the latest stable versions in those lists. The saved primary responses are in artifacts/evidence/m1/core-chain/dependencies-v1/nuget/. Dependency versions are pinned for reproduction; the minimum Windows version and UWP host remain explicit platform requirements.

The isolated controlled-memory research helper additionally uses triomphe 0.1.16 with
default features disabled, under MIT OR Apache-2.0. The unchanged upstream license
texts are retained in `Triomphe/`. Its version was checked against the official
crates.io sparse index on 2026-10-08; the original archive SHA256 is
`b40688ea6389c8171614b25491f71d4a27946e0c7ce2da1c6de27e25abf1a0ae`.
This research dependency has not been adopted by the production component or
validated in the UWP/AppContainer package chain.

The isolated V3 verifier directly uses caseless 0.2.2 for allocation-free Unicode
casefold iteration. This package already occurs in the fixed Comrak research
dependency graph; its latest stable version and original archive checksum were
rechecked against the official sparse index on 2026-10-08. Its original MIT
license and the original Unicode notices in `CaseFolding.txt` are retained in
`Caseless/`. The production C0 core now also uses this package through Comrak; the verified normal/build closure and unchanged upstream notices are registered in MarkdownCore/dependency-notices.json. The exact C0 .8 source/package has passed the recorded three-RID build and fixed-host x86/x64 UWP/AppContainer Native AOT chain; this does not qualify later source changes.

The preserved MarkdownCore Rust normal/build notice snapshot was captured on 2026-10-24.
`MarkdownCore/dependency-notices.json` records 158 third-party packages and
305 package notice records across three MSVC targets, including bounded math,
the NativeFlowchartV1 typed parser, Windows text bindings and host macros.
It retains original archive hashes, license expressions and unchanged notices;
membership does not assert that every function is linked or UWP-qualified.
Legacy Merman rendering and Dugong are dev-only comparisons. Existing immutable
packages retain their original inventories and qualification boundaries.
The historical MarkdownCore snapshot does not describe the complete current graph.
Resolve current product dependencies from the active Cargo.toml/Cargo.lock;
`vendor/forks.json` and `FORKS.md` bind current retained-fork identities.
Current package notice coverage combines MarkdownCore with
`NativeSvg/dependency-notices.json` and `NativeSvg/supplemental-inventory.json`.
The MarkdownCore snapshot preserves its captured lock, version, feature and notice hashes.

SVG scalar roles use Unicode 17.0.0
[GraphemeBreakProperty data](https://www.unicode.org/Public/17.0.0/ucd/auxiliary/GraphemeBreakProperty.txt).
The data notice is retained in `Unicode17Grapheme/Unicode-license.txt`.

See `MarkdownCore/README.md` for source, attribution and syntax-asset details,
and `NativeMath/NOTICE.txt` for the fixed KaTeX font assets. Comrak's BSD terms,
WHATWG data attribution, dependency alternatives, MPL-2.0 files and font OFL
terms remain separate from Notepads' own MIT license. ARM64 device runtime,
minimum-OS/WACK and clean production deployment remain outside current results.
