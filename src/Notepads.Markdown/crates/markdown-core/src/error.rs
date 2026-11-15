// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Typed errors used by the Markdown engine.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    SourceBytes,
    SourceCapacity,
    SourceOwners,
    ParseBytes,
    InputPieces,
    Edits,
    InsertBytes,
    AuxiliaryCapacity,
    Work,
    NativeRecords,
    ModelRecords,
    SourcePieces,
    ProofRecords,
    Depth,
    DecodedBytes,
    ReferenceLookups,
    ReferenceBytes,
    IndexRecords,
    QueryRequests,
    QueryRecords,
    QueryCapacity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreError {
    InvalidState,
    StaleResult,
    InvalidRevision,
    WrongDocument,
    WrongRevision {
        expected: u64,
        actual: u64,
    },
    ResyncRequired {
        expected: u64,
        actual: u64,
    },
    WrongBudget,
    WrongGeneration {
        expected: u64,
        actual: u64,
    },
    UnknownId,
    OutsideScope,
    InvalidRange,
    UnsupportedConfiguration,
    InvalidProvenance,
    InvalidBoundary,
    InvalidEdits,
    /// Byte offset in the initial UTF8 input, not normalized P.
    InvalidUtf8 {
        offset: u64,
    },
    /// Index in the original u16 input, not a UTF8 source byte offset.
    InvalidUtf16 {
        offset: u64,
    },
    /// Original batch index and byte offset local to that replacement.
    InvalidEditUtf8 {
        edit_index: u64,
        offset: u64,
    },
    ArithmeticOverflow,
    ResourceLimit {
        kind: LimitKind,
        requested: u64,
        limit: u64,
    },
    OutOfMemory,
    Cancelled,
    /// First observed monotonic elapsed deadline, in nanoseconds.
    DeadlineExceeded {
        elapsed_ns: u64,
        limit_ns: u64,
    },
    /// A monotonic clock moved backwards or could not represent its interval.
    ClockFailure,
    InternalInvariant,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CoreError {}
