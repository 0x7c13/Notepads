// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

mod cjk;

use std::borrow::Cow;
use std::cell::Cell;
use std::convert::TryFrom;
use std::str;
use std::{mem, ptr};

use smallvec::SmallVec;

#[cfg(test)]
use crate::Arena;
use crate::native::value::NodeValueRole;
use crate::native::{
    self, AcceptedFact, BufferId, Context, Host, NativeAdmission, NativeViolation, RAW_TEXT_TAGS,
    RSpan, SyntaxRole,
};

macro_rules! native_some { ($label:lifetime, $value:expr) => { match $value { Some(value) => value, None => break $label None } }; }
use crate::ctype::{isdigit, ispunct, isspace};
use crate::entity;
use crate::matchers::{backslash_matcher, backtick_matcher, dollar_matcher};
use crate::nodes::{
    Ast, Node, NodeCode, NodeFootnoteDefinition, NodeFootnoteReference, NodeLink, NodeMath,
    NodeValue, NodeWikiLink, Sourcepos, ValueText,
};
#[cfg(feature = "attributes")]
use crate::parser::attributes;
use crate::parser::inlines::cjk::FlankingCheckHelper;
use crate::parser::options::{BrokenLinkReference, WikiLinksMode};
#[cfg(feature = "shortcodes")]
use crate::parser::shortcodes::NodeShortCode;
use crate::parser::{AutolinkType, Options, ResolvedReference, autolink};
use crate::scanners;
use crate::strings::{self, Case, is_blank};

const MAXBACKTICKS: usize = 80;
const MAX_LINK_LABEL_LENGTH: usize = 1000;
const MAX_MATH_DOLLARS: usize = 2;
const MAX_INLINE_FOOTNOTE_DEPTH: usize = 5;
type AutolinkScanResult<'a, E> = Result<Option<(Node<'a>, usize, usize)>, E>;

pub(crate) struct ByteTables {
    pub(crate) special: [bool; 256],
    pub(crate) skip: [bool; 256],
    pub(crate) emph: [bool; 256],
    pub(crate) colon_needs_slashes: bool,
}

impl ByteTables {
    pub(crate) fn new(options: &Options) -> Self {
        let mut t = ByteTables {
            special: [false; 256],
            skip: [false; 256],
            emph: [false; 256],
            colon_needs_slashes: false,
        };
        for &b in b"\n\r_*\"`\\&<[]!" {
            t.special[b as usize] = true;
        }
        if options.extension.math_dollars || options.extension.math_code {
            t.special[b'$' as usize] = true;
        }
        if options.parse.smart {
            for &b in b"\"'.->" {
                t.special[b as usize] = true;
            }
        }
        if options.extension.autolink {
            t.special[b':' as usize] = true;
            t.special[b'w' as usize] = true;
            t.colon_needs_slashes = true;
        }
        if options.extension.strikethrough || options.extension.subscript {
            t.special[b'~' as usize] = true;
            t.skip[b'~' as usize] = true;
            t.emph[b'~' as usize] = true;
        }
        if options.extension.highlight {
            t.special[b'=' as usize] = true;
            t.skip[b'=' as usize] = true;
            t.emph[b'=' as usize] = true;
        }
        if options.extension.insert {
            t.special[b'+' as usize] = true;
            t.skip[b'+' as usize] = true;
            t.emph[b'+' as usize] = true;
        }
        if options.extension.superscript || options.extension.inline_footnotes {
            t.special[b'^' as usize] = true;
        }
        if options.extension.superscript {
            t.emph[b'^' as usize] = true;
        }
        #[cfg(feature = "shortcodes")]
        if options.extension.shortcodes {
            t.special[b':' as usize] = true;
            t.colon_needs_slashes = false;
        }
        if options.extension.underline {
            t.special[b'_' as usize] = true;
        }
        if options.extension.spoiler {
            t.special[b'|' as usize] = true;
            t.emph[b'|' as usize] = true;
        }
        #[cfg(feature = "phoenix_heex")]
        if options.extension.phoenix_heex {
            t.special[b'{' as usize] = true;
            t.special[b'<' as usize] = true;
        }
        for &b in b"*_" {
            t.emph[b as usize] = true;
        }
        t
    }
}

fn checked_scan_end<H: Host>(
    start: usize,
    bytes: usize,
    lo: usize,
    hi: usize,
    ctx: &mut Context<'_, H>,
) -> Result<usize, H::Error> {
    let end = start
        .checked_add(bytes)
        .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    if start < lo || end > hi {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    Ok(end)
}

struct InlineLinkSuffix<'input> {
    url: &'input str,
    starturl: usize,
    endurl: usize,
    starttitle: usize,
    endtitle: usize,
    endall: usize,
}

#[derive(Clone, Copy)]
struct ImageProbeBracket {
    position: usize,
    image: bool,
    bracket_after: bool,
    inside_image: bool,
}

struct InlineNoteBracket {
    note: Option<usize>,
    probe: ImageProbeBracket,
}

fn scan_inline_link_suffix<'input, H: Host>(
    input: &'input str,
    start: usize,
    ctx: &mut Context<'_, H>,
) -> Result<Option<InlineLinkSuffix<'input>>, H::Error> {
    if input.as_bytes().get(start) != Some(&b'(') {
        return Ok(None);
    }
    let limit = input.len();
    let after_open = start + 1;
    let space = scanners::spacechars_with_host(&input[after_open..], ctx)?.unwrap_or(0);
    let starturl = checked_scan_end(after_open, space, start, limit, ctx)?;
    if starturl == limit {
        return Ok(None);
    }
    let Some((url, bytes)) = manual_scan_link_url_with_host(&input[starturl..], ctx)? else {
        return Ok(None);
    };
    let endurl = checked_scan_end(starturl, bytes, start, limit, ctx)?;
    let space = scanners::spacechars_with_host(&input[endurl..], ctx)?.unwrap_or(0);
    let starttitle = checked_scan_end(endurl, space, start, limit, ctx)?;
    let bytes = if starttitle == endurl {
        0
    } else {
        scanners::link_title_with_host(&input[starttitle..], ctx)?.unwrap_or(0)
    };
    let endtitle = checked_scan_end(starttitle, bytes, start, limit, ctx)?;
    let space = scanners::spacechars_with_host(&input[endtitle..], ctx)?.unwrap_or(0);
    let endall = checked_scan_end(endtitle, space, start, limit, ctx)?;
    Ok(
        (input.as_bytes().get(endall) == Some(&b')')).then_some(InlineLinkSuffix {
            url,
            starturl,
            endurl,
            starttitle,
            endtitle,
            endall,
        }),
    )
}

fn inline_note_has_content<H: Host>(
    mut input: &str,
    ctx: &mut Context<'_, H>,
) -> Result<bool, H::Error> {
    while let Some(ch) = input.chars().next() {
        ctx.scan(ch.len_utf8())?;
        input = &input[ch.len_utf8()..];
        if ch.is_whitespace()
            || ch == '\\'
                && input
                    .as_bytes()
                    .first()
                    .copied()
                    .is_some_and(strings::is_line_end_char)
        {
            continue;
        }
        if ch == '&'
            && let Some((decoded, bytes)) = entity::unescape_with_host(input, ctx)?
        {
            let whitespace = decoded.chars().all(char::is_whitespace);
            if let Cow::Owned(decoded) = decoded {
                ctx.discard_string(decoded)?;
            }
            if whitespace {
                input = &input[bytes..];
                continue;
            }
        }
        return Ok(true);
    }
    Ok(false)
}

