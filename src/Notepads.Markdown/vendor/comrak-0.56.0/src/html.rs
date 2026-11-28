// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

//! HTML rendering infrastructure for the CommonMark AST, as well as helper
//! functions.
//!
//! [`format_document`] and [`format_document_with_plugins`] use the standard
//! formatter. The [`create_formatter!`][super::create_formatter] macro allows
//! specialisation of formatting for specific node types.

mod anchorizer;
mod context;

use memchr_n::MemchrN;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::str;
use std::sync::OnceLock;

use crate::adapters::HeadingMeta;
use crate::character_set::character_set;
use crate::ctype::isspace;
#[cfg(feature = "shortcodes")]
use crate::nodes::NodeShortCode;
use crate::nodes::{
    ListType, Node, NodeAlert, NodeBlockDirective, NodeCode, NodeCodeBlock, NodeFootnoteDefinition,
    NodeFootnoteReference, NodeHeading, NodeHtmlBlock, NodeLink, NodeList, NodeMath, NodeTaskItem,
    NodeValue, NodeWikiLink, TableAlignment,
};
use crate::parser::options::{AlertStyleType, Options, Plugins};
use crate::{node_matches, scanners};

#[doc(hidden)]
pub use anchorizer::Anchorizer;
pub use context::Context;

/// Legacy formatters cannot materialize private native accepted values.
pub(crate) fn require_owned_ast(root: Node<'_>) -> fmt::Result {
    for node in root.descendants() {
        require_owned_node(&node.data().value)?;
    }
    Ok(())
}

fn require_owned_node(value: &NodeValue) -> fmt::Result {
    match value {
        NodeValue::Text(text) | NodeValue::HtmlInline(text) => {
            text.owned_str().ok_or(fmt::Error)?;
        }
        NodeValue::Math(math) => {
            math.literal.owned_str().ok_or(fmt::Error)?;
        }
        NodeValue::Code(code) => {
            code.literal.owned_str().ok_or(fmt::Error)?;
        }
        NodeValue::CodeBlock(code) => {
            code.info.owned_str().ok_or(fmt::Error)?;
            code.literal.owned_str().ok_or(fmt::Error)?;
        }
        NodeValue::HtmlBlock(html) => {
            html.literal.owned_str().ok_or(fmt::Error)?;
        }
        NodeValue::Link(link) | NodeValue::Image(link) => {
            link.url.owned_str().ok_or(fmt::Error)?;
            link.title.owned_str().ok_or(fmt::Error)?;
        }
        _ => {}
    }
    Ok(())
}

/// Formats an AST as HTML, modified by the given options.
pub fn format_document(root: Node<'_>, options: &Options, output: &mut dyn Write) -> fmt::Result {
    require_owned_ast(root)?;
    // Validate the AST as part of the debug build. See https://github.com/kivikakk/comrak/issues/371.
    #[cfg(debug_assertions)]
    root.validate().unwrap_or_else(|e| {
        panic!("The document to format is ill-formed: {:?}", e);
    });

    format_document_with_formatter(
        root,
        options,
        output,
        &Plugins::default(),
        format_node_default,
        (),
    )
}

/// Formats an AST as HTML, modified by the given options. Accepts custom plugins.
pub fn format_document_with_plugins(
    root: Node<'_>,
    options: &Options,
    output: &mut dyn fmt::Write,
    plugins: &Plugins,
) -> fmt::Result {
    require_owned_ast(root)?;
    format_document_with_formatter(root, options, output, plugins, format_node_default, ())
}

/// Returned by the [`format_document_with_formatter`] callback to indicate
/// whether children of a node should be rendered with full HTML as usual, in
/// "plain" mode, such as for the `title` attribute of an image (which is a full
/// subtree in CommonMark — there are probably few other use cases for "plain"
/// mode), or whether they should be skipped.
#[derive(Debug, Clone, Copy)]
pub enum ChildRendering {
    /// Indicates children should be rendered in full HTML as usual.
    HTML,
    /// Indicates children should be rendered in "plain" mode; see the source of
    /// [`format_document_with_formatter`] for details.
    Plain,
    /// Indicates children should be skipped.
    Skip,
}

