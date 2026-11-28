// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private per-leaf pipeline values. These numeric shadow/selection rows never
//! issue a source capability; only the actual committing native grammar does.
//! The isolated StrictG4 entry wires raw A/B/C and decoded email. General G4
//! admission remains blocked pending complete product qualification.

use super::{BufferId, Context, Host, NativeNodeId, NativeViolation, rows::NativeRows};
use std::marker::PhantomData;

#[cfg(test)]
mod commit_tests;
pub(crate) mod email;
#[cfg(test)]
mod email_commit_tests;
#[cfg(test)]
mod lifecycle_tests;
mod selector;
pub(crate) mod shadow;

type Invariant<'leaf> = PhantomData<fn(&'leaf ()) -> &'leaf ()>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OwnerKind {
    Code,
    Math,
    Footnote,
    Html,
    Link,
    Image,
    Delimited { marker: u8, width: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectedKind {
    Www,
    Url,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtendedKind {
    Www,
    Url,
    Email,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtendedAccepted {
    pub(crate) node: NativeNodeId,
    pub(crate) label: NativeNodeId,
    pub(crate) kind: ExtendedKind,
    pub(crate) raw: super::RSpan,
    pub(crate) destination: super::PayloadId,
}

/// The HRTB scope cannot escape a leaf, including into another session's leaf.
/// Node/buffer values come from the real block parser, not sourcepos searches.
#[derive(Clone, Copy)]
pub(crate) struct LeafBrand<'leaf> {
    node: NativeNodeId,
    buffer: BufferId,
    base: usize,
    bytes: usize,
    invariant: Invariant<'leaf>,
}

pub(crate) fn with_leaf<T>(
    node: NativeNodeId,
    buffer: BufferId,
    base: usize,
    bytes: usize,
    body: impl for<'leaf> FnOnce(LeafBrand<'leaf>) -> T,
) -> T {
    body(LeafBrand {
        node,
        buffer,
        base,
        bytes,
        invariant: PhantomData,
    })
}

/// Rows are in accepted opening order. Parent/subtree_end use that same order.
/// subtree_end is exclusive; a leaf at i ends at i + 1.
/// These values have no AST pointers and outlive the actually freed A arena.
#[derive(Clone, Copy)]
pub(crate) struct ShadowOwner<'leaf> {
    leaf: LeafBrand<'leaf>,
    kind: OwnerKind,
    opening_slot: u64,
    start: usize,
    end: usize,
    parent: Option<usize>,
    subtree_end: usize,
}

/// NativeRows owns its own leases, independently of A's Context registry.
/// The root A driver may construct this only after its child scope has dropped.
pub(crate) struct ShadowReleased<'leaf, L> {
    leaf: LeafBrand<'leaf>,
    owners: NativeRows<ShadowOwner<'leaf>, L>,
}

#[derive(Clone, Copy)]
pub(crate) struct SelectedLink<'leaf> {
    leaf: LeafBrand<'leaf>,
    kind: SelectedKind,
    start: usize,
    end: usize,
    domain_end: usize,
}

impl SelectedLink<'_> {
    /// Numeric transport checked against the actual committing leaf. This
    /// does not mint a SourceToken; the grammar must still accept real facts.
    pub(crate) fn raw_for<H: Host>(
        self,
        node: NativeNodeId,
        buffer: BufferId,
        base: usize,
        bytes: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<super::RSpan, H::Error> {
        ctx.event()?;
        if self.leaf.node != node
            || self.leaf.buffer != buffer
            || self.leaf.base != base
            || self.leaf.bytes != bytes
            || self.start >= self.end
            || self.end > bytes
        {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let start = base
            .checked_add(self.start)
            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        let end = base
            .checked_add(self.end)
            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        Ok(super::RSpan { buffer, start, end })
    }
    pub(crate) fn kind(self) -> SelectedKind {
        self.kind
    }
    pub(crate) fn start(self) -> usize {
        self.start
    }
    pub(crate) fn end(self) -> usize {
        self.end
    }
    pub(crate) fn domain_end(self) -> usize {
        self.domain_end
    }
}

/// Single consumer in source order; no selected row or A node is a source grant.
pub(crate) struct Selection<'leaf, L> {
    leaf: LeafBrand<'leaf>,
    links: NativeRows<SelectedLink<'leaf>, L>,
    next: usize,
}

impl<'leaf, L> Selection<'leaf, L> {
    pub(crate) fn next<H: Host<Lease = L>>(
        &mut self,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<SelectedLink<'leaf>>, H::Error> {
        ctx.event()?;
        let row = self
            .links
            .get(self.next, |bytes| super::rows::charge_host(ctx.host, bytes))?;
        if row.is_some() {
            super::rows::charge_host(ctx.host, std::mem::size_of::<SelectedLink<'_>>())?;
        }
        let link = row.copied();
        if let Some(link) = link {
            if link.leaf.node != self.leaf.node
                || link.leaf.buffer != self.leaf.buffer
                || link.leaf.base != self.leaf.base
                || link.leaf.bytes != self.leaf.bytes
            {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        }
        Ok(link)
    }
}

pub(crate) fn select<'leaf, H: Host>(
    raw: &str,
    released: ShadowReleased<'leaf, H::Lease>,
    ctx: &mut Context<'_, H>,
) -> Result<Selection<'leaf, H::Lease>, H::Error> {
    selector::select(raw, released, ctx)
}

pub(crate) fn has_raw_autolink_marker<H: Host>(
    bytes: &[u8],
    ctx: &mut Context<'_, H>,
) -> Result<bool, H::Error> {
    let mut window = [0; 4];
    let found = ctx.find(bytes, |part| {
        part.iter().position(|&byte| {
            window.rotate_left(1);
            window[3] = byte;
            window == *b"www." || window[1..] == *b"://"
        })
    })?;
    Ok(found.is_some())
}

#[cfg(test)]
mod test_values;
