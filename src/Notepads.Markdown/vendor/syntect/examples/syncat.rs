use getopts::Options;
use std::borrow::Cow;
use std::io::BufRead;
use std::path::Path;
use syntect::dumps::{dump_to_file, from_dump_file};
use syntect::easy::HighlightFile;
use syntect::highlighting::{Style, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

fn load_theme(tm_file: &str, enable_caching: bool) -> Theme {
    let tm_path = Path::new(tm_file);

    if enable_caching {
        let tm_cache = tm_path.with_extension("tmdump");

        if tm_cache.exists() {
            from_dump_file(tm_cache).unwrap()
        } else {
            let theme = ThemeSet::get_theme(tm_path).unwrap();
            dump_to_file(&theme, tm_cache).unwrap();
            theme
        }
    } else {
        ThemeSet::get_theme(tm_path).unwrap()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut opts = Options::new();
    opts.optflag("l", "list-file-types", "Lists supported file types");
    opts.optflag(
        "L",
        "list-embedded-themes",
        "Lists themes present in the executable",
    );
    opts.optopt("t", "theme-file", "THEME_FILE", "Theme file to use. May be a path, or an embedded theme. Embedded themes will take precendence. Default: base16-ocean.dark");
    opts.optopt(
        "s",
        "extra-syntaxes",
        "SYNTAX_FOLDER",
        "Additional folder to search for .sublime-syntax files in.",
    );
    opts.optflag(
        "e",
        "no-default-syntaxes",
        "Doesn't load default syntaxes, intended for use with --extra-syntaxes.",
    );
    opts.optflag(
        "n",
        "no-newlines",
        "Uses the no newlines versions of syntaxes and dumps.",
    );
    opts.optflag("c", "cache-theme", "Cache the parsed theme file.");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            panic!("{}", f.to_string())
        }
    };

    let no_newlines = matches.opt_present("no-newlines");
    let mut ss = if matches.opt_present("no-default-syntaxes") {
        SyntaxSet::new()
    } else if no_newlines {
        SyntaxSet::load_defaults_nonewlines()
    } else {
        SyntaxSet::load_defaults_newlines()
    };

    if let Some(folder) = matches.opt_str("extra-syntaxes") {
        let mut builder = ss.into_builder();
        builder.add_from_folder(folder, !no_newlines).unwrap();
        ss = builder.build();
    }

    let ts = ThemeSet::load_defaults();

    if matches.opt_present("list-file-types") {
        println!("Supported file types:");

        for sd in ss.syntaxes() {
            println!("- {} (.{})", sd.name, sd.file_extensions.join(", ."));
        }
    } else if matches.opt_present("list-embedded-themes") {
        println!("Embedded themes:");

        for t in ts.themes.keys() {
            println!("- {}", t);
        }
    } else if matches.free.is_empty() {
        let brief = format!("USAGE: {} [options] FILES", args[0]);
        println!("{}", opts.usage(&brief));
    } else {
        let theme_file: String = matches
            .opt_str("theme-file")
            .unwrap_or_else(|| "base16-ocean.dark".to_string());

        let theme = ts
            .themes
            .get(&theme_file)
            .map(Cow::Borrowed)
            .unwrap_or_else(|| {
                Cow::Owned(load_theme(&theme_file, matches.opt_present("cache-theme")))
            });

        for src in &matches.free[..] {
            if matches.free.len() > 1 {
                println!("==> {} <==", src);
            }

            let mut highlighter = HighlightFile::new(src, &ss, &theme).unwrap();

            // We use read_line instead of `for line in highlighter.reader.lines()` because that
            // doesn't return strings with a `\n`, and including the `\n` gets us more robust highlighting.
            // See the documentation for `SyntaxSetBuilder::add_from_folder`.
            // It also allows re-using the line buffer, which should be a tiny bit faster.
            let mut line = String::new();
            while highlighter.reader.read_line(&mut line).unwrap() > 0 {
                if no_newlines && line.ends_with('\n') {
                    let _ = line.pop();
                }

                {
                    let regions: Vec<(Style, &str)> = highlighter
                        .highlight_lines
                        .highlight_line(&line, &ss)
                        .unwrap();
                    print!("{}", as_24_bit_terminal_escaped(&regions[..], true));
                }
                line.clear();

                if no_newlines {
                    println!();
                }
            }

            // Clear the formatting
            println!("\x1b[0m");
        }
    }
}
