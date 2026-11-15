// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Source-member geometry indices only. The actual permission remains the
//! native claim occurrence and is written when a selected gap is emitted.
use super::target::{Emission, Open, Posting, growing, sort};
use super::*;
use crate::memory::{BuildRows, BuildValue};
const NONE: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct OwnerEntry {
    flow: u32,
    owner: u32,
}
impl BuildValue for OwnerEntry {}
const _: () = assert!(size_of::<OwnerEntry>() == 8);

/// One memo for this Graph's flow family. Values select members, never authority.
pub(super) struct Owners<'view, 'input, 'id> {
    rows: Option<BuildRows<'id, OwnerEntry>>,
    main_root: Option<b::NodeRef>,
    next_flow: usize,
    identity: std::marker::PhantomData<n::LineageToken<'view, 'input>>,
    #[cfg(test)]
    initialized: usize,
    #[cfg(test)]
    capacity: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OwnerStats {
    pub(super) rows: usize,
    pub(super) initialized: usize,
    pub(super) prepares: usize,
    pub(super) capacity: u64,
}

struct FlowOwners<'cache, 'view, 'input, 'id> {
    rows: &'cache mut BuildRows<'id, OwnerEntry>,
    flow: u32,
    members: usize,
    identity: std::marker::PhantomData<n::LineageToken<'view, 'input>>,
    #[cfg(test)]
    stats: OwnerStats,
}

fn owner_ordinal(value: usize, work: &mut OwnedWork<'_, '_>) -> Result<u32, CoreError> {
    let value = u32::try_from(value).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
    if value == NONE {
        return Err(work.fail(CoreError::ArithmeticOverflow));
    }
    Ok(value)
}

impl<'view, 'input, 'id> Owners<'view, 'input, 'id> {
    pub(super) fn new() -> Self {
        Self {
            rows: None,
            main_root: None,
            next_flow: 0,
            identity: std::marker::PhantomData,
            #[cfg(test)]
            initialized: 0,
            #[cfg(test)]
            capacity: 0,
        }
    }

    fn initialize(&mut self, count: usize, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        if self.rows.is_some() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        #[cfg(test)]
        let before = work.frozen_budget().usage().auxiliary_capacity;
        let mut rows = BuildRows::new(
            work.require_model_limits()?.native_records,
            LimitKind::NativeRecords,
            work,
        )?;
        for _ in 0..count {
            rows.push(
                OwnerEntry {
                    flow: NONE,
                    owner: NONE,
                },
                work,
            )?;
            #[cfg(test)]
            {
                self.initialized += 1;
            }
        }
        #[cfg(test)]
        {
            self.capacity = work.frozen_budget().usage().auxiliary_capacity - before;
        }
        self.rows = Some(rows);
        Ok(())
    }

