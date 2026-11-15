// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Exclusive source lifecycle and single-use parse/install tickets.
//! A platform controller owns synchronization and its additional device/layout generations.
//! Owned transactions carry one operation through source work, queuing and installation.

mod owned;
pub use owned::{OwnedCompletedParse, OwnedParseRequest, PendingSourceRequest, SourceTransaction};

use crate::config::ParseConfig;
use crate::error::CoreError;
use crate::identity::{ModelIdentity, SourceKey};
use crate::model::AcceptedModel;
use crate::policy::{CancellationToken, Lease, Operation, SourceBudget};
use crate::source::{EditBatch, SourceDocument, SourceSnapshot};
use std::sync::Arc;

pub struct SourceSession {
    document: Option<SourceDocument>,
    budget: SourceBudget,
    config: ParseConfig,
    epoch: u64,
    generation: u64,
    needs_resync: bool,
    installed: Option<AcceptedModel>,
    scope: Arc<SessionScope>,
}

struct SessionScope {
    _control: Lease,
}

struct Ticket {
    identity: ModelIdentity,
    epoch: u64,
    scope: Arc<SessionScope>,
}

/// This value has no Clone: execution cannot mint another install ticket.
pub struct ParseRequest {
    source: SourceSnapshot,
    ticket: Ticket,
    cancel: CancellationToken,
}

/// Only a successful ParseRequest can produce a completed result.
pub struct CompletedParse {
    model: AcceptedModel,
    ticket: Ticket,
    cancel: CancellationToken,
}

impl ParseRequest {
    pub fn identity(&self) -> ModelIdentity {
        self.ticket.identity
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn execute(self) -> Result<CompletedParse, CoreError> {
        let mut op = self.source.budget().begin(self.cancel.clone());
        self.execute_with_operation(&mut op)
    }

    /// Continue the source transaction's counters and cancellation flag.
    pub fn execute_with_operation(self, op: &mut Operation) -> Result<CompletedParse, CoreError> {
        op.ensure_budget(self.source.budget())?;
        ensure_session_mode(op)?;
        self.execute_inner(op)
    }

    fn execute_inner(self, op: &mut Operation) -> Result<CompletedParse, CoreError> {
        op.ensure_budget(self.source.budget())?;
        op.checkpoint_now()?;
        if !op.uses_cancellation(&self.cancel) {
            return Err(op.fail(CoreError::InvalidState));
        }
        let model = AcceptedModel::parse(
            self.source,
            self.ticket.identity.config(),
            self.ticket.identity.generation(),
            op,
        )?;
        op.checkpoint_now()?;
        Ok(CompletedParse {
            model,
            ticket: self.ticket,
            cancel: self.cancel,
        })
    }
}

impl SourceSession {
    /// A real session identity with no source. Full resync is required first.
    pub fn unattached(config: ParseConfig, op: &mut Operation) -> Result<Self, CoreError> {
        ensure_session_mode(op)?;
        Self::unattached_inner(config, op)
    }

    fn unattached_inner(config: ParseConfig, op: &mut Operation) -> Result<Self, CoreError> {
        op.checkpoint_now()?;
        let control_bytes =
            size_of::<SessionScope>() + 2 * size_of::<usize>() + align_of::<SessionScope>() - 1;
        let control = op
            .budget()
            .reserve_auxiliary(control_bytes as u64)
            .map_err(|e| op.fail(e))?;
        let scope = Arc::new(SessionScope { _control: control });
        op.checkpoint_now()?;
        Ok(Self {
            document: None,
            budget: op.budget().clone(),
            config,
            epoch: 1,
            generation: 0,
            needs_resync: true,
            installed: None,
            scope,
        })
    }

    pub fn new(
        key: SourceKey,
        utf8: &[u8],
        config: ParseConfig,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        ensure_session_mode(op)?;
        let document = SourceDocument::new(key, utf8, op)?;
        let mut session = Self::unattached(config, op)?;
        session.document = Some(document);
        session.needs_resync = false;
        Ok(session)
    }

    pub fn budget(&self) -> &SourceBudget {
        &self.budget
    }

