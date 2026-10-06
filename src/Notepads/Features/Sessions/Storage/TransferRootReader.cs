// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Validation;
using Windows.Storage;

namespace Notepads.Features.Sessions.Storage;

/// <summary>Read and validate raw transfer content. Lifecycle admission belongs to the catalog.</summary>
internal static class TransferRootReader
{
    public static async Task<TextEditorSessionDataV2> ReadSourceTransferAsync(TransferToken token,
        CancellationToken cancellationToken = default)
    {
        if (token == null) throw new ArgumentNullException(nameof(token));
        token = token.Clone(); token.Validate();
        var root = await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.TransferOffer, cancellationToken);
        var session = root.Record.Session;
        var editor = RecoveryRecordValidator.ValidateEditorRoot(session);
        if (!token.Matches(session.TransferSource) || editor.Id != token.SourceEditorId ||
            editor.Journal.OwnerId != token.SourceOwnerId || editor.ComputeSha256() != token.DescriptorSha256 ||
            root.Record.SourceStamp.ScopeId != token.SourceOwnerId || root.Record.SourceStamp.EpochId != token.SourceEpochId)
        {
            throw new InvalidDataException("The transfer token does not match its durable source descriptor.");
        }

        return editor;
    }

    internal static async Task<RecoveryReadResult> ReadReceiptAsync(TransferToken token,
        CancellationToken cancellation = default)
    {
        var read = await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.TransferReceipt, cancellation);
        RecoveryRecordValidator.ValidateReceipt(read.Record.Session, token);
        return read;
    }

    internal static async Task<bool> IsSourceAcknowledgedAsync(TransferToken token,
        CancellationToken cancellationToken = default)
    {
        try
        {
            var root = await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.SourceMoved, cancellationToken);
            RecoveryRecordValidator.ValidateAcknowledgement(root.Record.Session, token);
            return true;
        }
        catch (FileNotFoundException) { return false; }
    }

    internal static async Task ValidateAssetLengthsAsync(TextEditorSessionDataV2 editor, CancellationToken cancellation)
    {
        foreach (var baseline in new[] { editor.SavedBaseline, editor.RecoveryBaseline })
        {
            cancellation.ThrowIfCancellationRequested();
            if (baseline.FileName == null) continue;
            var folder = await ApplicationData.Current.LocalFolder.GetFolderAsync("DocumentBaselines");
            var file = await folder.GetFileAsync(baseline.FileName);
            if (checked((long)(await file.GetBasicPropertiesAsync()).Size) != baseline.ByteLength)
                throw new InvalidDataException("A transfer baseline has an invalid length.");
        }
        var journals = await ApplicationData.Current.LocalFolder.GetFolderAsync("DocumentJournals");
        var journal = await journals.GetFileAsync(editor.Journal.FileName);
        if ((await journal.GetBasicPropertiesAsync()).Size < checked((ulong)editor.Journal.CommittedByteLength))
            throw new InvalidDataException("A transfer journal is shorter than its committed prefix.");
    }
}
