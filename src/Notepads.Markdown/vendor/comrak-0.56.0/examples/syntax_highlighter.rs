//! This example shows how to implement a syntax highlighter plugin.

use comrak::adapters::SyntaxHighlighterAdapter;
use comrak::{Options, markdown_to_html_with_plugins, options};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write};

#[derive(Debug, Copy, Clone)]
pub struct PotatoSyntaxAdapter {
    potato_size: i32,
}

impl PotatoSyntaxAdapter {
    pub fn new(potato_size: i32) -> Self {
        PotatoSyntaxAdapter { potato_size }
    }
}

impl SyntaxHighlighterAdapter for PotatoSyntaxAdapter {
    fn write_highlighted(
        &self,
        output: &mut dyn Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        write!(
            output,
            "<span class=\"potato-{}\">{}</span><span class=\"size-{}\">potato</span>",
            lang.unwrap(),
            code,
            self.potato_size
        )
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn Write,
        attributes: HashMap<&'static str, Cow<str>>,
    ) -> fmt::Result {
        if attributes.contains_key("lang") {
            write!(output, "<pre lang=\"{}\">", attributes["lang"])
        } else {
            output.write_str("<pre>")
        }
    }

    fn write_code_tag(
        &self,
        output: &mut dyn Write,
        attributes: HashMap<&'static str, Cow<str>>,
    ) -> fmt::Result {
        if attributes.contains_key("class") {
            write!(output, "<code class=\"{}\">", attributes["class"])
        } else {
            output.write_str("<code>")
        }
    }
}

fn main() {
    let adapter = PotatoSyntaxAdapter::new(42);
    let options = Options::default();
    let mut plugins = options::Plugins::default();

    plugins.render.codefence_syntax_highlighter = Some(&adapter);

    let input = concat!("```Rust\n", "fn main<'a>();\n", "```");

    let formatted = markdown_to_html_with_plugins(input, &options, &plugins);

    println!("{}", formatted);
}
