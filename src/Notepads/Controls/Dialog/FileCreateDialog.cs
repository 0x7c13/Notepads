namespace Notepads.Controls.Dialog
{
    using System;

    public class FileCreateDialog : NotepadsDialog
    {
        public FileCreateDialog(string filePath, Action createAction)
        {
            Title = ResourceLoader.GetString("GetFileOpenCreateNewFileReminderDialog_Title");
            Content = string.Format(ResourceLoader.GetString("GetFileOpenCreateNewFileReminderDialog_Content"), filePath);
            PrimaryButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_PrimaryButtonText");
            CloseButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_CloseButtonText");

            PrimaryButtonClick += (dialog, args) => { createAction(); };
        }
    }
}
