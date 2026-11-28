// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::html::{picture, ruby};

const MAX_CANDIDATES: usize = 256;
const MAX_INLINE_REGION_BYTES: usize = 4096;

pub(super) fn tag_span<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    token: n::NodeToken<'view, 'arena, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<Option<SourceSpan>, CoreError> {
    let lineage = native.read(work, |access| access.lineage(token))?;
    let binding = native.read(work, |access| access.binding(lineage))?;
    Ok(native
        .inline_html_copy(binding.value, 3..=MAX_INLINE_REGION_BYTES, work)?
        .map(|(_, span, _)| span))
}

fn admitted_closer<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    mut token: Option<n::NodeToken<'view, 'arena, 'input>>,
    closing: SourceSpan,
    work: &mut OwnedWork<'id, '_>,
) -> Result<Option<n::NodeToken<'view, 'arena, 'input>>, CoreError> {
    // A simple ruby group may contain only accepted inline text and its own
    // HTML tags. Stop on a Markdown construct or on a different sibling list.
    for _ in 0..512 {
        let Some(current) = token else { break };
        let row = native.read(work, |access| access.node(current))?;
        match row.kind {
            native::NativeKind::Text => {}
            native::NativeKind::HtmlInline => {
                let Some(span) = tag_span(native, current, work)? else {
                    break;
                };
                if span == closing {
                    return Ok(Some(current));
                }
                if span.end() > closing.start() {
                    break;
                }
            }
            _ => break,
        }
        token = row.next_sibling;
    }
    Ok(None)
}

fn picture_closer<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    mut token: Option<n::NodeToken<'view, 'arena, 'input>>,
    opening: SourceSpan,
    work: &mut OwnedWork<'id, '_>,
) -> Result<Option<(n::NodeToken<'view, 'arena, 'input>, SourceSpan)>, CoreError> {
    let mut image = false;
    for _ in 0..36 {
        let Some(current) = token else { break };
        let row = native.read(work, |access| access.node(current))?;
        match row.kind {
            native::NativeKind::Text => {}
            native::NativeKind::HtmlInline => {
                let Some(span) = tag_span(native, current, work)? else {
                    break;
                };
                if span.end() > opening.start() + MAX_INLINE_REGION_BYTES as u64 {
                    break;
                }
                let raw = native.html_source(span, work)?;
                if image && picture::is_inline_delimiter(raw, true) {
                    return Ok(Some((current, span)));
                }
                if raw.starts_with(b"<!--") && raw.ends_with(b"-->") {
                    token = row.next_sibling;
                    continue;
                }
                let Some((name, closing, _)) = inline_tag_prefix(raw) else {
                    break;
                };
                if closing
                    || image
                    || !(name.eq_ignore_ascii_case(b"source") || name.eq_ignore_ascii_case(b"img"))
                {
                    break;
                }
                image = name.eq_ignore_ascii_case(b"img");
            }
            _ => break,
        }
        token = row.next_sibling;
    }
    Ok(None)
}

const RAW_TEXT_TAGS: &[&[u8]] = &[
    b"script",
    b"style",
    b"textarea",
    b"title",
    b"xmp",
    b"iframe",
    b"noembed",
    b"noframes",
    b"plaintext",
];

#[derive(Default)]
pub(super) struct OpaqueTags {
    tags: [&'static [u8]; 16],
    len: usize,
    unknown: bool,
}

impl OpaqueTags {
    pub(super) fn active(&self) -> bool {
        self.unknown || self.len != 0
    }

    pub(super) fn unknown(&mut self) {
        self.unknown = true;
    }

