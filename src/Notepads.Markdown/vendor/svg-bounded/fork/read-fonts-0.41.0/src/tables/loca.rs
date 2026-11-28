// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! The [loca (Index to Location)][loca] table
//!
//! [loca]: https://docs.microsoft.com/en-us/typography/opentype/spec/loca

use crate::control::{self, OrdinaryControl, ReadCtl, TryFontRead, TryFontReadWithArgs};
use crate::{
    read::{FontReadWithArgs, ReadArgs, ReadError},
    table_provider::TopLevelTable,
    FontData,
};
use markdown_svg_budget::WorkControl;
use types::{BigEndian, GlyphId, Tag};

#[cfg(feature = "experimental_traverse")]
use crate::traversal;

/// The [loca] table.
///
/// [loca]: https://docs.microsoft.com/en-us/typography/opentype/spec/loca
#[derive(Clone)]
pub enum Loca<'a> {
    Short(&'a [BigEndian<u16>]),
    Long(&'a [BigEndian<u32>]),
}

impl TopLevelTable for Loca<'_> {
    const TAG: Tag = Tag::new(b"loca");
}

impl<'a> Loca<'a> {
    pub fn read(data: FontData<'a>, is_long: bool) -> Result<Self, crate::ReadError> {
        Self::read_with_args(data, &is_long)
    }

    pub fn len(&self) -> usize {
        match self {
            Loca::Short(data) => data.len().saturating_sub(1),
            Loca::Long(data) => data.len().saturating_sub(1),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn all_offsets_are_ascending(&self) -> bool {
        control::ordinary(self.try_all_offsets_are_ascending(&mut OrdinaryControl::new()))
    }
    pub fn try_all_offsets_are_ascending<C: WorkControl>(
        &self,
        cx: &mut C,
    ) -> Result<bool, C::Error> {
        fn ascending<T: Ord, C: WorkControl>(data: &[T], cx: &mut C) -> Result<bool, C::Error> {
            for (start, end) in data.iter().zip(data.iter().skip(1)) {
                cx.checkpoint(1)?;
                if start > end {
                    return Ok(false);
                }
            }
            cx.checkpoint(0)?;
            Ok(true)
        }
        cx.checkpoint(0)?;
        match self {
            Self::Short(data) => ascending(data, cx),
            Self::Long(data) => ascending(data, cx),
        }
    }

    /// Attempt to return the offset for a given glyph id.
    pub fn get_raw(&self, idx: usize) -> Option<u32> {
        match self {
            Loca::Short(data) => data.get(idx).map(|x| x.get() as u32 * 2),
            Loca::Long(data) => data.get(idx).map(|x| x.get()),
        }
    }

    pub fn get_glyf(
        &self,
        gid: GlyphId,
        glyf: &super::glyf::Glyf<'a>,
    ) -> Result<Option<super::glyf::Glyph<'a>>, ReadError> {
        control::ordinary(self.try_get_glyf(gid, glyf, &mut OrdinaryControl::new()))
    }
    pub fn try_get_glyf<C: WorkControl>(
        &self,
        gid: GlyphId,
        glyf: &super::glyf::Glyf<'a>,
        cx: &mut C,
    ) -> ReadCtl<Option<super::glyf::Glyph<'a>>, C> {
        cx.checkpoint(1)?;
        let index = gid.to_u32() as usize;
        let start = control::font_try!(self.get_raw(index).ok_or(ReadError::OutOfBounds));
        let end = control::font_try!(self.get_raw(index + 1).ok_or(ReadError::OutOfBounds));
        if start == end {
            return Ok(Ok(None));
        }
        let data = control::font_try!(glyf
            .offset_data()
            .slice(start as usize..end as usize)
            .ok_or(ReadError::OutOfBounds));
        Ok(super::glyf::Glyph::try_read(data, cx)?.map(Some))
    }
}

impl ReadArgs for Loca<'_> {
    type Args = bool;
}

impl<'a> FontReadWithArgs<'a> for Loca<'a> {
    fn read_with_args(data: FontData<'a>, args: &bool) -> Result<Self, ReadError> {
        control::ordinary(Self::try_read_with_args(
            data,
            args,
            &mut OrdinaryControl::new(),
        ))
    }
}
impl control::sealed::Reader for Loca<'_> {}
impl<'a> TryFontReadWithArgs<'a> for Loca<'a> {
    fn try_read_with_args<C: WorkControl>(
        data: FontData<'a>,
        args: &bool,
        cx: &mut C,
    ) -> ReadCtl<Self, C> {
        cx.checkpoint(1)?;
        let result = if *args {
            data.read_array(0..data.len()).map(Loca::Long)
        } else {
            data.read_array(0..data.len()).map(Loca::Short)
        };
        cx.checkpoint(0)?;
        Ok(result)
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> traversal::SomeTable<'a> for Loca<'a> {
    fn type_name(&self) -> &str {
        "loca"
    }

    fn get_field(&self, idx: usize) -> Option<traversal::Field<'a>> {
        match idx {
            0usize => Some(traversal::Field::new("offsets", self.clone())),
            _ => None,
        }
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> traversal::SomeArray<'a> for Loca<'a> {
    fn len(&self) -> usize {
        self.len()
    }

    fn get(&self, idx: usize) -> Option<traversal::FieldType<'a>> {
        self.get_raw(idx).map(|off| off.into())
    }

    fn type_name(&self) -> &str {
        "Offset32"
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> std::fmt::Debug for Loca<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self as &dyn traversal::SomeTable<'a>).fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use font_test_data::bebuffer::BeBuffer;
    use types::Scalar;

    use super::Loca;

    fn to_loca_bytes<T: Scalar + Copy>(values: &[T]) -> (BeBuffer, bool) {
        let value_num_bytes = std::mem::size_of::<T>();
        let is_long = if value_num_bytes == 2 {
            false
        } else if value_num_bytes == 4 {
            true
        } else {
            panic!("invalid integer type must be u32 or u16")
        };
        let mut buffer = BeBuffer::default();

        for v in values {
            buffer = buffer.push(*v);
        }

        (buffer, is_long)
    }

    fn check_loca_sorting(values: &[u16], is_sorted: bool) {
        let (bytes, is_long) = to_loca_bytes(values);
        let loca = Loca::read(bytes.data().into(), is_long).unwrap();
        assert_eq!(loca.all_offsets_are_ascending(), is_sorted);

        let u32_values: Vec<u32> = values.iter().map(|v| *v as u32).collect();
        let (bytes, is_long) = to_loca_bytes(&u32_values);
        let loca = Loca::read(bytes.data().into(), is_long).unwrap();
        assert_eq!(loca.all_offsets_are_ascending(), is_sorted);
    }

    #[test]
    fn all_offsets_are_ascending() {
        // Sorted
        let empty: &[u16] = &[];
        check_loca_sorting(empty, true);
        check_loca_sorting(&[0], true);
        check_loca_sorting(&[0, 0], true);
        check_loca_sorting(&[0, 1], true);
        check_loca_sorting(&[1, 2, 2, 3, 7], true);

        // Unsorted
        check_loca_sorting(&[1, 0], false);
        check_loca_sorting(&[1, 3, 2], false);
        check_loca_sorting(&[2, 1, 3], false);
        check_loca_sorting(&[1, 2, 3, 2, 7], false);
    }
}
