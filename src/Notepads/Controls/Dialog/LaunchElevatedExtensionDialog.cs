namespace Notepads.Controls.Dialog
{
    using System;
    using Notepads.Services;
    using Windows.ApplicationModel.Resources;

    public class LaunchElevatedExtensionDialog : NotepadsDialog
    {
        public LaunchElevatedExtensionDialog(AdminOperationType type, string fileName, Action confirmedAction, Action closeAction)
        {
            if (type == AdminOperationType.Save)
            {
                Title = ResourceLoader.GetString("LaunchElevatedExtensionDialog_SaveFailed_Title");
                Content = string.Format(ResourceLoader.GetString("LaunchElevatedExtensionDialog_SaveFailed_Content"), fileName);
                CloseButtonText = ResourceLoader.GetString("LaunchElevatedExtensionDialog_SaveFailed_CloseButtonText");
            }
            else
            {
                Title = ResourceLoader.GetString("LaunchElevatedExtensionDialog_RenameFailed_Title");
                Content = string.Format(ResourceLoader.GetString("LaunchElevatedExtensionDialog_RenameFailed_Content"), fileName);
                CloseButtonText = ResourceLoader.GetString("LaunchElevatedExtensionDialog_RenameFailed_CloseButtonText");
            }
            
            PrimaryButtonText = ResourceLoader.GetString("LaunchElevatedExtensionDialog_PrimaryButtonText");

            if (confirmedAction != null) PrimaryButtonClick += (dialog, args) => { confirmedAction(); };
            if (closeAction != null) CloseButtonClick += (dialog, args) => { closeAction(); };
        }
    }
}
