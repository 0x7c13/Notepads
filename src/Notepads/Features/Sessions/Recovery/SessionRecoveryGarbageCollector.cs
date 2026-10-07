// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;
using Windows.Storage.AccessCache;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Reclaim only this writer's resources after its complete durable reference graph is known.</summary>
internal static class SessionRecoveryGarbageCollector
{
    // Caller holds admission from the reference scan through every delete.
    public static async Task CollectAsync(Guid ownerId, string futureAccessTokenPrefix,
        Func<CancellationToken, Task<SessionRecoveryReferences>> readReferencesAsync,
        CancellationToken cancellationToken = default, RecoveryCatalogPass pass = null)
    {
        if (ownerId == Guid.Empty) throw new ArgumentException("Recovery GC requires an owned scope.", nameof(ownerId));
        if (string.IsNullOrEmpty(futureAccessTokenPrefix))
            throw new ArgumentException("Recovery GC requires a scoped permission prefix.", nameof(futureAccessTokenPrefix));
        if (readReferencesAsync == null) throw new ArgumentNullException(nameof(readReferencesAsync));
        var references = await readReferencesAsync(cancellationToken);
        if (references == null) throw new InvalidOperationException("Recovery GC has no reference graph.");
        if (references.BlocksGlobalCollection || references.BlockedOwners.Contains(ownerId) ||
            !SessionScopeLease.HasWriterLease(ownerId))
        {
            return;
        }

        using var readers = SessionScopeLease.TryAcquireExclusiveReaders(ownerId);
        if (readers == null) return;
        // One listing serves this owner and every retired scope; anything added after it waits for a later pass.
        var assets = await ListAssetsAsync();
        await CollectOwnedResourcesAsync(ownerId, futureAccessTokenPrefix, references, assets, cancellationToken);
        // Foreign lifetime is proved by OS leases, never by process
        // enumeration or a descriptor that happens to name another owner.
        var scopes = RecoveryRootStore.CreateStore().EnumerateScopeIds();
        if (!scopes.IsComplete) return;
        foreach (var retired in scopes.Entries.Where(scope => scope != ownerId && !SessionScopeData.IsReservedOwner(scope)))
        {
            if (references.BlockedOwners.Contains(retired)) continue;
            using var writer = SessionScopeLease.TryAcquireInactiveWriter(retired);
            if (writer == null) continue;
            using var foreignReaders = SessionScopeLease.TryAcquireExclusiveReaders(retired);
            if (foreignReaders == null) continue;
            // The reference graph predates this prune, so assets that only the pruned checkpoints
            // referenced stay protected until the next pass reads the graph again.
            await SessionCheckpointRetention.PruneRetiredAsync(retired, cancellationToken, pass);
            await CollectOwnedResourcesAsync(retired, SessionDocumentStore.GetFutureAccessTokenPrefix(retired), references, assets, cancellationToken);
        }
    }

    private static async Task<AssetListing> ListAssetsAsync()
    {
        static async Task<IReadOnlyList<StorageFile>> ListFilesAsync(string folderName) =>
            await ApplicationData.Current.LocalFolder.TryGetItemAsync(folderName) is StorageFolder folder ?
                await folder.GetFilesAsync() : Array.Empty<StorageFile>();
        return new(await ListFilesAsync("DocumentBaselines"), await ListFilesAsync("DocumentJournals"),
            StorageApplicationPermissions.FutureAccessList.Entries.Select(entry => entry.Token).ToArray());
    }

    private static async Task CollectOwnedResourcesAsync(Guid ownerId, string futureAccessTokenPrefix,
        SessionRecoveryReferences references, AssetListing assets, CancellationToken cancellationToken)
    {
        // Referencing a foreign generation does not grant permission to delete
        // that owner's other files. Historical unknown owners remain intact.
        foreach (var (files, referenced, extension) in new[]
        {
            (assets.Baselines, references.BaselineFileNames, ".utf8"),
            (assets.Journals, references.JournalFileNames, ".npj")
        })
        {
            cancellationToken.ThrowIfCancellationRequested();
            foreach (var file in files)
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (referenced.Contains(file.Name) || !file.Name.EndsWith(extension, StringComparison.OrdinalIgnoreCase)) continue;
                var identity = file.Name.Substring(0, file.Name.Length - extension.Length).Split('-');
                if (identity.Length != 2 || !Guid.TryParseExact(identity[0], "N", out var assetOwner) ||
                    assetOwner != ownerId || !Guid.TryParseExact(identity[1], "N", out _))
                {
                    continue;
                }
                // The shared lease registry also protects unpublished candidates
                // and readers against acquisition racing asynchronous deletion.
                await DocumentAssetLease.TryDeleteUnusedAsync(file);
            }
        }

        cancellationToken.ThrowIfCancellationRequested();
        var tokens = assets.PermissionTokens.Where(token => token.StartsWith(futureAccessTokenPrefix, StringComparison.Ordinal) &&
            !references.FutureAccessTokens.Contains(token)).ToArray();
        foreach (var token in tokens)
        {
            cancellationToken.ThrowIfCancellationRequested();
            try { StorageApplicationPermissions.FutureAccessList.Remove(token); }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(SessionRecoveryGarbageCollector)}] Failed to delete orphaned permission: {ex}");
            }
        }
        // Unscoped V1 tokens need a global manifest graph to establish sole
        // ownership. Loading such a token never makes it safe to reclaim here.
    }

    private sealed record AssetListing(IReadOnlyList<StorageFile> Baselines, IReadOnlyList<StorageFile> Journals,
        IReadOnlyList<string> PermissionTokens);
}
