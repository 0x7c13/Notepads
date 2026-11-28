#[cfg(feature = "all-diagrams")]
use crate::diagrams::xychart::{XyChartAxisRenderModel, XyChartPlotType};
use crate::*;
use futures::executor::block_on;
use serde_json::{Map, Value, json};
use std::fmt::Write;

#[test]
fn parse_graph_defaults_to_flowchart_v2() {
    let engine = Engine::new();
    let res = block_on(engine.parse_metadata("graph TD;A-->B;")).unwrap();
    assert_eq!(res.diagram_type, "flowchart-v2");
    assert_eq!(res.config.as_value(), &json!({}));
    assert_eq!(
        res.effective_config.get_str("themeVariables.mainBkg"),
        Some("#ffffff")
    );
}

#[test]
fn parse_indented_headers_across_common_diagrams() {
    let engine = Engine::new();

    let cases = [
        ("     flowchart TB\n     A-->B\n", "flowchart-v2"),
        ("     sequenceDiagram\n     Alice->>Bob: hi\n", "sequence"),
        ("     stateDiagram-v2\n     [*] --> A\n", "stateDiagram"),
        (
            "     requirementDiagram\n\n     requirement test_req {\n       id: test_id\n       text: the test text.\n       risk: high\n       verifymethod: analysis\n     }\n",
            "requirement",
        ),
        ("     packet\n     0-10: \"test\"\n", "packet"),
        (
            "     timeline\n     section abc-123\n     task1\n",
            "timeline",
        ),
        (
            "     gantt\n     section Demo\n     task1: 03-01,1d\n",
            "gantt",
        ),
        (
            "     classDiagram\n     class C1[\"Class 1 with text label\"]\n",
            "classDiagram",
        ),
        ("     erDiagram\n     PERSON ||--o{ ORDER : places\n", "er"),
        (
            "     xychart horizontal\n     title \"Basic xychart\"\n",
            "xychart",
        ),
        ("     C4Context\n     title System Context\n", "c4"),
    ];

    for (text, expected_type) in cases {
        let meta = engine.parse_metadata_sync(text).unwrap();
        assert_eq!(meta.diagram_type, expected_type, "input was: {text:?}");
    }

    for (text, expected_type) in [
        ("     mindmap\n       root\n", "mindmap"),
        (
            "     architecture-beta\n       service db\n",
            "architecture",
        ),
    ] {
        let meta = engine.parse_metadata_sync(text).unwrap();
        assert_eq!(meta.diagram_type, expected_type, "input was: {text:?}");
    }
}

#[test]
fn parse_merges_frontmatter_and_directive_config() {
    let engine = Engine::new();
    let text = r#"---
config:
  theme: forest
  flowchart:
    htmlLabels: true
---
%%{init: { 'theme': 'base' } }%%
graph TD;A-->B;"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(
        res.config.as_value(),
        &json!({
            "theme": "base",
            "flowchart": {
                "htmlLabels": true
            }
        })
    );
}

#[test]
fn parse_frontmatter_and_directives_deep_merge_nested_config_like_upstream() {
    let engine = Engine::new();
    let text = r#"---
config:
  flowchart:
    nodeSpacing: 50
    rankSpacing: 100
  theme: default
---
%%{init: {"flowchart": {"nodeSpacing": 75}, "fontSize": 12}}%%
%%{init: {"flowchart": {"curve": "basis", "nodeSpacing": 100}, "mindmap": {"padding": 15}}}%%
graph TD;A-->B;"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(
        res.config.as_value(),
        &json!({
            "flowchart": {
                "curve": "basis",
                "nodeSpacing": 100,
                "rankSpacing": 100
            },
            "fontSize": 12,
            "mindmap": {
                "padding": 15
            },
            "theme": "default"
        })
    );
}

#[test]
fn parse_init_directives_deep_merge_in_source_order_like_upstream() {
    let engine = Engine::new();
    let text = r#"%%{init: {"flowchart": {"nodeSpacing": 50, "rankSpacing": 100}, "theme": "default"}}%%
%%{init: {"flowchart": {"nodeSpacing": 75}, "fontSize": 12}}%%
%%{init: {"flowchart": {"curve": "basis", "nodeSpacing": 100}, "mindmap": {"padding": 15}}}%%
graph TD;A-->B;"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(
        res.config.as_value(),
        &json!({
            "flowchart": {
                "curve": "basis",
                "nodeSpacing": 100,
                "rankSpacing": 100
            },
            "fontSize": 12,
            "mindmap": {
                "padding": 15
            },
            "theme": "default"
        })
    );
}

#[test]
fn parse_theme_variables_reach_retained_and_effective_config() {
    let engine = Engine::new();
    let text = r##"%%{init: {"theme": "forest", "themeVariables": {"primaryColor": "#123456"}}}%%
graph TD;A-->B;"##;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(res.config.get_str("theme"), Some("forest"));
    assert_eq!(
        res.config.get_str("themeVariables.primaryColor"),
        Some("#123456")
    );
    assert_eq!(res.config.get_str("themeVariables.mainBkg"), None);
    assert_eq!(
        res.effective_config.get_str("themeVariables.primaryColor"),
        Some("#123456")
    );
    assert_eq!(
        res.effective_config.get_str("themeVariables.mainBkg"),
        Some("#123456")
    );
    assert_eq!(
        res.effective_config.get_str("themeVariables.fontFamily"),
        Some("\"trebuchet ms\", verdana, arial, sans-serif")
    );
}

#[test]
fn init_directive_theme_variables_use_source_value_admission() {
    let engine = Engine::new();
    let text = r##"%%{init: {"themeVariables": {"primaryColor": "url(javascript:alert(1))", "secondaryColor": "rgb(1, 2, 3)"}}}%%
graph TD;A-->B;"##;

    let res = block_on(engine.parse_metadata(text)).expect("directive should parse");

    assert_eq!(
        res.config.get_str("themeVariables.primaryColor"),
        Some("url(javascript:alert(1))")
    );
    assert_eq!(
        res.effective_config.get_str("themeVariables.primaryColor"),
        Some("#cccccc")
    );
    assert_eq!(
        res.effective_config
            .get_str("themeVariables.secondaryColor"),
        Some("rgb(1, 2, 3)")
    );
    assert_eq!(
        res.config.get_str("themeVariables.secondaryColor"),
        Some("rgb(1, 2, 3)")
    );
}

#[test]
fn upstream_site_secure_policy_preserves_source_theme_variables() {
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "secure": ["secure", "securityLevel", "startOnLoad", "maxTextSize", "suppressErrorRendering", "maxEdges"]
    })));
    let text = r##"%%{init: {"theme": "forest", "themeVariables": {"primaryColor": "#123456"}}}%%
graph TD;A-->B;"##;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(
        res.effective_config.get_str("themeVariables.primaryColor"),
        Some("#123456")
    );
    assert_eq!(
        res.effective_config.get_str("themeVariables.mainBkg"),
        Some("#123456")
    );
}

#[test]
fn parse_maps_top_level_frontmatter_diagram_config() {
    let engine = Engine::new();
    let text = r#"---
title: Frontmatter Example
displayMode: compact
config:
  theme: forest
gantt:
  useWidth: 400
  topAxis: true
  numberSectionStyles: 2
swimlane:
  ignoreCrossLaneEdges: false
cynefin:
  width: 900
railroad:
  showMarkers: false
treeView:
  showIcons: false
eventmodeling:
  rowHeight: 36
treemap:
  padding: 12
unknownDiagram:
  ignored: true
---
gantt
    section Waffle
        Iron  : 1982, 3y
        House : 1986, 3y
"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(res.diagram_type, "gantt");
    assert_eq!(
        res.config.as_value(),
        &json!({
            "theme": "forest",
            "gantt": {
                "displayMode": "compact",
                "useWidth": 400,
                "topAxis": true,
                "numberSectionStyles": 2
            },
            "swimlane": {
                "ignoreCrossLaneEdges": false
            },
            "cynefin": {
                "width": 900
            },
            "railroad": {
                "showMarkers": false
            },
            "treeView": {
                "showIcons": false
            },
            "eventmodeling": {
                "rowHeight": 36
            },
            "treemap": {
                "padding": 12
            }
        })
    );
}

#[test]
fn parse_frontmatter_config_takes_priority_over_diagram_compat() {
    let engine = Engine::new();
    let text = r#"---
config:
  look: neo
  layout: elk
  gantt:
    useWidth: 640
gantt:
  useWidth: 400
  rightPadding: 10
classDiagram:
  htmlLabels: false
---
gantt
    section Waffle
        Iron  : 1982, 3y
"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(res.config.get_str("look"), Some("neo"));
    assert_eq!(res.config.get_str("layout"), Some("elk"));
    assert_eq!(res.config.as_value()["gantt"]["useWidth"], json!(640));
    assert_eq!(res.config.as_value()["gantt"]["rightPadding"], json!(10));
    assert_eq!(res.config.get_bool("class.htmlLabels"), Some(false));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_diagram_with_type_sync_matches_auto_detect_for_flowchart_v2() {
    let engine = Engine::new();
    let input = "flowchart TD; A[Start]-->B[End];";

    let auto = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(auto.meta.diagram_type, "flowchart-v2");

    let known = engine
        .parse_diagram_with_type_sync("flowchart-v2", input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(known.meta.diagram_type, "flowchart-v2");
    assert_eq!(known.model, auto.model);
}

#[test]
fn parse_metadata_with_type_sync_moves_init_config_without_detection() {
    let engine = Engine::new();
    let input = "%%{init: {\"config\": {\"htmlLabels\": true}}}%%\nflowchart TD; A-->B;";

    let meta = engine
        .parse_metadata_with_type_sync("flowchart-v2", input)
        .unwrap();
    // Mermaid special-case: `flowchart-v2` config is stored under `flowchart`.
    assert_eq!(meta.config.get_bool("flowchart.htmlLabels"), Some(true));
}

#[test]
fn parse_metadata_with_type_sync_moves_init_config_through_diagram_aliases() {
    let engine = Engine::new();

    for (diagram_type, source, config_key) in [
        ("classDiagram", "classDiagram\nclass A\n", "class"),
        ("stateDiagram", "stateDiagram-v2\n[*] --> A\n", "state"),
        ("erDiagram", "erDiagram\nA ||--|| B : owns\n", "er"),
        (
            "xychart",
            "xychart-beta\nx-axis [a]\ny-axis \"Y\" 0 --> 1\nbar [1]\n",
            "xyChart",
        ),
        ("flowchart-elk", "flowchart-elk TD\nA-->B\n", "flowchart"),
    ] {
        let input = format!("%%{{init: {{\"config\": {{\"useMaxWidth\": false}}}}}}%%\n{source}");

        let meta = engine
            .parse_metadata_with_type_sync(diagram_type, &input)
            .unwrap();
        assert_eq!(
            meta.config
                .as_value()
                .pointer(&format!("/{config_key}/useMaxWidth")),
            Some(&json!(false)),
            "config key for {diagram_type}"
        );
        if diagram_type != config_key {
            assert!(
                meta.config.as_value().get(diagram_type).is_none(),
                "raw diagram key should not survive for {diagram_type}"
            );
        }
    }
}

#[test]
fn parse_metadata_with_type_sync_preserves_flowchart_elk_layout_side_effect() {
    let engine = Engine::new();
    let input = "flowchart-elk TD\nA-->B;";

    let meta = engine
        .parse_metadata_with_type_sync("flowchart-elk", input)
        .unwrap();
    assert_eq!(meta.effective_config.get_str("layout"), Some("elk"));
}

#[test]
fn parse_sanitizes_frontmatter_title_like_mermaid_common_db() {
    let engine = Engine::new();
    let text = r#"---
title: "Flowchart v2 arrows: graph direction \"<\""
---
graph <
A-->B
"#;

    let meta = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(
        meta.title,
        Some(r#"Flowchart v2 arrows: graph direction "&lt;""#.to_string())
    );
}

#[test]
fn parse_stringifies_truthy_frontmatter_title_like_upstream() {
    let engine = Engine::new();
    let meta = engine
        .parse_metadata_sync(
            "---
title: true
---
sequenceDiagram
Alice->Bob: Hi
",
        )
        .unwrap();

    assert_eq!(meta.title, Some("true".to_string()));
}

#[test]
fn parse_indented_frontmatter_like_upstream() {
    let engine = Engine::new();
    let meta = engine
        .parse_metadata_sync(
            "   ---
   title: Flow
   config:
     flowchart:
       htmlLabels: true
   ---
   graph TD
   A-->B
",
        )
        .unwrap();

    assert_eq!(meta.diagram_type, "flowchart-v2");
    assert_eq!(meta.title, Some("Flow".to_string()));
    assert_eq!(meta.config.get_bool("flowchart.htmlLabels"), Some(true));
}

#[test]
fn parse_merges_init_directive_numeric_values_like_upstream() {
    let engine = Engine::new();
    let text = r#"%%{init: { 'logLevel': 0 } }%%
sequenceDiagram
Alice->Bob: Hi
"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(res.diagram_type, "sequence");
    assert_eq!(res.config.as_value(), &json!({ "logLevel": 0 }));
}

#[test]
fn parse_init_font_family_reaches_retained_and_effective_config() {
    let engine = Engine::new();
    let text = r#"%%{init: { "fontFamily": "Courier" } }%%
graph TD;A-->B;
"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(res.config.get_str("fontFamily"), Some("Courier"));
    assert_eq!(
        res.config.get_str("themeVariables.fontFamily"),
        Some("Courier")
    );
    assert_eq!(res.effective_config.get_str("fontFamily"), Some("Courier"));
    assert_eq!(
        res.effective_config.get_str("themeVariables.fontFamily"),
        Some("Courier")
    );
}

#[test]
fn parse_init_theme_font_family_takes_precedence_over_legacy_font() {
    let engine = Engine::new();
    let text = r#"%%{init: { "fontFamily": "Courier", "themeVariables": { "fontFamily": "Inter" } } }%%
graph TD;A-->B;
"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(res.config.get_str("fontFamily"), Some("Courier"));
    assert_eq!(
        res.config.get_str("themeVariables.fontFamily"),
        Some("Inter")
    );
    assert_eq!(res.effective_config.get_str("fontFamily"), Some("Courier"));
    assert_eq!(
        res.effective_config.get_str("themeVariables.fontFamily"),
        Some("Inter")
    );
}

#[test]
fn upstream_site_secure_policy_preserves_source_fonts() {
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "secure": ["secure", "securityLevel", "startOnLoad", "maxTextSize", "suppressErrorRendering", "maxEdges"]
    })));
    let text = r#"%%{init: { "fontFamily": "Courier", "themeVariables": { "fontFamily": "Inter" } } }%%
