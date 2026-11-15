// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! The same account and first failure throughout parse, font replay and render.
use crate::memory::{Context, release_ticket, reserve_ticket};
use markdown_svg_budget::{Backend, Error, ResourceKind, Ticket};

pub(crate) struct NativeBudgetBackend {
    context: Context,
}
impl NativeBudgetBackend {
    pub(crate) fn new(context: &Context) -> Self {
        Self {
            context: context.clone(),
        }
    }
}
impl Backend for NativeBudgetBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        reserve_ticket(&self.context, kind, bytes)
            .map_err(|_| self.context.resource_failure().unwrap())
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        self.context.allocation_committed(ticket, bytes)
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        release_ticket(&self.context, ticket).map_err(|_| self.context.resource_failure().unwrap())
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.context
            .checkpoint(work)
            .map_err(|_| self.context.resource_failure().unwrap())
    }
}
