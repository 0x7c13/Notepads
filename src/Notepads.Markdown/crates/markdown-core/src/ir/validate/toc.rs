// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

struct Reading<'a, 'id> {
    ir: &'a CompactIr,
    source: &'a SourceSnapshot,
    targets: &'a BuildGrowingVec<'id, TargetIndex>,
    positives: &'a BuildGrowingVec<'id, AtomId>,
}

#[derive(Clone, Copy)]
struct Heading {
    node: NodeId,
    depth: u8,
}

impl BuildValue for Heading {}

pub(super) fn verify<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    main: FlowId,
    targets: &BuildGrowingVec<'id, TargetIndex>,
    positives: &BuildGrowingVec<'id, AtomId>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let reading = Reading {
        ir,
        source,
        targets,
        positives,
    };
    let limits = op.require_model_limits()?;
    let mut headings = BuildRows::new(limits.model_records, LimitKind::ModelRecords, op)?;
    let mut markers = BuildRows::new(limits.model_records, LimitKind::ModelRecords, op)?;
    let mut levels = [0; 6];
    let mut depth = 0;
    reading.walk(reading.flow(main, op)?.root, false, op, |id, row, op| {
        if row.kind == NodeKind::Heading {
            let DetailRow::Heading { level, .. } = reading.detail(row, op)? else {
                return Err(CoreError::InvalidProvenance);
            };
            require((1..=6).contains(&level))?;
            while depth != 0 && levels[depth - 1] >= level {
                depth -= 1;
            }
            headings.push(
                Heading {
                    node: id,
                    depth: depth as u8,
                },
                op,
            )?;
            levels[depth] = level;
            depth += 1;
        } else if row.kind == NodeKind::TableOfContents {
            markers.push(id, op)?;
        }
        Ok(row.kind != NodeKind::TableOfContents)
    })?;
    let mut label_bytes = 0u64;
    for index in 0..markers.len() {
        let marker = reading.node(markers.get(index, op)?, op)?;
        let mut entry = marker.first_child;
        for ordinal in 0..headings.len() {
            let expected = headings.get(ordinal, op)?;
            let id = entry.ok_or(CoreError::InvalidProvenance)?;
            let row = reading.node(id, op)?;
            require(row.kind == NodeKind::TableOfContentsEntry)?;
            let DetailRow::TableOfContentsEntry {
                heading,
                depth,
                claim,
            } = reading.detail(row, op)?
            else {
                return Err(CoreError::InvalidProvenance);
            };
            require(heading == expected.node && depth == expected.depth)?;
            let atom = reading.atom(reading.after(row.flow, row.logical.start(), op)?, op)?;
            require(
                atom.owner == id
                    && atom.claim == claim
                    && atom.target.start == row.logical.start()
                    && atom.target.end == row.logical.end(),
            )?;
            let label = reading.atom_text(atom, op)?;
            label_bytes = label_bytes
                .checked_add(label.len() as u64)
                .ok_or(CoreError::ArithmeticOverflow)?;
            op.check_limit(LimitKind::DecodedBytes, label_bytes, limits.decoded_bytes)?;
            reading.verify_label(heading, label.as_bytes(), op)?;
            entry = row.next_sibling;
        }
        require(entry.is_none())?;
    }
    Ok(())
}

impl<'a, 'id> Reading<'a, 'id> {
    fn node(&self, id: NodeId, op: &mut OwnedWork<'id, '_>) -> Result<NodeRow, CoreError> {
        op.events(1)?;
        op.scan(size_of::<NodeRow>() as u64)?;
        self.ir
            .nodes()
            .get(id.index())
            .copied()
            .ok_or(CoreError::InvalidProvenance)
    }

    fn detail(&self, row: NodeRow, op: &mut OwnedWork<'id, '_>) -> Result<DetailRow, CoreError> {
        op.scan(size_of::<DetailRow>() as u64)?;
        row.detail
            .and_then(|id| self.ir.details().get(id.index()))
            .copied()
            .ok_or(CoreError::InvalidProvenance)
    }

    fn flow(&self, id: FlowId, op: &mut OwnedWork<'id, '_>) -> Result<FlowRow, CoreError> {
        op.scan(size_of::<FlowRow>() as u64)?;
        self.ir
            .flows()
            .get(id.index())
            .copied()
            .ok_or(CoreError::InvalidProvenance)
    }

    fn object(&self, id: ObjectId, op: &mut OwnedWork<'id, '_>) -> Result<ObjectRow, CoreError> {
        op.scan(size_of::<ObjectRow>() as u64)?;
        self.ir
            .objects()
            .get(id.index())
            .copied()
            .ok_or(CoreError::InvalidProvenance)
    }

