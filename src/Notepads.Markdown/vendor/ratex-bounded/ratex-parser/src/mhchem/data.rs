//! Load `machines.json` + `patterns_regex.json` and compile regex patterns.

use crate::mhchem::error::{MhchemError, MhchemResult};
use crate::mhchem::json::Machines;
use fancy_regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

const MACHINES_JSON: &str = include_str!("data/machines.json");
const PATTERNS_JSON: &str = include_str!("data/patterns_regex.json");

#[derive(Debug)]
pub struct RegexPatterns {
    pub map: HashMap<String, Regex>,
}

#[derive(Debug)]
pub struct MhchemData {
    pub machines: Machines,
    pub regexes: RegexPatterns,
}

impl MhchemData {
    pub fn load() -> MhchemResult<Self> {
        let machines: Machines =
            serde_json::from_str(MACHINES_JSON).map_err(|e| MhchemError::msg(e.to_string()))?;

        let v: serde_json::Value =
            serde_json::from_str(PATTERNS_JSON).map_err(|e| MhchemError::msg(e.to_string()))?;
        let obj = v
            .get("regex")
            .and_then(|x| x.as_object())
            .ok_or_else(|| MhchemError::msg("patterns_regex: missing regex"))?;

        let mut map = HashMap::new();
        for (k, src_val) in obj {
            let src = src_val
                .as_str()
                .ok_or_else(|| MhchemError::msg("pattern not string"))?;
            let re = Regex::new(src)
                .map_err(|e| MhchemError::msg(format!("regex compile {:?}: {}", k, e)))?;
            map.insert(k.clone(), re);
        }

        Ok(MhchemData {
            machines,
            regexes: RegexPatterns { map },
        })
    }
}

static MHCHEM_DATA: OnceLock<MhchemData> = OnceLock::new();

pub fn data() -> &'static MhchemData {
    MHCHEM_DATA
        .get_or_init(|| MhchemData::load().expect("mhchem static data must parse and compile"))
}
