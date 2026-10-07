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
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Windows.Storage;

namespace Notepads.Features.Sessions.Transfer;

internal static class TransferRecoveryStore
{
    public static async Task<TransferToken> CreateSourceTransferAsync(Guid transferId, Guid sourceInstanceId,
        Guid nonce, TextEditorSessionDataV2 editor, StorageFile editingFile, RecoveryStamp expectedSourceStamp,
        CancellationToken cancellationToken = default)
    {
        var expected = SessionRecoveryAuthority.CopyStamp(expectedSourceStamp);
        editor = SessionRecoveryCatalog.CloneEditor(editor);
        ValidateFileAssociation(editor, editingFile);
        var draft = new SessionGenerationDraft();
        try
        {
            using (await SessionRecoveryTransaction.EnterAsync(cancellationToken))
            {
                await RequireOpenAsync(expected, editor.Id, cancellationToken);
                if (editor.Journal.OwnerId != expected.ScopeId) throw new InvalidDataException("The transfer capture has a different source owner.");
                editor.EditingFileFutureAccessToken = editingFile == null ? null :
                    SessionDocumentStore.TryRegisterFileAccess(editor.Id, editingFile, expected.ScopeId, draft);
                var token = new TransferToken
                {
                    TransferId = transferId,
                    SourceInstanceId = sourceInstanceId,
                    SourceEditorId = editor.Id,
                    SourceOwnerId = expected.ScopeId,
                    SourceEpochId = expected.EpochId,
                    Nonce = nonce,
                    DescriptorSha256 = editor.ComputeSha256()
                };
                token.Validate();
                await RequireAbsentAsync(transferId, RecoveryRecordKind.TransferOffer, cancellationToken);
                var session = RecoveryRecordValidator.CreateEditorRoot(editor);
                session.Scope = RecoveryRecordValidator.GetProvenScope(expected.ScopeId, sourceInstanceId);
                session.TransferSource = token.Clone();
                draft.PreserveForRecovery();
                RecoveryRootStore.RequireCommitted(await RecoveryRootStore.PublishTransferAsync(transferId,
                    new RecoveryRecord
                    {
                        Kind = RecoveryRecordKind.TransferOffer,
                        Stamp = expected,
                        SourceStamp = SessionRecoveryAuthority.CopyStamp(expected),
                        Editor = editor,
                        Session = session
                    }, cancellationToken));
                draft.Commit();
                return token;
            }
        }
        finally { await RollbackAsync(draft); }
    }

    public static async Task CreateReceiptAsync(TransferToken token, Guid targetRootId, Guid targetInstanceId,
        TextEditorSessionDataV2 target, DocumentRecoveryState capture, StorageFile editingFile,
        RecoveryStamp expectedTargetStamp, CancellationToken cancellationToken = default)
    {
        if (capture == null) throw new ArgumentNullException(nameof(capture));
        token = token?.Clone() ?? throw new ArgumentNullException(nameof(token)); token.Validate();
        var expected = SessionRecoveryAuthority.CopyStamp(expectedTargetStamp);
        target = SessionRecoveryCatalog.CloneEditor(target);
        ValidateFileAssociation(target, editingFile);
        var draft = new SessionGenerationDraft();
        using var owned = capture.Retain();
        await owned.Checkpoint.FlushAsync().AsCompletionTask(cancellationToken);
        try
        {
            using (await SessionRecoveryTransaction.EnterAsync(cancellationToken))
            {
                await RequireOpenAsync(expected, target.Id, cancellationToken);
                var sourceStamp = new RecoveryStamp { ScopeId = token.SourceOwnerId, EpochId = token.SourceEpochId };
                await RequireOpenAsync(sourceStamp, token.SourceEditorId, cancellationToken);
                var captured = SessionDocumentStore.CaptureEditor(target.Id, owned, target.EditingFileName, target.EditingFilePath);
                captured.EditingFileFutureAccessToken = target.EditingFileFutureAccessToken;
                if (captured.ComputeSha256() != target.ComputeSha256())
                    throw new InvalidDataException("The target descriptor does not match its retained native capture.");
                var source = await TransferRootReader.ReadSourceTransferAsync(token, cancellationToken);
                RecoveryRecordValidator.ValidateIndependentTarget(source, target);
                if (target.Journal.OwnerId != expected.ScopeId) throw new InvalidDataException("The target capture has a different recovery owner.");
                await RequireAbsentAsync(token.TransferId, RecoveryRecordKind.TransferReceipt, cancellationToken);
                target.EditingFileFutureAccessToken = editingFile == null ? null :
                    SessionDocumentStore.TryRegisterFileAccess(target.Id, editingFile, expected.ScopeId, draft);
                var session = RecoveryRecordValidator.CreateEditorRoot(target);
                session.Scope = RecoveryRecordValidator.GetProvenScope(expected.ScopeId, targetInstanceId);
                session.TransferReceipt = new TransferReceiptData
                {
                    Source = token.Clone(),
                    TargetRootId = targetRootId,
                    TargetInstanceId = targetInstanceId,
                    TargetOwnerId = expected.ScopeId,
                    TargetEpochId = expected.EpochId,
                    TargetDescriptorSha256 = target.ComputeSha256()
                };
                session.TransferReceipt.Validate();
                draft.PreserveForRecovery(); owned.PreserveForRecovery();
                RecoveryRootStore.RequireCommitted(await RecoveryRootStore.PublishTransferAsync(token.TransferId,
                    new RecoveryRecord
                    {
                        Kind = RecoveryRecordKind.TransferReceipt,
                        Stamp = expected,
                        SourceStamp = sourceStamp,
                        TargetStamp = SessionRecoveryAuthority.CopyStamp(expected),
                        Editor = target,
                        Session = session
                    }, cancellationToken));
                draft.Commit();
            }
        }
        finally { await RollbackAsync(draft); }
    }

