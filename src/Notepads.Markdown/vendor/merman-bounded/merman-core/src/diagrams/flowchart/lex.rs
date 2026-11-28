// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use super::{
    ClassAssignStmt, ClassDefStmt, ClickAction, ClickStmt, LabeledText, LexError, LinkStylePos,
    LinkStyleStmt, StyleStmt, TitleKind, ast::FlowchartClickEditorEvidence,
};
use crate::{SourceSpan, diagrams::scan::is_ecmascript_whitespace};

pub(super) fn parse_node_label_text_controlled(
    raw: &str,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<LabeledText, LexError> {
    let quoted = raw.starts_with('"') && raw.ends_with('"');

    let (text, kind) = super::text::label_text_parts(raw);

    match kind {
        TitleKind::Text => {
            // Mermaid Jison-based flowchart lexer treats these as structural tokens (PS/PE/SQE/etc)
            // and will throw parse errors if they appear inside TEXT.
            if text.contains('"')
                || text.contains('(')
                || text.contains(')')
                || text.contains('[')
                || text.contains(']')
                || text.contains('{')
                || text.contains('}')
                || text.contains('|')
            {
                return Err(fixed_or_legacy_error(
                    region,
                    "Invalid text label: contains structural characters; quote it to use them",
                ));
            }
            if contains_bound_at(raw) {
                return Err(fixed_or_legacy_error(
                    region,
                    "Invalid text label: unquoted `@` must be preceded by whitespace",
                ));
            }
        }
        TitleKind::String => {
            // Mermaid's Jison string state closes on every double quote; backslashes are literal.
            if quoted {
                let inner = raw.get(1..raw.len().saturating_sub(1)).unwrap_or_default();
                if inner.contains('"') {
                    return Err(fixed_or_legacy_error(
                        region,
                        "Invalid string label: contains nested quotes",
                    ));
                }
            }
        }
        TitleKind::Markdown => {}
    }

    Ok(LabeledText {
        text: owned_text(region, text)?,
        kind,
        span: None,
        selection: None,
    })
}

fn fixed_or_legacy_error(region: Option<&markdown_memory::RegionHandle>, message: &str) -> LexError {
    if region.is_some() {
        LexError::terminal()
    } else {
        LexError::new(message)
    }
}

pub(super) fn owned_text(
    region: Option<&markdown_memory::RegionHandle>,
    text: &str,
) -> std::result::Result<String, LexError> {
    match region {
        Some(region) => region
            .try_string(text, markdown_memory::Family::Parse)
            .map_err(|_| LexError::terminal()),
        None => Ok(text.to_owned()),
    }
}

fn contains_bound_at(raw: &str) -> bool {
    raw.char_indices().any(|(index, ch)| {
        ch == '@'
            && raw[..index]
                .chars()
                .next_back()
                .is_none_or(|previous| !super::is_ecmascript_trim_char(previous))
    })
}

pub(super) fn parse_edge_text_controlled(
    raw: &str,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<LabeledText, LexError> {
    if raw.is_empty() {
        return Err(fixed_or_legacy_error(
            region,
            "Flowchart edge label payload cannot be empty",
        ));
    }

    if raw.starts_with("\"`") {
        if raw.len() < 4 || !raw.ends_with("`\"") {
            return Err(fixed_or_legacy_error(
                region,
                "Invalid Markdown string in flowchart edge label",
            ));
        }
        let inner = &raw[2..raw.len() - 2];
        if inner.is_empty() || inner.contains(['`', '"']) {
            return Err(fixed_or_legacy_error(
                region,
                "Invalid Markdown string in flowchart edge label",
            ));
        }
    } else if raw.starts_with('"') {
        if raw.len() < 2 || !raw.ends_with('"') {
            return Err(fixed_or_legacy_error(
                region,
                "Invalid edge label: quoted strings cannot be mixed with unquoted text",
            ));
        }
        let inner = &raw[1..raw.len() - 1];
        if inner.is_empty() || inner.contains('"') {
            return Err(fixed_or_legacy_error(
                region,
                "Invalid edge label: expected exactly one quoted string token",
            ));
        }
    } else if raw.contains('"') {
        return Err(fixed_or_legacy_error(
            region,
            "Invalid edge label: quoted strings cannot be mixed with unquoted text",
        ));
    }

    let (text, kind) = super::text::label_text_parts(raw);

    Ok(LabeledText {
        text: owned_text(region, text)?,
        kind,
        span: None,
        selection: None,
    })
}

pub(super) fn parse_rect_border_label(raw: &str) -> (&'static str, &str, usize) {
    // Mermaid supports a special "rect" variant via `[|borders:...|Label]`.
    // The `[|` token is recognized lexically before FlowDB trim. Leading whitespace therefore
    // keeps this on the ordinary square-label path and is not allowed to synthesize a rect.
    let Some(rest) = raw.strip_prefix('|') else {
        return ("square", raw, 0);
    };
    let Some((prefix, label)) = rest.split_once('|') else {
        return ("square", raw, 0);
    };
    if prefix.starts_with("borders:") {
        let offset = 1 + prefix.len() + 1;
        return ("rect", label, offset);
    }
    ("square", raw, 0)
}

pub(super) fn find_unquoted_delim(input: &str, start: usize, delim: &str) -> Option<usize> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let delim_bytes = delim.as_bytes();
    let mut pos = start;

    while pos + delim_bytes.len() <= len {
        if bytes[pos..pos + delim_bytes.len()] == *delim_bytes {
            return Some(pos);
        }

        // Mermaid's flowchart lexer stays in a label-specific text state until the shape closer,
        // so newlines and semicolons inside node labels are label text rather than statement ends.
        match bytes[pos] {
            b'"' => {
                pos += 1;
                while pos < len {
                    if bytes[pos] == b'"' {
                        pos += 1;
                        break;
                    }
                    pos += 1;
                }
            }
            _ => pos += 1,
        }
    }

    None
}

fn split_first_word(s: &str) -> Option<(&str, &str)> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let first = &trimmed[..i];
    let rest = &trimmed[i..];
    Some((first, rest))
}