/// Create a formatter with specialised rules for certain node types.
///
/// Give the name of the newly created struct, and then a list of [`NodeValue`]
/// match cases within curly braces. The left-hand side are regular patterns and
/// can include captures. The right-hand side starts with a mandatory list of
/// contextual captures, similar to lambdas. The following contextual captures
/// are available:
///
/// * `context`: the <code>[&mut] [Context]</code>, giving access to rendering
///   options, plugins, and output appending via its <code>[Write]</code>
///   implementation.
/// * `node`: the <code>[&][&][Node]</code> being formatted, when the
///   [`NodeValue`]'s contents aren't enough.
/// * `entering`: [`true`] when the node is being first descended into,
///   [`false`] when being exited.
///
/// By default, an overridden formatter will return [`ChildRendering::HTML`],
/// causing children of the node to be rendered as HTML as usual.  You can
/// return one of these enum values (wrapped in [`Ok`]) from within your
/// override to change this behaviour, in some or all cases.  These values are
/// only noted when `entering` a node.
///
/// If you supply a type parameter after the name of your formatter, it will be
/// taken as an additional argument on the generated `format_document` method,
/// is available on the [`Context`] as the `user` property, and becomes the
/// return value of `format_document`.
///
/// ```rust
/// # use comrak::{create_formatter, parse_document, Arena, Options, nodes::NodeValue, html::ChildRendering};
/// # use std::fmt::Write;
/// create_formatter!(CustomFormatter<usize>, {
///     NodeValue::Emph => |context, entering| {
///         context.user += 1;
///         if entering {
///             context.write_str("<i>")?;
///         } else {
///             context.write_str("</i>")?;
///         }
///     },
///     NodeValue::Strong => |context, entering| {
///         context.user += 1;
///         context.write_str(if entering { "<b>" } else { "</b>" })?;
///     },
///     NodeValue::Image(ref nl) => |context, node, entering| {
///         assert!(node.data().sourcepos == (3, 1, 3, 18).into());
///         if entering {
///             context.write_str(&nl.url.owned_str().ok_or(std::fmt::Error)?.to_uppercase())?;
///             return Ok(ChildRendering::Skip);
///         }
///     },
/// });
///
/// let options = Options::default();
/// let arena = Arena::new();
/// let doc = parse_document(
///     &arena,
///     "_Hello_, **world**.\n\n![title](/img.png)",
///     &options,
/// );
///
/// let mut result: String = String::new();
/// let converted_count = CustomFormatter::format_document(doc, &options, &mut result, 0).unwrap();
///
/// assert_eq!(
///     result,
///     "<p><i>Hello</i>, <b>world</b>.</p>\n<p>/IMG.PNG</p>\n"
/// );
///
/// assert_eq!(converted_count, 4);
/// ```
#[macro_export]
macro_rules! create_formatter {
    ($name:ident, { $( $pat:pat => | $( $capture:ident ),* | $case:tt ),* $(,)? }) => {
        $crate::create_formatter!(@inner $name<()>, { $( $pat => | $( $capture ),* | $case ),*, });
    };

    ($name:ident<$type:ty>, { $( $pat:pat => | $( $capture:ident ),* | $case:tt ),* $(,)? }) => {
        $crate::create_formatter!(@inner $name<$type, $type>, { $( $pat => | $( $capture ),* | $case ),*, });
    };

    // Actual implementation
    (@inner $name:ident<$type:ty $(, $user_type:ty)?>, { $( $pat:pat => | $( $capture:ident ),* | $case:tt ),* $(,)? }) => {
        #[allow(missing_copy_implementations)]
        #[allow(missing_debug_implementations)]
        /// Created by [`comrak::create_formatter!`][crate::create_formatter].
        pub struct $name;

        impl $name {
            /// Formats an AST as HTML, modified by the given options.
            #[inline]
            pub fn format_document<'a>(
                root: &'a $crate::nodes::AstNode<'a>,
                options: &$crate::Options,
                output: &mut dyn ::std::fmt::Write,
                $(user: $user_type,)?
            ) -> ::std::result::Result<$type, ::std::fmt::Error> {
                // Validate the AST as part of the debug build. See https://github.com/kivikakk/comrak/issues/371.
                #[cfg(debug_assertions)]
                root.validate().unwrap_or_else(|e| {
                    panic!("The document to format is ill-formed: {:?}", e);
                });

                #[allow(unused_mut)]
                let mut maybe_user = None$(::<$user_type>)?;
                $(maybe_user = Some::<$user_type>(user);)?
                $crate::html::format_document_with_formatter(
                    root,
                    options,
                    output,
                    &$crate::options::Plugins::default(),
                    Self::formatter,
                    maybe_user.unwrap_or(<$type>::default()),
                )
            }

            /// Formats an AST as HTML, modified by the given options. Accepts custom plugins.
            #[inline]
            pub fn format_document_with_plugins<'a, 'o, 'c: 'o>(
                root: &'a $crate::nodes::AstNode<'a>,
                options: &'o $crate::Options<'c>,
                output: &'o mut dyn ::std::fmt::Write,
                plugins: &'o $crate::options::Plugins<'o>,
                $(user: $user_type,)?
            ) -> ::std::result::Result<$type, ::std::fmt::Error> {
                #[allow(unused_mut)]
                let mut maybe_user = None$(::<$user_type>)?;
                $(maybe_user = Some::<$user_type>(user);)?
                $crate::html::format_document_with_formatter(
                    root,
                    options,
                    output,
                    plugins,
                    Self::formatter,
                    maybe_user.unwrap_or(<$type>::default()),
                )
            }

            fn formatter<'a>(
                context: &mut $crate::html::Context<$type>,
                node: &'a $crate::nodes::AstNode<'a>,
                entering: bool,
            ) -> ::std::result::Result<$crate::html::ChildRendering, ::std::fmt::Error> {
                match node.data().value {
                    $(
                        $pat => {
                            $crate::formatter_captures!((context, node, entering), ($( $capture ),*));
                            $case
                            // Don't warn on unconditional return in user code.
                            #[allow(unreachable_code)]
                            ::std::result::Result::Ok($crate::html::ChildRendering::HTML)
                        }
                    ),*
                    _ => $crate::html::format_node_default(context, node, entering),
                }
            }
        }
    };
}

/// This must be exported so its uses in [`create_formatter!`] can be expanded,
/// but it's not intended for direct use.
#[doc(hidden)]
#[macro_export]
macro_rules! formatter_captures {
    (($context:ident, $node:ident, $entering:ident), context, $bind:ident) => {
        let $bind = $context;
    };
    (($context:ident, $node:ident, $entering:ident), node, $bind:ident) => {
        let $bind = $node;
    };
    (($context:ident, $node:ident, $entering:ident), entering, $bind:ident) => {
        let $bind = $entering;
    };
    (($context:ident, $node:ident, $entering:ident), $unknown:ident, $bind:ident) => {
        compile_error!(concat!("unknown capture '", stringify!($unknown), "'; available are 'context', 'node', 'entering'"));
    };
    (($context:ident, $node:ident, $entering:ident), ($capture:ident)) => {
        $crate::formatter_captures!(($context, $node, $entering), $capture, $capture);
    };
    (($context:ident, $node:ident, $entering:ident), ($capture:ident, $( $rest:ident ),*)) => {
        $crate::formatter_captures!(($context, $node, $entering), $capture, $capture);
        $crate::formatter_captures!(($context, $node, $entering), ($( $rest ),*));
    };
}

