// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Services
{
    using System;
    using System.Collections.Concurrent;
    using System.Diagnostics;
    using System.IO;
    using System.Text.Json;
    using System.Threading.Tasks;
    using Windows.Storage;

    public class AutosaveSettings
    {
        public bool AutosaveEnabled { get; set; } = false;
        public bool ShowSavedNotification { get; set; } = true;
    }

    public static class EnhancedAutosaveService
    {
        public static bool FeatureFlag_EnhancedAutosave = true;

        private static string SettingsFilePath => Path.Combine(ApplicationData.Current.LocalFolder.Path, "settings", "autosave.json");

        private static AutosaveSettings _settings = new AutosaveSettings();

        public static bool IsAutosaveEnabled
        {
            get => _settings.AutosaveEnabled;
            set
            {
                if (_settings.AutosaveEnabled != value)
                {
                    _settings.AutosaveEnabled = value;
                    SaveSettings();
                    AutosaveStateChanged?.Invoke(null, value);
                }
            }
        }

        public static bool ShowSavedNotification
        {
            get => _settings.ShowSavedNotification;
            set
            {
                if (_settings.ShowSavedNotification != value)
                {
                    _settings.ShowSavedNotification = value;
                    SaveSettings();
                    NotificationSettingChanged?.Invoke(null, value);
                }
            }
        }

        public static event EventHandler<bool> AutosaveStateChanged;
        public static event EventHandler<bool> NotificationSettingChanged;
        public static event EventHandler<DateTime> LastSaveTimeChanged;

        public static DateTime LastSaveTime { get; private set; }

        private static BlockingCollection<Func<Task>> _saveQueue;

        public static void Initialize()
        {
            if (!FeatureFlag_EnhancedAutosave) return;
            LoadSettings();
            _saveQueue = new BlockingCollection<Func<Task>>();
            StartAsyncQueue();
        }

        private static void LoadSettings()
        {
            try
            {
                if (File.Exists(SettingsFilePath))
                {
                    string json = File.ReadAllText(SettingsFilePath);
                    _settings = JsonSerializer.Deserialize<AutosaveSettings>(json) ?? new AutosaveSettings();
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(EnhancedAutosaveService)}] LoadSettings error: {ex.Message}");
            }
        }

        private static void SaveSettings()
        {
            try
            {
                string dir = Path.GetDirectoryName(SettingsFilePath);
                if (!Directory.Exists(dir))
                {
                    Directory.CreateDirectory(dir);
                }
                string json = JsonSerializer.Serialize(_settings);
                File.WriteAllText(SettingsFilePath, json);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(EnhancedAutosaveService)}] SaveSettings error: {ex.Message}");
            }
        }

        private static void StartAsyncQueue()
        {
            Task.Run(async () =>
            {
                foreach (var saveAction in _saveQueue.GetConsumingEnumerable())
                {
                    try
                    {
                        var sw = Stopwatch.StartNew();
                        await saveAction();
                        sw.Stop();
                        LastSaveTime = DateTime.Now;
                        LastSaveTimeChanged?.Invoke(null, LastSaveTime);
                        LoggingService.LogInfo($"[{nameof(EnhancedAutosaveService)}] Save completed in {sw.ElapsedMilliseconds}ms. IO count: 1");
                    }
                    catch (Exception ex)
                    {
                        LoggingService.LogError($"[{nameof(EnhancedAutosaveService)}] AsyncQueue error: {ex.Message}");
                    }
                }
            });
        }

        public static void QueueSave(Func<Task> saveAction)
        {
            if (FeatureFlag_EnhancedAutosave && IsAutosaveEnabled && _saveQueue != null)
            {
                _saveQueue.Add(saveAction);
            }
        }
    }
}
