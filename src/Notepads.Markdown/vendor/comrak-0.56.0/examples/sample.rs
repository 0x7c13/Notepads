// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// The original Comrak copyright and BSD license in COPYING remain applicable.

// Samples used in the README.  Wanna make sure they work as advertised.

fn small() {
    use comrak::{Options, markdown_to_html};

    assert_eq!(
        markdown_to_html("¡Olá, **世界**!", &Options::default()),
        "<p>¡Olá, <strong>世界</strong>!</p>\n"
    );
}

fn large() {
    use comrak::nodes::NodeValue;
    use comrak::{Arena, Options, format_html, parse_document};

    fn replace_text(document: &str, orig_string: &str, replacement: &str) -> String {
        // The returned nodes are created in the supplied Arena, and are bound by its lifetime.
        let arena = Arena::new();

        // Parse the document into a root `Node`
        let root = parse_document(&arena, document, &Options::default());

        // Iterate over all the descendants of root.
        for node in root.descendants() {
            if let NodeValue::Text(ref mut text) = node.data_mut().value {
                // If the node is a text node, perform the string replacement.
                *text = text
                    .owned_str()
                    .expect("legacy example requires Owned AST")
                    .replace(orig_string, replacement)
                    .into()
            }
        }

        let mut html = String::new();
        format_html(root, &Options::default(), &mut html).unwrap();

        html
    }

    fn main() {
        let doc = "Hello, pretty world!\n\n1. Do you like [pretty](#) paintings?\n2. Or *pretty* music?\n";
        let orig = "pretty";
        let repl = "beautiful";
        let html = replace_text(doc, orig, repl);

        println!("{}", html);
        // Output:
        //
        // <p>Hello, beautiful world!</p>
        // <ol>
        // <li>Do you like <a href="#">beautiful</a> paintings?</li>
        // <li>Or <em>beautiful</em> music?</li>
        // </ol>
    }

    main()
}

fn main() {
    small();
    large();
}
