namespace Notepads.Controls.Dialog
{
    using System;
    using Windows.ApplicationModel.Resources;
    using Windows.UI.Xaml.Controls;

    public sealed partial class SetCloseSaveReminderDialog : ContentDialog, INotepadsDialog
    {
        public bool IsAborted { get; set; }

        private readonly Action _saveAction;
        private readonly Action _skipSavingAction;

        public SetCloseSaveReminderDialog(string fileNameOrPath, Action saveAction, Action skipSavingAction)
        {
            InitializeComponent();

            Content = string.Format(ResourceLoader.GetForCurrentView().GetString("SetCloseSaveReminderDialog_Content"), fileNameOrPath);
            _saveAction = saveAction;
            _skipSavingAction = skipSavingAction;
        }

        private void SetCloseSaveReminderDialog_PrimaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            _saveAction?.Invoke();
        }

        private void SetCloseSaveReminderDialog_SecondaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            _skipSavingAction?.Invoke();
        }
    }
}
