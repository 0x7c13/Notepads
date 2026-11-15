// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Explicit transport for the private request-bound font implementation.
//!
//! A font error can retain its original recovery behavior. A resource error
//! always exits the current kernel and is never converted into missing data.

use core::convert::Infallible;
use markdown_svg_budget::{NoControl, RequestControl, WorkControl};
use read_fonts::{
    control::{TryFontRead, TryFontReadWithArgs},
    types::Tag,
    FontData, FontRef, ReadError, TopLevelTable,
};

use crate::outline::DrawError;

#[path = "storage.rs"]
pub(crate) mod storage;

#[cfg(test)]
#[path = "request_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "representation_control_tests.rs"]
mod representation_control_tests;

pub(crate) type Ordinary = NoControl<Infallible>;
pub(crate) type KernelResult<T, E> = Result<T, KernelError<E>>;
pub(crate) type DrawResult<T, C> = Result<Result<T, DrawError>, <C as WorkControl>::Error>;

#[derive(Debug)]
pub(crate) enum KernelError<E, F = DrawError> {
    Font(F),
    Control(E),
}

pub(crate) type PsKernelResult<T, E> = Result<T, KernelError<E, read_fonts::ps::error::Error>>;
pub(crate) type PaintKernelResult<T, E> = Result<T, KernelError<E, crate::color::PaintError>>;
pub(crate) type PaintResult<T, C> =
    Result<Result<T, crate::color::PaintError>, <C as WorkControl>::Error>;

impl<E> From<ReadError> for KernelError<E, crate::color::PaintError> {
    fn from(error: ReadError) -> Self {
        Self::Font(error.into())
    }
}
impl<E> From<crate::color::PaintError> for KernelError<E, crate::color::PaintError> {
    fn from(error: crate::color::PaintError) -> Self {
        Self::Font(error)
    }
}
pub(crate) fn paint_read<T, E>(result: Result<Result<T, ReadError>, E>) -> PaintKernelResult<T, E> {
    result.map_err(KernelError::Control)?.map_err(Into::into)
}
pub(crate) fn paint_value<T, E>(
    result: Result<Result<T, crate::color::PaintError>, E>,
) -> PaintKernelResult<T, E> {
    result.map_err(KernelError::Control)?.map_err(Into::into)
}
pub(crate) fn split_paint<T, E>(
    result: PaintKernelResult<T, E>,
) -> Result<Result<T, crate::color::PaintError>, E> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(KernelError::Font(error)) => Ok(Err(error)),
        Err(KernelError::Control(error)) => Err(error),
    }
}
pub(crate) fn paint_checkpoint<C: WorkControl>(
    cx: &mut C,
    work: u64,
) -> PaintKernelResult<(), C::Error> {
    cx.checkpoint(work).map_err(KernelError::Control)
}
impl<E> From<ReadError> for KernelError<E, read_fonts::ps::error::Error> {
    fn from(error: ReadError) -> Self {
        Self::Font(error.into())
    }
}
impl<E> From<read_fonts::ps::error::Error> for KernelError<E, read_fonts::ps::error::Error> {
    fn from(error: read_fonts::ps::error::Error) -> Self {
        Self::Font(error)
    }
}

pub(crate) fn ps_read<T, E>(result: Result<Result<T, ReadError>, E>) -> PsKernelResult<T, E> {
    result.map_err(KernelError::Control)?.map_err(Into::into)
}
pub(crate) fn ps_value<T, E>(
    result: Result<Result<T, read_fonts::ps::error::Error>, E>,
) -> PsKernelResult<T, E> {
    result.map_err(KernelError::Control)?.map_err(Into::into)
}
pub(crate) fn split_ps<T, E>(
    result: PsKernelResult<T, E>,
) -> Result<Result<T, read_fonts::ps::error::Error>, E> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(KernelError::Font(error)) => Ok(Err(error)),
        Err(KernelError::Control(error)) => Err(error),
    }
}
impl<E> From<DrawError> for KernelError<E> {
    fn from(error: DrawError) -> Self {
        Self::Font(error)
    }
}
impl<E> From<ReadError> for KernelError<E> {
    fn from(error: ReadError) -> Self {
        Self::Font(error.into())
    }
}
impl<E> From<read_fonts::ps::error::Error> for KernelError<E> {
    fn from(error: read_fonts::ps::error::Error) -> Self {
        Self::Font(error.into())
    }
}

