// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.Storage.AccessCache;
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
                AnalyticsService.TrackEvent("FailedToAddTokenInFutureAccessList",
                    new Dictionary<string, string>()
                    {
                        { "ItemCount", GetFutureAccessListItemCount().ToString() },
                        { "Exception", ex.Message }
                    });
            }
            return false;
        }

        private static int GetFutureAccessListItemCount()
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