/// Formats the given AST with all options and formatter function specified.
///
/// The default formatter as used by [`format_document`] is
/// [`format_node_default`]. It is given the [`Context`], [`Node`], and a
/// boolean indicating whether the node is being entered into or exited.  The
/// returned [`ChildRendering`] is used to inform whether and how the node's
/// children are recursed into automatically.
pub fn format_document_with_formatter<'a, 'o, 'c: 'o, T>(
    root: Node<'a>,
    options: &'o Options<'c>,
    output: &'o mut dyn Write,
    plugins: &'o Plugins<'o>,
    formatter: fn(
        context: &mut Context<T>,
        node: Node<'a>,
        entering: bool,
    ) -> Result<ChildRendering, fmt::Error>,
    user: T,
) -> Result<T, fmt::Error> {
    require_owned_ast(root)?;
    // Traverse the AST iteratively using a work stack, with pre- and
    // post-child-traversal phases. During pre-order traversal render the
    // opening tags, then push the node back onto the stack for the
    // post-order traversal phase, then push the children in reverse order
    // onto the stack and begin rendering first child.

    let mut context = Context::new(output, options, plugins, user);

    enum Phase {
        Pre,
        Post,
    }
    let mut stack = vec![(root, ChildRendering::HTML, Phase::Pre)];

    while let Some((node, child_rendering, phase)) = stack.pop() {
        match phase {
            Phase::Pre => {
                let new_cr = match child_rendering {
                    ChildRendering::Plain => {
                        match node.data().value {
                            NodeValue::Text(ref literal) => {
                                context.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                            }
                            NodeValue::Code(NodeCode { ref literal, .. })
                            | NodeValue::HtmlInline(ref literal) => {
                                context.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                            }
                            NodeValue::LineBreak | NodeValue::SoftBreak => {
                                fmt::Write::write_str(&mut context, " ")?;
                            }
                            NodeValue::Math(NodeMath { ref literal, .. }) => {
                                context.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                            }
                            _ => (),
                        }
                        ChildRendering::Plain
                    }
                    ChildRendering::HTML => {
                        stack.push((node, ChildRendering::HTML, Phase::Post));
                        formatter(&mut context, node, true)?
                    }
                    ChildRendering::Skip => {
                        // We never push a node with ChildRendering::Skip.
                        unreachable!()
                    }
                };

                if !matches!(new_cr, ChildRendering::Skip) {
                    for ch in node.reverse_children() {
                        stack.push((ch, new_cr, Phase::Pre));
                    }
                }
            }
            Phase::Post => {
                debug_assert!(matches!(child_rendering, ChildRendering::HTML));
                formatter(&mut context, node, false)?;
            }
        }
    }

    context.finish()
}

/// Default node formatting function, used by [`format_document`],
/// [`format_document_with_plugins`] and as the fallback for any node types not
/// handled in custom formatters created by [`create_formatter!`].
#[inline]
pub fn format_node_default<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    require_owned_node(&node.data().value)?;
    match node.data().value {
        // Commonmark
        NodeValue::BlockQuote => render_block_quote(context, node, entering),
        NodeValue::Code(ref nc) => render_code(context, node, entering, nc),
        NodeValue::CodeBlock(ref ncb) => render_code_block(context, node, entering, ncb),
        NodeValue::Document => Ok(ChildRendering::HTML),
        NodeValue::Emph => render_emph(context, node, entering),
        NodeValue::Heading(ref nh) => render_heading(context, node, entering, nh),
        NodeValue::HtmlBlock(ref nhb) => render_html_block(context, entering, nhb),
        #[cfg(feature = "phoenix_heex")]
        NodeValue::HeexBlock(ref nhb) => render_heex_block(context, entering, nhb),
        NodeValue::HtmlInline(ref literal) => {
            render_html_inline(context, entering, literal.owned_str().ok_or(fmt::Error)?)
        }
        #[cfg(feature = "phoenix_heex")]
        NodeValue::HeexInline(ref literal) => render_heex_inline(context, entering, literal),
        NodeValue::Image(ref nl) => render_image(context, node, entering, nl),
        NodeValue::NoteEmbed(ref target) => {
            if entering {
                context.write_str("![[")?;
                context.escape(target.url.owned_str().ok_or(fmt::Error)?)?;
                context.write_str("]]")?;
            }
            Ok(ChildRendering::HTML)
        }
        NodeValue::Item(_) => render_item(context, node, entering),
        NodeValue::LineBreak => render_line_break(context, node, entering),
        NodeValue::Link(ref nl) => render_link(context, node, entering, nl),
        NodeValue::List(ref nl) => render_list(context, node, entering, nl),
        NodeValue::Paragraph => render_paragraph(context, node, entering),
        NodeValue::SoftBreak => render_soft_break(context, node, entering),
        NodeValue::Strong => render_strong(context, node, entering),
        NodeValue::Text(ref literal) => {
            render_text(context, entering, literal.owned_str().ok_or(fmt::Error)?)
        }
        NodeValue::ThematicBreak => render_thematic_break(context, node, entering),
        NodeValue::TableOfContents => {
            if entering {
                context.cr()?;
                context.write_str("<p>[TOC]</p>")?;
                context.lf()?;
            }
            Ok(ChildRendering::HTML)
        }

        // GFM
        NodeValue::FootnoteDefinition(ref nfd) => {
            render_footnote_definition(context, node, entering, nfd)
        }
        NodeValue::FootnoteReference(ref nfr) => {
            render_footnote_reference(context, node, entering, nfr)
        }
        NodeValue::Strikethrough => render_strikethrough(context, node, entering),
        NodeValue::Highlight => render_highlight(context, node, entering),
        NodeValue::Insert => render_insert(context, node, entering),
        NodeValue::Table(_) => render_table(context, node, entering),
        NodeValue::TableCell => render_table_cell(context, node, entering),
        NodeValue::TableRow(thead) => render_table_row(context, node, entering, thead),
        NodeValue::TaskItem(ref nti) => render_task_item(context, node, entering, nti),
        NodeValue::TaskCheckbox(marker) => {
            if entering {
                context.write_str("<input type=\"checkbox\" disabled=\"\"")?;
                if marker.checked {
                    context.write_str(" checked=\"\"")?;
                }
                context.write_str(" />")?;
            }
            Ok(ChildRendering::Skip)
        }

        // Extensions
        NodeValue::Alert(ref alert) => render_alert(context, node, entering, alert),
        NodeValue::DescriptionDetails => render_description_details(context, node, entering),
        NodeValue::DescriptionItem(_) => Ok(ChildRendering::HTML),
        NodeValue::DescriptionList => render_description_list(context, node, entering),
        NodeValue::DescriptionTerm => render_description_term(context, node, entering),
        NodeValue::Escaped => render_escaped(context, node, entering),
        NodeValue::EscapedTag(net) => render_escaped_tag(context, net),
        NodeValue::FrontMatter(_) => Ok(ChildRendering::HTML),
        NodeValue::Math(ref nm) => render_math(context, node, entering, nm),
        NodeValue::MultilineBlockQuote(_) => render_multiline_block_quote(context, node, entering),
        NodeValue::Raw(ref literal) => render_raw(context, entering, literal),
        #[cfg(feature = "shortcodes")]
        NodeValue::ShortCode(ref nsc) => render_short_code(context, entering, nsc),
        NodeValue::SpoileredText => render_spoiler_text(context, node, entering),
        NodeValue::Subscript => render_subscript(context, node, entering),
        NodeValue::Superscript => render_superscript(context, node, entering),
        NodeValue::Underline => render_underline(context, node, entering),
        NodeValue::WikiLink(ref nwl) => render_wiki_link(context, node, entering, nwl),
        NodeValue::Subtext => render_subtext(context, node, entering),
        NodeValue::BlockDirective(ref nbd) => render_block_directive(context, node, entering, nbd),
    }
}