pub(crate) fn checkpoint<C: WorkControl>(cx: &mut C, work: u64) -> KernelResult<(), C::Error> {
    cx.checkpoint(work).map_err(KernelError::Control)
}

pub(crate) fn read<T, E>(result: Result<Result<T, ReadError>, E>) -> KernelResult<T, E> {
    result.map_err(KernelError::Control)?.map_err(Into::into)
}

pub(crate) fn ps<T, E>(
    result: Result<Result<T, read_fonts::ps::error::Error>, E>,
) -> KernelResult<T, E> {
    result.map_err(KernelError::Control)?.map_err(Into::into)
}

pub(crate) fn split<T, E>(result: KernelResult<T, E>) -> Result<Result<T, DrawError>, E> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(KernelError::Font(error)) => Ok(Err(error)),
        Err(KernelError::Control(error)) => Err(error),
    }
}

pub(crate) fn ordinary<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

pub(crate) fn data<'f, C: WorkControl>(
    font: &FontRef<'f>,
    tag: Tag,
    cx: &mut C,
) -> Result<Result<FontData<'f>, ReadError>, C::Error> {
    cx.checkpoint(0)?;
    let value = font.try_table_data(tag, cx)?;
    cx.checkpoint(0)?;
    Ok(value.ok_or(ReadError::TableIsMissing(tag)))
}

pub(crate) fn table<'f, T: TopLevelTable + TryFontRead<'f>, C: WorkControl>(
    font: &FontRef<'f>,
    cx: &mut C,
) -> Result<Result<T, ReadError>, C::Error> {
    let data = match data(font, T::TAG, cx)? {
        Ok(data) => data,
        Err(error) => return Ok(Err(error)),
    };
    let result = T::try_read(data, cx)?;
    cx.checkpoint(0)?;
    Ok(result)
}

