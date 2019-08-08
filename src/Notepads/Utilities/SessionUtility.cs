
namespace Notepads.Utilities
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.IO;
    using System.Threading.Tasks;
    using Notepads.Core;
    using Windows.Storage;

    internal static class SessionUtility
    {
        private static readonly ConcurrentDictionary<INotepadsCore, ISessionManager> _sessionManagers = new ConcurrentDictionary<INotepadsCore, ISessionManager>();

        public static ISessionManager GetSessionManager(INotepadsCore notepadCore)
        {
            if (!_sessionManagers.TryGetValue(notepadCore, out ISessionManager sessionManager))
            {
                sessionManager = new SessionManager(notepadCore);

                if (!_sessionManagers.TryAdd(notepadCore, sessionManager))
                {
                    sessionManager = _sessionManagers[notepadCore];
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
