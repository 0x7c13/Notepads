// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One immutable, same-model catalog of headings and fragment targets.

use crate::error::{CoreError, LimitKind};
use crate::identity::{LogicalSpan, SourceSpan};
use crate::ir::{
    AtomContent, AtomKind, ClaimKind, DetailRow, FlowScope, NodeId, NodeKind, NodeRow, ObjectRow,
};
use crate::model::AcceptedModel;
use crate::policy::Operation;
use finl_unicode::categories::CharacterCategories;

mod lowercase;
mod storage;
use lowercase::Lowercase;
use storage::{PaidVec, copy, scan};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextRange {
    pub start: u32,
    pub len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Heading {
    pub node: NodeId,
    pub level: u32,
    pub logical: LogicalSpan,
    pub title: TextRange,
    pub anchor: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Anchor {
    pub position: u64,
    pub name: TextRange,
    pub legacy: bool,
    pub node: Option<NodeId>,
}

/// Buffers retire before the model that owns their original quota root.
pub struct Outline {
    headings: PaidVec<Heading>,
    anchors: PaidVec<Anchor>,
    blob: PaidVec<u8>,
    model: AcceptedModel,
}

impl Outline {
    pub fn build(model: AcceptedModel, op: &mut Operation) -> Result<Self, CoreError> {
        op.ensure_budget(model.budget())?;
        op.checkpoint_now()?;
        let mut count = 0_usize;
        let mut title_bytes = 0_usize;
        let mut max_title = 0_usize;
        let extensions = model.identity().config().extensions();
        let allow_ids = extensions.contains(crate::config::Extensions::HEADING_IDS);
        let inline_enabled = extensions.contains(crate::config::Extensions::HTML_INLINE);
        let mut probe_ids = false;
        let mut inline_count = 0;
        let mut anchor_bytes = 0;
        let limit = model
            .budget()
            .model_limits()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?
            .model_records;
        main_nodes(&model, op, |node, row, op| {
            if row.kind != NodeKind::Heading {
                if inline_enabled && let Some(names) = inline_anchors(&model, row, op)? {
                    for raw in names.into_iter().flatten() {
                        inline_count = add(inline_count, 1, op)?;
                        // Some HTML5 entities decode to more UTF-8 bytes than
                        // their spelling (for example, &nGt; is five to six).
                        let capacity = raw
                            .len()
                            .checked_mul(2)
                            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
                        anchor_bytes = add(anchor_bytes, capacity, op)?;
                    }
                }
                return Ok(());
            }
            count = add(count, 1, op)?;
            op.check_limit(LimitKind::ModelRecords, count as u64, limit)?;
            let mut sink = LabelSink::count();
            label(&model, node, &mut sink, op)?;
            title_bytes = add(title_bytes, sink.len, op)?;
            max_title = max_title.max(sink.len);
            if allow_ids && !probe_ids {
                let raw = source_text(&model, row.source_envelope, op)?;
                for start in (0..raw.len()).step_by(4096) {
                    let window = &raw.as_bytes()[start..(start + 4097).min(raw.len())];
                    op.scan(window.len() as u64)?;
                    if window.windows(2).any(|pair| pair == b"{#") {
                        probe_ids = true;
                        break;
                    }
                }
            }
            Ok(())
        })?;
        u32::try_from(count).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let explicit = model.html_anchors();
        let mut heading_ids = collect_heading_ids(&model, if probe_ids { count } else { 0 }, op)?;
        let anchor_count = add(explicit.len(), inline_count, op)?;
        let total = add(count, anchor_count, op)?;
        op.check_limit(LimitKind::ModelRecords, total as u64, limit)?;
        for anchor in explicit {
            op.events(1)?;
            anchor_bytes = add(anchor_bytes, anchor.name.len(), op)?;
        }
        for id in &heading_ids.values {
            anchor_bytes = add(anchor_bytes, id.name.len(), op)?;
        }
        // A lowercased title is <=2*title bytes for the pinned standard library;
        // suffix needs at most '-' plus 20 decimal u64 digits. Input label and
        // final anchor share this pool; temporary base keys use a separate pool.
        let blob_capacity = title_bytes
            .checked_mul(3)
            .and_then(|v| {
                count
                    .checked_mul(21)
                    .and_then(|suffix| v.checked_add(suffix))
            })
            .and_then(|length| length.checked_add(anchor_bytes))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        u32::try_from(blob_capacity).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let bases_capacity = title_bytes
            .checked_mul(2)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let candidate_capacity = max_title
            .checked_mul(2)
            .and_then(|v| v.checked_add(21))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let mut rows = PaidVec::new(count, op)?;
        let mut anchors = PaidVec::new(anchor_count, op)?;
        let mut blob = PaidVec::new(blob_capacity, op)?;
        let mut bases = PaidVec::new(bases_capacity, op)?;
        let mut candidate = PaidVec::new(if count == 0 { 0 } else { candidate_capacity }, op)?;
        let mut used = Table::new(total, op)?;
        let mut suffixes = Table::new(count, op)?;
        for anchor in explicit {
            let name = store_name(&mut blob, &mut used, &anchor.name, anchor.legacy, op)?;
            anchors.push(
                Anchor {
                    position: anchor.position,
                    name,
                    legacy: anchor.legacy,
                    node: anchor.node,
                },
                op,
            )?;
        }
        if inline_count > 0 {
            main_nodes(&model, op, |_, row, op| {
                if let Some(names) = inline_anchors(&model, row, op)? {
                    for (index, raw) in names.into_iter().enumerate() {
                        let Some(raw) = raw else { continue };
                        let raw = std::str::from_utf8(raw)
                            .map_err(|_| op.fail(CoreError::InvalidProvenance))?;
                        let legacy = index == 1;
                        let _scratch = op.reserve_boundary(raw.len() as u64 * 4)?;
                        let value = htmlize::unescape_attribute(raw);
                        if value.is_empty()
                            || value.contains('\0')
                            || (!legacy && value.bytes().any(crate::ir::inline_html_tag_space))
                        {
                            continue;
                        }
                        let name = store_name(&mut blob, &mut used, &value, legacy, op)?;
                        anchors.push(
                            Anchor {
                                position: row.logical.start(),
                                name,
                                legacy,
                                node: None,
                            },
                            op,
                        )?;
                    }
                }
                Ok(())
            })?;
        }
        for id in &mut heading_ids.values {
            id.range = store_name(&mut blob, &mut used, id.name, false, op)?;
        }
        let mut explicit_headings = heading_ids.values.iter().peekable();
        main_nodes(&model, op, |node, row, op| {
            if row.kind != NodeKind::Heading {
                return Ok(());
            }
            let title_start = blob.values.len();
            let mut sink = LabelSink::writing(&mut blob);
            label(&model, node, &mut sink, op)?;
            let has_sigma = sink.has_sigma;
            let title = range(title_start, sink.len, op)?;
            let anchor = if explicit_headings.peek().is_some_and(|id| id.node == node) {
                explicit_headings.next().unwrap().range
            } else if let Some(index) = html_heading_anchor(explicit, row.source_envelope, op)? {
                anchors.values[index].name
            } else {
                let title_text = text(&blob.values, title, op)?;
                let lower = Lowercase::new(title_text, has_sigma, op)?;
                let base_start = bases.values.len();
                let mut base_end = base_start;
                let mut has_content = false;
                for scalar in lower.text.chars() {
                    op.scan(scalar.len_utf8() as u64)?;
                    // Charge whitespace and the four possible Unicode property lookups.
                    op.events(10)?;
                    if (scalar == ' ' && has_content)
                        || scalar == '-'
                        || scalar.is_letter()
                        || scalar.is_mark()
                        || scalar.is_number()
                        || scalar.is_punctuation_connector()
                    {
                        let mut bytes = [0_u8; 4];
                        let value =
                            if scalar == ' ' { '-' } else { scalar }.encode_utf8(&mut bytes);
                        bases.append(value.as_bytes(), op)?;
                    }
                    if !scalar.is_whitespace() {
                        has_content = true;
                        base_end = bases.values.len();
                    }
                }
                // Trim the title's edge whitespace before assigning duplicate suffixes.
                // Punctuation still counts as content when deciding which spaces are interior.
                bases.values.truncate(base_end);
                drop(lower);
                let base = range(base_start, bases.values.len() - base_start, op)?;
                let base_bytes = bytes(&bases.values, base, op)?;
                let base_hash = hash(base_bytes, op)?;
                let (base_slot, exists) =
                    suffixes.find(&bases.values, base_bytes, base_hash, op)?;
                let mut suffix = if exists {
                    suffixes.slots.values[base_slot].next
                } else {
                    0
                };
                let (slot, candidate_hash) = loop {
                    // Only u8 values are discarded; capacity stays paid and reusable.
                    op.events(1)?;
                    candidate.values.clear();
                    candidate.append(base_bytes, op)?;
                    if suffix != 0 {
                        candidate.append(b"-", op)?;
                        let mut decimal = [0_u8; 20];
                        let mut offset = decimal.len();
                        let mut value = suffix;
                        while value != 0 {
                            op.events(1)?;
                            offset -= 1;
                            decimal[offset] = b'0' + (value % 10) as u8;
                            value /= 10;
                        }
                        candidate.append(&decimal[offset..], op)?;
                    }
                    let candidate_hash = hash(&candidate.values, op)?;
                    let (slot, taken) =
                        used.find(&blob.values, &candidate.values, candidate_hash, op)?;
                    if !taken {
                        break (slot, candidate_hash);
                    }
                    suffix = suffix
                        .checked_add(1)
                        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
                };
                let anchor = range(blob.values.len(), candidate.values.len(), op)?;
                blob.append(&candidate.values, op)?;
                used.put(slot, candidate_hash, anchor, 0, op)?;
                suffixes.put(
                    base_slot,
                    base_hash,
                    base,
                    suffix
                        .checked_add(1)
                        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?,
                    op,
                )?;
                anchor
            };
            let detail_id = row
                .detail
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            op.events(1)?;
            op.scan(size_of::<DetailRow>() as u64)?;
            let detail = model
                .ir()
                .details()
                .get(detail_id.index())
                .copied()
                .ok_or_else(|| op.fail(CoreError::UnknownId))?;
            let DetailRow::Heading { level, .. } = detail else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            if !(1..=6).contains(&level) {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            rows.push(
                Heading {
                    node,
                    level: u32::from(level),
                    logical: row.logical,
                    title,
                    anchor,
                },
                op,
            )?;
            Ok(())
        })?;
        if rows.values.len() != count || explicit_headings.next().is_some() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        drop(heading_ids);
        op.checkpoint_now()?;
        // All three construction-only containers and both tables die here.
        Ok(Self {
            headings: rows,
            anchors,
            blob,
            model,
        })
    }

    pub fn model(&self) -> &AcceptedModel {
        &self.model
    }
    pub fn headings(&self) -> &[Heading] {
        &self.headings.values
    }
    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors.values
    }
    pub fn blob(&self) -> &[u8] {
        &self.blob.values
    }
}

fn store_name(
    blob: &mut PaidVec<u8>,
    used: &mut Table,
    value: &str,
    legacy: bool,
    op: &mut Operation,
) -> Result<TextRange, CoreError> {
    let name = range(blob.values.len(), value.len(), op)?;
    let bytes = value.as_bytes();
    blob.append(bytes, op)?;
    if !legacy {
        let hash = hash(bytes, op)?;
        let (slot, exists) = used.find(&blob.values, bytes, hash, op)?;
        if !exists {
            used.put(slot, hash, name, 0, op)?;
        }
    }
    Ok(name)
}

fn inline_anchors<'a>(
    model: &'a AcceptedModel,
    row: NodeRow,
    op: &mut Operation,
) -> Result<Option<[Option<&'a [u8]>; 2]>, CoreError> {
    if row.kind != NodeKind::HtmlInline
        || !row.logical.is_empty()
        || row.source_envelope.len() > crate::ir::HtmlEmphasis::MAX_DELIMITER_BYTES as u64
    {
        return Ok(None);
    }
    let raw = source_text(model, row.source_envelope, op)?.as_bytes();
    scan(raw.len() as u64, op)?;
    Ok(crate::ir::inline_anchor_attributes(raw))
}

struct HeadingIdentifier<'a> {
    node: NodeId,
    name: &'a str,
    range: TextRange,
}

