// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private storage selection for the shared shaping implementation.

use alloc::{boxed::Box, vec::Vec};
use core::ops::{Deref, DerefMut};
use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, RequestControl, ResourceKind, ResourcesRef,
};
use read_fonts::{ResolveNullableOffset, ResolveOffset};

#[derive(Clone, Default)]
pub(crate) struct Control(pub(crate) Option<ResourcesRef>);

impl Control {
    pub(crate) fn step(&self, work: u64) -> Result<(), Error> {
        self.0.as_ref().map_or(Ok(()), |r| r.checkpoint(work))
    }
    pub(crate) fn record(&self, error: Error) -> Error {
        self.0.as_ref().map_or(error, |r| r.record(error))
    }
    pub(crate) fn failure(&self) -> Option<Error> {
        self.0.as_ref().and_then(ResourcesRef::failure)
    }
    pub(crate) fn font<T>(
        &self,
        value: Result<Result<T, read_fonts::ReadError>, Error>,
    ) -> Option<T> {
        match value {
            Ok(font) => font.ok(),
            Err(error) => {
                self.record(error);
                None
            }
        }
    }
    pub(crate) fn optional<T>(&self, value: Result<Option<T>, Error>) -> Option<T> {
        match value {
            Ok(result) => result,
            Err(error) => {
                self.record(error);
                None
            }
        }
    }
    pub(crate) fn read<'f, T: read_fonts::control::TryFontRead<'f>>(
        &self,
        data: read_fonts::FontData<'f>,
    ) -> Option<T> {
        if self.step(0).is_err() {
            return None;
        }
        match &self.0 {
            Some(r) => self.font(T::try_read(data, &mut RequestControl(r))),
            None => T::read(data).ok(),
        }
    }
    pub(crate) fn class_get(
        &self,
        table: &read_fonts::tables::layout::ClassDef<'_>,
        glyph: impl Into<read_fonts::types::GlyphId>,
    ) -> Result<u16, Error> {
        // Audited closed scalar kernel: Format1 has one fixed slice index;
        // Format2 binary-searches fixed ClassRangeRecords, with no reader,
        // allocation, callback, or input-length-dependent comparator.
        let comparisons = match table {
            read_fonts::tables::layout::ClassDef::Format1(_) => 1,
            read_fonts::tables::layout::ClassDef::Format2(t) => {
                (usize::BITS - t.class_range_records().len().leading_zeros() + 1) as u64
            }
        };
        self.step(comparisons)?;
        let result = table.get(glyph);
        self.step(0)?;
        Ok(result)
    }
    pub(crate) fn offset_get<'f, T, O>(
        &self,
        values: &read_fonts::ArrayOfOffsets<'f, T, O>,
        index: usize,
    ) -> Result<Result<T, read_fonts::ReadError>, Error>
    where
        T: read_fonts::control::TryFontReadWithArgs<'f>,
        T::Args: Copy + 'static,
        O: read_fonts::types::Scalar + read_fonts::control::TryResolveOffset,
    {
        self.step(0)?;
        match &self.0 {
            Some(r) => values.try_get(index, &mut RequestControl(r)),
            None => Ok(values.get(index)),
        }
    }
    pub(crate) fn nullable_get<'f, T, O>(
        &self,
        values: &read_fonts::ArrayOfNullableOffsets<'f, T, O>,
        index: usize,
    ) -> Result<Option<Result<T, read_fonts::ReadError>>, Error>
    where
        T: read_fonts::control::TryFontReadWithArgs<'f>,
        T::Args: Copy + 'static,
        O: read_fonts::types::Scalar + read_fonts::control::TryResolveOffset,
    {
        self.step(0)?;
        match &self.0 {
            Some(r) => values.try_get(index, &mut RequestControl(r)),
            None => Ok(values.get(index)),
        }
    }
    pub(crate) fn resolve<
        'f,
        T: read_fonts::control::TryFontRead<'f>,
        O: read_fonts::control::TryResolveOffset,
    >(
        &self,
        offset: O,
        data: read_fonts::FontData<'f>,
    ) -> Result<Result<T, read_fonts::ReadError>, Error> {
        self.step(0)?;
        match &self.0 {
            Some(r) => offset.try_resolve(data, &mut RequestControl(r)),
            None => Ok(offset.resolve(data)),
        }
    }
    pub(crate) fn resolve_args<
        'f,
        T: read_fonts::control::TryFontReadWithArgs<'f>,
        O: read_fonts::control::TryResolveOffset,
    >(
        &self,
        offset: O,
        data: read_fonts::FontData<'f>,
        args: &T::Args,
    ) -> Result<Result<T, read_fonts::ReadError>, Error> {
        self.step(0)?;
        match &self.0 {
            Some(r) => offset.try_resolve_with_args(data, args, &mut RequestControl(r)),
            None => Ok(offset.resolve_with_args(data, args)),
        }
    }
    pub(crate) fn resolve_nullable<
        'f,
        T: read_fonts::control::TryFontRead<'f>,
        O: read_fonts::control::TryResolveOffset + read_fonts::types::Scalar,
    >(
        &self,
        offset: read_fonts::types::Nullable<O>,
        data: read_fonts::FontData<'f>,
    ) -> Result<Option<Result<T, read_fonts::ReadError>>, Error>
    where
        read_fonts::types::Nullable<O>: read_fonts::control::TryResolveNullableOffset,
    {
        use read_fonts::control::TryResolveNullableOffset;
        self.step(0)?;
        match &self.0 {
            Some(r) => offset.try_resolve(data, &mut RequestControl(r)),
            None => Ok(offset.resolve(data)),
        }
    }
    pub(crate) fn table<'f, T: read_fonts::TopLevelTable + read_fonts::control::TryFontRead<'f>>(
        &self,
        font: &read_fonts::FontRef<'f>,
    ) -> Result<Option<T>, Error> {
        use read_fonts::TableProvider;
        match &self.0 {
            None => Ok(font.expect_table::<T>().ok()),
            Some(r) => {
                self.step(0)?;
                let Some(data) = font.try_table_data(T::TAG, &mut RequestControl(r))? else {
                    return Ok(None);
                };
                T::try_read(data, &mut RequestControl(r)).map(|font_result| font_result.ok())
            }
        }
    }
}