// Commonmark

/// Renders sourcepos data for the given node to the supplied [`Context`].
///
/// This function renders anything iff `context.options.render.sourcepos` is
/// true, and includes a leading space if so, so you can use it  unconditionally
/// immediately before writing a closing `>` in your opening HTML tag.
pub fn render_sourcepos<T>(context: &mut Context<T>, node: Node<'_>) -> fmt::Result {
    if context.options.render.sourcepos {
        let ast = node.data();
        if ast.sourcepos.start.line > 0 {
            write!(context, " data-sourcepos=\"{}\"", ast.sourcepos)?;
        }
    }
    Ok(())
}

fn render_block_directive<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nbd: &NodeBlockDirective,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<div class=\"")?;
        context.escape(&nbd.info)?;
        context.write_str("\"")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.cr()?;
        context.write_str("</div>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_block_quote<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<blockquote")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
        context.lf()?;
    } else {
        context.cr()?;
        context.write_str("</blockquote>")?;
        context.lf()?;
    }
    Ok(ChildRendering::HTML)
}

fn render_code<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nc: &NodeCode,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<code")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
        context.escape(nc.literal.owned_str().ok_or(fmt::Error)?)?;
        context.write_str("</code>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_code_block<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    ncb: &NodeCodeBlock,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        let info = ncb.info.owned_str().ok_or(fmt::Error)?;
        let literal = ncb.literal.owned_str().ok_or(fmt::Error)?;
        let info_bytes = info.as_bytes();
        let mut first_tag = 0;

        while first_tag < info.len() && !isspace(info_bytes[first_tag]) {
            first_tag += 1;
        }

        let lang = &info[..first_tag];
        let meta = info[first_tag..].trim();

        if lang.eq("math") {
            render_math_code_block(context, node, literal)?;
        } else if !lang.is_empty()
            && let Some(adapter) = context.plugins.render.codefence_renderers.get(lang)
        {
            context.cr()?;
            let sourcepos = if context.options.render.sourcepos {
                Some(node.data().sourcepos)
            } else {
                None
            };

            adapter.write(context, lang, meta, literal, sourcepos)?;
            return Ok(ChildRendering::HTML);
        }

        if !lang.eq("math") {
            context.cr()?;

            let mut pre_attributes: HashMap<&str, Cow<str>> = HashMap::new();
            let mut code_attributes: HashMap<&str, Cow<str>> = HashMap::new();
            let code_attr: String;

            if !info.is_empty() {
                if context.options.render.github_pre_lang {
                    pre_attributes.insert("lang", lang.into());

                    if context.options.render.full_info_string && !meta.is_empty() {
                        pre_attributes.insert("data-meta", meta.trim().into());
                    }
                } else {
                    code_attr = format!("language-{}", lang);
                    code_attributes.insert("class", code_attr.into());

                    if context.options.render.full_info_string && !meta.is_empty() {
                        code_attributes.insert("data-meta", meta.into());
                    }
                }
            }

            if context.options.render.sourcepos {
                let ast = node.data();
                pre_attributes.insert("data-sourcepos", ast.sourcepos.to_string().into());
            }

            match context.plugins.render.codefence_syntax_highlighter {
                None => {
                    write_opening_tag(context, "pre", pre_attributes)?;
                    write_opening_tag(context, "code", code_attributes)?;

                    context.escape(literal)?;

                    context.write_str("</code></pre>")?;
                    context.lf()?
                }
                Some(highlighter) => {
                    highlighter.write_pre_tag(context, pre_attributes)?;
                    highlighter.write_code_tag(context, code_attributes)?;

                    highlighter.write_highlighted(context, Some(lang), literal)?;

                    context.write_str("</code></pre>")?;
                    context.lf()?
                }
            }
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_emph<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<em")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</em>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_heading<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nh: &NodeHeading,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        require_owned_ast(node)?;
    }
    match context.plugins.render.heading_adapter {
        None => {
            if entering {
                context.cr()?;
                write!(context, "<h{}", nh.level)?;

                if let Some(prefix) = &context.options.extension.header_id_prefix {
                    let text_content = node.collect_text();
                    let id = context.anchorizer.anchorize(&text_content);

                    write!(context, r##" id="{prefix}{id}""##)?;
                    context.current_anchorized_id = Some(id);
                    context.current_heading_text = Some(text_content);
                };

                render_sourcepos(context, node)?;
                context.write_str(">")?;
            } else {
                if let Some(prefix) = &context.options.extension.header_id_prefix {
                    let text_content = context.current_heading_text.take().unwrap();
                    let id = context.current_anchorized_id.take().unwrap();
                    let href_prefix = if context.options.extension.header_id_prefix_in_href {
                        prefix.as_str()
                    } else {
                        ""
                    };
                    write!(
                        context,
                        r##"<a href="#{href_prefix}{id}" aria-label="Link to heading '"##
                    )?;
                    context.escape(&text_content)?;
                    context.write_str(r#"'" data-heading-content=""#)?;
                    context.escape(&text_content)?;
                    context.write_str(r#"" class="anchor"></a>"#)?;
                }
                write!(context, "</h{}>", nh.level)?;
                context.lf()?;
            }
        }
        Some(adapter) => {
            if entering {
                let heading = HeadingMeta {
                    level: nh.level,
                    content: node.collect_text(),
                };

                context.cr()?;
                let sp = if context.options.render.sourcepos {
                    Some(node.data().sourcepos)
                } else {
                    None
                };
                adapter.enter(context, &heading, sp)?;
                context.current_heading_text = Some(heading.content);
            } else {
                let heading = HeadingMeta {
                    level: nh.level,
                    content: context.current_heading_text.take().unwrap(),
                };
                adapter.exit(context, &heading)?;
            }
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_html_block<T>(
    context: &mut Context<T>,
    entering: bool,
    nhb: &NodeHtmlBlock,
) -> Result<ChildRendering, fmt::Error> {
    // No sourcepos.
    if entering {
        context.cr()?;
        let literal = nhb.literal.owned_str().ok_or(fmt::Error)?;
        #[allow(deprecated)]
        if context.options.render.escape {
            context.escape(literal)?;
        } else if !context.options.render.r#unsafe {
            context.write_str("<!-- raw HTML omitted -->")?;
        } else {
            context.write_str(literal)?;
        }
        context.cr()?;
    }

    Ok(ChildRendering::HTML)
}

#[cfg(feature = "phoenix_heex")]
fn render_heex_block<T>(
    context: &mut Context<T>,
    entering: bool,
    nhb: &crate::nodes::NodeHeexBlock,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str(&nhb.literal)?;
        context.cr()?;
    }

    Ok(ChildRendering::HTML)
}

#[cfg(feature = "phoenix_heex")]
fn render_heex_inline<T>(
    context: &mut Context<T>,
    entering: bool,
    literal: &str,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str(literal)?;
    }

    Ok(ChildRendering::HTML)
}

fn render_html_inline<T>(
    context: &mut Context<T>,
    entering: bool,
    literal: &str,
) -> Result<ChildRendering, fmt::Error> {
    // No sourcepos.
    if entering {
        #[allow(deprecated)]
        if context.options.render.escape {
            context.escape(literal)?;
        } else if !context.options.render.r#unsafe {
            context.write_str("<!-- raw HTML omitted -->")?;
        } else {
            context.write_str(literal)?;
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_image<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nl: &NodeLink,
) -> Result<ChildRendering, fmt::Error> {
    let url = nl.url.owned_str().ok_or(fmt::Error)?;
    let title = nl.title.owned_str().ok_or(fmt::Error)?;
    if entering {
        if context.options.render.figure_with_caption {
            context.write_str("<figure>")?;
        }
        context.write_str("<img")?;
        render_sourcepos(context, node)?;
        context.write_str(" src=\"")?;
        if context.options.render.r#unsafe || !dangerous_url(url) {
            if let Some(rewriter) = &context.options.extension.image_url_rewriter {
                context.escape_href(&rewriter.to_html(url))?;
            } else {
                context.escape_href(url)?;
            }
        }
        context.write_str("\" alt=\"")?;
        return Ok(ChildRendering::Plain);
    } else {
        if !title.is_empty() {
            context.write_str("\" title=\"")?;
            context.escape(title)?;
        }
        context.write_str("\" />")?;
        if context.options.render.figure_with_caption {
            if !title.is_empty() {
                context.write_str("<figcaption>")?;
                context.escape(title)?;
                context.write_str("</figcaption>")?;
            }
            context.write_str("</figure>")?;
        };
    }

    Ok(ChildRendering::HTML)
}

fn render_item<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<li")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</li>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_line_break<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<br")?;
        render_sourcepos(context, node)?;
        context.write_str(" />")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_link<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nl: &NodeLink,
) -> Result<ChildRendering, fmt::Error> {
    let url = nl.url.owned_str().ok_or(fmt::Error)?;
    let title = nl.title.owned_str().ok_or(fmt::Error)?;
    let parent_node = node.parent();

    if !context.options.parse.relaxed_autolinks
        || (parent_node.is_none() || !node_matches!(parent_node.unwrap(), NodeValue::Link(..)))
    {
        if entering {
            context.write_str("<a")?;
            render_sourcepos(context, node)?;
            context.write_str(" href=\"")?;
            if context.options.render.r#unsafe || !dangerous_url(url) {
                if let Some(rewriter) = &context.options.extension.link_url_rewriter {
                    context.escape_href(&rewriter.to_html(url))?;
                } else {
                    context.escape_href(url)?;
                }
            }
            if !title.is_empty() {
                context.write_str("\" title=\"")?;
                context.escape(title)?;
            }
            context.write_str("\">")?;
        } else {
            context.write_str("</a>")?;
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_list<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nl: &NodeList,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        match nl.list_type {
            ListType::Bullet => {
                context.write_str("<ul")?;
                if nl.is_task_list && context.options.render.tasklist_classes {
                    context.write_str(" class=\"contains-task-list\"")?;
                }
                render_sourcepos(context, node)?;
                context.write_str(">")?;
                context.lf()?;
            }
            ListType::Ordered => {
                context.write_str("<ol")?;
                if nl.is_task_list && context.options.render.tasklist_classes {
                    context.write_str(" class=\"contains-task-list\"")?;
                }
                render_sourcepos(context, node)?;
                if nl.start == 1 {
                    context.write_str(">")?;
                    context.lf()?;
                } else {
                    write!(context, " start=\"{}\">", nl.start)?;
                    context.lf()?;
                }
            }
        }
    } else if nl.list_type == ListType::Bullet {
        context.write_str("</ul>")?;
        context.lf()?;
    } else {
        context.write_str("</ol>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_paragraph<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    let tight = node
        .parent()
        .and_then(|n| n.parent())
        .is_some_and(|n| match n.data().value {
            NodeValue::List(nl) => nl.tight,
            NodeValue::DescriptionItem(ndi) => ndi.tight,
            _ => false,
        })
        || node
            .parent()
            .is_some_and(|n| node_matches!(n, NodeValue::DescriptionTerm));

    if !tight {
        if entering {
            context.cr()?;
            context.write_str("<p")?;
            render_sourcepos(context, node)?;
            context.write_str(">")?;
        } else {
            if let Some(parent) = node.parent()
                && let NodeValue::FootnoteDefinition(ref nfd) = parent.data().value
                && node.next_sibling().is_none()
            {
                context.write_str(" ")?;
                put_footnote_backref(context, nfd)?;
            }
            context.write_str("</p>")?;
            context.lf()?;
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_soft_break<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        if context.options.render.hardbreaks {
            context.write_str("<br")?;
            render_sourcepos(context, node)?;
            context.write_str(" />")?;
            context.lf()?;
        } else {
            context.write_str("\n")?;
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_strong<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    let parent_node = node.parent();
    if !context.options.render.gfm_quirks
        || (parent_node.is_none() || !node_matches!(parent_node.unwrap(), NodeValue::Strong))
    {
        if entering {
            context.write_str("<strong")?;
            render_sourcepos(context, node)?;
            context.write_str(">")?;
        } else {
            context.write_str("</strong>")?;
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_text<T>(
    context: &mut Context<T>,
    entering: bool,
    literal: &str,
) -> Result<ChildRendering, fmt::Error> {
    // Nowhere to put sourcepos.
    if entering {
        context.escape(literal)?;
    }

    Ok(ChildRendering::HTML)
}

fn render_thematic_break<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<hr")?;
        render_sourcepos(context, node)?;
        context.write_str(" />")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

// GFM

fn render_footnote_definition<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nfd: &NodeFootnoteDefinition,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        if context.footnote_ix == 0 {
            context.write_str("<section")?;
            render_sourcepos(context, node)?;
            context.write_str(" class=\"footnotes\" data-footnotes>")?;
            context.lf()?;
            context.write_str("<ol>")?;
            context.lf()?;
        }
        context.footnote_ix += 1;
        context.write_str("<li")?;
        render_sourcepos(context, node)?;
        context.write_str(" id=\"fn-")?;
        context.escape_href(&nfd.name)?;
        context.write_str("\">")?;
    } else {
        if put_footnote_backref(context, nfd)? {
            context.lf()?;
        }
        context.write_str("</li>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_footnote_reference<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nfr: &NodeFootnoteReference,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        let mut ref_id = format!("fnref-{}", nfr.name);
        if nfr.ref_num > 1 {
            ref_id = format!("{}-{}", ref_id, nfr.ref_num);
        }

        context.write_str("<sup")?;
        render_sourcepos(context, node)?;
        context.write_str(" class=\"footnote-ref\"><a href=\"#fn-")?;
        context.escape_href(&nfr.name)?;
        context.write_str("\" id=\"")?;
        context.escape_href(&ref_id)?;
        write!(context, "\" data-footnote-ref>{}</a></sup>", nfr.ix)?;
    }

    Ok(ChildRendering::HTML)
}

fn render_strikethrough<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<del")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</del>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_highlight<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<mark")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</mark>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_insert<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<ins")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</ins>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_table<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<table")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
        context.lf()?;
    } else {
        if let Some(true) = node
            .last_child()
            .map(|n| !n.same_node(node.first_child().unwrap()))
        // node.first_child() guaranteed to exist in block since last_child does!
        {
            context.cr()?;
            context.write_str("</tbody>")?;
            context.lf()?;
        }
        context.cr()?;
        context.write_str("</table>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_table_cell<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    let Some(row_node) = node.parent() else {
        panic!("rendered a table cell without a containing table row");
    };
    let row = &row_node.data().value;
    let in_header = match *row {
        NodeValue::TableRow(header) => header,
        _ => panic!("rendered a table cell contained by something other than a table row"),
    };

    let Some(table_node) = row_node.parent() else {
        panic!("rendered a table cell without a containing table");
    };
    let table = &table_node.data().value;
    let alignments = match table {
        NodeValue::Table(nt) => &nt.alignments,
        _ => {
            panic!("rendered a table cell in a table row contained by something other than a table")
        }
    };

    if entering {
        context.cr()?;
        if in_header {
            context.write_str("<th")?;
            render_sourcepos(context, node)?;
        } else {
            context.write_str("<td")?;
            render_sourcepos(context, node)?;
        }

        let mut start = row_node.first_child().unwrap(); // guaranteed to exist because `node' itself does!
        let mut i = 0;
        while !start.same_node(node) {
            i += 1;
            start = start.next_sibling().unwrap();
        }

        match alignments[i] {
            TableAlignment::Left => {
                context.write_str(" align=\"left\"")?;
            }
            TableAlignment::Right => {
                context.write_str(" align=\"right\"")?;
            }
            TableAlignment::Center => {
                context.write_str(" align=\"center\"")?;
            }
            TableAlignment::None => (),
        }

        context.write_str(">")?;
    } else if in_header {
        context.write_str("</th>")?;
    } else {
        context.write_str("</td>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_table_row<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    thead: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        if thead {
            context.write_str("<thead>")?;
            context.lf()?;
        } else if let Some(n) = node.previous_sibling()
            && let NodeValue::TableRow(true) = n.data().value
        {
            context.write_str("<tbody>")?;
            context.lf()?;
        }
        context.write_str("<tr")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.cr()?;
        context.write_str("</tr>")?;
        if thead {
            context.cr()?;
            context.write_str("</thead>")?;
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_task_item<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nti: &NodeTaskItem,
) -> Result<ChildRendering, fmt::Error> {
    let write_li = node
        .parent()
        .map(|p| node_matches!(p, NodeValue::List(_)))
        .unwrap_or_default();

    if entering {
        context.cr()?;
        if write_li {
            context.write_str("<li")?;
            if context.options.render.tasklist_classes {
                context.write_str(" class=\"task-list-item\"")?;
            }
            render_sourcepos(context, node)?;
            context.write_str(">")?;
        }
        context.write_str("<input type=\"checkbox\"")?;
        if !write_li {
            render_sourcepos(context, node)?;
        }
        if context.options.render.tasklist_classes {
            context.write_str(" class=\"task-list-item-checkbox\"")?;
        }
        if nti.symbol.is_some() {
            context.write_str(" checked=\"\"")?;
        }
        context.write_str(" disabled=\"\" /> ")?;
    } else if write_li {
        context.write_str("</li>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

// Extensions

fn render_alert<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    alert: &NodeAlert,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str(match context.options.render.alert_style {
            AlertStyleType::Specific => "<div class=\"markdown-alert ",
            AlertStyleType::Semantic => "<aside class=\"admonition ",
        })?;
        context.write_str(
            alert
                .alert_type
                .css_class(context.options.render.alert_style),
        )?;
        context.write_str("\"")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
        context.lf()?;
        context.write_str("<p class=\"")?;
        context.write_str(match context.options.render.alert_style {
            AlertStyleType::Specific => "markdown-alert-title",
            AlertStyleType::Semantic => "admonition-title",
        })?;
        context.write_str("\">")?;
        match alert.title {
            Some(ref title) => context.escape(title)?,
            None => {
                context.write_str(alert.alert_type.default_title())?;
            }
        }
        context.write_str("</p>")?;
        context.lf()?;
    } else {
        context.cr()?;
        context.write_str(match context.options.render.alert_style {
            AlertStyleType::Specific => "</div>",
            AlertStyleType::Semantic => "</aside>",
        })?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_description_details<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<dd")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</dd>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_description_list<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<dl")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
        context.lf()?;
    } else {
        context.write_str("</dl>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_description_term<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<dt")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</dt>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_escaped<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if context.options.render.escaped_char_spans {
        if entering {
            context.write_str("<span data-escaped-char")?;
            render_sourcepos(context, node)?;
            context.write_str(">")?;
        } else {
            context.write_str("</span>")?;
        }
    }

    Ok(ChildRendering::HTML)
}

fn render_escaped_tag<T>(
    context: &mut Context<T>,
    net: &str,
) -> Result<ChildRendering, fmt::Error> {
    // Nowhere to put sourcepos.
    context.write_str(net)?;

    Ok(ChildRendering::HTML)
}

/// Renders a math dollar inline, `$...$` and `$$...$$` using `<span>` to be
/// similar to other renderers.
pub fn render_math<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nm: &NodeMath,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        let mut tag_attributes: Vec<(&str, Cow<str>)> = Vec::new();
        let style_attr = if nm.display_math { "display" } else { "inline" };
        let tag: &str = if nm.dollar_math { "span" } else { "code" };

        tag_attributes.push(("data-math-style", style_attr.into()));

        if context.options.render.sourcepos {
            let ast = node.data();
            tag_attributes.push(("data-sourcepos", ast.sourcepos.to_string().into()));
        }

        write_opening_tag(context, tag, tag_attributes)?;
        context.escape(nm.literal.owned_str().ok_or(fmt::Error)?)?;
        write!(context, "</{tag}>")?;
    }

    Ok(ChildRendering::HTML)
}

/// Renders a math code block, ```` ```math ```` using `<pre><code>`.
pub fn render_math_code_block<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    literal: &str,
) -> Result<ChildRendering, fmt::Error> {
    context.cr()?;

    // use vectors to ensure attributes always written in the same order,
    // for testing stability
    let mut pre_attributes: Vec<(&str, Cow<str>)> = Vec::new();
    let mut code_attributes: Vec<(&str, Cow<str>)> = Vec::new();
    let lang_str = "math";

    if context.options.render.github_pre_lang {
        pre_attributes.push(("lang", lang_str.into()));
        pre_attributes.push(("data-math-style", "display".into()));
    } else {
        let code_attr = format!("language-{}", lang_str);
        code_attributes.push(("class", code_attr.into()));
        code_attributes.push(("data-math-style", "display".into()));
    }

    if context.options.render.sourcepos {
        let ast = node.data();
        pre_attributes.push(("data-sourcepos", ast.sourcepos.to_string().into()));
    }

    write_opening_tag(context, "pre", pre_attributes)?;
    write_opening_tag(context, "code", code_attributes)?;

    context.escape(literal)?;
    context.write_str("</code></pre>")?;
    context.lf()?;

    Ok(ChildRendering::HTML)
}

fn render_multiline_block_quote<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<blockquote")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
        context.lf()?;
    } else {
        context.cr()?;
        context.write_str("</blockquote>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_raw<T>(
    context: &mut Context<T>,
    entering: bool,
    literal: &str,
) -> Result<ChildRendering, fmt::Error> {
    // No sourcepos.
    if entering {
        context.write_str(literal)?;
    }

    Ok(ChildRendering::HTML)
}

#[cfg(feature = "shortcodes")]
fn render_short_code<T>(
    context: &mut Context<T>,
    entering: bool,
    nsc: &NodeShortCode,
) -> Result<ChildRendering, fmt::Error> {
    // Nowhere to put sourcepos.
    if entering {
        context.write_str(&nsc.emoji)?;
    }

    Ok(ChildRendering::HTML)
}

fn render_spoiler_text<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<span")?;
        render_sourcepos(context, node)?;
        context.write_str(" class=\"spoiler\">")?;
    } else {
        context.write_str("</span>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_subscript<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<sub")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</sub>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_subtext<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.cr()?;
        context.write_str("<p><sub")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        write!(context, "</sub></p>")?;
        context.lf()?;
    }

    Ok(ChildRendering::HTML)
}

fn render_superscript<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<sup")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</sup>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_underline<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<u")?;
        render_sourcepos(context, node)?;
        context.write_str(">")?;
    } else {
        context.write_str("</u>")?;
    }

    Ok(ChildRendering::HTML)
}