fn html_heading_anchor(
    anchors: &[crate::html::HtmlAnchor],
    heading: SourceSpan,
    op: &mut Operation,
) -> Result<Option<usize>, CoreError> {
    let Some(depth) = anchors.len().checked_ilog2() else {
        return Ok(None);
    };
    op.events(u64::from(depth) + 1)?;
    let start = anchors.partition_point(|anchor| anchor.source < heading.start());
    for (index, anchor) in anchors.iter().enumerate().skip(start) {
        op.events(1)?;
        if anchor.source >= heading.end() {
            break;
        }
        if anchor.is_heading() {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn source_text<'a>(
    model: &'a AcceptedModel,
    span: SourceSpan,
    op: &mut Operation,
) -> Result<&'a str, CoreError> {
    model
        .source()
        .text()
        .get(span.start() as usize..span.end() as usize)
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))
}

fn collect_heading_ids<'a>(
    model: &'a AcceptedModel,
    capacity: usize,
    op: &mut Operation,
) -> Result<PaidVec<HeadingIdentifier<'a>>, CoreError> {
    let mut ids = PaidVec::new(capacity, op)?;
    if capacity == 0 {
        return Ok(ids);
    }
    let ir = model.ir();
    let atoms = ir.flows()[model.main_flow_id().index()].atoms;
    let mut previous = None;
    for at in atoms.start()..atoms.start() + atoms.len() {
        let atom = ir.atom_at(at, op)?;
        if atom.kind != AtomKind::Hidden || previous == Some(atom.claim) {
            continue;
        }
        let claim = ir.authority(atom.claim, op)?;
        if claim.kind != ClaimKind::HeadingIdentifier {
            continue;
        }
        previous = Some(atom.claim);
        let spans = claim
            .sources
            .slice(ir.sources())
            .map_err(|error| op.fail(error))?;
        let span = SourceSpan::new(spans[0].start(), spans.last().unwrap().end())
            .map_err(|error| op.fail(error))?;
        let raw = source_text(model, span, op)?;
        scan(raw.len() as u64, op)?;
        let name = comrak::native::heading_identifier(raw)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        ids.push(
            HeadingIdentifier {
                node: claim.owner,
                name,
                range: TextRange::default(),
            },
            op,
        )?;
    }
    Ok(ids)
}

