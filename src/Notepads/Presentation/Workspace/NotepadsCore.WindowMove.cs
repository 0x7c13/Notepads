// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace.Activation;
using Windows.Storage;
using Windows.Storage.AccessCache;

namespace Notepads.Presentation.Workspace;

public sealed partial class NotepadsCore
{
    public bool CanMoveTabToNewWindow(WorkspaceTab tab) => !_disposed && !IsClosing && tab != null && ContainsTab(tab) &&
        tab.ReadyEditor is TextEditor { IsMovingToWindow: false } && !_transfers.ContainsKey(tab.Id);

    public Task<bool> MoveTabToNewWindowAsync(WorkspaceTab tab)
    {
        if (!CanMoveTabToNewWindow(tab)) return Task.FromResult(false);
        // A completing reservation is owned by this task, including during Dispose.
        // Native DnD and subsequent menu invocations see the same admission table.
        var active = new ActiveTabTransfer(Guid.NewGuid(), null) { Completing = true };
        _transfers.Add(tab.Id, active);
        return TrackWorkAsync(MoveTabCoreAsync(tab, active));
    }

    private async Task<bool> MoveTabCoreAsync(WorkspaceTab tab, ActiveTabTransfer active)
    {
        var editor = (TextEditor)tab.ReadyEditor;
        TextEditorTransfer transfer = null;
        TransferToken token = null;
        bool moved = false, fenced = false;
        var result = TextEditorTransferResult.Cancelled;
        try
        {
            using var lifetime = CaptureFileOperation();
            lifetime.CancelAfter(TimeSpan.FromSeconds(90));
            await editor.WithWindowMoveAsync(async (capture, cancellation) =>
            {
                var request = capture.Request;
                try
                {
                    transfer = new TextEditorTransfer(editor, _context);
                    token = request.Source = await transfer.PrepareAsync(cancellation);
                    await capture.ExportAsync(cancellation);
                    await WindowMoveStore.PublishAsync(request, cancellation);
                    cancellation.ThrowIfCancellationRequested();
                    var launch = WindowMoveActivation.LaunchAsync(token, _context.ProtocolScheme);
                    try
                    {
                        if (!await launch.WaitAsync(cancellation)) throw new IOException("Windows did not launch the receiving window.");
                    }
                    finally { if (!launch.IsCompleted) _ = ObserveLateWindowLaunchAsync(launch); }
                    while (!await transfer.HasReceiptAsync(cancellation))
                    {
                        cancellation.ThrowIfCancellationRequested();
                        if (!await WindowMoveStore.IsPendingAsync(token, cancellation))
                            throw new IOException("The receiving window retired its move request.");
                        await Task.Delay(50, cancellation);
                    }
                }
                finally
                {
                    if (token != null)
                    {
                        // Use a fresh cleanup budget, not the cancelled request token.
                        // Fence under the same admission as receipt publication before
                        // releasing the source. Committed receipts win a soft close.
                        using var cleanup = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                        var evidence = await WindowMoveStore.FenceAsync(token, cleanup.Token);
                        fenced = true;
                        if (evidence != WindowMoveEvidence.Absent) result = TextEditorTransferResult.UnverifiedMove;
                        if (evidence == WindowMoveEvidence.Received && !_disposed && GetTab(editor) == tab)
                        {
                            moved = await editor.CommitReservedMoveAsync(
                                () => !_disposed && GetTab(editor) == tab && transfer.IsSourceUnchanged(includeView: true),
                                () => transfer.MarkSourceAcknowledgedAsync(cleanup.Token),
                                () => TextEditorMovedToAnotherAppInstance?.Invoke(this, editor));
                            if (moved) result = TextEditorTransferResult.Received;
                        }
                    }
                }
            }, lifetime.Token);
        }
        catch (Exception error)
        {
            LoggingService.LogException(error);
        }
        finally
        {
            try
            {
                using var cleanup = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                if (transfer != null) await transfer.CompleteAsync(fenced ? result : TextEditorTransferResult.UnverifiedMove,
                    () => !_disposed && GetTab(editor) == tab, cleanup.Token);
                if (fenced) await WindowMoveStore.TryCleanupAsync(token, cleanup.Token);
            }
            catch (Exception error) { LoggingService.LogException(error); }
            finally
            {
                if (_transfers.TryGetValue(tab.Id, out var current) && ReferenceEquals(current, active)) _transfers.Remove(tab.Id);
            }
        }
        if (!moved && !_disposed && !IsClosing)
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("Tab_NotificationMsg_MoveToNewWindowFailed"), 7000);
        return moved;
    }

    private static async Task ObserveLateWindowLaunchAsync(Task<bool> launch)
    {
        // Windows may finish launch after cancellation. Its request has already
        // been fenced; this observer owns no editor, native lease or capture.
        try { await launch; }
        catch (Exception error) { LoggingService.LogException(error); }
    }

    internal async Task ReceiveWindowMoveAsync(TransferToken token, CancellationToken cancellation)
    {
        if (_context.WindowMove == null || !_context.WindowMove.Matches(token) || GetTabCount() != 0)
            throw new InvalidOperationException("This window is not the dedicated receiver.");
        IDisposable sourcePin = null;
        try
        {
            WindowMoveRequest request;
            TextEditorSessionDataV2 data;
            using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            {
                sourcePin = SessionScopeLease.AcquireReader(token.SourceOwnerId);
                request = await WindowMoveStore.ReadPendingAsync(token, cancellation);
                data = await TransferRootReader.ReadSourceTransferAsync(token, cancellation);
            }
            StorageFile file = null;
            if (data.StateMetaData.HasEditingFile)
            {
                if (string.IsNullOrEmpty(data.EditingFileFutureAccessToken)) throw new IOException("The source file permission is unavailable.");
                file = await StorageApplicationPermissions.FutureAccessList.GetFileAsync(data.EditingFileFutureAccessToken);
                if (!string.Equals(file.Path, data.EditingFilePath, StringComparison.OrdinalIgnoreCase) || file.Name != data.EditingFileName)
                    throw new InvalidDataException("The source file permission no longer identifies the transferred file.");
            }
            var expected = _context.Sessions.CurrentStamp;
            var result = await OpenTransferredFileAsync(file, async () =>
            {
                var candidate = (TextEditor)await PrepareTransferredEditorAsync(data, file);
                try
                {
                    await using var input = WindowMoveStore.OpenState(token);
                    await candidate.RestoreWindowMoveStateAsync(request, input, cancellation);
                    if (data.StateMetaData.IsContentPreviewPanelOpened) candidate.ShowHideContentPreview();
                    if (data.StateMetaData.IsInDiffPreviewMode)
                    {
                        await candidate.OpenSideBySideDiffViewerAsync();
                        if (candidate.Mode != TextEditorMode.DiffPreview)
                            throw new IOException("The receiving window could not restore the diff preview.");
                    }
                    return candidate;
                }
                catch { await DiscardUnattachedEditorAsync(candidate); throw; }
            }, editor => ProtectTransferredEditorAsync(editor, token, expected, request), -1, cancellation);
            if (result.Kind != WorkspaceFileOpenKind.Created)
                throw result.Error ?? new OperationCanceledException("The receiving window was closed.");
        }
        finally { sourcePin?.Dispose(); }
    }
}
