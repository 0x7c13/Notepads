// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Interprets immutable content roots separately from durable lifecycle decisions.</summary>
internal static class SessionRecoveryCatalog
{
    // Callers hold store admission throughout all catalog reads and repairs.
    public static async Task<RecoveryScopeSnapshot> ReadScopeAsync(Guid scopeId,
        CancellationToken cancellation = default, bool content = true)
    {
        using var measurement = OperationMetrics.Measure("session.catalog");
        var store = RecoveryRootStore.CreateStore();
        var result = await ReadControlAsync(store, scopeId, cancellation);
        if (!store.ScanLayout().IsComplete) Block(result, "The recovery layout contains unrecognized roots.");
        var scopes = store.EnumerateScopeIds();
        if (!scopes.IsComplete) Block(result, "The recovery scope directory cannot be enumerated reliably.");
        foreach (var owner in scopes.Entries.Where(owner => owner != scopeId))
        {
            var other = await ReadControlAsync(store, owner, cancellation);
            // A missing foreign consumption decision can resurrect this source.
            if (other.Blocked) Block(result, "A recovery lifecycle decision cannot be verified.");
            result.GlobalRecords.AddRange(other.Decisions);
        }
        result.GlobalRecords.AddRange(await ReadGlobalAsync(store, RecoveryAreaKind.Transfers, cancellation));
        if (result.GlobalRecords.Any(read => read.State != RecoveryReadState.Valid))
            Block(result, "A transfer lifecycle decision cannot be verified.");
        if (!content || result.Blocked) return result;
        result.Checkpoints.AddRange(await ReadAreaAsync(store,
            store.GetScopeArea(scopeId, RecoveryAreaKind.Checkpoints), cancellation));
        var pendingAreas = store.EnumerateScopeAreas(scopeId, RecoveryAreaKind.Pending);
        if (!pendingAreas.IsComplete) Block(result, "The pending descriptor directory is unreadable.");
        foreach (var area in pendingAreas.Entries)
            result.Pending.AddRange(await ReadAreaAsync(store, area, cancellation));
        var rescueAreas = store.EnumerateScopeAreas(scopeId, RecoveryAreaKind.Rescue);
        if (!rescueAreas.IsComplete) Block(result, "The rescue descriptor directory is unreadable.");
        foreach (var area in rescueAreas.Entries)
            result.Rescues.AddRange(await ReadAreaAsync(store, area, cancellation));
        if (result.Checkpoints.Concat(result.Pending).Concat(result.Rescues).Any(read => read.State == RecoveryReadState.Conflict))
            Block(result, "Complete recovery mirrors disagree.");
        if (result.Blocked) return result;

        var own = result.Decisions.Select(read => read.Record).ToArray();
        var all = result.AllDecisions.ToArray();
        var newest = result.Checkpoints.OrderByDescending(read => read.Address.Ordinal).FirstOrDefault();
        result.SelectedCheckpoint = result.Checkpoints.Where(read => read.State == RecoveryReadState.Valid &&
            SessionRecoveryAuthority.SameEpoch(read.Record.Stamp, result.Stamp))
            .OrderByDescending(read => read.Address.Ordinal).FirstOrDefault();
        result.HasCheckpointEvidence = result.Checkpoints.Count != 0;
        result.Session = result.SelectedCheckpoint == null ? new NotepadsSessionDataV2() :
            CloneSession(result.SelectedCheckpoint.Record.Session);
        result.Session.Scope ??= new SessionScopeData
        {
            OwnerId = scopeId,
            InstanceId = scopeId,
            Kind = SessionScopeData.IsReservedOwner(scopeId) ? SessionScopeData.Primary : SessionScopeData.Secondary
        };
        if (result.SelectedCheckpoint != null)
        {
            result.Session.TextEditors.RemoveAll(editor => !SessionRecoveryAuthority.IsEligible(
                result.SelectedCheckpoint.Record.Stamp, editor, result.Stamp, own, all));
        }
        // Durable target adoption content survives bounded checkpoint history.
        foreach (var read in result.Decisions.Concat(result.GlobalRecords)
            .Where(read => read.State == RecoveryReadState.Valid && read.Record.Editor != null &&
                read.Record.Kind is RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive or RecoveryRecordKind.TransferReceipt)
            .OrderByDescending(read => read.Address.Ordinal))
        {
            var record = read.Record;
            var stamp = record.Kind == RecoveryRecordKind.TransferReceipt ? record.TargetStamp : record.Stamp;
            if (!SessionRecoveryAuthority.IsEligible(stamp, record.Editor, result.Stamp, own, all) ||
                result.Session.TextEditors.Any(editor => editor.Id == record.Editor.Id))
            {
                continue;
            }

            result.Session.TextEditors.Add(CloneEditor(record.Editor));
        }
        foreach (var read in result.GlobalRecords.Where(read => read.State == RecoveryReadState.Valid &&
            read.Record.Kind == RecoveryRecordKind.TransferOffer))
        {
            var record = read.Record;
            if (!SessionRecoveryAuthority.IsEligible(record.SourceStamp, record.Editor, result.Stamp, own, all)) continue;
            var existing = result.Session.TextEditors.FindIndex(editor => editor.Id == record.Editor.Id);
            if (existing < 0) result.Session.TextEditors.Add(CloneEditor(record.Editor));
            else if (record.Editor.CaptureRevision > result.Session.TextEditors[existing].CaptureRevision)
                result.Session.TextEditors[existing] = CloneEditor(record.Editor);
        }
        if (result.SelectedCheckpoint != null)
        {
            result.Outcome = newest?.Address == result.SelectedCheckpoint.Address ?
                result.SelectedCheckpoint.Redundancy is RecoveryRedundancy.Single or RecoveryRedundancy.Repaired ?
                    SessionRecoveryOutcome.MirrorRepaired : SessionRecoveryOutcome.Exact : SessionRecoveryOutcome.OlderCheckpoint;
        }
        else
        {
            foreach (var read in result.Rescues.Where(read => read.State == RecoveryReadState.Valid)
                .OrderByDescending(read => read.Address.Ordinal))
            {
                if (!SessionRecoveryAuthority.IsEligible(read.Record.Stamp, read.Record.Editor, result.Stamp, own, all) ||
                    result.Session.TextEditors.Any(existing => existing.Id == read.Record.Editor.Id) ||
                    result.EligibleRescues.Any(existing => existing.Record.Editor.Id == read.Record.Editor.Id))
                {
                    continue;
                }

                result.EligibleRescues.Add(read);
            }
            if (result.EligibleRescues.Count != 0) result.Outcome = SessionRecoveryOutcome.Rescue;
            else if (result.Session.TextEditors.Count != 0) result.Outcome = SessionRecoveryOutcome.OlderCheckpoint;
            else if (result.Checkpoints.Any(read => read.State != RecoveryReadState.Valid))
                Block(result, "No valid checkpoint or rescue descriptor remains.");
        }
        return result;
    }