pub(crate) enum Storage<T> {
    Plain(Vec<T>),
    Paid(BudgetVec<T>),
}

pub(crate) fn sort_slice<T: Copy>(
    values: &mut [T],
    control: &Control,
    mut compare: impl FnMut(&T, &T) -> core::cmp::Ordering,
) -> Result<(), Error> {
    control.step(0)?;
    let Some(r) = &control.0 else {
        values.sort_by(compare);
        return Ok(());
    };
    let result = {
        let mut scratch = Storage::new(control);
        scratch.try_extend(values)?;
        let result = {
            let mut comparator = |a: &T, b: &T, _: &mut RequestControl<'_>| Ok(compare(a, b));
            markdown_svg_budget::slice::stable_sort_by_with_control(
                values,
                &mut scratch,
                &mut RequestControl(r),
                &mut comparator,
            )
            .map_err(|e| r.record(e))
        };
        drop(scratch);
        result
    };
    result?;
    r.checkpoint(0)
}

impl<T> Default for Storage<T> {
    fn default() -> Self {
        Self::Plain(Vec::new())
    }
}

impl<T> Storage<T> {
    pub(crate) fn new(control: &Control) -> Self {
        match &control.0 {
            Some(r) => Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
            None => Self::default(),
        }
    }
    pub(crate) fn control(&self) -> Control {
        Control(match self {
            Self::Plain(_) => None,
            Self::Paid(v) => Some(v.resources().clone()),
        })
    }
    pub(crate) fn try_reserve(&mut self, count: usize) -> Result<(), Error> {
        match self {
            Self::Plain(v) => {
                v.reserve(count);
                Ok(())
            }
            Self::Paid(v) => v.try_reserve(count),
        }
    }
    pub(crate) fn try_push(&mut self, value: T) -> Result<(), Error> {
        // Reserve before passing the value to a fallible method. On failure the
        // original error is recorded before the value's nested owners retire.
        let control = self.control();
        if let Err(error) = self.try_reserve(1) {
            return Err(control.record(error));
        }
        match self {
            Self::Plain(v) => v.push(value),
            Self::Paid(v) => v.push_reserved(value),
        }
        Ok(())
    }
    pub(crate) fn try_resize_with(
        &mut self,
        len: usize,
        mut make: impl FnMut() -> T,
    ) -> Result<(), Error> {
        let control = self.control();
        control.step(0)?;
        if len < self.len() {
            self.truncate(len);
            return control.step(0);
        }
        self.try_reserve(len - self.len())?;
        while self.len() < len {
            control.step(1)?;
            let value = make();
            match self {
                Self::Plain(v) => v.push(value),
                Self::Paid(v) => v.push_reserved(value),
            }
        }
        control.step(0)
    }
    pub(crate) fn pop(&mut self) -> Option<T> {
        match self {
            Self::Plain(v) => v.pop(),
            Self::Paid(v) => v.pop(),
        }
    }
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Plain(v) => v.clear(),
            Self::Paid(v) => v.clear(),
        }
    }
    pub(crate) fn truncate(&mut self, len: usize) {
        while self.len() > len {
            self.pop();
        }
    }
    pub(crate) fn as_slice(&self) -> &[T] {
        self
    }

    pub(crate) fn push(&mut self, value: T) {
        let control = self.control();
        if let Err(error) = self.try_push(value) {
            control.record(error);
        }
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        // Paid capacity is retained with its lease. The ordinary path keeps
        // its original allocator behavior.
        if let Self::Plain(v) = self {
            v.shrink_to_fit();
        }
    }
}

