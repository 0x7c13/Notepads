// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

//! Configuration for the parser and renderer.  Extensions affect both.

#[cfg(feature = "bon")]
use bon::Builder;
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::panic::RefUnwindSafe;
use std::str;
use std::sync::Arc;

use crate::adapters::{CodefenceRendererAdapter, HeadingAdapter, SyntaxHighlighterAdapter};
use crate::parser::ResolvedReference;

#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
/// Umbrella options struct.
pub struct Options<'c> {
    /// Enable CommonMark extensions.
    pub extension: Extension<'c>,

    /// Configure parse-time options.
    pub parse: Parse<'c>,

    /// Configure render-time options.
    pub render: Render,
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "bon", derive(Builder))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
/// Options to select extensions.
pub struct Extension<'c> {
    /// Enables the
    /// [strikethrough extension](https://github.github.com/gfm/#strikethrough-extension-)
    /// from the GFM spec.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.strikethrough = true;
    /// assert_eq!(markdown_to_html("Hello ~world~ there.\n", &options),
    ///            "<p>Hello <del>world</del> there.</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub strikethrough: bool,

    /// Enables the [table extension](https://github.github.com/gfm/#tables-extension-)
    /// from the GFM spec.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.table = true;
    /// assert_eq!(markdown_to_html("| a | b |\n|---|---|\n| c | d |\n", &options),
    ///            "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n\
    ///             <tbody>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody>\n</table>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub table: bool,

    /// Enables the [autolink extension](https://github.github.com/gfm/#autolinks-extension-)
    /// from the GFM spec.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.autolink = true;
    /// assert_eq!(markdown_to_html("Hello www.github.com.\n", &options),
    ///            "<p>Hello <a href=\"http://www.github.com\">www.github.com</a>.</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub autolink: bool,

    /// Enables the
    /// [task list items extension](https://github.github.com/gfm/#task-list-items-extension-)
    /// from the GFM spec.
    ///
    /// Note that the spec does not define the precise output, so only the bare essentials are
    /// rendered.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.tasklist = true;
    /// options.render.r#unsafe = true;
    /// assert_eq!(markdown_to_html("* [x] Done\n* [ ] Not done\n", &options),
    ///            "<ul>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> Done</li>\n\
    ///            <li><input type=\"checkbox\" disabled=\"\" /> Not done</li>\n</ul>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub tasklist: bool,

    /// Enables the superscript Comrak extension.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.superscript = true;
    /// assert_eq!(markdown_to_html("e = mc^2^.\n", &options),
    ///            "<p>e = mc<sup>2</sup>.</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub superscript: bool,

    /// Enables the header IDs Comrak extension, with the given ID prefix.
    ///
    /// When set, each heading gains an anchor element with an `id` attribute
    /// formed by prefixing the slugified heading text. This is useful for
    /// namespacing heading anchors to avoid collisions when rendered Markdown
    /// is embedded alongside other content on a page (e.g. GitHub uses the
    /// prefix `"user-content-"` for this purpose).
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.header_id_prefix = Some("user-content-".to_string());
    /// assert_eq!(markdown_to_html("# README\n", &options),
    ///            "<h1 id=\"user-content-readme\">README<a href=\"#readme\" aria-label=\"Link to heading 'README'\" data-heading-content=\"README\" class=\"anchor\"></a></h1>\n");
    /// ```
    pub header_id_prefix: Option<String>,

    /// When enabled alongside [`header_id_prefix`](#structfield.header_id_prefix), the header ID
    /// prefix is also applied to the `href` anchor in the generated link.
    ///
    /// Has no effect if `header_id_prefix` is `None`.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.header_id_prefix = Some("user-content-".to_string());
    /// options.extension.header_id_prefix_in_href = true;
    /// assert_eq!(markdown_to_html("# README\n", &options),
    ///            "<h1 id=\"user-content-readme\">README<a href=\"#user-content-readme\" aria-label=\"Link to heading 'README'\" data-heading-content=\"README\" class=\"anchor\"></a></h1>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub header_id_prefix_in_href: bool,

    /// Enables the footnotes extension per `cmark-gfm`.
    ///
    /// For usage, see `src/tests.rs`.  The extension is modelled after
    /// [Kramdown](https://kramdown.gettalong.org/syntax.html#footnotes).
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.footnotes = true;
    /// assert_eq!(markdown_to_html("Hi[^x].\n\n[^x]: A greeting.\n", &options),
    ///            "<p>Hi<sup class=\"footnote-ref\"><a href=\"#fn-x\" id=\"fnref-x\" data-footnote-ref>1</a></sup>.</p>\n<section class=\"footnotes\" data-footnotes>\n<ol>\n<li id=\"fn-x\">\n<p>A greeting. <a href=\"#fnref-x\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n</li>\n</ol>\n</section>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub footnotes: bool,

    /// Enables the inline footnotes extension.
    ///
    /// Allows inline footnote syntax `^[content]` where the content can include
    /// inline markup. Inline footnotes are automatically converted to regular
    /// footnotes with auto-generated names and share the same numbering sequence.
    ///
    /// Requires `footnotes` to be enabled as well.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.footnotes = true;
    /// options.extension.inline_footnotes = true;
    /// assert_eq!(markdown_to_html("Hi^[An inline note].\n", &options),
    ///            "<p>Hi<sup class=\"footnote-ref\"><a href=\"#fn-__inline_1\" id=\"fnref-__inline_1\" data-footnote-ref>1</a></sup>.</p>\n<section class=\"footnotes\" data-footnotes>\n<ol>\n<li id=\"fn-__inline_1\">\n<p>An inline note <a href=\"#fnref-__inline_1\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n</li>\n</ol>\n</section>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub inline_footnotes: bool,

    /// Enables the description lists extension.
    ///
    /// Each term must be defined in one paragraph, followed by a blank line,
    /// and then by the details.  Details begins with a colon.
    ///
    /// Not (yet) compatible with render.sourcepos.
    ///
    /// ```markdown
    /// First term
    ///
    /// : Details for the **first term**
    ///
    /// Second term
    ///
    /// : Details for the **second term**
    ///
    ///     More details in second paragraph.
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.description_lists = true;
    /// assert_eq!(markdown_to_html("Term\n\n: Definition", &options),
    ///            "<dl>\n<dt>Term</dt>\n<dd>\n<p>Definition</p>\n</dd>\n</dl>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub description_lists: bool,

    /// Enables the front matter extension.
    ///
    /// Front matter, which begins with the delimiter string at the beginning of the file and ends
    /// at the end of the next line that contains only the delimiter, is passed through unchanged
    /// in markdown output and omitted from HTML output.
    ///
    /// ```markdown
    /// ---
    /// layout: post
    /// title: Formatting Markdown with Comrak
    /// ---
    ///
    /// # Shorter Title
    ///
    /// etc.
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.front_matter_delimiter = Some("---".to_owned());
    /// assert_eq!(
    ///     markdown_to_html("---\nlayout: post\n---\nText\n", &options),
    ///     markdown_to_html("Text\n", &Options::default()));
    /// ```
    ///
    /// ```rust
    /// # use comrak::{format_commonmark, Arena, Options};
    /// use comrak::parse_document;
    /// let mut options = Options::default();
    /// options.extension.front_matter_delimiter = Some("---".to_owned());
    /// let arena = Arena::new();
    /// let input = "---\nlayout: post\n---\nText\n";
    /// let root = parse_document(&arena, input, &options);
    /// let mut buf = String::new();
    /// format_commonmark(&root, &options, &mut buf);
    /// assert_eq!(buf, input);
    /// ```
    pub front_matter_delimiter: Option<String>,

    /// Enables the multiline block quote extension.
    ///
    /// Place `>>>` before and after text to make it into
    /// a block quote.
    ///
    /// ```markdown
    /// Paragraph one
    ///
    /// >>>
    /// Paragraph two
    ///
    /// - one
    /// - two
    /// >>>
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.multiline_block_quotes = true;
    /// assert_eq!(markdown_to_html(">>>\nparagraph\n>>>", &options),
    ///            "<blockquote>\n<p>paragraph</p>\n</blockquote>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub multiline_block_quotes: bool,

    /// Enables GitHub style alerts
    ///
    /// ```md
    /// > [!note]
    /// > Something of note
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.alerts = true;
    /// assert_eq!(markdown_to_html("> [!note]\n> Something of note", &options),
    ///            "<div class=\"markdown-alert markdown-alert-note\">\n<p class=\"markdown-alert-title\">Note</p>\n<p>Something of note</p>\n</div>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub alerts: bool,

    /// Admit a source-backed trailing `{#identifier}` on the native heading path.
    #[cfg_attr(feature = "bon", builder(default))]
    pub native_heading_ids: bool,

    /// Admit standalone table-of-contents directives in the native grammar.
    #[cfg_attr(feature = "bon", builder(default))]
    pub native_table_of_contents: bool,

    /// Admit local target-first wiki links in the native grammar.
    #[cfg_attr(feature = "bon", builder(default))]
    pub native_wiki_links: bool,

    /// Admit local Markdown document transclusions in the native grammar.
    #[cfg_attr(feature = "bon", builder(default))]
    pub native_note_embeds: bool,

    /// Enables math using dollar syntax.
    ///
    /// ```markdown
    /// Inline math $1 + 2$ and display math $$x + y$$
    ///
    /// $$
    /// x^2
    /// $$
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.math_dollars = true;
    /// assert_eq!(markdown_to_html("$1 + 2$ and $$x = y$$", &options),
    ///            "<p><span data-math-style=\"inline\">1 + 2</span> and <span data-math-style=\"display\">x = y</span></p>\n");
    /// assert_eq!(markdown_to_html("$$\nx^2\n$$\n", &options),
    ///            "<p><span data-math-style=\"display\">\nx^2\n</span></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub math_dollars: bool,

    /// Enables math using LaTeX-style delimiters.
    ///
    /// ```markdown
    /// Inline math \(1 + 2\) and display math \[x + y\]
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.math_latex = true;
    /// assert_eq!(markdown_to_html("\\(1 + 2\\) and \\[x = y\\]", &options),
    ///            "<p><span data-math-style=\"inline\">1 + 2</span> and <span data-math-style=\"display\">x = y</span></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub math_latex: bool,

    /// Enables math using code syntax.
    ///
    /// ````markdown
    /// Inline math $`1 + 2`$
    ///
    /// ```math
    /// x^2
    /// ```
    /// ````
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.math_code = true;
    /// assert_eq!(markdown_to_html("$`1 + 2`$", &options),
    ///            "<p><code data-math-style=\"inline\">1 + 2</code></p>\n");
    /// assert_eq!(markdown_to_html("```math\nx^2\n```\n", &options),
    ///            "<pre><code class=\"language-math\" data-math-style=\"display\">x^2\n</code></pre>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub math_code: bool,

    #[cfg(feature = "shortcodes")]
    #[cfg_attr(docsrs, doc(cfg(feature = "shortcodes")))]
    /// Phrases wrapped inside of ':' blocks will be replaced with emojis.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// assert_eq!(markdown_to_html("Happy Friday! :smile:", &options),
    ///            "<p>Happy Friday! :smile:</p>\n");
    ///
    /// options.extension.shortcodes = true;
    /// assert_eq!(markdown_to_html("Happy Friday! :smile:", &options),
    ///            "<p>Happy Friday! 😄</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub shortcodes: bool,

    /// Enables wikilinks using title after pipe syntax
    ///
    /// ````markdown
    /// [[url|link label]]
    /// ````
    ///
    /// When both this option and [`wikilinks_title_before_pipe`][0] are enabled, this option takes
    /// precedence.
    ///
    /// [0]: Self::wikilinks_title_before_pipe
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.wikilinks_title_after_pipe = true;
    /// assert_eq!(markdown_to_html("[[url|link label]]", &options),
    ///            "<p><a href=\"url\" data-wikilink=\"true\">link label</a></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub wikilinks_title_after_pipe: bool,

    /// Enables wikilinks using title before pipe syntax
    ///
    /// ````markdown
    /// [[link label|url]]
    /// ````
    /// When both this option and [`wikilinks_title_after_pipe`][0] are enabled,
    /// [`wikilinks_title_after_pipe`][0] takes precedence.
    ///
    /// [0]: Self::wikilinks_title_after_pipe
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.wikilinks_title_before_pipe = true;
    /// assert_eq!(markdown_to_html("[[link label|url]]", &options),
    ///            "<p><a href=\"url\" data-wikilink=\"true\">link label</a></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub wikilinks_title_before_pipe: bool,

    /// Enables underlines using double underscores
    ///
    /// ```md
    /// __underlined text__
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.underline = true;
    ///
    /// assert_eq!(markdown_to_html("__underlined text__", &options),
    ///            "<p><u>underlined text</u></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub underline: bool,

    /// Enables subscript text using single tildes.
    ///
    /// If the strikethrough option is also enabled, this overrides the single
    /// tilde case to output subscript text.
    ///
    /// ```md
    /// H~2~O
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.subscript = true;
    ///
    /// assert_eq!(markdown_to_html("H~2~O", &options),
    ///            "<p>H<sub>2</sub>O</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub subscript: bool,

    /// Enables spoilers using double vertical bars
    ///
    /// ```md
    /// Darth Vader is ||Luke's father||
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.spoiler = true;
    ///
    /// assert_eq!(markdown_to_html("Darth Vader is ||Luke's father||", &options),
    ///            "<p>Darth Vader is <span class=\"spoiler\">Luke's father</span></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub spoiler: bool,

    /// Requires at least one space after a `>` character to generate a blockquote,
    /// and restarts blockquote nesting across unique lines of input
    ///
    /// ```md
    /// >implying implications
    ///
    /// > one
    /// > > two
    /// > three
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.greentext = true;
    ///
    /// assert_eq!(markdown_to_html(">implying implications", &options),
    ///            "<p>&gt;implying implications</p>\n");
    ///
    /// assert_eq!(markdown_to_html("> one\n> > two\n> three", &options),
    ///            concat!(
    ///             "<blockquote>\n",
    ///             "<p>one</p>\n",
    ///             "<blockquote>\n<p>two</p>\n</blockquote>\n",
    ///             "<p>three</p>\n",
    ///             "</blockquote>\n"));
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub greentext: bool,

    /// Wraps embedded image URLs using a function or custom trait object.
    ///
    /// ```rust
    /// # use std::sync::Arc;
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    ///
    /// options.extension.image_url_rewriter = Some(Arc::new(
    ///     |url: &str| format!("https://safe.example.com?url={}", url)
    /// ));
    ///
    /// assert_eq!(markdown_to_html("![](http://unsafe.example.com/bad.png)", &options),
    ///            "<p><img src=\"https://safe.example.com?url=http://unsafe.example.com/bad.png\" alt=\"\" /></p>\n");
    /// ```
    #[cfg_attr(feature = "arbitrary", arbitrary(value = None))]
    pub image_url_rewriter: Option<Arc<dyn URLRewriter + 'c>>,

    /// Wraps link URLs using a function or custom trait object.
    ///
    /// ```rust
    /// # use std::sync::Arc;
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    ///
    /// options.extension.link_url_rewriter = Some(Arc::new(
    ///     |url: &str| format!("https://safe.example.com/norefer?url={}", url)
    /// ));
    ///
    /// assert_eq!(markdown_to_html("[my link](http://unsafe.example.com/bad)", &options),
    ///            "<p><a href=\"https://safe.example.com/norefer?url=http://unsafe.example.com/bad\">my link</a></p>\n");
    /// ```
    #[cfg_attr(feature = "arbitrary", arbitrary(value = None))]
    pub link_url_rewriter: Option<Arc<dyn URLRewriter + 'c>>,

    /// Recognizes many emphasis that appear in CJK contexts but are not recognized by plain CommonMark.
    ///
    /// ```md
    /// **この文は重要です。**但这句话并不重要。
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.cjk_friendly_emphasis = true;
    ///
    /// assert_eq!(markdown_to_html("**この文は重要です。**但这句话并不重要。", &options),
    ///            "<p><strong>この文は重要です。</strong>但这句话并不重要。</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub cjk_friendly_emphasis: bool,

    /// Enables block scoped subscript that acts similar to a header.
    ///
    /// ```md
    /// -# subtext
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.subtext = true;
    ///
    /// assert_eq!(markdown_to_html("-# subtext", &options),
    ///           "<p><sub>subtext</sub></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub subtext: bool,

    /// Enables highlighting (mark) using `==`.
    ///
    /// ```md
    /// Hey, ==this is important!==
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.highlight = true;
    ///
    /// assert_eq!(markdown_to_html("Hey, ==this is important!==", &options),
    ///           "<p>Hey, <mark>this is important!</mark></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub highlight: bool,

    /// Enables inserted text using `++`.
    ///
    /// ```md
    /// This is ++added text++
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.insert = true;
    ///
    /// assert_eq!(markdown_to_html("This is ++added text++", &options),
    ///           "<p>This is <ins>added text</ins></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub insert: bool,

    #[cfg(feature = "phoenix_heex")]
    #[cfg_attr(docsrs, doc(cfg(feature = "phoenix_heex")))]
    /// Enables Phoenix HEEx template syntax support.
    ///
    /// Recognizes Phoenix HEEx directives, tags, and inline expressions.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.phoenix_heex = true;
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub phoenix_heex: bool,

    /// Enables the container block directive extension.
    ///
    /// Container block directives are container blocks that start and end with `:::`.
    /// The info string after the opening `:::` is used as the block type.
    ///
    /// ```md
    /// :::warning
    /// A paragraph.
    ///
    /// - item one
    /// - item two
    /// :::
    /// ```
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.block_directive = true;
    ///
    /// assert_eq!(markdown_to_html(":::warning\nparagraph\n:::", &options),
    ///            "<div class=\"warning\">\n<p>paragraph</p>\n</div>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub block_directive: bool,

    /// Parse attributes in setext and ATX headers.
    /// ```rust
    /// # use comrak::{parse_document, Arena, Options, nodes::NodeValue};
    /// let mut options = Options::default();
    /// options.extension.header_attributes = true;
    /// let arena = Arena::new();
    /// let input = "## Catgirl {author=\"City Girl\"}\n";
    /// let root = parse_document(&arena, input, &options);
    /// for node in root.descendants() {
    ///   let ast = node.data();
    ///   if let NodeValue::Heading(_) = ast.value {
    ///     assert_eq!(ast.attrs.as_ref().unwrap().pairs,
    ///                &[("author".to_string(), "City Girl".to_string())]);
    ///   }
    /// }
    /// ```
    #[cfg(feature = "attributes")]
    #[cfg_attr(feature = "bon", builder(default))]
    pub header_attributes: bool,

    /// Parse attributes in fenced code blocks' info strings.
    /// ```rust
    /// # use comrak::{parse_document, Arena, Options, nodes::NodeValue};
    /// let mut options = Options::default();
    /// options.extension.fenced_code_attributes = true;
    /// let arena = Arena::new();
    /// let input = "```german {#beispel}\nÄhm... egal.\n```\n";
    /// let root = parse_document(&arena, input, &options);
    /// for node in root.descendants() {
    ///   let ast = node.data();
    ///   if let NodeValue::CodeBlock(_) = ast.value {
    ///     assert_eq!(ast.attrs.as_ref().unwrap().id,
    ///                Some("beispel".to_string()));
    ///   }
    /// }
    /// ```
    #[cfg(feature = "attributes")]
    #[cfg_attr(feature = "bon", builder(default))]
    pub fenced_code_attributes: bool,

    /// Parse attributes immediately following inline code spans.
    /// ```rust
    /// # use comrak::{parse_document, Arena, Options, nodes::NodeValue};
    /// let mut options = Options::default();
    /// options.extension.inline_code_attributes = true;
    /// let arena = Arena::new();
    /// let input = "More inline spans should be `syntax-highlighted`{.common-lisp}.";
    /// let root = parse_document(&arena, input, &options);
    /// for node in root.descendants() {
    ///   let ast = node.data();
    ///   if let NodeValue::Code(_) = ast.value {
    ///     assert_eq!(ast.attrs.as_ref().unwrap().classes,
    ///                &["common-lisp".to_string()]);
    ///   }
    /// }
    /// ```
    #[cfg(feature = "attributes")]
    #[cfg_attr(feature = "bon", builder(default))]
    pub inline_code_attributes: bool,

    /// Parse attributes immediately following links and images.
    /// ```rust
    /// # use comrak::{parse_document, Arena, Options, nodes::NodeValue};
    /// let mut options = Options::default();
    /// options.extension.link_attributes = true;
    /// let arena = Arena::new();
    /// let input = "For instance:\n\n![A photo of an open-hearth furnace](zaporizhstal.jfif){data-date=2012-04-03}\n";
    /// let root = parse_document(&arena, input, &options);
    /// for node in root.descendants() {
    ///   let ast = node.data();
    ///   if let NodeValue::Image(_) = ast.value {
    ///     assert_eq!(ast.attrs.as_ref().unwrap().pairs,
    ///                &[("data-date".to_string(), "2012-04-03".to_string())]);
    ///   }
    /// }
    /// ```
    #[cfg(feature = "attributes")]
    #[cfg_attr(feature = "bon", builder(default))]
    pub link_attributes: bool,
}

