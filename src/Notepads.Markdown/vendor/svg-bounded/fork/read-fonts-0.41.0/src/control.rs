// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Explicit control for the private font reader fork. The outside Result is
//! always the caller's original control failure; the inside is a font error.

use core::{convert::Infallible, marker::PhantomData};
use markdown_svg_budget::{NoControl, WorkControl};

use crate::{
    ComputeSize, FontData, FontRead, FontReadWithArgs, ReadArgs, ReadError, TopLevelTable, VarSize,
};

/// A borrowed parser result. No allocated error or replacement request exists.
pub type ReadCtl<T, C> = Result<Result<T, ReadError>, <C as WorkControl>::Error>;
/// The same outside control transport for PostScript font errors.
pub type PsCtl<T, C> = Result<Result<T, crate::ps::error::Error>, <C as WorkControl>::Error>;

pub(crate) mod sealed {
    pub trait Reader {}
    pub trait Offset {}
    pub trait NullableOffset {}
    pub trait Provider {}
}

/// A reader whose entire execution uses the supplied control.
pub trait TryFontRead<'a>: FontRead<'a> + sealed::Reader {
    fn try_read<C: WorkControl>(data: FontData<'a>, control: &mut C) -> ReadCtl<Self, C>;
}

/// Controlled counterpart to FontReadWithArgs; Args retains its original Copy bound.
pub trait TryFontReadWithArgs<'a>: FontReadWithArgs<'a> + sealed::Reader {
    fn try_read_with_args<C: WorkControl>(
        data: FontData<'a>,
        args: &Self::Args,
        control: &mut C,
    ) -> ReadCtl<Self, C>;
}

// This bridge requires the checked reader, never just FontRead. Original
// ReadArgs already assigns () to a FontRead type, so no second Args impl exists.
impl<'a, T: TryFontRead<'a>> TryFontReadWithArgs<'a> for T {
    fn try_read_with_args<C: WorkControl>(
        data: FontData<'a>,
        _args: &Self::Args,
        control: &mut C,
    ) -> ReadCtl<Self, C> {
        T::try_read(data, control)
    }
}

/// A possibly nonconstant runtime size computation.
pub trait TryComputeSize: ComputeSize + sealed::Reader {
    fn try_compute_size<C: WorkControl>(args: &Self::Args, control: &mut C) -> ReadCtl<usize, C>;
}

/// Variable-length prefixes and their nonconstant accumulated length.
pub trait TryVarSize: VarSize + sealed::Reader {
    fn try_read_len_at<C: WorkControl>(
        data: FontData<'_>,
        position: usize,
        control: &mut C,
    ) -> Result<Option<usize>, C::Error>;

    fn try_total_len_for_count<C: WorkControl>(
        data: FontData<'_>,
        count: usize,
        control: &mut C,
    ) -> ReadCtl<usize, C> {
        total_length::<Checked<Self>, C>(data, count, control)
    }
}

/// Explicit controlled offset resolution, implemented only by real font offsets.
pub trait TryResolveOffset: crate::Offset + sealed::Offset {
    fn try_resolve<'a, T: TryFontRead<'a>, C: WorkControl>(
        &self,
        data: FontData<'a>,
        control: &mut C,
    ) -> ReadCtl<T, C>;
    fn try_resolve_with_args<'a, T: TryFontReadWithArgs<'a>, C: WorkControl>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
        control: &mut C,
    ) -> ReadCtl<T, C>;
}

/// Null stays distinct from an invalid offset or an outside control failure.
pub trait TryResolveNullableOffset: sealed::NullableOffset {
    fn try_resolve<'a, T: TryFontRead<'a>, C: WorkControl>(
        &self,
        data: FontData<'a>,
        control: &mut C,
    ) -> Result<Option<Result<T, ReadError>>, C::Error>;
    fn try_resolve_with_args<'a, T: TryFontReadWithArgs<'a>, C: WorkControl>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
        control: &mut C,
    ) -> Result<Option<Result<T, ReadError>>, C::Error>;
}

