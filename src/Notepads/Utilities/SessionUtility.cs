namespace Notepads.Utilities
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Microsoft.Toolkit.Uwp.Helpers;
    using Notepads.Core;
    using Windows.Storage;

    internal static class SessionUtility
    {
        private const string BackupFolderDefaultName = "BackupFiles";
        private const string SessionMetaDataFileDefaultName = "NotepadsSessionData.json";

        private static readonly ConcurrentDictionary<INotepadsCore, ISessionManager> SessionManagers = new ConcurrentDictionary<INotepadsCore, ISessionManager>();

        public static ISessionManager GetSessionManager(INotepadsCore notepadCore)
        {
            return GetSessionManager(notepadCore, null);
        }

        public static ISessionManager GetSessionManager(INotepadsCore notepadCore, string filePathPrefix)
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

                sessionManager = new SessionManager(notepadCore,
                    backupFolderName,
                    sessionMetaDataFileName);

                if (!SessionManagers.TryAdd(notepadCore, sessionManager))
                {
                    sessionManager = SessionManagers[notepadCore];
                }
            }

            return sessionManager;
        }

        public static async Task<StorageFolder> GetBackupFolderAsync(string backupFolderName)
        {
            return await FileSystemUtility.GetOrCreateAppFolderAsync(backupFolderName);
        }

        public static async Task<IReadOnlyList<StorageFile>> GetAllFilesInBackupFolderAsync(string backupFolderName)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync(backupFolderName);
            return await backupFolder.GetFilesAsync();
        }

        public static async Task<bool> IsSessionMetaDataFileExists(string sessionMetaDataFileName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            return await localFolder.FileExistsAsync(sessionMetaDataFileName);
        }

        public static async Task<string> GetSerializedSessionMetaDataAsync(string sessionMetaDataFileName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            return await localFolder.ReadTextFromFileAsync(sessionMetaDataFileName);
        }

        public static async Task SaveSerializedSessionMetaDataAsync(string serializedData, string sessionMetaDataFileName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            StorageFile metaDataFile = await FileSystemUtility.GetOrCreateFileAsync(localFolder, sessionMetaDataFileName);
            await FileSystemUtility.WriteTextToFileAsync(metaDataFile, serializedData);
        }

        public static async Task DeleteSerializedSessionMetaDataAsync(string sessionMetaDataFileName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;

            StorageFile sessionDataFile = null;

            try
            {
                sessionDataFile = await localFolder.GetFileAsync(sessionMetaDataFileName);
            }
            catch (Exception)
            {
                // ignored
            }

            if (sessionDataFile != null)
            {
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