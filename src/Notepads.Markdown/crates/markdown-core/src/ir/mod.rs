// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One final semantic representation. A draft has no model query authority.

#[cfg(test)]
pub(crate) mod construction;
pub(crate) mod direct;
mod direct_rows;
mod entities;
#[cfg(test)]
mod manual;
mod node_rows;
#[cfg(test)]
mod read_tests;
mod replay;
mod row_view;
mod segmented_text;
mod storage;
pub use row_view::RowView;
pub(crate) use storage::owned_row;
#[cfg(test)]
mod tests;
mod types;
pub(crate) mod validate;
pub(crate) mod validation_output;
#[cfg(test)]
pub(crate) use manual::IrBuilder;
pub use types::*;

use crate::error::CoreError;
use crate::identity::SourceSpan;
use crate::policy::OwnedWork;
use crate::policy::{Operation, SourceBudget};
#[cfg(test)]
use storage::DraftIr;
use storage::{ModelRows, ModelText};

pub(crate) const INLINE_HTML_BREAK_BYTES: std::ops::RangeInclusive<usize> = 4..=34;
pub(crate) const EMPTY_TOC_HEADING: &str = "Untitled heading";

pub(crate) fn is_inline_html_break(raw: &[u8]) -> bool {
    if !INLINE_HTML_BREAK_BYTES.contains(&raw.len())
        || raw[0] != b'<'
        || !raw[1..3].eq_ignore_ascii_case(b"br")
        || raw[raw.len() - 1] != b'>'
    {
        return false;
    }
    let tail = &raw[3..raw.len() - 1];
    tail.strip_suffix(b"/")
        .unwrap_or(tail)
        .iter()
        .all(|byte| matches!(byte, b' ' | b'\t'))
}

pub struct CompactIr {
    pub(crate) budget: SourceBudget,
    pub(crate) nodes: ModelRows<NodeRow>,
    pub(crate) flows: ModelRows<FlowRow>,
    pub(crate) packets: ModelRows<AtomPacket>,
    pub(crate) extra_authorities: ModelRows<ExtraAuthority>,
    pub(crate) details: ModelRows<DetailRow>,
    pub(crate) objects: ModelRows<ObjectRow>,
    pub(crate) payloads: ModelRows<PayloadRow>,
    pub(crate) definitions: ModelRows<DefinitionRow>,
    pub(crate) references: ModelRows<ReferenceUseRow>,
    pub(crate) proofs: ModelRows<ProofRow>,
    pub(crate) primitives: ModelRows<PrimitiveRow>,
    pub(crate) stages: ModelRows<StageRow>,
    pub(crate) attributes: ModelRows<AttributeRow>,
    pub(crate) sources: ModelRows<SourceSpan>,
    pub(crate) alignments: ModelRows<Alignment>,
    pub(crate) text: ModelText,
}

macro_rules! getters {
    ($($name:ident: $ty:ty),+ $(,)?) => {$(
        pub fn $name(&self) -> RowView<'_, $ty> { self.$name.view() }
    )+};
}
impl CompactIr {
    getters!(nodes: NodeRow, flows: FlowRow, details: DetailRow,
        objects: ObjectRow, payloads: PayloadRow, definitions: DefinitionRow,
        references: ReferenceUseRow, proofs: ProofRow, primitives: PrimitiveRow,
        stages: StageRow, attributes: AttributeRow, sources: SourceSpan,
        alignments: Alignment);

    pub(crate) fn first_stage(&self, atom: AtomRow) -> Option<StageRow> {
        let proof = self.proofs().get(atom.proof?.index())?;
        if proof.stages.is_empty() {
            return None;
        }
        self.stages().get(proof.stages.start()).copied()
    }

    pub fn atoms_len(&self) -> usize {
        self.packets.len()
    }
    pub fn extra_authorities_len(&self) -> usize {
        self.extra_authorities.len()
    }