    private static async Task<RecoveryScopeSnapshot> ReadControlAsync(RecoveryRecordStore store, Guid scopeId,
        CancellationToken cancellation)
    {
        var result = new RecoveryScopeSnapshot { Stamp = new RecoveryStamp { ScopeId = scopeId, EpochId = scopeId } };
        var area = store.GetScopeArea(scopeId, RecoveryAreaKind.Decisions);
        var scan = store.Enumerate(area);
        if (!store.ScanLayout(scopeId).IsComplete) Block(result, "The recovery scope layout contains unrecognized roots.");
        if (!scan.IsComplete) Block(result, "The lifecycle decision directory is unreadable.");
        foreach (var address in scan.Entries.OrderBy(address => address.Ordinal))
        {
            var read = await store.ReadAsync(address, true, cancellation);
            var intent = await store.ReadResetIntentAsync(address, cancellation);
            if (intent.State != RecoveryReadState.Absent)
            {
                if (intent.State != RecoveryReadState.Valid)
                {
                    Block(result, "A reset intent is unreadable.");
                }
                else if (read.State == RecoveryReadState.Valid)
                {
                    if (read.Record.Kind != RecoveryRecordKind.Reset || intent.PayloadSha256 != read.PayloadSha256)
                        Block(result, "A reset intent conflicts with its decision.");
                }
                else if (read.State is RecoveryReadState.Absent or RecoveryReadState.Corrupt)
                {
                    try
                    {
                        var prior = SessionRecoveryAuthority.ResolveEpoch(scopeId, result.Decisions.Select(item => item.Record));
                        if (intent.Record.PreviousEpochId != prior.EpochId)
                            throw new InvalidDataException("The reset intent has an inconsistent predecessor.");
                        var publication = await store.CompleteResetIntentAsync(address, cancellation);
                        if (publication.IsCommitted) read = await store.ReadAsync(address, true, CancellationToken.None);
                        else Block(result, "A reset intent could not be completed durably.");
                    }
                    catch (Exception ex) { Block(result, ex.Message); }
                }
                else
                {
                    Block(result, "A reset decision cannot be reconciled.");
                }
            }
            if (read.State == RecoveryReadState.Valid) result.Decisions.Add(read);
            else Block(result, "A lifecycle decision cannot be verified.");
        }
        try { result.Stamp = SessionRecoveryAuthority.ResolveEpoch(scopeId, result.Decisions.Select(read => read.Record)); }
        catch (InvalidDataException ex) { Block(result, ex.Message); }
        return result;
    }

