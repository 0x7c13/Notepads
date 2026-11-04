// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;

namespace Notepads.Features.Sessions;

internal sealed partial class SessionService
{
    // Called under store admission, before the unchanged-checkpoint shortcut. Pending imports
    // carry discovery membership, not native ownership; only exact own rescue roots authorize it.
    private async Task<List<TextEditorSessionDataV2>> ValidatePendingRescuesAsync(SessionCapture capture,
        RecoveryScopeSnapshot authority, CancellationToken cancellation)
    {
        var expected = capture.ExpectedStamp;
        var ids = capture.Documents.Select(document => document.Id).ToHashSet();
        foreach (var source in capture.PendingRescues)
        {
            if (source == null) throw new InvalidDataException("A pending rescue requires its discovery source.");
            source.RequireAvailable();
            var origin = source.Origin;
            if (!source.IsOwnScopeRescue || !SessionRecoveryAuthority.SameEpoch(expected, source.ExpectedStamp) ||
                !SessionRecoveryAuthority.SameEpoch(expected, source.TargetStamp) ||
                !SessionRecoveryAuthority.SameEpoch(expected, origin?.SourceStamp) ||
                origin.RootAddress == null || origin.Decision == null ||
                origin.RootAddress.Area.ScopeId != expected.ScopeId ||
                origin.RootAddress.Area.Kind is not (RecoveryAreaKind.Rescue or RecoveryAreaKind.Checkpoints) ||
                capture.Documents.Any(document => document.Id == source.Id) || !ids.Add(origin.Decision.Id))
                throw new InvalidDataException("A pending rescue must be a distinct original source in this recovery epoch.");
        }

        var result = new List<TextEditorSessionDataV2>(capture.PendingRescues.Count);
        // Restarted imports commonly share a checkpoint. Decode and index that root once.
        foreach (var group in capture.PendingRescues.GroupBy(source => source.Origin.RootAddress))
        {
            var root = await _recordStore.ReadAsync(group.Key, true, cancellation);
            if (root.State != RecoveryReadState.Valid || !SessionRecoveryAuthority.SameEpoch(expected, root.Record.Stamp))
                throw new InvalidDataException("The pending rescue root is unavailable or obsolete.");
            var members = (root.Record.Kind == RecoveryRecordKind.Rescue ? new[] { root.Record.Editor } :
                root.Record.Session.PendingRescues.ToArray()).ToDictionary(editor => editor.Id);
            foreach (var source in group)
            {
                var origin = source.Origin;
                if (root.PayloadSha256 != origin.RootSha256 ||
                    !members.TryGetValue(origin.Decision.Id, out var editor) ||
                    editor.ComputeSha256() != origin.Decision.DescriptorSha256)
                    throw new InvalidDataException("The pending rescue no longer matches its discovery root.");
                // Publication can succeed while its readback fails. The failed import stays queued,
                // but this exact independently durable adoption already preserves it across restart.
                if (authority.Decisions.Any(read => read.Record.Kind == RecoveryRecordKind.AdoptInactive &&
                    SessionRecoveryAuthority.SameEpoch(expected, read.Record.Stamp) && read.Record.Editor.Id == source.Id &&
                    SessionRecoveryAuthority.SameEpoch(origin.SourceStamp, read.Record.SourceStamp) &&
                    read.Record.Consumptions.Any(key => key.Kind == RecoveryConsumptionKind.Inactive &&
                        key.SourceScopeId == origin.SourceStamp.ScopeId && key.SourceEpochId == origin.SourceStamp.EpochId &&
                        key.SourceEditorId == editor.Id && key.SourceCaptureRevision == origin.ConsumptionCutoff))) continue;
                if (!SessionRecoveryAuthority.IsEligible(root.Record.Stamp, editor, authority.Stamp,
                    authority.Decisions.Select(read => read.Record), authority.AllDecisions))
                    throw new InvalidDataException("The pending rescue no longer matches its eligible discovery root.");
                result.Add(SessionRecoveryCatalog.CloneEditor(editor));
            }
        }
        return result;
    }