pub(crate) fn hmtx<'f, C: WorkControl>(
    font: &FontRef<'f>,
    cx: &mut C,
) -> read_fonts::control::ReadCtl<read_fonts::tables::hmtx::Hmtx<'f>, C> {
    use read_fonts::control::TryFontReadWithArgs;
    let hhea = match table::<read_fonts::tables::hhea::Hhea, _>(font, cx)? {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    let bytes = match data(font, Tag::new(b"hmtx"), cx)? {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    read_fonts::tables::hmtx::Hmtx::try_read_with_args(bytes, &hhea.number_of_h_metrics(), cx)
}
pub(crate) fn loca<'f, C: WorkControl>(
    font: &FontRef<'f>,
    cx: &mut C,
) -> read_fonts::control::ReadCtl<read_fonts::tables::loca::Loca<'f>, C> {
    use read_fonts::control::TryFontReadWithArgs;
    let head = match table::<read_fonts::tables::head::Head, _>(font, cx)? {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    let bytes = match data(font, Tag::new(b"loca"), cx)? {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    read_fonts::tables::loca::Loca::try_read_with_args(
        bytes,
        &(head.index_to_loc_format() == 1),
        cx,
    )
}

pub(crate) fn sbix<'f, C: WorkControl>(
    font: &FontRef<'f>,
    cx: &mut C,
) -> Result<Result<read_fonts::tables::sbix::Sbix<'f>, ReadError>, C::Error> {
    let maxp = match table::<read_fonts::tables::maxp::Maxp, _>(font, cx)? {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    let bytes = match data(font, Tag::new(b"sbix"), cx)? {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    read_fonts::tables::sbix::Sbix::try_read_with_args(bytes, &maxp.num_glyphs(), cx)
}

/// The two owner-aware RF operations have deliberately different ordinary and
/// request entry types. Keep that distinction here rather than accepting an
/// arbitrary ordinary callback in a request font kernel.
pub(crate) trait FontControl: WorkControl + Sized {
    fn normalize(
        &mut self,
        fvar: &read_fonts::tables::fvar::Fvar<'_>,
        avar: Option<&read_fonts::tables::avar::Avar<'_>>,
        settings: impl IntoIterator<Item = (read_fonts::types::Tag, read_fonts::types::Fixed)>,
        coords: &mut [read_fonts::types::F2Dot14],
    ) -> Result<(), Self::Error>;
    fn validate_glyf(
        &mut self,
        outline: &crate::outline::glyf::Outline<'_>,
    ) -> KernelResult<(), Self::Error>;
    fn add_capacity(&mut self, left: usize, right: usize) -> Result<usize, Self::Error>;
    fn glyf_buffer_size(
        &mut self,
        outline: &crate::outline::glyf::Outline<'_>,
        hint: crate::outline::Hinting,
    ) -> Result<usize, Self::Error>;
    // These modes contain only a borrowed request handle, or NoControl. This
    // copies policy identity, never an account, owner, cache or mutable ledger.
    fn reborrow(&self) -> Self;
    fn temporary<R>(
        &mut self,
        size: usize,
        f: &mut impl FnMut(&mut [u8], &mut Self) -> KernelResult<R, Self::Error>,
    ) -> KernelResult<R, Self::Error>;
    type Small<T: Copy + Default, const N: usize>: storage::SmallBuffer<T, Error = Self::Error>;
    type Owned<T>: storage::Sequence<T, Error = Self::Error>;
    fn small<T: Copy + Default, const N: usize>(
        &mut self,
    ) -> Result<Self::Small<T, N>, Self::Error>;
    fn owned<T>(&mut self) -> Result<Self::Owned<T>, Self::Error>;
    fn stable_sort<T: Copy + Default, const N: usize>(
        &mut self,
        values: &mut Self::Small<T, N>,
        compare: &mut impl FnMut(&T, &T) -> core::cmp::Ordering,
    ) -> Result<(), Self::Error>;
    fn cff_evaluate<'f>(
        &mut self,
        cff: &'f [u8],
        charstrings: &read_fonts::ps::cff::index::Index<'f>,
        global: &read_fonts::ps::cff::index::Index<'f>,
        local: &read_fonts::ps::cff::index::Index<'f>,
        blend: Option<read_fonts::ps::cff::blend::BlendState<'f>>,
        bytes: &[u8],
        sink: &mut impl read_fonts::ps::cs::CommandSink,
    ) -> Result<Result<Option<read_fonts::types::Fixed>, read_fonts::ps::error::Error>, Self::Error>;
    fn phantom_deltas(
        &mut self,
        gvar: &read_fonts::tables::gvar::Gvar<'_>,
        glyf: &read_fonts::tables::glyf::Glyf<'_>,
        loca: &read_fonts::tables::loca::Loca<'_>,
        coords: &[read_fonts::types::F2Dot14],
        glyph: read_fonts::types::GlyphId,
    ) -> Result<
        Result<Option<[read_fonts::types::Point<read_fonts::types::Fixed>; 4]>, ReadError>,
        Self::Error,
    >;
}