    internal static async Task<List<RecoveryReadResult>> ReadAreaAsync(RecoveryRecordStore store, RecoveryArea area,
        CancellationToken cancellation)
    {
        var scan = store.Enumerate(area);
        var results = new List<RecoveryReadResult>();
        if (!scan.IsComplete)
        {
            results.Add(new RecoveryReadResult(RecoveryReadState.Unreadable,
            new RecoveryAddress(area, ulong.MaxValue, Guid.Empty), Error: scan.Error));
        }

        foreach (var address in scan.Entries) results.Add(await store.ReadAsync(address, true, cancellation));
        return results;
    }

    private static async Task<List<RecoveryReadResult>> ReadGlobalAsync(RecoveryRecordStore store,
        RecoveryAreaKind kind, CancellationToken cancellation)
    {
        var scan = store.EnumerateGlobalAreas(kind);
        var results = new List<RecoveryReadResult>();
        if (!scan.IsComplete)
        {
            results.Add(new RecoveryReadResult(RecoveryReadState.Unreadable,
            new RecoveryAddress(store.GetGlobalArea(kind, Guid.NewGuid()), ulong.MaxValue, Guid.Empty), Error: scan.Error));
        }

        foreach (var area in scan.Entries) results.AddRange(await ReadAreaAsync(store, area, cancellation));
        return results;
    }

