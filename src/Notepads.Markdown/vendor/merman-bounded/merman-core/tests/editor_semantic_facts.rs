#![cfg(feature = "all-diagrams")]

use merman_core::time::CivilDate;
use merman_core::{
    EditorSemanticFacts, Engine, Error, MermaidConfig, SourceSpan, diagram_family_capabilities,
};
use merman_fixture_render_context::RenderContextCatalog;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn fixtures_root() -> PathBuf {
    workspace_root().join("fixtures")
}

fn fixture_render_contexts() -> &'static RenderContextCatalog {
    static CATALOG: OnceLock<RenderContextCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        RenderContextCatalog::load(fixtures_root()).expect("valid fixture render context catalog")
    })
}

fn fixture_site_config_for_path(path: &Path) -> Option<MermaidConfig> {
    fixture_render_contexts()
        .context_for_fixture(path)
        .unwrap_or_else(|error| {
            panic!(
                "invalid fixture render context lookup for {}: {error}",
                path.display()
            )
        })
        .map(|context| MermaidConfig::from_value(context.site_config_value()))
}

fn engine_for_fixture(base: &Engine, path: &Path) -> Engine {
    match fixture_site_config_for_path(path) {
        Some(site_config) => base.clone().with_site_config(site_config),
        None => base.clone(),
    }
}

fn list_formal_fixture_mmd_files(root: &Path) -> Vec<PathBuf> {
    // Generated reference SVGs and temporary/private trees are not authored source fixtures.
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with('_') || name.starts_with('.') || name == "upstream-svgs"
            })
        {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with('_') || name.starts_with('.') || name == "upstream-svgs"
                    })
                {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|extension| extension == "mmd") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[derive(Default)]
struct FormalFixtureEditorFactsAudit {
    total: usize,
    malformed_metadata: usize,
    unsupported: usize,
    supported: usize,
    available: usize,
    malformed_editor: usize,
    localized_drop_documents: usize,
}

impl FormalFixtureEditorFactsAudit {
    fn accounted(&self) -> usize {
        self.malformed_metadata + self.unsupported + self.available + self.malformed_editor
    }
}

