// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;

namespace Notepads.Presentation.Workspace;

public sealed partial class NotepadsCore
{
    private async Task<ITextEditor> PrepareTransferredEditorAsync(TextEditorSessionDataV2 data, StorageFile editingFile)
    {
        using var savedSource = await SessionDocumentStore.OpenBaselineAsync(data.SavedBaseline);
        using var recoverySource = await SessionDocumentStore.OpenBaselineAsync(data.RecoveryBaseline);
        using var journal = await SessionDocumentStore.OpenJournalAsync(data.Journal);
        using var checkpoint = await journal.OpenCheckpointAsync(
            data.Journal.BaselineSequence, data.Journal.CommittedSequence,
            data.Journal.CommittedByteLength, data.Journal.PrefixSha256, data.Journal.DocumentByteLength);
        using var saved = await SessionDocumentStore.ForkBaselineAsync(savedSource, DocumentOwnerId);
        using var recovery = data.SavedBaseline.Matches(data.RecoveryBaseline) ?
            saved.Retain() : await SessionDocumentStore.ForkBaselineAsync(recoverySource, DocumentOwnerId);
        using var snapshot = new DocumentSnapshot(saved,
            EncodingCatalog.GetEncodingByName(data.StateMetaData.LastSavedEncoding),
            LineEndingUtility.GetLineEndingByName(data.StateMetaData.LastSavedLineEnding), data.StateMetaData.DateModifiedFileTime);
        var candidate = CreateTextEditor(Guid.NewGuid(), editingFile, data.StateMetaData.FileNamePlaceholder);
        try
        {
            await candidate.RestoreV2Async(snapshot, recovery, journal, checkpoint,
                editingFile, data.StateMetaData, data.TextDirty);
            return candidate;
        }
        catch
        {
            await DiscardUnattachedEditorAsync(candidate);
            throw;
        }
    }

    private async Task ProtectTransferredEditorAsync(ITextEditor editor, TransferToken token, RecoveryStamp expectedStamp)
    {
        var rootId = Guid.NewGuid();
        using (var recovery = editor.CaptureRecoveryState())
        {
            await recovery.Checkpoint.FlushAsync();
            var descriptor = SessionDocumentStore.CaptureEditor(editor.Id, recovery,
                editor.EditingFile?.Name, editor.EditingFilePath);
            await TransferRecoveryStore.CreateReceiptAsync(token, rootId, _context.InstanceId,
                descriptor, recovery, editor.EditingFile, expectedStamp);
        }
    }
}
