// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private direct-finalization capabilities. The input and view identities are
//! invariant generative lifetimes; neither a numeric node ID nor an AST address
//! is a public capability constructor.

use super::value::{
    self, AcceptedValueRef, NodeValueRole, ValueControl, ValuePosition, ValueStorage, ValueText,
};
use super::{DefinitionId, Host, NativeKind, NativeNodeId, NativeViolation, RSpan, WorkDelta};
use crate::nodes::{ListDelimType, ListType, Node, NodeValue};
use std::marker::PhantomData;

pub use super::mapped::{ClaimKind, ClaimOrder, HostRole, NativePhase, TransformKind};

type Invariant<'id> = PhantomData<fn(&'id ()) -> &'id ()>;

#[cfg(test)]
mod tests;

/// The one normalized input selected by the caller's private source binding.
/// This capability is deliberately neither Clone nor Copy: parsing consumes it.
#[derive(Debug)]
pub struct NativeInput<'input> {
    pub(super) text: &'input str,
    identity: Invariant<'input>,
}

/// Creates a fresh input identity, without normalizing or copying any bytes.
/// The result cannot contain the newly generated identity.
pub fn with_input<T>(text: &str, body: impl for<'input> FnOnce(NativeInput<'input>) -> T) -> T {
    body(NativeInput {
        text,
        identity: PhantomData,
    })
}

impl NativeInput<'_> {
    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// A real tree member reached through this view. Detached provenance owners
/// are deliberately not manufactured as final member tokens.
#[derive(Clone, Copy)]
pub struct NodeToken<'view, 'arena, 'input> {
    node: Node<'arena>,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

impl std::fmt::Debug for NodeToken<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeToken").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Field {
    Text,
    Code,
    Math,
    Info,
    Html,
    Destination,
    Title,
    Key,
}

#[derive(Clone, Copy, Debug)]
pub struct DefinitionToken<'view, 'input> {
    index: usize,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

#[derive(Clone, Copy, Debug)]
enum ValueOwner<'view, 'arena, 'input> {
    Node(NodeToken<'view, 'arena, 'input>),
    Definition(DefinitionToken<'view, 'input>),
}

/// The actual immutable field slot, not a string reconstructed from sourcepos.
#[derive(Clone, Copy, Debug)]
pub struct ValueToken<'view, 'arena, 'input> {
    owner: ValueOwner<'view, 'arena, 'input>,
    field: Field,
}

/// A consuming cursor over one canonical accepted field. On an error or
/// unwind no replacement cursor is returned, including at outer read_finish.
pub struct ValueCursor<'view, 'arena, 'input> {
    token: ValueToken<'view, 'arena, 'input>,
    position: ValuePosition,
    upper: usize,
}
impl ValueCursor<'_, '_, '_> {
    pub fn position(&self) -> usize {
        self.position.position
    }
}
impl std::fmt::Debug for ValueCursor<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueCursor")
            .field("position", &self.position())
            .field("upper", &self.upper)
            .finish_non_exhaustive()
    }
}

