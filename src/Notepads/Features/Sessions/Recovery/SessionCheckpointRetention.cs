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
using Notepads.Features.Sessions.Validation;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Content history is bounded; lifecycle decisions and failed/manual roots are never pruned here.</summary>
internal static class SessionCheckpointRetention
{
    private const int RetainedCheckpoints = 3;
    private const int RetainedRescuesPerEditor = 3;

    // Caller holds admission and its writer lease. Delete failures leave the
    // old reference graph in place for the subsequent catalog scan.
    public static async Task PruneAsync(Guid owner, CancellationToken cancellation, RecoveryCatalogPass pass = null)
    {
        if (!SessionScopeLease.HasWriterLease(owner)) return;
        using var readers = SessionScopeLease.TryAcquireExclusiveReaders(owner);
        if (readers == null) return;
        var state = await PruneCheckpointsAsync(owner, cancellation, pass);
        if (state == null) return;

        var store = RecoveryRootStore.CreateStore();
        // A closed, moved, consumed or cleared editor's rescue fails the predicate that already
        // lets GC collect its assets. Closed decisions, Pending and adoption records stay.
        var own = state.Decisions.Select(read => read.Record).ToArray();
        var all = state.AllDecisions.ToArray();
        foreach (var group in state.Rescues.GroupBy(read => read.Record.Editor.Id))
        {
            var retired = group.OrderByDescending(read => read.Address.Ordinal).Where((read, index) => index >= RetainedRescuesPerEditor ||
                !SessionRecoveryAuthority.IsEligible(read.Record.Stamp, read.Record.Editor, state.Stamp, own, all)).ToArray();
            var deleted = 0;
            foreach (var obsolete in retired)
                if (await store.DeleteAsync(obsolete.Address, cancellation)) deleted++;
            // Later catalog passes would otherwise list an empty directory for every closed tab.
            if (deleted == group.Count()) store.TryDeleteEmptyArea(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, group.Key));
        }
    }

    // A closed or crashed window never prunes its own scope again. Caller holds admission plus that
    // retired scope's inactive writer lease and exclusive readers. Its rescues stay (deferred, D1).
    public static async Task PruneRetiredAsync(Guid owner, CancellationToken cancellation, RecoveryCatalogPass pass)
    {
        // A listing settles scopes an earlier pass already bounded without reading them again.
        var store = RecoveryRootStore.CreateStore();
        if (store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries.Count > RetainedCheckpoints)
            await PruneCheckpointsAsync(owner, cancellation, pass);
    }

    // Caller holds admission and the scope's writer and exclusive reader leases. Keeps the newest three
    // checkpoints; unless the scope's whole history verifies it deletes nothing and returns null.
    private static async Task<RecoveryScopeSnapshot> PruneCheckpointsAsync(Guid owner, CancellationToken cancellation,
        RecoveryCatalogPass pass)
    {
        var state = await SessionRecoveryCatalog.ReadScopeAsync(owner, cancellation, pass: pass);
        if (state.Blocked || state.SelectedCheckpoint == null ||
            state.Checkpoints.Any(read => read.State != RecoveryReadState.Valid) ||
            state.Rescues.Any(read => read.State != RecoveryReadState.Valid))
        {
            return null;
        }

        var store = RecoveryRootStore.CreateStore();
        foreach (var obsolete in state.Checkpoints.OrderByDescending(read => read.Address.Ordinal).Skip(RetainedCheckpoints))
            await store.DeleteAsync(obsolete.Address, cancellation);
        return state;
    }
}
