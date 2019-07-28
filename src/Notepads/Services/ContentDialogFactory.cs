
namespace Notepads.Services
{
    using System;
    using Windows.ApplicationModel.Resources;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public static class ContentDialogFactory
    {
        private static readonly ResourceLoader ResourceLoader = ResourceLoader.GetForCurrentView();

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
            };
            saveReminderDialog.PrimaryButtonClick += (dialog, eventArgs) => saveAndExitAction();
            saveReminderDialog.SecondaryButtonClick += (dialog, eventArgs) => discardAndExitAction();
            return saveReminderDialog;
        }

        public static ContentDialog GetSetCloseSaveReminderDialog(string fileNameOrPath, Action saveAction, Action skipSavingAction)
        {
            ContentDialog setCloseSaveReminder = new ContentDialog
            {
                Title = ResourceLoader.GetString("SetCloseSaveReminderDialog_Title"),
                Content = string.Format(ResourceLoader.GetString("SetCloseSaveReminderDialog_Content"), fileNameOrPath),
                PrimaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_PrimaryButtonText"),
                SecondaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_SecondaryButtonText"),
                CloseButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_CloseButtonText"),
                RequestedTheme = ThemeSettingsService.ThemeMode,
            };
            setCloseSaveReminder.PrimaryButtonClick += (dialog, args) => { saveAction(); };
            setCloseSaveReminder.SecondaryButtonClick += (dialog, args) => { skipSavingAction(); };
            return setCloseSaveReminder;
        }

        public static ContentDialog GetFileOpenErrorDialog(string filePath, string errorMsg)
        {
            ContentDialog fileOpenErrorDialog = new ContentDialog
            {
                Title = ResourceLoader.GetString("FileOpenErrorDialog_Title"),
                Content = string.Format(ResourceLoader.GetString("FileOpenErrorDialog_Content"), filePath, errorMsg),
                PrimaryButtonText = ResourceLoader.GetString("FileOpenErrorDialog_PrimaryButtonText"),
                RequestedTheme = ThemeSettingsService.ThemeMode
            };
            return fileOpenErrorDialog;
        }

        public static ContentDialog GetFileSaveErrorDialog(string filePath, string errorMsg)
        {
            var content = string.IsNullOrEmpty(filePath) ? errorMsg : string.Format(ResourceLoader.GetString("FileSaveErrorDialog_Content"), filePath, errorMsg);
            return new ContentDialog
            {
                Title = ResourceLoader.GetString("FileSaveErrorDialog_Title"),
                Content = content,
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

        public static ContentDialog GetRevertAllChangesConfirmationDialog(string fileNameOrPath, Action confirmedAction)
        {
            ContentDialog revertAllChangesConfirmationDialog = new ContentDialog
            {
                Title = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_Title"),
                Content = string.Format(ResourceLoader.GetString("RevertAllChangesConfirmationDialog_Content"), fileNameOrPath),
                PrimaryButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_PrimaryButtonText"),
                CloseButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_CloseButtonText"),
                RequestedTheme = ThemeSettingsService.ThemeMode,
            };
            revertAllChangesConfirmationDialog.PrimaryButtonClick += (dialog, args) => { confirmedAction(); };
            return revertAllChangesConfirmationDialog;
        }
    }
}
