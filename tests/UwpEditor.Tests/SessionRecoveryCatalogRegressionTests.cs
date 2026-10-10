// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Features.Sessions.Validation;
using static NotepadsEditorTests.SessionTestProtocol;

namespace NotepadsEditorTests;

internal static class SessionRecoveryCatalogRegressionTests
{
    private const string OriginalText = "receipt العربية e\u0301😀\r";
    private const string ReceiverEdit = "receiver-only 世界\r";

    public static async Task RunAsync(StringBuilder log)
    {
        foreach (var history in Enum.GetValues<ReceiverHistory>()) await CheckReceiverHistoryAsync(history);
        await CheckCheckpointPendingAfterOfferAsync(newerOffer: false);
        await CheckCheckpointPendingAfterOfferAsync(newerOffer: true);
        await CheckOrdinaryRescueAfterOfferAsync();
        await CheckUnreadableRescueAsync(locked: false);
        await CheckUnreadableRescueAsync(locked: true);
        await CheckPartialRescueAsync();
        await CheckSupersededRescueAsync();
        await CheckResetContentBoundaryAsync(corruptBeforeReset: true);
        await CheckResetContentBoundaryAsync(corruptBeforeReset: false);
        await CheckFailedResetDoesNotRetireAsync();
        await CheckDurableFallbackAsync(receipt: true);
        await CheckDurableFallbackAsync(receipt: false);
        log.AppendLine("PASS: receiver checkpoints and rescues survive missing ACKs and older outgoing offers.");
        log.AppendLine("PASS: unreadable rescues block empty replacement and retry after repair.");
        log.AppendLine("PASS: reset retires observed content damage while new-epoch uncertainty remains protected.");
        log.AppendLine("PASS: durable transfer/adoption fallback preserves Pending evidence and lifecycle decisions.");
    }

    private enum ReceiverHistory
    {
        ReceiptOnly, IdenticalCheckpoint, ChangedCheckpoint, IdenticalRescue,
        ChangedRescue, RescueAfterEmptyCheckpoint, RescueAfterOutgoingOffer, UnreadableHistory
    }

