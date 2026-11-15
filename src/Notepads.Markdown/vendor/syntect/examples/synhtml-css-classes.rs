//! Generates highlighted HTML with CSS classes for a Rust and a C++ source.
//! Run with ```cargo run --example synhtml-css-classes```
//!
//! will generate 4 files as usage example
//! * synhtml-css-classes.html
//! * synhtml-css-classes.css
//! * theme-dark.css
//! * theme-light.css
//!
//! You can open the html with a web browser and change between light and dark
//! mode.
use syntect::highlighting::ThemeSet;
use syntect::html::css_for_theme_with_class_style;
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

fn main() -> Result<(), std::io::Error> {
    // ---------------------------------------------------------------------------------------------
    // generate html
    let ss = SyntaxSet::load_defaults_newlines();

    let html_file = File::create(Path::new("synhtml-css-classes.html"))?;
    let mut html = BufWriter::new(&html_file);

    // write html header
    writeln!(html, "<!DOCTYPE html>")?;
    writeln!(html, "<html>")?;
    writeln!(html, "  <head>")?;
    writeln!(html, "    <title>synhtml-css-classes.rs</title>")?;
    writeln!(
        html,
        "    <link rel=\"stylesheet\" href=\"synhtml-css-classes.css\">"
    )?;
    writeln!(html, "  </head>")?;
    writeln!(html, "  <body>")?;

    // Rust
    let code_rs = "// Rust source
fn main() {
    println!(\"Hello World!\");
}";

    let sr_rs = ss.find_syntax_by_extension("rs").unwrap();
    let mut rs_html_generator =
        ClassedHTMLGenerator::new_with_class_style(sr_rs, &ss, ClassStyle::Spaced);
    for line in LinesWithEndings::from(code_rs) {
        rs_html_generator
            .parse_html_for_line_which_includes_newline(line)
            .unwrap();
    }
    let html_rs = rs_html_generator.finalize();

    writeln!(html, "<pre class=\"code\">")?;
    writeln!(html, "{}", html_rs)?;
    writeln!(html, "</pre>")?;

    // C++
    let code_cpp = "/* C++ source */
#include <iostream>
int main() {
    std::cout << \"Hello World!\" << std::endl;
}";

    let sr_cpp = ss.find_syntax_by_extension("cpp").unwrap();
    let mut cpp_html_generator =
        ClassedHTMLGenerator::new_with_class_style(sr_cpp, &ss, ClassStyle::Spaced);
    for line in LinesWithEndings::from(code_cpp) {
        cpp_html_generator
            .parse_html_for_line_which_includes_newline(line)
            .unwrap();
    }
    let html_cpp = cpp_html_generator.finalize();

    writeln!(html, "<pre class=\"code\">")?;
    writeln!(html, "{}", html_cpp)?;
    writeln!(html, "</pre>")?;

    // write html end
    writeln!(html, "  </body>")?;
    writeln!(html, "</html>")?;

    // ---------------------------------------------------------------------------------------------
    // generate css
    let css = "@import url(\"theme-light.css\") (prefers-color-scheme: light);
    @import url(\"theme-dark.css\") (prefers-color-scheme: dark);

    @media (prefers-color-scheme: dark) {
      body {
        background-color: gray;
      }
    }
    @media (prefers-color-scheme: light) {
      body {
        background-color: lightgray;
      }
    }";

    let css_file = File::create(Path::new("synhtml-css-classes.css"))?;
    let mut css_writer = BufWriter::new(&css_file);

    writeln!(css_writer, "{}", css)?;

    // ---------------------------------------------------------------------------------------------
    // generate css files for themes
    let ts = ThemeSet::load_defaults();

    // create dark color scheme css
    let dark_theme = &ts.themes["Solarized (dark)"];
    let css_dark_file = File::create(Path::new("theme-dark.css"))?;
    let mut css_dark_writer = BufWriter::new(&css_dark_file);

    let css_dark = css_for_theme_with_class_style(dark_theme, ClassStyle::Spaced).unwrap();
    writeln!(css_dark_writer, "{}", css_dark)?;

    // create light color scheme css
    let light_theme = &ts.themes["Solarized (light)"];
    let css_light_file = File::create(Path::new("theme-light.css"))?;
    let mut css_light_writer = BufWriter::new(&css_light_file);

    let css_light = css_for_theme_with_class_style(light_theme, ClassStyle::Spaced).unwrap();
    writeln!(css_light_writer, "{}", css_light)?;

    Ok(())
}
