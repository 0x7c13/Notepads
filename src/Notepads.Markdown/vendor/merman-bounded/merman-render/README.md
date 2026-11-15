# merman-render

[![Crates.io](https://img.shields.io/crates/v/merman-render.svg)](https://crates.io/crates/merman-render) [![Documentation](https://docs.rs/merman-render/badge.svg)](https://docs.rs/merman-render) [![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-59636e.svg)](https://github.com/Latias94/merman/blob/main/LICENSE-MIT)

`merman-render` is the low-level layout and SVG crate behind [merman](https://crates.io/crates/merman). It consumes typed `merman-core` family semantics and produces compatibility layout JSON or Mermaid-like SVG through one family artifact.

> **Implementation crate:** this crate is published to support Merman's Cargo dependency chain,
> not as the normal product entry point. Applications should depend on
> [`merman`](https://crates.io/crates/merman) and use its operation-scoped `Renderer` with a typed
> target request.

Direct use is reserved for Merman maintainers and advanced integrations that deliberately own the typed core model, render session, text measurement, layout, and SVG postprocessing lifecycle.

## What It Provides

- Headless layout for parsed Mermaid diagrams.
- Mermaid-parity SVG emission.
- `FamilyRenderArtifact`, which keeps one matching built-in semantic/layout pair opaque and projects layout JSON or consuming SVG output.
- `LayoutOptions::headless_svg_defaults()` for editor/export use cases.
- Text measurement hooks through `TextMeasurer`.
- Math rendering hooks through `MathRenderer`.
- Shared Root Viewport policy for computed sizing, accessibility chrome, and root SVG emission.
- `SvgPipeline` presets and postprocessors for readable or rasterizer-friendly SVG.

## Feature Selection

Defaults are empty. Select `all-diagrams` or the required `diagram-*` families for layout and SVG
handlers; these selectors also forward to core. SVG itself is intrinsic to this crate. For example,
`default-features = false, features = ["diagram-flowchart", "diagram-gantt"]` selects those two
families without optional engines or math. Family payloads and artifacts are conditional public
types. If another dependency widens core, a family missing its local render handler returns an
explicit unsupported result before backend planning. See the
[migration guide](../../docs/FEATURES.md#select-diagram-families).

Optional features add distinct backends or system adapters:

| Feature | Adds |
| --- | --- |
| `layout-cytoscape` | Architecture FCoSE and non-`tidy-tree` Mindmap COSE-Bilkent layout through `manatee`. |
| `layout-elk` | Source-backed ELK layered layout for Flowchart ELK, Class, and ER. |
| `math` | RaTeX parsing, layout, SVG output, and embedded math fonts. |
| `system-clock`, `system-timezone`, `system-random`, `system-timing` | Explicit host runtime adapters; none are selected by default. |

Omitting an optional backend preserves parsing and semantic support. Flowchart, Class, and ER follow Mermaid 12's registered-layout lookup: an unknown layout, or an ELK request in a build without `layout-elk`, resolves to Dagre before capability admission. The original requested configuration remains available in the prepared artifact metadata. An installed backend denied by the host policy still returns a typed capability error. Layout failures, cancellation, work limits, and missing math support do not trigger layout fallback.

## Render Environment

`RenderEnvironment` owns SVG-specific adapters and policy for one operation: named
text-measurement routes, math rendering, icons, time, randomness, and resource limits. Low-level
callers bind a caller-owned `OperationContext` and `OperationControl` with
`begin_session_in_context()` before layout and retain that `RenderSession` through SVG
postprocessing so every phase observes the same snapshot and provenance. The higher-level
`Renderer` captures that context once and also applies its frozen date and timezone to
date-sensitive parsing; direct low-level callers are responsible for configuring the core
`Engine` consistently.

`TextMeasurer` keeps browser DOM primitives distinct. In particular, `measure_svg_create_text_bbox_y_offset_px` measures ordinary Mermaid createText, while `measure_svg_create_text_middle_bbox_y_offset_px` measures Architecture's formatted text under an inherited middle baseline. The latter is font- and x-height-dependent and cannot reuse the former. The built-in deterministic measurer is a font-agnostic fallback, not a named-font or browser formula; an authoritative host measurement bypasses it.

Use `DeterministicTextMeasurer::with_width_callback(...)` when the application can always return a width for a complete string and Merman should handle wrapping. If measurement can fail or must vary by operation, use `HostTextMeasurer` so Merman can validate the result, fall back, and report which source it used.

This is a breaking replacement for independently configured layout and SVG services. Text and math adapters no longer live in `LayoutOptions`, and render code does not read process-global policy. Production request values stay in `SvgRenderOptions`; diagnostics, including timing output, live in `SvgDebugOptions` and are accepted only by the explicit `*_with_debug` entry points.

## Low-Level Pipeline Example

```rust
use merman_core::{Engine, OperationControl, ParseOptions};
use merman_render::{
    environment::RenderEnvironment, family, LayoutOptions,
};
use merman_render::svg::{
    SvgDebugOptions, SvgPipeline, SvgPostprocessMetadata, SvgRenderOptions,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_for_render_model_sync(
            "flowchart TD\nA[API] --> B[DB]",
            ParseOptions::strict(),
        )?
        .expect("diagram detected");

    let layout_options = LayoutOptions::headless_svg_defaults();
    let control = OperationControl::new();
    let session = RenderEnvironment::deterministic().begin_session_with_control(control)?;
    let artifact = family::prepare(parsed, &layout_options, session)?;

    // Compatibility layout JSON projects from this exact typed family artifact.
    let layout_json = artifact.layout_json()?;
    eprintln!("layout family: {}", layout_json["meta"]["diagram_type"]);

    let svg_options = SvgRenderOptions {
        diagram_id: Some("example-diagram".to_string()),
        ..SvgRenderOptions::default()
    };

    // SVG consumes the artifact, so its semantic model and layout cannot be recombined.
    let rendered = artifact.render_svg(&svg_options, &SvgDebugOptions::default())?;
    let (svg, family_kind, metadata, session) = rendered.into_parts();
    assert_eq!(family_kind, family::RenderFamilyKind::Flowchart);
    let pipeline_metadata = SvgPostprocessMetadata::from_svg(&svg)
        .with_family_kind(family_kind)
        .with_diagram_type(metadata.diagram_type)
        .with_optional_diagram_title(metadata.title);
    let svg = SvgPipeline::resvg_safe()
        .process_to_string_with_metadata(&svg, &pipeline_metadata, &session)?;
    println!("{svg}");

    Ok(())
}
```

## SVG Output Pipelines

The default SVG renderer aims for Mermaid DOM parity. Host applications can opt into an output pipeline after rendering:

- `SvgPipeline::parity()` leaves the SVG unchanged.
- `SvgPipeline::readable()` keeps fallback text for `<foreignObject>` labels.
- `SvgPipeline::resvg_safe()` prepares SVG for common `usvg` / `resvg` rasterization paths.
- `ScopedCssPostprocessor`, `CssOverridePostprocessor`, and custom `SvgPostprocessor` implementations let applications inject host-specific styling without forking the renderer.

See [`docs/rendering/SVG_OUTPUT_PIPELINE.md`](https://github.com/Latias94/merman/blob/main/docs/rendering/SVG_OUTPUT_PIPELINE.md) for the higher-level integration guide.

## Relationship To merman

`merman` re-exports the target-local SVG vocabulary behind its `svg` feature and owns the
source-to-output `Renderer`, typed target requests, `SemanticArtifact`, SVG id sanitization, and
optional binary targets. Direct `merman-render` users call `family::prepare` and retain its
`RenderSession`; the old public raw model/layout SVG helpers and per-family pass-through wrappers
are not retained as compatibility paths.
