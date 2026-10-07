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
        await CheckDeletedEditingFileAsync();
        await CheckCleanFileRowAsync();
        await CheckEmptyLegacyRowAsync();
        await CheckInterruptedResetAsync();
        await CheckPartialCloseBatchAsync();
        await CheckRescueRetirementAsync();
        await CheckSnapshotOffMaintenanceAsync();
        await CheckMaintenancePassAsync();
        await CheckRetiredScopePruningAsync();
        await CheckLiveRecoveryBlockAsync();
        await SessionPerformanceRegressionTests.RunAsync(log);
        log.AppendLine("PASS: immutable session checkpoints/mirrors, bounded history, failed newest Pending protection, rescue Save As, stale epoch rejection, V1 partial adoption/close/reset, deleted editing files, clean file reload, empty V1 rows, interrupted reset, partial close batches, retirement of closed or cleared rescue history, snapshot-off orphan collection that spares live editors and other windows, one maintenance pass that both prunes and collects, another window's pruning of a retired scope's checkpoints that keeps its rescues and referenced assets, and a live blocked-recovery signal that saves and closes set and the next clean save clears.");
    }

    private static async Task CheckLiveRecoveryBlockAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var document = await SessionTestDocument.CreateAsync(owner, "blocked later");
            // An unrecognized root that appears after startup, which startup's RecoveryOutcome never sees.
            var stray = Path.Combine(RecoveryRootStore.CreateStore().Paths.ScopePath(owner), "Unrecognized");
            Directory.CreateDirectory(stray);
            using (var capture = document.Capture(id, service.CurrentStamp))
            {
                SessionTestProtocol.Check(!(await service.SaveAsync(capture, CancellationToken.None)).Succeeded && service.IsRecoveryBlocked,
                    "A save refused by recovery blocked after startup did not report the blockage.");
            }
            Directory.Delete(stray);
            using (var capture = document.Capture(id, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded && !service.IsRecoveryBlocked,
                    "Removing the cause of the blockage did not clear it on the next save.");
            }
            Directory.CreateDirectory(stray);
            var vetoed = false;
            try { await service.PrepareExplicitCloseAsync(new[] { id }); }
            catch (SessionDataCorruptedException) { vetoed = true; }
            SessionTestProtocol.Check(vetoed && service.IsRecoveryBlocked, "A close vetoed by blocked recovery did not report the blockage.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckSnapshotOffMaintenanceAsync()
    {
        var owner = Guid.NewGuid(); var otherWindow = Guid.NewGuid(); var crashedWindow = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var other = SessionTestProtocol.CreateService(otherWindow);
        service.IsBackupEnabled = other.IsBackupEnabled = false;
        try
        {
            // Every window binds authority at load whatever the snapshot preference; with snapshots off nothing is published.
            await service.InitializeAuthorityAsync();
            await other.InitializeAuthorityAsync();
            var crashed = SessionTestProtocol.CreateService(crashedWindow);
            await crashed.InitializeAuthorityAsync();
            await crashed.DisposeAsync();
            await using var live = await SessionTestDocument.CreateAsync(owner, "live العربية😀");
            live.Editor.InsertText(0, "typed\r");
            var orphans = new[]
            {
                PlantAsset(owner, ".utf8"), PlantAsset(owner, ".npj"),
                PlantAsset(crashedWindow, ".utf8"), PlantAsset(crashedWindow, ".npj")
            };
            var foreign = new[] { PlantAsset(otherWindow, ".utf8"), PlantAsset(otherWindow, ".npj") };
            await service.RunMaintenanceAsync(CancellationToken.None);
            SessionTestProtocol.Check(orphans.All(path => !File.Exists(path)),
                "Snapshot-off maintenance kept an orphaned baseline or journal of this window or of a crashed one.");
            SessionTestProtocol.Check(File.Exists(live.Baseline.File.Path) && File.Exists(live.Journal.File.Path) && foreign.All(File.Exists),
                "Snapshot-off maintenance collected a live editor's assets or another live window's files.");
        }
        finally
        {
            await service.DisposeAsync(); await other.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(owner, otherWindow, crashedWindow);
        }
    }

    private static async Task CheckMaintenancePassAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var document = await SessionTestDocument.CreateAsync(owner, "maintained");
            var orphan = PlantAsset(owner, ".npj");
            var store = RecoveryRootStore.CreateStore();
            async Task<int> CountCheckpointsAsync()
            {
                using (await SessionRecoveryTransaction.EnterAsync())
                    return store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries.Count;
            }

            // Saves alone never prune, so the fixture reaches five checkpoints before the one pass.
            for (var index = 0; index < 5; index++)
            {
                document.Editor.InsertText(0, $"{index}\r");
                using var capture = document.Capture(id, service.CurrentStamp);
                var result = await service.SaveAsync(capture, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded && result.Changed, "A maintenance fixture save did not commit.");
            }
            SessionTestProtocol.Check(await CountCheckpointsAsync() == 5 && File.Exists(orphan),
                "The maintenance fixture lacks its five checkpoints or its orphan.");
            await service.RunMaintenanceAsync(CancellationToken.None);
            SessionTestProtocol.Check(await CountCheckpointsAsync() == 3 && !File.Exists(orphan) && File.Exists(document.Journal.File.Path),
                "One maintenance pass did not prune to three checkpoints and collect only the orphan.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckRetiredScopePruningAsync()
    {
        var owner = Guid.NewGuid(); var retired = Guid.NewGuid(); var id = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var closed = SessionTestProtocol.CreateService(retired);
        try
        {
            // A secondary window's last stretch: each save publishes a checkpoint and a rescue, and none prunes.
            await closed.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using (var document = await SessionTestDocument.CreateAsync(retired, "retired العربية😀"))
            {
                for (var index = 0; index < 5; index++)
                {
                    document.Editor.InsertText(0, $"{index}\r");
                    using var capture = document.Capture(id, closed.CurrentStamp);
                    var result = await closed.SaveAsync(capture, CancellationToken.None);
                    SessionTestProtocol.Check(result.Succeeded && result.Changed, "A retired-scope fixture save did not commit.");
                }
            }
            // The window is gone: no writer lease and no live editor leases its assets, as after a crash.
            await closed.DisposeAsync();
            var orphan = PlantAsset(retired, ".npj");
            var store = RecoveryRootStore.CreateStore();
            var checkpointArea = store.GetScopeArea(retired, RecoveryAreaKind.Checkpoints);
            var rescueArea = store.GetScopeArea(retired, RecoveryAreaKind.Rescue, id);
            IReadOnlyList<RecoveryAddress> checkpoints, rescues;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                checkpoints = store.Enumerate(checkpointArea).Entries;
                rescues = store.Enumerate(rescueArea).Entries;
            }
            SessionTestProtocol.Check(checkpoints.Count == 5 && rescues.Count == 5,
                "The retired scope fixture lacks its five checkpoints and five rescues.");

            await service.InitializeAuthorityAsync();
            await service.RunMaintenanceAsync(CancellationToken.None);
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(retired);
                SessionTestProtocol.Check(snapshot.Checkpoints.Select(read => read.Address).ToHashSet()
                    .SetEquals(checkpoints.OrderByDescending(address => address.Ordinal).Take(3)),
                    "Another window's maintenance did not keep exactly a retired scope's newest three checkpoints.");
                SessionTestProtocol.Check(store.Enumerate(rescueArea).Entries.SequenceEqual(rescues),
                    "Another window's maintenance retired a retired scope's rescue records.");
                var local = ApplicationData.Current.LocalFolder.Path;
                var referenced = snapshot.Checkpoints.SelectMany(read => read.Record.Session.TextEditors).SelectMany(editor =>
                    new[] { editor.SavedBaseline.FileName, editor.RecoveryBaseline.FileName }.Where(name => name != null)
                        .Select(name => Path.Combine(local, "DocumentBaselines", name))
                        .Append(Path.Combine(local, "DocumentJournals", editor.Journal.FileName))).ToArray();
                SessionTestProtocol.Check(referenced.Length != 0 && referenced.All(File.Exists) && !File.Exists(orphan),
                    "Collecting a retired scope removed an asset its kept checkpoints reference, or skipped its orphan.");
            }
        }
        finally
        {
            await service.DisposeAsync(); await closed.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(owner, retired);
        }
    }

    // A previous process's leftover: on disk, named for its owner, referenced by nothing and leased by no one.
    private static string PlantAsset(Guid owner, string extension)
    {
        var folder = Path.Combine(ApplicationData.Current.LocalFolder.Path, extension == ".npj" ? "DocumentJournals" : "DocumentBaselines");
        Directory.CreateDirectory(folder);
        var path = Path.Combine(folder, owner.ToString("N") + "-" + Guid.NewGuid().ToString("N") + extension);
        File.WriteAllText(path, "orphan");
        return path;
    }

    private static async Task CheckRescueRetirementAsync()
    {
        var owner = Guid.NewGuid();
        var identities = Enumerable.Range(0, 4).Select(_ => Guid.NewGuid()).ToArray();
        var kept = identities[0];
        var closed = identities.Skip(1).ToArray();
        var service = SessionTestProtocol.CreateService(owner);
        var documents = new List<SessionTestDocument>();
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            foreach (var id in identities) documents.Add(await SessionTestDocument.CreateAsync(owner, "churn العربية😀\r"));
            async Task PublishAsync()
            {
                using var capture = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp);
                var result = await service.SaveAsync(capture, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded && result.Changed, "A churn publication did not commit.");
                await service.RunMaintenanceAsync(CancellationToken.None);
            }

            // Each generation edits every tab, so every tab builds rescue history.
            for (var generation = 0; generation < 4; generation++)
            {
                foreach (var document in documents) document.Editor.InsertText(0, $"{generation}\r");
                await PublishAsync();
            }
            var store = RecoveryRootStore.CreateStore();
            TextEditorSessionDataV2 pending;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                pending = (await SessionRecoveryCatalog.ReadScopeAsync(owner)).Rescues.Where(read => read.Record.Editor.Id == closed[0])
                    .MaxBy(read => read.Address.Ordinal).Record.Editor;
                var area = store.GetScopeArea(owner, RecoveryAreaKind.Pending, closed[0]);
                RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
                { Kind = RecoveryRecordKind.Pending, Stamp = store.CreatePublicationStamp(area, service.CurrentStamp), Editor = pending }));
            }
            SessionTestProtocol.Check(await service.PrepareExplicitCloseAsync(closed), "The churned tabs did not close durably.");
            foreach (var document in documents.Skip(1)) await document.DisposeAsync();
            documents.RemoveRange(1, documents.Count - 1);
            documents[0].Editor.InsertText(0, "after close\r");
            await PublishAsync();
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                SessionTestProtocol.Check(closed.All(id => !Directory.Exists(store.Paths.AreaPath(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, id)))),
                    "Maintenance kept the rescue history of a closed tab.");
                SessionTestProtocol.Check(store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, kept)).Entries.Count == 3,
                    "Maintenance did not keep exactly three rescue generations of the open tab.");
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(snapshot.Checkpoints.Count == 3 && snapshot.Session.TextEditors.Single().Id == kept &&
                    snapshot.Pending.Single().Record.Editor.ComputeSha256() == pending.ComputeSha256() &&
                    closed.All(id => SessionRecoveryAuthority.IsClosed(snapshot.Stamp, id, snapshot.Decisions.Select(read => read.Record))),
                    "Rescue retirement lost checkpoint history, Pending evidence or close suppression.");
                SessionTestProtocol.Check(File.Exists(Path.Combine(ApplicationData.Current.LocalFolder.Path, "DocumentJournals", pending.Journal.FileName)),
                    "Maintenance collected the journal that Pending evidence references.");
            }

            await service.ClearSessionDataAsync();
            await PublishAsync();
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var rescues = await SessionRecoveryCatalog.ReadAreaAsync(store, store.GetScopeArea(owner, RecoveryAreaKind.Rescue, kept), CancellationToken.None);
                SessionTestProtocol.Check(rescues.Count == 1 && SessionRecoveryAuthority.SameEpoch(rescues[0].Record.Stamp, service.CurrentStamp),
                    "Maintenance kept rescue records of a cleared epoch.");
            }
        }
        finally
        {
            foreach (var document in documents) await document.DisposeAsync();
            await service.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(owner);
        }
    }

    private static async Task CheckPartialCloseBatchAsync()
    {
        var owner = Guid.NewGuid(); var first = Guid.NewGuid(); var second = Guid.NewGuid();
        var faults = new SecondDecisionFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = SessionTestProtocol.CreateService(owner, store);
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var a = await SessionTestDocument.CreateAsync(owner, "first");
            await using var b = await SessionTestDocument.CreateAsync(owner, "second");
            using (var capture = SessionTestProtocol.CaptureDocuments(new[] { a, b }, new[] { first, second }, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                    "The two-tab close fixture did not commit.");
            }
            faults.Enabled = true;
            var closed = new List<Guid>();
            var failed = false;
            try { await service.PrepareExplicitCloseAsync(new[] { first, second }, closed); }
            catch (Exception) { failed = true; }
            SessionTestProtocol.Check(failed && closed.Count == 1 && closed[0] == first,
                "A partially committed close batch did not report exactly the tab whose close is confirmed, leaving out an indeterminate one.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    // The second close decision fails to rename and then to read back, so its publication is indeterminate.
    private sealed class SecondDecisionFailure : IRecoveryStorageFaults
    {
        private readonly HashSet<Guid> _operations = [];
        public bool Enabled { get; set; }
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (!Enabled || address.Area.Kind != RecoveryAreaKind.Decisions) return;
            if (step == RecoveryStorageStep.BeforeRename) _operations.Add(address.OperationId);
            if (_operations.Count > 1 && step is RecoveryStorageStep.BeforeRename or RecoveryStorageStep.BeforeRead)
                throw new IOException("Injected failure of the second close decision.");
        }
    }

    private static async Task CheckInterruptedResetAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new ResetDecisionFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = SessionTestProtocol.CreateService(owner, store);
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            var previous = service.CurrentStamp;
            faults.Enabled = true;
            var failed = false;
            try { await service.ClearSessionDataAsync(); }
            catch (Exception) { failed = true; }
            faults.Enabled = false;
            SessionTestProtocol.Check(failed && service.CurrentStamp.EpochId != previous.EpochId,
                "A reset whose intent committed left the service on the dead epoch.");
            await using var document = await SessionTestDocument.CreateAsync(owner, "after reset");
            using var capture = document.Capture(id, service.CurrentStamp);
            SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                "Backup did not resume after a reset whose decision publication failed.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private sealed class ResetDecisionFailure : IRecoveryStorageFaults
    {
        public bool Enabled { get; set; }
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (Enabled && address.Area.Kind == RecoveryAreaKind.Decisions && copy != "intent" && step == RecoveryStorageStep.BeforeRename)
                throw new IOException("Injected reset decision failure after its intent committed.");
        }
    }

    private static async Task CheckCleanFileRowAsync()
    {
        var owner = Guid.NewGuid();
        var id = Guid.NewGuid();
        var file = await ApplicationData.Current.TemporaryFolder.CreateFileAsync("clean.txt", CreationCollisionOption.GenerateUniqueName);
        await FileIO.WriteTextAsync(file, "captured");
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using (var document = await SessionTestDocument.CreateAsync(owner, "captured"))
            {
                document.Metadata.HasEditingFile = true; document.Metadata.RequiresSaveAs = false;
                using var checkpoint = document.Editor.AcquireJournalCheckpoint();
                using var capture = new SessionCapture { ExpectedStamp = service.CurrentStamp, SelectedEditorId = id };
                capture.Documents.Add(new SessionDocumentCapture
                {
                    Id = id, EditingFile = file, EditingFileName = file.Name, EditingFilePath = file.Path,
                    Recovery = new DocumentRecoveryState(document.Saved, document.Baseline, document.Journal, checkpoint,
                        document.Metadata, false, document.Editor.Length)
                });
                SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                    "The clean file row did not commit.");
            }
            await service.DisposeAsync();
            await FileIO.WriteTextAsync(file, "changed on disk");
            service = SessionTestProtocol.CreateService(owner);
            using (var batch = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Single().InitializeFromFile &&
                    await batch.Documents[0].SavedSnapshot.ReadTextAsync() == "changed on disk",
                    "A clean file row restored a stale recovery copy instead of its file.");
            }
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using (new FileStream(file.Path, FileMode.Open, FileAccess.Read, FileShare.None))
            using (var batch = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Single().Checkpoint != null && service.RecoveryOutcome != SessionRecoveryOutcome.Partial,
                    "An unreadable clean file did not fall back to its recovery copy.");
            }
        }
        finally
        {
            await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner);
            await file.DeleteAsync(StorageDeleteOption.PermanentDelete);
        }
    }

    private static async Task CheckEmptyLegacyRowAsync()
    {
        var owner = Guid.NewGuid();
        var good = Guid.NewGuid(); var empty = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles", CreationCollisionOption.OpenIfExists);
        var file = await folder.CreateFileAsync("legacy.txt", CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(file, "legacy");
        var metadata = new DocumentMetadata { LastSavedEncoding = "UTF-8", LastSavedLineEnding = "CRLF", FontZoomFactor = 1 };
        await SessionTestProtocol.WriteLegacySourceAsync(owner, new NotepadsSessionDataV1
        {
            SelectedTextEditor = good,
            TextEditors = new List<TextEditorSessionDataV1>
            {
                new() { Id = empty, StateMetaData = metadata },
                new() { Id = good, LastSavedBackupFilePath = file.Path, StateMetaData = metadata }
            }
        });
        try
        {
            using var batch = await service.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None);
            SessionTestProtocol.Check(batch.Documents.Count == 1 && batch.SelectedEditorId == good &&
                service.RecoveryOutcome != SessionRecoveryOutcome.Partial && service.UnrecoveredEditorCount == 1,
                "An empty V1 row was reported as a failed restore.");
            await using var restored = await SessionTestDocument.CreateAsync(owner, await batch.Documents[0].SavedSnapshot.ReadTextAsync());
            using var capture = restored.Capture(good, batch.ExpectedStamp);
            await service.CommitAttachedAsync(capture, new[] { good }, CancellationToken.None);
            SessionTestProtocol.Check(service.UnrecoveredEditorCount == 0, "An empty V1 row kept the restore incomplete.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckDeletedEditingFileAsync()
    {
        var owner = Guid.NewGuid();
        var id = Guid.NewGuid();
        var file = await ApplicationData.Current.TemporaryFolder.CreateFileAsync("editing.txt", CreationCollisionOption.GenerateUniqueName);
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var document = await SessionTestDocument.CreateAsync(owner, "content");
            document.Metadata.HasEditingFile = true; document.Metadata.RequiresSaveAs = false;
            async Task<SessionSaveResult> SaveAsync(SessionService target)
            {
                using var capture = document.Capture(id, target.CurrentStamp);
                capture.Documents[0].EditingFile = file;
                capture.Documents[0].EditingFileName = file.Name;
                capture.Documents[0].EditingFilePath = file.Path;
                return await target.SaveAsync(capture, CancellationToken.None);
            }

            var result = await SaveAsync(service);
            SessionTestProtocol.Check(result.Succeeded && result.Changed, "The editing file fixture did not commit.");
            await file.DeleteAsync(StorageDeleteOption.PermanentDelete);
            result = await SaveAsync(service);
            SessionTestProtocol.Check(result.Succeeded && !result.Changed,
                "A deleted editing file failed or re-granted an unchanged session publication.");

            // After a restart the old grant no longer resolves and a new one is refused.
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            SessionTestProtocol.Check((await SaveAsync(service)).Succeeded,
                "A refused editing file grant failed the whole session publication.");
        }
        finally
        {
            await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner);
            if (await ApplicationData.Current.TemporaryFolder.TryGetItemAsync(file.Name) is { } leftover)
                await leftover.DeleteAsync(StorageDeleteOption.PermanentDelete);
        }
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
                await service.RunMaintenanceAsync(CancellationToken.None);
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
                    var roots = await SessionRecoveryCatalog.ReadReferencesAsync();
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
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles", CreationCollisionOption.OpenIfExists);
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
            SessionTestProtocol.Check(await service.PrepareExplicitCloseAsync(new[] { good }), "The adopted tab close could not publish.");
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
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles", CreationCollisionOption.OpenIfExists);
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
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles", CreationCollisionOption.OpenIfExists);
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
                var references = await SessionRecoveryCatalog.ReadReferencesAsync();
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
        SessionScopeData.GetManifestFileName(owner), () => [], store)
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
