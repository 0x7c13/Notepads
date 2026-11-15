use std::process::Command;

use ratex_layout::{
    layout, layout_with_fallback_metrics, to_display_list, validate_layout, LayoutOptions,
};
use ratex_parser::parser::parse;

const CHILD_ENV: &str = "RATEX_STACK_SAFETY_CHILD";

fn nested(prefix: &str, suffix: &str, depth: usize) -> String {
    format!("{}x{}", prefix.repeat(depth), suffix.repeat(depth))
}

fn nested_fraction(depth: usize) -> String {
    format!(r"{}x{}", r"\frac{1}{".repeat(depth), "}".repeat(depth))
}

fn nested_superscript(depth: usize) -> String {
    format!(r"{}x{}", r"x^{".repeat(depth), "}".repeat(depth))
}

fn unbraced_command_chain(command: &str, depth: usize) -> String {
    format!("{}x", command.repeat(depth))
}

fn nested_tag(depth: usize) -> String {
    format!("{}{{x}}", r"\tag".repeat(depth))
}

fn nested_braket(depth: usize) -> String {
    let mut input = "x".to_string();
    for _ in 0..depth {
        input = format!(r"\Braket{{{input}}}");
    }
    input
}

fn nested_raisebox(depth: usize) -> String {
    let mut body = "x".to_owned();
    for _ in 0..depth {
        body = format!(r"\raisebox{{0pt}}{{{body}}}");
    }
    body
}

fn environment_with_explicit_tag(environment: &str, tag: &str) -> String {
    let body = if environment == "align" {
        r"x &= y"
    } else {
        "x"
    };
    format!(r"\begin{{{environment}}}{body}\tag{{{tag}}}\end{{{environment}}}")
}

fn unary_prooftree(inferences: usize) -> String {
    format!(
        r"\begin{{prooftree}}\AxiomC{{P}}{}\end{{prooftree}}",
        r"\UnaryInfC{P}".repeat(inferences)
    )
}

fn braced_body(depth: usize) -> String {
    format!("{}x{}", "{".repeat(depth), "}".repeat(depth))
}

fn prooftree_axiom_payload(payload_depth: usize) -> String {
    format!(
        r"\begin{{prooftree}}\AxiomC{{{}}}\end{{prooftree}}",
        braced_body(payload_depth)
    )
}

fn prooftree_left_label_payload(payload_depth: usize) -> String {
    format!(
        r"\begin{{prooftree}}\LeftLabel{{{}}}\AxiomC{{P}}\UnaryInfC{{Q}}\end{{prooftree}}",
        braced_body(payload_depth)
    )
}

fn assert_pipeline_ok(input: &str) {
    let ast = parse(input).unwrap_or_else(|error| panic!("failed to parse bounded input: {error}"));
    let layout = layout(&ast, &LayoutOptions::default());
    validate_layout(&layout).unwrap();
    let expected = serde_json::to_value(to_display_list(&layout)).unwrap();
    let with_metrics = layout_with_fallback_metrics(
        &ast,
        &LayoutOptions::default(),
        std::sync::Arc::new(Default::default()),
    );
    validate_layout(&with_metrics).unwrap();
    assert_eq!(
        serde_json::to_value(to_display_list(&with_metrics)).unwrap(),
        expected
    );
}

fn assert_recursion_limit(input: &str) {
    let error = parse(input).expect_err("over-limit input unexpectedly parsed");
    assert!(
        error.to_string().contains("Recursion limit exceeded"),
        "unexpected error: {error}"
    );
}

fn assert_parse_error(input: &str) {
    parse(input).expect_err("adversarial input unexpectedly parsed");
}

