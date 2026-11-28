// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One occurrence's actual native ancestry, written directly into final rows.
//! Only the bounded DFS stack is temporary; no Unit/step recipe store exists.

use super::*;
use crate::memory::BuildRows;

#[derive(Clone, Copy)]
struct Frame<'view, 'input> {
    atom: n::MappingAtom<'view, 'input>,
    proof: Option<n::ProofInfo<'view, 'input>>,
    next: usize,
    bytes: usize,
    base: usize,
    root: bool,
}

const INLINE_FRAMES: usize = 4;
const _: () = assert!(
    size_of::<[Option<Frame<'static, 'static>>; INLINE_FRAMES]>()
        <= crate::policy::CHECKPOINT_BYTES as usize
);

/// Shallow native transformations need no allocation. Deeper ancestry keeps
/// the same explicit, metered stack; only its spill pages own heap storage.
struct Stack<'id, 'view, 'input> {
    inline: [Option<Frame<'view, 'input>>; INLINE_FRAMES],
    spill: BuildRows<'id, Frame<'view, 'input>>,
    limit: u64,
}

impl<'id, 'view, 'input> Stack<'id, 'view, 'input> {
    fn new(limit: u64, work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        work.copy(size_of::<[Option<Frame<'view, 'input>>; INLINE_FRAMES]>() as u64)?;
        Ok(Self {
            inline: [None; INLINE_FRAMES],
            spill: BuildRows::new(limit, LimitKind::ProofRecords, work)?,
            limit,
        })
    }