fn add(left: usize, right: usize, op: &mut Operation) -> Result<usize, CoreError> {
    left.checked_add(right)
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))
}

fn range(start: usize, len: usize, op: &mut Operation) -> Result<TextRange, CoreError> {
    u32::try_from(add(start, len, op)?).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
    Ok(TextRange {
        start: u32::try_from(start).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
        len: u32::try_from(len).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
    })
}

fn bytes<'a>(blob: &'a [u8], part: TextRange, op: &mut Operation) -> Result<&'a [u8], CoreError> {
    let start = part.start as usize;
    let end = add(start, part.len as usize, op)?;
    blob.get(start..end)
        .ok_or_else(|| op.fail(CoreError::InvalidRange))
}

fn text<'a>(blob: &'a [u8], part: TextRange, op: &mut Operation) -> Result<&'a str, CoreError> {
    let value = bytes(blob, part, op)?;
    scan(value.len() as u64, op)?;
    std::str::from_utf8(value).map_err(|_| op.fail(CoreError::InvalidBoundary))
}

fn read_node(model: &AcceptedModel, id: NodeId, op: &mut Operation) -> Result<NodeRow, CoreError> {
    op.events(1)?;
    op.scan(size_of::<NodeRow>() as u64)?;
    model
        .ir()
        .nodes()
        .get(id.index())
        .copied()
        .ok_or_else(|| op.fail(CoreError::UnknownId))
}

