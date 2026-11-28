// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// The original Comrak copyright and BSD license in COPYING remain applicable.

use comrak::html::ChildRendering;
use comrak::{create_formatter, nodes::NodeValue};
use std::fmt::Write;

create_formatter!(CustomFormatter<usize>, {
    NodeValue::Emph => |context, entering| {
        context.user += 1;
        if entering {
            context.write_str("<i>")?;
        } else {
            context.write_str("</i>")?;
        }
    },
    NodeValue::Strong => |context, entering| {
        context.user += 1;
        context.write_str(if entering { "<b>" } else { "</b>" })?;
    },
    NodeValue::Image(ref nl) => |context, node, entering| {
        assert!(node.data().sourcepos == (3, 1, 3, 18).into());
        if entering {
            context.write_str(&nl.url.owned_str().expect("legacy example requires Owned AST").to_uppercase())?;
        }
        return Ok(ChildRendering::Skip);
    },
});

fn main() {
    use comrak::{Arena, Options, parse_document};

    let options = Options::default();
    let arena = Arena::new();
    let doc = parse_document(
        &arena,
        "_Hello_, **world**.\n\n![title](/img.png)",
        &options,
    );

    let mut out = String::new();
    let converted_count = CustomFormatter::format_document(doc, &options, &mut out, 0).unwrap();

    assert_eq!(out, "<p><i>Hello</i>, <b>world</b>.</p>\n<p>/IMG.PNG</p>\n");

    assert_eq!(converted_count, 4);
}