enum CurrentValue<'store> {
    Accepted(AcceptedValueRef),
    Empty,
    Key(&'store str),
}
struct ValueReads<'call, 'input, R> {
    reads: &'call mut R,
    input: Invariant<'input>,
}
impl<'input, R: NativeReads<'input>> ValueControl for ValueReads<'_, 'input, R> {
    type Error = R::Error;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Self::Error> {
        self.reads.read_work(delta)
    }
    fn reject(&mut self, violation: NativeViolation) -> Self::Error {
        self.reads.read_reject(violation)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DefinitionInfo<'view, 'arena, 'input> {
    pub id: DefinitionId,
    pub syntax: SourceToken<'view, 'input>,
    pub raw_label: SourceToken<'view, 'input>,
    pub normalized_key: ValueToken<'view, 'arena, 'input>,
    pub destination: ValueToken<'view, 'arena, 'input>,
    pub title: Option<ValueToken<'view, 'arena, 'input>>,
    pub winner: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct LineageToken<'view, 'input> {
    id: NativeNodeId,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

/// An accepted sparse extension record from this exact provenance view.
#[derive(Clone, Copy, Debug)]
pub struct G3Token<'view, 'input> {
    index: usize,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct AlignmentInfo<'view, 'input> {
    pub value: super::g3::Alignment,
    pub delimiter_cell: SourceToken<'view, 'input>,
}

#[derive(Clone, Copy, Debug)]
pub enum G3Info<'view, 'input> {
    Table {
        columns: u32,
        delimiter: SourceToken<'view, 'input>,
    },
    Row {
        index: u32,
        header: bool,
        raw: SourceToken<'view, 'input>,
    },
    Cell {
        column: u32,
        missing: bool,
        raw: SourceToken<'view, 'input>,
        content: SourceToken<'view, 'input>,
    },
    Task {
        checked: bool,
        item: LineageToken<'view, 'input>,
        paragraph: LineageToken<'view, 'input>,
        marker: SourceToken<'view, 'input>,
        spacer: SourceToken<'view, 'input>,
    },
}

#[derive(Clone, Copy, Debug)]
enum MapOwner {
    Node(usize),
    Payload(usize),
    Proof(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct MappingToken<'view, 'input> {
    owner: MapOwner,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct SourceToken<'view, 'input> {
    source: super::mapped::ClaimSource,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct ProofToken<'view, 'input> {
    index: usize,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct MappingAtom<'view, 'input> {
    pub bytes: usize,
    pub source: SourceToken<'view, 'input>,
    pub proof: Option<ProofToken<'view, 'input>>,
    pub hidden_syntax: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefixToken<'view, 'input> {
    payload: usize,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefixInfo<'view, 'input> {
    pub kind: super::GeneratedPrefix,
    pub source: SourceToken<'view, 'input>,
    pub bytes: usize,
    pub token: PrefixToken<'view, 'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct ExtendedToken<'view, 'input> {
    syntax: usize,
    view: Invariant<'view>,
    input: Invariant<'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct ExtendedInfo<'view, 'input> {
    pub kind: super::g4::ExtendedKind,
    pub syntax: SourceToken<'view, 'input>,
    pub label: SourceToken<'view, 'input>,
    pub label_lineage: LineageToken<'view, 'input>,
    pub destination: MappingToken<'view, 'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct HostDomain<'view, 'input> {
    pub source: SourceToken<'view, 'input>,
    pub utf8_len: usize,
    pub consumed_start: usize,
    pub consumed_end: usize,
    pub payload_role: Option<super::PayloadKind>,
    pub role: HostRole,
    pub phase: NativePhase,
}

/// Storage-domain coordinates only. Role, phase, owner and occurrence are
/// deliberately not inferred from geometry; the accepted caller supplies them.
#[derive(Clone, Copy, Debug)]
pub struct SourceDomain<'view, 'input> {
    pub source: SourceToken<'view, 'input>,
    pub utf8_len: usize,
    pub consumed_start: usize,
    pub consumed_end: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ProofInfo<'view, 'input> {
    pub kind: TransformKind,
    pub stage: u32,
    pub host: HostDomain<'view, 'input>,
    pub input: MappingToken<'view, 'input>,
    pub local_start: usize,
    pub local_end: usize,
    pub output_len: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ClaimInfo<'view, 'input> {
    pub owner: LineageToken<'view, 'input>,
    pub order: ClaimOrder,
    pub kind: ClaimKind,
    pub source: SourceToken<'view, 'input>,
    pub host: Option<HostDomain<'view, 'input>>,
}

#[derive(Clone, Copy, Debug)]
pub struct BindingInfo<'view, 'input> {
    pub id: NativeNodeId,
    pub birth_parent: Option<LineageToken<'view, 'input>>,
    pub start_p: Option<usize>,
    pub marker: Option<SourceToken<'view, 'input>>,
    pub value: MappingToken<'view, 'input>,
    pub syntax: Option<SourceToken<'view, 'input>>,
    pub math_syntax: Option<super::MathSyntax>,
    pub math_input: Option<SourceToken<'view, 'input>>,
    pub footnote_definition: Option<LineageToken<'view, 'input>>,
    pub label: Option<SourceToken<'view, 'input>>,
    pub wiki: Option<super::WikiForm>,
    pub definition: Option<DefinitionId>,
    pub reference_use: bool,
    pub terminal: Option<SourceToken<'view, 'input>>,
    pub terminal_synthetic: bool,
    pub buffer: Option<SourceToken<'view, 'input>>,
    pub fence_opening: Option<SourceToken<'view, 'input>>,
    pub fence_closing: Option<SourceToken<'view, 'input>>,
}

#[derive(Clone, Copy, Debug)]
pub struct InlineFootnoteInfo<'view, 'input> {
    pub reference: LineageToken<'view, 'input>,
    pub definition: LineageToken<'view, 'input>,
    pub opening: SourceToken<'view, 'input>,
    pub body: SourceToken<'view, 'input>,
    pub closing: SourceToken<'view, 'input>,
}

#[derive(Clone, Copy, Debug)]
pub struct CalloutInfo<'view, 'input> {
    pub kind: super::CalloutKind,
    pub disclosure: super::CalloutDisclosure,
    pub marker: SourceToken<'view, 'input>,
    pub title: Option<LineageToken<'view, 'input>>,
}

#[derive(Clone, Copy, Debug)]
pub struct SourcePiece {
    pub r_start: usize,
    pub r_end: usize,
    pub p: super::PSpan,
    pub kind: super::AppendKind,
}

#[derive(Debug)]
pub struct SourceCursor<'view, 'input> {
    source: SourceToken<'view, 'input>,
    index: usize,
    covered: usize,
    done: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum NodeAttributes<'view, 'arena, 'input> {
    None,
    Text(ValueToken<'view, 'arena, 'input>),
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
    },
    DescriptionItem {
        tight: bool,
    },
    Code {
        fenced: bool,
        fence: u8,
        fence_length: usize,
        info: ValueToken<'view, 'arena, 'input>,
        literal: ValueToken<'view, 'arena, 'input>,
    },
    InlineCode {
        delimiter_length: usize,
        literal: ValueToken<'view, 'arena, 'input>,
    },
    Math {
        syntax: super::MathSyntax,
        literal: ValueToken<'view, 'arena, 'input>,
    },
    Html(ValueToken<'view, 'arena, 'input>),
    Link {
        destination: ValueToken<'view, 'arena, 'input>,
        title: ValueToken<'view, 'arena, 'input>,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct NodeInfo<'view, 'arena, 'input> {
    pub id: NativeNodeId,
    pub kind: NativeKind,
    pub parent: Option<NodeToken<'view, 'arena, 'input>>,
    pub previous_sibling: Option<NodeToken<'view, 'arena, 'input>>,
    pub next_sibling: Option<NodeToken<'view, 'arena, 'input>>,
    pub first_child: Option<NodeToken<'view, 'arena, 'input>>,
    pub last_child: Option<NodeToken<'view, 'arena, 'input>>,
    pub attributes: NodeAttributes<'view, 'arena, 'input>,
}

fn charge_read<'input, R: NativeReads<'input>>(
    reads: &mut R,
    bytes: usize,
) -> Result<(), R::Error> {
    reads.read_work(WorkDelta::Events(1))?;
    reads.read_work(WorkDelta::Scan(bytes as u64))
}

/// Available only during native finalization. All reads use the same borrowed
/// host that performed grammar work. The arena lifetime is not shortened via
/// a cast: arena_tree's interior-mutability lifetime remains explicit.
pub trait DirectHost<'input>: Host {
    type ReadBrand;
    /// Latch a callback's typed error before its guard can end, and reject an
    /// otherwise successful callback if this same operation already failed.
    fn finish<T>(&mut self, result: Result<T, Self::Error>) -> Result<T, Self::Error>;
    /// An actual same-operation clock/cancellation checkpoint, not Events(0).
    fn checkpoint_now(&mut self) -> Result<(), Self::Error>;
}

/// A short read capability reborrowed from that same construction operation.
/// The adapter's private factory supplies the invariant construction witness;
/// this trait neither creates a budget nor owns a second fault latch.
pub trait NativeReads<'input> {
    type Error;
    type Brand;
    fn read_work(&mut self, delta: WorkDelta) -> Result<(), Self::Error>;
    fn read_reject(&mut self, violation: NativeViolation) -> Self::Error;
    fn read_finish<T>(&mut self, result: Result<T, Self::Error>) -> Result<T, Self::Error>;
}

/// A callback owns no independent fault state. If it unwinds, rejection is
/// latched by the original read capability while that capability is alive.
struct ReadCall<'call, 'input, R: NativeReads<'input>> {
    reads: &'call mut R,
    closed: bool,
    input: Invariant<'input>,
}

impl<'input, R: NativeReads<'input>> Drop for ReadCall<'_, 'input, R> {
    fn drop(&mut self) {
        if !self.closed {
            self.reads.read_reject(NativeViolation::InvalidProvenance);
        }
    }
}

fn read_call<'input, R: NativeReads<'input>, T>(
    reads: &mut R,
    body: impl FnOnce(&mut R) -> Result<T, R::Error>,
) -> Result<T, R::Error> {
    let mut guard = ReadCall {
        reads,
        closed: false,
        input: PhantomData,
    };
    let result = body(guard.reads);
    let result = guard.reads.read_finish(result);
    guard.closed = true;
    result
}

impl<'input, H: DirectHost<'input>> NativeReads<'input> for H {
    type Error = H::Error;
    type Brand = H::ReadBrand;
    fn read_work(&mut self, delta: WorkDelta) -> Result<(), Self::Error> {
        self.work(delta)
    }
    fn read_reject(&mut self, violation: NativeViolation) -> Self::Error {
        self.reject(violation)
    }
    fn read_finish<T>(&mut self, result: Result<T, Self::Error>) -> Result<T, Self::Error> {
        self.finish(result)
    }
}

/// The physical arena phase is separate from the invariant provenance brand.
#[derive(Clone, Copy)]
pub struct AstPhase<'arena> {
    root: Node<'arena>,
}

impl std::fmt::Debug for AstPhase<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AstPhase").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProvenancePhase;

pub type AstReader<'store, 'view, 'arena, 'input, Brand, L> =
    NativeReader<'store, 'view, 'input, Brand, L, AstPhase<'arena>>;
pub type ProvenanceReader<'store, 'view, 'input, Brand, L> =
    NativeReader<'store, 'view, 'input, Brand, L, ProvenancePhase>;
pub type AstReadAccess<'call, 'store, 'view, 'arena, 'input, H, L> =
    ReadAccess<'call, 'store, 'view, 'input, H, L, AstPhase<'arena>>;
pub type ProvenanceReadAccess<'call, 'store, 'view, 'input, H, L> =
    ReadAccess<'call, 'store, 'view, 'input, H, L, ProvenancePhase>;

/// Thin callback compatibility for capability tests; the product uses compile.
pub type FinalView<'store, 'view, 'arena, 'input, H> =
    AstReadAccess<'store, 'store, 'view, 'arena, 'input, H, <H as Host>::Lease>;

pub struct NativeReader<'store, 'view, 'input, Brand, L, Phase> {
    phase: Phase,
    input_text: &'input str,
    created_nodes: u32,
    definitions: &'store super::rows::NativeRows<super::definitions::DefinitionRecord, L>,
    mappings: &'store super::mapped::Mappings<L>,
    view: Invariant<'view>,
    input: Invariant<'input>,
    brand: PhantomData<fn(Brand) -> Brand>,
}

impl<B, L, Phase> std::fmt::Debug for NativeReader<'_, '_, '_, B, L, Phase> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeReader")
            .field("created_nodes", &self.created_nodes)
            .finish_non_exhaustive()
    }
}

impl<'store, 'view, 'input, Brand, L, Phase: Copy>
    NativeReader<'store, 'view, 'input, Brand, L, Phase>
{
    pub fn with_reads<R: NativeReads<'input, Brand = Brand>, T>(
        &self,
        reads: &mut R,
        body: impl FnOnce(
            &mut ReadAccess<'_, 'store, 'view, 'input, R, L, Phase>,
        ) -> Result<T, R::Error>,
    ) -> Result<T, R::Error> {
        read_call(reads, |reads| {
            body(&mut ReadAccess {
                phase: self.phase,
                host: reads,
                created_nodes: self.created_nodes,
                input: NativeInput {
                    text: self.input_text,
                    identity: PhantomData,
                },
                view: PhantomData,
                definitions: self.definitions,
                mappings: self.mappings,
            })
        })
    }
}

impl<'store, 'view, 'arena, 'input, Brand, L> AstReader<'store, 'view, 'arena, 'input, Brand, L> {
    /// Same provenance identity and actual data; no new grammar or membership.
    pub fn provenance_reader(&self) -> ProvenanceReader<'store, 'view, 'input, Brand, L> {
        NativeReader {
            phase: ProvenancePhase,
            input_text: self.input_text,
            created_nodes: self.created_nodes,
            definitions: self.definitions,
            mappings: self.mappings,
            view: PhantomData,
            input: PhantomData,
            brand: PhantomData,
        }
    }
}

pub struct ReadAccess<'call, 'store, 'view, 'input, H: NativeReads<'input>, L, Phase> {
    phase: Phase,
    pub(super) host: &'call mut H,
    pub(super) created_nodes: u32,
    pub(super) input: NativeInput<'input>,
    view: Invariant<'view>,
    definitions: &'store super::rows::NativeRows<super::definitions::DefinitionRecord, L>,
    mappings: &'store super::mapped::Mappings<L>,
}

/// State may carry this invocation's provenance identity, never its AST or store borrow.
pub trait DirectCompiler<'input, H: DirectHost<'input>> {
    type Output;
    type State<'view>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: AstReader<'store, 'view, 'arena, 'input, H::ReadBrand, H::Lease>,
        host: &mut H,
    ) -> Result<Self::State<'view>, H::Error>;

    fn provenance<'store, 'view>(
        &mut self,
        reader: ProvenanceReader<'store, 'view, 'input, H::ReadBrand, H::Lease>,
        state: Self::State<'view>,
        host: &mut H,
    ) -> Result<Self::Output, H::Error>;
}

struct ViewBrand<'view>(Invariant<'view>);

fn with_view<T>(body: impl for<'view> FnOnce(ViewBrand<'view>) -> T) -> T {
    body(ViewBrand(PhantomData))
}

struct NativeSession<'host, H: Host> {
    host: &'host mut H,
    closed: bool,
}
impl<H: Host> Drop for NativeSession<'_, H> {
    fn drop(&mut self) {
        if !self.closed {
            self.host.reject(NativeViolation::InternalInvariant);
        }
    }
}

/// One factory, one Work, one grammar; actual arena destruction separates phases.
pub fn compile<'input, H: DirectHost<'input>, C: DirectCompiler<'input, H>>(
    input: NativeInput<'input>,
    options: &crate::Options,
    host: &mut H,
    compiler: C,
) -> Result<C::Output, H::Error> {
    compile_with_inline_policy(
        input,
        options,
        host,
        compiler,
        super::InlinePolicy::Ordinary,
    )
}

/// Private native candidate route. Product profile admission remains the
/// caller's separate gate; upstream autolink must remain disabled.
pub fn compile_with_inline_policy<'input, H: DirectHost<'input>, C: DirectCompiler<'input, H>>(
    input: NativeInput<'input>,
    options: &crate::Options,
    host: &mut H,
    compiler: C,
    policy: super::InlinePolicy,
) -> Result<C::Output, H::Error> {
    compile_with_inline_html_policy(
        input,
        options,
        host,
        compiler,
        policy,
        super::InlineHtmlPolicy::Disabled,
    )
}

