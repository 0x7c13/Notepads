
namespace Notepads.Services
{
    using System;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.Storage;

    public static class ContentDialogFactory
    {
        public static ContentDialog GetAppCloseSaveReminderDialog(Action saveAndExitAction, Action discardAndExitAction)
        {
            ContentDialog saveReminderDialog = new ContentDialog
            {
                Title = "Do you want to save the changes?",
                HorizontalAlignment = HorizontalAlignment.Center,
                Content = "There are unsaved changes.",
                PrimaryButtonText = "Save All & Exit",
                SecondaryButtonText = "Discard & Exit",
                CloseButtonText = "Cancel",
                RequestedTheme = ThemeSettingsService.ThemeMode,
            };
            saveReminderDialog.PrimaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 38, 114, 201));
            saveReminderDialog.SecondaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 216, 0, 12));
            saveReminderDialog.PrimaryButtonClick += (dialog, eventArgs) => saveAndExitAction();
            saveReminderDialog.SecondaryButtonClick += (dialog, eventArgs) => discardAndExitAction();
            return saveReminderDialog;
        }

        public static ContentDialog GetSetCloseSaveReminderDialog(string fileName, Action saveAction, Action skipSavingAction)
        {
            ContentDialog setCloseSaveReminder = new ContentDialog
            {
                Title = "Save",
                Content = $"Save file \"{(fileName)}\"?",
                PrimaryButtonText = "Save",
                SecondaryButtonText = "Don't Save",
                CloseButtonText = "Cancel",
                RequestedTheme = ThemeSettingsService.ThemeMode,
            };
            setCloseSaveReminder.PrimaryButtonClick += (dialog, args) => { saveAction(); };
            setCloseSaveReminder.SecondaryButtonClick += (dialog, args) => { skipSavingAction(); };
            return setCloseSaveReminder;
        }

        public static ContentDialog GetFileOpenErrorDialog(StorageFile file, Exception ex)
        {
            ContentDialog fileOpenErrorDialog = new ContentDialog
            {
                Title = "File Open Error",
                Content = $"Sorry, file \"{file.Name}\" couldn't be opened:\n{ex.Message}",
                PrimaryButtonText = "Ok",
                RequestedTheme = ThemeSettingsService.ThemeMode
            };
            return fileOpenErrorDialog;
        }

        public static ContentDialog GetFileSaveErrorDialog(StorageFile file)
        {
            return new ContentDialog
            {
                Title = "File Save Error",
                Content = $"File {file.Name} couldn't be saved.",
                PrimaryButtonText = "Ok",
                RequestedTheme = ThemeSettingsService.ThemeMode
            };
        }

        private static Style GetButtonStyle(Color backgroundColor)
        {
            var redButtonStyle = new Windows.UI.Xaml.Style(typeof(Button));
            redButtonStyle.Setters.Add(new Setter(Control.BackgroundProperty, backgroundColor));
            redButtonStyle.Setters.Add(new Setter(Control.ForegroundProperty, Colors.White));

            return redButtonStyle;
        }
    }
}
