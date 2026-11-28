// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Paid source EGC boundaries and one deterministic script/VO decision per
//! source EGC/context slice. No glyph, face or test-case identity is consulted.
use crate::{
    abi::{self, Failure, Result},
    grapheme_roles::{SourceGraphemeRole as Role, source_role},
    memory::{Context, Lease, WorkBatch},
};
use unicode_script::{Script, ScriptExtension, UnicodeScript};
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};

// GraphemeCursor returns one boundary per call. A scalar-sized forward slice
// avoids repaying a large already-supplied suffix for each ordinary character.
const FORWARD_CHUNK_BYTES: usize = 1;

pub struct SourceIndex {
    boundaries: Vec<usize>,
    _lease: Lease,
}
impl SourceIndex {
    pub fn build_batched(context: &Context, work: &mut WorkBatch<'_>, text: &str) -> Result<Self> {
        let mut count = 0usize;
        scan(
            text,
            FORWARD_CHUNK_BYTES,
            |_| {
                count += 1;
                Ok(())
            },
            |n| work.charge(n),
        )
        .map_err(|error| context.record_failure(error))?;
        let bytes = count
            .checked_mul(size_of::<usize>())
            .ok_or_else(|| context.record_failure(Failure::Contract))?;
        let lease = Lease::reserve(context, abi::SCRATCH, bytes)?;
        let mut boundaries = Vec::new();
        boundaries
            .try_reserve_exact(count)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        lease.allocation_committed(boundaries.capacity() * size_of::<usize>())?;
        scan(
            text,
            FORWARD_CHUNK_BYTES,
            |offset| {
                boundaries.push(offset);
                Ok(())
            },
            |n| work.charge(n),
        )
        .map_err(|error| context.record_failure(error))?;
        if boundaries.len() != count {
            return Err(context.record_failure(Failure::Contract));
        }
        Ok(Self {
            boundaries,
            _lease: lease,
        })
    }
    pub fn boundaries(&self) -> &[usize] {
        &self.boundaries
    }
    pub fn parent(&self, offset: usize) -> Result<usize> {
        match self.boundaries.binary_search(&offset) {
            Ok(i) if i + 1 < self.boundaries.len() => Ok(i),
            Err(i) if i != 0 && i < self.boundaries.len() => Ok(i - 1),
            _ => Err(Failure::Contract),
        }
    }
}

