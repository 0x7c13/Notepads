use merman_core::{Engine, MermaidConfig};
use serde_json::{Value, json};

fn sources(config: &Value) -> [String; 2] {
    [
        format!("%%{{init: {config}}}%%\nsequenceDiagram\nAlice->>Bob: hi\n"),
        format!("---\nconfig: {config}\n---\nsequenceDiagram\nAlice->>Bob: hi\n"),
    ]
}

#[test]
fn source_presentation_values_are_allowed_by_default_in_both_syntaxes() {
    let config = json!({
        "theme": "base",
        "fontFamily": "Courier New, monospace",
        "themeVariables": {
            "primaryColor": "#181818",
            "fontFamily": "Inter, sans-serif",
            "fontSize": "18px",
            "radar": { "axisColor": "#123456" }
        },
        "sequence": { "actorFontFamily": "Noto Sans", "actorFontWeight": "600" }
    });
    for source in sources(&config) {
        let metadata = Engine::new().parse_metadata_sync(&source).unwrap();
        let effective = metadata.effective_config;
        for (key, expected) in [
            ("fontFamily", "Courier New, monospace"),
            ("themeVariables.fontFamily", "Inter, sans-serif"),
            ("themeVariables.fontSize", "18px"),
            ("themeVariables.actorBkg", "#181818"),
            ("themeVariables.radar.axisColor", "#123456"),
            ("sequence.actorFontWeight", "600"),
        ] {
            assert_eq!(effective.get_str(key), Some(expected), "{key}: {source}");
        }
        assert_eq!(
            effective.get_str("sequence.actorFontFamily"),
            Some("Noto Sans")
        );
    }
}

#[test]
fn unsafe_source_values_preserve_trusted_site_values_in_both_syntaxes() {
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "fontFamily": "Host Font",
        "themeVariables": { "actorBkg": "#112233", "radar": { "axisColor": "#445566" } },
        "sequence": { "actorFontWeight": "500" }
    })));
    for attack in [
        "red;stroke:blue",
        "x;a{b} :not(&){background:green !important} c{d}",
        "</style><script>alert(1)</script>",
        "url(https://example.com/x)",
        r"u\72l('https://example.com/x')",
        "image-set('https://example.com/x' 1x)",
        "var(--host-resource)",
        "red#59;stroke:blue",
        "'font#34;'",
        "'font&#x22;'",
        "'fontﬂ°quot¶ß'",
        "calc((1px)",
        "'unterminated",
    ] {
        let config = json!({
            "fontFamily": attack,
            "themeVariables": { "actorBkg": attack, "radar": { "axisColor": attack } },
            "sequence": { "actorFontWeight": attack, "actorFontFamily": [attack] }
        });
        for source in sources(&config) {
            let effective = engine
                .parse_metadata_sync(&source)
                .unwrap()
                .effective_config;
            for (key, expected) in [
                ("fontFamily", "Host Font"),
                ("themeVariables.fontFamily", "Host Font"),
                ("themeVariables.actorBkg", "#112233"),
                ("themeVariables.radar.axisColor", "#445566"),
                ("sequence.actorFontWeight", "500"),
            ] {
                assert_eq!(effective.get_str(key), Some(expected), "{key}: {source}");
            }
            assert!(effective.as_value()["sequence"]["actorFontFamily"][0].is_null());
        }
    }
}

#[test]
fn source_cannot_unlock_host_policy_or_supply_stylesheets() {
    let config = json!({
        "secure": [], "securityLevel": "loose", "maxEdges": 999999,
        "themeCSS": "body { color: red; }",
        "fontFamily": "Source Font",
        "themeVariables": { "lineColor": "#abcdef" }
    });
    for source in sources(&config) {
        let effective = Engine::new()
            .parse_metadata_sync(&source)
            .unwrap()
            .effective_config;
        assert_eq!(effective.get_str("securityLevel"), Some("strict"));
        assert_ne!(effective.as_value()["maxEdges"], 999999);
        assert!(effective.get_str("themeCSS").is_none());

        let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
            "secure": ["secure", "securityLevel", "themeVariables", "fontFamily", "themeCSS"],
            "fontFamily": "Host Font", "themeVariables": { "lineColor": "#123456" },
            "themeCSS": ".node { opacity: 0.8; }"
        })));
        let locked = engine
            .parse_metadata_sync(&source)
            .unwrap()
            .effective_config;
        assert_eq!(locked.get_str("themeVariables.lineColor"), Some("#123456"));
        assert_eq!(
            locked.get_str("themeVariables.fontFamily"),
            Some("Host Font")
        );
        assert_eq!(locked.get_str("fontFamily"), Some("Host Font"));
        assert_eq!(locked.get_str("themeCSS"), Some(".node { opacity: 0.8; }"));
    }
}
