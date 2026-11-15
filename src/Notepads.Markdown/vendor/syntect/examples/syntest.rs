//! An example of using syntect for testing syntax definitions.
//! Basically exactly the same as what Sublime Text can do,
//! but without needing ST installed
// To run tests only for a particular package, while showing the operations, you could use:
// cargo run --example syntest -- --debug testdata/Packages/Makefile/
// to specify that the syntax definitions should be parsed instead of loaded from the dump file,
// you can tell it where to parse them from - the following will execute only 1 syntax test after
// parsing the sublime-syntax files in the JavaScript folder:
// cargo run --example syntest testdata/Packages/JavaScript/syntax_test_json.json testdata/Packages/JavaScript/

use syntect::easy::ScopeRegionIterator;
use syntect::highlighting::ScopeSelectors;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet, SyntaxSetBuilder};

use std::cmp::{max, min};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;
use std::time::Instant;

use getopts::Options;
use once_cell::sync::Lazy;
use regex::Regex;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxTestHeaderError {
    MalformedHeader,
    SyntaxDefinitionNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxTestFileResult {
    FailedAssertions(usize, usize),
    Success(usize),
}

pub static SYNTAX_TEST_HEADER_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?xm)
        ^(?P<testtoken_start>\s*\S+)
        \s+SYNTAX\sTEST\s+
        "(?P<syntax_file>[^"]+)"
        \s*(?P<testtoken_end>\S+)?$
    "#,
    )
    .unwrap()
});
pub static SYNTAX_TEST_ASSERTION_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?xm)
    \s*(?:
        (?P<begin_of_token><-)|(?P<range>\^+)
    )(.*)$"#,
    )
    .unwrap()
});

#[derive(Clone, Copy)]
struct OutputOptions {
    time: bool,
    debug: bool,
    summary: bool,
}

#[derive(Debug)]
struct AssertionRange<'a> {
    begin_char: usize,
    end_char: usize,
    scope_selector_text: &'a str,
    is_pure_assertion_line: bool,
}

#[derive(Debug)]
struct ScopedText {
    scope: Vec<Scope>,
    char_start: usize,
    text_len: usize,
}

#[derive(Debug)]
struct RangeTestResult {
    column_begin: usize,
    column_end: usize,
    success: bool,
}

fn get_line_assertion_details<'a>(
    testtoken_start: &str,
    testtoken_end: Option<&str>,
    line: &'a str,
) -> Option<AssertionRange<'a>> {
    // if the test start token specified in the test file's header is on the line
    if let Some(index) = line.find(testtoken_start) {
        let (before_token_start, token_and_rest_of_line) = line.split_at(index);

        if let Some(captures) =
            SYNTAX_TEST_ASSERTION_PATTERN.captures(&token_and_rest_of_line[testtoken_start.len()..])
        {
            let mut sst = captures.get(3).unwrap().as_str(); // get the scope selector text
            let mut only_whitespace_after_token_end = true;

            if let Some(token) = testtoken_end {
                // if there is an end token defined in the test file header
                if let Some(end_token_pos) = sst.find(token) {
                    // and there is an end token in the line
                    let (ss, after_token_end) = sst.split_at(end_token_pos); // the scope selector text ends at the end token
                    sst = ss;
                    only_whitespace_after_token_end = after_token_end.trim_end().is_empty();
                }
            }
            return Some(AssertionRange {
                begin_char: index
                    + if captures.get(2).is_some() {
                        testtoken_start.len() + captures.get(2).unwrap().start()
                    } else {
                        0
                    },
                end_char: index
                    + if captures.get(2).is_some() {
                        testtoken_start.len() + captures.get(2).unwrap().end()
                    } else {
                        1
                    },
                scope_selector_text: sst,
                is_pure_assertion_line: before_token_start.trim_start().is_empty()
                    && only_whitespace_after_token_end, // if only whitespace surrounds the test tokens on the line, then it is a pure assertion line
            });
        }
    }
    None
}