    pub(super) fn observe(&mut self, raw: &[u8], inline_region: bool) {
        if self.unknown {
            return;
        }
        let Some((name, closing, tail)) = inline_tag_prefix(raw) else {
            return;
        };
        let separate = if inline_region {
            &[][..]
        } else {
            &[b"pre".as_slice(), b"a"][..]
        };
        let Some(tag) = [b"code".as_slice(), b"kbd", b"samp"]
            .into_iter()
            .chain(separate.iter().copied())
            .chain(RAW_TEXT_TAGS.iter().copied())
            .find(|tag| name.eq_ignore_ascii_case(tag))
        else {
            return;
        };
        if inline_region && native::gfm_tagfilter_matches(raw) {
            return;
        }
        let top = self.tags[..self.len].last().copied();
        if closing {
            if top == Some(tag) && tag != b"plaintext" {
                self.len -= 1;
            }
        } else if top.is_none_or(|open| !RAW_TEXT_TAGS.contains(&open))
            && !tail
                .strip_suffix(b"/>")
                .is_some_and(|space| space.iter().all(|byte| inline_html_tag_space(*byte)))
        {
            if let Some(slot) = self.tags.get_mut(self.len) {
                *slot = tag;
                self.len += 1;
            } else {
                self.unknown();
            }
        }
    }
}

pub(super) fn collect<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    notes: Option<&footnotes::NotePlan<'view, 'arena, 'input, 'id>>,
    output: &mut BuildGrowingVec<'id, projection::ProjectionBlock>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let original = native.source.input().source();
    // Reserve the fixed lexical lookahead and one bounded entity decode.
    work.ensure_budget(original.budget())?;
    let Ok(_scratch) = original.budget().reserve_auxiliary(8192) else {
        return Ok(());
    };
    let source = original.text();
    let root = native.read(work, |access| access.root())?;
    let root_id = native.read(work, |access| access.node(root))?.id;
    let mut token = root;
    let mut scope = None;
    let mut opaque = OpaqueTags::default();
    let mut candidates = 0;
    loop {
        let mut row = native.read(work, |access| access.node(token))?;
        match row.kind {
            native::NativeKind::Paragraph
            | native::NativeKind::Heading
            | native::NativeKind::TableCell => {
                scope = Some(row.id);
                opaque = OpaqueTags::default();
            }
            native::NativeKind::HtmlInline if scope.is_some() => {
                if let Some(span) = tag_span(native, token, work)? {
                    let raw = native.html_source(span, work)?;
                    opaque.observe(raw, true);
                    if !opaque.active() && ruby::is_opening(raw) {
                        if candidates >= MAX_CANDIDATES {
                            break;
                        }
                        candidates += 1;
                        let start = span.start() as usize;
                        work.scan((source.len() - start).min(ruby::MAX_SOURCE_BYTES) as u64)?;
                        if let Some(close) = ruby::closing_at(source, start) {
                            let close = SourceSpan::new(close.start as u64, close.end as u64)?;
                            if let Some(last) =
                                admitted_closer(native, row.next_sibling, close, work)?
                            {
                                output.push(
                                    projection::ProjectionBlock {
                                        kind: NodeKind::HtmlInline,
                                        source: SourceSpan::new(span.start(), close.end())?,
                                    },
                                    work,
                                )?;
                                row = native.read(work, |access| access.node(last))?;
                            }
                        }
                    } else if !opaque.active() && picture::is_inline_delimiter(raw, false) {
                        if candidates >= MAX_CANDIDATES {
                            break;
                        }
                        candidates += 1;
                        if let Some((last, close)) =
                            picture_closer(native, row.next_sibling, span, work)?
                        {
                            output.push(
                                projection::ProjectionBlock {
                                    kind: NodeKind::HtmlInline,
                                    source: SourceSpan::new(span.start(), close.end())?,
                                },
                                work,
                            )?;
                            row = native.read(work, |access| access.node(last))?;
                        }
                    }
                } else {
                    opaque.unknown();
                }
            }
            _ => {}
        }
        let hidden_note = match notes {
            Some(notes) if row.kind == native::NativeKind::FootnoteDefinition => {
                !notes.is_visible_definition(row.id.get(), work)?
            }
            _ => false,
        };
        if !hidden_note
            && row.kind != native::NativeKind::Image
            && let Some(child) = row.first_child
        {
            token = child;
            continue;
        }
        loop {
            if scope == Some(row.id) {
                scope = None;
            }
            if row.id == root_id {
                return Ok(());
            }
            if let Some(next) = row.next_sibling {
                token = next;
                break;
            }
            let parent = row.parent.ok_or(CoreError::InvalidProvenance)?;
            row = native.read(work, |access| access.node(parent))?;
        }
    }
    Ok(())
}
