# Comrak

Comrak is a [CommonMark] and [GitHub Flavored Markdown] compatible parser and renderer, written in Rust.

It is [developed on GitHub] by a community of contributors, and released under the [BSD 2-clause license], forever[^no-cla], for use by all.

[CommonMark]: https://commonmark.org/
[GitHub Flavored Markdown]: https://github.github.com/gfm/
[developed on GitHub]: https://github.com/kivikakk/comrak
[BSD 2-clause license]: https://github.com/kivikakk/comrak/blob/main/COPYING

[^no-cla]: Comrak intentionally does not use a [Contributor License Agreement] --- every contributor retains copyright on their contributions.

[Contributor License Agreement]: https://en.wikipedia.org/wiki/Contributor_license_agreement


## Features

* Compatible with [CommonMark 0.31.2] by default.
* One option to toggle on all [GitHub Flavored Markdown] extensions (or enable them separately).
* Many additional [extensions] developed by the community.
* Fine-grained [parse] and [render] options.
* [Pluggable] syntax highlighting for code blocks.
* [Custom formatter] support to override the rendering of any node type.

[CommonMark 0.31.2]: https://spec.commonmark.org/0.31.2/
[extensions]: https://docs.rs/comrak/latest/comrak/options/struct.Extension.html
[parse]: https://docs.rs/comrak/latest/comrak/options/struct.Parse.html
[render]: https://docs.rs/comrak/latest/comrak/options/struct.Render.html
[Pluggable]: https://github.com/kivikakk/comrak#plugins
[Custom formatter]: https://docs.rs/comrak/latest/comrak/macro.create_formatter.html


## Usage

Comrak can be used directly on the command-line or as part of a batch processing pipeline, and is available pre-built on [many platforms].

It can also be used as a library in Rust as a Cargo dependency, or with [bindings to other languages].
The WASM target lets it be used [directly in webpages].

This webpage is [created with Comrak] by stitching together three elements:

* [`header.html`]
* [`index.md`] converted with `--header-id-prefix "" --smart -e footnotes`
* [`footer.html`]

The CLI help for the latest version is available in the [README], and the Rust documentation is on [docs.rs].

[many platforms]: https://github.com/kivikakk/comrak#cli
[bindings to other languages]: #bindings
[directly in webpages]: https://gitlab-org.gitlab.io/ruby/gems/gitlab-glfm-markdown/
[created with Comrak]: Makefile
[`header.html`]: https://github.com/kivikakk/comrak/blob/main/www/header.html
[`index.md`]: https://github.com/kivikakk/comrak/blob/main/www/index.md?plain=1
[`footer.html`]: https://github.com/kivikakk/comrak/blob/main/www/footer.html
[README]: https://github.com/kivikakk/comrak#usage
[docs.rs]: https://docs.rs/comrak/latest/comrak/


## Bindings

* [Commonmarker], [gitlab-glfm-markdown] (Ruby)
* [MDEx] (Elixir)
* [comrak], [comrak-ext] (Python)
* [comrak-wasm] (TypeScript)

[Commonmarker]: https://github.com/gjtorikian/commonmarker
[gitlab-glfm-markdown]: https://gitlab.com/gitlab-org/ruby/gems/gitlab-glfm-markdown
[MDEx]: https://mdelixir.dev
[comrak]: https://github.com/lmmx/comrak
[comrak-ext]: https://github.com/Martin005/comrak-ext
[comrak-wasm]: https://github.com/nberlette/comrak-wasm


## Who uses Comrak?

* [crates.io] and [docs.rs]
* [GitLab]
* [Deno]
* [Reddit]
* [Lockbook]
* [many] [more!]

[crates.io]: https://crates.io/
[docs.rs]: https://docs.rs/
[GitLab]: https://gitlab.com/
[Deno]: https://deno.com/
[Reddit]: https://www.reddit.com/
[Lockbook]: https://lockbook.net/
[many]: https://github.com/kivikakk/comrak/network/dependents
[more!]: https://crates.io/crates/comrak/reverse_dependencies


## Questions? Commits? Cat pictures?

You can contact the maintainers and community [on GitHub], or you can
email the author at [`ashe@kivikakk.ee`](mailto:ashe@kivikakk.ee).
Please report security issues to that email address. 

Contributions are **highly encouraged**, and we would love to help you
through the process.  We practice [Optimistic Merging] as described by
Peter Hintjens.  We have a [Code of Conduct] which forms the basis of how
we treat one another.

If you have a GitHub account, you are welcome to use its interface to
report issues, open pull requests, and [privately report security issues].
If not, you are very welcome to discuss and contribute by email, and a
maintainer will make those actions on your behalf, attributed however you
like.

[on GitHub]: https://github.com/kivikakk/comrak
[Optimistic Merging]: http://hintjens.com/blog:106
[Code of Conduct]: https://github.com/kivikakk/comrak/blob/main/CODE_OF_CONDUCT.md
[privately report security issues]: https://github.com/kivikakk/comrak/security/advisories/new

---