impl FontControl for Ordinary {
    fn normalize(
        &mut self,
        fvar: &read_fonts::tables::fvar::Fvar<'_>,
        avar: Option<&read_fonts::tables::avar::Avar<'_>>,
        settings: impl IntoIterator<Item = (read_fonts::types::Tag, read_fonts::types::Fixed)>,
        coords: &mut [read_fonts::types::F2Dot14],
    ) -> Result<(), Infallible> {
        fvar.user_to_normalized(avar, settings, coords);
        Ok(())
    }
    fn validate_glyf(
        &mut self,
        _outline: &crate::outline::glyf::Outline<'_>,
    ) -> KernelResult<(), Infallible> {
        Ok(())
    }
    fn add_capacity(&mut self, left: usize, right: usize) -> Result<usize, Infallible> {
        Ok(left + right)
    }
    fn glyf_buffer_size(
        &mut self,
        outline: &crate::outline::glyf::Outline<'_>,
        hint: crate::outline::Hinting,
    ) -> Result<usize, Infallible> {
        Ok(outline.required_buffer_size(hint))
    }
    fn reborrow(&self) -> Self {
        Self::new()
    }
    fn temporary<R>(
        &mut self,
        size: usize,
        f: &mut impl FnMut(&mut [u8], &mut Self) -> KernelResult<R, Infallible>,
    ) -> KernelResult<R, Infallible> {
        crate::outline::memory::with_temporary_memory(size, |buf| f(buf, self))
    }
    type Small<T: Copy + Default, const N: usize> = crate::collections::SmallVec<T, N>;
    type Owned<T> = alloc::vec::Vec<T>;
    fn small<T: Copy + Default, const N: usize>(
        &mut self,
    ) -> Result<Self::Small<T, N>, Infallible> {
        Ok(crate::collections::SmallVec::new())
    }
    fn owned<T>(&mut self) -> Result<Self::Owned<T>, Infallible> {
        Ok(alloc::vec::Vec::new())
    }
    fn stable_sort<T: Copy + Default, const N: usize>(
        &mut self,
        values: &mut Self::Small<T, N>,
        compare: &mut impl FnMut(&T, &T) -> core::cmp::Ordering,
    ) -> Result<(), Infallible> {
        values.as_mut_slice().sort_by(compare);
        Ok(())
    }
    fn cff_evaluate<'f>(
        &mut self,
        cff: &'f [u8],
        charstrings: &read_fonts::ps::cff::index::Index<'f>,
        global: &read_fonts::ps::cff::index::Index<'f>,
        local: &read_fonts::ps::cff::index::Index<'f>,
        blend: Option<read_fonts::ps::cff::blend::BlendState<'f>>,
        bytes: &[u8],
        sink: &mut impl read_fonts::ps::cs::CommandSink,
    ) -> Result<Result<Option<read_fonts::types::Fixed>, read_fonts::ps::error::Error>, Infallible>
    {
        Ok(read_fonts::ps::cs::evaluate(
            &(cff, charstrings, global, local),
            blend,
            bytes,
            sink,
        ))
    }
    fn phantom_deltas(
        &mut self,
        gvar: &read_fonts::tables::gvar::Gvar<'_>,
        glyf: &read_fonts::tables::glyf::Glyf<'_>,
        loca: &read_fonts::tables::loca::Loca<'_>,
        coords: &[read_fonts::types::F2Dot14],
        glyph: read_fonts::types::GlyphId,
    ) -> Result<
        Result<Option<[read_fonts::types::Point<read_fonts::types::Fixed>; 4]>, ReadError>,
        Infallible,
    > {
        Ok(gvar.phantom_point_deltas(glyf, loca, coords, glyph))
    }
}