impl Extension<'_> {
    pub(crate) fn wikilinks(&self) -> Option<WikiLinksMode> {
        match (
            self.wikilinks_title_before_pipe,
            self.wikilinks_title_after_pipe,
        ) {
            (false, false) => None,
            (true, false) => Some(WikiLinksMode::TitleFirst),
            (_, _) => Some(WikiLinksMode::UrlFirst),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
/// Selects between wikilinks with the title first or the URL first.
pub enum WikiLinksMode {
    /// Indicates that the URL precedes the title. For example: `[[http://example.com|link
    /// title]]`.
    UrlFirst,

    /// Indicates that the title precedes the URL. For example: `[[link title|http://example.com]]`.
    TitleFirst,
}

/// Trait for link and image URL rewrite extensions.
pub trait URLRewriter: RefUnwindSafe + Send + Sync {
    /// Converts the given URL from Markdown to its representation when output as HTML.
    fn to_html(&self, url: &str) -> String;
}

impl Debug for dyn URLRewriter + '_ {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        formatter.write_str("<dyn URLRewriter>")
    }
}

impl<F> URLRewriter for F
where
    F: for<'a> Fn(&'a str) -> String,
    F: RefUnwindSafe + Send + Sync,
{
    fn to_html(&self, url: &str) -> String {
        self(url)
    }
}

