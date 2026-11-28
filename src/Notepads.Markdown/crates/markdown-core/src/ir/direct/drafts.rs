// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Construction fields carry their issuing scope on every table reference.

use super::*;
use crate::identity::{LocalUtf8Span, LogicalSpan};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftNodeSeed {
    pub parent: Option<NodeRef>,
    pub flow: FlowRef,
    pub kind: NodeKind,
    pub detail: Option<DetailRef>,
    pub initial: Option<SourceSpan>,
}

/// Local ordinals only locate already retained scoped references in the graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GraphNode {
    pub index: usize,
    pub parent: Option<usize>,
    pub flow: usize,
    pub kind: NodeKind,
    pub detail: Option<DetailRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftNodeRow {
    pub parent: Option<NodeRef>,
    pub first_child: Option<NodeRef>,
    pub next_sibling: Option<NodeRef>,
    pub flow: FlowRef,
    pub kind: NodeKind,
    pub detail: Option<DetailRef>,
    pub source_envelope: SourceSpan,
    pub logical: LogicalSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftFlowScope {
    Main,
    ImageAlt { object: ObjectRef },
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DraftFlowRow {
    pub scope: DraftFlowScope,
    pub root: NodeRef,
    pub domains: DraftRef<Rows<SourceSpan>>,
    pub atoms: DraftAtomBlock,
    pub logical_utf16: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftAtomContent {
    Empty,
    Pool(TextRef),
    Object(ObjectRef),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftAtomFields {
    pub owner: NodeRef,
    pub kind: AtomKind,
    pub target: TargetSpan,
    pub sources: DraftRef<Rows<SourceSpan>>,
    pub proof: Option<ProofRef>,
    pub content: DraftAtomContent,
}

/// Owned packet fields have one owner and one source block by construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftOwnedAtom {
    pub owner: NodeRef,
    pub kind: AtomKind,
    pub target: TargetSpan,
    pub sources: SourceBlockRef,
    pub proof: Option<ProofRef>,
    pub content: DraftAtomContent,
    pub primary: DraftPrimaryAuthority,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPrimaryAuthority {
    pub phase: NativePhase,
    pub kind: ClaimKind,
    pub consumed: LocalUtf8Span,
    pub host_role: HostRole,
    pub host_utf8_len: u64,
    pub host_domains: SourceBlockRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftDetailRow {
    Heading {
        level: u8,
        form: HeadingForm,
        claim: AuthorityRef,
    },
    List {
        marker: ListMarker,
        start: u64,
        tight: bool,
        claim: AuthorityRef,
    },
    Item {
        occurrence: u32,
        marker: ListMarker,
        value: u64,
        claim: AuthorityRef,
    },
    DescriptionItem {
        tight: bool,
        claim: AuthorityRef,
    },
    Code {
        form: CodeForm,
        info: Option<PayloadRef>,
        claim: AuthorityRef,
    },
    Html {
        claim: AuthorityRef,
    },
    Table {
        columns: u32,
        alignments: DraftRef<Rows<Alignment>>,
        claim: AuthorityRef,
    },
    Row {
        index: u32,
        header: bool,
        claim: AuthorityRef,
    },
    Cell {
        column: u32,
        missing: bool,
        claim: AuthorityRef,
    },
    Link {
        destination: PayloadRef,
        title: Option<PayloadRef>,
        reference: Option<ReferenceUseRef>,
        claim: AuthorityRef,
    },
    FootnoteDefinition {
        ordinal: u32,
        claim: AuthorityRef,
    },
    FootnoteReference {
        definition: NodeRef,
        ordinal: u32,
        occurrence: u32,
        claim: AuthorityRef,
        closing: Option<AuthorityRef>,
    },
    FootnoteBacklink {
        reference: NodeRef,
        ordinal: u32,
        occurrence: u32,
        claim: AuthorityRef,
    },
    TableOfContentsEntry {
        heading: NodeRef,
        depth: u8,
        claim: AuthorityRef,
    },
    Callout {
        kind: comrak::native::CalloutKind,
        disclosure: comrak::native::CalloutDisclosure,
        title: NodeRef,
        claim: AuthorityRef,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftObjectRow {
    Derived {
        node: NodeRef,
        atom: AtomRef,
        kind: DerivedKind,
        input: PayloadRef,
        export_sources: DraftRef<Rows<SourceSpan>>,
    },
    Image {
        node: NodeRef,
        atom: AtomRef,
        alt: FlowRef,
        destination: PayloadRef,
        title: Option<PayloadRef>,
        reference: Option<ReferenceUseRef>,
        wiki_size: Option<(u16, u16)>,
    },
    Task {
        node: NodeRef,
        atom: AtomRef,
        item: NodeRef,
        checked: bool,
        marker: DraftRef<Rows<SourceSpan>>,
        spacer: DraftRef<Rows<SourceSpan>>,
        action: Option<TaskAction>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftPayloadOwner {
    Node(NodeRef),
    Definition(DefinitionRef),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DraftPayloadRow {
    pub owner: DraftPayloadOwner,
    pub role: PayloadRole,
    pub value: TextRef,
    pub domains: DraftRef<Rows<SourceSpan>>,
    pub atoms: DraftAtomBlock,
    pub utf8_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftDefinitionRow {
    pub syntax: DraftRef<Rows<SourceSpan>>,
    pub label: PayloadRef,
    pub destination: PayloadRef,
    pub title: Option<PayloadRef>,
    pub key: TextRef,
    pub winner: bool,
    pub claim: AuthorityRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftReferenceUseRow {
    pub node: NodeRef,
    pub syntax: DraftRef<Rows<SourceSpan>>,
    pub winner: DefinitionRef,
    pub key: TextRef,
    pub destination: PayloadRef,
    pub title: Option<PayloadRef>,
    pub claim: AuthorityRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftHostRow {
    pub owner: NodeRef,
    pub role: HostRole,
    pub utf8_len: u64,
    pub domains: DraftRef<Rows<SourceSpan>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftAuthoritySpec {
    pub owner: NodeRef,
    pub host: DraftHostRow,
    pub phase: NativePhase,
    pub kind: ClaimKind,
    pub consumed: LocalUtf8Span,
    pub sources: DraftRef<Rows<SourceSpan>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftStageRow {
    pub kind: StageKind,
    pub input: LocalUtf8Span,
    pub output: TextRef,
    pub parameter: u32,
    pub accepted_by: AuthorityRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftProofRow {
    pub atom: AtomRef,
    pub primitives: DraftRef<Rows<PrimitiveRow>>,
    pub stages: DraftRef<Rows<StageRow>>,
    pub value: TextRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftAttributeRow {
    pub owner: NodeRef,
    pub kind: AttributeKind,
    pub claim: AuthorityRef,
    pub sources: DraftRef<Rows<SourceSpan>>,
}
