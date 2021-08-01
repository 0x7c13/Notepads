namespace Notepads.Controls.Dialog
{
    using System;
    using Windows.ApplicationModel.Resources;
    using Windows.UI.Xaml.Controls;

    public sealed partial class RevertAllChangesConfirmationDialog : ContentDialog, INotepadsDialog
    {
        public bool IsAborted { get; set; }

        private readonly Action _confirmedAction;

        public RevertAllChangesConfirmationDialog(string fileNameOrPath, Action confirmedAction)
        {
            InitializeComponent();

            Content = string.Format(ResourceLoader.GetForCurrentView().GetString("RevertAllChangesConfirmationDialog_Content"), fileNameOrPath);
            _confirmedAction = confirmedAction;
        }

        private void RevertAllChangesConfirmationDialog_PrimaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            _confirmedAction?.Invoke();
        }
    }
}
