// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! The last AST consumer stores real final text and only sparse metadata facts.
//! Neither stored column nor the consuming handoff carries an arena capability.

use super::*;

fn matches_projected_kind(
    native_kind: native::NativeKind,
    projected: NodeKind,
    derived_fence: Option<graph::FenceKind>,
) -> Result<bool, CoreError> {
    if let Some(fence) = derived_fence {
        return Ok(native_kind == native::NativeKind::CodeBlock && projected == fence.node());
    }
    Ok((native_kind == native::NativeKind::HtmlInline
        && matches!(projected, NodeKind::HardBreak | NodeKind::HtmlComment))
        || projected == graph::kind(native_kind)?)
}
use crate::memory::BuildValue;

/// Every fallible capture/consume operation closes over the original work. In
/// particular a panic caught outside the operation cannot resume construction.
struct CaptureCall<'work, 'id, 'op> {
    work: &'work mut OwnedWork<'id, 'op>,
    returned: bool,
}
impl Drop for CaptureCall<'_, '_, '_> {
    fn drop(&mut self) {
        if !self.returned {
            self.work.fail(CoreError::InternalInvariant);
        }
    }
}
pub(super) fn call<'id, T>(
    work: &mut OwnedWork<'id, '_>,
    body: impl FnOnce(&mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
) -> Result<T, CoreError> {
    work.check()?;
    let mut guard = CaptureCall {
        work,
        returned: false,
    };
    let result = body(guard.work).map_err(|error| guard.work.fail(error));
    let result = match result {
        Ok(value) => guard.work.check().map(|()| value),
        Err(error) => Err(error),
    };
    guard.returned = true;
    result
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ValueOwner {
    Definition(u32),
    Node(b::NodeRef),
}
#[derive(Clone, Copy)]
struct Value<'view, 'input> {
    mapping: n::MappingToken<'view, 'input>,
    text: b::TextRef,
    len: usize,
    owner: ValueOwner,
    role: PayloadRole,
}
impl BuildValue for Value<'_, '_> {}

/// Issued only while consuming the private columns. A payload consumes this
/// value, checking the factory, actual occurrence and field before any writes.
pub(super) struct CapturedValue<'view, 'input> {
    factory: b::BuilderBinding,
    mapping: n::MappingToken<'view, 'input>,
    text: b::TextRef,
    len: usize,
    owner: b::DraftPayloadOwner,
    node: b::NodeRef,
    role: PayloadRole,
}
impl<'view, 'input> CapturedValue<'view, 'input> {
    pub(super) fn consume<'id>(
        self,
        builder: &b::DirectBuilder<'id>,
        owner: b::DraftPayloadOwner,
        node: b::NodeRef,
        role: PayloadRole,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(n::MappingToken<'view, 'input>, b::TextRef, usize), CoreError> {
        call(work, |work| {
            builder.check_binding(self.factory, work)?;
            if self.owner != owner || self.node != node || self.role != role {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            builder.graph_node(node, work)?;
            if builder.text_subslice(self.text, 0, self.len, work)? != self.text {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            Ok((self.mapping, self.text, self.len))
        })
    }
}

#[derive(Clone, Copy)]
struct Definition<'view, 'input> {
    syntax: n::SourceToken<'view, 'input>,
    label: n::SourceToken<'view, 'input>,
    key: b::TextRef,
    destination: u32,
    title: Option<u32>,
    winner: bool,
}
impl BuildValue for Definition<'_, '_> {}

#[derive(Clone, Copy)]
enum Kind {
    Heading,
    List,
    Item,
    DescriptionItem,
    Code,
    HtmlBlock,
    HtmlInline,
    HtmlCode,
    Link,
    Image,
    NoteEmbed,
    Table,
    TableRow,
    TableCell,
    Task,
    Math,
    Footnote,
    Callout,
}

#[derive(Clone, Copy)]
struct Math {
    syntax: MathSyntax,
    input: u32,
}
impl BuildValue for Math {}

#[derive(Clone, Copy)]
struct Extension<'view, 'input> {
    token: n::G3Token<'view, 'input>,
}
impl BuildValue for Extension<'_, '_> {}
#[derive(Clone, Copy)]
struct Node {
    member: u32,
    metadata: u32,
    kind: Kind,
    column: u32,
}
impl BuildValue for Node {}
#[derive(Clone, Copy)]
struct Heading {
    level: u8,
    setext: bool,
}
impl BuildValue for Heading {}
#[derive(Clone, Copy)]
struct List<'view, 'input> {
    fallback_marker: Option<n::SourceToken<'view, 'input>>,
    start: usize,
    delimiter: u8,
    bullet: u8,
    ordered: bool,
    tight: bool,
}
impl BuildValue for List<'_, '_> {}
#[derive(Clone, Copy)]
struct Item<'view, 'input> {
    previous: Option<n::LineageToken<'view, 'input>>,
    start: usize,
    delimiter: u8,
    bullet: u8,
    ordered: bool,
}
impl BuildValue for Item<'_, '_> {}
#[derive(Clone, Copy)]
struct Code {
    fence_length: usize,
    info: Option<u32>,
    fence: u8,
    fenced: bool,
    fence_input: Option<u32>,
}
impl BuildValue for Code {}
#[derive(Clone, Copy)]
struct Link {
    destination: u32,
    title: Option<u32>,
    definition: Option<u32>,
}
impl BuildValue for Link {}
// Html/Link/Image need only their sparse link-column association here. Their
// marker remains in the retained binding read by the metadata consumer.
#[derive(Clone, Copy)]
struct Inline {
    link: Option<u32>,
}
impl BuildValue for Inline {}

pub(super) struct NodeCaptureInput<'view, 'arena, 'input> {
    pub member: usize,
    pub metadata: usize,
    pub node: b::NodeRef,
    pub info: n::NodeInfo<'view, 'arena, 'input>,
    pub lineage: n::LineageToken<'view, 'input>,
    pub binding: n::BindingInfo<'view, 'input>,
    pub derived_fence: Option<graph::FenceKind>,
    pub footnote: Option<footnotes::Note>,
    pub callout: bool,
}

pub(super) struct CaptureBuilder<'view, 'input, 'id> {
    factory: b::BuilderBinding,
    definitions: BuildRows<'id, Definition<'view, 'input>>,
    nodes: BuildRows<'id, Node>,
    headings: BuildRows<'id, Heading>,
    lists: BuildRows<'id, List<'view, 'input>>,
    items: BuildRows<'id, Item<'view, 'input>>,
    codes: BuildRows<'id, Code>,
    maths: BuildRows<'id, Math>,
    footnotes: BuildRows<'id, footnotes::Note>,
    links: BuildRows<'id, Link>,
    inlines: BuildRows<'id, Inline>,
    values: BuildRows<'id, Value<'view, 'input>>,
    extensions: BuildRows<'id, Extension<'view, 'input>>,
    last_member: Option<usize>,
}

fn ordinal<'id>(value: usize, work: &mut OwnedWork<'id, '_>) -> Result<u32, CoreError> {
    u32::try_from(value).map_err(|_| work.fail(CoreError::ArithmeticOverflow))
}
fn text<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    value: n::ValueToken<'view, 'arena, 'input>,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(b::TextRef, usize), CoreError> {
    let len = native.read(work, |access| access.value_len(value))?;
    text_prefix(native, value, len, builder, work)
}
fn text_prefix<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    value: n::ValueToken<'view, 'arena, 'input>,
    len: usize,
    builder: &mut b::DirectBuilder<'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(b::TextRef, usize), CoreError> {
    let mut cursor = Some(native.read(work, |access| access.value_cursor(value, 0, len))?);
    let reference = builder.text_from_chunks(len, work, |offset, work| {
        let current = cursor
            .take()
            .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
        if current.position() != offset {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let (chunk, next) = native.read(work, |access| {
            access.with_next_value_chunk(current, 256, |text, reads| reads.text_chunk(text))
        })?;
        // Install only after the complete native read and all of its finish
        // checks succeed. An error returns no cursor that can be retried.
        cursor = Some(next);
        chunk.ok_or_else(|| work.fail(CoreError::InvalidProvenance))
    })?;
    Ok((reference, len))
}

