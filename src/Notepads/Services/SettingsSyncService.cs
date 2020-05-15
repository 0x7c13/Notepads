namespace Notepads.Services
{
    using Microsoft.Toolkit.Uwp.Helpers;
    using Notepads.Utilities;
    using System;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;

    public static class SettingsSyncService
    {
        public static CoreDispatcher Dispatcher;

        public static Action<object> EditorFontFamily = (value) =>
        {
            AppSettingsService.EditorFontFamily = (string)value;
        };

        public static Action<object> EditorFontSize = (value) =>
        {
            AppSettingsService.EditorFontSize = (int)value;
        };

        public static Action<object> EditorFontStyle = (value) =>
        {
            Enum.TryParse(typeof(FontStyle), (string)value, out var result);
            AppSettingsService.EditorFontStyle = (FontStyle)result;
        };

        public static Action<object> EditorFontWeight = (value) =>
        {
            AppSettingsService.EditorFontWeight = new FontWeight()
            {
                Weight = (ushort)value
            };
        };

        public static Action<object> EditorDefaultTextWrapping = (value) =>
        {
            Enum.TryParse(typeof(TextWrapping), (string)value, out var result);
            AppSettingsService.EditorDefaultTextWrapping = (TextWrapping)result;
        };

        public static Action<object> EditorDisplayLineHighlighter = (value) =>
        {
            AppSettingsService.EditorDisplayLineHighlighter = (bool)value;
        };

        public static Action<object> EditorDisplayLineNumbers = (value) =>
        {
            AppSettingsService.EditorDisplayLineNumbers = (bool)value;
        };

        public static Action<object> EditorDefaultLineEnding = (value) =>
        {
            Enum.TryParse(typeof(LineEnding), (string)value, out var result);
            AppSettingsService.EditorDefaultLineEnding = (LineEnding)result;
        };

        public static Action<object> EditorDefaultEncoding = (value) =>
        {
            AppSettingsService.InitializeEncodingSettings();
        };

        public static Action<object> EditorDefaultDecoding = (value) =>
        {
            AppSettingsService.InitializeDecodingSettings();
        };

        public static Action<object> EditorDefaultTabIndents = (value) =>
        {
            AppSettingsService.EditorDefaultTabIndents = (int)value;
        };

        public static Action<object> EditorDefaultSearchEngine = (value) =>
        {
            Enum.TryParse(typeof(SearchEngine), (string)value, out var result);
            AppSettingsService.EditorDefaultSearchEngine = (SearchEngine)result;
        };

        public static Action<object> EditorCustomMadeSearchUrl = (value) =>
        {
            AppSettingsService.EditorCustomMadeSearchUrl = (string)value;
        };

        public static Action<object> ShowStatusBar = (value) =>
        {
            AppSettingsService.ShowStatusBar = (bool)value;
        };

        public static Action<object> IsHighlightMisspelledWordsEnabled = (value) =>
        {
            AppSettingsService.IsHighlightMisspelledWordsEnabled = (bool)value;
        };

        public static Action<object> AlwaysOpenNewWindow = (value) =>
        {
            AppSettingsService.AlwaysOpenNewWindow = (bool)value;
        };

        public static Action<object> UseWindowsAccentColor = (value) =>
        {
            ThemeSettingsService.UseWindowsAccentColor = (bool)value;
        };

        public static Action<object> AppBackgroundPanelTintOpacity = async (value) =>
        {
            await Dispatcher.RunAsync(CoreDispatcherPriority.High, () =>
            {
                ThemeSettingsService.AppBackgroundPanelTintOpacity = (double)value;
            });
        };

        public static Action<object> AppAccentColor = (value) =>
        {
            ThemeSettingsService.AppAccentColor = ((string)value).ToColor();
        };

        public static Action<object> CustomAccentColor = (value) =>
        {
            ThemeSettingsService.CustomAccentColor = ((string)value).ToColor();
        };

        public static Action<object> ThemeMode = (value) =>
        {
            Enum.TryParse(typeof(ElementTheme), (string)value, out var result);
            ThemeSettingsService.ThemeMode = (ElementTheme)result;
        };
    }
}
