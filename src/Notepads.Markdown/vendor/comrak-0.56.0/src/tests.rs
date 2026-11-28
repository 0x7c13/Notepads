// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::panic;

#[cfg(feature = "attributes")]
use crate::nodes::Attributes;
use crate::nodes::{Node, NodeValue, Sourcepos};
use crate::options;
use crate::*;

mod alerts;
mod attributes;
mod autolink;
mod block_directive;
mod cjk_friendly_emphasis;
mod code;
mod commonmark;
mod compact_html;
mod core;
mod description_lists;
mod empty;
mod escape;
mod escaped_char_spans;
mod footnotes;
mod front_matter;
mod fuzz;
mod greentext;
mod header_id_prefix;
mod highlight;
#[path = "tests/html.rs"]
mod html_;
mod inline_footnotes;
mod insert;
mod math;
mod multiline_block_quotes;
#[path = "tests/options.rs"]
mod options_;
mod pathological;
mod phoenix_heex;
mod plugins;
mod raw;
mod regressions;
mod rewriter;
mod shortcodes;
#[path = "tests/sourcepos.rs"]
mod sourcepos_;
mod sourcepos_chars;
mod spoiler;
mod strikethrough;
mod subscript;
mod subtext;
mod supersubscript;
mod table;
mod tasklist;
mod underline;
mod wikilinks;
mod xml;

#[track_caller]
fn compare_strs(output: &str, expected: &str, kind: &str, original_input: &str) {
    if output != expected {
        println!("Running {} test", kind);
        println!("Original Input:");
        println!("==============================");
        println!("{}", original_input);
        println!("==============================");
        println!("Got:");
        println!("==============================");
        println!("{}", output);
        println!("==============================");
        println!();
        println!("Expected:");
        println!("==============================");
        println!("{}", expected);
        println!("==============================");
        println!();
    }
    assert_eq!(output, expected);
}

#[track_caller]
fn commonmark(input: &str, expected: &str, opts: Option<&Options>) {
    let arena = Arena::new();
    let defaults = Options::default();
    let options = opts.unwrap_or(&defaults);

    let root = parse_document(&arena, input, options);
    let mut output = String::new();
    cm::format_document(root, options, &mut output).unwrap();
    compare_strs(&output, expected, "regular", input);
}

#[track_caller]
pub fn html(input: &str, expected: &str) {
    html_opts_i(input, expected, true, |_| ());
}

