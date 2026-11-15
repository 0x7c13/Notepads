use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use std::str::FromStr;
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::{ScopeStack, SyntaxSet};

mod highlight_utils;
mod utils;

fn highlight_file(b: &mut Bencher, file: &str) {
    let path = utils::get_test_file_path(file);

    // don't load from dump so we don't count lazy regex compilation time
    let ss = SyntaxSet::load_defaults_nonewlines();
    let ts = ThemeSet::load_defaults();

    let syntax = ss.find_syntax_for_file(path).unwrap().unwrap();
    let s = std::fs::read_to_string(path).unwrap();

    b.iter(|| highlight_utils::do_highlight(&s, &ss, syntax, &ts.themes["base16-ocean.dark"]));
}

fn stack_matching(b: &mut Bencher) {
    let s = "source.js meta.group.js meta.group.js meta.block.js meta.function-call.method.js meta.group.js meta.object-literal.js meta.block.js meta.function-call.method.js meta.group.js variable.other.readwrite.js";
    let stack = ScopeStack::from_str(s).unwrap();
    let selector = ScopeStack::from_str("source meta.function-call.method").unwrap();
    b.iter(|| selector.does_match(stack.as_slice()));
}

fn highlight_html(b: &mut Bencher) {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let path = "testdata/parser.rs";
    let syntax = ss.find_syntax_for_file(path).unwrap().unwrap();
    let s = std::fs::read_to_string(path).unwrap();

    b.iter(|| highlighted_html_for_string(&s, &ss, syntax, &ts.themes["base16-ocean.dark"]));
}

fn highlighting_benchmark(c: &mut Criterion) {
    c.bench_function("stack_matching", stack_matching);
    c.bench_function("highlight_html", highlight_html);
    let mut highlight = c.benchmark_group("highlight");
    for input in &[
        "highlight_test.erb",
        "InspiredGitHub.tmTheme",
        "Ruby.sublime-syntax",
        "jquery.js",
        "parser.rs",
        "scope.rs",
    ] {
        highlight.bench_with_input(format!("\"{}\"", input), input, |b, s| highlight_file(b, s));
    }
    highlight.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = highlighting_benchmark
}
criterion_main!(benches);
