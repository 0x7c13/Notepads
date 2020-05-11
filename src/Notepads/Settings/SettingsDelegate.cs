namespace Notepads.Settings
{
    using Microsoft.Toolkit.Uwp.Helpers;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;

    public delegate void Settings(object value);

    public static class SettingsDelegate
    {
        public static CoreDispatcher Dispatcher;

        public static Settings EditorFontFamily = SetEditorFontFamily;
        public static Settings EditorFontSize = SetEditorFontSize;
        public static Settings EditorFontStyle = SetEditorFontStyle;
        public static Settings EditorFontWeight = SetEditorFontWeight; 
        public static Settings EditorDefaultTextWrapping = SetEditorDefaultTextWrapping;
        public static Settings EditorDisplayLineHighlighter = SetEditorDisplayLineHighlighter;
        public static Settings EditorDisplayLineNumbers = SetEditorDisplayLineNumbers;
        public static Settings EditorDefaultLineEnding = SetEditorDefaultLineEnding;
        public static Settings EditorDefaultEncoding = SetEditorDefaultEncoding;
        public static Settings EditorDefaultDecoding = SetEditorDefaultDecoding;
        public static Settings EditorDefaultTabIndents = SetEditorDefaultTabIndents;
        public static Settings EditorDefaultSearchEngine = SetEditorDefaultSearchEngine;
        public static Settings EditorCustomMadeSearchUrl = SetEditorCustomMadeSearchUrl;
        public static Settings ShowStatusBar = SetShowStatusBar;
        public static Settings IsHighlightMisspelledWordsEnabled = SetIsHighlightMisspelledWordsEnabled;
        public static Settings AlwaysOpenNewWindow = SetAlwaysOpenNewWindow;

        private static void SetEditorFontFamily(object value)
        {
            AppSettingsService.EditorFontFamily = (string)value;
        }

        private static void SetEditorFontSize(object value)
        {
            AppSettingsService.EditorFontSize = (int)value;
        }

        private static void SetEditorFontStyle(object value)
        {
            Enum.TryParse(typeof(FontStyle), (string)value, out var result);
            AppSettingsService.EditorFontStyle = (FontStyle)result;
        }

        private static void SetEditorFontWeight(object value)
        {
            AppSettingsService.EditorFontWeight = new FontWeight()
            {
                Weight = (ushort)value
            };
        }

        private static void SetEditorDefaultTextWrapping(object value)
        {
            Enum.TryParse(typeof(TextWrapping), (string)value, out var result);
            AppSettingsService.EditorDefaultTextWrapping = (TextWrapping)result;
        }

        private static void SetEditorDisplayLineHighlighter(object value)
        {
            AppSettingsService.EditorDisplayLineHighlighter = (bool)value;
        }

        private static void SetEditorDisplayLineNumbers(object value)
        {
            AppSettingsService.EditorDisplayLineNumbers = (bool)value;
        }

        private static void SetEditorDefaultLineEnding(object value)
        {
            Enum.TryParse(typeof(LineEnding), (string)value, out var result);
            AppSettingsService.EditorDefaultLineEnding = (LineEnding)result;
        }

        private static void SetEditorDefaultEncoding(object value)
        {
            AppSettingsService.InitializeEncodingSettings();
        }

        private static void SetEditorDefaultDecoding(object value)
        {
            AppSettingsService.InitializeDecodingSettings();
        }

        private static void SetEditorDefaultTabIndents(object value)
        {
            AppSettingsService.EditorDefaultTabIndents = (int)value;
        }

        private static void SetEditorDefaultSearchEngine(object value)
        {
            Enum.TryParse(typeof(SearchEngine), (string)value, out var result);
            AppSettingsService.EditorDefaultSearchEngine = (SearchEngine)result;
        }

        private static void SetEditorCustomMadeSearchUrl(object value)
        {
            AppSettingsService.EditorCustomMadeSearchUrl = (string)value;
        }

        private static void SetShowStatusBar(object value)
        {
            AppSettingsService.ShowStatusBar = (bool)value;
        }

        private static void SetIsHighlightMisspelledWordsEnabled(object value)
        {
            AppSettingsService.IsHighlightMisspelledWordsEnabled = (bool)value;
        }

        private static void SetAlwaysOpenNewWindow(object value)
        {
            AppSettingsService.AlwaysOpenNewWindow = (bool)value;
        }

        public static Settings UseWindowsAccentColor = SetUseWindowsAccentColor;
        public static Settings AppBackgroundPanelTintOpacity = SetAppBackgroundPanelTintOpacity;
        public static Settings AppAccentColor = SetAppAccentColor;
        public static Settings CustomAccentColor = SetCustomAccentColor;
        public static Settings ThemeMode = SetThemeMode;

        private static void SetUseWindowsAccentColor(object value)
        {
            ThemeSettingsService.UseWindowsAccentColor = (bool)value;
        }

        private static async void SetAppBackgroundPanelTintOpacity(object value)
        {
            await Dispatcher.RunAsync(CoreDispatcherPriority.High, () =>
            {
                ThemeSettingsService.AppBackgroundPanelTintOpacity = (double)value;
            });
        }

        private static void SetAppAccentColor(object value)
        {
            ThemeSettingsService.AppAccentColor = ((string)value).ToColor();
        }

        private static void SetCustomAccentColor(object value)
        {
            ThemeSettingsService.CustomAccentColor = ((string)value).ToColor();
        }

        private static void SetThemeMode(object value)
        {
            Enum.TryParse(typeof(ElementTheme), (string)value, out var result);
            ThemeSettingsService.ThemeMode = (ElementTheme)result;
        }
    }
}
