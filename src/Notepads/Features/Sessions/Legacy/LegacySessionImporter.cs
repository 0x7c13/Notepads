// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Infrastructure.Storage;
using Windows.Storage;

namespace Notepads.Features.Sessions.Legacy;

internal static class LegacySessionImporter
{
    public static async Task<PreparedRecoveryDocument> PrepareAsync(TextEditorSessionDataV1 data,
        Guid ownerId, DocumentLoadOptions defaults, CancellationToken cancellation)
    {
        var file = data.EditingFileFutureAccessToken == null ? null :
            await FutureAccessListUtility.GetFileFromFutureAccessListAsync(data.EditingFileFutureAccessToken);
        var lastSaved = data.LastSavedBackupFilePath;
        var pending = data.PendingBackupFilePath;
        if (lastSaved == null && pending == null)
        {
            return file == null ? null :
                await PreparedRecoveryDocument.FromFileAsync(data.Id, data.StateMetaData, file, ownerId, defaults, cancellation);
        }

        var prepared = new PreparedRecoveryDocument
        {
            Id = data.Id,
            Metadata = data.StateMetaData,
            EditingFile = file,
            FileNamePlaceholder = data.StateMetaData.FileNamePlaceholder
        };
        try
        {
            var savedEncoding = EncodingCatalog.GetEncodingByName(data.StateMetaData.LastSavedEncoding);
            var backupEncoding = EncodingCatalog.GetEncodingByName(data.BackupEncoding ?? data.StateMetaData.LastSavedEncoding);
            using (var saved = lastSaved == null ? DocumentBaseline.CreateEmpty(ownerId) :
                await DecodeBaselineAsync(lastSaved, backupEncoding, defaults, ownerId, cancellation))
            {
                prepared.SavedSnapshot = new DocumentSnapshot(saved, savedEncoding,
                    LineEndingUtility.GetLineEndingByName(data.StateMetaData.LastSavedLineEnding),
                    data.StateMetaData.DateModifiedFileTime);
            }
            if (pending != null)
                prepared.RecoveryBaseline = await DecodeBaselineAsync(pending, backupEncoding, defaults, ownerId, cancellation);
            return prepared;
        }
        catch
        {
            prepared.Dispose();
            throw;
        }
    }

    private static async Task<DocumentBaseline> DecodeBaselineAsync(string path, Encoding encoding,
        DocumentLoadOptions defaults, Guid ownerId, CancellationToken cancellation)
    {
        var file = await StorageFile.GetFileFromPathAsync(path);
        var options = new DocumentLoadOptions(encoding, defaults.GetInitialEncoding());
        using (var decoded = await DocumentTextPipeline.DecodeFileAsync(file, options, ownerId,
            cancellationToken: cancellation))
        {
            return decoded.Baseline.Retain();
        }
    }
}
