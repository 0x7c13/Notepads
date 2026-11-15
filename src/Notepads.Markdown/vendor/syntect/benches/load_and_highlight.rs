mod highlight_utils;
mod utils;

/// Measures the time it takes to run the whole pipeline:
///  1. Load assets
///  2. Parse
///  3. Highlight
fn run(b: &mut criterion::Bencher, file: &str) {
    let path = utils::get_test_file_path(file);

    b.iter(|| {
        let ss = syntect::parsing::SyntaxSet::load_defaults_nonewlines();
        let ts = syntect::highlighting::ThemeSet::load_defaults();

        let syntax = ss.find_syntax_for_file(path).unwrap().unwrap();
        let s = std::fs::read_to_string(path).unwrap();

        highlight_utils::do_highlight(&s, &ss, syntax, &ts.themes["base16-ocean.dark"]);
    })
}

fn load_and_highlight_benchmark(c: &mut criterion::Criterion) {
    let mut group = c.benchmark_group("load_and_highlight");
    for input in &[
        "highlight_test.erb",
        "InspiredGitHub.tmTheme",
        "Ruby.sublime-syntax",
        "parser.rs",
    ] {
        group.bench_with_input(format!("\"{}\"", input), input, |b, s| run(b, s));
    }
    group.finish();
}

criterion::criterion_group! {
    name = benches;
    config = criterion::Criterion::default().sample_size(50);
    targets = load_and_highlight_benchmark
}
criterion::criterion_main!(benches);
