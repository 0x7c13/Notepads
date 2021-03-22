namespace Notepads.Services
{
    using Notepads.Core;
    using Notepads.Extensions;
    using Notepads.Settings;
    using Notepads.Views.MainPage;
    using System;
    using System.Collections.Generic;
    using Windows.Storage;

    public static class InterInstanceSyncService
    {
        private static NotepadsMainPage _notepadsMainPage = null;

        public static readonly string RecentFilesListKey = "BuildOpenRecentButtonSubItems";

        public static readonly IReadOnlyDictionary<string, Action<bool>> SyncManager = new Dictionary<string, Action<bool>>
        {
            { SettingsKey.AppBackgroundTintOpacityDouble, ThemeSettingsService.InitializeAppBackgroundPanelTintOpacity },
            { SettingsKey.RequestedThemeStr, ThemeSettingsService.InitializeThemeMode },
            { SettingsKey.UseWindowsAccentColorBool, ThemeSettingsService.InitializeAppAccentColor },
            { SettingsKey.AppAccentColorHexStr, ThemeSettingsService.InitializeAppAccentColor },
            { SettingsKey.CustomAccentColorHexStr, ThemeSettingsService.InitializeCustomAccentColor },
            { SettingsKey.EditorDefaultLineHighlighterViewStateBool, AppSettingsService.InitializeDisplayLineHighlighterSettings },
            { SettingsKey.EditorDefaultDisplayLineNumbersBool, AppSettingsService.InitializeDisplayLineNumbersSettings },
            { SettingsKey.EditorDefaultTabIndentsInt, AppSettingsService.InitializeTabIndentsSettings },
            { SettingsKey.EditorDefaultTextWrappingStr, AppSettingsService.InitializeTextWrappingSettings },
            { SettingsKey.EditorFontFamilyStr, AppSettingsService.InitializeFontFamilySettings },
            { SettingsKey.EditorFontSizeInt, AppSettingsService.InitializeFontSizeSettings },
            { SettingsKey.EditorFontStyleStr, AppSettingsService.InitializeFontStyleSettings },
            { SettingsKey.EditorFontWeightUshort, AppSettingsService.InitializeFontWeightSettings },
            { SettingsKey.EditorHighlightMisspelledWordsBool, AppSettingsService.InitializeSpellingSettings },
            { SettingsKey.EditorDefaultEncodingCodePageInt, AppSettingsService.InitializeEncodingSettings },
            { SettingsKey.EditorDefaultLineEndingStr, AppSettingsService.InitializeLineEndingSettings },
            { SettingsKey.EditorShowStatusBarBool, AppSettingsService.InitializeStatusBarSettings },
            { SettingsKey.EditorCustomMadeSearchUrlStr, AppSettingsService.InitializeCustomSearchUrlSettings },
            { SettingsKey.EditorDefaultDecodingCodePageInt, AppSettingsService.InitializeDecodingSettings },
            { SettingsKey.EditorDefaultSearchEngineStr, AppSettingsService.InitializeSearchEngineSettings },
            { SettingsKey.EditorEnableSmartCopyBool, AppSettingsService.InitializeSmartCopySettings },
            { SettingsKey.AlwaysOpenNewWindowBool, AppSettingsService.InitializeAppOpeningPreferencesSettings },
            { CoreKey.LaunchElevatedProcessFailedStr, DesktopExtensionService.OnElevetedProcessLaunchRequested },
            { CoreKey.LaunchElevatedProcessSuccessStr, DesktopExtensionService.OnElevetedProcessLaunchRequested },
            { RecentFilesListKey, async (permission) => await _notepadsMainPage.BuildOpenRecentButtonSubItems(!permission) }
        };

        public static void Initialize(NotepadsMainPage page)
        {
            _notepadsMainPage = page;
            ApplicationData.Current.DataChanged += Application_OnDataChanged;
        }

        private static async void Application_OnDataChanged(ApplicationData sender, object args)
        {
            if (ApplicationSettingsStore.Read(CoreKey.LastChangedSettingsAppInstanceIdStr) is string lastChangedSettingsAppInstanceIdStr &&
                lastChangedSettingsAppInstanceIdStr == App.Id.ToString())
            {
                return;
            }

            if (ApplicationSettingsStore.Read(CoreKey.LastChangedSettingsKeyStr) is string lastChangedSettingsKeyStr &&
                SyncManager.ContainsKey(lastChangedSettingsKeyStr) && _notepadsMainPage != null)
            {
                await DispatcherExtensions.CallOnUIThreadAsync(_notepadsMainPage.Dispatcher, () =>
                {
                    if (lastChangedSettingsKeyStr != RecentFilesListKey &&
                    lastChangedSettingsKeyStr != CoreKey.LaunchElevatedProcessSuccessStr &&
                    lastChangedSettingsKeyStr != CoreKey.LaunchElevatedProcessFailedStr)
                    {
                        _notepadsMainPage.CloseSettingsPane();
                    }

                    if (lastChangedSettingsKeyStr == CoreKey.LaunchElevatedProcessFailedStr)
                    {
                        SyncManager[lastChangedSettingsKeyStr].Invoke(false);
                    }
                    else
                    {
                        SyncManager[lastChangedSettingsKeyStr].Invoke(true);
                    }
                });
            }
        }
    }
}