    public static async Task<IReadOnlyList<SessionRecoverySource>> ReadInactiveSourcesAsync(Guid currentOwnerId,
        ISet<Guid> liveInstances, CancellationToken cancellationToken = default)
    {
        var store = RecoveryRootStore.CreateStore();
        var sources = new List<SessionRecoverySource>();
        var scan = store.EnumerateScopeIds();
        if (!scan.IsComplete) throw new InvalidDataException("Inactive recovery scopes cannot be enumerated reliably.");
        try
        {
            foreach (var owner in scan.Entries)
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (owner == currentOwnerId || SessionScopeData.IsReservedOwner(owner)) continue;
                var writer = SessionScopeLease.TryAcquireInactiveWriter(owner);
                if (writer == null) continue;
                IDisposable reader = null;
                try
                {
                    reader = SessionScopeLease.AcquireReader(owner);
                    var snapshot = await ReadScopeAsync(owner, cancellationToken);
                    if (snapshot.Blocked)
                    {
                        // The primary catalog has already verified every
                        // lifecycle log under this admission. Content failure
                        // in one inactive owner must preserve that owner and
                        // report degradation without aborting healthy tabs.
                        sources.Add(new SessionRecoverySource
                        {
                            Session = new NotepadsSessionDataV2
                            {
                                Scope = new SessionScopeData
                                { OwnerId = owner, InstanceId = owner, Kind = SessionScopeData.Secondary }
                            },
                            ExpectedStamp = snapshot.Stamp,
                            Outcome = SessionRecoveryOutcome.Partial,
                            CaptureRevisionFloor = SessionRecoveryAuthority.GetCaptureRevisionFloor(snapshot.VerifiedRecords),
                            WriterLease = writer,
                            ReaderPin = reader
                        });
                        writer = null; reader = null;
                        continue;
                    }
                    if (snapshot.Outcome == SessionRecoveryOutcome.Rescue)
                        snapshot.Session.TextEditors.AddRange(snapshot.EligibleRescues.Select(read => CloneEditor(read.Record.Editor)));
                    // Another window's unacknowledged receiver is not a move.
                    snapshot.Session.TextEditors.RemoveAll(editor => snapshot.GlobalRecords.Any(read =>
                        read.Record.Kind == RecoveryRecordKind.TransferReceipt && read.Record.Editor.Id == editor.Id &&
                        read.Record.TargetStamp.ScopeId == owner && !snapshot.GlobalRecords.Any(ack =>
                            ack.Record.Kind == RecoveryRecordKind.SourceMoved && ack.Record.TargetReceiptOperationId == read.Address.OperationId)));
                    if (snapshot.Session.TextEditors.Count == 0) continue;
                    var source = new SessionRecoverySource
                    {
                        Session = snapshot.Session,
                        ExpectedStamp = snapshot.Stamp,
                        RootAddress = snapshot.SelectedCheckpoint?.Address,
                        RootSha256 = snapshot.SelectedCheckpoint?.PayloadSha256,
                        WriterLease = writer,
                        ReaderPin = reader,
                        Outcome = snapshot.Outcome,
                        CaptureRevisionFloor = SessionRecoveryAuthority.GetCaptureRevisionFloor(snapshot.VerifiedRecords)
                    };
                    source.RescueEditorIds.UnionWith(snapshot.EligibleRescues.Select(read => read.Record.Editor.Id));
                    foreach (var editor in source.Session.TextEditors)
                    {
                        source.EarlierDescriptors[editor.Id] = snapshot.EarlierDescriptors(editor.Id).ToArray();
                        source.ConsumptionCutoffs[editor.Id] = snapshot.ConsumptionCutoff(editor.Id);
                        source.UnselectedNewerDescriptors[editor.Id] = snapshot.CurrentDescriptors(editor.Id)
                            .Where(candidate => candidate.CaptureRevision > editor.CaptureRevision)
                            .DistinctBy(candidate => candidate.ComputeSha256()).ToArray();
                    }
                    sources.Add(source);
                    writer = null; reader = null;
                }
                finally { reader?.Dispose(); writer?.Dispose(); }
            }
            return sources;
        }
        catch { foreach (var source in sources) source.Dispose(); throw; }
    }

    public static async Task<SessionRecoveryReferences> ReadReferencesAsync(string backupFolderName,
        CancellationToken cancellationToken = default)
    {
        var store = RecoveryRootStore.CreateStore();
        var references = new SessionRecoveryReferences();
        if (!store.ScanLayout().IsComplete) references.BlocksGlobalCollection = true;
        var scopes = store.EnumerateScopeIds();
        if (!scopes.IsComplete) references.BlocksGlobalCollection = true;
        var controls = new Dictionary<Guid, RecoveryScopeSnapshot>();
        var transfers = await ReadGlobalAsync(store, RecoveryAreaKind.Transfers, cancellationToken);
        foreach (var owner in scopes.Entries)
        {
            var control = await ReadControlAsync(store, owner, cancellationToken);
            controls[owner] = control;
            if (control.Blocked) references.BlockedOwners.Add(owner);
        }
        var decisions = controls.Values.SelectMany(control => control.Decisions).Concat(transfers)
            .Where(read => read.State == RecoveryReadState.Valid).Select(read => read.Record).ToArray();
        foreach (var owner in scopes.Entries)
        {
            var control = controls[owner];
            foreach (var read in control.Decisions) IncludeContentReferences(references, read, controls, decisions);
            foreach (var kind in new[] { RecoveryAreaKind.Checkpoints, RecoveryAreaKind.Pending, RecoveryAreaKind.Rescue })
            {
                var areas = store.EnumerateScopeAreas(owner, kind);
                if (!areas.IsComplete) references.BlockedOwners.Add(owner);
                foreach (var area in areas.Entries)
                {
                    foreach (var read in await ReadAreaAsync(store, area, cancellationToken))
                        IncludeContentReferences(references, read, controls, decisions);
                }
            }
        }
        foreach (var read in transfers) IncludeContentReferences(references, read, controls, decisions);
        foreach (var read in await ReadGlobalAsync(store, RecoveryAreaKind.Archives, cancellationToken))
            IncludeContentReferences(references, read, controls, decisions);
        foreach (var file in await ApplicationData.Current.LocalFolder.GetFilesAsync())
        {
            if (file.Name != "NotepadsSessionData.json" && !SessionScopeData.TryGetSecondaryOwner(file.Name, out _)) continue;
            try
            {
                var (legacy, _, error) = await SessionManifestStore.ReadLegacyAsync(file.Name, cancellationToken);
                if (error != null) references.BlocksGlobalCollection = true;
                else if (legacy != null) references.Include(legacy);
            }
            catch (Exception) { references.BlocksGlobalCollection = true; }
        }
        return references;
    }

    private static void IncludeContentReferences(SessionRecoveryReferences references, RecoveryReadResult read,
        IReadOnlyDictionary<Guid, RecoveryScopeSnapshot> controls, RecoveryRecord[] allDecisions)
    {
        if (read.State != RecoveryReadState.Valid)
        {
            if (read.Address.Area.AllowsForeignOwners) references.BlocksGlobalCollection = true;
            else references.BlockedOwners.Add(read.Address.Area.ScopeId);
            return;
        }
        var record = read.Record;
        if (record.Kind is RecoveryRecordKind.Pending or RecoveryRecordKind.Archive)
        { references.Include(record); return; }
        void IncludeEligible(RecoveryStamp stamp, TextEditorSessionDataV2 editor)
        {
            if (editor == null) return;
            if (!controls.TryGetValue(stamp.ScopeId, out var control) || control.Blocked)
            { references.IncludeEditor(editor); return; }
            if (SessionRecoveryAuthority.IsEligible(stamp, editor, control.Stamp,
                control.Decisions.Select(decision => decision.Record), allDecisions))
            {
                references.IncludeEditor(editor);
            }
        }
        switch (record.Kind)
        {
            case RecoveryRecordKind.Checkpoint:
                foreach (var editor in record.Session.TextEditors) IncludeEligible(record.Stamp, editor);
                break;
            case RecoveryRecordKind.Rescue:
            case RecoveryRecordKind.AdoptLegacy:
            case RecoveryRecordKind.AdoptInactive:
                IncludeEligible(record.Stamp, record.Editor);
                break;
            case RecoveryRecordKind.TransferOffer:
                IncludeEligible(record.SourceStamp, record.Editor);
                break;
            case RecoveryRecordKind.TransferReceipt:
            case RecoveryRecordKind.SourceMoved:
                // ACK source metadata remains durable suppression evidence;
                // its old source assets are not needed to replay the target.
                IncludeEligible(record.TargetStamp, record.Editor);
                break;
        }
    }

    internal static NotepadsSessionDataV2 CloneSession(NotepadsSessionDataV2 session) => JsonSerializer.Deserialize(
        JsonSerializer.SerializeToUtf8Bytes(session, SessionJsonContext.Default.NotepadsSessionDataV2),
        SessionJsonContext.Default.NotepadsSessionDataV2);
    internal static TextEditorSessionDataV2 CloneEditor(TextEditorSessionDataV2 editor) => JsonSerializer.Deserialize(
        JsonSerializer.SerializeToUtf8Bytes(editor, SessionJsonContext.Default.TextEditorSessionDataV2),
        SessionJsonContext.Default.TextEditorSessionDataV2);

    private static void Block(RecoveryScopeSnapshot result, string reason)
    {
        result.Blocked = true; result.Outcome = SessionRecoveryOutcome.Blocked; result.AttentionReason ??= reason;
    }
}
