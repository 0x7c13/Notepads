// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions.Contracts;
using Windows.Storage;
using Windows.Storage.AccessCache;

namespace Notepads.Features.Sessions.Storage;

/// <summary>Resolve validated owned recovery identities; never reopen the mutable user file as a baseline.</summary>
internal static class SessionDocumentStore
{
    public static string GetFutureAccessTokenPrefix(Guid ownerId)
    {
        if (ownerId == Guid.Empty) throw new ArgumentException("A file permission requires an owned scope.", nameof(ownerId));
        return "Notepads:DocumentOwner:" + ownerId.ToString("N") + ":";
    }

    public static string RegisterFileAccess(Guid editorId, StorageFile file, Guid ownerId, SessionGenerationDraft draft)
    {
        if (editorId == Guid.Empty) throw new ArgumentException("A file permission requires an editor identity.", nameof(editorId));
        if (file == null) throw new ArgumentNullException(nameof(file));
        if (draft == null) throw new ArgumentNullException(nameof(draft));
        var token = GetFutureAccessTokenPrefix(ownerId) + editorId.ToString("N") + "-" + Guid.NewGuid().ToString("N");
        draft.RegisterAccessToken(token);
        StorageApplicationPermissions.FutureAccessList.AddOrReplace(token, file);
        return token;
    }

    /// <summary>Capture descriptors only after the retained native prefix has flushed.</summary>
    public static TextEditorSessionDataV2 CaptureEditor(Guid id, DocumentRecoveryState recovery,
        string editingFileName, string editingFilePath)
    {
        if (recovery == null) throw new ArgumentNullException(nameof(recovery));
        var checkpoint = recovery.Checkpoint;
        var journal = recovery.DocumentJournal;
        if (!string.Equals(journal.File.Path, checkpoint.FilePath, StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("The captured native journal does not match its owned file.");
        var data = new TextEditorSessionDataV2
        {
            Id = id,
            CaptureRevision = recovery.CaptureRevision,
            SavedBaseline = CaptureBaseline(recovery.SavedSnapshot.Baseline),
            RecoveryBaseline = CaptureBaseline(recovery.RecoveryBaseline),
            Journal = new DocumentJournalData
            {
                OwnerId = journal.OwnerId,
                GenerationId = journal.GenerationId,
                FileName = journal.File.Name,
                BaselineSequence = checkpoint.BaseSequence,
                CommittedSequence = checkpoint.CommittedSequence,
                CommittedByteLength = checked((long)checkpoint.CommittedByteLength),
                DocumentByteLength = recovery.DocumentByteLength,
                PrefixSha256 = checkpoint.PrefixSha256
            },
            TextDirty = recovery.TextDirty,
            StateMetaData = recovery.MetaData,
            EditingFileName = editingFileName,
            EditingFilePath = editingFilePath
        };
        data.SavedBaseline.Validate();
        data.RecoveryBaseline.Validate();
        data.Journal.Validate();
        return data;
    }

    public static async Task<DocumentBaseline> ForkBaselineAsync(DocumentBaseline source, Guid ownerId,
        CancellationToken cancellationToken = default)
    {
        if (source == null) throw new ArgumentNullException(nameof(source));
        cancellationToken.ThrowIfCancellationRequested();
        if (source.ByteLength == 0) return DocumentBaseline.CreateEmpty(ownerId);
        using (var stream = await source.OpenReadStreamAsync(cancellationToken).ConfigureAwait(false))
        {
            return await DocumentBaseline.CreateAsync(output => stream.CopyToAsync(output, 65536, cancellationToken),
                cancellationToken, ownerId).ConfigureAwait(false);
        }
    }

    public static DocumentBaselineData CaptureBaseline(DocumentBaseline baseline)
    {
        if (baseline == null) throw new ArgumentNullException(nameof(baseline));
        return new DocumentBaselineData
        {
            OwnerId = baseline.OwnerId,
            GenerationId = baseline.GenerationId,
            FileName = baseline.File?.Name,
            ByteLength = baseline.ByteLength,
            Sha256 = baseline.Sha256
        };
    }

    public static async Task<DocumentBaseline> OpenBaselineAsync(DocumentBaselineData data,
        CancellationToken cancellationToken = default)
    {
        if (data == null) throw new ArgumentNullException(nameof(data));
        data.Validate();
        cancellationToken.ThrowIfCancellationRequested();
        StorageFile file = null;
        if (data.FileName != null)
        {
            var folder = await ApplicationData.Current.LocalFolder.GetFolderAsync("DocumentBaselines");
            file = await folder.GetFileAsync(data.FileName);
        }
        return await DocumentBaseline.OpenExistingAsync(file, data.OwnerId, data.GenerationId,
            data.ByteLength, data.Sha256, cancellationToken).ConfigureAwait(false);
    }

    public static async Task<DocumentJournal> OpenJournalAsync(DocumentJournalData data,
        CancellationToken cancellationToken = default)
    {
        if (data == null) throw new ArgumentNullException(nameof(data));
        data.Validate();
        cancellationToken.ThrowIfCancellationRequested();
        var folder = await ApplicationData.Current.LocalFolder.GetFolderAsync("DocumentJournals");
        var file = await folder.GetFileAsync(data.FileName);
        cancellationToken.ThrowIfCancellationRequested();
        // Take the managed lease before any native validation or prefix read.
        // The native checkpoint factory verifies framing and PrefixSha256.
        return DocumentJournal.OpenExisting(file, data.OwnerId, data.GenerationId);
    }
}
