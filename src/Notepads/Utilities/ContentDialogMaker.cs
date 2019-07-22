
namespace Notepads.Utilities
{
    using System;
    using System.Threading.Tasks;
    using Windows.UI.Xaml.Controls;

    // Took from https://stackoverflow.com/questions/33018346/only-a-single-contentdialog-can-be-open-at-any-time-error-while-opening-anoth
    // with some modifications
    public static class ContentDialogMaker
    {
        public static async void CreateContentDialog(ContentDialog dialog, bool awaitPreviousDialog) { await CreateDialog(dialog, awaitPreviousDialog); }

        public static async Task CreateContentDialogAsync(ContentDialog dialog, bool awaitPreviousDialog) { await CreateDialog(dialog, awaitPreviousDialog); }

        public static ContentDialog ActiveDialog;

        private static TaskCompletionSource<bool> _dialogAwaiter = new TaskCompletionSource<bool>();

        private static void ActiveDialog_Closed(ContentDialog sender, ContentDialogClosedEventArgs args)
        {
            try
            {
                _dialogAwaiter.SetResult(true);
            }
            catch (Exception)
            {
                // Ignore
            }
        }

        static async Task CreateDialog(ContentDialog dialog, bool awaitPreviousDialog)
        {
            if (ActiveDialog != null)
            {
                if (awaitPreviousDialog)
                {
                    await _dialogAwaiter.Task;
                    _dialogAwaiter = new TaskCompletionSource<bool>();
                }
                else ActiveDialog.Hide();
            }
            ActiveDialog = dialog;
            ActiveDialog.Closed += ActiveDialog_Closed;
            await ActiveDialog.ShowAsync();

            // Only unsubscribe the callback if the active dialog is us
            // Otherwise, the in-flight dialog may end up not signaling the awaiter
            if (ActiveDialog == dialog)
            {
                ActiveDialog.Closed -= ActiveDialog_Closed;
            }
        }
    }
}