#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "bon", derive(Builder))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
/// Options for parser functions.
pub struct Parse<'c> {
    /// Punctuation (quotes, full-stops and hyphens) are converted into 'smart' punctuation.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// assert_eq!(markdown_to_html("'Hello,' \"world\" ...", &options),
    ///            "<p>'Hello,' &quot;world&quot; ...</p>\n");
    ///
    /// options.parse.smart = true;
    /// assert_eq!(markdown_to_html("'Hello,' \"world\" ...", &options),
    ///            "<p>‘Hello,’ “world” …</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub smart: bool,

    /// The default info string for fenced code blocks.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// assert_eq!(markdown_to_html("```\nfn hello();\n```\n", &options),
    ///            "<pre><code>fn hello();\n</code></pre>\n");
    ///
    /// options.parse.default_info_string = Some("rust".into());
    /// assert_eq!(markdown_to_html("```\nfn hello();\n```\n", &options),
    ///            "<pre><code class=\"language-rust\">fn hello();\n</code></pre>\n");
    /// ```
    pub default_info_string: Option<String>,

    /// Whether or not a simple `x` or `X` is used for tasklist or any other symbol is allowed.
    #[cfg_attr(feature = "bon", builder(default))]
    pub relaxed_tasklist_matching: bool,

    /// Whether tasklist items can be parsed in table cells. At present, the
    /// tasklist item must be the only content in the cell. Both tables and
    /// tasklists much be enabled for this to work.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.table = true;
    /// options.extension.tasklist = true;
    /// assert_eq!(markdown_to_html("| val |\n| - |\n| [ ] |\n", &options),
    ///            "<table>\n<thead>\n<tr>\n<th>val</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>[ ]</td>\n</tr>\n</tbody>\n</table>\n");
    ///
    /// options.parse.tasklist_in_table = true;
    /// assert_eq!(markdown_to_html("| val |\n| - |\n| [ ] |\n", &options),
    ///            "<table>\n<thead>\n<tr>\n<th>val</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>\n<input type=\"checkbox\" disabled=\"\" /> </td>\n</tr>\n</tbody>\n</table>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub tasklist_in_table: bool,

    /// Relax parsing of autolinks, allow links to be detected inside brackets
    /// and allow all url schemes. It is intended to allow a very specific type of autolink
    /// detection, such as `[this http://and.com that]` or `{http://foo.com}`, on a best can basis.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.autolink = true;
    /// assert_eq!(markdown_to_html("[https://foo.com]", &options),
    ///            "<p>[https://foo.com]</p>\n");
    ///
    /// options.parse.relaxed_autolinks = true;
    /// assert_eq!(markdown_to_html("[https://foo.com]", &options),
    ///            "<p>[<a href=\"https://foo.com\">https://foo.com</a>]</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub relaxed_autolinks: bool,

    /// Require LaTeX display delimiters to enclose an entire paragraph.
    #[cfg_attr(feature = "bon", builder(default))]
    pub latex_display_standalone: bool,

    /// Ignore setext headings in input.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// let input = "setext heading\n---";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<h2>setext heading</h2>\n");
    ///
    /// options.parse.ignore_setext = true;
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p>setext heading</p>\n<hr />\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub ignore_setext: bool,

    /// In case the parser encounters any potential links that have a broken
    /// reference (e.g `[foo]` when there is no `[foo]: url` entry at the
    /// bottom) the provided callback will be called with the reference name,
    /// both in normalized form and unmodified, and the returned pair will be
    /// used as the link destination and title if not [`None`].
    ///
    /// ```rust
    /// # use std::{str, sync::Arc};
    /// # use comrak::{markdown_to_html, options::BrokenLinkReference, Options, ResolvedReference};
    /// let cb = |link_ref: BrokenLinkReference| match link_ref.normalized {
    ///     "foo" => Some(ResolvedReference::new(
    ///         "https://www.rust-lang.org/".to_string(),
    ///         "The Rust Language".to_string(),
    ///     )),
    ///     _ => None,
    /// };
    ///
    /// let mut options = Options::default();
    /// options.parse.broken_link_callback = Some(Arc::new(cb));
    ///
    /// let output = markdown_to_html(
    ///     "# Cool input!\nWow look at this cool [link][foo]. A [broken link] renders as text.",
    ///     &options,
    /// );
    ///
    /// assert_eq!(output,
    ///            "<h1>Cool input!</h1>\n<p>Wow look at this cool \
    ///            <a href=\"https://www.rust-lang.org/\" title=\"The Rust Language\">link</a>. \
    ///            A [broken link] renders as text.</p>\n");
    /// ```
    #[cfg_attr(feature = "arbitrary", arbitrary(default))]
    pub broken_link_callback: Option<Arc<dyn BrokenLinkCallback + 'c>>,

    /// Leave footnote definitions in place in the document tree, rather than
    /// reordering them to the end.  This will also cause unreferenced footnote
    /// definitions to remain in the tree, rather than being removed.
    ///
    /// Comrak's default formatters expect this option to be turned off, so use
    /// with care if you use the default formatters.
    ///
    /// ```rust
    /// # use comrak::{Arena, parse_document, Node, Options};
    /// let mut options = Options::default();
    /// options.extension.footnotes = true;
    /// let arena = Arena::new();
    /// let input = concat!(
    ///   "Remember burning a CD?[^cd]\n",
    ///   "\n",
    ///   "[^cd]: In the Old Days, a 4x burner was considered good.\n",
    ///   "\n",
    ///   "[^dvd]: And DVD-RWs? Those were something else.\n",
    ///   "\n",
    ///   "Me neither.",
    /// );
    ///
    /// fn node_kinds<'a>(doc: Node<'a>) -> Vec<&'static str> {
    ///   doc.descendants().map(|n| n.data().value.xml_node_name()).collect()
    /// }
    ///
    /// let root = parse_document(&arena, input, &options);
    /// assert_eq!(
    ///   node_kinds(root),
    ///   &["document", "paragraph", "text", "footnote_reference", "paragraph", "text",
    ///     "footnote_definition", "paragraph", "text"],
    /// );
    ///
    /// options.parse.leave_footnote_definitions = true;
    ///
    /// let root = parse_document(&arena, input, &options);
    /// assert_eq!(
    ///   node_kinds(root),
    ///   &["document", "paragraph", "text", "footnote_reference", "footnote_definition",
    ///     "paragraph", "text", "footnote_definition", "paragraph", "text", "paragraph", "text"],
    /// );
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub leave_footnote_definitions: bool,

    /// Leave escaped characters in an `Escaped` node in the document tree.
    ///
    /// ```rust
    /// # use comrak::{Arena, parse_document, Node, Options};
    /// let mut options = Options::default();
    /// let arena = Arena::new();
    /// let input = "Notify user \\@example";
    ///
    /// fn node_kinds<'a>(doc: Node<'a>) -> Vec<&'static str> {
    ///   doc.descendants().map(|n| n.data().value.xml_node_name()).collect()
    /// }
    ///
    /// let root = parse_document(&arena, input, &options);
    /// assert_eq!(
    ///   node_kinds(root),
    ///   &["document", "paragraph", "text"],
    /// );
    ///
    /// options.parse.escaped_char_spans = true;
    /// let root = parse_document(&arena, input, &options);
    /// assert_eq!(
    ///   node_kinds(root),
    ///   &["document", "paragraph", "text", "escaped", "text", "text"],
    /// );
    /// ```
    ///
    /// Note that enabling the `escaped_char_spans` render option will cause
    /// this option to be enabled.
    #[cfg_attr(feature = "bon", builder(default))]
    pub escaped_char_spans: bool,

    /// When enabled, the [`column`][crate::nodes::LineColumn::column] values in
    /// [`Sourcepos`][crate::nodes::Sourcepos] are counted as Unicode characters
    /// (i.e. `char`s) rather than as UTF-8 bytes.
    ///
    /// By default, column values follow cmark behaviour: each byte of a
    /// multi-byte UTF-8 character counts as a separate column. Enabling this
    /// option converts those byte-based columns to character-based columns after
    /// parsing, so that a 3-byte character such as `好` occupies only one
    /// column position instead of three.
    ///
    /// ```rust
    /// # use comrak::{Arena, parse_document, Options};
    /// let arena = Arena::new();
    /// let mut options = Options::default();
    ///
    /// // Default (byte-based): "好" spans columns 1-3
    /// let root = parse_document(&arena, "好", &options);
    /// let sp = root.first_child().unwrap().data().sourcepos;
    /// assert_eq!(sp.start.column, 1);
    /// assert_eq!(sp.end.column, 3);
    ///
    /// // Char-based: "好" occupies only column 1
    /// options.parse.sourcepos_chars = true;
    /// let root = parse_document(&arena, "好", &options);
    /// let sp = root.first_child().unwrap().data().sourcepos;
    /// assert_eq!(sp.start.column, 1);
    /// assert_eq!(sp.end.column, 1);
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub sourcepos_chars: bool,
}

