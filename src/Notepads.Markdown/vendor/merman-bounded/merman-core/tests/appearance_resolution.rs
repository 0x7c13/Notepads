use merman_core::{Engine, MermaidConfig};
use serde_json::json;

#[test]
fn diagram_appearance_uses_raw_user_variables_without_leaking_between_operations() {
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "theme": "dark",
        "themeVariables": { "primaryColor": "#123456" },
        "flowchart": { "theme": "forest", "look": "handDrawn" }
    })));
    let flow = engine.parse_metadata_sync("flowchart TD\nA-->B").unwrap();
    assert_eq!(flow.effective_config.get_str("theme"), Some("forest"));
    assert_eq!(flow.effective_config.get_str("look"), Some("handDrawn"));
    assert_eq!(
        flow.effective_config.get_str("themeVariables.primaryColor"),
        Some("#123456")
    );

    let source = "---\nconfig:\n  theme: base\n  themeVariables:\n    primaryColor: '#654321'\n---\nflowchart TD\nA-->B";
    let flow = engine.parse_metadata_sync(source).unwrap();
    assert_eq!(flow.effective_config.get_str("theme"), Some("base"));
    assert_eq!(
        flow.effective_config.get_str("themeVariables.primaryColor"),
        Some("#654321")
    );
    assert_eq!(
        flow.effective_config.get_str("themeVariables.actorBkg"),
        Some("#654321")
    );

    let sequence = engine
        .parse_metadata_sync("sequenceDiagram\nA->>B: hello")
        .unwrap();
    assert_eq!(sequence.effective_config.get_str("theme"), Some("dark"));
    assert_eq!(
        sequence
            .effective_config
            .get_str("themeVariables.primaryColor"),
        Some("#123456")
    );
}

#[test]
fn appearance_resolution_cannot_restore_secure_source_keys() {
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "theme": "dark", "secure": ["theme"],
    })));
    let source =
        "---\nconfig:\n  theme: base\n  flowchart:\n    theme: forest\n---\nflowchart TD\nA-->B";
    let meta = engine.parse_metadata_sync(source).unwrap();
    assert_eq!(meta.effective_config.get_str("theme"), Some("dark"));
    assert_eq!(meta.config.get_str("theme"), Some("base"));
}

#[test]
fn null_theme_sentinel_retains_initialized_variables_without_rederiving_them() {
    let engine =
        Engine::new().with_site_config(MermaidConfig::from_value(json!({ "theme": "dark" })));
    let initialized = engine
        .parse_metadata_sync("flowchart TD\nA-->B")
        .unwrap()
        .effective_config;
    let source = "---\nconfig:\n  theme: 'null'\n  themeVariables:\n    primaryColor: '#654321'\n---\nflowchart TD\nA-->B";
    let effective = engine.parse_metadata_sync(source).unwrap().effective_config;
    assert_eq!(effective.get_str("theme"), Some("null"));
    assert_eq!(
        effective.get_str("themeVariables.primaryColor"),
        Some("#654321")
    );
    assert_eq!(
        effective.get_str("themeVariables.actorBkg"),
        initialized.get_str("themeVariables.actorBkg")
    );
}

#[test]
fn exact_site_config_discards_prior_appearance_provenance() {
    let engine = Engine::new()
        .with_site_config(MermaidConfig::from_value(
            json!({ "flowchart": { "theme": "dark" } }),
        ))
        .with_exact_site_config(Some(MermaidConfig::from_value(
            json!({ "theme": "forest" }),
        )));
    assert_eq!(
        engine
            .parse_metadata_sync("flowchart TD\nA-->B")
            .unwrap()
            .effective_config
            .get_str("theme"),
        Some("forest")
    );
    let reset = engine.with_exact_site_config(None);
    assert_eq!(
        reset
            .parse_metadata_sync("flowchart TD\nA-->B")
            .unwrap()
            .effective_config,
        Engine::new()
            .parse_metadata_sync("flowchart TD\nA-->B")
            .unwrap()
            .effective_config
    );
}

#[test]
fn invalid_initialization_theme_normalizes_but_invalid_source_theme_falls_through() {
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "theme": "bogus", "flowchart": { "theme": "constructor" }
    })));
    assert_eq!(
        engine
            .parse_metadata_sync("flowchart TD\nA-->B")
            .unwrap()
            .effective_config
            .get_str("theme"),
        Some("default")
    );

    let engine =
        Engine::new().with_site_config(MermaidConfig::from_value(json!({ "theme": "dark" })));
    for value in ["bogus", "null"] {
        let source = format!("---\nconfig:\n  theme: {value}\n---\nflowchart TD\nA-->B");
        assert_eq!(
            engine
                .parse_metadata_sync(&source)
                .unwrap()
                .effective_config
                .get_str("theme"),
            Some("dark")
        );
    }
}

#[test]
fn syntax_aliases_detect_independently_of_obsolete_renderer_selectors() {
    for renderer in ["elk", "dagre-wrapper", "dagre-d3", "bogus"] {
        let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
            "layout": "dagre", "flowchart": { "defaultRenderer": renderer },
            "class": { "defaultRenderer": renderer }, "state": { "defaultRenderer": renderer },
        })));
        for (source, diagram_type) in [
            ("graph TD\nA-->B", "flowchart-v2"),
            ("flowchart TD\nA-->B", "flowchart-v2"),
            ("classDiagram\nclass A", "classDiagram"),
            ("classDiagram-v2\nclass A", "classDiagram"),
            ("stateDiagram\n[*] --> A", "stateDiagram"),
            ("stateDiagram-v2\n[*] --> A", "stateDiagram"),
        ] {
            let meta = engine.parse_metadata_sync(source).unwrap();
            assert_eq!(meta.diagram_type, diagram_type);
            assert_eq!(meta.effective_config.get_str("layout"), Some("dagre"));
        }
        let meta = engine
            .parse_metadata_sync("flowchart-elk TD\nA-->B")
            .unwrap();
        assert_eq!(meta.diagram_type, "flowchart-elk");
        assert_eq!(meta.effective_config.get_str("layout"), Some("elk"));
    }
}
