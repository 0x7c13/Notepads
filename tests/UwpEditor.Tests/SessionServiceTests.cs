// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Windows.Storage;
using WinUIEditor;

namespace NotepadsEditorTests;

internal static class SessionServiceTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        await CheckBoundedHistoryAndFallbackAsync();
        await CheckLegacyConsumptionAsync();
        await CheckAdoptionRetryAsync();
        await CheckMixedRescueAsync();
        await CheckInactiveAdoptionRetryAsync();
        await CheckNewerRescueRetentionAsync();
        await SessionPerformanceRegressionTests.RunAsync(log);
        log.AppendLine("PASS: immutable session checkpoints/mirrors, bounded history, failed newest Pending protection, rescue Save As, stale epoch rejection, and V1 partial adoption/close/reset.");
    }

    private static async Task CheckBoundedHistoryAndFallbackAsync()
    {
        var owner = Guid.NewGuid();
        var id = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var document = await SessionTestDocument.CreateAsync(owner, "saved العربية😀");
            document.Metadata.LanguageOverride = "csharp";
            for (var index = 0; index < 4; index++)
            {
                document.Editor.GotoPos(document.Editor.Length); document.Editor.PasteText("\r" + index);
                using var capture = document.Capture(id, service.CurrentStamp);
                var result = await service.SaveAsync(capture, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded, "A detached native capture did not commit.");
                await service.FinishPublicationAsync(result, CancellationToken.None);
            }
            var store = RecoveryRootStore.CreateStore();
            RecoveryScopeSnapshot snapshot;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(snapshot.Checkpoints.Count == 3, "Checkpoint history exceeded three successful publications.");
            }
            var newest = snapshot.SelectedCheckpoint.Address;
            using var staleCapture = document.Capture(id, service.CurrentStamp);
            File.WriteAllText(store.Paths.CopyPath(newest, "a"), "truncated");
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var repaired = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(repaired.Outcome == SessionRecoveryOutcome.MirrorRepaired &&
                    repaired.Session.TextEditors.Single().Id == id, "A verified peer did not repair its mirror.");
                // A rescue may commit before its workspace checkpoint fails.
                // Its higher revision must still be part of restart ordering.
                var laterRescue = SessionRecoveryCatalog.CloneEditor(snapshot.Session.TextEditors.Single());
                laterRescue.CaptureRevision += 10000;
                var area = store.GetScopeArea(owner, RecoveryAreaKind.Rescue, id);
                RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.Rescue,
                    Stamp = store.CreatePublicationStamp(area, service.CurrentStamp),
                    Editor = laterRescue
                }));
            }
            await service.DisposeAsync(); service = SessionTestProtocol.CreateService(owner);
            using (var ordering = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            using (var resumed = document.CaptureState())
            {
                SessionTestProtocol.Check(resumed.CaptureRevision > snapshot.Session.TextEditors.Single().CaptureRevision + 10000,
                    "Restart ordering omitted a newer rescue whose workspace checkpoint never committed.");
            }
            // Metadata remains valid while only the newer NPJ tail is missing.
            // An earlier prefix can still recover the same active incarnation.
            await document.Editor.StopJournalAsync();
            var earlier = snapshot.Checkpoints.OrderByDescending(read => read.Address.Ordinal).Skip(1).First().Record.Session.TextEditors.Single();
            using (var journal = new FileStream(document.Journal.File.Path, FileMode.Open, FileAccess.Write, FileShare.Read))
            { journal.SetLength(earlier.Journal.CommittedByteLength); journal.Flush(true); }
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using (var batch = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Count == 1 && service.RecoveryOutcome == SessionRecoveryOutcome.Partial,
                    "A bad latest document did not recover an earlier descriptor independently.");
                SessionTestProtocol.Check(batch.Documents[0].Checkpoint.CommittedSequence == earlier.Journal.CommittedSequence,
                    "Fallback recovered an uncommitted or newer journal prefix.");
                SessionTestProtocol.Check(batch.Documents[0].Metadata.LanguageOverride == "csharp",
                    "Checkpoint fallback lost the manual document language.");
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var roots = await SessionRecoveryCatalog.ReadReferencesAsync("BackupFiles");
                    var pending = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Pending, id));
                    SessionTestProtocol.Check(pending.Entries.Count != 0 && roots.JournalFileNames.Contains(document.Journal.File.Name),
                        "Failed latest content was not durably retained as Pending.");
                }
            }
            await service.DisposeAsync();
            foreach (var checkpoint in snapshot.Checkpoints)
                foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(checkpoint.Address, peer), "bad");
            service = SessionTestProtocol.CreateService(owner);
            using (var batch = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Count == 1 && batch.Documents[0].Id != id &&
                    batch.Documents[0].Metadata.RequiresSaveAs && batch.Documents[0].EditingFile == null,
                    "All-index salvage reused an original file association or failed to recover an older rescue prefix.");
                SessionTestProtocol.Check(batch.Documents[0].Metadata.LanguageOverride == "csharp",
                    "Rescue salvage lost the manual document language.");
            }
            await service.ClearSessionDataAsync();
            SessionTestProtocol.Check(!(await service.SaveAsync(staleCapture, CancellationToken.None)).Succeeded,
                "A capture silently rebound to the epoch created during its await.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckLegacyConsumptionAsync()
    {
        var owner = Guid.NewGuid();
        var good = Guid.NewGuid(); var missing = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var folder = await SessionManifestStore.GetBackupFolderAsync(SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles");
        var file = await folder.CreateFileAsync("legacy.txt", CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(file, "legacy العربية😀");
        var metadata = new DocumentMetadata { LastSavedEncoding = "UTF-8", LastSavedLineEnding = "CRLF", FontZoomFactor = 1 };
        var legacy = new NotepadsSessionDataV1
        {
            SelectedTextEditor = good,
            TextEditors = new List<TextEditorSessionDataV1>
            {
                new() { Id = good, LastSavedBackupFilePath = file.Path, StateMetaData = metadata },
                new() { Id = missing, LastSavedBackupFilePath = Path.Combine(folder.Path, "missing"), StateMetaData = metadata }
            }
        };
        var manifest = SessionScopeData.GetManifestFileName(owner);
        await SessionTestProtocol.WriteLegacySourceAsync(owner, legacy);
        try
        {
            using (var batch = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Count == 1 && service.UnrecoveredEditorCount == 2,
                    "A failed V1 row blocked the valid row or preparation consumed rows prematurely.");
                await using var restored = await SessionTestDocument.CreateAsync(owner, await batch.Documents[0].SavedSnapshot.ReadTextAsync());
                using var capture = restored.Capture(good, batch.ExpectedStamp);
                await service.CommitAttachedAsync(capture, new[] { good }, CancellationToken.None);
                SessionTestProtocol.Check(service.UnrecoveredEditorCount == 1, "Partial V1 adoption consumed the failed row.");
            }
            SessionTestProtocol.Check(await service.PrepareExplicitCloseAsync(good), "The adopted tab close could not publish.");
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using (var afterClose = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(afterClose.Documents.Count == 0 && service.UnrecoveredEditorCount == 1,
                    "Close revoked successful V1 consumption and reimported discarded content.");
            }

            await service.ClearSessionDataAsync();
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using (var afterReset = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(afterReset.Documents.Count == 0 && service.UnrecoveredEditorCount == 0,
                    "Reset did not suppress the complete accepted V1 source, including its failed row.");
            }

            SessionTestProtocol.Check(await ApplicationData.Current.LocalFolder.TryGetItemAsync(manifest) != null,
                "Reset deleted retained V1 migration artifacts instead of making them manual-only.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckAdoptionRetryAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new CheckpointFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = SessionTestProtocol.CreateService(owner, store);
        var folder = await SessionManifestStore.GetBackupFolderAsync(SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles");
        var file = await folder.CreateFileAsync("legacy.txt", CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(file, "retry source");
        var legacy = new NotepadsSessionDataV1();
        legacy.TextEditors.Add(new TextEditorSessionDataV1
        {
            Id = id,
            LastSavedBackupFilePath = file.Path,
            StateMetaData = new DocumentMetadata { LastSavedEncoding = "UTF-8", LastSavedLineEnding = "CRLF", FontZoomFactor = 1 }
        });
        await SessionTestProtocol.WriteLegacySourceAsync(owner, legacy);
        try
        {
            using var batch = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None);
            await using var target = await SessionTestDocument.CreateAsync(owner, "retry source");
            using var capture = target.Capture(id, batch.ExpectedStamp);
            faults.Enabled = true;
            try
            {
                await service.CommitAttachedAsync(capture, new[] { id }, CancellationToken.None);
                throw new InvalidOperationException("The injected checkpoint publication failure was ignored.");
            }
            catch (IOException) { }
            faults.Enabled = false;
            await service.CommitAttachedAsync(capture, new[] { id }, CancellationToken.None);
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var authority = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(authority.Decisions.Count(read => read.Record.Kind == RecoveryRecordKind.AdoptLegacy) == 1 &&
                    authority.Session.TextEditors.Single().Id == id,
                    "A committed adoption followed by failed checkpoint could not retry idempotently.");
            }
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private sealed class CheckpointFailure : IRecoveryStorageFaults
    {
        public bool Enabled { get; set; }
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (Enabled && address.Area.Kind == RecoveryAreaKind.Checkpoints && step == RecoveryStorageStep.BeforeRename)
                throw new IOException("Injected checkpoint rename failure after durable adoption.");
        }
    }

    private static async Task CheckMixedRescueAsync()
    {
        var owner = Guid.NewGuid(); var adoptedId = Guid.NewGuid(); var ordinaryId = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var folder = await SessionManifestStore.GetBackupFolderAsync(SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles");
        var file = await folder.CreateFileAsync("legacy.txt", CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(file, "adopted text");
        var legacy = new NotepadsSessionDataV1();
        legacy.TextEditors.Add(new TextEditorSessionDataV1
        {
            Id = adoptedId,
            LastSavedBackupFilePath = file.Path,
            StateMetaData = new DocumentMetadata { LastSavedEncoding = "UTF-8", LastSavedLineEnding = "CRLF", FontZoomFactor = 1 }
        });
        await SessionTestProtocol.WriteLegacySourceAsync(owner, legacy);
        try
        {
            using (var prepared = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                await using var adopted = await SessionTestDocument.CreateAsync(owner, "adopted text");
                using (var initial = adopted.Capture(adoptedId, prepared.ExpectedStamp))
                    await service.CommitAttachedAsync(initial, new[] { adoptedId }, CancellationToken.None);
                await using var ordinary = await SessionTestDocument.CreateAsync(owner, "ordinary text");
                using var capture = adopted.Capture(adoptedId, service.CurrentStamp);
                capture.Documents.Add(new SessionDocumentCapture { Id = ordinaryId, Recovery = ordinary.CaptureState() });
                SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                    "Mixed adopted/ordinary checkpoint publication failed.");
            }
            RecoveryScopeSnapshot snapshot;
            using (await SessionRecoveryTransaction.EnterAsync()) snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
            var store = RecoveryRootStore.CreateStore();
            foreach (var checkpoint in snapshot.Checkpoints)
                foreach (var copy in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(checkpoint.Address, copy), "bad");
            await service.DisposeAsync(); service = SessionTestProtocol.CreateService(owner);
            using var recovered = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None);
            SessionTestProtocol.Check(recovered.Documents.Count == 2 && recovered.Documents.Any(document => document.Id == adoptedId) &&
                recovered.Documents.Any(document => document.Id != adoptedId && document.Metadata.RequiresSaveAs) &&
                service.RecoveryOutcome == SessionRecoveryOutcome.Rescue,
                "Durable adoption suppressed salvage of another ordinary tab when every checkpoint was lost.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckInactiveAdoptionRetryAsync()
    {
        var sourceOwner = Guid.NewGuid(); var targetOwner = Guid.NewGuid(); var id = Guid.NewGuid();
        var sourceService = SessionTestProtocol.CreateService(sourceOwner);
        var faults = new CheckpointFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var targetService = SessionTestProtocol.CreateService(targetOwner, store, primary: true);
        try
        {
            await sourceService.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var source = await SessionTestDocument.CreateAsync(sourceOwner, "inactive retry source");
            source.Editor.GotoPos(source.Editor.Length); source.Editor.PasteText("\rcommitted edit");
            long goodPrefixLength;
            ulong goodSequence;
            using (var sourceCapture = source.Capture(id, sourceService.CurrentStamp))
            {
                SessionTestProtocol.Check((await sourceService.SaveAsync(sourceCapture, CancellationToken.None)).Succeeded,
                    "The inactive source checkpoint did not commit.");
                goodPrefixLength = checked((long)sourceCapture.Documents[0].Recovery.Checkpoint.CommittedByteLength);
                goodSequence = sourceCapture.Documents[0].Recovery.Checkpoint.CommittedSequence;
            }
            source.Editor.GotoPos(source.Editor.Length); source.Editor.PasteText("\rmissing newer tail");
            using (var latest = source.Capture(id, sourceService.CurrentStamp))
            {
                SessionTestProtocol.Check((await sourceService.SaveAsync(latest, CancellationToken.None)).Succeeded,
                    "The inactive latest checkpoint did not commit.");
            }

            await source.Editor.StopJournalAsync();
            using (var stream = new FileStream(source.Journal.File.Path, FileMode.Open, FileAccess.Write, FileShare.Read))
            { stream.SetLength(goodPrefixLength); stream.Flush(true); }
            await sourceService.DisposeAsync();
            using var batch = await targetService.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None);
            var prepared = batch.Documents.Single(document => document.Id == id);
            SessionTestProtocol.Check(prepared.Checkpoint.CommittedSequence == goodSequence &&
                targetService.RecoveryOutcome == SessionRecoveryOutcome.Partial &&
                store.Enumerate(store.GetScopeArea(sourceOwner, RecoveryAreaKind.Pending, id)).Entries.Count != 0,
                "An inactive scope's bad latest document skipped earlier good content or failed to preserve Pending.");
            await using var target = await SessionTestDocument.FromPreparedAsync(targetOwner, prepared);
            using var capture = target.Capture(id, batch.ExpectedStamp);
            faults.Enabled = true;
            try
            {
                await targetService.CommitAttachedAsync(capture, new[] { id }, CancellationToken.None);
                throw new InvalidOperationException("The injected checkpoint failure after inactive adoption was ignored.");
            }
            catch (IOException) { }
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                SessionTestProtocol.Check((await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner)).Session.TextEditors.Count == 0,
                    "The committed inactive adoption did not consume its source before the later checkpoint failed.");
            }

            faults.Enabled = false;
            using var retry = target.Capture(id, batch.ExpectedStamp);
            await targetService.CommitAttachedAsync(retry, new[] { id }, CancellationToken.None);
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var targetRoots = await SessionRecoveryCatalog.ReadScopeAsync(targetOwner);
                SessionTestProtocol.Check(targetRoots.Decisions.Count(read => read.Record.Kind == RecoveryRecordKind.AdoptInactive) == 1 &&
                    targetRoots.Session.TextEditors.Any(editor => editor.Id == id),
                    "Already consumed inactive source prevented checkpoint retry or was consumed twice.");
            }
        }
        finally
        {
            await sourceService.DisposeAsync(); await targetService.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(sourceOwner, targetOwner);
        }
    }

    private static async Task CheckNewerRescueRetentionAsync()
    {
        var sourceOwner = Guid.NewGuid(); var targetOwner = Guid.NewGuid(); var id = Guid.NewGuid();
        var sourceService = SessionTestProtocol.CreateService(sourceOwner);
        var targetService = SessionTestProtocol.CreateService(targetOwner, primary: true);
        try
        {
            await sourceService.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var source = await SessionTestDocument.CreateAsync(sourceOwner, "selected workspace content");
            source.Editor.GotoPos(source.Editor.Length); source.Editor.PasteText("\rcheckpoint edit");
            ulong selectedSequence;
            using (var selected = source.Capture(id, sourceService.CurrentStamp))
            {
                SessionTestProtocol.Check((await sourceService.SaveAsync(selected, CancellationToken.None)).Succeeded,
                    "The selected source workspace did not commit.");
                selectedSequence = selected.Documents[0].Recovery.Checkpoint.CommittedSequence;
            }
            source.Editor.GotoPos(source.Editor.Length); source.Editor.PasteText("\rnewer independently published rescue");
            var store = RecoveryRootStore.CreateStore();
            TextEditorSessionDataV2 newerDescriptor;
            using (var newer = source.CaptureState())
            {
                await newer.Checkpoint.FlushAsync();
                newerDescriptor = SessionDocumentStore.CaptureEditor(id, newer, null, null);
                newerDescriptor.CaptureRevision += 10000;
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var area = store.GetScopeArea(sourceOwner, RecoveryAreaKind.Rescue, id);
                    RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
                    {
                        Kind = RecoveryRecordKind.Rescue,
                        Stamp = store.CreatePublicationStamp(area, sourceService.CurrentStamp),
                        Editor = newerDescriptor
                    }));
                }
                newer.PreserveForRecovery();
            }
            await source.Editor.StopJournalAsync();
            await sourceService.DisposeAsync();
            using var batch = await targetService.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None);
            var prepared = batch.Documents.Single(document => document.Id == id);
            SessionTestProtocol.Check(prepared.Checkpoint.CommittedSequence == selectedSequence,
                "An incomplete workspace publication silently changed the selected recovery content.");
            await using var target = await SessionTestDocument.FromPreparedAsync(targetOwner, prepared);
            using var capture = target.Capture(id, batch.ExpectedStamp);
            SessionTestProtocol.Check(capture.Documents[0].Recovery.CaptureRevision > newerDescriptor.CaptureRevision,
                "The importing capture floor omitted higher inactive source rescue evidence.");
            await targetService.CommitAttachedAsync(capture, new[] { id }, CancellationToken.None);
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner);
                var adoption = (await SessionRecoveryCatalog.ReadScopeAsync(targetOwner)).Decisions.Single(
                    read => read.Record.Kind == RecoveryRecordKind.AdoptInactive).Record;
                SessionTestProtocol.Check(adoption.Consumptions.Single().SourceCaptureRevision == newerDescriptor.CaptureRevision &&
                    snapshot.Pending.Any(read => read.State == RecoveryReadState.Valid &&
                        read.Record.Editor.ComputeSha256() == newerDescriptor.ComputeSha256()),
                    "The highest observed source revision was consumed without retaining its unselected newer content.");
                foreach (var checkpoint in snapshot.Checkpoints)
                    foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(checkpoint.Address, peer), "bad");
                snapshot = await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner);
                var references = await SessionRecoveryCatalog.ReadReferencesAsync("BackupFiles");
                SessionTestProtocol.Check(snapshot.Session.TextEditors.Count == 0 && snapshot.EligibleRescues.Count == 0 &&
                    references.JournalFileNames.Contains(newerDescriptor.Journal.FileName),
                    "A consumed higher source rescue was reimported or its manual recovery assets became collectible.");
            }
            batch.Dispose();
            await targetService.DisposeAsync(); targetService = SessionTestProtocol.CreateService(targetOwner, primary: true);
            using var restarted = await targetService.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None);
            SessionTestProtocol.Check(restarted.Documents.Any(document => document.Id == id) &&
                targetService.RecoveryOutcome == SessionRecoveryOutcome.Partial,
                "A consumed source's damaged content aborted healthy target startup or lost its degraded notification.");
        }
        finally
        {
            await sourceService.DisposeAsync(); await targetService.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(sourceOwner, targetOwner);
        }
    }
}

