namespace Notepads.Controls.Dialog
{
    using System;

    public class RevertAllChangesConfirmationDialog : NotepadsDialog
    {
        public RevertAllChangesConfirmationDialog(string fileNameOrPath, Action confirmedAction)
        {
            Title = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_Title");
            Content = string.Format(ResourceLoader.GetString("RevertAllChangesConfirmationDialog_Content"), fileNameOrPath);
            PrimaryButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_PrimaryButtonText");
            CloseButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_CloseButtonText");

            PrimaryButtonClick += (dialog, args) => { confirmedAction(); };
        }
    }
}