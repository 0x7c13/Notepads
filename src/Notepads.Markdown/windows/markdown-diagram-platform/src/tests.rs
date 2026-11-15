// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_memory::{AllocationAuthority, Family, MemoryError, MemoryScope};
use markdown_mermaid::canonical::*;
use merman_core::OperationControl;
use std::{
    alloc::Layout,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use windows::{
    Win32::Graphics::DirectWrite::*,
    core::{Interface, PCWSTR, w},
};

struct Authority {
    used: AtomicUsize,
    peak: AtomicUsize,
    work: AtomicUsize,
    work_limit: usize,
    limit: usize,
}

fn test_output_directory() -> &'static std::path::Path {
    static OUTPUT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    OUTPUT.get_or_init(|| {
        let root = std::env::var_os("NOTEPADS_MARKDOWN_TEST_OUTPUT")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join("Notepads.Build/Markdown/tests")
            });
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = root.join(format!("diagram-platform-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    })
}

fn export_successful_svg(name: &str, svg: &[u8]) {
    if std::env::var_os("NOTEPADS_MARKDOWN_TEST_OUTPUT").is_some_and(|value| !value.is_empty()) {
        let path = test_output_directory().join(name);
        std::fs::write(&path, svg).unwrap();
        eprintln!("SVG artifact: {}", path.display());
    }
}

impl AllocationAuthority for Authority {
    fn reserve(&self, l: Layout, _: Family) -> Result<(), MemoryError> {
        let old = self
            .used
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(l.size()).filter(|n| *n <= self.limit)
            })
            .map_err(|_| MemoryError::BudgetLimited)?;
        self.peak.fetch_max(old + l.size(), Ordering::SeqCst);
        Ok(())
    }
    fn release(&self, l: Layout, _: Family) {
        self.used.fetch_sub(l.size(), Ordering::SeqCst);
    }
    fn failure(&self) -> Option<MemoryError> {
        None
    }
    fn charge_work(&self, n: usize) -> Result<(), MemoryError> {
        self.work
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                old.checked_add(n).filter(|n| *n <= self.work_limit)
            })
            .map(|_| ())
            .map_err(|_| MemoryError::WorkLimited)
    }
}

fn capture(limit: usize) -> (Arc<Authority>, MemoryScope, OperationControl, WindowsText) {
    capture_with_work(limit, usize::MAX)
}
fn capture_with_work(
    limit: usize,
    work_limit: usize,
) -> (Arc<Authority>, MemoryScope, OperationControl, WindowsText) {
    let authority = Arc::new(Authority {
        used: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        work: AtomicUsize::new(0),
        work_limit,
        limit,
    });
    let scope = MemoryScope::new(authority.clone());
    let control = OperationControl::new().with_memory_scope(scope.clone());
    let factory: IDWriteFactory =
        unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }.unwrap();
    let mut collection = None;
    unsafe { factory.GetSystemFontCollection(&mut collection, false) }.unwrap();
    let collection = collection.unwrap();
    let factory2 = factory.cast::<IDWriteFactory2>().unwrap();
    let builder = unsafe { factory2.CreateFontFallbackBuilder() }.unwrap();
    let names = [
        w!("Segoe UI").0,
        w!("Microsoft YaHei UI").0,
        w!("Segoe UI Symbol").0,
        w!("Segoe UI Emoji").0,
    ];
    unsafe {
        builder.AddMapping(
            &[DWRITE_UNICODE_RANGE {
                first: 0,
                last: 0x10ffff,
            }],
            &names,
            &collection,
            PCWSTR::null(),
            PCWSTR::null(),
            1.0,
        )
    }
    .unwrap();
    let fallback = unsafe { builder.CreateFontFallback() }.unwrap();
    let mut locale = [0u16; 85];
    for (out, ch) in locale.iter_mut().zip("en-US".encode_utf16()) {
        *out = ch;
    }
    let capability = FontCapability {
        version: 1,
        size: std::mem::size_of::<FontCapability>() as u32,
        factory: factory.as_raw(),
        collection: collection.as_raw(),
        fallback: fallback.as_raw(),
        font_epoch: 1,
        locale_name: locale,
    };
    let platform =
        unsafe { WindowsText::from_capability(capability, scope.clone(), control.clone()) }
            .unwrap();
    // The original capability's COM owners really die here. The platform's QI
    // strong refs, rather than borrowed addresses, sustain later worker calls.
    (authority, scope, control, platform)
}