pub struct Subject<'a: 'd, 'r, 'o, 'd, 'c, 'p, 'h, H: Host> {
    pub(crate) ctx: &'r mut Context<'h, H>,
    pub(crate) buffer: Option<BufferId>,
    pub(crate) base: usize,
    pub arena: &'a native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
    pub options: &'o Options<'c>,
    pub input: Cow<'r, str>,
    // Whole-leaf coordinates throughout. Only the private strict driver can
    // advance these bounds; caches are local to this Subject and gap sequence.
    gap_lo: usize,
    gap_hi: usize,
    gap_sequence: u64,
    disabled_openers_through: Option<usize>,
    line: usize,
    block_line: usize,
    pub scanner: Scanner,
    column_offset: isize,
    line_offset: usize,
    inline_footnote_depth: usize,
    in_table_cell: bool,
    inline_note_closers: Option<Vec<(usize, usize)>>,
    flags: HtmlSkipFlags,
    html_scope: HtmlScope,
    html_code_frames: SmallVec<[CodeScope; 8]>,
    html_code_blocked_through: usize,
    html_code_pairs: Option<HtmlCodePairs>,
    html_specials: Option<Vec<(usize, usize)>>,
    probe_specials: Option<&'r [(usize, usize)]>,
    image_backticks: Option<Box<ProbeBackticks>>,
    probe_backticks: Option<&'r ProbeBackticks>,
    image_math: Option<Box<ProbeMathClosers>>,
    probe_math: Option<&'r ProbeMathClosers>,
    image_tokens: Option<ProbeImageTokens>,
    image_fallback_through: usize,
    image_probe_memo: Option<ImageProbeMemo>,
    observed_image: Option<ObservedImage>,
    recording_images: Option<&'r mut ImageProbeMemo>,
    inherited_html_opacity: bool,
    pub refmap: &'r RefMap,
    tables: &'r ByteTables,
    footnote_defs: &'p mut FootnoteDefs<'a>,
    delimiter_arena: &'d native::FallibleArena<Delimiter<'a, 'd>, H::Lease>,
    last_delimiter: Option<&'d Delimiter<'a, 'd>>,
    brackets: SmallVec<[Bracket<'a>; 8]>,
    pub backticks: [usize; MAXBACKTICKS + 1],
    backticks_valid: u128,
    pub scanned_for_backticks: bool,
    no_link_openers: bool,
    pub(crate) relaxed_autolink_dead_zone_end: usize,
}

#[derive(Clone, Copy, Default)]
struct HtmlSkipFlags {
    cdata: bool,
    declaration: bool,
    pi: bool,
    comment: bool,
}

// The input starts just after '<'. Failed special-tag scans are remembered by
// the caller so repeated comments or instructions cannot rescan the leaf.
fn inline_html_len<H: Host>(
    input: &str,
    flags: &mut HtmlSkipFlags,
    ctx: &mut Context<'_, H>,
) -> Result<Option<usize>, H::Error> {
    let bytes = input.as_bytes();
    if bytes.len() < 2 {
        return Ok(None);
    }
    if bytes[0] == b'!' && !flags.comment {
        if bytes.starts_with(b"!--") {
            if bytes.starts_with(b"!-->") {
                return Ok(Some(4));
            }
            if bytes.starts_with(b"!--->") {
                return Ok(Some(5));
            }
            if let Some(len) = scanners::html_comment_with_host(&input[1..], ctx)? {
                return Ok(Some(len + 1));
            }
            flags.comment = true;
        } else if bytes[1] == b'[' {
            if !flags.cdata
                && bytes.len() >= 3
                && let Some(len) = scanners::html_cdata_with_host(&input[2..], ctx)?
            {
                if len + 5 <= bytes.len() {
                    return Ok(Some(len + 5));
                }
                flags.cdata = true;
            }
        } else if !flags.declaration
            && let Some(len) = scanners::html_declaration_with_host(&input[1..], ctx)?
        {
            if len + 2 <= bytes.len() {
                return Ok(Some(len + 2));
            }
            flags.declaration = true;
        }
        return Ok(None);
    }
    if bytes[0] == b'?' {
        if !flags.pi {
            let len =
                scanners::html_processing_instruction_with_host(&input[1..], ctx)?.unwrap_or(0);
            if len + 3 <= bytes.len() {
                return Ok(Some(len + 3));
            }
            flags.pi = true;
        }
        return Ok(None);
    }
    scanners::html_tag_with_host(input, ctx)
}

// Lookahead certificates can start at any accepted tag. An unterminated
// comment rules out a later ordinary comment closer, but does not rule out
// declarations, CDATA, or the two short comments handled by Comrak.
fn html_code_inline_len<H: Host>(
    input: &str,
    flags: &mut HtmlSkipFlags,
    ctx: &mut Context<'_, H>,
) -> Result<Option<usize>, H::Error> {
    let failed_comment = flags.comment;
    if failed_comment && input.starts_with("!--") {
        return Ok(if input.starts_with("!-->") {
            Some(4)
        } else if input.starts_with("!--->") {
            Some(5)
        } else {
            None
        });
    }
    flags.comment = false;
    let result = inline_html_len(input, flags, ctx);
    flags.comment |= failed_comment;
    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HtmlCodeKind {
    Code,
    Keyboard,
    Sample,
}

#[derive(Clone, Copy)]
enum OpaqueTag {
    Code {
        kind: HtmlCodeKind,
        closing: bool,
        exact: bool,
        self_closing: bool,
    },
    Raw {
        name: &'static [u8],
        closing: bool,
    },
}

struct HtmlCodeToken {
    start: usize,
    end: usize,
    tag: Option<OpaqueTag>,
    next: usize,
    // Close token ordinal plus one for each active kind and ancestor mask.
    closes: [[u32; 8]; 3],
}

struct HtmlCodePairs(Vec<HtmlCodeToken>);

struct ProbeBackticks([Vec<usize>; MAXBACKTICKS + 1]);

impl ProbeBackticks {
    fn build<H: Host>(
        input: &str,
        mut at: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Box<Self>, H::Error> {
        let mut index = ctx.boxed(Self(std::array::from_fn(|_| Vec::new())))?;
        let bytes = input.as_bytes();
        let matcher = backtick_matcher();
        while let Some(offset) = ctx.find(&bytes[at..], |chunk| matcher.find(chunk))? {
            let start = at + offset;
            at = start + 1;
            while bytes.get(at) == Some(&b'`') {
                ctx.scan_step()?;
                at += 1;
            }
            let width = at - start;
            if width <= MAXBACKTICKS {
                ctx.push(&mut index.0[width], at)?;
            }
        }
        Ok(index)
    }

    fn closing<H: Host>(
        &self,
        bytes: &[u8],
        start: usize,
        limit: usize,
        width: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<usize>, H::Error> {
        let ends = &self.0[width];
        let index = HtmlCodePairs::lower_bound(ends.len(), start + width, |i| ends[i], ctx)?;
        if let Some(&end) = ends.get(index)
            && end <= limit
        {
            return Ok(Some(end));
        }
        // The ordinary scanner sees a shorter run at the edge of a gap.
        if bytes.get(limit) == Some(&b'`') && limit >= start + width {
            let run_start = limit - width;
            ctx.scan(width + 2)?;
            if bytes[run_start..limit].iter().all(|&b| b == b'`')
                && (run_start == start || bytes[run_start - 1] != b'`')
            {
                return Ok(Some(limit));
            }
        }
        Ok(None)
    }
}

#[derive(Clone, Copy)]
enum ProbeMathCloser {
    LatexParen,
    LatexBracket,
    SingleDollar,
    DoubleDollar,
    CodeDollar,
}

struct ProbeMathClosers([Vec<usize>; 5]);

impl ProbeMathClosers {
    fn indexed<'index, H: Host>(
        cache: &'index mut Option<Box<Self>>,
        input: &str,
        start: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<&'index Self, H::Error> {
        if cache.is_none() {
            *cache = Some(Self::build(input, start, ctx)?);
        }
        Ok(cache.as_deref().unwrap())
    }

    fn dollar_end<H: Host>(
        &self,
        input: &[u8],
        start: usize,
        width: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<usize>, H::Error> {
        let kind = if width == 1 {
            ProbeMathCloser::SingleDollar
        } else {
            ProbeMathCloser::DoubleDollar
        };
        Ok(self
            .first(kind, start, input.len(), ctx)?
            .filter(|&pos| {
                width != 1
                    || (!isspace(input[pos - 1])
                        && !input.get(pos + 1).copied().is_some_and(isdigit))
            })
            .map(|pos| pos + width))
    }

    fn build<H: Host>(
        input: &str,
        start: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Box<Self>, H::Error> {
        let mut index = ctx.boxed(Self(std::array::from_fn(|_| Vec::new())))?;
        let bytes = input.as_bytes();
        for at in start..bytes.len() {
            ctx.scan_step()?;
            match bytes[at] {
                b'\\' => {
                    let kind = match bytes.get(at + 1) {
                        Some(b')') => ProbeMathCloser::LatexParen,
                        Some(b']') => ProbeMathCloser::LatexBracket,
                        _ => continue,
                    };
                    ctx.push(&mut index.0[kind as usize], at)?;
                }
                b'$' => {
                    let previous = at.checked_sub(1).map(|pos| bytes[pos]);
                    if previous != Some(b'\\') {
                        ctx.push(&mut index.0[ProbeMathCloser::SingleDollar as usize], at)?;
                    }
                    if previous == Some(b'`') {
                        ctx.push(&mut index.0[ProbeMathCloser::CodeDollar as usize], at)?;
                    }
                    if bytes.get(at + 1) == Some(&b'$') {
                        ctx.push(&mut index.0[ProbeMathCloser::DoubleDollar as usize], at)?;
                    }
                }
                _ => {}
            }
        }
        Ok(index)
    }

    fn first<H: Host>(
        &self,
        kind: ProbeMathCloser,
        start: usize,
        limit: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<usize>, H::Error> {
        let positions = &self.0[kind as usize];
        let offset = HtmlCodePairs::lower_bound(positions.len(), start, |i| positions[i], ctx)?;
        let width = if matches!(
            kind,
            ProbeMathCloser::SingleDollar | ProbeMathCloser::CodeDollar
        ) {
            1
        } else {
            2
        };
        Ok(positions
            .get(offset)
            .copied()
            .filter(|&pos| pos + width <= limit))
    }
}

fn node_for_probe<'a, H: Host>(
    ctx: &mut Context<'_, H>,
    arena: &'a native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
    line: usize,
) -> Result<Node<'a>, H::Error> {
    make_inline(
        ctx,
        arena,
        NodeValue::Text("".into()),
        (line, 1, line, 1).into(),
    )
}

/// The generated scanners recognize these terminators with a bounded
/// lookahead. Index their suffix states once because an accepted special
/// token can contain many other potential openings with the same terminator.
fn html_code_special_tokens<H: Host>(
    input: &[u8],
    start: usize,
    ctx: &mut Context<'_, H>,
) -> Result<Vec<(usize, usize)>, H::Error> {
    let mut tokens = Vec::new();
    let mut next_gt = 0;
    let mut comment = [0; 3];
    let mut instruction = [0; 3];
    let mut cdata = [0; 3];
    for at in (start..input.len()).rev() {
        ctx.scan_step()?;
        let slot = at % 3;
        comment[slot] = html_code_special_end(input, at, b'-', true, &comment);
        instruction[slot] = html_code_special_end(input, at, b'?', false, &instruction);
        cdata[slot] = html_code_special_end(input, at, b']', true, &cdata);
        if input[at] == b'>' {
            next_gt = at + 1;
        }

        // A new declaration can begin at the first body byte of an earlier
        // special token. Record its larger opening offset first.
        if next_gt != 0
            && input[at..].starts_with(b"<!")
            && input.get(at + 2).is_some_and(u8::is_ascii_uppercase)
        {
            let mut name_end = at + 3;
            while input.get(name_end).is_some_and(u8::is_ascii_uppercase) {
                ctx.scan_step()?;
                name_end += 1;
            }
            if input.get(name_end).is_some_and(|&b| tag_space(b)) {
                ctx.push(&mut tokens, (at, next_gt))?;
            }
        }

        let left = &input[start..at];
        let token = if left.ends_with(b"<!--") {
            let end = if input[at] == b'>' {
                at + 1
            } else if input[at..].starts_with(b"->") {
                at + 2
            } else {
                comment[slot]
            };
            Some((at - 4, end))
        } else if left.ends_with(b"<?") {
            Some((at - 2, instruction[slot]))
        } else if left.len() >= 9 && left[left.len() - 9..].eq_ignore_ascii_case(b"<![CDATA[") {
            Some((at - 9, cdata[slot]))
        } else {
            None
        };
        if let Some((start, end)) = token
            && end != 0
        {
            ctx.push(&mut tokens, (start, end))?;
        }
    }
    Ok(tokens)
}

fn probe_special_html_len<H: Host>(
    input: &str,
    open: usize,
    flags: &mut HtmlSkipFlags,
    index: &[(usize, usize)],
    ctx: &mut Context<'_, H>,
) -> Result<Option<usize>, H::Error> {
    let bytes = input.as_bytes();
    if bytes.len() < 2 {
        return Ok(None);
    }
    let failed = match bytes[0] {
        b'?' => &mut flags.pi,
        b'!' if flags.comment => return Ok(None),
        b'!' if bytes.starts_with(b"!--") => &mut flags.comment,
        b'!' if bytes[1] == b'[' => {
            if !bytes
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"![CDATA["))
            {
                return Ok(None);
            }
            &mut flags.cdata
        }
        b'!' => {
            let mut end = 1;
            while bytes.get(end).is_some_and(u8::is_ascii_uppercase) {
                ctx.scan_step()?;
                end += 1;
            }
            if end == 1 || !bytes.get(end).is_some_and(|&byte| tag_space(byte)) {
                return Ok(None);
            }
            &mut flags.declaration
        }
        _ => return Err(ctx.host.reject(NativeViolation::InvalidProvenance)),
    };
    if *failed {
        return Ok(None);
    }
    let offset =
        HtmlCodePairs::lower_bound(index.len(), open, |i| index[index.len() - 1 - i].0, ctx)?;
    if offset < index.len() {
        let (start, end) = index[index.len() - 1 - offset];
        if start == open && end - open - 1 <= bytes.len() {
            return Ok(Some(end - open - 1));
        }
    }
    *failed = true;
    Ok(None)
}

fn html_code_special_end(
    input: &[u8],
    at: usize,
    marker: u8,
    double: bool,
    suffix: &[usize; 3],
) -> usize {
    if input[at] != marker {
        return suffix[(at + 1) % 3];
    }
    if double && input.get(at + 1) != Some(&marker) {
        return suffix[(at + 2) % 3];
    }
    let after = at + if double { 2 } else { 1 };
    if input.get(after) == Some(&b'>') {
        after + 1
    } else {
        suffix[(after + 1) % 3]
    }
}

impl HtmlCodePairs {
    fn build<H: Host>(
        input: &str,
        mut start: usize,
        special: &[(usize, usize)],
        ctx: &mut Context<'_, H>,
    ) -> Result<Self, H::Error> {
        let mut tokens = Vec::new();
        let mut raw_closers: [Vec<usize>; RAW_TEXT_TAGS.len()] = Default::default();
        let skipped = Self::lower_bound(
            special.len(),
            start,
            |i| special[special.len() - 1 - i].0,
            ctx,
        )?;
        let mut special = &special[..special.len() - skipped];
        while let Some(offset) = ctx.find(&input.as_bytes()[start..], |bytes| {
            bytes.iter().position(|&b| b == b'<')
        })? {
            let at = start + offset;
            start = at + 1;
            let end = if matches!(input.as_bytes().get(start), Some(b'!' | b'?')) {
                let Some(&(open, end)) = special.last() else {
                    continue;
                };
                if open != at {
                    continue;
                }
                special = &special[..special.len() - 1];
                if end > input.len() {
                    continue;
                }
                end
            } else {
                let Some(width) = scanners::html_tag_with_host(&input[start..], ctx)? else {
                    continue;
                };
                start + width
            };
            let tag = accepted_opaque_tag(&input.as_bytes()[at..end], ctx)?;
            // An ordinary token only matters to pairing if it hides a later
            // angle. Keeping inert tokens would inflate literal code bodies.
            if tag.is_none()
                && ctx
                    .find(&input.as_bytes()[start..end], |bytes| {
                        bytes.iter().position(|&b| b == b'<')
                    })?
                    .is_none()
            {
                continue;
            }
            if let Some(OpaqueTag::Raw {
                name,
                closing: true,
            }) = tag
            {
                let index = Self::raw_index(name);
                ctx.push(&mut raw_closers[index], at)?;
            }
            ctx.push(
                &mut tokens,
                HtmlCodeToken {
                    start: at,
                    end,
                    tag,
                    next: 0,
                    closes: [[0; 8]; 3],
                },
            )?;
        }

        let mut pairs = Self(tokens);
        for position in (0..pairs.0.len()).rev() {
            ctx.event()?;
            let end = pairs.0[position].end;
            let tag = pairs.0[position].tag;
            let next = pairs.at_or_after(end, ctx)?;
            let ordinal = u32::try_from(position + 1)
                .map_err(|_| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            let raw_end = match tag {
                Some(OpaqueTag::Raw {
                    name: b"plaintext",
                    closing: false,
                }) => None,
                Some(OpaqueTag::Raw {
                    name,
                    closing: false,
                }) => {
                    let closers = &raw_closers[Self::raw_index(name)];
                    let index = Self::lower_bound(closers.len(), end, |i| closers[i], ctx)?;
                    if let Some(&close) = closers.get(index) {
                        let close = pairs.at_or_after(close, ctx)?;
                        Some(pairs.at_or_after(pairs.0[close].end, ctx)?)
                    } else {
                        None
                    }
                }
                _ => Some(next),
            };
            let mut closes = [[0; 8]; 3];
            for (kind, row) in closes.iter_mut().enumerate() {
                for (ancestors, answer) in row.iter_mut().enumerate() {
                    *answer = match tag {
                        Some(OpaqueTag::Code {
                            kind: tag,
                            closing: true,
                            ..
                        }) if tag as usize == kind => ordinal,
                        Some(OpaqueTag::Code {
                            kind: tag,
                            closing: true,
                            ..
                        }) if ancestors & (1 << tag as usize) != 0 => 0,
                        Some(OpaqueTag::Code {
                            kind: tag,
                            closing: false,
                            self_closing: false,
                            ..
                        }) => {
                            let inner = pairs.answer(next, tag as usize, ancestors | (1 << kind));
                            if let Some(inner) = inner.checked_sub(1) {
                                let after = pairs.0[inner as usize].next;
                                pairs.answer(after, kind, ancestors)
                            } else {
                                0
                            }
                        }
                        _ => raw_end.map_or(0, |after| pairs.answer(after, kind, ancestors)),
                    };
                }
            }
            ctx.host.work(native::WorkDelta::Copy(
                std::mem::size_of_val(&closes) as u64
            ))?;
            pairs.0[position].next = next;
            pairs.0[position].closes = closes;
        }
        for closers in raw_closers {
            ctx.discard_vec(closers)?;
        }
        Ok(pairs)
    }

    fn raw_index(name: &[u8]) -> usize {
        RAW_TEXT_TAGS.iter().position(|&tag| tag == name).unwrap()
    }

    fn lower_bound<H: Host>(
        len: usize,
        at: usize,
        start: impl Fn(usize) -> usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<usize, H::Error> {
        let (mut lo, mut hi) = (0, len);
        while lo < hi {
            ctx.event()?;
            let mid = lo + (hi - lo) / 2;
            if start(mid) < at {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Ok(lo)
    }

    fn at_or_after<H: Host>(&self, at: usize, ctx: &mut Context<'_, H>) -> Result<usize, H::Error> {
        Self::lower_bound(self.0.len(), at, |i| self.0[i].start, ctx)
    }

    fn answer(&self, at: usize, kind: usize, ancestors: usize) -> u32 {
        self.0
            .get(at)
            .map_or(0, |token| token.closes[kind][ancestors])
    }

    fn find<H: Host>(
        &self,
        body: usize,
        kind: HtmlCodeKind,
        limit: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<(usize, usize)>, H::Error> {
        let start = self.at_or_after(body, ctx)?;
        let Some(ordinal) = self.answer(start, kind as usize, 0).checked_sub(1) else {
            return Ok(None);
        };
        let close = &self.0[ordinal as usize];
        Ok(
            (close.end <= limit && matches!(close.tag, Some(OpaqueTag::Code { exact: true, .. })))
                .then_some((close.start, close.end)),
        )
    }
}

fn tag_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\x0b' | b'\x0c' | b'\r' | b'\n')
}

fn accepted_opaque_tag<H: Host>(
    text: &[u8],
    ctx: &mut Context<'_, H>,
) -> Result<Option<OpaqueTag>, H::Error> {
    let Some(text) = text.strip_prefix(b"<") else {
        return Ok(None);
    };
    let (closing, text) = match text.strip_prefix(b"/") {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let Some(end) = text
        .iter()
        .take(10)
        .position(|byte| !byte.is_ascii_alphanumeric() && *byte != b'-')
    else {
        return Ok(None);
    };
    let (name, tail) = text.split_at(end);
    ctx.scan(end + 2)?;
    let code = [
        (b"code".as_slice(), HtmlCodeKind::Code),
        (b"kbd".as_slice(), HtmlCodeKind::Keyboard),
        (b"samp".as_slice(), HtmlCodeKind::Sample),
    ];
    if let Some((_, kind)) = code
        .into_iter()
        .find(|(tag, _)| tag.eq_ignore_ascii_case(name))
    {
        let exact = (text.len() + 1 + usize::from(closing)) <= 34
            && tail
                .strip_suffix(b">")
                .is_some_and(|rest| rest.iter().all(|byte| matches!(byte, b' ' | b'\t')));
        return Ok(Some(OpaqueTag::Code {
            kind,
            closing,
            exact,
            self_closing: !closing && code_tag_self_closes(tail, ctx)?,
        }));
    }
    Ok(RAW_TEXT_TAGS
        .into_iter()
        .find(|tag| tag.eq_ignore_ascii_case(name))
        .map(|name| OpaqueTag::Raw { name, closing }))
}

// The upstream scanner has accepted the complete opening tag. In the
// unquoted attribute grammar, a final slash can be part of the value.
fn code_tag_self_closes<H: Host>(tail: &[u8], ctx: &mut Context<'_, H>) -> Result<bool, H::Error> {
    if !tail.ends_with(b"/>") {
        return Ok(false);
    }
    #[derive(Clone, Copy)]
    enum Value {
        Outside,
        Before,
        Unquoted,
        Quoted(u8),
    }
    let mut value = Value::Outside;
    for &byte in &tail[..tail.len() - 2] {
        ctx.scan_step()?;
        value = match value {
            Value::Outside if byte == b'=' => Value::Before,
            Value::Before if matches!(byte, b'\'' | b'"') => Value::Quoted(byte),
            Value::Before if !tag_space(byte) => Value::Unquoted,
            Value::Unquoted if tag_space(byte) => Value::Outside,
            Value::Quoted(quote) if byte == quote => Value::Outside,
            _ => value,
        };
    }
    Ok(matches!(value, Value::Outside))
}

#[derive(Clone, Copy, Default)]
struct HtmlScope {
    code: Option<usize>,
    raw: Option<&'static [u8]>,
}

#[derive(Clone, Copy)]
struct CodeScope {
    parent: Option<usize>,
    kind: HtmlCodeKind,
    kinds: u8,
}

impl HtmlScope {
    fn opaque(self) -> bool {
        self.raw.is_some() || self.code.is_some()
    }

    // Return false for a crossed active code pair. Its whole group stays
    // literal; clearing the code scope lets a later sibling start afresh.
    fn advance<H: Host>(
        &mut self,
        tag: OpaqueTag,
        frames: &mut SmallVec<[CodeScope; 8]>,
        ctx: &mut Context<'_, H>,
    ) -> Result<bool, H::Error> {
        ctx.event()?;
        if let Some(open) = self.raw {
            if matches!(tag, OpaqueTag::Raw { name, closing: true } if name == open && name != b"plaintext")
            {
                self.raw = None;
            }
            return Ok(true);
        }
        match tag {
            OpaqueTag::Raw {
                name,
                closing: false,
            } => self.raw = Some(name),
            OpaqueTag::Code {
                kind,
                closing: false,
                self_closing: false,
                ..
            } => {
                let index = frames.len();
                let kinds = self.code.map_or(0, |at| frames[at].kinds) | (1 << kind as u8);
                ctx.small_push(
                    frames,
                    CodeScope {
                        parent: self.code,
                        kind,
                        kinds,
                    },
                )?;
                self.code = Some(index);
            }
            OpaqueTag::Code {
                kind,
                closing: true,
                ..
            } if let Some(at) = self.code => {
                let frame = frames[at];
                if frame.kind == kind {
                    self.code = frame.parent;
                } else if frame.kinds & (1 << kind as u8) != 0 {
                    self.code = None;
                    return Ok(false);
                }
            }
            _ => {}
        }
        Ok(true)
    }
}

impl<'a, 'r, 'o, 'd, 'c, 'p, 'h, H: Host> Subject<'a, 'r, 'o, 'd, 'c, 'p, 'h, H> {
    pub fn new(
        ctx: &'r mut Context<'h, H>,
        buffer: Option<BufferId>,
        base: usize,
        arena: &'a native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
        options: &'o Options<'c>,
        input: impl Into<Cow<'r, str>>,
        line: usize,
        refmap: &'r RefMap,
        tables: &'r ByteTables,
        footnote_defs: &'p mut FootnoteDefs<'a>,
        delimiter_arena: &'d native::FallibleArena<Delimiter<'a, 'd>, H::Lease>,
        inline_footnote_depth: usize,
    ) -> Self {
        let input = input.into();
        let gap_hi = input.len();
        Subject {
            ctx,
            buffer,
            base,
            arena,
            options,
            input,
            gap_lo: 0,
            gap_hi,
            gap_sequence: 0,
            disabled_openers_through: None,
            line,
            block_line: line,
            scanner: Scanner::new(),
            column_offset: 0,
            line_offset: 0,
            inline_footnote_depth,
            in_table_cell: false,
            inline_note_closers: None,
            flags: HtmlSkipFlags::default(),
            html_scope: HtmlScope::default(),
            html_code_frames: SmallVec::new(),
            html_code_blocked_through: 0,
            html_code_pairs: None,
            html_specials: None,
            probe_specials: None,
            image_backticks: None,
            probe_backticks: None,
            image_math: None,
            probe_math: None,
            image_tokens: None,
            image_fallback_through: 0,
            image_probe_memo: None,
            observed_image: None,
            recording_images: None,
            inherited_html_opacity: false,
            refmap,
            tables,
            footnote_defs,
            delimiter_arena,
            last_delimiter: None,
            brackets: SmallVec::new(),
            backticks: [0; MAXBACKTICKS + 1],
            backticks_valid: 0,
            scanned_for_backticks: false,
            no_link_openers: true,
            relaxed_autolink_dead_zone_end: 0,
        }
    }

    fn input_end(&self) -> usize {
        self.gap_hi
    }

    /// One Subject owns every cache and delimiter. No gap can reuse a failed
    /// search certificate from a different sequence, even if endpoints match.
    fn begin_gap(&mut self, lo: usize, hi: usize) -> Result<(), H::Error> {
        self.ctx.event()?;
        self.ctx.scan(2)?;
        if lo != self.scanner.pos
            || lo > hi
            || hi > self.input.len()
            || (self.gap_sequence != 0 && lo < self.gap_hi)
            || !self.input.is_char_boundary(lo)
            || !self.input.is_char_boundary(hi)
        {
            return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let sequence = self
            .gap_sequence
            .checked_add(1)
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        self.ctx.host.work(native::WorkDelta::Copy(
            (std::mem::size_of::<u128>()
                + std::mem::size_of::<HtmlSkipFlags>()
                + std::mem::size_of::<bool>()) as u64,
        ))?;
        // An 81-bit validity mask retires the previous gap's entries without
        // rewriting the entire position array for every short selected link.
        self.backticks_valid = 0;
        self.scanned_for_backticks = false;
        self.flags = HtmlSkipFlags::default();
        if let Some(pairs) = self.html_code_pairs.take() {
            self.ctx.discard_vec(pairs.0)?;
        }
        if let Some(closers) = self.inline_note_closers.take() {
            self.ctx.discard_vec(closers)?;
        }
        self.gap_lo = lo;
        self.gap_hi = hi;
        self.gap_sequence = sequence;
        Ok(())
    }

    pub(crate) fn parse_selected<'leaf>(
        &mut self,
        node: Node<'a>,
        start_line: usize,
        offsets: &[usize],
        mut selection: native::g4::Selection<'leaf, H::Lease>,
    ) -> Result<(), H::Error> {
        if self.gap_sequence != 0
            || self.scanner.pos != 0
            || self.ctx.is_shadow()
            || self.options.extension.autolink
        {
            return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        while let Some(selected) = selection.next(self.ctx)? {
            self.begin_gap(self.scanner.pos, selected.start())?;
            while self.parse_inline(node, start_line, offsets)? {}
            let line = self
                .line
                .checked_sub(start_line)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            self.line_offset = *offsets
                .get(line)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            self.commit_selected(node, selected)?;
        }
        self.begin_gap(self.scanner.pos, self.input.len())?;
        while self.parse_inline(node, start_line, offsets)? {}
        Ok(())
    }

    fn commit_selected(
        &mut self,
        parent: Node<'a>,
        selected: native::g4::SelectedLink<'_>,
    ) -> Result<(), H::Error> {
        self.ctx.event()?;
        let node = parent
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let buffer = self
            .buffer
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let raw = selected.raw_for(node, buffer, self.base, self.input.len(), self.ctx)?;
        let start = selected.start();
        let end = selected.end();
        self.ctx.scan(2)?;
        if self.scanner.pos != start
            || self.gap_hi != start
            || selected.domain_end() <= start
            || selected.domain_end() > end
            || !self.input.is_char_boundary(start)
            || !self.input.is_char_boundary(end)
        {
            return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let payload = self.ctx.payload(native::PayloadKind::Destination, raw)?;
        let (destination, kind) = match selected.kind() {
            native::g4::SelectedKind::Www => {
                self.ctx.admit_decoded((end - start) as u64)?;
                let mut value = self
                    .ctx
                    .decoded_string(native::GeneratedPrefix::Http.text())?;
                self.ctx.append(&mut value, &self.input[start..end])?;
                self.ctx.fact(AcceptedFact::PayloadPrefix {
                    payload,
                    kind: native::GeneratedPrefix::Http,
                    at: RSpan {
                        buffer,
                        start: raw.start,
                        end: raw.start,
                    },
                    value: "http://",
                })?;
                (value, native::g4::ExtendedKind::Www)
            }
            native::g4::SelectedKind::Url => (
                self.ctx.decoded_string(&self.input[start..end])?,
                native::g4::ExtendedKind::Url,
            ),
        };
        self.ctx.fact(AcceptedFact::PayloadEnd {
            payload,
            value: &destination,
        })?;
        let destination = self.ctx.finish_payload_value(
            destination,
            Some(payload),
            native::PayloadKind::Destination,
        )?;
        let link = self.ctx.boxed(NodeLink {
            url: destination,
            title: self.ctx.empty_value(),
        })?;
        let link = self.make_inline(NodeValue::Link(link), start, end - 1)?;
        let label = self.make_inline_text_owned(start, end)?;
        let label_id = label
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let link_id = link
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        link.append(label);
        self.ctx.fact(AcceptedFact::Link {
            node: link_id,
            syntax: raw,
            label: raw,
            image: false,
            wiki: None,
            destination: Some(payload),
            title: None,
            definition: None,
        })?;
        self.ctx
            .fact(AcceptedFact::Extended(native::g4::ExtendedAccepted {
                node: link_id,
                label: label_id,
                kind,
                raw,
                destination: payload,
            }))?;
        parent.append(link);
        self.no_link_openers = true;
        self.disabled_openers_through = Some(start);
        self.scanner.pos = end;
        Ok(())
    }

    //////////////////
    // Constructors //
    //////////////////

    fn make_inline(
        &mut self,
        value: NodeValue,
        start_column: usize,
        end_column: usize,
    ) -> Result<Node<'a>, H::Error> {
        let produced = match &value {
            // This is a not-yet-accepted birth value, not an AST read. Native
            // current-value readers deliberately reject Owned final slots.
            NodeValue::Text(value) | NodeValue::HtmlInline(value) => value
                .owned_str()
                .map(str::len)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?,
            _ => 0,
        };
        self.ctx.admit_decoded(produced as u64)?;
        self.make_inline_admitted(value, start_column, end_column)
    }

    // End is exclusive in the actual input; the AST keeps its legacy inclusive column.
    fn make_inline_text_owned(&mut self, start: usize, end: usize) -> Result<Node<'a>, H::Error> {
        self.ctx.admit_decoded((end - start) as u64)?;
        let value = if self.ctx.direct {
            ValueText::pending()
        } else {
            self.ctx.string(&self.input[start..end])?.into()
        };
        self.make_inline_admitted(NodeValue::Text(value), start, end - 1)
    }

    // Callers have admitted the semantic output once; this builder only owns
    // the shared node/arena/fact path and does not produce another text copy.
    fn make_inline_admitted(
        &mut self,
        value: NodeValue,
        start_column: usize,
        end_column: usize,
    ) -> Result<Node<'a>, H::Error> {
        let result = {
            let raw_start = start_column;
            let raw_end = end_column.saturating_add(1);
            let native_kind = native::kind(&value);
            let text = matches!(value, NodeValue::Text(_));
            let native_id = if self.ctx.capture {
                let native_kind = native_kind.ok_or_else(|| {
                    self.ctx
                        .host
                        .reject(NativeViolation::UnsupportedConfiguration)
                })?;
                Some(self.ctx.node(None, native_kind)?)
            } else {
                None
            };
            let start_column =
                start_column as isize + 1 + self.column_offset + self.line_offset as isize;
            let end_column =
                end_column as isize + 1 + self.column_offset + self.line_offset as isize;

            let ast = Ast {
                value,
                sourcepos: (
                    self.line,
                    usize::try_from(start_column).unwrap(),
                    self.line,
                    usize::try_from(end_column).unwrap(),
                )
                    .into(),
                #[cfg(feature = "attributes")]
                attrs: None,
                native_id,
                native_callout_title: false,
                open: false,
                last_line_blank: false,
                table_visited: false,
                lines: None,
            };
            let node: Node<'a> = self.arena.try_alloc(ast.into(), self.ctx)?;
            if text && let (Some(node), Some(buffer)) = (native_id, self.buffer) {
                self.ctx.fact(AcceptedFact::TextAccepted {
                    node,
                    span: RSpan {
                        buffer,
                        start: self.base + raw_start,
                        end: self.base + raw_end,
                    },
                })?;
            }
            if text && self.ctx.direct {
                // Decoder callers retain their actual output locally until
                // DecodeAtom; this slot first follows the accepted raw map.
                let mut data = node.data_mut();
                let NodeValue::Text(value) = &mut data.value else {
                    unreachable!()
                };
                let previous = mem::replace(value, ValueText::pending());
                *value = self
                    .ctx
                    .finish_node_value(previous, native_id, NodeValueRole::Text)?;
            }
            node
        };
        Ok(result)
    }

    fn make_autolink(
        &mut self,
        url: &str,
        kind: AutolinkType,
        start_column: usize,
        end_column: usize,
    ) -> Result<Node<'a>, H::Error> {
        let result = {
            let raw = self.buffer.map(|buffer| RSpan {
                buffer,
                start: self.base + start_column + 1,
                end: self.base + end_column,
            });
            let payload = match raw.filter(|_| !self.ctx.is_shadow()) {
                Some(raw) => Some((
                    self.ctx.payload(native::PayloadKind::Destination, raw)?,
                    raw,
                )),
                None => None,
            };
            // Shadow grammar needs the accepted kind and real label subtree,
            // never destination bytes. No payload is materialized in A.
            let destination = if self.ctx.is_shadow() {
                String::new()
            } else {
                strings::clean_autolink_with_host(url, kind, self.ctx, payload)?
            };
            let destination = self.ctx.finish_payload_value(
                destination,
                payload.map(|(id, _)| id),
                native::PayloadKind::Destination,
            )?;
            let link = self.ctx.boxed(NodeLink {
                url: destination,
                title: self.ctx.empty_value(),
            })?;
            let inl = self.make_inline(NodeValue::Link(link), start_column, end_column)?;
            let text =
                self.make_inline(NodeValue::Text("".into()), start_column + 1, end_column - 1)?;
            let identity = match (text.data().native_id, raw) {
                (Some(node), Some(raw)) => Some((node, raw)),
                _ => None,
            };
            let label = entity::unescape_html_with_targets(url, self.ctx, None, 1, identity)?;
            let label = self.ctx.finish_node_value(
                label.into(),
                text.data().native_id,
                NodeValueRole::Text,
            )?;
            text.data_mut().value = NodeValue::Text(label);
            inl.append(text);
            if let (Some(node), Some(raw)) = (inl.data().native_id, raw) {
                self.ctx.fact(AcceptedFact::Link {
                    node,
                    syntax: RSpan {
                        buffer: raw.buffer,
                        start: self.base + start_column,
                        end: self.base + end_column + 1,
                    },
                    label: raw,
                    image: false,
                    wiki: None,
                    destination: payload.map(|(id, _)| id),
                    title: None,
                    definition: None,
                })?;
            }
            inl
        };
        Ok(result)
    }

    pub(crate) fn release_owned(mut self) -> Result<(), H::Error> {
        if let Cow::Owned(input) = mem::take(&mut self.input) {
            self.ctx.discard_string(input)?;
        }
        let brackets = mem::take(&mut self.brackets);
        self.ctx.discard_small(brackets)?;
        self.ctx
            .discard_small(mem::take(&mut self.html_code_frames))?;
        if let Some(pairs) = self.html_code_pairs.take() {
            self.ctx.discard_vec(pairs.0)?;
        }
        if let Some(specials) = self.html_specials.take() {
            self.ctx.discard_vec(specials)?;
        }
        if let Some(backticks) = self.image_backticks.take() {
            for width in self.ctx.unbox(backticks)?.0 {
                self.ctx.discard_vec(width)?;
            }
        }
        if let Some(math) = self.image_math.take() {
            for closers in self.ctx.unbox(math)?.0 {
                self.ctx.discard_vec(closers)?;
            }
        }
        if let Some(tokens) = self.image_tokens.take() {
            self.ctx.discard_vec(tokens.structural)?;
            self.ctx.discard_vec(tokens.closers)?;
        }
        if let Some(memo) = self.image_probe_memo.take() {
            self.ctx.discard_vec(memo.0)?;
        }
        if let Some(closers) = self.inline_note_closers.take() {
            self.ctx.discard_vec(closers)?;
        }
        Ok(())
    }

    /////////////
    // Parsers //
    /////////////

    pub fn parse_inline(
        &mut self,
        node: Node<'a>,
        start_line: usize,
        line_offsets: &[usize],
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            if self.scanner.pos < self.gap_lo || self.scanner.pos > self.gap_hi {
                return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            let Some(b) = self.peek_byte() else {
                break 'native false;
            };

            let adjusted_line = self.line - start_line;
            self.line_offset = line_offsets[adjusted_line];
            self.in_table_cell = matches!(node.data().value, NodeValue::TableCell);

            let new_inl: Option<Node<'a>> = match b {
                b'\r' | b'\n' => Some(self.handle_newline()?),
                b'`' => Some(self.handle_backticks(line_offsets)?),
                b'=' if self.options.extension.highlight => Some(self.handle_delim(b'=')?),
                b'+' if self.options.extension.insert => Some(self.handle_delim(b'+')?),
                b'\\' => Some(self.handle_backslash(
                    line_offsets,
                    matches!(node.data().value, NodeValue::Paragraph),
                )?),
                b'&' => Some(self.handle_entity()?),
                b'<' => Some(self.handle_pointy_brace(
                    line_offsets,
                    matches!(node.data().value, NodeValue::Paragraph),
                )?),
                #[cfg(feature = "phoenix_heex")]
                b'{' => {
                    let mut res = None;

                    if self.options.extension.phoenix_heex {
                        res = self.handle_heex_inline_expression(line_offsets)?;
                    }

                    if res.is_none() {
                        self.scanner.pos += 1;
                        res = Some(self.make_inline(
                            NodeValue::Text("{".into()),
                            self.scanner.pos - 1,
                            self.scanner.pos - 1,
                        )?);
                    }

                    res
                }
                b':' => {
                    let mut res = None;

                    if self.options.extension.autolink {
                        res = self.handle_autolink_with(node, autolink::url_match)?;
                    }

                    #[cfg(feature = "shortcodes")]
                    if res.is_none() && self.options.extension.shortcodes {
                        res = self.handle_shortcodes_colon()?;
                    }

                    if res.is_none() {
                        self.scanner.pos += 1;
                        res = Some(self.make_inline(
                            NodeValue::Text(":".into()),
                            self.scanner.pos - 1,
                            self.scanner.pos - 1,
                        )?);
                    }

                    res
                }
                b'w' if self.options.extension.autolink => {
                    match self.handle_autolink_with(node, autolink::www_match)? {
                        Some(inl) => Some(inl),
                        None => {
                            self.scanner.pos += 1;
                            Some(self.make_inline(
                                NodeValue::Text("w".into()),
                                self.scanner.pos - 1,
                                self.scanner.pos - 1,
                            )?)
                        }
                    }
                }
                b'*' | b'_' | b'\'' | b'"' => Some(self.handle_delim(b)?),
                b'-' => Some(self.handle_hyphen()?),
                b'.' => Some(self.handle_period()?),
                b'[' => {
                    self.scanner.pos += 1;

                    let mut wikilink_inl = None;

                    if self.brackets.is_empty() && self.peek_byte() == Some(b'[') {
                        if (self.ctx.capture || self.ctx.is_boundary_probe())
                            && self.options.extension.native_wiki_links
                        {
                            wikilink_inl = self.handle_native_wiki_link(
                                false,
                                matches!(node.data().value, NodeValue::TableCell),
                            )?;
                        } else if self.options.extension.wikilinks().is_some() {
                            wikilink_inl = self.handle_wikilink()?;
                        }
                    }

                    if wikilink_inl.is_none() {
                        let inl = self.make_inline(
                            NodeValue::Text("[".into()),
                            self.scanner.pos - 1,
                            self.scanner.pos - 1,
                        )?;
                        self.push_bracket(false, inl, line_offsets, false)?;

                        Some(inl)
                    } else {
                        wikilink_inl
                    }
                }
                b']' => self.handle_close_bracket(line_offsets)?,
                b'!' => {
                    self.scanner.pos += 1;
                    let mut embed = None;
                    if (self.ctx.capture || self.ctx.is_boundary_probe())
                        && (self.options.extension.native_wiki_links
                            || self.options.extension.native_note_embeds)
                        && self.peek_byte() == Some(b'[')
                        && self.peek_byte_n(1) == Some(b'[')
                    {
                        self.scanner.pos += 1;
                        embed = self.handle_native_wiki_link(
                            true,
                            matches!(node.data().value, NodeValue::TableCell),
                        )?;
                        if embed.is_none() {
                            self.scanner.pos -= 1;
                        }
                    }
                    if embed.is_some() {
                        embed
                    } else if self.peek_byte() == Some(b'[')
                        && (self.peek_byte_n(1) != Some(b'^')
                            || (self.ctx.capture || self.ctx.is_boundary_probe())
                                && self.options.extension.footnotes)
                    {
                        self.scanner.pos += 1;
                        let inl = self.make_inline(
                            NodeValue::Text("![".into()),
                            self.scanner.pos - 2,
                            self.scanner.pos - 1,
                        )?;
                        self.push_bracket(
                            true,
                            inl,
                            line_offsets,
                            matches!(node.data().value, NodeValue::Paragraph),
                        )?;
                        Some(inl)
                    } else {
                        Some(self.make_inline(
                            NodeValue::Text("!".into()),
                            self.scanner.pos - 1,
                            self.scanner.pos - 1,
                        )?)
                    }
                }
                b'~' if self.options.extension.strikethrough
                    || self.options.extension.subscript =>
                {
                    Some(self.handle_delim(b'~')?)
                }
                b'^' if self.is_special_char_at(self.scanner.pos) => {
                    // Check for inline footnote first
                    let mapped = self.ctx.capture || self.ctx.is_boundary_probe();
                    if self.options.extension.footnotes
                        && self.options.extension.inline_footnotes
                        && self.inline_footnote_depth < MAX_INLINE_FOOTNOTE_DEPTH
                        && self.peek_byte_n(1) == Some(b'[')
                        && (!mapped
                            || (self.brackets.is_empty()
                                && !self.html_scope.opaque()
                                && !self.inherited_html_opacity))
                    {
                        if mapped {
                            Some(self.handle_mapped_inline_note(line_offsets)?)
                        } else {
                            self.handle_inline_footnote()?
                        }
                    } else if self.options.extension.superscript && self.brackets.is_empty() {
                        Some(self.handle_delim(b'^')?)
                    } else {
                        // Just regular text
                        self.scanner.pos += 1;
                        Some(self.make_inline(
                            NodeValue::Text("^".into()),
                            self.scanner.pos - 1,
                            self.scanner.pos - 1,
                        )?)
                    }
                }
                b'$' => Some(self.handle_dollars(line_offsets)?),
                b'>' if self.options.parse.smart => Some(self.handle_guillemet_close()?),
                b'|' if self.options.extension.spoiler => Some(self.handle_delim(b'|')?),
                _ => {
                    let mut endpos = self.find_special_char()?;
                    let startpos = self.scanner.pos;
                    self.scanner.pos = endpos;

                    if self.peek_byte().is_some_and(strings::is_line_end_char) {
                        let before = endpos;
                        while endpos > startpos && isspace(self.input.as_bytes()[endpos - 1]) {
                            self.ctx.scan_step()?;
                            endpos -= 1;
                        }
                        if let Some(buffer) = self.buffer
                            && before != endpos
                        {
                            self.ctx.fact(AcceptedFact::Cut {
                                span: RSpan {
                                    buffer,
                                    start: self.base + endpos,
                                    end: self.base + before,
                                },
                                reason: SyntaxRole::InlineTrim,
                            })?;
                        }
                    }
                    // Don't create empty text nodes - this can happen after trimming trailing
                    // whitespace, is useless, and would cause sourcepos underflow in endpos - 1.
                    let contents = if self.ctx.direct {
                        None
                    } else {
                        Some(self.ctx.string(&self.input[startpos..endpos])?)
                    };
                    if startpos != endpos {
                        Some(match contents {
                            Some(contents) => self.make_inline(
                                NodeValue::Text(contents.into()),
                                startpos,
                                endpos - 1,
                            )?,
                            None => self.make_inline_text_owned(startpos, endpos)?,
                        })
                    } else {
                        None
                    }
                }
            };

            if let Some(inl) = new_inl {
                node.append(inl);
            }

            true
        };
        Ok(result)
    }

    fn handle_newline(&mut self) -> Result<Node<'a>, H::Error> {
        let result = {
            let nlpos = self.scanner.pos;
            if self.peek_byte() == Some(b'\r') {
                self.scanner.pos += 1;
            }
            if self.peek_byte() == Some(b'\n') {
                self.scanner.pos += 1;
            }
            assert_ne!(nlpos, self.scanner.pos);

            let spaces = nlpos - self.gap_lo >= 2
                && self.input.as_bytes()[nlpos - 1] == b' '
                && self.input.as_bytes()[nlpos - 2] == b' ';
            let inl = if spaces {
                self.make_inline(NodeValue::LineBreak, nlpos - 2, self.scanner.pos - 1)?
            } else {
                self.make_inline(NodeValue::SoftBreak, nlpos, self.scanner.pos - 1)?
            };
            self.line += 1;
            self.column_offset = -(self.scanner.pos as isize);
            let ending_end = self.scanner.pos;
            self.skip_spaces()?;
            if let (Some(node), Some(buffer)) = (inl.data().native_id, self.buffer) {
                self.ctx.fact(AcceptedFact::Break {
                    node,
                    kind: if spaces {
                        native::BreakKind::Spaces
                    } else {
                        native::BreakKind::Soft
                    },
                    marker: if spaces {
                        Some(RSpan {
                            buffer,
                            start: self.base + nlpos - 2,
                            end: self.base + nlpos,
                        })
                    } else {
                        None
                    },
                    ending: RSpan {
                        buffer,
                        start: self.base + nlpos,
                        end: self.base + ending_end,
                    },
                    prefix: RSpan {
                        buffer,
                        start: self.base + ending_end,
                        end: self.base + self.scanner.pos,
                    },
                })?;
            }
            inl
        };
        Ok(result)
    }

    fn handle_backticks(&mut self, parent_line_offsets: &[usize]) -> Result<Node<'a>, H::Error> {
        let result = 'native: {
            let startpos = self.scanner.pos;
            let openticks = self.take_while(b'`')?;
            let endpos = self.scan_to_closing_backtick(openticks)?;

            let Some(endpos) = endpos else {
                self.scanner.pos = startpos + openticks;
                break 'native self.make_inline_text_owned(startpos, self.scanner.pos)?;
            };

            if self.ctx.is_boundary_probe() {
                return node_for_probe(self.ctx, self.arena, self.line);
            }
            let code = NodeCode {
                num_backticks: openticks,
                literal: if self.ctx.direct {
                    ValueText::pending()
                } else {
                    String::new().into()
                },
            };
            let node = self.make_inline(NodeValue::Code(code), startpos, endpos - 1)?;
            let source = match (node.data().native_id, self.buffer) {
                (Some(id), Some(buffer)) => {
                    self.ctx.fact(AcceptedFact::Code {
                        node: id,
                        syntax: RSpan {
                            buffer,
                            start: self.base + startpos,
                            end: self.base + endpos,
                        },
                        delimiter_bytes: openticks,
                    })?;
                    Some((
                        id,
                        RSpan {
                            buffer,
                            start: self.base + startpos + openticks,
                            end: self.base + endpos - openticks,
                        },
                    ))
                }
                _ => None,
            };
            let literal = strings::normalize_code_with_host(
                &self.input[startpos + openticks..endpos - openticks],
                self.ctx,
                source,
            )?;
            let literal = self.ctx.finish_node_value(
                literal.into(),
                node.data().native_id,
                NodeValueRole::InlineCode,
            )?;
            if let NodeValue::Code(code) = &mut node.data_mut().value {
                code.literal = literal;
            }
            self.adjust_node_newlines(
                node,
                endpos - startpos - openticks,
                openticks,
                parent_line_offsets,
            )?;

            #[cfg(feature = "attributes")]
            if self.options.extension.inline_code_attributes {
                self.handle_potential_attribute(node, parent_line_offsets)?;
            }

            node
        };
        Ok(result)
    }

    fn handle_backslash(
        &mut self,
        parent_line_offsets: &[usize],
        in_paragraph: bool,
    ) -> Result<Node<'a>, H::Error> {
        let result = 'native: {
            let startpos = self.scanner.pos;
            self.scanner.pos += 1;

            if let Some(math) =
                self.handle_latex_math(startpos, parent_line_offsets, in_paragraph)?
            {
                break 'native math;
            }

            if self.peek_byte().is_some_and(ispunct) {
                self.scanner.pos += 1;
                self.punctuation_escape(startpos)?
            } else if !self.eof() && self.skip_line_end()? {
                let inl = self.make_inline(NodeValue::LineBreak, startpos, self.scanner.pos - 1)?;
                self.line += 1;
                self.column_offset = -(self.scanner.pos as isize);
                let ending_end = self.scanner.pos;
                self.skip_spaces()?;
                if let (Some(node), Some(buffer)) = (inl.data().native_id, self.buffer) {
                    self.ctx.fact(AcceptedFact::Break {
                        node,
                        kind: native::BreakKind::Backslash,
                        marker: Some(RSpan {
                            buffer,
                            start: self.base + startpos,
                            end: self.base + startpos + 1,
                        }),
                        ending: RSpan {
                            buffer,
                            start: self.base + startpos + 1,
                            end: self.base + ending_end,
                        },
                        prefix: RSpan {
                            buffer,
                            start: self.base + ending_end,
                            end: self.base + self.scanner.pos,
                        },
                    })?;
                }
                inl
            } else {
                self.make_inline(
                    NodeValue::Text("\\".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?
            }
        };
        Ok(result)
    }

    fn handle_latex_math(
        &mut self,
        startpos: usize,
        parent_line_offsets: &[usize],
        in_paragraph: bool,
    ) -> Result<Option<Node<'a>>, H::Error> {
        let result = 'native: {
            if !self.options.extension.math_latex {
                break 'native None;
            }

            let (closing_delimiter, display_math) = match native_some!('native, self.peek_byte()) {
                b'(' => (b')', false),
                b'[' => (b']', true),
                _ => break 'native None,
            };
            let standalone = display_math && self.options.parse.latex_display_standalone;
            // A G4 inline gap cannot turn a subsection of a leaf into a paragraph.
            if standalone
                && (!in_paragraph || startpos != 0 || self.input_end() != self.input.len())
            {
                break 'native None;
            }
            let content_start = self.scanner.pos + 1;
            let endpos = native_some!('native, self.scan_to_closing_latex_math(content_start, closing_delimiter)?);

            if endpos == content_start || (standalone && endpos + 2 != self.input.len()) {
                break 'native None;
            }

            self.scanner.pos = endpos + 2;
            if self.ctx.is_boundary_probe() {
                return node_for_probe(self.ctx, self.arena, self.line).map(Some);
            }
            let math = NodeMath {
                dollar_math: true,
                display_math,
                literal: ValueText::pending(),
            };
            let node = self.make_inline(NodeValue::Math(math), startpos, self.scanner.pos - 1)?;
            if let (Some(id), Some(buffer)) = (node.data().native_id, self.buffer) {
                self.ctx.fact(AcceptedFact::Math {
                    node: id,
                    syntax: RSpan {
                        buffer,
                        start: self.base + startpos,
                        end: self.base + self.scanner.pos,
                    },
                    literal_source: RSpan {
                        buffer,
                        start: self.base + content_start,
                        end: self.base + endpos,
                    },
                    delimiter_form: if display_math {
                        native::MathSyntax::LatexDisplay
                    } else {
                        native::MathSyntax::LatexInline
                    },
                })?;
            }
            self.ctx.admit_decoded((endpos - content_start) as u64)?;
            let literal = if self.ctx.direct {
                ValueText::pending()
            } else {
                self.ctx.string(&self.input[content_start..endpos])?.into()
            };
            let literal = self.ctx.finish_node_value(
                literal,
                node.data().native_id,
                NodeValueRole::MathInput,
            )?;
            if let NodeValue::Math(math) = &mut node.data_mut().value {
                math.literal = literal;
            }
            self.adjust_node_newlines(
                node,
                self.scanner.pos - startpos - 2,
                2,
                parent_line_offsets,
            )?;
            Some(node)
        };
        Ok(result)
    }