fn parse_styles_list(s: &str) -> Vec<String> {
    // Used by `classDef` / `style` statements. Mermaid normalizes these style tokens by trimming
    // whitespace around each comma-separated entry.
    let placeholder = "\u{0000}";
    let replaced = s.replace("\\,", placeholder);
    replaced
        .split(',')
        .map(|p| p.replace(placeholder, ","))
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn parse_linkstyle_styles_list(s: &str) -> Vec<String> {
    // Mermaid's Jison grammar preserves whitespace inside each style token (e.g. `, stroke: ...`
    // becomes `" stroke: ..."`) and downstream FlowDB joins the style list verbatim via
    // `styles.join(';')` (see `flow.jison` + `flowDb.updateLink(...)`).
    //
    // Keep the raw spacing (except for filtering out all-whitespace entries).
    let placeholder = "\u{0000}";
    let replaced = s.replace("\\,", placeholder);
    replaced
        .split(',')
        .map(|p| p.replace(placeholder, ","))
        .filter(|p| !p.trim().is_empty())
        .collect()
}

fn push_value<T>(
    region: Option<&markdown_memory::RegionHandle>,
    values: &mut Vec<T>,
    value: T,
) -> std::result::Result<(), LexError> {
    match region {
        Some(region) => region
            .try_push(values, value, markdown_memory::Family::Parse)
            .map_err(|_| LexError::terminal()),
        None => {
            values.push(value);
            Ok(())
        }
    }
}

fn string_list<'a, I>(
    region: Option<&markdown_memory::RegionHandle>,
    values: I,
) -> std::result::Result<Vec<String>, LexError>
where
    I: Iterator<Item = &'a str> + Clone,
{
    let Some(region) = region else {
        return Ok(values.map(str::to_owned).collect());
    };
    let count = values.clone().count();
    region
        .try_build_recipe(
            markdown_memory::Family::Parse,
            |plan| {
                plan.array::<String>(count)?;
                for value in values.clone() {
                    plan.array::<u8>(value.len())?;
                }
                Ok(())
            },
            |group| {
                let mut result = group.vector::<String>(count)?;
                for value in values.clone() {
                    result.push(group.string(value)?);
                }
                Ok(result)
            },
        )
        .map_err(|_| LexError::terminal())
}

