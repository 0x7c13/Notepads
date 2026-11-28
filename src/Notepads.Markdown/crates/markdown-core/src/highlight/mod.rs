// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Finite immutable source cache. Main/code copy/source mapping are never rewritten.
//! Caller supplies independent same-root highlight Operations. The legacy pass is source-wide;
//! selected passes share a finite exact-content cache without retaining obsolete models.

mod build;
mod cache;
mod engine;
mod storage;
mod types;
mod work;
pub use cache::{HighlightCache, MAX_CACHE_BYTES};
pub use types::*;

use crate::error::CoreError;
use crate::identity::ModelIdentity;
use crate::model::AcceptedModel;
use crate::policy::Operation;

pub struct HighlightSnapshot {
    // Actual buffers are freed before the source/model pin and its root ownership.
    buffers: build::Buffers,
    model: AcceptedModel,
    summary: HighlightSummary,
}

impl HighlightSnapshot {
    /// Err: invalid owner, front-loaded enumeration or inability to fund the snapshot table.
    /// Once the table exists, cooperative/auxiliary failure is Summary::Aborted; unprocessed
    /// rows stay Pending with no tokens. Ordinary regex failure is per-block Failed.
    /// The caller retires this Operation and separately checks current source before publishing;
    /// neither this cache nor a budget match is authority to publish an obsolete source.
    pub fn build(model: AcceptedModel, op: &mut Operation) -> Result<Self, CoreError> {
        let (buffers, summary) = build::build(&model, op)?;
        Ok(Self {
            buffers,
            model,
            summary,
        })
    }

    /// Highlight only sorted, unique, nonzero main-flow CodeBlock node IDs.
    /// The serialized cache retains exact text/tokens, never an accepted model.
    pub fn build_selected(
        model: AcceptedModel,
        nodes: &[u32],
        cache: &mut HighlightCache,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        let (buffers, summary) = build::build_selected(&model, nodes, cache, op)?;
        Ok(Self {
            buffers,
            model,
            summary,
        })
    }

    pub fn model(&self) -> &AcceptedModel {
        &self.model
    }
    pub fn identity(&self) -> ModelIdentity {
        self.model.identity()
    }
    pub fn summary(&self) -> HighlightSummary {
        self.summary
    }
    pub fn rows(&self) -> &[HighlightBlock] {
        &self.buffers.rows.values
    }
    pub fn spans(&self) -> &[TokenSpan] {
        self.buffers.spans.values()
    }
}

#[cfg(test)]
mod selected_tests;
#[cfg(test)]
mod tests;
