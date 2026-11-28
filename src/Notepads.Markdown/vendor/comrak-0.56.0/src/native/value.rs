// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Current AST slots carry acceptance identities, never a second text owner.
//! Compatibility grammar and bounded grammar temporaries retain explicit Cow.

use super::{NativeNodeId, PayloadId, PayloadKind};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NodeValueRole {
    Text,
    InlineCode,
    MathInput,
    BlockCode,
    Html,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AcceptedValueRef {
    Node {
        node: NativeNodeId,
        role: NodeValueRole,
    },
    Payload {
        payload: PayloadId,
        role: PayloadKind,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ValueStorage {
    Owned(Cow<'static, str>),
    Pending,
    Empty,
    Accepted(AcceptedValueRef),
}

/// Explicit compatibility storage or a private native accepted-field identity.
/// It has no implicit string conversion, dereference, formatting or materializer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueText {
    pub(crate) repr: ValueStorage,
}

impl Default for ValueText {
    fn default() -> Self {
        Self::from("")
    }
}

impl From<Cow<'static, str>> for ValueText {
    fn from(value: Cow<'static, str>) -> Self {
        Self {
            repr: ValueStorage::Owned(value),
        }
    }
}
impl From<String> for ValueText {
    fn from(value: String) -> Self {
        Self::from(Cow::Owned(value))
    }
}
impl From<&'static str> for ValueText {
    fn from(value: &'static str) -> Self {
        Self::from(Cow::Borrowed(value))
    }
}

impl ValueText {
    /// Only legacy-owned storage has a continuous string; native identities do
    /// not fall back to source reconstruction or allocate through this method.
    pub fn owned_str(&self) -> Option<&str> {
        match &self.repr {
            ValueStorage::Owned(value) => Some(value),
            _ => None,
        }
    }
    /// Explicit compatibility mutation. Callers needing native values must use
    /// the Context acceptance operations instead of mutating retained strings.
    pub fn owned_cow_mut(&mut self) -> Option<&mut Cow<'static, str>> {
        match &mut self.repr {
            ValueStorage::Owned(value) => Some(value),
            _ => None,
        }
    }
    pub fn into_owned_cow(self) -> Result<Cow<'static, str>, Self> {
        match self.repr {
            ValueStorage::Owned(value) => Ok(value),
            _ => Err(self),
        }
    }
    pub(crate) fn accepted(value: AcceptedValueRef) -> Self {
        Self {
            repr: ValueStorage::Accepted(value),
        }
    }
    pub(crate) fn pending() -> Self {
        Self {
            repr: ValueStorage::Pending,
        }
    }
    pub(crate) fn empty() -> Self {
        Self {
            repr: ValueStorage::Empty,
        }
    }
}

use super::{AppendKind, Context, Host, NativeViolation, WorkDelta, mapped};

pub(super) trait ValueControl {
    type Error;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Self::Error>;
    fn reject(&mut self, error: NativeViolation) -> Self::Error;
    fn record(&mut self, mut bytes: usize) -> Result<(), Self::Error> {
        while bytes != 0 {
            let count = bytes.min(65_536);
            self.work(WorkDelta::Events(1))?;
            self.work(WorkDelta::Copy(count as u64))?;
            bytes -= count;
        }
        Ok(())
    }
}

pub(super) struct GrammarControl<'a, H: Host>(pub &'a mut H);
impl<H: Host> ValueControl for GrammarControl<'_, H> {
    type Error = H::Error;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Self::Error> {
        self.0.work(delta)
    }
    fn reject(&mut self, error: NativeViolation) -> Self::Error {
        self.0.reject(error)
    }
}

/// Borrowed storage dispatch only. Code LF is an already accepted terminal;
/// this view never changes the map or runs a decoder/normalizer.
pub(super) struct ValueView<'a> {
    pub map: &'a mapped::MappedValue,
    pub prefix: &'a str,
    pub body_len: usize,
    pub terminal: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) struct ValuePosition {
    pub position: usize,
    atom: usize,
    offset: usize,
    piece: Option<(super::BufferId, usize)>,
}
impl ValuePosition {
    pub(super) fn key(at: usize) -> Self {
        Self {
            position: at,
            ..Self::default()
        }
    }
}

