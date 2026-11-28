// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::memory::{BuildString, BuildValue};

#[derive(Clone, Copy)]
pub(super) struct Heading<'view, 'input> {
    pub lineage: n::LineageToken<'view, 'input>,
    pub label: b::TextRef,
    pub bytes: u64,
    pub utf16: u64,
    pub depth: u8,
}

impl BuildValue for Heading<'_, '_> {}

pub(super) struct Plan<'view, 'input, 'id> {
    pub headings: BuildRows<'id, Heading<'view, 'input>>,
    label_bytes: u64,
    projected_bytes: u64,
}

impl<'view, 'input, 'id> Plan<'view, 'input, 'id> {
    pub(super) fn prepare<'arena>(
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        root: n::NodeToken<'view, 'arena, 'input>,
        notes: Option<&footnotes::NotePlan<'view, 'arena, 'input, 'id>>,
        inline_html: bool,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        let limit = work.require_model_limits()?.model_records;
        let mut headings = BuildRows::new(limit, LimitKind::ModelRecords, work)?;
        let mut levels = [0; 6];
        let mut depth = 0usize;
        let mut label_bytes = 0u64;
        let mut add =
            |token, info: n::NodeInfo<'view, 'arena, 'input>, work: &mut OwnedWork<'id, '_>| {
                if info.kind == native::NativeKind::FootnoteDefinition && notes.is_some() {
                    return Ok(false);
                }
                let n::NodeAttributes::Heading { level, .. } = info.attributes else {
                    return Ok(true);
                };
                if !(1..=6).contains(&level) {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                while depth != 0 && levels[depth - 1] >= level {
                    depth -= 1;
                }
                let (label, bytes, utf16) = label(native, token, inline_html, builder, work)?;
                label_bytes = label_bytes
                    .checked_add(bytes)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                let lineage = native.read(work, |access| access.lineage(token))?;
                headings.push(
                    Heading {
                        lineage,
                        label,
                        bytes,
                        utf16,
                        depth: depth as u8,
                    },
                    work,
                )?;
                levels[depth] = level;
                depth += 1;
                Ok(false)
            };
        walk(native, root, work, &mut add)?;
        if let Some(notes) = notes {
            let mut index = 0;
            while let Some(note) = notes.at(index, work)? {
                let mut child = native.read(work, |access| access.node(note))?.first_child;
                while let Some(node) = child {
                    child = native.read(work, |access| access.node(node))?.next_sibling;
                    walk(native, node, work, &mut add)?;
                }
                index += 1;
            }
        }
        Ok(Self {
            headings,
            label_bytes,
            projected_bytes: 0,
        })
    }

    pub(super) fn reserve_entries(
        &mut self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<usize, CoreError> {
        self.projected_bytes = self
            .projected_bytes
            .checked_add(self.label_bytes)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let limit = work.require_model_limits()?.decoded_bytes;
        work.check_limit(LimitKind::DecodedBytes, self.projected_bytes, limit)?;
        Ok(self.headings.len())
    }
}

fn walk<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    root: n::NodeToken<'view, 'arena, 'input>,
    work: &mut OwnedWork<'id, '_>,
    mut visit: impl FnMut(
        n::NodeToken<'view, 'arena, 'input>,
        n::NodeInfo<'view, 'arena, 'input>,
        &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError>,
) -> Result<(), CoreError> {
    let mut token = root;
    let mut info = native.read(work, |access| access.node(token))?;
    let stop = info.id;
    loop {
        if visit(token, info, work)?
            && let Some(child) = info.first_child
        {
            token = child;
            info = native.read(work, |access| access.node(token))?;
            continue;
        }
        loop {
            if info.id == stop {
                return Ok(());
            }
            if let Some(sibling) = info.next_sibling {
                token = sibling;
                info = native.read(work, |access| access.node(token))?;
                break;
            }
            let parent = info.parent.ok_or(CoreError::InvalidProvenance)?;
            info = native.read(work, |access| access.node(parent))?;
        }
    }
}

fn label_parts<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    root: n::NodeToken<'view, 'arena, 'input>,
    inline_html: bool,
    work: &mut OwnedWork<'id, '_>,
    mut accept: impl FnMut(
        Option<n::ValueToken<'view, 'arena, 'input>>,
        &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    walk(native, root, work, |token, info, work| {
        match info.attributes {
            n::NodeAttributes::Text(value)
            | n::NodeAttributes::InlineCode { literal: value, .. }
            | n::NodeAttributes::Math { literal: value, .. } => accept(Some(value), work)?,
            _ if matches!(
                info.kind,
                native::NativeKind::SoftBreak | native::NativeKind::HardBreak
            ) =>
            {
                accept(None, work)?
            }
            _ if inline_html && info.kind == native::NativeKind::HtmlInline => {
                let binding = native.read(work, |access| {
                    let lineage = access.lineage(token)?;
                    access.binding(lineage)
                })?;
                if native.inline_html_break(binding.value, work)?.is_some() {
                    accept(None, work)?;
                }
            }
            _ => {}
        }
        Ok(true)
    })
}

fn label<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    root: n::NodeToken<'view, 'arena, 'input>,
    inline_html: bool,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(b::TextRef, u64, u64), CoreError> {
    let mut bytes = 0usize;
    label_parts(native, root, inline_html, work, |value, work| {
        let len = match value {
            Some(value) => native.read(work, |access| access.value_len(value))?,
            None => 1,
        };
        bytes = bytes
            .checked_add(len)
            .ok_or(CoreError::ArithmeticOverflow)?;
        Ok(())
    })?;
    if bytes == 0 {
        let len = EMPTY_TOC_HEADING.len() as u64;
        return Ok((builder.text(EMPTY_TOC_HEADING, work)?, len, len));
    }
    let mut title = BuildString::auxiliary(bytes, work)?;
    let mut utf16 = 0u64;
    label_parts(native, root, inline_html, work, |value, work| {
        let Some(value) = value else {
            title.append(" ", work)?;
            utf16 += 1;
            return Ok(());
        };
        let len = native.read(work, |access| access.value_len(value))?;
        let mut cursor = native.read(work, |access| access.value_cursor(value, 0, len))?;
        loop {
            let (chunk, next) = native.read(work, |access| {
                access.with_next_value_chunk(cursor, 256, |text, reads| reads.text_chunk(text))
            })?;
            let Some(chunk) = chunk else { break };
            let part = chunk.as_str();
            work.scan(part.len() as u64)?;
            utf16 = utf16
                .checked_add(part.encode_utf16().count() as u64)
                .ok_or(CoreError::ArithmeticOverflow)?;
            title.append(part, work)?;
            cursor = next;
        }
        Ok(())
    })?;
    let title = title.freeze(work)?;
    Ok((builder.text(title.as_str(), work)?, bytes as u64, utf16))
}
