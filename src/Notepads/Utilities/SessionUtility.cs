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
        private const string BackupFolderDefaultName = "BackupFiles";
        private const string SessionMetaDataFileDefaultName = "NotepadsSessionData.json";
        private static readonly ConcurrentDictionary<INotepadsCore, ISessionManager> SessionManagers = new ConcurrentDictionary<INotepadsCore, ISessionManager>();

        public static ISessionManager GetSessionManager(INotepadsCore notepadCore, string filePathPrefix = null)
        {
            if (!SessionManagers.TryGetValue(notepadCore, out ISessionManager sessionManager))
            {
                var backupFolderName = BackupFolderDefaultName;
                var sessionMetaDataFileName = SessionMetaDataFileDefaultName;

                if (filePathPrefix != null)
                {
                    backupFolderName = filePathPrefix + backupFolderName;
                    sessionMetaDataFileName = filePathPrefix + SessionMetaDataFileDefaultName;
                }

                sessionManager = new SessionManager(notepadCore, backupFolderName, sessionMetaDataFileName);

                if (!SessionManagers.TryAdd(notepadCore, sessionManager))
                {
                    sessionManager = SessionManagers[notepadCore];
                }
            }

            return sessionManager;
        }

        public static async Task<StorageFolder> GetBackupFolderAsync(string backupFolderName)
        {
            return await FileSystemUtility.GetOrCreateAppFolder(backupFolderName);
        }

        public static async Task<IReadOnlyList<StorageFile>> GetAllBackupFilesAsync(string backupFolderName)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync(backupFolderName);
            return await backupFolder.GetFilesAsync();
        }

        public static async Task<string> GetSerializedSessionMetaDataAsync(string sessionMetaDataFileName)
        {
            try
            {
                StorageFolder localFolder = ApplicationData.Current.LocalFolder;
                if (await localFolder.FileExistsAsync(sessionMetaDataFileName))
                {
                    var data = await localFolder.ReadTextFromFileAsync(sessionMetaDataFileName);
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

        public static async Task SaveSerializedSessionMetaDataAsync(string serializedData, string sessionMetaDataFileName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;

            // Attempt to delete session meta data file first in case it was not been deleted
            try
            {
                await DeleteSerializedSessionMetaDataAsync(sessionMetaDataFileName);
            }
            catch (Exception)
            {
                // ignored
            }

            await localFolder.WriteTextToFileAsync(serializedData, sessionMetaDataFileName, CreationCollisionOption.ReplaceExisting);
        }

        public static async Task DeleteSerializedSessionMetaDataAsync(string sessionMetaDataFileName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            if (await localFolder.FileExistsAsync(sessionMetaDataFileName))
            {
                var sessionDataFile = await localFolder.GetFileAsync(sessionMetaDataFileName);
                await sessionDataFile.DeleteAsync();
            }
        }

        public static async Task<StorageFile> CreateNewFileInBackupFolderAsync(string fileName, CreationCollisionOption collisionOption, string backupFolderName)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync(backupFolderName);
            return await backupFolder.CreateFileAsync(fileName, collisionOption);
        }
    }
}