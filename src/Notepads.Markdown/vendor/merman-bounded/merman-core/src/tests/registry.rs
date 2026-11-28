use crate::{
    DetectorRegistry, DiagramRegistry, Engine, RenderDiagramRegistry,
    built_in_typed_render_families, diagram_family_capabilities, diagram_header_facts,
    supported_diagrams,
};
use std::collections::BTreeSet;

const PINNED_SEMANTIC_WITHOUT_EDITOR: &[&str] = &["error"];
const PINNED_WITHOUT_SEMANTICS: &[&str] = &[];
const MALFORMED_SOURCE: &str = "not-a-mermaid-diagram\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CharacterizedCapabilities {
    semantic: bool,
    editor: bool,
    combined: bool,
    typed: bool,
}

const COMBINED_CAPABILITIES: CharacterizedCapabilities = CharacterizedCapabilities {
    semantic: true,
    editor: true,
    combined: true,
    typed: true,
};
const ERROR_CAPABILITIES: CharacterizedCapabilities = CharacterizedCapabilities {
    semantic: true,
    editor: false,
    combined: false,
    typed: true,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MalformedContract {
    StrictAcceptsEditorAvailable,
    StrictAcceptsEditorUnavailable,
    StrictRejectsEditorAvailable,
    StrictRejectsEditorUnavailable,
    Unsupported,
}

#[derive(Debug, Clone, Copy)]
struct FamilyCharacterization {
    variant_id: &'static str,
    logical_family: &'static str,
    representative_source: &'static str,
    malformed_source: &'static str,
    capabilities: CharacterizedCapabilities,
    malformed_contract: MalformedContract,
}

macro_rules! combined_family {
    ($variant_id:literal, $logical_family:literal, $representative_source:expr) => {
        FamilyCharacterization {
            variant_id: $variant_id,
            logical_family: $logical_family,
            representative_source: $representative_source,
            malformed_source: MALFORMED_SOURCE,
            capabilities: COMBINED_CAPABILITIES,
            malformed_contract: MalformedContract::StrictRejectsEditorAvailable,
        }
    };
}

// This matrix characterizes the full implementation set for the pinned Mermaid baseline.
// Known identities are independent of the callbacks available in an individual build.
const FAMILY_CHARACTERIZATION_MATRIX: &[FamilyCharacterization] = &[
    FamilyCharacterization {
        variant_id: "error",
        logical_family: "error",
        representative_source: "error\n",
        malformed_source: MALFORMED_SOURCE,
        capabilities: ERROR_CAPABILITIES,
        malformed_contract: MalformedContract::StrictAcceptsEditorUnavailable,
    },
    combined_family!("flowchart-elk", "flowchart", "flowchart-elk TD\nA-->B\n"),
    combined_family!("flowchart-v2", "flowchart", "flowchart TD\nA-->B\n"),
    combined_family!("flowchart", "flowchart", "graph TD\nA-->B\n"),
    combined_family!("swimlane", "swimlane", "swimlane-beta LR\nA-->B\n"),
    combined_family!("mindmap", "mindmap", "mindmap\n  root\n    child\n"),
    combined_family!(
        "architecture",
        "architecture",
        "architecture-beta\n  service api(server)[API]\n"
    ),
    combined_family!("zenuml", "zenuml", "zenuml\n  Alice->Bob: Hello\n"),
    combined_family!(
        "sequence",
        "sequence",
        "sequenceDiagram\nAlice->>Bob: Hello\n"
    ),
    combined_family!("c4", "c4", "C4Context\nPerson(user, \"User\")\n"),
    combined_family!("kanban", "kanban", "kanban\n  Todo\n    item1\n"),
    combined_family!("classDiagram", "class", "classDiagram\nclass Animal\n"),
    combined_family!("class", "class", "classDiagram\nclass Animal\n"),
    combined_family!("er", "er", "erDiagram\nCUSTOMER\n"),
    combined_family!("erDiagram", "er", "erDiagram\nCUSTOMER\n"),
    combined_family!(
        "gantt",
        "gantt",
        "gantt\ndateFormat YYYY-MM-DD\nsection Work\nTask :a, 2024-01-01, 1d\n"
    ),
    combined_family!("info", "info", "info\n"),
    combined_family!("pie", "pie", "pie\n\"A\": 1\n"),
    combined_family!(
        "requirement",
        "requirement",
        "requirementDiagram\nrequirement req1 {\n  id: 1\n  text: Test\n  risk: low\n  verifymethod: analysis\n}\n"
    ),
    combined_family!("timeline", "timeline", "timeline\n2024 : Event\n"),
    combined_family!("gitGraph", "gitGraph", "gitGraph\ncommit id:\"first\"\n"),
    combined_family!("stateDiagram", "state", "stateDiagram-v2\n[*] --> Idle\n"),
    combined_family!("state", "state", "stateDiagram\n[*] --> Idle\n"),
    combined_family!("journey", "journey", "journey\nsection Work\nTask: 5\n"),
    combined_family!(
        "quadrantChart",
        "quadrantChart",
        "quadrantChart\nx-axis Low --> High\ny-axis Low --> High\nA: [0.5, 0.5]\n"
    ),
    combined_family!("sankey", "sankey", "sankey\nA,B,1\n"),
    combined_family!("packet", "packet", "packet-beta\n0-7: \"A\"\n"),
    combined_family!("xychart", "xychart", "xychart-beta\nline [10, 30, 20]\n"),
    combined_family!("block", "block", "block\n  a b c\n"),
    combined_family!(
        "eventmodeling",
        "eventmodeling",
        "eventmodeling\ntf 01 ui Shop.Cart\n"
    ),
    combined_family!("treeView", "treeView", "treeView-beta\n  root\n    child\n"),
    combined_family!(
        "radar",
        "radar",
        "radar-beta\naxis A,B,C\ncurve sample{1,2,3}\n"
    ),
    combined_family!(
        "ishikawa",
        "ishikawa",
        "ishikawa-beta\n  Effect\n    Cause\n"
    ),
    combined_family!(
        "treemap",
        "treemap",
        "treemap-beta\n\"Root\"\n  \"Child\": 1\n"
    ),
    combined_family!(
        "railroad",
        "railroad",
        "railroad-beta\nrule = terminal(\"a\") ;\n"
    ),
    combined_family!(
        "railroadEbnf",
        "railroad",
        "railroad-ebnf-beta\nrule = \"a\" ;\n"
    ),
    combined_family!(
        "railroadAbnf",
        "railroad",
        "railroad-abnf-beta\nrule = \"a\" ;\n"
    ),
    combined_family!(
        "railroadPeg",
        "railroad",
        "railroad-peg-beta\nrule <- \"a\" ;\n"
    ),
    combined_family!(
        "venn",
        "venn",
        "venn-beta\nset Frontend\nset Backend\nunion Frontend,Backend[\"API\"]\n"
    ),
    combined_family!(
        "wardley",
        "wardley",
        "wardley-beta\ncomponent API [0.6, 0.7]\n"
    ),
    combined_family!("cynefin", "cynefin", "cynefin-beta\n  complex\n"),
    combined_family!("agentflow", "agentflow", "agentflow-beta\nA --> B\n"),
    combined_family!(
        "usecase",
        "usecase",
        "usecase-beta\nactor Customer(\"Customer\")\nCheckout(\"Place order\")\nCustomer --> Checkout\n"
    ),
];

#[test]
fn canonical_characterization_matrix_covers_every_variant_and_logical_family() {
    let capabilities = diagram_family_capabilities();
    assert_eq!(FAMILY_CHARACTERIZATION_MATRIX.len(), 43);
    assert_eq!(capabilities.len(), 43, "pinned Mermaid 12 catalog drift");

    let expected_ids = FAMILY_CHARACTERIZATION_MATRIX
        .iter()
        .map(|row| row.variant_id)
        .collect::<BTreeSet<_>>();
    let actual_ids = capabilities
        .iter()
        .map(|fact| fact.diagram_type)
        .collect::<BTreeSet<_>>();
    let logical_families = FAMILY_CHARACTERIZATION_MATRIX
        .iter()
        .map(|row| row.logical_family)
        .collect::<BTreeSet<_>>();
    assert_eq!(expected_ids.len(), 43, "matrix variant ids must be unique");
    assert_eq!(
        logical_families.len(),
        35,
        "matrix logical families drifted"
    );
    assert_eq!(
        actual_ids, expected_ids,
        "canonical catalog admission drift"
    );

    for row in FAMILY_CHARACTERIZATION_MATRIX {
        let fact = capabilities
            .iter()
            .find(|fact| fact.diagram_type == row.variant_id)
            .unwrap_or_else(|| panic!("missing capability for {}", row.variant_id));
        assert_eq!(
            fact.logical_family_kind, row.logical_family,
            "{} logical family",
            row.variant_id
        );
        assert_eq!(
            CharacterizedCapabilities {
                semantic: fact.has_semantic_parser,
                editor: fact.has_editor_parser,
                combined: fact.has_combined_parser,
                typed: fact.has_render_parser,
            },
            row.capabilities,
            "{} capability contract",
            row.variant_id
        );
    }
}

#[test]
fn canonical_catalog_has_no_undeclared_semantic_or_editor_capability_gaps() {
    let capabilities = diagram_family_capabilities();
    let semantic_without_editor = capabilities
        .iter()
        .filter_map(|fact| {
            (fact.has_semantic_parser && !fact.has_editor_parser).then_some(fact.diagram_type)
        })
        .collect::<BTreeSet<_>>();
    let without_semantics = capabilities
        .iter()
        .filter_map(|fact| (!fact.has_semantic_parser).then_some(fact.diagram_type))
        .collect::<BTreeSet<_>>();

    assert_eq!(
        semantic_without_editor,
        PINNED_SEMANTIC_WITHOUT_EDITOR.iter().copied().collect(),
        "canonical catalog introduced an undeclared semantic/editor capability gap"
    );
    assert_eq!(
        without_semantics,
        PINNED_WITHOUT_SEMANTICS.iter().copied().collect(),
        "canonical catalog introduced an undeclared semantic admission gap"
    );
}

#[test]
fn canonical_characterization_matrix_executes_representative_and_malformed_contracts() {
    let engine = Engine::new();
    let mut malformed_contract_mismatches = Vec::new();
    let mut recovery_contract_mismatches = Vec::new();

    for row in FAMILY_CHARACTERIZATION_MATRIX {
        if row.capabilities.semantic {
            let parsed = engine
                .parse_diagram_with_type_sync(
                    row.variant_id,
                    row.representative_source,
                    crate::ParseOptions::strict(),
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} representative semantic parse failed: {error}",
                        row.variant_id
                    )
                })
                .unwrap_or_else(|| panic!("{} returned no semantic model", row.variant_id));
            assert_eq!(parsed.meta.diagram_type, row.variant_id);
        }

        if row.capabilities.editor {
            assert!(
                engine
                    .parse_editor_semantic_facts_with_type_sync(
                        row.variant_id,
                        row.representative_source,
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{} representative editor parse failed: {error}",
                            row.variant_id
                        )
                    })
                    .is_some(),
                "{} returned no editor facts",
                row.variant_id
            );
        }

        if row.capabilities.typed {
            let parsed = engine
                .parse_diagram_for_render_model_with_type_sync(
                    row.variant_id,
                    row.representative_source,
                    crate::ParseOptions::strict(),
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} representative typed parse failed: {error}",
                        row.variant_id
                    )
                })
                .unwrap_or_else(|| panic!("{} returned no typed model", row.variant_id));
            assert_eq!(parsed.metadata().diagram_type, row.variant_id);
        }

        if row.capabilities.combined {
            let parsed = engine
                .parse_diagram_snapshot_sync(row.representative_source)
                .unwrap_or_else(|error| {
                    panic!(
                        "{} representative combined parse failed: {error}",
                        row.variant_id
                    )
                })
                .unwrap_or_else(|| panic!("{} returned no combined model", row.variant_id));
            assert_eq!(
                crate::diagram_type_family_kind(&parsed.metadata().diagram_type),
                Some(row.logical_family),
                "{} combined detection left its logical family",
                row.variant_id
            );
            assert!(matches!(
                parsed.editor_facts(),
                crate::ParsedEditorFacts::Available(_)
            ));
        }

        let malformed_semantic = engine.parse_diagram_with_type_sync(
            row.variant_id,
            row.malformed_source,
            crate::ParseOptions::strict(),
        );
        let malformed_editor =
            engine.parse_editor_semantic_facts_with_type_sync(row.variant_id, row.malformed_source);

        if row.capabilities.combined {
            let snapshot = engine
                .parse_diagram_snapshot_with_type_sync(row.variant_id, row.malformed_source)
                .unwrap_or_else(|error| {
                    panic!(
                        "{} malformed snapshot operation failed: {error}",
                        row.variant_id
                    )
                })
                .unwrap_or_else(|| panic!("{} malformed snapshot was absent", row.variant_id));
            let outcome_matches_semantic = matches!(
                (&malformed_semantic, snapshot.outcome()),
                (Ok(Some(_)), crate::DiagramParseOutcome::Parsed { .. })
                    | (Err(_), crate::DiagramParseOutcome::Failed(_))
            );
            if !outcome_matches_semantic {
                recovery_contract_mismatches.push(format!(
                    "{} malformed snapshot outcome drifted from strict semantics",
                    row.variant_id
                ));
            }
            if !matches!(
                snapshot.editor_facts(),
                crate::ParsedEditorFacts::Available(_)
            ) {
                recovery_contract_mismatches.push(format!(
                    "{} malformed snapshot omitted parser-backed editor facts",
                    row.variant_id
                ));
            }
        }

        if let (Err(_), Ok(Some(facts))) = (&malformed_semantic, &malformed_editor) {
            if facts.completeness != crate::EditorSemanticCompleteness::Recovered {
                recovery_contract_mismatches.push(format!(
                    "{} strict failure returned {:?} editor semantics",
                    row.variant_id, facts.completeness
                ));
            }
            if !facts.diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                    && diagnostic.span.is_some()
            }) {
                recovery_contract_mismatches.push(format!(
                    "{} strict failure returned editor recovery without a source-backed parser diagnostic: {:?}",
                    row.variant_id, facts.diagnostics
                ));
            }
        }

        let observed_contract = match (&malformed_semantic, &malformed_editor) {
            (Ok(Some(_)), Ok(Some(_))) => MalformedContract::StrictAcceptsEditorAvailable,
            (Ok(Some(_)), Ok(None)) => MalformedContract::StrictAcceptsEditorUnavailable,
            (Err(crate::Error::UnsupportedDiagram { .. }), Ok(None))
            | (
                Err(crate::Error::UnsupportedDiagram { .. }),
                Err(crate::Error::UnsupportedDiagram { .. }),
            ) => MalformedContract::Unsupported,
            (Err(_), Ok(Some(_))) => MalformedContract::StrictRejectsEditorAvailable,
            (Err(_), Ok(None)) => MalformedContract::StrictRejectsEditorUnavailable,
            _ => panic!(
                "{} exposed an unclassified malformed contract: semantic={malformed_semantic:?}, editor={malformed_editor:?}",
                row.variant_id
            ),
        };
        if observed_contract != row.malformed_contract {
            malformed_contract_mismatches.push(format!(
                "{}: expected {:?}, observed {:?}",
                row.variant_id, row.malformed_contract, observed_contract
            ));
        }
    }

    assert!(
        malformed_contract_mismatches.is_empty(),
        "malformed contracts drifted:\n{}",
        malformed_contract_mismatches.join("\n")
    );
    assert!(
        recovery_contract_mismatches.is_empty(),
        "malformed recovery contracts drifted:\n{}",
        recovery_contract_mismatches.join("\n")
    );
}

