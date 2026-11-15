// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! G3 task acceptance on the actual first paragraph, before ordinary decoding.

use crate::native::{
    AcceptedFact, BufferId, Context, Host, NativeKind, NativeViolation, RSpan, SyntaxRole, g3,
};
use crate::nodes::{Ast, Node, NodeTaskCheckbox, NodeValue};
use crate::parser::Parser;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RawTask {
    leading: RSpan,
    marker: RSpan,
    spacer: RSpan,
    body_start: usize,
    checked: bool,
}

fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

pub(super) fn probe<H: Host>(
    ctx: &mut Context<'_, H>,
    buffer: BufferId,
    base: usize,
    raw: &str,
) -> Result<Option<RawTask>, H::Error> {
    let bytes = raw.as_bytes();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        ctx.scan_step()?;
        if byte != b' ' {
            break;
        }
        at += 1;
    }
    let marker_start = at;
    let Some(marker_end) = at.checked_add(3) else {
        return Err(ctx.host.reject(NativeViolation::ArithmeticOverflow));
    };
    let Some(marker) = bytes.get(at..marker_end) else {
        return Ok(None);
    };
    ctx.scan(3)?;
    if marker[0] != b'['
        || marker[2] != b']'
        || !(whitespace(marker[1]) || matches!(marker[1], b'x' | b'X'))
    {
        return Ok(None);
    }
    at = marker_end;
    while let Some(&byte) = bytes.get(at) {
        ctx.scan_step()?;
        if !whitespace(byte) {
            break;
        }
        at += 1;
    }
    if at == marker_end {
        return Ok(None);
    }
    let absolute = |offset: usize, ctx: &mut Context<'_, H>| {
        base.checked_add(offset)
            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))
    };
    let start = absolute(marker_start, ctx)?;
    let end = absolute(marker_end, ctx)?;
    let body_start = absolute(at, ctx)?;
    Ok(Some(RawTask {
        leading: RSpan {
            buffer,
            start: base,
            end: start,
        },
        marker: RSpan { buffer, start, end },
        spacer: RSpan {
            buffer,
            start: end,
            end: body_start,
        },
        body_start,
        checked: matches!(marker[1], b'x' | b'X'),
    }))
}

