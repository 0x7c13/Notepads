// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// The original Comrak copyright and BSD license in COPYING remain applicable.

// Update the "comrak --help" text in Comrak's own README.

use std::error::Error;
use std::fmt::Write;
use std::str;
use toml::Table;

use comrak::nodes::NodeValue;
use comrak::{Arena, Options, format_commonmark, parse_document};

const DEPENDENCIES: &str = "[dependencies]\ncomrak = ";
const HELP: &str = "$ comrak --help\n";
const HELP_START: &str =
    "A 100% CommonMark-compatible GitHub Flavored Markdown parser and formatter\n";

fn main() -> Result<(), Box<dyn Error>> {
    let arena = Arena::new();

    let readme = std::fs::read_to_string("README.md")?;
    let doc = parse_document(&arena, &readme, &Options::default());

    let cargo_toml = std::fs::read_to_string("Cargo.toml")?.parse::<Table>()?;
    let msrv = cargo_toml["package"].as_table().unwrap()["rust-version"]
        .as_str()
        .unwrap();

    let mut in_msrv = false;
    let mut next_block_is_help_body = false;

    for node in doc.descendants() {
        match node.data_mut().value {
            NodeValue::CodeBlock(ref mut ncb) => {
                // Look for the Cargo.toml example block.
                if ncb
                    .info
                    .owned_str()
                    .expect("legacy example requires Owned AST")
                    == "toml"
                    && ncb
                        .literal
                        .owned_str()
                        .expect("legacy example requires Owned AST")
                        .starts_with(DEPENDENCIES)
                {
                    let mut content = DEPENDENCIES.to_string();
                    let mut version_parts = comrak::version().split('.').collect::<Vec<&str>>();
                    version_parts.pop();
                    write!(content, "\"{}\"", version_parts.join(".")).unwrap();
                    ncb.literal = content.into();
                    continue;
                }

                // Look for a console code block whose contents starts with the HELP string.
                // The *next* code block contains our help, minus the starting string.
                if ncb
                    .info
                    .owned_str()
                    .expect("legacy example requires Owned AST")
                    == "console"
                    && ncb
                        .literal
                        .owned_str()
                        .expect("legacy example requires Owned AST")
                        .starts_with(HELP)
                {
                    next_block_is_help_body = true;
                    continue;
                }

                if next_block_is_help_body {
                    next_block_is_help_body = false;
                    assert!(
                        ncb.info
                            .owned_str()
                            .expect("legacy example requires Owned AST")
                            .is_empty()
                            && ncb
                                .literal
                                .owned_str()
                                .expect("legacy example requires Owned AST")
                                .starts_with(HELP_START)
                    );
                    let mut content = String::new();
                    let mut cmd = std::process::Command::new("cargo");
                    content.push_str(
                        str::from_utf8(
                            &cmd.args(["run", "--all-features", "--", "--help"])
                                .output()
                                .unwrap()
                                .stdout,
                        )
                        .unwrap(),
                    );
                    ncb.literal = content.into();
                    continue;
                }
            }
            NodeValue::HtmlInline(ref mut s) => {
                if s.owned_str().expect("legacy example requires Owned AST")
                    == "<span class=\"msrv\">"
                {
                    in_msrv = true;
                } else if in_msrv
                    && s.owned_str().expect("legacy example requires Owned AST") == "</span>"
                {
                    in_msrv = false;
                }
            }
            NodeValue::Text(ref mut t) => {
                if in_msrv {
                    *t = msrv.to_string().into();
                }
            }
            _ => {}
        }
    }

    let mut options = Options::default();
    options.render.prefer_fenced = true;
    options.render.experimental_minimize_commonmark = true;

    let mut out = String::new();
    format_commonmark(doc, &options, &mut out)?;
    std::fs::write("README.md", &out)?;

    // Strip everything except the "Contributing" section.

    let mut in_contributing = false;

    for node in doc.children() {
        if let NodeValue::Heading(ref nh) = node.data().value {
            if !in_contributing {
                in_contributing = nh.level == 2 && node.collect_text() == "Contributing";
                node.detach();
                continue;
            } else if nh.level <= 2 {
                in_contributing = false;
            }
        }

        if !in_contributing {
            node.detach();
        }
    }

    out.clear();
    writeln!(
        out,
        "<!-- This file is generated by examples/update-readme.rs. -->"
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "The following is included verbatim from [README.md](README.md)."
    )?;
    writeln!(out)?;
    writeln!(out, "---")?;
    writeln!(out)?;
    format_commonmark(doc, &options, &mut out)?;

    std::fs::write("CONTRIBUTING.md", &out)?;

    Ok(())
}
