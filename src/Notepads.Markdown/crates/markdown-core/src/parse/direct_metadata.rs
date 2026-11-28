// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Metadata is written once into reserved final slots. The only retained
//! bindings are definition identities/keys and objects awaiting their flow atom.
use super::*;
use crate::memory::{BuildRows, BuildValue};

#[derive(Clone, Copy)]
pub(super) struct ImageBinding {
    pub object: b::ObjectRef,
    pub node: b::NodeRef,
    pub alt: b::FlowRef,
    pub destination: b::PayloadRef,
    pub title: Option<b::PayloadRef>,
    pub reference: Option<b::ReferenceUseRef>,
    pub wiki_size: Option<(u16, u16)>,
    pub claim: b::AuthorityRef,
    pub sources: b::SourceBlockRef,
}
impl BuildValue for ImageBinding {}

#[derive(Clone, Copy)]
pub(super) struct DerivedBinding {
    pub object: b::ObjectRef,
    pub node: b::NodeRef,
    pub kind: DerivedKind,
    pub input: b::PayloadRef,
    pub claim: b::AuthorityRef,
    pub sources: b::SourceBlockRef,
}
impl BuildValue for DerivedBinding {}

#[derive(Clone, Copy)]
struct DefinitionBinding {
    reference: b::DefinitionRef,
    key: b::TextRef,
}
impl BuildValue for DefinitionBinding {}

pub(super) fn authority<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    owner: b::NodeRef,
    flavor: (HostRole, NativePhase, ClaimKind),
    source: n::SourceToken<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(b::AuthorityRef, b::SourceBlockRef), CoreError> {
    let host = graph.host_sources(native, source, builder, work)?;
    authority_in_host(native, builder, owner, flavor, source, host, work)
}

fn authority_in_host<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    owner: b::NodeRef,
    flavor: (HostRole, NativePhase, ClaimKind),
    source: n::SourceToken<'view, 'input>,
    (domains, domain): (b::SourceBlockRef, n::SourceDomain<'view, 'input>),
    work: &mut OwnedWork<'id, '_>,
) -> Result<(b::AuthorityRef, b::SourceBlockRef), CoreError> {
    let (role, phase, kind) = flavor;
    let sources = native.sources(source, builder, work)?.0;
    let claim = builder.write_extra_authority(
        b::DraftAuthoritySpec {
            owner,
            host: b::DraftHostRow {
                owner,
                role,
                utf8_len: domain.utf8_len as u64,
                domains,
            },
            phase,
            kind,
            consumed: LocalUtf8Span::new(domain.consumed_start as u64, domain.consumed_end as u64)?,
            sources,
        },
        work,
    )?;
    Ok((claim, sources))
}

