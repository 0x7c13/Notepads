// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Text;
using Notepads.Features.Preferences;
using Notepads.Features.WebSearch;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Theming;
using Windows.UI;
using Windows.UI.Text;
using Windows.UI.Xaml;

// Deterministic settings at the application boundary. The editor, command and
// text-processing implementations are linked directly from production sources.
namespace Notepads.Features.Preferences
{
    internal static class ApplicationPreferences
    {
        public static void Initialize() { }
        public static string EditorFontFamily => "Consolas";
        public static int EditorFontSize => 14;
        public static FontSlant EditorFontStyle => FontSlant.Normal;
        public static ushort EditorFontWeight => 400;
        public static bool EditorDefaultWordWrap => false;
        public static bool EditorDisplayLineNumbers => true;
        public static bool EditorDisplayLineHighlighter => true;
        public static int EditorDefaultTabIndents { get; set; } = 4;
        public static Encoding EditorDefaultDecoding { get; set; }
        public static SearchEngine EditorDefaultSearchEngine => SearchEngine.Bing;
        public static string EditorCustomMadeSearchUrl => "https://www.bing.com/search?q={0}";
#pragma warning disable CS0067
        public static event EventHandler<string> OnFontFamilyChanged;
        public static event EventHandler<int> OnFontSizeChanged;
        public static event EventHandler<FontSlant> OnFontStyleChanged;
        public static event EventHandler<ushort> OnFontWeightChanged;
        public static event EventHandler<bool> OnDefaultTextWrappingChanged;
        public static event EventHandler<bool> OnDefaultDisplayLineNumbersViewStateChanged;
        public static event EventHandler<bool> OnDefaultLineHighlighterViewStateChanged;
#pragma warning restore CS0067
    }
}

namespace Notepads.Presentation.Theming
{
    internal static class ThemeSettingsService
    {
        public static void Initialize() { }
        public static Color AppAccentColor { get; private set; } = Colors.DodgerBlue;
        public static ElementTheme ThemeMode { get; private set; } = ElementTheme.Dark;
        public static event EventHandler<ElementTheme> OnThemeChanged;
        public static void SetTheme(ElementTheme theme)
        {
            if (ThemeMode == theme) return;
            ThemeMode = theme;
            OnThemeChanged?.Invoke(null, theme);
        }
        public static event EventHandler<Color> OnAccentColorChanged;
        public static void SetAccentColor(Color color)
        {
            AppAccentColor = color;
            OnAccentColorChanged?.Invoke(null, color);
        }
    }
}

namespace Notepads.Infrastructure.Diagnostics
{
    internal static class LoggingService
    {
        public static void LogError(string message, bool consoleOnly = false) => System.Diagnostics.Debug.WriteLine(message);
        public static void LogInfo(string message, bool consoleOnly = false) => System.Diagnostics.Debug.WriteLine(message);
        public static void LogWarning(string message, bool consoleOnly = false) => System.Diagnostics.Debug.WriteLine(message);
        public static void LogException(Exception exception, bool consoleOnly = false) => System.Diagnostics.Debug.WriteLine(exception);
    }
    internal static class AnalyticsService
    {
        public static void TrackEvent(string name, IDictionary<string, string> properties = null) { }
    }
}