pub(super) fn seek<C: ValueControl>(
    view: &ValueView<'_>,
    at: usize,
    control: &mut C,
) -> Result<ValuePosition, C::Error> {
    if at > view.len(control)? {
        return Err(control.reject(NativeViolation::InvalidProvenance));
    }
    let mut cursor = ValuePosition {
        position: at,
        ..ValuePosition::default()
    };
    let mut remaining = at.saturating_sub(view.prefix.len()).min(view.body_len);
    if at < view.prefix.len() {
        return Ok(cursor);
    }
    for atom in &view.map.atoms {
        control.work(WorkDelta::Events(1))?;
        control.work(WorkDelta::Scan(std::mem::size_of::<mapped::Atom>() as u64))?;
        if remaining < atom.bytes {
            cursor.offset = remaining;
            return Ok(cursor);
        }
        remaining -= atom.bytes;
        cursor.atom = cursor
            .atom
            .checked_add(1)
            .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
    }
    if remaining != 0 {
        return Err(control.reject(NativeViolation::InvalidProvenance));
    }
    Ok(cursor)
}

fn bounded_slice<'a, C: ValueControl>(
    text: &'a str,
    start: usize,
    count: usize,
    control: &mut C,
) -> Result<&'a str, C::Error> {
    let mut end = start
        .checked_add(count)
        .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
    if end > text.len() || !text.is_char_boundary(start) {
        return Err(control.reject(NativeViolation::InvalidProvenance));
    }
    while !text.is_char_boundary(end) {
        control.work(WorkDelta::Scan(1))?;
        end = end
            .checked_sub(1)
            .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
    }
    if start == end {
        return Err(control.reject(NativeViolation::InvalidProvenance));
    }
    control.work(WorkDelta::Scan((end - start) as u64))?;
    text.get(start..end)
        .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))
}

struct Backing<'a> {
    text: &'a str,
    start: usize,
    available: usize,
}