    fn handle_entity(&mut self) -> Result<Node<'a>, H::Error> {
        let result = {
            self.scanner.pos += 1;

            match entity::unescape_with_host(
                &self.input[self.scanner.pos..self.input_end()],
                self.ctx,
            )? {
                None => self.make_inline(
                    NodeValue::Text("&".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?,
                Some((entity, len)) => {
                    self.scanner.pos += len;
                    self.ctx.admit_decoded(entity.len() as u64)?;
                    let node = self.make_inline_admitted(
                        NodeValue::Text(ValueText::pending()),
                        self.scanner.pos - 1 - len,
                        self.scanner.pos - 1,
                    )?;
                    if let (Some(id), Some(buffer)) = (node.data().native_id, self.buffer) {
                        self.ctx.fact(AcceptedFact::DecodeAtom {
                            node: id,
                            source: RSpan {
                                buffer,
                                start: self.base + self.scanner.pos - 1 - len,
                                end: self.base + self.scanner.pos,
                            },
                            kind: native::DecodeKind::Entity,
                            value: &entity,
                        })?;
                    }
                    let entity = self.ctx.finish_node_value(
                        entity.into(),
                        node.data().native_id,
                        NodeValueRole::Text,
                    )?;
                    node.data_mut().value = NodeValue::Text(entity);
                    node
                }
            }
        };
        Ok(result)
    }

    fn handle_html_code(
        &mut self,
        start: usize,
        opening_end: usize,
        parent_line_offsets: &[usize],
        in_paragraph: bool,
    ) -> Result<Option<Node<'a>>, H::Error> {
        let Some(tag) = accepted_opaque_tag(&self.input.as_bytes()[start..opening_end], self.ctx)?
        else {
            return Ok(None);
        };
        // Tagfilter retains the raw delimiter as authored text. Its free
        // outer scope must not suppress inline semantics around that text.
        // A code body's pair scan still treats raw sections as literal.
        if self.ctx.inline_html == native::InlineHtmlPolicy::Tagfilter
            && matches!(tag, OpaqueTag::Raw { .. })
            && !self.html_scope.opaque()
            && !self.inherited_html_opacity
            && native::gfm_tagfilter_matches(&self.input.as_bytes()[start..opening_end])
        {
            return Ok(None);
        }
        let inside_image = self
            .brackets
            .last()
            .is_some_and(|bracket| bracket.inside_image);
        let shared_pairs = self.html_code_pairs.is_some()
            || inside_image
            || start < self.html_code_blocked_through;
        if let OpaqueTag::Code {
            kind,
            closing: false,
            exact: true,
            ..
        } = tag
            && !self.html_scope.opaque()
            && !self.inherited_html_opacity
            && start >= self.image_fallback_through
            && self.buffer.is_some()
        {
            let limit = self.input_end();
            let end = if shared_pairs {
                if self.html_code_pairs.is_none() {
                    self.ensure_html_specials(opening_end)?;
                    self.html_code_pairs = Some(HtmlCodePairs::build(
                        &self.input[..limit],
                        opening_end,
                        self.html_specials.as_deref().unwrap(),
                        self.ctx,
                    )?);
                }
                self.html_code_pairs
                    .as_ref()
                    .unwrap()
                    .find(opening_end, kind, limit, self.ctx)?
            } else {
                let saved = self.html_code_frames.len();
                let result = self.find_html_code_end(opening_end, limit, tag);
                self.html_code_frames.truncate(saved);
                result?
            };
            if let Some((body_end, syntax_end)) = end {
                if inside_image
                    && let Some(image) = self.image_closed_before(
                        opening_end,
                        syntax_end,
                        parent_line_offsets,
                        in_paragraph,
                    )?
                {
                    self.image_fallback_through = image.close + 1;
                    self.html_scope
                        .advance(tag, &mut self.html_code_frames, self.ctx)?;
                    return Ok(None);
                }
                let buffer = self
                    .buffer
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                self.ctx.admit_decoded((body_end - opening_end) as u64)?;
                let code = NodeCode {
                    num_backticks: 0,
                    literal: if self.ctx.direct {
                        ValueText::pending()
                    } else {
                        self.ctx.string(&self.input[opening_end..body_end])?.into()
                    },
                };
                self.scanner.pos = syntax_end;
                let node =
                    self.make_inline_admitted(NodeValue::Code(code), start, syntax_end - 1)?;
                let id = node
                    .data()
                    .native_id
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                self.ctx.fact(AcceptedFact::HtmlCode {
                    node: id,
                    syntax: RSpan {
                        buffer,
                        start: self.base + start,
                        end: self.base + syntax_end,
                    },
                    body: RSpan {
                        buffer,
                        start: self.base + opening_end,
                        end: self.base + body_end,
                    },
                })?;
                if self.ctx.direct {
                    let literal = self.ctx.finish_node_value(
                        ValueText::pending(),
                        Some(id),
                        NodeValueRole::InlineCode,
                    )?;
                    if let NodeValue::Code(code) = &mut node.data_mut().value {
                        code.literal = literal;
                    }
                }
                self.adjust_node_newlines(node, syntax_end - start, 0, parent_line_offsets)?;
                return Ok(Some(node));
            }
        }
        self.html_scope
            .advance(tag, &mut self.html_code_frames, self.ctx)?;
        Ok(None)
    }

    fn image_closed_before(
        &mut self,
        start: usize,
        stop: usize,
        line_offsets: &[usize],
        in_paragraph: bool,
    ) -> Result<Option<ObservedImage>, H::Error> {
        let closer = if let Some(tokens) = &self.image_tokens {
            ProbeImageTokens::first(&tokens.closers, start, stop, self.ctx)?
        } else {
            self.ctx
                .find(&self.input.as_bytes()[start..stop], |bytes| {
                    bytes.iter().position(|&byte| byte == b']')
                })?
        };
        if closer.is_none() {
            return Ok(None);
        }
        self.probe_images(start, stop, line_offsets, in_paragraph, false, None)
    }

    fn image_inherits_html_scope(
        &mut self,
        line_offsets: &[usize],
        in_paragraph: bool,
    ) -> Result<bool, H::Error> {
        let start = self.scanner.pos;
        let stop = self.input_end();
        if let Some(memo) = &self.image_probe_memo
            && let Some(accepted) = memo.lookup(start, self.flags, self.ctx)?
        {
            return Ok(accepted);
        }
        self.ensure_image_tokens(start)?;
        let tokens = self.image_tokens.as_ref().unwrap();
        if ProbeImageTokens::first(&tokens.closers, start, stop, self.ctx)?.is_none() {
            return Ok(false);
        }
        if self.image_probe_memo.is_none() {
            self.image_probe_memo = Some(ImageProbeMemo::new(&self.input, start, self.ctx)?);
        }
        Ok(self
            .probe_images(start, stop, line_offsets, in_paragraph, true, None)?
            .is_some())
    }

    fn ensure_image_tokens(&mut self, start: usize) -> Result<(), H::Error> {
        if self.image_tokens.is_none() {
            self.image_tokens = Some(ProbeImageTokens::build(&self.input, start, self.ctx)?);
        }
        Ok(())
    }

    fn ensure_html_specials(&mut self, start: usize) -> Result<(), H::Error> {
        if self.html_specials.is_none() {
            self.html_specials = Some(html_code_special_tokens(
                self.input.as_bytes(),
                start,
                self.ctx,
            )?);
        }
        Ok(())
    }

    fn probe_images(
        &mut self,
        start: usize,
        stop: usize,
        line_offsets: &[usize],
        in_paragraph: bool,
        inheritance: bool,
        note_state: Option<(&[InlineNoteBracket], bool)>,
    ) -> Result<Option<ObservedImage>, H::Error> {
        // In the ordinary branch the tentative HTML opener is literal.
        // A token or destination can extend past the candidate's end.
        let index_start = if note_state.is_some() {
            self.gap_lo
        } else {
            start
        };
        self.ensure_image_tokens(index_start)?;
        self.ensure_html_specials(index_start)?;
        let tokens = &self.image_tokens.as_ref().unwrap().structural;
        if self.image_backticks.is_none() {
            self.image_backticks = Some(ProbeBackticks::build(&self.input, index_start, self.ctx)?);
        }
        if self.image_math.is_none()
            && (self.options.extension.math_latex
                || self.options.extension.math_dollars
                || self.options.extension.math_code)
        {
            self.image_math = Some(ProbeMathClosers::build(&self.input, index_start, self.ctx)?);
        }
        let buffer = self
            .buffer
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let mut child = self.ctx.shadow_child();
        child.inline_html = native::InlineHtmlPolicy::Disabled;
        child.capture = false;
        let arena = native::FallibleArena::new();
        let delimiters = native::FallibleArena::new();
        let mut footnotes = FootnoteDefs::new();
        let root = make_inline(
            &mut child,
            &arena,
            if in_paragraph {
                NodeValue::Paragraph
            } else if self.in_table_cell {
                NodeValue::TableCell
            } else {
                NodeValue::Heading(crate::nodes::NodeHeading {
                    level: 1,
                    ..Default::default()
                })
            },
            (self.line, 1, self.line, 1).into(),
        )?;
        let mut probe = Subject::new(
            &mut child,
            Some(buffer),
            self.base,
            &arena,
            self.options,
            self.input.as_ref(),
            self.block_line,
            self.refmap,
            self.tables,
            &mut footnotes,
            &delimiters,
            0,
        );
        probe.scanner.pos = start;
        probe.gap_lo = self.gap_lo;
        probe.gap_hi = self.gap_hi;
        probe.disabled_openers_through = if note_state.is_some() {
            None
        } else {
            self.disabled_openers_through
        };
        probe.line = self.line;
        probe.column_offset = self.column_offset;
        probe.flags = self.flags;
        probe.probe_specials = self.html_specials.as_deref();
        probe.probe_backticks = self.image_backticks.as_deref();
        probe.probe_math = self.image_math.as_deref();
        probe.no_link_openers = note_state.map_or(self.no_link_openers, |(_, disabled)| disabled);
        if inheritance {
            probe.recording_images = self.image_probe_memo.as_mut();
        }
        let mut original = note_state
            .map_or(&[][..], |(brackets, _)| brackets)
            .iter()
            .rev()
            .take_while(|entry| entry.note.is_none())
            .map(|entry| entry.probe)
            .chain(self.brackets.iter().rev().map(|bracket| ImageProbeBracket {
                position: bracket.position,
                image: bracket.image,
                bracket_after: bracket.bracket_after,
                inside_image: bracket.inside_image,
            }));
        let mut accepted = None;
        let mut seeded = false;
        while probe.scanner.pos < stop {
            if probe.brackets.is_empty() {
                if inheritance && seeded {
                    break;
                }
                let Some(bracket) = original.next() else {
                    break;
                };
                seeded = true;
                // Earlier bracket metadata is needed only if the previous
                // active bracket actually popped on this branch.
                probe.ctx.event()?;
                let inl = make_inline(
                    probe.ctx,
                    &arena,
                    NodeValue::Text("".into()),
                    (self.line, 1, self.line, 1).into(),
                )?;
                root.prepend(inl);
                let probe_key = probe.observe_image_open(bracket.image, bracket.position)?;
                probe.ctx.small_push(
                    &mut probe.brackets,
                    Bracket {
                        inl_text: inl,
                        position: bracket.position,
                        image: bracket.image,
                        bracket_after: bracket.bracket_after,
                        probe_key,
                        inside_image: bracket.inside_image,
                        html_scope: HtmlScope::default(),
                        inherited_html_opacity: false,
                    },
                )?;
            }
            let Some(position) =
                ProbeImageTokens::first(tokens, probe.scanner.pos, stop, probe.ctx)?
            else {
                break;
            };
            probe.scanner.pos = position;
            // The resolver observes original byte offsets, never scratch
            // coordinates. Skipped text and opaque values can contain lines.
            probe.line = self.block_line;
            probe.column_offset = 0;
            if !probe.parse_inline(root, self.block_line, line_offsets)? {
                break;
            }
            if let Some(image) = probe.observed_image.take()
                && image.open <= start
                && image.close < stop
            {
                accepted = Some(image);
                break;
            }
        }
        probe.clear_brackets()?;
        probe.release_owned()?;
        child.flush_scan()?;
        Ok(accepted)
    }

    fn find_html_code_end(
        &mut self,
        mut at: usize,
        limit: usize,
        opening: OpaqueTag,
    ) -> Result<Option<(usize, usize)>, H::Error> {
        let bytes = self.input.as_bytes();
        let mut flags = HtmlSkipFlags::default();
        let mut scope = HtmlScope::default();
        scope.advance(opening, &mut self.html_code_frames, self.ctx)?;
        let outer = scope.code;
        while at < limit {
            self.ctx.scan_step()?;
            if bytes[at] != b'<' {
                at += 1;
                continue;
            }
            // Only a matching raw-text closer is grammar inside raw text.
            if let Some(raw) = scope.raw {
                let candidate = bytes[at..limit].strip_prefix(b"</");
                if raw == b"plaintext"
                    || !candidate.is_some_and(|tail| {
                        tail.get(..raw.len())
                            .is_some_and(|name| name.eq_ignore_ascii_case(raw))
                            && tail
                                .get(raw.len())
                                .is_some_and(|byte| *byte == b'>' || tag_space(*byte))
                    })
                {
                    at += 1;
                    continue;
                }
            }
            let Some(width) =
                html_code_inline_len(&self.input[at + 1..limit], &mut flags, self.ctx)?
            else {
                at += 1;
                continue;
            };
            let end = at + 1 + width;
            if let Some(tag) = accepted_opaque_tag(&bytes[at..end], self.ctx)? {
                let outer_close = scope.raw.is_none()
                    && scope.code == outer
                    && matches!(tag, OpaqueTag::Code { closing: true, .. });
                let ordered = scope.advance(tag, &mut self.html_code_frames, self.ctx)?;
                if !ordered
                    || (outer_close
                        && !scope.opaque()
                        && !matches!(tag, OpaqueTag::Code { exact: true, .. }))
                {
                    self.html_code_blocked_through = end;
                    return Ok(None);
                }
                if !scope.opaque() {
                    return Ok(Some((at, end)));
                }
            }
            at = end;
        }
        // Only the scanned prefix is unavailable. The rest of the inline
        // stream can still contain valid pairs after an image or strict gap.
        self.html_code_blocked_through = limit;
        Ok(None)
    }

    fn handle_pointy_brace(
        &mut self,
        parent_line_offsets: &[usize],
        in_paragraph: bool,
    ) -> Result<Node<'a>, H::Error> {
        let result = 'native: {
            self.scanner.pos += 1;

