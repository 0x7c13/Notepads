// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Original HTML control facts, bound only to a second independently accepted
//! model. Generated GFM marker bytes never authorize an original-source edit.

use super::HtmlProjection;
use crate::error::CoreError;
use crate::identity::LogicalSpan;
use crate::ir::{AtomContent, AtomId, AtomKind, NodeKind, ObjectId, ObjectRow};
use crate::mapping::Affinity;
use crate::model::AcceptedModel;
use crate::policy::Operation;
use std::ops::Range;

#[derive(Debug)]
pub(super) struct StaticCheckbox {
    pub marker: Range<usize>,
    pub source: Range<usize>,
    pub checked: bool,
    pub object: Option<ObjectId>,
    pub atom: Option<AtomId>,
    pub logical: Option<LogicalSpan>,
}

impl HtmlProjection {
    pub fn bind_static_checkboxes(
        &mut self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.checkpoint_now()?;
        if self.static_checkboxes.is_empty() {
            return Ok(());
        }
        if model.source().key() != self.original.key() {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        let mut previous_end = 0;
        let mut previous_source_end = 0;
        for control in &self.static_checkboxes {
            op.events(1)?;
            if control.marker.start < previous_end
                || control.source.start < previous_source_end
                || control.marker.end.checked_sub(control.marker.start) != Some(3)
                || control.source.is_empty()
                || self.original.text().get(control.source.clone()).is_none()
                || model.source().text().get(control.marker.clone())
                    != Some(if control.checked { "[x]" } else { "[ ]" })
                || control.object.is_some()
                || control.atom.is_some()
                || control.logical.is_some()
                || self.from_preview(control.marker.start as u64, Affinity::After)
                    != control.source.start as u64
                || self.from_preview(control.marker.end as u64, Affinity::Before)
                    != control.source.end as u64
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            previous_end = control.marker.end;
            previous_source_end = control.source.end;
        }
        let ir = model.ir();
        let lookup_work = u64::from(self.static_checkboxes.len().checked_ilog2().unwrap_or(0)) + 1;
        for (index, object) in ir.objects().iter().enumerate() {
            op.events(1)?;
            let ObjectRow::Task {
                node,
                atom,
                checked,
                marker,
                ..
            } = *object
            else {
                continue;
            };
            let sources = marker.slice(ir.sources()).map_err(|error| op.fail(error))?;
            let Some(first) = sources.first() else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            op.events(lookup_work)?;
            let candidate = self
                .static_checkboxes
                .partition_point(|control| control.marker.end as u64 <= first.start());
            let Some(control) = self.static_checkboxes.get_mut(candidate) else {
                continue;
            };
            let last = sources.last().unwrap();
            if last.end() <= control.marker.start as u64 {
                continue;
            }
            if sources.len() != 1
                || first.start() != control.marker.start as u64
                || first.end() != control.marker.end as u64
                || checked != control.checked
                || control.object.is_some()
                || ir.nodes()[node.index()].kind != NodeKind::TaskCheckbox
                || ir.nodes()[node.index()].source_envelope != *first
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            control.object = Some(ObjectId::from_index(index).map_err(|error| op.fail(error))?);
            let actual = ir.atom_at(atom.index(), op)?;
            if actual.kind != AtomKind::Object
                || actual.content != AtomContent::Object(control.object.unwrap())
                || actual.owner != node
                || actual.target.end.checked_sub(actual.target.start) != Some(1)
                || ir.nodes()[node.index()].flow != model.main_flow_id()
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            control.atom = Some(atom);
            control.logical = Some(
                LogicalSpan::new(actual.target.start, actual.target.end)
                    .map_err(|error| op.fail(error))?,
            );
        }
        for control in &self.static_checkboxes {
            op.events(1)?;
            if control.object.is_none() {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
        }
        // Keep source/marker order for mapping. A separate paid identity index
        // supports presentation without assuming object ids follow source order.
        if !self.static_checkbox_objects.is_empty() {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        self.static_checkbox_objects.extend(
            self.static_checkboxes
                .iter()
                .map(|control| control.object.unwrap()),
        );
        super::charge_events(op, self.static_checkboxes.len() as u64 * lookup_work)?;
        self.static_checkbox_objects.sort_unstable();
        self.static_checkbox_flow = Some(model.main_flow_id());
        op.checkpoint_now()?;
        Ok(())
    }

    pub fn is_static_html_checkbox(&self, object: ObjectId) -> bool {
        self.static_checkbox_objects.binary_search(&object).is_ok()
    }
}
