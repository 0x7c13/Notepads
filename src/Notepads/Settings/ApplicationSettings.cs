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
            if (localSettings.Values.ContainsKey(key))
            {
                obj = localSettings.Values[key];
            }

            return obj;
        }

        public static void Write(string key, object obj, bool saveToRoaming = false)
        {
            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
            localSettings.Values[key] = obj;

            //if (saveToRoaming)
            //{
            //    ApplicationDataContainer roamingSettings = Windows.Storage.ApplicationData.Current.RoamingSettings;
            //    roamingSettings.Values[key] = obj;
            //}
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
                LoggingService.LogError($"Failed to remove key [{key}] from application settings: {ex.Message}");
            }

            return false;
        }
    }
}