    private static async Task CheckReceiverHistoryAsync(ReceiverHistory history)
    {
        await using var transfer = await TransferFixture.CreateAsync(history == ReceiverHistory.RescueAfterEmptyCheckpoint);
        var owner = Guid.NewGuid();
        var service = CreateService(owner, primary: true);
        try
        {
            var changed = history is ReceiverHistory.ChangedCheckpoint or ReceiverHistory.ChangedRescue or
                ReceiverHistory.RescueAfterEmptyCheckpoint or ReceiverHistory.RescueAfterOutgoingOffer or ReceiverHistory.UnreadableHistory;
            var rescueOnly = history is ReceiverHistory.IdenticalRescue or ReceiverHistory.ChangedRescue or
                ReceiverHistory.RescueAfterEmptyCheckpoint or ReceiverHistory.RescueAfterOutgoingOffer;
            if (history == ReceiverHistory.RescueAfterOutgoingOffer) await transfer.CreateOutgoingOfferAsync();
            if (changed) transfer.Receiver.Editor.InsertText(0, ReceiverEdit);
            if (history != ReceiverHistory.ReceiptOnly)
            {
                using var capture = changed ? transfer.Receiver.Capture(transfer.ReceiverId, transfer.ReceiverService.CurrentStamp) :
                    transfer.CaptureReceipt();
                transfer.Faults.Enabled = rescueOnly;
                var result = await transfer.ReceiverService.SaveAsync(capture, CancellationToken.None);
                transfer.Faults.Enabled = false;
                Check(result.Succeeded != rescueOnly, "Receiver fixture did not reach its intended checkpoint publication boundary.");
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(transfer.ReceiverOwner);
                    Check(snapshot.Rescues.Any(read => read.State == RecoveryReadState.Valid &&
                        read.Record.Editor.Id == transfer.ReceiverId), "Interrupted receiver publication did not leave a durable rescue.");
                    if (history == ReceiverHistory.UnreadableHistory)
                    {
                        var store = RecoveryRootStore.CreateStore();
                        foreach (var root in snapshot.Checkpoints.Concat(snapshot.Rescues))
                            foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(root.Address, peer), "bad");
                    }
                }
            }
            await transfer.RetireReceiverAsync();
            using (var batch = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                if (history == ReceiverHistory.ReceiptOnly)
                {
                    Check(batch.Plan.Sources.Count == 0, "An untouched unacknowledged receipt was imported as an independent receiver document.");
                    return;
                }
                var prepared = batch.Documents.Single(document => document.Source.ExpectedStamp.ScopeId == transfer.ReceiverOwner);
                await using var restored = await SessionTestDocument.FromPreparedAsync(owner, prepared);
                Check(restored.Editor.GetText(1000) == (changed && history != ReceiverHistory.UnreadableHistory ? ReceiverEdit : string.Empty) + OriginalText,
                    "Inactive recovery lost independently published receiver content after its source never acknowledged.");
                if (history == ReceiverHistory.UnreadableHistory)
                    Check(service.RecoveryOutcome == SessionRecoveryOutcome.Partial,
                        "Unknown receiver history was treated as proof of duplication or its degraded recovery was hidden.");
                using var capture = restored.Capture(prepared.Id, batch.ExpectedStamp);
                await PublishRecoveredAsync(service, batch, capture);
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var target = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                    Check(target.Decisions.Any(read => read.Record.Kind == RecoveryRecordKind.AdoptInactive &&
                        read.Record.Consumptions.Any(key => key.SourceScopeId == transfer.ReceiverOwner &&
                            key.SourceEditorId == transfer.ReceiverId)), "Receiver discovery selected content that could not be durably adopted.");
                    Check(!await TransferRootReader.IsSourceAcknowledgedAsync(transfer.Token), "Receiver recovery fabricated a source move ACK.");
                }
            }
            if (history == ReceiverHistory.RescueAfterOutgoingOffer)
            {
                await service.RunMaintenanceAsync(CancellationToken.None);
                await service.DisposeAsync(); service = CreateService(owner, primary: true);
                using var restarted = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
                Check(restarted.Documents.Count == 1 && restarted.Documents[0].Source.IsOwned,
                    "Outgoing-offer recovery duplicated or failed to consume its inactive incarnation.");
                await using var restored = await SessionTestDocument.FromPreparedAsync(owner, restarted.Documents[0]);
                Check(restored.Editor.GetText(1000) == ReceiverEdit + OriginalText,
                    "Adoption and maintenance lost receiver edits newer than its outgoing transfer offer.");
            }
            else
            {
                using var second = await service.DiscoverRecoveryAsync();
                Check(second.Sources.Count == 1 && second.Sources[0].IsOwned,
                    "Receiver adoption did not consume its inactive incarnation or lost the independently owned target.");
            }
        }
        finally { await service.DisposeAsync(); await CleanupAsync(owner); }
    }

    private static async Task CheckCheckpointPendingAfterOfferAsync(bool newerOffer)
    {
        await using var transfer = await TransferFixture.CreateAsync();
        const string offeredEdit = "offered intermediate Україна\r";
        transfer.Receiver.Editor.InsertText(0, offeredEdit);
        await PublishOfferAsync(transfer.ReceiverOwner, transfer.ReceiverId,
            transfer.Receiver, transfer.ReceiverService.CurrentStamp);
        transfer.Receiver.Editor.InsertText(0, ReceiverEdit);
        transfer.Faults.Enabled = true;
        using (var capture = transfer.Receiver.Capture(transfer.ReceiverId, transfer.ReceiverService.CurrentStamp))
            Check(!(await transfer.ReceiverService.SaveAsync(capture, CancellationToken.None)).Succeeded,
                "Checkpoint-pending fixture did not stop after publishing its newer rescue.");
        transfer.Faults.Enabled = false;
        using (var plan = await transfer.ReceiverService.DiscoverRecoveryAsync())
        using (var capture = new SessionCapture { ExpectedStamp = plan.ExpectedStamp })
        {
            var source = plan.Sources.Single();
            Check(source.IsOwnScopeRescue, "Receiver's newest rescue was masked by its earlier outgoing offer.");
            capture.PendingRescues.Add(source);
            Check((await transfer.ReceiverService.SaveAsync(capture, CancellationToken.None)).Succeeded,
                "The real pending-only checkpoint publication failed.");
        }
        using (await SessionRecoveryTransaction.EnterAsync())
        {
            var store = RecoveryRootStore.CreateStore();
            var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(transfer.ReceiverOwner);
            Check(snapshot.SelectedCheckpoint.Record.Session.TextEditors.Count == 0 &&
                snapshot.SelectedCheckpoint.Record.Session.PendingRescues.Single().Id == transfer.ReceiverId,
                "The pending fixture accidentally persisted an ordinary checkpoint member.");
            foreach (var rescue in snapshot.Rescues)
                Check(await store.DeleteAsync(rescue.Address), "The checkpoint-only fixture retained its standalone rescue metadata.");
        }
        TextEditorSessionDataV2 failedNewest = null;
        string brokenJournal = null;
        if (newerOffer)
        {
            await using var newest = await SessionTestDocument.CreateAsync(transfer.ReceiverOwner, "newer offered generation 日本語😀\r");
            failedNewest = await PublishOfferAsync(transfer.ReceiverOwner, transfer.ReceiverId,
                newest, transfer.ReceiverService.CurrentStamp);
            brokenJournal = newest.Journal.File.Path;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(transfer.ReceiverOwner);
                Check(snapshot.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor.ComputeSha256() == failedNewest.ComputeSha256() &&
                    snapshot.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Rescue) == 0,
                    "A newer outgoing offer failed to replace the older checkpoint-carried rescue candidate.");
                Check(snapshot.Selection.Documents.Single(row => row.Id == transfer.ReceiverId).Fallbacks.Select(row => row.Descriptor).Any(editor => editor.CaptureRevision ==
                    snapshot.SelectedCheckpoint.Record.Session.PendingRescues.Single().CaptureRevision),
                    "Offer selection made its checkpoint-carried predecessor inaccessible as fallback.");
            }
        }
        await transfer.RetireReceiverAsync();
        if (brokenJournal != null) File.WriteAllText(brokenJournal, "bad newer offered NPJ");
        await CheckInactiveAdoptionRestartAsync(transfer.ReceiverOwner, ReceiverEdit + offeredEdit + OriginalText, failedNewest);
    }

    private static async Task CheckOrdinaryRescueAfterOfferAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new CheckpointFailure { Enabled = true };
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = CreateService(owner, store);
        SessionTestDocument document = null;
        try
        {
            await service.InitializeAuthorityAsync();
            document = await SessionTestDocument.CreateAsync(owner, OriginalText);
            await PublishOfferAsync(owner, id, document, service.CurrentStamp);
            document.Editor.InsertText(0, ReceiverEdit);
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check(!(await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                    "Ordinary outgoing-offer fixture unexpectedly committed its checkpoint.");
            Check(store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries.Count == 0,
                "The ordinary rescue fixture has authoritative checkpoint membership.");
            await document.DisposeAsync(); document = null;
            await service.DisposeAsync();
            await CheckInactiveAdoptionRestartAsync(owner, ReceiverEdit + OriginalText);
        }
        finally
        {
            if (document != null) await document.DisposeAsync();
            await service.DisposeAsync(); await CleanupAsync(owner);
        }
    }

    private static async Task<TextEditorSessionDataV2> PublishOfferAsync(Guid owner, Guid id,
        SessionTestDocument document, RecoveryStamp stamp)
    {
        using var state = document.CaptureState();
        await state.Checkpoint.FlushAsync(); state.PreserveForRecovery();
        var descriptor = SessionDocumentStore.CaptureEditor(id, state, null, null);
        await TransferRecoveryStore.CreateSourceTransferAsync(Guid.NewGuid(), owner, Guid.NewGuid(),
            descriptor, null, stamp);
        return descriptor;
    }

    private static async Task CheckInactiveAdoptionRestartAsync(Guid sourceOwner, string expectedText,
        TextEditorSessionDataV2 failedNewest = null)
    {
        var owner = Guid.NewGuid();
        var service = CreateService(owner, primary: true);
        try
        {
            using (var batch = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                Check(batch.Documents.Count == 1 && batch.Documents[0].Source.ExpectedStamp.ScopeId == sourceOwner,
                    "Recovery selected duplicate or unrelated candidates for one inactive incarnation.");
                var prepared = batch.Documents[0];
                await using var restored = await SessionTestDocument.FromPreparedAsync(owner, prepared);
                Check(restored.Editor.GetText(1000) == expectedText,
                    "An older outgoing offer masked a newer checkpoint-carried or standalone rescue.");
                if (failedNewest != null)
                {
                    Check(service.RecoveryOutcome == SessionRecoveryOutcome.Partial, "Failed newer offer recovery hid its degradation.");
                    using (await SessionRecoveryTransaction.EnterAsync())
                        Check((await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner)).Pending.Any(read =>
                            read.State == RecoveryReadState.Valid && read.Record.Editor.ComputeSha256() == failedNewest.ComputeSha256()),
                            "Fallback did not durably preserve the failed newest offer before adoption.");
                }
                using var capture = restored.Capture(prepared.Id, batch.ExpectedStamp);
                await PublishRecoveredAsync(service, batch, capture);
            }
            await service.RunMaintenanceAsync(CancellationToken.None);
            await service.DisposeAsync(); service = CreateService(owner, primary: true);
            using var restarted = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
            Check(restarted.Documents.Count == 1 && restarted.Documents[0].Source.IsOwned,
                "Adoption did not consume exactly one inactive source or lost its independent target.");
            await using var native = await SessionTestDocument.FromPreparedAsync(owner, restarted.Documents[0]);
            Check(native.Editor.GetText(1000) == expectedText,
                "Adoption, maintenance and restart lost the newest selected rescue content.");
            if (failedNewest != null)
                using (await SessionRecoveryTransaction.EnterAsync())
                    Check((await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner)).Pending.Any(read =>
                        read.State == RecoveryReadState.Valid && read.Record.Editor.ComputeSha256() == failedNewest.ComputeSha256()),
                        "Adoption or maintenance discarded the failed newest offer's Pending evidence.");
        }
        finally { await service.DisposeAsync(); await CleanupAsync(owner); }
    }

    private static async Task CheckUnreadableRescueAsync(bool locked)
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new CheckpointFailure { Enabled = true };
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = CreateService(owner, store);
        FileStream firstLock = null, secondLock = null;
        try
        {
            await service.InitializeAuthorityAsync();
            var stamp = service.CurrentStamp;
            await using var original = await SessionTestDocument.CreateAsync(owner, OriginalText);
            using (var capture = original.Capture(id, stamp))
                Check(!(await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Rescue-only fixture unexpectedly committed its checkpoint.");
            faults.Enabled = false;
            var rescue = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, id)).Entries.Single();
            var a = store.Paths.CopyPath(rescue, "a"); var b = store.Paths.CopyPath(rescue, "b");
            var aBytes = File.ReadAllBytes(a); var bBytes = File.ReadAllBytes(b);
            if (locked)
            {
                firstLock = new FileStream(a, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
                secondLock = new FileStream(b, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
            }
            else { File.WriteAllText(a, "broken rescue A"); File.WriteAllText(b, "broken rescue B"); }
            using var empty = new SessionCapture { ExpectedStamp = stamp };
            Check(!(await service.SaveAsync(empty, CancellationToken.None)).Succeeded && service.IsRecoveryBlocked,
                "The first complete checkpoint overwrote rescue-only uncertainty with an empty workspace.");
            await service.DisposeAsync(); service = CreateService(owner);
            await ExpectBlockedAsync(() => service.InitializeAuthorityAsync());
            await ExpectBlockedAsync(async () => { using var ignored = await service.DiscoverRecoveryAsync(); });
            Check(!(await service.SaveAsync(empty, CancellationToken.None)).Succeeded && service.IsRecoveryBlocked &&
                store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries.Count == 0,
                "A retry after blocked startup created an apparently authoritative empty checkpoint.");
            firstLock?.Dispose(); firstLock = null;
            secondLock?.Dispose(); secondLock = null;
            if (!locked) { File.WriteAllBytes(a, aBytes); File.WriteAllBytes(b, bBytes); }
            // Also verify that a healthy peer repairs a single corrupt rescue copy.
            File.WriteAllText(a, "one damaged peer");
            await service.InitializeAuthorityAsync();
            using var recovered = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
            Check(recovered.Documents.Count == 1 && File.ReadAllBytes(a).SequenceEqual(File.ReadAllBytes(b)),
                "Unlocking/restoring rescue evidence did not allow retry and verified mirror repair.");
            await using var restored = await SessionTestDocument.FromPreparedAsync(owner, recovered.Documents[0]);
            Check(restored.Editor.GetText(1000) == OriginalText, "Rescue retry failed to recover the original Unicode content.");
        }
        finally
        {
            firstLock?.Dispose(); secondLock?.Dispose();
            await service.DisposeAsync(); await CleanupAsync(owner);
        }
    }

    private static async Task CheckPartialRescueAsync()
    {
        var owner = Guid.NewGuid(); var firstId = Guid.NewGuid(); var secondId = Guid.NewGuid();
        var faults = new CheckpointFailure { Enabled = true };
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = CreateService(owner, store);
        try
        {
            await service.InitializeAuthorityAsync();
            await using var first = await SessionTestDocument.CreateAsync(owner, OriginalText);
            await using var second = await SessionTestDocument.CreateAsync(owner, "unreadable second rescue");
            using (var capture = CaptureDocuments(new[] { first, second }, new[] { firstId, secondId }, service.CurrentStamp))
                Check(!(await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Partial rescue fixture unexpectedly committed.");
            var failed = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, secondId)).Entries.Single();
            foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(failed, peer), "bad");
            await service.DisposeAsync(); service = CreateService(owner);
            using var batch = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
            Check(batch.Documents.Count == 1 && service.RecoveryOutcome == SessionRecoveryOutcome.Partial,
                "A healthy rescue hid unreadable sibling evidence or unreadable evidence blocked every healthy document.");
        }
        finally { await service.DisposeAsync(); await CleanupAsync(owner); }
    }

    private static async Task CheckSupersededRescueAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new CheckpointFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = CreateService(owner, store);
        try
        {
            await service.InitializeAuthorityAsync();
            await using var document = await SessionTestDocument.CreateAsync(owner, OriginalText);
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Superseded rescue fixture did not commit.");
            var oldRescue = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, id)).Entries.Single();
            foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(oldRescue, peer), "bad old rescue");
            document.Editor.InsertText(0, ReceiverEdit);
            faults.Enabled = true;
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check(!(await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Superseded rescue fixture unexpectedly committed.");
            foreach (var checkpoint in store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries)
                foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(checkpoint, peer), "bad checkpoint");
            await service.DisposeAsync(); service = CreateService(owner);
            using var batch = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None);
            Check(batch.Documents.Count == 1 && service.RecoveryOutcome == SessionRecoveryOutcome.Rescue,
                "An invalid older rescue degraded its newer verified successor in the same document directory.");
            await using var restored = await SessionTestDocument.FromPreparedAsync(owner, batch.Documents[0]);
            Check(restored.Editor.GetText(1000) == ReceiverEdit + OriginalText, "Superseded rescue recovery selected stale content.");
        }
        finally { await service.DisposeAsync(); await CleanupAsync(owner); }
    }

    private static async Task CheckResetContentBoundaryAsync(bool corruptBeforeReset)
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new CheckpointFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = CreateService(owner, store);
        try
        {
            await service.InitializeAuthorityAsync();
            await using var document = await SessionTestDocument.CreateAsync(owner, OriginalText);
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Reset boundary fixture did not commit.");
            var rescueArea = store.GetScopeArea(owner, RecoveryAreaKind.Rescue, id);
            var oldRescue = store.Enumerate(rescueArea).Entries.Single();
            var oldRescueRecord = (await store.ReadAsync(oldRescue)).Record;
            var oldCheckpoint = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries.Single();
            if (corruptBeforeReset)
                foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(oldRescue, peer), "bad before reset");
            await service.ClearSessionDataAsync([]);
            var newEpoch = service.CurrentStamp.EpochId;
            foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(oldCheckpoint, peer), "bad retired checkpoint");
            if (!corruptBeforeReset)
            {
                oldRescueRecord.Editor.EditingFileName = "conflicting retired rescue";
                File.WriteAllBytes(store.Paths.CopyPath(oldRescue, "b"), RecoveryRecordCodec.Encode(oldRescueRecord, oldRescue).EnvelopeBytes);
                Check((await store.ReadAsync(oldRescue)).State == RecoveryReadState.Conflict, "Retired conflict fixture has matching mirrors.");
            }
            await service.DisposeAsync(); service = CreateService(owner, store);
            await service.InitializeAuthorityAsync();
            using (var empty = await service.DiscoverRecoveryAsync())
                Check(empty.Sources.Count == 0 && service.CurrentStamp.EpochId == newEpoch && !service.IsRecoveryBlocked,
                    "Damage in content observed by reset prevented an empty new epoch from reopening.");
            using (await SessionRecoveryTransaction.EnterAsync())
                Check((await SessionRecoveryCatalog.ReadReferencesAsync()).BlockedOwners.Contains(owner),
                    "Reset retirement of automatic candidates weakened GC protection for unreadable content.");
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                    "The first new-epoch save remained blocked by retired content damage.");

            // A second durable reset retires both earlier history and its own epoch's roots.
            await service.ClearSessionDataAsync([]);
            newEpoch = service.CurrentStamp.EpochId;
            var retiredRoots = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries
                .Concat(store.Enumerate(rescueArea).Entries).ToArray();
            foreach (var retired in retiredRoots)
                foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(retired, peer), "bad repeatedly retired content");
            await service.DisposeAsync(); service = CreateService(owner, store);
            await service.InitializeAuthorityAsync();
            Check(service.CurrentStamp.EpochId == newEpoch && !service.IsRecoveryBlocked,
                "Repeated resets forgot the retired content boundary.");

            // A later operation is not covered by the reset, even in the same rescue directory.
            document.Editor.InsertText(0, ReceiverEdit);
            faults.Enabled = true;
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check(!(await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "New-epoch rescue fixture unexpectedly committed.");
            faults.Enabled = false;
            var current = store.Enumerate(rescueArea).Entries.Single(address => !retiredRoots.Contains(address));
            var currentA = store.Paths.CopyPath(current, "a"); var currentB = store.Paths.CopyPath(current, "b");
            var bytes = File.ReadAllBytes(currentA);
            File.WriteAllText(currentA, "bad new-epoch A"); File.WriteAllText(currentB, "bad new-epoch B");
            await service.DisposeAsync(); service = CreateService(owner, store);
            await ExpectBlockedAsync(() => service.InitializeAuthorityAsync());
            using (var empty = new SessionCapture { ExpectedStamp = new RecoveryStamp { ScopeId = owner, EpochId = newEpoch } })
                Check(!(await service.SaveAsync(empty, CancellationToken.None)).Succeeded && service.IsRecoveryBlocked,
                    "Reset suppression incorrectly covered corrupt content first published in its new epoch.");
            File.WriteAllBytes(currentA, bytes); File.WriteAllBytes(currentB, bytes);
            await service.InitializeAuthorityAsync();
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded,
                    "Verified new-epoch rescue content could not be checkpointed after repair.");
        }
        finally { await service.DisposeAsync(); await CleanupAsync(owner); }
    }

    private static async Task CheckFailedResetDoesNotRetireAsync()
    {
        var owner = Guid.NewGuid(); var id = Guid.NewGuid();
        var faults = new CheckpointFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = CreateService(owner, store);
        try
        {
            await service.InitializeAuthorityAsync();
            var oldEpoch = service.CurrentStamp.EpochId;
            await using var document = await SessionTestDocument.CreateAsync(owner, OriginalText);
            using (var capture = document.Capture(id, service.CurrentStamp))
                Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Failed reset fixture did not commit.");
            faults.FailResetIntent = true;
            var failed = false;
            try { await service.ClearSessionDataAsync([]); }
            catch (IOException) { failed = true; }
            Check(failed && service.CurrentStamp.EpochId == oldEpoch, "An uncommitted reset intent changed authority.");
            faults.FailResetIntent = false;
            var oldContent = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries
                .Concat(store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, id)).Entries);
            foreach (var address in oldContent)
                foreach (var peer in new[] { "a", "b" }) File.WriteAllText(store.Paths.CopyPath(address, peer), "unretired damaged content");
            await service.DisposeAsync(); service = CreateService(owner);
            await ExpectBlockedAsync(() => service.InitializeAuthorityAsync());
        }
        finally { await service.DisposeAsync(); await CleanupAsync(owner); }
    }

    private static async Task CheckDurableFallbackAsync(bool receipt)
    {
        await using var transfer = await TransferFixture.CreateAsync();
        var owner = transfer.ReceiverOwner; var id = transfer.ReceiverId;
        var service = transfer.ReceiverService;
        var store = RecoveryRootStore.CreateStore();
        TextEditorSessionDataV2 durable;
        using (await SessionRecoveryTransaction.EnterAsync())
        {
            var root = await TransferRootReader.ReadReceiptAsync(transfer.Token);
            durable = SessionRecoveryCatalog.CloneEditor(root.Record.Editor);
            if (!receipt)
            {
                var area = store.GetScopeArea(owner, RecoveryAreaKind.Decisions);
                RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.AdoptLegacy,
                    Stamp = store.CreatePublicationStamp(area, service.CurrentStamp), Editor = durable,
                    Consumptions = [new RecoveryConsumption
                    { Kind = RecoveryConsumptionKind.Legacy, LegacyFingerprint = new string('a', 64), SourceEditorId = id }]
                }));
                Check(await store.DeleteAsync(root.Address), "The adoption fallback fixture retained its receipt and could pass through the wrong root kind.");
            }
        }
        using (var initial = transfer.CaptureReceipt())
            Check((await service.SaveAsync(initial, CancellationToken.None)).Succeeded, "Initial durable history did not commit.");
        string brokenJournal;
        TextEditorSessionDataV2 newest;
        await using (var newer = await SessionTestDocument.CreateAsync(owner, "later generation that will be damaged\r"))
        {
            for (var index = 0; index < 4; index++)
            {
                newer.Editor.InsertText(0, index + "\r");
                using var capture = newer.Capture(id, service.CurrentStamp);
                Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Later fallback history did not commit.");
                await service.RunMaintenanceAsync(CancellationToken.None);
            }
            brokenJournal = newer.Journal.File.Path;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                newest = snapshot.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor;
                Check(snapshot.Checkpoints.Count == 3 && snapshot.Rescues.Count == 3 &&
                    snapshot.Checkpoints.SelectMany(read => read.Record.Session.TextEditors)
                        .Concat(snapshot.Rescues.Select(read => read.Record.Editor))
                        .All(editor => editor.Journal.GenerationId != durable.Journal.GenerationId),
                    "Ordinary fallback history still contained the durable root's older healthy generation.");
                CheckFallbackLifecycleBoundaries(snapshot, id, durable);
            }
        }
        await transfer.RetireReceiverAsync();
        File.WriteAllText(brokenJournal, "damaged NPJ");
        service = CreateService(owner);
        try
        {
            using (var batch = await MaterializeAsync(service, new DocumentLoadOptions(), CancellationToken.None))
            {
                Check(batch.Documents.Count == 1 && service.RecoveryOutcome == SessionRecoveryOutcome.Partial,
                    "A retained durable descriptor was not attempted after every ordinary history generation failed.");
                await using var restored = await SessionTestDocument.FromPreparedAsync(owner, batch.Documents[0]);
                Check(restored.Editor.GetText(1000) == OriginalText, "Durable fallback restored different text from its retained descriptor.");
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                    Check(snapshot.Pending.Any(read => read.State == RecoveryReadState.Valid &&
                        read.Record.Editor.ComputeSha256() == newest.ComputeSha256()),
                        "Durable fallback retired the failed newest descriptor instead of preserving Pending evidence.");
                }
            }
            Check(await service.PrepareExplicitCloseAsync(new[] { id }), "Explicit close of the recovered durable incarnation failed.");
            using (await SessionRecoveryTransaction.EnterAsync())
                Check(!(await SessionRecoveryCatalog.ReadScopeAsync(owner)).Selection.Documents.Any(row => row.Id == id),
                    "Fallback exposed descriptors after the incarnation was explicitly closed.");
            await service.ClearSessionDataAsync([]);
            using (await SessionRecoveryTransaction.EnterAsync())
                Check(!(await SessionRecoveryCatalog.ReadScopeAsync(owner)).Selection.Documents.Any(row => row.Id == id),
                    "Fallback crossed the completed reset boundary.");
        }
        finally { await service.DisposeAsync(); }
    }

    private static void CheckFallbackLifecycleBoundaries(RecoveryScopeSnapshot source, Guid id, TextEditorSessionDataV2 durable)
    {
        RecoveryScopeSnapshot Copy()
        {
            var copy = new RecoveryScopeSnapshot { Stamp = SessionRecoveryAuthority.CopyStamp(source.Stamp) };
            copy.Checkpoints.AddRange(source.Checkpoints); copy.Rescues.AddRange(source.Rescues);
            copy.Decisions.AddRange(source.Decisions); copy.GlobalRecords.AddRange(source.GlobalRecords);
            return copy;
        }
        var cutoff = source.Selection.Documents.Single(row => row.Id == id).ConsumptionCutoff;
        var address = new RecoveryAddress(new RecoveryArea(RecoveryAreaKind.Decisions, Guid.NewGuid()), 1, Guid.NewGuid());
        var consumed = Copy();
        consumed.GlobalRecords.Add(new RecoveryReadResult(RecoveryReadState.Valid, address, new RecoveryRecord
        {
            Kind = RecoveryRecordKind.AdoptInactive,
            Consumptions = [new RecoveryConsumption { Kind = RecoveryConsumptionKind.Inactive,
                SourceScopeId = source.Stamp.ScopeId, SourceEpochId = source.Stamp.EpochId, SourceEditorId = id, SourceCaptureRevision = cutoff }]
        }));
        Check(!SessionRecoverySelector.Select(consumed).Documents.Any(row => row.Id == id), "Fallback exposed a consumed inactive incarnation.");
        var moved = Copy();
        moved.GlobalRecords.Add(new RecoveryReadResult(RecoveryReadState.Valid, address, new RecoveryRecord
        {
            Kind = RecoveryRecordKind.SourceMoved, SourceStamp = source.Stamp, SourceEditor = durable,
            Session = new NotepadsSessionDataV2 { SourceRemovalRevision = cutoff }
        }));
        Check(!SessionRecoverySelector.Select(moved).Documents.Any(row => row.Id == id), "Fallback exposed content consumed by an exact source move.");
    }

    private static async Task ExpectBlockedAsync(Func<Task> action)
    {
        try { await action(); }
        catch (SessionDataCorruptedException) { return; }
        throw new InvalidOperationException("Unreadable rescue-only authority was treated as an empty session.");
    }

    private sealed class CheckpointFailure : IRecoveryStorageFaults
    {
        public bool Enabled { get; set; }
        public bool FailResetIntent { get; set; }
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (FailResetIntent && copy == "intent" && step == RecoveryStorageStep.BeforeTemporaryWrite)
                throw new IOException("Injected interruption before reset intent publication.");
            if (Enabled && address.Area.Kind == RecoveryAreaKind.Checkpoints && step == RecoveryStorageStep.BeforeTemporaryWrite)
                throw new IOException("Injected interruption after rescue publication and before the first checkpoint copy.");
        }
    }

    private sealed class TransferFixture : IAsyncDisposable
    {
        public Guid SourceOwner { get; } = Guid.NewGuid();
        public Guid ReceiverOwner { get; } = Guid.NewGuid();
        public Guid ReceiverId { get; } = Guid.NewGuid();
        public CheckpointFailure Faults { get; } = new();
        public SessionService SourceService { get; private set; }
        public SessionService ReceiverService { get; private set; }
        public SessionTestDocument Source { get; private set; }
        public SessionTestDocument Receiver { get; private set; }
        public TransferToken Token { get; private set; }
        private DocumentRecoveryState _receiptState;

        public static async Task<TransferFixture> CreateAsync(bool emptyCheckpoint = false)
        {
            var fixture = new TransferFixture();
            try
            {
                fixture.SourceService = CreateService(fixture.SourceOwner);
                fixture.ReceiverService = CreateService(fixture.ReceiverOwner,
                    new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, fixture.Faults));
                await fixture.SourceService.InitializeAuthorityAsync();
                await fixture.ReceiverService.InitializeAuthorityAsync();
                if (emptyCheckpoint)
                {
                    using var empty = new SessionCapture { ExpectedStamp = fixture.ReceiverService.CurrentStamp };
                    Check((await fixture.ReceiverService.SaveAsync(empty, CancellationToken.None)).Succeeded, "Prior empty checkpoint did not commit.");
                }
                fixture.Source = await SessionTestDocument.CreateAsync(fixture.SourceOwner, OriginalText);
                using var sourceState = fixture.Source.CaptureState();
                await sourceState.Checkpoint.FlushAsync(); sourceState.PreserveForRecovery();
                var descriptor = SessionDocumentStore.CaptureEditor(Guid.NewGuid(), sourceState, null, null);
                fixture.Token = await TransferRecoveryStore.CreateSourceTransferAsync(Guid.NewGuid(), fixture.SourceOwner,
                    Guid.NewGuid(), descriptor, null, fixture.SourceService.CurrentStamp);
                IDisposable pin;
                using (await SessionRecoveryTransaction.EnterAsync()) pin = SessionScopeLease.AcquireReader(fixture.SourceOwner);
                using var sourcePin = pin;
                var decision = new RecoveryDocumentDecision(RecoveryMembership.Owned, new(descriptor),
                    Array.Empty<RecoveryDescriptor>(), Array.Empty<RecoveryDescriptor>(),
                    descriptor.CaptureRevision, null, null, true);
                var source = new RecoveryDocumentSource(decision, null, false, fixture.SourceService.CurrentStamp,
                    null, new RecoverySourceLifetime(), fixture.ReceiverService.CurrentStamp);
                using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, fixture.ReceiverOwner,
                    new DocumentLoadOptions(), CancellationToken.None);
                fixture.Receiver = await SessionTestDocument.FromPreparedAsync(fixture.ReceiverOwner, prepared);
                fixture._receiptState = fixture.Receiver.CaptureState();
                await fixture._receiptState.Checkpoint.FlushAsync();
                var target = SessionDocumentStore.CaptureEditor(fixture.ReceiverId, fixture._receiptState, null, null);
                await TransferRecoveryStore.CreateReceiptAsync(fixture.Token, Guid.NewGuid(), fixture.ReceiverOwner,
                    target, fixture._receiptState, null, fixture.ReceiverService.CurrentStamp);
                return fixture;
            }
            catch { await fixture.DisposeAsync(); throw; }
        }

        public SessionCapture CaptureReceipt()
        {
            var capture = new SessionCapture { ExpectedStamp = ReceiverService.CurrentStamp, SelectedEditorId = ReceiverId };
            capture.Documents.Add(new SessionDocumentCapture { Id = ReceiverId, Recovery = _receiptState.Retain() });
            return capture;
        }

        public Task<TransferToken> CreateOutgoingOfferAsync() => TransferRecoveryStore.CreateSourceTransferAsync(
            Guid.NewGuid(), ReceiverOwner, Guid.NewGuid(),
            SessionDocumentStore.CaptureEditor(ReceiverId, _receiptState, null, null), null, ReceiverService.CurrentStamp);

        public async Task RetireReceiverAsync()
        {
            _receiptState?.Dispose(); _receiptState = null;
            if (Receiver != null) { await Receiver.DisposeAsync(); Receiver = null; }
            if (ReceiverService != null) { await ReceiverService.DisposeAsync(); ReceiverService = null; }
        }

        public async ValueTask DisposeAsync()
        {
            await RetireReceiverAsync();
            if (Source != null) { await Source.DisposeAsync(); Source = null; }
            if (SourceService != null) { await SourceService.DisposeAsync(); SourceService = null; }
            await CleanupAsync(SourceOwner, ReceiverOwner);
        }
    }
}
