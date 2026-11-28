mod activation;
mod actor_man;
mod actor_man_glyphs;
mod actor_popup;
mod actor_shapes;
mod actors;
mod block_collection;
mod block_geometry;
mod block_text;
mod blocks;
mod css;
mod frames;
mod geometry;
mod interactions;
mod math_label;
mod messages;
mod model;
mod notes;
mod render;
mod root;
mod settings;
mod text;

use crate::Result;
use crate::resources::OperationWorkMeter;
use merman_core::OperationPhase;

const SEQUENCE_EMIT_CHECKPOINT_INTERVAL: usize = 64;

/// Non-billing cancellation projection shared by Sequence SVG emission passes.
#[derive(Clone, Copy)]
pub(super) struct SequenceEmitCheckpoints<'a> {
    work_meter: &'a OperationWorkMeter,
}

impl<'a> SequenceEmitCheckpoints<'a> {
    pub(super) const fn new(work_meter: &'a OperationWorkMeter) -> Self {
        Self { work_meter }
    }

    pub(super) fn checkpoint(self) -> Result<()> {
        self.work_meter
            .checkpoint(OperationPhase::Emit)
            .map_err(Into::into)
    }

    pub(super) fn checkpoint_loop(self, iteration: usize) -> Result<()> {
        if iteration.is_multiple_of(SEQUENCE_EMIT_CHECKPOINT_INTERVAL) {
            self.checkpoint()?;
        }
        Ok(())
    }

    pub(super) const fn text(self) -> crate::sequence::SequenceTextCheckpoints<'a> {
        crate::sequence::SequenceTextCheckpoints::for_phase(self.work_meter, OperationPhase::Emit)
    }
}

pub(super) use render::render_sequence_diagram_svg_model_with_config;
