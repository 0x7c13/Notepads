// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
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
    private readonly string _sessionMetaDataFileName;
    private readonly Func<IReadOnlyCollection<Guid>> _getLiveInstances;
    private readonly RecoveryRecordStore _recordStore;
    private readonly HashSet<string> _acceptedLegacySources = new(StringComparer.OrdinalIgnoreCase);
    // The grant last made or verified for each editing file instance an editor holds.
    private readonly ConditionalWeakTable<StorageFile, string> _fileGrants = new();
    private NotepadsSessionDataV2 _committedSessionData;
    private RecoveryStamp _currentStamp;
    private IDisposable _writerLease;
    private bool _disposed;
    private string _lastSessionJson;

    public SessionService(SessionScopeData scope, string manifestFileName,
        Func<IReadOnlyCollection<Guid>> getLiveInstances, RecoveryRecordStore recordStore = null)
    {
        scope.Validate();
        _sessionScope = scope;
        _sessionMetaDataFileName = manifestFileName;
        _getLiveInstances = getLiveInstances ?? throw new ArgumentNullException(nameof(getLiveInstances));
        _recordStore = recordStore ?? RecoveryRootStore.CreateStore();
        if (!string.Equals(_recordStore.Paths.RootPath, RecoveryRootStore.CreateStore().Paths.RootPath, StringComparison.OrdinalIgnoreCase))
            throw new ArgumentException("The metadata repository must use this app's recovery admission root.", nameof(recordStore));
    }

    public bool IsBackupEnabled { get; set; }
    public SessionRecoveryOutcome RecoveryOutcome { get; private set; } = SessionRecoveryOutcome.Absent;
    // Whether the latest authority read (startup, save, adoption, close or clear) found recovery blocked;
    // saves and adoptions never update RecoveryOutcome.
    public bool IsRecoveryBlocked { get; private set; }
    public RecoveryStamp CurrentStamp => SessionRecoveryAuthority.CopyStamp(_currentStamp) ??
        throw new InvalidOperationException("Recovery authority must be initialized before document capture.");

    public async Task InitializeAuthorityAsync(CancellationToken cancellation = default)
    {
        await EnsureWriterLeaseAsync(cancellation);
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            BindAuthority(await SessionRecoveryCatalog.ReadScopeAsync(_sessionScope.OwnerId, cancellation,
                content: _committedSessionData == null));
    }

    private void BindAuthority(RecoveryScopeSnapshot authority)
    {
        IsRecoveryBlocked = authority.Blocked;
        if (authority.Blocked)
        {
            RecoveryOutcome = SessionRecoveryOutcome.Blocked;
            throw new SessionDataCorruptedException(authority.AttentionReason);
        }
        _currentStamp = authority.Stamp;
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
            // While the editor holds the instance this grant was made for, the grant still names
            // its file, even if that file was since deleted or went offline. No broker call needed.
            if (_fileGrants.TryGetValue(file, out var granted) && granted == token) return token;
            var committedFile = await FutureAccessListUtility.GetFileFromFutureAccessListAsync(token);
            if (committedFile != null && file.IsEqual(committedFile))
            {
                _fileGrants.AddOrUpdate(file, token);
                return token;
            }
        }
        token = SessionDocumentStore.TryRegisterFileAccess(editorId, file, _sessionScope.OwnerId, draft);
        if (token != null) _fileGrants.AddOrUpdate(file, token);
        return token;
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
        if (expected.ScopeId != _sessionScope.OwnerId) throw new InvalidDataException("Capture belongs to another recovery scope.");
        if (capture.Documents.Select(document => document.Id).Distinct().Count() != capture.Documents.Count)
            throw new InvalidDataException("A capture must contain each tab exactly once.");
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
            {
                ValidateCapture(document, expected);
                session.TextEditors.Add(document.IsRetained ? document.Source.Descriptor :
                    await BuildTextEditorSessionDataAsync(document, cancellation));
            }
            var sessionEditors = session.TextEditors.ToDictionary(editor => editor.Id);
            if (session.SelectedTextEditor != Guid.Empty && !sessionEditors.ContainsKey(session.SelectedTextEditor))
                throw new InvalidDataException("The selected tab is missing from the complete capture.");
            using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            {
                var pass = new RecoveryCatalogPass();
                // Before this service admits a complete checkpoint, unreadable rescue-only
                // history must not become an apparently empty replacement workspace.
                var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation,
                    content: _committedSessionData == null, pass);
                IsRecoveryBlocked = authority.Blocked;
                SessionRecoveryAuthority.RequireCurrent(expected, authority);
                session.PendingRescues = await ValidatePendingRescuesAsync(capture, authority, cancellation);
                var committedEditors = _committedSessionData?.TextEditors.ToDictionary(editor => editor.Id) ?? [];
                foreach (var document in capture.Documents)
                {
                    if (SessionRecoveryAuthority.IsClosed(expected, document.Id, authority.Decisions.Select(read => read.Record)))
                        throw new InvalidDataException("A closed tab cannot be republished.");
                    if (document.IsRetained) continue;
                    var data = sessionEditors[document.Id];
                    committedEditors.TryGetValue(document.Id, out var committed);
                    if (document.EditingFile != null)
                        data.EditingFileFutureAccessToken = await GetCapturedFileAccessTokenAsync(document, committed, draft);
                    RetainCaptureRevision(data, committed);
                }
                var json = JsonSerializer.Serialize(session, SessionJsonContext.Default.NotepadsSessionDataV2);
                if (json == _lastSessionJson && !capture.Documents.Any(document => document.Source?.Origin != null))
                {
                    draft.Commit();
                    return CommittedResult(false, session);
                }
                var store = _recordStore;
                draft.PreserveForRecovery();
                foreach (var document in capture.Documents) document.Recovery?.PreserveForRecovery();
                foreach (var document in capture.Documents)
                {
                    var editor = sessionEditors[document.Id];
                    if (document.Source?.Origin is { } origin)
                    {
                        document.Source.RequireAvailable();
                        await PublishAdoptionAsync(store, expected, editor, origin, pass, cancellation);
                    }
                    var area = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Rescue, editor.Id);
                    if (committedEditors.TryGetValue(editor.Id, out var committed) &&
                        committed.ComputeSha256() == editor.ComputeSha256() &&
                        await HasVerifiedRescueAsync(store, area, expected, editor, cancellation)) continue;
                    RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
                    {
                        Kind = RecoveryRecordKind.Rescue,
                        Stamp = store.CreatePublicationStamp(area, expected), Editor = editor
                    }, cancellation));
                }
                var checkpointArea = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Checkpoints);
                RecoveryRootStore.RequireCommitted(await store.PublishAsync(checkpointArea, new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.Checkpoint,
                    Stamp = store.CreatePublicationStamp(checkpointArea, expected), Session = session
                }, cancellation));
                draft.Commit();
                _committedSessionData = session;
                _lastSessionJson = json;
                return CommittedResult(true, session);
            }
        }
        finally
        {
            foreach (var failure in await draft.RollbackAsync())
                LoggingService.LogError($"[{nameof(SessionService)}] Failed to retire an unpublished permission: {failure}");
        }
    }

    private static SessionSaveResult CommittedResult(bool changed, NotepadsSessionDataV2 session) =>
        new(true, changed, session.TextEditors.ToDictionary(editor => editor.Id, SessionRecoveryCatalog.CloneEditor));

    private static void ValidateCapture(SessionDocumentCapture document, RecoveryStamp expected)
    {
        if (document.Id == Guid.Empty || document.Recovery == null && !document.IsRetained)
            throw new InvalidDataException("A tab must have one native or retained recovery representation.");
        if (document.Source != null && document.Source.Id != document.Id)
            throw new InvalidDataException("The captured source belongs to another tab.");
        if (document.Source != null && !SessionRecoveryAuthority.SameEpoch(expected, document.Source.TargetStamp))
            throw new InvalidDataException("The captured source belongs to another target recovery epoch.");
        if (document.IsRetained && (!document.Source.IsOwned ||
            !SessionRecoveryAuthority.SameEpoch(expected, document.Source.ExpectedStamp)))
            throw new InvalidDataException("Retained tabs require an owned source in the captured epoch.");
        if (document.Source?.IsOwned == true && !SessionRecoveryAuthority.SameEpoch(expected, document.Source.ExpectedStamp))
            throw new InvalidDataException("A native capture retained an obsolete recovery association.");
    }

    private Task<string> GetCapturedFileAccessTokenAsync(SessionDocumentCapture document,
        TextEditorSessionDataV2 committed, SessionGenerationDraft draft)
    {
        var binding = document.FileAccessBinding;
        var prefix = SessionDocumentStore.GetFutureAccessTokenPrefix(_sessionScope.OwnerId) + document.Id.ToString("N") + "-";
        if (binding?.File != null && ReferenceEquals(binding.File, document.EditingFile) &&
            (document.Source?.IsOwned == true || committed?.EditingFileFutureAccessToken == binding.Token) &&
            binding.Token?.StartsWith(prefix, StringComparison.Ordinal) == true &&
            binding.Token.Length == prefix.Length + 32 && Guid.TryParseExact(binding.Token.AsSpan(prefix.Length), "N", out var grant) &&
            grant != Guid.Empty)
        {
            _fileGrants.AddOrUpdate(document.EditingFile, binding.Token);
            return Task.FromResult(binding.Token);
        }
        return GetFileAccessTokenAsync(document.Id, committed, document.EditingFile, draft);
    }

    /// <summary>The adoption decision itself contains durable independently owned content. No fallible write follows it.</summary>
    public async Task<TextEditorSessionDataV2> PublishAdoptionAsync(SessionDocumentCapture document,
        RecoveryStamp expected, CancellationToken cancellation = default)
    {
        expected = SessionRecoveryAuthority.CopyStamp(expected) ?? throw new InvalidDataException("Adoption has no target epoch binding.");
        ValidateCapture(document, expected);
        if (document.Recovery == null || document.Source?.Origin == null)
            throw new InvalidDataException("An import requires a native capture and its frozen provenance.");
        document.Source.RequireAvailable();
        if (expected.ScopeId != _sessionScope.OwnerId) throw new InvalidDataException("The import target belongs to another scope.");
        var draft = new SessionGenerationDraft();
        try
        {
            var editor = await BuildTextEditorSessionDataAsync(document, cancellation);
            using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            {
                var pass = new RecoveryCatalogPass();
                var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation,
                    content: _committedSessionData == null, pass);
                IsRecoveryBlocked = authority.Blocked;
                SessionRecoveryAuthority.RequireCurrent(expected, authority);
                if (SessionRecoveryAuthority.IsClosed(expected, document.Id, authority.Decisions.Select(read => read.Record)))
                    throw new InvalidDataException("A closed import cannot be adopted again.");
                if (document.EditingFile != null)
                    editor.EditingFileFutureAccessToken = await GetCapturedFileAccessTokenAsync(document, null, draft);
                draft.PreserveForRecovery();
                document.Recovery.PreserveForRecovery();
                var adopted = await PublishAdoptionAsync(_recordStore, expected, editor, document.Source.Origin, pass, cancellation);
                draft.Commit();
                return adopted;
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

    private async Task<TextEditorSessionDataV2> PublishAdoptionAsync(RecoveryRecordStore store, RecoveryStamp expected,
        TextEditorSessionDataV2 target, RecoveryAdoptionOrigin origin, RecoveryCatalogPass pass, CancellationToken cancellation)
    {
        if (target.Journal.OwnerId != expected.ScopeId || target.SavedBaseline.OwnerId != expected.ScopeId ||
            target.RecoveryBaseline.OwnerId != expected.ScopeId)
        {
            throw new InvalidDataException("Adoption requires the actual independently owned target capture.");
        }

        var currentTarget = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false, pass);
        SessionRecoveryAuthority.RequireCurrent(expected, currentTarget);
        var sourceCutoff = origin.ConsumptionCutoff;

        var priorAdoption = currentTarget.Decisions.FirstOrDefault(read => read.Record.Editor?.Id == target.Id &&
            SessionRecoveryAuthority.SameEpoch(expected, read.Record.Stamp) &&
            read.Record.Kind is RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive &&
            read.Record.Consumptions.Any(key => origin.SourceStamp == null ?
                key.Kind == RecoveryConsumptionKind.Legacy && string.Equals(key.LegacyFingerprint, origin.LegacyFingerprint,
                    StringComparison.OrdinalIgnoreCase) && key.SourceEditorId == target.Id :
                key.Kind == RecoveryConsumptionKind.Inactive && key.SourceScopeId == origin.SourceStamp.ScopeId &&
                key.SourceEpochId == origin.SourceStamp.EpochId && key.SourceEditorId == origin.Decision.Id &&
                key.SourceCaptureRevision == sourceCutoff));
        if (priorAdoption != null) return SessionRecoveryCatalog.CloneEditor(priorAdoption.Record.Editor);
        var area = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Decisions);
        var record = new RecoveryRecord
        {
            Kind = origin.SourceStamp == null ? RecoveryRecordKind.AdoptLegacy : RecoveryRecordKind.AdoptInactive,
            Stamp = store.CreatePublicationStamp(area, expected),
            Editor = target
        };
        if (origin.SourceStamp == null)
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
            var source = origin;
            var current = await SessionRecoveryCatalog.ReadScopeAsync(source.SourceStamp.ScopeId, cancellation, pass: pass);
            IsRecoveryBlocked = current.Blocked;
            SessionRecoveryAuthority.RequireCurrent(source.SourceStamp, current);
            if (source.RootAddress != null)
            {
                var exact = await store.ReadAsync(source.RootAddress, true, cancellation);
                if (exact.State != RecoveryReadState.Valid || exact.PayloadSha256 != source.RootSha256)
                    throw new InvalidDataException("The inactive source checkpoint changed before adoption.");
            }
            var candidateHash = current.Selection.Documents.FirstOrDefault(document => document.Id == source.Decision.Id)
                ?.DescriptorSha256;
            // Another own tab can publish the new workspace while this queued rescue is loading.
            // Its exact pinned rescue remains valid even if a caller omitted it from the new index.
            if (source.SourceStamp.ScopeId == expected.ScopeId && source.RootAddress?.Area.Kind == RecoveryAreaKind.Rescue &&
                SessionRecoveryAuthority.IsEligible(source.SourceStamp, source.SourceEditor, current.Stamp,
                    current.Decisions.Select(read => read.Record), current.AllDecisions))
                candidateHash = current.CurrentDescriptors(source.Decision.Id)
                    .Any(editor => editor.ComputeSha256() == source.Decision.DescriptorSha256) ? source.Decision.DescriptorSha256 : null;
            if (candidateHash != source.Decision.DescriptorSha256)
                throw new InvalidDataException("The inactive source incarnation changed before adoption.");
            // A partially published newer capture may not have entered the
            // selected workspace. Preserve its exact source descriptor before
            // consuming the whole observed incarnation history.
            foreach (var protectedVersion in source.Decision.ProtectedNewer)
            {
                var descriptor = protectedVersion.Descriptor;
                var pendingArea = store.GetScopeArea(source.SourceStamp.ScopeId, RecoveryAreaKind.Pending, descriptor.Id);
                if (current.Pending.Any(read => read.State == RecoveryReadState.Valid &&
                    read.Record.Editor.ComputeSha256() == protectedVersion.DescriptorSha256))
                {
                    continue;
                }

                RecoveryRootStore.RequireCommitted(await store.PublishAsync(pendingArea, new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.Pending,
                    Stamp = store.CreatePublicationStamp(pendingArea, source.SourceStamp),
                    Editor = descriptor
                }, cancellation));
            }

            record.SourceStamp = SessionRecoveryAuthority.CopyStamp(source.SourceStamp);
            record.Consumptions.Add(new RecoveryConsumption
            {
                Kind = RecoveryConsumptionKind.Inactive,
                SourceScopeId = source.SourceStamp.ScopeId,
                SourceEpochId = source.SourceStamp.EpochId,
                SourceEditorId = origin.Decision.Id,
                SourceCaptureRevision = sourceCutoff
            });
        }
        // Whatever the outcome, later reads in this pass must see the target's decisions afresh.
        pass.Invalidate(expected.ScopeId);
        RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, record, cancellation));
        return SessionRecoveryCatalog.CloneEditor(target);
    }

    /// <summary>
    /// Record explicit tab closes. Each editor whose close is confirmed, already Closed or newly committed, is added to
    /// <paramref name="closed"/>, so a caller can remove those tabs if a later one fails. An indeterminate publication
    /// throws without adding its editor, so that tab stays open.
    /// </summary>
    public async Task<bool> PrepareExplicitCloseAsync(IReadOnlyCollection<Guid> editorIds, ICollection<Guid> closed = null,
        CancellationToken cancellation = default)
    {
        await EnsureWriterLeaseAsync(cancellation);
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            // One fresh full read both binds authority, as InitializeAuthorityAsync does, and supplies
            // the durable-content evidence; any corruption it reports still vetoes the close.
            var authority = await SessionRecoveryCatalog.ReadScopeAsync(_sessionScope.OwnerId, cancellation);
            BindAuthority(authority);
            var expected = CurrentStamp;
            var store = _recordStore;
            var area = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Decisions);
            foreach (var editorId in editorIds)
            {
                if (SessionRecoveryAuthority.IsClosed(expected, editorId, authority.Decisions.Select(read => read.Record)))
                {
                    closed?.Add(editorId);
                    continue;
                }
                var durable = authority.Decisions.Concat(authority.GlobalRecords).Any(read => read.Record.Editor?.Id == editorId &&
                    read.Record.Kind is RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive or RecoveryRecordKind.TransferReceipt or RecoveryRecordKind.TransferOffer &&
                    SessionRecoveryAuthority.SameEpoch(read.Record.Kind == RecoveryRecordKind.TransferReceipt ? read.Record.TargetStamp : read.Record.Stamp, expected));
                durable |= authority.Rescues.Any(read => read.State == RecoveryReadState.Valid &&
                    read.Record.Editor.Id == editorId && SessionRecoveryAuthority.SameEpoch(read.Record.Stamp, expected));
                durable |= authority.Checkpoints.Any(read => read.State == RecoveryReadState.Valid &&
                    SessionRecoveryAuthority.SameEpoch(read.Record.Stamp, expected) &&
                    read.Record.Session.TextEditors.Any(editor => editor.Id == editorId));
                if (!durable) continue;
                var published = await store.PublishAsync(area, new RecoveryRecord
                { Kind = RecoveryRecordKind.Closed, Stamp = store.CreatePublicationStamp(area, expected), ClosedEditorId = editorId }, cancellation);
                if (published.IsCommitted) closed?.Add(editorId);
                RecoveryRootStore.RequireCommitted(published);
            }
            return true;
        }
    }

    /// <summary>Preserve explicitly unclaimed sources before replacing the workspace index.</summary>
    public async Task ArchiveRecoverySourcesAsync(IEnumerable<RecoveryDocumentSource> sources,
        CancellationToken cancellation = default)
    {
        var unconsumed = sources.ToArray();
        if (unconsumed.Length == 0) return;
        await InitializeAuthorityAsync(cancellation);
        var expected = CurrentStamp;
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false);
            IsRecoveryBlocked = authority.Blocked;
            SessionRecoveryAuthority.RequireCurrent(expected, authority);
            await ArchiveSourcesAsync(unconsumed, expected, cancellation);
            // Archive first. Partial/uncertain decisions can be retried from the same frozen plan.
            var decisions = _recordStore.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Decisions);
            foreach (var source in unconsumed.Where(source => source.IsOwned || source.IsOwnScopeRescue))
            {
                var editorId = source.IsOwned ? source.Id : source.Origin.Decision.Id;
                if (SessionRecoveryAuthority.IsClosed(expected, editorId, authority.Decisions.Select(read => read.Record))) continue;
                RecoveryRootStore.RequireCommitted(await _recordStore.PublishAsync(decisions, new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.Closed,
                    Stamp = _recordStore.CreatePublicationStamp(decisions, expected), ClosedEditorId = editorId
                }, cancellation));
            }
            _lastSessionJson = null;
        }
    }

    private async Task ArchiveSourcesAsync(IReadOnlyCollection<RecoveryDocumentSource> sources,
        RecoveryStamp expected, CancellationToken cancellation)
    {
        if (sources.Count == 0) return;
        foreach (var source in sources)
        {
            if (!SessionRecoveryAuthority.SameEpoch(source.TargetStamp, expected))
                throw new InvalidDataException("An obsolete recovery source cannot be archived as the current workspace.");
        }
        var area = _recordStore.GetGlobalArea(RecoveryAreaKind.Archives, Guid.NewGuid());
        var archive = new NotepadsSessionDataV2
        {
            Scope = _sessionScope,
            TextEditors = sources.Select(source => source.Origin?.SourceEditor ?? source.Descriptor)
                .Where(editor => editor != null).Select(SessionRecoveryCatalog.CloneEditor)
                .DistinctBy(editor => (editor.Id, editor.ComputeSha256())).ToList(),
            UnrecoveredLegacyEditors = sources.Select(source => source.LegacyDescriptor)
                .Where(editor => editor != null).ToList()
        };
        RecoveryRootStore.RequireCommitted(await _recordStore.PublishAsync(area, new RecoveryRecord
        {
            Kind = RecoveryRecordKind.Archive,
            Stamp = _recordStore.CreatePublicationStamp(area, expected), Session = archive
        }, cancellation));
    }

    public async Task ClearSessionDataAsync(IEnumerable<RecoveryDocumentSource> unconsumedSources,
        CancellationToken cancellation = default)
    {
        var unconsumed = unconsumedSources.ToArray();
        await InitializeAuthorityAsync(cancellation);
        var expected = CurrentStamp;
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            var authority = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false);
            IsRecoveryBlocked = authority.Blocked;
            SessionRecoveryAuthority.RequireCurrent(expected, authority);
            AcceptLegacyDecisions(authority);
            var (_, fingerprint, _) = await SessionManifestStore.ReadLegacyAsync(_sessionMetaDataFileName, cancellation);
            if (fingerprint != null) _acceptedLegacySources.Add(fingerprint);
            await ArchiveSourcesAsync(unconsumed, expected, cancellation);
            var store = _recordStore;
            var decisions = store.GetScopeArea(expected.ScopeId, RecoveryAreaKind.Decisions);
            var stamp = store.CreatePublicationStamp(decisions, expected);
            stamp.EpochId = Guid.NewGuid();
            var reset = new RecoveryRecord
            {
                Kind = RecoveryRecordKind.Reset,
                Stamp = stamp,
                PreviousEpochId = expected.EpochId,
                ResetAcceptedLegacySources = _acceptedLegacySources.ToList(),
                ResetRetiredContentOperations = CollectResetContentOperations(store, expected.ScopeId, cancellation)
            };
            try
            {
                RecoveryRootStore.RequireCommitted(await store.PublishResetIntentAsync(decisions, reset, cancellation));
                // A committed intent kills the old epoch, even if publishing its final decision later fails.
                AcceptResetEpoch(new RecoveryStamp { ScopeId = expected.ScopeId, EpochId = stamp.EpochId });
                RecoveryRootStore.RequireCommitted(await store.PublishAsync(decisions, reset, CancellationToken.None));
            }
            catch
            {
                // A renamed intent may have lost its acknowledgement. Reconcile only verified authority;
                // never resume using an old epoch merely because the original write threw or was cancelled.
                try
                {
                    var actual = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, CancellationToken.None, content: false);
                    var previous = CurrentStamp;
                    BindAuthority(actual);
                    if (!SessionRecoveryAuthority.SameEpoch(previous, actual.Stamp)) AcceptResetEpoch(actual.Stamp);
                }
                catch (Exception error)
                {
                    LoggingService.LogError($"[{nameof(SessionService)}] Reset authority could not be reconciled: {error}");
                }
                throw;
            }
        }
    }

    private static List<Guid> CollectResetContentOperations(RecoveryRecordStore store, Guid owner,
        CancellationToken cancellation)
    {
        // Freeze exact operations under the reset's store admission. Per-directory
        // ordinals cannot prove a reset boundary and can be reused after pruning.
        var operations = new HashSet<Guid>();
        foreach (var kind in new[] { RecoveryAreaKind.Checkpoints, RecoveryAreaKind.Rescue })
        {
            var areas = store.EnumerateScopeAreas(owner, kind);
            if (!areas.IsComplete) throw new IOException("Cannot reset an incompletely enumerated content directory.", areas.Error);
            foreach (var area in areas.Entries)
            {
                cancellation.ThrowIfCancellationRequested();
                var roots = store.Enumerate(area);
                if (!roots.IsComplete) throw new IOException("Cannot reset incompletely enumerated content operations.", roots.Error);
                operations.UnionWith(roots.Entries.Select(address => address.OperationId));
            }
        }
        return operations.OrderBy(operation => operation).ToList();
    }

    private void AcceptResetEpoch(RecoveryStamp stamp)
    {
        _currentStamp = SessionRecoveryAuthority.CopyStamp(stamp);
        _committedSessionData = null;
        _lastSessionJson = null;
        _fileGrants.Clear();
        RecoveryOutcome = SessionRecoveryOutcome.Absent;
    }
    /// <summary>
    /// Prune content history to its retention bound, then collect unreferenced assets and grants, in one admission.
    /// Publication never does this itself; the caller schedules it. Without this scope's writer lease it does nothing.
    /// </summary>
    public async Task RunMaintenanceAsync(CancellationToken cancellation)
    {
        if (_disposed || _writerLease == null) return;
        using var measurement = OperationMetrics.Measure("session.maintenance");
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            // Pruning, here and of retired scopes during GC, deletes only checkpoint and rescue records,
            // so the lifecycle reads stay valid for the whole pass.
            var pass = new RecoveryCatalogPass();
            await SessionCheckpointRetention.PruneAsync(_sessionScope.OwnerId, cancellation, pass);
            await SessionRecoveryGarbageCollector.CollectAsync(_sessionScope.OwnerId,
                SessionDocumentStore.GetFutureAccessTokenPrefix(_sessionScope.OwnerId),
                token => SessionRecoveryCatalog.ReadReferencesAsync(token, pass), cancellation, pass: pass);
        }
    }

    public Task DisposeAsync()
    {
        if (_disposed) return Task.CompletedTask;
        _disposed = true;
        _writerLease?.Dispose(); _writerLease = null;
        return Task.CompletedTask;
    }

}
