// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Arrays of offsets with dynamic resolution
//!
//! This module provides a number of types that wrap arrays of offsets, dynamically
//! resolving individual offsets as they are accessed.

use crate::control::{
    self, Checked, Ordinary, OrdinaryControl, ReadCtl, ReaderMode, TryFontReadWithArgs,
    TryResolveOffset,
};
use font_types::{BigEndian, Nullable, Offset16, Scalar};
use markdown_svg_budget::WorkControl;

use crate::{FontData, FontReadWithArgs, Offset, ReadArgs, ReadError};

/// An array of offsets that can be resolved on access.
///
/// This bundles up the raw offsets with the data used to resolve them, along
/// with any arguments needed to resolve those offsets; it provides a simple
/// ergonomic interface that unburdens the user from needing to manually
/// determine the appropriate input data and arguments for a raw offset.
#[derive(Clone)]
pub struct ArrayOfOffsets<'a, T: ReadArgs, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<O>],
    data: FontData<'a>,
    args: T::Args,
}

/// An array of nullable offsets that can be resolved on access.
///
/// This is identical to [`ArrayOfOffsets`], except that each offset is
/// allowed to be null.
#[derive(Clone)]
pub struct ArrayOfNullableOffsets<'a, T: ReadArgs, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<Nullable<O>>],
    data: FontData<'a>,
    args: T::Args,
}

impl<'a, T, O> ArrayOfOffsets<'a, T, O>
where
    O: Scalar,
    T: ReadArgs,
{
    pub(crate) fn new(offsets: &'a [BigEndian<O>], data: FontData<'a>, args: T::Args) -> Self {
        Self {
            offsets,
            data,
            args,
        }
    }
}

impl<'a, T, O> ArrayOfOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadArgs + FontReadWithArgs<'a>,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    ///
    /// Note: if the index is invalid this will return the `InvalidCollectionIndex`
    /// error variant instead of `None`.
    pub fn get(&self, idx: usize) -> Result<T, ReadError> {
        control::ordinary(offset_get::<Ordinary<T>, _, O>(
            self.offsets,
            self.data,
            &self.args,
            idx,
            &mut OrdinaryControl::new(),
        ))
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered.
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let offsets = self.offsets;
        let args = self.args;
        let data = self.data;
        let mut index = 0;
        std::iter::from_fn(move || {
            control::ordinary(offset_next::<Ordinary<T>, _, O>(
                offsets,
                data,
                &args,
                &mut index,
                &mut OrdinaryControl::new(),
            ))
        })
    }

    /// Iterate over all of the offset targets.
    ///
    /// Offset is treated as nullable and each offset will be resolved as it is encountered.
    pub(crate) fn iter_as_nullable(
        &self,
    ) -> impl Iterator<Item = Option<Result<T, ReadError>>> + 'a {
        self.iter().map(|off| match off {
            Err(ReadError::NullOffset) => None,
            other => Some(other),
        })
    }
}

impl<'a, T, O> ArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadArgs,
{
    pub(crate) fn new(
        offsets: &'a [BigEndian<Nullable<O>>],
        data: FontData<'a>,
        args: T::Args,
    ) -> Self {
        Self {
            offsets,
            data,
            args,
        }
    }
}

