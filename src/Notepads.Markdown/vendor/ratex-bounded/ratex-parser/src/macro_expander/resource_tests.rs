// Notepads internal-test signature adaptation: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use super::*;
use crate::parser::Parser;

fn limited(tokens: usize, bytes: usize) -> ExpansionBudget {
    ExpansionBudget::new(Cost {
        tokens,
        bytes,
        nodes: 100_000,
    })
}
fn stack(text: &str) -> Vec<Token> {
    let mut tokens = Lexer::new(text).lex_all();
    tokens.pop();
    tokens.reverse();
    tokens
}
#[test]
fn replacement_moves_last_reference_and_preserves_hashes() {
    let budget = limited(2, 2);
    let template = stack("#1#1");
    let output = Template::Tokens(&template)
        .expand(vec![stack("ab")], &budget, None)
        .unwrap();
    assert_eq!(
        output
            .iter()
            .rev()
            .map(|t| t.text.as_str())
            .collect::<String>(),
        "abab"
    );
    assert_eq!(
        budget.used(),
        Cost {
            tokens: 2,
            bytes: 2,
            nodes: 0
        }
    );
    let b = limited(1, 1);
    let output = Template::Tokens(&stack("###1"))
        .expand(vec![stack("ab")], &b, None)
        .unwrap();
    assert_eq!(
        output
            .iter()
            .rev()
            .map(|t| t.text.as_str())
            .collect::<String>(),
        "#ab"
    );
    assert_eq!(b.used().tokens, 1);
}
#[test]
fn escaped_hash_preserves_source_location_and_allocation_cost() {
    let text = "C ##N#1";
    let tokens = stack(text);
    for template in [Template::Text(text), Template::Tokens(&tokens)] {
        let budget = limited(4, 4);
        let output = template.expand(vec![stack("x")], &budget, None).unwrap();
        let hash = output.iter().find(|t| t.text == "#").unwrap();
        assert_eq!(hash.loc, SourceLocation { start: 3, end: 4 });
        assert_eq!(
            output
                .iter()
                .rev()
                .map(|t| t.text.as_str())
                .collect::<String>(),
            "C #Nx"
        );
        assert_eq!(
            budget.used(),
            Cost {
                tokens: 4,
                bytes: 4,
                nodes: 0
            }
        );
    }
}

#[test]
fn escaped_hash_in_macro_preserves_mhchem_bond_spacing() {
    for (body, expected) in [("C ##N", "C #N"), ("C ## N", "C # N"), ("##N", "#N")] {
        let input = format!(r"\def\foo#1{{\ce{{{body}}}}}\foo{{x}}");
        let expected = format!(r"\ce{{{expected}}}");
        assert_eq!(
            serde_json::to_value(crate::parse(&input).unwrap()).unwrap(),
            serde_json::to_value(crate::parse(&expected).unwrap()).unwrap(),
            "{input} should match {expected}"
        );
    }
}

