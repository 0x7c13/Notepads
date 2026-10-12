// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
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

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Reads and verifies roots, then selects recovery content from the complete evidence.</summary>
internal static class SessionRecoveryCatalog
{
    // Callers hold store admission throughout all catalog reads and repairs. A pass shares
    // lifecycle reads between the catalog reads of that one admission.
    public static async Task<RecoveryScopeSnapshot> ReadScopeAsync(Guid scopeId,
        CancellationToken cancellation = default, bool content = true, RecoveryCatalogPass pass = null)
    {
        using var measurement = OperationMetrics.Measure("session.catalog");
        var store = RecoveryRootStore.CreateStore();
        pass ??= new RecoveryCatalogPass();
        // Copy the shared control's verdict; the pass keeps exactly what was read.
        var control = await ReadControlAsync(store, scopeId, pass, cancellation);
        var result = new RecoveryScopeSnapshot { Stamp = SessionRecoveryAuthority.CopyStamp(control.Stamp) };
        result.Decisions.AddRange(control.Decisions);
        if (control.Blocked) Block(result, control.AttentionReason);
        if (!store.ScanLayout().IsComplete) Block(result, "The recovery layout contains unrecognized roots.");
        var scopes = store.EnumerateScopeIds();
        if (!scopes.IsComplete) Block(result, "The recovery scope directory cannot be enumerated reliably.");
        foreach (var owner in scopes.Entries.Where(owner => owner != scopeId))
        {
            var other = await ReadControlAsync(store, owner, pass, cancellation);
            // A missing foreign consumption decision can resurrect this source.
            if (other.Blocked) Block(result, "A recovery lifecycle decision cannot be verified.");
            result.GlobalRecords.AddRange(other.Decisions);
        }
        pass.Transfers ??= await ReadGlobalAsync(store, RecoveryAreaKind.Transfers, cancellation);
        result.GlobalRecords.AddRange(pass.Transfers);
        if (result.GlobalRecords.Any(read => read.State != RecoveryReadState.Valid))
            Block(result, "A transfer lifecycle decision cannot be verified.");
        if (!content) return result;
        if (result.Blocked)
        {
            result.AttachSelection(SessionRecoverySelector.Select(result));
            return result;
        }
        result.RetiredContentOperations.UnionWith(result.Decisions.Where(read => read.State == RecoveryReadState.Valid &&
            read.Record.Kind == RecoveryRecordKind.Reset && read.Record.Stamp.ScopeId == scopeId)
            .SelectMany(read => read.Record.ResetRetiredContentOperations));
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
        result.AttachSelection(SessionRecoverySelector.Select(result));
        return result;
    }

    private static async Task<RecoveryScopeSnapshot> ReadControlAsync(RecoveryRecordStore store, Guid scopeId,
        RecoveryCatalogPass pass, CancellationToken cancellation)
    {
        if (!pass.Controls.TryGetValue(scopeId, out var control))
            pass.Controls[scopeId] = control = await ReadControlAsync(store, scopeId, cancellation);
        return control;
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
            // Only an intent copy the scan saw can exist under admission; an incomplete scan probes every decision.
            var intent = !scan.IsComplete || scan.ResetIntents.Contains(address) ?
                await store.ReadResetIntentAsync(address, cancellation) : null;
            if (intent != null && intent.State != RecoveryReadState.Absent)
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
        ISet<Guid> liveInstances, CancellationToken cancellationToken = default, RecoveryCatalogPass pass = null)
    {
        var store = RecoveryRootStore.CreateStore();
        // Every inactive scope's catalog read needs every scope's decisions; read them once.
        pass ??= new RecoveryCatalogPass();
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
                    var snapshot = await ReadScopeAsync(owner, cancellationToken, pass: pass);
                    var selection = snapshot.Selection;
                    if (!snapshot.Blocked && !selection.Documents.Any(document => document.ImportFromInactive)) continue;
                    // Failed inactive content remains pinned as an empty Partial source;
                    // it must not abort healthy imports or authorize collection.
                    sources.Add(new SessionRecoverySource(selection,
                        SessionRecoveryAuthority.GetCaptureRevisionFloor(snapshot.VerifiedRecords), writer, reader));
                    writer = null; reader = null;
                }
                finally { reader?.Dispose(); writer?.Dispose(); }
            }
            return sources;
        }
        catch { foreach (var source in sources) source.Dispose(); throw; }
    }

    public static async Task<SessionRecoveryReferences> ReadReferencesAsync(
        CancellationToken cancellationToken = default, RecoveryCatalogPass pass = null)
    {
        var store = RecoveryRootStore.CreateStore();
        pass ??= new RecoveryCatalogPass();
        var references = new SessionRecoveryReferences();
        if (!store.ScanLayout().IsComplete) references.BlocksGlobalCollection = true;
        var scopes = store.EnumerateScopeIds();
        if (!scopes.IsComplete) references.BlocksGlobalCollection = true;
        var controls = new Dictionary<Guid, RecoveryScopeSnapshot>();
        var transfers = pass.Transfers ??= await ReadGlobalAsync(store, RecoveryAreaKind.Transfers, cancellationToken);
        foreach (var owner in scopes.Entries)
        {
            var control = await ReadControlAsync(store, owner, pass, cancellationToken);
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
                foreach (var editor in record.Session.TextEditors.Concat(record.Session.PendingRescues))
                    IncludeEligible(record.Stamp, editor);
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
        result.ControlBlocked = true;
        result.ControlOutcome = SessionRecoveryOutcome.Blocked;
        result.ControlAttentionReason ??= reason;
    }
}

/// <summary>
/// Lifecycle controls and transfer roots read once for one admitted operation. Create it under that
/// admission and drop it with it, so authority is never cached across admissions.
/// </summary>
internal sealed class RecoveryCatalogPass
{
    internal Dictionary<Guid, RecoveryScopeSnapshot> Controls { get; } = [];
    internal List<RecoveryReadResult> Transfers { get; set; }

    /// <summary>A caller that publishes a decision in this pass must invalidate that scope.</summary>
    public void Invalidate(Guid scopeId) => Controls.Remove(scopeId);
}
