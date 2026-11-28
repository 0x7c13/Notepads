// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use std::cmp;
use std::fmt::{self, Write};

use crate::node_matches;
use crate::nodes::{ListType, NodeCode, NodeMath, NodeValue};
use crate::nodes::{Node, NodeHtmlBlock};
use crate::parser::options::{Options, Plugins};

const MAX_INDENT: u32 = 40;

/// Formats an AST as HTML, modified by the given options.
pub fn format_document(root: Node<'_>, options: &Options, output: &mut dyn Write) -> fmt::Result {
    crate::html::require_owned_ast(root)?;
    // Validate the AST as part of the debug build. See https://github.com/kivikakk/comrak/issues/371.
    #[cfg(debug_assertions)]
    root.validate().unwrap_or_else(|e| {
        panic!("The document to format is ill-formed: {:?}", e);
    });

    format_document_with_plugins(root, options, output, &Plugins::default())
}

/// Formats an AST as HTML, modified by the given options. Accepts custom plugins.
pub fn format_document_with_plugins(
    root: Node<'_>,
    options: &Options,
    output: &mut dyn Write,
    plugins: &Plugins,
) -> fmt::Result {
    crate::html::require_owned_ast(root)?;
    output.write_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n")?;
    output.write_str("<!DOCTYPE document SYSTEM \"CommonMark.dtd\">\n")?;

    XmlFormatter::new(options, output, plugins).format(root, false)
}

struct XmlFormatter<'o, 'c> {
    output: &'o mut dyn Write,
    options: &'o Options<'c>,
    _plugins: &'o Plugins<'o>,
    indent: u32,
}

impl<'o, 'c> XmlFormatter<'o, 'c> {
    fn new(options: &'o Options<'c>, output: &'o mut dyn Write, plugins: &'o Plugins) -> Self {
        XmlFormatter {
            options,
            output,
            _plugins: plugins,
            indent: 0,
        }
    }

    fn escape(&mut self, buffer: &str) -> fmt::Result {
        crate::html::escape(self.output, buffer)
    }

    fn format(&mut self, node: Node<'_>, plain: bool) -> fmt::Result {
        // Traverse the AST iteratively using a work stack, with pre- and
        // post-child-traversal phases. During pre-order traversal render the
        // opening tags, then push the node back onto the stack for the
        // post-order traversal phase, then push the children in reverse order
        // onto the stack and begin rendering first child.

        enum Phase {
            Pre,
            Post,
        }
        let mut stack = vec![(node, plain, Phase::Pre)];

        while let Some((node, plain, phase)) = stack.pop() {
            match phase {
                Phase::Pre => {
                    let new_plain = if plain {
                        match node.data().value {
                            NodeValue::Text(ref literal) => {
                                self.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                            }
                            NodeValue::Code(NodeCode { ref literal, .. })
                            | NodeValue::HtmlInline(ref literal) => {
                                self.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                            }
                            NodeValue::Raw(ref literal) => {
                                self.escape(literal)?;
                            }
                            NodeValue::LineBreak | NodeValue::SoftBreak => {
                                self.output.write_str(" ")?;
                            }
                            NodeValue::Math(NodeMath { ref literal, .. }) => {
                                self.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                            }
                            _ => (),
                        }
                        plain
                    } else {
                        stack.push((node, false, Phase::Post));
                        self.format_node(node, true)?;
                        false
                    };

                    for ch in node.reverse_children() {
                        stack.push((ch, new_plain, Phase::Pre));
                    }
                }
                Phase::Post => {
                    debug_assert!(!plain);
                    self.format_node(node, false)?;
                }
            }
        }

        Ok(())
    }

    fn indent(&mut self) -> fmt::Result {
        for _ in 0..(cmp::min(self.indent, MAX_INDENT)) {
            self.output.write_str(" ")?;
        }
        Ok(())
    }

