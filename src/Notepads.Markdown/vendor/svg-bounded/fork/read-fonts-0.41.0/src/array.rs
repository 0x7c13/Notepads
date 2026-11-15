// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Custom array types

#![deny(clippy::arithmetic_side_effects)]

use bytemuck::AnyBitPattern;
use font_types::FixedSize;

use crate::control::{
    self, Checked, LengthMode, Ordinary, OrdinaryControl, ReadCtl, ReaderMode, SizeMode,
    TryComputeSize, TryFontRead, TryFontReadWithArgs, TryVarSize,
};
use crate::read::{ComputeSize, FontReadWithArgs, ReadArgs, VarSize};
use crate::{FontData, FontRead, ReadError};
use markdown_svg_budget::WorkControl;

/// An array whose items size is not known at compile time.
///
/// This requires the inner type to implement [`FontReadWithArgs`] as well as
/// [`ComputeSize`].
///
/// At runtime, `Args` are provided which will be used to compute the size
/// of each item; this size is then used to compute the positions of the items
/// within the underlying data, from which they will be read lazily.
#[derive(Clone)]
pub struct ComputedArray<'a, T: ReadArgs> {
    // the length of each item
    item_len: usize,
    len: usize,
    data: FontData<'a>,
    args: T::Args,
}

impl<'a, T: ComputeSize> ComputedArray<'a, T> {
    pub fn new(data: FontData<'a>, args: T::Args) -> Result<Self, ReadError> {
        control::ordinary(computed_new::<T, Ordinary<T>, _>(
            data,
            args,
            &mut OrdinaryControl::new(),
        ))
    }

    /// The number of items in the array
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<T: ReadArgs> ReadArgs for ComputedArray<'_, T> {
    type Args = T::Args;
}

impl<'a, T> FontReadWithArgs<'a> for ComputedArray<'a, T>
where
    T: ComputeSize + FontReadWithArgs<'a>,
    T::Args: Copy,
{
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        Self::new(data, *args)
    }
}

impl<T> Default for ComputedArray<'_, T>
where
    T: ReadArgs,
    T::Args: Default,
{
    fn default() -> Self {
        Self {
            item_len: 0,
            len: 0,
            data: Default::default(),
            args: Default::default(),
        }
    }
}

impl<'a, T> ComputedArray<'a, T>
where
    T: FontReadWithArgs<'a>,
    T::Args: Copy + 'static,
{
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut i = 0;
        let data = self.data;
        let args = self.args;
        let item_len = self.item_len;
        let len = self.len;

        std::iter::from_fn(move || {
            control::ordinary(computed_next::<Ordinary<T>, _>(
                data,
                &args,
                item_len,
                len,
                &mut i,
                &mut OrdinaryControl::new(),
            ))
        })
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Result<T, ReadError> {
        control::ordinary(computed_get::<Ordinary<T>, _>(
            self.data,
            &self.args,
            self.item_len,
            idx,
            &mut OrdinaryControl::new(),
        ))
    }
}

impl<T: ReadArgs> std::fmt::Debug for ComputedArray<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("DynSizedArray")
            .field("bytes", &self.data)
            .finish()
    }
}

/// An array of items of non-uniform length.
///
/// Random access into this array cannot be especially efficient, since it requires
/// a linear scan.
pub struct VarLenArray<'a, T> {
    data: FontData<'a>,
    phantom: std::marker::PhantomData<*const T>,
}

impl<'a, T: FontRead<'a> + VarSize> VarLenArray<'a, T> {
    /// Return the item at the provided index.
    ///
    /// # Performance
    ///
    /// Determining the position of an item in this collection requires looking
    /// at all the preceding items; that is, it is `O(n)` instead of `O(1)` as
    /// it would be for a `Vec`.
    ///
    /// As a consequence, calling this method in a loop could potentially be
    /// very slow. If this is something you need to do, it will probably be
    /// much faster to first collect all the items into a `Vec` beforehand,
    /// and then fetch them from there.
    pub fn get(&self, idx: usize) -> Option<Result<T, ReadError>> {
        control::ordinary(variable_get::<Ordinary<T>, _>(
            self.data,
            idx,
            &mut OrdinaryControl::new(),
        ))
    }

    /// Return an iterator over this array's items.
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut data = self.data;
        std::iter::from_fn(move || {
            control::ordinary(variable_next::<Ordinary<T>, _>(
                &mut data,
                &mut OrdinaryControl::new(),
            ))
        })
    }
}

impl<'a, T> FontRead<'a> for VarLenArray<'a, T> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        control::ordinary(<Self as TryFontRead<'a>>::try_read(
            data,
            &mut OrdinaryControl::new(),
        ))
    }
}