fn render_wiki_link<T>(
    context: &mut Context<T>,
    node: Node<'_>,
    entering: bool,
    nwl: &NodeWikiLink,
) -> Result<ChildRendering, fmt::Error> {
    if entering {
        context.write_str("<a")?;
        render_sourcepos(context, node)?;
        context.write_str(" href=\"")?;
        let url = &nwl.url;
        if context.options.render.r#unsafe || !dangerous_url(url) {
            context.escape_href(url)?;
        }
        context.write_str("\" data-wikilink=\"true")?;
        context.write_str("\">")?;
    } else {
        context.write_str("</a>")?;
    }

    Ok(ChildRendering::HTML)
}

fn put_footnote_backref<T>(
    context: &mut Context<T>,
    nfd: &NodeFootnoteDefinition,
) -> Result<bool, fmt::Error> {
    if context.written_footnote_ix >= context.footnote_ix {
        return Ok(false);
    }

    context.written_footnote_ix = context.footnote_ix;

    let mut ref_suffix = String::new();
    let mut superscript = String::new();

    for ref_num in 1..=nfd.total_references {
        if ref_num > 1 {
            ref_suffix = format!("-{ref_num}");
            superscript = format!("<sup class=\"footnote-ref\">{ref_num}</sup>");
            write!(context, " ")?;
        }

        context.write_str("<a href=\"#fnref-")?;
        context.escape_href(&nfd.name)?;
        let fnix = context.footnote_ix;
        write!(
            context,
            "{ref_suffix}\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"{fnix}{ref_suffix}\" aria-label=\"Back to reference {fnix}{ref_suffix}\">↩{superscript}</a>",
        )?;
    }
    Ok(true)
}