impl<T: Copy> Storage<T> {
    pub(crate) fn try_extend(&mut self, values: &[T]) -> Result<(), Error> {
        match self {
            Self::Plain(v) => {
                v.extend_from_slice(values);
                Ok(())
            }
            Self::Paid(v) => {
                v.try_reserve(values.len())?;
                let r = v.resources().clone();
                let window = 256.min(65536 / size_of::<T>().max(1));
                if window == 0 {
                    return Err(r.record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
                }
                for chunk in values.chunks(window) {
                    r.checkpoint((chunk.len() * size_of::<T>().max(1)) as u64)?;
                    v.extend_reserved_copy(chunk);
                }
                r.checkpoint(0)
            }
        }
    }
    pub(crate) fn try_resize(&mut self, len: usize, value: T) -> Result<(), Error> {
        match self {
            Self::Plain(v) => {
                v.resize(len, value);
                Ok(())
            }
            Self::Paid(v) => v.try_resize_copy(len, value),
        }
    }
    pub(crate) fn try_sort_by(
        &mut self,
        mut compare: impl FnMut(&T, &T) -> core::cmp::Ordering,
    ) -> Result<(), Error> {
        let control = self.control();
        match &control.0 {
            None => {
                self.deref_mut().sort_by(compare);
                Ok(())
            }
            Some(r) => {
                let mut scratch = Storage::new(&control);
                scratch.try_extend(self)?;
                let mut cmp = |a: &T, b: &T, _: &mut RequestControl<'_>| Ok(compare(a, b));
                let result = markdown_svg_budget::slice::stable_sort_by_with_control(
                    self,
                    &mut scratch,
                    &mut RequestControl(r),
                    &mut cmp,
                )
                .map_err(|e| r.record(e));
                drop(cmp);
                drop(scratch);
                result?;
                r.checkpoint(0)
            }
        }
    }

    pub(crate) fn sort(&mut self)
    where
        T: Ord,
    {
        let control = self.control();
        if let Err(e) = self.try_sort_by(T::cmp) {
            control.record(e);
        }
    }
}

impl<T> Deref for Storage<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v.as_slice(),
        }
    }
}
impl<T> DerefMut for Storage<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v.as_mut_slice(),
        }
    }
}
impl<'a, T> IntoIterator for &'a Storage<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut Storage<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// One allocation with unique ownership. The paid representation uses the
/// existing allocator-backed vector, and admits its exact capacity first.
pub(crate) enum Owned<T> {
    Plain(Box<T>),
    Paid(BudgetVec<T>),
}
impl<T> Owned<T> {
    pub(crate) fn try_new_optional(
        control: &Control,
        make: impl FnOnce() -> Result<Option<T>, Error>,
    ) -> Result<Option<Self>, Error> {
        control.step(0)?;
        match &control.0 {
            None => Ok(make()?.map(|v| Self::Plain(Box::new(v)))),
            Some(r) => {
                let mut values = BudgetVec::new(r.clone(), ResourceKind::Scratch);
                values.try_reserve_exact(1)?;
                let value = make().map_err(|e| r.record(e))?;
                r.checkpoint(0)?;
                if let Some(value) = value {
                    values.push_reserved(value);
                    r.checkpoint(0)?;
                    Ok(Some(Self::Paid(values)))
                } else {
                    drop(values);
                    r.checkpoint(0)?;
                    Ok(None)
                }
            }
        }
    }
    pub(crate) fn try_new(
        control: &Control,
        make: impl FnOnce() -> Result<T, Error>,
    ) -> Result<Self, Error> {
        control.step(0)?;
        match &control.0 {
            None => Ok(Self::Plain(Box::new(make()?))),
            Some(r) => {
                if size_of::<T>() == 0 {
                    return Err(r.record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
                }
                let mut values = BudgetVec::new(r.clone(), ResourceKind::Scratch);
                values.try_reserve_exact(1)?;
                let value = make().map_err(|e| r.record(e))?;
                r.checkpoint(0)?;
                values.push_reserved(value);
                r.checkpoint(0)?;
                Ok(Self::Paid(values))
            }
        }
    }
}
impl<T> Deref for Owned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => &v.as_slice()[0],
        }
    }
}
impl<T> DerefMut for Owned<T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => &mut v.as_mut_slice()[0],
        }
    }
}

