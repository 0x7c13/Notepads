use crate::MermaidConfig;
use std::collections::BTreeSet;
use std::sync::OnceLock;

pub mod dompurify_defaults;
pub(crate) mod editor_rename_policy;
pub mod mermaid_reference;

static UPSTREAM_DEFAULT_CONFIG: OnceLock<MermaidConfig> = OnceLock::new();
static DEFAULT_SITE_CONFIG: OnceLock<MermaidConfig> = OnceLock::new();
static DEFAULT_CONFIG_SHAPE: OnceLock<DefaultConfigShape> = OnceLock::new();

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DefaultConfigShape {
    baseline_version: String,
    config_keys: BTreeSet<String>,
}

fn default_config_shape() -> &'static DefaultConfigShape {
    DEFAULT_CONFIG_SHAPE.get_or_init(|| {
        let shape: DefaultConfigShape =
            serde_json::from_str(include_str!("default_config_shape.json"))
                .expect("generated default config shape JSON is valid");
        assert_eq!(
            shape.baseline_version,
            crate::baseline::PINNED_MERMAID_BASELINE_VERSION,
            "generated default config shape targets the pinned Mermaid baseline"
        );
        shape
    })
}

pub(crate) fn upstream_default_config() -> MermaidConfig {
    UPSTREAM_DEFAULT_CONFIG
        .get_or_init(|| {
            let json_text = include_str!("default_config.json");
            let value: serde_json::Value =
                serde_json::from_str(json_text).expect("generated default config JSON is valid");
            MermaidConfig::from_value(value)
        })
        .clone()
}

pub fn default_site_config() -> MermaidConfig {
    DEFAULT_SITE_CONFIG
        .get_or_init(|| {
            let mut config = upstream_default_config();
            crate::config::apply_hardened_site_policy(&mut config);
            config
        })
        .clone()
}

/// Returns the flat key set used by Mermaid 11.16 to sanitize init directives.
fn default_config_keys() -> &'static BTreeSet<String> {
    &default_config_shape().config_keys
}

/// Returns whether `key` belongs to Mermaid's generated default-config shape.
pub(crate) fn is_default_config_key(key: &str) -> bool {
    default_config_keys().contains(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_shape_includes_json_undefined_and_function_keys() {
        assert!(is_default_config_key("flowchart"));
        assert!(is_default_config_key("nodeColors"));
        assert!(is_default_config_key("messageFont"));
        assert!(is_default_config_key("themeVariables"));
        assert!(!is_default_config_key("notAConfigKey"));
    }
}
