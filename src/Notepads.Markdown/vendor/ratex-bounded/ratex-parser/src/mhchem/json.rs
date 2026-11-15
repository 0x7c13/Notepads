//! JSON structures for `data/machines.json` (from `tools/generate_mhchem_data.mjs`).

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct Machines(pub HashMap<String, MachineDef>);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDef {
    pub transitions: HashMap<String, Vec<Transition>>,
    #[serde(default)]
    pub has_local_actions: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Transition {
    pub pattern: String,
    pub task: Task,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub next_state: Option<String>,
    #[serde(default)]
    pub revisit: bool,
    #[serde(default)]
    pub to_continue: bool,
    /// JSON keeps KaTeX name `action_`; `camelCase` would wrongly expect `action`.
    #[serde(default, rename = "action_")]
    pub action_: Vec<ActionSpec>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ActionSpec {
    pub type_: String,
    #[serde(default)]
    pub option: Option<serde_json::Value>,
}
