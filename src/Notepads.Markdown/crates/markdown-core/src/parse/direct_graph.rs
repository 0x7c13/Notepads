// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Native membership and unfinished builder identities. Semantic NodeRows are
//! written directly at DFS exit; no second table of those rows is retained.

use super::*;
use crate::memory::{BuildRows, BuildValue};

const NONE: u32 = u32::MAX;

fn ordinal(value: usize) -> Result<u32, CoreError> {
    let value = extent(value)?;
    if value == NONE {
        return Err(CoreError::ArithmeticOverflow);
    }
    Ok(value)
}

// An exclusive subtree end may equal MAX; it is never an optional ordinal.
fn extent(value: usize) -> Result<u32, CoreError> {
    u32::try_from(value).map_err(|_| CoreError::ArithmeticOverflow)
}

#[derive(Clone, Copy)]
pub(super) struct Member<'view, 'input> {
    pub lineage: n::LineageToken<'view, 'input>,
    pub node: b::NodeRef,
    metadata: u32,
    subtree_end: u32,
    pub output: bool,
    pub alt_root: bool,
    pub callout_title: bool,
}
impl BuildValue for Member<'_, '_> {}

#[derive(Clone, Copy)]
pub(super) struct Backlink<'view, 'input> {
    pub index: usize,
    pub node: b::NodeRef,
    pub reference: n::LineageToken<'view, 'input>,
    pub detail: b::DetailRef,
    pub ordinal: u32,
    pub occurrence: u32,
}

