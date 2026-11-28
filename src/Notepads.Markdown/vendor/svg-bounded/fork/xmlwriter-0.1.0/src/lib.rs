// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
/*!
A simple, streaming, partially-validating XML writer that writes XML data into an internal buffer.

## Features

- A simple, bare-minimum, panic-based API.
- Non-allocating API. All methods are accepting either `fmt::Display` or `fmt::Arguments`.
- Nodes auto-closing.

## Example

```rust
use xmlwriter::*;

let opt = Options {
    use_single_quote: true,
    ..Options::default()
};

let mut w = XmlWriter::new(opt);
w.start_element("svg");
w.write_attribute("xmlns", "http://www.w3.org/2000/svg");
w.write_attribute_fmt("viewBox", format_args!("{} {} {} {}", 0, 0, 128, 128));
w.start_element("text");
// We can write any object that implements `fmt::Display`.
w.write_attribute("x", &10);
w.write_attribute("y", &20);
w.write_text_fmt(format_args!("length is {}", 5));

assert_eq!(w.end_document(),
"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 128 128'>
    <text x='10' y='20'>
        length is 5
    </text>
</svg>
");
```
*/

#![doc(html_root_url = "https://docs.rs/xmlwriter/0.1.0")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(missing_copy_implementations)]

use std::fmt::{self, Display};
use std::ops::Range;

/// An XML node indention.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Indent {
    /// Disable indention and new lines.
    None,
    /// Indent with spaces. Preferred range is 0..4.
    Spaces(u8),
    /// Indent with tabs.
    Tabs,
}

/// An XML writing options.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Use single quote marks instead of double quote.
    ///
    /// # Examples
    ///
    /// Before:
    ///
    /// ```text
    /// <rect fill="red"/>
    /// ```
    ///
    /// After:
    ///
    /// ```text
    /// <rect fill='red'/>
    /// ```
    ///
    /// Default: disabled
    pub use_single_quote: bool,

    /// Set XML nodes indention.
    ///
    /// # Examples
    ///
    /// `Indent::None`
    /// Before:
    ///
    /// ```text
    /// <svg>
    ///     <rect fill="red"/>
    /// </svg>
    /// ```
    ///
    /// After:
    ///
    /// ```text
    /// <svg><rect fill="red"/></svg>
    /// ```
    ///
    /// Default: 4 spaces
    pub indent: Indent,

    /// Set XML attributes indention.
    ///
    /// # Examples
    ///
    /// `Indent::Spaces(2)`
    ///
    /// Before:
    ///
    /// ```text
    /// <svg>
    ///     <rect fill="red" stroke="black"/>
    /// </svg>
    /// ```
    ///
    /// After:
    ///
    /// ```text
    /// <svg>
    ///     <rect
    ///       fill="red"
    ///       stroke="black"/>
    /// </svg>
    /// ```
    ///
    /// Default: `None`
    pub attributes_indent: Indent,
}

