// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;

namespace Notepads.Features.Sessions;

/// <summary>Payload work over frozen input. No workspace or mutable SessionService state is consulted.</summary>
internal static class RecoveryDocumentPreparation
{
    public static async Task<PreparedRecoveryDocument> PrepareAsync(RecoveryDocumentSource source,
        Guid targetOwnerId, DocumentLoadOptions defaults, CancellationToken cancellation, int fallbackIndex = -1)
    {
        source.RequireAvailable();
        cancellation.ThrowIfCancellationRequested();
        PreparedRecoveryDocument prepared;
        var legacy = source.LegacyDescriptor;
        if (legacy != null)
        {
            if (fallbackIndex >= 0) throw new ArgumentOutOfRangeException(nameof(fallbackIndex));
            var editingFile = await source.ResolveEditingFileAsync().WaitAsync(cancellation);
            prepared = await LegacySessionImporter.PrepareAsync(legacy, editingFile, targetOwnerId, defaults, cancellation);
            if (prepared?.EditingFile != null)
                prepared.FileAccessBinding = new RecoveryFileAccessBinding(prepared.EditingFile, legacy.EditingFileFutureAccessToken);
        }
        else
        {
            var data = source.GetDescriptor(fallbackIndex);
            if (data.StateMetaData == null) throw new InvalidDataException("The recovery editor has no saved metadata.");
            var editingFile = await source.ResolveEditingFileAsync(fallbackIndex).WaitAsync(cancellation);
            prepared = await PrepareCurrentDocumentAsync(data, editingFile, targetOwnerId, defaults, cancellation);
        }
        if (prepared != null) prepared.Source = source;
        return prepared;
    }
    private static async Task<PreparedRecoveryDocument> PrepareCurrentDocumentAsync(TextEditorSessionDataV2 data,
        StorageFile editingFile, Guid targetOwnerId, DocumentLoadOptions defaults, CancellationToken cancellation)
    {
        if (editingFile != null && !data.StateMetaData.IsModified && !data.TextDirty)
        {
            try
            {
                var fromFile = await PreparedRecoveryDocument.FromFileAsync(data.Id, data.StateMetaData, editingFile, targetOwnerId, defaults, cancellation);
                fromFile.FileAccessBinding = new RecoveryFileAccessBinding(editingFile, data.EditingFileFutureAccessToken);
                return fromFile;
            }
            catch (OperationCanceledException) { throw; }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(RecoveryDocumentPreparation)}] Restoring clean row [{data.Id}] from its recovery copy: {ex.Message}");
            }
        }
        var prepared = new PreparedRecoveryDocument { Id = data.Id, Metadata = data.StateMetaData, TextDirty = data.TextDirty };
        try
        {
            using (var savedSource = await SessionDocumentStore.OpenBaselineAsync(data.SavedBaseline, cancellation))
            using (var recoverySource = data.SavedBaseline.Matches(data.RecoveryBaseline) ? savedSource.Retain() :
                await SessionDocumentStore.OpenBaselineAsync(data.RecoveryBaseline, cancellation))
            using (var saved = data.SavedBaseline.OwnerId != targetOwnerId ?
                await SessionDocumentStore.ForkBaselineAsync(savedSource, targetOwnerId, cancellation) : savedSource.Retain())
            {
                prepared.SavedSnapshot = new DocumentSnapshot(saved,
                    EncodingCatalog.GetEncodingByName(data.StateMetaData.LastSavedEncoding),
                    LineEndingUtility.GetLineEndingByName(data.StateMetaData.LastSavedLineEnding),
                    data.StateMetaData.DateModifiedFileTime);
                prepared.RecoveryBaseline = data.SavedBaseline.Matches(data.RecoveryBaseline) ? saved.Retain() :
                    data.RecoveryBaseline.OwnerId != targetOwnerId ?
                        await SessionDocumentStore.ForkBaselineAsync(recoverySource, targetOwnerId, cancellation) : recoverySource.Retain();
            }
            prepared.Journal = await SessionDocumentStore.OpenJournalAsync(data.Journal, cancellation);
            prepared.Checkpoint = await prepared.Journal.OpenCheckpointAsync(data.Journal.BaselineSequence,
                data.Journal.CommittedSequence, data.Journal.CommittedByteLength,
                data.Journal.PrefixSha256, data.Journal.DocumentByteLength, cancellation);
            prepared.EditingFile = editingFile;
            if (editingFile != null) prepared.FileAccessBinding = new RecoveryFileAccessBinding(editingFile, data.EditingFileFutureAccessToken);
            prepared.FileNamePlaceholder = data.StateMetaData.FileNamePlaceholder ?? data.EditingFileName;
            return prepared;
        }
        catch { prepared.Dispose(); throw; }
    }
}
