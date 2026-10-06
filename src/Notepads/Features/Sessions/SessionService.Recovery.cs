// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;

namespace Notepads.Features.Sessions;

internal sealed partial class SessionService
{
    public async Task<PreparedRecoveryBatch> PrepareRecoveryAsync(DocumentLoadOptions defaults,
        CancellationToken cancellation)
    {
        using var measurement = OperationMetrics.Measure("session.prepare");
        await EnsureMetadataRetainedAsync(cancellation);
        var batch = new PreparedRecoveryBatch
        {
            ExpectedStamp = CurrentStamp,
            SelectedEditorId = _recoverySnapshot.Session.SelectedTextEditor,
            TabScrollOffset = _recoverySnapshot.Session.TabScrollViewerHorizontalOffset
        };
        try
        {
            using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            {
                var authority = await SessionRecoveryCatalog.ReadScopeAsync(_sessionScope.OwnerId, cancellation, content: false);
                SessionRecoveryAuthority.RequireCurrent(batch.ExpectedStamp, authority);
                batch.Pins.Add(SessionScopeLease.AcquireReader(_sessionScope.OwnerId));
                batch.Pins.AddRange(_importedRecoverySources);
            }
            foreach (var record in _unrecoveredEditors.Values)
            {
                cancellation.ThrowIfCancellationRequested();
                try
                {
                    var document = await LegacySessionImporter.PrepareAsync(record, _sessionScope.OwnerId, defaults, cancellation);
                    if (document != null) batch.Documents.Add(document);
                    else RecoveryOutcome = SessionRecoveryOutcome.Partial;
                }
                catch (OperationCanceledException) { throw; }
                catch (Exception ex)
                {
                    RecoveryOutcome = SessionRecoveryOutcome.Partial;
                    LoggingService.LogError($"[{nameof(SessionService)}] Retaining failed V1 row [{record.Id}]: {ex}");
                }
            }
            foreach (var record in _unrecoveredV2Editors.Values)
            {
                cancellation.ThrowIfCancellationRequested();
                PreparedRecoveryDocument document = null;
                try { document = await PrepareCurrentDocumentAsync(record); }
                catch (OperationCanceledException) { throw; }
                catch (Exception ex)
                {
                    RecoveryOutcome = SessionRecoveryOutcome.Partial;
                    LoggingService.LogError($"[{nameof(SessionService)}] Retaining failed current row [{record.Id}]: {ex}");
                    // Persist the failed newest descriptor before a successful
                    // fallback can cause its older history to be retired.
                    var originId = _adoptions.TryGetValue(record.Id, out var origin) ? origin.SourceEditor?.Id ?? record.Id : record.Id;
                    var source = origin?.Source;
                    if (record.Journal.OwnerId == _sessionScope.OwnerId || source?.WriterLease != null)
                    {
                        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
                        {
                            var pendingStamp = record.Journal.OwnerId == _sessionScope.OwnerId ? batch.ExpectedStamp : source.ExpectedStamp;
                            var authority = await SessionRecoveryCatalog.ReadScopeAsync(pendingStamp.ScopeId, cancellation, content: false);
                            SessionRecoveryAuthority.RequireCurrent(pendingStamp, authority);
                            var store = _recordStore;
                            var failedDescriptor = origin?.SourceEditor ?? record;
                            var area = store.GetScopeArea(pendingStamp.ScopeId, RecoveryAreaKind.Pending, failedDescriptor.Id);
                            RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
                            { Kind = RecoveryRecordKind.Pending, Stamp = store.CreatePublicationStamp(area, pendingStamp), Editor = failedDescriptor }, cancellation));
                        }
                        var alternatives = source?.EarlierDescriptors.TryGetValue(originId, out var sourceAlternatives) == true ?
                            sourceAlternatives : _recoverySnapshot.EarlierDescriptors(originId);
                        foreach (var older in alternatives)
                        {
                            cancellation.ThrowIfCancellationRequested();
                            try
                            {
                                var fallback = older;
                                if (originId != record.Id)
                                {
                                    fallback = SessionRecoveryCatalog.CloneEditor(older);
                                    fallback.Id = record.Id;
                                    fallback.EditingFileFutureAccessToken = null; fallback.EditingFilePath = null;
                                    fallback.StateMetaData.HasEditingFile = false; fallback.StateMetaData.RequiresSaveAs = true;
                                    fallback.StateMetaData.IsModified = true; fallback.TextDirty = true;
                                }
                                document = await PrepareCurrentDocumentAsync(fallback); break;
                            }
                            catch (OperationCanceledException) { throw; }
                            catch (Exception fallbackError)
                            { LoggingService.LogError($"[{nameof(SessionService)}] Older row [{record.Id}] is unavailable: {fallbackError.Message}"); }
                        }
                    }
                }
                if (document != null) batch.Documents.Add(document);
            }
            cancellation.ThrowIfCancellationRequested();
            return batch;
        }
        catch { batch.Dispose(); throw; }
    }

    private async Task<PreparedRecoveryDocument> PrepareCurrentDocumentAsync(TextEditorSessionDataV2 data)
    {
        if (data.StateMetaData == null) throw new InvalidDataException("The recovery editor has no saved metadata.");
        var prepared = new PreparedRecoveryDocument { Id = data.Id, Metadata = data.StateMetaData, TextDirty = data.TextDirty };
        try
        {
            using (var savedSource = await SessionDocumentStore.OpenBaselineAsync(data.SavedBaseline))
            using (var recoverySource = data.SavedBaseline.Matches(data.RecoveryBaseline) ? savedSource.Retain() :
                await SessionDocumentStore.OpenBaselineAsync(data.RecoveryBaseline))
            using (var saved = data.SavedBaseline.OwnerId != _sessionScope.OwnerId ?
                await SessionDocumentStore.ForkBaselineAsync(savedSource, _sessionScope.OwnerId) : savedSource.Retain())
            {
                prepared.SavedSnapshot = new DocumentSnapshot(saved,
                    EncodingCatalog.GetEncodingByName(data.StateMetaData.LastSavedEncoding),
                    LineEndingUtility.GetLineEndingByName(data.StateMetaData.LastSavedLineEnding),
                    data.StateMetaData.DateModifiedFileTime);
                prepared.RecoveryBaseline = data.SavedBaseline.Matches(data.RecoveryBaseline) ? saved.Retain() :
                    data.RecoveryBaseline.OwnerId != _sessionScope.OwnerId ?
                        await SessionDocumentStore.ForkBaselineAsync(recoverySource, _sessionScope.OwnerId) : recoverySource.Retain();
            }
            prepared.Journal = await SessionDocumentStore.OpenJournalAsync(data.Journal);
            prepared.Checkpoint = await prepared.Journal.OpenCheckpointAsync(data.Journal.BaselineSequence,
                data.Journal.CommittedSequence, data.Journal.CommittedByteLength,
                data.Journal.PrefixSha256, data.Journal.DocumentByteLength);
            prepared.EditingFile = data.EditingFileFutureAccessToken == null ? null :
                await FutureAccessListUtility.GetFileFromFutureAccessListAsync(data.EditingFileFutureAccessToken);
            prepared.FileNamePlaceholder = data.StateMetaData.FileNamePlaceholder ?? data.EditingFileName;
            return prepared;
        }
        catch { prepared.Dispose(); throw; }
    }
}