/// RawTask has no public constructor. It is issued only for this paragraph's
/// still-unmodified raw buffer; callers must not reuse it across paragraphs.
pub(super) fn accept<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    paragraph: Node<'a>,
    task: RawTask,
) -> Result<(), H::Error> {
    let item = paragraph
        .parent()
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    if !matches!(paragraph.data().value, NodeValue::Paragraph)
        || !matches!(item.data().value, NodeValue::Item(_))
        || !item
            .first_child()
            .is_some_and(|first| first.same_node(paragraph))
    {
        return Err(parser.ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    let paragraph_id = paragraph
        .data()
        .native_id
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let item_id = item
        .data()
        .native_id
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let mut data = paragraph.data_mut();
    let lines = data
        .lines
        .as_mut()
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    if lines.buffer != Some(task.marker.buffer)
        || lines.base != task.leading.start
        || task.body_start < lines.base
    {
        return Err(parser.ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    let consumed = task.body_start - lines.base;
    let body = lines
        .content
        .get(consumed..)
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    // Count actual consumed line endings to preserve the parser's diagnostics
    // and offset vector; provenance remains in the original buffer coordinates.
    let mut removed_lines = 0_usize;
    let mut since_newline = 0_usize;
    for &byte in &lines.content.as_bytes()[..consumed] {
        parser.ctx.scan_step()?;
        if byte == b'\n' {
            removed_lines += 1;
            since_newline = 0;
        } else {
            since_newline += 1;
        }
    }
    let new_content = parser.ctx.string(body)?;
    let mut new_offsets = smallvec::SmallVec::<[usize; 4]>::new();
    for (index, &offset) in lines.offsets.iter().enumerate().skip(removed_lines) {
        parser.ctx.event()?;
        let offset = if index == removed_lines {
            offset
                .checked_add(since_newline)
                .ok_or_else(|| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?
        } else {
            offset
        };
        parser.ctx.small_push(&mut new_offsets, offset)?;
    }
    if new_offsets.is_empty() {
        // Empty labels never get parsed into a fabricated paragraph. A valid
        // offset still allows the existing inline finalizer to seal this buffer.
        parser.ctx.small_push(&mut new_offsets, 0)?;
    }
    let mut depth = 1_usize;
    for _ in item.ancestors() {
        parser.ctx.event()?;
        depth = depth
            .checked_add(1)
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        parser
            .ctx
            .host
            .admit(crate::native::NativeAdmission::Depth(depth as u64))?;
    }
    let checkbox_id = parser.ctx.node(Some(item_id), NativeKind::TaskCheckbox)?;
    let mut checkbox_ast = Ast::new(
        NodeValue::TaskCheckbox(NodeTaskCheckbox {
            checked: task.checked,
        }),
        data.sourcepos.start,
    );
    checkbox_ast.native_id = Some(checkbox_id);
    checkbox_ast.open = false;
    let checkbox = parser.arena.try_alloc(checkbox_ast.into(), parser.ctx)?;
    parser.ctx.fact(AcceptedFact::G3(g3::Fact::Task {
        node: checkbox_id,
        item: item_id,
        paragraph: paragraph_id,
        checked: task.checked,
        marker: task.marker,
        spacer: task.spacer,
    }))?;
    if task.leading.start != task.leading.end {
        parser.ctx.fact(AcceptedFact::Cut {
            span: task.leading,
            reason: SyntaxRole::TaskWhitespace,
        })?;
    }
    parser.ctx.fact(AcceptedFact::Cut {
        span: task.spacer,
        reason: SyntaxRole::TaskWhitespace,
    })?;
    // A failure from here aborts the whole parse. No grammar fallback publishes
    // an AST after an accepted task fact or a partial mutation.
    let lines = data.lines.as_mut().expect("checked paragraph lines");
    let old_content = std::mem::replace(&mut lines.content, new_content);
    let old_offsets = std::mem::replace(&mut lines.offsets, new_offsets);
    lines.base = task.body_start;
    data.sourcepos.start.line = data
        .sourcepos
        .start
        .line
        .checked_add(removed_lines)
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    drop(data);
    parser.ctx.discard_string(old_content)?;
    parser.ctx.discard_small(old_offsets)?;
    paragraph.insert_before(checkbox);
    if let Some(list) = item.parent()
        && let NodeValue::List(ref mut value) = list.data_mut().value
    {
        value.is_task_list = true;
    }
    Ok(())
}

/// Call only after ordinary inline finalization, with traversal successor saved
/// (or in a later pass). Detaching the iterator's current node loses siblings.
pub(super) fn finish(paragraph: Node<'_>) {
    if paragraph.first_child().is_none()
        && paragraph
            .previous_sibling()
            .is_some_and(|previous| matches!(previous.data().value, NodeValue::TaskCheckbox(_)))
    {
        paragraph.detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::g3::tests::TestHost;

    #[test]
    fn g3_task_probe_requires_raw_marker_and_real_spacer() {
        let buffer = BufferId::from_raw(1).unwrap();
        for raw in [
            "[x]",
            "[ ]",
            "[X]",
            "[x]label",
            "[x]\u{a0}label",
            "&#91;x] label",
            "[&#120;] label",
            "[x]&#32;label",
            "\\[x] label",
            "[x\\] label",
            "[\u{a0}] label",
            "\t[x] label",
        ] {
            let mut host = TestHost::new();
            let mut ctx = Context::new(&mut host, raw.len());
            assert_eq!(probe(&mut ctx, buffer, 17, raw).unwrap(), None, "{raw:?}");
            drop(ctx);
            assert_eq!(host.facts, 0);
            assert_eq!(host.first, None);
        }
    }

    #[test]
    fn g3_task_probe_six_ascii_whitespace_bytes_keep_absolute_ranges() {
        let buffer = BufferId::from_raw(1).unwrap();
        for byte in [b' ', b'\t', b'\n', b'\r', 0x0b, 0x0c, b'x', b'X'] {
            let raw = format!("  [{}] \t\r\nlabel", char::from(byte));
            let mut host = TestHost::new();
            let mut ctx = Context::new(&mut host, raw.len());
            let task = probe(&mut ctx, buffer, 17, &raw).unwrap().unwrap();
            assert_eq!(
                task.leading,
                RSpan {
                    buffer,
                    start: 17,
                    end: 19
                }
            );
            assert_eq!(
                task.marker,
                RSpan {
                    buffer,
                    start: 19,
                    end: 22
                }
            );
            assert_eq!(
                task.spacer,
                RSpan {
                    buffer,
                    start: 22,
                    end: 26
                }
            );
            assert_eq!(task.body_start, 26);
            assert_eq!(task.checked, matches!(byte, b'x' | b'X'));
            drop(ctx);
            assert_eq!(host.facts, 0, "probe must not commit facts");
        }
    }

    #[test]
    fn g3_task_probe_empty_label_needs_real_trailing_whitespace() {
        let buffer = BufferId::from_raw(1).unwrap();
        for (raw, spacer_end) in [("[x] ", 21), ("[x]\n", 21), ("[x]\r\n", 22)] {
            let mut host = TestHost::new();
            let mut ctx = Context::new(&mut host, raw.len());
            let task = probe(&mut ctx, buffer, 17, raw).unwrap().unwrap();
            assert_eq!(
                task.marker,
                RSpan {
                    buffer,
                    start: 17,
                    end: 20
                }
            );
            assert_eq!(
                task.spacer,
                RSpan {
                    buffer,
                    start: 20,
                    end: spacer_end
                }
            );
            assert_eq!(task.body_start, 17 + raw.len());
        }
    }
}
