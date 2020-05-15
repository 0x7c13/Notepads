namespace Notepads.Controls.Dialog
{
    using System;
    using Windows.UI;
    using Windows.UI.Xaml;

    public class AppCloseSaveReminderDialog : NotepadsDialog
    {
        public AppCloseSaveReminderDialog(Action saveAndExitAction, Action discardAndExitAction, Action cancelAction)
        {
            Title = ResourceLoader.GetString("AppCloseSaveReminderDialog_Title");
            HorizontalAlignment = HorizontalAlignment.Center;
            Content = ResourceLoader.GetString("AppCloseSaveReminderDialog_Content");
            PrimaryButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_PrimaryButtonText");
            SecondaryButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_SecondaryButtonText");
            CloseButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_CloseButtonText");
            PrimaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 38, 114, 201));

            PrimaryButtonClick += (dialog, eventArgs) => saveAndExitAction();
            SecondaryButtonClick += (dialog, eventArgs) => discardAndExitAction();
            CloseButtonClick += (dialog, eventArgs) => cancelAction();
        }
    }
}