graph TD;A-->B;
"#;

    let res = block_on(engine.parse_metadata(text)).unwrap();
    assert_eq!(res.effective_config.get_str("fontFamily"), Some("Courier"));
    assert_eq!(
        res.effective_config.get_str("themeVariables.fontFamily"),
        Some("Inter")
    );
}

#[test]
fn parse_architecture_exposes_11_16_fcose_config_defaults_and_overrides() {
    let engine = Engine::new();
    let default =
        block_on(engine.parse_metadata("architecture-beta\n  service a(server)[A]\n")).unwrap();

    let arch = &default.effective_config.as_value()["architecture"];
    assert_eq!(arch["randomize"], json!(false));
    assert_eq!(arch["nodeSeparation"], json!(75));
    assert_eq!(arch["idealEdgeLengthMultiplier"], json!(1.5));
    assert_eq!(arch["edgeElasticity"], json!(0.45));
    assert_eq!(arch["numIter"], json!(2500));
    assert_eq!(arch["seed"], json!(1));

    let configured = block_on(engine.parse_metadata(
        r#"%%{init: {"architecture": {"randomize": true, "nodeSeparation": 120, "idealEdgeLengthMultiplier": 2, "edgeElasticity": 0.6, "numIter": 5000, "seed": 7}}}%%
architecture-beta
  service a(server)[A]
"#,
    ))
    .unwrap();

    let arch = &configured.effective_config.as_value()["architecture"];
    assert_eq!(arch["randomize"], json!(true));
    assert_eq!(arch["nodeSeparation"], json!(120));
    assert_eq!(arch["idealEdgeLengthMultiplier"], json!(2));
    assert_eq!(arch["edgeElasticity"], json!(0.6));
    assert_eq!(arch["numIter"], json!(5000));
    assert_eq!(arch["seed"], json!(7));
}

#[test]
fn parse_metadata_exposes_admitted_11_16_family_config_defaults() {
    let engine = Engine::new();
    let meta = block_on(engine.parse_metadata("flowchart TD\nA-->B")).unwrap();
    let config = meta.effective_config.as_value();

    let eventmodeling = &config["eventmodeling"];
    assert_eq!(eventmodeling["padding"], json!(30));
    assert_eq!(eventmodeling["rowHeight"], json!(32));

    let tree_view = &config["treeView"];
    assert_eq!(tree_view["rowIndent"], json!(10));
    assert_eq!(tree_view["paddingX"], json!(5));
    assert_eq!(tree_view["paddingY"], json!(5));
    assert_eq!(tree_view["lineThickness"], json!(1));

    let ishikawa = &config["ishikawa"];
    assert_eq!(ishikawa["diagramPadding"], json!(20));
    assert_eq!(ishikawa["useMaxWidth"], json!(true));

    let venn = &config["venn"];
    assert_eq!(venn["width"], json!(800));
    assert_eq!(venn["height"], json!(450));
    assert_eq!(venn["padding"], json!(8));
    assert_eq!(venn["useDebugLayout"], json!(false));

    let cynefin = &config["cynefin"];
    assert_eq!(cynefin["width"], json!(800));
    assert_eq!(cynefin["height"], json!(600));
    assert_eq!(cynefin["padding"], json!(40));
    assert_eq!(cynefin["showDomainDescriptions"], json!(true));

    let railroad = &config["railroad"];
    for theme_derived_key in [
        "fontFamily",
        "fontSize",
        "terminalFill",
        "terminalStroke",
        "terminalTextColor",
        "nonTerminalFill",
        "nonTerminalStroke",
        "nonTerminalTextColor",
        "lineColor",
        "markerFill",
        "commentFill",
        "commentStroke",
        "commentTextColor",
        "specialFill",
        "specialStroke",
        "ruleNameColor",
    ] {
        assert!(
            railroad.get(theme_derived_key).is_none(),
            "railroad.{theme_derived_key} should derive from active themeVariables"
        );
    }
    assert!(railroad.get("showMarkers").is_none());

    let swimlane = &config["swimlane"];
    assert_eq!(swimlane["lineHops"], json!("arc"));
    assert_eq!(swimlane["ignoreCrossLaneEdges"], json!(true));
    assert_eq!(swimlane["optimizeRanksByCrossings"], json!(true));

    assert!(
        config.get("wardley-beta").is_none(),
        "Mermaid 11.16 keeps Wardley defaults in its renderer rather than defaultConfig"
    );
}

#[test]
fn site_config_deep_merge_handles_deep_public_config_with_small_stack() {
    const DEPTH: usize = 1_024;
    let site_config = MermaidConfig::from_value(deep_config_value(
        "sequence",
        DEPTH,
        Value::String("#112233".to_string()),
    ));
    let engine = Engine::new();

    let handle = std::thread::Builder::new()
        .name("merman-core-deep-site-config".to_string())
        .stack_size(128 * 1024)
        .spawn(move || {
            let engine = engine.with_site_config(site_config);
            let meta = engine
                .parse_metadata_sync("sequenceDiagram\nAlice->Bob: Hi")
                .expect("parse succeeds");
            assert_eq!(
                deep_config_leaf(meta.effective_config.as_value(), "sequence", DEPTH)
                    .and_then(Value::as_str),
                Some("#112233")
            );
        })
        .expect("spawn deep site config test");
    handle
        .join()
        .expect("deep site config merge should finish without stack overflow");
}

#[test]
fn site_config_secure_keys_protect_effective_config_from_diagram_config() {
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "fontFamily": "site-font",
        "fontSize": 16,
        "securityLevel": "strict",
        "themeCSS": ".node rect { fill: #123456; }",
        "secure": ["secure", "fontFamily", "fontSize", "securityLevel", "themeCSS", "themeVariables", "customLockedKey"]
    })));
    let text = r#"%%{init: {"fontFamily": "diagram-font", "fontSize": 99, "securityLevel": "loose", "themeCSS": ".node rect { fill: red; }", "themeVariables": {"fontFamily": "diagram-theme-font"}}}%%
flowchart TD
    A --> B
"#;

    let meta = engine.parse_metadata_sync(text).expect("parse succeeds");
    assert_eq!(meta.config.get_str("fontFamily"), Some("diagram-font"));
    assert_eq!(meta.config.as_value()["fontSize"], json!(99));
    assert_eq!(meta.config.get_str("securityLevel"), Some("loose"));
    assert_eq!(
        meta.config.get_str("themeCSS"),
        Some(".node rect { fill: red; }")
    );
    assert_eq!(
        meta.effective_config.get_str("fontFamily"),
        Some("site-font")
    );
    assert_eq!(
        meta.effective_config.get_str("themeCSS"),
        Some(".node rect { fill: #123456; }")
    );
    assert_eq!(
        meta.effective_config.get_str("themeVariables.fontFamily"),
        Some("site-font")
    );
    assert_eq!(meta.effective_config.as_value()["fontSize"], json!(16));
    assert_eq!(
        meta.effective_config.get_str("securityLevel"),
        Some("strict")
    );
}

#[test]
fn default_secure_keys_protect_effective_config_from_diagram_config() {
    let engine = Engine::new();
    let text = r#"%%{init: {"theme": "dark", "securityLevel": "loose", "fontFamily": "x;a{b} :not(&){background:green !important} c{d}", "altFontFamily": "Injected", "themeCSS": ".node rect { fill: red; }", "themeVariables": {"fontFamily": "Injected Theme"}}}%%
flowchart TD
    A --> B
"#;

    let meta = engine.parse_metadata_sync(text).expect("parse succeeds");
    assert_eq!(meta.config.get_str("theme"), Some("dark"));
    assert_eq!(meta.config.get_str("securityLevel"), Some("loose"));
    assert_eq!(
        meta.config.get_str("fontFamily"),
        Some("x;a{b} :not(&){background:green !important} c{d}")
    );
    assert_eq!(meta.effective_config.get_str("theme"), Some("dark"));
    assert_eq!(
        meta.effective_config.get_str("securityLevel"),
        Some("strict")
    );
    assert_eq!(
        meta.effective_config.get_str("fontFamily"),
        Some("\"trebuchet ms\", verdana, arial, sans-serif;")
    );
    assert!(meta.effective_config.get_str("altFontFamily").is_none());
    assert!(meta.effective_config.get_str("themeCSS").is_none());
    assert_eq!(
        meta.effective_config.get_str("themeVariables.fontFamily"),
        Some("Injected Theme")
    );
}

#[test]
fn directive_sanitization_uses_generated_config_shape_end_to_end() {
    let source = r##"%%{init: {
        "notAConfigKey": "removed",
        "theme": null,
        "prototype": "removed",
        "constructor": "removed",
        "deterministicIDSeed": "accepted undefined key",
        "sequence": {"messageFont": "accepted function key"},
        "secure": ["theme"],
        "flowchart": {
            "secure": ["htmlLabels"],
            "securityLevel": "loose",
            "htmlLabels": false
        },
        "sankey": {
            "nodeColors": {
                "valid": "#abc",
                "invalid": "url(javascript:alert(1))",
                "constructor": "red"
            }
        }
    }}%%
flowchart TD
    A --> B
