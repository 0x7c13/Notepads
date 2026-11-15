use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, OperationControl, OperationControlResult,
    SourceSpan,
};
use std::ops::Range;

const CONTROLLED_EDIT_REBUILD_CHECKPOINT_BYTES: usize = 4 * 1024;
const CONTROLLED_EDIT_REBUILD_CHECKPOINT_ITEMS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplacementMapping {
    ExactBytes,
    Boundaries,
}

#[derive(Debug, Clone)]
pub(super) struct SourceEdit {
    range: Range<usize>,
    replacement: String,
    mapping: ReplacementMapping,
}

impl SourceEdit {
    pub(super) fn delete(range: Range<usize>) -> Self {
        Self {
            range,
            replacement: String::new(),
            mapping: ReplacementMapping::Boundaries,
        }
    }

    pub(super) fn replace(
        range: Range<usize>,
        replacement: impl Into<String>,
        mapping: ReplacementMapping,
    ) -> Self {
        Self {
            range,
            replacement: replacement.into(),
            mapping,
        }
    }
}

/// Parser text together with an exact, composable map back to the caller's source.
#[derive(Debug, Clone)]
pub struct PreprocessedSource {
    text: String,
    edit_map: SourceEditMap,
    global_expected_syntax: Vec<EditorExpectedSyntax>,
    global_directive_prefixes: Vec<String>,
    recovered_incomplete_directive: bool,
    frontmatter_line_offset: usize,
    first_line_column_offset: usize,
}

impl PreprocessedSource {
    pub fn new(source: &str) -> Self {
        Self::new_controlled(source, &OperationControl::new())
            .expect("a private parse control cannot be cancelled")
    }