#[track_caller]
fn html_opts_i<'c, F>(input: &str, expected: &str, roundtrip: bool, opts: F)
where
    F: FnOnce(&mut Options<'c>),
{
    let mut options = Options::default();
    opts(&mut options);

    html_opts_w(input, expected, roundtrip, &options);
}

#[track_caller]
fn html_opts_w(input: &str, expected: &str, roundtrip: bool, options: &Options) {
    let arena = Arena::new();

    let root = parse_document(&arena, input, options);
    let mut output = String::new();
    html::format_document(root, options, &mut output).unwrap();
    compare_strs(&output, expected, "regular", input);

    if !roundtrip {
        return;
    }

    let mut md = String::new();
    cm::format_document(root, options, &mut md).unwrap();

    let root = parse_document(&arena, &md, options);
    let mut output_from_rt = String::new();
    html::format_document(root, options, &mut output_from_rt).unwrap();

    let expected_no_sourcepos = remove_sourcepos(expected);
    let actual_no_sourcepos = remove_sourcepos(&output_from_rt);

    compare_strs(
        &actual_no_sourcepos,
        &expected_no_sourcepos,
        "roundtrip",
        &md,
    );
}

fn remove_sourcepos(i: &str) -> String {
    const S: &str = " data-sourcepos=\"";

    let mut r = i.to_string();
    while let Some(start_ix) = r.find(S) {
        let end_offset = start_ix + S.len();
        let end_ix = r.get(start_ix + S.len()..).unwrap().find('"').unwrap();
        r.replace_range(start_ix..end_offset + end_ix + 1, "");
    }

    r
}

macro_rules! html_opts {
    ([$($optclass:ident.$optname:ident),*], $input:expr, $expected:expr) => {
        html_opts!([$($optclass.$optname),*], $input, $expected,)
    };
    ([$($optclass:ident.$optname:ident = $val:expr),*], $input:expr, $expected:expr) => {
        html_opts!([$($optclass.$optname = $val),*], $input, $expected,)
    };
    ([$($optclass:ident.$optname:ident),*], $input:expr, $expected:expr,) => {
        html_opts!([$($optclass.$optname),*], $input, $expected, roundtrip)
    };
    ([$($optclass:ident.$optname:ident = $val:expr),*], $input:expr, $expected:expr,) => {
        html_opts!([$($optclass.$optname = $val),*], $input, $expected, roundtrip)
    };
    ([$($optclass:ident.$optname:ident),*], $input:expr, $expected:expr, $rt:ident) => {
        html_opts!([$($optclass.$optname),*], $input, $expected, $rt,)
    };
    ([$($optclass:ident.$optname:ident = $val:expr),*], $input:expr, $expected:expr, $rt:ident) => {
        html_opts!([$($optclass.$optname = $val),*], $input, $expected, $rt,)
    };
    ([$($optclass:ident.$optname:ident),*], $input:expr, $expected:expr, roundtrip,) => {
        html_opts!([$($optclass.$optname = true),*], $input, $expected, roundtrip,)
    };
    ([$($optclass:ident.$optname:ident = $val:expr),*], $input:expr, $expected:expr, roundtrip,) => {
        $crate::tests::html_opts_i($input, $expected, true, |opts| {
            $(opts.$optclass.$optname = $val;)*
        });
    };
    ([$($optclass:ident.$optname:ident),*], $input:expr, $expected:expr, no_roundtrip,) => {
        html_opts!([$($optclass.$optname = true),*], $input, $expected, no_roundtrip,)
    };
    ([$($optclass:ident.$optname:ident = $val:expr),*], $input:expr, $expected:expr, no_roundtrip,) => {
        $crate::tests::html_opts_i($input, $expected, false, |opts| {
            $(opts.$optclass.$optname = $val;)*
        });
    };
}

pub(crate) use html_opts;

#[track_caller]
fn html_plugins(input: &str, expected: &str, plugins: &options::Plugins) {
    let arena = Arena::new();
    let options = Options::default();

    let root = parse_document(&arena, input, &options);
    let mut output = String::new();
    html::format_document_with_plugins(root, &options, &mut output, plugins).unwrap();
    compare_strs(&output, expected, "regular", input);

    let mut md = String::new();
    cm::format_document(root, &options, &mut md).unwrap();

    let root = parse_document(&arena, &md, &options);
    let mut output_from_rt = String::new();
    html::format_document_with_plugins(root, &options, &mut output_from_rt, plugins).unwrap();
    compare_strs(&output_from_rt, expected, "roundtrip", &md);
}

#[track_caller]
fn xml(input: &str, expected: &str) {
    xml_opts(input, expected, |_| ());
}

#[track_caller]
fn xml_opts<F>(input: &str, expected: &str, opts: F)
where
    F: Fn(&mut Options),
{
    let arena = Arena::new();
    let mut options = Options::default();
    opts(&mut options);

    let root = parse_document(&arena, input, &options);
    let mut output = String::new();
    crate::xml::format_document(root, &options, &mut output).unwrap();
    compare_strs(&output, expected, "regular", input);

    if options.render.sourcepos {
        return;
    }

    let mut md = String::new();
    cm::format_document(root, &options, &mut md).unwrap();

    let root = parse_document(&arena, &md, &options);
    let mut output_from_rt = String::new();
    crate::xml::format_document(root, &options, &mut output_from_rt).unwrap();
    compare_strs(&output_from_rt, expected, "roundtrip", &md);
}

fn assert_node_eq(node: Node<'_>, location: &[usize], expected: &NodeValue) {
    let node = location
        .iter()
        .fold(node, |node, &n| node.children().nth(n).unwrap());

    let data = node.data();
    let actual = format!("{:?}", data.value);
    let expected = format!("{:?}", expected);

    compare_strs(&actual, &expected, "ast comparison", "ast node");
}

macro_rules! sourcepos {
    (($spsl:literal:$spsc:literal-$spel:literal:$spec:literal)) => {
        $crate::nodes::Sourcepos {
            start: $crate::nodes::LineColumn {
                line: $spsl,
                column: $spsc,
            },
            end: $crate::nodes::LineColumn {
                line: $spel,
                column: $spec,
            },
        }
    };
    ((XXX)) => {
        $crate::tests::sourcepos!((0:1-0:1))
    };
}

pub(crate) use sourcepos;

macro_rules! ast {
    (($name:tt $sp:tt $( $content:tt )*)) => (#[allow(clippy::vec_init_then_push)] {
        #[allow(unused_mut)]
        let mut matches = vec![];
        $crate::tests::ast_content!(matches ~ $( $content )*);

        $crate::tests::AstMatchTree {
            name: stringify!($name).to_string(),
            sourcepos: $crate::tests::sourcepos!($sp),
            matches,
        }
    });
}