    pub fn snapshot(&self) -> Option<SourceSnapshot> {
        self.document.as_ref().map(SourceDocument::snapshot)
    }
    pub fn needs_resync(&self) -> bool {
        self.needs_resync
    }
    pub fn installed(&self) -> Option<&AcceptedModel> {
        self.installed.as_ref()
    }

    pub fn apply(
        &mut self,
        batch: EditBatch<'_>,
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        op.ensure_budget(&self.budget)?;
        ensure_session_mode(op)?;
        self.apply_inner(batch, op)
    }

    fn apply_inner(
        &mut self,
        batch: EditBatch<'_>,
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.checkpoint_now()?;
        let current = self
            .snapshot()
            .ok_or_else(|| op.fail(CoreError::InvalidState))?
            .key();
        if batch.before.document() != current.document()
            || batch.after.document() != current.document()
        {
            return Err(op.fail(CoreError::WrongDocument));
        }
        if batch.after.revision() <= current.revision() {
            return Err(op.fail(CoreError::WrongRevision {
                expected: current.revision(),
                actual: batch.after.revision(),
            }));
        }
        if self.needs_resync || batch.before != current {
            // Invalidate existing tickets without changing the accepted source.
            if !self.needs_resync {
                let epoch = self.next_epoch(op)?;
                op.checkpoint_now()?;
                self.epoch = epoch;
                self.needs_resync = true;
                self.installed = None;
            }
            return Err(op.fail(CoreError::ResyncRequired {
                expected: current.revision(),
                actual: batch.before.revision(),
            }));
        }
        let snapshot = self
            .document
            .as_mut()
            .ok_or_else(|| op.fail(CoreError::InvalidState))?
            .apply(batch, op)?;
        self.installed = None;
        Ok(snapshot)
    }

    /// Explicit full synchronization starts a new epoch, even for an identical source key.
    pub fn resync(
        &mut self,
        key: SourceKey,
        utf8: &[u8],
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        op.ensure_budget(&self.budget)?;
        ensure_session_mode(op)?;
        self.resync_inner(key, utf8, op)
    }

    fn resync_inner(
        &mut self,
        key: SourceKey,
        utf8: &[u8],
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.checkpoint_now()?;
        if let Some(current) = self.snapshot().filter(|source| source.key() == key) {
            let fault = CoreError::WrongRevision {
                expected: key.revision(),
                actual: key.revision(),
            };
            if current.text().len() != utf8.len() {
                return Err(op.fail(fault));
            }
            for (left, right) in current.text().bytes().zip(utf8.iter().copied()) {
                op.scan(2)?;
                if left != right {
                    return Err(op.fail(fault));
                }
            }
        }
        let epoch = self.next_epoch(op)?;
        let document = SourceDocument::new(key, utf8, op)?;
        let snapshot = document.snapshot();
        op.checkpoint_now()?;
        self.document = Some(document);
        self.epoch = epoch;
        self.needs_resync = false;
        self.installed = None;
        Ok(snapshot)
    }

    /// Source validation/allocation must succeed before changing either config
    /// or installed state. The resync checkpoint is the joint commit boundary.
    pub fn resync_with_config(
        &mut self,
        key: SourceKey,
        utf8: &[u8],
        config: ParseConfig,
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        op.ensure_budget(&self.budget)?;
        ensure_session_mode(op)?;
        self.resync_with_config_inner(key, utf8, config, op)
    }

    fn resync_with_config_inner(
        &mut self,
        key: SourceKey,
        utf8: &[u8],
        config: ParseConfig,
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        let snapshot = self.resync_inner(key, utf8, op)?;
        self.config = config;
        Ok(snapshot)
    }

    fn resync_utf16_inner(
        &mut self,
        key: SourceKey,
        utf16: &[u16],
        config: ParseConfig,
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.checkpoint_now()?;
        let current = self.snapshot().filter(|source| source.key() == key);
        let epoch = self.next_epoch(op)?;
        let document = SourceDocument::new_utf16_matching(key, utf16, current.as_ref(), op)?;
        let snapshot = document.snapshot();
        op.checkpoint_now()?;
        self.document = Some(document);
        self.epoch = epoch;
        self.needs_resync = false;
        self.installed = None;
        self.config = config;
        Ok(snapshot)
    }

