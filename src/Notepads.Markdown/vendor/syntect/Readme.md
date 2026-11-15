# syntect

[![Crates.io](https://img.shields.io/crates/v/syntect.svg?maxAge=2591000)](https://crates.io/crates/syntect)
[![Documentation](https://docs.rs/syntect/badge.svg)](https://docs.rs/syntect)
[![Crates.io](https://img.shields.io/crates/l/syntect.svg?maxAge=2591000)]()
[![Build Status](https://github.com/trishume/syntect/actions/workflows/CI.yml/badge.svg)](https://github.com/trishume/syntect/actions)
[![codecov](https://codecov.io/gh/trishume/syntect/branch/master/graph/badge.svg)](https://codecov.io/gh/trishume/syntect)

`syntect` is a syntax highlighting library for Rust that uses [Sublime Text syntax definitions](http://www.sublimetext.com/docs/3/syntax.html#include-syntax).
It aims to be a good solution for any Rust project that needs syntax highlighting, including deep integration with text editors written in Rust.
It's used in production by at least two companies, and by [many open source projects](#projects-using-syntect).

If you are writing a text editor (or something else needing highlighting) in Rust and this library doesn't fit your needs, I consider that a bug and you should file an issue or email me.
I consider this project mostly complete, I still maintain it and review PRs, but it's not under heavy development.

## Important Links

- API docs with examples: <https://docs.rs/syntect>
- [Changelogs and upgrade notes for past releases](https://github.com/trishume/syntect/releases)

## Getting Started

`syntect` is [available on crates.io](https://crates.io/crates/syntect). You can add it to your `Cargo.toml` with the following command

```bash
cargo add syntect
```

After that take a look at the [documentation](https://docs.rs/syntect) and the [examples](https://github.com/trishume/syntect/tree/master/examples).

If you've cloned this repository, be sure to run

```bash
git submodule update --init
```

to fetch all the required dependencies for running the tests.

## Features/Goals

- [x] Work with many languages (accomplished through using existing grammar formats)
- [x] Highlight super quickly, faster than nearly all text editors
- [x] Include easy to use API for basic cases
- [x] API allows use in fancy text editors with piece tables and incremental re-highlighting and the like.
- [x] Expose internals of the parsing process so text editors can do things like cache parse states and use semantic info for code intelligence
- [x] High quality highlighting, supporting things like heredocs and complex syntaxes (like Rust's).
- [x] Include a compressed dump of all the default syntax definitions in the library binary so users don't have to manage a folder of syntaxes.
- [x] Well documented, I've tried to add a useful documentation comment to everything that isn't utterly self explanatory.
- [x] Built-in output to coloured HTML `<pre>` tags or 24-bit colour ANSI terminal escape sequences.
- [x] Nearly complete compatibility with Sublime Text 3, including lots of edge cases. Passes nearly all of Sublime's syntax tests, see [issue 59](https://github.com/trishume/syntect/issues/59).
- [x] Load up quickly, currently in around 23ms but could potentially be even faster.

## Screenshots

There's currently an example program called `syncat` that prints one of the source files using hard-coded themes and syntaxes using 24-bit terminal escape sequences supported by many newer terminals.
These screenshots don't look as good as they could for two reasons:
first the sRGB colours aren't corrected properly, and second the Rust syntax definition uses some fancy labels that these themes don't have highlighting for.

![Nested languages](http://i.imgur.com/bByxb1E.png)
![Base 16 Ocean Dark](http://i.imgur.com/CwiPOwZ.png)
![Solarized Light](http://i.imgur.com/l3zcO4J.png)
![InspiredGithub](http://i.imgur.com/a7U1r2j.png)

## Example Code

Prints highlighted lines of a string to the terminal.
See the [easy](https://docs.rs/syntect/latest/syntect/easy/index.html) and [html](https://docs.rs/syntect/latest/syntect/html/index.html) module docs for more basic use case examples.

```rust
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::{ThemeSet, Style};
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

// Load these once at the start of your program
let ps = SyntaxSet::load_defaults_newlines();
let ts = ThemeSet::load_defaults();

let syntax = ps.find_syntax_by_extension("rs").unwrap();
let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);
let s = "pub struct Wow { hi: u64 }\nfn blah() -> u64 {}";
for line in LinesWithEndings::from(s) {
    let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps).unwrap();
    let escaped = as_24_bit_terminal_escaped(&ranges[..], true);
    print!("{}", escaped);
}
```

## Performance

Currently `syntect` is one of the faster syntax highlighting engines, but not the fastest. The following perf features are done:

- [x] Pre-link references between languages (e.g `<script>` tags) so there are no tree traversal string lookups in the hot-path
- [x] Compact binary representation of scopes to allow quickly passing and copying them around
- [x] Determine if a scope is a prefix of another scope using bit manipulation in only a few instructions
- [x] Cache regex matches to reduce number of times oniguruma is asked to search a line
- [x] Accelerate scope lookups to reduce how much selector matching has to be done to highlight a list of scope operations
- [x] Lazily compile regexes so startup time isn't taken compiling a thousand regexes for Actionscript that nobody will use
- [ ] Optionally use the fancy-regex crate. Unfortunately this isn't yet faster than oniguruma on our benchmarks but it might be in the future.

The current perf numbers are below.
These numbers may get better if more of the things above are implemented, but they're better than many other text editors.
All measurements were taken on a mid 2012 15" retina Macbook Pro, my new 2019 Macbook takes about 70% of these times.

- Highlighting 9200 lines/247kb of jQuery 2.1 takes 600ms. For comparison:
    - Textmate 2, Spacemacs and Visual Studio Code all take around 2ish seconds (measured by hand with a stopwatch, hence approximate).
    - Atom takes 6 seconds
    - Sublime Text 3 dev build takes `98ms` (highlighting only, takes `~200ms` click to pixels), despite having a super fancy javascript syntax definition.
    - Vim is instantaneous but that isn't a fair comparison since vim's highlighting is far more basic than the other editors.
      Compare [vim's grammar](https://github.com/vim/vim/blob/master/runtime/syntax/javascript.vim) to [Sublime's](https://github.com/sublimehq/Packages/blob/master/JavaScript/JavaScript.sublime-syntax).
    - These comparisons aren't totally fair, except the one to Sublime Text since that is using the same theme and the same complex definition for ES6 syntax.
- Simple syntaxes are faster, JS is one of the most complex.
  It only takes 34ms to highlight a 1700 line 62kb XML file or 50,000 lines/sec.
- `~138ms` to load and link all the syntax definitions in the default Sublime package set.
    - but only `~23ms` to load and link all the syntax definitions from an internal pre-made binary dump with lazy regex compilation.
- `~1.9ms` to parse and highlight the 30 line 791 character `testdata/highlight_test.erb` file. This works out to around 16,000 lines/second or 422 kilobytes/second.
- `~250ms` end to end for `syncat` to start, load the definitions, highlight the test file and shut down.
  This is mostly spent loading.

## Feature Flags

Syntect makes heavy use of [cargo features](http://doc.crates.io/manifest.html#the-features-section), to support users who require only a subset of functionality.
In particular, it is possible to use the highlighting component of syntect without the parser (for instance when hand-rolling a higher performance parser for a particular language), by adding `default-features = false` to the syntect entry in your `Cargo.toml`.

For more information on available features, see the features section in `Cargo.toml`.

## Pure Rust `fancy-regex` mode, without `onig`

Since 4.0 `syntect` offers an alternative pure-rust regex engine based on the [fancy-regex](https://github.com/fancy-regex/fancy-regex) engine which extends the awesome [regex crate](https://github.com/rust-lang/regex) with support for fancier regex features that Sublime syntaxes need like lookaheads.

The advantage of `fancy-regex` is that it does not require the [onig crate](https://github.com/rust-onig/rust-onig) which requires building and linking the Oniguruma C library. Many users experience difficulty building the `onig` crate, especially on Windows and Webassembly.

As far as our tests can tell this new engine is just as correct, but it hasn't been tested as extensively in production. It also currently seems to be about **half the speed** of the default Oniguruma engine, although further testing and optimization (perhaps by you!) may eventually see it surpass Oniguruma's speed and become the default.

To use the fancy-regex engine with syntect, add it to your `Cargo.toml` like so:

```toml
syntect = { version = "4.2", default-features = false, features = ["default-fancy"]}
```

If you want to run examples with the fancy-regex engine you can use a command line like the following:

```bash
cargo run --features default-fancy --no-default-features --release --example syncat testdata/highlight_test.erb
```

Due to the way Cargo features work, if any crate you depend on depends on `syntect` without enabling `fancy-regex` then you'll get the default `onig` mode.

**Note:** The `fancy-regex` engine is *absurdly* slow in debug mode, because the regex engine (the main hot spot of highlighting) is now in Rust instead of C that's always built with optimizations. Consider using release mode or `onig` when testing.

## Caching

Because `syntect`'s API exposes internal cacheable data structures, there is a caching strategy that text editors can use that allows the text on screen to be re-rendered instantaneously regardless of the file size when a change is made after the initial highlight.

Basically, on the initial parse every 1000 lines or so copy the parse state into a side-buffer for that line.
When a change is made to the text, because of the way Sublime Text grammars work (and languages in general), only the highlighting after that change can be affected.
Thus when a change is made to the text, search backwards in the parse state cache for the last state before the edit, then kick off a background task to start re-highlighting from there.
Once the background task highlights past the end of the current editor viewport, render the new changes and continue re-highlighting the rest of the file in the background.

This way from the time the edit happens to the time the new colouring gets rendered in the worst case only `999+length of viewport` lines must be re-highlighted.
Given the speed of `syntect` even with a long file and the most complicated syntax and theme this should take less than 100ms.
This is enough to re-highlight on every key-stroke of the world's fastest typist *in the worst possible case*.
And you can reduce this asymptotically to the length of the viewport by caching parse states more often, at the cost of more memory.

Any time the file is changed the latest cached state is found, the cache is cleared after that point, and a background job is started.
Any already running jobs are stopped because they would be working on old state. This way you can just have one thread dedicated to highlighting that is always doing the most up-to-date work, or sleeping.

## Parallelizing

Since 3.0, `syntect` can be used to do parsing/highlighting in parallel.
`SyntaxSet` is both `Send` and `Sync` and so can easily be used from multiple threads.
It is also `Clone`, which means you can construct a syntax set and then clone it to use for other threads if you prefer.

Compared to older versions, there's nothing preventing the serialization of a `SyntaxSet` either.
So you can directly deserialize a fully linked `SyntaxSet` and start using it for parsing/highlighting.
Before, it was always necessary to do linking first.

It is worth mentioning that regex compilation is done lazily only when the regexes are actually needed.
Once a regex has been compiled, the compiled version is used for all threads after that.
Note that this is done using interior mutability, so if multiple threads happen to encounter the same uncompiled regex at the same time, compiling might happen multiple times.
After that, one of the compiled regexes will be used.
When a `SyntaxSet` is cloned, the regexes in the cloned set will need to be recompiled currently.

For adding parallelism to a previously single-threaded program, the recommended thread pooling is [`rayon`](https://github.com/nikomatsakis/rayon).
However, if you're working in an already-threaded context where there might be more threads than you want (such as writing a handler for an Iron request), the recommendation is to force all highlighting to be done within a fixed-size thread pool using [`rust-scoped-pool`](https://github.com/reem/rust-scoped-pool).
An example of the former is in `examples/parsyncat.rs`.

## Examples Available

There's a number of examples of programs that use `syntect` in the `examples` folder and some code outside the repo:

- `syncat` prints a highlighted file to the terminal using 24-bit colour ANSI escape codes.
  It demonstrates a simple file highlighting workflow.
- `synhtml` prints an HTML file that will display the highlighted code.
  Demonstrates how syntect could be used by web servers and static site generators.
- `synstats` collects a bunch of statistics about the code in a folder.
  Includes basic things like line count but also fancier things like number of functions.
  Demonstrates how `syntect` can be used for code analysis as well as highlighting, as well as how to use the APIs to parse out the semantic tokenization.
- [`faiyels`](https://github.com/trishume/faiyels) is a little code minimap visualizer I wrote that uses `syntect` for highlighting.
- `parsyncat` is like `syncat`, but accepts multiple files and highlights them in parallel.
  It demonstrates how to use `syntect` from multiple threads.

Here's that stats that `synstats` extracts from `syntect`'s codebase (not including examples and test data) as of [this commit](https://github.com/trishume/syntect/commit/10baa6888f84ea4ae35c746526302a8ff4956eb1):

```text
################## Stats ###################
File count:                               19
Total characters:                     155504

Function count:                          165
Type count (structs, enums, classes):     64

Code lines (traditional SLOC):          2960
Total lines (w/ comments & blanks):     4011
Comment lines (comment but no code):     736
Blank lines (lines-blank-comment):       315

Lines with a documentation comment:      646
Total words written in doc comments:    4734
Total words written in all comments:    5145
Characters of comment:                 41099
```

## Projects using Syntect

Below is a list of projects using Syntect, in approximate order by how long they've been using `syntect` (feel free to send PRs to add to this list):

- [bat](https://github.com/sharkdp/bat), a `cat(1)` clone, uses `syntect` for syntax highlighting.
- [Bolt](https://github.com/hiro-codes/bolt), a desktop application for building and testing APIs, uses `syntect` for syntax highlighting. 
- [catmark](https://github.com/bestouff/catmark), a console markdown printer, uses `syntect` for code blocks.
- [Cobalt](https://github.com/cobalt-org/cobalt.rs), a static site generator that uses `syntect` for highlighting code snippets.
- [crowbook](https://github.com/lise-henry/crowbook), a Markdown book generator, uses `syntect` for code blocks.
- [delta](https://github.com/dandavison/delta), a syntax-highlighting pager for Git.
- [Docket](https://github.com/iwillspeak/docket), a documentation site generator that uses `syntect` for highlighting.
- [hors](https://github.com/WindSoilder/hors), instant coding answers via command line, uses `syntect` for highlighting code blocks.
- [mdcat](https://github.com/lunaryorn/mdcat), a console markdown printer, uses `syntect` for code blocks.
- [Scribe](https://github.com/jmacdonald/scribe), a Rust text editor framework which uses `syntect` for highlighting.
- [syntect_server](https://github.com/sourcegraph/syntect_server), an HTTP server for syntax highlighting.
- [Television](https://github.com/alexpasmantier/television), a fast and versatile fuzzy finder TUI.
- [tokio-cassandra](https://github.com/nhellwig/tokio-cassandra), CQL shell in Rust, uses `syntect` for shell colouring.
- [xi-editor](https://github.com/google/xi-editor), a text editor in Rust which uses `syntect` for highlighting.
- [Zola](https://github.com/getzola/zola), a static site generator that uses `syntect` for highlighting code snippets.
- [The Way](https://github.com/out-of-cheese-error/the-way), a code snippets manager for your terminal that uses `syntect`for highlighting.
- [Broot](https://github.com/Canop/broot), a terminal file manager, uses `syntect` for file previews.
- [Rusty Slider](https://ollej.github.io/rusty-slider/), a markdown slideshow presentation application, uses `syntect` for code blocks.
- [Typst](https://github.com/typst/typst), a typesetting system and LaTeX alternative, uses `syntect` for highlighting code blocks.
- [bingus-blog](https://git.slonk.ing/slonk/bingus-blog), a blog software written in Rust, uses `syntect` for fenced code blocks.
- [BugStalker](https://github.com/godzie44/BugStalker/), modern debugger for Linux x86-64. Written in Rust for Rust programs.
- [Yazi](https://github.com/sxyazi/yazi), blazing fast terminal file manager based on async I/O, uses `syntect` for text file previews.
- [CodeSnap.nvim](https://github.com/mistricky/codesnap.nvim), snapshot plugin for Neovim, that uses `syntect` for code syntax highlighting.
- [code-to-pdf](https://github.com/Tommypop2/code-to-pdf), a tool that generates a syntax-highlighted PDF of a directory
- [Comrak](https://github.com/kivikakk/comrak), a CommonMark parser and formatter that uses `syntect` for highlighting code blocks.


## License and Acknowledgements

Thanks to [Robin Stocker](https://github.com/robinst), [Keith Hall](https://github.com/keith-hall) and [Martin Nordholts](https://github.com/Enselic) for making awesome substantial contributions of the most important impressive improvements `syntect` has had post-`v1.0`!
They deserve lots of credit for where `syntect` is today. For example @robinst implemented [fancy-regex support](https://github.com/trishume/syntect/pull/270) and [a massive refactor](https://github.com/trishume/syntect/pull/182) to enable parallel highlighting using an arena. @keith-hall found and fixed many bugs and [implemented Sublime syntax test support](https://github.com/trishume/syntect/pull/44).

Thanks to [Textmate 2](https://github.com/textmate/textmate) and @defuz's [sublimate](https://github.com/defuz/sublimate) for the existing open source code I used as inspiration and in the case of sublimate's `tmTheme` loader, copy-pasted.
All code (including defuz's sublimate code) is released under the MIT license.
