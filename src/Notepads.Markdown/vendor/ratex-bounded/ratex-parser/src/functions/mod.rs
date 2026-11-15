// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
pub mod accent;
pub mod arrow;
pub mod bussproofs;
pub mod char_cmd;
pub mod color;
pub mod cr;
pub mod def;
pub mod delimsizing;
pub mod enclose;
pub mod environment;
pub mod font;
pub mod genfrac;
pub mod hbox;
pub mod horiz_brace;
pub mod href;
pub mod htmlmathml;
pub mod kern;
pub mod lap;
pub mod left_right;
pub mod math;
pub mod mathchoice;
pub mod mclass;
pub mod nonumber;
pub mod op;
pub mod operatorname;
pub mod overline;
pub mod phantom;
pub mod pmb;
pub mod raisebox;
pub mod relax;
pub mod rule;
pub mod sizing;
pub mod spacing;
pub mod sqrt;
pub mod styling;
pub mod symbols_cmd;
pub mod tag;
pub mod text;
pub mod vcenter;
pub mod verb;

use crate::error::ParseResult;
use crate::parse_node::{Mode, ParseNode};
use std::collections::HashMap;

use ratex_lexer::token::Token;

/// Context passed to function handlers.
pub struct FunctionContext<'a, 'b> {
    pub func_name: String,
    pub parser: &'a mut crate::parser::Parser<'b>,
    pub token: Option<Token>,
    pub break_on_token_text: Option<String>,
}

/// Handler function signature.
pub type FunctionHandler = fn(
    ctx: &mut FunctionContext,
    args: Vec<ParseNode>,
    opt_args: Vec<Option<ParseNode>>,
) -> ParseResult<ParseNode>;

/// Argument types for function parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    Color,
    Size,
    Url,
    Math,
    Text,
    HBox,
    Raw,
    Primitive,
    Original,
}

/// Specification for a registered function.
pub struct FunctionSpec {
    pub node_type: &'static str,
    pub num_args: usize,
    pub num_optional_args: usize,
    pub arg_types: Option<std::borrow::Cow<'static, [ArgType]>>,
    pub allowed_in_argument: bool,
    pub allowed_in_text: bool,
    pub allowed_in_math: bool,
    pub infix: bool,
    pub primitive: bool,
    pub handler: FunctionHandler,
}

/// Get the global function registry.
pub struct StaticFunctions;
pub static FUNCTIONS: StaticFunctions = StaticFunctions;
static REGISTRIES: &[&[(&[&str], FunctionSpec)]] = &[
    genfrac::BOUNDED_REGISTRATIONS,
    sqrt::BOUNDED_REGISTRATIONS,
    op::BOUNDED_REGISTRATIONS,
    accent::BOUNDED_REGISTRATIONS,
    font::BOUNDED_REGISTRATIONS,
    color::BOUNDED_REGISTRATIONS,
    sizing::BOUNDED_REGISTRATIONS,
    delimsizing::BOUNDED_REGISTRATIONS,
    left_right::BOUNDED_REGISTRATIONS,
    spacing::BOUNDED_REGISTRATIONS,
    styling::BOUNDED_REGISTRATIONS,
    overline::BOUNDED_REGISTRATIONS,
    kern::BOUNDED_REGISTRATIONS,
    phantom::BOUNDED_REGISTRATIONS,
    text::BOUNDED_REGISTRATIONS,
    cr::BOUNDED_REGISTRATIONS,
    relax::BOUNDED_REGISTRATIONS,
    verb::BOUNDED_REGISTRATIONS,
    symbols_cmd::BOUNDED_REGISTRATIONS,
    environment::BOUNDED_REGISTRATIONS,
    mclass::BOUNDED_REGISTRATIONS,
    operatorname::BOUNDED_REGISTRATIONS,
    horiz_brace::BOUNDED_REGISTRATIONS,
    arrow::BOUNDED_REGISTRATIONS,
    enclose::BOUNDED_REGISTRATIONS,
    rule::BOUNDED_REGISTRATIONS,
    href::BOUNDED_REGISTRATIONS,
    hbox::BOUNDED_REGISTRATIONS,
    lap::BOUNDED_REGISTRATIONS,
    raisebox::BOUNDED_REGISTRATIONS,
    vcenter::BOUNDED_REGISTRATIONS,
    pmb::BOUNDED_REGISTRATIONS,
    mathchoice::BOUNDED_REGISTRATIONS,
    def::BOUNDED_REGISTRATIONS,
    htmlmathml::BOUNDED_REGISTRATIONS,
    char_cmd::BOUNDED_REGISTRATIONS,
    math::BOUNDED_REGISTRATIONS,
    tag::BOUNDED_REGISTRATIONS,
    nonumber::BOUNDED_REGISTRATIONS,
    bussproofs::BOUNDED_REGISTRATIONS,
];
impl StaticFunctions {
    pub fn get(&self, name: &str) -> Option<&'static FunctionSpec> {
        for groups in REGISTRIES.iter().rev() {
            for (names, spec) in groups.iter().rev() {
                if names.contains(&name) {
                    return Some(spec);
                }
            }
        }
        None
    }
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

