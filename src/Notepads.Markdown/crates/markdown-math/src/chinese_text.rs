// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use ratex_layout::{engine::collect_chinese_text, layout_options::ChineseTextRun};
use ratex_parser::ParseNode;
use ratex_types::{
    execution::{ExecutionError, Phase, Resource},
    path_command::PathCommand,
    storage::{StorageFamily, StorageLedger},
};

use crate::MathError;

pub use ratex_layout::layout_options::{
    ChineseTextFamily, ChineseTextStyle, FallbackGlyphMetrics as ChineseTextMetrics,
};
pub use ratex_types::path_command::PathCommand as ChineseTextCommand;

/// Platform boundary used only before recursive math layout. Positions and
/// metrics are relative to the run's baseline and use a one-em font size.
pub trait ChineseTextShaper {
    fn shape(
        &mut self,
        utf16: &[u16],
        style: ChineseTextStyle,
        outline: &mut ChineseTextOutline<'_, '_>,
    ) -> Result<Option<ChineseTextMetrics>, MathError>;
}

/// Funded numerical sink for one shaped run. A provider emits closed,
/// nonzero-winding contours and can check the current job during callbacks.
pub struct ChineseTextOutline<'s, 'a> {
    storage: &'s StorageLedger<'a>,
    remaining: &'s mut u64,
    commands: Vec<PathCommand>,
    open: bool,
}

impl ChineseTextOutline<'_, '_> {
    pub fn checkpoint(&self) -> Result<(), MathError> {
        self.storage.check(Phase::Layout)?;
        Ok(())
    }

    pub fn push(&mut self, command: ChineseTextCommand) -> Result<(), MathError> {
        self.checkpoint()?;
        let open = match command {
            PathCommand::MoveTo { .. } if !self.open => true,
            PathCommand::Close if self.open => false,
            PathCommand::LineTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::CubicTo { .. }
                if self.open =>
            {
                true
            }
            _ => return Err(MathError::InvalidGeometry),
        };
        if *self.remaining == 0 {
            let control = self.storage.control();
            let limit = control.limits().values[Resource::OutlineSegments as usize];
            return Err(MathError::Execution(control.fail(
                ExecutionError::ResourceLimit {
                    phase: Phase::Layout,
                    resource: Resource::OutlineSegments,
                    limit,
                    requested: limit.saturating_add(1),
                },
            )));
        }
        self.storage.try_push(
            Phase::Layout,
            StorageFamily::Path,
            &mut self.commands,
            command,
        )?;
        *self.remaining -= 1;
        self.open = open;
        Ok(())
    }
}

pub(crate) struct PreparedChineseText<'n> {
    pub(crate) runs: Vec<ChineseTextRun<'n>>,
    pub(crate) paths: Vec<Vec<PathCommand>>,
}

pub(crate) fn prepare<'n>(
    nodes: &'n [ParseNode],
    storage: &StorageLedger<'_>,
    shaper: &mut dyn ChineseTextShaper,
) -> Result<PreparedChineseText<'n>, MathError> {
    let requests = collect_chinese_text(nodes, storage)?;
    let mut runs =
        storage.try_vec_with_capacity(Phase::Layout, StorageFamily::Auxiliary, requests.len())?;
    let mut paths =
        storage.try_vec_with_capacity(Phase::Layout, StorageFamily::Auxiliary, requests.len())?;
    // Bound provisional outlines separately from final output. The output
    // consumer charges each placed contour even when one shape is reused.
    let mut remaining = storage.control().limits().values[Resource::OutlineSegments as usize];
    for request in requests {
        let mut utf16 = storage.try_vec_with_capacity(
            Phase::Layout,
            StorageFamily::Auxiliary,
            request.text.encode_utf16().count(),
        )?;
        utf16.extend(request.text.encode_utf16());
        let mut outline = ChineseTextOutline {
            storage,
            remaining: &mut remaining,
            commands: Vec::new(),
            open: false,
        };
        outline.checkpoint()?;
        let metrics = shaper.shape(&utf16, request.style, &mut outline)?;
        outline.checkpoint()?;
        let commands = if metrics.is_some() {
            if outline.open
                || (outline.commands.is_empty() && request.text.chars().any(|ch| ch != '\u{3000}'))
            {
                return Err(MathError::InvalidGeometry);
            }
            outline.commands
        } else {
            Vec::new()
        };
        runs.push(ChineseTextRun { request, metrics });
        paths.push(commands);
    }
    Ok(PreparedChineseText { runs, paths })
}
