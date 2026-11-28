<!--

Categories to use in this document, and the order in which to give them:

Security:
Reverts:
Parser changes:
Changed APIs:
New APIs:
Bug fixes:
Stability:
Performance:
Dependency updates:
Documentation:
Build changes:
Behind the scenes:

-->

# [v0.56.0] - 2026-10-06

The headline announcement: Comrak's MSRV has been bumped to **1.89**.  I really don't like bumping it, but not doing so limits our ongoing access to the developing Rust ecosystem.  Thanks for your understanding!

There's also been quite a few pathological edge-cases solved in this release; updating is recommended.

Security:

* Fix non-functional table row counter. (by @kivikakk in https://github.com/kivikakk/comrak/pull/848)
* Fix pathological emphasis. (by @kivikakk in https://github.com/kivikakk/comrak/pull/851)
* Fix pathological relaxed autolink scan. (by @kivikakk in https://github.com/kivikakk/comrak/pull/852)
* Fix pathological footnote parse. (by @kivikakk in https://github.com/kivikakk/comrak/pull/858)
* Fix description list quadratic case. (by @kivikakk in https://github.com/kivikakk/comrak/pull/859)
* Remove quadratic case in Anchorizer. (by @kivikakk in https://github.com/kivikakk/comrak/pull/860)

Parser changes:

* We addressed a trio of autolink parser differences from cmark-gfm. (by @kivikakk in https://github.com/kivikakk/comrak/pull/843 and https://github.com/kivikakk/comrak/pull/844)

Changed APIs:

* The `tagfilter` extension has been **removed**, as I warned in the last release. It's bad. Don't use it. (by @kivikakk in https://github.com/kivikakk/comrak/pull/845)

Bug fixes:

* Measure fenced block offset in columns, not bytes. (by @szdytom in https://github.com/kivikakk/comrak/pull/836)
* Fix multiline inline source positions. (by @szdytom in https://github.com/kivikakk/comrak/pull/855)
* Count lone carriage returns in front matter source positions. (by @szdytom in https://github.com/kivikakk/comrak/pull/861)
* Fix table row source positions for differing indents. (by @szdytom in https://github.com/kivikakk/comrak/pull/862)
* Fix indented code block source positions after trailing blanks. (by @szdytom in https://github.com/kivikakk/comrak/pull/863)
* Fix paragraph source positions before tables with CRLF. (by @szdytom in https://github.com/kivikakk/comrak/pull/864)
* Zero-end columns adopt the widest end position in the path. (by @kivikakk in https://github.com/kivikakk/comrak/pull/868)

Stability:

* Update ranges for `is_cjk` to Unicode 18 & add fast path. (by @tats-u in https://github.com/kivikakk/comrak/pull/838)

Performance:

* Address `Node::collect_text` recursion and some wastefulness. (by @kivikakk in https://github.com/kivikakk/comrak/pull/847)
* Allocate one delimiter arena + byte scan table set per document. (by @kivikakk in https://github.com/kivikakk/comrak/pull/849)
* Autolink/sourcepos optimisations. (by @kivikakk in https://github.com/kivikakk/comrak/pull/850)
* Store exact-sized content in nodes during block phase. (by @kivikakk in https://github.com/kivikakk/comrak/pull/853)
* Special character detection in inline scanning considers context. (by @kivikakk in https://github.com/kivikakk/comrak/pull/856)
* Move 40 bytes of block-only data per node out of the arena. (by @kivikakk in https://github.com/kivikakk/comrak/pull/857)
* List sourcepos optimisations. (by @kivikakk in https://github.com/kivikakk/comrak/pull/866)

Dependency updates:

* Bump bon from 3.9.0 to 3.9.3. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/834)
* Bump emojis from 0.8.2 to 0.9.0. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/841)
* Bump smallvec from 1.15.1 to 1.15.2. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/835)
* Bump phf and phf_codegen from 0.13.1 to 0.14.0. (by @kivikakk in https://github.com/kivikakk/comrak/pull/842)
* Bump rustc-hash from 2.1.2 to 2.1.3. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/869)

Documentation:

* Clarified the LLM policy. (by @kivikakk in https://github.com/kivikakk/comrak/pull/865)

Build changes:

* Double test timeouts in CI. (by @kivikakk in https://github.com/kivikakk/comrak/pull/867)

Behind the scenes:

* Make `parser::autolink::check_domain` make sense. (by @kivikakk in https://github.com/kivikakk/comrak/pull/846)
* CodSpeed benches run against the GFM option-set too. (by @kivikakk in https://github.com/kivikakk/comrak/pull/854)


## New Contributors

* @szdytom made their first contribution in https://github.com/kivikakk/comrak/pull/836

Diff: https://github.com/kivikakk/comrak/compare/v0.55.0...v0.56.0


# [v0.55.0] - 2026-09-06

Security:

* Fixed two DoS issues, published as [GHSA-xg9p-p4jc-c46g](https://github.com/kivikakk/comrak/security/advisories/GHSA-xg9p-p4jc-c46g).
  * Both are in the `autolinks` extension, which are part of the `gfm` option set and fairly widely used, and so you almost certainly want to upgrade if you process untrusted content.

Stability:

* The `tagfilter` extension has been marked deprecated, and will be removed in Comrak 0.56.0. (by @kivikakk in https://github.com/kivikakk/comrak/pull/830)
  * It's not fit for any purpose; signed, the author.

Dependency updates:

* Bump emojis from 0.8.0 to 0.8.2 (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/825, https://github.com/kivikakk/comrak/pull/827)
* Bump clap from 4.5.60 to 4.6.1 (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/826)

Diff: https://github.com/kivikakk/comrak/compare/v0.54.0...v0.55.0


# [v0.54.0] - 2026-07-12

Changed APIs:

* Header/anchor renders been changed to improve accessibility (by @aozerova in https://github.com/kivikakk/comrak/pull/808)
  * The anchor no longer has `aria-hidden`, and gets a proper `aria-label`. It also now appears at the _end_ of the header, not the front.
* `html::collect_text` has been moved onto `Node` (by @kivikakk in https://github.com/kivikakk/comrak/pull/818)
* All items marked as deprecated has been removed. You've been warned! (by @kivikakk in https://github.com/kivikakk/comrak/pull/823)

New APIs:

* LaTeX-style math delimiters are now supported with an option! (by @blib in https://github.com/kivikakk/comrak/pull/809)
* Attribute support has been added! (by @kivikakk in https://github.com/kivikakk/comrak/pull/814, https://github.com/kivikakk/comrak/pull/816)
  * They aren't exposed in any formatter yet, so you'll need a custom formatter.
  * Attribute parsing can be enabled on headers, fenced code blocks, inline code spans, and links and images.
  * Aaaand the parsing code was cleaned up a bit :) (by @KevSlashNull in https://github.com/kivikakk/comrak/pull/817)
* Semantic HTML for alerts/admonitions can now be used! (by @flying-sheep in https://github.com/kivikakk/comrak/pull/822)

Bug fixes:

* Fix a CommonMark roundtripping failure when whitespace entities are place at emphasis boundaries (by @kivikakk in https://github.com/kivikakk/comrak/pull/750)
* Fix a crash in the CommonMark renderer with ordered lists nested in blockquotes (by @kivikakk in https://github.com/kivikakk/comrak/pull/815)

Dependency updates:

* Bump toml from `1.1.0+spec-1.1.0` to `1.1.2+spec-1.1.0` (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/819)


## New Contributors

* @blib made their first contribution in https://github.com/kivikakk/comrak/pull/809
* @KevSlashNull made their first contribution in https://github.com/kivikakk/comrak/pull/817
* @flying-sheep made their first contribution in https://github.com/kivikakk/comrak/pull/822

Diff: https://github.com/kivikakk/comrak/compare/v0.53.0...v0.54.0


# [v0.53.0] - 2026-07-02

Changed APIs:

* `--inplace` now accepts multiple arguments (by @ilkecan in https://github.com/kivikakk/comrak/pull/792)

Bug fixes:

* `~`, `~~~` and `:::` are escaped in CommonMark output (by @sandmor in https://github.com/kivikakk/comrak/pull/791)

Stability:

* AST now validated when formatting HTML and XML in debug builds (@aozerova in https://github.com/kivikakk/comrak/pull/800)

Dependency updates:

* Bump mshick/add-pr-comment from 2 to 2.8.2 (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/793)
* Bump peter-evans/create-pull-request from 8 to 8.1.0 (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/797)
* Bump CodSpeedHQ/action from 4 to 4.10.4 (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/794, https://github.com/kivikakk/comrak/pull/798, https://github.com/kivikakk/comrak/pull/799)
* Bump toml from 1.0.6+spec-1.1.0 to 1.1.0+spec-1.1.0 (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/810)
* Bump rustc-hash from 2.1.1 to 2.1.2 (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/811)
* GitHub Actions SHAs pinned with 90 day minimum age to avoid supply-chain nonsense (by @kivikakk in https://github.com/kivikakk/comrak/pull/802)

Documentation:

* Scoop installation instructions added to README (by @tats-u in https://github.com/kivikakk/comrak/pull/796)

Build changes:

* Alternative syntect backends are exposed as features (by @gjtorikian in https://github.com/kivikakk/comrak/pull/812)
  * **WASM users**: you will need to build with `--no-default-features`. Please read the pull request for more details; suggestions/fixes most appreciated!


## New Contributors

* @sandmor made their first contribution in https://github.com/kivikakk/comrak/pull/791
* @ilkecan made their first contribution in https://github.com/kivikakk/comrak/pull/792
* @aozerova made their first contribution in https://github.com/kivikakk/comrak/pull/800!

Diff: https://github.com/kivikakk/comrak/compare/v0.52.0...v0.53.0


# [v0.52.0] - 2026-04-04

Happy April!

Reverts:

* I had to remove the Typst formatter I was so happy about in the last release! This is very unfortunate. It turned out to be LLM-generated, and I just haven't had the time or energy to replace it myself. (by @kivikakk in https://github.com/kivikakk/comrak/pull/781)

Changed APIs:

* Deprecate the option name `header_ids` in favor of `header_id_prefix`, to make it clear that the option value actually adds a prefix to the `id` attribute, and add the `header_id_prefix_in_href` option, which adds the same prefix to generated `href`s. (by @miketheman in https://github.com/kivikakk/comrak/pull/776)
* Decouple greentext handling from blockquote parsing. (by @Martin005 in https://github.com/kivikakk/comrak/pull/789)
  * This means a lone `>` amongst blockquotes won't trigger greentext when enabled.

New APIs:

* Add parse option for char-based columns in `Sourcepos`. They report by default in byte columns according to the input UTF-8 source. (by @Martin005 in https://github.com/kivikakk/comrak/pull/779)
* Add block directive extension. (by @P-SiZK in https://github.com/kivikakk/comrak/pull/782)

Dependency updates:

* Bump toml from 1.0.3+spec-1.1.0 to 1.0.6+spec-1.1.0. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/778)
* I've lengthened the dependency cooldown in Dependabot to 90 days, given _gestures aimlessly_ the state of everything. (15c22fa2)

Documentation:

* Added a `CONTRIBUTING.md` that directs the reader to the README's contributing section. (ddae1558)

Build changes:

* Set `codegen-units = 1` for release builds. (suggested by @zamazan4ik, added in ea026ef3)

Behind the scenes:

* Stop fuzz targets regressing by adding the build to CI. (by @kivikakk in https://github.com/kivikakk/comrak/pull/771)


## New Contributors

* @P-SiZK made their first contribution in https://github.com/kivikakk/comrak/pull/782

Diff: https://github.com/kivikakk/comrak/compare/v0.51.0...v0.52.0


# [v0.51.0] - 2026-03-10

新年快乐! :) What a lovely assortment of improvements, fixes, and new contributors we have in this release. Of particular note, Comrak has gained initial support for [formatting to Typst](https://github.com/kivikakk/comrak/pull/763), thanks to @neilberkman!  This is a first cut and there are some known issues — see the PR — but I'm super happy to have folks testing it sooner rather than later!

Changed APIs:

* Add "css" to CLI's syntax highlighting options; set CSS as the default highlighting mode. (by @gjtorikian in https://github.com/kivikakk/comrak/pull/739)
* Allow raw nodes to be children of anything. (by @JeanMertz in https://github.com/kivikakk/comrak/pull/743)
* Support comma-delimited language tokens in Syntect plugin. (by @neilberkman in https://github.com/kivikakk/comrak/pull/752)

New APIs:

* Added `RenderPlugins::codefence_renderers` to register language-specific codefence renderers. (by @neilberkman in https://github.com/kivikakk/comrak/pull/751)
* Added `CodefenceRendererAdapter` for language-specific codefence rendering. Its `write` method receives parsed codefence language and metadata (`lang`, `meta`) alongside code and source position. (by @neilberkman in https://github.com/kivikakk/comrak/pull/751)
* Add `++insert++` extension and guillemet smart punctuation. (by @neilberkman in https://github.com/kivikakk/comrak/pull/754)
* Add Typst formatter. (by @neilberkman in https://github.com/kivikakk/comrak/pull/763)
* Add `compact_html` render option to suppress newlines in pretty-printing. (by @xvchris in https://github.com/kivikakk/comrak/pull/769)

Bug fixes:

* Don't wrap text in table cells in CommonMark output. (by @cinerea0 in https://github.com/kivikakk/comrak/pull/737)
* Fix incorrect `sourcepos` for inserted table cells. (by @Martin005 in https://github.com/kivikakk/comrak/pull/747)
* Fix HEEx nested element edge cases. (by @leandrocp in https://github.com/kivikakk/comrak/pull/749)
* Fix off-by-one in hex entity digit limit. (by @neilberkman in https://github.com/kivikakk/comrak/pull/753)

Dependency updates:

* Replace `unicode_categories` with `finl_unicode` for Unicode character categories. (by @Martin005 in https://github.com/kivikakk/comrak/pull/757)
* Bump bon from `3.8.2` to `3.9.0`. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/759)
* Bump clap from `4.5.54` to `4.5.60`. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/748, https://github.com/kivikakk/comrak/pull/756, https://github.com/kivikakk/comrak/pull/758, https://github.com/kivikakk/comrak/pull/766)
* Bump strum from `0.27.2` to `0.28.0`. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/765)
* Bump time from `0.3.36` to `0.3.47` in `/fuzz`. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/744)
* Bump toml from `0.9.10+spec-1.1.0` to `1.0.3+spec-1.1.0`. (by `@dependabot[bot]` in https://github.com/kivikakk/comrak/pull/742, https://github.com/kivikakk/comrak/pull/760, https://github.com/kivikakk/comrak/pull/767)

Documentation:

* Add `alerts` (admonitions) to README. (by @kritzelkrak in https://github.com/kivikakk/comrak/pull/761)
* Explain column counting in `LineColumn`, and fix shortcode context link. (by @Martin005 in https://github.com/kivikakk/comrak/pull/764)


## New Contributors

* @cinerea0 made their first contribution in https://github.com/kivikakk/comrak/pull/737
* @JeanMertz made their first contribution in https://github.com/kivikakk/comrak/pull/743
* @neilberkman made their first contribution in https://github.com/kivikakk/comrak/pull/751
* @kritzelkrak made their first contribution in https://github.com/kivikakk/comrak/pull/761
* @xvchris made their first contribution in https://github.com/kivikakk/comrak/pull/769

Diff: https://github.com/kivikakk/comrak/compare/v0.50.0...v0.51.0


# [v0.50.0] - 2026-01-22

The big news is that I've updated Comrak to use Rust 2024, which means our MSRV has been updated to
1.85.  I'm sorry if this affects you negatively!  Keeping dependencies up-to-date while respecting
our MSRV is basically impossible without the [MSRV
resolver](https://rust-lang.github.io/rfcs/3537-msrv-resolver.html), and I've spent far too many
hours of my short life trying to do without it.  Thank you for your understanding 🤍

Bug fixes:

* Incorrect `sourcepos` for lists before CodeBlocks fixed. (by @Martin005 in https://github.com/kivikakk/comrak/pull/712)
* Incorrect `sourcepos` for HTML and HEEx blocks inside blockquotes fixed. (by @Martin005 in https://github.com/kivikakk/comrak/pull/714)
* HTML block in blockquote in multiline block quotes had no content; now it does! (by @Martin005 in https://github.com/kivikakk/comrak/pull/713)
* Fixed some bugs with HEEx components mixed with Markdown. (by @leandrocp in https://github.com/kivikakk/comrak/pull/719)

Performance:

* Reduced some heap allocations and improved hash function performance. (by @gjtorikian in https://github.com/kivikakk/comrak/pull/717)

Dependency updates:

* A whole _lot_ of Dependabot updates:
  * https://github.com/kivikakk/comrak/pull/711
  * https://github.com/kivikakk/comrak/pull/721
  * https://github.com/kivikakk/comrak/pull/722
  * https://github.com/kivikakk/comrak/pull/723
  * https://github.com/kivikakk/comrak/pull/724
  * https://github.com/kivikakk/comrak/pull/725
  * https://github.com/kivikakk/comrak/pull/726
  * https://github.com/kivikakk/comrak/pull/730
  * https://github.com/kivikakk/comrak/pull/733
  * https://github.com/kivikakk/comrak/pull/734
  * https://github.com/kivikakk/comrak/pull/735
* Upgrade to edition 2021, then 2024. (by @kivikakk in https://github.com/kivikakk/comrak/pull/731, https://github.com/kivikakk/comrak/pull/732)
  * The latter PR includes even more dependency updates (using the v3 resolver), using the latest versions possible on 1.85.

Documentation:

* Added a policy on LLM-generated code. (by @kivikakk in https://github.com/kivikakk/comrak/pull/718)

Build changes:

* Dependency updates to help building on LoongArch64. (by @kivikakk in https://github.com/kivikakk/comrak/pull/708)
* Don't run the pathological test suite on RISC-V. (by @kivikakk in https://github.com/kivikakk/comrak/pull/710)
* Turns out the Darwin builds were also linking to the Nix store! (by @kivikakk in https://github.com/kivikakk/comrak/pull/736)

Behind the scenes:

* Integrated CodSpeed benchmarks on PRs. (by @kivikakk in https://github.com/kivikakk/comrak/pull/715)


Diff: https://github.com/kivikakk/comrak/compare/v0.49.0...v0.50.0


# [v0.49.0] - 2025-12-09

New APIs:

* Support for Phoenix HEEx has been added! Comrak can now avoid interpreting the contents of HEEx tags, representing them in their own node type in the AST. (by @leandrocp in https://github.com/kivikakk/comrak/pull/693, https://github.com/kivikakk/comrak/pull/702)
* Add sourcepos for the task item symbol (i.e. the `x` in `[x]`). (by @kivikakk in https://github.com/kivikakk/comrak/pull/705)

Build changes:

* Fix typos in repository and add typos CI config. (by @liamwhite in https://github.com/kivikakk/comrak/pull/691)
* Promote clippy warns to errors in CI. (by @leandrocp in https://github.com/kivikakk/comrak/pull/701)

Behind the scenes:

* Clean up `find_special_char` logic for inlines. (by @liamwhite in https://github.com/kivikakk/comrak/pull/692)
* Add `byte_matches` helper to simplify `get`/`map_or` chaining tests in parser. (by @liamwhite in https://github.com/kivikakk/comrak/pull/694)
* Apply clippy suggestions. (by @leandrocp in https://github.com/kivikakk/comrak/pull/700)
* DRY out the `create_formatter` macro a bit. (by @nberlette in https://github.com/kivikakk/comrak/pull/704)

## New Contributors

* @nberlette made their first contribution in https://github.com/kivikakk/comrak/pull/704

Diff: https://github.com/kivikakk/comrak/compare/v0.48.0...v0.49.0


# [v0.48.0] - 2025-11-13

The breaking changes are listed right at the top!  Please note that AST content now represents `NUL` bytes (codepoint number zero) as they were in the input; these used to be translated to the lovely � character at the very beginning of the input process, presumably so the rest of the reference C parser didn't have to deal with the possibility of strings containing NUL bytes.  We can do better, though, so let's!  The � character is now emitted by our formatters in place of `NUL`, but if you use custom or manual formatters and emit any part of the AST content **directly** (without using `comrak::html::escape`, `context::html::escape_href`, or the same-named functions on `Context`), you may need to do the same translation yourself.

We also no longer append a newline to the end of the file where there wasn't one originally, which meant a lot of places in the parser had to adapt to their strings not necessarily containing a newline before they ended.  Careful review and extensive fuzzing should have squeaked out any unexpected overruns, but consider my eyes peeled for reports regarding this.  (Ew.)  We've cleaned up some sourcepos calculation which depended on this behaviour in odd ways, but there may yet be more to discover which our test suite didn't catch.

Did you know November is [Trans Month](https://www.transmonth.org.au/)? I didn't! I'm guessing it's because Trans Awareness Week falls within it, and we've been having a pretty bad time of it rights-wise around the world lately!

Happy Trans Month, and if you happen to typo it as Trans Moth, we can be happy about that too! 🏳️‍⚧️ ᖭི༏ᖫྀ 

Parser changes:

* No longer translate `NUL` bytes into `U+FFFD REPLACEMENT CHARACTER` in the parse stage; do it in formatters instead. (by @kivikakk in https://github.com/kivikakk/comrak/pull/681)
  * This means the AST now contains `NUL` bytes where they were present in input, preserving the difference between `NUL` and literally-entered `�` characters.
* No longer append a virtual newline at the end of the file where missing. (by @kivikakk in https://github.com/kivikakk/comrak/pull/682)
  * The spec allows a line to end with either a newline or EOF; the reference parser would assume any given input string will always have a terminating linefeed and forced that to be the case, and so Comrak used to. Comrak no longer does.
  * We also now handle line feed, carriage return, and carriage return plus line feed (as allow'd by the spec) without pretending they're all just a line feed, meaning e.g. sourcepos for softbreaks now correctly spans two bytes when it was produced by a carriage return plus line feed.

Changed APIs:

* Remove mandatory space before fenced codeblock info string in CommonMark output. (by @kivikakk in https://github.com/kivikakk/comrak/pull/686)
* Write out `%25` in hrefs where not part of a percent-encode sequence. (by @kivikakk in https://github.com/kivikakk/comrak/pull/687)
  * We used to leave any `%` character alone, such that `[link](%%20)` would roundtrip without change. It now roundtrips to `[link](%25%20)`.
* Relaxed tasklist matching now supports a full Unicode scalar for the character between the `[…]`, and no longer turns single-byte UTF-8 characters into the Unicode codepoint numbered at the UTF-8 byte (!). (by @kivikakk in https://github.com/kivikakk/comrak/pull/689)

New APIs:

* Add `highlight` extension, `==for highlights==`! These render with `<mark>` in the HTML formatter. (by @pferreir in https://github.com/kivikakk/comrak/pull/672)
* Add `comrak::Node<'a>` as an alias for `comrak::nodes::Node<'a>`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/673)
* Add `comrak::Arena<'a>` as an alias for `typed_arena::Arena<comrak::nodes::AstNode<'a>>`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/675)
* Add `From<(LineColumn, LineColumn)>` impl for `Sourcepos`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/675)
* Make `comrak::nodes::NodeValue::xml_node_name` public, when you want a handy-to-access name for a node type. (by @kivikakk in https://github.com/kivikakk/comrak/pull/673)
* Add `options.parse.leave_footnote_definitions`; this option causes footnote definitions to not be relocated to the bottom of the document, and unused references not to be garbage collected, for use with custom formatters. (by @kivikakk in https://github.com/kivikakk/comrak/pull/673)

Bug fixes:

* Fix relaxed autolink email in footnote edge case/panic. (by @kivikakk in https://github.com/kivikakk/comrak/pull/677)
* Prevent unexpected post-processing, such as `\[x]` still being eligible for tasklist inclusion despite the escaped `[`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/679)

Performance:

* Simplify internal feed function, no longer requiring any allocation before the block parser. (by @kivikakk in https://github.com/kivikakk/comrak/pull/679, https://github.com/kivikakk/comrak/pull/681)
  * This is possible due to not translating `NUL` and not appending a virtual EOF newline; compare [before](https://github.com/kivikakk/comrak/blob/4997b3f0579fadc24d68d37b69a6ae1fea302289/src/parser/mod.rs#L128-L214) and [after](https://github.com/kivikakk/comrak/blob/95fc43bc84e3069a832c8c2ed01c6cda5e2e7fd6/src/parser/mod.rs#L124-L165).
* Don't buffer CommonMark output unless necessary. (by @kivikakk in https://github.com/kivikakk/comrak/pull/684)
  * The full output was always buffered in a string before being written to the destination, which in many cases is going to be another string.  Buffering is now done only to the extent required by output options, which often will be "not at all."
* Use SIMD for core line feed process. (by @kivikakk in https://github.com/kivikakk/comrak/pull/688)

Build changes:

* We now build Linux release binaries against musl, making them actually useful for anyone not running my exact Nix build :') (by @kivikakk in https://github.com/kivikakk/comrak/pull/671)
* The benchmark CI job no longer causes the whole PR to fail checks if it can't post its comment. (by @kivikakk in https://github.com/kivikakk/comrak/pull/674)

Behind the scenes:

* Factor out `inlines::Scanner`, reducing some needless allocations. (by @kivikakk in https://github.com/kivikakk/comrak/pull/675)
* The `all_options` fuzzer now fuzzes *across* all options, and not just with most of them switched on. (by @kivikakk in https://github.com/kivikakk/comrak/pull/678)

## New Contributors

* @pferreir made their first contribution in https://github.com/kivikakk/comrak/pull/672

Diff: https://github.com/kivikakk/comrak/compare/v0.47.0...v0.48.0


# [v0.47.0] - 2025-10-30

Martin Chrástek has fixed _all known sourcepos issues_ in Comrak, while closing a number of other bugs at the same time! I'm so happy.

New APIs:

* `NodeCodeBlock` now has a `closed` property. (by @Martin005 in https://github.com/kivikakk/comrak/pull/661)
* `NodeHeading` now has a `closed` property, for closed ATX-style headings. (by @Martin005 in https://github.com/kivikakk/comrak/pull/665)

Bug fixes:

* Source position information for lists and their children is fixed. (by @Martin005 in https://github.com/kivikakk/comrak/pull/666)
* Source position information for unclosed fenced code blocks is fixed. (by @Martin005 in https://github.com/kivikakk/comrak/pull/661)
* `Escaped` and `EscapedTag` no longer fail AST validation when formatting as CommonMark with debug assertions. (by @kivikakk in https://github.com/kivikakk/comrak/pull/662, https://github.com/kivikakk/comrak/pull/664)

Build changes:

* The fuzzer now also runs on CommonMark and XML output formats. (by @kivikakk in https://github.com/kivikakk/comrak/pull/663)

Diff: https://github.com/kivikakk/comrak/compare/v0.46.0...v0.47.0


# [v0.46.0] - 2025-10-28

Please note the MSRV has been bumped from 1.65 to 1.70; see [the pull request](https://github.com/kivikakk/comrak/pull/649) for more details. It's a kind of sticky and awkward situation — thanks to the inevitability of Progress — with no particularly clean solution. (wherein telling GCC 15 users "sorry it just won't build from source for you without messing with dependencies" is not a solution.)

Security:

* Footnote resolution no longer recurses over the document tree; on documents with deeply nested elements, this could cause a stack overflow, with resultant denial of service. (by @kivikakk in https://github.com/kivikakk/comrak/pull/659)
* Inline footnotes are restricted to a depth of 5 for similar reasons. An iterative rewrite here to avoid a limit is possible, but for now I'm hoping we can all pretend to be responsible adult human beings and limit our recursive inline footnote usage accordingly. (PRs welcome tho, non-human users are very welcome!) (by @kivikakk in https://github.com/kivikakk/comrak/pull/659)

Parser changes:

* U+2069 POP DIRECTIONAL ISOLATE will be treated as terminating an autolink, rather than included as part of the link, making autolinks much easier to use correctly in RTL text. (by @SethFalco in https://github.com/kivikakk/comrak/pull/654)
* HTML start condition 4 is correctly detected when non-capital letters follow "<!". (by @kivikakk in https://github.com/kivikakk/comrak/pull/658)

New APIs:

* Discord-style subtext support is added as the `subtext` extension. (by @Kuuuube in https://github.com/kivikakk/comrak/pull/648, https://github.com/kivikakk/comrak/pull/650)

Bug fixes:

* Source position information is corrected for description lists, HTML blocks, multiline block quotes, links with newlines following the destination, tables with leading indentation, and escaped character spans. (by @Martin005 in https://github.com/kivikakk/comrak/pull/646, https://github.com/kivikakk/comrak/pull/651, https://github.com/kivikakk/comrak/pull/652, https://github.com/kivikakk/comrak/pull/653, https://github.com/kivikakk/comrak/pull/656, https://github.com/kivikakk/comrak/pull/657)
* `escaped_char_span` users can now successfully format to CommonMark with debug assertions enabled. These ASTs previously did not validate, which currently is enabled experimentally only in CommonMark output in debug. (by @kivikakk in https://github.com/kivikakk/comrak/pull/659)

Build changes:

* Comrak's MSRV is bumped from 1.65 to 1.70. (by @kivikakk in https://github.com/kivikakk/comrak/pull/649)

## New Contributors

* @Martin005 made their first contribution in https://github.com/kivikakk/comrak/pull/646
* @Kuuuube made their first contribution in https://github.com/kivikakk/comrak/pull/648
* @SethFalco made their first contribution in https://github.com/kivikakk/comrak/pull/654

Diff: https://github.com/kivikakk/comrak/compare/v0.45.0...v0.46.0


# [v0.45.0] - 2025-10-23

Welcome to v0.45.0! This is a big update, much of them part of from [rc.1 from last week](https://github.com/kivikakk/comrak/releases/tag/v0.45.0-rc.1).  More context on the size of the update in the changelog there.

The biggest library user-facing changes are ergonomic: `Node<'a>` instead of `&'a AstNode<'a>`, is nice, and so likewise `node.data()` instead of `node.data.borrow()`.  They're small, but I appreciate them a lot in my own work.

You'll also notice more bovine creatures in the Comrak pasture: there's a few `Cow<str>` instead of `String`, such as in `NodeValue::Text`.  At most an extra `.into()` will be required; take note if you use any `'static str`, as they'll no longer need to be heap-allocated.  Some `Box`es have been added, too, to reduce the size of every `NodeValue`.  Let the types guide you.

Other than this, the options have been put in their own module (`comrak::options`), and a lot of things generally cleaned up. Read below for all the deets!  Here's the final performance comparison to v0.44.0 on aarch64:

```
Benchmark 1: ./bench.sh ./comrak-0.44.0
  Time (mean ± σ):      88.1 ms ±   1.9 ms    [User: 71.2 ms, System: 17.8 ms]
  Range (min … max):    86.2 ms …  93.2 ms    31 runs

Benchmark 2: ./bench.sh ./comrak-0.45.0
  Time (mean ± σ):      67.0 ms ±   1.2 ms    [User: 51.2 ms, System: 17.0 ms]
  Range (min … max):    65.2 ms …  70.0 ms    42 runs

Summary
  ./bench.sh ./comrak-0.45.0 ran
    1.32 ± 0.04 times faster than ./bench.sh ./comrak-0.44.0
```

Be well!

Parser changes:

* Runs of more than two `~` are no longer recognised as valid delimiters, meaning they will not prevent strikethrough recognition when they occur within correct delimiters. See the PR for discussion. (by @miketheman in https://github.com/kivikakk/comrak/pull/635)
  * This does not impact spec compatibility, matches `cmark-gfm`, and follows the intent of the original implementation and implementor (hi!).

Changed APIs:

* `r#unsafe` is used instead of `unsafe_`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/640)
* `--gemojis` is renamed to `--gemoji`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/641)
* `NodeValue::Text` now contains a `Cow<'static, str>` instead of a `String`. This is a pretty major change, but means we can now create text nodes with static content without duplicating the string on the heap. This particularly benefits smart quotes and HTML entity resolution. (by @kivikakk in https://github.com/kivikakk/comrak/pull/627)
  * Adapting to this change usually means nothing on the read-only side (you can use it as a `&str` without issues); to write in-place, use `.to_mut()` on the `Cow` to get a `&mut String`. To assign, use `.into()` on a `&str` or `String`, like `NodeValue::Text("moo".into())`.
  * `NodeValue::text()` now returns a `&str`. It used to return a `&String` (!).
  * `NodeValue::text_mut()` now returns a `&mut Cow<'static, str>`, instead of a `&mut String`. This permits writing a borrowed reference.
  * I am experimenting with parameterising the lifetime on the `Cow`; it'd be amazing to refer continuously to the input where possible.
* `NodeValue`'s `CodeBlock`, `Table`, `Link`, `Image`, `ShortCode` and `Alert` variants' payloads are now boxed. (by @kivikakk in https://github.com/kivikakk/comrak/pull/632)
  * Adapting to this change usually means adding a `Box::new` call when constructing these nodes, and on matches, pulling the box out and then just dereferencing it directly (e.g. `NodeValue::Table(nt) => &nt.alignments` instead of `NodeValue::Table(NodeTable { ref alignments })`.
  * These payloads were larger than average, increasing the size of every node considerably. The changes reduce an `Ast` to 128 bytes, and a full `AstNode<'_>` to 176 bytes.
  * This produces a performance sweet spot: boxing the whole `NodeValue` results in worse performance than doing nothing at all. This change appreciably improves matters.
  * We now assert the size of a node during build to ensure future payload changes don't increase the total size of an `Ast`.
* Options now live in `comrak::options`. Structs have been renamed to remove `Options` from their name: `comrak::RenderOptions` is now `comrak::options::Render`, etc. The old names are marked deprecated. (@kivikakk in https://github.com/kivikakk/comrak/pull/636)
  * Traits cannot be aliased [yet](https://github.com/rust-lang/rust/issues/41517) :( `URLRewriter` and `BrokenLinkCallback` have been moved, without a deprecation period.
* `SyntaxHighlighterAdapter`'s `attributes` arguments now take `HashMap<&'static str, Cow<'s, str>>`; they used to take `HashMap<String, String>`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/633)
* `html::write_opening_tag` can now take different `AsRef<str>` types for the attribute key and value.
* `parse_document_with_broken_link_callback` has been removed!  This entrypoint has been deprecated since 0.25.0. (by @kivikakk in https://github.com/kivikakk/comrak/pull/623)
* `options.render.ignore_setext` was moved to `options.parse.ignore_setext`, as its effect takes place only in the parse stage. (by @kivikakk in https://github.com/kivikakk/comrak/pull/623)
* `nodes::can_contain_type` is now `Node::can_contain_type`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/625)


New APIs:

* `node.data()` and `node.data_mut()` are added as short-hand for `node.data.borrow()` and `node.data.borrow_mut()` respectively. (by @kivikakk in https://github.com/kivikakk/comrak/pull/643)
* `comrak::nodes::Node<'a>` is introduced as an alias for `&'a comrak::nodes::AstNode<'a>`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/627)
* `options.parse.tasklist_in_table` added: parse a tasklist item if it's the only content of a table cell. (by @kivikakk in https://github.com/kivikakk/comrak/pull/622)

Performance:

* Inline content is transferred to Text nodes without copying where possible. (by @kivikakk in https://github.com/kivikakk/comrak/pull/642).
* Have you looked at your 7 year old code lately? A detail in the C-to-Rust translation meant essentially every line of input was being copied completely unnecessarily at the very beginning of the line processing stage. This no longer happens. We regret the error. (by @kivikakk in https://github.com/kivikakk/comrak/pull/629)
* Preprocess entity data at build-time so we don't spend time doing a linear search over an unsorted array, some of which we will never match. (by @kivikakk in https://github.com/kivikakk/comrak/pull/631)
* Inline content is consumed by the inline processor, instead of being borrowed by it and retained in memory indefinitely. (by @kivikakk in https://github.com/kivikakk/comrak/pull/631)
* Don't try to do better than the stdlib at guessing buffer sizes; it's very good at it. (by @kivikakk in https://github.com/kivikakk/comrak/pull/626)
* Use `str` internally in block and inline processing, eliminating many UTF-8 rechecks. The `strings` module actually operates on strings now. (by @kivikakk in https://github.com/kivikakk/comrak/pull/626)
* Many, many needless clones have been removed in almost every subsystem.

Dependency updates:

* `memchr` removed from `Cargo.toml`; it wasn't used directly, though it still is included unconditionally due to `caseless`. (by @kivikakk in https://github.com/kivikakk/comrak/pull/630)
* `slug` is moved to a development-only dependency; it's only used in an example. (by @kivikakk in https://github.com/kivikakk/comrak/pull/630)
* `jetscii` is added for faster string searching, including SIMD on x86_64. (by @kivikakk in https://github.com/kivikakk/comrak/pull/630)
  * I'm [experimenting](https://github.com/kivikakk/comrak/pull/634) with aarch64 SIMD.

Documentation:

* The CLI help text has been copy-edited to a consistent style. (by @kivikakk in https://github.com/kivikakk/comrak/pull/641)
* The `README` example code is updated to build with recent API changes. (by @kivikakk in https://github.com/kivikakk/comrak/pull/621)

Build changes:

* `shortcodes` is enabled by default (but still optional) for CLI builds. (by @kivikakk in https://github.com/kivikakk/comrak/pull/641)
* `syntect` is now optional (but still default) in CLI builds. (by @kivikakk in https://github.com/kivikakk/comrak/pull/624)

Behind the scenes:

* Much of the block parser code has been re-organised, and many C-isms from the original port have been refactored into readable Rust. (by @kivikakk in https://github.com/kivikakk/comrak/pull/627)
* Likewise the inline parser has been re-organised. (by @kivikakk in https://github.com/kivikakk/comrak/pull/644)
* All `unsafe` blocks now have a `SAFETY` comment describing why their actions are safe.

## New Contributors

* @miketheman made their first contribution in https://github.com/kivikakk/comrak/pull/635

Diff: https://github.com/kivikakk/comrak/compare/v0.44.0...v0.45.0


# [v0.44.0] - 2025-10-14

Parser changes:

* Autolink validation is now stricter in the default mode, to maintain conformance with the GitHub Flavored Markdown autolinks extension spec. Those parses which previously worked but no longer do --- such as `http://localhost` (!), `www.com` (!?), or `https://` (!?!) --- are now part of the `relaxed_autolinks` option. See more discussion in the PR. (by @chamlis in https://github.com/kivikakk/comrak/pull/618)

New APIs:

* You can write footnotes with their body inline by enabling the `inline_footnotes` extension and using the syntax `^[footnote content]` (by @sheremetyev in https://github.com/kivikakk/comrak/pull/619)

## New Contributors

* @sheremetyev made their first contribution in https://github.com/kivikakk/comrak/pull/619
* @chamlis made their first contribution in https://github.com/kivikakk/comrak/pull/618

Diff: https://github.com/kivikakk/comrak/compare/v0.43.0...v0.44.0


# [v0.43.0] - 2025-09-29

Parser changes:

* `superscript` or `subscript` extensions only: punctuation following a superscript or subscript delimiter no longer disqualifies the delimiter from being considered left-flanking, such that `e^-i^` and `n~-i~` now parse as superscript or subscript respectively (by @kivikakk in https://github.com/kivikakk/comrak/pull/593)

Changed APIs:

* `html::format_document`, `xml::format_document`, `cm::format_document` and friends now take an `std::fmt::Write` as their `output` argument, instead of an `std::io::Write`, to avoid revalidating UTF-8 (by @kivikakk in https://github.com/kivikakk/comrak/pull/601)
* bin: allow `--header-ids ''` for prefix-less headers (by @kivikakk in https://github.com/kivikakk/comrak/pull/610)

New APIs:

* Add CJK Friendly Emphasis to CLI option (by @tats-u in https://github.com/kivikakk/comrak/pull/607)

Documentation updates:

* Introduce `cargo binstall` by (@tats-u in https://github.com/kivikakk/comrak/pull/608)
* www: add (by @kivikakk in https://github.com/kivikakk/comrak/pull/611)

Diff: https://github.com/kivikakk/comrak/compare/v0.42.0...v0.43.0


# [v0.42.0] - 2025-09-24

New APIs:

* `cm::escape_inline` (aliased at crate level as `escape_commonmark_inline`)
  is added; escapes input text suitable for inclusion in a CommonMark
  document where regular inline processing takes place.  (by @kivikakk in
  https://github.com/kivikakk/comrak/pull/602)
* `cm::escape_link_destination` (aliased at crate level as
  `escape_commonmark_link_destination`) is added; escapes input URL suitable
  for use as a link destination in a CommonMark document.  (by @kivikakk in
  https://github.com/kivikakk/comrak/pull/605)

Changed APIs:

* `html::collect_text` now returns a `String`.  `html::collect_text_append` is
  added if you still want to start with your own (`String`) buffer.  (by
  @kivikakk in https://github.com/kivikakk/comrak/pull/600)
  * There was no particular reason for this populating a `Vec<u8>` instead of a
    `String`; it was just old.
* `Anchorizer::anchorizer` now takes `&str` instead of a `String`.  (by
  @kivikakk in https://github.com/kivikakk/comrak/pull/603)
  * As above.

Updates:

* Update `is_cjk` in CJK Friendly Emphasis to Unicode 17.  (by @tats-u in
  https://github.com/kivikakk/comrak/pull/598)

Behind the scenes:

* Rename a bunch of "nch" to "nh". (by @kivikakk in
  https://github.com/kivikakk/comrak/pull/599)

Diff: https://github.com/kivikakk/comrak/compare/v0.41.1...v0.42.0


# [v0.41.1] - 2025-09-14

Bug fixes:

* Fix the range of non-emoji general purpose variation selector by @tats-u in https://github.com/kivikakk/comrak/pull/596

Stability:

* html: remove some panics on unusual ASTs, and document others. by @kivikakk in https://github.com/kivikakk/comrak/pull/589

Behind the scenes:

* Cleanup fuzzers, add unresolved `relaxed_autolink_email_in_footnote` test by @Mrmaxmeier in https://github.com/kivikakk/comrak/pull/594
* build(deps): bump actions/checkout from 4 to 5 by @dependabot[bot] in https://github.com/kivikakk/comrak/pull/590

## New Contributors

* @Mrmaxmeier made their first contribution in https://github.com/kivikakk/comrak/pull/594

Diff: https://github.com/kivikakk/comrak/compare/v0.41.0...v0.41.1


# [v0.41.0] - 2025-08-09

New features:

* Add CJK friendly emphasis extension by @tats-u in https://github.com/kivikakk/comrak/pull/582
  * Add CJK friendly emphasis to README by @tats-u in https://github.com/kivikakk/comrak/pull/583

Build changes:

* Use syntect's default-fancy feature for ios by @tvanderstad in https://github.com/kivikakk/comrak/pull/585

## New Contributors

* @tats-u made their first contribution in https://github.com/kivikakk/comrak/pull/582
* @tvanderstad made their first contribution in https://github.com/kivikakk/comrak/pull/585

Diff: https://github.com/kivikakk/comrak/compare/v0.40.0...v0.41.0


# [v0.40.0] - 2025-07-18

Reverts:

* "Fix header-ids accessibility" was reverted in https://github.com/kivikakk/comrak/commit/2cb6188bfb4c5d69bf55c73c19b2049e9dfe5dba
  * See discussion at https://github.com/kivikakk/comrak/pull/574#issuecomment-3046307805 for details.

Bug fixes:

* html: don't escape IPv6 address square bracket delimiters in links. by @kivikakk in https://github.com/kivikakk/comrak/pull/578

New APIs:

* Syntect syntax highlighting: Make a CSS class prefix configurable. by @SteveBinary in https://github.com/kivikakk/comrak/pull/580

## New Contributors

* @SteveBinary made their first contribution in https://github.com/kivikakk/comrak/pull/580

Diff: https://github.com/kivikakk/comrak/compare/v0.39.1...v0.40.0


# [v0.39.1] - 2025-06-27

Bug fixes:

* Fix header-ids accessibility by @davemackintosh in https://github.com/kivikakk/comrak/pull/574
* Recursively join text nodes inside links, images, and wikilinks by @JamieMagee in https://github.com/kivikakk/comrak/pull/575

## New Contributors

* @davemackintosh made their first contribution in https://github.com/kivikakk/comrak/pull/574
* @JamieMagee made their first contribution in https://github.com/kivikakk/comrak/pull/575

Diff: https://github.com/kivikakk/comrak/compare/v0.39.0...v0.39.1


# [v0.39.0] - 2025-05-02

New APIs:

* Make dangerous_url public by @digitalmoksha in https://github.com/kivikakk/comrak/pull/564

Diff: https://github.com/kivikakk/comrak/compare/v0.38.0...v0.39.0


# [v0.38.0] - 2025-04-05

Bug fixes:

* Only delete parent if the node has no siblings by @digitalmoksha in https://github.com/kivikakk/comrak/pull/559

New features/APIs:

* html: add user data to context. by @kivikakk in https://github.com/kivikakk/comrak/pull/555

Coming of age???:

* experimental-inline-sourcepos: not really experimental any more! by @kivikakk in https://github.com/kivikakk/comrak/pull/560

Diff: https://github.com/kivikakk/comrak/compare/v0.37.0...v0.38.0


# [v0.37.0] - 2025-03-25

Bug fixes:

* add --all-features to CI matrix, add missing shortcode case. by @charlottia in https://github.com/kivikakk/comrak/pull/546
* Inline sourcepos fixes. by @kivikakk in https://github.com/kivikakk/comrak/pull/542

Documentation:

* docs: add mdex (elixir bindings) in the list of related projects by @leandrocp in https://github.com/kivikakk/comrak/pull/547
* Add Commonmarker to related projects by @gjtorikian in https://github.com/kivikakk/comrak/pull/548

Diff: https://github.com/kivikakk/comrak/compare/v0.36.0...v0.37.0


# [v0.36.0] - 2025-02-28

Bug fixes:

* Stop at first suitable $ at inline math by @Bubbis in https://github.com/kivikakk/comrak/pull/533

New features/APIs:

* Create custom HTML formatters. by @kivikakk in https://github.com/kivikakk/comrak/pull/540
* make `AlertType` methods public by @fiji-flo in https://github.com/kivikakk/comrak/pull/532
* commonmark: experimental minimize by @charlottia in https://github.com/kivikakk/comrak/pull/523

Behind the scenes:

* tests: don't let sourcepos stand in the way of roundtrip tests. by @kivikakk in https://github.com/kivikakk/comrak/pull/543
* Refactor html output functions by @digitalmoksha in https://github.com/kivikakk/comrak/pull/529
* Write a sourcepos test for each `NodeValue` variant by @SamWilsn in https://github.com/kivikakk/comrak/pull/498

Documentation:

* docs: add Python bindings repo and PyPI links to Related Projects by @lmmx in https://github.com/kivikakk/comrak/pull/539

## New Contributors

* @Bubbis made their first contribution in https://github.com/kivikakk/comrak/pull/533
* @lmmx made their first contribution in https://github.com/kivikakk/comrak/pull/539

Diff: https://github.com/kivikakk/comrak/compare/v0.35.0...v0.36.0


# [v0.35.0] - 2025-01-23

* Use CSS class `markdown-alert` instead of `alert` by @digitalmoksha in https://github.com/kivikakk/comrak/pull/524

Diff: https://github.com/kivikakk/comrak/compare/v0.34.0...v0.35.0


# [v0.34.0] - 2025-01-21

Admonition special!

* Add GitHub style alerts / admonitions by @digitalmoksha in https://github.com/kivikakk/comrak/pull/519
* Enable GitLab multiline alerts by @digitalmoksha in https://github.com/kivikakk/comrak/pull/521

Diff: https://github.com/kivikakk/comrak/compare/v0.33.0...v0.34.0


# [v0.33.0] - 2025-01-04

Happy new year! Thanks to @nicoburns for these changes, enabling much faster
compiles if you don't need the builders!

* Eliminate `regex` and `once_cell` dependencies. by @nicoburns in https://github.com/kivikakk/comrak/pull/514
* Make bon builders optional by @nicoburns in https://github.com/kivikakk/comrak/pull/515
* Make options structs exhaustive by @nicoburns in https://github.com/kivikakk/comrak/pull/516

Diff: https://github.com/kivikakk/comrak/compare/v0.32.0...v0.33.0


# [v0.32.0] - 2024-12-17

* rust-toolchain: remove by @charlottia in https://github.com/kivikakk/comrak/pull/493
* Account for front matter when calculating `sourcepos` by @SamWilsn in https://github.com/kivikakk/comrak/pull/494
* callbacks: constrain to input lifetime by @liamwhite in https://github.com/kivikakk/comrak/pull/499
* Refactor open_new_blocks by @digitalmoksha in https://github.com/kivikakk/comrak/pull/505
* Refactors open_new_blocks by lifting out handlers by @digitalmoksha in https://github.com/kivikakk/comrak/pull/506
* Make `wikilinks_title_after_pipe` override `wikilinks_title_before_pipe` by @SamWilsn in https://github.com/kivikakk/comrak/pull/500
* Detect ending front matter delimiter at EOF by @kivikakk in https://github.com/kivikakk/comrak/pull/508
* Add Raw Node by @wakairo in https://github.com/kivikakk/comrak/pull/511

## New Contributors

* @charlottia made their first contribution in https://github.com/kivikakk/comrak/pull/493
* @SamWilsn made their first contribution in https://github.com/kivikakk/comrak/pull/494
* @wakairo made their first contribution in https://github.com/kivikakk/comrak/pull/511

Diff: https://github.com/kivikakk/comrak/compare/v0.31.0...v0.32.0


# [v0.31.0] - 2024-11-26

* Enhance description lists by @digitalmoksha in https://github.com/kivikakk/comrak/pull/462

Diff: https://github.com/kivikakk/comrak/compare/v0.30.0...v0.31.0


# [v0.30.0] - 2024-11-22

* Add `task-list-item` class to task list items by @nicoburns in https://github.com/kivikakk/comrak/pull/468
* Add option for specifying a minimum width of ordered lists by @edwar4rd in https://github.com/kivikakk/comrak/pull/465
* Use `bon` for an infallible and compile-time-checked builder by @Veetaha in https://github.com/kivikakk/comrak/pull/466
* Add support for image and link URL rewriting by @liamwhite in https://github.com/kivikakk/comrak/pull/481
* Unwrap Mutex from broken_link_callback by @liamwhite in https://github.com/kivikakk/comrak/pull/484
* Prevent panic in format_item by @silverpill in https://github.com/kivikakk/comrak/pull/486
* Bump `bon` version to 3.0 by @Veetaha in https://github.com/kivikakk/comrak/pull/487
* Add support for subscript extension by @liamwhite in https://github.com/kivikakk/comrak/pull/488
* Add macro for character tables by @liamwhite in https://github.com/kivikakk/comrak/pull/490

## New Contributors

* @nicoburns made their first contribution in https://github.com/kivikakk/comrak/pull/468
* @edwar4rd made their first contribution in https://github.com/kivikakk/comrak/pull/465
* @Veetaha made their first contribution in https://github.com/kivikakk/comrak/pull/466

Diff: https://github.com/kivikakk/comrak/compare/v0.29.0...v0.30.0


# [v0.29.0] - 2024-10-10

* Add support for backslash escape in wikilinks by @digitalmoksha in https://github.com/kivikakk/comrak/pull/471

Diff: https://github.com/kivikakk/comrak/compare/v0.28.0...v0.29.0


# [v0.28.0] - 2024-09-05

* Add a render option to render the image as <figure> by @JmPotato in https://github.com/kivikakk/comrak/pull/458
* Fix edge cases for relaxed-autolink option by @digitalmoksha in https://github.com/kivikakk/comrak/pull/461

Diff: https://github.com/kivikakk/comrak/compare/v0.27.0...v0.28.0


# [v0.27.0] - 2024-08-19

* Track line offsets for better accuracy of inline sourcepos by @digitalmoksha in https://github.com/kivikakk/comrak/pull/453
* Add experimental-inline-sourcepos to cli options by @digitalmoksha in https://github.com/kivikakk/comrak/pull/455

Diff: https://github.com/kivikakk/comrak/compare/v0.26.0...v0.27.0


# [v0.26.0] - 2024-07-12

* Restore inline sourcepos as experimental. by @kivikakk in https://github.com/kivikakk/comrak/pull/444
  * This is needed by some downstream users, so we re-introduce it, with
    a clearly labelled option.

Diff: https://github.com/kivikakk/comrak/compare/v0.25.0...v0.26.0


# [v0.25.0] - 2024-07-12

* Discord-flavored Markdown by @Meow and @liamwhite in https://github.com/kivikakk/comrak/pull/421
  * Three new extensions and two render options are added:
    * `extension.underline` adds support for `__underlined__` text.
    * `extension.spoiler` adds support for `||spoiler||` text.
    * `extension.greentext` adds support for image board-style `>greentext`,
      which isn't transformed into a blockquote.
    * `render.ignore_setext` disables parsing setext-style headings.
    * `render.ignore_empty_links` causes links with no text (like `[](xyz)`) to
      remain in the text as-is.
* nodes: add From impls for AstNode. by @kivikakk in https://github.com/kivikakk/comrak/pull/424
  * Back by popular demand: `AstNode::from(NodeValue)`.
  * Also added is `AstNode::from(Ast)`, if you have sourcepos.
* AST validation by @yannham in https://github.com/kivikakk/comrak/pull/425
  * The AST is validated when formatting a document as CommonMark in debug builds.
* Address autolink edge cases. by @kivikakk in https://github.com/kivikakk/comrak/pull/426
  * Autolinks had many edge cases where output differed from upstream
    `cmark-gfm`. These have been fixed by following upstream's parser design
    closely.
* shortcodes: capture all known aliases. by @kivikakk in https://github.com/kivikakk/comrak/pull/427
  * We didn't parse shortcodes containing numbers or `+`. We do now.
* Support both upstream CommonMark and GFM's differences in the base spec. by @kivikakk in https://github.com/kivikakk/comrak/pull/428
  * GFM modifies even base CommonMark output somewhat. We now support and
    validate against both.
* cm: count ol items from start of each list. by @kivikakk in https://github.com/kivikakk/comrak/pull/429
  * Ordered list item numbers are normalised on formatting back to CommonMark.
* arena_tree: panic if iterator invalidation causes trouble. by @kivikakk in https://github.com/kivikakk/comrak/pull/437
  * `arena_tree` would silently stop iteration when trying to proceed from a
    child that had lost its parent. It now panics instead, as the old behaviour
    is incorrect and impossible to notice.
* broken reflink callback updates & big cleanup. by @kivikakk in https://github.com/kivikakk/comrak/pull/438
  * The broken reference link callback has been moved into `ParseOptions` (which
    now takes a lifetime, meaning `Options` does too).
  * The callback now takes a struct containing both the normalised reference,
    and the original text, and the return value has changed from a 2-tuple to a
    struct for clarity.
  * `parse_document_with_broken_link_callback` has been marked deprecated.
* Inline sourcepos fixes. by @kivikakk in https://github.com/kivikakk/comrak/pull/439
  * Inline sourcepos was provided on a best-effort basis, but there are multiple
    correctness issues which can't be fixed without significant work.
  * Inline sourcepos is no longer reported in HTML output. It remains in the AST
    and in XML output, but it is not reliable. See the PR for details.
  * Link sourcepos is slightly better than it was when it spans multiple lines.

## New Contributors

* @liamwhite made their first contribution in https://github.com/kivikakk/comrak/pull/421
* @yannham made their first contribution in https://github.com/kivikakk/comrak/pull/425

Diff: https://github.com/kivikakk/comrak/compare/v0.24.1...v0.25.0


# [v0.24.1] - 2024-05-19

* Add GH_TOKEN to release workflow by @digitalmoksha in https://github.com/kivikakk/comrak/pull/418

Diff: https://github.com/kivikakk/comrak/compare/v0.24.0...v0.24.1


# [v0.24.0] - 2024-05-19

* Miscellany. by @kivikakk in https://github.com/kivikakk/comrak/pull/387
* Add automation to release new crates by @gjtorikian in https://github.com/kivikakk/comrak/pull/374
* build(deps): bump emojis from 0.5.2 to 0.6.2 by @dependabot in https://github.com/kivikakk/comrak/pull/393
* build(deps): bump arbitrary from 1.3.0 to 1.3.2 by @dependabot in https://github.com/kivikakk/comrak/pull/394
* build(deps): bump actions/checkout from 3 to 4 by @dependabot in https://github.com/kivikakk/comrak/pull/389
* build(deps): bump once_cell from 1.17.0 to 1.19.0 by @dependabot in https://github.com/kivikakk/comrak/pull/390
* build(deps): bump xdg from 2.4.1 to 2.5.2 by @dependabot in https://github.com/kivikakk/comrak/pull/391
* build(deps): bump derive_builder from 0.12.0 to 0.20.0 by @dependabot in https://github.com/kivikakk/comrak/pull/392
* build(deps): bump memchr from 2.5.0 to 2.7.2 by @dependabot in https://github.com/kivikakk/comrak/pull/396
* build(deps): bump ntest from 0.9.0 to 0.9.2 by @dependabot in https://github.com/kivikakk/comrak/pull/397
* build(deps): bump typed-arena from 2.0.1 to 2.0.2 by @dependabot in https://github.com/kivikakk/comrak/pull/398
* Update automerge.yml by @gjtorikian in https://github.com/kivikakk/comrak/pull/401
* build(deps): bump clap from 4.0.32 to 4.5.4 by @dependabot in https://github.com/kivikakk/comrak/pull/400
* build(deps): bump regex from 1.7.0 to 1.10.4 by @dependabot in https://github.com/kivikakk/comrak/pull/402
* Fix release workflows by @gjtorikian in https://github.com/kivikakk/comrak/pull/395
* workflows: check MSRV in CI. by @kivikakk in https://github.com/kivikakk/comrak/pull/406
* Add support for wikilinks format by @digitalmoksha in https://github.com/kivikakk/comrak/pull/407
* Autolink should ignore wikilinks by @digitalmoksha in https://github.com/kivikakk/comrak/pull/413
* Bump version to 0.24.0 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/415

Diff: https://github.com/kivikakk/comrak/compare/0.23.0...v0.24.0


# [0.23.0]

* add traverse() demo example by @kaesluder in https://github.com/kivikakk/comrak/pull/370
* Avoid backslashes before a new block. by @jneem in https://github.com/kivikakk/comrak/pull/373
* Expand traverse and descendants documentation: Issue #369 by @kaesluder in https://github.com/kivikakk/comrak/pull/375
* Feat/inplace: add new parameter `--inplace` (`-i`) for in-place formatting by @bioinformatist in https://github.com/kivikakk/comrak/pull/377
* Change `relaxed-autolinks` to allow any url scheme by @digitalmoksha in https://github.com/kivikakk/comrak/pull/380
* Fix sourcepos for setext headers by @digitalmoksha in https://github.com/kivikakk/comrak/pull/381
* Add  iterative search/replace example to examples and README.md by @kaesluder in https://github.com/kivikakk/comrak/pull/383
* un-Nix in CI. by @kivikakk in https://github.com/kivikakk/comrak/pull/384
* Return brackets in autolinks behavior back to cmark-gfm by @digitalmoksha in https://github.com/kivikakk/comrak/pull/386


# [0.22.0]

* Fix broken docs link in README by @ohakutsu in https://github.com/kivikakk/comrak/pull/364
* Make non public nodes public by @mfontanini in https://github.com/kivikakk/comrak/pull/363
* cargo update -p rustix --precise 0.36.17 by @kivikakk in https://github.com/kivikakk/comrak/pull/368
* Add render option to wrap escaped chars in span by @digitalmoksha in https://github.com/kivikakk/comrak/pull/367
* Add math support by @digitalmoksha in https://github.com/kivikakk/comrak/pull/366


# [0.21.0]

* Add a multiline blockquote extension by @digitalmoksha in https://github.com/kivikakk/comrak/pull/359


# [0.20.0]

* build(deps): bump rustix from 0.36.11 to 0.36.16 in /fuzz by @dependabot in https://github.com/kivikakk/comrak/pull/346
* Use Nix for CI. by @charlottia in https://github.com/kivikakk/comrak/pull/338
* Allow for Syntect to simply generate CSS classes by @gjtorikian in https://github.com/kivikakk/comrak/pull/347


# [0.19.0]

* Simplify anchorize() by @kornelski in https://github.com/kivikakk/comrak/pull/297
* Use footnote name for reference id by @digitalmoksha in https://github.com/kivikakk/comrak/pull/300
* Escape footnote name by @digitalmoksha in https://github.com/kivikakk/comrak/pull/308
* Add in-doc labels for public facing features by @CosmicHorrorDev in https://github.com/kivikakk/comrak/pull/304
* build(deps): bump xml-rs from 0.8.4 to 0.8.14 by @dependabot in https://github.com/kivikakk/comrak/pull/312
* Handle footnote names that have been parsed into multiple nodes by @digitalmoksha in https://github.com/kivikakk/comrak/pull/311
* Sync with cmark-gfm-0.29.0.gfm.3 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/313
* Sync with cmark-gfm-0.29.0.gfm.4 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/314
* Sync with cmark-gfm-0.29.0.gfm.5 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/315
* Fix backslash in a link issue by @vpetrigo in https://github.com/kivikakk/comrak/pull/317
* Sync with cmark-gfm-0.29.0.gfm.7 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/318
* Rename `ComrakFoo` types to just `Foo` for easier usage by @tgross35 in https://github.com/kivikakk/comrak/pull/320
* Make `ComrakExtensionOptions` non-exhaustive by @CosmicHorrorDev in https://github.com/kivikakk/comrak/pull/305
* Add builder derive and non_exhaustive for option structs by @YJDoc2 in https://github.com/kivikakk/comrak/pull/292
* add PartialEq and Eq derive for Ast and its components by @YJDoc2 in https://github.com/kivikakk/comrak/pull/322
* Sync with cmark-gfm-0.29.0.gfm.11 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/319
* Fix autolink detection inside wiki style link brackets by @digitalmoksha in https://github.com/kivikakk/comrak/pull/325
* Add CI for running benchmarks by @YJDoc2 in https://github.com/kivikakk/comrak/pull/326
* Make adapters Send + Sync by @lucperkins in https://github.com/kivikakk/comrak/pull/337
* docs: fix-up broken docs.rs link by @silverjam in https://github.com/kivikakk/comrak/pull/341
* Use github/cmark-gfm submodule by @digitalmoksha in https://github.com/kivikakk/comrak/pull/344
* Sync with cmark-gfm-0.29.0.gfm.12 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/343
* Sync with cmark-gfm-0.29.0.gfm.13 by @digitalmoksha in https://github.com/kivikakk/comrak/pull/345


# [0.18.0]

* Improve performance of bundled plugins, and streaming I/O by @kivikakk in https://github.com/kivikakk/comrak/pull/288
* Implement Default for enums without using #[default] attribute by @silverpill in https://github.com/kivikakk/comrak/pull/293
* XML and sourcepos support by @kivikakk in https://github.com/kivikakk/comrak/pull/232
* Add a quadratic fuzzer by @philipturnbull in https://github.com/kivikakk/comrak/pull/295


# [0.17.1]

* Fix some panics found by trivial fuzzing.

Missed from the 0.17.0 changelog:

* Add footnote attributes that mirror cmark-gfm by @digitalmoksha in https://github.com/kivikakk/comrak/pull/273
* Add support for full_info_string render option by @digitalmoksha in https://github.com/kivikakk/comrak/pull/276
* chore: improve debug performance by @conradludgate in https://github.com/kivikakk/comrak/pull/283


# [0.17.0]

This contains some breaking changes from an API point of view, but output is
largely unchanged.  Spec compliance is improved, and benchmark runtime is over
20% faster.

* SECURITY: GHSA-8hqf-xjwp-p67v / Quadratic runtime when parsing Markdown (GHSL-2023-047)
  * <https://github.com/kivikakk/comrak/security/advisories/GHSA-8hqf-xjwp-p67v>
  * A variety of quadratic runtime issues that could lead to DoS were reported
    and addressed.
  * We replaced pest with an re2c-based scanner.
* SECURITY: GHSA-xxmq-4vph-956w / Excessive output when parsing Markdown (GHSL-2023-048)
  * <https://github.com/kivikakk/comrak/security/advisories/GHSA-xxmq-4vph-956w>
  * Reference output is limited to 100Kb.
* SECURITY: GHSA-5r3x-p7xx-x6q5 / Attacker controlled data in AST nodes is not validated (GHSL-2023-049)
  * <https://github.com/kivikakk/comrak/security/advisories/GHSA-5r3x-p7xx-x6q5>
  * AST nodes no longer store raw `Vec<u8>`s, and instead store `String`s.
* Various API points were cleaned up.
* Comrak now targets Rust 2018.

Many thanks to @philipturnbull and @darakian of the GitHub Security Lab for
bringing these issues to my attention and detailing the reproduction steps for
each case.


# [0.16.0]

* Track which symbol was used to mark task item as checked by @felipesere in https://github.com/kivikakk/comrak/pull/252
* improve tagfilter performance by @fiji-flo in https://github.com/kivikakk/comrak/pull/256
* [ShortCode] Add support for gemojis via shortcodes extension by @eklipse2k8 in https://github.com/kivikakk/comrak/pull/260
* "mod three rule" fix by @kivikakk in https://github.com/kivikakk/comrak/pull/262
* Add `shortcodes` to the README by @gjtorikian in https://github.com/kivikakk/comrak/pull/263
* Cargo.toml: remove timebomb by @kivikakk in https://github.com/kivikakk/comrak/pull/264
* Add custom heading adapter by @lucperkins in https://github.com/kivikakk/comrak/pull/266
* Keep track of "^" symbol when within footnotes by @gjtorikian in https://github.com/kivikakk/comrak/pull/274


# [0.15.0]

* table: fix start_line of Table itself by @kivikakk in https://github.com/kivikakk/comrak/pull/231
* Rename header file to match c libname by @gjtorikian in https://github.com/kivikakk/comrak/pull/233
* Change the name of the ifdef by @gjtorikian in https://github.com/kivikakk/comrak/pull/234
* Add `comrak_set_parse_option_smart` by @gjtorikian in https://github.com/kivikakk/comrak/pull/235
* Allow `c_char` options to be NULL by @gjtorikian in https://github.com/kivikakk/comrak/pull/237
* Replace `lazy_static` dependency with `once_cell` by @Turbo87 in https://github.com/kivikakk/comrak/pull/238
* Make `comrak --help` readable on my terminal by @mgeisler in https://github.com/kivikakk/comrak/pull/242
* c-api: fix CI build by @kivikakk in https://github.com/kivikakk/comrak/pull/240
* Bump versions of some dependencies by @helmet91 in https://github.com/kivikakk/comrak/pull/243
* Adding functionality to build SyntectAdapters with custom themes, syntax sets, etc. by @ArvinSKushwaha in https://github.com/kivikakk/comrak/pull/239
* Make shell-words and xdg dependencies optional by @silverpill in https://github.com/kivikakk/comrak/pull/245
* Bump clap version to 4.0 and switch to the Derive API by @tranzystorek-io in https://github.com/kivikakk/comrak/pull/248
* c-api: remove by @kivikakk in https://github.com/kivikakk/comrak/pull/249


# [0.14.0]

* Add C FFI, allowing Comrak to be used from other languages. (#171, Garen
  Torikian)
* Fix line wrapping in CommonMark output. (#228, Edward Loveall)
* Add option to specify character used for unordered list bullets in
  CommonMark output. (#229, Edward Loveall)


# [0.13.2]

* Fix Windows build.


# [0.13.1]

* Support compiling for WASM. (#222, Ben Wishoshavich)
* Replace deprecated twoway dependency. (#224)


# [0.13.0]

* SECURITY: Bump regex to 1.5.5. (#221, Dependabot)
* Drop unneeded YAML dependency from Syntect. (#199, Chris Wong)
* Match newline handling in code inlines to upstream, and improve test failure
  reporting. (#210, Michael Anderson)
* Make all node value fields public. (#216, Evan Schwartz)
* Line break handling adjustments. (#214, Michael Anderson)
* Disable control characters in link definitions. (#219, Michael Anderson)


# [0.12.1]

* Only load syntax and theme sets once, on Syntect plugin instantiation. (#197)
* Match syntax highlighting language names more loosely. (#198)


# [0.12.0]

* Add pluggable syntax highlighting, and default implementation with syntect.
  (Daniel Simon, #194)


# [0.11.0]

* Allow short URLs even with non-empty path. (#191, Bernard Teo)
* Expose NodeCode struct in AST. (#192, Vojtech Kral)


# [0.10.1]

* SECURITY: it was possible to smuggle unsafe URLs --- like `javascript:` ones
  --- even without using the "unsafe" mode of operation.  Thanks to Sam Sanoop
  (snoopysecurity) for reporting.
* Recognise tables without a preceding newline. (#183)


# [0.10.0]

* 0.9.1 was a semver-breaking change.
* Add -o/--output CLI option. (#177)


# [0.9.1]

* SECURITY: we were matching unsafe URL prefixes, such as `data:` or
  `javascript:`, in a case-sensitive manner.  This meant prefixes like `Data:`
  were untouched.  Please upgrade as soon as possible.  (Kouhei Morita)
* Add support for ignoring front matter. (#170, Eitan Mosenkis.)


# [0.9.0]

* 0.8.2 was a semver-breaking change, so we're now bumping to 0.9.0.  Some
  tests have been added to catch this in future.
* Allow image/ prefix on data URIs. (#169, Daniel Sorichetti)


# [0.8.2]

* Fix some lint issues. (#152, Caleb Maclennan)
* Build benchmarks separately to tests. (#154)
* Add support for a config file for CLI use. (#157, with thanks to AJ ONeal.)


# [0.8.1]

* Add escape option to escape raw HTML instead of clobbering it. (#150, Ryan
  Westlund)


# [0.8.0]

* 0.7.1 was a semver-breaking change.  This is now 0.8.0.


# [0.7.1]

* Reduce list item indentation in line with spec. (#135, Casey Rodarmor)
* Split uber-struct ComrakOptions into substructures.
* Refactor HTML formatter escaping. (#140, Donough Liu)
* Don't render <p> inside <dt> tags. (#145)


# [0.7.0]

* Supporting stable and newer again, since dependencies keep breaking for
  1.27.0. (#134)


# [0.6.2]

* Exclude unneeded files from crate. (#120, Igor Gnatenko)
* Bump the twoway dependency. (#121, Igor Gnatenko)


# [0.6.1]

* Add --gfm flag to CLI to enable all GitHub Flavored Markdown extensions and
  options. (#118, James R Miller)


# [0.6.0]

* Add TaskItem variant to NodeValue. (#115, Élisabeth Henry)


# [0.5.1]

* Support building on Rust versions back to 1.27.0. (#114)


# [0.5.0]

* Update API so that footnote reference and definition identifiers match.
  (#110, Élisabeth Henry)
* Update to CommonMark spec 0.29. (#112)


# [0.4.4]

* Add From<NodeValue> impl to AstNode. (#105, Sunjay Varma)


# [0.4.3]

* Add a Default derive and Ast::new to make ASTs programmatically
  constructible. (#101, Sunjay Varma and #102)


# [0.4.2]

* Add a callback to fill in broken reference links, per pulldown_cmark's
  Parser::new_with_broken_link_callback. (#100, Sunjay Varma)
* Update to latest spec.  (#99)


# [0.4.1]

* Fix a bug in anchor generation; it should now be on par with GitHub's. (#97, Clifford T. Matthews)
* Expose anchor generation for use in library consumers. (#94, Clifford T. Matthews)


# [0.4.0]

* Invert default-false `safe` flag to default-false `unsafe_` flag.  If you
  were not enabling safe mode before, you'll need to enable unsafe mode now.


# [0.3.1]

* Keep up-to-date with the spec.


# [0.3.0]

* Significant test coverage and code clean up. (#82, #83, Brian Anderson)
* Description list support. (#86, Ayose Cazorla)
* Example use of comrak to convert CommonMark documents into S-expressions. (#86, Ayose Cazorla)
* Footnotes are now enabled via an extension option, not a flag of its own. (#87)
* Extend `cmark-gfm` compatibility to include all extension and regression tests. (#87)


# [0.2.14]

* Speed enhancements. (#76, Brian Anderson)
* Target latest spec; bring comrak closer into line with cmark. (#81, Brian Anderson and Ashe Connor)


# [0.2.13]

* Speed enhancements. (#75, Shaquille Johnson)


# [0.2.12]

* Add safety options per the reference C implementation. (#67)


# [0.2.11]

* Expose Arena type so users don't need to bring it in themselves (#66, Vincent
  Prouillet).


# [0.2.10]

* Bring up to date with latest spec.
* Fix parsing of tables nested in other block elements (#61, Brian Anderson).
* Protect against stack smashing in inline processors and CommonMark and HTML
  formatters (#63, Brian Anderson).


# [0.2.9]

* Fix a corner case in the ATX header parser (#53, Brian Anderson).
* Fix grammar for scanning table marker rows (#55, Brian Anderson).
* Add smart punctuation (#57).


# [0.2.8]

* Add `default-info-string` argument/option to specify a default language in fenced
  code blocks. (Thanks to @steveklabnik for the suggestion.)


# [0.2.7]

* Use [`pest`](https://github.com/pest-parser/pest) instead of regexes for lexing.


# [0.2.6]

* Fixed a bug where back-to-back emphases would not be processed correctly.
  (#45; thanks to @SSJohns for the report.)


# [0.2.5]

* Fixed a bug where an exclamation mark "!" followed by a footnote would be
  eaten by the parser.


# [0.2.4]

* Added footnotes support.


# [0.2.3]

* Added header IDs extension.


# [0.2.2]

* Fix for pathological reference link parsing.


# [0.2.1]

* Speed optimisations.


# [0.2.0]

* The formatters no longer produce Strings themselves; you must specify an
  output stream.
* Speed up whitespace normalisation.


# [0.1.9]

* Multibyte character fix for autolink (#35, Shaquille Johnson).
* Resolve panics with tables in awkward situations (#36).


# [0.1.8]

* Fix possible DoS in link parsing (#33, Demi Obenour).