impl<T> Default for VarLenArray<'_, T> {
    fn default() -> Self {
        Self {
            data: Default::default(),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<T: AnyBitPattern> ReadArgs for &[T] {
    type Args = u16;
}

impl<'a, T: AnyBitPattern + FixedSize> FontReadWithArgs<'a> for &'a [T] {
    fn read_with_args(data: FontData<'a>, args: &u16) -> Result<Self, ReadError> {
        let len = (*args as usize)
            .checked_mul(T::RAW_BYTE_LEN)
            .ok_or(ReadError::OutOfBounds)?;
        data.read_array(0..len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_test::records::VarLenItem;
    use font_test_data::bebuffer::BeBuffer;

    impl VarSize for VarLenItem<'_> {
        type Size = u32;

        fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
            data.read_at::<u32>(pos).ok().map(|len| len as usize)
        }
    }

    /// HB/HarfRuzz test "shlana_9_006" has a morx table containing a chain
    /// with a length of 0. This caused the VarLenArray iterator to loop
    /// indefinitely.
    #[test]
    fn var_len_iter_with_zero_length_item() {
        // Create a buffer containing three elements where the last
        // has zero length
        let mut buf = BeBuffer::new();
        buf = buf.push(8u32).extend([0u8; 4]);
        buf = buf.push(18u32).extend([0u8; 14]);
        buf = buf.push(0u32);
        let arr: VarLenArray<VarLenItem> = VarLenArray::read(FontData::new(buf.data())).unwrap();
        // Ensure we don't iterate forever and only read two elements (the
        // take() exists so that the test fails rather than hanging if the
        // code regresses in the future)
        assert_eq!(arr.iter().take(10).count(), 2);
    }
}

fn computed_new<'a, T: ComputeSize, M: SizeMode<C, Target = T>, C: WorkControl>(
    data: FontData<'a>,
    args: T::Args,
    control: &mut C,
) -> ReadCtl<ComputedArray<'a, T>, C> {
    control.checkpoint(0)?;
    let item_len = control::font_try!(M::size(&args, control)?);
    let len = data.len().checked_div(item_len).unwrap_or(0);
    control.checkpoint(0)?;
    Ok(Ok(ComputedArray {
        item_len,
        len,
        data,
        args,
    }))
}

fn computed_get<'a, M: ReaderMode<'a, C>, C: WorkControl>(
    data: FontData<'a>,
    args: &<M::Target as ReadArgs>::Args,
    item_len: usize,
    index: usize,
    control: &mut C,
) -> ReadCtl<M::Target, C> {
    control.checkpoint(0)?;
    let Some(start) = index.checked_mul(item_len) else {
        return Ok(Err(ReadError::OutOfBounds));
    };
    let Some(data) = data.split_off(start) else {
        return Ok(Err(ReadError::OutOfBounds));
    };
    let result = M::read(data, args, control)?;
    control.checkpoint(0)?;
    Ok(result)
}

fn computed_next<'a, M: ReaderMode<'a, C>, C: WorkControl>(
    data: FontData<'a>,
    args: &<M::Target as ReadArgs>::Args,
    item_len: usize,
    length: usize,
    index: &mut usize,
    control: &mut C,
) -> Result<Option<Result<M::Target, ReadError>>, C::Error> {
    control.checkpoint(0)?;
    if *index == length {
        return Ok(None);
    }
    control.checkpoint(1)?;
    let Some(start) = item_len.checked_mul(*index) else {
        return Ok(None);
    };
    let Some(next) = index.checked_add(1) else {
        return Ok(None);
    };
    *index = next;
    let Some(data) = data.split_off(start) else {
        return Ok(None);
    };
    let result = M::read(data, args, control)?;
    control.checkpoint(0)?;
    Ok(Some(result))
}

fn variable_get<'a, M: ReaderMode<'a, C> + LengthMode<C>, C: WorkControl>(
    data: FontData<'a>,
    index: usize,
    control: &mut C,
) -> Result<Option<Result<M::Target, ReadError>>, C::Error>
where
    M::Target: FontRead<'a> + ReadArgs<Args = ()>,
{
    control.checkpoint(0)?;
    if data.is_empty() {
        return Ok(None);
    }
    let mut position = 0usize;
    for _ in 0..index {
        control.checkpoint(1)?;
        let Some(length) = M::length(data, position, control)? else {
            return Ok(None);
        };
        let Some(next) = position.checked_add(length) else {
            return Ok(None);
        };
        position = next;
    }
    let Some(data) = data.split_off(position) else {
        return Ok(None);
    };
    let result = M::read(data, &(), control)?;
    control.checkpoint(0)?;
    Ok(Some(result))
}

fn variable_next<'a, M: ReaderMode<'a, C> + LengthMode<C>, C: WorkControl>(
    data: &mut FontData<'a>,
    control: &mut C,
) -> Result<Option<Result<M::Target, ReadError>>, C::Error>
where
    M::Target: FontRead<'a> + ReadArgs<Args = ()>,
{
    control.checkpoint(0)?;
    if data.is_empty() {
        return Ok(None);
    }
    control.checkpoint(1)?;
    let Some(length) = M::length(*data, 0, control)? else {
        return Ok(None);
    };
    // Original iterator terminates on zero lengths, but get above does not.
    if length == 0 {
        return Ok(None);
    }
    let Some(item_data) = data.slice(..length) else {
        return Ok(None);
    };
    let result = M::read(item_data, &(), control)?;
    let Some(next) = data.split_off(length) else {
        return Ok(None);
    };
    *data = next;
    control.checkpoint(0)?;
    Ok(Some(result))
}