impl<'view, 'input, 'id> CaptureBuilder<'view, 'input, 'id> {
    pub(super) fn new<'arena>(
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        call(work, |work| {
            let max = work
                .model_limits()
                .ok_or(CoreError::UnsupportedConfiguration)?
                .model_records;
            let mut capture = Self {
                factory: builder.binding(work)?,
                definitions: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                nodes: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                headings: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                lists: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                items: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                codes: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                maths: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                footnotes: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                links: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                inlines: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                values: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                extensions: BuildRows::new(max, LimitKind::ModelRecords, work)?,
                last_member: None,
            };
            let count = native.read(work, |access| Ok(access.definition_count()))?;
            for index in 0..count {
                let row = native.read(work, |access| access.definition(index))?;
                if row.id.get() as usize != index + 1 {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                let owner = ValueOwner::Definition(ordinal(index, work)?);
                let destination = capture.value(
                    native,
                    row.destination,
                    owner,
                    PayloadRole::Destination,
                    builder,
                    work,
                )?;
                let title = row
                    .title
                    .map(|value| {
                        capture.value(native, value, owner, PayloadRole::Title, builder, work)
                    })
                    .transpose()?;
                let key = text(native, row.normalized_key, builder, work)?.0;
                capture.definitions.push(
                    Definition {
                        syntax: row.syntax,
                        label: row.raw_label,
                        key,
                        destination,
                        title,
                        winner: row.winner,
                    },
                    work,
                )?;
            }
            Ok(capture)
        })
    }