impl<'a, T, O> ArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadArgs + FontReadWithArgs<'a>,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    ///
    /// This will return `None` only if the offset *exists*, but is null. if the
    /// provided index does not exist, this will return the `InvalidCollectionIndex`
    /// error variant.
    pub fn get(&self, idx: usize) -> Option<Result<T, ReadError>> {
        control::ordinary(nullable_get::<Ordinary<T>, _, O>(
            self.offsets,
            self.data,
            &self.args,
            idx,
            &mut OrdinaryControl::new(),
        ))
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered.
    pub fn iter(&self) -> impl Iterator<Item = Option<Result<T, ReadError>>> + 'a {
        let offsets = self.offsets;
        let args = self.args;
        let data = self.data;
        let mut index = 0;
        std::iter::from_fn(move || {
            control::ordinary(nullable_next::<Ordinary<T>, _, O>(
                offsets,
                data,
                &args,
                &mut index,
                &mut OrdinaryControl::new(),
            ))
        })
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a, T, O> crate::traversal::SomeArray<'a> for ArrayOfOffsets<'a, T, O>
where
    O: Scalar + Offset + Into<crate::traversal::OffsetType>,
    T: ReadArgs + FontReadWithArgs<'a> + crate::traversal::SomeTable<'a> + 'a,
    T::Args: Copy + 'static,
{
    fn type_name(&self) -> &str {
        let full_name = std::any::type_name::<T>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn get(&self, idx: usize) -> Option<crate::traversal::FieldType<'a>> {
        let off = self.offsets.get(idx)?;
        let raw = off.get();
        let result = raw.resolve_with_args::<T>(self.data, &self.args);
        Some(crate::traversal::FieldType::offset(raw, result))
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a, T, O> crate::traversal::SomeArray<'a> for ArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset + Into<crate::traversal::OffsetType> + Clone,
    T: ReadArgs + FontReadWithArgs<'a> + crate::traversal::SomeTable<'a> + 'a,
    T::Args: Copy + 'static,
{
    fn type_name(&self) -> &str {
        let full_name = std::any::type_name::<T>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn get(&self, idx: usize) -> Option<crate::traversal::FieldType<'a>> {
        let off = self.offsets.get(idx)?;
        let raw = off.get();
        let result = raw.resolve_with_args::<T>(self.data, &self.args);
        Some(crate::traversal::FieldType::offset(raw, result))
    }
}

fn offset_get<'a, M: ReaderMode<'a, C>, C: WorkControl, O: Scalar + Offset>(
    offsets: &[BigEndian<O>],
    data: FontData<'a>,
    args: &<M::Target as ReadArgs>::Args,
    index: usize,
    control: &mut C,
) -> ReadCtl<M::Target, C> {
    control.checkpoint(0)?;
    let Some(offset) = offsets.get(index) else {
        return Ok(Err(ReadError::InvalidCollectionIndex(index as _)));
    };
    crate::offset::resolve_mode::<M, C>(offset.get().to_usize(), data, args, control)
}
fn nullable_get<'a, M: ReaderMode<'a, C>, C: WorkControl, O: Scalar + Offset>(
    offsets: &[BigEndian<Nullable<O>>],
    data: FontData<'a>,
    args: &<M::Target as ReadArgs>::Args,
    index: usize,
    control: &mut C,
) -> Result<Option<Result<M::Target, ReadError>>, C::Error> {
    control.checkpoint(0)?;
    let Some(offset) = offsets.get(index) else {
        return Ok(Some(Err(ReadError::InvalidCollectionIndex(index as _))));
    };
    Ok(
        match crate::offset::resolve_mode::<M, C>(
            offset.get().offset().to_usize(),
            data,
            args,
            control,
        )? {
            Err(ReadError::NullOffset) => None,
            other => Some(other),
        },
    )
}
fn offset_next<'a, M: ReaderMode<'a, C>, C: WorkControl, O: Scalar + Offset>(
    offsets: &[BigEndian<O>],
    data: FontData<'a>,
    args: &<M::Target as ReadArgs>::Args,
    index: &mut usize,
    control: &mut C,
) -> Result<Option<Result<M::Target, ReadError>>, C::Error> {
    control.checkpoint(0)?;
    if *index == offsets.len() {
        return Ok(None);
    }
    control.checkpoint(1)?;
    let current = *index;
    *index += 1;
    let result = offset_get::<M, C, O>(offsets, data, args, current, control)?;
    control.checkpoint(0)?;
    Ok(Some(result))
}
fn nullable_next<'a, M: ReaderMode<'a, C>, C: WorkControl, O: Scalar + Offset>(
    offsets: &[BigEndian<Nullable<O>>],
    data: FontData<'a>,
    args: &<M::Target as ReadArgs>::Args,
    index: &mut usize,
    control: &mut C,
) -> Result<Option<Option<Result<M::Target, ReadError>>>, C::Error> {
    control.checkpoint(0)?;
    if *index == offsets.len() {
        return Ok(None);
    }
    control.checkpoint(1)?;
    let current = *index;
    *index += 1;
    let result = nullable_get::<M, C, O>(offsets, data, args, current, control)?;
    control.checkpoint(0)?;
    Ok(Some(result))
}

impl<'a, T: TryFontReadWithArgs<'a>, O: Scalar + TryResolveOffset> ArrayOfOffsets<'a, T, O> {
    pub fn try_get<C: WorkControl>(&self, index: usize, control: &mut C) -> ReadCtl<T, C> {
        offset_get::<Checked<T>, C, O>(self.offsets, self.data, &self.args, index, control)
    }
    pub fn try_iter(&self) -> OffsetArrayCursor<'a, T, O> {
        OffsetArrayCursor {
            offsets: self.offsets,
            data: self.data,
            args: self.args,
            index: 0,
            marker: core::marker::PhantomData,
        }
    }
}
/// Offset cursor with a distinct outside control error.
pub struct OffsetArrayCursor<'a, T: ReadArgs, O: Scalar> {
    offsets: &'a [BigEndian<O>],
    data: FontData<'a>,
    args: T::Args,
    index: usize,
    marker: core::marker::PhantomData<fn() -> T>,
}
impl<'a, T: TryFontReadWithArgs<'a>, O: Scalar + TryResolveOffset> OffsetArrayCursor<'a, T, O> {
    pub fn try_next<C: WorkControl>(
        &mut self,
        control: &mut C,
    ) -> Result<Option<Result<T, ReadError>>, C::Error> {
        offset_next::<Checked<T>, C, O>(
            self.offsets,
            self.data,
            &self.args,
            &mut self.index,
            control,
        )
    }
    pub fn try_next_as_nullable<C: WorkControl>(
        &mut self,
        control: &mut C,
    ) -> Result<Option<Option<Result<T, ReadError>>>, C::Error> {
        Ok(self.try_next(control)?.map(|result| match result {
            Err(ReadError::NullOffset) => None,
            other => Some(other),
        }))
    }
}
impl<'a, T: TryFontReadWithArgs<'a>, O: Scalar + TryResolveOffset>
    ArrayOfNullableOffsets<'a, T, O>
{
    pub fn try_get<C: WorkControl>(
        &self,
        index: usize,
        control: &mut C,
    ) -> Result<Option<Result<T, ReadError>>, C::Error> {
        nullable_get::<Checked<T>, C, O>(self.offsets, self.data, &self.args, index, control)
    }
    pub fn try_iter(&self) -> NullableOffsetArrayCursor<'a, T, O> {
        NullableOffsetArrayCursor {
            offsets: self.offsets,
            data: self.data,
            args: self.args,
            index: 0,
            marker: core::marker::PhantomData,
        }
    }
}
/// EOF, a null item, malformed font data and control failure are four states.
pub struct NullableOffsetArrayCursor<'a, T: ReadArgs, O: Scalar> {
    offsets: &'a [BigEndian<Nullable<O>>],
    data: FontData<'a>,
    args: T::Args,
    index: usize,
    marker: core::marker::PhantomData<fn() -> T>,
}
impl<'a, T: TryFontReadWithArgs<'a>, O: Scalar + TryResolveOffset>
    NullableOffsetArrayCursor<'a, T, O>
{
    pub fn try_next<C: WorkControl>(
        &mut self,
        control: &mut C,
    ) -> Result<Option<Option<Result<T, ReadError>>>, C::Error> {
        nullable_next::<Checked<T>, C, O>(
            self.offsets,
            self.data,
            &self.args,
            &mut self.index,
            control,
        )
    }
}
