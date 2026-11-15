// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Inline storage retains the upstream capacities. Request spills have one
//! real payer and cannot be cloned into an ordinary Vec.

use crate::collections::SmallVec;
use alloc::vec::Vec;
use core::{
    convert::Infallible,
    ops::{Deref, DerefMut},
};
use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};

pub(crate) trait SmallBuffer<T>: Deref<Target = [T]> + DerefMut {
    type Error;
    fn as_slice(&self) -> &[T] {
        self
    }
    fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }
    fn try_push(&mut self, value: T) -> Result<(), Self::Error>;
    fn pop(&mut self) -> Option<T>;
    fn clear(&mut self);
    fn try_resize_and_fill(&mut self, len: usize, value: T) -> Result<(), Self::Error>;
}
impl<T: Copy + Default, const N: usize> SmallBuffer<T> for SmallVec<T, N> {
    type Error = Infallible;
    fn try_push(&mut self, value: T) -> Result<(), Infallible> {
        self.push(value);
        Ok(())
    }
    fn pop(&mut self) -> Option<T> {
        Self::pop(self)
    }
    fn clear(&mut self) {
        Self::clear(self)
    }
    fn try_resize_and_fill(&mut self, len: usize, value: T) -> Result<(), Infallible> {
        self.resize_and_fill(len, value);
        Ok(())
    }
}

pub(crate) struct PaidSmall<T, const N: usize> {
    // This SmallVec is never asked to grow past N, so it never spills.
    inline: SmallVec<T, N>,
    heap: Option<BudgetVec<T>>,
    resources: ResourcesRef,
}
impl<T: Copy + Default, const N: usize> PaidSmall<T, N> {
    pub(crate) fn resources(&self) -> &ResourcesRef {
        &self.resources
    }
    pub(crate) fn new(resources: &ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let bytes = N.checked_mul(core::mem::size_of::<T>());
        if N > 256 || bytes.is_none_or(|bytes| bytes > 65536) {
            return Err(resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity)));
        }
        resources.checkpoint(bytes.unwrap_or_default().max(N) as u64)?;
        let value = Self {
            inline: SmallVec::new(),
            heap: None,
            resources: resources.clone(),
        };
        resources.checkpoint(0)?;
        Ok(value)
    }
}
impl<T, const N: usize> Deref for PaidSmall<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match &self.heap {
            Some(heap) => heap.as_slice(),
            None => self.inline.as_slice(),
        }
    }
}
impl<T, const N: usize> DerefMut for PaidSmall<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        match &mut self.heap {
            Some(heap) => heap.as_mut_slice(),
            None => self.inline.as_mut_slice(),
        }
    }
}
impl<T: Copy + Default, const N: usize> SmallBuffer<T> for PaidSmall<T, N> {
    type Error = Error;
    fn try_push(&mut self, value: T) -> Result<(), Error> {
        self.resources
            .checkpoint(core::mem::size_of::<T>().max(1) as u64)?;
        if let Some(heap) = &mut self.heap {
            return heap.try_push(value);
        }
        if self.inline.len() < N {
            self.inline.push(value);
            return self.resources.checkpoint(0);
        }
        let mut heap = BudgetVec::new(self.resources.clone(), ResourceKind::Scratch);
        heap.try_reserve_exact(self.inline.len() + 1)?;
        // N and the complete initialized byte extent were checked before construction.
        self.resources.checkpoint(
            (self.inline.len() * core::mem::size_of::<T>()).max(self.inline.len()) as u64,
        )?;
        for &value in self.inline.iter() {
            heap.push_reserved(value);
        }
        self.resources.checkpoint(0)?;
        heap.push_reserved(value);
        self.inline.clear();
        self.heap = Some(heap);
        self.resources.checkpoint(0)
    }
    fn pop(&mut self) -> Option<T> {
        match &mut self.heap {
            Some(heap) => heap.pop(),
            None => self.inline.pop(),
        }
    }
    fn clear(&mut self) {
        match &mut self.heap {
            Some(heap) => heap.clear(),
            None => self.inline.clear(),
        }
    }
    fn try_resize_and_fill(&mut self, len: usize, value: T) -> Result<(), Error> {
        self.resources.checkpoint(0)?;
        if let Some(heap) = &mut self.heap {
            heap.clear();
            return heap.try_resize_copy(len, value);
        }
        if len <= N {
            self.resources
                .checkpoint((len * core::mem::size_of::<T>()).max(len) as u64)?;
            self.inline.resize_and_fill(len, value);
            return self.resources.checkpoint(0);
        }
        let mut heap = BudgetVec::new(self.resources.clone(), ResourceKind::Scratch);
        heap.try_resize_copy(len, value)?;
        self.inline.clear();
        self.heap = Some(heap);
        self.resources.checkpoint(0)
    }
}

pub(crate) trait Sequence<T> {
    type Error;
    fn as_slice(&self) -> &[T];
    fn as_mut_slice(&mut self) -> &mut [T];
    fn try_push(&mut self, value: T) -> Result<(), Self::Error>;
    fn pop(&mut self) -> Option<T>;
}
impl<T> Sequence<T> for Vec<T> {
    type Error = Infallible;
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
    fn as_mut_slice(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
    fn try_push(&mut self, value: T) -> Result<(), Infallible> {
        self.push(value);
        Ok(())
    }
    fn pop(&mut self) -> Option<T> {
        self.pop()
    }
}
impl<T> Sequence<T> for BudgetVec<T> {
    type Error = Error;
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
    fn as_mut_slice(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
    fn try_push(&mut self, value: T) -> Result<(), Error> {
        self.try_push(value)
    }
    fn pop(&mut self) -> Option<T> {
        self.pop()
    }
}
