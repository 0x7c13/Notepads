// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Shared work protocol for private geometry algorithms. This trait neither
//! owns a request nor creates an account; callers supply the original policy.
use core::{convert::Infallible, marker::PhantomData};

use crate::{Error, ResourcesRef};

/// Check and pay before an action; zero checks the caller's first-failure slot.
/// Implementations must preserve their original error without allocating it.
pub trait WorkControl {
    type Error;

    fn checkpoint(&mut self, work: u64) -> Result<(), Self::Error>;
}

/// Ordinary facade policy. It never calls a backend or manufactures an error.
/// The generic error also permits an unchanged fallible mathematical callback.
pub struct NoControl<E = Infallible>(PhantomData<fn() -> E>);

impl<E> NoControl<E> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<E> Default for NoControl<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> WorkControl for NoControl<E> {
    type Error = E;

    #[inline]
    fn checkpoint(&mut self, _work: u64) -> Result<(), E> {
        Ok(())
    }
}

/// Borrow the existing request; this adapter creates no owner or account.
pub struct RequestControl<'a>(pub &'a ResourcesRef);

impl WorkControl for RequestControl<'_> {
    type Error = Error;

    #[inline]
    fn checkpoint(&mut self, work: u64) -> Result<(), Error> {
        self.0.checkpoint(work)
    }
}
