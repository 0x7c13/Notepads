// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private synchronous acceptance interface for the Notepads Markdown parser adapter.
//! Coordinates describe executed parser operations, never diagnostic sourcepos.

use std::num::NonZeroU32;

mod allocation;
pub(crate) mod callout;
pub use callout::{CalloutDisclosure, CalloutKind, callout_marker};
mod definitions;
pub mod direct;
pub mod g3;
pub mod g4;
mod mapped;
mod mapping_access;
use mapping_access::MappingAccess;
mod rows;
mod small_value;
pub(crate) mod value;
#[cfg(test)]
mod value_tests;
use rows::NativeRows;

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name(NonZeroU32);
        impl $name {
            pub fn get(self) -> u32 {
                self.0.get()
            }
            pub(crate) fn from_raw(value: u32) -> Option<Self> {
                NonZeroU32::new(value).map(Self)
            }
        }
    };
}
identity!(NativeNodeId);
identity!(BufferId);
identity!(PayloadId);
identity!(DefinitionId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PSpan {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RSpan {
    pub buffer: BufferId,
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DSpan {
    pub payload: PayloadId,
    pub stage: u32,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkDelta {
    Scan(u64),
    Copy(u64),
    Events(u64),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeAdmission {
    Node(u64),
    Record(u64),
    Depth(u64),
    DecodedBytes(u64),
    ReferenceLookup(u64),
    ReferenceBytes(u64),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeViolation {
    InternalInvariant,
    UnsupportedConfiguration,
    InvalidProvenance,
    ArithmeticOverflow,
    OutOfMemory,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeKind {
    Document,
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
    Text,
    SoftBreak,
    HardBreak,
    InlineCode,
    Math,
    Emphasis,
    Strong,
    Highlight,
    Superscript,
    Subscript,
    Link,
    Image,
    NoteEmbed,
    HtmlInline,
    Escaped,
    Table,
    TableRow,
    TableCell,
    Strike,
    TaskCheckbox,
    FootnoteDefinition,
    FootnoteReference,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathSyntax {
    DollarInline,
    DollarDisplay,
    LatexInline,
    LatexDisplay,
    CodeInline,
}
impl MathSyntax {
    pub fn is_display(self) -> bool {
        matches!(self, Self::DollarDisplay | Self::LatexDisplay)
    }
    pub(crate) fn delimiter_bytes(self) -> usize {
        match self {
            Self::DollarInline => 1,
            Self::DollarDisplay | Self::LatexInline | Self::LatexDisplay | Self::CodeInline => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxRole {
    LeadingBom,
    BlankLine,
    BlockWhitespace,
    QuotePrefix,
    ListPrefix,
    DescriptionMarker,
    DescriptionIndent,
    CodeIndent,
    HeadingPrefix,
    HeadingSuffix,
    SetextUnderline,
    ThematicBreak,
    TableOfContents,
    FenceOpening,
    FenceClosing,
    BlockTerminator,
    ParagraphTerminator,
    ReferenceDefinition,
    CodeInfo,
    InlineTrim,
    HeadingIdentifier,
    FootnoteMarker,
    FootnoteIndent,
    FootnoteContent,
    CalloutMarker,
    TableTrim,
    TableSeparator,
    TableDelimiter,
    TableExtraCell,
    TaskWhitespace,
}

/// The source syntax of a single explicit Markdown heading ID.
pub fn heading_identifier(attribute: &str) -> Option<&str> {
    let name = attribute.strip_prefix("{#")?.strip_suffix('}')?;
    (!name.is_empty()
        && name.chars().all(|ch| {
            !ch.is_whitespace() && !ch.is_control() && !matches!(ch, '{' | '}' | '\\' | '`')
        }))
    .then_some(name)
}

pub fn footnote_label_break(byte: u8) -> bool {
    matches!(byte, b'[' | b']' | b'^') || byte.is_ascii_whitespace() || byte.is_ascii_control()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppendKind {
    Copy,
    PartialTab { spaces: u8, column: usize },
    CodeTerminal,
    TableEscapedPipe,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeKind {
    Entity,
    EmojiShortcode,
    Escape,
    CodeLineEndingToSpace,
    CodeTrim,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreakKind {
    Soft,
    Spaces,
    Backslash,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadKind {
    Destination,
    Title,
    FenceInfo,
    AutolinkLabel,
    ReferenceLabel,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedPrefix {
    Http,
    Mailto,
}
impl GeneratedPrefix {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Http => "http://",
            Self::Mailto => "mailto:",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageKind {
    Trim,
    ReferenceTitleIndent,
    Entity,
    Escape,
    SyntheticMailto,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceIdentity {
    pub(crate) definition: DefinitionId,
    pub(crate) destination: PayloadId,
    pub(crate) title: Option<PayloadId>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct LinkIdentity {
    pub(crate) syntax: RSpan,
    pub(crate) label: RSpan,
    pub(crate) destination: Option<PayloadId>,
    pub(crate) title: Option<PayloadId>,
    pub(crate) definition: Option<DefinitionId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg(test)]
pub enum Attributes<'a> {
    None,
    Text(&'a str),
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
    Code {
        fenced: bool,
        fence: u8,
        fence_length: usize,
        info: &'a str,
        literal: &'a str,
    },
    InlineCode {
        delimiter_length: usize,
        literal: &'a str,
    },
    Html(&'a str),
    Link {
        destination: &'a str,
        title: &'a str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum FinalPlacement {
    Root,
    Child { parent: NativeNodeId, ordinal: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiForm {
    Link,
    Image { width: u16, height: u16 },
    NoteEmbed,
}

pub fn wiki_image_dimensions(size: &str) -> Option<(u16, u16)> {
    fn dimension(text: &str) -> Option<u16> {
        if !(1..=4).contains(&text.len()) || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse::<u16>()
            .ok()
            .filter(|value| (1..=8192).contains(value))
    }
    if let Some((width, height)) = size.split_once(['x', 'X']) {
        Some((dimension(width)?, dimension(height)?))
    } else {
        Some((dimension(size)?, 0))
    }
}

pub fn wiki_image_target(target: &str) -> bool {
    !target.starts_with(['/', '\\'])
        && !target.contains([':', '*', '?', '"', '<', '>', '|', '#', '\\'])
        && target.rsplit_once('.').is_some_and(|(name, ext)| {
            !name.is_empty()
                && [
                    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "tif", "tiff",
                ]
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

pub fn wiki_note_target(target: &str) -> bool {
    let (path, heading) = target.split_once('#').unwrap_or((target, ""));
    (!path.trim().is_empty() || !heading.trim().is_empty())
        && !path.starts_with(['/', '\\'])
        && !path.contains([':', '*', '?', '"', '<', '>', '|', '\\'])
}

/// All slices expire when Host::fact returns. Addresses never enter this interface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AcceptedFact<'a> {
    G3(g3::Fact),
    Extended(g4::ExtendedAccepted),
    NodeCreated {
        node: NativeNodeId,
        parent: Option<NativeNodeId>,
        kind: NativeKind,
    },
    Retyped {
        node: NativeNodeId,
        kind: NativeKind,
    },
    #[cfg(test)]
    FinalBegin {
        root: NativeNodeId,
        created_nodes: u32,
    },
    #[cfg(test)]
    FinalEnter {
        node: NativeNodeId,
        kind: NativeKind,
        attributes: Attributes<'a>,
        placement: FinalPlacement,
    },
    #[cfg(test)]
    FinalExit {
        node: NativeNodeId,
        child_count: u32,
    },
    #[cfg(test)]
    FinalSeal {
        root: NativeNodeId,
        member_nodes: u32,
        edges: u32,
    },
    BlockStart {
        node: NativeNodeId,
        at: usize,
    },
    BlockConsume {
        node: NativeNodeId,
        span: PSpan,
    },
    Syntax {
        node: NativeNodeId,
        role: SyntaxRole,
        span: PSpan,
    },
    BufferCreated {
        node: NativeNodeId,
        buffer: BufferId,
    },
    Append {
        span: RSpan,
        source: PSpan,
        kind: AppendKind,
    },
    Cut {
        span: RSpan,
        reason: SyntaxRole,
    },
    Seal {
        buffer: BufferId,
        len: usize,
    },
    TextAccepted {
        node: NativeNodeId,
        span: RSpan,
    },
    DecodeAtom {
        node: NativeNodeId,
        source: RSpan,
        kind: DecodeKind,
        value: &'a str,
    },
    TextJoin {
        survivor: NativeNodeId,
        donor: NativeNodeId,
        survivor_bytes_before: usize,
    },
    Break {
        node: NativeNodeId,
        kind: BreakKind,
        marker: Option<RSpan>,
        ending: RSpan,
        prefix: RSpan,
    },
    Emphasis {
        node: NativeNodeId,
        opener: NativeNodeId,
        closer: NativeNodeId,
        used: usize,
    },
    Code {
        node: NativeNodeId,
        syntax: RSpan,
        delimiter_bytes: usize,
    },
    Math {
        node: NativeNodeId,
        syntax: RSpan,
        literal_source: RSpan,
        delimiter_form: MathSyntax,
    },
    FootnoteReference {
        node: NativeNodeId,
        definition: NativeNodeId,
        syntax: RSpan,
    },
    InlineFootnote {
        node: NativeNodeId,
        definition: Option<NativeNodeId>,
        body: RSpan,
    },
    Callout {
        node: NativeNodeId,
        kind: CalloutKind,
        disclosure: CalloutDisclosure,
        marker: PSpan,
        title: Option<NativeNodeId>,
    },
    HtmlCode {
        node: NativeNodeId,
        syntax: RSpan,
        body: RSpan,
    },
    RawHtml {
        node: NativeNodeId,
        span: RSpan,
        block: bool,
    },
    BlockPayload {
        node: NativeNodeId,
        payload: PayloadId,
    },
    CodeTerminal {
        node: NativeNodeId,
        buffer: BufferId,
        at: usize,
        eof: Option<PSpan>,
    },
    PayloadBegin {
        payload: PayloadId,
        role: PayloadKind,
        raw: RSpan,
    },
    Stage {
        payload: PayloadId,
        stage: u32,
        kind: StageKind,
        input: DSpan,
        output_start: usize,
        output_end: usize,
        value: &'a str,
        hidden_runs: &'a [DSpan],
    },
    PayloadPrefix {
        payload: PayloadId,
        kind: GeneratedPrefix,
        at: RSpan,
        value: &'a str,
    },
    PayloadEnd {
        payload: PayloadId,
        value: &'a str,
    },
    Definition {
        definition: DefinitionId,
        buffer: BufferId,
        syntax: RSpan,
        label: RSpan,
        normalized_label: &'a str,
        destination: PayloadId,
        title: Option<PayloadId>,
    },
    Winner {
        definition: DefinitionId,
        accepted: bool,
    },
    ReferenceUse {
        node: NativeNodeId,
        definition: DefinitionId,
        syntax: RSpan,
    },
    Link {
        node: NativeNodeId,
        syntax: RSpan,
        label: RSpan,
        image: bool,
        wiki: Option<WikiForm>,
        destination: Option<PayloadId>,
        title: Option<PayloadId>,
        definition: Option<DefinitionId>,
    },
}

pub trait Host {
    type Error;
    type Lease;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Self::Error>;
    fn admit(&mut self, request: NativeAdmission) -> Result<(), Self::Error>;
    fn reserve(&mut self, bytes: u64) -> Result<Self::Lease, Self::Error>;
    fn admit_actual(&mut self, lease: &mut Self::Lease, bytes: u64) -> Result<(), Self::Error>;
    fn fact(&mut self, fact: AcceptedFact<'_>) -> Result<(), Self::Error>;
    /// Bounds come from the candidate's actual first and last copied source bytes.
    /// Hosts may keep generated literal regions from acquiring email links.
    fn email_autolink_allowed(&mut self, _span: PSpan) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn reject(&mut self, violation: NativeViolation) -> Self::Error;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FinalPhase {
    Grammar,
    Active,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlinePolicy {
    Ordinary,
    StrictG4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlineHtmlPolicy {
    Disabled,
    RawText,
    Tagfilter,
}

impl InlineHtmlPolicy {
    pub(crate) fn enabled(self) -> bool {
        self != Self::Disabled
    }
}

pub(crate) const RAW_TEXT_TAGS: [&[u8]; 9] = [
    b"script",
    b"style",
    b"textarea",
    b"title",
    b"xmp",
    b"iframe",
    b"noembed",
    b"noframes",
    b"plaintext",
];

/// Match the lexical prefix used by GFM's raw HTML output filter.
pub fn gfm_tagfilter_matches(raw: &[u8]) -> bool {
    let Some(raw) = raw.strip_prefix(b"<") else {
        return false;
    };
    let name_start = raw.strip_prefix(b"/").unwrap_or(raw);
    RAW_TEXT_TAGS.iter().any(|name| {
        name_start
            .get(..name.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(name))
            && matches!(
                &name_start[name.len()..],
                [b'>' | b' ' | b'\t' | b'\n' | b'\r', ..] | [b'/', b'>', ..]
            )
    })
}

struct DirectMutation<'phase> {
    phase: &'phase mut FinalPhase,
    committed: bool,
}
impl Drop for DirectMutation<'_> {
    fn drop(&mut self) {
        if !self.committed {
            *self.phase = FinalPhase::Failed;
        }
    }
}

pub(crate) struct Context<'h, H: Host> {
    pub(crate) host: &'h mut H,
    next_node: u32,
    document: Option<NativeNodeId>,
    final_phase: FinalPhase,
    next_buffer: u32,
    next_payload: u32,
    next_definition: u32,
    pub(crate) line_start: usize,
    pub(crate) input_len: usize,
    input_text: Option<&'h str>,
    pub(crate) capture: bool,
    pending_scan: usize,
    // These actual values precede the allocation ledger in drop order.
    pub(crate) direct: bool,
    pub(crate) inline_policy: InlinePolicy,
    pub(crate) inline_html: InlineHtmlPolicy,
    shadow: Option<g4::shadow::ShadowState<'h, H::Lease>>,
    mappings: mapped::Mappings<H::Lease>,
    definitions: NativeRows<definitions::DefinitionRecord, H::Lease>,
    pending_definition: Option<definitions::PendingDefinition>,
    registry: allocation::Registry<H::Lease>,
}

impl<'h, H: Host> Context<'h, H> {
    fn mapping_access(&mut self) -> MappingAccess<'_, H> {
        MappingAccess {
            host: self.host,
            registry: &mut self.registry,
            input_len: self.input_len,
            inline_policy: self.inline_policy,
        }
    }

    pub(crate) fn new(host: &'h mut H, input_len: usize) -> Self {
        Self {
            host,
            next_node: 0,
            document: None,
            final_phase: FinalPhase::Grammar,
            next_buffer: 0,
            next_payload: 0,
            next_definition: 0,
            line_start: 0,
            input_len,
            input_text: None,
            capture: true,
            pending_scan: 0,
            direct: false,
            inline_policy: InlinePolicy::Ordinary,
            inline_html: InlineHtmlPolicy::Disabled,
            shadow: None,
            mappings: mapped::Mappings::default(),
            definitions: NativeRows::default(),
            pending_definition: None,
            registry: allocation::Registry::default(),
        }
    }
    pub(crate) fn new_direct(host: &'h mut H, text: &'h str) -> Self {
        let mut context = Self::new(host, text.len());
        context.direct = true;
        context.input_text = Some(text);
        context
    }
    pub(crate) fn shadow_child(&mut self) -> Context<'_, H> {
        let mut child = Context::new(&mut *self.host, self.input_len);
        child.inline_html = self.inline_html;
        let definitions = self
            .shadow
            .as_ref()
            .map_or(&self.definitions, g4::shadow::ShadowState::definitions);
        child.shadow = Some(g4::shadow::ShadowState::new(definitions));
        child
    }
    pub(crate) fn is_shadow(&self) -> bool {
        self.shadow.is_some()
    }
    pub(crate) fn is_boundary_probe(&self) -> bool {
        self.is_shadow() && !self.capture
    }
    pub(crate) fn shadow_delimited(
        &mut self,
        node: NativeNodeId,
        span: RSpan,
        marker: u8,
        width: u8,
    ) -> Result<(), H::Error> {
        self.event()?;
        self.shadow
            .as_mut()
            .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?
            .delimited(node, span, marker, width, self.host)
    }
    pub(crate) fn admit_decoded(&mut self, bytes: u64) -> Result<(), H::Error> {
        // A boundary probe allocates only scratch text. Copy, scan and live
        // capacity remain funded, with no second semantic-output admission.
        if self.is_boundary_probe() {
            Ok(())
        } else {
            self.host.admit(NativeAdmission::DecodedBytes(bytes))
        }
    }
    pub(crate) fn shadow_winner(
        &mut self,
        id: DefinitionId,
    ) -> Result<ReferenceIdentity, H::Error> {
        self.shadow
            .as_ref()
            .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?
            .winner(id, self.host)
    }
    pub(crate) fn event(&mut self) -> Result<(), H::Error> {
        self.host.work(WorkDelta::Events(1))
    }
    pub(crate) fn scan_step(&mut self) -> Result<(), H::Error> {
        self.pending_scan += 1;
        if self.pending_scan == 256 {
            self.flush_scan()?;
        }
        Ok(())
    }
    pub(crate) fn flush_scan(&mut self) -> Result<(), H::Error> {
        let pending = std::mem::take(&mut self.pending_scan);
        if pending != 0 {
            self.host.work(WorkDelta::Scan(pending as u64))?;
        }
        Ok(())
    }
    pub(crate) fn find<F>(&mut self, bytes: &[u8], mut find: F) -> Result<Option<usize>, H::Error>
    where
        F: FnMut(&[u8]) -> Option<usize>,
    {
        let mut offset = 0;
        for part in bytes.chunks(65_536) {
            self.event()?;
            if let Some(index) = find(part) {
                self.scan(index + 1)?;
                return Ok(Some(offset + index));
            }
            self.scan(part.len())?;
            offset += part.len();
        }
        Ok(None)
    }
    pub(crate) fn scan(&mut self, mut bytes: usize) -> Result<(), H::Error> {
        while bytes != 0 {
            let count = bytes.min(65_536);
            self.host.work(WorkDelta::Scan(count as u64))?;
            bytes -= count;
        }
        Ok(())
    }
    pub(crate) fn fact(&mut self, fact: AcceptedFact<'_>) -> Result<(), H::Error> {
        let result = (|| {
            self.require_grammar()?;
            #[cfg(test)]
            if matches!(
                fact,
                AcceptedFact::FinalBegin { .. }
                    | AcceptedFact::FinalEnter { .. }
                    | AcceptedFact::FinalExit { .. }
                    | AcceptedFact::FinalSeal { .. }
            ) {
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            if !self.capture {
                return Ok(());
            }
            self.prepare_fact(fact)?;
            if let Some(shadow) = &mut self.shadow {
                return shadow.fact(fact, self.host);
            }
            if self.direct
                && let AcceptedFact::Definition {
                    definition,
                    syntax,
                    label,
                    ..
                } = fact
            {
                if self.pending_definition.is_some() {
                    return Err(self.host.reject(NativeViolation::InvalidProvenance));
                }
                self.pending_definition = Some(definitions::PendingDefinition {
                    id: definition,
                    syntax,
                    label,
                });
            }
            if let AcceptedFact::BlockConsume { node, .. } = fact
                && self.document != Some(node)
            {
                // Continuation still validates, flushes and observes the host. Only
                // Document consumption is retained; Syntax/R/final placement locate
                // other nodes without authorizing their lookahead envelopes.
                return Ok(());
            }
            self.host.admit(NativeAdmission::Record(1))?;
            if self.direct {
                self.accept_direct(fact)
            } else {
                #[cfg(test)]
                {
                    self.host.fact(fact)
                }
                #[cfg(not(test))]
                {
                    Err(self.host.reject(NativeViolation::InvalidProvenance))
                }
            }
        })();
        if result.is_err() {
            self.final_phase = FinalPhase::Failed;
        }
        result
    }
    fn mutate_mappings<T>(
        &mut self,
        body: impl FnOnce(
            &mut mapped::Mappings<H::Lease>,
            &mut MappingAccess<'_, H>,
        ) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        // The actual partial owner stays in Context through Err or unwind.
        // The guard closes the phase without replacing the Host's first fault.
        let mut mutation = DirectMutation {
            phase: &mut self.final_phase,
            committed: false,
        };
        let mut access = MappingAccess {
            host: self.host,
            registry: &mut self.registry,
            input_len: self.input_len,
            inline_policy: self.inline_policy,
        };
        let result = body(&mut self.mappings, &mut access);
        mutation.committed = result.is_ok();
        result
    }
    fn accept_direct(&mut self, fact: AcceptedFact<'_>) -> Result<(), H::Error> {
        self.mutate_mappings(|mappings, access| mappings.accept(fact, access))
    }
    pub(crate) fn fail_grammar(&mut self) {
        self.final_phase = FinalPhase::Failed;
    }
    fn require_grammar(&mut self) -> Result<(), H::Error> {
        if self.final_phase != FinalPhase::Grammar {
            // A latched Host fault takes precedence over an illegal restart. This
            // does not clone or store Host::Error, nor reset the operation's flag.
            let error = match self.host.work(WorkDelta::Events(0)) {
                Err(error) => error,
                Ok(()) => self.host.reject(NativeViolation::InvalidProvenance),
            };
            self.final_phase = FinalPhase::Failed;
            return Err(error);
        }
        Ok(())
    }
    fn prepare_fact(&mut self, fact: AcceptedFact<'_>) -> Result<(), H::Error> {
        if let AcceptedFact::CodeTerminal { eof: Some(eof), .. } = fact
            && (eof.start != eof.end || eof.end != self.input_len)
        {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        let source = match fact {
            AcceptedFact::Append { source, .. } => Some(source),
            AcceptedFact::BlockConsume { span, .. } | AcceptedFact::Syntax { span, .. } => {
                Some(span)
            }
            AcceptedFact::BlockStart { at, .. } => Some(PSpan { start: at, end: at }),
            AcceptedFact::Callout { marker, .. } => Some(marker),
            AcceptedFact::CodeTerminal { eof, .. } => eof,
            _ => None,
        };
        if source.is_some_and(|source| source.start > source.end || source.end > self.input_len) {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        if let AcceptedFact::BlockConsume { node, .. } = fact
            && (self.document.is_none() || node.get() > self.next_node)
        {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        self.flush_scan()?;
        self.event()
    }
    pub(crate) fn node(
        &mut self,
        parent: Option<NativeNodeId>,
        kind: NativeKind,
    ) -> Result<NativeNodeId, H::Error> {
        let result = self.create_node(parent, kind);
        if result.is_err() {
            self.final_phase = FinalPhase::Failed;
        }
        result
    }
    fn create_node(
        &mut self,
        parent: Option<NativeNodeId>,
        kind: NativeKind,
    ) -> Result<NativeNodeId, H::Error> {
        self.require_grammar()?;
        self.event()?;
        if self.capture
            && kind == NativeKind::Document
            && (parent.is_some() || self.document.is_some())
        {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        self.host.admit(NativeAdmission::Node(1))?;
        self.next_node = self
            .next_node
            .checked_add(1)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        let id = NativeNodeId::from_raw(self.next_node).expect("checked nonzero native ID");
        if let Some(shadow) = &mut self.shadow {
            shadow.node(id, self.host)?;
            if kind == NativeKind::Document {
                self.document = Some(id);
            }
            return Ok(id);
        }
        if self.capture {
            let fact = AcceptedFact::NodeCreated {
                node: id,
                parent,
                kind,
            };
            // This sole creation factory already admitted the actual entity before its ID.
            // Keep the separate notification event and full callback, without admitting
            // the same creation again as a Record. Ordinary fact() still admits Record.
            self.prepare_fact(fact)?;
            if self.direct {
                self.accept_direct(fact)?;
            } else {
                #[cfg(test)]
                self.host.fact(fact)?;
                #[cfg(not(test))]
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            if kind == NativeKind::Document {
                self.document = Some(id);
            }
        }
        Ok(id)
    }
    pub(crate) fn buffer(&mut self, node: NativeNodeId) -> Result<BufferId, H::Error> {
        self.require_grammar()?;
        self.next_buffer = self
            .next_buffer
            .checked_add(1)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        let buffer = BufferId::from_raw(self.next_buffer).expect("checked nonzero buffer ID");
        self.fact(AcceptedFact::BufferCreated { node, buffer })?;
        Ok(buffer)
    }
    pub(crate) fn payload(&mut self, role: PayloadKind, raw: RSpan) -> Result<PayloadId, H::Error> {
        self.require_grammar()?;
        self.next_payload = self
            .next_payload
            .checked_add(1)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        let payload = PayloadId::from_raw(self.next_payload).expect("checked nonzero payload ID");
        self.fact(AcceptedFact::PayloadBegin { payload, role, raw })?;
        Ok(payload)
    }
    pub(crate) fn definition(&mut self) -> Result<DefinitionId, H::Error> {
        self.require_grammar()?;
        self.next_definition = self
            .next_definition
            .checked_add(1)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        Ok(DefinitionId::from_raw(self.next_definition).expect("checked nonzero definition ID"))
    }
    fn ensure_slot(&mut self) -> Result<(), H::Error> {
        self.mapping_access().ensure_slot()
    }
    fn retain(&mut self, pointer: usize, lease: H::Lease) -> Result<(), H::Error> {
        self.mapping_access().retain(pointer, lease)
    }
    fn release(&mut self, pointer: usize) -> Result<(), H::Error> {
        self.mapping_access().release(pointer)
    }
    pub(crate) fn prepare_ast_retirement(&mut self) -> Result<(), H::Error> {
        // Every Context field has an explicit ownership classification. Adding
        // another retained heap field must update this exhaustive boundary.
        let Self {
            host,
            next_node: _,
            document: _,
            final_phase: _,
            next_buffer: _,
            next_payload: _,
            next_definition: _,
            line_start: _,
            input_len: _,
            input_text: _,
            capture: _,
            pending_scan: _,
            direct: _,
            inline_policy: _,
            inline_html: _,
            shadow,
            mappings,
            definitions,
            pending_definition,
            registry,
        } = self;
        if pending_definition.is_some() || shadow.is_some() {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        registry.begin_mark(*host)?;
        mappings.mark_retained(registry, *host)?;
        definitions::mark_retained(definitions, registry, *host)?;
        registry.finish_mark(*host)
    }
    /// Driver calls only after its real AST and delimiter arenas have dropped.
    pub(crate) fn retire_ast(&mut self) -> Result<(), H::Error> {
        self.registry.retire(self.host)
    }
    pub(crate) fn discard_string(&mut self, value: String) -> Result<(), H::Error> {
        self.mapping_access().discard_string(value)
    }
    pub(crate) fn discard_vec<T>(&mut self, value: Vec<T>) -> Result<(), H::Error> {
        let pointer = if value.capacity() == 0 {
            0
        } else {
            value.as_ptr() as usize
        };
        drop(value);
        self.release(pointer)
    }
    pub(crate) fn discard_small<A: smallvec::Array>(
        &mut self,
        value: smallvec::SmallVec<A>,
    ) -> Result<(), H::Error> {
        let pointer = if value.spilled() {
            value.as_ptr() as usize
        } else {
            0
        };
        drop(value);
        self.release(pointer)
    }
    pub(crate) fn unbox<T>(&mut self, value: Box<T>) -> Result<T, H::Error> {
        let layout = std::alloc::Layout::new::<T>();
        if layout.size() == 0 {
            return Ok(*value);
        }
        let pointer = Box::into_raw(value);
        // SAFETY: Box uniquely owns a correctly initialized T. Read transfers T;
        // the exact allocation is freed without dropping the transferred value.
        let value = unsafe {
            let value = pointer.read();
            std::alloc::dealloc(pointer.cast::<u8>(), layout);
            value
        };
        self.release(pointer as usize)?;
        Ok(value)
    }
    pub(crate) fn grow_string(
        &mut self,
        value: &mut String,
        additional: usize,
    ) -> Result<(), H::Error> {
        self.mapping_access().grow_string(value, additional)
    }
    pub(crate) fn grow_vec<T>(
        &mut self,
        value: &mut Vec<T>,
        additional: usize,
    ) -> Result<(), H::Error> {
        self.registry.mutable(self.host)?;
        let needed = value
            .len()
            .checked_add(additional)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        if needed <= value.capacity() {
            return Ok(());
        }
        let capacity = needed.max(
            value
                .capacity()
                .max(4)
                .checked_mul(2)
                .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?,
        );
        let size = std::mem::size_of::<T>();
        let bytes = capacity
            .checked_mul(size)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        self.ensure_slot()?;
        let mut lease = self.host.reserve(bytes as u64)?;
        let mut grown = Vec::new();
        grown
            .try_reserve_exact(capacity)
            .map_err(|_| self.host.reject(NativeViolation::OutOfMemory))?;
        let actual = grown
            .capacity()
            .checked_mul(size)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        self.host.admit_actual(&mut lease, actual as u64)?;
        self.retain(grown.as_ptr() as usize, lease)?;
        let old_pointer = if value.capacity() == 0 {
            0
        } else {
            value.as_ptr() as usize
        };
        for item in value.drain(..) {
            self.host.work(WorkDelta::Copy(size as u64))?;
            grown.push(item);
        }
        *value = grown;
        self.release(old_pointer)
    }
    pub(crate) fn push<T>(&mut self, value: &mut Vec<T>, item: T) -> Result<(), H::Error> {
        self.event()?;
        self.grow_vec(value, 1)?;
        value.push(item);
        Ok(())
    }
    pub(crate) fn small_push<A: smallvec::Array>(
        &mut self,
        value: &mut smallvec::SmallVec<A>,
        item: A::Item,
    ) -> Result<(), H::Error> {
        self.registry.mutable(self.host)?;
        self.event()?;
        if value.len() == value.capacity() {
            let capacity = value
                .capacity()
                .max(4)
                .checked_mul(2)
                .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
            let size = std::mem::size_of::<A::Item>();
            let bytes = capacity
                .checked_mul(size)
                .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
            self.ensure_slot()?;
            let mut lease = self.host.reserve(bytes as u64)?;
            let mut grown = smallvec::SmallVec::<A>::new();
            grown
                .try_reserve_exact(capacity)
                .map_err(|_| self.host.reject(NativeViolation::OutOfMemory))?;
            self.host
                .admit_actual(&mut lease, (grown.capacity() * size) as u64)?;
            self.retain(grown.as_ptr() as usize, lease)?;
            let old_pointer = if value.spilled() {
                value.as_ptr() as usize
            } else {
                0
            };
            for previous in value.drain(..) {
                self.host.work(WorkDelta::Copy(size as u64))?;
                grown.push(previous);
            }
            *value = grown;
            self.release(old_pointer)?;
        }
        value.push(item);
        Ok(())
    }
    pub(crate) fn boxed<T>(&mut self, value: T) -> Result<Box<T>, H::Error> {
        let layout = std::alloc::Layout::new::<T>();
        if layout.size() == 0 {
            return Ok(Box::new(value));
        }
        self.ensure_slot()?;
        let mut lease = self.host.reserve(layout.size() as u64)?;
        // SAFETY: the exact T layout is nonzero; null is handled as a typed fault.
        let raw = unsafe { std::alloc::alloc(layout) }.cast::<T>();
        if raw.is_null() {
            return Err(self.host.reject(NativeViolation::OutOfMemory));
        }
        // Establish the real buffer owner before a forwarding Host can unwind
        // after admit_actual. Local drop order is boxed, then original lease.
        // SAFETY: raw has T's exact nonzero layout and unique ownership.
        let boxed = unsafe {
            raw.write(value);
            Box::from_raw(raw)
        };
        self.host.admit_actual(&mut lease, layout.size() as u64)?;
        self.retain(raw as usize, lease)?;
        Ok(boxed)
    }
    pub(crate) fn append(&mut self, target: &mut String, value: &str) -> Result<(), H::Error> {
        self.mapping_access().append(target, value)
    }
    // DecodedBytes counts produced native semantic/stage outputs once. Raw R
    // transport, capacity migration and joining existing text charge Copy only.
    pub(crate) fn decode_append(
        &mut self,
        target: &mut String,
        value: &str,
    ) -> Result<(), H::Error> {
        let mut offset = 0;
        while offset < value.len() {
            let mut end = (offset + 65_536).min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            self.admit_decoded((end - offset) as u64)?;
            self.append(target, &value[offset..end])?;
            offset = end;
        }
        Ok(())
    }
    pub(crate) fn decoded_string(&mut self, value: &str) -> Result<String, H::Error> {
        let mut target = String::new();
        self.decode_append(&mut target, value)?;
        Ok(target)
    }
    pub(crate) fn string(&mut self, value: &str) -> Result<String, H::Error> {
        self.mapping_access().string(value)
    }
    pub(crate) fn cow_mut<'v>(
        &mut self,
        value: &'v mut std::borrow::Cow<'static, str>,
    ) -> Result<&'v mut String, H::Error> {
        if let std::borrow::Cow::Borrowed(borrowed) = value {
            *value = std::borrow::Cow::Owned(self.string(borrowed)?);
        }
        match value {
            std::borrow::Cow::Owned(owned) => Ok(owned),
            std::borrow::Cow::Borrowed(_) => unreachable!(),
        }
    }
    pub(crate) fn append_cow(
        &mut self,
        target: &mut std::borrow::Cow<'static, str>,
        value: &str,
    ) -> Result<(), H::Error> {
        let target = self.cow_mut(target)?;
        self.append(target, value)
    }
}

struct Chunk<T, L> {
    values: Vec<T>,
    _lease: L,
}
struct Storage<T, L> {
    chunks: Vec<Chunk<T, L>>,
    _metadata: Option<L>,
    pending_metadata: Option<PendingArenaMetadata<T, L>>,
}
struct PendingArenaMetadata<T, L> {
    chunks: Vec<Chunk<T, L>>,
    metadata: L,
}

/// Stable chunk arena with fallible admission before each known capacity allocation.
pub struct FallibleArena<T, L = ()> {
    storage: std::cell::RefCell<Storage<T, L>>,
}
impl<T, L> std::fmt::Debug for FallibleArena<T, L> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FallibleArena")
    }
}
impl<T, L> Default for FallibleArena<T, L> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T, L> FallibleArena<T, L> {
    pub fn new() -> Self {
        Self {
            storage: std::cell::RefCell::new(Storage {
                chunks: Vec::new(),
                _metadata: None,
                pending_metadata: None,
            }),
        }
    }
    pub(crate) fn try_alloc<H: Host<Lease = L>>(
        &self,
        value: T,
        ctx: &mut Context<'_, H>,
    ) -> Result<&T, H::Error> {
        ctx.event()?;
        let mut storage = self.storage.borrow_mut();
        if storage
            .chunks
            .last()
            .is_none_or(|chunk| chunk.values.len() == chunk.values.capacity())
        {
            if storage.chunks.len() == storage.chunks.capacity() {
                let count = storage
                    .chunks
                    .capacity()
                    .max(2)
                    .checked_mul(2)
                    .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
                let bytes = count
                    .checked_mul(std::mem::size_of::<Chunk<T, L>>())
                    .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
                let mut lease = ctx.host.reserve(bytes as u64)?;
                let mut chunks = Vec::new();
                chunks
                    .try_reserve_exact(count)
                    .map_err(|_| ctx.host.reject(NativeViolation::OutOfMemory))?;
                ctx.host.admit_actual(
                    &mut lease,
                    (chunks.capacity() * std::mem::size_of::<Chunk<T, L>>()) as u64,
                )?;
                storage.pending_metadata = Some(PendingArenaMetadata {
                    chunks,
                    metadata: lease,
                });
                // Old chunks are already full. Their descriptor order has no semantic
                // meaning; all referenced T allocations retain their original address.
                // A failed checkpoint keeps both descriptor owners inside this arena.
                while !storage.chunks.is_empty() {
                    ctx.host
                        .work(WorkDelta::Copy(std::mem::size_of::<Chunk<T, L>>() as u64))?;
                    let chunk = storage.chunks.pop().expect("nonempty arena metadata");
                    storage
                        .pending_metadata
                        .as_mut()
                        .expect("pending arena metadata")
                        .chunks
                        .push(chunk);
                }
                let PendingArenaMetadata { chunks, metadata } = storage
                    .pending_metadata
                    .take()
                    .expect("pending arena metadata");
                let old = std::mem::replace(&mut storage.chunks, chunks);
                drop(old);
                storage._metadata = Some(metadata);
            }
            let bytes = 512usize
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            let mut lease = ctx.host.reserve(bytes as u64)?;
            let mut values = Vec::new();
            values
                .try_reserve_exact(512)
                .map_err(|_| ctx.host.reject(NativeViolation::OutOfMemory))?;
            ctx.host.admit_actual(
                &mut lease,
                (values.capacity() * std::mem::size_of::<T>()) as u64,
            )?;
            storage.chunks.push(Chunk {
                values,
                _lease: lease,
            });
        }
        let values = &mut storage
            .chunks
            .last_mut()
            .expect("admitted arena chunk")
            .values;
        let index = values.len();
        values.push(value);
        // SAFETY: the chunk was admitted with spare capacity, so push cannot reallocate.
        // Vec::as_mut_ptr does not materialize a slice reference over previously borrowed T.
        // Only the new initialized slot is used to construct the returned reference.
        let pointer = unsafe { values.as_mut_ptr().add(index) };
        drop(storage);
        // SAFETY: each chunk reserves its entire capacity before any reference escapes. A full
        // chunk is never grown again; moving its Vec descriptor leaves its allocation in place.
        // No mutable slice/reference to old elements is created by insertion. This call returns
        // a shared reference; parser nodes mutate through their own Cell/RefCell fields.
        // The reference is bounded by the arena borrow; objects drop only when the arena drops.
        Ok(unsafe { &*pointer })
    }
}
impl<T> FallibleArena<T, ()> {
    pub fn alloc(&self, value: T) -> &T {
        let mut host = LegacyHost;
        let mut ctx = Context::new(&mut host, 0);
        ctx.capture = false;
        match self.try_alloc(value, &mut ctx) {
            Ok(value) => value,
            Err(error) => match error {},
        }
    }
}

/// Compatibility API host; the product always uses its actual operation host.
pub(crate) struct LegacyHost;
impl Host for LegacyHost {
    type Error = std::convert::Infallible;
    type Lease = ();
    fn work(&mut self, _: WorkDelta) -> Result<(), Self::Error> {
        Ok(())
    }
    fn admit(&mut self, _: NativeAdmission) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve(&mut self, _: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn admit_actual(&mut self, _: &mut (), _: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reject(&mut self, violation: NativeViolation) -> Self::Error {
        panic!("legacy parser resource failure: {violation:?}")
    }
}

pub(crate) fn kind(value: &crate::nodes::NodeValue) -> Option<NativeKind> {
    use crate::nodes::NodeValue as V;
    Some(match value {
        V::Document => NativeKind::Document,
        V::Paragraph => NativeKind::Paragraph,
        V::Heading(_) => NativeKind::Heading,
        V::BlockQuote => NativeKind::BlockQuote,
        V::List(_) => NativeKind::List,
        V::Item(_) => NativeKind::ListItem,
        V::DescriptionList => NativeKind::DescriptionList,
        V::DescriptionItem(_) => NativeKind::DescriptionItem,
        V::DescriptionTerm => NativeKind::DescriptionTerm,
        V::DescriptionDetails => NativeKind::DescriptionDetails,
        V::CodeBlock(_) => NativeKind::CodeBlock,
        V::HtmlBlock(_) => NativeKind::HtmlBlock,
        V::ThematicBreak => NativeKind::ThematicBreak,
        V::TableOfContents => NativeKind::TableOfContents,
        V::Text(_) => NativeKind::Text,
        V::SoftBreak => NativeKind::SoftBreak,
        V::LineBreak => NativeKind::HardBreak,
        V::Code(_) => NativeKind::InlineCode,
        V::Math(_) => NativeKind::Math,
        V::Emph => NativeKind::Emphasis,
        V::Strong => NativeKind::Strong,
        V::Highlight => NativeKind::Highlight,
        V::Superscript => NativeKind::Superscript,
        V::Subscript => NativeKind::Subscript,
        V::Link(_) => NativeKind::Link,
        V::Image(_) => NativeKind::Image,
        V::NoteEmbed(_) => NativeKind::NoteEmbed,
        V::HtmlInline(_) => NativeKind::HtmlInline,
        V::Escaped => NativeKind::Escaped,
        V::Table(_) => NativeKind::Table,
        V::TableRow(_) => NativeKind::TableRow,
        V::TableCell => NativeKind::TableCell,
        V::Strikethrough => NativeKind::Strike,
        V::TaskCheckbox(_) => NativeKind::TaskCheckbox,
        V::FootnoteDefinition(_) => NativeKind::FootnoteDefinition,
        V::FootnoteReference(_) => NativeKind::FootnoteReference,
        _ => return None,
    })
}

#[cfg(test)]
fn final_attributes(value: &crate::nodes::NodeValue) -> Attributes<'_> {
    use crate::nodes::{ListDelimType, ListType, NodeValue as V};
    let delimiter = |list: &crate::nodes::NodeList| match list.delimiter {
        ListDelimType::Period => b'.',
        ListDelimType::Paren => b')',
    };
    fn owned(value: &value::ValueText) -> &str {
        value
            .owned_str()
            .expect("legacy final_attributes requires Owned AST fields")
    }
    match value {
        V::Text(text) => Attributes::Text(owned(text)),
        V::Heading(heading) => Attributes::Heading {
            level: heading.level,
            setext: heading.setext,
        },
        V::List(list) => Attributes::List {
            ordered: list.list_type == ListType::Ordered,
            start: list.start,
            delimiter: delimiter(list),
            bullet: list.bullet_char,
            tight: list.tight,
        },
        V::Item(list) => Attributes::Item {
            ordered: list.list_type == ListType::Ordered,
            start: list.start,
            delimiter: delimiter(list),
            bullet: list.bullet_char,
        },
        V::CodeBlock(code) => Attributes::Code {
            fenced: code.fenced,
            fence: code.fence_char,
            fence_length: code.fence_length,
            info: owned(&code.info),
            literal: owned(&code.literal),
        },
        V::Code(code) => Attributes::InlineCode {
            delimiter_length: code.num_backticks,
            literal: owned(&code.literal),
        },
        V::HtmlBlock(html) => Attributes::Html(owned(&html.literal)),
        V::HtmlInline(html) => Attributes::Html(owned(html)),
        V::Link(link) | V::Image(link) | V::NoteEmbed(link) => Attributes::Link {
            destination: owned(&link.url),
            title: owned(&link.title),
        },
        _ => Attributes::None,
    }
}

#[derive(Clone, Copy)]
#[cfg(test)]
struct FinalCursor<'a> {
    node: crate::nodes::Node<'a>,
    id: NativeNodeId,
    next_child: Option<crate::nodes::Node<'a>>,
    previous_child: Option<crate::nodes::Node<'a>>,
    child_count: u32,
    depth: u32,
}

#[derive(Default)]
#[cfg(test)]
struct FinalCounts {
    entered: u32,
    exited: u32,
    edges: u32,
}

#[cfg(test)]
struct FinalWorkspace<'a> {
    root: crate::nodes::Node<'a>,
    created_nodes: u32,
    once: Vec<u8>,
    stack: Vec<FinalCursor<'a>>,
    counts: FinalCounts,
}

#[cfg(test)]
fn same_node<'a>(
    left: Option<crate::nodes::Node<'a>>,
    right: Option<crate::nodes::Node<'a>>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => std::ptr::eq(left, right),
        _ => false,
    }
}

#[cfg(test)]
impl<'a> FinalWorkspace<'a> {
    fn new<H: Host>(
        root: crate::nodes::Node<'a>,
        ctx: &mut Context<'_, H>,
    ) -> Result<Self, H::Error> {
        let result = Self::create(root, ctx);
        if result.is_err() {
            ctx.final_phase = FinalPhase::Failed;
        }
        result
    }

    fn create<H: Host>(
        root: crate::nodes::Node<'a>,
        ctx: &mut Context<'_, H>,
    ) -> Result<Self, H::Error> {
        ctx.require_grammar()?;
        ctx.event()?;
        let ast = root.data();
        if !ctx.capture
            || ctx.next_node == 0
            || ctx.document.is_none()
            || root.parent().is_some()
            || root.previous_sibling().is_some()
            || root.next_sibling().is_some()
            || ast.native_id != ctx.document
            || kind(&ast.value) != Some(NativeKind::Document)
        {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let root_id = ast.native_id.expect("validated document identity");
        drop(ast);
        ctx.final_phase = FinalPhase::Active;
        ctx.host
            .work(WorkDelta::Copy(std::mem::size_of::<Self>() as u64))?;
        let mut workspace = Self {
            root,
            created_nodes: ctx.next_node,
            once: Vec::new(),
            stack: Vec::new(),
            counts: FinalCounts::default(),
        };
        let len = usize::try_from(workspace.created_nodes)
            .map_err(|_| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        ctx.grow_vec(&mut workspace.once, len)?;
        while workspace.once.len() < len {
            let count = (len - workspace.once.len()).min(65_536);
            ctx.event()?;
            ctx.host.work(WorkDelta::Copy(count as u64))?;
            workspace.once.resize(workspace.once.len() + count, 0);
        }
        ctx.final_fact(AcceptedFact::FinalBegin {
            root: root_id,
            created_nodes: workspace.created_nodes,
        })?;
        Ok(workspace)
    }

    fn node_id<H: Host>(
        &self,
        node: crate::nodes::Node<'a>,
        ctx: &mut Context<'_, H>,
    ) -> Result<NativeNodeId, H::Error> {
        match node.data().native_id {
            Some(id) if id.get() <= self.created_nodes => Ok(id),
            _ => Err(ctx.host.reject(NativeViolation::InvalidProvenance)),
        }
    }

    fn index<H: Host>(
        &self,
        id: NativeNodeId,
        ctx: &mut Context<'_, H>,
    ) -> Result<usize, H::Error> {
        let index = id
            .get()
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|&value| value < self.once.len())
            .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
        Ok(index)
    }

    fn check_end<H: Host>(&self, ctx: &mut Context<'_, H>) -> Result<(), H::Error> {
        let root = self.node_id(self.root, ctx)?;
        let root_index = self.index(root, ctx)?;
        if !self.stack.is_empty()
            || self.counts.entered == 0
            || self.counts.entered != self.counts.exited
            || self.counts.edges != self.counts.entered - 1
            || self.once[root_index] != 2
        {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let mut members = 0u32;
        for chunk in self.once.chunks(65_536) {
            ctx.event()?;
            ctx.scan(chunk.len())?;
            for &state in chunk {
                match state {
                    0 => {}
                    2 => {
                        members = members
                            .checked_add(1)
                            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
                    }
                    _ => return Err(ctx.host.reject(NativeViolation::InvalidProvenance)),
                }
            }
        }
        if members != self.counts.entered {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        Ok(())
    }
}

#[cfg(test)]
impl<H: Host> Context<'_, H> {
    fn final_fact(&mut self, fact: AcceptedFact<'_>) -> Result<(), H::Error> {
        let result = (|| {
            if self.final_phase != FinalPhase::Active {
                self.host.work(WorkDelta::Events(0))?;
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            // Only this actual-tree protocol sends terminal state for already admitted nodes.
            self.prepare_fact(fact)?;
            self.host.fact(fact)
        })();
        if result.is_err() {
            self.final_phase = FinalPhase::Failed;
        }
        result
    }

    fn final_enter<'a>(
        &mut self,
        workspace: &mut FinalWorkspace<'a>,
        node: crate::nodes::Node<'a>,
        placement: FinalPlacement,
        depth: u32,
    ) -> Result<(), H::Error> {
        let result = self.enter_final_node(workspace, node, placement, depth);
        if result.is_err() {
            self.final_phase = FinalPhase::Failed;
        }
        result
    }

    fn enter_final_node<'a>(
        &mut self,
        workspace: &mut FinalWorkspace<'a>,
        node: crate::nodes::Node<'a>,
        placement: FinalPlacement,
        depth: u32,
    ) -> Result<(), H::Error> {
        self.event()?;
        if self.final_phase != FinalPhase::Active {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        let id = workspace.node_id(node, self)?;
        let index = workspace.index(id, self)?;
        if workspace.once[index] != 0 {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        let child = match placement {
            FinalPlacement::Root => {
                if self.next_node != workspace.created_nodes
                    || self.document != Some(id)
                    || !same_node(Some(node), Some(workspace.root))
                    || node.parent().is_some()
                    || node.previous_sibling().is_some()
                    || node.next_sibling().is_some()
                    || workspace.counts.entered != 0
                    || !workspace.stack.is_empty()
                    || depth != 1
                {
                    return Err(self.host.reject(NativeViolation::InvalidProvenance));
                }
                false
            }
            FinalPlacement::Child { parent, ordinal } => {
                let frame = workspace
                    .stack
                    .last()
                    .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
                if workspace.node_id(frame.node, self)? != frame.id
                    || frame.id != parent
                    || workspace.once[workspace.index(parent, self)?] != 1
                    || !same_node(node.parent(), Some(frame.node))
                    || !same_node(frame.next_child, Some(node))
                    || !same_node(node.previous_sibling(), frame.previous_child)
                    || ordinal != frame.child_count
                    || depth
                        != frame
                            .depth
                            .checked_add(1)
                            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?
                {
                    return Err(self.host.reject(NativeViolation::InvalidProvenance));
                }
                true
            }
        };
        self.host.admit(NativeAdmission::Depth(u64::from(depth)))?;
        let entered = workspace.counts.entered.checked_add(1);
        let edges = workspace.counts.edges.checked_add(u32::from(child));
        let (entered, edges) = match (entered, edges) {
            (Some(entered), Some(edges))
                if entered <= workspace.created_nodes && edges < workspace.created_nodes =>
            {
                (entered, edges)
            }
            _ => return Err(self.host.reject(NativeViolation::InvalidProvenance)),
        };
        let ast = node.data();
        let kind = kind(&ast.value)
            .ok_or_else(|| self.host.reject(NativeViolation::UnsupportedConfiguration))?;
        if (kind == NativeKind::Document) == child {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        self.host
            .work(WorkDelta::Copy(1 + 2 * std::mem::size_of::<u32>() as u64))?;
        workspace.once[index] = 1;
        workspace.counts.entered = entered;
        workspace.counts.edges = edges;
        if child {
            self.host.work(WorkDelta::Copy(
                std::mem::size_of::<FinalCursor<'_>>() as u64
            ))?;
            let frame = workspace
                .stack
                .last_mut()
                .expect("checked actual parent cursor");
            let child_count = frame
                .child_count
                .checked_add(1)
                .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
            *frame = FinalCursor {
                child_count,
                next_child: node.next_sibling(),
                previous_child: Some(node),
                ..*frame
            };
        }
        self.grow_vec(&mut workspace.stack, 1)?;
        self.host.work(WorkDelta::Copy(
            std::mem::size_of::<FinalCursor<'_>>() as u64
        ))?;
        self.push(
            &mut workspace.stack,
            FinalCursor {
                node,
                id,
                next_child: node.first_child(),
                previous_child: None,
                child_count: 0,
                depth,
            },
        )?;
        if child {
            self.event()?; // Actual parent-to-child edge, independently of its callback.
        }
        self.final_fact(AcceptedFact::FinalEnter {
            node: id,
            kind,
            placement,
            attributes: final_attributes(&ast.value),
        })
    }

    fn final_exit(&mut self, workspace: &mut FinalWorkspace<'_>) -> Result<(), H::Error> {
        let result = self.exit_final_node(workspace);
        if result.is_err() {
            self.final_phase = FinalPhase::Failed;
        }
        result
    }

    fn exit_final_node(&mut self, workspace: &mut FinalWorkspace<'_>) -> Result<(), H::Error> {
        self.event()?;
        if self.final_phase != FinalPhase::Active {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        let frame = *workspace
            .stack
            .last()
            .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
        let index = workspace.index(frame.id, self)?;
        let parent = workspace
            .stack
            .iter()
            .rev()
            .nth(1)
            .map(|parent| parent.node);
        if workspace.node_id(frame.node, self)? != frame.id
            || workspace.once[index] != 1
            || frame.next_child.is_some()
            || !same_node(frame.node.last_child(), frame.previous_child)
            || (frame.child_count == 0) != frame.node.first_child().is_none()
            || !same_node(frame.node.parent(), parent)
            || (parent.is_none() && !same_node(Some(frame.node), Some(workspace.root)))
        {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        let exited = workspace
            .counts
            .exited
            .checked_add(1)
            .filter(|&count| count <= workspace.counts.entered)
            .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
        self.host.work(WorkDelta::Copy(
            1 + std::mem::size_of::<u32>() as u64 + std::mem::size_of::<usize>() as u64,
        ))?;
        self.final_fact(AcceptedFact::FinalExit {
            node: frame.id,
            child_count: frame.child_count,
        })?;
        workspace.once[index] = 2;
        workspace.counts.exited = exited;
        workspace.stack.truncate(workspace.stack.len() - 1);
        Ok(())
    }

    fn final_seal(&mut self, workspace: &FinalWorkspace<'_>) -> Result<(), H::Error> {
        let result = (|| {
            self.event()?;
            if self.final_phase != FinalPhase::Active {
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            workspace.check_end(self)?;
            let root = workspace.node_id(workspace.root, self)?;
            if self.document != Some(root)
                || workspace.root.parent().is_some()
                || workspace.root.previous_sibling().is_some()
                || workspace.root.next_sibling().is_some()
                || kind(&workspace.root.data().value) != Some(NativeKind::Document)
            {
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            self.final_fact(AcceptedFact::FinalSeal {
                root,
                member_nodes: workspace.counts.entered,
                edges: workspace.counts.edges,
            })?;
            self.final_phase = FinalPhase::Closed;
            Ok(())
        })();
        if result.is_err() {
            self.final_phase = FinalPhase::Failed;
        }
        result
    }
}

#[cfg(test)]
pub(crate) fn finish<H: Host>(
    root: crate::nodes::Node<'_>,
    ctx: &mut Context<'_, H>,
) -> Result<(), H::Error> {
    let result = (|| {
        let mut workspace = FinalWorkspace::new(root, ctx)?;
        ctx.final_enter(&mut workspace, root, FinalPlacement::Root, 1)?;
        while let Some(frame) = workspace.stack.last().copied() {
            ctx.event()?;
            if let Some(child) = frame.next_child {
                let depth = frame
                    .depth
                    .checked_add(1)
                    .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
                ctx.final_enter(
                    &mut workspace,
                    child,
                    FinalPlacement::Child {
                        parent: frame.id,
                        ordinal: frame.child_count,
                    },
                    depth,
                )?;
            } else {
                ctx.final_exit(&mut workspace)?;
            }
        }
        ctx.final_seal(&workspace)?;
        // Seal grants no result: tail cancellation and native cleanup still belong to this call.
        ctx.flush_scan()?;
        ctx.event()?;
        let FinalWorkspace { once, stack, .. } = workspace;
        ctx.discard_vec(stack)?;
        ctx.discard_vec(once)?;
        ctx.event()?;
        Ok(())
    })();
    if result.is_err() {
        ctx.final_phase = FinalPhase::Failed;
    }
    result
}

/// Legacy fact-transport entry retained only for vendor protocol regression tests.
#[cfg(test)]
pub fn parse_with_host<H: Host>(
    p: &str,
    options: &crate::Options,
    host: &mut H,
) -> Result<(), H::Error> {
    crate::parser::parse_with_host(p, options, host)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    include!("native/ownership_tests.rs");
    include!("native/grammar_delta_tests.rs");
    include!("native/title_indent_tests.rs");
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Debug, Eq, PartialEq)]
    pub(crate) enum Fault {
        Reserve,
        Actual,
        Work,
        WorkBudget { requested: u64, limit: u64 },
        Fact,
        NativeRecords { requested: u64, limit: u64 },
        Depth { requested: u64, limit: u64 },
        Cancelled,
        Violation(NativeViolation),
    }
    #[derive(Default)]
    struct Gauge {
        current: u64,
        peak: u64,
    }
    pub(crate) struct ChargedLease {
        gauge: Rc<RefCell<Gauge>>,
        bytes: u64,
    }
    impl ChargedLease {
        fn resize(&mut self, bytes: u64) {
            let mut gauge = self.gauge.borrow_mut();
            gauge.current = gauge.current - self.bytes + bytes;
            gauge.peak = gauge.peak.max(gauge.current);
            self.bytes = bytes;
        }
    }
    impl Drop for ChargedLease {
        fn drop(&mut self) {
            let mut gauge = self.gauge.borrow_mut();
            gauge.current = gauge.current.checked_sub(self.bytes).unwrap();
        }
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FinalRecord {
        Begin {
            root: NativeNodeId,
            created_nodes: u32,
        },
        Enter {
            node: NativeNodeId,
            kind: NativeKind,
            placement: FinalPlacement,
        },
        Exit {
            node: NativeNodeId,
            child_count: u32,
        },
        Seal {
            root: NativeNodeId,
            member_nodes: u32,
            edges: u32,
        },
    }
    pub(crate) struct MemoryHost {
        gauge: Rc<RefCell<Gauge>>,
        cancelled: Arc<AtomicBool>,
        first_fault: Option<Arc<Fault>>,
        reserve_calls: u64,
        actual_calls: u64,
        copy_calls: u64,
        event_calls: u64,
        event_units: u64,
        work_units: u64,
        work_limit: Option<u64>,
        scan_calls: u64,
        scan_bytes: u64,
        admit_calls: u64,
        node_admissions: u64,
        record_admissions: u64,
        native_records: u64,
        native_records_limit: Option<u64>,
        depth_limit: Option<u64>,
        depths: Vec<u64>,
        reserve_fault_at: Option<u64>,
        actual_fault_at: Option<u64>,
        cancel_reserve_at: Option<u64>,
        cancel_actual_at: Option<u64>,
        cancel_copy_at: Option<u64>,
        cancel_event_at: Option<u64>,
        cancel_scan_at: Option<u64>,
        cancel_admit_at: Option<u64>,
        event_fault_at: Option<u64>,
        fact_fault_at: Option<u64>,
        cancel_after_last_final: bool,
        fault_event_after_last_final: Option<u64>,
        cancel_large_scan: bool,
        large_scan_fault: bool,
        decoded: u64,
        facts: u64,
        node_created: Vec<(NativeNodeId, Option<NativeNodeId>, NativeKind)>,
        terminal_facts: Vec<FinalRecord>,
        block_consumes: Vec<(NativeNodeId, PSpan)>,
        block_whitespace: Vec<PSpan>,
        code_terminals: Vec<(usize, Option<PSpan>)>,
        code_literals: Vec<(usize, bool)>,
        code_text: Vec<RSpan>,
    }
    impl MemoryHost {
        pub(crate) fn new() -> Self {
            Self {
                gauge: Rc::new(RefCell::new(Gauge::default())),
                cancelled: Arc::new(AtomicBool::new(false)),
                first_fault: None,
                reserve_calls: 0,
                actual_calls: 0,
                copy_calls: 0,
                event_calls: 0,
                event_units: 0,
                work_units: 0,
                work_limit: None,
                scan_calls: 0,
                scan_bytes: 0,
                admit_calls: 0,
                node_admissions: 0,
                record_admissions: 0,
                native_records: 0,
                native_records_limit: None,
                depth_limit: None,
                depths: Vec::new(),
                reserve_fault_at: None,
                actual_fault_at: None,
                cancel_reserve_at: None,
                cancel_actual_at: None,
                cancel_copy_at: None,
                cancel_event_at: None,
                cancel_scan_at: None,
                cancel_admit_at: None,
                event_fault_at: None,
                fact_fault_at: None,
                cancel_after_last_final: false,
                fault_event_after_last_final: None,
                cancel_large_scan: false,
                large_scan_fault: false,
                decoded: 0,
                facts: 0,
                node_created: Vec::new(),
                terminal_facts: Vec::new(),
                block_consumes: Vec::new(),
                block_whitespace: Vec::new(),
                code_terminals: Vec::new(),
                code_literals: Vec::new(),
                code_text: Vec::new(),
            }
        }
        fn fail(&mut self, fault: Fault) -> Arc<Fault> {
            self.first_fault
                .get_or_insert_with(|| Arc::new(fault))
                .clone()
        }
        fn checkpoint(&mut self) -> Result<(), Arc<Fault>> {
            if let Some(error) = &self.first_fault {
                return Err(error.clone());
            }
            if self.cancelled.load(Ordering::Acquire) {
                return Err(self.fail(Fault::Cancelled));
            }
            Ok(())
        }
        fn assert_original(&mut self, error: &Arc<Fault>, expected: Fault) {
            assert_eq!(**error, expected);
            assert!(Arc::ptr_eq(error, self.first_fault.as_ref().unwrap()));
            let later = self.work(WorkDelta::Events(1)).unwrap_err();
            assert!(Arc::ptr_eq(error, &later));
        }
        pub(crate) fn arm_chain_fault(&mut self, cancel: bool) {
            if cancel {
                self.cancel_event_at = Some(self.event_calls + 2);
            } else {
                self.event_fault_at = Some(self.event_calls + 2);
            }
        }
        pub(crate) fn observed_scan_bytes(&self) -> u64 {
            self.scan_bytes
        }
        pub(crate) fn arm_latex_scan_fault(&mut self, cancel: bool) {
            self.cancel_large_scan = cancel;
            self.large_scan_fault = !cancel;
        }
        pub(crate) fn arm_title_reread_fault(&mut self, cancel: bool) {
            self.cancel_large_scan = cancel;
            self.large_scan_fault = !cancel;
        }
        pub(crate) fn assert_work_fault_released(&mut self, error: &Arc<Fault>, cancel: bool) {
            self.assert_original(
                error,
                if cancel {
                    Fault::Cancelled
                } else {
                    Fault::Work
                },
            );
            assert_eq!(self.cancelled.load(Ordering::Acquire), cancel);
            assert_eq!(self.gauge.borrow().current, 0);
        }
    }
    impl Host for MemoryHost {
        type Error = Arc<Fault>;
        type Lease = ChargedLease;
        fn work(&mut self, delta: WorkDelta) -> Result<(), Self::Error> {
            self.checkpoint()?;
            let amount = match delta {
                WorkDelta::Copy(amount) | WorkDelta::Scan(amount) | WorkDelta::Events(amount) => {
                    amount
                }
            };
            let requested = self.work_units.checked_add(amount).unwrap();
            if let Some(limit) = self.work_limit
                && requested > limit
            {
                return Err(self.fail(Fault::WorkBudget { requested, limit }));
            }
            self.work_units = requested;
            match delta {
                WorkDelta::Copy(bytes) => {
                    assert!(bytes <= 65_536);
                    self.copy_calls += 1;
                    if self.cancel_copy_at == Some(self.copy_calls) {
                        self.cancelled.store(true, Ordering::Release);
                    }
                }
                WorkDelta::Scan(bytes) => {
                    assert!(bytes <= 65_536);
                    self.scan_calls += 1;
                    self.scan_bytes += bytes;
                    if self.cancel_scan_at == Some(self.scan_calls) {
                        self.cancelled.store(true, Ordering::Release);
                    }
                    if bytes == 65_536 {
                        if self.cancel_large_scan {
                            self.cancelled.store(true, Ordering::Release);
                        } else if self.large_scan_fault {
                            return Err(self.fail(Fault::Work));
                        }
                    }
                }
                WorkDelta::Events(events) => {
                    assert!(events <= 256);
                    self.event_calls += 1;
                    self.event_units += events;
                    if self.cancel_event_at == Some(self.event_calls) {
                        self.cancelled.store(true, Ordering::Release);
                    } else if self.event_fault_at == Some(self.event_calls) {
                        return Err(self.fail(Fault::Work));
                    }
                }
            }
            self.checkpoint()
        }
        fn admit(&mut self, request: NativeAdmission) -> Result<(), Self::Error> {
            self.admit_calls += 1;
            if self.cancel_admit_at == Some(self.admit_calls) {
                self.cancelled.store(true, Ordering::Release);
            }
            self.checkpoint()?;
            if let NativeAdmission::Depth(depth) = request {
                self.depths.push(depth);
                if let Some(limit) = self.depth_limit
                    && depth > limit
                {
                    return Err(self.fail(Fault::Depth {
                        requested: depth,
                        limit,
                    }));
                }
            }
            let amount = match request {
                NativeAdmission::Node(amount) => {
                    self.node_admissions += amount;
                    amount
                }
                NativeAdmission::Record(amount) => {
                    self.record_admissions += amount;
                    amount
                }
                _ => 0,
            };
            let requested = self.native_records + amount;
            if let Some(limit) = self.native_records_limit
                && requested > limit
            {
                return Err(self.fail(Fault::NativeRecords { requested, limit }));
            }
            self.native_records = requested;
            if let NativeAdmission::DecodedBytes(bytes) = request {
                self.decoded += bytes;
            }
            Ok(())
        }
        fn reserve(&mut self, bytes: u64) -> Result<Self::Lease, Self::Error> {
            self.reserve_calls += 1;
            if self.cancel_reserve_at == Some(self.reserve_calls) {
                self.cancelled.store(true, Ordering::Release);
            }
            self.checkpoint()?;
            if self.reserve_fault_at == Some(self.reserve_calls) {
                return Err(self.fail(Fault::Reserve));
            }
            let mut lease = ChargedLease {
                gauge: self.gauge.clone(),
                bytes: 0,
            };
            lease.resize(bytes);
            Ok(lease)
        }
        fn admit_actual(&mut self, lease: &mut Self::Lease, bytes: u64) -> Result<(), Self::Error> {
            self.actual_calls += 1;
            if self.cancel_actual_at == Some(self.actual_calls) {
                self.cancelled.store(true, Ordering::Release);
            }
            self.checkpoint()?;
            if self.actual_fault_at == Some(self.actual_calls) {
                return Err(self.fail(Fault::Actual));
            }
            lease.resize(bytes);
            Ok(())
        }
        fn fact(&mut self, fact: AcceptedFact<'_>) -> Result<(), Self::Error> {
            self.checkpoint()?;
            self.facts += 1;
            if self.fact_fault_at == Some(self.facts) {
                return Err(self.fail(Fault::Fact));
            }
            if let AcceptedFact::Syntax {
                role: SyntaxRole::BlockWhitespace,
                span,
                ..
            } = fact
            {
                self.block_whitespace.push(span);
            }
            match fact {
                AcceptedFact::NodeCreated { node, parent, kind } => {
                    self.node_created.push((node, parent, kind));
                }
                AcceptedFact::BlockConsume { node, span } => self.block_consumes.push((node, span)),
                AcceptedFact::CodeTerminal { at, eof, .. } => self.code_terminals.push((at, eof)),
                AcceptedFact::FinalBegin {
                    root,
                    created_nodes,
                } => {
                    self.terminal_facts.push(FinalRecord::Begin {
                        root,
                        created_nodes,
                    });
                }
                AcceptedFact::FinalEnter {
                    node,
                    kind,
                    attributes,
                    placement,
                } => {
                    self.terminal_facts.push(FinalRecord::Enter {
                        node,
                        kind,
                        placement,
                    });
                    if let Attributes::Code { literal, .. } = attributes {
                        self.code_literals
                            .push((literal.len(), literal.ends_with('\n')));
                    }
                }
                AcceptedFact::FinalExit { node, child_count } => {
                    self.terminal_facts
                        .push(FinalRecord::Exit { node, child_count });
                }
                AcceptedFact::FinalSeal {
                    root,
                    member_nodes,
                    edges,
                } => {
                    self.terminal_facts.push(FinalRecord::Seal {
                        root,
                        member_nodes,
                        edges,
                    });
                    if self.cancel_after_last_final {
                        self.cancelled.store(true, Ordering::Release);
                    }
                    if let Some(offset) = self.fault_event_after_last_final {
                        self.event_fault_at = Some(self.event_calls + offset);
                    }
                }
                AcceptedFact::TextAccepted { span, .. } => self.code_text.push(span),
                _ => {}
            }
            Ok(())
        }
        fn reject(&mut self, violation: NativeViolation) -> Self::Error {
            self.fail(Fault::Violation(violation))
        }
    }

    fn graph_node<'a>(
        arena: &'a FallibleArena<crate::nodes::AstNode<'a>, ChargedLease>,
        ctx: &mut Context<'_, MemoryHost>,
        value: crate::nodes::NodeValue,
        parent: Option<crate::nodes::Node<'a>>,
    ) -> crate::nodes::Node<'a> {
        let id = ctx
            .node(
                parent.and_then(|parent| parent.data().native_id),
                kind(&value).unwrap(),
            )
            .unwrap();
        let value = crate::nodes::AstNode::from(value);
        value.data_mut().native_id = Some(id);
        let node = arena.try_alloc(value, ctx).unwrap();
        if let Some(parent) = parent {
            parent.append(node);
        }
        node
    }

    #[test]
    fn terminal_actual_reparented_dfs_and_detached_births_have_paired_protocol() {
        use crate::nodes::NodeValue as V;
        let mut host = MemoryHost::new();
        {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            let detached = graph_node(&arena, &mut ctx, V::Paragraph, None);
            let root = graph_node(&arena, &mut ctx, V::Document, None);
            let first = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
            let text = graph_node(&arena, &mut ctx, V::Text("a".into()), Some(first));
            let second = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
            let strong = graph_node(&arena, &mut ctx, V::Strong, Some(first));
            strong.append(text);
            let id = |node: crate::nodes::Node<'_>| node.data().native_id.unwrap();
            let enter = |node, kind, parent, ordinal| FinalRecord::Enter {
                node,
                kind,
                placement: FinalPlacement::Child { parent, ordinal },
            };
            finish(root, &mut ctx).unwrap();
            assert_eq!(
                ctx.host.terminal_facts,
                [
                    FinalRecord::Begin {
                        root: id(root),
                        created_nodes: 6
                    },
                    FinalRecord::Enter {
                        node: id(root),
                        kind: NativeKind::Document,
                        placement: FinalPlacement::Root
                    },
                    enter(id(first), NativeKind::Paragraph, id(root), 0),
                    enter(id(strong), NativeKind::Strong, id(first), 0),
                    enter(id(text), NativeKind::Text, id(strong), 0),
                    FinalRecord::Exit {
                        node: id(text),
                        child_count: 0
                    },
                    FinalRecord::Exit {
                        node: id(strong),
                        child_count: 1
                    },
                    FinalRecord::Exit {
                        node: id(first),
                        child_count: 1
                    },
                    enter(id(second), NativeKind::Paragraph, id(root), 1),
                    FinalRecord::Exit {
                        node: id(second),
                        child_count: 0
                    },
                    FinalRecord::Exit {
                        node: id(root),
                        child_count: 2
                    },
                    FinalRecord::Seal {
                        root: id(root),
                        member_nodes: 5,
                        edges: 4
                    },
                ]
            );
            assert_eq!(id(root).get(), 2);
            assert!(ctx.host.terminal_facts.iter().all(|fact| !matches!(fact,
                FinalRecord::Enter { node, .. } | FinalRecord::Exit { node, .. } if *node == id(detached))));
            assert_eq!(ctx.final_phase, FinalPhase::Closed);
            assert_eq!(ctx.host.node_admissions, 6);
            assert_eq!(ctx.host.record_admissions, 0);
            assert_eq!(ctx.registry.test_counts().0, 0);
        }
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_empty_nonfirst_root_keeps_unseen_births_out_of_membership() {
        use crate::nodes::NodeValue as V;
        let mut host = MemoryHost::new();
        {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            graph_node(&arena, &mut ctx, V::Paragraph, None);
            let root = graph_node(&arena, &mut ctx, V::Document, None);
            let root_id = root.data().native_id.unwrap();
            finish(root, &mut ctx).unwrap();
            assert_eq!(
                ctx.host.terminal_facts,
                [
                    FinalRecord::Begin {
                        root: root_id,
                        created_nodes: 2
                    },
                    FinalRecord::Enter {
                        node: root_id,
                        kind: NativeKind::Document,
                        placement: FinalPlacement::Root
                    },
                    FinalRecord::Exit {
                        node: root_id,
                        child_count: 0
                    },
                    FinalRecord::Seal {
                        root: root_id,
                        member_nodes: 1,
                        edges: 0
                    },
                ]
            );
            assert_eq!(ctx.host.native_records, 2);
        }
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_actual_children_prevent_early_root_exit_without_trusted_bool() {
        use crate::nodes::NodeValue as V;
        let mut host = MemoryHost::new();
        let error = {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            let root = graph_node(&arena, &mut ctx, V::Document, None);
            let child = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
            let mut workspace = FinalWorkspace::new(root, &mut ctx).unwrap();
            ctx.final_enter(&mut workspace, root, FinalPlacement::Root, 1)
                .unwrap();
            assert!(same_node(workspace.stack[0].next_child, Some(child)));
            let error = ctx.final_exit(&mut workspace).unwrap_err();
            assert_eq!(workspace.counts.exited, 0);
            assert_eq!(ctx.host.terminal_facts.len(), 2);
            assert_eq!(ctx.final_phase, FinalPhase::Failed);
            error
        };
        host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_generic_fact_rejects_every_protocol_variant_and_cannot_restart() {
        for packet in 0..4 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, crate::nodes::NodeValue::Document, None);
                let node = root.data().native_id.unwrap();
                let fact = match packet {
                    0 => AcceptedFact::FinalBegin {
                        root: node,
                        created_nodes: 1,
                    },
                    1 => AcceptedFact::FinalEnter {
                        node,
                        kind: NativeKind::Document,
                        attributes: Attributes::None,
                        placement: FinalPlacement::Root,
                    },
                    2 => AcceptedFact::FinalExit {
                        node,
                        child_count: 0,
                    },
                    _ => AcceptedFact::FinalSeal {
                        root: node,
                        member_nodes: 1,
                        edges: 0,
                    },
                };
                let error = ctx.fact(fact).unwrap_err();
                assert!(Arc::ptr_eq(&error, &finish(root, &mut ctx).unwrap_err()));
                assert!(Arc::ptr_eq(
                    &error,
                    &ctx.node(None, NativeKind::Paragraph).unwrap_err()
                ));
                assert_eq!(ctx.next_node, 1);
                assert!(ctx.host.terminal_facts.is_empty());
                error
            };
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_closed_context_rejects_second_graph_and_grammar_mutation() {
        for mutation in [false, true] {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, crate::nodes::NodeValue::Document, None);
                finish(root, &mut ctx).unwrap();
                let error = if mutation {
                    ctx.fact(AcceptedFact::Retyped {
                        node: root.data().native_id.unwrap(),
                        kind: NativeKind::Paragraph,
                    })
                    .unwrap_err()
                } else {
                    finish(root, &mut ctx).unwrap_err()
                };
                assert_eq!(ctx.final_phase, FinalPhase::Failed);
                assert_eq!(ctx.host.terminal_facts.len(), 4);
                assert_eq!(ctx.host.record_admissions, 0);
                error
            };
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_enter_checks_actual_root_parent_ordinal_depth_and_once() {
        use crate::nodes::NodeValue as V;
        for case in 0..7 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, V::Document, None);
                let first = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
                let second = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
                let mut workspace = FinalWorkspace::new(root, &mut ctx).unwrap();
                if case != 0 {
                    ctx.final_enter(&mut workspace, root, FinalPlacement::Root, 1)
                        .unwrap();
                }
                let root_id = root.data().native_id.unwrap();
                let (node, placement, depth) = match case {
                    0 => (first, FinalPlacement::Root, 1),
                    1 => (root, FinalPlacement::Root, 1),
                    2 => (
                        first,
                        FinalPlacement::Child {
                            parent: NativeNodeId::from_raw(99).unwrap(),
                            ordinal: 0,
                        },
                        2,
                    ),
                    3 => (
                        first,
                        FinalPlacement::Child {
                            parent: root_id,
                            ordinal: 1,
                        },
                        2,
                    ),
                    4 => (
                        second,
                        FinalPlacement::Child {
                            parent: root_id,
                            ordinal: 0,
                        },
                        2,
                    ),
                    5 => (
                        first,
                        FinalPlacement::Child {
                            parent: root_id,
                            ordinal: 0,
                        },
                        3,
                    ),
                    _ => (first, FinalPlacement::Root, 1),
                };
                let accepted = ctx.host.terminal_facts.len();
                let error = ctx
                    .final_enter(&mut workspace, node, placement, depth)
                    .unwrap_err();
                assert_eq!(ctx.final_phase, FinalPhase::Failed);
                assert_eq!(ctx.host.terminal_facts.len(), accepted);
                assert!(Arc::ptr_eq(
                    &error,
                    &ctx.final_enter(&mut workspace, node, placement, depth)
                        .unwrap_err()
                ));
                error
            };
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_rejects_unknown_missing_aliased_ids_and_nonroot_document() {
        use crate::nodes::NodeValue as V;
        for case in 0..4 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, V::Document, None);
                let child = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
                match case {
                    0 => child.data_mut().native_id = None,
                    1 => child.data_mut().native_id = NativeNodeId::from_raw(3),
                    2 => child.data_mut().native_id = root.data().native_id,
                    _ => child.data_mut().value = V::Document,
                }
                let error = finish(root, &mut ctx).unwrap_err();
                assert_eq!(ctx.final_phase, FinalPhase::Failed);
                assert_eq!(ctx.host.terminal_facts.len(), 2);
                error
            };
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_allocation_initialization_and_cursor_faults_release_all_owners() {
        use crate::nodes::NodeValue as V;
        for point in 0..7 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, V::Document, None);
                graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
                ctx.ensure_slot().unwrap();
                let error = if point < 5 {
                    match point {
                        0 => ctx.host.reserve_fault_at = Some(ctx.host.reserve_calls + 1),
                        1 => ctx.host.actual_fault_at = Some(ctx.host.actual_calls + 1),
                        2 => ctx.host.cancel_copy_at = Some(ctx.host.copy_calls + 2),
                        3 => ctx.host.cancel_reserve_at = Some(ctx.host.reserve_calls + 1),
                        _ => ctx.host.cancel_actual_at = Some(ctx.host.actual_calls + 1),
                    }
                    finish(root, &mut ctx).unwrap_err()
                } else {
                    let mut workspace = FinalWorkspace::new(root, &mut ctx).unwrap();
                    if point == 5 {
                        ctx.host.reserve_fault_at = Some(ctx.host.reserve_calls + 1);
                    } else {
                        ctx.host.actual_fault_at = Some(ctx.host.actual_calls + 1);
                    }
                    let error = ctx
                        .final_enter(&mut workspace, root, FinalPlacement::Root, 1)
                        .unwrap_err();
                    assert_eq!(workspace.once[0], 1);
                    assert_eq!(ctx.host.terminal_facts.len(), 1); // Begin only.
                    error
                };
                assert_eq!(ctx.final_phase, FinalPhase::Failed);
                ctx.host.cancelled.store(true, Ordering::Release);
                assert!(Arc::ptr_eq(&error, &finish(root, &mut ctx).unwrap_err()));
                error
            };
            host.assert_original(
                &error,
                match point {
                    0 | 5 => Fault::Reserve,
                    1 | 6 => Fault::Actual,
                    _ => Fault::Cancelled,
                },
            );
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_cursor_migration_cancel_retains_old_new_capacity_and_active_state() {
        use crate::nodes::NodeValue as V;
        let mut host = MemoryHost::new();
        let error = {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            let root = graph_node(&arena, &mut ctx, V::Document, None);
            let mut parent = root;
            for _ in 0..10 {
                parent = graph_node(&arena, &mut ctx, V::Paragraph, Some(parent));
            }
            let mut workspace = FinalWorkspace::new(root, &mut ctx).unwrap();
            ctx.final_enter(&mut workspace, root, FinalPlacement::Root, 1)
                .unwrap();
            while workspace.stack.len() < workspace.stack.capacity() {
                let frame = *workspace.stack.last().unwrap();
                ctx.final_enter(
                    &mut workspace,
                    frame.next_child.unwrap(),
                    FinalPlacement::Child {
                        parent: frame.id,
                        ordinal: frame.child_count,
                    },
                    frame.depth + 1,
                )
                .unwrap();
            }
            let frame = *workspace.stack.last().unwrap();
            let child = frame.next_child.unwrap();
            let old_capacity = workspace.stack.capacity();
            let before = ctx.host.gauge.borrow().current;
            let accepted = ctx.host.terminal_facts.len();
            ctx.host.cancel_copy_at = Some(ctx.host.copy_calls + 3);
            let error = ctx
                .final_enter(
                    &mut workspace,
                    child,
                    FinalPlacement::Child {
                        parent: frame.id,
                        ordinal: frame.child_count,
                    },
                    frame.depth + 1,
                )
                .unwrap_err();
            assert_eq!(ctx.final_phase, FinalPhase::Failed);
            assert_eq!(
                workspace.once[(child.data().native_id.unwrap().get() - 1) as usize],
                1
            );
            assert_eq!(ctx.host.terminal_facts.len(), accepted);
            assert!(
                ctx.host.gauge.borrow().peak
                    >= before + (old_capacity * 2 * std::mem::size_of::<FinalCursor<'_>>()) as u64
            );
            error
        };
        host.assert_original(&error, Fault::Cancelled);
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_each_begin_enter_exit_seal_callback_fault_is_terminal() {
        use crate::nodes::NodeValue as V;
        for packet in 1..=8 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, V::Document, None);
                let paragraph = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
                graph_node(&arena, &mut ctx, V::Text("a".into()), Some(paragraph));
                ctx.host.fact_fault_at = Some(ctx.host.facts + packet);
                let error = finish(root, &mut ctx).unwrap_err();
                assert_eq!(ctx.final_phase, FinalPhase::Failed);
                assert_eq!(ctx.host.terminal_facts.len() as u64, packet - 1);
                let callbacks = ctx.host.facts;
                assert!(Arc::ptr_eq(&error, &finish(root, &mut ctx).unwrap_err()));
                assert_eq!(ctx.host.facts, callbacks);
                assert_eq!(ctx.host.native_records, 3);
                error
            };
            host.assert_original(&error, Fault::Fact);
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_seal_tail_same_flag_cancel_rejects_and_frees_native_objects() {
        let mut host = MemoryHost::new();
        host.cancel_after_last_final = true;
        let error = parse_with_host("**a**", &crate::Options::default(), &mut host).unwrap_err();
        assert!(matches!(
            host.terminal_facts.last(),
            Some(FinalRecord::Seal { .. })
        ));
        host.assert_original(&error, Fault::Cancelled);
        assert!(host.cancelled.load(Ordering::Acquire));
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_depth_comes_from_actual_stack_and_does_not_readmit_entities() {
        use crate::nodes::NodeValue as V;
        let mut host = MemoryHost::new();
        host.native_records_limit = Some(3);
        {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            let root = graph_node(&arena, &mut ctx, V::Document, None);
            let paragraph = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
            graph_node(&arena, &mut ctx, V::Text("a".into()), Some(paragraph));
            finish(root, &mut ctx).unwrap();
            assert_eq!(ctx.host.depths, [1, 2, 3]);
            assert_eq!(ctx.host.node_admissions, 3);
            assert_eq!(ctx.host.record_admissions, 0);
            assert_eq!(ctx.host.native_records, 3);
            assert_eq!(ctx.host.facts, 11); // Three births, Begin + paired three nodes + Seal.
        }
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_actual_stack_depth_limit_is_typed_before_child_callback() {
        use crate::nodes::NodeValue as V;
        let mut host = MemoryHost::new();
        host.depth_limit = Some(2);
        let error = {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            let root = graph_node(&arena, &mut ctx, V::Document, None);
            let paragraph = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
            graph_node(&arena, &mut ctx, V::Strong, Some(paragraph));
            let error = finish(root, &mut ctx).unwrap_err();
            assert_eq!(ctx.host.depths, [1, 2, 3]);
            assert_eq!(ctx.host.terminal_facts.len(), 3);
            error
        };
        host.assert_original(
            &error,
            Fault::Depth {
                requested: 3,
                limit: 2,
            },
        );
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_actual_cycle_and_changed_parent_are_bounded_without_ancestors() {
        use crate::nodes::NodeValue as V;
        for cycle in [false, true] {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, V::Document, None);
                let child = graph_node(&arena, &mut ctx, V::Paragraph, Some(root));
                let mut workspace = FinalWorkspace::new(root, &mut ctx).unwrap();
                ctx.final_enter(&mut workspace, root, FinalPlacement::Root, 1)
                    .unwrap();
                if cycle {
                    child.append(root);
                } else {
                    child.detach();
                }
                let placement = FinalPlacement::Child {
                    parent: root.data().native_id.unwrap(),
                    ordinal: 0,
                };
                let error = if cycle {
                    ctx.final_enter(&mut workspace, child, placement, 2)
                        .unwrap();
                    ctx.final_enter(
                        &mut workspace,
                        root,
                        FinalPlacement::Child {
                            parent: child.data().native_id.unwrap(),
                            ordinal: 0,
                        },
                        3,
                    )
                    .unwrap_err()
                } else {
                    ctx.final_enter(&mut workspace, child, placement, 2)
                        .unwrap_err()
                };
                assert_eq!(ctx.final_phase, FinalPhase::Failed);
                assert!(
                    !ctx.host
                        .terminal_facts
                        .iter()
                        .any(|fact| matches!(fact, FinalRecord::Seal { .. }))
                );
                error
            };
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_dense_seal_rejects_unclosed_duplicate_or_forged_membership() {
        use crate::nodes::NodeValue as V;
        for case in 0..5 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                let root = graph_node(&arena, &mut ctx, V::Document, None);
                graph_node(&arena, &mut ctx, V::Paragraph, None);
                let mut workspace = FinalWorkspace::new(root, &mut ctx).unwrap();
                ctx.final_enter(&mut workspace, root, FinalPlacement::Root, 1)
                    .unwrap();
                if case != 0 {
                    ctx.final_exit(&mut workspace).unwrap();
                }
                match case {
                    1 => workspace.once[1] = 2,
                    2 => workspace.once[1] = 1,
                    3 => workspace.counts.edges = 1,
                    4 => {
                        workspace.once[0] = 0;
                        workspace.once[1] = 2;
                    }
                    _ => {}
                }
                let error = ctx.final_seal(&workspace).unwrap_err();
                assert_eq!(ctx.final_phase, FinalPhase::Failed);
                assert!(
                    !ctx.host
                        .terminal_facts
                        .iter()
                        .any(|fact| matches!(fact, FinalRecord::Seal { .. }))
                );
                error
            };
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn terminal_work_is_charged_before_once_and_counts_and_first_fault_is_kept() {
        let mut host = MemoryHost::new();
        let (error, limit) = {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            let root = graph_node(&arena, &mut ctx, crate::nodes::NodeValue::Document, None);
            let mut workspace = FinalWorkspace::new(root, &mut ctx).unwrap();
            let limit = ctx.host.work_units + 1;
            ctx.host.work_limit = Some(limit);
            let error = ctx
                .final_enter(&mut workspace, root, FinalPlacement::Root, 1)
                .unwrap_err();
            assert_eq!(workspace.once[0], 0);
            assert_eq!(workspace.counts.entered, 0);
            assert_eq!(ctx.host.terminal_facts.len(), 1);
            ctx.host.cancelled.store(true, Ordering::Release);
            assert!(Arc::ptr_eq(&error, &finish(root, &mut ctx).unwrap_err()));
            (error, limit)
        };
        assert!(
            matches!(*error, Fault::WorkBudget { requested, limit: actual } if requested > limit && actual == limit)
        );
        let again = host.work(WorkDelta::Events(0)).unwrap_err();
        assert!(Arc::ptr_eq(&error, &again));
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn terminal_tail_work_and_each_workspace_release_failure_discards_seal() {
        for offset in 1..=4 {
            let mut host = MemoryHost::new();
            host.fault_event_after_last_final = Some(offset);
            let error = parse_with_host("", &crate::Options::default(), &mut host).unwrap_err();
            assert!(matches!(
                host.terminal_facts.last(),
                Some(FinalRecord::Seal { .. })
            ));
            host.cancelled.store(true, Ordering::Release);
            host.assert_original(&error, Fault::Work);
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn node_creation_has_one_admission_and_two_real_events() {
        let mut host = MemoryHost::new();
        let document = {
            let mut ctx = Context::new(&mut host, 0);
            let document = ctx.node(None, NativeKind::Document).unwrap();
            assert_eq!(ctx.document, Some(document));
            assert_eq!(ctx.next_node, 1);
            document
        };
        assert_eq!(host.node_admissions, 1);
        assert_eq!(host.record_admissions, 0);
        assert_eq!(host.native_records, 1);
        assert_eq!(host.event_calls, 2);
        assert_eq!(host.node_created, [(document, None, NativeKind::Document)]);
        assert_eq!(host.reserve_calls, 0);
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn node_admission_boundaries_precede_identity_and_next_record() {
        for limit in [0, 1] {
            let mut host = MemoryHost::new();
            host.native_records_limit = Some(limit);
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                if limit == 0 {
                    let error = ctx.node(None, NativeKind::Document).unwrap_err();
                    assert_eq!(ctx.next_node, 0);
                    assert_eq!(ctx.document, None);
                    error
                } else {
                    let node = ctx.node(None, NativeKind::Document).unwrap();
                    let error = ctx
                        .fact(AcceptedFact::BlockStart { node, at: 0 })
                        .unwrap_err();
                    assert_eq!(ctx.next_node, 1);
                    assert_eq!(ctx.document, Some(node));
                    error
                }
            };
            host.assert_original(
                &error,
                Fault::NativeRecords {
                    requested: limit + 1,
                    limit,
                },
            );
            assert_eq!(host.native_records, limit);
            assert_eq!(host.node_created.len() as u64, limit);
            assert_eq!(host.facts, limit);
            assert_eq!(host.reserve_calls, 0);
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn consume_validation_uses_registered_document_without_fixed_id() {
        let mut host = MemoryHost::new();
        let (document, paragraph) = {
            let mut ctx = Context::new(&mut host, 4);
            let before_root = ctx.node(None, NativeKind::Paragraph).unwrap();
            let document = ctx.node(None, NativeKind::Document).unwrap();
            let paragraph = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
            assert_eq!(document.get(), 2);
            for _ in 0..7 {
                ctx.scan_step().unwrap();
            }
            for node in [before_root, paragraph, document] {
                ctx.fact(AcceptedFact::BlockConsume {
                    node,
                    span: PSpan { start: 0, end: 4 },
                })
                .unwrap();
            }
            assert_eq!(ctx.pending_scan, 0);
            (document, paragraph)
        };
        assert_eq!(
            host.node_created[2],
            (paragraph, Some(document), NativeKind::Paragraph)
        );
        assert_eq!(host.node_admissions, 3);
        assert_eq!(host.record_admissions, 1);
        assert_eq!(host.native_records, 4);
        assert_eq!(host.event_calls, 9);
        assert_eq!(host.scan_calls, 1);
        assert_eq!(host.scan_bytes, 7);
        assert_eq!(host.facts, 4);
        assert_eq!(
            host.block_consumes,
            [(document, PSpan { start: 0, end: 4 })]
        );
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn consume_rejects_missing_root_unknown_identity_and_invalid_spans() {
        for case in 0..4 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 4);
                let paragraph = if case == 0 {
                    ctx.node(None, NativeKind::Paragraph).unwrap()
                } else {
                    let document = ctx.node(None, NativeKind::Document).unwrap();
                    ctx.node(Some(document), NativeKind::Paragraph).unwrap()
                };
                let (node, span) = match case {
                    0 => (paragraph, PSpan { start: 0, end: 4 }),
                    1 => (
                        NativeNodeId::from_raw(3).unwrap(),
                        PSpan { start: 0, end: 4 },
                    ),
                    2 => (paragraph, PSpan { start: 3, end: 2 }),
                    _ => (paragraph, PSpan { start: 0, end: 5 }),
                };
                let events = ctx.host.event_calls;
                let error = ctx
                    .fact(AcceptedFact::BlockConsume { node, span })
                    .unwrap_err();
                assert_eq!(ctx.host.event_calls, events);
                assert_eq!(ctx.host.record_admissions, 0);
                assert!(ctx.host.block_consumes.is_empty());
                error
            };
            host.cancelled.store(true, Ordering::Release);
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn document_registration_requires_unique_root_and_successful_callback() {
        for with_parent in [false, true] {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let prior = ctx
                    .node(
                        None,
                        if with_parent {
                            NativeKind::Paragraph
                        } else {
                            NativeKind::Document
                        },
                    )
                    .unwrap();
                let error = ctx
                    .node(with_parent.then_some(prior), NativeKind::Document)
                    .unwrap_err();
                assert_eq!(ctx.next_node, 1);
                assert_eq!(ctx.document, (!with_parent).then_some(prior));
                assert_eq!(ctx.host.node_admissions, 1);
                assert_eq!(ctx.host.node_created.len(), 1);
                error
            };
            host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
            assert_eq!(host.gauge.borrow().current, 0);
        }
        let mut host = MemoryHost::new();
        host.fact_fault_at = Some(1);
        let error = {
            let mut ctx = Context::new(&mut host, 0);
            let _owned = ctx.string("owned until context drop").unwrap();
            let error = ctx.node(None, NativeKind::Document).unwrap_err();
            assert_eq!(ctx.next_node, 1);
            assert_eq!(ctx.document, None);
            assert!(ctx.host.node_created.is_empty());
            ctx.host.cancelled.store(true, Ordering::Release);
            let later = ctx.node(None, NativeKind::Document).unwrap_err();
            assert!(Arc::ptr_eq(&error, &later));
            assert_eq!(ctx.next_node, 1);
            assert_eq!(ctx.host.facts, 1);
            error
        };
        host.assert_original(&error, Fault::Fact);
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn creation_and_nontransported_consume_observe_same_flag_and_release() {
        for point in 0..5 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 4);
                let _owned = ctx.string("owned until cancelled context drop").unwrap();
                if point < 3 {
                    match point {
                        0 => ctx.host.cancel_event_at = Some(ctx.host.event_calls + 1),
                        1 => ctx.host.cancel_admit_at = Some(ctx.host.admit_calls + 1),
                        _ => ctx.host.cancel_event_at = Some(ctx.host.event_calls + 2),
                    }
                    let error = ctx.node(None, NativeKind::Document).unwrap_err();
                    assert_eq!(ctx.next_node, u32::from(point == 2));
                    assert_eq!(ctx.document, None);
                    assert!(ctx.host.node_created.is_empty());
                    error
                } else {
                    let document = ctx.node(None, NativeKind::Document).unwrap();
                    let node = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
                    if point == 3 {
                        for _ in 0..7 {
                            ctx.scan_step().unwrap();
                        }
                        ctx.host.cancel_scan_at = Some(ctx.host.scan_calls + 1);
                    } else {
                        ctx.host.cancel_event_at = Some(ctx.host.event_calls + 1);
                    }
                    let error = ctx
                        .fact(AcceptedFact::BlockConsume {
                            node,
                            span: PSpan { start: 0, end: 4 },
                        })
                        .unwrap_err();
                    assert_eq!(ctx.host.record_admissions, 0);
                    assert!(ctx.host.block_consumes.is_empty());
                    assert_eq!(ctx.host.facts, 2);
                    error
                }
            };
            host.assert_original(&error, Fault::Cancelled);
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn creation_notification_flush_observes_cancel_before_callback() {
        let mut host = MemoryHost::new();
        let error = {
            let mut ctx = Context::new(&mut host, 4);
            let _owned = ctx.string("owned until context drop").unwrap();
            let before_scan = ctx.host.scan_bytes;
            for _ in 0..7 {
                ctx.scan_step().unwrap();
            }
            ctx.host.cancel_scan_at = Some(ctx.host.scan_calls + 1);
            let error = ctx.node(None, NativeKind::Document).unwrap_err();
            assert_eq!(ctx.next_node, 1);
            assert_eq!(ctx.document, None);
            assert_eq!(ctx.pending_scan, 0);
            assert_eq!(ctx.host.scan_bytes - before_scan, 7);
            assert_eq!(ctx.host.node_admissions, 1);
            assert_eq!(ctx.host.record_admissions, 0);
            assert!(ctx.host.node_created.is_empty());
            error
        };
        host.assert_original(&error, Fault::Cancelled);
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn consume_keeps_first_work_or_capacity_fault_after_later_cancel() {
        for point in 0..3 {
            let mut host = MemoryHost::new();
            let error = {
                let mut ctx = Context::new(&mut host, 4);
                let document = ctx.node(None, NativeKind::Document).unwrap();
                let node = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
                let _owned = ctx.string("owned until context drop").unwrap();
                let consume = AcceptedFact::BlockConsume {
                    node,
                    span: PSpan { start: 0, end: 4 },
                };
                for _ in 0..7 {
                    ctx.scan_step().unwrap();
                }
                let error = if point == 0 {
                    ctx.host.event_fault_at = Some(ctx.host.event_calls + 1);
                    ctx.fact(consume).unwrap_err()
                } else {
                    if point == 1 {
                        ctx.host.reserve_fault_at = Some(ctx.host.reserve_calls + 1);
                    } else {
                        ctx.host.actual_fault_at = Some(ctx.host.actual_calls + 1);
                    }
                    ctx.string("second allocation").unwrap_err()
                };
                ctx.host.cancelled.store(true, Ordering::Release);
                let later = ctx.fact(consume).unwrap_err();
                assert!(Arc::ptr_eq(&error, &later));
                assert_eq!(ctx.host.record_admissions, 0);
                assert_eq!(ctx.host.facts, 2);
                assert!(ctx.host.block_consumes.is_empty());
                error
            };
            host.assert_original(
                &error,
                match point {
                    0 => Fault::Work,
                    1 => Fault::Reserve,
                    _ => Fault::Actual,
                },
            );
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn other_transported_facts_keep_each_record_admission() {
        let mut host = MemoryHost::new();
        {
            let mut ctx = Context::new(&mut host, 0);
            let document = ctx.node(None, NativeKind::Document).unwrap();
            let node = ctx.node(Some(document), NativeKind::Paragraph).unwrap();
            for fact in [
                AcceptedFact::BlockStart { node, at: 0 },
                AcceptedFact::Syntax {
                    node,
                    role: SyntaxRole::BlockWhitespace,
                    span: PSpan { start: 0, end: 0 },
                },
                AcceptedFact::Retyped {
                    node,
                    kind: NativeKind::Paragraph,
                },
                AcceptedFact::BlockConsume {
                    node: document,
                    span: PSpan { start: 0, end: 0 },
                },
            ] {
                ctx.fact(fact).unwrap();
            }
        }
        assert_eq!(host.node_admissions, 2);
        assert_eq!(host.record_admissions, 4);
        assert_eq!(host.native_records, 6);
        assert_eq!(host.facts, 6);
        assert_eq!(host.event_calls, 8);
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn consume_transport_keeps_every_actual_document_line() {
        let input = "# h\n\n> p\n> c\n\n- q\n\n```\nx\n```\n";
        let mut host = MemoryHost::new();
        parse_with_host(input, &crate::Options::default(), &mut host).unwrap();
        let document = host
            .node_created
            .iter()
            .find(|(_, _, kind)| *kind == NativeKind::Document)
            .unwrap()
            .0;
        let mut offset = 0;
        let spans: Vec<_> = input
            .split_inclusive('\n')
            .map(|line| {
                let span = PSpan {
                    start: offset,
                    end: offset + line.len(),
                };
                offset = span.end;
                (document, span)
            })
            .collect();
        assert_eq!(host.block_consumes, spans);
        assert_eq!(offset, input.len());
        assert_eq!(host.node_admissions as usize, host.node_created.len());
        assert_eq!(
            host.native_records + host.terminal_facts.len() as u64,
            host.facts
        );
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn noncapturing_compatibility_does_not_register_or_check_document_facts() {
        let mut host = MemoryHost::new();
        {
            let mut ctx = Context::new(&mut host, 0);
            ctx.capture = false;
            let unknown = NativeNodeId::from_raw(7).unwrap();
            ctx.fact(AcceptedFact::BlockConsume {
                node: unknown,
                span: PSpan { start: 2, end: 1 },
            })
            .unwrap();
            ctx.node(Some(unknown), NativeKind::Document).unwrap();
            assert_eq!(ctx.document, None);
        }
        assert_eq!(host.facts, 0);
        assert_eq!(host.event_calls, 1);
        assert_eq!(host.node_admissions, 1);
        assert_eq!(host.record_admissions, 0);
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn reserve_and_actual_faults_preserve_identity_and_release_all_capacity() {
        for actual in [false, true] {
            let mut host = MemoryHost::new();
            // Fault during the registry page/directory admission preceding String.
            if actual {
                host.actual_fault_at = Some(2);
            } else {
                host.reserve_fault_at = Some(2);
            }
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                ctx.string("allocation").unwrap_err()
            };
            host.assert_original(
                &error,
                if actual {
                    Fault::Actual
                } else {
                    Fault::Reserve
                },
            );
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn independent_string_growth_accounts_for_old_and_new_peak() {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        {
            let mut ctx = Context::new(&mut host, 0);
            let mut text = ctx.string(&"x".repeat(65_536)).unwrap();
            let old_capacity = text.capacity() as u64;
            let before = gauge.borrow().current;
            ctx.append(&mut text, "y").unwrap();
            let new_capacity = text.capacity() as u64;
            assert!(gauge.borrow().peak >= before + new_capacity);
            assert_eq!(gauge.borrow().current, before - old_capacity + new_capacity);
            assert_eq!(text.len(), 65_537);
            ctx.discard_string(text).unwrap();
        }
        assert_eq!(gauge.borrow().current, 0);
    }

    #[test]
    fn cancellation_interrupts_string_and_vec_capacity_moves() {
        for vector in [false, true] {
            let mut host = MemoryHost::new();
            let gauge = host.gauge.clone();
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                if vector {
                    let mut rows = Vec::new();
                    for value in 0..8usize {
                        ctx.push(&mut rows, value).unwrap();
                    }
                    ctx.host.cancel_copy_at = Some(ctx.host.copy_calls + 3);
                    ctx.push(&mut rows, 9).unwrap_err()
                } else {
                    let mut text = ctx.string(&"x".repeat(131_072)).unwrap();
                    ctx.host.cancel_copy_at = Some(ctx.host.copy_calls + 2);
                    let error = ctx.append(&mut text, "y").unwrap_err();
                    assert_eq!(text.len(), 131_072);
                    error
                }
            };
            host.assert_original(&error, Fault::Cancelled);
            assert!(host.cancelled.load(Ordering::Acquire));
            assert_eq!(gauge.borrow().current, 0);
        }
    }

    #[test]
    fn cancellation_during_context_ledger_move_keeps_both_owners_until_drop() {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        let error = {
            let mut ctx = Context::new(&mut host, 0);
            let mut strings = Vec::new();
            for _ in 0..8 {
                strings.push(ctx.string("owned").unwrap());
            }
            ctx.host.cancel_copy_at = Some(ctx.host.copy_calls + 2);
            let error = ctx.string("ninth").unwrap_err();
            let (live, old_index, pending_index) = ctx.registry.test_counts();
            assert_eq!(live, 8);
            assert_eq!(old_index, 16);
            assert!(pending_index.is_some());
            assert_eq!(strings.iter().map(String::len).sum::<usize>(), 40);
            error
        };
        host.assert_original(&error, Fault::Cancelled);
        assert_eq!(gauge.borrow().current, 0);
    }

    #[test]
    fn cancel_during_retain_keeps_box_permit_live_until_box_owner_drops() {
        struct Witness {
            gauge: Rc<RefCell<Gauge>>,
            dropped: Rc<std::cell::Cell<bool>>,
            minimum: u64,
        }
        impl Drop for Witness {
            fn drop(&mut self) {
                assert!(self.gauge.borrow().current >= self.minimum);
                self.dropped.set(true);
            }
        }
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        let dropped = Rc::new(std::cell::Cell::new(false));
        let error = {
            let mut ctx = Context::new(&mut host, 0);
            ctx.ensure_slot().unwrap();
            let minimum = gauge.borrow().current + std::mem::size_of::<Witness>() as u64;
            ctx.host.cancel_event_at = Some(ctx.host.event_calls + 1);
            match ctx.boxed(Witness {
                gauge: gauge.clone(),
                dropped: dropped.clone(),
                minimum,
            }) {
                Ok(_) => panic!("retain checkpoint must cancel"),
                Err(error) => error,
            }
        };
        assert!(dropped.get());
        host.assert_original(&error, Fault::Cancelled);
        assert_eq!(gauge.borrow().current, 0);
    }

    #[test]
    fn repeated_temporary_releases_bound_ledger_capacity_and_work() {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        {
            let mut ctx = Context::new(&mut host, 0);
            for _ in 0..1000 {
                let temporary = ctx.string("temporary").unwrap();
                ctx.discard_string(temporary).unwrap();
            }
            let (live, capacity, pending) = ctx.registry.test_counts();
            assert_eq!(live, 0);
            assert!(capacity <= 32);
            assert!(pending.is_none());
            assert!(ctx.host.event_calls < 10_000);
        }
        assert_eq!(gauge.borrow().current, 0);
    }

    #[test]
    fn arena_references_survive_insertion_chunk_switch_and_descriptor_growth() {
        let mut host = MemoryHost::new();
        {
            let mut ctx = Context::new(&mut host, 0);
            let integers = FallibleArena::new();
            let first = integers.try_alloc(7usize, &mut ctx).unwrap();
            let second = integers.try_alloc(11usize, &mut ctx).unwrap();
            for value in 2..2050 {
                integers.try_alloc(value, &mut ctx).unwrap();
                assert_eq!((*first, *second), (7, 11));
            }
            let cells = FallibleArena::new();
            let left = cells
                .try_alloc(std::cell::Cell::new(1usize), &mut ctx)
                .unwrap();
            let right = cells
                .try_alloc(std::cell::Cell::new(2usize), &mut ctx)
                .unwrap();
            for value in 2..2050 {
                cells
                    .try_alloc(std::cell::Cell::new(value), &mut ctx)
                    .unwrap();
                left.set(left.get() + 1);
                right.set(right.get() + 2);
            }
            assert_eq!((left.get(), right.get()), (2049, 4098));
        }
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn arena_chunk_and_actual_faults_release_capacity() {
        for actual in [false, true] {
            let mut host = MemoryHost::new();
            if actual {
                host.actual_fault_at = Some(2);
            } else {
                host.cancel_reserve_at = Some(3);
            }
            let error = {
                let mut ctx = Context::new(&mut host, 0);
                let arena = FallibleArena::new();
                if actual {
                    arena.try_alloc(1usize, &mut ctx).unwrap_err()
                } else {
                    for value in 0..512usize {
                        arena.try_alloc(value, &mut ctx).unwrap();
                    }
                    arena.try_alloc(513, &mut ctx).unwrap_err()
                }
            };
            host.assert_original(
                &error,
                if actual {
                    Fault::Actual
                } else {
                    Fault::Cancelled
                },
            );
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn arena_descriptor_cancel_preserves_all_existing_references_and_capacity() {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        let error = {
            let mut ctx = Context::new(&mut host, 0);
            let arena = FallibleArena::new();
            let first = arena.try_alloc(0usize, &mut ctx).unwrap();
            for value in 1..2048usize {
                arena.try_alloc(value, &mut ctx).unwrap();
            }
            ctx.host.cancel_copy_at = Some(ctx.host.copy_calls + 2);
            let before = gauge.borrow().current;
            let error = arena.try_alloc(2048, &mut ctx).unwrap_err();
            assert_eq!(*first, 0);
            let storage = arena.storage.borrow();
            assert_eq!(storage.chunks.len(), 3);
            assert_eq!(storage.pending_metadata.as_ref().unwrap().chunks.len(), 1);
            assert!(gauge.borrow().current > before);
            error
        };
        host.assert_original(&error, Fault::Cancelled);
        assert_eq!(gauge.borrow().current, 0);
    }

    #[test]
    fn native_success_and_internal_cancel_return_with_zero_live_leases() {
        let input = "## H\n\n> **x** &NotEqualTilde; [l](a&amp;b \"c\\*d\")\n\n[a]: <u>";
        for cancelled in [false, true] {
            let mut host = MemoryHost::new();
            if cancelled {
                host.cancel_copy_at = Some(4);
            }
            let result = parse_with_host(input, &crate::Options::default(), &mut host);
            if cancelled {
                host.assert_original(&result.unwrap_err(), Fault::Cancelled);
            } else {
                result.unwrap();
                assert!(host.facts > 20);
            }
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn accepted_block_whitespace_records_only_the_actual_removed_indent() {
        for (input, expected) in [
            (">\tcode\n", PSpan { start: 1, end: 2 }),
            ("   p\n", PSpan { start: 0, end: 3 }),
            ("   # h\n", PSpan { start: 0, end: 3 }),
            ("  > p\n", PSpan { start: 0, end: 2 }),
            ("  - p\n", PSpan { start: 0, end: 2 }),
            ("  ```\nx\n  ```\n", PSpan { start: 0, end: 2 }),
            ("a\n\n   b\n", PSpan { start: 3, end: 6 }),
            ("p\n  ===\n", PSpan { start: 2, end: 4 }),
            ("  ---\n", PSpan { start: 0, end: 2 }),
        ] {
            let mut host = MemoryHost::new();
            parse_with_host(input, &crate::Options::default(), &mut host).unwrap();
            assert_eq!(host.block_whitespace, [expected], "{input:?}");
            assert!(
                input.as_bytes()[expected.start..expected.end]
                    .iter()
                    .all(|byte| matches!(byte, b' ' | b'\t'))
            );
            assert_eq!(host.gauge.borrow().current, 0);
        }
        let mut host = MemoryHost::new();
        parse_with_host("p\n  continued\n", &crate::Options::default(), &mut host).unwrap();
        // Paragraph continuation transports the indent through R; it is not removed here.
        assert!(host.block_whitespace.is_empty());
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn fenced_code_terminal_comes_from_a_real_nonempty_unterminated_eof_line() {
        for (input, bytes, terminal, ends_lf) in [
            ("```\nx", 2, true, true),
            ("```\nx\n", 2, false, true),
            ("```\nx\r", 2, false, false),
            ("```\nx\n```", 2, false, true),
            ("```", 0, false, false),
            ("```\n", 0, false, false),
            ("```\n\n", 1, false, true),
            ("```\nx\n\n\n", 4, false, true),
            ("```\nx\n   ", 6, true, true),
            ("> ```\n> x", 2, true, true),
            ("> ```\n> x\nother", 2, false, true),
        ] {
            let mut host = MemoryHost::new();
            parse_with_host(input, &crate::Options::default(), &mut host).unwrap();
            assert_eq!(host.code_literals, [(bytes, ends_lf)], "{input:?}");
            if terminal {
                let (_, eof) = host.code_terminals[0];
                assert_eq!(host.code_terminals.len(), 1, "{input:?}");
                assert_eq!(
                    eof,
                    Some(PSpan {
                        start: input.len(),
                        end: input.len()
                    })
                );
                let raw = host.code_text[0];
                assert_eq!(raw.end - raw.start, bytes - 1);
                assert_eq!(host.code_terminals[0].0, raw.end);
            } else {
                assert!(host.code_terminals.is_empty(), "{input:?}");
            }
            assert_eq!(host.gauge.borrow().current, 0);
        }
        let input = format!("```\n{}", "x".repeat(262_144));
        let mut host = MemoryHost::new();
        parse_with_host(&input, &crate::Options::default(), &mut host).unwrap();
        assert_eq!(host.code_literals, [(262_145, true)]);
        assert_eq!(host.code_text[0].end - host.code_text[0].start, 262_144);
        assert_eq!(host.decoded, 262_145);
        assert_eq!(host.gauge.borrow().current, 0);
    }

    #[test]
    fn indented_code_terminal_distinguishes_real_line_ending_and_actual_eof() {
        for (input, bytes, eof) in [
            ("    x", 2, true),
            ("    x  ", 4, true),
            ("    x\n", 2, false),
            ("    x\n\n", 2, false),
            ("    x\n   ", 2, false),
        ] {
            let mut host = MemoryHost::new();
            parse_with_host(input, &crate::Options::default(), &mut host).unwrap();
            assert_eq!(host.code_literals, [(bytes, true)], "{input:?}");
            assert_eq!(host.code_terminals.len(), 1);
            assert_eq!(
                host.code_terminals[0].1,
                eof.then_some(PSpan {
                    start: input.len(),
                    end: input.len()
                }),
                "{input:?}"
            );
            assert_eq!(host.gauge.borrow().current, 0);
        }
    }

    #[test]
    fn native_decoded_count_tracks_real_outputs_without_final_borrow_charge() {
        let mut host = MemoryHost::new();
        parse_with_host("&#xE000;", &crate::Options::default(), &mut host).unwrap();
        assert_eq!(host.decoded, 3);
        {
            let mut ctx = Context::new(&mut host, 0);
            let raw = ctx.string("a\\*").unwrap();
            let value = crate::strings::clean_payload_with_host(
                &raw,
                PayloadKind::Destination,
                &mut ctx,
                None,
            )
            .unwrap();
            assert_eq!(value, "a*");
            ctx.discard_string(value).unwrap();
            ctx.discard_string(raw).unwrap();
        }
        // Entity stage produces 3 bytes; Escape stage produces 2.
        assert_eq!(host.decoded, 3 + 3 + 2);
        assert_eq!(host.gauge.borrow().current, 0);
    }
}
