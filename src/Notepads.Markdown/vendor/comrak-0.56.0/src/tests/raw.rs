use crate::nodes::{Ast, NodeValue};

use super::*;

#[test]
fn raw_node() {
    let user_input = "User input: <iframe></iframe>";
    let system_input_inline = "System Inline input: <iframe></iframe>";
    let system_input_block = "System Block input: <iframe></iframe>";
    let input = user_input.to_owned() + system_input_inline + "\n\n" + system_input_block + "\n";

    let mut options = Options::default();
    options.render.escape = true;
    options.render.r#unsafe = false;

    let arena = Arena::new();
    let root = parse_document(&arena, user_input, &options);
    let raw_ast_inline = Ast::new(
        NodeValue::Raw(system_input_inline.to_string()),
        (0, 0).into(),
    );
    let raw_node_inline = arena.alloc(raw_ast_inline.into());
    root.first_child()
        .unwrap()
        .last_child()
        .unwrap()
        .insert_after(raw_node_inline);
    let raw_ast_block = Ast::new(
        NodeValue::Raw(system_input_block.to_string()),
        (0, 0).into(),
    );
    let raw_node_block = arena.alloc(raw_ast_block.into());
    root.first_child().unwrap().insert_after(raw_node_block);

    let mut output = String::new();
    html::format_document(root, &options, &mut output).unwrap();
    compare_strs(
        &output,
        concat!(
            "<p>User input: &lt;iframe&gt;&lt;/iframe&gt;",
            "System Inline input: <iframe></iframe></p>\n",
            "System Block input: <iframe></iframe>"
        ),
        "html",
        &input,
    );

    let mut md = String::new();
    cm::format_document_with_plugins(root, &options, &mut md, &options::Plugins::default())
        .unwrap();
    compare_strs(&md, &input, "cm", &input);

    let mut xml = String::new();
    crate::xml::format_document(root, &options, &mut xml).unwrap();
    compare_strs(
        &xml,
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE document SYSTEM \"CommonMark.dtd\">\n",
            "<document xmlns=\"http://commonmark.org/xml/1.0\">\n",
            "  <paragraph>\n",
            "    <text xml:space=\"preserve\">User input: </text>\n",
            "    <html_inline xml:space=\"preserve\">&lt;iframe&gt;</html_inline>\n",
            "    <html_inline xml:space=\"preserve\">&lt;/iframe&gt;</html_inline>\n",
            "    <raw xml:space=\"preserve\">System Inline input: &lt;iframe&gt;&lt;/iframe&gt;</raw>\n",
            "  </paragraph>\n",
            "  <raw xml:space=\"preserve\">System Block input: &lt;iframe&gt;&lt;/iframe&gt;</raw>\n</document>\n"
        ),
        "xml",
        &input,
    );
}
