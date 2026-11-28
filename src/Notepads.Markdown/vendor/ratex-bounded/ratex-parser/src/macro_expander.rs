// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::expansion_budget::{Cost, ExpansionBudget};
use ratex_types::execution::{ExecutionControl, ExecutionError, Phase, Resource};
use std::collections::HashMap;
use std::sync::Arc;
mod template;
use template::{Template, token_cost};

use ratex_lexer::Lexer;
use ratex_lexer::token::{SourceLocation, Token};

use crate::error::{ParseError, ParseResult};
use crate::functions::FUNCTIONS;
use crate::parse_node::Mode;
use crate::stack_safety::{DepthBudget, MAX_INPUT_DEPTH};

/// Commands that act like macros but aren't defined as a macro, function, or symbol.
/// Used in `is_defined`.
pub static IMPLICIT_COMMANDS: &[&str] = &["^", "_", "\\limits", "\\nolimits"];

/// Handler type for function-based macros (e.g. \TextOrMath, \@ifstar).
/// Takes the MacroExpander mutably and returns tokens to push onto the stack.
pub type FnMacroHandler = fn(&mut MacroExpander) -> ParseResult<Vec<Token>>;

/// A macro definition: string template, token list, or function.
#[derive(Clone)]
pub enum MacroDefinition {
    /// Simple string expansion (e.g., `\def\foo{bar}` → "bar")
    Text(String),
    StaticText(&'static str),
    /// Pre-tokenized expansion with argument count
    Tokens {
        tokens: Vec<Token>,
        num_args: usize,
    },
    /// Function-based macro (consumes tokens directly, returns expansion)
    Function(FnMacroHandler),
}

impl std::fmt::Debug for MacroDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(s) => write!(f, "Text({:?})", s),
            Self::StaticText(s) => write!(f, "Text({:?})", s),
            Self::Tokens { tokens, num_args } => {
                write!(f, "Tokens {{ {} tokens, {} args }}", tokens.len(), num_args)
            }
            Self::Function(_) => write!(f, "Function(...)"),
        }
    }
}

/// Immutable namespace storage. Aliases and group undo records share the body.
struct StoredMacro {
    definition: MacroDefinition,
    token_cost: Option<Cost>,
}
impl StoredMacro {
    fn new(definition: MacroDefinition) -> Self {
        let token_cost = match &definition {
            MacroDefinition::Tokens { tokens, .. } => token_cost(tokens),
            _ => Some(Cost::default()),
        };
        Self {
            definition,
            token_cost,
        }
    }
}

/// The MacroExpander (or "gullet") manages macro expansion.
///
/// It sits between the Lexer (mouth) and the Parser (stomach).
/// Tokens are read from the lexer, pushed onto an internal stack,
/// and macros are expanded until only non-expandable tokens remain.
///
/// Modeled after KaTeX's MacroExpander.ts.
pub struct MacroExpander<'a> {
    bounded_groups: usize,
    execution: Option<&'a ExecutionControl<'a>>,
    pub lexer: Lexer<'a>,
    pub mode: Mode,
    stack: Vec<Token>,
    macros: MacroNamespace,
    expansion_count: usize,
    allocation_budget: ExpansionBudget,
    max_expand: usize,
    depth_budget: DepthBudget,
    expand_tokens_depth: usize,
}

// Builtins never change in place. Each parse stores only its own overrides;
// aliases and group undo records can safely retain these shared definitions.
mod builtin_views;
use builtin_views::{BUILTINS, BuiltinRegistry};
pub(crate) const fn bounded_readonly_bytes() -> usize {
    builtin_views::bounded_readonly_bytes()
}
#[derive(Clone)]
enum SharedMacro {
    Static(&'static StoredMacro),
    Owned(Arc<StoredMacro>),
}
#[cfg(test)]
impl SharedMacro {
    fn ptr_eq(a: impl std::borrow::Borrow<Self>, b: impl std::borrow::Borrow<Self>) -> bool {
        std::ptr::eq(
            std::ops::Deref::deref(a.borrow()),
            std::ops::Deref::deref(b.borrow()),
        )
    }
}
impl std::ops::Deref for SharedMacro {
    type Target = StoredMacro;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Static(value) => value,
            Self::Owned(value) => value,
        }
    }
}

/// Scoped macro namespace supporting group nesting.
struct MacroNamespace {
    builtins: Option<&'static BuiltinRegistry>,
    current: HashMap<String, SharedMacro>,
    group_stack: Vec<HashMap<String, Option<SharedMacro>>>,
}

impl MacroNamespace {
    fn new() -> Self {
        Self {
            builtins: Some(&BUILTINS),
            ..Self::empty()
        }
    }
    fn empty() -> Self {
        Self {
            builtins: None,
            current: HashMap::new(),
            group_stack: Vec::new(),
        }
    }

    fn get_shared(&self, name: &str) -> Option<SharedMacro> {
        self.current.get(name).cloned().or_else(|| {
            self.builtins
                .and_then(|b| b.get(name))
                .map(SharedMacro::Static)
        })
    }
    fn get(&self, name: &str) -> Option<&MacroDefinition> {
        self.current
            .get(name)
            .map(|value| &value.definition)
            .or_else(|| {
                self.builtins
                    .and_then(|b| b.get(name))
                    .map(|value| &value.definition)
            })
    }

    fn set(&mut self, name: String, def: MacroDefinition) {
        self.set_shared(name, SharedMacro::Owned(Arc::new(StoredMacro::new(def))));
    }
    fn set_shared(&mut self, name: String, def: SharedMacro) {
        if let Some(undo) = self.group_stack.last_mut() {
            undo.entry(name.clone())
                .or_insert_with(|| self.current.get(&name).cloned());
        }
        self.current.insert(name, def);
    }

    fn set_global(&mut self, name: String, def: MacroDefinition) {
        self.current
            .insert(name, SharedMacro::Owned(Arc::new(StoredMacro::new(def))));
    }

    fn has(&self, name: &str) -> bool {
        self.get_shared(name).is_some()
    }

    fn begin_group(&mut self) {
        self.group_stack.push(HashMap::new());
    }

    fn end_group(&mut self) {
        if let Some(undo) = self.group_stack.pop() {
            for (name, old_val) in undo {
                match old_val {
                    Some(def) => {
                        self.current.insert(name, def);
                    }
                    None => {
                        self.current.remove(&name);
                    }
                }
            }
        }
    }

    fn end_groups(&mut self) {
        while !self.group_stack.is_empty() {
            self.end_group();
        }
    }
}

/// Tokenize a macro expansion string into stack order (same as [`MacroDefinition::Text`] bodies).
fn lex_string_to_stack_tokens(text: &str, budget: &ExpansionBudget) -> ParseResult<Vec<Token>> {
    let mut lexer = Lexer::new(text);
    let mut tokens = Vec::new();
    loop {
        let span = lexer.next_span();
        if span.is_eof {
            break;
        }
        budget.charge(Some(Cost::token(span.text)), "generated text lexer")?;
        tokens.push(span.into_token());
    }
    tokens.reverse();
    Ok(tokens)
}

fn dotsc_space_after(next: &str) -> bool {
    matches!(
        next,
        ")" | "]"
            | "\\rbrack"
            | "\\}"
            | "\\rbrace"
            | "\\rangle"
            | "\\rceil"
            | "\\rfloor"
            | "\\rgroup"
            | "\\rmoustache"
            | "\\right"
            | "\\bigr"
            | "\\biggr"
            | "\\Bigr"
            | "\\Biggr"
            | "$"
            | ";"
            | "."
    )
}

fn dotso_space_after(next: &str) -> bool {
    next == "," || dotsc_space_after(next)
}