fn object(model: &AcceptedModel, row: NodeRow, op: &mut Operation) -> Result<ObjectRow, CoreError> {
    let atom_id = model.flow_atom_covering(row.flow, row.logical.start(), op)?;
    let atom = model.ir().atom_at(atom_id.index(), op)?;
    let AtomContent::Object(id) = atom.content else {
        return Err(op.fail(CoreError::InvalidProvenance));
    };
    op.events(1)?;
    op.scan(size_of::<ObjectRow>() as u64)?;
    model
        .ir()
        .objects()
        .get(id.index())
        .copied()
        .ok_or_else(|| op.fail(CoreError::UnknownId))
}

/// Iterative DFS. AltRoot has no IR parent; its validated ImageAlt object is
/// the sole bridge back to the image in the enclosing flow.
fn walk(
    model: &AcceptedModel,
    root: NodeId,
    alt: bool,
    op: &mut Operation,
    mut emit: impl FnMut(NodeId, NodeRow, &mut Operation) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    let mut current = root;
    let mut fuel = model
        .ir()
        .nodes()
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4))
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    loop {
        fuel = fuel
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let row = read_node(model, current, op)?;
        emit(current, row, op)?;
        let child = if alt && row.kind == NodeKind::Image {
            let ObjectRow::Image { alt, .. } = object(model, row, op)? else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            op.events(1)?;
            op.scan(size_of::<crate::ir::FlowRow>() as u64)?;
            Some(
                model
                    .ir()
                    .flows()
                    .get(alt.index())
                    .ok_or_else(|| op.fail(CoreError::UnknownId))?
                    .root,
            )
        } else {
            row.first_child
        };
        if let Some(child) = child {
            current = child;
            continue;
        }
        let mut cursor = current;
        loop {
            fuel = fuel
                .checked_sub(1)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            if cursor == root {
                return Ok(());
            }
            let row = read_node(model, cursor, op)?;
            if let Some(sibling) = row.next_sibling {
                current = sibling;
                break;
            }
            cursor = if let Some(parent) = row.parent {
                parent
            } else if alt && row.kind == NodeKind::AltRoot {
                op.events(1)?;
                op.scan(size_of::<crate::ir::FlowRow>() as u64)?;
                let flow = model
                    .ir()
                    .flows()
                    .get(row.flow.index())
                    .copied()
                    .ok_or_else(|| op.fail(CoreError::UnknownId))?;
                let FlowScope::ImageAlt { object } = flow.scope else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                op.scan(size_of::<ObjectRow>() as u64)?;
                let ObjectRow::Image { node, .. } = model
                    .ir()
                    .objects()
                    .get(object.index())
                    .copied()
                    .ok_or_else(|| op.fail(CoreError::UnknownId))?
                else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                node
            } else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
        }
    }
}