    public static async Task<bool> HasReceiptAsync(TransferToken token, CancellationToken cancellation = default)
    {
        if (token == null) throw new ArgumentNullException(nameof(token));
        token = token.Clone(); token.Validate();
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            try
            {
                var receipt = await TransferRootReader.ReadReceiptAsync(token, cancellation);
                await RequireOpenAsync(receipt.Record.TargetStamp, receipt.Record.Editor.Id, cancellation);
                await RequireOpenAsync(receipt.Record.SourceStamp, token.SourceEditorId, cancellation);
                await TransferRootReader.ValidateAssetLengthsAsync(receipt.Record.Editor, cancellation);
                return true;
            }
            catch (FileNotFoundException) { return false; }
            catch (InvalidDataException) { return false; }
        }
    }

    public static async Task MarkSourceAcknowledgedAsync(TransferToken token, long sourceRemovalRevision,
        CancellationToken cancellationToken = default)
    {
        if (token == null) throw new ArgumentNullException(nameof(token));
        token = token.Clone(); token.Validate();
        using (await SessionRecoveryTransaction.EnterAsync(cancellationToken))
        {
            if (await TransferRootReader.IsSourceAcknowledgedAsync(token, cancellationToken)) return;
            var receipt = await TransferRootReader.ReadReceiptAsync(token, cancellationToken);
            await RequireOpenAsync(receipt.Record.SourceStamp, token.SourceEditorId, cancellationToken);
            await RequireOpenAsync(receipt.Record.TargetStamp, receipt.Record.Editor.Id, cancellationToken);
            await TransferRootReader.ValidateAssetLengthsAsync(receipt.Record.Editor, cancellationToken);
            var source = await TransferRootReader.ReadSourceTransferAsync(token, cancellationToken);
            if (sourceRemovalRevision < source.CaptureRevision)
                throw new InvalidDataException("Source removal precedes the frozen capture.");
            var session = new NotepadsSessionDataV2
            {
                Scope = RecoveryRecordValidator.GetProvenScope(token.SourceOwnerId, token.SourceInstanceId),
                TransferAcknowledgement = token.Clone(),
                AcknowledgedSource = source,
                SourceRemovalRevision = sourceRemovalRevision
            };
            RecoveryRootStore.RequireCommitted(await RecoveryRootStore.PublishTransferAsync(token.TransferId,
                new RecoveryRecord
                {
                    Kind = RecoveryRecordKind.SourceMoved,
                    Stamp = SessionRecoveryAuthority.CopyStamp(receipt.Record.SourceStamp),
                    SourceStamp = SessionRecoveryAuthority.CopyStamp(receipt.Record.SourceStamp),
                    TargetStamp = SessionRecoveryAuthority.CopyStamp(receipt.Record.TargetStamp),
                    SourceEditor = source,
                    Editor = receipt.Record.Editor,
                    Session = session,
                    TargetReceiptOperationId = receipt.Address.OperationId,
                    TargetReceiptSha256 = receipt.PayloadSha256
                }, cancellationToken));
        }
    }

    public static async Task DeleteSourceTransferAsync(TransferToken token)
    {
        using (await SessionRecoveryTransaction.EnterAsync())
        {
            try
            {
                var source = await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.TransferOffer);
                RecoveryRecordValidator.ValidateEditorRoot(source.Record.Session);
                if (!token.Matches(source.Record.Session.TransferSource)) throw new InvalidDataException("The source transfer identity differs.");
                // Acknowledgement retains the complete source and target roots.
                // An unacknowledged offer may be the only flushed recovery prefix.
                if (await TransferRootReader.IsSourceAcknowledgedAsync(token))
                    await RecoveryRootStore.CreateStore().DeleteAsync(source.Address);
            }
            catch (FileNotFoundException) { }
        }
    }

    private static async Task RequireOpenAsync(RecoveryStamp expected, Guid editorId, CancellationToken cancellation)
    {
        if (expected == null) throw new InvalidDataException("A transfer has no expected recovery epoch.");
        var current = await SessionRecoveryCatalog.ReadScopeAsync(expected.ScopeId, cancellation, content: false);
        SessionRecoveryAuthority.RequireCurrent(expected, current);
        if (SessionRecoveryAuthority.IsClosed(expected, editorId, current.Decisions.Select(read => read.Record)))
            throw new InvalidDataException("The transfer incarnation has already been closed.");
    }

    private static async Task RequireAbsentAsync(Guid transferId, RecoveryRecordKind kind, CancellationToken cancellation)
    {
        try { await RecoveryRootStore.ReadTransferAsync(transferId, kind, cancellation); }
        catch (FileNotFoundException) { return; }
        throw new InvalidDataException("This transfer lifecycle operation has already been published.");
    }

    private static void ValidateFileAssociation(TextEditorSessionDataV2 editor, StorageFile file)
    {
        if (editor.StateMetaData.HasEditingFile != (file != null) || file != null &&
            (editor.EditingFileName != file.Name || !string.Equals(editor.EditingFilePath, file.Path, StringComparison.OrdinalIgnoreCase)))
        {
            throw new InvalidDataException("The transfer file association does not match its captured metadata.");
        }
    }

    private static async Task RollbackAsync(SessionGenerationDraft draft)
    {
        foreach (var error in await draft.RollbackAsync())
            LoggingService.LogError($"[{nameof(TransferRecoveryStore)}] Failed to retire an unpublished grant: {error}");
    }
}