impl<'a, T: TryComputeSize> ComputedArray<'a, T> {
    pub fn try_new<C: WorkControl>(
        data: FontData<'a>,
        args: T::Args,
        control: &mut C,
    ) -> ReadCtl<Self, C> {
        computed_new::<T, Checked<T>, C>(data, args, control)
    }
}
impl<'a, T: TryFontReadWithArgs<'a>> ComputedArray<'a, T> {
    pub fn try_get<C: WorkControl>(&self, index: usize, control: &mut C) -> ReadCtl<T, C> {
        computed_get::<Checked<T>, C>(self.data, &self.args, self.item_len, index, control)
    }
    pub fn try_iter(&self) -> ComputedArrayCursor<'a, T> {
        ComputedArrayCursor {
            data: self.data,
            args: self.args,
            item_len: self.item_len,
            length: self.len,
            index: 0,
            marker: core::marker::PhantomData,
        }
    }
}
/// A synchronous cursor. Outside Err must be propagated, never filtered into EOF.
pub struct ComputedArrayCursor<'a, T: ReadArgs> {
    data: FontData<'a>,
    args: T::Args,
    item_len: usize,
    length: usize,
    index: usize,
    marker: core::marker::PhantomData<fn() -> T>,
}
impl<'a, T: TryFontReadWithArgs<'a>> ComputedArrayCursor<'a, T> {
    pub fn try_next<C: WorkControl>(
        &mut self,
        control: &mut C,
    ) -> Result<Option<Result<T, ReadError>>, C::Error> {
        computed_next::<Checked<T>, C>(
            self.data,
            &self.args,
            self.item_len,
            self.length,
            &mut self.index,
            control,
        )
    }
}
impl<'a, T: TryComputeSize + TryFontReadWithArgs<'a>> control::sealed::Reader
    for ComputedArray<'a, T>
{
}
impl<'a, T: TryComputeSize + TryFontReadWithArgs<'a>> TryFontReadWithArgs<'a>
    for ComputedArray<'a, T>
{
    fn try_read_with_args<C: WorkControl>(
        data: FontData<'a>,
        args: &Self::Args,
        control: &mut C,
    ) -> ReadCtl<Self, C> {
        Self::try_new(data, *args, control)
    }
}

impl<'a, T: TryFontRead<'a> + TryVarSize> VarLenArray<'a, T> {
    pub fn try_get<C: WorkControl>(
        &self,
        index: usize,
        control: &mut C,
    ) -> Result<Option<Result<T, ReadError>>, C::Error> {
        variable_get::<Checked<T>, C>(self.data, index, control)
    }
    pub fn try_iter(&self) -> VarLenArrayCursor<'a, T> {
        VarLenArrayCursor {
            data: self.data,
            marker: core::marker::PhantomData,
        }
    }
}
/// Variable records retain the original malformed-length and zero-length stops.
pub struct VarLenArrayCursor<'a, T> {
    data: FontData<'a>,
    marker: core::marker::PhantomData<fn() -> T>,
}
impl<'a, T: TryFontRead<'a> + TryVarSize> VarLenArrayCursor<'a, T> {
    pub fn try_next<C: WorkControl>(
        &mut self,
        control: &mut C,
    ) -> Result<Option<Result<T, ReadError>>, C::Error> {
        variable_next::<Checked<T>, C>(&mut self.data, control)
    }
}
// Reading this view never reads T. Its checked get/next separately require T's
// explicit checked reader and length protocol; an ordinary T cannot escape there.
impl<T> control::sealed::Reader for VarLenArray<'_, T> {}
impl<'a, T> TryFontRead<'a> for VarLenArray<'a, T> {
    fn try_read<C: WorkControl>(data: FontData<'a>, control: &mut C) -> ReadCtl<Self, C> {
        control.checkpoint(0)?;
        Ok(Ok(Self {
            data,
            phantom: core::marker::PhantomData,
        }))
    }
}

impl<T: AnyBitPattern + FixedSize> control::sealed::Reader for &[T] {}
impl<'a, T: AnyBitPattern + FixedSize> TryFontReadWithArgs<'a> for &'a [T] {
    fn try_read_with_args<C: WorkControl>(
        data: FontData<'a>,
        args: &Self::Args,
        control: &mut C,
    ) -> ReadCtl<Self, C> {
        control.checkpoint(0)?;
        let length = match (*args as usize).checked_mul(T::RAW_BYTE_LEN) {
            Some(length) => length,
            None => return Ok(Err(ReadError::OutOfBounds)),
        };
        Ok(data.read_array(0..length))
    }
}