fn scalar_list<T: Copy, I>(
    region: Option<&markdown_memory::RegionHandle>,
    values: I,
) -> std::result::Result<Vec<T>, LexError>
where
    I: Iterator<Item = T> + Clone,
{
    let Some(region) = region else {
        return Ok(values.collect());
    };
    let count = values.clone().count();
    region
        .try_build_recipe(
            markdown_memory::Family::Parse,
            |plan| plan.array::<T>(count),
            |group| {
                let mut result = group.vector::<T>(count)?;
                result.extend(values);
                Ok(result)
            },
        )
        .map_err(|_| LexError::terminal())
}

pub(super) fn replace_controlled(
    source: &str,
    from: &str,
    to: &str,
    region: &markdown_memory::RegionHandle,
) -> std::result::Result<String, LexError> {
    let work = source.len().checked_mul(2).ok_or_else(|| {
        region
            .scope()
            .record_failure(markdown_memory::MemoryError::ArithmeticOverflow);
        LexError::terminal()
    })?;
    region
        .scope()
        .charge_work(work)
        .map_err(|_| LexError::terminal())?;
    // Internal callers use nonempty fixed ASCII patterns. Count before allocating.
    let count = source.matches(from).count();
    let length = count
        .checked_mul(from.len())
        .and_then(|old| source.len().checked_sub(old))
        .and_then(|base| {
            count
                .checked_mul(to.len())
                .and_then(|new| base.checked_add(new))
        })
        .ok_or_else(|| {
            region
                .scope()
                .record_failure(markdown_memory::MemoryError::ArithmeticOverflow);
            LexError::terminal()
        })?;
    let mut result = region
        .try_string_capacity(length, markdown_memory::Family::Parse)
        .map_err(|_| LexError::terminal())?;
    let mut rest = source;
    for _ in 0..count {
        let Some(at) = rest.find(from) else {
            return Err(LexError::terminal());
        };
        result.push_str(&rest[..at]);
        result.push_str(to);
        rest = &rest[at + from.len()..];
    }
    result.push_str(rest);
    Ok(result)
}

fn parse_styles_list_controlled(
    source: &str,
    region: Option<&markdown_memory::RegionHandle>,
    trim: bool,
) -> std::result::Result<Vec<String>, LexError> {
    let Some(region) = region else {
        return Ok(if trim {
            parse_styles_list(source)
        } else {
            parse_linkstyle_styles_list(source)
        });
    };
    let replaced = replace_controlled(source, "\\,", "\0", region)?;
    let parts = || {
        replaced
            .split(',')
            .map(|part| if trim { part.trim() } else { part })
            .filter(|part| !part.trim().is_empty())
    };
    let count = parts().count();
    region
        .try_build_recipe(
            markdown_memory::Family::Parse,
            |plan| {
                plan.array::<String>(count)?;
                for part in parts() {
                    plan.array::<u8>(part.len())?;
                }
                Ok(())
            },
            |group| {
                let mut result = group.vector::<String>(count)?;
                for part in parts() {
                    let mut bytes = group.vector::<u8>(part.len())?;
                    bytes.extend(part.bytes().map(|byte| if byte == 0 { b',' } else { byte }));
                    // Replacing ASCII NUL with ASCII comma preserves valid UTF-8.
                    let value = String::from_utf8(bytes)
                        .map_err(|_| markdown_memory::MemoryError::InvalidState)?;
                    result.push(value);
                }
                Ok(result)
            },
        )
        .map_err(|_| LexError::terminal())
}