    public async Task<SessionRecoveryPlan> DiscoverRecoveryAsync(CancellationToken cancellation = default)
    {
        using var measurement = OperationMetrics.Measure("session.discover");
        await EnsureWriterLeaseAsync(cancellation);
        var pins = new List<IDisposable>();
        var lifetime = new RecoverySourceLifetime();
        try
        {
            using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            {
                var pass = new RecoveryCatalogPass();
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(_sessionScope.OwnerId, cancellation, pass: pass);
                BindAuthority(snapshot);
                RecoveryOutcome = snapshot.Outcome;
                _committedSessionData = snapshot.SelectedCheckpoint?.Record.Session;
                AcceptLegacyDecisions(snapshot);
                pins.Add(SessionScopeLease.AcquireReader(_sessionScope.OwnerId));
                var sources = new List<RecoveryDocumentSource>();
                var selection = snapshot.Selection;
                foreach (var decision in selection.Documents.Where(document => document.Membership == RecoveryMembership.Owned))
                    sources.Add(new RecoveryDocumentSource(decision, null, true, CurrentStamp, null, lifetime));
                var ownedCount = sources.Count;
                var selected = selection.SelectedEditorId;
                var scroll = selection.TabScrollViewerHorizontalOffset;
                var (legacy, fingerprint, legacyError) = await SessionManifestStore.ReadLegacyAsync(_sessionMetaDataFileName, cancellation);
                if (fingerprint != null) _acceptedLegacySources.Add(fingerprint);
                if (legacyError != null)
                {
                    RecoveryOutcome = SessionRecoveryAuthority.CombineOutcomes(RecoveryOutcome,
                        selection.Documents.Count == 0 ?
                            SessionRecoveryOutcome.Blocked : SessionRecoveryOutcome.Partial);
                    LoggingService.LogError($"[{nameof(SessionService)}] Preserving unreadable migration source: {legacyError}");
                }
                if (legacy != null)
                {
                    foreach (var editor in legacy.TextEditors)
                    {
                        if (SessionRecoveryAuthority.IsLegacyConsumed(fingerprint, editor.Id, snapshot.AllDecisions) ||
                            sources.Any(source => source.Id == editor.Id)) continue;
                        sources.Add(new RecoveryDocumentSource(null, editor, false, CurrentStamp,
                            new RecoveryAdoptionOrigin(fingerprint), lifetime));
                    }
                    if (ownedCount == 0 && sources.Count != 0)
                        (selected, scroll) = (legacy.SelectedTextEditor, legacy.TabScrollViewerHorizontalOffset);
                }
                foreach (var decision in selection.Documents.Where(document => document.Membership == RecoveryMembership.Rescue))
                {
                    sources.Add(new RecoveryDocumentSource(decision, null, false, CurrentStamp,
                        new RecoveryAdoptionOrigin(CurrentStamp, decision, decision.RescueRootAddress, decision.RescueRootSha256),
                        lifetime, descriptorOverride: AsRescue(decision.Descriptor)));
                }
                long importedFloor = 0;
                if (_sessionScope.Kind == SessionScopeData.Primary)
                {
                    var imported = await SessionRecoveryCatalog.ReadInactiveSourcesAsync(_sessionScope.OwnerId,
                        new HashSet<Guid>(_getLiveInstances()), cancellation, pass);
                    pins.AddRange(imported);
                    foreach (var source in imported)
                    {
                        importedFloor = Math.Max(importedFloor, source.CaptureRevisionFloor);
                        RecoveryOutcome = SessionRecoveryAuthority.CombineOutcomes(RecoveryOutcome, source.Outcome);
                        foreach (var decision in source.Documents)
                        {
                            if (sources.Any(existing => existing.Id == decision.Id)) continue;
                            sources.Add(new RecoveryDocumentSource(decision, null, false, source.ExpectedStamp,
                                new RecoveryAdoptionOrigin(source.ExpectedStamp, decision, source.RootAddress, source.RootSha256),
                                lifetime, CurrentStamp, decision.Membership == RecoveryMembership.Rescue ? AsRescue(decision.Descriptor) : null));
                        }
                    }
                }
                DocumentRecoveryState.AdvanceCaptureRevisionFloor(Math.Max(importedFloor,
                    SessionRecoveryAuthority.GetCaptureRevisionFloor(snapshot.VerifiedRecords)));
                return new SessionRecoveryPlan(sources, selected == Guid.Empty ? null : selected,
                    scroll, CurrentStamp, pins, lifetime);
            }
        }
        catch
        {
            lifetime.Disposed = true;
            foreach (var pin in pins) pin.Dispose();
            throw;
        }
    }

    private void AcceptLegacyDecisions(RecoveryScopeSnapshot snapshot)
    {
        foreach (var decision in snapshot.Decisions.Select(read => read.Record))
        {
            _acceptedLegacySources.UnionWith(decision.ResetAcceptedLegacySources);
            _acceptedLegacySources.UnionWith(decision.Consumptions.Where(key => key.Kind == RecoveryConsumptionKind.Legacy)
                .Select(key => key.LegacyFingerprint));
        }
    }

    private static TextEditorSessionDataV2 AsRescue(TextEditorSessionDataV2 source)
    {
        var editor = SessionRecoveryCatalog.CloneEditor(source);
        editor.Id = Guid.NewGuid();
        editor.EditingFileFutureAccessToken = null;
        editor.EditingFilePath = null;
        editor.StateMetaData.HasEditingFile = false;
        editor.StateMetaData.RequiresSaveAs = true;
        editor.StateMetaData.IsModified = true;
        editor.TextDirty = true;
        return editor;
    }

    /// <summary>Must commit before the caller tries any older payload.</summary>
    public async Task PreserveFailedNewestAsync(RecoveryDocumentSource source, CancellationToken cancellation = default)
    {
        source.RequireAvailable();
        var descriptor = source.Origin?.SourceEditor ?? source.Descriptor;
        if (descriptor == null) throw new InvalidOperationException("Legacy recovery has no earlier descriptor.");
        var expected = source.ExpectedStamp;
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            if (!SessionRecoveryAuthority.SameEpoch(expected, source.TargetStamp))
            {
                var target = await SessionRecoveryCatalog.ReadScopeAsync(_sessionScope.OwnerId, cancellation, content: false);
                IsRecoveryBlocked = target.Blocked;
                SessionRecoveryAuthority.RequireCurrent(source.TargetStamp, target);
            }
            var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false);
            IsRecoveryBlocked = authority.Blocked;
            SessionRecoveryAuthority.RequireCurrent(expected, authority);
            var area = _recordStore.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Pending, descriptor.Id);
            RecoveryRootStore.RequireCommitted(await _recordStore.PublishAsync(area, new RecoveryRecord
            {
                Kind = RecoveryRecordKind.Pending,
                Stamp = _recordStore.CreatePublicationStamp(area, expected),
                Editor = descriptor
            }, cancellation));
            RecoveryOutcome = SessionRecoveryOutcome.Partial;
        }
    }
}