/// Locate existing storage, without consuming any current-value bytes. This
/// also serves boundary checks and semantic reference-use admissions.
fn backing_at<'a, L, C: ValueControl>(
    view: &ValueView<'a>,
    mappings: &'a mapped::Mappings<L>,
    input: &'a str,
    cursor: &mut ValuePosition,
    control: &mut C,
) -> Result<Backing<'a>, C::Error> {
    if cursor.position < view.prefix.len() {
        return Ok(Backing {
            text: view.prefix,
            start: cursor.position,
            available: view.prefix.len() - cursor.position,
        });
    }
    let body_position = cursor
        .position
        .checked_sub(view.prefix.len())
        .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
    if body_position == view.body_len && view.terminal {
        return Ok(Backing {
            text: "\n",
            start: 0,
            available: 1,
        });
    }
    if body_position >= view.body_len {
        return Err(control.reject(NativeViolation::InvalidProvenance));
    }
    loop {
        control.work(WorkDelta::Events(1))?;
        control.work(WorkDelta::Scan(std::mem::size_of::<mapped::Atom>() as u64))?;
        let atom = view
            .map
            .atoms
            .get(cursor.atom)
            .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
        if atom.bytes == 0 {
            cursor.atom = cursor
                .atom
                .checked_add(1)
                .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
            continue;
        }
        if cursor.offset >= atom.bytes || atom.hidden_syntax {
            return Err(control.reject(NativeViolation::InvalidProvenance));
        }
        let available = (atom.bytes - cursor.offset).min(view.body_len - body_position);
        if let Some(id) = atom.proof {
            let proof = mappings
                .proofs
                .get(id, |n| control.record(n))?
                .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
            if proof.result.len() != atom.bytes {
                return Err(control.reject(NativeViolation::InvalidProvenance));
            }
            return Ok(Backing {
                text: &proof.result,
                start: cursor.offset,
                available,
            });
        }
        if atom.source.end.checked_sub(atom.source.start) != Some(atom.bytes) {
            return Err(control.reject(NativeViolation::InvalidProvenance));
        }
        let r = atom
            .source
            .start
            .checked_add(cursor.offset)
            .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
        let buffer = mappings
            .buffers
            .get(atom.source.buffer.get() as usize - 1, |n| control.record(n))?
            .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
        let mut index = match cursor.piece {
            Some((id, i))
                if id == atom.source.buffer
                    && buffer.pieces.get(i).is_some_and(|p| p.r.start <= r) =>
            {
                i
            }
            _ => {
                let mut lo = 0;
                let mut hi = buffer.pieces.len();
                while lo < hi {
                    control.work(WorkDelta::Events(1))?;
                    control.work(WorkDelta::Scan(
                        std::mem::size_of::<mapped::BufferPiece>() as u64
                    ))?;
                    let mid = lo + (hi - lo) / 2;
                    if buffer.pieces[mid].r.end <= r {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                lo
            }
        };
        while buffer.pieces.get(index).is_some_and(|p| p.r.end <= r) {
            control.work(WorkDelta::Events(1))?;
            control.work(WorkDelta::Scan(
                std::mem::size_of::<mapped::BufferPiece>() as u64
            ))?;
            index = index
                .checked_add(1)
                .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
        }
        let piece = buffer
            .pieces
            .get(index)
            .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
        if piece.r.start > r || r >= piece.r.end {
            return Err(control.reject(NativeViolation::InvalidProvenance));
        }
        let available = available.min(piece.r.end - r);
        cursor.piece = Some((atom.source.buffer, index));
        return match piece.kind {
            AppendKind::Copy => {
                if piece.r.end.checked_sub(piece.r.start) != piece.p.end.checked_sub(piece.p.start)
                {
                    return Err(control.reject(NativeViolation::InvalidProvenance));
                }
                let start = piece
                    .p
                    .start
                    .checked_add(r - piece.r.start)
                    .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
                if start
                    .checked_add(available)
                    .is_none_or(|end| end > piece.p.end || end > input.len())
                {
                    return Err(control.reject(NativeViolation::InvalidProvenance));
                }
                Ok(Backing {
                    text: input,
                    start,
                    available,
                })
            }
            AppendKind::PartialTab { spaces, .. }
                if usize::from(spaces) == piece.r.end - piece.r.start && spaces <= 4 =>
            {
                Ok(Backing {
                    text: "    ",
                    start: r - piece.r.start,
                    available,
                })
            }
            AppendKind::TableEscapedPipe
                if piece.r.end - piece.r.start == 1 && piece.p.end - piece.p.start == 2 =>
            {
                Ok(Backing {
                    text: "|",
                    start: 0,
                    available,
                })
            }
            _ => Err(control.reject(NativeViolation::InvalidProvenance)),
        };
    }
}

fn advance<C: ValueControl>(
    view: &ValueView<'_>,
    cursor: &mut ValuePosition,
    bytes: usize,
    control: &mut C,
) -> Result<(), C::Error> {
    if cursor.position >= view.prefix.len() && cursor.position - view.prefix.len() < view.body_len {
        let atom = view
            .map
            .atoms
            .get(cursor.atom)
            .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
        cursor.offset = cursor
            .offset
            .checked_add(bytes)
            .filter(|n| *n <= atom.bytes)
            .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
        if cursor.offset == atom.bytes {
            cursor.atom = cursor
                .atom
                .checked_add(1)
                .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
            cursor.offset = 0;
        }
    }
    cursor.position = cursor
        .position
        .checked_add(bytes)
        .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
    Ok(())
}

pub(super) fn checked_position<L, C: ValueControl>(
    view: &ValueView<'_>,
    mappings: &mapped::Mappings<L>,
    input: &str,
    at: usize,
    control: &mut C,
) -> Result<ValuePosition, C::Error> {
    let mut cursor = seek(view, at, control)?;
    if at != view.len(control)? {
        let backing = backing_at(view, mappings, input, &mut cursor, control)?;
        control.work(WorkDelta::Scan(1))?;
        if !backing.text.is_char_boundary(backing.start) {
            return Err(control.reject(NativeViolation::InvalidProvenance));
        }
    }
    Ok(cursor)
}

/// One actual backing per call. State has no reference, owner or allocation;
/// no prefix rescan or readonly string materialization occurs between chunks.
pub(super) fn next_backing<'a, L, C: ValueControl>(
    view: &ValueView<'a>,
    mappings: &'a mapped::Mappings<L>,
    input: &'a str,
    cursor: &mut ValuePosition,
    upper: usize,
    maximum: usize,
    control: &mut C,
) -> Result<Option<&'a str>, C::Error> {
    control.work(WorkDelta::Events(1))?;
    if cursor.position > upper || upper > view.len(control)? || !(4..=65_536).contains(&maximum) {
        return Err(control.reject(NativeViolation::InvalidProvenance));
    }
    if cursor.position == upper {
        return Ok(None);
    }
    let backing = backing_at(view, mappings, input, cursor, control)?;
    let text = bounded_slice(
        backing.text,
        backing.start,
        backing.available.min(upper - cursor.position).min(maximum),
        control,
    )?;
    advance(view, cursor, text.len(), control)?;
    Ok(Some(text))
}
impl ValueView<'_> {
    pub(super) fn len<C: ValueControl>(&self, control: &mut C) -> Result<usize, C::Error> {
        self.prefix
            .len()
            .checked_add(self.body_len)
            .and_then(|n| n.checked_add(usize::from(self.terminal)))
            .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))
    }
}