    fn value<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        token: n::ValueToken<'view, 'arena, 'input>,
        owner: ValueOwner,
        role: PayloadRole,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<u32, CoreError> {
        self.value_prefix(native, token, owner, role, false, builder, work)
    }

    #[allow(clippy::too_many_arguments)]
    fn value_prefix<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        token: n::ValueToken<'view, 'arena, 'input>,
        owner: ValueOwner,
        role: PayloadRole,
        omit_synthetic_terminal: bool,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<u32, CoreError> {
        let mapping = native
            .read(work, |access| access.value_mapping(token))?
            .ok_or(CoreError::InvalidProvenance)?;
        let len = native.read(work, |access| access.value_len(token))?;
        let len = len
            .checked_sub(usize::from(omit_synthetic_terminal))
            .ok_or(CoreError::InvalidProvenance)?;
        let (text, len) = text_prefix(native, token, len, builder, work)?;
        let index = self.values.push(
            Value {
                mapping,
                text,
                len,
                owner,
                role,
            },
            work,
        )?;
        ordinal(index, work)
    }

    pub(super) fn node<'arena>(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        input: NodeCaptureInput<'view, 'arena, 'input>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        call(work, |work| {
            builder.check_binding(self.factory, work)?;
            let projected = builder.graph_node(input.node, work)?;
            if input.metadata != self.nodes.len()
                || self
                    .last_member
                    .is_some_and(|previous| previous >= input.member)
                || input.info.id != input.binding.id
                || projected.index != input.member
                || !matches_projected_kind(input.info.kind, projected.kind, input.derived_fence)?
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            // The actual lineage capability must identify this occurrence, not
            // merely another node with equal attributes or source geometry.
            if native
                .read(work, |access| access.binding(input.lineage))?
                .id
                != input.info.id
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let info = input.info;
            let binding = input.binding;
            let fallback_marker =
                if binding.marker.is_none() && info.kind == native::NativeKind::List {
                    let child = info.first_child.ok_or(CoreError::InvalidProvenance)?;
                    let lineage = native.read(work, |access| access.lineage(child))?;
                    native.read(work, |access| access.binding(lineage))?.marker
                } else {
                    None
                };
            let (kind, column) = match (info.kind, info.attributes) {
                (native::NativeKind::BlockQuote, n::NodeAttributes::None) if input.callout => {
                    (Kind::Callout, 0)
                }
                (
                    native::NativeKind::FootnoteDefinition | native::NativeKind::FootnoteReference,
                    n::NodeAttributes::None,
                ) => (
                    Kind::Footnote,
                    self.footnotes
                        .push(input.footnote.ok_or(CoreError::InvalidProvenance)?, work)?,
                ),
                (native::NativeKind::Math, n::NodeAttributes::Math { syntax, literal }) => {
                    if binding.math_syntax != Some(syntax) {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    let input_value = self.value(
                        native,
                        literal,
                        ValueOwner::Node(input.node),
                        PayloadRole::MathInput,
                        builder,
                        work,
                    )?;
                    (
                        Kind::Math,
                        self.maths.push(
                            Math {
                                syntax: graph::math_syntax(syntax),
                                input: input_value,
                            },
                            work,
                        )?,
                    )
                }
                (
                    kind @ (native::NativeKind::Table
                    | native::NativeKind::TableRow
                    | native::NativeKind::TableCell
                    | native::NativeKind::TaskCheckbox),
                    n::NodeAttributes::None,
                ) => {
                    let token = native
                        .read(work, |a| a.g3_for_lineage(input.lineage))?
                        .ok_or(CoreError::InvalidProvenance)?;
                    let kind = match kind {
                        native::NativeKind::Table => Kind::Table,
                        native::NativeKind::TableRow => Kind::TableRow,
                        native::NativeKind::TableCell => Kind::TableCell,
                        native::NativeKind::TaskCheckbox => Kind::Task,
                        _ => return Err(work.fail(CoreError::InvalidProvenance)),
                    };
                    (kind, self.extensions.push(Extension { token }, work)?)
                }
                (native::NativeKind::Heading, n::NodeAttributes::Heading { level, setext }) => (
                    Kind::Heading,
                    self.headings.push(Heading { level, setext }, work)?,
                ),
                (
                    native::NativeKind::List,
                    n::NodeAttributes::List {
                        ordered,
                        start,
                        delimiter,
                        bullet,
                        tight,
                    },
                ) => (
                    Kind::List,
                    self.lists.push(
                        List {
                            fallback_marker,
                            start,
                            delimiter,
                            bullet,
                            ordered,
                            tight,
                        },
                        work,
                    )?,
                ),
                (
                    native::NativeKind::ListItem,
                    n::NodeAttributes::Item {
                        ordered,
                        start,
                        delimiter,
                        bullet,
                    },
                ) => {
                    let previous = info
                        .previous_sibling
                        .map(|token| native.read(work, |access| access.lineage(token)))
                        .transpose()?;
                    (
                        Kind::Item,
                        self.items.push(
                            Item {
                                previous,
                                start,
                                delimiter,
                                bullet,
                                ordered,
                            },
                            work,
                        )?,
                    )
                }
                (
                    native::NativeKind::DescriptionItem,
                    n::NodeAttributes::DescriptionItem { tight },
                ) => (Kind::DescriptionItem, usize::from(tight)),
                (
                    native::NativeKind::CodeBlock,
                    n::NodeAttributes::Code {
                        fenced,
                        fence,
                        fence_length,
                        info,
                        literal,
                    },
                ) => {
                    let info = if native
                        .read(work, |access| access.value_mapping(info))?
                        .is_some()
                    {
                        Some(self.value(
                            native,
                            info,
                            ValueOwner::Node(input.node),
                            PayloadRole::FenceInfo,
                            builder,
                            work,
                        )?)
                    } else {
                        None
                    };
                    let fence_input = if input.derived_fence.is_some() {
                        Some(self.value_prefix(
                            native,
                            literal,
                            ValueOwner::Node(input.node),
                            PayloadRole::FenceInput,
                            binding.terminal_synthetic,
                            builder,
                            work,
                        )?)
                    } else {
                        None
                    };
                    (
                        Kind::Code,
                        self.codes.push(
                            Code {
                                fence_length,
                                info,
                                fence,
                                fenced,
                                fence_input,
                            },
                            work,
                        )?,
                    )
                }
                (native::NativeKind::HtmlBlock, n::NodeAttributes::Html(_)) => (
                    Kind::HtmlBlock,
                    self.inlines.push(Inline { link: None }, work)?,
                ),
                (native::NativeKind::HtmlInline, n::NodeAttributes::Html(_)) => (
                    Kind::HtmlInline,
                    self.inlines.push(Inline { link: None }, work)?,
                ),
                (
                    native::NativeKind::InlineCode,
                    n::NodeAttributes::InlineCode {
                        delimiter_length: 0,
                        ..
                    },
                ) => (
                    Kind::HtmlCode,
                    self.inlines.push(Inline { link: None }, work)?,
                ),
                (
                    native::NativeKind::Link
                    | native::NativeKind::Image
                    | native::NativeKind::NoteEmbed,
                    n::NodeAttributes::Link { destination, title },
                ) => {
                    let definition = binding.definition.map(|id| id.get() as usize - 1);
                    let (destination, title) = if let Some(index) = definition {
                        if index >= self.definitions.len() {
                            return Err(work.fail(CoreError::InvalidProvenance));
                        }
                        let row = native.read(work, |access| access.definition(index))?;
                        if row.id.get() as usize != index + 1 {
                            return Err(work.fail(CoreError::InvalidProvenance));
                        }
                        (row.destination, row.title)
                    } else {
                        (
                            destination,
                            native
                                .read(work, |access| access.value_mapping(title))?
                                .map(|_| title),
                        )
                    };
                    let owner = ValueOwner::Node(input.node);
                    let destination = self.value(
                        native,
                        destination,
                        owner,
                        PayloadRole::Destination,
                        builder,
                        work,
                    )?;
                    let title = title
                        .map(|value| {
                            self.value(native, value, owner, PayloadRole::Title, builder, work)
                        })
                        .transpose()?;
                    let definition = definition.map(|index| ordinal(index, work)).transpose()?;
                    let link = ordinal(
                        self.links.push(
                            Link {
                                destination,
                                title,
                                definition,
                            },
                            work,
                        )?,
                        work,
                    )?;
                    let kind = match info.kind {
                        native::NativeKind::Image => Kind::Image,
                        native::NativeKind::NoteEmbed => Kind::NoteEmbed,
                        _ => Kind::Link,
                    };
                    (kind, self.inlines.push(Inline { link: Some(link) }, work)?)
                }
                _ => return Err(CoreError::InvalidProvenance),
            };
            self.nodes.push(
                Node {
                    member: ordinal(input.member, work)?,
                    metadata: ordinal(input.metadata, work)?,
                    kind,
                    column: ordinal(column, work)?,
                },
                work,
            )?;
            self.last_member = Some(input.member);
            Ok(())
        })
    }

