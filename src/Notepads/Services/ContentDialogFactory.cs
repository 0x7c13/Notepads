
namespace Notepads.Services
{
    using System;
    using Windows.ApplicationModel.Resources;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.Storage;

    public static class ContentDialogFactory
    {
        private static readonly ResourceLoader ResourceLoader = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView();

        public static ContentDialog GetAppCloseSaveReminderDialog(Action saveAndExitAction, Action discardAndExitAction)
        {
            ContentDialog saveReminderDialog = new ContentDialog
            {
                Title = ResourceLoader.GetString("AppCloseSaveReminderDialog_Title"),
                HorizontalAlignment = HorizontalAlignment.Center,
                Content = ResourceLoader.GetString("AppCloseSaveReminderDialog_Content"),
                PrimaryButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_PrimaryButtonText"),
                SecondaryButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_SecondaryButtonText"),
                CloseButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_CloseButtonText"),
                RequestedTheme = ThemeSettingsService.ThemeMode,
                PrimaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 38, 114, 201)),
                SecondaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 216, 0, 12)),
            };
            saveReminderDialog.PrimaryButtonClick += (dialog, eventArgs) => saveAndExitAction();
            saveReminderDialog.SecondaryButtonClick += (dialog, eventArgs) => discardAndExitAction();
            return saveReminderDialog;
        }

        public static ContentDialog GetSetCloseSaveReminderDialog(string fileName, Action saveAction, Action skipSavingAction)
        {
            ContentDialog setCloseSaveReminder = new ContentDialog
            {
                Title = ResourceLoader.GetString("SetCloseSaveReminderDialog_Title"),
                Content = $"{ResourceLoader.GetString("SetCloseSaveReminderDialog_Content")} \"{(fileName)}\"?",
                PrimaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_PrimaryButtonText"),
                SecondaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_SecondaryButtonText"),
                CloseButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_CloseButtonText"),
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
                Title = ResourceLoader.GetString("FileOpenErrorDialog_Title"),
                Content = $"{ResourceLoader.GetString("FileOpenErrorDialog_Content_Part1")} \"{file.Name}\" {ResourceLoader.GetString("FileOpenErrorDialog_Content_Part2")}: {ex.Message}",
                PrimaryButtonText = ResourceLoader.GetString("FileOpenErrorDialog_PrimaryButtonText"),
                RequestedTheme = ThemeSettingsService.ThemeMode
            };
            return fileOpenErrorDialog;
        }

        public static ContentDialog GetFileSaveErrorDialog()
        {
            return new ContentDialog
            {
                Title = ResourceLoader.GetString("FileSaveErrorDialog_Title"),
                Content = $"{ResourceLoader.GetString("FileSaveErrorDialog_Content_Part1")} {ResourceLoader.GetString("FileSaveErrorDialog_Content_Part2")}",
                PrimaryButtonText = ResourceLoader.GetString("FileSaveErrorDialog_PrimaryButtonText"),
                RequestedTheme = ThemeSettingsService.ThemeMode
            };
        }

        public static ContentDialog GetFileSaveErrorDialog(StorageFile file)
        {
            return new ContentDialog
            {
                Title = ResourceLoader.GetString("FileSaveErrorDialog_Title"),
                Content = $"{ResourceLoader.GetString("FileSaveErrorDialog_Content_Part1")} \"{file.Name}\" {ResourceLoader.GetString("FileSaveErrorDialog_Content_Part2")}",
                PrimaryButtonText = ResourceLoader.GetString("FileSaveErrorDialog_PrimaryButtonText"),
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
