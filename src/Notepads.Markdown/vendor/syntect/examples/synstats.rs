//! An example of using syntect for code analysis.
//! Basically a fancy lines of code count program that works
//! for all languages Sublime Text supports and also counts things
//! like number of functions and number of types defined.
//!
//! Another thing it does that other line count programs can't always
//! do is properly count comments in embedded syntaxes. For example
//! JS, CSS and Ruby comments embedded in ERB files.
use syntect::easy::ScopeRegionIterator;
use syntect::highlighting::{ScopeSelector, ScopeSelectors};
use syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxSet};

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug)]
struct Selectors {
    comment: ScopeSelector,
    doc_comment: ScopeSelectors,
    function: ScopeSelector,
    types: ScopeSelectors,
}

impl Default for Selectors {
    fn default() -> Selectors {
        Selectors {
            comment: ScopeSelector::from_str("comment - comment.block.attribute").unwrap(),
            doc_comment: ScopeSelectors::from_str(
                "comment.line.documentation, comment.block.documentation",
            )
            .unwrap(),
            function: ScopeSelector::from_str("entity.name.function").unwrap(),
            types: ScopeSelectors::from_str(
                "entity.name.class, entity.name.struct, entity.name.enum, entity.name.type",
            )
            .unwrap(),
        }
    }
}

#[derive(Debug, Default)]
struct Stats {
    selectors: Selectors,
    files: usize,
    functions: usize,
    types: usize,
    lines: usize,
    chars: usize,
    code_lines: usize,
    comment_lines: usize,
    comment_chars: usize,
    comment_words: usize,
    doc_comment_lines: usize,
    doc_comment_words: usize,
}

fn print_stats(stats: &Stats) {
    println!();
    println!("################## Stats ###################");
    println!("File count:                           {:>6}", stats.files);
    println!("Total characters:                     {:>6}", stats.chars);
    println!();
    println!(
        "Function count:                       {:>6}",
        stats.functions
    );
    println!("Type count (structs, enums, classes): {:>6}", stats.types);
    println!();
    println!(
        "Code lines (traditional SLOC):        {:>6}",
        stats.code_lines
    );
    println!("Total lines (w/ comments & blanks):   {:>6}", stats.lines);
    println!(
        "Comment lines (comment but no code):  {:>6}",
        stats.comment_lines
    );
    println!(
        "Blank lines (lines-blank-comment):    {:>6}",
        stats.lines - stats.code_lines - stats.comment_lines
    );
    println!();
    println!(
        "Lines with a documentation comment:   {:>6}",
        stats.doc_comment_lines
    );
    println!(
        "Total words written in doc comments:  {:>6}",
        stats.doc_comment_words
    );
    println!(
        "Total words written in all comments:  {:>6}",
        stats.comment_words
    );
    println!(
        "Characters of comment:                {:>6}",
        stats.comment_chars
    );
}

fn is_ignored(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.') && s.len() > 1 || s.ends_with(".md"))
        .unwrap_or(false)
}

fn count_line(
    ops: &[(usize, ScopeStackOp)],
    line: &str,
    stack: &mut ScopeStack,
    stats: &mut Stats,
) {
    stats.lines += 1;

    let mut line_has_comment = false;
    let mut line_has_doc_comment = false;
    let mut line_has_code = false;
    for (s, op) in ScopeRegionIterator::new(ops, line) {
        stack.apply(op).unwrap();
        if s.is_empty() {
            // in this case we don't care about blank tokens
            continue;
        }
        if stats
            .selectors
            .comment
            .does_match(stack.as_slice())
            .is_some()
        {
            let words = s
                .split_whitespace()
                .filter(|w| {
                    w.chars()
                        .all(|c| c.is_alphanumeric() || c == '.' || c == '\'')
                })
                .count();
            if stats
                .selectors
                .doc_comment
                .does_match(stack.as_slice())
                .is_some()
            {
                line_has_doc_comment = true;
                stats.doc_comment_words += words;
            }
            stats.comment_chars += s.len();
            stats.comment_words += words;
            line_has_comment = true;
        } else if !s.chars().all(|c| c.is_whitespace()) {
            line_has_code = true;
        }
        if stats
            .selectors
            .function
            .does_match(stack.as_slice())
            .is_some()
        {
            stats.functions += 1;
        }
        if stats.selectors.types.does_match(stack.as_slice()).is_some() {
            stats.types += 1;
        }
    }
    if line_has_comment && !line_has_code {
        stats.comment_lines += 1;
    }
    if line_has_doc_comment {
        stats.doc_comment_lines += 1;
    }
    if line_has_code {
        stats.code_lines += 1;
    }
}

fn count(ss: &SyntaxSet, path: &Path, stats: &mut Stats) {
    let syntax = match ss.find_syntax_for_file(path).unwrap_or(None) {
        Some(syntax) => syntax,
        None => return,
    };
    stats.files += 1;
    let mut state = ParseState::new(syntax);

    let f = File::open(path).unwrap();
    let mut reader = BufReader::new(f);
    let mut line = String::new();
    let mut stack = ScopeStack::new();
    while reader.read_line(&mut line).unwrap() > 0 {
        {
            let ops = state.parse_line(&line, ss).unwrap();
            stats.chars += line.len();
            count_line(&ops, &line, &mut stack, stats);
        }
        line.clear();
    }
}

fn main() {
    let ss = SyntaxSet::load_defaults_newlines(); // note we load the version with newlines

    let args: Vec<String> = std::env::args().collect();
    let path = if args.len() < 2 { "." } else { &args[1] };

    println!("################## Files ###################");
    let mut stats = Stats::default();
    let walker = WalkDir::new(path).into_iter();
    for entry in walker.filter_entry(|e| !is_ignored(e)) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            println!("{}", entry.path().display());
            count(&ss, entry.path(), &mut stats);
        }
    }

    // println!("{:?}", stats);
    print_stats(&stats);
}
