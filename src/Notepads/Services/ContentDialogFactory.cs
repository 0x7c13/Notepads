
namespace Notepads.Services
{
    using System;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.Storage;

    public static class ContentDialogFactory
    {
        public static ContentDialog GetAppCloseSaveReminderDialog(Action skipSavingAction)
        {
            ContentDialog saveReminderDialog = new ContentDialog
            {
                Title = "Exit and discard changes?",
                HorizontalAlignment = HorizontalAlignment.Center,
                Content = "There are unsaved changes.",
                PrimaryButtonText = "Discard & Exit",
                SecondaryButtonText = "Cancel",
                RequestedTheme = ThemeSettingsService.ThemeMode,
            };
            saveReminderDialog.PrimaryButtonStyle = GetRedButtonStyle();
            saveReminderDialog.PrimaryButtonClick += (dialog, eventArgs) => skipSavingAction();
            return saveReminderDialog;
        }

        public static ContentDialog GetSetCloseSaveReminderDialog(string fileName, Action saveAction, Action skipSavingAction)
        {
            ContentDialog setCloseSaveReminder = new ContentDialog
            {
                Title = "Save",
                Content = $"Save file \"{(fileName)}\"?",
                PrimaryButtonText = "Yes",
                SecondaryButtonText = "No",
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

        private static Style GetRedButtonStyle()
        {
            var redButtonStyle = new Windows.UI.Xaml.Style(typeof(Button));
            redButtonStyle.Setters.Add(new Setter(Control.BackgroundProperty, Color.FromArgb(255, 140, 37, 21)));
            redButtonStyle.Setters.Add(new Setter(Control.ForegroundProperty, Colors.White));

            return redButtonStyle;
        }
    }
}