fn contextual_dots_for(next: &str) -> &'static str {
    if next == "," {
        "\\dotsc"
    } else if matches!(
        next,
        "\\not"
            | "+"
            | "="
            | "<"
            | ">"
            | "-"
            | "*"
            | ":"
            | "\\DOTSB"
            | "\\coprod"
            | "\\bigvee"
            | "\\bigwedge"
            | "\\biguplus"
            | "\\bigcap"
            | "\\bigcup"
            | "\\prod"
            | "\\sum"
            | "\\bigotimes"
            | "\\bigoplus"
            | "\\bigodot"
            | "\\bigsqcup"
            | "\\And"
            | "\\longrightarrow"
            | "\\Longrightarrow"
            | "\\longleftarrow"
            | "\\Longleftarrow"
            | "\\longleftrightarrow"
            | "\\Longleftrightarrow"
            | "\\mapsto"
            | "\\longmapsto"
            | "\\hookrightarrow"
            | "\\doteq"
            | "\\mathbin"
            | "\\mathrel"
            | "\\relbar"
            | "\\Relbar"
            | "\\xrightarrow"
            | "\\xleftarrow"
    ) || next.starts_with("\\not")
        || ratex_font::get_symbol(next, ratex_font::Mode::Math).is_some_and(|symbol| {
            matches!(
                symbol.group,
                ratex_font::symbols::Group::Bin | ratex_font::symbols::Group::Rel
            )
        })
    {
        "\\dotsb"
    } else if matches!(
        next,
        "\\DOTSI" | "\\int" | "\\oint" | "\\iint" | "\\iiint" | "\\iiiint" | "\\idotsint"
    ) {
        "\\dotsi"
    } else if next == "\\DOTSX" {
        "\\dotsx"
    } else {
        "\\dotso"
    }
}

impl<'a> MacroExpander<'a> {
    pub(crate) fn new_bounded(
        input: &'a str,
        mode: Mode,
        depth_budget: DepthBudget,
        storage: &'a ratex_types::storage::StorageLedger<'a>,
    ) -> Result<Self, ratex_types::storage::StorageError> {
        let mut lexer = Lexer::new(input);
        lexer.set_execution(storage.control());
        Ok(Self {
            bounded_groups: 0,
            lexer,
            mode,
            stack: storage.try_vec_with_capacity(
                Phase::Macro,
                ratex_types::storage::StorageFamily::Tokens,
                4096,
            )?,
            macros: MacroNamespace::new(),
            expansion_count: 0,
            allocation_budget: ExpansionBudget::new_bounded(storage)?,
            max_expand: 128,
            depth_budget,
            expand_tokens_depth: 0,
            execution: Some(storage.control()),
        })
    }
    pub fn new(input: &'a str, mode: Mode) -> Self {
        Self::new_with_budget(input, mode, DepthBudget::new(MAX_INPUT_DEPTH))
    }

    pub(crate) fn new_with_budget(input: &'a str, mode: Mode, depth_budget: DepthBudget) -> Self {
        Self {
            bounded_groups: 0,
            lexer: Lexer::new(input),
            mode,
            stack: Vec::new(),
            macros: MacroNamespace::new(),
            expansion_count: 0,
            allocation_budget: ExpansionBudget::default(),
            max_expand: 1000,
            depth_budget,
            expand_tokens_depth: 0,
            execution: None,
        }
    }

    pub(crate) fn set_execution(&mut self, control: &'a ExecutionControl<'a>) {
        self.execution = Some(control);
        self.lexer.set_execution(control);
    }

    pub(crate) fn execution_control(&self) -> Option<&'a ExecutionControl<'a>> {
        self.execution
    }
    fn pushed<T>(&self, out: &mut Vec<T>, value: T) -> ParseResult<()> {
        if let Some(storage) = self.execution.and_then(ExecutionControl::storage) {
            storage
                .try_push(
                    Phase::Macro,
                    ratex_types::storage::StorageFamily::Tokens,
                    out,
                    value,
                )
                .map_err(ParseError::storage)
        } else {
            out.push(value);
            Ok(())
        }
    }
    fn token(&self, text: &str, start: usize, end: usize) -> ParseResult<Token> {
        if let Some(storage) = self.execution.and_then(ExecutionControl::storage) {
            storage
                .control()
                .consume(Phase::Macro, Resource::Tokens, 1)
                .map_err(ParseError::control)?;
            Ok(Token {
                is_builtin_literal: false,
                text: storage
                    .try_string(Phase::Macro, text)
                    .map_err(ParseError::storage)?,
                loc: SourceLocation { start, end },
                noexpand: false,
                treat_as_relax: false,
            })
        } else {
            Ok(Token::new(text, start, end))
        }
    }

