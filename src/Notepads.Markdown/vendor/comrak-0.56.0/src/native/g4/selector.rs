// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! B only: a lazy fixed host summary and a monotone accepted-owner sweep.
//! Neither lookahead nor these numeric rows grants source authority.

use super::super::{
    Context, Host, NativeAdmission, NativeViolation, WorkDelta,
    rows::{NativeRows, charge_host},
};
use super::{
    LeafBrand, OwnerKind, SelectedKind, SelectedLink, Selection, ShadowOwner, ShadowReleased,
};

fn add<H: Host>(a: usize, b: usize, ctx: &mut Context<'_, H>) -> Result<usize, H::Error> {
    a.checked_add(b)
        .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))
}

fn sub<H: Host>(a: usize, b: usize, ctx: &mut Context<'_, H>) -> Result<usize, H::Error> {
    a.checked_sub(b)
        .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))
}

fn scalar<H: Host>(raw: &str, at: usize, ctx: &mut Context<'_, H>) -> Result<char, H::Error> {
    ctx.event()?;
    let c = raw
        .get(at..)
        .and_then(|s| s.chars().next())
        .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
    for _ in 0..c.len_utf8() {
        ctx.scan_step()?;
    }
    Ok(c)
}

fn previous<H: Host>(raw: &str, at: usize, ctx: &mut Context<'_, H>) -> Result<char, H::Error> {
    ctx.event()?;
    let c = raw
        .get(..at)
        .and_then(|s| s.chars().next_back())
        .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
    for _ in 0..c.len_utf8() {
        ctx.scan_step()?;
    }
    Ok(c)
}

fn boundary<H: Host>(raw: &str, at: usize, ctx: &mut Context<'_, H>) -> Result<(), H::Error> {
    ctx.event()?;
    if at < raw.len() {
        ctx.scan_step()?;
    }
    if at > raw.len() || !raw.is_char_boundary(at) {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    Ok(())
}

fn whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ')
}

fn host_char<H: Host>(c: char, ctx: &mut Context<'_, H>) -> Result<bool, H::Error> {
    ctx.event()?;
    // The pinned toolchain's Unicode 17 gate is checked by the root producer.
    Ok(matches!(c, '-' | '_') || c.is_alphanumeric())
}

fn prefix<H: Host>(
    raw: &str,
    p: usize,
    at_delimited_body_start: bool,
    ctx: &mut Context<'_, H>,
) -> Result<Option<(SelectedKind, usize)>, H::Error> {
    if p != 0 && !at_delimited_body_start {
        let before = previous(raw, p, ctx)?;
        ctx.event()?;
        if !whitespace(before) && !matches!(before, '*' | '_' | '~' | '(') {
            return Ok(None);
        }
    }
    let first = scalar(raw, p, ctx)?;
    let patterns: &[(&[u8], SelectedKind)] = match first {
        'w' => &[(b"www.", SelectedKind::Www)],
        'h' | 'H' => &[
            (b"https://", SelectedKind::Url),
            (b"http://", SelectedKind::Url),
        ],
        'f' | 'F' => &[(b"ftp://", SelectedKind::Url)],
        _ => return Ok(None),
    };
    for &(pattern, kind) in patterns {
        let mut at = p;
        let mut matched = true;
        for &want in pattern {
            ctx.event()?;
            let Some(&actual) = raw.as_bytes().get(at) else {
                matched = false;
                break;
            };
            ctx.scan_step()?;
            if (if kind == SelectedKind::Url {
                actual.to_ascii_lowercase()
            } else {
                actual
            }) != want
            {
                matched = false;
                break;
            }
            at = add(at, 1, ctx)?;
        }
        if matched {
            return Ok(Some((kind, at)));
        }
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug)]
struct HostRun {
    boundaries_valid: bool,
    begin: usize,
    end: usize,
    effective_end: usize,
    last_nonempty: Option<usize>,
    internal_empty: Option<usize>,
    last_two_underscore: bool,
    replay: usize,
    suffix_ordinal: usize,
}