fn ids(values: impl IntoIterator<Item = &'static str>) -> BTreeSet<&'static str> {
    values.into_iter().collect()
}

#[test]
fn pinned_baseline_uses_one_catalog_for_all_registry_projections() {
    let detector_ids = ids(DetectorRegistry::pinned_mermaid_baseline()
        .detector_ids()
        .collect::<Vec<_>>());
    let semantic_ids = ids(DiagramRegistry::pinned_mermaid_baseline()
        .parser_ids()
        .collect::<Vec<_>>());
    let render_ids = ids(RenderDiagramRegistry::pinned_mermaid_baseline()
        .parser_ids()
        .collect::<Vec<_>>());
    let combined_ids = ids(crate::family::combined_parser_facts()
        .iter()
        .map(|fact| fact.id));
    let header_ids = ids(diagram_header_facts().iter().map(|fact| fact.diagram_type));

    let mut expected_detector_ids = ids(crate::family::detector_facts().iter().map(|fact| fact.id));
    // Front-matter enters the detector registry as a protocol adapter, not as a Mermaid family.
    expected_detector_ids.insert("---");
    assert_eq!(detector_ids, expected_detector_ids);
    assert_eq!(
        semantic_ids,
        ids(crate::family::semantic_parser_facts()
            .iter()
            .map(|fact| fact.id))
    );
    assert_eq!(
        render_ids,
        ids(crate::family::render_parser_facts()
            .iter()
            .map(|fact| fact.id))
    );

    for capability in diagram_family_capabilities() {
        let id = capability.diagram_type;
        assert_eq!(capability.has_detector, detector_ids.contains(id), "{id}");
        assert_eq!(
            capability.has_semantic_parser,
            semantic_ids.contains(id),
            "{id}"
        );
        assert_eq!(
            capability.has_editor_parser,
            combined_ids.contains(id),
            "{id}"
        );
        assert_eq!(
            capability.has_combined_parser,
            combined_ids.contains(id),
            "{id}"
        );
        assert_eq!(
            capability.has_render_parser,
            render_ids.contains(id),
            "{id}"
        );
        assert_eq!(capability.has_header, header_ids.contains(id), "{id}");

        if capability.has_editor_parser {
            assert!(
                capability.has_semantic_parser,
                "{id} exposes editor facts without a semantic parser"
            );
        }
        if capability.has_render_parser && id != "error" {
            assert!(
                capability.has_semantic_parser && capability.has_combined_parser,
                "{id} exposes typed rendering without the family-owned semantic construction"
            );
        }
    }

    for header in diagram_header_facts() {
        assert!(
            semantic_ids.contains(header.diagram_type),
            "header {} has no semantic parser",
            header.diagram_type
        );
    }
}

#[test]
fn canonical_catalog_admits_every_mermaid_family() {
    let capabilities = diagram_family_capabilities();
    assert_eq!(capabilities.len(), 43, "pinned Mermaid 12 catalog drift");

    for capability in capabilities
        .iter()
        .filter(|capability| capability.diagram_type != "error")
    {
        assert!(
            capability.has_semantic_parser,
            "{} semantic parser missing",
            capability.diagram_type
        );
        assert!(
            capability.has_editor_parser,
            "{} editor parser missing",
            capability.diagram_type
        );
    }

    for id in ["architecture", "flowchart-elk", "mindmap"] {
        let capability = capabilities
            .iter()
            .find(|fact| fact.diagram_type == id)
            .unwrap_or_else(|| panic!("{id} is missing from the canonical catalog"));
        assert!(capability.has_detector, "{id} detector missing");
        assert!(
            capability.has_semantic_parser,
            "{id} semantic parser missing"
        );
        assert!(capability.has_editor_parser, "{id} editor parser missing");
    }

    let supported = supported_diagrams();
    assert!(supported.contains(&"architecture"));
    assert!(supported.contains(&"mindmap"));
    assert!(supported.contains(&"flowchart"));
}

#[test]
fn canonical_header_facts_preserve_the_pinned_authoring_surface() {
    let labels = diagram_header_facts()
        .iter()
        .map(|fact| fact.label)
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        vec![
            "flowchart TD",
            "graph TD",
            "sequenceDiagram",
            "swimlane-beta",
            "classDiagram",
            "classDiagram-v2",
            "stateDiagram-v2",
            "stateDiagram",
            "erDiagram",
            "gantt",
            "mindmap",
            "info",
            "journey",
            "timeline",
            "gitGraph",
            "pie",
            "requirementDiagram",
            "sankey",
            "packet",
            "packet-beta",
            "xychart",
            "xychart-beta",
            "treeView-beta",
            "ishikawa-beta",
            "eventmodeling",
            "quadrantChart",
            "venn-beta",
            "zenuml",
            "C4Context",
            "C4Container",
            "C4Component",
            "C4Dynamic",
            "C4Deployment",
            "kanban",
            "architecture-beta",
            "block-beta",
            "radar-beta",
            "treemap-beta",
            "railroad-beta",
            "railroad-ebnf-beta",
            "railroad-abnf-beta",
            "railroad-peg-beta",
            "wardley-beta",
            "cynefin-beta",
            "flowchart-elk TD",
            "usecase-beta",
            "agentflow-beta",
        ]
    );
    for header in diagram_header_facts() {
        assert!(
            diagram_family_capabilities().iter().any(|capability| {
                capability.diagram_type == header.diagram_type && capability.has_semantic_parser
            }),
            "header {} must be backed by a semantic parser",
            header.label
        );
    }
}

