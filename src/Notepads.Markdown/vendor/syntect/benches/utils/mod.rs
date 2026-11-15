/// To be able to keep the same Criterion benchmark names as before (for the
/// `the --baseline` feature of Criterion) we use one level of indirection to
/// map file name to file path.
pub fn get_test_file_path(file: &str) -> &str {
    match file {
        "highlight_test.erb" => "testdata/highlight_test.erb",
        "InspiredGitHub.tmTheme" => "testdata/InspiredGitHub.tmtheme/InspiredGitHub.tmTheme",
        "Ruby.sublime-syntax" => "testdata/Packages/Ruby/Ruby.sublime-syntax",
        "jquery.js" => "testdata/jquery.js",
        "parser.rs" => "testdata/parser.rs",
        "scope.rs" => "src/parsing/scope.rs",
        _ => panic!("Unknown test file {}", file),
    }
}
