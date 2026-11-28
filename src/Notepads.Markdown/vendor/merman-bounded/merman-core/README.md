# merman-core

[![Crates.io](https://img.shields.io/crates/v/merman-core.svg)](https://crates.io/crates/merman-core) [![Documentation](https://docs.rs/merman-core/badge.svg)](https://docs.rs/merman-core) [![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-59636e.svg)](https://github.com/Latias94/merman/blob/main/LICENSE-MIT)

`merman-core` is the parser and semantic-model crate behind [`merman`](https://crates.io/crates/merman). Use it when you need Mermaid detection, metadata, compatibility semantic JSON, parser-backed editor facts, or typed render models without pulling in layout, SVG, or binary-export dependencies.

Most applications that want rendered output should use the `merman` facade instead.

This guide targets `merman-core 0.9.1`. For earlier versions, use their tagged documentation and matching dependencies.

## Source Feature Migration

The current checkout keeps empty default features and adds positive `diagram-*` selectors.
Select `all-diagrams` to retain every built-in parser, or select only the families your host accepts:

```toml
merman-core = { path = "../merman/crates/merman-core", default-features = false, features = ["diagram-gantt"] }
```

Family-exclusive typed models and enum variants are conditional. The complete
identity catalog still recognizes disabled families, whose strict parse returns
`UnsupportedDiagram`. `supported_diagrams()` enumerates compiled parsers;
`diagram_family_capabilities()` retains complete identities with actual implementation flags.
See the [feature guide](../../docs/FEATURES.md#select-diagram-families) for custom registries,
suppression, and Cargo feature-unification rules.

## Quick Start

Add the stable release with all built-in parsers:

```sh
cargo add merman-core@0.9.1 --features all-diagrams
```

Parse Mermaid into its compatibility semantic JSON projection:

```rust
use merman_core::{Engine, ParseOptions};

fn main() -> Result<(), merman_core::Error> {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_sync("flowchart TD; A[API] --> B[DB];", ParseOptions::strict())?
        .expect("diagram detected");

    assert_eq!(parsed.metadata().diagram_type, "flowchart-v2");
    println!("{}", parsed.model());

    Ok(())
}
```

## What It Provides

- Mermaid detection and preprocessing, including front matter and directives.
- Strict and lenient parsing through `ParseOptions`.
- Structured parse diagnostics with exact spans, insertion points, or explicit fallback locations.
- Compatibility semantic JSON via `Engine::parse_diagram_sync`.
- Typed render models via `Engine::parse_diagram_for_render_model_sync`.
- Parser-backed editor facts and family capability metadata.
- Metadata-only parsing for integrations that only need type, title, and effective config.
- Project-owned civil and offset time types that preserve Mermaid's wide year domain.
- Runtime-agnostic async APIs plus synchronous helpers.

`merman-core` has no default Cargo features. Select `all-diagrams` or the required `diagram-*`
families to compile built-in parsers and typed models. Configuration, sanitization, detection, and
complete identity facts remain available without a family; optional `system-*` features make
explicit host runtime adapters available.

Relative operation deadlines use the native monotonic clock on supported targets. Browser-facing
`wasm32-unknown-unknown` artifacts must enable `operation-deadlines` to expose the deadline methods
and use the Web monotonic clock. This keeps browser adapters out of pure WASM dependency closures;
cancellation remains available in every artifact.

## Deterministic Time

Use `time::CivilDate` for date-only runtime controls such as `Engine::with_fixed_today`. Its canonical text syntax uses four unsigned digits for years `0000` through `9999`, a leading `+` for later years, and `-` plus at least four digits for negative years. The signed 32-bit year domain includes Mermaid's `+10000` and `-10000` boundaries. `time::CivilDateTime`, `time::UtcOffset`, and `time::OffsetDateTime` provide checked calendar and instant conversions without exposing a third-party time type in the public API.

System clock and named time-zone access remain opt-in through the corresponding `system-*` features. Named-zone rules are implemented with Jiff internally, while deterministic and fixed-offset profiles do not pull Jiff into their dependency closure.

## Skip Detection When The Type Is Known

Markdown renderers often know the diagram type from the fence info string. Use the `*_with_type_sync` APIs to skip detection.

```rust
use merman_core::{Engine, ParseOptions};

fn main() -> Result<(), merman_core::Error> {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_with_type_sync(
            "sequence",
            "sequenceDiagram\nAlice->>Bob: Hello",
            ParseOptions::strict(),
        )?
        .expect("diagram detected");

    assert_eq!(parsed.metadata().diagram_type, "sequence");
    Ok(())
}
```

Common internal ids include `flowchart-v2`, `sequence`, `classDiagram`, `stateDiagram`, `architecture`, `mindmap`, and `gantt`.

## Rendering Handoff

If the next step is low-level layout or SVG rendering, prefer
`Engine::parse_diagram_for_render_model_sync`. It returns the typed render projection of the same
family-owned semantics and avoids building a large compatibility JSON tree. Applications that
start with Mermaid source and want complete SVG, ASCII, layout JSON, or binary output should
normally use `merman::Renderer` with a typed target request; that facade carries the same
projection through the canonical operation control, runtime context, and resource policy.

```rust
use merman_core::{Engine, ParseOptions};

fn main() -> Result<(), merman_core::Error> {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_for_render_model_sync("flowchart TD; A --> B", ParseOptions::strict())?
        .expect("diagram detected");

    println!(
        "{} -> {}",
        parsed.metadata().diagram_type,
        parsed.model().kind()
    );
    Ok(())
}
```

## Compatibility

Semantic JSON uses `ManagedSemanticJson`; keep its managed owner for deep clone, equality, destruction, and output. Direct State, Block, Treemap, and Ishikawa model users must migrate to canonical document/child indices. See the [0.8.0 to 0.9.0 migration](../../docs/release/V080_TO_V090_UPGRADE_GUIDE.md#rust-models-and-json).

Merman 0.9.1 retains the Mermaid `12.1.0` baseline selected by stable 0.8.0. The selected pinned upstream behavior graph is the compatibility authority.
Compatibility semantic JSON is the public serialized parser projection. It is not a second
successful grammar or the master built-in render input. Typed render models and editor facts
project the same family semantic construction into purpose-specific shapes.

The built-in Diagram Family catalog is the authoritative source for ids, aliases, detector order, parser/editor/render capabilities, metadata, configuration namespaces, and authoring headers. The pinned Mermaid catalog is complete and independent of Cargo feature selection. Custom parser overlays remain explicit and do not inherit a built-in renderer or editor capability.

The public Rust flowchart render type is `diagrams::flowchart::FlowchartModel`. The former `FlowchartV2Model` type name was removed during the architecture reset without a deprecated alias. This rename does not change Mermaid's `flowchart-v2` diagram id or the compatibility layout JSON `FlowchartV2` variant key.

Core does not decide user-visible diagnostic merge policy. It reports parser facts and capability gaps; `merman-analysis` owns rule ids, Markdown remapping, recovered-parser deduplication, and editor-facing fallback policy.

## Migration Notes

`Error::DiagramParse` carries `diagnostic: ParseDiagnostic` instead of a raw parse-message field. Call `diagnostic.message()` for display text, and use `diagnostic.span()`, `diagnostic.span_kind()`, and `diagnostic.code()` when an integration can preserve structured parser metadata.

Railroad repetition bounds use `RailroadRepeatBound` for both `min` and `max`. Use `ZERO`, `ONE`, or `RailroadRepeatBound::from(value)` for finite bounds and `RailroadRepeatBound::INFINITY` for an unbounded maximum. Finite bounds serialize as JSON numbers, while infinity serializes as `null`.

## Maintainer Parser Generation

The Class, ER, Flowchart, Sequence, and State grammars use checked-in LALRPOP output. Downstream builds compile those parsers directly and do not run the LALRPOP generator. After editing any `src/diagrams/*_grammar.lalrpop` file, regenerate and verify the complete five-parser set:

```console
cargo run -p xtask -- gen-lalrpop-parsers
cargo run -p xtask -- verify-lalrpop-parsers
```

`verify-generated` includes the same byte-for-byte freshness check.

See the [parser generation guide](https://github.com/Latias94/merman/blob/main/docs/development/PARSER_GENERATION.md) for source ownership, transaction semantics, review expectations, and the parser/editor/LSP verification sequence.