#[test]
fn canonical_supported_diagrams_are_backed_by_typed_render_parsers() {
    assert_eq!(
        supported_diagrams(),
        &[
            "architecture",
            "block",
            "c4",
            "class",
            "cynefin",
            "er",
            "eventmodeling",
            "flowchart",
            "gantt",
            "gitgraph",
            "info",
            "ishikawa",
            "journey",
            "kanban",
            "mindmap",
            "packet",
            "pie",
            "quadrantchart",
            "radar",
            "railroad",
            "railroadAbnf",
            "railroadEbnf",
            "railroadPeg",
            "requirement",
            "sankey",
            "sequence",
            "state",
            "swimlane",
            "timeline",
            "treeView",
            "treemap",
            "venn",
            "wardley",
            "xychart",
            "zenuml",
            "agentflow",
            "usecase",
        ]
    );

    let render_ids = ids(RenderDiagramRegistry::pinned_mermaid_baseline()
        .parser_ids()
        .collect::<Vec<_>>());
    for capability in diagram_family_capabilities() {
        if let Some(metadata_id) = capability.metadata_id {
            assert!(
                supported_diagrams().contains(&metadata_id),
                "{metadata_id} has metadata but is not public"
            );
            assert!(
                capability.has_render_parser && render_ids.contains(capability.diagram_type),
                "{metadata_id} metadata is not backed by a typed render parser"
            );
        }
    }
}