            if self.options.parse.smart && self.peek_byte() == Some(b'<') {
                self.scanner.pos += 1;
                break 'native self.make_inline(
                    NodeValue::Text("\u{ab}".into()),
                    self.scanner.pos - 2,
                    self.scanner.pos - 1,
                )?;
            }

            #[cfg(feature = "phoenix_heex")]
            if self.options.extension.phoenix_heex {
                let scanners = [
                    scanners::phoenix_directive,
                    strings::phoenix_inline_tag,
                    scanners::phoenix_closing_tag,
                ];

                for scanner in scanners {
                    if let Some(inl) = self.make_heex_inline(scanner, parent_line_offsets)? {
                        break 'native inl;
                    }
                }
            }

            if let Some(matchlen) = scanners::autolink_uri_with_host(
                &self.input[self.scanner.pos..self.input_end()],
                self.ctx,
            )? {
                self.scanner.pos += matchlen;
                let url = self
                    .ctx
                    .string(&self.input[self.scanner.pos - matchlen..self.scanner.pos - 1])?;
                let inl = self.make_autolink(
                    &url,
                    AutolinkType::Uri,
                    self.scanner.pos - 1 - matchlen,
                    self.scanner.pos - 1,
                )?;
                self.ctx.discard_string(url)?;
                break 'native inl;
            }

            if let Some(matchlen) = scanners::autolink_email_with_host(
                &self.input[self.scanner.pos..self.input_end()],
                self.ctx,
            )? {
                self.scanner.pos += matchlen;
                let url = self
                    .ctx
                    .string(&self.input[self.scanner.pos - matchlen..self.scanner.pos - 1])?;
                let inl = self.make_autolink(
                    &url,
                    AutolinkType::Email,
                    self.scanner.pos - 1 - matchlen,
                    self.scanner.pos - 1,
                )?;
                self.ctx.discard_string(url)?;
                break 'native inl;
            }

            let rest = &self.input[self.scanner.pos..self.gap_hi];
            let matchlen = if let Some(index) = self.probe_specials
                && rest
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| matches!(byte, b'!' | b'?'))
            {
                probe_special_html_len(
                    rest,
                    self.scanner.pos - 1,
                    &mut self.flags,
                    index,
                    self.ctx,
                )?
            } else {
                inline_html_len(rest, &mut self.flags, self.ctx)?
            };

            if let Some(matchlen) = matchlen {
                let start = self.scanner.pos - 1;
                let tag_end = self.scanner.pos + matchlen;
                if self.ctx.is_boundary_probe() {
                    self.scanner.pos = tag_end;
                    return node_for_probe(self.ctx, self.arena, self.line);
                }
                if self.ctx.inline_html.enabled()
                    && let Some(inl) =
                        self.handle_html_code(start, tag_end, parent_line_offsets, in_paragraph)?
                {
                    break 'native inl;
                }
                let contents = &self.input[start..tag_end];
                self.scanner.pos += matchlen;
                let inl = if self.ctx.direct {
                    self.ctx.admit_decoded(contents.len() as u64)?;
                    self.make_inline_admitted(
                        NodeValue::HtmlInline(ValueText::pending()),
                        self.scanner.pos - matchlen - 1,
                        self.scanner.pos - 1,
                    )?
                } else {
                    let contents = self.ctx.string(contents)?;
                    self.make_inline(
                        NodeValue::HtmlInline(contents.into()),
                        self.scanner.pos - matchlen - 1,
                        self.scanner.pos - 1,
                    )?
                };
                if let (Some(node), Some(buffer)) = (inl.data().native_id, self.buffer) {
                    self.ctx.fact(AcceptedFact::RawHtml {
                        node,
                        block: false,
                        span: RSpan {
                            buffer,
                            start: self.base + self.scanner.pos - matchlen - 1,
                            end: self.base + self.scanner.pos,
                        },
                    })?;
                }
                if self.ctx.direct {
                    let value = self.ctx.finish_node_value(
                        ValueText::pending(),
                        inl.data().native_id,
                        NodeValueRole::Html,
                    )?;
                    inl.data_mut().value = NodeValue::HtmlInline(value);
                }
                self.adjust_node_newlines(inl, matchlen, 1, parent_line_offsets)?;
                break 'native inl;
            }

            self.make_inline(
                NodeValue::Text("<".into()),
                self.scanner.pos - 1,
                self.scanner.pos - 1,
            )?
        };
        Ok(result)
    }

    fn handle_autolink_with(
        &mut self,
        node: Node<'a>,
        f: fn(&mut Subject<'a, '_, '_, '_, '_, '_, '_, H>) -> AutolinkScanResult<'a, H::Error>,
    ) -> Result<Option<Node<'a>>, H::Error> {
        let result = 'native: {
            if !self.options.parse.relaxed_autolinks && !self.brackets.is_empty() {
                break 'native None;
            }
            let startpos = self.scanner.pos;
            let (post, need_reverse, skip) = native_some!('native, f(self)?);

            self.scanner.pos += skip - need_reverse;

            // We need to "rewind" by `need_reverse` chars, which should be in one
            // or more Text nodes beforehand. Typically the chars will *all* be in
            // a single Text node, containing whatever text came before the ":" that
            // triggered this method, eg. "See our website at http" ("://blah.com").
            //
            // relaxed_autolinks allows some slightly pathological cases. First,
            // "://…" is a possible parse, meaning `reverse == 0`. There may also be
            // a scheme including the letter "w", which will split Text inlines due
            // to them being their own trigger (for handle_autolink_w), meaning
            // "wa://…" will need to traverse two Texts to complete the rewind.
            let mut reverse = need_reverse;
            while reverse > 0 {
                let mut last_child = node.last_child().unwrap().data_mut();
                let NodeValue::Text(ref mut prev) = last_child.value else {
                    panic!("expected text node before autolink colon");
                };
                let prev = prev
                    .owned_cow_mut()
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                let prev_len = prev.len();
                if reverse < prev.len() {
                    self.ctx.cow_mut(prev)?.truncate(prev_len - reverse);
                    last_child.sourcepos.end.column -= reverse;
                    reverse = 0;
                } else {
                    reverse -= prev_len;
                    node.last_child().unwrap().detach();
                }
            }

            {
                let sp = &mut post.data_mut().sourcepos;
                // See [`make_inline`].
                sp.start = (
                    self.line,
                    (startpos as isize - need_reverse as isize
                        + 1
                        + self.column_offset
                        + self.line_offset as isize) as usize,
                )
                    .into();
                sp.end = (
                    self.line,
                    (self.scanner.pos as isize + self.column_offset + self.line_offset as isize)
                        as usize,
                )
                    .into();

                // Inner text node gets the same sp, since there are no surrounding
                // characters for autolinks of these kind.
                post.first_child().unwrap().data_mut().sourcepos = *sp;
            }

            Some(post)
        };
        Ok(result)
    }

    #[cfg(feature = "shortcodes")]
    fn handle_shortcodes_colon(&mut self) -> Result<Option<Node<'a>>, H::Error> {
        if self.ctx.capture || self.ctx.is_boundary_probe() {
            return self.handle_mapped_shortcode();
        }
        let result = 'native: {
            let matchlen = native_some!('native, scanners::shortcode(&self.input[self.scanner.pos + 1..self.input_end()]));

            let shortcode = &self.input[self.scanner.pos + 1..self.scanner.pos + 1 + matchlen - 1];

            let nsc = native_some!('native, NodeShortCode::resolve(shortcode));
            self.scanner.pos += 1 + matchlen;

            Some(self.make_inline(
                NodeValue::ShortCode(Box::new(nsc)),
                self.scanner.pos - 1 - matchlen,
                self.scanner.pos - 1,
            )?)
        };
        Ok(result)
    }

    #[cfg(feature = "shortcodes")]
    fn handle_mapped_shortcode(&mut self) -> Result<Option<Node<'a>>, H::Error> {
        if self.html_scope.opaque() || self.inherited_html_opacity {
            return Ok(None);
        }
        let open = self.scanner.pos;
        let start = open + 1;
        // Only a complete ASCII alias is eligible. Unknown or truncated
        // candidates leave the cursor at its original colon.
        for at in start..self.input_end().min(start + 65) {
            self.ctx.scan_step()?;
            let byte = self.input.as_bytes()[at];
            if byte == b':' && at > start {
                self.ctx.scan(at - start)?;
                let Some(emoji) = emojis::get_by_shortcode(&self.input[start..at]) else {
                    return Ok(None);
                };
                let value = emoji.as_str();
                self.scanner.pos = at + 1;
                self.ctx.admit_decoded(value.len() as u64)?;
                let node =
                    self.make_inline_admitted(NodeValue::Text(ValueText::pending()), open, at)?;
                if let (Some(id), Some(buffer)) = (node.data().native_id, self.buffer) {
                    self.ctx.fact(AcceptedFact::DecodeAtom {
                        node: id,
                        source: RSpan {
                            buffer,
                            start: self.base + open,
                            end: self.base + self.scanner.pos,
                        },
                        kind: native::DecodeKind::EmojiShortcode,
                        value,
                    })?;
                }
                let text = self.ctx.finish_node_value(
                    value.into(),
                    node.data().native_id,
                    NodeValueRole::Text,
                )?;
                node.data_mut().value = NodeValue::Text(text);
                return Ok(Some(node));
            }
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-')) {
                break;
            }
        }
        Ok(None)
    }

    fn handle_delim(&mut self, b: u8) -> Result<Node<'a>, H::Error> {
        let result = {
            let (numdelims, can_open, can_close) = self.scan_delims(b)?;

            let inl = if self.ctx.direct {
                self.make_inline_text_owned(self.scanner.pos - numdelims, self.scanner.pos)?
            } else {
                let contents: Cow<'static, str> = if b == b'\'' && self.options.parse.smart {
                    "’".into()
                } else if b == b'"' && self.options.parse.smart {
                    if can_close {
                        "”".into()
                    } else {
                        "“".into()
                    }
                } else {
                    self.ctx
                        .string(&self.input[self.scanner.pos - numdelims..self.scanner.pos])?
                        .into()
                };
                self.make_inline(
                    NodeValue::Text(contents.into()),
                    self.scanner.pos - numdelims,
                    self.scanner.pos - 1,
                )?
            };

            let valid_delimiter_count = if b == b'~' && self.options.extension.subscript {
                numdelims == 1 || (numdelims == 2 && self.options.extension.strikethrough)
            } else if b == b'~' && self.options.extension.strikethrough {
                numdelims <= 2
            } else if (b == b'=' && self.options.extension.highlight)
                || (b == b'+' && self.options.extension.insert)
            {
                numdelims == 2
            } else {
                true
            };

            if (can_open || can_close)
                && (!(b == b'\'' || b == b'"') || self.options.parse.smart)
                && valid_delimiter_count
            {
                self.push_delimiter(b, can_open, can_close, inl)?;
            }

            inl
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn make_heex_inline<F>(
        &mut self,
        scanner: F,
        parent_line_offsets: &[usize],
    ) -> Result<Option<Node<'a>>, H::Error>
    where
        F: Fn(&str) -> Option<usize>,
    {
        let result = 'native: {
            let matchlen = native_some!('native, scanner(&self.input[self.scanner.pos - 1..]));
            let start_pos = self.scanner.pos - 1;
            self.scanner.pos = self.scanner.pos - 1 + matchlen;
            let inl = self.make_inline(
                NodeValue::HeexInline(self.input[start_pos..self.scanner.pos].to_string()),
                start_pos,
                self.scanner.pos - 1,
            )?;
            self.adjust_node_newlines(inl, matchlen, 0, parent_line_offsets)?;
            Some(inl)
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn handle_heex_inline_expression(
        &mut self,
        parent_line_offsets: &[usize],
    ) -> Result<Option<Node<'a>>, H::Error> {
        let result = 'native: {
            let matchlen = native_some!('native, strings::phoenix_inline_expression(&self.input[self.scanner.pos..self.input_end()]));
            let start_pos = self.scanner.pos;
            self.scanner.pos += matchlen;
            let inl = self.make_inline(
                NodeValue::HeexInline(self.input[start_pos..self.scanner.pos].to_string()),
                start_pos,
                self.scanner.pos - 1,
            )?;
            self.adjust_node_newlines(inl, matchlen, 0, parent_line_offsets)?;
            Some(inl)
        };
        Ok(result)
    }

    fn handle_hyphen(&mut self) -> Result<Node<'a>, H::Error> {
        let result = 'native: {
            let start = self.scanner.pos;
            self.scanner.pos += 1;

            if !self.options.parse.smart || (self.peek_byte() != Some(b'-')) {
                break 'native self.make_inline(
                    NodeValue::Text("-".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?;
            }

            while self.options.parse.smart && (self.peek_byte() == Some(b'-')) {
                self.scanner.pos += 1;
            }

            let numhyphens = (self.scanner.pos - start) as i32;

            let (ens, ems) = if numhyphens % 3 == 0 {
                (0, numhyphens / 3)
            } else if numhyphens % 2 == 0 {
                (numhyphens / 2, 0)
            } else if numhyphens % 3 == 2 {
                (1, (numhyphens - 2) / 3)
            } else {
                (2, (numhyphens - 4) / 3)
            };

            let ens = if ens > 0 { ens as usize } else { 0 };
            let ems = if ems > 0 { ems as usize } else { 0 };

            let mut buf = String::new();
            for _ in 0..ems {
                self.ctx.append(&mut buf, "—")?;
            }
            for _ in 0..ens {
                self.ctx.append(&mut buf, "–")?;
            }
            self.make_inline(NodeValue::Text(buf.into()), start, self.scanner.pos - 1)?
        };
        Ok(result)
    }

    fn handle_period(&mut self) -> Result<Node<'a>, H::Error> {
        let result = {
            self.scanner.pos += 1;
            if self.options.parse.smart && (self.peek_byte() == Some(b'.')) {
                self.scanner.pos += 1;
                if self.peek_byte() == Some(b'.') {
                    self.scanner.pos += 1;
                    self.make_inline(
                        NodeValue::Text("…".into()),
                        self.scanner.pos - 3,
                        self.scanner.pos - 1,
                    )?
                } else {
                    self.make_inline(
                        NodeValue::Text("..".into()),
                        self.scanner.pos - 2,
                        self.scanner.pos - 1,
                    )?
                }
            } else {
                self.make_inline(
                    NodeValue::Text(".".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?
            }
        };
        Ok(result)
    }

    fn handle_guillemet_close(&mut self) -> Result<Node<'a>, H::Error> {
        let result = {
            self.scanner.pos += 1;
            if self.peek_byte() == Some(b'>') {
                self.scanner.pos += 1;
                self.make_inline(
                    NodeValue::Text("\u{bb}".into()),
                    self.scanner.pos - 2,
                    self.scanner.pos - 1,
                )?
            } else {
                self.make_inline(
                    NodeValue::Text(">".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?
            }
        };
        Ok(result)
    }

    fn punctuation_escape(&mut self, start: usize) -> Result<Node<'a>, H::Error> {
        let escaped = self.ctx.string(&self.input[start + 1..start + 2])?;
        self.ctx.admit_decoded(escaped.len() as u64)?;
        let text =
            self.make_inline_admitted(NodeValue::Text(ValueText::pending()), start + 1, start + 1)?;
        let node = self.make_inline(NodeValue::Escaped, start, start + 1)?;
        node.append(text);
        if let (Some(node), Some(buffer)) = (text.data().native_id, self.buffer) {
            self.ctx.fact(AcceptedFact::DecodeAtom {
                node,
                source: RSpan {
                    buffer,
                    start: self.base + start,
                    end: self.base + start + 2,
                },
                kind: native::DecodeKind::Escape,
                value: &escaped,
            })?;
        }
        let escaped = self.ctx.finish_node_value(
            escaped.into(),
            text.data().native_id,
            NodeValueRole::Text,
        )?;
        text.data_mut().value = NodeValue::Text(escaped);
        Ok(node)
    }

    fn handle_native_wiki_link(
        &mut self,
        image: bool,
        table: bool,
    ) -> Result<Option<Node<'a>>, H::Error> {
        let second_open = self.scanner.pos;
        let Some((target, mut label)) = self.native_wiki_parts(table)? else {
            self.scanner.pos = second_open;
            return Ok(None);
        };
        if self.wiki_has_markdown_suffix()? {
            self.scanner.pos = second_open;
            return Ok(None);
        }
        let destination = &self.input[target.clone()];
        let form = if !image {
            native::WikiForm::Link
        } else if self.options.extension.native_wiki_links && native::wiki_image_target(destination)
        {
            let (width, height) = if target == label {
                (0, 0)
            } else if let Some(size) = native::wiki_image_dimensions(&self.input[label.clone()]) {
                label = target.clone();
                size
            } else {
                (0, 0)
            };
            native::WikiForm::Image { width, height }
        } else if self.options.extension.native_note_embeds
            && target == label
            && native::wiki_note_target(destination)
        {
            native::WikiForm::NoteEmbed
        } else {
            self.scanner.pos = second_open;
            return Ok(None);
        };
        if self.ctx.is_boundary_probe() {
            return node_for_probe(self.ctx, self.arena, self.line).map(Some);
        }
        let buffer = self
            .buffer
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let start = second_open - if image { 2 } else { 1 };
        let end = self.scanner.pos;
        let base = self.base;
        let source = |range: std::ops::Range<usize>| RSpan {
            buffer,
            start: base + range.start,
            end: base + range.end,
        };
        let payload = if self.ctx.is_shadow() {
            None
        } else {
            Some(
                self.ctx
                    .payload(native::PayloadKind::Destination, source(target.clone()))?,
            )
        };
        let destination = if let Some(payload) = payload {
            let value = self.ctx.decoded_string(&self.input[target])?;
            self.ctx.fact(AcceptedFact::PayloadEnd {
                payload,
                value: &value,
            })?;
            value
        } else {
            String::new()
        };
        let url = self.ctx.finish_payload_value(
            destination,
            payload,
            native::PayloadKind::Destination,
        )?;
        let link = self.ctx.boxed(NodeLink {
            url,
            title: self.ctx.empty_value(),
        })?;
        let value = match form {
            native::WikiForm::Link => NodeValue::Link(link),
            native::WikiForm::Image { .. } => NodeValue::Image(link),
            native::WikiForm::NoteEmbed => NodeValue::NoteEmbed(link),
        };
        let link = self.make_inline(value, start, end - 1)?;
        let node = link
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        if form != native::WikiForm::NoteEmbed {
            let mut segment = label.start;
            let mut at = segment;
            while at < label.end {
                self.ctx.scan_step()?;
                if self.input.as_bytes()[at] == b'\\'
                    && at + 1 < label.end
                    && ispunct(self.input.as_bytes()[at + 1])
                {
                    if segment < at {
                        link.append(self.make_inline_text_owned(segment, at)?);
                    }
                    link.append(self.punctuation_escape(at)?);
                    at += 2;
                    segment = at;
                } else {
                    at += 1;
                }
            }
            if segment < label.end {
                link.append(self.make_inline_text_owned(segment, label.end)?);
            }
        }
        self.ctx.fact(AcceptedFact::Link {
            node,
            syntax: source(start..end),
            label: source(label),
            image,
            wiki: Some(form),
            destination: payload,
            title: None,
            definition: None,
        })?;
        Ok(Some(link))
    }

    fn wiki_has_markdown_suffix(&mut self) -> Result<bool, H::Error> {
        let end = self.input_end();
        let suffix = self.scanner.pos;
        if self.peek_byte() == Some(b'(') {
            return Ok(scan_inline_link_suffix(&self.input[..end], suffix, self.ctx)?.is_some());
        }
        if self.peek_byte() != Some(b'[')
            || self.peek_byte_n(1) == Some(b'[')
            || self.refmap.map.entries == 0
        {
            return Ok(false);
        }
        let mut scanner = Scanner { pos: suffix };
        let Some(label) = scanner.link_label_with_host(&self.input[..end], self.ctx)? else {
            return Ok(false);
        };
        if label.is_empty() {
            return Ok(false);
        }
        let normalized = strings::normalize_label_with_host(label, Case::Fold, self.ctx)?;
        let found = self.refmap.map.get(&normalized, self.ctx)?.is_some();
        self.ctx.discard_string(normalized)?;
        Ok(found)
    }

    fn native_wiki_parts(
        &mut self,
        table: bool,
    ) -> Result<Option<(std::ops::Range<usize>, std::ops::Range<usize>)>, H::Error> {
        let component = |input: &str, start: usize, end: usize| {
            let raw = &input[start..end];
            let left = raw.trim_start_matches([' ', '\t']);
            let text = left.trim_end_matches([' ', '\t']);
            let start = start + raw.len() - left.len();
            start..start + text.len()
        };
        let start = self.scanner.pos + 1;
        if !self.wikilink_component(table)? {
            return Ok(None);
        }
        let target = component(&self.input, start, self.scanner.pos);
        if table && self.peek_byte() == Some(b'\\') && self.peek_byte_n(1) == Some(b'|') {
            self.ctx.scan_step()?;
            self.scanner.pos += 1;
        }
        let label = if self.peek_byte() == Some(b'|') {
            let start = self.scanner.pos + 1;
            if !self.wikilink_component(false)? {
                return Ok(None);
            }
            component(&self.input, start, self.scanner.pos)
        } else {
            target.clone()
        };
        if self.peek_byte() != Some(b']')
            || self.peek_byte_n(1) != Some(b']')
            || target.is_empty()
            || label.is_empty()
        {
            return Ok(None);
        }
        self.ctx.scan(target.len() + label.len())?;
        let valid = |range: std::ops::Range<usize>| !self.input[range].contains(['\r', '\n', '\0']);
        if !valid(target.clone()) || !valid(label.clone()) {
            return Ok(None);
        }
        self.scanner.pos += 2;
        Ok(Some((target, label)))
    }

    // Handles wikilink syntax
    //   [[link text|url]]
    //   [[url|link text]]
    fn handle_wikilink(&mut self) -> Result<Option<Node<'a>>, H::Error> {
        let result = 'native: {
            let startpos = self.scanner.pos;
            let component = native_some!('native, self.wikilink_url_link_label()?);
            let url_clean = strings::clean_url(&component.url);
            let (link_label, link_label_start_column, _link_label_end_column): (
                Cow<'_, str>,
                _,
                _,
            ) = match &component.link_label {
                Some((label, sc, ec)) => (entity::unescape_html(label), *sc, *ec),
                None => (
                    entity::unescape_html(&component.url),
                    startpos + 1,
                    self.scanner.pos - 3,
                ),
            };

            let nl = NodeWikiLink {
                url: url_clean.into(),
            };
            let inl =
                self.make_inline(NodeValue::WikiLink(nl), startpos - 1, self.scanner.pos - 1)?;

            self.label_backslash_escapes(inl, &link_label, link_label_start_column)?;

            Some(inl)
        };
        Ok(result)
    }

    fn wikilink_url_link_label(&mut self) -> Result<Option<WikilinkComponents>, H::Error> {
        let result = 'native: {
            let left_startpos = self.scanner.pos;

            if self.peek_byte() != Some(b'[') {
                break 'native None;
            }

            let found_left = self.wikilink_component(false)?;

            if !found_left {
                self.scanner.pos = left_startpos;
                break 'native None;
            }

            let left =
                strings::trim_slice(&self.input[left_startpos + 1..self.scanner.pos]).to_string();

            if self.peek_byte() == Some(b']') && self.peek_byte_n(1) == Some(b']') {
                self.scanner.pos += 2;
                break 'native Some(WikilinkComponents {
                    url: left,
                    link_label: None,
                });
            } else if self.peek_byte() != Some(b'|') {
                self.scanner.pos = left_startpos;
                break 'native None;
            }

            let right_startpos = self.scanner.pos;
            let found_right = self.wikilink_component(false)?;

            if !found_right {
                self.scanner.pos = left_startpos;
                break 'native None;
            }

            let right = strings::trim_slice(&self.input[right_startpos + 1..self.scanner.pos]);

            if self.peek_byte() == Some(b']') && self.peek_byte_n(1) == Some(b']') {
                self.scanner.pos += 2;

                match self.options.extension.wikilinks() {
                    Some(WikiLinksMode::UrlFirst) => Some(WikilinkComponents {
                        url: left,
                        link_label: Some((right.into(), right_startpos + 1, self.scanner.pos - 3)),
                    }),
                    Some(WikiLinksMode::TitleFirst) => Some(WikilinkComponents {
                        url: right.into(),
                        link_label: Some((left, left_startpos + 1, right_startpos - 1)),
                    }),
                    None => unreachable!(),
                }
            } else {
                self.scanner.pos = left_startpos;
                None
            }
        };
        Ok(result)
    }

    // Locates the edge of a wikilink component (link label or url), and sets the
    // self.scanner.pos to it's end if it's found.
    fn wikilink_component(&mut self, table_delimiter: bool) -> Result<bool, H::Error> {
        let result = 'native: {
            let startpos = self.scanner.pos;

            if self.peek_byte() != Some(b'[') && self.peek_byte() != Some(b'|') {
                break 'native false;
            }

            self.scanner.pos += 1;

            let mut length = 0;
            while let Some(b) = self.peek_byte() {
                self.ctx.scan_step()?;
                if matches!(b, b'[' | b']' | b'|')
                    || table_delimiter && b == b'\\' && self.peek_byte_n(1) == Some(b'|')
                {
                    break;
                }

                if b == b'\\' {
                    self.scanner.pos += 1;
                    length += 1;
                    if self.peek_byte().is_some_and(ispunct) {
                        self.ctx.scan_step()?;
                        self.scanner.pos += 1;
                        length += 1;
                    }
                } else {
                    self.scanner.pos += 1;
                    length += 1;
                }
                if length > MAX_LINK_LABEL_LENGTH {
                    self.scanner.pos = startpos;
                    break 'native false;
                }
            }

            true
        };
        Ok(result)
    }

    // Given a label, handles backslash escaped characters. Appends the resulting
    // nodes to the container
    fn label_backslash_escapes(
        &mut self,
        container: Node<'a>,
        label: &str,
        start_column: usize,
    ) -> Result<(), H::Error> {
        {
            let mut startpos = 0;
            let mut offset = 0;
            let bytes = label.as_bytes();
            let len = label.len();

            while offset < len {
                let b = bytes[offset];

                if b == b'\\' && (offset + 1) < len && ispunct(bytes[offset + 1]) {
                    let preceding = self.ctx.string(&label[startpos..offset])?;
                    let preceding_text = self.make_inline(
                        NodeValue::Text(preceding.into()),
                        start_column + startpos,
                        start_column + offset - 1,
                    )?;

                    container.append(preceding_text);

                    let escaped = self.ctx.string(&label[offset + 1..offset + 2])?;
                    let inline_text = self.make_inline(
                        NodeValue::Text(escaped.into()),
                        start_column + offset,
                        start_column + offset + 1,
                    )?;

                    if self.options.render.escaped_char_spans {
                        let span = self.make_inline(
                            NodeValue::Escaped,
                            start_column + offset,
                            start_column + offset + 1,
                        )?;

                        span.append(inline_text);
                        container.append(span);
                    } else {
                        container.append(inline_text);
                    }

                    offset += 2;
                    startpos = offset;
                } else {
                    offset += 1;
                }
            }

            if startpos != offset {
                let trailing = self.ctx.string(&label[startpos..offset])?;
                container.append(self.make_inline(
                    NodeValue::Text(trailing.into()),
                    start_column + startpos,
                    start_column + offset - 1,
                )?);
            }
        };
        Ok(())
    }

    fn mapped_inline_note_end(
        &mut self,
        opener: usize,
        line_offsets: &[usize],
    ) -> Result<Option<usize>, H::Error> {
        if self.inline_note_closers.is_none() {
            // Build one index for this committed gap. Failed openers cannot
            // repeatedly scan the same suffix, including in a boundary probe.
            let limit = self.gap_hi;
            let mut stack: Vec<InlineNoteBracket> = Vec::new();
            let mut closers = Vec::new();
            let mut backticks = None;
            let mut math = None;
            let mut html_flags = HtmlSkipFlags::default();
            let mut html_pairs: Option<HtmlCodePairs> = None;
            let mut image_depth = 0;
            let mut no_link_openers = true;
            let mut escaped_punctuation = None;
            let mut at = opener + 1;
            while at < limit {
                self.ctx.scan_step()?;
                let input = &self.input[..limit];
                let bytes = input.as_bytes();
                match bytes[at] {
                    b'\\' => {
                        let next = bytes.get(at + 1).copied();
                        let kind = if self.options.extension.math_latex {
                            match next {
                                Some(b'(') => Some(ProbeMathCloser::LatexParen),
                                Some(b'[') if !self.options.parse.latex_display_standalone => {
                                    Some(ProbeMathCloser::LatexBracket)
                                }
                                _ => None,
                            }
                        } else {
                            None
                        };
                        if let Some(kind) = kind {
                            let start = at + 2;
                            let index =
                                ProbeMathClosers::indexed(&mut math, input, start, self.ctx)?;
                            if let Some(close) = index
                                .first(kind, start, bytes.len(), self.ctx)?
                                .filter(|&close| close > start)
                            {
                                at = close + 2;
                                continue;
                            }
                        }
                        let escaped = next.is_some_and(ispunct);
                        if escaped {
                            escaped_punctuation = Some(at + 1);
                        }
                        at += 1 + usize::from(escaped);
                    }
                    b'$' if self.options.extension.math_dollars
                        || self.options.extension.math_code =>
                    {
                        let open = at;
                        at += 1;
                        while bytes.get(at) == Some(&b'$') {
                            self.ctx.scan_step()?;
                            at += 1;
                        }
                        let width = at - open;
                        if width == 1
                            && self.options.extension.math_code
                            && bytes.get(at) == Some(&b'`')
                        {
                            let index =
                                ProbeMathClosers::indexed(&mut math, input, at + 1, self.ctx)?;
                            if let Some(end) = index
                                .first(ProbeMathCloser::CodeDollar, at + 1, bytes.len(), self.ctx)?
                                .map(|close| close + 1)
                                .filter(|&end| end - open > 4)
                            {
                                at = end;
                            }
                        } else if self.options.extension.math_dollars
                            && width <= MAX_MATH_DOLLARS
                            && (width != 1 || !bytes.get(at).copied().is_some_and(isspace))
                        {
                            let index = ProbeMathClosers::indexed(&mut math, input, at, self.ctx)?;
                            if let Some(end) = index
                                .dollar_end(bytes, at, width, self.ctx)?
                                .filter(|&end| end - open > 2 * width)
                            {
                                at = end;
                            }
                        }
                    }
                    b'<' => {
                        let rest = &input[at + 1..];
                        let width = if let Some(width) =
                            scanners::autolink_uri_with_host(rest, self.ctx)?
                        {
                            Some(width)
                        } else if let Some(width) =
                            scanners::autolink_email_with_host(rest, self.ctx)?
                        {
                            Some(width)
                        } else {
                            inline_html_len(rest, &mut html_flags, self.ctx)?
                        };
                        let tag_end = at + 1 + width.unwrap_or(0);
                        if self.ctx.inline_html.enabled()
                            && width.is_some()
                            && let Some(OpaqueTag::Code {
                                kind,
                                closing: false,
                                exact: true,
                                ..
                            }) = accepted_opaque_tag(&bytes[at..tag_end], self.ctx)?
                        {
                            if html_pairs.is_none() {
                                let specials = html_code_special_tokens(bytes, tag_end, self.ctx)?;
                                html_pairs = Some(HtmlCodePairs::build(
                                    input, tag_end, &specials, self.ctx,
                                )?);
                                self.ctx.discard_vec(specials)?;
                            }
                            if let Some((_, end)) = html_pairs.as_ref().unwrap().find(
                                tag_end,
                                kind,
                                bytes.len(),
                                self.ctx,
                            )? && (image_depth == 0
                                || self
                                    .probe_images(
                                        tag_end,
                                        end,
                                        line_offsets,
                                        true,
                                        false,
                                        Some((&stack, no_link_openers)),
                                    )?
                                    .is_none())
                            {
                                at = end;
                                continue;
                            }
                        }
                        at = tag_end;
                    }
                    b'`' => {
                        let start = at;
                        at += 1;
                        while bytes.get(at) == Some(&b'`') {
                            self.ctx.scan_step()?;
                            at += 1;
                        }
                        let width = at - start;
                        if width <= MAXBACKTICKS {
                            if backticks.is_none() {
                                backticks = Some(ProbeBackticks::build(input, at, self.ctx)?);
                            }
                            if let Some(end) = backticks.as_ref().unwrap().closing(
                                bytes,
                                at,
                                bytes.len(),
                                width,
                                self.ctx,
                            )? {
                                at = end;
                            }
                        }
                    }
                    b'[' => {
                        let unescaped = escaped_punctuation != Some(at - 1);
                        let note = if unescaped
                            && bytes[at - 1] == b'^'
                            && stack.last().is_none_or(|entry| entry.note.is_some())
                        {
                            let index = closers.len();
                            self.ctx.push(&mut closers, (at - 1, 0))?;
                            Some(index)
                        } else {
                            None
                        };
                        let image = unescaped && bytes[at - 1] == b'!';
                        if !image && note.is_none() {
                            no_link_openers = false;
                        }
                        image_depth += usize::from(image);
                        if let Some(last) = stack.last_mut() {
                            last.probe.bracket_after = true;
                        }
                        self.ctx.push(
                            &mut stack,
                            InlineNoteBracket {
                                note,
                                probe: ImageProbeBracket {
                                    position: at + 1,
                                    image,
                                    bracket_after: false,
                                    inside_image: image_depth != 0,
                                },
                            },
                        )?;
                        at += 1;
                    }
                    b']' => {
                        let closing = at;
                        at += 1;
                        if let Some(InlineNoteBracket {
                            note,
                            probe: ImageProbeBracket { image, .. },
                        }) = stack.pop()
                        {
                            image_depth -= usize::from(image);
                            if let Some(index) = note {
                                closers[index].1 = closing;
                            } else if (image || !no_link_openers)
                                && let Some(suffix) = scan_inline_link_suffix(input, at, self.ctx)?
                            {
                                at = suffix.endall + 1;
                                if !image {
                                    no_link_openers = true;
                                }
                            }
                        }
                    }
                    _ => at += 1,
                }
            }
            self.ctx.discard_vec(stack)?;
            if let Some(backticks) = backticks {
                for ends in self.ctx.unbox(backticks)?.0 {
                    self.ctx.discard_vec(ends)?;
                }
            }
            if let Some(math) = math {
                for ends in self.ctx.unbox(math)?.0 {
                    self.ctx.discard_vec(ends)?;
                }
            }
            if let Some(pairs) = html_pairs {
                self.ctx.discard_vec(pairs.0)?;
            }
            self.inline_note_closers = Some(closers);
        }
        let closers = self.inline_note_closers.as_ref().unwrap();
        let index = HtmlCodePairs::lower_bound(closers.len(), opener, |i| closers[i].0, self.ctx)?;
        Ok(closers
            .get(index)
            .and_then(|&(start, end)| (start == opener && end > opener + 2).then_some(end)))
    }

    fn handle_mapped_inline_note(&mut self, line_offsets: &[usize]) -> Result<Node<'a>, H::Error> {
        let opener = self.scanner.pos;
        let body_start = opener + 2;
        let end = match self.mapped_inline_note_end(opener, line_offsets)? {
            Some(end) if inline_note_has_content(&self.input[body_start..end], self.ctx)? => {
                Some(end)
            }
            _ => None,
        };
        let Some(end) = end else {
            self.scanner.pos += 1;
            return self.make_inline_text_owned(opener, opener + 1);
        };
        let line = self.line;
        if self.ctx.is_boundary_probe() {
            let text = self.make_inline_text_owned(opener, end + 1)?;
            self.scanner.pos = end + 1;
            self.adjust_node_newlines(text, end + 1 - opener, 0, line_offsets)?;
            return Ok(text);
        }
        let buffer = self
            .buffer
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let body = RSpan {
            buffer,
            start: self.base + body_start,
            end: self.base + end,
        };
        let reference = self.ctx.boxed(NodeFootnoteReference {
            name: String::new(),
            texts: Vec::new(),
            ref_num: 0,
            ix: 0,
        })?;
        let reference = self.make_inline(NodeValue::FootnoteReference(reference), opener, end)?;
        let node = reference
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let first_column =
            usize::try_from(body_start as isize + self.column_offset + self.line_offset as isize)
                .map_err(|_| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        self.scanner.pos = end + 1;
        self.adjust_node_newlines(reference, end + 1 - opener, 0, line_offsets)?;
        if self.ctx.is_shadow() {
            self.ctx.fact(AcceptedFact::InlineFootnote {
                node,
                definition: None,
                body,
            })?;
            return Ok(reference);
        }

        let body_at = self
            .ctx
            .accepted_copy_piece(RSpan {
                start: body.start - 1,
                end: body.start,
                ..body
            })?
            .end;
        let span = reference.data().sourcepos;
        let definition = make_inline(
            self.ctx,
            self.arena,
            NodeValue::FootnoteDefinition(NodeFootnoteDefinition {
                name: String::new(),
                total_references: 0,
            }),
            span,
        )?;
        let definition_id = definition
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let paragraph = make_inline(
            self.ctx,
            self.arena,
            NodeValue::Paragraph,
            (
                line,
                first_column + 1,
                span.end.line,
                span.end.column.saturating_sub(1).max(1),
            )
                .into(),
        )?;
        let paragraph_id = paragraph
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        self.ctx.fact(AcceptedFact::BlockStart {
            node: definition_id,
            at: body_at,
        })?;
        self.ctx.fact(AcceptedFact::BlockStart {
            node: paragraph_id,
            at: body_at,
        })?;
        self.ctx.fact(AcceptedFact::InlineFootnote {
            node,
            definition: Some(definition_id),
            body,
        })?;
        definition.append(paragraph);
        // Register before the nested parse, retaining allocation order even
        // if this body creates a second detached definition.
        self.ctx
            .push(&mut self.footnote_defs.native_defs, definition)?;

        let first = line - self.block_line;
        let last = self.line - self.block_line;
        let continuation = line_offsets
            .get(first + 1..last + 1)
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let mut offsets = Vec::new();
        self.ctx.push(&mut offsets, first_column)?;
        for &offset in continuation {
            self.ctx.event()?;
            self.ctx.push(&mut offsets, offset)?;
        }
        let text = &self.input[body_start..end];
        let depth = self.inline_footnote_depth + 1;
        let mut subject = Subject::new(
            self.ctx,
            Some(buffer),
            body.start,
            self.arena,
            self.options,
            text,
            line,
            self.refmap,
            self.tables,
            self.footnote_defs,
            self.delimiter_arena,
            depth,
        );
        if subject.ctx.inline_policy == native::InlinePolicy::StrictG4
            && native::g4::has_raw_autolink_marker(text.as_bytes(), subject.ctx)?
        {
            native::g4::with_leaf(paragraph_id, buffer, body.start, text.len(), |leaf| {
                let released = native::g4::shadow::run_with_note_depth(
                    leaf,
                    native::g4::shadow::Seed {
                        raw: text,
                        offsets: &offsets,
                        line,
                        in_paragraph: true,
                        in_table_cell: false,
                        options: subject.options,
                        refmap: subject.refmap,
                        tables: subject.tables,
                    },
                    subject.ctx,
                    depth,
                )?;
                let selection = native::g4::select(text, released, subject.ctx)?;
                subject.parse_selected(paragraph, line, &offsets, selection)
            })?;
        } else {
            while subject.parse_inline(paragraph, line, &offsets)? {}
        }
        subject.process_emphasis(0)?;
        subject.clear_brackets()?;
        subject.release_owned()?;
        self.ctx.discard_vec(offsets)?;
        Ok(reference)
    }

    fn handle_inline_footnote(&mut self) -> Result<Option<Node<'a>>, H::Error> {
        let result = 'native: {
            let startpos = self.scanner.pos;

            // We're at ^, next should be [
            self.scanner.pos += 2; // Skip ^[

            // Find the closing ]
            let mut depth = 1;
            let mut endpos = self.scanner.pos;
            while endpos < self.input.len() && depth > 0 {
                match self.input.as_bytes()[endpos] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'\\' if endpos + 1 < self.input.len() => {
                        endpos += 1; // Skip escaped character
                    }
                    _ => {}
                }
                endpos += 1;
            }

            if depth != 0 {
                // No matching closing bracket, treat as regular text
                self.scanner.pos = startpos + 1;
                break 'native Some(self.make_inline(
                    NodeValue::Text("^".into()),
                    startpos,
                    startpos,
                )?);
            }

            // endpos is now one past the ], so adjust
            endpos -= 1;

            // Extract the content
            let content = self.ctx.string(&self.input[self.scanner.pos..endpos])?;

            // Empty inline footnote should not parse
            if content.is_empty() {
                self.scanner.pos = startpos + 1;
                break 'native Some(self.make_inline(
                    NodeValue::Text("^".into()),
                    startpos,
                    startpos,
                )?);
            }

            // Generate unique name
            let name = self.footnote_defs.next_name();

            // Create the footnote reference node
            let ref_node = self.make_inline(
                NodeValue::FootnoteReference(Box::new(NodeFootnoteReference {
                    name: name.clone(),
                    texts: vec![], // Unused.
                    ref_num: 0,
                    ix: 0,
                })),
                startpos,
                endpos,
            )?;

            // Parse the content as inlines
            let def_node = self.arena.try_alloc(
                Ast::new(
                    NodeValue::FootnoteDefinition(NodeFootnoteDefinition {
                        name,
                        total_references: 0,
                    }),
                    (self.line, 1).into(),
                )
                .into(),
                self.ctx,
            )?;

            // Create a paragraph to hold the inline content
            let para_ast = Ast::new(
                NodeValue::Paragraph,
                (1, 1).into(), // Use line 1 as base
            );
            // Build line_offsets by scanning for newlines in the content
            let mut line_offsets = vec![0];

            let mut i = 0;
            let bytes = content.as_bytes();
            let len = content.len();
            while i < len {
                match bytes[i] {
                    b'\r' if i + 1 < len && bytes[i + 1] == b'\n' => {
                        i += 1;
                        line_offsets.push(i + 1);
                    }
                    b'\n' | b'\r' => {
                        line_offsets.push(i + 1);
                    }
                    _ => {}
                }
                i += 1;
            }

            let para_node = self.arena.try_alloc(para_ast.into(), self.ctx)?;
            def_node.append(para_node);

            // Parse the content recursively as inlines
            let mut subj = Subject::new(
                self.ctx,
                self.buffer,
                self.base,
                self.arena,
                self.options,
                content,
                1, // Use line 1 to match the paragraph's sourcepos
                self.refmap,
                self.tables,
                self.footnote_defs,
                self.delimiter_arena,
                self.inline_footnote_depth + 1,
            );

            while subj.parse_inline(para_node, 1, &line_offsets)? {}
            subj.process_emphasis(0)?;
            subj.clear_brackets()?;
            subj.release_owned()?;

            // Check if the parsed content is empty or contains only whitespace
            // This handles whitespace-only content, null bytes, etc. generically
            let has_non_whitespace_content = para_node.children().any(|child| {
                let child_data = child.data();
                match &child_data.value {
                    NodeValue::Text(text) => !text
                        .owned_str()
                        .expect("legacy inline footnotes require owned text")
                        .trim()
                        .is_empty(),
                    NodeValue::SoftBreak | NodeValue::LineBreak => false,
                    _ => true, // Any other node type (link, emphasis, etc.) counts as content
                }
            });

            if !has_non_whitespace_content {
                // Content is empty or whitespace-only after parsing, treat as literal text
                self.scanner.pos = startpos + 1;
                break 'native Some(self.make_inline(
                    NodeValue::Text("^".into()),
                    startpos,
                    startpos,
                )?);
            }

            // Store the footnote definition
            self.footnote_defs.add_definition(def_node);

            // Move position past the closing ]
            self.scanner.pos = endpos + 1;

            Some(ref_node)
        };
        Ok(result)
    }

    // Heuristics used from https://pandoc.org/MANUAL.html#extension-tex_math_dollars
    fn handle_dollars(&mut self, parent_line_offsets: &[usize]) -> Result<Node<'a>, H::Error> {
        let result = 'native: {
            if !(self.options.extension.math_dollars || self.options.extension.math_code) {
                self.scanner.pos += 1;
                break 'native self.make_inline(
                    NodeValue::Text("$".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?;
            }
            let startpos = self.scanner.pos;
            let opendollars = self.take_while(b'$')?;
            let mut code_math = false;

            // check for code math
            if opendollars == 1
                && self.options.extension.math_code
                && (self.peek_byte() == Some(b'`'))
            {
                code_math = true;
                self.scanner.pos += 1;
            }
            let fence_length = if code_math { 2 } else { opendollars };

            let endpos: Option<usize> = if code_math {
                self.scan_to_closing_code_dollar()?
            } else {
                self.scan_to_closing_dollar(opendollars)?
            }
            .filter(|endpos| endpos - startpos > fence_length * 2);

            if let Some(endpos) = endpos {
                if self.ctx.is_boundary_probe() {
                    return node_for_probe(self.ctx, self.arena, self.line);
                }
                let math = NodeMath {
                    dollar_math: !code_math,
                    display_math: opendollars == 2,
                    literal: ValueText::pending(),
                };
                let node = self.make_inline(NodeValue::Math(math), startpos, endpos - 1)?;
                let source = match (node.data().native_id, self.buffer) {
                    (Some(id), Some(buffer)) => {
                        let literal_source = RSpan {
                            buffer,
                            start: self.base + startpos + fence_length,
                            end: self.base + endpos - fence_length,
                        };
                        self.ctx.fact(AcceptedFact::Math {
                            node: id,
                            syntax: RSpan {
                                buffer,
                                start: self.base + startpos,
                                end: self.base + endpos,
                            },
                            literal_source,
                            delimiter_form: if code_math {
                                native::MathSyntax::CodeInline
                            } else if opendollars == 2 {
                                native::MathSyntax::DollarDisplay
                            } else {
                                native::MathSyntax::DollarInline
                            },
                        })?;
                        Some((id, literal_source))
                    }
                    _ => None,
                };
                let buf = &self.input[startpos + fence_length..endpos - fence_length];
                let literal = if code_math || opendollars == 1 {
                    strings::normalize_code_with_host(buf, self.ctx, source)?.into()
                } else {
                    self.ctx.admit_decoded(buf.len() as u64)?;
                    if self.ctx.direct {
                        ValueText::pending()
                    } else {
                        self.ctx.string(buf)?.into()
                    }
                };
                let literal = self.ctx.finish_node_value(
                    literal,
                    node.data().native_id,
                    NodeValueRole::MathInput,
                )?;
                if let NodeValue::Math(math) = &mut node.data_mut().value {
                    math.literal = literal;
                }
                self.adjust_node_newlines(
                    node,
                    endpos - startpos - fence_length,
                    fence_length,
                    parent_line_offsets,
                )?;
                node
            } else if code_math {
                self.scanner.pos = startpos + 1;
                self.make_inline(
                    NodeValue::Text("$".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?
            } else {
                self.scanner.pos = startpos + fence_length;
                let literal = self.ctx.string(&self.input[startpos..self.scanner.pos])?;
                self.make_inline(
                    NodeValue::Text(literal.into()),
                    self.scanner.pos - fence_length,
                    self.scanner.pos - 1,
                )?
            }
        };
        Ok(result)
    }

    /////////////////////////////////////
    // Emphasis and bracket processing //
    /////////////////////////////////////

    // After parsing a block (and sometimes during), this function traverses the
    // stack of `Delimiters`, tokens ("*", "_", etc.) that may delimit regions
    // of text for special rendering: emphasis, strong, superscript, subscript,
    // spoilertext; looking for pairs of opening and closing delimiters,
    // with the goal of placing the intervening nodes into new emphasis,
    // etc AST nodes.
    //
    // The term stack here is a bit of a misnomer, as the `Delimiters` actually
    // form a doubly-linked list. Items are pushed onto the stack during parsing,
    // but during post-processing are removed from arbitrary locations.
    //
    // The `Delimiter` contains references AST `Text` nodes, which are also
    // linked into the AST as siblings in the order they are parsed. This
    // function doesn't know a-priori which ones are markdown syntax and which
    // are just text: candidate delimiters that match have their nodes removed
    // from the AST, as they are markdown, and their intervening siblings
    // lowered into a new AST parent node via the `insert_emph` function;
    // candidate delimiters that don't match are left in the tree.
    //
    // The basic algorithm is to start at the bottom of the stack, walk upwards
    // looking for closing delimiters, and from each closing delimiter walk back
    // down the stack looking for its matching opening delimiter. This traversal
    // favors the smallest matching leftmost pairs, e.g.
    //
    //   _a *b c_ d* e_
    //    ~~~~~~
    //
    // (The emphasis region is wavy-underlined)
    //
    // All of the `_` and `*` tokens are scanned as candidates, but only the
    // region "a *b c" is lowered into an `Emph` node; the other candidate
    // delimiters are all actually text.
    //
    // And in
    //
    //   _a _b c_
    //       ~~~
    //
    // "b c" is the emphasis region, not "a _b c".
    //
    // Note that Delimiters are matched by comparing their `delim_char`, which
    // is simply a value used to compare opening and closing delimiters - the
    // actual text value of the scanned token can theoretically be different.
    //
    // There's some additional trickiness in the logic because "_", "__", and
    // "___" (and etc. etc.) all share the same delim_char, but represent
    // different emphasis. Note also that "_"- and "*"-delimited regions have
    // complex rules for which can be opening and/or closing delimiters,
    // determined in `scan_delims`.
    pub fn process_emphasis(&mut self, stack_bottom: usize) -> Result<(), H::Error> {
        {
            // This array is an important optimization that prevents searching down
            // the stack for openers we've previously searched for and know don't
            // exist, preventing exponential blowup on pathological cases.
            let mut openers_bottom: [usize; 54] = [stack_bottom; 54];

            // This is traversing the stack from the top to the bottom, setting `closer` to
            // the delimiter directly above `stack_bottom`. In the case where we are processing
            // emphasis on an entire block, `stack_bottom` is `None`, so `closer` references
            // the very bottom of the stack.
            let mut candidate = self.last_delimiter;
            let mut closer: Option<&Delimiter> = None;
            while candidate.is_some_and(|c| c.position >= stack_bottom) {
                self.ctx.event()?;
                closer = candidate;
                candidate = candidate.unwrap().prev.get();
            }

            while let Some(c) = closer {
                self.ctx.event()?;
                if c.can_close {
                    // Each time through the outer `closer` loop we reset the opener
                    // to the element below the closer, and search down the stack
                    // for a matching opener.

                    let mut opener = c.prev.get();
                    let mut opener_found = false;

                    let base = match c.delim_byte {
                        b'|' => 0,
                        b'~' => 1,
                        b'^' => 2,
                        b'"' => 3,
                        b'\'' => 4,
                        b'_' => 5,
                        b'*' => 6,
                        b'=' => 7,
                        b'+' => 8,
                        _ => unreachable!(),
                    };
                    let ix = base * 6 + (c.can_open as usize) * 3 + c.length % 3;

                    // Here's where we find the opener by searching down the stack,
                    // looking for matching delims with the `can_open` flag.
                    // On any invocation, on the first time through the outer
                    // `closer` loop, this inner `opener` search doesn't succeed:
                    // when processing a full block, `opener` starts out `None`;
                    // when processing emphasis otherwise, opener will be equal to
                    // `stack_bottom`.
                    //
                    // This search short-circuits for openers we've previously
                    // failed to find, avoiding repeatedly rescanning the bottom of
                    // the stack, using the openers_bottom array.
                    while opener.is_some_and(|o| o.position >= openers_bottom[ix]) {
                        self.ctx.event()?;
                        let o = opener.unwrap();
                        // Tildes consume a whole matched run. Keep looking past
                        // unequal runs so one mismatch cannot end the closer loop.
                        if o.can_open
                            && o.delim_byte == c.delim_byte
                            && (c.delim_byte != b'~' || o.length == c.length)
                        {
                            // This is a bit convoluted; see points 9 and 10 here:
                            // http://spec.commonmark.org/0.28/#can-open-emphasis.
                            // This is to aid processing of runs like this:
                            // “***hello*there**” or “***hello**there*”. In this
                            // case, the middle delimiter can both open and close
                            // emphasis; when trying to find an opening delimiter
                            // that matches the last ** or *, we need to skip it,
                            // and this algorithm ensures we do. (The sum of the
                            // lengths are a multiple of 3.)
                            let odd_match = (c.can_open || o.can_close)
                                && ((o.length + c.length) % 3 == 0)
                                && !(o.length % 3 == 0 && c.length % 3 == 0);
                            if !odd_match {
                                opener_found = true;
                                break;
                            }
                        }
                        opener = o.prev.get();
                    }

                    let old_c = c;

                    // There's a case here for every possible delimiter. If we found
                    // a matching opening delimiter for our closing delimiter, they
                    // both get passed.
                    if self.tables.emph[c.delim_byte as usize] {
                        if opener_found {
                            // Finally, here's the happy case where the delimiters
                            // match and they are inserted. We get a new closer
                            // delimiter and go around the loop again.
                            //
                            // Note that for "***" and "___" delimiters of length
                            // greater than 2, insert_emph will create a `Strong`
                            // node (i.e. "**"), then _truncate_ the delimiters in
                            // place, turning them into e.g. "*" delimiters, and
                            // hand us back the same mutated closer to be matched
                            // again.
                            //
                            // In general though the closer will be the next
                            // delimiter up the stack.
                            closer = self.insert_emph(opener.unwrap(), c)?;
                        } else {
                            // When no matching opener is found we move the closer
                            // up the stack, do some bookkeeping with old_closer
                            // (below), try again.
                            closer = c.next.get();
                        }
                    } else if c.delim_byte == b'\'' || c.delim_byte == b'"' {
                        *c.inl.data_mut().value.text_mut().unwrap() =
                            if c.delim_byte == b'\'' { "’" } else { "”" }.into();
                        closer = c.next.get();

                        if opener_found {
                            *opener.unwrap().inl.data_mut().value.text_mut().unwrap() =
                                if old_c.delim_byte == b'\'' {
                                    "‘"
                                } else {
                                    "“"
                                }
                                .into();
                            self.remove_delimiter(opener.unwrap())?;
                            self.remove_delimiter(old_c)?;
                        }
                    }

                    // If the search for an opener was unsuccessful, then record
                    // the position the search started at in the `openers_bottom`
                    // so that the `opener` search can avoid looking for this
                    // same opener at the bottom of the stack later.
                    if !opener_found {
                        openers_bottom[ix] = old_c.position;

                        // Now that we've failed the `opener` search starting from
                        // `old_closer`, future opener searches will be searching it
                        // for openers - if `old_closer` can't be used as an opener
                        // then we know it's just text - remove it from the
                        // delimiter stack, leaving it in the AST as text
                        if !old_c.can_open {
                            self.remove_delimiter(old_c)?;
                        }
                    }
                } else {
                    // Closer is !can_close. Move up the stack
                    closer = c.next.get();
                }
            }

            // At this point the entire delimiter stack from `stack_bottom` up has
            // been scanned for matches, everything left is just text. Pop it all
            // off.
            self.remove_delimiters(stack_bottom)?;
        };
        Ok(())
    }

    fn remove_delimiter(&mut self, delimiter: &'d Delimiter<'a, 'd>) -> Result<(), H::Error> {
        {
            if let Some(next) = delimiter.next.get() {
                next.prev.set(delimiter.prev.get());
            } else {
                assert!(Some(delimiter) == self.last_delimiter);
                self.last_delimiter = delimiter.prev.get();
            }

            if let Some(prev) = delimiter.prev.get() {
                prev.next.set(delimiter.next.get());
            }
        };
        Ok(())
    }

    fn remove_delimiters(&mut self, stack_bottom: usize) -> Result<(), H::Error> {
        while let Some(last) = self.last_delimiter {
            if last.position < stack_bottom {
                break;
            }
            self.ctx.event()?;
            self.remove_delimiter(last)?;
        }
        Ok(())
    }

    fn push_delimiter(
        &mut self,
        c: u8,
        can_open: bool,
        can_close: bool,
        inl: Node<'a>,
    ) -> Result<(), H::Error> {
        {
            let length = self.ctx.value_len(inl.data().value.text().unwrap())?;
            let d = self.delimiter_arena.try_alloc(
                Delimiter {
                    prev: Cell::new(self.last_delimiter),
                    next: Cell::new(None),
                    inl,
                    position: self.scanner.pos,
                    length,
                    delim_byte: c,
                    can_open,
                    can_close,
                },
                self.ctx,
            )?;
            if let Some(last) = self.last_delimiter {
                last.next.set(Some(d));
            }
            self.last_delimiter = Some(d);
        };
        Ok(())
    }

    // Create a new emphasis node, move all the nodes between `opener`
    // and `closer` into it, and insert it into the AST.
    //
    // As a side-effect, handle long "***" and "___" nodes by truncating them in
    // place to be re-matched by `process_emphasis`.
    fn insert_emph(
        &mut self,
        opener: &'d Delimiter<'a, 'd>,
        closer: &'d Delimiter<'a, 'd>,
    ) -> Result<Option<&'d Delimiter<'a, 'd>>, H::Error> {
        let result = 'native: {
            let opener_byte = self
                .ctx
                .value_first_byte(opener.inl.data().value.text().unwrap())?
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            let mut opener_num_bytes = self
                .ctx
                .value_len(opener.inl.data().value.text().unwrap())?;
            let mut closer_num_bytes = self
                .ctx
                .value_len(closer.inl.data().value.text().unwrap())?;
            let use_delims = if closer_num_bytes >= 2 && opener_num_bytes >= 2 {
                2
            } else {
                1
            };

            opener_num_bytes -= use_delims;
            closer_num_bytes -= use_delims;

            if (self.options.extension.strikethrough || self.options.extension.subscript)
                && opener_byte == b'~'
                && (opener_num_bytes != closer_num_bytes || opener_num_bytes > 0)
            {
                break 'native None;
            }

            if !self.ctx.direct {
                // Direct crop belongs to the original Emphasis fact below,
                // after its claim host and delimiter ancestry are accepted.
                let mut opener_data = opener.inl.data_mut();
                let opener_text = opener_data
                    .value
                    .text_mut()
                    .unwrap()
                    .owned_cow_mut()
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                self.ctx.cow_mut(opener_text)?.truncate(opener_num_bytes);
                drop(opener_data);
                let mut closer_data = closer.inl.data_mut();
                let closer_text = closer_data
                    .value
                    .text_mut()
                    .unwrap()
                    .owned_cow_mut()
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                self.ctx.cow_mut(closer_text)?.truncate(closer_num_bytes);
            }

            // Remove all the candidate delimiters from between the opener and the
            // closer. None of them are matched pairs. They've been scanned already.
            let mut prev = closer.prev.get();
            while let Some(delim) = prev {
                self.ctx.event()?;
                if delim == opener {
                    break;
                }
                self.remove_delimiter(delim)?;
                prev = delim.prev.get();
            }

            let emph = self.make_inline(
                if self.options.extension.subscript && opener_byte == b'~' && use_delims == 1 {
                    NodeValue::Subscript
                } else if opener_byte == b'~' {
                    // Not emphasis
                    // Unlike for |, these cases have to be handled because they will match
                    // in the event subscript but not strikethrough is enabled
                    if self.options.extension.strikethrough {
                        NodeValue::Strikethrough
                    } else if use_delims == 1 {
                        NodeValue::EscapedTag("~")
                    } else {
                        NodeValue::EscapedTag("~~")
                    }
                } else if self.options.extension.superscript && opener_byte == b'^' {
                    NodeValue::Superscript
                } else if opener_byte == b'=' {
                    if self.options.extension.highlight {
                        NodeValue::Highlight
                    } else {
                        NodeValue::EscapedTag("==")
                    }
                } else if opener_byte == b'+' {
                    if self.options.extension.insert {
                        NodeValue::Insert
                    } else {
                        NodeValue::EscapedTag("++")
                    }
                } else if self.options.extension.spoiler && opener_byte == b'|' {
                    if use_delims == 2 {
                        NodeValue::SpoileredText
                    } else {
                        NodeValue::EscapedTag("|")
                    }
                } else if self.options.extension.underline && opener_byte == b'_' && use_delims == 2
                {
                    NodeValue::Underline
                } else if use_delims == 1 {
                    NodeValue::Emph
                } else {
                    NodeValue::Strong
                },
                // These are overridden immediately below.
                self.scanner.pos,
                self.scanner.pos,
            )?;
            if let (Some(node), Some(opener), Some(closer)) = (
                emph.data().native_id,
                opener.inl.data().native_id,
                closer.inl.data().native_id,
            ) {
                self.ctx.fact(AcceptedFact::Emphasis {
                    node,
                    opener,
                    closer,
                    used: use_delims,
                })?;
            }
            if self.ctx.is_shadow()
                && self.ctx.capture
                && matches!(
                    emph.data().value,
                    NodeValue::Highlight | NodeValue::Superscript | NodeValue::Subscript
                )
            {
                let (Some(node), Some(buffer)) = (emph.data().native_id, self.buffer) else {
                    return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
                };
                let start = opener
                    .position
                    .checked_sub(opener.length)
                    .and_then(|at| at.checked_add(opener_num_bytes))
                    .and_then(|at| self.base.checked_add(at))
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                let end = closer
                    .position
                    .checked_sub(closer_num_bytes)
                    .and_then(|at| self.base.checked_add(at))
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                self.ctx.shadow_delimited(
                    node,
                    RSpan { buffer, start, end },
                    opener_byte,
                    use_delims as u8,
                )?;
            }

            emph.data_mut().sourcepos = (
                opener
                    .inl
                    .data()
                    .sourcepos
                    .start
                    .column_add(opener_num_bytes as isize),
                closer
                    .inl
                    .data()
                    .sourcepos
                    .end
                    .column_add(-(closer_num_bytes as isize)),
            )
                .into();

            // Drop all the interior AST nodes into the emphasis node
            // and then insert the emphasis node
            let mut it = opener.inl.next_sibling().unwrap();
            while !it.same_node(closer.inl) {
                self.ctx.event()?;
                let next = it.next_sibling();
                emph.append(it);
                if let Some(n) = next {
                    it = n;
                } else {
                    break;
                }
            }
            opener.inl.insert_after(emph);

            // Drop completely "used up" delimiters, adjust sourcepos of those not,
            // and return the next closest one for processing.
            if opener_num_bytes == 0 {
                opener.inl.detach();
                self.remove_delimiter(opener)?;
            } else {
                opener.inl.data_mut().sourcepos.end.column -= use_delims;
            }

            if closer_num_bytes == 0 {
                closer.inl.detach();
                self.remove_delimiter(closer)?;
                closer.next.get()
            } else {
                closer.inl.data_mut().sourcepos.start.column += use_delims;
                Some(closer)
            }
        };
        Ok(result)
    }

    fn observe_image_open(
        &mut self,
        image: bool,
        position: usize,
    ) -> Result<Option<ImageProbeKey>, H::Error> {
        if image && let Some(memo) = &mut self.recording_images {
            let key = memo.key(position, self.flags, self.ctx)?;
            memo.record(key, false, self.ctx)?;
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    fn push_bracket(
        &mut self,
        image: bool,
        inl_text: Node<'a>,
        line_offsets: &[usize],
        in_paragraph: bool,
    ) -> Result<(), H::Error> {
        let inside_image = image || self.brackets.last().is_some_and(|last| last.inside_image);
        if let Some(last) = self.brackets.last_mut() {
            last.bracket_after = true;
        }
        let probe_key = self.observe_image_open(image, self.scanner.pos)?;
        self.ctx.small_push(
            &mut self.brackets,
            Bracket {
                inl_text,
                position: self.scanner.pos,
                image,
                bracket_after: false,
                probe_key,
                inside_image,
                html_scope: self.html_scope,
                inherited_html_opacity: self.inherited_html_opacity,
            },
        )?;
        if image {
            if self.ctx.inline_html.enabled()
                && !self.inherited_html_opacity
                && self.html_scope.opaque()
                && self.image_inherits_html_scope(line_offsets, in_paragraph)?
            {
                self.inherited_html_opacity = true;
            }
        } else {
            self.no_link_openers = false;
        }
        Ok(())
    }

    fn pop_bracket(&mut self) -> Option<Bracket<'a>> {
        let bracket = self.brackets.pop()?;
        if bracket.image {
            self.inherited_html_opacity = bracket.inherited_html_opacity;
        }
        Some(bracket)
    }

    fn handle_close_bracket(
        &mut self,
        parent_line_offsets: &[usize],
    ) -> Result<Option<Node<'a>>, H::Error> {
        let result = 'native: {
            self.scanner.pos += 1;
            let initial_pos = self.scanner.pos;

            let Some(last) = self.brackets.last().copied() else {
                break 'native Some(self.make_inline(
                    NodeValue::Text("]".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?);
            };

            let is_image = last.image;

            if self
                .disabled_openers_through
                .is_some_and(|end| last.position <= end)
                || (!is_image && self.no_link_openers)
            {
                self.pop_bracket();
                break 'native Some(self.make_inline(
                    NodeValue::Text("]".into()),
                    self.scanner.pos - 1,
                    self.scanner.pos - 1,
                )?);
            }

            // Ensure there was text if this was a link and not an image link
            if self.options.render.ignore_empty_links && !is_image {
                let mut non_blank_found = false;
                let mut itm = last.inl_text.next_sibling();
                while let Some(it) = itm {
                    match it.data().value {
                        NodeValue::Text(ref s)
                            if is_blank(s.owned_str().ok_or_else(|| {
                                self.ctx.host.reject(NativeViolation::InvalidProvenance)
                            })?) => {}
                        _ => {
                            non_blank_found = true;
                            break;
                        }
                    }

                    itm = it.next_sibling();
                }

                if !non_blank_found {
                    self.pop_bracket();
                    break 'native Some(self.make_inline(
                        NodeValue::Text("]".into()),
                        self.scanner.pos - 1,
                        self.scanner.pos - 1,
                    )?);
                }
            }

            let after_link_text_pos = self.scanner.pos;

            // Try to find a link destination within parenthesis

            if let Some(InlineLinkSuffix {
                url,
                starturl,
                endurl,
                starttitle,
                endtitle,
                endall,
            }) = scan_inline_link_suffix(
                &self.input[..self.input_end()],
                after_link_text_pos,
                self.ctx,
            )? {
                let source_end_pos = if !self.ctx.is_boundary_probe()
                    && endurl < endall
                    && self
                        .ctx
                        .find(&self.input.as_bytes()[endurl..endall], |bytes| {
                            bytes.iter().position(|&c| strings::is_line_end_char(c))
                        })?
                        .is_some()
                {
                    endurl
                } else {
                    endall + 1
                };

                self.scanner.pos = endall + 1;
                let (identity, url, title) = if self.ctx.is_shadow() {
                    let buffer = self
                        .buffer
                        .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                    (
                        Some(native::LinkIdentity {
                            syntax: RSpan {
                                buffer,
                                start: self.base + last.position - if is_image { 2 } else { 1 },
                                end: self.base + self.scanner.pos,
                            },
                            label: RSpan {
                                buffer,
                                start: self.base + last.position,
                                end: self.base + initial_pos - 1,
                            },
                            destination: None,
                            title: None,
                            definition: None,
                        }),
                        self.ctx.empty_value(),
                        self.ctx.empty_value(),
                    )
                } else if let Some(buffer) = self.buffer {
                    let angle = self.input.as_bytes().get(starturl) == Some(&b'<');
                    let destination = self.ctx.payload(
                        native::PayloadKind::Destination,
                        RSpan {
                            buffer,
                            start: self.base + starturl + usize::from(angle),
                            end: self.base + endurl - usize::from(angle),
                        },
                    )?;
                    let decoded_url = strings::clean_payload_with_host(
                        url,
                        native::PayloadKind::Destination,
                        self.ctx,
                        Some(destination),
                    )?;
                    let title = if endtitle == starttitle {
                        None
                    } else {
                        Some(self.ctx.payload(
                            native::PayloadKind::Title,
                            RSpan {
                                buffer,
                                start: self.base + starttitle,
                                end: self.base + endtitle,
                            },
                        )?)
                    };
                    let decoded_title = strings::clean_payload_with_host(
                        &self.input[starttitle..endtitle],
                        native::PayloadKind::Title,
                        self.ctx,
                        title,
                    )?;
                    (
                        Some(native::LinkIdentity {
                            syntax: RSpan {
                                buffer,
                                start: self.base + last.position - if is_image { 2 } else { 1 },
                                end: self.base + self.scanner.pos,
                            },
                            label: RSpan {
                                buffer,
                                start: self.base + last.position,
                                end: self.base + initial_pos - 1,
                            },
                            destination: Some(destination),
                            title,
                            definition: None,
                        }),
                        self.ctx.finish_payload_value(
                            decoded_url,
                            Some(destination),
                            native::PayloadKind::Destination,
                        )?,
                        self.ctx.finish_payload_value(
                            decoded_title,
                            title,
                            native::PayloadKind::Title,
                        )?,
                    )
                } else {
                    (
                        None,
                        self.ctx.string(&strings::clean_url(url))?.into(),
                        self.ctx
                            .string(&strings::clean_title(&self.input[starttitle..endtitle]))?
                            .into(),
                    )
                };
                self.close_bracket_match(
                    identity,
                    is_image,
                    url,
                    title,
                    source_end_pos,
                    parent_line_offsets,
                )?;
                break 'native None;
            }

            // Try to see if this is a reference link

            let footnote = if (self.ctx.capture || self.ctx.is_boundary_probe())
                && self.options.extension.footnotes
                && !last.bracket_after
                && !last.inherited_html_opacity
                && !self
                    .brackets
                    .iter()
                    .rev()
                    .nth(1)
                    .is_some_and(|outer| outer.inside_image)
            {
                self.input[last.position..initial_pos - 1].strip_prefix('^')
            } else {
                None
            };
            if self.ctx.is_boundary_probe() && self.refmap.map.entries == 0 && footnote.is_none() {
                self.pop_bracket();
                return self
                    .make_inline(
                        NodeValue::Text("]".into()),
                        initial_pos - 1,
                        initial_pos - 1,
                    )
                    .map(Some);
            }
            let (mut lab, mut found_label): (Cow<str>, bool) = match self
                .scanner
                .link_label_with_host(&self.input[..self.input_end()], self.ctx)?
            {
                Some(lab) => (self.ctx.string(lab)?.into(), true),
                None => ("".into(), false),
            };

            if !found_label {
                self.scanner.pos = initial_pos;
            }

            if (!found_label || lab.is_empty()) && !last.bracket_after {
                lab = self.input[last.position..initial_pos - 1].into();
                found_label = true;
            }

            // Need to normalize to lookup in refmap
            if self.ctx.is_boundary_probe()
                && footnote.is_none()
                && !self.refmap.map.probe_label_can_match(&lab, self.ctx)?
            {
                if let Cow::Owned(label) = lab {
                    self.ctx.discard_string(label)?;
                }
                self.scanner.pos = initial_pos;
                self.pop_bracket();
                return self
                    .make_inline(
                        NodeValue::Text("]".into()),
                        initial_pos - 1,
                        initial_pos - 1,
                    )
                    .map(Some);
            }
            let normalized_lab = strings::normalize_label_with_host(&lab, Case::Fold, self.ctx)?;
            let mut reff: Option<Cow<ResolvedReference>> = if found_label {
                self.refmap.lookup(&normalized_lab, self.ctx)?
            } else {
                None
            };

            // Attempt to use the provided broken link callback if a reference cannot be resolved
            // Only clone the original label if we actually need to call the callback
            if reff.is_none()
                && let Some(callback) = &self.options.parse.broken_link_callback
            {
                reff = callback
                    .resolve(BrokenLinkReference {
                        normalized: &normalized_lab,
                        original: &lab,
                    })
                    .map(Cow::Owned);
            }

            if let Some(reff) = reff {
                // When reff is Cow::Owned (from callback), into_owned() is a no-op
                // When reff is Cow::Borrowed (from refmap), into_owned() clones
                let identity = match (self.buffer, reff.native) {
                    (Some(buffer), Some(reference)) => Some(native::LinkIdentity {
                        syntax: RSpan {
                            buffer,
                            start: self.base + last.position - if is_image { 2 } else { 1 },
                            end: self.base + self.scanner.pos,
                        },
                        label: RSpan {
                            buffer,
                            start: self.base + last.position,
                            end: self.base + initial_pos - 1,
                        },
                        destination: Some(reference.destination),
                        title: reference.title,
                        definition: Some(reference.definition),
                    }),
                    _ => None,
                };
                let (url, title) = match reff {
                    Cow::Owned(value) if self.ctx.direct || self.ctx.is_shadow() => {
                        (value.url, value.title)
                    }
                    value => {
                        let url = value.url.owned_str().ok_or_else(|| {
                            self.ctx.host.reject(NativeViolation::InvalidProvenance)
                        })?;
                        let title = value.title.owned_str().ok_or_else(|| {
                            self.ctx.host.reject(NativeViolation::InvalidProvenance)
                        })?;
                        (
                            self.ctx.decoded_string(url)?.into(),
                            self.ctx.decoded_string(title)?.into(),
                        )
                    }
                };
                self.ctx.discard_string(normalized_lab)?;
                if let Cow::Owned(lab) = lab {
                    self.ctx.discard_string(lab)?;
                }
                self.close_bracket_match(
                    identity,
                    is_image,
                    url,
                    title,
                    self.scanner.pos,
                    parent_line_offsets,
                )?;
                break 'native None;
            }
            self.ctx.discard_string(normalized_lab)?;
            if let Cow::Owned(lab) = lab {
                self.ctx.discard_string(lab)?;
            }

            if let Some(label) = footnote
                && let Some(definition) = self.refmap.lookup_footnote(label, self.ctx)?
            {
                let start = last.position - 1;
                let member = if self.ctx.is_boundary_probe() {
                    node_for_probe(self.ctx, self.arena, self.line)?
                } else {
                    let buffer = self
                        .buffer
                        .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                    let reference = self.ctx.boxed(NodeFootnoteReference {
                        name: String::new(),
                        texts: Vec::new(),
                        ref_num: 0,
                        ix: 0,
                    })?;
                    let member = self.make_inline(
                        NodeValue::FootnoteReference(reference),
                        start,
                        initial_pos - 1,
                    )?;
                    let node = member
                        .data()
                        .native_id
                        .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                    self.ctx.fact(AcceptedFact::FootnoteReference {
                        node,
                        definition,
                        syntax: RSpan {
                            buffer,
                            start: self.base + start,
                            end: self.base + initial_pos,
                        },
                    })?;
                    member
                };
                last.inl_text.insert_before(member);
                if is_image {
                    let exclamation = self.make_inline_text_owned(start - 1, start)?;
                    member.insert_before(exclamation);
                }
                self.remove_delimiters(last.position)?;
                for sibling in last.inl_text.following_siblings() {
                    self.ctx.event()?;
                    sibling.detach();
                }
                self.no_link_openers = true;
                self.disabled_openers_through = Some(last.position);
                self.scanner.pos = initial_pos;
                self.pop_bracket();
                break 'native None;
            }

            let bracket_inl_text = last.inl_text;

            if !self.ctx.capture
                && !self.ctx.is_boundary_probe()
                && self.options.extension.footnotes
                && !last.bracket_after
                && bracket_inl_text.next_sibling().is_some_and(|n| {
                    n.data().value.text().is_some_and(|t| {
                        t.owned_str()
                            .expect("legacy footnotes require owned text")
                            .as_bytes()
                            .starts_with(b"^")
                    })
                })
            {
                let mut text = String::new();
                let mut sibling_iterator = bracket_inl_text.following_siblings();

                self.scanner.pos = initial_pos;

                // Skip the initial node, which holds the `[`
                sibling_iterator.next().unwrap();

                // The footnote name could have been parsed into multiple text/htmlinline nodes.
                // For example `[^_foo]` gives `^`, `_`, and `foo`. So pull them together.
                // Since we're handling the closing bracket, the only siblings at this point are
                // related to the footnote name.
                //
                // This re-construction of the original text value is an awful HACK
                // we should reconsider.  Can't we go back to the subject and pull
                // it from there?

                // If there's a non-Text/HtmlInline, such as SoftBreak, don't try to
                // do anything fancy here at all.
                let mut sussy = false;

                let mut texts = vec![];

                for sibling in sibling_iterator {
                    let sibling_ast = sibling.data();
                    if sibling_ast.sourcepos.start.line != sibling_ast.sourcepos.end.line
                        || sibling_ast.sourcepos.end.column < sibling_ast.sourcepos.start.column
                    {
                        sussy = true;
                        break;
                    }

                    match sibling_ast.value {
                        NodeValue::Text(ref literal) => {
                            let literal = literal.owned_str().ok_or_else(|| {
                                self.ctx.host.reject(NativeViolation::InvalidProvenance)
                            })?;
                            text.push_str(literal);
                            texts.push((
                                literal.to_string(),
                                sibling_ast.sourcepos.end.column
                                    - sibling_ast.sourcepos.start.column
                                    + 1,
                            ));
                        }
                        NodeValue::HtmlInline(ref literal) => {
                            let literal = literal.owned_str().ok_or_else(|| {
                                self.ctx.host.reject(NativeViolation::InvalidProvenance)
                            })?;
                            text.push_str(literal);
                            texts.push((
                                literal.to_string(),
                                sibling_ast.sourcepos.end.column
                                    - sibling_ast.sourcepos.start.column
                                    + 1,
                            ));
                        }
                        _ => {
                            sussy = true;
                            break;
                        }
                    };
                }

                if !sussy && text.len() > 1 {
                    let inl = self.make_inline(
                        NodeValue::FootnoteReference(Box::new(NodeFootnoteReference {
                            name: text[1..].to_string(),
                            texts,
                            ref_num: 0,
                            ix: 0,
                        })),
                        // Overridden immediately below.
                        self.scanner.pos,
                        self.scanner.pos,
                    )?;
                    inl.data_mut().sourcepos.start.column =
                        bracket_inl_text.data().sourcepos.start.column;
                    inl.data_mut().sourcepos.end.column = usize::try_from(
                        self.scanner.pos as isize + self.column_offset + self.line_offset as isize,
                    )
                    .unwrap();
                    bracket_inl_text.insert_before(inl);

                    // detach all the nodes, including bracket_inl_text
                    sibling_iterator = bracket_inl_text.following_siblings();
                    for sibling in sibling_iterator {
                        match sibling.data().value {
                            NodeValue::Text(_) | NodeValue::HtmlInline(_) => {
                                sibling.detach();
                            }
                            _ => {}
                        };
                    }

                    // We don't need to process emphasis for footnote names, so cleanup
                    // any outstanding delimiters
                    self.remove_delimiters(last.position)?;

                    self.pop_bracket();
                    break 'native None;
                }
            }

            self.pop_bracket();
            self.scanner.pos = initial_pos;
            Some(self.make_inline(
                NodeValue::Text("]".into()),
                self.scanner.pos - 1,
                self.scanner.pos - 1,
            )?)
        };
        Ok(result)
    }

    fn close_bracket_match(
        &mut self,
        identity: Option<native::LinkIdentity>,
        is_image: bool,
        url: ValueText,
        title: ValueText,
        source_end_pos: usize,
        #[cfg_attr(not(feature = "attributes"), allow(unused))] parent_line_offsets: &[usize],
    ) -> Result<(), H::Error> {
        {
            let last = self.pop_bracket().unwrap();
            if is_image {
                self.html_scope = last.html_scope;
                if let (Some(key), Some(memo)) = (last.probe_key, &mut self.recording_images) {
                    memo.record(key, true, self.ctx)?;
                }
                if self.ctx.is_boundary_probe()
                    && let Some(identity) = identity.as_ref()
                {
                    self.observed_image = Some(ObservedImage {
                        open: last.position,
                        close: identity.label.end - self.base,
                    });
                }
            }

            let nl = self.ctx.boxed(NodeLink { url, title })?;
            let inl = make_inline(
                self.ctx,
                self.arena,
                if is_image {
                    NodeValue::Image(nl)
                } else {
                    NodeValue::Link(nl)
                },
                (
                    last.inl_text.data().sourcepos.start,
                    (
                        self.line,
                        usize::try_from(
                            source_end_pos as isize
                                + self.column_offset
                                + self.line_offset as isize,
                        )
                        .unwrap(),
                    )
                        .into(),
                )
                    .into(),
            )?;

            #[cfg(feature = "attributes")]
            if self.options.extension.link_attributes {
                self.handle_potential_attribute(inl, parent_line_offsets)?;
            }

            if let (Some(node), Some(identity)) = (inl.data().native_id, identity) {
                self.ctx.fact(AcceptedFact::Link {
                    node,
                    syntax: identity.syntax,
                    label: identity.label,
                    image: is_image,
                    wiki: None,
                    destination: identity.destination,
                    title: identity.title,
                    definition: identity.definition,
                })?;
                if let Some(definition) = identity.definition {
                    self.ctx.fact(AcceptedFact::ReferenceUse {
                        node,
                        definition,
                        syntax: identity.syntax,
                    })?;
                }
            }
            last.inl_text.insert_before(inl);
            let mut itm = last.inl_text.next_sibling();
            while let Some(it) = itm {
                self.ctx.event()?;
                itm = it.next_sibling();
                inl.append(it);
            }
            last.inl_text.detach();
            self.process_emphasis(last.position)?;

            if !is_image {
                self.no_link_openers = true;
            }
        };
        Ok(())
    }

    pub fn clear_brackets(&mut self) -> Result<(), H::Error> {
        {
            self.brackets.clear();
        };
        Ok(())
    }

    ////////////////////
    // Input scanning //
    ////////////////////

    #[inline]
    fn eof(&self) -> bool {
        self.scanner.pos >= self.input_end()
    }

    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.input
            .as_bytes()
            .get(..self.input_end())?
            .get(self.scanner.pos)
            .copied()
    }

    #[inline]
    fn peek_byte_n(&self, n: usize) -> Option<u8> {
        let pos = self.scanner.pos.checked_add(n)?;
        self.input
            .as_bytes()
            .get(..self.input_end())?
            .get(pos)
            .copied()
    }

    #[inline]
    fn skip_spaces(&mut self) -> Result<bool, H::Error> {
        let result = {
            self.scanner
                .skip_spaces_with_host(&self.input[..self.input_end()], self.ctx)?
        };
        Ok(result)
    }

    #[inline]
    fn skip_line_end(&mut self) -> Result<bool, H::Error> {
        let result = {
            self.scanner
                .skip_line_end_with_host(&self.input[..self.input_end()], self.ctx)?
        };
        Ok(result)
    }

    #[inline]
    fn take_while(&mut self, b: u8) -> Result<usize, H::Error> {
        let result = {
            self.scanner
                .take_while_with_host(&self.input[..self.input_end()], b, self.ctx)?
        };
        Ok(result)
    }

    #[inline]
    fn take_while_with_limit(&mut self, b: u8, limit: usize) -> Result<usize, H::Error> {
        let result = {
            self.scanner.take_while_with_limit_with_host(
                &self.input[..self.input_end()],
                b,
                limit,
                self.ctx,
            )?
        };
        Ok(result)
    }

    fn find_special_char(&mut self) -> Result<usize, H::Error> {
        let result = 'native: {
            let bytes = &self.input.as_bytes()[..self.input_end()];
            let mut i = self.scanner.pos;
            while i < bytes.len() {
                self.ctx.scan_step()?;
                if self.tables.special[bytes[i] as usize] && self.is_special_char_at(i) {
                    break 'native i;
                }
                i += 1;
            }
            i
        };
        Ok(result)
    }

    fn is_special_char_at(&self, i: usize) -> bool {
        let bytes = &self.input.as_bytes()[..self.input_end()];
        match bytes[i] {
            b'^' => {
                self.brackets.is_empty()
                    && (self.options.extension.superscript
                        || self.options.extension.inline_footnotes
                            && bytes.get(i + 1) == Some(&b'['))
            }
            b'w' => bytes[i..].starts_with(b"www."),
            b':' => !self.tables.colon_needs_slashes || bytes[i + 1..].starts_with(b"//"),
            _ => true,
        }
    }

    fn scan_to_closing_backtick(
        &mut self,
        openticklength: usize,
    ) -> Result<Option<usize>, H::Error> {
        let result = 'native: {
            if openticklength > MAXBACKTICKS {
                break 'native None;
            }

            if let Some(index) = self.probe_backticks {
                let end = index.closing(
                    self.input.as_bytes(),
                    self.scanner.pos,
                    self.gap_hi,
                    openticklength,
                    self.ctx,
                )?;
                self.scanner.pos = end.unwrap_or(self.gap_hi);
                break 'native end;
            }
            if self.scanned_for_backticks
                && (self.backticks_valid & (1u128 << openticklength) == 0
                    || self.backticks[openticklength] <= self.scanner.pos)
            {
                break 'native None;
            }

            let matcher = backtick_matcher();
            loop {
                let Some(index) = self.ctx.find(
                    &self.input.as_bytes()[self.scanner.pos..self.input_end()],
                    |part| matcher.find(part),
                )?
                else {
                    self.scanner.pos = self.input_end();
                    self.scanned_for_backticks = true;
                    break 'native None;
                };
                self.scanner.pos += index;
                let numticks = self.take_while(b'`')?;
                if numticks <= MAXBACKTICKS {
                    self.backticks[numticks] = self.scanner.pos - numticks;
                    self.backticks_valid |= 1u128 << numticks;
                }
                if numticks == openticklength {
                    break 'native Some(self.scanner.pos);
                }
            }
        };
        Ok(result)
    }

    fn scan_to_closing_dollar(
        &mut self,
        opendollarlength: usize,
    ) -> Result<Option<usize>, H::Error> {
        let result = 'native: {
            if !self.options.extension.math_dollars || opendollarlength > MAX_MATH_DOLLARS {
                break 'native None;
            }

            // space not allowed after initial $
            if opendollarlength == 1 && self.peek_byte().is_some_and(isspace) {
                break 'native None;
            }
            if let Some(index) = self.probe_math {
                let end = index.dollar_end(
                    &self.input.as_bytes()[..self.gap_hi],
                    self.scanner.pos,
                    opendollarlength,
                    self.ctx,
                )?;
                self.scanner.pos = end.unwrap_or(self.gap_hi);
                break 'native end;
            }

            let matcher = dollar_matcher();
            loop {
                let Some(index) = self.ctx.find(
                    &self.input.as_bytes()[self.scanner.pos..self.input_end()],
                    |part| matcher.find(part),
                )?
                else {
                    self.scanner.pos = self.input.len();
                    break 'native None;
                };
                self.scanner.pos += index;

                let c = self.input.as_bytes()[self.scanner.pos - 1];

                // space not allowed before ending $
                if opendollarlength == 1 && isspace(c) {
                    break 'native None;
                }

                // dollar signs must also be backslash-escaped if they occur within math
                if opendollarlength == 1 && c == b'\\' {
                    self.scanner.pos += 1;
                    continue;
                }

                let numdollars = self.take_while_with_limit(b'$', opendollarlength)?;

                // ending $ can't be followed by a digit
                if opendollarlength == 1 && self.peek_byte().is_some_and(isdigit) {
                    break 'native None;
                }

                if numdollars == opendollarlength {
                    break 'native Some(self.scanner.pos);
                }
            }
        };
        Ok(result)
    }

    fn scan_to_closing_code_dollar(&mut self) -> Result<Option<usize>, H::Error> {
        let result = 'native: {
            assert!(self.options.extension.math_code);
            if let Some(index) = self.probe_math {
                let end = index
                    .first(
                        ProbeMathCloser::CodeDollar,
                        self.scanner.pos,
                        self.gap_hi,
                        self.ctx,
                    )?
                    .map(|pos| pos + 1);
                self.scanner.pos = end.unwrap_or(self.gap_hi);
                break 'native end;
            }

            let matcher = dollar_matcher();
            loop {
                let Some(index) = self.ctx.find(
                    &self.input.as_bytes()[self.scanner.pos..self.input_end()],
                    |part| matcher.find(part),
                )?
                else {
                    self.scanner.pos = self.input.len();
                    break 'native None;
                };
                self.scanner.pos += index;

                let c = self.input.as_bytes()[self.scanner.pos - 1];
                self.scanner.pos += 1;
                if c == b'`' {
                    break 'native Some(self.scanner.pos);
                }
            }
        };
        Ok(result)
    }

    fn scan_to_closing_latex_math(
        &mut self,
        startpos: usize,
        closing_delimiter: u8,
    ) -> Result<Option<usize>, H::Error> {
        let result = 'native: {
            let bytes = self.input.as_bytes();
            let end = self.input_end();
            if startpos > end {
                return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            if let Some(index) = self.probe_math {
                let kind = if closing_delimiter == b')' {
                    ProbeMathCloser::LatexParen
                } else {
                    ProbeMathCloser::LatexBracket
                };
                break 'native index.first(kind, startpos, end, self.ctx)?;
            }
            let matcher = backslash_matcher();
            let mut at = startpos;
            while at < end {
                let Some(index) = self.ctx.find(&bytes[at..end], |part| matcher.find(part))? else {
                    break 'native None;
                };
                let pos = at + index;
                // The second closing byte belongs to the same active gap.
                // No lookahead beyond it may make an incomplete delimiter close.
                if pos + 1 < end {
                    self.ctx.scan(1)?;
                    if bytes[pos + 1] == closing_delimiter {
                        break 'native Some(pos);
                    }
                }
                at = pos + 1;
            }
            None
        };
        Ok(result)
    }

    fn get_before_char(&mut self, pos: usize) -> Result<(char, Option<usize>), H::Error> {
        if pos == 0 {
            return Ok(('\n', None));
        }
        let mut before_char_pos = pos - 1;
        while before_char_pos > 0
            && (self.input.as_bytes()[before_char_pos] >> 6 == 2
                || self.tables.skip[self.input.as_bytes()[before_char_pos] as usize])
        {
            self.ctx.scan_step()?;
            before_char_pos -= 1;
        }
        self.ctx.scan((pos - before_char_pos).min(4))?;
        Ok(match self.input[before_char_pos..pos].chars().next() {
            Some(x) => {
                if (x as usize) < 256 && self.tables.skip[x as usize] {
                    ('\n', None)
                } else {
                    (x, Some(before_char_pos))
                }
            }
            None => ('\n', None),
        })
    }

    fn scan_delims(&mut self, b: u8) -> Result<(usize, bool, bool), H::Error> {
        let result = {
            let (before_char, before_char_pos) = self.get_before_char(self.scanner.pos)?;

            let mut numdelims = 0;
            if b == b'\'' || b == b'"' {
                numdelims += 1;
                self.scanner.pos += 1;
            } else {
                while self.peek_byte() == Some(b) {
                    self.ctx.scan_step()?;
                    numdelims += 1;
                    self.scanner.pos += 1;
                }
            }

            // Flanking reads the real adjacent leaf scalar. Gap EOF only
            // limits consumption and must never synthesize a newline here.
            let after_char = if self.scanner.pos >= self.input.len() {
                '\n'
            } else {
                let mut after_char_pos = self.scanner.pos;
                while after_char_pos < self.input.len() - 1
                    && self.tables.skip[self.input.as_bytes()[after_char_pos] as usize]
                {
                    self.ctx.scan_step()?;
                    after_char_pos += 1;
                }
                self.ctx.scan((self.input.len() - after_char_pos).min(4))?;
                match self.input[after_char_pos..].chars().next() {
                    Some(x) => {
                        if (x as usize) < 256 && self.tables.skip[x as usize] {
                            '\n'
                        } else {
                            x
                        }
                    }
                    None => '\n',
                }
            };

            let cjk_friendly = self.options.extension.cjk_friendly_emphasis;
            let mut two_before_char: Option<char> = None;

            let left_flanking = numdelims > 0
                && !after_char.is_whitespace()
                && (!after_char.is_cmark_punctuation()
                    || (self.options.extension.superscript && b == b'^')
                    || (self.options.extension.subscript && b == b'~')
                    || before_char.is_whitespace()
                    || if !cjk_friendly {
                        before_char.is_cmark_punctuation()
                    } else {
                        after_char.is_cjk()
                            || if before_char.is_non_emoji_general_purpose_vs() {
                                if let Some(before_char_pos) = before_char_pos {
                                    let (two_before_char_, _) =
                                        self.get_before_char(before_char_pos)?;
                                    two_before_char = Some(two_before_char_);
                                    two_before_char_.is_cjk()
                                        || two_before_char_.is_cmark_punctuation()
                                        || two_before_char_.is_cjk_ambiguous_punctuation_candidate()
                                            && before_char == '\u{fe01}'
                                } else {
                                    false
                                }
                            } else {
                                before_char.is_cjk_or_ideographic_vs()
                                    || before_char.is_cmark_punctuation()
                            }
                    });
            let right_flanking = numdelims > 0
                && !before_char.is_whitespace()
                && (!if !cjk_friendly {
                    before_char.is_cmark_punctuation()
                } else {
                    !after_char.is_cjk()
                        && if before_char.is_non_emoji_general_purpose_vs() {
                            let two_before_char = if let Some(two_before_char_) = two_before_char {
                                two_before_char_
                            } else if let Some(before_char_pos) = before_char_pos {
                                let (two_before_char_, _) =
                                    self.get_before_char(before_char_pos)?;
                                two_before_char = Some(two_before_char_);
                                two_before_char_
                            } else {
                                '\n'
                            };
                            !two_before_char.is_cjk()
                                && two_before_char.is_cmark_punctuation()
                                && !(two_before_char.is_cjk_ambiguous_punctuation_candidate()
                                    && before_char == '\u{fe01}')
                        } else {
                            !before_char.is_cjk() && before_char.is_cmark_punctuation()
                        }
                } || after_char.is_whitespace()
                    || after_char.is_cmark_punctuation());

            if b == b'_' {
                (
                    numdelims,
                    left_flanking
                        && (!right_flanking
                            || if !(cjk_friendly && before_char.is_non_emoji_general_purpose_vs()) {
                                before_char.is_cmark_punctuation()
                            } else {
                                let two_before_char =
                                    if let Some(two_before_char_) = two_before_char {
                                        two_before_char_
                                    } else if let Some(before_char_pos) = before_char_pos {
                                        self.get_before_char(before_char_pos)?.0
                                    } else {
                                        '\n'
                                    };
                                two_before_char.is_cmark_punctuation()
                            }),
                    right_flanking && (!left_flanking || after_char.is_cmark_punctuation()),
                )
            } else if b == b'\'' || b == b'"' {
                (
                    numdelims,
                    left_flanking
                        && (!right_flanking || before_char == '(' || before_char == '[')
                        && before_char != ']'
                        && before_char != ')',
                    right_flanking,
                )
            } else {
                (numdelims, left_flanking, right_flanking)
            }
        };
        Ok(result)
    }

    /////////////
    // Utility //
    /////////////

    fn adjust_node_newlines(
        &mut self,
        node: Node<'a>,
        matchlen: usize,
        extra: usize,
        parent_line_offsets: &[usize],
    ) -> Result<(), H::Error> {
        {
            let (newlines, since_newline) = strings::count_newlines_with_host(
                &self.input[self.scanner.pos - matchlen - extra..self.scanner.pos - extra],
                self.ctx,
            )?;

            if newlines > 0 {
                self.line += newlines;
                let node_ast = &mut node.data_mut();
                node_ast.sourcepos.end.line += newlines;
                // `parent_line_offsets` is indexed from the block's first line.
                let adjusted_line = self.line - self.block_line;
                node_ast.sourcepos.end.column =
                    parent_line_offsets[adjusted_line] + since_newline + extra;
                self.column_offset =
                    -(self.scanner.pos as isize) + since_newline as isize + extra as isize;
            }
        };
        Ok(())
    }

    #[cfg(feature = "attributes")]
    fn handle_potential_attribute(
        &mut self,
        node: Node<'a>,
        parent_line_offsets: &[usize],
    ) -> Result<(), H::Error> {
        let result = 'native: {
            if self.peek_byte() != Some(b'{') {
                break 'native;
            }

            let Some((attrs, i)) =
                attributes::parse(&self.input[self.scanner.pos..self.input_end()])
            else {
                break 'native;
            };

            let mut ast = node.data_mut();
            let (lines, last_line) = strings::count_newlines_with_host(
                &self.input[self.scanner.pos..self.input_end()][..i],
                self.ctx,
            )?;
            if lines == 0 {
                ast.sourcepos.end.column += last_line;
            } else {
                ast.sourcepos.end.line += lines;
                // XXX I really freestyled this next line.
                ast.sourcepos.end.column = parent_line_offsets
                    [ast.sourcepos.end.line - ast.sourcepos.start.line]
                    + last_line;
            }
            ast.attrs = Some(Box::new(attrs));

            self.scanner.pos += i;
        };
        Ok(result)
    }
}