/// Mode-restricted provider; an arbitrary ordinary TableProvider is not checked.
pub trait TryTableProvider<'a, C: WorkControl>: sealed::Provider {
    fn try_data_for_tag(
        &self,
        tag: crate::types::Tag,
        control: &mut C,
    ) -> Result<Option<FontData<'a>>, C::Error>;

    fn try_expect_data_for_tag(
        &self,
        tag: crate::types::Tag,
        control: &mut C,
    ) -> ReadCtl<FontData<'a>, C> {
        let data = self.try_data_for_tag(tag, control)?;
        control.checkpoint(0)?;
        Ok(data.ok_or(ReadError::TableIsMissing(tag)))
    }

    fn try_expect_table<T: TopLevelTable + TryFontRead<'a>>(
        &self,
        control: &mut C,
    ) -> ReadCtl<T, C> {
        let data = match self.try_expect_data_for_tag(T::TAG, control)? {
            Ok(data) => data,
            Err(error) => return Ok(Err(error)),
        };
        let result = T::try_read(data, control)?;
        control.checkpoint(0)?;
        Ok(result)
    }
    fn try_head(&self, cx: &mut C) -> ReadCtl<crate::tables::head::Head<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_hhea(&self, cx: &mut C) -> ReadCtl<crate::tables::hhea::Hhea<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_vhea(&self, cx: &mut C) -> ReadCtl<crate::tables::vhea::Vhea<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_maxp(&self, cx: &mut C) -> ReadCtl<crate::tables::maxp::Maxp<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_hdmx(&self, cx: &mut C) -> ReadCtl<crate::tables::hdmx::Hdmx<'a>, C> {
        let num_glyphs = crate::control::font_try!(self.try_maxp(cx)?).num_glyphs();
        let data = crate::control::font_try!(
            self.try_expect_data_for_tag(crate::tables::hdmx::Hdmx::TAG, cx)?
        );
        crate::tables::hdmx::Hdmx::try_read_with_args(data, &num_glyphs, cx)
    }

    /// Borrows the cvt scalar array; no element is scanned or copied.
    fn try_cvt(&self, cx: &mut C) -> ReadCtl<&'a [crate::types::BigEndian<i16>], C> {
        let data = crate::control::font_try!(
            self.try_expect_data_for_tag(crate::types::Tag::new(b"cvt "), cx)?
        );
        cx.checkpoint(0)?;
        Ok(data.read_array(0..data.len()))
    }

    fn try_os2(&self, cx: &mut C) -> ReadCtl<crate::tables::os2::Os2<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_post(&self, cx: &mut C) -> ReadCtl<crate::tables::post::Post<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_cmap(&self, cx: &mut C) -> ReadCtl<crate::tables::cmap::Cmap<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_fvar(&self, cx: &mut C) -> ReadCtl<crate::tables::fvar::Fvar<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_avar(&self, cx: &mut C) -> ReadCtl<crate::tables::avar::Avar<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_hvar(&self, cx: &mut C) -> ReadCtl<crate::tables::hvar::Hvar<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_vvar(&self, cx: &mut C) -> ReadCtl<crate::tables::vvar::Vvar<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_mvar(&self, cx: &mut C) -> ReadCtl<crate::tables::mvar::Mvar<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_vorg(&self, cx: &mut C) -> ReadCtl<crate::tables::vorg::Vorg<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_glyf(&self, cx: &mut C) -> ReadCtl<crate::tables::glyf::Glyf<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_gvar(&self, cx: &mut C) -> ReadCtl<crate::tables::gvar::Gvar<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_cff(&self, cx: &mut C) -> ReadCtl<crate::tables::cff::Cff<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_cff2(&self, cx: &mut C) -> ReadCtl<crate::tables::cff2::Cff2<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_varc(&self, cx: &mut C) -> ReadCtl<crate::tables::varc::Varc<'a>, C> {
        self.try_expect_table(cx)
    }

    fn try_hmtx(&self, cx: &mut C) -> ReadCtl<crate::tables::hmtx::Hmtx<'a>, C> {
        let hhea = crate::control::font_try!(self.try_hhea(cx)?);
        let data = crate::control::font_try!(
            self.try_expect_data_for_tag(crate::tables::hmtx::Hmtx::TAG, cx)?
        );
        crate::tables::hmtx::Hmtx::try_read_with_args(data, &hhea.number_of_h_metrics(), cx)
    }
    fn try_vmtx(&self, cx: &mut C) -> ReadCtl<crate::tables::vmtx::Vmtx<'a>, C> {
        let vhea = crate::control::font_try!(self.try_vhea(cx)?);
        let data = crate::control::font_try!(
            self.try_expect_data_for_tag(crate::tables::vmtx::Vmtx::TAG, cx)?
        );
        crate::tables::vmtx::Vmtx::try_read_with_args(data, &vhea.number_of_long_ver_metrics(), cx)
    }
    fn try_loca(
        &self,
        is_long: Option<bool>,
        cx: &mut C,
    ) -> ReadCtl<crate::tables::loca::Loca<'a>, C> {
        let is_long = match is_long {
            Some(value) => value,
            None => crate::control::font_try!(self.try_head(cx)?).index_to_loc_format() == 1,
        };
        let data = crate::control::font_try!(
            self.try_expect_data_for_tag(crate::tables::loca::Loca::TAG, cx)?
        );
        crate::tables::loca::Loca::try_read_with_args(data, &is_long, cx)
    }
}