#[test]
fn built_in_typed_render_family_catalog_is_canonical_and_concrete() {
    let families = built_in_typed_render_families();
    let diagram_types = families
        .iter()
        .map(|family| family.diagram_type)
        .collect::<Vec<_>>();
    let model_kinds = families
        .iter()
        .map(|family| family.render_model_kind)
        .collect::<BTreeSet<_>>();

    assert_eq!(families.len(), 33);
    assert_eq!(model_kinds.len(), families.len());
    assert_eq!(
        diagram_types,
        vec![
            "architecture",
            "block",
            "c4",
            "class",
            "cynefin",
            "er",
            "eventmodeling",
            "flowchart",
            "gantt",
            "gitgraph",
            "info",
            "ishikawa",
            "journey",
            "kanban",
            "mindmap",
            "packet",
            "pie",
            "quadrantchart",
            "radar",
            "railroad",
            "requirement",
            "sankey",
            "sequence",
            "state",
            "timeline",
            "treeView",
            "treemap",
            "venn",
            "wardley",
            "xychart",
            "zenuml",
            "agentflow",
            "usecase",
        ]
    );
    assert!(
        families
            .iter()
            .all(|family| family.render_model_kind != "error")
    );
}

#[test]
fn empty_registries_are_explicit_overlay_starting_points() {
    assert_eq!(DetectorRegistry::new().detector_ids().count(), 0);
    assert_eq!(DiagramRegistry::new().parser_ids().count(), 0);
    assert_eq!(RenderDiagramRegistry::new().parser_ids().count(), 0);
}

