use super::*;

// A series of tests ensuring the default HTML formatter doesn't panic on
// unusual but still believable ASTs.  We also assert expected panics give at
// least helpful messages.

#[test]
fn test_paragraph_at_root_crash() {
    let options = Options {
        ..Default::default()
    };
    let arena = Arena::new();

    let para = parse_document(&arena, "para", &options)
        .first_child()
        .unwrap();

    // This is the bit we don't expect: what if the paragraph doesn't have a
    // parent at all?  Normally it should have at least a Document.
    para.detach();

    let mut output = String::new();
    html::format_document(para, &options, &mut output).unwrap();
}

#[test]
fn test_empty_table_crash() {
    let options = Options {
        extension: options::Extension {
            table: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let arena = Arena::new();

    let table = parse_document(&arena, "| x |\n| - |\n| z |", &options)
        .first_child()
        .unwrap();

    // What if the table has been emptied of *all* children?
    while let Some(child) = table.first_child() {
        child.detach();
    }

    let mut output = String::new();
    html::format_document(table, &options, &mut output).unwrap();
}

#[test]
#[should_panic(expected = "rendered a table cell without a containing table")]
fn test_table_cell_out_of_water_crash() {
    let options = Options {
        extension: options::Extension {
            table: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let arena = Arena::new();

    let doc = parse_document(&arena, "| x |\n| - |\n| z |", &options);

    let table_row = doc
        .first_child() // table
        .unwrap()
        .last_child() // table row
        .unwrap();

    let table_cell = table_row
        .first_child() // table cell
        .unwrap();

    // What if the table cell has no owning table?
    table_row.detach();

    let mut output = String::new();
    html::format_document(table_cell, &options, &mut output).unwrap();
}

#[test]
#[should_panic(expected = "rendered a table cell without a containing table row")]
fn test_table_cell_out_of_school_crash() {
    let options = Options {
        extension: options::Extension {
            table: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let arena = Arena::new();

    let doc = parse_document(&arena, "| x |\n| - |\n| z |", &options);

    let table_row = doc
        .first_child() // table
        .unwrap()
        .last_child() // table row
        .unwrap();

    let table_cell = table_row
        .first_child() // table cell
        .unwrap();

    // What if the table cell has no owning table row?
    table_cell.detach();

    let mut output = String::new();
    html::format_document(table_cell, &options, &mut output).unwrap();
}

// List of HTML block kinds: https://spec.commonmark.org/0.31.2/#html-blocks
#[test]
fn sourcepos_kind_1() {
    assert_ast_match!(
        [],
        "<script></script>",
        (document (1:1-1:17) [
            (html_block (1:1-1:17) "<script></script>")
        ])
    );

    assert_ast_match!(
        [],
        "<script> </script>",
        (document (1:1-1:18) [
            (html_block (1:1-1:18) "<script> </script>")
        ])
    );

    assert_ast_match!(
        [],
        "<script>A html block</script>",
        (document (1:1-1:29) [
            (html_block (1:1-1:29) "<script>A html block</script>")
        ])
    );

    assert_ast_match!(
        [],
        "<SCRIPT>A html\n\n**block** </Script>\n\n**parsed**",
        (document (1:1-5:10) [
            (html_block (1:1-3:19) "<SCRIPT>A html\n\n**block** </Script>\n")
            (paragraph (5:1-5:10) [
                (strong (5:1-5:10) [
                    (text (5:3-5:8) "parsed")
                ])
            ])
        ])
    );

    assert_ast_match!(
        [],
        "<style>A html block</style>",
        (document (1:1-1:27) [
            (html_block (1:1-1:27) "<style>A html block</style>")
        ])
    );

    assert_ast_match!(
        [],
        "<textarea>A html block</textarea>",
        (document (1:1-1:33) [
            (html_block (1:1-1:33) "<textarea>A html block</textarea>")
        ])
    );

    assert_ast_match!(
        [],
        "<pre>A html block</pre>",
        (document (1:1-1:23) [
            (html_block (1:1-1:23) "<pre>A html block</pre>")
        ])
    );

    assert_ast_match!(
        [],
        "<pre>\nA html block\n</pre>",
        (document (1:1-3:6) [
            (html_block (1:1-3:6) "<pre>\nA html block\n</pre>")
        ])
    );

    assert_ast_match!(
        [],
        "<pre>\n\nA html block\n\n</pre>",
        (document (1:1-5:6) [
            (html_block (1:1-5:6) "<pre>\n\nA html block\n\n</pre>")
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<pre>\nA html block\n</pre>\n\nMore text",
        (document (1:1-7:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-5:6) "<pre>\nA html block\n</pre>\n")
            (paragraph (7:1-7:9) [
                (text (7:1-7:9) "More text")
            ])
        ])
    );
}

#[test]
fn sourcepos_kind_2() {
    assert_ast_match!(
        [],
        "<!---->",
        (document (1:1-1:7) [
            (html_block (1:1-1:7) "<!---->")
        ])
    );

    assert_ast_match!(
        [],
        "<!-- -->",
        (document (1:1-1:8) [
            (html_block (1:1-1:8) "<!-- -->")
        ])
    );

    assert_ast_match!(
        [],
        "<!-- A html block -->",
        (document (1:1-1:21) [
            (html_block (1:1-1:21) "<!-- A html block -->")
        ])
    );

    assert_ast_match!(
        [],
        "<!-- A\nhtml\nblock -->",
        (document (1:1-3:9) [
            (html_block (1:1-3:9) "<!-- A\nhtml\nblock -->")
        ])
    );

    assert_ast_match!(
        [],
        "<!-- A\n\nhtml\n\nblock -->",
        (document (1:1-5:9) [
            (html_block (1:1-5:9) "<!-- A\n\nhtml\n\nblock -->")
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<!-- A\nhtml\nblock -->\n\nMore text",
        (document (1:1-7:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-5:9) "<!-- A\nhtml\nblock -->\n")
            (paragraph (7:1-7:9) [
                (text (7:1-7:9) "More text")
            ])
        ])
    );
}

#[test]
fn sourcepos_kind_3() {
    assert_ast_match!(
        [],
        "<??>",
        (document (1:1-1:4) [
            (html_block (1:1-1:4) "<??>")
        ])
    );

    assert_ast_match!(
        [],
        "<? ?>",
        (document (1:1-1:5) [
            (html_block (1:1-1:5) "<? ?>")
        ])
    );

    assert_ast_match!(
        [],
        "<?html a html block ?>",
        (document (1:1-1:22) [
            (html_block (1:1-1:22) "<?html a html block ?>")
        ])
    );

    assert_ast_match!(
        [],
        "<?html\na html\nblock ?>",
        (document (1:1-3:8) [
            (html_block (1:1-3:8) "<?html\na html\nblock ?>")
        ])
    );

    assert_ast_match!(
        [],
        "<?html\n\na html\n\nblock ?>",
        (document (1:1-5:8) [
            (html_block (1:1-5:8) "<?html\n\na html\n\nblock ?>")
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<?html\na html\nblock ?>\n\nMore text",
        (document (1:1-7:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-5:8) "<?html\na html\nblock ?>\n")
            (paragraph (7:1-7:9) [
                (text (7:1-7:9) "More text")
            ])
        ])
    );
}

#[test]
fn sourcepos_kind_4() {
    assert_ast_match!(
        [],
        "<!a>",
        (document (1:1-1:4) [
            (html_block (1:1-1:4) "<!a>")
        ])
    );

    assert_ast_match!(
        [],
        "<!A>",
        (document (1:1-1:4) [
            (html_block (1:1-1:4) "<!A>")
        ])
    );

    assert_ast_match!(
        [],
        "<!DOCTYPE html>",
        (document (1:1-1:15) [
            (html_block (1:1-1:15) "<!DOCTYPE html>")
        ])
    );

    assert_ast_match!(
        [],
        "<!DOCTYPE\nhtml>",
        (document (1:1-2:5) [
            (html_block (1:1-2:5) "<!DOCTYPE\nhtml>")
        ])
    );

    assert_ast_match!(
        [],
        "<!DOCTYPE\n\nhtml>",
        (document (1:1-3:5) [
            (html_block (1:1-3:5) "<!DOCTYPE\n\nhtml>")
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<!DOCTYPE\nhtml>\n\nMore text",
        (document (1:1-6:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-4:5) "<!DOCTYPE\nhtml>\n")
            (paragraph (6:1-6:9) [
                (text (6:1-6:9) "More text")
            ])
        ])
    );
}

#[test]
fn sourcepos_kind_5() {
    assert_ast_match!(
        [],
        "<![CDATA[]]>",
        (document (1:1-1:12) [
            (html_block (1:1-1:12) "<![CDATA[]]>")
        ])
    );

    assert_ast_match!(
        [],
        "<![CDATA[A html block]]>",
        (document (1:1-1:24) [
            (html_block (1:1-1:24) "<![CDATA[A html block]]>")
        ])
    );

    assert_ast_match!(
        [],
        "<![CDATA[\nA html\nblock]]>",
        (document (1:1-3:8) [
            (html_block (1:1-3:8) "<![CDATA[\nA html\nblock]]>")
        ])
    );

    assert_ast_match!(
        [],
        "<![CDATA[\n\nA html\n\nblock]]>",
        (document (1:1-5:8) [
            (html_block (1:1-5:8) "<![CDATA[\n\nA html\n\nblock]]>")
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<![CDATA[\nA html\nblock]]>\n\nMore text",
        (document (1:1-7:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-5:8) "<![CDATA[\nA html\nblock]]>\n")
            (paragraph (7:1-7:9) [
                (text (7:1-7:9) "More text")
            ])
        ])
    );
}

#[test]
fn sourcepos_kind_6() {
    assert_ast_match!(
        [],
        "<div></div>",
        (document (1:1-1:11) [
            (html_block (1:1-1:11) "<div></div>")
        ])
    );

    assert_ast_match!(
        [],
        "<div>A html block</div>",
        (document (1:1-1:23) [
            (html_block (1:1-1:23) "<div>A html block</div>")
        ])
    );

    assert_ast_match!(
        [],
        "<div>A html block\n</div>",
        (document (1:1-2:6) [
            (html_block (1:1-2:6) "<div>A html block\n</div>")
        ])
    );

    assert_ast_match!(
        [],
        "<div>\nA html block</div>",
        (document (1:1-2:18) [
            (html_block (1:1-2:18) "<div>\nA html block</div>")
        ])
    );

    assert_ast_match!(
        [],
        "<div>\nA html block\n</div>",
        (document (1:1-3:6) [
            (html_block (1:1-3:6) "<div>\nA html block\n</div>")
        ])
    );

    assert_ast_match!(
        [],
        "<div>\nA html block\n\n</div>",
        (document (1:1-4:6) [
            (html_block (1:1-2:12) "<div>\nA html block\n")
            (html_block (4:1-4:6) "</div>")
        ])
    );

    assert_ast_match!(
        [],
        "<div>\n\nA html block\n</div>",
        (document (1:1-4:6) [
            (html_block (1:1-1:5) "<div>\n")
            (paragraph (3:1-3:12) [
                (text (3:1-3:12) "A html block")
            ])
            (html_block (4:1-4:6) "</div>")
        ])
    );

    assert_ast_match!(
        [],
        "<div>\n\nA html block\n\n</div>",
        (document (1:1-5:6) [
            (html_block (1:1-1:5) "<div>\n")
            (paragraph (3:1-3:12) [
                (text (3:1-3:12) "A html block")
            ])
            (html_block (5:1-5:6) "</div>")
        ])
    );

    assert_ast_match!(
        [],
        "Test\n<div>\nA html block\n</div>\n\nMore text",
        (document (1:1-6:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (2:1-4:6) "<div>\nA html block\n</div>\n")
            (paragraph (6:1-6:9) [
                (text (6:1-6:9) "More text")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<div>\nA html block\n</div>\n\nMore text",
        (document (1:1-7:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-5:6) "<div>\nA html block\n</div>\n")
            (paragraph (7:1-7:9) [
                (text (7:1-7:9) "More text")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<div>\nA html block\n\n</div>\n\nMore text",
        (document (1:1-8:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-4:12) "<div>\nA html block\n")
            (html_block (6:1-6:6) "</div>\n")
            (paragraph (8:1-8:9) [
                (text (8:1-8:9) "More text")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<div>\n\nA html block\n</div>\n\nMore text",
        (document (1:1-8:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-3:5) "<div>\n")
            (paragraph (5:1-5:12) [
                (text (5:1-5:12) "A html block")
            ])
            (html_block (6:1-6:6) "</div>\n")
            (paragraph (8:1-8:9) [
                (text (8:1-8:9) "More text")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<div>\n\nA html block\n\n</div>\n\nMore text",
        (document (1:1-9:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-3:5) "<div>\n")
            (paragraph (5:1-5:12) [
                (text (5:1-5:12) "A html block")
            ])
            (html_block (7:1-7:6) "</div>\n")
            (paragraph (9:1-9:9) [
                (text (9:1-9:9) "More text")
            ])
        ])
    );
}

#[test]
fn sourcepos_kind_7() {
    assert_ast_match!(
        [],
        "<testa></testa>",
        (document (1:1-1:15) [
            (paragraph (1:1-1:15) [
                (html_inline (1:1-1:7) "<testa>")
                (html_inline (1:8-1:15) "</testa>")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "<testb>\n</testb>",
        (document (1:1-2:8) [
            (html_block (1:1-2:8) "<testb>\n</testb>")
        ])
    );

    assert_ast_match!(
        [],
        "<testc>\n\n</testc>",
        (document (1:1-3:8) [
            (html_block (1:1-1:7) "<testc>\n")
            (html_block (3:1-3:8) "</testc>")
        ])
    );

    assert_ast_match!(
        [],
        "<test>\nA html block\n</test>",
        (document (1:1-3:7) [
            (html_block (1:1-3:7) "<test>\nA html block\n</test>")
        ])
    );

    assert_ast_match!(
        [],
        "<test>\nB html block\n\n</test>",
        (document (1:1-4:7) [
            (html_block (1:1-2:12) "<test>\nB html block\n")
            (html_block (4:1-4:7) "</test>")
        ])
    );

    assert_ast_match!(
        [],
        "<test>\n\nC html block\n</test>",
        (document (1:1-4:7) [
            (html_block (1:1-1:6) "<test>\n")
            (paragraph (3:1-4:7) [
                (text (3:1-3:12) "C html block")
                (softbreak (3:13-3:13))
                (html_inline (4:1-4:7) "</test>")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "<test>\n\nD html block\n\n</test>",
        (document (1:1-5:7) [
            (html_block (1:1-1:6) "<test>\n")
            (paragraph (3:1-3:12) [
                (text (3:1-3:12) "D html block")
            ])
            (html_block (5:1-5:7) "</test>")
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<test>\nE html block\n</test>\n\nMore text",
        (document (1:1-7:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-5:7) "<test>\nE html block\n</test>\n")
            (paragraph (7:1-7:9) [
                (text (7:1-7:9) "More text")
            ])
        ])
    );

    assert_ast_match!(
        [],
        "Test\n\n<test>\n\nF html block\n\n</test>\n\nMore text",
        (document (1:1-9:9) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "Test")
            ])
            (html_block (3:1-3:6) "<test>\n")
            (paragraph (5:1-5:12) [
                (text (5:1-5:12) "F html block")
            ])
            (html_block (7:1-7:7) "</test>\n")
            (paragraph (9:1-9:9) [
                (text (9:1-9:9) "More text")
            ])
        ])
    );
}

#[test]
fn sourcepos_inside_blockquote() {
    // Kind 1
    assert_ast_match!(
        [],
        "> <script></script>",
        (document (1:1-1:19) [
            (block_quote (1:1-1:19) [
                (html_block (1:3-1:19) "<script></script>")
            ])
        ])
    );

    // Kind 2
    assert_ast_match!(
        [],
        "> <!---->",
        (document (1:1-1:9) [
            (block_quote (1:1-1:9) [
                (html_block (1:3-1:9) "<!---->")
            ])
        ])
    );

    // Kind 3
    assert_ast_match!(
        [],
        "> <??>",
        (document (1:1-1:6) [
            (block_quote (1:1-1:6) [
                (html_block (1:3-1:6) "<??>")
            ])
        ])
    );

    // Kind 4
    assert_ast_match!(
        [],
        "> <!a>",
        (document (1:1-1:6) [
            (block_quote (1:1-1:6) [
                (html_block (1:3-1:6) "<!a>")
            ])
        ])
    );

    // Kind 5
    assert_ast_match!(
        [],
        "> <![CDATA[]]>",
        (document (1:1-1:14) [
            (block_quote (1:1-1:14) [
                (html_block (1:3-1:14) "<![CDATA[]]>")
            ])
        ])
    );

    // Kind 6
    assert_ast_match!(
        [],
        "> <div></div>",
        (document (1:1-1:13) [
            (block_quote (1:1-1:13) [
                (html_block (1:3-1:13) "<div></div>")
            ])
        ])
    );

    // Kind 7
    assert_ast_match!(
        [],
        "> <testa></testa>",
        (document (1:1-1:17) [
            (block_quote (1:1-1:17) [
                (paragraph (1:3-1:17) [
                    (html_inline (1:3-1:9) "<testa>")
                    (html_inline (1:10-1:17) "</testa>")
                ])
            ])
        ])
    );
}
