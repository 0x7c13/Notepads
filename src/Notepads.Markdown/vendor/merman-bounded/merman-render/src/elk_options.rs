//! Shared Mermaid ELK configuration projection for graph families.
//!
//! Layer assignment keys follow Mermaid 12's `createRootElkGraph`:
//! https://github.com/mermaid-js/mermaid/blob/21f72f07ea22c0af48a3149c550654e80d8e40cb/packages/mermaid/src/rendering-util/layout-algorithms/elk/render.ts

use crate::config::{config_bool, config_string, value_at};
use merman_layout_elk as elk;

// elkjs 0.9.3 serializes JSON numbers before LayoutOptionData parses an INT.
// Strings remain exact: whitespace, fractional spelling, and booleans are invalid.
fn elk_integer(value: &serde_json::Value) -> Option<i32> {
    match value {
        serde_json::Value::String(value) => value.parse().ok(),
        serde_json::Value::Number(value) => value
            .as_f64()
            .filter(|value| {
                value.fract() == 0.0
                    && *value >= f64::from(i32::MIN)
                    && *value <= f64::from(i32::MAX)
            })
            .map(|value| value as i32),
        _ => None,
    }
}

// LayoutOptionData.enumForString uses exact names, then a Java enum ordinal.
// Lists below retain the pinned source order, not the Rust adapter's variant order.
fn elk_enum_name<'a>(value: &serde_json::Value, names: &'a [&str]) -> Option<&'a str> {
    if let Some(name) = value.as_str()
        && let Some(name) = names.iter().copied().find(|candidate| *candidate == name)
    {
        return Some(name);
    }
    let ordinal = usize::try_from(elk_integer(value)?).ok()?;
    names.get(ordinal).copied()
}

fn explicit_option<'a>(
    config: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    value_at(config, path).filter(|value| !value.is_null())
}

/// Mermaid 12.1 reverses compound feedback edges unless explicitly disabled.
pub(crate) fn orient_feedback_edges(effective_config: &serde_json::Value) -> bool {
    config_bool(effective_config, &["elk", "orientFeedbackEdges"]) != Some(false)
}