    pub(crate) fn checkpoint(&self, phase: Phase) -> ParseResult<()> {
        if let Some(control) = self.execution {
            control.checkpoint(phase).map_err(ParseError::control)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn load_builtins(&mut self) {
        let builtins: &[(&str, &str)] = &[
            // ── Grouping ──
            ("\\bgroup", "{"),
            ("\\egroup", "}"),
            // ── Symbols from latex.ltx ──
            ("\\lq", "`"),
            ("\\rq", "'"),
            // \lbrack and \rbrack are in the symbol table directly
            ("\\aa", "\\r a"),
            ("\\AA", "\\r A"),
            // ── Active characters ──
            ("~", "\\nobreakspace"),
            // ── Phantoms ──
            ("\\hphantom", "\\smash{\\phantom{#1}}"),
            // ── Negated symbols ──
            (
                "\\not",
                "\\html@mathml{\\mathrel{\\mathrlap\\@not}\\nobreak}{\\char\"338}",
            ),
            (
                "\\neq",
                "\\html@mathml{\\mathrel{\\not=}}{\\mathrel{\\char`≠}}",
            ),
            ("\\ne", "\\neq"),
            ("\u{2260}", "\\neq"),
            (
                "\\notin",
                "\\html@mathml{\\mathrel{{\\in}\\mathllap{/\\mskip1mu}}}{\\mathrel{\\char`∉}}",
            ),
            ("\u{2209}", "\\notin"),
            (
                "\\notni",
                "\\html@mathml{\\not\\ni}{\\mathrel{\\char`\u{220C}}}",
            ),
            ("\u{220C}", "\\notni"),
            // \le and \ge are in the symbol table directly, not macros

            // ── amsmath iff/implies ──
            ("\\iff", "\\DOTSB\\;\\Longleftrightarrow\\;"),
            ("\\implies", "\\DOTSB\\;\\Longrightarrow\\;"),
            ("\\impliedby", "\\DOTSB\\;\\Longleftarrow\\;"),
            // ── Italic Greek capitals ──
            ("\\varGamma", "\\mathit{\\Gamma}"),
            ("\\varDelta", "\\mathit{\\Delta}"),
            ("\\varTheta", "\\mathit{\\Theta}"),
            ("\\varLambda", "\\mathit{\\Lambda}"),
            ("\\varXi", "\\mathit{\\Xi}"),
            ("\\varPi", "\\mathit{\\Pi}"),
            ("\\varSigma", "\\mathit{\\Sigma}"),
            ("\\varUpsilon", "\\mathit{\\Upsilon}"),
            ("\\varPhi", "\\mathit{\\Phi}"),
            ("\\varPsi", "\\mathit{\\Psi}"),
            ("\\varOmega", "\\mathit{\\Omega}"),
            // ── Spacing (mode-aware via \TextOrMath) ──
            ("\\,", "\\TextOrMath{\\kern{.1667em}}{\\mskip{3mu}}"),
            ("\\thinspace", "\\,"),
            ("\\>", "\\mskip{4mu}"),
            ("\\:", "\\TextOrMath{\\kern{.2222em}}{\\mskip{4mu}}"),
            ("\\medspace", "\\:"),
            ("\\;", "\\TextOrMath{\\kern{.2777em}}{\\mskip{5mu}}"),
            ("\\thickspace", "\\;"),
            ("\\!", "\\TextOrMath{\\kern{-.1667em}}{\\mskip{-3mu}}"),
            ("\\negthinspace", "\\!"),
            (
                "\\negmedspace",
                "\\TextOrMath{\\kern{-.2222em}}{\\mskip{-4mu}}",
            ),
            (
                "\\negthickspace",
                "\\TextOrMath{\\kern{-.2777em}}{\\mskip{-5mu}}",
            ),
            ("\\enspace", "\\kern.5em "),
            ("\\enskip", "\\hskip.5em\\relax"),
            ("\\quad", "\\hskip1em\\relax"),
            ("\\qquad", "\\hskip2em\\relax"),
            // ── Newline ──
            ("\\newline", "\\\\\\relax"),
            // ── hspace ──
            ("\\@hspace", "\\hskip #1\\relax"),
            ("\\@hspacer", "\\rule{0pt}{0pt}\\hskip #1\\relax"),
            // ── llap / rlap / clap ──
            ("\\llap", "\\mathllap{\\textrm{#1}}"),
            ("\\rlap", "\\mathrlap{\\textrm{#1}}"),
            ("\\clap", "\\mathclap{\\textrm{#1}}"),
            // ── Logos ──
            (
                "\\TeX",
                "\\textrm{\\html@mathml{T\\kern-.1667em\\raisebox{-.5ex}{E}\\kern-.125emX}{TeX}}",
            ),
            (
                "\\LaTeX",
                "\\textrm{\\html@mathml{L\\kern-.36em\\raisebox{0.21em}{\\scriptstyle A}\\kern-.15em\\TeX}{LaTeX}}",
            ),
            (
                "\\KaTeX",
                "\\textrm{\\html@mathml{K\\kern-.17em\\raisebox{0.21em}{\\scriptstyle A}\\kern-.15em\\TeX}{KaTeX}}",
            ),
            // ── imath / jmath ──
            ("\\imath", "\\html@mathml{\\@imath}{\u{0131}}"),
            ("\\jmath", "\\html@mathml{\\@jmath}{\u{0237}}"),
            // ── minuso ──
            (
                "\\minuso",
                "\\mathbin{\\html@mathml{{\\mathrlap{\\mathchoice{\\kern{0.145em}}{\\kern{0.145em}}{\\kern{0.1015em}}{\\kern{0.0725em}}\\circ}{-}}}{\\char`\u{29B5}}}",
            ),
            ("\\clap", "\\mathclap{\\textrm{#1}}"),
            // ── mathstrut / underbar ──
            ("\\mathstrut", "\\vphantom{(}"),
            ("\\underbar", "\\underline{\\text{#1}}"),
            // ── Bbbk ──
            ("\\Bbbk", "\\Bbb{k}"),
            // ── substack ──
            ("\\substack", "\\begin{subarray}{c}#1\\end{subarray}"),
            // ── boxed ──
            ("\\boxed", "\\fbox{$\\displaystyle{#1}$}"),
            // ── colon ──
            (
                "\\colon",
                "\\nobreak\\mskip2mu\\mathpunct{}\\mathchoice{\\mkern-3mu}{\\mkern-3mu}{}{}{:}\\mskip6mu\\relax",
            ),
            // ── dots ──
            ("\\cdots", "\\@cdots"),
            ("\\dotsb", "\\cdots"),
            ("\\dotsm", "\\cdots"),
            ("\\dotsi", "\\!\\cdots"),
            ("\\dotsx", "\\ldots\\,"),
            ("\\DOTSI", "\\relax"),
            ("\\DOTSB", "\\relax"),
            ("\\DOTSX", "\\relax"),
            // ── negated relations / corners (→ symbol table \@xxx) ──
            ("\\gvertneqq", "\\@gvertneqq"),
            ("\\lvertneqq", "\\@lvertneqq"),
            ("\\ngeqq", "\\@ngeqq"),
            ("\\ngeqslant", "\\@ngeqslant"),
            ("\\nleqq", "\\@nleqq"),
            ("\\nleqslant", "\\@nleqslant"),
            ("\\nshortmid", "\\@nshortmid"),
            ("\\nshortparallel", "\\@nshortparallel"),
            ("\\nsubseteqq", "\\@nsubseteqq"),
            ("\\nsupseteqq", "\\@nsupseteqq"),
            ("\\ulcorner", "\\@ulcorner"),
            ("\\urcorner", "\\@urcorner"),
            ("\\llcorner", "\\@llcorner"),
            ("\\lrcorner", "\\@lrcorner"),
            ("\\varsubsetneq", "\\@varsubsetneq"),
            ("\\varsubsetneqq", "\\@varsubsetneqq"),
            ("\\varsupsetneq", "\\@varsupsetneq"),
            ("\\varsupsetneqq", "\\@varsupsetneqq"),
            // ── delimiters / text (compose from existing) ──
            // Match KaTeX `macros.ts` html@mathml first branch (STIX-style white tortoise brackets).
            ("\\lBrace", "\\mathopen{\\{\\mkern-3.2mu[}"),
            ("\\rBrace", "\\mathclose{]\\mkern-3.2mu\\}}"),
            ("\\llbracket", "\\mathopen{[\\mkern-3.2mu[}"),
            ("\\rrbracket", "\\mathclose{]\\mkern-3.2mu]}"),
            ("\\copyright", "\\textcircled{c}"),
            ("\\textregistered", "\\textcircled{\\scriptsize R}"),
            (
                "\\textcopyright",
                "\\html@mathml{\\textcircled{c}}{\\char`©}",
            ),
            // ── tmspace (TeX primitive used by \, \: \; \!) ──
            // KaTeX `\tmspace` takes 3 args: sign (+/-), mu-glue, em-kern.
            ("\\tmspace", "\\TextOrMath{\\kern#1#3}{\\mskip#1#2}\\relax"),
            // ── Unicode shorthand aliases ──
            // Mirrors the `defineMacro("\u{...}", "...")` block in
            // KaTeX `src/macros.ts`, so users can paste raw glyphs and
            // get the same expansion as the named macro.
            // Stacked relations (\u2258..\u225F).
            ("\u{2258}", "\\mathrel{\u{E258}}"),
            ("\u{2259}", "\\stackrel{\\tiny\\wedge}{=}"),
            ("\u{225A}", "\\stackrel{\\tiny\\vee}{=}"),
            ("\u{225B}", "\\stackrel{\\scriptsize\\star}{=}"),
            ("\u{225D}", "\\stackrel{\\tiny\\mathrm{def}}{=}"),
            ("\u{225E}", "\\mathrel{\u{E25E}}"),
            ("\u{225F}", "\\stackrel{\\tiny?}{=}"),
            // Misc relations / corners / punctuation.
            ("\u{27C2}", "\\perp"),
            ("\u{203C}", "\\mathclose{!\\mkern-0.8mu!}"),
            ("\u{231C}", "\\ulcorner"),
            ("\u{231D}", "\\urcorner"),
            ("\u{231E}", "\\llcorner"),
            ("\u{231F}", "\\lrcorner"),
            ("\u{00A9}", "\\copyright"),
            ("\u{00AE}", "\\textregistered"),
            // Mathtools colon variants (∷ ∹ ≔ ≕ ⩴).
            ("\u{2237}", "\\dblcolon"),
            ("\u{2239}", "\\eqcolon"),
            ("\u{2254}", "\\coloneqq"),
            ("\u{2255}", "\\eqqcolon"),
            ("\u{2A74}", "\\Coloneqq"),
            // stmaryrd brackets.
            ("\u{27E6}", "\\llbracket"),
            ("\u{27E7}", "\\rrbracket"),
            ("\u{2983}", "\\lBrace"),
            ("\u{2984}", "\\rBrace"),
            // Plimsoll.
            ("\u{29B5}", "\\minuso"),
            // ── vdots ──
            ("\\vdots", "{\\varvdots\\rule{0pt}{15pt}}"),
            ("\u{22ee}", "\\vdots"),
            // ── bmod / pod / pmod / mod ──
            (
                "\\bmod",
                "\\mathchoice{\\mskip1mu}{\\mskip1mu}{\\mskip5mu}{\\mskip5mu}\\mathbin{\\rm mod}\\mathchoice{\\mskip1mu}{\\mskip1mu}{\\mskip5mu}{\\mskip5mu}",
            ),
            (
                "\\pod",
                "\\allowbreak\\mathchoice{\\mkern18mu}{\\mkern8mu}{\\mkern8mu}{\\mkern8mu}(#1)",
            ),
            ("\\pmod", "\\pod{{\\rm mod}\\mkern6mu#1}"),
            (
                "\\mod",
                "\\allowbreak\\mathchoice{\\mkern18mu}{\\mkern12mu}{\\mkern12mu}{\\mkern12mu}{\\rm mod}\\,\\,#1",
            ),
            // ── limsup / liminf / etc ──
            ("\\limsup", "\\DOTSB\\operatorname*{lim\\,sup}"),
            ("\\liminf", "\\DOTSB\\operatorname*{lim\\,inf}"),
            ("\\injlim", "\\DOTSB\\operatorname*{inj\\,lim}"),
            ("\\projlim", "\\DOTSB\\operatorname*{proj\\,lim}"),
            (
                "\\varlimsup",
                "\\DOTSB\\operatorname*{\\overline{\\mathrm{lim}}}",
            ),
            (
                "\\varliminf",
                "\\DOTSB\\operatorname*{\\underline{\\mathrm{lim}}}",
            ),
            (
                "\\varinjlim",
                "\\DOTSB\\operatorname*{\\underrightarrow{\\mathrm{lim}}}",
            ),
            (
                "\\varprojlim",
                "\\DOTSB\\operatorname*{\\underleftarrow{\\mathrm{lim}}}",
            ),
            // ── statmath ──
            ("\\argmin", "\\DOTSB\\operatorname*{arg\\,min}"),
            ("\\argmax", "\\DOTSB\\operatorname*{arg\\,max}"),
            ("\\plim", "\\DOTSB\\mathop{\\operatorname{plim}}\\limits"),
            // ── mathtools colon variants ──
            ("\\ordinarycolon", ":"),
            ("\\vcentcolon", "\\mathrel{\\mathop\\ordinarycolon}"),
            (
                "\\dblcolon",
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-.9mu}\\vcentcolon}}{\\mathop{\\char\"2237}}",
            ),
            (
                "\\coloneqq",
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}=}}{\\mathop{\\char\"2254}}",
            ),
            (
                "\\Coloneqq",
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}=}}{\\mathop{\\char\"2237\\char\"3d}}",
            ),
            (
                "\\coloneq",
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}\\mathrel{-}}}{\\mathop{\\char\"3a\\char\"2212}}",
            ),
            (
                "\\Coloneq",
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}\\mathrel{-}}}{\\mathop{\\char\"2237\\char\"2212}}",
            ),
            (
                "\\eqqcolon",
                "\\html@mathml{\\mathrel{=\\mathrel{\\mkern-1.2mu}\\vcentcolon}}{\\mathop{\\char\"2255}}",
            ),
            (
                "\\Eqqcolon",
                "\\html@mathml{\\mathrel{=\\mathrel{\\mkern-1.2mu}\\dblcolon}}{\\mathop{\\char\"3d\\char\"2237}}",
            ),
            (
                "\\eqcolon",
                "\\html@mathml{\\mathrel{\\mathrel{-}\\mathrel{\\mkern-1.2mu}\\vcentcolon}}{\\mathop{\\char\"2239}}",
            ),
            (
                "\\Eqcolon",
                "\\html@mathml{\\mathrel{\\mathrel{-}\\mathrel{\\mkern-1.2mu}\\dblcolon}}{\\mathop{\\char\"2212\\char\"2237}}",
            ),
            (
                "\\colonapprox",
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}\\approx}}{\\mathop{\\char\"3a\\char\"2248}}",
            ),
            (
                "\\Colonapprox",
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}\\approx}}{\\mathop{\\char\"2237\\char\"2248}}",
            ),
            (
                "\\colonsim",
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}\\sim}}{\\mathop{\\char\"3a\\char\"223c}}",
            ),
            (
                "\\Colonsim",
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}\\sim}}{\\mathop{\\char\"2237\\char\"223c}}",
            ),
            // ── colonequals alternate names ──
            ("\\ratio", "\\vcentcolon"),
            ("\\coloncolon", "\\dblcolon"),
            ("\\colonequals", "\\coloneqq"),
            ("\\coloncolonequals", "\\Coloneqq"),
            ("\\equalscolon", "\\eqqcolon"),
            ("\\equalscoloncolon", "\\Eqqcolon"),
            ("\\colonminus", "\\coloneq"),
            ("\\coloncolonminus", "\\Coloneq"),
            ("\\minuscolon", "\\eqcolon"),
            ("\\minuscoloncolon", "\\Eqcolon"),
            ("\\coloncolonapprox", "\\Colonapprox"),
            ("\\coloncolonsim", "\\Colonsim"),
            (
                "\\simcolon",
                "\\mathrel{\\sim\\mathrel{\\mkern-1.2mu}\\vcentcolon}",
            ),
            (
                "\\simcoloncolon",
                "\\mathrel{\\sim\\mathrel{\\mkern-1.2mu}\\dblcolon}",
            ),
            (
                "\\approxcolon",
                "\\mathrel{\\approx\\mathrel{\\mkern-1.2mu}\\vcentcolon}",
            ),
            (
                "\\approxcoloncolon",
                "\\mathrel{\\approx\\mathrel{\\mkern-1.2mu}\\dblcolon}",
            ),
            // ── braket (string-based) ──
            ("\\bra", "\\mathinner{\\langle{#1}|}"),
            ("\\ket", "\\mathinner{|{#1}\\rangle}"),
            ("\\braket", "\\mathinner{\\langle{#1}\\rangle}"),
            (
                "\\Braket",
                "\\bra@ket{\\left\\langle}{\\,\\middle\\vert\\,}{\\,\\middle\\vert\\,}{\\right\\rangle}",
            ),
            ("\\Bra", "\\left\\langle#1\\right|"),
            ("\\Ket", "\\left|#1\\right\\rangle"),
            // ── texvc (MediaWiki) ──
            ("\\darr", "\\downarrow"),
            ("\\dArr", "\\Downarrow"),
            ("\\Darr", "\\Downarrow"),
            ("\\lang", "\\langle"),
            ("\\rang", "\\rangle"),
            ("\\uarr", "\\uparrow"),
            ("\\uArr", "\\Uparrow"),
            ("\\Uarr", "\\Uparrow"),
            ("\\N", "\\mathbb{N}"),
            ("\\R", "\\mathbb{R}"),
            ("\\Z", "\\mathbb{Z}"),
            ("\\alef", "\\aleph"),
            ("\\alefsym", "\\aleph"),
            ("\\Alpha", "\\mathrm{A}"),
            ("\\Beta", "\\mathrm{B}"),
            ("\\bull", "\\bullet"),
            ("\\Chi", "\\mathrm{X}"),
            ("\\clubs", "\\clubsuit"),
            ("\\cnums", "\\mathbb{C}"),
            ("\\Complex", "\\mathbb{C}"),
            ("\\Dagger", "\\ddagger"),
            ("\\diamonds", "\\diamondsuit"),
            ("\\empty", "\\emptyset"),
            ("\\Epsilon", "\\mathrm{E}"),
            ("\\Eta", "\\mathrm{H}"),
            ("\\exist", "\\exists"),
            ("\\harr", "\\leftrightarrow"),
            ("\\hArr", "\\Leftrightarrow"),
            ("\\Harr", "\\Leftrightarrow"),
            ("\\hearts", "\\heartsuit"),
            ("\\image", "\\Im"),
            ("\\infin", "\\infty"),
            ("\\Iota", "\\mathrm{I}"),
            ("\\isin", "\\in"),
            ("\\Kappa", "\\mathrm{K}"),
            ("\\larr", "\\leftarrow"),
            ("\\lArr", "\\Leftarrow"),
            ("\\Larr", "\\Leftarrow"),
            ("\\lrarr", "\\leftrightarrow"),
            ("\\lrArr", "\\Leftrightarrow"),
            ("\\Lrarr", "\\Leftrightarrow"),
            ("\\Mu", "\\mathrm{M}"),
            ("\\natnums", "\\mathbb{N}"),
            ("\\Nu", "\\mathrm{N}"),
            ("\\Omicron", "\\mathrm{O}"),
            ("\\plusmn", "\\pm"),
            ("\\rarr", "\\rightarrow"),
            ("\\rArr", "\\Rightarrow"),
            ("\\Rarr", "\\Rightarrow"),
            ("\\real", "\\Re"),
            ("\\reals", "\\mathbb{R}"),
            ("\\Reals", "\\mathbb{R}"),
            ("\\Rho", "\\mathrm{P}"),
            ("\\sdot", "\\cdot"),
            ("\\sect", "\\S"),
            ("\\spades", "\\spadesuit"),
            ("\\sub", "\\subset"),
            ("\\sube", "\\subseteq"),
            ("\\supe", "\\supseteq"),
            ("\\Tau", "\\mathrm{T}"),
            ("\\thetasym", "\\vartheta"),
            ("\\weierp", "\\wp"),
            ("\\Zeta", "\\mathrm{Z}"),
            // ── Khan Academy color aliases ──
            ("\\blue", "\\textcolor{##6495ed}{#1}"),
            ("\\orange", "\\textcolor{##ffa500}{#1}"),
            ("\\pink", "\\textcolor{##ff00af}{#1}"),
            ("\\red", "\\textcolor{##df0030}{#1}"),
            ("\\green", "\\textcolor{##28ae7b}{#1}"),
            ("\\gray", "\\textcolor{gray}{#1}"),
            ("\\purple", "\\textcolor{##9d38bd}{#1}"),
            // ── Unicode script letters ──
            ("\u{212C}", "\\mathscr{B}"),
            ("\u{2130}", "\\mathscr{E}"),
            ("\u{2131}", "\\mathscr{F}"),
            ("\u{210B}", "\\mathscr{H}"),
            ("\u{2110}", "\\mathscr{I}"),
            ("\u{2112}", "\\mathscr{L}"),
            ("\u{2133}", "\\mathscr{M}"),
            ("\u{211B}", "\\mathscr{R}"),
            ("\u{212D}", "\\mathfrak{C}"),
            ("\u{210C}", "\\mathfrak{H}"),
            ("\u{2128}", "\\mathfrak{Z}"),
            // ── notni ──
            (
                "\\notni",
                "\\html@mathml{\\not\\ni}{\\mathrel{\\char`\u{220C}}}",
            ),
            // ── actuarialangle ──
            ("\\angln", "{\\angl n}"),
            // ── set/Set (braket notation, simplified) ──
            ("\\set", "\\bra@set{\\{\\,}{\\mid}{}{\\,\\}}"),
            (
                "\\Set",
                "\\bra@set{\\left\\{\\:}{\\;\\middle\\vert\\;}{\\;\\middle\\Vert\\;}{\\:\\right\\}}",
            ),
            // ── KaTeX mhchem (\\tripledash for \\bond ~ forms) ──
            (
                "\\tripledash",
                "{\\vphantom{-}\\raisebox{2.56mu}{$\\mkern2mu\\tiny\\text{-}\\mkern1mu\\text{-}\\mkern1mu\\text{-}\\mkern2mu$}}",
            ),
        ];

        for &(name, expansion) in builtins {
            self.macros.set(
                name.to_string(),
                MacroDefinition::Text(expansion.to_string()),
            );
        }

        self.load_function_macros();
    }

    #[cfg(test)]
    fn load_function_macros(&mut self) {
        // \noexpand: mark the next token as non-expandable (only if expandable)
        self.macros.set(
            "\\noexpand".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let mut tok = me.pop_token();
                if me.is_expandable(&tok.text) {
                    tok.noexpand = true;
                    tok.treat_as_relax = true;
                }
                Ok(vec![tok])
            }),
        );

        // \@firstoftwo{A}{B} → A
        // NOTE: consume_args returns tokens in stack order (reversed).
        // We return them as-is since expand_once does stack.extend(tokens).
        self.macros.set(
            "\\@firstoftwo".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let args = me.consume_args(2)?;
                Ok(args.into_iter().next().unwrap())
            }),
        );

        // \@secondoftwo{A}{B} → B
        self.macros.set(
            "\\@secondoftwo".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let args = me.consume_args(2)?;
                Ok(args.into_iter().nth(1).unwrap())
            }),
        );

        // \@ifnextchar{C}{T}{F}: peek; if next non-space == C then T else F
        self.macros.set(
            "\\@ifnextchar".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let args = me.consume_args(3)?;
                me.consume_spaces();
                let next = me.future().text.clone();
                let char_arg = &args[0];
                // char_arg is reversed; the "first" char in original order is the last element
                let char_text = char_arg.first().map_or("", |t| t.text.as_str());
                if next == char_text {
                    Ok(args.into_iter().nth(1).unwrap())
                } else {
                    Ok(args.into_iter().nth(2).unwrap())
                }
            }),
        );

        // \@ifstar{with-star}{without-star}: if next is * → consume * and use first arg
        self.macros.set(
            "\\@ifstar".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let args = me.consume_args(2)?;
                let next = me.future().text.clone();
                if next == "*" {
                    me.pop_token();
                    Ok(args.into_iter().nth(0).unwrap())
                } else {
                    Ok(args.into_iter().nth(1).unwrap())
                }
            }),
        );

        // \TextOrMath{text-branch}{math-branch}: choose based on mode
        self.macros.set(
            "\\TextOrMath".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let args = me.consume_args(2)?;
                if me.mode == Mode::Text {
                    Ok(args.into_iter().nth(0).unwrap())
                } else {
                    Ok(args.into_iter().nth(1).unwrap())
                }
            }),
        );

        // AMSMath's contextual \dots selection. It expands the following token
        // once before classifying it, matching KaTeX's expandAfterFuture().
        self.macros.set(
            "\\dots".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let _guard = me
                    .depth_budget
                    .enter()
                    .map_err(|_| ParseError::recursion_limit_exceeded())?;
                me.expand_once(false)?;
                let next = me.future().text.clone();
                lex_string_to_stack_tokens(contextual_dots_for(&next), &me.allocation_budget)
            }),
        );

        // \dotso and \cdots add a thin space before punctuation/right
        // delimiters. \dotsc intentionally excludes a following comma.
        self.macros.set(
            "\\dotso".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let next = me.future().text.clone();
                let text = if dotso_space_after(&next) {
                    "\\ldots\\,"
                } else {
                    "\\ldots"
                };
                lex_string_to_stack_tokens(text, &me.allocation_budget)
            }),
        );

        self.macros.set(
            "\\dotsc".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let next = me.future().text.clone();
                let text = if dotsc_space_after(&next) {
                    "\\ldots\\,"
                } else {
                    "\\ldots"
                };
                lex_string_to_stack_tokens(text, &me.allocation_budget)
            }),
        );

        self.macros.set(
            "\\cdots".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let next = me.future().text.clone();
                let text = if dotso_space_after(&next) {
                    "\\@cdots\\,"
                } else {
                    "\\@cdots"
                };
                lex_string_to_stack_tokens(text, &me.allocation_budget)
            }),
        );

        // \html@mathml is registered as a function in htmlmathml.rs

        // \newcommand{\name}[nargs]{body}
        self.macros.set(
            "\\newcommand".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                handle_newcommand(me, false, true)
            }),
        );

        // \renewcommand{\name}[nargs]{body}
        self.macros.set(
            "\\renewcommand".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                handle_newcommand(me, true, false)
            }),
        );

        // \providecommand{\name}[nargs]{body}
        self.macros.set(
            "\\providecommand".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                handle_newcommand(me, true, true)
            }),
        );

        // \char: parse decimal/octal/hex/backtick number → \@char{N}
        self.macros.set(
            "\\char".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let mut tok = me.pop_token();
                let mut number: i64;
                let base: Option<u32>;

                if tok.text == "'" {
                    base = Some(8);
                    tok = me.pop_token();
                } else if tok.text == "\"" {
                    base = Some(16);
                    tok = me.pop_token();
                } else if tok.text == "`" {
                    tok = me.pop_token();
                    if tok.text.starts_with('\\') {
                        number = tok.text.chars().nth(1).map_or(0, |c| c as i64);
                    } else {
                        number = tok.text.chars().next().map_or(0, |c| c as i64);
                    }
                    let text = me
                        .allocation_budget
                        .format(format_args!("\\@char{{{number}}}"))?;
                    let mut tokens = lex_string_to_stack_tokens(&text, &me.allocation_budget)?;
                    for t in &mut tokens {
                        t.loc = tok.loc.clone();
                    }
                    return Ok(tokens);
                } else {
                    base = Some(10);
                }

                if let Some(b) = base {
                    number = i64::from_str_radix(&tok.text, b).unwrap_or(0);
                    loop {
                        let next = me.future().text.clone();
                        if let Ok(d) = i64::from_str_radix(&next, b) {
                            me.pop_token();
                            number = number
                                .checked_mul(i64::from(b))
                                .and_then(|n| n.checked_add(d))
                                .ok_or_else(|| ParseError::msg("\\char number out of range"))?;
                        } else {
                            break;
                        }
                    }
                } else {
                    number = 0;
                }

                let text = me
                    .allocation_budget
                    .format(format_args!("\\@char{{{number}}}"))?;
                let mut tokens = lex_string_to_stack_tokens(&text, &me.allocation_budget)?;
                for t in &mut tokens {
                    t.loc = tok.loc.clone();
                }
                Ok(tokens)
            }),
        );

        // \operatorname: \@ifstar\operatornamewithlimits\operatorname@
        self.macros.set(
            "\\operatorname".to_string(),
            MacroDefinition::Text("\\@ifstar\\operatornamewithlimits\\operatorname@".to_string()),
        );

        // \message{...}: consume argument and discard (no-op)
        self.macros.set(
            "\\message".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let _args = me.consume_args(1)?;
                Ok(vec![])
            }),
        );

        // \errmessage{...}: consume argument and discard (no-op)
        self.macros.set(
            "\\errmessage".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let _args = me.consume_args(1)?;
                Ok(vec![])
            }),
        );

        // KaTeX HTML extensions: no-op (only render content, no HTML attributes).
        // Not standard LaTeX; for compatibility we parse and expand to second argument only.
        // \htmlStyle is registered as a real function so the renderer can honor basic CSS.
        for name in &["\\htmlClass", "\\htmlData", "\\htmlId"] {
            let name = (*name).to_string();
            self.macros.set(
                name.clone(),
                MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let args = me.consume_args(2)?;
                    let content = args
                        .into_iter()
                        .nth(1)
                        .unwrap()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>();
                    Ok(content)
                }),
            );
        }

        self.macros.set(
            "\\bra@ket".to_string(),
            MacroDefinition::Function(|me| me.expand_braket(false)),
        );
        self.macros.set(
            "\\bra@set".to_string(),
            MacroDefinition::Function(|me| me.expand_braket(true)),
        );

        // \\ce / \\pu: KaTeX mhchem 3.3.0 (Rust port in `crate::mhchem`)
        self.macros.set(
            "\\ce".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let args = me.consume_args(1)?;
                let s = crate::mhchem::arg_tokens_with_resources(&args[0], &me.allocation_budget)
                    .map_err(|e| ParseError::msg(e.to_string()))?;
                let tex = crate::mhchem::chem_parse_str_with_resources(
                    &s,
                    "ce",
                    me.depth_budget.clone(),
                    me.allocation_budget.clone(),
                )
                .map_err(|e| ParseError::msg(format!("\\ce: {e}")))?;
                lex_string_to_stack_tokens(&tex, &me.allocation_budget)
            }),
        );
        self.macros.set(
            "\\pu".to_string(),
            MacroDefinition::Function(|me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                let args = me.consume_args(1)?;
                let s = crate::mhchem::arg_tokens_with_resources(&args[0], &me.allocation_budget)
                    .map_err(|e| ParseError::msg(e.to_string()))?;
                let tex = crate::mhchem::chem_parse_str_with_resources(
                    &s,
                    "pu",
                    me.depth_budget.clone(),
                    me.allocation_budget.clone(),
                )
                .map_err(|e| ParseError::msg(format!("\\pu: {e}")))?;
                lex_string_to_stack_tokens(&tex, &me.allocation_budget)
            }),
        );
    }

    pub fn set_macro(&mut self, name: String, def: MacroDefinition) {
        if let Some(control) = self.execution.filter(|control| control.storage().is_some()) {
            control.fail(ExecutionError::Unsupported {
                phase: Phase::Macro,
            });
            return;
        }
        self.macros.set(name, def);
    }

    pub fn set_macro_global(&mut self, name: String, def: MacroDefinition) {
        if let Some(control) = self.execution.filter(|control| control.storage().is_some()) {
            control.fail(ExecutionError::Unsupported {
                phase: Phase::Macro,
            });
            return;
        }
        self.macros.set_global(name, def);
    }

    pub fn set_text_macro(&mut self, name: &str, text: &str) {
        if let Some(control) = self.execution.filter(|control| control.storage().is_some()) {
            control.fail(ExecutionError::Unsupported {
                phase: Phase::Macro,
            });
            return;
        }
        self.macros
            .set(name.to_string(), MacroDefinition::Text(text.to_string()));
    }

    pub fn get_macro(&self, name: &str) -> Option<&MacroDefinition> {
        self.macros.get(name)
    }

    /// Expand a list of tokens fully (for \edef/\xdef).
    pub fn expand_tokens(&mut self, tokens: Vec<Token>) -> ParseResult<Vec<Token>> {
        if let Some(control) = self.execution.filter(|control| control.storage().is_some()) {
            return Err(ParseError::control(control.fail(
                ExecutionError::Unsupported {
                    phase: Phase::Macro,
                },
            )));
        }
        self.checkpoint(Phase::Macro)?;
        let nested = self.expand_tokens_depth > 0;
        let _guard = if nested {
            Some(
                self.depth_budget
                    .enter()
                    .map_err(|_| ParseError::recursion_limit_exceeded())?,
            )
        } else {
            None
        };

        self.expand_tokens_depth += 1;
        let saved_stack = std::mem::take(&mut self.stack);
        self.stack = tokens;

        let result: ParseResult<Vec<Token>> = (|| {
            let mut result = Vec::new();
            loop {
                if self.stack.is_empty() {
                    break;
                }
                let expanded = self.expand_once(false)?;
                if !expanded {
                    if let Some(tok) = self.stack.pop() {
                        if tok.is_eof() {
                            break;
                        }
                        result.push(tok);
                    }
                }
            }
            result.reverse();
            Ok(result)
        })();

        self.stack = saved_stack;
        self.expand_tokens_depth -= 1;
        result
    }

    pub fn switch_mode(&mut self, new_mode: Mode) {
        self.mode = new_mode;
    }

    pub fn begin_group(&mut self) {
        if let Some(control) = self.execution.filter(|control| control.storage().is_some()) {
            if self.bounded_groups >= 17 {
                let storage = control.storage().unwrap();
                storage.fail(ratex_types::storage::StorageError::DepthLimit {
                    phase: Phase::Macro,
                    limit: 16,
                    requested: self.bounded_groups as u32,
                });
            } else {
                self.bounded_groups += 1;
            }
            return;
        }
        self.macros.begin_group();
    }

    pub fn end_group(&mut self) {
        if self
            .execution
            .is_some_and(|control| control.storage().is_some())
        {
            self.bounded_groups = self.bounded_groups.saturating_sub(1);
            return;
        }
        self.macros.end_group();
    }

    pub fn end_groups(&mut self) {
        if self
            .execution
            .is_some_and(|control| control.storage().is_some())
        {
            self.bounded_groups = 0;
            return;
        }
        self.macros.end_groups();
    }

    /// Returns the topmost token on the stack, without expanding it.
    pub fn future(&mut self) -> &Token {
        if self.checkpoint(Phase::Macro).is_err() {
            self.stack.clear();
            // Bounded stack was funded with 4096 slots at admission. The failed-token
            // placeholder borrows no text, creates no allocation and cannot publish: the
            // sticky failure is checked by every result-bearing parser/expander loop.
            let text = if self
                .execution
                .is_some_and(|control| control.storage().is_some())
            {
                String::new()
            } else {
                "EOF".to_owned()
            };
            self.stack
                .push(Token::new(text, self.lexer.pos(), self.lexer.pos()));
            return self.stack.last().unwrap();
        }
        if self.stack.is_empty() {
            let tok = self.lexer.lex();
            self.push_token(tok);
            if self.stack.is_empty() {
                // Only possible after a sticky bounded failure; reserved capacity remains.
                self.stack.push(Token::new(
                    String::new(),
                    self.lexer.pos(),
                    self.lexer.pos(),
                ));
            }
        }
        self.stack.last().unwrap()
    }

    /// Remove and return the next unexpanded token.
    pub fn pop_token(&mut self) -> Token {
        self.future();
        match self.stack.pop() {
            Some(token) => token,
            None => {
                // A recorded control/storage error remains the first error. This sentinel
                // contains no allocation and cannot pass a result-bearing checkpoint.
                if let Some(control) = self.execution {
                    if control.failure().is_none()
                        && control
                            .storage()
                            .is_none_or(|storage| storage.failure().is_none())
                    {
                        control.fail(ExecutionError::InternalInvariant {
                            phase: Phase::Macro,
                        });
                    }
                }
                Token::new(String::new(), self.lexer.pos(), self.lexer.pos())
            }
        }
    }

    /// Modify the top token's text on the stack (for \global prefix handling).
    pub fn set_top_text(&mut self, text: String) {
        self.future();
        if let Some(tok) = self.stack.last_mut() {
            tok.text = text;
        }
    }

    /// Push a token onto the stack.
    pub fn push_token(&mut self, token: Token) {
        if let Some(storage) = self.execution.and_then(ExecutionControl::storage) {
            let _ = storage.try_push(
                Phase::Macro,
                ratex_types::storage::StorageFamily::Tokens,
                &mut self.stack,
                token,
            );
        } else {
            self.stack.push(token);
        }
    }

    /// Push multiple tokens onto the stack.
    pub fn push_tokens(&mut self, tokens: Vec<Token>) {
        for token in tokens {
            self.push_token(token);
        }
    }

    /// Consume all following space tokens, without expansion.
    pub fn consume_spaces(&mut self) {
        loop {
            let is_space = self.future().text == " ";
            if is_space {
                self.stack.pop();
            } else {
                break;
            }
        }
    }

    /// Expand the next token once, charging only newly constructed output.
    fn expand_once(&mut self, expandable_only: bool) -> ParseResult<bool> {
        let top_token = self.pop_token();
        self.checkpoint(Phase::Macro)?;
        let name = &top_token.text;
        if top_token.noexpand {
            self.push_token(top_token);
            self.checkpoint(Phase::Macro)?;
            return Ok(false);
        }
        let def = self.macros.get_shared(name);
        let Some(def) = def else {
            if expandable_only && name.starts_with('\\') && !self.is_defined(name) {
                return Err(ParseError::new(
                    format!("Undefined control sequence: {}", name),
                    Some(&top_token),
                ));
            }
            self.push_token(top_token);
            self.checkpoint(Phase::Macro)?;
            return Ok(false);
        };
        // Function macros have historically bypassed active-character checks.
        if !matches!(&def.definition, MacroDefinition::Function(_)) && name.len() == 1 {
            let code = self.lexer_catcode(name.chars().next().unwrap());
            if code != 0 && code != 13 {
                self.push_token(top_token);
                self.checkpoint(Phase::Macro)?;
                return Ok(false);
            }
        }
        self.count_expansion(1)?;
        let tokens = match &def.definition {
            MacroDefinition::Function(handler) => {
                // The first elementary slice does not enter unclosed function-macro builders.
                if let Some(control) = self.execution.filter(|control| control.storage().is_some())
                {
                    if name == "\\char" && top_token.is_builtin_literal {
                        self.expand_builtin_char()?
                    } else if name == "\\cdots" {
                        let expansion = if dotso_space_after(&self.future().text) {
                            "\\@cdots\\,"
                        } else {
                            "\\@cdots"
                        };
                        Template::BuiltinText(expansion).expand(
                            Vec::new(),
                            &self.allocation_budget,
                            self.execution,
                        )?
                    } else if top_token.is_builtin_literal
                        && matches!(name.as_str(), "\\TextOrMath" | "\\@ifstar")
                    {
                        // Consume both arguments through the job ledger and move the selected
                        // tokens without changing their provenance.
                        let args = self.consume_args(2)?;
                        let select_first = if name == "\\@ifstar" {
                            let starred = self.future().text == "*";
                            if starred {
                                self.pop_token();
                            }
                            starred
                        } else {
                            self.mode == Mode::Text
                        };
                        let tokens = args
                            .into_iter()
                            .nth(usize::from(!select_first))
                            .ok_or_else(|| {
                                ParseError::control(control.fail(
                                    ExecutionError::InternalInvariant {
                                        phase: Phase::Macro,
                                    },
                                ))
                            })?;
                        self.checkpoint(Phase::Macro)?;
                        tokens
                    } else {
                        return Err(ParseError::control(control.fail(
                            ratex_types::execution::ExecutionError::Unsupported {
                                phase: Phase::Macro,
                            },
                        )));
                    }
                } else {
                    handler(self)?
                }
            }
            MacroDefinition::Text(text) => {
                let template = Template::Text(text);
                let args = self.consume_args(template.num_args())?;
                template.expand(args, &self.allocation_budget, self.execution)?
            }
            MacroDefinition::StaticText(text) => {
                let template = Template::BuiltinText(text);
                let args = self.consume_args(template.num_args())?;
                template.expand(args, &self.allocation_budget, self.execution)?
            }
            MacroDefinition::Tokens { tokens, num_args } => {
                if let Some(control) = self.execution.filter(|control| control.storage().is_some())
                {
                    return Err(ParseError::control(control.fail(
                        ExecutionError::Unsupported {
                            phase: Phase::Macro,
                        },
                    )));
                }
                if *num_args == 0 {
                    self.allocation_budget
                        .charge(def.token_cost, "token macro body")?;
                    tokens.clone()
                } else {
                    let args = self.consume_args(*num_args)?;
                    Template::Tokens(tokens).expand(
                        args,
                        &self.allocation_budget,
                        self.execution,
                    )?
                }
            }
        };
        self.push_tokens(tokens);
        self.checkpoint(Phase::Macro)?;
        Ok(true)
    }

    fn expand_builtin_char(&mut self) -> ParseResult<Vec<Token>> {
        let storage = self
            .execution
            .and_then(ExecutionControl::storage)
            .ok_or_else(|| ParseError::msg("Missing builtin operation control"))?;
        let mut token = self.pop_token();
        self.checkpoint(Phase::Macro)?;
        if !token.is_builtin_literal {
            return Err(ParseError::control(storage.control().fail(
                ExecutionError::Unsupported {
                    phase: Phase::Macro,
                },
            )));
        }
        let number = if token.text == "`" {
            token = self.pop_token();
            self.checkpoint(Phase::Macro)?;
            if !token.is_builtin_literal {
                return Err(ParseError::control(storage.control().fail(
                    ExecutionError::Unsupported {
                        phase: Phase::Macro,
                    },
                )));
            }
            if token.text.starts_with('\\') {
                token.text.chars().nth(1).map_or(0, |c| c as i64)
            } else {
                token.text.chars().next().map_or(0, |c| c as i64)
            }
        } else {
            let base = if token.text == "\"" {
                token = self.pop_token();
                16
            } else if token.text == "'" {
                token = self.pop_token();
                8
            } else {
                10
            };
            self.checkpoint(Phase::Macro)?;
            if !token.is_builtin_literal {
                return Err(ParseError::control(storage.control().fail(
                    ExecutionError::Unsupported {
                        phase: Phase::Macro,
                    },
                )));
            }
            let mut number = i64::from_str_radix(&token.text, base)
                .map_err(|_| ParseError::msg("Invalid fixed builtin character"))?;
            loop {
                self.checkpoint(Phase::Macro)?;
                let next = self.future();
                let digit = i64::from_str_radix(&next.text, base).ok();
                let trusted = next.is_builtin_literal;
                self.checkpoint(Phase::Macro)?;
                let Some(digit) = digit else {
                    break;
                };
                if !trusted {
                    return Err(ParseError::control(storage.control().fail(
                        ExecutionError::Unsupported {
                            phase: Phase::Macro,
                        },
                    )));
                }
                self.pop_token();
                number = number
                    .checked_mul(base as i64)
                    .and_then(|n| n.checked_add(digit))
                    .ok_or_else(|| ParseError::msg("Fixed builtin character is out of range"))?;
            }
            number
        };
        if u32::try_from(number)
            .ok()
            .and_then(char::from_u32)
            .is_none()
        {
            return Err(ParseError::msg("Invalid fixed builtin scalar"));
        }
        let text = storage
            .try_format(Phase::Macro, format_args!("\\@char{{{number}}}"))
            .map_err(ParseError::storage)?;
        let mut result = Template::BuiltinText(&text).expand(
            Vec::new(),
            &self.allocation_budget,
            self.execution,
        )?;
        for output in &mut result {
            self.checkpoint(Phase::Macro)?;
            output.loc = token.loc.clone();
        }
        Ok(result)
    }

    fn expand_braket(&mut self, first_only: bool) -> ParseResult<Vec<Token>> {
        self.checkpoint(Phase::Macro)?;
        let mut args = self.consume_args(4)?.into_iter();
        let left = args.next().unwrap();
        let middle = args.next().unwrap();
        let double = args.next().unwrap();
        let right = args.next().unwrap();
        let content = self.consume_arg(None)?.tokens;
        let mut iter = content.into_iter().rev().peekable();
        let mut out = Vec::new();
        let mut depth = 0i32;
        let mut replaced = false;
        while let Some(t) = iter.next() {
            self.checkpoint(Phase::Macro)?;
            if t.text == "{" {
                depth += 1;
            } else if t.text == "}" {
                depth -= 1;
            } else if depth == 0 && t.text == "|" && !(first_only && replaced) {
                let replacement =
                    if !double.is_empty() && iter.peek().is_some_and(|t| t.text == "|") {
                        iter.next();
                        &double
                    } else {
                        &middle
                    };
                self.allocation_budget
                    .charge(token_cost(replacement), "braket replacement")?;
                out.extend(replacement.iter().rev().cloned());
                replaced = true;
                continue;
            }
            out.push(t);
        }
        let mut to_expand = right;
        to_expand.extend(out.into_iter().rev());
        to_expand.extend(left);
        self.begin_group();
        let result = self.expand_tokens(to_expand);
        self.end_group();
        result
    }

    /// Alias the immutable definition without copying a macro body.
    pub(crate) fn alias_macro(&mut self, name: &str, source: &str, global: bool) -> bool {
        if let Some(control) = self.execution.filter(|control| control.storage().is_some()) {
            control.fail(ExecutionError::Unsupported {
                phase: Phase::Macro,
            });
            return false;
        }
        let Some(def) = self.macros.get_shared(source) else {
            return false;
        };
        if global {
            self.macros.current.insert(name.to_string(), def);
        } else {
            self.macros.set_shared(name.to_string(), def);
        }
        true
    }

    fn lexer_catcode(&self, ch: char) -> u8 {
        self.lexer.get_catcode(ch)
    }

    fn count_expansion(&mut self, amount: usize) -> ParseResult<()> {
        if let Some(control) = self.execution {
            control
                .consume(Phase::Macro, Resource::MacroExpansions, amount as u64)
                .map_err(ParseError::control)?;
        }
        self.expansion_count = self
            .expansion_count
            .checked_add(amount)
            .ok_or_else(|| ParseError::msg("Too many expansions"))?;
        self.allocation_budget.calls(self.expansion_count);
        if self.expansion_count > self.max_expand {
            Err(ParseError::msg(
                "Too many expansions: infinite loop or need to increase maxExpand setting",
            ))
        } else {
            Ok(())
        }
    }

    /// Recursively expand the next token until a non-expandable token is found.
    pub fn expand_next_token(&mut self) -> ParseResult<Token> {
        self.checkpoint(Phase::Macro)?;
        loop {
            self.checkpoint(Phase::Macro)?;
            let expanded = self.expand_once(false)?;
            self.checkpoint(Phase::Macro)?;
            if !expanded {
                let mut token = match self.stack.pop() {
                    Some(token) => token,
                    None => {
                        let error = self.execution.map_or(
                            ExecutionError::InternalInvariant {
                                phase: Phase::Macro,
                            },
                            |control| {
                                control.fail(ExecutionError::InternalInvariant {
                                    phase: Phase::Macro,
                                })
                            },
                        );
                        return Err(ParseError::control(error));
                    }
                };
                if token.treat_as_relax {
                    token.text =
                        if let Some(storage) = self.execution.and_then(ExecutionControl::storage) {
                            storage
                                .try_string(Phase::Macro, "\\relax")
                                .map_err(ParseError::storage)?
                        } else {
                            self.allocation_budget.text("\\relax")?
                        };
                }
                return Ok(token);
            }
        }
    }

    /// Consume a single argument from the token stream.
    pub fn consume_arg(&mut self, delims: Option<&[&str]>) -> ParseResult<ConsumedArg> {
        self.checkpoint(Phase::Macro)?;
        let is_delimited = delims.is_some_and(|d| !d.is_empty());
        if !is_delimited {
            self.consume_spaces();
        }

        let execution = self.execution;
        let start = ratex_types::storage::clone_controlled(
            execution,
            Phase::Macro,
            ratex_types::storage::StorageFamily::Tokens,
            self.future(),
        )
        .map_err(ParseError::storage)?;
        let mut tokens = Vec::new();
        let mut depth: i32 = 0;
        let end_tok;

        loop {
            self.checkpoint(Phase::Macro)?;
            let tok = self.pop_token();

            if tok.text == "{" {
                if let Some(storage) = self.execution.and_then(ExecutionControl::storage) {
                    let requested = self
                        .depth_budget
                        .current()
                        .saturating_add(depth as usize)
                        .saturating_add(1);
                    if requested > 16 {
                        return Err(ParseError::storage(storage.fail(
                            ratex_types::storage::StorageError::DepthLimit {
                                phase: Phase::Macro,
                                limit: 16,
                                requested: requested as u32,
                            },
                        )));
                    }
                }
                depth += 1;
            } else if tok.text == "}" {
                depth -= 1;
                if depth == -1 {
                    return Err(ParseError::new("Extra }", Some(&tok)));
                }
            } else if tok.is_eof() {
                return Err(ParseError::new(
                    "Unexpected end of input in a macro argument",
                    Some(&tok),
                ));
            }

            if depth == 0 && !is_delimited {
                end_tok = ratex_types::storage::clone_controlled(
                    self.execution,
                    Phase::Macro,
                    ratex_types::storage::StorageFamily::Tokens,
                    &tok,
                )
                .map_err(ParseError::storage)?;
                self.pushed(&mut tokens, tok)?;
                break;
            }

            if let Some(delims) = delims {
                if is_delimited && depth == 0 {
                    if let Some(last) = delims.last() {
                        if tok.text == *last {
                            end_tok = tok;
                            break;
                        }
                    }
                }
            }
            self.pushed(&mut tokens, tok)?;
        }

        if start.text == "{" && tokens.last().is_some_and(|t| t.text == "}") {
            tokens.pop();
            tokens.remove(0);
        }

        tokens.reverse();

        Ok(ConsumedArg {
            tokens,
            start,
            end: end_tok,
        })
    }

    /// Consume N arguments.
    fn consume_args(&mut self, num_args: usize) -> ParseResult<Vec<Vec<Token>>> {
        let mut args = Vec::new();
        for _ in 0..num_args {
            self.checkpoint(Phase::Macro)?;
            let arg = self.consume_arg(None)?;
            self.pushed(&mut args, arg.tokens)?;
        }
        Ok(args)
    }

    /// Scan a function argument (optional or mandatory).
    /// Pushes an EOF token to mark the end, then pushes the argument tokens.
    pub fn scan_argument(&mut self, is_optional: bool) -> ParseResult<Option<Token>> {
        self.checkpoint(Phase::Macro)?;
        if is_optional {
            self.consume_spaces();
            let opening = self.future().text == "[";
            self.checkpoint(Phase::Macro)?;
            if !opening {
                return Ok(None);
            }
            let start = self.pop_token();
            let arg = self.consume_arg(Some(&["]"]))?;
            let end = &arg.end;
            let end_loc = end.loc.clone();

            let eof = self.token("EOF", end_loc.start, end_loc.end)?;
            self.push_token(eof);
            self.push_tokens(arg.tokens);
            self.checkpoint(Phase::Macro)?;

            let result = Token {
                is_builtin_literal: false,
                text: String::new(),
                loc: SourceLocation::range(&start.loc, &end_loc),
                noexpand: false,
                treat_as_relax: false,
            };
            Ok(Some(result))
        } else {
            let arg = self.consume_arg(None)?;
            let end_loc = arg.end.loc.clone();

            let eof = self.token("EOF", end_loc.start, end_loc.end)?;
            self.push_token(eof);
            self.push_tokens(arg.tokens);
            self.checkpoint(Phase::Macro)?;

            let result = Token {
                is_builtin_literal: false,
                text: String::new(),
                loc: SourceLocation::range(&arg.start.loc, &end_loc),
                noexpand: false,
                treat_as_relax: false,
            };
            Ok(Some(result))
        }
    }

    /// Check if a command name is currently defined.
    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.has(name)
            || FUNCTIONS.contains_key(name)
            || is_known_symbol(name)
            || IMPLICIT_COMMANDS.contains(&name)
    }

    /// Check if a command is expandable.
    pub fn is_expandable(&self, name: &str) -> bool {
        if let Some(_def) = self.macros.get(name) {
            return true;
        }
        if let Some(func) = FUNCTIONS.get(name) {
            return !func.primitive;
        }
        false
    }
}

