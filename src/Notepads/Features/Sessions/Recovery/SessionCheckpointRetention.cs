// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Content history is bounded; lifecycle decisions and failed/manual roots are never pruned here.</summary>
internal static class SessionCheckpointRetention
{
    // Caller holds admission and its writer lease. Delete failures leave the
    // old reference graph in place for the subsequent catalog scan.
    public static async Task PruneAsync(Guid owner, CancellationToken cancellation)
    {
        if (!SessionScopeLease.HasWriterLease(owner)) return;
        using var readers = SessionScopeLease.TryAcquireExclusiveReaders(owner);
        if (readers == null) return;
        var state = await SessionRecoveryCatalog.ReadScopeAsync(owner, cancellation);
        if (state.Blocked || state.SelectedCheckpoint == null ||
            state.Checkpoints.Any(read => read.State != RecoveryReadState.Valid) ||
            state.Rescues.Any(read => read.State != RecoveryReadState.Valid))
        {
            return;
        }

        var store = RecoveryRootStore.CreateStore();
        foreach (var obsolete in state.Checkpoints.OrderByDescending(read => read.Address.Ordinal).Skip(3))
            await store.DeleteAsync(obsolete.Address, cancellation);
        foreach (var group in state.Rescues.GroupBy(read => read.Record.Editor.Id))
        {
            foreach (var obsolete in group.OrderByDescending(read => read.Address.Ordinal).Skip(3))
                await store.DeleteAsync(obsolete.Address, cancellation);
        }
    }
}