internal static class SessionTestProtocol
{
    public static SessionService CreateService(Guid owner, RecoveryRecordStore store = null, bool primary = false) => new(
        new SessionScopeData { OwnerId = owner, InstanceId = owner, Kind = primary ? SessionScopeData.Primary : SessionScopeData.Secondary },
        SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles", SessionScopeData.GetManifestFileName(owner),
        () => [], store)
    { IsBackupEnabled = true };
    public static void Check(bool condition, string message)
    { if (!condition) throw new InvalidOperationException(message); }

    public static SessionCapture CaptureDocuments(IReadOnlyList<SessionTestDocument> documents,
        IReadOnlyList<Guid> identities, RecoveryStamp stamp)
    {
        var result = new SessionCapture { ExpectedStamp = stamp, SelectedEditorId = identities[0] };
        try
        {
            for (var index = 0; index < documents.Count; index++)
            {
                using var single = documents[index].Capture(identities[index], stamp);
                result.Documents.Add(single.Documents[0]);
                single.Documents.Clear();
            }
            return result;
        }
        catch { result.Dispose(); throw; }
    }

    public static async Task WriteLegacySourceAsync(Guid owner, NotepadsSessionDataV1 session)
    {
        var manifest = await ApplicationData.Current.LocalFolder.CreateFileAsync(
            SessionScopeData.GetManifestFileName(owner), CreationCollisionOption.ReplaceExisting);
        await FileIO.WriteTextAsync(manifest, JsonSerializer.Serialize(session, SessionJsonContext.Default.NotepadsSessionDataV1));
    }