fn structural<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    info: &capture::NodeRecord<'view, 'input>,
    binding: n::BindingInfo<'view, 'input>,
    marker: Option<n::SourceToken<'view, 'input>>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(b::AuthorityRef, b::SourceBlockRef), CoreError> {
    if info.kind == native::NativeKind::InlineCode {
        let syntax = binding.syntax.ok_or(CoreError::InvalidProvenance)?;
        let domains = graph
            .html_code_host(binding.id, work)?
            .ok_or(CoreError::InvalidProvenance)?;
        let domain = native.read(work, |a| a.source_domain(syntax))?;
        return authority_in_host(
            native,
            builder,
            info.node,
            (HostRole::Inline, NativePhase::Inline, ClaimKind::Html),
            syntax,
            (domains, domain),
            work,
        );
    }
    let kind = match info.kind {
        native::NativeKind::Math => ClaimKind::Math(graph::math_syntax(
            binding.math_syntax.ok_or(CoreError::InvalidProvenance)?,
        )),
        native::NativeKind::Link if binding.wiki == Some(native::WikiForm::Link) => {
            ClaimKind::WikiLink
        }
        native::NativeKind::Link => ClaimKind::Link,
        native::NativeKind::Image => ClaimKind::Image,
        native::NativeKind::NoteEmbed => ClaimKind::NoteEmbed,
        native::NativeKind::FootnoteDefinition => ClaimKind::FootnoteMarker,
        native::NativeKind::FootnoteReference => ClaimKind::FootnoteReference,
        native::NativeKind::CodeBlock => ClaimKind::Code,
        native::NativeKind::HtmlBlock | native::NativeKind::HtmlInline => ClaimKind::Html,
        _ => ClaimKind::Block,
    };
    if let Some(syntax) = autolink::syntax(native, graph, builder, info, work)?.or(binding.syntax) {
        return authority(
            native,
            graph,
            builder,
            info.node,
            (
                if info.kind == native::NativeKind::Math {
                    HostRole::MathInput
                } else {
                    HostRole::Inline
                },
                NativePhase::Inline,
                kind,
            ),
            syntax,
            work,
        );
    }
    if let Some(marker) = marker {
        return authority(
            native,
            graph,
            builder,
            info.node,
            (HostRole::Block, NativePhase::Block, kind),
            marker,
            work,
        );
    }
    let count = native.read(work, |access| access.mapping_len(binding.value))?;
    if count != 0 {
        // Current RawHtml is one native raw value, never a TextJoin/decoder
        // chain. Use its actual R source; S envelopes cannot recreate R hosts.
        if count != 1
            || !matches!(
                info.kind,
                native::NativeKind::HtmlBlock | native::NativeKind::HtmlInline
            )
        {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let atom = native.read(work, |access| access.mapping_atom(binding.value, 0))?;
        let (role, phase) = if info.kind == native::NativeKind::HtmlInline {
            (HostRole::Inline, NativePhase::Inline)
        } else {
            (HostRole::Block, NativePhase::Block)
        };
        let host = if let Some((_, domains)) = graph.html_emphasis(binding.id, work)? {
            (
                domains,
                native.read(work, |a| a.source_domain(atom.source))?,
            )
        } else {
            graph.host_sources(native, atom.source, builder, work)?
        };
        return authority_in_host(
            native,
            builder,
            info.node,
            (role, phase, kind),
            atom.source,
            host,
            work,
        );
    }
    let at = native
        .source
        .p_anchor(binding.start_p.ok_or(CoreError::InvalidProvenance)?, work)?;
    let sources = builder.source_parts(&[SourceSpan::new(at, at)?], work)?;
    let (role, phase) = if matches!(kind, ClaimKind::Link | ClaimKind::Image | ClaimKind::Html)
        && info.kind != native::NativeKind::HtmlBlock
    {
        (HostRole::Inline, NativePhase::Inline)
    } else {
        (HostRole::Block, NativePhase::Block)
    };
    let claim = builder.write_extra_authority(
        b::DraftAuthoritySpec {
            owner: info.node,
            host: b::DraftHostRow {
                owner: info.node,
                role,
                utf8_len: 0,
                domains: sources,
            },
            phase,
            kind,
            consumed: LocalUtf8Span::new(0, 0)?,
            sources,
        },
        work,
    )?;
    Ok((claim, sources))
}

fn attribute_sources<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    marker: Option<n::SourceToken<'view, 'input>>,
    kind: AttributeKind,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::SourceBlockRef, CoreError> {
    let token = marker.ok_or(CoreError::InvalidProvenance)?;
    if matches!(kind, AttributeKind::ListTightness | AttributeKind::CodeForm) {
        return native.sources(token, builder, work).map(|value| value.0);
    }
    let mut marker_span = None;
    native.visit_sources(token, work, |span, work| {
        if marker_span.replace(span).is_some() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(())
    })?;
    let span = marker_span.ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
    let raw = native.source.input().source().text().as_bytes();
    let mut start = usize::try_from(span.start()).map_err(|_| CoreError::ArithmeticOverflow)?;
    let end = usize::try_from(span.end()).map_err(|_| CoreError::ArithmeticOverflow)?;
    if end > raw.len() {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    while start < end && matches!(raw[start], b' ' | b'\t') {
        work.scan(1)?;
        start += 1;
    }
    let byte = *raw
        .get(start)
        .filter(|_| start < end)
        .ok_or(CoreError::InvalidProvenance)?;
    let mut finish = start;
    if matches!(
        kind,
        AttributeKind::HeadingLevel | AttributeKind::HeadingForm | AttributeKind::CodeFence
    ) {
        while finish < end && raw[finish] == byte {
            work.scan(1)?;
            finish += 1;
        }
    } else if byte.is_ascii_digit() {
        while finish < end && raw[finish].is_ascii_digit() {
            work.scan(1)?;
            finish += 1;
        }
        finish = finish.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
    } else {
        finish = finish.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
    }
    if finish > end {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    builder.source_parts(&[SourceSpan::new(start as u64, finish as u64)?], work)
}

fn list_marker(ordered: bool, delimiter: u8, bullet: u8) -> ListMarker {
    if ordered {
        ListMarker::Ordered { delimiter }
    } else {
        ListMarker::Bullet(bullet)
    }
}

fn occurrence<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &graph::Graph<'view, 'input, 'id>,
    builder: &b::DirectBuilder<'id>,
    previous: Option<n::LineageToken<'view, 'input>>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<u32, CoreError> {
    let Some(lineage) = previous else {
        return Ok(0);
    };
    let index = graph
        .member_for_lineage(lineage, native, work)?
        .ok_or(CoreError::InvalidProvenance)?;
    let member = graph.nodes.get(index, work)?;
    let detail = graph
        .binding(member, work)?
        .and_then(graph::Metadata::detail)
        .ok_or(CoreError::InvalidProvenance)?;
    let DetailRow::Item { occurrence, .. } = builder.read_detail(detail, work)? else {
        return Err(work.fail(CoreError::InvalidProvenance));
    };
    occurrence
        .checked_add(1)
        .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))
}

pub(super) fn emit<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    captured: capture::Captured<'view, 'input, 'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<BuildRows<'id, extensions::ObjectBinding>, CoreError> {
    capture::call(work, |work| {
        emit_inner(native, graph, builder, captured, work)
    })
}
fn emit_inner<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    mut captured: capture::Captured<'view, 'input, 'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<BuildRows<'id, extensions::ObjectBinding>, CoreError> {
    let max = work
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .model_records;
    let mut definitions = BuildRows::new(max, LimitKind::ModelRecords, work)?;
    let mut objects = BuildRows::new(max, LimitKind::ModelRecords, work)?;
    let root = graph
        .nodes
        .get(graph.flows.get(0, work)?.root(), work)?
        .node;
    while let Some(row) = captured.next_definition(root, builder, work)? {
        let reference = row.reference;
        let owner = b::DraftPayloadOwner::Definition(reference);
        let label = target::raw_label(native, graph, builder, owner, root, row.raw_label, work)?;
        let destination = target::payload(
            native,
            graph,
            builder,
            owner,
            root,
            PayloadRole::Destination,
            row.destination,
            work,
        )?;
        let title = row
            .title
            .map(|value| {
                target::payload(
                    native,
                    graph,
                    builder,
                    owner,
                    root,
                    PayloadRole::Title,
                    value,
                    work,
                )
            })
            .transpose()?;
        let key = row.key;
        let (claim, syntax) = authority(
            native,
            graph,
            builder,
            root,
            (
                HostRole::Block,
                NativePhase::Block,
                ClaimKind::ReferenceDefinition,
            ),
            row.syntax,
            work,
        )?;
        builder.write_definition(
            reference,
            b::DraftDefinitionRow {
                syntax,
                label,
                destination,
                title,
                key,
                winner: row.winner,
                claim,
            },
            work,
        )?;
        definitions.push(DefinitionBinding { reference, key }, work)?;
    }
    let mut next_alt = 1;
    while let Some(info) = captured.next_node(graph, builder, work)? {
        let metadata = info.metadata;
        if let capture::Attributes::G3 { token } = info.attributes {
            if let Some(binding) =
                extensions::emit(native, graph, builder, info.node, metadata, token, work)?
            {
                objects.push(binding, work)?;
            }
            continue;
        }
        let binding = native.read(work, |access| access.binding(info.lineage))?;
        let inline_note = if matches!(info.attributes, capture::Attributes::Footnote(_)) {
            native.read(work, |access| access.inline_footnote(info.lineage))?
        } else {
            None
        };
        let callout = if matches!(info.attributes, capture::Attributes::Callout) {
            Some(
                native
                    .read(work, |access| access.callout(info.lineage))?
                    .ok_or(CoreError::InvalidProvenance)?,
            )
        } else {
            None
        };
        // Reuse this already-read retained binding for both consumers. Only
        // List's first-child fallback needs the AST-period capture. Option::or
        // preserves every Some token, including a zero-width actual source.
        let marker = if info.kind == native::NativeKind::List {
            binding.marker.or(info.list_fallback)
        } else {
            binding.marker
        };
        let (claim, syntax) = if let Some(callout) = callout {
            authority(
                native,
                graph,
                builder,
                info.node,
                (
                    HostRole::Block,
                    NativePhase::Block,
                    ClaimKind::CalloutMarker,
                ),
                callout.marker,
                work,
            )?
        } else if let Some(note) = inline_note {
            let (kind, token) = if info.kind == native::NativeKind::FootnoteDefinition {
                (ClaimKind::InlineFootnoteBody, note.body)
            } else {
                (ClaimKind::InlineFootnoteOpening, note.opening)
            };
            authority(
                native,
                graph,
                builder,
                info.node,
                (HostRole::Inline, NativePhase::Inline, kind),
                token,
                work,
            )?
        } else if matches!(info.attributes, capture::Attributes::DescriptionItem { .. }) {
            authority(
                native,
                graph,
                builder,
                info.node,
                (
                    HostRole::Block,
                    NativePhase::Block,
                    ClaimKind::DescriptionMarker,
                ),
                marker.ok_or(CoreError::InvalidProvenance)?,
                work,
            )?
        } else if metadata.fence().is_some() {
            // Join only the original opening/closing grammar claims and the
            // actual sealed R buffer. BlockConsume is a lookahead envelope,
            // deliberately not retained or used as export permission.
            if binding.fence_opening.is_none() || binding.buffer.is_none() {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let first = builder.sources_len();
            let mut bytes = 0u64;
            let mut pending: Option<SourceSpan> = None;
            for source in [binding.fence_opening, binding.buffer, binding.fence_closing]
                .into_iter()
                .flatten()
            {
                let (begin, end) = native.visit_sources(source, work, |span, work| {
                    if let Some(previous) = pending {
                        if previous.end() == span.start() {
                            pending = Some(SourceSpan::new(previous.start(), span.end())?);
                            return Ok(());
                        }
                        if previous.end() > span.start() {
                            return Err(work.fail(CoreError::InvalidProvenance));
                        }
                        builder.source_parts(&[previous], work)?;
                    }
                    pending = Some(span);
                    Ok(())
                })?;
                bytes = bytes
                    .checked_add((end - begin) as u64)
                    .ok_or(CoreError::ArithmeticOverflow)?;
            }
            builder.source_parts(&[pending.ok_or(CoreError::InvalidProvenance)?], work)?;
            let sources =
                builder.written_source_block(first, builder.sources_len() - first, work)?;
            let claim = builder.write_extra_authority(
                b::DraftAuthoritySpec {
                    owner: info.node,
                    host: b::DraftHostRow {
                        owner: info.node,
                        role: HostRole::Block,
                        utf8_len: bytes,
                        domains: sources,
                    },
                    phase: NativePhase::Block,
                    kind: ClaimKind::Code,
                    consumed: LocalUtf8Span::new(0, bytes)?,
                    sources,
                },
                work,
            )?;
            (claim, sources)
        } else {
            structural(native, graph, builder, &info, binding, marker, work)?
        };
        let info_node = info.node;
        let (detail, attributes): (Option<b::DraftDetailRow>, &[AttributeKind]) = match info
            .attributes
        {
            capture::Attributes::Callout => {
                let callout = callout.ok_or(CoreError::InvalidProvenance)?;
                let owner = builder.graph_node(info_node, work)?;
                let index = graph
                    .first(owner.index, work)?
                    .ok_or(CoreError::InvalidProvenance)?;
                let title = graph.nodes.get(index, work)?;
                if let Some(authored) = callout.title {
                    if title.callout_title
                        || graph.member_for_lineage(authored, native, work)? != Some(index)
                    {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                } else if !title.callout_title {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                (
                    Some(b::DraftDetailRow::Callout {
                        kind: callout.kind,
                        disclosure: callout.disclosure,
                        title: title.node,
                        claim,
                    }),
                    &[],
                )
            }
            capture::Attributes::Footnote(note) => {
                let detail = match note {
                    footnotes::Note::Definition { ordinal } => {
                        b::DraftDetailRow::FootnoteDefinition { ordinal, claim }
                    }
                    footnotes::Note::Reference {
                        ordinal,
                        occurrence,
                    } => {
                        let lineage = binding
                            .footnote_definition
                            .ok_or(CoreError::InvalidProvenance)?;
                        let index = graph
                            .member_for_lineage(lineage, native, work)?
                            .ok_or(CoreError::InvalidProvenance)?;
                        let definition = graph.nodes.get(index, work)?.node;
                        if builder.graph_node(definition, work)?.kind
                            != NodeKind::FootnoteDefinition
                        {
                            return Err(work.fail(CoreError::InvalidProvenance));
                        }
                        b::DraftDetailRow::FootnoteReference {
                            definition,
                            ordinal,
                            occurrence,
                            claim,
                            closing: if let Some(note) = inline_note {
                                Some(
                                    authority(
                                        native,
                                        graph,
                                        builder,
                                        info_node,
                                        (
                                            HostRole::Inline,
                                            NativePhase::Inline,
                                            ClaimKind::InlineFootnoteClosing,
                                        ),
                                        note.closing,
                                        work,
                                    )?
                                    .0,
                                )
                            } else {
                                None
                            },
                        }
                    }
                };
                (Some(detail), &[])
            }
            capture::Attributes::Math {
                syntax: mode,
                input,
            } => {
                if binding.math_syntax.map(graph::math_syntax) != Some(mode) {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                let input =
                    target::math_payload(native, graph, builder, info_node, mode, input, work)?;
                objects.push(
                    extensions::ObjectBinding::Derived(DerivedBinding {
                        object: metadata.object().ok_or(CoreError::InvalidProvenance)?,
                        node: info_node,
                        kind: DerivedKind::Math(mode),
                        input,
                        claim,
                        sources: syntax,
                    }),
                    work,
                )?;
                (None, &[])
            }
            capture::Attributes::G3 { .. } => return Err(work.fail(CoreError::InvalidProvenance)),
            capture::Attributes::Heading { level, setext } => (
                Some(b::DraftDetailRow::Heading {
                    level,
                    form: if setext {
                        HeadingForm::Setext
                    } else {
                        HeadingForm::Atx
                    },
                    claim,
                }),
                &[AttributeKind::HeadingLevel, AttributeKind::HeadingForm],
            ),
            capture::Attributes::List {
                ordered,
                start,
                delimiter,
                bullet,
                tight,
            } => (
                Some(b::DraftDetailRow::List {
                    marker: list_marker(ordered, delimiter, bullet),
                    start: if ordered { start as u64 } else { 0 },
                    tight,
                    claim,
                }),
                &[
                    AttributeKind::ListMarker,
                    AttributeKind::ListStart,
                    AttributeKind::ListTightness,
                ],
            ),
            capture::Attributes::Item {
                ordered,
                start,
                delimiter,
                bullet,
                previous,
            } => (
                Some(b::DraftDetailRow::Item {
                    occurrence: occurrence(native, graph, builder, previous, work)?,
                    marker: list_marker(ordered, delimiter, bullet),
                    value: if ordered { start as u64 } else { 0 },
                    claim,
                }),
                &[AttributeKind::ItemMarker],
            ),
            capture::Attributes::DescriptionItem { tight } => (
                Some(b::DraftDetailRow::DescriptionItem { tight, claim }),
                &[],
            ),
            capture::Attributes::Code {
                fenced,
                fence,
                fence_length,
                info,
                fence_input,
            } => {
                if let Some(value) = fence_input {
                    let kind = metadata.fence().ok_or(CoreError::InvalidProvenance)?;
                    let input = target::payload(
                        native,
                        graph,
                        builder,
                        b::DraftPayloadOwner::Node(info_node),
                        info_node,
                        PayloadRole::FenceInput,
                        value,
                        work,
                    )?;
                    objects.push(
                        extensions::ObjectBinding::Derived(DerivedBinding {
                            object: metadata.object().ok_or(CoreError::InvalidProvenance)?,
                            node: info_node,
                            kind: kind.derived(),
                            input,
                            claim,
                            sources: syntax,
                        }),
                        work,
                    )?;
                }
                let value = info
                    .map(|value| {
                        target::payload(
                            native,
                            graph,
                            builder,
                            b::DraftPayloadOwner::Node(info_node),
                            info_node,
                            PayloadRole::FenceInfo,
                            value,
                            work,
                        )
                    })
                    .transpose()?;
                (
                    Some(b::DraftDetailRow::Code {
                        form: if fenced {
                            CodeForm::Fenced {
                                byte: fence,
                                length: u32::try_from(fence_length)
                                    .map_err(|_| CoreError::ArithmeticOverflow)?,
                            }
                        } else {
                            CodeForm::Indented
                        },
                        info: value,
                        claim,
                    }),
                    if fenced {
                        &[AttributeKind::CodeForm, AttributeKind::CodeFence]
                    } else {
                        &[AttributeKind::CodeForm]
                    },
                )
            }
            capture::Attributes::Html => (Some(b::DraftDetailRow::Html { claim }), &[]),
            capture::Attributes::Link {
                destination,
                title,
                definition,
            } => {
                if binding.definition.map(|id| id.get() as usize - 1) != definition {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                let definition = definition.map(|at| definitions.get(at, work)).transpose()?;
                let destination = target::payload(
                    native,
                    graph,
                    builder,
                    b::DraftPayloadOwner::Node(info.node),
                    info.node,
                    PayloadRole::Destination,
                    destination,
                    work,
                )?;
                let title = title
                    .map(|value| {
                        target::payload(
                            native,
                            graph,
                            builder,
                            b::DraftPayloadOwner::Node(info.node),
                            info.node,
                            PayloadRole::Title,
                            value,
                            work,
                        )
                    })
                    .transpose()?;
                let reference = if let Some(definition) = definition {
                    let raw = binding.syntax.ok_or(CoreError::InvalidProvenance)?;
                    let (reference_claim, reference_syntax) = authority(
                        native,
                        graph,
                        builder,
                        info.node,
                        (
                            HostRole::Inline,
                            NativePhase::Inline,
                            ClaimKind::ReferenceUse,
                        ),
                        raw,
                        work,
                    )?;
                    let reference = builder.reserve_reference(work)?;
                    builder.write_reference(
                        reference,
                        b::DraftReferenceUseRow {
                            node: info.node,
                            syntax: reference_syntax,
                            winner: definition.reference,
                            key: definition.key,
                            destination,
                            title,
                            claim: reference_claim,
                        },
                        work,
                    )?;
                    Some(reference)
                } else {
                    None
                };
                if info.kind == native::NativeKind::NoteEmbed {
                    if binding.wiki != Some(native::WikiForm::NoteEmbed)
                        || title.is_some()
                        || reference.is_some()
                    {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    objects.push(
                        extensions::ObjectBinding::Derived(DerivedBinding {
                            object: metadata.object().ok_or(CoreError::InvalidProvenance)?,
                            node: info.node,
                            kind: DerivedKind::NoteEmbed,
                            input: destination,
                            claim,
                            sources: syntax,
                        }),
                        work,
                    )?;
                    (None, &[])
                } else if let Some(object) = metadata.object() {
                    let alt = graph.flows.get(next_alt, work)?;
                    if alt.object != Some(object) {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    next_alt += 1;
                    objects.push(
                        extensions::ObjectBinding::Image(ImageBinding {
                            object,
                            node: info.node,
                            alt: alt.reference,
                            destination,
                            title,
                            reference,
                            wiki_size: match binding.wiki {
                                Some(native::WikiForm::Image { width, height }) => {
                                    Some((width, height))
                                }
                                None => None,
                                Some(native::WikiForm::Link | native::WikiForm::NoteEmbed) => {
                                    return Err(work.fail(CoreError::InvalidProvenance));
                                }
                            },
                            claim,
                            sources: syntax,
                        }),
                        work,
                    )?;
                    (None, &[])
                } else {
                    (
                        Some(b::DraftDetailRow::Link {
                            destination,
                            title,
                            reference,
                            claim,
                        }),
                        &[],
                    )
                }
            }
        };
        if let Some(detail) = detail {
            builder.write_detail(
                metadata.detail().ok_or(CoreError::InvalidProvenance)?,
                detail,
                work,
            )?;
        } else if metadata.detail().is_some() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        for kind in attributes {
            let sources = attribute_sources(native, builder, marker, *kind, work)?;
            builder.attribute(
                b::DraftAttributeRow {
                    owner: info.node,
                    kind: *kind,
                    claim,
                    sources,
                },
                work,
            )?;
        }
    }
    if next_alt != graph.flows.len() {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    captured.finish(work)?;
    Ok(objects)
}

pub(super) fn footnote_backlink<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &graph::Graph<'view, 'input, 'id>,
    link: graph::Backlink<'view, 'input>,
    anchor: SourceSpan,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let reference = graph
        .member_for_lineage(link.reference, native, work)?
        .ok_or(CoreError::InvalidProvenance)?;
    let reference = graph.nodes.get(reference, work)?.node;
    if builder.graph_node(reference, work)?.kind != NodeKind::FootnoteReference {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    let sources = builder.source_parts(&[anchor], work)?;
    let claim = builder.write_extra_authority(
        b::DraftAuthoritySpec {
            owner: link.node,
            host: b::DraftHostRow {
                owner: link.node,
                role: HostRole::Structure,
                utf8_len: 0,
                domains: sources,
            },
            phase: NativePhase::Structure,
            kind: ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteBacklink),
            consumed: LocalUtf8Span::new(0, 0)?,
            sources,
        },
        work,
    )?;
    builder.write_detail(
        link.detail,
        b::DraftDetailRow::FootnoteBacklink {
            reference,
            ordinal: link.ordinal,
            occurrence: link.occurrence,
            claim,
        },
        work,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SourceKey;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    fn inspect(text: &str, check: impl Fn(&CompactIr)) {
        for task in [false, true] {
            let budget = if task {
                SourceBudget::task_fixture()
            } else {
                SourceBudget::model_fixture()
            };
            let mut operation = budget.begin(CancellationToken::new());
            let source = SourceDocument::new(
                SourceKey::new([117; 16], 1).unwrap(),
                text.as_bytes(),
                &mut operation,
            )
            .unwrap()
            .snapshot();
            construction_scope(&mut operation, |mut work| {
                let input = ParseInput::prepare_owned(source.clone(), &mut work).unwrap();
                let ir = build(&input, ParseConfig::new(Profile::C0), &mut work)
                    .unwrap_or_else(|error| panic!("direct metadata build {text:?}: {error:?}"));
                ir::validate::validate_owned(&ir, &source, &mut work)
                    .unwrap_or_else(|error| panic!("direct metadata validate {text:?}: {error:?}"));
                check(&ir);
                drop((ir, input));
                work.finish().unwrap();
            });
            drop(source);
            let usage = budget.usage();
            assert_eq!(usage.auxiliary_capacity, 0);
            assert_eq!(usage.source_capacity, 0);
            assert_eq!(usage.source_owners, 0);
        }
    }

    #[test]
    fn accepted_definitions_keep_losers_and_per_use_payload_ownership() {
        inspect("[x]: /one \"title\"\n[x]: /two\n\n[x] ![x]\n", |ir| {
            assert_eq!(ir.definitions().len(), 2);
            assert!(ir.definitions()[0].winner);
            assert!(!ir.definitions()[1].winner);
            assert_eq!(ir.references().len(), 2);
            assert_eq!(ir.objects().len(), 1);
            for row in ir.references() {
                assert_eq!(
                    ir.text(ir.payloads()[row.destination.index()].value)
                        .unwrap(),
                    "/one"
                );
                assert_eq!(
                    ir.payloads()[row.destination.index()].owner,
                    PayloadOwner::Node(row.node)
                );
            }
        });
    }

    #[test]
    fn heading_list_code_and_html_metadata_preserve_original_fields() {
        inspect(
            "# heading\n\n3. first\n4. second\n\n```rust\nx\n```\n\n<div>raw</div>\n",
            |ir| {
                assert!(ir.details().iter().any(|row| matches!(
                    row,
                    DetailRow::Heading {
                        level: 1,
                        form: HeadingForm::Atx,
                        ..
                    }
                )));
                assert!(
                    ir.details()
                        .iter()
                        .any(|row| matches!(row, DetailRow::List { start: 3, .. }))
                );
                assert!(ir.details().iter().any(|row| matches!(
                    row,
                    DetailRow::Item {
                        occurrence: 0,
                        value: 3,
                        ..
                    }
                )));
                assert!(ir.details().iter().any(|row| matches!(
                    row,
                    DetailRow::Item {
                        occurrence: 1,
                        value: 4,
                        ..
                    }
                )));
                assert!(ir.details().iter().any(|row| matches!(
                    row,
                    DetailRow::Code {
                        form: CodeForm::Fenced {
                            byte: b'`',
                            length: 3
                        },
                        info: Some(_),
                        ..
                    }
                )));
                assert!(
                    ir.details()
                        .iter()
                        .any(|row| matches!(row, DetailRow::Html { .. }))
                );
            },
        );
    }

    #[test]
    fn inline_code_has_no_detail_and_empty_link_payload_remains_real() {
        inspect("`x` [empty]() ![alt](u \"\")\n", |ir| {
            let code = ir
                .nodes()
                .iter()
                .find(|row| row.kind == NodeKind::InlineCode)
                .unwrap();
            assert_eq!(code.detail, None);
            assert_eq!(ir.objects().len(), 1);
            assert!(
                ir.payloads()
                    .iter()
                    .any(|row| row.role == PayloadRole::Destination && row.utf8_len == 0)
            );
        });
    }

    #[test]
    fn review_regression_container_tabs_keep_block_authority_in_metadata() {
        for text in [
            "> [link](url \"line1\n>\tline2\")\n",
            "> [a\n>\tb]: /u\n\n[a b]\n",
            "> [a]: /u \"line1\n>\tline2\"\n\n[a]\n",
        ] {
            inspect(text, |ir| {
                if text.starts_with("> [link]") {
                    assert!(
                        ir.stages()
                            .iter()
                            .any(|stage| stage.kind == StageKind::PartialTab)
                    );
                }
                assert!(ir.payloads().iter().any(|payload| matches!(
                    payload.role,
                    PayloadRole::Title | PayloadRole::ReferenceLabel
                )));
            });
        }
    }
}