pub(super) fn accepted_view<'a, L, C: ValueControl>(
    value: AcceptedValueRef,
    mappings: &'a mapped::Mappings<L>,
    control: &mut C,
) -> Result<ValueView<'a>, C::Error> {
    control.work(WorkDelta::Events(1))?;
    match value {
        AcceptedValueRef::Payload { payload, role } => {
            let row = mappings
                .payloads
                .get(payload.get() as usize - 1, |n| control.record(n))?
                .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
            if row.role != role {
                return Err(control.reject(NativeViolation::InvalidProvenance));
            }
            Ok(ValueView {
                map: &row.value,
                prefix: row.prefix.as_ref().map_or("", |(_, s)| s.as_str()),
                body_len: row.value.bytes,
                terminal: false,
            })
        }
        AcceptedValueRef::Node { node, role } => {
            let row = mappings
                .nodes
                .get(node.get() as usize - 1, |n| control.record(n))?
                .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
            let id = row
                .value
                .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
            let map = mappings
                .values
                .get(id.get() as usize - 1, |n| control.record(n))?
                .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
            let mut body_len = map.bytes;
            let mut terminal = false;
            if role == NodeValueRole::BlockCode
                && let Some(id) = row.terminal
            {
                let &(buffer, at, eof) = mappings
                    .terminals
                    .get(id.get() as usize - 1, |n| control.record(n))?
                    .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
                terminal = true;
                if eof.is_none() {
                    let last = map
                        .atoms
                        .last()
                        .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
                    control.work(WorkDelta::Scan(std::mem::size_of::<mapped::Atom>() as u64))?;
                    if last.proof.is_some()
                        || last.hidden_syntax
                        || last.bytes != 1
                        || last.source.buffer != buffer
                        || last.source.start != at
                        || at.checked_add(1) != Some(last.source.end)
                    {
                        return Err(control.reject(NativeViolation::InvalidProvenance));
                    }
                    body_len = body_len
                        .checked_sub(1)
                        .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
                }
            }
            Ok(ValueView {
                map,
                prefix: "",
                body_len,
                terminal,
            })
        }
    }
}