    public static async Task CleanupAsync(params Guid[] owners)
    {
        using (await SessionRecoveryTransaction.EnterAsync())
        {
            var store = RecoveryRootStore.CreateStore();
            foreach (var kind in new[] { RecoveryAreaKind.Transfers, RecoveryAreaKind.Archives })
            {
                foreach (var area in store.EnumerateGlobalAreas(kind).Entries)
                {
                    foreach (var address in store.Enumerate(area).Entries)
                    {
                        var read = await store.ReadAsync(address);
                        if (read.State == RecoveryReadState.Valid && owners.Contains(read.Record.Stamp.ScopeId))
                            await store.DeleteAsync(address);
                    }
                }
            }

            foreach (var owner in owners)
            {
                if (owner == Guid.Empty || SessionScopeData.IsReservedOwner(owner))
                    throw new InvalidOperationException("Test cleanup requires its explicitly generated private scope.");
                var scopePath = store.Paths.ScopePath(owner);
                if (Directory.Exists(scopePath)) Directory.Delete(scopePath, true);
            }
        }
        foreach (var owner in owners)
        {
            var manifest = await ApplicationData.Current.LocalFolder.TryGetItemAsync(SessionScopeData.GetManifestFileName(owner));
            if (manifest != null) await manifest.DeleteAsync(StorageDeleteOption.PermanentDelete);
            var backup = await ApplicationData.Current.LocalFolder.TryGetItemAsync(SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles") as StorageFolder;
            if (backup != null) await backup.DeleteAsync(StorageDeleteOption.PermanentDelete);
            foreach (var folderName in new[] { "DocumentBaselines", "DocumentJournals" })
            {
                var assets = await ApplicationData.Current.LocalFolder.TryGetItemAsync(folderName) as StorageFolder;
                if (assets == null) continue;
                foreach (var file in await assets.GetFilesAsync())
                {
                    if (file.Name.StartsWith(owner.ToString("N") + "-", StringComparison.Ordinal))
                        await DocumentAssetLease.TryDeleteUnusedAsync(file);
                }
            }
        }
    }
}

internal sealed class SessionTestDocument : IAsyncDisposable
{
    private readonly EditorBaseControl _fixture = new();
    public Editor Editor => _fixture.Editor;
    public DocumentBaseline Baseline { get; private set; }
    public DocumentSnapshot Saved { get; private set; }
    public DocumentJournal Journal { get; private set; }
    public DocumentMetadata Metadata { get; set; } = new()
    { LastSavedEncoding = "UTF-8", LastSavedLineEnding = "CRLF", FontZoomFactor = 1, RequiresSaveAs = true };