#[derive(Debug)]
pub(crate) enum ReferenceEntry {
    Compatibility(ResolvedReference),
    Definition(native::DefinitionId),
}

#[derive(Debug)]
enum RefIndexKey {
    Owned(String),
    Definition(native::DefinitionId),
}

/// Bounded, fallible label lookup. The caller resolves duplicates before insertion.
#[derive(Debug)]
pub(crate) struct ReferenceTable<T> {
    slots: Vec<Option<(u64, RefIndexKey, T)>>,
    entries: usize,
    max_label_bytes: usize,
}
impl<T> Default for ReferenceTable<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            entries: 0,
            max_label_bytes: 0,
        }
    }
}

impl<T> ReferenceTable<T> {
    fn new() -> Self {
        Self::default()
    }
    fn hash<H: Host>(label: &str, ctx: &mut Context<'_, H>) -> Result<u64, H::Error> {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in label.bytes() {
            ctx.scan_step()?;
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        ctx.flush_scan()?;
        Ok(hash)
    }
    fn get<H: Host>(&self, label: &str, ctx: &mut Context<'_, H>) -> Result<Option<&T>, H::Error> {
        if self.slots.is_empty() {
            return Ok(None);
        }
        let hash = Self::hash(label, ctx)?;
        let mut slot = hash as usize & (self.slots.len() - 1);
        loop {
            ctx.event()?;
            match &self.slots[slot] {
                None => return Ok(None),
                Some((entry_hash, entry_label, reference)) => {
                    if *entry_hash == hash {
                        let equals = |key: &str, ctx: &mut Context<'_, H>| {
                            if key.len() != label.len() {
                                return Ok(false);
                            }
                            ctx.scan(label.len())?;
                            Ok(key == label)
                        };
                        let matches = match entry_label {
                            RefIndexKey::Owned(key) => equals(key, ctx)?,
                            RefIndexKey::Definition(id) => ctx.with_definition_key(*id, equals)?,
                        };
                        if matches {
                            return Ok(Some(reference));
                        }
                    }
                }
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }
    fn probe_label_can_match<H: Host>(
        &self,
        label: &str,
        ctx: &mut Context<'_, H>,
    ) -> Result<bool, H::Error> {
        let mut preserved = 0;
        for &byte in label.as_bytes() {
            ctx.scan_step()?;
            // ASCII graphic bytes survive whitespace folding and case
            // folding with one byte each. Stop before building a label
            // longer than every available definition.
            if byte.is_ascii_graphic() {
                preserved += 1;
                if preserved > self.max_label_bytes {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn insert_winner<H: Host>(
        &mut self,
        label: RefIndexKey,
        label_bytes: usize,
        value: T,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        if self.entries >= self.slots.len() / 2 {
            let capacity = self
                .slots
                .len()
                .max(8)
                .checked_mul(2)
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            let mut slots = Vec::new();
            ctx.grow_vec(&mut slots, capacity)?;
            for _ in 0..capacity {
                ctx.push(&mut slots, None)?;
            }
            for previous in &mut self.slots {
                ctx.event()?;
                let Some(entry) = previous.take() else {
                    continue;
                };
                ctx.host.work(native::WorkDelta::Copy(std::mem::size_of::<
                    Option<(u64, RefIndexKey, T)>,
                >() as u64))?;
                let mut slot = entry.0 as usize & (capacity - 1);
                while slots[slot].is_some() {
                    ctx.event()?;
                    slot = (slot + 1) & (capacity - 1);
                }
                slots[slot] = Some(entry);
            }
            let previous = mem::replace(&mut self.slots, slots);
            ctx.discard_vec(previous)?;
        }
        let hash = match &label {
            RefIndexKey::Owned(label) => Self::hash(label, ctx)?,
            RefIndexKey::Definition(id) => ctx.with_definition_key(*id, Self::hash)?,
        };
        let mut slot = hash as usize & (self.slots.len() - 1);
        while self.slots[slot].is_some() {
            ctx.event()?;
            slot = (slot + 1) & (self.slots.len() - 1);
        }
        self.slots[slot] = Some((hash, label, value));
        self.entries += 1;
        self.max_label_bytes = self.max_label_bytes.max(label_bytes);
        Ok(())
    }
}

impl ReferenceTable<ReferenceEntry> {
    pub(crate) fn insert<H: Host>(
        &mut self,
        label: String,
        reference: ResolvedReference,
        ctx: &mut Context<'_, H>,
    ) -> Result<bool, H::Error> {
        let label_bytes = label.len();
        let duplicate = self.get(&label, ctx)?.is_some();
        let (label, reference) = if ctx.direct {
            // The accepted definition keeps the sole normalized key owner.
            let id = ctx.preserve_definition(label, reference, !duplicate)?;
            (RefIndexKey::Definition(id), ReferenceEntry::Definition(id))
        } else {
            (
                RefIndexKey::Owned(label),
                ReferenceEntry::Compatibility(reference),
            )
        };
        if duplicate {
            if let RefIndexKey::Owned(label) = label {
                ctx.discard_string(label)?;
            }
            if let ReferenceEntry::Compatibility(reference) = reference {
                ctx.discard_value(reference.url)?;
                ctx.discard_value(reference.title)?;
            }
            return Ok(false);
        }
        self.insert_winner(label, label_bytes, reference, ctx)?;
        Ok(true)
    }
}

pub struct RefMap {
    pub(crate) map: ReferenceTable<ReferenceEntry>,
    footnotes: ReferenceTable<native::NativeNodeId>,
    pub(crate) max_ref_size: usize,
    ref_size: Cell<usize>,
}

impl RefMap {
    pub fn new() -> Self {
        Self {
            map: ReferenceTable::new(),
            footnotes: ReferenceTable::new(),
            max_ref_size: usize::MAX,
            ref_size: Cell::new(0),
        }
    }

    pub(crate) fn release_owned<H: Host>(
        &mut self,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        let mut table = mem::take(&mut self.map);
        for slot in &mut table.slots {
            ctx.event()?;
            if let Some((_, label, reference)) = slot.take() {
                if let RefIndexKey::Owned(label) = label {
                    ctx.discard_string(label)?;
                }
                if let ReferenceEntry::Compatibility(reference) = reference {
                    ctx.discard_value(reference.url)?;
                    ctx.discard_value(reference.title)?;
                }
            }
        }
        ctx.discard_vec(table.slots)?;
        let mut footnotes = mem::take(&mut self.footnotes);
        for slot in &mut footnotes.slots {
            ctx.event()?;
            if let Some((_, RefIndexKey::Owned(label), _)) = slot.take() {
                ctx.discard_string(label)?;
            }
        }
        ctx.discard_vec(footnotes.slots)
    }

    pub(super) fn insert_footnote<H: Host>(
        &mut self,
        label: &str,
        definition: native::NativeNodeId,
        ctx: &mut Context<'_, H>,
    ) -> Result<bool, H::Error> {
        let normalized = strings::normalize_label_with_host(label, Case::Fold, ctx)?;
        if self.footnotes.get(&normalized, ctx)?.is_some() {
            ctx.discard_string(normalized)?;
            return Ok(false);
        }
        let bytes = normalized.len();
        self.footnotes
            .insert_winner(RefIndexKey::Owned(normalized), bytes, definition, ctx)?;
        Ok(true)
    }

    fn lookup_footnote<H: Host>(
        &self,
        label: &str,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<native::NativeNodeId>, H::Error> {
        if self.footnotes.entries == 0
            || label.is_empty()
            || !self.footnotes.probe_label_can_match(label, ctx)?
        {
            return Ok(None);
        }
        if ctx
            .find(label.as_bytes(), |bytes| {
                bytes.iter().copied().position(native::footnote_label_break)
            })?
            .is_some()
        {
            return Ok(None);
        }
        if !ctx.is_boundary_probe() {
            ctx.host.admit(NativeAdmission::ReferenceLookup(1))?;
        }
        let normalized = strings::normalize_label_with_host(label, Case::Fold, ctx)?;
        let matched = self.footnotes.get(&normalized, ctx)?.copied();
        ctx.discard_string(normalized)?;
        Ok(matched)
    }

    fn lookup_winner<H: Host>(
        &self,
        lab: &str,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<&ReferenceEntry>, H::Error> {
        ctx.event()?;
        if ctx.is_boundary_probe() {
            if self.map.entries == 0 {
                return Ok(None);
            }
            // This is a lexical check. The table still charges hash and
            // comparison work, and committing reference uses are admitted.
        } else {
            ctx.host.admit(NativeAdmission::ReferenceLookup(1))?;
        }
        self.map.get(lab, ctx)
    }

    fn lookup<H: Host>(
        &self,
        lab: &str,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<Cow<'_, ResolvedReference>>, H::Error> {
        Ok(match self.lookup_winner(lab, ctx)? {
            Some(entry) => {
                if ctx.is_shadow() {
                    let ReferenceEntry::Definition(id) = entry else {
                        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
                    };
                    let native = ctx.shadow_winner(*id)?;
                    return Ok(Some(Cow::Owned(ResolvedReference {
                        url: ctx.empty_value(),
                        title: ctx.empty_value(),
                        native: Some(native),
                    })));
                }
                let size = match entry {
                    ReferenceEntry::Compatibility(entry) => {
                        let url = ctx.value_len(&entry.url)?;
                        let title = ctx.value_len(&entry.title)?;
                        url.checked_add(title)
                            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?
                    }
                    ReferenceEntry::Definition(id) => ctx.definition_bytes(*id)?,
                };
                if ctx.capture {
                    ctx.host
                        .admit(NativeAdmission::ReferenceBytes(size as u64))?;
                    return Ok(Some(match entry {
                        ReferenceEntry::Compatibility(entry) => Cow::Borrowed(entry),
                        ReferenceEntry::Definition(id) => {
                            Cow::Owned(ctx.materialize_definition(*id)?)
                        }
                    }));
                }
                let ref_size = self.ref_size.get();
                if size > self.max_ref_size - ref_size {
                    None
                } else {
                    self.ref_size.set(ref_size + size);
                    match entry {
                        ReferenceEntry::Compatibility(entry) => Some(Cow::Borrowed(entry)),
                        ReferenceEntry::Definition(_) => {
                            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
                        }
                    }
                }
            }
            None => None,
        })
    }
}

pub struct FootnoteDefs<'a> {
    defs: Vec<Node<'a>>,
    native_defs: Vec<Node<'a>>,
    next: usize,
}

impl<'a> FootnoteDefs<'a> {
    pub fn new() -> Self {
        Self {
            defs: Vec::new(),
            native_defs: Vec::new(),
            next: 1,
        }
    }

    pub fn next_name(&mut self) -> String {
        let counter = self.next;
        self.next += 1;
        format!("__inline_{}", counter)
    }

    pub fn add_definition(&mut self, def: Node<'a>) {
        self.defs.push(def);
    }

    pub fn take(&mut self) -> Vec<Node<'a>> {
        mem::take(&mut self.defs)
    }

    pub(crate) fn append_native<H: Host>(
        &mut self,
        root: Node<'a>,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        let defs = mem::take(&mut self.native_defs);
        for &definition in &defs {
            ctx.event()?;
            root.append(definition);
        }
        ctx.discard_vec(defs)
    }
}

pub struct Delimiter<'a: 'd, 'd> {
    inl: Node<'a>,
    position: usize,
    length: usize,
    delim_byte: u8,
    can_open: bool,
    can_close: bool,
    prev: Cell<Option<&'d Delimiter<'a, 'd>>>,
    next: Cell<Option<&'d Delimiter<'a, 'd>>>,
}

impl<'d> PartialEq for &'d Delimiter<'_, 'd> {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(*self, *other)
    }
}

impl<'a: 'd, 'd> std::fmt::Debug for Delimiter<'a, 'd> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[pos {}, len {}, delim_char {:?}, open? {} close? {} -- {}]",
            self.position,
            self.length,
            self.delim_byte,
            self.can_open,
            self.can_close,
            self.inl
                .try_data()
                .map_or("<couldn't borrow>".to_string(), |d| format!(
                    "{}",
                    d.sourcepos
                ))
        )
    }
}

