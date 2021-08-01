namespace Notepads.Controls.Dialog
{
    using Windows.Foundation;
    using Windows.UI.Xaml.Controls;

    public interface INotepadsDialog
    {
        bool IsAborted { get; set; }
        object Title { get; set; }

        void Hide();
        IAsyncOperation<ContentDialogResult> ShowAsync();
    }
}
