// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! A single operation moves with its request; no later stage can renew it.

use super::{CompletedParse, ParseRequest, SessionScope, SourceSession};
use crate::config::ParseConfig;
use crate::error::CoreError;
use crate::identity::{ModelIdentity, SourceKey};
use crate::model::AcceptedModel;
use crate::policy::{BoundaryWork, CancellationToken, Operation, SourceBudget};
use crate::source::{EditBatch, SourceSnapshot};
use std::sync::Arc;

#[derive(Clone, Copy, Eq, PartialEq)]
struct Origin {
    epoch: u64,
    generation: u64,
    source: Option<SourceKey>,
    config: ParseConfig,
}

/// A request starts before input conversion and does not keep a session borrowed.
/// Binding consumes this value and checks its original, immutable authority.
///
/// ```compile_fail,E0599
/// use markdown_core::session::PendingSourceRequest;
/// fn duplicate(request: PendingSourceRequest) { let _ = request.clone(); }
/// ```
/// ```compile_fail,E0616
/// use markdown_core::session::PendingSourceRequest;
/// fn renew(request: PendingSourceRequest) { let _ = request.op; }
/// ```
pub struct PendingSourceRequest {
    op: Operation,
    scope: Arc<SessionScope>,
    origin: Origin,
}

impl PendingSourceRequest {
    pub fn cancellation(&self) -> CancellationToken {
        self.op.cancellation()
    }

    pub fn boundary_work(&mut self) -> BoundaryWork<'_> {
        self.op.boundary_work()
    }

    /// Identity only; reading another session's state is not authorized by this.
    pub fn matches_scope(&self, session: &SourceSession) -> bool {
        Arc::ptr_eq(&self.scope, &session.scope)
    }

    pub fn bind(mut self, session: &mut SourceSession) -> Result<SourceTransaction<'_>, CoreError> {
        self.op.ensure_budget(&session.budget)?;
        self.op.checkpoint_now()?;
        self.op.view_scan(size_of::<Origin>() as u64)?;
        if !self.matches_scope(session) || self.origin != session.origin() {
            return Err(self.op.fail(CoreError::StaleResult));
        }
        self.op.checkpoint_now()?;
        let cancel = self.op.cancellation();
        Ok(SourceTransaction {
            session,
            op: self.op,
            cancel,
        })
    }
}

/// Exclusive source actions followed by a single capture, using one operation.
///
/// Successful source actions remain committed when this value is dropped or
/// later parsing fails. A transaction is a request lifetime, not a rollback log.
/// Capture consumes the session borrow, so queueing and parsing do not hold it.
///
/// ```compile_fail,E0599
/// use markdown_core::session::SourceTransaction;
/// fn duplicate(transaction: SourceTransaction<'_>) {
///     let _ = transaction.clone();
/// }
/// ```
pub struct SourceTransaction<'session> {
    session: &'session mut SourceSession,
    op: Operation,
    // A handle to the operation's same flag, not independent cancellation state.
    cancel: CancellationToken,
}

/// A single-use immutable source ticket and its original operation.
///
/// The operation keeps running while this request is queued. Execution accepts
/// no replacement operation and checks the original deadline before parsing.
///
/// ```compile_fail,E0599
/// use markdown_core::session::OwnedParseRequest;
/// fn duplicate(request: OwnedParseRequest) { let _ = request.clone(); }
/// ```
/// ```compile_fail,E0061
/// use markdown_core::{policy::Operation, session::OwnedParseRequest};
/// fn renew(request: OwnedParseRequest, replacement: &mut Operation) {
///     let _ = request.execute(replacement);
/// }
/// ```
/// ```compile_fail,E0616
/// use markdown_core::session::OwnedParseRequest;
/// fn extract(request: OwnedParseRequest) { let _ = request.request; }
/// ```
/// ```compile_fail,E0616
/// use markdown_core::session::OwnedParseRequest;
/// fn extract_operation(request: OwnedParseRequest) { let _ = request.op; }
/// ```
pub struct OwnedParseRequest {
    request: ParseRequest,
    op: Operation,
}

/// A real completed model that still owes the original installation deadline.
///
/// The model remains private until installation validates its current ticket.
/// Dropping a completion releases its owners without publishing the model.
///
/// ```compile_fail,E0599
/// use markdown_core::session::OwnedCompletedParse;
/// fn duplicate(completed: OwnedCompletedParse) { let _ = completed.clone(); }
/// ```
/// ```compile_fail,E0061
/// use markdown_core::{policy::Operation, session::{OwnedCompletedParse, SourceSession}};
/// fn renew(session: &mut SourceSession, completed: OwnedCompletedParse, op: &mut Operation) {
///     let _ = session.install_owned(completed, op);
/// }
/// ```
/// ```compile_fail,E0616
/// use markdown_core::session::OwnedCompletedParse;
/// fn extract(completed: OwnedCompletedParse) { let _ = completed.completed; }
/// ```
pub struct OwnedCompletedParse {
    completed: CompletedParse,
    op: Operation,
}

