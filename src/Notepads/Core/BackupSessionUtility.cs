namespace Notepads.Utilities
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Microsoft.Toolkit.Uwp.Helpers;
    using Notepads.Core;
    using Notepads.Services;
    using Windows.Storage;
    using Microsoft.AppCenter.Analytics;
    internal static class BackupSessionUtility
    {
        public static async Task<StorageFolder> GetBackupFolderAsync(string backupFolderName)
        {
            return await FileSystemUtility.GetOrCreateAppFolder(backupFolderName);
        }

        public static async Task<IReadOnlyList<StorageFile>> GetAllBackupFilesAsync(string backupFolderName)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync(backupFolderName);
            return await backupFolder.GetFilesAsync();
        }

        public static async Task<StorageFile> CreateNewFileInBackupFolderAsync(string fileName, CreationCollisionOption collisionOption, string backupFolderName)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync(backupFolderName);
            return await backupFolder.CreateFileAsync(fileName, collisionOption);
        }
    }
}
