using Notepads.Services;
using Notepads.Utilities;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Windows.UI;
using Windows.UI.Core;
using Windows.UI.Xaml;

namespace Notepads.Settings
{
    public delegate void Settings(object value);

    public static class SettingsDelegate
    {
        public static CoreDispatcher Dispatcher;

        public static Settings EditorFontFamily = SetEditorFontFamily;
        public static Settings EditorFontSize = SetEditorFontSize;
        public static Settings EditorDefaultTextWrapping = SetEditorDefaultTextWrapping;
        public static Settings IsLineHighlighterEnabled = SetIsLineHighlighterEnabled;
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
            EditorSettingsService.EditorFontFamily = (string)value;
        }

        private static void SetEditorFontSize(object value)
        {
            EditorSettingsService.EditorFontSize = (int)value;
        }

        private static void SetEditorDefaultTextWrapping(object value)
        {
            Enum.TryParse(typeof(TextWrapping), (string)value, out var result);
            EditorSettingsService.EditorDefaultTextWrapping = (TextWrapping)result;
        }

        private static void SetIsLineHighlighterEnabled(object value)
        {
            EditorSettingsService.IsLineHighlighterEnabled = (bool)value;
        }

        private static void SetEditorDefaultLineEnding(object value)
        {
            Enum.TryParse(typeof(LineEnding), (string)value, out var result);
            EditorSettingsService.EditorDefaultLineEnding = (LineEnding)result;
        }

        private static void SetEditorDefaultEncoding(object value)
        {
            EditorSettingsService.InitializeEncodingSettings();
        }

        private static void SetEditorDefaultDecoding(object value)
        {
            EditorSettingsService.InitializeDecodingSettings();
        }

        private static void SetEditorDefaultTabIndents(object value)
        {
            EditorSettingsService.EditorDefaultTabIndents = (int)value;
        }

        private static void SetEditorDefaultSearchEngine(object value)
        {
            Enum.TryParse(typeof(SearchEngine), (string)value, out var result);
            EditorSettingsService.EditorDefaultSearchEngine = (SearchEngine)result;
        }

        private static void SetEditorCustomMadeSearchUrl(object value)
        {
            EditorSettingsService.EditorCustomMadeSearchUrl = (string)value;
        }

        private static void SetShowStatusBar(object value)
        {
            EditorSettingsService.ShowStatusBar = (bool)value;
        }

        private static void SetIsHighlightMisspelledWordsEnabled(object value)
        {
            EditorSettingsService.IsHighlightMisspelledWordsEnabled = (bool)value;
        }

        private static void SetAlwaysOpenNewWindow(object value)
        {
            EditorSettingsService.AlwaysOpenNewWindow = (bool)value;
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
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                ThemeSettingsService.AppBackgroundPanelTintOpacity = (double)value;
            });
        }

        private static void SetAppAccentColor(object value)
        {
            ThemeSettingsService.AppAccentColor = ThemeSettingsService.GetColor((string)value);
        }

        private static void SetCustomAccentColor(object value)
        {
            ThemeSettingsService.CustomAccentColor = ThemeSettingsService.GetColor((string)value);
        }

        private static void SetThemeMode(object value)
        {
            Enum.TryParse(typeof(ElementTheme), (string)value, out var result);
            ThemeSettingsService.ThemeMode = (ElementTheme)result;
        }
    }
}