impl FontControl for RequestControl<'_> {
    fn normalize(
        &mut self,
        fvar: &read_fonts::tables::fvar::Fvar<'_>,
        avar: Option<&read_fonts::tables::avar::Avar<'_>>,
        settings: impl IntoIterator<Item = (read_fonts::types::Tag, read_fonts::types::Fixed)>,
        coords: &mut [read_fonts::types::F2Dot14],
    ) -> Result<(), Self::Error> {
        self.checkpoint(0)?;
        let result = fvar.try_user_to_normalized(avar, settings, coords, self.0);
        self.checkpoint(0)?;
        result
    }
    fn validate_glyf(
        &mut self,
        outline: &crate::outline::glyf::Outline<'_>,
    ) -> KernelResult<(), Self::Error> {
        checkpoint(self, 0)?;
        outline.ensure_point_count_limit()?;
        Ok(())
    }
    fn add_capacity(&mut self, left: usize, right: usize) -> Result<usize, Self::Error> {
        self.checkpoint(0)?;
        left.checked_add(right).ok_or_else(|| {
            self.0.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Quota,
                markdown_svg_budget::LimitKind::Capacity,
            ))
        })
    }
    fn glyf_buffer_size(
        &mut self,
        outline: &crate::outline::glyf::Outline<'_>,
        hint: crate::outline::Hinting,
    ) -> Result<usize, Self::Error> {
        self.checkpoint(0)?;
        outline.checked_buffer_size(hint).ok_or_else(|| {
            self.0.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Quota,
                markdown_svg_budget::LimitKind::Capacity,
            ))
        })
    }
    fn reborrow(&self) -> Self {
        Self(self.0)
    }
    fn temporary<R>(
        &mut self,
        size: usize,
        f: &mut impl FnMut(&mut [u8], &mut Self) -> KernelResult<R, Self::Error>,
    ) -> KernelResult<R, Self::Error> {
        checkpoint(self, 0)?;
        if size <= 16384 {
            // The upstream bucket helper reaches only its stack branches for
            // this proven bound. Pay the entire initialized bucket, not just
            // the requested prefix exposed to the callback (at most16KiB).
            let bucket = size.max(512).next_power_of_two();
            checkpoint(self, bucket as u64)?;
            let result = crate::outline::memory::with_temporary_memory(size, |buf| f(buf, self));
            checkpoint(self, 0)?;
            return result;
        }
        let mut buffer = markdown_svg_budget::BudgetVec::new(
            self.0.clone(),
            markdown_svg_budget::ResourceKind::Scratch,
        );
        buffer
            .try_resize_copy(size, 0u8)
            .map_err(KernelError::Control)?;
        let result = f(buffer.as_mut_slice(), self);
        drop(buffer); // Real storage is freed before its lease is refunded.
        checkpoint(self, 0)?;
        result
    }
    type Small<T: Copy + Default, const N: usize> = storage::PaidSmall<T, N>;
    type Owned<T> = markdown_svg_budget::BudgetVec<T>;
    fn small<T: Copy + Default, const N: usize>(
        &mut self,
    ) -> Result<Self::Small<T, N>, Self::Error> {
        storage::PaidSmall::new(self.0)
    }
    fn owned<T>(&mut self) -> Result<Self::Owned<T>, Self::Error> {
        self.checkpoint(0)?;
        Ok(markdown_svg_budget::BudgetVec::new(
            self.0.clone(),
            markdown_svg_budget::ResourceKind::Scratch,
        ))
    }
    fn stable_sort<T: Copy + Default, const N: usize>(
        &mut self,
        values: &mut Self::Small<T, N>,
        compare: &mut impl FnMut(&T, &T) -> core::cmp::Ordering,
    ) -> Result<(), Self::Error> {
        validate_resources(values.resources(), self.0)?;
        let mut scratch = markdown_svg_budget::BudgetVec::new(
            self.0.clone(),
            markdown_svg_budget::ResourceKind::Scratch,
        );
        let result = (|| {
            scratch.try_resize_copy(values.len(), T::default())?;
            markdown_svg_budget::slice::stable_sort_by_with_control(
                values,
                scratch.as_mut_slice(),
                self,
                &mut |left, right, _| Ok(compare(left, right)),
            )
        })()
        .map_err(|error| self.0.record(error));
        drop(scratch);
        self.checkpoint(0)?;
        result
    }
    fn cff_evaluate<'f>(
        &mut self,
        cff: &'f [u8],
        charstrings: &read_fonts::ps::cff::index::Index<'f>,
        global: &read_fonts::ps::cff::index::Index<'f>,
        local: &read_fonts::ps::cff::index::Index<'f>,
        blend: Option<read_fonts::ps::cff::blend::BlendState<'f>>,
        bytes: &[u8],
        sink: &mut impl read_fonts::ps::cs::CommandSink,
    ) -> Result<Result<Option<read_fonts::types::Fixed>, read_fonts::ps::error::Error>, Self::Error>
    {
        self.checkpoint(0)?;
        let context =
            read_fonts::ps::cs::RequestCffContext::new(cff, charstrings, global, local, self.0);
        let result = read_fonts::ps::cs::try_evaluate(&context, blend, bytes, sink)?;
        self.checkpoint(0)?;
        Ok(result)
    }
    fn phantom_deltas(
        &mut self,
        gvar: &read_fonts::tables::gvar::Gvar<'_>,
        glyf: &read_fonts::tables::glyf::Glyf<'_>,
        loca: &read_fonts::tables::loca::Loca<'_>,
        coords: &[read_fonts::types::F2Dot14],
        glyph: read_fonts::types::GlyphId,
    ) -> Result<
        Result<Option<[read_fonts::types::Point<read_fonts::types::Fixed>; 4]>, ReadError>,
        Self::Error,
    > {
        gvar.try_phantom_point_deltas(glyf, loca, coords, glyph, self)
    }
}