pub(crate) const fn bounded_readonly_bytes() -> usize {
    let mut sum = size_of_val(REGISTRIES);
    let mut registry = 0;
    let mut count = 0;
    while registry < REGISTRIES.len() {
        let groups = REGISTRIES[registry];
        sum += size_of_val(groups);
        let mut group = 0;
        while group < groups.len() {
            let (names, spec) = &groups[group];
            sum += size_of_val(*names) + spec.node_type.len();
            let mut name = 0;
            while name < names.len() {
                sum += names[name].len();
                name += 1;
                count += 1;
            }
            if let Some(types) = &spec.arg_types {
                match types {
                    std::borrow::Cow::Borrowed(types) => sum += size_of_val(*types),
                    std::borrow::Cow::Owned(_) => {
                        panic!("Fixed registry must not own runtime type arrays")
                    }
                }
            }
            group += 1;
        }
        registry += 1;
    }
    assert!(count <= 1024);
    sum
}

#[cfg(test)]
mod fixed_registry_tests {
    use super::*;
    #[test]
    fn every_fixed_function_matches_original_registration_metadata_and_handler() {
        let mut original = HashMap::new();
        let registrars: [fn(&mut HashMap<&'static str, FunctionSpec>); 40] = [
            genfrac::register,
            sqrt::register,
            op::register,
            accent::register,
            font::register,
            color::register,
            sizing::register,
            delimsizing::register,
            left_right::register,
            spacing::register,
            styling::register,
            overline::register,
            kern::register,
            phantom::register,
            text::register,
            cr::register,
            relax::register,
            verb::register,
            symbols_cmd::register,
            environment::register,
            mclass::register,
            operatorname::register,
            horiz_brace::register,
            arrow::register,
            enclose::register,
            rule::register,
            href::register,
            hbox::register,
            lap::register,
            raisebox::register,
            vcenter::register,
            pmb::register,
            mathchoice::register,
            def::register,
            htmlmathml::register,
            char_cmd::register,
            math::register,
            tag::register,
            nonumber::register,
            bussproofs::register,
        ];
        for register in registrars {
            register(&mut original);
        }
        assert!(original.len() <= 1024);
        for (name, expected) in &original {
            let actual = FUNCTIONS
                .get(name)
                .unwrap_or_else(|| panic!("Missing original {name}"));
            assert_eq!(actual.node_type, expected.node_type, "{name}");
            assert_eq!(actual.num_args, expected.num_args, "{name}");
            assert_eq!(
                actual.num_optional_args, expected.num_optional_args,
                "{name}"
            );
            assert_eq!(actual.arg_types, expected.arg_types, "{name}");
            assert_eq!(
                (
                    actual.allowed_in_argument,
                    actual.allowed_in_text,
                    actual.allowed_in_math,
                    actual.infix,
                    actual.primitive
                ),
                (
                    expected.allowed_in_argument,
                    expected.allowed_in_text,
                    expected.allowed_in_math,
                    expected.infix,
                    expected.primitive
                ),
                "{name}"
            );
            assert!(
                std::ptr::fn_addr_eq(actual.handler, expected.handler),
                "{name}"
            );
        }
        for groups in REGISTRIES {
            for (names, _) in *groups {
                for name in *names {
                    assert!(original.contains_key(name));
                }
            }
        }
    }
}

/// Helper to define a function with common defaults.
pub fn define_function(
    map: &mut HashMap<&'static str, FunctionSpec>,
    names: &[&'static str],
    node_type: &'static str,
    num_args: usize,
    handler: FunctionHandler,
) {
    define_function_full(
        map, names, node_type, num_args, 0, None, false, false, true, false, false, handler,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn define_function_full(
    map: &mut HashMap<&'static str, FunctionSpec>,
    names: &[&'static str],
    node_type: &'static str,
    num_args: usize,
    num_optional_args: usize,
    arg_types: Option<Vec<ArgType>>,
    allowed_in_argument: bool,
    allowed_in_text: bool,
    allowed_in_math: bool,
    infix: bool,
    primitive: bool,
    handler: FunctionHandler,
) {
    for &name in names {
        map.insert(
            name,
            FunctionSpec {
                node_type,
                num_args,
                num_optional_args,
                arg_types: arg_types.clone().map(std::borrow::Cow::Owned),
                allowed_in_argument,
                allowed_in_text,
                allowed_in_math,
                infix,
                primitive,
                handler,
            },
        );
    }
}

/// Check if a mode is compatible with a function spec.
pub fn check_mode_compatibility(
    func: &FunctionSpec,
    mode: Mode,
    func_name: &str,
    token: Option<&Token>,
) -> ParseResult<()> {
    if mode == Mode::Text && !func.allowed_in_text {
        return Err(crate::error::ParseError::new(
            format!("Can't use function '{}' in text mode", func_name),
            token,
        ));
    }
    if mode == Mode::Math && !func.allowed_in_math {
        return Err(crate::error::ParseError::new(
            format!("Can't use function '{}' in math mode", func_name),
            token,
        ));
    }
    Ok(())
}