/// Direct compiler with opt-in native inline HTML semantics.
pub fn compile_with_inline_html_policy<
    'input,
    H: DirectHost<'input>,
    C: DirectCompiler<'input, H>,
>(
    input: NativeInput<'input>,
    options: &crate::Options,
    host: &mut H,
    mut compiler: C,
    policy: super::InlinePolicy,
    inline_html: super::InlineHtmlPolicy,
) -> Result<C::Output, H::Error> {
    let mut session = NativeSession {
        host,
        closed: false,
    };
    let host = &mut *session.host;
    let result = with_view(|brand| {
        compile_in_view(
            input,
            options,
            host,
            &mut compiler,
            brand,
            policy,
            inline_html,
        )
    });
    // All Context data and native permits have really dropped before this tail.
    let result = match host.finish(result) {
        Ok(output) => host.checkpoint_now().map(|()| output),
        Err(error) => Err(error),
    };
    let result = host.finish(result);
    session.closed = true;
    result
}

fn compile_in_view<'view, 'input, H: DirectHost<'input>, C: DirectCompiler<'input, H>>(
    input: NativeInput<'input>,
    options: &crate::Options,
    host: &mut H,
    compiler: &mut C,
    _brand: ViewBrand<'view>,
    policy: super::InlinePolicy,
    inline_html: super::InlineHtmlPolicy,
) -> Result<C::Output, H::Error> {
    let mut ctx = super::Context::new_direct(host, input.text);
    ctx.inline_policy = policy;
    ctx.inline_html = inline_html;
    let state: C::State<'view> =
        crate::parser::parse_in_context(input.text, options, &mut ctx, |root, ctx| {
            ctx.flush_scan()?;
            ctx.event()?;
            ctx.mappings.g3.validate(ctx.host)?;
            ctx.final_phase = super::FinalPhase::Active;
            let reader = NativeReader {
                phase: AstPhase { root },
                input_text: input.text,
                created_nodes: ctx.next_node,
                definitions: &ctx.definitions,
                mappings: &ctx.mappings,
                view: PhantomData,
                input: PhantomData,
                brand: PhantomData,
            };
            let state = read_call(&mut *ctx.host, |host| compiler.ast(reader, host))?;
            ctx.prepare_ast_retirement()?;
            Ok(state)
        })?;
    // parse_in_context returned only after AST/delimiter chunks and nested values dropped.
    ctx.retire_ast()?;
    ctx.host.checkpoint_now()?;
    let reader = NativeReader {
        phase: ProvenancePhase,
        input_text: input.text,
        created_nodes: ctx.next_node,
        definitions: &ctx.definitions,
        mappings: &ctx.mappings,
        view: PhantomData,
        input: PhantomData,
        brand: PhantomData,
    };
    let result = read_call(&mut *ctx.host, |host| {
        compiler.provenance(reader, state, host)
    });
    if result.is_err() {
        ctx.final_phase = super::FinalPhase::Failed;
    }
    let output = result?;
    ctx.flush_scan()?;
    ctx.event()?;
    ctx.final_phase = super::FinalPhase::Closed;
    Ok(output)
}

struct CallbackCompiler<F>(Option<F>);

impl<'input, H, F, T> DirectCompiler<'input, H> for CallbackCompiler<F>
where
    H: DirectHost<'input>,
    F: for<'store, 'view, 'arena> FnOnce(
        FinalView<'store, 'view, 'arena, 'input, H>,
    ) -> Result<T, H::Error>,
{
    type Output = T;
    type State<'view> = T;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: AstReader<'store, 'view, 'arena, 'input, H::ReadBrand, H::Lease>,
        host: &mut H,
    ) -> Result<T, H::Error> {
        let body = self
            .0
            .take()
            .ok_or_else(|| host.reject(NativeViolation::InternalInvariant))?;
        body(ReadAccess {
            phase: reader.phase,
            host,
            created_nodes: reader.created_nodes,
            input: NativeInput {
                text: reader.input_text,
                identity: PhantomData,
            },
            view: PhantomData,
            definitions: reader.definitions,
            mappings: reader.mappings,
        })
    }

    fn provenance<'store, 'view>(
        &mut self,
        _reader: ProvenanceReader<'store, 'view, 'input, H::ReadBrand, H::Lease>,
        state: T,
        _host: &mut H,
    ) -> Result<T, H::Error> {
        Ok(state)
    }
}

/// Capability-test adapter over the exact same two-phase factory.
/// Production Core supplies its explicit DirectCompiler instead.
#[doc(hidden)]
pub fn parse<'input, H: DirectHost<'input>, T>(
    input: NativeInput<'input>,
    options: &crate::Options,
    host: &mut H,
    body: impl for<'store, 'view, 'arena> FnOnce(
        FinalView<'store, 'view, 'arena, 'input, H>,
    ) -> Result<T, H::Error>,
) -> Result<T, H::Error> {
    compile(input, options, host, CallbackCompiler(Some(body)))
}

impl<'input, H: NativeReads<'input>, L, Phase> std::fmt::Debug
    for ReadAccess<'_, '_, '_, 'input, H, L, Phase>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadAccess")
            .field("created_nodes", &self.created_nodes)
            .finish_non_exhaustive()
    }
}