fn process_assertions(
    assertion: &AssertionRange<'_>,
    test_against_line_scopes: &[ScopedText],
) -> Vec<RangeTestResult> {
    // format the scope selector to include a space at the beginning, because, currently, ScopeSelector expects excludes to begin with " -"
    // and they are sometimes in the syntax test as ^^^-comment, for example
    let selector =
        ScopeSelectors::from_str(&format!(" {}", &assertion.scope_selector_text)).unwrap();
    // find the scope at the specified start column, and start matching the selector through the rest of the tokens on the line from there until the end column is reached
    let mut results = Vec::new();
    for scoped_text in test_against_line_scopes
        .iter()
        .skip_while(|s| s.char_start + s.text_len <= assertion.begin_char)
        .take_while(|s| s.char_start < assertion.end_char)
    {
        let match_value = selector.does_match(scoped_text.scope.as_slice());
        let result = RangeTestResult {
            column_begin: max(scoped_text.char_start, assertion.begin_char),
            column_end: min(
                scoped_text.char_start + scoped_text.text_len,
                assertion.end_char,
            ),
            success: match_value.is_some(),
        };
        results.push(result);
    }
    // don't ignore assertions after the newline, they should be treated as though they are asserting against the newline
    let last = test_against_line_scopes.last().unwrap();
    if last.char_start + last.text_len < assertion.end_char {
        let match_value = selector.does_match(last.scope.as_slice());
        let result = RangeTestResult {
            column_begin: max(last.char_start + last.text_len, assertion.begin_char),
            column_end: assertion.end_char,
            success: match_value.is_some(),
        };
        results.push(result);
    }
    results
}

/// If `parse_test_lines` is `false` then lines that only contain assertions are not parsed
fn test_file(
    ss: &SyntaxSet,
    path: &Path,
    parse_test_lines: bool,
    out_opts: OutputOptions,
) -> Result<SyntaxTestFileResult, SyntaxTestHeaderError> {
    use syntect::util::debug_print_ops;
    let f = File::open(path).unwrap();
    let mut reader = BufReader::new(f);
    let mut line = String::new();

    // read the first line from the file - if we have reached EOF already, it's an invalid file
    if reader.read_line(&mut line).unwrap() == 0 {
        return Err(SyntaxTestHeaderError::MalformedHeader);
    }

    line = line.replace('\r', "");

    // parse the syntax test header in the first line of the file
    let header_line = line.clone();
    let search_result = SYNTAX_TEST_HEADER_PATTERN.captures(&header_line);
    let captures = search_result.ok_or(SyntaxTestHeaderError::MalformedHeader)?;

    let testtoken_start = captures.name("testtoken_start").unwrap().as_str();
    let testtoken_end = captures.name("testtoken_end").map(|c| c.as_str());
    let syntax_file = captures.name("syntax_file").unwrap().as_str();

    // find the relevant syntax definition to parse the file with - case is important!
    if !out_opts.summary {
        println!(
            "The test file references syntax definition file: {}",
            syntax_file
        );
    }
    let syntax = ss
        .find_syntax_by_path(syntax_file)
        .ok_or(SyntaxTestHeaderError::SyntaxDefinitionNotFound)?;

    // iterate over the lines of the file, testing them
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();

    let mut current_line_number = 1;
    let mut test_against_line_number = 1;
    let mut scopes_on_line_being_tested = Vec::new();
    let mut previous_non_assertion_line = line.to_string();

    let mut assertion_failures: usize = 0;
    let mut total_assertions: usize = 0;

    loop {
        // over lines of file, starting with the header line
        let mut line_only_has_assertion = false;
        let mut line_has_assertion = false;
        if let Some(assertion) = get_line_assertion_details(testtoken_start, testtoken_end, &line) {
            let result = process_assertions(&assertion, &scopes_on_line_being_tested);
            total_assertions += assertion.end_char - assertion.begin_char;
            for failure in result.iter().filter(|r| !r.success) {
                let length = failure.column_end - failure.column_begin;
                let text: String = previous_non_assertion_line
                    .chars()
                    .skip(failure.column_begin)
                    .take(length)
                    .collect();
                if !out_opts.summary {
                    println!(
                        "  Assertion selector {:?} \
                        from line {:?} failed against line {:?}, column range {:?}-{:?} \
                        (with text {:?}) \
                        has scope {:?}",
                        assertion.scope_selector_text.trim(),
                        current_line_number,
                        test_against_line_number,
                        failure.column_begin,
                        failure.column_end,
                        text,
                        scopes_on_line_being_tested
                            .iter()
                            .find(|s| s.char_start + s.text_len > failure.column_begin)
                            .unwrap_or_else(|| scopes_on_line_being_tested.last().unwrap())
                            .scope
                    );
                }
                assertion_failures += failure.column_end - failure.column_begin;
            }
            line_only_has_assertion = assertion.is_pure_assertion_line;
            line_has_assertion = true;
        }
        if !line_only_has_assertion || parse_test_lines {
            if !line_has_assertion {
                // ST seems to ignore lines that have assertions when calculating which line the assertion tests against
                scopes_on_line_being_tested.clear();
                test_against_line_number = current_line_number;
                previous_non_assertion_line = line.to_string();
            }
            if out_opts.debug && !line_only_has_assertion {
                println!(
                    "-- debugging line {} -- scope stack: {:?}, -- parse state: {:?}",
                    current_line_number, stack, state
                );
            }
            let ops = state.parse_line(&line, ss).unwrap();
            if out_opts.debug && !line_only_has_assertion {
                if ops.is_empty() && !line.is_empty() {
                    println!("no operations for this line...");
                } else {
                    debug_print_ops(&line, &ops);
                }
            }
            let mut col: usize = 0;
            for (s, op) in ScopeRegionIterator::new(&ops, &line) {
                stack.apply(op).unwrap();
                if s.is_empty() {
                    // in this case we don't care about blank tokens
                    continue;
                }
                if !line_has_assertion {
                    // if the line has no assertions on it, remember the scopes on the line so we can test against them later
                    let len = s.chars().count();
                    scopes_on_line_being_tested.push(ScopedText {
                        char_start: col,
                        text_len: len,
                        scope: stack.as_slice().to_vec(),
                    });
                    // TODO: warn when there are duplicate adjacent (non-meta?) scopes, as it is almost always undesired
                    col += len;
                }
            }
        }

        line.clear();
        current_line_number += 1;
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        line = line.replace('\r', "");
    }
    let res = if assertion_failures > 0 {
        Ok(SyntaxTestFileResult::FailedAssertions(
            assertion_failures,
            total_assertions,
        ))
    } else {
        Ok(SyntaxTestFileResult::Success(total_assertions))
    };

    if out_opts.summary {
        if let Ok(SyntaxTestFileResult::FailedAssertions(failures, _)) = res {
            // Don't print total assertion count so that diffs don't pick up new succeeding tests
            println!("FAILED {}: {}", path.display(), failures);
        }
    } else {
        println!("{:?}", res);
    }

    res
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut opts = Options::new();
    opts.optflag("d", "debug", "Show parsing results for each test line");
    opts.optflag(
        "t",
        "time",
        "Time execution as a more broad-ranging benchmark",
    );
    opts.optflag("s", "summary", "Print only summary of test failures");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            panic!("{}", f.to_string())
        }
    };

    let tests_path = if matches.free.is_empty() {
        "."
    } else {
        &args[1]
    };

    let syntaxes_path = if matches.free.len() < 2 { "" } else { &args[2] };

    // load the syntaxes from disk if told to
    // (as opposed to from the binary dumps)
    // this helps to ensure that a recompile isn't needed
    // when using this for syntax development
    let mut ss = if syntaxes_path.is_empty() {
        SyntaxSet::load_defaults_newlines() // note we load the version with newlines
    } else {
        SyntaxSet::new()
    };
    if !syntaxes_path.is_empty() {
        println!("loading syntax definitions from {}", syntaxes_path);
        let mut builder = SyntaxSetBuilder::new();
        builder.add_from_folder(syntaxes_path, true).unwrap(); // note that we load the version with newlines
        ss = builder.build();
    }

    let out_opts = OutputOptions {
        debug: matches.opt_present("debug"),
        time: matches.opt_present("time"),
        summary: matches.opt_present("summary"),
    };

    let exit_code = recursive_walk(&ss, tests_path, out_opts);
    println!("exiting with code {}", exit_code);
    std::process::exit(exit_code);
}

