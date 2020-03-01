namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Microsoft.AppCenter.Analytics;

    public static class MRUService
    {
        public static void Add(IStorageItem item)
        {
            Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList.Add(item);
        }

        public static async Task<IList<IStorageItem>> Get(int top = 10)
        {
            IList<IStorageItem> items = new List<IStorageItem>();

            try
            {
                var mru = Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList;

                for (int i = 0; i < mru.Entries.Count; i++)
                {
                    if (i >= top) break;
                    items.Add(await mru.GetItemAsync(mru.Entries[i].Token));
                }
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("MRUService_FailedToGetMostRecentlyUsedList", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() }
                });
            }

            return items;
        }

        public static void ClearAll()
        {
            Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList.Clear();
        }
    }
}