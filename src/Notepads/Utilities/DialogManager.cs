namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Notepads.Controls.Dialog;
    using Windows.UI.Xaml.Controls;
    using Microsoft.AppCenter.Analytics;
    
    public static class DialogManager
    {
        public static NotepadsDialog ActiveDialog;

        private static TaskCompletionSource<bool> _dialogAwaiter = new TaskCompletionSource<bool>();

        public static async Task<ContentDialogResult?> OpenDialogAsync(NotepadsDialog dialog, bool awaitPreviousDialog)
        {
            try
            {
                return await OpenDialog(dialog, awaitPreviousDialog);
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
                Analytics.TrackEvent("FailedToOpenDialog", new Dictionary<string, string>() 
                {
                    { "Message", ex.Message },
                    { "Exception", ex.ToString() },
                    { "ActiveDialogTitle", activeDialogTitle },
                    { "PendingDialogTitle", pendingDialogTitle }
                });
            }

            return null;
        }

        static async Task<ContentDialogResult> OpenDialog(NotepadsDialog dialog, bool awaitPreviousDialog)
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
            var result = await ActiveDialog.ShowAsync();
            nextAwaiter.SetResult(true);
            return result;
        }
    }
}