fn main_nodes(
    model: &AcceptedModel,
    op: &mut Operation,
    mut emit: impl FnMut(NodeId, NodeRow, &mut Operation) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    op.events(1)?;
    op.scan(size_of::<crate::ir::FlowRow>() as u64)?;
    let root = model
        .ir()
        .flows()
        .get(model.main_flow_id().index())
        .ok_or_else(|| op.fail(CoreError::UnknownId))?
        .root;
    walk(model, root, false, op, |id, row, op| {
        if row.flow == model.main_flow_id() {
            emit(id, row, op)?;
        }
        Ok(())
    })
}

struct LabelSink<'a> {
    data: Option<&'a mut PaidVec<u8>>,
    len: usize,
    has_sigma: bool,
}

impl<'a> LabelSink<'a> {
    fn count() -> Self {
        Self {
            data: None,
            len: 0,
            has_sigma: false,
        }
    }
    fn writing(data: &'a mut PaidVec<u8>) -> Self {
        Self {
            data: Some(data),
            len: 0,
            has_sigma: false,
        }
    }
    fn append(&mut self, value: &str, op: &mut Operation) -> Result<(), CoreError> {
        for scalar in value.chars() {
            op.scan(scalar.len_utf8() as u64)?;
            op.events(1)?;
            self.has_sigma |= scalar == 'Σ';
        }
        self.len = add(self.len, value.len(), op)?;
        if let Some(data) = &mut self.data {
            data.append(value.as_bytes(), op)?;
        }
        Ok(())
    }
}

