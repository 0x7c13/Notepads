// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace Notepads.Features.Sessions.Contracts;

internal enum RecoveryAreaKind { Checkpoints, Decisions, Rescue, Pending, Transfers, Archives }
internal enum RecoveryPublicationState { Committed, NotCommitted, Indeterminate }
internal enum RecoveryReadState { Valid, Absent, Corrupt, Unreadable, Conflict }
internal enum RecoveryRedundancy { None, Single, Mirrored, Repaired }

/// <summary>A validated logical directory beneath the configured Recovery root.</summary>
internal sealed record RecoveryArea(RecoveryAreaKind Kind, Guid ScopeId, Guid DocumentId = default, Guid RootId = default)
{
    public bool AllowsForeignOwners => Kind is RecoveryAreaKind.Transfers or RecoveryAreaKind.Archives;
}

internal sealed record RecoveryAddress(RecoveryArea Area, ulong Ordinal, Guid OperationId)
{
    public string Stem => $"{Ordinal:D20}-{OperationId:N}";
}

internal sealed record RecoveryReadResult(
    RecoveryReadState State,
    RecoveryAddress Address,
    RecoveryRecord Record = null,
    string PayloadSha256 = null,
    byte[] VerifiedEnvelopeBytes = null,
    RecoveryRedundancy Redundancy = RecoveryRedundancy.None,
    Exception Error = null);

internal sealed record RecoveryPublicationResult(
    RecoveryPublicationState State,
    RecoveryAddress Address,
    string PayloadSha256,
    RecoveryRedundancy Redundancy = RecoveryRedundancy.None,
    Exception Error = null)
{
    public bool IsCommitted => State == RecoveryPublicationState.Committed;
}

internal sealed record RecoveryIntentResult(
    RecoveryReadState State,
    RecoveryAddress Address,
    RecoveryRecord Record = null,
    string PayloadSha256 = null,
    Exception Error = null);

internal sealed record RecoveryDirectoryScan<T>(
    IReadOnlyList<T> Entries,
    IReadOnlyList<string> UnrecognizedPaths,
    Exception Error = null,
    ulong HighestOccupiedOrdinal = 0)
{
    public bool IsComplete => Error == null && UnrecognizedPaths.Count == 0;
}
