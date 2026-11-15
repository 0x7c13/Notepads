// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Decoded D-run selection only. No decoding, AST reparenting, raw-source
//! envelope or source capability is produced by this numeric private module.

use super::super::{
    Context, Host, NativeNodeId, NativeViolation,
    rows::{NativeRows, charge_host},
};
use std::marker::PhantomData;

#[path = "email/matcher.rs"]
mod matcher;

type Invariant<'run> = PhantomData<fn(&'run ()) -> &'run ()>;

#[derive(Clone, Copy)]
pub(crate) struct RunBrand<'run> {
    node: NativeNodeId,
    bytes: usize,
    invariant: Invariant<'run>,
}

pub(crate) fn with_run<T>(
    node: NativeNodeId,
    bytes: usize,
    body: impl for<'run> FnOnce(RunBrand<'run>) -> T,
) -> T {
    body(RunBrand {
        node,
        bytes,
        invariant: PhantomData,
    })
}

#[derive(Clone, Copy)]
pub(crate) struct EmailMatch<'run> {
    start: usize,
    at: usize,
    end: usize,
    invariant: Invariant<'run>,
}
impl EmailMatch<'_> {
    pub(crate) fn start(self) -> usize {
        self.start
    }
    pub(crate) fn at(self) -> usize {
        self.at
    }
    pub(crate) fn end(self) -> usize {
        self.end
    }
}

pub(crate) struct EmailSelection<'run, L> {
    run: RunBrand<'run>,
    selected: NativeRows<EmailMatch<'run>, L>,
    next: usize,
}
impl<'run, L> EmailSelection<'run, L> {
    pub(crate) fn next<H: Host<Lease = L>>(
        &mut self,
        node: NativeNodeId,
        bytes: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<Option<EmailMatch<'run>>, H::Error> {
        ctx.event()?;
        if self.run.node != node || self.run.bytes != bytes {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let row = self.selected.get(self.next, |n| charge_host(ctx.host, n))?;
        if row.is_some() {
            charge_host(ctx.host, std::mem::size_of::<EmailMatch<'_>>())?;
        }
        let row = row.copied();
        if row.is_some() {
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        }
        Ok(row)
    }
}

pub(crate) fn select<'run, H: Host>(
    decoded: &str,
    run: RunBrand<'run>,
    ctx: &mut Context<'_, H>,
) -> Result<EmailSelection<'run, H::Lease>, H::Error> {
    matcher::select(decoded, run, ctx)
}
