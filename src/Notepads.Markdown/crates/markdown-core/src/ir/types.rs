// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Compact model data. Only crate-internal builders create identities and ranges.

use crate::error::CoreError;
use crate::identity::{LocalUtf8Span, LogicalSpan, SourceSpan};
use std::marker::PhantomData;
use std::num::NonZeroU32;

macro_rules! id {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(NonZeroU32);

        impl crate::memory::BuildValue for $name {}

        impl $name {
            pub(crate) fn from_index(index: usize) -> Result<Self, CoreError> {
                let value = u32::try_from(index)
                    .map_err(|_| CoreError::ArithmeticOverflow)?
                    .checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
                Ok(Self(NonZeroU32::new(value).ok_or(CoreError::InternalInvariant)?))
            }

            pub fn index(self) -> usize { (self.0.get() - 1) as usize }
            pub fn value(self) -> u32 { self.0.get() }
        }
    )+};
}

id!(
    NodeId,
    FlowId,
    AtomId,
    DetailId,
    ObjectId,
    PayloadId,
    DefinitionId,
    ReferenceUseId,
    ExtraAuthorityId,
    ProofId
);

// Only the handwritten draft factory returns an attribute identity. Accepted
// models expose the immutable attribute rows through their owner relationships.
#[cfg(test)]
id!(AttributeId);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(super) enum ClaimStorage {
    Primary(AtomId),
    Extra(ExtraAuthorityId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ClaimId {
    storage: ClaimStorage,
}

impl ClaimId {
    pub(super) fn primary(atom: AtomId) -> Self {
        Self {
            storage: ClaimStorage::Primary(atom),
        }
    }
    pub(super) fn extra(extra: ExtraAuthorityId) -> Self {
        Self {
            storage: ClaimStorage::Extra(extra),
        }
    }
    pub(super) fn storage(self) -> ClaimStorage {
        self.storage
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rows<T: Copy> {
    start: u32,
    len: u32,
    _type: PhantomData<T>,
}

// Rows stores only two integers and PhantomData; it never retains T or a borrow.
impl<T: Copy> crate::memory::BuildValue for Rows<T> {}

macro_rules! task_values {
    ($($ty:ty),+ $(,)?) => {$(impl crate::memory::BuildValue for $ty {})+};
}

// Audited owned IR records: integer identities/ranges and flat enums only.
task_values!(
    ClaimId,
    TextSlice,
    NodeRow,
    FlowRow,
    AtomRow,
    AtomFields,
    PrimaryAuthority,
    AuthorityBinding,
    AtomPacket,
    ExtraAuthority,
    DetailRow,
    ObjectRow,
    PayloadRow,
    DefinitionRow,
    ReferenceUseRow,
    HostRow,
    AuthoritySpec,
    PrimitiveRow,
    StageRow,
    ProofRow,
    AttributeRow,
    Alignment,
);

impl<T: Copy> Rows<T> {
    pub(crate) fn new(start: usize, len: usize) -> Result<Self, CoreError> {
        let start = u32::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?;
        let len = u32::try_from(len).map_err(|_| CoreError::ArithmeticOverflow)?;
        start
            .checked_add(len)
            .ok_or(CoreError::ArithmeticOverflow)?;
        Ok(Self {
            start,
            len,
            _type: PhantomData,
        })
    }

    pub fn start(self) -> usize {
        self.start as usize
    }
    pub fn len(self) -> usize {
        self.len as usize
    }
    pub fn is_empty(self) -> bool {
        self.len == 0
    }

    pub(crate) fn slice<R: super::row_view::RowRange<T>>(
        self,
        rows: R,
    ) -> Result<R::View, CoreError> {
        self.start()
            .checked_add(self.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
        rows.bounded(self.start(), self.len())
            .ok_or(CoreError::InvalidProvenance)
    }

    pub(crate) fn indices(self, total: usize) -> Result<std::ops::Range<usize>, CoreError> {
        let end = self
            .start()
            .checked_add(self.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
        if end > total {
            return Err(CoreError::InvalidProvenance);
        }
        Ok(self.start()..end)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl TextSlice {
    pub fn start(self) -> usize {
        self.start as usize
    }
    pub fn len(self) -> usize {
        self.len as usize
    }
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Document,
    AltRoot,
    Paragraph,
    Heading,
    BlockQuote,
    List,
    ListItem,
    DescriptionList,
    DescriptionItem,
    DescriptionTerm,
    DescriptionDetails,
    CodeBlock,
    HtmlBlock,
    ThematicBreak,
    TableOfContents,
    TableOfContentsEntry,
    Table,
    TableRow,
    TableCell,
    Text,
    SoftBreak,
    HardBreak,
    InlineCode,
    Emphasis,
    Strong,
    Highlight,
    Superscript,
    Subscript,
    Strike,
    Link,
    Image,
    NoteEmbed,
    TaskCheckbox,
    HtmlInline,
    Math,
    Diagram,
    HtmlComment,
    FootnoteDefinition,
    FootnoteReference,
    FootnoteFooter,
    FootnoteBacklink,
    MathBlock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeRow {
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub flow: FlowId,
    pub kind: NodeKind,
    pub detail: Option<DetailId>,
    pub source_envelope: SourceSpan,
    pub logical: LogicalSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowScope {
    Main,
    ImageAlt { object: ObjectId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlowRow {
    pub scope: FlowScope,
    pub root: NodeId,
    pub domains: Rows<SourceSpan>,
    pub atoms: Rows<AtomRow>,
    pub logical_utf16: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomKind {
    Copy,
    Transform,
    Hidden,
    Synthetic,
    Object,
}

/// Units are UTF16 for a flow and UTF8 for a payload; its owning table fixes the domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetSpan {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomContent {
    Empty,
    Pool(TextSlice),
    Object(ObjectId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtomRow {
    pub owner: NodeId,
    pub kind: AtomKind,
    pub target: TargetSpan,
    pub sources: Rows<SourceSpan>,
    pub claim: ClaimId,
    pub proof: Option<ProofId>,
    pub content: AtomContent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtomFields {
    pub owner: NodeId,
    pub kind: AtomKind,
    pub target: TargetSpan,
    pub sources: Rows<SourceSpan>,
    pub proof: Option<ProofId>,
    pub content: AtomContent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimaryAuthority {
    pub phase: NativePhase,
    pub kind: ClaimKind,
    pub consumed: LocalUtf8Span,
    pub host_role: HostRole,
    pub host_utf8_len: u64,
    pub host_domains: Rows<SourceSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityBinding {
    Owned(PrimaryAuthority),
    Shared(ClaimId),
}

// Conservative selector-read fee; this describes a typed operation, not the
// physical enum tag layout or a stable ABI.
pub(super) const AUTHORITY_SELECTOR_BYTES: u64 =
    size_of::<std::mem::Discriminant<AuthorityBinding>>() as u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtomPacket {
    pub atom: AtomFields,
    pub authority: AuthorityBinding,
}

// The sole production BuildRecord implementation. This is the original atom_at
// field projection and fee order, with no loop, callback, allocation or borrow.
impl crate::memory::BuildRecord for AtomPacket {
    type Output = AtomRow;

    fn project(
        &self,
        index: usize,
        op: &mut crate::policy::ConstructionCharges<'_, '_, '_>,
    ) -> Result<AtomRow, CoreError> {
        op.view_scan(size_of::<AtomFields>() as u64 + AUTHORITY_SELECTOR_BYTES)?;
        op.view_copy(size_of::<AtomRow>() as u64)?;
        let claim = match self.authority {
            AuthorityBinding::Owned(_) => {
                ClaimId::primary(AtomId::from_index(index).map_err(|error| op.fail(error))?)
            }
            AuthorityBinding::Shared(id) => {
                op.view_scan(size_of::<ClaimId>() as u64)?;
                id
            }
        };
        Ok(AtomRow {
            owner: self.atom.owner,
            kind: self.atom.kind,
            target: self.atom.target,
            sources: self.atom.sources,
            claim,
            proof: self.atom.proof,
            content: self.atom.content,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtraAuthority {
    pub value: AuthoritySpec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeadingForm {
    Atx,
    Setext,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListMarker {
    Bullet(u8),
    Ordered { delimiter: u8 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeForm {
    Indented,
    Fenced { byte: u8, length: u32 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetailRow {
    Heading {
        level: u8,
        form: HeadingForm,
        claim: ClaimId,
    },
    List {
        marker: ListMarker,
        start: u64,
        tight: bool,
        claim: ClaimId,
    },
    Item {
        occurrence: u32,
        marker: ListMarker,
        value: u64,
        claim: ClaimId,
    },
    DescriptionItem {
        tight: bool,
        claim: ClaimId,
    },
    Code {
        form: CodeForm,
        info: Option<PayloadId>,
        claim: ClaimId,
    },
    Html {
        claim: ClaimId,
    },
    Table {
        columns: u32,
        alignments: Rows<Alignment>,
        claim: ClaimId,
    },
    Row {
        index: u32,
        header: bool,
        claim: ClaimId,
    },
    Cell {
        column: u32,
        missing: bool,
        claim: ClaimId,
    },
    Link {
        destination: PayloadId,
        title: Option<PayloadId>,
        reference: Option<ReferenceUseId>,
        claim: ClaimId,
    },
    FootnoteDefinition {
        ordinal: u32,
        claim: ClaimId,
    },
    FootnoteReference {
        definition: NodeId,
        ordinal: u32,
        occurrence: u32,
        claim: ClaimId,
        closing: Option<ClaimId>,
    },
    FootnoteBacklink {
        reference: NodeId,
        ordinal: u32,
        occurrence: u32,
        claim: ClaimId,
    },
    TableOfContentsEntry {
        heading: NodeId,
        depth: u8,
        claim: ClaimId,
    },
    Callout {
        kind: comrak::native::CalloutKind,
        disclosure: comrak::native::CalloutDisclosure,
        title: NodeId,
        claim: ClaimId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskAction {
    pub source: SourceSpan,
    pub expected: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathSyntax {
    DollarInline,
    DollarDisplay,
    LatexInline,
    LatexDisplay,
    CodeInline,
    FencedDisplay,
}

impl MathSyntax {
    pub const fn is_display(self) -> bool {
        matches!(
            self,
            Self::DollarDisplay | Self::LatexDisplay | Self::FencedDisplay
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DerivedKind {
    Math(MathSyntax),
    Diagram,
    NoteEmbed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectRow {
    Derived {
        node: NodeId,
        atom: AtomId,
        kind: DerivedKind,
        input: PayloadId,
        export_sources: Rows<SourceSpan>,
    },
    Image {
        node: NodeId,
        atom: AtomId,
        alt: FlowId,
        destination: PayloadId,
        title: Option<PayloadId>,
        reference: Option<ReferenceUseId>,
        wiki_size: Option<(u16, u16)>,
    },
    Task {
        node: NodeId,
        atom: AtomId,
        item: NodeId,
        checked: bool,
        marker: Rows<SourceSpan>,
        spacer: Rows<SourceSpan>,
        action: Option<TaskAction>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadRole {
    Destination,
    Title,
    FenceInfo,
    ReferenceLabel,
    MathInput,
    FenceInput,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadOwner {
    Node(NodeId),
    Definition(DefinitionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadRow {
    pub owner: PayloadOwner,
    pub role: PayloadRole,
    pub value: TextSlice,
    pub domains: Rows<SourceSpan>,
    pub atoms: Rows<AtomRow>,
    pub utf8_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefinitionRow {
    pub syntax: Rows<SourceSpan>,
    pub label: PayloadId,
    pub destination: PayloadId,
    pub title: Option<PayloadId>,
    pub key: TextSlice,
    pub winner: bool,
    pub claim: ClaimId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceUseRow {
    pub node: NodeId,
    pub syntax: Rows<SourceSpan>,
    pub winner: DefinitionId,
    pub key: TextSlice,
    pub destination: PayloadId,
    pub title: Option<PayloadId>,
    pub claim: ClaimId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRole {
    Block,
    Inline,
    Destination,
    Title,
    FenceInfo,
    ReferenceLabel,
    Structure,
    MathInput,
    FenceInput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostRow {
    pub owner: NodeId,
    pub role: HostRole,
    pub utf8_len: u64,
    pub domains: Rows<SourceSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePhase {
    Block,
    Inline,
    Metadata,
    Structure,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticKind {
    BlockLineFeed,
    CellTab,
    RowLineFeed,
    TaskLabelSpace,
    HttpScheme,
    MailtoScheme,
    CodeTerminalLineFeed,
    FootnoteNumber,
    FootnoteLabel,
    FootnoteBacklink,
    CalloutTitle,
    TableOfContentsLabel,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimKind {
    Block,
    ContainerPrefix,
    Delimiter,
    Break,
    Entity,
    EmojiShortcode,
    Escape,
    Code,
    Html,
    Link,
    WikiLink,
    Image,
    NoteEmbed,
    ReferenceDefinition,
    ReferenceUse,
    LeadingBom,
    Whitespace,
    HeadingIdentifier,
    TableOfContentsMarker,
    FootnoteMarker,
    FootnoteReference,
    InlineFootnoteOpening,
    InlineFootnoteBody,
    InlineFootnoteClosing,
    CalloutMarker,
    DescriptionMarker,
    Task,
    Table,
    ProjectionSeparator(SyntheticKind),
    Payload,
    Math(MathSyntax),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthoritySpec {
    pub owner: NodeId,
    pub host: HostRow,
    pub phase: NativePhase,
    pub kind: ClaimKind,
    pub consumed: LocalUtf8Span,
    pub sources: Rows<SourceSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveKind {
    Copy,
    CrLf,
    Cr,
    Nul,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimitiveRow {
    pub source: SourceSpan,
    pub kind: PrimitiveKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageKind {
    NamedEntity,
    NumericEntity,
    EmojiShortcode,
    AsciiEscape,
    PartialTab,
    TableEscapedPipe,
    SoftBreakToSpace,
    CodeLineEndingToSpace,
    CodeEdgeTrim,
    AsciiTrim,
    HtmlBreakToLineFeed,
    HtmlEmphasisDelimiter,
    HtmlCodeDelimiter,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageRow {
    pub kind: StageKind,
    pub input: LocalUtf8Span,
    pub output: TextSlice,
    pub parameter: u32,
    pub accepted_by: ClaimId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProofRow {
    pub atom: AtomId,
    pub primitives: Rows<PrimitiveRow>,
    pub stages: Rows<StageRow>,
    pub value: TextSlice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttributeKind {
    HeadingLevel,
    HeadingForm,
    ListMarker,
    ListStart,
    ListTightness,
    ItemMarker,
    CodeForm,
    CodeFence,
    TableColumns,
    TableAlignment,
    RowHeader,
    CellMissing,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttributeRow {
    pub owner: NodeId,
    pub kind: AttributeKind,
    pub claim: ClaimId,
    pub sources: Rows<SourceSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TextTarget {
    Flow(FlowId),
    Payload(PayloadId),
}
