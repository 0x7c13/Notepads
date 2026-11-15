// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Fixed lexical assets only. Syntect's parser/ops/scope/regex allocations remain third-party
//! temporaries/lazy caches, not root-metered actual bytes or a hard RSS bound. No dynamic grammar,
//! dynamic scope name, extra SyntaxSet copy, theme engine, or per-block shared regex cache is added.

use super::storage::GrowingRows;
use super::work::{Monitor, Work};
use super::{FailureReason, Language, SemanticKind, TokenSpan};
use crate::error::CoreError;
use std::sync::OnceLock;
use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{
    ExecutionControl, ExecutionError, ParseState, ParsingError, RegexPhase, Scope, ScopeStack,
    SyntaxReference, SyntaxSet,
};

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static PREFIXES: OnceLock<[(Scope, SemanticKind); 37]> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(|| {
        syntect::dumps::from_uncompressed_data(include_bytes!(
            "../../assets/code-syntaxes/highlight.packdump"
        ))
        .expect("fixed Notepads syntax pack")
    })
}

#[cfg(test)]
pub(super) fn default_syntaxes_for_test() -> &'static SyntaxSet {
    syntax_set()
}

fn prefixes() -> &'static [(Scope, SemanticKind)] {
    // One fixed, finite initialization. User text never becomes a Scope name.
    PREFIXES.get_or_init(|| {
        [
            ("comment", SemanticKind::Comment),
            ("string", SemanticKind::String),
            ("constant.character", SemanticKind::String),
            ("constant.numeric", SemanticKind::Number),
            ("keyword", SemanticKind::Keyword),
            ("storage.modifier", SemanticKind::Keyword),
            ("storage.type", SemanticKind::Type),
            ("support.type", SemanticKind::Type),
            ("support.class.cocoa.objc", SemanticKind::Type),
            ("support.class.latex", SemanticKind::Type),
            ("entity.name.type", SemanticKind::Type),
            ("entity.name.class", SemanticKind::Type),
            ("entity.name.function", SemanticKind::Function),
            ("support.function", SemanticKind::Function),
            ("variable.function", SemanticKind::Function),
            ("entity.name.tag", SemanticKind::Type),
            ("entity.other.attribute-name.html", SemanticKind::Keyword),
            (
                "entity.other.attribute-name.localname.xml",
                SemanticKind::Keyword,
            ),
            ("constant.language.boolean.yaml", SemanticKind::Keyword),
            ("constant.language.boolean.true", SemanticKind::Keyword),
            ("constant.language.boolean.false", SemanticKind::Keyword),
            ("constant.language.toml", SemanticKind::Keyword),
            ("entity.name.table.toml", SemanticKind::Type),
            ("entity.other.attribute-name.tsx", SemanticKind::Keyword),
            ("entity.name.section.markdown", SemanticKind::Type),
            ("entity.name.section.latex", SemanticKind::Type),
            (
                "punctuation.section.embedded.begin.php",
                SemanticKind::Keyword,
            ),
            (
                "punctuation.section.embedded.end.php",
                SemanticKind::Keyword,
            ),
            (
                "punctuation.definition.heading.begin.markdown",
                SemanticKind::Type,
            ),
            ("markup.bold.markdown", SemanticKind::Keyword),
            ("markup.italic.markdown", SemanticKind::Keyword),
            ("markup.raw.inline.markdown", SemanticKind::String),
            ("markup.underline.link.markdown", SemanticKind::String),
            (
                "meta.other.directive.restructuredtext",
                SemanticKind::Keyword,
            ),
            ("markup.raw.restructuredtext", SemanticKind::String),
            ("markup.inserted.diff", SemanticKind::Added),
            ("markup.deleted.diff", SemanticKind::Removed),
        ]
        .map(|(name, kind)| {
            (
                Scope::new(name).expect("fixed valid finite semantic scope"),
                kind,
            )
        })
    })
}

