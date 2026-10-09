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
            var rows = Enumerable.Range(0, 4).Select(_ => new PreparedRecoveryDocument { RecoveryBaseline = baseline }).ToArray();
            // Only gate-based callbacks run on this context. Native fixture creation
            // and cleanup retain the owning UI apartment's normal context.
            using (var context = new RestoreTestContext())
            {
                CheckRollingRestore(context, rows);
                CheckRestoreCallbackFailures(context, rows);
                CheckCompletedPeerFailures(context, rows);
                CheckAsyncRestoreFailure(context, rows);
                CheckRestoreCancellation(context, rows);
                CheckRestoreMemoryPressure(context, rows);
                CheckExclusiveRestore(context, rows);
                CheckAdmissionFailure(context, rows);
            }
            await CheckRestoreSizeBudgetAsync(owner, baseline);
            log.AppendLine("PASS: deterministic rolling restore order, maximum two loaders, completed-peer failure/cancellation, exclusive/pressure barriers, 32 MiB size boundary, and admission failure drain.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static void CheckRollingRestore(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        var gates = rows.Select(_ => new RestoreGate()).ToArray();
        var calls = new List<int>();
        gates[1].Succeed();
        var running = RunGatedRestore(rows, gates, calls);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2 }) && !gates[0].Finished && !running.IsCompleted,
            "A completed second loader must admit the third while the first remains pending, with at most two active loaders.");
        context.Complete(gates[2].Succeed);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2, 3 }) && !gates[0].Finished && !gates[3].Finished,
            "Rolling admission must preserve input indices and keep refilling the freed slot.");
        context.Complete(gates[0].Succeed);
        SessionTestProtocol.Check(!running.IsCompleted, "Restoration returned before its final active loader drained.");
        context.Complete(gates[3].Succeed);
        CheckRestoreSucceeded(running);
    }

    private static void CheckRestoreCallbackFailures(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        var calls = new List<int>();
        var synchronous = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
        {
            calls.Add(index);
            throw new InvalidOperationException("First callback failed synchronously.");
        }, CancellationToken.None, hasMemoryHeadroom: () => true);
        CheckRestoreFailed<InvalidOperationException>(synchronous);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0 }), "A synchronous first-callback failure admitted another row.");

        calls.Clear();
        var noTask = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
        {
            calls.Add(index);
            return null;
        }, CancellationToken.None, hasMemoryHeadroom: () => true);
        CheckRestoreFailed<InvalidOperationException>(noTask);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0 }), "A null callback task admitted another row.");

        calls.Clear();
        var first = new RestoreGate();
        var peerFailure = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
        {
            calls.Add(index);
            if (index == 1) throw new InvalidOperationException("Peer callback failed synchronously.");
            return first.RunAsync();
        }, CancellationToken.None, hasMemoryHeadroom: () => true);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1 }) && !peerFailure.IsCompleted && !first.Finished,
            "A synchronous peer failure must drain the first loader before returning.");
        context.Complete(first.Succeed);
        CheckRestoreFailed<InvalidOperationException>(peerFailure);
        SessionTestProtocol.Check(first.Finished && calls.SequenceEqual(new[] { 0, 1 }),
            "A synchronous peer failure escaped without draining, or admitted another row.");
    }

    private static void CheckCompletedPeerFailures(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        foreach (var cancelPeer in new[] { false, true })
        {
            var calls = new List<int>();
            // Return these tasks directly: both loader tasks must already be complete
            // before the queued WhenAny continuation is allowed to refill a slot.
            var first = new TaskCompletionSource();
            var second = new TaskCompletionSource();
            var running = RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
            {
                calls.Add(index);
                return index == 0 ? first.Task : second.Task;
            }, CancellationToken.None, hasMemoryHeadroom: () => true);
            context.Complete(() =>
            {
                first.SetResult();
                if (cancelPeer) second.SetCanceled();
                else second.SetException(new InvalidOperationException("Completed peer failed."));
            });
            if (cancelPeer) CheckRestoreFailed<OperationCanceledException>(running);
            else CheckRestoreFailed<InvalidOperationException>(running);
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1 }),
                "A successful WhenAny result hid a completed peer failure or cancellation and admitted another row.");
        }
    }

    private static void CheckAsyncRestoreFailure(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        var gates = rows.Select(_ => new RestoreGate()).ToArray();
        var calls = new List<int>();
        var running = RunGatedRestore(rows, gates, calls);
        context.Complete(() => gates[1].Fail(new InvalidOperationException("Asynchronous peer failure.")));
        SessionTestProtocol.Check(gates[1].Finished && !gates[0].Finished && !running.IsCompleted &&
            calls.SequenceEqual(new[] { 0, 1 }), "An asynchronous peer failure must stop admission and drain its active peer.");
        context.Complete(gates[0].Succeed);
        CheckRestoreFailed<InvalidOperationException>(running);
        SessionTestProtocol.Check(gates[0].Finished && calls.SequenceEqual(new[] { 0, 1 }),
            "A failed restore returned before drain or admitted another row.");
    }

    private static void CheckRestoreCancellation(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        var gates = rows.Select(_ => new RestoreGate()).ToArray();
        var calls = new List<int>();
        using var pairedCancellation = new CancellationTokenSource();
        var paired = RunGatedRestore(rows, gates, calls, pairedCancellation.Token);
        context.Complete(() =>
        {
            pairedCancellation.Cancel();
            gates[0].Succeed();
        });
        SessionTestProtocol.Check(gates[0].Finished && !gates[1].Finished && !paired.IsCompleted &&
            calls.SequenceEqual(new[] { 0, 1 }), "Cancellation must stop admission and drain the remaining active peer.");
        context.Complete(gates[1].Succeed);
        CheckRestoreFailed<OperationCanceledException>(paired);

        gates = rows.Select(_ => new RestoreGate()).ToArray();
        calls.Clear();
        using var finalCancellation = new CancellationTokenSource();
        var final = RunGatedRestore(rows.Take(1).ToArray(), gates, calls, finalCancellation.Token);
        finalCancellation.Cancel();
        SessionTestProtocol.Check(!final.IsCompleted && !gates[0].Finished, "Cancellation escaped while the final loader was pending.");
        context.Complete(gates[0].Succeed);
        CheckRestoreFailed<OperationCanceledException>(final);
        SessionTestProtocol.Check(gates[0].Finished && calls.SequenceEqual(new[] { 0 }),
            "Cancellation during the final drain must be reported after the loader finishes.");

        rows[0].InitializeFromFile = true;
        try
        {
            gates = rows.Select(_ => new RestoreGate()).ToArray();
            calls.Clear();
            using var exclusiveCancellation = new CancellationTokenSource();
            var exclusive = RunGatedRestore(rows, gates, calls, exclusiveCancellation.Token);
            exclusiveCancellation.Cancel();
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0 }) && !exclusive.IsCompleted,
                "Cancellation must retain the pending exclusive loader.");
            context.Complete(gates[0].Succeed);
            CheckRestoreFailed<OperationCanceledException>(exclusive);
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0 }), "Cancellation admitted a row after the exclusive loader.");
        }
        finally { rows[0].InitializeFromFile = false; }
    }

    private static void CheckRestoreMemoryPressure(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        var gates = rows.Select(_ => new RestoreGate()).ToArray();
        var calls = new List<int>();
        var headroom = true;
        var admissionChecks = 0;
        var running = RunGatedRestore(rows, gates, calls, hasMemoryHeadroom: () => { admissionChecks++; return headroom; });
        SessionTestProtocol.Check(admissionChecks == 2, "Memory headroom must be checked before each initial admission.");
        headroom = false;
        context.Complete(gates[0].Succeed);
        SessionTestProtocol.Check(admissionChecks == 3 && calls.SequenceEqual(new[] { 0, 1 }) && !gates[1].Finished,
            "Pressure after a slot becomes free must drain the other loader before admitting another row.");
        context.Complete(gates[1].Succeed);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2 }) && !running.IsCompleted,
            "Persistent pressure must allow progress with one serial loader.");
        context.Complete(gates[2].Succeed);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2, 3 }) && gates[2].Finished && !gates[3].Finished,
            "Under pressure, the previous loader must finish before the next starts.");
        context.Complete(gates[3].Succeed);
        CheckRestoreSucceeded(running);

        gates = rows.Select(_ => new RestoreGate()).ToArray();
        calls.Clear();
        var serial = RunGatedRestore(rows, gates, calls, hasMemoryHeadroom: () => false);
        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0 }), "Initial memory pressure must admit only one loader.");
        for (var index = 0; index < gates.Length; index++)
        {
            SessionTestProtocol.Check(calls.Count == index + 1, "Memory pressure allowed overlapping serial loaders.");
            context.Complete(gates[index].Succeed);
        }
        CheckRestoreSucceeded(serial);
    }

    private static void CheckExclusiveRestore(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        rows[2].InitializeFromFile = true;
        try
        {
            var gates = rows.Select(_ => new RestoreGate()).ToArray();
            var calls = new List<int>();
            var running = RunGatedRestore(rows, gates, calls);
            context.Complete(gates[0].Succeed);
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1 }) && !gates[1].Finished,
                "An unknown-size row must wait for all preceding loaders.");
            context.Complete(gates[1].Succeed);
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2 }) && !gates[2].Finished,
                "Rows after an exclusive loader must wait for it to finish.");
            context.Complete(gates[2].Succeed);
            SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2, 3 }) && gates[2].Finished,
                "An exclusive row changed input order or failed to release the next row.");
            context.Complete(gates[3].Succeed);
            CheckRestoreSucceeded(running);
        }
        finally { rows[2].InitializeFromFile = false; }
    }

    private static void CheckAdmissionFailure(RestoreTestContext context, PreparedRecoveryDocument[] rows)
    {
        var gates = rows.Select(_ => new RestoreGate()).ToArray();
        var calls = new List<int>();
        var admissionChecks = 0;
        var running = RunGatedRestore(rows, gates, calls, hasMemoryHeadroom: () =>
        {
            if (++admissionChecks == 3) throw new InvalidOperationException("Memory headroom query failed.");
            return true;
        });
        context.Complete(gates[0].Succeed);
        SessionTestProtocol.Check(admissionChecks == 3 && !gates[1].Finished && !running.IsCompleted &&
            calls.SequenceEqual(new[] { 0, 1 }), "An admission-check failure must drain every active loader before escaping.");
        context.Complete(gates[1].Succeed);
        CheckRestoreFailed<InvalidOperationException>(running);
        SessionTestProtocol.Check(gates[1].Finished && calls.SequenceEqual(new[] { 0, 1 }),
            "An admission-check failure escaped without draining or started a later row.");
    }

    private static async Task CheckRestoreSizeBudgetAsync(Guid owner, DocumentBaseline smallBaseline)
    {
        const int parallelBytes = 32 * 1024 * 1024;
        var chunk = new byte[64 * 1024];
        for (var index = 0; index < chunk.Length; index++) chunk[index] = (byte)'x';
        DocumentBaseline atLimit = null;
        DocumentBaseline aboveLimit = null;
        try
        {
            // Stream real canonical assets with one small reusable buffer. The
            // scheduler callbacks below never load these assets into native editors.
            atLimit = await CreateBaselineAsync(parallelBytes);
            aboveLimit = await CreateBaselineAsync(parallelBytes + 1);
            SessionTestProtocol.Check(atLimit.ByteLength == parallelBytes && aboveLimit.ByteLength == parallelBytes + 1,
                "The restore size fixture must straddle the exact 32 MiB boundary by one byte.");
            using (var context = new RestoreTestContext())
            {
                foreach (var overBudget in new[] { false, true })
                {
                    var rows = Enumerable.Range(0, 4).Select(index => new PreparedRecoveryDocument
                    {
                        RecoveryBaseline = index == 2 ? (overBudget ? aboveLimit : atLimit) : smallBaseline
                    }).ToArray();
                    var gates = rows.Select(_ => new RestoreGate()).ToArray();
                    var calls = new List<int>();
                    var running = RunGatedRestore(rows, gates, calls);
                    context.Complete(gates[0].Succeed);
                    if (overBudget)
                    {
                        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1 }) && !gates[1].Finished,
                            "A baseline one byte above 32 MiB must wait for every preceding loader.");
                        context.Complete(gates[1].Succeed);
                        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2 }) && !gates[2].Finished,
                            "A later eligible row must wait for the over-budget middle row to finish.");
                    }
                    else
                    {
                        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2 }) && !gates[1].Finished && !gates[2].Finished,
                            "A baseline exactly at 32 MiB must overlap the pending preceding loader.");
                        context.Complete(gates[1].Succeed);
                        SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2, 3 }) && !gates[2].Finished && !gates[3].Finished,
                            "A later eligible row must overlap a baseline exactly at 32 MiB.");
                    }
                    context.Complete(gates[2].Succeed);
                    SessionTestProtocol.Check(calls.SequenceEqual(new[] { 0, 1, 2, 3 }) && gates[2].Finished && !gates[3].Finished,
                        "The size boundary must preserve row order and drain the final eligible loader.");
                    context.Complete(gates[3].Succeed);
                    CheckRestoreSucceeded(running);
                }
            }
        }
        finally
        {
            if (aboveLimit != null) await aboveLimit.DisposeAsync();
            if (atLimit != null) await atLimit.DisposeAsync();
        }

        Task<DocumentBaseline> CreateBaselineAsync(int byteLength) => DocumentBaseline.CreateAsync(async stream =>
        {
            for (var remaining = byteLength; remaining > 0;)
            {
                var count = Math.Min(chunk.Length, remaining);
                await stream.WriteAsync(chunk, 0, count);
                remaining -= count;
            }
        }, owner);
    }

    private static Task RunGatedRestore(PreparedRecoveryDocument[] rows, RestoreGate[] gates, List<int> calls,
        CancellationToken cancellationToken = default, Func<bool> hasMemoryHeadroom = null)
    {
        return RecoveryRestorePipeline.RunAsync(rows, (index, document) =>
        {
            SessionTestProtocol.Check(ReferenceEquals(rows[index], document), "Restore callbacks lost their original document index.");
            SessionTestProtocol.Check(gates.Count(gate => gate.Entered && !gate.Finished) < 2, "Restoration exceeded two active loaders.");
            calls.Add(index);
            return gates[index].RunAsync();
        }, cancellationToken, hasMemoryHeadroom ?? (() => true));
    }

    private static void CheckRestoreSucceeded(Task running)
    {
        SessionTestProtocol.Check(running.IsCompleted, "Restore did not finish after all gates drained.");
        running.GetAwaiter().GetResult();
    }

    private static void CheckRestoreFailed<TException>(Task running) where TException : Exception
    {
        SessionTestProtocol.Check(running.IsCompleted, "Restore did not report its failure after all gates drained.");
        try { running.GetAwaiter().GetResult(); }
        catch (TException) { return; }
        throw new Exception("Restore failed to report " + typeof(TException).Name + ".");
    }

    private sealed class RestoreGate
    {
        // Synchronous task signaling posts captured-context continuations before
        // Complete returns; no worker-pool timing is used as a test barrier.
        private readonly TaskCompletionSource _release = new TaskCompletionSource();
        public bool Entered { get; private set; }
        public bool Finished { get; private set; }

        public async Task RunAsync()
        {
            SessionTestProtocol.Check(!Entered, "A restore callback ran twice for the same input row.");
            Entered = true;
            try { await _release.Task; }
            finally { Finished = true; }
        }

        public void Succeed() => _release.SetResult();
        public void Fail(Exception error) => _release.SetException(error);
    }

    private sealed class RestoreTestContext : SynchronizationContext, IDisposable
    {
        private readonly SynchronizationContext _previous = Current;
        private readonly Queue<Action> _pending = new Queue<Action>();

        public RestoreTestContext() => SetSynchronizationContext(this);

        public override void Post(SendOrPostCallback callback, object state)
        {
            lock (_pending) _pending.Enqueue(() => callback(state));
        }

        public void Complete(Action completion)
        {
            // Complete outside the captured context, keeping both direct loader
            // tasks completable before any scheduler continuation runs.
            SetSynchronizationContext(_previous);
            try { completion(); }
            finally { SetSynchronizationContext(this); }
            while (true)
            {
                Action continuation;
                lock (_pending)
                {
                    if (_pending.Count == 0) return;
                    continuation = _pending.Dequeue();
                }
                continuation();
            }
        }

        public void Dispose() => SetSynchronizationContext(_previous);
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