impl<'call, 'store, 'view, 'input, H: NativeReads<'input>, L, Phase: Copy>
    ReadAccess<'call, 'store, 'view, 'input, H, L, Phase>
{
    fn next_chunk<T>(
        text: &str,
        start: usize,
        upper: usize,
        maximum: usize,
        host: &mut H,
        body: impl for<'chunk> FnOnce(&'chunk str, &mut H) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        host.read_work(WorkDelta::Events(1))?;
        if !(4..=4096).contains(&maximum) || start > upper || upper > text.len() {
            return Err(host.read_reject(NativeViolation::InvalidProvenance));
        }
        let boundary = |at: usize, host: &mut H| -> Result<bool, H::Error> {
            if at < text.len() {
                host.read_work(WorkDelta::Scan(1))?;
            }
            Ok(text.is_char_boundary(at))
        };
        if !boundary(start, host)? || !boundary(upper, host)? {
            return Err(host.read_reject(NativeViolation::InvalidProvenance));
        }
        let mut end = start
            .checked_add(maximum)
            .ok_or_else(|| host.read_reject(NativeViolation::ArithmeticOverflow))?
            .min(upper);
        while !boundary(end, host)? {
            end -= 1;
        }
        host.read_work(WorkDelta::Scan((end - start) as u64))?;
        read_call(host, |host| body(&text[start..end], host))
    }

    pub fn with_proof_chunk_next<T>(
        &mut self,
        token: ProofToken<'view, 'input>,
        start: usize,
        upper: usize,
        maximum: usize,
        body: impl for<'chunk> FnOnce(&'chunk str, &mut H) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        let text = self
            .mappings
            .proofs
            .get(token.index, |bytes| charge_read(self.host, bytes))?
            .map(|p| p.result.as_str())
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        Self::next_chunk(text, start, upper, maximum, self.host, body)
    }

    pub fn with_reader<T>(
        &mut self,
        body: impl FnOnce(
            NativeReader<'store, 'view, 'input, H::Brand, L, Phase>,
            &mut H,
        ) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        let reader = NativeReader {
            phase: self.phase,
            input_text: self.input.text,
            created_nodes: self.created_nodes,
            definitions: self.definitions,
            mappings: self.mappings,
            view: PhantomData,
            input: PhantomData,
            brand: PhantomData,
        };
        read_call(self.host, |host| body(reader, host))
    }

    pub fn input_len(&self) -> usize {
        self.input.len()
    }

    pub fn created_nodes(&self) -> u32 {
        self.created_nodes
    }

    pub fn has_footnote_definitions(&self) -> bool {
        self.mappings.has_footnote_definitions
    }

    pub fn has_table_of_contents(&self) -> bool {
        self.mappings.has_table_of_contents
    }

    /// The sole host/work reborrow for direct builder operations. No Context,
    /// AST mutation handle or grammar continuation is exposed.
    pub fn with_host<T>(
        &mut self,
        body: impl FnOnce(&mut H) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        read_call(self.host, body)
    }

    fn source_r(&self, source: RSpan) -> SourceToken<'view, 'input> {
        SourceToken {
            source: super::mapped::ClaimSource::R(source),
            view: PhantomData,
            input: PhantomData,
        }
    }

    fn source_p(&self, source: super::PSpan) -> SourceToken<'view, 'input> {
        SourceToken {
            source: super::mapped::ClaimSource::P(source),
            view: PhantomData,
            input: PhantomData,
        }
    }

    fn lineage_token(&self, id: NativeNodeId) -> LineageToken<'view, 'input> {
        LineageToken {
            id,
            view: PhantomData,
            input: PhantomData,
        }
    }

    pub fn g3_info(
        &mut self,
        token: G3Token<'view, 'input>,
    ) -> Result<G3Info<'view, 'input>, H::Error> {
        let record = self
            .mappings
            .g3
            .get(token.index, |n| charge_read(self.host, n))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        use super::g3::Record;
        let info = match record {
            Record::Table {
                columns, delimiter, ..
            } => G3Info::Table {
                columns,
                delimiter: self.source_r(delimiter),
            },
            Record::Row {
                index, header, raw, ..
            } => G3Info::Row {
                index,
                header,
                raw: self.source_r(raw),
            },
            Record::Cell {
                column,
                missing,
                raw,
                content,
                ..
            } => {
                let len = self
                    .mappings
                    .buffers
                    .get(content.get() as usize - 1, |n| charge_read(self.host, n))?
                    .filter(|b| b.sealed && b.owner == record.node())
                    .map(|b| b.len)
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                G3Info::Cell {
                    column,
                    missing,
                    raw: self.source_r(raw),
                    content: self.source_r(RSpan {
                        buffer: content,
                        start: 0,
                        end: len,
                    }),
                }
            }
            Record::Task {
                checked,
                item,
                paragraph,
                marker,
                spacer,
                ..
            } => G3Info::Task {
                checked,
                item: self.lineage_token(item),
                paragraph: self.lineage_token(paragraph),
                marker: self.source_r(marker),
                spacer: self.source_r(spacer),
            },
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(info)
    }

    pub fn g3_for_lineage(
        &mut self,
        token: LineageToken<'view, 'input>,
    ) -> Result<Option<G3Token<'view, 'input>>, H::Error> {
        if token.id.get() > self.created_nodes {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        Ok(self
            .mappings
            .g3
            .find(token.id, |n| charge_read(self.host, n))?
            .map(|(index, _)| G3Token {
                index,
                view: PhantomData,
                input: PhantomData,
            }))
    }

    pub fn table_alignment(
        &mut self,
        token: G3Token<'view, 'input>,
        column: usize,
    ) -> Result<AlignmentInfo<'view, 'input>, H::Error> {
        let row = self
            .mappings
            .g3
            .alignment(token.index, column, |n| charge_read(self.host, n))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let info = AlignmentInfo {
            value: row.value,
            delimiter_cell: self.source_r(row.delimiter_cell),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(info)
    }

    /// Retrieves the real acceptance occurrence for one atomic cell pipe.
    /// Geometry only selects within an existing branded source; it grants no
    /// grammar authority when the append-to-claim association is absent.
    pub fn table_pipe_claim(
        &mut self,
        source: SourceToken<'view, 'input>,
        start: usize,
        end: usize,
    ) -> Result<ClaimInfo<'view, 'input>, H::Error> {
        let super::mapped::ClaimSource::R(source) = source.source else {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        };
        if start < source.start || end > source.end || end.checked_sub(start) != Some(1) {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        let wanted = (source.buffer.get(), start);
        let mut lo = 0;
        let mut hi = self.mappings.table_pipes.len();
        let mut found = None;
        while lo < hi {
            let middle = lo + (hi - lo) / 2;
            let row = self
                .mappings
                .table_pipes
                .get(middle, |n| charge_read(self.host, n))?
                .copied()
                .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
            charge_read(self.host, std::mem::size_of_val(&row))?;
            match (row.destination.buffer.get(), row.destination.start).cmp(&wanted) {
                std::cmp::Ordering::Less => lo = middle + 1,
                std::cmp::Ordering::Greater => hi = middle,
                std::cmp::Ordering::Equal => {
                    found = Some(row);
                    break;
                }
            }
        }
        let found = found
            .filter(|r| r.destination.end == end)
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let claim = self.claim(found.claim)?;
        if claim.kind != ClaimKind::Table
            || !claim
                .host
                .is_some_and(|h| h.role == HostRole::Block && h.phase == NativePhase::Block)
        {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        Ok(claim)
    }

    fn map_token(&self, owner: MapOwner) -> MappingToken<'view, 'input> {
        MappingToken {
            owner,
            view: PhantomData,
            input: PhantomData,
        }
    }

    fn host_domain(&self, host: super::mapped::HostSnapshot) -> HostDomain<'view, 'input> {
        let source = match host.source {
            super::mapped::ClaimSource::P(p) => self.source_p(p),
            super::mapped::ClaimSource::R(r) => self.source_r(r),
        };
        HostDomain {
            source,
            utf8_len: host.utf8_len,
            consumed_start: host.consumed_start,
            consumed_end: host.consumed_end,
            payload_role: host.payload_role,
            role: host.role,
            phase: host.phase,
        }
    }

    /// This reads accepted birth/source lineage, not final membership. A
    /// detached donor can be followed here but cannot be converted to NodeToken.
    pub fn binding(
        &mut self,
        token: LineageToken<'view, 'input>,
    ) -> Result<BindingInfo<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let index = token.id.get() as usize - 1;
        let row = self
            .mappings
            .nodes
            .get(index, |bytes| charge_read(self.host, bytes))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        // Read borrowed sparse fields directly into the existing closed Info;
        // no second owned copy of each whole sparse record is materialized.
        let empty_block = super::mapped::BlockBinding::default();
        let block = match row.block {
            Some(id) => self
                .mappings
                .blocks
                .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
                .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?,
            None => &empty_block,
        };
        let empty_syntax = super::mapped::SyntaxBinding::default();
        let syntax = match row.syntax {
            Some(id) => self
                .mappings
                .syntax
                .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
                .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?,
            None => &empty_syntax,
        };
        let math = match syntax.math {
            Some(id) => Some(
                self.mappings
                    .maths
                    .get(id.get() as usize - 1, |n| charge_read(self.host, n))?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?,
            ),
            None => None,
        };
        let terminal_data = match row.terminal {
            Some(id) => Some(
                self.mappings
                    .terminals
                    .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?,
            ),
            None => None,
        };
        let terminal = terminal_data.map(|&(buffer, at, eof)| match eof {
            Some(p) => self.source_p(p),
            None => self.source_r(RSpan {
                buffer,
                start: at,
                end: at + 1,
            }),
        });
        let buffer = if let Some(id) = block.buffer {
            let buffer = self
                .mappings
                .buffers
                .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
                .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
            Some(self.source_r(RSpan {
                buffer: id,
                start: 0,
                end: buffer.len,
            }))
        } else {
            None
        };
        let info = BindingInfo {
            id: token.id,
            birth_parent: row.birth_parent.map(|id| self.lineage_token(id)),
            start_p: block.start,
            marker: block.marker.map(|span| self.source_p(span)),
            value: self.map_token(MapOwner::Node(index)),
            syntax: syntax.syntax.map(|r| self.source_r(r)),
            math_syntax: math.map(|m| m.syntax),
            math_input: math.map(|m| self.source_r(m.literal_source)),
            footnote_definition: syntax.footnote_definition.map(|id| self.lineage_token(id)),
            label: syntax.label.map(|r| self.source_r(r)),
            wiki: syntax.wiki,
            definition: syntax.definition,
            reference_use: syntax.reference_use,
            terminal,
            terminal_synthetic: terminal_data.is_some_and(|(_, _, eof)| eof.is_some()),
            buffer,
            fence_opening: block.fence_opening.map(|p| self.source_p(p)),
            fence_closing: block.fence_closing.map(|p| self.source_p(p)),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(info)
    }

    pub fn callout(
        &mut self,
        token: LineageToken<'view, 'input>,
    ) -> Result<Option<CalloutInfo<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let row = self
            .mappings
            .nodes
            .get(token.id.get() as usize - 1, |bytes| {
                charge_read(self.host, bytes)
            })?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let Some(block) = row.block else {
            return Ok(None);
        };
        let block = self
            .mappings
            .blocks
            .get(block.get() as usize - 1, |bytes| {
                charge_read(self.host, bytes)
            })?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let Some(id) = block.callout else {
            return Ok(None);
        };
        let callout = self
            .mappings
            .callouts
            .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let info = CalloutInfo {
            kind: callout.kind,
            disclosure: callout.disclosure,
            marker: self.source_p(callout.marker),
            title: callout.title.map(|title| self.lineage_token(title)),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(Some(info))
    }

    pub fn inline_footnote(
        &mut self,
        token: LineageToken<'view, 'input>,
    ) -> Result<Option<InlineFootnoteInfo<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let row = self
            .mappings
            .nodes
            .get(token.id.get() as usize - 1, |bytes| {
                charge_read(self.host, bytes)
            })?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let Some(syntax) = row.syntax else {
            return Ok(None);
        };
        let syntax = self
            .mappings
            .syntax
            .get(syntax.get() as usize - 1, |bytes| {
                charge_read(self.host, bytes)
            })?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let Some(id) = syntax.inline_footnote else {
            return Ok(None);
        };
        let note = self
            .mappings
            .inline_footnotes
            .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let body = note.body;
        let opening = RSpan {
            start: body.start - 2,
            end: body.start,
            ..body
        };
        let closing = RSpan {
            start: body.end,
            end: body.end + 1,
            ..body
        };
        let info = InlineFootnoteInfo {
            reference: self.lineage_token(note.reference),
            definition: self.lineage_token(note.definition),
            opening: self.source_r(opening),
            body: self.source_r(body),
            closing: self.source_r(closing),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(Some(info))
    }

    fn atoms(
        &mut self,
        token: MappingToken<'view, 'input>,
    ) -> Result<Option<&'store [super::mapped::Atom]>, H::Error> {
        Ok(match token.owner {
            MapOwner::Node(i) => {
                let row = self
                    .mappings
                    .nodes
                    .get(i, |bytes| charge_read(self.host, bytes))?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                match row.value {
                    None => Some(&[]),
                    Some(id) => self
                        .mappings
                        .values
                        .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
                        .map(|v| &v.atoms[..]),
                }
            }
            MapOwner::Payload(i) => self
                .mappings
                .payloads
                .get(i, |bytes| charge_read(self.host, bytes))?
                .map(|p| &p.value.atoms[..]),
            MapOwner::Proof(i) => self
                .mappings
                .proofs
                .get(i, |bytes| charge_read(self.host, bytes))?
                .map(|p| &p.input[..]),
        })
    }

    pub fn mapping_len(&mut self, token: MappingToken<'view, 'input>) -> Result<usize, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        self.atoms(token)?
            .map(<[_]>::len)
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))
    }

    /// The payload's accepted full raw domain, including an empty domain.
    /// Current atoms cannot substitute for this range after trim or decoding.
    pub fn mapping_source(
        &mut self,
        token: MappingToken<'view, 'input>,
    ) -> Result<Option<SourceToken<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let raw = match token.owner {
            MapOwner::Payload(index) => Some(
                self.mappings
                    .payloads
                    .get(index, |bytes| charge_read(self.host, bytes))?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?
                    .raw,
            ),
            MapOwner::Node(index) => {
                if let Some(math) = self.math_binding(index)? {
                    Some(math.literal_source)
                } else {
                    let row = self
                        .mappings
                        .nodes
                        .get(index, |n| charge_read(self.host, n))?
                        .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                    if let Some(id) = row.block {
                        self.mappings
                            .blocks
                            .get(id.get() as usize - 1, |n| charge_read(self.host, n))?
                            .ok_or_else(|| {
                                self.host.read_reject(NativeViolation::InvalidProvenance)
                            })?
                            .fence_literal
                    } else {
                        None
                    }
                }
            }
            _ => None,
        };
        Ok(raw.map(|r| self.source_r(r)))
    }

    fn math_binding(
        &mut self,
        node: usize,
    ) -> Result<Option<super::mapped::MathBinding>, H::Error> {
        let row = self
            .mappings
            .nodes
            .get(node, |n| charge_read(self.host, n))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let Some(id) = row.syntax else {
            return Ok(None);
        };
        let syntax = self
            .mappings
            .syntax
            .get(id.get() as usize - 1, |n| charge_read(self.host, n))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let Some(id) = syntax.math else {
            return Ok(None);
        };
        self.mappings
            .maths
            .get(id.get() as usize - 1, |n| charge_read(self.host, n))?
            .copied()
            .map(Some)
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))
    }

    pub fn mapping_prefix(
        &mut self,
        token: MappingToken<'view, 'input>,
    ) -> Result<Option<PrefixInfo<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let MapOwner::Payload(index) = token.owner else {
            return Ok(None);
        };
        let payload = self
            .mappings
            .payloads
            .get(index, |bytes| charge_read(self.host, bytes))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        Ok(payload.prefix.as_ref().map(|(source, text)| PrefixInfo {
            kind: payload.prefix_kind,
            source: self.source_r(*source),
            bytes: text.len(),
            token: PrefixToken {
                payload: index,
                view: PhantomData,
                input: PhantomData,
            },
        }))
    }

    pub fn extended_for_lineage(
        &mut self,
        token: LineageToken<'view, 'input>,
    ) -> Result<Option<ExtendedToken<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let node = self
            .mappings
            .nodes
            .get(token.id.get() as usize - 1, |bytes| {
                charge_read(self.host, bytes)
            })?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let Some(id) = node.syntax else {
            return Ok(None);
        };
        let index = id.get() as usize - 1;
        let syntax = self
            .mappings
            .syntax
            .get(index, |bytes| charge_read(self.host, bytes))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        Ok(syntax.extended.map(|_| ExtendedToken {
            syntax: index,
            view: PhantomData,
            input: PhantomData,
        }))
    }

    pub fn extended(
        &mut self,
        token: ExtendedToken<'view, 'input>,
    ) -> Result<ExtendedInfo<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let row = self
            .mappings
            .syntax
            .get(token.syntax, |bytes| charge_read(self.host, bytes))?
            .and_then(|s| s.extended)
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let result = ExtendedInfo {
            kind: row.kind,
            syntax: self.source_r(row.raw),
            label: self.source_r(row.raw),
            label_lineage: self.lineage_token(row.label),
            destination: self.map_token(MapOwner::Payload(row.destination.get() as usize - 1)),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&result) as u64))?;
        Ok(result)
    }

    pub fn with_prefix_chunk_next<T>(
        &mut self,
        token: PrefixToken<'view, 'input>,
        start: usize,
        upper: usize,
        maximum: usize,
        body: impl for<'chunk> FnOnce(&'chunk str, &mut H) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        let text = self
            .mappings
            .payloads
            .get(token.payload, |bytes| charge_read(self.host, bytes))?
            .and_then(|p| p.prefix.as_ref())
            .map(|(_, text)| text.as_str())
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        Self::next_chunk(text, start, upper, maximum, self.host, body)
    }

    pub fn mapping_atom(
        &mut self,
        token: MappingToken<'view, 'input>,
        index: usize,
    ) -> Result<MappingAtom<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let atom = self
            .atoms(token)?
            .and_then(|a| a.get(index))
            .copied()
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let info = MappingAtom {
            bytes: atom.bytes,
            source: self.source_r(atom.source),
            hidden_syntax: atom.hidden_syntax,
            proof: atom.proof.map(|index| ProofToken {
                index,
                view: PhantomData,
                input: PhantomData,
            }),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(info)
    }

    pub fn proof(
        &mut self,
        token: ProofToken<'view, 'input>,
    ) -> Result<ProofInfo<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let row = self
            .mappings
            .proofs
            .get(token.index, |bytes| charge_read(self.host, bytes))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let info = ProofInfo {
            kind: row.kind,
            stage: row.stage,
            host: self.host_domain(row.host),
            input: self.map_token(MapOwner::Proof(token.index)),
            local_start: row.local_start,
            local_end: row.local_end,
            output_len: row.result.len(),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(info)
    }

    pub fn with_proof_chunk<T>(
        &mut self,
        token: ProofToken<'view, 'input>,
        start: usize,
        end: usize,
        body: impl for<'chunk> FnOnce(&'chunk str, &mut H) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        if end.checked_sub(start).is_none_or(|n| n > 4096) {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        let text = self
            .mappings
            .proofs
            .get(token.index, |bytes| charge_read(self.host, bytes))?
            .and_then(|p| p.result.get(start..end))
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        self.host.read_work(WorkDelta::Scan(text.len() as u64))?;
        read_call(self.host, |host| body(text, host))
    }

    pub fn claim_count(&self) -> usize {
        self.mappings.claims.len()
    }

    pub fn claim(&mut self, index: usize) -> Result<ClaimInfo<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let row = self
            .mappings
            .claims
            .get(index, |bytes| charge_read(self.host, bytes))?
            .copied()
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let source = match row.source {
            super::mapped::ClaimSource::P(p) => self.source_p(p),
            super::mapped::ClaimSource::R(r) => self.source_r(r),
        };
        let info = ClaimInfo {
            owner: self.lineage_token(row.owner),
            order: row.order,
            kind: row.kind,
            source,
            host: Some(self.host_domain(row.host())),
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(info)
    }

    pub fn source_cursor(
        &mut self,
        source: SourceToken<'view, 'input>,
    ) -> Result<SourceCursor<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let (index, covered) = match source.source {
            super::mapped::ClaimSource::P(p) => (0, p.start),
            super::mapped::ClaimSource::R(r) => {
                let buffer = self
                    .mappings
                    .buffers
                    .get(r.buffer.get() as usize - 1, |bytes| {
                        charge_read(self.host, bytes)
                    })?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                if r.start > r.end || r.end > buffer.len {
                    return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
                }
                let mut lo = 0usize;
                let mut hi = buffer.pieces.len();
                while lo < hi {
                    self.host.read_work(WorkDelta::Events(1))?;
                    let middle = lo + (hi - lo) / 2;
                    let end = buffer.pieces[middle].r.end;
                    if end < r.start || (end == r.start && r.start != r.end) {
                        lo = middle + 1;
                    } else {
                        hi = middle;
                    }
                }
                (lo, r.start)
            }
        };
        Ok(SourceCursor {
            source,
            index,
            covered,
            done: false,
        })
    }

    /// Read the real full storage domain and this source's consumed range.
    /// This does not grant any owner/role/phase/occurrence authority.
    pub fn source_domain(
        &mut self,
        source: SourceToken<'view, 'input>,
    ) -> Result<SourceDomain<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let (domain, len, start, end) = match source.source {
            super::mapped::ClaimSource::P(p) => (
                self.source_p(super::PSpan {
                    start: 0,
                    end: self.input.len(),
                }),
                self.input.len(),
                p.start,
                p.end,
            ),
            super::mapped::ClaimSource::R(r) => {
                let len = self
                    .mappings
                    .buffers
                    .get(r.buffer.get() as usize - 1, |bytes| {
                        charge_read(self.host, bytes)
                    })?
                    .map(|b| b.len)
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                (
                    self.source_r(RSpan {
                        start: 0,
                        end: len,
                        ..r
                    }),
                    len,
                    r.start,
                    r.end,
                )
            }
        };
        if start > end || end > len {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        let output = SourceDomain {
            source: domain,
            utf8_len: len,
            consumed_start: start,
            consumed_end: end,
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&output) as u64))?;
        Ok(output)
    }

    /// Compare the actual source domain and range inside this input/view.
    /// Equal P geometry does not make a different R buffer the same authority.
    pub fn same_source(
        &mut self,
        left: SourceToken<'view, 'input>,
        right: SourceToken<'view, 'input>,
    ) -> Result<bool, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        Ok(match (left.source, right.source) {
            (super::mapped::ClaimSource::P(left), super::mapped::ClaimSource::P(right)) => {
                left == right
            }
            (super::mapped::ClaimSource::R(left), super::mapped::ClaimSource::R(right)) => {
                left == right
            }
            _ => false,
        })
    }

    /// The original R buffer owner survives donation or detachment. It is
    /// lineage authority only, never permission to manufacture a final member.
    pub fn source_owner(
        &mut self,
        source: SourceToken<'view, 'input>,
    ) -> Result<Option<LineageToken<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        match source.source {
            super::mapped::ClaimSource::P(_) => Ok(None),
            super::mapped::ClaimSource::R(r) => {
                let owner = self
                    .mappings
                    .buffers
                    .get(r.buffer.get() as usize - 1, |bytes| {
                        charge_read(self.host, bytes)
                    })?
                    .map(|b| b.owner)
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                Ok(Some(self.lineage_token(owner)))
            }
        }
    }

    /// Emits the actual accepted R→P pieces in R order. N01/S composition is
    /// exclusively the adapter's job, using its same branded PreparedBinding.
    pub fn next_source(
        &mut self,
        cursor: &mut SourceCursor<'view, 'input>,
    ) -> Result<Option<SourcePiece>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        if cursor.done {
            return Ok(None);
        }
        let piece = match cursor.source.source {
            super::mapped::ClaimSource::P(p) => {
                cursor.done = true;
                SourcePiece {
                    r_start: p.start,
                    r_end: p.end,
                    p,
                    kind: super::AppendKind::Copy,
                }
            }
            super::mapped::ClaimSource::R(r) => {
                let buffer = self
                    .mappings
                    .buffers
                    .get(r.buffer.get() as usize - 1, |bytes| {
                        charge_read(self.host, bytes)
                    })?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                if r.start == r.end {
                    let p = if let Some(piece) = buffer.pieces.get(cursor.index) {
                        match piece.kind {
                            super::AppendKind::Copy => piece.p.start + r.start - piece.r.start,
                            _ if r.start == piece.r.start => piece.p.start,
                            _ if r.start == piece.r.end => piece.p.end,
                            _ => {
                                return Err(self
                                    .host
                                    .read_reject(NativeViolation::InvalidProvenance));
                            }
                        }
                    } else if r.start == 0 {
                        0
                    } else {
                        self.input.len()
                    };
                    cursor.done = true;
                    SourcePiece {
                        r_start: r.start,
                        r_end: r.end,
                        p: super::PSpan { start: p, end: p },
                        kind: super::AppendKind::Copy,
                    }
                } else {
                    let piece = buffer
                        .pieces
                        .get(cursor.index)
                        .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                    let start = r.start.max(piece.r.start);
                    let end = r.end.min(piece.r.end);
                    if start != cursor.covered || start >= end {
                        return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
                    }
                    let p = match piece.kind {
                        super::AppendKind::Copy => super::PSpan {
                            start: piece.p.start + start - piece.r.start,
                            end: piece.p.start + end - piece.r.start,
                        },
                        _ if start == piece.r.start && end == piece.r.end => piece.p,
                        _ => return Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
                    };
                    cursor.covered = end;
                    cursor.index += 1;
                    cursor.done = end == r.end;
                    SourcePiece {
                        r_start: start,
                        r_end: end,
                        p,
                        kind: piece.kind,
                    }
                }
            }
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&piece) as u64))?;
        Ok(Some(piece))
    }
}

