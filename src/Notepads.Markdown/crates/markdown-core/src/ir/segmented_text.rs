// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Owned UTF8 fragments with virtual coordinates and true single-segment borrows.

use super::TextSlice;
use crate::error::{CoreError, LimitKind};
use crate::memory::{BuildOwnerVec, BuildString, BuildTextChunk, FrozenRows, FrozenText};
use crate::policy::{Operation, OwnedWork};

struct Segment {
    start: usize,
    text: FrozenText,
}
pub(super) struct SegmentedText {
    segments: FrozenRows<Segment>,
    len: usize,
}
impl SegmentedText {
    #[cfg(test)]
    pub(super) fn from_frozen<'id>(
        text: FrozenText,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        let len = text.as_str().len();
        let mut segments = BuildOwnerVec::new(1, LimitKind::DecodedBytes, work)?;
        segments.push(Segment { start: 0, text }, work)?;
        Ok(Self {
            segments: segments.freeze(work)?,
            len,
        })
    }
    // Plain immutable inspection is not a budgeted engine read. Production
    // validation, index/query and reader paths use text_charged below.
    pub(super) fn text(&self, value: TextSlice) -> Result<&str, CoreError> {
        self.text_impl(value, None)
    }
    pub(super) fn text_charged(
        &self,
        value: TextSlice,
        work: &mut TextWork<'_, '_, '_>,
    ) -> Result<&str, CoreError> {
        work.check()?;
        self.text_impl(value, Some(work))
            .map_err(|error| work.fail(error))
    }
    fn text_impl(
        &self,
        value: TextSlice,
        mut work: Option<&mut TextWork<'_, '_, '_>>,
    ) -> Result<&str, CoreError> {
        let start = value.start();
        let end = start
            .checked_add(value.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
        if end > self.len {
            return Err(CoreError::InvalidProvenance);
        }
        let segments = self.segments.as_slice();
        if segments.is_empty() {
            return if start == 0 && end == 0 {
                Ok("")
            } else {
                Err(CoreError::InvalidProvenance)
            };
        }
        let mut low = 0;
        let mut high = segments.len();
        let mut selected = None;
        while low < high {
            let mid = low + (high - low) / 2;
            if let Some(work) = work.as_deref_mut() {
                work.segment()?;
            }
            let segment = &segments[mid];
            if segment.start <= start {
                selected = Some(segment);
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        // The candidate is the actual immutable descriptor read and charged
        // during comparison. Reuse its borrow instead of indexing it again.
        let segment = selected.ok_or(CoreError::InvalidProvenance)?;
        let local = start
            .checked_sub(segment.start)
            .ok_or(CoreError::InvalidProvenance)?;
        let local_end = local
            .checked_add(value.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
        let text = segment.text.as_str();
        if let Some(work) = work {
            work.boundaries(text, local, local_end)?;
        }
        // One actual bounds/UTF8 check and one directory search. The returned
        // immutable slice is reused by the semantic caller.
        text.get(local..local_end)
            .ok_or(CoreError::InvalidProvenance)
    }
    #[cfg(test)]
    pub(super) fn directory_record_bytes(&self) -> usize {
        self.segments.as_slice().first().map_or(0, size_of_val)
    }
    #[cfg(test)]
    pub(super) fn segments(&self) -> impl Iterator<Item = &str> {
        self.segments.as_slice().iter().map(|s| s.text.as_str())
    }
}

// A short borrow of the existing authority, never a new quota or fault owner.
pub(super) enum TextWork<'call, 'id, 'op> {
    Query(&'call mut Operation),
    Build(&'call mut OwnedWork<'id, 'op>),
}
impl TextWork<'_, '_, '_> {
    fn check(&mut self) -> Result<(), CoreError> {
        match self {
            Self::Query(work) => work.checkpoint(),
            Self::Build(work) => work.check(),
        }
    }
    pub(super) fn fail(&mut self, error: CoreError) -> CoreError {
        match self {
            Self::Query(work) => work.fail(error),
            Self::Build(work) => work.fail(error),
        }
    }
    fn segment(&mut self) -> Result<(), CoreError> {
        // Same actual owner-record read/comparison as BuildOwnerVec::with.
        match self {
            Self::Query(work) => {
                work.events(1)?;
                work.scan(size_of::<Segment>() as u64)
            }
            Self::Build(work) => {
                work.events(1)?;
                work.scan(size_of::<Segment>() as u64)
            }
        }
    }
    pub(super) fn boundaries(
        &mut self,
        text: &str,
        start: usize,
        end: usize,
    ) -> Result<(), CoreError> {
        let bytes =
            u64::from(start > 0 && start < text.len()) + u64::from(end > 0 && end < text.len());
        match self {
            Self::Query(work) => work.scan(bytes),
            Self::Build(work) => work.scan(bytes),
        }
    }
}

pub(super) struct SegmentBuilder<'id> {
    segments: BuildOwnerVec<'id, Segment>,
    active: Option<BuildString<'id>>,
    active_start: usize,
    active_capacity: usize,
    len: usize,
}
impl<'id> SegmentBuilder<'id> {
    pub(super) fn new(work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        let limit = work
            .model_limits()
            .ok_or_else(|| work.fail(CoreError::UnsupportedConfiguration))?
            .decoded_bytes;
        Ok(Self {
            segments: BuildOwnerVec::new(limit, LimitKind::DecodedBytes, work)?,
            active: None,
            active_start: 0,
            active_capacity: 0,
            len: 0,
        })
    }
    fn seal_active(&mut self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        if let Some(text) = self.active.take() {
            self.segments.push(
                Segment {
                    start: self.active_start,
                    text: text.freeze(work)?,
                },
                work,
            )?;
        }
        self.active_capacity = 0;
        Ok(())
    }
    pub(super) fn append_chunks(
        &mut self,
        len: usize,
        work: &mut OwnedWork<'id, '_>,
        mut chunk: impl FnMut(usize, &mut OwnedWork<'id, '_>) -> Result<BuildTextChunk, CoreError>,
    ) -> Result<TextSlice, CoreError> {
        let result = super::storage::guarded_callback(work, |work| {
            let start = self.len;
            let end = start
                .checked_add(len)
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
            u32::try_from(end).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
            let reference = TextSlice {
                start: start as u32,
                len: len as u32,
            };
            // Native produced bytes and the final pool have distinct existing
            // checks. Copying a produced value here must not admit it again.
            let limit = work
                .model_limits()
                .ok_or_else(|| work.fail(CoreError::UnsupportedConfiguration))?
                .decoded_bytes;
            work.check_limit(LimitKind::DecodedBytes, end as u64, limit)?;
            if len == 0 {
                return Ok(reference);
            }
            const PAGE: usize = 4096;
            let available = self
                .active
                .as_ref()
                .map_or(0, |text| self.active_capacity - text.len());
            if available < len {
                self.seal_active(work)?;
                self.active_start = start;
                self.active_capacity = PAGE.max(len);
                self.active = Some(BuildString::auxiliary(self.active_capacity, work)?);
            }
            let mut offset = 0;
            while offset < len {
                work.events(1)?;
                let value = chunk(offset, work).map_err(|error| work.fail(error))?;
                if value.len() == 0 || value.len() > len - offset {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                self.active
                    .as_mut()
                    .ok_or_else(|| work.fail(CoreError::InternalInvariant))?
                    .append(value.as_str(), work)?;
                offset += value.len();
            }
            self.len = end;
            if len > PAGE {
                self.seal_active(work)?;
            }
            Ok(reference)
        });
        result.map_err(|error| work.fail(error))
    }
    fn locate(
        &self,
        value: TextSlice,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(Option<usize>, usize, usize), CoreError> {
        let end = value
            .start()
            .checked_add(value.len())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        if end > self.len {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        if let Some(active) = &self.active
            && value.start() >= self.active_start
        {
            let start = value.start() - self.active_start;
            let end = start
                .checked_add(value.len())
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
            validate_active(active, start, end, work)?;
            return Ok((None, start, end));
        }
        let mut low = 0;
        let mut high = self.segments.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let start = self
                .segments
                .with(mid, work, |segment, _| Ok(segment.start))?;
            if start <= value.start() {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let index = low
            .checked_sub(1)
            .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
        self.segments.with(index, work, |segment, work| {
            let start = value.start() - segment.start;
            let end = start
                .checked_add(value.len())
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
            work.scan(2)?;
            if segment.text.as_str().get(start..end).is_none() {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            Ok((Some(index), start, end))
        })
    }
    pub(super) fn validate(
        &self,
        value: TextSlice,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        if self.len == 0 && value.start() == 0 && value.is_empty() {
            work.check()
        } else {
            self.locate(value, work).map(|_| ())
        }
    }
    pub(super) fn chunk(
        &self,
        value: TextSlice,
        offset: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<BuildTextChunk, CoreError> {
        if self.len == 0 && value.start() == 0 && value.is_empty() && offset == 0 {
            return read_chunk("", 0, 0, work);
        }
        let (index, start, end) = self.locate(value, work)?;
        let start = start
            .checked_add(offset)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        match index {
            Some(index) => self.segments.with(index, work, |segment, work| {
                read_chunk(segment.text.as_str(), start, end, work)
            }),
            None => read_active(
                self.active
                    .as_ref()
                    .ok_or_else(|| work.fail(CoreError::InternalInvariant))?,
                start,
                end,
                work,
            ),
        }
    }
    pub(super) fn freeze(
        mut self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<SegmentedText, CoreError> {
        self.seal_active(work)?;
        Ok(SegmentedText {
            segments: self.segments.freeze(work)?,
            len: self.len,
        })
    }
}

fn validate_active<'id>(
    text: &BuildString<'id>,
    start: usize,
    end: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    struct Read<'a, 'id> {
        text: &'a BuildString<'id>,
        span: crate::identity::SourceSpan,
    }
    impl<'id> crate::memory::BuildKernel<'id> for Read<'_, 'id> {
        type Output = ();
        fn run<'scope>(
            &'scope mut self,
            window: &mut crate::memory::BuildWindow<'scope, '_, 'id, '_>,
        ) -> Result<crate::memory::BuildStep<()>, CoreError> {
            use crate::memory::BuildStep;
            match window.read_string(self.text)? {
                BuildStep::Ready(text) => text.validate_span(self.span, window),
                BuildStep::Full => Ok(BuildStep::Full),
                BuildStep::NeedCapacity(n) => Ok(BuildStep::NeedCapacity(n)),
            }
        }
    }
    work.scan(2)?;
    let span =
        crate::identity::SourceSpan::new(start as u64, end as u64).map_err(|e| work.fail(e))?;
    let mut read = Read { text, span };
    loop {
        match crate::memory::run_window(&mut read, work)? {
            crate::memory::BuildStep::Ready(()) => return Ok(()),
            crate::memory::BuildStep::Full => {}
            crate::memory::BuildStep::NeedCapacity(_) => {
                return Err(work.fail(CoreError::InternalInvariant));
            }
        }
    }
}
fn read_active<'id>(
    text: &BuildString<'id>,
    start: usize,
    end: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<BuildTextChunk, CoreError> {
    struct Read<'a, 'id> {
        text: &'a BuildString<'id>,
        start: usize,
        end: usize,
    }
    impl<'id> crate::memory::BuildKernel<'id> for Read<'_, 'id> {
        type Output = BuildTextChunk;
        fn run<'scope>(
            &'scope mut self,
            window: &mut crate::memory::BuildWindow<'scope, '_, 'id, '_>,
        ) -> Result<crate::memory::BuildStep<Self::Output>, CoreError> {
            use crate::memory::BuildStep;
            match window.read_string(self.text)? {
                BuildStep::Ready(text) => text.chunk_next(self.start, self.end, 256, window),
                BuildStep::Full => Ok(BuildStep::Full),
                BuildStep::NeedCapacity(n) => Ok(BuildStep::NeedCapacity(n)),
            }
        }
    }
    let mut read = Read { text, start, end };
    loop {
        match crate::memory::run_window(&mut read, work)? {
            crate::memory::BuildStep::Ready(value) => return Ok(value),
            crate::memory::BuildStep::Full => {}
            crate::memory::BuildStep::NeedCapacity(_) => {
                return Err(work.fail(CoreError::InternalInvariant));
            }
        }
    }
}

pub(crate) fn read_chunk<'id>(
    text: &str,
    start: usize,
    end: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<BuildTextChunk, CoreError> {
    struct Read<'a> {
        text: &'a str,
        start: usize,
        end: usize,
    }
    impl<'id> crate::memory::BuildKernel<'id> for Read<'_> {
        type Output = BuildTextChunk;
        fn run<'scope>(
            &'scope mut self,
            window: &mut crate::memory::BuildWindow<'scope, '_, 'id, '_>,
        ) -> Result<crate::memory::BuildStep<Self::Output>, CoreError> {
            use crate::memory::BuildStep;
            match window.source_text(self.text)? {
                BuildStep::Ready(text) => text.chunk_next(self.start, self.end, 256, window),
                BuildStep::Full => Ok(BuildStep::Full),
                BuildStep::NeedCapacity(n) => Ok(BuildStep::NeedCapacity(n)),
            }
        }
    }
    let mut read = Read { text, start, end };
    loop {
        match crate::memory::run_window(&mut read, work)? {
            crate::memory::BuildStep::Ready(value) => return Ok(value),
            crate::memory::BuildStep::Full => {}
            crate::memory::BuildStep::NeedCapacity(_) => {
                return Err(work.fail(CoreError::InternalInvariant));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{
        CancellationToken, ModelLimits, SourceBudget, SourceLimits, construction_scope,
    };

    fn eight_segments<'id>(work: &mut OwnedWork<'id, '_>) -> SegmentBuilder<'id> {
        let mut text = SegmentBuilder::new(work).unwrap();
        let value = "a".repeat(5000);
        for _ in 0..8 {
            text.append_chunks(value.len(), work, |at, work| {
                read_chunk(&value, at, value.len(), work)
            })
            .unwrap();
        }
        assert_eq!(text.segments.len(), 8);
        assert!(text.active.is_none());
        text
    }
    fn interior() -> TextSlice {
        TextSlice {
            start: 15001,
            len: 2,
        }
    }

    #[test]
    fn builder_and_frozen_directory_charge_each_real_read_once() {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        let text = construction_scope(&mut operation, |mut work| {
            let text = eight_segments(&mut work);
            let before = work.stats().unwrap();
            text.validate(interior(), &mut work).unwrap();
            let after = work.stats().unwrap();
            // Binary-search records 4, 2, 3, then selected record 3. Both
            // interior UTF8 boundaries are checked exactly once.
            assert_eq!(after.events() - before.events(), 4);
            assert_eq!(
                after.scanned_bytes() - before.scanned_bytes(),
                4 * size_of::<Segment>() as u64 + 2
            );
            assert_eq!(after.copied_bytes(), before.copied_bytes());
            let text = text.freeze(&mut work).unwrap();
            let before = work.stats().unwrap();
            assert_eq!(
                text.text_charged(interior(), &mut TextWork::Build(&mut work))
                    .unwrap(),
                "aa"
            );
            let after = work.stats().unwrap();
            assert_eq!(after.events() - before.events(), 3);
            assert_eq!(
                after.scanned_bytes() - before.scanned_bytes(),
                3 * size_of::<Segment>() as u64 + 2
            );
            assert_eq!(after.copied_bytes(), before.copied_bytes());
            work.finish().unwrap();
            text
        });
        let mut query = budget.begin(CancellationToken::new());
        assert_eq!(
            text.text_charged(interior(), &mut TextWork::Query(&mut query))
                .unwrap(),
            "aa"
        );
        let stats = query.stats().unwrap();
        assert_eq!(stats.events(), 3);
        assert_eq!(stats.scanned_bytes(), 3 * size_of::<Segment>() as u64 + 2);
        assert_eq!(stats.copied_bytes(), 0);
        assert_eq!(
            stats.charged_units(),
            3 * (size_of::<Segment>() as u64 + 1) + 2
        );
        drop(text);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn directory_interruptions_preserve_original_limit_first_fault_and_release() {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        let text = construction_scope(&mut operation, |mut work| {
            let text = eight_segments(&mut work).freeze(&mut work).unwrap();
            work.finish().unwrap();
            text
        });
        let record = size_of::<Segment>() as u64 + 1;
        // Interrupt the first event/read, a middle search, last search read,
        // and each endpoint admission under the unchanged default Work limit.
        for remaining in [0, 1, record - 1, record, 3 * record, 3 * record + 1] {
            for owned in [false, true] {
                let cancel = CancellationToken::new();
                let mut operation = budget.begin(cancel.clone());
                let mut spend = budget.limits().max_work - remaining;
                while spend != 0 {
                    let chunk = spend.min(crate::policy::CHECKPOINT_BYTES);
                    operation.scan(chunk).unwrap();
                    spend -= chunk;
                }
                let verify = |error: CoreError| {
                    assert!(matches!(error, CoreError::ResourceLimit {
                        kind: LimitKind::Work, limit, ..
                    } if limit == budget.limits().max_work));
                };
                if owned {
                    construction_scope(&mut operation, |mut work| {
                        let error = text
                            .text_charged(interior(), &mut TextWork::Build(&mut work))
                            .unwrap_err();
                        verify(error);
                        cancel.cancel();
                        assert_eq!(
                            text.text_charged(interior(), &mut TextWork::Build(&mut work)),
                            Err(error)
                        );
                        assert_eq!(work.finish(), Err(error));
                    });
                } else {
                    let error = text
                        .text_charged(interior(), &mut TextWork::Query(&mut operation))
                        .unwrap_err();
                    verify(error);
                    cancel.cancel();
                    assert_eq!(
                        text.text_charged(interior(), &mut TextWork::Query(&mut operation)),
                        Err(error)
                    );
                    assert_eq!(operation.first_fault(), Some(error));
                }
            }
        }
        // Cancellation observed before an invalid slice wins. Once an invalid
        // physical cross-segment slice is first, later cancellation cannot mask it.
        for cancel_first in [false, true] {
            let cancel = CancellationToken::new();
            let mut operation = budget.begin(cancel.clone());
            if cancel_first {
                cancel.cancel();
            }
            let invalid = TextSlice {
                start: 4999,
                len: 2,
            };
            let error = text
                .text_charged(invalid, &mut TextWork::Query(&mut operation))
                .unwrap_err();
            assert_eq!(
                error,
                if cancel_first {
                    CoreError::Cancelled
                } else {
                    CoreError::InvalidProvenance
                }
            );
            cancel.cancel();
            assert_eq!(
                text.text_charged(interior(), &mut TextWork::Query(&mut operation)),
                Err(error)
            );
        }
        drop(text);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
    }

    #[test]
    fn builder_search_interrupts_before_each_real_record_and_cannot_freeze() {
        let record = size_of::<Segment>() as u64 + 1;
        for remaining in [0, 1, record, 3 * record, 4 * record + 1] {
            let budget = SourceBudget::model_fixture();
            let cancel = CancellationToken::new();
            let mut operation = budget.begin(cancel.clone());
            construction_scope(&mut operation, |mut work| {
                let text = eight_segments(&mut work);
                let mut spend =
                    work.limits().max_work - work.stats().unwrap().charged_units() - remaining;
                while spend != 0 {
                    let bytes = spend.min(crate::policy::CHECKPOINT_BYTES);
                    work.scan(bytes).unwrap();
                    spend -= bytes;
                }
                let error = text.validate(interior(), &mut work).unwrap_err();
                assert!(matches!(
                    error,
                    CoreError::ResourceLimit {
                        kind: LimitKind::Work,
                        ..
                    }
                ));
                cancel.cancel();
                assert_eq!(text.chunk(interior(), 0, &mut work).err(), Some(error));
                assert_eq!(text.freeze(&mut work).err(), Some(error));
                assert_eq!(work.finish(), Err(error));
            });
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(budget.usage().source_owners, 0);
        }
    }

    #[test]
    fn native_produced_and_final_pool_keep_independent_decoded_limits() {
        let budget = SourceBudget::new_with_model_limits(
            SourceLimits::default(),
            ModelLimits {
                decoded_bytes: 8,
                ..ModelLimits::default()
            },
        )
        .unwrap();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            // This is the exact cumulative admission used by NativeAdmission's
            // DecodedBytes arm. The final pool independently checks its total.
            work.admit_model(LimitKind::DecodedBytes, 8).unwrap();
            let mut text = SegmentBuilder::new(&mut work).unwrap();
            let value = text
                .append_chunks(8, &mut work, |offset, work| {
                    read_chunk("abcdefgh", offset, 8, work)
                })
                .unwrap();
            let text = text.freeze(&mut work).unwrap();
            assert_eq!(text.text(value).unwrap(), "abcdefgh");
            assert_eq!(work.first_fault(), None);
            drop(text);
            work.finish().unwrap();
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);

        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut text = SegmentBuilder::new(&mut work).unwrap();
            text.append_chunks(8, &mut work, |offset, work| {
                read_chunk("abcdefgh", offset, 8, work)
            })
            .unwrap();
            assert_eq!(
                text.append_chunks(1, &mut work, |offset, work| read_chunk(
                    "x", offset, 1, work
                )),
                Err(CoreError::ResourceLimit {
                    kind: LimitKind::DecodedBytes,
                    requested: 9,
                    limit: 8
                })
            );
            let first = work.first_fault();
            drop(text);
            assert_eq!(work.finish().err(), first);
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
    #[test]
    fn packed_small_fragments_and_dedicated_large_text_keep_virtual_utf8_ranges() {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut text = SegmentBuilder::new(&mut work).unwrap();
            let small = "中😀a";
            let mut references = Vec::new();
            for _ in 0..100 {
                references.push(
                    text.append_chunks(small.len(), &mut work, |at, work| {
                        read_chunk(small, at, small.len(), work)
                    })
                    .unwrap(),
                );
            }
            let large = "ab中😀".repeat(700);
            let large_ref = text
                .append_chunks(large.len(), &mut work, |at, work| {
                    read_chunk(&large, at, large.len(), work)
                })
                .unwrap();
            let suffix = text
                .append_chunks(1, &mut work, |at, work| read_chunk("z", at, 1, work))
                .unwrap();
            let text = text.freeze(&mut work).unwrap();
            assert_eq!(text.segments.len(), 3);
            assert_eq!(
                text.segments.as_slice()[0].text.as_str().len(),
                100 * small.len()
            );
            for reference in references {
                assert_eq!(text.text(reference).unwrap(), small);
            }
            assert_eq!(text.text(large_ref).unwrap(), large);
            assert_eq!(text.text(suffix).unwrap(), "z");
            assert_eq!(large_ref.start(), 100 * small.len());
            assert_eq!(suffix.start(), 100 * small.len() + large.len());
            assert_eq!(
                text.text(TextSlice {
                    start: large_ref.start - 1,
                    len: 2
                }),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(
                text.text(TextSlice { start: 1, len: 1 }),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(
                text.text(TextSlice {
                    start: suffix.start + 1,
                    len: 0
                })
                .unwrap(),
                ""
            );
            drop(text);
            work.finish().unwrap();
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn chunk_failure_after_real_writes_preserves_fault_and_releases_segments() {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut text = SegmentBuilder::new(&mut work).unwrap();
            let value = "a".repeat(6000);
            let result = text.append_chunks(value.len(), &mut work, |at, work| {
                if at >= 256 {
                    return Err(CoreError::ArithmeticOverflow);
                }
                read_chunk(&value, at, value.len(), work)
            });
            assert_eq!(result, Err(CoreError::ArithmeticOverflow));
            assert_eq!(text.active.as_ref().unwrap().len(), 256);
            assert!(matches!(
                text.freeze(&mut work),
                Err(CoreError::ArithmeticOverflow)
            ));
            assert_eq!(work.finish(), Err(CoreError::ArithmeticOverflow));
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
    #[test]
    fn caught_chunk_unwind_after_real_write_cannot_reuse_partial_segment() {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut text = SegmentBuilder::new(&mut work).unwrap();
            let value = "a".repeat(6000);
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = text.append_chunks(value.len(), &mut work, |at, work| {
                    assert!(at < 256, "unwind after actual chunk write");
                    read_chunk(&value, at, value.len(), work)
                });
            }));
            assert!(caught.is_err());
            assert_eq!(text.active.as_ref().unwrap().len(), 256);
            assert_eq!(
                text.append_chunks(1, &mut work, |at, work| read_chunk("z", at, 1, work)),
                Err(CoreError::InvalidProvenance)
            );
            assert!(matches!(
                text.freeze(&mut work),
                Err(CoreError::InvalidProvenance)
            ));
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
