// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Settings
{
    using System;
    using Notepads.Services;
    using Windows.Storage;

    public static class ApplicationSettingsStore
    {
        public static object Read(string key)
        {
            object obj = null;

            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;

            if (localSettings.Values.TryGetValue(key, out var value))
            {
                obj = value;
            }

            return obj;
        }

        public static void Write(string key, object obj)
        {
            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
            localSettings.Values[key] = obj;
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
    }
}