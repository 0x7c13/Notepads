// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Fixed upstream builtin expressions; last registration wins, no LazyLock/heap initialization.
use super::*;
pub(super) struct BuiltinRegistry;
pub(super) static BUILTINS: BuiltinRegistry = BuiltinRegistry;
static ROWS: &[(&str, StoredMacro)] = &[
    (
        "\\bgroup",
        StoredMacro {
            definition: MacroDefinition::StaticText("{"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\egroup",
        StoredMacro {
            definition: MacroDefinition::StaticText("}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lq",
        StoredMacro {
            definition: MacroDefinition::StaticText("`"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\rq",
        StoredMacro {
            definition: MacroDefinition::StaticText("'"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\aa",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\r a"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\AA",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\r A"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "~",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\nobreakspace"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\hphantom",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\smash{\\phantom{#1}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\not",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\mathrlap\\@not}\\nobreak}{\\char\"338}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\neq",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\not=}}{\\mathrel{\\char`≠}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ne",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\neq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2260}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\neq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\notin",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{{\\in}\\mathllap{/\\mskip1mu}}}{\\mathrel{\\char`∉}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2209}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\notin"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\notni",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\not\\ni}{\\mathrel{\\char`\u{220C}}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{220C}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\notni"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\iff",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\;\\Longleftrightarrow\\;"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\implies",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\;\\Longrightarrow\\;"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\impliedby",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\;\\Longleftarrow\\;"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varGamma",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Gamma}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varDelta",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Delta}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varTheta",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Theta}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varLambda",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Lambda}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varXi",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Xi}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varPi",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Pi}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varSigma",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Sigma}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varUpsilon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Upsilon}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varPhi",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Phi}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varPsi",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Psi}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varOmega",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathit{\\Omega}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\,",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\TextOrMath{\\kern{.1667em}}{\\mskip{3mu}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\thinspace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\,"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\>",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mskip{4mu}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\:",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\TextOrMath{\\kern{.2222em}}{\\mskip{4mu}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\medspace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\:"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\;",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\TextOrMath{\\kern{.2777em}}{\\mskip{5mu}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\thickspace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\;"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\!",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\TextOrMath{\\kern{-.1667em}}{\\mskip{-3mu}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\negthinspace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\!"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\negmedspace",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\TextOrMath{\\kern{-.2222em}}{\\mskip{-4mu}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\negthickspace",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\TextOrMath{\\kern{-.2777em}}{\\mskip{-5mu}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\enspace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\kern.5em "),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\enskip",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\hskip.5em\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\quad",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\hskip1em\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\qquad",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\hskip2em\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\newline",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\\\\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\@hspace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\hskip #1\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\@hspacer",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\rule{0pt}{0pt}\\hskip #1\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\llap",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathllap{\\textrm{#1}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\rlap",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrlap{\\textrm{#1}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\clap",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathclap{\\textrm{#1}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\TeX",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\textrm{\\html@mathml{T\\kern-.1667em\\raisebox{-.5ex}{E}\\kern-.125emX}{TeX}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\LaTeX",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\textrm{\\html@mathml{L\\kern-.36em\\raisebox{0.21em}{\\scriptstyle A}\\kern-.15em\\TeX}{LaTeX}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\KaTeX",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\textrm{\\html@mathml{K\\kern-.17em\\raisebox{0.21em}{\\scriptstyle A}\\kern-.15em\\TeX}{KaTeX}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\imath",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\html@mathml{\\@imath}{\u{0131}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\jmath",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\html@mathml{\\@jmath}{\u{0237}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\minuso",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\mathbin{\\html@mathml{{\\mathrlap{\\mathchoice{\\kern{0.145em}}{\\kern{0.145em}}{\\kern{0.1015em}}{\\kern{0.0725em}}\\circ}{-}}}{\\char`\u{29B5}}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\clap",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathclap{\\textrm{#1}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\mathstrut",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\vphantom{(}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\underbar",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\underline{\\text{#1}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Bbbk",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Bbb{k}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\substack",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\begin{subarray}{c}#1\\end{subarray}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\boxed",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\fbox{$\\displaystyle{#1}$}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\colon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\nobreak\\mskip2mu\\mathpunct{}\\mathchoice{\\mkern-3mu}{\\mkern-3mu}{}{}{:}\\mskip6mu\\relax",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\cdots",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@cdots"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dotsb",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\cdots"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dotsm",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\cdots"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dotsi",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\!\\cdots"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dotsx",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\ldots\\,"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\DOTSI",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\DOTSB",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\DOTSX",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\gvertneqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@gvertneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lvertneqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@lvertneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ngeqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@ngeqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ngeqslant",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@ngeqslant"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\nleqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@nleqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\nleqslant",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@nleqslant"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\nshortmid",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@nshortmid"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\nshortparallel",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@nshortparallel"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\nsubseteqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@nsubseteqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\nsupseteqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@nsupseteqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ulcorner",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@ulcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\urcorner",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@urcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\llcorner",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@llcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lrcorner",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@lrcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varsubsetneq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@varsubsetneq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varsubsetneqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@varsubsetneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varsupsetneq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@varsupsetneq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varsupsetneqq",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\@varsupsetneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lBrace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathopen{\\{\\mkern-3.2mu[}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\rBrace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathclose{]\\mkern-3.2mu\\}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\llbracket",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathopen{[\\mkern-3.2mu[}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\rrbracket",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathclose{]\\mkern-3.2mu]}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\copyright",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcircled{c}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\textregistered",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcircled{\\scriptsize R}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\textcopyright",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\html@mathml{\\textcircled{c}}{\\char`©}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\tmspace",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\TextOrMath{\\kern#1#3}{\\mskip#1#2}\\relax"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2258}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrel{\u{E258}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2259}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\stackrel{\\tiny\\wedge}{=}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{225A}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\stackrel{\\tiny\\vee}{=}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{225B}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\stackrel{\\scriptsize\\star}{=}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{225D}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\stackrel{\\tiny\\mathrm{def}}{=}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{225E}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrel{\u{E25E}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{225F}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\stackrel{\\tiny?}{=}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{27C2}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\perp"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{203C}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathclose{!\\mkern-0.8mu!}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{231C}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\ulcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{231D}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\urcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{231E}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\llcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{231F}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\lrcorner"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{00A9}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\copyright"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{00AE}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textregistered"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2237}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\dblcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2239}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\eqcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2254}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\coloneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2255}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\eqqcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2A74}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Coloneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{27E6}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\llbracket"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{27E7}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\rrbracket"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2983}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\lBrace"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2984}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\rBrace"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{29B5}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\minuso"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\vdots",
        StoredMacro {
            definition: MacroDefinition::StaticText("{\\varvdots\\rule{0pt}{15pt}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{22ee}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\vdots"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\bmod",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\mathchoice{\\mskip1mu}{\\mskip1mu}{\\mskip5mu}{\\mskip5mu}\\mathbin{\\rm mod}\\mathchoice{\\mskip1mu}{\\mskip1mu}{\\mskip5mu}{\\mskip5mu}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\pod",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\allowbreak\\mathchoice{\\mkern18mu}{\\mkern8mu}{\\mkern8mu}{\\mkern8mu}(#1)",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\pmod",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\pod{{\\rm mod}\\mkern6mu#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\mod",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\allowbreak\\mathchoice{\\mkern18mu}{\\mkern12mu}{\\mkern12mu}{\\mkern12mu}{\\rm mod}\\,\\,#1",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\limsup",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\operatorname*{lim\\,sup}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\liminf",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\operatorname*{lim\\,inf}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\injlim",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\operatorname*{inj\\,lim}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\projlim",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\operatorname*{proj\\,lim}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varlimsup",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\DOTSB\\operatorname*{\\overline{\\mathrm{lim}}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varliminf",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\DOTSB\\operatorname*{\\underline{\\mathrm{lim}}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varinjlim",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\DOTSB\\operatorname*{\\underrightarrow{\\mathrm{lim}}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\varprojlim",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\DOTSB\\operatorname*{\\underleftarrow{\\mathrm{lim}}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\argmin",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\operatorname*{arg\\,min}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\argmax",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\DOTSB\\operatorname*{arg\\,max}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\plim",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\DOTSB\\mathop{\\operatorname{plim}}\\limits",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ordinarycolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(":"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\vcentcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrel{\\mathop\\ordinarycolon}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dblcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-.9mu}\\vcentcolon}}{\\mathop{\\char\"2237}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\coloneqq",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}=}}{\\mathop{\\char\"2254}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Coloneqq",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}=}}{\\mathop{\\char\"2237\\char\"3d}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\coloneq",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}\\mathrel{-}}}{\\mathop{\\char\"3a\\char\"2212}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Coloneq",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}\\mathrel{-}}}{\\mathop{\\char\"2237\\char\"2212}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\eqqcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{=\\mathrel{\\mkern-1.2mu}\\vcentcolon}}{\\mathop{\\char\"2255}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Eqqcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{=\\mathrel{\\mkern-1.2mu}\\dblcolon}}{\\mathop{\\char\"3d\\char\"2237}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\eqcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\mathrel{-}\\mathrel{\\mkern-1.2mu}\\vcentcolon}}{\\mathop{\\char\"2239}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Eqcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\mathrel{-}\\mathrel{\\mkern-1.2mu}\\dblcolon}}{\\mathop{\\char\"2212\\char\"2237}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\colonapprox",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}\\approx}}{\\mathop{\\char\"3a\\char\"2248}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Colonapprox",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}\\approx}}{\\mathop{\\char\"2237\\char\"2248}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\colonsim",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\vcentcolon\\mathrel{\\mkern-1.2mu}\\sim}}{\\mathop{\\char\"3a\\char\"223c}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Colonsim",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\mathrel{\\dblcolon\\mathrel{\\mkern-1.2mu}\\sim}}{\\mathop{\\char\"2237\\char\"223c}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ratio",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\vcentcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\coloncolon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\dblcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\colonequals",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\coloneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\coloncolonequals",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Coloneqq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\equalscolon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\eqqcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\equalscoloncolon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Eqqcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\colonminus",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\coloneq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\coloncolonminus",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Coloneq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\minuscolon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\eqcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\minuscoloncolon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Eqcolon"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\coloncolonapprox",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Colonapprox"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\coloncolonsim",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Colonsim"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\simcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\mathrel{\\sim\\mathrel{\\mkern-1.2mu}\\vcentcolon}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\simcoloncolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\mathrel{\\sim\\mathrel{\\mkern-1.2mu}\\dblcolon}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\approxcolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\mathrel{\\approx\\mathrel{\\mkern-1.2mu}\\vcentcolon}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\approxcoloncolon",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\mathrel{\\approx\\mathrel{\\mkern-1.2mu}\\dblcolon}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\bra",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathinner{\\langle{#1}|}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ket",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathinner{|{#1}\\rangle}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\braket",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathinner{\\langle{#1}\\rangle}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Braket",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\bra@ket{\\left\\langle}{\\,\\middle\\vert\\,}{\\,\\middle\\vert\\,}{\\right\\rangle}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Bra",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\left\\langle#1\\right|"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Ket",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\left|#1\\right\\rangle"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\darr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\downarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dArr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Downarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Darr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Downarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lang",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\langle"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\rang",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\rangle"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\uarr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\uparrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\uArr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Uparrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Uarr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Uparrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\N",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{N}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\R",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{R}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Z",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{Z}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\alef",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\aleph"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\alefsym",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\aleph"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Alpha",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{A}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Beta",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{B}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\bull",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\bullet"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Chi",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{X}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\clubs",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\clubsuit"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\cnums",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{C}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Complex",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{C}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Dagger",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\ddagger"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\diamonds",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\diamondsuit"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\empty",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\emptyset"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Epsilon",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{E}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Eta",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{H}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\exist",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\exists"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\harr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\leftrightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\hArr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Leftrightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Harr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Leftrightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\hearts",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\heartsuit"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\image",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Im"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\infin",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\infty"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Iota",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{I}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\isin",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\in"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Kappa",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{K}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\larr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\leftarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lArr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Leftarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Larr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Leftarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lrarr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\leftrightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\lrArr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Leftrightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Lrarr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Leftrightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Mu",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{M}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\natnums",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{N}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Nu",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{N}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Omicron",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{O}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\plusmn",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\pm"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\rarr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\rightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\rArr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Rightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Rarr",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Rightarrow"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\real",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\Re"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\reals",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{R}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Reals",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathbb{R}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Rho",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{P}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\sdot",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\cdot"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\sect",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\S"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\spades",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\spadesuit"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\sub",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\subset"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\sube",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\subseteq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\supe",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\supseteq"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Tau",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{T}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\thetasym",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\vartheta"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\weierp",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\wp"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Zeta",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathrm{Z}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\blue",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcolor{##6495ed}{#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\orange",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcolor{##ffa500}{#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\pink",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcolor{##ff00af}{#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\red",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcolor{##df0030}{#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\green",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcolor{##28ae7b}{#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\gray",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcolor{gray}{#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\purple",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\textcolor{##9d38bd}{#1}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{212C}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{B}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2130}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{E}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2131}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{F}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{210B}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{H}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2110}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{I}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2112}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{L}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2133}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{M}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{211B}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathscr{R}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{212D}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathfrak{C}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{210C}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathfrak{H}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\u{2128}",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\mathfrak{Z}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\notni",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\html@mathml{\\not\\ni}{\\mathrel{\\char`\u{220C}}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\angln",
        StoredMacro {
            definition: MacroDefinition::StaticText("{\\angl n}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\set",
        StoredMacro {
            definition: MacroDefinition::StaticText("\\bra@set{\\{\\,}{\\mid}{}{\\,\\}}"),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\Set",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\bra@set{\\left\\{\\:}{\\;\\middle\\vert\\;}{\\;\\middle\\Vert\\;}{\\:\\right\\}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\tripledash",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "{\\vphantom{-}\\raisebox{2.56mu}{$\\mkern2mu\\tiny\\text{-}\\mkern1mu\\text{-}\\mkern1mu\\text{-}\\mkern2mu$}}",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\noexpand",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let mut tok = me.pop_token();
                    if me.is_expandable(&tok.text) {
                        tok.noexpand = true;
                        tok.treat_as_relax = true;
                    }
                    Ok(vec![tok])
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\@firstoftwo",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let args = me.consume_args(2)?;
                    Ok(args.into_iter().next().unwrap())
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\@secondoftwo",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let args = me.consume_args(2)?;
                    Ok(args.into_iter().nth(1).unwrap())
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\@ifnextchar",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
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
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\@ifstar",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let args = me.consume_args(2)?;
                    let next = me.future().text.clone();
                    if next == "*" {
                        me.pop_token();
                        Ok(args.into_iter().nth(0).unwrap())
                    } else {
                        Ok(args.into_iter().nth(1).unwrap())
                    }
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\TextOrMath",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let args = me.consume_args(2)?;
                    if me.mode == Mode::Text {
                        Ok(args.into_iter().nth(0).unwrap())
                    } else {
                        Ok(args.into_iter().nth(1).unwrap())
                    }
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dots",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let _guard = me
                        .depth_budget
                        .enter()
                        .map_err(|_| ParseError::recursion_limit_exceeded())?;
                    me.expand_once(false)?;
                    let next = me.future().text.clone();
                    lex_string_to_stack_tokens(contextual_dots_for(&next), &me.allocation_budget)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dotso",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let next = me.future().text.clone();
                    let text = if dotso_space_after(&next) {
                        "\\ldots\\,"
                    } else {
                        "\\ldots"
                    };
                    lex_string_to_stack_tokens(text, &me.allocation_budget)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\dotsc",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let next = me.future().text.clone();
                    let text = if dotsc_space_after(&next) {
                        "\\ldots\\,"
                    } else {
                        "\\ldots"
                    };
                    lex_string_to_stack_tokens(text, &me.allocation_budget)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\cdots",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let next = me.future().text.clone();
                    let text = if dotso_space_after(&next) {
                        "\\@cdots\\,"
                    } else {
                        "\\@cdots"
                    };
                    lex_string_to_stack_tokens(text, &me.allocation_budget)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\newcommand",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    handle_newcommand(me, false, true)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\renewcommand",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    handle_newcommand(me, true, false)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\providecommand",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    handle_newcommand(me, true, true)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\char",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
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
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\operatorname",
        StoredMacro {
            definition: MacroDefinition::StaticText(
                "\\@ifstar\\operatornamewithlimits\\operatorname@",
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\message",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let _args = me.consume_args(1)?;
                    Ok(vec![])
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\errmessage",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let _args = me.consume_args(1)?;
                    Ok(vec![])
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\bra@ket",
        StoredMacro {
            definition: MacroDefinition::Function(|me| me.expand_braket(false)),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\bra@set",
        StoredMacro {
            definition: MacroDefinition::Function(|me| me.expand_braket(true)),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\ce",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let args = me.consume_args(1)?;
                    let s =
                        crate::mhchem::arg_tokens_with_resources(&args[0], &me.allocation_budget)
                            .map_err(|e| ParseError::msg(e.to_string()))?;
                    let tex = crate::mhchem::chem_parse_str_with_resources(
                        &s,
                        "ce",
                        me.depth_budget.clone(),
                        me.allocation_budget.clone(),
                    )
                    .map_err(|e| ParseError::msg(format!("\\ce: {e}")))?;
                    lex_string_to_stack_tokens(&tex, &me.allocation_budget)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
    (
        "\\pu",
        StoredMacro {
            definition: MacroDefinition::Function(
                |me: &mut MacroExpander| -> ParseResult<Vec<Token>> {
                    let args = me.consume_args(1)?;
                    let s =
                        crate::mhchem::arg_tokens_with_resources(&args[0], &me.allocation_budget)
                            .map_err(|e| ParseError::msg(e.to_string()))?;
                    let tex = crate::mhchem::chem_parse_str_with_resources(
                        &s,
                        "pu",
                        me.depth_budget.clone(),
                        me.allocation_budget.clone(),
                    )
                    .map_err(|e| ParseError::msg(format!("\\pu: {e}")))?;
                    lex_string_to_stack_tokens(&tex, &me.allocation_budget)
                },
            ),
            token_cost: Some(Cost {
                tokens: 0,
                bytes: 0,
                nodes: 0,
            }),
        },
    ),
];
impl BuiltinRegistry {
    pub(super) fn get(&self, name: &str) -> Option<&'static StoredMacro> {
        ROWS.iter()
            .rev()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    }
}
pub(super) const fn bounded_readonly_bytes() -> usize {
    let mut sum = size_of_val(ROWS);
    let mut index = 0;
    assert!(ROWS.len() <= 1024);
    while index < ROWS.len() {
        let (name, value) = &ROWS[index];
        sum += name.len();
        match &value.definition {
            MacroDefinition::StaticText(text) => {
                assert!(text.len() <= 65536);
                sum += text.len();
            }
            MacroDefinition::Function(_) => {}
            _ => panic!("Fixed builtin may not own runtime text or token containers"),
        }
        index += 1;
    }
    sum
}

#[cfg(test)]
mod static_builtin_tests {
    use super::*;
    #[test]
    fn fixed_text_and_unicode_match_every_original_builtin_definition() {
        let mut original = MacroExpander::new("", Mode::Math);
        // Construct only the legacy test oracle from the unchanged published literal table.
        original.macros = MacroNamespace::empty();
        original.load_builtins();
        let unsupported_attributes = ["\\htmlClass", "\\htmlId", "\\htmlData"];
        let mut actual_names = std::collections::HashSet::new();
        for (name, _) in ROWS {
            actual_names.insert(*name);
            let actual = BUILTINS.get(name).unwrap();
            let expected = original
                .macros
                .get(name)
                .unwrap_or_else(|| panic!("Unexpected {name}"));
            match (&actual.definition, expected) {
                (MacroDefinition::StaticText(actual), MacroDefinition::Text(expected)) => {
                    assert_eq!(actual.as_bytes(), expected.as_bytes(), "{name}")
                }
                (MacroDefinition::Function(_), MacroDefinition::Function(_)) => {}
                _ => panic!("Changed original builtin kind: {name}"),
            }
        }
        for name in original.macros.current.keys() {
            assert!(
                actual_names.contains(name.as_str())
                    || unsupported_attributes.contains(&name.as_str()),
                "Missing {name}"
            );
        }
        assert!(ROWS.len() <= 1024);
        assert!(ROWS.iter().all(|(name, _)| name.len() <= 128));
        assert!(ROWS.iter().all(|(_, value)| match &value.definition {
            MacroDefinition::StaticText(text) => text.len() <= 4096,
            MacroDefinition::Function(_) => true,
            _ => false,
        }));
    }
}
const _: () = assert!(ROWS.len() <= 1024);