/// The original fixed inline capacity followed by an explicitly admitted heap.
pub(crate) struct Inline<T: Copy, const N: usize> {
    inline: [T; N],
    zero: T,
    len: usize,
    heap: Option<Storage<T>>,
    control: Control,
}
impl<T: Copy, const N: usize> Inline<T, N> {
    pub(crate) fn new(control: &Control) -> Self
    where
        T: Default,
    {
        Self::new_with_value(control, T::default())
    }
    pub(crate) fn new_with_value(control: &Control, zero: T) -> Self {
        Self {
            inline: [zero; N],
            zero,
            len: 0,
            heap: None,
            control: control.clone(),
        }
    }
    pub(crate) fn clear(&mut self) {
        if let Some(heap) = &mut self.heap {
            heap.clear();
        } else {
            self.len = 0;
        }
    }
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        self.control.step(0)?;
        let required = self.len().checked_add(additional).ok_or_else(|| {
            self.control
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
        })?;
        if let Some(heap) = &mut self.heap {
            return heap.try_reserve(additional);
        }
        if required > N {
            let mut heap = Storage::new(&self.control);
            heap.try_reserve(required)?;
            heap.try_extend(&self.inline[..self.len])?;
            self.heap = Some(heap);
            self.len = 0;
        }
        self.control.step(0)
    }
    pub(crate) fn try_push(&mut self, value: T) -> Result<(), Error> {
        let len = self.len();
        self.try_resize(
            len.checked_add(1).ok_or_else(|| {
                self.control
                    .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
            })?,
            self.zero,
        )?;
        self[len] = value;
        Ok(())
    }
    pub(crate) fn try_extend(&mut self, input: &[T]) -> Result<(), Error> {
        let start = self.len();
        self.try_resize(
            start.checked_add(input.len()).ok_or_else(|| {
                self.control
                    .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
            })?,
            self.zero,
        )?;
        let control = self.control.clone();
        let width = 256.min(65536 / size_of::<T>().max(1));
        if width == 0 {
            return Err(control.record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        for (target, input) in self[start..].chunks_mut(width).zip(input.chunks(width)) {
            control.step((input.len() * size_of::<T>().max(1)) as u64)?;
            target.copy_from_slice(input);
        }
        control.step(0)
    }
    pub(crate) fn try_resize(&mut self, len: usize, value: T) -> Result<(), Error> {
        self.control.step(0)?;
        if let Some(heap) = &mut self.heap {
            return heap.try_resize(len, value);
        }
        if len <= N {
            if len > self.len {
                self.control
                    .step(((len - self.len) * size_of::<T>()) as u64)?;
                self.inline[self.len..len].fill(value);
            }
            self.len = len;
            return Ok(());
        }
        let mut heap = Storage::new(&self.control);
        heap.try_reserve(len)?;
        heap.try_extend(&self.inline[..self.len])?;
        heap.try_resize(len, value)?;
        self.heap = Some(heap);
        self.len = 0;
        self.control.step(0)
    }
    pub(crate) fn try_clone(&self) -> Result<Self, Error> {
        let mut copy = Self::new_with_value(&self.control, self.zero);
        copy.try_resize(self.len(), self.zero)?;
        match &self.control.0 {
            Some(r) => {
                for (out, input) in copy.chunks_mut(256).zip(self.chunks(256)) {
                    r.checkpoint((input.len() * size_of::<T>()) as u64)?;
                    out.copy_from_slice(input);
                }
            }
            None => copy.copy_from_slice(self),
        }
        Ok(copy)
    }
}
impl<T: Copy, const N: usize> Deref for Inline<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match &self.heap {
            Some(h) => h,
            None => &self.inline[..self.len],
        }
    }
}
impl<T: Copy, const N: usize> DerefMut for Inline<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        match &mut self.heap {
            Some(h) => h,
            None => &mut self.inline[..self.len],
        }
    }
}

#[cfg(feature = "std")]
type PlainSet = read_fonts::collections::int_set::U32Set;
#[cfg(not(feature = "std"))]
type PlainSet = crate::digest_u32_set::DigestU32Set;

