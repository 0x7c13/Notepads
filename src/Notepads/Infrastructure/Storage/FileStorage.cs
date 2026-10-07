// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;
using Windows.Storage.FileProperties;

namespace Notepads.Infrastructure.Storage;

public static class FileStorage
{
    private static async Task<BasicProperties> GetFilePropertiesAsync(StorageFile file)
    {
        return await file.GetBasicPropertiesAsync();
    }

    public static async Task<long> GetDateModifiedAsync(StorageFile file)
    {
        var properties = await GetFilePropertiesAsync(file);
        var dateModified = properties.DateModified;
        return dateModified.ToFileTime();
    }

    public static bool IsFileReadOnly(StorageFile file)
    {
        return (file.Attributes & Windows.Storage.FileAttributes.ReadOnly) != 0;
    }

    public static async Task<StorageFile> GetFileAsync(string filePath)
    {
        try
        {
            return await StorageFile.GetFileFromPathAsync(filePath);
        }
        catch
        {
            return null;
        }
    }

    public static async Task<bool> FileExistsAsync(StorageFile file)
    {
        try
        {
            using (var stream = await file.OpenStreamForReadAsync()) { }
            return true;
        }
        catch (FileNotFoundException)
        {
            return false;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(FileStorage)}] Failed to check if file [{file.Path}] exists: {ex.Message}", consoleOnly: true);
            return true;
        }
    }
}
