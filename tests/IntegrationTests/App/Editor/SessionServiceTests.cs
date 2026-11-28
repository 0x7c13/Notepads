// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
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
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Windows.Storage;
using Windows.Storage.AccessCache;
using WinUIEditor;

namespace NotepadsEditorTests;

internal static class SessionServiceTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        await CheckFrozenMixedCaptureAsync();
        await CheckCheckpointOnlyCloseAsync();
        await CheckArchiveUnclaimedAsync();
        await CheckResetFileBindingAsync();
        await CheckBoundedHistoryAndFallbackAsync();
        await CheckLegacyConsumptionAsync();
        await CheckAdoptionRetryAsync();
        await CheckMixedRescueAsync();
        await SessionPendingRescueTests.RunAsync(log);
        await CheckInactiveAdoptionRetryAsync();
        await CheckNewerRescueRetentionAsync();
        await CheckDeletedEditingFileAsync();
        await CheckCleanFileRowAsync();
        await CheckEmptyLegacyRowAsync();
        await CheckInterruptedResetAsync();
        await CheckIndeterminateResetIntentAsync();
        await CheckPartialCloseBatchAsync();
        await CheckRescueRetirementAsync();
        await CheckSnapshotOffMaintenanceAsync();
        await CheckMaintenancePassAsync();
        await CheckRetiredScopePruningAsync();
        await CheckLiveRecoveryBlockAsync();
        await SessionPerformanceRegressionTests.RunAsync(log);
        log.AppendLine("PASS: session recovery, reset and maintenance preserve documents across save failures.");
    }

    private static async Task CheckFrozenMixedCaptureAsync()
    {
        var owner = Guid.NewGuid(); var first = Guid.NewGuid(); var second = Guid.NewGuid();
        var faults = new CheckpointFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = SessionTestProtocol.CreateService(owner, store);
        try
        {
            await service.InitializeAuthorityAsync();
            await using var a = await SessionTestDocument.CreateAsync(owner, "first العربية😀");
            await using var b = await SessionTestDocument.CreateAsync(owner, "second pending");
            using (var initial = SessionTestProtocol.CaptureDocuments(new[] { a, b }, new[] { first, second }, service.CurrentStamp))
                SessionTestProtocol.Check((await service.SaveAsync(initial, CancellationToken.None)).Succeeded, "Mixed fixture did not commit.");
            using var plan = await service.DiscoverRecoveryAsync();
            var source = plan.Sources.Single(item => item.Id == second);
            var originalHash = source.Descriptor.ComputeSha256();
            var changedCopy = source.Descriptor;
            changedCopy.StateMetaData.FileNamePlaceholder = "caller changed its copy";
            var changedStamp = source.ExpectedStamp;
            changedStamp.EpochId = Guid.NewGuid();
            SessionTestProtocol.Check(source.Descriptor.ComputeSha256() == originalHash && source.ExpectedStamp.EpochId == plan.ExpectedStamp.EpochId,
                "Discovery exposed mutable descriptor or epoch evidence.");
            using var prepared = await RecoveryDocumentPreparation.PrepareAsync(plan.Sources[0], owner, new DocumentLoadOptions(), CancellationToken.None);
            await using var loaded = await SessionTestDocument.FromPreparedAsync(owner, prepared);
            SessionTestProtocol.Check(loaded.Editor.GetText(100) == "first العربية😀", "Pure preparation failed to make native content available.");
            loaded.Editor.InsertText(0, "edited ");
            using var mixed = loaded.Capture(first, plan.ExpectedStamp);
            mixed.Documents.Insert(0, SessionDocumentCapture.Retain(source));
            mixed.SelectedEditorId = second;
            mixed.TabScrollOffset = 31;
            faults.Enabled = true;
            SessionTestProtocol.Check(!(await service.SaveAsync(mixed, CancellationToken.None)).Succeeded &&
                loaded.Editor.GetText(100).StartsWith("edited ", StringComparison.Ordinal),
                "A failed backup revoked or replaced an already usable native document.");
            faults.Enabled = false;
            var saved = await service.SaveAsync(mixed, CancellationToken.None);
            SessionTestProtocol.Check(saved.Succeeded && saved.Descriptors.Count == 2 && saved.Descriptors[second].ComputeSha256() == originalHash,
                "The save result omitted or changed the retained descriptor.");
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(snapshot.SelectedCheckpoint.Record.Session.TextEditors.Select(item => item.Id).SequenceEqual(new[] { second, first }) &&
                    snapshot.Selection.SelectedEditorId == second && snapshot.Selection.TabScrollViewerHorizontalOffset == 31,
                    "Mixed native/retained capture lost exact tab order, selection or scroll.");
            }
            plan.Dispose();
            mixed.TabScrollOffset = 32;
            SessionTestProtocol.Check((await service.SaveAsync(mixed, CancellationToken.None)).Succeeded,
                "Releasing drained history pins invalidated still-owned retained metadata.");
            using var invalid = loaded.Capture(first, service.CurrentStamp);
            invalid.Documents.Add(SessionDocumentCapture.Retain(source));
            invalid.Documents.Add(SessionDocumentCapture.Retain(source));
            SessionTestProtocol.Check(!(await service.SaveAsync(invalid, CancellationToken.None)).Succeeded, "Duplicate captured IDs were accepted.");
            using var exact = loaded.Capture(first, service.CurrentStamp);
            SessionTestProtocol.Check((await service.SaveAsync(exact, CancellationToken.None)).Succeeded, "Exact one-row publication failed.");
            using (await SessionRecoveryTransaction.EnterAsync())
                SessionTestProtocol.Check((await SessionRecoveryCatalog.ReadScopeAsync(owner)).SelectedCheckpoint.Record.Session.TextEditors.Single().Id == first,
                    "Publication silently appended a source omitted by the complete capture.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckCheckpointOnlyCloseAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.InitializeAuthorityAsync();
            await using var document = await SessionTestDocument.CreateAsync(owner, "checkpoint-only retained");
            using (var capture = document.Capture(id, service.CurrentStamp))
                SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Checkpoint-only fixture did not commit.");
            var store = RecoveryRootStore.CreateStore();
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var area = store.GetScopeArea(owner, RecoveryAreaKind.Rescue, id);
                foreach (var address in store.Enumerate(area).Entries)
                    foreach (var copy in new[] { "a", "b" }) File.Delete(store.Paths.CopyPath(address, copy));
            }
            var closed = new List<Guid>();
            SessionTestProtocol.Check(await service.PrepareExplicitCloseAsync(new[] { id }, closed) && closed.SequenceEqual(new[] { id }),
                "An owned checkpoint-only tab did not receive a durable Closed decision.");
            using var restart = await service.DiscoverRecoveryAsync();
            SessionTestProtocol.Check(restart.Sources.Count == 0, "Closing a checkpoint-only tab resurrected it on discovery.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckArchiveUnclaimedAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid(); var replacement = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.InitializeAuthorityAsync();
            await using var original = await SessionTestDocument.CreateAsync(owner, "unclaimed old workspace");
            using (var capture = original.Capture(id, service.CurrentStamp))
                SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Archive fixture did not commit.");
            using var unclaimed = await service.DiscoverRecoveryAsync();
            var old = unclaimed.Sources.Single().Descriptor;
            await service.ArchiveRecoverySourcesAsync(unclaimed.Sources);
            await using var current = await SessionTestDocument.CreateAsync(owner, "new workspace");
            using (var capture = current.Capture(replacement, service.CurrentStamp))
                SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "New exact workspace did not commit after archival.");
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                var references = await SessionRecoveryCatalog.ReadReferencesAsync();
                SessionTestProtocol.Check(snapshot.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor.Id == replacement &&
                    references.JournalFileNames.Contains(old.Journal.FileName) &&
                    SessionRecoveryAuthority.IsClosed(snapshot.Stamp, id, snapshot.Decisions.Select(item => item.Record)),
                    "Archiving unclaimed recovery dropped its bytes or left it eligible for automatic reimport.");
            }
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }
    private static async Task CheckResetFileBindingAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var file = await ApplicationData.Current.TemporaryFolder.CreateFileAsync("reset-grant.txt", CreationCollisionOption.GenerateUniqueName);
        await FileIO.WriteTextAsync(file, "still open after reset");
        try
        {
            await service.InitializeAuthorityAsync();
            await using var document = await SessionTestDocument.CreateAsync(owner, "still open after reset");
            document.Metadata.HasEditingFile = true;
            document.Metadata.RequiresSaveAs = false;
            using (var initial = document.Capture(id, service.CurrentStamp))
            {
                initial.Documents[0].EditingFile = file;
                initial.Documents[0].EditingFileName = file.Name;
                initial.Documents[0].EditingFilePath = file.Path;
                SessionTestProtocol.Check((await service.SaveAsync(initial, CancellationToken.None)).Succeeded, "File-binding fixture did not commit.");
            }
            RecoveryFileAccessBinding binding;
            using (var plan = await service.DiscoverRecoveryAsync())
            using (var prepared = await RecoveryDocumentPreparation.PrepareAsync(plan.Sources.Single(), owner,
                new DocumentLoadOptions(), CancellationToken.None))
                binding = prepared.FileAccessBinding;
            await service.ClearSessionDataAsync([]);
            await service.RunMaintenanceAsync(CancellationToken.None);
            SessionTestProtocol.Check(!StorageApplicationPermissions.FutureAccessList.ContainsItem(binding.Token),
                "Reset fixture did not collect the obsolete file grant.");
            using var resumed = document.Capture(id, service.CurrentStamp);
            resumed.Documents[0].EditingFile = binding.File;
            resumed.Documents[0].EditingFileName = binding.File.Name;
            resumed.Documents[0].EditingFilePath = binding.File.Path;
            resumed.Documents[0].FileAccessBinding = binding;
            var saved = await service.SaveAsync(resumed, CancellationToken.None);
            SessionTestProtocol.Check(saved.Succeeded && saved.Descriptors[id].EditingFileFutureAccessToken != binding.Token &&
                StorageApplicationPermissions.FutureAccessList.ContainsItem(saved.Descriptors[id].EditingFileFutureAccessToken),
                "A pre-reset file binding republished an obsolete grant after its token was collected.");
        }
        finally
        {
            await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner);
            await file.DeleteAsync(StorageDeleteOption.PermanentDelete);
        }
    }

    private static async Task CheckLiveRecoveryBlockAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        try
        {
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
            await closed.InitializeAuthorityAsync(CancellationToken.None);
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
                SessionTestProtocol.Check(snapshot.Checkpoints.Count == 3 && snapshot.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor.Id == kept &&
                    snapshot.Pending.Single().Record.Editor.ComputeSha256() == pending.ComputeSha256() &&
                    closed.All(id => SessionRecoveryAuthority.IsClosed(snapshot.Stamp, id, snapshot.Decisions.Select(read => read.Record))),
                    "Rescue retirement lost checkpoint history, Pending evidence or close suppression.");
                SessionTestProtocol.Check(File.Exists(Path.Combine(ApplicationData.Current.LocalFolder.Path, "DocumentJournals", pending.Journal.FileName)),
                    "Maintenance collected the journal that Pending evidence references.");
            }

            await service.ClearSessionDataAsync([]);
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
            var previous = service.CurrentStamp;
            faults.Enabled = true;
            var failed = false;
            try { await service.ClearSessionDataAsync([]); }
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

    private static async Task CheckIndeterminateResetIntentAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new ResetIntentAcknowledgementFailure();
        var service = SessionTestProtocol.CreateService(owner,
            new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults));
        try
        {
            await service.InitializeAuthorityAsync();
            var before = service.CurrentStamp;
            var failed = false;
            try { await service.ClearSessionDataAsync([]); }
            catch (IOException) { failed = true; }
            SessionTestProtocol.Check(failed && service.CurrentStamp.EpochId != before.EpochId,
                "An indeterminate reset intent did not reconcile the verified new epoch before returning failure.");
            await using var document = await SessionTestDocument.CreateAsync(owner, "after indeterminate intent");
            using var capture = document.Capture(id, service.CurrentStamp);
            SessionTestProtocol.Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                "Backup stayed bound to the dead epoch after a renamed reset intent lost its acknowledgement.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private sealed class ResetIntentAcknowledgementFailure : IRecoveryStorageFaults
    {
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (address.Area.Kind == RecoveryAreaKind.Decisions && copy == "intent" &&
                step is RecoveryStorageStep.AfterRename or RecoveryStorageStep.BeforeRead)
                throw new IOException("Injected lost reset-intent acknowledgement and failed readback.");
        }
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
            using (var batch = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Single().InitializeFromFile &&
                    await batch.Documents[0].SavedSnapshot.ReadTextAsync() == "changed on disk",
                    "A clean file row restored a stale recovery copy instead of its file.");
                SessionTestProtocol.Check(ReferenceEquals(batch.Documents[0].EditingFile, batch.Documents[0].FileAccessBinding?.File) &&
                    batch.Documents[0].FileAccessBinding.Token == batch.Plan.Sources.Single().EditingFileFutureAccessToken,
                    "Preparation discarded the exact resolved file/token binding before publication.");
            }
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using (new FileStream(file.Path, FileMode.Open, FileAccess.Read, FileShare.None))
            using (var batch = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
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
            using var batch = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
            SessionTestProtocol.Check(batch.Documents.Count == 1 && batch.SelectedEditorId == good &&
                service.RecoveryOutcome != SessionRecoveryOutcome.Partial && batch.UnconsumedSources.Count == 1,
                "An empty V1 row was reported as a failed restore.");
            await using var restored = await SessionTestDocument.CreateAsync(owner, await batch.Documents[0].SavedSnapshot.ReadTextAsync());
            using var capture = restored.Capture(good, batch.ExpectedStamp);
            await SessionTestProtocol.PublishRecoveredAsync(service, batch, capture);
            SessionTestProtocol.Check(batch.UnconsumedSources.Count == 0, "An empty V1 row kept the restore incomplete.");
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
            await service.InitializeAuthorityAsync(CancellationToken.None);
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
                    repaired.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor.Id == id, "A verified peer did not repair its mirror.");
                // A rescue may commit before its workspace checkpoint fails.
                // Its higher revision must still be part of restart ordering.
                var laterRescue = SessionRecoveryCatalog.CloneEditor(snapshot.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor);
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
            using (var ordering = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            using (var resumed = document.CaptureState())
            {
                SessionTestProtocol.Check(resumed.CaptureRevision > snapshot.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor.CaptureRevision + 10000,
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
            using (var batch = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
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
            using (var batch = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Count == 1 && batch.Documents[0].Id != id &&
                    batch.Documents[0].Metadata.RequiresSaveAs && batch.Documents[0].EditingFile == null,
                    "All-index salvage reused an original file association or failed to recover an older rescue prefix.");
                SessionTestProtocol.Check(batch.Documents[0].Metadata.LanguageOverride == "csharp",
                    "Rescue salvage lost the manual document language.");
            }
            await service.ClearSessionDataAsync([]);
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
            using (var batch = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(batch.Documents.Count == 1 && batch.UnconsumedSources.Count == 2,
                    "A failed V1 row blocked the valid row or preparation consumed rows prematurely.");
                await using var restored = await SessionTestDocument.CreateAsync(owner, await batch.Documents[0].SavedSnapshot.ReadTextAsync());
                using var capture = restored.Capture(good, batch.ExpectedStamp);
                await SessionTestProtocol.PublishRecoveredAsync(service, batch, capture);
                SessionTestProtocol.Check(batch.UnconsumedSources.Count == 1, "Partial V1 adoption consumed the failed row.");
            }
            SessionTestProtocol.Check(await service.PrepareExplicitCloseAsync(new[] { good }), "The adopted tab close could not publish.");
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using (var afterClose = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(afterClose.Documents.Count == 0 && afterClose.UnconsumedSources.Count == 1,
                    "Close revoked successful V1 consumption and reimported discarded content.");
            }

            using (var unconsumed = await service.DiscoverRecoveryAsync())
                await service.ClearSessionDataAsync(unconsumed.Sources);
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using (var afterReset = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                SessionTestProtocol.Check(afterReset.Documents.Count == 0 && afterReset.UnconsumedSources.Count == 0,
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
            using var batch = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
            await using var target = await SessionTestDocument.CreateAsync(owner, "retry source");
            using var capture = target.Capture(id, batch.ExpectedStamp);
            faults.Enabled = true;
            try
            {
                await SessionTestProtocol.PublishRecoveredAsync(service, batch, capture);
                throw new InvalidOperationException("The injected checkpoint publication failure was ignored.");
            }
            catch (IOException) { }
            faults.Enabled = false;
            await SessionTestProtocol.PublishRecoveredAsync(service, batch, capture);
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var authority = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(authority.Decisions.Count(read => read.Record.Kind == RecoveryRecordKind.AdoptLegacy) == 1 &&
                    authority.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor.Id == id,
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
        const string adoptedEdit = "later adopted العربية e\u0301😀\r";
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
            using (var prepared = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                await using var adopted = await SessionTestDocument.CreateAsync(owner, "adopted text");
                using (var initial = adopted.Capture(adoptedId, prepared.ExpectedStamp))
                    await SessionTestProtocol.PublishRecoveredAsync(service, prepared, initial);
                adopted.Editor.InsertText(0, adoptedEdit);
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
            using var recovered = await SessionTestProtocol.MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
            SessionTestProtocol.Check(recovered.Documents.Count == 2 && recovered.Documents.Any(document => document.Id == adoptedId && document.Source.IsOwned) &&
                recovered.Documents.Any(document => document.Id != adoptedId && document.Metadata.RequiresSaveAs) &&
                service.RecoveryOutcome == SessionRecoveryOutcome.Rescue,
                "Durable adoption suppressed salvage of another ordinary tab when every checkpoint was lost.");
            // A normal checkpoint may publish while another own rescue waits in the background queue.
            await using var ready = await SessionTestDocument.FromPreparedAsync(owner, recovered.Documents.Single(item => item.Id == adoptedId));
            SessionTestProtocol.Check(ready.Editor.GetText(1000) == adoptedEdit + "adopted text",
                "Recovery kept the adopted identity but replaced its later persisted edits with the original adoption content.");
            using (var first = ready.Capture(adoptedId, recovered.ExpectedStamp))
                await SessionTestProtocol.PublishRecoveredAsync(service, recovered, first);
            var queuedRescue = recovered.Documents.Single(item => item.Id != adoptedId);
            await using var rescued = await SessionTestDocument.FromPreparedAsync(owner, queuedRescue);
            using (var both = ready.Capture(adoptedId, recovered.ExpectedStamp))
            {
                both.Documents.Add(new SessionDocumentCapture { Id = queuedRescue.Id, Recovery = rescued.CaptureState() });
                await SessionTestProtocol.PublishRecoveredAsync(service, recovered, both);
            }
            using (await SessionRecoveryTransaction.EnterAsync())
                SessionTestProtocol.Check((await SessionRecoveryCatalog.ReadScopeAsync(owner)).Selection.Documents.Count(row => row.Membership == RecoveryMembership.Owned) == 2,
                    "An intervening workspace checkpoint made a queued, pinned own rescue impossible to adopt.");
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
            await sourceService.InitializeAuthorityAsync(CancellationToken.None);
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
            using var batch = await SessionTestProtocol.MaterializeAsync(targetService, new DocumentLoadOptions(), CancellationToken.None);
            var prepared = batch.Documents.Single(document => document.Id == id);
            using (var foreign = new SessionCapture { ExpectedStamp = batch.ExpectedStamp, SelectedEditorId = id })
            {
                foreign.Documents.Add(SessionDocumentCapture.Retain(prepared.Source));
                SessionTestProtocol.Check(!(await targetService.SaveAsync(foreign, CancellationToken.None)).Succeeded,
                    "An unadopted foreign descriptor entered the owned workspace index.");
            }
            SessionTestProtocol.Check(prepared.Checkpoint.CommittedSequence == goodSequence &&
                targetService.RecoveryOutcome == SessionRecoveryOutcome.Partial &&
                store.Enumerate(store.GetScopeArea(sourceOwner, RecoveryAreaKind.Pending, id)).Entries.Count != 0,
                "An inactive scope's bad latest document skipped earlier good content or failed to preserve Pending.");
            await using var target = await SessionTestDocument.FromPreparedAsync(targetOwner, prepared);
            using var capture = target.Capture(id, batch.ExpectedStamp);
            faults.Enabled = true;
            try
            {
                await SessionTestProtocol.PublishRecoveredAsync(targetService, batch, capture);
                throw new InvalidOperationException("The injected checkpoint failure after inactive adoption was ignored.");
            }
            catch (IOException) { }
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                SessionTestProtocol.Check((await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner)).Selection.Documents.Count(row => row.Membership == RecoveryMembership.Owned) == 0,
                    "The committed inactive adoption did not consume its source before the later checkpoint failed.");
            }

            faults.Enabled = false;
            using var retry = target.Capture(id, batch.ExpectedStamp);
            await SessionTestProtocol.PublishRecoveredAsync(targetService, batch, retry);
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var targetRoots = await SessionRecoveryCatalog.ReadScopeAsync(targetOwner);
                SessionTestProtocol.Check(targetRoots.Decisions.Count(read => read.Record.Kind == RecoveryRecordKind.AdoptInactive) == 1 &&
                    targetRoots.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Descriptor).Any(editor => editor.Id == id),
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
            await sourceService.InitializeAuthorityAsync(CancellationToken.None);
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
            using var batch = await SessionTestProtocol.MaterializeAsync(targetService, new DocumentLoadOptions(), CancellationToken.None);
            var prepared = batch.Documents.Single(document => document.Id == id);
            SessionTestProtocol.Check(prepared.Checkpoint.CommittedSequence == selectedSequence,
                "An incomplete workspace publication silently changed the selected recovery content.");
            await using var target = await SessionTestDocument.FromPreparedAsync(targetOwner, prepared);
            using var capture = target.Capture(id, batch.ExpectedStamp);
            SessionTestProtocol.Check(capture.Documents[0].Recovery.CaptureRevision > newerDescriptor.CaptureRevision,
                "The importing capture floor omitted higher inactive source rescue evidence.");
            await SessionTestProtocol.PublishRecoveredAsync(targetService, batch, capture);
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
                SessionTestProtocol.Check(snapshot.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Owned) == 0 && snapshot.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Rescue) == 0 &&
                    references.JournalFileNames.Contains(newerDescriptor.Journal.FileName),
                    "A consumed higher source rescue was reimported or its manual recovery assets became collectible.");
            }
            batch.Dispose();
            await targetService.DisposeAsync(); targetService = SessionTestProtocol.CreateService(targetOwner, primary: true);
            using var restarted = await SessionTestProtocol.MaterializeAsync(targetService, new DocumentLoadOptions(), CancellationToken.None);
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

internal sealed partial class SessionTestRecoveryBatch : IDisposable
{
    public SessionTestRecoveryBatch(SessionRecoveryPlan plan) { Plan = plan; }
    public SessionRecoveryPlan Plan { get; }
    public List<PreparedRecoveryDocument> Documents { get; } = [];
    public HashSet<Guid> EmptyIds { get; } = [];
    public HashSet<Guid> AcknowledgedIds { get; } = [];
    public IReadOnlyList<RecoveryDocumentSource> UnconsumedSources => Plan.Sources
        .Where(source => !EmptyIds.Contains(source.Id) && !AcknowledgedIds.Contains(source.Id)).ToArray();
    public Guid? SelectedEditorId => Plan.SelectedEditorId;
    public double TabScrollOffset => Plan.TabScrollOffset;
    public RecoveryStamp ExpectedStamp => Plan.ExpectedStamp;
    public void Dispose()
    {
        foreach (var document in Documents) document.Dispose();
        Plan.Dispose();
    }
}
internal static class SessionTestProtocol
{
    // This test harness deliberately owns the plan and materializes it eagerly. Production discovers only metadata.
    public static async Task<SessionTestRecoveryBatch> MaterializeAsync(SessionService service,
        DocumentLoadOptions defaults, CancellationToken cancellation, SessionRecoveryPlan plan = null)
    {
        var batch = new SessionTestRecoveryBatch(plan ?? await service.DiscoverRecoveryAsync(cancellation));
        try
        {
            foreach (var source in batch.Plan.Sources)
            {
                PreparedRecoveryDocument prepared = null;
                try { prepared = await RecoveryDocumentPreparation.PrepareAsync(source, batch.ExpectedStamp.ScopeId, defaults, cancellation); }
                catch (OperationCanceledException) { throw; }
                catch
                {
                    if (source.Descriptor != null)
                    {
                        await service.PreserveFailedNewestAsync(source, cancellation);
                        for (var index = 0; index < source.FallbackCount; index++)
                        {
                            try { prepared = await RecoveryDocumentPreparation.PrepareAsync(source, batch.ExpectedStamp.ScopeId, defaults, cancellation, index); break; }
                            catch (OperationCanceledException) { throw; }
                            catch { }
                        }
                    }
                    if (prepared == null) continue;
                }
                if (prepared == null) batch.EmptyIds.Add(source.Id);
                else batch.Documents.Add(prepared);
            }
            return batch;
        }
        catch { batch.Dispose(); throw; }
    }

    public static async Task PublishRecoveredAsync(SessionService service, SessionTestRecoveryBatch batch, SessionCapture capture)
    {
        foreach (var document in capture.Documents)
        {
            var prepared = batch.Documents.FirstOrDefault(item => item.Id == document.Id);
            document.Source = prepared?.Source;
            document.FileAccessBinding = prepared?.FileAccessBinding;
            if (document.Source is { IsOwned: false })
            {
                await service.PublishAdoptionAsync(document, capture.ExpectedStamp);
                batch.AcknowledgedIds.Add(document.Id);
            }
        }
        foreach (var source in batch.UnconsumedSources.Where(source => source.IsOwned && capture.Documents.All(item => item.Id != source.Id)))
            capture.Documents.Add(SessionDocumentCapture.Retain(source));
        var result = await service.SaveAsync(capture, CancellationToken.None);
        if (!result.Succeeded) throw new IOException("The recovery checkpoint did not commit.");
        foreach (var document in capture.Documents.Where(document => !document.IsRetained)) batch.AcknowledgedIds.Add(document.Id);
    }
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
        foreach (var owner in owners)
        {
            if (owner == Guid.Empty || SessionScopeData.IsReservedOwner(owner))
                throw new InvalidOperationException("Test cleanup requires its explicitly generated private scope.");
        }
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

            // Committed grants intentionally outlive service disposal so restart and
            // durability assertions can use them. Retire them only with this fixture's
            // explicitly generated owner, after those assertions and all host disposal.
            var prefix = SessionDocumentStore.GetFutureAccessTokenPrefix(owner);
            foreach (var token in StorageApplicationPermissions.FutureAccessList.Entries
                .Where(entry => entry.Token.StartsWith(prefix, StringComparison.Ordinal))
                .Select(entry => entry.Token).ToArray())
                StorageApplicationPermissions.FutureAccessList.Remove(token);
        }
    }
}

internal sealed class SessionTestDocument : IAsyncDisposable
{
    private readonly EditorBaseControl _fixture = new();
    internal EditorBaseControl View => _fixture;
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
        Notepads.Features.Sessions.Recovery.PreparedRecoveryDocument prepared, Action nativeRestoreReturned = null)
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
            nativeRestoreReturned?.Invoke();
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
