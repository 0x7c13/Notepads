// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One live final target, with only the indices required for its permutation.
use super::*;
use crate::memory::{BuildKernel, BuildRows, BuildStep, BuildValue, BuildWindow, run_window};

#[derive(Clone, Copy)]
pub(super) struct Emission {
    pub born: b::BornAtom,
    pub owner: usize,
    pub end: u64,
    pub first: u64,
    delimiter: bool,
}
impl BuildValue for Emission {}
#[derive(Clone, Copy)]
pub(super) struct Posting {
    pub source: SourceSpan,
    pub emission: u32,
}
impl BuildValue for Posting {}

pub(super) struct Open<'builder, 'work, 'id, 'op> {
    pub writer: b::TargetWriter<'builder, 'work, 'id, 'op>,
    pub emissions: BuildRows<'id, Emission>,
    pub postings: BuildRows<'id, Posting>,
    positives: BuildGrowingVec<'id, u32>,
    zeros: BuildGrowingVec<'id, u32>,
    pub cursor: u64,
    pub flow: bool,
    relocated: bool,
}

pub(super) fn growing<'id, T: BuildValue>(
    rows: &BuildGrowingVec<'id, T>,
    at: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<T, CoreError> {
    struct Read<'a, 'id, T: Copy> {
        rows: &'a BuildGrowingVec<'id, T>,
        at: usize,
    }
    impl<'id, T: BuildValue> BuildKernel<'id> for Read<'_, 'id, T> {
        type Output = Option<T>;
        fn run<'scope>(
            &'scope mut self,
            window: &mut BuildWindow<'scope, '_, 'id, '_>,
        ) -> Result<BuildStep<Self::Output>, CoreError> {
            match window.read_growing(self.rows)? {
                BuildStep::Ready(view) => view.get(self.at, window),
                BuildStep::Full => Ok(BuildStep::Full),
                BuildStep::NeedCapacity(_) => Err(window.fail(CoreError::InternalInvariant)),
            }
        }
    }
    let mut read = Read { rows, at };
    loop {
        match run_window(&mut read, work)? {
            BuildStep::Ready(v) => return v.ok_or_else(|| work.fail(CoreError::InvalidProvenance)),
            BuildStep::Full => {}
            _ => return Err(work.fail(CoreError::InternalInvariant)),
        }
    }
}

/// Same presorted fast path and non-stable heap permutation as the previous
/// semantic algorithm. Only indices move; equal keys acquire no invented tie.
pub(super) fn sort<'id, K: Ord>(
    values: &mut BuildGrowingVec<'id, u32>,
    mut key: impl FnMut(u32, &mut OwnedWork<'id, '_>) -> Result<K, CoreError>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    work.check()?;
    let mut ordered = true;
    if values.len() > 1 {
        let mut previous = key(growing(values, 0, work)?, work)?;
        for at in 1..values.len() {
            work.events(1)?;
            let current = key(growing(values, at, work)?, work)?;
            if previous > current {
                ordered = false;
                break;
            }
            previous = current;
        }
    }
    if ordered {
        return work.check();
    }
    fn sift<'id, K: Ord>(
        values: &mut BuildGrowingVec<'id, u32>,
        mut root: usize,
        end: usize,
        key: &mut impl FnMut(u32, &mut OwnedWork<'id, '_>) -> Result<K, CoreError>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        // The four internal callers read immutable emission/part/posting/candidate
        // rows. Only indices move here: a root's copied key remains valid as that
        // same element descends. This is not a sorter for stateful key callbacks.
        let mut root_value = None;
        loop {
            w.events(1)?;
            let child = root
                .checked_mul(2)
                .and_then(|v| v.checked_add(1))
                .ok_or(CoreError::ArithmeticOverflow)?;
            if child >= end {
                return Ok(());
            }
            let (selected, selected_value, selected_key) = if child + 1 < end {
                // Preserve the original first reads: left, right, then root.
                let left = growing(values, child, w)?;
                let left_key = key(left, w)?;
                let right = growing(values, child + 1, w)?;
                let right_key = key(right, w)?;
                if left_key < right_key {
                    (child + 1, right, right_key)
                } else {
                    (child, left, left_key)
                }
            } else {
                // A lone child originally follows the root read. Leaf sifts
                // return above without reading either a root value or its key.
                if root_value.is_none() {
                    let value = growing(values, root, w)?;
                    root_value = Some((value, key(value, w)?));
                }
                let value = growing(values, child, w)?;
                (child, value, key(value, w)?)
            };
            if root_value.is_none() {
                let value = growing(values, root, w)?;
                root_value = Some((value, key(value, w)?));
            }
            let (value, root_key) = root_value
                .as_ref()
                .ok_or_else(|| w.fail(CoreError::InternalInvariant))?;
            if *root_key >= selected_key {
                return Ok(());
            }
            values.replace(root, selected_value, w)?;
            values.replace(selected, *value, w)?;
            root = selected;
        }
    }
    for root in (0..values.len() / 2).rev() {
        sift(values, root, values.len(), &mut key, work)?;
    }
    for end in (1..values.len()).rev() {
        let a = growing(values, 0, work)?;
        let b = growing(values, end, work)?;
        values.replace(0, b, work)?;
        values.replace(end, a, work)?;
        sift(values, 0, end, &mut key, work)?;
    }
    work.check()
}

#[cfg(test)]
#[path = "direct_sort_tests.rs"]
mod sort_tests;

impl<'builder, 'work, 'id, 'op> Open<'builder, 'work, 'id, 'op> {
    pub(super) fn new(
        builder: &'builder mut b::DirectBuilder<'id>,
        work: &'work mut OwnedWork<'id, 'op>,
        flow: bool,
    ) -> Result<Self, CoreError> {
        let limits = work
            .model_limits()
            .ok_or(CoreError::UnsupportedConfiguration)?;
        let emissions = BuildRows::new(limits.model_records, LimitKind::ModelRecords, work)?;
        let postings = BuildRows::new(limits.source_pieces, LimitKind::SourcePieces, work)?;
        let positives = BuildGrowingVec::new(limits.model_records, LimitKind::ModelRecords, work)?;
        let zeros = BuildGrowingVec::new(limits.model_records, LimitKind::ModelRecords, work)?;
        let writer = builder.begin_target(work)?;
        Ok(Self {
            writer,
            emissions,
            postings,
            positives,
            zeros,
            cursor: 0,
            flow,
            relocated: false,
        })
    }
    pub(super) fn record(
        &mut self,
        born: b::BornAtom,
        owner: usize,
        end: u64,
        sources: b::SourceBlockRef,
        hidden: bool,
    ) -> Result<(), CoreError> {
        self.record_with_delimiter(born, owner, end, sources, hidden, false)
    }

    fn record_with_delimiter(
        &mut self,
        born: b::BornAtom,
        owner: usize,
        end: u64,
        sources: b::SourceBlockRef,
        hidden: bool,
        delimiter: bool,
    ) -> Result<(), CoreError> {
        self.writer.with_read(|builder, work| {
            work.events(1)?;
            let first = builder.source_at(sources, 0, work)?.start();
            let index =
                u32::try_from(self.emissions.len()).map_err(|_| CoreError::ArithmeticOverflow)?;
            self.emissions.push(
                Emission {
                    born,
                    owner,
                    end,
                    first,
                    delimiter,
                },
                work,
            )?;
            if !hidden && end > self.cursor {
                self.positives.push(index, work)?;
            } else {
                self.zeros.push(index, work)?;
            }
            if !hidden {
                if end < self.cursor {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                self.cursor = end;
            }
            if self.flow && !hidden {
                for i in 0..builder.source_len(sources, work)? {
                    let source = builder.source_at(sources, i, work)?;
                    if !source.is_empty() {
                        self.postings.push(
                            Posting {
                                source,
                                emission: index,
                            },
                            work,
                        )?;
                    }
                }
            }
            Ok(())
        })
    }
    pub(super) fn close(self) -> Result<b::ClosedTarget<'id>, CoreError> {
        self.close_impl(
            #[cfg(test)]
            None,
        )
    }