pub(crate) type OrdinaryControl = NoControl<Infallible>;

pub(crate) fn ordinary<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

// These zero-sized modes are private. An external FontRead implementation may
// be used by the old API only with NoControl. It cannot satisfy a request mode.
pub(crate) struct Ordinary<T: ?Sized>(PhantomData<fn() -> T>);
pub(crate) struct Checked<T: ?Sized>(PhantomData<fn() -> T>);

pub(crate) trait ReaderMode<'a, C: WorkControl> {
    type Target: FontReadWithArgs<'a>;
    fn read(
        data: FontData<'a>,
        args: &<Self::Target as ReadArgs>::Args,
        control: &mut C,
    ) -> ReadCtl<Self::Target, C>;
}
impl<'a, T: FontReadWithArgs<'a>> ReaderMode<'a, OrdinaryControl> for Ordinary<T> {
    type Target = T;
    fn read(
        data: FontData<'a>,
        args: &T::Args,
        _: &mut OrdinaryControl,
    ) -> ReadCtl<T, OrdinaryControl> {
        Ok(T::read_with_args(data, args))
    }
}
impl<'a, T: TryFontReadWithArgs<'a>, C: WorkControl> ReaderMode<'a, C> for Checked<T> {
    type Target = T;
    fn read(data: FontData<'a>, args: &T::Args, control: &mut C) -> ReadCtl<T, C> {
        T::try_read_with_args(data, args, control)
    }
}

pub(crate) trait SizeMode<C: WorkControl> {
    type Target: ComputeSize;
    fn size(args: &<Self::Target as ReadArgs>::Args, control: &mut C) -> ReadCtl<usize, C>;
}
impl<T: ComputeSize> SizeMode<OrdinaryControl> for Ordinary<T> {
    type Target = T;
    fn size(args: &T::Args, _: &mut OrdinaryControl) -> ReadCtl<usize, OrdinaryControl> {
        Ok(T::compute_size(args))
    }
}
impl<T: TryComputeSize, C: WorkControl> SizeMode<C> for Checked<T> {
    type Target = T;
    fn size(args: &T::Args, control: &mut C) -> ReadCtl<usize, C> {
        T::try_compute_size(args, control)
    }
}

pub(crate) trait LengthMode<C: WorkControl> {
    fn length(
        data: FontData<'_>,
        position: usize,
        control: &mut C,
    ) -> Result<Option<usize>, C::Error>;
}
impl<T: VarSize + ?Sized> LengthMode<OrdinaryControl> for Ordinary<T> {
    fn length(
        data: FontData<'_>,
        position: usize,
        _: &mut OrdinaryControl,
    ) -> Result<Option<usize>, Infallible> {
        Ok(T::read_len_at(data, position))
    }
}
impl<T: TryVarSize + ?Sized, C: WorkControl> LengthMode<C> for Checked<T> {
    fn length(
        data: FontData<'_>,
        position: usize,
        control: &mut C,
    ) -> Result<Option<usize>, C::Error> {
        T::try_read_len_at(data, position, control)
    }
}

pub(crate) fn total_length<M: LengthMode<C>, C: WorkControl>(
    data: FontData<'_>,
    count: usize,
    control: &mut C,
) -> ReadCtl<usize, C> {
    control.checkpoint(0)?;
    let mut position = 0usize;
    for _ in 0..count {
        control.checkpoint(1)?;
        let Some(length) = M::length(data, position, control)? else {
            return Ok(Err(ReadError::OutOfBounds));
        };
        if length == 0 {
            control.checkpoint(0)?;
            return Ok(Ok(position));
        }
        let Some(next) = position.checked_add(length) else {
            return Ok(Err(ReadError::OutOfBounds));
        };
        position = next;
    }
    control.checkpoint(0)?;
    Ok(Ok(position))
}

// Unwrap a font-only Result in a nested-result kernel. Control is always
// extracted separately with ? before invoking this macro.
macro_rules! font_try {
    ($expression:expr) => {
        match $expression {
            Ok(value) => value,
            Err(error) => return Ok(Err(error.into())),
        }
    };
}
pub(crate) use font_try;