"##;

    let metadata = Engine::new()
        .parse_metadata_sync(source)
        .expect("parse succeeds");
    let retained = metadata.config.as_value();

    assert!(retained.get("notAConfigKey").is_none());
    assert!(retained.get("theme").is_none());
    assert!(retained.get("prototype").is_none());
    assert!(retained.get("constructor").is_none());
    assert!(retained.get("secure").is_none());
    assert!(retained["flowchart"].get("secure").is_none());
    assert_eq!(retained["deterministicIDSeed"], "accepted undefined key");
    assert_eq!(retained["sequence"]["messageFont"], "accepted function key");
    assert_eq!(retained["flowchart"]["htmlLabels"], false);
    assert_eq!(retained["sankey"]["nodeColors"], json!({ "valid": "#abc" }));

    assert_eq!(
        metadata.config.get_str("flowchart.securityLevel"),
        Some("loose")
    );
    assert!(
        metadata
            .effective_config
            .get_str("flowchart.securityLevel")
            .is_none(),
        "the hardened site policy must remove secure keys recursively"
    );
    assert_eq!(
        metadata.effective_config.get_str("securityLevel"),
        Some("strict")
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn retained_semantic_config_handles_deep_public_config_with_small_stack() {
    const DEPTH: usize = 1_024;
    let site_config = MermaidConfig::from_value(deep_config_value(
        "retainedConfig",
        DEPTH,
        Value::String("#556677".to_string()),
    ));
    let engine = Engine::new().with_site_config(site_config);

    let handle = std::thread::Builder::new()
        .name("merman-core-deep-retained-semantic-config".to_string())
        .stack_size(128 * 1024)
        .spawn(move || {
            for (label, diagram_type, source) in [
                ("block", "block", "block\n  A\n"),
                ("state", "stateDiagram", "stateDiagram-v2\n  [*] --> A\n"),
                ("treemap", "treemap", "treemap\n\"A\": 1\n"),
                ("sankey", "sankey", "sankey\nA,B,1\n"),
                ("c4", "c4", "C4Context\nPerson(a, \"A\")\n"),
            ] {
                let parsed = engine
                    .parse_diagram_with_type_sync(diagram_type, source, ParseOptions::strict())
                    .expect("parse succeeds")
                    .expect("diagram detected");
                let ParsedDiagram { model, .. } = parsed;

                assert_eq!(
                    deep_config_leaf(&model["config"], "retainedConfig", DEPTH)
                        .and_then(Value::as_str),
                    Some("#556677"),
                    "retained config for {label}"
                );

                drop(model);
            }

            {
                let (label, diagram_type, source) = (
                    "architecture",
                    "architecture",
                    "architecture-beta\n  service a(server)[A]\n",
                );
                let parsed = engine
                    .parse_diagram_with_type_sync(diagram_type, source, ParseOptions::strict())
                    .expect("parse succeeds")
                    .expect("diagram detected");
                let ParsedDiagram { model, .. } = parsed;

                assert_eq!(
                    deep_config_leaf(&model["config"], "retainedConfig", DEPTH)
                        .and_then(Value::as_str),
                    Some("#556677"),
                    "retained config for {label}"
                );

                drop(model);
            }
        })
        .expect("spawn deep retained semantic config test");
    handle
        .join()
        .expect("retained semantic config projection should finish without stack overflow");
}

#[test]
#[cfg(feature = "all-diagrams")]
fn remaining_retained_semantic_config_handles_deep_public_config_with_small_stack() {
    const DEPTH: usize = 1_024;
    let site_config = MermaidConfig::from_value(deep_config_value(
        "retainedConfig",
        DEPTH,
        Value::String("#778899".to_string()),
    ));
    let engine = Engine::new().with_site_config(site_config);

    let handle = std::thread::Builder::new()
        .name("merman-core-remaining-deep-retained-config".to_string())
        .stack_size(128 * 1024)
        .spawn(move || {
            for (label, diagram_type, source) in [
                ("gitGraph", "gitGraph", "gitGraph:\n commit\n"),
                ("kanban", "kanban", "kanban\n  Todo\n    item\n"),
                ("packet", "packet", "packet\n+8: \"byte\"\n"),
                (
                    "quadrantChart",
                    "quadrantChart",
                    "quadrantChart\nx-axis Low --> High\ny-axis Low --> High\nquadrant-1 A\nP: [0.5, 0.5]\n",
                ),
                ("radar", "radar", "radar-beta\naxis A, B\ncurve one{1,2}\n"),
                (
                    "requirement",
                    "requirement",
                    "requirementDiagram\nrequirement r {\n  id: R\n  text: \"T\"\n  risk: low\n  verifymethod: test\n}\n",
                ),
            ] {
                let parsed = engine
                    .parse_diagram_with_type_sync(diagram_type, source, ParseOptions::strict())
                    .expect("parse succeeds")
                    .expect("diagram detected");
                let ParsedDiagram { model, .. } = parsed;

                assert_eq!(
                    deep_config_leaf(&model["config"], "retainedConfig", DEPTH)
                        .and_then(Value::as_str),
                    Some("#778899"),
                    "retained config for {label}"
                );

                drop(model);
            }

            for (label, diagram_type, source) in [
                ("mindmap", "mindmap", "mindmap\nroot\n child\n"),
                ("mindmap-empty", "mindmap", "mindmap\n"),
            ] {
                let parsed = engine
                    .parse_diagram_with_type_sync(diagram_type, source, ParseOptions::strict())
                    .expect("parse succeeds")
                    .expect("diagram detected");
                let ParsedDiagram { model, .. } = parsed;

                assert_eq!(
                    deep_config_leaf(&model["config"], "retainedConfig", DEPTH)
                        .and_then(Value::as_str),
                    Some("#778899"),
                    "retained config for {label}"
                );

                drop(model);
            }
        })
        .expect("spawn remaining deep retained semantic config test");
    handle.join().expect(
        "remaining retained semantic config projection should finish without stack overflow",
    );
}

#[test]
fn init_directive_config_sanitizes_deep_values_with_small_stack() {
    const DEPTH: usize = 32;
    let source = deep_init_directive_source("sequence", DEPTH, "<blocked>");

    let handle = std::thread::Builder::new()
        .name("merman-core-deep-init-config".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            let meta = Engine::new()
                .parse_metadata_sync(&source)
                .expect("parse succeeds");
            assert_eq!(
                deep_repeated_config_leaf(meta.config.as_value(), "sequence", DEPTH)
                    .and_then(Value::as_str),
                Some("")
            );
        })
        .expect("spawn deep init config test");
    handle
        .join()
        .expect("deep init config should finish without stack overflow");
}

#[test]
fn default_engine_construction_uses_bounded_stack() {
    let handle = std::thread::Builder::new()
        .name("merman-core-default-engine-small-stack".to_string())
        .stack_size(256 * 1024)
        .spawn(|| std::hint::black_box(Engine::new()))
        .expect("spawn default engine construction test");

    handle
        .join()
        .expect("default engine construction should not overflow a 256 KiB stack");
}

#[test]
fn frontmatter_config_deep_merge_handles_deep_values_with_small_stack() {
    const DEPTH: usize = 32;
    let source = deep_frontmatter_config_source("sequence", DEPTH, "#334455");

    let handle = std::thread::Builder::new()
        .name("merman-core-deep-frontmatter-config".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            let meta = Engine::new()
                .parse_metadata_sync(&source)
                .expect("parse succeeds");
            assert_eq!(
                deep_config_leaf(meta.config.as_value(), "sequence", DEPTH).and_then(Value::as_str),
                Some("#334455")
            );
        })
        .expect("spawn deep frontmatter config test");
    handle
        .join()
        .expect("deep frontmatter config should finish without stack overflow");
}

#[test]
fn init_directive_rejects_excessive_config_nesting_with_small_stack() {
    const DEPTH: usize = 300;
    let source = deep_init_directive_source("sequence", DEPTH, "#112233");
    let engine = Engine::new();

    let handle = std::thread::Builder::new()
        .name("merman-core-too-deep-init-config".to_string())
        .stack_size(128 * 1024)
        .spawn(move || {
            let err = engine
                .parse_metadata_sync(&source)
                .expect_err("excessive init config depth should be rejected");
            assert!(
                err.to_string().contains("config nesting exceeds"),
                "unexpected error: {err}"
            );
        })
        .expect("spawn excessive init config test");
    handle
        .join()
        .expect("excessive init config should return an error without stack overflow");
}

#[test]
fn frontmatter_rejects_excessive_config_nesting_with_small_stack() {
    const DEPTH: usize = 300;
    let source = deep_frontmatter_config_source("sequence", DEPTH, "#334455");
    let engine = Engine::new();

    let handle = std::thread::Builder::new()
        .name("merman-core-too-deep-frontmatter-config".to_string())
        .stack_size(128 * 1024)
        .spawn(move || {
            let err = engine
                .parse_metadata_sync(&source)
                .expect_err("excessive frontmatter config depth should be rejected");
            assert!(
                err.to_string().contains("config nesting exceeds"),
                "unexpected error: {err}"
            );
        })
        .expect("spawn excessive frontmatter config test");
    handle
        .join()
        .expect("excessive frontmatter config should return an error without stack overflow");
}

#[test]
fn frontmatter_rejects_excessive_inline_yaml_sequence_nesting_with_small_stack() {
    const DEPTH: usize = 300;
    let mut source = String::from("---\nconfig:\n  ");
    source.push_str(&"- ".repeat(DEPTH));
    source.push_str("\"leaf\"\n---\nsequenceDiagram\nAlice->Bob: Hi\n");
    let engine = Engine::new();

    let handle = std::thread::Builder::new()
        .name("merman-core-too-deep-inline-yaml-sequence".to_string())
        .stack_size(128 * 1024)
        .spawn(move || {
            let err = engine
                .parse_metadata_sync(&source)
                .expect_err("excessive inline YAML sequence depth should be rejected");
            assert!(
                err.to_string().contains("config nesting exceeds"),
                "unexpected error: {err}"
            );
        })
        .expect("spawn excessive inline YAML sequence test");
    handle
        .join()
        .expect("excessive inline YAML sequence should return an error without stack overflow");
}

#[test]
fn frontmatter_non_string_yaml_keys_are_ignored_like_legacy_conversion() {
    let engine = Engine::new();
    let res = block_on(engine.parse_metadata(
        "---\n? [non, string, key]\n: ignored\n---\nsequenceDiagram\nAlice->Bob: Hi\n",
    ))
    .expect("non-string YAML keys should not fail frontmatter parsing");

    assert_eq!(res.diagram_type, "sequence");
    assert_eq!(res.config.as_value(), &json!({}));
}

#[test]
fn parse_returns_malformed_frontmatter_error_for_unclosed_frontmatter() {
    let engine = Engine::new();
    let err =
        block_on(engine.parse_metadata("---\ntitle: a malformed YAML front-matter\n")).unwrap_err();
    assert!(err.to_string().contains("Malformed YAML front-matter"));
}

#[test]
fn parse_matches_public_api_for_mismatched_indented_frontmatter() {
    let engine = Engine::new();
    let err = block_on(engine.parse_metadata(
        "---\ntitle: mismatched YAML front-matter\n   ---\nsequenceDiagram\nAlice->Bob: Hi\n",
    ))
    .unwrap_err();
    assert!(err.to_string().contains("Malformed YAML front-matter"));

    // Mermaid 12 mermaidAPI.parse passes DiagramCode directly to Diagram.fromText:
    // a second preprocess no longer strips the indented opener. Upstream rejects
    // this input in the sequence parser; metadata-only Merman retains its public
    // MalformedFrontMatter boundary for an unconsumed leading delimiter.
    let err = block_on(engine.parse_metadata(
        "   ---\ntitle: mismatched YAML front-matter\n---\nsequenceDiagram\nAlice->Bob: Hi\n",
    ))
    .expect_err("a mismatched frontmatter opener must survive the single preprocess pass");
    assert!(matches!(err, Error::MalformedFrontMatter));
}

#[test]
fn parse_metadata_rejects_unknown_diagram_sources() {
    let engine = Engine::new();
    let error = block_on(engine.parse_metadata("this is not a mermaid diagram definition"))
        .expect_err("metadata detection has no lenient mode");

    assert!(matches!(error, Error::DetectType(_)));
}

