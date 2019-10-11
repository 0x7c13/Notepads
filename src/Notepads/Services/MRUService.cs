namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Storage;

    public static class MRUService
    {
        public static void Add(IStorageItem item)
        {
            Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList.Add(item);
        }

        public static async Task<IList<IStorageItem>> Get(int top = 10)
        {
            IList<IStorageItem> items = new List<IStorageItem>();

            var mru = Windows.Storage.AccessCache.StorageApplicationPermissions.MostRecentlyUsedList;

            for (int i = 0; i < mru.Entries.Count; i++)
            {
                if (i >= top) break;
                items.Add(await mru.GetItemAsync(mru.Entries[i].Token));
            }

            return items;
        }
    }
}