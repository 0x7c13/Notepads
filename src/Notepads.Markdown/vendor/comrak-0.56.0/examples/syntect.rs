//! This example shows how to use the bundled syntect plugin.

use comrak::plugins::syntect::SyntectAdapterBuilder;
use comrak::{Options, markdown_to_html_with_plugins, options};

fn main() {
    run_with(SyntectAdapterBuilder::new().theme("base16-ocean.dark"));
    run_with(SyntectAdapterBuilder::new().css());
}

fn run_with(builder: SyntectAdapterBuilder) {
    let adapter = builder.build();
    let options = Options::default();
    let mut plugins = options::Plugins::default();

    plugins.render.codefence_syntax_highlighter = Some(&adapter);

    let input = concat!("```Rust\n", "fn main<'a>();\n", "```");

    let formatted = markdown_to_html_with_plugins(input, &options, &plugins);

    println!("{}", formatted);
}