#[derive(Clone, Copy)]
struct ObservedImage {
    open: usize,
    close: usize,
}

#[derive(Clone, Copy)]
struct ImageProbeKey {
    row: usize,
    bit: u16,
}

struct ImageProbeRow {
    position: usize,
    known: u16,
    accepted: u16,
}

struct ProbeImageTokens {
    structural: Vec<usize>,
    closers: Vec<usize>,
}

impl ProbeImageTokens {
    fn build<H: Host>(
        input: &str,
        start: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Self, H::Error> {
        let bytes = input.as_bytes();
        let mut structural = Vec::new();
        let mut closers = Vec::new();
        // Direct profiles disable smart punctuation, footnotes, relaxed
        // autolinks and the empty-link check. None of their remaining text
        // or delimiter nodes can change the bracket resolver.
        for at in start..bytes.len() {
            ctx.scan_step()?;
            let byte = bytes[at];
            if matches!(byte, b'[' | b']' | b'<' | b'\\' | b'`' | b'$')
                || byte == b'!' && bytes.get(at + 1) == Some(&b'[')
            {
                ctx.push(&mut structural, at)?;
                if byte == b']' {
                    ctx.push(&mut closers, at)?;
                }
            }
        }
        Ok(Self {
            structural,
            closers,
        })
    }

    fn first<H: Host>(
        positions: &[usize],
        start: usize,
        limit: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<usize>, H::Error> {
        let offset = HtmlCodePairs::lower_bound(positions.len(), start, |i| positions[i], ctx)?;
        Ok(positions
            .get(offset)
            .copied()
            .filter(|&position| position < limit))
    }
}

struct ImageProbeMemo(Vec<ImageProbeRow>);

impl ImageProbeMemo {
    fn new<H: Host>(input: &str, start: usize, ctx: &mut Context<'_, H>) -> Result<Self, H::Error> {
        let mut rows = Vec::new();
        let mut at = start - 2;
        let mut bang = false;
        while let Some(offset) = ctx.find(&input.as_bytes()[at..], |bytes| {
            bytes.iter().position(|&byte| {
                let opener = bang && byte == b'[';
                bang = byte == b'!';
                opener
            })
        })? {
            at += offset + 1;
            ctx.push(
                &mut rows,
                ImageProbeRow {
                    position: at,
                    known: 0,
                    accepted: 0,
                },
            )?;
        }
        Ok(Self(rows))
    }

    fn key<H: Host>(
        &self,
        position: usize,
        flags: HtmlSkipFlags,
        ctx: &mut Context<'_, H>,
    ) -> Result<ImageProbeKey, H::Error> {
        let row = HtmlCodePairs::lower_bound(self.0.len(), position, |i| self.0[i].position, ctx)?;
        if self.0.get(row).is_none_or(|item| item.position != position) {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let state = u8::from(flags.cdata)
            | (u8::from(flags.declaration) << 1)
            | (u8::from(flags.pi) << 2)
            | (u8::from(flags.comment) << 3);
        Ok(ImageProbeKey {
            row,
            bit: 1 << state,
        })
    }

    fn lookup<H: Host>(
        &self,
        position: usize,
        flags: HtmlSkipFlags,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<bool>, H::Error> {
        let key = self.key(position, flags, ctx)?;
        let row = &self.0[key.row];
        Ok((row.known & key.bit != 0).then_some(row.accepted & key.bit != 0))
    }

    fn record<H: Host>(
        &mut self,
        key: ImageProbeKey,
        accepted: bool,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        ctx.host
            .work(native::WorkDelta::Copy(std::mem::size_of::<u32>() as u64))?;
        let row = &mut self.0[key.row];
        row.known |= key.bit;
        if accepted {
            row.accepted |= key.bit;
        } else {
            row.accepted &= !key.bit;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Bracket<'a> {
    inl_text: Node<'a>,
    position: usize,
    image: bool,
    bracket_after: bool,
    probe_key: Option<ImageProbeKey>,
    inside_image: bool,
    html_scope: HtmlScope,
    inherited_html_opacity: bool,
}

#[derive(Clone)]
struct WikilinkComponents {
    url: String,
    link_label: Option<(String, usize, usize)>,
}

pub(crate) fn manual_scan_link_url_with_host<'i, H: Host>(
    input: &'i str,
    ctx: &mut Context<'_, H>,
) -> Result<Option<(&'i str, usize)>, H::Error> {
    let result = 'native: {
        let bytes = input.as_bytes();
        let len = input.len();
        let mut i = 0;
        let mut closed = false;

        if i < len && bytes[i] == b'<' {
            i += 1;
            while i < len {
                ctx.scan_step()?;
                let b = bytes[i];
                if b == b'>' {
                    i += 1;
                    closed = true;
                    break;
                } else if b == b'\\' {
                    i += 2;
                } else if b == b'\n' || b == b'\r' || b == b'<' {
                    break 'native None;
                } else {
                    i += 1;
                }
            }
        } else {
            break 'native manual_scan_link_url_2_with_host(input, ctx)?;
        }

        if !closed {
            None
        } else {
            Some((&input[1..i - 1], i))
        }
    };
    ctx.flush_scan()?;
    Ok(result)
}

pub(crate) fn manual_scan_link_url_2_with_host<'i, H: Host>(
    input: &'i str,
    ctx: &mut Context<'_, H>,
) -> Result<Option<(&'i str, usize)>, H::Error> {
    let result = 'native: {
        let bytes = input.as_bytes();
        let len = input.len();
        let mut i = 0;
        let mut nb_p = 0;

        while i < len {
            ctx.scan_step()?;
            if bytes[i] == b'\\' && i + 1 < len && ispunct(bytes[i + 1]) {
                i += 2;
            } else if bytes[i] == b'(' {
                nb_p += 1;
                i += 1;
                if nb_p > 32 {
                    break 'native None;
                }
            } else if bytes[i] == b')' {
                if nb_p == 0 {
                    break;
                }
                nb_p -= 1;
                i += 1;
            } else if isspace(bytes[i]) || (bytes[i].is_ascii_control() && bytes[i] != 0) {
                if i == 0 {
                    break 'native None;
                }
                break;
            } else {
                i += 1;
            }
        }

        if len == 0 || nb_p != 0 {
            None
        } else {
            Some((&input[..i], i))
        }
    };
    ctx.flush_scan()?;
    Ok(result)
}

pub(crate) fn make_inline<'a, H: Host>(
    ctx: &mut Context<'_, H>,
    arena: &'a native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
    value: NodeValue,
    sourcepos: Sourcepos,
) -> Result<Node<'a>, H::Error> {
    let native_id = if ctx.capture {
        let kind = native::kind(&value)
            .ok_or_else(|| ctx.host.reject(NativeViolation::UnsupportedConfiguration))?;
        Some(ctx.node(None, kind)?)
    } else {
        None
    };
    let ast = Ast {
        value,
        sourcepos,
        #[cfg(feature = "attributes")]
        attrs: None,
        native_id,
        native_callout_title: false,
        open: false,
        last_line_blank: false,
        table_visited: false,
        lines: None,
    };
    arena.try_alloc(ast.into(), ctx)
}