/// Check if the input would be considered a dangerous url
pub fn dangerous_url(input: &str) -> bool {
    scanners::dangerous_url(input).is_some()
}

/// Writes buffer to output, escaping anything that could be interpreted as an
/// HTML tag.
///
/// Namely:
///
/// * U+0022 QUOTATION MARK " is rendered as &quot;
/// * U+0026 AMPERSAND & is rendered as &amp;
/// * U+003C LESS-THAN SIGN < is rendered as &lt;
/// * U+003E GREATER-THAN SIGN > is rendered as &gt;
/// * Everything else is passed through unchanged.
///
/// Note that this is appropriate and sufficient for free text, but not for
/// URLs in attributes.  See escape_href.
pub fn escape(output: &mut dyn Write, buffer: &str) -> fmt::Result {
    static MATCHER: OnceLock<MemchrN> = OnceLock::new();
    let bytes = buffer.as_bytes();
    let matcher = MATCHER.get_or_init(|| MemchrN::new(b"\"&<>\0"));

    let mut last = 0;
    for i in matcher.iter(bytes) {
        let esc: &str = match bytes[i] {
            b'"' => "&quot;",
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'\0' => "\u{fffd}",
            _ => unreachable!(),
        };
        output.write_str(&buffer[last..i])?;
        output.write_str(esc)?;
        last = i + 1;
    }
    output.write_str(&buffer[last..])?;
    Ok(())
}

