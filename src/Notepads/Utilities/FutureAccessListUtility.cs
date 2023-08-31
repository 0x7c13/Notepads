namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.Storage.AccessCache;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;

    public static class FutureAccessListUtility
    {
        public static async Task<StorageFile> GetFileFromFutureAccessListAsync(string token)
        {
            try
            {
                if (StorageApplicationPermissions.FutureAccessList.ContainsItem(token))
                {
                    return await StorageApplicationPermissions.FutureAccessList.GetFileAsync(token);
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(FutureAccessListUtility)}] Failed to get file from future access list: {ex.Message}");
            }
            return null;
        }

        public static async Task<bool> TryAddOrReplaceTokenInFutureAccessListAsync(string token, StorageFile file)
        {
            try
            {
                if (await FileSystemUtility.FileExistsAsync(file))
                {
                    StorageApplicationPermissions.FutureAccessList.AddOrReplace(token, file);
                    return true;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(FutureAccessListUtility)}] Failed to add file [{file.Path}] to future access list: {ex.Message}");
                Analytics.TrackEvent("FailedToAddTokenInFutureAccessList",
                    new Dictionary<string, string>()
                    {
                        { "ItemCount", GetFutureAccessListItemCount().ToString() },
                        { "Exception", ex.Message }
                    });
            }
            return false;
        }

        public static int GetFutureAccessListItemCount()
        {
            try
            {
                return StorageApplicationPermissions.FutureAccessList.Entries.Count;
            }
            catch
            {
                return -1;
            }
        }
    }
}