/// Validate the actual source account before erasing a request-bound view.
/// A failed source cannot be revived through a clean destination account.
pub(crate) fn validate_resources(
    source: &markdown_svg_budget::ResourcesRef,
    target: &markdown_svg_budget::ResourcesRef,
) -> Result<(), markdown_svg_budget::Error> {
    let existing = target.failure().or_else(|| source.failure());
    if !source.is_same_request(target) {
        let invariant = markdown_svg_budget::Error::new(
            markdown_svg_budget::FailureKind::Invariant,
            markdown_svg_budget::LimitKind::None,
        );
        source.record(invariant);
        return Err(target.record(existing.unwrap_or(invariant)));
    }
    if let Some(error) = existing {
        return Err(target.record(error));
    }
    target.checkpoint(0)
}

pub(crate) fn copy<T: Copy, C: WorkControl>(
    target: &mut [T],
    source: &[T],
    cx: &mut C,
) -> Result<(), C::Error> {
    cx.checkpoint(0)?;
    // Preserve slice::copy_from_slice's existing invariant on a length mismatch.
    // All scaler callers derive the two equal ranges from the same point count.
    if target.len() != source.len() {
        target.copy_from_slice(source);
    }
    let window = 256.min(65536 / core::mem::size_of::<T>().max(1)).max(1);
    for (target, source) in target.chunks_mut(window).zip(source.chunks(window)) {
        cx.checkpoint(target.len() as u64)?;
        target.copy_from_slice(source);
    }
    cx.checkpoint(0)
}

/// Callback captures and the temporary allocation both retire before success.
pub(crate) fn temporary<C: FontControl, R>(
    size: usize,
    cx: &mut C,
    mut f: impl FnMut(&mut [u8], &mut C) -> KernelResult<R, C::Error>,
) -> KernelResult<R, C::Error> {
    let result = cx.temporary(size, &mut f);
    drop(f);
    checkpoint(cx, 0)?;
    result
}

pub(crate) fn fill<T: Copy, C: WorkControl>(
    target: &mut [T],
    value: T,
    cx: &mut C,
) -> Result<(), C::Error> {
    cx.checkpoint(0)?;
    let window = 256.min(65536 / core::mem::size_of::<T>().max(1)).max(1);
    for chunk in target.chunks_mut(window) {
        cx.checkpoint(chunk.len() as u64)?;
        chunk.fill(value);
    }
    cx.checkpoint(0)
}

impl<E, F> KernelError<E, F> {
    pub(crate) fn map_font<G>(self, f: impl FnOnce(F) -> G) -> KernelError<E, G> {
        match self {
            Self::Font(e) => KernelError::Font(f(e)),
            Self::Control(e) => KernelError::Control(e),
        }
    }
}