    pub(super) fn finish(
        self,
        expected_metadata: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Captured<'view, 'input, 'id>, CoreError> {
        call(work, |work| {
            work.events(1)?;
            if self.nodes.len() != expected_metadata {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            Ok(Captured {
                data: self,
                definition: 0,
                node: 0,
            })
        })
    }
}

pub(super) struct Captured<'view, 'input, 'id> {
    data: CaptureBuilder<'view, 'input, 'id>,
    definition: usize,
    node: usize,
}

pub(super) struct DefinitionRecord<'view, 'input> {
    pub reference: b::DefinitionRef,
    pub syntax: n::SourceToken<'view, 'input>,
    pub raw_label: n::SourceToken<'view, 'input>,
    pub key: b::TextRef,
    pub destination: CapturedValue<'view, 'input>,
    pub title: Option<CapturedValue<'view, 'input>>,
    pub winner: bool,
}
/// This transient projection is not stored as the maximum enum in every row.
pub(super) enum Attributes<'view, 'input> {
    Footnote(footnotes::Note),
    Callout,
    Math {
        syntax: MathSyntax,
        input: CapturedValue<'view, 'input>,
    },
    G3 {
        token: n::G3Token<'view, 'input>,
    },
    Heading {
        level: u8,
        setext: bool,
    },
    List {
        ordered: bool,
        start: usize,
        delimiter: u8,
        bullet: u8,
        tight: bool,
    },
    Item {
        ordered: bool,
        start: usize,
        delimiter: u8,
        bullet: u8,
        previous: Option<n::LineageToken<'view, 'input>>,
    },
    DescriptionItem {
        tight: bool,
    },
    Code {
        fenced: bool,
        fence: u8,
        fence_length: usize,
        info: Option<CapturedValue<'view, 'input>>,
        fence_input: Option<CapturedValue<'view, 'input>>,
    },
    Html,
    Link {
        destination: CapturedValue<'view, 'input>,
        title: Option<CapturedValue<'view, 'input>>,
        definition: Option<usize>,
    },
}
pub(super) struct NodeRecord<'view, 'input> {
    pub node: b::NodeRef,
    pub lineage: n::LineageToken<'view, 'input>,
    pub kind: native::NativeKind,
    pub list_fallback: Option<n::SourceToken<'view, 'input>>,
    pub metadata: graph::Metadata,
    pub attributes: Attributes<'view, 'input>,
}

