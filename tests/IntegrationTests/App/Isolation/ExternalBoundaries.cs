// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Windows.Storage;

namespace Notepads.Infrastructure.Shell
{
    public static class MRUService
    {
        private static readonly List<IStorageItem> Items = [];
        public static bool TryAdd(IStorageItem item) { Items.Remove(item); Items.Insert(0, item); return true; }
        public static Task<IList<IStorageItem>> GetMostRecentlyUsedListAsync(int top = 10) =>
            Task.FromResult<IList<IStorageItem>>(Items.Take(top).ToArray());
        public static void ClearAll() => Items.Clear();
        // Seed only the OS list boundary; production menu filtering and commands still run.
        internal static void SeedForTest(params IStorageItem[] items)
        {
            Items.Clear();
            Items.AddRange(items);
        }
    }
}

namespace Notepads.Infrastructure.Diagnostics
{
    public static class AnalyticsService
    {
        internal static event Action<string> EventRecorded;
        public static void TrackEvent(string eventName, IDictionary<string, string> properties = null) => EventRecorded?.Invoke(eventName);
        public static void TrackError(Exception exception, IDictionary<string, string> properties = null) { }
    }
}