pub(crate) fn layout_options(effective_config: &serde_json::Value) -> elk::LayoutOptions {
    let (preset_placement, preset_alignment, preset_cycle_breaking) =
        match config_string(effective_config, &["elk", "preset"]).as_deref() {
            Some("legacy") => (
                elk::NodePlacementStrategy::BrandesKoepf,
                elk::NodePlacementAlignment::None,
                elk::CycleBreakingStrategy::Greedy,
            ),
            Some("modelOrder") => (
                elk::NodePlacementStrategy::NetworkSimplex,
                elk::NodePlacementAlignment::None,
                elk::CycleBreakingStrategy::GreedyModelOrder,
            ),
            Some("depthFirst") => (
                elk::NodePlacementStrategy::NetworkSimplex,
                elk::NodePlacementAlignment::None,
                elk::CycleBreakingStrategy::DepthFirst,
            ),
            _ => (
                elk::NodePlacementStrategy::BrandesKoepf,
                elk::NodePlacementAlignment::Balanced,
                elk::CycleBreakingStrategy::DepthFirst,
            ),
        };
    let model_order = explicit_option(effective_config, &["elk", "considerModelOrder"])
        .map(|value| {
            match elk_enum_name(
                value,
                &["NONE", "NODES_AND_EDGES", "PREFER_EDGES", "PREFER_NODES"],
            ) {
                Some("NODES_AND_EDGES") => elk::ModelOrderStrategy::NodesAndEdges,
                Some("PREFER_EDGES") => elk::ModelOrderStrategy::PreferEdges,
                Some("PREFER_NODES") => elk::ModelOrderStrategy::PreferNodes,
                _ => elk::ModelOrderStrategy::None,
            }
        })
        .unwrap_or_default();
    let cycle_breaking = explicit_option(effective_config, &["elk", "cycleBreakingStrategy"])
        .map(|value| {
            match elk_enum_name(
                value,
                &[
                    "GREEDY",
                    "DEPTH_FIRST",
                    "INTERACTIVE",
                    "MODEL_ORDER",
                    "GREEDY_MODEL_ORDER",
                ],
            ) {
                Some("DEPTH_FIRST") => elk::CycleBreakingStrategy::DepthFirst,
                Some("INTERACTIVE") => elk::CycleBreakingStrategy::Interactive,
                Some("MODEL_ORDER") => elk::CycleBreakingStrategy::ModelOrder,
                Some("GREEDY_MODEL_ORDER") => elk::CycleBreakingStrategy::GreedyModelOrder,
                _ => elk::CycleBreakingStrategy::Greedy,
            }
        })
        .unwrap_or(preset_cycle_breaking);
    let explicit_node_placement =
        explicit_option(effective_config, &["elk", "nodePlacementStrategy"]).map(|value| {
            match elk_enum_name(
                value,
                &[
                    "SIMPLE",
                    "INTERACTIVE",
                    "LINEAR_SEGMENTS",
                    "BRANDES_KOEPF",
                    "NETWORK_SIMPLEX",
                ],
            ) {
                Some("SIMPLE") => elk::NodePlacementStrategy::Simple,
                Some("INTERACTIVE") => elk::NodePlacementStrategy::Interactive,
                Some("NETWORK_SIMPLEX") => elk::NodePlacementStrategy::NetworkSimplex,
                Some("LINEAR_SEGMENTS") => elk::NodePlacementStrategy::LinearSegments,
                _ => elk::NodePlacementStrategy::BrandesKoepf,
            }
        });
    let node_placement = explicit_node_placement.unwrap_or(preset_placement);
    let node_placement_alignment =
        explicit_option(effective_config, &["elk", "nodePlacementAlignment"])
            .map(|value| {
                match elk_enum_name(
                    value,
                    &[
                        "NONE",
                        "LEFTUP",
                        "RIGHTUP",
                        "LEFTDOWN",
                        "RIGHTDOWN",
                        "BALANCED",
                    ],
                ) {
                    Some("LEFTUP") => elk::NodePlacementAlignment::LeftUp,
                    Some("LEFTDOWN") => elk::NodePlacementAlignment::LeftDown,
                    Some("RIGHTUP") => elk::NodePlacementAlignment::RightUp,
                    Some("RIGHTDOWN") => elk::NodePlacementAlignment::RightDown,
                    Some("BALANCED") => elk::NodePlacementAlignment::Balanced,
                    _ => elk::NodePlacementAlignment::None,
                }
            })
            .unwrap_or(preset_alignment);
    let self_loop_ordering = config_string(
        effective_config,
        &["elk", "layered", "edgeRouting", "selfLoopOrdering"],
    )
    .map(
        |strategy| match strategy.trim().to_ascii_uppercase().as_str() {
            "REVERSE_STACKED" => elk::SelfLoopOrderingStrategy::ReverseStacked,
            "SEQUENCED" => elk::SelfLoopOrderingStrategy::Sequenced,
            _ => elk::SelfLoopOrderingStrategy::Stacked,
        },
    )
    .unwrap_or_default();

    let layering =
        match explicit_option(effective_config, &["elk", "layeringStrategy"]).and_then(|value| {
            elk_enum_name(
                value,
                &[
                    "NETWORK_SIMPLEX",
                    "LONGEST_PATH",
                    "LONGEST_PATH_SOURCE",
                    "COFFMAN_GRAHAM",
                    "INTERACTIVE",
                    "STRETCH_WIDTH",
                    "MIN_WIDTH",
                    "BF_MODEL_ORDER",
                    "DF_MODEL_ORDER",
                ],
            )
        }) {
            Some("LONGEST_PATH") => elk::LayeringStrategy::LongestPath,
            Some("LONGEST_PATH_SOURCE") => elk::LayeringStrategy::LongestPathSource,
            Some("COFFMAN_GRAHAM") => elk::LayeringStrategy::CoffmanGraham,
            Some("MIN_WIDTH") => elk::LayeringStrategy::MinWidth,
            Some("STRETCH_WIDTH") => elk::LayeringStrategy::StretchWidth,
            Some("INTERACTIVE") => elk::LayeringStrategy::Interactive,
            _ => elk::LayeringStrategy::NetworkSimplex,
        };
    // elkjs 0.9.3's JsonImporter serializes numbers with JavaScript String(), then
    // LayoutOptionData.parseValue validates an INT without truncation. Invalid values
    // leave ELK's own layer-bound default (i32::MAX), not Mermaid's default (4).
    let layering_layer_bound = match effective_config.pointer("/elk/layeringLayerBound") {
        None | Some(serde_json::Value::Null) => 4,
        Some(value) => elk_integer(value).unwrap_or(i32::MAX),
    };

    elk::LayoutOptions {
        algorithm: crate::layout_backend::ElkRootAlgorithm::from_name(
            effective_config
                .get("layout")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        )
        .unwrap_or(crate::layout_backend::ElkRootAlgorithm::Layered)
        .algorithm(),
        container: elk::ContainerOptions {
            cycle_breaking,
            node_placement: explicit_node_placement
                .unwrap_or(elk::NodePlacementStrategy::BrandesKoepf),
            node_placement_alignment,
        },
        layered: elk::LayeredOptions {
            merge_edges: config_bool(effective_config, &["elk", "mergeEdges"]).unwrap_or(false),
            merge_hierarchy_edges: true,
            unnecessary_bendpoints: true,
            inside_self_loops_activate: config_bool(
                effective_config,
                &["elk", "insideSelfLoops", "activate"],
            )
            .unwrap_or(false),
            self_loop_distribution: elk::SelfLoopDistributionStrategy::North,
            self_loop_ordering,
            force_node_model_order: config_bool(effective_config, &["elk", "forceNodeModelOrder"])
                .unwrap_or(false),
            consider_model_order: model_order != elk::ModelOrderStrategy::None,
            model_order,
            cycle_breaking,
            layering,
            layering_layer_bound,
            node_placement,
            node_placement_alignment,
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn elk_enum_options_accept_exact_names_and_pinned_source_ordinals() {
        for (field, names) in [
            (
                "nodePlacementStrategy",
                &[
                    "SIMPLE",
                    "INTERACTIVE",
                    "LINEAR_SEGMENTS",
                    "BRANDES_KOEPF",
                    "NETWORK_SIMPLEX",
                ][..],
            ),
            (
                "nodePlacementAlignment",
                &[
                    "NONE",
                    "LEFTUP",
                    "RIGHTUP",
                    "LEFTDOWN",
                    "RIGHTDOWN",
                    "BALANCED",
                ][..],
            ),
            (
                "cycleBreakingStrategy",
                &[
                    "GREEDY",
                    "DEPTH_FIRST",
                    "INTERACTIVE",
                    "MODEL_ORDER",
                    "GREEDY_MODEL_ORDER",
                ][..],
            ),
            (
                "considerModelOrder",
                &["NONE", "NODES_AND_EDGES", "PREFER_EDGES", "PREFER_NODES"][..],
            ),
            (
                "layeringStrategy",
                &[
                    "NETWORK_SIMPLEX",
                    "LONGEST_PATH",
                    "LONGEST_PATH_SOURCE",
                    "COFFMAN_GRAHAM",
                    "INTERACTIVE",
                    "STRETCH_WIDTH",
                    "MIN_WIDTH",
                ][..],
            ),
        ] {
            for (ordinal, name) in names.iter().enumerate() {
                let mut config = json!({"elk": {}});
                config["elk"][field] = json!(name);
                let expected = layout_options(&config);
                for value in [
                    json!(ordinal),
                    json!(ordinal as f64),
                    json!(ordinal.to_string()),
                    json!(format!("+{ordinal}")),
                ] {
                    config["elk"][field] = value.clone();
                    assert_eq!(
                        layout_options(&config),
                        expected,
                        "{field}: {value} must select {name}"
                    );
                }
            }
        }
        assert_eq!(
            layout_options(&json!({"elk": {"nodePlacementStrategy": 1}}))
                .layered
                .node_placement,
            elk::NodePlacementStrategy::Interactive
        );
        assert_eq!(
            layout_options(&json!({"elk": {"nodePlacementAlignment": 2}}))
                .layered
                .node_placement_alignment,
            elk::NodePlacementAlignment::RightUp
        );
        assert_eq!(
            layout_options(&json!({"elk": {"layeringStrategy": 4}}))
                .layered
                .layering,
            elk::LayeringStrategy::Interactive
        );
    }

    #[test]
    fn invalid_elk_enum_options_use_provider_defaults_instead_of_rewriting_names() {
        for (field, valid, fallback) in [
            ("nodePlacementStrategy", "SIMPLE", "BRANDES_KOEPF"),
            ("nodePlacementAlignment", "RIGHTDOWN", "NONE"),
            ("cycleBreakingStrategy", "DEPTH_FIRST", "GREEDY"),
            ("considerModelOrder", "PREFER_EDGES", "NONE"),
            ("layeringStrategy", "COFFMAN_GRAHAM", "NETWORK_SIMPLEX"),
        ] {
            let mut config = json!({"elk": {"preset": "modelOrder"}});
            config["elk"][field] = json!(fallback);
            let expected = layout_options(&config);
            for value in [
                json!(valid.to_ascii_lowercase()),
                json!(format!(" {valid} ")),
                json!(true),
                json!(false),
                json!("0.0"),
                json!("0e0"),
                json!(" 0"),
                json!(0.5),
                json!(-1),
                json!(999),
            ] {
                config["elk"][field] = value.clone();
                assert_eq!(
                    layout_options(&config),
                    expected,
                    "{field}: invalid {value} must use {fallback}"
                );
            }
            config["elk"][field] = serde_json::Value::Null;
            assert_eq!(
                layout_options(&config),
                layout_options(&json!({"elk": {"preset": "modelOrder"}})),
                "{field}: null keeps the preset"
            );
        }
    }

    #[test]
    fn layering_bound_preserves_equivalent_json_number_representations() {
        for literal in ["2", "2.0", "2e0"] {
            let config: serde_json::Value = serde_json::from_str(&format!(
                r#"{{"elk":{{"layeringStrategy":"COFFMAN_GRAHAM","layeringLayerBound":{literal}}}}}"#
            ))
            .unwrap();
            let options = layout_options(&config);
            assert_eq!(
                options.layered.layering,
                elk::LayeringStrategy::CoffmanGraham
            );
            assert_eq!(options.layered.layering_layer_bound, 2, "{literal}");
        }
    }

    #[test]
    fn layering_bound_uses_elk_integer_parsing_and_invalid_value_default() {
        // Pinned elkjs 0.9.3 LayoutOptionData.parseValue / __parseAndValidateInt:
        // invalid INT options are omitted, so ELK uses its unbounded layer default.
        for (value, expected) in [
            (json!(0), 0),
            (json!(-2), -2),
            (json!(i32::MIN), i32::MIN),
            (json!(f64::from(i32::MIN)), i32::MIN),
            (json!(i32::MAX), i32::MAX),
            (json!(f64::from(i32::MAX)), i32::MAX),
            (json!(2147483648_i64), i32::MAX),
            (json!(2147483648.0), i32::MAX),
            (json!(-2147483649_i64), i32::MAX),
            (json!(-2147483649.0), i32::MAX),
            (json!(2.5), i32::MAX),
            (json!(-2.5), i32::MAX),
            (json!("2"), 2),
            (json!("+2"), 2),
            (json!("-2"), -2),
            (json!("2.0"), i32::MAX),
            (json!("2e0"), i32::MAX),
            (json!(" 2"), i32::MAX),
            (json!("2147483648"), i32::MAX),
            (json!(true), i32::MAX),
        ] {
            assert_eq!(
                layout_options(&json!({"elk": {"layeringLayerBound": value}}))
                    .layered
                    .layering_layer_bound,
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn layering_bound_keeps_mermaid_default_when_no_effective_option_is_available() {
        // Mermaid's public configuration merge ignores null object properties before
        // ELK projection. Direct internal callers without a value use the same default.
        for config in [
            serde_json::Value::Null,
            json!({"elk": {}}),
            json!({"elk": {"layeringLayerBound": null}}),
            json!({"elk": {"layeringLayerBound": 4}}),
        ] {
            assert_eq!(layout_options(&config).layered.layering_layer_bound, 4);
        }
    }

    #[cfg(feature = "diagram-flowchart")]
    #[test]
    fn public_configuration_ignores_null_layer_bounds_before_elk_projection() {
        use merman_core::{Engine, MermaidConfig, ParseOptions};

        // Mermaid 12 assignWithDepth.ts ignores null-valued object properties.
        // Its initialize, frontmatter, and init-directive routes therefore preserve
        // the prior layer bound instead of passing null to elkjs's JSON importer.
        let frontmatter_value = "---\nconfig:\n  elk:\n    layeringLayerBound: null\n---\n";
        let frontmatter_namespace = "---\nconfig:\n  elk: null\n---\n";
        let directive_value = "%%{init: {\"elk\": {\"layeringLayerBound\": null}}}%%\n";
        let directive_namespace = "%%{init: {\"elk\": null}}%%\n";
        for (site, prefix, expected) in [
            (json!({"elk": {"layeringLayerBound": null}}), "", 4),
            (json!({"elk": null}), "", 4),
            (json!({}), frontmatter_value, 4),
            (json!({}), frontmatter_namespace, 4),
            (json!({}), directive_value, 4),
            (json!({}), directive_namespace, 4),
            (
                json!({"elk": {"layeringLayerBound": 2}}),
                frontmatter_value,
                2,
            ),
            (
                json!({"elk": {"layeringLayerBound": 2}}),
                frontmatter_namespace,
                2,
            ),
            (
                json!({"elk": {"layeringLayerBound": 2}}),
                directive_value,
                2,
            ),
            (
                json!({"elk": {"layeringLayerBound": 2}}),
                directive_namespace,
                2,
            ),
        ] {
            let source = format!("{prefix}flowchart TD\nA-->B\nA-->C\n");
            for exact in [false, true] {
                let site = MermaidConfig::from_value(site.clone());
                let engine = if exact {
                    Engine::new().with_exact_site_config(Some(site))
                } else {
                    Engine::new().with_site_config(site)
                };
                let parsed = engine
                    .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                    .unwrap()
                    .unwrap();
                let effective = parsed.metadata().effective_config.as_value();
                assert_eq!(
                    effective.pointer("/elk/layeringLayerBound"),
                    Some(&json!(expected)),
                    "exact={exact}, source={source}"
                );
                assert_eq!(
                    layout_options(effective).layered.layering_layer_bound,
                    expected,
                    "exact={exact}, source={source}"
                );
            }
        }
    }

    #[test]
    fn elk_layout_options_use_mermaid_node_self_loop_default() {
        assert_eq!(
            layout_options(&serde_json::Value::Null)
                .layered
                .self_loop_distribution,
            elk::SelfLoopDistributionStrategy::North
        );
    }

    #[test]
    fn elk_layout_options_ignore_unsupported_self_loop_distribution_config() {
        let options = layout_options(&serde_json::json!({
            "elk": {
                "layered": {
                    "edgeRouting": { "selfLoopDistribution": "EQUALLY" }
                }
            }
        }));
        assert_eq!(
            options.layered.self_loop_distribution,
            elk::SelfLoopDistributionStrategy::North
        );
    }

    #[test]
    fn registered_root_names_select_the_corresponding_provider() {
        for (name, algorithm) in [
            ("elk", elk::Algorithm::Layered),
            ("elk.stress", elk::Algorithm::Stress),
            ("elk.force", elk::Algorithm::Force),
            ("elk.mrtree", elk::Algorithm::MrTree),
            ("elk.sporeOverlap", elk::Algorithm::SporeOverlap),
            ("elk.box", elk::Algorithm::Box),
            ("elk.rectpacking", elk::Algorithm::Rectpacking),
        ] {
            assert_eq!(
                layout_options(&json!({"layout": name})).algorithm,
                algorithm
            );
        }
    }

    #[test]
    fn elk_layout_options_resolve_presets_for_root_and_container() {
        use elk::NodePlacementStrategy as Placement;
        use elk::{CycleBreakingStrategy as Cycle, NodePlacementAlignment as Align};

        for (preset, placement, alignment, cycle) in [
            (
                "default",
                Placement::BrandesKoepf,
                Align::Balanced,
                Cycle::DepthFirst,
            ),
            (
                "legacy",
                Placement::BrandesKoepf,
                Align::None,
                Cycle::Greedy,
            ),
            (
                "modelOrder",
                Placement::NetworkSimplex,
                Align::None,
                Cycle::GreedyModelOrder,
            ),
            (
                "depthFirst",
                Placement::NetworkSimplex,
                Align::None,
                Cycle::DepthFirst,
            ),
            (
                "unknown",
                Placement::BrandesKoepf,
                Align::Balanced,
                Cycle::DepthFirst,
            ),
        ] {
            let options = layout_options(&json!({"elk": {"preset": preset}}));
            assert_eq!(options.layered.node_placement, placement, "{preset}");
            assert_eq!(
                options.container.node_placement,
                Placement::BrandesKoepf,
                "{preset}"
            );
            assert_eq!(
                options.layered.node_placement_alignment, alignment,
                "{preset}"
            );
            assert_eq!(
                options.container.node_placement_alignment, alignment,
                "{preset}"
            );
            assert_eq!(options.layered.cycle_breaking, cycle, "{preset}");
            assert_eq!(options.container.cycle_breaking, cycle, "{preset}");
        }
    }

    #[test]
    fn elk_layout_options_explicit_values_override_both_preset_scopes() {
        let options = layout_options(&json!({"elk": {
            "preset": "modelOrder",
            "nodePlacementStrategy": "SIMPLE",
            "nodePlacementAlignment": "RIGHTDOWN",
            "cycleBreakingStrategy": "INTERACTIVE"
        }}));
        assert_eq!(
            options.layered.node_placement,
            elk::NodePlacementStrategy::Simple
        );
        assert_eq!(
            options.container.node_placement,
            elk::NodePlacementStrategy::Simple
        );
        assert_eq!(
            options.layered.node_placement_alignment,
            elk::NodePlacementAlignment::RightDown
        );
        assert_eq!(
            options.container.node_placement_alignment,
            elk::NodePlacementAlignment::RightDown
        );
        assert_eq!(
            options.layered.cycle_breaking,
            elk::CycleBreakingStrategy::Interactive
        );
        assert_eq!(
            options.container.cycle_breaking,
            elk::CycleBreakingStrategy::Interactive
        );
    }
}
