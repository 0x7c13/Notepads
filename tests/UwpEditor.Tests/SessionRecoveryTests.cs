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
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Features.Sessions.Validation;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static class SessionRecoveryTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        await CheckTransferCloseOrderingAsync(closeBeforeAck: true);
        await CheckTransferCloseOrderingAsync(closeBeforeAck: false);
        await CheckInactiveConsumptionAsync();
        log.AppendLine("PASS: streamed independent transfer journal forks, receipt/ACK target binding, receiver close before/after ACK, exact stale-source suppression, epoch rejection and inactive incarnation consumption across target reset.");
    }

    private static async Task CheckTransferCloseOrderingAsync(bool closeBeforeAck)
    {
        var sourceOwner = Guid.NewGuid(); var targetOwner = Guid.NewGuid();
        var sourceId = Guid.NewGuid(); var targetId = Guid.NewGuid();
        var sourceService = SessionTestProtocol.CreateService(sourceOwner);
        var targetService = SessionTestProtocol.CreateService(targetOwner);
        try
        {
            await sourceService.EnsureMetadataRetainedAsync(CancellationToken.None);
            await targetService.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var source = await SessionTestDocument.CreateAsync(sourceOwner, "saved العربية😀\0");
            source.Editor.GotoPos(source.Editor.Length); source.Editor.PasteText("\rchanged");
            using var sourceCapture = source.Capture(sourceId, sourceService.CurrentStamp);
            SessionTestProtocol.Check((await sourceService.SaveAsync(sourceCapture, CancellationToken.None)).Succeeded,
                "Source checkpoint did not commit.");
            // A metadata-only change precedes the move. Older checkpoints
            // at the same native sequence must still be consumed by its cutoff.
            source.Metadata.RequestedEncoding = "UTF-16 LE BOM";
            DocumentRecoveryState.AdvanceCaptureRevisionFloor(sourceCapture.Documents[0].Recovery.CaptureRevision + 1000);
            using var sourceState = source.CaptureState();
            SessionTestProtocol.Check(sourceState.CaptureRevision > sourceCapture.Documents[0].Recovery.CaptureRevision + 1000,
                "A resumed process capture counter did not advance above durable revisions.");
            var sourceData = SessionDocumentStore.CaptureEditor(sourceId, sourceState, null, null);
            sourceState.PreserveForRecovery();
            var token = await TransferRecoveryStore.CreateSourceTransferAsync(Guid.NewGuid(), sourceOwner,
                Guid.NewGuid(), sourceData, null, sourceCapture.ExpectedStamp);
            TextEditorSessionDataV2 offered;
            IDisposable pin;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                pin = SessionScopeLease.AcquireReader(sourceOwner);
                offered = await TransferRootReader.ReadSourceTransferAsync(token);
            }
            using (pin)
            await using (var target = await SessionTestDocument.CreateAsync(targetOwner, "initial placeholder"))
            {
                await target.Editor.StopJournalAsync();
                target.Baseline.Dispose(); target.Saved.Dispose(); target.Journal.Dispose();
                // Read precisely the committed source prefix, then create a
                // separately owned journal before publishing target authority.
                using var saved = await SessionDocumentStore.ForkBaselineAsync(sourceState.SavedSnapshot.Baseline, targetOwner);
                using var baseline = await SessionDocumentStore.ForkBaselineAsync(sourceState.RecoveryBaseline, targetOwner);
                using var input = await baseline.OpenReadStreamAsync();
                using var adapter = input.AsInputStream();
                await target.Editor.RestoreUtf8Async(adapter, (ulong)baseline.ByteLength, sourceState.Checkpoint);
                using var journal = await Notepads.Features.Documents.Storage.DocumentJournal.CreateAsync(targetOwner);
                await target.Editor.StartJournalFromCheckpointAsync(journal.File.Path, sourceState.Checkpoint);
                using var snapshot = new DocumentSnapshot(saved, new UTF8Encoding(false), LineEnding.Crlf);
                using var checkpoint = target.Editor.AcquireJournalCheckpoint();
                using var targetState = new DocumentRecoveryState(snapshot, baseline, journal, checkpoint,
                    sourceState.MetaData, true, target.Editor.Length);
                await targetState.Checkpoint.FlushAsync();
                var targetData = SessionDocumentStore.CaptureEditor(targetId, targetState, null, null);
                SessionTestProtocol.Check(targetData.Journal.OwnerId == targetOwner &&
                    targetData.Journal.FileName != offered.Journal.FileName && targetData.SavedBaseline.FileName != offered.SavedBaseline.FileName,
                    "Target reused source assets rather than streaming an independent fork.");
                await TransferRecoveryStore.CreateReceiptAsync(token, Guid.NewGuid(), targetOwner,
                    targetData, targetState, null, targetService.CurrentStamp);
                SessionTestProtocol.Check(await TransferRecoveryStore.HasReceiptAsync(token), "The target receipt was not verifiable.");
                if (closeBeforeAck)
                {
                    SessionTestProtocol.Check(await targetService.PrepareExplicitCloseAsync(new[] { targetId }), "Receiver close did not persist.");
                    await AssertThrowsAsync<InvalidDataException>(() => TransferRecoveryStore.MarkSourceAcknowledgedAsync(token, sourceData.CaptureRevision));
                    SessionTestProtocol.Check(!await TransferRecoveryStore.HasReceiptAsync(token), "Closed receiver still authorized source removal.");
                }
                else
                {
                    await TransferRecoveryStore.MarkSourceAcknowledgedAsync(token, sourceData.CaptureRevision);
                    await TransferRecoveryStore.MarkSourceAcknowledgedAsync(token, sourceData.CaptureRevision);
                    SessionTestProtocol.Check(await targetService.PrepareExplicitCloseAsync(new[] { targetId }), "Acknowledged receiver close did not persist.");
                }
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var sourceAfter = await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner);
                    var targetAfter = await SessionRecoveryCatalog.ReadScopeAsync(targetOwner);
                    SessionTestProtocol.Check(sourceAfter.Session.TextEditors.Any(editor => editor.Id == sourceId) == closeBeforeAck,
                        "Receiver close reversed an ACK or an unacknowledged close discarded the source.");
                    SessionTestProtocol.Check(targetAfter.Session.TextEditors.All(editor => editor.Id != targetId),
                        "A receipt resurrected its explicitly closed target.");
                    var references = await SessionRecoveryCatalog.ReadReferencesAsync();
                    SessionTestProtocol.Check(!references.JournalFileNames.Contains(targetData.Journal.FileName),
                        "A durable receiver close kept its full target assets rooted forever.");
                    SessionTestProtocol.Check(references.JournalFileNames.Contains(sourceData.Journal.FileName) == closeBeforeAck,
                        "ACK did not release old source content or an unacknowledged close lost its source roots.");
                    if (!closeBeforeAck)
                    {
                        var moved = await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.SourceMoved);
                        var receipt = await TransferRootReader.ReadReceiptAsync(token);
                        SessionTestProtocol.Check(moved.Record.TargetReceiptOperationId == receipt.Address.OperationId &&
                            moved.Record.TargetReceiptSha256 == receipt.PayloadSha256 && moved.Record.Editor.Id == targetId,
                            "ACK did not retain and bind the exact target recovery content.");
                    }
                }
                await target.Editor.StopJournalAsync();
            }
            if (!closeBeforeAck)
            {
                source.Editor.GotoPos(source.Editor.Length); source.Editor.PasteText("\rlater edit");
                using var later = source.Capture(sourceId, sourceService.CurrentStamp);
                SessionTestProtocol.Check((await sourceService.SaveAsync(later, CancellationToken.None)).Succeeded, "Later source capture did not commit.");
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    SessionTestProtocol.Check((await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner)).Session.TextEditors.Any(editor => editor.Id == sourceId),
                        "An old move consumed edits made after its frozen source cutoff.");
                }
            }
            var stale = sourceService.CurrentStamp;
            await sourceService.ClearSessionDataAsync();
            await AssertThrowsAsync<InvalidDataException>(() => TransferRecoveryStore.CreateSourceTransferAsync(Guid.NewGuid(),
                sourceOwner, Guid.NewGuid(), sourceData, null, stale));
        }
        finally
        {
            await sourceService.DisposeAsync(); await targetService.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(sourceOwner, targetOwner);
        }
    }

    private static async Task CheckInactiveConsumptionAsync()
    {
        var sourceOwner = Guid.NewGuid(); var targetOwner = Guid.NewGuid();
        var sourceId = Guid.NewGuid(); var targetId = Guid.NewGuid();
        var sourceService = SessionTestProtocol.CreateService(sourceOwner);
        var targetService = SessionTestProtocol.CreateService(targetOwner);
        try
        {
            await sourceService.EnsureMetadataRetainedAsync(CancellationToken.None);
            await targetService.EnsureMetadataRetainedAsync(CancellationToken.None);
            await using var source = await SessionTestDocument.CreateAsync(sourceOwner, "inactive source");
            using var sourceCapture = source.Capture(sourceId, sourceService.CurrentStamp);
            SessionTestProtocol.Check((await sourceService.SaveAsync(sourceCapture, CancellationToken.None)).Succeeded, "Inactive source preparation failed.");
            var sourceStamp = sourceCapture.ExpectedStamp;
            var cutoff = sourceCapture.Documents[0].Recovery.CaptureRevision;
            await sourceService.DisposeAsync();
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                using var writer = SessionScopeLease.TryAcquireInactiveWriter(sourceOwner);
                using var reader = SessionScopeLease.AcquireReader(sourceOwner);
                SessionTestProtocol.Check(writer != null, "Inactive writer could not be proven by its OS lease.");
                var store = RecoveryRootStore.CreateStore();
                var area = store.GetScopeArea(targetOwner, RecoveryAreaKind.Decisions);
                await using var target = await SessionTestDocument.CreateAsync(targetOwner, "independent target");
                using var state = target.CaptureState();
                await state.Checkpoint.FlushAsync(); state.PreserveForRecovery();
                var descriptor = SessionDocumentStore.CaptureEditor(targetId, state, null, null);
                var adoption = new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.AdoptInactive,
                    Stamp = store.CreatePublicationStamp(area, targetService.CurrentStamp),
                    SourceStamp = sourceStamp,
                    Editor = descriptor
                };
                adoption.Consumptions.Add(new RecoveryConsumption
                {
                    Kind = RecoveryConsumptionKind.Inactive,
                    SourceScopeId = sourceOwner,
                    SourceEpochId = sourceStamp.EpochId,
                    SourceEditorId = sourceId,
                    SourceCaptureRevision = cutoff
                });
                RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, adoption));
                SessionTestProtocol.Check((await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner)).Session.TextEditors.Count == 0,
                    "Inactive consumption did not suppress its older checkpoint incarnation.");
            }
            await targetService.ClearSessionDataAsync();
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                SessionTestProtocol.Check((await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner)).Session.TextEditors.Count == 0,
                    "Target reset revoked source consumption and resurrected the inactive source.");
            }
        }
        finally
        {
            await sourceService.DisposeAsync(); await targetService.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(sourceOwner, targetOwner);
        }
    }

    private static async Task AssertThrowsAsync<T>(Func<Task> action) where T : Exception
    {
        try { await action(); }
        catch (T) { return; }
        throw new InvalidOperationException("Expected " + typeof(T).Name + " was not reported.");
    }
}