/// The type of the callback used when a reference link is encountered with no
/// matching reference.
///
/// The details of the broken reference are passed in the
/// [`BrokenLinkReference`] argument. If a [`ResolvedReference`] is returned, it
/// is used as the link; otherwise, no link is made and the reference text is
/// preserved in its entirety.
pub trait BrokenLinkCallback: RefUnwindSafe + Send + Sync {
    /// Potentially resolve a single broken link reference.
    fn resolve(&self, broken_link_reference: BrokenLinkReference) -> Option<ResolvedReference>;
}

impl Debug for dyn BrokenLinkCallback + '_ {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        formatter.write_str("<dyn BrokenLinkCallback>")
    }
}

impl<F> BrokenLinkCallback for F
where
    F: Fn(BrokenLinkReference) -> Option<ResolvedReference>,
    F: RefUnwindSafe + Send + Sync,
{
    fn resolve(&self, broken_link_reference: BrokenLinkReference) -> Option<ResolvedReference> {
        self(broken_link_reference)
    }
}

/// Struct to the broken link callback, containing details on the link reference
/// which failed to find a match.
#[derive(Debug)]
pub struct BrokenLinkReference<'l> {
    /// The normalized reference link label. Unicode case folding is applied;
    /// see <https://github.com/commonmark/commonmark-spec/issues/695> for a
    /// discussion on the details of what this exactly means.
    pub normalized: &'l str,

    /// The original text in the link label.
    pub original: &'l str,
}

