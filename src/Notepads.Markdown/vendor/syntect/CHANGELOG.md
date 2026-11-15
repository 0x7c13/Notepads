# Changelog

## [Version 5.3.0](https://github.com/trishume/syntect/compare/v5.2.0...v5.3.0)

### Improvements

- Add `HighlightLines::from_state()` constructor [#569]

### Fixes

- Ignore UTF-8 BOM on syntax detection [#530]
- Escape CSS class names [#539]
- Avoid an infinite loop when a syntax definition continually pushes contexts without consuming any input [#597]
- Return an error when a syntax definition contains an unescaped trailing backslash instead of panicking at runtime [#548]

### Dependencies

- Replace `bitflags` with a manual implementation [#590]
- Update `public-api` to work with new lockfile versions [#591]
- Update `thiserror` to v2 [#594]
- Update `fancy-regex` to `0.16.2` [#596]
- Bump `rust_onig` version to avoid build failures on newer GCC versions [#584]

## Docs

- Add various projects to the README showcase:
  - `BugStalker` [#533]
  - `Yazi` [#543]
  - `bingus-blog` [#534]
  - `CodeSnap.nvim` [#553]
  - `television` [#571]
  - `code-to-pdf` [#579]
  - `Comrak` [#581]
- Deprecate `syntect::parsing::SCOPE_REPO` for removal [#580]

### Other

- Format with `rustfmt` [#528]
- Bump nightly toolchain version to fix a test build failure [#542]
- Remove unused import in yaml load test [#531]
- Use `BufWriter` to improve unoptimized serialization perf [#554]
- Commit `Cargo.lock` to avoid flaky CI issues [#567]
- Switch from `expect-test` to `insta` [#568] then later to `public-api`'s `snapshot-testing` support [#595]
- Speed up unittest runtime [#577] [#598]
- Fix new clippy lints [#585]
- Use `cargo-hack` to check many different features in CI [#593]

[#528]: https://github.com/trishume/syntect/pull/528
[#530]: https://github.com/trishume/syntect/pull/530
[#531]: https://github.com/trishume/syntect/pull/531
[#533]: https://github.com/trishume/syntect/pull/533
[#534]: https://github.com/trishume/syntect/pull/534
[#539]: https://github.com/trishume/syntect/pull/539
[#542]: https://github.com/trishume/syntect/pull/542
[#543]: https://github.com/trishume/syntect/pull/543
[#548]: https://github.com/trishume/syntect/pull/548
[#553]: https://github.com/trishume/syntect/pull/553
[#554]: https://github.com/trishume/syntect/pull/554
[#567]: https://github.com/trishume/syntect/pull/567
[#568]: https://github.com/trishume/syntect/pull/568
[#569]: https://github.com/trishume/syntect/pull/569
[#571]: https://github.com/trishume/syntect/pull/571
[#577]: https://github.com/trishume/syntect/pull/577
[#579]: https://github.com/trishume/syntect/pull/579
[#580]: https://github.com/trishume/syntect/pull/580
[#581]: https://github.com/trishume/syntect/pull/581
[#584]: https://github.com/trishume/syntect/pull/584
[#585]: https://github.com/trishume/syntect/pull/585
[#590]: https://github.com/trishume/syntect/pull/590
[#591]: https://github.com/trishume/syntect/pull/591
[#593]: https://github.com/trishume/syntect/pull/593
[#594]: https://github.com/trishume/syntect/pull/594
[#595]: https://github.com/trishume/syntect/pull/595
[#596]: https://github.com/trishume/syntect/pull/596
[#597]: https://github.com/trishume/syntect/pull/597
[#598]: https://github.com/trishume/syntect/pull/598

## [Version 5.2.0](https://github.com/trishume/syntect/compare/v5.1.0...v5.2.0) (2024-02-07)

### Improvements

- Eliminate syntect library's dependency on serde's "derive" feature. Increases build parallelism.
- Add parsing of gutterSettings.

### Dependencies

- Update `regex-syntax` to 0.8.

## [Version 5.1.0](https://github.com/trishume/syntect/compare/v5.0.0...v5.1.0) (2023-08-01)

### Improvements

- Fix how `syntect::util::split_at()` handles multi-byte characters
- Allow case insensitive search for .tmtheme paths
- impl `PartialEq` for `Theme`

### Dependencies

- Upgrade `fancy-regex` to 0.11
- Upgrade `regex-syntax` to 0.7
- Replace `lazy_static` with `once_cell`

### Other

- Change MSRV policy to "last three stable versions"
- Make `Debug` impl of `syntect::highlighting::Color` less verbose

## [Version 5.0.0](https://github.com/trishume/syntect/compare/v4.6.0...v5.0.0) (2022-05-03)

Breaking changes

- Lazy-load syntaxes to significantly improve startup time. This changes the binary format of syntax dump files.
- Remove `ContextId::new()` from public API to support lazy-loading of syntaxes
- Rename `HighlightLines::highlight()` to `HighlightLines::highlight_line()` to make it clear that the function takes one line at a time
- Make `plist` dependency (used for loading themes) optional via new `plist-load` feature
- Remove obsolete `dump-load-rs` and `dump-create-rs` features that has been identical to `dump-load` and `dump-create` for two years
- Remove deprecated items `ThemeSettings::highlight_foreground`, `ThemeSettings::selection_background`, `ClassedHTMLGenerator::new`, `ClassedHTMLGenerator::parse_html_for_line`, `html::css_for_theme`, `html::tokens_to_classed_html` and `html::tokens_to_classed_spans`
- Mark all error enums as `#[non_exhaustive]`
- These functions have been changed to return a `Result` to allow propagation of errors:
  - `html::ClassedHTMLGenerator::parse_html_for_line_which_includes_newline`
  - `html::append_highlighted_html_for_styled_line`
  - `html::css_for_theme_with_class_style`
  - `html::highlighted_html_for_string`
  - `html::line_tokens_to_classed_spans`
  - `html::styled_line_to_highlighted_html`
  - `parsing::ParseState::parse_line`
  - `parsing::ScopeStack::apply`
  - `parsing::ScopeStack::apply_with_hook`
  - `parsing::syntax_definition::Context::match_at`
  - `parsing::syntax_definition::ContextReference::id`
  - `parsing::syntax_definition::ContextReference::resolve`

Other changes

- Fall back to `Plain Text` if a referenced syntax is missing
- Add support for `hidden_file_extensions` key in syntaxes.
- Implement `Error` and `Display` for all error enums by using `thiserror`
- Replace `lazycell` with `once_cell` to fix crash on lazy initialization
- Add `ScopeRangeIterator`
- Add CI check for Minimum Supported Rust Version. This is currently Rust 1.53.
- Make looking up a syntax by extension use case-insensitive comparison
- Make `from_dump_file()` ~15% faster
- Blend alpha value on converting colors to ANSI color sequences
- Fix sample code in documentation to avoid double newlines
- Fix lots of build warnings and lints
- Add Criterion benchmarks for a whole syntect pipeline and for `from_dump_file()`

## [Version 4.7.1](https://github.com/trishume/syntect/compare/v4.7.0...v4.7.1) (2022-01-03)

This version was yanked from crates.io due to a semver violation issue.

## [Version 4.7.0](https://github.com/trishume/syntect/compare/v4.6.0...v4.7.0) (2021-12-25)

This version was yanked from crates.io due to a semver violation issue.

## [Version 4.6.0](https://github.com/trishume/syntect/compare/v4.5.0...v4.6.0) (2021-08-01)

- Add `html::line_tokens_to_classed_spans` to also take a mutable ScopeStack, deprecate `tokens_to_classed_spans`, to avoid panics and incorrect highlighting.
- Derive Hash for Color and Style
- Add `find_unlinked_contexts` to `SyntaxSet`
- Add `syntaxes` method to `SyntaxSetBuilder`
- Bump `fancy-regex` to v0.7 and `yaml-rust` to v0.4.5

## [Version 4.5.0](https://github.com/trishume/syntect/compare/v4.4.0...v4.5.0) (2020-12-09)

- Added a new function for producing classed HTML which handles newlines correctly and deprecated old one. [#307](https://github.com/trishume/syntect/pull/307)

## [Version 4.4.0](https://github.com/trishume/syntect/compare/v4.3.0...v4.4.0) (2020-08-19)

- Errors are now `Send + Sync + 'static` [#304](https://github.com/trishume/syntect/pull/304)

## [Version 4.3.0](https://github.com/trishume/syntect/compare/v4.2.0...v4.3.0) (2020-08-01)

- Fixes unnecesary dependency of the `html` feature on the `assets` feature. [#300](https://github.com/trishume/syntect/pull/300)
- Adds ability to add prefixes to `html` module CSS class names. [#296](https://github.com/trishume/syntect/pull/296)

## [Version 4.2.0](https://github.com/trishume/syntect/compare/v4.1.1...v4.2.0) (2020-04-20)

- Updates to new versions of `onig` and `plist`. The new `onig` version doesn't require `bindgen` thus making compilation easier. [#293](https://github.com/trishume/syntect/pull/293)

## [Version 4.1.1](https://github.com/trishume/syntect/compare/v4.1.0...v4.1.1) (2020-04-20)

- Properly handle backreferences in included contexts [#288](https://github.com/trishume/syntect/pull/288)

## [Version 4.1.0](https://github.com/trishume/syntect/compare/v4.0.0...v4.1.0) (2020-03-30)

- Make sure errors implement `Send` [#285](https://github.com/trishume/syntect/pull/285)
- Fix errors to not use the deprecated `description()` [#286](https://github.com/trishume/syntect/pull/286)

Thanks @sharkdp for the bug fixes! Bumping second part of semver since `Send` is adding functionality (back).

## [Version 4.0.0](https://github.com/trishume/syntect/compare/v3.3.0...v4.0.0) (2020-03-29)

### Headline feature: pure-Rust `fancy-regex` engine option

Users can now opt in to a pure-Rust regex engine using Cargo features, making
compilation easier in general. People experiencing difficulty compiling for
Windows and Wasm should try switching to `fancy-regex`. Note this currently
approximately halves highlighting speed.

See the Readme and [#270](https://github.com/trishume/syntect/pull/270) for details.
Thanks to @robinst for implementing this!

### Other changes

- Ability to generate CSS for a theme for use with classed HTML generation (won't always be correct) [#274](https://github.com/trishume/syntect/pull/274/files)
- Don't generate empty spans in classed HTML [#276](https://github.com/trishume/syntect/pull/276)
- Miscellaneous dependency bumps and cleanup

### Breaking changes and upgrading

Upgrading should cause no errors for nearly all users. Users using more unusual APIs may have a small amount of tweaking to do.

- If you use `default-features = false` you may need to update your features to choose a regex engine
- A bunch of technically public APIs that I don't know if anyone uses changed due to the regex engine refactor, common uses shouldn't break

## [Version 3.3.0](https://github.com/trishume/syntect/compare/v3.2.1...v3.3.0) (2019-09-22)

> Bug fixes and new utilities

- Fixes multiple bugs
- Add RangedHighlightIterator
- Add `as_latex_escaped` util

## [Version 3.2.1](https://github.com/trishume/syntect/compare/v3.2.0...v3.2.1) (2019-08-10)

- Bump onig dependency
- inconsequential patches

## [Version 3.2.0](https://github.com/trishume/syntect/compare/v3.1.0...v3.2.0) (2019-03-09)

- Actually make `tokens_to_classed_spans` public like intended

## [Version 3.1.0](https://github.com/trishume/syntect/compare/v3.0.2...v3.1.0) (2019-02-24)

> Metadata and new classed HTML generation

- Add support for loading metadata ([#223](https://github.com/trishume/syntect/pull/223) [#225](https://github.com/trishume/syntect/pull/225) [#230](https://github.com/trishume/syntect/pull/230))
- Improve support for generating classed HTML and fix a bug, old function is deprecated because it's impossible to use correctly ([#235](https://github.com/trishume/syntect/pull/235))
- Update `plist` to `v0.4` and `pretty_assertions` to `v0.6` ([#232](https://github.com/trishume/syntect/pull/232) [#236](https://github.com/trishume/syntect/pull/236))

## [Version 3.0.2](https://github.com/trishume/syntect/compare/v3.0.1...v3.0.2) (2018-11-11)

> Bug fixes

- Fix application of multiple `with_prototype`s ([#220](https://github.com/trishume/syntect/pull/220), fixes [#160](https://github.com/trishume/syntect/issues/160), [#178](https://github.com/trishume/syntect/issues/178), ASP highlighting)
- Fix prototype marking logic ([#221](https://github.com/trishume/syntect/pull/221), fixes [#219](https://github.com/trishume/syntect/issues/219))

## [Version 3.0.1](https://github.com/trishume/syntect/compare/v3.0.0...v3.0.1) (2018-10-16)

> Minor bug fixes

- Fix a bug with syntaxes that used captures in lookarounds ([#176](https://github.com/trishume/syntect/issues/176) [#215](https://github.com/trishume/syntect/pull/215))
- Fix the precedence order of syntaxes to match Sublime ([#217](https://github.com/trishume/syntect/pull/217) [#216](https://github.com/trishume/syntect/pull/216))

## [Version 3.0.0](https://github.com/trishume/syntect/compare/v2.1.0...v3.0.0) (2018-10-09)

> Breaking changes and major new features

This is a major release with multiple breaking API changes, although upgrading shouldn't be too difficult. It fixes bugs and comes with some nice new features.

### Breaking changes and upgrading

- The `SyntaxSet` API has been revamped to use a builder and an arena of contexts. See [example usage](https://github.com/trishume/syntect/blob/51208d35a6d98c07468fbe044d5c6f37eb129205/examples/gendata.rs#L25-L28).
- Many functions now need to be passed the `SyntaxSet` that goes with the rest of their arguments because of this new arena.
- Filename added to `LoadingError::ParseSyntax`
- Many functions in the `html` module now take the `newlines` version of syntaxes.
  - These methods have also been renamed, partially so that code that needs updating doesn't break without a compile error.
  - The HTML they output also treats newlines slightly differently and I think more correctly but uglier when you look at the HTML.

#### Breaking rename upgrade guide

- `SyntaxSet::add_syntax -> SyntaxSetBuilder::add`
- `SyntaxSet::load_syntaxes -> SyntaxSetBuilder::add_from_folder`
- `SyntaxSet::load_plain_text_syntax -> SyntaxSetBuilder::add_plain_text_syntax`
- `html::highlighted_snippet_for_string -> html::highlighted_html_for_string`: also change to `newlines` `SyntaxSet`
- `html::highlighted_snippet_for_file -> html::highlighted_html_for_file`: also change to `newlines` `SyntaxSet`
- `html::styles_to_coloured_html -> html::styled_line_to_highlighted_html`: also change to `newlines` `SyntaxSet`
- `html::start_coloured_html_snippet -> html::start_highlighted_html_snippet`: return type also changed

### Major changes and new features

- Use arena for contexts ([#182](https://github.com/trishume/syntect/pull/182) [#186](https://github.com/trishume/syntect/pull/186) [#187](https://github.com/trishume/syntect/pull/187) [#190](https://github.com/trishume/syntect/pull/190) [#195](https://github.com/trishume/syntect/pull/195)): This makes the code cleaner, enables use of syntaxes from multiple threads, and prevents accidental misuse.
  - This involves a new `SyntaxSetBuilder` API for constructing new `SyntaxSet`s
  - See the revamped [parsyncat example](https://github.com/trishume/syntect/blob/51208d35a6d98c07468fbe044d5c6f37eb129205/examples/parsyncat.rs).
- Encourage use of newlines ([#197](https://github.com/trishume/syntect/pull/197) [#207](https://github.com/trishume/syntect/pull/207) [#196](https://github.com/trishume/syntect/issues/196)): The `nonewlines` mode is often buggy so we made it easier to use the `newlines` mode.
  - Added a `LinesWithEndings` utility for iterating over the lines of a string with `\n` characters.
  - Reengineer the `html` module to use `newlines` syntaxes.
- Add helpers for modifying highlighted lines ([#198](https://github.com/trishume/syntect/pull/198)): For use cases like highlighting a piece of text in a blog code snippet or debugger. This allows you to reach into the highlighted spans and add styles.
  - Check out `split_at` and `modify_range` in the `util` module.
- New `ThemeSet::add_from_folder` function ([#200](https://github.com/trishume/syntect/pull/200)): For modifying existing theme sets.

### Bug Fixes

- Improve nonewlines regex rewriting: [#212](https://github.com/trishume/syntect/pull/212) [#211](https://github.com/trishume/syntect/issues/211)
- Reengineer theme application to match Sublime: [#209](https://github.com/trishume/syntect/pull/209)
- Also mark contexts referenced by name as "no prototype" (same as ST): [#180](https://github.com/trishume/syntect/pull/180)
- keep with_prototype when switching contexts with `set`: [#177](https://github.com/trishume/syntect/pull/177) [#166](https://github.com/trishume/syntect/pull/166)
- Fix unused import warning: [#174](https://github.com/trishume/syntect/pull/174)
- Ignore trailing dots in selectors: [#173](https://github.com/trishume/syntect/pull/173)
- Fix `embed` to not include prototypes: [#172](https://github.com/trishume/syntect/pull/172) [#160](https://github.com/trishume/syntect/issues/160)

### Upgraded dependencies

- plist: `0.2 -> 0.3`
- regex: `0.2 -> 1.0`
- onig: `3.2.1 -> 4.1`

## [Version 2.1.0](https://github.com/trishume/syntect/compare/v2.0.1...v2.1.0) (2018-05-31)

> Regex checking and plain file names

* Check regexes compile upon loading from YAML (There's technically a small breaking change here if you match on the previously unused regex error, but I don't think anyone does)
* Can detect the correct syntax on full file names like `CMakeLists.txt`
* Make `nonewlines` mode marginally less buggy (still prefer using `newlines` mode)
* Better error types
* Better examples and tests

## [Version 2.0.1](https://github.com/trishume/syntect/compare/v2.0.0...v2.0.1) (2018-04-28)

> More robust parsing

* Parsing now abandons a regex after reaching a recursion depth limit instead of taking forever
* Loop detection better matches Sublime Text
* Parsing is faster!
* Dependency upgrades
* Other minor tweaks

Thanks to [@robinst](https://github.com/ronbinst) for the headline features of this release!

## [Version 2.0.0](https://github.com/trishume/syntect/compare/v1.8.2...v2.0.0) (2018-01-02)

> Breaking Changes and New Stuff

### Breaking changes

* The `static-onig` feature was removed, static linking is now the default
* Font styles and color constants now use associated consts because of bitflags upgrade
* `SyntaxDefinition::load_from_str` now has an extra parameter

### Other notable changes

* Support for new `embed` syntax, see [#124](https://github.com/trishume/syntect/issues/124)
* Updates to many dependencies
* Updated dumps
* More compact HTML output

## [Version 1.8.2](https://github.com/trishume/syntect/compare/v1.8.0...v1.8.2) (2017-11-11)

> New Inspired GitHub and libonig

## [Version 1.8.0](https://github.com/trishume/syntect/compare/v1.7.3...v1.8.0) (2017-10-14)

> Update bitflags & packages

This release changes how the constants for `FontStyle` and `Color`, relying on the new associated consts feature in `Rust 1.20`. The old constants are still available but are deprecated and will be removed in `v2.0`.

Packages were also updated to newer versions.

## [Version 1.7.3](https://github.com/trishume/syntect/compare/v1.7.2...v1.7.3) (2017-09-15)

> Enable comparison of parse states

Fixes comparisons of parse states so they are fast and don't recurse infinitely. Thanks [@raphlinus](https://github.com/raphlinus)

## [Version 1.7.2](https://github.com/trishume/syntect/compare/v1.7.0...v1.7.2) (2017-09-05)

> Bug fixes and package updates

* Fixes [#101](https://github.com/trishume/syntect/issues/101), which caused some syntaxes like PHP to behave incorrectly.
* Updates Packages with new syntax versions
* Adds new handy flags to the `syncat` example

## [Version 1.7.0](https://github.com/trishume/syntect/compare/v1.6.0...v1.7.0) (2017-06-30)

> Pure Rust dump loading / creation features

## [Version 1.6.0](https://github.com/trishume/syntect/compare/v1.5.0...v1.6.0) (2017-06-21)

> Helper methods and more theme attributes

## [Version 1.5.0](https://github.com/trishume/syntect/compare/v1.4.0...v1.5.0) (2017-05-31)

> Highlighting stacks

Small release, adds a convenience method for highlighting an entire stack, and derives some more things on `Scope`.

## [Version 1.4.0](https://github.com/trishume/syntect/compare/v1.3.0...v1.4.0) (2017-05-25)

> Serde and optional parsing

This release switches the dump format from `rustc-serialize` to `Serde`, anyone using custom dumps will have to update them.

It also makes the parsing part of the library optional behind a feature flag, anyone not using the default feature flags probably will want to add the `parsing` flag.

## [Version 1.3.0](https://github.com/trishume/syntect/tree/v1.3.0) (2017-04-05)

> Bug fixes, tests, updates and feature flags

* Syntax tests: there is a new `syntest` example for running Sublime Text syntax tests
* Bug fixes: there's a ton of bugs fixed in this release, mostly found via the syntax tests. These mostly affected certain syntaxes which pushed/set multiple contexts at once.
* Updated packages: The Sublime packages have been updated to the latest version
* Feature flags: there's now Cargo feature flags for disabling some parts of syntect if you don't want unnecessary binary and dependency bloat.
