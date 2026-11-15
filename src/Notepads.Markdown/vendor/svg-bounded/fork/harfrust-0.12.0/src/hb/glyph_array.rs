// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! One physical record layout with sealed info/position borrows. Reinterpreting
//! a buffer swaps complete owners; no Vec, allocation lease or raw pointer escapes.

use alloc::vec::Vec;
use core::{
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut, Range},
};
use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, NoControl, RequestControl, ResourceKind,
    ResourcesRef, WorkControl,
};

use super::buffer::{GlyphInfo, GlyphPosition};

type Words = [u32; 5];

const _: () = {
    assert!(size_of::<GlyphInfo>() == 20);
    assert!(size_of::<GlyphPosition>() == 20);
    assert!(align_of::<GlyphInfo>() == 4);
    assert!(align_of::<GlyphPosition>() == 4);
    assert!(size_of::<Words>() == 20);
    assert!(align_of::<Words>() == 4);
};

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::GlyphInfo {}
    impl Sealed for super::GlyphPosition {}
}

pub(crate) trait GlyphView: sealed::Sealed + bytemuck::Pod {}
impl GlyphView for GlyphInfo {}
impl GlyphView for GlyphPosition {}

enum Storage {
    Plain(Vec<Words>),
    Paid {
        records: BudgetVec<Words>,
        resources: ResourcesRef,
    },
}

/// Raw length is the initialized extent; active glyph counts live in hb_buffer_t.
pub(crate) struct GlyphArray<T: GlyphView> {
    storage: Storage,
    view: PhantomData<T>,
}

impl<T: GlyphView> GlyphArray<T> {
    pub(crate) fn new(resources: Option<&ResourcesRef>) -> Self {
        let storage = match resources {
            Some(resources) => Storage::Paid {
                records: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
                resources: resources.clone(),
            },
            None => Storage::Plain(Vec::new()),
        };
        Self {
            storage,
            view: PhantomData,
        }
    }

    pub(crate) fn resources(&self) -> Option<&ResourcesRef> {
        match &self.storage {
            Storage::Plain(_) => None,
            Storage::Paid { resources, .. } => Some(resources),
        }
    }

    pub(crate) fn record(&self, error: Error) -> Error {
        self.resources()
            .map_or(error, |resources| resources.record(error))
    }

    pub(crate) fn checkpoint(&self, work: u64) -> Result<(), Error> {
        match self.resources() {
            Some(resources) => resources.checkpoint(work),
            None => Ok(()),
        }
    }

    fn words(&self) -> &[Words] {
        match &self.storage {
            Storage::Plain(values) => values,
            Storage::Paid { records, .. } => records.as_slice(),
        }
    }

    fn words_mut(&mut self) -> &mut [Words] {
        match &mut self.storage {
            Storage::Plain(values) => values,
            Storage::Paid { records, .. } => records.as_mut_slice(),
        }
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        // Only the two sealed Pod types have this view. The static layout
        // assertions establish equal alignment/size, including empty slices.
        bytemuck::cast_slice(self.words())
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [T] {
        bytemuck::cast_slice_mut(self.words_mut())
    }

    pub(crate) fn len(&self) -> usize {
        self.words().len()
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        match &self.storage {
            Storage::Plain(values) => values.capacity(),
            Storage::Paid { records, .. } => records.capacity(),
        }
    }

    pub(crate) fn clear(&mut self) {
        match &mut self.storage {
            Storage::Plain(values) => values.clear(),
            Storage::Paid { records, .. } => records.clear(),
        }
    }

    pub(crate) fn try_resize(&mut self, length: usize, value: T) -> Result<(), Error> {
        let value: Words = bytemuck::cast(value);
        match &mut self.storage {
            // Preserve ordinary Vec::resize allocation, growth and OOM behavior.
            // NoControl does not imply that a fallible allocator cannot fail.
            Storage::Plain(values) => {
                values.resize(length, value);
                Ok(())
            }
            Storage::Paid { records, .. } => records.try_resize_copy(length, value),
        }
    }

    pub(crate) fn try_copy_within(
        &mut self,
        source: Range<usize>,
        destination: usize,
    ) -> Result<(), Error> {
        let resources = self.resources().cloned();
        let result = match &resources {
            Some(resources) => markdown_svg_budget::slice::copy_within_with_control(
                self.words_mut(),
                source,
                destination,
                &mut RequestControl(resources),
            ),
            None => markdown_svg_budget::slice::copy_within_with_control(
                self.words_mut(),
                source,
                destination,
                &mut NoControl::<Error>::new(),
            ),
        };
        result.map_err(|error| self.record(error))
    }

    pub(crate) fn try_fill(&mut self, range: Range<usize>, value: T) -> Result<(), Error> {
        let resources = self.resources().cloned();
        let result = match &resources {
            Some(resources) => Self::fill_with_control(
                self.words_mut(),
                range,
                bytemuck::cast(value),
                &mut RequestControl(resources),
            ),
            None => Self::fill_with_control(
                self.words_mut(),
                range,
                bytemuck::cast(value),
                &mut NoControl::<Error>::new(),
            ),
        };
        result.map_err(|error| self.record(error))
    }

    fn fill_with_control<C: WorkControl<Error = Error>>(
        values: &mut [Words],
        range: Range<usize>,
        value: Words,
        control: &mut C,
    ) -> Result<(), Error> {
        control.checkpoint(0)?;
        if range.start > range.end || range.end > values.len() {
            return Err(Error::new(FailureKind::Invariant, LimitKind::Capacity));
        }
        for chunk in values[range].chunks_mut(256) {
            control.checkpoint((chunk.len() * size_of::<Words>()) as u64)?;
            chunk.fill(value);
        }
        control.checkpoint(0)
    }

    pub(crate) fn swap_storage<U: GlyphView>(
        &mut self,
        other: &mut GlyphArray<U>,
    ) -> Result<(), Error> {
        let same_request = match (self.resources(), other.resources()) {
            (None, None) => true,
            (Some(a), Some(b)) => a.is_same_request(b),
            _ => false,
        };
        if !same_request {
            let error = Error::new(FailureKind::Invariant, LimitKind::None);
            return Err(match self.resources().or_else(|| other.resources()) {
                Some(resources) => resources.record(error),
                None => error,
            });
        }
        self.checkpoint(0)?;
        // Both enum values have identical types. Their physical allocation,
        // initialized extent, capacity and lease move together. Field markers
        // stay Info/Position; the moved block therefore gains the opposite view.
        core::mem::swap(&mut self.storage, &mut other.storage);
        Ok(())
    }
}

impl<T: GlyphView> Deref for GlyphArray<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}
impl<T: GlyphView> DerefMut for GlyphArray<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}
impl<'a, T: GlyphView> IntoIterator for &'a GlyphArray<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}
impl<'a, T: GlyphView> IntoIterator for &'a mut GlyphArray<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
    }
}
impl<T: GlyphView + fmt::Debug> fmt::Debug for GlyphArray<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(formatter)
    }
}

#[cfg(test)]
#[path = "glyph_array_tests.rs"]
pub(crate) mod tests;