fn run_small_stack_cases() {
    // Debug parser frames are intentionally much larger than production
    // frames. Verify the supported boundary on the constrained stack in the
    // release configuration used by platform bindings.
    #[cfg(not(debug_assertions))]
    run_boundary_cases();

    let over_limit_cases = [
        ("group-33", nested("{", "}", 33)),
        ("sqrt-33", nested(r"\sqrt{", "}", 33)),
        ("slashed-33", nested(r"\slashed{", "}", 33)),
        ("centernot-33", nested(r"\centernot{", "}", 33)),
        ("frac-33", nested_fraction(33)),
        ("left-right-33", nested(r"\left(", r"\right)", 33)),
        ("superscript-33", nested_superscript(33)),
        ("dots-33", r"\dots".repeat(33)),
    ];
    for (name, input) in &over_limit_cases {
        eprintln!("stack-safety over-limit case: {name}");
        assert_recursion_limit(input);
    }

    let adversarial_cases = [
        ("group-300", nested("{", "}", 300)),
        ("sqrt-300", nested(r"\sqrt{", "}", 300)),
        ("slashed-300", nested(r"\slashed{", "}", 300)),
        ("centernot-300", nested(r"\centernot{", "}", 300)),
        ("frac-300", nested_fraction(300)),
        ("left-right-300", nested(r"\left(", r"\right)", 300)),
        ("superscript-300", nested_superscript(300)),
        ("dots-300", r"\dots".repeat(300)),
        ("tag-4200", nested_tag(4_200)),
        ("braket-300", nested_braket(300)),
        ("prooftree-axiom-payload-32", prooftree_axiom_payload(32)),
        (
            "prooftree-left-label-payload-32",
            prooftree_left_label_payload(32),
        ),
        (
            "equation-explicit-tag-raisebox-12",
            environment_with_explicit_tag("equation", &nested_raisebox(12)),
        ),
        (
            "align-explicit-tag-raisebox-12",
            environment_with_explicit_tag("align", &nested_raisebox(12)),
        ),
    ];
    for (name, input) in &adversarial_cases {
        eprintln!("stack-safety adversarial case: {name}");
        assert_recursion_limit(input);
    }

    let accents_4200 = "\u{301}".repeat(4_200);
    assert_recursion_limit(&format!("x{accents_4200}"));
    assert_recursion_limit(&format!(r"\text{{x{accents_4200}}}"));
    assert_recursion_limit(&format!(r"\text{{x{}}}", "\u{301}".repeat(32)));
    assert_recursion_limit(&unary_prooftree(32));

    // Unbraced primitive/function arguments must also terminate with a regular
    // parse error. This guards against accidentally turning command chains such
    // as `\sqrt\sqrt...x` into an unbounded parse_group -> parse_function path
    // that bypasses the expression-depth counter.
    eprintln!("stack-safety parse-error case: unbraced-sqrt-4200");
    assert_parse_error(&unbraced_command_chain(r"\sqrt", 4_200));
    assert_parse_error(&unbraced_command_chain(r"\slashed", 300));
    assert_parse_error(&unbraced_command_chain(r"\centernot", 300));

    eprintln!("stack-safety parse-error case: unbraced-bigl-4200");
    assert_parse_error(&format!("{}(", r"\bigl".repeat(4_200)));

    eprintln!("stack-safety parse-error case: unbraced-left-4200");
    assert_parse_error(&format!("{}(", r"\left".repeat(4_200)));

    eprintln!("stack-safety flat lexer case: comments-4200");
    assert_pipeline_ok(&("%\n".repeat(4_200) + "x"));

    eprintln!("stack-safety flat prefix case: global-4200");
    assert_pipeline_ok(&format!(r"{}\def\foo{{x}}\foo", r"\global".repeat(4_200)));
}

fn run_boundary_cases() {
    let boundary_cases = [
        ("group", nested("{", "}", 32)),
        ("sqrt", nested(r"\sqrt{", "}", 32)),
        ("slashed", nested(r"\slashed{", "}", 32)),
        ("centernot", nested(r"\centernot{", "}", 32)),
        ("frac", nested_fraction(32)),
        ("left-right", nested(r"\left(", r"\right)", 32)),
        ("superscript", nested_superscript(32)),
        ("dots", r"\dots".repeat(32)),
    ];
    for (_name, input) in &boundary_cases {
        assert_pipeline_ok(input);
    }

    let accents_32 = "\u{301}".repeat(32);
    let accents_31 = "\u{301}".repeat(31);
    assert_pipeline_ok(&format!("x{accents_32}"));
    assert_pipeline_ok(&format!(r"\text{{x{accents_31}}}"));

    assert_pipeline_ok(&unary_prooftree(31));
    assert_pipeline_ok(&nested_tag(32));
    assert_pipeline_ok(&nested_braket(16));
    assert_pipeline_ok(&prooftree_axiom_payload(31));
    assert_pipeline_ok(&prooftree_left_label_payload(31));
}

#[test]
fn parser_driven_pipeline_is_safe_on_a_small_stack() {
    if cfg!(debug_assertions) {
        run_boundary_cases();
        run_small_stack_cases();
        return;
    }

    if std::env::var_os(CHILD_ENV).is_some() {
        std::thread::Builder::new()
            .name("ratex-512k-stack".into())
            .stack_size(512 * 1024)
            .spawn(run_small_stack_cases)
            .expect("failed to spawn small-stack thread")
            .join()
            .expect("small-stack test thread panicked");
        return;
    }

    run_boundary_cases();

    let status = Command::new(std::env::current_exe().expect("test executable path"))
        .arg("--exact")
        .arg("parser_driven_pipeline_is_safe_on_a_small_stack")
        .arg("--nocapture")
        .env(CHILD_ENV, "1")
        .status()
        .expect("failed to start isolated stack-safety test process");

    assert!(status.success(), "isolated stack-safety process failed");
}
