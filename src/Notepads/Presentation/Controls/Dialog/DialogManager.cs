// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Controls.Dialog;

public static class DialogManager
{
    public static NotepadsDialog ActiveDialog;

    private static TaskCompletionSource<bool> _dialogAwaiter = new();

    public static async Task<ContentDialogResult?> OpenDialogAsync(NotepadsDialog dialog, bool awaitPreviousDialog)
    {
        try
        {
            return await OpenDialogInternalAsync(dialog, awaitPreviousDialog);
        }
        catch (Exception ex)
        {
            var activeDialogTitle = string.Empty;
            var pendingDialogTitle = string.Empty;
            if (ActiveDialog?.Title is string activeTitle)
            {
                activeDialogTitle = activeTitle;
            }
            if (dialog?.Title is string pendingTitle)
            {
                pendingDialogTitle = pendingTitle;
            }
            AnalyticsService.TrackEvent("FailedToOpenDialog", new Dictionary<string, string>()
            {
                { "Message", ex.Message },
                { "Exception", ex.ToString() },
                { "ActiveDialogTitle", activeDialogTitle },
                { "PendingDialogTitle", pendingDialogTitle }
            });
        }

        return null;
    }

    private static async Task<ContentDialogResult> OpenDialogInternalAsync(NotepadsDialog dialog, bool awaitPreviousDialog)
    {
        TaskCompletionSource<bool> currentAwaiter = _dialogAwaiter;
        TaskCompletionSource<bool> nextAwaiter = new TaskCompletionSource<bool>();
        _dialogAwaiter = nextAwaiter;

        if (ActiveDialog != null)
        {
            if (awaitPreviousDialog)
            {
                await currentAwaiter.Task;
            }
            else
            {
                ActiveDialog.IsAborted = true;
                ActiveDialog.Hide();
            }
        }

        ActiveDialog = dialog;

        try
        {
            return await ActiveDialog.ShowAsync();
        }
        finally
        {
            nextAwaiter.SetResult(true);
        }
    }
}