#[test]
fn text_scanning_checks_before_copying_large_token() {
    for text in [
        format!("\\{}", "a".repeat(4096)),
        format!("\\verb|{}|", "a".repeat(4096)),
    ] {
        let b = limited(10, 8);
        assert!(lex_string_to_stack_tokens(&text, &b).is_err());
        assert_eq!(b.used(), Cost::default());
        let mut me = MacroExpander::new("\\foo", Mode::Math);
        me.allocation_budget = b.clone();
        me.set_text_macro("\\foo", &text);
        assert!(
            me.expand_next_token()
                .unwrap_err()
                .message
                .contains("resource limit")
        );
        assert_eq!(b.used(), Cost::default());
    }
}
#[test]
fn expansion_collection_does_not_recharge_and_restores_stack() {
    let mut me = MacroExpander::new("", Mode::Math);
    me.allocation_budget = limited(2, 2);
    me.set_text_macro("\\foo", "ab");
    me.push_token(Token::new("sentinel", 0, 0));
    assert_eq!(me.expand_tokens(stack("\\foo")).unwrap().len(), 2);
    assert_eq!(me.allocation_budget.used().tokens, 2);
    assert!(me.expand_tokens(stack("\\foo")).is_err());
    assert_eq!(me.expand_tokens_depth, 0);
    assert_eq!(me.pop_token().text, "sentinel");
}
#[test]
fn aliases_and_group_undo_share_immutable_definition() {
    let mut me = MacroExpander::new("", Mode::Math);
    me.set_macro(
        "\\foo".into(),
        MacroDefinition::Tokens {
            tokens: stack("abc"),
            num_args: 0,
        },
    );
    let original = me.macros.current["\\foo"].clone();
    assert!(me.alias_macro("\\bar", "\\foo", false));
    assert!(SharedMacro::ptr_eq(&original, &me.macros.current["\\bar"]));
    me.begin_group();
    me.set_text_macro("\\foo", "x");
    assert!(SharedMacro::ptr_eq(
        &original,
        me.macros.group_stack.last().unwrap()["\\foo"]
            .as_ref()
            .unwrap()
    ));
    me.end_group();
    assert!(SharedMacro::ptr_eq(&original, &me.macros.current["\\foo"]));
    assert_eq!(me.allocation_budget.used(), Cost::default());
}
#[test]
fn original_reports_and_alphabetic_xdef_chains_are_rejected() {
    let first = concat!(
        r"\def\zz{zzzzzzzzzz}",
        r"\edef\aa{\zz\zz\zz\zz\zz\zz\zz\zz\zz\zz}",
        r"\edef\bb{\aa\aa\aa\aa\aa\aa\aa\aa\aa\aa}",
        r"\edef\cc{\bb\bb\bb\bb\bb\bb\bb\bb\bb\bb}",
        r"\edef\dd{\cc\cc\cc\cc\cc\cc\cc\cc\cc\cc}",
        r"\edef\ee{\dd\dd\dd\dd\dd\dd\dd\dd\dd\dd}",
        r"\edef\ff{\ee\ee\ee\ee\ee\ee\ee\ee\ee\ee}x"
    );
    let mut second = String::from(r"\edef\m0{xxxxxxxx}");
    for k in 1..=18 {
        second += &format!("\n\\edef\\m{k}{{\\m{}\\m{}}}", k - 1, k - 1);
    }
    second += "\n\\m18";
    let mut cases = vec![first.to_owned(), second];
    for def in ["edef", "xdef"] {
        for used in [false, true] {
            let mut s = format!("\\{def}\\ma{{xxxxxxxx}}");
            for (a, b) in ('a'..='r').zip('b'..='s') {
                s += &format!("\\{def}\\m{b}{{\\m{a}\\m{a}}}");
            }
            if used {
                s += r"\ms";
            } else {
                s += "x";
            }
            cases.push(s);
        }
    }
    for (i, input) in cases.iter().enumerate() {
        let mut p = Parser::new(input);
        let e = p.parse().unwrap_err();
        eprintln!("report case {i}: {e}");
        assert!(
            e.message
                .contains("Macro expansion resource limit exceeded"),
            "{e}"
        );
        assert!(p.gullet.expansion_count < 1000);
        assert!(p.gullet.allocation_budget.used().tokens <= 100_000);
    }
}
#[test]
fn recursive_error_depends_on_which_budget_exhausts_first() {
    let e = crate::parse(r"\def\a{\a}\a").unwrap_err();
    assert!(e.message.contains("Too many expansions"));
    for body in [
        "a".repeat(2048),
        r"\&".repeat(256),
        r"\bmod".repeat(128),
        r"\\".repeat(128),
    ] {
        let input = format!("\\def\\a{{{body}\\a}}\\a");
        let mut p = Parser::new(&input);
        if body.starts_with('\\') {
            p.gullet.allocation_budget = limited(500, 8 * 1024 * 1024);
        }
        let e = p.parse().unwrap_err();
        eprintln!("large recursive body: {e}");
        assert!(e.message.contains("resource limit"), "{e}");
        assert!(p.gullet.expansion_count < 1000);
    }
}
#[test]
fn token_and_byte_limits_are_independent_and_cumulative() {
    let mut me = MacroExpander::new("\\foo\\foo", Mode::Math);
    me.allocation_budget = limited(100, 3);
    me.set_text_macro("\\foo", "é");
    assert_eq!(me.expand_next_token().unwrap().text, "é");
    let before = me.allocation_budget.used();
    me.begin_group();
    assert!(
        me.expand_next_token()
            .unwrap_err()
            .message
            .contains("resource limit")
    );
    me.end_group();
    assert_eq!(me.allocation_budget.used(), before);
}
#[test]
fn braket_replacements_are_bounded_before_copy() {
    let mut me = MacroExpander::new("{} {abc} {} {} {x|y|z}", Mode::Math);
    me.allocation_budget = limited(5, 5);
    let e = me.expand_braket(false).unwrap_err();
    assert!(e.message.contains("braket replacement"));
    assert_eq!(me.allocation_budget.used().tokens, 3);
}

