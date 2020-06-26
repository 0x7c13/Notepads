namespace Notepads.Services
{
    using Notepads.Extensions;
    using Notepads.Settings;
    using Notepads.Views.MainPage;
    using System;
    using System.Collections.Generic;
    using Windows.Storage;

    public static class InterInstanceSyncService
    {
        private static NotepadsMainPage _notepadsMainPage = null;

        public static readonly string OpenRecentKey = "BuildOpenRecentButtonSubItems";

        public static IReadOnlyDictionary<string, Action> SyncManager = new Dictionary<string, Action>
        {
            {SettingsKey.AppBackgroundTintOpacityDouble, () => ThemeSettingsService.InitializeAppBackgroundPanelTintOpacity(true)},
            {SettingsKey.RequestedThemeStr, () => ThemeSettingsService.InitializeThemeMode(true)},
            {SettingsKey.UseWindowsAccentColorBool, () => { } },
            {SettingsKey.AppAccentColorHexStr, () => { } },
            {SettingsKey.CustomAccentColorHexStr, () => ThemeSettingsService.InitializeCustomAccentColor()},
            {SettingsKey.EditorDefaultLineHighlighterViewStateBool, () => AppSettingsService.InitializeDisplayLineHighlighterSettings(true)},
            {SettingsKey.EditorDefaultDisplayLineNumbersBool, () => AppSettingsService.InitializeDisplayLineNumbersSettings(true)},
            {SettingsKey.EditorDefaultTabIndentsInt, () => AppSettingsService.InitializeTabIndentsSettings(true)},
            {SettingsKey.EditorDefaultTextWrappingStr, () => AppSettingsService.InitializeTextWrappingSettings(true)},
            {SettingsKey.EditorFontFamilyStr, () => AppSettingsService.InitializeFontFamilySettings(true)},
            {SettingsKey.EditorFontSizeInt, () => AppSettingsService.InitializeFontSizeSettings(true)},
            {SettingsKey.EditorFontStyleStr, () => AppSettingsService.InitializeFontStyleSettings(true)},
            {SettingsKey.EditorFontWeightUshort, () => AppSettingsService.InitializeFontWeightSettings(true)},
            {SettingsKey.EditorHighlightMisspelledWordsBool, () => AppSettingsService.InitializeSpellingSettings(true)},
            {SettingsKey.EditorDefaultEncodingCodePageInt, () => AppSettingsService.InitializeEncodingSettings(true)},
            {SettingsKey.EditorDefaultLineEndingStr, () => AppSettingsService.InitializeLineEndingSettings(true)},
            {SettingsKey.EditorShowStatusBarBool, () => AppSettingsService.InitializeStatusBarSettings(true)},
            {SettingsKey.EditorCustomMadeSearchUrlStr, () => AppSettingsService.InitializeCustomSearchUrlSettings()},
            {SettingsKey.EditorDefaultDecodingCodePageInt, () => AppSettingsService.InitializeDecodingSettings()},
            {SettingsKey.EditorDefaultSearchEngineStr, () => AppSettingsService.InitializeSearchEngineSettings()},
            {SettingsKey.EditorEnableSmartCopyBool, () => AppSettingsService.InitializeSmartCopySettings() },
            {SettingsKey.AlwaysOpenNewWindowBool, () => AppSettingsService.InitializeAppOpeningPreferencesSettings() },
            {OpenRecentKey, async () => await _notepadsMainPage.BuildOpenRecentButtonSubItems(false) }
        };

        public static void Initialize(NotepadsMainPage page)
        {
            _notepadsMainPage = page;
            ApplicationData.Current.DataChanged += Application_OnDataChanged;
        }

        private static async void Application_OnDataChanged(ApplicationData sender, object args)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.LastChangedSettingsAppInstanceIdStr) is string lastChangedSettingsAppInstanceIdStr &&
                lastChangedSettingsAppInstanceIdStr == App.Id.ToString())
            {
                return;
            }

            if (ApplicationSettingsStore.Read(SettingsKey.LastChangedSettingsKeyStr) is string lastChangedSettingsKeyStr &&
                SyncManager.ContainsKey(lastChangedSettingsKeyStr) && _notepadsMainPage != null)
            {
                await DispatcherExtensions.CallOnUIThreadAsync(_notepadsMainPage.Dispatcher, () =>
                {
                    if (lastChangedSettingsKeyStr != OpenRecentKey) _notepadsMainPage.CloseSettingsPane();
                    SyncManager[lastChangedSettingsKeyStr].Invoke();
                    ThemeSettingsService.InitializeAppAccentColor(true);
                });
            }
        }
    }
}
