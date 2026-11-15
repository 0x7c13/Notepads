use merman_core::{DiagramParseOutcome, Engine};
use serde_json::Value;

fn diagnostics(source: &str) -> Value {
    let snapshot = Engine::new()
        .parse_diagram_snapshot_with_type_sync("agentflow", source)
        .expect("snapshot parse")
        .expect("snapshot");
    let DiagramParseOutcome::Parsed { model, .. } = snapshot.outcome() else {
        panic!("Agentflow source should parse");
    };
    model["diagnostics"].clone()
}

#[test]
fn agentflow_positions_match_mermaid12_utf16_and_zero_indices_after_frontmatter() {
    let source = "---\r\ntitle: \"中文😀\"\r\n---\r\nagentflow-beta\r\n%% 中文😀 comment\r\na[\"中文😀\"]; b@{ shape: cloud }\r\n";
    let diagnostics = diagnostics(source);
    assert_eq!(diagnostics[0]["position"]["startLine"], 6);
    assert_eq!(diagnostics[0]["position"]["startColumn"], 11);
    assert_eq!(diagnostics[0]["position"]["endLine"], 6);
    assert_eq!(diagnostics[0]["position"]["endColumn"], 28);
    assert_eq!(diagnostics[0]["position"]["startIndex"], 0);
    assert_eq!(diagnostics[0]["position"]["endIndex"], 0);
}

#[test]
fn agentflow_positions_skip_bom_and_count_unicode_as_utf16() {
    let source = "\u{feff}agentflow-beta\r\na[\"中文😀\"]; b@{ shape: cloud }\r\n";
    let diagnostics = diagnostics(source);
    assert_eq!(diagnostics[0]["position"]["startLine"], 2);
    assert_eq!(diagnostics[0]["position"]["startColumn"], 11);
    assert_eq!(diagnostics[0]["position"]["endColumn"], 28);
    assert_eq!(diagnostics[0]["position"]["startIndex"], 0);
    assert_eq!(diagnostics[0]["position"]["endIndex"], 0);
}