    fn close_impl(
        mut self,
        #[cfg(test)] observation: Option<(
            &mut super::hidden::owner_tests::Case,
            usize,
            super::hidden::OwnerStats,
        )>,
    ) -> Result<b::ClosedTarget<'id>, CoreError> {
        let order = self.writer.with_read(|_, work| {
            sort(
                &mut self.zeros,
                |id, w| {
                    let e = self.emissions.get(id as usize, w)?;
                    Ok((if self.relocated { e.end } else { 0 }, e.first, id))
                },
                work,
            )?;
            let mut order = BuildGrowingVec::new(
                work.require_model_limits()?.model_records,
                LimitKind::ModelRecords,
                work,
            )?;
            let mut zero = 0;
            let mut boundary = 0;
            for at in 0..self.positives.len() {
                let positive = self
                    .emissions
                    .get(growing(&self.positives, at, work)? as usize, work)?;
                while zero < self.zeros.len() {
                    let value = self
                        .emissions
                        .get(growing(&self.zeros, zero, work)? as usize, work)?;
                    work.events(1)?;
                    work.scan(size_of::<u64>() as u64)?;
                    // A synthetic separator can precede a hidden HTML delimiter at
                    // the same source byte. Preserve the delimiter's logical
                    // position while leaving the older hidden ties ordered
                    // by their original source and emission identity.
                    let follows = if self.relocated {
                        value.end > boundary
                    } else {
                        value.first > positive.first
                            || (value.first == positive.first
                                && value.delimiter
                                && value.end >= positive.end)
                    };
                    if follows {
                        break;
                    }
                    order.push(value.born, work)?;
                    zero += 1;
                }
                work.events(1)?;
                order.push(positive.born, work)?;
                boundary = positive.end;
            }
            while zero < self.zeros.len() {
                order.push(
                    self.emissions
                        .get(growing(&self.zeros, zero, work)? as usize, work)?
                        .born,
                    work,
                )?;
                zero += 1;
            }
            #[cfg(test)]
            if let Some((case, flow, stats)) = observation {
                case.at(
                    super::hidden::owner_tests::Stage::CloseOrder,
                    flow,
                    stats,
                    work,
                )?;
            }
            Ok(order)
        })?;
        // These actual rows have no consumer once the complete Born order exists.
        // Keep the original writer guard alive across their destruction and close.
        drop(self.emissions);
        drop(self.postings);
        drop(self.positives);
        drop(self.zeros);
        self.writer.close(order)
    }
}

pub(super) fn host_role(role: PayloadRole) -> HostRole {
    match role {
        PayloadRole::Destination => HostRole::Destination,
        PayloadRole::Title => HostRole::Title,
        PayloadRole::FenceInfo => HostRole::FenceInfo,
        PayloadRole::ReferenceLabel => HostRole::ReferenceLabel,
        PayloadRole::MathInput => HostRole::MathInput,
        PayloadRole::FenceInput => HostRole::FenceInput,
    }
}

#[derive(Clone, Copy)]
struct Owner {
    node: b::NodeRef,
    index: usize,
    kind: NodeKind,
}
#[derive(Clone, Copy)]
enum RawTransform {
    N01(PrimitiveKind),
    PartialTab(u32),
    TableEscapedPipe(b::AuthorityRef),
    HtmlBreak,
    HtmlEmphasis(u32),
    HtmlCode(u32),
}