impl HostRun {
    fn summarize<H: Host>(
        raw: &str,
        begin: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Self, H::Error> {
        boundary(raw, begin, ctx)?;
        let mut q = begin;
        let mut ordinal = 0;
        let mut nonempty = false;
        let mut underscore = false;
        let mut pending_empty = None;
        let mut internal_empty = None;
        let mut last_nonempty = None;
        let mut previous_underscore = false;
        let mut last_underscore = false;
        let mut effective_end = begin;
        while q < raw.len() {
            let c = scalar(raw, q, ctx)?;
            let is_host = host_char(c, ctx)?;
            if !is_host && c != '.' {
                break;
            }
            if c == '.' {
                if nonempty {
                    previous_underscore = last_underscore;
                    last_underscore = underscore;
                    last_nonempty = Some(ordinal);
                } else {
                    pending_empty = Some(ordinal);
                }
                ordinal = add(ordinal, 1, ctx)?;
                nonempty = false;
                underscore = false;
            } else {
                internal_empty = pending_empty;
                nonempty = true;
                underscore |= c == '_';
                effective_end = add(q, c.len_utf8(), ctx)?;
            }
            q = add(q, c.len_utf8(), ctx)?;
        }
        if nonempty {
            previous_underscore = last_underscore;
            last_underscore = underscore;
            last_nonempty = Some(ordinal);
        }
        Ok(Self {
            // begin was checked; every subsequent endpoint is advanced by a
            // complete scalar read from this same immutable str.
            boundaries_valid: true,
            begin,
            end: q,
            effective_end,
            last_nonempty,
            internal_empty,
            last_two_underscore: previous_underscore || last_underscore,
            replay: begin,
            suffix_ordinal: 0,
        })
    }

    fn suffix<H: Host>(
        &mut self,
        raw: &str,
        d: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<usize>, H::Error> {
        ctx.event()?;
        if !self.boundaries_valid || d < self.begin || d > self.end || d < self.replay {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        boundary(raw, d, ctx)?;
        if d != self.begin && previous(raw, d, ctx)? != '.' {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        while self.replay < d {
            let c = scalar(raw, self.replay, ctx)?;
            ctx.event()?;
            if c == '.' {
                self.suffix_ordinal = add(self.suffix_ordinal, 1, ctx)?;
            }
            self.replay = add(self.replay, c.len_utf8(), ctx)?;
        }
        if self.replay != d {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        if d >= self.effective_end {
            return Ok(None);
        }
        let first = scalar(raw, d, ctx)?;
        if !host_char(first, ctx)?
            || self
                .internal_empty
                .is_some_and(|n| n >= self.suffix_ordinal)
        {
            return Ok(None);
        }
        let Some(last) = self.last_nonempty else {
            return Ok(None);
        };
        let Some(distance) = last.checked_sub(self.suffix_ordinal) else {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        };
        if distance < 1 || self.last_two_underscore {
            return Ok(None);
        }
        Ok(Some(self.effective_end))
    }
}

fn path_end<H: Host>(
    raw: &str,
    domain_end: usize,
    ctx: &mut Context<'_, H>,
) -> Result<usize, H::Error> {
    let mut frontier = domain_end;
    let mut opens = 0;
    let mut closes = 0;
    while frontier < raw.len() {
        let c = scalar(raw, frontier, ctx)?;
        ctx.event()?;
        if whitespace(c) || c == '<' {
            break;
        }
        if c == '(' {
            opens = add(opens, 1, ctx)?;
        }
        if c == ')' {
            closes = add(closes, 1, ctx)?;
        }
        frontier = add(frontier, c.len_utf8(), ctx)?;
    }
    let mut end = frontier;
    while end > domain_end {
        let c = previous(raw, end, ctx)?;
        ctx.event()?;
        if matches!(c, '?' | '!' | '.' | ',' | ':' | '*' | '_' | '~') {
            end = sub(end, 1, ctx)?;
        } else if c == ')' && closes > opens {
            closes = sub(closes, 1, ctx)?;
            end = sub(end, 1, ctx)?;
        } else if c == ';' {
            let mut j = sub(end, 1, ctx)?;
            let mut name = false;
            let mut ampersand = false;
            while j > domain_end {
                let before = previous(raw, j, ctx)?;
                ctx.event()?;
                if !before.is_ascii_alphanumeric() {
                    ampersand = before == '&';
                    break;
                }
                name = true;
                j = sub(j, 1, ctx)?;
            }
            if name && ampersand && j > domain_end {
                end = sub(j, 1, ctx)?;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    Ok(end)
}

fn same_leaf(a: LeafBrand<'_>, b: LeafBrand<'_>) -> bool {
    a.node == b.node && a.buffer == b.buffer && a.base == b.base && a.bytes == b.bytes
}

fn row<T: Copy, H: Host>(
    rows: &NativeRows<T, H::Lease>,
    index: usize,
    ctx: &mut Context<'_, H>,
) -> Result<T, H::Error> {
    let value = rows
        .get(index, |n| charge_host(ctx.host, n))?
        .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
    ctx.scan(std::mem::size_of::<T>())?;
    ctx.host
        .work(WorkDelta::Copy(std::mem::size_of::<T>() as u64))?;
    Ok(*value)
}

fn append<T: Copy, H: Host>(
    rows: &mut NativeRows<T, H::Lease>,
    value: T,
    ctx: &mut Context<'_, H>,
) -> Result<(), H::Error> {
    ctx.host.admit(NativeAdmission::Record(1))?;
    ctx.host
        .work(WorkDelta::Copy(std::mem::size_of::<T>() as u64))?;
    rows.push(value, ctx.host)
}

#[derive(Clone, Copy)]
struct StackEntry {
    index: usize,
    end: usize,
    subtree_end: usize,
    active: bool,
    excludes_links: bool,
    delimited_body: Option<(usize, usize)>,
}

struct Sweep<'leaf, L> {
    leaf: LeafBrand<'leaf>,
    owners: NativeRows<ShadowOwner<'leaf>, L>,
    pending: Option<ShadowOwner<'leaf>>,
    next: usize,
    stack: NativeRows<StackEntry, L>,
    // Fixed cache: no descriptor/full-row reread for every protected scalar.
    top: Option<StackEntry>,
    depth: usize,
    live: usize,
    previous_opening: Option<(usize, u64)>,
}

impl<'leaf, L> Sweep<'leaf, L> {
    fn new(released: ShadowReleased<'leaf, L>) -> Self {
        Self {
            leaf: released.leaf,
            owners: released.owners,
            pending: None,
            next: 0,
            stack: NativeRows::default(),
            top: None,
            depth: 0,
            live: 0,
            previous_opening: None,
        }
    }

    fn top<H: Host<Lease = L>>(
        &self,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<&StackEntry>, H::Error> {
        ctx.event()?;
        Ok(self.top.as_ref())
    }

    fn close<H: Host<Lease = L>>(
        &mut self,
        position: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        while let Some(top) = self.top(ctx)? {
            if top.end > position {
                break;
            }
            if top.subtree_end != self.next {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            let was_live = top.excludes_links;
            self.depth = sub(self.depth, 1, ctx)?;
            if was_live {
                self.live = self
                    .live
                    .checked_sub(1)
                    .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
            }
            self.top = if self.depth == 0 {
                None
            } else {
                let index = sub(self.depth, 1, ctx)?;
                Some(row(&self.stack, index, ctx)?)
            };
        }
        Ok(())
    }

    fn advance<H: Host<Lease = L>>(
        &mut self,
        raw: &str,
        position: usize,
        selected_start: Option<usize>,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        loop {
            ctx.event()?;
            if self.pending.is_none() && self.next < self.owners.len() {
                self.pending = Some(row(&self.owners, self.next, ctx)?);
            }
            let Some(owner) = self.pending.as_ref() else {
                break;
            };
            if if selected_start.is_some() {
                owner.start >= position
            } else {
                owner.start > position
            } {
                break;
            }
            let owner = self
                .pending
                .take()
                .ok_or_else(|| ctx.host.reject(NativeViolation::InternalInvariant))?;
            self.close(owner.start, ctx)?;
            boundary(raw, owner.start, ctx)?;
            boundary(raw, owner.end, ctx)?;
            let next = add(self.next, 1, ctx)?;
            if !same_leaf(owner.leaf, self.leaf)
                || owner.start >= owner.end
                || owner.subtree_end < next
                || owner.subtree_end > self.owners.len()
                || self
                    .previous_opening
                    .is_some_and(|(start, slot)| owner.start < start || owner.opening_slot <= slot)
            {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            // Formatting is transparent to links, but an already-open pair
            // bounds a link started in its body.
            let delimited_body = match owner.kind {
                OwnerKind::Delimited { marker, width } => {
                    let delimiter: &[u8] = match (marker, width) {
                        (b'=', 2) => b"==",
                        (b'^', 1) => b"^",
                        (b'^', 2) => b"^^",
                        (b'~', 1) => b"~",
                        _ => return Err(ctx.host.reject(NativeViolation::InvalidProvenance)),
                    };
                    let start = add(owner.start, delimiter.len(), ctx)?;
                    let end = sub(owner.end, delimiter.len(), ctx)?;
                    ctx.scan(delimiter.len() * 2)?;
                    if start >= end
                        || raw.as_bytes().get(owner.start..start) != Some(delimiter)
                        || raw.as_bytes().get(end..owner.end) != Some(delimiter)
                    {
                        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
                    }
                    Some((start, end))
                }
                OwnerKind::Code
                | OwnerKind::Math
                | OwnerKind::Footnote
                | OwnerKind::Html
                | OwnerKind::Link
                | OwnerKind::Image => None,
            };
            let parent = self.top(ctx)?;
            if owner.parent != parent.map(|p| p.index)
                || parent.is_some_and(|p| {
                    self.next >= p.subtree_end
                        || owner.subtree_end > p.subtree_end
                        || owner.end > p.end
                })
            {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            let killed =
                selected_start.is_some_and(|start| start < owner.start && owner.start < position);
            let active = !killed && parent.is_none_or(|p| p.active);
            let excludes_links = active && delimited_body.is_none();
            let entry = StackEntry {
                index: self.next,
                end: owner.end,
                subtree_end: owner.subtree_end,
                active,
                excludes_links,
                delimited_body: delimited_body
                    .filter(|_| active)
                    .or_else(|| parent.and_then(|p| p.delimited_body)),
            };
            // Each accepted sweep record is admitted, including reused physical slots.
            if self.depth == self.stack.len() {
                append(&mut self.stack, entry, ctx)?;
            } else {
                ctx.host.admit(NativeAdmission::Record(1))?;
                ctx.host
                    .work(WorkDelta::Copy(std::mem::size_of::<StackEntry>() as u64))?;
                *self
                    .stack
                    .get_mut(self.depth, |n| charge_host(ctx.host, n))?
                    .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))? = entry;
            }
            self.depth = add(self.depth, 1, ctx)?;
            ctx.host
                .work(WorkDelta::Copy(std::mem::size_of::<StackEntry>() as u64))?;
            self.top = Some(entry);
            if excludes_links {
                self.live = add(self.live, 1, ctx)?;
            }
            self.previous_opening = Some((owner.start, owner.opening_slot));
            self.next = next;
            self.pending = None;
        }
        self.close(position, ctx)
    }
}

pub(super) fn select<'leaf, H: Host>(
    raw: &str,
    released: ShadowReleased<'leaf, H::Lease>,
    ctx: &mut Context<'_, H>,
) -> Result<Selection<'leaf, H::Lease>, H::Error> {
    ctx.event()?;
    let leaf = released.leaf;
    if raw.len() != leaf.bytes {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    add(leaf.base, leaf.bytes, ctx)?;
    let mut sweep = Sweep::new(released);
    let mut links = NativeRows::default();
    let mut cache: Option<HostRun> = None;
    let mut p = 0;
    while p < raw.len() {
        sweep.advance(raw, p, None, ctx)?;
        ctx.event()?;
        let delimited = sweep.top.and_then(|entry| entry.delimited_body);
        if sweep.live == 0
            && delimited.is_none_or(|(_, end)| p < end)
            && let Some((kind, d)) =
                prefix(raw, p, delimited.is_some_and(|(start, _)| p == start), ctx)?
        {
            if cache.is_none_or(|frame| d >= frame.end) {
                cache = Some(HostRun::summarize(raw, d, ctx)?);
            }
            let frame = cache
                .as_mut()
                .ok_or_else(|| ctx.host.reject(NativeViolation::InternalInvariant))?;
            if let Some(domain_end) = frame.suffix(raw, d, ctx)? {
                let path_input = &raw[..delimited.map_or(raw.len(), |(_, end)| end)];
                let end = path_end(path_input, domain_end, ctx)?;
                if !(p < d && d < domain_end && domain_end <= end && end <= raw.len()) {
                    return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
                }
                append(
                    &mut links,
                    SelectedLink {
                        leaf,
                        kind,
                        start: p,
                        end,
                        domain_end,
                    },
                    ctx,
                )?;
                // q and path's frontier have no owner effect. Only accepted e does.
                sweep.advance(raw, end, Some(p), ctx)?;
                p = end;
                if p >= frame.end {
                    cache = None;
                }
                continue;
            }
        }
        let c = scalar(raw, p, ctx)?;
        p = add(p, c.len_utf8(), ctx)?;
    }
    sweep.advance(raw, raw.len(), None, ctx)?;
    if sweep.next != sweep.owners.len() || sweep.depth != 0 || sweep.live != 0 {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    ctx.flush_scan()?;
    // The result retains only selected rows. Owner/stack leases are really released.
    drop(sweep);
    Ok(Selection {
        leaf,
        links,
        next: 0,
    })
}

#[cfg(test)]
#[path = "selector_tests.rs"]
mod tests;
