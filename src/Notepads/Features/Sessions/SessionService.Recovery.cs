// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
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
                foreach (var editor in snapshot.Session.TextEditors)
                    sources.Add(new RecoveryDocumentSource(editor, null, true, CurrentStamp,
                        snapshot.EarlierDescriptors(editor.Id), null, lifetime));
                var selected = snapshot.Session.SelectedTextEditor;
                var scroll = snapshot.Session.TabScrollViewerHorizontalOffset;
                var (legacy, fingerprint, legacyError) = await SessionManifestStore.ReadLegacyAsync(_sessionMetaDataFileName, cancellation);
                if (fingerprint != null) _acceptedLegacySources.Add(fingerprint);
                if (legacyError != null)
                {
                    RecoveryOutcome = SessionRecoveryAuthority.CombineOutcomes(RecoveryOutcome,
                        snapshot.Session.TextEditors.Count == 0 && snapshot.EligibleRescues.Count == 0 ?
                            SessionRecoveryOutcome.Blocked : SessionRecoveryOutcome.Partial);
                    LoggingService.LogError($"[{nameof(SessionService)}] Preserving unreadable migration source: {legacyError}");
                }
                if (legacy != null)
                {
                    foreach (var editor in legacy.TextEditors)
                    {
                        if (SessionRecoveryAuthority.IsLegacyConsumed(fingerprint, editor.Id, snapshot.AllDecisions) ||
                            sources.Any(source => source.Id == editor.Id)) continue;
                        sources.Add(new RecoveryDocumentSource(null, editor, false, CurrentStamp, null,
                            new RecoveryAdoptionOrigin { LegacyFingerprint = fingerprint }, lifetime));
                    }
                    if (snapshot.Session.TextEditors.Count == 0 && sources.Count != 0)
                        (selected, scroll) = (legacy.SelectedTextEditor, legacy.TabScrollViewerHorizontalOffset);
                }
                foreach (var rescue in snapshot.EligibleRescues)
                {
                    var editor = rescue.Record.Editor;
                    sources.Add(new RecoveryDocumentSource(AsRescue(editor), null, false, CurrentStamp,
                        snapshot.EarlierDescriptors(editor.Id), new RecoveryAdoptionOrigin
                        {
                            SourceStamp = CurrentStamp,
                            RootAddress = rescue.Address,
                            RootSha256 = rescue.PayloadSha256,
                            SourceEditor = SessionRecoveryCatalog.CloneEditor(editor),
                            ConsumptionCutoff = snapshot.ConsumptionCutoff(editor.Id),
                            UnselectedNewerDescriptors = snapshot.CurrentDescriptors(editor.Id)
                                .Where(candidate => candidate.CaptureRevision > editor.CaptureRevision)
                                .DistinctBy(candidate => candidate.ComputeSha256()).Select(SessionRecoveryCatalog.CloneEditor).ToArray()
                        }, lifetime));
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
                        foreach (var editor in source.Session.TextEditors)
                        {
                            if (sources.Any(existing => existing.Id == editor.Id)) continue;
                            source.EarlierDescriptors.TryGetValue(editor.Id, out var earlier);
                            source.UnselectedNewerDescriptors.TryGetValue(editor.Id, out var newer);
                            sources.Add(new RecoveryDocumentSource(source.RescueEditorIds.Contains(editor.Id) ? AsRescue(editor) : editor,
                                null, false, source.ExpectedStamp, earlier, new RecoveryAdoptionOrigin
                                {
                                    SourceStamp = SessionRecoveryAuthority.CopyStamp(source.ExpectedStamp),
                                    RootAddress = source.RootAddress, RootSha256 = source.RootSha256,
                                    SourceEditor = SessionRecoveryCatalog.CloneEditor(editor),
                                    ConsumptionCutoff = source.ConsumptionCutoffs.TryGetValue(editor.Id, out var cutoff) ? cutoff : editor.CaptureRevision,
                                    UnselectedNewerDescriptors = (newer ?? []).Select(SessionRecoveryCatalog.CloneEditor).ToArray()
                                }, lifetime, CurrentStamp));
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