fn label(
    model: &AcceptedModel,
    root: NodeId,
    sink: &mut LabelSink<'_>,
    op: &mut Operation,
) -> Result<(), CoreError> {
    walk(model, root, true, op, |_, row, op| {
        match row.kind {
            NodeKind::Text | NodeKind::InlineCode => {
                crate::reader::text::visit(model, row.flow, row.logical, op, |value, op| {
                    sink.append(value, op)
                })?
            }
            NodeKind::SoftBreak | NodeKind::HardBreak => sink.append(" ", op)?,
            NodeKind::Math => {
                let ObjectRow::Derived { input, .. } = object(model, row, op)? else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                op.events(1)?;
                op.scan(size_of::<crate::ir::PayloadRow>() as u64)?;
                let payload = model
                    .ir()
                    .payloads()
                    .get(input.index())
                    .copied()
                    .ok_or_else(|| op.fail(CoreError::UnknownId))?;
                sink.append(model.ir().text_query(payload.value, op)?, op)?;
            }
            _ => {}
        }
        Ok(())
    })
}

#[derive(Clone, Copy, Default)]
struct Slot {
    hash: u64,
    key: TextRange,
    next: u64,
    occupied: bool,
}

struct Table {
    slots: PaidVec<Slot>,
}

impl Table {
    fn new(headings: usize, op: &mut Operation) -> Result<Self, CoreError> {
        let capacity = if headings == 0 {
            0
        } else {
            headings
                .checked_mul(2)
                .and_then(usize::checked_next_power_of_two)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
        };
        let mut slots = PaidVec::new(capacity, op)?;
        for _ in 0..capacity {
            slots.push(Slot::default(), op)?;
        }
        Ok(Self { slots })
    }
    fn find(
        &self,
        blob: &[u8],
        value: &[u8],
        hash: u64,
        op: &mut Operation,
    ) -> Result<(usize, bool), CoreError> {
        let len = self.slots.values.len();
        if len == 0 {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let mut index = hash as usize & (len - 1);
        for _ in 0..len {
            op.scan(size_of::<Slot>() as u64)?;
            op.events(1)?;
            let slot = self.slots.values[index];
            if !slot.occupied {
                return Ok((index, false));
            }
            if slot.hash == hash && equal(bytes(blob, slot.key, op)?, value, op)? {
                return Ok((index, true));
            }
            index = (index + 1) & (len - 1);
        }
        Err(op.fail(CoreError::InternalInvariant))
    }
    fn put(
        &mut self,
        index: usize,
        hash: u64,
        key: TextRange,
        next: u64,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.events(1)?;
        copy(size_of::<Slot>() as u64, op)?;
        *self
            .slots
            .values
            .get_mut(index)
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))? = Slot {
            hash,
            key,
            next,
            occupied: true,
        };
        Ok(())
    }
}

fn equal(left: &[u8], right: &[u8], op: &mut Operation) -> Result<bool, CoreError> {
    op.events(1)?;
    if left.len() != right.len() {
        return Ok(false);
    }
    for (a, b) in left.chunks(32768).zip(right.chunks(32768)) {
        op.scan((a.len() + b.len()) as u64)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}

fn hash(value: &[u8], op: &mut Operation) -> Result<u64, CoreError> {
    let mut state = 0xcbf2_9ce4_8422_2325_u64;
    for chunk in value.chunks(256) {
        op.scan(chunk.len() as u64)?;
        op.events(chunk.len() as u64)?;
        for &byte in chunk {
            state = (state ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests;
