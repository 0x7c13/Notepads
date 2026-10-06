// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
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
    public static async Task CollectAsync(Guid ownerId, string futureAccessTokenPrefix,
        Func<CancellationToken, Task<SessionRecoveryReferences>> readReferencesAsync,
        CancellationToken cancellationToken = default,
        Func<SessionRecoveryReferences, CancellationToken, Task> collectLegacyBackupsAsync = null)
    {
        if (ownerId == Guid.Empty) throw new ArgumentException("Recovery GC requires an owned scope.", nameof(ownerId));
        if (string.IsNullOrEmpty(futureAccessTokenPrefix))
            throw new ArgumentException("Recovery GC requires a scoped permission prefix.", nameof(futureAccessTokenPrefix));
        if (readReferencesAsync == null) throw new ArgumentNullException(nameof(readReferencesAsync));
        using (await SessionRecoveryTransaction.EnterAsync(cancellationToken))
        {
            var references = await readReferencesAsync(cancellationToken);
            if (references == null) throw new InvalidOperationException("Recovery GC has no reference graph.");
            if (references.BlocksGlobalCollection || references.BlockedOwners.Contains(ownerId) ||
                !SessionScopeLease.HasWriterLease(ownerId))
            {
                return;
            }

            using var readers = SessionScopeLease.TryAcquireExclusiveReaders(ownerId);
            if (readers == null) return;
            if (collectLegacyBackupsAsync != null) await collectLegacyBackupsAsync(references, cancellationToken);
            await CollectOwnedResourcesAsync(ownerId, futureAccessTokenPrefix, references, cancellationToken);
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
                await CollectOwnedResourcesAsync(retired, SessionDocumentStore.GetFutureAccessTokenPrefix(retired), references, cancellationToken);
            }
        }
    }

    private static async Task CollectOwnedResourcesAsync(Guid ownerId, string futureAccessTokenPrefix,
        SessionRecoveryReferences references, CancellationToken cancellationToken)
    {

        // Referencing a foreign generation does not grant permission to delete
        // that owner's other files. Historical unknown owners remain intact.
        foreach (var folderName in new[] { "DocumentBaselines", "DocumentJournals" })
        {
            cancellationToken.ThrowIfCancellationRequested();
            var folder = await ApplicationData.Current.LocalFolder.TryGetItemAsync(folderName) as StorageFolder;
            if (folder == null) continue;
            var referenced = folderName == "DocumentBaselines" ? references.BaselineFileNames : references.JournalFileNames;
            var extension = folderName == "DocumentBaselines" ? ".utf8" : ".npj";
            foreach (var file in await folder.GetFilesAsync())
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
        var tokens = StorageApplicationPermissions.FutureAccessList.Entries
            .Where(entry => entry.Token.StartsWith(futureAccessTokenPrefix, StringComparison.Ordinal) &&
                !references.FutureAccessTokens.Contains(entry.Token)).Select(entry => entry.Token).ToArray();
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
}