#[test]
fn real_directwrite_baseline_and_outline_cleanup() {
    let (authority, _, _, platform) = capture(8 * 1024 * 1024);
    let run = TextRun {
        text: "a b",
        font: FontPlan {
            epoch: 1,
            ..FontPlan::default()
        },
    };
    let metrics = OutlineProvider::measure(&platform, run).unwrap();
    assert!(metrics.advance > 0.0 && metrics.ascent > 0.0 && metrics.ink.top < 0.0);
    assert!(
        metrics.ink.bottom < run.font.em_size * 0.3,
        "ink is baseline-relative rather than positive top-left"
    );
    struct Count(usize);
    impl PathSink for Count {
        fn push(&mut self, _: PathCommand) -> Result<(), CanonicalError> {
            self.0 += 1;
            Ok(())
        }
    }
    let mut count = Count(0);
    platform.outline(run, &mut count).unwrap();
    assert!(count.0 > 10);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn empty_directwrite_span_keeps_positive_font_metrics() {
    let (authority, _, _, platform) = capture(8 * 1024 * 1024);
    let metrics = OutlineProvider::measure(
        &platform,
        TextRun {
            text: "",
            font: FontPlan {
                epoch: 1,
                ..FontPlan::default()
            },
        },
    )
    .unwrap();
    assert_eq!(metrics.advance, 0.0);
    assert!(metrics.ascent > 0.0 && metrics.descent >= 0.0);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn real_chinese_and_combining_runs_and_canonical_transport() {
    let (authority, scope, control, platform) = capture(16 * 1024 * 1024);
    for text in ["中文混排", "e\u{301}", "Latin λ"] {
        let metrics = OutlineProvider::measure(
            &platform,
            TextRun {
                text,
                font: FontPlan {
                    epoch: 1,
                    ..FontPlan::default()
                },
            },
        )
        .unwrap();
        assert!(
            metrics.advance > 0.0 && metrics.ink.right > metrics.ink.left && metrics.ink.top < 0.0
        );
    }
    let svg = r##"<svg id="d" viewBox="0 0 180 60"><style>#d .node .label{text-align:center}#d text{font-family:Segoe UI;fill:blue;text-anchor:middle}</style><g class="node"><g class="label"><text x="90" y="30">中英 Hello</text></g></g></svg>"##;
    let document = compile(svg, &scope, &control, &platform, CanonicalLimits::default()).unwrap();
    assert!(document.text_runs > 0 && document.path_commands > 10);
    assert!(
        !std::str::from_utf8(document.as_bytes())
            .unwrap()
            .contains("<text")
    );
    drop(document);
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn callback_sink_failure_is_typed_and_restores_all_owned_blocks() {
    let (authority, _, _, platform) = capture(8 * 1024 * 1024);
    struct Reject;
    impl PathSink for Reject {
        fn push(&mut self, _: PathCommand) -> Result<(), CanonicalError> {
            Err(CanonicalError::Limited)
        }
    }
    assert_eq!(
        platform
            .outline(
                TextRun {
                    text: "failure",
                    font: FontPlan {
                        epoch: 1,
                        ..FontPlan::default()
                    }
                },
                &mut Reject
            )
            .err(),
        Some(CanonicalError::Limited)
    );
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn expired_font_epoch_is_rejected_before_directwrite_uses_it() {
    let (authority, _, _, platform) = capture(8 * 1024 * 1024);
    assert_eq!(
        OutlineProvider::measure(
            &platform,
            TextRun {
                text: "old",
                font: FontPlan {
                    epoch: 2,
                    ..FontPlan::default()
                }
            }
        )
        .err(),
        Some(CanonicalError::FontUnavailable)
    );
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn real_native_probe_preserves_formatted_tspan_baseline_and_space_semantics() {
    use merman_render::text::{TextMeasurer, TextStyle};
    let (authority, _, _, platform) = capture(8 * 1024 * 1024);
    let style = TextStyle::default();
    let raw = TextMeasurer::measure_svg_raw_text_bbox_y_offset_px(&platform, "A", &style).unwrap();
    let formatted = TextMeasurer::measure_svg_create_text_bbox_y_offset_px(&platform, "A", &style);
    assert!((formatted - raw - style.font_size).abs() < 1e-9);
    let collapsed = TextMeasurer::measure_svg_tspan_text_bbox_width_px(&platform, "A   A", &style);
    let preserved =
        TextMeasurer::measure_svg_preserved_tspan_text_bbox_width_px(&platform, "A   A", &style);
    assert!(preserved > collapsed);
    assert!(platform.failure().is_none());
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

#[test]
fn real_rich_formats_and_rtl_use_one_direction_and_outline_context() {
    use markdown_mermaid::rich_text::{RichTextProvider, TextDirection};
    let (authority, _, _, platform) = capture(16 * 1024 * 1024);
    let normal = FontPlan {
        epoch: 1,
        ..FontPlan::default()
    };
    let bold = FontPlan {
        weight: 700,
        ..normal
    };
    let italic = FontPlan {
        italic: true,
        ..normal
    };
    struct Count {
        count: usize,
        max: f64,
    }
    impl PathSink for Count {
        fn push(&mut self, c: PathCommand) -> Result<(), CanonicalError> {
            self.count += 1;
            self.max = self.max.max(c.x1.abs()).max(c.x2.abs()).max(c.x3.abs());
            Ok(())
        }
    }
    for spans in [
        [
            TextRun {
                text: "中文 e",
                font: normal,
            },
            TextRun {
                text: "\u{301} Bold",
                font: bold,
            },
            TextRun {
                text: " italic",
                font: italic,
            },
        ],
        [
            TextRun {
                text: "שלום ",
                font: normal,
            },
            TextRun {
                text: "(42)",
                font: bold,
            },
            TextRun {
                text: " עולם",
                font: italic,
            },
        ],
        [
            TextRun {
                text: "مرحباً ",
                font: normal,
            },
            TextRun {
                text: "(42)",
                font: bold,
            },
            TextRun {
                text: " عالم",
                font: italic,
            },
        ],
    ] {
        let metrics = platform.measure_spans(&spans, TextDirection::Auto).unwrap();
        assert!(metrics.advance > 0.0 && metrics.ink.top < 0.0 && metrics.ascent > 0.0);
        assert!(
            metrics.ink.left > -64.0 && metrics.ink.right < metrics.advance + 64.0,
            "RTL must use the same physical origin as LTR"
        );
        let mut count = Count { count: 0, max: 0.0 };
        platform
            .outline_spans(&spans, TextDirection::Auto, &mut count)
            .unwrap();
        assert!(count.count > 10 && count.max < metrics.advance + 64.0);
    }
    assert!(platform.failure().is_none());
    assert_eq!(authority.used.load(Ordering::SeqCst), 0);
}

fn native_theme() -> NativeTheme {
    NativeTheme {
        dark: false,
        foreground: [32, 36, 50, 255],
        background: [255, 255, 255, 255],
        border: [81, 90, 133, 255],
        panel: [241, 243, 255, 220],
        link: [40, 80, 160, 255],
    }
}

#[test]
fn native_production_path_reuses_original_four_sources_with_real_fonts_and_funded_artifact() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for case in [
        "basic",
        "class_style",
        "stress_flowchart_long_labels_punctuation_unicode_006",
        "stress_flowchart_subgraph_title_unicode_and_entities_043",
    ] {
        let (authority, scope, control, platform) = capture_with_work(64 * 1024 * 1024, 1_000_000);
        let source = std::fs::read_to_string(
            repository
                .join("tests/fixtures/mermaid-flowchart-v1")
                .join(case)
                .with_extension("mmd"),
        )
        .unwrap();
        let artifact = render_native(&source, &scope, &control, &platform, native_theme())
            .unwrap_or_else(|e| panic!("native flowchart {case}: {e:?}"));
        let value = std::str::from_utf8(artifact.as_bytes()).unwrap();
        assert!(artifact.width > 0.0 && artifact.height > 0.0 && artifact.path_commands > 10);
        assert!(
            !value.contains("<text")
                && !value.contains("<style")
                && !value.contains("filter=")
                && !value.contains("<foreignObject")
        );
        assert!(
            value.contains("fill-opacity=\"0.8627450980392157\""),
            "captured theme alpha must reach the native scene"
        );
        export_successful_svg(
            &format!("mermaid-v1-native-{case}.svg"),
            artifact.as_bytes(),
        );
        eprintln!(
            "native flowchart {case}: {}x{}, commands={}, bytes={}, peakRequested={}, workUnits={}",
            artifact.width,
            artifact.height,
            artifact.path_commands,
            artifact.as_bytes().len(),
            authority.peak.load(Ordering::SeqCst),
            authority.work.load(Ordering::SeqCst)
        );
        drop(artifact);
        assert_eq!(
            authority.used.load(Ordering::SeqCst),
            0,
            "{case}: real whole-job allocations released"
        );
    }
}

#[test]
fn native_production_path_preserves_unsupported_syntax_cancel_and_budget_reasons() {
    use markdown_mermaid::DiagramError;
    for (source, expected) in [
        ("gantt\ntitle Unsupported family", DiagramError::Unsupported),
        ("flowchart TD\nA[broken", DiagramError::InvalidInput),
    ] {
        let (a, s, c, p) = capture(8 * 1024 * 1024);
        assert_eq!(
            render_native(source, &s, &c, &p, native_theme()).err(),
            Some(expected)
        );
        assert_eq!(a.used.load(Ordering::SeqCst), 0);
    }
    let (a, s, c, p) = capture(0);
    assert_eq!(
        render_native("flowchart TD\nA-->B", &s, &c, &p, native_theme()).err(),
        Some(DiagramError::Memory(MemoryError::BudgetLimited))
    );
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
    let (a, s, c, p) = capture(8 * 1024 * 1024);
    c.cancel();
    assert_eq!(
        render_native("flowchart TD\nA-->B", &s, &c, &p, native_theme()).err(),
        Some(DiagramError::Cancelled)
    );
    assert_eq!(a.used.load(Ordering::SeqCst), 0);
}

#[test]
fn native_production_dispatches_additional_families_with_real_fonts_and_owned_artifacts() {
    for source in [
        "sequenceDiagram\nparticipant E as Editor\nparticipant P as Preview\nE->>P: Publish revision 1\nP-->>E: Frame ready\nNote over E,P: Keep the viewport anchor",
        "\u{feff}%% header comment\nsequenceDiagram; actor A; A->>A: hi",
        "stateDiagram-v2\n[*] --> Loading\nLoading --> Ready\nReady --> [*]",
        "\u{feff}stateDiagram\nA --> B: done",
        "\u{feff}pie showData\ntitle 预览资源\n\"文本\" : 62\n\"图片\" : 25\n\"公式\" : 13",
        "classDiagram\n    direction LR\n    class Document[\"Markdown 文档\"] {\n        +String source\n        +save()\n    }\n    class Preview {\n        +render()\n    }\n    Document \"1\" *-- \"1\" Preview : owns\n    Preview ..|> Renderer : implements\n    Renderer <|-- NativeRenderer",
        "erDiagram\n    direction LR\n    DOCUMENT {\n        string id PK\n        string title\n    }\n    SECTION {\n        string documentId FK\n        int ordinal\n    }\n    DOCUMENT ||--|{ SECTION : contains\n    SECTION }o..o| PARENT : references",
    ] {
        let (authority, scope, control, platform) = capture_with_work(64 * 1024 * 1024, 1_000_000);
        let artifact = render_native(source, &scope, &control, &platform, native_theme())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(artifact.width > 0.0 && artifact.height > 0.0 && artifact.path_commands > 10);
        let svg = std::str::from_utf8(artifact.as_bytes()).unwrap();
        assert!(!svg.contains("<text") && !svg.contains("<foreignObject"));
        drop(artifact);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0, "{source}");
    }
}

#[test]
fn native_accessibility_statements_leave_six_diagram_scenes_unchanged() {
    for (header, body) in [
        ("flowchart TB", "A-->B"),
        ("sequenceDiagram", "A->>B: message"),
        ("stateDiagram", "A --> B"),
        ("pie", "\"A\": 1\n\"B\": 2"),
        ("classDiagram", "A --> B"),
        ("erDiagram", "A ||--o{ B : contains"),
        ("flowchart TB", "A@{shape:rect,label:'path\\'}"),
        ("sequenceDiagram", "A->>B: say \\\"hello"),
        ("classDiagram", "class A {\n+String key: value}"),
        ("classDiagram", "A --> B : open {"),
    ] {
        let (authority, scope, control, platform) = capture_with_work(64 * 1024 * 1024, 1_000_000);
        let plain = format!("{header}\n{body}");
        let metadata = format!(
            "{header}\r\naccTitle: 图表; plain \" quote\r\naccDescr {{\r\n  第一行\r\n%% second line\r\n}}\r\n{body}"
        );
        let baseline = render_native(&plain, &scope, &control, &platform, native_theme())
            .unwrap_or_else(|error| panic!("{plain}: {error:?}"));
        let named = render_native(&metadata, &scope, &control, &platform, native_theme())
            .unwrap_or_else(|error| panic!("{metadata}: {error:?}"));
        assert_eq!(named.as_bytes(), baseline.as_bytes(), "{header}");
        let trailing = format!("{plain}\naccTitle: Accessible title");
        let trailing_scene = render_native(&trailing, &scope, &control, &platform, native_theme())
            .unwrap_or_else(|error| panic!("{trailing}: {error:?}"));
        assert_eq!(trailing_scene.as_bytes(), baseline.as_bytes(), "{plain}");
        drop((baseline, named, trailing_scene));
        assert_eq!(authority.used.load(Ordering::SeqCst), 0, "{header}");
    }
}

#[test]
fn development_new_backend_reuses_four_original_flowchart_sources() {
    // Functional Desktop development only. This is neither the retired G1 run
    // nor a claim that every upstream allocation/profile/UWP gate is closed.
    use merman::{
        Engine, MermaidConfig, ParseOptions, RenderOutput, RenderRequest, Renderer, SvgEnvironment,
        SvgRequest,
    };
    use merman_render::environment::{
        MeasurementProfileId, TextMeasurementPhase, TextMeasurementPolicy,
        TextMeasurementProfileIdentity, TextMeasurementSource,
    };
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for case in [
        "basic",
        "class_style",
        "stress_flowchart_long_labels_punctuation_unicode_006",
        "stress_flowchart_subgraph_title_unicode_and_entities_043",
    ] {
        let (authority, scope, control, platform) = capture(64 * 1024 * 1024);
        let region = markdown_memory::NativeRegion::try_new(&scope, 16384).unwrap();
        let control = control.with_memory_region(region.clone());
        let platform = Arc::new(platform);
        let identity = TextMeasurementProfileIdentity::new(
            MeasurementProfileId::new("notepads-markdown-directwrite").unwrap(),
            "native fonts",
        )
        .unwrap();
        let policy = TextMeasurementPolicy::host_display(
            identity,
            platform.clone(),
            TextMeasurementPhase::ALL,
        );
        let mut config = MermaidConfig::default();
        config.set_value("fontFamily", "Segoe UI".into());
        // Explicit native flowchart development profile, rather than the
        // upstream Neo/Redux default that actually requires SVG drop filters.
        // Original source files, PNGs and expected semantic files stay intact.
        for key in ["look", "flowchart.look"] {
            config.set_value(key, "classic".into());
        }
        for key in ["theme", "flowchart.theme"] {
            config.set_value(key, "default".into());
        }
        config.set_value("htmlLabels", false.into());
        config.set_value("flowchart.htmlLabels", false.into());
        config.set_value("themeVariables.fontFamily", "Segoe UI".into());
        let engine = Engine::new().with_site_config(config);
        let renderer = Renderer::new()
            .with_engine(engine)
            .with_parse_options(ParseOptions::strict());
        let source = std::fs::read_to_string(
            repository
                .join("tests/fixtures/mermaid-flowchart-v1")
                .join(case)
                .with_extension("mmd"),
        )
        .unwrap();
        let output = renderer
            .render(RenderRequest::svg(
                &source,
                control.clone(),
                SvgRequest {
                    environment: SvgEnvironment::deterministic()
                        .with_text_measurement_policy(policy),
                    ..Default::default()
                },
            ))
            .unwrap();
        let RenderOutput::Svg(Some(output)) = output else {
            panic!("expected real flowchart SVG");
        };
        assert!(platform.failure().is_none(), "{case}: platform failure");
        for entry in output.evidence().measurement().entries() {
            assert_eq!(
                entry.provenance().source,
                TextMeasurementSource::Host,
                "{case}: no fallback/profile approximation"
            );
        }
        let canonical = compile(
            output.svg(),
            &scope,
            &control,
            platform.as_ref(),
            CanonicalLimits::default(),
        );
        if let Err(error) = &canonical {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                test_output_directory().join(format!("mermaid-development-{case}-{stamp}.svg"));
            std::fs::write(&path, output.svg()).unwrap();
            panic!(
                "new backend {case} failed {error:?}; unchanged source and actual SVG preserved at {}",
                path.display()
            );
        }
        let canonical = canonical.unwrap();
        assert!(canonical.text_runs > 0 && canonical.path_commands > 0);
        export_successful_svg(
            &format!("mermaid-native-development-{case}.svg"),
            canonical.as_bytes(),
        );
        eprintln!(
            "development {case}: profile=classic/default+native fonts+htmlLabels=false, {}x{}, commands={}, textRuns={}, bytes={}",
            canonical.width,
            canonical.height,
            canonical.path_commands,
            canonical.text_runs,
            canonical.as_bytes().len()
        );
        drop(canonical);
        drop(output);
        drop(renderer);
        drop(platform);
        drop(control);
        drop(region);
        drop(scope);
        assert_eq!(
            authority.used.load(Ordering::SeqCst),
            0,
            "{case}: every owned block really released"
        );
    }
}
