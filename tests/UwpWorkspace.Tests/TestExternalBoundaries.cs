// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Windows.Storage;

namespace Notepads.Infrastructure.Settings
{
    // Preferences are initialized afresh in every explicit phase. Never use package LocalSettings.
    public static class ApplicationSettingsStore
    {
        private static readonly Dictionary<string, object> Values = new(StringComparer.Ordinal);
        public static object Read(string key) { lock (Values) return Values.TryGetValue(key, out var value) ? value : null; }
        public static void Write(string key, object value) { lock (Values) Values[key] = value; }
        public static bool Remove(string key) { lock (Values) return Values.Remove(key); }
    }
}

namespace Notepads.Infrastructure.Shell
{
    public static class MRUService
    {
        private static readonly List<IStorageItem> Items = [];
        public static bool TryAdd(IStorageItem item) { Items.Remove(item); Items.Insert(0, item); return true; }
        public static Task<IList<IStorageItem>> GetMostRecentlyUsedListAsync(int top = 10) =>
            Task.FromResult<IList<IStorageItem>>(Items.Take(top).ToArray());
        public static void ClearAll() => Items.Clear();
    }
}

namespace Notepads.Infrastructure.Diagnostics
{
    public static class AnalyticsService
    {
        public static void TrackEvent(string eventName, IDictionary<string, string> properties = null) { }
        public static void TrackError(Exception exception, IDictionary<string, string> properties = null) { }
    }
}