pub struct ConsumedArg {
    pub tokens: Vec<Token>,
    pub start: Token,
    pub end: Token,
}

fn handle_newcommand(
    me: &mut MacroExpander,
    exists_ok: bool,
    nonexists_ok: bool,
) -> ParseResult<Vec<Token>> {
    let name_arg = me.consume_arg(None)?;
    // name_arg.tokens is reversed (stack order); last element = first token in original
    let name = name_arg
        .tokens
        .last()
        .map_or_else(String::new, |t| t.text.clone());

    let exists = me.is_defined(&name);
    if exists && !exists_ok {
        return Err(ParseError::msg(format!(
            "\\newcommand{{{}}} attempting to redefine {}; use \\renewcommand",
            name, name
        )));
    }
    if !exists && !nonexists_ok {
        return Err(ParseError::msg(format!(
            "\\renewcommand{{{}}} when command {} does not yet exist; use \\newcommand",
            name, name
        )));
    }

    me.consume_spaces();
    let mut num_args = 0usize;
    if me.future().text == "[" {
        me.pop_token();
        let narg_tok = me.pop_token();
        num_args = narg_tok.text.parse().unwrap_or(0);
        let close = me.pop_token();
        if close.text != "]" {
            return Err(ParseError::msg("Expected ] in \\newcommand"));
        }
    }

    let body_arg = me.consume_arg(None)?;
    let tokens = body_arg.tokens;

    me.set_macro(name, MacroDefinition::Tokens { tokens, num_args });
    Ok(vec![])
}

fn is_known_symbol(name: &str) -> bool {
    use ratex_font::symbols;
    symbols::get_symbol(name, symbols::Mode::Math).is_some()
        || symbols::get_symbol(name, symbols::Mode::Text).is_some()
}

#[cfg(test)]
mod resource_tests;
