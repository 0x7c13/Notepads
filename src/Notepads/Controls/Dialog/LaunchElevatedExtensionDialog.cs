namespace Notepads.Controls.Dialog
{
    using System;
    using Notepads.Services;
    using Windows.ApplicationModel.Resources;

    public class LaunchElevatedProcessDialog : NotepadsDialog
    {
        public LaunchElevatedProcessDialog(ElevatedOperationType type, string fileName, Action confirmedAction, Action closeAction)
        {
            if (type == ElevatedOperationType.Save)
            {
                Title = ResourceLoader.GetString("LaunchElevatedProcessDialog_SaveFailed_Title");
                Content = string.Format(ResourceLoader.GetString("LaunchElevatedProcessDialog_SaveFailed_Content"), fileName);
                CloseButtonText = ResourceLoader.GetString("LaunchElevatedProcessDialog_SaveFailed_CloseButtonText");
            }
            else
            {
                Title = ResourceLoader.GetString("LaunchElevatedProcessDialog_RenameFailed_Title");
                Content = string.Format(ResourceLoader.GetString("LaunchElevatedProcessDialog_RenameFailed_Content"), fileName);
                CloseButtonText = ResourceLoader.GetString("LaunchElevatedProcessDialog_RenameFailed_CloseButtonText");
            }
            
            PrimaryButtonText = ResourceLoader.GetString("LaunchElevatedProcessDialog_PrimaryButtonText");

            if (confirmedAction != null) PrimaryButtonClick += (dialog, args) => { confirmedAction(); };
            if (closeAction != null) CloseButtonClick += (dialog, args) => { closeAction(); };
        }
    }
}
