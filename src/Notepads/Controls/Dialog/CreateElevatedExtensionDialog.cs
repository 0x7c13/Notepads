namespace Notepads.Controls.Dialog
{
    using System;
    using Windows.ApplicationModel.Resources;

    public class CreateElevatedExtensionDialog : NotepadsDialog
    {
        public CreateElevatedExtensionDialog(Action confirmedAction, Action closeAction)
        {
            Title = ResourceLoader.GetString("CreateElevatedExtensionDialog_Title");
            Content = ResourceLoader.GetString("CreateElevatedExtensionDialog_Content");
            PrimaryButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_PrimaryButtonText");
            CloseButtonText = ResourceLoader.GetString("CreateElevatedExtensionDialog_CloseButtonText");

            PrimaryButtonClick += (dialog, args) => { confirmedAction(); };
            CloseButtonClick += (dialog, args) => { closeAction(); };
        }
    }
}
