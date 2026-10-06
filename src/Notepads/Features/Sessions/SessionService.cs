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
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Notepads.Infrastructure.Threading;
using Windows.Storage;

namespace Notepads.Features.Sessions;

internal sealed partial class SessionService
{
    private readonly SessionScopeData _sessionScope;
    private readonly string _backupFolderName;
    private readonly string _sessionMetaDataFileName;
    private readonly Func<IReadOnlyCollection<Guid>> _getLiveInstances;
    private readonly RecoveryRecordStore _recordStore;
    private readonly Dictionary<Guid, TextEditorSessionDataV1> _unrecoveredEditors = [];
    private readonly Dictionary<Guid, TextEditorSessionDataV2> _unrecoveredV2Editors = [];
    private readonly Dictionary<Guid, RecoveryAdoptionOrigin> _adoptions = [];
    private readonly List<SessionRecoverySource> _importedRecoverySources = [];
    private readonly HashSet<string> _acceptedLegacySources = new(StringComparer.OrdinalIgnoreCase);
    private NotepadsSessionDataV2 _committedSessionData;
    private RecoveryScopeSnapshot _recoverySnapshot;
    private RecoveryStamp _currentStamp;
    private IDisposable _writerLease;
    private bool _sessionMetadataRetained;
    private bool _disposed;
    private string _lastSessionJson;

    public SessionService(SessionScopeData scope, string backupFolderName, string manifestFileName,
        Func<IReadOnlyCollection<Guid>> getLiveInstances, RecoveryRecordStore recordStore = null)
    {
        scope.Validate();
        _sessionScope = scope;
        _backupFolderName = backupFolderName;
        _sessionMetaDataFileName = manifestFileName;
        _getLiveInstances = getLiveInstances ?? throw new ArgumentNullException(nameof(getLiveInstances));
        _recordStore = recordStore ?? RecoveryRootStore.CreateStore();
        if (!string.Equals(_recordStore.Paths.RootPath, RecoveryRootStore.CreateStore().Paths.RootPath, StringComparison.OrdinalIgnoreCase))
            throw new ArgumentException("The metadata repository must use this app's recovery admission root.", nameof(recordStore));
    }

    public bool IsBackupEnabled { get; set; }
    public int UnrecoveredEditorCount => _unrecoveredEditors.Count + _unrecoveredV2Editors.Count;
    public SessionRecoveryOutcome RecoveryOutcome { get; private set; } = SessionRecoveryOutcome.Absent;
    public RecoveryStamp CurrentStamp => SessionRecoveryAuthority.CopyStamp(_currentStamp) ??
        throw new InvalidOperationException("Recovery authority must be initialized before document capture.");

