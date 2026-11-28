use comrak::{
    Options,
    adapters::{HeadingAdapter, HeadingMeta},
    markdown_to_html_with_plugins,
    nodes::Sourcepos,
    options::Plugins,
};
use std::fmt::{self, Write};

fn main() {
    let adapter = CustomHeadingAdapter;
    let mut options = Options::default();
    let mut plugins = Plugins::default();
    plugins.render.heading_adapter = Some(&adapter);

    print_html(
        "Some text.\n\n## Please hide me from search\n\nSome other text",
        &options,
        &plugins,
    );
    print_html(
        "Some text.\n\n### Here is some `code`\n\nSome other text",
        &options,
        &plugins,
    );
    print_html(
        "Some text.\n\n### Here is some **bold** text and some *italicized* text\n\nSome other text",
        &options,
        &plugins,
    );
    options.render.sourcepos = true;
    print_html("# Here is a [link](/)", &options, &plugins);
}

struct CustomHeadingAdapter;

impl HeadingAdapter for CustomHeadingAdapter {
    fn enter(
        &self,
        output: &mut dyn Write,
        heading: &HeadingMeta,
        sourcepos: Option<Sourcepos>,
    ) -> fmt::Result {
        let id = slug::slugify(&heading.content);

        let search_include = !&heading.content.contains("hide");

        write!(output, "<h{}", heading.level)?;

        if let Some(sourcepos) = sourcepos {
            write!(output, " data-sourcepos=\"{}\"", sourcepos)?;
        }

        write!(
            output,
            " id=\"{}\" data-search-include=\"{}\">",
            id, search_include
        )
    }

    fn exit(&self, output: &mut dyn Write, heading: &HeadingMeta) -> fmt::Result {
        write!(output, "</h{}>", heading.level)
    }
}

fn print_html(document: &str, options: &Options, plugins: &Plugins) {
    let html = markdown_to_html_with_plugins(document, options, plugins);
    println!("{}", html);
}