#[derive(Default, Debug, Clone, Copy)]
#[cfg_attr(feature = "bon", derive(Builder))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
/// Options for formatter functions.
pub struct Render {
    /// [Soft line breaks](http://spec.commonmark.org/0.27/#soft-line-breaks) in the input
    /// translate into hard line breaks in the output.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// assert_eq!(markdown_to_html("Hello.\nWorld.\n", &options),
    ///            "<p>Hello.\nWorld.</p>\n");
    ///
    /// options.render.hardbreaks = true;
    /// assert_eq!(markdown_to_html("Hello.\nWorld.\n", &options),
    ///            "<p>Hello.<br />\nWorld.</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub hardbreaks: bool,

    /// GitHub-style `<pre lang="xyz">` is used for fenced code blocks with info tags.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// assert_eq!(markdown_to_html("``` rust\nfn hello();\n```\n", &options),
    ///            "<pre><code class=\"language-rust\">fn hello();\n</code></pre>\n");
    ///
    /// options.render.github_pre_lang = true;
    /// assert_eq!(markdown_to_html("``` rust\nfn hello();\n```\n", &options),
    ///            "<pre lang=\"rust\"><code>fn hello();\n</code></pre>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub github_pre_lang: bool,

    /// Enable full info strings for code blocks
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// assert_eq!(markdown_to_html("``` rust extra info\nfn hello();\n```\n", &options),
    ///            "<pre><code class=\"language-rust\">fn hello();\n</code></pre>\n");
    ///
    /// options.render.full_info_string = true;
    /// let html = markdown_to_html("``` rust extra info\nfn hello();\n```\n", &options);
    /// eprintln!("{}", html);
    /// assert!(html.contains(r#"data-meta="extra info""#));
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub full_info_string: bool,