#[test]
fn formal_fixture_corpus_keeps_supported_editor_facts_available() {
    let root = fixtures_root();
    let fixtures = list_formal_fixture_mmd_files(&root);
    assert!(
        !fixtures.is_empty(),
        "formal fixture corpus must not be empty"
    );

    let editor_capabilities = diagram_family_capabilities()
        .iter()
        .map(|capability| (capability.diagram_type, capability.has_editor_parser))
        .collect::<BTreeMap<_, _>>();
    let base = Engine::new()
        .with_fixed_today(Some(
            CivilDate::new(2026, 2, 15).expect("valid fixed fixture date"),
        ))
        .try_with_fixed_local_offset_minutes(0)
        .expect("UTC is a valid fixed offset");
    let mut audit = FormalFixtureEditorFactsAudit {
        total: fixtures.len(),
        ..Default::default()
    };
    let mut unavailable = Vec::new();
    let mut malformed_samples = Vec::new();
    let mut localized_drop_fixtures = Vec::new();
    let mut unsupported_types = BTreeMap::<String, usize>::new();

    for path in fixtures {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let relative = path.strip_prefix(&root).unwrap_or(&path);
        let engine = engine_for_fixture(&base, &path);
        let metadata = match engine.parse_metadata_sync(&source) {
            Ok(metadata) => metadata,
            Err(error) => {
                assert!(
                    is_malformed_fixture_error(&error),
                    "{} failed metadata extraction for a non-syntax reason: {error}",
                    relative.display()
                );
                audit.malformed_metadata += 1;
                record_sample(
                    &mut malformed_samples,
                    format!("{} (metadata): {error}", relative.display()),
                );
                continue;
            }
        };

        let has_editor_parser = editor_capabilities
            .get(metadata.diagram_type.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "{} detected uncataloged diagram type {}",
                    relative.display(),
                    metadata.diagram_type
                )
            });
        if !*has_editor_parser {
            audit.unsupported += 1;
            *unsupported_types
                .entry(metadata.diagram_type.clone())
                .or_default() += 1;
            continue;
        }
        audit.supported += 1;

        match engine.parse_editor_semantic_facts_with_type_sync(&metadata.diagram_type, &source) {
            Ok(Some(facts)) => {
                audit.available += 1;
                let localized_drop_diagnostics = facts
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        diagnostic
                            .message
                            .contains("editor fact span(s) that crossed a preprocessing edit")
                    })
                    .map(|diagnostic| diagnostic.message.as_str())
                    .collect::<Vec<_>>();
                if !localized_drop_diagnostics.is_empty() {
                    audit.localized_drop_documents += 1;
                    localized_drop_fixtures.push(format!(
                        "{} ({}): {}",
                        relative.display(),
                        metadata.diagram_type,
                        localized_drop_diagnostics.join("; ")
                    ));
                }
                assert_editor_fact_spans_belong_to_source(relative, &source, &facts);
            }
            Ok(None) => unavailable.push(format!(
                "{} ({})",
                relative.display(),
                metadata.diagram_type
            )),
            Err(error) => {
                assert!(
                    is_malformed_fixture_error(&error),
                    "{} ({}) failed editor parsing for a non-syntax reason: {error}",
                    relative.display(),
                    metadata.diagram_type
                );
                audit.malformed_editor += 1;
                record_sample(
                    &mut malformed_samples,
                    format!(
                        "{} ({} editor): {error}",
                        relative.display(),
                        metadata.diagram_type
                    ),
                );
            }
        }
    }

    assert_eq!(
        audit.accounted() + unavailable.len(),
        audit.total,
        "every formal fixture must receive exactly one honest outcome"
    );
    assert_eq!(
        audit.supported,
        audit.available + audit.malformed_editor + unavailable.len(),
        "every supported fixture must receive exactly one editor-parser outcome"
    );
    assert!(audit.available > 0, "the corpus must exercise editor facts");
    assert!(
        unavailable.is_empty(),
        "supported fixtures must not lose the whole editor-fact document because one span is unmappable:\n{}",
        unavailable.join("\n")
    );

    eprintln!(
        "formal fixture editor-fact audit: total={}, supported={}, available={}, localized_drops={}, malformed_editor={}, unsupported={} {:?}, malformed_metadata={}{}{}",
        audit.total,
        audit.supported,
        audit.available,
        audit.localized_drop_documents,
        audit.malformed_editor,
        audit.unsupported,
        unsupported_types,
        audit.malformed_metadata,
        if localized_drop_fixtures.is_empty() {
            String::new()
        } else {
            format!(
                "\nlocalized preprocessing drops retained document facts:\n{}",
                localized_drop_fixtures.join("\n")
            )
        },
        if malformed_samples.is_empty() {
            String::new()
        } else {
            format!("\nmalformed samples:\n{}", malformed_samples.join("\n"))
        }
    );
}

fn is_malformed_fixture_error(error: &Error) -> bool {
    matches!(
        error,
        Error::DiagramParse { .. }
            | Error::MalformedFrontMatter
            | Error::InvalidDirectiveJson { .. }
            | Error::InvalidFrontMatterYaml { .. }
    )
}

fn record_sample(samples: &mut Vec<String>, sample: String) {
    const SAMPLE_LIMIT: usize = 8;
    if samples.len() < SAMPLE_LIMIT {
        samples.push(sample);
    }
}

fn assert_editor_fact_spans_belong_to_source(
    fixture: &Path,
    source: &str,
    facts: &EditorSemanticFacts,
) {
    let fixture = fixture.display().to_string();
    for symbol in &facts.symbols {
        assert_source_span(&fixture, source, "symbol", symbol.span);
        assert_source_span(&fixture, source, "symbol selection", symbol.selection);
    }
    for diagnostic in &facts.diagnostics {
        if let Some(span) = diagnostic.span {
            assert_source_span(&fixture, source, "diagnostic", span);
        }
    }
    for expected in &facts.expected_syntax {
        assert_source_span(&fixture, source, "expected syntax", expected.span);
    }
}

fn assert_source_span(fixture: &str, source: &str, owner: &str, span: SourceSpan) {
    assert!(
        span.start <= span.end,
        "{fixture}: {owner} span is reversed: {span:?}"
    );
    assert!(
        span.end <= source.len(),
        "{fixture}: {owner} span is out of bounds: {span:?}"
    );
    assert!(
        source.is_char_boundary(span.start) && source.is_char_boundary(span.end),
        "{fixture}: {owner} span splits a UTF-8 code point: {span:?}"
    );
}
