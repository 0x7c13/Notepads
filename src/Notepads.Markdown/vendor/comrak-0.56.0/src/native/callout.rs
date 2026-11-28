// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use super::{Context, Host, PSpan};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CalloutKind {
    Note = 1,
    Tip,
    Important,
    Warning,
    Caution,
    Abstract,
    Info,
    Todo,
    Success,
    Question,
    Failure,
    Danger,
    Bug,
    Example,
    Quote,
}

impl CalloutKind {
    pub fn title(self) -> &'static str {
        use CalloutKind::*;
        match self {
            Note => "Note",
            Tip => "Tip",
            Important => "Important",
            Warning => "Warning",
            Caution => "Caution",
            Abstract => "Abstract",
            Info => "Info",
            Todo => "Todo",
            Success => "Success",
            Question => "Question",
            Failure => "Failure",
            Danger => "Danger",
            Bug => "Bug",
            Example => "Example",
            Quote => "Quote",
        }
    }

    fn from_name(name: &[u8]) -> Option<Self> {
        use CalloutKind::*;
        const NAMES: &[(CalloutKind, &[&[u8]])] = &[
            (Note, &[b"note"]),
            (Tip, &[b"tip", b"hint"]),
            (Important, &[b"important"]),
            (Warning, &[b"warning", b"attention"]),
            (Caution, &[b"caution"]),
            (Abstract, &[b"abstract", b"summary", b"tldr"]),
            (Info, &[b"info"]),
            (Todo, &[b"todo"]),
            (Success, &[b"success", b"check", b"done"]),
            (Question, &[b"question", b"help", b"faq"]),
            (Failure, &[b"failure", b"fail", b"missing"]),
            (Danger, &[b"danger", b"error"]),
            (Bug, &[b"bug"]),
            (Example, &[b"example"]),
            (Quote, &[b"quote", b"cite"]),
        ];
        NAMES.iter().find_map(|&(kind, aliases)| {
            aliases
                .iter()
                .any(|alias| name.eq_ignore_ascii_case(alias))
                .then_some(kind)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalloutDisclosure {
    Static,
    InitiallyOpen,
    InitiallyClosed,
}

/// The original marker, including an adjacent disclosure suffix if present.
pub fn callout_marker(raw: &[u8]) -> Option<(CalloutKind, CalloutDisclosure)> {
    let name = raw.strip_prefix(b"[!")?;
    let (name, disclosure) = match name.last()? {
        b'+' => (name.strip_suffix(b"]+")?, CalloutDisclosure::InitiallyOpen),
        b'-' => (
            name.strip_suffix(b"]-")?,
            CalloutDisclosure::InitiallyClosed,
        ),
        b']' => (name.strip_suffix(b"]")?, CalloutDisclosure::Static),
        _ => return None,
    };
    Some((CalloutKind::from_name(name)?, disclosure))
}

#[derive(Clone, Copy)]
pub(crate) struct CalloutLine {
    pub(crate) kind: CalloutKind,
    pub(crate) disclosure: CalloutDisclosure,
    pub(crate) marker: PSpan,
    pub(crate) title_start: usize,
    pub(crate) has_title: bool,
}

pub(crate) fn scan_line<H: Host>(
    line: &[u8],
    start: usize,
    start_column: usize,
    ctx: &mut Context<'_, H>,
) -> Result<Option<CalloutLine>, H::Error> {
    if line.get(start) != Some(&b'>') {
        return Ok(None);
    }
    ctx.scan_step()?;
    let mut marker = start + 1;
    let mut column = start_column + 1;
    while let Some(byte @ (b' ' | b'\t')) = line.get(marker).copied() {
        ctx.scan_step()?;
        marker += 1;
        column += if byte == b'\t' {
            crate::parser::TAB_STOP - column % crate::parser::TAB_STOP
        } else {
            1
        };
        // One column may belong to the quote prefix. Four additional
        // columns start indented code in the ordinary CommonMark parser.
        if column - start_column > 5 {
            return Ok(None);
        }
    }
    if !line[marker..].starts_with(b"[!") {
        return Ok(None);
    }
    let mut end = marker + 2;
    while line.get(end).is_some_and(u8::is_ascii_alphabetic) && end - marker <= 16 {
        ctx.scan_step()?;
        end += 1;
    }
    if line.get(end) != Some(&b']') {
        return Ok(None);
    }
    end += 1;
    if matches!(line.get(end), Some(b'+' | b'-')) {
        end += 1;
    }
    let Some((kind, disclosure)) = callout_marker(&line[marker..end]) else {
        return Ok(None);
    };
    if line
        .get(end)
        .is_some_and(|&byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        return Ok(None);
    }
    let mut title_start = end;
    while matches!(line.get(title_start), Some(b' ' | b'\t')) {
        ctx.scan_step()?;
        title_start += 1;
    }
    let has_title = line
        .get(title_start)
        .is_some_and(|&byte| !matches!(byte, b'\r' | b'\n'));
    Ok(Some(CalloutLine {
        kind,
        disclosure,
        marker: PSpan { start: marker, end },
        title_start,
        has_title,
    }))
}
