namespace Notepads.Utilities
{
    using System;
    using System.Threading.Tasks;
    using Notepads.Controls.Dialog;
    using Windows.UI.Xaml.Controls;

    public static class DialogManager
    {
        public static NotepadsDialog ActiveDialog;

        private static TaskCompletionSource<bool> _dialogAwaiter = new TaskCompletionSource<bool>();

        public static async Task<ContentDialogResult> OpenDialogAsync(NotepadsDialog dialog, bool awaitPreviousDialog)
        {
            return await OpenDialog(dialog, awaitPreviousDialog);
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