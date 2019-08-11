
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

        public static async Task<IReadOnlyList<StorageFile>> GetAllBackupFilesAsync()
        {
            StorageFolder backupFolder = await GetBackupFolderAsync();
            return await backupFolder.GetFilesAsync();
        }

        public static async Task<StorageFile> CreateNewBackupFileAsync(string fileName)
        {
            StorageFolder backupFolder = await GetBackupFolderAsync();
            return await backupFolder.CreateFileAsync(fileName, CreationCollisionOption.ReplaceExisting);
        }

        private static async Task<StorageFolder> GetBackupFolderAsync()
        {
            return await FileSystemUtility.GetOrCreateAppFolder("BackupFiles");
        }
    }
}
