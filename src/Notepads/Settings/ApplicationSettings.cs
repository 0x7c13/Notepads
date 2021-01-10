namespace Notepads.Settings
{
    using System;
    using Notepads.Services;
    using Windows.Storage;

    public static class ApplicationSettingsStore
    {
        private static readonly ApplicationDataContainer _localSettings = ApplicationData.Current.LocalSettings;

        public static object Read(string key)
        {
            object obj = null;

            if (_localSettings.Values.ContainsKey(key))
            {
                obj = _localSettings.Values[key];
            }

            return obj;
        }

        public static void Write(string key, object obj)
        {
            if (_localSettings.Values.ContainsKey(key) && _localSettings.Values[key].Equals(obj)) return;

            _localSettings.Values[key] = obj;
            SignalDataChanged(key);
        }

        public static void SignalDataChanged(string key)
        {
            if (InterInstanceSyncService.SyncManager.ContainsKey(key))
            {
                _localSettings.Values[SettingsKey.LastChangedSettingsKeyStr] = key;
                _localSettings.Values[SettingsKey.LastChangedSettingsAppInstanceIdStr] = App.Id.ToString();
                ApplicationData.Current.SignalDataChanged();
            }
        }

        public static bool Remove(string key)
        {
            try
            {
                return _localSettings.Values.Remove(key);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(ApplicationSettingsStore)}] Failed to remove key [{key}] from application settings: {ex.Message}");
            }

            return false;
        }
    }
}