    pub fn atom_at(&self, index: usize, op: &mut Operation) -> Result<AtomRow, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        let packet = self
            .packets
            .view()
            .get(index)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        op.view_scan(size_of::<AtomFields>() as u64 + types::AUTHORITY_SELECTOR_BYTES)?;
        op.view_copy(size_of::<AtomRow>() as u64)?;
        let claim = match &packet.authority {
            AuthorityBinding::Owned(_) => {
                ClaimId::primary(AtomId::from_index(index).map_err(|e| op.fail(e))?)
            }
            AuthorityBinding::Shared(id) => {
                op.view_scan(size_of::<ClaimId>() as u64)?;
                *id
            }
        };
        let atom = &packet.atom;
        Ok(AtomRow {
            owner: atom.owner,
            kind: atom.kind,
            target: atom.target,
            sources: atom.sources,
            proof: atom.proof,
            content: atom.content,
            claim,
        })
    }

    pub fn authority(&self, id: ClaimId, op: &mut Operation) -> Result<AuthoritySpec, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        op.view_copy(size_of::<AuthoritySpec>() as u64)?;
        match id.storage() {
            types::ClaimStorage::Extra(id) => {
                op.view_scan(size_of::<ExtraAuthority>() as u64)?;
                self.extra_authorities
                    .view()
                    .get(id.index())
                    .map(|row| row.value)
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))
            }
            types::ClaimStorage::Primary(id) => {
                let packet = self
                    .packets
                    .view()
                    .get(id.index())
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                op.view_scan(types::AUTHORITY_SELECTOR_BYTES)?;
                let AuthorityBinding::Owned(primary) = &packet.authority else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                op.view_scan(
                    (size_of::<PrimaryAuthority>()
                        + size_of::<NodeId>()
                        + size_of::<Rows<SourceSpan>>()) as u64,
                )?;
                Ok(AuthoritySpec {
                    owner: packet.atom.owner,
                    sources: packet.atom.sources,
                    phase: primary.phase,
                    kind: primary.kind,
                    consumed: primary.consumed,
                    host: HostRow {
                        owner: packet.atom.owner,
                        role: primary.host_role,
                        utf8_len: primary.host_utf8_len,
                        domains: primary.host_domains,
                    },
                })
            }
        }
    }

    pub(crate) fn read_atom_at(
        &self,
        index: usize,
        op: &mut Operation,
    ) -> Result<AtomRow, CoreError> {
        self.atom_at(index, op)
    }

    #[cfg(test)]
    pub(crate) fn read_authority(
        &self,
        id: ClaimId,
        op: &mut Operation,
    ) -> Result<AuthoritySpec, CoreError> {
        self.authority(id, op)
    }

    #[cfg(test)]
    pub(crate) fn atom_target_end(
        &self,
        index: usize,
        op: &mut Operation,
    ) -> Result<u64, CoreError> {
        self.atom_at(index, op).map(|row| row.target.end)
    }

    pub(crate) fn owned_atom_at(
        &self,
        index: usize,
        op: &mut OwnedWork<'_, '_>,
    ) -> Result<AtomRow, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        let packet = self
            .packets
            .view()
            .get(index)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        op.view_scan(size_of::<AtomFields>() as u64 + types::AUTHORITY_SELECTOR_BYTES)?;
        op.view_copy(size_of::<AtomRow>() as u64)?;
        let claim = match &packet.authority {
            AuthorityBinding::Owned(_) => {
                ClaimId::primary(AtomId::from_index(index).map_err(|e| op.fail(e))?)
            }
            AuthorityBinding::Shared(id) => {
                op.view_scan(size_of::<ClaimId>() as u64)?;
                *id
            }
        };
        let atom = &packet.atom;
        Ok(AtomRow {
            owner: atom.owner,
            kind: atom.kind,
            target: atom.target,
            sources: atom.sources,
            proof: atom.proof,
            content: atom.content,
            claim,
        })
    }

    pub(crate) fn owned_authority(
        &self,
        id: ClaimId,
        op: &mut OwnedWork<'_, '_>,
    ) -> Result<AuthoritySpec, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.events(1)?;
        op.view_copy(size_of::<AuthoritySpec>() as u64)?;
        match id.storage() {
            types::ClaimStorage::Extra(id) => {
                op.view_scan(size_of::<ExtraAuthority>() as u64)?;
                self.extra_authorities
                    .view()
                    .get(id.index())
                    .map(|row| row.value)
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))
            }
            types::ClaimStorage::Primary(id) => {
                let packet = self
                    .packets
                    .view()
                    .get(id.index())
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                op.view_scan(types::AUTHORITY_SELECTOR_BYTES)?;
                let AuthorityBinding::Owned(primary) = &packet.authority else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                op.view_scan(
                    (size_of::<PrimaryAuthority>()
                        + size_of::<NodeId>()
                        + size_of::<Rows<SourceSpan>>()) as u64,
                )?;
                Ok(AuthoritySpec {
                    owner: packet.atom.owner,
                    sources: packet.atom.sources,
                    phase: primary.phase,
                    kind: primary.kind,
                    consumed: primary.consumed,
                    host: HostRow {
                        owner: packet.atom.owner,
                        role: primary.host_role,
                        utf8_len: primary.host_utf8_len,
                        domains: primary.host_domains,
                    },
                })
            }
        }
    }

    pub(crate) fn owned_read_atom_at(
        &self,
        index: usize,
        op: &mut OwnedWork<'_, '_>,
    ) -> Result<AtomRow, CoreError> {
        self.owned_atom_at(index, op)
    }

    pub(crate) fn owned_read_authority(
        &self,
        id: ClaimId,
        op: &mut OwnedWork<'_, '_>,
    ) -> Result<AuthoritySpec, CoreError> {
        self.owned_authority(id, op)
    }

    pub(crate) fn owned_atom_target_end(
        &self,
        index: usize,
        op: &mut OwnedWork<'_, '_>,
    ) -> Result<u64, CoreError> {
        self.owned_atom_at(index, op).map(|row| row.target.end)
    }

    pub(crate) fn text_owned(
        &self,
        value: TextSlice,
        work: &mut OwnedWork<'_, '_>,
    ) -> Result<&str, CoreError> {
        work.ensure_budget(&self.budget)?;
        self.text
            .text_charged(value, &mut segmented_text::TextWork::Build(work))
    }
    pub(crate) fn text_query(
        &self,
        value: TextSlice,
        work: &mut Operation,
    ) -> Result<&str, CoreError> {
        work.ensure_budget(&self.budget)?;
        self.text
            .text_charged(value, &mut segmented_text::TextWork::Query(work))
    }
    pub fn text(&self, value: TextSlice) -> Result<&str, CoreError> {
        self.text.text(value)
    }
}