impl<'view, 'input, 'id> Captured<'view, 'input, 'id> {
    fn issue(
        &self,
        index: u32,
        origin: ValueOwner,
        owner: b::DraftPayloadOwner,
        node: b::NodeRef,
        role: PayloadRole,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<CapturedValue<'view, 'input>, CoreError> {
        let value = self.data.values.get(index as usize, work)?;
        if value.owner != origin || value.role != role {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(CapturedValue {
            factory: self.data.factory,
            mapping: value.mapping,
            text: value.text,
            len: value.len,
            owner,
            node,
            role,
        })
    }

    pub(super) fn next_definition(
        &mut self,
        root: b::NodeRef,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<DefinitionRecord<'view, 'input>>, CoreError> {
        call(work, |work| {
            builder.check_binding(self.data.factory, work)?;
            if self.definition == self.data.definitions.len() {
                return Ok(None);
            }
            let root_row = builder.graph_node(root, work)?;
            if root_row.index != 0
                || root_row.parent.is_some()
                || root_row.flow != 0
                || root_row.kind != NodeKind::Document
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let value = self.data.definitions.get(self.definition, work)?;
            // This is the original P6 semantic order, not an AST-period slot.
            let reference = builder.reserve_definition(work)?;
            let origin = ValueOwner::Definition(ordinal(self.definition, work)?);
            let owner = b::DraftPayloadOwner::Definition(reference);
            let destination = self.issue(
                value.destination,
                origin,
                owner,
                root,
                PayloadRole::Destination,
                work,
            )?;
            let title = value
                .title
                .map(|at| self.issue(at, origin, owner, root, PayloadRole::Title, work))
                .transpose()?;
            self.definition += 1;
            Ok(Some(DefinitionRecord {
                reference,
                syntax: value.syntax,
                raw_label: value.label,
                key: value.key,
                destination,
                title,
                winner: value.winner,
            }))
        })
    }

    pub(super) fn next_node(
        &mut self,
        graph: &graph::Graph<'view, 'input, 'id>,
        builder: &b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<NodeRecord<'view, 'input>>, CoreError> {
        call(work, |work| {
            builder.check_binding(self.data.factory, work)?;
            if self.definition != self.data.definitions.len() {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            if self.node == self.data.nodes.len() {
                return Ok(None);
            }
            let row = self.data.nodes.get(self.node, work)?;
            let member = graph.nodes.get(row.member as usize, work)?;
            let projected = builder.graph_node(member.node, work)?;
            if row.metadata as usize != self.node
                || member.alt_root
                || member.metadata_index() != Some(self.node)
                || projected.index != row.member as usize
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let metadata = graph
                .binding(member, work)?
                .ok_or(CoreError::InvalidProvenance)?;
            match metadata {
                graph::Metadata::Detail(reference) => builder.check_reference(reference, work)?,
                graph::Metadata::Object(reference) => builder.check_reference(reference, work)?,
                graph::Metadata::Fence { detail, object, .. } => {
                    builder.check_reference(detail, work)?;
                    builder.check_reference(object, work)?;
                }
            }
            let owner = b::DraftPayloadOwner::Node(member.node);
            let origin = ValueOwner::Node(member.node);
            let at = row.column as usize;
            let (kind, list_fallback, attributes) = match row.kind {
                Kind::Math => {
                    let r = self.data.maths.get(at, work)?;
                    let input = self.issue(
                        r.input,
                        origin,
                        owner,
                        member.node,
                        PayloadRole::MathInput,
                        work,
                    )?;
                    (
                        native::NativeKind::Math,
                        None,
                        Attributes::Math {
                            syntax: r.syntax,
                            input,
                        },
                    )
                }
                Kind::Table | Kind::TableRow | Kind::TableCell | Kind::Task => {
                    let r = self.data.extensions.get(at, work)?;
                    let kind = match row.kind {
                        Kind::Table => native::NativeKind::Table,
                        Kind::TableRow => native::NativeKind::TableRow,
                        Kind::TableCell => native::NativeKind::TableCell,
                        Kind::Task => native::NativeKind::TaskCheckbox,
                        _ => return Err(work.fail(CoreError::InvalidProvenance)),
                    };
                    (kind, None, Attributes::G3 { token: r.token })
                }
                Kind::Heading => {
                    let r = self.data.headings.get(at, work)?;
                    (
                        native::NativeKind::Heading,
                        None,
                        Attributes::Heading {
                            level: r.level,
                            setext: r.setext,
                        },
                    )
                }
                Kind::List => {
                    let r = self.data.lists.get(at, work)?;
                    (
                        native::NativeKind::List,
                        r.fallback_marker,
                        Attributes::List {
                            ordered: r.ordered,
                            start: r.start,
                            delimiter: r.delimiter,
                            bullet: r.bullet,
                            tight: r.tight,
                        },
                    )
                }
                Kind::Item => {
                    let r = self.data.items.get(at, work)?;
                    (
                        native::NativeKind::ListItem,
                        None,
                        Attributes::Item {
                            ordered: r.ordered,
                            start: r.start,
                            delimiter: r.delimiter,
                            bullet: r.bullet,
                            previous: r.previous,
                        },
                    )
                }
                Kind::DescriptionItem => (
                    native::NativeKind::DescriptionItem,
                    None,
                    Attributes::DescriptionItem { tight: at == 1 },
                ),
                Kind::Code => {
                    let r = self.data.codes.get(at, work)?;
                    let info = r
                        .info
                        .map(|at| {
                            self.issue(at, origin, owner, member.node, PayloadRole::FenceInfo, work)
                        })
                        .transpose()?;
                    (
                        native::NativeKind::CodeBlock,
                        None,
                        Attributes::Code {
                            fenced: r.fenced,
                            fence: r.fence,
                            fence_length: r.fence_length,
                            info,
                            fence_input: r
                                .fence_input
                                .map(|at| {
                                    self.issue(
                                        at,
                                        origin,
                                        owner,
                                        member.node,
                                        PayloadRole::FenceInput,
                                        work,
                                    )
                                })
                                .transpose()?,
                        },
                    )
                }
                Kind::Footnote => {
                    let note = self.data.footnotes.get(at, work)?;
                    let kind = match note {
                        footnotes::Note::Definition { .. } => {
                            native::NativeKind::FootnoteDefinition
                        }
                        footnotes::Note::Reference { .. } => native::NativeKind::FootnoteReference,
                    };
                    (kind, None, Attributes::Footnote(note))
                }
                Kind::Callout => (native::NativeKind::BlockQuote, None, Attributes::Callout),
                Kind::HtmlBlock | Kind::HtmlInline | Kind::HtmlCode => {
                    let r = self.data.inlines.get(at, work)?;
                    if r.link.is_some() {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    (
                        match row.kind {
                            Kind::HtmlBlock => native::NativeKind::HtmlBlock,
                            Kind::HtmlInline => native::NativeKind::HtmlInline,
                            _ => native::NativeKind::InlineCode,
                        },
                        None,
                        Attributes::Html,
                    )
                }
                Kind::Link | Kind::Image | Kind::NoteEmbed => {
                    let r = self.data.inlines.get(at, work)?;
                    let rlink = self
                        .data
                        .links
                        .get(r.link.ok_or(CoreError::InvalidProvenance)? as usize, work)?;
                    let destination = self.issue(
                        rlink.destination,
                        origin,
                        owner,
                        member.node,
                        PayloadRole::Destination,
                        work,
                    )?;
                    let title = rlink
                        .title
                        .map(|at| {
                            self.issue(at, origin, owner, member.node, PayloadRole::Title, work)
                        })
                        .transpose()?;
                    (
                        match row.kind {
                            Kind::Image => native::NativeKind::Image,
                            Kind::NoteEmbed => native::NativeKind::NoteEmbed,
                            _ => native::NativeKind::Link,
                        },
                        None,
                        Attributes::Link {
                            destination,
                            title,
                            definition: rlink.definition.map(|i| i as usize),
                        },
                    )
                }
            };
            let derived_fence = metadata.fence();
            if !matches_projected_kind(kind, projected.kind, derived_fence)?
                || (derived_fence.is_some()
                    || matches!(
                        kind,
                        native::NativeKind::Image
                            | native::NativeKind::TaskCheckbox
                            | native::NativeKind::Math
                            | native::NativeKind::NoteEmbed
                    ))
                    != metadata.object().is_some()
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            self.node += 1;
            Ok(Some(NodeRecord {
                node: member.node,
                lineage: member.lineage,
                kind,
                list_fallback,
                metadata,
                attributes,
            }))
        })
    }

    pub(super) fn finish(self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        call(work, |work| {
            work.events(1)?;
            if self.definition != self.data.definitions.len() || self.node != self.data.nodes.len()
            {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "direct_capture_tests.rs"]
mod tests;