#[test]
fn engine_uses_the_canonical_catalog_regardless_of_host_features() {
    let engine = Engine::new();
    for (source, expected) in [
        ("mindmap\n  root", "mindmap"),
        (
            "architecture-beta\n  service api(server)[API]",
            "architecture",
        ),
        ("flowchart-elk TD\n  A-->B", "flowchart-elk"),
    ] {
        let metadata = engine
            .parse_metadata_sync(source)
            .unwrap_or_else(|error| panic!("{expected} detection failed: {error}"));
        assert_eq!(metadata.diagram_type, expected);

        let snapshot = engine
            .parse_diagram_snapshot_sync(source)
            .unwrap_or_else(|error| panic!("{expected} semantic construction failed: {error}"))
            .unwrap_or_else(|| panic!("{expected} produced no diagram snapshot"));
        assert!(
            matches!(
                snapshot.editor_facts(),
                crate::ParsedEditorFacts::Available(_)
            ),
            "{expected} must keep parser-backed editor facts in the canonical catalog"
        );
    }
}

fn assert_snapshot_has_editor_facts(source: &str, family: &str) {
    let parsed = Engine::new()
        .parse_diagram_snapshot_sync(source)
        .unwrap_or_else(|error| panic!("{family} combined parse failed: {error}"))
        .unwrap_or_else(|| panic!("{family} combined parse returned no diagram"));
    assert!(matches!(
        parsed.editor_facts(),
        crate::ParsedEditorFacts::Available(_)
    ));
}

#[test]
fn langium_family_combined_parse_constructs_syntax_once() {
    for (family, source) in [
        ("info", "info\n"),
        ("pie", "pie\n\"A\": 1\n"),
        ("packet", "packet-beta\n0-7: \"A\"\n"),
        ("cynefin", "cynefin-beta\n  complex\n"),
        ("radar", "radar-beta\naxis A,B,C\ncurve sample{1,2,3}\n"),
        ("wardley", "wardley-beta\ncomponent API [0.6, 0.7]\n"),
    ] {
        crate::diagrams::langium_common::reset_family_syntax_construction_count(family);
        assert_snapshot_has_editor_facts(source, family);
        assert_eq!(
            crate::diagrams::langium_common::family_syntax_construction_count(family),
            1,
            "one combined request must construct {family} syntax once"
        );
    }
}