#[test]
fn parse_metadata_with_type_rejects_errors_without_a_suppression_mode() {
    let error = Engine::new()
        .parse_metadata_with_type_sync(
            "flowchart-v2",
            "---\ntitle: unclosed frontmatter\nflowchart TD\nA-->B\n",
        )
        .expect_err("known-type metadata must surface preprocessing errors");

    assert!(matches!(error, Error::MalformedFrontMatter));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_lenient_unknown_diagram_returns_none_only_for_model_entrypoints() {
    let engine = Engine::new();
    let input = "this is not a mermaid diagram definition";
    let options = ParseOptions::lenient();

    assert!(matches!(
        engine.parse_metadata_sync(input),
        Err(Error::DetectType(_))
    ));
    assert!(engine.parse_diagram_sync(input, options).unwrap().is_none());
    assert!(
        engine
            .parse_diagram_for_render_model_sync(input, options)
            .unwrap()
            .is_none()
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_lenient_failures_use_error_diagram_across_engine_entrypoints() {
    let engine = Engine::new();
    let input = "flowchart TD\nA -->";
    let options = ParseOptions::lenient();
    let message = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap_err()
        .to_string();

    let parsed = engine.parse_diagram_sync(input, options).unwrap().unwrap();
    assert_eq!(parsed.model["errorMessage"], message);
    assert_suppressed_error_diagram(&parsed);

    let parsed = engine
        .parse_diagram_with_type_sync("flowchart-v2", input, options)
        .unwrap()
        .unwrap();
    assert_suppressed_error_diagram(&parsed);

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, options)
        .unwrap()
        .unwrap();
    assert_suppressed_error_render_diagram(&parsed);

    let parsed = engine
        .parse_diagram_for_render_model_with_type_sync("flowchart-v2", input, options)
        .unwrap()
        .unwrap();
    assert_suppressed_error_render_diagram(&parsed);
    let RenderSemanticModel::Error(model) = parsed.model() else {
        unreachable!()
    };
    assert_eq!(model.error_message.as_deref(), Some(message.as_str()));

    let parsed = engine
        .parse_diagram_for_render_model_controlled_sync(input, options, &OperationControl::new())
        .unwrap()
        .unwrap()
        .unwrap();
    let RenderSemanticModel::Error(model) = parsed.model() else {
        unreachable!()
    };
    assert_eq!(model.error_message.as_deref(), Some(message.as_str()));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn controlled_render_model_parse_returns_structured_cancellation_before_work() {
    let operation = OperationControl::new();
    operation.cancel();

    let error = Engine::new()
        .parse_diagram_for_render_model_controlled_sync(
            "flowchart TD\nA-->B\n",
            ParseOptions::strict(),
            &operation,
        )
        .expect_err("cancelled operation must stop before parsing");

    assert_eq!(error.phase, OperationPhase::Admission);
    assert_eq!(error.reason, CancelReason::Requested);
}

#[test]
#[cfg(feature = "all-diagrams")]
fn controlled_render_model_parse_stops_inside_family_parser() {
    let mut source = String::from("flowchart TD\n");
    for index in 0..4_096 {
        source.push_str(&format!("n{index}-->n{}\n", index + 1));
    }
    let operation = OperationControl::new();
    operation.cancel_after_checkpoints(128);

    let error = Engine::new()
        .parse_diagram_for_render_model_controlled_sync(&source, ParseOptions::strict(), &operation)
        .expect_err("scheduled cancellation must stop family parsing");

    assert_eq!(error.reason, CancelReason::Requested);
    assert!(matches!(
        error.phase,
        OperationPhase::Parse | OperationPhase::Semantic
    ));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn controlled_known_type_render_model_parse_skips_detection_but_observes_control() {
    let operation = OperationControl::new();
    operation.cancel();

    let error = Engine::new()
        .parse_diagram_for_render_model_with_type_controlled_sync(
            "flowchart-v2",
            "flowchart TD\nA-->B\n",
            ParseOptions::strict(),
            &operation,
        )
        .expect_err("known-type operation must still observe cancellation");

    assert_eq!(error.reason, CancelReason::Requested);
    assert_eq!(error.phase, OperationPhase::Admission);
}

#[test]
#[cfg(feature = "all-diagrams")]
fn explicit_error_diagram_uses_the_typed_builtin_render_model() {
    let parsed = Engine::new()
        .parse_diagram_for_render_model_sync("error", ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_suppressed_error_render_diagram(&parsed);
    assert_eq!(
        parsed
            .model()
            .compatibility_json(parsed.metadata())
            .unwrap(),
        json!({ "type": "error" })
    );
    assert!(parsed.model().supports_diagram_type("error"));
    assert!(!parsed.model().supports_diagram_type("flowchart-v2"));
}

#[cfg(feature = "all-diagrams")]
fn assert_suppressed_error_diagram(parsed: &ParsedDiagram) {
    assert_eq!(parsed.meta.diagram_type, "error");
    assert_eq!(parsed.model["type"], json!("error"));
}

#[cfg(feature = "all-diagrams")]
fn assert_suppressed_error_render_diagram(parsed: &ParsedDiagramRender) {
    assert_eq!(parsed.metadata().diagram_type, "error");
    match parsed.model() {
        RenderSemanticModel::Error(model) => {
            assert_eq!(model.diagram_type, "error");
            assert_eq!(
                serde_json::to_value(model).unwrap(),
                parsed
                    .model()
                    .compatibility_json(parsed.metadata())
                    .unwrap()
            );
        }
        other => panic!("suppressed parse failures must use the typed error model, got {other:?}"),
    }
}

#[test]
#[cfg(feature = "all-diagrams")]
fn render_semantic_model_kind_reports_canonical_names() {
    let sequence = render_model_for("sequenceDiagram\nAlice->>Bob: Hi");
    assert_eq!(sequence.kind(), "sequence");

    let flowchart = render_model_for("flowchart TD\nA-->B");
    assert_eq!(flowchart.kind(), "flowchart");

    let er = render_model_for("erDiagram\nCUSTOMER ||--o{ ORDER : places");
    assert_eq!(er.kind(), "er");

    let custom_model = RenderSemanticModel::CustomJson(CustomJsonRenderModel::new(
        "custom-model",
        json!({ "type": "custom" }),
    ));
    assert_eq!(custom_model.kind(), "custom-json");
}

#[test]
#[cfg(feature = "all-diagrams")]
fn render_semantic_model_supports_diagram_type_aliases() {
    let sequence = render_model_for("sequenceDiagram\nAlice->>Bob: Hi");
    assert!(sequence.supports_diagram_type("sequence"));
    assert!(!sequence.supports_diagram_type("zenuml"));
    assert!(!sequence.supports_diagram_type("flowchart-v2"));

    let zenuml = render_model_for("zenuml\n@Starter(A)\nB.call()\n");
    assert!(zenuml.supports_diagram_type("zenuml"));
    assert!(!zenuml.supports_diagram_type("sequence"));

    let flowchart = render_model_for("flowchart TD\nA-->B");
    assert!(flowchart.supports_diagram_type("flowchart-v2"));
    assert!(flowchart.supports_diagram_type("flowchart"));
    assert!(flowchart.supports_diagram_type("flowchart-elk"));
    assert!(!flowchart.supports_diagram_type("sequence"));

    let er = render_model_for("erDiagram\nCUSTOMER ||--o{ ORDER : places");
    assert!(er.supports_diagram_type("er"));
    assert!(er.supports_diagram_type("erDiagram"));
    assert!(!er.supports_diagram_type("classDiagram"));

    let custom_model = RenderSemanticModel::CustomJson(CustomJsonRenderModel::new(
        "unknown-plugin",
        json!({ "type": "custom" }),
    ));
    assert!(custom_model.supports_diagram_type("unknown-plugin"));
    assert!(!custom_model.supports_diagram_type("flowchart-v2"));

    let builtin_named_custom = RenderSemanticModel::CustomJson(CustomJsonRenderModel::new(
        "flowchart-v2",
        json!({ "type": "custom" }),
    ));
    assert!(!builtin_named_custom.supports_diagram_type("flowchart-v2"));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn render_parser_registry_drives_typed_alias_parse() {
    let engine = Engine::new();
    assert!(engine.render_diagram_registry().contains("flowchart-elk"));

    let parsed = engine
        .parse_diagram_for_render_model_with_type_sync(
            "flowchart-elk",
            "flowchart-elk TD\nA-->B;",
            ParseOptions::strict(),
        )
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "flowchart-elk");
    assert_eq!(parsed.model().kind(), "flowchart");
    assert!(matches!(parsed.model(), RenderSemanticModel::Flowchart(_)));
}

#[test]
fn custom_semantic_parser_projects_an_explicit_json_render_boundary() {
    let mut engine = Engine::new();
    engine
        .diagram_registry_mut()
        .insert("customDiagram", custom_json_parser);
    let source = "%%{init: { 'securityLevel': 'strict' }}%%\ncustomDiagram\npayload";

    let parsed = engine
        .parse_diagram_for_render_model_with_type_sync(
            "customDiagram",
            source,
            ParseOptions::strict(),
        )
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "customDiagram");
    let RenderSemanticModel::CustomJson(model) = parsed.model() else {
        panic!("custom semantic parsers must produce an explicit CustomJson render boundary");
    };
    assert_eq!(model.model_name(), "customDiagram");
    assert_eq!(
        model.provenance(),
        CustomJsonProvenance::SemanticRegistryOverlay
    );
    assert_eq!(model.value()["type"], json!("customDiagram"));
    assert_eq!(
        model.value()["title"],
        json!(expected_custom_title_after_sanitization())
    );
    let payload_start = source.find("payload").unwrap();
    assert_eq!(
        model.value()["warningFacts"][0]["span"],
        json!({ "start": payload_start, "end": payload_start + "payload".len() })
    );
}

#[test]
fn combined_custom_warning_adapter_remaps_valid_json_into_typed_facts() {
    let mut engine = Engine::new();
    engine
        .diagram_registry_mut()
        .insert("customDiagram", custom_json_parser);
    let source = "%%{init: { 'securityLevel': 'strict' }}%%\ncustomDiagram\npayload";
    let snapshot = engine
        .parse_diagram_snapshot_with_type_sync("customDiagram", source)
        .unwrap()
        .unwrap();
    let DiagramParseOutcome::Parsed {
        model,
        warning_facts,
    } = snapshot.outcome()
    else {
        panic!("custom warning fixture must parse");
    };
    let payload_start = source.find("payload").unwrap();

    assert_eq!(warning_facts.len(), 1);
    assert_eq!(
        warning_facts[0].span,
        Some(SourceSpan::new(
            payload_start,
            payload_start + "payload".len()
        ))
    );
    assert_eq!(model["warningFacts"], json!(warning_facts));
}

#[test]
fn combined_custom_warning_adapter_keeps_malformed_arrays_all_or_nothing() {
    let mut engine = Engine::new();
    engine
        .diagram_registry_mut()
        .insert("customDiagram", malformed_custom_warning_json_parser);
    let source = "%%{init: { 'securityLevel': 'strict' }}%%\ncustomDiagram\npayload";
    let snapshot = engine
        .parse_diagram_snapshot_with_type_sync("customDiagram", source)
        .unwrap()
        .unwrap();
    let DiagramParseOutcome::Parsed {
        model,
        warning_facts,
    } = snapshot.outcome()
    else {
        panic!("malformed custom warning fixture must retain its semantic model");
    };

    assert!(warning_facts.is_empty());
    assert_eq!(
        model["warningFacts"],
        json!([
            {
                "ruleId": FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID,
                "message": "custom warning",
                "span": { "start": 0, "end": 1 }
            },
            null
        ])
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn combined_builtin_warning_facts_stay_typed_and_sync_compatibility_json() {
    let cases = [
        (
            "---\ntitle: Demo\n---\nflowchart\nA-->B",
            FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID,
        ),
        (
            "block-beta\n  columns 1\n  A:1\n  B:2\n  C:3\n",
            BLOCK_WIDTH_WARNING_RULE_ID,
        ),
        (
            "gitGraph\ncommit id:\"duplicate\"\ncommit id:\"duplicate\"\n",
            GIT_GRAPH_DUPLICATE_COMMIT_WARNING_RULE_ID,
        ),
    ];

    for (source, expected_rule_id) in cases {
        let snapshot = Engine::new()
            .parse_diagram_snapshot_sync(source)
            .unwrap()
            .unwrap();
        let DiagramParseOutcome::Parsed {
            model,
            warning_facts,
        } = snapshot.outcome()
        else {
            panic!("built-in warning fixture must parse: {source}");
        };

        assert!(
            warning_facts
                .iter()
                .any(|fact| fact.rule_id == expected_rule_id),
            "missing typed warning {expected_rule_id} for {source}"
        );
        assert_eq!(model["warningFacts"], json!(warning_facts));
    }

    let flowchart_source = cases[0].0;
    let snapshot = Engine::new()
        .parse_diagram_snapshot_sync(flowchart_source)
        .unwrap()
        .unwrap();
    let DiagramParseOutcome::Parsed { warning_facts, .. } = snapshot.outcome() else {
        unreachable!("flowchart warning fixture parsed above");
    };
    let flowchart_start = flowchart_source.find("flowchart").unwrap();
    assert_eq!(
        warning_facts[0].span,
        Some(SourceSpan::new(
            flowchart_start,
            flowchart_start + "flowchart".len()
        ))
    );
}

#[test]
fn custom_semantic_overlay_does_not_inherit_builtin_family_capabilities() {
    for (diagram_type, source) in [
        ("flowchart-v2", "flowchart TD\nA-->B"),
        (
            "architecture",
            "architecture-beta\nservice api(server)[API]",
        ),
    ] {
        let mut engine = Engine::new();
        engine
            .diagram_registry_mut()
            .insert(diagram_type, custom_overlay_json_parser);

        let parsed = engine
            .parse_diagram_for_render_model_with_type_sync(
                diagram_type,
                source,
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let RenderSemanticModel::CustomJson(model) = parsed.model() else {
            panic!("custom semantic overlay for {diagram_type} inherited a built-in typed model");
        };
        assert_eq!(model.model_name(), diagram_type);
        assert_eq!(
            model.provenance(),
            CustomJsonProvenance::SemanticRegistryOverlay
        );
        assert_eq!(model.value()["owner"], json!("custom-semantic"));
        assert_eq!(model.value()["diagramType"], json!(diagram_type));
        assert!(!parsed.model().supports_diagram_type(diagram_type));

        assert!(
            engine
                .parse_editor_semantic_facts_with_type_sync(diagram_type, source,)
                .unwrap()
                .is_none(),
            "custom semantic overlay for {diagram_type} inherited built-in editor facts"
        );
    }
}

#[test]
fn combined_parse_keeps_custom_semantic_overlay_and_reports_editor_unavailable() {
    for (diagram_type, source) in [
        ("flowchart-v2", "flowchart TD\nA-->B"),
        (
            "architecture",
            "architecture-beta\nservice api(server)[API]",
        ),
    ] {
        let mut engine = Engine::new();
        engine
            .diagram_registry_mut()
            .insert(diagram_type, custom_overlay_json_parser);

        let parsed = engine.parse_diagram_snapshot_sync(source).unwrap().unwrap();
        assert_eq!(parsed.metadata().diagram_type, diagram_type);
        assert_eq!(
            parsed
                .outcome()
                .parsed_model()
                .expect("expected parsed snapshot")["owner"],
            json!("custom-semantic")
        );
        assert!(matches!(
            parsed.editor_facts(),
            crate::ParsedEditorFacts::Unavailable
        ));
    }
}

#[test]
fn combined_parse_keeps_generic_custom_semantics_and_reports_editor_unavailable() {
    let mut engine = Engine::new();
    engine
        .registry_mut()
        .add_fn("generic-custom", detect_generic_custom);
    engine
        .diagram_registry_mut()
        .insert("generic-custom", custom_overlay_json_parser);

    let parsed = engine
        .parse_diagram_snapshot_sync("generic-custom\npayload")
        .unwrap()
        .unwrap();
    assert_eq!(parsed.metadata().diagram_type, "generic-custom");
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["owner"],
        json!("custom-semantic")
    );
    assert!(matches!(
        parsed.editor_facts(),
        crate::ParsedEditorFacts::Unavailable
    ));
}

#[test]
fn combined_parse_retains_metadata_when_a_custom_parser_panics() {
    let mut engine = Engine::new();
    engine
        .registry_mut()
        .add_fn("generic-custom", detect_generic_custom);
    engine
        .diagram_registry_mut()
        .insert("generic-custom", panicking_custom_parser);
    let snapshot = engine
        .parse_diagram_snapshot_sync(
            "%%{ init: {\"theme\":\"dark\"} }%%\ngeneric-custom\npayload\n",
        )
        .expect("snapshot capture should contain parser panics")
        .expect("custom detector should produce a snapshot");

    assert_eq!(snapshot.metadata().diagram_type, "generic-custom");
    assert_eq!(snapshot.metadata().config.get_str("theme"), Some("dark"));
    let crate::DiagramParseOutcome::Panicked(message) = snapshot.outcome() else {
        panic!("custom parser panic was not retained in the snapshot");
    };
    assert!(message.contains("custom parser fixture panic"));
    assert!(matches!(
        snapshot.editor_facts(),
        crate::ParsedEditorFacts::Unavailable
    ));
    assert!(!snapshot.recovered_incomplete_directive());
}

#[test]
#[cfg(feature = "all-diagrams")]
fn combined_parse_records_recovered_incomplete_directives() {
    let snapshot = Engine::new()
        .parse_diagram_snapshot_sync(concat!(
            "%%{ init: {\"theme\":\"dark\"} }%%\n",
            "%%{ malformed\n",
            "flowchart TD\n",
            "A-->B\n",
        ))
        .expect("editor snapshot preprocessing should recover the malformed line")
        .expect("the later flowchart should still be detected");

    assert_eq!(snapshot.metadata().config.get_str("theme"), Some("dark"));
    assert!(snapshot.recovered_incomplete_directive());
}

#[test]
#[cfg(feature = "diagram-wardley")]
fn combined_parse_uses_wardley_semantics() {
    let wardley = Engine::new()
        .parse_diagram_snapshot_sync("wardley-beta\ntitle Example")
        .unwrap()
        .unwrap();
    assert_eq!(wardley.metadata().diagram_type, "wardley");
    assert_eq!(
        wardley
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["type"],
        json!("wardley")
    );
    assert_eq!(
        wardley
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["title"],
        json!("Example")
    );
    assert!(matches!(
        wardley.editor_facts(),
        crate::ParsedEditorFacts::Available(_)
    ));
}

#[test]
fn combined_parse_retains_missing_custom_parser_failure() {
    let mut custom = Engine::new();
    custom
        .registry_mut()
        .add_fn("generic-custom", detect_generic_custom);
    assert_combined_missing_semantic_contract(&custom, "generic-custom", "generic-custom\npayload");
}

fn assert_combined_missing_semantic_contract(engine: &Engine, expected_type: &str, source: &str) {
    let snapshot = engine.parse_diagram_snapshot_sync(source).unwrap().unwrap();
    let (meta, outcome, editor_facts) = snapshot.into_parts();
    assert_eq!(meta.diagram_type, expected_type);
    let crate::DiagramParseOutcome::Failed(error) = outcome else {
        panic!("custom snapshot unexpectedly parsed {expected_type}");
    };
    let crate::Error::UnsupportedDiagram { diagram_type } = error else {
        panic!("unexpected combined error for {expected_type}: {error}");
    };
    assert_eq!(diagram_type, expected_type);
    assert!(matches!(
        editor_facts,
        crate::ParsedEditorFacts::Unavailable
    ));
}

#[test]
fn explicit_custom_render_overlay_wins_over_semantic_and_builtin_renderers() {
    let mut engine = Engine::new();
    engine
        .diagram_registry_mut()
        .insert("flowchart-v2", custom_overlay_json_parser);
    engine
        .render_diagram_registry_mut()
        .insert("flowchart-v2", custom_overlay_render_parser);

    let source = "%%{init: { 'securityLevel': 'strict' }}%%\nflowchart TD\nA-->B";
    let parsed = engine
        .parse_diagram_for_render_model_with_type_sync(
            "flowchart-v2",
            source,
            ParseOptions::strict(),
        )
        .unwrap()
        .unwrap();
    let RenderSemanticModel::CustomJson(model) = parsed.model() else {
        panic!("explicit custom render overlay should select an explicit CustomJson model");
    };
    assert_eq!(model.model_name(), "custom-flowchart-model");
    assert_eq!(
        model.provenance(),
        CustomJsonProvenance::RenderRegistryOverlay
    );
    assert_eq!(model.value()["owner"], json!("custom-render"));
    assert_eq!(
        model.value()["title"],
        json!(expected_custom_title_after_sanitization())
    );
    let edge_start = source.find("A-->B").unwrap();
    assert_eq!(
        model.value()["warningFacts"][0]["span"],
        json!({ "start": edge_start, "end": edge_start + "A-->B".len() })
    );
}

#[test]
#[cfg(feature = "diagram-flowchart")]
fn missing_builtin_typed_parser_does_not_fall_back_to_custom_json() {
    let mut engine = Engine::new();
    assert!(engine.render_diagram_registry_mut().remove("flowchart-v2"));

    let err = engine
        .parse_diagram_for_render_model_with_type_sync(
            "flowchart-v2",
            "flowchart TD\nA-->B",
            ParseOptions::strict(),
        )
        .unwrap_err();
    let message = err.to_string();

    assert!(message.contains("missing a typed render parser"));
    assert!(message.contains("custom JSON boundary is reserved"));
}

fn custom_json_parser(
    code: &str,
    _meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<serde_json::Value>> {
    control.checkpoint()?;
    let payload_start = code.find("payload").unwrap();
    Ok(Ok(json!({
        "type": "customDiagram",
        "title": "<script>discarded</script><b>custom</b>",
        "warningFacts": [{
            "ruleId": "custom.semantic.warning",
            "message": "custom semantic warning",
            "span": {
                "start": payload_start,
                "end": payload_start + "payload".len(),
            },
        }],
    })))
}

fn malformed_custom_warning_json_parser(
    _code: &str,
    _meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<serde_json::Value>> {
    control.checkpoint()?;
    Ok(Ok(json!({
        "warningFacts": [
            {
                "ruleId": FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID,
                "message": "custom warning",
                "span": { "start": 0, "end": 1 }
            },
            null
        ]
    })))
}

fn custom_overlay_json_parser(
    _code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<serde_json::Value>> {
    control.checkpoint()?;
    Ok(Ok(json!({
        "owner": "custom-semantic",
        "diagramType": meta.diagram_type,
    })))
}

fn panicking_custom_parser(
    _code: &str,
    _meta: &ParseMetadata,
    _control: &OperationControl,
) -> OperationControlResult<Result<serde_json::Value>> {
    panic!("custom parser fixture panic")
}

fn custom_overlay_render_parser(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<CustomJsonRenderModel>> {
    control.checkpoint()?;
    let edge_start = code.find("A-->B").unwrap();
    Ok(Ok(CustomJsonRenderModel::new(
        "custom-flowchart-model",
        json!({
            "owner": "custom-render",
            "diagramType": meta.diagram_type,
            "title": "<script>discarded</script><b>custom</b>",
            "warningFacts": [{
                "ruleId": "custom.render.warning",
                "message": "custom render warning",
                "span": {
                    "start": edge_start,
                    "end": edge_start + "A-->B".len(),
                },
            }],
        }),
    )))
}

fn expected_custom_title_after_sanitization() -> &'static str {
    "<b>custom</b>"
}

fn detect_generic_custom(text: &str, _config: &mut MermaidConfig) -> bool {
    text.trim_start().starts_with("generic-custom")
}

#[cfg(feature = "all-diagrams")]
fn render_model_for(input: &str) -> RenderSemanticModel {
    Engine::new()
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap()
        .into_parts()
        .1
}

fn deep_config_value(root_key: &str, depth: usize, leaf: Value) -> Value {
    let mut value = leaf;
    for idx in (0..depth).rev() {
        let mut map = Map::new();
        map.insert(format!("k{idx}"), value);
        value = Value::Object(map);
    }

    let mut root = Map::new();
    root.insert(root_key.to_string(), value);
    Value::Object(root)
}

fn deep_config_leaf<'a>(mut value: &'a Value, root_key: &str, depth: usize) -> Option<&'a Value> {
    value = value.as_object()?.get(root_key)?;
    for idx in 0..depth {
        value = value.as_object()?.get(&format!("k{idx}"))?;
    }
    Some(value)
}

fn deep_repeated_config_leaf<'a>(
    mut value: &'a Value,
    key: &str,
    depth: usize,
) -> Option<&'a Value> {
    value = value.as_object()?.get(key)?;
    for _ in 0..depth {
        value = value.as_object()?.get(key)?;
    }
    Some(value)
}

fn deep_init_directive_source(root_key: &str, depth: usize, leaf: &str) -> String {
    let mut source = format!(r#"%%{{init: {{"{root_key}": "#);
    for _ in 0..depth {
        write!(&mut source, r#"{{"{root_key}":"#).expect("write init config");
    }
    write!(&mut source, "{leaf:?}").expect("write init leaf");
    for _ in 0..depth {
        source.push('}');
    }
    source.push_str("}}%%\nsequenceDiagram\nAlice->Bob: Hi\n");
    source
}

fn deep_frontmatter_config_source(root_key: &str, depth: usize, leaf: &str) -> String {
    let mut source = format!("---\nconfig: {{\"{root_key}\": ");
    for idx in 0..depth {
        write!(&mut source, r#"{{"k{idx}":"#).expect("write frontmatter config");
    }
    write!(&mut source, "{leaf:?}").expect("write frontmatter leaf");
    for _ in 0..depth {
        source.push('}');
    }
    source.push_str("}\n---\nsequenceDiagram\nAlice->Bob: Hi\n");
    source
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_flowchart_json_and_typed_render_model_share_semantic_source() {
    let engine = Engine::new();
    let input = r#"%%{init: {"securityLevel":"strict","flowchart":{"inheritDir":true}}}%%
flowchart LR
accTitle: Flowchart <b>access title</b>
accDescr { Flowchart <b>access description</b>
    second line
}
subgraph Cluster["Cluster <b>One</b>"]
  A["<b>Start</b>"]:::hot e1@-->|"<i>Edge</i>"| B@{ shape: rounded, label: "End" }
end
classDef hot fill:#fff,stroke:#333
class B hot
style Cluster stroke:#f66
linkStyle 0 stroke:#111,stroke-width:2px
click A href "https://example.test" "tip <b>safe</b>" _blank
"#;

    let typed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let model = match typed.model() {
        RenderSemanticModel::Flowchart(model) => model,
        other => panic!("flowchart render parse should return typed model, got {other:?}"),
    };
    let typed_json = serde_json::to_value(model).unwrap();

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap()
        .model;

    for key in [
        "accTitle",
        "accDescr",
        "classDefs",
        "direction",
        "edgeDefaults",
        "vertexCalls",
        "tooltips",
    ] {
        assert_eq!(typed_json[key], parsed_json[key], "shared key {key}");
    }

    let typed_nodes = typed_json["nodes"].as_array().unwrap();
    let json_nodes = parsed_json["nodes"].as_array().unwrap();
    assert_eq!(typed_nodes.len(), json_nodes.len());
    for (typed_node, json_node) in typed_nodes.iter().zip(json_nodes) {
        for key in [
            "id",
            "label",
            "labelType",
            "layoutShape",
            "icon",
            "form",
            "pos",
            "img",
            "constraint",
            "assetWidth",
            "assetHeight",
            "styles",
            "classes",
            "link",
            "linkTarget",
            "haveCallback",
        ] {
            assert_eq!(typed_node[key], json_node[key], "node key {key}");
        }
    }

    let typed_edges = typed_json["edges"].as_array().unwrap();
    let json_edges = parsed_json["edges"].as_array().unwrap();
    assert_eq!(typed_edges.len(), json_edges.len());
    for (typed_edge, json_edge) in typed_edges.iter().zip(json_edges) {
        for key in [
            "id",
            "from",
            "to",
            "label",
            "labelType",
            "type",
            "stroke",
            "length",
            "style",
            "classes",
            "interpolate",
            "animate",
            "animation",
        ] {
            assert_eq!(typed_edge[key], json_edge[key], "edge key {key}");
        }
    }

    let typed_subgraphs = typed_json["subgraphs"].as_array().unwrap();
    let json_subgraphs = parsed_json["subgraphs"].as_array().unwrap();
    assert_eq!(typed_subgraphs.len(), json_subgraphs.len());
    for (typed_subgraph, json_subgraph) in typed_subgraphs.iter().zip(json_subgraphs) {
        for key in [
            "id",
            "nodes",
            "title",
            "classes",
            "styles",
            "dir",
            "labelType",
        ] {
            assert_eq!(
                typed_subgraph[key], json_subgraph[key],
                "subgraph key {key}"
            );
        }
    }
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_sequence_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"sequenceDiagram
participant Alice as Alice A.
participant Bob
box aqua Core
participant Carol
end
Alice->>+Bob: Hi
Note right of Bob: Seen
create participant Dana
Bob-->>Dana: Spawn
destroy Dana
Dana-->>Bob: Done
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "sequence");
    let typed_json = match parsed.model() {
        RenderSemanticModel::Sequence(model) => {
            assert_eq!(model.actor_order, ["Alice", "Bob", "Carol", "Dana"]);
            assert_eq!(model.messages[0].message_text(), "Hi");
            assert_eq!(model.notes[0].message, "Seen");
            assert_eq!(model.boxes[0].actor_keys, ["Carol"]);
            assert_eq!(model.created_actors["Dana"], 3);
            assert_eq!(model.destroyed_actors["Dana"], 4);
            model.to_compat_json(&parsed.metadata().diagram_type)
        }
        other => panic!("sequence render parse should return typed model, got {other:?}"),
    };

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("sequence"));
    assert_eq!(parsed_json.model["messages"][0]["message"], json!("Hi"));
    assert!(parsed_json.model["messages"][0].get("placement").is_none());
    assert!(
        parsed_json.model["messages"][0]
            .get("centralConnection")
            .is_none()
    );
    assert_eq!(typed_json, parsed_json.model);
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_kanban_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = "kanban\n  Todo\n    item1\n  Doing\n    item2";

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "kanban");
    match parsed.model() {
        RenderSemanticModel::Kanban(model) => {
            assert_eq!(model.nodes.len(), 4);
            assert!(model.nodes[0].is_group);
            assert_eq!(model.nodes[0].label, "Todo");
            assert_eq!(model.nodes[1].label, "item1");
            assert_eq!(model.nodes[1].parent_id.as_deref(), Some("Todo"));
        }
        other => panic!("kanban render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("kanban"));
    assert_eq!(parsed_json.model["nodes"][0]["label"], json!("Todo"));
    assert_eq!(parsed_json.model["nodes"][1]["label"], json!("item1"));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_gantt_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
gantt
dateFormat YYYY-MM-DD
title Typed Gantt
section Alpha
Task 1: id1, 2024-01-01, 2d
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "gantt");
    match parsed.model() {
        RenderSemanticModel::Gantt(model) => {
            assert_eq!(model.title.as_deref(), Some("Typed Gantt"));
            assert_eq!(model.date_format, "YYYY-MM-DD");
            assert_eq!(model.tasks.len(), 1);
            assert_eq!(model.tasks[0].id, "id1");
            assert_eq!(model.tasks[0].task, "Task 1");
            assert_eq!(model.tasks[0].section, "Alpha");
        }
        other => panic!("gantt render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("gantt"));
    assert_eq!(parsed_json.model["title"], json!("Typed Gantt"));
    assert_eq!(parsed_json.model["tasks"][0]["id"], json!("id1"));
    assert_eq!(parsed_json.model["tasks"][0]["task"], json!("Task 1"));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_pie_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
pie showData title Typed Pie
  "Alpha" : 60
  "Beta" : 40
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "pie");
    match parsed.model() {
        RenderSemanticModel::Pie(model) => {
            assert!(model.show_data);
            assert_eq!(model.title.as_deref(), Some("Typed Pie"));
            assert_eq!(model.sections.len(), 2);
            assert_eq!(model.sections[0].label, "Alpha");
            assert_eq!(model.sections[0].value, 60.0);
        }
        other => panic!("pie render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("pie"));
    assert_eq!(parsed_json.model["showData"], json!(true));
    assert_eq!(parsed_json.model["title"], json!("Typed Pie"));
    assert_eq!(parsed_json.model["sections"][0]["label"], json!("Alpha"));
    assert_eq!(parsed_json.model["sections"][0]["value"], json!(60.0));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_xychart_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
xychart horizontal
title "Typed XYChart"
accTitle: XY accTitle
accDescr: XY accDescription
x-axis "X Axis" [Alpha, Beta]
y-axis "Y Axis" 1 --> 5
bar "Series 1" [1, 2]
line "Series 2" [2 "early", 3 "late"]
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "xychart");
    let typed_json = match parsed.model() {
        RenderSemanticModel::XyChart(model) => {
            assert_eq!(model.orientation, "horizontal");
            assert_eq!(model.title.as_deref(), Some("Typed XYChart"));
            assert_eq!(model.acc_title.as_deref(), Some("XY accTitle"));
            assert_eq!(model.acc_descr.as_deref(), Some("XY accDescription"));
            assert_eq!(
                model.x_axis,
                XyChartAxisRenderModel::Band {
                    title: "X Axis".to_string(),
                    categories: vec!["Alpha".to_string(), "Beta".to_string()],
                }
            );
            assert_eq!(
                model.y_axis,
                XyChartAxisRenderModel::Linear {
                    title: "Y Axis".to_string(),
                    min: Some(1.0),
                    max: Some(5.0),
                }
            );
            assert_eq!(model.plots.len(), 2);
            assert_eq!(model.plots[0].plot_type, XyChartPlotType::Bar);
            assert_eq!(model.plots[0].title.as_deref(), Some("Series 1"));
            assert_eq!(model.plots[0].values, vec![1.0, 2.0]);
            assert_eq!(
                model.plots[0].data,
                vec![
                    ("Alpha".to_string(), Some(1.0)),
                    ("Beta".to_string(), Some(2.0)),
                ]
            );
            assert_eq!(model.plots[1].plot_type, XyChartPlotType::Line);
            assert_eq!(model.plots[1].title.as_deref(), Some("Series 2"));
            assert_eq!(model.plots[1].point_labels, vec!["early", "late"]);
            model.to_compat_json(parsed.metadata())
        }
        other => panic!("xychart render parse should return typed model, got {other:?}"),
    };

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("xychart"));
    assert_eq!(parsed_json.model["title"], json!("Typed XYChart"));
    assert_eq!(parsed_json.model["xAxis"]["type"], json!("band"));
    assert_eq!(
        parsed_json.model["xAxis"]["categories"],
        json!(["Alpha", "Beta"])
    );
    assert_eq!(parsed_json.model["yAxis"]["type"], json!("linear"));
    assert_eq!(parsed_json.model["yAxis"]["min"], json!(1.0));
    assert_eq!(parsed_json.model["yAxis"]["max"], json!(5.0));
    assert_eq!(parsed_json.model["plots"][0]["type"], json!("bar"));
    assert_eq!(parsed_json.model["plots"][0]["title"], json!("Series 1"));
    assert_eq!(parsed_json.model["plots"][1]["type"], json!("line"));
    assert_eq!(
        parsed_json.model["plots"][1]["pointLabels"],
        json!(["early", "late"])
    );
    assert_eq!(parsed_json.model["plots"][1]["title"], json!("Series 2"));
    assert!(parsed_json.model.get("config").is_some());
    assert_eq!(typed_json, parsed_json.model);
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_xychart_render_model_includes_display_policy_from_effective_config() {
    let engine = Engine::new();
    let input = r#"%%{init: {"xyChart": {"showTitle": false, "showDataLabel": true, "showDataLabelOutsideBar": true, "xAxis": {"showLabel": false, "showTitle": false, "showTick": false, "showAxisLine": false}, "yAxis": {"showLabel": false, "showTitle": false, "showTick": false, "showAxisLine": false}}}}%%
xychart
x-axis "X Axis" [Alpha, Beta]
y-axis "Y Axis" 1 --> 5
bar "Series 1" [1, 2]
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    let typed_json = match parsed.model() {
        RenderSemanticModel::XyChart(model) => {
            assert!(!model.display.show_title);
            assert!(model.display.show_data_label);
            assert!(model.display.show_data_label_outside_bar);
            assert!(!model.display.x_axis.show_label);
            assert!(!model.display.x_axis.show_title);
            assert!(!model.display.x_axis.show_tick);
            assert!(!model.display.x_axis.show_axis_line);
            assert!(!model.display.y_axis.show_label);
            assert!(!model.display.y_axis.show_title);
            assert!(!model.display.y_axis.show_tick);
            assert!(!model.display.y_axis.show_axis_line);
            model.to_compat_json(parsed.metadata())
        }
        other => panic!("xychart render parse should return typed model, got {other:?}"),
    };

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert!(parsed_json.model.get("display").is_none());
    assert_eq!(typed_json, parsed_json.model);
}

#[test]
fn parse_xychart_exposes_11_15_data_label_outside_default_and_override() {
    let engine = Engine::new();
    let default = block_on(engine.parse_metadata("xychart\nbar [1]")).unwrap();
    let xychart = &default.effective_config.as_value()["xyChart"];
    assert_eq!(xychart["showDataLabel"], json!(false));
    assert_eq!(xychart["showDataLabelOutsideBar"], json!(false));

    let configured = block_on(engine.parse_metadata(
        r#"%%{init: {"xyChart": {"showDataLabel": true, "showDataLabelOutsideBar": true}}}%%
xychart
bar [1]"#,
    ))
    .unwrap();
    let xychart = &configured.effective_config.as_value()["xyChart"];
    assert_eq!(xychart["showDataLabel"], json!(true));
    assert_eq!(xychart["showDataLabelOutsideBar"], json!(true));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_xychart_editor_facts_expose_parser_backed_spans() {
    let engine = Engine::new();
    let text = r#"
xychart horizontal
title "Typed XYChart"
accTitle: XY accTitle
accDescr: XY accDescription
x-axis "X Axis" [Alpha, Beta]
y-axis "Y Axis" 1 --> 5
bar "Series 1" [1, 2]
line "Series 2" [2, 3]
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("xychart", text)
        .unwrap()
        .unwrap();

    assert!(facts.directive_prefixes.iter().any(|p| p == "title"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "accTitle"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "accDescr"));
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Typed XYChart"
                && symbol.kind == EditorSemanticKind::String)
    );
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "X Axis" && symbol.kind == EditorSemanticKind::String)
    );
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Series 1" && symbol.kind == EditorSemanticKind::String)
    );

    let title_start = text.find("Typed XYChart").unwrap();
    let axis_start = text.find("X Axis").unwrap();
    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::Payload
            && expected.span == SourceSpan::new(title_start, title_start + "Typed XYChart".len())
    }));
    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::Payload
            && expected.span == SourceSpan::new(axis_start, axis_start + "X Axis".len())
    }));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_quadrant_chart_editor_facts_expose_parser_backed_spans() {
    let engine = Engine::new();
    let text = r#"
quadrantChart
title "Typed Quadrant"
accTitle: Quadrant accTitle
accDescr: Quadrant accDescription
x-axis Low --> High
y-axis Low --> High
quadrant-1 Expand
quadrant-2 "Maintain"
classDef class1 color: #109060, radius: 10
Point A:::class1: [0.9, 0.0]
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("quadrantChart", text)
        .unwrap()
        .unwrap();

    assert!(facts.directive_prefixes.iter().any(|p| p == "title"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "accTitle"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "accDescr"));
    assert!(facts.symbols.iter().any(
        |symbol| symbol.name == "Typed Quadrant" && symbol.role == EditorSemanticRole::Payload
    ));
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Low" && symbol.role == EditorSemanticRole::Outline)
    );
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Expand" && symbol.role == EditorSemanticRole::Outline)
    );
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "class1" && symbol.kind == EditorSemanticKind::Class)
    );

    let title_start = text.find("Typed Quadrant").unwrap();
    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::Payload
            && expected.span == SourceSpan::new(title_start, title_start + "Typed Quadrant".len())
    }));
    let class_use_start = text.find("Point A:::class1").unwrap() + "Point A:::".len();
    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::ClassName
            && expected.span == SourceSpan::new(class_use_start, class_use_start + "class1".len())
    }));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_quadrant_chart_class_selectors_expose_partial_and_empty_slots() {
    let engine = Engine::new();
    let text = concat!(
        "quadrantChart\n",
        "classDef priority color: #109060\n",
        "Complete:::priority: [0.2, 0.8]\n",
        "Partial:::pri: [0.3, 0.7]\n",
        "Empty:::: [0.4, 0.6]\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("quadrantChart", text)
        .unwrap()
        .unwrap();

    for (line, selector) in [("Complete", "priority"), ("Partial", "pri")] {
        let start = text.find(&format!("{line}:::{selector}")).unwrap() + line.len() + 3;
        assert!(
            facts.expected_syntax.iter().any(|expected| {
                expected.kind == EditorExpectedSyntaxKind::ClassName
                    && expected.span == SourceSpan::new(start, start + selector.len())
            }),
            "missing ClassName evidence for {selector} at {start}: {:?}",
            facts.expected_syntax
        );
    }
    let empty = text.find("Empty:::").unwrap() + "Empty:::".len();
    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::ClassName
            && expected.span == SourceSpan::new(empty, empty)
    }));
}

