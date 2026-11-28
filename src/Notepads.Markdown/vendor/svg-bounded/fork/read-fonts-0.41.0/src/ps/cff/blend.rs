// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Support for the dictionary and charstring blend operator.

use font_types::{BigEndian, F2Dot14, Fixed};

use crate::control::{self, OrdinaryControl, PsCtl};
use crate::ps::error::Error;
use crate::tables::variations::{ItemVariationData, ItemVariationStore};
use markdown_svg_budget::WorkControl;

/// The maximum number of region scalars that we precompute.
///
/// Completely made up number chosen to balance size with trying to capture as
/// many precomputed regions as possible.
///
/// TODO: measure with a larger set of CFF2 fonts and adjust accordingly.
const MAX_PRECOMPUTED_SCALARS: usize = 16;

/// State for processing the blend operator for DICTs and charstrings.
///
/// To avoid allocation, scalars are computed on demand but this can be
/// prohibitively expensive in charstrings where blends are applied to
/// large numbers of elements. To amortize the cost, a fixed number of
/// precomputed scalars are stored internally and the overflow is computed
/// as needed.
///
/// The `MAX_PRECOMPUTED_SCALARS` constant determines the size of the
/// internal buffer (currently 16).
///
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2charstr#45-variation-data-operators>
pub struct BlendState<'a> {
    store: ItemVariationStore<'a>,
    coords: &'a [F2Dot14],
    store_index: u16,
    // The following are dependent on the current `store_index`
    data: Option<ItemVariationData<'a>>,
    region_indices: &'a [BigEndian<u16>],
    scalars: [Fixed; MAX_PRECOMPUTED_SCALARS],
}

impl<'a> BlendState<'a> {
    pub fn new(
        store: ItemVariationStore<'a>,
        coords: &'a [F2Dot14],
        store_index: u16,
    ) -> Result<Self, Error> {
        control::ordinary(Self::try_new(
            store,
            coords,
            store_index,
            &mut OrdinaryControl::new(),
        ))
    }
    pub fn try_new<C: WorkControl>(
        store: ItemVariationStore<'a>,
        coords: &'a [F2Dot14],
        store_index: u16,
        cx: &mut C,
    ) -> PsCtl<Self, C> {
        cx.checkpoint(0)?;
        let mut state = Self {
            store,
            coords,
            store_index,
            data: None,
            region_indices: &[],
            scalars: Default::default(),
        };
        control::font_try!(state.try_update_precomputed_scalars(cx)?);
        cx.checkpoint(0)?;
        Ok(Ok(state))
    }

    /// Sets the active variation store index.
    ///
    /// This should be called with the operand of the `vsindex` operator
    /// for both DICTs and charstrings.
    pub fn set_store_index(&mut self, store_index: u16) -> Result<(), Error> {
        control::ordinary(self.try_set_store_index(store_index, &mut OrdinaryControl::new()))
    }
    pub fn try_set_store_index<C: WorkControl>(
        &mut self,
        store_index: u16,
        cx: &mut C,
    ) -> PsCtl<(), C> {
        cx.checkpoint(0)?;
        if self.store_index != store_index {
            self.store_index = store_index;
            control::font_try!(self.try_update_precomputed_scalars(cx)?);
        }
        cx.checkpoint(0)?;
        Ok(Ok(()))
    }

    /// Returns the number of variation regions for the currently active
    /// variation store index.
    pub fn region_count(&self) -> Result<usize, Error> {
        Ok(self.region_indices.len())
    }