#[test]
fn git_graph_combined_parse_constructs_syntax_once() {
    let family = "gitGraph";
    crate::diagrams::langium_common::reset_family_syntax_construction_count(family);
    assert_snapshot_has_editor_facts("gitGraph\ncommit\n", family);
    assert_eq!(
        crate::diagrams::langium_common::family_syntax_construction_count(family),
        1,
        "one combined request must construct gitGraph syntax once"
    );
}

#[test]
fn er_combined_parse_constructs_family_syntax_once() {
    crate::diagrams::er::reset_er_syntax_construction_count();
    assert_snapshot_has_editor_facts("erDiagram\nCUSTOMER ||--o{ ORDER : places\n", "er");
    assert_eq!(
        crate::diagrams::er::er_syntax_construction_count(),
        1,
        "one combined request must construct ER syntax once"
    );
}

#[test]
fn sequence_combined_parse_constructs_family_syntax_once() {
    crate::diagrams::sequence::reset_sequence_syntax_construction_count();
    assert_snapshot_has_editor_facts("sequenceDiagram\nAlice->>Bob: Hello\n", "sequence");
    assert_eq!(
        crate::diagrams::sequence::sequence_syntax_construction_count(),
        1,
        "one combined request must construct Sequence syntax once"
    );
}

#[test]
fn class_combined_parse_constructs_family_syntax_once() {
    crate::diagrams::class::reset_class_syntax_construction_count();
    assert_snapshot_has_editor_facts(
        "classDiagram-v2\nclass Customer\nCustomer --> Order : places\n",
        "class",
    );
    assert_eq!(
        crate::diagrams::class::class_syntax_construction_count(),
        1,
        "one combined request must construct Class syntax once"
    );
}

#[test]
fn mindmap_combined_parse_constructs_family_syntax_once() {
    crate::diagrams::mindmap::reset_mindmap_syntax_construction_count();
    assert_snapshot_has_editor_facts("mindmap\n  root\n    child\n", "mindmap");
    assert_eq!(
        crate::diagrams::mindmap::mindmap_syntax_construction_count(),
        1,
        "one combined request must construct Mindmap syntax once"
    );
}

#[test]
fn railroad_combined_parse_constructs_family_syntax_once_for_every_dialect() {
    for source in [
        "railroad-beta\nrule = terminal(\"a\") ;\n",
        "railroad-ebnf-beta\nrule = \"a\" ;\n",
        "railroad-abnf-beta\nrule = \"a\" ;\n",
        "railroad-peg-beta\nrule <- \"a\" ;\n",
    ] {
        crate::diagrams::railroad::reset_railroad_syntax_construction_count();
        assert_snapshot_has_editor_facts(source, "railroad");
        assert_eq!(
            crate::diagrams::railroad::railroad_syntax_construction_count(),
            1,
            "one combined request must construct Railroad syntax once for {source:?}"
        );
    }
}

#[test]
fn sankey_combined_parse_constructs_family_syntax_once() {
    crate::diagrams::sankey::reset_sankey_syntax_construction_count();
    assert_snapshot_has_editor_facts("sankey-beta\nA,B,1\n", "sankey");
    assert_eq!(
        crate::diagrams::sankey::sankey_syntax_construction_count(),
        1,
        "one combined request must construct Sankey syntax once"
    );
}

fn assert_combined_projections_match_standalone(
    engine: &Engine,
    family: &str,
    source: &str,
    volatile_top_level_json_field: Option<&str>,
) -> serde_json::Value {
    let standalone = engine
        .parse_diagram_sync(source, crate::ParseOptions::strict())
        .unwrap_or_else(|error| panic!("{family} standalone JSON failed: {error}"))
        .unwrap_or_else(|| panic!("{family} standalone JSON returned no diagram"));
    let standalone_editor = engine
        .parse_editor_semantic_facts_with_type_sync(family, source)
        .unwrap_or_else(|error| panic!("{family} standalone editor failed: {error}"))
        .unwrap_or_else(|| panic!("{family} standalone editor returned no facts"));
    let combined = engine
        .parse_diagram_snapshot_sync(source)
        .unwrap_or_else(|error| panic!("{family} combined parse failed: {error}"))
        .unwrap_or_else(|| panic!("{family} combined parse returned no diagram"));

    assert_eq!(standalone.meta.diagram_type, family);
    assert_eq!(combined.metadata().diagram_type, family);

    let mut standalone_model = standalone.model.into_unmanaged_value();
    let mut combined_model = combined
        .outcome()
        .parsed_model()
        .expect("expected parsed snapshot")
        .clone();
    if let Some(field) = volatile_top_level_json_field {
        let standalone_value = standalone_model
            .as_object_mut()
            .and_then(|model| model.remove(field))
            .unwrap_or_else(|| panic!("{family} standalone JSON omitted volatile field {field}"));
        let combined_value = combined_model
            .as_value_mut()
            .as_object_mut()
            .and_then(|model| model.remove(field))
            .unwrap_or_else(|| panic!("{family} combined JSON omitted volatile field {field}"));
        assert!(
            standalone_value.is_string() && combined_value.is_string(),
            "{family} volatile JSON field {field} must remain a string"
        );
    }
    assert_eq!(
        standalone_model, combined_model,
        "{family} JSON projection drift"
    );

    let crate::ParsedEditorFacts::Available(combined_editor) = combined.editor_facts() else {
        panic!("{family} combined parse returned unavailable editor facts");
    };
    assert_eq!(
        &standalone_editor, combined_editor,
        "{family} editor projection drift"
    );

    standalone_model
}

