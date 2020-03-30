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

    internal static class SessionUtility
    {
        private const string BackupFolderName = "BackupFiles";
        private const string SessionMetaDataFileName = "NotepadsSessionData.json";
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
            return await backupFolder.GetFilesAsync();
        }

        public static async Task<string> GetSerializedSessionMetaDataAsync()
        {
            try
            {
                StorageFolder localFolder = ApplicationData.Current.LocalFolder;
                if (await localFolder.FileExistsAsync(SessionMetaDataFileName))
                {
                    var data = await localFolder.ReadTextFromFileAsync(SessionMetaDataFileName);
                    LoggingService.LogInfo($"[SessionUtility] Session metadata Loaded from {localFolder.Path}");
                    return data;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[SessionUtility] Failed to get session meta data: {ex.Message}");
                Analytics.TrackEvent("FailedToGetSerializedSessionMetaData", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() },
                    { "Message", ex.Message }
                });
            }

            return null;
        }

        public static async Task SaveSerializedSessionMetaDataAsync(string serializedData)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;

            // Attempt to delete session meta data file first in case it was not been deleted
            try
            {
                await DeleteSerializedSessionMetaDataAsync();
            }
            catch (Exception)
            {
                // ignored
            }

            await localFolder.WriteTextToFileAsync(serializedData, SessionMetaDataFileName, CreationCollisionOption.ReplaceExisting);
        }

        public static async Task DeleteSerializedSessionMetaDataAsync()
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            if (await localFolder.FileExistsAsync(SessionMetaDataFileName))
            {
                var sessionDataFile = await localFolder.GetFileAsync(SessionMetaDataFileName);
                await sessionDataFile.DeleteAsync();
            }
        }

        public static async Task<StorageFile> CreateNewFileInBackupFolderAsync(string fileName, CreationCollisionOption collisionOption)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync();
            return await backupFolder.CreateFileAsync(fileName, collisionOption);
        }
    }
}