/// Writes buffer to output, escaping in a manner appropriate for URLs in HTML
/// attributes.
///
/// Namely:
///
/// * U+0026 AMPERSAND & is rendered as &amp;
/// * U+0027 APOSTROPHE ' is rendered as &#x27;
/// * Alphanumeric and a range of non-URL safe characters.
///
/// Note that we leave "%" alone if it is followed by two hexdigits.
/// See:
///
/// <https://github.com/github/cmark-gfm/blob/c32ef78bae851cb83b7ad52d0fbff880acdcd44a/src/houdini_href_e.c#L7-L31>
///
/// In other words, if a CommonMark user enters:
///
/// ```markdown
/// [hi](https://ddg.gg/?q=a%20b)
/// ```
///
/// We assume they actually want the query string "?q=a%20b", a search for
/// the string "a b", rather than "?q=a%2520b", a search for the literal
/// string "a%20b".
///
/// We take some care with the square bracket characters `[` and `]`. We permit
/// them to be unescaped in the output iff they surround what reasonably appears
/// to be an IPv6 address in the host part of a URI.  If `relaxed_ipv6` is
/// `true`, any scheme is permitted for such an address.  Otherwise, only `http`
/// or `https` are permitted.
pub fn escape_href(output: &mut dyn Write, buffer: &str, relaxed_ipv6: bool) -> fmt::Result {
    static MATCHER: OnceLock<MemchrN> = OnceLock::new();
    const HREF_SAFE: [bool; 256] = character_set!(
        b"-_.+!*(),#@?=;:/,+$~",
        b"abcdefghijklmnopqrstuvwxyz",
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
    );

    let matcher = MATCHER.get_or_init(|| {
        let mut bytes = Vec::new();
        for byte in 0..=u8::MAX {
            if !HREF_SAFE[byte as usize] {
                bytes.push(byte);
            }
        }
        MemchrN::new(&bytes)
    });
    let bytes = buffer.as_bytes();
    let mut last = 0;

    let possible_ipv6_url_end = if relaxed_ipv6 {
        scanners::ipv6_relaxed_url_start(buffer)
    } else {
        scanners::ipv6_url_start(buffer)
    };
    if let Some(ipv6_url_end) = possible_ipv6_url_end {
        output.write_str(&buffer[0..ipv6_url_end])?;
        last = ipv6_url_end;
    }

    let start = last;
    for i in matcher.iter(&bytes[start..]) {
        let mut i = start + i;
        if i > last {
            output.write_str(&buffer[last..i])?;
        }

        match bytes[i] {
            b'&' => {
                output.write_str("&amp;")?;
            }
            b'\'' => {
                output.write_str("&#x27;")?;
            }
            b'%' => {
                if bytes.get(i + 1).is_some_and(|b| b.is_ascii_hexdigit())
                    && bytes.get(i + 2).is_some_and(|b| b.is_ascii_hexdigit())
                {
                    output.write_str(&buffer[i..=i + 2])?;
                    i += 2;
                } else {
                    output.write_str("%25")?;
                }
            }
            0 => {
                // U+FFFD REPLACEMENT CHARACTER
                output.write_str("%EF%BF%BD")?;
            }
            _ => write!(output, "%{:02X}", bytes[i])?,
        }

        last = i + 1;
    }

    output.write_str(&buffer[last..])?;
    Ok(())
}

/// Writes an opening HTML tag, using an iterator to enumerate the attributes.
/// Note that attribute values are automatically escaped.
pub fn write_opening_tag<K: AsRef<str>, V: AsRef<str>>(
    output: &mut dyn Write,
    tag: &str,
    attributes: impl IntoIterator<Item = (K, V)>,
) -> fmt::Result {
    write!(output, "<{tag}")?;
    for (attr, val) in attributes {
        write!(output, " {}=\"", attr.as_ref())?;
        escape(output, val.as_ref())?;
        output.write_str("\"")?;
    }
    output.write_str(">")?;
    Ok(())
}