impl<'call, 'store, 'view, 'arena, 'input, H: NativeReads<'input>, L>
    AstReadAccess<'call, 'store, 'view, 'arena, 'input, H, L>
{
    fn token(&self, node: Node<'arena>) -> NodeToken<'view, 'arena, 'input> {
        NodeToken {
            node,
            view: PhantomData,
            input: PhantomData,
        }
    }

    pub fn root(&mut self) -> Result<NodeToken<'view, 'arena, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        if self.phase.root.parent().is_some()
            || self.phase.root.previous_sibling().is_some()
            || self.phase.root.next_sibling().is_some()
            || !matches!(self.phase.root.data().value, NodeValue::Document)
        {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        Ok(self.token(self.phase.root))
    }

    pub fn node(
        &mut self,
        token: NodeToken<'view, 'arena, 'input>,
    ) -> Result<NodeInfo<'view, 'arena, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let data = token.node.data();
        let id = data
            .native_id
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        if id.get() > self.created_nodes {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        let kind = super::kind(&data.value).ok_or_else(|| {
            self.host
                .read_reject(NativeViolation::UnsupportedConfiguration)
        })?;
        let value = |field| ValueToken {
            owner: ValueOwner::Node(token),
            field,
        };
        let delimiter = |list: &crate::nodes::NodeList| match list.delimiter {
            ListDelimType::Period => b'.',
            ListDelimType::Paren => b')',
        };
        let attributes = match &data.value {
            NodeValue::Text(_) => NodeAttributes::Text(value(Field::Text)),
            NodeValue::Heading(h) => NodeAttributes::Heading {
                level: h.level,
                setext: h.setext,
            },
            NodeValue::List(l) => NodeAttributes::List {
                ordered: l.list_type == ListType::Ordered,
                start: l.start,
                delimiter: delimiter(l),
                bullet: l.bullet_char,
                tight: l.tight,
            },
            NodeValue::Item(l) => NodeAttributes::Item {
                ordered: l.list_type == ListType::Ordered,
                start: l.start,
                delimiter: delimiter(l),
                bullet: l.bullet_char,
            },
            NodeValue::DescriptionItem(item) => {
                NodeAttributes::DescriptionItem { tight: item.tight }
            }
            NodeValue::CodeBlock(c) => NodeAttributes::Code {
                fenced: c.fenced,
                fence: c.fence_char,
                fence_length: c.fence_length,
                info: value(Field::Info),
                literal: value(Field::Code),
            },
            NodeValue::Code(c) => NodeAttributes::InlineCode {
                delimiter_length: c.num_backticks,
                literal: value(Field::Code),
            },
            NodeValue::Math(math) => {
                let syntax = self
                    .math_binding(id.get() as usize - 1)?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?
                    .syntax;
                if math.dollar_math != (syntax != super::MathSyntax::CodeInline)
                    || syntax.is_display() != math.display_math
                {
                    return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
                }
                NodeAttributes::Math {
                    syntax,
                    literal: value(Field::Math),
                }
            }
            NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_) => {
                NodeAttributes::Html(value(Field::Html))
            }
            NodeValue::Link(_) | NodeValue::Image(_) | NodeValue::NoteEmbed(_) => {
                NodeAttributes::Link {
                    destination: value(Field::Destination),
                    title: value(Field::Title),
                }
            }
            _ => NodeAttributes::None,
        };
        // Core's existing full traversal must check every occurrence slot,
        // including reference uses whose IR payload reuses the definition.
        // No second AST pass or materialized current value is needed.
        match &attributes {
            NodeAttributes::Text(value) | NodeAttributes::Html(value) => {
                self.current_value(*value)?;
            }
            NodeAttributes::Code { info, literal, .. } => {
                self.current_value(*info)?;
                self.current_value(*literal)?;
            }
            NodeAttributes::InlineCode { literal, .. } | NodeAttributes::Math { literal, .. } => {
                self.current_value(*literal)?;
            }
            NodeAttributes::Link { destination, title } => {
                self.current_value(*destination)?;
                self.current_value(*title)?;
            }
            _ => {}
        }
        let output = NodeInfo {
            id,
            kind,
            parent: token.node.parent().map(|n| self.token(n)),
            previous_sibling: token.node.previous_sibling().map(|n| self.token(n)),
            next_sibling: token.node.next_sibling().map(|n| self.token(n)),
            first_child: token.node.first_child().map(|n| self.token(n)),
            last_child: token.node.last_child().map(|n| self.token(n)),
            attributes,
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&output) as u64))?;
        Ok(output)
    }

    pub fn g3(
        &mut self,
        token: NodeToken<'view, 'arena, 'input>,
    ) -> Result<Option<G3Token<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let id = token
            .node
            .data()
            .native_id
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        if id.get() > self.created_nodes {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        let found = self.mappings.g3.find(id, |n| charge_read(self.host, n))?;
        let actual = super::kind(&token.node.data().value);
        if let Some((index, record)) = found {
            let expected = match record {
                super::g3::Record::Table { .. } => NativeKind::Table,
                super::g3::Record::Row { .. } => NativeKind::TableRow,
                super::g3::Record::Cell { .. } => NativeKind::TableCell,
                super::g3::Record::Task { .. } => NativeKind::TaskCheckbox,
            };
            if actual != Some(expected) {
                return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
            }
            Ok(Some(G3Token {
                index,
                view: PhantomData,
                input: PhantomData,
            }))
        } else if matches!(
            actual,
            Some(
                NativeKind::Table
                    | NativeKind::TableRow
                    | NativeKind::TableCell
                    | NativeKind::TaskCheckbox
            )
        ) {
            Err(self.host.read_reject(NativeViolation::InvalidProvenance))
        } else {
            Ok(None)
        }
    }

    fn value_slot(value: &NodeValue, field: Field) -> Option<&ValueText> {
        match (value, field) {
            (NodeValue::Text(t), Field::Text) => Some(t),
            (NodeValue::CodeBlock(c), Field::Code) => Some(&c.literal),
            (NodeValue::CodeBlock(c), Field::Info) => Some(&c.info),
            (NodeValue::Code(c), Field::Code) => Some(&c.literal),
            (NodeValue::Math(m), Field::Math) => Some(&m.literal),
            (NodeValue::HtmlBlock(h), Field::Html) => Some(&h.literal),
            (NodeValue::HtmlInline(h), Field::Html) => Some(h),
            (
                NodeValue::Link(l) | NodeValue::Image(l) | NodeValue::NoteEmbed(l),
                Field::Destination,
            ) => Some(&l.url),
            (NodeValue::Link(l) | NodeValue::Image(l) | NodeValue::NoteEmbed(l), Field::Title) => {
                Some(&l.title)
            }
            _ => None,
        }
    }

    /// Resolve the real field, then compare its identity with the existing
    /// node/block/syntax/definition binding. Owned and Pending never finalize.
    fn current_value(
        &mut self,
        token: ValueToken<'view, 'arena, 'input>,
    ) -> Result<CurrentValue<'store>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let (slot, role, node_id) = match token.owner {
            ValueOwner::Node(node) => {
                let data = node.node.data();
                let id = data
                    .native_id
                    .filter(|id| id.get() <= self.created_nodes)
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                let slot = Self::value_slot(&data.value, token.field)
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                let slot = match slot.repr {
                    ValueStorage::Accepted(value) => Some(value),
                    ValueStorage::Empty => None,
                    _ => return Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
                };
                let role = match (&data.value, token.field) {
                    (NodeValue::Text(_), Field::Text) => Some(NodeValueRole::Text),
                    (NodeValue::CodeBlock(_), Field::Code) => Some(NodeValueRole::BlockCode),
                    (NodeValue::Code(_), Field::Code) => Some(NodeValueRole::InlineCode),
                    (NodeValue::Math(_), Field::Math) => Some(NodeValueRole::MathInput),
                    (NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_), Field::Html) => {
                        Some(NodeValueRole::Html)
                    }
                    _ => None,
                };
                (slot, role, Some(id))
            }
            ValueOwner::Definition(definition) => {
                let entry = self
                    .definitions
                    .get(definition.index, |n| charge_read(self.host, n))?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                let identity = entry
                    .value
                    .native
                    .filter(|identity| identity.definition == entry.metadata.id)
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                if token.field == Field::Key {
                    return Ok(CurrentValue::Key(&entry.key));
                }
                let (slot, expected) = match token.field {
                    Field::Destination => (
                        &entry.value.url,
                        Some(AcceptedValueRef::Payload {
                            payload: identity.destination,
                            role: super::PayloadKind::Destination,
                        }),
                    ),
                    Field::Title if identity.title.is_some() => (
                        &entry.value.title,
                        identity.title.map(|payload| AcceptedValueRef::Payload {
                            payload,
                            role: super::PayloadKind::Title,
                        }),
                    ),
                    _ => return Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
                };
                return match &slot.repr {
                    ValueStorage::Accepted(actual) if Some(*actual) == expected => {
                        Ok(CurrentValue::Accepted(*actual))
                    }
                    _ => Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
                };
            }
        };
        let mapping = self.binding_mapping(token)?;
        let expected = match mapping.map(|m| m.owner) {
            Some(MapOwner::Node(index)) => Some(AcceptedValueRef::Node {
                node: node_id
                    .filter(|id| id.get() as usize - 1 == index)
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?,
                role: role
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?,
            }),
            Some(MapOwner::Payload(index)) => Some(AcceptedValueRef::Payload {
                payload: super::PayloadId::from_raw(
                    u32::try_from(index)
                        .ok()
                        .and_then(|i| i.checked_add(1))
                        .ok_or_else(|| {
                            self.host.read_reject(NativeViolation::ArithmeticOverflow)
                        })?,
                )
                .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?,
                role: match token.field {
                    Field::Info => super::PayloadKind::FenceInfo,
                    Field::Destination => super::PayloadKind::Destination,
                    Field::Title => super::PayloadKind::Title,
                    _ => return Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
                },
            }),
            None => None,
            Some(MapOwner::Proof(_)) => {
                return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
            }
        };
        if slot != expected {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        Ok(match slot {
            Some(value) => CurrentValue::Accepted(value),
            None => CurrentValue::Empty,
        })
    }

    pub fn definition_count(&self) -> usize {
        self.definitions.len()
    }

    /// Enumeration follows actual acceptance order, including lost and unused
    /// definitions. An index is never a node or value capability constructor.
    pub fn definition(
        &mut self,
        index: usize,
    ) -> Result<DefinitionInfo<'view, 'arena, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let entry = self
            .definitions
            .get(index, |bytes| charge_read(self.host, bytes))?
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let identity = entry
            .value
            .native
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        let token = DefinitionToken {
            index,
            view: PhantomData,
            input: PhantomData,
        };
        let value = |field| ValueToken {
            owner: ValueOwner::Definition(token),
            field,
        };
        let info = DefinitionInfo {
            id: entry.metadata.id,
            syntax: self.source_r(entry.metadata.syntax),
            raw_label: self.source_r(entry.metadata.label),
            normalized_key: value(Field::Key),
            destination: value(Field::Destination),
            title: identity.title.map(|_| value(Field::Title)),
            winner: entry.winner,
        };
        self.host
            .read_work(WorkDelta::Copy(std::mem::size_of_val(&info) as u64))?;
        Ok(info)
    }

    pub fn lineage(
        &mut self,
        token: NodeToken<'view, 'arena, 'input>,
    ) -> Result<LineageToken<'view, 'input>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let id = token
            .node
            .data()
            .native_id
            .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
        Ok(self.lineage_token(id))
    }

    /// Binding is resolved from the actual field token. No numeric payload ID
    /// supplied by the caller can select another field's mapping.
    pub fn value_mapping(
        &mut self,
        token: ValueToken<'view, 'arena, 'input>,
    ) -> Result<Option<MappingToken<'view, 'input>>, H::Error> {
        self.current_value(token)?;
        self.binding_mapping(token)
    }

    fn binding_mapping(
        &mut self,
        token: ValueToken<'view, 'arena, 'input>,
    ) -> Result<Option<MappingToken<'view, 'input>>, H::Error> {
        self.host.read_work(WorkDelta::Events(1))?;
        let payload = match token.owner {
            ValueOwner::Node(node) => {
                let id = node
                    .node
                    .data()
                    .native_id
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                let index = id.get() as usize - 1;
                let row = self
                    .mappings
                    .nodes
                    .get(index, |bytes| charge_read(self.host, bytes))?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                match token.field {
                    Field::Text | Field::Code | Field::Math | Field::Html => {
                        return Ok(Some(self.map_token(MapOwner::Node(index))));
                    }
                    Field::Info => match row.block {
                        Some(id) => {
                            self.mappings
                                .blocks
                                .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
                                .ok_or_else(|| {
                                    self.host.read_reject(NativeViolation::InvalidProvenance)
                                })?
                                .info
                        }
                        None => None,
                    },
                    Field::Destination | Field::Title => match row.syntax {
                        Some(id) => {
                            let syntax = self
                                .mappings
                                .syntax
                                .get(id.get() as usize - 1, |bytes| charge_read(self.host, bytes))?
                                .ok_or_else(|| {
                                    self.host.read_reject(NativeViolation::InvalidProvenance)
                                })?;
                            if matches!(token.field, Field::Destination) {
                                syntax.destination
                            } else {
                                syntax.title
                            }
                        }
                        None => None,
                    },
                    Field::Key => {
                        return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
                    }
                }
            }
            ValueOwner::Definition(definition) => {
                let row = self
                    .definitions
                    .get(definition.index, |bytes| charge_read(self.host, bytes))?
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                let identity = row
                    .value
                    .native
                    .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                match token.field {
                    Field::Key => return Ok(None),
                    Field::Destination => Some(identity.destination),
                    Field::Title => identity.title,
                    _ => return Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
                }
            }
        };
        Ok(payload.map(|id| self.map_token(MapOwner::Payload(id.get() as usize - 1))))
    }

    pub fn value_len(
        &mut self,
        token: ValueToken<'view, 'arena, 'input>,
    ) -> Result<usize, H::Error> {
        match self.current_value(token)? {
            CurrentValue::Empty => Ok(0),
            CurrentValue::Key(key) => Ok(key.len()),
            CurrentValue::Accepted(id) => {
                let mut control = ValueReads {
                    reads: &mut *self.host,
                    input: PhantomData,
                };
                value::accepted_view(id, self.mappings, &mut control)?.len(&mut control)
            }
        }
    }

    /// Validate both exact UTF-8 endpoints against real accepted backing.
    pub fn value_cursor(
        &mut self,
        token: ValueToken<'view, 'arena, 'input>,
        start: usize,
        upper: usize,
    ) -> Result<ValueCursor<'view, 'arena, 'input>, H::Error> {
        if start > upper {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        let position = match self.current_value(token)? {
            CurrentValue::Empty if start == 0 && upper == 0 => ValuePosition::default(),
            CurrentValue::Key(key)
                if upper <= key.len()
                    && key.is_char_boundary(start)
                    && key.is_char_boundary(upper) =>
            {
                self.host.read_work(WorkDelta::Scan(2))?;
                ValuePosition::key(start)
            }
            CurrentValue::Accepted(id) => {
                let mut control = ValueReads {
                    reads: &mut *self.host,
                    input: PhantomData,
                };
                let view = value::accepted_view(id, self.mappings, &mut control)?;
                value::checked_position(
                    &view,
                    self.mappings,
                    self.input.text,
                    upper,
                    &mut control,
                )?;
                value::checked_position(&view, self.mappings, self.input.text, start, &mut control)?
            }
            _ => return Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
        };
        Ok(ValueCursor {
            token,
            position,
            upper,
        })
    }

    /// Borrow at most one real backing. A failed or unwound read consumes the
    /// old cursor and returns no progress, even when an outer finish rejects.
    pub fn with_next_value_chunk<T>(
        &mut self,
        mut cursor: ValueCursor<'view, 'arena, 'input>,
        maximum: usize,
        body: impl for<'chunk> FnOnce(&'chunk str, &mut H) -> Result<T, H::Error>,
    ) -> Result<(Option<T>, ValueCursor<'view, 'arena, 'input>), H::Error> {
        if !(4..=4096).contains(&maximum) {
            return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
        }
        let text = match self.current_value(cursor.token)? {
            CurrentValue::Accepted(id) => {
                let mut control = ValueReads {
                    reads: &mut *self.host,
                    input: PhantomData,
                };
                let view = value::accepted_view(id, self.mappings, &mut control)?;
                value::next_backing(
                    &view,
                    self.mappings,
                    self.input.text,
                    &mut cursor.position,
                    cursor.upper,
                    maximum,
                    &mut control,
                )?
            }
            CurrentValue::Key(key) => {
                if cursor.position.position > cursor.upper
                    || cursor.upper > key.len()
                    || !key.is_char_boundary(cursor.upper)
                    || !key.is_char_boundary(cursor.position.position)
                {
                    return Err(self.host.read_reject(NativeViolation::InvalidProvenance));
                }
                if cursor.position.position == cursor.upper {
                    None
                } else {
                    let start = cursor.position.position;
                    let mut end = start
                        .checked_add(maximum)
                        .map(|n| n.min(cursor.upper))
                        .ok_or_else(|| {
                            self.host.read_reject(NativeViolation::ArithmeticOverflow)
                        })?;
                    while !key.is_char_boundary(end) {
                        self.host.read_work(WorkDelta::Scan(1))?;
                        end -= 1;
                    }
                    let text = key
                        .get(start..end)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| self.host.read_reject(NativeViolation::InvalidProvenance))?;
                    self.host.read_work(WorkDelta::Scan(text.len() as u64))?;
                    cursor.position.position = end;
                    Some(text)
                }
            }
            CurrentValue::Empty if cursor.position.position == 0 && cursor.upper == 0 => None,
            _ => return Err(self.host.read_reject(NativeViolation::InvalidProvenance)),
        };
        let result = match text {
            Some(text) => read_call(self.host, |host| body(text, host)).map(Some),
            None => Ok(None),
        };
        self.host.read_finish(result.map(|output| (output, cursor)))
    }
}