    fn get(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Frame<'view, 'input>, CoreError> {
        if index < INLINE_FRAMES {
            work.events(1)?;
            work.scan(size_of::<Option<Frame<'view, 'input>>>() as u64)?;
            work.copy(size_of::<Frame<'view, 'input>>() as u64)?;
            self.inline[index].ok_or_else(|| work.fail(CoreError::InvalidProvenance))
        } else {
            self.spill.get(index - INLINE_FRAMES, work)
        }
    }

    fn set(
        &mut self,
        index: usize,
        frame: Frame<'view, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let required = index.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
        work.check_limit(LimitKind::ProofRecords, required as u64, self.limit)?;
        if index < INLINE_FRAMES {
            work.events(1)?;
            work.copy(size_of::<Option<Frame<'view, 'input>>>() as u64)?;
            self.inline[index] = Some(frame);
        } else {
            let at = index - INLINE_FRAMES;
            if at == self.spill.len() {
                self.spill.push(frame, work)?;
            } else {
                self.spill.replace(at, frame, work)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Occurrence<'view, 'input> {
    pub(super) primary: b::DraftAuthoritySpec,
    pub(super) host: n::SourceToken<'view, 'input>,
    pub(super) payload_role: Option<HostRole>,
    pub(super) born: b::BornAtom,
    /// An already written final payload slice at this mapping occurrence's
    /// actual output offset. Flow occurrences have no prewritten value.
    pub(super) value: Option<b::TextRef>,
}

#[derive(Clone, Copy)]
struct ProofTarget<'view, 'input> {
    occurrence: Occurrence<'view, 'input>,
    source: n::SourceToken<'view, 'input>,
    value: b::TextRef,
}

/// All authority arguments describe this actual target occurrence. Matching
/// source geometry alone never grants the primary claim to another stage.
pub(super) fn write<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    atom: n::MappingAtom<'view, 'input>,
    occurrence: Occurrence<'view, 'input>,
    target: &mut b::TargetWriter<'_, '_, 'id, '_>,
) -> Result<(b::ProofRef, b::TextRef), CoreError> {
    target.with_rows(|builder, work| {
        let root = atom.proof.ok_or(CoreError::InvalidProvenance)?;
        let info = native.read(work, |access| access.proof(root))?;
        if atom.hidden_syntax || info.output_len != atom.bytes {
            return Err(CoreError::InvalidProvenance);
        }
        let value = match occurrence.value {
            Some(value) => builder.text_subslice(value, 0, info.output_len, work)?,
            None => proof_text(native, root, 0, info.output_len, builder, work)?,
        };
        let target = ProofTarget {
            occurrence,
            source: atom.source,
            value,
        };
        let primitive_start = builder.primitives_len();
        let stage_start = builder.stages_len();
        write_ancestry(native, graph, atom, info, target, builder, work)?;
        let primitives = builder.written_primitive_block(
            primitive_start,
            builder.primitives_len() - primitive_start,
            work,
        )?;
        let stages =
            builder.written_stage_block(stage_start, builder.stages_len() - stage_start, work)?;
        let proof = builder.reserve_proof(work)?;
        builder.write_proof(
            proof,
            b::DraftProofRow {
                atom: occurrence.born.into(),
                primitives,
                stages,
                value,
            },
            work,
        )?;
        Ok((proof, value))
    })
}

fn write_ancestry<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    atom: n::MappingAtom<'view, 'input>,
    info: n::ProofInfo<'view, 'input>,
    target: ProofTarget<'view, 'input>,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    // A single raw input has no pending ancestors to retain or revisit.
    // Entity, escape and newline leaves use the same source/stage writers.
    if native.read(work, |access| access.mapping_len(info.input))? == 1 {
        let child = native.read(work, |access| access.mapping_atom(info.input, 0))?;
        if child.proof.is_none() {
            write_raw(
                native,
                graph,
                child,
                0,
                target.occurrence.primary.owner,
                builder,
                work,
            )?;
            return write_stage(
                native,
                Frame {
                    atom,
                    proof: Some(info),
                    next: 1,
                    bytes: child.bytes,
                    base: 0,
                    root: true,
                },
                info,
                atom.proof.ok_or(CoreError::InvalidProvenance)?,
                target,
                builder,
                work,
            );
        }
    }
    let limit = work
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .proof_records;
    let mut stack = Stack::new(limit, work)?;
    stack.set(
        0,
        Frame {
            atom,
            proof: Some(info),
            next: 0,
            bytes: 0,
            base: 0,
            root: true,
        },
        work,
    )?;
    let mut depth = 1;
    while depth != 0 {
        work.events(1)?;
        let mut frame = stack.get(depth - 1, work)?;
        if let Some(token) = frame.atom.proof {
            let current = match frame.proof {
                Some(info) => info,
                None => native.read(work, |access| access.proof(token))?,
            };
            if current.output_len != frame.atom.bytes {
                return Err(CoreError::InvalidProvenance);
            }
            let count = native.read(work, |access| access.mapping_len(current.input))?;
            if frame.next < count {
                let child = native.read(work, |access| {
                    access.mapping_atom(current.input, frame.next)
                })?;
                let base = frame
                    .base
                    .checked_add(frame.bytes)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                frame.proof = Some(current);
                frame.next += 1;
                frame.bytes = frame
                    .bytes
                    .checked_add(child.bytes)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                stack.set(depth - 1, frame, work)?;
                let child = Frame {
                    atom: child,
                    proof: None,
                    next: 0,
                    bytes: 0,
                    base,
                    root: false,
                };
                stack.set(depth, child, work)?;
                depth = depth.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
            } else {
                write_stage(native, frame, current, token, target, builder, work)?;
                depth -= 1;
            }
        } else {
            write_raw(
                native,
                graph,
                frame.atom,
                frame.base,
                target.occurrence.primary.owner,
                builder,
                work,
            )?;
            depth -= 1;
        }
    }
    drop(stack);
    Ok(())
}

fn proof_text<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    token: n::ProofToken<'view, 'input>,
    start: usize,
    end: usize,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::TextRef, CoreError> {
    let len = end.checked_sub(start).ok_or(CoreError::InvalidProvenance)?;
    builder.text_from_chunks(len, work, |offset, work| {
        let at = start
            .checked_add(offset)
            .ok_or(CoreError::ArithmeticOverflow)?;
        native.read(work, |access| {
            access.with_proof_chunk_next(token, at, end, 256, |text, reads| reads.text_chunk(text))
        })
    })
}

fn write_stage<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    frame: Frame<'view, 'input>,
    info: n::ProofInfo<'view, 'input>,
    token: n::ProofToken<'view, 'input>,
    target: ProofTarget<'view, 'input>,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let Occurrence {
        primary,
        host: primary_host,
        payload_role,
        born,
        value: _,
    } = target.occurrence;
    let suffix = frame
        .bytes
        .checked_sub(info.local_end)
        .ok_or(CoreError::InvalidProvenance)?;
    let output_end = info
        .output_len
        .checked_sub(suffix)
        .ok_or(CoreError::InvalidProvenance)?;
    let output = if frame.root && info.local_start == 0 && output_end == info.output_len {
        target.value
    } else {
        proof_text(native, token, info.local_start, output_end, builder, work)?
    };
    let (kind, mut claim_kind) = stage_kind(native, info.kind, frame.atom.source, work)?;
    if payload_role == Some(HostRole::MathInput)
        && matches!(
            kind,
            StageKind::CodeLineEndingToSpace | StageKind::CodeEdgeTrim
        )
    {
        if !matches!(
            primary.kind,
            ClaimKind::Math(MathSyntax::DollarInline | MathSyntax::CodeInline)
        ) || info.host.role != n::HostRole::MathInput
        {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        claim_kind = primary.kind;
    }
    let role = payload_role.unwrap_or_else(|| host_role(info.host.role));
    let phase = match payload_role {
        Some(HostRole::MathInput) => NativePhase::Inline,
        Some(HostRole::FenceInput) => NativePhase::Block,
        Some(_) => NativePhase::Metadata,
        None => phase(info.host.phase),
    };
    let consumed = LocalUtf8Span::new(
        info.host.consumed_start as u64,
        info.host.consumed_end as u64,
    )?;
    let same_source = native.read(work, |access| {
        access.same_source(target.source, frame.atom.source)
    })?;
    let same_host = native.read(work, |access| {
        access.same_source(primary_host, info.host.source)
    })?;
    let accepted_by = if same_source
        && same_host
        && primary.kind == claim_kind
        && primary.phase == phase
        && primary.consumed == consumed
        && primary.host.role == role
        && primary.host.utf8_len == info.host.utf8_len as u64
    {
        born.primary()
    } else {
        let sources = if same_source {
            primary.sources
        } else {
            native.sources(frame.atom.source, builder, work)?.0
        };
        let domains = if same_host {
            primary.host.domains
        } else {
            native.sources(info.host.source, builder, work)?.0
        };
        builder.write_extra_authority(
            b::DraftAuthoritySpec {
                owner: primary.owner,
                host: b::DraftHostRow {
                    owner: primary.owner,
                    role,
                    utf8_len: info.host.utf8_len as u64,
                    domains,
                },
                phase,
                kind: claim_kind,
                consumed,
                sources,
            },
            work,
        )?
    };
    let start = frame
        .base
        .checked_add(info.local_start)
        .ok_or(CoreError::ArithmeticOverflow)?;
    let end = frame
        .base
        .checked_add(info.local_end)
        .ok_or(CoreError::ArithmeticOverflow)?;
    builder.stage_parts(
        &[b::DraftStageRow {
            kind,
            input: LocalUtf8Span::new(start as u64, end as u64)?,
            output,
            parameter: 0,
            accepted_by,
        }],
        work,
    )?;
    Ok(())
}

// Keep the origin and owner explicit. The graph supplies
// the accepted table-pipe witness and must share this operation's cache.
#[allow(clippy::too_many_arguments)]
fn write_raw<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    atom: n::MappingAtom<'view, 'input>,
    base: usize,
    owner: b::NodeRef,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    if atom.hidden_syntax {
        return Err(CoreError::InvalidProvenance);
    }
    let mut cursor = native.read(work, |access| access.source_cursor(atom.source))?;
    let mut bytes = 0_usize;
    while let Some(piece) = native.read(work, |access| access.next_source(&mut cursor))? {
        let mut tab_source = None;
        native.source.p_pieces(piece.p, work, |part, work| {
            let kind = match part.kind {
                InputPieceKind::Copy => PrimitiveKind::Copy,
                InputPieceKind::CrLf => PrimitiveKind::CrLf,
                InputPieceKind::Cr => PrimitiveKind::Cr,
                InputPieceKind::Nul => PrimitiveKind::Nul,
            };
            builder.primitive_parts(
                &[PrimitiveRow {
                    source: part.source,
                    kind,
                }],
                work,
            )?;
            if matches!(piece.kind, native::AppendKind::PartialTab { .. })
                && (kind != PrimitiveKind::Copy || tab_source.replace(part.source).is_some())
            {
                return Err(CoreError::InvalidProvenance);
            }
            Ok(())
        })?;
        match piece.kind {
            native::AppendKind::Copy => {}
            native::AppendKind::TableEscapedPipe => {
                if piece.r_end.checked_sub(piece.r_start) != Some(1)
                    || native.source.input().text().get(piece.p.start..piece.p.end) != Some("\\|")
                {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                work.scan(2)?;
                let accepted_by = extensions::pipe_authority(
                    native,
                    graph,
                    builder,
                    atom.source,
                    piece.r_start,
                    piece.r_end,
                    work,
                )?;
                let output = builder.text("|", work)?;
                let start = base
                    .checked_add(bytes)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                let end = start.checked_add(2).ok_or(CoreError::ArithmeticOverflow)?;
                builder.stage_parts(
                    &[b::DraftStageRow {
                        kind: StageKind::TableEscapedPipe,
                        input: LocalUtf8Span::new(start as u64, end as u64)?,
                        output,
                        parameter: 0,
                        accepted_by,
                    }],
                    work,
                )?;
            }
            native::AppendKind::PartialTab { spaces, column } => {
                let source = tab_source.ok_or(CoreError::InvalidProvenance)?;
                let sources = builder.source_parts(&[source], work)?;
                let output = builder.text(
                    "    "
                        .get(..spaces as usize)
                        .ok_or(CoreError::InvalidProvenance)?,
                    work,
                )?;
                let accepted_by = builder.write_extra_authority(
                    b::DraftAuthoritySpec {
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
                    },
                    work,
                )?;
                let start = base
                    .checked_add(bytes)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                let end = start.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
                builder.stage_parts(
                    &[b::DraftStageRow {
                        kind: StageKind::PartialTab,
                        input: LocalUtf8Span::new(start as u64, end as u64)?,
                        output,
                        parameter: u32::try_from(column)
                            .map_err(|_| CoreError::ArithmeticOverflow)?,
                        accepted_by,
                    }],
                    work,
                )?;
            }
            native::AppendKind::CodeTerminal => return Err(CoreError::InvalidProvenance),
        }
        bytes = bytes
            .checked_add(
                piece
                    .r_end
                    .checked_sub(piece.r_start)
                    .ok_or(CoreError::InvalidProvenance)?,
            )
            .ok_or(CoreError::ArithmeticOverflow)?;
    }
    if bytes != atom.bytes {
        return Err(CoreError::InvalidProvenance);
    }
    Ok(())
}

fn stage_kind<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    kind: n::TransformKind,
    source: n::SourceToken<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(StageKind, ClaimKind), CoreError> {
    Ok(match kind {
        n::TransformKind::Decode(native::DecodeKind::Entity)
        | n::TransformKind::Payload(native::StageKind::Entity) => {
            let mut cursor = native.read(work, |access| access.source_cursor(source))?;
            let piece = native
                .read(work, |access| access.next_source(&mut cursor))?
                .ok_or(CoreError::InvalidProvenance)?;
            let mut first = None;
            native.source.p_pieces(piece.p, work, |part, _| {
                first.get_or_insert(part.source.start());
                Ok(())
            })?;
            let offset = usize::try_from(first.ok_or(CoreError::InvalidProvenance)?)
                .map_err(|_| CoreError::ArithmeticOverflow)?;
            work.scan(2)?;
            let numeric = native
                .source
                .input()
                .source()
                .text()
                .as_bytes()
                .get(offset.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?)
                == Some(&b'#');
            (
                if numeric {
                    StageKind::NumericEntity
                } else {
                    StageKind::NamedEntity
                },
                ClaimKind::Entity,
            )
        }
        n::TransformKind::Decode(native::DecodeKind::Escape)
        | n::TransformKind::Payload(native::StageKind::Escape) => {
            (StageKind::AsciiEscape, ClaimKind::Escape)
        }
        n::TransformKind::Decode(native::DecodeKind::EmojiShortcode) => {
            (StageKind::EmojiShortcode, ClaimKind::EmojiShortcode)
        }
        n::TransformKind::Decode(native::DecodeKind::CodeLineEndingToSpace) => {
            (StageKind::CodeLineEndingToSpace, ClaimKind::Code)
        }
        n::TransformKind::Decode(native::DecodeKind::CodeTrim) => {
            (StageKind::CodeEdgeTrim, ClaimKind::Code)
        }
        n::TransformKind::SoftBreakToSpace => (StageKind::SoftBreakToSpace, ClaimKind::Break),
        n::TransformKind::Payload(native::StageKind::Trim) => {
            (StageKind::AsciiTrim, ClaimKind::Payload)
        }
        n::TransformKind::Payload(
            native::StageKind::SyntheticMailto | native::StageKind::ReferenceTitleIndent,
        ) => {
            return Err(CoreError::InvalidProvenance);
        }
    })
}

fn host_role(role: n::HostRole) -> HostRole {
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
fn phase(value: n::NativePhase) -> NativePhase {
    match value {
        n::NativePhase::Block => NativePhase::Block,
        n::NativePhase::Inline => NativePhase::Inline,
        n::NativePhase::Metadata => NativePhase::Metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{ModelIdentity, SourceKey};
    use crate::mapping::MapIndex;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    fn check(
        text: &str,
        expected: &str,
        stages: &[(StageKind, &str)],
        primitive: Option<PrimitiveKind>,
    ) {
        let policies = [SourceBudget::model_fixture, SourceBudget::task_fixture];
        for (policy, create) in policies.into_iter().enumerate() {
            let budget = create();
            let mut operation = budget.begin(CancellationToken::new());
            let source = SourceDocument::new(
                SourceKey::new([79; 16], 1).unwrap(),
                text.as_bytes(),
                &mut operation,
            )
            .unwrap()
            .snapshot();
            construction_scope(&mut operation, |mut work| {
                let input = ParseInput::prepare_owned(source.clone(), &mut work)?;
                let config = ParseConfig::new(Profile::C0);
                let ir = super::super::build(&input, config, &mut work).unwrap_or_else(|error| {
                    panic!("direct proof build {text:?} policy={policy}: {error:?}")
                });
                let identity = ModelIdentity::new(source.key(), 1, config)?;
                if work.uses_verified_index() {
                    let facts =
                        ir::validate::validate_for_index_owned(&ir, &source, identity, &mut work)
                            .unwrap_or_else(|error| {
                                panic!(
                                    "direct proof validation {text:?} policy={policy}: {error:?}"
                                )
                            });
                    drop(MapIndex::build_verified(facts)?);
                } else {
                    ir::validate::validate_owned(&ir, &source, &mut work).unwrap_or_else(|error| {
                        panic!("direct proof validation {text:?} policy={policy}: {error:?}")
                    });
                    drop(MapIndex::build_owned(&ir, &source, identity, &mut work)?);
                }
                let flow = ir.flows()[0];
                let mut actual = String::new();
                for index in flow.atoms.start()..flow.atoms.start() + flow.atoms.len() {
                    let atom = ir.owned_read_atom_at(index, &mut work)?;
                    match atom.kind {
                        AtomKind::Hidden => {}
                        AtomKind::Object => actual.push('\u{fffc}'),
                        AtomKind::Copy => {
                            let parts = atom.sources.slice(ir.sources())?;
                            assert_eq!(parts.len(), 1);
                            let span = parts[0];
                            actual.push_str(
                                &source.text()[span.start() as usize..span.end() as usize],
                            );
                        }
                        AtomKind::Transform | AtomKind::Synthetic => {
                            let AtomContent::Pool(value) = atom.content else {
                                panic!("final text must be owned");
                            };
                            actual.push_str(ir.text(value)?);
                        }
                    }
                }
                assert_eq!(actual, expected, "{text:?} policy={policy}");
                assert_eq!(flow.logical_utf16, expected.encode_utf16().count() as u64);
                let actual_stages: Vec<_> = ir
                    .stages()
                    .iter()
                    .map(|stage| (stage.kind, ir.text(stage.output).unwrap()))
                    .collect();
                assert_eq!(actual_stages, stages, "{text:?} policy={policy}");
                if let Some(kind) = primitive {
                    assert!(
                        ir.primitives().iter().any(|row| row.kind == kind),
                        "{text:?}: {kind:?}"
                    );
                }
                drop(ir);
                drop(input);
                work.finish()
            })
            .unwrap();
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            drop(source);
            assert_eq!(budget.usage().source_capacity, 0);
            assert_eq!(budget.usage().source_owners, 0);
        }
    }

    #[test]
    fn native_entities_keep_independent_results_and_scalar_widths() {
        check(
            "&amp;",
            "&",
            &[(StageKind::NamedEntity, "&")],
            Some(PrimitiveKind::Copy),
        );
        check(
            "&#0;",
            "\u{fffd}",
            &[(StageKind::NumericEntity, "\u{fffd}")],
            None,
        );
        check(
            "&NotEqualTilde;",
            "\u{2242}\u{338}",
            &[(StageKind::NamedEntity, "\u{2242}\u{338}")],
            None,
        );
    }

    #[test]
    fn adjacent_transforms_keep_their_own_proof_components() {
        check(
            "&amp;\\*&#0;中😀",
            "&*\u{fffd}中😀",
            &[
                (StageKind::NamedEntity, "&"),
                (StageKind::AsciiEscape, "*"),
                (StageKind::NumericEntity, "\u{fffd}"),
            ],
            None,
        );
    }

    #[test]
    fn code_line_endings_compose_with_original_whole_n01() {
        for (text, primitive) in [
            ("`a\nb`", PrimitiveKind::Copy),
            ("`a\r\nb`", PrimitiveKind::CrLf),
            ("`a\rb`", PrimitiveKind::Cr),
        ] {
            check(
                text,
                "a b",
                &[(StageKind::CodeLineEndingToSpace, " ")],
                Some(primitive),
            );
        }
    }

    #[test]
    fn code_trim_retains_each_accepted_zero_output_edge() {
        check(
            "` a\r\nb `",
            "a b",
            &[
                (StageKind::CodeEdgeTrim, ""),
                (StageKind::CodeLineEndingToSpace, " "),
                (StageKind::CodeEdgeTrim, ""),
            ],
            Some(PrimitiveKind::CrLf),
        );
    }

    #[test]
    fn trimmed_newlines_keep_ancestor_space_distinct_from_root_empty_value() {
        check(
            "`\ra\r\n`",
            "a",
            &[
                (StageKind::CodeLineEndingToSpace, " "),
                (StageKind::CodeEdgeTrim, ""),
                (StageKind::CodeLineEndingToSpace, " "),
                (StageKind::CodeEdgeTrim, ""),
            ],
            Some(PrimitiveKind::CrLf),
        );
    }

    #[test]
    fn payload_escape_replays_two_entity_ancestors_before_the_escape() {
        check(
            "[x](u \"&bsol;&ast;\")",
            "x",
            &[
                (StageKind::NamedEntity, "\\"),
                (StageKind::NamedEntity, "*"),
                (StageKind::AsciiEscape, "*"),
            ],
            None,
        );
    }

    #[test]
    fn soft_break_preserves_crlf_source_and_utf16_target() {
        check(
            "中\r\n😀",
            "中 😀",
            &[(StageKind::SoftBreakToSpace, " ")],
            Some(PrimitiveKind::CrLf),
        );
    }
}