    /// The wrap column when outputting CommonMark.
    ///
    /// ```rust
    /// # use comrak::{Arena, parse_document, Options, format_commonmark};
    /// # fn main() {
    /// # let arena = Arena::new();
    /// let mut options = Options::default();
    /// let node = parse_document(&arena, "hello hello hello hello hello hello", &options);
    /// let mut output = String::new();
    /// format_commonmark(node, &options, &mut output).unwrap();
    /// assert_eq!(output,
    ///            "hello hello hello hello hello hello\n");
    ///
    /// options.render.width = 20;
    /// let mut output = String::new();
    /// format_commonmark(node, &options, &mut output).unwrap();
    /// assert_eq!(output,
    ///            "hello hello hello\nhello hello hello\n");
    /// # }
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub width: usize,

    /// Allow rendering of raw HTML and potentially dangerous links.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// let input = "<script>\nalert('xyz');\n</script>\n\n\
    ///              Possibly <marquee>annoying</marquee>.\n\n\
    ///              [Dangerous](javascript:alert(document.cookie)).\n\n\
    ///              [Safe](http://commonmark.org).\n";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<!-- raw HTML omitted -->\n\
    ///             <p>Possibly <!-- raw HTML omitted -->annoying<!-- raw HTML omitted -->.</p>\n\
    ///             <p><a href=\"\">Dangerous</a>.</p>\n\
    ///             <p><a href=\"http://commonmark.org\">Safe</a>.</p>\n");
    ///
    /// options.render.r#unsafe = true;
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<script>\nalert(\'xyz\');\n</script>\n\
    ///             <p>Possibly <marquee>annoying</marquee>.</p>\n\
    ///             <p><a href=\"javascript:alert(document.cookie)\">Dangerous</a>.</p>\n\
    ///             <p><a href=\"http://commonmark.org\">Safe</a>.</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub r#unsafe: bool,

    /// Escape raw HTML instead of clobbering it.
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// let input = "<i>italic text</i>";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p><!-- raw HTML omitted -->italic text<!-- raw HTML omitted --></p>\n");
    ///
    /// options.render.escape = true;
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p>&lt;i&gt;italic text&lt;/i&gt;</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub escape: bool,

    /// Set the type of [bullet list marker](https://spec.commonmark.org/0.30/#bullet-list-marker) to use. Options are:
    ///
    /// * [`ListStyleType::Dash`] to use `-` (default)
    /// * [`ListStyleType::Plus`] to use `+`
    /// * [`ListStyleType::Star`] to use `*`
    ///
    /// ```rust
    /// # use comrak::{markdown_to_commonmark, Options, options::ListStyleType};
    /// let mut options = Options::default();
    /// let input = "- one\n- two\n- three";
    /// assert_eq!(markdown_to_commonmark(input, &options),
    ///            "- one\n- two\n- three\n"); // default is Dash
    ///
    /// options.render.list_style = ListStyleType::Plus;
    /// assert_eq!(markdown_to_commonmark(input, &options),
    ///            "+ one\n+ two\n+ three\n");
    ///
    /// options.render.list_style = ListStyleType::Star;
    /// assert_eq!(markdown_to_commonmark(input, &options),
    ///            "* one\n* two\n* three\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub list_style: ListStyleType,

    /// Include source position attributes in HTML and XML output.
    ///
    /// Sourcepos information is reliable for core block items excluding
    /// lists and list items, all inlines, and most extensions.
    /// The description lists extension still has issues; see
    /// <https://github.com/kivikakk/comrak/blob/3bb6d4ce/src/tests/description_lists.rs#L60-L125>.
    ///
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.render.sourcepos = true;
    /// let input = "Hello *world*!";
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p data-sourcepos=\"1:1-1:14\">Hello <em data-sourcepos=\"1:7-1:13\">world</em>!</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub sourcepos: bool,

    /// Wrap escaped characters in a `<span>` to allow any
    /// post-processing to recognize them.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// let input = "Notify user \\@example";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p>Notify user @example</p>\n");
    ///
    /// options.render.escaped_char_spans = true;
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p>Notify user <span data-escaped-char>@</span>example</p>\n");
    /// ```
    ///
    /// Enabling this option will cause the `escaped_char_spans` parse option to
    /// be enabled.
    #[cfg_attr(feature = "bon", builder(default))]
    pub escaped_char_spans: bool,

    /// Ignore empty links in input.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// let input = "[]()";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p><a href=\"\"></a></p>\n");
    ///
    /// options.render.ignore_empty_links = true;
    /// assert_eq!(markdown_to_html(input, &options), "<p>[]()</p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub ignore_empty_links: bool,

    /// Enables GFM quirks in HTML output which break CommonMark compatibility.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// let input = "****abcd**** *_foo_*";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p><strong><strong>abcd</strong></strong> <em><em>foo</em></em></p>\n");
    ///
    /// options.render.gfm_quirks = true;
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p><strong>abcd</strong> <em><em>foo</em></em></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub gfm_quirks: bool,

    /// Prefer fenced code blocks when outputting CommonMark.
    ///
    /// ```rust
    /// # use std::str;
    /// # use comrak::{Arena, Options, format_commonmark, parse_document};
    /// let arena = Arena::new();
    /// let mut options = Options::default();
    /// let input = "```\nhello\n```\n";
    /// let root = parse_document(&arena, input, &options);
    ///
    /// let mut buf = String::new();
    /// format_commonmark(&root, &options, &mut buf);
    /// assert_eq!(buf, "    hello\n");
    ///
    /// buf.clear();
    /// options.render.prefer_fenced = true;
    /// format_commonmark(&root, &options, &mut buf);
    /// assert_eq!(buf, "```\nhello\n```\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub prefer_fenced: bool,

    /// Render the image as a figure element with the title as its caption.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// let input = "![image](https://example.com/image.png \"this is an image\")";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p><img src=\"https://example.com/image.png\" alt=\"image\" title=\"this is an image\" /></p>\n");
    ///
    /// options.render.figure_with_caption = true;
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<p><figure><img src=\"https://example.com/image.png\" alt=\"image\" title=\"this is an image\" /><figcaption>this is an image</figcaption></figure></p>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub figure_with_caption: bool,

    /// Add classes to the output of the tasklist extension. This allows tasklists to be styled.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// options.extension.tasklist = true;
    /// let input = "- [ ] Foo";
    ///
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<ul>\n<li><input type=\"checkbox\" disabled=\"\" /> Foo</li>\n</ul>\n");
    ///
    /// options.render.tasklist_classes = true;
    /// assert_eq!(markdown_to_html(input, &options),
    ///            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> Foo</li>\n</ul>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub tasklist_classes: bool,