// Explicitly registered generated view constructors. This macro's only parser
// is the original MIN_SIZE check plus Self { data }; it never calls ordinary
// FontRead and cannot register a reader with extra fields or work by accident.
macro_rules! simple_table_read {
    ($name:ident) => {
        impl<'a> $crate::FontRead<'a> for $name<'a> {
            fn read(data: $crate::FontData<'a>) -> Result<Self, $crate::ReadError> {
                $crate::control::ordinary(<Self as $crate::control::TryFontRead<'a>>::try_read(
                    data,
                    &mut $crate::control::OrdinaryControl::new(),
                ))
            }
        }
        impl $crate::control::sealed::Reader for $name<'_> {}
        impl<'a> $crate::control::TryFontRead<'a> for $name<'a> {
            fn try_read<C: markdown_svg_budget::WorkControl>(
                data: $crate::FontData<'a>,
                control: &mut C,
            ) -> $crate::control::ReadCtl<Self, C> {
                control.checkpoint(0)?;
                #[allow(clippy::absurd_extreme_comparisons)]
                if data.len() < Self::MIN_SIZE {
                    return Ok(Err($crate::ReadError::OutOfBounds));
                }
                Ok(Ok(Self { data }))
            }
        }
    };
}
pub(crate) use simple_table_read;

// Closed generated sum types dispatch only to their explicitly controlled
// concrete variants. No T:FontRead default or ordinary parser callback exists.
macro_rules! format_table_read {
    ($name:ident,$scalar:ty,$offset:expr; $($kind:ident => $variant:ident),+ $(,)?) => {
        impl<'a> $crate::FontRead<'a> for $name<'a> {
            fn read(data:$crate::FontData<'a>) -> Result<Self,$crate::ReadError> {
                $crate::control::ordinary(<Self as $crate::control::TryFontRead<'a>>::try_read(
                    data,&mut $crate::control::OrdinaryControl::new()))
            }
        }
        impl $crate::control::sealed::Reader for $name<'_> {}
        impl<'a> $crate::control::TryFontRead<'a> for $name<'a> {
            fn try_read<C:markdown_svg_budget::WorkControl>(
                data:$crate::FontData<'a>,control:&mut C,
            ) -> $crate::control::ReadCtl<Self,C> {
                control.checkpoint(0)?;
                let format:$scalar=$crate::control::font_try!(data.read_at($offset));
                Ok(match format {
                    $(<$kind<'a> as $crate::read::Format<$scalar>>::FORMAT =>
                        Ok(Self::$variant($crate::control::font_try!(
                            <$kind<'a> as $crate::control::TryFontRead<'a>>::try_read(data,control)?))),)+
                    other=>Err($crate::ReadError::InvalidFormat(other.into())),
                })
            }
        }
    };
}
pub(crate) use format_table_read;

macro_rules! args_table_read {
    ($name:ident, $pattern:tt; $($field:ident),+ $(,)?) => {
        impl<'a> $crate::FontReadWithArgs<'a> for $name<'a> {
            fn read_with_args(data:$crate::FontData<'a>,args:&Self::Args) -> Result<Self,$crate::ReadError> {
                $crate::control::ordinary(<Self as $crate::control::TryFontReadWithArgs<'a>>::try_read_with_args(
                    data,args,&mut $crate::control::OrdinaryControl::new()))
            }
        }
        impl $crate::control::sealed::Reader for $name<'_> {}
        impl<'a> $crate::control::TryFontReadWithArgs<'a> for $name<'a> {
            fn try_read_with_args<C:markdown_svg_budget::WorkControl>(
                data:$crate::FontData<'a>,args:&Self::Args,control:&mut C,
            ) -> $crate::control::ReadCtl<Self,C> {
                control.checkpoint(0)?;
                let $pattern=*args;
                #[allow(clippy::absurd_extreme_comparisons)]
                if data.len()<Self::MIN_SIZE { return Ok(Err($crate::ReadError::OutOfBounds)); }
                Ok(Ok(Self { data,$($field),+ }))
            }
        }
    };
}
pub(crate) use args_table_read;

// Font-only absence is preserved; an outside control Result is always unwrapped
// with ? before this macro, never converted to Option.
macro_rules! font_option {
    ($value:expr) => {
        match $value {
            Some(value) => value,
            None => return Ok(None),
        }
    };
}
pub(crate) use font_option;