macro_rules! ast_content {
    ($matches:ident ~ $text:literal $( $rest:tt )*) => {
        $matches.push($crate::tests::AstMatchContent::Text($text.to_string()));
        $crate::tests::ast_content!($matches ~ $( $rest )*);
    };
    ($matches:ident ~ [ $( $children:tt )* ] $( $rest:tt )*) => {
        $matches.push($crate::tests::AstMatchContent::Children(vec![ $( $crate::tests::ast!($children), )* ]));
        $crate::tests::ast_content!($matches ~ $( $rest )*);
    };
     ($matches:ident ~ info:$info:literal $( $rest:tt )*) => {
        $matches.push($crate::tests::AstMatchContent::Info($info.to_string()));
        $crate::tests::ast_content!($matches ~ $( $rest )*);
     };

    // Are you reading this comment right now?
    //
    // Can you see that the following definition forces test authors to specify
    // attributes in the order "id, classes, key/value pairs", even though in
    // the actual syntax, that's not necessary? (Indeed, you could have multiple
    // ids in real syntax; the last one wins.)
    //
    // It's VERY possible to rewrite this to allow parsing them in any order, just
    // like the actual syntax. Feel free to try it and open a PR, and you can ask for help.
    //
    // Hint: look at how ast_content itself is implemented; recursion is the trick!
    //
    // Extra credit: at the moment, we only support x="y" syntax for pairs.
    // The above change would let us easily support both that *and* x=y syntax
    // (when there's no 'need' to quote the value).
    ($matches:ident ~ { $( # $id:ident )? $( . $class:ident )* $( $key:ident = $value:literal )* } $( $rest:tt )*) => {
        #[allow(unused_mut)]
        let mut attrs = $crate::nodes::Attributes::default();
        $( attrs.id = Some(stringify!($id).to_string()); )?
        $( attrs.classes.push(stringify!($class).to_string()); )*
        $( attrs.pairs.push((stringify!($key).to_string(), $value.to_string())); )*
        $matches.push($crate::tests::AstMatchContent::Attributes(attrs));
        $crate::tests::ast_content!($matches ~ $( $rest )*);
    };

    ($matches:ident ~) => {};
}

pub(crate) use ast;
pub(crate) use ast_content;

#[track_caller]
fn assert_ast_match_i<F>(md: &str, amt: AstMatchTree, opts: F)
where
    F: Fn(&mut Options),
{
    let mut options = Options::default();
    options.render.sourcepos = true;
    opts(&mut options);

    let result = panic::catch_unwind(|| {
        let arena = Arena::new();
        let root = parse_document(&arena, md, &options);

        amt.assert_match(root);
    });

    if let Err(err) = result {
        let arena = Arena::new();
        let root = parse_document(&arena, md, &options);

        let mut output = String::new();
        // Show escaped nodes in debugging XML where they exist.
        options.render.escaped_char_spans = true;
        format_xml(root, &options, &mut output).unwrap();
        eprintln!("{output}");

        panic::resume_unwind(err)
    }
}

macro_rules! assert_ast_match_set_opt_single {
    ($opts:ident; $optclass:ident.$optname:ident = $val:expr) => {
        $opts.$optclass.$optname = $val;
    };
    ($opts:ident; $optclass:ident.$optname:ident) => {
        $opts.$optclass.$optname = true;
    };
}

pub(crate) use assert_ast_match_set_opt_single;

macro_rules! assert_ast_match {
    ([ $( $optclass:ident.$optname:ident ),* ], $( $md:literal )+, $amt:tt,) => {
        $crate::tests::assert_ast_match!(
            [ $( $optclass.$optname ),* ],
            $( $md )+,
            $amt
        )
    };
    ([ $( $optclass:ident.$optname:ident = $val:expr ),* ], $( $md:literal )+, $amt:tt) => {
        $crate::tests::assert_ast_match_i(
            concat!( $( $md ),+ ),
            $crate::tests::ast!($amt),
            |#[allow(unused_variables)] opts| {$(opts.$optclass.$optname = $val;)*},
        );
    };
    ([ $( $optclass:ident.$optname:ident $(= $val:expr)? ),* ], $( $md:literal )+, $amt:tt) => {
        $crate::tests::assert_ast_match_i(
            concat!( $( $md ),+ ),
            $crate::tests::ast!($amt),
            |#[allow(unused_variables)] opts| { $( $crate::tests::assert_ast_match_set_opt_single!(opts; $optclass.$optname $(= $val)? ); )* },
        );
    };
    ([ $( $optclass:ident.$optname:ident ),* ], $( $md:literal )+, $amt:tt) => {
        $crate::tests::assert_ast_match!(
            [ $( $optclass.$optname  = true),* ],
            $( $md )+,
            $amt
        )
    };
}

pub(crate) use assert_ast_match;

enum AstMatchContent {
    Text(String),
    Info(String),
    Children(Vec<AstMatchTree>),
    #[cfg(feature = "attributes")]
    Attributes(Attributes),
}

struct AstMatchTree {
    name: String,
    sourcepos: Sourcepos,
    matches: Vec<AstMatchContent>,
}

impl AstMatchTree {
    #[track_caller]
    fn assert_match(&self, node: Node<'_>) {
        let ast = node.data();
        assert_eq!(self.name, ast.value.xml_node_name(), "node type matches");
        assert_eq!(self.sourcepos, ast.sourcepos, "sourcepos are equal");

        let mut asserted_text = false;
        let mut asserted_info = false;
        let mut asserted_children = false;
        #[cfg(feature = "attributes")]
        let mut asserted_attributes = false;

        for m in &self.matches {
            match m {
                AstMatchContent::Text(text) => match ast.value {
                    NodeValue::Math(ref nm) => {
                        assert_eq!(
                            text.as_str(),
                            nm.literal.owned_str().expect("legacy math literal"),
                            "Math literal should match"
                        );
                        asserted_text = true;
                    }
                    NodeValue::CodeBlock(ref ncb) => {
                        assert_eq!(
                            text.as_str(),
                            ncb.literal.owned_str().expect("legacy code block literal"),
                            "CodeBlock literal should match"
                        );
                        asserted_text = true;
                    }
                    NodeValue::Code(ref nc) => {
                        assert_eq!(
                            text.as_str(),
                            nc.literal.owned_str().expect("legacy code literal"),
                            "Code literal should match"
                        );
                        asserted_text = true;
                    }
                    #[cfg(feature = "shortcodes")]
                    NodeValue::ShortCode(ref nsc) => {
                        assert_eq!(text, &nsc.code, "Shortcode code should match");
                        asserted_text = true;
                    }
                    NodeValue::HtmlBlock(ref nhb) => {
                        assert_eq!(
                            text.as_str(),
                            nhb.literal.owned_str().expect("legacy HTML block literal"),
                            "HtmlBlock literal should match"
                        );
                        asserted_text = true;
                    }
                    #[cfg(feature = "phoenix_heex")]
                    NodeValue::HeexBlock(ref nhb) => {
                        assert_eq!(text, &nhb.literal, "HeexBlock literal should match");
                        asserted_text = true;
                    }
                    NodeValue::HtmlInline(ref nhi) => {
                        assert_eq!(
                            text.as_str(),
                            nhi.owned_str().expect("legacy inline HTML"),
                            "HtmlInline content should match"
                        );
                        asserted_text = true;
                    }
                    #[cfg(feature = "phoenix_heex")]
                    NodeValue::HeexInline(ref nhi) => {
                        assert_eq!(text, nhi, "HeexInline content should match");
                        asserted_text = true;
                    }
                    NodeValue::Text(ref nt) => {
                        assert_eq!(
                            text.as_str(),
                            nt.owned_str().expect("legacy text"),
                            "Text content should match"
                        );
                        asserted_text = true;
                    }
                    NodeValue::Link(ref nl) => {
                        assert_eq!(
                            text.as_str(),
                            nl.url.owned_str().expect("legacy link destination"),
                            "Link destination should match"
                        );
                        asserted_text = true;
                    }
                    NodeValue::Image(ref ni) => {
                        assert_eq!(
                            text.as_str(),
                            ni.url.owned_str().expect("legacy image source"),
                            "Image source should match"
                        );
                        asserted_text = true;
                    }
                    NodeValue::FrontMatter(ref nfm) => {
                        assert_eq!(text, nfm, "Front matter content should match");
                        asserted_text = true;
                    }
                    _ => panic!(
                        "no text content matcher for this node type: {:?}",
                        ast.value
                    ),
                },
                AstMatchContent::Info(info) => match ast.value {
                    NodeValue::CodeBlock(ref ncb) => {
                        assert_eq!(
                            info.as_str(),
                            ncb.info.owned_str().expect("legacy code block info"),
                            "CodeBlock info should match"
                        );
                        asserted_info = true;
                    }
                    _ => panic!("no info matcher for this node type: {:?}", ast.value),
                },
                AstMatchContent::Children(children) => {
                    assert_eq!(
                        children.len(),
                        node.children().count(),
                        "children count should match ({:?})",
                        node
                    );
                    for (e, a) in children.iter().zip(node.children()) {
                        e.assert_match(a);
                    }
                    asserted_children = true;
                }
                #[cfg(feature = "attributes")]
                AstMatchContent::Attributes(attrs) => {
                    assert!(ast.attrs.is_some());
                    assert_eq!(attrs, &**ast.attrs.as_ref().unwrap());
                    asserted_attributes = true;
                }
            }
        }

        assert!(
            asserted_children || node.children().count() == 0,
            "children were not asserted"
        );
        assert!(
            asserted_info
                || match ast.value {
                    NodeValue::CodeBlock(ref ncb) => ncb
                        .info
                        .owned_str()
                        .expect("legacy code block info")
                        .is_empty(),
                    _ => true,
                },
            "info string was not asserted"
        );
        #[cfg(feature = "attributes")]
        assert!(
            asserted_attributes || ast.attrs.is_none(),
            "attributes were not asserted"
        );
        assert!(
            asserted_text
                || !matches!(
                    ast.value,
                    NodeValue::Math(_)
                        | NodeValue::CodeBlock(_)
                        | NodeValue::Text(_)
                        | NodeValue::Link(_)
                        | NodeValue::Image(_)
                        | NodeValue::FrontMatter(_)
                        | NodeValue::HtmlBlock(_)
                        | NodeValue::HtmlInline(_)
                ),
            "text wasn't asserted"
        );
    }
}
