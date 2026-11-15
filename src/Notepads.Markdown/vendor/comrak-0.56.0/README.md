<!-- Notepads modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. Original Comrak copyright/license remain applicable. -->

# [Comrak](https://comrak.ee/)

[![Build status](https://github.com/kivikakk/comrak/actions/workflows/rust.yml/badge.svg)](https://github.com/kivikakk/comrak/actions/workflows/rust.yml)
[![CommonMark: 652/652](https://img.shields.io/badge/commonmark-652%2F652-brightgreen.svg)](https://github.com/commonmark/commonmark-spec/blob/9103e341a973013013bb1a80e13567007c5cef6f/spec.txt)
[![GFM: 670/670](https://img.shields.io/badge/gfm-670%2F670-brightgreen.svg)](https://github.com/kivikakk/cmark-gfm/blob/2f13eeedfe9906c72a1843b03552550af7bee29a/test/spec.txt)
[![crates.io version](https://img.shields.io/crates/v/comrak.svg)](https://crates.io/crates/comrak)
[![docs.rs](https://docs.rs/comrak/badge.svg)](https://docs.rs/comrak)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/11796/badge)](https://www.bestpractices.dev/projects/11796)

[Comrak](https://comrak.ee/) is a [CommonMark](https://commonmark.org/) and [GitHub Flavored Markdown](https://github.github.com/gfm/) compatible parser and renderer, written in Rust.

Compliant with [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) by default.

## Installation

Specify it as a requirement in `Cargo.toml`:

```toml
[dependencies]
comrak = "0.56"
```

Comrak's library supports Rust <span class="msrv">1.89</span>+.

### CLI

- Anywhere with a Rust toolchain:
  - `cargo install comrak`
  - <code>[cargo binstall](https://github.com/cargo-bins/cargo-binstall) comrak</code>
- Many Unix distributions:
  - `pacman -S comrak`
  - `brew install comrak`
  - `dnf install comrak`
  - `nix run nixpkgs#comrak`
- Windows:
  - <code>[scoop](https://scoop.sh/) install comrak</code>

You can also find builds I've published in [GitHub Releases](https://github.com/kivikakk/comrak/releases), but they're limited to machines I have access to at the time of making them! [webinstall.dev](https://webinstall.dev/comrak/) offers `curl | shell`-style installation of the latest of these for your OS, including Windows.

## Usage

<details>

<summary>Click to expand the CLI <code>--help</code> output.

```console
$ comrak --help
```

</summary>

```
A 100% CommonMark-compatible GitHub Flavored Markdown parser and formatter

Usage: comrak [OPTIONS] [FILE]...

Arguments:
  [FILE]...
          CommonMark file(s) to parse; or standard input if none passed

Options:
  -c, --config-file <PATH>
          Path to config file containing command-line arguments, or 'none'
          
          [default: /home/runner/.config/comrak/config]

  -i, --inplace
          Reformat CommonMark files in-place

      --hardbreaks
          Treat newlines as hard line breaks

      --smart
          Replace punctuation like "this" with smart punctuation like “this”

      --github-pre-lang
          Use GitHub-style "<pre lang>" for code blocks

      --full-info-string
          Include words following the code block info string in a data-meta attribute

      --gfm
          Enable GitHub-flavored markdown extensions: strikethrough, table, autolink, and tasklist.
          Also enables --github-pre-lang and --gfm-quirks

      --gfm-quirks
          Use GFM-style quirks in output HTML, such as not nesting <strong> tags, which otherwise
          breaks CommonMark compatibility

      --relaxed-tasklist-character
          Permit any character inside a tasklist item, not just " ", "x" or "X"

      --relaxed-autolinks
          Relax autolink parsing: allows links to be recognised when in brackets, permits all URL
          schemes, and permits domains without a TLD (like "http://localhost")

      --tasklist-classes
          Include "task-list-item" and "task-list-item-checkbox" classes on

      --default-info-string <INFO>
          Default value for fenced code block's info strings if none is given

      --unsafe
          Allow inline and block HTML (unless --escape is given), and permit

      --gemoji
          Translate gemoji like ":thumbsup:" into Unicode emoji like "👍"

      --escape
          Escape raw HTML, instead of clobbering it; takes precedence over --unsafe

      --escaped-char-spans
          Wrap escaped Markdown characters in "<span data-escaped-char>" in HTML

  -e, --extension <EXTENSION>
          Specify extensions to use
          
          Multiple extensions can be delimited with ",", e.g. '--extension strikethrough,table', or
          you can pass --extension/-e multiple times
          
          [possible values: strikethrough, table, autolink, tasklist, superscript, footnotes,
          inline-footnotes, description-lists, multiline-block-quotes, math-dollars, math-latex,
          math-code, wikilinks-title-after-pipe, wikilinks-title-before-pipe, underline, subscript,
          spoiler, greentext, alerts, cjk-friendly-emphasis, subtext, highlight, insert,
          phoenix-heex, block-directive, header-attributes, fenced-code-attributes,
          inline-code-attributes, link-attributes]

  -t, --to <FORMAT>
          Specify output format
          
          [default: html]
          [possible values: html, xml, commonmark]

  -o, --output <FILE>
          Write output to FILE instead of stdout

      --width <WIDTH>
          Specify wrap width for output CommonMark, or '0' to disable wrapping
          
          [default: 0]

      --header-id-prefix <PREFIX>
          Use the Comrak header IDs extension, with the given ID prefix

      --header-id-prefix-in-href
          Apply the header ID prefix to the href anchor as well

      --front-matter-delimiter <DELIMITER>
          Detect frontmatter that starts and ends with the given string, and do not include it in
          the resulting document

      --syntax-highlighting <THEME>
          Syntax highlighting for fenced code blocks; 'css' for CSS classes (default), a theme name
          for inline styles, or 'none' to disable
          
          [default: css]

      --list-style <LIST_STYLE>
          Specify bullet character for lists ("-", "+", "*") in CommonMark output
          
          [default: dash]
          [possible values: dash, plus, star]

      --alert-style <ALERT_STYLE>
          Specify alert style (<div> vs <aside>) in HTML output
          
          [default: specific]
          [possible values: specific, semantic]

      --sourcepos
          Include source position attributes in HTML and XML output

      --ignore-setext
          Do not parse setext headers

      --ignore-empty-links
          Do not parse empty links

      --sourcepos-chars
          Report column positions in sourcepos as a Unicode character count rather than UTF-8 byte
          offsets

      --experimental-minimize-commonmark
          Minimise escapes in CommonMark output using a trial-and-error algorithm

      --compact
          Suppress pretty-printing newlines between block-level HTML elements

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

By default, Comrak will attempt to read command-line options from a config file specified by
--config-file. This behaviour can be disabled by passing --config-file none. It is not an error if
the file does not exist.
```

</details>

And there's a Rust interface. You can use `comrak::markdown_to_html` directly:

```rust
use comrak::{markdown_to_html, Options};
assert_eq!(
    markdown_to_html("¡Olá, **世界**!", &Options::default()),
    "<p>¡Olá, <strong>世界</strong>!</p>\n"
);
```

Or you can parse the input into an AST yourself, manipulate it, and then use your desired formatter:

```rust
use comrak::nodes::NodeValue;
use comrak::{format_html, parse_document, Arena, Options};

fn replace_text(document: &str, orig_string: &str, replacement: &str) -> String {
    // The returned nodes are created in the supplied Arena, and are bound by its lifetime.
    let arena = Arena::new();

    // Parse the document into a root `AstNode`
    let root = parse_document(&arena, document, &Options::default());

    // Iterate over all the descendants of root.
    for node in root.descendants() {
        if let NodeValue::Text(ref mut text) = node.data_mut().value {
            // If the node is a text node, perform the string replacement.
            *text = text.owned_str().expect("legacy example requires Owned AST").replace(orig_string, replacement).into()
        }
    }

    let mut html = String::new();
    format_html(root, &Options::default(), &mut html).unwrap();

    html
}

fn main() {
    let doc = "Hello, pretty world!\n\n1. Do you like [pretty](#) paintings?\n2. Or *pretty* music?\n";
    let orig = "pretty";
    let repl = "beautiful";
    let html = replace_text(doc, orig, repl);

    println!("{}", html);
    // Output:
    //
    // <p>Hello, beautiful world!</p>
    // <ol>
    // <li>Do you like <a href="#">beautiful</a> paintings?</li>
    // <li>Or <em>beautiful</em> music?</li>
    // </ol>
}
```

For a slightly more real-world example, see how I [generate my GitHub user README](https://github.com/kivikakk/kivikakk) from a base document with embedded YAML, which itself has embedded Markdown, or
[check out some of Comrak's dependents on crates.io](https://crates.io/crates/comrak/reverse_dependencies) or [on GitHub](https://github.com/kivikakk/comrak/network/dependents).

## Security

As with [`cmark`](https://github.com/commonmark/cmark) and [`cmark-gfm`](https://github.com/github/cmark-gfm#security),
Comrak will scrub raw HTML and potentially dangerous links. This change was introduced in Comrak 0.4.0 in support of a
safe-by-default posture, and later adopted by our contemporaries. :)

To allow these, use the `r#unsafe` option (or `--unsafe` with the command line program). If doing so, we recommend the
use of a sanitisation library like [`ammonia`](https://github.com/notriddle/ammonia) configured specific to your needs.

## Extensions

Comrak supports four (!) of the five extensions to CommonMark defined in the
[GitHub Flavored Markdown Spec](https://github.github.com/gfm/):

- [Tables](https://github.github.com/gfm/#tables-extension-)
- [Task list items](https://github.github.com/gfm/#task-list-items-extension-)
- [Strikethrough](https://github.github.com/gfm/#strikethrough-extension-)
- [Autolinks](https://github.github.com/gfm/#autolinks-extension-)

We [deprecated](https://github.com/kivikakk/comrak/pull/830) the
[Disallowed Raw HTML](https://github.github.com/gfm/#disallowed-raw-html-extension-)
(a.k.a. `tagfilter`) extension in Comrak 0.55.0, and
[removed](https://github.com/kivikakk/comrak/pull/845) it in Comrak 0.56.0. It is poorly
designed, specific to GitHub's CommonMark transition of 2017, and should not be used.

Comrak additionally supports its own extensions, which are yet to be specced out (PRs welcome!):

- Superscript
- Header IDs
- Footnotes
- Inline footnotes
- Description lists
- Front matter
- Multi-line blockquotes
- Math
- Emoji shortcodes
- Wikilinks
- Underline
- Spoiler text
- "Greentext"
- Alerts
- [CJK friendly emphasis](https://github.com/tats-u/markdown-cjk-friendly)

By default none are enabled; they are individually enabled with each parse by setting the appropriate values in the
[`options::Extension` struct](https://docs.rs/comrak/latest/comrak/options/struct.Extension.html).

## Custom formatting

The default HTML formatter can be partially specialised, to
allow customising the output for certain node types without
having to reimplement a whole formatter.  See the docs for
[`comrak::create_formatter`](https://docs.rs/comrak/latest/comrak/macro.create_formatter.html)
for details.

## Plugins

### Fenced code block syntax highlighting

You can provide your own syntax highlighting engine.

Create an implementation of the `SyntaxHighlighterAdapter` trait, and then provide an instance of such adapter to
`Plugins.render.codefence_syntax_highlighter`. For formatting a Markdown document with plugins, use the
`markdown_to_html_with_plugins` function, which accepts your plugins object as a parameter.

See the `syntax_highlighter.rs` and `syntect.rs` examples for more details.

#### Syntect

[`syntect`](https://github.com/trishume/syntect) is a syntax highlighting library for Rust. By default, `comrak` offers
a plugin for it. In order to utilize it, create an instance of `plugins::syntect::SyntectAdapter` and use it in your
`Plugins` option.

## Related projects

Comrak's original design goal was to model the upstream
[`cmark-gfm`](https://github.com/github/cmark-gfm) as closely as possible in
terms of code structure. Many years have passed since its inception, and the codebases
have since grown considerably apart. It does remain the case, though, that there
are bugs in `cmark-gfm` that are likely in Comrak too, as a result.

Over the years, we have increasingly opted to fix such bugs, rather than
maintain upstream compatibility at all costs.  `cmark-gfm` no longer appears to
be under active maintenance, but Comrak is a living and growing project.

This library offers an AST backed by
[`typed_arena`](https://github.com/thomcc/rust-typed-arena), with extensive
use of `RefCell` in the core node type to provide mutable access with
parent/sibling/child pointers.  This can produce non-idiomatic-looking code,
though in practice it has proven very usable.

For whatever reason, Comrak may not meet your requirements. Here are some
projects and resources to also consider:

- [Raph Levien](https://github.com/raphlinus)'s [`pulldown-cmark`](https://github.com/google/pulldown-cmark). It's
  very fast, uses a novel parsing algorithm, and doesn't construct an AST (but you can use it to make one if you
  want). `cargo doc` uses this, as do many other projects in the ecosystem.
- [markdown-rs](https://github.com/wooorm/markdown-rs) looks really promising.
- [markdown-it](https://github.com/markdown-it-rust/markdown-it) is a port of JavaScript's [markdown-it.js](https://github.com/markdown-it/markdown-it).
- [babelmark](https://babelmark.github.io/) lets you compare many implementations at once, including the above.
- Know of another library? Please open a PR to add it!

### Bindings

- [Commonmarker](https://github.com/gjtorikian/commonmarker) — Ruby bindings for this library built with Magnus/rb-sys.
  Available on RubyGems as [`commonmarker`](https://rubygems.org/gems/commonmarker).
- [MDEx](https://github.com/leandrocp/mdex) — Elixir bindings for this library built with Rustler.
  Available on Hex as [`mdex`](https://hex.pm/packages/mdex).
- [comrak](https://github.com/lmmx/comrak) — Python bindings for this library built with PyO3.
  Available on PyPI as [`comrak`](https://pypi.org/project/comrak), benchmarked at 15-60x faster than pure Python alternatives.
- [comrak-ext](https://github.com/Martin005/comrak-ext) — Python bindings; fork of `comrak` with additional APIs exposed, including the AST.
  Available on PyPI as [`comrak-ext`](https://pypi.org/project/comrak-ext).
- [comrak-wasm](https://github.com/nberlette/comrak-wasm) — TypeScript bindings for this library, built with WebAssembly.
  Available on JSR as [`@nick/comrak`](https://jsr.io/@nick/comrak).

### Users

Comrak is used in a few Rust-y places, and more beyond:

- [crates.io](https://crates.io), [docs.rs](https://docs.rs) and [lib.rs](https://lib.rs) use Comrak to render README Markdown faithfully.
- [GitLab](https://gitlab.com) uses Comrak to render Markdown documents, issues, comments, and more.
- [Deno](https://deno.com) uses Comrak to render documentation in [`deno_doc`](https://github.com/denoland/deno_doc).
- [Reddit](https://reddit.com)'s new-style site uses a Comrak fork[^reddit].
- [Lockbook](https://lockbook.net/) is a Markdown-based secure notebook with native apps. It looks really neat!!
- [many](https://github.com/kivikakk/comrak/network/dependents) [more!](https://crates.io/crates/comrak/reverse_dependencies)

I'd be really happy to add your site or app here, just open a PR or issue. :)

[^reddit]: And they contributed some really nice changes, years back. Then management went and sold their user base and all their credibility. What can you do.

## Benchmarking

We offer some tools to perform stdin-to-stdout benchmarking of Comrak with its contemporaries.  In this respect, Comrak is not and will not be the fastest: some alternatives do not construct an AST in this scenario.

You'll need to [install hyperfine](https://github.com/sharkdp/hyperfine#installation), and CMake if you want to compare against `cmark-gfm`.

If you want to just run the benchmark for the `comrak` binary itself, run:

```bash
make bench-comrak
```

This will build Comrak in release mode, and run benchmark on it. You will see the time measurements as reported by hyperfine in the console.

The `Makefile` also provides a way to run benchmarks for `comrak` current state (with your changes), `comrak` main branch, [`cmark-gfm`](https://github.com/github/cmark-gfm), [`pulldown-cmark`](https://github.com/raphlinus/pulldown-cmark) and [`markdown-it.rs`](https://github.com/rlidwka/markdown-it.rs). You'll need CMake, and ensure [submodules are prepared](https://stackoverflow.com/a/10168693/499609).

```bash
make bench-all
```

This will build and run benchmarks across all, and report the time taken by each as well as relative time.

<!-- XXX: The following isn't really true at the moment, due to https://github.com/kivikakk/comrak/issues/339 -->

<!-- Apart from this, CI is also setup for running benchmarks when a pull request is first opened. It will add a comment with the results on the pull request in a tabular format comparing the 5 versions. After that you can manually trigger this CI by commenting `/run-bench` on the PR, this will update the existing comment with new results. Note benchmarks won't be automatically run on each push. -->

## Contributing

Contributions are **highly encouraged**; if you'd like to assist, consider checking out the [`good first issue` label](https://github.com/kivikakk/comrak/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)! I'm happy to help provide direction and guidance throughout, even if (especially if!) you're new to Rust or open source.

Where possible I practice [Optimistic Merging](http://hintjens.com/blog:106) as described by Peter Hintjens. Please keep the [code of conduct](CODE_OF_CONDUCT.md) in mind too.

Thank you to Comrak's many contributors for PRs and issues opened!

### Vulnerabilities

Please report security issues to the author at <<ashe@kivikakk.ee>>.  If you
have a GitHub account, you are welcome to use its interface to [privately report
security issues](https://github.com/kivikakk/comrak/security/advisories/new).

### Policy on LLM contributions

Please do not submit the output of LLMs to Comrak, in any form. For example, do
not use them to:

- Write code.
- Write tests.
- Write comments.
- Write issue or pull request descriptions.
- Write documentation.

This list isn't exhaustive, and skirting the rules is not tolerated.

If an LLM has played a part in your discovery of a bug, or performance or
security issue, that's OK — I simply ask that you write the issue, fix, or pull
request description yourself, by hand. If you do not personally understand what
you have submitted, you are assisting neither Comrak's maintainers nor yourself
with your submission.

I make an exception for vulnerability reports. If you've found a legitimate
security issue in Comrak, you may privately submit an LLM-written report. If,
however, your reports are consistently trash tier, I will block you from the
project. There's no reason they have to be. This hasn't happened, yet, and (a)
I am grateful, and (b) don't be the first. I will update this canary should this
fact ever change.

## Contributors

[![Small chart showing Comrak contributors.](https://opencollective.com/comrak/contributors.svg?width=890&button=false)](https://github.com/kivikakk/comrak/graphs/contributors)

## Contact

Asherah Connor <<ashe@kivikakk.ee>>

## Legal

Copyright (c) 2017–2026, Comrak contributors. Licensed under
the [2-Clause BSD License](https://opensource.org/licenses/BSD-2-Clause).

`cmark` itself is is copyright (c) 2014, John MacFarlane.

See [COPYING](COPYING) for all the details.