#[test]
fn multiple_parameters_preserve_logical_order_and_unused_arguments_are_free() {
    let budget = limited(4, 4);
    let template = stack("A#2#1#2Z");
    let output = Template::Tokens(&template)
        .expand(
            vec![stack("ab"), stack("cd"), stack("unused")],
            &budget,
            None,
        )
        .unwrap();
    assert_eq!(
        output
            .iter()
            .rev()
            .map(|t| t.text.as_str())
            .collect::<String>(),
        "AcdabcdZ"
    );
    assert_eq!(
        budget.used(),
        Cost {
            tokens: 4,
            bytes: 4,
            nodes: 0
        }
    );
    assert_eq!(template, stack("A#2#1#2Z"));
}

#[test]
fn a_single_token_macro_is_rejected_before_cloning_its_body() {
    let mut me = MacroExpander::new("\\foo", Mode::Math);
    me.allocation_budget = limited(3, 100);
    me.set_macro(
        "\\foo".into(),
        MacroDefinition::Tokens {
            tokens: stack("abcd"),
            num_args: 0,
        },
    );
    let error = me.expand_next_token().unwrap_err();
    assert!(error.message.contains("token macro body"));
    assert_eq!(me.allocation_budget.used(), Cost::default());
    assert_eq!(me.expansion_count, 1);
}

#[test]
fn builtin_storage_is_shared_but_overrides_are_parse_local() {
    let mut a = MacroExpander::new("", Mode::Math);
    let b = MacroExpander::new("", Mode::Math);
    let shared = b.macros.get_shared("\\quad").unwrap();
    assert!(SharedMacro::ptr_eq(
        a.macros.get_shared("\\quad").unwrap(),
        &shared
    ));
    a.begin_group();
    a.set_text_macro("\\quad", "x");
    assert!(!SharedMacro::ptr_eq(
        a.macros.get_shared("\\quad").unwrap(),
        &shared
    ));
    assert!(a.alias_macro("\\saved", "\\quad", true));
    a.end_group();
    assert!(SharedMacro::ptr_eq(
        a.macros.get_shared("\\quad").unwrap(),
        &shared
    ));
    assert!(matches!(a.get_macro("\\saved"), Some(MacroDefinition::Text(s)) if s == "x"));
    assert!(b.get_macro("\\saved").is_none());
}

#[test]
fn text_arity_preserves_raw_hash_inference_without_body_copying() {
    for (text, expected) in [
        ("abc", 0),
        ("##1", 0),
        ("###1", 1),
        (r"\verb|#1|", 1),
        ("% #1", 1),
        ("#1#3", 1),
        ("#1#2#3#4#5#6#7#8#9#10", 10),
    ] {
        assert_eq!(Template::Text(text).num_args(), expected, "{text}");
    }
}

#[test]
fn fixup_builtin_literals_do_not_launder_moved_or_cloned_arguments() {
    let budget = ExpansionBudget::default();
    let arguments = stack(r"\html@mathml");
    let output = Template::BuiltinText("#1+#1")
        .expand(vec![arguments], &budget, None)
        .unwrap();
    assert_eq!(
        output
            .iter()
            .filter(|token| token.text == r"\html@mathml")
            .count(),
        2
    );
    assert!(
        output
            .iter()
            .filter(|token| token.text == r"\html@mathml")
            .all(|token| !token.is_builtin_literal)
    );
    assert!(
        output
            .iter()
            .filter(|token| token.text == "+")
            .all(|token| token.is_builtin_literal)
    );
}