    fn after(
        &self,
        flow: FlowId,
        at: u64,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<usize, CoreError> {
        let positive = owned_row(self.targets, flow.index(), op)?.positives;
        positive_after(self.ir, positive, self.positives, at, op)?
            .ok_or(CoreError::InvalidProvenance)
    }

    fn atom(&self, index: usize, op: &mut OwnedWork<'id, '_>) -> Result<AtomRow, CoreError> {
        let id = owned_row(self.positives, index, op)?;
        self.ir.owned_read_atom_at(id.index(), op)
    }

    fn atom_text(&self, atom: AtomRow, op: &mut OwnedWork<'id, '_>) -> Result<&'a str, CoreError> {
        match (atom.kind, atom.content) {
            (AtomKind::Copy, AtomContent::Empty) => {
                source_text(self.source, atom.sources.slice(self.ir.sources())?[0])
            }
            (AtomKind::Transform | AtomKind::Synthetic, AtomContent::Pool(text)) => {
                self.ir.text_owned(text, op)
            }
            _ => Err(CoreError::InvalidProvenance),
        }
    }

    fn node_object(
        &self,
        row: NodeRow,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ObjectRow, CoreError> {
        let atom = self.atom(self.after(row.flow, row.logical.start(), op)?, op)?;
        let AtomContent::Object(id) = atom.content else {
            return Err(CoreError::InvalidProvenance);
        };
        self.object(id, op)
    }

    fn walk(
        &self,
        root: NodeId,
        alt: bool,
        op: &mut OwnedWork<'id, '_>,
        mut visit: impl FnMut(NodeId, NodeRow, &mut OwnedWork<'id, '_>) -> Result<bool, CoreError>,
    ) -> Result<(), CoreError> {
        let mut current = root;
        loop {
            let row = self.node(current, op)?;
            if visit(current, row, op)? {
                let child = if alt && row.kind == NodeKind::Image {
                    let ObjectRow::Image { alt, .. } = self.node_object(row, op)? else {
                        return Err(CoreError::InvalidProvenance);
                    };
                    Some(self.flow(alt, op)?.root)
                } else {
                    row.first_child
                };
                if let Some(child) = child {
                    current = child;
                    continue;
                }
            }
            loop {
                if current == root {
                    return Ok(());
                }
                let row = self.node(current, op)?;
                if let Some(sibling) = row.next_sibling {
                    current = sibling;
                    break;
                }
                current = if let Some(parent) = row.parent {
                    parent
                } else {
                    let FlowScope::ImageAlt { object } = self.flow(row.flow, op)?.scope else {
                        return Err(CoreError::InvalidProvenance);
                    };
                    let ObjectRow::Image { node, .. } = self.object(object, op)? else {
                        return Err(CoreError::InvalidProvenance);
                    };
                    node
                };
            }
        }
    }

    fn verify_label(
        &self,
        heading: NodeId,
        label: &[u8],
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let mut rest = label;
        let mut compare = |value: &str, op: &mut OwnedWork<'id, '_>| -> Result<(), CoreError> {
            for chunk in value.as_bytes().chunks(CHECKPOINT_BYTES as usize) {
                op.scan(chunk.len() as u64)?;
                rest = rest
                    .strip_prefix(chunk)
                    .ok_or(CoreError::InvalidProvenance)?;
            }
            Ok(())
        };
        self.walk(heading, true, op, |id, row, op| {
            match row.kind {
                NodeKind::Text | NodeKind::InlineCode if !row.logical.is_empty() => {
                    let mut index = self.after(row.flow, row.logical.start(), op)?;
                    let mut at = row.logical.start();
                    while at < row.logical.end() {
                        let atom = self.atom(index, op)?;
                        require(
                            atom.owner == id
                                && atom.target.start == at
                                && atom.target.end <= row.logical.end(),
                        )?;
                        compare(self.atom_text(atom, op)?, op)?;
                        at = atom.target.end;
                        index += 1;
                    }
                }
                NodeKind::SoftBreak | NodeKind::HardBreak => compare(" ", op)?,
                NodeKind::Math => {
                    let ObjectRow::Derived { input, .. } = self.node_object(row, op)? else {
                        return Err(CoreError::InvalidProvenance);
                    };
                    op.scan(size_of::<PayloadRow>() as u64)?;
                    let payload = self
                        .ir
                        .payloads()
                        .get(input.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    compare(self.ir.text_owned(payload.value, op)?, op)?;
                }
                _ => {}
            }
            Ok(true)
        })?;
        if rest.len() == label.len() {
            op.scan(EMPTY_TOC_HEADING.len() as u64)?;
            require(label == EMPTY_TOC_HEADING.as_bytes())
        } else {
            require(rest.is_empty())
        }
    }
}