    pub(super) fn new_controlled(
        source: &str,
        control: &OperationControl,
    ) -> OperationControlResult<Self> {
        control.checkpoint()?;
        let mut text = String::with_capacity(source.len());
        let mut chunk_start = 0usize;
        while source.len().saturating_sub(chunk_start) >= CONTROLLED_EDIT_REBUILD_CHECKPOINT_BYTES {
            let mut chunk_end = chunk_start + CONTROLLED_EDIT_REBUILD_CHECKPOINT_BYTES;
            while !source.is_char_boundary(chunk_end) {
                chunk_end += 1;
            }
            text.push_str(&source[chunk_start..chunk_end]);
            control.checkpoint()?;
            chunk_start = chunk_end;
        }
        text.push_str(&source[chunk_start..]);
        control.checkpoint()?;
        Ok(Self {
            text,
            edit_map: SourceEditMap::identity(source.len()),
            global_expected_syntax: Vec::new(),
            global_directive_prefixes: Vec::new(),
            recovered_incomplete_directive: false,
            frontmatter_line_offset: 0,
            first_line_column_offset: 0,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn copy_global_editor_evidence_from(&mut self, source: &Self) {
        self.global_expected_syntax
            .clone_from(&source.global_expected_syntax);
        self.global_directive_prefixes
            .clone_from(&source.global_directive_prefixes);
    }

    pub(crate) fn parser_position_offset(&self) -> (usize, usize) {
        (self.frontmatter_line_offset, self.first_line_column_offset)
    }

    pub(super) fn record_frontmatter_removal(&mut self, end: usize) {
        self.frontmatter_line_offset += self.text[..end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        self.first_line_column_offset = 0;
    }

    pub(super) fn set_first_line_column_offset(&mut self, columns: usize) {
        self.first_line_column_offset = columns;
    }

    pub fn try_map_span(&self, span: SourceSpan) -> Option<SourceSpan> {
        if !self.text.is_char_boundary(span.start) || !self.text.is_char_boundary(span.end) {
            return None;
        }
        self.edit_map.try_map_span(span)
    }

    pub(super) fn try_map_enclosing_span(&self, span: SourceSpan) -> Option<SourceSpan> {
        if !self.text.is_char_boundary(span.start) || !self.text.is_char_boundary(span.end) {
            return None;
        }
        self.edit_map.try_map_enclosing_span(span)
    }

    pub(crate) fn global_expected_syntax(&self) -> &[EditorExpectedSyntax] {
        &self.global_expected_syntax
    }

    pub(crate) fn global_directive_prefixes(&self) -> &[String] {
        &self.global_directive_prefixes
    }

    pub(crate) const fn recovered_incomplete_directive(&self) -> bool {
        self.recovered_incomplete_directive
    }

    pub(super) fn mark_recovered_incomplete_directive(&mut self) {
        self.recovered_incomplete_directive = true;
    }

    pub(super) fn record_global_directive_prefix(&mut self, prefix: String) {
        if !self.global_directive_prefixes.contains(&prefix) {
            self.global_directive_prefixes.push(prefix);
        }
    }

    pub(super) fn record_global_expected_syntax(
        &mut self,
        kind: EditorExpectedSyntaxKind,
        span: SourceSpan,
    ) {
        let Some(span) = self.try_map_span(span) else {
            return;
        };
        self.global_expected_syntax
            .push(EditorExpectedSyntax::new(kind, span));
    }

    pub fn into_text(self) -> String {
        self.text
    }

    pub(super) fn apply_edits(
        &mut self,
        edits: Vec<SourceEdit>,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let mut checkpoints = ControlledEditRebuildCheckpoints::new(control)?;
        if edits.is_empty() {
            return Ok(());
        }
        // Preprocessors emit edits in source order. Keeping that as an invariant avoids an
        // uninterruptible sort inside the controlled reconstruction path.
        assert!(
            edits_are_sorted_controlled(&edits, &mut checkpoints)?,
            "source edits must be sorted by range"
        );
        let (replacement_bytes, removed_bytes) =
            assert_valid_edits_controlled(&self.text, &edits, &mut checkpoints)?;

        // Borrow the published state until every checkpoint and invariant check succeeds. A
        // cancellation drops only these local builders and cannot expose a partial rewrite.
        let old_text = &self.text;
        let old_map = &self.edit_map;
        let mut text = String::with_capacity(
            old_text
                .len()
                .saturating_sub(removed_bytes)
                .saturating_add(replacement_bytes),
        );
        let mut builder = EditMapBuilder::new(old_map.original_len);
        let mut cursor = 0usize;

        for edit in edits {
            checkpoints.processed_item()?;
            checkpoints.push_str(&mut text, &old_text[cursor..edit.range.start])?;
            builder.copy_from(old_map, cursor..edit.range.start, &mut checkpoints)?;

            if edit.replacement.is_empty() {
                builder.delete_from(old_map, edit.range.clone(), &mut checkpoints)?;
            } else {
                checkpoints.push_str(&mut text, &edit.replacement)?;
                builder.replace_from(
                    old_map,
                    edit.range.clone(),
                    edit.replacement.len(),
                    edit.mapping,
                    &mut checkpoints,
                )?;
            }
            cursor = edit.range.end;
        }

        checkpoints.push_str(&mut text, &old_text[cursor..])?;
        builder.copy_from(old_map, cursor..old_text.len(), &mut checkpoints)?;
        let edit_map = builder.finish(text.len(), &mut checkpoints)?;
        #[cfg(debug_assertions)]
        {
            let well_formed = edit_map.is_well_formed_controlled(&mut checkpoints)?;
            debug_assert!(well_formed);
        }
        checkpoints.finish()?;

        self.text = text;
        self.edit_map = edit_map;
        Ok(())
    }

    #[cfg(test)]
    fn apply_edits_uncontrolled(&mut self, edits: Vec<SourceEdit>) {
        self.apply_edits(edits, &OperationControl::new())
            .expect("a private parse control cannot be cancelled");
    }
}

struct ControlledEditRebuildCheckpoints<'a> {
    control: &'a OperationControl,
    bytes_since_checkpoint: usize,
    items_since_checkpoint: usize,
}

impl<'a> ControlledEditRebuildCheckpoints<'a> {
    fn new(control: &'a OperationControl) -> OperationControlResult<Self> {
        control.checkpoint()?;
        Ok(Self {
            control,
            bytes_since_checkpoint: 0,
            items_since_checkpoint: 0,
        })
    }

    fn push_str(&mut self, output: &mut String, value: &str) -> OperationControlResult<()> {
        let mut start = 0usize;
        while start < value.len() {
            let remaining_budget = CONTROLLED_EDIT_REBUILD_CHECKPOINT_BYTES
                .saturating_sub(self.bytes_since_checkpoint);
            if remaining_budget == 0 {
                self.checkpoint()?;
                continue;
            }

            let mut end = start.saturating_add(remaining_budget).min(value.len());
            while end > start && !value.is_char_boundary(end) {
                end -= 1;
            }
            if end == start {
                self.checkpoint()?;
                continue;
            }

            output.push_str(&value[start..end]);
            self.bytes_since_checkpoint += end - start;
            start = end;
            if self.bytes_since_checkpoint == CONTROLLED_EDIT_REBUILD_CHECKPOINT_BYTES {
                self.checkpoint()?;
            }
        }
        Ok(())
    }

    fn processed_item(&mut self) -> OperationControlResult<()> {
        self.items_since_checkpoint += 1;
        if self.items_since_checkpoint == CONTROLLED_EDIT_REBUILD_CHECKPOINT_ITEMS {
            self.checkpoint()?;
        }
        Ok(())
    }

    fn checkpoint(&mut self) -> OperationControlResult<()> {
        self.control.checkpoint()?;
        self.bytes_since_checkpoint = 0;
        self.items_since_checkpoint = 0;
        Ok(())
    }

    fn finish(&self) -> OperationControlResult<()> {
        self.control.checkpoint()
    }
}

#[derive(Debug, Clone)]
struct SourceEditMap {
    original_len: usize,
    output_len: usize,
    segments: Vec<EditMapSegment>,
    gaps: Vec<EditMapGap>,
    // Boundary mappings are frequent (notably CRLF normalization) but do not make a span
    // unmappable. Keep only the relevant regions in a searchable index for fact lookups.
    unmapped_output_ranges: Vec<Range<usize>>,
    #[cfg(test)]
    scan_stats: EditMapScanStats,
}

impl SourceEditMap {
    fn identity(source_len: usize) -> Self {
        let segments = (source_len > 0)
            .then_some(EditMapSegment {
                output: 0..source_len,
                original: 0..source_len,
                mapping: SegmentMapping::ExactBytes,
            })
            .into_iter()
            .collect();
        Self {
            original_len: source_len,
            output_len: source_len,
            segments,
            gaps: Vec::new(),
            unmapped_output_ranges: Vec::new(),
            #[cfg(test)]
            scan_stats: EditMapScanStats::default(),
        }
    }

    /// Maps a parser-input span to the smallest exact span in the original source.
    ///
    /// A span that crosses deleted text or ends inside a length-changing replacement is not exact
    /// and returns `None`. Adjacent facts remain independently mappable.
    fn try_map_span(&self, span: SourceSpan) -> Option<SourceSpan> {
        if span.start > span.end || span.end > self.output_len {
            return None;
        }
        if self.has_unmapped_overlap(span.start, span.end)
            || (span.start < span.end && self.has_gap_inside(span.start, span.end))
        {
            return None;
        }

        let start = self.original_at_start(span.start)?;
        let end = if span.start == span.end {
            start
        } else {
            self.original_at_end(span.end)?
        };
        (start <= end).then(|| SourceSpan::new(start, end))
    }

    /// Maps span boundaries while allowing deleted source bytes inside the enclosing range.
    ///
    /// This is provenance-only: callers must not treat the returned range as an exact rewrite
    /// target. It is used for parser facts such as multiline keys that cross dedented indentation
    /// or normalized line endings.
    fn try_map_enclosing_span(&self, span: SourceSpan) -> Option<SourceSpan> {
        if span.start > span.end
            || span.end > self.output_len
            || self.has_unmapped_overlap(span.start, span.end)
        {
            return None;
        }

        let start = self.original_at_start(span.start)?;
        let end = if span.start == span.end {
            start
        } else {
            self.original_at_end(span.end)?
        };
        (start <= end).then(|| SourceSpan::new(start, end))
    }

    fn original_at_start(&self, offset: usize) -> Option<usize> {
        if offset > self.output_len {
            return None;
        }
        if let Some(gap) = self.gap_at(offset) {
            return Some(gap.original_right);
        }
        if offset == self.output_len {
            return match self.segments.last() {
                Some(segment) => segment.original_at_end(offset),
                None => Some(self.original_len),
            };
        }
        self.segment_to_right(offset)?.original_at_start(offset)
    }

    fn original_at_end(&self, offset: usize) -> Option<usize> {
        if offset > self.output_len {
            return None;
        }
        if let Some(gap) = self.gap_at(offset) {
            return Some(gap.original_left);
        }
        if offset == 0 {
            return match self.segments.first() {
                Some(segment) => segment.original_at_start(offset),
                None => Some(0),
            };
        }
        self.segment_to_left(offset)?.original_at_end(offset)
    }

    fn segment_to_right(&self, offset: usize) -> Option<&EditMapSegment> {
        let index = self
            .segments
            .partition_point(|segment| segment.output.end <= offset);
        self.segments
            .get(index)
            .filter(|segment| segment.output.start <= offset && offset < segment.output.end)
    }

    fn segment_to_left(&self, offset: usize) -> Option<&EditMapSegment> {
        let index = self
            .segments
            .partition_point(|segment| segment.output.start < offset)
            .checked_sub(1)?;
        self.segments
            .get(index)
            .filter(|segment| segment.output.start < offset && offset <= segment.output.end)
    }

    fn gap_at(&self, offset: usize) -> Option<&EditMapGap> {
        self.gaps
            .binary_search_by_key(&offset, |gap| gap.output_offset)
            .ok()
            .and_then(|index| self.gaps.get(index))
    }

    fn has_gap_inside(&self, start: usize, end: usize) -> bool {
        let first = self.gaps.partition_point(|gap| gap.output_offset <= start);
        self.gaps
            .get(first)
            .is_some_and(|gap| gap.output_offset < end)
    }

    fn has_unmapped_overlap(&self, start: usize, end: usize) -> bool {
        let first = self
            .unmapped_output_ranges
            .partition_point(|range| range.end <= start);
        self.unmapped_output_ranges.get(first).is_some_and(|range| {
            if start == end {
                range.start < start && start < range.end
            } else {
                range.start < end && start < range.end
            }
        })
    }

    #[cfg(test)]
    fn unmapped_ranges_match_segments(&self) -> bool {
        self.unmapped_output_ranges.iter().eq(self
            .segments
            .iter()
            .filter(|segment| segment.mapping == SegmentMapping::Unmapped)
            .map(|segment| &segment.output))
    }

    #[cfg(test)]
    fn is_well_formed(&self) -> bool {
        let control = OperationControl::new();
        let mut checkpoints = ControlledEditRebuildCheckpoints::new(&control)
            .expect("a private parse control cannot be cancelled");
        self.is_well_formed_controlled(&mut checkpoints)
            .expect("a private parse control cannot be cancelled")
    }

    #[cfg(any(test, debug_assertions))]
    fn is_well_formed_controlled(
        &self,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<bool> {
        let mut previous_segment: Option<&EditMapSegment> = None;
        let mut unmapped_index = 0usize;
        for segment in &self.segments {
            checkpoints.processed_item()?;
            if segment.output.start >= segment.output.end {
                return Ok(false);
            }
            if let Some(previous) = previous_segment
                && (previous.output.end > segment.output.start
                    || previous.original.end > segment.original.start)
            {
                return Ok(false);
            }
            if segment.mapping == SegmentMapping::Unmapped {
                if self.unmapped_output_ranges.get(unmapped_index) != Some(&segment.output) {
                    return Ok(false);
                }
                unmapped_index += 1;
            }
            previous_segment = Some(segment);
        }
        if unmapped_index != self.unmapped_output_ranges.len()
            || self
                .segments
                .last()
                .is_some_and(|segment| segment.output.end > self.output_len)
        {
            return Ok(false);
        }

        let mut previous_gap_offset: Option<usize> = None;
        for gap in &self.gaps {
            checkpoints.processed_item()?;
            if previous_gap_offset.is_some_and(|offset| offset >= gap.output_offset) {
                return Ok(false);
            }
            previous_gap_offset = Some(gap.output_offset);
        }

        let mut previous_unmapped_end: Option<usize> = None;
        for range in &self.unmapped_output_ranges {
            checkpoints.processed_item()?;
            if previous_unmapped_end.is_some_and(|end| end > range.start) {
                return Ok(false);
            }
            previous_unmapped_end = Some(range.end);
        }
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditMapSegment {
    output: Range<usize>,
    original: Range<usize>,
    mapping: SegmentMapping,
}

impl EditMapSegment {
    fn original_at_start(&self, offset: usize) -> Option<usize> {
        match self.mapping {
            SegmentMapping::ExactBytes => self
                .original
                .start
                .checked_add(offset.checked_sub(self.output.start)?),
            SegmentMapping::Boundaries if offset == self.output.start => Some(self.original.start),
            SegmentMapping::Boundaries if offset == self.output.end => Some(self.original.end),
            SegmentMapping::Boundaries => None,
            SegmentMapping::Unmapped => None,
        }
    }

    fn original_at_end(&self, offset: usize) -> Option<usize> {
        self.original_at_start(offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentMapping {
    ExactBytes,
    Boundaries,
    Unmapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EditMapGap {
    output_offset: usize,
    original_left: usize,
    original_right: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EditMapScanStats {
    copy_ranges: usize,
    copy_segment_steps: usize,
    copy_gap_steps: usize,
    lookup_segment_advances: usize,
    lookup_gap_advances: usize,
    exact_segment_steps: usize,
    exact_gap_steps: usize,
}

/// Monotonic read positions into the map consumed by one sorted edit batch.
///
/// Copying, endpoint lookup, and exactness checks advance independently because replacements read
/// the same range for all three purposes. Every index only increases; copying can revisit at most
/// the one segment or gap that straddles the next range boundary.
#[derive(Default)]
struct EditMapCursor {
    copy_segment_index: usize,
    copy_gap_index: usize,
    lookup_segment_index: usize,
    lookup_gap_index: usize,
    exact_segment_index: usize,
    exact_gap_index: usize,
    #[cfg(debug_assertions)]
    last_copy_end: usize,
    #[cfg(debug_assertions)]
    last_lookup_offset: usize,
    #[cfg(debug_assertions)]
    last_exact_end: usize,
    #[cfg(test)]
    scan_stats: EditMapScanStats,
}

impl EditMapCursor {
    fn advance_lookup_to(
        &mut self,
        old: &SourceEditMap,
        offset: usize,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<()> {
        #[cfg(debug_assertions)]
        {
            debug_assert!(self.last_lookup_offset <= offset);
            self.last_lookup_offset = offset;
        }

        while old
            .segments
            .get(self.lookup_segment_index)
            .is_some_and(|segment| segment.output.end <= offset)
        {
            checkpoints.processed_item()?;
            self.lookup_segment_index += 1;
            #[cfg(test)]
            {
                self.scan_stats.lookup_segment_advances += 1;
            }
        }
        while old
            .gaps
            .get(self.lookup_gap_index)
            .is_some_and(|gap| gap.output_offset < offset)
        {
            checkpoints.processed_item()?;
            self.lookup_gap_index += 1;
            #[cfg(test)]
            {
                self.scan_stats.lookup_gap_advances += 1;
            }
        }
        Ok(())
    }

    fn gap_at_lookup_offset<'a>(
        &self,
        old: &'a SourceEditMap,
        offset: usize,
    ) -> Option<&'a EditMapGap> {
        old.gaps
            .get(self.lookup_gap_index)
            .filter(|gap| gap.output_offset == offset)
    }

    fn original_at_start(
        &mut self,
        old: &SourceEditMap,
        offset: usize,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<Option<usize>> {
        if offset > old.output_len {
            return Ok(None);
        }
        self.advance_lookup_to(old, offset, checkpoints)?;
        if let Some(gap) = self.gap_at_lookup_offset(old, offset) {
            return Ok(Some(gap.original_right));
        }
        if offset == old.output_len {
            return Ok(match old.segments.last() {
                Some(segment) => segment.original_at_end(offset),
                None => Some(old.original_len),
            });
        }
        Ok(old
            .segments
            .get(self.lookup_segment_index)
            .filter(|segment| segment.output.start <= offset && offset < segment.output.end)
            .and_then(|segment| segment.original_at_start(offset)))
    }

    fn original_at_end(
        &mut self,
        old: &SourceEditMap,
        offset: usize,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<Option<usize>> {
        if offset > old.output_len {
            return Ok(None);
        }
        self.advance_lookup_to(old, offset, checkpoints)?;
        if let Some(gap) = self.gap_at_lookup_offset(old, offset) {
            return Ok(Some(gap.original_left));
        }
        if offset == 0 {
            return Ok(match old.segments.first() {
                Some(segment) => segment.original_at_start(offset),
                None => Some(0),
            });
        }
        if let Some(segment) = old.segments.get(self.lookup_segment_index)
            && segment.output.start < offset
            && offset < segment.output.end
        {
            return Ok(segment.original_at_end(offset));
        }
        Ok(self
            .lookup_segment_index
            .checked_sub(1)
            .and_then(|index| old.segments.get(index))
            .filter(|segment| segment.output.start < offset && offset <= segment.output.end)
            .and_then(|segment| segment.original_at_end(offset)))
    }

    fn original_anchor(
        &mut self,
        old: &SourceEditMap,
        offset: usize,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<usize> {
        self.advance_lookup_to(old, offset, checkpoints)?;
        if let Some(gap) = self.gap_at_lookup_offset(old, offset) {
            return Ok(gap.original_right);
        }
        if offset == old.output_len {
            return Ok(old.original_len);
        }
        Ok(old
            .segments
            .get(self.lookup_segment_index)
            .filter(|segment| segment.output.start <= offset && offset < segment.output.end)
            .map_or(old.original_len, |segment| segment.original.start))
    }

    fn range_is_exact_bytes(
        &mut self,
        old: &SourceEditMap,
        range: &Range<usize>,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<bool> {
        #[cfg(debug_assertions)]
        {
            debug_assert!(self.last_exact_end <= range.start);
            self.last_exact_end = range.end;
        }
        if range.start > range.end || range.end > old.output_len {
            return Ok(false);
        }
        if range.start == range.end {
            return Ok(true);
        }

        while old
            .segments
            .get(self.exact_segment_index)
            .is_some_and(|segment| segment.output.end <= range.start)
        {
            checkpoints.processed_item()?;
            self.exact_segment_index += 1;
            #[cfg(test)]
            {
                self.scan_stats.exact_segment_steps += 1;
            }
        }
        while old
            .gaps
            .get(self.exact_gap_index)
            .is_some_and(|gap| gap.output_offset <= range.start)
        {
            checkpoints.processed_item()?;
            self.exact_gap_index += 1;
            #[cfg(test)]
            {
                self.scan_stats.exact_gap_steps += 1;
            }
        }
        if old
            .gaps
            .get(self.exact_gap_index)
            .is_some_and(|gap| gap.output_offset < range.end)
        {
            checkpoints.processed_item()?;
            #[cfg(test)]
            {
                self.scan_stats.exact_gap_steps += 1;
            }
            return Ok(false);
        }

        let mut output_offset = range.start;
        while output_offset < range.end {
            let Some(segment) = old.segments.get(self.exact_segment_index) else {
                return Ok(false);
            };
            checkpoints.processed_item()?;
            #[cfg(test)]
            {
                self.scan_stats.exact_segment_steps += 1;
            }
            if segment.output.start > output_offset || segment.mapping != SegmentMapping::ExactBytes
            {
                return Ok(false);
            }
            output_offset = segment.output.end.min(range.end);
            if segment.output.end <= range.end {
                self.exact_segment_index += 1;
            }
        }
        Ok(true)
    }
}

struct EditMapBuilder {
    original_len: usize,
    output_len: usize,
    segments: Vec<EditMapSegment>,
    gaps: Vec<EditMapGap>,
    cursor: EditMapCursor,
}

impl EditMapBuilder {
    fn new(original_len: usize) -> Self {
        Self {
            original_len,
            output_len: 0,
            segments: Vec::new(),
            gaps: Vec::new(),
            cursor: EditMapCursor::default(),
        }
    }

    fn copy_from(
        &mut self,
        old: &SourceEditMap,
        range: Range<usize>,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<()> {
        if range.start >= range.end {
            return Ok(());
        }
        #[cfg(debug_assertions)]
        {
            debug_assert!(self.cursor.last_copy_end <= range.start);
            self.cursor.last_copy_end = range.end;
        }
        #[cfg(test)]
        {
            self.cursor.scan_stats.copy_ranges += 1;
        }
        let output_base = self.output_len;

        while old
            .segments
            .get(self.cursor.copy_segment_index)
            .is_some_and(|segment| segment.output.end <= range.start)
        {
            checkpoints.processed_item()?;
            #[cfg(test)]
            {
                self.cursor.scan_stats.copy_segment_steps += 1;
            }
            self.cursor.copy_segment_index += 1;
        }
        while let Some(segment) = old.segments.get(self.cursor.copy_segment_index) {
            if segment.output.start >= range.end {
                break;
            }
            checkpoints.processed_item()?;
            #[cfg(test)]
            {
                self.cursor.scan_stats.copy_segment_steps += 1;
            }
            let start = segment.output.start.max(range.start);
            let end = segment.output.end.min(range.end);
            debug_assert!(start < end);

            let (original, mapping) = match segment.mapping {
                SegmentMapping::ExactBytes => {
                    let original_start = segment.original.start + (start - segment.output.start);
                    (
                        original_start..original_start + (end - start),
                        SegmentMapping::ExactBytes,
                    )
                }
                SegmentMapping::Boundaries
                    if start == segment.output.start && end == segment.output.end =>
                {
                    (segment.original.clone(), SegmentMapping::Boundaries)
                }
                SegmentMapping::Boundaries | SegmentMapping::Unmapped => {
                    let anchor = segment.original.start;
                    (anchor..anchor, SegmentMapping::Unmapped)
                }
            };
            self.push_segment(EditMapSegment {
                output: output_base + (start - range.start)..output_base + (end - range.start),
                original,
                mapping,
            });
            if segment.output.end <= range.end {
                self.cursor.copy_segment_index += 1;
            } else {
                break;
            }
        }

        while old
            .gaps
            .get(self.cursor.copy_gap_index)
            .is_some_and(|gap| gap.output_offset < range.start)
        {
            checkpoints.processed_item()?;
            #[cfg(test)]
            {
                self.cursor.scan_stats.copy_gap_steps += 1;
            }
            self.cursor.copy_gap_index += 1;
        }
        while let Some(gap) = old.gaps.get(self.cursor.copy_gap_index) {
            if gap.output_offset > range.end {
                break;
            }
            checkpoints.processed_item()?;
            #[cfg(test)]
            {
                self.cursor.scan_stats.copy_gap_steps += 1;
            }
            self.push_gap(EditMapGap {
                output_offset: output_base + (gap.output_offset - range.start),
                original_left: gap.original_left,
                original_right: gap.original_right,
            });
            if gap.output_offset == range.end {
                break;
            }
            self.cursor.copy_gap_index += 1;
        }
        self.output_len += range.end - range.start;
        Ok(())
    }

    fn delete_from(
        &mut self,
        old: &SourceEditMap,
        range: Range<usize>,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<()> {
        let left = match self.cursor.original_at_end(old, range.start, checkpoints)? {
            Some(offset) => offset,
            None => self.cursor.original_anchor(old, range.start, checkpoints)?,
        };
        let right = match self.cursor.original_at_start(old, range.end, checkpoints)? {
            Some(offset) => offset,
            None => self.cursor.original_anchor(old, range.end, checkpoints)?,
        };
        self.push_gap(EditMapGap {
            output_offset: self.output_len,
            original_left: left,
            original_right: right,
        });
        Ok(())
    }

    fn replace_from(
        &mut self,
        old: &SourceEditMap,
        range: Range<usize>,
        replacement_len: usize,
        mapping: ReplacementMapping,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<()> {
        let original_start = self
            .cursor
            .original_at_start(old, range.start, checkpoints)?;
        let start_is_mappable = original_start.is_some();
        let original_start = match original_start {
            Some(offset) => offset,
            None => self.cursor.original_anchor(old, range.start, checkpoints)?,
        };
        let original_end = self.cursor.original_at_end(old, range.end, checkpoints)?;
        let endpoints_are_mappable = start_is_mappable && original_end.is_some();
        let original_end = original_end.unwrap_or(original_start);
        let exact = mapping == ReplacementMapping::ExactBytes
            && endpoints_are_mappable
            && replacement_len == range.end - range.start
            && self.cursor.range_is_exact_bytes(old, &range, checkpoints)?
            && original_end.checked_sub(original_start) == Some(replacement_len);
        self.push_segment(EditMapSegment {
            output: self.output_len..self.output_len + replacement_len,
            original: original_start..original_end,
            mapping: if !endpoints_are_mappable {
                SegmentMapping::Unmapped
            } else if exact {
                SegmentMapping::ExactBytes
            } else {
                SegmentMapping::Boundaries
            },
        });
        self.output_len += replacement_len;
        Ok(())
    }

    fn push_segment(&mut self, segment: EditMapSegment) {
        if let Some(last) = self.segments.last_mut()
            && last.mapping == SegmentMapping::ExactBytes
            && segment.mapping == SegmentMapping::ExactBytes
            && last.output.end == segment.output.start
            && last.original.end == segment.original.start
        {
            last.output.end = segment.output.end;
            last.original.end = segment.original.end;
            return;
        }
        self.segments.push(segment);
    }

    fn push_gap(&mut self, gap: EditMapGap) {
        if let Some(last) = self.gaps.last_mut() {
            assert!(
                last.output_offset <= gap.output_offset,
                "source edit map gaps must be emitted in output order"
            );
            if last.output_offset == gap.output_offset {
                last.original_left = last.original_left.min(gap.original_left);
                last.original_right = last.original_right.max(gap.original_right);
                return;
            }
        }
        self.gaps.push(gap);
    }

    fn finish(
        self,
        output_len: usize,
        checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
    ) -> OperationControlResult<SourceEditMap> {
        debug_assert_eq!(self.output_len, output_len);
        let segments = self.segments;
        let mut unmapped_output_ranges = Vec::new();
        for segment in &segments {
            checkpoints.processed_item()?;
            if segment.mapping == SegmentMapping::Unmapped {
                unmapped_output_ranges.push(segment.output.clone());
            }
        }
        Ok(SourceEditMap {
            original_len: self.original_len,
            output_len,
            segments,
            gaps: self.gaps,
            unmapped_output_ranges,
            #[cfg(test)]
            scan_stats: self.cursor.scan_stats,
        })
    }
}

fn edits_are_sorted_controlled(
    edits: &[SourceEdit],
    checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
) -> OperationControlResult<bool> {
    for pair in edits.windows(2) {
        checkpoints.processed_item()?;
        if (pair[0].range.start, pair[0].range.end) > (pair[1].range.start, pair[1].range.end) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn assert_valid_edits_controlled(
    source: &str,
    edits: &[SourceEdit],
    checkpoints: &mut ControlledEditRebuildCheckpoints<'_>,
) -> OperationControlResult<(usize, usize)> {
    let mut cursor = 0usize;
    let mut replacement_bytes = 0usize;
    let mut removed_bytes = 0usize;
    for edit in edits {
        checkpoints.processed_item()?;
        assert!(edit.range.start <= edit.range.end, "reversed source edit");
        assert!(edit.range.start >= cursor, "overlapping source edits");
        assert!(edit.range.end <= source.len(), "source edit out of bounds");
        assert!(source.is_char_boundary(edit.range.start));
        assert!(source.is_char_boundary(edit.range.end));
        replacement_bytes = replacement_bytes.saturating_add(edit.replacement.len());
        removed_bytes = removed_bytes.saturating_add(edit.range.end - edit.range.start);
        cursor = edit.range.end;
    }
    Ok((replacement_bytes, removed_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn composes_deletions_replacements_and_unicode_without_global_degradation() {
        let original = "前A\r\nREMOVE中#quot;后";
        let mut source = PreprocessedSource::new(original);
        let cr = source.text().find('\r').unwrap();
        source.apply_edits_uncontrolled(vec![SourceEdit::replace(
            cr..cr + 2,
            "\n",
            ReplacementMapping::Boundaries,
        )]);
        let remove = source.text().find("REMOVE").unwrap();
        source.apply_edits_uncontrolled(vec![SourceEdit::delete(remove..remove + "REMOVE".len())]);
        let hash = source.text().find('#').unwrap();
        source.apply_edits_uncontrolled(vec![
            SourceEdit::replace(hash..hash + 1, "ﬂ°", ReplacementMapping::Boundaries),
            SourceEdit::replace(
                hash + "#quot".len()..hash + "#quot;".len(),
                "¶ß",
                ReplacementMapping::Boundaries,
            ),
        ]);

        assert_eq!(source.text(), "前A\n中ﬂ°quot¶ß后");
        let middle = source.text().find('中').unwrap();
        let mapped = source
            .try_map_span(SourceSpan::new(middle, middle + '中'.len_utf8()))
            .unwrap();
        assert_eq!(&original[mapped.start..mapped.end], "中");

        let entity = source.text().find("ﬂ°quot¶ß").unwrap();
        let mapped = source
            .try_map_span(SourceSpan::new(entity, entity + "ﬂ°quot¶ß".len()))
            .unwrap();
        assert_eq!(&original[mapped.start..mapped.end], "#quot;");

        let crossing_deletion = source.text().find('A').unwrap();
        let after_deletion = source.text().find('中').unwrap() + '中'.len_utf8();
        assert_eq!(
            source.try_map_span(SourceSpan::new(crossing_deletion, after_deletion)),
            None
        );
    }

    #[test]
    fn enclosing_mapping_preserves_provenance_across_deleted_indentation() {
        let original = "  first\n  second\n";
        let mut source = PreprocessedSource::new(original);
        let second_indent = original.find("  second").expect("second-line indentation");
        source.apply_edits_uncontrolled(vec![
            SourceEdit::delete(0..2),
            SourceEdit::delete(second_indent..second_indent + 2),
        ]);

        let span = SourceSpan::new(0, "first\nsecond".len());
        assert_eq!(source.try_map_span(span), None);
        let enclosing = source
            .try_map_enclosing_span(span)
            .expect("provenance boundaries remain mappable");
        assert_eq!(&original[enclosing.start..enclosing.end], "first\n  second");
    }

    #[test]
    fn boundary_replacements_only_reject_locally_ambiguous_offsets() {
        let mut source = PreprocessedSource::new("A#x;B");
        source.apply_edits_uncontrolled(vec![
            SourceEdit::replace(1..2, "ﬂ°", ReplacementMapping::Boundaries),
            SourceEdit::replace(3..4, "¶ß", ReplacementMapping::Boundaries),
        ]);
        assert_eq!(source.text(), "Aﬂ°x¶ßB");

        let b = source.text().find('B').unwrap();
        assert_eq!(
            source.try_map_span(SourceSpan::new(b, b + 1)),
            Some(SourceSpan::new(4, 5))
        );
        assert_eq!(source.try_map_span(SourceSpan::new(2, 2)), None);

        let identity = PreprocessedSource::new("A😀B");
        assert_eq!(identity.try_map_span(SourceSpan::new(2, 2)), None);
    }

    #[test]
    fn controlled_text_rebuild_cancellation_preserves_the_previous_source_atomically() {
        let original = format!(
            "{}TAIL",
            "a".repeat(CONTROLLED_EDIT_REBUILD_CHECKPOINT_BYTES * 2)
        );
        let mut source = PreprocessedSource::new(&original);
        let before = source.clone();
        let tail = source.text().len() - "TAIL".len();
        let control = OperationControl::new();
        control.cancel_after_checkpoints(1);

        let result = source.apply_edits(
            vec![SourceEdit::replace(
                tail..tail + "TAIL".len(),
                "tail",
                ReplacementMapping::ExactBytes,
            )],
            &control,
        );

        assert!(result.is_err());
        assert!(control.is_cancelled());
        assert_source_state_eq(&source, &before);
    }

    #[test]
    fn controlled_mapping_rebuild_cancellation_preserves_the_previous_source_atomically() {
        let original = "x".repeat(CONTROLLED_EDIT_REBUILD_CHECKPOINT_ITEMS * 2);
        let mut source = PreprocessedSource::new(&original);
        source.apply_edits_uncontrolled(
            (0..original.len())
                .map(|offset| {
                    SourceEdit::replace(offset..offset + 1, "x", ReplacementMapping::Boundaries)
                })
                .collect(),
        );
        assert!(source.edit_map.segments.len() > CONTROLLED_EDIT_REBUILD_CHECKPOINT_ITEMS);
        assert!(source.text().len() < CONTROLLED_EDIT_REBUILD_CHECKPOINT_BYTES);

        let before = source.clone();
        let control = OperationControl::new();
        control.cancel_after_checkpoints(1);
        let last_byte = source.text().len() - 1;
        let result =
            source.apply_edits(vec![SourceEdit::delete(last_byte..last_byte + 1)], &control);

        assert!(result.is_err());
        assert!(control.is_cancelled());
        assert_source_state_eq(&source, &before);
    }

    #[test]
    fn same_offset_insertions_and_adjacent_deletions_coalesce_gaps_in_order() {
        let mut source = PreprocessedSource::new("aXbYc");
        source.apply_edits_uncontrolled(vec![SourceEdit::delete(1..2)]);

        source.apply_edits_uncontrolled(vec![
            SourceEdit::replace(1..1, "<", ReplacementMapping::Boundaries),
            SourceEdit::replace(1..1, ">", ReplacementMapping::Boundaries),
            SourceEdit::delete(1..2),
            SourceEdit::delete(2..3),
        ]);

        assert_eq!(source.text(), "a<>c");
        assert!(source.edit_map.is_well_formed());
        assert_eq!(source.edit_map.gaps.len(), 2);

        let c = source.text().find('c').unwrap();
        assert_eq!(
            source.try_map_span(SourceSpan::new(c, c + 1)),
            Some(SourceSpan::new(4, 5))
        );
        assert_eq!(source.try_map_span(SourceSpan::new(0, c + 1)), None);
    }

    #[test]
    fn composing_an_edit_inside_generated_text_marks_only_that_region_unmappable() {
        let mut source = PreprocessedSource::new("A#x;B");
        source.apply_edits_uncontrolled(vec![SourceEdit::replace(
            1..2,
            "ﬂ°",
            ReplacementMapping::Boundaries,
        )]);
        let degree = source.text().find('°').unwrap();
        source.apply_edits_uncontrolled(vec![SourceEdit::replace(
            degree..degree + '°'.len_utf8(),
            "!",
            ReplacementMapping::ExactBytes,
        )]);
        assert!(source.edit_map.unmapped_ranges_match_segments());
        assert!(!source.edit_map.unmapped_output_ranges.is_empty());

        let generated = source.text().find('ﬂ').unwrap();
        assert_eq!(
            source.try_map_span(SourceSpan::new(generated, generated + 'ﬂ'.len_utf8())),
            None
        );
        let b = source.text().find('B').unwrap();
        assert_eq!(
            source.try_map_span(SourceSpan::new(b, b + 1)),
            Some(SourceSpan::new(4, 5))
        );
    }

    #[test]
    fn crlf_comment_batches_scan_old_map_once_and_preserve_offsets() {
        const MIN_INPUT_BYTES: usize = 1024 * 1024;

        let mut original = String::with_capacity(MIN_INPUT_BYTES + 256);
        let mut pair_count = 0usize;
        while original.len() < MIN_INPUT_BYTES {
            let group = if pair_count.is_multiple_of(2) {
                "even"
            } else {
                "odd"
            };
            write!(
                original,
                "KEEP-{pair_count:05} payload\r\n%% {group}-comment-{pair_count:05} payload\r\n"
            )
            .unwrap();
            pair_count += 1;
        }
        original.push_str("TAIL\r\n");
        assert!(original.len() >= MIN_INPUT_BYTES);

        let mut source = PreprocessedSource::new(&original);
        source.apply_edits_uncontrolled(crlf_normalization_edits(source.text()));
        assert!(!source.text().contains('\r'));
        assert!(
            source.edit_map.unmapped_output_ranges.is_empty(),
            "CRLF boundary mappings must not make every fact lookup scan every line"
        );

        let even_comments = comment_line_deletions(source.text(), "%% even-comment-");
        source.apply_edits_uncontrolled(even_comments);

        let old_segment_count = source.edit_map.segments.len();
        let old_gap_count = source.edit_map.gaps.len();
        let odd_comments = comment_line_deletions(source.text(), "%% odd-comment-");
        let odd_comment_count = odd_comments.len();
        assert!(odd_comment_count > 8_000);
        assert!(old_gap_count > 8_000);

        source.apply_edits_uncontrolled(odd_comments);

        let scans = source.edit_map.scan_stats;
        assert_eq!(scans.copy_ranges, odd_comment_count + 1);
        assert!(
            scans.copy_segment_steps <= old_segment_count + scans.copy_ranges,
            "copying revisited old segments: {scans:?} for {old_segment_count} segments"
        );
        assert!(
            scans.copy_gap_steps <= old_gap_count + scans.copy_ranges,
            "copying revisited old gaps: {scans:?} for {old_gap_count} gaps"
        );
        assert!(
            scans.lookup_segment_advances <= old_segment_count,
            "endpoint lookup revisited old segments: {scans:?}"
        );
        assert!(
            scans.lookup_gap_advances <= old_gap_count,
            "endpoint lookup revisited old gaps: {scans:?}"
        );

        assert!(!source.text().contains("%%"));
        for pair_index in [0, pair_count / 2, pair_count - 1] {
            let literal = format!("KEEP-{pair_index:05}");
            assert_literal_mapping(&source, &original, &literal, &literal);
        }

        let first_keep = "KEEP-00000 payload";
        let output_newline = source.text().find(first_keep).unwrap() + first_keep.len();
        let original_newline = original.find(first_keep).unwrap() + first_keep.len();
        assert_eq!(
            source.try_map_span(SourceSpan::new(output_newline, output_newline + 1)),
            Some(SourceSpan::new(original_newline, original_newline + 2))
        );

        let second_keep = source.text().find("KEEP-00001").unwrap();
        assert_eq!(
            source.try_map_span(SourceSpan::new(output_newline, second_keep + 1)),
            None,
            "a span crossing a removed comment must remain unmappable"
        );
    }

    #[test]
    fn seeded_edit_compositions_preserve_exact_monotonic_source_ranges() {
        const SEED: u64 = 0x4d45_524d_414e_5538;
        const CASES: usize = 64;
        let original = "前|KEEP_A|COPY|DELETE|\r\n|#quot;|<span title=\"值😀\">|WIDE|KEEP_B|后";
        let mut rng = DeterministicRng::new(SEED);

        for case in 0..CASES {
            let mut source = PreprocessedSource::new(original);
            for transformation in shuffled_transformations(&mut rng) {
                apply_transformation(&mut source, transformation);
                assert_mapping_invariants(&source, original, case, transformation);
            }

            assert_literal_mapping(&source, original, "KEEP_A", "KEEP_A");
            assert_literal_mapping(&source, original, "copy", "COPY");
            assert_literal_mapping(&source, original, "\n", "\r\n");
            assert_literal_mapping(
                &source,
                original,
                "title=&quot;值😀&quot;",
                "title=\"值😀\"",
            );
            assert_literal_mapping(&source, original, "长值😀", "WIDE");
            assert_literal_mapping(&source, original, "KEEP_B", "KEEP_B");

            let crossing_start = source.text().find("copy").unwrap();
            let crossing_end = source.text().find('\n').unwrap() + 1;
            assert_eq!(
                source.try_map_span(SourceSpan::new(crossing_start, crossing_end)),
                None,
                "case {case}: a span crossing deleted source must stay locally unmappable"
            );

            if case % 2 == 0 {
                assert_literal_mapping(&source, original, "ﬂ°quot¶ß", "#quot;");
            } else {
                let degree = source.text().find('°').unwrap();
                source.apply_edits_uncontrolled(vec![SourceEdit::delete(
                    degree..degree + '°'.len_utf8(),
                )]);
                assert_mapping_invariants(
                    &source,
                    original,
                    case,
                    SeededTransformation::GeneratedInteriorDeletion,
                );
                let generated = source.text().find("ﬂquot¶ß").unwrap();
                assert_eq!(
                    source.try_map_span(SourceSpan::new(generated, generated + "ﬂquot¶ß".len(),)),
                    None,
                    "case {case}: editing generated bytes must not invent an original range"
                );
                assert_literal_mapping(&source, original, "KEEP_B", "KEEP_B");
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum SeededTransformation {
        ExactByteReplacement,
        Deletion,
        CrLfNormalization,
        EntityEncoding,
        QuoteExpansion,
        BoundaryReplacement,
        GeneratedInteriorDeletion,
    }

    struct DeterministicRng(u64);

    impl DeterministicRng {
        fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn index(&mut self, upper: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 as usize) % upper
        }
    }

    fn shuffled_transformations(rng: &mut DeterministicRng) -> [SeededTransformation; 6] {
        let mut transformations = [
            SeededTransformation::ExactByteReplacement,
            SeededTransformation::Deletion,
            SeededTransformation::CrLfNormalization,
            SeededTransformation::EntityEncoding,
            SeededTransformation::QuoteExpansion,
            SeededTransformation::BoundaryReplacement,
        ];
        for index in (1..transformations.len()).rev() {
            transformations.swap(index, rng.index(index + 1));
        }
        transformations
    }

    fn apply_transformation(source: &mut PreprocessedSource, transformation: SeededTransformation) {
        match transformation {
            SeededTransformation::ExactByteReplacement => {
                replace_once(source, "COPY", "copy", ReplacementMapping::ExactBytes);
            }
            SeededTransformation::Deletion => {
                let start = source.text().find("DELETE|").unwrap();
                source.apply_edits_uncontrolled(vec![SourceEdit::delete(
                    start..start + "DELETE|".len(),
                )]);
            }
            SeededTransformation::CrLfNormalization => {
                replace_once(source, "\r\n", "\n", ReplacementMapping::Boundaries);
            }
            SeededTransformation::EntityEncoding => {
                let start = source.text().find("#quot;").unwrap();
                source.apply_edits_uncontrolled(vec![
                    SourceEdit::replace(start..start + 1, "ﬂ°", ReplacementMapping::Boundaries),
                    SourceEdit::replace(
                        start + "#quot".len()..start + "#quot;".len(),
                        "¶ß",
                        ReplacementMapping::Boundaries,
                    ),
                ]);
            }
            SeededTransformation::QuoteExpansion => {
                let edits = source
                    .text()
                    .match_indices('"')
                    .map(|(start, _)| {
                        SourceEdit::replace(
                            start..start + 1,
                            "&quot;",
                            ReplacementMapping::Boundaries,
                        )
                    })
                    .collect();
                source.apply_edits_uncontrolled(edits);
            }
            SeededTransformation::BoundaryReplacement => {
                replace_once(source, "WIDE", "长值😀", ReplacementMapping::Boundaries);
            }
            SeededTransformation::GeneratedInteriorDeletion => unreachable!(),
        }
    }

    fn replace_once(
        source: &mut PreprocessedSource,
        needle: &str,
        replacement: &str,
        mapping: ReplacementMapping,
    ) {
        let start = source.text().find(needle).unwrap();
        source.apply_edits_uncontrolled(vec![SourceEdit::replace(
            start..start + needle.len(),
            replacement,
            mapping,
        )]);
    }

    fn crlf_normalization_edits(text: &str) -> Vec<SourceEdit> {
        text.match_indices("\r\n")
            .map(|(start, _)| {
                SourceEdit::replace(start..start + 2, "\n", ReplacementMapping::Boundaries)
            })
            .collect()
    }

    fn comment_line_deletions(text: &str, prefix: &str) -> Vec<SourceEdit> {
        let mut line_start = 0usize;
        text.split_inclusive('\n')
            .filter_map(|line| {
                let range = line_start..line_start + line.len();
                line_start = range.end;
                line.starts_with(prefix).then(|| SourceEdit::delete(range))
            })
            .collect()
    }

    fn assert_literal_mapping(
        source: &PreprocessedSource,
        original: &str,
        output_literal: &str,
        original_literal: &str,
    ) {
        let output_start = source.text().find(output_literal).unwrap();
        let original_start = original.find(original_literal).unwrap();
        assert_eq!(
            source.try_map_span(SourceSpan::new(
                output_start,
                output_start + output_literal.len(),
            )),
            Some(SourceSpan::new(
                original_start,
                original_start + original_literal.len(),
            )),
            "{output_literal:?} must map exactly to {original_literal:?}"
        );
    }

    fn assert_source_state_eq(actual: &PreprocessedSource, expected: &PreprocessedSource) {
        assert_eq!(actual.text, expected.text);
        assert_eq!(actual.edit_map.original_len, expected.edit_map.original_len);
        assert_eq!(actual.edit_map.output_len, expected.edit_map.output_len);
        assert_eq!(actual.edit_map.segments, expected.edit_map.segments);
        assert_eq!(actual.edit_map.gaps, expected.edit_map.gaps);
        assert_eq!(
            actual.edit_map.unmapped_output_ranges,
            expected.edit_map.unmapped_output_ranges
        );
        assert_eq!(actual.edit_map.scan_stats, expected.edit_map.scan_stats);
        assert_eq!(
            actual.global_expected_syntax,
            expected.global_expected_syntax
        );
        assert_eq!(
            actual.global_directive_prefixes,
            expected.global_directive_prefixes
        );
        assert_eq!(
            actual.recovered_incomplete_directive,
            expected.recovered_incomplete_directive
        );
    }

    fn assert_mapping_invariants(
        source: &PreprocessedSource,
        original: &str,
        case: usize,
        transformation: SeededTransformation,
    ) {
        assert!(
            source.edit_map.is_well_formed(),
            "case {case} after {transformation:?}: malformed composed edit map"
        );
        assert_eq!(source.edit_map.original_len, original.len());
        assert_eq!(source.edit_map.output_len, source.text().len());

        let mut boundaries = source
            .text()
            .char_indices()
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        boundaries.push(source.text().len());

        let mut previous_original_offset = None;
        for &offset in &boundaries {
            if let Some(mapped) = source.try_map_span(SourceSpan::new(offset, offset)) {
                assert_eq!(mapped.start, mapped.end);
                assert!(mapped.end <= original.len());
                assert!(original.is_char_boundary(mapped.start));
                if let Some(previous) = previous_original_offset {
                    assert!(
                        previous <= mapped.start,
                        "case {case} after {transformation:?}: output boundary mapping regressed"
                    );
                }
                previous_original_offset = Some(mapped.start);
            }
        }

        for (start_index, &start) in boundaries.iter().enumerate() {
            for &end in &boundaries[start_index..] {
                let Some(mapped) = source.try_map_span(SourceSpan::new(start, end)) else {
                    continue;
                };
                assert!(mapped.start <= mapped.end);
                assert!(mapped.end <= original.len());
                assert!(original.is_char_boundary(mapped.start));
                assert!(original.is_char_boundary(mapped.end));
            }
        }

        assert_eq!(
            source.try_map_span(SourceSpan::new(0, source.text().len() + 1)),
            None
        );
        let emoji = source.text().find('😀').unwrap();
        assert_eq!(
            source.try_map_span(SourceSpan::new(emoji + 1, emoji + 1)),
            None
        );
    }
}