impl<H: Host> Context<'_, H> {
    /// Grammar-only transport for the strict G4 email selector. This is not a
    /// readonly materializer: only this actual Text slot in Grammar is allowed.
    /// No-@ values allocate nothing; a matching necessary condition permits one
    /// temporary D-run which dies before the parser's Context and ledger.
    pub(crate) fn g4_email_text(
        &mut self,
        node: NativeNodeId,
        text: &ValueText,
    ) -> Result<Option<String>, H::Error> {
        self.require_grammar()?;
        let expected = AcceptedValueRef::Node {
            node,
            role: NodeValueRole::Text,
        };
        if !self.direct
            || self.is_shadow()
            || self.inline_policy != super::InlinePolicy::StrictG4
            || !matches!(text.repr, ValueStorage::Accepted(id) if id == expected)
        {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        let input = self
            .input_text
            .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
        let mappings = &self.mappings;
        let mut mutation = super::DirectMutation {
            phase: &mut self.final_phase,
            committed: false,
        };
        let mut access = super::MappingAccess {
            host: self.host,
            registry: &mut self.registry,
            input_len: self.input_len,
            inline_policy: self.inline_policy,
        };
        let result = (|| {
            let (view, len) = {
                let mut control = GrammarControl(&mut *access.host);
                let view = accepted_view(expected, mappings, &mut control)?;
                let len = view.len(&mut control)?;
                (view, len)
            };
            let mut position = ValuePosition::default();
            let mut has_at = false;
            loop {
                let piece = {
                    let mut control = GrammarControl(&mut *access.host);
                    next_backing(
                        &view,
                        mappings,
                        input,
                        &mut position,
                        len,
                        65_536,
                        &mut control,
                    )?
                };
                let Some(piece) = piece else { break };
                access.scan(piece.len())?;
                if piece.as_bytes().contains(&b'@') {
                    has_at = true;
                    break;
                }
            }
            if !has_at {
                return Ok(None);
            }
            // Each real backing is borrowed only until this charged copy. The
            // original map and AST identity stay in place until email_atoms.
            let mut output = String::new();
            let mut position = ValuePosition::default();
            loop {
                let piece = {
                    let mut control = GrammarControl(&mut *access.host);
                    next_backing(
                        &view,
                        mappings,
                        input,
                        &mut position,
                        len,
                        65_536,
                        &mut control,
                    )?
                };
                let Some(piece) = piece else { break };
                access.append(&mut output, piece)?;
            }
            Ok(Some(output))
        })();
        mutation.committed = result.is_ok();
        result
    }
    pub(crate) fn empty_value(&self) -> ValueText {
        if self.direct {
            ValueText::empty()
        } else {
            ValueText::default()
        }
    }
    pub(crate) fn value_len(&mut self, text: &ValueText) -> Result<usize, H::Error> {
        self.require_grammar()?;
        self.event()?;
        match &text.repr {
            ValueStorage::Owned(value) if !self.direct => Ok(value.len()),
            ValueStorage::Accepted(value) if self.direct => {
                let mut control = GrammarControl(&mut *self.host);
                accepted_view(*value, &self.mappings, &mut control)?.len(&mut control)
            }
            ValueStorage::Empty => Ok(0),
            _ => Err(self.host.reject(NativeViolation::InvalidProvenance)),
        }
    }
    pub(crate) fn value_is_empty(&mut self, text: &ValueText) -> Result<bool, H::Error> {
        self.value_len(text).map(|n| n == 0)
    }
    pub(crate) fn value_first_byte(&mut self, text: &ValueText) -> Result<Option<u8>, H::Error> {
        self.require_grammar()?;
        self.event()?;
        match &text.repr {
            ValueStorage::Owned(value) if !self.direct => {
                self.scan(usize::from(!value.is_empty()))?;
                Ok(value.as_bytes().first().copied())
            }
            ValueStorage::Empty => Ok(None),
            ValueStorage::Accepted(value) if self.direct => {
                let mut control = GrammarControl(&mut *self.host);
                let view = accepted_view(*value, &self.mappings, &mut control)?;
                let input = self
                    .input_text
                    .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
                let len = view.len(&mut control)?;
                let mut cursor = ValuePosition::default();
                Ok(next_backing(
                    &view,
                    &self.mappings,
                    input,
                    &mut cursor,
                    len,
                    4,
                    &mut control,
                )?
                .and_then(|text| text.as_bytes().first().copied()))
            }
            _ => Err(self.host.reject(NativeViolation::InvalidProvenance)),
        }
    }
    pub(crate) fn discard_value(&mut self, text: ValueText) -> Result<(), H::Error> {
        match text.repr {
            ValueStorage::Owned(Cow::Owned(value)) => self.discard_string(value),
            ValueStorage::Owned(Cow::Borrowed(_))
            | ValueStorage::Accepted(_)
            | ValueStorage::Empty => Ok(()),
            ValueStorage::Pending => Err(self.host.reject(NativeViolation::InvalidProvenance)),
        }
    }
    pub(crate) fn share_reference_value(
        &mut self,
        value: &ValueText,
    ) -> Result<ValueText, H::Error> {
        self.require_grammar()?;
        let ValueStorage::Accepted(id) = value.repr else {
            if matches!(value.repr, ValueStorage::Empty) {
                return Ok(ValueText::empty());
            }
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        };
        let mut control = GrammarControl(&mut *self.host);
        let view = accepted_view(id, &self.mappings, &mut control)?;
        let input = self
            .input_text
            .ok_or_else(|| control.reject(NativeViolation::InvalidProvenance))?;
        let len = view.len(&mut control)?;
        let mut cursor = ValuePosition::default();
        while cursor.position < len {
            let start = cursor.position;
            loop {
                let produced = cursor.position - start;
                if produced == 65_536 || cursor.position == len {
                    break;
                }
                let backing = backing_at(&view, &self.mappings, input, &mut cursor, &mut control)?;
                if !backing.text.is_char_boundary(backing.start) {
                    return Err(control.reject(NativeViolation::InvalidProvenance));
                }
                let mut end = backing
                    .start
                    .checked_add(backing.available.min(65_536 - produced))
                    .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
                while !backing.text.is_char_boundary(end) {
                    control.work(WorkDelta::Scan(1))?;
                    end = end
                        .checked_sub(1)
                        .ok_or_else(|| control.reject(NativeViolation::ArithmeticOverflow))?;
                }
                let count = end - backing.start;
                if count == 0 {
                    // Only the logical admission boundary can cut a scalar.
                    // A piece that cannot supply its first scalar is invalid.
                    if backing.available <= 65_536 - produced {
                        return Err(control.reject(NativeViolation::InvalidProvenance));
                    }
                    break;
                }
                control.work(WorkDelta::Scan(count as u64))?;
                advance(&view, &mut cursor, count, &mut control)?;
            }
            let produced = cursor.position - start;
            if produced == 0 {
                return Err(control.reject(NativeViolation::InvalidProvenance));
            }
            control
                .0
                .admit(super::NativeAdmission::DecodedBytes(produced as u64))?;
        }
        control.work(WorkDelta::Copy(
            std::mem::size_of::<AcceptedValueRef>() as u64
        ))?;
        Ok(ValueText::accepted(id))
    }
    pub(crate) fn finish_node_value(
        &mut self,
        value: ValueText,
        node: Option<NativeNodeId>,
        role: NodeValueRole,
    ) -> Result<ValueText, H::Error> {
        self.require_grammar()?;
        if !self.direct {
            return Ok(value);
        }
        let node = node.ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
        let accepted = AcceptedValueRef::Node { node, role };
        {
            let mut control = GrammarControl(&mut *self.host);
            accepted_view(accepted, &self.mappings, &mut control)?;
        }
        match value.repr {
            ValueStorage::Pending => {}
            ValueStorage::Accepted(previous) if previous == accepted => {}
            ValueStorage::Accepted(_) | ValueStorage::Empty => {
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            _ => self.discard_value(value)?,
        }
        Ok(ValueText::accepted(accepted))
    }
    pub(crate) fn finish_payload_value(
        &mut self,
        value: String,
        payload: Option<PayloadId>,
        role: PayloadKind,
    ) -> Result<ValueText, H::Error> {
        self.require_grammar()?;
        if !self.direct {
            return Ok(value.into());
        }
        let result = if let Some(payload) = payload {
            let accepted = AcceptedValueRef::Payload { payload, role };
            let mut control = GrammarControl(&mut *self.host);
            if accepted_view(accepted, &self.mappings, &mut control)?.len(&mut control)?
                != value.len()
            {
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            ValueText::accepted(accepted)
        } else if value.is_empty() {
            ValueText::empty()
        } else {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        };
        self.discard_string(value)?;
        Ok(result)
    }
}
