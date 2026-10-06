// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
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

    public static async Task<StorageFile> GetOrCreateFileAsync(StorageFolder folder, string fileName)
    {
        try
        {
            return await folder.CreateFileAsync(fileName, CreationCollisionOption.OpenIfExists);
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(FileStorage)}] Failed to get or create file, Exception: {ex.Message}");
            AnalyticsService.TrackEvent("GetOrCreateFileAsync_Failed", new Dictionary<string, string>()
            {
                { "Exception", ex.ToString() },
            });
            throw; // Rethrow
        }
    }

    internal static async Task DeleteFileAsync(string filePath, StorageDeleteOption deleteOption = StorageDeleteOption.PermanentDelete)
    {
        try
        {
            var file = await GetFileAsync(filePath);
            if (file != null)
            {
                await file.DeleteAsync(deleteOption);
            }
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(FileStorage)}] Failed to delete file: {filePath}, Exception: {ex.Message}");
        }
    }

    public static async Task<StorageFolder> GetOrCreateAppFolderAsync(string folderName)
    {
        StorageFolder localFolder = ApplicationData.Current.LocalFolder;
        return await localFolder.CreateFolderAsync(folderName, CreationCollisionOption.OpenIfExists);
    }

    public static async Task<StorageFile> CreateFileAsync(StorageFolder folder, string fileName, CreationCollisionOption option = CreationCollisionOption.ReplaceExisting)
    {
        return await folder.CreateFileAsync(fileName, option);
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
