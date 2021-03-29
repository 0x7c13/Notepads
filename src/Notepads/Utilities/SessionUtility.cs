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
    using System.IO;
    using Newtonsoft.Json.Linq;
    using Notepads.Core.SessionDataModels;
    using System.Text.Json;

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

                sessionManager = new SessionManager(notepadCore, backupFolderName, sessionMetaDataFileName);

                if (!SessionManagers.TryAdd(notepadCore, sessionManager))
                {
                    sessionManager = SessionManagers[notepadCore];
                }
            }

            return sessionManager;
        }

        public static string GetBackupFolderPath(string backupFolderName)
        {
            return Path.Combine(ApplicationData.Current.LocalFolder.Path, backupFolderName);
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

        public static async Task<NotepadsSessionDataV1> GetSessionMetaDataAsync(string sessionMetaDataFileName)
        {
            try
            {
                StorageFolder localFolder = ApplicationData.Current.LocalFolder;
                if (await localFolder.FileExistsAsync(sessionMetaDataFileName))
                {
                    NotepadsSessionDataV1 sessionData = null;
                    string metadataFilePath = null;

                    var tempMetaDataFilePath = Path.Combine(localFolder.Path, sessionMetaDataFileName + "~.TMP");
                    if (File.Exists(tempMetaDataFilePath))
                    {
                        var data = await File.ReadAllTextAsync(tempMetaDataFilePath);
                        try
                        {
                            var json = JsonDocument.Parse(data);
                            var version = json.RootElement.GetProperty("Version").GetInt32();

                            if (version == 1)
                            {
                                sessionData = JsonSerializer.Deserialize<NotepadsSessionDataV1>(data);
                            }
                            else
                            {
                                throw new Exception($"Invalid version found in temporary session metadata: {version}");
                            }

                            metadataFilePath = tempMetaDataFilePath;
                        }
                        catch (Exception ex)
                        {
                            LoggingService.LogError($"[{nameof(SessionManager)}] Failed to load temporary last session metadata: {ex.Message}");
                            Analytics.TrackEvent("SessionManager_FailedToLoadLastTempSession", new Dictionary<string, string>() { { "Exception", ex.Message } });
                        }
                    }

                    if (sessionData == null)
                    {
                        var data = await localFolder.ReadTextFromFileAsync(sessionMetaDataFileName);

                        try
                        {
                            var json = JObject.Parse(data);
                            var version = (int)json["Version"];

                            if (version == 1)
                            {
                                sessionData = JsonSerializer.Deserialize<NotepadsSessionDataV1>(data);
                            }
                            else
                            {
                                throw new Exception($"Invalid version found in session metadata: {version}");
                            }

                            metadataFilePath = Path.Combine(localFolder.Path, sessionMetaDataFileName);
                        }
                        catch (Exception ex)
                        {
                            LoggingService.LogError($"[{nameof(SessionManager)}] Failed to load last session metadata: {ex.Message}");
                            Analytics.TrackEvent("SessionManager_FailedToLoadLastSession", new Dictionary<string, string>() { { "Exception", ex.Message } });
                        }
                    }

                    LoggingService.LogInfo($"[{nameof(SessionUtility)}] Session metadata Loaded from {metadataFilePath}");
                    return sessionData;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(SessionUtility)}] Failed to get session meta data: {ex.Message}");
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