    fn prepare(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        graph: &graph::Graph<'view, 'input, 'id>,
        flow_index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<FlowOwners<'_, 'view, 'input, 'id>, CoreError> {
        work.events(1)?;
        let flow = owner_ordinal(flow_index, work)?;
        if flow_index != self.next_flow || flow_index >= graph.flows.len() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let main = graph.flows.get(0, work)?;
        let main_root = graph.nodes.get(main.root(), work)?.node;
        match self.main_root {
            Some(root) if root != main_root || self.rows.is_none() => {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            None => {
                let count = native.read(work, |a| Ok(a.created_nodes()))? as usize;
                self.initialize(count, work)?;
                self.main_root = Some(main_root);
            }
            _ => {}
        }
        self.next_flow = self
            .next_flow
            .checked_add(1)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        #[cfg(test)]
        let stats = self.stats();
        Ok(FlowOwners {
            rows: self
                .rows
                .as_mut()
                .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?,
            flow,
            members: graph.nodes.len(),
            identity: std::marker::PhantomData,
            #[cfg(test)]
            stats,
        })
    }

    pub(super) fn retire(&mut self) {
        drop(self.rows.take());
        #[cfg(test)]
        {
            self.capacity = 0;
        }
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> OwnerStats {
        OwnerStats {
            rows: self.rows.as_ref().map_or(0, BuildRows::len),
            initialized: self.initialized,
            prepares: self.next_flow,
            capacity: self.capacity,
        }
    }
}

impl<'id> FlowOwners<'_, '_, '_, 'id> {
    fn len(&self) -> usize {
        self.rows.len()
    }

    fn get(&self, id: usize, work: &mut OwnedWork<'id, '_>) -> Result<Option<usize>, CoreError> {
        let entry = self.rows.get(id, work)?;
        if entry.flow != self.flow {
            return Ok(None);
        }
        let owner = entry.owner as usize;
        if entry.owner == NONE || owner >= self.members {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(Some(owner))
    }

    fn put(
        &mut self,
        id: usize,
        owner: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let member = owner_ordinal(owner, work)?;
        if owner >= self.members {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        self.rows.replace(
            id,
            OwnerEntry {
                flow: self.flow,
                owner: member,
            },
            work,
        )
    }
}

#[derive(Clone, Copy)]
struct Part {
    claim: usize,
    span: SourceSpan,
}
impl BuildValue for Part {}
#[derive(Clone, Copy)]
struct Candidate {
    part: u32,
    span: SourceSpan,
}
impl BuildValue for Candidate {}
pub(super) struct Index<'id> {
    parts: BuildRows<'id, Part>,
    order: BuildGrowingVec<'id, u32>,
    max_end: BuildRows<'id, u64>,
}
impl<'id> Index<'id> {
    pub(super) fn new(
        native: &Native<'_, '_, '_, '_, 'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        let max = work.require_model_limits()?.source_pieces;
        let mut result = Self {
            parts: BuildRows::new(max, LimitKind::SourcePieces, work)?,
            order: BuildGrowingVec::new(max, LimitKind::SourcePieces, work)?,
            max_end: BuildRows::new(max, LimitKind::SourcePieces, work)?,
        };
        let count = native.read(work, |a| Ok(a.claim_count()))?;
        let mut previous = None;
        for claim in 0..count {
            let row = native.read(work, |a| a.claim(claim))?;
            if previous.is_some_and(|p| p >= row.order) {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            previous = Some(row.order);
            native.visit_sources(row.source, work, |span, w| {
                if !span.is_empty() {
                    let id = u32::try_from(result.parts.len())
                        .map_err(|_| CoreError::ArithmeticOverflow)?;
                    result.parts.push(Part { claim, span }, w)?;
                    result.order.push(id, w)?;
                }
                Ok(())
            })?;
        }
        sort(
            &mut result.order,
            |id, w| {
                let p = result.parts.get(id as usize, w)?;
                Ok((p.span.start(), p.span.end()))
            },
            work,
        )?;
        let mut end = 0;
        for at in 0..result.order.len() {
            end = end.max(
                result
                    .parts
                    .get(growing(&result.order, at, work)? as usize, work)?
                    .span
                    .end(),
            );
            result.max_end.push(end, work)?;
        }
        Ok(result)
    }
}

struct Active<'id> {
    rows: BuildGrowingVec<'id, u32>,
    len: usize,
}
impl<'id> Active<'id> {
    fn new(max: u64, op: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        Ok(Self {
            rows: BuildGrowingVec::new(max, LimitKind::SourcePieces, op)?,
            len: 0,
        })
    }
    fn key(
        &self,
        rank: u32,
        candidates: &BuildRows<'id, Candidate>,
        order: &BuildGrowingVec<'id, u32>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(u64, u32), CoreError> {
        op.events(1)?;
        let row = growing(order, rank as usize, op)?;
        Ok((candidates.get(row as usize, op)?.span.len(), rank))
    }
    fn push(
        &mut self,
        rank: u32,
        candidates: &BuildRows<'id, Candidate>,
        order: &BuildGrowingVec<'id, u32>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let mut at = self.len;
        if at == self.rows.len() {
            self.rows.push(rank, op)?;
        } else {
            self.rows.replace(at, rank, op)?;
        }
        self.len = self
            .len
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        while at != 0 {
            op.events(1)?;
            let parent = (at - 1) / 2;
            if self.key(growing(&self.rows, parent, op)?, candidates, order, op)?
                <= self.key(growing(&self.rows, at, op)?, candidates, order, op)?
            {
                break;
            }
            let a = growing(&self.rows, parent, op)?;
            let b = growing(&self.rows, at, op)?;
            self.rows.replace(parent, b, op)?;
            self.rows.replace(at, a, op)?;
            at = parent;
        }
        Ok(())
    }
    fn pop(
        &mut self,
        candidates: &BuildRows<'id, Candidate>,
        order: &BuildGrowingVec<'id, u32>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        if self.len == 0 {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        self.len -= 1;
        if self.len == 0 {
            return Ok(());
        }
        let value = growing(&self.rows, self.len, op)?;
        self.rows.replace(0, value, op)?;
        let mut at = 0usize;
        loop {
            op.events(1)?;
            let left = at
                .checked_mul(2)
                .and_then(|v| v.checked_add(1))
                .ok_or(CoreError::ArithmeticOverflow)?;
            if left >= self.len {
                break;
            }
            let mut child = left;
            if left + 1 < self.len
                && self.key(growing(&self.rows, left + 1, op)?, candidates, order, op)?
                    < self.key(growing(&self.rows, left, op)?, candidates, order, op)?
            {
                child += 1;
            }
            if self.key(growing(&self.rows, at, op)?, candidates, order, op)?
                <= self.key(growing(&self.rows, child, op)?, candidates, order, op)?
            {
                break;
            }
            let a = growing(&self.rows, at, op)?;
            let b = growing(&self.rows, child, op)?;
            self.rows.replace(at, b, op)?;
            self.rows.replace(child, a, op)?;
            at = child;
        }
        Ok(())
    }
    fn best(
        &mut self,
        cursor: u64,
        candidates: &BuildRows<'id, Candidate>,
        order: &BuildGrowingVec<'id, u32>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Candidate, CoreError> {
        loop {
            op.events(1)?;
            if self.len == 0 {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let rank = growing(&self.rows, 0, op)?;
            let row = growing(order, rank as usize, op)?;
            let candidate = candidates.get(row as usize, op)?;
            if candidate.span.end() > cursor {
                return Ok(candidate);
            }
            self.pop(candidates, order, op)?;
        }
    }
}

struct Future<'id> {
    general: Option<GeneralFuture<'id>>,
    begin: usize,
    end: usize,
}
impl<'id> Future<'id> {
    fn new(
        range: std::ops::Range<usize>,
        emissions: &BuildRows<'id, Emission>,
        postings: &BuildRows<'id, Posting>,
        order: &BuildGrowingVec<'id, u32>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        let mut previous = None;
        let mut monotone = true;
        for at in 0..order.len() {
            w.events(1)?;
            let p = postings.get(growing(order, at, w)? as usize, w)?;
            let rank = emissions.get(p.emission as usize, w)?.owner;
            if !range.contains(&rank) {
                return Err(w.fail(CoreError::InvalidProvenance));
            }
            if previous.is_some_and(|p| p > rank) {
                monotone = false;
            }
            previous = Some(rank);
        }
        let general = if monotone {
            None
        } else {
            Some(GeneralFuture::new(
                range.clone(),
                emissions,
                postings,
                order,
                w,
            )?)
        };
        Ok(Self {
            general,
            begin: range.start,
            end: range.end,
        })
    }
    fn advance(
        &mut self,
        emissions: &BuildRows<'id, Emission>,
        posting: Posting,
        rank: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        match &mut self.general {
            None => w.check(),
            Some(g) => g.advance(emissions.get(posting.emission as usize, w)?.owner, rank, w),
        }
    }
    fn minimum(
        &self,
        emissions: &BuildRows<'id, Emission>,
        postings: &BuildRows<'id, Posting>,
        order: &BuildGrowingVec<'id, u32>,
        pi: usize,
        range: std::ops::Range<usize>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<u32, CoreError> {
        if let Some(g) = &self.general {
            return g.minimum(range, w);
        }
        if range.start < self.begin || range.start >= range.end || range.end > self.end {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        let mut a = 0;
        let mut b = order.len();
        while a < b {
            w.events(1)?;
            let m = a + (b - a) / 2;
            let p = postings.get(growing(order, m, w)? as usize, w)?;
            if emissions.get(p.emission as usize, w)?.owner < range.start {
                a = m + 1;
            } else {
                b = m;
            }
        }
        let selected = a.max(pi);
        if selected >= order.len() {
            return Ok(NONE);
        }
        let p = postings.get(growing(order, selected, w)? as usize, w)?;
        if emissions.get(p.emission as usize, w)?.owner < range.end {
            u32::try_from(selected).map_err(|_| w.fail(CoreError::ArithmeticOverflow))
        } else {
            Ok(NONE)
        }
    }
}
struct GeneralFuture<'id> {
    tree: BuildRows<'id, u32>,
    next: BuildRows<'id, u32>,
    base: usize,
    begin: usize,
    end: usize,
}
impl<'id> GeneralFuture<'id> {
    fn new(
        range: std::ops::Range<usize>,
        emissions: &BuildRows<'id, Emission>,
        postings: &BuildRows<'id, Posting>,
        order: &BuildGrowingVec<'id, u32>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        let base = (range.end - range.start)
            .checked_next_power_of_two()
            .ok_or(CoreError::ArithmeticOverflow)?;
        let width = base.checked_mul(2).ok_or(CoreError::ArithmeticOverflow)?;
        let mut tree = BuildRows::new(width as u64, LimitKind::ModelRecords, w)?;
        for _ in 0..width {
            tree.push(NONE, w)?;
        }
        let mut next = BuildRows::new(order.len() as u64, LimitKind::SourcePieces, w)?;
        for _ in 0..order.len() {
            next.push(NONE, w)?;
        }
        for rank in (0..order.len()).rev() {
            w.events(1)?;
            let posting = postings.get(growing(order, rank, w)? as usize, w)?;
            let owner = emissions.get(posting.emission as usize, w)?.owner;
            let local = owner
                .checked_sub(range.start)
                .ok_or(CoreError::InvalidProvenance)?;
            if owner >= range.end {
                return Err(w.fail(CoreError::InvalidProvenance));
            }
            let leaf = base + local;
            let previous = tree.get(leaf, w)?;
            next.replace(rank, previous, w)?;
            tree.replace(
                leaf,
                u32::try_from(rank).map_err(|_| CoreError::ArithmeticOverflow)?,
                w,
            )?;
        }
        for at in (1..base).rev() {
            let value = tree.get(at * 2, w)?.min(tree.get(at * 2 + 1, w)?);
            if value != NONE {
                tree.replace(at, value, w)?;
            }
        }
        Ok(Self {
            tree,
            next,
            base,
            begin: range.start,
            end: range.end,
        })
    }
    fn advance(
        &mut self,
        owner: usize,
        rank: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        if owner < self.begin || owner >= self.end {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        let mut at = self.base + owner - self.begin;
        if self.tree.get(at, w)? as usize != rank {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        self.tree.replace(at, self.next.get(rank, w)?, w)?;
        while at > 1 {
            w.events(1)?;
            at /= 2;
            let value = self.tree.get(at * 2, w)?.min(self.tree.get(at * 2 + 1, w)?);
            if self.tree.get(at, w)? == value {
                break;
            }
            self.tree.replace(at, value, w)?;
        }
        Ok(())
    }
    fn minimum(
        &self,
        range: std::ops::Range<usize>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<u32, CoreError> {
        if range.start < self.begin || range.start >= range.end || range.end > self.end {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        let mut a = self.base + range.start - self.begin;
        let mut b = self.base + range.end - self.begin;
        let mut result = NONE;
        while a < b {
            w.events(1)?;
            if a & 1 != 0 {
                result = result.min(self.tree.get(a, w)?);
                a += 1;
            }
            if b & 1 != 0 {
                b -= 1;
                result = result.min(self.tree.get(b, w)?);
            }
            a /= 2;
            b /= 2;
        }
        Ok(result)
    }
}

fn claim_kind(kind: n::ClaimKind) -> ClaimKind {
    match kind {
        n::ClaimKind::Syntax(role) => match role {
            native::SyntaxRole::LeadingBom => ClaimKind::LeadingBom,
            native::SyntaxRole::QuotePrefix | native::SyntaxRole::ListPrefix => {
                ClaimKind::ContainerPrefix
            }
            native::SyntaxRole::BlankLine
            | native::SyntaxRole::BlockTerminator
            | native::SyntaxRole::ParagraphTerminator
            | native::SyntaxRole::BlockWhitespace => ClaimKind::Whitespace,
            native::SyntaxRole::ReferenceDefinition => ClaimKind::ReferenceDefinition,
            native::SyntaxRole::HeadingIdentifier => ClaimKind::HeadingIdentifier,
            native::SyntaxRole::TableOfContents => ClaimKind::TableOfContentsMarker,
            native::SyntaxRole::DescriptionMarker => ClaimKind::DescriptionMarker,
            native::SyntaxRole::FootnoteIndent | native::SyntaxRole::DescriptionIndent => {
                ClaimKind::Whitespace
            }
            native::SyntaxRole::TableTrim
            | native::SyntaxRole::TableSeparator
            | native::SyntaxRole::TableDelimiter
            | native::SyntaxRole::TableExtraCell => ClaimKind::Table,
            native::SyntaxRole::TaskWhitespace => ClaimKind::Task,
            _ => ClaimKind::Block,
        },
        n::ClaimKind::Delimiter => ClaimKind::Delimiter,
        n::ClaimKind::Code => ClaimKind::Code,
        n::ClaimKind::Break => ClaimKind::Break,
        n::ClaimKind::Whitespace => ClaimKind::Whitespace,
        n::ClaimKind::Link => ClaimKind::Link,
        n::ClaimKind::PayloadTrim => ClaimKind::Payload,
        n::ClaimKind::Table => ClaimKind::Table,
        n::ClaimKind::Task => ClaimKind::Task,
        n::ClaimKind::Math(syntax) => ClaimKind::Math(graph::math_syntax(syntax)),
        n::ClaimKind::FootnoteReference => ClaimKind::Block,
    }
}
fn role(role: n::HostRole) -> HostRole {
    match role {
        n::HostRole::Block => HostRole::Block,
        n::HostRole::Inline => HostRole::Inline,
        n::HostRole::Destination => HostRole::Destination,
        n::HostRole::Title => HostRole::Title,
        n::HostRole::FenceInfo => HostRole::FenceInfo,
        n::HostRole::ReferenceLabel => HostRole::ReferenceLabel,
        n::HostRole::MathInput => HostRole::MathInput,
    }
}
fn phase(phase: n::NativePhase) -> NativePhase {
    match phase {
        n::NativePhase::Block => NativePhase::Block,
        n::NativePhase::Inline => NativePhase::Inline,
        n::NativePhase::Metadata => NativePhase::Metadata,
    }
}

fn owner<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &graph::Graph<'view, 'input, 'id>,
    builder: &b::DirectBuilder<'id>,
    mut token: n::LineageToken<'view, 'input>,
    flow: usize,
    scratch: (
        &mut FlowOwners<'_, 'view, 'input, 'id>,
        &mut BuildGrowingVec<'id, usize>,
    ),
    work: &mut OwnedWork<'id, '_>,
) -> Result<usize, CoreError> {
    let (cache, path) = scratch;
    let mut count = 0;
    let root = graph.flows.get(flow, work)?.root();
    let selected = loop {
        work.events(1)?;
        let binding = native.read(work, |a| a.binding(token))?;
        let id = binding.id.get() as usize - 1;
        if let Some(cached) = cache.get(id, work)? {
            break cached;
        }
        if count >= cache.len() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        if count == path.len() {
            path.push(id, work)?;
        } else {
            path.replace(count, id, work)?;
        }
        count += 1;
        if let Some(index) = graph.member_for_lineage(token, native, work)?
            && builder
                .graph_node(graph.nodes.get(index, work)?.node, work)?
                .flow
                == flow
        {
            break index;
        }
        match binding.birth_parent {
            Some(parent) => token = parent,
            None => break root,
        }
    };
    for at in 0..count {
        work.events(1)?;
        cache.put(growing(path, at, work)?, selected, work)?;
    }
    Ok(selected)
}

fn precedes_envelope<'id>(
    postings: &BuildRows<'id, Posting>,
    order: &BuildGrowingVec<'id, u32>,
    pi: usize,
    selected: usize,
    envelope_end: u64,
    work: &mut OwnedWork<'id, '_>,
) -> Result<bool, CoreError> {
    work.check()?;
    // For sorted starts, selected <= lower_bound(pi..len, envelope_end)
    // depends only on the selected predecessor. Keep the original <= tie.
    if selected <= pi {
        return Ok(true);
    }
    if selected > order.len() {
        return Ok(false);
    }
    work.events(1)?;
    let previous = postings.get(growing(order, selected - 1, work)? as usize, work)?;
    Ok(previous.source.start() < envelope_end)
}

fn host_domains<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    host: n::HostDomain<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::SourceBlockRef, CoreError> {
    let domain = native.read(work, |access| access.source_domain(host.source))?;
    // A claim snapshots its host before a later append/cut can change the
    // buffer. Reuse only the exact full source, never an enlarged final host.
    if domain.utf8_len == host.utf8_len
        && native.read(work, |access| {
            access.same_source(host.source, domain.source)
        })?
    {
        let owner = native.read(work, |access| access.source_owner(host.source))?;
        let reusable = match owner {
            None => true,
            Some(owner) => match native.read(work, |access| access.binding(owner))?.buffer {
                Some(buffer) => {
                    native.read(work, |access| access.same_source(buffer, domain.source))?
                }
                None => false,
            },
        };
        // Detached or donated R hosts need not satisfy the metadata helper's
        // canonical-buffer precondition. Preserve their original projection.
        if reusable {
            return graph
                .host_sources(native, host.source, builder, work)
                .map(|(sources, _)| sources);
        }
    }
    native
        .sources(host.source, builder, work)
        .map(|(sources, _, _)| sources)
}

pub(super) fn emit<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    flow_index: usize,
    index: &Index<'id>,
    owners: &mut Owners<'view, 'input, 'id>,
    target: &mut Open<'_, '_, 'id, '_>,
    #[cfg(test)] mut case: Option<&mut owner_tests::Case>,
) -> Result<(), CoreError> {
    let result = (|| {
        let (flow, order, candidates, candidate_order, mut active, mut future, mut cache, mut path) =
            target.writer.with_read(|builder, w| {
                let flow = graph.flows.get(flow_index, w)?;
                let max = w.require_model_limits()?.source_pieces;
                let mut order = BuildGrowingVec::new(max, LimitKind::SourcePieces, w)?;
                for at in 0..target.postings.len() {
                    order.push(
                        u32::try_from(at).map_err(|_| CoreError::ArithmeticOverflow)?,
                        w,
                    )?;
                }
                sort(
                    &mut order,
                    |id, w| {
                        let span = target.postings.get(id as usize, w)?.source;
                        Ok((span.start(), span.end()))
                    },
                    w,
                )?;
                let mut candidates = BuildRows::new(max, LimitKind::SourcePieces, w)?;
                let mut candidate_order = BuildGrowingVec::new(max, LimitKind::SourcePieces, w)?;
                for member in 0..builder.source_len(flow.domains, w)? {
                    let domain = builder.source_at(flow.domains, member, w)?;
                    let mut a = 0;
                    let mut b = index.order.len();
                    while a < b {
                        w.events(1)?;
                        let m = a + (b - a) / 2;
                        if index.max_end.get(m, w)? <= domain.start() {
                            a = m + 1;
                        } else {
                            b = m;
                        }
                    }
                    for at in a..index.order.len() {
                        let id = growing(&index.order, at, w)?;
                        let part = index.parts.get(id as usize, w)?;
                        if part.span.start() >= domain.end() {
                            break;
                        }
                        if domain.start() < part.span.end() {
                            let next = u32::try_from(candidates.len())
                                .map_err(|_| CoreError::ArithmeticOverflow)?;
                            candidates.push(
                                Candidate {
                                    part: id,
                                    span: SourceSpan::new(
                                        part.span.start().max(domain.start()),
                                        part.span.end().min(domain.end()),
                                    )?,
                                },
                                w,
                            )?;
                            candidate_order.push(next, w)?;
                        }
                    }
                }
                sort(
                    &mut candidate_order,
                    |id, w| {
                        let span = candidates.get(id as usize, w)?.span;
                        Ok((span.start(), span.end()))
                    },
                    w,
                )?;
                let active = Active::new(max, w)?;
                let end = graph.nodes.get(flow.root(), w)?.end();
                let future = Future::new(
                    flow.root()..end,
                    &target.emissions,
                    &target.postings,
                    &order,
                    w,
                )?;
                let cache = owners.prepare(native, graph, flow_index, w)?;
                #[cfg(test)]
                if let Some(case) = case.as_deref_mut() {
                    case.at(owner_tests::Stage::Prepared, flow_index, cache.stats, w)?;
                }
                let path = BuildGrowingVec::new(
                    w.require_model_limits()?.native_records,
                    LimitKind::NativeRecords,
                    w,
                )?;
                Ok((
                    flow,
                    order,
                    candidates,
                    candidate_order,
                    active,
                    future,
                    cache,
                    path,
                ))
            })?;
        let mut ci = 0;
        let mut pi = 0;
        let members = target
            .writer
            .with_read(|b, w| b.source_len(flow.domains, w))?;
        for member in 0..members {
            let domain = target
                .writer
                .with_read(|b, w| b.source_at(flow.domains, member, w))?;
            let mut cursor = domain.start();
            while cursor < domain.end() {
                let decision = target.writer.with_read(|builder, w| {
                    w.events(1)?;
                    let posting = if pi < order.len() {
                        Some(target.postings.get(growing(&order, pi, w)? as usize, w)?)
                    } else {
                        None
                    };
                    if let Some(p) = posting {
                        if p.source.start() < cursor {
                            return Err(w.fail(CoreError::InvalidProvenance));
                        }
                        if p.source.start() == cursor {
                            if p.source.end() > domain.end() {
                                return Err(w.fail(CoreError::InvalidProvenance));
                            }
                            future.advance(&target.emissions, p, pi, w)?;
                            pi += 1;
                            return Ok(Err(p.source.end()));
                        }
                    }
                    let end = posting.map_or(domain.end(), |p| p.source.start().min(domain.end()));
                    while ci < candidate_order.len() {
                        let c = candidates.get(growing(&candidate_order, ci, w)? as usize, w)?;
                        if c.span.start() > cursor {
                            break;
                        }
                        active.push(
                            u32::try_from(ci).map_err(|_| CoreError::ArithmeticOverflow)?,
                            &candidates,
                            &candidate_order,
                            w,
                        )?;
                        ci += 1;
                    }
                    let candidate = active.best(cursor, &candidates, &candidate_order, w)?;
                    let cut = end.min(candidate.span.end());
                    if cut <= cursor {
                        return Err(w.fail(CoreError::InvalidProvenance));
                    }
                    let part = index.parts.get(candidate.part as usize, w)?;
                    let claim = native.read(w, |a| a.claim(part.claim))?;
                    #[cfg(test)]
                    if let Some(case) = case.as_deref_mut() {
                        case.at(owner_tests::Stage::Owner, flow_index, cache.stats, w)?;
                    }
                    let owner = owner(
                        native,
                        graph,
                        builder,
                        claim.owner,
                        flow_index,
                        (&mut cache, &mut path),
                        w,
                    )?;
                    let member = graph.nodes.get(owner, w)?;
                    let row = builder.read_node(member.node, w)?;
                    let mut anchor = row.logical.end();
                    let script_delimiter = claim.kind == n::ClaimKind::Delimiter
                        && matches!(row.kind, NodeKind::Superscript | NodeKind::Subscript);
                    if script_delimiter && cursor == row.source_envelope.start() {
                        // A synthetic first child has no positive source posting.
                        // Its opening delimiter still belongs before that child.
                        anchor = row.logical.start();
                    } else if !script_delimiter && cursor < row.source_envelope.end() {
                        let selected = future.minimum(
                            &target.emissions,
                            &target.postings,
                            &order,
                            pi,
                            owner..member.end(),
                            w,
                        )?;
                        if selected != NONE {
                            if precedes_envelope(
                                &target.postings,
                                &order,
                                pi,
                                selected as usize,
                                row.source_envelope.end(),
                                w,
                            )? {
                                let p = target
                                    .postings
                                    .get(growing(&order, selected as usize, w)? as usize, w)?;
                                anchor = if p.emission == 0 {
                                    0
                                } else {
                                    target.emissions.get(p.emission as usize - 1, w)?.end
                                };
                            }
                        }
                    }
                    // The consumed raw task spacer precedes the synthetic
                    // label SP (the frozen projection policy), so its anchor
                    // is the actual Checkbox end, not the following Copy.
                    // This uses the accepted Task relation and spacer source;
                    // geometry alone cannot grant a new hidden permission.
                    if claim.kind == n::ClaimKind::Syntax(native::SyntaxRole::TaskWhitespace)
                        && row.kind == NodeKind::ListItem
                    {
                        let first = graph.first(owner, w)?.ok_or(CoreError::InvalidProvenance)?;
                        let checkbox = graph.nodes.get(first, w)?;
                        let checkbox_row = builder.read_node(checkbox.node, w)?;
                        if checkbox_row.kind != NodeKind::TaskCheckbox {
                            return Err(w.fail(CoreError::InvalidProvenance));
                        }
                        let token = native
                            .read(w, |a| a.g3_for_lineage(checkbox.lineage))?
                            .ok_or(CoreError::InvalidProvenance)?;
                        let n::G3Info::Task { item, spacer, .. } =
                            native.read(w, |a| a.g3_info(token))?
                        else {
                            return Err(w.fail(CoreError::InvalidProvenance));
                        };
                        if graph.member_for_lineage(item, native, w)? != Some(owner) {
                            return Err(w.fail(CoreError::InvalidProvenance));
                        }
                        let mut consumed_spacer = false;
                        native.visit_sources(spacer, w, |span, _| {
                            consumed_spacer |= span.start() <= cursor && cut <= span.end();
                            Ok(())
                        })?;
                        if consumed_spacer {
                            anchor = checkbox_row.logical.end();
                        }
                    }
                    anchor = anchor.clamp(row.logical.start(), row.logical.end());
                    Ok(Ok((claim, owner, member.node, anchor, cut)))
                })?;
                let (claim, owner, node, anchor, cut) = match decision {
                    Err(end) => {
                        cursor = end;
                        continue;
                    }
                    Ok(v) => v,
                };
                let (sources, primary) = target.writer.with_rows(|builder, w| {
                    let sources = builder.source_parts(&[SourceSpan::new(cursor, cut)?], w)?;
                    let host = claim.host.ok_or(CoreError::InvalidProvenance)?;
                    let domains = host_domains(native, graph, builder, host, w)?;
                    let mut kind = claim_kind(claim.kind);
                    let mut host_role = role(host.role);
                    // Claims from unused footnote bodies still cover source bytes.
                    // Their omitted nodes cannot authorize specialized claims.
                    if owner == flow.root()
                        && flow_index == 0
                        && matches!(
                            kind,
                            ClaimKind::Math(_)
                                | ClaimKind::TableOfContentsMarker
                                | ClaimKind::DescriptionMarker
                        )
                        && graph.member_for_lineage(claim.owner, native, w)?.is_none()
                    {
                        kind = ClaimKind::Block;
                        if host_role == HostRole::MathInput {
                            host_role = HostRole::Inline;
                        }
                    }
                    Ok((
                        sources,
                        b::DraftPrimaryAuthority {
                            phase: phase(host.phase),
                            kind,
                            consumed: LocalUtf8Span::new(
                                host.consumed_start as u64,
                                host.consumed_end as u64,
                            )?,
                            host_role,
                            host_utf8_len: host.utf8_len as u64,
                            host_domains: domains,
                        },
                    ))
                })?;
                let ticket = target.writer.reserve_next()?;
                let born = target.writer.commit_owned(
                    ticket,
                    b::DraftOwnedAtom {
                        owner: node,
                        kind: AtomKind::Hidden,
                        target: TargetSpan {
                            start: anchor,
                            end: anchor,
                        },
                        sources,
                        proof: None,
                        content: b::DraftAtomContent::Empty,
                        primary,
                    },
                )?;
                target.record(born, owner, anchor, sources, true)?;
                cursor = cut;
            }
        }
        target.writer.with_read(|_, w| {
            if pi == order.len() {
                Ok(())
            } else {
                Err(w.fail(CoreError::InvalidProvenance))
            }
        })
    })();
    result.map_err(|error| target.writer.fail(error))
}

#[cfg(test)]
#[path = "direct_partition_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "direct_owner_tests.rs"]
pub(super) mod owner_tests;

#[cfg(test)]
#[path = "direct_hidden_cost_tests.rs"]
mod cost_tests;
