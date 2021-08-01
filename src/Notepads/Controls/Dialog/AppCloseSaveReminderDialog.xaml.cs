namespace Notepads.Controls.Dialog
{
    using System;
    using Windows.UI.Xaml.Controls;

    public sealed partial class AppCloseSaveReminderDialog : ContentDialog, INotepadsDialog
    {
        public bool IsAborted { get; set; }

        private readonly Action _saveAndExitAction;
        private readonly Action _discardAndExitAction;
        private readonly Action _cancelAction;

        public AppCloseSaveReminderDialog(Action saveAndExitAction, Action discardAndExitAction, Action cancelAction)
        {
            InitializeComponent();

            _saveAndExitAction = saveAndExitAction;
            _discardAndExitAction = discardAndExitAction;
            _cancelAction = cancelAction;
        }

        private void ContentDialog_PrimaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            _saveAndExitAction?.Invoke();
        }

        private void ContentDialog_SecondaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            _discardAndExitAction?.Invoke();
        }

        private void ContentDialog_CloseButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            _cancelAction?.Invoke();
        }
    }
}