#[derive(Default)]
pub struct Scanner {
    pub pos: usize,
}

impl Scanner {
    pub fn new() -> Self {
        Default::default()
    }

    #[inline]
    fn eof(&self, input: &str) -> bool {
        self.pos >= input.len()
    }

    #[inline]
    pub fn peek_byte(&self, input: &str) -> Option<u8> {
        self.peek_byte_n(input, 0)
    }

    #[inline]
    fn peek_byte_n(&self, input: &str, n: usize) -> Option<u8> {
        if self.pos + n >= input.len() {
            None
        } else {
            Some(input.as_bytes()[self.pos + n])
        }
    }

    pub fn spnl_with_host<H: Host>(
        &mut self,
        input: &str,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        {
            self.skip_spaces_with_host(input, ctx)?;
            if self.skip_line_end_with_host(input, ctx)? {
                self.skip_spaces_with_host(input, ctx)?;
            }
        };
        ctx.flush_scan()?;
        Ok(())
    }

    pub fn skip_spaces_with_host<H: Host>(
        &mut self,
        input: &str,
        ctx: &mut Context<'_, H>,
    ) -> Result<bool, H::Error> {
        let result = {
            let mut skipped = false;
            while self
                .peek_byte(input)
                .is_some_and(|b| b == b' ' || b == b'\t')
            {
                ctx.scan_step()?;
                self.pos += 1;
                skipped = true;
            }
            skipped
        };
        ctx.flush_scan()?;
        Ok(result)
    }