    public static async Task<SessionTestDocument> CreateAsync(Guid owner, string text)
    {
        using var baseline = await DocumentBaseline.FromTextAsync(text, ownerId: owner);
        return await CreateAsync(owner, baseline);
    }

    public static async Task<SessionTestDocument> CreateAsync(Guid owner, DocumentBaseline baseline)
    {
        var result = new SessionTestDocument();
        try
        {
            result.Baseline = baseline.Retain();
            result.Saved = new DocumentSnapshot(result.Baseline, new UTF8Encoding(false), LineEnding.Crlf);
            result.Journal = await DocumentJournal.CreateAsync(owner);
            using var input = await result.Baseline.OpenReadStreamAsync();
            using var adapter = input.AsInputStream();
            await result.Editor.LoadUtf8Async(adapter, (ulong)result.Baseline.ByteLength, false);
            result.Editor.StartJournal(result.Journal.File.Path, 0);
            return result;
        }
        catch { await result.DisposeAsync(); throw; }
    }

    public DocumentRecoveryState CaptureState()
    {
        using var checkpoint = Editor.AcquireJournalCheckpoint();
        return new DocumentRecoveryState(Saved, Baseline, Journal, checkpoint, Metadata,
            true, Editor.Length);
    }
    public static async Task<SessionTestDocument> FromPreparedAsync(Guid owner,
        Notepads.Features.Sessions.Recovery.PreparedRecoveryDocument prepared)
    {
        var result = new SessionTestDocument();
        try
        {
            result.Baseline = prepared.RecoveryBaseline.Retain();
            result.Saved = prepared.SavedSnapshot.Retain();
            result.Metadata = prepared.Metadata;
            result.Journal = await DocumentJournal.CreateAsync(owner);
            using var input = await result.Baseline.OpenReadStreamAsync();
            using var adapter = input.AsInputStream();
            await result.Editor.RestoreUtf8Async(adapter, (ulong)result.Baseline.ByteLength, prepared.Checkpoint);
            await result.Editor.StartJournalFromCheckpointAsync(result.Journal.File.Path, prepared.Checkpoint);
            return result;
        }
        catch { await result.DisposeAsync(); throw; }
    }
    public SessionCapture Capture(Guid id, RecoveryStamp stamp)
    {
        var result = new SessionCapture { ExpectedStamp = stamp, SelectedEditorId = id, TabScrollOffset = 17 };
        result.Documents.Add(new SessionDocumentCapture { Id = id, Recovery = CaptureState() });
        return result;
    }
    public async ValueTask DisposeAsync()
    {
        await Editor.StopJournalAsync();
        Saved?.Dispose(); Baseline?.Dispose(); Journal?.Dispose();
        GC.KeepAlive(_fixture);
    }
}
