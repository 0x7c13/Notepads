// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Handling offsets

use super::read::{FontRead, ReadError};
use crate::control::{
    self, Checked, Ordinary, OrdinaryControl, ReadCtl, ReaderMode, TryFontRead,
    TryFontReadWithArgs, TryResolveNullableOffset, TryResolveOffset,
};
use crate::{font_data::FontData, read::FontReadWithArgs};
use markdown_svg_budget::WorkControl;
use types::{Nullable, Offset16, Offset24, Offset32};

/// Any offset type.
pub trait Offset: Copy {
    fn to_usize(self) -> usize;

    fn non_null(self) -> Option<usize> {
        match self.to_usize() {
            0 => None,
            other => Some(other),
        }
    }
}

macro_rules! impl_offset {
    ($name:ident, $width:literal) => {
        impl Offset for $name {
            #[inline]
            fn to_usize(self) -> usize {
                self.to_u32() as _
            }
        }
    };
}

impl_offset!(Offset16, 2);
impl_offset!(Offset24, 3);
impl_offset!(Offset32, 4);

/// A helper trait providing a 'resolve' method for offset types
pub trait ResolveOffset {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Result<T, ReadError>;

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Result<T, ReadError>;
}

/// A helper trait providing a 'resolve' method for nullable offset types
pub trait ResolveNullableOffset {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Option<Result<T, ReadError>>;

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Option<Result<T, ReadError>>;
}

impl<O: Offset> ResolveNullableOffset for Nullable<O> {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Option<Result<T, ReadError>> {
        match self.offset().resolve(data) {
            Ok(thing) => Some(Ok(thing)),
            Err(ReadError::NullOffset) => None,
            Err(e) => Some(Err(e)),
        }
    }

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Option<Result<T, ReadError>> {
        match self.offset().resolve_with_args(data, args) {
            Ok(thing) => Some(Ok(thing)),
            Err(ReadError::NullOffset) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<O: Offset> ResolveOffset for O {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Result<T, ReadError> {
        control::ordinary(resolve_mode::<Ordinary<T>, _>(
            self.to_usize(),
            data,
            &(),
            &mut OrdinaryControl::new(),
        ))
    }

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Result<T, ReadError> {
        control::ordinary(resolve_mode::<Ordinary<T>, _>(
            self.to_usize(),
            data,
            args,
            &mut OrdinaryControl::new(),
        ))
    }
}

pub(crate) fn resolve_mode<'a, M: ReaderMode<'a, C>, C: WorkControl>(
    offset: usize,
    data: FontData<'a>,
    args: &<M::Target as crate::ReadArgs>::Args,
    control: &mut C,
) -> ReadCtl<M::Target, C> {
    control.checkpoint(0)?;
    if offset == 0 {
        return Ok(Err(ReadError::NullOffset));
    }
    let Some(data) = data.split_off(offset) else {
        return Ok(Err(ReadError::OutOfBounds));
    };
    let result = M::read(data, args, control)?;
    control.checkpoint(0)?;
    Ok(result)
}

macro_rules! checked_offset {
    ($($offset:ty),+ $(,)?) => {$(
        impl control::sealed::Offset for $offset {}
        impl TryResolveOffset for $offset {
            fn try_resolve<'a, T: TryFontRead<'a>, C: WorkControl>(
                &self, data: FontData<'a>, control: &mut C,
            ) -> ReadCtl<T,C> {
                resolve_mode::<Checked<T>, C>(self.to_usize(), data, &(), control)
            }
            fn try_resolve_with_args<'a, T: TryFontReadWithArgs<'a>, C: WorkControl>(
                &self, data: FontData<'a>, args: &T::Args, control: &mut C,
            ) -> ReadCtl<T,C> {
                resolve_mode::<Checked<T>, C>(self.to_usize(), data, args, control)
            }
        }
        impl control::sealed::NullableOffset for Nullable<$offset> {}
        impl TryResolveNullableOffset for Nullable<$offset> {
            fn try_resolve<'a, T: TryFontRead<'a>, C: WorkControl>(
                &self, data: FontData<'a>, control: &mut C,
            ) -> Result<Option<Result<T,ReadError>>,C::Error> {
                Ok(match self.offset().try_resolve::<T,C>(data, control)? {
                    Err(ReadError::NullOffset) => None,
                    other => Some(other),
                })
            }
            fn try_resolve_with_args<'a, T: TryFontReadWithArgs<'a>, C: WorkControl>(
                &self, data: FontData<'a>, args: &T::Args, control: &mut C,
            ) -> Result<Option<Result<T,ReadError>>,C::Error> {
                Ok(match self.offset().try_resolve_with_args::<T,C>(data, args, control)? {
                    Err(ReadError::NullOffset) => None,
                    other => Some(other),
                })
            }
        }
    )+};
}
checked_offset!(Offset16, Offset24, Offset32);
