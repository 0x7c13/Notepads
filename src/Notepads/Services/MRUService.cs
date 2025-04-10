﻿// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.Storage.AccessCache;

    public static class MRUService
    {
        public static bool TryAdd(IStorageItem item)
        {
            try
            {
                Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList
                    .Add(item, string.Empty, RecentStorageItemVisibility.AppAndSystem);
                return true;
            }
            catch (Exception ex)
            {
                AnalyticsService.TrackEvent("MRUService_FailedToAddStorageItemToMRU", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() }
                });
                return false;
            }
        }

        public static async Task<IList<IStorageItem>> GetMostRecentlyUsedListAsync(int top = 10)
        {
            IList<IStorageItem> items = new List<IStorageItem>();

            try
            {
                var mru = Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList;

                for (int i = 0; i < mru.Entries.Count; i++)
                {
                    if (i >= top) break;

                    try
                    {
                        items.Add(await mru.GetItemAsync(mru.Entries[i].Token, AccessCacheOptions.SuppressAccessTimeUpdate));
                    }
                    catch (Exception)
                    {
                        // File might be renamed or deleted
                        // We should continue in case of GetItemAsync() failure
                        continue;
                    }
                }
            }
            catch (Exception ex)
            {
                AnalyticsService.TrackEvent("MRUService_FailedToGetMRU", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() }
                });
            }

            return items;
        }

        public static void ClearAll()
        {
            try
            {
                Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList.Clear();
            }
            catch (Exception ex)
            {
                AnalyticsService.TrackEvent("MRUService_FailedToClearMRU", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() }
                });
            }
        }
    }
}