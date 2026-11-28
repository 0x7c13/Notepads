// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in this vendored crate for upstream license information.
// ---------------------------------------------------------------------------------------------

//! Borrowed, block-scoped cooperative execution control. No TLS or platform callbacks.
//! Opaque regex calls, lazy context decoding, OnceCell waits and allocation cannot be preempted.

use std::error::Error;
use std::fmt;

/// Observation points; units count parser work events, not regex VM instructions or bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Line,
    Token,
    ContextVisit,
    IncludeStep,
    PatternVisit,
    Backreference,
    RegexCompile,
    RegexSearch,
    CacheHit,
    CacheWrite,
    Capture,
    ScopeOperation,
    ContextChange,
}

/// The owner supplies cancellation, an actual clock-based deadline check, and checked accounting.
/// A zero-unit observation must still check cancellation and the deadline.
pub trait ExecutionMonitor {
    fn checkpoint(&mut self, stage: Stage, units: u32) -> Result<(), StopReason>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
    WorkLimitExceeded,
    ResourceLimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegexPhase {
    Compile,
    Search,
}

/// Compact mirror of the first observed terminal event, without retaining backend allocations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Termination {
    Stopped(StopReason),
    Regex(RegexPhase),
    Parsing,
}

#[derive(Debug)]
pub enum ExecutionError {
    Stopped(StopReason),
    Regex {
        phase: RegexPhase,
        source: Box<dyn Error + Send + Sync + 'static>,
    },
    AlreadyTerminated(Termination),
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped(reason) => write!(f, "execution stopped: {reason:?}"),
            Self::Regex { phase, source } => write!(f, "regex {phase:?} failed: {source}"),
            Self::AlreadyTerminated(reason) => {
                write!(f, "execution already terminated: {reason:?}")
            }
        }
    }
}

impl Error for ExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Regex { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Borrow for one block, across all its lines. After any error discard the parser and all tokens.
/// The monitor retains budget/first-fault authority; this object only mirrors terminal state.
pub struct ExecutionControl<'a> {
    monitor: Option<&'a mut dyn ExecutionMonitor>,
    terminal: Option<Termination>,
}

impl<'a> ExecutionControl<'a> {
    pub fn new(monitor: &'a mut dyn ExecutionMonitor) -> Self {
        Self {
            monitor: Some(monitor),
            terminal: None,
        }
    }

    pub(crate) fn uncontrolled() -> Self {
        Self {
            monitor: None,
            terminal: None,
        }
    }

    pub fn termination(&self) -> Option<Termination> {
        self.terminal
    }

    pub(crate) fn is_controlled(&self) -> bool {
        self.monitor.is_some()
    }

    pub fn checkpoint(&mut self, stage: Stage, units: u32) -> Result<(), ExecutionError> {
        if let Some(reason) = self.terminal {
            return Err(ExecutionError::AlreadyTerminated(reason));
        }
        if let Some(monitor) = self.monitor.as_mut() {
            if let Err(reason) = monitor.checkpoint(stage, units) {
                self.terminal = Some(Termination::Stopped(reason));
                return Err(ExecutionError::Stopped(reason));
            }
        }
        Ok(())
    }

    pub(crate) fn regex_error(
        &mut self,
        phase: RegexPhase,
        source: Box<dyn Error + Send + Sync + 'static>,
    ) -> ExecutionError {
        if let Some(reason) = self.terminal {
            return ExecutionError::AlreadyTerminated(reason);
        }
        self.terminal = Some(Termination::Regex(phase));
        ExecutionError::Regex { phase, source }
    }

    pub(crate) fn parsing_error(&mut self) {
        if self.terminal.is_none() {
            self.terminal = Some(Termination::Parsing);
        }
    }
}
