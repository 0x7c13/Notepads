// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::identity::LogicalSpan;
use crate::ir::NodeId;

pub const MAX_BLOCKS: usize = 64;
pub const MAX_PARTICIPATING_UTF8: usize = 1_048_576;
pub const MAX_SPANS: usize = 131_072;
pub const MAX_BLOCK_UTF8: usize = 262_144;
pub const MAX_BLOCK_LINES: usize = 4096;
pub const MAX_LINE_UTF8: usize = 8192;
pub const MAX_EXTERNAL_EVENTS: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    Cpp,
    Rust,
    Json,
    JavaScript,
    Python,
    Yaml,
    Shell,
    CSharp,
    Html,
    Css,
    Sql,
    Diff,
    Go,
    Java,
    Ruby,
    Xml,
    Lua,
    Makefile,
    C,
    R,
    Scala,
    Markdown,
    Batchfile,
    Perl,
    Haskell,
    ObjectiveC,
    ObjectiveCpp,
    Php,
    HtmlPhp,
    Tex,
    Clojure,
    Erlang,
    Groovy,
    Ocaml,
    CommonLisp,
    Tcl,
    JavaProperties,
    Graphviz,
    Matlab,
    Pascal,
    RestructuredText,
    AppleScript,
    D,
    Haml,
    HtmlErb,
    TypeScript,
    TypeScriptReact,
    PowerShell,
    Toml,
    Dockerfile,
}

impl Language {
    pub(super) fn from_info(info: &str) -> Option<Self> {
        let token = crate::code_fence::FenceInfo::parse(info).language;
        [
            ("cpp", Self::Cpp),
            ("c++", Self::Cpp),
            ("cxx", Self::Cpp),
            ("cc", Self::Cpp),
            ("hpp", Self::Cpp),
            ("hh", Self::Cpp),
            ("hxx", Self::Cpp),
            ("h++", Self::Cpp),
            ("rust", Self::Rust),
            ("rs", Self::Rust),
            ("json", Self::Json),
            ("jsonc", Self::Json),
            ("js", Self::JavaScript),
            ("javascript", Self::JavaScript),
            ("mjs", Self::JavaScript),
            ("cjs", Self::JavaScript),
            ("node", Self::JavaScript),
            ("nodejs", Self::JavaScript),
            ("py", Self::Python),
            ("python", Self::Python),
            ("python3", Self::Python),
            ("py3", Self::Python),
            ("yaml", Self::Yaml),
            ("yml", Self::Yaml),
            ("sh", Self::Shell),
            ("bash", Self::Shell),
            ("shell", Self::Shell),
            ("shell-script", Self::Shell),
            ("zsh", Self::Shell),
            ("envrc", Self::Shell),
            ("ksh", Self::Shell),
            ("cs", Self::CSharp),
            ("c#", Self::CSharp),
            ("csharp", Self::CSharp),
            ("html", Self::Html),
            ("htm", Self::Html),
            ("xhtml", Self::Html),
            ("css", Self::Css),
            ("sql", Self::Sql),
            ("diff", Self::Diff),
            ("udiff", Self::Diff),
            ("patch", Self::Diff),
            ("go", Self::Go),
            ("golang", Self::Go),
            ("java", Self::Java),
            ("rb", Self::Ruby),
            ("ruby", Self::Ruby),
            ("xml", Self::Xml),
            ("svg", Self::Xml),
            ("lua", Self::Lua),
            ("make", Self::Makefile),
            ("makefile", Self::Makefile),
            ("c", Self::C),
            ("r", Self::R),
            ("rscript", Self::R),
            ("scala", Self::Scala),
            ("markdown", Self::Markdown),
            ("md", Self::Markdown),
            ("batchfile", Self::Batchfile),
            ("bat", Self::Batchfile),
            ("cmd", Self::Batchfile),
            ("dos", Self::Batchfile),
            ("batch", Self::Batchfile),
            ("dosbatch", Self::Batchfile),
            ("winbatch", Self::Batchfile),
            ("perl", Self::Perl),
            ("haskell", Self::Haskell),
            ("objective-c", Self::ObjectiveC),
            ("obj-c", Self::ObjectiveC),
            ("objc", Self::ObjectiveC),
            ("objectivec", Self::ObjectiveC),
            ("objective-c++", Self::ObjectiveCpp),
            ("obj-c++", Self::ObjectiveCpp),
            ("objc++", Self::ObjectiveCpp),
            ("objectivec++", Self::ObjectiveCpp),
            ("php", Self::Php),
            ("html+php", Self::HtmlPhp),
            ("phtml", Self::HtmlPhp),
            ("tex", Self::Tex),
            ("latex", Self::Tex),
            ("clojure", Self::Clojure),
            ("clj", Self::Clojure),
            ("erlang", Self::Erlang),
            ("erl", Self::Erlang),
            ("groovy", Self::Groovy),
            ("gradle", Self::Groovy),
            ("ocaml", Self::Ocaml),
            ("lisp", Self::CommonLisp),
            ("common-lisp", Self::CommonLisp),
            ("tcl", Self::Tcl),
            ("properties", Self::JavaProperties),
            ("java-properties", Self::JavaProperties),
            ("dot", Self::Graphviz),
            ("gv", Self::Graphviz),
            ("matlab", Self::Matlab),
            ("octave", Self::Matlab),
            ("pascal", Self::Pascal),
            ("objectpascal", Self::Pascal),
            ("delphi", Self::Pascal),
            ("rst", Self::RestructuredText),
            ("restructuredtext", Self::RestructuredText),
            ("applescript", Self::AppleScript),
            ("apples", Self::AppleScript),
            ("osascript", Self::AppleScript),
            ("d", Self::D),
            ("dlang", Self::D),
            ("haml", Self::Haml),
            ("html+erb", Self::HtmlErb),
            ("erb", Self::HtmlErb),
            ("rhtml", Self::HtmlErb),
            ("html+ruby", Self::HtmlErb),
            ("typescript", Self::TypeScript),
            ("ts", Self::TypeScript),
            ("mts", Self::TypeScript),
            ("cts", Self::TypeScript),
            ("tsx", Self::TypeScriptReact),
            ("typescriptreact", Self::TypeScriptReact),
            ("powershell", Self::PowerShell),
            ("pwsh", Self::PowerShell),
            ("ps1", Self::PowerShell),
            ("psm1", Self::PowerShell),
            ("psd1", Self::PowerShell),
            ("toml", Self::Toml),
            ("dockerfile", Self::Dockerfile),
            ("docker", Self::Dockerfile),
            ("containerfile", Self::Dockerfile),
        ]
        .into_iter()
        .find_map(|(name, language)| token.eq_ignore_ascii_case(name).then_some(language))
    }

    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Cpp => "cpp",
            Self::Rust => "rs",
            Self::Json => "json",
            Self::JavaScript => "js",
            Self::Python => "py",
            Self::Yaml => "yaml",
            Self::Shell => "sh",
            Self::CSharp => "cs",
            Self::Html => "html",
            Self::Css => "css",
            Self::Sql => "sql",
            Self::Diff => "diff",
            Self::Go => "go",
            Self::Java => "java",
            Self::Ruby => "rb",
            Self::Xml => "xml",
            Self::Lua => "lua",
            Self::Makefile => "make",
            Self::C => "c",
            Self::R => "r",
            Self::Scala => "scala",
            Self::Markdown => "md",
            Self::Batchfile => "bat",
            Self::Perl => "pl",
            Self::Haskell => "hs",
            Self::ObjectiveC => "m",
            Self::ObjectiveCpp => "mm",
            Self::Php | Self::HtmlPhp => "php",
            Self::Tex => "tex",
            Self::Clojure => "clj",
            Self::Erlang => "erl",
            Self::Groovy => "groovy",
            Self::Ocaml => "ml",
            Self::CommonLisp => "lisp",
            Self::Tcl => "tcl",
            Self::JavaProperties => "properties",
            Self::Graphviz => "dot",
            Self::Matlab => "matlab",
            Self::Pascal => "pas",
            Self::RestructuredText => "rst",
            Self::AppleScript => "applescript",
            Self::D => "d",
            Self::Haml => "haml",
            Self::HtmlErb => "rhtml",
            Self::TypeScript => "ts",
            Self::TypeScriptReact => "tsx",
            Self::PowerShell => "ps1",
            Self::Toml => "toml",
            Self::Dockerfile => "dockerfile",
        }
    }
}