/// At most one bounded UTF8 chunk is scanned between cancellation checks.
/// Pre-context requests may leave the cursor offset unchanged: that is a valid
/// resume state, and each supplied chunk is independently charged.
fn scan(
    text: &str,
    chunk_bytes: usize,
    mut emit: impl FnMut(usize) -> Result<()>,
    mut checkpoint: impl FnMut(u64) -> Result<()>,
) -> Result<()> {
    if chunk_bytes == 0 {
        return Err(Failure::Contract);
    }
    checkpoint(1)?;
    emit(0)?;
    if text.is_empty() {
        return Ok(());
    }
    let mut cursor = GraphemeCursor::new(0, text.len(), true);
    let mut start = 0;
    let mut end = chunk_end(text, start, chunk_bytes);
    loop {
        // Prepay the bounded supplied slice before entering the Unicode state
        // machine. A failing/cancelled callback never leaves scanned work unpaid.
        checkpoint((end - start + 1) as u64)?;
        let answer = cursor.next_boundary(&text[start..end], start);
        match answer {
            Ok(Some(offset)) => emit(offset)?,
            Ok(None) => return Ok(()),
            Err(GraphemeIncomplete::NextChunk) if end < text.len() => {
                let seam = end;
                // Keep the preceding scalar in the read-only chunk. At a seam
                // GraphemeCursor otherwise asks for RI pre-context even when its
                // cached RI count already includes that scalar, double-counting
                // it when provide_context resumes. No algorithm/table is changed.
                start = seam
                    - text[..seam]
                        .chars()
                        .next_back()
                        .ok_or(Failure::Contract)?
                        .len_utf8();
                end = chunk_end(text, seam, chunk_bytes.saturating_sub(seam - start).max(1));
            }
            Err(GraphemeIncomplete::PreContext(offset)) => {
                if offset == 0 || offset > text.len() || !text.is_char_boundary(offset) {
                    return Err(Failure::Contract);
                }
                // Supply only the preceding scalar. Common emoji/Indic joins
                // need just that scalar; a longer prefix must be requested and
                // paid separately instead of charging 4KiB for every short join.
                let context_start = offset
                    - text[..offset]
                        .chars()
                        .next_back()
                        .ok_or(Failure::Contract)?
                        .len_utf8();
                checkpoint((offset - context_start + 1) as u64)?;
                cursor.provide_context(&text[context_start..offset], context_start);
            }
            _ => return Err(Failure::Contract),
        }
    }
}
fn chunk_end(text: &str, start: usize, count: usize) -> usize {
    let mut end = start.saturating_add(count).min(text.len());
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    end
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedContext {
    pub script: Script,
    pub orientation: u32,
    /// 0 direct U/R, 1 TuAsU, 2 TrAsR, 3 Me override.
    pub orientation_reason: u32,
}

#[cfg(test)]
pub fn resolve(text: &str, preferred: Script, vertical: bool) -> Result<ResolvedContext> {
    resolve_checked(text, preferred, vertical, |_| Ok(()))
}
pub fn resolve_checked(
    text: &str,
    preferred: Script,
    vertical: bool,
    mut checkpoint: impl FnMut(u64) -> Result<()>,
) -> Result<ResolvedContext> {
    let first = text.chars().next().ok_or(Failure::Contract)?;
    let mut anchors = ScriptExtension::default();
    let mut has_anchor = false;
    let mut constrained_anchor = false;
    let mut strong_anchor = false;
    let mut any_strong = false;
    let mut me = false;
    each_scalar(text, &mut checkpoint, |ch| {
        any_strong |= strong(ch.script());
        me |= harfrust::notepads_markdown_probe::is_enclosing_mark(ch);
        if source_role(ch) == Role::Anchor {
            has_anchor = true;
            let scx = ch.script_extension();
            if !scx.is_common() && !scx.is_inherited() {
                constrained_anchor = true;
                anchors.intersect_with(scx);
            }
            strong_anchor |= strong(ch.script());
        }
    })?;
    let mut compatible = anchors;
    each_scalar(text, &mut checkpoint, |ch| {
        if matches!(
            source_role(ch),
            Role::Extend | Role::SpacingMark | Role::Zwj
        ) || !has_anchor
        {
            let scx = ch.script_extension();
            if !scx.is_common() && !scx.is_inherited() {
                compatible.intersect_with(scx);
            }
        }
    })?;
    if compatible.is_empty() {
        return Err(unsupported());
    }
    let mut first_primary = None;
    each_scalar(text, &mut checkpoint, |ch| {
        let script = ch.script();
        if first_primary.is_none()
            && (!has_anchor || source_role(ch) != Role::Prepend)
            && strong(script)
            && compatible.contains_script(script)
        {
            first_primary = Some(script);
        }
    })?;
    let script = if has_anchor && !strong_anchor {
        if constrained_anchor && strong(preferred) && compatible.contains_script(preferred) {
            preferred
        } else {
            Script::Common
        }
    } else if strong(preferred) && compatible.contains_script(preferred) {
        preferred
    } else if let Some(primary) = first_primary {
        primary
    } else if (!has_anchor && !any_strong) || compatible.is_common() || compatible.is_inherited() {
        Script::Common
    } else {
        compatible
            .iter()
            .min_by_key(|s| s.short_name())
            .ok_or(Failure::Contract)?
    };
    let (upright, reason) = if me {
        (true, 3)
    } else {
        usvg::replay_source_vertical_orientation(first)
    };
    Ok(ResolvedContext {
        script,
        orientation: u32::from(vertical && upright),
        orientation_reason: reason,
    })
}
fn each_scalar(
    text: &str,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
    mut f: impl FnMut(char),
) -> Result<()> {
    for ch in text.chars() {
        checkpoint(ch.len_utf8() as u64)?;
        f(ch);
    }
    Ok(())
}

fn strong(script: Script) -> bool {
    !matches!(script, Script::Common | Script::Inherited | Script::Unknown)
}
pub fn unsupported() -> Failure {
    Failure::Native(0x80070032u32 as i32)
}
/// Pure source/shape validators use the historical unsupported sentinel. Its
/// source is semantic, so do not invent an original Native callback HRESULT.
pub(crate) fn record_semantic_failure(context: &Context, failure: Failure) -> Failure {
    if failure == unsupported() {
        crate::memory::failure_from_resource(context.record_resource(
            markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Unsupported,
                markdown_svg_budget::LimitKind::None,
            ),
        ))
    } else {
        context.record_failure(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;
    #[test]
    fn chunks_match_unicode17_official_766_cases() {
        let data =
            include_str!("../../../tests/baselines/unicode-17.0.0/GraphemeBreakTest-17.0.0.txt");
        let mut cases = 0;
        for line in data.lines() {
            let before = line.split('#').next().unwrap().trim();
            if before.is_empty() {
                continue;
            }
            let mut text = String::new();
            let mut expected = Vec::new();
            for word in before.split_whitespace() {
                match word {
                    "÷" => expected.push(text.len()),
                    "×" => {}
                    hex => {
                        text.push(char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap())
                    }
                }
            }
            for chunk in [1, 3, 7, 4096] {
                let mut actual = Vec::new();
                scan(
                    &text,
                    chunk,
                    |v| {
                        actual.push(v);
                        Ok(())
                    },
                    |_| Ok(()),
                )
                .unwrap();
                assert_eq!(actual, expected, "{line}; chunk={chunk}");
            }
            cases += 1;
        }
        assert_eq!(cases, 766);
    }
    #[test]
    fn long_context_resumes_and_cancellation_does_not_need_cursor_progress() {
        for text in [
            "🇦".repeat(3000),
            format!("👩{}💻", "\u{301}".repeat(9000)),
            format!("क{}ष", "\u{94d}".repeat(9000)),
        ] {
            let mut expected: Vec<_> = text.grapheme_indices(true).map(|(n, _)| n).collect();
            expected.push(text.len());
            let mut actual = Vec::new();
            let mut checks = 0;
            scan(
                &text,
                4096,
                |v| {
                    actual.push(v);
                    Ok(())
                },
                |_| {
                    checks += 1;
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert!(checks > 2);
            let mut work = 0;
            let error = scan(
                &text,
                4096,
                |_| Ok(()),
                |n| {
                    work += n;
                    if work > 4096 {
                        Err(Failure::Native(-123))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
            assert_eq!(error, Failure::Native(-123));
        }
    }
    #[test]
    fn review_regression_source_chunk_work_stays_proportional() {
        for text in [
            "a".repeat(8192),
            "a\u{301}".repeat(4096),
            "🇦".repeat(3000),
            "👩\u{200d}💻 ".repeat(1000),
            "क्\u{937} ".repeat(1000),
        ] {
            let mut expected: Vec<_> = text.grapheme_indices(true).map(|(n, _)| n).collect();
            expected.push(text.len());
            let mut actual = Vec::new();
            let mut work = 0u64;
            scan(
                &text,
                FORWARD_CHUNK_BYTES,
                |offset| {
                    actual.push(offset);
                    Ok(())
                },
                |units| {
                    work += units;
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert!(
                work <= text.len() as u64 * 16,
                "source bytes={}, charged={work}",
                text.len()
            );
        }
    }
    #[test]
    fn source_roles_script_orientation_and_exact_suppression_do_not_mix() {
        for (text, script, orientation) in [
            ("A\u{301}", "Latn", 0),
            ("中\u{fe0f}", "Hani", 1),
            ("👩\u{200d}💻", "Zyyy", 1),
            ("\u{600}ب", "Arab", 0),
            ("\u{605}中", "Hani", 0),
            ("각", "Hang", 1),
            ("क्\u{937}", "Deva", 0),
            ("1\u{fe0f}\u{20e3}", "Zyyy", 1),
        ] {
            let value = resolve(text, Script::Common, true).unwrap();
            assert_eq!(
                (value.script.short_name(), value.orientation),
                (script, orientation),
                "{text:?}"
            );
        }
        assert_eq!(
            resolve("A\u{93e}", Script::Common, true).err(),
            Some(unsupported())
        );
        assert!(!harfrust::notepads_markdown_probe::shaping_suppression_candidate('\u{180f}'));
        assert!(!harfrust::notepads_markdown_probe::shaping_suppression_candidate('\u{115f}'));
        assert!(harfrust::notepads_markdown_probe::shaping_suppression_candidate('\u{ad}'));
    }
}
