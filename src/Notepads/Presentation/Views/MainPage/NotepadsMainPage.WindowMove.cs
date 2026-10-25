// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Transfer;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Workspace;
using Windows.UI.ViewManagement;
using WorkspaceCore = Notepads.Presentation.Workspace.NotepadsCore;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private Task _windowMoveTask;
    private bool _windowMoveReceiverRetired;

    private static async Task CleanupInactiveWindowMovesAsync()
    {
        try { await WindowMoveStore.CleanupInactiveAsync(); }
        catch (Exception error) { LoggingService.LogException(error); }
    }

    internal Task ActivateWindowMoveAsync(TransferToken token)
    {
        if (_context.WindowMove == null || !_context.WindowMove.Matches(token))
            throw new InvalidOperationException("The incoming tab belongs to another receiving window.");
        if (_windowMoveTask != null) return _windowMoveTask;
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _windowMoveTask = completion.Task;
        _ = CompleteWindowMoveAsync(token.Clone(), completion);
        return _windowMoveTask;
    }

    private async Task CompleteWindowMoveAsync(TransferToken token, TaskCompletionSource completion)
    {
        try
        {
            await QueueActivationAsync(async () =>
            {
                try { await ((WorkspaceCore)NotepadsCore).ReceiveWindowMoveAsync(token, _activationLifetime.Token); }
                catch (Exception error)
                {
                    LoggingService.LogException(error);
                    var retire = !_viewClosed && NotepadsCore.GetTabCount() == 0;
                    if (retire)
                    {
                        // Fence queued file/command activations before the first
                        // await. A retiring receiver cannot acquire new documents.
                        _isAppClosing = true;
                        NotepadsCore.IsClosing = true;
                        _windowMoveReceiverRetired = true;
                    }
                    // Wake the source promptly. Receipt publication and retirement
                    // share admission, so a receipt that already won remains valid.
                    using var cleanup = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                    try { await WindowMoveStore.FenceAsync(token, cleanup.Token); }
                    catch (Exception retirementError) { LoggingService.LogException(retirementError); }
                    if (_viewClosed) return;
                    // Only a dedicated, still-empty receiver may retire itself.
                    // Startup drains and exits after this queued activation settles.
                    if (retire && await ApplicationView.GetForCurrentView().TryConsolidateAsync())
                    {
                        return;
                    }
                    if (retire && !_viewClosed)
                    {
                        _windowMoveReceiverRetired = false;
                        _isAppClosing = false;
                        NotepadsCore.IsClosing = false;
                    }
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("Tab_NotificationMsg_MoveToNewWindowFailed"), 7000);
                }
            });
        }
        finally { completion.TrySetResult(); }
    }
}
