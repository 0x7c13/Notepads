//! The `comrak` binary.

use std::env;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process;
use std::{boxed::Box, io::BufWriter};

use clap::{Parser, ValueEnum};

use comrak::options;
use comrak::{Arena, Options};
#[cfg(feature = "syntect")]
use comrak::{adapters::SyntaxHighlighterAdapter, plugins::syntect::SyntectAdapter};

const EXIT_SUCCESS: i32 = 0;
const EXIT_PARSE_CONFIG: i32 = 2;
const EXIT_READ_INPUT: i32 = 3;
const EXIT_CHECK_FILE_NUM: i32 = 4;

#[derive(Debug, Parser)]
#[command(about, author, version)]
#[command(after_help = "\
By default, Comrak will attempt to read command-line options from a config file specified by \
--config-file. This behaviour can be disabled by passing --config-file none. It is not an error \
if the file does not exist.\
        ")]
struct Cli {
    /// CommonMark file(s) to parse; or standard input if none passed
    #[arg(value_name = "FILE")]
    files: Option<Vec<PathBuf>>,

    /// Path to config file containing command-line arguments, or 'none'
    #[arg(short, long, value_name = "PATH", default_value = get_default_config_path())]
    config_file: String,

    /// Reformat CommonMark files in-place
    #[arg(short, long, conflicts_with_all(["format", "output"]))]
    inplace: bool,

    /// Treat newlines as hard line breaks
    #[arg(long)]
    hardbreaks: bool,

    /// Replace punctuation like "this" with smart punctuation like “this”
    #[arg(long)]
    smart: bool,

    /// Use GitHub-style "<pre lang>" for code blocks
    #[arg(long)]
    github_pre_lang: bool,

    /// Include words following the code block info string in a data-meta attribute
    #[arg(long)]
    full_info_string: bool,

    /// Enable GitHub-flavored markdown extensions: strikethrough, table,
    /// autolink, and tasklist. Also enables --github-pre-lang and --gfm-quirks.
    #[arg(long)]
    gfm: bool,

    /// Use GFM-style quirks in output HTML, such as not nesting <strong>
    /// tags, which otherwise breaks CommonMark compatibility.
    #[arg(long)]
    gfm_quirks: bool,

    /// Permit any character inside a tasklist item, not just " ", "x" or "X"
    #[arg(long)]
    relaxed_tasklist_character: bool,

    /// Relax autolink parsing: allows links to be recognised when in brackets,
    /// permits all URL schemes, and permits domains without a TLD (like "http://localhost")
    #[arg(long)]
    relaxed_autolinks: bool,

    /// Include "task-list-item" and "task-list-item-checkbox" classes on
    // tasklist "<li>" and "<input>" elements respectively
    #[arg(long)]
    tasklist_classes: bool,

    /// Default value for fenced code block's info strings if none is given
    #[arg(long, value_name = "INFO")]
    default_info_string: Option<String>,

    /// Allow inline and block HTML (unless --escape is given), and permit
    // dangerous URLs (like "javascript:" and non-image "data:" URLs)
    #[arg(long = "unsafe")]
    r#unsafe: bool,

    /// Translate gemoji like ":thumbsup:" into Unicode emoji like "👍"
    #[arg(long)]
    #[cfg(feature = "shortcodes")]
    gemoji: bool,

    /// Escape raw HTML, instead of clobbering it; takes precedence over --unsafe
    #[arg(long)]
    escape: bool,

    /// Wrap escaped Markdown characters in "<span data-escaped-char>" in HTML
    #[arg(long)]
    escaped_char_spans: bool,