pub(super) fn parse_style_stmt_controlled(
    rest: &str,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<StyleStmt, LexError> {
    let Some((target, styles_raw)) = split_first_word(rest) else {
        return Err(fixed_or_legacy_error(region, "Invalid style statement"));
    };
    let styles = parse_styles_list_controlled(styles_raw, region, true)?;
    Ok(StyleStmt {
        target: owned_text(region, target.trim())?,
        target_span: None,
        styles,
        styles_text: None,
        styles_span: None,
        editor_evidence: Default::default(),
    })
}

pub(super) fn parse_classdef_stmt_controlled(
    rest: &str,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<ClassDefStmt, LexError> {
    let Some((ids_raw, styles_raw)) = split_first_word(rest) else {
        return Err(fixed_or_legacy_error(region, "Invalid classDef statement"));
    };
    let ids = string_list(
        region,
        ids_raw.split(',').map(str::trim).filter(|s| !s.is_empty()),
    )?;
    let styles = parse_styles_list_controlled(styles_raw, region, true)?;
    Ok(ClassDefStmt {
        ids,
        id_spans: Vec::new(),
        styles,
        styles_text: None,
        styles_span: None,
        editor_evidence: Default::default(),
    })
}

pub(super) fn parse_class_assign_stmt_controlled(
    rest: &str,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<ClassAssignStmt, LexError> {
    let Some((targets_raw, class_raw)) = split_first_word(rest) else {
        return Err(fixed_or_legacy_error(region, "Invalid class statement"));
    };
    let targets = string_list(
        region,
        targets_raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )?;
    let class_name = owned_text(region, class_raw.trim())?;
    if class_name.is_empty() {
        return Err(fixed_or_legacy_error(region, "Invalid class statement"));
    }
    Ok(ClassAssignStmt {
        targets,
        target_spans: Vec::new(),
        class_name,
        class_name_span: None,
        editor_evidence: Default::default(),
    })
}

pub(super) fn attach_style_stmt_spans(
    stmt: &mut StyleStmt,
    rest: &str,
    rest_start: usize,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<(), LexError> {
    let Some((target, styles)) = split_first_word_with_span(rest, rest_start) else {
        return Ok(());
    };
    stmt.target_span = Some(target.span);
    if let Some(styles) = trim_spanned_slice(styles) {
        stmt.styles_text = Some(owned_text(region, styles.text)?);
        stmt.styles_span = Some(styles.span);
    }
    Ok(())
}

pub(super) fn attach_classdef_stmt_spans(
    stmt: &mut ClassDefStmt,
    rest: &str,
    rest_start: usize,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<(), LexError> {
    let Some((ids, styles)) = split_first_word_with_span(rest, rest_start) else {
        return Ok(());
    };
    stmt.id_spans = split_comma_value_spans(ids.text, ids.span.start, region)?;
    if let Some(styles) = trim_spanned_slice(styles) {
        stmt.styles_text = Some(owned_text(region, styles.text)?);
        stmt.styles_span = Some(styles.span);
    }
    Ok(())
}

pub(super) fn attach_class_assign_stmt_spans(
    stmt: &mut ClassAssignStmt,
    rest: &str,
    rest_start: usize,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<(), LexError> {
    let Some((targets, class_name)) = split_first_word_with_span(rest, rest_start) else {
        return Ok(());
    };
    stmt.target_spans = split_comma_value_spans(targets.text, targets.span.start, region)?;
    stmt.class_name_span = trim_spanned_slice(class_name).map(|class_name| class_name.span);
    Ok(())
}

#[derive(Clone, Copy)]
struct SpannedSlice<'a> {
    text: &'a str,
    span: SourceSpan,
}

fn split_first_word_with_span(
    rest: &str,
    rest_start: usize,
) -> Option<(SpannedSlice<'_>, SpannedSlice<'_>)> {
    let leading = rest.len().saturating_sub(rest.trim_start().len());
    let trimmed = &rest[leading..];
    if trimmed.is_empty() {
        return None;
    }

    let mut first_len = 0usize;
    while first_len < trimmed.len() && !trimmed.as_bytes()[first_len].is_ascii_whitespace() {
        first_len += 1;
    }

    let first_start = rest_start + leading;
    let rest_after_first_start = first_start + first_len;
    Some((
        SpannedSlice {
            text: &trimmed[..first_len],
            span: SourceSpan::new(first_start, rest_after_first_start),
        },
        SpannedSlice {
            text: &trimmed[first_len..],
            span: SourceSpan::new(rest_after_first_start, rest_start + rest.len()),
        },
    ))
}

fn trim_spanned_slice(slice: SpannedSlice<'_>) -> Option<SpannedSlice<'_>> {
    let leading = slice
        .text
        .len()
        .saturating_sub(slice.text.trim_start().len());
    let text = &slice.text[leading..];
    let trimmed_len = text.trim_end().len();
    if trimmed_len == 0 {
        return None;
    }
    let start = slice.span.start + leading;
    Some(SpannedSlice {
        text: &text[..trimmed_len],
        span: SourceSpan::new(start, start + trimmed_len),
    })
}

fn split_comma_value_spans(
    text: &str,
    text_start: usize,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<Vec<SourceSpan>, LexError> {
    let mut out = Vec::new();
    let mut value_start = 0usize;
    let bytes = text.as_bytes();

    for idx in 0..=bytes.len() {
        if idx != bytes.len() && bytes[idx] != b',' {
            continue;
        }

        let value = &text[value_start..idx];
        if let Some(value) = trim_spanned_slice(SpannedSlice {
            text: value,
            span: SourceSpan::new(text_start + value_start, text_start + idx),
        }) {
            push_value(region, &mut out, value.span)?;
        }
        value_start = idx.saturating_add(1);
    }

    Ok(out)
}

struct ClickParse<'a> {
    s: &'a str,
    base: usize,
    i: usize,
}

impl<'a> ClickParse<'a> {
    fn new(s: &'a str, base: usize) -> Self {
        Self { s, base, i: 0 }
    }

    fn skip_ws(&mut self) {
        while self.i < self.s.len() {
            let Some(ch) = self.s[self.i..].chars().next() else {
                break;
            };
            if !is_ecmascript_whitespace(ch) {
                break;
            }
            self.i += ch.len_utf8();
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }

    fn take_word(&mut self) -> Option<&'a str> {
        self.skip_ws();
        let start = self.i;
        while self.i < self.s.len() {
            let Some(ch) = self.s[self.i..].chars().next() else {
                break;
            };
            if is_ecmascript_whitespace(ch) {
                break;
            }
            self.i += ch.len_utf8();
        }
        if self.i == start {
            return None;
        }
        Some(&self.s[start..self.i])
    }

    fn take_quoted(&mut self) -> Option<&'a str> {
        self.skip_ws();
        if self.peek()? != b'"' {
            return None;
        }
        self.i += 1;
        let start = self.i;
        while self.i < self.s.len() && self.s.as_bytes()[self.i] != b'"' {
            self.i += 1;
        }
        let out = &self.s[start..self.i];
        if self.i < self.s.len() && self.s.as_bytes()[self.i] == b'"' {
            self.i += 1;
        }
        Some(out)
    }

    fn rest(&self) -> &str {
        &self.s[self.i..]
    }

    fn source_span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(self.base + start, self.base + end)
    }
}

fn click_following_span(parser: &ClickParse<'_>, boundary_end: usize) -> Option<SourceSpan> {
    (parser.i > boundary_end).then(|| {
        // Keep the completion slot after the first separator, including any
        // remaining spaces, without placing a byte span inside a UTF-8 character.
        let separator_len = parser.s[boundary_end..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        parser.source_span(boundary_end + separator_len, parser.s.len())
    })
}

fn click_action_prefix(word: &str) -> bool {
    !word.is_empty() && ("href".starts_with(word) || "call".starts_with(word))
}

fn click_statement_error(
    message: impl Into<String>,
    evidence: FlowchartClickEditorEvidence,
) -> LexError {
    evidence
        .iter()
        .fold(LexError::new(message), |error, expected| {
            error.expecting(expected.kind, expected.span)
        })
}

fn click_statement_error_controlled(
    region: Option<&markdown_memory::RegionHandle>,
    message: &str,
    evidence: FlowchartClickEditorEvidence,
) -> LexError {
    if region.is_some() {
        LexError::terminal()
    } else {
        click_statement_error(message, evidence)
    }
}

#[cfg(test)]
pub(super) fn parse_click_stmt(
    rest: &str,
    rest_start: usize,
) -> std::result::Result<ClickStmt, LexError> {
    parse_click_stmt_controlled(rest, rest_start, None)
}

pub(super) fn parse_click_stmt_controlled(
    rest: &str,
    rest_start: usize,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<ClickStmt, LexError> {
    let mut p = ClickParse::new(rest, rest_start);
    p.skip_ws();
    let id_start = p.i;
    let Some(id) = p.take_word() else {
        return Err(fixed_or_legacy_error(region, "Invalid click statement"));
    };
    let ids = string_list(region, std::iter::once(id))?;
    let id_spans = scalar_list(region, std::iter::once(p.source_span(id_start, p.i)))?;

    let target_end = p.i;
    p.skip_ws();
    let after_target = click_following_span(&p, target_end);
    let tooltip: Option<String>;
    let action: ClickAction;

    if p.rest().starts_with("href")
        && p.rest()
            .get(4..)
            .and_then(|rest| rest.chars().next())
            .is_none_or(is_ecmascript_whitespace)
    {
        let action_start = p.i;
        let _ = p.take_word();
        let action_end = p.i;
        let action_span = p.source_span(action_start, action_end);
        p.skip_ws();
        let interaction_evidence = FlowchartClickEditorEvidence::new(
            Some(action_span),
            click_following_span(&p, action_end),
        );
        let Some(link) = p.take_quoted() else {
            return Err(click_statement_error_controlled(
                region,
                "Invalid click statement",
                interaction_evidence,
            ));
        };
        let maybe_tt = p.take_quoted();
        let maybe_target = p.take_word();
        if maybe_target
            .as_deref()
            .is_some_and(|target| !matches!(target, "_self" | "_blank" | "_parent" | "_top"))
        {
            return Err(click_statement_error_controlled(
                region,
                "Invalid click target",
                interaction_evidence,
            ));
        }
        tooltip = maybe_tt.map(|s| owned_text(region, s)).transpose()?;
        action = ClickAction::Link {
            href: owned_text(region, link)?,
            target: maybe_target.map(|s| owned_text(region, s)).transpose()?,
        };
        p.skip_ws();
        if p.i != p.s.len() {
            return Err(click_statement_error_controlled(
                region,
                "Unexpected content after click statement",
                interaction_evidence,
            ));
        }
        return Ok(ClickStmt {
            ids,
            id_spans,
            tooltip,
            action,
            editor_evidence: Default::default(),
            interaction_evidence,
            recovery_error: None,
        });
    }

    if p.rest().starts_with("call")
        && p.rest()
            .get(4..)
            .and_then(|rest| rest.chars().next())
            .is_none_or(is_ecmascript_whitespace)
    {
        let action_start = p.i;
        let _ = p.take_word();
        let action_end = p.i;
        p.skip_ws();
        let interaction_evidence = FlowchartClickEditorEvidence::new(
            Some(p.source_span(action_start, action_end)),
            click_following_span(&p, action_end),
        );
        let start = p.i;
        while p.i < p.s.len() {
            let Some(ch) = p.s[p.i..].chars().next() else {
                break;
            };
            if is_ecmascript_whitespace(ch) || ch == '(' {
                break;
            }
            p.i += ch.len_utf8();
        }
        if p.i == start {
            return Err(click_statement_error_controlled(
                region,
                "Invalid click statement",
                interaction_evidence,
            ));
        }
        p.skip_ws();
        if p.peek() == Some(b'(') {
            p.i += 1;
            while p.i < p.s.len() && p.s.as_bytes()[p.i] != b')' {
                p.i += 1;
            }
            if p.peek() != Some(b')') {
                return Err(click_statement_error_controlled(
                    region,
                    "Unterminated click callback arguments",
                    interaction_evidence,
                ));
            }
            p.i += 1;
        }

        tooltip = p.take_quoted().map(|s| owned_text(region, s)).transpose()?;
        action = ClickAction::Callback;
        p.skip_ws();
        if p.i != p.s.len() {
            return Err(click_statement_error_controlled(
                region,
                "Unexpected content after click statement",
                interaction_evidence,
            ));
        }
        return Ok(ClickStmt {
            ids,
            id_spans,
            tooltip,
            action,
            editor_evidence: Default::default(),
            interaction_evidence,
            recovery_error: None,
        });
    }

    if let Some(link) = p.take_quoted() {
        let interaction_evidence = FlowchartClickEditorEvidence::new(None, after_target);
        let maybe_tt = p.take_quoted();
        let maybe_target = p.take_word();
        if maybe_target
            .as_deref()
            .is_some_and(|target| !matches!(target, "_self" | "_blank" | "_parent" | "_top"))
        {
            return Err(click_statement_error_controlled(
                region,
                "Invalid click target",
                interaction_evidence,
            ));
        }
        tooltip = maybe_tt.map(|s| owned_text(region, s)).transpose()?;
        action = ClickAction::Link {
            href: owned_text(region, link)?,
            target: maybe_target.map(|s| owned_text(region, s)).transpose()?,
        };
        p.skip_ws();
        if p.i != p.s.len() {
            return Err(click_statement_error_controlled(
                region,
                "Unexpected content after click statement",
                interaction_evidence,
            ));
        }
        return Ok(ClickStmt {
            ids,
            id_spans,
            tooltip,
            action,
            editor_evidence: Default::default(),
            interaction_evidence,
            recovery_error: None,
        });
    }

    let function_start = p.i;
    let Some(function_name) = p.take_word() else {
        return Err(click_statement_error_controlled(
            region,
            "Invalid click statement",
            FlowchartClickEditorEvidence::new(after_target, None),
        ));
    };
    let function_span = p.source_span(function_start, p.i);
    let interaction_evidence = if click_action_prefix(&function_name) {
        FlowchartClickEditorEvidence::new(Some(function_span), None)
    } else {
        FlowchartClickEditorEvidence::new(None, after_target)
    };
    tooltip = p.take_quoted().map(|s| owned_text(region, s)).transpose()?;
    action = ClickAction::Callback;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(click_statement_error_controlled(
            region,
            "Unexpected content after click statement",
            interaction_evidence,
        ));
    }
    Ok(ClickStmt {
        ids,
        id_spans,
        tooltip,
        action,
        editor_evidence: Default::default(),
        interaction_evidence,
        recovery_error: None,
    })
}

pub(super) fn parse_link_style_stmt_controlled(
    rest: &str,
    rest_start: usize,
    region: Option<&markdown_memory::RegionHandle>,
) -> std::result::Result<LinkStyleStmt, LexError> {
    let mut p = ClickParse::new(rest, rest_start);
    p.skip_ws();
    let Some(pos_raw) = p.take_word() else {
        return Err(fixed_or_legacy_error(region, "Invalid linkStyle statement"));
    };

    let mut positions = Vec::new();
    if pos_raw == "default" {
        push_value(region, &mut positions, LinkStylePos::Default)?;
    } else {
        for value in pos_raw.split(',') {
            let index = value
                .trim()
                .parse::<usize>()
                .map_err(|_| fixed_or_legacy_error(region, "Invalid linkStyle statement"))?;
            push_value(region, &mut positions, LinkStylePos::Index(index))?;
        }
    }

    p.skip_ws();
    let mut interpolate: Option<String> = None;
    if p.rest().starts_with("interpolate")
        && p.rest()
            .get("interpolate".len()..)
            .and_then(|rest| rest.chars().next())
            .is_none_or(is_ecmascript_whitespace)
    {
        let _ = p.take_word();
        interpolate = p.take_word().map(|s| owned_text(region, s)).transpose()?;
    }

    // Mermaid's `linkStyle ... interpolate <curve> ...` still tokenizes the styles list without the
    // leading whitespace between the curve name and the first style token. Keep the whitespace
    // inside comma-separated tokens (handled by `parse_linkstyle_styles_list`), but drop the
    // leading separator spaces at the list boundary.
    p.skip_ws();
    let styles = parse_styles_list_controlled(p.rest(), region, false)?;
    Ok(LinkStyleStmt {
        positions,
        interpolate,
        styles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_click_stmt_parses_callback() {
        let stmt = parse_click_stmt("A callback", 0).unwrap();
        assert_eq!(stmt.ids, vec!["A"]);
        assert!(stmt.tooltip.is_none());
        match stmt.action {
            ClickAction::Callback => {}
            _ => panic!("expected callback action"),
        }
    }

    #[test]
    fn parse_click_stmt_parses_call_callback_empty_args() {
        let stmt = parse_click_stmt("A call callback()", 0).unwrap();
        assert_eq!(stmt.ids, vec!["A"]);
        assert!(stmt.tooltip.is_none());
        match stmt.action {
            ClickAction::Callback => {}
            _ => panic!("expected callback action"),
        }
    }

    #[test]
    fn parse_click_stmt_parses_call_callback_with_args() {
        let stmt = parse_click_stmt("A call callback(\"test0\", test1, test2)", 0).unwrap();
        match stmt.action {
            ClickAction::Callback => {}
            _ => panic!("expected callback action"),
        }
    }

    #[test]
    fn parse_click_stmt_parses_link_and_tooltip_and_target() {
        let stmt = parse_click_stmt("A \"click.html\" \"tooltip\" _blank", 0).unwrap();
        assert_eq!(stmt.tooltip.as_deref(), Some("tooltip"));
        match stmt.action {
            ClickAction::Link { href, target } => {
                assert_eq!(href, "click.html");
                assert_eq!(target.as_deref(), Some("_blank"));
            }
            _ => panic!("expected link action"),
        }
    }

    #[test]
    fn parse_click_stmt_accepts_ecmascript_whitespace_without_corrupting_utf8_spans() {
        for separator in [
            "\u{0009}", "\u{000a}", "\u{000b}", "\u{000c}", "\u{000d}", "\u{0020}", "\u{00a0}",
            "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}", "\u{2005}",
            "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200a}", "\u{2028}", "\u{2029}",
            "\u{202f}", "\u{205f}", "\u{3000}", "\u{feff}",
        ] {
            let source =
                format!("A{separator}href{separator}\"url\"{separator}\"tip\"{separator}_blank");
            let stmt = parse_click_stmt(&source, 11).unwrap();
            assert_eq!(stmt.ids, vec!["A"]);
            assert_eq!(stmt.tooltip.as_deref(), Some("tip"));
            assert_eq!(stmt.id_spans[0], SourceSpan::new(11, 12));
            let expected_payload = format!("\"url\"{separator}\"tip\"{separator}_blank");
            for expected in stmt.interaction_evidence.iter() {
                let span = expected.span;
                let actual = source
                    .get(span.start - 11..span.end - 11)
                    .expect("interaction spans must stay on UTF-8 character boundaries");
                match expected.kind {
                    crate::EditorExpectedSyntaxKind::InteractionAction => {
                        assert_eq!(actual, "href")
                    }
                    crate::EditorExpectedSyntaxKind::Payload => {
                        assert_eq!(actual, expected_payload)
                    }
                    _ => panic!("unexpected click interaction evidence"),
                }
            }
        }
        assert!(parse_click_stmt("A\u{0085}href\u{0085}\"url\"", 0).is_err());
    }

    #[test]
    fn parse_click_stmt_parses_href_link_and_tooltip_and_target() {
        let stmt = parse_click_stmt("A href \"click.html\" \"tooltip\" _blank", 0).unwrap();
        assert_eq!(stmt.tooltip.as_deref(), Some("tooltip"));
        match stmt.action {
            ClickAction::Link { href, target } => {
                assert_eq!(href, "click.html");
                assert_eq!(target.as_deref(), Some("_blank"));
            }
            _ => panic!("expected link action"),
        }
    }
}