#[test]
fn every_combined_catalog_variant_matches_its_standalone_semantic_and_editor_projections() {
    let engine = Engine::new();

    for row in FAMILY_CHARACTERIZATION_MATRIX
        .iter()
        .filter(|row| row.capabilities.combined)
    {
        let standalone = engine
            .parse_diagram_with_type_sync(
                row.variant_id,
                row.representative_source,
                crate::ParseOptions::strict(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{} standalone semantic parse failed: {error}",
                    row.variant_id
                )
            })
            .unwrap_or_else(|| panic!("{} standalone semantic parse was absent", row.variant_id));
        let standalone_editor = engine
            .parse_editor_semantic_facts_with_type_sync(row.variant_id, row.representative_source)
            .unwrap_or_else(|error| {
                panic!("{} standalone editor parse failed: {error}", row.variant_id)
            })
            .unwrap_or_else(|| panic!("{} standalone editor parse was absent", row.variant_id));
        let combined = engine
            .parse_diagram_snapshot_with_type_sync(row.variant_id, row.representative_source)
            .unwrap_or_else(|error| {
                panic!("{} combined semantic parse failed: {error}", row.variant_id)
            })
            .unwrap_or_else(|| panic!("{} combined semantic parse was absent", row.variant_id));

        assert_eq!(standalone.meta.diagram_type, row.variant_id);
        assert_eq!(combined.metadata().diagram_type, row.variant_id);

        let mut standalone_model = standalone.model.into_unmanaged_value();
        let mut combined_model = combined
            .outcome()
            .parsed_model()
            .expect("combined parse must retain a model")
            .clone();
        if row.variant_id == "mindmap" {
            let standalone_id = standalone_model
                .as_object_mut()
                .and_then(|model| model.remove("diagramId"));
            let combined_id = combined_model
                .as_value_mut()
                .as_object_mut()
                .and_then(|model| model.remove("diagramId"));
            assert!(
                standalone_id.is_some_and(|value| value.is_string())
                    && combined_id.is_some_and(|value| value.is_string()),
                "mindmap diagramId must remain a string projection"
            );
        }
        assert_eq!(
            standalone_model, combined_model,
            "{} JSON projection drift",
            row.variant_id
        );

        let crate::ParsedEditorFacts::Available(combined_editor) = combined.editor_facts() else {
            panic!(
                "{} combined parse returned unavailable editor facts",
                row.variant_id
            );
        };
        assert_eq!(
            &standalone_editor, combined_editor,
            "{} editor projection drift",
            row.variant_id
        );
    }
}

#[test]
fn mindmap_combined_projections_match_standalone_public_entrypoints() {
    let source = concat!(
        "mindmap root(Root Node)\n",
        "  child1(Child 1)\n",
        "  :::hot\n",
        "  ::icon(bomb)\n",
        "  child2\n",
    );
    let model = assert_combined_projections_match_standalone(
        &Engine::new(),
        "mindmap",
        source,
        Some("diagramId"),
    );
    assert_eq!(model["rootNode"]["descr"], "Root Node");
    assert_eq!(model["rootNode"]["children"].as_array().unwrap().len(), 2);
}

#[test]
fn railroad_combined_projections_match_standalone_public_entrypoints() {
    let engine = Engine::new();
    for (family, source) in [
        (
            "railroad",
            "railroad-beta\ntitle \"Grammar\"\nrule = sequence(terminal(\"a\"), nonterminal(\"next\")) ;\n",
        ),
        (
            "railroadEbnf",
            "railroad-ebnf-beta\nrule ::= \"a\" | [ next ] ;\n",
        ),
        (
            "railroadAbnf",
            "railroad-abnf-beta\nrule = 1*2\"a\" / [ next ] ;\n",
        ),
        (
            "railroadPeg",
            "railroad-peg-beta\nrule <- &\"a\" !\"b\" . next? ;\n",
        ),
    ] {
        let model = assert_combined_projections_match_standalone(&engine, family, source, None);
        assert_eq!(model["rules"][0]["name"], "rule");
    }
}

#[test]
fn sankey_combined_projections_match_standalone_public_entrypoints() {
    let source = concat!(
        "sankey-beta\n",
        "\"Source, Inc.\",\"Target \"\"quoted\"\"\",1.5\n",
        "Target,Done,2\n",
    );
    let model =
        assert_combined_projections_match_standalone(&Engine::new(), "sankey", source, None);
    assert_eq!(model["graph"]["links"][0]["source"], "Source, Inc.");
    assert_eq!(model["graph"]["links"][0]["target"], "Target \"quoted\"");
}

