namespace Notepads.Controls.Dialog
{
    using System;

    public class SetCloseSaveReminderDialog : NotepadsDialog
    {
        public SetCloseSaveReminderDialog(string fileNameOrPath, Action saveAction, Action skipSavingAction)
        {
            Title = ResourceLoader.GetString("SetCloseSaveReminderDialog_Title");
            Content = string.Format(ResourceLoader.GetString("SetCloseSaveReminderDialog_Content"), fileNameOrPath);
            PrimaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_PrimaryButtonText");
            SecondaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_SecondaryButtonText");
            CloseButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_CloseButtonText");

            PrimaryButtonClick += (dialog, args) => { saveAction(); };
            SecondaryButtonClick += (dialog, args) => { skipSavingAction(); };
        }
    }
}