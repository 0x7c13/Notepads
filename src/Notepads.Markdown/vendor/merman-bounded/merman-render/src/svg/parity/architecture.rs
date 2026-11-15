mod edges;
mod foreign_object;
mod geometry;
mod icons;
mod labels;
mod model;
mod nodes;
mod render;
mod root;
mod settings;
mod viewport;

/// Non-billing terminal replay shared by Architecture SVG emission passes.
#[derive(Clone, Copy)]
pub(super) struct ArchitectureEmitCheckpoints<'a> {
    work_meter: &'a crate::resources::OperationWorkMeter,
}

impl<'a> ArchitectureEmitCheckpoints<'a> {
    pub(super) const fn new(work_meter: &'a crate::resources::OperationWorkMeter) -> Self {
        Self { work_meter }
    }

    pub(super) fn checkpoint(self) -> crate::Result<()> {
        self.work_meter
            .checkpoint(merman_core::OperationPhase::Emit)
            .map_err(Into::into)
    }
}

pub(super) use render::render_architecture_diagram_svg_typed_with_config;