#[test]
fn fixup_all_nonexpansion_exits_propagate_the_void_push_first_failure() {
    use ratex_types::{
        execution::{ExecutionError, Limits, Observer},
        storage::StorageLedger,
    };
    use std::{
        cell::Cell,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct At<'a> {
        flag: &'a AtomicBool,
        ordinal: Cell<u64>,
    }
    impl Observer for At<'_> {
        fn checkpoint(&self, phase: Phase, ordinal: u64) {
            if phase == Phase::Macro && ordinal == self.ordinal.get() {
                self.flag.store(true, Ordering::Release);
            }
        }
    }
    for (text, noexpand, inactive) in [("x", false, false), ("x", true, false), ("~", false, true)]
    {
        let cancel = AtomicBool::new(false);
        let observer = At {
            flag: &cancel,
            ordinal: Cell::new(u64::MAX),
        };
        let control =
            ExecutionControl::new(&cancel, None, Limits::default()).with_observer(&observer);
        let storage = StorageLedger::new(&control);
        let depth = DepthBudget::new_bounded(16, &storage).unwrap();
        let mut me = MacroExpander::new_bounded("", Mode::Math, depth, &storage).unwrap();
        let mut token = Token::new(text, 0, 1);
        token.noexpand = noexpand;
        me.push_token(token);
        if inactive {
            me.lexer.set_catcode('~', 12);
        }
        // future checkpoint + post-pop checkpoint + the pre-push storage checkpoint.
        observer
            .ordinal
            .set(control.checkpoint_counts()[Phase::Macro as usize] + 3);
        assert!(
            me.expand_once(false).is_err(),
            "{text} noexpand={noexpand} inactive={inactive}"
        );
        assert!(matches!(
            control.failure(),
            Some(ExecutionError::Cancelled {
                phase: Phase::Macro
            })
        ));
        assert!(me.expand_next_token().is_err());
        assert!(matches!(
            control.failure(),
            Some(ExecutionError::Cancelled {
                phase: Phase::Macro
            })
        ));
    }
}

#[test]
fn fixup3_builtin_text_or_math_moves_selected_user_tokens_without_granting_trust() {
    use ratex_types::{execution::Limits, storage::StorageLedger};
    use std::sync::atomic::AtomicBool;
    for (mode, expected) in [(Mode::Math, "y"), (Mode::Text, "x")] {
        let cancel = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancel, None, Limits::default());
        let storage = StorageLedger::new(&control);
        let depth = DepthBudget::new_bounded(16, &storage).unwrap();
        let mut me = MacroExpander::new_bounded("{x}{y}", mode, depth, &storage).unwrap();
        let mut command = me.token(r"\TextOrMath", 0, 11).unwrap();
        command.is_builtin_literal = true;
        me.push_token(command);
        assert!(me.expand_once(false).unwrap());
        let token = me.pop_token();
        assert_eq!(token.text, expected);
        assert!(!token.is_builtin_literal);
        storage.check(Phase::Macro).unwrap();
    }
}

#[test]
fn fixup3_builtin_text_or_math_argument_producer_preserves_cancelled_first_error() {
    use ratex_types::{
        execution::{ExecutionError, Limits, Observer},
        storage::StorageLedger,
    };
    use std::{
        cell::Cell,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct At<'a> {
        flag: &'a AtomicBool,
        ordinal: Cell<u64>,
    }
    impl Observer for At<'_> {
        fn checkpoint(&self, phase: Phase, ordinal: u64) {
            if phase == Phase::Macro && ordinal == self.ordinal.get() {
                self.flag.store(true, Ordering::Release);
            }
        }
    }
    let cancel = AtomicBool::new(false);
    let observer = At {
        flag: &cancel,
        ordinal: Cell::new(u64::MAX),
    };
    let control = ExecutionControl::new(&cancel, None, Limits::default()).with_observer(&observer);
    let storage = StorageLedger::new(&control);
    let depth = DepthBudget::new_bounded(16, &storage).unwrap();
    let mut me = MacroExpander::new_bounded("{x}{y}", Mode::Math, depth, &storage).unwrap();
    let mut command = me.token(r"\TextOrMath", 0, 11).unwrap();
    command.is_builtin_literal = true;
    me.push_token(command);
    // future + post-pop + count_expansion + consume_args first iteration.
    observer
        .ordinal
        .set(control.checkpoint_counts()[Phase::Macro as usize] + 4);
    assert!(me.expand_once(false).is_err());
    assert_eq!(me.expansion_count, 1);
    assert!(matches!(
        control.failure(),
        Some(ExecutionError::Cancelled {
            phase: Phase::Macro
        })
    ));
    assert!(me.expand_next_token().is_err());
    assert!(matches!(
        control.failure(),
        Some(ExecutionError::Cancelled {
            phase: Phase::Macro
        })
    ));
}