    fn format_node(&mut self, node: Node<'_>, entering: bool) -> fmt::Result {
        if !self.options.render.escaped_char_spans && node_matches!(node, NodeValue::Escaped) {
            return Ok(());
        }

        if entering {
            self.indent()?;

            let ast = node.data();

            write!(self.output, "<{}", ast.value.xml_node_name())?;

            if self.options.render.sourcepos && ast.sourcepos.start.line != 0 {
                write!(self.output, " sourcepos=\"{}\"", ast.sourcepos)?;
            }

            let mut was_literal = false;

            match ast.value {
                NodeValue::Document => self
                    .output
                    .write_str(" xmlns=\"http://commonmark.org/xml/1.0\"")?,
                NodeValue::Text(ref literal) => {
                    self.output.write_str(" xml:space=\"preserve\">")?;
                    self.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                    write!(self.output, "</{}", ast.value.xml_node_name())?;
                    was_literal = true;
                }
                NodeValue::Code(NodeCode { ref literal, .. })
                | NodeValue::HtmlBlock(NodeHtmlBlock { ref literal, .. })
                | NodeValue::HtmlInline(ref literal) => {
                    self.output.write_str(" xml:space=\"preserve\">")?;
                    self.escape(literal.owned_str().ok_or(fmt::Error)?)?;
                    write!(self.output, "</{}", ast.value.xml_node_name())?;
                    was_literal = true;
                }
                NodeValue::Raw(ref literal) => {
                    self.output.write_str(" xml:space=\"preserve\">")?;
                    self.escape(literal)?;
                    write!(self.output, "</{}", ast.value.xml_node_name())?;
                    was_literal = true;
                }
                #[cfg(feature = "phoenix_heex")]
                NodeValue::HeexBlock(ref nhb) => {
                    self.output.write_str(" xml:space=\"preserve\">")?;
                    self.escape(&nhb.literal)?;
                    write!(self.output, "</{}", ast.value.xml_node_name())?;
                    was_literal = true;
                }
                #[cfg(feature = "phoenix_heex")]
                NodeValue::HeexInline(ref literal) => {
                    self.output.write_str(" xml:space=\"preserve\">")?;
                    self.escape(literal)?;
                    write!(self.output, "</{}", ast.value.xml_node_name())?;
                    was_literal = true;
                }
                NodeValue::List(ref nl) => {
                    match nl.list_type {
                        ListType::Bullet => {
                            self.output.write_str(" type=\"bullet\"")?;
                        }
                        ListType::Ordered => {
                            write!(
                                self.output,
                                " type=\"ordered\" start=\"{}\" delim=\"{}\"",
                                nl.start,
                                nl.delimiter.xml_name()
                            )?;
                        }
                    }
                    if nl.is_task_list {
                        self.output.write_str(" tasklist=\"true\"")?;
                    }
                    write!(self.output, " tight=\"{}\"", nl.tight)?;
                }
                NodeValue::FrontMatter(_) => (),
                NodeValue::BlockQuote => {}
                NodeValue::MultilineBlockQuote(..) => {}
                NodeValue::Item(..) => {}
                NodeValue::DescriptionList => {}
                NodeValue::DescriptionItem(..) => (),
                NodeValue::DescriptionTerm => {}
                NodeValue::DescriptionDetails => {}
                NodeValue::Heading(ref nh) => {
                    write!(self.output, " level=\"{}\"", nh.level)?;
                }
                NodeValue::CodeBlock(ref ncb) => {
                    let info = ncb.info.owned_str().ok_or(fmt::Error)?;
                    let literal = ncb.literal.owned_str().ok_or(fmt::Error)?;
                    if !info.is_empty() {
                        self.output.write_str(" info=\"")?;
                        self.output.write_str(info)?;
                        self.output.write_str("\"")?;

                        if info.eq("math") {
                            self.output.write_str(" math_style=\"display\"")?;
                        }
                    }
                    self.output.write_str(" xml:space=\"preserve\">")?;
                    self.escape(literal)?;
                    write!(self.output, "</{}", ast.value.xml_node_name())?;
                    was_literal = true;
                }
                NodeValue::ThematicBreak | NodeValue::TableOfContents => {}
                NodeValue::Paragraph => {}
                NodeValue::LineBreak => {}
                NodeValue::SoftBreak => {}
                NodeValue::Strong => {}
                NodeValue::Emph => {}
                NodeValue::Strikethrough => {}
                NodeValue::Highlight => {}
                NodeValue::Insert => {}
                NodeValue::Superscript => {}
                NodeValue::Link(ref nl)
                | NodeValue::Image(ref nl)
                | NodeValue::NoteEmbed(ref nl) => {
                    self.output.write_str(" destination=\"")?;
                    self.escape(nl.url.owned_str().ok_or(fmt::Error)?)?;
                    self.output.write_str("\" title=\"")?;
                    self.escape(nl.title.owned_str().ok_or(fmt::Error)?)?;
                    self.output.write_str("\"")?;
                }
                NodeValue::Table(..) => {
                    // noop
                }
                NodeValue::TableRow(..) => {
                    // noop
                }
                NodeValue::TableCell => {
                    let mut ancestors = node.ancestors().skip(1);

                    let header_row = &ancestors.next().unwrap().data().value;
                    let table = &ancestors.next().unwrap().data().value;

                    if let (NodeValue::TableRow(true), NodeValue::Table(nt)) = (header_row, table) {
                        let ix = node.preceding_siblings().count() - 1;
                        if let Some(xml_align) = nt.alignments[ix].xml_name() {
                            write!(self.output, " align=\"{}\"", xml_align)?;
                        }
                    }
                }
                NodeValue::FootnoteDefinition(ref fd) => {
                    self.output.write_str(" label=\"")?;
                    self.escape(&fd.name)?;
                    self.output.write_str("\"")?;
                }
                NodeValue::FootnoteReference(ref nfr) => {
                    self.output.write_str(" label=\"")?;
                    self.escape(&nfr.name)?;
                    self.output.write_str("\"")?;
                }
                NodeValue::TaskItem(ref nti) => {
                    if nti.symbol.is_some() {
                        self.output.write_str(" completed=\"true\"")?;
                    } else {
                        self.output.write_str(" completed=\"false\"")?;
                    }
                }
                NodeValue::TaskCheckbox(marker) => {
                    self.output.write_str(if marker.checked {
                        " completed=\"true\""
                    } else {
                        " completed=\"false\""
                    })?;
                }
                #[cfg(feature = "shortcodes")]
                NodeValue::ShortCode(ref nsc) => {
                    self.output.write_str(" id=\"")?;
                    self.escape(&nsc.code)?;
                    self.output.write_str("\"")?;
                }
                NodeValue::Escaped => {
                    // noop
                }
                NodeValue::Math(ref math, ..) => {
                    if math.display_math {
                        self.output.write_str(" math_style=\"display\"")?;
                    } else {
                        self.output.write_str(" math_style=\"inline\"")?;
                    }
                    self.output.write_str(" xml:space=\"preserve\">")?;
                    self.escape(math.literal.owned_str().ok_or(fmt::Error)?)?;
                    write!(self.output, "</{}", ast.value.xml_node_name())?;
                    was_literal = true;
                }
                NodeValue::WikiLink(ref nl) => {
                    self.output.write_str(" destination=\"")?;
                    self.escape(&nl.url)?;
                    self.output.write_str("\"")?;
                }
                NodeValue::Underline => {}
                NodeValue::Subscript => {}
                NodeValue::SpoileredText => {}
                NodeValue::EscapedTag(data) => {
                    self.output.write_str(data)?;
                }
                NodeValue::Alert(ref alert) => {
                    self.output.write_str(" type=\"")?;
                    self.output
                        .write_str(&alert.alert_type.default_title().to_lowercase())?;
                    self.output.write_str("\"")?;
                    if let Some(title) = &alert.title {
                        self.output.write_str(" title=\"")?;
                        self.escape(title)?;
                        self.output.write_str("\"")?;
                    }

                    if alert.multiline {
                        self.output.write_str(" multiline=\"true\"")?;
                    }
                }
                NodeValue::Subtext => {}
                NodeValue::BlockDirective(ref nbd) => {
                    self.output.write_str(" info=\"")?;
                    self.escape(&nbd.info)?;
                    self.output.write_str("\"")?;
                }
            }

            if node.first_child().is_some() {
                self.indent += 2;
            } else if !was_literal {
                self.output.write_str(" /")?;
            }
            self.output.write_str(">\n")?;
        } else if node.first_child().is_some() {
            self.indent -= 2;
            self.indent()?;
            writeln!(self.output, "</{}>", node.data().value.xml_node_name())?;
        }
        Ok(())
    }
}
