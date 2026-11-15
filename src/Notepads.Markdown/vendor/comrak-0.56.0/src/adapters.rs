//! Adapter traits for plugins.
//!
//! Each plugin implements one or more of the traits available in this module.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use crate::nodes::Sourcepos;

/// Implement this adapter for custom rendering of codefence blocks.
pub trait CodefenceRendererAdapter: Send + Sync {
    /// Render a codefence block.
    ///
    /// lang: Name of the programming language (the first token of the info string).
    /// meta: The remaining codefence info string after the language token, trimmed.
    fn write(
        &self,
        output: &mut dyn fmt::Write,
        lang: &str,
        meta: &str,
        code: &str,
        sourcepos: Option<Sourcepos>,
    ) -> fmt::Result;
}

/// Implement this adapter for creating a plugin for custom syntax highlighting of codefence blocks.
pub trait SyntaxHighlighterAdapter: Send + Sync {
    /// Generates a syntax highlighted HTML output.
    ///
    /// lang: Name of the programming language (the info string of the codefence block after the initial "```" part).
    /// code: The source code to be syntax highlighted.
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result;

    /// Generates the opening `<pre>` tag. Some syntax highlighter libraries might include their own
    /// `<pre>` tag possibly with some HTML attribute pre-filled.
    ///
    /// `attributes`: A map of HTML attributes provided by Comrak.
    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result;

    /// Generates the opening `<code>` tag. Some syntax highlighter libraries might include their own
    /// `<code>` tag possibly with some HTML attribute pre-filled.
    ///
    /// `attributes`: A map of HTML attributes provided by Comrak.
    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result;
}

/// The struct passed to the [`HeadingAdapter`] for custom heading implementations.
#[derive(Clone, Debug)]
pub struct HeadingMeta {
    /// The level of the heading; from 1 to 6 for ATX headings, 1 or 2 for setext headings.
    pub level: u8,

    /// The content of the heading as a "flattened" string&mdash;flattened in the sense that any
    /// `<strong>` or other tags are removed. In the Markdown heading `## This is **bold**`, for
    /// example, the would be the string `"This is bold"`.
    pub content: String,
}

/// Implement this adapter for creating a plugin for custom headings (`h1`, `h2`, etc.). The `enter`
/// method defines what's rendered prior the AST content of the heading while the `exit` method
/// defines what's rendered after it. Both methods provide access to a [`HeadingMeta`] struct and
/// leave the AST content of the heading unchanged.
pub trait HeadingAdapter: Send + Sync {
    /// Render the opening tag.
    fn enter(
        &self,
        output: &mut dyn fmt::Write,
        heading: &HeadingMeta,
        sourcepos: Option<Sourcepos>,
    ) -> fmt::Result;

    /// Render the closing tag.
    fn exit(&self, output: &mut dyn fmt::Write, heading: &HeadingMeta) -> fmt::Result;
}