// Actual bounded metadata payload sizes.
const _: () = assert!(size_of::<types::AtomPacket>() <= 512);
const _: () = assert!(size_of::<types::ExtraAuthority>() <= 512);

pub(crate) fn inline_html_tag_space(byte: u8) -> bool {
    // Comrak's HTML tag scanner uses space, tab, VT, form feed, CR and LF.
    matches!(byte, b' ' | b'\t' | b'\x0b' | b'\x0c' | b'\r' | b'\n')
}

pub(crate) fn inline_tag_prefix(raw: &[u8]) -> Option<(&[u8], bool, &[u8])> {
    let raw = raw.strip_prefix(b"<")?;
    let (closing, raw) = match raw.strip_prefix(b"/") {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    let end = raw
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric() && *byte != b'-')?;
    let (name, tail) = raw.split_at(end);
    if !matches!(tail[0], b'>' | b'/') && !inline_html_tag_space(tail[0]) {
        return None;
    }
    Some((name, closing, tail))
}

pub(crate) fn inline_anchor_attributes(raw: &[u8]) -> Option<[Option<&[u8]>; 2]> {
    let (tag, closing, tail) = inline_tag_prefix(raw)?;
    if closing
        || !tail
            .first()
            .is_some_and(|byte| inline_html_tag_space(*byte))
    {
        return None;
    }
    let mut tail = tail.strip_suffix(b">")?;
    let mut names = [None; 2];
    while !tail.is_empty() {
        let skipped = tail
            .iter()
            .take_while(|byte| inline_html_tag_space(**byte))
            .count();
        if skipped == 0 {
            return None;
        }
        tail = &tail[skipped..];
        if tail.is_empty() {
            break;
        }
        let end = tail
            .iter()
            .position(|byte| *byte == b'=' || inline_html_tag_space(*byte))?;
        let attribute = &tail[..end];
        let index = if attribute.eq_ignore_ascii_case(b"id") {
            0
        } else if tag.eq_ignore_ascii_case(b"a") && attribute.eq_ignore_ascii_case(b"name") {
            1
        } else {
            return None;
        };
        if names[index].is_some() {
            return None;
        }
        tail = &tail[end..];
        tail = &tail[tail
            .iter()
            .take_while(|byte| inline_html_tag_space(**byte))
            .count()..];
        tail = tail.strip_prefix(b"=")?;
        tail = &tail[tail
            .iter()
            .take_while(|byte| inline_html_tag_space(**byte))
            .count()..];
        let first = *tail.first()?;
        let (value, rest) = if matches!(first, b'\'' | b'"') {
            let end = tail[1..].iter().position(|byte| *byte == first)? + 1;
            (&tail[1..end], &tail[end + 1..])
        } else {
            let end = tail
                .iter()
                .position(|byte| inline_html_tag_space(*byte))
                .unwrap_or(tail.len());
            (&tail[..end], &tail[end..])
        };
        if value.is_empty() || value.contains(&0) {
            return None;
        }
        names[index] = Some(value);
        tail = rest;
    }
    names.iter().any(Option::is_some).then_some(names)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScriptKind {
    Superscript,
    Subscript,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HtmlEmphasis {
    name: u8,
    pub(crate) closing: bool,
}

impl HtmlEmphasis {
    // The one-based tag ID occupies the low five bits of the delimiter parameter.
    const NAMES: [&[u8]; 17] = [
        b"b", b"strong", b"i", b"em", b"del", b"s", b"strike", b"sup", b"sub", b"ins", b"var",
        b"cite", b"dfn", b"span", b"tt", b"a", b"mark",
    ];
    const CLOSING: u32 = 32;
    pub(crate) const MAX_DELIMITER_BYTES: usize = crate::policy::CHECKPOINT_BYTES as usize;

    pub(crate) const STYLE_COUNT: usize = 6;
    pub(crate) const MONOSPACE_STYLE: usize = 4;
    pub(crate) const HIGHLIGHT_STYLE: usize = 5;

    /// A recognized name is still significant when its attributes or spelling
    /// make the complete delimiter ineligible.
    pub(crate) fn prefix(raw: &[u8]) -> Option<(Self, &[u8])> {
        let (name, closing, tail) = inline_tag_prefix(raw)?;
        let name = Self::NAMES
            .iter()
            .position(|tag| tag.eq_ignore_ascii_case(name))? as u8;
        Some((Self { name, closing }, tail))
    }

    pub(crate) fn delimiter(raw: &[u8]) -> Option<Self> {
        if !(3..=Self::MAX_DELIMITER_BYTES).contains(&raw.len()) {
            return None;
        }
        let (tag, tail) = Self::prefix(raw)?;
        let plain = raw.len() <= 34
            && tail
                .strip_suffix(b">")?
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t'));
        (plain || (!tag.closing && inline_anchor_attributes(raw).is_some())).then_some(tag)
    }

    pub(crate) fn matches(self, closing: Self) -> bool {
        !self.closing && closing.closing && self.name == closing.name
    }

    pub(crate) fn parameter(self) -> u32 {
        u32::from(self.name) + 1 + if self.closing { Self::CLOSING } else { 0 }
    }

    pub(crate) fn from_parameter(parameter: u32) -> Option<Self> {
        let name = (parameter & !Self::CLOSING).checked_sub(1)?;
        (name < Self::NAMES.len() as u32).then_some(Self {
            name: name as u8,
            closing: parameter & Self::CLOSING != 0,
        })
    }

    pub(crate) fn style_index(self) -> Option<usize> {
        match self.name {
            0 | 1 => Some(0),
            2 | 3 | 10..=12 => Some(1),
            4..=6 => Some(2),
            9 => Some(3),
            14 => Some(Self::MONOSPACE_STYLE),
            16 => Some(Self::HIGHLIGHT_STYLE),
            _ => None,
        }
    }

    pub(crate) fn script_kind(self) -> Option<ScriptKind> {
        match self.name {
            7 => Some(ScriptKind::Superscript),
            8 => Some(ScriptKind::Subscript),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct HtmlCodeDelimiter {
    name: u8,
    pub(crate) closing: bool,
}

impl HtmlCodeDelimiter {
    const NAMES: [&[u8]; 3] = [b"code", b"kbd", b"samp"];

    pub(crate) fn parse(raw: &[u8]) -> Option<Self> {
        if !(5..=34).contains(&raw.len()) {
            return None;
        }
        let (name, closing, tail) = inline_tag_prefix(raw)?;
        let name = Self::NAMES
            .iter()
            .position(|tag| tag.eq_ignore_ascii_case(name))? as u8;
        tail.strip_suffix(b">")?
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'))
            .then_some(Self { name, closing })
    }

    pub(crate) fn matches(self, close: Self) -> bool {
        !self.closing && close.closing && self.name == close.name
    }

    pub(crate) fn has_code_line_height(self) -> bool {
        self.name == 0
    }

    pub(crate) fn parameter(self) -> u32 {
        u32::from(self.name) + 1 + if self.closing { 16 } else { 0 }
    }

    pub(crate) fn from_parameter(parameter: u32) -> Option<Self> {
        let name = (parameter & !16).checked_sub(1)?;
        (name < Self::NAMES.len() as u32).then_some(Self {
            name: name as u8,
            closing: parameter & 16 != 0,
        })
    }
}