#[test]
fn failed_editor_snapshot_runs_one_preprocess_and_one_family_construction() {
    let source = concat!(
        "%%{ initialize: {\"theme\": \"dark\"} }%%\n",
        "mindmap\n",
        " root\n",
        "  broken[unterminated\n",
        "  after\n",
    );
    crate::preprocess::reset_public_parse_preprocess_count();
    crate::diagrams::mindmap::reset_mindmap_syntax_construction_count();

    let snapshot = Engine::new()
        .parse_diagram_snapshot_sync(source)
        .expect("snapshot operation")
        .expect("detected mindmap");

    assert_eq!(snapshot.metadata().diagram_type, "mindmap");
    assert!(matches!(
        snapshot.outcome(),
        crate::DiagramParseOutcome::Failed(_)
    ));
    let crate::ParsedEditorFacts::Available(facts) = snapshot.editor_facts() else {
        panic!("mindmap snapshot must retain parser-backed recovery facts");
    };
    assert_eq!(
        facts.completeness,
        crate::EditorSemanticCompleteness::Recovered
    );
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "initialize")
    );
    assert_eq!(crate::preprocess::public_parse_preprocess_count(), 1);
    assert_eq!(
        crate::diagrams::mindmap::mindmap_syntax_construction_count(),
        1
    );
}

#[test]
fn known_identities_without_registered_parsers_remain_unsupported_instead_of_unknown() {
    let mut engine = Engine::new();
    *engine.diagram_registry_mut() = DiagramRegistry::new();
    *engine.render_diagram_registry_mut() = RenderDiagramRegistry::new();
    let source = "flowchart-elk TD\nA-->B\n";
    let metadata = engine.parse_metadata_sync(source).unwrap();
    assert_eq!(metadata.diagram_type, "flowchart-elk");
    assert_eq!(metadata.effective_config.get_str("layout"), Some("elk"));
    assert!(matches!(
        engine.parse_diagram_sync(source, crate::ParseOptions::strict()),
        Err(crate::Error::UnsupportedDiagram { diagram_type }) if diagram_type == "flowchart-elk"
    ));
    assert!(matches!(
        engine.parse_diagram_for_render_model_sync(source, crate::ParseOptions::strict()),
        Err(crate::Error::UnsupportedDiagram { diagram_type }) if diagram_type == "flowchart-elk"
    ));
    assert!(matches!(
        engine.parse_diagram_sync(MALFORMED_SOURCE, crate::ParseOptions::strict()),
        Err(crate::Error::DetectType(_))
    ));
    let suppressed = engine
        .parse_diagram_sync(source, crate::ParseOptions::lenient())
        .unwrap()
        .unwrap();
    assert_eq!(suppressed.meta.diagram_type, "error");
    assert!(
        engine
            .parse_diagram_sync(MALFORMED_SOURCE, crate::ParseOptions::lenient())
            .unwrap()
            .is_none()
    );

    let control = crate::OperationControl::new();
    control.cancel();
    let cancelled = engine
        .parse_diagram_for_render_model_controlled_sync(
            source,
            crate::ParseOptions::strict(),
            &control,
        )
        .unwrap_err();
    assert_eq!(cancelled.reason, crate::CancelReason::Requested);
}

#[test]
fn custom_overlays_on_known_ids_work_without_builtin_parsers() {
    let mut engine = Engine::new();
    *engine.diagram_registry_mut() = DiagramRegistry::new();
    *engine.render_diagram_registry_mut() = RenderDiagramRegistry::new();
    engine
        .diagram_registry_mut()
        .insert("flowchart-v2", |_, meta, control| {
            control.checkpoint()?;
            Ok(Ok(
                serde_json::json!({ "owner": "semantic", "diagramType": meta.diagram_type }),
            ))
        });
    let source = "flowchart TD\nA-->B\n";
    let parsed = engine
        .parse_diagram_for_render_model_sync(source, crate::ParseOptions::strict())
        .unwrap()
        .unwrap();
    let crate::RenderSemanticModel::CustomJson(model) = parsed.model() else {
        panic!("custom semantic overlay must retain its custom JSON boundary");
    };
    assert_eq!(model.value()["owner"], "semantic");
    assert_eq!(
        model.provenance(),
        crate::CustomJsonProvenance::SemanticRegistryOverlay
    );
    assert!(!parsed.model().supports_diagram_type("flowchart-v2"));
    assert!(
        engine
            .parse_editor_semantic_facts_with_type_sync("flowchart-v2", source)
            .unwrap()
            .is_none()
    );

    engine
        .render_diagram_registry_mut()
        .insert("flowchart-v2", |_, _, control| {
            control.checkpoint()?;
            Ok(Ok(crate::CustomJsonRenderModel::new(
                "flowchart-v2",
                serde_json::json!({ "owner": "render" }),
            )))
        });
    let parsed = engine
        .parse_diagram_for_render_model_sync(source, crate::ParseOptions::strict())
        .unwrap()
        .unwrap();
    let crate::RenderSemanticModel::CustomJson(model) = parsed.model() else {
        panic!("explicit custom render overlay must take precedence");
    };
    assert_eq!(model.value()["owner"], "render");
    assert_eq!(
        model.provenance(),
        crate::CustomJsonProvenance::RenderRegistryOverlay
    );
    assert!(!parsed.model().supports_diagram_type("flowchart-v2"));
}
