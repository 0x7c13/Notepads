// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;
using Windows.Storage.AccessCache;

namespace Notepads.Infrastructure.Storage;

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

}