/// Lexical visual roles, independent of colors, fonts, platform objects and the parser library.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticKind {
    Plain,
    Comment,
    String,
    Number,
    Keyword,
    Type,
    Function,
    Added,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenSpan {
    pub start_utf16: u32,
    pub length_utf16: u32,
    pub kind: SemanticKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpanRange {
    pub start: u32,
    pub len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkipReason {
    UnknownLanguage,
    BlockBytes,
    BlockLines,
    LineBytes,
    DocumentBytes,
    SpanCount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureReason {
    RegexCompile,
    RegexSearch,
    Grammar,
    Scope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockStatus {
    /// An aborted snapshot leaves the current and remaining entries unprocessed.
    Pending,
    Complete,
    Skipped(SkipReason),
    Failed(FailureReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HighlightBlock {
    pub node: NodeId,
    pub logical: LogicalSpan,
    pub language: Option<Language>,
    pub status: BlockStatus,
    pub spans: SpanRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotStatus {
    /// The finite pass finished; individual skips/failures remain visible in rows and summary.
    Complete,
    /// Only the first 64 real Main CodeBlock entries are cached.
    EntryLimit,
    /// Prior complete blocks are immutable; current/remaining Pending entries have no spans.
    Aborted(CoreError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HighlightSummary {
    pub status: SnapshotStatus,
    pub complete_blocks: u32,
    pub skipped_blocks: u32,
    pub failed_blocks: u32,
    /// Selected blocks copied from exact normalized-language/text cache entries.
    pub reused_blocks: u32,
    pub participating_utf8: u64,
    /// Bounded external parser/adapter events, never regex VM instructions.
    pub external_events: u64,
}
