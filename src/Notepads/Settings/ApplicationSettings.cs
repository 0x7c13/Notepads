namespace Notepads.Settings
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Notepads.Services;
    using Windows.Storage;

    public static class ApplicationSettingsStore
    {
        private static readonly IReadOnlyList<string> _roamingCandidatesKeys = new List<string>()
        {
            // App related
            SettingsKey.AlwaysOpenNewWindowBool,

            // Theme related
            SettingsKey.RequestedThemeStr,
            SettingsKey.UseWindowsThemeBool,
            SettingsKey.AppBackgroundTintOpacityDouble,
            SettingsKey.AppAccentColorHexStr,
            SettingsKey.CustomAccentColorHexStr,
            SettingsKey.UseWindowsAccentColorBool,

            // Editor related
            SettingsKey.EditorFontFamilyStr,
            SettingsKey.EditorFontSizeInt,
            SettingsKey.EditorFontStyleStr,
            SettingsKey.EditorFontWeightUshort,
            SettingsKey.EditorDefaultTextWrappingStr,
            SettingsKey.EditorDefaultLineHighlighterViewStateBool,
            SettingsKey.EditorDefaultLineEndingStr,
            SettingsKey.EditorDefaultEncodingCodePageInt,
            SettingsKey.EditorDefaultDecodingCodePageInt,
            SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool,
            SettingsKey.EditorDefaultTabIndentsInt,
            SettingsKey.EditorDefaultSearchEngineStr,
            SettingsKey.EditorCustomMadeSearchUrlStr,
            SettingsKey.EditorShowStatusBarBool,
            SettingsKey.EditorEnableSessionBackupAndRestoreBool,
            SettingsKey.EditorHighlightMisspelledWordsBool,
            SettingsKey.EditorDefaultDisplayLineNumbersBool,
            SettingsKey.EditorEnableSmartCopyBool
        };

        public static void Initialize()
        {
            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
            ApplicationDataContainer roamingSettings = Windows.Storage.ApplicationData.Current.RoamingSettings;
            if (roamingSettings.Values.Count >= localSettings.Values.Count)
            {
                foreach(var value in roamingSettings.Values)
                {
                    localSettings.Values.TryAdd(value.Key, value.Value);
                }
            }
        }

        public static object Read(string key)
        {
            object obj = null;

            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
            if (localSettings.Values.ContainsKey(key))
            {
                obj = localSettings.Values[key];
            }

            return obj;
        }

        public static void Write(string key, object obj)
        {
            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
            localSettings.Values[key] = obj;
            if (IsRoamingApplicable(key))
            {
                ApplicationDataContainer roamingSettings = Windows.Storage.ApplicationData.Current.RoamingSettings;
                roamingSettings.Values[key] = obj;
            }
        }

        public static bool Remove(string key)
        {
            try
            {
                ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
                return localSettings.Values.Remove(key);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(ApplicationSettingsStore)}] Failed to remove key [{key}] from application settings: {ex.Message}");
            }

            return false;
        }

        private static bool IsRoamingApplicable(string key)
        {
            return _roamingCandidatesKeys.Contains(key);
        }
    }
}