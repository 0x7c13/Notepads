// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned results from the same core used by the ordinary bidi API.

use core::ops::Range;
use markdown_svg_budget::{BudgetVec, Error, FailureKind, LimitKind, ResourcesRef};

use crate::{
    storage::{RequestStorage, Storage},
    BidiClass, BidiDataSource, Level, LevelRun, ParagraphInfo,
};

/// Whole-text bidi results whose allocations stay paired with the request.
///
/// This type deliberately has no Clone or conversion to an unpaired Vec.
#[derive(Debug)]
pub struct BidiInfo<'t> {
    text: &'t str,
    original_classes: BudgetVec<BidiClass>,
    levels: BudgetVec<Level>,
    paragraphs: BudgetVec<ParagraphInfo>,
    resources: ResourcesRef,
}

/// Simultaneously owned L1 levels and visually ordered level runs.
#[derive(Debug)]
pub struct VisualRuns {
    levels: BudgetVec<Level>,
    runs: BudgetVec<LevelRun>,
}

impl VisualRuns {
    /// Borrow all L1-adjusted levels, including bytes outside the selected line.
    pub fn levels(&self) -> &[Level] {
        &self.levels
    }

    /// Borrow the visual run order.
    pub fn runs(&self) -> &[LevelRun] {
        &self.runs
    }

    /// Transfer both allocations together with their original payment owners.
    pub fn into_parts(self) -> (BudgetVec<Level>, BudgetVec<LevelRun>) {
        (self.levels, self.runs)
    }
}

impl<'t> BidiInfo<'t> {
    /// Resolve the whole input using the fixed hardcoded Unicode data.
    #[cfg(feature = "hardcoded-data")]
    pub fn try_new(
        text: &'t str,
        default_level: Option<Level>,
        resources: &ResourcesRef,
    ) -> Result<Self, Error> {
        Self::try_new_with_data_source(&crate::HardcodedBidiData, text, default_level, resources)
    }

    /// Resolve the whole input using the supplied bidi properties.
    pub fn try_new_with_data_source<D: BidiDataSource>(
        data: &D,
        text: &'t str,
        default_level: Option<Level>,
        resources: &ResourcesRef,
    ) -> Result<Self, Error> {
        let mut cx = RequestStorage::new(resources)?;
        let result = crate::bidi_info_in(data, text, default_level, &mut cx)?;
        cx.finish()?;
        Ok(Self {
            text,
            original_classes: result.original_classes,
            levels: result.levels,
            paragraphs: result.paragraphs,
            resources: resources.clone(),
        })
    }

    /// Borrow the original input without copying its bytes.
    pub fn text(&self) -> &'t str {
        self.text
    }

    /// Borrow one original bidi class per UTF-8 byte.
    pub fn original_classes(&self) -> &[BidiClass] {
        &self.original_classes
    }

    /// Borrow one resolved embedding level per UTF-8 byte.
    pub fn levels(&self) -> &[Level] {
        &self.levels
    }

    /// Borrow the whole-input paragraph boundaries and base levels.
    pub fn paragraphs(&self) -> &[ParagraphInfo] {
        &self.paragraphs
    }

    fn validate_line(
        &self,
        paragraph_index: usize,
        line: &Range<usize>,
    ) -> Result<&ParagraphInfo, Error> {
        let invalid = || {
            self.resources
                .record(Error::new(FailureKind::Invariant, LimitKind::None))
        };
        let paragraph = self.paragraphs.get(paragraph_index).ok_or_else(invalid)?;
        if line.start > line.end
            || line.start < paragraph.range.start
            || line.end > paragraph.range.end
            || !self.text.is_char_boundary(line.start)
            || !self.text.is_char_boundary(line.end)
        {
            return Err(invalid());
        }
        Ok(paragraph)
    }

    /// Copy the complete levels and apply L1 only to the selected line.
    ///
    /// A valid empty line, including one at the input end, preserves the full
    /// level result. Invalid internal parameters retain the first typed error.
    pub fn try_reordered_levels(
        &self,
        paragraph_index: usize,
        line: Range<usize>,
    ) -> Result<BudgetVec<Level>, Error> {
        let mut cx = RequestStorage::new(&self.resources)?;
        let paragraph = self.validate_line(paragraph_index, &line)?;
        let result = crate::reordered_levels_in(
            &self.original_classes,
            &self.levels,
            self.text,
            paragraph,
            line,
            &mut cx,
        )?;
        cx.finish()?;
        Ok(result)
    }

    /// Return paid full L1 levels and visual runs for the selected line.
    ///
    /// The upstream core indexes the level at the line start. Thus an empty
    /// line inside the input keeps its ordinary one-empty-run value, while a
    /// line starting at the input end is rejected before entering the core.
    pub fn try_visual_runs(
        &self,
        paragraph_index: usize,
        line: Range<usize>,
    ) -> Result<VisualRuns, Error> {
        let mut cx = RequestStorage::new(&self.resources)?;
        let paragraph = self.validate_line(paragraph_index, &line)?;
        if line.start >= self.levels.len() {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        let levels = crate::reordered_levels_in(
            &self.original_classes,
            &self.levels,
            self.text,
            paragraph,
            line.start..line.end,
            &mut cx,
        )?;
        let (levels, runs) = crate::visual_runs_for_line_in(levels, &line, &mut cx)?;
        cx.finish()?;
        Ok(VisualRuns { levels, runs })
    }
}

/// Build a visual permutation with the same core as ordinary `reorder_visual`.
pub fn try_reorder_visual(
    levels: &[Level],
    resources: &ResourcesRef,
) -> Result<BudgetVec<usize>, Error> {
    let mut cx = RequestStorage::new(resources)?;
    let result = crate::reorder_visual_in(levels, &mut cx)?;
    cx.finish()?;
    Ok(result)
}

#[cfg(test)]
#[path = "controlled_tests.rs"]
mod tests;