    /// Returns an iterator yielding scalars for each variation region of
    /// the currently active variation store index.
    pub fn scalars(&self) -> Result<impl Iterator<Item = Result<Fixed, Error>> + '_, Error> {
        let mut cursor = self.try_scalars();
        Ok(std::iter::from_fn(move || {
            control::ordinary(cursor.try_next(&mut OrdinaryControl::new()))
        }))
    }
    pub fn try_scalars(&self) -> BlendScalars<'_, 'a> {
        BlendScalars {
            state: self,
            index: 0,
        }
    }
    fn try_update_precomputed_scalars<C: WorkControl>(&mut self, cx: &mut C) -> PsCtl<(), C> {
        cx.checkpoint(0)?;
        self.data = None;
        self.region_indices = &[];
        let entry = control::font_try!(self
            .store
            .item_variation_data()
            .try_get(self.store_index as usize, cx)?
            .ok_or(Error::InvalidVariationStoreIndex(self.store_index)));
        let data = control::font_try!(entry);
        let region_indices = data.region_indexes();
        let regions = control::font_try!(self.store.try_variation_region_list(cx)?)
            .try_variation_regions(cx)?;
        for (region_ix, scalar) in region_indices
            .iter()
            .take(MAX_PRECOMPUTED_SCALARS)
            .zip(&mut self.scalars)
        {
            cx.checkpoint(1)?;
            let region = control::font_try!(regions.try_get(region_ix.get() as usize, cx)?);
            *scalar = region.try_compute_scalar(self.coords, cx)?;
        }
        self.data = Some(data);
        self.region_indices = region_indices;
        cx.checkpoint(0)?;
        Ok(Ok(()))
    }
    fn try_region_scalar<C: WorkControl>(&self, index: u16, cx: &mut C) -> PsCtl<Fixed, C> {
        let regions = control::font_try!(self.store.try_variation_region_list(cx)?)
            .try_variation_regions(cx)?;
        let region = control::font_try!(regions.try_get(index as usize, cx)?);
        Ok(Ok(region.try_compute_scalar(self.coords, cx)?))
    }
}
/// A cursor cannot accidentally use the ordinary iterator on a request.
pub struct BlendScalars<'s, 'f> {
    state: &'s BlendState<'f>,
    index: usize,
}
impl BlendScalars<'_, '_> {
    pub fn try_next<C: WorkControl>(
        &mut self,
        cx: &mut C,
    ) -> Result<Option<Result<Fixed, Error>>, C::Error> {
        cx.checkpoint(1)?;
        let Some(region) = self.state.region_indices.get(self.index) else {
            return Ok(None);
        };
        let scalar = if self.index < MAX_PRECOMPUTED_SCALARS {
            Ok(self.state.scalars[self.index])
        } else {
            self.state.try_region_scalar(region.get(), cx)?
        };
        self.index += 1;
        cx.checkpoint(0)?;
        Ok(Some(scalar))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{FontData, FontRead};

    #[test]
    fn example_blends() {
        // args are (coords, expected_scalars)
        example_test(&[-1.0], &[0.0, 1.0]);
        example_test(&[-0.25], &[0.5, 0.0]);
        example_test(&[-0.5], &[1.0, 0.0]);
        example_test(&[-0.75], &[0.5, 0.5]);
        example_test(&[0.0], &[0.0, 0.0]);
        example_test(&[0.5], &[0.0, 0.0]);
        example_test(&[1.0], &[0.0, 0.0]);
    }

    fn example_test(coords: &[f32], expected: &[f64]) {
        let scalars = example_scalars_for_coords(coords);
        let expected: Vec<_> = expected.iter().copied().map(Fixed::from_f64).collect();
        assert_eq!(scalars, expected);
    }

    fn example_scalars_for_coords(coords: &[f32]) -> Vec<Fixed> {
        let ivs = example_ivs();
        let coords: Vec<_> = coords
            .iter()
            .map(|coord| F2Dot14::from_f32(*coord))
            .collect();
        let blender = BlendState::new(ivs, &coords, 0).unwrap();
        blender.scalars().unwrap().map(|res| res.unwrap()).collect()
    }

    fn example_ivs() -> ItemVariationStore<'static> {
        // ItemVariationStore is at offset 18 in the CFF example table
        let ivs_data = &font_test_data::cff2::EXAMPLE[18..];
        ItemVariationStore::read(FontData::new(ivs_data)).unwrap()
    }
}