/// Exact disjoint inclusive ranges in request mode. Ordinary caches retain
/// their original platform-selected set representation.
pub(crate) enum GlyphSet {
    Plain(PlainSet),
    Paid(Storage<(u32, u32)>),
}
impl GlyphSet {
    pub(crate) fn default() -> Self {
        Self::Plain(PlainSet::default())
    }
    pub(crate) fn new(control: &Control) -> Self {
        if control.0.is_some() {
            Self::Paid(Storage::new(control))
        } else {
            Self::default()
        }
    }
    pub(crate) fn control(&self) -> Control {
        match self {
            Self::Plain(_) => Control::default(),
            Self::Paid(r) => r.control(),
        }
    }
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Plain(v) => v.clear(),
            Self::Paid(v) => v.clear(),
        }
    }
    pub(crate) fn insert(&mut self, value: u32) {
        self.insert_range(value..=value)
    }
    pub(crate) fn insert_range(&mut self, range: core::ops::RangeInclusive<u32>) {
        let control = self.control();
        let result = self.try_insert_range(*range.start(), *range.end());
        if let Err(error) = result {
            control.record(error);
        }
    }
    fn try_insert_range(&mut self, mut start: u32, mut end: u32) -> Result<(), Error> {
        if start > end {
            return Ok(());
        }
        let Self::Paid(values) = self else {
            if let Self::Plain(v) = self {
                v.insert_range(start..=end);
            }
            return Ok(());
        };
        let control = values.control();
        control.step(0)?;
        // Locate the first possible overlap without scanning every disjoint
        // prefix again for each ascending glyph inserted into a cache.
        control.step((usize::BITS - values.len().max(1).leading_zeros() + 1) as u64)?;
        let at = values.partition_point(|range| range.1.saturating_add(1) < start);
        let mut after = at;
        while after < values.len() {
            control.step(1)?;
            if values[after].0 > end.saturating_add(1) {
                break;
            }
            start = start.min(values[after].0);
            end = end.max(values[after].1);
            after += 1;
        }
        let old = values.len();
        let r = control.0.as_ref().unwrap();
        if at == after {
            values.try_resize(
                old.checked_add(1)
                    .ok_or_else(|| r.record(Error::new(FailureKind::Quota, LimitKind::Capacity)))?,
                (0, 0),
            )?;
            markdown_svg_budget::slice::copy_within_with_control(
                values,
                at..old,
                at + 1,
                &mut RequestControl(r),
            )
            .map_err(|e| r.record(e))?;
        } else {
            markdown_svg_budget::slice::copy_within_with_control(
                values,
                after..old,
                at + 1,
                &mut RequestControl(r),
            )
            .map_err(|e| r.record(e))?;
            values.truncate(old - (after - at) + 1);
        }
        values[at] = (start, end);
        r.checkpoint(0)
    }
    pub(crate) fn extend_unsorted(&mut self, iter: impl IntoIterator<Item = u32>) {
        let control = self.control();
        for value in iter {
            if control.step(1).is_err() {
                return;
            }
            self.insert(value);
            if control.step(0).is_err() {
                return;
            }
        }
    }
    pub(crate) fn contains(&self, value: u32) -> bool {
        match self {
            Self::Plain(v) => v.contains(value),
            Self::Paid(v) => {
                let control = v.control();
                if control
                    .step(1 + usize::BITS as u64 - v.len().max(1).leading_zeros() as u64)
                    .is_err()
                {
                    return false;
                }
                v.binary_search_by(|&(start, end)| {
                    if value < start {
                        core::cmp::Ordering::Greater
                    } else if value > end {
                        core::cmp::Ordering::Less
                    } else {
                        core::cmp::Ordering::Equal
                    }
                })
                .is_ok()
            }
        }
    }
    pub(crate) fn intersects_set(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Plain(a), Self::Plain(b)) => a.intersects_set(b),
            (Self::Paid(a), Self::Paid(b)) => {
                let control = a.control();
                if !control
                    .0
                    .as_ref()
                    .unwrap()
                    .is_same_request(b.control().0.as_ref().unwrap())
                {
                    control.record(Error::new(FailureKind::Invariant, LimitKind::None));
                    b.control()
                        .record(Error::new(FailureKind::Invariant, LimitKind::None));
                    return false;
                }
                let (mut i, mut j) = (0, 0);
                while i < a.len() && j < b.len() {
                    if control.step(1).is_err() {
                        return false;
                    }
                    if a[i].1 < b[j].0 {
                        i += 1;
                    } else if b[j].1 < a[i].0 {
                        j += 1;
                    } else {
                        return true;
                    }
                }
                false
            }
            _ => {
                self.control()
                    .record(Error::new(FailureKind::Invariant, LimitKind::None));
                other
                    .control()
                    .record(Error::new(FailureKind::Invariant, LimitKind::None));
                false
            }
        }
    }
}
