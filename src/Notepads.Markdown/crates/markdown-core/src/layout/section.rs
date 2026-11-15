// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::storage::LayoutRows;
use crate::error::CoreError;
use crate::ir::{NodeId, NodeRow};
use crate::model::AcceptedModel;
use crate::outline::Outline;
use crate::policy::Operation;

const EXCLUDED: u8 = 0;
const ANCESTOR: u8 = 1;
const BODY: u8 = 2;

pub(super) struct Section {
    pub(super) heading: NodeId,
    members: LayoutRows<u8>,
}

impl Section {
    pub(super) fn new(
        model: &AcceptedModel,
        first: NodeId,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        let outline = Outline::build(model.clone(), op)?;
        let mut headings = outline.headings().iter();
        let level = loop {
            op.events(1)?;
            let heading = headings
                .next()
                .ok_or_else(|| op.fail(CoreError::UnknownId))?;
            if heading.node == first {
                break heading.level;
            }
        };
        let mut stop = None;
        for heading in headings {
            op.events(1)?;
            if heading.level <= level {
                stop = Some(heading.node);
                break;
            }
        }
        drop(outline);
        let nodes = model.ir().nodes();
        let mut members = LayoutRows::new(nodes.len(), op)?;
        for _ in 0..nodes.len() {
            members.push(EXCLUDED, op)?;
        }
        let row = |node: NodeId, op: &mut Operation| -> Result<NodeRow, CoreError> {
            op.events(1)?;
            op.scan(size_of::<NodeRow>() as u64)?;
            nodes
                .get(node.index())
                .copied()
                .ok_or_else(|| op.fail(CoreError::UnknownId))
        };
        let mut ancestor = row(first, op)?.parent;
        while let Some(node) = ancestor {
            op.copy(1)?;
            members.values[node.index()] = ANCESTOR;
            ancestor = row(node, op)?.parent;
        }
        let mut current = first;
        loop {
            if Some(current) == stop {
                // Containers entered solely to reach the stopping heading have no excerpt content.
                while let Some(parent) = row(current, op)?.parent {
                    if members.values[parent.index()] != BODY
                        || row(parent, op)?.first_child != Some(current)
                    {
                        break;
                    }
                    op.copy(1)?;
                    members.values[parent.index()] = EXCLUDED;
                    current = parent;
                }
                break;
            }
            op.copy(1)?;
            members.values[current.index()] = BODY;
            if let Some(child) = row(current, op)?.first_child {
                current = child;
                continue;
            }
            loop {
                let current_row = row(current, op)?;
                if let Some(next) = current_row.next_sibling {
                    current = next;
                    break;
                }
                let Some(parent) = current_row.parent else {
                    return Ok(Self {
                        heading: first,
                        members,
                    });
                };
                current = parent;
            }
        }
        Ok(Self {
            heading: first,
            members,
        })
    }

    pub(super) fn contains(&self, node: NodeId, op: &mut Operation) -> Result<bool, CoreError> {
        Ok(self.membership(node, op)? != EXCLUDED)
    }

    pub(super) fn contains_body(
        &self,
        node: NodeId,
        op: &mut Operation,
    ) -> Result<bool, CoreError> {
        Ok(self.membership(node, op)? == BODY)
    }

    fn membership(&self, node: NodeId, op: &mut Operation) -> Result<u8, CoreError> {
        op.scan(1)?;
        self.members
            .values
            .get(node.index())
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnknownId))
    }
}
