
using System.Linq;
using Microsoft.Toolkit.Uwp.Helpers;
using Notepads.Services;

namespace Notepads.Utilities
{
    using Notepads.Core;
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Storage;

    internal static class SessionUtility
    {
        private static string BackupFolderName = "BackupFiles";
        private static string SessionMetaDataFileName = "NotepadsSessionData.json";
        private static readonly ConcurrentDictionary<INotepadsCore, ISessionManager> SessionManagers = new ConcurrentDictionary<INotepadsCore, ISessionManager>();

        public static ISessionManager GetSessionManager(INotepadsCore notepadCore)
        {
            if (!SessionManagers.TryGetValue(notepadCore, out ISessionManager sessionManager))
            {
                sessionManager = new SessionManager(notepadCore);

                if (!SessionManagers.TryAdd(notepadCore, sessionManager))
                {
                    sessionManager = SessionManagers[notepadCore];
                }
            }

            return sessionManager;
        }

        public static async Task<StorageFolder> GetBackupFolderAsync()
        {
            return await FileSystemUtility.GetOrCreateAppFolder(BackupFolderName);
        }

        public static async Task<IReadOnlyList<StorageFile>> GetAllBackupFilesAsync()
        {
            StorageFolder backupFolder = await GetBackupFolderAsync();
            var files = await backupFolder.GetFilesAsync();
            return files.Where(file => !string.Equals(file.Name, SessionMetaDataFileName, StringComparison.OrdinalIgnoreCase)).ToList();
        }

        public static async Task<string> GetSerializedSessionMetaDataAsync()
        {
            try
            {
                StorageFolder backupFolder = await GetBackupFolderAsync();
                if (await backupFolder.FileExistsAsync(SessionMetaDataFileName))
                {
                    var data = await backupFolder.ReadTextFromFileAsync(SessionMetaDataFileName);
                    LoggingService.LogInfo($"[SessionUtility] Session metadata Loaded from {backupFolder}");
                    return data;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[SessionUtility] Failed to get session meta data: {ex.Message}");
            }

            return null;
        }

        public static async Task SaveSerializedSessionMetaDataAsync(string serializedData)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync();
            await backupFolder.WriteTextToFileAsync(serializedData, SessionMetaDataFileName, CreationCollisionOption.ReplaceExisting);
        }

        public static async Task DeleteSerializedSessionMetaDataAsync()
        {
            try
            {
                StorageFolder backupFolder = await GetBackupFolderAsync();
                if (await backupFolder.FileExistsAsync(SessionMetaDataFileName))
                {
                    var sessionDataFile = await backupFolder.GetFileAsync(SessionMetaDataFileName);
                    await sessionDataFile.DeleteAsync();
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[SessionUtility] Failed to delete session meta data: {ex.Message}");
            }
        }

        public static async Task<StorageFile> CreateNewFileInBackupFolderAsync(string fileName, CreationCollisionOption collisionOption)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync();
            return await backupFolder.CreateFileAsync(fileName, collisionOption);
        }
    }
}