impl BuildValue for Backlink<'_, '_> {}
impl Member<'_, '_> {
    pub(super) fn end(self) -> usize {
        self.subtree_end as usize
    }
    pub(super) fn has_metadata(self) -> bool {
        self.metadata != NONE
    }
    pub(super) fn metadata_index(self) -> Option<usize> {
        (self.metadata != NONE).then_some(self.metadata as usize)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum FenceKind {
    Diagram,
    Math,
}

impl FenceKind {
    pub(super) fn node(self) -> NodeKind {
        match self {
            Self::Diagram => NodeKind::Diagram,
            Self::Math => NodeKind::MathBlock,
        }
    }

    pub(super) fn derived(self) -> DerivedKind {
        match self {
            Self::Diagram => DerivedKind::Diagram,
            Self::Math => DerivedKind::Math(MathSyntax::FencedDisplay),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Metadata {
    Detail(b::DetailRef),
    Object(b::ObjectRef),
    Fence {
        kind: FenceKind,
        detail: b::DetailRef,
        object: b::ObjectRef,
    },
}
impl BuildValue for Metadata {}
impl Metadata {
    pub(super) fn fence(self) -> Option<FenceKind> {
        if let Self::Fence { kind, .. } = self {
            Some(kind)
        } else {
            None
        }
    }

    pub(super) fn detail(self) -> Option<b::DetailRef> {
        match self {
            Self::Detail(value) => Some(value),
            Self::Object(_) => None,
            Self::Fence { detail, .. } => Some(detail),
        }
    }
    pub(super) fn object(self) -> Option<b::ObjectRef> {
        match self {
            Self::Object(value) => Some(value),
            Self::Detail(_) => None,
            Self::Fence { object, .. } => Some(object),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Flow {
    pub reference: b::FlowRef,
    root: u32,
    pub domains: b::SourceBlockRef,
    pub object: Option<b::ObjectRef>,
}
impl BuildValue for Flow {}
impl Flow {
    pub(super) fn root(self) -> usize {
        self.root as usize
    }
}

#[derive(Clone, Copy)]
struct PendingAlt<'view, 'arena, 'input> {
    image: n::NodeToken<'view, 'arena, 'input>,
    object: b::ObjectRef,
    domains: b::SourceBlockRef,
    envelope: SourceSpan,
    emphasis_allowed: bool,
}
impl BuildValue for PendingAlt<'_, '_, '_> {}

struct Preparation<'view, 'arena, 'input, 'id> {
    pending: BuildRows<'id, PendingAlt<'view, 'arena, 'input>>,
    footnotes: Option<footnotes::NotePlan<'view, 'arena, 'input, 'id>>,
}

#[derive(Clone, Copy)]
struct ReserveFrame<'view, 'arena, 'input> {
    node: usize,
    next: Option<n::NodeToken<'view, 'arena, 'input>>,
    emphasis_allowed: bool,
}
impl BuildValue for ReserveFrame<'_, '_, '_> {}

#[cfg(test)]
#[derive(Clone, Copy)]
struct MeasureFrame {
    node: usize,
    next: Option<usize>,
    begin: u64,
    envelope: SourceSpan,
    separators: bool,
}
#[cfg(test)]
impl BuildValue for MeasureFrame {}

#[derive(Clone, Copy)]
struct Placement {
    parent: Option<usize>,
    flow: usize,
    alt_root: bool,
    initial: Option<SourceSpan>,
    emphasis_allowed: bool,
}

#[derive(Clone, Copy)]
struct NativeSlot {
    member: u32,
    domains: u32,
}
impl BuildValue for NativeSlot {}

#[derive(Clone, Copy)]
enum InlineHtmlMark {
    None,
    OpaqueChild,
    Comment,
    Endpoint { host: u32, parameter: u8 },
    CodeHost(u32),
}
impl BuildValue for InlineHtmlMark {}
impl BuildValue for HtmlEmphasis {}

#[derive(Clone, Copy)]
enum InlineTag {
    Comment,
    Emphasis(HtmlEmphasis),
    Opaque {
        name: &'static [u8],
        closing: bool,
        filtered: bool,
    },
}

impl InlineTag {
    fn prefix(raw: &[u8]) -> Option<Self> {
        // Only an already accepted native HtmlInline reaches this classifier.
        if raw.starts_with(b"<!--") {
            return Some(Self::Comment);
        }
        if let Some((tag, _)) = HtmlEmphasis::prefix(raw) {
            return Some(Self::Emphasis(tag));
        }
        const OPAQUE: [&[u8]; 12] = [
            b"script",
            b"style",
            b"textarea",
            b"title",
            b"xmp",
            b"iframe",
            b"noembed",
            b"noframes",
            b"plaintext",
            b"code",
            b"kbd",
            b"samp",
        ];
        let (name, closing, _) = inline_tag_prefix(raw)?;
        let name = OPAQUE
            .into_iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(name))?;
        Some(Self::Opaque {
            name,
            closing,
            filtered: native::gfm_tagfilter_matches(raw),
        })
    }

    fn code_kind(name: &[u8]) -> Option<OpaqueCode> {
        match name {
            b"code" => Some(OpaqueCode::Code),
            b"kbd" => Some(OpaqueCode::Keyboard),
            b"samp" => Some(OpaqueCode::Sample),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OpaqueCode {
    Code,
    Keyboard,
    Sample,
}
impl BuildValue for OpaqueCode {}

#[derive(Default)]
enum AttributeValue {
    #[default]
    Outside,
    Before,
    Unquoted,
    Quoted(u8),
}

#[derive(Default)]
struct CodeTagEnd {
    value: AttributeValue,
    outside_slash: bool,
    self_closing: bool,
}

impl CodeTagEnd {
    // The native parser already accepted the complete HTML tag. Its final
    // slash is a delimiter only when it is outside an attribute value.
    fn observe(&mut self, text: &str) {
        for byte in text.bytes() {
            self.self_closing = self.outside_slash && byte == b'>';
            self.outside_slash = matches!(self.value, AttributeValue::Outside) && byte == b'/';
            self.value = match self.value {
                AttributeValue::Outside if byte == b'=' => AttributeValue::Before,
                AttributeValue::Before if matches!(byte, b'\'' | b'"') => {
                    AttributeValue::Quoted(byte)
                }
                AttributeValue::Before if !inline_html_tag_space(byte) => AttributeValue::Unquoted,
                AttributeValue::Unquoted if inline_html_tag_space(byte) => AttributeValue::Outside,
                AttributeValue::Quoted(quote) if byte == quote => AttributeValue::Outside,
                _ => continue,
            };
        }
    }
}

pub(super) struct Graph<'view, 'input, 'id> {
    pub nodes: BuildRows<'id, Member<'view, 'input>>,
    pub flows: BuildRows<'id, Flow>,
    native_nodes: BuildRows<'id, NativeSlot>,
    inline_html_marks: Option<BuildRows<'id, InlineHtmlMark>>,
    metadata: BuildRows<'id, Metadata>,
    hosts: BuildRows<'id, b::SourceBlockRef>,
    structural_extensions: bool,
    pub reordered_footnotes: bool,
    pub backlinks: BuildRows<'id, Backlink<'view, 'input>>,
    pub(super) table_of_contents: Option<toc::Plan<'view, 'input, 'id>>,
    mermaid_enabled: bool,
    math_enabled: bool,
    callouts_enabled: bool,
    inline_html_enabled: bool,
    inline_html_tagfilter: bool,
}

pub(super) fn kind(kind: native::NativeKind) -> Result<NodeKind, CoreError> {
    Ok(match kind {
        native::NativeKind::Document => NodeKind::Document,
        native::NativeKind::Paragraph => NodeKind::Paragraph,
        native::NativeKind::Heading => NodeKind::Heading,
        native::NativeKind::BlockQuote => NodeKind::BlockQuote,
        native::NativeKind::List => NodeKind::List,
        native::NativeKind::ListItem => NodeKind::ListItem,
        native::NativeKind::DescriptionList => NodeKind::DescriptionList,
        native::NativeKind::DescriptionItem => NodeKind::DescriptionItem,
        native::NativeKind::DescriptionTerm => NodeKind::DescriptionTerm,
        native::NativeKind::DescriptionDetails => NodeKind::DescriptionDetails,
        native::NativeKind::CodeBlock => NodeKind::CodeBlock,
        native::NativeKind::HtmlBlock => NodeKind::HtmlBlock,
        native::NativeKind::ThematicBreak => NodeKind::ThematicBreak,
        native::NativeKind::TableOfContents => NodeKind::TableOfContents,
        native::NativeKind::Text => NodeKind::Text,
        native::NativeKind::SoftBreak => NodeKind::SoftBreak,
        native::NativeKind::HardBreak => NodeKind::HardBreak,
        native::NativeKind::InlineCode => NodeKind::InlineCode,
        native::NativeKind::Emphasis => NodeKind::Emphasis,
        native::NativeKind::Strong => NodeKind::Strong,
        native::NativeKind::Highlight => NodeKind::Highlight,
        native::NativeKind::Superscript => NodeKind::Superscript,
        native::NativeKind::Subscript => NodeKind::Subscript,
        native::NativeKind::Link => NodeKind::Link,
        native::NativeKind::Image => NodeKind::Image,
        native::NativeKind::NoteEmbed => NodeKind::NoteEmbed,
        native::NativeKind::HtmlInline => NodeKind::HtmlInline,
        native::NativeKind::Table => NodeKind::Table,
        native::NativeKind::TableRow => NodeKind::TableRow,
        native::NativeKind::TableCell => NodeKind::TableCell,
        native::NativeKind::Strike => NodeKind::Strike,
        native::NativeKind::TaskCheckbox => NodeKind::TaskCheckbox,
        native::NativeKind::Math => NodeKind::Math,
        native::NativeKind::FootnoteDefinition => NodeKind::FootnoteDefinition,
        native::NativeKind::FootnoteReference => NodeKind::FootnoteReference,
        native::NativeKind::Escaped => return Err(CoreError::InvalidProvenance),
    })
}

pub(super) fn math_syntax(syntax: native::MathSyntax) -> MathSyntax {
    match syntax {
        native::MathSyntax::DollarInline => MathSyntax::DollarInline,
        native::MathSyntax::DollarDisplay => MathSyntax::DollarDisplay,
        native::MathSyntax::LatexInline => MathSyntax::LatexInline,
        native::MathSyntax::LatexDisplay => MathSyntax::LatexDisplay,
        native::MathSyntax::CodeInline => MathSyntax::CodeInline,
    }
}

fn union(previous: Option<SourceSpan>, next: SourceSpan) -> Result<SourceSpan, CoreError> {
    match previous {
        Some(previous) => SourceSpan::new(
            previous.start().min(next.start()),
            previous.end().max(next.end()),
        ),
        None => Ok(next),
    }
}

impl<'view, 'input, 'id> Graph<'view, 'input, 'id> {
    pub(super) fn metadata_len(&self) -> usize {
        self.metadata.len()
    }

    pub(super) fn binding(
        &self,
        member: Member<'view, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<Metadata>, CoreError> {
        work.check()?;
        if !member.has_metadata() {
            return Ok(None);
        }
        self.metadata.get(member.metadata as usize, work).map(Some)
    }

    #[cfg(test)]
    pub(super) fn prepare<'arena>(
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        captures: &mut capture::CaptureBuilder<'view, 'input, 'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        Self::prepare_for_config(
            native,
            ParseConfig::new(Profile::C0),
            builder,
            captures,
            work,
        )
    }

    pub(super) fn prepare_for_config<'arena>(
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        config: ParseConfig,
        builder: &mut b::DirectBuilder<'id>,
        captures: &mut capture::CaptureBuilder<'view, 'input, 'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        let extensions = config.extensions();
        let limits = work
            .model_limits()
            .ok_or(CoreError::UnsupportedConfiguration)?;
        let count = native.read(work, |access| Ok(access.created_nodes()))?;
        let mut graph = Self {
            nodes: BuildRows::new(limits.model_records, LimitKind::ModelRecords, work)?,
            flows: BuildRows::new(limits.model_records, LimitKind::ModelRecords, work)?,
            native_nodes: BuildRows::new(limits.native_records, LimitKind::NativeRecords, work)?,
            inline_html_marks: None,
            metadata: BuildRows::new(limits.model_records, LimitKind::ModelRecords, work)?,
            hosts: BuildRows::new(limits.native_records, LimitKind::NativeRecords, work)?,
            structural_extensions: false,
            reordered_footnotes: false,
            backlinks: BuildRows::new(limits.model_records, LimitKind::ModelRecords, work)?,
            table_of_contents: None,
            mermaid_enabled: extensions.contains(Extensions::MERMAID),
            math_enabled: extensions.contains(Extensions::MATH),
            callouts_enabled: extensions.contains(Extensions::CALLOUTS),
            inline_html_enabled: extensions.contains(Extensions::HTML_INLINE),
            inline_html_tagfilter: config.profile() == Profile::G3,
        };
        for _ in 0..count {
            graph.native_nodes.push(
                NativeSlot {
                    member: NONE,
                    domains: NONE,
                },
                work,
            )?;
        }
        let root = native.read(work, |access| access.root())?;
        let footnotes = if extensions.contains(Extensions::FOOTNOTES)
            && native.read(work, |access| Ok(access.has_footnote_definitions()))?
        {
            footnotes::NotePlan::prepare(native, root, work)?
        } else {
            None
        };
        graph.reordered_footnotes = footnotes
            .as_ref()
            .is_some_and(footnotes::NotePlan::has_visible_definitions);
        if extensions.contains(Extensions::TABLE_OF_CONTENTS)
            && native.read(work, |access| Ok(access.has_table_of_contents()))?
        {
            graph.table_of_contents = Some(toc::Plan::prepare(
                native,
                root,
                footnotes.as_ref(),
                graph.inline_html_enabled,
                builder,
                work,
            )?);
        }
        let mut preparation = Preparation {
            pending: BuildRows::new(limits.model_records, LimitKind::ModelRecords, work)?,
            footnotes,
        };
        let full = SourceSpan::new(0, native.source.input().source().text().len() as u64)?;
        let domains = builder.source_parts(&[full], work)?;
        graph.flows.push(
            Flow {
                reference: builder.reserve_flow(work)?,
                root: 0,
                domains,
                object: None,
            },
            work,
        )?;
        graph.reserve_flow(
            native,
            builder,
            root,
            Placement {
                parent: None,
                flow: 0,
                alt_root: false,
                initial: Some(full),
                emphasis_allowed: true,
            },
            (&mut preparation, &mut *captures),
            work,
        )?;
        let mut next = 0;
        while next < preparation.pending.len() {
            let queued = preparation.pending.get(next, work)?;
            let flow = graph.flows.len();
            graph.flows.push(
                Flow {
                    reference: builder.reserve_flow(work)?,
                    root: ordinal(graph.nodes.len()).map_err(|e| work.fail(e))?,
                    domains: queued.domains,
                    object: Some(queued.object),
                },
                work,
            )?;
            graph.reserve_flow(
                native,
                builder,
                queued.image,
                Placement {
                    parent: None,
                    flow,
                    alt_root: true,
                    initial: Some(queued.envelope),
                    emphasis_allowed: queued.emphasis_allowed,
                },
                (&mut preparation, &mut *captures),
                work,
            )?;
            next += 1;
        }
        drop(preparation.pending);
        // Only scalar envelope accumulation is needed from accepted claims.
        // Actual source rows are emitted later only for references that survive.
        let claims = native.read(work, |access| Ok(access.claim_count()))?;
        for index in 0..claims {
            let claim = native.read(work, |access| access.claim(index))?;
            let owner = native.read(work, |access| access.binding(claim.owner))?;
            let index = graph
                .native_nodes
                .get(owner.id.get() as usize - 1, work)?
                .member;
            if index != NONE {
                let index = index as usize;
                let node = graph.nodes.get(index, work)?.node;
                if builder.graph_node(node, work)?.kind != NodeKind::Text {
                    let mut envelope = builder.node_initial(node, work)?;
                    native.visit_sources(claim.source, work, |span, _| {
                        envelope = Some(union(envelope, span)?);
                        Ok(())
                    })?;
                    builder.set_node_initial(node, envelope, work)?;
                }
            }
        }
        Ok(graph)
    }

    /// Independent test-only measurement for manually constructed negative
    /// controls. Production NodeRows are written by the actual output DFS.
    #[cfg(test)]
    pub(super) fn measure(
        &self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        for index in 0..self.flows.len() {
            self.measure_flow(native, builder, index, work)?;
        }
        Ok(())
    }

    pub(super) fn member_for_lineage(
        &self,
        lineage: n::LineageToken<'view, 'input>,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<usize>, CoreError> {
        let binding = native.read(work, |access| access.binding(lineage))?;
        let index = self
            .native_nodes
            .get(binding.id.get() as usize - 1, work)?
            .member;
        Ok((index != NONE).then_some(index as usize))
    }

    /// Reuse only the actual full P/R host block. Each permission occurrence
    /// still receives its own authority; matching S geometry is never identity.
    pub(super) fn host_sources(
        &mut self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        source: n::SourceToken<'view, 'input>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(b::SourceBlockRef, n::SourceDomain<'view, 'input>), CoreError> {
        let domain = native.read(work, |a| a.source_domain(source))?;
        let Some(owner) = native.read(work, |a| a.source_owner(source))? else {
            return Ok((self.flows.get(0, work)?.domains, domain));
        };
        let binding = native.read(work, |a| a.binding(owner))?;
        let buffer = binding
            .buffer
            .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
        if !native.read(work, |a| a.same_source(buffer, domain.source))? {
            // A Table retains its header as well as the delimiter buffer. A
            // split Paragraph retains real claims on its original buffer after
            // receiving a new prefix buffer. Native source_owner proves this
            // full buffer capability; the final graph proves Paragraph kind.
            // Distinct buffers must never share the owner-indexed cache entry.
            let table = match native.read(work, |a| a.g3_for_lineage(owner))? {
                Some(extension) => matches!(
                    native.read(work, |a| a.g3_info(extension))?,
                    n::G3Info::Table { .. }
                ),
                None => false,
            };
            let paragraph = match self.member_for_lineage(owner, native, work)? {
                Some(index) => {
                    let member = self.nodes.get(index, work)?;
                    builder.graph_node(member.node, work)?.kind == NodeKind::Paragraph
                }
                None => false,
            };
            if !table && !paragraph {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            return Ok((native.sources(domain.source, builder, work)?.0, domain));
        }
        let index = binding.id.get() as usize - 1;
        let mut slot = self.native_nodes.get(index, work)?;
        let domains = if slot.domains == NONE {
            let rows = native.sources(domain.source, builder, work)?.0;
            let next = ordinal(self.hosts.len()).map_err(|e| work.fail(e))?;
            self.hosts.push(rows, work)?;
            slot.domains = next;
            self.native_nodes.replace(index, slot, work)?;
            rows
        } else {
            self.hosts.get(slot.domains as usize, work)?
        };
        Ok((domains, domain))
    }

    pub(super) fn html_emphasis(
        &self,
        id: native::NativeNodeId,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<(u32, b::SourceBlockRef)>, CoreError> {
        let InlineHtmlMark::Endpoint { host, parameter } = self.inline_html_mark(id, work)? else {
            return Ok(None);
        };
        let domains = self.hosts.get(host as usize, work)?;
        Ok(Some((u32::from(parameter), domains)))
    }

    pub(super) fn html_code_host(
        &self,
        id: native::NativeNodeId,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<b::SourceBlockRef>, CoreError> {
        let InlineHtmlMark::CodeHost(host) = self.inline_html_mark(id, work)? else {
            return Ok(None);
        };
        self.hosts.get(host as usize, work).map(Some)
    }

    fn inline_html_mark(
        &self,
        id: native::NativeNodeId,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<InlineHtmlMark, CoreError> {
        match &self.inline_html_marks {
            Some(marks) => marks.get(id.get() as usize - 1, work),
            None => Ok(InlineHtmlMark::None),
        }
    }

    fn write_inline_html_mark(
        &mut self,
        id: native::NativeNodeId,
        mark: InlineHtmlMark,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        if self.inline_html_marks.is_none() {
            let count = self.native_nodes.len();
            let mut marks = BuildRows::new(count as u64, LimitKind::NativeRecords, work)?;
            for _ in 0..count {
                marks.push(InlineHtmlMark::None, work)?;
            }
            self.inline_html_marks = Some(marks);
        }
        self.inline_html_marks
            .as_mut()
            .ok_or(CoreError::InternalInvariant)?
            .replace(id.get() as usize - 1, mark, work)
    }

    fn inline_tag<'arena>(
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        info: n::NodeInfo<'view, 'arena, 'input>,
        check_opaque: bool,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<InlineTag>, CoreError> {
        if info.kind != native::NativeKind::HtmlInline {
            return Ok(None);
        }
        let n::NodeAttributes::Html(value) = info.attributes else {
            return Err(work.fail(CoreError::InvalidProvenance));
        };
        native.read(work, |a| {
            let len = a.value_len(value)?;
            let mut cursor = a.value_cursor(value, 0, len)?;
            let mut prefix = [0_u8; 16];
            let mut ending = CodeTagEnd::default();
            let mut filled = 0;
            while filled < prefix.len() {
                let (count, next) = a.with_next_value_chunk(cursor, 16, |text, _| {
                    if check_opaque {
                        ending.observe(text);
                    }
                    let count = (prefix.len() - filled).min(text.len());
                    prefix[filled..filled + count].copy_from_slice(&text.as_bytes()[..count]);
                    Ok(count)
                })?;
                cursor = next;
                let Some(count) = count else { break };
                filled += count;
            }
            let tag = InlineTag::prefix(&prefix[..filled]);
            if check_opaque
                && matches!(tag, Some(InlineTag::Opaque { name, closing: false, .. }) if InlineTag::code_kind(name).is_some())
            {
                // Read the accepted value through its end for long whitespace
                // or attributes. UTF-8 cursor offsets never come from byte guesses.
                while let (Some(()), next) = a.with_next_value_chunk(cursor, 256, |text, _| {
                    ending.observe(text);
                    Ok(())
                })? {
                    cursor = next;
                }
                if ending.self_closing {
                    return Ok(None);
                }
            }
            Ok(tag)
        })
    }

    fn emphasis_candidate<'arena>(
        &self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        token: n::NodeToken<'view, 'arena, 'input>,
        info: n::NodeInfo<'view, 'arena, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<(HtmlEmphasis, Option<n::SourceToken<'view, 'input>>)>, CoreError> {
        if info.kind != native::NativeKind::HtmlInline
            || matches!(
                self.inline_html_mark(info.id, work)?,
                InlineHtmlMark::OpaqueChild
            )
        {
            return Ok(None);
        }
        let Some(InlineTag::Emphasis(tag)) = Self::inline_tag(native, info, false, work)? else {
            return Ok(None);
        };
        let lineage = native.read(work, |a| a.lineage(token))?;
        let binding = native.read(work, |a| a.binding(lineage))?;
        let source = match native.inline_html_copy(
            binding.value,
            3..=HtmlEmphasis::MAX_DELIMITER_BYTES,
            work,
        )? {
            Some((source, span, _))
                if HtmlEmphasis::delimiter(native.html_source(span, work)?) == Some(tag) =>
            {
                Some(source)
            }
            _ => None,
        };
        Ok(Some((tag, source)))
    }

    /// Walk the native inline subtree in source order, across Markdown
    /// formatting and links. Alt labels are classified in their own flow.
    fn mark_opaque_html<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        root: native::NativeNodeId,
        first: n::NodeToken<'view, 'arena, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let mut next = first;
        let mut raw = None;
        let max = work
            .model_limits()
            .ok_or(CoreError::UnsupportedConfiguration)?
            .native_records;
        let mut code = BuildRows::new(max, LimitKind::NativeRecords, work)?;
        let mut code_depth = 0_usize;
        loop {
            let mut row = native.read(work, |a| a.node(next))?;
            let tag = Self::inline_tag(native, row, raw.is_none(), work)?;
            let opaque = raw.is_some() || code_depth != 0;
            if !opaque && matches!(tag, Some(InlineTag::Comment)) {
                self.write_inline_html_mark(row.id, InlineHtmlMark::Comment, work)?;
            } else if opaque
                && (matches!(tag, Some(InlineTag::Emphasis(_)))
                    || row.kind == native::NativeKind::Image)
            {
                self.write_inline_html_mark(row.id, InlineHtmlMark::OpaqueChild, work)?;
            }
            if let Some(InlineTag::Opaque {
                name,
                closing,
                filtered,
            }) = tag
            {
                if let Some(open) = raw {
                    if closing && name == open && name != b"plaintext" {
                        raw = None;
                    }
                } else if let Some(kind) = InlineTag::code_kind(name) {
                    if !closing {
                        if code_depth == code.len() {
                            code.push(kind, work)?;
                        } else {
                            code.replace(code_depth, kind, work)?;
                        }
                        code_depth += 1;
                    } else if code_depth > 0 && code.get(code_depth - 1, work)? == kind {
                        code_depth -= 1;
                    }
                } else if !closing && (!self.inline_html_tagfilter || !filtered || code_depth != 0)
                {
                    raw = Some(name);
                }
            }

            if row.kind != native::NativeKind::Image
                && let Some(child) = row.first_child
            {
                next = child;
                continue;
            }
            loop {
                if let Some(sibling) = row.next_sibling {
                    next = sibling;
                    break;
                }
                let parent = row.parent.ok_or(CoreError::InvalidProvenance)?;
                row = native.read(work, |a| a.node(parent))?;
                if row.id == root {
                    return Ok(());
                }
            }
        }
    }

    /// A full sibling group must close in source order before its delimiters
    /// can stop contributing output. Rescan only closed groups to publish the
    /// markers, so malformed input cannot leave a partially accepted group.
    fn mark_html_emphasis<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        first: n::NodeToken<'view, 'arena, 'input>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let limit = work.require_model_limits()?.depth;
        let mut stack = BuildRows::new(limit, LimitKind::Depth, work)?;
        let mut depth = 0;
        let mut overdepth = 0_usize;
        let mut group = None;
        let mut next = Some(first);
        while let Some(token) = next {
            let info = native.read(work, |a| a.node(token))?;
            next = info.next_sibling;
            let Some((tag, source)) = self.emphasis_candidate(native, token, info, work)? else {
                continue;
            };
            if overdepth != 0 {
                if tag.closing {
                    overdepth -= 1;
                } else {
                    overdepth += 1;
                }
                continue;
            }
            let Some(source) = source else {
                depth = 0;
                group = None;
                continue;
            };
            let domain = native.read(work, |a| a.source_domain(source))?;
            if let Some((_, host)) = group
                && !native.read(work, |a| a.same_source(host, domain.source))?
            {
                depth = 0;
                group = None;
            }
            if !tag.closing {
                if depth as u64 >= limit {
                    // Preserve an over-nested group verbatim. Track its
                    // remaining raw extent without allocating more stack rows.
                    overdepth = depth + 1;
                    depth = 0;
                    group = None;
                    continue;
                }
                if group.is_none() {
                    group = Some((token, domain.source));
                }
                if depth == stack.len() {
                    stack.push(tag, work)?;
                } else {
                    stack.replace(depth, tag, work)?;
                }
                depth += 1;
                continue;
            }
            let Some(top) = depth.checked_sub(1) else {
                continue;
            };
            let opening = stack.get(top, work)?;
            if !opening.matches(tag) {
                depth = 0;
                group = None;
                continue;
            }
            depth = top;
            if top != 0 {
                continue;
            }
            let (first, _) = group.take().ok_or(CoreError::InvalidProvenance)?;
            let (domains, _) = self.host_sources(native, source, builder, work)?;
            let host = ordinal(self.hosts.len()).map_err(|error| work.fail(error))?;
            self.hosts.push(domains, work)?;
            let mut current = first;
            loop {
                let row = native.read(work, |a| a.node(current))?;
                if let Some((tag, Some(_))) = self.emphasis_candidate(native, current, row, work)? {
                    self.write_inline_html_mark(
                        row.id,
                        InlineHtmlMark::Endpoint {
                            host,
                            parameter: tag.parameter() as u8,
                        },
                        work,
                    )?;
                }
                if row.id == info.id {
                    break;
                }
                current = row.next_sibling.ok_or(CoreError::InvalidProvenance)?;
            }
        }
        Ok(())
    }

    fn effective<'arena>(
        &self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        mut token: n::NodeToken<'view, 'arena, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<n::NodeToken<'view, 'arena, 'input>, CoreError> {
        let mut depth = 0;
        loop {
            let info = native.read(work, |access| access.node(token))?;
            if info.kind != native::NativeKind::Escaped {
                return Ok(token);
            }
            depth += 1;
            let limit = work.require_model_limits()?.depth;
            work.check_limit(LimitKind::Depth, depth, limit)?;
            let child = info
                .first_child
                .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
            if native
                .read(work, |access| access.node(child))?
                .next_sibling
                .is_some()
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            token = child;
        }
    }

    fn reserve_node<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        token: n::NodeToken<'view, 'arena, 'input>,
        placement: Placement,
        preparation: (
            &mut Preparation<'view, 'arena, 'input, 'id>,
            &mut capture::CaptureBuilder<'view, 'input, 'id>,
        ),
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<ReserveFrame<'view, 'arena, 'input>, CoreError> {
        let (preparation, captures) = preparation;
        let Placement {
            parent,
            flow,
            alt_root,
            mut initial,
            emphasis_allowed,
        } = placement;
        let info = native.read(work, |access| access.node(token))?;
        let emphasis_allowed = emphasis_allowed
            && (info.kind != native::NativeKind::Image
                || !matches!(
                    self.inline_html_mark(info.id, work)?,
                    InlineHtmlMark::OpaqueChild
                ));
        let lineage = native.read(work, |access| access.lineage(token))?;
        let binding = native.read(work, |access| access.binding(lineage))?;
        let footnote = match preparation.footnotes.as_mut() {
            Some(plan) => plan.metadata(native, info.kind, binding, work)?,
            None => None,
        };
        let callout = if self.callouts_enabled && info.kind == native::NativeKind::BlockQuote {
            native.read(work, |access| access.callout(lineage))?
        } else {
            None
        };
        let derived_fence = if (self.mermaid_enabled || self.math_enabled) && !alt_root {
            if let n::NodeAttributes::Code {
                fenced: true,
                info: value,
                ..
            } = info.attributes
            {
                let len = native.read(work, |a| a.value_len(value))?;
                if len >= 4 {
                    let mut cursor = native.read(work, |a| a.value_cursor(value, 0, len))?;
                    let mut language = [0; b"mermaid".len()];
                    let mut length = 0;
                    loop {
                        let (finished, next) = native.read(work, |a| {
                            a.with_next_value_chunk(cursor, 256, |text, _| {
                                for byte in text.bytes() {
                                    if byte.is_ascii_whitespace() {
                                        if length > 0 {
                                            return Ok(true);
                                        }
                                    } else if let Some(slot) = language.get_mut(length) {
                                        *slot = byte;
                                        length += 1;
                                    } else {
                                        length += 1;
                                        return Ok(true);
                                    }
                                }
                                Ok(false)
                            })
                        })?;
                        if finished.unwrap_or(true) {
                            break;
                        }
                        cursor = next;
                    }
                    match language.get(..length) {
                        Some(b"mermaid") if self.mermaid_enabled => Some(FenceKind::Diagram),
                        Some(b"math") if self.math_enabled => Some(FenceKind::Math),
                        _ => None,
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        if derived_fence.is_some() {
            for source in [binding.fence_opening, binding.buffer, binding.fence_closing]
                .into_iter()
                .flatten()
            {
                native.visit_sources(source, work, |span, _| {
                    initial = Some(union(initial, span)?);
                    Ok(())
                })?;
            }
        }
        let mut structural_output = false;
        if !alt_root
            && matches!(
                info.kind,
                native::NativeKind::Table
                    | native::NativeKind::TableRow
                    | native::NativeKind::TableCell
                    | native::NativeKind::TaskCheckbox
            )
        {
            self.structural_extensions = true;
            let extension = native
                .read(work, |a| a.g3(token))?
                .ok_or(CoreError::InvalidProvenance)?;
            let accepted = native.read(work, |a| a.g3_info(extension))?;
            let source = match accepted {
                n::G3Info::Table { delimiter, .. } => delimiter,
                n::G3Info::Row { index, raw, .. } => {
                    structural_output =
                        extensions::structural_output(extensions::Position::Row(index));
                    raw
                }
                n::G3Info::Cell { column, raw, .. } => {
                    structural_output =
                        extensions::structural_output(extensions::Position::Cell(column));
                    raw
                }
                n::G3Info::Task { marker, .. } => marker,
            };
            native.visit_sources(source, work, |span, _| {
                initial = Some(union(initial, span)?);
                Ok(())
            })?;
        }
        let index = self.nodes.len();
        let inline_html = if info.kind == native::NativeKind::HtmlInline {
            self.inline_html_mark(info.id, work)?
        } else {
            InlineHtmlMark::None
        };
        if !alt_root {
            let slot_index = info.id.get() as usize - 1;
            let mut slot = self.native_nodes.get(slot_index, work)?;
            if slot.member != NONE {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            slot.member = ordinal(index).map_err(|e| work.fail(e))?;
            self.native_nodes.replace(slot_index, slot, work)?;
        }
        let html_code = !alt_root
            && matches!(
                info.attributes,
                n::NodeAttributes::InlineCode {
                    delimiter_length: 0,
                    ..
                }
            );
        if html_code {
            let syntax = binding.syntax.ok_or(CoreError::InvalidProvenance)?;
            let (domains, _) = self.host_sources(native, syntax, builder, work)?;
            let host = ordinal(self.hosts.len()).map_err(|error| work.fail(error))?;
            self.hosts.push(domains, work)?;
            self.write_inline_html_mark(info.id, InlineHtmlMark::CodeHost(host), work)?;
        }
        let has_detail = footnote.is_some()
            || callout.is_some()
            || html_code
            || !alt_root
                && matches!(
                    info.kind,
                    native::NativeKind::Heading
                        | native::NativeKind::List
                        | native::NativeKind::ListItem
                        | native::NativeKind::DescriptionItem
                        | native::NativeKind::CodeBlock
                        | native::NativeKind::HtmlBlock
                        | native::NativeKind::HtmlInline
                        | native::NativeKind::Link
                        | native::NativeKind::Table
                        | native::NativeKind::TableRow
                        | native::NativeKind::TableCell
                );
        let object = if !alt_root
            && (derived_fence.is_some()
                || matches!(
                    info.kind,
                    native::NativeKind::Image
                        | native::NativeKind::TaskCheckbox
                        | native::NativeKind::Math
                        | native::NativeKind::NoteEmbed
                )) {
            Some(builder.reserve_object(work)?)
        } else {
            None
        };
        let metadata = if let Some(kind) = derived_fence {
            Some(Metadata::Fence {
                kind,
                detail: builder.reserve_detail(work)?,
                object: object.ok_or(CoreError::InvalidProvenance)?,
            })
        } else if has_detail {
            Some(Metadata::Detail(builder.reserve_detail(work)?))
        } else {
            object.map(Metadata::Object)
        };
        let detail = metadata.and_then(Metadata::detail);
        let parent_ref = parent
            .map(|i| self.nodes.get(i, work).map(|row| row.node))
            .transpose()?;
        let flow_ref = self.flows.get(flow, work)?.reference;
        let html_break = self.inline_html_enabled
            && !alt_root
            && info.kind == native::NativeKind::HtmlInline
            && native.inline_html_break(binding.value, work)?.is_some();
        let node = builder.reserve_node(
            b::DraftNodeSeed {
                parent: parent_ref,
                flow: flow_ref,
                kind: if alt_root {
                    NodeKind::AltRoot
                } else if let Some(fence) = derived_fence {
                    fence.node()
                } else if html_break {
                    NodeKind::HardBreak
                } else if matches!(inline_html, InlineHtmlMark::Comment) {
                    NodeKind::HtmlComment
                } else {
                    kind(info.kind).map_err(|e| work.fail(e))?
                },
                detail,
                initial,
            },
            work,
        )?;
        // IDs are only checked ordinals here. All later writes use the original
        // scoped references stored in this graph, never a reminted raw ID.
        let seeded = builder.graph_node(node, work)?;
        if seeded.index != index || seeded.parent != parent || seeded.flow != flow {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let metadata = if let Some(binding) = metadata {
            let at = ordinal(self.metadata.len()).map_err(|e| work.fail(e))?;
            self.metadata.push(binding, work)?;
            at
        } else {
            NONE
        };
        let mut member = Member {
            lineage,
            node,
            metadata,
            subtree_end: 0,
            output: structural_output
                || object.is_some()
                || footnote.is_some()
                || callout.is_some()
                || (!alt_root && binding.terminal_synthetic),
            alt_root,
            callout_title: false,
        };
        if !alt_root {
            let count = native.read(work, |access| access.mapping_len(binding.value))?;
            for at in 0..count {
                let atom = native.read(work, |access| access.mapping_atom(binding.value, at))?;
                member.output |= atom.bytes != 0
                    && !matches!(
                        inline_html,
                        InlineHtmlMark::Comment | InlineHtmlMark::Endpoint { .. }
                    );
                native.visit_sources(atom.source, work, |span, _| {
                    initial = Some(union(initial, span)?);
                    Ok(())
                })?;
            }
            if let Some(syntax) = binding.syntax {
                native.visit_sources(syntax, work, |span, _| {
                    initial = Some(union(initial, span)?);
                    Ok(())
                })?;
            }
        }
        if let Some(object) = object.filter(|_| info.kind == native::NativeKind::Image) {
            let label = binding
                .label
                .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
            let (domains, _, _) = native.sources(label, builder, work)?;
            let mut envelope = None;
            native.visit_sources(label, work, |span, _| {
                envelope = Some(union(envelope, span)?);
                Ok(())
            })?;
            preparation.pending.push(
                PendingAlt {
                    image: token,
                    object,
                    domains,
                    envelope: envelope.ok_or_else(|| work.fail(CoreError::InvalidProvenance))?,
                    emphasis_allowed,
                },
                work,
            )?;
        }
        builder.set_node_initial(node, initial, work)?;
        self.nodes.push(member, work)?;
        if info.kind == native::NativeKind::TableOfContents {
            let plan = self
                .table_of_contents
                .as_mut()
                .ok_or(CoreError::InvalidProvenance)?;
            for _ in 0..plan.reserve_entries(work)? {
                let detail = builder.reserve_detail(work)?;
                let entry = builder.reserve_node(
                    b::DraftNodeSeed {
                        parent: Some(node),
                        flow: flow_ref,
                        kind: NodeKind::TableOfContentsEntry,
                        detail: Some(detail),
                        initial: None,
                    },
                    work,
                )?;
                self.nodes.push(
                    Member {
                        lineage,
                        node: entry,
                        metadata: NONE,
                        subtree_end: extent(self.nodes.len() + 1)
                            .map_err(|error| work.fail(error))?,
                        output: true,
                        alt_root: false,
                        callout_title: false,
                    },
                    work,
                )?;
            }
        }
        if metadata != NONE {
            captures.node(
                native,
                capture::NodeCaptureInput {
                    member: index,
                    metadata: metadata as usize,
                    node,
                    info,
                    lineage,
                    binding,
                    derived_fence,
                    footnote,
                    callout: callout.is_some(),
                },
                builder,
                work,
            )?;
        }
        if let Some(callout) = callout.filter(|callout| callout.title.is_none()) {
            let mut anchor = None;
            native.visit_sources(callout.marker, work, |part, _| {
                anchor = Some(part.end());
                Ok(())
            })?;
            let anchor = anchor.ok_or(CoreError::InvalidProvenance)?;
            let title = builder.reserve_node(
                b::DraftNodeSeed {
                    parent: Some(node),
                    flow: flow_ref,
                    kind: NodeKind::Paragraph,
                    detail: None,
                    initial: Some(SourceSpan::new(anchor, anchor)?),
                },
                work,
            )?;
            self.nodes.push(
                Member {
                    lineage,
                    node: title,
                    metadata: NONE,
                    subtree_end: extent(self.nodes.len() + 1).map_err(|error| work.fail(error))?,
                    output: true,
                    alt_root: false,
                    callout_title: true,
                },
                work,
            )?;
        }
        let next = if object.is_some() {
            None
        } else {
            info.first_child
        };
        if self.inline_html_enabled
            && emphasis_allowed
            && let Some(first) = next
            && (alt_root
                || matches!(
                    info.kind,
                    native::NativeKind::Paragraph
                        | native::NativeKind::Heading
                        | native::NativeKind::TableCell
                        | native::NativeKind::Emphasis
                        | native::NativeKind::Strong
                        | native::NativeKind::Link
                        | native::NativeKind::Strike
                        | native::NativeKind::Highlight
                        | native::NativeKind::Superscript
                        | native::NativeKind::Subscript
                ))
        {
            if alt_root
                || matches!(
                    info.kind,
                    native::NativeKind::Paragraph
                        | native::NativeKind::Heading
                        | native::NativeKind::TableCell
                )
            {
                self.mark_opaque_html(native, info.id, first, work)?;
            }
            self.mark_html_emphasis(native, first, builder, work)?;
        }
        Ok(ReserveFrame {
            node: index,
            next,
            emphasis_allowed,
        })
    }

    fn reserve_flow<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        root: n::NodeToken<'view, 'arena, 'input>,
        placement: Placement,
        preparation: (
            &mut Preparation<'view, 'arena, 'input, 'id>,
            &mut capture::CaptureBuilder<'view, 'input, 'id>,
        ),
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let (preparation, captures) = preparation;
        let mut stack = BuildRows::new(work.require_model_limits()?.depth, LimitKind::Depth, work)?;
        stack.push(
            self.reserve_node(
                native,
                builder,
                root,
                placement,
                (&mut *preparation, &mut *captures),
                work,
            )?,
            work,
        )?;
        let mut top = 1;
        let mut footnote = 0;
        while top > 0 {
            let mut frame = stack.get(top - 1, work)?;
            let root = top == 1 && placement.flow == 0;
            let next = if let Some(raw) = frame.next {
                let info = native.read(work, |access| access.node(raw))?;
                frame.next = info.next_sibling;
                stack.replace(top - 1, frame, work)?;
                if root
                    && preparation.footnotes.is_some()
                    && info.kind == native::NativeKind::FootnoteDefinition
                {
                    continue;
                }
                Some(raw)
            } else if root && let Some(plan) = &preparation.footnotes {
                let next = plan.at(footnote, work)?;
                if next.is_some() {
                    footnote += 1;
                }
                next
            } else {
                None
            };
            if let Some(raw) = next {
                let token = self.effective(native, raw, work)?;
                let child = self.reserve_node(
                    native,
                    builder,
                    token,
                    Placement {
                        parent: Some(frame.node),
                        flow: placement.flow,
                        alt_root: false,
                        initial: None,
                        emphasis_allowed: frame.emphasis_allowed,
                    },
                    (&mut *preparation, &mut *captures),
                    work,
                )?;
                if top == stack.len() {
                    stack.push(child, work)?;
                } else {
                    stack.replace(top, child, work)?;
                }
                top += 1;
            } else {
                let mut node = self.nodes.get(frame.node, work)?;
                if builder.graph_node(node.node, work)?.kind == NodeKind::FootnoteDefinition {
                    let plan = preparation
                        .footnotes
                        .as_ref()
                        .ok_or(CoreError::InvalidProvenance)?;
                    self.reserve_backlinks(native, builder, node, placement.flow, plan, work)?;
                }
                node.subtree_end = extent(self.nodes.len()).map_err(|e| work.fail(e))?;
                self.nodes.replace(frame.node, node, work)?;
                if let Some(parent) = builder.graph_node(node.node, work)?.parent {
                    let mut row = self.nodes.get(parent, work)?;
                    row.output |= node.output;
                    self.nodes.replace(parent, row, work)?;
                }
                top -= 1;
            }
        }
        Ok(())
    }

    fn reserve_backlinks<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        definition: Member<'view, 'input>,
        flow: usize,
        plan: &footnotes::NotePlan<'view, 'arena, 'input, 'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let id = native
            .read(work, |access| access.binding(definition.lineage))?
            .id;
        let (first, ordinal) = plan.first_reference(id.get(), work)?;
        let flow = self.flows.get(flow, work)?.reference;
        let footer_node = builder.reserve_node(
            b::DraftNodeSeed {
                parent: Some(definition.node),
                flow,
                kind: NodeKind::FootnoteFooter,
                detail: None,
                initial: None,
            },
            work,
        )?;
        let mut footer = Member {
            lineage: definition.lineage,
            node: footer_node,
            metadata: NONE,
            subtree_end: 0,
            output: true,
            alt_root: false,
            callout_title: false,
        };
        let footer_index = self.nodes.push(footer, work)?;
        let mut link = Some(first);
        while let Some(index) = link {
            let reference = plan.reference(index, work)?;
            let detail = builder.reserve_detail(work)?;
            let node = builder.reserve_node(
                b::DraftNodeSeed {
                    parent: Some(footer.node),
                    flow,
                    kind: NodeKind::FootnoteBacklink,
                    detail: Some(detail),
                    initial: None,
                },
                work,
            )?;
            let index = self.nodes.len();
            self.backlinks.push(
                Backlink {
                    index,
                    node,
                    reference: reference.lineage,
                    detail,
                    ordinal,
                    occurrence: reference.occurrence,
                },
                work,
            )?;
            self.nodes.push(
                Member {
                    lineage: definition.lineage,
                    node,
                    metadata: NONE,
                    subtree_end: extent(index + 1).map_err(|e| work.fail(e))?,
                    output: true,
                    alt_root: false,
                    callout_title: false,
                },
                work,
            )?;
            link = reference.next;
        }
        footer.subtree_end = extent(self.nodes.len()).map_err(|e| work.fail(e))?;
        self.nodes.replace(footer_index, footer, work)
    }

    pub(super) fn backlink(
        &self,
        member: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Backlink<'view, 'input>, CoreError> {
        let mut start = 0;
        let mut end = self.backlinks.len();
        while start < end {
            let middle = start + (end - start) / 2;
            let link = self.backlinks.get(middle, work)?;
            match link.index.cmp(&member) {
                std::cmp::Ordering::Less => start = middle + 1,
                std::cmp::Ordering::Greater => end = middle,
                std::cmp::Ordering::Equal => return Ok(link),
            }
        }
        Err(work.fail(CoreError::InvalidProvenance))
    }

    pub(super) fn first(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<usize>, CoreError> {
        let row = self.nodes.get(index, work)?;
        Ok((index + 1 < row.end()).then_some(index + 1))
    }
    pub(super) fn next(
        &self,
        index: usize,
        builder: &b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<usize>, CoreError> {
        let row = self.nodes.get(index, work)?;
        let Some(parent) = builder.graph_node(row.node, work)?.parent else {
            return Ok(None);
        };
        Ok((row.end() < self.nodes.get(parent, work)?.end()).then_some(row.end()))
    }

    /// Inspect only accepted sparse extension facts. Ordinary C0 nodes do not
    /// acquire extension rows or additional native reads.
    pub(super) fn position(
        &self,
        index: usize,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        builder: &b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<extensions::Position, CoreError> {
        use extensions::Position;
        if !self.structural_extensions {
            return Ok(Position::Ordinary);
        }
        let member = self.nodes.get(index, work)?;
        let projected = builder.graph_node(member.node, work)?;
        if matches!(projected.kind, NodeKind::TableRow | NodeKind::TableCell) {
            let token = native
                .read(work, |a| a.g3_for_lineage(member.lineage))?
                .ok_or(CoreError::InvalidProvenance)?;
            return match native.read(work, |a| a.g3_info(token))? {
                n::G3Info::Row { index, .. } if projected.kind == NodeKind::TableRow => {
                    Ok(Position::Row(index))
                }
                n::G3Info::Cell { column, .. } if projected.kind == NodeKind::TableCell => {
                    Ok(Position::Cell(column))
                }
                _ => Err(work.fail(CoreError::InvalidProvenance)),
            };
        }
        if projected.kind == NodeKind::Paragraph && index > 0 {
            let previous = self.nodes.get(index - 1, work)?;
            let node = builder.graph_node(previous.node, work)?;
            if node.kind == NodeKind::TaskCheckbox
                && node.parent == projected.parent
                && previous.end() == index
            {
                let token = native
                    .read(work, |a| a.g3_for_lineage(previous.lineage))?
                    .ok_or(CoreError::InvalidProvenance)?;
                let n::G3Info::Task { paragraph, .. } = native.read(work, |a| a.g3_info(token))?
                else {
                    return Err(work.fail(CoreError::InvalidProvenance));
                };
                let expected = native.read(work, |a| a.binding(paragraph))?.id;
                let actual = native.read(work, |a| a.binding(member.lineage))?.id;
                if expected == actual {
                    return Ok(Position::TaskLabel);
                }
            }
        }
        Ok(Position::Ordinary)
    }

    /// Resolve the accepted occurrence behind the shared projection decision;
    /// raw row boundaries and normalized text never substitute for cell facts.
    pub(super) fn separator_origin(
        &self,
        index: usize,
        separator: extensions::Separator,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        builder: &b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(b::NodeRef, usize, u64), CoreError> {
        let owner_index = match separator.owner {
            extensions::SeparatorOwner::Current => index,
            extensions::SeparatorOwner::TaskCheckbox => {
                index.checked_sub(1).ok_or(CoreError::InvalidProvenance)?
            }
        };
        let owner = self.nodes.get(owner_index, work)?;
        let row = builder.graph_node(owner.node, work)?;
        if matches!(
            row.kind,
            NodeKind::FootnoteFooter | NodeKind::TableOfContentsEntry
        ) || owner.callout_title
        {
            let at = builder
                .node_initial(owner.node, work)?
                .ok_or(CoreError::InvalidProvenance)?
                .start();
            return Ok((owner.node, owner_index, at));
        }
        let extension_index = if separator.kind == SyntheticKind::RowLineFeed {
            self.first(index, work)?
                .ok_or(CoreError::InvalidProvenance)?
        } else {
            owner_index
        };
        let member = self.nodes.get(extension_index, work)?;
        let source = if matches!(
            separator.kind,
            SyntheticKind::CellTab | SyntheticKind::RowLineFeed | SyntheticKind::TaskLabelSpace
        ) || row.kind == NodeKind::TaskCheckbox
        {
            let token = native
                .read(work, |a| a.g3_for_lineage(member.lineage))?
                .ok_or(CoreError::InvalidProvenance)?;
            match (separator.kind, native.read(work, |a| a.g3_info(token))?) {
                (
                    SyntheticKind::CellTab | SyntheticKind::RowLineFeed,
                    n::G3Info::Cell { raw, .. },
                ) => raw,
                (SyntheticKind::TaskLabelSpace, n::G3Info::Task { spacer, .. }) => spacer,
                (SyntheticKind::BlockLineFeed, n::G3Info::Task { marker, .. }) => marker,
                _ => return Err(work.fail(CoreError::InvalidProvenance)),
            }
        } else {
            let start = native
                .read(work, |a| a.binding(owner.lineage))?
                .start_p
                .ok_or(CoreError::InvalidProvenance)?;
            return Ok((
                owner.node,
                owner_index,
                native.source.p_anchor(start, work)?,
            ));
        };
        let mut anchor = None;
        native.visit_sources(source, work, |span, _| {
            if separator.kind == SyntheticKind::TaskLabelSpace {
                anchor = Some(span.end());
            } else if anchor.is_none() {
                anchor = Some(span.start());
            }
            Ok(())
        })?;
        Ok((
            owner.node,
            owner_index,
            anchor.ok_or(CoreError::InvalidProvenance)?,
        ))
    }

    #[cfg(test)]
    fn measure_flow(
        &self,
        native: &Native<'_, 'view, 'input, '_, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        flow_index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let flow = self.flows.get(flow_index, work)?;
        let mut stack = BuildRows::new(work.require_model_limits()?.depth, LimitKind::Depth, work)?;
        let mut top = 0;
        let mut next = Some((flow.root(), flow.object.is_none()));
        let mut cursor = 0_u64;
        let mut previous = false;
        loop {
            if let Some((index, separators)) = next {
                let row = self.nodes.get(index, work)?;
                let node = builder.graph_node(row.node, work)?;
                let position = self.position(index, native, builder, work)?;
                let display = !row.alt_root && extensions::block(node.kind);
                if extensions::before(node.kind, position, separators, row.output, previous)?
                    .is_some()
                {
                    cursor = cursor.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
                }
                previous |= extensions::marks_block(node.kind, position, separators, row.output);
                let begin = cursor;
                if self
                    .binding(row, work)?
                    .and_then(Metadata::object)
                    .is_some()
                {
                    cursor = cursor.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
                } else if node.kind == NodeKind::HardBreak && row.has_metadata() {
                    cursor = cursor.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
                } else if !row.alt_root {
                    let lineage = row.lineage;
                    let binding = native.read(work, |access| access.binding(lineage))?;
                    if node.kind != NodeKind::HtmlComment
                        && self.html_emphasis(binding.id, work)?.is_none()
                    {
                        cursor = cursor
                            .checked_add(native.mapping_width(binding.value, work)?)
                            .ok_or(CoreError::ArithmeticOverflow)?;
                    }
                    if binding.terminal_synthetic {
                        cursor = cursor.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
                    }
                }
                let anchor = if let Some(initial) = builder.node_initial(row.node, work)? {
                    initial
                } else if position == extensions::Position::TaskLabel {
                    // The retained Paragraph's old BlockStart precedes the
                    // accepted marker. Its real label starts after the accepted
                    // spacer, even when EOF leaves no paragraph-owned trim cut.
                    // position checked this exact task/paragraph relation.
                    let (_, _, at) = self.separator_origin(
                        index,
                        extensions::Separator {
                            kind: SyntheticKind::TaskLabelSpace,
                            owner: extensions::SeparatorOwner::TaskCheckbox,
                        },
                        native,
                        builder,
                        work,
                    )?;
                    SourceSpan::new(at, at)?
                } else {
                    let lineage = row.lineage;
                    let start = native
                        .read(work, |access| access.binding(lineage))?
                        .start_p
                        .unwrap_or(0);
                    let at = native.source.p_anchor(start, work)?;
                    SourceSpan::new(at, at)?
                };
                let frame = MeasureFrame {
                    node: index,
                    next: self.first(index, work)?,
                    begin,
                    envelope: anchor,
                    separators: separators && !display,
                };
                if top == stack.len() {
                    stack.push(frame, work)?;
                } else {
                    stack.replace(top, frame, work)?;
                }
                top += 1;
            }
            let mut frame = stack.get(top - 1, work)?;
            if let Some(child) = frame.next {
                frame.next = self.next(child, builder, work)?;
                stack.replace(top - 1, frame, work)?;
                next = Some((child, frame.separators));
            } else {
                let row = self.nodes.get(frame.node, work)?;
                let node = builder.graph_node(row.node, work)?;
                if node.flow != flow_index {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                let parent = node
                    .parent
                    .map(|i| self.nodes.get(i, work).map(|r| r.node))
                    .transpose()?;
                let first_child = self
                    .first(frame.node, work)?
                    .map(|i| self.nodes.get(i, work).map(|r| r.node))
                    .transpose()?;
                let next_sibling = self
                    .next(frame.node, builder, work)?
                    .map(|i| self.nodes.get(i, work).map(|r| r.node))
                    .transpose()?;
                builder.write_node(
                    row.node,
                    b::DraftNodeRow {
                        parent,
                        first_child,
                        next_sibling,
                        flow: flow.reference,
                        kind: node.kind,
                        detail: self.binding(row, work)?.and_then(Metadata::detail),
                        source_envelope: frame.envelope,
                        logical: LogicalSpan::new(frame.begin, cursor)?,
                    },
                    work,
                )?;
                top -= 1;
                if top == 0 {
                    break;
                }
                let mut parent = stack.get(top - 1, work)?;
                parent.envelope = union(Some(parent.envelope), frame.envelope)?;
                stack.replace(top - 1, parent, work)?;
                next = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SourceKey;
    use crate::parse::prepared::NativeFault;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    enum GraphCase<'case> {
        Sparse,
        Fifo,
        Foreign(&'case CancellationToken),
    }

    struct GraphCompiler<'case, 'id> {
        case: GraphCase<'case>,
        brand: std::marker::PhantomData<fn(&'id ()) -> &'id ()>,
    }

    impl<'input, 'source, 'work, 'id, 'op>
        n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
        for GraphCompiler<'_, 'id>
    {
        type Output = b::DirectBuilder<'id>;
        type State<'view> = CoreState<'view, 'input, 'id>;

        fn ast<'store, 'view, 'arena>(
            &mut self,
            reader: n::AstReader<
                'store,
                'view,
                'arena,
                'input,
                ConstructionId<'id>,
                NativeLease<'id>,
            >,
            host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
        ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
            host.native_parts(|source, work| compile_ast(&AstNative::new(reader, source), work))
        }

        fn provenance<'store, 'view>(
            &mut self,
            reader: n::ProvenanceReader<
                'store,
                'view,
                'input,
                ConstructionId<'id>,
                NativeLease<'id>,
            >,
            state: Self::State<'view>,
            host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
        ) -> Result<Self::Output, NativeFault<'input, 'id>> {
            host.native_parts(|source, work| {
                let native = Native { reader, source };
                let CoreState {
                    mut graph,
                    captured,
                    mut builder,
                } = state;
                match self.case {
                    GraphCase::Sparse => inspect_sparse(&native, &mut graph, &mut builder, work)?,
                    GraphCase::Fifo => {}
                    GraphCase::Foreign(cancel) => {
                        let mut foreign = b::DirectBuilder::new(work)?;
                        let reference = foreign.reserve_detail(work)?;
                        graph
                            .metadata
                            .replace(0, Metadata::Detail(reference), work)?;
                        let result =
                            metadata::emit(&native, &mut graph, &mut builder, captured, work);
                        assert!(matches!(result, Err(CoreError::InvalidProvenance)));
                        assert_eq!(work.first_fault(), Some(CoreError::InvalidProvenance));
                        cancel.cancel();
                        assert_eq!(work.check(), Err(CoreError::InvalidProvenance));
                        return Err(CoreError::InvalidProvenance);
                    }
                }
                let images = metadata::emit(&native, &mut graph, &mut builder, captured, work)?;
                target::flows(&native, &mut graph, &mut builder, &images, work)?;
                builder.complete_nodes(work)?;
                match self.case {
                    GraphCase::Sparse => inspect_sparse_rows(&graph, &mut builder, work)?,
                    GraphCase::Fifo => inspect_fifo(&native, &mut graph, &mut builder, work)?,
                    GraphCase::Foreign(_) => unreachable!(),
                }
                drop((images, graph));
                Ok(builder)
            })
        }
    }

    fn inspect_sparse<'view, 'input, 'id>(
        native: &Native<'_, 'view, 'input, '_, 'id>,
        graph: &mut Graph<'view, 'input, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        assert!(graph.nodes.page_count() >= 3);
        assert!(graph.native_nodes.page_count() >= 2);
        assert_eq!(graph.flows.len(), 129);
        assert_eq!(graph.metadata.len(), 3 * 128);
        assert_eq!(graph.hosts.len(), 0);
        assert!(graph.metadata.len() < graph.nodes.len());
        let mut details = 0;
        let mut objects = 0;
        let mut metadata_count = 0;
        let mut host_source = None;
        for index in 0..graph.nodes.len() {
            let member = graph.nodes.get(index, work)?;
            assert!(index < member.end() && member.end() <= graph.nodes.len());
            let projected = builder.graph_node(member.node, work)?;
            assert_eq!(projected.index, index);
            if let Some(parent) = projected.parent {
                assert!(parent < index);
                assert!(member.end() <= graph.nodes.get(parent, work)?.end());
            }
            if let Some(metadata) = member.metadata_index() {
                assert!(!member.alt_root);
                assert_eq!(metadata, metadata_count);
                metadata_count += 1;
            }
            match graph.binding(member, work)? {
                Some(Metadata::Detail(_)) => {
                    details += 1;
                }
                Some(Metadata::Object(_)) => {
                    objects += 1;
                }
                Some(Metadata::Fence { .. }) => panic!("opt-out fixture has no derived fence"),
                None => {}
            }
            let lineage = member.lineage;
            let binding = native.read(work, |a| a.binding(lineage))?;
            if !member.alt_root {
                assert_eq!(
                    graph.member_for_lineage(lineage, native, work)?,
                    Some(index)
                );
            }
            host_source = host_source.or(binding.buffer);
        }
        assert_eq!((details, objects), (256, 128));
        assert_eq!(metadata_count, graph.metadata_len());
        let host_source = host_source.expect("real block buffer");
        let (first, _) = graph.host_sources(native, host_source, builder, work)?;
        assert_eq!(graph.hosts.len(), 1);
        let (again, _) = graph.host_sources(native, host_source, builder, work)?;
        assert_eq!(first, again);
        assert_eq!(graph.hosts.len(), 1);
        eprintln!(
            "Graph physical layouts: member={} metadata={} native_slot={} host_ref={} nodes={} metadata_rows={}",
            size_of::<Member<'_, '_>>(),
            size_of::<Metadata>(),
            size_of::<NativeSlot>(),
            size_of::<b::SourceBlockRef>(),
            graph.nodes.len(),
            graph.metadata.len()
        );
        Ok(())
    }

    // Seed/cache assertions above precede metadata. Actual final row assertions
    // follow DFS completion, preserving both checks at their real lifecycle.
    fn inspect_sparse_rows<'view, 'input, 'id>(
        graph: &Graph<'view, 'input, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        for index in 0..graph.nodes.len() {
            let member = graph.nodes.get(index, work)?;
            let row = builder.read_node(member.node, work)?;
            match graph.binding(member, work)? {
                Some(Metadata::Detail(_)) => assert!(row.detail.is_some()),
                Some(Metadata::Object(_)) => {
                    assert_eq!(row.kind, NodeKind::Image);
                    assert!(row.detail.is_none());
                }
                Some(Metadata::Fence { .. }) => panic!("opt-out fixture has no derived fence"),
                None => assert!(row.detail.is_none()),
            }
        }
        Ok(())
    }

    fn inspect_fifo<'view, 'input, 'id>(
        native: &Native<'_, 'view, 'input, '_, 'id>,
        graph: &mut Graph<'view, 'input, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        assert_eq!(graph.flows.len(), 3);
        let main = graph.flows.get(0, work)?;
        assert_eq!(main.root(), 0);
        let main_end = graph.nodes.get(main.root(), work)?.end();
        let mut next_root = main_end;
        let mut previous_image = None;
        for flow_index in 1..3 {
            let flow = graph.flows.get(flow_index, work)?;
            assert_eq!(flow.root(), next_root);
            let alt = graph.nodes.get(flow.root(), work)?;
            let image_index = graph
                .member_for_lineage(alt.lineage, native, work)?
                .unwrap();
            assert!(image_index < main_end);
            assert!(previous_image.is_none_or(|previous| previous < image_index));
            previous_image = Some(image_index);
            let image = graph.nodes.get(image_index, work)?;
            assert_ne!(image.node, alt.node);
            assert_eq!(
                graph.binding(image, work)?.and_then(Metadata::object),
                flow.object
            );
            let projected = builder.graph_node(alt.node, work)?;
            assert_eq!(projected.index, flow.root());
            assert_eq!(projected.parent, None);
            assert_eq!(projected.flow, flow_index);
            let row = builder.read_node(alt.node, work)?;
            assert_eq!(row.kind, NodeKind::AltRoot);
            if flow_index == 1 {
                assert!(!alt.output);
                assert_eq!(alt.end(), flow.root() + 1);
                assert_eq!(row.logical, LogicalSpan::new(0, 0)?);
                // The actual empty label is Some(2..2), not the
                // absent initial which would use a host anchor.
                assert_eq!(row.source_envelope, SourceSpan::new(2, 2)?);
            } else {
                assert!(alt.output);
                assert_eq!(row.logical, LogicalSpan::new(0, 1)?);
            }
            next_root = alt.end();
        }
        assert_eq!(next_root, graph.nodes.len());
        for index in 0..graph.nodes.len() {
            let member = graph.nodes.get(index, work)?;
            let projected = builder.graph_node(member.node, work)?;
            assert_eq!(projected.index, index);
            let row = builder.read_node(member.node, work)?;
            assert_eq!(projected.kind, row.kind);
            assert_eq!(projected.parent, row.parent.map(|id| id.index()));
            assert_eq!(projected.flow, row.flow.index());
        }
        Ok(())
    }

    #[test]
    fn ordinal_sentinel_does_not_exclude_maximum_subtree_end() {
        assert_eq!(ordinal(0), Ok(0));
        assert_eq!(ordinal(u32::MAX as usize - 1), Ok(u32::MAX - 1));
        assert_eq!(
            ordinal(u32::MAX as usize),
            Err(CoreError::ArithmeticOverflow)
        );
        assert_eq!(extent(u32::MAX as usize), Ok(u32::MAX));
        if let Some(too_large) = (u32::MAX as usize).checked_add(1) {
            assert_eq!(extent(too_large), Err(CoreError::ArithmeticOverflow));
        }
        assert_eq!(size_of::<NativeSlot>(), 2 * size_of::<u32>());
    }

    #[test]
    fn real_sparse_graph_crosses_pages_keeps_scoped_bindings_and_reuses_hosts() {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        let text = "# h\n\ntext **bold**\n\n![a](u)\n\n    code\n\n".repeat(128);
        let source = SourceDocument::new(
            SourceKey::new([103; 16], 1).unwrap(),
            text.as_bytes(),
            &mut operation,
        )
        .unwrap()
        .snapshot();
        construction_scope(&mut operation, |mut work| {
            let input = ParseInput::prepare_owned(source.clone(), &mut work)?;
            let builder = with_prepared(&input, &mut work, |input, mut host| {
                let result = n::compile(
                    input,
                    &options::c0(),
                    &mut host,
                    GraphCompiler {
                        case: GraphCase::Sparse,
                        brand: std::marker::PhantomData,
                    },
                );
                host.resolve(result)
            })?;
            let ir = builder.finish(&mut work)?;
            crate::ir::validate::validate_owned(&ir, &source, &mut work)?;
            assert_eq!(ir.objects().len(), 128);
            drop(ir);
            drop(input);
            work.finish()
        })
        .unwrap();
        drop(source);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn unified_rows_keep_fifo_alt_identity_and_empty_output_nodes() {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([105; 16], 1).unwrap(),
            b"![](u)\n\n![a](v)\n",
            &mut operation,
        )
        .unwrap()
        .snapshot();
        construction_scope(&mut operation, |mut work| {
            let input = ParseInput::prepare_owned(source.clone(), &mut work)?;
            let builder = with_prepared(&input, &mut work, |input, mut host| {
                let result = n::compile(
                    input,
                    &options::c0(),
                    &mut host,
                    GraphCompiler {
                        case: GraphCase::Fifo,
                        brand: std::marker::PhantomData,
                    },
                );
                host.resolve(result)
            })?;
            let ir = builder.finish(&mut work)?;
            crate::ir::validate::validate_owned(&ir, &source, &mut work)?;
            assert_eq!(ir.objects().len(), 2);
            drop((ir, input));
            work.finish()
        })
        .unwrap();
        drop(source);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn sparse_metadata_keeps_builder_scope_and_first_fault_after_cancel() {
        let budget = SourceBudget::model_fixture();
        let cancel = CancellationToken::new();
        let mut operation = budget.begin(cancel.clone());
        let source = SourceDocument::new(
            SourceKey::new([104; 16], 1).unwrap(),
            b"# h",
            &mut operation,
        )
        .unwrap()
        .snapshot();
        let result = construction_scope(&mut operation, |mut work| {
            let input = ParseInput::prepare_owned(source.clone(), &mut work)?;
            with_prepared(&input, &mut work, |input, mut host| {
                let result = n::compile(
                    input,
                    &options::c0(),
                    &mut host,
                    GraphCompiler {
                        case: GraphCase::Foreign(&cancel),
                        brand: std::marker::PhantomData,
                    },
                )
                .map(|_| ());
                host.resolve(result)
            })
        });
        assert_eq!(result, Err(CoreError::InvalidProvenance));
        assert_eq!(operation.first_fault(), Some(CoreError::InvalidProvenance));
        drop(source);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
