// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

//! A 100% [CommonMark](http://commonmark.org/) and [GFM](https://github.github.com/gfm/)
//! compatible Markdown parser.
//!
//! Source repository and detailed `README` is at
//! [github.com/kivikakk/comrak](https://github.com/kivikakk/comrak).
//!
//! You can use `comrak::markdown_to_html` directly:
//!
//! ```rust
//! use comrak::{markdown_to_html, Options};
//! assert_eq!(
//!     markdown_to_html("Olá, **世界**!", &Options::default()),
//!     "<p>Olá, <strong>世界</strong>!</p>\n"
//! );
//! ```
//!
//! Or you can parse the input into an AST yourself, manipulate it, and then use your desired
//! formatter:
//!
//! ```rust
//! use comrak::{Arena, parse_document, format_html, Options};
//! use comrak::nodes::NodeValue;
//!
//! # fn main() {
//! let arena = Arena::new();
//!
//! let root = parse_document(
//!     &arena,
//!     "Hello, pretty world!\n\n1. Do you like [pretty](#) paintings?\n2. Or *pretty* music?\n",
//!     &Options::default());
//!
//! for node in root.descendants() {
//!     if let NodeValue::Text(ref mut text) = node.data.borrow_mut().value {
//!         let owned = text.owned_cow_mut().expect("legacy parser produces owned text");
//!         *owned = owned.to_mut().replace("pretty", "beautiful").into();
//!     }
//! }
//!
//! let mut html = String::new();
//! format_html(root, &Options::default(), &mut html).unwrap();
//!
//! assert_eq!(
//!     &html,
//!     "<p>Hello, beautiful world!</p>\n\
//!      <ol>\n\
//!      <li>Do you like <a href=\"#\">beautiful</a> paintings?</li>\n\
//!      <li>Or <em>beautiful</em> music?</li>\n\
//!      </ol>\n");
//! # }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces
)]
#![allow(
    unknown_lints,
    clippy::doc_markdown,
    cyclomatic_complexity,
    clippy::bool_to_int_with_if,
    clippy::too_many_arguments
)]

pub mod adapters;
pub mod arena_tree;
pub mod html;
#[allow(missing_docs)]
pub mod native;
pub mod nodes;
pub mod plugins;

mod character_set;
mod cm;
mod ctype;
mod entity;
mod matchers;
mod parser;
mod scanners;
mod strings;
#[cfg(test)]
mod tests;
mod xml;

pub use cm::escape_inline as escape_commonmark_inline;
pub use cm::escape_link_destination as escape_commonmark_link_destination;
pub use cm::format_document as format_commonmark;
pub use cm::format_document_with_plugins as format_commonmark_with_plugins;
#[doc(inline)]
pub use html::Anchorizer;
pub use html::format_document as format_html;
pub use html::format_document_with_plugins as format_html_with_plugins;
pub use nodes::Node;
pub use parser::options;
pub use parser::{Options, ResolvedReference, parse_document};
pub use xml::format_document as format_xml;
pub use xml::format_document_with_plugins as format_xml_with_plugins;

/// Convenience type alias for arena used to hold nodes.
pub type Arena<'a> = native::FallibleArena<nodes::AstNode<'a>>;

/// Render Markdown to HTML.
///
/// See the documentation of the crate root for an example.
pub fn markdown_to_html(md: &str, options: &Options) -> String {
    markdown_to_html_with_plugins(md, options, &options::Plugins::default())
}

/// Render Markdown to HTML using plugins.
///
/// See the documentation of the crate root for an example.
pub fn markdown_to_html_with_plugins(
    md: &str,
    options: &Options,
    plugins: &options::Plugins,
) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, md, options);
    let mut out = String::new();
    format_html_with_plugins(root, options, &mut out, plugins).unwrap();
    out
}

/// Return the version of the crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Render Markdown back to CommonMark.
pub fn markdown_to_commonmark(md: &str, options: &Options) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, md, options);
    let mut out = String::new();
    format_commonmark(root, options, &mut out).unwrap();
    out
}

/// Render Markdown to CommonMark XML.
///
/// See <https://github.com/commonmark/commonmark-spec/blob/master/CommonMark.dtd>.
pub fn markdown_to_commonmark_xml(md: &str, options: &Options) -> String {
    markdown_to_commonmark_xml_with_plugins(md, options, &options::Plugins::default())
}

/// Render Markdown to CommonMark XML using plugins.
///
/// See <https://github.com/commonmark/commonmark-spec/blob/master/CommonMark.dtd>.
pub fn markdown_to_commonmark_xml_with_plugins(
    md: &str,
    options: &Options,
    plugins: &options::Plugins,
) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, md, options);
    let mut out = String::new();
    format_xml_with_plugins(root, options, &mut out, plugins).unwrap();
    out
}
