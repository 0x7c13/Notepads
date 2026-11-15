use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use std::time::Duration;
use syntect::parsing::{ParseState, SyntaxReference, SyntaxSet};

mod utils;

fn do_parse(s: &str, ss: &SyntaxSet, syntax: &SyntaxReference) -> usize {
    let mut state = ParseState::new(syntax);
    let mut count = 0;
    for line in s.lines() {
        let ops = state.parse_line(line, ss).unwrap();
        count += ops.len();
    }
    count
}

fn parse_file(b: &mut Bencher, file: &str) {
    let path = utils::get_test_file_path(file);

    // don't load from dump so we don't count lazy regex compilation time
    let ss = SyntaxSet::load_defaults_nonewlines();

    let syntax = ss.find_syntax_for_file(path).unwrap().unwrap();
    let s = std::fs::read_to_string(path).unwrap();

    b.iter(|| do_parse(&s, &ss, syntax));
}

fn parsing_benchmark(c: &mut Criterion) {
    let mut parse = c.benchmark_group("parse");
    for input in &[
        "highlight_test.erb",
        "InspiredGitHub.tmTheme",
        "Ruby.sublime-syntax",
        "jquery.js",
        "parser.rs",
        "scope.rs",
    ] {
        parse.bench_with_input(format!("\"{}\"", input), input, |b, s| parse_file(b, s));
    }
    parse.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(50).warm_up_time(Duration::from_secs(30));
    targets = parsing_benchmark
}
criterion_main!(benches);