#[test]
fn parse_class_exposes_11_15_hierarchical_namespaces_default_and_override() {
    let engine = Engine::new();
    let default = block_on(engine.parse_metadata("classDiagram\nclass A")).unwrap();
    let class = &default.effective_config.as_value()["class"];
    assert_eq!(class["hierarchicalNamespaces"], json!(true));

    let configured = block_on(engine.parse_metadata(
        r#"%%{init: {"class": {"hierarchicalNamespaces": false}}}%%
classDiagram
class A"#,
    ))
    .unwrap();
    let class = &configured.effective_config.as_value()["class"];
    assert_eq!(class["hierarchicalNamespaces"], json!(false));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_requirement_editor_facts_expose_parser_backed_spans() {
    let engine = Engine::new();
    let text = r#"
requirementDiagram
accTitle: Requirement accTitle
accDescr: Requirement accDescription
requirement test_req {
  id: test_id
  text: the test text.
  risk: high
  verifymethod: analysis
}
element test_el {
  type: test_type
  docref: test_ref
}
a - contains -> b
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("requirement", text)
        .unwrap()
        .unwrap();

    assert!(facts.directive_prefixes.iter().any(|p| p == "accTitle"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "accDescr"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "test_req"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "test_el"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "a"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "contains"));

    let id_start = text.find("test_id").unwrap();
    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::Payload
            && expected.span == SourceSpan::new(id_start, id_start + "test_id".len())
    }));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_requirement_editor_facts_emit_multi_id_style_class_and_classdef_symbols() {
    let engine = Engine::new();
    let text = r#"
requirementDiagram
requirement req {
  id: REQ-1
  text: demo
  risk: low
  verifymethod: test
}
element elem {
  type: service
}
classDef firstClass,secondClass fill:#f9f,stroke:#333
class req,elem firstClass,secondClass
style req,elem fill:#ffa,stroke:#000
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("requirement", text)
        .unwrap()
        .expect("requirement editor facts");

    let symbol_at = |name: &str, detail: &str, start: usize| {
        facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == name
                    && symbol.detail.as_deref() == Some(detail)
                    && symbol.selection.start == start
            })
            .unwrap_or_else(|| panic!("missing symbol {name} with detail {detail} at {start}"))
    };

    let class_def_start = text.find("classDef firstClass,secondClass").unwrap();
    let first_class_def_start = class_def_start + "classDef ".len();
    let second_class_def_start = first_class_def_start + "firstClass,".len();
    assert_eq!(
        symbol_at(
            "firstClass",
            "requirement class definition",
            first_class_def_start,
        )
        .role,
        EditorSemanticRole::ClassDefinition
    );
    assert_eq!(
        symbol_at(
            "secondClass",
            "requirement class definition",
            second_class_def_start,
        )
        .role,
        EditorSemanticRole::ClassDefinition
    );

    let class_stmt_start = text.find("class req,elem").unwrap();
    let req_class_target_start = class_stmt_start + "class ".len();
    let elem_class_target_start = req_class_target_start + "req,".len();
    assert_eq!(
        symbol_at("req", "requirement class target", req_class_target_start).role,
        EditorSemanticRole::Reference
    );
    assert_eq!(
        symbol_at("elem", "requirement class target", elem_class_target_start).role,
        EditorSemanticRole::Reference
    );

    let first_class_ref_start = class_stmt_start + "class req,elem ".len();
    let second_class_ref_start = first_class_ref_start + "firstClass,".len();
    assert_eq!(
        symbol_at("firstClass", "requirement class", first_class_ref_start).role,
        EditorSemanticRole::Payload
    );
    assert_eq!(
        symbol_at("secondClass", "requirement class", second_class_ref_start).role,
        EditorSemanticRole::Payload
    );

    let style_stmt_start = text.find("style req,elem").unwrap();
    let req_style_target_start = style_stmt_start + "style ".len();
    let elem_style_target_start = req_style_target_start + "req,".len();
    assert_eq!(
        symbol_at("req", "requirement style target", req_style_target_start).role,
        EditorSemanticRole::Reference
    );
    assert_eq!(
        symbol_at("elem", "requirement style target", elem_style_target_start).role,
        EditorSemanticRole::Reference
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_requirement_editor_facts_class_refs_use_class_list_spans_for_short_names() {
    let engine = Engine::new();
    let text = r#"
requirementDiagram
requirement req {
}
element elem {
}
class req,elem a,aa
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("requirement", text)
        .unwrap()
        .expect("requirement editor facts");

    let class_stmt_start = text.find("class req,elem").unwrap();
    let a_start = class_stmt_start + "class req,elem ".len();
    let aa_start = a_start + "a,".len();

    for (name, start) in [("a", a_start), ("aa", aa_start)] {
        let symbol = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == name
                    && symbol.detail.as_deref() == Some("requirement class")
                    && symbol.selection.start == start
            })
            .unwrap_or_else(|| panic!("missing class ref {name} at {start}"));
        assert_eq!(symbol.role, EditorSemanticRole::Payload);
    }
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_packet_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
packet
title Typed Packet
accTitle: Packet accTitle
accDescr: Packet accDescription
+8: "byte"
+16: "word"
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "packet");
    match parsed.model() {
        RenderSemanticModel::Packet(model) => {
            assert_eq!(model.title.as_deref(), Some("Typed Packet"));
            assert_eq!(model.acc_title.as_deref(), Some("Packet accTitle"));
            assert_eq!(model.acc_descr.as_deref(), Some("Packet accDescription"));
            assert_eq!(model.packet.len(), 1);
            assert_eq!(model.packet[0].len(), 2);
            assert_eq!(model.packet[0][0].start, 0);
            assert_eq!(model.packet[0][0].end, 7);
            assert_eq!(model.packet[0][0].bits, 8);
            assert_eq!(model.packet[0][0].label, "byte");
        }
        other => panic!("packet render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("packet"));
    assert_eq!(parsed_json.model["title"], json!("Typed Packet"));
    assert_eq!(parsed_json.model["accTitle"], json!("Packet accTitle"));
    assert_eq!(
        parsed_json.model["accDescr"],
        json!("Packet accDescription")
    );
    assert_eq!(parsed_json.model["packet"][0][0]["label"], json!("byte"));
    assert_eq!(parsed_json.model["packet"][0][0]["bits"], json!(8));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_timeline_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
timeline
title Typed Timeline
accTitle: Timeline accTitle
accDescr: Timeline accDescription
section Alpha
Task 1: event 1: event 2
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "timeline");
    match parsed.model() {
        RenderSemanticModel::Timeline(model) => {
            assert_eq!(model.title.as_deref(), Some("Typed Timeline"));
            assert_eq!(model.acc_title.as_deref(), Some("Timeline accTitle"));
            assert_eq!(model.acc_descr.as_deref(), Some("Timeline accDescription"));
            assert_eq!(model.sections.as_slice(), ["Alpha"]);
            assert_eq!(model.tasks.len(), 1);
            assert_eq!(model.tasks[0].id, 0);
            assert_eq!(model.tasks[0].section, "Alpha");
            assert_eq!(model.tasks[0].task_type, "Alpha");
            assert_eq!(model.tasks[0].task, "Task 1");
            assert_eq!(model.tasks[0].events.as_slice(), ["event 1", "event 2"]);
        }
        other => panic!("timeline render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("timeline"));
    assert_eq!(parsed_json.model["title"], json!("Typed Timeline"));
    assert_eq!(parsed_json.model["sections"][0], json!("Alpha"));
    assert_eq!(parsed_json.model["tasks"][0]["id"], json!(0));
    assert_eq!(parsed_json.model["tasks"][0]["type"], json!("Alpha"));
    assert_eq!(parsed_json.model["tasks"][0]["task"], json!("Task 1"));
    assert_eq!(
        parsed_json.model["tasks"][0]["events"],
        json!(["event 1", "event 2"])
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_journey_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
journey
title Typed Journey
accTitle: Journey accTitle
accDescr: Journey accDescription
section Shopping
Get keys: 5: Dad
Drive: bad-score: Dad, Mum
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "journey");
    match parsed.model() {
        RenderSemanticModel::Journey(model) => {
            assert_eq!(model.title.as_deref(), Some("Typed Journey"));
            assert_eq!(model.acc_title.as_deref(), Some("Journey accTitle"));
            assert_eq!(model.acc_descr.as_deref(), Some("Journey accDescription"));
            assert_eq!(model.sections.as_slice(), ["Shopping"]);
            assert_eq!(model.actors.as_slice(), ["Dad", "Mum"]);
            assert_eq!(model.tasks.len(), 2);
            assert_eq!(model.tasks[0].score, 5);
            assert!(!model.tasks[0].score_is_nan);
            assert_eq!(model.tasks[0].people.as_slice(), ["Dad"]);
            assert_eq!(model.tasks[1].task, "Drive");
            assert!(model.tasks[1].score_is_nan);
            assert_eq!(model.tasks[1].people.as_slice(), ["Dad", "Mum"]);
        }
        other => panic!("journey render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("journey"));
    assert_eq!(parsed_json.model["title"], json!("Typed Journey"));
    assert_eq!(parsed_json.model["actors"], json!(["Dad", "Mum"]));
    assert_eq!(parsed_json.model["tasks"][0]["score"], json!(5));
    assert!(parsed_json.model["tasks"][0].get("scoreIsNaN").is_none());
    assert_eq!(parsed_json.model["tasks"][1]["scoreIsNaN"], json!(true));
    assert_eq!(
        parsed_json.model["tasks"][1]["people"],
        json!(["Dad", "Mum"])
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_requirement_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r##"
requirementDiagram
accTitle: Requirement accTitle
accDescr: Requirement accDescription
direction LR
requirement req_login:::critical {
  id: REQ-1
  text: "Login must work"
  risk: high
  verifymethod: test
}
element api {
  type: service
  docRef: docs/api.md
}
class api external
classDef critical fill:#f9f,stroke:#333,color:#111
classDef external stroke:#0f0
req_login - verifies -> api
"##;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "requirement");
    match parsed.model() {
        RenderSemanticModel::Requirement(model) => {
            assert_eq!(model.acc_title.as_deref(), Some("Requirement accTitle"));
            assert_eq!(
                model.acc_descr.as_deref(),
                Some("Requirement accDescription")
            );
            assert_eq!(model.direction, "LR");
            assert_eq!(model.requirements.len(), 1);
            assert_eq!(model.requirements[0].name, "req_login");
            assert_eq!(model.requirements[0].node_type, "Requirement");
            assert_eq!(model.requirements[0].requirement_id, "REQ-1");
            assert_eq!(model.requirements[0].text, "Login must work");
            assert_eq!(model.requirements[0].risk, "High");
            assert_eq!(model.requirements[0].verify_method, "Test");
            assert!(
                model.requirements[0]
                    .classes
                    .iter()
                    .any(|c| c == "critical")
            );
            assert!(
                model.requirements[0]
                    .css_styles
                    .iter()
                    .any(|s| s == "fill:#f9f")
            );
            assert_eq!(model.elements.len(), 1);
            assert_eq!(model.elements[0].name, "api");
            assert_eq!(model.elements[0].element_type, "service");
            assert_eq!(model.elements[0].doc_ref, "docs/api.md");
            assert!(model.elements[0].classes.iter().any(|c| c == "external"));
            assert_eq!(model.relationships.len(), 1);
            assert_eq!(model.relationships[0].rel_type, "verifies");
            assert_eq!(model.relationships[0].src, "req_login");
            assert_eq!(model.relationships[0].dst, "api");
            assert!(model.classes.contains_key("critical"));
            assert!(model.classes.contains_key("external"));
        }
        other => panic!("requirement render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("requirement"));
    assert_eq!(parsed_json.model["accTitle"], json!("Requirement accTitle"));
    assert_eq!(
        parsed_json.model["accDescr"],
        json!("Requirement accDescription")
    );
    assert_eq!(parsed_json.model["direction"], json!("LR"));
    assert_eq!(
        parsed_json.model["requirements"][0]["name"],
        json!("req_login")
    );
    assert_eq!(
        parsed_json.model["requirements"][0]["requirementId"],
        json!("REQ-1")
    );
    assert_eq!(
        parsed_json.model["requirements"][0]["verifyMethod"],
        json!("Test")
    );
    assert_eq!(
        parsed_json.model["elements"][0]["docRef"],
        json!("docs/api.md")
    );
    assert_eq!(
        parsed_json.model["relationships"][0]["type"],
        json!("verifies")
    );
    assert!(parsed_json.model.get("config").is_some());
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_sankey_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
sankey-beta
Source,Target,10
Target,Done,2.5
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "sankey");
    match parsed.model() {
        RenderSemanticModel::Sankey(model) => {
            assert_eq!(model.graph.nodes.len(), 3);
            assert_eq!(model.graph.nodes[0].id, "Source");
            assert_eq!(model.graph.nodes[1].id, "Target");
            assert_eq!(model.graph.nodes[2].id, "Done");
            assert_eq!(model.graph.links.len(), 2);
            assert_eq!(model.graph.links[0].source, "Source");
            assert_eq!(model.graph.links[0].target, "Target");
            assert_eq!(model.graph.links[0].value, json!(10));
            assert_eq!(model.graph.links[1].source, "Target");
            assert_eq!(model.graph.links[1].target, "Done");
            assert_eq!(model.graph.links[1].value, json!(2.5));
        }
        other => panic!("sankey render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("sankey"));
    assert_eq!(
        parsed_json.model["graph"]["nodes"][0]["id"],
        json!("Source")
    );
    assert_eq!(
        parsed_json.model["graph"]["links"][0],
        json!({
            "source": "Source",
            "target": "Target",
            "value": 10,
        })
    );
    assert_eq!(parsed_json.model["graph"]["links"][1]["value"], json!(2.5));
    assert!(parsed_json.model.get("config").is_some());
}

#[test]
fn parse_sankey_exposes_config_defaults_and_overrides() {
    let engine = Engine::new();
    let default = block_on(engine.parse_metadata("sankey\nA,B,1")).unwrap();
    let sankey = &default.effective_config.as_value()["sankey"];
    assert_eq!(sankey["nodeWidth"], json!(10));
    assert_eq!(sankey["nodePadding"], json!(12));
    assert_eq!(sankey["labelStyle"], json!("legacy"));
    assert!(sankey.get("nodeColors").is_none());
    assert_eq!(sankey["useMaxWidth"], json!(true));

    let configured = block_on(engine.parse_metadata(
        r##"%%{init: {"sankey": {"nodeWidth": 24, "nodePadding": 18, "labelStyle": "outlined", "nodeColors": {"A": "#112233"}}}}%%
sankey
A,B,1"##,
    ))
    .unwrap();
    let sankey = &configured.effective_config.as_value()["sankey"];
    assert_eq!(sankey["nodeWidth"], json!(24));
    assert_eq!(sankey["nodePadding"], json!(18));
    assert_eq!(sankey["labelStyle"], json!("outlined"));
    assert_eq!(sankey["nodeColors"]["A"], json!("#112233"));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_radar_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
radar-beta
title Typed Radar
accTitle: Radar accTitle
accDescr: Radar accDescription
axis A["Axis A"], B["Axis B"], C["Axis C"]
curve first["First Curve"]{1,2,3}
curve second{ C: 9, A: 7, B: 8 }
showLegend false
ticks 4
min 1
max 10
graticule polygon
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "radar");
    match parsed.model() {
        RenderSemanticModel::Radar(model) => {
            assert_eq!(model.title.as_deref(), Some("Typed Radar"));
            assert_eq!(model.acc_title.as_deref(), Some("Radar accTitle"));
            assert_eq!(model.acc_descr.as_deref(), Some("Radar accDescription"));
            assert_eq!(model.axes.len(), 3);
            assert_eq!(model.axes[0].name, "A");
            assert_eq!(model.axes[0].label, "Axis A");
            assert_eq!(model.curves.len(), 2);
            assert_eq!(model.curves[0].label, "First Curve");
            assert_eq!(model.curves[0].entries, vec![json!(1), json!(2), json!(3)]);
            assert_eq!(model.curves[1].entries, vec![json!(7), json!(8), json!(9)]);
            assert!(!model.options.show_legend);
            assert_eq!(model.options.ticks, json!(4));
            assert_eq!(model.options.min, json!(1));
            assert_eq!(model.options.max, Some(json!(10)));
            assert_eq!(model.options.graticule, "polygon");
        }
        other => panic!("radar render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("radar"));
    assert_eq!(parsed_json.model["title"], json!("Typed Radar"));
    assert_eq!(parsed_json.model["accTitle"], json!("Radar accTitle"));
    assert_eq!(
        parsed_json.model["axes"][0],
        json!({"name": "A", "label": "Axis A"})
    );
    assert_eq!(
        parsed_json.model["curves"][1],
        json!({"name": "second", "label": "second", "entries": [7, 8, 9]})
    );
    assert_eq!(
        parsed_json.model["options"],
        json!({
            "showLegend": false,
            "ticks": 4,
            "max": 10,
            "min": 1,
            "graticule": "polygon",
        })
    );
    assert!(parsed_json.model.get("config").is_some());
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_info_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
info
showInfo
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "info");
    match parsed.model() {
        RenderSemanticModel::Info(model) => {
            assert!(model.show_info);
        }
        other => panic!("info render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("info"));
    assert_eq!(parsed_json.model["showInfo"], json!(true));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_er_render_model_uses_typed_variant_without_changing_json_parse() {
    let engine = Engine::new();
    let input = r#"
erDiagram
accTitle: ER accTitle
accDescr: ER accDescription
CUSTOMER ||--o{ ORDER : places
CUSTOMER {
  string id
}
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    assert_eq!(parsed.metadata().diagram_type, "er");
    match parsed.model() {
        RenderSemanticModel::Er(model) => {
            assert_eq!(model.acc_title.as_deref(), Some("ER accTitle"));
            assert_eq!(model.acc_descr.as_deref(), Some("ER accDescription"));
            assert_eq!(model.direction, "TB");
            assert!(model.entities.contains_key("CUSTOMER"));
            assert!(model.entities.contains_key("ORDER"));
            assert_eq!(model.relationships.len(), 1);
            assert_eq!(model.relationships[0].entity_a, "entity-CUSTOMER-0");
            assert_eq!(model.relationships[0].entity_b, "entity-ORDER-1");
            assert_eq!(model.relationships[0].role_a, "places");
            assert_eq!(model.relationships[0].rel_spec.card_a, "ZERO_OR_MORE");
            assert_eq!(model.relationships[0].rel_spec.card_b, "ONLY_ONE");
            assert_eq!(model.relationships[0].rel_spec.rel_type, "IDENTIFYING");
        }
        other => panic!("er render parse should return typed model, got {other:?}"),
    }

    let parsed_json = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed_json.model["type"], json!("er"));
    assert_eq!(parsed_json.model["accTitle"], json!("ER accTitle"));
    assert_eq!(
        parsed_json.model["relationships"][0]["roleA"],
        json!("places")
    );
    assert!(parsed_json.model.get("constants").is_some());
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_sanitizes_common_db_fields_in_strict_mode() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
title: <script>alert(1)</script><b>t</b>
accTitle: <script>alert(1)</script><b>a</b>
accDescr: <script>alert(1)</script><b>d</b>
Alice->Bob:Hello"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    assert_eq!(res.model["title"], json!("<b>t</b>"));
    assert_eq!(res.model["accTitle"], json!("<b>a</b>"));
    assert_eq!(res.model["accDescr"], json!("<b>d</b>"));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_render_model_sanitizes_common_db_fields_for_typed_families() {
    let engine = Engine::new();
    let flowchart = r#"flowchart TD
accTitle: <script>alert(1)</script><b>a</b>
accDescr { <script>alert(1)</script>line1
    line2
}
A[Start] --> B[End]
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(flowchart, ParseOptions::strict())
        .unwrap()
        .unwrap();

    match parsed.model() {
        RenderSemanticModel::Flowchart(model) => {
            assert_eq!(model.acc_title.as_deref(), Some("<b>a</b>"));
            assert_eq!(model.acc_descr.as_deref(), Some("line1\nline2"));
        }
        other => panic!("flowchart render parse should return typed model, got {other:?}"),
    }

    let sequence = r#"sequenceDiagram
title: <script>alert(1)</script><b>t</b>
accTitle: <script>alert(1)</script><b>a</b>
accDescr { <script>alert(1)</script>line1
    line2
}
Alice->Bob:Hello"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(sequence, ParseOptions::strict())
        .unwrap()
        .unwrap();

    match parsed.model() {
        RenderSemanticModel::Sequence(model) => {
            assert_eq!(model.title.as_deref(), Some("<b>t</b>"));
            assert_eq!(model.acc_title.as_deref(), Some("<b>a</b>"));
            assert_eq!(model.acc_descr.as_deref(), Some("line1\nline2"));
        }
        other => panic!("sequence render parse should return typed model, got {other:?}"),
    }

    let treemap = r#"treemap
title <script>alert(1)</script><b>t</b>
accTitle: <script>alert(1)</script><b>a</b>
accDescr: <script>alert(1)</script><b>d</b>
"Root": 1
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(treemap, ParseOptions::strict())
        .unwrap()
        .unwrap();

    match parsed.model() {
        RenderSemanticModel::Treemap(model) => {
            assert_eq!(model.title.as_deref(), Some("<b>t</b>"));
            assert_eq!(model.acc_title.as_deref(), Some("<b>a</b>"));
            assert_eq!(model.acc_descr.as_deref(), Some("<b>d</b>"));
        }
        other => panic!("treemap render parse should return typed model, got {other:?}"),
    }
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_architecture_applies_upstream_category_population_order() {
    let source = r#"architecture-beta
api:R -- L:join
align row api db
junction join in child
service api(server)[API] in child
service db(database)[DB] in child
group root(cloud)[Root]
group child(cloud)[Child] in root
"#;

    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect("Architecture should accept cross-category forward references")
        .expect("Architecture should be detected");

    let ids = |field: &str| {
        parsed.model[field]
            .as_array()
            .expect("Architecture projection should contain an array")
            .iter()
            .map(|item| {
                item["id"]
                    .as_str()
                    .expect("Architecture item should contain an id")
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("groups"), ["root", "child"]);
    assert_eq!(ids("nodes"), ["api", "db", "join"]);
    assert_eq!(parsed.model["edges"][0]["lhsId"], json!("api"));
    assert_eq!(parsed.model["edges"][0]["rhsId"], json!("join"));
    assert_eq!(
        parsed.model["layoutHints"][0]["members"],
        json!(["api", "db"])
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_architecture_converts_quoted_titles_like_langium() {
    let source = r#"architecture-beta
service api(server)["API \"Gateway\""]
"#;

    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect("quoted Architecture title should parse")
        .expect("Architecture should be detected");

    assert_eq!(
        parsed.model["services"][0]["title"],
        json!("API \"Gateway\"")
    );
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_architecture_rejects_backslash_followed_by_a_physical_newline_in_strings_and_titles() {
    for source in [
        "architecture-beta\nservice api \"icon\\\ntext\"\n",
        "architecture-beta\nservice api [\"title\\\ntext\"]\n",
    ] {
        Engine::new()
            .parse_diagram_sync(source, ParseOptions::strict())
            .expect_err("Langium \\. must not consume a physical newline");
    }
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_architecture_rejects_unterminated_acc_descr_but_recovers_editor_payload() {
    let source = "architecture-beta\naccDescr {\n  Draft description\n";
    let engine = Engine::new();

    let error = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect_err("an unterminated accessibility description must not be successful semantics");
    let Error::DiagramParse { diagnostic, .. } = error else {
        panic!("expected an Architecture parse diagnostic");
    };
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(source.len(), source.len()))
    );

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("architecture", source)
        .expect("editor recovery should remain available")
        .expect("Architecture should expose editor facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "accDescr")
    );
    let payload_start = source.find("Draft description").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.role == EditorSemanticRole::Payload
            && symbol.name == "Draft description"
            && symbol.selection
                == SourceSpan::new(payload_start, payload_start + "Draft description".len())
    }));
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(source.len(), source.len()))
    }));
}

#[test]
#[cfg(feature = "all-diagrams")]
fn parse_architecture_validates_junction_ids_and_parents_with_exact_spans() {
    let cases = [
        (
            "architecture-beta\njunction join\njunction join\n",
            "join",
            "The junction id [join] is already in use by another node",
        ),
        (
            "architecture-beta\njunction join in join\n",
            "join",
            "The junction [join] cannot be placed within itself",
        ),
        (
            "architecture-beta\njunction join in missing\n",
            "missing",
            "The junction [join]'s parent does not exist. Please make sure the parent is created before this junction",
        ),
        (
            "architecture-beta\nservice api\njunction join in api\n",
            "api",
            "The junction [join]'s parent is not a group",
        ),
    ];

    for (source, token, expected_message) in cases {
        let error = Engine::new()
            .parse_diagram_sync(source, ParseOptions::strict())
            .expect_err(expected_message);
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected an Architecture parse diagnostic");
        };
        let start = source.rfind(token).unwrap();
        assert_eq!(diagnostic.message(), expected_message);
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(start, start + token.len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }
}
