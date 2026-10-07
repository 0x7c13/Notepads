// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;

namespace NotepadsEditorTests;

internal static class SessionPerformanceRegressionTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        await CheckDeltaRescueAsync();
        await CheckRestoreDrainAsync(log);
        log.AppendLine("PASS: delta rescue publication, damaged evidence replacement, failed-checkpoint retry, reset epoch, and bounded restore ordering/cancellation/drain.");
    }

    private static async Task CheckDeltaRescueAsync()
    {
        var owner = Guid.NewGuid();
        var faults = new PublicationObserver();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = SessionTestProtocol.CreateService(owner, store);
        var documents = new List<SessionTestDocument>();
        var identities = Enumerable.Range(0, 3).Select(_ => Guid.NewGuid()).ToArray();
        try
        {
            await service.EnsureMetadataRetainedAsync(CancellationToken.None);
            foreach (var id in identities)
                documents.Add(await SessionTestDocument.CreateAsync(owner, "delta العربية😀\r"));
            using (var initial = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
                SessionTestProtocol.Check((await service.SaveAsync(initial, CancellationToken.None)).Succeeded, "Initial delta fixture failed.");
            SessionTestProtocol.Check(faults.RescueCopies == 6, "First publication must establish all rescue mirrors.");
            using (var selection = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                selection.SelectedEditorId = identities[1];
                var result = await service.SaveAsync(selection, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded && result.Changed && faults.RescueCopies == 6,
                    "Selection-only publication rewrote unchanged rescue evidence.");
            }
            documents[0].Editor.InsertText(0, "edited\r");
            using (var changed = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(changed, CancellationToken.None)).Succeeded && faults.RescueCopies == 8,
                    "One document change must publish just its two rescue copies.");
            }

            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var address = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, identities[1])).Entries.MaxBy(item => item.Ordinal);
                File.WriteAllText(store.Paths.CopyPath(address, "a"), "damaged A");
                File.WriteAllText(store.Paths.CopyPath(address, "b"), "damaged B");
            }
            using (var missing = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                missing.TabScrollOffset = 5;
                SessionTestProtocol.Check((await service.SaveAsync(missing, CancellationToken.None)).Succeeded && faults.RescueCopies == 10,
                    "A committed descriptor cannot substitute for missing verified rescue evidence.");
            }
            documents[0].Editor.InsertText(0, "retry\r");
            faults.FailCheckpoint = true;
            using (var failed = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check(!(await service.SaveAsync(failed, CancellationToken.None)).Succeeded && faults.RescueCopies == 12,
                    "The fixture must fail its checkpoint after publishing the changed rescue.");
            }

            faults.FailCheckpoint = false;
            using (var retry = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(retry, CancellationToken.None)).Succeeded && faults.RescueCopies == 14,
                    "An uncommitted checkpoint must remain retryable without rewriting other documents.");
            }

            var previousEpoch = service.CurrentStamp.EpochId;
            await service.ClearSessionDataAsync();
            SessionTestProtocol.Check(service.CurrentStamp.EpochId != previousEpoch, "Reset did not change the epoch.");
            using (var reset = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(reset, CancellationToken.None)).Succeeded && faults.RescueCopies == 20,
                    "A new epoch must publish fresh rescue evidence for every document.");
            }

            foreach (var document in documents) await document.DisposeAsync();
            documents.Clear();
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            SessionTestProtocol.Check(await service.PrepareExplicitCloseAsync(new[] { identities[1] }),
                "Explicit close must initialize authority in a fresh service after reset.");
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(snapshot.Session.TextEditors.Select(editor => editor.Id).SequenceEqual(new[] { identities[0], identities[2] }),
                    "Delta checkpoints must retain full tab order and honor durable closure.");
            }
        }
        finally
        {
            foreach (var document in documents) await document.DisposeAsync();
            await service.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(owner);
        }
    }

    private static async Task CheckRestoreDrainAsync(StringBuilder log)
    {
        var owner = Guid.NewGuid();
        try
        {
            using var baseline = await DocumentBaseline.FromTextAsync("restore\r", ownerId: owner);
            var rows = Enumerable.Range(0, 3).Select(_ => new PreparedRecoveryDocument { RecoveryBaseline = baseline }).ToArray();
            var calls = new List<int>();
            var first = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var running = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
            {
                calls.Add(index);
                return index == 0 ? first.Task : Task.CompletedTask;
            }, CancellationToken.None, hasMemoryHeadroom: () => true);
            SessionTestProtocol.Check(calls.Count == 2 && !running.IsCompleted,
                "Restoration must start at most two independent loaders and wait for the unfinished loader.");
            first.SetResult();
            await running;
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2 }), "Completion order changed tab order or skipped a row.");

            calls.Clear();
            first = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var failure = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
            {
                calls.Add(index);
                if (index == 1) throw new InvalidOperationException("Synchronous restore failure.");
                return first.Task;
            }, CancellationToken.None, hasMemoryHeadroom: () => true);
            SessionTestProtocol.Check(calls.Count == 2, "The peer failure must exercise two active loaders.");
            SessionTestProtocol.Check(!failure.IsCompleted, "A synchronous peer failure released pins before the active loader drained.");
            first.SetResult();
            try { await failure; throw new Exception("Restore failure was hidden."); }
            catch (InvalidOperationException) { }
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1 }), "A failed group must not start further rows.");

            rows[0].InitializeFromFile = true;
            calls.Clear();
            first = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            using var cancellation = new CancellationTokenSource();
            var canceled = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
            {
                calls.Add(index);
                return first.Task;
            }, cancellation.Token);
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0 }), "An unknown-size original file must restore serially.");
            cancellation.Cancel();
            SessionTestProtocol.Check(!canceled.IsCompleted, "Cancellation released the active loader's pins before drain.");
            first.SetResult();
            try { await canceled; throw new Exception("Restore cancellation was hidden."); }
            catch (OperationCanceledException) { }
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0 }), "Cancellation started another document.");
            rows[0].InitializeFromFile = false;
            calls.Clear();
            first = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var second = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            using var pairedCancellation = new CancellationTokenSource();
            var pairedCanceled = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
            {
                calls.Add(index);
                return index == 0 ? first.Task : second.Task;
            }, pairedCancellation.Token, hasMemoryHeadroom: () => true);
            SessionTestProtocol.Check(calls.Count == 2, "Paired cancellation must exercise two active loaders.");
            pairedCancellation.Cancel();
            first.SetResult();
            await Task.Yield();
            SessionTestProtocol.Check(!pairedCanceled.IsCompleted, "Cancellation failed to drain the second active loader.");
            second.SetResult();
            try { await pairedCanceled; throw new Exception("Paired restore cancellation was hidden."); }
            catch (OperationCanceledException) { }
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1 }), "Paired cancellation started a further row.");

            calls.Clear();
            first = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var pressured = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
            {
                calls.Add(index);
                return index == 0 ? first.Task : Task.CompletedTask;
            }, CancellationToken.None, hasMemoryHeadroom: () => false);
            SessionTestProtocol.Check(calls.Count == 1, "Memory pressure must prevent paired restoration.");
            first.SetResult();
            await pressured;
            log.AppendLine("PASS: deterministic paired failure/cancellation drain and serial restoration under memory pressure.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private sealed class PublicationObserver : IRecoveryStorageFaults
    {
        public int RescueCopies { get; private set; }
        public bool FailCheckpoint { get; set; }
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (step != RecoveryStorageStep.BeforeRename) return;
            if (address.Area.Kind == RecoveryAreaKind.Rescue) RescueCopies++;
            if (FailCheckpoint && address.Area.Kind == RecoveryAreaKind.Checkpoints)
                throw new IOException("Injected checkpoint failure after rescue publication.");
        }
    }
}