#[derive(Clone, Copy)]
struct Host<'view, 'input> {
    token: n::SourceToken<'view, 'input>,
    domains: b::SourceBlockRef,
    start: usize,
    len: usize,
    role: HostRole,
    phase: NativePhase,
    kind: ClaimKind,
}
impl Host<'_, '_> {
    fn primary(self, start: usize, end: usize) -> Result<b::DraftPrimaryAuthority, CoreError> {
        let start = start
            .checked_sub(self.start)
            .ok_or(CoreError::InvalidProvenance)?;
        let end = end
            .checked_sub(self.start)
            .ok_or(CoreError::InvalidProvenance)?;
        if end > self.len {
            return Err(CoreError::InvalidProvenance);
        }
        Ok(b::DraftPrimaryAuthority {
            phase: self.phase,
            kind: self.kind,
            consumed: LocalUtf8Span::new(start as u64, end as u64)?,
            host_role: self.role,
            host_utf8_len: self.len as u64,
            host_domains: self.domains,
        })
    }
}
fn spec(
    owner: b::NodeRef,
    sources: b::SourceBlockRef,
    primary: b::DraftPrimaryAuthority,
) -> b::DraftAuthoritySpec {
    b::DraftAuthoritySpec {
        owner,
        host: b::DraftHostRow {
            owner,
            role: primary.host_role,
            utf8_len: primary.host_utf8_len,
            domains: primary.host_domains,
        },
        phase: primary.phase,
        kind: primary.kind,
        consumed: primary.consumed,
        sources,
    }
}
fn flavor(kind: NodeKind) -> (HostRole, NativePhase, ClaimKind) {
    match kind {
        NodeKind::CodeBlock => (HostRole::Block, NativePhase::Block, ClaimKind::Code),
        NodeKind::InlineCode => (HostRole::Inline, NativePhase::Inline, ClaimKind::Code),
        NodeKind::HtmlBlock => (HostRole::Block, NativePhase::Block, ClaimKind::Html),
        NodeKind::HtmlInline | NodeKind::HtmlComment => {
            (HostRole::Inline, NativePhase::Inline, ClaimKind::Html)
        }
        NodeKind::SoftBreak | NodeKind::HardBreak => {
            (HostRole::Inline, NativePhase::Inline, ClaimKind::Break)
        }
        _ => (HostRole::Inline, NativePhase::Inline, ClaimKind::Block),
    }
}
impl<'id> Open<'_, '_, 'id, '_> {
    fn flow_host<'view, 'input>(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        graph: &mut graph::Graph<'view, 'input, 'id>,
        source: n::SourceToken<'view, 'input>,
        kind: NodeKind,
    ) -> Result<Host<'view, 'input>, CoreError> {
        let (domains, domain) = self
            .writer
            .with_rows(|b, w| graph.host_sources(native, source, b, w))?;
        let (role, phase, kind) = flavor(kind);
        Ok(Host {
            token: domain.source,
            domains,
            start: 0,
            len: domain.utf8_len,
            role,
            phase,
            kind,
        })
    }
    fn leaf(
        &mut self,
        owner: Owner,
        sources: b::SourceBlockRef,
        primary: b::DraftPrimaryAuthority,
        text: &str,
        transform: Option<RawTransform>,
        hidden: bool,
    ) -> Result<(), CoreError> {
        let Owner {
            node: owner,
            index: owner_index,
            ..
        } = owner;
        let payload = !self.flow;
        let primitive = match transform {
            Some(RawTransform::N01(kind)) => kind,
            _ => PrimitiveKind::Copy,
        };
        let result = (|| {
            let width = self.writer.with_read(|_, w| {
                if hidden {
                    return Ok(0);
                }
                if payload {
                    return Ok(text.len() as u64);
                }
                let mut at = 0;
                let mut width = 0_u64;
                while at < text.len() {
                    let mut end = at
                        .saturating_add(crate::policy::CHECKPOINT_BYTES as usize)
                        .min(text.len());
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    let chunk = &text[at..end];
                    w.scan(chunk.len() as u64)?;
                    width = width
                        .checked_add(chunk.chars().map(|c| c.len_utf16() as u64).sum::<u64>())
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    at = end;
                }
                Ok(width)
            })?;
            let end = self
                .cursor
                .checked_add(width)
                .ok_or(CoreError::ArithmeticOverflow)?;
            let ticket = self.writer.reserve_next()?;
            let born = ticket.born();
            let (proof, content) = if transform.is_some()
                && (!hidden
                    || matches!(
                        transform,
                        Some(RawTransform::HtmlEmphasis(_) | RawTransform::HtmlCode(_))
                    )) {
                self.writer.with_rows(|builder, work| {
                    let value = builder.text(text, work)?;
                    let source = builder.source_at(sources, 0, work)?;
                    let primitives = builder.primitive_parts(
                        &[PrimitiveRow {
                            source,
                            kind: primitive,
                        }],
                        work,
                    )?;
                    let stage_start = builder.stages_len();
                    if let Some(RawTransform::PartialTab(column)) = transform {
                        // Partial tabs are a block-container transform even when
                        // the final math input belongs to an inline host.
                        let stage = b::DraftAuthoritySpec {
                            owner,
                            host: b::DraftHostRow {
                                owner,
                                role: HostRole::Block,
                                utf8_len: 1,
                                domains: sources,
                            },
                            phase: NativePhase::Block,
                            kind: ClaimKind::ContainerPrefix,
                            consumed: LocalUtf8Span::new(0, 1)?,
                            sources,
                        };
                        let accepted_by = builder.write_extra_authority(stage, work)?;
                        builder.stage_parts(
                            &[b::DraftStageRow {
                                kind: StageKind::PartialTab,
                                input: LocalUtf8Span::new(0, 1)?,
                                output: value,
                                parameter: column,
                                accepted_by,
                            }],
                            work,
                        )?;
                    }
                    if let Some(RawTransform::TableEscapedPipe(accepted_by)) = transform {
                        builder.stage_parts(
                            &[b::DraftStageRow {
                                kind: StageKind::TableEscapedPipe,
                                input: LocalUtf8Span::new(0, 2)?,
                                output: value,
                                parameter: 0,
                                accepted_by,
                            }],
                            work,
                        )?;
                    }
                    let html_stage = match transform {
                        Some(RawTransform::HtmlBreak) => Some((StageKind::HtmlBreakToLineFeed, 0)),
                        Some(RawTransform::HtmlEmphasis(parameter)) => {
                            Some((StageKind::HtmlEmphasisDelimiter, parameter))
                        }
                        Some(RawTransform::HtmlCode(parameter)) => {
                            Some((StageKind::HtmlCodeDelimiter, parameter))
                        }
                        _ => None,
                    };
                    if let Some((kind, parameter)) = html_stage {
                        builder.stage_parts(
                            &[b::DraftStageRow {
                                kind,
                                input: LocalUtf8Span::new(0, source.len())?,
                                output: value,
                                parameter,
                                accepted_by: born.primary(),
                            }],
                            work,
                        )?;
                    }
                    let stages = builder.written_stage_block(
                        stage_start,
                        builder.stages_len() - stage_start,
                        work,
                    )?;
                    let proof = builder.reserve_proof(work)?;
                    builder.write_proof(
                        proof,
                        b::DraftProofRow {
                            atom: born.into(),
                            primitives,
                            stages,
                            value,
                        },
                        work,
                    )?;
                    let content = if hidden {
                        b::DraftAtomContent::Empty
                    } else {
                        b::DraftAtomContent::Pool(value)
                    };
                    Ok((Some(proof), content))
                })?
            } else {
                (None, b::DraftAtomContent::Empty)
            };
            let kind = if hidden {
                AtomKind::Hidden
            } else if proof.is_some() {
                AtomKind::Transform
            } else {
                AtomKind::Copy
            };
            let born = self.writer.commit_owned(
                ticket,
                b::DraftOwnedAtom {
                    owner,
                    kind,
                    target: TargetSpan {
                        start: self.cursor,
                        end,
                    },
                    sources,
                    proof,
                    content,
                    primary,
                },
            )?;
            self.record_with_delimiter(
                born,
                owner_index,
                end,
                sources,
                false,
                matches!(
                    transform,
                    Some(RawTransform::HtmlEmphasis(_) | RawTransform::HtmlCode(_))
                ),
            )
        })();
        result.map_err(|error| self.writer.fail(error))
    }
    fn raw<'view, 'input>(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        graph: &mut graph::Graph<'view, 'input, 'id>,
        source: n::SourceToken<'view, 'input>,
        host: Host<'view, 'input>,
        owner: Owner,
        hidden: bool,
    ) -> Result<(), CoreError> {
        let result = (|| {
            let mut cursor = self
                .writer
                .with_read(|_, w| native.read(w, |a| a.source_cursor(source)))?;
            while let Some(piece) = self
                .writer
                .with_read(|_, w| native.read(w, |a| a.next_source(&mut cursor)))?
            {
                match piece.kind {
                    native::AppendKind::Copy => {
                        let mut pieces = self
                            .writer
                            .with_read(|_, w| native.source.piece_cursor(piece.p, w))?;
                        while let Some(row) = self.writer.with_read(|_, w| pieces.next(w))? {
                            let start =
                                piece.r_start + (row.parse.start() as usize - piece.p.start);
                            let end = start + row.parse.len() as usize;
                            let primary = host.primary(start, end)?;
                            let sources = self
                                .writer
                                .with_rows(|b, w| b.source_parts(&[row.source], w))?;
                            let text = native
                                .source
                                .input()
                                .text()
                                .get(row.parse.start() as usize..row.parse.end() as usize)
                                .ok_or(CoreError::InvalidProvenance)?;
                            let primitive = match row.kind {
                                InputPieceKind::Copy => None,
                                InputPieceKind::CrLf => Some(PrimitiveKind::CrLf),
                                InputPieceKind::Cr => Some(PrimitiveKind::Cr),
                                InputPieceKind::Nul => Some(PrimitiveKind::Nul),
                            };
                            self.leaf(
                                owner,
                                sources,
                                primary,
                                text,
                                primitive.map(RawTransform::N01),
                                hidden,
                            )?;
                        }
                    }
                    native::AppendKind::PartialTab { spaces, column } => {
                        let sources = self.writer.with_rows(|b, w| {
                            let start = b.sources_len();
                            native.source.p_sources(piece.p, w, |source, w| {
                                b.source_parts(&[source], w).map(|_| ())
                            })?;
                            b.written_source_block(start, b.sources_len() - start, w)
                        })?;
                        let primary = host.primary(piece.r_start, piece.r_end)?;
                        let text = "    "
                            .get(..spaces as usize)
                            .ok_or(CoreError::InvalidProvenance)?;
                        self.leaf(
                            owner,
                            sources,
                            primary,
                            text,
                            Some(RawTransform::PartialTab(
                                u32::try_from(column).map_err(|_| CoreError::ArithmeticOverflow)?,
                            )),
                            hidden,
                        )?;
                    }
                    native::AppendKind::TableEscapedPipe => {
                        let (sources, claim) = self.writer.with_rows(|b, w| {
                            w.scan(2)?;
                            if piece.r_end.checked_sub(piece.r_start) != Some(1)
                                || native.source.input().text().get(piece.p.start..piece.p.end)
                                    != Some("\\|")
                            {
                                return Err(w.fail(CoreError::InvalidProvenance));
                            }
                            let start = b.sources_len();
                            native.source.p_sources(piece.p, w, |span, w| {
                                b.source_parts(&[span], w).map(|_| ())
                            })?;
                            let sources =
                                b.written_source_block(start, b.sources_len() - start, w)?;
                            let claim = extensions::pipe_authority(
                                native,
                                graph,
                                b,
                                source,
                                piece.r_start,
                                piece.r_end,
                                w,
                            )?;
                            Ok((sources, claim))
                        })?;
                        self.leaf(
                            owner,
                            sources,
                            host.primary(piece.r_start, piece.r_end)?,
                            "|",
                            Some(RawTransform::TableEscapedPipe(claim)),
                            hidden,
                        )?;
                    }
                    native::AppendKind::CodeTerminal => return Err(CoreError::InvalidProvenance),
                }
            }
            Ok(())
        })();
        result.map_err(|error| self.writer.fail(error))
    }
    fn html_comment<'view, 'input>(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        graph: &mut graph::Graph<'view, 'input, 'id>,
        mapping: n::MappingToken<'view, 'input>,
        owner: Owner,
    ) -> Result<(), CoreError> {
        let source = self.writer.with_read(|_, w| {
            native.read(w, |access| {
                if access.mapping_len(mapping)? != 1 {
                    return Err(CoreError::InvalidProvenance);
                }
                let atom = access.mapping_atom(mapping, 0)?;
                if atom.proof.is_some() || atom.hidden_syntax || atom.bytes == 0 {
                    return Err(CoreError::InvalidProvenance);
                }
                Ok(atom.source)
            })
        })?;
        let host = self.flow_host(native, graph, source, NodeKind::HtmlComment)?;
        self.raw(native, graph, source, host, owner, true)
    }

    fn html_code<'view, 'input>(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        binding: n::BindingInfo<'view, 'input>,
        domains: b::SourceBlockRef,
        owner: Owner,
        graph: &mut graph::Graph<'view, 'input, 'id>,
    ) -> Result<(), CoreError> {
        let (count, domain) = self.writer.with_read(|_, work| {
            let syntax = binding.syntax.ok_or(CoreError::InvalidProvenance)?;
            let domain = native.read(work, |a| a.source_domain(syntax))?;
            let count = native.read(work, |a| a.mapping_len(binding.value))?;
            if !matches!(count, 2 | 3) {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            Ok((count, domain))
        })?;
        let host = Host {
            token: domain.source,
            domains,
            start: 0,
            len: domain.utf8_len,
            role: HostRole::Inline,
            phase: NativePhase::Inline,
            kind: ClaimKind::Code,
        };
        let mut next = domain.consumed_start;
        for index in 0..count {
            let (atom, occurrence) = self.writer.with_read(|_, work| {
                let atom = native.read(work, |a| a.mapping_atom(binding.value, index))?;
                let occurrence = native.read(work, |a| a.source_domain(atom.source))?;
                if !native.read(work, |a| a.same_source(domain.source, occurrence.source))?
                    || occurrence.consumed_start != next
                    || occurrence.consumed_end > domain.consumed_end
                    || atom.proof.is_some()
                {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                Ok((atom, occurrence))
            })?;
            next = occurrence.consumed_end;
            if index == 0 || index == count - 1 {
                if !atom.hidden_syntax || atom.bytes != 0 {
                    return Err(self.writer.fail(CoreError::InvalidProvenance));
                }
                let (span, raw, tag) = self.writer.with_read(|_, work| {
                    let (span, raw) = native
                        .inline_html_source_copy(atom.source, 5..=34, work)?
                        .ok_or(CoreError::InvalidProvenance)?;
                    let tag = HtmlCodeDelimiter::parse(native.html_source(span, work)?)
                        .ok_or(CoreError::InvalidProvenance)?;
                    if tag.closing != (index != 0) {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    Ok((span, raw, tag))
                })?;
                let sources = self.writer.with_rows(|b, w| b.source_parts(&[span], w))?;
                let primary = Host {
                    kind: ClaimKind::Html,
                    ..host
                }
                .primary(raw.start, raw.end)?;
                self.leaf(
                    owner,
                    sources,
                    primary,
                    "",
                    Some(RawTransform::HtmlCode(tag.parameter())),
                    true,
                )?;
            } else {
                if atom.hidden_syntax
                    || occurrence.consumed_end - occurrence.consumed_start != atom.bytes
                {
                    return Err(self.writer.fail(CoreError::InvalidProvenance));
                }
                self.raw(native, graph, atom.source, host, owner, false)?;
            }
        }
        if next != domain.consumed_end {
            return Err(self.writer.fail(CoreError::InvalidProvenance));
        }
        Ok(())
    }

    fn mapping<'view, 'input>(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        mapping: n::MappingToken<'view, 'input>,
        owner: Owner,
        graph: &mut graph::Graph<'view, 'input, 'id>,
        payload: Option<(Host<'view, 'input>, b::TextRef)>,
    ) -> Result<(), CoreError> {
        let occurrence_owner = owner;
        let Owner {
            node: owner,
            index: owner_index,
            kind,
        } = owner;
        let payload_value = payload.map(|(_, value)| value);
        let payload = payload.map(|(host, _)| host);
        let result = (|| {
            let count = self
                .writer
                .with_read(|_, w| native.read(w, |a| a.mapping_len(mapping)))?;
            for index in 0..count {
                let atom = self
                    .writer
                    .with_read(|_, w| native.read(w, |a| a.mapping_atom(mapping, index)))?;
                let host = match payload {
                    Some(host) => host,
                    None => self.flow_host(native, graph, atom.source, kind)?,
                };
                if atom.proof.is_none() {
                    self.raw(
                        native,
                        graph,
                        atom.source,
                        host,
                        occurrence_owner,
                        atom.hidden_syntax,
                    )?;
                    continue;
                }
                let (sources, start, end) = self
                    .writer
                    .with_rows(|b, w| native.sources(atom.source, b, w))?;
                let primary = host.primary(start, end)?;
                let ticket = self.writer.reserve_next()?;
                let born = ticket.born();
                let hidden = atom.hidden_syntax || atom.bytes == 0;
                let (proof, content, width) = if atom.hidden_syntax {
                    (None, b::DraftAtomContent::Empty, 0)
                } else {
                    let (proof, value) = super::proof::write(
                        native,
                        graph,
                        atom,
                        super::proof::Occurrence {
                            primary: spec(owner, sources, primary),
                            host: host.token,
                            payload_role: payload.map(|h| h.role),
                            born,
                            value: match payload_value {
                                Some(value) => Some(self.writer.with_read(|b, w| {
                                    b.text_subslice(value, self.cursor as usize, atom.bytes, w)
                                })?),
                                None => None,
                            },
                        },
                        &mut self.writer,
                    )?;
                    let width = if payload.is_some() {
                        atom.bytes as u64
                    } else {
                        self.writer.with_read(|b, w| {
                            let mut at = 0;
                            let mut width = 0u64;
                            while at < atom.bytes {
                                let chunk = b.text_chunk(value, at, w)?;
                                at += chunk.as_str().len();
                                w.scan(chunk.as_str().len() as u64)?;
                                width += chunk
                                    .as_str()
                                    .chars()
                                    .map(|c| c.len_utf16() as u64)
                                    .sum::<u64>();
                            }
                            Ok(width)
                        })?
                    };
                    (
                        Some(proof),
                        if hidden {
                            b::DraftAtomContent::Empty
                        } else {
                            b::DraftAtomContent::Pool(value)
                        },
                        width,
                    )
                };
                let end = self
                    .cursor
                    .checked_add(width)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                let born = self.writer.commit_owned(
                    ticket,
                    b::DraftOwnedAtom {
                        owner,
                        kind: if hidden {
                            AtomKind::Hidden
                        } else {
                            AtomKind::Transform
                        },
                        target: TargetSpan {
                            start: self.cursor,
                            end,
                        },
                        sources,
                        proof,
                        content,
                        primary,
                    },
                )?;
                self.record(born, owner_index, end, sources, false)?;
            }
            Ok(())
        })();
        result.map_err(|error| self.writer.fail(error))
    }
}

// Definition payloads and node payloads have distinct owners but both need a
// real node authority; the graph carries the same table-source witness cache.
#[allow(clippy::too_many_arguments)]
pub(super) fn payload<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    owner: b::DraftPayloadOwner,
    node: b::NodeRef,
    role: PayloadRole,
    value: capture::CapturedValue<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::PayloadRef, CoreError> {
    payload_impl(native, graph, builder, owner, node, role, value, None, work)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn math_payload<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    node: b::NodeRef,
    syntax: MathSyntax,
    value: capture::CapturedValue<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::PayloadRef, CoreError> {
    payload_impl(
        native,
        graph,
        builder,
        b::DraftPayloadOwner::Node(node),
        node,
        PayloadRole::MathInput,
        value,
        Some(syntax),
        work,
    )
}

#[allow(clippy::too_many_arguments)]
fn payload_impl<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    owner: b::DraftPayloadOwner,
    node: b::NodeRef,
    role: PayloadRole,
    value: capture::CapturedValue<'view, 'input>,
    math_syntax: Option<MathSyntax>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::PayloadRef, CoreError> {
    capture::call(work, |work| {
        if (role == PayloadRole::MathInput) != math_syntax.is_some() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let (mapping, text, len) = value.consume(builder, owner, node, role, work)?;
        let raw = native
            .read(work, |a| a.mapping_source(mapping))?
            .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
        let (domains, start, end) = native.sources(raw, builder, work)?;
        let reference = builder.reserve_payload(work)?;
        let mut target = Open::new(builder, work, false)?;
        let result = (|| {
            if let Some(prefix) = target
                .writer
                .with_read(|_, w| native.read(w, |a| a.mapping_prefix(mapping)))?
            {
                if math_syntax.is_some() {
                    return Err(target.writer.fail(CoreError::InvalidProvenance));
                }
                // The native prefix is anchored at the raw destination start.
                // Map from its first authored piece so a skipped quote or
                // list prefix cannot pull the empty anchor to the previous line.
                let sources = target.writer.with_rows(|b, w| {
                    let start = b.source_at(domains, 0, w)?.start();
                    b.source_parts(&[SourceSpan::new(start, start)?], w)
                })?;
                let prefix_text = target.writer.with_rows(|b, w| {
                    b.text_from_chunks(prefix.bytes, w, |at, w| {
                        native.read(w, |a| {
                            a.with_prefix_chunk_next(
                                prefix.token,
                                at,
                                prefix.bytes,
                                256,
                                |text, reads| reads.text_chunk(text),
                            )
                        })
                    })
                })?;
                let ticket = target.writer.reserve_next()?;
                let born = target.writer.commit_owned(
                    ticket,
                    b::DraftOwnedAtom {
                        owner: node,
                        kind: AtomKind::Synthetic,
                        target: TargetSpan {
                            start: 0,
                            end: prefix.bytes as u64,
                        },
                        sources,
                        proof: None,
                        content: b::DraftAtomContent::Pool(prefix_text),
                        primary: b::DraftPrimaryAuthority {
                            phase: NativePhase::Metadata,
                            kind: ClaimKind::ProjectionSeparator(match prefix.kind {
                                native::GeneratedPrefix::Http => SyntheticKind::HttpScheme,
                                native::GeneratedPrefix::Mailto => SyntheticKind::MailtoScheme,
                            }),
                            consumed: LocalUtf8Span::new(0, 0)?,
                            host_role: host_role(role),
                            host_utf8_len: 0,
                            host_domains: sources,
                        },
                    },
                )?;
                target.record(born, 0, prefix.bytes as u64, sources, false)?;
            }
            let host = Host {
                token: raw,
                domains,
                start,
                len: end - start,
                role: host_role(role),
                phase: if math_syntax.is_some() {
                    NativePhase::Inline
                } else if role == PayloadRole::FenceInput {
                    NativePhase::Block
                } else {
                    NativePhase::Metadata
                },
                kind: math_syntax.map(ClaimKind::Math).unwrap_or(
                    if role == PayloadRole::FenceInput {
                        ClaimKind::Code
                    } else {
                        ClaimKind::Payload
                    },
                ),
            };
            target.mapping(
                native,
                mapping,
                Owner {
                    node,
                    index: 0,
                    kind: NodeKind::Text,
                },
                graph,
                Some((host, text)),
            )?;
            if target.cursor != len as u64 {
                return Err(target.writer.fail(CoreError::InvalidProvenance));
            }

            Ok(())
        })();
        result.map_err(|error| target.writer.fail(error))?;
        let closed = target.close()?;
        let atoms = closed.into_block(work)?;
        builder.write_payload(
            reference,
            b::DraftPayloadRow {
                owner,
                role,
                value: text,
                domains,
                atoms: atoms.into(),
                utf8_len: len as u64,
            },
            work,
        )?;
        Ok(reference)
    })
}

pub(super) fn raw_label<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    owner: b::DraftPayloadOwner,
    node: b::NodeRef,
    raw: n::SourceToken<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::PayloadRef, CoreError> {
    let (domains, start, end) = native.sources(raw, builder, work)?;
    let len = end - start;
    let mut cursor = native.read(work, |a| a.source_cursor(raw))?;
    let mut current = None;
    let mut consumed = 0usize;
    let text = builder.text_from_chunks(len, work, |_, work| {
        if current.is_none() {
            current = native.read(work, |a| a.next_source(&mut cursor))?;
            consumed = 0;
        }
        let piece = current.ok_or(CoreError::InvalidProvenance)?;
        let value = match piece.kind {
            native::AppendKind::Copy => native
                .source
                .input()
                .text()
                .get(piece.p.start..piece.p.end)
                .ok_or(CoreError::InvalidProvenance)?,
            native::AppendKind::PartialTab { spaces, .. } => "    "
                .get(..spaces as usize)
                .ok_or(CoreError::InvalidProvenance)?,
            native::AppendKind::TableEscapedPipe => "|",
            native::AppendKind::CodeTerminal => return Err(work.fail(CoreError::InvalidProvenance)),
        };
        let chunk = b::owned_text_chunk(value, consumed, value.len(), work)?;
        consumed += chunk.as_str().len();
        if consumed == value.len() {
            current = None;
        }
        Ok(chunk)
    })?;
    let reference = builder.reserve_payload(work)?;
    let mut target = Open::new(builder, work, false)?;
    let result = (|| {
        let host = Host {
            token: raw,
            domains,
            start,
            len,
            role: HostRole::ReferenceLabel,
            phase: NativePhase::Metadata,
            kind: ClaimKind::Payload,
        };
        target.raw(
            native,
            graph,
            raw,
            host,
            Owner {
                node,
                index: 0,
                kind: NodeKind::Text,
            },
            false,
        )?;
        if target.cursor != len as u64 {
            return Err(target.writer.fail(CoreError::InvalidProvenance));
        }

        Ok(())
    })();
    result.map_err(|error| target.writer.fail(error))?;
    let closed = target.close()?;
    let atoms = closed.into_block(work)?;
    builder.write_payload(
        reference,
        b::DraftPayloadRow {
            owner,
            role: PayloadRole::ReferenceLabel,
            value: text,
            domains,
            atoms: atoms.into(),
            utf8_len: len as u64,
        },
        work,
    )?;
    Ok(reference)
}

impl<'id> Open<'_, '_, 'id, '_> {
    fn callout_title(&mut self, owner: Owner, detail: b::DetailRef) -> Result<(), CoreError> {
        let title = self.writer.with_rows(|builder, work| {
            let DetailRow::Callout { kind, .. } = builder.read_detail(detail, work)? else {
                return Err(work.fail(CoreError::InvalidProvenance));
            };
            let label = kind.title();
            let at = builder
                .node_initial(owner.node, work)?
                .ok_or(CoreError::InvalidProvenance)?
                .start();
            Ok(Generated {
                sources: builder.source_parts(&[SourceSpan::new(at, at)?], work)?,
                text: builder.text(label, work)?,
                units: label.encode_utf16().count() as u64,
                kind: SyntheticKind::CalloutTitle,
                phase: NativePhase::Structure,
            })
        })?;
        self.append_generated(owner.node, owner.index, title)
            .map(|_| ())
    }

    fn footnote_text(&mut self, owner: Owner, detail: b::DetailRef) -> Result<(), CoreError> {
        let (sources, text, units, kind) = self.writer.with_rows(|builder, work| {
            let (mut number, kind) = match builder.read_detail(detail, work)? {
                DetailRow::FootnoteReference { ordinal, .. } => {
                    (ordinal, SyntheticKind::FootnoteNumber)
                }
                DetailRow::FootnoteDefinition { ordinal, .. } => {
                    (ordinal, SyntheticKind::FootnoteLabel)
                }
                DetailRow::FootnoteBacklink { occurrence, .. } => {
                    (occurrence, SyntheticKind::FootnoteBacklink)
                }
                _ => return Err(work.fail(CoreError::InvalidProvenance)),
            };
            if number == 0 {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let mut digits = [0_u8; 14];
            let mut start = digits.len();
            let prefix = if kind == SyntheticKind::FootnoteBacklink {
                if number == 1 {
                    number = 0;
                    "↩"
                } else {
                    " ↩"
                }
            } else {
                ""
            };
            if kind == SyntheticKind::FootnoteLabel {
                start -= 1;
                digits[start] = b'.';
            }
            while number != 0 {
                start -= 1;
                digits[start] = b'0' + (number % 10) as u8;
                number /= 10;
            }
            digits[start - prefix.len()..start].copy_from_slice(prefix.as_bytes());
            start -= prefix.len();
            let text =
                std::str::from_utf8(&digits[start..]).map_err(|_| CoreError::InvalidProvenance)?;
            let at = builder
                .node_initial(owner.node, work)?
                .ok_or(CoreError::InvalidProvenance)?
                .start();
            let sources = builder.source_parts(&[SourceSpan::new(at, at)?], work)?;
            Ok((
                sources,
                builder.text(text, work)?,
                text.encode_utf16().count() as u64,
                kind,
            ))
        })?;
        self.append_generated(
            owner.node,
            owner.index,
            Generated {
                sources,
                text,
                units,
                kind,
                phase: NativePhase::Structure,
            },
        )
        .map(|_| ())
    }

    fn toc_entry<'view, 'input>(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        graph: &graph::Graph<'view, 'input, 'id>,
        owner: Owner,
        parent: usize,
        detail: b::DetailRef,
    ) -> Result<(), CoreError> {
        let (heading, depth, label) = self.writer.with_rows(|builder, work| {
            let plan = graph
                .table_of_contents
                .as_ref()
                .ok_or(CoreError::InvalidProvenance)?;
            let ordinal = owner
                .index
                .checked_sub(parent + 1)
                .ok_or(CoreError::InvalidProvenance)?;
            let entry = plan.headings.get(ordinal, work)?;
            let heading = graph
                .member_for_lineage(entry.lineage, native, work)?
                .ok_or(CoreError::InvalidProvenance)?;
            let heading = graph.nodes.get(heading, work)?.node;
            let owner_row = builder.graph_node(owner.node, work)?;
            let heading_row = builder.graph_node(heading, work)?;
            if heading_row.kind != NodeKind::Heading || heading_row.flow != owner_row.flow {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let anchor = builder
                .node_initial(owner.node, work)?
                .ok_or(CoreError::InvalidProvenance)?;
            work.scan_chunked(entry.bytes)?;
            let label = Generated {
                sources: builder.source_parts(&[anchor], work)?,
                text: entry.label,
                units: entry.utf16,
                kind: SyntheticKind::TableOfContentsLabel,
                phase: NativePhase::Structure,
            };
            Ok((heading, entry.depth, label))
        })?;
        let atom = self.append_generated(owner.node, owner.index, label)?;
        self.writer.with_rows(|builder, work| {
            builder
                .write_detail(
                    detail,
                    b::DraftDetailRow::TableOfContentsEntry {
                        heading,
                        depth,
                        claim: atom.primary(),
                    },
                    work,
                )
                .map(|_| ())
        })
    }

    fn synthetic(
        &mut self,
        owner: b::NodeRef,
        owner_index: usize,
        source: u64,
        kind: SyntheticKind,
        phase: NativePhase,
        values: &mut [Option<b::TextRef>; 3],
    ) -> Result<(), CoreError> {
        let result = (|| {
            let (sources, text) = self.writer.with_rows(|b, w| {
                let sources = b.source_parts(&[SourceSpan::new(source, source)?], w)?;
                let slot = match kind {
                    SyntheticKind::CellTab => 1,
                    SyntheticKind::TaskLabelSpace => 2,
                    _ => 0,
                };
                let text = match values[slot] {
                    Some(v) => v,
                    None => {
                        let v = b.text(
                            extensions::Separator {
                                kind,
                                owner: extensions::SeparatorOwner::Current,
                            }
                            .text(),
                            w,
                        )?;
                        values[slot] = Some(v);
                        v
                    }
                };
                Ok((sources, text))
            })?;
            self.append_generated(
                owner,
                owner_index,
                Generated {
                    sources,
                    text,
                    units: 1,
                    kind,
                    phase,
                },
            )
            .map(|_| ())
        })();
        result.map_err(|error| self.writer.fail(error))
    }

    fn append_generated(
        &mut self,
        owner: b::NodeRef,
        index: usize,
        text: Generated,
    ) -> Result<b::BornAtom, CoreError> {
        let end = self
            .cursor
            .checked_add(text.units)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let ticket = self.writer.reserve_next()?;
        let born = self.writer.commit_owned(
            ticket,
            b::DraftOwnedAtom {
                owner,
                kind: AtomKind::Synthetic,
                target: TargetSpan {
                    start: self.cursor,
                    end,
                },
                sources: text.sources,
                proof: None,
                content: b::DraftAtomContent::Pool(text.text),
                primary: b::DraftPrimaryAuthority {
                    phase: text.phase,
                    kind: ClaimKind::ProjectionSeparator(text.kind),
                    consumed: LocalUtf8Span::new(0, 0)?,
                    host_role: if text.phase == NativePhase::Block {
                        HostRole::Block
                    } else {
                        HostRole::Structure
                    },
                    host_utf8_len: 0,
                    host_domains: text.sources,
                },
            },
        )?;
        self.record(born, index, end, text.sources, false)?;
        Ok(born)
    }
}

struct Generated {
    sources: b::SourceBlockRef,
    text: b::TextRef,
    units: u64,
    kind: SyntheticKind,
    phase: NativePhase,
}
pub(super) fn flows<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    objects: &BuildRows<'id, extensions::ObjectBinding>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    flows_impl(
        native,
        graph,
        builder,
        objects,
        work,
        #[cfg(test)]
        None,
    )
}

#[derive(Clone, Copy)]
struct FlowFrame {
    node: usize,
    next: Option<usize>,
    begin: u64,
    envelope: SourceSpan,
    separators: bool,
}
impl BuildValue for FlowFrame {}

pub(super) fn flows_impl<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    objects: &BuildRows<'id, extensions::ObjectBinding>,
    work: &mut OwnedWork<'id, '_>,
    #[cfg(test)] mut case: Option<&mut super::hidden::owner_tests::Case>,
) -> Result<(), CoreError> {
    let hidden = super::hidden::Index::new(native, work)?;
    let mut owners = super::hidden::Owners::new();
    let mut synthetic = [None; 3];
    let mut object_at = 0;
    for flow_index in 0..graph.flows.len() {
        let flow = graph.flows.get(flow_index, work)?;
        let root = graph.nodes.get(flow.root(), work)?;
        let mut target = Open::new(builder, work, true)?;
        target.relocated = flow_index == 0 && graph.reordered_footnotes;
        let result = (|| {
            #[cfg(test)]
            if let Some(case) = case.as_deref_mut() {
                target.writer.with_read(|_, w| {
                    case.at(
                        super::hidden::owner_tests::Stage::Positive,
                        flow_index,
                        owners.stats(),
                        w,
                    )
                })?;
            }
            let mut stack = target.writer.with_read(|_, w| {
                BuildRows::<FlowFrame>::new(w.require_model_limits()?.depth, LimitKind::Depth, w)
            })?;
            let mut top = 0_usize;
            let mut next = Some((flow.root(), flow.object.is_none()));
            let mut previous_block = false;
            loop {
                if let Some((index, separators)) = next {
                    let (member, node, binding) = target.writer.with_read(|builder, w| {
                        let member = graph.nodes.get(index, w)?;
                        let node = builder.graph_node(member.node, w)?;
                        let binding = native.read(w, |a| a.binding(member.lineage))?;
                        Ok((member, node, binding))
                    })?;
                    if matches!(
                        node.kind,
                        NodeKind::FootnoteFooter
                            | NodeKind::FootnoteBacklink
                            | NodeKind::TableOfContentsEntry
                    ) {
                        // The footnote footer follows all real children. TOC
                        // labels use the end of their authored marker.
                        target.writer.with_rows(|builder, w| {
                            let parent = stack
                                .get(top.checked_sub(1).ok_or(CoreError::InvalidProvenance)?, w)?;
                            if node.parent != Some(parent.node) {
                                return Err(w.fail(CoreError::InvalidProvenance));
                            }
                            let end = parent.envelope.end();
                            let anchor = SourceSpan::new(end, end)?;
                            builder.set_node_initial(member.node, Some(anchor), w)?;
                            if node.kind == NodeKind::FootnoteBacklink {
                                super::metadata::footnote_backlink(
                                    native,
                                    graph,
                                    graph.backlink(index, w)?,
                                    anchor,
                                    builder,
                                    w,
                                )?;
                            }
                            Ok(())
                        })?;
                    }
                    let position = target
                        .writer
                        .with_read(|builder, w| graph.position(index, native, builder, w))?;
                    let display = !member.alt_root && extensions::block(node.kind);
                    let separator = extensions::before(
                        node.kind,
                        position,
                        separators,
                        member.output,
                        previous_block,
                    )?;
                    previous_block |=
                        extensions::marks_block(node.kind, position, separators, member.output);
                    if let Some(separator) = separator {
                        let (owner, owner_index, at) = target.writer.with_read(|builder, w| {
                            graph.separator_origin(index, separator, native, builder, w)
                        })?;
                        target.synthetic(
                            owner,
                            owner_index,
                            at,
                            separator.kind,
                            NativePhase::Structure,
                            &mut synthetic,
                        )?;
                    }
                    // Separators precede the node's interval. An Alt root skips
                    // its Image binding but still traverses all real children.
                    let begin = target.cursor;
                    if !member.alt_root {
                        if member.callout_title {
                            let detail = target.writer.with_read(|_, w| {
                                let parent = graph
                                    .nodes
                                    .get(node.parent.ok_or(CoreError::InvalidProvenance)?, w)?;
                                graph
                                    .binding(parent, w)?
                                    .and_then(graph::Metadata::detail)
                                    .ok_or(CoreError::InvalidProvenance)
                            })?;
                            target.callout_title(
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                                detail,
                            )?;
                        } else if let Some(object) = target.writer.with_read(|_, w| {
                            Ok(graph.binding(member, w)?.and_then(graph::Metadata::object))
                        })? {
                            let object_binding =
                                target.writer.with_read(|_, w| objects.get(object_at, w))?;
                            if object_binding.node() != member.node
                                || object_binding.object() != object
                            {
                                return Err(target.writer.fail(CoreError::InvalidProvenance));
                            }
                            let end = target
                                .cursor
                                .checked_add(1)
                                .ok_or(CoreError::ArithmeticOverflow)?;
                            let ticket = target.writer.reserve_next()?;
                            let born = target.writer.commit_shared(
                                ticket,
                                b::DraftAtomFields {
                                    owner: member.node,
                                    kind: AtomKind::Object,
                                    target: TargetSpan {
                                        start: target.cursor,
                                        end,
                                    },
                                    sources: object_binding.sources(),
                                    proof: None,
                                    content: b::DraftAtomContent::Object(object),
                                },
                                object_binding.claim(),
                            )?;
                            target.writer.with_rows(|b, w| {
                                b.write_object(object, object_binding.draft(born.into()), w)
                            })?;
                            target.record(born, index, end, object_binding.sources(), false)?;
                            object_at += 1;
                        } else if matches!(
                            node.kind,
                            NodeKind::FootnoteReference
                                | NodeKind::FootnoteDefinition
                                | NodeKind::FootnoteBacklink
                        ) {
                            let detail = target.writer.with_read(|_, w| {
                                if node.kind == NodeKind::FootnoteBacklink {
                                    Ok(graph.backlink(index, w)?.detail)
                                } else {
                                    graph
                                        .binding(member, w)?
                                        .and_then(graph::Metadata::detail)
                                        .ok_or(CoreError::InvalidProvenance)
                                }
                            })?;
                            target.footnote_text(
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                                detail,
                            )?;
                        } else if node.kind == NodeKind::TableOfContentsEntry {
                            let parent = target.writer.with_read(|_, w| {
                                stack
                                    .get(top.checked_sub(1).ok_or(CoreError::InvalidProvenance)?, w)
                                    .map(|frame| frame.node)
                            })?;
                            target.toc_entry(
                                native,
                                graph,
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                                parent,
                                node.detail.ok_or(CoreError::InvalidProvenance)?,
                            )?;
                        } else if let Some(domains) = target.writer.with_read(|_, w| {
                            if node.kind == NodeKind::InlineCode && member.has_metadata() {
                                graph.html_code_host(binding.id, w)
                            } else {
                                Ok(None)
                            }
                        })? {
                            target.html_code(
                                native,
                                binding,
                                domains,
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                                graph,
                            )?;
                        } else if let Some((parameter, domains)) =
                            target.writer.with_read(|_, w| {
                                if node.kind == NodeKind::HtmlInline {
                                    graph.html_emphasis(binding.id, w)
                                } else {
                                    Ok(None)
                                }
                            })?
                        {
                            let (span, raw, domain) = target.writer.with_read(|_, w| {
                                let (token, span, raw) = native
                                    .inline_html_copy(
                                        binding.value,
                                        3..=crate::ir::HtmlEmphasis::MAX_DELIMITER_BYTES,
                                        w,
                                    )?
                                    .ok_or(CoreError::InvalidProvenance)?;
                                let domain = native.read(w, |a| a.source_domain(token))?;
                                Ok((span, raw, domain))
                            })?;
                            let (role, phase, kind) = flavor(NodeKind::HtmlInline);
                            let host = Host {
                                token: domain.source,
                                domains,
                                start: 0,
                                len: domain.utf8_len,
                                role,
                                phase,
                                kind,
                            };
                            let sources =
                                target.writer.with_rows(|b, w| b.source_parts(&[span], w))?;
                            target.leaf(
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                                sources,
                                host.primary(raw.start, raw.end)?,
                                "",
                                Some(RawTransform::HtmlEmphasis(parameter)),
                                true,
                            )?;
                        } else if node.kind == NodeKind::HtmlComment {
                            target.html_comment(
                                native,
                                graph,
                                binding.value,
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                            )?;
                        } else if node.kind == NodeKind::HardBreak && member.has_metadata() {
                            let (token, source, raw) = target.writer.with_read(|_, w| {
                                native
                                    .inline_html_break(binding.value, w)?
                                    .ok_or_else(|| w.fail(CoreError::InvalidProvenance))
                            })?;
                            let host =
                                target.flow_host(native, graph, token, NodeKind::HtmlInline)?;
                            let sources = target
                                .writer
                                .with_rows(|b, w| b.source_parts(&[source], w))?;
                            target.leaf(
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                                sources,
                                host.primary(raw.start, raw.end)?,
                                "\n",
                                Some(RawTransform::HtmlBreak),
                                false,
                            )?;
                        } else if node.kind != NodeKind::FootnoteFooter {
                            target.mapping(
                                native,
                                binding.value,
                                Owner {
                                    node: member.node,
                                    index,
                                    kind: node.kind,
                                },
                                graph,
                                None,
                            )?;
                            if binding.terminal_synthetic {
                                target.synthetic(
                                    member.node,
                                    index,
                                    native.source.input().source().text().len() as u64,
                                    SyntheticKind::CodeTerminalLineFeed,
                                    NativePhase::Block,
                                    &mut synthetic,
                                )?;
                            }
                        }
                    }
                    target.writer.with_read(|builder, w| {
                        let envelope =
                            if let Some(initial) = builder.node_initial(member.node, w)? {
                                initial
                            } else if position == extensions::Position::TaskLabel {
                                // The retained Paragraph's BlockStart is before the
                                // task marker. Its label begins after the real spacer.
                                let (_, _, at) = graph.separator_origin(
                                    index,
                                    extensions::Separator {
                                        kind: SyntheticKind::TaskLabelSpace,
                                        owner: extensions::SeparatorOwner::TaskCheckbox,
                                    },
                                    native,
                                    builder,
                                    w,
                                )?;
                                SourceSpan::new(at, at)?
                            } else {
                                let at = native.source.p_anchor(binding.start_p.unwrap_or(0), w)?;
                                SourceSpan::new(at, at)?
                            };
                        let frame = FlowFrame {
                            node: index,
                            next: graph.first(index, w)?,
                            begin,
                            envelope,
                            separators: separators
                                && (!display || node.kind == NodeKind::FootnoteDefinition),
                        };
                        if top == stack.len() {
                            stack.push(frame, w)?;
                        } else {
                            stack.replace(top, frame, w)?;
                        }
                        Ok(())
                    })?;
                    top += 1;
                    #[cfg(test)]
                    if target.cursor > 0
                        && let Some(case) = case.as_deref_mut()
                    {
                        assert!(top > 0 && stack.page_count() > 0);
                        target.writer.with_read(|builder, w| {
                            // initial() requires the actual unfinished root slot.
                            builder.node_initial(root.node, w)?;
                            case.at(
                                super::hidden::owner_tests::Stage::DfsEnter,
                                flow_index,
                                owners.stats(),
                                w,
                            )
                        })?;
                    }
                }
                let mut frame = target.writer.with_read(|_, w| stack.get(top - 1, w))?;
                if let Some(child) = frame.next {
                    target.writer.with_read(|builder, w| {
                        frame.next = graph.next(child, builder, w)?;
                        stack.replace(top - 1, frame, w)
                    })?;
                    next = Some((child, frame.separators));
                } else {
                    let end = target.cursor;
                    target.writer.with_rows(|builder, w| {
                        let member = graph.nodes.get(frame.node, w)?;
                        let node = builder.graph_node(member.node, w)?;
                        if node.flow != flow_index {
                            return Err(w.fail(CoreError::InvalidProvenance));
                        }
                        let parent = node
                            .parent
                            .map(|i| graph.nodes.get(i, w).map(|r| r.node))
                            .transpose()?;
                        let first_child = graph
                            .first(frame.node, w)?
                            .map(|i| graph.nodes.get(i, w).map(|r| r.node))
                            .transpose()?;
                        let next_sibling = graph
                            .next(frame.node, builder, w)?
                            .map(|i| graph.nodes.get(i, w).map(|r| r.node))
                            .transpose()?;
                        let detail = if node.kind == NodeKind::FootnoteBacklink {
                            Some(graph.backlink(frame.node, w)?.detail)
                        } else if node.kind == NodeKind::TableOfContentsEntry {
                            node.detail
                        } else {
                            graph.binding(member, w)?.and_then(graph::Metadata::detail)
                        };
                        builder.write_node(
                            member.node,
                            b::DraftNodeRow {
                                parent,
                                first_child,
                                next_sibling,
                                flow: flow.reference,
                                kind: node.kind,
                                detail,
                                source_envelope: frame.envelope,
                                logical: LogicalSpan::new(frame.begin, end)?,
                            },
                            w,
                        )?;
                        Ok(())
                    })?;
                    #[cfg(test)]
                    if top > 1
                        && target.cursor > 0
                        && let Some(case) = case.as_deref_mut()
                    {
                        assert!(stack.page_count() > 0);
                        target.writer.with_read(|builder, w| {
                            let child = graph.nodes.get(frame.node, w)?.node;
                            builder.read_node(child, w)?;
                            let ancestor = stack.get(top - 2, w)?;
                            let parent = graph.nodes.get(ancestor.node, w)?.node;
                            builder.node_initial(parent, w)?;
                            case.at(
                                super::hidden::owner_tests::Stage::DfsExit,
                                flow_index,
                                owners.stats(),
                                w,
                            )
                        })?;
                    }
                    top -= 1;
                    if top == 0 {
                        break;
                    }
                    target.writer.with_read(|_, w| {
                        let mut parent = stack.get(top - 1, w)?;
                        parent.envelope = SourceSpan::new(
                            parent.envelope.start().min(frame.envelope.start()),
                            parent.envelope.end().max(frame.envelope.end()),
                        )?;
                        stack.replace(top - 1, parent, w)
                    })?;
                    next = None;
                }
            }
            // Hidden needs every final interval of this flow. The DFS pages
            // have no later consumer and are physically retired first.
            drop(stack);
            let width = target.cursor;
            #[cfg(test)]
            if let Some(case) = case.as_deref_mut() {
                target.writer.with_read(|builder, w| {
                    for index in flow.root()..root.end() {
                        builder.read_node(graph.nodes.get(index, w)?.node, w)?;
                    }
                    if flow_index > 0 {
                        let prior = graph.flows.get(flow_index - 1, w)?;
                        builder.read_node(graph.nodes.get(prior.root(), w)?.node, w)?;
                    }
                    if flow_index + 1 < graph.flows.len() {
                        let next_flow = graph.flows.get(flow_index + 1, w)?;
                        builder.node_initial(graph.nodes.get(next_flow.root(), w)?.node, w)?;
                    }
                    case.at(
                        super::hidden::owner_tests::Stage::BeforeHidden,
                        flow_index,
                        owners.stats(),
                        w,
                    )
                })?;
            }
            super::hidden::emit(
                native,
                graph,
                flow_index,
                &hidden,
                &mut owners,
                &mut target,
                #[cfg(test)]
                case.as_deref_mut(),
            )?;

            Ok(width)
        })();
        let width = result.map_err(|error| target.writer.fail(error))?;
        #[cfg(test)]
        if let Some(case) = case.as_deref_mut() {
            target.writer.with_read(|_, w| {
                case.at(
                    super::hidden::owner_tests::Stage::BeforeClose,
                    flow_index,
                    owners.stats(),
                    w,
                )
            })?;
        }
        // The final flow has no later cache consumer. Release its physical pages
        // before allocating the closing permutation, as on the original path.
        if flow_index + 1 == graph.flows.len() {
            owners.retire();
            #[cfg(test)]
            if let Some(case) = case.as_deref_mut() {
                target.writer.with_read(|_, w| {
                    case.at(
                        super::hidden::owner_tests::Stage::Retired,
                        flow_index,
                        owners.stats(),
                        w,
                    )
                })?;
            }
        }
        let closed = target.close_impl(
            #[cfg(test)]
            case.as_deref_mut()
                .map(|case| (case, flow_index, owners.stats())),
        )?;
        let atoms = closed.into_block(work)?;
        builder.write_flow(
            flow.reference,
            b::DraftFlowRow {
                scope: match flow.object {
                    None => b::DraftFlowScope::Main,
                    Some(object) => b::DraftFlowScope::ImageAlt { object },
                },
                root: root.node,
                domains: flow.domains,
                atoms: atoms.into(),
                logical_utf16: width,
            },
            work,
        )?;
    }
    if object_at != objects.len() {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    Ok(())
}
