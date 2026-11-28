// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Font-reader mode selection. Resource failure remains an outer Result.
use crate::hb::storage::Control;
use markdown_svg_budget::{Error, RequestControl, ResourcesRef};
use read_fonts::{
    tables::{kern, kerx, morx},
    ReadError,
};

pub(crate) trait Cursor {
    type Item;
    fn next(&mut self, control: &Control) -> Result<Option<Result<Self::Item, ReadError>>, Error>;
}
pub(crate) struct Selected<P, N> {
    plain: P,
    paid: N,
}
impl<T, P, N> Cursor for Selected<P, N>
where
    P: Iterator<Item = Result<T, ReadError>>,
    N: FnMut(&ResourcesRef) -> Result<Option<Result<T, ReadError>>, Error>,
{
    type Item = T;
    fn next(&mut self, c: &Control) -> Result<Option<Result<T, ReadError>>, Error> {
        c.step(0)?;
        match &c.0 {
            Some(r) => (self.paid)(r).map_err(|e| r.record(e)),
            None => Ok(self.plain.next()),
        }
    }
}
pub(crate) fn offsets<'f, T, O>(
    values: &read_fonts::ArrayOfOffsets<'f, T, O>,
) -> impl Cursor<Item = T> + use<'f, T, O>
where
    T: read_fonts::control::TryFontReadWithArgs<'f> + 'f,
    T::Args: Copy + 'static,
    O: read_fonts::types::Scalar + read_fonts::control::TryResolveOffset + 'f,
{
    let mut paid = values.try_iter();
    Selected {
        plain: values.iter(),
        paid: move |r: &ResourcesRef| paid.try_next(&mut RequestControl(r)),
    }
}
pub(crate) fn next_valid<C: Cursor>(
    cursor: &mut C,
    control: &Control,
) -> Result<Option<C::Item>, Error> {
    loop {
        control.step(1)?;
        match cursor.next(control)? {
            Some(Ok(value)) => return Ok(Some(value)),
            Some(Err(_)) => {}
            None => return Ok(None),
        }
    }
}
pub(crate) fn chains<'f>(
    table: &morx::Morx<'f>,
    c: &Control,
) -> Result<impl Cursor<Item = morx::Chain<'f>>, Error> {
    c.step(0)?;
    let values = match &c.0 {
        Some(r) => table.try_chains(&mut RequestControl(r))?,
        None => table.chains(),
    };
    let mut paid = values.try_iter();
    Ok(Selected {
        plain: values.iter(),
        paid: move |r: &ResourcesRef| paid.try_next(&mut RequestControl(r)),
    })
}
pub(crate) fn morx_subtables<'f>(
    table: &morx::Chain<'f>,
    c: &Control,
) -> Result<impl Cursor<Item = morx::Subtable<'f>>, Error> {
    c.step(0)?;
    let values = match &c.0 {
        Some(r) => table.try_subtables(&mut RequestControl(r))?,
        None => table.subtables(),
    };
    let mut paid = values.try_iter();
    Ok(Selected {
        plain: values.iter(),
        paid: move |r: &ResourcesRef| paid.try_next(&mut RequestControl(r)),
    })
}
pub(crate) fn kerx_subtables<'f>(
    table: &kerx::Kerx<'f>,
    c: &Control,
) -> Result<impl Cursor<Item = kerx::Subtable<'f>>, Error> {
    c.step(0)?;
    let values = match &c.0 {
        Some(r) => table.try_subtables(&mut RequestControl(r))?,
        None => table.subtables(),
    };
    let mut paid = values.try_iter();
    Ok(Selected {
        plain: values.iter(),
        paid: move |r: &ResourcesRef| paid.try_next(&mut RequestControl(r)),
    })
}
pub(crate) fn kern_subtables<'f>(
    table: &kern::Kern<'f>,
    c: &Control,
) -> Result<impl Cursor<Item = kern::Subtable<'f>>, Error> {
    c.step(0)?;
    let mut paid = match &c.0 {
        Some(r) => table.try_subtables(&mut RequestControl(r))?,
        None => match table.try_subtables(&mut markdown_svg_budget::NoControl::<
            core::convert::Infallible,
        >::new())
        {
            Ok(v) => v,
            Err(never) => match never {},
        },
    };
    Ok(Selected {
        plain: table.subtables(),
        paid: move |r: &ResourcesRef| paid.try_next(&mut RequestControl(r)),
    })
}
pub(crate) fn morx_kind<'f>(
    table: &morx::Subtable<'f>,
    c: &Control,
) -> Result<Result<morx::SubtableKind<'f>, ReadError>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_kind(&mut RequestControl(r)),
        None => Ok(table.kind()),
    }
}
pub(crate) fn kerx_kind<'f>(
    table: &kerx::Subtable<'f>,
    c: &Control,
) -> Result<Result<kerx::SubtableKind<'f>, ReadError>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_kind(&mut RequestControl(r)),
        None => Ok(table.kind()),
    }
}
pub(crate) fn kern_kind<'f>(
    table: &kern::Subtable<'f>,
    c: &Control,
) -> Result<Result<kern::SubtableKind<'f>, ReadError>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_kind(&mut RequestControl(r)),
        None => Ok(table.kind()),
    }
}
pub(crate) fn extended_entry<T: bytemuck::AnyBitPattern + read_fonts::types::FixedSize>(
    table: &read_fonts::tables::aat::ExtendedStateTable<'_, T>,
    state: u16,
    class: u16,
    c: &Control,
) -> Result<Result<read_fonts::tables::aat::StateEntry<T>, ReadError>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_entry(state, class, &mut RequestControl(r)),
        None => Ok(table.entry(state, class)),
    }
}
pub(crate) fn state_entry(
    table: &read_fonts::tables::aat::StateTable<'_>,
    state: u16,
    class: u8,
    c: &Control,
) -> Result<Result<read_fonts::tables::aat::StateEntry, ReadError>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_entry(state, class, &mut RequestControl(r)),
        None => Ok(table.entry(state, class)),
    }
}
pub(crate) fn typed_value<T: read_fonts::tables::aat::TryLookupValue>(
    table: &read_fonts::tables::aat::TypedLookup<'_, T>,
    glyph: u16,
    c: &Control,
) -> Result<Result<T, ReadError>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_value(glyph, &mut RequestControl(r)),
        None => Ok(table.value(glyph)),
    }
}

pub(crate) fn feat_find(
    table: &read_fonts::tables::feat::Feat<'_>,
    feature: u16,
    c: &Control,
) -> Result<Option<read_fonts::tables::feat::FeatureName>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_find(feature, &mut RequestControl(r)),
        None => Ok(table.find(feature)),
    }
}
pub(crate) fn anchors<'f>(
    table: &read_fonts::tables::ankr::Ankr<'f>,
    glyph: read_fonts::types::GlyphId,
    c: &Control,
) -> Result<Result<&'f [read_fonts::tables::ankr::AnchorPoint], ReadError>, Error> {
    c.step(0)?;
    match &c.0 {
        Some(r) => table.try_anchor_points(glyph, &mut RequestControl(r)),
        None => Ok(table.anchor_points(glyph)),
    }
}