    pub fn skip_line_end_with_host<H: Host>(
        &mut self,
        input: &str,
        ctx: &mut Context<'_, H>,
    ) -> Result<bool, H::Error> {
        let result = {
            let old_pos = self.pos;
            if self.peek_byte(input) == Some(b'\r') {
                ctx.scan_step()?;
                self.pos += 1;
            }
            if self.peek_byte(input) == Some(b'\n') {
                ctx.scan_step()?;
                self.pos += 1;
            }
            self.pos > old_pos || self.eof(input)
        };
        ctx.flush_scan()?;
        Ok(result)
    }

    fn take_while_with_host<H: Host>(
        &mut self,
        input: &str,
        b: u8,
        ctx: &mut Context<'_, H>,
    ) -> Result<usize, H::Error> {
        let result = {
            let start_pos = self.pos;
            while self.peek_byte(input) == Some(b) {
                ctx.scan_step()?;
                self.pos += 1;
            }
            self.pos - start_pos
        };
        ctx.flush_scan()?;
        Ok(result)
    }

    fn take_while_with_limit_with_host<H: Host>(
        &mut self,
        input: &str,
        b: u8,
        limit: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<usize, H::Error> {
        let result = {
            let start_pos = self.pos;
            let mut count = 0;
            while count < limit && self.peek_byte(input) == Some(b) {
                ctx.scan_step()?;
                self.pos += 1;
                count += 1;
            }
            self.pos - start_pos
        };
        ctx.flush_scan()?;
        Ok(result)
    }

    pub fn link_label_with_host<'i, H: Host>(
        &mut self,
        input: &'i str,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<&'i str>, H::Error> {
        let result = 'native: {
            let startpos = self.pos;

            if self.peek_byte(input) != Some(b'[') {
                break 'native None;
            }
            self.pos += 1;

            let mut length = 0;
            while let Some(b) = self.peek_byte(input) {
                if b == b']' {
                    let raw_label = strings::trim_slice(&input[startpos + 1..self.pos]);
                    ctx.scan_step()?;
                    self.pos += 1;
                    break 'native Some(raw_label);
                }
                if b == b'[' {
                    break;
                }
                if b == b'\\' {
                    ctx.scan_step()?;
                    self.pos += 1;
                    length += 1;
                    if self.peek_byte(input).is_some_and(ispunct) {
                        ctx.scan_step()?;
                        self.pos += 1;
                        length += 1;
                    }
                } else {
                    ctx.scan_step()?;
                    self.pos += 1;
                    length += 1;
                }
                if length > MAX_LINK_LABEL_LENGTH {
                    self.pos = startpos;
                    break 'native None;
                }
            }

            self.pos = startpos;
            None
        };
        ctx.flush_scan()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_code_special_index_matches_the_generated_scanners_at_every_opening() {
        let prefixes = [
            "<!--",
            "<?",
            "<![CDATA[",
            "<![cDaTa[",
            "<!X ",
            "<!XX\t",
            "<!X",
            "<!x ",
        ];
        let alphabet = [b'-', b'?', b'>', b']', b' ', b'<', b'X'];
        let mut host = native::LegacyHost;
        let mut ctx = Context::new(&mut host, 0);
        for prefix in prefixes {
            for n in 0..alphabet.len().pow(5) {
                let mut input = String::from(prefix);
                let mut digits = n;
                for _ in 0..5 {
                    input.push(char::from(alphabet[digits % alphabet.len()]));
                    digits /= alphabet.len();
                }
                input.push_str("<!--x--><?x?><![CDATA[x]]><!Y >");
                let indexed = html_code_special_tokens(input.as_bytes(), 0, &mut ctx).unwrap();
                let mut actual = indexed.iter().rev().copied();
                for (at, _) in input.match_indices('<') {
                    if !matches!(input.as_bytes().get(at + 1), Some(b'!' | b'?')) {
                        continue;
                    }
                    let expected =
                        inline_html_len(&input[at + 1..], &mut HtmlSkipFlags::default(), &mut ctx)
                            .unwrap()
                            .map(|width| (at, at + 1 + width));
                    if let Some(expected) = expected {
                        assert_eq!(actual.next(), Some(expected), "{input:?} at {at}");
                    }
                }
                assert_eq!(actual.next(), None, "{input:?}");
                ctx.discard_vec(indexed).unwrap();
            }
        }
    }

    #[test]
    fn initial_delimiter_walk_faults_before_matching_mutates_the_chain() {
        for cancel in [false, true] {
            let mut host = native::tests::MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                ctx.capture = false;
                let arena = native::FallibleArena::new();
                let delimiter_arena = native::FallibleArena::new();
                let options = Options::default();
                let mut refmap = RefMap::new();
                let mut footnote_defs = FootnoteDefs::new();
                let tables = ByteTables::new(&options);
                let mut subject = Subject::new(
                    &mut ctx,
                    None,
                    0,
                    &arena,
                    &options,
                    String::new(),
                    1,
                    &mut refmap,
                    &tables,
                    &mut footnote_defs,
                    &delimiter_arena,
                    0,
                );
                let text = subject.ctx.string("*").unwrap();
                let inl = subject
                    .make_inline(NodeValue::Text(text.into()), 0, 0)
                    .unwrap();
                for position in 0..300 {
                    subject.scanner.pos = position;
                    subject.push_delimiter(b'*', false, true, inl).unwrap();
                }
                let mut first = subject.last_delimiter.unwrap();
                while let Some(previous) = first.prev.get() {
                    first = previous;
                }
                let second = first.next.get().unwrap();
                subject.ctx.host.arm_chain_fault(cancel);
                let error = subject.process_emphasis(0).unwrap_err();
                // A fault in the later matching loop would already have removed this closer.
                assert!(second.prev.get() == Some(first));
                assert!(first.next.get() == Some(second));
                error
            };
            host.assert_work_fault_released(&error, cancel);
        }
    }

    #[test]
    fn accepted_inline_title_reread_faults_inside_the_bounded_search() {
        for cancel in [false, true] {
            let input = format!("[x](u \"{}\")", "t".repeat(131_072));
            let mut host = native::tests::MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, input.len());
                ctx.capture = false;
                let arena = native::FallibleArena::new();
                let delimiter_arena = native::FallibleArena::new();
                let options = Options::default();
                let mut refmap = RefMap::new();
                let mut footnote_defs = FootnoteDefs::new();
                let tables = ByteTables::new(&options);
                let owned_input = ctx.string(&input).unwrap();
                let mut subject = Subject::new(
                    &mut ctx,
                    None,
                    0,
                    &arena,
                    &options,
                    owned_input,
                    1,
                    &mut refmap,
                    &tables,
                    &mut footnote_defs,
                    &delimiter_arena,
                    0,
                );
                let parent = subject.make_inline(NodeValue::Paragraph, 0, 0).unwrap();
                assert!(subject.parse_inline(parent, 1, &[0]).unwrap());
                assert!(subject.parse_inline(parent, 1, &[0]).unwrap());
                assert_eq!(subject.scanner.pos, 2);
                // DFA work batches are <=256; a 65536 Scan here belongs to this later reread.
                subject.ctx.host.arm_title_reread_fault(cancel);
                let error = subject.handle_close_bracket(&[0]).unwrap_err();
                assert_eq!(subject.scanner.pos, 3);
                assert_eq!(subject.brackets.len(), 1);
                error
            };
            host.assert_work_fault_released(&error, cancel);
        }
    }

    #[test]
    fn latex_math_closing_search_stays_inside_the_actual_gap() {
        for (input, gap_end, expected) in [
            (r"\(a\)", 5, Some(3)),
            (r"\(a\)", 4, None),
            (r"\(a\)", 3, None),
            (r"\(a\)", 2, None),
            (r"\(a\", 4, None),
            (r"\(a\x\)", 7, Some(5)),
            (r"\(\(\(x\)", 9, Some(7)),
        ] {
            let mut host = native::tests::MemoryHost::new();
            {
                let mut ctx = Context::new(&mut host, input.len());
                ctx.capture = false;
                let arena = native::FallibleArena::new();
                let delimiters = native::FallibleArena::new();
                let options = Options::default();
                let mut refs = RefMap::new();
                let mut footnotes = FootnoteDefs::new();
                let tables = ByteTables::new(&options);
                let input = ctx.string(input).unwrap();
                let mut subject = Subject::new(
                    &mut ctx,
                    None,
                    0,
                    &arena,
                    &options,
                    input,
                    1,
                    &mut refs,
                    &tables,
                    &mut footnotes,
                    &delimiters,
                    0,
                );
                subject.begin_gap(0, gap_end).unwrap();
                let before = subject.ctx.host.observed_scan_bytes();
                assert_eq!(
                    subject.scan_to_closing_latex_math(2, b')').unwrap(),
                    expected
                );
                assert!(
                    subject.ctx.host.observed_scan_bytes() - before <= (gap_end - 2 + 2) as u64
                );
            }
        }
    }

    #[test]
    fn latex_math_long_failed_and_repeated_openers_charge_actual_search_work() {
        for input in [format!(r"\({}", "x".repeat(131_072)), r"\(".repeat(128)] {
            let mut host = native::tests::MemoryHost::new();
            {
                let mut ctx = Context::new(&mut host, input.len());
                ctx.capture = false;
                let arena = native::FallibleArena::new();
                let delimiters = native::FallibleArena::new();
                let options = Options::default();
                let mut refs = RefMap::new();
                let mut footnotes = FootnoteDefs::new();
                let tables = ByteTables::new(&options);
                let owned = ctx.string(&input).unwrap();
                let mut subject = Subject::new(
                    &mut ctx,
                    None,
                    0,
                    &arena,
                    &options,
                    owned,
                    1,
                    &mut refs,
                    &tables,
                    &mut footnotes,
                    &delimiters,
                    0,
                );
                let before = subject.ctx.host.observed_scan_bytes();
                let mut minimum = 0;
                let starts: Vec<_> = if input.len() > 1_000 {
                    vec![2]
                } else {
                    (2..=input.len()).step_by(2).collect()
                };
                for start in starts {
                    assert_eq!(
                        subject.scan_to_closing_latex_math(start, b')').unwrap(),
                        None
                    );
                    minimum += input.len() - start;
                }
                assert!(subject.ctx.host.observed_scan_bytes() - before >= minimum as u64);
            }
        }
    }

    #[test]
    fn latex_math_success_and_failure_search_propagate_scan_fault_and_cancel() {
        for cancel in [false, true] {
            for closed in [false, true] {
                let input = format!(
                    r"\({}{}",
                    "x".repeat(131_072),
                    if closed { r"\)" } else { "" }
                );
                let mut host = native::tests::MemoryHost::new();
                let error = {
                    let mut ctx = Context::new(&mut host, input.len());
                    ctx.capture = false;
                    let arena = native::FallibleArena::new();
                    let delimiters = native::FallibleArena::new();
                    let options = Options::default();
                    let mut refs = RefMap::new();
                    let mut footnotes = FootnoteDefs::new();
                    let tables = ByteTables::new(&options);
                    let owned = ctx.string(&input).unwrap();
                    let mut subject = Subject::new(
                        &mut ctx,
                        None,
                        0,
                        &arena,
                        &options,
                        owned,
                        1,
                        &mut refs,
                        &tables,
                        &mut footnotes,
                        &delimiters,
                        0,
                    );
                    subject.ctx.host.arm_latex_scan_fault(cancel);
                    subject.scan_to_closing_latex_math(2, b')').unwrap_err()
                };
                host.assert_work_fault_released(&error, cancel);
            }
        }
    }

    #[test]
    fn closing_backticks_cache_skipped_runs() {
        let arena = Arena::new();
        let options = Options::default();
        let mut refmap = RefMap::new();
        let mut footnote_defs = FootnoteDefs::new();
        let delimiter_arena = native::FallibleArena::new();
        let tables = ByteTables::new(&options);
        let input = format!("`{} ``tail`", "語".repeat(32));
        let double_backtick_pos = input.find("``").unwrap();
        let mut host = native::LegacyHost;
        let mut ctx = Context::new(&mut host, input.len());
        ctx.capture = false;
        let mut subject = Subject::new(
            &mut ctx,
            None,
            0,
            &arena,
            &options,
            input,
            1,
            &mut refmap,
            &tables,
            &mut footnote_defs,
            &delimiter_arena,
            0,
        );
        subject.scanner.pos = 1;

        assert_eq!(
            subject.scan_to_closing_backtick(1).unwrap(),
            Some(subject.input.len())
        );
        assert_eq!(subject.backticks[2], double_backtick_pos);
        assert!(!subject.scanned_for_backticks);
    }

    #[test]
    fn closing_backticks_reuse_cached_runs_after_failed_searches() {
        let arena = Arena::new();
        let options = Options::default();
        let mut refmap = RefMap::new();
        let mut footnote_defs = FootnoteDefs::new();
        let delimiter_arena = native::FallibleArena::new();
        let tables = ByteTables::new(&options);
        let input = format!("`{} ``tail", "語".repeat(32));
        let double_backtick_pos = input.find("``").unwrap();
        let mut host = native::LegacyHost;
        let mut ctx = Context::new(&mut host, input.len());
        ctx.capture = false;
        let mut subject = Subject::new(
            &mut ctx,
            None,
            0,
            &arena,
            &options,
            input,
            1,
            &mut refmap,
            &tables,
            &mut footnote_defs,
            &delimiter_arena,
            0,
        );
        subject.scanner.pos = 1;
        assert_eq!(subject.scan_to_closing_backtick(1).unwrap(), None);
        assert_eq!(subject.scanner.pos, subject.input.len());
        assert_eq!(subject.backticks[2], double_backtick_pos);
        assert!(subject.scanned_for_backticks);

        subject.scanner.pos = double_backtick_pos;
        assert_eq!(subject.scan_to_closing_backtick(2).unwrap(), None);
        assert_eq!(subject.scanner.pos, double_backtick_pos);

        subject.scanner.pos = 1;
        assert_eq!(
            subject.scan_to_closing_backtick(2).unwrap(),
            Some(double_backtick_pos + 2)
        );
        assert_eq!(subject.scanner.pos, double_backtick_pos + 2);
        assert!(subject.scanned_for_backticks);

        subject.scanner.pos = 1;
        assert_eq!(
            subject.scan_to_closing_backtick(MAXBACKTICKS + 1).unwrap(),
            None
        );
        assert_eq!(subject.scanner.pos, 1);
    }
}