    pub fn detach(&mut self, op: &mut Operation) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        ensure_session_mode(op)?;
        self.detach_inner(op)
    }

    fn detach_inner(&mut self, op: &mut Operation) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        op.checkpoint_now()?;
        let epoch = self.next_epoch(op)?;
        op.checkpoint_now()?;
        self.epoch = epoch;
        self.document = None;
        self.installed = None;
        self.needs_resync = true;
        Ok(())
    }

    pub fn set_config(&mut self, config: ParseConfig, op: &mut Operation) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        ensure_session_mode(op)?;
        self.set_config_inner(config, op)
    }

    fn set_config_inner(
        &mut self,
        config: ParseConfig,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.ensure_budget(&self.budget)?;
        op.checkpoint_now()?;
        if self.config != config {
            let epoch = self.next_epoch(op)?;
            op.checkpoint_now()?;
            self.config = config;
            self.epoch = epoch;
            self.installed = None;
        }
        Ok(())
    }

    /// Must be called while the host holds exclusive session access.
    pub fn capture(
        &mut self,
        cancel: CancellationToken,
        op: &mut Operation,
    ) -> Result<ParseRequest, CoreError> {
        op.ensure_budget(&self.budget)?;
        ensure_session_mode(op)?;
        self.capture_inner(cancel, op)
    }

    fn capture_inner(
        &mut self,
        cancel: CancellationToken,
        op: &mut Operation,
    ) -> Result<ParseRequest, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.checkpoint_now()?;
        if self.needs_resync {
            return Err(op.fail(CoreError::InvalidState));
        }
        let source = self
            .snapshot()
            .ok_or_else(|| op.fail(CoreError::InvalidState))?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let identity =
            ModelIdentity::new(source.key(), generation, self.config).map_err(|e| op.fail(e))?;
        let ticket = Ticket {
            identity,
            epoch: self.epoch,
            scope: self.scope.clone(),
        };
        op.checkpoint_now()?;
        self.generation = generation;
        Ok(ParseRequest {
            source,
            ticket,
            cancel,
        })
    }

    /// Consume exactly once; cancelled, superseded and ABA results cannot publish.
    pub fn install(&mut self, completed: CompletedParse) -> Result<AcceptedModel, CoreError> {
        let mut op = self.budget.begin(completed.cancel.clone());
        ensure_session_mode(&mut op)?;
        self.install_inner(completed, &mut op)
    }

    fn install_inner(
        &mut self,
        completed: CompletedParse,
        op: &mut Operation,
    ) -> Result<AcceptedModel, CoreError> {
        op.ensure_budget(&self.budget)?;
        op.checkpoint_now()?;
        if !op.uses_cancellation(&completed.cancel) {
            return Err(op.fail(CoreError::InvalidState));
        }
        self.validate_ticket(&completed.ticket)
            .map_err(|e| op.fail(e))?;
        op.ensure_budget(completed.model.source().budget())?;
        if completed.model.identity() != completed.ticket.identity {
            return Err(op.fail(CoreError::StaleResult));
        }
        op.checkpoint_now()?;
        self.installed = Some(completed.model.clone());
        Ok(completed.model)
    }

    fn next_epoch(&self, op: &mut Operation) -> Result<u64, CoreError> {
        self.epoch
            .checked_add(1)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))
    }

    fn validate_ticket(&self, ticket: &Ticket) -> Result<(), CoreError> {
        if !Arc::ptr_eq(&self.scope, &ticket.scope)
            || self.needs_resync
            || ticket.epoch != self.epoch
        {
            return Err(CoreError::StaleResult);
        }
        let current = self.snapshot().ok_or(CoreError::StaleResult)?.key();
        if ticket.identity.source() != current
            || ticket.identity.config() != self.config
            || ticket.identity.generation() != self.generation
        {
            return Err(CoreError::StaleResult);
        }
        Ok(())
    }
}

/// Borrowed tickets do not own an operation through queueing and installation.
/// They must not silently renew an elapsed request's deadline.
fn ensure_session_mode(op: &mut Operation) -> Result<(), CoreError> {
    op.checkpoint_now()?;
    if op.budget().elapsed_limit_ns().is_some() {
        return Err(op.fail(CoreError::UnsupportedConfiguration));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