fn recursive_walk(ss: &SyntaxSet, path: &str, out_opts: OutputOptions) -> i32 {
    let mut exit_code: i32 = 0; // exit with code 0 by default, if all tests pass
    let walker = WalkDir::new(path).into_iter();

    // accumulate and sort for consistency of diffs across machines
    let mut files = Vec::new();
    for entry in walker.filter_entry(|e| e.file_type().is_dir() || is_a_syntax_test_file(e)) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            files.push(entry.path().to_owned());
        }
    }
    files.sort();

    for path in &files {
        if !out_opts.summary {
            println!("Testing file {}", path.display());
        }
        let start = Instant::now();
        let result = test_file(ss, path, true, out_opts);
        let elapsed = start.elapsed();
        if out_opts.time {
            let ms = (elapsed.as_secs() * 1_000) + elapsed.subsec_millis() as u64;
            println!("{} ms for file {}", ms, path.display());
        }
        if exit_code != 2 {
            // leave exit code 2 if there was an error
            if result.is_err() {
                // set exit code 2 if there was an error
                exit_code = 2;
            } else if let Ok(SyntaxTestFileResult::FailedAssertions(_, _)) = result {
                exit_code = 1; // otherwise, if there were failures, exit with code 1
            }
        }
    }

    exit_code
}

fn is_a_syntax_test_file(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with("syntax_test_"))
        .unwrap_or(false)
}