    /// How to render alert blocks. Options are:
    ///
    /// * [`AlertStyleType::Specific`] to use `div`s with `markdown-` prefixed classes (default)
    /// * [`AlertStyleType::Semantic`] to use `aside`s with an `admonition` class
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options, options::AlertStyleType};
    /// let mut options = Options::default();
    /// options.extension.alerts = true;
    /// options.render.alert_style = AlertStyleType::Semantic;
    /// assert_eq!(markdown_to_html("> [!note]\n> Something of note", &options),
    ///            "<aside class=\"admonition note\">\n<p class=\"admonition-title\">Note</p>\n<p>Something of note</p>\n</aside>\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub alert_style: AlertStyleType,

    /// Render ordered list with a minimum marker width.
    /// Having a width lower than 3 doesn't do anything.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_commonmark, Options};
    /// let mut options = Options::default();
    /// let input = "1. Something";
    ///
    /// assert_eq!(markdown_to_commonmark(input, &options),
    ///            "1. Something\n");
    ///
    /// options.render.ol_width = 5;
    /// assert_eq!(markdown_to_commonmark(input, &options),
    ///            "1.   Something\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub ol_width: usize,

    /// Minimise escapes used in CommonMark output (`-t commonmark`) by removing
    /// each individually and seeing if the resulting document roundtrips.
    /// Brute-force and expensive, but produces nicer output.  Note that the
    /// result may not in fact be minimal.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_commonmark, Options};
    /// let mut options = Options::default();
    /// let input = "__hi";
    ///
    /// assert_eq!(markdown_to_commonmark(input, &options),
    ///            "\\_\\_hi\n");
    ///
    /// options.render.experimental_minimize_commonmark = true;
    /// assert_eq!(markdown_to_commonmark(input, &options),
    ///            "__hi\n");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub experimental_minimize_commonmark: bool,

    /// Suppress pretty-printing newlines between block-level HTML elements.
    ///
    /// Normally comrak puts a `\n` after closing tags like `</p>`, `</li>`,
    /// etc.  With this option on, those newlines are omitted.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options};
    /// let mut options = Options::default();
    /// assert_eq!(markdown_to_html("# Hello\n\nWorld.\n", &options),
    ///            "<h1>Hello</h1>\n<p>World.</p>\n");
    ///
    /// options.render.compact_html = true;
    /// assert_eq!(markdown_to_html("# Hello\n\nWorld.\n", &options),
    ///            "<h1>Hello</h1><p>World.</p>");
    /// ```
    #[cfg_attr(feature = "bon", builder(default))]
    pub compact_html: bool,
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
/// Options for bulleted list rendering in markdown. See [`Render::list_style`] for more details.
pub enum ListStyleType {
    /// The `-` character
    #[default]
    Dash = 45,
    /// The `+` character
    Plus = 43,
    /// The `*` character
    Star = 42,
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
/// Options for alert rendering in markdown. See [`Render::alert_style`] for more details.
pub enum AlertStyleType {
    /// `div`s with `class="markdown-alert markdown-alert-<type>"`
    #[default]
    Specific,
    /// `aside`s with `class="admonition <type>"`, matching `docutils`' output
    Semantic,
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "bon", derive(Builder))]
/// Umbrella plugins struct.
pub struct Plugins<'p> {
    /// Configure render-time plugins.
    #[cfg_attr(feature = "bon", builder(default))]
    pub render: RenderPlugins<'p>,
}

