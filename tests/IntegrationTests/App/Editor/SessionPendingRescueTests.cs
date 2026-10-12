// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static class SessionPendingRescueTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        await CheckIndeterminateOwnAdoptionAsync();
        await CheckDurabilityAsync(reset: false);
        await CheckDurabilityAsync(reset: true);
        await CheckDurabilityAsync(reset: false, archive: true);
        await CheckRejectedEvidenceAsync();
        await CheckInactivePendingAsync();
        log.AppendLine("PASS: pending rescues survive restart/pruning, reject invalid evidence and adopt once.");
    }

    private static async Task CheckIndeterminateOwnAdoptionAsync()
    {
        var owner = Guid.NewGuid(); var newId = Guid.NewGuid(); var adoptedId = Guid.Empty;
        var faults = new AdoptionAcknowledgementFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = SessionTestProtocol.CreateService(owner, store);
        const string rescuedText = "adoption العربية😀\r日本語";
        const string newText = "new work Україна😀";
        try
        {
            await service.InitializeAuthorityAsync();
            var rescue = await CreateRescueAsync(service, rescuedText);
            using (var plan = await service.DiscoverRecoveryAsync())
            {
                var source = plan.Sources.Single();
                adoptedId = source.Id;
                using (var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, owner, new DocumentLoadOptions(), CancellationToken.None))
                await using (var native = await SessionTestDocument.FromPreparedAsync(owner, prepared))
                using (var capture = native.Capture(source.Id, service.CurrentStamp))
                {
                    capture.Documents[0].Source = source;
                    var failed = false;
                    faults.Enabled = true;
                    try { await service.PublishAdoptionAsync(capture.Documents[0], capture.ExpectedStamp); }
                    catch (IOException) { failed = true; }
                    finally { faults.Enabled = false; }
                    Check(failed && faults.FailedAddress != null &&
                        File.Exists(store.Paths.CopyPath(faults.FailedAddress, "a")) &&
                        !File.Exists(store.Paths.CopyPath(faults.FailedAddress, "b")),
                        "The adoption fixture did not lose acknowledgement after durably publishing only mirror A.");
                }
                // The controller discarded its unattached native candidate, leaving the failed import queued.
                await using var newDocument = await SessionTestDocument.CreateAsync(owner, newText);
                using var next = newDocument.Capture(newId, service.CurrentStamp);
                next.PendingRescues.Add(Forge(source, rescue, targetId: Guid.NewGuid()));
                Check(!(await service.SaveAsync(next, CancellationToken.None)).Succeeded,
                    "Adoption reconciliation accepted a different target identity.");
                next.PendingRescues.Clear();
                next.PendingRescues.Add(Forge(source, rescue, cutoff: source.Origin.ConsumptionCutoff + 1));
                Check(!(await service.SaveAsync(next, CancellationToken.None)).Succeeded,
                    "Adoption reconciliation accepted a different source consumption cutoff.");
                next.PendingRescues.Clear();
                next.PendingRescues.Add(source);
                Check((await service.SaveAsync(next, CancellationToken.None)).Succeeded,
                    "A durably adopted own rescue with indeterminate acknowledgement permanently blocked a later ordinary backup.");
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                    var adoption = snapshot.Decisions.Single(read => read.Record.Kind == RecoveryRecordKind.AdoptInactive);
                    Check(adoption.Address == faults.FailedAddress && adoption.Record.Editor.Id == adoptedId &&
                        snapshot.SelectedCheckpoint.Record.Session.PendingRescues.Count == 0 &&
                        snapshot.SelectedCheckpoint.Record.Session.TextEditors.Single().Id == newId &&
                        snapshot.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Owned) == 2 &&
                        snapshot.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Id).ToHashSet().SetEquals(new[] { adoptedId, newId }),
                        "The later checkpoint retained a consumed rescue, duplicated adoption, or omitted durable native content.");
                }
            }
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using var restart = await service.DiscoverRecoveryAsync();
            Check(restart.Sources.Count == 2 && restart.Sources.All(source => source.IsOwned) &&
                restart.Sources.Select(source => source.Id).ToHashSet().SetEquals(new[] { adoptedId, newId }),
                "Restart duplicated a consumed rescue or lost the adopted document or later ordinary tab.");
            foreach (var source in restart.Sources)
            {
                using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, owner, new DocumentLoadOptions(), CancellationToken.None);
                await using var native = await SessionTestDocument.FromPreparedAsync(owner, prepared);
                Check(native.Editor.GetText(200) == (source.Id == adoptedId ? rescuedText : newText),
                    "Restart changed Unicode content after reconciling indeterminate adoption.");
            }
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckDurabilityAsync(bool reset, bool archive = false)
    {
        var owner = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var store = RecoveryRootStore.CreateStore();
        const string text = "pending العربية😀\r日本語";
        try
        {
            await service.InitializeAuthorityAsync();
            var rescue = await CreateRescueAsync(service, text);
            var original = rescue.Record.Editor;
            using (var plan = await service.DiscoverRecoveryAsync())
            {
                var source = plan.Sources.Single();
                Check(source.IsOwnScopeRescue && source.Id != original.Id, "Own rescue discovery lost its original/import identity distinction.");
                Check((await SavePendingAsync(service, source)).Succeeded, "A pending-only capture failed.");
            }
            // No document handles or discovery pins remain: only the checkpoint can protect the source.
            using (await SessionRecoveryTransaction.EnterAsync())
                Check(await store.DeleteAsync(rescue.Address), "The original rescue metadata could not be removed.");
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            await service.InitializeAuthorityAsync();
            var orphan = Path.Combine(ApplicationData.Current.LocalFolder.Path, "DocumentBaselines", $"{owner:N}-{Guid.NewGuid():N}.utf8");
            File.WriteAllText(orphan, "unreferenced GC sentinel");
            await service.RunMaintenanceAsync(CancellationToken.None);
            Check(!File.Exists(orphan), "The pending-asset durability fixture did not actually run garbage collection.");
            for (var iteration = 0; iteration < 4; iteration++)
            {
                using (var plan = await service.DiscoverRecoveryAsync())
                {
                    var source = plan.Sources.Single();
                    Check(source.IsOwnScopeRescue && source.Origin.RootAddress.Area.Kind == RecoveryAreaKind.Checkpoints &&
                        source.Origin.SourceEditor.ComputeSha256() == original.ComputeSha256() &&
                        source.Descriptor.StateMetaData.RequiresSaveAs && source.EditingFileFutureAccessToken == null,
                        "A checkpoint-carried rescue lost source evidence or acquired original-file authority.");
                    Check((await SavePendingAsync(service, source, iteration + 1)).Succeeded, "Repeated pending publication failed.");
                }
                await service.RunMaintenanceAsync(CancellationToken.None);
            }
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                var references = await SessionRecoveryCatalog.ReadReferencesAsync();
                Check(snapshot.Checkpoints.Count == 3 && snapshot.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Owned) == 0 &&
                    snapshot.SelectedCheckpoint.Record.Session.PendingRescues.Single().ComputeSha256() == original.ComputeSha256() &&
                    snapshot.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Rescue) == 1 && snapshot.Outcome == SessionRecoveryOutcome.Rescue &&
                    snapshot.CurrentDescriptors(original.Id).Any(row => row.ComputeSha256() == original.ComputeSha256()) &&
                    snapshot.Selection.Documents.Single(row => row.Id == original.Id).Fallbacks.Select(row => row.Descriptor).All(editor => editor.ComputeSha256() != original.ComputeSha256()) &&
                    references.BaselineFileNames.Contains(original.RecoveryBaseline.FileName) &&
                    references.JournalFileNames.Contains(original.Journal.FileName),
                    "Pruning or reference collection lost checkpoint-carried rescue evidence.");
            }
            Check(File.Exists(Path.Combine(ApplicationData.Current.LocalFolder.Path, "DocumentBaselines", original.RecoveryBaseline.FileName)) &&
                File.Exists(Path.Combine(ApplicationData.Current.LocalFolder.Path, "DocumentJournals", original.Journal.FileName)),
                "Maintenance collected the pending rescue payload.");
            Guid adoptedId = Guid.Empty;
            using (var plan = await service.DiscoverRecoveryAsync())
            {
                var source = plan.Sources.Single();
                if (reset || archive)
                {
                    if (reset) await service.ClearSessionDataAsync(plan.Sources);
                    else
                    {
                        await service.ArchiveRecoverySourcesAsync(plan.Sources);
                        using var empty = new SessionCapture { ExpectedStamp = service.CurrentStamp };
                        Check((await service.SaveAsync(empty, CancellationToken.None)).Succeeded, "Skipping an archived rescue failed to publish an empty workspace.");
                        using (await SessionRecoveryTransaction.EnterAsync())
                        {
                            var newest = (await SessionRecoveryCatalog.ReadScopeAsync(owner)).SelectedCheckpoint;
                            Check(await store.DeleteAsync(newest.Address), "The newest empty checkpoint could not be removed for rollback.");
                        }
                    }
                    Check(!(await SavePendingAsync(service, source)).Succeeded, "A reset or archived pending source was republished.");
                }
                else
                {
                    using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, owner, new DocumentLoadOptions(), CancellationToken.None);
                    await using var native = await SessionTestDocument.FromPreparedAsync(owner, prepared);
                    Check(native.Editor.GetText(200) == text, "Checkpoint-only pending rescue changed its Unicode payload.");
                    using var capture = native.Capture(source.Id, service.CurrentStamp);
                    capture.Documents[0].Source = source;
                    adoptedId = (await service.PublishAdoptionAsync(capture.Documents[0], capture.ExpectedStamp)).Id;
                    // Crash boundary: deliberately leave the old checkpoint's pending list unchanged.
                }
            }
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            using var restart = await service.DiscoverRecoveryAsync();
            Check(reset || archive ? restart.Sources.Count == 0 : restart.Sources.Count == 1 &&
                restart.Sources[0].IsOwned && restart.Sources[0].Id == adoptedId,
                "Reset/archive resurrected a pending rescue on rollback or committed adoption imported it twice.");
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var references = await SessionRecoveryCatalog.ReadReferencesAsync();
                if (reset || archive) Check(references.JournalFileNames.Contains(original.Journal.FileName),
                    "Reset/archive discarded the unconsumed rescue payload.");
                else Check((await SessionRecoveryCatalog.ReadScopeAsync(owner)).SelectedCheckpoint.Record.Session.PendingRescues.Count == 1,
                    "The adoption test accidentally cleaned up its pending checkpoint.");
            }
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckRejectedEvidenceAsync()
    {
        var owner = Guid.NewGuid();
        var service = SessionTestProtocol.CreateService(owner);
        var store = RecoveryRootStore.CreateStore();
        try
        {
            await service.InitializeAuthorityAsync();
            var rescue = await CreateRescueAsync(service, "original");
            var unrelated = await CreateRescueAsync(service, "unrelated");
            using var plan = await service.DiscoverRecoveryAsync();
            var source = plan.Sources.Single(row => row.Origin.SourceEditor.Id == rescue.Record.Editor.Id);
            CheckSchema(rescue.Record.Editor, service.CurrentStamp);
            RecoveryReadResult pending, archive, ordinary;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                pending = await PublishAsync(store, store.GetScopeArea(owner, RecoveryAreaKind.Pending, rescue.Record.Editor.Id),
                    new RecoveryRecord { Kind = RecoveryRecordKind.Pending, Editor = rescue.Record.Editor }, service.CurrentStamp);
                archive = await PublishAsync(store, store.GetGlobalArea(RecoveryAreaKind.Archives, Guid.NewGuid()),
                    new RecoveryRecord { Kind = RecoveryRecordKind.Archive, Session = Session(owner, rescue.Record.Editor, pending: false) }, service.CurrentStamp);
                ordinary = await PublishAsync(store, store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints),
                    new RecoveryRecord { Kind = RecoveryRecordKind.Checkpoint, Session = Session(owner, rescue.Record.Editor, pending: false) }, service.CurrentStamp);
                Check((await SessionRecoveryCatalog.ReadScopeAsync(owner)).Selection.Documents.Count(row => row.Membership == RecoveryMembership.Rescue) == 0,
                    "An ordinary valid checkpoint resurrected unrelated rescue history.");
            }
            foreach (var root in new[] { unrelated, pending, archive, ordinary })
                Check(!(await SavePendingAsync(service, Forge(source, root))).Succeeded,
                    "An unrelated, manual-only, or ordinary workspace root admitted a pending rescue.");
            Check(!(await SavePendingAsync(service, Forge(source, rescue, hash: new string('0', 64)))).Succeeded,
                "A pending source with the wrong exact-root hash was accepted.");
            var foreign = SessionRecoveryAuthority.CopyStamp(service.CurrentStamp);
            foreign.ScopeId = foreign.EpochId = Guid.NewGuid();
            Check(!(await SavePendingAsync(service, Forge(source, rescue, stamp: foreign))).Succeeded,
                "A foreign pending source was accepted.");
            var obsolete = SessionRecoveryAuthority.CopyStamp(service.CurrentStamp);
            obsolete.EpochId = Guid.NewGuid();
            Check(!(await SavePendingAsync(service, Forge(source, rescue, stamp: obsolete))).Succeeded,
                "A pending source with an obsolete epoch was accepted.");
            Check(!(await SavePendingAsync(service, Forge(source, rescue, disposed: true))).Succeeded,
                "A pending source outlived its reader-pin lifetime.");
            using (await SessionRecoveryTransaction.EnterAsync())
                Check(store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries.Count == 1,
                    "Rejected evidence published a replacement checkpoint.");
            Check((await SavePendingAsync(service, source)).Succeeded, "The valid frozen rescue failed after rejecting forged evidence.");
            using (await SessionRecoveryTransaction.EnterAsync())
                Check(await store.DeleteAsync(rescue.Address), "The exact source root could not be removed for provenance revalidation.");
            Check(!(await SavePendingAsync(service, source)).Succeeded,
                "The unchanged JSON shortcut skipped revalidation of a missing exact rescue root.");
            using (await SessionRecoveryTransaction.EnterAsync())
                Check(store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Checkpoints)).Entries.Count == 2,
                    "A pending save with vanished provenance published another checkpoint.");
        }
        finally { await service.DisposeAsync(); await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task CheckInactivePendingAsync()
    {
        var sourceOwner = Guid.NewGuid(); var targetOwner = Guid.NewGuid();
        var sourceService = SessionTestProtocol.CreateService(sourceOwner);
        var targetService = SessionTestProtocol.CreateService(targetOwner, primary: true);
        try
        {
            await sourceService.InitializeAuthorityAsync();
            var rescue = await CreateRescueAsync(sourceService, "inactive pending 日本語");
            using (var plan = await sourceService.DiscoverRecoveryAsync())
                Check((await SavePendingAsync(sourceService, plan.Sources.Single())).Succeeded, "Inactive pending fixture did not commit.");
            using (await SessionRecoveryTransaction.EnterAsync())
                Check(await RecoveryRootStore.CreateStore().DeleteAsync(rescue.Address), "Inactive original rescue metadata could not be removed.");
            await sourceService.DisposeAsync();
            Guid adoptedId;
            using (var plan = await targetService.DiscoverRecoveryAsync())
            {
                var source = plan.Sources.Single(row => row.Origin?.SourceStamp?.ScopeId == sourceOwner);
                Check(!source.IsOwned && !source.IsOwnScopeRescue && source.Id != rescue.Record.Editor.Id &&
                    source.Origin.RootAddress.Area.Kind == RecoveryAreaKind.Checkpoints && source.Metadata.RequiresSaveAs,
                    "Primary discovery lost a retired scope's checkpoint-only pending rescue.");
                using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, targetOwner, new DocumentLoadOptions(), CancellationToken.None);
                await using var native = await SessionTestDocument.FromPreparedAsync(targetOwner, prepared);
                Check(native.Editor.GetText(200) == "inactive pending 日本語", "Inactive pending adoption changed its payload.");
                using var capture = native.Capture(source.Id, targetService.CurrentStamp);
                capture.Documents[0].Source = source;
                adoptedId = (await targetService.PublishAdoptionAsync(capture.Documents[0], capture.ExpectedStamp)).Id;
            }
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var source = await SessionRecoveryCatalog.ReadScopeAsync(sourceOwner);
                var target = await SessionRecoveryCatalog.ReadScopeAsync(targetOwner);
                Check(source.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Rescue) == 0 && source.Selection.Documents.Count(row => row.Membership == RecoveryMembership.Owned) == 0 &&
                    target.Selection.Documents.Single(row => row.Membership == RecoveryMembership.Owned).Descriptor.Id == adoptedId,
                    "Inactive pending adoption failed to consume its source or retain target content.");
            }
        }
        finally
        {
            await sourceService.DisposeAsync(); await targetService.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(sourceOwner, targetOwner);
        }
    }

    private static void CheckSchema(TextEditorSessionDataV2 editor, RecoveryStamp epoch)
    {
        var stamp = SessionRecoveryAuthority.CopyStamp(epoch);
        stamp.Ordinal = 1; stamp.OperationId = Guid.NewGuid();
        var address = new RecoveryAddress(new RecoveryArea(RecoveryAreaKind.Checkpoints, epoch.ScopeId), stamp.Ordinal, stamp.OperationId);
        void Encode(NotepadsSessionDataV2 session) =>
            RecoveryRecordCodec.Encode(new RecoveryRecord { Kind = RecoveryRecordKind.Checkpoint, Stamp = stamp, Session = session }, address);
        Encode(Session(epoch.ScopeId, editor, pending: true));
        foreach (var mutate in new Action<NotepadsSessionDataV2>[]
        {
            session => session.PendingRescues = null,
            session => session.PendingRescues[0] = null,
            session => session.PendingRescues.Add(session.PendingRescues[0]),
            session => session.TextEditors.Add(session.PendingRescues[0]),
            session => session.SelectedTextEditor = editor.Id,
            session => session.PendingRescues[0].Journal.OwnerId = Guid.NewGuid()
        })
        {
            var session = Session(epoch.ScopeId, SessionRecoveryCatalog.CloneEditor(editor), pending: true);
            mutate(session);
            ExpectInvalid(() => Encode(session));
        }
        var token = new TransferToken
        {
            TransferId = Guid.NewGuid(), SourceInstanceId = epoch.ScopeId, SourceEditorId = editor.Id,
            SourceOwnerId = epoch.ScopeId, SourceEpochId = epoch.EpochId, Nonce = Guid.NewGuid(), DescriptorSha256 = editor.ComputeSha256()
        };
        var offer = RecoveryRecordValidator.CreateEditorRoot(editor);
        offer.TransferSource = token;
        RecoveryRecordValidator.ValidateEditorRoot(offer);
        offer.PendingRescues.Add(editor);
        ExpectInvalid(() => RecoveryRecordValidator.ValidateEditorRoot(offer));
        ExpectInvalid(() => SessionRecoveryReferences.ValidateTransferReferences(offer));
        var acknowledgement = new NotepadsSessionDataV2
        { TransferAcknowledgement = token, AcknowledgedSource = editor, SourceRemovalRevision = editor.CaptureRevision };
        RecoveryRecordValidator.ValidateAcknowledgement(acknowledgement, token);
        acknowledgement.PendingRescues.Add(editor);
        ExpectInvalid(() => RecoveryRecordValidator.ValidateAcknowledgement(acknowledgement, token));
    }

    private static NotepadsSessionDataV2 Session(Guid owner, TextEditorSessionDataV2 editor, bool pending)
    {
        var session = new NotepadsSessionDataV2
        { Scope = new SessionScopeData { OwnerId = owner, InstanceId = owner, Kind = SessionScopeData.Secondary } };
        if (pending) session.PendingRescues.Add(editor); else session.TextEditors.Add(editor);
        return session;
    }

    private static RecoveryDocumentSource Forge(RecoveryDocumentSource source, RecoveryReadResult root,
        string hash = null, RecoveryStamp stamp = null, bool disposed = false, Guid? targetId = null, long? cutoff = null)
    {
        var descriptor = source.Descriptor;
        if (targetId.HasValue) descriptor.Id = targetId.Value;
        var original = source.Origin.Decision;
        var decision = new RecoveryDocumentDecision(original.Membership, original.Primary,
            original.Fallbacks, original.ProtectedNewer, cutoff ?? original.ConsumptionCutoff,
            original.RescueRootAddress, original.RescueRootSha256, original.ImportFromInactive);
        var origin = new RecoveryAdoptionOrigin(stamp ?? source.Origin.SourceStamp, decision,
            root.Address, hash ?? root.PayloadSha256);
        return new(decision, null, false, stamp ?? source.ExpectedStamp, origin,
            new RecoverySourceLifetime { Disposed = disposed }, source.TargetStamp, descriptor);
    }

    private static async Task<SessionSaveResult> SavePendingAsync(SessionService service, RecoveryDocumentSource source, double scroll = 0)
    {
        using var capture = new SessionCapture { ExpectedStamp = service.CurrentStamp, TabScrollOffset = scroll };
        capture.PendingRescues.Add(source);
        return await service.SaveAsync(capture, CancellationToken.None);
    }

    private static async Task<RecoveryReadResult> CreateRescueAsync(SessionService service, string text)
    {
        await using var document = await SessionTestDocument.CreateAsync(service.CurrentStamp.ScopeId, text);
        using var state = document.CaptureState();
        await state.Checkpoint.FlushAsync();
        var editor = SessionDocumentStore.CaptureEditor(Guid.NewGuid(), state, null, null);
        state.PreserveForRecovery();
        using (await SessionRecoveryTransaction.EnterAsync())
        {
            var store = RecoveryRootStore.CreateStore();
            return await PublishAsync(store, store.GetScopeArea(service.CurrentStamp.ScopeId, RecoveryAreaKind.Rescue, editor.Id),
                new RecoveryRecord { Kind = RecoveryRecordKind.Rescue, Editor = editor }, service.CurrentStamp);
        }
    }

    private static async Task<RecoveryReadResult> PublishAsync(RecoveryRecordStore store, RecoveryArea area,
        RecoveryRecord record, RecoveryStamp epoch)
    {
        record.Stamp = store.CreatePublicationStamp(area, epoch);
        var publication = await store.PublishAsync(area, record);
        RecoveryRootStore.RequireCommitted(publication);
        return await store.ReadAsync(publication.Address);
    }

    private static void ExpectInvalid(Action action)
    {
        try { action(); }
        catch (InvalidDataException) { return; }
        throw new InvalidOperationException("Malformed pending recovery metadata was accepted.");
    }

    private sealed class AdoptionAcknowledgementFailure : IRecoveryStorageFaults
    {
        public bool Enabled { get; set; }
        public RecoveryAddress FailedAddress { get; private set; }

        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (!Enabled) return;
            if (address.Area.Kind == RecoveryAreaKind.Decisions && step == RecoveryStorageStep.AfterRename && copy == "a")
            {
                FailedAddress = address;
                throw new IOException("Injected lost adoption acknowledgement after mirror A committed.");
            }
            if (address == FailedAddress && step == RecoveryStorageStep.BeforeRead)
                throw new IOException("Injected unavailable adoption readback.");
        }
    }

    private static void Check(bool condition, string message) => SessionTestProtocol.Check(condition, message);
}
