//! mhchem (`\ce`, `\pu`): pure Rust port of KaTeX mhchem 3.3.0.
//!
//! Data (`machines.json`, `patterns_regex.json`) is generated from `tools/mhchem_reference.js`;
//! update workflow: `docs/MHCHEM_DATA.md`.

mod actions;
mod buffer;
mod data;
mod engine;
mod error;
mod json;
mod patterns;
mod resources;
mod texify;

pub use data::data;
pub use error::{MhchemError, MhchemResult};

use crate::expansion_budget::ExpansionBudget;
use crate::mhchem::data::MhchemData;
use crate::stack_safety::{DepthBudget, MAX_INPUT_DEPTH};
use serde_json::Value;

/// Context for recursive `go`.
///
/// ```
/// let ctx = ratex_parser::mhchem::ParserCtx {
///     data: ratex_parser::mhchem::data(),
/// };
/// let ast = ctx.go("H2O", "ce").expect("mhchem");
/// assert!(!ast.is_empty());
/// ```
pub struct ParserCtx<'a> {
    pub data: &'a MhchemData,
}

impl ParserCtx<'_> {
    pub fn go(&self, input: &str, machine: &str) -> MhchemResult<Vec<Value>> {
        RuntimeCtx {
            data: self.data,
            depth_budget: DepthBudget::new(MAX_INPUT_DEPTH),
            resources: ExpansionBudget::default(),
        }
        .go(input, machine)
    }
}

/// Internal mhchem context that shares the parser's depth budget across
/// recursive state-machine calls.
pub(crate) struct RuntimeCtx<'a> {
    pub(crate) data: &'a MhchemData,
    pub depth_budget: DepthBudget,
    pub(crate) resources: ExpansionBudget,
}

impl RuntimeCtx<'_> {
    pub(crate) fn go(&self, input: &str, machine: &str) -> MhchemResult<Vec<Value>> {
        engine::go_machine(self, input, machine)
    }
}

/// Parse `\ce` / `\pu` argument to TeX fragment (wrap `\mathrm` etc. is done here).
pub fn chem_parse_str(input: &str, mode: &str) -> MhchemResult<String> {
    chem_parse_str_with_budget(input, mode, DepthBudget::new(MAX_INPUT_DEPTH))
}

pub(crate) fn chem_parse_str_with_budget(
    input: &str,
    mode: &str,
    depth_budget: DepthBudget,
) -> MhchemResult<String> {
    chem_parse_str_with_resources(input, mode, depth_budget, ExpansionBudget::default())
}

pub(crate) fn chem_parse_str_with_resources(
    input: &str,
    mode: &str,
    depth_budget: DepthBudget,
    resources: ExpansionBudget,
) -> MhchemResult<String> {
    let d = data();
    let ctx = RuntimeCtx {
        data: d,
        depth_budget: depth_budget.clone(),
        resources: resources.clone(),
    };
    let sm = match mode {
        "ce" => "ce",
        "pu" => "pu",
        _ => {
            return Err(MhchemError::msg(format!(
                "unknown mhchem mode (expected ce|pu): {mode}"
            )));
        }
    };
    let ast = ctx.go(input.trim(), sm)?;
    texify::go_with_budget(&ast, false, &depth_budget, &resources)
}

pub(crate) fn arg_tokens_with_resources(
    tokens: &[ratex_lexer::token::Token],
    resources: &ExpansionBudget,
) -> MhchemResult<String> {
    let Some(last) = tokens.last() else {
        return Ok(String::new());
    };
    let mut expected_loc = last.loc.start;
    let mut out = String::new();
    for t in tokens.iter().rev() {
        if t.loc.start > expected_loc {
            resources.append(&mut out, " ")?;
            expected_loc = t.loc.start;
        }
        resources.append(&mut out, &t.text)?;
        expected_loc = expected_loc.saturating_add(t.text.len());
    }
    Ok(out)
}

/// Rebuild a macro argument string from tokens ([KaTeX `chemParse`]).
pub fn mhchem_arg_tokens_to_string(tokens: &[ratex_lexer::token::Token]) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    let mut expected_loc = tokens.last().unwrap().loc.start;
    let mut out = String::new();
    for i in (0..tokens.len()).rev() {
        let t = &tokens[i];
        if t.loc.start > expected_loc {
            out.push(' ');
            expected_loc = t.loc.start;
        }
        out.push_str(&t.text);
        expected_loc += t.text.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h2o_ce() {
        let t = chem_parse_str("H2O", "ce").expect("mhchem");
        assert!(!t.is_empty());
        assert!(t.contains('H'));
    }

    #[test]
    fn reaction_arrow() {
        let t = chem_parse_str("2H + O -> H2O", "ce").expect("mhchem");
        assert!(t.contains("rightarrow") || t.contains("->"), "{}", t);
    }

    #[test]
    fn pu_simple() {
        let t = chem_parse_str("123 kJ/mol", "pu").expect("mhchem");
        assert!(!t.is_empty());
    }

    #[test]
    fn nested_submachines_have_a_depth_budget() {
        let nested_ce = |depth: usize| format!("{}H{}", r"\ce{".repeat(depth), "}".repeat(depth));

        // mhchem also enters helper machines while processing terminal atoms,
        // so visible \ce nesting consumes slightly less than the full engine budget.
        assert!(chem_parse_str(&nested_ce(30), "ce").is_ok());
        let error = chem_parse_str(&nested_ce(31), "ce").unwrap_err();
        assert!(
            error.to_string().contains("Recursion limit exceeded"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn nested_submachine_engine_depth_is_bounded_before_texify() {
        let nested_ce = |depth: usize| format!("{}H{}", r"\ce{".repeat(depth), "}".repeat(depth));
        let nested_empty_ce =
            |depth: usize| format!("{}{}", r"\ce{".repeat(depth), "}".repeat(depth));
        let d = data();
        let ctx = RuntimeCtx {
            data: d,
            depth_budget: DepthBudget::new(MAX_INPUT_DEPTH),
            resources: ExpansionBudget::default(),
        };

        assert!(ctx.go(&nested_ce(30), "ce").is_ok());
        let error = ctx.go(&nested_ce(31), "ce").unwrap_err();
        assert!(
            error.to_string().contains("Recursion limit exceeded"),
            "unexpected error: {error}"
        );

        assert!(ctx.go(&nested_empty_ce(31), "ce").is_ok());
        let error = ctx.go(&nested_empty_ce(32), "ce").unwrap_err();
        assert!(
            error.to_string().contains("Recursion limit exceeded"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn pu_scientific_lowercase_e_cdot_uppercase_e_times() {
        for src in ["1.2e3 kJ", "1,2e3 kJ"] {
            let t = chem_parse_str(src, "pu").expect("mhchem");
            assert!(
                t.contains("\\cdot") && t.contains("10^{3}") && !t.contains("\\times"),
                "expected \\cdot for lowercase e: {src:?} → {t:?}"
            );
        }
        for src in ["1.2E3 kJ", "1,2E3 kJ"] {
            let t = chem_parse_str(src, "pu").expect("mhchem");
            assert!(
                t.contains("\\times") && t.contains("10^{3}") && !t.contains("\\cdot"),
                "expected \\times for uppercase E: {src:?} → {t:?}"
            );
        }
    }

    #[test]
    fn dollar_underset_inner_ce_tex_is_valid_latex() {
        let inner = r"$\underset{\mathrm{red}}{\ce{HgI2}}$";
        let tex = chem_parse_str(inner, "ce").expect("mhchem");
        crate::parser::parse(&tex).expect("mhchem TeX should parse");
    }
}