#[derive(Default, Clone)]
#[cfg_attr(feature = "bon", derive(Builder))]
/// Plugins for alternative rendering.
pub struct RenderPlugins<'p> {
    /// Provide language-specific renderers for codefence blocks.
    ///
    /// `math` codefence blocks are handled separately by Comrak's built-in math renderer,
    /// so entries keyed by `"math"` in this map are not used.
    #[cfg_attr(feature = "bon", builder(default))]
    pub codefence_renderers: HashMap<String, &'p dyn CodefenceRendererAdapter>,

    /// Provide a syntax highlighter adapter implementation for syntax
    /// highlighting of codefence blocks.
    ///
    /// ```rust
    /// # use comrak::{markdown_to_html, Options, options::Plugins, markdown_to_html_with_plugins};
    /// # use comrak::adapters::SyntaxHighlighterAdapter;
    /// use std::borrow::Cow;
    /// use std::collections::HashMap;
    /// use std::fmt::{self, Write};
    /// let options = Options::default();
    /// let mut plugins = Plugins::default();
    /// let input = "```rust\nfn main<'a>();\n```";
    ///
    /// assert_eq!(markdown_to_html_with_plugins(input, &options, &plugins),
    ///            "<pre><code class=\"language-rust\">fn main&lt;'a&gt;();\n</code></pre>\n");
    ///
    /// pub struct MockAdapter {}
    /// impl SyntaxHighlighterAdapter for MockAdapter {
    ///     fn write_highlighted(&self, output: &mut dyn fmt::Write, lang: Option<&str>, code: &str) -> fmt::Result {
    ///         write!(output, "<span class=\"lang-{}\">{}</span>", lang.unwrap(), code)
    ///     }
    ///
    ///     fn write_pre_tag<'s>(&self, output: &mut dyn fmt::Write, _attributes: HashMap<&'static str, Cow<'s, str>>) -> fmt::Result {
    ///         output.write_str("<pre lang=\"rust\">")
    ///     }
    ///
    ///     fn write_code_tag<'s>(&self, output: &mut dyn fmt::Write, _attributes: HashMap<&'static str, Cow<'s, str>>) -> fmt::Result {
    ///         output.write_str("<code class=\"language-rust\">")
    ///     }
    /// }
    ///
    /// let adapter = MockAdapter {};
    /// plugins.render.codefence_syntax_highlighter = Some(&adapter);
    ///
    /// assert_eq!(markdown_to_html_with_plugins(input, &options, &plugins),
    ///            "<pre lang=\"rust\"><code class=\"language-rust\"><span class=\"lang-rust\">fn main<'a>();\n</span></code></pre>\n");
    /// ```
    pub codefence_syntax_highlighter: Option<&'p dyn SyntaxHighlighterAdapter>,

    /// Optional heading adapter
    pub heading_adapter: Option<&'p dyn HeadingAdapter>,
}

impl Debug for RenderPlugins<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderPlugins")
            .field(
                "codefence_renderers",
                &"HashMap<String, impl CodefenceRendererAdapter>",
            )
            .field(
                "codefence_syntax_highlighter",
                &"impl SyntaxHighlighterAdapter",
            )
            .finish()
    }
}