    /// Specify extensions to use
    ///
    /// Multiple extensions can be delimited with ",", e.g. '--extension
    /// strikethrough,table', or you can pass --extension/-e multiple times
    #[arg(
        short,
        long = "extension",
        value_name = "EXTENSION",
        value_delimiter = ',',
        value_enum
    )]
    extensions: Vec<Extension>,

    /// Specify output format
    #[arg(short = 't', long = "to", value_enum, default_value_t = Format::Html)]
    format: Format,

    /// Write output to FILE instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Specify wrap width for output CommonMark, or '0' to disable wrapping
    #[arg(long, default_value_t = 0)]
    width: usize,

    /// Use the Comrak header IDs extension, with the given ID prefix
    #[arg(long, value_name = "PREFIX", required = false)]
    header_id_prefix: Option<String>,

    /// Apply the header ID prefix to the href anchor as well
    #[arg(long)]
    header_id_prefix_in_href: bool,

    /// Detect frontmatter that starts and ends with the given string, and do
    /// not include it in the resulting document
    #[arg(long, value_name = "DELIMITER", allow_hyphen_values = true)]
    front_matter_delimiter: Option<String>,

    /// Syntax highlighting for fenced code blocks; 'css' for CSS classes (default),
    /// a theme name for inline styles, or 'none' to disable
    #[arg(long, value_name = "THEME", default_value = "css")]
    #[cfg(feature = "syntect")]
    syntax_highlighting: String,

    /// Specify bullet character for lists ("-", "+", "*") in CommonMark output
    #[arg(long, value_enum, default_value_t = ListStyle::Dash)]
    list_style: ListStyle,

    /// Specify alert style (<div> vs <aside>) in HTML output
    #[arg(long, value_enum, default_value_t = AlertStyle::Specific)]
    alert_style: AlertStyle,

    /// Include source position attributes in HTML and XML output
    #[arg(long)]
    sourcepos: bool,

    /// Do not parse setext headers
    #[arg(long)]
    ignore_setext: bool,

    /// Do not parse empty links
    #[arg(long)]
    ignore_empty_links: bool,

    /// Report column positions in sourcepos as a Unicode character count
    /// rather than UTF-8 byte offsets
    #[arg(long)]
    sourcepos_chars: bool,

    /// Minimise escapes in CommonMark output using a trial-and-error algorithm
    #[arg(long)]
    experimental_minimize_commonmark: bool,

    /// Suppress pretty-printing newlines between block-level HTML elements
    #[arg(long)]
    compact: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Html,

    Xml,

    #[value(name = "commonmark")]
    CommonMark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Extension {
    Strikethrough,
    Table,
    Autolink,
    Tasklist,
    Superscript,
    Footnotes,
    InlineFootnotes,
    DescriptionLists,
    MultilineBlockQuotes,
    MathDollars,
    MathLatex,
    MathCode,
    WikilinksTitleAfterPipe,
    WikilinksTitleBeforePipe,
    Underline,
    Subscript,
    Spoiler,
    Greentext,
    Alerts,
    CjkFriendlyEmphasis,
    Subtext,
    Highlight,
    Insert,
    PhoenixHeex,
    BlockDirective,
    HeaderAttributes,
    FencedCodeAttributes,
    InlineCodeAttributes,
    LinkAttributes,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ListStyle {
    Dash,
    Plus,
    Star,
}

impl From<ListStyle> for options::ListStyleType {
    fn from(style: ListStyle) -> Self {
        match style {
            ListStyle::Dash => Self::Dash,
            ListStyle::Plus => Self::Plus,
            ListStyle::Star => Self::Star,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AlertStyle {
    Specific,
    Semantic,
}

impl From<AlertStyle> for options::AlertStyleType {
    fn from(style: AlertStyle) -> Self {
        match style {
            AlertStyle::Specific => Self::Specific,
            AlertStyle::Semantic => Self::Semantic,
        }
    }
}

fn cli_with_config() -> Cli {
    let cli = Cli::parse();
    let config_file_path = &cli.config_file;

    if config_file_path == "none" {
        return cli;
    }

    if let Ok(args) = fs::read_to_string(config_file_path) {
        match shell_words::split(&args) {
            Ok(mut args) => {
                for (i, arg) in env::args_os().enumerate() {
                    if let Some(s) = arg.to_str() {
                        args.insert(i, s.into());
                    }
                }

                Cli::parse_from(args)
            }
            Err(e) => {
                eprintln!("failed to parse {}: {}", config_file_path, e);
                process::exit(EXIT_PARSE_CONFIG);
            }
        }
    } else {
        cli
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = cli_with_config();

    if cli.inplace && cli.files.is_none() {
        eprintln!("no input file specified: cannot use standard input with in-place mode");
        process::exit(EXIT_CHECK_FILE_NUM);
    }

    let exts = &cli.extensions;

    let extension = options::Extension::builder()
        .strikethrough(exts.contains(&Extension::Strikethrough) || cli.gfm)
        .table(exts.contains(&Extension::Table) || cli.gfm)
        .autolink(exts.contains(&Extension::Autolink) || cli.gfm)
        .tasklist(exts.contains(&Extension::Tasklist) || cli.gfm)
        .superscript(exts.contains(&Extension::Superscript))
        .maybe_header_id_prefix(cli.header_id_prefix)
        .header_id_prefix_in_href(cli.header_id_prefix_in_href)
        .footnotes(exts.contains(&Extension::Footnotes))
        .inline_footnotes(exts.contains(&Extension::InlineFootnotes))
        .description_lists(exts.contains(&Extension::DescriptionLists))
        .multiline_block_quotes(exts.contains(&Extension::MultilineBlockQuotes))
        .math_dollars(exts.contains(&Extension::MathDollars))
        .math_latex(exts.contains(&Extension::MathLatex))
        .math_code(exts.contains(&Extension::MathCode))
        .wikilinks_title_after_pipe(exts.contains(&Extension::WikilinksTitleAfterPipe))
        .wikilinks_title_before_pipe(exts.contains(&Extension::WikilinksTitleBeforePipe))
        .underline(exts.contains(&Extension::Underline))
        .subscript(exts.contains(&Extension::Subscript))
        .spoiler(exts.contains(&Extension::Spoiler))
        .greentext(exts.contains(&Extension::Greentext))
        .alerts(exts.contains(&Extension::Alerts))
        .maybe_front_matter_delimiter(cli.front_matter_delimiter)
        .cjk_friendly_emphasis(exts.contains(&Extension::CjkFriendlyEmphasis))
        .subtext(exts.contains(&Extension::Subtext))
        .highlight(exts.contains(&Extension::Highlight))
        .insert(exts.contains(&Extension::Insert))
        .phoenix_heex(exts.contains(&Extension::PhoenixHeex))
        .block_directive(exts.contains(&Extension::BlockDirective));

    #[cfg(feature = "attributes")]
    let extension = extension
        .header_attributes(exts.contains(&Extension::HeaderAttributes))
        .fenced_code_attributes(exts.contains(&Extension::FencedCodeAttributes))
        .inline_code_attributes(exts.contains(&Extension::InlineCodeAttributes))
        .link_attributes(exts.contains(&Extension::LinkAttributes));

    #[cfg(feature = "shortcodes")]
    let extension = extension.shortcodes(cli.gemoji);

    let extension = extension.build();

    let parse = options::Parse::builder()
        .smart(cli.smart)
        .maybe_default_info_string(cli.default_info_string)
        .relaxed_tasklist_matching(cli.relaxed_tasklist_character)
        .relaxed_autolinks(cli.relaxed_autolinks)
        .ignore_setext(cli.ignore_setext)
        .sourcepos_chars(cli.sourcepos_chars)
        .build();

    let render = options::Render::builder()
        .hardbreaks(cli.hardbreaks)
        .github_pre_lang(cli.github_pre_lang || cli.gfm)
        .full_info_string(cli.full_info_string)
        .width(cli.width)
        .r#unsafe(cli.r#unsafe)
        .escape(cli.escape)
        .list_style(cli.list_style.into())
        .alert_style(cli.alert_style.into())
        .sourcepos(cli.sourcepos)
        .experimental_minimize_commonmark(cli.experimental_minimize_commonmark)
        .compact_html(cli.compact)
        .escaped_char_spans(cli.escaped_char_spans)
        .ignore_empty_links(cli.ignore_empty_links)
        .gfm_quirks(cli.gfm_quirks || cli.gfm)
        .tasklist_classes(cli.tasklist_classes)
        .build();

    let options = Options {
        extension,
        parse,
        render,
    };

    #[cfg(feature = "syntect")]
    let syntax_highlighter: Option<&dyn SyntaxHighlighterAdapter>;
    #[cfg(feature = "syntect")]
    let adapter: SyntectAdapter;

    #[cfg_attr(not(feature = "syntect"), allow(unused_mut))]
    let mut plugins = options::Plugins::default();

    #[cfg(feature = "syntect")]
    {
        let theme = cli.syntax_highlighting;
        if theme.is_empty() || theme == "none" {
            syntax_highlighter = None;
        } else if theme == "css" {
            adapter = SyntectAdapter::new(None);
            syntax_highlighter = Some(&adapter);
        } else {
            adapter = SyntectAdapter::new(Some(&theme));
            syntax_highlighter = Some(&adapter);
        }
    }

    if cli.inplace {
        for path in cli.files.as_ref().unwrap() {
            let mut input = String::new();
            match fs::File::open(path) {
                Ok(mut io) => {
                    io.read_to_string(&mut input)?;
                }
                Err(e) => {
                    eprintln!("failed to read {}: {}", path.display(), e);
                    process::exit(EXIT_READ_INPUT);
                }
            }

            let arena = Arena::new();
            let root = comrak::parse_document(&arena, &input, &options);
            let mut bw = BufWriter::new(fs::File::create(path)?);
            fmt2io::write(&mut bw, |writer| {
                comrak::format_commonmark_with_plugins(root, &options, writer, &plugins)
            })?;
            std::io::Write::flush(&mut bw)?;
        }
    } else {
        // The stdlib is very good at reserving buffer space based on available
        // information; don't try to one-up it.
        let input = match cli.files {
            None => {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            }
            Some(ref paths) => {
                let mut buf = String::new();
                for path in paths {
                    match fs::File::open(path) {
                        Ok(mut io) => {
                            io.read_to_string(&mut buf)?;
                        }
                        Err(e) => {
                            eprintln!("failed to read {}: {}", path.display(), e);
                            process::exit(EXIT_READ_INPUT);
                        }
                    }
                }
                buf
            }
        };

        let arena = Arena::new();
        let root = comrak::parse_document(&arena, &input, &options);

        let formatter = match cli.format {
            Format::Html => {
                #[cfg(feature = "syntect")]
                {
                    plugins.render.codefence_syntax_highlighter = syntax_highlighter;
                }
                comrak::format_html_with_plugins
            }
            Format::Xml => comrak::format_xml_with_plugins,
            Format::CommonMark => comrak::format_commonmark_with_plugins,
        };

        if let Some(output_filename) = cli.output {
            let mut bw = BufWriter::new(fs::File::create(output_filename)?);
            fmt2io::write(&mut bw, |writer| {
                formatter(root, &options, writer, &plugins)
            })?;
            std::io::Write::flush(&mut bw)?;
        } else {
            let stdout = std::io::stdout();
            let mut bw = BufWriter::new(stdout.lock());
            fmt2io::write(&mut bw, |writer| {
                formatter(root, &options, writer, &plugins)
            })?;
            std::io::Write::flush(&mut bw)?;
        };
    }

    process::exit(EXIT_SUCCESS);
}

#[cfg(all(not(windows), not(target_arch = "wasm32")))]
fn get_default_config_path() -> String {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("comrak");
    if let Ok(path) = xdg_dirs.place_config_file("config")
        && let Some(path_str) = path.to_str()
    {
        return path_str.into();
    }

    "comrak.config".into()
}
// If on Windows or compiling to wasm, disable default config file check
#[cfg(any(windows, target_arch = "wasm32"))]
fn get_default_config_path() -> String {
    "none".into()
}
