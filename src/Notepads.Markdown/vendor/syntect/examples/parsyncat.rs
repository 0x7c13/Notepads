//! Highlights the files given on the command line, in parallel.
//! Prints the highlighted output to stdout.

use rayon::prelude::*;
use syntect::easy::HighlightFile;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;

use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();

    if files.is_empty() {
        println!("Please provide some files to highlight.");
        return;
    }

    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();

    // We first collect the contents of the files...
    let contents: Vec<Vec<String>> = files
        .par_iter()
        .map(|filename| {
            let mut lines = Vec::new();
            // We use `String::new()` and `read_line()` instead of `BufRead::lines()`
            // in order to preserve the newlines and get better highlighting.
            let mut line = String::new();
            let mut reader = BufReader::new(File::open(filename).unwrap());
            while reader.read_line(&mut line).unwrap() > 0 {
                lines.push(line);
                line = String::new();
            }
            lines
        })
        .collect();

    // ...so that the highlighted regions have valid lifetimes...
    let regions: Vec<Vec<(Style, &str)>> = files
        .par_iter()
        .zip(&contents)
        .map(|(filename, contents)| {
            let mut regions = Vec::new();
            let theme = &theme_set.themes["base16-ocean.dark"];
            let mut highlighter = HighlightFile::new(filename, &syntax_set, theme).unwrap();

            for line in contents {
                for region in highlighter
                    .highlight_lines
                    .highlight_line(line, &syntax_set)
                    .unwrap()
                {
                    regions.push(region);
                }
            }

            regions
        })
        .collect();

    // ...and then print them all out.
    for file_regions in regions {
        print!(
            "{}",
            syntect::util::as_24_bit_terminal_escaped(&file_regions[..], true)
        );
    }
}
