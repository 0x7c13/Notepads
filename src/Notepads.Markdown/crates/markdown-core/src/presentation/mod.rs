// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded, immutable semantic presentation of an accepted layout batch.

use crate::error::CoreError;
use crate::layout::{LayoutKey, MeasurementBatch};
use crate::model::AcceptedModel;
use crate::policy::Operation;

mod build;
mod storage;
mod types;
pub use types::*;

/// Owns only this finite projection and the original validated model pin.
pub struct PresentationBatch {
    buffers: build::Buffers,
    model: AcceptedModel,
    key: LayoutKey,
    start: usize,
}

impl PresentationBatch {
    pub fn build(
        model: AcceptedModel,
        batch: &MeasurementBatch<'_>,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        let buffers = build::build(&model, batch, op)?;
        Ok(Self {
            buffers,
            model,
            key: batch.key,
            start: batch.start,
        })
    }

    pub fn model(&self) -> &AcceptedModel {
        &self.model
    }

    pub fn key(&self) -> LayoutKey {
        self.key
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn blocks(&self) -> &[Block] {
        &self.buffers.blocks.values
    }
    pub fn texts(&self) -> &[Text] {
        &self.buffers.texts.values
    }
    pub fn runs(&self) -> &[Run] {
        &self.buffers.runs.values
    }
    pub fn script_runs(&self) -> &[ScriptRun] {
        &self.buffers.script_runs.values
    }
    pub fn script_scopes(&self) -> &[ScriptScope] {
        &self.buffers.script_scopes.values
    }
    pub fn ruby_units(&self) -> &[RubyUnit] {
        &self.buffers.ruby_units.values
    }

    pub fn cells(&self) -> &[Cell] {
        &self.buffers.cells.values
    }
    pub fn decorations(&self) -> &[Decoration] {
        &self.buffers.decorations.values
    }
    pub fn images(&self) -> &[Image] {
        &self.buffers.images.values
    }
    pub fn derived(&self) -> &[DerivedObject] {
        &self.buffers.derived.values
    }
    pub fn links(&self) -> &[Link] {
        &self.buffers.links.values
    }
    pub fn blob(&self) -> &[u8] {
        &self.buffers.blob.values
    }
}

#[cfg(test)]
mod tests;