impl Default for Options {
    #[inline]
    fn default() -> Self {
        Options {
            use_single_quote: false,
            indent: Indent::Spaces(4),
            attributes_indent: Indent::None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Empty,
    Document,
    Attributes,
}

struct DepthData {
    range: Range<usize>,
    has_children: bool,
}

mod engine;
mod request;
mod storage;

use crate::engine::WriterCore;
pub use crate::request::RequestWriter;
use crate::storage::{plain, Plain, PlainBytes};

/// An XML writer.
pub struct XmlWriter {
    core: WriterCore<Plain>,
}

impl XmlWriter {
    /// Creates a new `XmlWriter`.
    #[inline]
    pub fn new(opt: Options) -> Self {
        Self {
            core: WriterCore::new(Plain, PlainBytes(Vec::new()), opt),
        }
    }

    /// Creates a new `XmlWriter` with a specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize, opt: Options) -> Self {
        Self {
            core: WriterCore::new(Plain, PlainBytes(Vec::with_capacity(capacity)), opt),
        }
    }

    /// Writes an XML declaration.
    ///
    /// `<?xml version="1.0" encoding="UTF-8" standalone="no"?>`
    ///
    /// # Panics
    ///
    /// - When called twice.
    #[inline(never)]
    pub fn write_declaration(&mut self) {
        plain(self.core.write_declaration());
    }

    /// Writes a comment string.
    pub fn write_comment(&mut self, text: &str) {
        plain(self.core.write_comment_fmt(format_args!("{}", text)));
    }

    /// Writes a formatted comment.
    #[inline(never)]
    pub fn write_comment_fmt(&mut self, fmt: fmt::Arguments) {
        plain(self.core.write_comment_fmt(fmt));
    }

    /// Starts writing a new element.
    ///
    /// This method writes only the `<tag-name` part.
    #[inline(never)]
    pub fn start_element(&mut self, name: &str) {
        plain(self.core.start_element(name));
    }

    /// Writes an attribute.
    ///
    /// Quotes in the value will be escaped.
    ///
    /// # Panics
    ///
    /// - When called before `start_element()`.
    /// - When called after `close_element()`.
    ///
    /// # Example
    ///
    /// ```
    /// use xmlwriter::*;
    ///
    /// let mut w = XmlWriter::new(Options::default());
    /// w.start_element("svg");
    /// w.write_attribute("x", "5");
    /// w.write_attribute("y", &5);
    /// assert_eq!(w.end_document(), "<svg x=\"5\" y=\"5\"/>\n");
    /// ```
    pub fn write_attribute<V: Display + ?Sized>(&mut self, name: &str, value: &V) {
        plain(
            self.core
                .write_attribute_fmt(name, format_args!("{}", value)),
        );
    }

    /// Writes a formatted attribute value.
    ///
    /// Quotes in the value will be escaped.
    ///
    /// # Panics
    ///
    /// - When called before `start_element()`.
    /// - When called after `close_element()`.
    ///
    /// # Example
    ///
    /// ```
    /// use xmlwriter::*;
    ///
    /// let mut w = XmlWriter::new(Options::default());
    /// w.start_element("rect");
    /// w.write_attribute_fmt("fill", format_args!("url(#{})", "gradient"));
    /// assert_eq!(w.end_document(), "<rect fill=\"url(#gradient)\"/>\n");
    /// ```
    #[inline(never)]
    pub fn write_attribute_fmt(&mut self, name: &str, fmt: fmt::Arguments) {
        plain(self.core.write_attribute_fmt(name, fmt));
    }

    /// Writes a raw attribute value.
    ///
    /// Closure provides a mutable reference to an internal buffer.
    ///
    /// **Warning:** this method is an escape hatch for cases when you need to write
    /// a lot of data very fast.
    ///
    /// # Panics
    ///
    /// - When called before `start_element()`.
    /// - When called after `close_element()`.
    ///
    /// # Example
    ///
    /// ```
    /// use xmlwriter::*;
    ///
    /// let mut w = XmlWriter::new(Options::default());
    /// w.start_element("path");
    /// w.write_attribute_raw("d", |buf| buf.extend_from_slice(b"M 10 20 L 30 40"));
    /// assert_eq!(w.end_document(), "<path d=\"M 10 20 L 30 40\"/>\n");
    /// ```
    #[inline(never)]
    pub fn write_attribute_raw<F>(&mut self, name: &str, f: F)
    where
        F: FnOnce(&mut Vec<u8>),
    {
        self.core.write_attribute_raw(name, f);
    }

    /// Sets the preserve whitespaces flag.
    ///
    /// - If set, text nodes will be written as is.
    /// - If not set, text nodes will be indented.
    ///
    /// Can be set at any moment.
    ///
    /// # Example
    ///
    /// ```
    /// use xmlwriter::*;
    ///
    /// let mut w = XmlWriter::new(Options::default());
    /// w.start_element("html");
    /// w.start_element("p");
    /// w.write_text("text");
    /// w.end_element();
    /// w.start_element("p");
    /// w.set_preserve_whitespaces(true);
    /// w.write_text("text");
    /// w.end_element();
    /// w.set_preserve_whitespaces(false);
    /// assert_eq!(w.end_document(),
    /// "<html>
    ///     <p>
    ///         text
    ///     </p>
    ///     <p>text</p>
    /// </html>
    /// ");
    /// ```
    pub fn set_preserve_whitespaces(&mut self, preserve: bool) {
        self.core.set_preserve_whitespaces(preserve);
    }

    /// Writes a text node.
    ///
    /// See `write_text_fmt()` for details.
    pub fn write_text(&mut self, text: &str) {
        plain(self.core.write_text_fmt(format_args!("{}", text)));
    }

    /// Writes a formatted text node.
    ///
    /// `<` will be escaped.
    ///
    /// # Panics
    ///
    /// - When called not after `start_element()`.
    #[inline(never)]
    pub fn write_text_fmt(&mut self, fmt: fmt::Arguments) {
        plain(self.core.write_text_fmt(fmt));
    }

    /// Closes an open element.
    #[inline(never)]
    pub fn end_element(&mut self) {
        plain(self.core.end_element());
    }

    /// Closes all open elements and returns an internal XML buffer.
    ///
    /// # Example
    ///
    /// ```
    /// use xmlwriter::*;
    ///
    /// let mut w = XmlWriter::new(Options::default());
    /// w.start_element("svg");
    /// w.start_element("g");
    /// w.start_element("rect");
    /// assert_eq!(w.end_document(),
    /// "<svg>
    ///     <g>
    ///         <rect/>
    ///     </g>
    /// </svg>
    /// ");
    /// ```
    pub fn end_document(mut self) -> String {
        plain(self.core.finish());
        String::from_utf8(self.core.buf.0).unwrap()
    }
}
