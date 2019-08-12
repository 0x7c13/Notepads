
namespace Notepads.Utilities
{
    using System;
    using System.Threading.Tasks;
    using Windows.UI.Xaml.Controls;

    public static class ContentDialogMaker
    {
        public static async void CreateContentDialog(ContentDialog dialog, bool awaitPreviousDialog) { await CreateDialog(dialog, awaitPreviousDialog); }

        public static async Task CreateContentDialogAsync(ContentDialog dialog, bool awaitPreviousDialog) { await CreateDialog(dialog, awaitPreviousDialog); }

        public static ContentDialog ActiveDialog;

        private static TaskCompletionSource<bool> _dialogAwaiter = new TaskCompletionSource<bool>();

        private static async Task CreateDialog(ContentDialog dialog, bool awaitPreviousDialog)
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
                    ActiveDialog.Hide();
                }
            }

            ActiveDialog = dialog;
            await ActiveDialog.ShowAsync();
            nextAwaiter.SetResult(true);
        }
    }
}