    public async Task InitializeAuthorityAsync(CancellationToken cancellation = default)
    {
        await EnsureWriterLeaseAsync(cancellation);
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            var authority = await SessionRecoveryCatalog.ReadScopeAsync(_sessionScope.OwnerId, cancellation, content: false);
            if (authority.Blocked)
            {
                RecoveryOutcome = SessionRecoveryOutcome.Blocked;
                throw new SessionDataCorruptedException(authority.AttentionReason);
            }
            _currentStamp = authority.Stamp;
        }
    }

    public async Task EnsureMetadataRetainedAsync(CancellationToken cancellation)
    {
        using var measurement = OperationMetrics.Measure("session.metadata");
        if (_sessionMetadataRetained) return;
        await EnsureWriterLeaseAsync(cancellation);
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            _recoverySnapshot = await SessionRecoveryCatalog.ReadScopeAsync(_sessionScope.OwnerId, cancellation);
            RecoveryOutcome = _recoverySnapshot.Outcome;
            if (_recoverySnapshot.Blocked)
                throw new SessionDataCorruptedException(_recoverySnapshot.AttentionReason);
            _currentStamp = _recoverySnapshot.Stamp;
            _committedSessionData = _recoverySnapshot.SelectedCheckpoint?.Record.Session;
            foreach (var decision in _recoverySnapshot.Decisions.Select(read => read.Record))
            {
                _acceptedLegacySources.UnionWith(decision.ResetAcceptedLegacySources);
                _acceptedLegacySources.UnionWith(decision.Consumptions.Where(key => key.Kind == RecoveryConsumptionKind.Legacy)
                    .Select(key => key.LegacyFingerprint));
            }
            var (legacy, fingerprint, legacyError) = await SessionManifestStore.ReadLegacyAsync(_sessionMetaDataFileName, cancellation);
            if (fingerprint != null) _acceptedLegacySources.Add(fingerprint);
            if (legacyError != null)
            {
                RecoveryOutcome = SessionRecoveryAuthority.CombineOutcomes(RecoveryOutcome,
                    _recoverySnapshot.Session.TextEditors.Count == 0 && _recoverySnapshot.EligibleRescues.Count == 0 ?
                        SessionRecoveryOutcome.Blocked : SessionRecoveryOutcome.Partial);
                LoggingService.LogError($"[{nameof(SessionService)}] Preserving unreadable migration source: {legacyError}");
            }
            if (legacy != null)
            {
                _acceptedLegacySources.Add(fingerprint);
                foreach (var editor in legacy.TextEditors)
                {
                    if (SessionRecoveryAuthority.IsLegacyConsumed(fingerprint, editor.Id, _recoverySnapshot.AllDecisions)) continue;
                    _unrecoveredEditors[editor.Id] = editor;
                    _adoptions[editor.Id] = new RecoveryAdoptionOrigin { LegacyFingerprint = fingerprint };
                }
                if (_unrecoveredEditors.Count != 0)
                {
                    _recoverySnapshot.Session.SelectedTextEditor = legacy.SelectedTextEditor;
                    _recoverySnapshot.Session.TabScrollViewerHorizontalOffset = legacy.TabScrollViewerHorizontalOffset;
                }
            }
            foreach (var editor in _recoverySnapshot.Session.TextEditors) _unrecoveredV2Editors[editor.Id] = editor;
            if (_recoverySnapshot.EligibleRescues.Count != 0)
            {
                foreach (var rescue in _recoverySnapshot.EligibleRescues)
                {
                    var editor = SessionRecoveryCatalog.CloneEditor(rescue.Record.Editor);
                    editor.Id = Guid.NewGuid();
                    editor.EditingFileFutureAccessToken = null;
                    editor.EditingFilePath = null;
                    editor.StateMetaData.HasEditingFile = false;
                    editor.StateMetaData.RequiresSaveAs = true;
                    editor.StateMetaData.IsModified = true;
                    editor.TextDirty = true;
                    _unrecoveredV2Editors[editor.Id] = editor;
                    var source = new SessionRecoverySource { ExpectedStamp = CurrentStamp };
                    source.ConsumptionCutoffs[rescue.Record.Editor.Id] = _recoverySnapshot.ConsumptionCutoff(rescue.Record.Editor.Id);
                    source.UnselectedNewerDescriptors[rescue.Record.Editor.Id] = _recoverySnapshot.CurrentDescriptors(rescue.Record.Editor.Id)
                        .Where(candidate => candidate.CaptureRevision > rescue.Record.Editor.CaptureRevision)
                        .DistinctBy(candidate => candidate.ComputeSha256()).ToArray();
                    _adoptions[editor.Id] = new RecoveryAdoptionOrigin
                    {
                        Source = source,
                        SourceEditor = rescue.Record.Editor
                    };
                }
            }
            if (_sessionScope.Kind == SessionScopeData.Primary)
            {
                var sources = await SessionRecoveryCatalog.ReadInactiveSourcesAsync(_sessionScope.OwnerId,
                    new HashSet<Guid>(_getLiveInstances()), cancellation);
                _importedRecoverySources.AddRange(sources);
                foreach (var source in sources)
                {
                    RecoveryOutcome = SessionRecoveryAuthority.CombineOutcomes(RecoveryOutcome, source.Outcome);
                    foreach (var editor in source.Session.TextEditors)
                    {
                        if (_unrecoveredV2Editors.ContainsKey(editor.Id) || _unrecoveredEditors.ContainsKey(editor.Id)) continue;
                        var imported = editor;
                        if (source.RescueEditorIds.Contains(editor.Id))
                        {
                            imported = SessionRecoveryCatalog.CloneEditor(editor);
                            imported.Id = Guid.NewGuid(); imported.EditingFileFutureAccessToken = null;
                            imported.EditingFilePath = null; imported.StateMetaData.HasEditingFile = false;
                            imported.StateMetaData.RequiresSaveAs = true; imported.StateMetaData.IsModified = true; imported.TextDirty = true;
                        }
                        _unrecoveredV2Editors[imported.Id] = imported;
                        _adoptions[imported.Id] = new RecoveryAdoptionOrigin { Source = source, SourceEditor = editor };
                    }
                }
            }
            var revisionFloor = Math.Max(_unrecoveredV2Editors.Values.Select(editor => editor.CaptureRevision).DefaultIfEmpty(0).Max(),
                Math.Max(SessionRecoveryAuthority.GetCaptureRevisionFloor(_recoverySnapshot.VerifiedRecords),
                    _importedRecoverySources.Select(source => source.CaptureRevisionFloor).DefaultIfEmpty(0).Max()));
            Notepads.Features.Documents.DocumentRecoveryState.AdvanceCaptureRevisionFloor(revisionFloor);
            _sessionMetadataRetained = true;
        }
    }

    private async Task<TextEditorSessionDataV2> BuildTextEditorSessionDataAsync(SessionDocumentCapture prepared,
        CancellationToken cancellation)
    {
        using var measurement = OperationMetrics.Measure("session.journal.flush");
        await prepared.Recovery.Checkpoint.FlushAsync().AsCompletionTask(cancellation);
        cancellation.ThrowIfCancellationRequested();
        var data = SessionDocumentStore.CaptureEditor(prepared.Id, prepared.Recovery,
            prepared.EditingFileName, prepared.EditingFilePath);
        return data;
    }

    private static void RetainCaptureRevision(TextEditorSessionDataV2 data, TextEditorSessionDataV2 committed)
    {
        if (committed != null)
        {
            var revision = data.CaptureRevision;
            data.CaptureRevision = committed.CaptureRevision;
            if (data.ComputeSha256() != committed.ComputeSha256()) data.CaptureRevision = revision;
        }
    }

    private async Task<string> GetFileAccessTokenAsync(Guid editorId, TextEditorSessionDataV2 committed,
        StorageFile file, SessionGenerationDraft draft)
    {
        var token = committed?.EditingFileFutureAccessToken;
        if (token != null)
        {
            var committedFile = await FutureAccessListUtility.GetFileFromFutureAccessListAsync(token);
            if (committedFile != null && file.IsEqual(committedFile)) return token;
        }
        return SessionDocumentStore.RegisterFileAccess(editorId, file, _sessionScope.OwnerId, draft);
    }

    public async Task<SessionSaveResult> SaveAsync(SessionCapture capture, CancellationToken cancellation)
    {
        if (!IsBackupEnabled || _disposed) return SessionSaveResult.Failed;
        try { return await PublishCaptureAsync(capture, cancellation); }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(SessionService)}] Recovery publication failed: {ex}");
            return SessionSaveResult.Failed;
        }
    }

    private async Task<SessionSaveResult> PublishCaptureAsync(SessionCapture capture, CancellationToken cancellation)
    {
        using var measurement = OperationMetrics.Measure("session.publish");
        if (capture.ExpectedStamp == null) throw new InvalidOperationException("Capture has no recovery epoch binding.");
        var expected = SessionRecoveryAuthority.CopyStamp(capture.ExpectedStamp);
        var draft = new SessionGenerationDraft();
        try
        {
            var session = new NotepadsSessionDataV2
            {
                Scope = _sessionScope,
                SelectedTextEditor = capture.SelectedEditorId ?? Guid.Empty,
                TabScrollViewerHorizontalOffset = capture.TabScrollOffset
            };
            // Native flushes and large document work precede global admission.
            foreach (var document in capture.Documents)
                session.TextEditors.Add(await BuildTextEditorSessionDataAsync(document, cancellation));
            var sessionEditors = session.TextEditors.ToDictionary(editor => editor.Id);
            foreach (var pending in _unrecoveredV2Editors.Values)
            {
                if (!_adoptions.ContainsKey(pending.Id) && !sessionEditors.ContainsKey(pending.Id))
                {
                    session.TextEditors.Add(pending);
                    sessionEditors.Add(pending.Id, pending);
                }
            }

            using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            {
                var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false);
                SessionRecoveryAuthority.RequireCurrent(expected, authority);
                var committedEditors = _committedSessionData?.TextEditors.ToDictionary(editor => editor.Id) ?? [];
                foreach (var document in capture.Documents)
                {
                    var data = sessionEditors[document.Id];
                    committedEditors.TryGetValue(document.Id, out var committed);
                    if (document.EditingFile != null)
                        data.EditingFileFutureAccessToken = await GetFileAccessTokenAsync(document.Id, committed, document.EditingFile, draft);
                    RetainCaptureRevision(data, committed);
                }
                if (!sessionEditors.ContainsKey(session.SelectedTextEditor))
                    session.SelectedTextEditor = session.TextEditors.FirstOrDefault()?.Id ?? Guid.Empty;
                var json = JsonSerializer.Serialize(session, SessionJsonContext.Default.NotepadsSessionDataV2);
                if (json == _lastSessionJson && !_adoptions.Keys.Any(id => session.TextEditors.Any(editor => editor.Id == id)))
                {
                    draft.Commit();
                    return new SessionSaveResult(true, false);
                }
                var store = _recordStore;
                // Promote before the first final rename. Even an unobservable
                // acknowledgement must retain all possibly published resources.
                draft.PreserveForRecovery();
                foreach (var document in capture.Documents) document.Recovery.PreserveForRecovery();
                foreach (var editor in session.TextEditors)
                {
                    if (_adoptions.TryGetValue(editor.Id, out var origin))
                        await PublishAdoptionAsync(store, expected, editor, origin, cancellation);
                    var area = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Rescue, editor.Id);
                    if (committedEditors.TryGetValue(editor.Id, out var committed) &&
                        committed.ComputeSha256() == editor.ComputeSha256() &&
                        await HasVerifiedRescueAsync(store, area, expected, editor, cancellation))
                    {
                        continue;
                    }

                    var rescue = new RecoveryRecord
                    {
                        Kind = RecoveryRecordKind.Rescue,
                        Stamp = store.CreatePublicationStamp(area, expected),
                        Editor = editor
                    };
                    RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, rescue, cancellation));
                }
                var checkpointArea = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Checkpoints);
                var checkpoint = new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.Checkpoint,
                    Stamp = store.CreatePublicationStamp(checkpointArea, expected),
                    Session = session
                };
                var published = await store.PublishAsync(checkpointArea, checkpoint, cancellation);
                RecoveryRootStore.RequireCommitted(published);
                draft.Commit();
                _committedSessionData = session;
                _lastSessionJson = json;
                foreach (var document in capture.Documents)
                {
                    _unrecoveredEditors.Remove(document.Id);
                    _unrecoveredV2Editors.Remove(document.Id);
                    _adoptions.Remove(document.Id);
                }
                return new SessionSaveResult(true, true);
            }
        }
        finally
        {
            foreach (var failure in await draft.RollbackAsync())
                LoggingService.LogError($"[{nameof(SessionService)}] Failed to retire an unpublished permission: {failure}");
        }
    }

    private async Task EnsureWriterLeaseAsync(CancellationToken cancellation)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        if (_writerLease == null) _writerLease = await SessionScopeLease.AcquireWriterAsync(_sessionScope.OwnerId, cancellation);
    }

    private static async Task<bool> HasVerifiedRescueAsync(RecoveryRecordStore store, RecoveryArea area,
        RecoveryStamp epoch, TextEditorSessionDataV2 editor, CancellationToken cancellation)
    {
        var scan = store.Enumerate(area);
        if (!scan.IsComplete || scan.Entries.Count == 0) return false;
        var address = scan.Entries.MaxBy(candidate => candidate.Ordinal);
        var read = await store.ReadAsync(address, repair: true, cancellationToken: cancellation);
        return read.State == RecoveryReadState.Valid &&
            read.Record.Kind == RecoveryRecordKind.Rescue &&
            SessionRecoveryAuthority.SameEpoch(read.Record.Stamp, epoch) &&
            read.Record.Editor.ComputeSha256() == editor.ComputeSha256();
    }

    private async Task PublishAdoptionAsync(RecoveryRecordStore store, RecoveryStamp expected,
        TextEditorSessionDataV2 target, RecoveryAdoptionOrigin origin, CancellationToken cancellation)
    {
        if (target.Journal.OwnerId != expected.ScopeId || target.SavedBaseline.OwnerId != expected.ScopeId ||
            target.RecoveryBaseline.OwnerId != expected.ScopeId)
        {
            throw new InvalidDataException("Adoption requires the actual independently owned target capture.");
        }

        var currentTarget = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false);
        SessionRecoveryAuthority.RequireCurrent(expected, currentTarget);
        var sourceCutoff = origin.Source?.ConsumptionCutoffs.TryGetValue(origin.SourceEditor.Id, out var frozenCutoff) == true ?
            frozenCutoff : origin.SourceEditor?.CaptureRevision ?? 0;
        var priorAdoption = currentTarget.Decisions.FirstOrDefault(read => read.Record.Editor?.Id == target.Id &&
            SessionRecoveryAuthority.SameEpoch(expected, read.Record.Stamp) &&
            read.Record.Kind is RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive &&
            read.Record.Consumptions.Any(key => origin.Source == null ?
                key.Kind == RecoveryConsumptionKind.Legacy && string.Equals(key.LegacyFingerprint, origin.LegacyFingerprint,
                    StringComparison.OrdinalIgnoreCase) && key.SourceEditorId == target.Id :
                key.Kind == RecoveryConsumptionKind.Inactive && key.SourceScopeId == origin.Source.ExpectedStamp.ScopeId &&
                key.SourceEpochId == origin.Source.ExpectedStamp.EpochId && key.SourceEditorId == origin.SourceEditor.Id &&
                key.SourceCaptureRevision == sourceCutoff));
        if (priorAdoption != null) return;
        var area = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Decisions);
        var record = new RecoveryRecord
        {
            Kind = origin.Source == null ? RecoveryRecordKind.AdoptLegacy : RecoveryRecordKind.AdoptInactive,
            Stamp = store.CreatePublicationStamp(area, expected),
            Editor = target
        };
        if (origin.Source == null)
        {
            record.Consumptions.Add(new RecoveryConsumption
            {
                Kind = RecoveryConsumptionKind.Legacy,
                LegacyFingerprint = origin.LegacyFingerprint,
                SourceEditorId = target.Id
            });
        }
        else
        {
            var source = origin.Source;
            var current = await SessionRecoveryCatalog.ReadScopeAsync(source.ExpectedStamp.ScopeId, cancellation);
            SessionRecoveryAuthority.RequireCurrent(source.ExpectedStamp, current);
            if (source.RootAddress != null)
            {
                var exact = await store.ReadAsync(source.RootAddress, true, cancellation);
                if (exact.State != RecoveryReadState.Valid || exact.PayloadSha256 != source.RootSha256)
                    throw new InvalidDataException("The inactive source checkpoint changed before adoption.");
            }
            var candidate = current.Session.TextEditors.Concat(current.EligibleRescues.Select(read => read.Record.Editor))
                .FirstOrDefault(editor => editor.Id == origin.SourceEditor.Id);
            if (candidate == null || candidate.ComputeSha256() != origin.SourceEditor.ComputeSha256())
                throw new InvalidDataException("The inactive source incarnation changed before adoption.");
            // A partially published newer capture may not have entered the
            // selected workspace. Preserve its exact source descriptor before
            // consuming the whole observed incarnation history.
            if (source.UnselectedNewerDescriptors.TryGetValue(origin.SourceEditor.Id, out var newerDescriptors))
            {
                foreach (var descriptor in newerDescriptors)
                {
                    var pendingArea = store.GetScopeArea(source.ExpectedStamp.ScopeId, RecoveryAreaKind.Pending, descriptor.Id);
                    if (current.Pending.Any(read => read.State == RecoveryReadState.Valid &&
                        read.Record.Editor.ComputeSha256() == descriptor.ComputeSha256()))
                    {
                        continue;
                    }

                    RecoveryRootStore.RequireCommitted(await store.PublishAsync(pendingArea, new RecoveryRecord
                    {
                        Kind = RecoveryRecordKind.Pending,
                        Stamp = store.CreatePublicationStamp(pendingArea, source.ExpectedStamp),
                        Editor = descriptor
                    }, cancellation));
                }
            }

            record.SourceStamp = SessionRecoveryAuthority.CopyStamp(source.ExpectedStamp);
            record.Consumptions.Add(new RecoveryConsumption
            {
                Kind = RecoveryConsumptionKind.Inactive,
                SourceScopeId = source.ExpectedStamp.ScopeId,
                SourceEpochId = source.ExpectedStamp.EpochId,
                SourceEditorId = origin.SourceEditor.Id,
                SourceCaptureRevision = sourceCutoff
            });
        }
        RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, record, cancellation));
    }

    public async Task CommitAttachedAsync(SessionCapture capture, IEnumerable<Guid> ids, CancellationToken cancellation)
    {
        var attached = new HashSet<Guid>(ids);
        if (attached.Any(id => capture.Documents.All(document => document.Id != id)))
            throw new ArgumentException("Attachment capture omits an attached recovery document.");
        // Publication both consumes imported rows and anchors rescue content
        // only after the editor has completed the native journal fork.
        await PublishCaptureAsync(capture, cancellation);
    }

    public async Task<bool> PrepareExplicitCloseAsync(Guid editorId, CancellationToken cancellation = default)
    {
        await InitializeAuthorityAsync(cancellation);
        var expected = CurrentStamp;
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation);
            SessionRecoveryAuthority.RequireCurrent(expected, authority);
            if (SessionRecoveryAuthority.IsClosed(expected, editorId, authority.Decisions.Select(read => read.Record))) return true;
            var durable = authority.Decisions.Concat(authority.GlobalRecords).Any(read => read.Record.Editor?.Id == editorId &&
                read.Record.Kind is RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive or RecoveryRecordKind.TransferReceipt or RecoveryRecordKind.TransferOffer &&
                SessionRecoveryAuthority.SameEpoch(read.Record.Kind == RecoveryRecordKind.TransferReceipt ? read.Record.TargetStamp : read.Record.Stamp, expected));
            durable |= authority.Rescues.Any(read => read.State == RecoveryReadState.Valid &&
                read.Record.Editor.Id == editorId && SessionRecoveryAuthority.SameEpoch(read.Record.Stamp, expected));
            if (!durable) return true;
            var store = _recordStore;
            var area = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Decisions);
            RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
            { Kind = RecoveryRecordKind.Closed, Stamp = store.CreatePublicationStamp(area, expected), ClosedEditorId = editorId }, cancellation));
            return true;
        }
    }

    public async Task ClearSessionDataAsync(CancellationToken cancellation = default)
    {
        await InitializeAuthorityAsync(cancellation);
        var expected = CurrentStamp;
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false);
            SessionRecoveryAuthority.RequireCurrent(expected, authority);
            foreach (var record in authority.Decisions.Select(read => read.Record))
            {
                _acceptedLegacySources.UnionWith(record.ResetAcceptedLegacySources);
                _acceptedLegacySources.UnionWith(record.Consumptions.Where(key => key.Kind == RecoveryConsumptionKind.Legacy)
                    .Select(key => key.LegacyFingerprint));
            }
            var (_, fingerprint, _) = await SessionManifestStore.ReadLegacyAsync(_sessionMetaDataFileName, cancellation);
            if (fingerprint != null) _acceptedLegacySources.Add(fingerprint);
            var store = _recordStore;
            if (UnrecoveredEditorCount != 0)
            {
                var archiveArea = store.GetGlobalArea(RecoveryAreaKind.Archives, Guid.NewGuid());
                var archive = new NotepadsSessionDataV2
                {
                    Scope = _sessionScope,
                    TextEditors = _unrecoveredV2Editors.Values.ToList(),
                    UnrecoveredLegacyEditors = _unrecoveredEditors.Values.ToList()
                };
                RecoveryRootStore.RequireCommitted(await store.PublishAsync(archiveArea, new RecoveryRecord
                { Kind = RecoveryRecordKind.Archive, Stamp = store.CreatePublicationStamp(archiveArea, expected), Session = archive }, cancellation));
            }
            var decisions = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Decisions);
            var stamp = store.CreatePublicationStamp(decisions, expected);
            stamp.EpochId = Guid.NewGuid();
            var reset = new RecoveryRecord
            {
                Kind = RecoveryRecordKind.Reset,
                Stamp = stamp,
                PreviousEpochId = expected.EpochId,
                ResetAcceptedLegacySources = _acceptedLegacySources.ToList()
            };
            RecoveryRootStore.RequireCommitted(await store.PublishResetIntentAsync(decisions, reset, cancellation));
            RecoveryRootStore.RequireCommitted(await store.PublishAsync(decisions, reset, CancellationToken.None));
            _currentStamp = new RecoveryStamp { ScopeId = expected.ScopeId, EpochId = stamp.EpochId };
            _committedSessionData = null; _lastSessionJson = null;
            _unrecoveredEditors.Clear(); _unrecoveredV2Editors.Clear(); _adoptions.Clear();
            foreach (var source in _importedRecoverySources) source.Dispose();
            _importedRecoverySources.Clear();
            _sessionMetadataRetained = true;
            RecoveryOutcome = SessionRecoveryOutcome.Absent;
        }
    }

    public async Task FinishPublicationAsync(SessionSaveResult result, CancellationToken cancellation)
    {
        if (!result.Succeeded || !result.Changed) return;
        try
        {
            using (await SessionRecoveryTransaction.EnterAsync(cancellation))
                await SessionCheckpointRetention.PruneAsync(_sessionScope.OwnerId, cancellation);
            await SessionRecoveryGarbageCollector.CollectAsync(_sessionScope.OwnerId,
                SessionDocumentStore.GetFutureAccessTokenPrefix(_sessionScope.OwnerId),
                token => SessionRecoveryCatalog.ReadReferencesAsync(_backupFolderName, token), cancellation);
        }
        catch (Exception ex) { LoggingService.LogError($"[{nameof(SessionService)}] Recovery maintenance was deferred: {ex}"); }
    }

    public Task<StorageFolder> GetBackupFolderAsync() => SessionManifestStore.GetBackupFolderAsync(_backupFolderName);
    public async Task<int> RecoverBackupFilesAsync(CancellationToken cancellation)
    {
        var recovered = 0;
        foreach (var file in await SessionManifestStore.GetAllFilesInBackupFolderAsync(_backupFolderName))
        {
            cancellation.ThrowIfCancellationRequested();
            if (file.Name.Contains('.')) continue;
            // Manual V1 salvage creates readable copies. Original paths
            // remain valid for the preserved migration reference graph.
            await file.CopyAsync(await GetBackupFolderAsync(),
                file.Name + "-Recovered-" + Guid.NewGuid().ToString("N") + ".txt", NameCollisionOption.FailIfExists);
            recovered++;
        }
        return recovered;
    }
    public Task DisposeAsync()
    {
        if (_disposed) return Task.CompletedTask;
        _disposed = true;
        foreach (var source in _importedRecoverySources) source.Dispose();
        _importedRecoverySources.Clear();
        _writerLease?.Dispose(); _writerLease = null;
        return Task.CompletedTask;
    }

    private sealed class RecoveryAdoptionOrigin
    {
        public string LegacyFingerprint { get; init; }
        public SessionRecoverySource Source { get; init; }
        public TextEditorSessionDataV2 SourceEditor { get; init; }
    }
}