fn kind(stack: &ScopeStack) -> SemanticKind {
    for scope in stack.as_slice().iter().rev() {
        if let Some((_, kind)) = prefixes()
            .iter()
            .find(|(prefix, _)| prefix.is_prefix_of(*scope))
        {
            return *kind;
        }
    }
    SemanticKind::Plain
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Outcome {
    Complete,
    SpanLimit,
    Failed(FailureReason),
}

fn starts_with_php_tag(code: &str, work: &Work<'_>) -> Result<bool, CoreError> {
    let mut remaining = code.strip_prefix('\u{feff}').unwrap_or(code).as_bytes();
    while !remaining.is_empty() {
        let chunk = &remaining[..remaining.len().min(4096)];
        // The opener can straddle two chunks. Account for the one-byte lookahead.
        let inspected = remaining.len().min(chunk.len() + 1);
        work.with_op(|op| op.external_read(inspected as u64))?;
        if let Some(start) = chunk.iter().position(|byte| !byte.is_ascii_whitespace()) {
            return Ok(remaining[start..].starts_with(b"<?"));
        }
        remaining = &remaining[chunk.len()..];
    }
    Ok(false)
}

pub(super) fn tokenize(
    language: Language,
    code: &str,
    expected_utf16: u32,
    work: &Work<'_>,
    spans: &mut GrowingRows<TokenSpan>,
) -> Result<Outcome, CoreError> {
    work.checkpoint(0)?;
    let set = syntax_set();
    let _ = prefixes();
    work.checkpoint(0)?;
    // The bundled PHP grammar starts in HTML mode. Use its PHP-only grammar
    // for snippets whose opening delimiter is omitted.
    let syntax = if language == Language::Php && !starts_with_php_tag(code, work)? {
        set.find_syntax_by_name("PHP Source")
    } else {
        set.find_syntax_by_extension(language.extension())
    };
    let Some(syntax) = syntax else {
        return work.with_op(|op| Err(op.fail(CoreError::InternalInvariant)));
    };
    tokenize_with_syntax(set, syntax, code, expected_utf16, work, spans)
}

pub(super) fn tokenize_with_syntax(
    set: &SyntaxSet,
    syntax: &SyntaxReference,
    code: &str,
    expected_utf16: u32,
    work: &Work<'_>,
    spans: &mut GrowingRows<TokenSpan>,
) -> Result<Outcome, CoreError> {
    work.checkpoint(0)?;
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    work.checkpoint(0)?;
    let mut monitor = Monitor(work);
    let mut control = ExecutionControl::new(&mut monitor);
    let base = spans.len();
    let mut position = 0u32;
    for line in syntect::util::LinesWithEndings::from(code) {
        let ops = match state.parse_line_with_control(line, set, &mut control) {
            Ok(ops) => ops,
            Err(ParsingError::Execution(ExecutionError::Regex { phase, source: _ })) => {
                // The vendor returned the actual typed backend error; persist a bounded phase,
                // not an unbudgeted error string or user code. Never latch it on the body op.
                return Ok(Outcome::Failed(match phase {
                    RegexPhase::Compile => FailureReason::RegexCompile,
                    RegexPhase::Search => FailureReason::RegexSearch,
                }));
            }
            Err(ParsingError::Execution(_)) => {
                return Err(work.fault().unwrap_or(CoreError::InternalInvariant));
            }
            Err(_) => return Ok(Outcome::Failed(FailureReason::Grammar)),
        };
        for (region, operation) in ScopeRegionIterator::new(&ops, line) {
            work.checkpoint(1)?;
            if stack.apply(operation).is_err() {
                return Ok(Outcome::Failed(FailureReason::Scope));
            }
            if region.is_empty() {
                continue;
            }
            work.with_op(|op| op.external_read(region.len() as u64))?;
            let length = u32::try_from(region.encode_utf16().count())
                .map_err(|_| CoreError::ArithmeticOverflow)?;
            let next = position
                .checked_add(length)
                .ok_or(CoreError::ArithmeticOverflow)?;
            let semantic = kind(&stack);
            let merge = spans.len() > base
                && spans.values().last().is_some_and(|last| {
                    last.kind == semantic
                        && last.start_utf16.checked_add(last.length_utf16) == Some(position)
                });
            if merge {
                work.with_op(|op| op.external_write(size_of::<TokenSpan>() as u64))?;
                let last = spans.last_mut().ok_or(CoreError::InternalInvariant)?;
                last.length_utf16 = next - last.start_utf16;
            } else if !work.with_op(|op| {
                spans.push(
                    TokenSpan {
                        start_utf16: position,
                        length_utf16: length,
                        kind: semantic,
                    },
                    op,
                )
            })? {
                return Ok(Outcome::SpanLimit);
            }
            position = next;
        }
    }
    work.checkpoint(0)?;
    if position != expected_utf16 {
        return work.with_op(|op| Err(op.fail(CoreError::InvalidProvenance)));
    }
    Ok(Outcome::Complete)
}