impl SourceSession {
    fn origin(&self) -> Origin {
        Origin {
            epoch: self.epoch,
            generation: self.generation,
            source: self.snapshot().map(|source| source.key()),
            config: self.config,
        }
    }

    /// The first clock reading precedes even the fresh cancellation allocation.
    /// This is the only complete native update bootstrap, not a renewed begin.
    pub fn start_request(
        &self,
        initially_cancelled: bool,
    ) -> Result<PendingSourceRequest, CoreError> {
        let mut op = self.budget.begin_cancellable(initially_cancelled)?;
        op.view_scan(size_of::<Origin>() as u64)?;
        let origin = self.origin();
        let scope = self.scope.clone();
        op.checkpoint_now()?;
        Ok(PendingSourceRequest { op, scope, origin })
    }

    /// Create an empty control owner. This does no source or parse work.
    ///
    /// Initialization is separately admitted; every actual update must begin a
    /// transaction before input validation, conversion or copying. The host
    /// remains responsible for beginning at actual request receipt.
    pub fn transactional(
        config: ParseConfig,
        budget: SourceBudget,
        cancel: CancellationToken,
    ) -> Result<Self, CoreError> {
        let mut op = budget.begin(cancel);
        Self::unattached_inner(config, &mut op)
    }

    /// Start the update's only operation before any source work.
    ///
    /// An initially cancelled/faulted operation remains faulted: its first
    /// action or capture will return that error without changing session state.
    pub fn begin_transaction(&mut self, cancel: CancellationToken) -> SourceTransaction<'_> {
        let op = self.budget.begin(cancel.clone());
        SourceTransaction {
            session: self,
            op,
            cancel,
        }
    }

    /// Install using the completion's original operation, including wait time.
    /// Later model queries are separate operations with their own lifetimes.
    pub fn install_owned(
        &mut self,
        completed: OwnedCompletedParse,
    ) -> Result<AcceptedModel, CoreError> {
        let OwnedCompletedParse { completed, mut op } = completed;
        self.install_inner(completed, &mut op)
    }
}

impl SourceTransaction<'_> {
    pub fn boundary_work(&mut self) -> BoundaryWork<'_> {
        self.op.boundary_work()
    }

    pub fn resync(&mut self, key: SourceKey, utf8: &[u8]) -> Result<SourceSnapshot, CoreError> {
        self.session.resync_inner(key, utf8, &mut self.op)
    }

    pub fn resync_with_config(
        &mut self,
        key: SourceKey,
        utf8: &[u8],
        config: ParseConfig,
    ) -> Result<SourceSnapshot, CoreError> {
        self.session
            .resync_with_config_inner(key, utf8, config, &mut self.op)
    }

    pub fn resync_utf16_with_config(
        &mut self,
        key: SourceKey,
        utf16: &[u16],
        config: ParseConfig,
    ) -> Result<SourceSnapshot, CoreError> {
        self.session
            .resync_utf16_inner(key, utf16, config, &mut self.op)
    }

    pub fn apply(&mut self, batch: EditBatch<'_>) -> Result<SourceSnapshot, CoreError> {
        self.session.apply_inner(batch, &mut self.op)
    }

    pub fn detach(&mut self) -> Result<(), CoreError> {
        self.session.detach_inner(&mut self.op)
    }

    pub fn set_config(&mut self, config: ParseConfig) -> Result<(), CoreError> {
        self.session.set_config_inner(config, &mut self.op)
    }

    /// Release exclusive source access and move the same operation into a ticket.
    pub fn capture(mut self) -> Result<OwnedParseRequest, CoreError> {
        let request = self.session.capture_inner(self.cancel, &mut self.op)?;
        Ok(OwnedParseRequest {
            request,
            op: self.op,
        })
    }
}

impl OwnedParseRequest {
    pub fn boundary_work(&mut self) -> BoundaryWork<'_> {
        self.op.boundary_work()
    }

    pub fn identity(&self) -> ModelIdentity {
        self.request.identity()
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.request.cancellation()
    }

    pub fn execute(self) -> Result<OwnedCompletedParse, CoreError> {
        let Self { request, mut op } = self;
        let completed = request.execute_inner(&mut op)?;
        Ok(OwnedCompletedParse { completed, op })
    }
}

impl OwnedCompletedParse {
    pub fn boundary_work(&mut self) -> BoundaryWork<'_> {
        self.op.boundary_work()
    }

    pub fn identity(&self) -> ModelIdentity {
        self.completed.ticket.identity
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.completed.cancel.clone()
    }
}

#[cfg(test)]
#[path = "owned_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod boundary_tests;
