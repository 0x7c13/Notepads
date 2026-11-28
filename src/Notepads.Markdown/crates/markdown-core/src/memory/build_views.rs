// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Physical views cannot outlive the finite window or survive an allocation.

use super::{BuildExactRows, BuildGrowingVec, BuildStep, BuildString, BuildValue, BuildWindow};
#[cfg(test)]
use super::{BuildGrowingText, BuildRecord, FrozenRows};
use crate::error::CoreError;
use crate::identity::SourceSpan;
use crate::policy::ConstructionId;
use std::cmp::Ordering;
use std::marker::PhantomData;

pub(crate) struct BuildExactRead<'scope, 'id, T: BuildValue> {
    values: &'scope [T],
    scope: ConstructionId<'scope>,
    id: ConstructionId<'id>,
}
pub(crate) type BuildGrowingRead<'scope, 'id, T> = BuildExactRead<'scope, 'id, T>;
pub(crate) struct BuildExactWrite<'scope, 'id, T: BuildValue> {
    values: Option<&'scope mut Vec<T>>,
    count: usize,
    scope: ConstructionId<'scope>,
    id: ConstructionId<'id>,
}
#[cfg(test)]
pub(crate) struct BuildGrowingWrite<'scope, 'id, T: BuildValue> {
    values: &'scope mut [T],
    scope: ConstructionId<'scope>,
    id: ConstructionId<'id>,
}
impl<'scope, 'call, 'id, 'op> BuildWindow<'scope, 'call, 'id, 'op> {
    pub(crate) fn read_exact<T: BuildValue>(
        &mut self,
        rows: &'scope BuildExactRows<'id, T>,
    ) -> Result<BuildStep<BuildExactRead<'scope, 'id, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<BuildExactRows<'id, T>>()? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(BuildExactRead {
            values: rows.as_slice(),
            scope: PhantomData,
            id: PhantomData,
        }))
    }
    #[cfg(test)]
    pub(crate) fn read_frozen<T: BuildValue>(
        &mut self,
        rows: &'scope FrozenRows<T>,
    ) -> Result<BuildStep<BuildExactRead<'scope, 'id, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<FrozenRows<T>>()? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(BuildExactRead {
            values: rows.as_slice(),
            scope: PhantomData,
            id: PhantomData,
        }))
    }
    pub(crate) fn write_exact<T: BuildValue>(
        &mut self,
        rows: &'scope mut BuildExactRows<'id, T>,
    ) -> Result<BuildStep<BuildExactWrite<'scope, 'id, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<BuildExactRows<'id, T>>()? {
            return Ok(BuildStep::Full);
        }
        if rows.data.is_none() && rows.count != 0 {
            return Ok(BuildStep::NeedCapacity(rows.count));
        }
        Ok(BuildStep::Ready(BuildExactWrite {
            values: rows.data.as_mut().map(|data| &mut data.values),
            count: rows.count,
            scope: PhantomData,
            id: PhantomData,
        }))
    }
    pub(crate) fn read_growing<T: BuildValue>(
        &mut self,
        rows: &'scope BuildGrowingVec<'id, T>,
    ) -> Result<BuildStep<BuildGrowingRead<'scope, 'id, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<BuildGrowingVec<'id, T>>()? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(BuildExactRead {
            values: rows.as_slice(),
            scope: PhantomData,
            id: PhantomData,
        }))
    }
    #[cfg(test)]
    pub(crate) fn write_growing<T: BuildValue>(
        &mut self,
        rows: &'scope mut BuildGrowingVec<'id, T>,
    ) -> Result<BuildStep<BuildGrowingWrite<'scope, 'id, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<BuildGrowingVec<'id, T>>()? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(BuildGrowingWrite {
            values: &mut rows.values,
            scope: PhantomData,
            id: PhantomData,
        }))
    }
}
fn row_bytes<T: BuildValue>() -> u64 {
    (4 * size_of::<T>() + 2 * size_of::<BuildExactWrite<'_, '_, T>>()) as u64
}
impl<'scope, 'id, T: BuildValue> BuildExactRead<'scope, 'id, T> {
    pub(crate) fn get(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Option<T>>, CoreError> {
        if !window.action(row_bytes::<T>())? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(self.values.get(index).copied()))
    }
}
#[cfg(test)]
impl<'scope, 'id, T: BuildRecord> BuildExactRead<'scope, 'id, T> {
    pub(crate) fn record(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<T::Output>, CoreError> {
        window.value::<T::Output>()?;
        if !window.action(row_bytes::<T>() + (4 * size_of::<T::Output>()) as u64)? {
            return Ok(BuildStep::Full);
        }
        window.parts().event()?;
        let row = self
            .values
            .get(index)
            .ok_or_else(|| window.fail(CoreError::InvalidProvenance))?;
        row.project(index, window.parts())
            .map(BuildStep::Ready)
            .map_err(|error| window.fail(error))
    }
}
#[cfg(test)]
impl<'scope, 'id, T: BuildValue> BuildGrowingWrite<'scope, 'id, T> {
    pub(crate) fn get(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Option<T>>, CoreError> {
        if !window.action(row_bytes::<T>())? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(self.values.get(index).copied()))
    }
    pub(crate) fn replace(
        &mut self,
        index: usize,
        value: T,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        if !window.action(row_bytes::<T>())? {
            return Ok(BuildStep::Full);
        }
        window.parts().event()?;
        window.parts().copy(size_of::<T>() as u64)?;
        let slot = self
            .values
            .get_mut(index)
            .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
        *slot = value;
        Ok(BuildStep::Ready(()))
    }
}
impl<'scope, 'id, T: BuildValue> BuildExactWrite<'scope, 'id, T> {
    #[cfg(test)]
    pub(crate) fn replace(
        &mut self,
        index: usize,
        row: T,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        if !window.action(row_bytes::<T>())? {
            return Ok(BuildStep::Full);
        }
        window.parts().event()?;
        let slot = self
            .values
            .as_mut()
            .and_then(|values| values.get_mut(index))
            .ok_or_else(|| window.fail(CoreError::InvalidProvenance))?;
        window.parts().copy(size_of::<T>() as u64)?;
        *slot = row;
        Ok(BuildStep::Ready(()))
    }
    pub(crate) fn push(
        &mut self,
        row: T,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        if !window.action(row_bytes::<T>())? {
            return Ok(BuildStep::Full);
        }
        window.parts().event()?;
        let len = self.values.as_ref().map_or(0, |values| values.len());
        if len >= self.count {
            return Err(window.fail(CoreError::InvalidProvenance));
        }
        let values = self
            .values
            .as_mut()
            .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
        window.parts().copy(size_of::<T>() as u64)?;
        window.parts().event()?;
        if values.len() == values.capacity() {
            return Err(window.fail(CoreError::InternalInvariant));
        }
        values.push(row);
        Ok(BuildStep::Ready(()))
    }
    #[cfg(test)]
    pub(crate) fn push_and_mark(
        &mut self,
        row: T,
        states: &mut BuildExactWrite<'scope, 'id, u8>,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        if !window.action(row_bytes::<T>() + row_bytes::<u8>())? {
            return Ok(BuildStep::Full);
        }
        window.parts().event()?;
        let len = self.values.as_ref().map_or(0, |values| values.len());
        if len >= self.count || len != index {
            return Err(window.fail(CoreError::InvalidProvenance));
        }
        let values = self
            .values
            .as_mut()
            .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
        window.parts().copy(size_of::<T>() as u64)?;
        window.parts().event()?;
        if values.len() == values.capacity() {
            return Err(window.fail(CoreError::InternalInvariant));
        }
        window.parts().event()?;
        let slot = states
            .values
            .as_mut()
            .and_then(|values| values.get_mut(index))
            .ok_or_else(|| window.fail(CoreError::InvalidProvenance))?;
        if *slot != 1 {
            return Err(window.fail(CoreError::InvalidProvenance));
        }
        window.parts().copy(1)?;
        // No fallible work remains between the actual packet and state writes.
        values.push(row);
        *slot = 2;
        Ok(BuildStep::Ready(()))
    }
}
#[cfg(test)]
impl<'scope, 'id> BuildExactRead<'scope, 'id, u8> {
    pub(crate) fn state(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Option<u8>>, CoreError> {
        if !window.action(row_bytes::<u8>())? {
            return Ok(BuildStep::Full);
        }
        window.parts().scan(1)?;
        Ok(BuildStep::Ready(self.values.get(index).copied()))
    }
}
#[cfg(test)]
impl<'scope, 'id> BuildExactWrite<'scope, 'id, u8> {
    pub(crate) fn state(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Option<u8>>, CoreError> {
        if !window.action(row_bytes::<u8>())? {
            return Ok(BuildStep::Full);
        }
        window.parts().scan(1)?;
        Ok(BuildStep::Ready(
            self.values
                .as_ref()
                .and_then(|values| values.get(index))
                .copied(),
        ))
    }
}

pub(crate) struct BuildBytePairs<'scope, 'id> {
    left: &'scope [u8],
    right: &'scope [u8],
    scope: ConstructionId<'scope>,
    id: ConstructionId<'id>,
}
pub(crate) struct BuildSourceText<'scope, 'id> {
    text: &'scope str,
    scope: ConstructionId<'scope>,
    id: ConstructionId<'id>,
}
#[derive(Clone, Copy)]
pub(crate) struct BuildTextChunk {
    bytes: [u8; 256],
    len: u16,
}
impl BuildValue for BuildTextChunk {}
impl BuildTextChunk {
    pub(crate) fn as_str(&self) -> &str {
        // Only chunk_next constructs this value, from a checked str boundary.
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("construction chunk preserves UTF8")
    }
    pub(crate) fn len(&self) -> usize {
        usize::from(self.len)
    }
}
impl<'scope, 'call, 'id, 'op> BuildWindow<'scope, 'call, 'id, 'op> {
    pub(crate) fn byte_pairs(
        &mut self,
        left: &'scope [u8],
        right: &'scope [u8],
    ) -> Result<BuildStep<BuildBytePairs<'scope, 'id>>, CoreError> {
        if !self.binding::<BuildBytePairs<'scope, 'id>>()? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(BuildBytePairs {
            left,
            right,
            scope: PhantomData,
            id: PhantomData,
        }))
    }
    /// Input identity/root is established at the owning construction entry.
    /// This view grants only endpoint/byte access, never provenance authority.
    pub(crate) fn source_text(
        &mut self,
        text: &'scope str,
    ) -> Result<BuildStep<BuildSourceText<'scope, 'id>>, CoreError> {
        if !self.binding::<BuildSourceText<'scope, 'id>>()? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(BuildSourceText {
            text,
            scope: PhantomData,
            id: PhantomData,
        }))
    }
    #[cfg(test)]
    pub(crate) fn growing_text(
        &mut self,
        text: &'scope BuildGrowingText<'id>,
    ) -> Result<BuildStep<BuildSourceText<'scope, 'id>>, CoreError> {
        self.source_text(text.as_str())
    }
    pub(crate) fn read_string(
        &mut self,
        text: &'scope BuildString<'id>,
    ) -> Result<BuildStep<BuildSourceText<'scope, 'id>>, CoreError> {
        self.source_text(text.as_str())
    }
}
impl<'scope, 'id> BuildBytePairs<'scope, 'id> {
    pub(crate) fn pair(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Ordering>, CoreError> {
        if !window.action((4 + 2 * size_of::<Self>()) as u64)? {
            return Ok(BuildStep::Full);
        }
        window.parts().scan(2)?;
        let left = self
            .left
            .get(index)
            .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
        let right = self
            .right
            .get(index)
            .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
        Ok(BuildStep::Ready(left.cmp(right)))
    }
    pub(crate) fn lengths(
        &self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Ordering>, CoreError> {
        if !window.action((2 * size_of::<Self>()) as u64)? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(self.left.len().cmp(&self.right.len())))
    }
}
impl<'scope, 'id> BuildSourceText<'scope, 'id> {
    pub(crate) fn chunk_next(
        &self,
        start: usize,
        upper_bound: usize,
        maximum: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<BuildTextChunk>, CoreError> {
        if maximum > 256 || (maximum == 0 && start != upper_bound) {
            return Err(window.fail(CoreError::InternalInvariant));
        }
        if !window.action((4 * size_of::<BuildTextChunk>() + 2 * size_of::<Self>() + 16) as u64)? {
            return Ok(BuildStep::Full);
        }
        if start > upper_bound
            || upper_bound > self.text.len()
            || !self.text.is_char_boundary(start)
            || !self.text.is_char_boundary(upper_bound)
        {
            return Err(window.fail(CoreError::InvalidBoundary));
        }
        let mut end = upper_bound.min(start.saturating_add(maximum));
        while !self.text.is_char_boundary(end) {
            window.parts().scan(1)?;
            end -= 1;
        }
        if end == start && start < upper_bound {
            return Err(window.fail(CoreError::InvalidBoundary));
        }
        let text = self
            .text
            .get(start..end)
            .ok_or_else(|| window.fail(CoreError::InvalidBoundary))?;
        window.parts().copy(text.len() as u64)?;
        let mut result = BuildTextChunk {
            bytes: [0; 256],
            len: text.len() as u16,
        };
        result.bytes[..text.len()].copy_from_slice(text.as_bytes());
        Ok(BuildStep::Ready(result))
    }
    pub(crate) fn validate_span(
        &self,
        span: SourceSpan,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        if !window.action((4 * size_of::<SourceSpan>() + 2 * size_of::<Self>() + 2) as u64)? {
            return Ok(BuildStep::Full);
        }
        let start = usize::try_from(span.start())
            .map_err(|_| window.fail(CoreError::ArithmeticOverflow))?;
        let end =
            usize::try_from(span.end()).map_err(|_| window.fail(CoreError::ArithmeticOverflow))?;
        if end > self.text.len()
            || !self.text.is_char_boundary(start)
            || !self.text.is_char_boundary(end)
        {
            return Err(window.fail(CoreError::InvalidBoundary));
        }
        Ok(BuildStep::Ready(()))
    }
}
